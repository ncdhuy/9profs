use sqlx::Row;

use crate::{
    ContentHash, ResearchCase, ResearchCaseId, ResearchError, ResearchSource, ResearchSourceId,
    ResearchSourceSnapshot, ResearchSourceSnapshotId,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column, json_text},
};

impl SqliteResearchRepository {
    pub(super) async fn list_cases(&self) -> Result<Vec<ResearchCase>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, title, created_at_ms, updated_at_ms FROM research_cases ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_case).collect()
    }

    pub(super) async fn get_case(
        &self,
        id: &ResearchCaseId,
    ) -> Result<Option<ResearchCase>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, title, created_at_ms, updated_at_ms FROM research_cases WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_case).transpose()
    }

    pub(super) async fn insert_case(&self, value: &ResearchCase) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_cases (id, title, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(&value.title)
        .bind(value.created_at_ms)
        .bind(value.updated_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn list_sources(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchSource>, ResearchError> {
        let rows = match research_case_id {
            Some(research_case_id) => {
                sqlx::query(
                    "SELECT id, research_case_id, kind, label, identity_json, created_at_ms \
                     FROM research_sources WHERE research_case_id = ? ORDER BY id ASC",
                )
                .bind(research_case_id.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT id, research_case_id, kind, label, identity_json, created_at_ms \
                     FROM research_sources ORDER BY id ASC",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(map_source).collect()
    }

    pub(super) async fn get_source(
        &self,
        id: &ResearchSourceId,
    ) -> Result<Option<ResearchSource>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, kind, label, identity_json, created_at_ms \
             FROM research_sources WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_source).transpose()
    }

    pub(super) async fn insert_source(&self, value: &ResearchSource) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_sources \
             (id, research_case_id, kind, label, identity_json, created_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(enum_text(&value.kind))
        .bind(&value.label)
        .bind(value.identity.as_ref().map(json_text).transpose()?)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn list_snapshots(
        &self,
        source_id: Option<&ResearchSourceId>,
    ) -> Result<Vec<ResearchSourceSnapshot>, ResearchError> {
        let query = match source_id {
            Some(_) => snapshot_select("WHERE source_id = ?"),
            None => snapshot_select(""),
        };
        let mut query = sqlx::query(&query);
        if let Some(source_id) = source_id {
            query = query.bind(source_id.as_str());
        }
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_snapshot).collect()
    }

    pub(super) async fn get_snapshot(
        &self,
        id: &ResearchSourceSnapshotId,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError> {
        let query = snapshot_select("WHERE id = ?");
        let row = sqlx::query(&query)
            .bind(id.as_str())
            .fetch_optional(&self.pool)
            .await?;
        row.map(map_snapshot).transpose()
    }

    pub(super) async fn find_snapshot_by_hash(
        &self,
        source_id: &ResearchSourceId,
        content_hash: &ContentHash,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError> {
        let query =
            snapshot_select("WHERE source_id = ? AND hash_algorithm = ? AND content_hash = ?");
        let row = sqlx::query(&query)
            .bind(source_id.as_str())
            .bind(enum_text(&content_hash.algorithm))
            .bind(&content_hash.value)
            .fetch_optional(&self.pool)
            .await?;
        row.map(map_snapshot).transpose()
    }

    pub(super) async fn insert_snapshot(
        &self,
        value: &ResearchSourceSnapshot,
    ) -> Result<bool, ResearchError> {
        let result = sqlx::query(
            "INSERT INTO research_source_snapshots \
             (id, source_id, hash_algorithm, content_hash, captured_at_ms, capture_method, origin_json, metadata_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(source_id, hash_algorithm, content_hash) DO NOTHING",
        )
        .bind(value.id.as_str())
        .bind(value.source_id.as_str())
        .bind(enum_text(&value.content_hash.algorithm))
        .bind(&value.content_hash.value)
        .bind(value.captured_at_ms)
        .bind(enum_text(&value.capture_method))
        .bind(json_text(&value.origin)?)
        .bind(json_text(&value.metadata)?)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}

fn snapshot_select(where_clause: &str) -> String {
    format!(
        "SELECT id, source_id, hash_algorithm, content_hash, captured_at_ms, capture_method, \
         origin_json, metadata_json FROM research_source_snapshots {where_clause} ORDER BY id ASC"
    )
}

fn map_case(row: sqlx::sqlite::SqliteRow) -> Result<ResearchCase, ResearchError> {
    Ok(ResearchCase {
        id: ResearchCaseId::parse(row.get::<String, _>("id"))?,
        title: row.get("title"),
        created_at_ms: row.get("created_at_ms"),
        updated_at_ms: row.get("updated_at_ms"),
    })
}

fn map_source(row: sqlx::sqlite::SqliteRow) -> Result<ResearchSource, ResearchError> {
    Ok(ResearchSource {
        id: ResearchSourceId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        kind: json_column(
            format!("\"{}\"", row.get::<String, _>("kind")),
            "source kind",
        )?,
        label: row.get("label"),
        identity: row
            .try_get::<Option<String>, _>("identity_json")
            .map_err(ResearchError::Database)?
            .map(|value| json_column(value, "source identity"))
            .transpose()?,
        created_at_ms: row.get("created_at_ms"),
    })
}

fn map_snapshot(row: sqlx::sqlite::SqliteRow) -> Result<ResearchSourceSnapshot, ResearchError> {
    Ok(ResearchSourceSnapshot {
        id: ResearchSourceSnapshotId::parse(row.get::<String, _>("id"))?,
        source_id: ResearchSourceId::parse(row.get::<String, _>("source_id"))?,
        content_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("hash_algorithm")),
                "snapshot hash algorithm",
            )?,
            value: row.get("content_hash"),
        },
        captured_at_ms: row.get("captured_at_ms"),
        capture_method: json_column(
            format!("\"{}\"", row.get::<String, _>("capture_method")),
            "snapshot capture method",
        )?,
        origin: json_column(row.get("origin_json"), "snapshot origin")?,
        metadata: json_column(row.get("metadata_json"), "snapshot metadata")?,
    })
}
