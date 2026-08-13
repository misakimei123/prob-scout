use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use chrono::{DateTime, Datelike, Duration, NaiveDateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Leaguepedia 历史候选查询的半开 UTC 时间范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalCandidateScope {
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
}

/// MatchSchedule 原始行。字段允许缺失，以便将业务不完整记录写入 rejection audit。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawHistoricalSeriesRow {
    #[serde(
        rename = "MatchId",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub match_id: Option<String>,
    #[serde(
        rename = "MatchStartUtc",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub match_start_utc: Option<String>,
    #[serde(
        rename = "Team1",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub team_1: Option<String>,
    #[serde(
        rename = "Team2",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub team_2: Option<String>,
    #[serde(rename = "Team1Score", default)]
    pub team_1_score: Option<u8>,
    #[serde(rename = "Team2Score", default)]
    pub team_2_score: Option<u8>,
    #[serde(rename = "Winner", default)]
    pub winner: Option<u8>,
    #[serde(rename = "BestOf", default)]
    pub best_of: Option<u8>,
    #[serde(
        rename = "OverviewPage",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub overview_page: Option<String>,
}

/// ScoreboardGames 原始行；`N GameInMatch` 和 `Gamelength Number` 使用 Cargo 实际 JSON key。
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawHistoricalGameRow {
    #[serde(
        rename = "MatchId",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub match_id: Option<String>,
    #[serde(
        rename = "Patch",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub patch: Option<String>,
    #[serde(rename = "N GameInMatch", alias = "N_GameInMatch", default)]
    pub game_number: Option<u16>,
    #[serde(
        rename = "GameStartUtc",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub game_start_utc: Option<String>,
    #[serde(rename = "Gamelength Number", alias = "Gamelength_Number", default)]
    pub game_length_minutes: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalCandidateBuildInput {
    pub series_rows: Vec<RawHistoricalSeriesRow>,
    pub game_rows: Vec<RawHistoricalGameRow>,
    /// M3R-002 只用原始赛事页到 Region 的精确关系做覆盖审计，不在候选阶段生成 canonical identity。
    #[serde(default)]
    pub tournament_rows: Vec<RawHistoricalTournamentRow>,
    /// 旧 eligible corpus 的最小引用投影，仅用于证明成员和时间窗口零重叠。
    #[serde(default)]
    pub reference_series_rows: Vec<HistoricalReferenceSeriesRow>,
    #[serde(default)]
    pub minimum_recovery_start_utc: Option<DateTime<Utc>>,
}

/// Leaguepedia Tournaments 的原始 Region 关系；缺失或冲突会保留为覆盖缺口。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RawHistoricalTournamentRow {
    #[serde(
        rename = "OverviewPage",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub overview_page: Option<String>,
    #[serde(
        rename = "Region",
        default,
        deserialize_with = "deserialize_optional_text"
    )]
    pub region: Option<String>,
}

/// 恢复任务只需旧 corpus 的 series identity 与 Scheduled Start，不读取任何特征或预测。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalReferenceSeriesRow {
    pub series_id: String,
    pub scheduled_start_utc: DateTime<Utc>,
}

