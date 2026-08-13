"""构建 MODEL-006 严格时间前推的模型比较 artifact。"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import platform
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

import numpy as np
import sklearn
from sklearn.calibration import CalibratedClassifierCV, calibration_curve
from sklearn.frozen import FrozenEstimator
from sklearn.metrics import brier_score_loss, log_loss

from research.model002_elo_baseline import (
    INITIAL_RATING,
    K_FACTOR,
    RATING_SCALE,
    EloSeries,
    run_chronological_elo,
)
from research.model004_statistical_model import (
    FEATURE_NAMES,
    RANDOM_SEED,
    StatisticalSeries,
    fit_statistical_model,
    load_statistical_data,
    predict_probability,
)
from research.model005_probability_calibration import (
    CALIBRATION_CV_FOLDS,
    CALIBRATION_METHOD,
    RawProbabilityEstimator,
)

ARTIFACT_SCHEMA_VERSION = 1
POSITIVE_LABEL = "team_1_win"
FINAL_TEST_STATUS = "sealed_not_evaluated"
FINAL_TEST_ACCESS_POLICY = "sealed_until_model_freeze"
DEVELOPMENT_SPLITS = ("train", "validation", "calibration")
MODEL_NAMES = (
    "constant_baseline",
    "elo_baseline",
    "raw_statistical",
    "calibrated_statistical",
)
CURVE_MODEL_NAMES = ("raw_statistical", "calibrated_statistical")
CALIBRATION_CURVE_BINS = 10
CALIBRATION_CURVE_STRATEGY = "quantile"
MIN_CALIBRATION_CLASS_COUNT = CALIBRATION_CV_FOLDS
SMALL_SEGMENT_THRESHOLD = 30
VALIDATION_FIRST_BLOCK_DAYS = 21
CALIBRATION_FIRST_BLOCK_DAYS = 21


class WalkForwardError(ValueError):
    """输入或合同不满足 MODEL-006 fail-closed 约束。"""


@dataclass(frozen=True)
class PublicSplitWindows:
    train_start: datetime
    validation_start: datetime
    validation_end: datetime
    calibration_start: datetime
    calibration_end: datetime
    final_test_start: datetime


@dataclass(frozen=True)
class WalkForwardWindow:
    name: str
    train_start: datetime
    train_end: datetime
    calibration_start: datetime
    calibration_end: datetime
    evaluation_start: datetime
    evaluation_end: datetime


@dataclass(frozen=True)
class WalkForwardSeries:
    series_id: str
    scheduled_start_utc: datetime
    region: str
    best_of: int
    team_ids: tuple[str, str]
    feature_values: tuple[float, ...]
    actual_team_1_win: int


@dataclass(frozen=True)
class LoadedWalkForwardData:
    series: tuple[WalkForwardSeries, ...]
    split_windows: PublicSplitWindows
    final_test: dict[str, Any]


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
        raise WalkForwardError(f"path escapes repository root: {path}") from error
    return relative.as_posix()


def _validated_input_reference(
    repository_root: Path,
    dataset_path: Path,
    manifest_path: Path,
    expected_dataset_name: str,
) -> dict[str, str]:
    if not dataset_path.is_file() or not manifest_path.is_file():
        raise WalkForwardError(
            f"missing dataset or manifest: dataset={dataset_path}, manifest={manifest_path}"
        )
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise WalkForwardError(f"invalid dataset manifest: {manifest_path}") from error
    dataset = manifest.get("dataset")
    output = manifest.get("output")
    if not isinstance(dataset, dict) or not isinstance(output, dict):
        raise WalkForwardError("dataset manifest is missing dataset/output metadata")
    if dataset.get("name") != expected_dataset_name:
        raise WalkForwardError(
            f"unexpected dataset name: expected={expected_dataset_name}, "
            f"actual={dataset.get('name')}"
        )
    relative_path = _repository_relative_path(repository_root, dataset_path)
    dataset_sha256 = _sha256_file(dataset_path)
    if output.get("relative_path") != relative_path:
        raise WalkForwardError("dataset path does not match its manifest output")
    if output.get("sha256") != dataset_sha256:
        raise WalkForwardError("dataset SHA-256 does not match its manifest output")
    return {
        "dataset_name": expected_dataset_name,
        "dataset_version": str(dataset.get("version", "")),
        "dataset_relative_path": relative_path,
        "dataset_sha256": dataset_sha256,
        "manifest_relative_path": _repository_relative_path(
            repository_root, manifest_path
        ),
        "manifest_sha256": _sha256_file(manifest_path),
    }


def _parse_utc(value: Any, field: str) -> datetime:
    if not isinstance(value, str) or not value:
        raise WalkForwardError(f"missing {field}")
    try:
        parsed = datetime.fromisoformat(value)
    except ValueError as error:
        raise WalkForwardError(f"invalid {field}: {value}") from error
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        raise WalkForwardError(f"{field} must be timezone-aware")
    return parsed.astimezone(UTC)


def _iso(value: datetime) -> str:
    return value.astimezone(UTC).isoformat().replace("+00:00", "Z")


def _read_split_windows(temporal_split_path: Path) -> PublicSplitWindows:
    try:
        split = json.loads(temporal_split_path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise WalkForwardError("invalid temporal split manifest") from error
    if split.get("manifest_version") != 1:
        raise WalkForwardError("unsupported temporal split manifest version")
    parsed: dict[str, tuple[datetime, datetime]] = {}
    for split_name in (*DEVELOPMENT_SPLITS, "final_test"):
        value = split.get(split_name)
        if not isinstance(value, dict):
            raise WalkForwardError(f"missing split contract: {split_name}")
        if split_name == "final_test" and "series_ids" in value:
            raise WalkForwardError("sealed final test must not expose series_ids")
        window = value.get("window")
        if not isinstance(window, dict):
            raise WalkForwardError(f"missing split window: {split_name}")
        start = _parse_utc(window.get("start_utc"), f"{split_name}.start_utc")
        end = _parse_utc(window.get("end_utc"), f"{split_name}.end_utc")
        if start >= end:
            raise WalkForwardError(f"invalid split window: {split_name}")
        parsed[split_name] = (start, end)
    if not (
        parsed["train"][1]
        == parsed["validation"][0]
        < parsed["validation"][1]
        == parsed["calibration"][0]
        < parsed["calibration"][1]
        == parsed["final_test"][0]
    ):
        raise WalkForwardError(
            "train/validation/calibration/final-test windows must be continuous"
        )
    return PublicSplitWindows(
        train_start=parsed["train"][0],
        validation_start=parsed["validation"][0],
        validation_end=parsed["validation"][1],
        calibration_start=parsed["calibration"][0],
        calibration_end=parsed["calibration"][1],
        final_test_start=parsed["final_test"][0],
    )


def build_walk_forward_windows(
    split: PublicSplitWindows,
) -> tuple[WalkForwardWindow, ...]:
    """从公开 Development 边界构造三个预注册 expanding folds。"""

    validation_mid = split.validation_start + timedelta(
        days=VALIDATION_FIRST_BLOCK_DAYS
    )
    calibration_mid = split.calibration_start + timedelta(
        days=CALIBRATION_FIRST_BLOCK_DAYS
    )
    if not split.validation_start < validation_mid < split.validation_end:
        raise WalkForwardError("validation window is too short for walk-forward")
    if not split.calibration_start < calibration_mid < split.calibration_end:
        raise WalkForwardError("calibration window is too short for walk-forward")
    windows = (
        WalkForwardWindow(
            "fold_1",
            split.train_start,
            split.validation_start,
            split.validation_start,
            validation_mid,
            validation_mid,
            split.validation_end,
        ),
        WalkForwardWindow(
            "fold_2",
            split.train_start,
            validation_mid,
            validation_mid,
            split.validation_end,
            split.calibration_start,
            calibration_mid,
        ),
        WalkForwardWindow(
            "fold_3",
            split.train_start,
            split.calibration_start,
            split.calibration_start,
            calibration_mid,
            calibration_mid,
            split.final_test_start,
        ),
    )
    previous_evaluation_end: datetime | None = None
    for window in windows:
        if not (
            window.train_start < window.train_end
            and window.train_end == window.calibration_start
            and window.calibration_start < window.calibration_end
            and window.calibration_end == window.evaluation_start
            and window.evaluation_start < window.evaluation_end
            and window.evaluation_end <= split.final_test_start
        ):
            raise WalkForwardError(f"invalid walk-forward ordering: {window.name}")
        if (
            previous_evaluation_end is not None
            and previous_evaluation_end != window.evaluation_start
        ):
            raise WalkForwardError("walk-forward evaluation windows must be continuous")
        previous_evaluation_end = window.evaluation_end
    return windows


def load_walk_forward_data(
    series_result_path: Path,
    feature_snapshot_path: Path,
    temporal_split_path: Path,
) -> LoadedWalkForwardData:
    loaded = load_statistical_data(
        series_result_path, feature_snapshot_path, temporal_split_path
    )
    public_ids = {row.series_id for row in loaded.series}
    metadata: dict[str, tuple[str, int]] = {}
    try:
        with series_result_path.open("r", encoding="utf-8-sig", newline="") as source:
            reader = csv.DictReader(source)
            required = {"series_id", "region", "best_of"}
            if reader.fieldnames is None or not required.issubset(reader.fieldnames):
                raise WalkForwardError(
                    "Series Result CSV is missing MODEL-006 segment fields"
                )
            for raw in reader:
                series_id = (raw.get("series_id") or "").strip()
                if series_id not in public_ids:
                    # 不访问非公开成员的 region、BO 或 label。
                    continue
                if series_id in metadata:
                    raise WalkForwardError(
                        f"duplicate public Series Result metadata: {series_id}"
                    )
                region = (raw.get("region") or "").strip()
                try:
                    best_of = int((raw.get("best_of") or "").strip())
                except ValueError as error:
                    raise WalkForwardError(
                        f"invalid best_of for public series: {series_id}"
                    ) from error
                if not region or best_of not in (3, 5):
                    raise WalkForwardError(
                        f"invalid segment metadata for public series: {series_id}"
                    )
                metadata[series_id] = (region, best_of)
    except OSError as error:
        raise WalkForwardError("failed to read Series Result metadata") from error
    if set(metadata) != public_ids:
        missing = min(public_ids.difference(metadata))
        raise WalkForwardError(f"missing public segment metadata: {missing}")

    series = tuple(
        sorted(
            (
                WalkForwardSeries(
                    series_id=row.series_id,
                    scheduled_start_utc=row.scheduled_start_utc,
                    region=metadata[row.series_id][0],
                    best_of=metadata[row.series_id][1],
                    team_ids=row.team_ids,
                    feature_values=row.feature_values,
                    actual_team_1_win=row.actual_team_1_win,
                )
                for row in loaded.series
            ),
            key=lambda row: (row.scheduled_start_utc, row.series_id),
        )
    )
    split_windows = _read_split_windows(temporal_split_path)
    if any(
        row.scheduled_start_utc < split_windows.train_start
        or row.scheduled_start_utc >= split_windows.final_test_start
        for row in series
    ):
        raise WalkForwardError("public development series escapes declared windows")
    return LoadedWalkForwardData(
        series=series,
        split_windows=split_windows,
        final_test={
            "status": FINAL_TEST_STATUS,
            **loaded.final_test,
            "supported_metrics": ["brier_score", "log_loss"],
            "release_requires": [
                "model_artifact_sha256",
                "model_config_sha256",
                "calibration_artifact_sha256",
                "walk_forward_artifact_sha256",
                "evaluation_code_sha256",
            ],
        },
    )


def _select(
    series: tuple[WalkForwardSeries, ...], start: datetime, end: datetime
) -> list[WalkForwardSeries]:
    return [row for row in series if start <= row.scheduled_start_utc < end]


def _to_statistical(
    rows: list[WalkForwardSeries], split_name: str
) -> list[StatisticalSeries]:
    return [
        StatisticalSeries(
            series_id=row.series_id,
            split=split_name,
            scheduled_start_utc=row.scheduled_start_utc,
            team_ids=row.team_ids,
            feature_values=row.feature_values,
            actual_team_1_win=row.actual_team_1_win,
        )
        for row in rows
    ]


def _fit_sigmoid(
    raw_probabilities: np.ndarray, labels: np.ndarray
) -> tuple[CalibratedClassifierCV, float, float]:
    class_counts = np.bincount(labels, minlength=2)
    if np.any(class_counts < MIN_CALIBRATION_CLASS_COUNT):
        raise WalkForwardError(
            "each calibration window must contain at least five rows from each class"
        )
    calibrator = CalibratedClassifierCV(
        estimator=FrozenEstimator(RawProbabilityEstimator()),
        method=CALIBRATION_METHOD,
        cv=CALIBRATION_CV_FOLDS,
        ensemble="auto",
    ).fit(raw_probabilities.reshape(-1, 1), labels)
    if len(calibrator.calibrated_classifiers_) != 1:
        raise WalkForwardError("frozen calibration must produce one classifier")
    sigmoid_calibrators = calibrator.calibrated_classifiers_[0].calibrators
    if len(sigmoid_calibrators) != 1:
        raise WalkForwardError("binary sigmoid calibration must produce one mapping")
    sigmoid = sigmoid_calibrators[0]
    slope = float(sigmoid.a_)
    intercept = float(sigmoid.b_)
    if not math.isfinite(slope) or not math.isfinite(intercept) or slope >= 0.0:
        raise WalkForwardError(
            "sigmoid mapping must be finite and monotonic increasing"
        )
    return calibrator, slope, intercept


def _metric_summary(labels: np.ndarray, probabilities: np.ndarray) -> dict[str, Any]:
    if len(labels) == 0 or len(labels) != len(probabilities):
        raise WalkForwardError("metric inputs must be non-empty and aligned")
    if np.any(~np.isfinite(probabilities)) or np.any(
        (probabilities <= 0.0) | (probabilities >= 1.0)
    ):
        raise WalkForwardError("evaluation probabilities must be finite and open")
    return {
        "series_count": len(labels),
        "team_1_win_count": int(labels.sum()),
        "observed_team_1_win_rate": float(labels.mean()),
        "mean_probability_team_1_win": float(probabilities.mean()),
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
    }


def _evaluate_rows(rows: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    labels = np.asarray([row["actual_team_1_win"] for row in rows], dtype=np.int64)
    return {
        model_name: _metric_summary(
            labels,
            np.asarray([row[model_name] for row in rows], dtype=np.float64),
        )
        for model_name in MODEL_NAMES
    }


def _calibration_curve(rows: list[dict[str, Any]], model_name: str) -> dict[str, Any]:
    labels = np.asarray([row["actual_team_1_win"] for row in rows], dtype=np.int64)
    probabilities = np.asarray([row[model_name] for row in rows], dtype=np.float64)
    observed, predicted = calibration_curve(
        labels,
        probabilities,
        pos_label=1,
        n_bins=CALIBRATION_CURVE_BINS,
        strategy=CALIBRATION_CURVE_STRATEGY,
    )
    return {
        "n_bins_requested": CALIBRATION_CURVE_BINS,
        "n_bins_returned": len(predicted),
        "strategy": CALIBRATION_CURVE_STRATEGY,
        "points": [
            {
                "mean_predicted_probability": float(mean_probability),
                "fraction_positive": float(fraction_positive),
            }
            for mean_probability, fraction_positive in zip(
                predicted, observed, strict=True
            )
        ],
    }


def _segment_summary(
    rows: list[dict[str, Any]], field: str
) -> dict[str, dict[str, Any]]:
    values = sorted({str(row[field]) for row in rows})
    result: dict[str, dict[str, Any]] = {}
    for value in values:
        selected = [row for row in rows if str(row[field]) == value]
        metrics = _evaluate_rows(selected)
        result[value] = {
            "small_sample_warning": len(selected) < SMALL_SEGMENT_THRESHOLD,
            "models": metrics,
            "deltas_vs_elo": _model_deltas(metrics),
            "calibrated_minus_raw": _calibrated_minus_raw(metrics),
        }
    return result


def _model_deltas(metrics: dict[str, dict[str, Any]]) -> dict[str, dict[str, float]]:
    elo = metrics["elo_baseline"]
    return {
        model_name: {
            "brier_score_minus_elo": metrics[model_name]["brier_score"]
            - elo["brier_score"],
            "log_loss_minus_elo": metrics[model_name]["log_loss"] - elo["log_loss"],
        }
        for model_name in ("raw_statistical", "calibrated_statistical")
    }


def _calibrated_minus_raw(
    metrics: dict[str, dict[str, Any]],
) -> dict[str, float]:
    calibrated = metrics["calibrated_statistical"]
    raw = metrics["raw_statistical"]
    return {
        "brier_score": calibrated["brier_score"] - raw["brier_score"],
        "log_loss": calibrated["log_loss"] - raw["log_loss"],
    }


def _run_fold(
    all_series: tuple[WalkForwardSeries, ...], window: WalkForwardWindow
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    train = _select(all_series, window.train_start, window.train_end)
    calibration = _select(all_series, window.calibration_start, window.calibration_end)
    evaluation = _select(all_series, window.evaluation_start, window.evaluation_end)
    if not train or not calibration or not evaluation:
        raise WalkForwardError(f"walk-forward fold is empty: {window.name}")
    for role, rows in (("train", train), ("calibration", calibration)):
        if {row.actual_team_1_win for row in rows} != {0, 1}:
            raise WalkForwardError(f"{window.name} {role} must contain both classes")

    statistical_train = _to_statistical(train, "train")
    fitted = fit_statistical_model(statistical_train)
    raw_calibration = predict_probability(
        fitted, _to_statistical(calibration, "calibration")
    )
    calibration_labels = np.asarray(
        [row.actual_team_1_win for row in calibration], dtype=np.int64
    )
    calibrator, sigmoid_a, sigmoid_b = _fit_sigmoid(raw_calibration, calibration_labels)
    raw_evaluation = predict_probability(
        fitted, _to_statistical(evaluation, "evaluation")
    )
    calibrated_evaluation = calibrator.predict_proba(raw_evaluation.reshape(-1, 1))[
        :, 1
    ]
    replayed = 1.0 / (1.0 + np.exp(sigmoid_a * raw_evaluation + sigmoid_b))
    if not np.allclose(calibrated_evaluation, replayed, rtol=0.0, atol=1e-15):
        raise WalkForwardError("serialized sigmoid cannot replay fold predictions")

    constant_probability = float(
        np.mean([row.actual_team_1_win for row in train], dtype=np.float64)
    )
    if not 0.0 < constant_probability < 1.0:
        raise WalkForwardError("fold constant probability must be open")

    # Elo 按现有 MODEL-002 合同逐场先预测后更新；评估行只能使用其开赛前 rating。
    elo_source = _select(all_series, window.train_start, window.evaluation_end)
    elo_series = [
        EloSeries(
            series_id=row.series_id,
            split="walk_forward",
            scheduled_start_utc=row.scheduled_start_utc,
            region=row.region,
            team_ids=row.team_ids,
            actual_team_1_win=row.actual_team_1_win,
        )
        for row in elo_source
    ]
    elo_predictions, _ = run_chronological_elo(elo_series)
    elo_by_id = {
        row["series_id"]: float(row["probability_team_1_win"])
        for row in elo_predictions
    }
    if any(row.series_id not in elo_by_id for row in evaluation):
        raise WalkForwardError("missing Elo evaluation prediction")

    prediction_rows = [
        {
            "fold": window.name,
            "series_id": row.series_id,
            "scheduled_start_utc": _iso(row.scheduled_start_utc),
            "region": row.region,
            "best_of": row.best_of,
            "actual_team_1_win": row.actual_team_1_win,
            "constant_baseline": constant_probability,
            "elo_baseline": elo_by_id[row.series_id],
            "raw_statistical": float(raw_evaluation[index]),
            "calibrated_statistical": float(calibrated_evaluation[index]),
        }
        for index, row in enumerate(evaluation)
    ]
    metrics = _evaluate_rows(prediction_rows)
    fold = {
        "name": window.name,
        "windows": {
            "train": {
                "start_utc": _iso(window.train_start),
                "end_utc": _iso(window.train_end),
                "series_count": len(train),
                "membership_sha256": _sha256_membership(
                    [row.series_id for row in train]
                ),
            },
            "calibration": {
                "start_utc": _iso(window.calibration_start),
                "end_utc": _iso(window.calibration_end),
                "series_count": len(calibration),
                "membership_sha256": _sha256_membership(
                    [row.series_id for row in calibration]
                ),
            },
            "evaluation": {
                "start_utc": _iso(window.evaluation_start),
                "end_utc": _iso(window.evaluation_end),
                "series_count": len(evaluation),
                "membership_sha256": _sha256_membership(
                    [row.series_id for row in evaluation]
                ),
            },
        },
        "fitted_parameters": {
            "constant_probability_team_1_win": constant_probability,
            "statistical_optimizer_iterations": fitted.iterations,
            "statistical_raw_space_intercept": fitted.raw_intercept,
            "statistical_raw_space_coefficients": dict(
                zip(FEATURE_NAMES, fitted.raw_coefficients, strict=True)
            ),
            "sigmoid_a": sigmoid_a,
            "sigmoid_b": sigmoid_b,
            "sigmoid_mapping": "expit(-(a * raw_probability + b))",
        },
        "models": metrics,
        "deltas_vs_elo": _model_deltas(metrics),
        "calibrated_minus_raw": _calibrated_minus_raw(metrics),
    }
    return fold, prediction_rows


def build_walk_forward_artifact(
    loaded: LoadedWalkForwardData,
    inputs: dict[str, dict[str, str]],
) -> dict[str, Any]:
    windows = build_walk_forward_windows(loaded.split_windows)
    folds: list[dict[str, Any]] = []
    predictions: list[dict[str, Any]] = []
    evaluation_ids: set[str] = set()
    for window in windows:
        fold, fold_predictions = _run_fold(loaded.series, window)
        fold_ids = {row["series_id"] for row in fold_predictions}
        overlap = evaluation_ids.intersection(fold_ids)
        if overlap:
            raise WalkForwardError(
                f"series appears in multiple evaluation folds: {min(overlap)}"
            )
        evaluation_ids.update(fold_ids)
        folds.append(fold)
        predictions.extend(fold_predictions)
    predictions.sort(key=lambda row: (row["scheduled_start_utc"], row["series_id"]))
    overall_models = _evaluate_rows(predictions)

    config = {
        "strategy": "expanding_train_disjoint_calibration_and_evaluation",
        "fold_count": len(windows),
        "validation_first_block_days": VALIDATION_FIRST_BLOCK_DAYS,
        "calibration_first_block_days": CALIBRATION_FIRST_BLOCK_DAYS,
        "constant_training": "expanding_train_label_prior",
        "elo": {
            "initial_rating": INITIAL_RATING,
            "rating_scale": RATING_SCALE,
            "k_factor": K_FACTOR,
            "update": "predict_then_update_chronologically",
        },
        "statistical": {
            "random_seed": RANDOM_SEED,
            "feature_names": list(FEATURE_NAMES),
            "fit": "expanding_train_only",
        },
        "calibration": {
            "method": CALIBRATION_METHOD,
            "fit": "immediately_preceding_disjoint_calibration_window",
            "cv_response_generation_folds": CALIBRATION_CV_FOLDS,
        },
        "segments": ["time_fold", "region", "best_of"],
        "small_segment_threshold": SMALL_SEGMENT_THRESHOLD,
    }
    return {
        "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "walk_forward_evaluation",
        "positive_label": POSITIVE_LABEL,
        "runtime": {
            "python_version": platform.python_version(),
            "numpy_version": np.__version__,
            "scikit_learn_version": sklearn.__version__,
        },
        "inputs": inputs,
        "evaluation": {
            "status": "public_development_walk_forward_complete",
            "config": config,
            "config_sha256": _sha256_json(config),
            "development_series_count": len(loaded.series),
            "evaluated_series_count": len(predictions),
            "evaluation_membership_sha256": _sha256_membership(
                [row["series_id"] for row in predictions]
            ),
            "unevaluated_public_series_count": len(loaded.series) - len(predictions),
            "folds": folds,
            "overall": {
                "models": overall_models,
                "deltas_vs_elo": _model_deltas(overall_models),
                "calibrated_minus_raw": _calibrated_minus_raw(overall_models),
                "calibration_curves": {
                    model_name: _calibration_curve(predictions, model_name)
                    for model_name in CURVE_MODEL_NAMES
                },
            },
            "by_region": _segment_summary(predictions, "region"),
            "by_best_of": _segment_summary(predictions, "best_of"),
            "market_baseline": {
                "status": "excluded_from_cross_model_metrics",
                "reason": "different linked-only population and time range",
            },
            "gate_decision": {
                "status": "not_made_in_model006",
                "next_task": "MODEL-007",
            },
        },
        "predictions": predictions,
        "final_test_evaluation": loaded.final_test,
    }


def _parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build MODEL-006 public-development walk-forward evaluation."
    )
    parser.add_argument("--repository-root", required=True, type=Path)
    parser.add_argument("--series-results", required=True, type=Path)
    parser.add_argument("--series-manifest", required=True, type=Path)
    parser.add_argument("--feature-snapshots", required=True, type=Path)
    parser.add_argument("--feature-manifest", required=True, type=Path)
    parser.add_argument("--temporal-split", required=True, type=Path)
    parser.add_argument("--temporal-split-manifest", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def main() -> None:
    arguments = _parse_arguments()
    repository_root = arguments.repository_root.resolve()
    inputs = {
        "series_result": _validated_input_reference(
            repository_root,
            arguments.series_results.resolve(),
            arguments.series_manifest.resolve(),
            "lol-series-results",
        ),
        "feature_snapshot": _validated_input_reference(
            repository_root,
            arguments.feature_snapshots.resolve(),
            arguments.feature_manifest.resolve(),
            "lol-prematch-features",
        ),
        "temporal_split": _validated_input_reference(
            repository_root,
            arguments.temporal_split.resolve(),
            arguments.temporal_split_manifest.resolve(),
            "lol-temporal-splits",
        ),
    }
    loaded = load_walk_forward_data(
        arguments.series_results.resolve(),
        arguments.feature_snapshots.resolve(),
        arguments.temporal_split.resolve(),
    )
    artifact = build_walk_forward_artifact(loaded, inputs)
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(
        json.dumps(artifact, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )


if __name__ == "__main__":
    try:
        main()
    except (WalkForwardError, OSError, json.JSONDecodeError, ValueError) as error:
        raise SystemExit(f"walk-forward build failed: {error}") from error
