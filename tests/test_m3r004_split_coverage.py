from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from research.m3r004_split_coverage import (
    build_coverage,
    membership_sha256,
    sha256_path,
)


class RecoverySplitCoverageTests(unittest.TestCase):
    def test_report_recomputes_seal_without_publishing_final_ids(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            features = root / "features.json"
            split = root / "split.json"
            rows = [
                self._row("train", "2026-01-10T00:00:00Z"),
                self._row("validation", "2026-02-10T00:00:00Z"),
                self._row("calibration", "2026-03-10T00:00:00Z"),
                self._row("secret-final", "2026-04-10T00:00:00Z"),
            ]
            features.write_text(json.dumps(rows), encoding="utf-8")
            manifest = {
                "source_dataset_sha256": sha256_path(features),
                "train": self._development(
                    "2026-01-01T00:00:00Z", "2026-02-01T00:00:00Z", "train"
                ),
                "validation": self._development(
                    "2026-02-01T00:00:00Z", "2026-03-01T00:00:00Z", "validation"
                ),
                "calibration": self._development(
                    "2026-03-01T00:00:00Z", "2026-04-01T00:00:00Z", "calibration"
                ),
                "final_test": {
                    "window": {
                        "start_utc": "2026-04-01T00:00:00Z",
                        "end_utc": "2026-05-01T00:00:00Z",
                    },
                    "series_count": 1,
                    "membership_sha256": membership_sha256([rows[-1]]),
                },
                "recovery": {
                    "retired_final_window": {
                        "start_utc": "2025-04-01T00:00:00Z",
                        "end_utc": "2025-05-01T00:00:00Z",
                    },
                    "retired_final_series_count": 1,
                    "retired_final_membership_sha256": "a" * 64,
                    "member_overlap_count": 0,
                    "temporal_overlap_count": 0,
                },
            }
            split.write_text(json.dumps(manifest), encoding="utf-8")

            serialized = json.dumps(build_coverage(features, split))
            self.assertNotIn("secret-final", serialized)
            self.assertNotIn("series_ids", serialized)
            self.assertEqual(
                json.loads(serialized)["splits"]["final_test"]["series_count"], 1
            )

    @staticmethod
    def _development(start: str, end: str, series_id: str) -> dict[str, object]:
        return {
            "window": {"start_utc": start, "end_utc": end},
            "series_ids": [series_id],
        }

    @staticmethod
    def _row(series_id: str, scheduled: str) -> dict[str, object]:
        team = {
            "prior_series_count": {"value": 0, "source_latest_at_utc": None},
            "same_patch_series_count": {"value": 0, "source_latest_at_utc": None},
        }
        return {
            "series_id": series_id,
            "scheduled_start_utc": scheduled,
            "snapshot_at_utc": scheduled,
            "best_of": 3,
            "region": "Test",
            "patch": "1.0",
            "team_features": [team, team],
        }


if __name__ == "__main__":
    unittest.main()
