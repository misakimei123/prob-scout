from __future__ import annotations

import copy
import json
import unittest
from pathlib import Path

from research.evid001_prematch_lineup import (
    PrematchLineupEvidenceError,
    assess_observation,
    audit_source_registry,
)


CONFIG_PATH = Path("research/evid001_prematch_lineup_config.json")


def load_config() -> dict:
    return json.loads(CONFIG_PATH.read_text(encoding="utf-8"))


def eligible_config() -> dict:
    config = load_config()
    source = config["sources"][0]
    for capability in config["evidence_contract"]["required_source_capabilities"]:
        source[capability] = True
    return config


def valid_observation() -> dict:
    return {
        "event_id": "series:example",
        "source_id": "riot_grid_fixtures",
        "event_at_utc": "2026-09-07T10:00:00Z",
        "available_at_utc": "2026-09-07T09:40:00Z",
        "captured_at_utc": "2026-09-07T09:41:00Z",
        "team_1_id": "team:1",
        "team_2_id": "team:2",
        "region": "China",
        "best_of": 3,
        "team_1_player_ids": [f"player:a{index}" for index in range(5)],
        "team_2_player_ids": [f"player:b{index}" for index in range(5)],
        "raw_sha256": "a" * 64,
    }


class PrematchLineupEvidenceTests(unittest.TestCase):
    def test_current_registry_blocks_forward_collection(self) -> None:
        result = audit_source_registry(load_config())

        self.assertEqual(result["decision"], "blocked_no_eligible_source")
        self.assertFalse(result["forward_collection_authorized"])
        self.assertEqual(result["eligible_source_ids"], [])
        self.assertEqual(len(result["source_assessments"]), 5)

    def test_source_capabilities_are_conjunctive(self) -> None:
        config = eligible_config()
        config["sources"][0]["available_at_seconds"] = False

        result = audit_source_registry(config)

        self.assertNotIn("riot_grid_fixtures", result["eligible_source_ids"])
        source = result["source_assessments"][0]
        self.assertEqual(source["missing_capabilities"], ["available_at_seconds"])

    def test_valid_forward_observation_is_eligible(self) -> None:
        result = assess_observation(valid_observation(), eligible_config())

        self.assertEqual(result["status"], "eligible")
        self.assertEqual(result["reasons"], [])

    def test_cutoff_and_identity_violations_fail_closed(self) -> None:
        observation = valid_observation()
        observation["captured_at_utc"] = "2026-09-07T09:50:00Z"
        observation["team_2_player_ids"][0] = observation["team_1_player_ids"][0]

        result = assess_observation(observation, eligible_config())

        self.assertEqual(result["status"], "rejected")
        self.assertIn("captured_after_t15", result["reasons"])
        self.assertIn("player_on_both_teams", result["reasons"])

    def test_current_source_rejects_otherwise_valid_observation(self) -> None:
        result = assess_observation(valid_observation(), load_config())

        self.assertEqual(result["status"], "rejected")
        self.assertEqual(result["reasons"], ["source_gate_failed"])

    def test_out_of_scope_region_and_best_of_fail_closed(self) -> None:
        observation = valid_observation()
        observation["region"] = "Americas"
        observation["best_of"] = 1

        result = assess_observation(observation, eligible_config())

        self.assertEqual(result["status"], "rejected")
        self.assertIn("outside_target_region", result["reasons"])
        self.assertIn("unsupported_best_of", result["reasons"])

    def test_duplicate_source_id_is_rejected(self) -> None:
        config = load_config()
        config["sources"].append(copy.deepcopy(config["sources"][0]))

        with self.assertRaisesRegex(PrematchLineupEvidenceError, "duplicate source_id"):
            audit_source_registry(config)


if __name__ == "__main__":
    unittest.main()
