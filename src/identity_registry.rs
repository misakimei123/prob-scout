use std::{collections::BTreeSet, error::Error, fmt};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::event_mapping::{DataSource, normalize_team_name};

/// 跨来源稳定的队伍身份；展示名变化不自动创建新身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalTeam {
    pub canonical_team_id: String,
    pub canonical_name: String,
}

/// 某来源队伍标识在半开时间区间 `[valid_from_utc, valid_until_utc)` 内的显式归属。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamIdentityPeriod {
    pub canonical_team_id: String,
    pub source: DataSource,
    pub source_team_id: Option<String>,
    pub observed_name: String,
    pub normalized_name: String,
    pub valid_from_utc: DateTime<Utc>,
    pub valid_until_utc: Option<DateTime<Utc>>,
    pub evidence_ref: String,
}

impl TeamIdentityPeriod {
    pub fn new(
        canonical_team_id: impl Into<String>,
        source: DataSource,
        source_team_id: Option<String>,
        observed_name: impl Into<String>,
        valid_from_utc: DateTime<Utc>,
        valid_until_utc: Option<DateTime<Utc>>,
        evidence_ref: impl Into<String>,
    ) -> Self {
        let observed_name = observed_name.into();
        Self {
            canonical_team_id: canonical_team_id.into(),
            source,
            source_team_id,
            normalized_name: normalize_team_name(&observed_name),
            observed_name,
            valid_from_utc,
            valid_until_utc,
            evidence_ref: evidence_ref.into(),
        }
    }

    fn is_active_at(&self, observed_at_utc: DateTime<Utc>) -> bool {
        self.valid_from_utc <= observed_at_utc
            && self
                .valid_until_utc
                .is_none_or(|valid_until| observed_at_utc < valid_until)
    }
}

/// 跨来源稳定的联赛或杯赛品牌身份；单场 Event 和赛季阶段不属于此身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalCompetition {
    pub canonical_competition_id: String,
    pub canonical_name: String,
}

/// 某来源赛事品牌标识在半开时间区间内的显式归属。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompetitionIdentityPeriod {
    pub canonical_competition_id: String,
    pub source: DataSource,
    pub source_competition_id: Option<String>,
    pub observed_name: String,
    pub normalized_name: String,
    pub valid_from_utc: DateTime<Utc>,
    pub valid_until_utc: Option<DateTime<Utc>>,
    pub evidence_ref: String,
}

impl CompetitionIdentityPeriod {
    pub fn new(
        canonical_competition_id: impl Into<String>,
        source: DataSource,
        source_competition_id: Option<String>,
        observed_name: impl Into<String>,
        valid_from_utc: DateTime<Utc>,
        valid_until_utc: Option<DateTime<Utc>>,
        evidence_ref: impl Into<String>,
    ) -> Self {
        let observed_name = observed_name.into();
        Self {
            canonical_competition_id: canonical_competition_id.into(),
            source,
            source_competition_id,
            normalized_name: normalize_team_name(&observed_name),
            observed_name,
            valid_from_utc,
            valid_until_utc,
            evidence_ref: evidence_ref.into(),
        }
    }

    fn is_active_at(&self, observed_at_utc: DateTime<Utc>) -> bool {
        self.valid_from_utc <= observed_at_utc
            && self
                .valid_until_utc
                .is_none_or(|valid_until| observed_at_utc < valid_until)
    }
}

/// 当前最小身份注册表；只保存显式证据，不执行相似度或包含关系猜测。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityRegistry {
    pub teams: Vec<CanonicalTeam>,
    pub team_identity_periods: Vec<TeamIdentityPeriod>,
    pub competitions: Vec<CanonicalCompetition>,
    pub competition_identity_periods: Vec<CompetitionIdentityPeriod>,
}

