"""构建 MODEL-001 训练期先验 Constant Baseline。"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import platform
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np
import sklearn
from sklearn.dummy import DummyClassifier
from sklearn.metrics import brier_score_loss, log_loss

ARTIFACT_SCHEMA_VERSION = 1
MODEL_FAMILY = "constant_baseline"
MODEL_STRATEGY = "train_class_prior"
POSITIVE_LABEL = "team_1_win"
FINAL_TEST_STATUS = "sealed_not_evaluated"
FINAL_TEST_ACCESS_POLICY = "sealed_until_model_freeze"
DEVELOPMENT_SPLITS = ("train", "validation", "calibration")


class ConstantBaselineError(ValueError):
    """输入或合同不满足 MODEL-001 fail-closed 约束。"""


@dataclass(frozen=True)
class LoadedDevelopmentData:
    labels_by_split: dict[str, list[int]]
    series_ids_by_split: dict[str, list[str]]
    final_test: dict[str, Any]
    total_series_count: int


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _sha256_membership(series_ids: Iterable[str]) -> str:
    normalized = "".join(f"{series_id}\n" for series_id in sorted(series_ids))
    return hashlib.sha256(normalized.encode("utf-8")).hexdigest()


def _repository_relative_path(repository_root: Path, path: Path) -> str:
    try:
        relative = path.resolve().relative_to(repository_root.resolve())
    except ValueError as error:
        raise ConstantBaselineError(f"path escapes repository root: {path}") from error
    return relative.as_posix()


def _validated_input_reference(
    repository_root: Path,
    dataset_path: Path,
    manifest_path: Path,
    expected_dataset_name: str,
) -> dict[str, str]:
    if not dataset_path.is_file() or not manifest_path.is_file():
        raise ConstantBaselineError(
            f"missing dataset or manifest: dataset={dataset_path}, manifest={manifest_path}"
        )
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ConstantBaselineError(
            f"invalid dataset manifest: {manifest_path}"
        ) from error

    dataset = manifest.get("dataset")
    output = manifest.get("output")
    if not isinstance(dataset, dict) or not isinstance(output, dict):
        raise ConstantBaselineError(
            "dataset manifest is missing dataset/output metadata"
        )
    if dataset.get("name") != expected_dataset_name:
        raise ConstantBaselineError(
            f"unexpected dataset name: expected={expected_dataset_name}, actual={dataset.get('name')}"
        )

    dataset_relative_path = _repository_relative_path(repository_root, dataset_path)
    manifest_relative_path = _repository_relative_path(repository_root, manifest_path)
    dataset_sha256 = _sha256_file(dataset_path)
    if output.get("relative_path") != dataset_relative_path:
        raise ConstantBaselineError("dataset path does not match its manifest output")
    if output.get("sha256") != dataset_sha256:
        raise ConstantBaselineError(
            "dataset SHA-256 does not match its manifest output"
        )

    return {
        "dataset_name": expected_dataset_name,
        "dataset_version": str(dataset.get("version", "")),
        "dataset_relative_path": dataset_relative_path,
        "dataset_sha256": dataset_sha256,
        "manifest_relative_path": manifest_relative_path,
        "manifest_sha256": _sha256_file(manifest_path),
    }


def _require_nonempty_series_ids(split_name: str, value: Any) -> list[str]:
    if not isinstance(value, list) or not value:
        raise ConstantBaselineError(f"{split_name}.series_ids must be a non-empty list")
    series_ids: list[str] = []
    for series_id in value:
        if not isinstance(series_id, str) or not series_id.strip():
            raise ConstantBaselineError(f"{split_name} contains an empty series_id")
        series_ids.append(series_id)
    if len(series_ids) != len(set(series_ids)):
        raise ConstantBaselineError(f"{split_name} contains duplicate series_id values")
    return series_ids


def load_development_data(
    series_result_path: Path, temporal_split_path: Path
) -> LoadedDevelopmentData:
    try:
        temporal_split = json.loads(temporal_split_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ConstantBaselineError("invalid temporal split manifest") from error
    if temporal_split.get("manifest_version") != 1:
        raise ConstantBaselineError("unsupported temporal split manifest version")

    # development IDs 是本任务唯一允许消费的成员；final_test 类型不得暴露 series_ids。
    series_ids_by_split: dict[str, list[str]] = {}
    all_development_ids: set[str] = set()
    for split_name in DEVELOPMENT_SPLITS:
        split = temporal_split.get(split_name)
        if not isinstance(split, dict):
            raise ConstantBaselineError(f"missing development split: {split_name}")
        series_ids = _require_nonempty_series_ids(split_name, split.get("series_ids"))
        overlap = all_development_ids.intersection(series_ids)
        if overlap:
            raise ConstantBaselineError(
                f"series_id appears in multiple development splits: {min(overlap)}"
            )
        all_development_ids.update(series_ids)
        series_ids_by_split[split_name] = series_ids

    final_test = temporal_split.get("final_test")
    if not isinstance(final_test, dict):
        raise ConstantBaselineError("missing sealed final_test split")
    if "series_ids" in final_test:
        raise ConstantBaselineError("sealed final_test must not expose series_ids")
    final_count = final_test.get("series_count")
    if (
        not isinstance(final_count, int)
        or isinstance(final_count, bool)
        or final_count <= 0
    ):
        raise ConstantBaselineError("sealed final_test series_count must be positive")
    membership_sha256 = final_test.get("membership_sha256")
    if not isinstance(membership_sha256, str) or len(membership_sha256) != 64:
        raise ConstantBaselineError("sealed final_test membership_sha256 is invalid")
    if final_test.get("access_policy") != FINAL_TEST_ACCESS_POLICY:
        raise ConstantBaselineError("sealed final_test access_policy is invalid")

    labels_by_id: dict[str, int] = {}
    total_series_count = 0
    try:
        with series_result_path.open("r", encoding="utf-8-sig", newline="") as source:
            reader = csv.DictReader(source)
            required_fields = {"series_id", "team_1_id", "team_2_id", "winner_team_id"}
            if reader.fieldnames is None or not required_fields.issubset(
                reader.fieldnames
            ):
                raise ConstantBaselineError(
                    "Series Result CSV is missing MODEL-001 fields"
                )
            for row in reader:
                total_series_count += 1
                series_id = (row.get("series_id") or "").strip()
                if series_id not in all_development_ids:
                    # 其他行保持不可归属状态；本任务不推断哪些行属于 final test。
                    continue
                if series_id in labels_by_id:
                    raise ConstantBaselineError(
                        f"duplicate development Series Result: {series_id}"
                    )
                team_one = (row.get("team_1_id") or "").strip()
                team_two = (row.get("team_2_id") or "").strip()
                winner = (row.get("winner_team_id") or "").strip()
                if not team_one or not team_two or team_one == team_two:
                    raise ConstantBaselineError(
                        f"invalid teams for series_id={series_id}"
                    )
                if winner == team_one:
                    labels_by_id[series_id] = 1
                elif winner == team_two:
                    labels_by_id[series_id] = 0
                else:
                    raise ConstantBaselineError(
                        f"winner does not match either team for series_id={series_id}"
                    )
    except OSError as error:
        raise ConstantBaselineError("failed to read Series Result CSV") from error

    missing = all_development_ids.difference(labels_by_id)
    if missing:
        raise ConstantBaselineError(
            f"development split references missing Series Result: {min(missing)}"
        )
    expected_total = len(all_development_ids) + final_count
    if total_series_count != expected_total:
        raise ConstantBaselineError(
            f"Series Result count does not match development plus sealed final count: "
            f"actual={total_series_count}, expected={expected_total}"
        )

    labels_by_split = {
        split_name: [
            labels_by_id[series_id] for series_id in series_ids_by_split[split_name]
        ]
        for split_name in DEVELOPMENT_SPLITS
    }
    return LoadedDevelopmentData(
        labels_by_split=labels_by_split,
        series_ids_by_split=series_ids_by_split,
        final_test={
            "series_count": final_count,
            "membership_sha256": membership_sha256,
            "access_policy": FINAL_TEST_ACCESS_POLICY,
        },
        total_series_count=total_series_count,
    )


def fit_train_prior(train_labels: list[int]) -> float:
    if not train_labels:
        raise ConstantBaselineError("train labels must not be empty")
    features = np.zeros((len(train_labels), 1), dtype=np.uint8)
    labels = np.asarray(train_labels, dtype=np.uint8)
    classifier = DummyClassifier(strategy="prior")
    classifier.fit(features, labels)
    if classifier.classes_.tolist() != [0, 1]:
        raise ConstantBaselineError("train split must contain both binary classes")
    positive_index = int(np.where(classifier.classes_ == 1)[0][0])
    probability = float(
        classifier.predict_proba(np.zeros((1, 1), dtype=np.uint8))[0][positive_index]
    )
    if not 0.0 < probability < 1.0:
        raise ConstantBaselineError(
            "train prior probability must be strictly between zero and one"
        )
    return probability


def evaluate_probability(
    labels: list[int], probability: float
) -> dict[str, int | float]:
    if not labels:
        raise ConstantBaselineError("evaluation labels must not be empty")
    y_true = np.asarray(labels, dtype=np.uint8)
    positive_probabilities = np.full(len(labels), probability, dtype=np.float64)
    class_probabilities = np.column_stack(
        (1.0 - positive_probabilities, positive_probabilities)
    )
    return {
        "series_count": len(labels),
        "team_1_win_count": int(y_true.sum()),
        "observed_team_1_win_rate": float(y_true.mean()),
        "brier_score": float(
            brier_score_loss(
                y_true,
                positive_probabilities,
                pos_label=1,
                scale_by_half=True,
            )
        ),
        "log_loss": float(log_loss(y_true, class_probabilities, labels=[0, 1])),
    }


def build_artifact(
    loaded: LoadedDevelopmentData,
    series_input: dict[str, str],
    temporal_split_input: dict[str, str],
) -> dict[str, Any]:
    probability = fit_train_prior(loaded.labels_by_split["train"])
    evaluations = {
        split_name: evaluate_probability(
            loaded.labels_by_split[split_name], probability
        )
        for split_name in DEVELOPMENT_SPLITS
    }
    return {
        "artifact_schema_version": ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "probability_model",
        "model": {
            "family": MODEL_FAMILY,
            "strategy": MODEL_STRATEGY,
            "positive_label": POSITIVE_LABEL,
            "probability_team_1_win": probability,
            "uses_features": False,
            "uses_market_data": False,
        },
        "runtime": {
            "python_version": platform.python_version(),
            "numpy_version": np.__version__,
            "scikit_learn_version": sklearn.__version__,
        },
        "inputs": {
            "series_result": series_input,
            "temporal_split": temporal_split_input,
        },
        "training": {
            "split": "train",
            "series_count": len(loaded.labels_by_split["train"]),
            "team_1_win_count": sum(loaded.labels_by_split["train"]),
            "series_membership_sha256": _sha256_membership(
                loaded.series_ids_by_split["train"]
            ),
        },
        "development_evaluation": evaluations,
        "final_test_evaluation": {
            "status": FINAL_TEST_STATUS,
            **loaded.final_test,
            "supported_metrics": ["brier_score", "log_loss"],
            "release_requires": [
                "model_artifact_sha256",
                "model_config_sha256",
                "evaluation_code_sha256",
            ],
        },
    }


def _parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build the MODEL-001 train-prior Constant Baseline artifact."
    )
    parser.add_argument("--repository-root", required=True, type=Path)
    parser.add_argument("--series-results", required=True, type=Path)
    parser.add_argument("--series-manifest", required=True, type=Path)
    parser.add_argument("--temporal-split", required=True, type=Path)
    parser.add_argument("--temporal-split-manifest", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


def main() -> None:
    arguments = _parse_arguments()
    repository_root = arguments.repository_root.resolve()
    series_input = _validated_input_reference(
        repository_root,
        arguments.series_results.resolve(),
        arguments.series_manifest.resolve(),
        "lol-series-results",
    )
    temporal_split_input = _validated_input_reference(
        repository_root,
        arguments.temporal_split.resolve(),
        arguments.temporal_split_manifest.resolve(),
        "lol-temporal-splits",
    )
    loaded = load_development_data(
        arguments.series_results.resolve(), arguments.temporal_split.resolve()
    )
    artifact = build_artifact(loaded, series_input, temporal_split_input)
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(
        json.dumps(artifact, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
        newline="\n",
    )


if __name__ == "__main__":
    try:
        main()
    except (ConstantBaselineError, OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"constant baseline build failed: {error}") from error
