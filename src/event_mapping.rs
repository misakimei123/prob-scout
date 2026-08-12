use std::{error::Error, fmt, fmt::Write};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// DATA-006 当前允许进入统一映射合同的数据来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSource {
    OraclesElixir,
    Leaguepedia,
    PolymarketGamma,
    PolymarketClob,
}

impl fmt::Display for DataSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OraclesElixir => "oracles_elixir",
            Self::Leaguepedia => "leaguepedia",
            Self::PolymarketGamma => "polymarket_gamma",
            Self::PolymarketClob => "polymarket_clob",
        })
    }
}

/// 明确区分来源时间的业务语义，禁止把 Gamma endDate 静默当作比赛开赛时间。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedTimeKind {
    ScheduledStart,
    MarketEnd,
    GameStart,
}

impl fmt::Display for ObservedTimeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ScheduledStart => "scheduled_start",
            Self::MarketEnd => "market_end",
            Self::GameStart => "game_start",
        })
    }
}

/// 一个赛事数据源对同一系列赛的原始身份、队名和时间证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSourceEvidence {
    pub source: DataSource,
    pub source_event_id: String,
    pub observed_team_names: [String; 2],
    pub observed_time_utc: DateTime<Utc>,
    pub observed_time_kind: ObservedTimeKind,
}

/// ProbScout 内部的系列赛身份；来源时间留在 evidence 中，不生成未经验证的统一时间。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub game: String,
    pub competition: String,
    pub best_of: u8,
    pub canonical_team_ids: [String; 2],
    pub source_evidence: Vec<EventSourceEvidence>,
}

/// 某数据源中的队伍身份和原始展示名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamAlias {
    pub canonical_team_id: String,
    pub source: DataSource,
    pub source_team_id: Option<String>,
    pub observed_name: String,
    pub normalized_name: String,
}

impl TeamAlias {
    pub fn new(
        canonical_team_id: impl Into<String>,
        source: DataSource,
        source_team_id: Option<String>,
        observed_name: impl Into<String>,
    ) -> Self {
        let observed_name = observed_name.into();
        Self {
            canonical_team_id: canonical_team_id.into(),
            source,
            source_team_id,
            normalized_name: normalize_team_name(&observed_name),
            observed_name,
        }
    }
}

/// Polymarket outcome 与内部队伍的有序关联；token 必须保持 API outcome index 顺序。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketOutcome {
    pub outcome_index: u8,
    pub canonical_team_id: String,
    pub observed_name: String,
    pub token_id: String,
}

/// 一个系列赛与一个 Polymarket Match Winner 市场之间的可审计关联。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketMapping {
    pub event_id: String,
    pub polymarket_event_id: String,
    pub market_id: String,
    pub condition_id: String,
    pub outcomes: [MarketOutcome; 2],
    pub gamma_end_date_utc: DateTime<Utc>,
    pub clob_game_start_time_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingContractError {
    EmptyField(&'static str),
    UnsupportedBestOf(u8),
    DuplicateCanonicalTeams,
    MissingSourceEvidence,
    EventIdMismatch {
        event_id: String,
        mapped_id: String,
    },
    InvalidOutcomeOrder,
    OutcomeTeamsMismatch,
    InvalidAlias {
        source: DataSource,
        observed_name: String,
    },
    MissingAlias {
        source: DataSource,
        observed_name: String,
    },
}

impl fmt::Display for MappingContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(formatter, "required field is empty: {field}"),
            Self::UnsupportedBestOf(best_of) => {
                write!(formatter, "best_of must be one of 1, 3, or 5: {best_of}")
            }
            Self::DuplicateCanonicalTeams => formatter.write_str("event teams must be different"),
            Self::MissingSourceEvidence => formatter.write_str("event source evidence is required"),
            Self::EventIdMismatch {
                event_id,
                mapped_id,
            } => write!(
                formatter,
                "market mapping event id mismatch: event={event_id}, mapping={mapped_id}"
            ),
            Self::InvalidOutcomeOrder => {
                formatter.write_str("market outcomes must preserve indices 0 and 1")
            }
            Self::OutcomeTeamsMismatch => {
                formatter.write_str("market outcome teams do not match event teams")
            }
            Self::InvalidAlias {
                source,
                observed_name,
            } => write!(
                formatter,
                "team alias normalized name is invalid: source={source}, name={observed_name}"
            ),
            Self::MissingAlias {
                source,
                observed_name,
            } => write!(
                formatter,
                "team alias evidence is missing: source={source}, name={observed_name}"
            ),
        }
    }
}

