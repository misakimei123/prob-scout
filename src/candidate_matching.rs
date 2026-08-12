use std::{collections::BTreeSet, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::event_mapping::{
    DataSource, Event, MarketMapping, MarketOutcome, ObservedTimeKind, TeamAlias,
    normalize_team_name,
};

/// DATA-007 对一个 Polymarket 候选给出的置信状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    Matched,
    NeedsReview,
    Rejected,
}

/// Polymarket outcome 与 token 的来源顺序对；数组位置就是 API outcome index。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketCandidateOutcome {
    pub observed_name: String,
    pub token_id: String,
}

/// Gamma Match Winner 候选与同一 market 的可选 CLOB 开赛时间证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketCandidate {
    pub polymarket_event_id: String,
    pub market_id: String,
    pub condition_id: String,
    pub best_of: u8,
    pub outcomes: [MarketCandidateOutcome; 2],
    pub gamma_end_date_utc: DateTime<Utc>,
    pub clob_game_start_time_utc: Option<DateTime<Utc>>,
}

/// 状态原因保留可复核字段，供 DATA-008 人工队列消费，但本任务不实现人工核验流程。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum MatchReason {
    EvidenceAligned {
        start_time_span_seconds: i64,
    },
    InvalidMarketCandidate {
        field: String,
    },
    MissingAlias {
        source: DataSource,
        observed_name: String,
    },
    AmbiguousAlias {
        source: DataSource,
        observed_name: String,
        canonical_team_ids: Vec<String>,
    },
    DuplicateOutcomeTeams {
        canonical_team_id: String,
    },
    TeamPairMismatch {
        canonical_team_ids: [String; 2],
    },
    BestOfMismatch {
        market_best_of: u8,
        event_candidates: Vec<(String, u8)>,
    },
    AmbiguousEventCandidates {
        event_ids: Vec<String>,
    },
    InvalidEventCandidate {
        event_id: String,
        error: String,
    },
    EventTeamEvidenceMismatch {
        event_id: String,
        source: DataSource,
        observed_name: String,
        expected_team_id: String,
        resolved_team_id: String,
    },
    MissingScheduledStart {
        event_id: String,
    },
    MissingClobGameStart,
    StartTimeConflict {
        event_id: String,
        span_seconds: i64,
        tolerance_seconds: i64,
    },
    MappingContractViolation {
        event_id: String,
        error: String,
    },
}

/// 候选匹配结果；只有 `Matched` 才携带可写入的正式映射。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateMatch {
    pub status: MatchStatus,
    pub polymarket_event_id: String,
    pub market_id: String,
    pub event_id: Option<String>,
    pub reasons: Vec<MatchReason>,
    pub mapping: Option<MarketMapping>,
}

/// 按 Gamma 输入顺序处理市场候选，保证输出可以与原始 fixture 逐行对照。
pub fn match_market_candidates(
    events: &[Event],
    markets: &[MarketCandidate],
    aliases: &[TeamAlias],
    start_time_tolerance: Duration,
) -> Vec<CandidateMatch> {
    markets
        .iter()
        .map(|market| match_market_candidate(events, market, aliases, start_time_tolerance))
        .collect()
}

