"""审计 EVID-001 赛前实际首发证据来源与前瞻观察记录。"""

from __future__ import annotations

import argparse
import json
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any


class PrematchLineupEvidenceError(ValueError):
    """EVID-001 配置或观察记录违反冻结合同。"""


def _load_json(path: Path, description: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise PrematchLineupEvidenceError(f"invalid {description}: {path}") from error


def _parse_utc(value: Any, field: str) -> datetime:
    if not isinstance(value, str):
        raise PrematchLineupEvidenceError(f"{field} must be an RFC3339 string")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise PrematchLineupEvidenceError(f"invalid timestamp: {field}") from error
    if parsed.tzinfo is None or parsed.utcoffset() != timedelta(0):
        raise PrematchLineupEvidenceError(f"{field} must be UTC")
    return parsed.astimezone(UTC)


def _validate_config(config: Any) -> dict[str, Any]:
    if not isinstance(config, dict) or config.get("config_schema_version") != 1:
        raise PrematchLineupEvidenceError("unsupported config schema")
    contract = config.get("evidence_contract")
    sources = config.get("sources")
    protocol = config.get("forward_observation_protocol")
    if not isinstance(contract, dict) or not isinstance(protocol, dict):
        raise PrematchLineupEvidenceError("missing evidence contract or protocol")
    if not isinstance(sources, list) or not sources:
        raise PrematchLineupEvidenceError("source registry must not be empty")
    required = contract.get("required_source_capabilities")
    if not isinstance(required, list) or not required:
        raise PrematchLineupEvidenceError("required source capabilities are empty")
    if len(required) != len(set(required)):
        raise PrematchLineupEvidenceError("duplicate required source capability")

    source_ids: set[str] = set()
    for source in sources:
        if not isinstance(source, dict):
            raise PrematchLineupEvidenceError("source registry entry must be an object")
        source_id = source.get("source_id")
        if not isinstance(source_id, str) or not source_id:
            raise PrematchLineupEvidenceError("invalid source_id")
        if source_id in source_ids:
            raise PrematchLineupEvidenceError(f"duplicate source_id: {source_id}")
        source_ids.add(source_id)
        for capability in required:
            if not isinstance(source.get(capability), bool):
                raise PrematchLineupEvidenceError(
                    f"source capability must be boolean: {source_id}.{capability}"
                )
    return config


def audit_source_registry(config: dict[str, Any]) -> dict[str, Any]:
    config = _validate_config(config)
    required = config["evidence_contract"]["required_source_capabilities"]
    assessments: list[dict[str, Any]] = []
    eligible_source_ids: list[str] = []
    for source in config["sources"]:
        missing = [capability for capability in required if not source[capability]]
        eligible = not missing
        if eligible:
            eligible_source_ids.append(source["source_id"])
        assessments.append(
            {
                "source_id": source["source_id"],
                "status": source["status"],
                "eligible": eligible,
                "missing_capabilities": missing,
            }
        )

    # 没有来源通过合取门槛时，必须停止采集；不能把多个弱来源的能力拼成一条虚构 feed。
    return {
        "task_id": config["task_id"],
        "audited_at_utc": config["audited_at_utc"],
        "decision": (
            "ready_for_forward_collection"
            if eligible_source_ids
            else "blocked_no_eligible_source"
        ),
        "eligible_source_ids": eligible_source_ids,
        "forward_collection_authorized": bool(eligible_source_ids),
        "source_assessments": assessments,
    }


def assess_observation(
    observation: dict[str, Any], config: dict[str, Any]
) -> dict[str, Any]:
    config = _validate_config(config)
    source_audit = audit_source_registry(config)
    sources = {source["source_id"]: source for source in config["sources"]}
    reasons: list[str] = []

    source_id = observation.get("source_id")
    source = sources.get(source_id)
    if source is None:
        reasons.append("unknown_source")
    elif source_id not in source_audit["eligible_source_ids"]:
        reasons.append("source_gate_failed")

    try:
        event_at = _parse_utc(observation.get("event_at_utc"), "event_at_utc")
        available_at = _parse_utc(
            observation.get("available_at_utc"), "available_at_utc"
        )
        captured_at = _parse_utc(observation.get("captured_at_utc"), "captured_at_utc")
    except PrematchLineupEvidenceError as error:
        reasons.append(str(error))
    else:
        cutoff = event_at - timedelta(
            minutes=config["evidence_contract"]["decision_cutoff_minutes"]
        )
        if available_at > captured_at:
            reasons.append("available_after_capture")
        if available_at > cutoff:
            reasons.append("published_after_t15")
        if captured_at > cutoff:
            reasons.append("captured_after_t15")

    expected_players = config["evidence_contract"]["players_per_team"]
    team_players: list[list[str]] = []
    for field in ("team_1_player_ids", "team_2_player_ids"):
        players = observation.get(field)
        if (
            not isinstance(players, list)
            or len(players) != expected_players
            or not all(isinstance(player, str) and player for player in players)
            or len(players) != len(set(players))
        ):
            reasons.append(f"invalid_{field}")
        else:
            team_players.append(players)
    if len(team_players) == 2 and set(team_players[0]) & set(team_players[1]):
        reasons.append("player_on_both_teams")

    required_ids = ("event_id", "team_1_id", "team_2_id", "raw_sha256")
    for field in required_ids:
        value = observation.get(field)
        if not isinstance(value, str) or not value:
            reasons.append(f"missing_{field}")
    raw_sha256 = observation.get("raw_sha256")
    if isinstance(raw_sha256, str) and (
        len(raw_sha256) != 64
        or any(character not in "0123456789abcdef" for character in raw_sha256)
    ):
        reasons.append("invalid_raw_sha256")
    if observation.get("team_1_id") == observation.get("team_2_id"):
        reasons.append("identical_team_ids")

    protocol = config["forward_observation_protocol"]
    if observation.get("region") not in protocol["target_regions"]:
        reasons.append("outside_target_region")
    if observation.get("best_of") not in protocol["target_best_of"]:
        reasons.append("unsupported_best_of")

    return {
        "event_id": observation.get("event_id"),
        "source_id": source_id,
        "status": "eligible" if not reasons else "rejected",
        "reasons": sorted(set(reasons)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--config",
        type=Path,
        default=Path("research/evid001_prematch_lineup_config.json"),
    )
    parser.add_argument("--observations", type=Path)
    arguments = parser.parse_args()

    config = _validate_config(_load_json(arguments.config, "EVID-001 config"))
    result = {"source_gate": audit_source_registry(config)}
    if arguments.observations is not None:
        observations = _load_json(arguments.observations, "observation file")
        if not isinstance(observations, list):
            raise PrematchLineupEvidenceError("observation root must be an array")
        result["observations"] = [
            assess_observation(observation, config)
            for observation in observations
            if isinstance(observation, dict)
        ]
        if len(result["observations"]) != len(observations):
            raise PrematchLineupEvidenceError("observation entry must be an object")
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
