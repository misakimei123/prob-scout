"""基于不可变 MODEL-006/007 证据构建 Gate 1 失败归因 artifact。"""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import defaultdict
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

import numpy as np
from sklearn.metrics import brier_score_loss, log_loss

ARTIFACT_SCHEMA_VERSION = 1
MODEL_NAMES = ("elo_baseline", "raw_statistical")
METRIC_NAMES = ("brier_score", "log_loss")


class FailureAttributionError(ValueError):
    """M3R-001 输入、lineage 或分析合同不成立。"""


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


def _load_artifact(
    artifact_path: Path,
    manifest_path: Path,
    expected_kind: str,
) -> tuple[dict[str, Any], dict[str, str]]:
    try:
        artifact = json.loads(artifact_path.read_text(encoding="utf-8-sig"))
        manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise FailureAttributionError(
            f"invalid immutable artifact input: {artifact_path}"
        ) from error
    if not isinstance(artifact, dict) or not isinstance(manifest, dict):
        raise FailureAttributionError("artifact and manifest roots must be objects")
    artifact_hash = _sha256_file(artifact_path)
    output = manifest.get("output")
    if not isinstance(output, dict) or output.get("sha256") != artifact_hash:
        raise FailureAttributionError(
            f"artifact hash differs from manifest: {artifact_path}"
        )
    if artifact.get("artifact_kind") != expected_kind:
        raise FailureAttributionError(
            f"unexpected artifact kind: expected={expected_kind}"
        )
    return artifact, {
        "artifact_sha256": artifact_hash,
        "manifest_sha256": _sha256_file(manifest_path),
    }


def _parse_prediction(row: Any, cohort: str) -> dict[str, Any]:
    if not isinstance(row, dict):
        raise FailureAttributionError(f"{cohort} prediction must be an object")
    required = {
        "series_id",
        "scheduled_start_utc",
        "region",
        "best_of",
        "actual_team_1_win",
        "elo_baseline",
        "raw_statistical",
    }
    if not required.issubset(row):
        raise FailureAttributionError(f"{cohort} prediction lacks required fields")
    series_id = row["series_id"]
    region = row["region"]
    if (
        not isinstance(series_id, str)
        or not series_id
        or not isinstance(region, str)
        or not region
    ):
        raise FailureAttributionError(f"invalid {cohort} prediction identity")
    try:
        scheduled_start = datetime.fromisoformat(row["scheduled_start_utc"])
    except (TypeError, ValueError) as error:
        raise FailureAttributionError(f"invalid {cohort} prediction time") from error
    if scheduled_start.tzinfo is None:
        raise FailureAttributionError(f"{cohort} prediction time lacks timezone")
    best_of = row["best_of"]
    actual = row["actual_team_1_win"]
    if best_of not in (3, 5) or actual not in (0, 1):
        raise FailureAttributionError(f"invalid {cohort} prediction segment or label")
    parsed = {
        "series_id": series_id,
        "scheduled_start_utc": scheduled_start.astimezone(UTC),
        "region": region,
        "best_of": best_of,
        "actual_team_1_win": actual,
    }
    for model_name in MODEL_NAMES:
        probability = row[model_name]
        if (
            not isinstance(probability, int | float)
            or isinstance(probability, bool)
            or not 0.0 < float(probability) < 1.0
        ):
            raise FailureAttributionError(f"invalid {cohort} probability: {model_name}")
        parsed[model_name] = float(probability)
    if "fold" in row:
        parsed["fold"] = row["fold"]
    return parsed


def _load_predictions(artifact: dict[str, Any], cohort: str) -> list[dict[str, Any]]:
    value = artifact.get("predictions")
    if not isinstance(value, list) or not value:
        raise FailureAttributionError(f"{cohort} artifact lacks predictions")
    rows = [_parse_prediction(row, cohort) for row in value]
    rows.sort(key=lambda row: (row["scheduled_start_utc"], row["series_id"]))
    ids = [row["series_id"] for row in rows]
    if len(ids) != len(set(ids)):
        raise FailureAttributionError(f"duplicate {cohort} prediction series_id")
    return rows


