use sqlx::Row;

use crate::{
    CitationOccurrenceId, ClaimCitationLinkId, ContentHash, ManuscriptCitationSyncRunId,
    ManuscriptClaimExtractionCoverage, ManuscriptClaimExtractionCoverageId,
    ManuscriptClaimExtractionItem, ManuscriptClaimExtractionItemId, ManuscriptClaimExtractionRun,
    ManuscriptClaimExtractionRunId, ManuscriptClaimExtractionWrite, ResearchCaseId,
    ResearchClaimId, ResearchError, ResearchSourceId,
};

use super::{
    SqliteResearchRepository,
    common::{enum_text, json_column, json_text},
};

impl SqliteResearchRepository {
    pub(super) async fn get_manuscript_claim_extraction_run(
        &self,
        id: &ManuscriptClaimExtractionRunId,
    ) -> Result<Option<ManuscriptClaimExtractionRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
             document_id, document_version, context_hash_algorithm, context_hash, \
             extractor_provider, extractor_version, extractor_model_id, \
             extraction_contract_version, status, claim_count, created_at_ms, \
             completed_at_ms, failure_code \
             FROM research_manuscript_claim_extraction_runs WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_claim_extraction_run).transpose()
    }

    pub(super) async fn find_completed_manuscript_claim_extraction(
        &self,
        citation_sync_run_id: &ManuscriptCitationSyncRunId,
        context_hash: &ContentHash,
        extractor_provider: &str,
        extractor_version: &str,
        extractor_model_id: Option<&str>,
        extraction_contract_version: &str,
    ) -> Result<Option<ManuscriptClaimExtractionRun>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
             document_id, document_version, context_hash_algorithm, context_hash, \
             extractor_provider, extractor_version, extractor_model_id, \
             extraction_contract_version, status, claim_count, created_at_ms, \
             completed_at_ms, failure_code \
             FROM research_manuscript_claim_extraction_runs \
             WHERE citation_sync_run_id = ? AND context_hash_algorithm = ? \
             AND context_hash = ? AND extractor_provider = ? AND extractor_version = ? \
             AND extractor_model_id IS ? AND extraction_contract_version = ? \
             AND status = 'completed' ORDER BY created_at_ms DESC, id DESC LIMIT 1",
        )
        .bind(citation_sync_run_id.as_str())
        .bind(enum_text(&context_hash.algorithm))
        .bind(&context_hash.value)
        .bind(extractor_provider)
        .bind(extractor_version)
        .bind(extractor_model_id)
        .bind(extraction_contract_version)
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_manuscript_claim_extraction_run).transpose()
    }

    pub(super) async fn list_manuscript_claim_extraction_runs(
        &self,
        citation_sync_run_id: Option<&ManuscriptCitationSyncRunId>,
    ) -> Result<Vec<ManuscriptClaimExtractionRun>, ResearchError> {
        let (query, bind) = match citation_sync_run_id {
            Some(_) => (
                "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
                 document_id, document_version, context_hash_algorithm, context_hash, \
                 extractor_provider, extractor_version, extractor_model_id, \
                 extraction_contract_version, status, claim_count, created_at_ms, \
                 completed_at_ms, failure_code \
                 FROM research_manuscript_claim_extraction_runs \
                 WHERE citation_sync_run_id = ? ORDER BY created_at_ms ASC, id ASC",
                true,
            ),
            None => (
                "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
                 document_id, document_version, context_hash_algorithm, context_hash, \
                 extractor_provider, extractor_version, extractor_model_id, \
                 extraction_contract_version, status, claim_count, created_at_ms, \
                 completed_at_ms, failure_code \
                 FROM research_manuscript_claim_extraction_runs \
                 ORDER BY created_at_ms ASC, id ASC",
                false,
            ),
        };
        let mut query = sqlx::query(query);
        if bind {
            query = query.bind(citation_sync_run_id.expect("bound sync run ID").as_str());
        }
        query
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(map_manuscript_claim_extraction_run)
            .collect()
    }

    pub(super) async fn list_manuscript_claim_extraction_items(
        &self,
        extraction_run_id: &ManuscriptClaimExtractionRunId,
    ) -> Result<Vec<ManuscriptClaimExtractionItem>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, extraction_run_id, research_claim_id, document_block_id, \
             source_start, source_end, source_excerpt, source_excerpt_hash_algorithm, \
             source_excerpt_hash, ordinal \
             FROM research_manuscript_claim_extraction_items \
             WHERE extraction_run_id = ? ORDER BY ordinal ASC",
        )
        .bind(extraction_run_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(map_manuscript_claim_extraction_item)
            .collect()
    }

    pub(super) async fn list_manuscript_claim_extraction_coverage(
        &self,
        extraction_run_id: &ManuscriptClaimExtractionRunId,
    ) -> Result<Vec<ManuscriptClaimExtractionCoverage>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, extraction_run_id, extraction_item_id, claim_citation_link_id, \
             citation_occurrence_id, status, reason \
             FROM research_manuscript_claim_extraction_citations \
             WHERE extraction_run_id = ? ORDER BY citation_occurrence_id ASC",
        )
        .bind(extraction_run_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(map_manuscript_claim_extraction_coverage)
            .collect()
    }

    pub(super) async fn persist_manuscript_claim_extraction(
        &self,
        value: &ManuscriptClaimExtractionWrite,
    ) -> Result<ManuscriptClaimExtractionRun, ResearchError> {
        let mut transaction = self.pool.begin().await?;
        let existing = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
             document_id, document_version, context_hash_algorithm, context_hash, \
             extractor_provider, extractor_version, extractor_model_id, \
             extraction_contract_version, status, claim_count, created_at_ms, \
             completed_at_ms, failure_code \
             FROM research_manuscript_claim_extraction_runs WHERE id = ?",
        )
        .bind(value.run.id.as_str())
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(row) = existing {
            let existing = map_manuscript_claim_extraction_run(row)?;
            transaction.commit().await?;
            return Ok(existing);
        }

        let existing_identity = sqlx::query(
            "SELECT id, research_case_id, manuscript_source_id, citation_sync_run_id, \
             document_id, document_version, context_hash_algorithm, context_hash, \
             extractor_provider, extractor_version, extractor_model_id, \
             extraction_contract_version, status, claim_count, created_at_ms, \
             completed_at_ms, failure_code \
             FROM research_manuscript_claim_extraction_runs \
             WHERE citation_sync_run_id = ? AND context_hash_algorithm = ? \
             AND context_hash = ? AND extractor_provider = ? AND extractor_version = ? \
             AND extractor_model_id IS ? AND extraction_contract_version = ? \
             AND status = 'completed' ORDER BY created_at_ms DESC, id DESC LIMIT 1",
        )
        .bind(value.run.citation_sync_run_id.as_str())
        .bind(enum_text(&value.run.context_hash.algorithm))
        .bind(&value.run.context_hash.value)
        .bind(&value.run.extractor_provider)
        .bind(&value.run.extractor_version)
        .bind(value.run.extractor_model_id.as_deref())
        .bind(&value.run.extraction_contract_version)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(row) = existing_identity {
            let existing = map_manuscript_claim_extraction_run(row)?;
            transaction.commit().await?;
            return Ok(existing);
        }

        sqlx::query(
            "INSERT INTO research_manuscript_claim_extraction_runs \
             (id, research_case_id, manuscript_source_id, citation_sync_run_id, document_id, \
              document_version, context_hash_algorithm, context_hash, extractor_provider, \
              extractor_version, extractor_model_id, extraction_contract_version, status, \
              claim_count, created_at_ms, completed_at_ms, failure_code) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.run.id.as_str())
        .bind(value.run.research_case_id.as_str())
        .bind(value.run.manuscript_source_id.as_str())
        .bind(value.run.citation_sync_run_id.as_str())
        .bind(&value.run.document_id)
        .bind(value.run.document_version)
        .bind(enum_text(&value.run.context_hash.algorithm))
        .bind(&value.run.context_hash.value)
        .bind(&value.run.extractor_provider)
        .bind(&value.run.extractor_version)
        .bind(&value.run.extractor_model_id)
        .bind(&value.run.extraction_contract_version)
        .bind(enum_text(&value.run.status))
        .bind(value.run.claim_count)
        .bind(value.run.created_at_ms)
        .bind(value.run.completed_at_ms)
        .bind(&value.run.failure_code)
        .execute(&mut *transaction)
        .await?;

        for claim in &value.claims {
            sqlx::query(
                "INSERT INTO research_claims (id, research_case_id, text, origin_json, created_at_ms) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(claim.id.as_str())
            .bind(claim.research_case_id.as_str())
            .bind(&claim.text)
            .bind(json_text(&claim.origin)? )
            .bind(claim.created_at_ms)
            .execute(&mut *transaction)
            .await?;
        }
        for link in &value.links {
            sqlx::query(
                "INSERT INTO research_claim_citations \
                 (id, research_case_id, claim_id, citation_occurrence_id, created_at_ms) \
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(link.id.as_str())
            .bind(link.research_case_id.as_str())
            .bind(link.claim_id.as_str())
            .bind(link.citation_occurrence_id.as_str())
            .bind(link.created_at_ms)
            .execute(&mut *transaction)
            .await?;
        }
        for item in &value.items {
            sqlx::query(
                "INSERT INTO research_manuscript_claim_extraction_items \
                 (id, extraction_run_id, research_claim_id, document_block_id, source_start, \
                  source_end, source_excerpt, source_excerpt_hash_algorithm, source_excerpt_hash, ordinal) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(item.id.as_str())
            .bind(item.extraction_run_id.as_str())
            .bind(item.research_claim_id.as_str())
            .bind(&item.document_block_id)
            .bind(i64::try_from(item.source_start).map_err(|_| ResearchError::Invalid("claim source start exceeds SQLite range".to_owned()))?)
            .bind(i64::try_from(item.source_end).map_err(|_| ResearchError::Invalid("claim source end exceeds SQLite range".to_owned()))?)
            .bind(&item.source_excerpt)
            .bind(enum_text(&item.source_excerpt_hash.algorithm))
            .bind(&item.source_excerpt_hash.value)
            .bind(item.ordinal)
            .execute(&mut *transaction)
            .await?;
        }
        for coverage in &value.coverage {
            sqlx::query(
                "INSERT INTO research_manuscript_claim_extraction_citations \
                 (id, extraction_run_id, extraction_item_id, claim_citation_link_id, \
                  citation_occurrence_id, status, reason) VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(coverage.id.as_str())
            .bind(coverage.extraction_run_id.as_str())
            .bind(coverage.extraction_item_id.as_ref().map(|id| id.as_str()))
            .bind(
                coverage
                    .claim_citation_link_id
                    .as_ref()
                    .map(|id| id.as_str()),
            )
            .bind(coverage.citation_occurrence_id.as_str())
            .bind(enum_text(&coverage.status))
            .bind(&coverage.reason)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(value.run.clone())
    }
}

fn map_manuscript_claim_extraction_run(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptClaimExtractionRun, ResearchError> {
    Ok(ManuscriptClaimExtractionRun {
        id: ManuscriptClaimExtractionRunId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        manuscript_source_id: ResearchSourceId::parse(
            row.get::<String, _>("manuscript_source_id"),
        )?,
        citation_sync_run_id: ManuscriptCitationSyncRunId::parse(
            row.get::<String, _>("citation_sync_run_id"),
        )?,
        document_id: row.get("document_id"),
        document_version: row.get("document_version"),
        context_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("context_hash_algorithm")),
                "claim extraction context hash algorithm",
            )?,
            value: row.get("context_hash"),
        },
        extractor_provider: row.get("extractor_provider"),
        extractor_version: row.get("extractor_version"),
        extractor_model_id: row.get("extractor_model_id"),
        extraction_contract_version: row.get("extraction_contract_version"),
        status: json_column(
            format!("\"{}\"", row.get::<String, _>("status")),
            "manuscript claim extraction status",
        )?,
        claim_count: row.get::<i64, _>("claim_count") as u32,
        created_at_ms: row.get("created_at_ms"),
        completed_at_ms: row.get("completed_at_ms"),
        failure_code: row.get("failure_code"),
    })
}

fn map_manuscript_claim_extraction_item(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptClaimExtractionItem, ResearchError> {
    Ok(ManuscriptClaimExtractionItem {
        id: ManuscriptClaimExtractionItemId::parse(row.get::<String, _>("id"))?,
        extraction_run_id: ManuscriptClaimExtractionRunId::parse(
            row.get::<String, _>("extraction_run_id"),
        )?,
        research_claim_id: ResearchClaimId::parse(row.get::<String, _>("research_claim_id"))?,
        document_block_id: row.get("document_block_id"),
        source_start: row.get::<i64, _>("source_start") as u64,
        source_end: row.get::<i64, _>("source_end") as u64,
        source_excerpt: row.get("source_excerpt"),
        source_excerpt_hash: ContentHash {
            algorithm: json_column(
                format!(
                    "\"{}\"",
                    row.get::<String, _>("source_excerpt_hash_algorithm")
                ),
                "claim source excerpt hash algorithm",
            )?,
            value: row.get("source_excerpt_hash"),
        },
        ordinal: row.get::<i64, _>("ordinal") as u32,
    })
}

fn map_manuscript_claim_extraction_coverage(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptClaimExtractionCoverage, ResearchError> {
    Ok(ManuscriptClaimExtractionCoverage {
        id: ManuscriptClaimExtractionCoverageId::parse(row.get::<String, _>("id"))?,
        extraction_run_id: ManuscriptClaimExtractionRunId::parse(
            row.get::<String, _>("extraction_run_id"),
        )?,
        extraction_item_id: row
            .get::<Option<String>, _>("extraction_item_id")
            .map(ManuscriptClaimExtractionItemId::parse)
            .transpose()?,
        claim_citation_link_id: row
            .get::<Option<String>, _>("claim_citation_link_id")
            .map(ClaimCitationLinkId::parse)
            .transpose()?,
        citation_occurrence_id: CitationOccurrenceId::parse(
            row.get::<String, _>("citation_occurrence_id"),
        )?,
        status: json_column(
            format!("\"{}\"", row.get::<String, _>("status")),
            "manuscript claim extraction coverage status",
        )?,
        reason: row.get("reason"),
    })
}
