from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from datetime import datetime
from pathlib import Path
from typing import Any


class SplitCoverageError(ValueError):
    """恢复切分或特征快照不满足可发布覆盖率合同。"""


def parse_utc(value: str) -> datetime:
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.utcoffset() is None or parsed.utcoffset().total_seconds() != 0:
        raise SplitCoverageError(f"timestamp must use UTC: {value}")
    return parsed


def sha256_path(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def membership_sha256(rows: list[dict[str, Any]]) -> str:
    hasher = hashlib.sha256()
    ordered = sorted(
        rows,
        key=lambda item: (parse_utc(item["scheduled_start_utc"]), item["series_id"]),
    )
    for row in ordered:
        # 与 Rust chrono::DateTime::to_rfc3339 的 UTC 输出保持一致。
        timestamp = parse_utc(row["scheduled_start_utc"]).isoformat(timespec="seconds")
        hasher.update(f"{timestamp}\t{row['series_id']}\n".encode())
    return hasher.hexdigest()


def _aggregate(rows: list[dict[str, Any]]) -> dict[str, Any]:
    teams = [team for row in rows for team in row["team_features"]]
    source_checks = 0
    source_time_violations = 0
    for row in rows:
        cutoff = parse_utc(row["snapshot_at_utc"])
        for team in row["team_features"]:
            for value in team.values():
                if not isinstance(value, dict) or not value.get("source_latest_at_utc"):
                    continue
                source_checks += 1
                if parse_utc(value["source_latest_at_utc"]) >= cutoff:
                    source_time_violations += 1

    return {
        "series_count": len(rows),
        "best_of": dict(sorted(Counter(str(row["best_of"]) for row in rows).items())),
        "regions": dict(sorted(Counter(row["region"] for row in rows).items())),
        "patches": dict(sorted(Counter(row["patch"] for row in rows).items())),
        "missingness": {
            "prior_history_unavailable_team_sides": sum(
                team["prior_series_count"]["value"] == 0 for team in teams
            ),
            "same_patch_unavailable_team_sides": sum(
                team["same_patch_series_count"]["value"] == 0 for team in teams
            ),
            "team_side_count": len(teams),
        },
        "source_time_checks": source_checks,
        "source_time_violations": source_time_violations,
    }


def build_coverage(feature_path: Path, split_path: Path) -> dict[str, Any]:
    rows = json.loads(feature_path.read_text(encoding="utf-8"))
    manifest = json.loads(split_path.read_text(encoding="utf-8"))
    if sha256_path(feature_path) != manifest["source_dataset_sha256"]:
        raise SplitCoverageError("feature dataset hash does not match split manifest")
    recovery = manifest.get("recovery")
    if (
        not recovery
        or recovery.get("member_overlap_count") != 0
        or recovery.get("temporal_overlap_count") != 0
    ):
        raise SplitCoverageError("split manifest lacks a zero-overlap recovery proof")

    by_id = {row["series_id"]: row for row in rows}
    if len(by_id) != len(rows):
        raise SplitCoverageError("feature snapshots contain duplicate series IDs")

    output_splits: dict[str, Any] = {}
    development_ids: set[str] = set()
    for split_name in ("train", "validation", "calibration"):
        split = manifest[split_name]
        ids = split["series_ids"]
        if development_ids.intersection(ids):
            raise SplitCoverageError("development split membership overlaps")
        development_ids.update(ids)
        try:
            split_rows = [by_id[series_id] for series_id in ids]
        except KeyError as error:
            raise SplitCoverageError(
                f"development member missing from features: {error.args[0]}"
            ) from error
        start = parse_utc(split["window"]["start_utc"])
        end = parse_utc(split["window"]["end_utc"])
        if any(
            not start <= parse_utc(row["scheduled_start_utc"]) < end
            for row in split_rows
        ):
            raise SplitCoverageError(f"{split_name} member lies outside its window")
        output_splits[split_name] = _aggregate(split_rows)

    final = manifest["final_test"]
    final_start = parse_utc(final["window"]["start_utc"])
    final_end = parse_utc(final["window"]["end_utc"])
    final_rows = [
        row
        for row in rows
        if final_start <= parse_utc(row["scheduled_start_utc"]) < final_end
    ]
    if (
        len(final_rows) != final["series_count"]
        or membership_sha256(final_rows) != final["membership_sha256"]
    ):
        raise SplitCoverageError(
            "sealed final membership does not match its commitment"
        )
    if development_ids.intersection(row["series_id"] for row in final_rows):
        raise SplitCoverageError("sealed final overlaps development membership")
    output_splits["final_test"] = _aggregate(final_rows)

    return {
        "contract": "m3r004_split_coverage_v1",
        "feature_dataset_sha256": sha256_path(feature_path),
        "split_manifest_sha256": sha256_path(split_path),
        "recovery_independence": {
            "retired_final_window": recovery["retired_final_window"],
            "retired_final_series_count": recovery["retired_final_series_count"],
            "retired_final_membership_sha256": recovery[
                "retired_final_membership_sha256"
            ],
            "member_overlap_count": recovery["member_overlap_count"],
            "temporal_overlap_count": recovery["temporal_overlap_count"],
        },
        "splits": output_splits,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Build an aggregate-only M3R-004 split coverage report"
    )
    parser.add_argument("--features", type=Path, required=True)
    parser.add_argument("--split", type=Path, required=True)
    arguments = parser.parse_args()
    print(
        json.dumps(
            build_coverage(arguments.features, arguments.split),
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
