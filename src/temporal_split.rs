use std::{collections::BTreeSet, error::Error, fmt, fmt::Write};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const TEMPORAL_SPLIT_MANIFEST_VERSION: u16 = 1;
pub const FINAL_TEST_ACCESS_POLICY: &str = "sealed_until_model_freeze";
pub const RECOVERY_SPLIT_CONTRACT_VERSION: u16 = 1;
pub const RECOVERY_INDEPENDENCE_POLICY: &str = "retired_final_excluded_from_entire_recovery_corpus";

/// HIST-005 只按 Event 的 Scheduled Start 分配；输入不需要读取任何特征值或 label。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalSplitCandidate {
    pub series_id: String,
    pub scheduled_start_utc: DateTime<Utc>,
}

/// 半开时间区间 `[start_utc, end_utc)`；相邻集合必须首尾相接。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeWindow {
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
}

/// 四段时间计划由调用方显式或确定性生成，Rust 合同负责拒绝间隙、重叠和乱序。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalSplitPlan {
    pub train: TimeWindow,
    pub validation: TimeWindow,
    pub calibration: TimeWindow,
    pub final_test: TimeWindow,
}

/// train/validation/calibration 可供开发流程显式读取。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevelopmentSplit {
    pub window: TimeWindow,
    pub series_ids: Vec<String>,
}

/// final test 在调参阶段只发布 commitment，不发布成员 ID。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SealedFinalTestSplit {
    pub window: TimeWindow,
    pub series_count: u32,
    pub membership_sha256: String,
    pub access_policy: String,
}

/// M3R 恢复划分额外固定旧 Final 的不可复用证据；不序列化旧成员 ID。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverySplitContext {
    pub contract_version: u16,
    pub reference_split_manifest_sha256: String,
    pub retired_source_dataset_sha256: String,
    pub retired_final_window: TimeWindow,
    pub retired_final_series_count: u32,
    pub retired_final_membership_sha256: String,
    pub member_overlap_count: u32,
    pub temporal_overlap_count: u32,
    pub independence_policy: String,
}

/// 调参阶段唯一允许消费的划分合同；final_test 的类型中不存在 `series_ids` 字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalSplitManifest {
    pub manifest_version: u16,
    pub source_dataset_sha256: String,
    pub train: DevelopmentSplit,
    pub validation: DevelopmentSplit,
    pub calibration: DevelopmentSplit,
    pub final_test: SealedFinalTestSplit,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<RecoverySplitContext>,
}

/// release 前必须冻结三个独立输入，避免看过 final test 后静默覆盖模型或评估代码。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalTestReleaseAuthorization {
    pub frozen_at_utc: DateTime<Utc>,
    pub model_artifact_sha256: String,
    pub model_config_sha256: String,
    pub evaluation_code_sha256: String,
}

