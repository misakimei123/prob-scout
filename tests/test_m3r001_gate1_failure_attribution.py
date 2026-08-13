from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from research.m3r001_gate1_failure_attribution import (
    FailureAttributionError,
    _composition_decomposition,
    _disagreement_attribution,
    _fixed_model_public_replay,
    _load_artifact,
    _metric_summary,
    build_failure_attribution_artifact,
)


def row(
    series_id: str,
    time: str,
    actual: int,
    elo: float,
    raw: float,
    *,
    region: str = "EMEA",
    best_of: int = 3,
    fold: str | None = None,
) -> dict:
    value = {
        "series_id": series_id,
        "scheduled_start_utc": time,
        "region": region,
        "best_of": best_of,
        "actual_team_1_win": actual,
        "elo_baseline": elo,
        "raw_statistical": raw,
    }
    if fold is not None:
        value["fold"] = fold
    return value


class Gate1FailureAttributionTests(unittest.TestCase):
    def test_composition_decomposition_separates_mix_and_within_cell_shift(
        self,
    ) -> None:
        public = [
            row("p1", "2025-01-01T00:00:00Z", 1, 0.6, 0.7, region="A"),
            row("p2", "2025-01-02T00:00:00Z", 0, 0.4, 0.3, region="B"),
        ]
        final = [
            row("f1", "2025-02-01T00:00:00Z", 1, 0.6, 0.4, region="A"),
            row("f2", "2025-02-02T00:00:00Z", 0, 0.4, 0.6, region="B"),
        ]
        result = _composition_decomposition(public, final)
        self.assertTrue(result["all_final_cells_have_public_reference"])
        self.assertAlmostEqual(result["composition_effect"]["brier_score"], 0.0)
        self.assertGreater(
            result["within_cell_time_shift_residual"]["brier_score"], 0.0
        )

    def test_composition_preserves_final_cell_without_public_reference(self) -> None:
        public = [row("p1", "2025-01-01T00:00:00Z", 1, 0.6, 0.7)]
        final = [
            row("f0", "2025-02-01T00:00:00Z", 0, 0.4, 0.3),
            row(
                "f1",
                "2025-02-02T00:00:00Z",
                1,
                0.6,
                0.7,
                region="Korea",
                best_of=5,
            ),
        ]
        result = _composition_decomposition(public, final)
        self.assertEqual(result["status"], "partial_due_to_unseen_final_cells")
        self.assertEqual(result["unseen_final_cells"], ["Korea|BO5"])
        self.assertEqual(result["unseen_final_series_count"], 1)

    def test_fixed_model_public_replay_does_not_use_expanding_probability(self) -> None:
        public = [
            row(
                "p1",
                "2025-01-01T00:00:00Z",
                1,
                0.55,
                0.9,
                fold="fold_1",
            )
        ]
        model = {
            "development_predictions": [
                {
                    "series_id": "p1",
                    "raw_probability_team_1_win": 0.6,
                }
            ]
        }
        replay, diagnostic = _fixed_model_public_replay(model, public)
        self.assertEqual(replay[0]["raw_statistical"], 0.6)
        self.assertNotEqual(replay[0]["raw_statistical"], public[0]["raw_statistical"])
        self.assertIn("fixed_candidate_overall", diagnostic)

    def test_disagreement_contributions_sum_to_overall_brier_delta(self) -> None:
        rows = [
            row("a", "2025-01-01T00:00:00Z", 1, 0.7, 0.8),
            row("b", "2025-01-02T00:00:00Z", 0, 0.4, 0.7),
            row("c", "2025-01-03T00:00:00Z", 1, 0.4, 0.6),
            row("d", "2025-01-04T00:00:00Z", 0, 0.7, 0.6),
        ]
        diagnostic = _disagreement_attribution(rows)
        contribution = sum(
            value["contribution_to_overall_brier_delta"]
            for value in diagnostic["categories"].values()
        )
        self.assertAlmostEqual(
            contribution, _metric_summary(rows)["raw_minus_elo"]["brier_score"]
        )

    def test_artifact_loader_rejects_manifest_hash_drift(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact_path = root / "artifact.json"
            manifest_path = root / "artifact.json.manifest.json"
            artifact_path.write_text(
                json.dumps({"artifact_kind": "probability_model"}), encoding="utf-8"
            )
            manifest_path.write_text(
                json.dumps({"output": {"sha256": "0" * 64}}), encoding="utf-8"
            )
            with self.assertRaisesRegex(FailureAttributionError, "hash differs"):
                _load_artifact(artifact_path, manifest_path, "probability_model")

    def test_full_build_retires_final_and_only_authorizes_data_expansion(self) -> None:
        public = [
            row(
                "p1",
                "2025-01-01T00:00:00Z",
                1,
                0.55,
                0.6,
                fold="fold_1",
            ),
            row(
                "p2",
                "2025-01-08T00:00:00Z",
                0,
                0.45,
                0.4,
                fold="fold_2",
            ),
            row(
                "p3",
                "2025-01-15T00:00:00Z",
                1,
                0.55,
                0.6,
                fold="fold_3",
            ),
        ]
        final = [
            row("f1", "2025-02-01T00:00:00Z", 1, 0.65, 0.45),
            row("f2", "2025-02-02T00:00:00Z", 0, 0.35, 0.55),
        ]
        model = {
            "training": {"series_count": 1},
            "development_predictions": [
                {
                    "series_id": public_row["series_id"],
                    "raw_probability_team_1_win": public_row["raw_statistical"],
                }
                for public_row in public
            ],
        }
        walk_forward = {
            "predictions": public,
            "evaluation": {
                "folds": [
                    {"windows": {"train": {"series_count": count}}}
                    for count in (1, 2, 3)
                ]
            },
        }
        gate = {
            "predictions": final,
            "gate1_decision": {"status": "failed_stop_modeling"},
            "release": {"status": "released_and_evaluated_once"},
        }
        first = build_failure_attribution_artifact(
            model_artifact=model,
            model_input={"artifact_sha256": "a" * 64},
            walk_forward_artifact=walk_forward,
            walk_forward_input={"artifact_sha256": "b" * 64},
            gate_artifact=gate,
            gate_input={"artifact_sha256": "c" * 64},
        )
        second = build_failure_attribution_artifact(
            model_artifact=model,
            model_input={"artifact_sha256": "a" * 64},
            walk_forward_artifact=walk_forward,
            walk_forward_input={"artifact_sha256": "b" * 64},
            gate_artifact=gate,
            gate_input={"artifact_sha256": "c" * 64},
        )
        self.assertEqual(first, second)
        self.assertEqual(
            first["cohort_governance"]["retired_final_status"],
            "retired_diagnostic_evidence_never_independent_again",
        )
        self.assertEqual(first["next_task"]["task_id"], "M3R-002")
        self.assertFalse(first["next_task"]["model_development_authorized"])
        self.assertFalse(first["next_task"]["m4_authorized"])

    def test_full_build_rejects_public_and_final_overlap(self) -> None:
        shared = row(
            "same",
            "2025-01-01T00:00:00Z",
            1,
            0.55,
            0.6,
            fold="fold_1",
        )
        with self.assertRaisesRegex(FailureAttributionError, "overlap"):
            build_failure_attribution_artifact(
                model_artifact={"development_predictions": []},
                model_input={},
                walk_forward_artifact={"predictions": [shared]},
                walk_forward_input={},
                gate_artifact={
                    "predictions": [shared],
                    "gate1_decision": {"status": "failed_stop_modeling"},
                    "release": {"status": "released_and_evaluated_once"},
                },
                gate_input={},
            )


if __name__ == "__main__":
    unittest.main()
