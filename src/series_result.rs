use std::{collections::BTreeMap, error::Error, fmt};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 单场系列赛候选；只保存赛事身份和最终赛果，不要求存在预测市场。
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
}

/// 独立的市场结算关联候选；outcome 顺序必须保持 Gamma/CLOB 的原始 index。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketResolutionLinkCandidate {
    pub series_id: String,
    pub market_id: String,
    pub resolution_status: String,
    pub closed: bool,
    pub outcome_team_ids: [String; 2],
    pub outcome_prices: [u8; 2],
    pub winner_outcome_index: u8,
    pub mapping_evidence_id: String,
    pub resolution_evidence_id: String,
}

/// 去重后的纯系列赛结果；它可以独立进入 Constant/Elo/统计模型语料。
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
    pub duplicate_candidate_count: u32,
}

/// 经独立校验的市场结算关联；没有该记录不影响对应 Series Result 的有效性。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarketResolutionLink {
    pub series_id: String,
    pub market_id: String,
    pub resolution_status: String,
    pub closed: bool,
    pub outcome_team_ids: [String; 2],
    pub outcome_prices: [u8; 2],
    pub winner_outcome_index: u8,
    pub mapping_evidence_id: String,
    pub resolution_evidence_id: String,
    pub duplicate_candidate_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeriesResultError {
    EmptyField(&'static str),
    InvalidCanonicalId(&'static str),
    UnsupportedBestOf(u8),
    DuplicateTeams,
    InvalidScore {
        best_of: u8,
        scores: [u8; 2],
    },
    WinnerMismatch,
    MarketNotResolved,
    InvalidMarketOutcomeOrder,
    InvalidMarketResolution,
    MarketWinnerMismatch,
    UnknownSeriesForMarket(String),
    DuplicateSeriesInput(String),
    DuplicateCountOverflow(String),
    ConflictingDuplicate(String),
    ConflictingMarketResolution {
        series_id: String,
        market_id: String,
    },
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
            Self::UnknownSeriesForMarket(series_id) => {
                write!(formatter, "market resolution references unknown series: {series_id}")
            }
            Self::DuplicateSeriesInput(series_id) => {
                write!(formatter, "series result input contains duplicate id: {series_id}")
            }
            Self::DuplicateCountOverflow(key) => {
                write!(formatter, "too many duplicate candidates for: {key}")
            }
            Self::ConflictingDuplicate(series_id) => {
                write!(formatter, "duplicate series contains conflicting results: {series_id}")
            }
            Self::ConflictingMarketResolution {
                series_id,
                market_id,
            } => write!(
                formatter,
                "duplicate market resolution conflicts: series={series_id}, market={market_id}"
            ),
        }
    }
}

impl Error for SeriesResultError {}

impl SeriesResultCandidate {
    /// 只校验身份与最终赛果；市场证据由独立的关联合同负责。
    pub fn validate(&self) -> Result<(), SeriesResultError> {
        validate_series_fields(
            &self.series_id,
            &self.competition_id,
            &self.region,
            &self.patch,
            self.best_of,
            &self.team_ids,
            &self.team_names,
            self.scores,
            &self.winner_team_id,
            &self.mapping_evidence_id,
            &self.result_evidence_id,
        )
    }
}

impl SeriesResult {
    /// 反序列化后的结果也必须重新通过业务校验，不能只信任 schema。
    pub fn validate(&self) -> Result<(), SeriesResultError> {
        validate_series_fields(
            &self.series_id,
            &self.competition_id,
            &self.region,
            &self.patch,
            self.best_of,
            &self.team_ids,
            &self.team_names,
            self.scores,
            &self.winner_team_id,
            &self.mapping_evidence_id,
            &self.result_evidence_id,
        )
    }
}