/// 只有显式 release 路径会产生带成员 ID 的 final test manifest。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleasedFinalTestManifest {
    pub source_dataset_sha256: String,
    pub window: TimeWindow,
    pub membership_sha256: String,
    pub series_ids: Vec<String>,
    pub authorization: FinalTestReleaseAuthorization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporalSplitError {
    UnsupportedVersion(u16),
    EmptyField(&'static str),
    InvalidSha256(&'static str),
    InvalidWindow(&'static str),
    NonContiguousWindows(&'static str, &'static str),
    DuplicateSeries(String),
    CandidateOutsidePlan(String),
    EmptySplit(&'static str),
    SeriesCountOverflow,
    InvalidAccessPolicy,
    FinalTestCommitmentMismatch,
    UnsupportedRecoveryVersion(u16),
    InvalidRecoveryPolicy,
    RecoverySourceNotIndependent,
    RecoveryMemberOverlap(u32),
    RecoveryTemporalOverlap(u32),
}

impl fmt::Display for TemporalSplitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported temporal split manifest version: {version}"
                )
            }
            Self::EmptyField(field) => write!(formatter, "required field is empty: {field}"),
            Self::InvalidSha256(field) => {
                write!(formatter, "field must be a lowercase SHA-256 hash: {field}")
            }
            Self::InvalidWindow(name) => {
                write!(formatter, "split window must have start < end: {name}")
            }
            Self::NonContiguousWindows(left, right) => write!(
                formatter,
                "split windows must be contiguous and ordered: {left} -> {right}"
            ),
            Self::DuplicateSeries(series_id) => {
                write!(formatter, "series occurs more than once: {series_id}")
            }
            Self::CandidateOutsidePlan(series_id) => {
                write!(
                    formatter,
                    "series does not belong to exactly one split: {series_id}"
                )
            }
            Self::EmptySplit(name) => write!(formatter, "split must not be empty: {name}"),
            Self::SeriesCountOverflow => formatter.write_str("final test series count exceeds u32"),
            Self::InvalidAccessPolicy => formatter
                .write_str("final test access policy must remain sealed_until_model_freeze"),
            Self::FinalTestCommitmentMismatch => {
                formatter.write_str("final test membership does not match the sealed commitment")
            }
            Self::UnsupportedRecoveryVersion(version) => {
                write!(
                    formatter,
                    "unsupported recovery split contract version: {version}"
                )
            }
            Self::InvalidRecoveryPolicy => {
                formatter.write_str("recovery split independence policy is invalid")
            }
            Self::RecoverySourceNotIndependent => formatter
                .write_str("recovery source dataset must differ from the retired source dataset"),
            Self::RecoveryMemberOverlap(count) => write!(
                formatter,
                "retired final members overlap the recovery corpus: {count}"
            ),
            Self::RecoveryTemporalOverlap(count) => write!(
                formatter,
                "recovery candidates overlap the retired final time range: {count}"
            ),
        }
    }
}

impl Error for TemporalSplitError {}

impl TimeWindow {
    fn contains(&self, value: DateTime<Utc>) -> bool {
        self.start_utc <= value && value < self.end_utc
    }

    fn validate(&self, name: &'static str) -> Result<(), TemporalSplitError> {
        if self.start_utc >= self.end_utc {
            return Err(TemporalSplitError::InvalidWindow(name));
        }
        Ok(())
    }
}

impl TemporalSplitPlan {
    pub fn validate(&self) -> Result<(), TemporalSplitError> {
        for (name, window) in [
            ("train", &self.train),
            ("validation", &self.validation),
            ("calibration", &self.calibration),
            ("final_test", &self.final_test),
        ] {
            window.validate(name)?;
        }
        for (left_name, left, right_name, right) in [
            ("train", &self.train, "validation", &self.validation),
            (
                "validation",
                &self.validation,
                "calibration",
                &self.calibration,
            ),
            (
                "calibration",
                &self.calibration,
                "final_test",
                &self.final_test,
            ),
        ] {
            if left.end_utc != right.start_utc {
                return Err(TemporalSplitError::NonContiguousWindows(
                    left_name, right_name,
                ));
            }
        }
        Ok(())
    }
}

