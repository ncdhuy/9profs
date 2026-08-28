use sqlx::Row;

use crate::{
    ContentHash, ManuscriptReferenceCatalogRunId, ManuscriptReferenceEntryId,
    ManuscriptReferenceResolutionCandidate, ManuscriptReferenceResolutionCandidateId,
    ManuscriptReferenceResolutionEntry, ManuscriptReferenceResolutionEntryId,
    ManuscriptReferenceResolutionRun, ManuscriptReferenceResolutionRunId,
    ManuscriptReferenceResolutionWrite, ResearchError,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column},
};

impl SqliteResearchRepository {
    pub(super) async fn get_manuscript_reference_resolution_run(
        &self,
        id: &ManuscriptReferenceResolutionRunId,
    ) -> Result<Option<ManuscriptReferenceResolutionRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, catalog_run_id, catalog_hash_algorithm, catalog_hash, \
             source_state_hash_algorithm, source_state_hash, resolver_policy_version, status, \
             entry_count, resolved_entry_count, candidate_entry_count, unresolved_entry_count, \
             conflict_entry_count, created_at_ms, completed_at_ms, failure_code \
             FROM research_manuscript_reference_resolution_runs WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_resolution_run).transpose()
    }

    pub(super) async fn get_manuscript_reference_resolution_for_catalog(
        &self,
        catalog_run_id: &ManuscriptReferenceCatalogRunId,
        catalog_hash: &ContentHash,
        source_state_hash: &ContentHash,
        resolver_policy_version: &str,
    ) -> Result<Option<ManuscriptReferenceResolutionRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, catalog_run_id, catalog_hash_algorithm, catalog_hash, \
             source_state_hash_algorithm, source_state_hash, resolver_policy_version, status, \
             entry_count, resolved_entry_count, candidate_entry_count, unresolved_entry_count, \
             conflict_entry_count, created_at_ms, completed_at_ms, failure_code \
             FROM research_manuscript_reference_resolution_runs \
             WHERE catalog_run_id = ? AND catalog_hash_algorithm = ? AND catalog_hash = ? \
               AND source_state_hash_algorithm = ? AND source_state_hash = ? \
               AND resolver_policy_version = ?",
        )
        .bind(catalog_run_id.as_str())
        .bind(enum_text(&catalog_hash.algorithm))
        .bind(&catalog_hash.value)
        .bind(enum_text(&source_state_hash.algorithm))
        .bind(&source_state_hash.value)
        .bind(resolver_policy_version)
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_resolution_run).transpose()
    }

    pub(super) async fn list_manuscript_reference_resolution_entries(
        &self,
        resolution_run_id: &ManuscriptReferenceResolutionRunId,
    ) -> Result<Vec<ManuscriptReferenceResolutionEntry>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, resolution_run_id, reference_entry_id, outcome, match_kind, \
             chosen_source_id, chosen_source_snapshot_id, chosen_extraction_id, \
             automatic_binding_permitted, candidate_count \
             FROM research_manuscript_reference_resolution_entries \
             WHERE resolution_run_id = ? ORDER BY reference_entry_id ASC, id ASC",
        )
        .bind(resolution_run_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_resolution_entry).collect()
    }

    pub(super) async fn get_manuscript_reference_resolution_entry(
        &self,
        id: &ManuscriptReferenceResolutionEntryId,
    ) -> Result<Option<ManuscriptReferenceResolutionEntry>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, resolution_run_id, reference_entry_id, outcome, match_kind, \
             chosen_source_id, chosen_source_snapshot_id, chosen_extraction_id, \
             automatic_binding_permitted, candidate_count \
             FROM research_manuscript_reference_resolution_entries WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_resolution_entry).transpose()
    }

    pub(super) async fn list_manuscript_reference_resolution_candidates(
        &self,
        resolution_entry_id: &ManuscriptReferenceResolutionEntryId,
    ) -> Result<Vec<ManuscriptReferenceResolutionCandidate>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, resolution_entry_id, ordinal, source_id, source_snapshot_id, \
             extraction_id, match_kind, automatic_binding_permitted \
             FROM research_manuscript_reference_resolution_candidates \
             WHERE resolution_entry_id = ? ORDER BY ordinal ASC, id ASC",
        )
        .bind(resolution_entry_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_resolution_candidate).collect()
    }

    pub(super) async fn get_manuscript_reference_resolution_candidate(
        &self,
        id: &ManuscriptReferenceResolutionCandidateId,
    ) -> Result<Option<ManuscriptReferenceResolutionCandidate>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, resolution_entry_id, ordinal, source_id, source_snapshot_id, \
             extraction_id, match_kind, automatic_binding_permitted \
             FROM research_manuscript_reference_resolution_candidates WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_resolution_candidate).transpose()
    }

    pub(super) async fn persist_manuscript_reference_resolution(
        &self,
        value: &ManuscriptReferenceResolutionWrite,
    ) -> Result<ManuscriptReferenceResolutionRun, ResearchError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO research_manuscript_reference_resolution_runs \
             (id, research_case_id, catalog_run_id, catalog_hash_algorithm, catalog_hash, \
              source_state_hash_algorithm, source_state_hash, resolver_policy_version, status, \
              entry_count, resolved_entry_count, candidate_entry_count, unresolved_entry_count, \
              conflict_entry_count, created_at_ms, completed_at_ms, failure_code) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.run.id.as_str())
        .bind(value.run.research_case_id.as_str())
        .bind(value.run.catalog_run_id.as_str())
        .bind(enum_text(&value.run.catalog_hash.algorithm))
        .bind(&value.run.catalog_hash.value)
        .bind(enum_text(&value.run.source_state_hash.algorithm))
        .bind(&value.run.source_state_hash.value)
        .bind(&value.run.resolver_policy_version)
        .bind(enum_text(&value.run.status))
        .bind(value.run.entry_count)
        .bind(value.run.resolved_entry_count)
        .bind(value.run.candidate_entry_count)
        .bind(value.run.unresolved_entry_count)
        .bind(value.run.conflict_entry_count)
        .bind(value.run.created_at_ms)
        .bind(value.run.completed_at_ms)
        .bind(&value.run.failure_code)
        .execute(&mut *transaction)
        .await?;

        for entry in &value.entries {
            sqlx::query(
                "INSERT INTO research_manuscript_reference_resolution_entries \
                 (id, resolution_run_id, reference_entry_id, outcome, match_kind, \
                  chosen_source_id, chosen_source_snapshot_id, chosen_extraction_id, \
                  automatic_binding_permitted, candidate_count) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(entry.id.as_str())
            .bind(entry.resolution_run_id.as_str())
            .bind(entry.reference_entry_id.as_str())
            .bind(enum_text(&entry.outcome))
            .bind(entry.match_kind.as_ref().map(enum_text))
            .bind(entry.chosen_source_id.as_ref().map(|id| id.as_str()))
            .bind(
                entry
                    .chosen_source_snapshot_id
                    .as_ref()
                    .map(|id| id.as_str()),
            )
            .bind(entry.chosen_extraction_id.as_ref().map(|id| id.as_str()))
            .bind(if entry.automatic_binding_permitted {
                1
            } else {
                0
            })
            .bind(entry.candidate_count)
            .execute(&mut *transaction)
            .await?;
        }

        for candidate in &value.candidates {
            sqlx::query(
                "INSERT INTO research_manuscript_reference_resolution_candidates \
                 (id, resolution_entry_id, ordinal, source_id, source_snapshot_id, \
                  extraction_id, match_kind, automatic_binding_permitted) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(candidate.id.as_str())
            .bind(candidate.resolution_entry_id.as_str())
            .bind(candidate.ordinal)
            .bind(candidate.source_id.as_str())
            .bind(candidate.source_snapshot_id.as_ref().map(|id| id.as_str()))
            .bind(candidate.extraction_id.as_ref().map(|id| id.as_str()))
            .bind(enum_text(&candidate.match_kind))
            .bind(if candidate.automatic_binding_permitted {
                1
            } else {
                0
            })
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;
        Ok(value.run.clone())
    }
}

fn map_resolution_run(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptReferenceResolutionRun, ResearchError> {
    Ok(ManuscriptReferenceResolutionRun {
        id: ManuscriptReferenceResolutionRunId::parse(row.get::<String, _>("id"))?,
        research_case_id: crate::ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        catalog_run_id: ManuscriptReferenceCatalogRunId::parse(
            row.get::<String, _>("catalog_run_id"),
        )?,
        catalog_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("catalog_hash_algorithm")),
                "catalog hash algorithm",
            )?,
            value: row.get("catalog_hash"),
        },
        source_state_hash: ContentHash {
            algorithm: json_column(
                format!(
                    "\"{}\"",
                    row.get::<String, _>("source_state_hash_algorithm")
                ),
                "source state hash algorithm",
            )?,
            value: row.get("source_state_hash"),
        },
        resolver_policy_version: row.get("resolver_policy_version"),
        status: json_column(
            format!("\"{}\"", row.get::<String, _>("status")),
            "manuscript reference resolution status",
        )?,
        entry_count: row.get::<i64, _>("entry_count") as u32,
        resolved_entry_count: row.get::<i64, _>("resolved_entry_count") as u32,
        candidate_entry_count: row.get::<i64, _>("candidate_entry_count") as u32,
        unresolved_entry_count: row.get::<i64, _>("unresolved_entry_count") as u32,
        conflict_entry_count: row.get::<i64, _>("conflict_entry_count") as u32,
        created_at_ms: row.get("created_at_ms"),
        completed_at_ms: row.get("completed_at_ms"),
        failure_code: row.get("failure_code"),
    })
}