/// 尚未猜测 Canonical identity 的完整赛事候选；source key 保持 Leaguepedia 原值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalSeriesCandidate {
    pub series_id: String,
    pub leaguepedia_match_id: String,
    pub competition_source_key: String,
    pub team_source_keys: [String; 2],
    pub scheduled_start_utc: DateTime<Utc>,
    pub completed_at_utc: DateTime<Utc>,
    pub best_of: u8,
    pub patch: String,
    pub scores: [u8; 2],
    pub winner_team_source_key: String,
    pub result_evidence_id: String,
    pub series_source_row_count: u32,
    pub game_source_row_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoricalCandidateRejectionReason {
    ConflictingSeriesRows,
    MissingRequiredField,
    InvalidScheduledStart,
    UnsupportedBestOf,
    DuplicateTeams,
    InvalidScore,
    WinnerMismatch,
    MissingGameRows,
    MissingPatch,
    ConflictingPatch,
    MissingGameNumber,
    InvalidGameSequence,
    GameCountMismatch,
    InvalidGameTime,
    CompletionNotAfterStart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RejectedHistoricalSeries {
    pub series_id: String,
    pub leaguepedia_match_id: String,
    pub scheduled_start_utc: Option<DateTime<Utc>>,
    pub reason: HistoricalCandidateRejectionReason,
    pub detail: String,
    pub series_source_row_count: u32,
    pub game_source_row_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalCandidateCoverage {
    pub raw_series_rows: u32,
    pub raw_game_rows: u32,
    pub distinct_match_ids: u32,
    pub candidate_count: u32,
    pub rejected_count: u32,
    pub distinct_utc_dates: u32,
    pub years: Vec<i32>,
    pub patches: BTreeMap<String, u32>,
    pub best_of: BTreeMap<String, u32>,
    pub rejection_counts: BTreeMap<HistoricalCandidateRejectionReason, u32>,
}

/// 原始赛事页的 Region 覆盖；该结果是描述性 source evidence，不代表 competition identity 已解析。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalSourceRegionCoverage {
    pub raw_tournament_rows: u32,
    pub resolved_candidate_count: u32,
    pub missing_candidate_count: u32,
    pub ambiguous_candidate_count: u32,
    pub regions: BTreeMap<String, u32>,
}

/// M3R-002 的零重叠证明；生成器只要发现一个成员或时间越界就整体失败。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalRecoveryDisjointness {
    pub minimum_new_start_utc: DateTime<Utc>,
    pub reference_series_count: u32,
    pub reference_start_utc: DateTime<Utc>,
    pub reference_end_utc: DateTime<Utc>,
    pub new_start_utc: DateTime<Utc>,
    pub new_end_utc: DateTime<Utc>,
    pub member_overlap_count: u32,
    pub temporal_overlap_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalCandidateAudit {
    pub audit_version: u16,
    pub scope: HistoricalCandidateScope,
    pub coverage: HistoricalCandidateCoverage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_region_coverage: Option<HistoricalSourceRegionCoverage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_disjointness: Option<HistoricalRecoveryDisjointness>,
    pub candidates: Vec<HistoricalSeriesCandidate>,
    pub rejections: Vec<RejectedHistoricalSeries>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoricalCandidateError {
    InvalidScope,
    EmptySeriesInput,
    MissingMatchId {
        dataset: &'static str,
        row_index: usize,
    },
    OrphanGameRows(String),
    SeriesOutsideScope(String),
    CountOverflow(&'static str),
    NoCandidates,
    IncompleteRecoveryContract,
    EmptyReferenceCorpus,
    DuplicateReferenceSeries(String),
    ReferenceSeriesOutsideOldWindow(String),
    RecoverySeriesOutsideNewWindow(String),
    RecoveryMemberOverlap(String),
}

impl fmt::Display for HistoricalCandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidScope => {
                formatter.write_str("historical candidate scope must be non-empty")
            }
            Self::EmptySeriesInput => formatter.write_str("MatchSchedule input is empty"),
            Self::MissingMatchId { dataset, row_index } => {
                write!(formatter, "{dataset} row {row_index} has no MatchId")
            }
            Self::OrphanGameRows(match_id) => {
                write!(
                    formatter,
                    "ScoreboardGames references unknown MatchId: {match_id}"
                )
            }
            Self::SeriesOutsideScope(match_id) => {
                write!(
                    formatter,
                    "MatchSchedule row is outside requested scope: {match_id}"
                )
            }
            Self::CountOverflow(field) => write!(formatter, "count exceeds u32: {field}"),
            Self::NoCandidates => {
                formatter.write_str("candidate audit produced zero usable series")
            }
            Self::IncompleteRecoveryContract => formatter.write_str(
                "reference_series_rows and minimum_recovery_start_utc must be provided together",
            ),
            Self::EmptyReferenceCorpus => {
                formatter.write_str("recovery reference corpus must not be empty")
            }
            Self::DuplicateReferenceSeries(series_id) => {
                write!(
                    formatter,
                    "recovery reference corpus contains duplicate series: {series_id}"
                )
            }
            Self::ReferenceSeriesOutsideOldWindow(series_id) => write!(
                formatter,
                "reference series is not strictly before the recovery boundary: {series_id}"
            ),
            Self::RecoverySeriesOutsideNewWindow(series_id) => write!(
                formatter,
                "recovery candidate is earlier than the recovery boundary: {series_id}"
            ),
            Self::RecoveryMemberOverlap(series_id) => write!(
                formatter,
                "recovery candidate overlaps the reference corpus: {series_id}"
            ),
        }
    }
}

impl Error for HistoricalCandidateError {}

