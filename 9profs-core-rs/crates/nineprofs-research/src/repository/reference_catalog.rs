use sqlx::Row;

use crate::{
    CitationOccurrenceId, CitationTargetId, ContentHash, ManuscriptCitationSyncRunId,
    ManuscriptReferenceCatalogRun, ManuscriptReferenceCatalogRunId,
    ManuscriptReferenceCatalogWrite, ManuscriptReferenceEntry, ManuscriptReferenceEntryId,
    ManuscriptReferenceTargetMapping, ManuscriptReferenceTargetMappingId, ResearchCaseId,
    ResearchError, ResearchSourceId,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column, json_text},
};

impl SqliteResearchRepository {
    pub(super) async fn get_manuscript_reference_catalog_run(
        &self,
        id: &ManuscriptReferenceCatalogRunId,
    ) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
             document_id, document_version, catalog_hash_algorithm, catalog_hash, status, \
             entry_count, target_mapping_count, created_at_ms, completed_at_ms, failure_code \
             FROM research_manuscript_reference_catalog_runs WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_reference_catalog_run).transpose()
    }

    pub(super) async fn get_manuscript_reference_catalog_for_sync(
        &self,
        citation_sync_run_id: &ManuscriptCitationSyncRunId,
    ) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
             document_id, document_version, catalog_hash_algorithm, catalog_hash, status, \
             entry_count, target_mapping_count, created_at_ms, completed_at_ms, failure_code \
             FROM research_manuscript_reference_catalog_runs WHERE citation_sync_run_id = ?",
        )
        .bind(citation_sync_run_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_reference_catalog_run).transpose()
    }

    pub(super) async fn latest_manuscript_reference_catalog(
        &self,
        research_case_id: &ResearchCaseId,
        manuscript_source_id: &ResearchSourceId,
    ) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
             document_id, document_version, catalog_hash_algorithm, catalog_hash, status, \
             entry_count, target_mapping_count, created_at_ms, completed_at_ms, failure_code \
             FROM research_manuscript_reference_catalog_runs \
             WHERE research_case_id = ? AND manuscript_source_id = ? AND status = 'completed' \
             ORDER BY created_at_ms DESC, id DESC LIMIT 1",
        )
        .bind(research_case_id.as_str())
        .bind(manuscript_source_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_reference_catalog_run).transpose()
    }

    pub(super) async fn list_manuscript_reference_entries(
        &self,
        catalog_run_id: &ManuscriptReferenceCatalogRunId,
    ) -> Result<Vec<ManuscriptReferenceEntry>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, catalog_run_id, ordinal, format, reference_key, \
             descriptor_hash_algorithm, descriptor_hash, word_tag, word_title, word_author, \
             word_year, zotero_item_id, zotero_uris_json, target_count \
             FROM research_manuscript_reference_entries \
             WHERE catalog_run_id = ? ORDER BY ordinal ASC",
        )
        .bind(catalog_run_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(map_manuscript_reference_entry)
            .collect()
    }

    pub(super) async fn get_manuscript_reference_entry(
        &self,
        id: &ManuscriptReferenceEntryId,
    ) -> Result<Option<ManuscriptReferenceEntry>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, catalog_run_id, ordinal, format, reference_key, \
             descriptor_hash_algorithm, descriptor_hash, word_tag, word_title, word_author, \
             word_year, zotero_item_id, zotero_uris_json, target_count \
             FROM research_manuscript_reference_entries WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_reference_entry).transpose()
    }

    pub(super) async fn list_manuscript_reference_target_mappings(
        &self,
        reference_entry_id: &ManuscriptReferenceEntryId,
    ) -> Result<Vec<ManuscriptReferenceTargetMapping>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, catalog_run_id, reference_entry_id, citation_occurrence_id, \
             citation_target_id, document_target_ordinal \
             FROM research_manuscript_reference_target_mappings \
             WHERE reference_entry_id = ? ORDER BY document_target_ordinal ASC, id ASC",
        )
        .bind(reference_entry_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(map_manuscript_reference_target_mapping)
            .collect()
    }

    pub(super) async fn persist_manuscript_reference_catalog(
        &self,
        value: &ManuscriptReferenceCatalogWrite,
    ) -> Result<ManuscriptReferenceCatalogRun, ResearchError> {
        let mut transaction = self.pool.begin().await?;
        let existing = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
             document_id, document_version, catalog_hash_algorithm, catalog_hash, status, \
             entry_count, target_mapping_count, created_at_ms, completed_at_ms, failure_code \
             FROM research_manuscript_reference_catalog_runs WHERE citation_sync_run_id = ?",
        )
        .bind(value.run.citation_sync_run_id.as_str())
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(row) = existing {
            let existing = map_manuscript_reference_catalog_run(row)?;
            if existing.catalog_hash == value.run.catalog_hash {
                transaction.commit().await?;
                return Ok(existing);
            }
            return Err(ResearchError::ManuscriptReferenceCatalogConflict {
                citation_sync_run_id: value.run.citation_sync_run_id.to_string(),
            });
        }

        sqlx::query(
            "INSERT INTO research_manuscript_reference_catalog_runs \
             (id, research_case_id, manuscript_source_id, citation_sync_run_id, document_id, \
              document_version, catalog_hash_algorithm, catalog_hash, status, entry_count, \
              target_mapping_count, created_at_ms, completed_at_ms, failure_code) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.run.id.as_str())
        .bind(value.run.research_case_id.as_str())
        .bind(value.run.manuscript_source_id.as_str())
        .bind(value.run.citation_sync_run_id.as_str())
        .bind(&value.run.document_id)
        .bind(value.run.document_version)
        .bind(enum_text(&value.run.catalog_hash.algorithm))
        .bind(&value.run.catalog_hash.value)
        .bind(enum_text(&value.run.status))
        .bind(value.run.entry_count)
        .bind(value.run.target_mapping_count)
        .bind(value.run.created_at_ms)
        .bind(value.run.completed_at_ms)
        .bind(&value.run.failure_code)
        .execute(&mut *transaction)
        .await?;

        for entry in &value.entries {
            sqlx::query(
                "INSERT INTO research_manuscript_reference_entries \
                 (id, catalog_run_id, ordinal, format, reference_key, descriptor_hash_algorithm, \
                  descriptor_hash, word_tag, word_title, word_author, word_year, zotero_item_id, \
                  zotero_uris_json, target_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(entry.id.as_str())
            .bind(entry.catalog_run_id.as_str())
            .bind(entry.ordinal)
            .bind(enum_text(&entry.format))
            .bind(&entry.reference_key)
            .bind(enum_text(&entry.descriptor_hash.algorithm))
            .bind(&entry.descriptor_hash.value)
            .bind(&entry.word_tag)
            .bind(&entry.word_title)
            .bind(&entry.word_author)
            .bind(&entry.word_year)
            .bind(&entry.zotero_item_id)
            .bind(json_text(&entry.zotero_uris)?)
            .bind(entry.target_count)
            .execute(&mut *transaction)
            .await?;
        }

        for mapping in &value.mappings {
            sqlx::query(
                "INSERT INTO research_manuscript_reference_target_mappings \
                 (id, catalog_run_id, reference_entry_id, citation_occurrence_id, \
                  citation_target_id, document_target_ordinal) VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(mapping.id.as_str())
            .bind(mapping.catalog_run_id.as_str())
            .bind(mapping.reference_entry_id.as_str())
            .bind(mapping.citation_occurrence_id.as_str())
            .bind(mapping.citation_target_id.as_str())
            .bind(mapping.document_target_ordinal)
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;
        Ok(value.run.clone())
    }
}

fn map_manuscript_reference_catalog_run(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptReferenceCatalogRun, ResearchError> {
    Ok(ManuscriptReferenceCatalogRun {
        id: ManuscriptReferenceCatalogRunId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        manuscript_source_id: ResearchSourceId::parse(
            row.get::<String, _>("manuscript_source_id"),
        )?,
        citation_sync_run_id: ManuscriptCitationSyncRunId::parse(
            row.get::<String, _>("citation_sync_run_id"),
        )?,
        document_id: row.get("document_id"),
        document_version: row.get("document_version"),
        catalog_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("catalog_hash_algorithm")),
                "manuscript reference catalog hash algorithm",
            )?,
            value: row.get("catalog_hash"),
        },
        entry_count: row.get::<i64, _>("entry_count") as u32,
        target_mapping_count: row.get::<i64, _>("target_mapping_count") as u32,
        status: json_column(
            format!("\"{}\"", row.get::<String, _>("status")),
            "manuscript reference catalog status",
        )?,
        created_at_ms: row.get("created_at_ms"),
        completed_at_ms: row.get("completed_at_ms"),
        failure_code: row.get("failure_code"),
    })
}