/// `Missing` 和 `Ambiguous` 都是正常的 fail-closed 结果，调用方不得擅自选第一个候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityResolution {
    Resolved(String),
    Missing,
    Ambiguous(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityRegistryError {
    EmptyField(&'static str),
    InvalidCanonicalId {
        field: &'static str,
        value: String,
    },
    DuplicateCanonicalId(String),
    UnknownCanonicalTeam(String),
    UnknownCanonicalCompetition(String),
    EmptySourceId(&'static str),
    InvalidNormalizedName {
        field: &'static str,
        observed_name: String,
    },
    InvalidValidityPeriod {
        evidence_ref: String,
    },
}

impl fmt::Display for IdentityRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(formatter, "required field is empty: {field}"),
            Self::InvalidCanonicalId { field, value } => {
                write!(formatter, "invalid canonical id in {field}: {value}")
            }
            Self::DuplicateCanonicalId(identity) => {
                write!(formatter, "canonical identity is duplicated: {identity}")
            }
            Self::UnknownCanonicalTeam(identity) => {
                write!(
                    formatter,
                    "team identity period references unknown team: {identity}"
                )
            }
            Self::UnknownCanonicalCompetition(identity) => write!(
                formatter,
                "competition identity period references unknown competition: {identity}"
            ),
            Self::EmptySourceId(field) => {
                write!(
                    formatter,
                    "optional source id cannot contain an empty value: {field}"
                )
            }
            Self::InvalidNormalizedName {
                field,
                observed_name,
            } => write!(
                formatter,
                "normalized identity name is invalid: field={field}, name={observed_name}"
            ),
            Self::InvalidValidityPeriod { evidence_ref } => write!(
                formatter,
                "identity valid_until must be after valid_from: evidence={evidence_ref}"
            ),
        }
    }
}

impl Error for IdentityRegistryError {}

impl IdentityRegistry {
    pub fn validate(&self) -> Result<(), IdentityRegistryError> {
        let mut canonical_ids = BTreeSet::new();
        for team in &self.teams {
            validate_canonical_id(
                "teams.canonical_team_id",
                &team.canonical_team_id,
                "lol-team:",
            )?;
            validate_non_empty("teams.canonical_name", &team.canonical_name)?;
            if !canonical_ids.insert(team.canonical_team_id.as_str()) {
                return Err(IdentityRegistryError::DuplicateCanonicalId(
                    team.canonical_team_id.clone(),
                ));
            }
        }

        let known_teams = self
            .teams
            .iter()
            .map(|team| team.canonical_team_id.as_str())
            .collect::<BTreeSet<_>>();
        for period in &self.team_identity_periods {
            if !known_teams.contains(period.canonical_team_id.as_str()) {
                return Err(IdentityRegistryError::UnknownCanonicalTeam(
                    period.canonical_team_id.clone(),
                ));
            }
            validate_identity_period(
                "team_identity_periods.source_team_id",
                period.source_team_id.as_deref(),
                "team_identity_periods.normalized_name",
                &period.observed_name,
                &period.normalized_name,
                period.valid_from_utc,
                period.valid_until_utc,
                &period.evidence_ref,
            )?;
        }

        for competition in &self.competitions {
            validate_canonical_id(
                "competitions.canonical_competition_id",
                &competition.canonical_competition_id,
                "lol-competition:",
            )?;
            validate_non_empty("competitions.canonical_name", &competition.canonical_name)?;
            if !canonical_ids.insert(competition.canonical_competition_id.as_str()) {
                return Err(IdentityRegistryError::DuplicateCanonicalId(
                    competition.canonical_competition_id.clone(),
                ));
            }
        }

        let known_competitions = self
            .competitions
            .iter()
            .map(|competition| competition.canonical_competition_id.as_str())
            .collect::<BTreeSet<_>>();
        for period in &self.competition_identity_periods {
            if !known_competitions.contains(period.canonical_competition_id.as_str()) {
                return Err(IdentityRegistryError::UnknownCanonicalCompetition(
                    period.canonical_competition_id.clone(),
                ));
            }
            validate_identity_period(
                "competition_identity_periods.source_competition_id",
                period.source_competition_id.as_deref(),
                "competition_identity_periods.normalized_name",
                &period.observed_name,
                &period.normalized_name,
                period.valid_from_utc,
                period.valid_until_utc,
                &period.evidence_ref,
            )?;
        }

        Ok(())
    }

