use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    event_mapping::DataSource,
    historical_candidates::{HistoricalCandidateAudit, HistoricalSeriesCandidate},
    identity_coverage::{
        IdentityCoverageAudit, IdentityCoverageBuildInput, IdentityCoverageError,
        build_identity_coverage_audit,
    },
    identity_registry::{
        CanonicalCompetition, CanonicalTeam, CompetitionIdentityPeriod, IdentityRegistry,
        TeamIdentityPeriod,
    },
    series_result::{SeriesResult, SeriesResultCandidate, SeriesResultError, build_series_results},
};

/// Leaguepedia TeamRedirects 的显式 alias -> canonical page 关系。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RawTeamRedirectRow {
    #[serde(rename = "CanonicalPage", deserialize_with = "deserialize_source_text")]
    pub canonical_page: String,
    #[serde(rename = "AllName", deserialize_with = "deserialize_source_text")]
    pub all_name: String,
}

/// Leaguepedia Tournaments 的 tournament page -> league brand/region 关系。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RawTournamentIdentityRow {
    #[serde(rename = "OverviewPage", deserialize_with = "deserialize_source_text")]
    pub overview_page: String,
    #[serde(rename = "Name", deserialize_with = "deserialize_source_text")]
    pub name: String,
    #[serde(rename = "League", deserialize_with = "deserialize_source_text")]
    pub league: String,
    #[serde(rename = "Year", deserialize_with = "deserialize_optional_source_text")]
    pub year: Option<String>,
    #[serde(rename = "Region", deserialize_with = "deserialize_source_text")]
    pub region: String,
}

/// Cargo 会把纯数字名称编码成 JSON number；这里仅保留标量原值文本，不做名称规范化。
fn deserialize_source_text<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::String(value) => Ok(value),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Null => Ok(String::new()),
        Value::Array(_) | Value::Object(_) => {
            Err(serde::de::Error::custom("source text must be a scalar"))
        }
    }
}

