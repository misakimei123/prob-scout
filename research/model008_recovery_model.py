"""M3R-005 P0：game Elo offset、轻量 Feature Lab 与生成式系列赛概率。"""

from __future__ import annotations

import argparse
import csv
import hashlib
import heapq
import json
import math
import platform
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

import numpy as np
import scipy
from scipy.optimize import minimize
from scipy.special import expit
from sklearn.metrics import brier_score_loss, log_loss

DEVELOPMENT_SPLITS = ("train", "validation", "calibration")
FINAL_TEST_ACCESS_POLICY = "sealed_until_model_freeze"
FINAL_TEST_STATUS = "sealed_not_evaluated"


class RecoveryModelError(ValueError):
    """输入、时间或模型合同不满足 M3R-005 fail-closed 约束。"""


@dataclass(frozen=True)
class RecoverySeries:
    series_id: str
    split: str
    scheduled_start_utc: datetime
    snapshot_at_utc: datetime
    completed_at_utc: datetime
    region: str
    best_of: int
    team_ids: tuple[str, str]
    scores: tuple[int, int]
    actual_team_1_win: int

    @property
    def cell(self) -> str:
        return f"{self.region}|BO{self.best_of}"


@dataclass(frozen=True)
class TeamObservation:
    completed_at_utc: datetime
    games: int
    game_residual: float
    opponent_pregame_rating: float


@dataclass(frozen=True)
class FeatureRow:
    series: RecoverySeries
    game_elo_logit: float
    game_elo_probability: float
    elo_series_probability: float
    team_history_games: tuple[int, int]
    feature_values: tuple[float, ...]
    audit_rows: tuple[dict[str, Any], ...]


@dataclass(frozen=True)
class FittedOffsetModel:
    means: np.ndarray
    scales: np.ndarray
    parameters: np.ndarray
    iterations: int
    objective: float


def parse_utc(value: Any, field: str) -> datetime:
    if not isinstance(value, str) or not value:
        raise RecoveryModelError(f"missing {field}")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise RecoveryModelError(f"invalid {field}: {value}") from error
    if parsed.utcoffset() is None:
        raise RecoveryModelError(f"{field} must be timezone-aware")
    return parsed.astimezone(UTC)


def iso(value: datetime) -> str:
    return value.astimezone(UTC).isoformat().replace("+00:00", "Z")


