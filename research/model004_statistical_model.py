"""构建 MODEL-004 基于赛前 team-form 差值的可解释统计模型。"""

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
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import brier_score_loss, log_loss
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler

ARTIFACT_SCHEMA_VERSION = 1
MODEL_FAMILY = "logistic_regression"
MODEL_STRATEGY = "train_only_standardized_team_form_differences"
POSITIVE_LABEL = "team_1_win"
DEVELOPMENT_SPLITS = ("train", "validation", "calibration")
FINAL_TEST_STATUS = "sealed_not_evaluated"
FINAL_TEST_ACCESS_POLICY = "sealed_until_model_freeze"
RANDOM_SEED = 20260813
MODEL_C = 1.0
MAX_ITERATIONS = 1000
TOLERANCE = 1e-8
NEUTRAL_WIN_RATE = 0.5
SNAPSHOT_LEAD_MINUTES = 15
FEATURE_NAMES = (
    "prior_series_win_rate_diff",
    "prior_game_win_rate_diff",
    "same_patch_series_win_rate_diff",
    "log1p_prior_series_count_diff",
    "log1p_prior_game_count_diff",
    "log1p_same_patch_series_count_diff",
    "log1p_rest_minutes_diff",
    "prior_history_available_diff",
    "same_patch_history_available_diff",
    "best_of_5",
)
FORBIDDEN_FEATURE_FIELDS = {
    "winner_team_id",
    "team_1_score",
    "team_2_score",
    "winner_outcome_index",
    "resolution_status",
    "market_resolution",
}


class StatisticalModelError(ValueError):
    """输入或合同不满足 MODEL-004 fail-closed 约束。"""


@dataclass(frozen=True)
class TeamForm:
    team_id: str
    prior_series_count: int
    prior_series_win_rate: float
    prior_game_count: int
    prior_game_win_rate: float
    same_patch_series_count: int
    same_patch_series_win_rate: float
    rest_minutes: int | None

    @property
    def prior_history_available(self) -> int:
        return int(self.prior_series_count > 0)

    @property
    def same_patch_history_available(self) -> int:
        return int(self.same_patch_series_count > 0)


@dataclass(frozen=True)
class StatisticalSeries:
    series_id: str
    split: str
    scheduled_start_utc: datetime
    team_ids: tuple[str, str]
    feature_values: tuple[float, ...]
    actual_team_1_win: int


@dataclass(frozen=True)
class LoadedStatisticalData:
    series: list[StatisticalSeries]
    series_ids_by_split: dict[str, list[str]]
    final_test: dict[str, Any]
    total_series_count: int


@dataclass(frozen=True)
class FittedStatisticalModel:
    pipeline: Pipeline
    standardized_coefficients: tuple[float, ...]
    standardized_intercept: float
    raw_coefficients: tuple[float, ...]
    raw_intercept: float
    scaler_mean: tuple[float, ...]
    scaler_scale: tuple[float, ...]
    iterations: int


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
        raise StatisticalModelError(f"path escapes repository root: {path}") from error
    return relative.as_posix()


def _validated_input_reference(
    repository_root: Path,
    dataset_path: Path,
    manifest_path: Path,
    expected_dataset_name: str,
) -> dict[str, str]:
    if not dataset_path.is_file() or not manifest_path.is_file():
        raise StatisticalModelError(
            f"missing dataset or manifest: dataset={dataset_path}, manifest={manifest_path}"
        )
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise StatisticalModelError(
            f"invalid dataset manifest: {manifest_path}"
        ) from error
    dataset = manifest.get("dataset")
    output = manifest.get("output")
    if not isinstance(dataset, dict) or not isinstance(output, dict):
        raise StatisticalModelError(
            "dataset manifest is missing dataset/output metadata"
        )
    if dataset.get("name") != expected_dataset_name:
        raise StatisticalModelError(
            f"unexpected dataset name: expected={expected_dataset_name}, "
            f"actual={dataset.get('name')}"
        )
    relative_path = _repository_relative_path(repository_root, dataset_path)
    dataset_sha256 = _sha256_file(dataset_path)
    if output.get("relative_path") != relative_path:
        raise StatisticalModelError("dataset path does not match its manifest output")
    if output.get("sha256") != dataset_sha256:
        raise StatisticalModelError(
            "dataset SHA-256 does not match its manifest output"
        )
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
        raise StatisticalModelError(f"invalid {field}: {value}") from error
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        raise StatisticalModelError(f"{field} must be timezone-aware")
    return parsed.astimezone(UTC)