    /// source ID 存在时只按 ID 解析；未知 ID 不降级为名称匹配。
    pub fn resolve_team(
        &self,
        source: DataSource,
        source_team_id: Option<&str>,
        observed_name: &str,
        observed_at_utc: DateTime<Utc>,
    ) -> Result<IdentityResolution, IdentityRegistryError> {
        self.validate()?;
        if source_team_id.is_some_and(|source_id| source_id.trim().is_empty()) {
            return Err(IdentityRegistryError::EmptySourceId(
                "resolve_team.source_team_id",
            ));
        }
        let normalized_name = normalize_team_name(observed_name);
        let candidates = self
            .team_identity_periods
            .iter()
            .filter(|period| period.source == source && period.is_active_at(observed_at_utc))
            .filter(|period| match source_team_id {
                Some(source_team_id) => period.source_team_id.as_deref() == Some(source_team_id),
                None => !normalized_name.is_empty() && period.normalized_name == normalized_name,
            })
            .map(|period| period.canonical_team_id.clone())
            .collect::<BTreeSet<_>>();
        Ok(to_resolution(candidates))
    }

    /// Competition 与 Event 分开解析；赛季/阶段名称只能经显式 period 指向品牌身份。
    pub fn resolve_competition(
        &self,
        source: DataSource,
        source_competition_id: Option<&str>,
        observed_name: &str,
        observed_at_utc: DateTime<Utc>,
    ) -> Result<IdentityResolution, IdentityRegistryError> {
        self.validate()?;
        if source_competition_id.is_some_and(|source_id| source_id.trim().is_empty()) {
            return Err(IdentityRegistryError::EmptySourceId(
                "resolve_competition.source_competition_id",
            ));
        }
        let normalized_name = normalize_team_name(observed_name);
        let candidates = self
            .competition_identity_periods
            .iter()
            .filter(|period| period.source == source && period.is_active_at(observed_at_utc))
            .filter(|period| match source_competition_id {
                Some(source_competition_id) => {
                    period.source_competition_id.as_deref() == Some(source_competition_id)
                }
                None => !normalized_name.is_empty() && period.normalized_name == normalized_name,
            })
            .map(|period| period.canonical_competition_id.clone())
            .collect::<BTreeSet<_>>();
        Ok(to_resolution(candidates))
    }
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), IdentityRegistryError> {
    if value.trim().is_empty() {
        return Err(IdentityRegistryError::EmptyField(field));
    }
    Ok(())
}