def sha256_path(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_json(value: Any) -> str:
    encoded = json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode()
    return hashlib.sha256(encoded).hexdigest()


def repository_relative(root: Path, path: Path) -> str:
    try:
        return path.resolve().relative_to(root.resolve()).as_posix()
    except ValueError as error:
        raise RecoveryModelError(f"path escapes repository root: {path}") from error


def validated_dataset_reference(
    root: Path, path: Path, manifest_path: Path, expected_name: str
) -> dict[str, str]:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
    output = manifest.get("output", {})
    dataset = manifest.get("dataset", {})
    digest = sha256_path(path)
    relative = repository_relative(root, path)
    if (
        dataset.get("name") != expected_name
        or output.get("relative_path") != relative
        or output.get("sha256") != digest
    ):
        raise RecoveryModelError(f"invalid upstream dataset lineage: {expected_name}")
    return {
        "dataset_name": expected_name,
        "dataset_version": str(dataset.get("version", "")),
        "dataset_relative_path": relative,
        "dataset_sha256": digest,
        "manifest_relative_path": repository_relative(root, manifest_path),
        "manifest_sha256": sha256_path(manifest_path),
    }


def game_elo_probability(
    team_one_rating: float, team_two_rating: float, rating_scale: float
) -> float:
    return 1.0 / (1.0 + 10.0 ** ((team_two_rating - team_one_rating) / rating_scale))


def logit(probability: float) -> float:
    clipped = min(max(probability, 1e-12), 1.0 - 1e-12)
    return math.log(clipped / (1.0 - clipped))


def series_win_probability(game_probability: float | np.ndarray, best_of: Any) -> Any:
    """固定逐局概率下的 BO3/BO5 精确动态规划闭式结果。"""

    probability = np.asarray(game_probability, dtype=np.float64)
    formats = np.asarray(best_of)
    bo3 = probability**2 * (3.0 - 2.0 * probability)
    bo5 = probability**3 * (10.0 - 15.0 * probability + 6.0 * probability**2)
    result = np.where(formats == 3, bo3, np.where(formats == 5, bo5, np.nan))
    if np.any(~np.isfinite(result)):
        raise RecoveryModelError("best_of must be 3 or 5")
    return float(result) if result.ndim == 0 else result


def series_probability_derivative(
    game_probability: np.ndarray, best_of: np.ndarray
) -> np.ndarray:
    bo3 = 6.0 * game_probability * (1.0 - game_probability)
    bo5 = 30.0 * game_probability**2 * (1.0 - game_probability) ** 2
    result = np.where(best_of == 3, bo3, np.where(best_of == 5, bo5, np.nan))
    if np.any(~np.isfinite(result)):
        raise RecoveryModelError("best_of must be 3 or 5")
    return result


def load_development_series(
    series_path: Path,
    feature_path: Path,
    candidate_audit_path: Path,
    split_path: Path,
) -> tuple[list[RecoverySeries], dict[str, Any]]:
    split = json.loads(split_path.read_text(encoding="utf-8-sig"))
    final = split.get("final_test")
    recovery = split.get("recovery")
    if (
        not isinstance(final, dict)
        or "series_ids" in final
        or final.get("access_policy") != FINAL_TEST_ACCESS_POLICY
        or not recovery
        or recovery.get("member_overlap_count") != 0
        or recovery.get("temporal_overlap_count") != 0
    ):
        raise RecoveryModelError("invalid recovery split or sealed Final contract")

    split_by_id: dict[str, str] = {}
    for name in DEVELOPMENT_SPLITS:
        ids = split.get(name, {}).get("series_ids")
        if not isinstance(ids, list) or not ids:
            raise RecoveryModelError(f"missing development membership: {name}")
        for series_id in ids:
            if series_id in split_by_id:
                raise RecoveryModelError(f"duplicate development member: {series_id}")
            split_by_id[series_id] = name

    snapshot_by_id: dict[str, tuple[datetime, datetime]] = {}
    for raw in json.loads(feature_path.read_text(encoding="utf-8")):
        series_id = raw.get("series_id")
        if series_id not in split_by_id:
            continue
        scheduled = parse_utc(raw.get("scheduled_start_utc"), "scheduled_start_utc")
        snapshot = parse_utc(raw.get("snapshot_at_utc"), "snapshot_at_utc")
        if snapshot != scheduled - timedelta(minutes=15):
            raise RecoveryModelError(f"snapshot is not exact T-15m: {series_id}")
        if series_id in snapshot_by_id:
            raise RecoveryModelError(f"duplicate feature snapshot: {series_id}")
        snapshot_by_id[series_id] = (scheduled, snapshot)

    candidate_by_id: dict[str, dict[str, Any]] = {}
    audit = json.loads(candidate_audit_path.read_text(encoding="utf-8"))
    for raw in audit.get("candidates", []):
        series_id = raw.get("series_id")
        if series_id in split_by_id:
            candidate_by_id[series_id] = raw

    rows: list[RecoverySeries] = []
    seen: set[str] = set()
    with series_path.open("r", encoding="utf-8-sig", newline="") as source:
        reader = csv.DictReader(source)
        for raw in reader:
            series_id = (raw.get("series_id") or "").strip()
            if series_id not in split_by_id:
                # 标准开发入口不读取非公开行的 team、score、winner 或 segment 字段。
                continue
            if series_id in seen:
                raise RecoveryModelError(f"duplicate Series Result: {series_id}")
            seen.add(series_id)
            if series_id not in snapshot_by_id or series_id not in candidate_by_id:
                raise RecoveryModelError(f"missing development evidence: {series_id}")
            scheduled, snapshot = snapshot_by_id[series_id]
            candidate = candidate_by_id[series_id]
            scores = (int(raw["team_1_score"]), int(raw["team_2_score"]))
            if list(scores) != candidate.get("scores"):
                raise RecoveryModelError(f"candidate score mismatch: {series_id}")
            best_of = int(raw["best_of"])
            wins_required = best_of // 2 + 1
            if (
                best_of not in (3, 5)
                or max(scores) != wins_required
                or min(scores) >= wins_required
            ):
                raise RecoveryModelError(f"invalid BO score: {series_id}")
            team_ids = (raw["team_1_id"].strip(), raw["team_2_id"].strip())
            winner = raw["winner_team_id"].strip()
            if winner not in team_ids or team_ids[0] == team_ids[1]:
                raise RecoveryModelError(f"invalid teams or winner: {series_id}")
            completed = parse_utc(candidate.get("completed_at_utc"), "completed_at_utc")
            if completed <= scheduled:
                raise RecoveryModelError(
                    f"completion must follow Scheduled Start: {series_id}"
                )
            rows.append(
                RecoverySeries(
                    series_id=series_id,
                    split=split_by_id[series_id],
                    scheduled_start_utc=scheduled,
                    snapshot_at_utc=snapshot,
                    completed_at_utc=completed,
                    region=raw["region"].strip(),
                    best_of=best_of,
                    team_ids=team_ids,
                    scores=scores,
                    actual_team_1_win=int(winner == team_ids[0]),
                )
            )
    if seen != set(split_by_id):
        raise RecoveryModelError("development evidence membership is incomplete")
    rows.sort(key=lambda row: (row.snapshot_at_utc, row.series_id))
    return rows, {
        "series_count": final["series_count"],
        "membership_sha256": final["membership_sha256"],
        "access_policy": final["access_policy"],
    }


def _weighted_feature(
    observations: list[TeamObservation],
    cutoff: datetime,
    half_life_days: float,
    field: str,
) -> tuple[float, int, datetime | None]:
    if not observations:
        return 0.0, 0, None
    weights = []
    values = []
    for observation in observations:
        age_days = (cutoff - observation.completed_at_utc).total_seconds() / 86400.0
        if age_days < 0.0:
            raise RecoveryModelError("future observation entered Feature Lab")
        weight = (
            math.exp(-math.log(2.0) * age_days / half_life_days) * observation.games
        )
        weights.append(weight)
        value = (
            observation.game_residual
            if field == "residual"
            else (observation.opponent_pregame_rating - 1500.0) / 400.0
        )
        values.append(value)
    return (
        float(np.average(np.asarray(values), weights=np.asarray(weights))),
        sum(observation.games for observation in observations),
        max(observation.completed_at_utc for observation in observations),
    )


def _team_features(
    observations: list[TeamObservation], cutoff: datetime, minimum_games: int
) -> tuple[dict[str, float], list[dict[str, Any]], int]:
    history_games = sum(observation.games for observation in observations)
    latest = max((item.completed_at_utc for item in observations), default=None)
    residual_30, count_30, source_30 = _weighted_feature(
        observations, cutoff, 30.0, "residual"
    )
    residual_90, count_90, source_90 = _weighted_feature(
        observations, cutoff, 90.0, "residual"
    )
    sos_90, sos_count, sos_source = _weighted_feature(
        observations, cutoff, 90.0, "opponent_rating"
    )
    games_7 = sum(
        item.games
        for item in observations
        if cutoff - item.completed_at_utc <= timedelta(days=7)
    )
    games_14 = sum(
        item.games
        for item in observations
        if cutoff - item.completed_at_utc <= timedelta(days=14)
    )
    rest = (
        min((cutoff - latest).total_seconds() / 86400.0, 30.0) / 30.0 if latest else 1.0
    )
    supported = int(history_games >= minimum_games)
    values = {
        "opponent_adjusted_residual_30d": residual_30,
        "opponent_adjusted_residual_90d": residual_90,
        "strength_of_schedule_90d": sos_90,
        "games_7d": float(games_7),
        "games_14d": float(games_14),
        "rest_days_capped": rest,
        "log_history_games": math.log1p(history_games),
        "history_supported": float(supported),
    }
    metadata = {
        "opponent_adjusted_residual_30d": (source_30, count_30),
        "opponent_adjusted_residual_90d": (source_90, count_90),
        "strength_of_schedule_90d": (sos_source, sos_count),
        "games_7d": (latest if games_7 else None, games_7),
        "games_14d": (latest if games_14 else None, games_14),
        "rest_days_capped": (latest, int(latest is not None)),
        "log_history_games": (latest, history_games),
        "history_supported": (latest, history_games),
    }
    audit = [
        {
            "feature_name": name,
            "value": values[name],
            "source_max_at": iso(metadata[name][0]) if metadata[name][0] else None,
            "input_count": metadata[name][1],
            "status": "available" if metadata[name][1] else "unavailable",
        }
        for name in values
    ]
    return values, audit, history_games


def materialize_feature_lab(
    series: list[RecoverySeries], config: dict[str, Any]
) -> list[FeatureRow]:
    elo = config["elo"]
    feature_config = config["feature_set"]
    feature_names = feature_config["feature_names"]
    minimum_games = int(feature_config["minimum_team_history_games"])
    ratings: dict[str, float] = {}
    history: dict[str, list[TeamObservation]] = defaultdict(list)
    pending: list[tuple[datetime, str, RecoverySeries, float, float, float]] = []
    output: list[FeatureRow] = []

    def apply_completion(
        event: tuple[datetime, str, RecoverySeries, float, float, float],
    ) -> None:
        # 当前证据没有逐局 winner，不能猜测局序；series 完成后只按真实总比分和赛前 p 做小局数 batch update。
        _, _, item, probability, rating_one, rating_two = event
        game_count = sum(item.scores)
        residual = item.scores[0] / game_count - probability
        history[item.team_ids[0]].append(
            TeamObservation(item.completed_at_utc, game_count, residual, rating_two)
        )
        history[item.team_ids[1]].append(
            TeamObservation(item.completed_at_utc, game_count, -residual, rating_one)
        )
        delta = float(elo["k_factor_per_game"]) * (
            item.scores[0] - game_count * probability
        )
        ratings[item.team_ids[0]] = (
            ratings.get(item.team_ids[0], float(elo["initial_rating"])) + delta
        )
        ratings[item.team_ids[1]] = (
            ratings.get(item.team_ids[1], float(elo["initial_rating"])) - delta
        )

    for item in series:
        # 只吸收完成时间不晚于当前 T-15m 的 pending series；仍在进行的比赛不得贡献 rating 或 residual。
        while pending and pending[0][0] <= item.snapshot_at_utc:
            apply_completion(heapq.heappop(pending))
        team_one, team_two = item.team_ids
        rating_one = ratings.get(team_one, float(elo["initial_rating"]))
        rating_two = ratings.get(team_two, float(elo["initial_rating"]))
        probability = game_elo_probability(
            rating_one, rating_two, float(elo["rating_scale"])
        )
        values_one, audit_one, history_one = _team_features(
            history[team_one], item.snapshot_at_utc, minimum_games
        )
        values_two, audit_two, history_two = _team_features(
            history[team_two], item.snapshot_at_utc, minimum_games
        )
        feature_values = tuple(
            values_one[name.removesuffix("_diff")]
            - values_two[name.removesuffix("_diff")]
            for name in feature_names
        )
        audits = tuple(
            {"series_id": item.series_id, "team_id": team_one, **row}
            for row in audit_one
        ) + tuple(
            {"series_id": item.series_id, "team_id": team_two, **row}
            for row in audit_two
        )
        output.append(
            FeatureRow(
                series=item,
                game_elo_logit=logit(probability),
                game_elo_probability=probability,
                elo_series_probability=series_win_probability(
                    probability, item.best_of
                ),
                team_history_games=(history_one, history_two),
                feature_values=feature_values,
                audit_rows=audits,
            )
        )
        heapq.heappush(
            pending,
            (
                item.completed_at_utc,
                item.series_id,
                item,
                probability,
                rating_one,
                rating_two,
            ),
        )
    return output


def fit_offset_model(
    rows: list[FeatureRow], config: dict[str, Any]
) -> FittedOffsetModel:
    if not rows or {row.series.actual_team_1_win for row in rows} != {0, 1}:
        raise RecoveryModelError("offset training rows need both classes")
    x = np.asarray([row.feature_values for row in rows], dtype=np.float64)
    means = x.mean(axis=0)
    scales = x.std(axis=0)
    scales[scales == 0.0] = 1.0
    standardized = (x - means) / scales
    design = np.column_stack((np.ones(len(rows)), standardized))
    offsets = np.asarray([row.game_elo_logit for row in rows])
    best_of = np.asarray([row.series.best_of for row in rows])
    labels = np.asarray(
        [row.series.actual_team_1_win for row in rows], dtype=np.float64
    )
    penalty = float(config["residual_model"]["l2_penalty"])

    def objective(parameters: np.ndarray) -> tuple[float, np.ndarray]:
        # Elo game logit 是不可训练 offset；residual 先修正逐局 p，再经 BO3/BO5 DP link 对 series label 求似然。
        game_probability = expit(offsets + design @ parameters)
        series_probability = np.clip(
            series_win_probability(game_probability, best_of), 1e-12, 1.0 - 1e-12
        )
        loss = -np.sum(
            labels * np.log(series_probability)
            + (1.0 - labels) * np.log(1.0 - series_probability)
        ) + 0.5 * penalty * float(parameters @ parameters)
        derivative = (
            (series_probability - labels)
            / (series_probability * (1.0 - series_probability))
            * series_probability_derivative(game_probability, best_of)
            * game_probability
            * (1.0 - game_probability)
        )
        gradient = design.T @ derivative + penalty * parameters
        return float(loss), gradient

    result = minimize(
        objective,
        np.zeros(design.shape[1]),
        method="L-BFGS-B",
        jac=True,
        options={
            "maxiter": int(config["residual_model"]["max_iterations"]),
            "gtol": float(config["residual_model"]["gradient_tolerance"]),
        },
    )
    if not result.success or np.any(~np.isfinite(result.x)):
        raise RecoveryModelError(f"offset optimizer failed: {result.message}")
    return FittedOffsetModel(
        means, scales, result.x, int(result.nit), float(result.fun)
    )


def predict_offset_model(
    model: FittedOffsetModel, rows: list[FeatureRow]
) -> np.ndarray:
    x = np.asarray([row.feature_values for row in rows], dtype=np.float64)
    design = np.column_stack((np.ones(len(rows)), (x - model.means) / model.scales))
    offsets = np.asarray([row.game_elo_logit for row in rows])
    game_probability = expit(offsets + design @ model.parameters)
    return series_win_probability(
        game_probability, np.asarray([row.series.best_of for row in rows])
    )


def metrics(rows: list[FeatureRow], probabilities: np.ndarray) -> dict[str, Any]:
    labels = np.asarray([row.series.actual_team_1_win for row in rows], dtype=np.uint8)
    return {
        "series_count": len(rows),
        "brier_score": float(brier_score_loss(labels, probabilities)),
        "log_loss": float(
            log_loss(
                labels,
                np.column_stack((1.0 - probabilities, probabilities)),
                labels=[0, 1],
            )
        ),
    }


def comparison_metrics(
    rows: list[FeatureRow], baseline: np.ndarray, model: np.ndarray
) -> dict[str, Any]:
    baseline_metrics = metrics(rows, baseline)
    model_metrics = metrics(rows, model)
    return {
        "series_count": len(rows),
        "elo": baseline_metrics,
        "offset_residual": model_metrics,
        "delta_model_minus_elo": {
            "brier_score": model_metrics["brier_score"]
            - baseline_metrics["brier_score"],
            "log_loss": model_metrics["log_loss"] - baseline_metrics["log_loss"],
        },
    }


def build_walk_forward(
    rows: list[FeatureRow], config: dict[str, Any]
) -> dict[str, Any]:
    minimum_games = int(config["feature_set"]["minimum_team_history_games"])
    minimum_cell = int(config["fallback"]["minimum_region_bo_train_series"])
    fold_outputs = []
    all_evaluation: list[tuple[FeatureRow, float, float, bool, str]] = []
    supported_cells_by_fold: list[set[str]] = []
    for index, raw_window in enumerate(config["walk_forward"]["evaluation_windows"], 1):
        # 每个月只用此前 Development 冻结 residual 系数；月内仅允许 Elo/Feature history 随已完成结果在线更新。
        start, end = (
            parse_utc(raw_window[0], "walk_forward.start"),
            parse_utc(raw_window[1], "walk_forward.end"),
        )
        train = [row for row in rows if row.series.snapshot_at_utc < start]
        evaluation = [row for row in rows if start <= row.series.snapshot_at_utc < end]
        cell_counts = Counter(row.series.cell for row in train)
        supported_cells = {
            cell for cell, count in cell_counts.items() if count >= minimum_cell
        }
        train_supported = [
            row
            for row in train
            if min(row.team_history_games) >= minimum_games
            and row.series.cell in supported_cells
        ]
        fitted = fit_offset_model(train_supported, config)
        candidate = predict_offset_model(fitted, evaluation)
        baseline = np.asarray([row.elo_series_probability for row in evaluation])
        support = np.asarray(
            [
                min(row.team_history_games) >= minimum_games
                and row.series.cell in supported_cells
                for row in evaluation
            ],
            dtype=bool,
        )
        team_support = np.asarray(
            [min(row.team_history_games) >= minimum_games for row in evaluation],
            dtype=bool,
        )
        cell_support = np.asarray(
            [row.series.cell in supported_cells for row in evaluation], dtype=bool
        )
        modeled = np.where(support, candidate, baseline)
        fold_name = f"fold_{index}"
        segments: dict[str, Any] = {}
        for key, selector in {
            **{
                f"region:{name}": lambda row, name=name: row.series.region == name
                for name in sorted({r.series.region for r in evaluation})
            },
            **{
                f"best_of:{bo}": lambda row, bo=bo: row.series.best_of == bo
                for bo in (3, 5)
            },
            **{
                f"cell:{cell}": lambda row, cell=cell: row.series.cell == cell
                for cell in sorted({r.series.cell for r in evaluation})
            },
        }.items():
            selected = [
                position for position, row in enumerate(evaluation) if selector(row)
            ]
            if selected:
                segments[key] = comparison_metrics(
                    [evaluation[position] for position in selected],
                    baseline[selected],
                    modeled[selected],
                )
        fold_outputs.append(
            {
                "fold": fold_name,
                "train_end_exclusive": iso(start),
                "evaluation_window": {"start": iso(start), "end": iso(end)},
                "train_series_count": len(train),
                "fit_series_count": len(train_supported),
                "evaluation": comparison_metrics(evaluation, baseline, modeled),
                "fallback_count": int((~support).sum()),
                "fallback_reasons": {
                    "team_history_only": int((~team_support & cell_support).sum()),
                    "region_bo_cell_only": int((team_support & ~cell_support).sum()),
                    "both": int((~team_support & ~cell_support).sum()),
                },
                "supported_cells": sorted(supported_cells),
                "optimizer": {
                    "iterations": fitted.iterations,
                    "objective": fitted.objective,
                },
                "fitted_parameters": {
                    "intercept": float(fitted.parameters[0]),
                    "features": [
                        {
                            "name": name,
                            "training_mean": float(fitted.means[position]),
                            "training_scale": float(fitted.scales[position]),
                            "standardized_coefficient": float(
                                fitted.parameters[position + 1]
                            ),
                        }
                        for position, name in enumerate(
                            config["feature_set"]["feature_names"]
                        )
                    ],
                },
                "segments": segments,
            }
        )
        supported_cells_by_fold.append(supported_cells)
        all_evaluation.extend(
            (row, float(base), float(model), bool(is_supported), fold_name)
            for row, base, model, is_supported in zip(
                evaluation, baseline, modeled, support, strict=True
            )
        )

    evaluation_rows = [item[0] for item in all_evaluation]
    baseline = np.asarray([item[1] for item in all_evaluation])
    modeled = np.asarray([item[2] for item in all_evaluation])
    common_cells = set.intersection(*supported_cells_by_fold)
    common_cells &= set.intersection(
        *[
            {item[0].series.cell for item in all_evaluation if item[4] == fold["fold"]}
            for fold in fold_outputs
        ]
    )
    pooled_counts = Counter(
        row.series.cell for row in evaluation_rows if row.series.cell in common_cells
    )
    total_common = sum(pooled_counts.values())
    reference_weights = (
        {cell: count / total_common for cell, count in sorted(pooled_counts.items())}
        if total_common
        else {}
    )
    standardized = []
    for fold in fold_outputs:
        brier_delta = 0.0
        log_delta = 0.0
        for cell, weight in reference_weights.items():
            delta = fold["segments"][f"cell:{cell}"]["delta_model_minus_elo"]
            brier_delta += weight * delta["brier_score"]
            log_delta += weight * delta["log_loss"]
        standardized.append(
            {
                "fold": fold["fold"],
                "common_cell_series_count": sum(
                    1
                    for item in all_evaluation
                    if item[4] == fold["fold"] and item[0].series.cell in common_cells
                ),
                "common_cell_coverage_share": sum(
                    1
                    for item in all_evaluation
                    if item[4] == fold["fold"] and item[0].series.cell in common_cells
                )
                / fold["evaluation"]["series_count"],
                "brier_delta_model_minus_elo": brier_delta,
                "log_loss_delta_model_minus_elo": log_delta,
            }
        )
    counterexamples = []
    for fold in fold_outputs:
        for segment, result in fold["segments"].items():
            delta = result["delta_model_minus_elo"]
            if delta["brier_score"] > 0.0 or delta["log_loss"] > 0.0:
                counterexamples.append(
                    {
                        "fold": fold["fold"],
                        "segment": segment,
                        **delta,
                        "series_count": result["series_count"],
                    }
                )
    return {
        "overall_natural_composition": comparison_metrics(
            evaluation_rows, baseline, modeled
        ),
        "fallback_count": sum(not item[3] for item in all_evaluation),
        "folds": fold_outputs,
        "fixed_region_bo_composition": {
            "common_cells": sorted(common_cells),
            "reference_weights": reference_weights,
            "folds": standardized,
        },
        "counterexamples": counterexamples,
        "predictions": [
            {
                "series_id": row.series.series_id,
                "fold": fold,
                "scheduled_start_utc": iso(row.series.scheduled_start_utc),
                "region": row.series.region,
                "best_of": row.series.best_of,
                "elo_series_probability": base,
                "offset_residual_probability": model,
                "fallback_to_elo": not supported,
                "actual_team_1_win": row.series.actual_team_1_win,
            }
            for row, base, model, supported, fold in all_evaluation
        ],
    }


def build_artifact(
    rows: list[RecoverySeries],
    final_test: dict[str, Any],
    config: dict[str, Any],
    inputs: dict[str, Any],
) -> dict[str, Any]:
    features = materialize_feature_lab(rows, config)
    matrix = [
        {
            "series_id": row.series.series_id,
            "split": row.series.split,
            "snapshot_at_utc": iso(row.series.snapshot_at_utc),
            "region": row.series.region,
            "best_of": row.series.best_of,
            "team_ids": list(row.series.team_ids),
            "game_elo_probability": row.game_elo_probability,
            "elo_series_probability": row.elo_series_probability,
            "team_history_games": list(row.team_history_games),
            "feature_values": dict(
                zip(
                    config["feature_set"]["feature_names"],
                    row.feature_values,
                    strict=True,
                )
            ),
            "actual_team_1_win": row.series.actual_team_1_win,
        }
        for row in features
    ]
    audit_rows = [audit for row in features for audit in row.audit_rows]
    cutoff_by_series = {
        row.series.series_id: row.series.snapshot_at_utc for row in features
    }
    if any(
        audit["source_max_at"] is not None
        and parse_utc(audit["source_max_at"], "source_max_at")
        > cutoff_by_series[audit["series_id"]]
        for audit in audit_rows
    ):
        raise RecoveryModelError("Feature Lab audit contains future source time")
    return {
        "artifact_schema_version": 1,
        "artifact_kind": "recovery_probability_model_development",
        "model": {
            "family": config["model_family"],
            "status": "development_walk_forward_not_frozen_for_final",
            "config": config,
            "config_sha256": sha256_json(config),
        },
        "runtime": {
            "python_version": platform.python_version(),
            "numpy_version": np.__version__,
            "scipy_version": scipy.__version__,
        },
        "inputs": inputs,
        "feature_lab": {
            "feature_set_status": "experimental_p0",
            "model_matrix": matrix,
            "audit_schema": [
                "series_id",
                "team_id",
                "feature_name",
                "value",
                "source_max_at",
                "input_count",
                "status",
            ],
            "audit_rows": audit_rows,
            "source_time_violation_count": 0,
        },
        "walk_forward": build_walk_forward(features, config),
        "final_test_evaluation": {
            "status": FINAL_TEST_STATUS,
            **final_test,
            "series_ids_exposed": False,
        },
        "limitations": [
            "ScoreboardGames lacks per-game winner order; Elo uses series-atomic game-count batch updates.",
            "P0 starts ratings at the recovery cohort boundary and does not consume retired Final labels.",
            "No calibration is fitted; raw generative probability is evaluated.",
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Build the M3R-005 P0 recovery model")
    parser.add_argument("--repository-root", required=True, type=Path)
    parser.add_argument("--series-results", required=True, type=Path)
    parser.add_argument("--series-manifest", required=True, type=Path)
    parser.add_argument("--feature-snapshots", required=True, type=Path)
    parser.add_argument("--feature-manifest", required=True, type=Path)
    parser.add_argument("--candidate-audit", required=True, type=Path)
    parser.add_argument("--candidate-manifest", required=True, type=Path)
    parser.add_argument("--temporal-split", required=True, type=Path)
    parser.add_argument("--temporal-split-manifest", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    root = arguments.repository_root.resolve()
    inputs = {
        "series_result": validated_dataset_reference(
            root,
            arguments.series_results,
            arguments.series_manifest,
            "lol-series-results",
        ),
        "feature_snapshot": validated_dataset_reference(
            root,
            arguments.feature_snapshots,
            arguments.feature_manifest,
            "lol-prematch-features",
        ),
        "candidate_audit": validated_dataset_reference(
            root,
            arguments.candidate_audit,
            arguments.candidate_manifest,
            "lol-historical-series-candidates",
        ),
        "temporal_split": validated_dataset_reference(
            root,
            arguments.temporal_split,
            arguments.temporal_split_manifest,
            "lol-temporal-splits",
        ),
        "config": {
            "relative_path": repository_relative(root, arguments.config),
            "sha256": sha256_path(arguments.config),
        },
    }
    config = json.loads(arguments.config.read_text(encoding="utf-8"))
    rows, final = load_development_series(
        arguments.series_results,
        arguments.feature_snapshots,
        arguments.candidate_audit,
        arguments.temporal_split,
    )
    artifact = build_artifact(rows, final, config, inputs)
    arguments.output.write_text(
        json.dumps(artifact, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
