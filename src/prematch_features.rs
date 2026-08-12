use std::{collections::BTreeSet, error::Error, fmt};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// HIST-004 的目标赛事投影；类型层面不接收比分、胜者或市场结算字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrematchSeriesTarget {
    pub series_id: String,
    pub competition_id: String,
    pub region: String,
    pub patch: String,
    pub scheduled_start_utc: DateTime<Utc>,
    pub best_of: u8,
    pub team_ids: [String; 2],
    pub source_team_keys: [String; 2],
}

/// 一支队伍在历史系列赛结束后才可用的结果记录；completed_at_utc 是防泄漏门禁。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamSeriesObservation {
    pub series_id: String,
    pub source_team_key: String,
    pub patch: String,
    pub scheduled_start_utc: DateTime<Utc>,
    pub completed_at_utc: DateTime<Utc>,
    pub best_of: u8,
    pub games_won: u8,
    pub games_lost: u8,
    pub series_won: bool,
}

/// 构建输入把目标赛前字段和历史结果证据分开，避免从目标 label 反向取特征。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrematchFeatureBuildInput {
    pub snapshot_lead_minutes: i64,
    pub targets: Vec<PrematchSeriesTarget>,
    pub team_series_observations: Vec<TeamSeriesObservation>,
}

/// 计数特征同时记录其最后一条来源记录时间；无历史时显式为 None。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcedCount {
    pub value: u32,
    pub source_latest_at_utc: Option<DateTime<Utc>>,
}

/// 比率使用精确分子/分母保存，避免在数据集阶段引入浮点舍入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcedRatio {
    pub numerator: u32,
    pub denominator: u32,
    pub source_latest_at_utc: Option<DateTime<Utc>>,
}

/// 距离上次系列赛完成的分钟数；没有历史时不使用任意常数填充。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcedOptionalMinutes {
    pub value: Option<i64>,
    pub source_latest_at_utc: Option<DateTime<Utc>>,
}

/// 单支队伍的最小赛前 form 特征；所有字段都能回到最新历史完成时间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamPrematchFeatures {
    pub team_id: String,
    pub source_team_key: String,
    pub prior_series_count: SourcedCount,
    pub prior_series_win_rate: SourcedRatio,
    pub prior_game_count: SourcedCount,
    pub prior_game_win_rate: SourcedRatio,
    pub same_patch_series_count: SourcedCount,
    pub same_patch_series_win_rate: SourcedRatio,
    pub rest_minutes: SourcedOptionalMinutes,
}

/// 一场比赛在固定 cutoff 生成的不可变赛前特征快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrematchFeatureSnapshot {
    pub series_id: String,
    pub competition_id: String,
    pub region: String,
    pub patch: String,
    pub scheduled_start_utc: DateTime<Utc>,
    pub snapshot_at_utc: DateTime<Utc>,
    pub best_of: u8,
    pub team_features: [TeamPrematchFeatures; 2],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrematchFeatureError {
    InvalidSnapshotLead(i64),
    EmptyField(&'static str),
    InvalidCanonicalId(&'static str),
    UnsupportedBestOf(u8),
    DuplicateTeams(String),
    InvalidObservationResult(String),
    InvalidObservationTime(String),
    DuplicateTarget(String),
    DuplicateObservation {
        series_id: String,
        source_team_key: String,
    },
    TimestampOverflow(String),
    FeatureOverflow(String),
}

impl fmt::Display for PrematchFeatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSnapshotLead(minutes) => write!(
                formatter,
                "snapshot lead must be between 1 and 1440 minutes: {minutes}"
            ),
            Self::EmptyField(field) => write!(formatter, "required field is empty: {field}"),
            Self::InvalidCanonicalId(field) => {
                write!(
                    formatter,
                    "field must contain a resolved canonical id: {field}"
                )
            }
            Self::UnsupportedBestOf(best_of) => {
                write!(
                    formatter,
                    "prematch features only accept BO3 or BO5: {best_of}"
                )
            }
            Self::DuplicateTeams(series_id) => {
                write!(formatter, "series teams must be different: {series_id}")
            }
            Self::InvalidObservationResult(series_id) => write!(
                formatter,
                "historical observation is not a completed BO3/BO5 result: {series_id}"
            ),
            Self::InvalidObservationTime(series_id) => write!(
                formatter,
                "historical observation completion must be after its scheduled start: {series_id}"
            ),
            Self::DuplicateTarget(series_id) => {
                write!(formatter, "duplicate prematch target: {series_id}")
            }
            Self::DuplicateObservation {
                series_id,
                source_team_key,
            } => write!(
                formatter,
                "duplicate team observation for series={series_id}, source_team_key={source_team_key}"
            ),
            Self::TimestampOverflow(series_id) => {
                write!(formatter, "snapshot timestamp overflow: {series_id}")
            }
            Self::FeatureOverflow(team_id) => {
                write!(formatter, "feature count exceeds u32 for team: {team_id}")
            }
        }
    }
}