def _require_nonnegative_int(value: Any, field: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise StatisticalModelError(f"{field} must be a nonnegative integer")
    return value


def _require_series_ids(split_name: str, value: Any) -> list[str]:
    if not isinstance(value, list) or not value:
        raise StatisticalModelError(f"{split_name}.series_ids must be non-empty")
    if any(
        not isinstance(series_id, str) or not series_id.strip() for series_id in value
    ):
        raise StatisticalModelError(f"{split_name} contains an empty series_id")
    if len(value) != len(set(value)):
        raise StatisticalModelError(f"{split_name} contains duplicate series_id values")
    return list(value)


def _contains_forbidden_feature_field(value: Any) -> bool:
    if isinstance(value, dict):
        return bool(FORBIDDEN_FEATURE_FIELDS.intersection(value)) or any(
            _contains_forbidden_feature_field(child) for child in value.values()
        )
    if isinstance(value, list):
        return any(_contains_forbidden_feature_field(child) for child in value)
    return False


def _load_split(
    temporal_split_path: Path,
) -> tuple[dict[str, list[str]], dict[str, str], dict[str, Any]]:
    try:
        temporal_split = json.loads(temporal_split_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise StatisticalModelError("invalid temporal split manifest") from error
    if temporal_split.get("manifest_version") != 1:
        raise StatisticalModelError("unsupported temporal split manifest version")

    series_ids_by_split: dict[str, list[str]] = {}
    split_by_series_id: dict[str, str] = {}
    for split_name in DEVELOPMENT_SPLITS:
        split = temporal_split.get(split_name)
        if not isinstance(split, dict):
            raise StatisticalModelError(f"missing development split: {split_name}")
        series_ids = _require_series_ids(split_name, split.get("series_ids"))
        for series_id in series_ids:
            if series_id in split_by_series_id:
                raise StatisticalModelError(
                    f"series_id appears in multiple development splits: {series_id}"
                )
            split_by_series_id[series_id] = split_name
        series_ids_by_split[split_name] = series_ids

    final_test = temporal_split.get("final_test")
    if not isinstance(final_test, dict):
        raise StatisticalModelError("missing sealed final_test split")
    if "series_ids" in final_test:
        raise StatisticalModelError("sealed final_test must not expose series_ids")
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
        raise StatisticalModelError("sealed final_test contract is invalid")
    return (
        series_ids_by_split,
        split_by_series_id,
        {
            "series_count": final_count,
            "membership_sha256": membership_sha256,
            "access_policy": FINAL_TEST_ACCESS_POLICY,
        },
    )


def _read_development_labels(
    series_result_path: Path,
    split_by_series_id: dict[str, str],
    expected_total: int,
) -> tuple[dict[str, dict[str, Any]], set[str]]:
    development: dict[str, dict[str, Any]] = {}
    all_series_ids: set[str] = set()
    try:
        with series_result_path.open("r", encoding="utf-8-sig", newline="") as source:
            reader = csv.DictReader(source)
            required = {
                "series_id",
                "competition_id",
                "region",
                "patch",
                "scheduled_start_utc",
                "best_of",
                "team_1_id",
                "team_2_id",
                "winner_team_id",
            }
            if reader.fieldnames is None or not required.issubset(reader.fieldnames):
                raise StatisticalModelError(
                    "Series Result CSV is missing MODEL-004 fields"
                )
            for row in reader:
                series_id = (row.get("series_id") or "").strip()
                if not series_id or series_id in all_series_ids:
                    raise StatisticalModelError(
                        f"empty or duplicate Series Result series_id: {series_id}"
                    )
                all_series_ids.add(series_id)
                split_name = split_by_series_id.get(series_id)
                if split_name is None:
                    # 未公开成员只参与全集完整性检查，不推断其 final-test 归属。
                    continue
                team_one = (row.get("team_1_id") or "").strip()
                team_two = (row.get("team_2_id") or "").strip()
                winner = (row.get("winner_team_id") or "").strip()
                if not team_one or not team_two or team_one == team_two:
                    raise StatisticalModelError(f"invalid teams: {series_id}")
                if winner == team_one:
                    actual = 1
                elif winner == team_two:
                    actual = 0
                else:
                    raise StatisticalModelError(
                        f"winner does not match either team: {series_id}"
                    )
                try:
                    best_of = int((row.get("best_of") or "").strip())
                except ValueError as error:
                    raise StatisticalModelError(
                        f"invalid best_of: {series_id}"
                    ) from error
                if best_of not in (3, 5):
                    raise StatisticalModelError(
                        f"MODEL-004 only supports BO3/BO5: {series_id}"
                    )
                development[series_id] = {
                    "split": split_name,
                    "competition_id": (row.get("competition_id") or "").strip(),
                    "region": (row.get("region") or "").strip(),
                    "patch": (row.get("patch") or "").strip(),
                    "scheduled_start_utc": _parse_utc(
                        (row.get("scheduled_start_utc") or "").strip(),
                        "scheduled_start_utc",
                    ),
                    "best_of": best_of,
                    "team_ids": (team_one, team_two),
                    "actual_team_1_win": actual,
                }
                if not all(
                    development[series_id][field]
                    for field in ("competition_id", "region", "patch")
                ):
                    raise StatisticalModelError(
                        f"missing pre-match Series Result field: {series_id}"
                    )
    except OSError as error:
        raise StatisticalModelError("failed to read Series Result CSV") from error
    if len(all_series_ids) != expected_total:
        raise StatisticalModelError(
            "Series Result count does not match development plus sealed final count: "
            f"actual={len(all_series_ids)}, expected={expected_total}"
        )
    missing = set(split_by_series_id).difference(development)
    if missing:
        raise StatisticalModelError(
            f"development split references missing Series Result: {min(missing)}"
        )
    return development, all_series_ids


def _validate_source_time(
    value: Any, snapshot_at: datetime, field: str, required: bool
) -> None:
    if value is None:
        if required:
            raise StatisticalModelError(f"missing source time for available {field}")
        return
    if not isinstance(value, str) or not value:
        raise StatisticalModelError(f"invalid source time for {field}")
    if _parse_utc(value, f"{field}.source_latest_at_utc") > snapshot_at:
        raise StatisticalModelError(f"feature source time exceeds snapshot: {field}")


def _read_count_feature(team: dict[str, Any], field: str, snapshot_at: datetime) -> int:
    feature = team.get(field)
    if not isinstance(feature, dict):
        raise StatisticalModelError(f"missing team feature: {field}")
    count = _require_nonnegative_int(feature.get("value"), f"{field}.value")
    _validate_source_time(
        feature.get("source_latest_at_utc"), snapshot_at, field, count > 0
    )
    if count == 0 and feature.get("source_latest_at_utc") is not None:
        raise StatisticalModelError(f"zero {field} must not have a source time")
    return count


def _read_rate_feature(
    team: dict[str, Any],
    field: str,
    expected_denominator: int,
    snapshot_at: datetime,
) -> float:
    feature = team.get(field)
    if not isinstance(feature, dict):
        raise StatisticalModelError(f"missing team feature: {field}")
    numerator = _require_nonnegative_int(feature.get("numerator"), f"{field}.numerator")
    denominator = _require_nonnegative_int(
        feature.get("denominator"), f"{field}.denominator"
    )
    if denominator != expected_denominator or numerator > denominator:
        raise StatisticalModelError(f"invalid numerator/denominator for {field}")
    _validate_source_time(
        feature.get("source_latest_at_utc"), snapshot_at, field, denominator > 0
    )
    if denominator == 0:
        if numerator != 0 or feature.get("source_latest_at_utc") is not None:
            raise StatisticalModelError(f"unavailable {field} must remain explicit")
        # 中性 0.5 只用于模型输入，availability 差值同时保留缺失语义；不是伪造历史胜率。
        return NEUTRAL_WIN_RATE
    return numerator / denominator


def _read_team_form(team: Any, snapshot_at: datetime) -> TeamForm:
    if not isinstance(team, dict):
        raise StatisticalModelError("team_features entries must be objects")
    if _contains_forbidden_feature_field(team):
        raise StatisticalModelError("team feature contains a forbidden target field")
    team_id = team.get("team_id")
    if not isinstance(team_id, str) or not team_id:
        raise StatisticalModelError("team feature is missing team_id")

    prior_series_count = _read_count_feature(team, "prior_series_count", snapshot_at)
    prior_game_count = _read_count_feature(team, "prior_game_count", snapshot_at)
    same_patch_count = _read_count_feature(team, "same_patch_series_count", snapshot_at)
    prior_series_rate = _read_rate_feature(
        team,
        "prior_series_win_rate",
        prior_series_count,
        snapshot_at,
    )
    prior_game_rate = _read_rate_feature(
        team,
        "prior_game_win_rate",
        prior_game_count,
        snapshot_at,
    )
    same_patch_rate = _read_rate_feature(
        team,
        "same_patch_series_win_rate",
        same_patch_count,
        snapshot_at,
    )

    rest = team.get("rest_minutes")
    if not isinstance(rest, dict):
        raise StatisticalModelError("missing team feature: rest_minutes")
    rest_value = rest.get("value")
    if prior_series_count == 0:
        if rest_value is not None or rest.get("source_latest_at_utc") is not None:
            raise StatisticalModelError(
                "rest_minutes must remain unavailable without prior history"
            )
        parsed_rest = None
    else:
        parsed_rest = _require_nonnegative_int(rest_value, "rest_minutes.value")
        _validate_source_time(
            rest.get("source_latest_at_utc"),
            snapshot_at,
            "rest_minutes",
            True,
        )
    return TeamForm(
        team_id=team_id,
        prior_series_count=prior_series_count,
        prior_series_win_rate=prior_series_rate,
        prior_game_count=prior_game_count,
        prior_game_win_rate=prior_game_rate,
        same_patch_series_count=same_patch_count,
        same_patch_series_win_rate=same_patch_rate,
        rest_minutes=parsed_rest,
    )


def build_feature_vector(
    team_one: TeamForm, team_two: TeamForm, best_of: int
) -> tuple[float, ...]:
    if best_of not in (3, 5):
        raise StatisticalModelError("feature vector only supports BO3/BO5")
    rest_one = (
        0.0 if team_one.rest_minutes is None else math.log1p(team_one.rest_minutes)
    )
    rest_two = (
        0.0 if team_two.rest_minutes is None else math.log1p(team_two.rest_minutes)
    )
    values = (
        team_one.prior_series_win_rate - team_two.prior_series_win_rate,
        team_one.prior_game_win_rate - team_two.prior_game_win_rate,
        team_one.same_patch_series_win_rate - team_two.same_patch_series_win_rate,
        math.log1p(team_one.prior_series_count)
        - math.log1p(team_two.prior_series_count),
        math.log1p(team_one.prior_game_count) - math.log1p(team_two.prior_game_count),
        math.log1p(team_one.same_patch_series_count)
        - math.log1p(team_two.same_patch_series_count),
        rest_one - rest_two,
        float(team_one.prior_history_available - team_two.prior_history_available),
        float(
            team_one.same_patch_history_available
            - team_two.same_patch_history_available
        ),
        float(best_of == 5),
    )
    if len(values) != len(FEATURE_NAMES) or not all(
        math.isfinite(value) for value in values
    ):
        raise StatisticalModelError("feature vector is invalid")
    return values


def _read_development_features(
    feature_snapshot_path: Path,
    development_labels: dict[str, dict[str, Any]],
    all_series_ids: set[str],
    expected_total: int,
) -> list[StatisticalSeries]:
    try:
        payload = json.loads(feature_snapshot_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise StatisticalModelError("invalid Feature Snapshot JSON") from error
    if not isinstance(payload, list):
        raise StatisticalModelError("Feature Snapshot root must be a list")
    if len(payload) != expected_total:
        raise StatisticalModelError(
            "Feature Snapshot count does not match development plus sealed final count"
        )

    all_feature_ids: set[str] = set()
    development: list[StatisticalSeries] = []
    for row in payload:
        if not isinstance(row, dict):
            raise StatisticalModelError("Feature Snapshot rows must be objects")
        if _contains_forbidden_feature_field(row):
            raise StatisticalModelError(
                "Feature Snapshot contains a forbidden target field"
            )
        series_id = row.get("series_id")
        if (
            not isinstance(series_id, str)
            or not series_id
            or series_id in all_feature_ids
        ):
            raise StatisticalModelError(
                f"empty or duplicate Feature Snapshot series_id: {series_id}"
            )
        all_feature_ids.add(series_id)
        label = development_labels.get(series_id)
        if label is None:
            continue

        scheduled_start = _parse_utc(
            str(row.get("scheduled_start_utc", "")), "scheduled_start_utc"
        )
        snapshot_at = _parse_utc(str(row.get("snapshot_at_utc", "")), "snapshot_at_utc")
        if scheduled_start != label["scheduled_start_utc"]:
            raise StatisticalModelError(
                f"Series Result and Feature Snapshot time differ: {series_id}"
            )
        if snapshot_at != scheduled_start - timedelta(minutes=SNAPSHOT_LEAD_MINUTES):
            raise StatisticalModelError(
                f"Feature Snapshot must use T-15m cutoff: {series_id}"
            )
        for field in ("competition_id", "region", "patch"):
            if row.get(field) != label[field]:
                raise StatisticalModelError(
                    f"Series Result and Feature Snapshot {field} differ: {series_id}"
                )
        best_of = row.get("best_of")
        if best_of != label["best_of"]:
            raise StatisticalModelError(
                f"Series Result and Feature Snapshot best_of differ: {series_id}"
            )
        team_features = row.get("team_features")
        if not isinstance(team_features, list) or len(team_features) != 2:
            raise StatisticalModelError(
                f"Feature Snapshot must contain exactly two teams: {series_id}"
            )
        teams_by_id: dict[str, TeamForm] = {}
        for team in team_features:
            parsed = _read_team_form(team, snapshot_at)
            if parsed.team_id in teams_by_id:
                raise StatisticalModelError(
                    f"duplicate team feature: series_id={series_id}, team_id={parsed.team_id}"
                )
            teams_by_id[parsed.team_id] = parsed
        team_ids = label["team_ids"]
        if set(teams_by_id) != set(team_ids):
            raise StatisticalModelError(
                f"Series Result and Feature Snapshot teams differ: {series_id}"
            )
        development.append(
            StatisticalSeries(
                series_id=series_id,
                split=label["split"],
                scheduled_start_utc=scheduled_start,
                team_ids=team_ids,
                feature_values=build_feature_vector(
                    teams_by_id[team_ids[0]], teams_by_id[team_ids[1]], best_of
                ),
                actual_team_1_win=label["actual_team_1_win"],
            )
        )

    if all_feature_ids != all_series_ids:
        raise StatisticalModelError(
            "Series Result and Feature Snapshot full membership differ"
        )
    missing = set(development_labels).difference(item.series_id for item in development)
    if missing:
        raise StatisticalModelError(
            f"development split references missing Feature Snapshot: {min(missing)}"
        )
    development.sort(key=lambda item: (item.scheduled_start_utc, item.series_id))
    return development


def load_statistical_data(
    series_result_path: Path,
    feature_snapshot_path: Path,
    temporal_split_path: Path,
) -> LoadedStatisticalData:
    series_ids_by_split, split_by_series_id, final_test = _load_split(
        temporal_split_path
    )
    expected_total = len(split_by_series_id) + int(final_test["series_count"])
    labels, all_series_ids = _read_development_labels(
        series_result_path, split_by_series_id, expected_total
    )
    series = _read_development_features(
        feature_snapshot_path, labels, all_series_ids, expected_total
    )
    actual_counts = {
        split_name: sum(item.split == split_name for item in series)
        for split_name in DEVELOPMENT_SPLITS
    }
    for split_name in DEVELOPMENT_SPLITS:
        if actual_counts[split_name] != len(series_ids_by_split[split_name]):
            raise StatisticalModelError(
                f"development feature count differs for split: {split_name}"
            )
    return LoadedStatisticalData(
        series=series,
        series_ids_by_split=series_ids_by_split,
        final_test=final_test,
        total_series_count=expected_total,
    )


def fit_statistical_model(series: list[StatisticalSeries]) -> FittedStatisticalModel:
    train = [item for item in series if item.split == "train"]
    if not train:
        raise StatisticalModelError("train split must not be empty")
    features = np.asarray([item.feature_values for item in train], dtype=np.float64)
    labels = np.asarray([item.actual_team_1_win for item in train], dtype=np.uint8)
    if sorted(np.unique(labels).tolist()) != [0, 1]:
        raise StatisticalModelError("train split must contain both binary classes")

    # StandardScaler 与 LogisticRegression 都只在 train 上拟合；后续 split 只调用 predict_proba。
    pipeline = Pipeline(
        [
            ("scaler", StandardScaler()),
            (
                "classifier",
                LogisticRegression(
                    l1_ratio=0.0,
                    C=MODEL_C,
                    solver="liblinear",
                    random_state=RANDOM_SEED,
                    max_iter=MAX_ITERATIONS,
                    tol=TOLERANCE,
                ),
            ),
        ]
    )
    pipeline.fit(features, labels)
    scaler = pipeline.named_steps["scaler"]
    classifier = pipeline.named_steps["classifier"]
    if classifier.classes_.tolist() != [0, 1]:
        raise StatisticalModelError("classifier binary class order is invalid")
    iterations = int(classifier.n_iter_[0])
    if iterations >= MAX_ITERATIONS:
        raise StatisticalModelError("LogisticRegression did not converge")

    standardized = classifier.coef_[0].astype(np.float64)
    scale = scaler.scale_.astype(np.float64)
    mean = scaler.mean_.astype(np.float64)
    if np.any(scale <= 0.0):
        raise StatisticalModelError("training feature scale must be positive")
    raw_coefficients = standardized / scale
    standardized_intercept = float(classifier.intercept_[0])
    raw_intercept = standardized_intercept - float(np.sum(standardized * mean / scale))
    return FittedStatisticalModel(
        pipeline=pipeline,
        standardized_coefficients=tuple(float(value) for value in standardized),
        standardized_intercept=standardized_intercept,
        raw_coefficients=tuple(float(value) for value in raw_coefficients),
        raw_intercept=raw_intercept,
        scaler_mean=tuple(float(value) for value in mean),
        scaler_scale=tuple(float(value) for value in scale),
        iterations=iterations,
    )


def predict_probability(
    fitted: FittedStatisticalModel, series: list[StatisticalSeries]
) -> np.ndarray:
    if not series:
        raise StatisticalModelError("prediction series must not be empty")
    features = np.asarray([item.feature_values for item in series], dtype=np.float64)
    probabilities = fitted.pipeline.predict_proba(features)[:, 1].astype(np.float64)
    if np.any(~np.isfinite(probabilities)) or np.any(
        (probabilities <= 0.0) | (probabilities >= 1.0)
    ):
        raise StatisticalModelError("predicted probabilities must be finite and open")
    return probabilities


def evaluate_split(
    series: list[StatisticalSeries], probabilities: np.ndarray, split_name: str
) -> dict[str, int | float]:
    indexes = [index for index, item in enumerate(series) if item.split == split_name]
    if not indexes:
        raise StatisticalModelError(f"evaluation split must not be empty: {split_name}")
    labels = np.asarray(
        [series[index].actual_team_1_win for index in indexes], dtype=np.uint8
    )
    selected = probabilities[indexes]
    return {
        "series_count": len(indexes),
        "team_1_win_count": int(labels.sum()),
        "observed_team_1_win_rate": float(labels.mean()),
        "mean_raw_probability_team_1_win": float(selected.mean()),
        "brier_score": float(
            brier_score_loss(labels, selected, pos_label=1, scale_by_half=True)
        ),
        "log_loss": float(
            log_loss(
                labels,
                np.column_stack((1.0 - selected, selected)),
                labels=[0, 1],
            )
        ),
    }


def build_artifact(
    loaded: LoadedStatisticalData,
    inputs: dict[str, dict[str, str]],
) -> dict[str, Any]:
    model_config = {
        "algorithm": "sklearn.linear_model.LogisticRegression",
        "solver": "liblinear",
        "regularization": "l2",
        "l1_ratio": 0.0,
        "C": MODEL_C,
        "fit_intercept": True,
        "max_iter": MAX_ITERATIONS,
        "tolerance": TOLERANCE,
        "random_seed": RANDOM_SEED,
        "scaler": "sklearn.preprocessing.StandardScaler",
        "feature_names": list(FEATURE_NAMES),
        "snapshot_lead_minutes": SNAPSHOT_LEAD_MINUTES,
        "unavailable_win_rate_input": NEUTRAL_WIN_RATE,
        "availability_features": [
            "prior_history_available_diff",
            "same_patch_history_available_diff",
        ],
    }
    fitted = fit_statistical_model(loaded.series)
    probabilities = predict_probability(fitted, loaded.series)

    fitted_parameters = []
    for index, feature_name in enumerate(FEATURE_NAMES):
        fitted_parameters.append(
            {
                "feature": feature_name,
                "training_mean": fitted.scaler_mean[index],
                "training_scale": fitted.scaler_scale[index],
                "standardized_coefficient": fitted.standardized_coefficients[index],
                "raw_space_coefficient": fitted.raw_coefficients[index],
            }
        )

    predictions = []
    for item, probability in zip(loaded.series, probabilities, strict=True):
        predictions.append(
            {
                "series_id": item.series_id,
                "split": item.split,
                "scheduled_start_utc": item.scheduled_start_utc.isoformat(),
                "team_ids": list(item.team_ids),
                "feature_values": dict(
                    zip(FEATURE_NAMES, item.feature_values, strict=True)
                ),
                "raw_probability_team_1_win": float(probability),
                "actual_team_1_win": item.actual_team_1_win,
            }
        )

    train_ids = loaded.series_ids_by_split["train"]
    return {
        "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "probability_model",
        "model": {
            "family": MODEL_FAMILY,
            "strategy": MODEL_STRATEGY,
            "positive_label": POSITIVE_LABEL,
            "uses_features": True,
            "uses_market_data": False,
            "training_split": "train",
            "probability_status": "raw_uncalibrated",
            "config": model_config,
            "config_sha256": _sha256_json(model_config),
        },
        "runtime": {
            "python_version": platform.python_version(),
            "numpy_version": np.__version__,
            "scikit_learn_version": sklearn.__version__,
        },
        "inputs": inputs,
        "training": {
            "series_count": len(train_ids),
            "team_1_win_count": sum(
                item.actual_team_1_win
                for item in loaded.series
                if item.split == "train"
            ),
            "series_membership_sha256": _sha256_membership(train_ids),
            "optimizer_iterations": fitted.iterations,
        },
        "fitted_parameters": {
            "standardized_intercept": fitted.standardized_intercept,
            "raw_space_intercept": fitted.raw_intercept,
            "features": fitted_parameters,
        },
        "development_predictions": predictions,
        "development_evaluation": {
            split_name: evaluate_split(loaded.series, probabilities, split_name)
            for split_name in DEVELOPMENT_SPLITS
        },
        "calibration": {
            "status": "not_applied_in_model004",
            "next_task": "MODEL-005",
        },
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
        description="Build the MODEL-004 train-only interpretable statistical model."
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
    loaded = load_statistical_data(
        arguments.series_results.resolve(),
        arguments.feature_snapshots.resolve(),
        arguments.temporal_split.resolve(),
    )
    artifact = build_artifact(loaded, inputs)
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(
        json.dumps(artifact, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )


if __name__ == "__main__":
    try:
        main()
    except (StatisticalModelError, OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"statistical model build failed: {error}") from error
