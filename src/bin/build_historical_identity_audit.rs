use std::{fs, path::PathBuf, process::ExitCode};

use clap::Parser;
use prob_scout::historical_identity::{
    HistoricalIdentityBuildInput, RawTeamRedirectRow, RawTournamentIdentityRow,
    build_historical_identity_audit,
};

#[derive(Debug, Parser)]
#[command(about = "Build explicit 2025 Leaguepedia identity evidence and coverage")]
struct Args {
    #[arg(long)]
    candidate_audit: PathBuf,
    #[arg(long, required = true)]
    team_redirects: Vec<PathBuf>,
    #[arg(long, required = true)]
    tournaments: Vec<PathBuf>,
    #[arg(long)]
    output: PathBuf,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("historical identity audit failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let candidate_audit = serde_json::from_slice(&fs::read(args.candidate_audit)?)?;
    let team_redirect_rows = read_rows::<RawTeamRedirectRow>(&args.team_redirects)?;
    let tournament_rows = read_rows::<RawTournamentIdentityRow>(&args.tournaments)?;
    let audit = build_historical_identity_audit(HistoricalIdentityBuildInput {
        candidate_audit,
        team_redirect_rows,
        tournament_rows,
    })?;
    let mut output = serde_json::to_vec_pretty(&audit)?;
    output.push(b'\n');
    fs::write(args.output, output)?;
    Ok(())
}

fn read_rows<T: for<'de> serde::Deserialize<'de>>(
    paths: &[PathBuf],
) -> Result<Vec<T>, Box<dyn std::error::Error>> {
    let mut rows = Vec::new();
    for path in paths {
        let mut page = serde_json::from_slice::<Vec<T>>(&fs::read(path)?)?;
        rows.append(&mut page);
    }
    Ok(rows)
}
