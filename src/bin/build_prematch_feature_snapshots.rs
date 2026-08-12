use std::{env, error::Error, fs, path::PathBuf};

use prob_scout::prematch_features::{PrematchFeatureBuildInput, build_prematch_feature_snapshots};

fn main() {
    if let Err(error) = run() {
        eprintln!("prematch feature build failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let input_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: build_prematch_feature_snapshots <input.json> <output.json>")?,
    );
    let output_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: build_prematch_feature_snapshots <input.json> <output.json>")?,
    );
    if arguments.next().is_some() {
        return Err("usage: build_prematch_feature_snapshots <input.json> <output.json>".into());
    }

    // 输入 JSON 已由研究脚本拆分为赛前目标和历史观察；未知字段会被 Serde 拒绝。
    let input: PrematchFeatureBuildInput = serde_json::from_slice(&fs::read(&input_path)?)?;
    let snapshots = build_prematch_feature_snapshots(input)?;
    if snapshots.is_empty() {
        return Err("prematch feature output must contain at least one snapshot".into());
    }
    fs::write(output_path, serde_json::to_vec_pretty(&snapshots)?)?;
    Ok(())
}