fn deserialize_optional_source_text<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Null => Ok(None),
        Value::String(value) => Ok(Some(value)),
        Value::Number(value) => Ok(Some(value.to_string())),
        Value::Bool(value) => Ok(Some(value.to_string())),
        Value::Array(_) | Value::Object(_) => Err(serde::de::Error::custom(
            "optional source text must be a scalar",
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalIdentityBuildInput {
    pub candidate_audit: HistoricalCandidateAudit,
    pub team_redirect_rows: Vec<RawTeamRedirectRow>,
    pub tournament_rows: Vec<RawTournamentIdentityRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalTeamIdentityEvidence {
    pub source_key: String,
    pub canonical_page: String,
    pub canonical_team_id: String,
    pub first_seen_utc: DateTime<Utc>,
    pub last_seen_utc: DateTime<Utc>,
    pub occurrence_count: u32,
    pub affected_series_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalCompetitionIdentityEvidence {
    pub source_key: String,
    pub tournament_name: String,
    pub league_brand: String,
    pub region: String,
    pub canonical_competition_id: String,
    pub first_seen_utc: DateTime<Utc>,
    pub last_seen_utc: DateTime<Utc>,
    pub occurrence_count: u32,
    pub affected_series_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalIdentitySummary {
    pub candidate_count: u32,
    pub source_team_key_count: u32,
    pub resolved_team_key_count: u32,
    pub unresolved_team_key_count: u32,
    pub ambiguous_team_key_count: u32,
    pub source_competition_key_count: u32,
    pub resolved_competition_key_count: u32,
    pub unresolved_competition_key_count: u32,
    pub ambiguous_competition_key_count: u32,
    pub fully_resolved_series: u32,
    pub blocked_series: u32,
    pub series_result_count: u32,
}

/// HIST-010 单一可重放输出：显式证据、时间化 registry、coverage 和纯 Series Result。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalIdentityAudit {
    pub audit_version: u16,
    pub source_candidate_audit_version: u16,
    pub summary: HistoricalIdentitySummary,
    pub team_evidence: Vec<HistoricalTeamIdentityEvidence>,
    pub competition_evidence: Vec<HistoricalCompetitionIdentityEvidence>,
    pub registry: IdentityRegistry,
    pub coverage: IdentityCoverageAudit,
    pub series_results: Vec<SeriesResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoricalIdentityError {
    EmptyCandidates,
    EmptyRawInput(&'static str),
    InvalidRawField {
        dataset: &'static str,
        field: &'static str,
    },
    InvalidTournamentYear {
        overview_page: String,
        year: Option<String>,
    },
    IdentityCoverage(IdentityCoverageError),
    SeriesResult(SeriesResultError),
    CountOverflow(&'static str),
}

impl fmt::Display for HistoricalIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCandidates => {
                formatter.write_str("historical identity input has no candidates")
            }
            Self::EmptyRawInput(dataset) => write!(
                formatter,
                "historical identity raw input is empty: {dataset}"
            ),
            Self::InvalidRawField { dataset, field } => write!(
                formatter,
                "historical identity raw field is empty: dataset={dataset}, field={field}"
            ),
            Self::InvalidTournamentYear {
                overview_page,
                year,
            } => write!(
                formatter,
                "target tournament has unexpected year: overview_page={overview_page}, year={year:?}"
            ),
            Self::IdentityCoverage(error) => write!(formatter, "identity coverage failed: {error}"),
            Self::SeriesResult(error) => write!(formatter, "series result build failed: {error}"),
            Self::CountOverflow(field) => write!(formatter, "count exceeds u32: {field}"),
        }
    }
}

impl Error for HistoricalIdentityError {}

impl From<IdentityCoverageError> for HistoricalIdentityError {
    fn from(value: IdentityCoverageError) -> Self {
        Self::IdentityCoverage(value)
    }
}

impl From<SeriesResultError> for HistoricalIdentityError {
    fn from(value: SeriesResultError) -> Self {
        Self::SeriesResult(value)
    }
}

#[derive(Debug, Clone)]
struct Occurrences {
    first_seen_utc: DateTime<Utc>,
    last_seen_utc: DateTime<Utc>,
    series_ids: BTreeSet<String>,
    events: BTreeSet<(DateTime<Utc>, String, String)>,
}

/// 只接受 Cargo 明确返回的 exact relation；缺失或一对多关系继续留在 coverage queue。
pub fn build_historical_identity_audit(
    input: HistoricalIdentityBuildInput,
) -> Result<HistoricalIdentityAudit, HistoricalIdentityError> {
    if input.candidate_audit.candidates.is_empty() {
        return Err(HistoricalIdentityError::EmptyCandidates);
    }
    if input.team_redirect_rows.is_empty() {
        return Err(HistoricalIdentityError::EmptyRawInput("TeamRedirects"));
    }
    if input.tournament_rows.is_empty() {
        return Err(HistoricalIdentityError::EmptyRawInput("Tournaments"));
    }

    let team_occurrences = collect_team_occurrences(&input.candidate_audit.candidates);
    let competition_occurrences =
        collect_competition_occurrences(&input.candidate_audit.candidates);
    let target_team_keys = team_occurrences.keys().cloned().collect::<BTreeSet<_>>();
    let target_competition_keys = competition_occurrences
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let team_relations = collect_team_relations(input.team_redirect_rows, &target_team_keys)?;
    let tournament_relations =
        collect_tournament_relations(input.tournament_rows, &target_competition_keys)?;

    let mut registry = IdentityRegistry::default();
    let mut team_evidence = Vec::new();
    let mut ambiguous_team_key_count = 0_u32;
    for (source_key, occurrences) in &team_occurrences {
        let pages = team_relations.get(source_key).cloned().unwrap_or_default();
        if pages.len() != 1 {
            if pages.len() > 1 {
                ambiguous_team_key_count =
                    checked_add(ambiguous_team_key_count, 1, "ambiguous_team_key_count")?;
                // 将全部显式候选写入同一观测时点，使 coverage 保留 Ambiguous 语义。
                for canonical_page in pages {
                    let canonical_team_id = canonical_id("lol-team:lp-", &canonical_page);
                    if !registry
                        .teams
                        .iter()
                        .any(|team| team.canonical_team_id == canonical_team_id)
                    {
                        registry.teams.push(CanonicalTeam {
                            canonical_team_id: canonical_team_id.clone(),
                            canonical_name: canonical_page.clone(),
                        });
                    }
                    for (observed_at, series_id, result_evidence_id) in &occurrences.events {
                        registry.team_identity_periods.push(TeamIdentityPeriod::new(
                            canonical_team_id.clone(),
                            DataSource::Leaguepedia,
                            None,
                            source_key.clone(),
                            *observed_at,
                            Some(*observed_at + Duration::seconds(1)),
                            format!(
                                "HIST-010:TeamRedirects:{source_key}->{canonical_page}|{series_id}|{result_evidence_id}"
                            ),
                        ));
                    }
                }
            }
            continue;
        }
        let canonical_page = pages
            .iter()
            .next()
            .expect("single team page must exist")
            .clone();
        let canonical_team_id = canonical_id("lol-team:lp-", &canonical_page);
        if !registry
            .teams
            .iter()
            .any(|team| team.canonical_team_id == canonical_team_id)
        {
            registry.teams.push(CanonicalTeam {
                canonical_team_id: canonical_team_id.clone(),
                canonical_name: canonical_page.clone(),
            });
        }
        // 每个 MatchSchedule 观测只授权对应事件时点，禁止在无证据区间内插值。
        for (observed_at, series_id, result_evidence_id) in &occurrences.events {
            registry.team_identity_periods.push(TeamIdentityPeriod::new(
                canonical_team_id.clone(),
                DataSource::Leaguepedia,
                None,
                source_key.clone(),
                *observed_at,
                Some(*observed_at + Duration::seconds(1)),
                format!(
                    "HIST-010:TeamRedirects:{source_key}->{canonical_page}|{series_id}|{result_evidence_id}"
                ),
            ));
        }
        team_evidence.push(HistoricalTeamIdentityEvidence {
            source_key: source_key.clone(),
            canonical_page,
            canonical_team_id,
            first_seen_utc: occurrences.first_seen_utc,
            last_seen_utc: occurrences.last_seen_utc,
            occurrence_count: to_u32(occurrences.events.len(), "team occurrence_count")?,
            affected_series_ids: occurrences.series_ids.iter().cloned().collect(),
        });
    }

    let mut competition_evidence = Vec::new();
    let mut competition_regions = BTreeMap::new();
    let mut ambiguous_competition_key_count = 0_u32;
    for (source_key, occurrences) in &competition_occurrences {
        let relations = tournament_relations
            .get(source_key)
            .cloned()
            .unwrap_or_default();
        if relations.len() != 1 {
            if relations.len() > 1 {
                ambiguous_competition_key_count = checked_add(
                    ambiguous_competition_key_count,
                    1,
                    "ambiguous_competition_key_count",
                )?;
                for (_, league_brand, _) in relations {
                    let canonical_competition_id =
                        canonical_id("lol-competition:lp-", &league_brand);
                    if !registry.competitions.iter().any(|competition| {
                        competition.canonical_competition_id == canonical_competition_id
                    }) {
                        registry.competitions.push(CanonicalCompetition {
                            canonical_competition_id: canonical_competition_id.clone(),
                            canonical_name: league_brand.clone(),
                        });
                    }
                    for (observed_at, series_id, result_evidence_id) in &occurrences.events {
                        registry.competition_identity_periods.push(
                            CompetitionIdentityPeriod::new(
                                canonical_competition_id.clone(),
                                DataSource::Leaguepedia,
                                Some(source_key.clone()),
                                source_key.clone(),
                                *observed_at,
                                Some(*observed_at + Duration::seconds(1)),
                                format!(
                                    "HIST-010:Tournaments:{source_key}->{league_brand}|{series_id}|{result_evidence_id}"
                                ),
                            ),
                        );
                    }
                }
            }
            continue;
        }
        let (tournament_name, league_brand, region) = relations
            .iter()
            .next()
            .expect("single tournament relation must exist")
            .clone();
        let canonical_competition_id = canonical_id("lol-competition:lp-", &league_brand);
        if !registry
            .competitions
            .iter()
            .any(|competition| competition.canonical_competition_id == canonical_competition_id)
        {
            registry.competitions.push(CanonicalCompetition {
                canonical_competition_id: canonical_competition_id.clone(),
                canonical_name: league_brand.clone(),
            });
        }
        for (observed_at, series_id, result_evidence_id) in &occurrences.events {
            registry
                .competition_identity_periods
                .push(CompetitionIdentityPeriod::new(
                    canonical_competition_id.clone(),
                    DataSource::Leaguepedia,
                    Some(source_key.clone()),
                    source_key.clone(),
                    *observed_at,
                    Some(*observed_at + Duration::seconds(1)),
                    format!(
                        "HIST-010:Tournaments:{source_key}->{league_brand}|{series_id}|{result_evidence_id}"
                    ),
                ));
        }
        competition_regions.insert(source_key.clone(), region.clone());
        competition_evidence.push(HistoricalCompetitionIdentityEvidence {
            source_key: source_key.clone(),
            tournament_name,
            league_brand,
            region,
            canonical_competition_id,
            first_seen_utc: occurrences.first_seen_utc,
            last_seen_utc: occurrences.last_seen_utc,
            occurrence_count: to_u32(occurrences.events.len(), "competition occurrence_count")?,
            affected_series_ids: occurrences.series_ids.iter().cloned().collect(),
        });
    }
    registry
        .teams
        .sort_by(|left, right| left.canonical_team_id.cmp(&right.canonical_team_id));
    registry.team_identity_periods.sort_by(|left, right| {
        left.valid_from_utc
            .cmp(&right.valid_from_utc)
            .then_with(|| left.observed_name.cmp(&right.observed_name))
    });
    registry.competitions.sort_by(|left, right| {
        left.canonical_competition_id
            .cmp(&right.canonical_competition_id)
    });
    registry
        .competition_identity_periods
        .sort_by(|left, right| {
            left.valid_from_utc
                .cmp(&right.valid_from_utc)
                .then_with(|| left.observed_name.cmp(&right.observed_name))
        });
    registry.validate().map_err(IdentityCoverageError::from)?;

    let coverage = build_identity_coverage_audit(IdentityCoverageBuildInput {
        candidate_audit: input.candidate_audit.clone(),
        registry: registry.clone(),
    })?;
    let series_results = build_resolved_series_results(
        &input.candidate_audit.candidates,
        &coverage,
        &competition_regions,
    )?;
    team_evidence.sort_by(|left, right| left.source_key.cmp(&right.source_key));
    competition_evidence.sort_by(|left, right| left.source_key.cmp(&right.source_key));

    let resolved_team_key_count = to_u32(team_evidence.len(), "resolved_team_key_count")?;
    let resolved_competition_key_count =
        to_u32(competition_evidence.len(), "resolved_competition_key_count")?;
    let source_team_key_count = to_u32(team_occurrences.len(), "source_team_key_count")?;
    let source_competition_key_count = to_u32(
        competition_occurrences.len(),
        "source_competition_key_count",
    )?;
    Ok(HistoricalIdentityAudit {
        audit_version: 1,
        source_candidate_audit_version: input.candidate_audit.audit_version,
        summary: HistoricalIdentitySummary {
            candidate_count: coverage.summary.candidate_count,
            source_team_key_count,
            resolved_team_key_count,
            unresolved_team_key_count: source_team_key_count - resolved_team_key_count,
            ambiguous_team_key_count,
            source_competition_key_count,
            resolved_competition_key_count,
            unresolved_competition_key_count: source_competition_key_count
                - resolved_competition_key_count,
            ambiguous_competition_key_count,
            fully_resolved_series: coverage.summary.fully_resolved_series,
            blocked_series: coverage.summary.blocked_series,
            series_result_count: to_u32(series_results.len(), "series_result_count")?,
        },
        team_evidence,
        competition_evidence,
        registry,
        coverage,
        series_results,
    })
}

fn collect_team_occurrences(
    candidates: &[HistoricalSeriesCandidate],
) -> BTreeMap<String, Occurrences> {
    let mut output = BTreeMap::new();
    for candidate in candidates {
        for source_key in &candidate.team_source_keys {
            add_occurrence(&mut output, source_key, candidate);
        }
    }
    output
}

fn collect_competition_occurrences(
    candidates: &[HistoricalSeriesCandidate],
) -> BTreeMap<String, Occurrences> {
    let mut output = BTreeMap::new();
    for candidate in candidates {
        add_occurrence(&mut output, &candidate.competition_source_key, candidate);
    }
    output
}

fn add_occurrence(
    output: &mut BTreeMap<String, Occurrences>,
    source_key: &str,
    candidate: &HistoricalSeriesCandidate,
) {
    let item = output
        .entry(source_key.to_owned())
        .or_insert_with(|| Occurrences {
            first_seen_utc: candidate.scheduled_start_utc,
            last_seen_utc: candidate.scheduled_start_utc,
            series_ids: BTreeSet::new(),
            events: BTreeSet::new(),
        });
    item.first_seen_utc = item.first_seen_utc.min(candidate.scheduled_start_utc);
    item.last_seen_utc = item.last_seen_utc.max(candidate.scheduled_start_utc);
    item.series_ids.insert(candidate.series_id.clone());
    item.events.insert((
        candidate.scheduled_start_utc,
        candidate.series_id.clone(),
        candidate.result_evidence_id.clone(),
    ));
}

fn collect_team_relations(
    rows: Vec<RawTeamRedirectRow>,
    target_keys: &BTreeSet<String>,
) -> Result<BTreeMap<String, BTreeSet<String>>, HistoricalIdentityError> {
    let mut output = BTreeMap::<String, BTreeSet<String>>::new();
    for row in rows {
        if row.canonical_page.trim().is_empty() {
            return Err(HistoricalIdentityError::InvalidRawField {
                dataset: "TeamRedirects",
                field: "CanonicalPage",
            });
        }
        if row.all_name.trim().is_empty() {
            return Err(HistoricalIdentityError::InvalidRawField {
                dataset: "TeamRedirects",
                field: "AllName",
            });
        }
        if target_keys.contains(&row.all_name) {
            output
                .entry(row.all_name)
                .or_default()
                .insert(row.canonical_page);
        }
    }
    Ok(output)
}

type TournamentRelation = (String, String, String);

fn collect_tournament_relations(
    rows: Vec<RawTournamentIdentityRow>,
    target_keys: &BTreeSet<String>,
) -> Result<BTreeMap<String, BTreeSet<TournamentRelation>>, HistoricalIdentityError> {
    let mut output = BTreeMap::<String, BTreeSet<TournamentRelation>>::new();
    for row in rows {
        if row.overview_page.trim().is_empty() {
            return Err(HistoricalIdentityError::InvalidRawField {
                dataset: "Tournaments",
                field: "OverviewPage",
            });
        }
        if !target_keys.contains(&row.overview_page) {
            continue;
        }
        for (field, value) in [
            ("Name", row.name.as_str()),
            ("League", row.league.as_str()),
            ("Region", row.region.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(HistoricalIdentityError::InvalidRawField {
                    dataset: "Tournaments",
                    field,
                });
            }
        }
        if row.year.as_deref() != Some("2025") {
            return Err(HistoricalIdentityError::InvalidTournamentYear {
                overview_page: row.overview_page,
                year: row.year,
            });
        }
        output
            .entry(row.overview_page)
            .or_default()
            .insert((row.name, row.league, row.region));
    }
    Ok(output)
}

fn build_resolved_series_results(
    candidates: &[HistoricalSeriesCandidate],
    coverage: &IdentityCoverageAudit,
    competition_regions: &BTreeMap<String, String>,
) -> Result<Vec<SeriesResult>, HistoricalIdentityError> {
    let candidate_by_id = candidates
        .iter()
        .map(|candidate| (candidate.series_id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut result_candidates = Vec::new();
    for resolution in &coverage.series_resolutions {
        if !resolution.fully_resolved {
            continue;
        }
        let candidate = candidate_by_id
            .get(resolution.series_id.as_str())
            .expect("coverage series must come from candidate audit");
        let team_ids = [
            resolution.teams[0]
                .canonical_id
                .clone()
                .expect("resolved team must have canonical id"),
            resolution.teams[1]
                .canonical_id
                .clone()
                .expect("resolved team must have canonical id"),
        ];
        let winner_index = candidate
            .team_source_keys
            .iter()
            .position(|team| team == &candidate.winner_team_source_key)
            .expect("candidate winner must be one of its teams");
        result_candidates.push(SeriesResultCandidate {
            series_id: candidate.series_id.clone(),
            competition_id: resolution
                .competition
                .canonical_id
                .clone()
                .expect("resolved competition must have canonical id"),
            region: competition_regions
                .get(&candidate.competition_source_key)
                .expect("resolved competition must have region")
                .clone(),
            patch: candidate.patch.clone(),
            scheduled_start_utc: candidate.scheduled_start_utc,
            best_of: candidate.best_of,
            team_ids: team_ids.clone(),
            team_names: candidate.team_source_keys.clone(),
            scores: candidate.scores,
            winner_team_id: team_ids[winner_index].clone(),
            mapping_evidence_id: format!("HIST-010:{}", candidate.series_id),
            result_evidence_id: candidate.result_evidence_id.clone(),
        });
    }
    Ok(build_series_results(result_candidates)?)
}

fn canonical_id(prefix: &str, source_identity: &str) -> String {
    let digest = Sha256::digest(format!("leaguepedia\0{source_identity}").as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{prefix}{hex}")
}

fn checked_add(
    value: u32,
    increment: u32,
    field: &'static str,
) -> Result<u32, HistoricalIdentityError> {
    value
        .checked_add(increment)
        .ok_or(HistoricalIdentityError::CountOverflow(field))
}

fn to_u32(value: usize, field: &'static str) -> Result<u32, HistoricalIdentityError> {
    u32::try_from(value).map_err(|_| HistoricalIdentityError::CountOverflow(field))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::historical_candidates::{HistoricalCandidateCoverage, HistoricalCandidateScope};

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp must be valid")
            .with_timezone(&Utc)
    }

    fn candidate(series_id: &str, team_1: &str, team_2: &str) -> HistoricalSeriesCandidate {
        HistoricalSeriesCandidate {
            series_id: series_id.to_owned(),
            leaguepedia_match_id: format!("match-{series_id}"),
            competition_source_key: "League/2025 Season/Spring".to_owned(),
            team_source_keys: [team_1.to_owned(), team_2.to_owned()],
            scheduled_start_utc: utc("2025-02-01T00:00:00Z"),
            completed_at_utc: utc("2025-02-01T02:00:00Z"),
            best_of: 3,
            patch: "25.1".to_owned(),
            scores: [2, 0],
            winner_team_source_key: team_1.to_owned(),
            result_evidence_id: format!("leaguepedia:match-{series_id}"),
            series_source_row_count: 1,
            game_source_row_count: 2,
        }
    }

    fn input(team_rows: Vec<RawTeamRedirectRow>) -> HistoricalIdentityBuildInput {
        let candidates = vec![candidate("series-1", "Old Alpha", "Beta")];
        HistoricalIdentityBuildInput {
            candidate_audit: HistoricalCandidateAudit {
                audit_version: 1,
                scope: HistoricalCandidateScope {
                    start_utc: utc("2025-01-01T00:00:00Z"),
                    end_utc: utc("2025-07-01T00:00:00Z"),
                },
                coverage: HistoricalCandidateCoverage {
                    raw_series_rows: 1,
                    raw_game_rows: 2,
                    distinct_match_ids: 1,
                    candidate_count: 1,
                    rejected_count: 0,
                    distinct_utc_dates: 1,
                    years: vec![2025],
                    patches: BTreeMap::from([("25.1".to_owned(), 1)]),
                    best_of: BTreeMap::from([("bo3".to_owned(), 1)]),
                    rejection_counts: BTreeMap::new(),
                },
                source_region_coverage: None,
                recovery_disjointness: None,
                candidates,
                rejections: Vec::new(),
            },
            team_redirect_rows: team_rows,
            tournament_rows: vec![RawTournamentIdentityRow {
                overview_page: "League/2025 Season/Spring".to_owned(),
                name: "League 2025 Spring".to_owned(),
                league: "League Brand".to_owned(),
                year: Some("2025".to_owned()),
                region: "Europe".to_owned(),
            }],
        }
    }

    fn redirect(alias: &str, page: &str) -> RawTeamRedirectRow {
        RawTeamRedirectRow {
            canonical_page: page.to_owned(),
            all_name: alias.to_owned(),
        }
    }

    #[test]
    fn cargo_numeric_alias_is_preserved_as_source_text() {
        let row: RawTeamRedirectRow =
            serde_json::from_str(r#"{"CanonicalPage":"One Hundred","AllName":100}"#)
                .expect("numeric Cargo alias must deserialize");
        assert_eq!(row.all_name, "100");
    }

    #[test]
    fn explicit_redirect_and_tournament_relation_build_series_result() {
        let audit = build_historical_identity_audit(input(vec![
            redirect("Old Alpha", "Alpha Esports"),
            redirect("Beta", "Beta"),
        ]))
        .expect("explicit relations must resolve");
        assert_eq!(audit.summary.fully_resolved_series, 1);
        assert_eq!(audit.summary.series_result_count, 1);
        assert_eq!(audit.series_results[0].region, "Europe");
        assert_eq!(audit.series_results[0].team_names, ["Old Alpha", "Beta"]);
    }

    #[test]
    fn missing_redirect_remains_blocked_without_source_key_fallback() {
        let audit = build_historical_identity_audit(input(vec![redirect("Beta", "Beta")]))
            .expect("missing relation is an auditable result");
        assert_eq!(audit.summary.resolved_team_key_count, 1);
        assert_eq!(audit.summary.unresolved_team_key_count, 1);
        assert_eq!(audit.summary.fully_resolved_series, 0);
        assert!(audit.series_results.is_empty());
    }

    #[test]
    fn conflicting_redirects_are_ambiguous_and_fail_closed() {
        let audit = build_historical_identity_audit(input(vec![
            redirect("Old Alpha", "Alpha Esports"),
            redirect("Old Alpha", "Different Alpha"),
            redirect("Beta", "Beta"),
        ]))
        .expect("ambiguity is an auditable result");
        assert_eq!(audit.summary.ambiguous_team_key_count, 1);
        assert_eq!(audit.summary.fully_resolved_series, 0);
        let queue_item = audit
            .coverage
            .review_queue
            .iter()
            .find(|item| item.source_key == "Old Alpha")
            .expect("ambiguous source key must remain queued");
        assert_eq!(
            queue_item.status,
            crate::identity_coverage::IdentityCoverageStatus::Ambiguous
        );
    }

    #[test]
    fn conflicting_tournament_relation_blocks_competition() {
        let mut build_input = input(vec![
            redirect("Old Alpha", "Alpha Esports"),
            redirect("Beta", "Beta"),
        ]);
        build_input.tournament_rows.push(RawTournamentIdentityRow {
            overview_page: "League/2025 Season/Spring".to_owned(),
            name: "League 2025 Spring".to_owned(),
            league: "Other Brand".to_owned(),
            year: Some("2025".to_owned()),
            region: "Europe".to_owned(),
        });
        let audit = build_historical_identity_audit(build_input)
            .expect("conflicting relation is an auditable result");
        assert_eq!(audit.summary.ambiguous_competition_key_count, 1);
        assert_eq!(audit.summary.fully_resolved_series, 0);
    }

    #[test]
    fn non_2025_target_tournament_fails_closed() {
        let mut build_input = input(vec![
            redirect("Old Alpha", "Alpha Esports"),
            redirect("Beta", "Beta"),
        ]);
        build_input.tournament_rows[0].year = Some("2026".to_owned());
        assert!(matches!(
            build_historical_identity_audit(build_input),
            Err(HistoricalIdentityError::InvalidTournamentYear { .. })
        ));
    }

    #[test]
    fn output_is_deterministic_under_raw_row_reordering() {
        let forward = build_historical_identity_audit(input(vec![
            redirect("Old Alpha", "Alpha Esports"),
            redirect("Beta", "Beta"),
        ]))
        .expect("forward");
        let reverse = build_historical_identity_audit(input(vec![
            redirect("Beta", "Beta"),
            redirect("Old Alpha", "Alpha Esports"),
        ]))
        .expect("reverse");
        assert_eq!(forward, reverse);
    }
}
