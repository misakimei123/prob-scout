use std::{error::Error, fs, path::Path, time::Duration};

use sqlx::{
    SqlitePool,
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

type DatabaseResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// 打开 SQLite、执行未应用的 migration，并返回容量受限的共享连接池。
pub async fn connect(database_path: &Path) -> DatabaseResult<SqlitePool> {
    // SQLite 不会自动创建父目录；只在路径确实包含父目录时创建它。
    if let Some(parent) = database_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }

    let connect_options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));

    // 个人单进程服务不需要大量连接；显式上限可避免 SQLite 写锁争用被放大。
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;

    // SQLx 会校验历史 migration hash，并且只执行尚未应用的版本。
    MIGRATOR.run(&pool).await?;

    Ok(pool)
}

/// 执行最小只读查询，确认连接池能够正常访问数据库。
pub async fn health_check(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let _: i64 = sqlx::query_scalar("SELECT 1").fetch_one(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{connect, health_check};

    #[tokio::test]
    async fn initializes_writes_and_reopens_without_reapplying_migration() {
        let directory = tempdir().expect("应能创建临时数据库目录");
        let database_path = directory.path().join("nested/prob-scout.db");

        let pool = connect(&database_path)
            .await
            .expect("空数据库应自动创建并执行 migration");
        health_check(&pool).await.expect("健康检查应通过");

        sqlx::query(
            r#"
INSERT INTO source_records (
    source_name,
    external_id,
    captured_at,
    content_hash,
    raw_path
) VALUES (?, ?, ?, ?, ?)
"#,
        )
        .bind("fixture")
        .bind("match-1")
        .bind("2026-08-12T00:00:00Z")
        .bind("sha256:test")
        .bind("data/raw/fixture.json")
        .execute(&pool)
        .await
        .expect("最小写入应成功");

        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .expect("应能读取 journal mode");
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .expect("应能读取 foreign key 配置");
        assert_eq!(journal_mode, "wal");
        assert_eq!(foreign_keys, 1);
        pool.close().await;

        let reopened_pool = connect(&database_path)
            .await
            .expect("已有数据库应安全重复执行 migration runner");
        let content_hash: String = sqlx::query_scalar(
            "SELECT content_hash FROM source_records WHERE source_name = ? AND external_id = ?",
        )
        .bind("fixture")
        .bind("match-1")
        .fetch_one(&reopened_pool)
        .await
        .expect("重开后已写入数据应保留");
        let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(&reopened_pool)
            .await
            .expect("应能读取 SQLx migration 记录");

        assert_eq!(content_hash, "sha256:test");
        assert_eq!(migration_count, 1);
        reopened_pool.close().await;
    }
}
