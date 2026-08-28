use sqlx::Row;

use crate::{
    ContentHash, ManuscriptClaimInventoryBlockKind, ManuscriptClaimInventoryCoverage,
    ManuscriptClaimInventoryCoverageId, ManuscriptClaimInventoryCoverageStatus,
    ManuscriptClaimInventoryItem, ManuscriptClaimInventoryItemId, ManuscriptClaimInventoryRun,
    ManuscriptClaimInventoryRunId, ManuscriptClaimInventoryStatus, ManuscriptClaimInventoryWrite,
    ResearchCaseId, ResearchError, ResearchSourceId,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column, json_text},
};

const COMPLETED_IDENTITY_INDEX: &str =
    "idx_research_manuscript_claim_inventory_runs_completed_identity";

impl SqliteResearchRepository {
    pub(super) async fn get_manuscript_claim_inventory_run(
        &self,
        id: &ManuscriptClaimInventoryRunId,
    ) -> Result<Option<ManuscriptClaimInventoryRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, document_id, document_version, \
             document_context_hash_algorithm, document_context_hash, extractor_provider, \
             extractor_version, extractor_model_id, extraction_contract_version, \
             coverage_contract_version, coverage_scope, coverage_limitations_json, status, \
             item_count, covered_block_count, created_at_ms, completed_at_ms, failure_code \
             FROM research_manuscript_claim_inventory_runs WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_claim_inventory_run).transpose()
    }

    pub(super) async fn find_completed_manuscript_claim_inventory(
        &self,
        research_case_id: &ResearchCaseId,
        manuscript_source_id: &ResearchSourceId,
        document_id: &str,
        document_version: i64,
        context_hash: &ContentHash,
        extractor_provider: &str,
        extractor_version: &str,
        extractor_model_id: Option<&str>,
        extraction_contract_version: &str,
        coverage_contract_version: &str,
    ) -> Result<Option<ManuscriptClaimInventoryRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, document_id, document_version, \
             document_context_hash_algorithm, document_context_hash, extractor_provider, \
             extractor_version, extractor_model_id, extraction_contract_version, \
             coverage_contract_version, coverage_scope, coverage_limitations_json, status, \
             item_count, covered_block_count, created_at_ms, completed_at_ms, failure_code \
             FROM research_manuscript_claim_inventory_runs \
             WHERE research_case_id = ? AND manuscript_source_id = ? AND document_id = ? \
             AND document_version = ? AND document_context_hash_algorithm = ? \
             AND document_context_hash = ? AND extractor_provider = ? AND extractor_version = ? \
              AND extractor_model_id IS ? AND extraction_contract_version = ? \
              AND coverage_contract_version = ? \
              AND status = 'completed' ORDER BY created_at_ms DESC, id DESC LIMIT 1",
        )
        .bind(research_case_id.as_str())
        .bind(manuscript_source_id.as_str())
        .bind(document_id)
        .bind(document_version)
        .bind(enum_text(&context_hash.algorithm))
        .bind(&context_hash.value)
        .bind(extractor_provider)
        .bind(extractor_version)
        .bind(extractor_model_id)
        .bind(extraction_contract_version)
        .bind(coverage_contract_version)
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_claim_inventory_run).transpose()
    }

    pub(super) async fn list_manuscript_claim_inventory_items(
        &self,
        inventory_run_id: &ManuscriptClaimInventoryRunId,
    ) -> Result<Vec<ManuscriptClaimInventoryItem>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, inventory_run_id, ordinal, document_block_id, block_ordinal, \
             block_kind, source_start, source_end, source_excerpt, source_excerpt_hash_algorithm, \
             source_excerpt_hash, claim_text, review_kind, overlapping_citation_count \
             FROM research_manuscript_claim_inventory_items \
             WHERE inventory_run_id = ? ORDER BY ordinal ASC",
        )
        .bind(inventory_run_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(map_manuscript_claim_inventory_item)
            .collect()
    }

    pub(super) async fn list_manuscript_claim_inventory_coverage(
        &self,
        inventory_run_id: &ManuscriptClaimInventoryRunId,
    ) -> Result<Vec<ManuscriptClaimInventoryCoverage>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, inventory_run_id, document_block_id, block_ordinal, block_kind, \
             status, reason FROM research_manuscript_claim_inventory_coverage \
             WHERE inventory_run_id = ? ORDER BY block_ordinal ASC",
        )
        .bind(inventory_run_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(map_manuscript_claim_inventory_coverage)
            .collect()
    }

    pub(super) async fn persist_manuscript_claim_inventory(
        &self,
        value: &ManuscriptClaimInventoryWrite,
    ) -> Result<ManuscriptClaimInventoryRun, ResearchError> {
        if matches!(&value.run.status, ManuscriptClaimInventoryStatus::Failed)
            && (value.run.item_count != 0
                || value.run.covered_block_count != 0
                || !value.items.is_empty()
                || !value.coverage.is_empty())
        {
            return Err(ResearchError::Invalid(
                "failed claim inventory runs must not persist items or coverage".to_owned(),
            ));
        }
        if matches!(&value.run.status, ManuscriptClaimInventoryStatus::Completed)
            && value.run.covered_block_count as usize != value.coverage.len()
        {
            return Err(ResearchError::Invalid(
                "completed claim inventory coverage count must match persisted coverage".to_owned(),
            ));
        }
        if let Some(existing) = self
            .get_manuscript_claim_inventory_run(&value.run.id)
            .await?
        {
            return Ok(existing);
        }
        let mut transaction = self.pool.begin().await?;
        let run_insert = sqlx::query(
            "INSERT INTO research_manuscript_claim_inventory_runs \
             (id, research_case_id, manuscript_source_id, document_id, document_version, \
              document_context_hash_algorithm, document_context_hash, extractor_provider, \
              extractor_version, extractor_model_id, extraction_contract_version, \
              coverage_contract_version, coverage_scope, coverage_limitations_json, status, \
              item_count, covered_block_count, created_at_ms, completed_at_ms, failure_code) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.run.id.as_str())
        .bind(value.run.research_case_id.as_str())
        .bind(value.run.manuscript_source_id.as_str())
        .bind(&value.run.document_id)
        .bind(value.run.document_version)
        .bind(enum_text(&value.run.document_context_hash.algorithm))
        .bind(&value.run.document_context_hash.value)
        .bind(&value.run.extractor_provider)
        .bind(&value.run.extractor_version)
        .bind(&value.run.extractor_model_id)
        .bind(&value.run.extraction_contract_version)
        .bind(&value.run.coverage_contract_version)
        .bind(&value.run.coverage_scope)
        .bind(json_text(&value.run.coverage_limitations)?)
        .bind(enum_text(&value.run.status))
        .bind(value.run.item_count)
        .bind(value.run.covered_block_count)
        .bind(value.run.created_at_ms)
        .bind(value.run.completed_at_ms)
        .bind(&value.run.failure_code)
        .execute(&mut *transaction)
        .await;
        if let Err(error) = run_insert {
            if is_completed_identity_unique_violation(&error) {
                drop(transaction);
                if let Some(existing) = self
                    .find_completed_manuscript_claim_inventory(
                        &value.run.research_case_id,
                        &value.run.manuscript_source_id,
                        &value.run.document_id,
                        value.run.document_version,
                        &value.run.document_context_hash,
                        &value.run.extractor_provider,
                        &value.run.extractor_version,
                        value.run.extractor_model_id.as_deref(),
                        &value.run.extraction_contract_version,
                        &value.run.coverage_contract_version,
                    )
                    .await?
                {
                    return Ok(existing);
                }
            }
            return Err(error.into());
        }

        for item in &value.items {
            sqlx::query(
                "INSERT INTO research_manuscript_claim_inventory_items \
                 (id, inventory_run_id, ordinal, document_block_id, block_ordinal, block_kind, \
                  source_start, source_end, source_excerpt, source_excerpt_hash_algorithm, \
                  source_excerpt_hash, claim_text, review_kind, overlapping_citation_count) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(item.id.as_str())
            .bind(item.inventory_run_id.as_str())
            .bind(item.ordinal)
            .bind(&item.document_block_id)
            .bind(item.block_ordinal)
            .bind(enum_text(&item.block_kind))
            .bind(i64::try_from(item.source_start).map_err(|_| {
                ResearchError::Invalid(
                    "claim inventory source start exceeds SQLite range".to_owned(),
                )
            })?)
            .bind(i64::try_from(item.source_end).map_err(|_| {
                ResearchError::Invalid("claim inventory source end exceeds SQLite range".to_owned())
            })?)
            .bind(&item.source_excerpt)
            .bind(enum_text(&item.source_excerpt_hash.algorithm))
            .bind(&item.source_excerpt_hash.value)
            .bind(&item.claim_text)
            .bind(enum_text(&item.review_kind))
            .bind(item.overlapping_citation_count)
            .execute(&mut *transaction)
            .await?;
        }

        for coverage in &value.coverage {
            sqlx::query(
                "INSERT INTO research_manuscript_claim_inventory_coverage \
                 (id, inventory_run_id, document_block_id, block_ordinal, block_kind, status, reason) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(coverage.id.as_str())
            .bind(coverage.inventory_run_id.as_str())
            .bind(&coverage.document_block_id)
            .bind(coverage.block_ordinal)
            .bind(enum_text(&coverage.block_kind))
            .bind(enum_text(&coverage.status))
            .bind(&coverage.reason)
            .execute(&mut *transaction)
            .await?;
        }

        transaction.commit().await?;
        Ok(value.run.clone())
    }
}

fn is_completed_identity_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(error)
            if error.code().as_deref() == Some("2067")
                && error.message().contains(COMPLETED_IDENTITY_INDEX)
    )
}

