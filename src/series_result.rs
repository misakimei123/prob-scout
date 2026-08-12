use std::{collections::BTreeMap, error::Error, fmt};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// HIST-003 的单场系列赛候选；字段只保存赛前身份和最终赛果，不包含 HIST-004 特征。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesResultCandidate {
    pub series_id: String,
    pub competition_id: String,
    pub region: String,
    pub patch: String,
    pub scheduled_start_utc: DateTime<Utc>,
    pub best_of: u8,
    pub team_ids: [String; 2],
    pub team_names: [String; 2],
    pub scores: [u8; 2],
    pub winner_team_id: String,
    pub mapping_evidence_id: String,
    pub result_evidence_id: String,
    pub market_resolution: MarketResolutionEvidence,
}

/// 已完成市场的独立结算证据；outcome 顺序必须保持 Gamma/CLOB 的原始 index。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketResolutionEvidence {
    pub market_id: String,
    pub resolution_status: String,
    pub closed: bool,
    pub outcome_team_ids: [String; 2],
    pub outcome_prices: [u8; 2],
    pub winner_outcome_index: u8,
    pub evidence_id: String,
}

/// 去重后的每场系列赛结果；主证据稳定选择，重复数量用于审计来源重叠。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesResult {
    pub series_id: String,
    pub competition_id: String,
    pub region: String,
    pub patch: String,
    pub scheduled_start_utc: DateTime<Utc>,
    pub best_of: u8,
    pub team_ids: [String; 2],
    pub team_names: [String; 2],
    pub scores: [u8; 2],
    pub winner_team_id: String,
    pub mapping_evidence_id: String,
    pub result_evidence_id: String,
    pub market_id: String,
    pub market_winner_outcome_index: u8,
    pub market_resolution_evidence_id: String,
    pub duplicate_candidate_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeriesResultError {
    EmptyField(&'static str),
    InvalidCanonicalId(&'static str),
    UnsupportedBestOf(u8),
    DuplicateTeams,
    InvalidScore { best_of: u8, scores: [u8; 2] },
    WinnerMismatch,
    MarketNotResolved,
    InvalidMarketOutcomeOrder,
    InvalidMarketResolution,
    MarketWinnerMismatch,
    DuplicateCountOverflow(String),
    ConflictingDuplicate(String),
}

impl fmt::Display for SeriesResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(formatter, "required field is empty: {field}"),
            Self::InvalidCanonicalId(field) => {
                write!(formatter, "field must contain a resolved canonical id: {field}")
            }
            Self::UnsupportedBestOf(best_of) => {
                write!(formatter, "series result only accepts BO3 or BO5: {best_of}")
            }
            Self::DuplicateTeams => formatter.write_str("series teams must be different"),
            Self::InvalidScore { best_of, scores } => write!(
                formatter,
                "score does not represent a completed BO{best_of}: {}-{}",
                scores[0], scores[1]
            ),
            Self::WinnerMismatch => {
                formatter.write_str("winner_team_id does not match the completed score")
            }
            Self::MarketNotResolved => formatter
                .write_str("market resolution must be closed with resolution_status=resolved"),
            Self::InvalidMarketOutcomeOrder => formatter
                .write_str("market outcomes must contain the two series teams exactly once"),
            Self::InvalidMarketResolution => formatter.write_str(
                "market outcome prices must be binary with exactly one winner matching the winner index",
            ),
            Self::MarketWinnerMismatch => {
                formatter.write_str("series winner and market resolution winner do not match")
            }
            Self::DuplicateCountOverflow(series_id) => {
                write!(formatter, "too many duplicate candidates for series: {series_id}")
            }
            Self::ConflictingDuplicate(series_id) => {
                write!(formatter, "duplicate series contains conflicting results: {series_id}")
            }
        }
    }
}

impl Error for SeriesResultError {}

