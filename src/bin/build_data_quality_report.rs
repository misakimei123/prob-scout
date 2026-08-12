use std::{env, error::Error, fs, path::PathBuf};

use prob_scout::data_quality::{
    DataQualityBuildInput, build_data_quality_report, render_data_quality_markdown,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("data quality report build failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let input_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: build_data_quality_report <input.json> <output.md>")?,
    );
    let output_path = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: build_data_quality_report <input.json> <output.md>")?,
    );
    if arguments.next().is_some() {
        return Err("usage: build_data_quality_report <input.json> <output.md>".into());
    }

    let input: DataQualityBuildInput = serde_json::from_slice(&fs::read(&input_path)?)?;
    let report = build_data_quality_report(input)?;
    fs::write(output_path, render_data_quality_markdown(&report))?;
    Ok(())
}