/// 从多个内部 Event 候选中为一个 Polymarket Match Winner 市场选择唯一映射。
///
/// 队伍或 BO 的直接矛盾属于 `Rejected`；缺失证据、时间冲突和多候选歧义属于
/// `NeedsReview`。Gamma Market End 只被原样写入映射，不参与开赛时间比较。
pub fn match_market_candidate(
    events: &[Event],
    market: &MarketCandidate,
    aliases: &[TeamAlias],
    start_time_tolerance: Duration,
) -> CandidateMatch {
    if let Some(field) = invalid_market_field(market) {
        return result(
            market,
            MatchStatus::Rejected,
            None,
            MatchReason::InvalidMarketCandidate {
                field: field.to_owned(),
            },
        );
    }

    // 必须逐个 outcome 按原始 index 解析，禁止为了匹配方便排序 outcome 或 token。
    let mut resolved_outcomes = Vec::with_capacity(2);
    for outcome in &market.outcomes {
        match resolve_alias(aliases, DataSource::PolymarketGamma, &outcome.observed_name) {
            AliasResolution::Resolved(team_id) => resolved_outcomes.push(team_id),
            AliasResolution::Missing => {
                return result(
                    market,
                    MatchStatus::NeedsReview,
                    None,
                    MatchReason::MissingAlias {
                        source: DataSource::PolymarketGamma,
                        observed_name: outcome.observed_name.clone(),
                    },
                );
            }
            AliasResolution::Ambiguous(canonical_team_ids) => {
                return result(
                    market,
                    MatchStatus::NeedsReview,
                    None,
                    MatchReason::AmbiguousAlias {
                        source: DataSource::PolymarketGamma,
                        observed_name: outcome.observed_name.clone(),
                        canonical_team_ids,
                    },
                );
            }
        }
    }

    if resolved_outcomes[0] == resolved_outcomes[1] {
        return result(
            market,
            MatchStatus::Rejected,
            None,
            MatchReason::DuplicateOutcomeTeams {
                canonical_team_id: resolved_outcomes[0].clone(),
            },
        );
    }
    let resolved_team_ids = [resolved_outcomes[0].clone(), resolved_outcomes[1].clone()];

    let team_candidates = events
        .iter()
        .filter(|event| same_team_pair(&event.canonical_team_ids, &resolved_team_ids))
        .collect::<Vec<_>>();
    if team_candidates.is_empty() {
        return result(
            market,
            MatchStatus::Rejected,
            None,
            MatchReason::TeamPairMismatch {
                canonical_team_ids: resolved_team_ids,
            },
        );
    }

    let format_candidates = team_candidates
        .iter()
        .copied()
        .filter(|event| event.best_of == market.best_of)
        .collect::<Vec<_>>();
    if format_candidates.is_empty() {
        let mut event_candidates = team_candidates
            .iter()
            .map(|event| (event.event_id.clone(), event.best_of))
            .collect::<Vec<_>>();
        event_candidates.sort();
        return result(
            market,
            MatchStatus::Rejected,
            None,
            MatchReason::BestOfMismatch {
                market_best_of: market.best_of,
                event_candidates,
            },
        );
    }

    // 同队伍和 BO 出现多个 Event 时，时间筛选仍可能掩盖重复或改期数据，必须人工判定。
    if format_candidates.len() > 1 {
        let mut event_ids = format_candidates
            .iter()
            .map(|event| event.event_id.clone())
            .collect::<Vec<_>>();
        event_ids.sort();
        return result(
            market,
            MatchStatus::NeedsReview,
            None,
            MatchReason::AmbiguousEventCandidates { event_ids },
        );
    }

    let event = format_candidates[0];
    if let Err(error) = event.validate() {
        return result(
            market,
            MatchStatus::Rejected,
            Some(event.event_id.clone()),
            MatchReason::InvalidEventCandidate {
                event_id: event.event_id.clone(),
                error: error.to_string(),
            },
        );
    }

    // Event 来源证据也必须通过显式 alias 回到约定的内部队伍，避免只校验市场一侧。
    if let Some(reason) = validate_event_team_evidence(event, aliases) {
        let status = match reason {
            MatchReason::EventTeamEvidenceMismatch { .. } => MatchStatus::Rejected,
            _ => MatchStatus::NeedsReview,
        };
        return result(market, status, Some(event.event_id.clone()), reason);
    }

    let scheduled_starts = event
        .source_evidence
        .iter()
        .filter(|evidence| evidence.observed_time_kind == ObservedTimeKind::ScheduledStart)
        .map(|evidence| evidence.observed_time_utc.timestamp())
        .collect::<Vec<_>>();
    if scheduled_starts.is_empty() {
        return result(
            market,
            MatchStatus::NeedsReview,
            Some(event.event_id.clone()),
            MatchReason::MissingScheduledStart {
                event_id: event.event_id.clone(),
            },
        );
    }
    let Some(clob_game_start_time_utc) = market.clob_game_start_time_utc else {
        return result(
            market,
            MatchStatus::NeedsReview,
            Some(event.event_id.clone()),
            MatchReason::MissingClobGameStart,
        );
    };

    let tolerance_seconds = i64::try_from(start_time_tolerance.as_secs()).unwrap_or(i64::MAX);
    let mut start_timestamps = scheduled_starts;
    start_timestamps.push(clob_game_start_time_utc.timestamp());
    let minimum = start_timestamps.iter().min().copied().unwrap_or_default();
    let maximum = start_timestamps.iter().max().copied().unwrap_or_default();
    let start_time_span_seconds = maximum.saturating_sub(minimum);
    if start_time_span_seconds > tolerance_seconds {
        return result(
            market,
            MatchStatus::NeedsReview,
            Some(event.event_id.clone()),
            MatchReason::StartTimeConflict {
                event_id: event.event_id.clone(),
                span_seconds: start_time_span_seconds,
                tolerance_seconds,
            },
        );
    }

    let mapping = MarketMapping {
        event_id: event.event_id.clone(),
        polymarket_event_id: market.polymarket_event_id.clone(),
        market_id: market.market_id.clone(),
        condition_id: market.condition_id.clone(),
        outcomes: [
            MarketOutcome {
                outcome_index: 0,
                canonical_team_id: resolved_outcomes[0].clone(),
                observed_name: market.outcomes[0].observed_name.clone(),
                token_id: market.outcomes[0].token_id.clone(),
            },
            MarketOutcome {
                outcome_index: 1,
                canonical_team_id: resolved_outcomes[1].clone(),
                observed_name: market.outcomes[1].observed_name.clone(),
                token_id: market.outcomes[1].token_id.clone(),
            },
        ],
        gamma_end_date_utc: market.gamma_end_date_utc,
        clob_game_start_time_utc,
    };

    if let Err(error) = mapping.validate_against(event, aliases) {
        return result(
            market,
            MatchStatus::Rejected,
            Some(event.event_id.clone()),
            MatchReason::MappingContractViolation {
                event_id: event.event_id.clone(),
                error: error.to_string(),
            },
        );
    }

    CandidateMatch {
        status: MatchStatus::Matched,
        polymarket_event_id: market.polymarket_event_id.clone(),
        market_id: market.market_id.clone(),
        event_id: Some(event.event_id.clone()),
        reasons: vec![MatchReason::EvidenceAligned {
            start_time_span_seconds,
        }],
        mapping: Some(mapping),
    }
}