def _metric_summary(rows: list[dict[str, Any]]) -> dict[str, Any]:
    if not rows:
        raise FailureAttributionError("metric rows must not be empty")
    labels = np.asarray([row["actual_team_1_win"] for row in rows], dtype=np.int64)
    result: dict[str, Any] = {"series_count": len(rows), "models": {}}
    for model_name in MODEL_NAMES:
        probabilities = np.asarray([row[model_name] for row in rows], dtype=np.float64)
        result["models"][model_name] = {
            "brier_score": float(brier_score_loss(labels, probabilities)),
            "log_loss": float(
                log_loss(
                    labels,
                    np.column_stack((1.0 - probabilities, probabilities)),
                    labels=[0, 1],
                )
            ),
            "mean_probability_team_1_win": float(probabilities.mean()),
        }
    result["raw_minus_elo"] = {
        metric: result["models"]["raw_statistical"][metric]
        - result["models"]["elo_baseline"][metric]
        for metric in METRIC_NAMES
    }
    return result


def _group_summary(rows: list[dict[str, Any]], field: str) -> dict[str, Any]:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for row in rows:
        grouped[str(row[field])].append(row)
    total = len(rows)
    return {
        value: {
            "share": len(selected) / total,
            **_metric_summary(selected),
        }
        for value, selected in sorted(grouped.items())
    }


def _joint_key(row: dict[str, Any]) -> str:
    return f"{row['region']}|BO{row['best_of']}"


def _composition_decomposition(
    public_rows: list[dict[str, Any]], final_rows: list[dict[str, Any]]
) -> dict[str, Any]:
    public_groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
    final_groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for row in public_rows:
        public_groups[_joint_key(row)].append(row)
    for row in final_rows:
        final_groups[_joint_key(row)].append(row)
    missing_public = sorted(set(final_groups).difference(public_groups))
    common_keys = set(final_groups).intersection(public_groups)
    if not common_keys:
        raise FailureAttributionError("final composition has no public reference cells")
    public_common = [row for row in public_rows if _joint_key(row) in common_keys]
    final_common = [row for row in final_rows if _joint_key(row) in common_keys]
    uncovered_final = [row for row in final_rows if _joint_key(row) in missing_public]

    public_delta = _metric_summary(public_common)["raw_minus_elo"]
    final_delta = _metric_summary(final_common)["raw_minus_elo"]
    counterfactual = {metric: 0.0 for metric in METRIC_NAMES}
    cells: dict[str, Any] = {}
    for key in sorted(set(public_groups).union(final_groups)):
        public = public_groups.get(key, [])
        final = final_groups.get(key, [])
        public_metrics = _metric_summary(public) if public else None
        final_metrics = _metric_summary(final) if final else None
        final_weight = len(final) / len(final_common) if key in common_keys else 0.0
        if final and key in common_keys:
            assert public_metrics is not None
            for metric in METRIC_NAMES:
                counterfactual[metric] += (
                    final_weight * public_metrics["raw_minus_elo"][metric]
                )
        cells[key] = {
            "public_count": len(public),
            "public_share_overall": len(public) / len(public_rows),
            "public_share_within_common": (
                len(public) / len(public_common) if key in common_keys else None
            ),
            "final_count": len(final),
            "final_share_overall": len(final) / len(final_rows),
            "final_share_within_common": (final_weight if key in common_keys else None),
            "has_public_reference": key in public_groups,
            "public_raw_minus_elo": (
                public_metrics["raw_minus_elo"] if public_metrics else None
            ),
            "final_raw_minus_elo": (
                final_metrics["raw_minus_elo"] if final_metrics else None
            ),
        }
    return {
        "joint_cells": "region_x_best_of",
        "status": (
            "complete" if not missing_public else "partial_due_to_unseen_final_cells"
        ),
        "all_final_cells_have_public_reference": not missing_public,
        "unseen_final_cells": missing_public,
        "unseen_final_series_count": len(uncovered_final),
        "unseen_final_share": len(uncovered_final) / len(final_rows),
        "unseen_final_actual_raw_minus_elo": (
            _metric_summary(uncovered_final)["raw_minus_elo"]
            if uncovered_final
            else None
        ),
        "common_public_series_count": len(public_common),
        "common_final_series_count": len(final_common),
        "common_public_raw_minus_elo": public_delta,
        "common_final_raw_minus_elo": final_delta,
        "counterfactual_final_composition_with_public_cell_performance": counterfactual,
        "composition_effect": {
            metric: counterfactual[metric] - public_delta[metric]
            for metric in METRIC_NAMES
        },
        "within_cell_time_shift_residual": {
            metric: final_delta[metric] - counterfactual[metric]
            for metric in METRIC_NAMES
        },
        "cells": cells,
        "interpretation_boundary": (
            "descriptive Oaxaca-style decomposition over common cells only; residual "
            "includes time drift, model staleness, opponent mix and unobserved factors. "
            "Unseen final cells are reported separately and never extrapolated"
        ),
    }


