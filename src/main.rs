use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use prob_scout::{APP_NAME, APP_VERSION, config::AppConfig, db, logging};

/// Research-first prediction market bot.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// TOML configuration file; PROB_SCOUT__* environment values override it.
    #[arg(long, value_name = "PATH", default_value = "config/default.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create/upgrade the SQLite database and run a read-only health query.
    Health,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = match AppConfig::load(&cli.config) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("configuration error: {error}");
            return ExitCode::from(2);
        }
    };

    if let Err(error) = logging::init(&config.log_level, config.log_json) {
        eprintln!("logging initialization error: {error}");
        return ExitCode::from(2);
    }

    // 只记录运行上下文，不输出 database_path、API Key 或未来的钱包凭证。
    tracing::info!(
        task = "startup",
        app = APP_NAME,
        version = APP_VERSION,
        environment = config.environment,
        config_path = %cli.config.display(),
        "application ready"
    );

    match cli.command {
        Some(Command::Health) => database_health(&config).await,
        None => ExitCode::SUCCESS,
    }
}

async fn database_health(config: &AppConfig) -> ExitCode {
    let pool = match db::connect(&config.database_path).await {
        Ok(pool) => pool,
        Err(error) => {
            tracing::error!(task = "database_health", error = %error, "database initialization failed");
            return ExitCode::FAILURE;
        }
    };

    let result = db::health_check(&pool).await;
    pool.close().await;

    match result {
        Ok(()) => {
            tracing::info!(task = "database_health", "database health check passed");
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!(task = "database_health", error = %error, "database health check failed");
            ExitCode::FAILURE
        }
    }
}