impl Error for PrematchFeatureError {}

impl PrematchSeriesTarget {
    fn validate(&self) -> Result<(), PrematchFeatureError> {
        for (field, value) in [
            ("series_id", self.series_id.as_str()),
            ("region", self.region.as_str()),
            ("patch", self.patch.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(PrematchFeatureError::EmptyField(field));
            }
        }
        if !self.competition_id.starts_with("lol-competition:") {
            return Err(PrematchFeatureError::InvalidCanonicalId("competition_id"));
        }
        if self
            .team_ids
            .iter()
            .any(|team_id| !team_id.starts_with("lol-team:"))
        {
            return Err(PrematchFeatureError::InvalidCanonicalId("team_ids"));
        }
        if !matches!(self.best_of, 3 | 5) {
            return Err(PrematchFeatureError::UnsupportedBestOf(self.best_of));
        }
        if self.team_ids[0] == self.team_ids[1] {
            return Err(PrematchFeatureError::DuplicateTeams(self.series_id.clone()));
        }
        if self
            .source_team_keys
            .iter()
            .any(|source_team_key| source_team_key.trim().is_empty())
        {
            return Err(PrematchFeatureError::EmptyField("source_team_keys"));
        }
        if self.source_team_keys[0] == self.source_team_keys[1] {
            return Err(PrematchFeatureError::DuplicateTeams(self.series_id.clone()));
        }
        Ok(())
    }
}

impl TeamSeriesObservation {
    fn validate(&self) -> Result<(), PrematchFeatureError> {
        for (field, value) in [
            ("observation.series_id", self.series_id.as_str()),
            ("observation.patch", self.patch.as_str()),
            ("observation.source_team_key", self.source_team_key.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(PrematchFeatureError::EmptyField(field));
            }
        }
        if !matches!(self.best_of, 3 | 5) {
            return Err(PrematchFeatureError::UnsupportedBestOf(self.best_of));
        }
        if self.completed_at_utc <= self.scheduled_start_utc {
            return Err(PrematchFeatureError::InvalidObservationTime(
                self.series_id.clone(),
            ));
        }

        let wins_needed = self.best_of / 2 + 1;
        let valid_result = if self.series_won {
            self.games_won == wins_needed && self.games_lost < wins_needed
        } else {
            self.games_lost == wins_needed && self.games_won < wins_needed
        };
        if !valid_result || self.games_won + self.games_lost > self.best_of {
            return Err(PrematchFeatureError::InvalidObservationResult(
                self.series_id.clone(),
            ));
        }
        Ok(())
    }
}

