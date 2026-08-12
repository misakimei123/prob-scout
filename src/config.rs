use std::path::{Path, PathBuf};

use ::config::{Config, ConfigError, Environment, File, FileFormat};
use serde::Deserialize;

const ENV_PREFIX: &str = "PROB_SCOUT";
const ENV_SEPARATOR: &str = "__";

/// ProbScout 当前运行所需的最小配置。
///
/// 敏感字段不会放进该结构；未来 API Key 只能从专用环境变量读取，且不得输出日志。
#[derive(Clone, Deserialize)]
pub struct AppConfig {
    pub environment: String,
    pub database_path: PathBuf,
    pub log_level: String,
    pub log_json: bool,
}

impl AppConfig {
    /// 从可选 TOML 文件加载配置，再使用 `PROB_SCOUT__*` 环境变量覆盖。
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        Self::load_with_environment(path, application_environment())
    }

    fn load_with_environment(path: &Path, environment: Environment) -> Result<Self, ConfigError> {
        Config::builder()
            // 配置合同固定为 TOML，即使测试或部署文件没有扩展名也按同一格式解析。
            .add_source(
                File::from(path.to_path_buf())
                    .format(FileFormat::Toml)
                    .required(false),
            )
            .add_source(environment)
            .build()?
            .try_deserialize()
    }
}

fn application_environment() -> Environment {
    Environment::with_prefix(ENV_PREFIX)
        .prefix_separator(ENV_SEPARATOR)
        .separator(ENV_SEPARATOR)
        .ignore_empty(true)
        .try_parsing(true)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, io::Write as _, path::PathBuf};

    use ::config::Environment;
    use tempfile::NamedTempFile;

    use super::AppConfig;

    #[test]
    fn loads_required_values_from_temporary_toml() {
        let mut file = temporary_config(
            r#"
environment = "test"
database_path = "test.db"
log_level = "warn"
log_json = true
"#,
        );
        file.flush().expect("临时配置应成功写入磁盘");

        let config = AppConfig::load_with_environment(file.path(), empty_environment())
            .expect("有效配置应成功加载");

        assert_eq!(config.environment, "test");
        assert_eq!(config.database_path, PathBuf::from("test.db"));
        assert_eq!(config.log_level, "warn");
        assert!(config.log_json);
    }

    #[test]
    fn environment_source_overrides_file_value() {
        let mut file = temporary_config(
            r#"
environment = "test"
database_path = "test.db"
log_level = "info"
log_json = false
"#,
        );
        file.flush().expect("临时配置应成功写入磁盘");

        let environment = Environment::with_prefix("PROB_SCOUT")
            .prefix_separator("__")
            .separator("__")
            .try_parsing(true)
            .source(Some(HashMap::from([(
                "PROB_SCOUT__LOG_LEVEL".to_owned(),
                "debug".to_owned(),
            )])));

        let config = AppConfig::load_with_environment(file.path(), environment)
            .expect("环境变量覆盖后配置仍应有效");

        assert_eq!(config.log_level, "debug");
    }

    #[test]
    fn reports_missing_required_value() {
        let mut file = temporary_config(
            r#"
environment = "test"
log_level = "info"
log_json = false
"#,
        );
        file.flush().expect("临时配置应成功写入磁盘");

        let error = AppConfig::load_with_environment(file.path(), empty_environment())
            .err()
            .expect("缺少 database_path 时必须失败");

        assert!(error.to_string().contains("database_path"));
    }

    fn temporary_config(contents: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("应能创建临时配置");
        file.write_all(contents.as_bytes())
            .expect("应能写入临时配置");
        file
    }

    fn empty_environment() -> Environment {
        Environment::default().source(Some(HashMap::new()))
    }
}
