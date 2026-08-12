use std::{env, fs, process::ExitCode};

use prob_scout::dataset_manifest::DatasetManifest;

fn main() -> ExitCode {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let Some(path) = arguments.next() else {
        eprintln!("usage: validate_dataset_manifest <manifest.json>");
        return ExitCode::from(2);
    };
    if arguments.next().is_some() {
        eprintln!("validate_dataset_manifest accepts exactly one path");
        return ExitCode::from(2);
    }

    // 先拒绝无法读取或 schema 不匹配的 JSON，再执行 Manifest v1 的业务约束。
    let result = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read manifest: {error}"))
        .and_then(|contents| {
            serde_json::from_str::<DatasetManifest>(&contents)
                .map_err(|error| format!("failed to deserialize manifest: {error}"))
        })
        .and_then(|manifest| {
            manifest
                .validate()
                .map_err(|error| format!("invalid dataset manifest: {error}"))
        });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
