"""构建 MODEL-003 同一赛前信息时点的 Market Baseline。"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import platform
from collections import Counter
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any

import numpy as np
import sklearn
from sklearn.metrics import brier_score_loss, log_loss

ARTIFACT_SCHEMA_VERSION = 1
MODEL_FAMILY = "market_baseline"
MODEL_STRATEGY = "last_pre_cutoff_market_price_signal"
POSITIVE_LABEL = "team_1_win"
DEVELOPMENT_SPLITS = ("train", "validation", "calibration")
FINAL_TEST_STATUS = "sealed_not_evaluated"
FINAL_TEST_ACCESS_POLICY = "sealed_until_model_freeze"
DECISION_LEAD_MINUTES = 15
ALLOWED_SIGNAL_GRADES = {"A", "B", "C"}


class MarketBaselineError(ValueError):
    """输入或合同不满足 MODEL-003 fail-closed 约束。"""


@dataclass(frozen=True)
class MarketBaselineSeries:
    series_id: str
    split: str
    scheduled_start_utc: datetime
    decision_time_utc: datetime
    market_id: str
    review_id: str
    team_ids: tuple[str, str]
    team_1_outcome_index: int
    outcome_prices: tuple[float, float]
    outcome_last_point_utc: tuple[datetime, datetime]
    outcome_staleness_seconds: tuple[int, int]
    outcome_source_sha256: tuple[str, str]
    grade: str
    actual_team_1_win: int

    @property
    def probability_team_1_win(self) -> float:
        return self.outcome_prices[self.team_1_outcome_index]


@dataclass(frozen=True)
class LoadedMarketData:
    series: list[MarketBaselineSeries]
    series_ids_by_split: dict[str, list[str]]
    final_test: dict[str, Any]
    source_scope: dict[str, Any]


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
        raise MarketBaselineError(f"path escapes repository root: {path}") from error
    return relative.as_posix()


def _validated_dataset_reference(
    repository_root: Path,
    dataset_path: Path,
    manifest_path: Path,
    expected_dataset_name: str,
) -> dict[str, str]:
    if not dataset_path.is_file() or not manifest_path.is_file():
        raise MarketBaselineError(
            f"missing dataset or manifest: dataset={dataset_path}, manifest={manifest_path}"
        )
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise MarketBaselineError(
            f"invalid dataset manifest: {manifest_path}"
        ) from error
    dataset = manifest.get("dataset")
    output = manifest.get("output")
    if not isinstance(dataset, dict) or not isinstance(output, dict):
        raise MarketBaselineError("dataset manifest is missing dataset/output metadata")
    if dataset.get("name") != expected_dataset_name:
        raise MarketBaselineError(
            f"unexpected dataset name: expected={expected_dataset_name}, "
            f"actual={dataset.get('name')}"
        )
    relative_path = _repository_relative_path(repository_root, dataset_path)
    dataset_sha256 = _sha256_file(dataset_path)
    if output.get("relative_path") != relative_path:
        raise MarketBaselineError("dataset path does not match its manifest output")
    if output.get("sha256") != dataset_sha256:
        raise MarketBaselineError("dataset SHA-256 does not match its manifest output")
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


def _evidence_reference(repository_root: Path, path: Path) -> dict[str, str]:
    if not path.is_file():
        raise MarketBaselineError(f"missing evidence file: {path}")
    return {
        "relative_path": _repository_relative_path(repository_root, path),
        "sha256": _sha256_file(path),
    }


def _parse_utc(value: str, field: str) -> datetime:
    try:
        parsed = datetime.fromisoformat(value)
    except ValueError as error:
        raise MarketBaselineError(f"invalid {field}: {value}") from error
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        raise MarketBaselineError(f"{field} must be timezone-aware")
    return parsed.astimezone(UTC)


def _parse_bool(value: str, field: str) -> bool:
    normalized = value.strip().lower()
    if normalized == "true":
        return True
    if normalized == "false":
        return False
    raise MarketBaselineError(f"invalid boolean {field}: {value}")


def _parse_probability(value: str, field: str) -> float:
    try:
        probability = float(value)
    except ValueError as error:
        raise MarketBaselineError(f"invalid probability {field}: {value}") from error
    if not np.isfinite(probability) or not 0.0 <= probability <= 1.0:
        raise MarketBaselineError(f"probability out of range {field}: {value}")
    return probability


def _parse_nonnegative_int(value: str, field: str) -> int:
    try:
        parsed = int(value)
    except ValueError as error:
        raise MarketBaselineError(f"invalid integer {field}: {value}") from error
    if parsed < 0:
        raise MarketBaselineError(f"negative integer {field}: {value}")
    return parsed


def _require_sha256(value: str, field: str) -> str:
    normalized = value.strip().lower()
    if len(normalized) != 64 or any(
        character not in "0123456789abcdef" for character in normalized
    ):
        raise MarketBaselineError(f"invalid SHA-256 {field}: {value}")
    return normalized


def _require_series_ids(split_name: str, value: Any) -> list[str]:
    if not isinstance(value, list) or not value:
        raise MarketBaselineError(f"{split_name}.series_ids must be non-empty")
    if any(
        not isinstance(series_id, str) or not series_id.strip() for series_id in value
    ):
        raise MarketBaselineError(f"{split_name} contains an empty series_id")
    if len(value) != len(set(value)):
        raise MarketBaselineError(f"{split_name} contains duplicate series_id values")
    return list(value)


def _read_keyed_csv(
    path: Path, key_field: str, required_fields: set[str], label: str
) -> dict[str, dict[str, str]]:
    rows: dict[str, dict[str, str]] = {}
    try:
        with path.open("r", encoding="utf-8-sig", newline="") as source:
            reader = csv.DictReader(source)
            if reader.fieldnames is None or not required_fields.issubset(
                reader.fieldnames
            ):
                raise MarketBaselineError(f"{label} CSV is missing required fields")
            for row in reader:
                key = (row.get(key_field) or "").strip()
                if not key:
                    raise MarketBaselineError(f"{label} contains an empty {key_field}")
                if key in rows:
                    raise MarketBaselineError(f"duplicate {label} {key_field}: {key}")
                rows[key] = row
    except OSError as error:
        raise MarketBaselineError(f"failed to read {label} CSV") from error
    if not rows:
        raise MarketBaselineError(f"{label} CSV must not be empty")
    return rows


def _load_split(
    temporal_split_path: Path,
) -> tuple[dict[str, list[str]], dict[str, str], dict[str, Any]]:
    try:
        temporal_split = json.loads(temporal_split_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise MarketBaselineError("invalid temporal split manifest") from error
    if temporal_split.get("manifest_version") != 1:
        raise MarketBaselineError("unsupported temporal split manifest version")

    series_ids_by_split: dict[str, list[str]] = {}
    split_by_series_id: dict[str, str] = {}
    for split_name in DEVELOPMENT_SPLITS:
        split = temporal_split.get(split_name)
        if not isinstance(split, dict):
            raise MarketBaselineError(f"missing development split: {split_name}")
        series_ids = _require_series_ids(split_name, split.get("series_ids"))
        for series_id in series_ids:
            if series_id in split_by_series_id:
                raise MarketBaselineError(
                    f"series_id appears in multiple development splits: {series_id}"
                )
            split_by_series_id[series_id] = split_name
        series_ids_by_split[split_name] = series_ids

    final_test = temporal_split.get("final_test")
    if not isinstance(final_test, dict):
        raise MarketBaselineError("missing sealed final_test split")
    if "series_ids" in final_test:
        raise MarketBaselineError("sealed final_test must not expose series_ids")
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
        raise MarketBaselineError("sealed final_test contract is invalid")
    return (
        series_ids_by_split,
        split_by_series_id,
        {
            "series_count": final_count,
            "membership_sha256": membership_sha256,
            "access_policy": FINAL_TEST_ACCESS_POLICY,
        },
    )


def load_market_data(
    series_result_path: Path,
    market_link_path: Path,
    temporal_split_path: Path,
    mapping_review_path: Path,
    market_grade_path: Path,
) -> LoadedMarketData:
    series_ids_by_split, split_by_series_id, final_test = _load_split(
        temporal_split_path
    )
    expected_total = len(split_by_series_id) + int(final_test["series_count"])

    review_fields = {
        "review_id",
        "market_id",
        "gamma_outcome_0",
        "gamma_outcome_1",
        "clob_game_start_utc",
        "leaguepedia_match_id",
        "leaguepedia_start_utc",
        "start_delta_seconds",
        "expected_status",
        "manual_result",
    }
    reviews = _read_keyed_csv(
        mapping_review_path, "review_id", review_fields, "mapping review"
    )
    if len({row["market_id"].strip() for row in reviews.values()}) != len(reviews):
        raise MarketBaselineError("mapping review contains duplicate market_id values")

    grade_fields = {
        "review_id",
        "market_id",
        "mapping_status",
        "decision_time_utc",
        "history_window_start_utc",
        "fidelity_minutes",
        "outcome_0",
        "outcome_0_last_point_utc",
        "outcome_0_last_price",
        "outcome_0_staleness_seconds",
        "outcome_1",
        "outcome_1_last_point_utc",
        "outcome_1_last_price",
        "outcome_1_staleness_seconds",
        "historical_depth",
        "historical_bid_ask",
        "price_history",
        "grade",
        "outcome_0_source_sha256",
        "outcome_1_source_sha256",
    }
    grades = _read_keyed_csv(
        market_grade_path, "review_id", grade_fields, "market grade"
    )
    if set(grades) != set(reviews):
        raise MarketBaselineError("mapping review and market grade membership differ")
    for review_id, grade_row in grades.items():
        if grade_row["market_id"].strip() != reviews[review_id]["market_id"].strip():
            raise MarketBaselineError(
                f"mapping review and market grade market_id differ: {review_id}"
            )

    series_fields = {
        "series_id",
        "scheduled_start_utc",
        "team_1_id",
        "team_2_id",
        "winner_team_id",
        "mapping_evidence_id",
    }
    all_series = _read_keyed_csv(
        series_result_path, "series_id", series_fields, "Series Result"
    )
    if len(all_series) != expected_total:
        raise MarketBaselineError(
            "Series Result count does not match development plus sealed final count: "
            f"actual={len(all_series)}, expected={expected_total}"
        )

    link_fields = {
        "series_id",
        "market_id",
        "resolution_status",
        "closed",
        "outcome_0_team_id",
        "outcome_1_team_id",
        "outcome_0_price",
        "outcome_1_price",
        "winner_outcome_index",
        "mapping_evidence_id",
    }
    all_links = _read_keyed_csv(
        market_link_path, "series_id", link_fields, "Market Resolution Link"
    )
    # 本任务只接受已经构建为 linked subset 的输入，不能把 marketless 行静默丢弃。
    if len(all_links) != len(all_series):
        raise MarketBaselineError(
            "Market Resolution Link count must equal linked Series Result count"
        )

    development: list[MarketBaselineSeries] = []
    for series_id, split_name in split_by_series_id.items():
        series_row = all_series.get(series_id)
        link_row = all_links.get(series_id)
        if series_row is None or link_row is None:
            raise MarketBaselineError(f"missing linked development series: {series_id}")

        team_one = series_row["team_1_id"].strip()
        team_two = series_row["team_2_id"].strip()
        winner = series_row["winner_team_id"].strip()
        if not team_one or not team_two or team_one == team_two:
            raise MarketBaselineError(f"invalid teams for series_id={series_id}")
        if winner == team_one:
            actual = 1
        elif winner == team_two:
            actual = 0
        else:
            raise MarketBaselineError(
                f"winner does not match either team for series_id={series_id}"
            )

        if link_row["resolution_status"].strip() != "resolved" or not _parse_bool(
            link_row["closed"], "closed"
        ):
            raise MarketBaselineError(
                f"market link is not closed/resolved: {series_id}"
            )
        outcome_teams = (
            link_row["outcome_0_team_id"].strip(),
            link_row["outcome_1_team_id"].strip(),
        )
        if set(outcome_teams) != {team_one, team_two}:
            raise MarketBaselineError(
                f"market outcome teams do not match Series Result: {series_id}"
            )
        team_one_outcome_index = 0 if outcome_teams[0] == team_one else 1

        resolution_prices = (
            _parse_probability(link_row["outcome_0_price"], "outcome_0_price"),
            _parse_probability(link_row["outcome_1_price"], "outcome_1_price"),
        )
        if sorted(resolution_prices) != [0.0, 1.0]:
            raise MarketBaselineError(
                f"market resolution prices must be unique binary values: {series_id}"
            )
        winner_outcome_index = _parse_nonnegative_int(
            link_row["winner_outcome_index"], "winner_outcome_index"
        )
        if winner_outcome_index not in (0, 1):
            raise MarketBaselineError(f"invalid winner outcome index: {series_id}")
        expected_winner_index = (
            team_one_outcome_index if actual == 1 else 1 - team_one_outcome_index
        )
        if (
            winner_outcome_index != expected_winner_index
            or resolution_prices[winner_outcome_index] != 1.0
        ):
            raise MarketBaselineError(
                f"market resolution conflicts with Series Result winner: {series_id}"
            )

        evidence_id = series_row["mapping_evidence_id"].strip()
        if evidence_id != link_row[
            "mapping_evidence_id"
        ].strip() or not evidence_id.startswith("DATA-008:"):
            raise MarketBaselineError(f"invalid mapping evidence: {series_id}")
        review_id = evidence_id.removeprefix("DATA-008:")
        review_row = reviews.get(review_id)
        grade_row = grades.get(review_id)
        if review_row is None or grade_row is None:
            raise MarketBaselineError(f"missing DATA-008/009 evidence: {series_id}")
        if (
            review_row["market_id"].strip() != link_row["market_id"].strip()
            or grade_row["market_id"].strip() != link_row["market_id"].strip()
        ):
            raise MarketBaselineError(f"market_id evidence conflict: {series_id}")
        if (
            review_row["expected_status"].strip() != "Matched"
            or review_row["manual_result"].strip() != "verified_correct"
            or grade_row["mapping_status"].strip() != "Matched"
        ):
            raise MarketBaselineError(
                f"only manually verified Matched mappings are eligible: {series_id}"
            )

        scheduled_start = _parse_utc(
            series_row["scheduled_start_utc"], "scheduled_start_utc"
        )
        review_series_id = f"leaguepedia:{review_row['leaguepedia_match_id'].strip()}"
        leaguepedia_start = _parse_utc(
            review_row["leaguepedia_start_utc"], "leaguepedia_start_utc"
        )
        clob_start = _parse_utc(
            review_row["clob_game_start_utc"], "clob_game_start_utc"
        )
        start_delta = _parse_nonnegative_int(
            review_row["start_delta_seconds"], "start_delta_seconds"
        )
        actual_start_delta = int(abs((leaguepedia_start - clob_start).total_seconds()))
        if (
            review_series_id != series_id
            or scheduled_start != leaguepedia_start
            or start_delta != actual_start_delta
            or start_delta > 300
        ):
            raise MarketBaselineError(f"event-time mapping conflict: {series_id}")

        if (
            grade_row["outcome_0"].strip() != review_row["gamma_outcome_0"].strip()
            or grade_row["outcome_1"].strip() != review_row["gamma_outcome_1"].strip()
        ):
            raise MarketBaselineError(f"outcome order evidence conflict: {series_id}")
        grade = grade_row["grade"].strip()
        if grade not in ALLOWED_SIGNAL_GRADES or not _parse_bool(
            grade_row["price_history"], "price_history"
        ):
            raise MarketBaselineError(
                f"reliable pre-cutoff market price is unavailable: {series_id}"
            )
        decision_time = _parse_utc(grade_row["decision_time_utc"], "decision_time_utc")
        if decision_time != clob_start - timedelta(minutes=DECISION_LEAD_MINUTES):
            raise MarketBaselineError(
                f"decision time is not CLOB Game Start - 15m: {series_id}"
            )
        window_start = _parse_utc(
            grade_row["history_window_start_utc"], "history_window_start_utc"
        )
        fidelity = _parse_nonnegative_int(
            grade_row["fidelity_minutes"], "fidelity_minutes"
        )
        if fidelity <= 0 or window_start >= decision_time:
            raise MarketBaselineError(f"invalid market history window: {series_id}")

        prices = (
            _parse_probability(
                grade_row["outcome_0_last_price"], "outcome_0_last_price"
            ),
            _parse_probability(
                grade_row["outcome_1_last_price"], "outcome_1_last_price"
            ),
        )
        point_times = (
            _parse_utc(
                grade_row["outcome_0_last_point_utc"],
                "outcome_0_last_point_utc",
            ),
            _parse_utc(
                grade_row["outcome_1_last_point_utc"],
                "outcome_1_last_point_utc",
            ),
        )
        staleness = (
            _parse_nonnegative_int(
                grade_row["outcome_0_staleness_seconds"],
                "outcome_0_staleness_seconds",
            ),
            _parse_nonnegative_int(
                grade_row["outcome_1_staleness_seconds"],
                "outcome_1_staleness_seconds",
            ),
        )
        for outcome_index in (0, 1):
            point_time = point_times[outcome_index]
            if point_time < window_start or point_time > decision_time:
                raise MarketBaselineError(
                    f"market price point falls outside pre-cutoff window: {series_id}"
                )
            actual_staleness = int((decision_time - point_time).total_seconds())
            if staleness[outcome_index] != actual_staleness:
                raise MarketBaselineError(
                    f"market price staleness evidence conflict: {series_id}"
                )

        source_hashes = (
            _require_sha256(
                grade_row["outcome_0_source_sha256"], "outcome_0_source_sha256"
            ),
            _require_sha256(
                grade_row["outcome_1_source_sha256"], "outcome_1_source_sha256"
            ),
        )
        development.append(
            MarketBaselineSeries(
                series_id=series_id,
                split=split_name,
                scheduled_start_utc=scheduled_start,
                decision_time_utc=decision_time,
                market_id=link_row["market_id"].strip(),
                review_id=review_id,
                team_ids=(team_one, team_two),
                team_1_outcome_index=team_one_outcome_index,
                outcome_prices=prices,
                outcome_last_point_utc=point_times,
                outcome_staleness_seconds=staleness,
                outcome_source_sha256=source_hashes,
                grade=grade,
                actual_team_1_win=actual,
            )
        )

    development.sort(key=lambda item: (item.scheduled_start_utc, item.series_id))
    return LoadedMarketData(
        series=development,
        series_ids_by_split=series_ids_by_split,
        final_test=final_test,
        source_scope={
            "mapping_review_count": len(reviews),
            "mapping_status_counts": dict(
                sorted(
                    Counter(
                        row["expected_status"].strip() for row in reviews.values()
                    ).items()
                )
            ),
            "market_grade_count": len(grades),
            "market_grade_counts": dict(
                sorted(Counter(row["grade"].strip() for row in grades.values()).items())
            ),
            "linked_series_count": len(all_links),
            "development_linked_series_count": len(development),
        },
    )


def evaluate_market_series(
    series: list[MarketBaselineSeries], split_name: str
) -> dict[str, int | float]:
    selected = [item for item in series if item.split == split_name]
    if not selected:
        raise MarketBaselineError(f"no market baseline rows for split: {split_name}")
    labels = np.asarray([item.actual_team_1_win for item in selected], dtype=np.uint8)
    probabilities = np.asarray(
        [item.probability_team_1_win for item in selected], dtype=np.float64
    )
    return {
        "series_count": len(selected),
        "team_1_win_count": int(labels.sum()),
        "observed_team_1_win_rate": float(labels.mean()),
        "mean_market_probability_team_1_win": float(probabilities.mean()),
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


def build_artifact(
    loaded: LoadedMarketData,
    inputs: dict[str, dict[str, str]],
) -> dict[str, Any]:
    model_config = {
        "decision_lead_minutes": DECISION_LEAD_MINUTES,
        "eligible_mapping_status": "Matched",
        "price_history_required": True,
        "point_selection": "last point per outcome at or before shared cutoff",
        "outcome_alignment": "explicit Market Resolution Link outcome order",
        "normalization": "none; preserve audited source p",
    }
    predictions = []
    for item in loaded.series:
        team_one_price = item.probability_team_1_win
        counterpart_index = 1 - item.team_1_outcome_index
        predictions.append(
            {
                "series_id": item.series_id,
                "split": item.split,
                "scheduled_start_utc": item.scheduled_start_utc.isoformat(),
                "decision_time_utc": item.decision_time_utc.isoformat(),
                "market_id": item.market_id,
                "mapping_evidence_id": f"DATA-008:{item.review_id}",
                "team_ids": list(item.team_ids),
                "team_1_outcome_index": item.team_1_outcome_index,
                "probability_team_1_win": team_one_price,
                "counterpart_price_signal": item.outcome_prices[counterpart_index],
                "two_sided_price_sum": sum(item.outcome_prices),
                "outcome_last_point_utc": [
                    value.isoformat() for value in item.outcome_last_point_utc
                ],
                "outcome_staleness_seconds": list(item.outcome_staleness_seconds),
                "outcome_source_sha256": list(item.outcome_source_sha256),
                "market_data_grade": item.grade,
                "actual_team_1_win": item.actual_team_1_win,
            }
        )

    return {
        "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "probability_baseline",
        "model": {
            "family": MODEL_FAMILY,
            "strategy": MODEL_STRATEGY,
            "positive_label": POSITIVE_LABEL,
            "uses_features": False,
            "uses_market_data": True,
            "fits_parameters": False,
            "config": model_config,
            "config_sha256": _sha256_json(model_config),
        },
        "probability_contract": {
            "source": "Polymarket GET /prices-history",
            "source_field": "p",
            "decision_cutoff": "CLOB Game Start - 15 minutes",
            "point_selection": "last point per outcome at or before shared cutoff",
            "outcome_alignment": "explicit Market Resolution Link outcome order",
            "normalization": "none; preserve audited source p",
            "execution_price_status": "unavailable",
            "not_equivalent_to": [
                "buy ask",
                "sell bid",
                "spread",
                "depth",
                "fee-adjusted executable price",
            ],
        },
        "runtime": {
            "python_version": platform.python_version(),
            "numpy_version": np.__version__,
            "scikit_learn_version": sklearn.__version__,
        },
        "inputs": inputs,
        "source_scope": loaded.source_scope,
        "development_predictions": predictions,
        "development_evaluation": {
            split_name: evaluate_market_series(loaded.series, split_name)
            for split_name in DEVELOPMENT_SPLITS
        },
        "comparability": {
            "constant_and_elo_corpus": "2025H1 1,778-series model corpus",
            "market_baseline_corpus": "2026 fixed reviewed linked-market subset",
            "direct_metric_comparison_allowed": False,
            "reason": "the evaluation populations and time windows differ",
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
        description="Build the MODEL-003 pre-cutoff Market Baseline artifact."
    )
    parser.add_argument("--repository-root", required=True, type=Path)
    parser.add_argument("--series-results", required=True, type=Path)
    parser.add_argument("--series-manifest", required=True, type=Path)
    parser.add_argument("--market-links", required=True, type=Path)
    parser.add_argument("--market-links-manifest", required=True, type=Path)
    parser.add_argument("--temporal-split", required=True, type=Path)
    parser.add_argument("--temporal-split-manifest", required=True, type=Path)
    parser.add_argument("--mapping-review", required=True, type=Path)
    parser.add_argument("--market-grades", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def main() -> None:
    arguments = _parse_arguments()
    repository_root = arguments.repository_root.resolve()
    inputs = {
        "series_result": _validated_dataset_reference(
            repository_root,
            arguments.series_results.resolve(),
            arguments.series_manifest.resolve(),
            "lol-series-results",
        ),
        "market_resolution_link": _validated_dataset_reference(
            repository_root,
            arguments.market_links.resolve(),
            arguments.market_links_manifest.resolve(),
            "lol-market-resolution-links",
        ),
        "temporal_split": _validated_dataset_reference(
            repository_root,
            arguments.temporal_split.resolve(),
            arguments.temporal_split_manifest.resolve(),
            "lol-temporal-splits",
        ),
        "mapping_review": _evidence_reference(
            repository_root, arguments.mapping_review.resolve()
        ),
        "historical_market_grades": _evidence_reference(
            repository_root, arguments.market_grades.resolve()
        ),
    }
    loaded = load_market_data(
        arguments.series_results.resolve(),
        arguments.market_links.resolve(),
        arguments.temporal_split.resolve(),
        arguments.mapping_review.resolve(),
        arguments.market_grades.resolve(),
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
    except (MarketBaselineError, OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"market baseline build failed: {error}") from error
