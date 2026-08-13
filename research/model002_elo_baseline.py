"""构建 MODEL-002 严格按赛前历史更新的 Elo Baseline。"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import platform
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import numpy as np
import sklearn
from sklearn.metrics import brier_score_loss, log_loss

ARTIFACT_SCHEMA_VERSION = 1
MODEL_FAMILY = "elo_baseline"
MODEL_STRATEGY = "global_chronological_elo"
POSITIVE_LABEL = "team_1_win"
FINAL_TEST_STATUS = "sealed_not_evaluated"
FINAL_TEST_ACCESS_POLICY = "sealed_until_model_freeze"
DEVELOPMENT_SPLITS = ("train", "validation", "calibration")
INITIAL_RATING = 1500.0
RATING_SCALE = 400.0
K_FACTOR = 20.0


class EloBaselineError(ValueError):
    """输入或合同不满足 MODEL-002 fail-closed 约束。"""


@dataclass(frozen=True)
class EloSeries:
    series_id: str
    split: str
    scheduled_start_utc: datetime
    region: str
    team_ids: tuple[str, str]
    actual_team_1_win: int


@dataclass(frozen=True)
class LoadedEloData:
    series: list[EloSeries]
    series_ids_by_split: dict[str, list[str]]
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


def _repository_relative_path(repository_root: Path, path: Path) -> str:
    try:
        relative = path.resolve().relative_to(repository_root.resolve())
    except ValueError as error:
        raise EloBaselineError(f"path escapes repository root: {path}") from error
    return relative.as_posix()


def _validated_input_reference(
    repository_root: Path,
    dataset_path: Path,
    manifest_path: Path,
    expected_dataset_name: str,
) -> dict[str, str]:
    if not dataset_path.is_file() or not manifest_path.is_file():
        raise EloBaselineError(
            f"missing dataset or manifest: dataset={dataset_path}, manifest={manifest_path}"
        )
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise EloBaselineError(f"invalid dataset manifest: {manifest_path}") from error
    dataset = manifest.get("dataset")
    output = manifest.get("output")
    if not isinstance(dataset, dict) or not isinstance(output, dict):
        raise EloBaselineError("dataset manifest is missing dataset/output metadata")
    if dataset.get("name") != expected_dataset_name:
        raise EloBaselineError(
            f"unexpected dataset name: expected={expected_dataset_name}, actual={dataset.get('name')}"
        )
    relative_path = _repository_relative_path(repository_root, dataset_path)
    dataset_sha256 = _sha256_file(dataset_path)
    if output.get("relative_path") != relative_path:
        raise EloBaselineError("dataset path does not match its manifest output")
    if output.get("sha256") != dataset_sha256:
        raise EloBaselineError("dataset SHA-256 does not match its manifest output")
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


def _parse_utc(value: str, field: str) -> datetime:
    try:
        parsed = datetime.fromisoformat(value)
    except ValueError as error:
        raise EloBaselineError(f"invalid {field}: {value}") from error
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        raise EloBaselineError(f"{field} must be timezone-aware")
    return parsed.astimezone(UTC)


def _require_series_ids(split_name: str, value: Any) -> list[str]:
    if not isinstance(value, list) or not value:
        raise EloBaselineError(f"{split_name}.series_ids must be a non-empty list")
    if any(
        not isinstance(series_id, str) or not series_id.strip() for series_id in value
    ):
        raise EloBaselineError(f"{split_name} contains an empty series_id")
    if len(value) != len(set(value)):
        raise EloBaselineError(f"{split_name} contains duplicate series_id values")
    return list(value)


def load_elo_data(series_result_path: Path, temporal_split_path: Path) -> LoadedEloData:
    try:
        temporal_split = json.loads(temporal_split_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise EloBaselineError("invalid temporal split manifest") from error
    if temporal_split.get("manifest_version") != 1:
        raise EloBaselineError("unsupported temporal split manifest version")

    series_ids_by_split: dict[str, list[str]] = {}
    split_by_series_id: dict[str, str] = {}
    for split_name in DEVELOPMENT_SPLITS:
        split = temporal_split.get(split_name)
        if not isinstance(split, dict):
            raise EloBaselineError(f"missing development split: {split_name}")
        series_ids = _require_series_ids(split_name, split.get("series_ids"))
        for series_id in series_ids:
            if series_id in split_by_series_id:
                raise EloBaselineError(
                    f"series_id appears in multiple development splits: {series_id}"
                )
            split_by_series_id[series_id] = split_name
        series_ids_by_split[split_name] = series_ids

    final_test = temporal_split.get("final_test")
    if not isinstance(final_test, dict):
        raise EloBaselineError("missing sealed final_test split")
    if "series_ids" in final_test:
        raise EloBaselineError("sealed final_test must not expose series_ids")
    final_count = final_test.get("series_count")
    membership_sha256 = final_test.get("membership_sha256")
    if (
        not isinstance(final_count, int)
        or isinstance(final_count, bool)
        or final_count <= 0
        or not isinstance(membership_sha256, str)
        or len(membership_sha256) != 64
        or final_test.get("access_policy") != FINAL_TEST_ACCESS_POLICY
    ):
        raise EloBaselineError("sealed final_test contract is invalid")

    development_series: list[EloSeries] = []
    seen_development_ids: set[str] = set()
    total_series_count = 0
    try:
        with series_result_path.open("r", encoding="utf-8-sig", newline="") as source:
            reader = csv.DictReader(source)
            required_fields = {
                "series_id",
                "scheduled_start_utc",
                "region",
                "team_1_id",
                "team_2_id",
                "winner_team_id",
            }
            if reader.fieldnames is None or not required_fields.issubset(
                reader.fieldnames
            ):
                raise EloBaselineError("Series Result CSV is missing MODEL-002 fields")
            for row in reader:
                total_series_count += 1
                series_id = (row.get("series_id") or "").strip()
                split_name = split_by_series_id.get(series_id)
                if split_name is None:
                    # 未公开的剩余行保持不可归属状态，不从时间窗口推断 final-test 成员。
                    continue
                if series_id in seen_development_ids:
                    raise EloBaselineError(
                        f"duplicate development Series Result: {series_id}"
                    )
                seen_development_ids.add(series_id)
                team_one = (row.get("team_1_id") or "").strip()
                team_two = (row.get("team_2_id") or "").strip()
                winner = (row.get("winner_team_id") or "").strip()
                region = (row.get("region") or "").strip()
                if not team_one or not team_two or team_one == team_two or not region:
                    raise EloBaselineError(f"invalid series fields: {series_id}")
                if winner == team_one:
                    actual = 1
                elif winner == team_two:
                    actual = 0
                else:
                    raise EloBaselineError(
                        f"winner does not match either team for series_id={series_id}"
                    )
                development_series.append(
                    EloSeries(
                        series_id=series_id,
                        split=split_name,
                        scheduled_start_utc=_parse_utc(
                            (row.get("scheduled_start_utc") or "").strip(),
                            "scheduled_start_utc",
                        ),
                        region=region,
                        team_ids=(team_one, team_two),
                        actual_team_1_win=actual,
                    )
                )
    except OSError as error:
        raise EloBaselineError("failed to read Series Result CSV") from error

    missing = set(split_by_series_id).difference(seen_development_ids)
    if missing:
        raise EloBaselineError(
            f"development split references missing Series Result: {min(missing)}"
        )
    expected_total = len(split_by_series_id) + final_count
    if total_series_count != expected_total:
        raise EloBaselineError(
            "Series Result count does not match development plus sealed final count: "
            f"actual={total_series_count}, expected={expected_total}"
        )

    development_series.sort(
        key=lambda series: (series.scheduled_start_utc, series.series_id)
    )
    # 同一队同一开赛时刻没有可证明的结果先后关系，不能用 series_id 排序制造历史。
    teams_by_start: dict[datetime, set[str]] = {}
    for series in development_series:
        teams = teams_by_start.setdefault(series.scheduled_start_utc, set())
        overlap = teams.intersection(series.team_ids)
        if overlap:
            raise EloBaselineError(
                "team appears in multiple series at the same scheduled start: "
                f"team_id={min(overlap)}"
            )
        teams.update(series.team_ids)

    return LoadedEloData(
        series=development_series,
        series_ids_by_split=series_ids_by_split,
        final_test={
            "series_count": final_count,
            "membership_sha256": membership_sha256,
            "access_policy": FINAL_TEST_ACCESS_POLICY,
        },
    )


def expected_team_one_win(
    team_one_rating: float, team_two_rating: float, rating_scale: float = RATING_SCALE
) -> float:
    if not math.isfinite(team_one_rating) or not math.isfinite(team_two_rating):
        raise EloBaselineError("ratings must be finite")
    if not math.isfinite(rating_scale) or rating_scale <= 0.0:
        raise EloBaselineError("rating_scale must be positive")
    return 1.0 / (1.0 + 10.0 ** ((team_two_rating - team_one_rating) / rating_scale))


def evaluate_predictions(predictions: list[dict[str, Any]]) -> dict[str, int | float]:
    if not predictions:
        raise EloBaselineError("evaluation predictions must not be empty")
    y_true = np.asarray(
        [prediction["actual_team_1_win"] for prediction in predictions],
        dtype=np.uint8,
    )
    probabilities = np.asarray(
        [prediction["probability_team_1_win"] for prediction in predictions],
        dtype=np.float64,
    )
    class_probabilities = np.column_stack((1.0 - probabilities, probabilities))
    return {
        "series_count": len(predictions),
        "team_1_win_count": int(y_true.sum()),
        "observed_team_1_win_rate": float(y_true.mean()),
        "mean_probability_team_1_win": float(probabilities.mean()),
        "brier_score": float(
            brier_score_loss(y_true, probabilities, pos_label=1, scale_by_half=True)
        ),
        "log_loss": float(log_loss(y_true, class_probabilities, labels=[0, 1])),
    }


def run_chronological_elo(
    series: list[EloSeries],
    *,
    initial_rating: float = INITIAL_RATING,
    rating_scale: float = RATING_SCALE,
    k_factor: float = K_FACTOR,
) -> tuple[list[dict[str, Any]], dict[str, float]]:
    if not math.isfinite(initial_rating):
        raise EloBaselineError("initial_rating must be finite")
    if not math.isfinite(k_factor) or k_factor <= 0.0:
        raise EloBaselineError("k_factor must be positive")
    if series != sorted(
        series, key=lambda item: (item.scheduled_start_utc, item.series_id)
    ):
        raise EloBaselineError("series must be sorted chronologically")

    ratings: dict[str, float] = {}
    predictions: list[dict[str, Any]] = []
    last_start: datetime | None = None
    teams_at_last_start: set[str] = set()
    for item in series:
        if item.scheduled_start_utc != last_start:
            last_start = item.scheduled_start_utc
            teams_at_last_start = set()
        overlap = teams_at_last_start.intersection(item.team_ids)
        if overlap:
            raise EloBaselineError(
                "team appears in multiple series at the same scheduled start: "
                f"team_id={min(overlap)}"
            )
        teams_at_last_start.update(item.team_ids)

        team_one, team_two = item.team_ids
        team_one_seen = team_one in ratings
        team_two_seen = team_two in ratings
        team_one_rating = ratings.get(team_one, initial_rating)
        team_two_rating = ratings.get(team_two, initial_rating)
        # 先用当前时刻之前累计的 rating 产生概率，再读取本场 label 更新；禁止当前赛果回流到自身预测。
        probability = expected_team_one_win(
            team_one_rating, team_two_rating, rating_scale
        )
        predictions.append(
            {
                "series_id": item.series_id,
                "split": item.split,
                "scheduled_start_utc": item.scheduled_start_utc.isoformat().replace(
                    "+00:00", "Z"
                ),
                "region": item.region,
                "team_ids": [team_one, team_two],
                "team_seen_before": [team_one_seen, team_two_seen],
                "pre_match_ratings": [team_one_rating, team_two_rating],
                "probability_team_1_win": probability,
                "actual_team_1_win": item.actual_team_1_win,
            }
        )
        # 两队使用同一 delta 的正负值，保持每场更新前后的 rating 总和不变。
        delta = k_factor * (item.actual_team_1_win - probability)
        ratings[team_one] = team_one_rating + delta
        ratings[team_two] = team_two_rating - delta

    return predictions, dict(sorted(ratings.items()))


def build_artifact(
    loaded: LoadedEloData,
    series_input: dict[str, str],
    temporal_split_input: dict[str, str],
) -> dict[str, Any]:
    predictions, terminal_ratings = run_chronological_elo(loaded.series)
    predictions_by_split = {
        split_name: [
            prediction
            for prediction in predictions
            if prediction["split"] == split_name
        ]
        for split_name in DEVELOPMENT_SPLITS
    }
    config = {
        "initial_rating": INITIAL_RATING,
        "rating_scale": RATING_SCALE,
        "k_factor": K_FACTOR,
        "rating_pool": "global",
        "update_unit": "completed_series",
        "update_timing": "after_prediction",
        "tie_breaker": "series_id_for_disjoint_same-start-series_only",
    }
    first_time_team_sides = sum(
        int(not seen)
        for prediction in predictions
        for seen in prediction["team_seen_before"]
    )
    return {
        "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "probability_model",
        "model": {
            "family": MODEL_FAMILY,
            "strategy": MODEL_STRATEGY,
            "positive_label": POSITIVE_LABEL,
            "uses_features": False,
            "uses_market_data": False,
            "configuration": config,
            "configuration_sha256": _sha256_json(config),
        },
        "runtime": {
            "python_version": platform.python_version(),
            "numpy_version": np.__version__,
            "scikit_learn_version": sklearn.__version__,
        },
        "inputs": {
            "series_result": series_input,
            "temporal_split": temporal_split_input,
        },
        "development_summary": {
            "series_count": len(predictions),
            "unique_team_count": len(terminal_ratings),
            "first_time_team_side_count": first_time_team_sides,
            "chronological_start_utc": predictions[0]["scheduled_start_utc"],
            "chronological_end_utc": predictions[-1]["scheduled_start_utc"],
        },
        "development_evaluation": {
            split_name: evaluate_predictions(predictions_by_split[split_name])
            for split_name in DEVELOPMENT_SPLITS
        },
        "development_predictions": predictions,
        "terminal_ratings_after_calibration": terminal_ratings,
        "final_test_evaluation": {
            "status": FINAL_TEST_STATUS,
            **loaded.final_test,
            "supported_metrics": ["brier_score", "log_loss"],
            "release_requires": [
                "model_artifact_sha256",
                "model_config_sha256",
                "evaluation_code_sha256",
            ],
        },
    }


def _parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build the MODEL-002 chronological Elo Baseline artifact."
    )
    parser.add_argument("--repository-root", required=True, type=Path)
    parser.add_argument("--series-results", required=True, type=Path)
    parser.add_argument("--series-manifest", required=True, type=Path)
    parser.add_argument("--temporal-split", required=True, type=Path)
    parser.add_argument("--temporal-split-manifest", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def main() -> None:
    arguments = _parse_arguments()
    repository_root = arguments.repository_root.resolve()
    series_input = _validated_input_reference(
        repository_root,
        arguments.series_results.resolve(),
        arguments.series_manifest.resolve(),
        "lol-series-results",
    )
    split_input = _validated_input_reference(
        repository_root,
        arguments.temporal_split.resolve(),
        arguments.temporal_split_manifest.resolve(),
        "lol-temporal-splits",
    )
    loaded = load_elo_data(
        arguments.series_results.resolve(), arguments.temporal_split.resolve()
    )
    artifact = build_artifact(loaded, series_input, split_input)
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(
        json.dumps(artifact, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )


if __name__ == "__main__":
    try:
        main()
    except (EloBaselineError, OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"Elo baseline build failed: {error}") from error
