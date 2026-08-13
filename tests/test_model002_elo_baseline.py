from __future__ import annotations

import json
import tempfile
import unittest
from datetime import UTC, datetime, timedelta
from pathlib import Path

from research.model002_elo_baseline import (
    EloBaselineError,
    EloSeries,
    expected_team_one_win,
    load_elo_data,
    run_chronological_elo,
)

START = datetime(2025, 1, 1, tzinfo=UTC)


def series(
    series_id: str,
    offset_hours: int,
    team_one: str,
    team_two: str,
    winner: int,
    *,
    region: str = "Korea",
    split: str = "train",
) -> EloSeries:
    return EloSeries(
        series_id=series_id,
        split=split,
        scheduled_start_utc=START + timedelta(hours=offset_hours),
        region=region,
        team_ids=(team_one, team_two),
        actual_team_1_win=winner,
    )


class EloBaselineTests(unittest.TestCase):
    def test_first_match_uses_equal_initial_ratings(self) -> None:
        predictions, ratings = run_chronological_elo(
            [series("s1", 0, "team-a", "team-b", 1)]
        )
        self.assertEqual(predictions[0]["pre_match_ratings"], [1500.0, 1500.0])
        self.assertEqual(predictions[0]["team_seen_before"], [False, False])
        self.assertEqual(predictions[0]["probability_team_1_win"], 0.5)
        self.assertEqual(ratings["team-a"], 1510.0)
        self.assertEqual(ratings["team-b"], 1490.0)

    def test_prediction_uses_only_prior_results_then_updates(self) -> None:
        predictions, _ = run_chronological_elo(
            [
                series("s1", 0, "team-a", "team-b", 1),
                series("s2", 1, "team-a", "team-b", 0),
            ]
        )
        self.assertEqual(predictions[0]["probability_team_1_win"], 0.5)
        self.assertEqual(predictions[1]["pre_match_ratings"], [1510.0, 1490.0])
        self.assertAlmostEqual(
            predictions[1]["probability_team_1_win"],
            expected_team_one_win(1510.0, 1490.0),
        )

    def test_cross_region_match_keeps_global_team_rating(self) -> None:
        predictions, _ = run_chronological_elo(
            [
                series("s1", 0, "team-a", "team-b", 1, region="Korea"),
                series(
                    "s2",
                    1,
                    "team-a",
                    "team-c",
                    1,
                    region="International",
                ),
            ]
        )
        self.assertEqual(predictions[1]["team_seen_before"], [True, False])
        self.assertEqual(predictions[1]["pre_match_ratings"], [1510.0, 1500.0])
        self.assertGreater(predictions[1]["probability_team_1_win"], 0.5)

    def test_validation_result_cannot_change_its_own_prediction(self) -> None:
        prefix = [series("s1", 0, "team-a", "team-b", 1)]
        loss = series("s2", 1, "team-a", "team-b", 0, split="validation")
        win = series("s2", 1, "team-a", "team-b", 1, split="validation")
        loss_predictions, _ = run_chronological_elo(prefix + [loss])
        win_predictions, _ = run_chronological_elo(prefix + [win])
        self.assertEqual(
            loss_predictions[1]["probability_team_1_win"],
            win_predictions[1]["probability_team_1_win"],
        )

    def test_rejects_same_team_at_same_scheduled_start(self) -> None:
        simultaneous = [
            series("s1", 0, "team-a", "team-b", 1),
            series("s2", 0, "team-a", "team-c", 0),
        ]
        with self.assertRaisesRegex(EloBaselineError, "same scheduled start"):
            run_chronological_elo(simultaneous)

    def test_rejects_non_chronological_input(self) -> None:
        unordered = [
            series("s2", 1, "team-a", "team-b", 1),
            series("s1", 0, "team-a", "team-b", 1),
        ]
        with self.assertRaisesRegex(EloBaselineError, "sorted chronologically"):
            run_chronological_elo(unordered)

    def test_rejects_exposed_final_test_ids(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            split_path = Path(directory) / "split.json"
            split_path.write_text(
                json.dumps(
                    {
                        "manifest_version": 1,
                        "train": {"series_ids": ["t1"]},
                        "validation": {"series_ids": ["v1"]},
                        "calibration": {"series_ids": ["c1"]},
                        "final_test": {
                            "series_count": 1,
                            "membership_sha256": "a" * 64,
                            "access_policy": "sealed_until_model_freeze",
                            "series_ids": ["f1"],
                        },
                    }
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(EloBaselineError, "must not expose series_ids"):
                load_elo_data(Path(directory) / "missing.csv", split_path)


if __name__ == "__main__":
    unittest.main()