fn validate_canonical_id(
    field: &'static str,
    value: &str,
    prefix: &str,
) -> Result<(), IdentityRegistryError> {
    let suffix = value.strip_prefix(prefix).unwrap_or_default();
    if suffix.is_empty()
        || !suffix.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(IdentityRegistryError::InvalidCanonicalId {
            field,
            value: value.to_owned(),
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_identity_period(
    source_id_field: &'static str,
    source_id: Option<&str>,
    normalized_name_field: &'static str,
    observed_name: &str,
    normalized_name: &str,
    valid_from_utc: DateTime<Utc>,
    valid_until_utc: Option<DateTime<Utc>>,
    evidence_ref: &str,
) -> Result<(), IdentityRegistryError> {
    if source_id.is_some_and(|source_id| source_id.trim().is_empty()) {
        return Err(IdentityRegistryError::EmptySourceId(source_id_field));
    }
    validate_non_empty("identity_period.observed_name", observed_name)?;
    validate_non_empty("identity_period.evidence_ref", evidence_ref)?;
    if normalized_name.is_empty() || normalized_name != normalize_team_name(observed_name) {
        return Err(IdentityRegistryError::InvalidNormalizedName {
            field: normalized_name_field,
            observed_name: observed_name.to_owned(),
        });
    }
    if valid_until_utc.is_some_and(|valid_until| valid_until <= valid_from_utc) {
        return Err(IdentityRegistryError::InvalidValidityPeriod {
            evidence_ref: evidence_ref.to_owned(),
        });
    }
    Ok(())
}

fn to_resolution(candidates: BTreeSet<String>) -> IdentityResolution {
    match candidates.len() {
        0 => IdentityResolution::Missing,
        1 => IdentityResolution::Resolved(
            candidates
                .into_iter()
                .next()
                .expect("single identity candidate must exist"),
        ),
        _ => IdentityResolution::Ambiguous(candidates.into_iter().collect()),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Duration, Utc};
    use serde::Deserialize;

    use super::*;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("test timestamp must be valid")
            .with_timezone(&Utc)
    }

    fn sample_registry() -> IdentityRegistry {
        IdentityRegistry {
            teams: vec![
                CanonicalTeam {
                    canonical_team_id: "lol-team:renamed-example".to_owned(),
                    canonical_name: "New Team Name".to_owned(),
                },
                CanonicalTeam {
                    canonical_team_id: "lol-team:los".to_owned(),
                    canonical_name: "LOS".to_owned(),
                },
            ],
            team_identity_periods: vec![
                TeamIdentityPeriod::new(
                    "lol-team:renamed-example",
                    DataSource::Leaguepedia,
                    Some("Team:OLD_NAME".to_owned()),
                    "Old Team Name",
                    utc("2025-01-01T00:00:00Z"),
                    Some(utc("2026-01-01T00:00:00Z")),
                    "fixture:verified-rename:old",
                ),
                TeamIdentityPeriod::new(
                    "lol-team:renamed-example",
                    DataSource::Leaguepedia,
                    Some("Team:NEW_NAME".to_owned()),
                    "New Team Name",
                    utc("2026-01-01T00:00:00Z"),
                    None,
                    "fixture:verified-rename:new",
                ),
                TeamIdentityPeriod::new(
                    "lol-team:los",
                    DataSource::Leaguepedia,
                    None,
                    "LØS",
                    utc("2026-01-01T00:00:00Z"),
                    Some(utc("2027-01-01T00:00:00Z")),
                    "DATA-008:47",
                ),
                TeamIdentityPeriod::new(
                    "lol-team:los",
                    DataSource::PolymarketGamma,
                    None,
                    "LOS",
                    utc("2026-01-01T00:00:00Z"),
                    Some(utc("2027-01-01T00:00:00Z")),
                    "DATA-008:47",
                ),
            ],
            competitions: vec![CanonicalCompetition {
                canonical_competition_id: "lol-competition:lck".to_owned(),
                canonical_name: "LCK".to_owned(),
            }],
            competition_identity_periods: vec![
                CompetitionIdentityPeriod::new(
                    "lol-competition:lck",
                    DataSource::Leaguepedia,
                    Some("LCK/2026 Season/Rounds 3-4".to_owned()),
                    "LCK/2026 Season/Rounds 3-4",
                    utc("2026-01-01T00:00:00Z"),
                    Some(utc("2027-01-01T00:00:00Z")),
                    "DATA-008:42",
                ),
                CompetitionIdentityPeriod::new(
                    "lol-competition:lck",
                    DataSource::PolymarketGamma,
                    None,
                    "LCK Round 3-4 Legend Group",
                    utc("2026-01-01T00:00:00Z"),
                    Some(utc("2027-01-01T00:00:00Z")),
                    "DATA-008:42",
                ),
            ],
        }
    }

    #[test]
    fn resolves_time_bounded_rename_fixture_to_one_canonical_team() {
        let registry = sample_registry();

        assert_eq!(
            registry
                .resolve_team(
                    DataSource::Leaguepedia,
                    Some("Team:OLD_NAME"),
                    "ignored display name",
                    utc("2025-06-01T00:00:00Z"),
                )
                .unwrap(),
            IdentityResolution::Resolved("lol-team:renamed-example".to_owned())
        );
        assert_eq!(
            registry
                .resolve_team(
                    DataSource::Leaguepedia,
                    Some("Team:NEW_NAME"),
                    "ignored display name",
                    utc("2026-06-01T00:00:00Z"),
                )
                .unwrap(),
            IdentityResolution::Resolved("lol-team:renamed-example".to_owned())
        );
    }

    #[test]
    fn resolves_explicit_cross_source_name_variants() {
        let registry = sample_registry();
        let observed_at = utc("2026-08-10T00:00:00Z");

        assert_eq!(
            registry
                .resolve_team(DataSource::Leaguepedia, None, "LØS", observed_at)
                .unwrap(),
            IdentityResolution::Resolved("lol-team:los".to_owned())
        );
        assert_eq!(
            registry
                .resolve_team(DataSource::PolymarketGamma, None, "LOS", observed_at)
                .unwrap(),
            IdentityResolution::Resolved("lol-team:los".to_owned())
        );
    }

    #[test]
    fn does_not_fallback_to_name_when_source_id_is_unknown() {
        let registry = sample_registry();

        assert_eq!(
            registry
                .resolve_team(
                    DataSource::Leaguepedia,
                    Some("Team:UNKNOWN"),
                    "New Team Name",
                    utc("2026-08-10T00:00:00Z"),
                )
                .unwrap(),
            IdentityResolution::Missing
        );
    }

    #[test]
    fn reports_overlapping_name_reuse_as_ambiguous() {
        let mut registry = sample_registry();
        registry.teams.push(CanonicalTeam {
            canonical_team_id: "lol-team:other-los".to_owned(),
            canonical_name: "Other LOS".to_owned(),
        });
        registry.team_identity_periods.push(TeamIdentityPeriod::new(
            "lol-team:other-los",
            DataSource::Leaguepedia,
            None,
            "LØS",
            utc("2026-06-01T00:00:00Z"),
            Some(utc("2026-12-01T00:00:00Z")),
            "manual-review:ambiguous-name",
        ));

        assert_eq!(
            registry
                .resolve_team(
                    DataSource::Leaguepedia,
                    None,
                    "LØS",
                    utc("2026-08-10T00:00:00Z"),
                )
                .unwrap(),
            IdentityResolution::Ambiguous(vec![
                "lol-team:los".to_owned(),
                "lol-team:other-los".to_owned(),
            ])
        );
    }

    #[test]
    fn resolves_competition_without_collapsing_it_into_event() {
        let registry = sample_registry();
        let observed_at = utc("2026-08-09T08:00:00Z");

        assert_eq!(
            registry
                .resolve_competition(
                    DataSource::Leaguepedia,
                    Some("LCK/2026 Season/Rounds 3-4"),
                    "ignored stage name",
                    observed_at,
                )
                .unwrap(),
            IdentityResolution::Resolved("lol-competition:lck".to_owned())
        );
        assert_eq!(
            registry
                .resolve_competition(
                    DataSource::PolymarketGamma,
                    None,
                    "LCK Round 3-4 Legend Group",
                    observed_at,
                )
                .unwrap(),
            IdentityResolution::Resolved("lol-competition:lck".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_period_and_unknown_identity() {
        let mut invalid_period = sample_registry();
        invalid_period.team_identity_periods[0].valid_until_utc =
            Some(invalid_period.team_identity_periods[0].valid_from_utc);
        assert!(matches!(
            invalid_period.validate(),
            Err(IdentityRegistryError::InvalidValidityPeriod { .. })
        ));

        let mut unknown_identity = sample_registry();
        unknown_identity.team_identity_periods[0].canonical_team_id = "lol-team:missing".to_owned();
        assert_eq!(
            unknown_identity.validate(),
            Err(IdentityRegistryError::UnknownCanonicalTeam(
                "lol-team:missing".to_owned()
            ))
        );
    }

    #[derive(Debug, Deserialize)]
    struct MappingReviewRow {
        review_id: String,
        clob_game_start_utc: String,
    }

    #[derive(Debug, Deserialize)]
    struct TeamAliasReviewRow {
        canonical_team_id: String,
        gamma_name: String,
        leaguepedia_name: String,
        evidence_review_ids: String,
        review_status: String,
    }

    #[derive(Debug, Deserialize)]
    struct CompetitionReviewRow {
        canonical_competition_id: String,
        gamma_competition_name: String,
        leaguepedia_competition_id: String,
        evidence_review_ids: String,
        review_status: String,
    }

    #[test]
    fn replays_reviewed_team_aliases_and_competition_mappings() {
        let review_times = csv::Reader::from_reader(
            include_str!("../docs/DATA_008_MAPPING_REVIEW.csv").as_bytes(),
        )
        .deserialize::<MappingReviewRow>()
        .map(|row| {
            let row = row.expect("DATA-008 review row must deserialize");
            let observed_at = utc(&row.clob_game_start_utc);
            (row.review_id, observed_at)
        })
        .collect::<BTreeMap<_, _>>();
        assert_eq!(review_times.len(), 50);

        let team_rows = csv::Reader::from_reader(
            include_str!("../docs/HIST_002_TEAM_ALIAS_REVIEW.csv").as_bytes(),
        )
        .deserialize::<TeamAliasReviewRow>()
        .map(|row| row.expect("team alias review row must deserialize"))
        .collect::<Vec<_>>();
        let competition_rows = csv::Reader::from_reader(
            include_str!("../docs/HIST_002_COMPETITION_MAPPING.csv").as_bytes(),
        )
        .deserialize::<CompetitionReviewRow>()
        .map(|row| row.expect("competition review row must deserialize"))
        .collect::<Vec<_>>();
        assert_eq!(team_rows.len(), 12);
        assert_eq!(competition_rows.len(), 21);

        let mut registry = IdentityRegistry::default();
        for row in &team_rows {
            assert_eq!(row.review_status, "verified_explicit");
            if !registry
                .teams
                .iter()
                .any(|team| team.canonical_team_id == row.canonical_team_id)
            {
                registry.teams.push(CanonicalTeam {
                    canonical_team_id: row.canonical_team_id.clone(),
                    canonical_name: row.gamma_name.clone(),
                });
            }
            for review_id in row.evidence_review_ids.split(';') {
                let observed_at = *review_times
                    .get(review_id)
                    .expect("team alias evidence must reference DATA-008 row");
                let valid_until = Some(observed_at + Duration::seconds(1));
                let evidence_ref = format!("DATA-008:{review_id}");
                registry.team_identity_periods.extend([
                    TeamIdentityPeriod::new(
                        row.canonical_team_id.clone(),
                        DataSource::PolymarketGamma,
                        None,
                        row.gamma_name.clone(),
                        observed_at,
                        valid_until,
                        evidence_ref.clone(),
                    ),
                    TeamIdentityPeriod::new(
                        row.canonical_team_id.clone(),
                        DataSource::Leaguepedia,
                        None,
                        row.leaguepedia_name.clone(),
                        observed_at,
                        valid_until,
                        evidence_ref,
                    ),
                ]);
            }
        }

        for row in &competition_rows {
            assert_eq!(row.review_status, "verified_explicit");
            if !registry.competitions.iter().any(|competition| {
                competition.canonical_competition_id == row.canonical_competition_id
            }) {
                registry.competitions.push(CanonicalCompetition {
                    canonical_competition_id: row.canonical_competition_id.clone(),
                    canonical_name: row.canonical_competition_id.clone(),
                });
            }
            for review_id in row.evidence_review_ids.split(';') {
                let observed_at = *review_times
                    .get(review_id)
                    .expect("competition evidence must reference DATA-008 row");
                let valid_until = Some(observed_at + Duration::seconds(1));
                let evidence_ref = format!("DATA-008:{review_id}");
                registry.competition_identity_periods.extend([
                    CompetitionIdentityPeriod::new(
                        row.canonical_competition_id.clone(),
                        DataSource::PolymarketGamma,
                        None,
                        row.gamma_competition_name.clone(),
                        observed_at,
                        valid_until,
                        evidence_ref.clone(),
                    ),
                    CompetitionIdentityPeriod::new(
                        row.canonical_competition_id.clone(),
                        DataSource::Leaguepedia,
                        Some(row.leaguepedia_competition_id.clone()),
                        row.leaguepedia_competition_id.clone(),
                        observed_at,
                        valid_until,
                        evidence_ref,
                    ),
                ]);
            }
        }

        registry
            .validate()
            .expect("reviewed registry must be valid");
        for row in &team_rows {
            for review_id in row.evidence_review_ids.split(';') {
                let observed_at = *review_times.get(review_id).unwrap();
                assert_eq!(
                    registry
                        .resolve_team(
                            DataSource::PolymarketGamma,
                            None,
                            &row.gamma_name,
                            observed_at,
                        )
                        .unwrap(),
                    IdentityResolution::Resolved(row.canonical_team_id.clone())
                );
                assert_eq!(
                    registry
                        .resolve_team(
                            DataSource::Leaguepedia,
                            None,
                            &row.leaguepedia_name,
                            observed_at,
                        )
                        .unwrap(),
                    IdentityResolution::Resolved(row.canonical_team_id.clone())
                );
            }
        }
        for row in &competition_rows {
            for review_id in row.evidence_review_ids.split(';') {
                let observed_at = *review_times.get(review_id).unwrap();
                assert_eq!(
                    registry
                        .resolve_competition(
                            DataSource::PolymarketGamma,
                            None,
                            &row.gamma_competition_name,
                            observed_at,
                        )
                        .unwrap(),
                    IdentityResolution::Resolved(row.canonical_competition_id.clone())
                );
                assert_eq!(
                    registry
                        .resolve_competition(
                            DataSource::Leaguepedia,
                            Some(&row.leaguepedia_competition_id),
                            "ignored display name",
                            observed_at,
                        )
                        .unwrap(),
                    IdentityResolution::Resolved(row.canonical_competition_id.clone())
                );
            }
        }
    }
}