impl TemporalSplitManifest {
    pub fn validate(&self) -> Result<(), TemporalSplitError> {
        if self.manifest_version != TEMPORAL_SPLIT_MANIFEST_VERSION {
            return Err(TemporalSplitError::UnsupportedVersion(
                self.manifest_version,
            ));
        }
        require_sha256("source_dataset_sha256", &self.source_dataset_sha256)?;
        let plan = TemporalSplitPlan {
            train: self.train.window.clone(),
            validation: self.validation.window.clone(),
            calibration: self.calibration.window.clone(),
            final_test: self.final_test.window.clone(),
        };
        plan.validate()?;
        for (name, split) in [
            ("train", &self.train),
            ("validation", &self.validation),
            ("calibration", &self.calibration),
        ] {
            if split.series_ids.is_empty() {
                return Err(TemporalSplitError::EmptySplit(name));
            }
            if split.series_ids.iter().any(|value| value.trim().is_empty()) {
                return Err(TemporalSplitError::EmptyField("series_ids"));
            }
        }
        let mut development_ids = BTreeSet::new();
        for series_id in self.development_series_ids() {
            if !development_ids.insert(series_id) {
                return Err(TemporalSplitError::DuplicateSeries(series_id.clone()));
            }
        }
        if self.final_test.series_count == 0 {
            return Err(TemporalSplitError::EmptySplit("final_test"));
        }
        require_sha256(
            "final_test.membership_sha256",
            &self.final_test.membership_sha256,
        )?;
        if self.final_test.access_policy != FINAL_TEST_ACCESS_POLICY {
            return Err(TemporalSplitError::InvalidAccessPolicy);
        }
        if let Some(recovery) = &self.recovery {
            if recovery.contract_version != RECOVERY_SPLIT_CONTRACT_VERSION {
                return Err(TemporalSplitError::UnsupportedRecoveryVersion(
                    recovery.contract_version,
                ));
            }
            require_sha256(
                "recovery.reference_split_manifest_sha256",
                &recovery.reference_split_manifest_sha256,
            )?;
            require_sha256(
                "recovery.retired_source_dataset_sha256",
                &recovery.retired_source_dataset_sha256,
            )?;
            require_sha256(
                "recovery.retired_final_membership_sha256",
                &recovery.retired_final_membership_sha256,
            )?;
            recovery.retired_final_window.validate("retired_final")?;
            if recovery.retired_final_series_count == 0 {
                return Err(TemporalSplitError::EmptySplit("retired_final"));
            }
            if recovery.independence_policy != RECOVERY_INDEPENDENCE_POLICY {
                return Err(TemporalSplitError::InvalidRecoveryPolicy);
            }
            if recovery.retired_source_dataset_sha256 == self.source_dataset_sha256 {
                return Err(TemporalSplitError::RecoverySourceNotIndependent);
            }
            if recovery.member_overlap_count != 0 {
                return Err(TemporalSplitError::RecoveryMemberOverlap(
                    recovery.member_overlap_count,
                ));
            }
            if recovery.temporal_overlap_count != 0
                || recovery.retired_final_window.end_utc > self.train.window.start_utc
            {
                return Err(TemporalSplitError::RecoveryTemporalOverlap(
                    recovery.temporal_overlap_count,
                ));
            }
        }
        Ok(())
    }

    /// 调参入口只暴露三个 development splits；final test 没有旁路 getter。
    pub fn development_series_ids(&self) -> impl Iterator<Item = &String> {
        self.train
            .series_ids
            .iter()
            .chain(&self.validation.series_ids)
            .chain(&self.calibration.series_ids)
    }
}

/// 以半开时间窗确定性划分；输入顺序不会改变各集合顺序或 final test commitment。
pub fn build_temporal_split_manifest(
    mut candidates: Vec<TemporalSplitCandidate>,
    plan: TemporalSplitPlan,
    source_dataset_sha256: String,
) -> Result<TemporalSplitManifest, TemporalSplitError> {
    require_sha256("source_dataset_sha256", &source_dataset_sha256)?;
    plan.validate()?;

    candidates.sort_by(|left, right| {
        (left.scheduled_start_utc, left.series_id.as_str())
            .cmp(&(right.scheduled_start_utc, right.series_id.as_str()))
    });
    let mut seen_ids = BTreeSet::new();
    for candidate in &candidates {
        if candidate.series_id.trim().is_empty() {
            return Err(TemporalSplitError::EmptyField("series_id"));
        }
        if !seen_ids.insert(candidate.series_id.as_str()) {
            return Err(TemporalSplitError::DuplicateSeries(
                candidate.series_id.clone(),
            ));
        }
    }

    let mut train = Vec::new();
    let mut validation = Vec::new();
    let mut calibration = Vec::new();
    let mut final_test = Vec::new();
    for candidate in candidates {
        let memberships = [
            plan.train.contains(candidate.scheduled_start_utc),
            plan.validation.contains(candidate.scheduled_start_utc),
            plan.calibration.contains(candidate.scheduled_start_utc),
            plan.final_test.contains(candidate.scheduled_start_utc),
        ];
        if memberships.iter().filter(|is_member| **is_member).count() != 1 {
            return Err(TemporalSplitError::CandidateOutsidePlan(
                candidate.series_id,
            ));
        }
        match memberships {
            [true, false, false, false] => train.push(candidate.series_id),
            [false, true, false, false] => validation.push(candidate.series_id),
            [false, false, true, false] => calibration.push(candidate.series_id),
            [false, false, false, true] => final_test.push(candidate),
            _ => unreachable!("exactly one membership was checked above"),
        }
    }

    for (name, count) in [
        ("train", train.len()),
        ("validation", validation.len()),
        ("calibration", calibration.len()),
        ("final_test", final_test.len()),
    ] {
        if count == 0 {
            return Err(TemporalSplitError::EmptySplit(name));
        }
    }
    let final_test_count =
        u32::try_from(final_test.len()).map_err(|_| TemporalSplitError::SeriesCountOverflow)?;
    let final_test_membership_sha256 = membership_sha256(&final_test);

    let manifest = TemporalSplitManifest {
        manifest_version: TEMPORAL_SPLIT_MANIFEST_VERSION,
        source_dataset_sha256,
        train: DevelopmentSplit {
            window: plan.train,
            series_ids: train,
        },
        validation: DevelopmentSplit {
            window: plan.validation,
            series_ids: validation,
        },
        calibration: DevelopmentSplit {
            window: plan.calibration,
            series_ids: calibration,
        },
        final_test: SealedFinalTestSplit {
            window: plan.final_test,
            series_count: final_test_count,
            membership_sha256: final_test_membership_sha256,
            access_policy: FINAL_TEST_ACCESS_POLICY.to_owned(),
        },
        recovery: None,
    };
    manifest.validate()?;
    Ok(manifest)
}

