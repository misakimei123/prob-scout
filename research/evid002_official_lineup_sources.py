"""审计 EVID-002 China/Korea 官方首发公告来源登记表。"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


class OfficialLineupSourceError(ValueError):
    """EVID-002 registry 违反冻结合同。"""


def _load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise OfficialLineupSourceError(f"invalid registry: {path}") from error


def _validate_registry(registry: Any) -> dict[str, Any]:
    if not isinstance(registry, dict) or registry.get("registry_schema_version") != 1:
        raise OfficialLineupSourceError("unsupported registry schema")

    contract = registry.get("source_contract")
    sources = registry.get("sources")
    if not isinstance(contract, dict):
        raise OfficialLineupSourceError("missing source contract")
    if not isinstance(sources, list) or not sources:
        raise OfficialLineupSourceError("source registry must not be empty")

    target_regions = contract.get("target_regions")
    required_capabilities = contract.get("required_capabilities")
    if (
        not isinstance(target_regions, list)
        or not target_regions
        or not all(isinstance(region, str) and region for region in target_regions)
        or len(target_regions) != len(set(target_regions))
    ):
        raise OfficialLineupSourceError("invalid target regions")
    if (
        not isinstance(required_capabilities, list)
        or not required_capabilities
        or not all(
            isinstance(capability, str) and capability
            for capability in required_capabilities
        )
        or len(required_capabilities) != len(set(required_capabilities))
    ):
        raise OfficialLineupSourceError("invalid required capabilities")

    source_ids: set[str] = set()
    for source in sources:
        if not isinstance(source, dict):
            raise OfficialLineupSourceError("source entry must be an object")
        source_id = source.get("source_id")
        region = source.get("region")
        if not isinstance(source_id, str) or not source_id:
            raise OfficialLineupSourceError("invalid source_id")
        if source_id in source_ids:
            raise OfficialLineupSourceError(f"duplicate source_id: {source_id}")
        source_ids.add(source_id)
        if region not in target_regions:
            raise OfficialLineupSourceError(
                f"source region is outside target regions: {source_id}.{region}"
            )
        for capability in required_capabilities:
            if not isinstance(source.get(capability), bool):
                raise OfficialLineupSourceError(
                    f"source capability must be boolean: {source_id}.{capability}"
                )
        evidence_urls = source.get("evidence_urls")
        if (
            not isinstance(evidence_urls, list)
            or not evidence_urls
            or not all(
                isinstance(url, str) and url.startswith("https://")
                for url in evidence_urls
            )
        ):
            raise OfficialLineupSourceError(f"invalid evidence_urls: {source_id}")
    return registry


def audit_source_registry(registry: dict[str, Any]) -> dict[str, Any]:
    registry = _validate_registry(registry)
    contract = registry["source_contract"]
    required_capabilities = contract["required_capabilities"]
    target_regions = contract["target_regions"]

    source_assessments: list[dict[str, Any]] = []
    eligible_by_region = {region: [] for region in target_regions}
    for source in registry["sources"]:
        missing = [
            capability for capability in required_capabilities if not source[capability]
        ]
        eligible = not missing
        if eligible:
            eligible_by_region[source["region"]].append(source["source_id"])
        source_assessments.append(
            {
                "source_id": source["source_id"],
                "region": source["region"],
                "status": source["status"],
                "eligible": eligible,
                "missing_capabilities": missing,
            }
        )

    # 每个目标赛区都必须由一条来源独立通过全部能力；禁止跨来源或跨赛区拼接能力。
    region_assessments = [
        {
            "region": region,
            "status": (
                "ready" if eligible_by_region[region] else "blocked_no_eligible_source"
            ),
            "eligible_source_ids": eligible_by_region[region],
        }
        for region in target_regions
    ]
    ready = all(eligible_by_region[region] for region in target_regions)
    return {
        "task_id": registry["task_id"],
        "audited_at_utc": registry["audited_at_utc"],
        "decision": (
            "ready_for_forward_collection" if ready else "blocked_registry_incomplete"
        ),
        "forward_collection_authorized": ready,
        "region_assessments": region_assessments,
        "source_assessments": source_assessments,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--registry",
        type=Path,
        default=Path("research/evid002_official_lineup_sources.json"),
    )
    arguments = parser.parse_args()
    result = audit_source_registry(_load_json(arguments.registry))
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
