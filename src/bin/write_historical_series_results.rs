use std::{collections::BTreeMap, fs, path::PathBuf, process::ExitCode};

use chrono::SecondsFormat;
use clap::Parser;
use prob_scout::historical_identity::HistoricalIdentityAudit;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(about = "Write validated historical pure Series Results as stable CSV")]
struct Args {
    #[arg(long)]
    identity_audit: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Serialize)]
struct SeriesResultCsvRow<'a> {
    series_id: &'a str,
    competition_id: &'a str,
    league: &'a str,
    region: &'a str,
    patch: &'a str,
    scheduled_start_utc: String,
    best_of: u8,
    team_1_id: &'a str,
    team_1_name: &'a str,
    team_2_id: &'a str,
    team_2_name: &'a str,
    team_1_score: u8,
    team_2_score: u8,
    winner_team_id: &'a str,
    mapping_evidence_id: &'a str,
    result_evidence_id: &'a str,
    duplicate_candidate_count: u32,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("historical Series Result export failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let audit: HistoricalIdentityAudit = serde_json::from_slice(&fs::read(args.identity_audit)?)?;
    if audit.series_results.len() != audit.summary.series_result_count as usize
        || audit.summary.series_result_count != audit.summary.fully_resolved_series
    {
        return Err("historical identity audit Series Result counts are inconsistent".into());
    }
    let competition_names = audit
        .registry
        .competitions
        .iter()
        .map(|competition| {
            (
                competition.canonical_competition_id.as_str(),
                competition.canonical_name.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .from_path(args.output)?;
    for result in &audit.series_results {
        result.validate()?;
        let league = competition_names
            .get(result.competition_id.as_str())
            .ok_or("Series Result references unknown competition")?;
        writer.serialize(SeriesResultCsvRow {
            series_id: &result.series_id,
            competition_id: &result.competition_id,
            league,
            region: &result.region,
            patch: &result.patch,
            scheduled_start_utc: result
                .scheduled_start_utc
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            best_of: result.best_of,
            team_1_id: &result.team_ids[0],
            team_1_name: &result.team_names[0],
            team_2_id: &result.team_ids[1],
            team_2_name: &result.team_names[1],
            team_1_score: result.scores[0],
            team_2_score: result.scores[1],
            winner_team_id: &result.winner_team_id,
            mapping_evidence_id: &result.mapping_evidence_id,
            result_evidence_id: &result.result_evidence_id,
            duplicate_candidate_count: result.duplicate_candidate_count,
        })?;
    }
    writer.flush()?;
    Ok(())
}