/// 恢复划分在普通时间合同之上，重新核对旧 Final commitment，并证明整个新 corpus 与其成员和时间均不重叠。
pub fn build_recovery_temporal_split_manifest(
    candidates: Vec<TemporalSplitCandidate>,
    plan: TemporalSplitPlan,
    source_dataset_sha256: String,
    reference_candidates: &[TemporalSplitCandidate],
    reference_manifest: &TemporalSplitManifest,
    reference_split_manifest_sha256: String,
) -> Result<TemporalSplitManifest, TemporalSplitError> {
    require_sha256(
        "recovery.reference_split_manifest_sha256",
        &reference_split_manifest_sha256,
    )?;
    let retired_final = validated_final_candidates(reference_manifest, reference_candidates)?;
    let retired_ids = retired_final
        .iter()
        .map(|candidate| candidate.series_id.as_str())
        .collect::<BTreeSet<_>>();
    let member_overlap_count = checked_count(
        candidates
            .iter()
            .filter(|candidate| retired_ids.contains(candidate.series_id.as_str()))
            .count(),
    )?;
    if member_overlap_count != 0 {
        return Err(TemporalSplitError::RecoveryMemberOverlap(
            member_overlap_count,
        ));
    }
    let temporal_overlap_count = checked_count(
        candidates
            .iter()
            .filter(|candidate| {
                candidate.scheduled_start_utc < reference_manifest.final_test.window.end_utc
            })
            .count(),
    )?;
    if temporal_overlap_count != 0 {
        return Err(TemporalSplitError::RecoveryTemporalOverlap(
            temporal_overlap_count,
        ));
    }

    let mut manifest = build_temporal_split_manifest(candidates, plan, source_dataset_sha256)?;
    manifest.recovery = Some(RecoverySplitContext {
        contract_version: RECOVERY_SPLIT_CONTRACT_VERSION,
        reference_split_manifest_sha256,
        retired_source_dataset_sha256: reference_manifest.source_dataset_sha256.clone(),
        retired_final_window: reference_manifest.final_test.window.clone(),
        retired_final_series_count: reference_manifest.final_test.series_count,
        retired_final_membership_sha256: reference_manifest.final_test.membership_sha256.clone(),
        member_overlap_count,
        temporal_overlap_count,
        independence_policy: RECOVERY_INDEPENDENCE_POLICY.to_owned(),
    });
    manifest.validate()?;
    Ok(manifest)
}

