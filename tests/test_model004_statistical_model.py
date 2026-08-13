from __future__ import annotations

import csv
import json
import tempfile
import unittest
from dataclasses import replace
from datetime import UTC, datetime, timedelta
from pathlib import Path

from research.model004_statistical_model import (
    FEATURE_NAMES,
    LoadedStatisticalData,
    StatisticalModelError,
    StatisticalSeries,
    TeamForm,
    build_artifact,
    build_feature_vector,
    fit_statistical_model,
    load_statistical_data,
)

START = datetime(2025, 1, 1, 12, tzinfo=UTC)


def model_series(
    series_id: str,
    split: str,
    offset: int,
    features: tuple[float, ...],
    label: int,
) -> StatisticalSeries:
    return StatisticalSeries(
        series_id=series_id,
        split=split,
        scheduled_start_utc=START + timedelta(days=offset),
        team_ids=(f"{series_id}-a", f"{series_id}-b"),
        feature_values=features,
        actual_team_1_win=label,
    )


class StatisticalModelTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.series_path = self.root / "series.csv"
        self.feature_path = self.root / "features.json"
        self.split_path = self.root / "split.json"
        self._write_valid_fixture()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    @staticmethod
    def _write_csv(path: Path, rows: list[dict[str, str]]) -> None:
        with path.open("w", encoding="utf-8", newline="") as target:
            writer = csv.DictWriter(target, fieldnames=list(rows[0]))
            writer.writeheader()
            writer.writerows(rows)

    @staticmethod
    def _team_feature(
        team_id: str,
        wins: int,
        count: int,
        source_time: str,
    ) -> dict[str, object]:
        games = count * 3
        game_wins = min(games, wins * 2)
        return {
            "team_id": team_id,
            "source_team_key": team_id,
            "prior_series_count": {
                "value": count,
                "source_latest_at_utc": source_time,
            },
            "prior_series_win_rate": {
                "numerator": wins,
                "denominator": count,
                "source_latest_at_utc": source_time,
            },
            "prior_game_count": {
                "value": games,
                "source_latest_at_utc": source_time,
            },
            "prior_game_win_rate": {
                "numerator": game_wins,
                "denominator": games,
                "source_latest_at_utc": source_time,
            },
            "same_patch_series_count": {
                "value": count,
                "source_latest_at_utc": source_time,
            },
            "same_patch_series_win_rate": {
                "numerator": wins,
                "denominator": count,
                "source_latest_at_utc": source_time,
            },
            "rest_minutes": {
                "value": 1440 + count,
                "source_latest_at_utc": source_time,
            },
        }

    def _write_valid_fixture(self) -> None:
        split_ids = {
            "train": ["t1", "t2"],
            "validation": ["v1"],
            "calibration": ["c1"],
        }
        self.split_path.write_text(
            json.dumps(
                {
                    "manifest_version": 1,
                    **{
                        name: {"series_ids": series_ids}
                        for name, series_ids in split_ids.items()
                    },
                    "final_test": {
                        "series_count": 1,
                        "membership_sha256": "a" * 64,
                        "access_policy": "sealed_until_model_freeze",
                    },
                }
            ),
            encoding="utf-8",
        )

        all_ids = ["t1", "t2", "v1", "c1", "hidden"]
        series_rows = []
        feature_rows = []
        for index, series_id in enumerate(all_ids):
            scheduled = START + timedelta(days=index)
            snapshot = scheduled - timedelta(minutes=15)
            source = scheduled - timedelta(days=1)
            team_one = f"{series_id}-a"
            team_two = f"{series_id}-b"
            winner = team_one if index % 2 == 0 else team_two
            series_rows.append(
                {
                    "series_id": series_id,
                    "competition_id": "competition-a",
                    "region": "Korea",
                    "patch": "25.1",
                    "scheduled_start_utc": scheduled.isoformat(),
                    "best_of": "5" if index % 2 == 0 else "3",
                    "team_1_id": team_one,
                    "team_2_id": team_two,
                    "winner_team_id": winner,
                }
            )
            # 特意反转 JSON team_features 顺序，验证模型只按显式 team_id 对齐。
            feature_rows.append(
                {
                    "series_id": series_id,
                    "competition_id": "competition-a",
                    "region": "Korea",
                    "patch": "25.1",
                    "scheduled_start_utc": scheduled.isoformat(),
                    "snapshot_at_utc": snapshot.isoformat(),
                    "best_of": 5 if index % 2 == 0 else 3,
                    "team_features": [
                        self._team_feature(
                            team_two, 1 + index % 2, 4 + index, source.isoformat()
                        ),
                        self._team_feature(
                            team_one, 3 + index % 2, 5 + index, source.isoformat()
                        ),
                    ],
                }
            )
        self._write_csv(self.series_path, series_rows)
        self.feature_path.write_text(json.dumps(feature_rows), encoding="utf-8")

    def _load(self) -> LoadedStatisticalData:
        return load_statistical_data(
            self.series_path,
            self.feature_path,
            self.split_path,
        )

    def _read_features(self) -> list[dict[str, object]]:
        return json.loads(self.feature_path.read_text(encoding="utf-8"))

    def test_missing_history_uses_neutral_rate_and_availability_signal(self) -> None:
        unavailable = TeamForm(
            team_id="a",
            prior_series_count=0,
            prior_series_win_rate=0.5,
            prior_game_count=0,
            prior_game_win_rate=0.5,
            same_patch_series_count=0,
            same_patch_series_win_rate=0.5,
            rest_minutes=None,
        )
        available = TeamForm(
            team_id="b",
            prior_series_count=4,
            prior_series_win_rate=0.5,
            prior_game_count=12,
            prior_game_win_rate=0.5,
            same_patch_series_count=0,
            same_patch_series_win_rate=0.5,
            rest_minutes=1440,
        )
        values = dict(
            zip(
                FEATURE_NAMES,
                build_feature_vector(unavailable, available, 3),
                strict=True,
            )
        )
        self.assertEqual(values["prior_series_win_rate_diff"], 0.0)
        self.assertEqual(values["same_patch_series_win_rate_diff"], 0.0)
        self.assertEqual(values["prior_history_available_diff"], -1.0)

    def test_aligns_features_by_team_id_instead_of_json_position(self) -> None:
        loaded = self._load()
        train_one = next(item for item in loaded.series if item.series_id == "t1")
        values = dict(zip(FEATURE_NAMES, train_one.feature_values, strict=True))
        self.assertGreater(values["prior_series_win_rate_diff"], 0.0)
        self.assertEqual(train_one.team_ids, ("t1-a", "t1-b"))

    def test_rejects_feature_source_after_snapshot(self) -> None:
        features = self._read_features()
        features[0]["team_features"][0]["prior_series_count"][
            "source_latest_at_utc"
        ] = "2025-01-01T12:00:01+00:00"
        self.feature_path.write_text(json.dumps(features), encoding="utf-8")
        with self.assertRaisesRegex(StatisticalModelError, "exceeds snapshot"):
            self._load()

    def test_rejects_target_field_in_feature_snapshot(self) -> None:
        features = self._read_features()
        features[0]["winner_team_id"] = "t1-a"
        self.feature_path.write_text(json.dumps(features), encoding="utf-8")
        with self.assertRaisesRegex(StatisticalModelError, "forbidden target field"):
            self._load()

    def test_rejects_snapshot_outside_fixed_t_minus_15_minutes(self) -> None:
        features = self._read_features()
        features[0]["snapshot_at_utc"] = "2025-01-01T11:44:00+00:00"
        self.feature_path.write_text(json.dumps(features), encoding="utf-8")
        with self.assertRaisesRegex(StatisticalModelError, "T-15m cutoff"):
            self._load()

    def test_validation_labels_cannot_change_fitted_parameters(self) -> None:
        base = (0.1,) * len(FEATURE_NAMES)
        opposite = (-0.2,) * len(FEATURE_NAMES)
        original = [
            model_series("t1", "train", 0, base, 1),
            model_series("t2", "train", 1, opposite, 0),
            model_series("v1", "validation", 2, base, 0),
        ]
        changed = [
            item if item.split == "train" else replace(item, actual_team_1_win=1)
            for item in original
        ]
        first = fit_statistical_model(original)
        second = fit_statistical_model(changed)
        self.assertEqual(first.raw_coefficients, second.raw_coefficients)
        self.assertEqual(first.raw_intercept, second.raw_intercept)

    def test_artifact_is_deterministic_and_keeps_raw_probability(self) -> None:
        loaded = self._load()
        first = build_artifact(loaded, {})
        second = build_artifact(loaded, {})
        self.assertEqual(first, second)
        self.assertEqual(first["model"]["probability_status"], "raw_uncalibrated")
        self.assertEqual(first["calibration"]["status"], "not_applied_in_model004")
        self.assertNotIn("series_ids", first["final_test_evaluation"])

    def test_rejects_exposed_final_test_ids(self) -> None:
        split = json.loads(self.split_path.read_text(encoding="utf-8"))
        split["final_test"]["series_ids"] = ["hidden"]
        self.split_path.write_text(json.dumps(split), encoding="utf-8")
        with self.assertRaisesRegex(
            StatisticalModelError, "must not expose series_ids"
        ):
            self._load()


if __name__ == "__main__":
    unittest.main()
