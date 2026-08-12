use std::{env, error::Error, fs, path::PathBuf};

use prob_scout::temporal_split::{
    TemporalSplitCandidate, TemporalSplitPlan, build_temporal_split_manifest,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildInput {
    source_dataset_sha256: String,
    plan: TemporalSplitPlan,
    candidates: Vec<TemporalSplitCandidate>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("temporal split build failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let input_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: build_temporal_split_manifest <input.json> <output.json>")?,
    );
    let output_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: build_temporal_split_manifest <input.json> <output.json>")?,
    );
    if arguments.next().is_some() {
        return Err("usage: build_temporal_split_manifest <input.json> <output.json>".into());
    }

    // 构建入口只读取 series identity 与 Scheduled Start，不读取特征值或赛果 label。
    let input: BuildInput = serde_json::from_slice(&fs::read(&input_path)?)?;
    let manifest =
        build_temporal_split_manifest(input.candidates, input.plan, input.source_dataset_sha256)?;
    fs::write(output_path, serde_json::to_vec_pretty(&manifest)?)?;
    Ok(())
}