fn map_manuscript_claim_inventory_run(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptClaimInventoryRun, ResearchError> {
    Ok(ManuscriptClaimInventoryRun {
        id: ManuscriptClaimInventoryRunId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        manuscript_source_id: ResearchSourceId::parse(
            row.get::<String, _>("manuscript_source_id"),
        )?,
        document_id: row.get("document_id"),
        document_version: row.get("document_version"),
        document_context_hash: ContentHash {
            algorithm: json_column(
                format!(
                    "\"{}\"",
                    row.get::<String, _>("document_context_hash_algorithm")
                ),
                "claim inventory document context hash algorithm",
            )?,
            value: row.get("document_context_hash"),
        },
        extractor_provider: row.get("extractor_provider"),
        extractor_version: row.get("extractor_version"),
        extractor_model_id: row.get("extractor_model_id"),
        extraction_contract_version: row.get("extraction_contract_version"),
        coverage_contract_version: row.get("coverage_contract_version"),
        coverage_scope: row.get("coverage_scope"),
        coverage_limitations: json_column(
            row.get("coverage_limitations_json"),
            "claim inventory coverage limitations",
        )?,
        status: json_column(
            format!("\"{}\"", row.get::<String, _>("status")),
            "manuscript claim inventory status",
        )?,
        item_count: row.get::<i64, _>("item_count") as u32,
        covered_block_count: row.get::<i64, _>("covered_block_count") as u32,
        created_at_ms: row.get("created_at_ms"),
        completed_at_ms: row.get("completed_at_ms"),
        failure_code: row.get("failure_code"),
    })
}

fn map_manuscript_claim_inventory_item(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptClaimInventoryItem, ResearchError> {
    Ok(ManuscriptClaimInventoryItem {
        id: ManuscriptClaimInventoryItemId::parse(row.get::<String, _>("id"))?,
        inventory_run_id: ManuscriptClaimInventoryRunId::parse(
            row.get::<String, _>("inventory_run_id"),
        )?,
        ordinal: row.get::<i64, _>("ordinal") as u32,
        document_block_id: row.get("document_block_id"),
        block_ordinal: row.get::<i64, _>("block_ordinal") as u32,
        block_kind: json_column(
            format!("\"{}\"", row.get::<String, _>("block_kind")),
            "manuscript claim inventory block kind",
        )?,
        source_start: row.get::<i64, _>("source_start") as u64,
        source_end: row.get::<i64, _>("source_end") as u64,
        source_excerpt: row.get("source_excerpt"),
        source_excerpt_hash: ContentHash {
            algorithm: json_column(
                format!(
                    "\"{}\"",
                    row.get::<String, _>("source_excerpt_hash_algorithm")
                ),
                "claim inventory source excerpt hash algorithm",
            )?,
            value: row.get("source_excerpt_hash"),
        },
        claim_text: row.get("claim_text"),
        review_kind: json_column(
            format!("\"{}\"", row.get::<String, _>("review_kind")),
            "manuscript claim review kind",
        )?,
        overlapping_citation_count: row.get::<i64, _>("overlapping_citation_count") as u32,
    })
}

fn map_manuscript_claim_inventory_coverage(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptClaimInventoryCoverage, ResearchError> {
    Ok(ManuscriptClaimInventoryCoverage {
        id: ManuscriptClaimInventoryCoverageId::parse(row.get::<String, _>("id"))?,
        inventory_run_id: ManuscriptClaimInventoryRunId::parse(
            row.get::<String, _>("inventory_run_id"),
        )?,
        document_block_id: row.get("document_block_id"),
        block_ordinal: row.get::<i64, _>("block_ordinal") as u32,
        block_kind: json_column(
            format!("\"{}\"", row.get::<String, _>("block_kind")),
            "manuscript claim inventory coverage block kind",
        )?,
        status: json_column(
            format!("\"{}\"", row.get::<String, _>("status")),
            "manuscript claim inventory coverage status",
        )?,
        reason: row.get("reason"),
    })
}