impl Error for MappingContractError {}

/// 只做大小写、标点和空白归一化；不猜缩写、历史改名或二队关系。
pub fn normalize_team_name(name: &str) -> String {
    name.chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

impl Event {
    pub fn validate(&self) -> Result<(), MappingContractError> {
        for (field, value) in [
            ("event.event_id", self.event_id.as_str()),
            ("event.game", self.game.as_str()),
            ("event.competition", self.competition.as_str()),
            ("event.team_0", self.canonical_team_ids[0].as_str()),
            ("event.team_1", self.canonical_team_ids[1].as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(MappingContractError::EmptyField(field));
            }
        }
        if !matches!(self.best_of, 1 | 3 | 5) {
            return Err(MappingContractError::UnsupportedBestOf(self.best_of));
        }
        if self.canonical_team_ids[0] == self.canonical_team_ids[1] {
            return Err(MappingContractError::DuplicateCanonicalTeams);
        }
        if self.source_evidence.is_empty() {
            return Err(MappingContractError::MissingSourceEvidence);
        }
        for evidence in &self.source_evidence {
            if evidence.source_event_id.trim().is_empty() {
                return Err(MappingContractError::EmptyField(
                    "event_source.source_event_id",
                ));
            }
            if evidence
                .observed_team_names
                .iter()
                .any(|name| normalize_team_name(name).is_empty())
            {
                return Err(MappingContractError::EmptyField(
                    "event_source.observed_team_names",
                ));
            }
        }
        Ok(())
    }
}

impl MarketMapping {
    pub fn validate_against(
        &self,
        event: &Event,
        aliases: &[TeamAlias],
    ) -> Result<(), MappingContractError> {
        event.validate()?;
        for (field, value) in [
            (
                "mapping.polymarket_event_id",
                self.polymarket_event_id.as_str(),
            ),
            ("mapping.market_id", self.market_id.as_str()),
            ("mapping.condition_id", self.condition_id.as_str()),
            (
                "mapping.outcome_0.token_id",
                self.outcomes[0].token_id.as_str(),
            ),
            (
                "mapping.outcome_1.token_id",
                self.outcomes[1].token_id.as_str(),
            ),
        ] {
            if value.trim().is_empty() {
                return Err(MappingContractError::EmptyField(field));
            }
        }
        if self.event_id != event.event_id {
            return Err(MappingContractError::EventIdMismatch {
                event_id: event.event_id.clone(),
                mapped_id: self.event_id.clone(),
            });
        }
        if self.outcomes[0].outcome_index != 0 || self.outcomes[1].outcome_index != 1 {
            return Err(MappingContractError::InvalidOutcomeOrder);
        }
        let outcome_teams = [
            self.outcomes[0].canonical_team_id.as_str(),
            self.outcomes[1].canonical_team_id.as_str(),
        ];
        if event
            .canonical_team_ids
            .iter()
            .any(|team_id| !outcome_teams.contains(&team_id.as_str()))
            || outcome_teams[0] == outcome_teams[1]
        {
            return Err(MappingContractError::OutcomeTeamsMismatch);
        }

        for alias in aliases {
            if alias.normalized_name.is_empty()
                || alias.normalized_name != normalize_team_name(&alias.observed_name)
            {
                return Err(MappingContractError::InvalidAlias {
                    source: alias.source,
                    observed_name: alias.observed_name.clone(),
                });
            }
        }

        // 每个来源队名都必须能追溯到内部队伍；这里只验证显式 alias，不猜测相似名称。
        for evidence in &event.source_evidence {
            for (index, observed_name) in evidence.observed_team_names.iter().enumerate() {
                ensure_alias(
                    aliases,
                    &event.canonical_team_ids[index],
                    evidence.source,
                    observed_name,
                )?;
            }
        }
        for outcome in &self.outcomes {
            ensure_alias(
                aliases,
                &outcome.canonical_team_id,
                DataSource::PolymarketGamma,
                &outcome.observed_name,
            )?;
        }
        Ok(())
    }

    /// 只比较具有“开赛”语义的时间；Gamma market end 始终单独保留，不参与该差值。
    pub fn start_time_span_seconds(&self, event: &Event) -> i64 {
        let mut timestamps = event
            .source_evidence
            .iter()
            .filter(|evidence| evidence.observed_time_kind == ObservedTimeKind::ScheduledStart)
            .map(|evidence| evidence.observed_time_utc.timestamp())
            .collect::<Vec<_>>();
        timestamps.push(self.clob_game_start_time_utc.timestamp());

        let minimum = timestamps.iter().min().copied().unwrap_or_default();
        let maximum = timestamps.iter().max().copied().unwrap_or_default();
        maximum - minimum
    }

    pub fn has_start_time_conflict(&self, event: &Event, tolerance_seconds: i64) -> bool {
        tolerance_seconds < 0 || self.start_time_span_seconds(event) > tolerance_seconds
    }

    pub fn gamma_end_offset_from_clob_start_seconds(&self) -> i64 {
        self.gamma_end_date_utc
            .signed_duration_since(self.clob_game_start_time_utc)
            .num_seconds()
    }

    /// 生成可直接写入核验报告的稳定文本，列出来源 ID、原始队名和各自时间语义。
    pub fn explain(
        &self,
        event: &Event,
        aliases: &[TeamAlias],
    ) -> Result<String, MappingContractError> {
        self.validate_against(event, aliases)?;

        let mut explanation = String::new();
        writeln!(
            explanation,
            "event id={} game={} competition={} best_of={} teams={} vs {}",
            event.event_id,
            event.game,
            event.competition,
            event.best_of,
            event.canonical_team_ids[0],
            event.canonical_team_ids[1]
        )
        .expect("writing to String cannot fail");

        let mut source_evidence = event.source_evidence.iter().collect::<Vec<_>>();
        source_evidence.sort_by_key(|evidence| (evidence.source, &evidence.source_event_id));
        for evidence in source_evidence {
            writeln!(
                explanation,
                "source_event source={} id={} teams={} vs {} time_kind={} time_utc={}",
                evidence.source,
                evidence.source_event_id,
                evidence.observed_team_names[0],
                evidence.observed_team_names[1],
                evidence.observed_time_kind,
                evidence.observed_time_utc.to_rfc3339()
            )
            .expect("writing to String cannot fail");
        }

        writeln!(
            explanation,
            "market source=polymarket_gamma event_id={} market_id={} condition_id={} market_end_utc={}",
            self.polymarket_event_id,
            self.market_id,
            self.condition_id,
            self.gamma_end_date_utc.to_rfc3339()
        )
        .expect("writing to String cannot fail");
        writeln!(
            explanation,
            "market_time source=polymarket_clob time_kind=game_start time_utc={} start_span_seconds={} gamma_end_offset_seconds={}",
            self.clob_game_start_time_utc.to_rfc3339(),
            self.start_time_span_seconds(event),
            self.gamma_end_offset_from_clob_start_seconds()
        )
        .expect("writing to String cannot fail");

        for outcome in &self.outcomes {
            writeln!(
                explanation,
                "outcome index={} team_id={} name={} token_id={}",
                outcome.outcome_index,
                outcome.canonical_team_id,
                outcome.observed_name,
                outcome.token_id
            )
            .expect("writing to String cannot fail");
        }

        let mut aliases = aliases.iter().collect::<Vec<_>>();
        aliases.sort_by_key(|alias| (alias.source, &alias.canonical_team_id, &alias.observed_name));
        for alias in aliases {
            writeln!(
                explanation,
                "team_alias source={} source_team_id={} team_id={} name={} normalized={}",
                alias.source,
                alias.source_team_id.as_deref().unwrap_or("-"),
                alias.canonical_team_id,
                alias.observed_name,
                alias.normalized_name
            )
            .expect("writing to String cannot fail");
        }

        Ok(explanation)
    }
}

fn ensure_alias(
    aliases: &[TeamAlias],
    canonical_team_id: &str,
    source: DataSource,
    observed_name: &str,
) -> Result<(), MappingContractError> {
    let normalized_name = normalize_team_name(observed_name);
    if aliases.iter().any(|alias| {
        alias.canonical_team_id == canonical_team_id
            && alias.source == source
            && alias.normalized_name == normalized_name
    }) {
        return Ok(());
    }
    Err(MappingContractError::MissingAlias {
        source,
        observed_name: observed_name.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::{
        DataSource, Event, EventSourceEvidence, MappingContractError, MarketMapping, MarketOutcome,
        ObservedTimeKind, TeamAlias, normalize_team_name,
    };

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("测试时间必须是 RFC3339")
            .with_timezone(&Utc)
    }

    fn sample_contract() -> (Event, Vec<TeamAlias>, MarketMapping) {
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
        let mapping = MarketMapping {
            event_id: event.event_id.clone(),
            polymarket_event_id: "816302".to_owned(),
            market_id: "3422466".to_owned(),
            condition_id:
                "0x621f09a374447eb0965f70f78e67bb79dd773e7ca76a7646f1dd94b787597968"
                    .to_owned(),
            outcomes: [
                MarketOutcome {
                    outcome_index: 0,
                    canonical_team_id: "lol-team:dn-soopers".to_owned(),
                    observed_name: "DN SOOPers".to_owned(),
                    token_id: "89601065606835654708323034232613903153677834435591158292528649436426629091306".to_owned(),
                },
                MarketOutcome {
                    outcome_index: 1,
                    canonical_team_id: "lol-team:nongshim".to_owned(),
                    observed_name: "Nongshim Red Force".to_owned(),
                    token_id: "83918012109539856325069121542829351861121755068443319105428160825083612328645".to_owned(),
                },
            ],
            gamma_end_date_utc: utc("2026-08-12T14:00:00Z"),
            clob_game_start_time_utc: utc("2026-08-12T08:00:00Z"),
        };
        (event, aliases, mapping)
    }

    #[test]
    fn normalizes_only_case_spacing_and_punctuation() {
        assert_eq!(normalize_team_name("Nongshim RedForce"), "nongshimredforce");
        assert_eq!(
            normalize_team_name("Nongshim Red Force"),
            "nongshimredforce"
        );
        assert_ne!(normalize_team_name("NS"), "nongshimredforce");
    }

    #[test]
    fn explains_real_source_ids_names_and_time_semantics() {
        let (event, aliases, mapping) = sample_contract();

        assert_eq!(mapping.start_time_span_seconds(&event), 0);
        assert!(!mapping.has_start_time_conflict(&event, 300));
        assert_eq!(mapping.gamma_end_offset_from_clob_start_seconds(), 21_600);

        let explanation = mapping
            .explain(&event, &aliases)
            .expect("完整证据应生成映射说明");
        for expected in [
            "LCK/2026 Season/Rounds 3-4_Week 12_1",
            "Nongshim RedForce",
            "Nongshim Red Force",
            "event_id=816302",
            "market_id=3422466",
            "time_kind=scheduled_start time_utc=2026-08-12T08:00:00+00:00",
            "time_kind=game_start time_utc=2026-08-12T08:00:00+00:00",
            "gamma_end_offset_seconds=21600",
        ] {
            assert!(explanation.contains(expected), "映射说明缺少：{expected}");
        }
    }

    #[test]
    fn rejects_missing_explicit_alias_instead_of_guessing() {
        let (event, mut aliases, mapping) = sample_contract();
        aliases.retain(|alias| {
            !(alias.source == DataSource::PolymarketGamma
                && alias.canonical_team_id == "lol-team:nongshim")
        });

        let error = mapping
            .validate_against(&event, &aliases)
            .expect_err("缺少来源别名时必须拒绝合同");
        assert_eq!(
            error,
            MappingContractError::MissingAlias {
                source: DataSource::PolymarketGamma,
                observed_name: "Nongshim Red Force".to_owned(),
            }
        );
    }

    #[test]
    fn reports_scheduled_start_conflict_without_using_gamma_end() {
        let (mut event, aliases, mapping) = sample_contract();
        event.source_evidence[0].observed_time_utc = utc("2026-08-12T07:45:00Z");

        mapping
            .validate_against(&event, &aliases)
            .expect("时间冲突是后续匹配状态输入，不破坏证据合同");
        assert_eq!(mapping.start_time_span_seconds(&event), 900);
        assert!(mapping.has_start_time_conflict(&event, 300));
        assert!(!mapping.has_start_time_conflict(&event, 900));
        assert_eq!(mapping.gamma_end_offset_from_clob_start_seconds(), 21_600);
    }
}
