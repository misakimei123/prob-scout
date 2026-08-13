use std::{collections::BTreeMap, fs, path::PathBuf, process::ExitCode};

use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use prob_scout::{
    event_mapping::DataSource,
    historical_candidates::HistoricalCandidateAudit,
    identity_coverage::{IdentityCoverageBuildInput, build_identity_coverage_audit},
    identity_registry::{
        CanonicalCompetition, CanonicalTeam, CompetitionIdentityPeriod, IdentityRegistry,
        TeamIdentityPeriod,
    },
};
use serde::Deserialize;

#[derive(Debug, Parser)]
#[command(about = "Build a fail-closed time-bounded identity coverage audit")]
struct Args {
    #[arg(long)]
    candidate_audit: PathBuf,
    #[arg(long)]
    mapping_review: PathBuf,
    #[arg(long)]
    team_review: PathBuf,
    #[arg(long)]
    competition_review: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Debug, Deserialize)]
struct MappingReviewRow {
    review_id: String,
    clob_game_start_utc: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct TeamReviewRow {
    canonical_team_id: String,
    gamma_name: String,
    leaguepedia_name: String,
    evidence_review_ids: String,
    review_status: String,
}

#[derive(Debug, Deserialize)]
struct CompetitionReviewRow {
    canonical_competition_id: String,
    leaguepedia_competition_id: String,
    evidence_review_ids: String,
    review_status: String,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("identity coverage audit failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let candidate_audit: HistoricalCandidateAudit =
        serde_json::from_slice(&fs::read(&args.candidate_audit)?)?;
    let registry = load_reviewed_registry(&args)?;
    let input = IdentityCoverageBuildInput {
        candidate_audit,
        registry,
    };
    let audit = build_identity_coverage_audit(input)?;
    let mut output = serde_json::to_vec_pretty(&audit)?;
    output.push(b'\n');
    fs::write(args.output, output)?;
    Ok(())
}

/// HIST-002 的证据只证明 DATA-008 观测时点，不允许把 2026 映射无限回填到历史赛事。
fn load_reviewed_registry(args: &Args) -> Result<IdentityRegistry, Box<dyn std::error::Error>> {
    let review_rows = read_csv::<MappingReviewRow>(&args.mapping_review)?;
    let mut review_times = BTreeMap::new();
    for row in review_rows {
        if row.review_id.trim().is_empty() {
            return Err("mapping review contains an empty review_id".into());
        }
        if review_times
            .insert(row.review_id.clone(), row.clob_game_start_utc)
            .is_some()
        {
            return Err(format!("mapping review_id is duplicated: {}", row.review_id).into());
        }
    }

    let mut registry = IdentityRegistry::default();
    for row in read_csv::<TeamReviewRow>(&args.team_review)? {
        require_verified("team", &row.canonical_team_id, &row.review_status)?;
        if !registry
            .teams
            .iter()
            .any(|team| team.canonical_team_id == row.canonical_team_id)
        {
            registry.teams.push(CanonicalTeam {
                canonical_team_id: row.canonical_team_id.clone(),
                canonical_name: row.gamma_name,
            });
        }
        for review_id in evidence_ids(&row.evidence_review_ids)? {
            let observed_at = *review_times.get(review_id).ok_or_else(|| {
                format!("team evidence references unknown DATA-008 review_id: {review_id}")
            })?;
            registry.team_identity_periods.push(TeamIdentityPeriod::new(
                row.canonical_team_id.clone(),
                DataSource::Leaguepedia,
                None,
                row.leaguepedia_name.clone(),
                observed_at,
                Some(observed_at + Duration::seconds(1)),
                format!("DATA-008:{review_id}"),
            ));
        }
    }

    for row in read_csv::<CompetitionReviewRow>(&args.competition_review)? {
        require_verified(
            "competition",
            &row.canonical_competition_id,
            &row.review_status,
        )?;
        if !registry
            .competitions
            .iter()
            .any(|competition| competition.canonical_competition_id == row.canonical_competition_id)
        {
            registry.competitions.push(CanonicalCompetition {
                canonical_competition_id: row.canonical_competition_id.clone(),
                canonical_name: row.canonical_competition_id.clone(),
            });
        }
        for review_id in evidence_ids(&row.evidence_review_ids)? {
            let observed_at = *review_times.get(review_id).ok_or_else(|| {
                format!("competition evidence references unknown DATA-008 review_id: {review_id}")
            })?;
            registry
                .competition_identity_periods
                .push(CompetitionIdentityPeriod::new(
                    row.canonical_competition_id.clone(),
                    DataSource::Leaguepedia,
                    Some(row.leaguepedia_competition_id.clone()),
                    row.leaguepedia_competition_id.clone(),
                    observed_at,
                    Some(observed_at + Duration::seconds(1)),
                    format!("DATA-008:{review_id}"),
                ));
        }
    }
    registry.validate()?;
    Ok(registry)
}

fn read_csv<T: for<'de> Deserialize<'de>>(
    path: &PathBuf,
) -> Result<Vec<T>, Box<dyn std::error::Error>> {
    csv::Reader::from_path(path)?
        .deserialize()
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn require_verified(
    kind: &str,
    canonical_id: &str,
    review_status: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if review_status != "verified_explicit" {
        return Err(format!(
            "{kind} identity is not verified_explicit: canonical_id={canonical_id}, status={review_status}"
        )
        .into());
    }
    Ok(())
}

fn evidence_ids(value: &str) -> Result<Vec<&str>, Box<dyn std::error::Error>> {
    let ids = value
        .split(';')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .collect::<Vec<_>>();
    if ids.is_empty() {
        return Err("identity review row has no evidence_review_ids".into());
    }
    Ok(ids)
}