/// 构建候选覆盖审计：可预期的数据缺陷进入 rejections，查询合同破坏则整体 fail closed。
pub fn build_historical_candidate_audit(
    scope: HistoricalCandidateScope,
    input: HistoricalCandidateBuildInput,
) -> Result<HistoricalCandidateAudit, HistoricalCandidateError> {
    if scope.start_utc >= scope.end_utc {
        return Err(HistoricalCandidateError::InvalidScope);
    }
    if input.series_rows.is_empty() {
        return Err(HistoricalCandidateError::EmptySeriesInput);
    }

    let HistoricalCandidateBuildInput {
        series_rows,
        game_rows,
        tournament_rows,
        reference_series_rows,
        minimum_recovery_start_utc,
    } = input;

    let raw_series_rows = to_u32(series_rows.len(), "raw_series_rows")?;
    let raw_game_rows = to_u32(game_rows.len(), "raw_game_rows")?;
    let mut series_groups: BTreeMap<String, Vec<RawHistoricalSeriesRow>> = BTreeMap::new();
    for (row_index, row) in series_rows.into_iter().enumerate() {
        let match_id = required_match_id(row.match_id.as_deref()).ok_or(
            HistoricalCandidateError::MissingMatchId {
                dataset: "MatchSchedule",
                row_index,
            },
        )?;
        series_groups.entry(match_id).or_default().push(row);
    }

    let mut game_groups: BTreeMap<String, Vec<RawHistoricalGameRow>> = BTreeMap::new();
    for (row_index, row) in game_rows.into_iter().enumerate() {
        let match_id = required_match_id(row.match_id.as_deref()).ok_or(
            HistoricalCandidateError::MissingMatchId {
                dataset: "ScoreboardGames",
                row_index,
            },
        )?;
        if !series_groups.contains_key(&match_id) {
            return Err(HistoricalCandidateError::OrphanGameRows(match_id));
        }
        game_groups.entry(match_id).or_default().push(row);
    }

    let distinct_match_ids = to_u32(series_groups.len(), "distinct_match_ids")?;
    let mut candidates = Vec::new();
    let mut rejections = Vec::new();

    for (match_id, series_rows) in series_groups {
        let game_rows = game_groups.remove(&match_id).unwrap_or_default();
        match build_candidate(&scope, &match_id, &series_rows, &game_rows)? {
            Ok(candidate) => candidates.push(candidate),
            Err(rejection) => rejections.push(rejection),
        }
    }

    candidates.sort_by(|left, right| {
        (left.scheduled_start_utc, left.series_id.as_str())
            .cmp(&(right.scheduled_start_utc, right.series_id.as_str()))
    });
    rejections.sort_by(|left, right| {
        (
            left.scheduled_start_utc,
            left.series_id.as_str(),
            left.reason,
        )
            .cmp(&(
                right.scheduled_start_utc,
                right.series_id.as_str(),
                right.reason,
            ))
    });
    if candidates.is_empty() {
        return Err(HistoricalCandidateError::NoCandidates);
    }

    let coverage = build_coverage(
        raw_series_rows,
        raw_game_rows,
        distinct_match_ids,
        &candidates,
        &rejections,
    )?;
    let source_region_coverage = build_source_region_coverage(&candidates, &tournament_rows)?;
    let recovery_disjointness = build_recovery_disjointness(
        &candidates,
        &reference_series_rows,
        minimum_recovery_start_utc,
    )?;
    Ok(HistoricalCandidateAudit {
        audit_version: 1,
        scope,
        coverage,
        source_region_coverage,
        recovery_disjointness,
        candidates,
        rejections,
    })
}

