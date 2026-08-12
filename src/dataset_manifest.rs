use std::{collections::BTreeSet, error::Error, fmt};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// HIST-001 首版 manifest 合同版本；不兼容变更必须提升版本号。
pub const DATASET_MANIFEST_VERSION: u16 = 1;

/// 一个 processed dataset 的完整来源链；未知字段由 Serde 拒绝，避免格式漂移被静默忽略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetManifest {
    pub manifest_version: u16,
    pub dataset: DatasetIdentity,
    pub generated_at_utc: DateTime<Utc>,
    pub code: CodeProvenance,
    pub generator: GeneratorProvenance,
    pub raw_inputs: Vec<RawInput>,
    pub output: ProcessedOutput,
}

/// 数据集的逻辑名称与不可变版本目录名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetIdentity {
    pub name: String,
    pub version: String,
}

/// 生成数据时的代码状态；dirty 工作区必须额外记录 diff hash，不能只冒充某个 commit。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeProvenance {
    pub git_commit: String,
    pub dirty: bool,
    pub diff_sha256: Option<String>,
}

/// 实际生成入口及参数；entrypoint 必须是仓库相对路径，避免 manifest 绑定个人机器绝对路径。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratorProvenance {
    pub entrypoint: String,
    pub arguments: Vec<String>,
}

/// processed dataset 直接依赖的不可变 raw 文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawInput {
    pub source: String,
    pub relative_path: String,
    pub sha256: String,
    pub captured_at_utc: DateTime<Utc>,
}

/// processed 文件的内容摘要和数据范围；空数据集不允许伪装成成功产物。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessedOutput {
    pub relative_path: String,
    pub sha256: String,
    pub row_count: u64,
    pub event_time_range_utc: UtcTimeRange,
}

/// 数据集中 Event 时间的闭区间，不代表 manifest 的生成时间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UtcTimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetManifestError {
    UnsupportedVersion(u16),
    EmptyField(&'static str),
    InvalidDatasetSegment(&'static str),
    InvalidGitCommit,
    MissingDirtyDiffHash,
    UnexpectedCleanDiffHash,
    InvalidSha256(&'static str),
    InvalidRepoPath {
        field: &'static str,
        expected_root: Option<&'static str>,
    },
    MissingRawInputs,
    DuplicateRawInputPath(String),
    RawCapturedAfterGeneration(String),
    ZeroRowCount,
    InvalidTimeRange,
}

impl fmt::Display for DatasetManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported dataset manifest version: {version}")
            }
            Self::EmptyField(field) => write!(formatter, "required field is empty: {field}"),
            Self::InvalidDatasetSegment(field) => write!(
                formatter,
                "dataset name or version is not a safe path segment: {field}"
            ),
            Self::InvalidGitCommit => formatter.write_str(
                "code.git_commit must be a lowercase 40- or 64-character hexadecimal hash",
            ),
            Self::MissingDirtyDiffHash => formatter
                .write_str("code.diff_sha256 is required when the generating worktree is dirty"),
            Self::UnexpectedCleanDiffHash => formatter
                .write_str("code.diff_sha256 must be absent when the generating worktree is clean"),
            Self::InvalidSha256(field) => {
                write!(formatter, "field must be a lowercase SHA-256 hash: {field}")
            }
            Self::InvalidRepoPath {
                field,
                expected_root,
            } => match expected_root {
                Some(root) => write!(
                    formatter,
                    "field must be a portable repository-relative path under {root}: {field}"
                ),
                None => write!(
                    formatter,
                    "field must be a portable repository-relative path: {field}"
                ),
            },
            Self::MissingRawInputs => {
                formatter.write_str("processed dataset must reference at least one raw input")
            }
            Self::DuplicateRawInputPath(path) => {
                write!(formatter, "raw input path is duplicated: {path}")
            }
            Self::RawCapturedAfterGeneration(path) => write!(
                formatter,
                "raw input capture time is after manifest generation time: {path}"
            ),
            Self::ZeroRowCount => {
                formatter.write_str("processed dataset row_count must be greater than zero")
            }
            Self::InvalidTimeRange => {
                formatter.write_str("output.event_time_range_utc.start must not be after its end")
            }
        }
    }
}

