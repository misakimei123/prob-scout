from __future__ import annotations

import unittest
from dataclasses import replace
from datetime import UTC, datetime, timedelta

from research.model006_walk_forward import (
    MODEL_NAMES,
    LoadedWalkForwardData,
    PublicSplitWindows,
    WalkForwardError,
    WalkForwardSeries,
    _run_fold,
    build_walk_forward_artifact,
    build_walk_forward_windows,
)


def utc(value: str) -> datetime:
    return datetime.fromisoformat(value).astimezone(UTC)


def split_windows() -> PublicSplitWindows:
    return PublicSplitWindows(
        train_start=utc("2025-01-12T00:00:00+00:00"),
        validation_start=utc("2025-02-23T00:00:00+00:00"),
        validation_end=utc("2025-04-07T00:00:00+00:00"),
        calibration_start=utc("2025-04-07T00:00:00+00:00"),
        calibration_end=utc("2025-05-19T00:00:00+00:00"),
        final_test_start=utc("2025-05-19T00:00:00+00:00"),
    )


def loaded_data() -> LoadedWalkForwardData:
    windows = split_windows()
    series: list[WalkForwardSeries] = []
    day = windows.train_start
    day_index = 0
    while day < windows.final_test_start:
        for slot in range(2):
            label = (day_index + slot) % 2
            direction = 1.0 if label == 1 else -1.0
            features = tuple(
                direction * (1.0 + feature_index * 0.05)
                + ((day_index + feature_index * (slot + 1)) % 7) * 0.01
                for feature_index in range(10)
            )
            series_id = f"series-{day_index:03d}-{slot}"
            series.append(
                WalkForwardSeries(
                    series_id=series_id,
                    scheduled_start_utc=day + timedelta(hours=12 + slot),
                    region=("Korea", "EMEA", "Americas")[(day_index + slot) % 3],
                    best_of=5 if (day_index + slot) % 6 == 0 else 3,
                    team_ids=(f"{series_id}-a", f"{series_id}-b"),
                    feature_values=features,
                    actual_team_1_win=label,
                )
            )
        day += timedelta(days=1)
        day_index += 1
    return LoadedWalkForwardData(
        series=tuple(series),
        split_windows=windows,
        final_test={
            "status": "sealed_not_evaluated",
            "series_count": 20,
            "membership_sha256": "a" * 64,
            "access_policy": "sealed_until_model_freeze",
            "supported_metrics": ["brier_score", "log_loss"],
            "release_requires": ["walk_forward_artifact_sha256"],
        },
    )


class WalkForwardTests(unittest.TestCase):
    def test_windows_are_ordered_disjoint_and_stop_before_final_test(self) -> None:
        windows = build_walk_forward_windows(split_windows())
        self.assertEqual(len(windows), 3)
        for index, window in enumerate(windows):
            self.assertEqual(window.train_end, window.calibration_start)
            self.assertEqual(window.calibration_end, window.evaluation_start)
            self.assertLessEqual(
                window.evaluation_end, split_windows().final_test_start
            )
            if index > 0:
                self.assertEqual(
                    windows[index - 1].evaluation_end, window.evaluation_start
                )

    def test_build_is_deterministic_and_reports_every_model_and_segment(self) -> None:
        loaded = loaded_data()
        first = build_walk_forward_artifact(loaded, {})
        second = build_walk_forward_artifact(loaded, {})
        self.assertEqual(first, second)
        evaluation = first["evaluation"]
        self.assertEqual(len(evaluation["folds"]), 3)
        self.assertEqual(set(evaluation["overall"]["models"]), set(MODEL_NAMES))
        self.assertEqual(set(evaluation["by_region"]), {"Americas", "EMEA", "Korea"})
        self.assertEqual(set(evaluation["by_best_of"]), {"3", "5"})
        self.assertEqual(
            evaluation["evaluated_series_count"], len(first["predictions"])
        )

    def test_evaluation_membership_is_unique(self) -> None:
        artifact = build_walk_forward_artifact(loaded_data(), {})
        ids = [row["series_id"] for row in artifact["predictions"]]
        self.assertEqual(len(ids), len(set(ids)))
        fold_counts = sum(
            fold["windows"]["evaluation"]["series_count"]
            for fold in artifact["evaluation"]["folds"]
        )
        self.assertEqual(fold_counts, len(ids))

    def test_last_evaluation_label_cannot_change_its_own_probabilities(self) -> None:
        loaded = loaded_data()
        window = build_walk_forward_windows(loaded.split_windows)[0]
        original_fold, original_predictions = _run_fold(loaded.series, window)
        last_id = original_predictions[-1]["series_id"]
        changed = replace(
            loaded,
            series=tuple(
                replace(row, actual_team_1_win=1 - row.actual_team_1_win)
                if row.series_id == last_id
                else row
                for row in loaded.series
            ),
        )
        changed_fold, changed_predictions = _run_fold(changed.series, window)
        self.assertEqual(
            original_fold["fitted_parameters"], changed_fold["fitted_parameters"]
        )
        for original, updated in zip(
            original_predictions, changed_predictions, strict=True
        ):
            for model_name in MODEL_NAMES:
                self.assertEqual(original[model_name], updated[model_name])

    def test_calibration_label_changes_only_calibrated_mapping(self) -> None:
        loaded = loaded_data()
        window = build_walk_forward_windows(loaded.split_windows)[0]
        original_fold, original_predictions = _run_fold(loaded.series, window)
        calibration_row = next(
            row
            for row in loaded.series
            if window.calibration_start
            <= row.scheduled_start_utc
            < window.calibration_end
        )
        changed_series = tuple(
            replace(row, actual_team_1_win=1 - row.actual_team_1_win)
            if row.series_id == calibration_row.series_id
            else row
            for row in loaded.series
        )
        changed_fold, changed_predictions = _run_fold(changed_series, window)
        for original, updated in zip(
            original_predictions, changed_predictions, strict=True
        ):
            self.assertEqual(
                original["constant_baseline"], updated["constant_baseline"]
            )
            self.assertEqual(original["raw_statistical"], updated["raw_statistical"])
        self.assertNotEqual(
            original_fold["fitted_parameters"]["sigmoid_b"],
            changed_fold["fitted_parameters"]["sigmoid_b"],
        )

    def test_rejects_calibration_window_with_one_class(self) -> None:
        loaded = loaded_data()
        window = build_walk_forward_windows(loaded.split_windows)[0]
        invalid = tuple(
            replace(row, actual_team_1_win=1)
            if window.calibration_start
            <= row.scheduled_start_utc
            < window.calibration_end
            else row
            for row in loaded.series
        )
        with self.assertRaisesRegex(WalkForwardError, "calibration must contain"):
            _run_fold(invalid, window)

    def test_final_test_remains_sealed_and_gate_is_not_decided(self) -> None:
        artifact = build_walk_forward_artifact(loaded_data(), {})
        self.assertEqual(
            artifact["final_test_evaluation"]["status"], "sealed_not_evaluated"
        )
        self.assertNotIn("series_ids", artifact["final_test_evaluation"])
        self.assertEqual(
            artifact["evaluation"]["gate_decision"]["status"],
            "not_made_in_model006",
        )

    def test_rejects_discontinuous_public_windows(self) -> None:
        invalid = replace(
            split_windows(),
            calibration_start=utc("2025-04-08T00:00:00+00:00"),
        )
        with self.assertRaisesRegex(WalkForwardError, "invalid walk-forward ordering"):
            build_walk_forward_windows(invalid)


if __name__ == "__main__":
    unittest.main()
