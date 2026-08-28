//! SQLite infrastructure and repository contracts for 9Profs Core.
//!
//! Migrations 0002–0012 own assistants, agent metadata, MCP configuration, and
//! persistent research evidence/provenance/retrieval/manuscript citation sync,
//! reference catalog, and claim extraction state.

use std::path::Path;

use async_trait::async_trait;
use nineprofs_common::{TimestampMs, now_ms};
use sqlx::migrate::Migrator;
use sqlx::pool::PoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::{Row, Sqlite, SqlitePool};
use thiserror::Error;

// Keep the migration directory attached to this crate so additive migrations are rebuilt.
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("database query failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

#[derive(Clone, Debug)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .pragma("busy_timeout", "5000");
        let pool = PoolOptions::<Sqlite>::new()
            .max_connections(5)
            .connect_with(options)
            .await?;

        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn in_memory() -> Result<Self, DbError> {
        let pool = PoolOptions::<Sqlite>::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn metadata_repository(&self) -> SqliteMetadataRepository {
        SqliteMetadataRepository::new(self.pool.clone())
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataRecord {
    pub key: String,
    pub value: String,
    pub updated_at_ms: TimestampMs,
}

#[async_trait]
pub trait MetadataRepository: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<MetadataRecord>, DbError>;
    async fn upsert(&self, key: &str, value: &str) -> Result<(), DbError>;
}

#[derive(Clone, Debug)]
pub struct SqliteMetadataRepository {
    pool: SqlitePool,
}

impl SqliteMetadataRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MetadataRepository for SqliteMetadataRepository {
    async fn get(&self, key: &str) -> Result<Option<MetadataRecord>, DbError> {
        let row = sqlx::query("SELECT key, value, updated_at_ms FROM core_metadata WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|row| MetadataRecord {
            key: row.get("key"),
            value: row.get("value"),
            updated_at_ms: row.get("updated_at_ms"),
        }))
    }

    async fn upsert(&self, key: &str, value: &str) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO core_metadata (key, value, updated_at_ms) VALUES (?, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_ms = excluded.updated_at_ms",
        )
        .bind(key)
        .bind(value)
        .bind(now_ms())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_database_runs_migrations_and_repository_round_trips() {
        let database = Database::in_memory().await.unwrap();
        let repository = database.metadata_repository();

        assert!(repository.get("test.key").await.unwrap().is_none());
        repository.upsert("test.key", "test-value").await.unwrap();

        let record = repository.get("test.key").await.unwrap().unwrap();
        assert_eq!(record.key, "test.key");
        assert_eq!(record.value, "test-value");
        assert!(record.updated_at_ms > 0);
    }
}