/// 只消费 completed_at_utc 不晚于 snapshot 的记录；输入顺序不影响输出顺序和特征值。
pub fn build_prematch_feature_snapshots(
    mut input: PrematchFeatureBuildInput,
) -> Result<Vec<PrematchFeatureSnapshot>, PrematchFeatureError> {
    if !(1..=1440).contains(&input.snapshot_lead_minutes) {
        return Err(PrematchFeatureError::InvalidSnapshotLead(
            input.snapshot_lead_minutes,
        ));
    }

    let mut target_ids = BTreeSet::new();
    for target in &input.targets {
        target.validate()?;
        if !target_ids.insert(target.series_id.as_str()) {
            return Err(PrematchFeatureError::DuplicateTarget(
                target.series_id.clone(),
            ));
        }
    }

    let mut observation_keys = BTreeSet::new();
    for observation in &input.team_series_observations {
        observation.validate()?;
        let key = (
            observation.series_id.as_str(),
            observation.source_team_key.as_str(),
        );
        if !observation_keys.insert(key) {
            return Err(PrematchFeatureError::DuplicateObservation {
                series_id: observation.series_id.clone(),
                source_team_key: observation.source_team_key.clone(),
            });
        }
    }

    input.targets.sort_by(|left, right| {
        (left.scheduled_start_utc, left.series_id.as_str())
            .cmp(&(right.scheduled_start_utc, right.series_id.as_str()))
    });
    input.team_series_observations.sort_by(|left, right| {
        (
            left.source_team_key.as_str(),
            left.completed_at_utc,
            left.series_id.as_str(),
        )
            .cmp(&(
                right.source_team_key.as_str(),
                right.completed_at_utc,
                right.series_id.as_str(),
            ))
    });

    input
        .targets
        .into_iter()
        .map(|target| {
            let snapshot_at_utc = target
                .scheduled_start_utc
                .checked_sub_signed(Duration::minutes(input.snapshot_lead_minutes))
                .ok_or_else(|| PrematchFeatureError::TimestampOverflow(target.series_id.clone()))?;
            let team_1 = build_team_features(
                &target.team_ids[0],
                &target.source_team_keys[0],
                &target.patch,
                snapshot_at_utc,
                &input.team_series_observations,
            )?;
            let team_2 = build_team_features(
                &target.team_ids[1],
                &target.source_team_keys[1],
                &target.patch,
                snapshot_at_utc,
                &input.team_series_observations,
            )?;

            Ok(PrematchFeatureSnapshot {
                series_id: target.series_id,
                competition_id: target.competition_id,
                region: target.region,
                patch: target.patch,
                scheduled_start_utc: target.scheduled_start_utc,
                snapshot_at_utc,
                best_of: target.best_of,
                team_features: [team_1, team_2],
            })
        })
        .collect()
}

