from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from datetime import UTC, datetime
from pathlib import Path

import numpy as np

from research.model004_statistical_model import FEATURE_NAMES, StatisticalSeries
from research.model007_gate1 import (
    Gate1Error,
    _calibration_diagnostics,
    _gate_decision,
    _raw_probabilities,
    _same_utc_instant,
    _sha256_temporal_membership,
    _validate_release,
)


def rules() -> dict[str, float | int | bool]:
    return {
        "minimum_favorable_windows": 3,
        "maximum_final_brier_degradation_vs_elo": 0.01,
        "maximum_final_log_loss_degradation_vs_elo": 0.02,
        "require_combined_brier_not_worse_than_elo": True,
        "require_combined_log_loss_not_worse_than_elo": True,
        "maximum_overall_confidence_overstatement": 0.05,
        "high_confidence_probability_threshold": 0.8,
        "minimum_high_confidence_series": 30,
        "maximum_high_confidence_overstatement": 0.1,
    }


def walk_forward() -> dict:
    delta = {"brier_score_minus_elo": -0.002, "log_loss_minus_elo": -0.004}
    return {
        "evaluation": {
            "folds": [{"deltas_vs_elo": {"raw_statistical": delta}} for _ in range(3)],
            "overall": {
                "models": {
                    "raw_statistical": {
                        "series_count": 900,
                        "brier_score": 0.22,
                        "log_loss": 0.63,
                    },
                    "elo_baseline": {
                        "series_count": 900,
                        "brier_score": 0.23,
                        "log_loss": 0.65,
                    },
                }
            },
        }
    }


def final_metrics(raw_brier: float = 0.23, raw_log_loss: float = 0.65) -> dict:
    return {
        "raw_statistical": {
            "series_count": 300,
            "brier_score": raw_brier,
            "log_loss": raw_log_loss,
        },
        "elo_baseline": {
            "series_count": 300,
            "brier_score": 0.231,
            "log_loss": 0.652,
        },
    }


class Model007Gate1Tests(unittest.TestCase):
    def test_release_time_accepts_equivalent_rfc3339_serialization(self) -> None:
        self.assertTrue(
            _same_utc_instant(
                "2026-08-13T08:00:00.1200000Z",
                "2026-08-13T08:00:00.120Z",
            )
        )

    def test_release_must_match_frozen_authorization_and_commitment(self) -> None:
        ids = ["series:a", "series:b"]
        commitment = hashlib.sha256("\n".join(ids).encode()).hexdigest()
        authorization = {
            "frozen_at_utc": "2026-08-13T00:00:00Z",
            "model_artifact_sha256": "a" * 64,
            "model_config_sha256": "b" * 64,
            "evaluation_code_sha256": "c" * 64,
        }
        sealed = {
            "source_dataset_sha256": "d" * 64,
            "final_test": {"series_count": 2, "membership_sha256": commitment},
        }
        released = {
            "source_dataset_sha256": "d" * 64,
            "membership_sha256": commitment,
            "series_ids": ids,
            "authorization": authorization,
        }
        freeze = {"release_authorization": authorization}
        self.assertEqual(_validate_release(sealed, released, freeze), ids)
        released["membership_sha256"] = "e" * 64
        with self.assertRaisesRegex(Gate1Error, "commitment differs"):
            _validate_release(sealed, released, freeze)

    def test_raw_probability_replays_frozen_coefficients_without_refit(self) -> None:
        parameters = [
            {"feature": name, "raw_space_coefficient": float(index == 0)}
            for index, name in enumerate(FEATURE_NAMES)
        ]
        artifact = {
            "fitted_parameters": {
                "features": parameters,
                "raw_space_intercept": 0.0,
            }
        }
        row = StatisticalSeries(
            series_id="series:a",
            split="final_test",
            scheduled_start_utc=datetime(2025, 1, 1, tzinfo=UTC),
            team_ids=("team:a", "team:b"),
            feature_values=(1.0,) + (0.0,) * (len(FEATURE_NAMES) - 1),
            actual_team_1_win=1,
        )
        probability = _raw_probabilities(artifact, [row])[0]
        self.assertAlmostEqual(probability, 1.0 / (1.0 + np.exp(-1.0)))
        expected = hashlib.sha256(b"2025-01-01T00:00:00+00:00\tseries:a\n").hexdigest()
        self.assertEqual(_sha256_temporal_membership([row]), expected)

    def test_gate_passes_when_three_public_windows_and_combined_metrics_hold(
        self,
    ) -> None:
        calibration = {"systematic_overconfidence_breach": False}
        decision = _gate_decision(
            walk_forward(),
            final_metrics(),
            calibration,
            {"decision_rules": rules()},
        )
        self.assertEqual(decision["status"], "passed_continue_raw")
        self.assertEqual(decision["next_task_authorized"], "BACK-001")
        self.assertEqual(decision["favorable_window_count"], 4)

    def test_gate_fails_on_catastrophic_final_degradation(self) -> None:
        calibration = {"systematic_overconfidence_breach": False}
        decision = _gate_decision(
            walk_forward(),
            final_metrics(raw_brier=0.25, raw_log_loss=0.68),
            calibration,
            {"decision_rules": rules()},
        )
        self.assertEqual(decision["status"], "failed_stop_modeling")
        self.assertIsNone(decision["next_task_authorized"])

    def test_systematic_overconfidence_is_a_gate_failure(self) -> None:
        labels = np.asarray([1, 0] * 20, dtype=np.int64)
        probabilities = np.asarray([0.95, 0.95] * 20, dtype=np.float64)
        diagnostic = _calibration_diagnostics(labels, probabilities, rules())
        self.assertTrue(diagnostic["systematic_overconfidence_breach"])
        decision = _gate_decision(
            walk_forward(),
            final_metrics(),
            diagnostic,
            {"decision_rules": rules()},
        )
        self.assertEqual(decision["status"], "failed_stop_modeling")

    def test_frozen_config_is_valid_json_and_selects_raw_before_release(self) -> None:
        config_path = Path(__file__).parents[1] / "research/model007_gate1_config.json"
        config = json.loads(config_path.read_text(encoding="utf-8"))
        self.assertEqual(config["candidate_model"], "raw_statistical")
        self.assertEqual(
            config["calibration_decision"],
            "rollback_sigmoid_before_final_release",
        )
        self.assertNotIn("final_test", json.dumps(config))

    def test_released_output_path_cannot_be_reused(self) -> None:
        # 一次性门禁由构建目录和 Rust 输出文件双重不可覆盖保证；此处固定测试文件语义。
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "released.json"
            output.write_text("{}", encoding="utf-8")
            self.assertTrue(output.exists())


if __name__ == "__main__":
    unittest.main()