impl SeriesResultCandidate {
    /// 同时校验赛事最终比分与独立市场结算；identity 只接受上游已解析的 canonical ID。
    pub fn validate(&self) -> Result<(), SeriesResultError> {
        for (field, value) in [
            ("series_id", self.series_id.as_str()),
            ("region", self.region.as_str()),
            ("patch", self.patch.as_str()),
            ("team_names[0]", self.team_names[0].as_str()),
            ("team_names[1]", self.team_names[1].as_str()),
            ("winner_team_id", self.winner_team_id.as_str()),
            ("mapping_evidence_id", self.mapping_evidence_id.as_str()),
            ("result_evidence_id", self.result_evidence_id.as_str()),
            ("market_id", self.market_resolution.market_id.as_str()),
            (
                "market_resolution.evidence_id",
                self.market_resolution.evidence_id.as_str(),
            ),
        ] {
            if value.trim().is_empty() {
                return Err(SeriesResultError::EmptyField(field));
            }
        }
        if !self.competition_id.starts_with("lol-competition:") {
            return Err(SeriesResultError::InvalidCanonicalId("competition_id"));
        }
        if self
            .team_ids
            .iter()
            .any(|team_id| !team_id.starts_with("lol-team:"))
        {
            return Err(SeriesResultError::InvalidCanonicalId("team_ids"));
        }
        if !matches!(self.best_of, 3 | 5) {
            return Err(SeriesResultError::UnsupportedBestOf(self.best_of));
        }
        if self.team_ids[0] == self.team_ids[1] {
            return Err(SeriesResultError::DuplicateTeams);
        }

        let wins_needed = self.best_of / 2 + 1;
        let completed_winner_index = match self.scores {
            [left, right] if left == wins_needed && right < wins_needed => 0,
            [left, right] if right == wins_needed && left < wins_needed => 1,
            _ => {
                return Err(SeriesResultError::InvalidScore {
                    best_of: self.best_of,
                    scores: self.scores,
                });
            }
        };
        if self.winner_team_id != self.team_ids[completed_winner_index] {
            return Err(SeriesResultError::WinnerMismatch);
        }

        let market = &self.market_resolution;
        if !market.closed || market.resolution_status != "resolved" {
            return Err(SeriesResultError::MarketNotResolved);
        }
        if market.outcome_team_ids[0] == market.outcome_team_ids[1]
            || !market
                .outcome_team_ids
                .iter()
                .all(|team_id| self.team_ids.contains(team_id))
        {
            return Err(SeriesResultError::InvalidMarketOutcomeOrder);
        }
        let winner_index = usize::from(market.winner_outcome_index);
        if winner_index >= market.outcome_prices.len()
            || market.outcome_prices[winner_index] != 1
            || market
                .outcome_prices
                .iter()
                .enumerate()
                .any(|(index, price)| *price != u8::from(index == winner_index))
        {
            return Err(SeriesResultError::InvalidMarketResolution);
        }
        if market.outcome_team_ids[winner_index] != self.winner_team_id {
            return Err(SeriesResultError::MarketWinnerMismatch);
        }

        Ok(())
    }
}

/// 以 `series_id` 去重：相同结果合并并稳定选最小证据键，任一业务字段冲突立即拒绝。
pub fn build_series_results(
    mut candidates: Vec<SeriesResultCandidate>,
) -> Result<Vec<SeriesResult>, SeriesResultError> {
    for candidate in &candidates {
        candidate.validate()?;
    }
    candidates.sort_by(|left, right| {
        (
            left.series_id.as_str(),
            left.result_evidence_id.as_str(),
            left.market_resolution.evidence_id.as_str(),
            left.market_resolution.market_id.as_str(),
        )
            .cmp(&(
                right.series_id.as_str(),
                right.result_evidence_id.as_str(),
                right.market_resolution.evidence_id.as_str(),
                right.market_resolution.market_id.as_str(),
            ))
    });

    let mut grouped: BTreeMap<String, Vec<SeriesResultCandidate>> = BTreeMap::new();
    for candidate in candidates {
        grouped
            .entry(candidate.series_id.clone())
            .or_default()
            .push(candidate);
    }

    grouped
        .into_iter()
        .map(|(series_id, group)| {
            let primary = &group[0];
            if group
                .iter()
                .skip(1)
                .any(|candidate| !has_same_result(primary, candidate))
            {
                return Err(SeriesResultError::ConflictingDuplicate(series_id));
            }
            let duplicate_candidate_count = u32::try_from(group.len()).map_err(|_| {
                SeriesResultError::DuplicateCountOverflow(primary.series_id.clone())
            })?;

            Ok(SeriesResult {
                series_id: primary.series_id.clone(),
                competition_id: primary.competition_id.clone(),
                region: primary.region.clone(),
                patch: primary.patch.clone(),
                scheduled_start_utc: primary.scheduled_start_utc,
                best_of: primary.best_of,
                team_ids: primary.team_ids.clone(),
                team_names: primary.team_names.clone(),
                scores: primary.scores,
                winner_team_id: primary.winner_team_id.clone(),
                mapping_evidence_id: primary.mapping_evidence_id.clone(),
                result_evidence_id: primary.result_evidence_id.clone(),
                market_id: primary.market_resolution.market_id.clone(),
                market_winner_outcome_index: primary.market_resolution.winner_outcome_index,
                market_resolution_evidence_id: primary.market_resolution.evidence_id.clone(),
                duplicate_candidate_count,
            })
        })
        .collect()
}