impl MarketResolutionLinkCandidate {
    /// 只有存在关联候选时才校验市场；无关联的 Series Result 保持合法。
    pub fn validate(&self, result: &SeriesResult) -> Result<(), SeriesResultError> {
        for (field, value) in [
            ("market.series_id", self.series_id.as_str()),
            ("market.market_id", self.market_id.as_str()),
            (
                "market.mapping_evidence_id",
                self.mapping_evidence_id.as_str(),
            ),
            (
                "market.resolution_evidence_id",
                self.resolution_evidence_id.as_str(),
            ),
        ] {
            if value.trim().is_empty() {
                return Err(SeriesResultError::EmptyField(field));
            }
        }
        if !self.closed || self.resolution_status != "resolved" {
            return Err(SeriesResultError::MarketNotResolved);
        }
        if self.outcome_team_ids[0] == self.outcome_team_ids[1]
            || !self
                .outcome_team_ids
                .iter()
                .all(|team_id| result.team_ids.contains(team_id))
        {
            return Err(SeriesResultError::InvalidMarketOutcomeOrder);
        }

        let winner_index = usize::from(self.winner_outcome_index);
        if winner_index >= self.outcome_prices.len()
            || self.outcome_prices[winner_index] != 1
            || self
                .outcome_prices
                .iter()
                .enumerate()
                .any(|(index, price)| *price != u8::from(index == winner_index))
        {
            return Err(SeriesResultError::InvalidMarketResolution);
        }
        if self.outcome_team_ids[winner_index] != result.winner_team_id {
            return Err(SeriesResultError::MarketWinnerMismatch);
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_series_fields(
    series_id: &str,
    competition_id: &str,
    region: &str,
    patch: &str,
    best_of: u8,
    team_ids: &[String; 2],
    team_names: &[String; 2],
    scores: [u8; 2],
    winner_team_id: &str,
    mapping_evidence_id: &str,
    result_evidence_id: &str,
) -> Result<(), SeriesResultError> {
    for (field, value) in [
        ("series_id", series_id),
        ("region", region),
        ("patch", patch),
        ("team_names[0]", team_names[0].as_str()),
        ("team_names[1]", team_names[1].as_str()),
        ("winner_team_id", winner_team_id),
        ("mapping_evidence_id", mapping_evidence_id),
        ("result_evidence_id", result_evidence_id),
    ] {
        if value.trim().is_empty() {
            return Err(SeriesResultError::EmptyField(field));
        }
    }
    if !competition_id.starts_with("lol-competition:") {
        return Err(SeriesResultError::InvalidCanonicalId("competition_id"));
    }
    if team_ids
        .iter()
        .any(|team_id| !team_id.starts_with("lol-team:"))
    {
        return Err(SeriesResultError::InvalidCanonicalId("team_ids"));
    }
    if !matches!(best_of, 3 | 5) {
        return Err(SeriesResultError::UnsupportedBestOf(best_of));
    }
    if team_ids[0] == team_ids[1] {
        return Err(SeriesResultError::DuplicateTeams);
    }

    let wins_needed = best_of / 2 + 1;
    let completed_winner_index = match scores {
        [left, right] if left == wins_needed && right < wins_needed => 0,
        [left, right] if right == wins_needed && left < wins_needed => 1,
        _ => return Err(SeriesResultError::InvalidScore { best_of, scores }),
    };
    if winner_team_id != team_ids[completed_winner_index] {
        return Err(SeriesResultError::WinnerMismatch);
    }
    Ok(())
}

/// 以 `series_id` 去重；相同赛事事实合并并稳定选择最小证据键，业务冲突立即拒绝。
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
            left.mapping_evidence_id.as_str(),
        )
            .cmp(&(
                right.series_id.as_str(),
                right.result_evidence_id.as_str(),
                right.mapping_evidence_id.as_str(),
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
                duplicate_candidate_count,
            })
        })
        .collect()
}

/// 独立构建市场关联；没有候选时返回空集合，不会反向淘汰 Series Result。
pub fn build_market_resolution_links(
    series_results: &[SeriesResult],
    mut candidates: Vec<MarketResolutionLinkCandidate>,
) -> Result<Vec<MarketResolutionLink>, SeriesResultError> {
    let mut results_by_id = BTreeMap::new();
    for result in series_results {
        result.validate()?;
        if results_by_id
            .insert(result.series_id.as_str(), result)
            .is_some()
        {
            return Err(SeriesResultError::DuplicateSeriesInput(
                result.series_id.clone(),
            ));
        }
    }
    for candidate in &candidates {
        let result = results_by_id
            .get(candidate.series_id.as_str())
            .ok_or_else(|| {
                SeriesResultError::UnknownSeriesForMarket(candidate.series_id.clone())
            })?;
        candidate.validate(result)?;
    }

    candidates.sort_by(|left, right| {
        (
            left.series_id.as_str(),
            left.market_id.as_str(),
            left.resolution_evidence_id.as_str(),
            left.mapping_evidence_id.as_str(),
        )
            .cmp(&(
                right.series_id.as_str(),
                right.market_id.as_str(),
                right.resolution_evidence_id.as_str(),
                right.mapping_evidence_id.as_str(),
            ))
    });

    let mut grouped: BTreeMap<(String, String), Vec<MarketResolutionLinkCandidate>> =
        BTreeMap::new();
    for candidate in candidates {
        grouped
            .entry((candidate.series_id.clone(), candidate.market_id.clone()))
            .or_default()
            .push(candidate);
    }

    grouped
        .into_iter()
        .map(|((series_id, market_id), group)| {
            let primary = &group[0];
            if group
                .iter()
                .skip(1)
                .any(|candidate| !has_same_market_resolution(primary, candidate))
            {
                return Err(SeriesResultError::ConflictingMarketResolution {
                    series_id,
                    market_id,
                });
            }
            let duplicate_candidate_count = u32::try_from(group.len()).map_err(|_| {
                SeriesResultError::DuplicateCountOverflow(format!(
                    "{}:{}",
                    primary.series_id, primary.market_id
                ))
            })?;

            Ok(MarketResolutionLink {
                series_id: primary.series_id.clone(),
                market_id: primary.market_id.clone(),
                resolution_status: primary.resolution_status.clone(),
                closed: primary.closed,
                outcome_team_ids: primary.outcome_team_ids.clone(),
                outcome_prices: primary.outcome_prices,
                winner_outcome_index: primary.winner_outcome_index,
                mapping_evidence_id: primary.mapping_evidence_id.clone(),
                resolution_evidence_id: primary.resolution_evidence_id.clone(),
                duplicate_candidate_count,
            })
        })
        .collect()
}