fn build_candidate(
    scope: &HistoricalCandidateScope,
    match_id: &str,
    series_rows: &[RawHistoricalSeriesRow],
    game_rows: &[RawHistoricalGameRow],
) -> Result<Result<HistoricalSeriesCandidate, RejectedHistoricalSeries>, HistoricalCandidateError> {
    let series_row_count = to_u32(series_rows.len(), "series_source_row_count")?;
    let game_row_count = to_u32(game_rows.len(), "game_source_row_count")?;
    let first = &series_rows[0];
    let scheduled_start = first.match_start_utc.as_deref().and_then(parse_cargo_utc);
    let reject = |reason, detail: String| {
        Err(RejectedHistoricalSeries {
            series_id: format!("leaguepedia:{match_id}"),
            leaguepedia_match_id: match_id.to_owned(),
            scheduled_start_utc: scheduled_start,
            reason,
            detail,
            series_source_row_count: series_row_count,
            game_source_row_count: game_row_count,
        })
    };

    if series_rows.iter().skip(1).any(|row| row != first) {
        return Ok(reject(
            HistoricalCandidateRejectionReason::ConflictingSeriesRows,
            "MatchSchedule duplicate rows disagree".to_owned(),
        ));
    }

    let Some(scheduled_start_utc) = scheduled_start else {
        return Ok(reject(
            HistoricalCandidateRejectionReason::InvalidScheduledStart,
            "MatchStartUtc is missing or not yyyy-MM-dd HH:mm:ss".to_owned(),
        ));
    };
    if scheduled_start_utc < scope.start_utc || scheduled_start_utc >= scope.end_utc {
        return Err(HistoricalCandidateError::SeriesOutsideScope(
            match_id.to_owned(),
        ));
    }

    let Some(competition_source_key) = non_empty(first.overview_page.as_deref()) else {
        return Ok(reject(
            HistoricalCandidateRejectionReason::MissingRequiredField,
            "OverviewPage is missing".to_owned(),
        ));
    };
    let Some(team_1) = non_empty(first.team_1.as_deref()) else {
        return Ok(reject(
            HistoricalCandidateRejectionReason::MissingRequiredField,
            "Team1 is missing".to_owned(),
        ));
    };
    let Some(team_2) = non_empty(first.team_2.as_deref()) else {
        return Ok(reject(
            HistoricalCandidateRejectionReason::MissingRequiredField,
            "Team2 is missing".to_owned(),
        ));
    };
    if team_1 == team_2 {
        return Ok(reject(
            HistoricalCandidateRejectionReason::DuplicateTeams,
            "Team1 and Team2 are identical source keys".to_owned(),
        ));
    }

    let Some(best_of) = first.best_of else {
        return Ok(reject(
            HistoricalCandidateRejectionReason::MissingRequiredField,
            "BestOf is missing".to_owned(),
        ));
    };
    if !matches!(best_of, 3 | 5) {
        return Ok(reject(
            HistoricalCandidateRejectionReason::UnsupportedBestOf,
            format!("only BO3/BO5 are eligible, got BO{best_of}"),
        ));
    }
    let (Some(team_1_score), Some(team_2_score)) = (first.team_1_score, first.team_2_score) else {
        return Ok(reject(
            HistoricalCandidateRejectionReason::MissingRequiredField,
            "Team1Score or Team2Score is missing".to_owned(),
        ));
    };
    let scores = [team_1_score, team_2_score];
    let wins_needed = best_of / 2 + 1;
    let score_winner_index = match scores {
        [left, right] if left == wins_needed && right < wins_needed => 0,
        [left, right] if right == wins_needed && left < wins_needed => 1,
        _ => {
            return Ok(reject(
                HistoricalCandidateRejectionReason::InvalidScore,
                format!(
                    "score {}-{} is not a completed BO{best_of}",
                    scores[0], scores[1]
                ),
            ));
        }
    };
    let winner_index = match first.winner {
        Some(1) => 0,
        Some(2) => 1,
        value => {
            return Ok(reject(
                HistoricalCandidateRejectionReason::WinnerMismatch,
                format!("Winner must be 1 or 2, got {value:?}"),
            ));
        }
    };
    if winner_index != score_winner_index {
        return Ok(reject(
            HistoricalCandidateRejectionReason::WinnerMismatch,
            "Winner does not agree with final score".to_owned(),
        ));
    }

    if game_rows.is_empty() {
        return Ok(reject(
            HistoricalCandidateRejectionReason::MissingGameRows,
            "ScoreboardGames has no rows for this MatchId".to_owned(),
        ));
    }
    let patches: BTreeSet<String> = game_rows
        .iter()
        .filter_map(|row| non_empty(row.patch.as_deref()).map(str::to_owned))
        .collect();
    if game_rows
        .iter()
        .any(|row| non_empty(row.patch.as_deref()).is_none())
    {
        return Ok(reject(
            HistoricalCandidateRejectionReason::MissingPatch,
            "at least one game has no Patch".to_owned(),
        ));
    }
    if patches.len() != 1 {
        return Ok(reject(
            HistoricalCandidateRejectionReason::ConflictingPatch,
            format!("series has {} distinct patches", patches.len()),
        ));
    }

    let mut game_numbers = BTreeSet::new();
    for row in game_rows {
        let Some(game_number) = row.game_number else {
            return Ok(reject(
                HistoricalCandidateRejectionReason::MissingGameNumber,
                "at least one game has no N GameInMatch".to_owned(),
            ));
        };
        if !game_numbers.insert(game_number) {
            return Ok(reject(
                HistoricalCandidateRejectionReason::InvalidGameSequence,
                format!("duplicate game number: {game_number}"),
            ));
        }
    }
    let expected_game_count = usize::from(scores[0]) + usize::from(scores[1]);
    if game_rows.len() != expected_game_count {
        return Ok(reject(
            HistoricalCandidateRejectionReason::GameCountMismatch,
            format!(
                "ScoreboardGames rows={} but final score requires {expected_game_count}",
                game_rows.len()
            ),
        ));
    }
    let expected_numbers: BTreeSet<u16> =
        (1..=u16::try_from(expected_game_count).expect("BO5 game count always fits u16")).collect();
    if game_numbers != expected_numbers {
        return Ok(reject(
            HistoricalCandidateRejectionReason::InvalidGameSequence,
            "game numbers must be contiguous from 1".to_owned(),
        ));
    }

    let mut game_ends = Vec::with_capacity(game_rows.len());
    for row in game_rows {
        let Some(game_start) = row.game_start_utc.as_deref().and_then(parse_cargo_utc) else {
            return Ok(reject(
                HistoricalCandidateRejectionReason::InvalidGameTime,
                "GameStartUtc is missing or invalid".to_owned(),
            ));
        };
        let Some(length_minutes) = row.game_length_minutes else {
            return Ok(reject(
                HistoricalCandidateRejectionReason::InvalidGameTime,
                "Gamelength Number is missing".to_owned(),
            ));
        };
        if !length_minutes.is_finite() || length_minutes <= 0.0 {
            return Ok(reject(
                HistoricalCandidateRejectionReason::InvalidGameTime,
                format!("invalid game length: {length_minutes}"),
            ));
        }
        let milliseconds = (length_minutes * 60_000.0).round();
        if milliseconds > i64::MAX as f64 {
            return Ok(reject(
                HistoricalCandidateRejectionReason::InvalidGameTime,
                "game length overflows chrono duration".to_owned(),
            ));
        }
        game_ends.push(game_start + Duration::milliseconds(milliseconds as i64));
    }
    let completed_at_utc = *game_ends
        .iter()
        .max()
        .expect("non-empty game rows must produce game ends");
    if completed_at_utc <= scheduled_start_utc {
        return Ok(reject(
            HistoricalCandidateRejectionReason::CompletionNotAfterStart,
            "latest game end is not after Scheduled Start".to_owned(),
        ));
    }

    let team_source_keys = [team_1.to_owned(), team_2.to_owned()];
    Ok(Ok(HistoricalSeriesCandidate {
        series_id: format!("leaguepedia:{match_id}"),
        leaguepedia_match_id: match_id.to_owned(),
        competition_source_key: competition_source_key.to_owned(),
        team_source_keys: team_source_keys.clone(),
        scheduled_start_utc,
        completed_at_utc,
        best_of,
        patch: patches.into_iter().next().expect("one patch was validated"),
        scores,
        winner_team_source_key: team_source_keys[winner_index].clone(),
        result_evidence_id: format!("leaguepedia:{match_id}"),
        series_source_row_count: series_row_count,
        game_source_row_count: game_row_count,
    }))
}

