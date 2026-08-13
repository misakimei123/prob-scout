from __future__ import annotations

import csv
import json
import tempfile
import unittest
from pathlib import Path

from research.model003_market_baseline import (
    MarketBaselineError,
    build_artifact,
    load_market_data,
)


class MarketBaselineTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.series_path = self.root / "series.csv"
        self.links_path = self.root / "links.csv"
        self.split_path = self.root / "split.json"
        self.review_path = self.root / "review.csv"
        self.grades_path = self.root / "grades.csv"
        self._write_valid_fixture()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    @staticmethod
    def _write_csv(path: Path, rows: list[dict[str, str]]) -> None:
        with path.open("w", encoding="utf-8", newline="") as target:
            writer = csv.DictWriter(target, fieldnames=list(rows[0]))
            writer.writeheader()
            writer.writerows(rows)

    def _write_valid_fixture(self) -> None:
        ids = ["leaguepedia:t1", "leaguepedia:v1", "leaguepedia:c1", "leaguepedia:f1"]
        self.split_path.write_text(
            json.dumps(
                {
                    "manifest_version": 1,
                    "train": {"series_ids": [ids[0]]},
                    "validation": {"series_ids": [ids[1]]},
                    "calibration": {"series_ids": [ids[2]]},
                    "final_test": {
                        "series_count": 1,
                        "membership_sha256": "a" * 64,
                        "access_policy": "sealed_until_model_freeze",
                    },
                }
            ),
            encoding="utf-8",
        )
        series_rows = []
        link_rows = []
        review_rows = []
        grade_rows = []
        for index, series_id in enumerate(ids, start=1):
            review_id = f"{index:02d}"
            market_id = str(100 + index)
            team_one = f"team-{index}-a"
            team_two = f"team-{index}-b"
            start = f"2026-08-0{index}T12:00:00Z"
            decision = f"2026-08-0{index}T11:45:00Z"
            series_rows.append(
                {
                    "series_id": series_id,
                    "scheduled_start_utc": start,
                    "team_1_id": team_one,
                    "team_2_id": team_two,
                    "winner_team_id": team_one,
                    "mapping_evidence_id": f"DATA-008:{review_id}",
                }
            )
            link_rows.append(
                {
                    "series_id": series_id,
                    "market_id": market_id,
                    "resolution_status": "resolved",
                    "closed": "True",
                    "outcome_0_team_id": team_one,
                    "outcome_1_team_id": team_two,
                    "outcome_0_price": "1",
                    "outcome_1_price": "0",
                    "winner_outcome_index": "0",
                    "mapping_evidence_id": f"DATA-008:{review_id}",
                }
            )
            review_rows.append(
                {
                    "review_id": review_id,
                    "market_id": market_id,
                    "gamma_outcome_0": f"Team {index} A",
                    "gamma_outcome_1": f"Team {index} B",
                    "clob_game_start_utc": start,
                    "leaguepedia_match_id": series_id.removeprefix("leaguepedia:"),
                    "leaguepedia_start_utc": start,
                    "start_delta_seconds": "0",
                    "expected_status": "Matched",
                    "manual_result": "verified_correct",
                }
            )
            grade_rows.append(
                {
                    "review_id": review_id,
                    "market_id": market_id,
                    "mapping_status": "Matched",
                    "decision_time_utc": decision,
                    "history_window_start_utc": f"2026-08-0{index - 1}T11:45:00Z"
                    if index > 1
                    else "2026-07-31T11:45:00Z",
                    "fidelity_minutes": "1",
                    "outcome_0": f"Team {index} A",
                    "outcome_0_last_point_utc": f"2026-08-0{index}T11:44:50Z",
                    "outcome_0_last_price": "0.7",
                    "outcome_0_staleness_seconds": "10",
                    "outcome_1": f"Team {index} B",
                    "outcome_1_last_point_utc": f"2026-08-0{index}T11:44:40Z",
                    "outcome_1_last_price": "0.3",
                    "outcome_1_staleness_seconds": "20",
                    "historical_depth": "false",
                    "historical_bid_ask": "false",
                    "price_history": "true",
                    "grade": "C",
                    "outcome_0_source_sha256": "b" * 64,
                    "outcome_1_source_sha256": "c" * 64,
                }
            )
        self._write_csv(self.series_path, series_rows)
        self._write_csv(self.links_path, link_rows)
        self._write_csv(self.review_path, review_rows)
        self._write_csv(self.grades_path, grade_rows)

    def _load(self):
        return load_market_data(
            self.series_path,
            self.links_path,
            self.split_path,
            self.review_path,
            self.grades_path,
        )

    def _read_rows(self, path: Path) -> list[dict[str, str]]:
        with path.open("r", encoding="utf-8", newline="") as source:
            return list(csv.DictReader(source))

    def test_selects_price_by_explicit_outcome_order(self) -> None:
        links = self._read_rows(self.links_path)
        links[0]["outcome_0_team_id"], links[0]["outcome_1_team_id"] = (
            links[0]["outcome_1_team_id"],
            links[0]["outcome_0_team_id"],
        )
        links[0]["outcome_0_price"] = "0"
        links[0]["outcome_1_price"] = "1"
        links[0]["winner_outcome_index"] = "1"
        self._write_csv(self.links_path, links)

        loaded = self._load()
        first = next(item for item in loaded.series if item.split == "train")
        self.assertEqual(first.team_1_outcome_index, 1)
        self.assertEqual(first.probability_team_1_win, 0.3)

    def test_artifact_separates_price_signal_from_executable_ask(self) -> None:
        artifact = build_artifact(self._load(), {})
        contract = artifact["probability_contract"]
        self.assertEqual(contract["source_field"], "p")
        self.assertEqual(contract["execution_price_status"], "unavailable")
        self.assertIn("buy ask", contract["not_equivalent_to"])
        self.assertFalse(artifact["comparability"]["direct_metric_comparison_allowed"])

    def test_rejects_unverified_mapping(self) -> None:
        reviews = self._read_rows(self.review_path)
        reviews[0]["expected_status"] = "NeedsReview"
        reviews[0]["manual_result"] = "correctly_escalated"
        self._write_csv(self.review_path, reviews)
        with self.assertRaisesRegex(MarketBaselineError, "manually verified Matched"):
            self._load()

    def test_rejects_unavailable_price_history(self) -> None:
        grades = self._read_rows(self.grades_path)
        grades[0]["grade"] = "Unavailable"
        grades[0]["price_history"] = "false"
        self._write_csv(self.grades_path, grades)
        with self.assertRaisesRegex(MarketBaselineError, "price is unavailable"):
            self._load()

    def test_rejects_post_cutoff_price_point(self) -> None:
        grades = self._read_rows(self.grades_path)
        grades[0]["outcome_0_last_point_utc"] = "2026-08-01T11:45:01Z"
        grades[0]["outcome_0_staleness_seconds"] = "0"
        self._write_csv(self.grades_path, grades)
        with self.assertRaisesRegex(MarketBaselineError, "pre-cutoff window"):
            self._load()

    def test_rejects_wrong_decision_cutoff(self) -> None:
        grades = self._read_rows(self.grades_path)
        grades[0]["decision_time_utc"] = "2026-08-01T11:50:00Z"
        self._write_csv(self.grades_path, grades)
        with self.assertRaisesRegex(MarketBaselineError, "Game Start - 15m"):
            self._load()

    def test_rejects_exposed_final_test_ids(self) -> None:
        split = json.loads(self.split_path.read_text(encoding="utf-8"))
        split["final_test"]["series_ids"] = ["leaguepedia:f1"]
        self.split_path.write_text(json.dumps(split), encoding="utf-8")
        with self.assertRaisesRegex(MarketBaselineError, "must not expose series_ids"):
            self._load()


if __name__ == "__main__":
    unittest.main()