impl Error for DatasetManifestError {}

impl DatasetManifest {
    /// 统一校验来源、代码和输出链路；任何无法追溯的 processed dataset 都 fail closed。
    pub fn validate(&self) -> Result<(), DatasetManifestError> {
        if self.manifest_version != DATASET_MANIFEST_VERSION {
            return Err(DatasetManifestError::UnsupportedVersion(
                self.manifest_version,
            ));
        }

        validate_dataset_segment("dataset.name", &self.dataset.name)?;
        validate_dataset_segment("dataset.version", &self.dataset.version)?;

        if !is_git_hash(&self.code.git_commit) {
            return Err(DatasetManifestError::InvalidGitCommit);
        }
        match (self.code.dirty, self.code.diff_sha256.as_deref()) {
            (true, Some(hash)) if is_sha256(hash) => {}
            (true, Some(_)) => {
                return Err(DatasetManifestError::InvalidSha256("code.diff_sha256"));
            }
            (true, None) => return Err(DatasetManifestError::MissingDirtyDiffHash),
            (false, Some(_)) => return Err(DatasetManifestError::UnexpectedCleanDiffHash),
            (false, None) => {}
        }

        validate_repo_relative_path("generator.entrypoint", &self.generator.entrypoint, None)?;

        if self.raw_inputs.is_empty() {
            return Err(DatasetManifestError::MissingRawInputs);
        }
        let mut raw_paths = BTreeSet::new();
        for raw_input in &self.raw_inputs {
            if raw_input.source.trim().is_empty() {
                return Err(DatasetManifestError::EmptyField("raw_inputs.source"));
            }
            validate_repo_relative_path(
                "raw_inputs.relative_path",
                &raw_input.relative_path,
                Some("data/raw/"),
            )?;
            if !is_sha256(&raw_input.sha256) {
                return Err(DatasetManifestError::InvalidSha256("raw_inputs.sha256"));
            }
            if !raw_paths.insert(raw_input.relative_path.as_str()) {
                return Err(DatasetManifestError::DuplicateRawInputPath(
                    raw_input.relative_path.clone(),
                ));
            }
            if raw_input.captured_at_utc > self.generated_at_utc {
                return Err(DatasetManifestError::RawCapturedAfterGeneration(
                    raw_input.relative_path.clone(),
                ));
            }
        }

        validate_repo_relative_path(
            "output.relative_path",
            &self.output.relative_path,
            Some("data/processed/"),
        )?;
        if !is_sha256(&self.output.sha256) {
            return Err(DatasetManifestError::InvalidSha256("output.sha256"));
        }
        if self.output.row_count == 0 {
            return Err(DatasetManifestError::ZeroRowCount);
        }
        if self.output.event_time_range_utc.start > self.output.event_time_range_utc.end {
            return Err(DatasetManifestError::InvalidTimeRange);
        }

        Ok(())
    }
}

fn validate_dataset_segment(field: &'static str, value: &str) -> Result<(), DatasetManifestError> {
    if value.trim().is_empty() {
        return Err(DatasetManifestError::EmptyField(field));
    }
    if matches!(value, "." | "..")
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(DatasetManifestError::InvalidDatasetSegment(field));
    }
    Ok(())
}

