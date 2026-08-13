from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

import numpy as np

from research.model005_probability_calibration import (
    CALIBRATION_BINS,
    LoadedRawModel,
    ProbabilityCalibrationError,
    RawPrediction,
    RawProbabilityEstimator,
    build_calibration_artifact,
    load_raw_model_artifact,
)


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def raw_model() -> LoadedRawModel:
    predictions: list[RawPrediction] = []
    for split, count in (("train", 12), ("validation", 12), ("calibration", 20)):
        for index in range(count):
            # calibration 中两类各 10 条，满足固定五折响应生成的 fail-closed 下限。
            actual = index % 2
            probability = 0.18 + (0.62 * index / max(1, count - 1))
            predictions.append(
                RawPrediction(
                    series_id=f"{split}-{index:02d}",
                    split=split,
                    raw_probability=probability,
                    actual_team_1_win=actual,
                )
            )
    return LoadedRawModel(
        predictions=tuple(predictions),
        final_test={
            "status": "sealed_not_evaluated",
            "series_count": 7,
            "membership_sha256": "a" * 64,
            "access_policy": "sealed_until_model_freeze",
            "supported_metrics": ["brier_score", "log_loss"],
            "release_requires": [
                "model_artifact_sha256",
                "model_config_sha256",
                "evaluation_code_sha256",
            ],
        },
        input_reference={
            "artifact_name": "statistical-model",
            "artifact_version": "fixture",
            "artifact_relative_path": "artifacts/model.json",
            "artifact_sha256": "b" * 64,
            "manifest_relative_path": "artifacts/model.json.manifest.json",
            "manifest_sha256": "c" * 64,
        },
        model_config_sha256="d" * 64,
    )


class ProbabilityCalibrationTests(unittest.TestCase):
    def test_build_is_deterministic_and_preserves_both_probabilities(self) -> None:
        loaded = raw_model()
        first = build_calibration_artifact(loaded)
        second = build_calibration_artifact(loaded)
        self.assertEqual(first, second)
        self.assertEqual(first["calibration"]["config"]["fitting_split"], "calibration")
        self.assertEqual(first["calibration"]["config"]["method"], "sigmoid")
        self.assertEqual(len(first["development_predictions"]), len(loaded.predictions))
        prediction = first["development_predictions"][0]
        self.assertIn("raw_probability_team_1_win", prediction)
        self.assertIn("calibrated_probability_team_1_win", prediction)

    def test_non_calibration_labels_cannot_change_fitted_calibrator(self) -> None:
        loaded = raw_model()
        changed = replace(
            loaded,
            predictions=tuple(
                row
                if row.split == "calibration"
                else replace(row, actual_team_1_win=1 - row.actual_team_1_win)
                for row in loaded.predictions
            ),
        )
        first = build_calibration_artifact(loaded)
        second = build_calibration_artifact(changed)
        self.assertEqual(first["calibration"], second["calibration"])
        self.assertEqual(
            first["calibration_fit_diagnostics"],
            second["calibration_fit_diagnostics"],
        )

    def test_calibration_diagnostics_compare_metrics_and_curves(self) -> None:
        artifact = build_calibration_artifact(raw_model())
        diagnostics = artifact["calibration_fit_diagnostics"]
        self.assertEqual(
            diagnostics["scope"],
            "in_sample_calibration_fit_diagnostic_not_gate_evidence",
        )
        for probability_kind in ("raw", "calibrated"):
            self.assertIn("brier_score", diagnostics[probability_kind])
            self.assertIn("log_loss", diagnostics[probability_kind])
            curve = diagnostics[probability_kind]["calibration_curve"]
            self.assertEqual(curve["n_bins_requested"], CALIBRATION_BINS)
            self.assertGreater(curve["n_bins_returned"], 0)
            self.assertEqual(curve["n_bins_returned"], len(curve["points"]))

    def test_sigmoid_mapping_is_monotonic_and_open(self) -> None:
        artifact = build_calibration_artifact(raw_model())
        fitted = artifact["calibration"]["fitted_parameters"]
        self.assertLess(fitted["sklearn_sigmoid_a"], 0.0)
        probabilities = np.asarray(
            [
                row["calibrated_probability_team_1_win"]
                for row in artifact["development_predictions"]
                if row["split"] == "calibration"
            ]
        )
        self.assertTrue(np.all((probabilities > 0.0) & (probabilities < 1.0)))
        self.assertTrue(np.all(np.diff(probabilities) > 0.0))

    def test_final_test_remains_sealed(self) -> None:
        artifact = build_calibration_artifact(raw_model())
        final_test = artifact["final_test_evaluation"]
        self.assertEqual(final_test["status"], "sealed_not_evaluated")
        self.assertNotIn("series_ids", final_test)

    def test_rejects_calibration_split_without_both_classes(self) -> None:
        loaded = raw_model()
        invalid = replace(
            loaded,
            predictions=tuple(
                replace(row, actual_team_1_win=1) if row.split == "calibration" else row
                for row in loaded.predictions
            ),
        )
        with self.assertRaisesRegex(
            ProbabilityCalibrationError, "at least five rows from each class"
        ):
            build_calibration_artifact(invalid)

    def test_raw_probability_estimator_rejects_invalid_values(self) -> None:
        estimator = RawProbabilityEstimator()
        with self.assertRaisesRegex(
            ProbabilityCalibrationError, "strictly between zero and one"
        ):
            estimator.predict_proba(np.asarray([[0.0], [0.5]]))

    def test_loader_rejects_tampered_model_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifact_path = root / "model.json"
            manifest_path = root / "model.json.manifest.json"
            artifact = {
                "artifact_schema_version": 1,
                "model": {
                    "family": "logistic_regression",
                    "training_split": "train",
                    "probability_status": "raw_uncalibrated",
                    "config_sha256": "d" * 64,
                },
                "calibration": {"status": "not_applied_in_model004"},
                "development_predictions": [
                    {
                        "series_id": row.series_id,
                        "split": row.split,
                        "raw_probability_team_1_win": row.raw_probability,
                        "actual_team_1_win": row.actual_team_1_win,
                    }
                    for row in raw_model().predictions
                ],
                "final_test_evaluation": raw_model().final_test,
            }
            artifact_path.write_text(json.dumps(artifact), encoding="utf-8")
            manifest = {
                "artifact_manifest_version": 1,
                "artifact": {
                    "kind": "probability-model",
                    "name": "statistical-model",
                    "version": "fixture",
                },
                "output": {
                    "relative_path": "model.json",
                    "sha256": sha256_file(artifact_path),
                },
            }
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            artifact_path.write_text(
                json.dumps({**artifact, "tampered": True}), encoding="utf-8"
            )
            with self.assertRaisesRegex(
                ProbabilityCalibrationError, "SHA-256 does not match"
            ):
                load_raw_model_artifact(root, artifact_path, manifest_path)


if __name__ == "__main__":
    unittest.main()
