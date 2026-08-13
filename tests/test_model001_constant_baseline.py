from __future__ import annotations

import csv
import json
import tempfile
import unittest
from pathlib import Path

from research.model001_constant_baseline import (
    ConstantBaselineError,
    build_artifact,
    evaluate_probability,
    load_development_data,
)

FAKE_SHA256 = "a" * 64
SERIES_INPUT = {
    "dataset_name": "lol-series-results",
    "dataset_version": "test",
    "dataset_relative_path": "data/processed/results/test/series-results.csv",
    "dataset_sha256": FAKE_SHA256,
    "manifest_relative_path": "data/processed/results/test/series-results.csv.manifest.json",
    "manifest_sha256": FAKE_SHA256,
}
SPLIT_INPUT = {
    "dataset_name": "lol-temporal-splits",
    "dataset_version": "test",
    "dataset_relative_path": "data/processed/splits/test/temporal-split-manifest.json",
    "dataset_sha256": FAKE_SHA256,
    "manifest_relative_path": "data/processed/splits/test/temporal-split-manifest.json.manifest.json",
    "manifest_sha256": FAKE_SHA256,
}


class ConstantBaselineTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary_directory.name)
        self.series_path = self.root / "series-results.csv"
        self.split_path = self.root / "temporal-split-manifest.json"

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def _write_series(self, winners: dict[str, int]) -> None:
        with self.series_path.open("w", encoding="utf-8", newline="") as output:
            writer = csv.DictWriter(
                output,
                fieldnames=["series_id", "team_1_id", "team_2_id", "winner_team_id"],
            )
            writer.writeheader()
            for series_id, winner_index in winners.items():
                writer.writerow(
                    {
                        "series_id": series_id,
                        "team_1_id": f"{series_id}-team-1",
                        "team_2_id": f"{series_id}-team-2",
                        "winner_team_id": f"{series_id}-team-{winner_index + 1}",
                    }
                )

    def _write_split(self, *, expose_final_ids: bool = False) -> None:
        final_test = {
            "window": {
                "start_utc": "2025-04-01T00:00:00Z",
                "end_utc": "2025-05-01T00:00:00Z",
            },
            "series_count": 2,
            "membership_sha256": "b" * 64,
            "access_policy": "sealed_until_model_freeze",
        }
        if expose_final_ids:
            final_test["series_ids"] = ["f1", "f2"]
        self.split_path.write_text(
            json.dumps(
                {
                    "manifest_version": 1,
                    "source_dataset_sha256": "c" * 64,
                    "train": {"window": {}, "series_ids": ["t1", "t2", "t3", "t4"]},
                    "validation": {"window": {}, "series_ids": ["v1", "v2"]},
                    "calibration": {"window": {}, "series_ids": ["c1", "c2"]},
                    "final_test": final_test,
                }
            ),
            encoding="utf-8",
        )

    def test_builds_train_prior_and_keeps_final_test_sealed(self) -> None:
        self._write_series(
            {
                "t1": 0,
                "t2": 0,
                "t3": 0,
                "t4": 1,
                "v1": 0,
                "v2": 1,
                "c1": 1,
                "c2": 1,
                "f1": 0,
                "f2": 1,
            }
        )
        self._write_split()

        loaded = load_development_data(self.series_path, self.split_path)
        artifact = build_artifact(loaded, SERIES_INPUT, SPLIT_INPUT)

        self.assertEqual(artifact["model"]["probability_team_1_win"], 0.75)
        self.assertEqual(artifact["training"]["series_count"], 4)
        self.assertEqual(
            artifact["development_evaluation"]["validation"]["series_count"], 2
        )
        self.assertEqual(
            artifact["final_test_evaluation"]["status"], "sealed_not_evaluated"
        )
        self.assertNotIn("series_ids", artifact["final_test_evaluation"])

    def test_validation_labels_do_not_change_train_probability(self) -> None:
        self._write_series(
            {
                "t1": 0,
                "t2": 1,
                "t3": 0,
                "t4": 1,
                "v1": 0,
                "v2": 0,
                "c1": 1,
                "c2": 0,
                "f1": 0,
                "f2": 1,
            }
        )
        self._write_split()
        first = build_artifact(
            load_development_data(self.series_path, self.split_path),
            SERIES_INPUT,
            SPLIT_INPUT,
        )

        self._write_series(
            {
                "t1": 0,
                "t2": 1,
                "t3": 0,
                "t4": 1,
                "v1": 1,
                "v2": 1,
                "c1": 1,
                "c2": 0,
                "f1": 0,
                "f2": 1,
            }
        )
        second = build_artifact(
            load_development_data(self.series_path, self.split_path),
            SERIES_INPUT,
            SPLIT_INPUT,
        )
        self.assertEqual(
            first["model"]["probability_team_1_win"],
            second["model"]["probability_team_1_win"],
        )

    def test_rejects_final_test_ids_in_development_manifest(self) -> None:
        self._write_series(
            {
                key: index % 2
                for index, key in enumerate(
                    ["t1", "t2", "t3", "t4", "v1", "v2", "c1", "c2", "f1", "f2"]
                )
            }
        )
        self._write_split(expose_final_ids=True)
        with self.assertRaisesRegex(
            ConstantBaselineError, "must not expose series_ids"
        ):
            load_development_data(self.series_path, self.split_path)

    def test_rejects_winner_outside_series_teams(self) -> None:
        self._write_series(
            {
                "t1": 0,
                "t2": 1,
                "t3": 0,
                "t4": 1,
                "v1": 0,
                "v2": 1,
                "c1": 0,
                "c2": 1,
                "f1": 0,
                "f2": 1,
            }
        )
        rows = list(
            csv.DictReader(self.series_path.read_text(encoding="utf-8").splitlines())
        )
        rows[0]["winner_team_id"] = "unknown-team"
        with self.series_path.open("w", encoding="utf-8", newline="") as output:
            writer = csv.DictWriter(output, fieldnames=rows[0].keys())
            writer.writeheader()
            writer.writerows(rows)
        self._write_split()
        with self.assertRaisesRegex(ConstantBaselineError, "winner does not match"):
            load_development_data(self.series_path, self.split_path)

    def test_metrics_support_single_class_development_slice(self) -> None:
        metrics = evaluate_probability([1, 1, 1], 0.75)
        self.assertAlmostEqual(metrics["brier_score"], 0.0625)
        self.assertGreater(metrics["log_loss"], 0.0)


if __name__ == "__main__":
    unittest.main()
