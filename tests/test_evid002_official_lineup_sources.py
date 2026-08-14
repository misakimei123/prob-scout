from __future__ import annotations

import copy
import json
import unittest
from pathlib import Path

from research.evid002_official_lineup_sources import (
    OfficialLineupSourceError,
    audit_source_registry,
)


REGISTRY_PATH = Path("research/evid002_official_lineup_sources.json")


def load_registry() -> dict:
    return json.loads(REGISTRY_PATH.read_text(encoding="utf-8"))


def make_source_eligible(registry: dict, source_id: str) -> None:
    source = next(
        source for source in registry["sources"] if source["source_id"] == source_id
    )
    for capability in registry["source_contract"]["required_capabilities"]:
        source[capability] = True


class OfficialLineupSourceTests(unittest.TestCase):
    def test_current_registry_blocks_forward_collection(self) -> None:
        result = audit_source_registry(load_registry())

        self.assertEqual(result["decision"], "blocked_registry_incomplete")
        self.assertFalse(result["forward_collection_authorized"])
        self.assertEqual(
            [region["status"] for region in result["region_assessments"]],
            ["blocked_no_eligible_source", "blocked_no_eligible_source"],
        )
        self.assertEqual(len(result["source_assessments"]), 5)

    def test_one_ready_region_does_not_authorize_collection(self) -> None:
        registry = load_registry()
        make_source_eligible(registry, "lpl_official_weibo")

        result = audit_source_registry(registry)

        self.assertEqual(result["decision"], "blocked_registry_incomplete")
        self.assertFalse(result["forward_collection_authorized"])
        self.assertEqual(
            result["region_assessments"][0]["eligible_source_ids"],
            ["lpl_official_weibo"],
        )
        self.assertEqual(result["region_assessments"][1]["eligible_source_ids"], [])

    def test_one_eligible_source_per_region_authorizes_collection(self) -> None:
        registry = load_registry()
        make_source_eligible(registry, "lpl_official_weibo")
        make_source_eligible(registry, "lck_league_entry_disclosure")

        result = audit_source_registry(registry)

        self.assertEqual(result["decision"], "ready_for_forward_collection")
        self.assertTrue(result["forward_collection_authorized"])

    def test_capabilities_cannot_be_stitched_across_sources(self) -> None:
        registry = load_registry()
        china_sources = [
            source for source in registry["sources"] if source["region"] == "China"
        ]
        for index, capability in enumerate(
            registry["source_contract"]["required_capabilities"]
        ):
            china_sources[index % len(china_sources)][capability] = True

        result = audit_source_registry(registry)

        china = result["region_assessments"][0]
        self.assertEqual(china["eligible_source_ids"], [])

    def test_duplicate_source_id_is_rejected(self) -> None:
        registry = load_registry()
        registry["sources"].append(copy.deepcopy(registry["sources"][0]))

        with self.assertRaisesRegex(OfficialLineupSourceError, "duplicate source_id"):
            audit_source_registry(registry)

    def test_unknown_region_is_rejected(self) -> None:
        registry = load_registry()
        registry["sources"][0]["region"] = "Americas"

        with self.assertRaisesRegex(
            OfficialLineupSourceError, "outside target regions"
        ):
            audit_source_registry(registry)

    def test_missing_boolean_capability_is_rejected(self) -> None:
        registry = load_registry()
        del registry["sources"][0]["available_at_seconds"]

        with self.assertRaisesRegex(OfficialLineupSourceError, "must be boolean"):
            audit_source_registry(registry)


if __name__ == "__main__":
    unittest.main()