fn map_manuscript_reference_entry(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptReferenceEntry, ResearchError> {
    Ok(ManuscriptReferenceEntry {
        id: ManuscriptReferenceEntryId::parse(row.get::<String, _>("id"))?,
        catalog_run_id: ManuscriptReferenceCatalogRunId::parse(
            row.get::<String, _>("catalog_run_id"),
        )?,
        ordinal: row.get::<i64, _>("ordinal") as u32,
        format: json_column(
            format!("\"{}\"", row.get::<String, _>("format")),
            "manuscript reference entry format",
        )?,
        reference_key: row.get("reference_key"),
        descriptor_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("descriptor_hash_algorithm")),
                "manuscript reference descriptor hash algorithm",
            )?,
            value: row.get("descriptor_hash"),
        },
        word_tag: row.get("word_tag"),
        word_title: row.get("word_title"),
        word_author: row.get("word_author"),
        word_year: row.get("word_year"),
        zotero_item_id: row.get("zotero_item_id"),
        zotero_uris: json_column(row.get("zotero_uris_json"), "Zotero URI list")?,
        target_count: row.get::<i64, _>("target_count") as u32,
    })
}

fn map_manuscript_reference_target_mapping(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptReferenceTargetMapping, ResearchError> {
    Ok(ManuscriptReferenceTargetMapping {
        id: ManuscriptReferenceTargetMappingId::parse(row.get::<String, _>("id"))?,
        catalog_run_id: ManuscriptReferenceCatalogRunId::parse(
            row.get::<String, _>("catalog_run_id"),
        )?,
        reference_entry_id: ManuscriptReferenceEntryId::parse(
            row.get::<String, _>("reference_entry_id"),
        )?,
        citation_occurrence_id: CitationOccurrenceId::parse(
            row.get::<String, _>("citation_occurrence_id"),
        )?,
        citation_target_id: CitationTargetId::parse(row.get::<String, _>("citation_target_id"))?,
        document_target_ordinal: row.get::<i64, _>("document_target_ordinal") as u32,
    })
}
