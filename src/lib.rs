//! ProbScout 的共享应用元数据。

pub mod candidate_matching;
pub mod config;
pub mod data_quality;
pub mod dataset_manifest;
pub mod db;
pub mod event_mapping;
pub mod identity_registry;
pub mod logging;
pub mod prematch_features;
pub mod series_result;
pub mod temporal_split;

/// 程序名称直接取自 Cargo package，避免 CLI 与构建元数据出现不同名称。
pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

/// 程序版本直接取自 Cargo package，后续发布时只需更新一个版本来源。
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::{APP_NAME, APP_VERSION};

    #[test]
    fn exposes_cargo_package_metadata() {
        assert_eq!(APP_NAME, "prob-scout");
        assert!(!APP_VERSION.is_empty());
    }
}
