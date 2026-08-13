use std::{env, error::Error, fs, path::PathBuf};

use prob_scout::temporal_split::{
    FinalTestReleaseAuthorization, TemporalSplitCandidate, TemporalSplitManifest,
    release_final_test,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseInput {
    sealed_manifest: TemporalSplitManifest,
    candidates: Vec<TemporalSplitCandidate>,
    authorization: FinalTestReleaseAuthorization,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("final test release failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let input_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: release_final_test_manifest <input.json> <output.json>")?,
    );
    let output_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: release_final_test_manifest <input.json> <output.json>")?,
    );
    if arguments.next().is_some() {
        return Err("usage: release_final_test_manifest <input.json> <output.json>".into());
    }
    if output_path.exists() {
        return Err(format!(
            "released final-test manifest already exists: {}",
            output_path.display()
        )
        .into());
    }

    // release 输入只包含不可变时间成员与冻结授权；Rust 合同会重新核对 count 和 commitment。
    let input: ReleaseInput = serde_json::from_slice(&fs::read(&input_path)?)?;
    let released = release_final_test(
        &input.sealed_manifest,
        &input.candidates,
        input.authorization,
    )?;
    fs::write(output_path, serde_json::to_vec_pretty(&released)?)?;
    Ok(())
}