fn map_resolution_entry(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptReferenceResolutionEntry, ResearchError> {
    Ok(ManuscriptReferenceResolutionEntry {
        id: ManuscriptReferenceResolutionEntryId::parse(row.get::<String, _>("id"))?,
        resolution_run_id: ManuscriptReferenceResolutionRunId::parse(
            row.get::<String, _>("resolution_run_id"),
        )?,
        reference_entry_id: ManuscriptReferenceEntryId::parse(
            row.get::<String, _>("reference_entry_id"),
        )?,
        outcome: json_column(
            format!("\"{}\"", row.get::<String, _>("outcome")),
            "manuscript reference resolution outcome",
        )?,
        match_kind: row
            .get::<Option<String>, _>("match_kind")
            .map(|value| {
                json_column(
                    format!("\"{value}\""),
                    "manuscript reference resolution match kind",
                )
            })
            .transpose()?,
        chosen_source_id: row
            .get::<Option<String>, _>("chosen_source_id")
            .map(crate::ResearchSourceId::parse)
            .transpose()?,
        chosen_source_snapshot_id: row
            .get::<Option<String>, _>("chosen_source_snapshot_id")
            .map(crate::ResearchSourceSnapshotId::parse)
            .transpose()?,
        chosen_extraction_id: row
            .get::<Option<String>, _>("chosen_extraction_id")
            .map(crate::ResearchPdfExtractionId::parse)
            .transpose()?,
        automatic_binding_permitted: row.get::<i64, _>("automatic_binding_permitted") != 0,
        candidate_count: row.get::<i64, _>("candidate_count") as u32,
    })
}

fn map_resolution_candidate(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptReferenceResolutionCandidate, ResearchError> {
    Ok(ManuscriptReferenceResolutionCandidate {
        id: ManuscriptReferenceResolutionCandidateId::parse(row.get::<String, _>("id"))?,
        resolution_entry_id: ManuscriptReferenceResolutionEntryId::parse(
            row.get::<String, _>("resolution_entry_id"),
        )?,
        ordinal: row.get::<i64, _>("ordinal") as u32,
        source_id: crate::ResearchSourceId::parse(row.get::<String, _>("source_id"))?,
        source_snapshot_id: row
            .get::<Option<String>, _>("source_snapshot_id")
            .map(crate::ResearchSourceSnapshotId::parse)
            .transpose()?,
        extraction_id: row
            .get::<Option<String>, _>("extraction_id")
            .map(crate::ResearchPdfExtractionId::parse)
            .transpose()?,
        match_kind: json_column(
            format!("\"{}\"", row.get::<String, _>("match_kind")),
            "manuscript reference resolution match kind",
        )?,
        automatic_binding_permitted: row.get::<i64, _>("automatic_binding_permitted") != 0,
    })
}