fn invalid_market_field(market: &MarketCandidate) -> Option<&'static str> {
    for (field, value) in [
        (
            "market.polymarket_event_id",
            market.polymarket_event_id.as_str(),
        ),
        ("market.market_id", market.market_id.as_str()),
        ("market.condition_id", market.condition_id.as_str()),
        (
            "market.outcome_0.name",
            market.outcomes[0].observed_name.as_str(),
        ),
        (
            "market.outcome_0.token_id",
            market.outcomes[0].token_id.as_str(),
        ),
        (
            "market.outcome_1.name",
            market.outcomes[1].observed_name.as_str(),
        ),
        (
            "market.outcome_1.token_id",
            market.outcomes[1].token_id.as_str(),
        ),
    ] {
        if value.trim().is_empty() {
            return Some(field);
        }
    }
    if !matches!(market.best_of, 1 | 3 | 5) {
        return Some("market.best_of");
    }
    if market.outcomes[0].token_id == market.outcomes[1].token_id {
        return Some("market.outcome_token_ids");
    }
    None
}

fn result(
    market: &MarketCandidate,
    status: MatchStatus,
    event_id: Option<String>,
    reason: MatchReason,
) -> CandidateMatch {
    CandidateMatch {
        status,
        polymarket_event_id: market.polymarket_event_id.clone(),
        market_id: market.market_id.clone(),
        event_id,
        reasons: vec![reason],
        mapping: None,
    }
}

fn same_team_pair(left: &[String; 2], right: &[String; 2]) -> bool {
    (left[0] == right[0] && left[1] == right[1]) || (left[0] == right[1] && left[1] == right[0])
}

enum AliasResolution {
    Resolved(String),
    Missing,
    Ambiguous(Vec<String>),
}