fn validate_repo_relative_path(
    field: &'static str,
    value: &str,
    expected_root: Option<&'static str>,
) -> Result<(), DatasetManifestError> {
    let is_portable_relative = !value.trim().is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."));
    let is_under_root = expected_root.is_none_or(|root| value.starts_with(root));
    if !is_portable_relative || !is_under_root {
        return Err(DatasetManifestError::InvalidRepoPath {
            field,
            expected_root,
        });
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
}

fn is_git_hash(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .chars()
            .all(|character| character.is_ascii_hexdigit() && !character.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp must be valid")
            .with_timezone(&Utc)
    }

    fn valid_manifest() -> DatasetManifest {
        DatasetManifest {
            manifest_version: DATASET_MANIFEST_VERSION,
            dataset: DatasetIdentity {
                name: "lol-series-results".to_owned(),
                version: "2026-08-12.dee62ca".to_owned(),
            },
            generated_at_utc: utc("2026-08-12T12:00:00Z"),
            code: CodeProvenance {
                git_commit: "d".repeat(40),
                dirty: false,
                diff_sha256: None,
            },
            generator: GeneratorProvenance {
                entrypoint: "research/build_series_dataset.ps1".to_owned(),
                arguments: vec!["-Season".to_owned(), "2025".to_owned()],
            },
            raw_inputs: vec![RawInput {
                source: "oracles_elixir".to_owned(),
                relative_path: "data/raw/oracles_elixir/source/2025.csv".to_owned(),
                sha256: "a".repeat(64),
                captured_at_utc: utc("2026-08-12T11:00:00Z"),
            }],
            output: ProcessedOutput {
                relative_path: "data/processed/lol-series-results/2026-08-12.dee62ca/series.csv"
                    .to_owned(),
                sha256: "b".repeat(64),
                row_count: 500,
                event_time_range_utc: UtcTimeRange {
                    start: utc("2025-01-01T00:00:00Z"),
                    end: utc("2025-12-31T23:59:59Z"),
                },
            },
        }
    }

    #[test]
    fn accepts_traceable_processed_dataset() {
        assert_eq!(valid_manifest().validate(), Ok(()));
    }

    #[test]
    fn rejects_processed_dataset_without_raw_input() {
        let mut manifest = valid_manifest();
        manifest.raw_inputs.clear();

        assert_eq!(
            manifest.validate(),
            Err(DatasetManifestError::MissingRawInputs)
        );
    }

    #[test]
    fn rejects_raw_input_outside_raw_root() {
        let mut manifest = valid_manifest();
        manifest.raw_inputs[0].relative_path = "data/processed/leaked.csv".to_owned();

        assert_eq!(
            manifest.validate(),
            Err(DatasetManifestError::InvalidRepoPath {
                field: "raw_inputs.relative_path",
                expected_root: Some("data/raw/"),
            })
        );
    }

    #[test]
    fn rejects_processed_output_outside_processed_root() {
        let mut manifest = valid_manifest();
        manifest.output.relative_path = "artifacts/model.bin".to_owned();

        assert_eq!(
            manifest.validate(),
            Err(DatasetManifestError::InvalidRepoPath {
                field: "output.relative_path",
                expected_root: Some("data/processed/"),
            })
        );
    }

    #[test]
    fn rejects_dirty_code_without_diff_hash() {
        let mut manifest = valid_manifest();
        manifest.code.dirty = true;

        assert_eq!(
            manifest.validate(),
            Err(DatasetManifestError::MissingDirtyDiffHash)
        );
    }

    #[test]
    fn rejects_duplicate_raw_input_path() {
        let mut manifest = valid_manifest();
        manifest.raw_inputs.push(manifest.raw_inputs[0].clone());

        assert_eq!(
            manifest.validate(),
            Err(DatasetManifestError::DuplicateRawInputPath(
                "data/raw/oracles_elixir/source/2025.csv".to_owned()
            ))
        );
    }

    #[test]
    fn rejects_raw_input_captured_after_generation() {
        let mut manifest = valid_manifest();
        manifest.raw_inputs[0].captured_at_utc = utc("2026-08-12T12:00:01Z");

        assert_eq!(
            manifest.validate(),
            Err(DatasetManifestError::RawCapturedAfterGeneration(
                "data/raw/oracles_elixir/source/2025.csv".to_owned()
            ))
        );
    }

    #[test]
    fn rejects_zero_rows_and_reversed_time_range() {
        let mut empty_manifest = valid_manifest();
        empty_manifest.output.row_count = 0;
        assert_eq!(
            empty_manifest.validate(),
            Err(DatasetManifestError::ZeroRowCount)
        );

        let mut reversed_manifest = valid_manifest();
        reversed_manifest.output.event_time_range_utc = UtcTimeRange {
            start: utc("2025-12-31T23:59:59Z"),
            end: utc("2025-01-01T00:00:00Z"),
        };
        assert_eq!(
            reversed_manifest.validate(),
            Err(DatasetManifestError::InvalidTimeRange)
        );
    }
}
