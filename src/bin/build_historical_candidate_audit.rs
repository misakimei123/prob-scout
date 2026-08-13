use std::{fs, path::PathBuf, process::ExitCode};

use chrono::{DateTime, Utc};
use clap::Parser;
use prob_scout::historical_candidates::{
    HistoricalCandidateBuildInput, HistoricalCandidateScope, build_historical_candidate_audit,
};

#[derive(Debug, Parser)]
#[command(about = "Build a fail-closed Leaguepedia historical candidate audit")]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    start_utc: String,
    #[arg(long)]
    end_utc: String,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("historical candidate audit failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let input: HistoricalCandidateBuildInput = serde_json::from_slice(&fs::read(&args.input)?)?;
    let scope = HistoricalCandidateScope {
        start_utc: parse_utc(&args.start_utc)?,
        end_utc: parse_utc(&args.end_utc)?,
    };
    let audit = build_historical_candidate_audit(scope, input)?;
    let mut output = serde_json::to_vec_pretty(&audit)?;
    output.push(b'\n');
    fs::write(args.output, output)?;
    Ok(())
}

fn parse_utc(value: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    DateTime::parse_from_rfc3339(value).map(|value| value.with_timezone(&Utc))
}