/// 证据 ID 和 market ID 可以不同；其余赛果事实必须完全一致才属于可合并重复。
fn has_same_result(left: &SeriesResultCandidate, right: &SeriesResultCandidate) -> bool {
    left.series_id == right.series_id
        && left.competition_id == right.competition_id
        && left.region == right.region
        && left.patch == right.patch
        && left.scheduled_start_utc == right.scheduled_start_utc
        && left.best_of == right.best_of
        && left.team_ids == right.team_ids
        && left.team_names == right.team_names
        && left.scores == right.scores
        && left.winner_team_id == right.winner_team_id
        && left.mapping_evidence_id == right.mapping_evidence_id
        && left.market_resolution.outcome_team_ids == right.market_resolution.outcome_team_ids
        && left.market_resolution.outcome_prices == right.market_resolution.outcome_prices
        && left.market_resolution.winner_outcome_index
            == right.market_resolution.winner_outcome_index
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp must be valid")
            .with_timezone(&Utc)
    }

    fn candidate() -> SeriesResultCandidate {
        SeriesResultCandidate {
            series_id: "leaguepedia:worlds-finals".to_owned(),
            competition_id: "lol-competition:worlds".to_owned(),
            region: "International".to_owned(),
            patch: "25.20".to_owned(),
            scheduled_start_utc: utc("2025-11-09T07:00:00Z"),
            best_of: 5,
            team_ids: ["lol-team:t1".to_owned(), "lol-team:kt".to_owned()],
            team_names: ["T1".to_owned(), "KT Rolster".to_owned()],
            scores: [3, 2],
            winner_team_id: "lol-team:t1".to_owned(),
            mapping_evidence_id: "DATA-008:01".to_owned(),
            result_evidence_id: "leaguepedia:a".to_owned(),
            market_resolution: MarketResolutionEvidence {
                market_id: "100".to_owned(),
                resolution_status: "resolved".to_owned(),
                closed: true,
                outcome_team_ids: ["lol-team:t1".to_owned(), "lol-team:kt".to_owned()],
                outcome_prices: [1, 0],
                winner_outcome_index: 0,
                evidence_id: "gamma:b".to_owned(),
            },
        }
    }

    #[test]
    fn validates_completed_series_and_independent_market_resolution() {
        candidate().validate().expect("valid result must pass");
    }

    #[test]
    fn rejects_bo1_and_incomplete_scores() {
        let mut bo1 = candidate();
        bo1.best_of = 1;
        assert_eq!(bo1.validate(), Err(SeriesResultError::UnsupportedBestOf(1)));

        let mut incomplete = candidate();
        incomplete.scores = [2, 2];
        assert_eq!(
            incomplete.validate(),
            Err(SeriesResultError::InvalidScore {
                best_of: 5,
                scores: [2, 2]
            })
        );
    }

    #[test]
    fn rejects_series_and_market_winner_disagreement() {
        let mut mismatched = candidate();
        mismatched.market_resolution.outcome_prices = [0, 1];
        mismatched.market_resolution.winner_outcome_index = 1;
        assert_eq!(
            mismatched.validate(),
            Err(SeriesResultError::MarketWinnerMismatch)
        );
    }

    #[test]
    fn merges_equal_duplicates_with_stable_primary_evidence() {
        let later = candidate();
        let mut earlier = later.clone();
        earlier.result_evidence_id = "leaguepedia:0".to_owned();
        earlier.market_resolution.evidence_id = "gamma:0".to_owned();
        earlier.market_resolution.market_id = "099".to_owned();

        let results = build_series_results(vec![later, earlier]).expect("duplicates must merge");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result_evidence_id, "leaguepedia:0");
        assert_eq!(results[0].market_id, "099");
        assert_eq!(results[0].duplicate_candidate_count, 2);
    }

    #[test]
    fn rejects_conflicting_duplicate_series() {
        let first = candidate();
        let mut conflict = candidate();
        conflict.scores = [3, 1];
        conflict.result_evidence_id = "leaguepedia:z".to_owned();

        assert_eq!(
            build_series_results(vec![first, conflict]),
            Err(SeriesResultError::ConflictingDuplicate(
                "leaguepedia:worlds-finals".to_owned()
            ))
        );
    }
}
