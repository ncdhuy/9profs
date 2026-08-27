use sqlx::Row;

use crate::{
    CitationOccurrenceId, CitationTargetId, ContentHash, ManuscriptCitationSyncOccurrence,
    ManuscriptCitationSyncOccurrenceId, ManuscriptCitationSyncRun, ManuscriptCitationSyncRunId,
    ManuscriptCitationSyncTarget, ManuscriptCitationSyncTargetId, ManuscriptCitationSyncWrite,
    ResearchCaseId, ResearchError, ResearchSourceId,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column, json_text},
};

impl SqliteResearchRepository {
    pub(super) async fn get_manuscript_citation_sync(
        &self,
        id: &ManuscriptCitationSyncRunId,
    ) -> Result<Option<ManuscriptCitationSyncRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, document_id, document_version, \
             inventory_hash_algorithm, inventory_hash, status, occurrence_count, created_at_ms, \
             completed_at_ms, failure_code \
             FROM research_manuscript_citation_sync_runs WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_citation_sync_run).transpose()
    }

    pub(super) async fn latest_manuscript_citation_sync(
        &self,
        research_case_id: &ResearchCaseId,
        manuscript_source_id: &ResearchSourceId,
    ) -> Result<Option<ManuscriptCitationSyncRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, document_id, document_version, \
             inventory_hash_algorithm, inventory_hash, status, occurrence_count, created_at_ms, \
             completed_at_ms, failure_code \
             FROM research_manuscript_citation_sync_runs \
             WHERE research_case_id = ? AND manuscript_source_id = ? AND status = 'completed' \
             ORDER BY created_at_ms DESC, id DESC LIMIT 1",
        )
        .bind(research_case_id.as_str())
        .bind(manuscript_source_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_citation_sync_run).transpose()
    }

    pub(super) async fn list_manuscript_citation_sync_occurrences(
        &self,
        sync_run_id: &ManuscriptCitationSyncRunId,
    ) -> Result<Vec<ManuscriptCitationSyncOccurrence>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, sync_run_id, ordinal, citation_occurrence_id, document_block_id, \
             start, end, format FROM research_manuscript_citation_sync_occurrences \
             WHERE sync_run_id = ? ORDER BY ordinal ASC",
        )
        .bind(sync_run_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(map_manuscript_citation_sync_occurrence)
            .collect()
    }

    pub(super) async fn get_manuscript_citation_sync_occurrence(
        &self,
        id: &ManuscriptCitationSyncOccurrenceId,
    ) -> Result<Option<ManuscriptCitationSyncOccurrence>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, sync_run_id, ordinal, citation_occurrence_id, document_block_id, \
             start, end, format FROM research_manuscript_citation_sync_occurrences WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_citation_sync_occurrence).transpose()
    }

    pub(super) async fn list_manuscript_citation_sync_targets(
        &self,
        sync_occurrence_id: &ManuscriptCitationSyncOccurrenceId,
    ) -> Result<Vec<ManuscriptCitationSyncTarget>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, sync_occurrence_id, document_target_ordinal, citation_target_id \
             FROM research_manuscript_citation_sync_targets \
             WHERE sync_occurrence_id = ? ORDER BY document_target_ordinal ASC",
        )
        .bind(sync_occurrence_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(map_manuscript_citation_sync_target)
            .collect()
    }

    pub(super) async fn persist_manuscript_citation_sync(
        &self,
        value: &ManuscriptCitationSyncWrite,
    ) -> Result<ManuscriptCitationSyncRun, ResearchError> {
        let mut transaction = self.pool.begin().await?;
        let existing = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, document_id, document_version, \
             inventory_hash_algorithm, inventory_hash, status, occurrence_count, created_at_ms, \
             completed_at_ms, failure_code \
             FROM research_manuscript_citation_sync_runs \
             WHERE research_case_id = ? AND manuscript_source_id = ? AND document_id = ? \
             AND document_version = ?",
        )
        .bind(value.run.research_case_id.as_str())
        .bind(value.run.manuscript_source_id.as_str())
        .bind(&value.run.document_id)
        .bind(value.run.document_version)
        .fetch_optional(&mut *transaction)
        .await?;

        if let Some(row) = existing {
            let existing = map_manuscript_citation_sync_run(row)?;
            if existing.inventory_hash == value.run.inventory_hash {
                transaction.commit().await?;
                return Ok(existing);
            }
            return Err(ResearchError::ManuscriptCitationSyncConflict {
                research_case_id: value.run.research_case_id.to_string(),
                manuscript_source_id: value.run.manuscript_source_id.to_string(),
                document_id: value.run.document_id.clone(),
                document_version: value.run.document_version,
            });
        }

        sqlx::query(
            "INSERT INTO research_manuscript_citation_sync_runs \
             (id, research_case_id, manuscript_source_id, document_id, document_version, \
              inventory_hash_algorithm, inventory_hash, status, occurrence_count, created_at_ms, \
              completed_at_ms, failure_code) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.run.id.as_str())
        .bind(value.run.research_case_id.as_str())
        .bind(value.run.manuscript_source_id.as_str())
        .bind(&value.run.document_id)
        .bind(value.run.document_version)
        .bind(enum_text(&value.run.inventory_hash.algorithm))
        .bind(&value.run.inventory_hash.value)
        .bind(enum_text(&value.run.status))
        .bind(value.run.occurrence_count)
        .bind(value.run.created_at_ms)
        .bind(value.run.completed_at_ms)
        .bind(&value.run.failure_code)
        .execute(&mut *transaction)
        .await?;

        for occurrence in &value.citation_occurrences {
            sqlx::query(
                "INSERT INTO research_citation_occurrences \
                 (id, research_case_id, origin_json, rendered_text, created_at_ms) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(occurrence.id.as_str())
            .bind(occurrence.research_case_id.as_str())
            .bind(json_text(&occurrence.origin)?)
            .bind(&occurrence.rendered_text)
            .bind(occurrence.created_at_ms)
            .execute(&mut *transaction)
            .await?;
        }

        for target in &value.citation_targets {
            sqlx::query(
                "INSERT INTO research_citation_targets \
                 (id, citation_occurrence_id, ordinal, reference_key, cited_locator) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(target.id.as_str())
            .bind(target.citation_occurrence_id.as_str())
            .bind(target.ordinal)
            .bind(&target.reference_key)
            .bind(&target.cited_locator)
            .execute(&mut *transaction)
            .await?;
        }

        for occurrence in &value.sync_occurrences {
            sqlx::query(
                "INSERT INTO research_manuscript_citation_sync_occurrences \
                 (id, sync_run_id, ordinal, citation_occurrence_id, document_block_id, start, end, format) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(occurrence.id.as_str())
            .bind(occurrence.sync_run_id.as_str())
            .bind(occurrence.ordinal)
            .bind(occurrence.citation_occurrence_id.as_str())
            .bind(&occurrence.document_block_id)
            .bind(i64::try_from(occurrence.start).map_err(|_| {
                ResearchError::Invalid("manuscript citation start exceeds SQLite range".to_owned())
            })?)
            .bind(i64::try_from(occurrence.end).map_err(|_| {
                ResearchError::Invalid("manuscript citation end exceeds SQLite range".to_owned())
            })?)
            .bind(enum_text(&occurrence.format))
            .execute(&mut *transaction)
            .await?;
        }

        for target in &value.sync_targets {
            sqlx::query(
                "INSERT INTO research_manuscript_citation_sync_targets \
                 (id, sync_occurrence_id, document_target_ordinal, citation_target_id) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(target.id.as_str())
            .bind(target.sync_occurrence_id.as_str())
            .bind(target.document_target_ordinal)
            .bind(target.citation_target_id.as_str())
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;
        Ok(value.run.clone())
    }
}

fn map_manuscript_citation_sync_run(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptCitationSyncRun, ResearchError> {
    Ok(ManuscriptCitationSyncRun {
        id: ManuscriptCitationSyncRunId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        manuscript_source_id: ResearchSourceId::parse(
            row.get::<String, _>("manuscript_source_id"),
        )?,
        document_id: row.get("document_id"),
        document_version: row.get("document_version"),
        inventory_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("inventory_hash_algorithm")),
                "manuscript citation inventory hash algorithm",
            )?,
            value: row.get("inventory_hash"),
        },
        status: json_column(
            format!("\"{}\"", row.get::<String, _>("status")),
            "manuscript citation sync status",
        )?,
        occurrence_count: row.get::<i64, _>("occurrence_count") as u32,
        created_at_ms: row.get("created_at_ms"),
        completed_at_ms: row.get("completed_at_ms"),
        failure_code: row.get("failure_code"),
    })
}

fn map_manuscript_citation_sync_occurrence(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptCitationSyncOccurrence, ResearchError> {
    Ok(ManuscriptCitationSyncOccurrence {
        id: ManuscriptCitationSyncOccurrenceId::parse(row.get::<String, _>("id"))?,
        sync_run_id: ManuscriptCitationSyncRunId::parse(row.get::<String, _>("sync_run_id"))?,
        ordinal: row.get::<i64, _>("ordinal") as u32,
        citation_occurrence_id: CitationOccurrenceId::parse(
            row.get::<String, _>("citation_occurrence_id"),
        )?,
        document_block_id: row.get("document_block_id"),
        start: row.get::<i64, _>("start") as u64,
        end: row.get::<i64, _>("end") as u64,
        format: json_column(
            format!("\"{}\"", row.get::<String, _>("format")),
            "manuscript citation format",
        )?,
    })
}

fn map_manuscript_citation_sync_target(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptCitationSyncTarget, ResearchError> {
    Ok(ManuscriptCitationSyncTarget {
        id: ManuscriptCitationSyncTargetId::parse(row.get::<String, _>("id"))?,
        sync_occurrence_id: ManuscriptCitationSyncOccurrenceId::parse(
            row.get::<String, _>("sync_occurrence_id"),
        )?,
        document_target_ordinal: row.get::<i64, _>("document_target_ordinal") as u32,
        citation_target_id: CitationTargetId::parse(row.get::<String, _>("citation_target_id"))?,
    })
}
