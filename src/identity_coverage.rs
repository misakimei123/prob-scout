use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    event_mapping::DataSource,
    historical_candidates::{HistoricalCandidateAudit, HistoricalSeriesCandidate},
    identity_registry::{IdentityRegistry, IdentityRegistryError, IdentityResolution},
};

/// HIST-009 只审计现有显式 identity evidence，不创建或猜测 Canonical identity。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityCoverageBuildInput {
    pub candidate_audit: HistoricalCandidateAudit,
    pub registry: IdentityRegistry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityKind {
    Team,
    Competition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityCoverageStatus {
    Resolved,
    Missing,
    Ambiguous,
}

/// 单个 source key 在赛事计划开始时刻的解析结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentityResolution {
    pub source_key: String,
    pub status: IdentityCoverageStatus,
    pub canonical_id: Option<String>,
    pub ambiguous_canonical_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesIdentityResolution {
    pub series_id: String,
    pub scheduled_start_utc: DateTime<Utc>,
    pub competition: SourceIdentityResolution,
    pub teams: [SourceIdentityResolution; 2],
    pub fully_resolved: bool,
}

/// 将相同 source key 和相同失败状态聚合，避免逐场人工重复补证。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityReviewQueueItem {
    pub identity_kind: IdentityKind,
    pub source_key: String,
    pub status: IdentityCoverageStatus,
    pub ambiguous_canonical_ids: Vec<String>,
    pub first_seen_utc: DateTime<Utc>,
    pub last_seen_utc: DateTime<Utc>,
    pub occurrence_count: u32,
    pub affected_series_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityResolutionCounts {
    pub resolved: u32,
    pub missing: u32,
    pub ambiguous: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityCoverageSummary {
    pub candidate_count: u32,
    pub fully_resolved_series: u32,
    pub blocked_series: u32,
    pub distinct_team_source_keys: u32,
    pub distinct_competition_source_keys: u32,
    pub team_occurrences: IdentityResolutionCounts,
    pub competition_occurrences: IdentityResolutionCounts,
    pub review_queue_items: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityCoverageAudit {
    pub audit_version: u16,
    pub source_candidate_audit_version: u16,
    pub summary: IdentityCoverageSummary,
    pub series_resolutions: Vec<SeriesIdentityResolution>,
    pub review_queue: Vec<IdentityReviewQueueItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityCoverageError {
    Registry(IdentityRegistryError),
    UnsupportedCandidateAuditVersion(u16),
    EmptyCandidates,
    CandidateCountMismatch {
        declared: u32,
        actual: usize,
    },
    DuplicateSeriesId(String),
    InvalidCandidate {
        series_id: String,
        detail: &'static str,
    },
    CountOverflow(&'static str),
}

impl fmt::Display for IdentityCoverageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => write!(formatter, "identity registry is invalid: {error}"),
            Self::UnsupportedCandidateAuditVersion(version) => {
                write!(formatter, "unsupported candidate audit version: {version}")
            }
            Self::EmptyCandidates => {
                formatter.write_str("identity coverage input has no candidates")
            }
            Self::CandidateCountMismatch { declared, actual } => write!(
                formatter,
                "candidate count differs from audit coverage: declared={declared}, actual={actual}"
            ),
            Self::DuplicateSeriesId(series_id) => {
                write!(formatter, "candidate series_id is duplicated: {series_id}")
            }
            Self::InvalidCandidate { series_id, detail } => {
                write!(
                    formatter,
                    "candidate is invalid: series_id={series_id}, detail={detail}"
                )
            }
            Self::CountOverflow(field) => write!(formatter, "count exceeds u32: {field}"),
        }
    }
}

impl Error for IdentityCoverageError {}

impl From<IdentityRegistryError> for IdentityCoverageError {
    fn from(value: IdentityRegistryError) -> Self {
        Self::Registry(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ReviewQueueKey {
    identity_kind: IdentityKind,
    source_key: String,
    status: IdentityCoverageStatus,
    ambiguous_canonical_ids: Vec<String>,
}

struct ReviewQueueAccumulator {
    first_seen_utc: DateTime<Utc>,
    last_seen_utc: DateTime<Utc>,
    occurrence_count: u32,
    affected_series_ids: BTreeSet<String>,
}

/// 对每条候选在其 Scheduled Start 时刻执行显式时间化解析；Missing/Ambiguous 保留为阻塞结果。
pub fn build_identity_coverage_audit(
    input: IdentityCoverageBuildInput,
) -> Result<IdentityCoverageAudit, IdentityCoverageError> {
    let IdentityCoverageBuildInput {
        candidate_audit,
        registry,
    } = input;
    if candidate_audit.audit_version != 1 {
        return Err(IdentityCoverageError::UnsupportedCandidateAuditVersion(
            candidate_audit.audit_version,
        ));
    }
    if candidate_audit.candidates.is_empty() {
        return Err(IdentityCoverageError::EmptyCandidates);
    }
    if candidate_audit.coverage.candidate_count as usize != candidate_audit.candidates.len() {
        return Err(IdentityCoverageError::CandidateCountMismatch {
            declared: candidate_audit.coverage.candidate_count,
            actual: candidate_audit.candidates.len(),
        });
    }
    let resolver = registry.validated_resolver()?;

    let mut seen_series_ids = BTreeSet::new();
    let mut team_source_keys = BTreeSet::new();
    let mut competition_source_keys = BTreeSet::new();
    let mut team_counts = IdentityResolutionCounts::default();
    let mut competition_counts = IdentityResolutionCounts::default();
    let mut fully_resolved_series = 0_u32;
    let mut series_resolutions = Vec::with_capacity(candidate_audit.candidates.len());
    let mut review_queue = BTreeMap::<ReviewQueueKey, ReviewQueueAccumulator>::new();

    for candidate in &candidate_audit.candidates {
        validate_candidate(candidate, &candidate_audit, &mut seen_series_ids)?;
        team_source_keys.extend(candidate.team_source_keys.iter().cloned());
        competition_source_keys.insert(candidate.competition_source_key.clone());

        let competition = to_source_resolution(
            candidate.competition_source_key.clone(),
            resolver.resolve_competition(
                DataSource::Leaguepedia,
                None,
                &candidate.competition_source_key,
                candidate.scheduled_start_utc,
            )?,
        );
        let team_0 = to_source_resolution(
            candidate.team_source_keys[0].clone(),
            resolver.resolve_team(
                DataSource::Leaguepedia,
                None,
                &candidate.team_source_keys[0],
                candidate.scheduled_start_utc,
            )?,
        );
        let team_1 = to_source_resolution(
            candidate.team_source_keys[1].clone(),
            resolver.resolve_team(
                DataSource::Leaguepedia,
                None,
                &candidate.team_source_keys[1],
                candidate.scheduled_start_utc,
            )?,
        );
        increment_counts(&mut competition_counts, competition.status)?;
        increment_counts(&mut team_counts, team_0.status)?;
        increment_counts(&mut team_counts, team_1.status)?;

        for (kind, resolution) in [
            (IdentityKind::Competition, &competition),
            (IdentityKind::Team, &team_0),
            (IdentityKind::Team, &team_1),
        ] {
            if resolution.status != IdentityCoverageStatus::Resolved {
                add_review_queue_item(
                    &mut review_queue,
                    kind,
                    resolution,
                    candidate.scheduled_start_utc,
                    &candidate.series_id,
                )?;
            }
        }

        let fully_resolved = [competition.status, team_0.status, team_1.status]
            .iter()
            .all(|status| *status == IdentityCoverageStatus::Resolved);
        if fully_resolved {
            fully_resolved_series = fully_resolved_series.checked_add(1).ok_or(
                IdentityCoverageError::CountOverflow("fully_resolved_series"),
            )?;
        }
        series_resolutions.push(SeriesIdentityResolution {
            series_id: candidate.series_id.clone(),
            scheduled_start_utc: candidate.scheduled_start_utc,
            competition,
            teams: [team_0, team_1],
            fully_resolved,
        });
    }

    series_resolutions.sort_by(|left, right| {
        left.scheduled_start_utc
            .cmp(&right.scheduled_start_utc)
            .then_with(|| left.series_id.cmp(&right.series_id))
    });
    let review_queue = review_queue
        .into_iter()
        .map(|(key, value)| IdentityReviewQueueItem {
            identity_kind: key.identity_kind,
            source_key: key.source_key,
            status: key.status,
            ambiguous_canonical_ids: key.ambiguous_canonical_ids,
            first_seen_utc: value.first_seen_utc,
            last_seen_utc: value.last_seen_utc,
            occurrence_count: value.occurrence_count,
            affected_series_ids: value.affected_series_ids.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    let candidate_count = to_u32(candidate_audit.candidates.len(), "candidate_count")?;
    let blocked_series = candidate_count
        .checked_sub(fully_resolved_series)
        .expect("fully resolved count cannot exceed candidates");

    Ok(IdentityCoverageAudit {
        audit_version: 1,
        source_candidate_audit_version: candidate_audit.audit_version,
        summary: IdentityCoverageSummary {
            candidate_count,
            fully_resolved_series,
            blocked_series,
            distinct_team_source_keys: to_u32(team_source_keys.len(), "distinct_team_source_keys")?,
            distinct_competition_source_keys: to_u32(
                competition_source_keys.len(),
                "distinct_competition_source_keys",
            )?,
            team_occurrences: team_counts,
            competition_occurrences: competition_counts,
            review_queue_items: to_u32(review_queue.len(), "review_queue_items")?,
        },
        series_resolutions,
        review_queue,
    })
}

fn validate_candidate(
    candidate: &HistoricalSeriesCandidate,
    audit: &HistoricalCandidateAudit,
    seen_series_ids: &mut BTreeSet<String>,
) -> Result<(), IdentityCoverageError> {
    let invalid = |detail| IdentityCoverageError::InvalidCandidate {
        series_id: candidate.series_id.clone(),
        detail,
    };
    if candidate.series_id.trim().is_empty()
        || candidate.competition_source_key.trim().is_empty()
        || candidate
            .team_source_keys
            .iter()
            .any(|source_key| source_key.trim().is_empty())
    {
        return Err(invalid("required source identity field is empty"));
    }
    if !seen_series_ids.insert(candidate.series_id.clone()) {
        return Err(IdentityCoverageError::DuplicateSeriesId(
            candidate.series_id.clone(),
        ));
    }
    if candidate.team_source_keys[0] == candidate.team_source_keys[1] {
        return Err(invalid("team source keys must be distinct"));
    }
    if !candidate
        .team_source_keys
        .contains(&candidate.winner_team_source_key)
    {
        return Err(invalid("winner source key is not one of the teams"));
    }
    if candidate.scheduled_start_utc < audit.scope.start_utc
        || candidate.scheduled_start_utc >= audit.scope.end_utc
    {
        return Err(invalid("scheduled start is outside candidate audit scope"));
    }
    if candidate.completed_at_utc <= candidate.scheduled_start_utc {
        return Err(invalid("completion must be after scheduled start"));
    }
    Ok(())
}

fn to_source_resolution(
    source_key: String,
    resolution: IdentityResolution,
) -> SourceIdentityResolution {
    match resolution {
        IdentityResolution::Resolved(canonical_id) => SourceIdentityResolution {
            source_key,
            status: IdentityCoverageStatus::Resolved,
            canonical_id: Some(canonical_id),
            ambiguous_canonical_ids: Vec::new(),
        },
        IdentityResolution::Missing => SourceIdentityResolution {
            source_key,
            status: IdentityCoverageStatus::Missing,
            canonical_id: None,
            ambiguous_canonical_ids: Vec::new(),
        },
        IdentityResolution::Ambiguous(canonical_ids) => SourceIdentityResolution {
            source_key,
            status: IdentityCoverageStatus::Ambiguous,
            canonical_id: None,
            ambiguous_canonical_ids: canonical_ids,
        },
    }
}

fn increment_counts(
    counts: &mut IdentityResolutionCounts,
    status: IdentityCoverageStatus,
) -> Result<(), IdentityCoverageError> {
    let (field, target) = match status {
        IdentityCoverageStatus::Resolved => ("resolution.resolved", &mut counts.resolved),
        IdentityCoverageStatus::Missing => ("resolution.missing", &mut counts.missing),
        IdentityCoverageStatus::Ambiguous => ("resolution.ambiguous", &mut counts.ambiguous),
    };
    *target = target
        .checked_add(1)
        .ok_or(IdentityCoverageError::CountOverflow(field))?;
    Ok(())
}

fn add_review_queue_item(
    queue: &mut BTreeMap<ReviewQueueKey, ReviewQueueAccumulator>,
    identity_kind: IdentityKind,
    resolution: &SourceIdentityResolution,
    observed_at_utc: DateTime<Utc>,
    series_id: &str,
) -> Result<(), IdentityCoverageError> {
    let key = ReviewQueueKey {
        identity_kind,
        source_key: resolution.source_key.clone(),
        status: resolution.status,
        ambiguous_canonical_ids: resolution.ambiguous_canonical_ids.clone(),
    };
    let item = queue.entry(key).or_insert_with(|| ReviewQueueAccumulator {
        first_seen_utc: observed_at_utc,
        last_seen_utc: observed_at_utc,
        occurrence_count: 0,
        affected_series_ids: BTreeSet::new(),
    });
    item.first_seen_utc = item.first_seen_utc.min(observed_at_utc);
    item.last_seen_utc = item.last_seen_utc.max(observed_at_utc);
    item.occurrence_count =
        item.occurrence_count
            .checked_add(1)
            .ok_or(IdentityCoverageError::CountOverflow(
                "review_queue.occurrence_count",
            ))?;
    item.affected_series_ids.insert(series_id.to_owned());
    Ok(())
}

fn to_u32(value: usize, field: &'static str) -> Result<u32, IdentityCoverageError> {
    u32::try_from(value).map_err(|_| IdentityCoverageError::CountOverflow(field))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Duration;

    use super::*;
    use crate::{
        historical_candidates::{
            HistoricalCandidateAudit, HistoricalCandidateCoverage, HistoricalCandidateScope,
        },
        identity_registry::{
            CanonicalCompetition, CanonicalTeam, CompetitionIdentityPeriod, TeamIdentityPeriod,
        },
    };

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp must be valid")
            .with_timezone(&Utc)
    }

    fn candidate(
        series_id: &str,
        at: &str,
        team_1: &str,
        team_2: &str,
    ) -> HistoricalSeriesCandidate {
        let scheduled_start_utc = utc(at);
        HistoricalSeriesCandidate {
            series_id: series_id.to_owned(),
            leaguepedia_match_id: format!("match-{series_id}"),
            competition_source_key: "League/2025 Season/Spring".to_owned(),
            team_source_keys: [team_1.to_owned(), team_2.to_owned()],
            scheduled_start_utc,
            completed_at_utc: scheduled_start_utc + Duration::hours(2),
            best_of: 3,
            patch: "25.1".to_owned(),
            scores: [2, 0],
            winner_team_source_key: team_1.to_owned(),
            result_evidence_id: format!("leaguepedia:match-{series_id}"),
            series_source_row_count: 1,
            game_source_row_count: 2,
        }
    }

    fn audit(candidates: Vec<HistoricalSeriesCandidate>) -> HistoricalCandidateAudit {
        HistoricalCandidateAudit {
            audit_version: 1,
            scope: HistoricalCandidateScope {
                start_utc: utc("2025-01-01T00:00:00Z"),
                end_utc: utc("2025-07-01T00:00:00Z"),
            },
            coverage: HistoricalCandidateCoverage {
                raw_series_rows: candidates.len() as u32,
                raw_game_rows: candidates.len() as u32 * 2,
                distinct_match_ids: candidates.len() as u32,
                candidate_count: candidates.len() as u32,
                rejected_count: 0,
                distinct_utc_dates: 1,
                years: vec![2025],
                patches: BTreeMap::from([("25.1".to_owned(), candidates.len() as u32)]),
                best_of: BTreeMap::from([("bo3".to_owned(), candidates.len() as u32)]),
                rejection_counts: BTreeMap::new(),
            },
            source_region_coverage: None,
            recovery_disjointness: None,
            candidates,
            rejections: Vec::new(),
        }
    }

    fn registry(valid_from: &str, valid_until: &str) -> IdentityRegistry {
        let from = utc(valid_from);
        let until = Some(utc(valid_until));
        IdentityRegistry {
            teams: vec![
                CanonicalTeam {
                    canonical_team_id: "lol-team:alpha".to_owned(),
                    canonical_name: "Alpha".to_owned(),
                },
                CanonicalTeam {
                    canonical_team_id: "lol-team:beta".to_owned(),
                    canonical_name: "Beta".to_owned(),
                },
            ],
            team_identity_periods: vec![
                TeamIdentityPeriod::new(
                    "lol-team:alpha",
                    DataSource::Leaguepedia,
                    None,
                    "Alpha Esports",
                    from,
                    until,
                    "review:1",
                ),
                TeamIdentityPeriod::new(
                    "lol-team:beta",
                    DataSource::Leaguepedia,
                    None,
                    "Beta Gaming",
                    from,
                    until,
                    "review:2",
                ),
            ],
            competitions: vec![CanonicalCompetition {
                canonical_competition_id: "lol-competition:league".to_owned(),
                canonical_name: "League".to_owned(),
            }],
            competition_identity_periods: vec![CompetitionIdentityPeriod::new(
                "lol-competition:league",
                DataSource::Leaguepedia,
                None,
                "League/2025 Season/Spring",
                from,
                until,
                "review:3",
            )],
        }
    }

    #[test]
    fn resolves_only_when_explicit_period_is_active() {
        let input = IdentityCoverageBuildInput {
            candidate_audit: audit(vec![candidate(
                "series-1",
                "2025-02-01T00:00:00Z",
                "Alpha Esports",
                "Beta Gaming",
            )]),
            registry: registry("2025-01-01T00:00:00Z", "2025-07-01T00:00:00Z"),
        };
        let result = build_identity_coverage_audit(input).expect("coverage must build");
        assert_eq!(result.summary.fully_resolved_series, 1);
        assert_eq!(result.summary.review_queue_items, 0);
        assert!(result.series_resolutions[0].fully_resolved);
    }

    #[test]
    fn historical_candidate_does_not_use_future_identity_evidence() {
        let input = IdentityCoverageBuildInput {
            candidate_audit: audit(vec![candidate(
                "series-1",
                "2025-02-01T00:00:00Z",
                "Alpha Esports",
                "Beta Gaming",
            )]),
            registry: registry("2026-08-01T00:00:00Z", "2026-08-02T00:00:00Z"),
        };
        let result = build_identity_coverage_audit(input).expect("missing is auditable");
        assert_eq!(result.summary.fully_resolved_series, 0);
        assert_eq!(result.summary.team_occurrences.missing, 2);
        assert_eq!(result.summary.competition_occurrences.missing, 1);
        assert_eq!(result.summary.review_queue_items, 3);
    }

    #[test]
    fn aggregates_repeated_missing_source_key_into_one_review_item() {
        let input = IdentityCoverageBuildInput {
            candidate_audit: audit(vec![
                candidate(
                    "series-1",
                    "2025-02-01T00:00:00Z",
                    "Unknown Team",
                    "Beta Gaming",
                ),
                candidate(
                    "series-2",
                    "2025-03-01T00:00:00Z",
                    "Unknown Team",
                    "Alpha Esports",
                ),
            ]),
            registry: registry("2025-01-01T00:00:00Z", "2025-07-01T00:00:00Z"),
        };
        let result = build_identity_coverage_audit(input).expect("coverage must build");
        let item = result
            .review_queue
            .iter()
            .find(|item| item.source_key == "Unknown Team")
            .expect("missing team queue item");
        assert_eq!(item.occurrence_count, 2);
        assert_eq!(item.affected_series_ids, ["series-1", "series-2"]);
        assert_eq!(item.first_seen_utc, utc("2025-02-01T00:00:00Z"));
        assert_eq!(item.last_seen_utc, utc("2025-03-01T00:00:00Z"));
    }

    #[test]
    fn ambiguous_identity_blocks_series_and_preserves_all_candidates() {
        let mut registry = registry("2025-01-01T00:00:00Z", "2025-07-01T00:00:00Z");
        registry.teams.push(CanonicalTeam {
            canonical_team_id: "lol-team:alpha-two".to_owned(),
            canonical_name: "Alpha Two".to_owned(),
        });
        registry.team_identity_periods.push(TeamIdentityPeriod::new(
            "lol-team:alpha-two",
            DataSource::Leaguepedia,
            None,
            "Alpha Esports",
            utc("2025-01-01T00:00:00Z"),
            Some(utc("2025-07-01T00:00:00Z")),
            "review:4",
        ));
        let result = build_identity_coverage_audit(IdentityCoverageBuildInput {
            candidate_audit: audit(vec![candidate(
                "series-1",
                "2025-02-01T00:00:00Z",
                "Alpha Esports",
                "Beta Gaming",
            )]),
            registry,
        })
        .expect("ambiguity is auditable");
        assert_eq!(result.summary.team_occurrences.ambiguous, 1);
        assert_eq!(
            result.review_queue[0].status,
            IdentityCoverageStatus::Ambiguous
        );
        assert_eq!(
            result.review_queue[0].ambiguous_canonical_ids,
            ["lol-team:alpha", "lol-team:alpha-two"]
        );
    }

    #[test]
    fn duplicate_series_id_fails_closed() {
        let duplicate = candidate(
            "series-1",
            "2025-02-01T00:00:00Z",
            "Alpha Esports",
            "Beta Gaming",
        );
        let error = build_identity_coverage_audit(IdentityCoverageBuildInput {
            candidate_audit: audit(vec![duplicate.clone(), duplicate]),
            registry: registry("2025-01-01T00:00:00Z", "2025-07-01T00:00:00Z"),
        })
        .expect_err("duplicate must fail closed");
        assert_eq!(
            error,
            IdentityCoverageError::DuplicateSeriesId("series-1".to_owned())
        );
    }
}
