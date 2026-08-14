from __future__ import annotations

import unittest
from datetime import UTC, datetime, timedelta

import numpy as np

from research.model008_recovery_model import (
    FeatureRow,
    RecoverySeries,
    fit_offset_model,
    materialize_feature_lab,
    predict_offset_model,
    series_win_probability,
)


START = datetime(2026, 1, 1, tzinfo=UTC)


def config() -> dict[str, object]:
    return {
        "elo": {
            "initial_rating": 1500.0,
            "rating_scale": 400.0,
            "k_factor_per_game": 20.0,
        },
        "feature_set": {
            "minimum_team_history_games": 2,
            "feature_names": [
                "opponent_adjusted_residual_30d_diff",
                "opponent_adjusted_residual_90d_diff",
                "strength_of_schedule_90d_diff",
                "games_7d_diff",
                "games_14d_diff",
                "rest_days_capped_diff",
                "log_history_games_diff",
            ],
        },
        "residual_model": {
            "l2_penalty": 10.0,
            "max_iterations": 1000,
            "gradient_tolerance": 1e-9,
        },
    }


def series(
    series_id: str,
    start_hours: int,
    duration_hours: int,
    teams: tuple[str, str],
    scores: tuple[int, int],
) -> RecoverySeries:
    scheduled = START + timedelta(hours=start_hours)
    return RecoverySeries(
        series_id=series_id,
        split="train",
        scheduled_start_utc=scheduled,
        snapshot_at_utc=scheduled - timedelta(minutes=15),
        completed_at_utc=scheduled + timedelta(hours=duration_hours),
        region="Test",
        best_of=3,
        team_ids=teams,
        scores=scores,
        actual_team_1_win=int(scores[0] > scores[1]),
    )


class RecoveryModelTests(unittest.TestCase):
    def test_best_of_probability_is_exact_and_format_specific(self) -> None:
        self.assertAlmostEqual(series_win_probability(0.5, 3), 0.5)
        self.assertAlmostEqual(series_win_probability(0.5, 5), 0.5)
        self.assertAlmostEqual(series_win_probability(0.6, 3), 0.648)
        self.assertAlmostEqual(series_win_probability(0.6, 5), 0.68256)

    def test_unfinished_series_cannot_enter_next_t15m_snapshot(self) -> None:
        rows = [
            series("slow", 0, 4, ("a", "b"), (2, 0)),
            series("overlap", 2, 2, ("a", "c"), (2, 0)),
            series("after", 6, 2, ("a", "d"), (2, 0)),
        ]
        features = materialize_feature_lab(rows, config())
        self.assertEqual(features[1].team_history_games[0], 0)
        self.assertEqual(features[1].game_elo_probability, 0.5)
        self.assertEqual(features[2].team_history_games[0], 4)
        self.assertGreater(features[2].game_elo_probability, 0.5)
        for row in features:
            for audit in row.audit_rows:
                if audit["source_max_at"] is not None:
                    self.assertLessEqual(
                        datetime.fromisoformat(
                            audit["source_max_at"].replace("Z", "+00:00")
                        ),
                        row.series.snapshot_at_utc,
                    )

    def test_offset_fit_keeps_elo_as_fixed_input_and_returns_open_probabilities(
        self,
    ) -> None:
        base_series = series("base", 0, 1, ("a", "b"), (2, 0))
        rows = []
        for index in range(20):
            item = RecoverySeries(
                **{
                    **base_series.__dict__,
                    "series_id": f"row-{index}",
                    "actual_team_1_win": index % 2,
                }
            )
            values = (float(index % 3),) + (0.0,) * 6
            rows.append(FeatureRow(item, 0.0, 0.5, 0.5, (10, 10), values, ()))
        fitted = fit_offset_model(rows, config())
        probabilities = predict_offset_model(fitted, rows)
        self.assertTrue(np.all(np.isfinite(probabilities)))
        self.assertTrue(np.all((probabilities > 0.0) & (probabilities < 1.0)))


if __name__ == "__main__":
    unittest.main()