fn build_coverage(
    raw_series_rows: u32,
    raw_game_rows: u32,
    distinct_match_ids: u32,
    candidates: &[HistoricalSeriesCandidate],
    rejections: &[RejectedHistoricalSeries],
) -> Result<HistoricalCandidateCoverage, HistoricalCandidateError> {
    let mut dates = BTreeSet::new();
    let mut years = BTreeSet::new();
    let mut patches = BTreeMap::new();
    let mut best_of = BTreeMap::new();
    for candidate in candidates {
        dates.insert(candidate.scheduled_start_utc.date_naive());
        years.insert(candidate.scheduled_start_utc.year());
        *patches.entry(candidate.patch.clone()).or_insert(0) += 1;
        *best_of
            .entry(format!("BO{}", candidate.best_of))
            .or_insert(0) += 1;
    }
    let mut rejection_counts = BTreeMap::new();
    for rejection in rejections {
        *rejection_counts.entry(rejection.reason).or_insert(0) += 1;
    }

    Ok(HistoricalCandidateCoverage {
        raw_series_rows,
        raw_game_rows,
        distinct_match_ids,
        candidate_count: to_u32(candidates.len(), "candidate_count")?,
        rejected_count: to_u32(rejections.len(), "rejected_count")?,
        distinct_utc_dates: to_u32(dates.len(), "distinct_utc_dates")?,
        years: years.into_iter().collect(),
        patches,
        best_of,
        rejection_counts,
    })
}

fn build_source_region_coverage(
    candidates: &[HistoricalSeriesCandidate],
    tournament_rows: &[RawHistoricalTournamentRow],
) -> Result<Option<HistoricalSourceRegionCoverage>, HistoricalCandidateError> {
    if tournament_rows.is_empty() {
        return Ok(None);
    }

    let mut relations: BTreeMap<String, BTreeSet<Option<String>>> = BTreeMap::new();
    for row in tournament_rows {
        let Some(overview_page) = non_empty(row.overview_page.as_deref()) else {
            continue;
        };
        relations
            .entry(overview_page.to_owned())
            .or_default()
            .insert(non_empty(row.region.as_deref()).map(str::to_owned));
    }

    let mut resolved_candidate_count = 0_usize;
    let mut missing_candidate_count = 0_usize;
    let mut ambiguous_candidate_count = 0_usize;
    let mut regions = BTreeMap::new();
    for candidate in candidates {
        match relations.get(&candidate.competition_source_key) {
            Some(values) if values.len() == 1 => match values.iter().next() {
                Some(Some(region)) => {
                    resolved_candidate_count += 1;
                    *regions.entry(region.clone()).or_insert(0) += 1;
                }
                _ => missing_candidate_count += 1,
            },
            Some(_) => ambiguous_candidate_count += 1,
            None => missing_candidate_count += 1,
        }
    }

    Ok(Some(HistoricalSourceRegionCoverage {
        raw_tournament_rows: to_u32(tournament_rows.len(), "raw_tournament_rows")?,
        resolved_candidate_count: to_u32(
            resolved_candidate_count,
            "resolved_region_candidate_count",
        )?,
        missing_candidate_count: to_u32(missing_candidate_count, "missing_region_candidate_count")?,
        ambiguous_candidate_count: to_u32(
            ambiguous_candidate_count,
            "ambiguous_region_candidate_count",
        )?,
        regions,
    }))
}