fn resolve_alias(
    aliases: &[TeamAlias],
    source: DataSource,
    observed_name: &str,
) -> AliasResolution {
    let normalized_name = normalize_team_name(observed_name);
    let canonical_team_ids = aliases
        .iter()
        .filter(|alias| {
            alias.source == source
                && alias.normalized_name == normalized_name
                && alias.normalized_name == normalize_team_name(&alias.observed_name)
        })
        .map(|alias| alias.canonical_team_id.clone())
        .collect::<BTreeSet<_>>();

    match canonical_team_ids.len() {
        0 => AliasResolution::Missing,
        1 => AliasResolution::Resolved(
            canonical_team_ids
                .into_iter()
                .next()
                .expect("单元素集合必须存在队伍 ID"),
        ),
        _ => AliasResolution::Ambiguous(canonical_team_ids.into_iter().collect()),
    }
}

fn validate_event_team_evidence(event: &Event, aliases: &[TeamAlias]) -> Option<MatchReason> {
    for evidence in &event.source_evidence {
        for (index, observed_name) in evidence.observed_team_names.iter().enumerate() {
            match resolve_alias(aliases, evidence.source, observed_name) {
                AliasResolution::Resolved(resolved_team_id) => {
                    if resolved_team_id != event.canonical_team_ids[index] {
                        return Some(MatchReason::EventTeamEvidenceMismatch {
                            event_id: event.event_id.clone(),
                            source: evidence.source,
                            observed_name: observed_name.clone(),
                            expected_team_id: event.canonical_team_ids[index].clone(),
                            resolved_team_id,
                        });
                    }
                }
                AliasResolution::Missing => {
                    return Some(MatchReason::MissingAlias {
                        source: evidence.source,
                        observed_name: observed_name.clone(),
                    });
                }
                AliasResolution::Ambiguous(canonical_team_ids) => {
                    return Some(MatchReason::AmbiguousAlias {
                        source: evidence.source,
                        observed_name: observed_name.clone(),
                        canonical_team_ids,
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, time::Duration};

    use chrono::{DateTime, Utc};
    use serde::Deserialize;

    use super::{
        MarketCandidate, MarketCandidateOutcome, MatchReason, MatchStatus, match_market_candidate,
        match_market_candidates,
    };
    use crate::event_mapping::{
        DataSource, Event, EventSourceEvidence, ObservedTimeKind, TeamAlias,
    };

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("测试时间必须是 RFC3339")
            .with_timezone(&Utc)
    }

    fn sample_input() -> (Vec<Event>, Vec<TeamAlias>, MarketCandidate) {
        let event = Event {
            event_id: "lol:lck:2026-08-12:dns-ns".to_owned(),
            game: "league_of_legends".to_owned(),
            competition: "LCK/2026 Season/Rounds 3-4".to_owned(),
            best_of: 3,
            canonical_team_ids: [
                "lol-team:dn-soopers".to_owned(),
                "lol-team:nongshim".to_owned(),
            ],
            source_evidence: vec![EventSourceEvidence {
                source: DataSource::Leaguepedia,
                source_event_id: "LCK/2026 Season/Rounds 3-4_Week 12_1".to_owned(),
                observed_team_names: ["DN SOOPers".to_owned(), "Nongshim RedForce".to_owned()],
                observed_time_utc: utc("2026-08-12T08:00:00Z"),
                observed_time_kind: ObservedTimeKind::ScheduledStart,
            }],
        };
        let aliases = vec![
            TeamAlias::new(
                "lol-team:dn-soopers",
                DataSource::Leaguepedia,
                Some("DN SOOPers".to_owned()),
                "DN SOOPers",
            ),
            TeamAlias::new(
                "lol-team:nongshim",
                DataSource::Leaguepedia,
                Some("Nongshim RedForce".to_owned()),
                "Nongshim RedForce",
            ),
            TeamAlias::new(
                "lol-team:dn-soopers",
                DataSource::PolymarketGamma,
                None,
                "DN SOOPers",
            ),
            TeamAlias::new(
                "lol-team:nongshim",
                DataSource::PolymarketGamma,
                None,
                "Nongshim Red Force",
            ),
        ];
        let market = MarketCandidate {
            polymarket_event_id: "816302".to_owned(),
            market_id: "3422466".to_owned(),
            condition_id: "0x621f09a374447eb0965f70f78e67bb79dd773e7ca76a7646f1dd94b787597968"
                .to_owned(),
            best_of: 3,
            outcomes: [
                MarketCandidateOutcome {
                    observed_name: "DN SOOPers".to_owned(),
                    token_id: "token-dns".to_owned(),
                },
                MarketCandidateOutcome {
                    observed_name: "Nongshim Red Force".to_owned(),
                    token_id: "token-ns".to_owned(),
                },
            ],
            gamma_end_date_utc: utc("2026-08-12T14:00:00Z"),
            clob_game_start_time_utc: Some(utc("2026-08-12T08:00:00Z")),
        };
        (vec![event], aliases, market)
    }

    #[test]
    fn matches_unique_real_candidate_and_preserves_outcome_order() {
        let (events, aliases, market) = sample_input();

        let matched = match_market_candidate(&events, &market, &aliases, Duration::from_secs(300));

        assert_eq!(matched.status, MatchStatus::Matched);
        assert_eq!(
            matched.event_id.as_deref(),
            Some(events[0].event_id.as_str())
        );
        assert_eq!(
            matched.reasons,
            vec![MatchReason::EvidenceAligned {
                start_time_span_seconds: 0
            }]
        );
        let mapping = matched.mapping.expect("唯一一致候选必须生成正式映射");
        assert_eq!(mapping.outcomes[0].outcome_index, 0);
        assert_eq!(mapping.outcomes[0].token_id, "token-dns");
        assert_eq!(mapping.outcomes[1].outcome_index, 1);
        assert_eq!(mapping.outcomes[1].token_id, "token-ns");
        assert_eq!(mapping.gamma_end_offset_from_clob_start_seconds(), 21_600);
    }

    #[test]
    fn rejects_team_pair_contradiction() {
        let (events, mut aliases, mut market) = sample_input();
        aliases.push(TeamAlias::new(
            "lol-team:t1",
            DataSource::PolymarketGamma,
            None,
            "T1",
        ));
        market.outcomes[1].observed_name = "T1".to_owned();

        let rejected = match_market_candidate(&events, &market, &aliases, Duration::from_secs(300));

        assert_eq!(rejected.status, MatchStatus::Rejected);
        assert!(matches!(
            rejected.reasons.as_slice(),
            [MatchReason::TeamPairMismatch { .. }]
        ));
        assert!(rejected.mapping.is_none());
    }

    #[test]
    fn rejects_best_of_contradiction() {
        let (events, aliases, mut market) = sample_input();
        market.best_of = 5;

        let rejected = match_market_candidate(&events, &market, &aliases, Duration::from_secs(300));

        assert_eq!(rejected.status, MatchStatus::Rejected);
        assert!(matches!(
            rejected.reasons.as_slice(),
            [MatchReason::BestOfMismatch { .. }]
        ));
    }

    #[test]
    fn sends_start_time_conflict_to_review_without_using_gamma_end() {
        let (mut events, aliases, market) = sample_input();
        events[0].source_evidence[0].observed_time_utc = utc("2026-08-12T07:45:00Z");

        let needs_review =
            match_market_candidate(&events, &market, &aliases, Duration::from_secs(300));

        assert_eq!(needs_review.status, MatchStatus::NeedsReview);
        assert_eq!(
            needs_review.reasons,
            vec![MatchReason::StartTimeConflict {
                event_id: events[0].event_id.clone(),
                span_seconds: 900,
                tolerance_seconds: 300,
            }]
        );
        assert!(needs_review.mapping.is_none());
    }

    #[test]
    fn sends_missing_explicit_alias_to_review_instead_of_guessing() {
        let (events, mut aliases, mut market) = sample_input();
        aliases.retain(|alias| alias.observed_name != "Nongshim Red Force");
        market.outcomes[1].observed_name = "NS".to_owned();

        let needs_review =
            match_market_candidate(&events, &market, &aliases, Duration::from_secs(300));

        assert_eq!(needs_review.status, MatchStatus::NeedsReview);
        assert_eq!(
            needs_review.reasons,
            vec![MatchReason::MissingAlias {
                source: DataSource::PolymarketGamma,
                observed_name: "NS".to_owned(),
            }]
        );
    }

    #[test]
    fn sends_missing_clob_start_to_review() {
        let (events, aliases, mut market) = sample_input();
        market.clob_game_start_time_utc = None;

        let needs_review =
            match_market_candidate(&events, &market, &aliases, Duration::from_secs(300));

        assert_eq!(needs_review.status, MatchStatus::NeedsReview);
        assert_eq!(
            needs_review.reasons,
            vec![MatchReason::MissingClobGameStart]
        );
    }

    #[test]
    fn sends_duplicate_event_candidates_to_review() {
        let (mut events, aliases, market) = sample_input();
        let mut duplicate = events[0].clone();
        duplicate.event_id = "lol:lck:2026-08-12:dns-ns:duplicate".to_owned();
        events.push(duplicate);

        let needs_review =
            match_market_candidate(&events, &market, &aliases, Duration::from_secs(300));

        assert_eq!(needs_review.status, MatchStatus::NeedsReview);
        assert!(matches!(
            needs_review.reasons.as_slice(),
            [MatchReason::AmbiguousEventCandidates { .. }]
        ));
    }

    #[test]
    fn matches_market_batch_in_source_order() {
        let (events, aliases, market) = sample_input();
        let mut conflicting_market = market.clone();
        conflicting_market.market_id = "3422466-bo5".to_owned();
        conflicting_market.best_of = 5;

        let results = match_market_candidates(
            &events,
            &[market, conflicting_market],
            &aliases,
            Duration::from_secs(300),
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].market_id, "3422466");
        assert_eq!(results[0].status, MatchStatus::Matched);
        assert_eq!(results[1].market_id, "3422466-bo5");
        assert_eq!(results[1].status, MatchStatus::Rejected);
    }

    #[derive(Debug, Deserialize)]
    struct ReviewRow {
        review_id: String,
        market_id: String,
        polymarket_event_id: String,
        condition_id: String,
        gamma_title: String,
        gamma_outcome_0: String,
        gamma_token_0: String,
        gamma_outcome_1: String,
        gamma_token_1: String,
        gamma_market_end_utc: String,
        clob_game_start_utc: String,
        best_of: u8,
        leaguepedia_match_id: String,
        leaguepedia_start_utc: String,
        leaguepedia_team_1: String,
        leaguepedia_team_2: String,
        leaguepedia_team_1_outcome_index: usize,
        start_delta_seconds: i64,
        expected_status: String,
        manual_result: String,
        error_class: String,
        review_notes: String,
    }

    #[test]
    fn replays_all_fifty_manually_reviewed_mappings_without_false_matches() {
        const REVIEW_FIXTURE: &str = include_str!("../docs/DATA_008_MAPPING_REVIEW.csv");

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(REVIEW_FIXTURE.as_bytes());
        let rows = reader
            .deserialize::<ReviewRow>()
            .map(|row| row.expect("DATA-008 核验表必须是有效 CSV"))
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 50, "DATA-008 必须完整重放 50 场");

        let mut market_ids = BTreeSet::new();
        let mut matched_count = 0;
        let mut needs_review_count = 0;
        for row in rows {
            assert!(
                market_ids.insert(row.market_id.clone()),
                "market ID 必须唯一"
            );
            assert!(!row.review_id.is_empty());
            assert!(row.gamma_title.contains("LoL:"));

            let team_ids = [
                format!("review:{}:team:0", row.market_id),
                format!("review:{}:team:1", row.market_id),
            ];
            let first_index = row.leaguepedia_team_1_outcome_index;
            assert!(first_index <= 1, "Leaguepedia team 1 必须对应一个 outcome");
            let second_index = 1 - first_index;
            let event = Event {
                event_id: format!("review:{}", row.market_id),
                game: "league_of_legends".to_owned(),
                competition: row.leaguepedia_match_id.clone(),
                best_of: row.best_of,
                canonical_team_ids: [
                    team_ids[first_index].clone(),
                    team_ids[second_index].clone(),
                ],
                source_evidence: vec![EventSourceEvidence {
                    source: DataSource::Leaguepedia,
                    source_event_id: row.leaguepedia_match_id.clone(),
                    observed_team_names: [
                        row.leaguepedia_team_1.clone(),
                        row.leaguepedia_team_2.clone(),
                    ],
                    observed_time_utc: utc(&row.leaguepedia_start_utc),
                    observed_time_kind: ObservedTimeKind::ScheduledStart,
                }],
            };
            let aliases = vec![
                TeamAlias::new(
                    &team_ids[0],
                    DataSource::PolymarketGamma,
                    None,
                    &row.gamma_outcome_0,
                ),
                TeamAlias::new(
                    &team_ids[1],
                    DataSource::PolymarketGamma,
                    None,
                    &row.gamma_outcome_1,
                ),
                TeamAlias::new(
                    &team_ids[first_index],
                    DataSource::Leaguepedia,
                    None,
                    &row.leaguepedia_team_1,
                ),
                TeamAlias::new(
                    &team_ids[second_index],
                    DataSource::Leaguepedia,
                    None,
                    &row.leaguepedia_team_2,
                ),
            ];
            let market = MarketCandidate {
                polymarket_event_id: row.polymarket_event_id.clone(),
                market_id: row.market_id.clone(),
                condition_id: row.condition_id.clone(),
                best_of: row.best_of,
                outcomes: [
                    MarketCandidateOutcome {
                        observed_name: row.gamma_outcome_0.clone(),
                        token_id: row.gamma_token_0.clone(),
                    },
                    MarketCandidateOutcome {
                        observed_name: row.gamma_outcome_1.clone(),
                        token_id: row.gamma_token_1.clone(),
                    },
                ],
                gamma_end_date_utc: utc(&row.gamma_market_end_utc),
                clob_game_start_time_utc: Some(utc(&row.clob_game_start_utc)),
            };

            // 人工核验沿用 DATA-006 的 5 分钟容忍值；Gamma Market End 只作为映射字段传入。
            let result =
                match_market_candidate(&[event], &market, &aliases, Duration::from_secs(5 * 60));
            let expected = match row.expected_status.as_str() {
                "Matched" => MatchStatus::Matched,
                "NeedsReview" => MatchStatus::NeedsReview,
                other => panic!("DATA-008 出现未审核状态：{other}"),
            };
            assert_eq!(
                result.status, expected,
                "review_id={} market_id={} notes={}",
                row.review_id, row.market_id, row.review_notes
            );

            match expected {
                MatchStatus::Matched => {
                    matched_count += 1;
                    assert_eq!(row.manual_result, "verified_correct");
                    assert_eq!(row.error_class, "none");
                    assert!(row.start_delta_seconds <= 5 * 60);
                    let mapping = result.mapping.expect("人工确认正确的匹配必须生成映射");
                    assert_eq!(mapping.outcomes[0].observed_name, row.gamma_outcome_0);
                    assert_eq!(mapping.outcomes[0].token_id, row.gamma_token_0);
                    assert_eq!(mapping.outcomes[1].observed_name, row.gamma_outcome_1);
                    assert_eq!(mapping.outcomes[1].token_id, row.gamma_token_1);
                }
                MatchStatus::NeedsReview => {
                    needs_review_count += 1;
                    assert_eq!(row.manual_result, "correctly_escalated");
                    assert_eq!(row.error_class, "start_time_conflict");
                    assert!(row.start_delta_seconds > 5 * 60);
                    assert!(result.mapping.is_none());
                    assert!(matches!(
                        result.reasons.as_slice(),
                        [MatchReason::StartTimeConflict { .. }]
                    ));
                }
                MatchStatus::Rejected => unreachable!("核验 fixture 不包含硬矛盾样本"),
            }
        }

        assert_eq!(matched_count, 29);
        assert_eq!(needs_review_count, 21);
    }
}