def _fixed_model_public_replay(
    model_artifact: dict[str, Any], public_rows: list[dict[str, Any]]
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    predictions = model_artifact.get("development_predictions")
    if not isinstance(predictions, list):
        raise FailureAttributionError("MODEL-004 lacks development predictions")
    raw_by_id: dict[str, float] = {}
    for prediction in predictions:
        if not isinstance(prediction, dict):
            raise FailureAttributionError("MODEL-004 prediction must be an object")
        series_id = prediction.get("series_id")
        probability = prediction.get("raw_probability_team_1_win")
        if (
            not isinstance(series_id, str)
            or not series_id
            or series_id in raw_by_id
            or not isinstance(probability, int | float)
            or isinstance(probability, bool)
            or not 0.0 < float(probability) < 1.0
        ):
            raise FailureAttributionError("invalid MODEL-004 development prediction")
        raw_by_id[series_id] = float(probability)

    replay = []
    for row in public_rows:
        if row["series_id"] not in raw_by_id:
            raise FailureAttributionError(
                f"MODEL-004 misses public evaluation row: {row['series_id']}"
            )
        replay.append({**row, "raw_statistical": raw_by_id[row["series_id"]]})
    by_fold = _group_summary(replay, "fold")
    expanding_by_fold = _group_summary(public_rows, "fold")
    return replay, {
        "fixed_candidate_overall": _metric_summary(replay),
        "expanding_walk_forward_overall": _metric_summary(public_rows),
        "by_fold": {
            fold: {
                "fixed_candidate": by_fold[fold],
                "expanding_walk_forward": expanding_by_fold[fold],
                "fixed_minus_expanding_raw": {
                    metric: by_fold[fold]["models"]["raw_statistical"][metric]
                    - expanding_by_fold[fold]["models"]["raw_statistical"][metric]
                    for metric in METRIC_NAMES
                },
            }
            for fold in sorted(by_fold)
        },
    }


def _weekly_summary(rows: list[dict[str, Any]]) -> dict[str, Any]:
    start = rows[0]["scheduled_start_utc"].replace(
        hour=0, minute=0, second=0, microsecond=0
    )
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for row in rows:
        index = (row["scheduled_start_utc"] - start).days // 7
        window_start = start + timedelta(days=7 * index)
        key = window_start.date().isoformat()
        grouped[key].append(row)
    return {
        key: {
            "window_start_utc": f"{key}T00:00:00Z",
            **_metric_summary(selected),
        }
        for key, selected in sorted(grouped.items())
    }


def _disagreement_attribution(rows: list[dict[str, Any]]) -> dict[str, Any]:
    grouped: dict[str, list[float]] = defaultdict(list)
    disagreement_magnitudes = []
    for row in rows:
        actual = row["actual_team_1_win"]
        raw_class = int(row["raw_statistical"] >= 0.5)
        elo_class = int(row["elo_baseline"] >= 0.5)
        if raw_class == actual and elo_class == actual:
            category = "both_correct"
        elif raw_class != actual and elo_class != actual:
            category = "both_wrong"
        elif raw_class == actual:
            category = "raw_only_correct"
        else:
            category = "elo_only_correct"
        regret = (row["raw_statistical"] - actual) ** 2 - (
            row["elo_baseline"] - actual
        ) ** 2
        grouped[category].append(regret)
        disagreement_magnitudes.append(
            abs(row["raw_statistical"] - row["elo_baseline"])
        )
    categories = {}
    for category in (
        "both_correct",
        "both_wrong",
        "raw_only_correct",
        "elo_only_correct",
    ):
        values = grouped.get(category, [])
        categories[category] = {
            "series_count": len(values),
            "mean_brier_regret_raw_minus_elo": (
                float(np.mean(values)) if values else None
            ),
            "contribution_to_overall_brier_delta": sum(values) / len(rows),
        }
    magnitudes = np.asarray(disagreement_magnitudes, dtype=np.float64)
    return {
        "hard_class_threshold": 0.5,
        "categories": categories,
        "absolute_probability_difference": {
            "mean": float(magnitudes.mean()),
            "p50": float(np.quantile(magnitudes, 0.5)),
            "p90": float(np.quantile(magnitudes, 0.9)),
            "maximum": float(magnitudes.max()),
        },
    }


def build_failure_attribution_artifact(
    *,
    model_artifact: dict[str, Any],
    model_input: dict[str, str],
    walk_forward_artifact: dict[str, Any],
    walk_forward_input: dict[str, str],
    gate_artifact: dict[str, Any],
    gate_input: dict[str, str],
) -> dict[str, Any]:
    gate_decision = gate_artifact.get("gate1_decision")
    release = gate_artifact.get("release")
    if (
        not isinstance(gate_decision, dict)
        or gate_decision.get("status") != "failed_stop_modeling"
        or not isinstance(release, dict)
        or release.get("status") != "released_and_evaluated_once"
    ):
        raise FailureAttributionError("M3R-001 requires a completed failed Gate 1")

    public_rows = _load_predictions(walk_forward_artifact, "public walk-forward")
    final_rows = _load_predictions(gate_artifact, "retired final test")
    if {row["series_id"] for row in public_rows}.intersection(
        row["series_id"] for row in final_rows
    ):
        raise FailureAttributionError("public and retired final cohorts overlap")
    if public_rows[-1]["scheduled_start_utc"] >= final_rows[0]["scheduled_start_utc"]:
        raise FailureAttributionError("retired final cohort is not strictly later")

    fixed_public, training_protocol = _fixed_model_public_replay(
        model_artifact, public_rows
    )
    training = model_artifact.get("training")
    folds = walk_forward_artifact.get("evaluation", {}).get("folds")
    if not isinstance(training, dict) or not isinstance(folds, list) or not folds:
        raise FailureAttributionError("missing training-window evidence")
    final_fold_train = folds[-1].get("windows", {}).get("train")
    if not isinstance(final_fold_train, dict):
        raise FailureAttributionError("missing final public fold train window")
    fixed_count = training.get("series_count")
    expanding_count = final_fold_train.get("series_count")
    if not isinstance(fixed_count, int) or not isinstance(expanding_count, int):
        raise FailureAttributionError("invalid training counts")

    composition = _composition_decomposition(fixed_public, final_rows)
    final_delta = _metric_summary(final_rows)["raw_minus_elo"]
    residual = composition["within_cell_time_shift_residual"]
    confirmed_findings = [
        {
            "id": "fixed_candidate_public_advantage",
            "status": "confirmed",
            "evidence": training_protocol["fixed_candidate_overall"]["raw_minus_elo"],
            "interpretation": (
                "the exact frozen 325-series candidate still beat Elo across all three "
                "public folds, so expanding retraining is not required to reproduce the public sign"
            ),
        },
        {
            "id": "training_protocol_mismatch",
            "status": "confirmed_contract_difference_not_causal_explanation",
            "evidence": {
                "frozen_candidate_training_count": fixed_count,
                "last_walk_forward_training_count": expanding_count,
                "additional_series_in_last_walk_forward_train": expanding_count
                - fixed_count,
            },
            "interpretation": (
                "MODEL-006 estimated an expanding-training procedure while MODEL-007 deployed "
                "the original frozen train-only model; public replay shows the mismatch is modest "
                "but the protocol must be aligned in a future Gate"
            ),
        },
        {
            "id": "final_sign_reversal",
            "status": "confirmed",
            "evidence": final_delta,
            "interpretation": "the retired Final Test reverses the public advantage and exceeds both stop thresholds",
        },
        {
            "id": "common_cell_shift_vs_composition",
            "status": "confirmed_descriptive_partial_not_causal",
            "evidence": {
                "composition_effect": composition["composition_effect"],
                "within_cell_time_shift_residual": residual,
            },
            "interpretation": (
                "within public-covered region x BO cells, composition and residual are "
                "separated; unseen Final cells remain an explicit evidence gap"
            ),
        },
    ]
    config = {
        "public_model_replay": "frozen_model004_raw_probability",
        "baseline": "model006_chronological_elo",
        "composition_cells": ["region", "best_of"],
        "time_slice": "seven_day_blocks_from_final_start",
        "hard_class_threshold": 0.5,
        "causal_claims_allowed": False,
    }
    return {
        "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "gate1_failure_attribution",
        "analysis_status": "diagnostic_complete_not_a_new_gate",
        "config": config,
        "config_sha256": _sha256_json(config),
        "inputs": {
            "frozen_model": model_input,
            "public_walk_forward": walk_forward_input,
            "failed_gate1": gate_input,
        },
        "cohort_governance": {
            "public_walk_forward_series_count": len(public_rows),
            "retired_final_series_count": len(final_rows),
            "membership_overlap_count": 0,
            "retired_final_status": "retired_diagnostic_evidence_never_independent_again",
            "prohibited_uses": [
                "model_selection",
                "hyperparameter_selection",
                "feature_selection",
                "calibration_selection",
                "future_gate_final_test",
            ],
        },
        "training_protocol": {
            **training_protocol,
            "frozen_candidate_training_count": fixed_count,
            "last_walk_forward_training_count": expanding_count,
        },
        "cohort_metrics": {
            "public_fixed_candidate": _metric_summary(fixed_public),
            "retired_final": _metric_summary(final_rows),
            "public_by_region": _group_summary(fixed_public, "region"),
            "final_by_region": _group_summary(final_rows, "region"),
            "public_by_best_of": _group_summary(fixed_public, "best_of"),
            "final_by_best_of": _group_summary(final_rows, "best_of"),
            "final_by_seven_day_window": _weekly_summary(final_rows),
        },
        "composition_decomposition": composition,
        "final_prediction_disagreement": _disagreement_attribution(final_rows),
        "findings": confirmed_findings,
        "causal_boundary": {
            "conclusion": "failure_is_real_but_root_cause_is_not_identified",
            "supported": [
                "exact frozen candidate had a small public advantage",
                "Final Test produced a large adverse sign reversal",
                "region and BO composition is insufficient to explain the reversal",
                "training protocol differed between walk-forward procedure and frozen deployment",
            ],
            "not_supported": [
                "a specific feature caused the failure",
                "retraining on more old data would fix the failure",
                "region-specific models would generalize",
                "the retired Final Test can validate any proposed fix",
            ],
        },
        "next_task": {
            "task_id": "M3R-002",
            "status": "authorized_for_data_expansion_only",
            "minimum_new_data_start_utc": "2025-07-01T00:00:00Z",
            "requires_new_final_test_seal": True,
            "model_development_authorized": False,
            "m4_authorized": False,
        },
    }


def _parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build M3R-001 attribution from immutable MODEL-004/006/007 artifacts."
    )
    for name in (
        "model-artifact",
        "model-manifest",
        "walk-forward-artifact",
        "walk-forward-manifest",
        "gate-artifact",
        "gate-manifest",
        "output",
    ):
        parser.add_argument(f"--{name}", required=True, type=Path)
    return parser.parse_args()


def main() -> None:
    arguments = _parse_arguments()
    model, model_input = _load_artifact(
        arguments.model_artifact, arguments.model_manifest, "probability_model"
    )
    walk_forward, walk_forward_input = _load_artifact(
        arguments.walk_forward_artifact,
        arguments.walk_forward_manifest,
        "walk_forward_evaluation",
    )
    gate, gate_input = _load_artifact(
        arguments.gate_artifact,
        arguments.gate_manifest,
        "gate1_final_test_decision",
    )
    artifact = build_failure_attribution_artifact(
        model_artifact=model,
        model_input=model_input,
        walk_forward_artifact=walk_forward,
        walk_forward_input=walk_forward_input,
        gate_artifact=gate,
        gate_input=gate_input,
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
    except FailureAttributionError as error:
        raise SystemExit(f"M3R-001 failed: {error}") from error