fn build_recovery_disjointness(
    candidates: &[HistoricalSeriesCandidate],
    reference_series_rows: &[HistoricalReferenceSeriesRow],
    minimum_recovery_start_utc: Option<DateTime<Utc>>,
) -> Result<Option<HistoricalRecoveryDisjointness>, HistoricalCandidateError> {
    let Some(minimum_new_start_utc) = minimum_recovery_start_utc else {
        if reference_series_rows.is_empty() {
            return Ok(None);
        }
        return Err(HistoricalCandidateError::IncompleteRecoveryContract);
    };
    if reference_series_rows.is_empty() {
        return Err(HistoricalCandidateError::EmptyReferenceCorpus);
    }

    let mut reference_ids = BTreeSet::new();
    for row in reference_series_rows {
        if !reference_ids.insert(row.series_id.as_str()) {
            return Err(HistoricalCandidateError::DuplicateReferenceSeries(
                row.series_id.clone(),
            ));
        }
        if row.scheduled_start_utc >= minimum_new_start_utc {
            return Err(HistoricalCandidateError::ReferenceSeriesOutsideOldWindow(
                row.series_id.clone(),
            ));
        }
    }

    for candidate in candidates {
        if candidate.scheduled_start_utc < minimum_new_start_utc {
            return Err(HistoricalCandidateError::RecoverySeriesOutsideNewWindow(
                candidate.series_id.clone(),
            ));
        }
        if reference_ids.contains(candidate.series_id.as_str()) {
            return Err(HistoricalCandidateError::RecoveryMemberOverlap(
                candidate.series_id.clone(),
            ));
        }
    }

    let reference_start_utc = reference_series_rows
        .iter()
        .map(|row| row.scheduled_start_utc)
        .min()
        .expect("non-empty reference corpus was validated");
    let reference_end_utc = reference_series_rows
        .iter()
        .map(|row| row.scheduled_start_utc)
        .max()
        .expect("non-empty reference corpus was validated");
    let new_start_utc = candidates
        .iter()
        .map(|row| row.scheduled_start_utc)
        .min()
        .expect("candidate audit rejects empty candidate sets");
    let new_end_utc = candidates
        .iter()
        .map(|row| row.scheduled_start_utc)
        .max()
        .expect("candidate audit rejects empty candidate sets");
    if new_start_utc <= reference_end_utc {
        return Err(HistoricalCandidateError::RecoverySeriesOutsideNewWindow(
            candidates
                .iter()
                .find(|row| row.scheduled_start_utc <= reference_end_utc)
                .expect("overlap was detected")
                .series_id
                .clone(),
        ));
    }

    Ok(Some(HistoricalRecoveryDisjointness {
        minimum_new_start_utc,
        reference_series_count: to_u32(reference_series_rows.len(), "reference_series_count")?,
        reference_start_utc,
        reference_end_utc,
        new_start_utc,
        new_end_utc,
        member_overlap_count: 0,
        temporal_overlap_count: 0,
    }))
}