/// release 会重新从源数据计算 final test 成员并核对 seal；任何源数据漂移都会拒绝。
pub fn release_final_test(
    manifest: &TemporalSplitManifest,
    candidates: &[TemporalSplitCandidate],
    authorization: FinalTestReleaseAuthorization,
) -> Result<ReleasedFinalTestManifest, TemporalSplitError> {
    manifest.validate()?;
    require_sha256(
        "authorization.model_artifact_sha256",
        &authorization.model_artifact_sha256,
    )?;
    require_sha256(
        "authorization.model_config_sha256",
        &authorization.model_config_sha256,
    )?;
    require_sha256(
        "authorization.evaluation_code_sha256",
        &authorization.evaluation_code_sha256,
    )?;

    let final_candidates = validated_final_candidates(manifest, candidates)?;

    Ok(ReleasedFinalTestManifest {
        source_dataset_sha256: manifest.source_dataset_sha256.clone(),
        window: manifest.final_test.window.clone(),
        membership_sha256: manifest.final_test.membership_sha256.clone(),
        series_ids: final_candidates
            .into_iter()
            .map(|candidate| candidate.series_id)
            .collect(),
        authorization,
    })
}

fn validated_final_candidates(
    manifest: &TemporalSplitManifest,
    candidates: &[TemporalSplitCandidate],
) -> Result<Vec<TemporalSplitCandidate>, TemporalSplitError> {
    manifest.validate()?;
    let mut final_candidates = candidates
        .iter()
        .filter(|candidate| {
            manifest
                .final_test
                .window
                .contains(candidate.scheduled_start_utc)
        })
        .cloned()
        .collect::<Vec<_>>();
    final_candidates.sort_by(|left, right| {
        (left.scheduled_start_utc, left.series_id.as_str())
            .cmp(&(right.scheduled_start_utc, right.series_id.as_str()))
    });
    let commitment = membership_sha256(&final_candidates);
    if final_candidates.len() != manifest.final_test.series_count as usize
        || commitment != manifest.final_test.membership_sha256
    {
        return Err(TemporalSplitError::FinalTestCommitmentMismatch);
    }
    Ok(final_candidates)
}

fn checked_count(count: usize) -> Result<u32, TemporalSplitError> {
    u32::try_from(count).map_err(|_| TemporalSplitError::SeriesCountOverflow)
}

