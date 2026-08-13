"""构建 MODEL-005 概率校准 artifact。"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import platform
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np
import sklearn
from sklearn.base import BaseEstimator, ClassifierMixin
from sklearn.calibration import CalibratedClassifierCV, calibration_curve
from sklearn.frozen import FrozenEstimator
from sklearn.metrics import brier_score_loss, log_loss
from sklearn.utils.validation import check_array

ARTIFACT_SCHEMA_VERSION = 1
RAW_MODEL_ARTIFACT_SCHEMA_VERSION = 1
CALIBRATION_METHOD = "sigmoid"
CALIBRATION_SPLIT = "calibration"
DEVELOPMENT_SPLITS = ("train", "validation", "calibration")
POSITIVE_LABEL = "team_1_win"
FINAL_TEST_STATUS = "sealed_not_evaluated"
FINAL_TEST_ACCESS_POLICY = "sealed_until_model_freeze"
CALIBRATION_BINS = 10
CALIBRATION_CURVE_STRATEGY = "quantile"
CALIBRATION_CV_FOLDS = 5


class ProbabilityCalibrationError(ValueError):
    """输入或合同不满足 MODEL-005 fail-closed 约束。"""


@dataclass(frozen=True)
class RawPrediction:
    series_id: str
    split: str
    raw_probability: float
    actual_team_1_win: int


@dataclass(frozen=True)
class LoadedRawModel:
    predictions: tuple[RawPrediction, ...]
    final_test: dict[str, Any]
    input_reference: dict[str, str]
    model_config_sha256: str


class RawProbabilityEstimator(ClassifierMixin, BaseEstimator):
    """把 MODEL-004 的 raw probability 暴露为 sklearn classifier 响应。"""

    def __init__(self) -> None:
        # 该适配器没有可训练参数；FrozenEstimator 会确保 fit 始终是 no-op。
        self.classes_ = np.asarray([0, 1], dtype=np.int64)
        self.n_features_in_ = 1

    def fit(self, X: Any, y: Any) -> RawProbabilityEstimator:
        # 直接拟合意味着 MODEL-005 绕过冻结合同，因此显式拒绝。
        raise ProbabilityCalibrationError(
            "raw probability estimator must be wrapped in FrozenEstimator"
        )

    def predict_proba(self, X: Any) -> np.ndarray:
        values = check_array(X, ensure_2d=True, dtype=np.float64)
        if values.shape[1] != 1:
            raise ProbabilityCalibrationError(
                "raw probability estimator requires exactly one feature"
            )
        probabilities = values[:, 0]
        if np.any(~np.isfinite(probabilities)) or np.any(
            (probabilities <= 0.0) | (probabilities >= 1.0)
        ):
            raise ProbabilityCalibrationError(
                "raw probabilities must be finite and strictly between zero and one"
            )
        return np.column_stack((1.0 - probabilities, probabilities))

    def predict(self, X: Any) -> np.ndarray:
        return (self.predict_proba(X)[:, 1] >= 0.5).astype(np.int64)


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _sha256_json(value: Any) -> str:
    encoded = json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _sha256_membership(series_ids: list[str]) -> str:
    return hashlib.sha256("\n".join(sorted(series_ids)).encode("utf-8")).hexdigest()


def _repository_relative_path(repository_root: Path, path: Path) -> str:
    try:
        relative = path.resolve().relative_to(repository_root.resolve())
    except ValueError as error:
        raise ProbabilityCalibrationError(
            f"path escapes repository root: {path}"
        ) from error
    return relative.as_posix()


def _load_json(path: Path, description: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise ProbabilityCalibrationError(f"invalid {description}: {path}") from error
    if not isinstance(value, dict):
        raise ProbabilityCalibrationError(f"{description} must be a JSON object")
    return value


def _require_sha256(value: Any, field: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != 64
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise ProbabilityCalibrationError(f"{field} must be a lowercase SHA-256")
    return value


def _read_final_test(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProbabilityCalibrationError("raw model is missing final_test_evaluation")
    if "series_ids" in value:
        raise ProbabilityCalibrationError(
            "sealed final test must not expose series_ids"
        )
    series_count = value.get("series_count")
    membership_sha256 = value.get("membership_sha256")
    if (
        value.get("status") != FINAL_TEST_STATUS
        or value.get("access_policy") != FINAL_TEST_ACCESS_POLICY
        or not isinstance(series_count, int)
        or isinstance(series_count, bool)
        or series_count <= 0
    ):
        raise ProbabilityCalibrationError("raw model final-test seal is invalid")
    return {
        "status": FINAL_TEST_STATUS,
        "series_count": series_count,
        "membership_sha256": _require_sha256(
            membership_sha256, "final_test.membership_sha256"
        ),
        "access_policy": FINAL_TEST_ACCESS_POLICY,
        "supported_metrics": ["brier_score", "log_loss"],
        "release_requires": [
            "model_artifact_sha256",
            "model_config_sha256",
            "calibration_artifact_sha256",
            "evaluation_code_sha256",
        ],
    }


def _read_predictions(value: Any) -> tuple[RawPrediction, ...]:
    if not isinstance(value, list) or not value:
        raise ProbabilityCalibrationError(
            "raw model development_predictions must be non-empty"
        )
    predictions: list[RawPrediction] = []
    seen: set[str] = set()
    split_counts = {split: 0 for split in DEVELOPMENT_SPLITS}
    for row in value:
        if not isinstance(row, dict):
            raise ProbabilityCalibrationError("raw prediction rows must be objects")
        series_id = row.get("series_id")
        split = row.get("split")
        raw_probability = row.get("raw_probability_team_1_win")
        actual = row.get("actual_team_1_win")
        if not isinstance(series_id, str) or not series_id or series_id in seen:
            raise ProbabilityCalibrationError(
                f"empty or duplicate raw prediction series_id: {series_id}"
            )
        if split not in DEVELOPMENT_SPLITS:
            raise ProbabilityCalibrationError(f"unexpected prediction split: {split}")
        if (
            not isinstance(raw_probability, (int, float))
            or isinstance(raw_probability, bool)
            or not math.isfinite(float(raw_probability))
            or not 0.0 < float(raw_probability) < 1.0
        ):
            raise ProbabilityCalibrationError(
                f"invalid raw probability for series: {series_id}"
            )
        if (
            not isinstance(actual, int)
            or isinstance(actual, bool)
            or actual not in (0, 1)
        ):
            raise ProbabilityCalibrationError(
                f"invalid development label for series: {series_id}"
            )
        seen.add(series_id)
        split_counts[split] += 1
        predictions.append(
            RawPrediction(series_id, split, float(raw_probability), actual)
        )
    if any(count == 0 for count in split_counts.values()):
        raise ProbabilityCalibrationError("every development split must be non-empty")
    return tuple(predictions)


def load_raw_model_artifact(
    repository_root: Path,
    model_artifact_path: Path,
    model_manifest_path: Path,
) -> LoadedRawModel:
    """读取并验证不可变 MODEL-004 artifact 及其 manifest。"""

    repository_root = repository_root.resolve()
    model_artifact_path = model_artifact_path.resolve()
    model_manifest_path = model_manifest_path.resolve()
    relative_artifact = _repository_relative_path(repository_root, model_artifact_path)
    relative_manifest = _repository_relative_path(repository_root, model_manifest_path)
    artifact = _load_json(model_artifact_path, "MODEL-004 artifact")
    manifest = _load_json(model_manifest_path, "MODEL-004 artifact manifest")

    output = manifest.get("output")
    manifest_artifact = manifest.get("artifact")
    if (
        manifest.get("artifact_manifest_version") != 1
        or not isinstance(output, dict)
        or not isinstance(manifest_artifact, dict)
        or manifest_artifact.get("kind") != "probability-model"
        or manifest_artifact.get("name") != "statistical-model"
        or output.get("relative_path") != relative_artifact
    ):
        raise ProbabilityCalibrationError("MODEL-004 manifest contract is invalid")
    artifact_sha256 = _sha256_file(model_artifact_path)
    if output.get("sha256") != artifact_sha256:
        raise ProbabilityCalibrationError(
            "MODEL-004 artifact SHA-256 does not match its manifest"
        )

    model = artifact.get("model")
    calibration = artifact.get("calibration")
    if (
        artifact.get("artifact_schema_version") != RAW_MODEL_ARTIFACT_SCHEMA_VERSION
        or not isinstance(model, dict)
        or model.get("family") != "logistic_regression"
        or model.get("training_split") != "train"
        or model.get("probability_status") != "raw_uncalibrated"
        or not isinstance(calibration, dict)
        or calibration.get("status") != "not_applied_in_model004"
    ):
        raise ProbabilityCalibrationError("MODEL-004 probability contract is invalid")
    model_config_sha256 = _require_sha256(
        model.get("config_sha256"), "model.config_sha256"
    )
    return LoadedRawModel(
        predictions=_read_predictions(artifact.get("development_predictions")),
        final_test=_read_final_test(artifact.get("final_test_evaluation")),
        input_reference={
            "artifact_name": "statistical-model",
            "artifact_version": str(manifest_artifact.get("version", "")),
            "artifact_relative_path": relative_artifact,
            "artifact_sha256": artifact_sha256,
            "manifest_relative_path": relative_manifest,
            "manifest_sha256": _sha256_file(model_manifest_path),
        },
        model_config_sha256=model_config_sha256,
    )


def _metric_summary(labels: np.ndarray, probabilities: np.ndarray) -> dict[str, float]:
    return {
        "brier_score": float(
            brier_score_loss(labels, probabilities, pos_label=1, scale_by_half=True)
        ),
        "log_loss": float(
            log_loss(
                labels,
                np.column_stack((1.0 - probabilities, probabilities)),
                labels=[0, 1],
            )
        ),
        "mean_probability_team_1_win": float(probabilities.mean()),
    }


def _curve_summary(labels: np.ndarray, probabilities: np.ndarray) -> dict[str, Any]:
    fraction_positive, mean_predicted = calibration_curve(
        labels,
        probabilities,
        pos_label=1,
        n_bins=CALIBRATION_BINS,
        strategy=CALIBRATION_CURVE_STRATEGY,
    )
    return {
        "n_bins_requested": CALIBRATION_BINS,
        "n_bins_returned": len(mean_predicted),
        "strategy": CALIBRATION_CURVE_STRATEGY,
        "points": [
            {
                "mean_predicted_probability": float(predicted),
                "fraction_positive": float(observed),
            }
            for predicted, observed in zip(
                mean_predicted, fraction_positive, strict=True
            )
        ],
    }


def build_calibration_artifact(loaded: LoadedRawModel) -> dict[str, Any]:
    """只使用 calibration split label 拟合 sigmoid 校准器。"""

    calibration_rows = [
        row for row in loaded.predictions if row.split == CALIBRATION_SPLIT
    ]
    raw_calibration = np.asarray(
        [row.raw_probability for row in calibration_rows], dtype=np.float64
    )
    labels = np.asarray(
        [row.actual_team_1_win for row in calibration_rows], dtype=np.int64
    )
    class_counts = np.bincount(labels, minlength=2)
    if len(calibration_rows) < CALIBRATION_BINS * 2:
        raise ProbabilityCalibrationError(
            "calibration split is too small for the fixed curve contract"
        )
    if np.any(class_counts < CALIBRATION_CV_FOLDS):
        raise ProbabilityCalibrationError(
            "calibration split must contain at least five rows from each class"
        )

    # FrozenEstimator 阻止 identity estimator 被拟合；所有 calibration rows 只学习 sigmoid 映射。
    calibrator = CalibratedClassifierCV(
        estimator=FrozenEstimator(RawProbabilityEstimator()),
        method=CALIBRATION_METHOD,
        cv=CALIBRATION_CV_FOLDS,
        ensemble="auto",
    ).fit(raw_calibration.reshape(-1, 1), labels)
    if len(calibrator.calibrated_classifiers_) != 1:
        raise ProbabilityCalibrationError(
            "frozen calibration must produce exactly one calibrated classifier"
        )
    calibrated_classifier = calibrator.calibrated_classifiers_[0]
    if len(calibrated_classifier.calibrators) != 1:
        raise ProbabilityCalibrationError(
            "binary sigmoid calibration must produce exactly one calibrator"
        )
    sigmoid = calibrated_classifier.calibrators[0]
    slope = float(sigmoid.a_)
    intercept = float(sigmoid.b_)
    if not math.isfinite(slope) or not math.isfinite(intercept) or slope >= 0.0:
        raise ProbabilityCalibrationError(
            "sigmoid calibration must be finite and monotonic increasing"
        )

    all_raw = np.asarray(
        [row.raw_probability for row in loaded.predictions], dtype=np.float64
    )
    all_calibrated = calibrator.predict_proba(all_raw.reshape(-1, 1))[:, 1]
    if np.any(~np.isfinite(all_calibrated)) or np.any(
        (all_calibrated <= 0.0) | (all_calibrated >= 1.0)
    ):
        raise ProbabilityCalibrationError(
            "calibrated probabilities must be finite and strictly between zero and one"
        )
    # artifact 必须能只凭公开参数重放映射，避免部署时依赖 Python estimator 序列化。
    replayed = 1.0 / (1.0 + np.exp(slope * all_raw + intercept))
    if not np.allclose(all_calibrated, replayed, rtol=0.0, atol=1e-15):
        raise ProbabilityCalibrationError(
            "serialized sigmoid parameters cannot replay calibrated probabilities"
        )
    calibration_indexes = np.asarray(
        [
            index
            for index, row in enumerate(loaded.predictions)
            if row.split == CALIBRATION_SPLIT
        ],
        dtype=np.int64,
    )
    calibrated_fit = all_calibrated[calibration_indexes]
    raw_metrics = _metric_summary(labels, raw_calibration)
    calibrated_metrics = _metric_summary(labels, calibrated_fit)

    calibration_config = {
        "library": "sklearn.calibration.CalibratedClassifierCV",
        "method": CALIBRATION_METHOD,
        "base_estimator": "sklearn.frozen.FrozenEstimator",
        "input_signal": "raw_probability_team_1_win",
        "fitting_split": CALIBRATION_SPLIT,
        "cv_response_generation_folds": CALIBRATION_CV_FOLDS,
        "curve_bins": CALIBRATION_BINS,
        "curve_strategy": CALIBRATION_CURVE_STRATEGY,
    }
    return {
        "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "probability_calibration",
        "positive_label": POSITIVE_LABEL,
        "runtime": {
            "python_version": platform.python_version(),
            "numpy_version": np.__version__,
            "scikit_learn_version": sklearn.__version__,
        },
        "input_model": {
            **loaded.input_reference,
            "model_config_sha256": loaded.model_config_sha256,
        },
        "calibration": {
            "status": "fitted_on_public_calibration_split",
            "probability_status": "sigmoid_calibrated",
            "config": calibration_config,
            "config_sha256": _sha256_json(calibration_config),
            "fitted_parameters": {
                "sklearn_sigmoid_a": slope,
                "sklearn_sigmoid_b": intercept,
                "mapping": "expit(-(a * raw_probability + b))",
            },
            "fit": {
                "series_count": len(calibration_rows),
                "team_1_win_count": int(labels.sum()),
                "series_membership_sha256": _sha256_membership(
                    [row.series_id for row in calibration_rows]
                ),
            },
        },
        "calibration_fit_diagnostics": {
            "scope": "in_sample_calibration_fit_diagnostic_not_gate_evidence",
            "raw": {
                **raw_metrics,
                "calibration_curve": _curve_summary(labels, raw_calibration),
            },
            "calibrated": {
                **calibrated_metrics,
                "calibration_curve": _curve_summary(labels, calibrated_fit),
            },
            "calibrated_minus_raw": {
                "brier_score": calibrated_metrics["brier_score"]
                - raw_metrics["brier_score"],
                "log_loss": calibrated_metrics["log_loss"] - raw_metrics["log_loss"],
            },
            "unbiased_evaluation_task": "MODEL-006",
        },
        "development_predictions": [
            {
                "series_id": row.series_id,
                "split": row.split,
                "evaluation_role": (
                    "calibration_fit_diagnostic"
                    if row.split == CALIBRATION_SPLIT
                    else "transformed_reference_not_evaluation"
                ),
                "raw_probability_team_1_win": row.raw_probability,
                "calibrated_probability_team_1_win": float(all_calibrated[index]),
            }
            for index, row in enumerate(loaded.predictions)
        ],
        "final_test_evaluation": loaded.final_test,
    }


def _parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build MODEL-005 calibration from an immutable MODEL-004 artifact."
    )
    parser.add_argument("--repository-root", required=True, type=Path)
    parser.add_argument("--model-artifact", required=True, type=Path)
    parser.add_argument("--model-manifest", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def main() -> None:
    arguments = _parse_arguments()
    loaded = load_raw_model_artifact(
        arguments.repository_root,
        arguments.model_artifact,
        arguments.model_manifest,
    )
    artifact = build_calibration_artifact(loaded)
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(
        json.dumps(artifact, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )


if __name__ == "__main__":
    try:
        main()
    except (ProbabilityCalibrationError, OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"probability calibration build failed: {error}") from error