fn parse_cargo_utc(value: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(value.trim(), "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|value| value.and_utc())
}

fn required_match_id(value: Option<&str>) -> Option<String> {
    non_empty(value).map(str::to_owned)
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn to_u32(value: usize, field: &'static str) -> Result<u32, HistoricalCandidateError> {
    u32::try_from(value).map_err(|_| HistoricalCandidateError::CountOverflow(field))
}

fn deserialize_optional_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(Value::Number(value)) => Ok(Some(value.to_string())),
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected string, number, or null; got {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp must be valid")
            .with_timezone(&Utc)
    }

    fn scope() -> HistoricalCandidateScope {
        HistoricalCandidateScope {
            start_utc: utc("2025-01-01T00:00:00Z"),
            end_utc: utc("2025-02-01T00:00:00Z"),
        }
    }

    fn series(match_id: &str) -> RawHistoricalSeriesRow {
        RawHistoricalSeriesRow {
            match_id: Some(match_id.to_owned()),
            match_start_utc: Some("2025-01-10 12:00:00".to_owned()),
            team_1: Some("Team A".to_owned()),
            team_2: Some("Team B".to_owned()),
            team_1_score: Some(2),
            team_2_score: Some(1),
            winner: Some(1),
            best_of: Some(3),
            overview_page: Some("League/2025 Season/Spring".to_owned()),
        }
    }

    fn games(match_id: &str) -> Vec<RawHistoricalGameRow> {
        (1..=3)
            .map(|game_number| RawHistoricalGameRow {
                match_id: Some(match_id.to_owned()),
                patch: Some("25.01".to_owned()),
                game_number: Some(game_number),
                game_start_utc: Some(format!("2025-01-10 1{}:00:00", game_number + 1)),
                game_length_minutes: Some(30.0),
            })
            .collect()
    }

    fn input() -> HistoricalCandidateBuildInput {
        HistoricalCandidateBuildInput {
            series_rows: vec![series("league-spring-week1-1")],
            game_rows: games("league-spring-week1-1"),
            tournament_rows: vec![],
            reference_series_rows: vec![],
            minimum_recovery_start_utc: None,
        }
    }

    #[test]
    fn builds_marketless_candidate_with_source_identity_and_completion_time() {
        let audit = build_historical_candidate_audit(scope(), input()).expect("valid input");
        assert_eq!(audit.coverage.candidate_count, 1);
        assert_eq!(audit.coverage.rejected_count, 0);
        assert_eq!(audit.candidates[0].patch, "25.01");
        assert_eq!(audit.candidates[0].winner_team_source_key, "Team A");
        assert!(audit.candidates[0].completed_at_utc > audit.candidates[0].scheduled_start_utc);
    }

    #[test]
    fn reports_exact_source_region_coverage_without_resolving_identity() {
        let mut input = input();
        input.tournament_rows = vec![RawHistoricalTournamentRow {
            overview_page: Some("League/2025 Season/Spring".to_owned()),
            region: Some("Europe".to_owned()),
        }];

        let audit = build_historical_candidate_audit(scope(), input).expect("valid region input");
        let coverage = audit
            .source_region_coverage
            .expect("source region coverage must be present");
        assert_eq!(coverage.resolved_candidate_count, 1);
        assert_eq!(coverage.missing_candidate_count, 0);
        assert_eq!(coverage.ambiguous_candidate_count, 0);
        assert_eq!(coverage.regions, BTreeMap::from([("Europe".to_owned(), 1)]));
        assert_eq!(
            audit.candidates[0].competition_source_key,
            "League/2025 Season/Spring"
        );
    }

    #[test]
    fn proves_recovery_member_and_time_disjointness() {
        let mut input = input();
        input.reference_series_rows = vec![HistoricalReferenceSeriesRow {
            series_id: "leaguepedia:old-series".to_owned(),
            scheduled_start_utc: utc("2024-12-31T23:00:00Z"),
        }];
        input.minimum_recovery_start_utc = Some(utc("2025-01-01T00:00:00Z"));

        let audit = build_historical_candidate_audit(scope(), input).expect("disjoint recovery");
        let proof = audit
            .recovery_disjointness
            .expect("recovery proof must be present");
        assert_eq!(proof.reference_series_count, 1);
        assert_eq!(proof.member_overlap_count, 0);
        assert_eq!(proof.temporal_overlap_count, 0);
        assert!(proof.reference_end_utc < proof.new_start_utc);
    }

    #[test]
    fn fails_closed_on_recovery_member_overlap() {
        let mut input = input();
        input.reference_series_rows = vec![HistoricalReferenceSeriesRow {
            series_id: "leaguepedia:league-spring-week1-1".to_owned(),
            scheduled_start_utc: utc("2024-12-31T23:00:00Z"),
        }];
        input.minimum_recovery_start_utc = Some(utc("2025-01-01T00:00:00Z"));

        assert_eq!(
            build_historical_candidate_audit(scope(), input),
            Err(HistoricalCandidateError::RecoveryMemberOverlap(
                "leaguepedia:league-spring-week1-1".to_owned()
            ))
        );
    }

    #[test]
    fn rejects_unsupported_best_of_without_aborting_audit() {
        let mut input = input();
        input.series_rows[0].best_of = Some(1);
        input.series_rows[0].team_1_score = Some(1);
        input.series_rows[0].team_2_score = Some(0);
        input.game_rows.truncate(1);

        assert_eq!(
            build_historical_candidate_audit(scope(), input),
            Err(HistoricalCandidateError::NoCandidates)
        );

        let mut mixed = HistoricalCandidateBuildInput {
            series_rows: vec![series("valid"), series("bo1")],
            game_rows: [games("valid"), games("bo1")].concat(),
            tournament_rows: vec![],
            reference_series_rows: vec![],
            minimum_recovery_start_utc: None,
        };
        mixed.series_rows[1].best_of = Some(1);
        let audit =
            build_historical_candidate_audit(scope(), mixed).expect("one candidate remains");
        assert_eq!(audit.coverage.candidate_count, 1);
        assert_eq!(
            audit.rejections[0].reason,
            HistoricalCandidateRejectionReason::UnsupportedBestOf
        );
    }

    #[test]
    fn rejects_conflicting_patch() {
        let mut input = HistoricalCandidateBuildInput {
            series_rows: vec![series("valid"), series("conflict")],
            game_rows: [games("valid"), games("conflict")].concat(),
            tournament_rows: vec![],
            reference_series_rows: vec![],
            minimum_recovery_start_utc: None,
        };
        input.game_rows[4].patch = Some("25.02".to_owned());
        let audit = build_historical_candidate_audit(scope(), input).expect("valid row remains");
        assert_eq!(
            audit.rejections[0].reason,
            HistoricalCandidateRejectionReason::ConflictingPatch
        );
    }

    #[test]
    fn rejects_game_count_mismatch_and_duplicate_sequence() {
        let mut count_input = HistoricalCandidateBuildInput {
            series_rows: vec![series("valid"), series("count")],
            game_rows: [games("valid"), games("count")].concat(),
            tournament_rows: vec![],
            reference_series_rows: vec![],
            minimum_recovery_start_utc: None,
        };
        count_input.game_rows.pop();
        let count_audit =
            build_historical_candidate_audit(scope(), count_input).expect("valid remains");
        assert_eq!(
            count_audit.rejections[0].reason,
            HistoricalCandidateRejectionReason::GameCountMismatch
        );

        let mut sequence_input = HistoricalCandidateBuildInput {
            series_rows: vec![series("valid"), series("sequence")],
            game_rows: [games("valid"), games("sequence")].concat(),
            tournament_rows: vec![],
            reference_series_rows: vec![],
            minimum_recovery_start_utc: None,
        };
        sequence_input.game_rows[4].game_number = Some(1);
        let sequence_audit =
            build_historical_candidate_audit(scope(), sequence_input).expect("valid remains");
        assert_eq!(
            sequence_audit.rejections[0].reason,
            HistoricalCandidateRejectionReason::InvalidGameSequence
        );
    }

    #[test]
    fn rejects_score_and_winner_disagreement() {
        let mut input = HistoricalCandidateBuildInput {
            series_rows: vec![series("valid"), series("winner")],
            game_rows: [games("valid"), games("winner")].concat(),
            tournament_rows: vec![],
            reference_series_rows: vec![],
            minimum_recovery_start_utc: None,
        };
        input.series_rows[1].winner = Some(2);
        let audit = build_historical_candidate_audit(scope(), input).expect("valid remains");
        assert_eq!(
            audit.rejections[0].reason,
            HistoricalCandidateRejectionReason::WinnerMismatch
        );
    }

    #[test]
    fn output_is_deterministic_across_input_order() {
        let forward_input = HistoricalCandidateBuildInput {
            series_rows: vec![series("a"), series("b")],
            game_rows: [games("a"), games("b")].concat(),
            tournament_rows: vec![],
            reference_series_rows: vec![],
            minimum_recovery_start_utc: None,
        };
        let mut reverse_input = forward_input.clone();
        reverse_input.series_rows.reverse();
        reverse_input.game_rows.reverse();

        let forward = build_historical_candidate_audit(scope(), forward_input).expect("forward");
        let reverse = build_historical_candidate_audit(scope(), reverse_input).expect("reverse");
        assert_eq!(forward, reverse);
    }

    #[test]
    fn fails_closed_on_orphan_game_rows() {
        let mut input = input();
        input.game_rows.push(RawHistoricalGameRow {
            match_id: Some("orphan".to_owned()),
            patch: Some("25.01".to_owned()),
            game_number: Some(1),
            game_start_utc: Some("2025-01-10 12:30:00".to_owned()),
            game_length_minutes: Some(30.0),
        });
        assert_eq!(
            build_historical_candidate_audit(scope(), input),
            Err(HistoricalCandidateError::OrphanGameRows(
                "orphan".to_owned()
            ))
        );
    }

    #[test]
    fn preserves_numeric_patch_as_source_text() {
        let row: RawHistoricalGameRow = serde_json::from_str(
            r#"{"MatchId":"m","Patch":25.1,"N GameInMatch":1,"GameStartUtc":"2025-01-10 12:00:00","Gamelength Number":30}"#,
        )
        .expect("Cargo numeric Patch must deserialize");
        assert_eq!(row.patch.as_deref(), Some("25.1"));
    }

    #[test]
    fn reports_missing_game_rows_instead_of_hiding_series() {
        let input = HistoricalCandidateBuildInput {
            series_rows: vec![series("valid"), series("missing-games")],
            game_rows: games("valid"),
            tournament_rows: vec![],
            reference_series_rows: vec![],
            minimum_recovery_start_utc: None,
        };
        let audit = build_historical_candidate_audit(scope(), input).expect("valid remains");
        assert_eq!(audit.coverage.candidate_count, 1);
        assert_eq!(audit.coverage.rejected_count, 1);
        assert_eq!(
            audit.rejections[0].reason,
            HistoricalCandidateRejectionReason::MissingGameRows
        );
    }
}