fn membership_sha256(candidates: &[TemporalSplitCandidate]) -> String {
    let mut hasher = Sha256::new();
    for candidate in candidates {
        hasher.update(candidate.scheduled_start_utc.to_rfc3339());
        hasher.update(b"\t");
        hasher.update(candidate.series_id.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn require_sha256(field: &'static str, value: &str) -> Result<(), TemporalSplitError> {
    if value.len() != 64
        || !value
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
    {
        return Err(TemporalSplitError::InvalidSha256(field));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp must be valid")
            .with_timezone(&Utc)
    }

    fn plan() -> TemporalSplitPlan {
        TemporalSplitPlan {
            train: TimeWindow {
                start_utc: utc("2026-01-01T00:00:00Z"),
                end_utc: utc("2026-02-01T00:00:00Z"),
            },
            validation: TimeWindow {
                start_utc: utc("2026-02-01T00:00:00Z"),
                end_utc: utc("2026-03-01T00:00:00Z"),
            },
            calibration: TimeWindow {
                start_utc: utc("2026-03-01T00:00:00Z"),
                end_utc: utc("2026-04-01T00:00:00Z"),
            },
            final_test: TimeWindow {
                start_utc: utc("2026-04-01T00:00:00Z"),
                end_utc: utc("2026-05-01T00:00:00Z"),
            },
        }
    }

    fn candidates() -> Vec<TemporalSplitCandidate> {
        vec![
            TemporalSplitCandidate {
                series_id: "train-b".to_owned(),
                scheduled_start_utc: utc("2026-01-20T00:00:00Z"),
            },
            TemporalSplitCandidate {
                series_id: "final".to_owned(),
                scheduled_start_utc: utc("2026-04-10T00:00:00Z"),
            },
            TemporalSplitCandidate {
                series_id: "validation".to_owned(),
                scheduled_start_utc: utc("2026-02-10T00:00:00Z"),
            },
            TemporalSplitCandidate {
                series_id: "train-a".to_owned(),
                scheduled_start_utc: utc("2026-01-10T00:00:00Z"),
            },
            TemporalSplitCandidate {
                series_id: "calibration".to_owned(),
                scheduled_start_utc: utc("2026-03-10T00:00:00Z"),
            },
        ]
    }

    fn build() -> TemporalSplitManifest {
        build_temporal_split_manifest(candidates(), plan(), "a".repeat(64))
            .expect("valid split must build")
    }

    fn retired_manifest() -> (TemporalSplitManifest, Vec<TemporalSplitCandidate>) {
        let retired_plan = TemporalSplitPlan {
            train: TimeWindow {
                start_utc: utc("2025-01-01T00:00:00Z"),
                end_utc: utc("2025-02-01T00:00:00Z"),
            },
            validation: TimeWindow {
                start_utc: utc("2025-02-01T00:00:00Z"),
                end_utc: utc("2025-03-01T00:00:00Z"),
            },
            calibration: TimeWindow {
                start_utc: utc("2025-03-01T00:00:00Z"),
                end_utc: utc("2025-04-01T00:00:00Z"),
            },
            final_test: TimeWindow {
                start_utc: utc("2025-04-01T00:00:00Z"),
                end_utc: utc("2025-05-01T00:00:00Z"),
            },
        };
        let retired_candidates = vec![
            TemporalSplitCandidate {
                series_id: "retired-train".to_owned(),
                scheduled_start_utc: utc("2025-01-10T00:00:00Z"),
            },
            TemporalSplitCandidate {
                series_id: "retired-validation".to_owned(),
                scheduled_start_utc: utc("2025-02-10T00:00:00Z"),
            },
            TemporalSplitCandidate {
                series_id: "retired-calibration".to_owned(),
                scheduled_start_utc: utc("2025-03-10T00:00:00Z"),
            },
            TemporalSplitCandidate {
                series_id: "retired-final".to_owned(),
                scheduled_start_utc: utc("2025-04-10T00:00:00Z"),
            },
        ];
        let manifest =
            build_temporal_split_manifest(retired_candidates.clone(), retired_plan, "e".repeat(64))
                .expect("retired split must build");
        (manifest, retired_candidates)
    }

    #[test]
    fn assigns_every_series_once_in_chronological_order() {
        let manifest = build();
        assert_eq!(manifest.train.series_ids, ["train-a", "train-b"]);
        assert_eq!(manifest.validation.series_ids, ["validation"]);
        assert_eq!(manifest.calibration.series_ids, ["calibration"]);
        assert_eq!(manifest.final_test.series_count, 1);
        assert_eq!(manifest.development_series_ids().count(), 4);
    }

    #[test]
    fn serialized_development_manifest_does_not_expose_final_test_ids() {
        let value = serde_json::to_value(build()).expect("manifest must serialize");
        assert!(value["final_test"].get("series_ids").is_none());
        assert_eq!(
            value["final_test"]["access_policy"],
            FINAL_TEST_ACCESS_POLICY
        );
        assert!(!value.to_string().contains("\"final\""));
    }

    #[test]
    fn rejects_gap_overlap_duplicate_and_outside_candidate() {
        let mut gap = plan();
        gap.validation.start_utc = utc("2026-02-02T00:00:00Z");
        assert_eq!(
            gap.validate(),
            Err(TemporalSplitError::NonContiguousWindows(
                "train",
                "validation"
            ))
        );

        let mut overlap = plan();
        overlap.validation.start_utc = utc("2026-01-31T00:00:00Z");
        assert_eq!(
            overlap.validate(),
            Err(TemporalSplitError::NonContiguousWindows(
                "train",
                "validation"
            ))
        );

        let mut duplicate = candidates();
        duplicate.push(duplicate[0].clone());
        assert_eq!(
            build_temporal_split_manifest(duplicate, plan(), "a".repeat(64)),
            Err(TemporalSplitError::DuplicateSeries("train-b".to_owned()))
        );

        let mut outside = candidates();
        outside.push(TemporalSplitCandidate {
            series_id: "outside".to_owned(),
            scheduled_start_utc: utc("2026-05-01T00:00:00Z"),
        });
        assert_eq!(
            build_temporal_split_manifest(outside, plan(), "a".repeat(64)),
            Err(TemporalSplitError::CandidateOutsidePlan(
                "outside".to_owned()
            ))
        );
    }

    #[test]
    fn refuses_empty_split() {
        let candidates = candidates()
            .into_iter()
            .filter(|candidate| candidate.series_id != "calibration")
            .collect();
        assert_eq!(
            build_temporal_split_manifest(candidates, plan(), "a".repeat(64)),
            Err(TemporalSplitError::EmptySplit("calibration"))
        );
    }

    #[test]
    fn final_test_release_requires_frozen_hashes_and_matching_commitment() {
        let manifest = build();
        let authorization = FinalTestReleaseAuthorization {
            frozen_at_utc: utc("2026-05-02T00:00:00Z"),
            model_artifact_sha256: "b".repeat(64),
            model_config_sha256: "c".repeat(64),
            evaluation_code_sha256: "d".repeat(64),
        };
        let released = release_final_test(&manifest, &candidates(), authorization.clone())
            .expect("frozen release must pass");
        assert_eq!(released.series_ids, ["final"]);
        assert_eq!(released.authorization, authorization);

        let mut invalid_authorization = released.authorization;
        invalid_authorization.model_config_sha256 = "not-a-hash".to_owned();
        assert_eq!(
            release_final_test(&manifest, &candidates(), invalid_authorization),
            Err(TemporalSplitError::InvalidSha256(
                "authorization.model_config_sha256"
            ))
        );
    }

    #[test]
    fn final_test_release_detects_source_membership_drift() {
        let manifest = build();
        let authorization = FinalTestReleaseAuthorization {
            frozen_at_utc: utc("2026-05-02T00:00:00Z"),
            model_artifact_sha256: "b".repeat(64),
            model_config_sha256: "c".repeat(64),
            evaluation_code_sha256: "d".repeat(64),
        };
        let drifted = candidates()
            .into_iter()
            .map(|mut candidate| {
                if candidate.series_id == "final" {
                    candidate.series_id = "changed-final".to_owned();
                }
                candidate
            })
            .collect::<Vec<_>>();

        assert_eq!(
            release_final_test(&manifest, &drifted, authorization),
            Err(TemporalSplitError::FinalTestCommitmentMismatch)
        );
    }

    #[test]
    fn recovery_split_seals_retired_final_exclusion_without_exposing_ids() {
        let (retired, retired_candidates) = retired_manifest();
        let manifest = build_recovery_temporal_split_manifest(
            candidates(),
            plan(),
            "a".repeat(64),
            &retired_candidates,
            &retired,
            "f".repeat(64),
        )
        .expect("independent recovery split must build");

        let recovery = manifest.recovery.as_ref().expect("recovery must exist");
        assert_eq!(recovery.member_overlap_count, 0);
        assert_eq!(recovery.temporal_overlap_count, 0);
        assert_eq!(
            recovery.retired_final_membership_sha256,
            retired.final_test.membership_sha256
        );
        let serialized = serde_json::to_string(&manifest).expect("manifest must serialize");
        assert!(!serialized.contains("retired-final"));
        assert!(!serialized.contains("\"series_ids\":[\"final\"]"));
    }

    #[test]
    fn recovery_split_rejects_retired_final_member_or_time_overlap() {
        let (retired, retired_candidates) = retired_manifest();
        let mut member_overlap = candidates();
        member_overlap[0].series_id = "retired-final".to_owned();
        assert_eq!(
            build_recovery_temporal_split_manifest(
                member_overlap,
                plan(),
                "a".repeat(64),
                &retired_candidates,
                &retired,
                "f".repeat(64),
            ),
            Err(TemporalSplitError::RecoveryMemberOverlap(1))
        );

        let mut time_overlap = candidates();
        time_overlap[0].scheduled_start_utc = utc("2025-04-20T00:00:00Z");
        assert_eq!(
            build_recovery_temporal_split_manifest(
                time_overlap,
                plan(),
                "a".repeat(64),
                &retired_candidates,
                &retired,
                "f".repeat(64),
            ),
            Err(TemporalSplitError::RecoveryTemporalOverlap(1))
        );
    }
}
