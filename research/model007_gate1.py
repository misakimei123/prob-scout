"""执行 MODEL-007 唯一一次 Final Test 主评估并裁决 Gate 1。"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import platform
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

import numpy as np
import sklearn
from sklearn.calibration import calibration_curve
from sklearn.metrics import brier_score_loss, log_loss

from research.model002_elo_baseline import expected_team_one_win
from research.model004_statistical_model import (
    FEATURE_NAMES,
    StatisticalSeries,
    _read_development_features,
    _read_development_labels,
)

ARTIFACT_SCHEMA_VERSION = 1
POSITIVE_LABEL = "team_1_win"
MODEL_NAMES = ("constant_baseline", "elo_baseline", "raw_statistical")
CURVE_BINS = 10


class Gate1Error(ValueError):
    """MODEL-007 输入或冻结合同不成立。"""


@dataclass(frozen=True)
class FinalMetadata:
    region: str
    best_of: int


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


def _sha256_temporal_membership(series: list[StatisticalSeries]) -> str:
    digest = hashlib.sha256()
    for row in sorted(
        series, key=lambda item: (item.scheduled_start_utc, item.series_id)
    ):
        digest.update(row.scheduled_start_utc.isoformat().encode("utf-8"))
        digest.update(b"\t")
        digest.update(row.series_id.encode("utf-8"))
        digest.update(b"\n")
    return digest.hexdigest()


def _load_json(path: Path, description: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise Gate1Error(f"invalid {description}: {path}") from error
    if not isinstance(value, dict):
        raise Gate1Error(f"{description} root must be an object")
    return value


def _require_sha256(value: Any, field: str) -> str:
    if (
        not isinstance(value, str)
        or len(value) != 64
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise Gate1Error(f"invalid SHA-256: {field}")
    return value


def _same_utc_instant(left: Any, right: Any) -> bool:
    if not isinstance(left, str) or not isinstance(right, str):
        return False
    try:
        return datetime.fromisoformat(left) == datetime.fromisoformat(right)
    except ValueError:
        return False


def _validate_artifact(
    artifact_path: Path,
    manifest_path: Path,
    expected_kind: str,
) -> tuple[dict[str, Any], str]:
    artifact = _load_json(artifact_path, expected_kind)
    manifest = _load_json(manifest_path, f"{expected_kind} manifest")
    artifact_hash = _sha256_file(artifact_path)
    output = manifest.get("output")
    if not isinstance(output, dict) or output.get("sha256") != artifact_hash:
        raise Gate1Error(f"{expected_kind} hash differs from manifest")
    if artifact.get("artifact_kind") != expected_kind:
        raise Gate1Error(f"unexpected artifact kind: {expected_kind}")
    return artifact, artifact_hash


def _validate_release(
    sealed: dict[str, Any], released: dict[str, Any], freeze: dict[str, Any]
) -> list[str]:
    final_test = sealed.get("final_test")
    authorization = released.get("authorization")
    if not isinstance(final_test, dict) or not isinstance(authorization, dict):
        raise Gate1Error("missing sealed final test or release authorization")
    frozen_authorization = freeze.get("release_authorization")
    if not isinstance(frozen_authorization, dict):
        raise Gate1Error("freeze manifest lacks release authorization")
    # Chrono 会规范化 RFC3339 的小数秒文本；时间按同一 UTC 时刻比较，三个授权 hash 必须逐字一致。
    if not _same_utc_instant(
        authorization.get("frozen_at_utc"),
        frozen_authorization.get("frozen_at_utc"),
    ) or any(
        authorization.get(field) != frozen_authorization.get(field)
        for field in (
            "model_artifact_sha256",
            "model_config_sha256",
            "evaluation_code_sha256",
        )
    ):
        raise Gate1Error("released authorization differs from frozen authorization")
    ids = released.get("series_ids")
    if (
        not isinstance(ids, list)
        or not ids
        or not all(isinstance(value, str) and value for value in ids)
        or len(ids) != len(set(ids))
    ):
        raise Gate1Error("released final-test IDs are invalid")
    if len(ids) != final_test.get("series_count"):
        raise Gate1Error("released final-test count differs from seal")
    if released.get("membership_sha256") != final_test.get("membership_sha256"):
        raise Gate1Error("Rust release commitment differs from seal")
    if released.get("source_dataset_sha256") != sealed.get("source_dataset_sha256"):
        raise Gate1Error("released source dataset differs from seal")
    return ids


def _read_final_series(
    series_result_path: Path,
    feature_snapshot_path: Path,
    sealed: dict[str, Any],
    final_ids: list[str],
) -> tuple[list[StatisticalSeries], dict[str, FinalMetadata]]:
    split_by_id: dict[str, str] = {}
    for split_name in ("train", "validation", "calibration"):
        split = sealed.get(split_name)
        if not isinstance(split, dict) or not isinstance(split.get("series_ids"), list):
            raise Gate1Error(f"missing public split: {split_name}")
        for series_id in split["series_ids"]:
            if series_id in split_by_id:
                raise Gate1Error(f"duplicate split member: {series_id}")
            split_by_id[series_id] = split_name
    for series_id in final_ids:
        if series_id in split_by_id:
            raise Gate1Error(f"final member overlaps development: {series_id}")
        split_by_id[series_id] = "final_test"

    expected_total = len(split_by_id)
    labels, all_ids = _read_development_labels(
        series_result_path, split_by_id, expected_total
    )
    all_series = _read_development_features(
        feature_snapshot_path, labels, all_ids, expected_total
    )
    final = [row for row in all_series if row.split == "final_test"]
    if len(final) != len(final_ids):
        raise Gate1Error("final feature rows differ from released membership")

    metadata: dict[str, FinalMetadata] = {}
    with series_result_path.open("r", encoding="utf-8-sig", newline="") as source:
        reader = csv.DictReader(source)
        for row in reader:
            series_id = (row.get("series_id") or "").strip()
            if series_id not in set(final_ids):
                continue
            try:
                best_of = int((row.get("best_of") or "").strip())
            except ValueError as error:
                raise Gate1Error(f"invalid final best_of: {series_id}") from error
            region = (row.get("region") or "").strip()
            if not region or best_of not in (3, 5):
                raise Gate1Error(f"invalid final segment metadata: {series_id}")
            metadata[series_id] = FinalMetadata(region=region, best_of=best_of)
    if set(metadata) != set(final_ids):
        raise Gate1Error("missing final segment metadata")
    return final, metadata


def _raw_probabilities(
    model_artifact: dict[str, Any], final: list[StatisticalSeries]
) -> np.ndarray:
    fitted = model_artifact.get("fitted_parameters")
    if not isinstance(fitted, dict):
        raise Gate1Error("raw model lacks frozen fitted parameters")
    features = fitted.get("features")
    if not isinstance(features, list) or len(features) != len(FEATURE_NAMES):
        raise Gate1Error("raw model feature parameters are invalid")
    names = [row.get("feature") for row in features if isinstance(row, dict)]
    if names != list(FEATURE_NAMES):
        raise Gate1Error("raw model feature order differs from evaluation code")
    coefficients = np.asarray(
        [float(row["raw_space_coefficient"]) for row in features], dtype=np.float64
    )
    intercept = float(fitted.get("raw_space_intercept"))
    matrix = np.asarray([row.feature_values for row in final], dtype=np.float64)
    logits = intercept + matrix @ coefficients
    probabilities = 1.0 / (1.0 + np.exp(-logits))
    if np.any(~np.isfinite(probabilities)) or np.any(
        (probabilities <= 0.0) | (probabilities >= 1.0)
    ):
        raise Gate1Error("raw final probabilities are invalid")
    return probabilities


def _elo_probabilities(
    elo_artifact: dict[str, Any], final: list[StatisticalSeries]
) -> np.ndarray:
    model = elo_artifact.get("model")
    ratings_value = elo_artifact.get("terminal_ratings_after_calibration")
    if not isinstance(model, dict) or not isinstance(ratings_value, dict):
        raise Gate1Error("Elo artifact lacks frozen state")
    config = model.get("configuration")
    if not isinstance(config, dict):
        raise Gate1Error("Elo artifact lacks frozen configuration")
    initial = float(config.get("initial_rating"))
    scale = float(config.get("rating_scale"))
    k_factor = float(config.get("k_factor"))
    ratings = {str(key): float(value) for key, value in ratings_value.items()}
    probabilities: list[float] = []
    last_start: datetime | None = None
    teams_at_last_start: set[str] = set()
    for row in final:
        if row.scheduled_start_utc != last_start:
            last_start = row.scheduled_start_utc
            teams_at_last_start = set()
        overlap = teams_at_last_start.intersection(row.team_ids)
        if overlap:
            raise Gate1Error(
                "team appears in multiple final series at the same scheduled start: "
                f"{min(overlap)}"
            )
        teams_at_last_start.update(row.team_ids)
        team_one, team_two = row.team_ids
        rating_one = ratings.get(team_one, initial)
        rating_two = ratings.get(team_two, initial)
        probability = expected_team_one_win(rating_one, rating_two, scale)
        probabilities.append(probability)
        # Final Test 内仍严格先预测后更新，当前 label 不得回流到自身预测。
        delta = k_factor * (row.actual_team_1_win - probability)
        ratings[team_one] = rating_one + delta
        ratings[team_two] = rating_two - delta
    return np.asarray(probabilities, dtype=np.float64)


def _metric_summary(labels: np.ndarray, probabilities: np.ndarray) -> dict[str, Any]:
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


def _deltas(metrics: dict[str, dict[str, Any]]) -> dict[str, float]:
    return {
        "brier_score_minus_elo": metrics["raw_statistical"]["brier_score"]
        - metrics["elo_baseline"]["brier_score"],
        "log_loss_minus_elo": metrics["raw_statistical"]["log_loss"]
        - metrics["elo_baseline"]["log_loss"],
    }


def _segments(rows: list[dict[str, Any]], field: str) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for value in sorted({str(row[field]) for row in rows}):
        selected = [row for row in rows if str(row[field]) == value]
        metrics = _evaluate_rows(selected)
        result[value] = {
            "small_sample_warning": len(selected) < 30,
            "models": metrics,
            "raw_statistical_delta_vs_elo": _deltas(metrics),
        }
    return result


def _calibration_diagnostics(
    labels: np.ndarray, probabilities: np.ndarray, rules: dict[str, Any]
) -> dict[str, Any]:
    observed, predicted = calibration_curve(
        labels, probabilities, n_bins=CURVE_BINS, strategy="quantile", pos_label=1
    )
    predicted_class = (probabilities >= 0.5).astype(np.int64)
    confidence = np.maximum(probabilities, 1.0 - probabilities)
    correct = (predicted_class == labels).astype(np.float64)
    overall_gap = float(confidence.mean() - correct.mean())
    threshold = float(rules["high_confidence_probability_threshold"])
    high_mask = confidence >= threshold
    high_count = int(high_mask.sum())
    high_gap = (
        float(confidence[high_mask].mean() - correct[high_mask].mean())
        if high_count
        else None
    )
    overall_breach = overall_gap > float(
        rules["maximum_overall_confidence_overstatement"]
    )
    high_breach = high_count >= int(rules["minimum_high_confidence_series"]) and (
        high_gap is not None
        and high_gap > float(rules["maximum_high_confidence_overstatement"])
    )
    return {
        "curve": {
            "n_bins_requested": CURVE_BINS,
            "n_bins_returned": len(predicted),
            "strategy": "quantile",
            "points": [
                {
                    "mean_predicted_probability": float(mean_probability),
                    "fraction_positive": float(fraction_positive),
                }
                for mean_probability, fraction_positive in zip(
                    predicted, observed, strict=True
                )
            ],
        },
        "overall": {
            "mean_predicted_class_confidence": float(confidence.mean()),
            "classification_accuracy": float(correct.mean()),
            "confidence_minus_accuracy": overall_gap,
            "threshold": float(rules["maximum_overall_confidence_overstatement"]),
            "breach": overall_breach,
        },
        "high_confidence": {
            "probability_threshold": threshold,
            "series_count": high_count,
            "minimum_series_for_gate": int(rules["minimum_high_confidence_series"]),
            "confidence_minus_accuracy": high_gap,
            "threshold": float(rules["maximum_high_confidence_overstatement"]),
            "breach": high_breach,
        },
        "systematic_overconfidence_breach": overall_breach or high_breach,
    }


def _gate_decision(
    walk_forward: dict[str, Any],
    final_metrics: dict[str, dict[str, Any]],
    calibration: dict[str, Any],
    config: dict[str, Any],
) -> dict[str, Any]:
    evaluation = walk_forward.get("evaluation")
    if not isinstance(evaluation, dict):
        raise Gate1Error("walk-forward artifact lacks evaluation")
    folds = evaluation.get("folds")
    if not isinstance(folds, list) or len(folds) != 3:
        raise Gate1Error("Gate 1 requires exactly three public walk-forward folds")
    rules = config["decision_rules"]
    public_deltas = [fold["deltas_vs_elo"]["raw_statistical"] for fold in folds]
    final_delta = _deltas(final_metrics)
    window_deltas = [*public_deltas, final_delta]
    favorable = sum(
        delta["brier_score_minus_elo"] <= 0.0 and delta["log_loss_minus_elo"] <= 0.0
        for delta in window_deltas
    )

    public_overall = evaluation["overall"]["models"]
    public_count = int(public_overall["raw_statistical"]["series_count"])
    final_count = int(final_metrics["raw_statistical"]["series_count"])
    combined: dict[str, float] = {}
    for metric in ("brier_score", "log_loss"):
        raw = (
            public_overall["raw_statistical"][metric] * public_count
            + final_metrics["raw_statistical"][metric] * final_count
        ) / (public_count + final_count)
        elo = (
            public_overall["elo_baseline"][metric] * public_count
            + final_metrics["elo_baseline"][metric] * final_count
        ) / (public_count + final_count)
        combined[f"raw_{metric}"] = raw
        combined[f"elo_{metric}"] = elo
        combined[f"raw_minus_elo_{metric}"] = raw - elo

    checks = {
        "minimum_favorable_windows": {
            "actual": favorable,
            "required": int(rules["minimum_favorable_windows"]),
            "passed": favorable >= int(rules["minimum_favorable_windows"]),
        },
        "final_brier_catastrophic_degradation": {
            "actual": final_delta["brier_score_minus_elo"],
            "maximum": float(rules["maximum_final_brier_degradation_vs_elo"]),
            "passed": final_delta["brier_score_minus_elo"]
            <= float(rules["maximum_final_brier_degradation_vs_elo"]),
        },
        "final_log_loss_catastrophic_degradation": {
            "actual": final_delta["log_loss_minus_elo"],
            "maximum": float(rules["maximum_final_log_loss_degradation_vs_elo"]),
            "passed": final_delta["log_loss_minus_elo"]
            <= float(rules["maximum_final_log_loss_degradation_vs_elo"]),
        },
        "combined_brier_not_worse": {
            "actual": combined["raw_minus_elo_brier_score"],
            "maximum": 0.0,
            "passed": combined["raw_minus_elo_brier_score"] <= 0.0,
        },
        "combined_log_loss_not_worse": {
            "actual": combined["raw_minus_elo_log_loss"],
            "maximum": 0.0,
            "passed": combined["raw_minus_elo_log_loss"] <= 0.0,
        },
        "no_systematic_overconfidence": {
            "passed": not calibration["systematic_overconfidence_breach"],
        },
    }
    passed = all(check["passed"] for check in checks.values())
    return {
        "status": "passed_continue_raw" if passed else "failed_stop_modeling",
        "candidate_model": "raw_statistical",
        "calibration_decision": "sigmoid_rolled_back_before_final_release",
        "public_walk_forward_window_count": len(folds),
        "final_test_window_count": 1,
        "favorable_window_count": favorable,
        "window_deltas_vs_elo": window_deltas,
        "combined_public_and_final": {
            "series_count": public_count + final_count,
            **combined,
        },
        "checks": checks,
        "next_task_authorized": "BACK-001" if passed else None,
    }


def build_gate1_artifact(
    *,
    series_result_path: Path,
    feature_snapshot_path: Path,
    temporal_split_path: Path,
    released_manifest_path: Path,
    constant_artifact_path: Path,
    constant_manifest_path: Path,
    elo_artifact_path: Path,
    elo_manifest_path: Path,
    model_artifact_path: Path,
    model_manifest_path: Path,
    calibration_artifact_path: Path,
    calibration_manifest_path: Path,
    walk_forward_artifact_path: Path,
    walk_forward_manifest_path: Path,
    config_path: Path,
    freeze_path: Path,
) -> dict[str, Any]:
    sealed = _load_json(temporal_split_path, "sealed temporal split")
    released = _load_json(released_manifest_path, "released final-test manifest")
    freeze = _load_json(freeze_path, "freeze manifest")
    config = _load_json(config_path, "Gate 1 config")
    if _sha256_file(config_path) != freeze.get("gate_config_sha256"):
        raise Gate1Error("Gate 1 config differs from frozen hash")
    final_ids = _validate_release(sealed, released, freeze)

    constant, constant_hash = _validate_artifact(
        constant_artifact_path, constant_manifest_path, "probability_model"
    )
    elo, elo_hash = _validate_artifact(
        elo_artifact_path, elo_manifest_path, "probability_model"
    )
    raw, raw_hash = _validate_artifact(
        model_artifact_path, model_manifest_path, "probability_model"
    )
    _calibration, calibration_hash = _validate_artifact(
        calibration_artifact_path,
        calibration_manifest_path,
        "probability_calibration",
    )
    walk_forward, walk_forward_hash = _validate_artifact(
        walk_forward_artifact_path,
        walk_forward_manifest_path,
        "walk_forward_evaluation",
    )
    expected_hashes = freeze.get("frozen_artifacts")
    actual_hashes = {
        "constant_artifact_sha256": constant_hash,
        "elo_artifact_sha256": elo_hash,
        "model_artifact_sha256": raw_hash,
        "calibration_artifact_sha256": calibration_hash,
        "walk_forward_artifact_sha256": walk_forward_hash,
    }
    if expected_hashes != actual_hashes:
        raise Gate1Error("artifact hashes differ from freeze manifest")
    model = raw.get("model")
    if not isinstance(model, dict) or model.get("config_sha256") != freeze.get(
        "model_config_sha256"
    ):
        raise Gate1Error("raw model config differs from freeze manifest")
    if freeze["release_authorization"]["model_artifact_sha256"] != raw_hash:
        raise Gate1Error("release authorization does not freeze raw model")

    final, metadata = _read_final_series(
        series_result_path, feature_snapshot_path, sealed, final_ids
    )
    if _sha256_temporal_membership(final) != released["membership_sha256"]:
        raise Gate1Error(
            "evaluated final-test rows differ from Rust release commitment"
        )
    constant_probability = float(constant["model"]["probability_team_1_win"])
    raw_probabilities = _raw_probabilities(raw, final)
    elo_probabilities = _elo_probabilities(elo, final)
    predictions = []
    for index, row in enumerate(final):
        segment = metadata[row.series_id]
        predictions.append(
            {
                "series_id": row.series_id,
                "scheduled_start_utc": row.scheduled_start_utc.isoformat().replace(
                    "+00:00", "Z"
                ),
                "region": segment.region,
                "best_of": segment.best_of,
                "actual_team_1_win": row.actual_team_1_win,
                "constant_baseline": constant_probability,
                "elo_baseline": float(elo_probabilities[index]),
                "raw_statistical": float(raw_probabilities[index]),
            }
        )
    predictions.sort(key=lambda row: (row["scheduled_start_utc"], row["series_id"]))
    metrics = _evaluate_rows(predictions)
    labels = np.asarray(
        [row["actual_team_1_win"] for row in predictions], dtype=np.int64
    )
    probabilities = np.asarray(
        [row["raw_statistical"] for row in predictions], dtype=np.float64
    )
    diagnostics = _calibration_diagnostics(
        labels, probabilities, config["decision_rules"]
    )
    decision = _gate_decision(walk_forward, metrics, diagnostics, config)
    return {
        "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "gate1_final_test_decision",
        "positive_label": POSITIVE_LABEL,
        "runtime": {
            "python_version": platform.python_version(),
            "numpy_version": np.__version__,
            "scikit_learn_version": sklearn.__version__,
        },
        "freeze": freeze,
        "release": {
            "status": "released_and_evaluated_once",
            "series_count": len(final_ids),
            "membership_sha256": released["membership_sha256"],
            "released_manifest_sha256": _sha256_file(released_manifest_path),
        },
        "candidate_selection": {
            "selected_before_final_release": "raw_statistical",
            "reason": "raw beat Elo in all three public folds while sigmoid worsened two of three folds and overall",
            "calibrated_probability_evaluated_on_final_test": False,
        },
        "final_test": {
            "models": metrics,
            "raw_statistical_delta_vs_elo": _deltas(metrics),
            "raw_calibration_diagnostics": diagnostics,
            "by_region": _segments(predictions, "region"),
            "by_best_of": _segments(predictions, "best_of"),
        },
        "gate1_decision": decision,
        "predictions": predictions,
    }


def _parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Execute the one-time MODEL-007 Gate 1 evaluation."
    )
    for name in (
        "series-results",
        "feature-snapshots",
        "temporal-split",
        "released-manifest",
        "constant-artifact",
        "constant-manifest",
        "elo-artifact",
        "elo-manifest",
        "model-artifact",
        "model-manifest",
        "calibration-artifact",
        "calibration-manifest",
        "walk-forward-artifact",
        "walk-forward-manifest",
        "config",
        "freeze",
        "output",
    ):
        parser.add_argument(f"--{name}", required=True, type=Path)
    return parser.parse_args()


def main() -> None:
    arguments = _parse_arguments()
    artifact = build_gate1_artifact(
        series_result_path=arguments.series_results,
        feature_snapshot_path=arguments.feature_snapshots,
        temporal_split_path=arguments.temporal_split,
        released_manifest_path=arguments.released_manifest,
        constant_artifact_path=arguments.constant_artifact,
        constant_manifest_path=arguments.constant_manifest,
        elo_artifact_path=arguments.elo_artifact,
        elo_manifest_path=arguments.elo_manifest,
        model_artifact_path=arguments.model_artifact,
        model_manifest_path=arguments.model_manifest,
        calibration_artifact_path=arguments.calibration_artifact,
        calibration_manifest_path=arguments.calibration_manifest,
        walk_forward_artifact_path=arguments.walk_forward_artifact,
        walk_forward_manifest_path=arguments.walk_forward_manifest,
        config_path=arguments.config,
        freeze_path=arguments.freeze,
    )
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(
        json.dumps(artifact, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )


if __name__ == "__main__":
    try:
        main()
    except Gate1Error as error:
        raise SystemExit(f"MODEL-007 failed: {error}") from error