fn build_team_features(
    team_id: &str,
    source_team_key: &str,
    target_patch: &str,
    snapshot_at_utc: DateTime<Utc>,
    observations: &[TeamSeriesObservation],
) -> Result<TeamPrematchFeatures, PrematchFeatureError> {
    let eligible: Vec<&TeamSeriesObservation> = observations
        .iter()
        .filter(|observation| {
            observation.source_team_key == source_team_key
                && observation.completed_at_utc <= snapshot_at_utc
        })
        .collect();
    let same_patch: Vec<&TeamSeriesObservation> = eligible
        .iter()
        .copied()
        .filter(|observation| observation.patch == target_patch)
        .collect();

    let latest = eligible
        .last()
        .map(|observation| observation.completed_at_utc);
    let same_patch_latest = same_patch
        .last()
        .map(|observation| observation.completed_at_utc);
    let prior_series_count = checked_count(eligible.len(), team_id)?;
    let prior_series_wins = checked_count(
        eligible
            .iter()
            .filter(|observation| observation.series_won)
            .count(),
        team_id,
    )?;
    let prior_game_count = checked_sum(
        eligible
            .iter()
            .map(|observation| u32::from(observation.games_won + observation.games_lost)),
        team_id,
    )?;
    let prior_game_wins = checked_sum(
        eligible
            .iter()
            .map(|observation| u32::from(observation.games_won)),
        team_id,
    )?;
    let same_patch_series_count = checked_count(same_patch.len(), team_id)?;
    let same_patch_series_wins = checked_count(
        same_patch
            .iter()
            .filter(|observation| observation.series_won)
            .count(),
        team_id,
    )?;

    Ok(TeamPrematchFeatures {
        team_id: team_id.to_owned(),
        source_team_key: source_team_key.to_owned(),
        prior_series_count: SourcedCount {
            value: prior_series_count,
            source_latest_at_utc: latest,
        },
        prior_series_win_rate: SourcedRatio {
            numerator: prior_series_wins,
            denominator: prior_series_count,
            source_latest_at_utc: latest,
        },
        prior_game_count: SourcedCount {
            value: prior_game_count,
            source_latest_at_utc: latest,
        },
        prior_game_win_rate: SourcedRatio {
            numerator: prior_game_wins,
            denominator: prior_game_count,
            source_latest_at_utc: latest,
        },
        same_patch_series_count: SourcedCount {
            value: same_patch_series_count,
            source_latest_at_utc: same_patch_latest,
        },
        same_patch_series_win_rate: SourcedRatio {
            numerator: same_patch_series_wins,
            denominator: same_patch_series_count,
            source_latest_at_utc: same_patch_latest,
        },
        rest_minutes: SourcedOptionalMinutes {
            value: latest.map(|completed_at| (snapshot_at_utc - completed_at).num_minutes()),
            source_latest_at_utc: latest,
        },
    })
}

fn checked_count(count: usize, team_id: &str) -> Result<u32, PrematchFeatureError> {
    u32::try_from(count).map_err(|_| PrematchFeatureError::FeatureOverflow(team_id.to_owned()))
}

