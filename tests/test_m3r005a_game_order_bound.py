from __future__ import annotations

import unittest

from research.m3r005a_game_order_bound import (
    compute_probability_bounds,
    valid_series_sequences,
)


class RecoveryP1EvidenceDecisionTests(unittest.TestCase):
    def test_series_sequences_stop_when_winner_is_decided(self) -> None:
        self.assertEqual(len(valid_series_sequences(3)), 6)
        self.assertEqual(len(valid_series_sequences(5)), 20)

    def test_sequential_game_order_has_a_small_bounded_effect(self) -> None:
        bounds = {
            (row["previous_best_of"], row["next_best_of"]): row
            for row in compute_probability_bounds()
        }

        expected_ranges = {
            (3, 3): (0.0021, 0.0022),
            (3, 5): (0.0026, 0.0027),
            (5, 3): (0.0078, 0.0079),
            (5, 5): (0.0096, 0.0097),
        }
        for key, (lower, upper) in expected_ranges.items():
            with self.subTest(previous_best_of=key[0], next_best_of=key[1]):
                value = bounds[key]["absolute_probability_delta"]
                self.assertGreater(value, lower)
                self.assertLess(value, upper)


if __name__ == "__main__":
    unittest.main()