/// 证据 ID 可以不同；其余赛事事实必须完全一致才属于可合并重复。
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
}

/// 同一 series/market 的多份证据只有结算事实完全一致时才能合并。
fn has_same_market_resolution(
    left: &MarketResolutionLinkCandidate,
    right: &MarketResolutionLinkCandidate,
) -> bool {
    left.series_id == right.series_id
        && left.market_id == right.market_id
        && left.resolution_status == right.resolution_status
        && left.closed == right.closed
        && left.outcome_team_ids == right.outcome_team_ids
        && left.outcome_prices == right.outcome_prices
        && left.winner_outcome_index == right.winner_outcome_index
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
            mapping_evidence_id: "identity:01".to_owned(),
            result_evidence_id: "leaguepedia:a".to_owned(),
        }
    }

    fn result() -> SeriesResult {
        build_series_results(vec![candidate()])
            .expect("fixture must build")
            .remove(0)
    }

    fn market_link() -> MarketResolutionLinkCandidate {
        MarketResolutionLinkCandidate {
            series_id: "leaguepedia:worlds-finals".to_owned(),
            market_id: "100".to_owned(),
            resolution_status: "resolved".to_owned(),
            closed: true,
            outcome_team_ids: ["lol-team:t1".to_owned(), "lol-team:kt".to_owned()],
            outcome_prices: [1, 0],
            winner_outcome_index: 0,
            mapping_evidence_id: "DATA-008:01".to_owned(),
            resolution_evidence_id: "gamma:b".to_owned(),
        }
    }

    #[test]
    fn builds_series_result_without_market_link() {
        let results = build_series_results(vec![candidate()]).expect("valid result must pass");
        assert_eq!(results.len(), 1);
        assert!(
            build_market_resolution_links(&results, Vec::new())
                .expect("market links are optional")
                .is_empty()
        );
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
    fn validates_independent_market_resolution() {
        let results = vec![result()];
        let links = build_market_resolution_links(&results, vec![market_link()])
            .expect("valid link must pass");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].market_id, "100");
    }

    #[test]
    fn rejects_series_and_market_winner_disagreement() {
        let results = vec![result()];
        let mut mismatched = market_link();
        mismatched.outcome_prices = [0, 1];
        mismatched.winner_outcome_index = 1;
        assert_eq!(
            build_market_resolution_links(&results, vec![mismatched]),
            Err(SeriesResultError::MarketWinnerMismatch)
        );
    }

    #[test]
    fn rejects_market_link_for_unknown_series() {
        let mut unknown = market_link();
        unknown.series_id = "leaguepedia:unknown".to_owned();
        assert_eq!(
            build_market_resolution_links(&[result()], vec![unknown]),
            Err(SeriesResultError::UnknownSeriesForMarket(
                "leaguepedia:unknown".to_owned()
            ))
        );
    }

    #[test]
    fn merges_equal_series_duplicates_with_stable_primary_evidence() {
        let later = candidate();
        let mut earlier = later.clone();
        earlier.result_evidence_id = "leaguepedia:0".to_owned();
        earlier.mapping_evidence_id = "identity:00".to_owned();

        let results = build_series_results(vec![later, earlier]).expect("duplicates must merge");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result_evidence_id, "leaguepedia:0");
        assert_eq!(results[0].mapping_evidence_id, "identity:00");
        assert_eq!(results[0].duplicate_candidate_count, 2);
    }

    #[test]
    fn merges_equal_market_links_with_stable_primary_evidence() {
        let later = market_link();
        let mut earlier = later.clone();
        earlier.resolution_evidence_id = "gamma:a".to_owned();
        earlier.mapping_evidence_id = "DATA-008:00".to_owned();

        let links = build_market_resolution_links(&[result()], vec![later, earlier])
            .expect("duplicate links must merge");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].resolution_evidence_id, "gamma:a");
        assert_eq!(links[0].mapping_evidence_id, "DATA-008:00");
        assert_eq!(links[0].duplicate_candidate_count, 2);
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