fn checked_sum(
    mut values: impl Iterator<Item = u32>,
    team_id: &str,
) -> Result<u32, PrematchFeatureError> {
    values.try_fold(0_u32, |total, value| {
        total
            .checked_add(value)
            .ok_or_else(|| PrematchFeatureError::FeatureOverflow(team_id.to_owned()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp must be valid")
            .with_timezone(&Utc)
    }

    fn target() -> PrematchSeriesTarget {
        PrematchSeriesTarget {
            series_id: "leaguepedia:target".to_owned(),
            competition_id: "lol-competition:lck".to_owned(),
            region: "Korea".to_owned(),
            patch: "26.15".to_owned(),
            scheduled_start_utc: utc("2026-08-10T10:00:00Z"),
            best_of: 3,
            team_ids: ["lol-team:t1".to_owned(), "lol-team:gen-g".to_owned()],
            source_team_keys: ["T1".to_owned(), "Gen.G".to_owned()],
        }
    }

    fn observation(
        series_id: &str,
        source_team_key: &str,
        completed_at: &str,
        series_won: bool,
    ) -> TeamSeriesObservation {
        TeamSeriesObservation {
            series_id: series_id.to_owned(),
            source_team_key: source_team_key.to_owned(),
            patch: "26.15".to_owned(),
            scheduled_start_utc: utc("2026-08-01T08:00:00Z"),
            completed_at_utc: utc(completed_at),
            best_of: 3,
            games_won: if series_won { 2 } else { 1 },
            games_lost: if series_won { 1 } else { 2 },
            series_won,
        }
    }

    #[test]
    fn builds_sourced_features_at_fixed_prematch_cutoff() {
        let snapshots = build_prematch_feature_snapshots(PrematchFeatureBuildInput {
            snapshot_lead_minutes: 15,
            targets: vec![target()],
            team_series_observations: vec![
                observation("old-a", "T1", "2026-08-01T09:00:00Z", true),
                observation("old-b", "T1", "2026-08-05T09:30:00Z", false),
            ],
        })
        .expect("valid history must build");

        let snapshot = &snapshots[0];
        let features = &snapshot.team_features[0];
        assert_eq!(snapshot.snapshot_at_utc, utc("2026-08-10T09:45:00Z"));
        assert_eq!(features.prior_series_count.value, 2);
        assert_eq!(features.prior_series_win_rate.numerator, 1);
        assert_eq!(features.prior_series_win_rate.denominator, 2);
        assert_eq!(features.prior_game_win_rate.numerator, 3);
        assert_eq!(features.prior_game_win_rate.denominator, 6);
        assert_eq!(features.rest_minutes.value, Some(7215));
        assert_eq!(
            features.prior_series_count.source_latest_at_utc,
            Some(utc("2026-08-05T09:30:00Z"))
        );
    }

    #[test]
    fn post_snapshot_records_cannot_change_earlier_features() {
        let old = observation("old", "T1", "2026-08-05T09:30:00Z", true);
        let baseline = build_prematch_feature_snapshots(PrematchFeatureBuildInput {
            snapshot_lead_minutes: 15,
            targets: vec![target()],
            team_series_observations: vec![old.clone()],
        })
        .expect("baseline must build");

        let mut target_result =
            observation("leaguepedia:target", "T1", "2026-08-10T12:00:00Z", false);
        target_result.scheduled_start_utc = utc("2026-08-10T10:00:00Z");
        let with_future = build_prematch_feature_snapshots(PrematchFeatureBuildInput {
            snapshot_lead_minutes: 15,
            targets: vec![target()],
            team_series_observations: vec![old, target_result],
        })
        .expect("future record must be ignored");

        assert_eq!(with_future, baseline);
    }

    #[test]
    fn preserves_missing_history_without_synthetic_fill() {
        let snapshots = build_prematch_feature_snapshots(PrematchFeatureBuildInput {
            snapshot_lead_minutes: 15,
            targets: vec![target()],
            team_series_observations: vec![],
        })
        .expect("empty history is an explicit feature state");

        let features = &snapshots[0].team_features[0];
        assert_eq!(features.prior_series_count.value, 0);
        assert_eq!(features.prior_series_win_rate.denominator, 0);
        assert_eq!(features.prior_series_count.source_latest_at_utc, None);
        assert_eq!(features.rest_minutes.value, None);
    }

    #[test]
    fn target_projection_rejects_postmatch_label_fields() {
        let leaked_target = serde_json::json!({
            "series_id": "leaguepedia:target",
            "competition_id": "lol-competition:lck",
            "region": "Korea",
            "patch": "26.15",
            "scheduled_start_utc": "2026-08-10T10:00:00Z",
            "best_of": 3,
            "team_ids": ["lol-team:t1", "lol-team:gen-g"],
            "source_team_keys": ["T1", "Gen.G"],
            "winner_team_id": "lol-team:t1"
        });

        assert!(
            serde_json::from_value::<PrematchSeriesTarget>(leaked_target).is_err(),
            "target contract must reject target winner and other unknown postmatch fields"
        );
    }

    #[test]
    fn rejects_duplicate_observations_and_invalid_completion_time() {
        let duplicate = observation("old", "T1", "2026-08-05T09:30:00Z", true);
        let duplicate_result = build_prematch_feature_snapshots(PrematchFeatureBuildInput {
            snapshot_lead_minutes: 15,
            targets: vec![target()],
            team_series_observations: vec![duplicate.clone(), duplicate],
        });
        assert_eq!(
            duplicate_result,
            Err(PrematchFeatureError::DuplicateObservation {
                series_id: "old".to_owned(),
                source_team_key: "T1".to_owned(),
            })
        );

        let mut invalid_time = observation("invalid-time", "T1", "2026-08-01T07:59:00Z", true);
        invalid_time.scheduled_start_utc = utc("2026-08-01T08:00:00Z");
        assert_eq!(
            invalid_time.validate(),
            Err(PrematchFeatureError::InvalidObservationTime(
                "invalid-time".to_owned()
            ))
        );
    }
}
