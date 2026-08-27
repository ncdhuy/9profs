use async_trait::async_trait;
use nineprofs_common::now_ms;
use serde::{Serialize, de::DeserializeOwned};
use sqlx::{Row, SqlitePool};

use crate::{
    CitationOccurrence, CitationOccurrenceId, CitationTarget, CitationTargetBinding,
    CitationTargetBindingId, CitationTargetId, ClaimCitationLink, ClaimCitationLinkId,
    ClaimEvidenceLink, ClaimEvidenceLinkId, ContentHash, ManuscriptCitationSyncOccurrence,
    ManuscriptCitationSyncOccurrenceId, ManuscriptCitationSyncRun, ManuscriptCitationSyncRunId,
    ManuscriptCitationSyncTarget, ManuscriptCitationSyncTargetId, ManuscriptCitationSyncWrite,
    ResearchCase, ResearchCaseId, ResearchClaim, ResearchClaimId, ResearchError, ResearchEvidence,
    ResearchEvidenceId, ResearchPdfExtraction, ResearchPdfExtractionId, ResearchPdfPage,
    ResearchSource, ResearchSourceId, ResearchSourceSnapshot, ResearchSourceSnapshotId,
};

#[async_trait]
pub trait ResearchRepository: Send + Sync {
    async fn list_cases(&self) -> Result<Vec<ResearchCase>, ResearchError>;
    async fn get_case(&self, id: &ResearchCaseId) -> Result<Option<ResearchCase>, ResearchError>;
    async fn insert_case(&self, value: &ResearchCase) -> Result<(), ResearchError>;

    async fn list_sources(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchSource>, ResearchError>;
    async fn get_source(
        &self,
        id: &ResearchSourceId,
    ) -> Result<Option<ResearchSource>, ResearchError>;
    async fn insert_source(&self, value: &ResearchSource) -> Result<(), ResearchError>;

    async fn list_snapshots(
        &self,
        source_id: Option<&ResearchSourceId>,
    ) -> Result<Vec<ResearchSourceSnapshot>, ResearchError>;
    async fn get_snapshot(
        &self,
        id: &ResearchSourceSnapshotId,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError>;
    async fn find_snapshot_by_hash(
        &self,
        source_id: &ResearchSourceId,
        content_hash: &ContentHash,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError>;
    async fn insert_snapshot(&self, value: &ResearchSourceSnapshot) -> Result<bool, ResearchError>;

    async fn get_pdf_extraction(
        &self,
        id: &ResearchPdfExtractionId,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError>;
    /// Returns the deterministic latest revision by `(extracted_at_ms DESC, id DESC)`.
    async fn latest_pdf_extraction(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError>;
    /// Returns all revisions in stable `(extracted_at_ms ASC, id ASC)` order.
    async fn list_pdf_extractions(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
    ) -> Result<Vec<ResearchPdfExtraction>, ResearchError>;
    async fn find_pdf_extraction(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
        extractor: &str,
        extractor_version: &str,
        extraction_hash: &ContentHash,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError>;
    async fn insert_pdf_extraction(
        &self,
        value: &ResearchPdfExtraction,
    ) -> Result<bool, ResearchError>;
    async fn insert_pdf_extraction_with_pages(
        &self,
        extraction: &ResearchPdfExtraction,
        pages: &[ResearchPdfPage],
    ) -> Result<bool, ResearchError>;
    /// Lists pages at or after the one-based `start_page` in stable page order.
    async fn list_pdf_pages(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        start_page: u32,
        limit: u32,
    ) -> Result<Vec<ResearchPdfPage>, ResearchError>;
    async fn get_pdf_page(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        page: u32,
    ) -> Result<Option<ResearchPdfPage>, ResearchError>;
    async fn insert_pdf_page(&self, value: &ResearchPdfPage) -> Result<(), ResearchError>;

    async fn list_evidence(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        source_snapshot_id: Option<&ResearchSourceSnapshotId>,
    ) -> Result<Vec<ResearchEvidence>, ResearchError>;
    async fn get_evidence(
        &self,
        id: &ResearchEvidenceId,
    ) -> Result<Option<ResearchEvidence>, ResearchError>;
    async fn insert_evidence(&self, value: &ResearchEvidence) -> Result<(), ResearchError>;

    async fn list_claims(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchClaim>, ResearchError>;
    async fn get_claim(&self, id: &ResearchClaimId)
    -> Result<Option<ResearchClaim>, ResearchError>;
    async fn insert_claim(&self, value: &ResearchClaim) -> Result<(), ResearchError>;

    async fn list_links(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        claim_id: Option<&ResearchClaimId>,
        evidence_id: Option<&ResearchEvidenceId>,
    ) -> Result<Vec<ClaimEvidenceLink>, ResearchError>;
    async fn get_link(
        &self,
        id: &ClaimEvidenceLinkId,
    ) -> Result<Option<ClaimEvidenceLink>, ResearchError>;
    async fn insert_link(&self, value: &ClaimEvidenceLink) -> Result<(), ResearchError>;

    async fn list_citation_occurrences(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<CitationOccurrence>, ResearchError>;
    async fn get_citation_occurrence(
        &self,
        id: &CitationOccurrenceId,
    ) -> Result<Option<CitationOccurrence>, ResearchError>;
    async fn insert_citation_occurrence(
        &self,
        value: &CitationOccurrence,
    ) -> Result<(), ResearchError>;

    async fn list_citation_targets(
        &self,
        citation_occurrence_id: &CitationOccurrenceId,
    ) -> Result<Vec<CitationTarget>, ResearchError>;
    async fn get_citation_target(
        &self,
        id: &CitationTargetId,
    ) -> Result<Option<CitationTarget>, ResearchError>;
    async fn insert_citation_target(&self, value: &CitationTarget) -> Result<(), ResearchError>;

    async fn get_manuscript_citation_sync(
        &self,
        id: &ManuscriptCitationSyncRunId,
    ) -> Result<Option<ManuscriptCitationSyncRun>, ResearchError>;
    async fn latest_manuscript_citation_sync(
        &self,
        research_case_id: &ResearchCaseId,
        manuscript_source_id: &ResearchSourceId,
    ) -> Result<Option<ManuscriptCitationSyncRun>, ResearchError>;
    async fn list_manuscript_citation_sync_occurrences(
        &self,
        sync_run_id: &ManuscriptCitationSyncRunId,
    ) -> Result<Vec<ManuscriptCitationSyncOccurrence>, ResearchError>;
    async fn get_manuscript_citation_sync_occurrence(
        &self,
        id: &ManuscriptCitationSyncOccurrenceId,
    ) -> Result<Option<ManuscriptCitationSyncOccurrence>, ResearchError>;
    async fn list_manuscript_citation_sync_targets(
        &self,
        sync_occurrence_id: &ManuscriptCitationSyncOccurrenceId,
    ) -> Result<Vec<ManuscriptCitationSyncTarget>, ResearchError>;
    async fn persist_manuscript_citation_sync(
        &self,
        value: &ManuscriptCitationSyncWrite,
    ) -> Result<ManuscriptCitationSyncRun, ResearchError>;

    async fn list_citation_target_bindings(
        &self,
        citation_target_id: &CitationTargetId,
    ) -> Result<Vec<CitationTargetBinding>, ResearchError>;
    async fn get_citation_target_binding(
        &self,
        id: &CitationTargetBindingId,
    ) -> Result<Option<CitationTargetBinding>, ResearchError>;
    async fn latest_citation_target_binding(
        &self,
        citation_target_id: &CitationTargetId,
    ) -> Result<Option<CitationTargetBinding>, ResearchError>;
    async fn insert_citation_target_binding(
        &self,
        value: &CitationTargetBinding,
    ) -> Result<(), ResearchError>;

    async fn list_claim_citation_links(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        claim_id: Option<&ResearchClaimId>,
        citation_occurrence_id: Option<&CitationOccurrenceId>,
    ) -> Result<Vec<ClaimCitationLink>, ResearchError>;
    async fn get_claim_citation_link(
        &self,
        id: &ClaimCitationLinkId,
    ) -> Result<Option<ClaimCitationLink>, ResearchError>;
    async fn find_claim_citation_link(
        &self,
        claim_id: &ResearchClaimId,
        citation_occurrence_id: &CitationOccurrenceId,
    ) -> Result<Option<ClaimCitationLink>, ResearchError>;
    async fn insert_claim_citation_link(
        &self,
        value: &ClaimCitationLink,
    ) -> Result<(), ResearchError>;
}

#[derive(Clone, Debug)]
pub struct SqliteResearchRepository {
    pool: SqlitePool,
}

impl SqliteResearchRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ResearchRepository for SqliteResearchRepository {
    async fn list_cases(&self) -> Result<Vec<ResearchCase>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, title, created_at_ms, updated_at_ms FROM research_cases ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_case).collect()
    }

    async fn get_case(&self, id: &ResearchCaseId) -> Result<Option<ResearchCase>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, title, created_at_ms, updated_at_ms FROM research_cases WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_case).transpose()
    }

    async fn insert_case(&self, value: &ResearchCase) -> Result<(), ResearchError> {
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

    async fn list_sources(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchSource>, ResearchError> {
        let rows = match research_case_id {
            Some(research_case_id) => {
                sqlx::query(
                    "SELECT id, research_case_id, kind, label, created_at_ms \
                     FROM research_sources WHERE research_case_id = ? ORDER BY id ASC",
                )
                .bind(research_case_id.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT id, research_case_id, kind, label, created_at_ms \
                     FROM research_sources ORDER BY id ASC",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(map_source).collect()
    }

    async fn get_source(
        &self,
        id: &ResearchSourceId,
    ) -> Result<Option<ResearchSource>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, kind, label, created_at_ms \
             FROM research_sources WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_source).transpose()
    }

    async fn insert_source(&self, value: &ResearchSource) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_sources (id, research_case_id, kind, label, created_at_ms) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(enum_text(&value.kind))
        .bind(&value.label)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_snapshots(
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

    async fn get_snapshot(
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

    async fn find_snapshot_by_hash(
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

    async fn insert_snapshot(&self, value: &ResearchSourceSnapshot) -> Result<bool, ResearchError> {
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

    async fn get_pdf_extraction(
        &self,
        id: &ResearchPdfExtractionId,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError> {
        let row = sqlx::query(&pdf_extraction_select(
            "WHERE id = ?",
            "extracted_at_ms ASC, id ASC",
        ))
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_pdf_extraction).transpose()
    }

    async fn latest_pdf_extraction(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError> {
        let row = sqlx::query(&pdf_extraction_select(
            "WHERE source_snapshot_id = ?",
            "extracted_at_ms DESC, id DESC",
        ))
        .bind(source_snapshot_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_pdf_extraction).transpose()
    }

    async fn list_pdf_extractions(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
    ) -> Result<Vec<ResearchPdfExtraction>, ResearchError> {
        let rows = sqlx::query(&pdf_extraction_select(
            "WHERE source_snapshot_id = ?",
            "extracted_at_ms ASC, id ASC",
        ))
        .bind(source_snapshot_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_pdf_extraction).collect()
    }

    async fn find_pdf_extraction(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
        extractor: &str,
        extractor_version: &str,
        extraction_hash: &ContentHash,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError> {
        let row = sqlx::query(&pdf_extraction_select(
            "WHERE source_snapshot_id = ? AND extractor = ? AND extractor_version = ? \
             AND hash_algorithm = ? AND extraction_hash = ?",
            "extracted_at_ms ASC, id ASC",
        ))
        .bind(source_snapshot_id.as_str())
        .bind(extractor)
        .bind(extractor_version)
        .bind(enum_text(&extraction_hash.algorithm))
        .bind(&extraction_hash.value)
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_pdf_extraction).transpose()
    }

    async fn insert_pdf_extraction(
        &self,
        value: &ResearchPdfExtraction,
    ) -> Result<bool, ResearchError> {
        let result = sqlx::query(
            "INSERT INTO research_pdf_extractions \
             (id, source_snapshot_id, artifact_id, extractor, extractor_version, page_count, \
              hash_algorithm, extraction_hash, extracted_at_ms, status) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(source_snapshot_id, extractor, extractor_version, extraction_hash) DO NOTHING",
        )
        .bind(value.id.as_str())
        .bind(value.source_snapshot_id.as_str())
        .bind(&value.artifact_id)
        .bind(&value.extractor)
        .bind(&value.extractor_version)
        .bind(value.page_count as i64)
        .bind(enum_text(&value.extraction_hash.algorithm))
        .bind(&value.extraction_hash.value)
        .bind(value.extracted_at_ms)
        .bind(enum_text(&value.status))
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn insert_pdf_extraction_with_pages(
        &self,
        extraction: &ResearchPdfExtraction,
        pages: &[ResearchPdfPage],
    ) -> Result<bool, ResearchError> {
        let mut transaction = self.pool.begin().await?;
        let result = sqlx::query(
            "INSERT INTO research_pdf_extractions \
             (id, source_snapshot_id, artifact_id, extractor, extractor_version, page_count, \
              hash_algorithm, extraction_hash, extracted_at_ms, status) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(source_snapshot_id, extractor, extractor_version, extraction_hash) DO NOTHING",
        )
        .bind(extraction.id.as_str())
        .bind(extraction.source_snapshot_id.as_str())
        .bind(&extraction.artifact_id)
        .bind(&extraction.extractor)
        .bind(&extraction.extractor_version)
        .bind(extraction.page_count as i64)
        .bind(enum_text(&extraction.extraction_hash.algorithm))
        .bind(&extraction.extraction_hash.value)
        .bind(extraction.extracted_at_ms)
        .bind(enum_text(&extraction.status))
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() == 1 {
            for page in pages {
                sqlx::query(
                    "INSERT INTO research_pdf_pages \
                     (extraction_id, page, text, hash_algorithm, text_hash) VALUES (?, ?, ?, ?, ?)",
                )
                .bind(page.extraction_id.as_str())
                .bind(page.page as i64)
                .bind(&page.text)
                .bind(enum_text(&page.text_hash.algorithm))
                .bind(&page.text_hash.value)
                .execute(&mut *transaction)
                .await?;
            }
        }
        transaction.commit().await?;
        Ok(result.rows_affected() == 1)
    }

    async fn list_pdf_pages(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        start_page: u32,
        limit: u32,
    ) -> Result<Vec<ResearchPdfPage>, ResearchError> {
        let rows = sqlx::query(
            "SELECT extraction_id, page, text, hash_algorithm, text_hash \
             FROM research_pdf_pages WHERE extraction_id = ? AND page >= ? \
             ORDER BY page ASC LIMIT ?",
        )
        .bind(extraction_id.as_str())
        .bind(start_page as i64)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_pdf_page).collect()
    }

    async fn get_pdf_page(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        page: u32,
    ) -> Result<Option<ResearchPdfPage>, ResearchError> {
        let row = sqlx::query(
            "SELECT extraction_id, page, text, hash_algorithm, text_hash \
             FROM research_pdf_pages WHERE extraction_id = ? AND page = ?",
        )
        .bind(extraction_id.as_str())
        .bind(page as i64)
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_pdf_page).transpose()
    }

    async fn insert_pdf_page(&self, value: &ResearchPdfPage) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_pdf_pages \
             (extraction_id, page, text, hash_algorithm, text_hash) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.extraction_id.as_str())
        .bind(value.page as i64)
        .bind(&value.text)
        .bind(enum_text(&value.text_hash.algorithm))
        .bind(&value.text_hash.value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_evidence(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        source_snapshot_id: Option<&ResearchSourceSnapshotId>,
    ) -> Result<Vec<ResearchEvidence>, ResearchError> {
        let (query, binds) = match (research_case_id, source_snapshot_id) {
            (Some(case_id), Some(snapshot_id)) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                  locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
                 FROM research_evidence WHERE research_case_id = ? AND source_snapshot_id = ? ORDER BY id ASC",
                Some((case_id.as_str().to_owned(), snapshot_id.as_str().to_owned())),
            ),
            (Some(case_id), None) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                  locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
                 FROM research_evidence WHERE research_case_id = ? ORDER BY id ASC",
                Some((case_id.as_str().to_owned(), String::new())),
            ),
            (None, Some(snapshot_id)) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                  locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
                 FROM research_evidence WHERE source_snapshot_id = ? ORDER BY id ASC",
                Some((snapshot_id.as_str().to_owned(), String::new())),
            ),
            (None, None) => (
                "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
                  locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
                 FROM research_evidence ORDER BY id ASC",
                None,
            ),
        };
        let mut query = sqlx::query(query);
        if let Some((first, second)) = binds {
            query = query.bind(first);
            if research_case_id.is_some() && source_snapshot_id.is_some() {
                query = query.bind(second);
            }
        }
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_evidence).collect()
    }

    async fn get_evidence(
        &self,
        id: &ResearchEvidenceId,
    ) -> Result<Option<ResearchEvidence>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, \
             locator_json, hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id \
             FROM research_evidence WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_evidence).transpose()
    }

    async fn insert_evidence(&self, value: &ResearchEvidence) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_evidence \
             (id, research_case_id, source_snapshot_id, verbatim_excerpt, normalized_text, locator_json, \
              hash_algorithm, excerpt_hash, captured_at_ms, capture_method, pdf_extraction_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(value.source_snapshot_id.as_str())
        .bind(&value.verbatim_excerpt)
        .bind(&value.normalized_text)
        .bind(json_text(&value.locator)?)
        .bind(enum_text(&value.excerpt_hash.algorithm))
        .bind(&value.excerpt_hash.value)
        .bind(value.captured_at_ms)
        .bind(enum_text(&value.capture_method))
        .bind(value.pdf_extraction_id.as_ref().map(|id| id.as_str()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_claims(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchClaim>, ResearchError> {
        let rows = match research_case_id {
            Some(case_id) => {
                sqlx::query(
                    "SELECT id, research_case_id, text, origin_json, created_at_ms \
                 FROM research_claims WHERE research_case_id = ? ORDER BY id ASC",
                )
                .bind(case_id.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT id, research_case_id, text, origin_json, created_at_ms \
                 FROM research_claims ORDER BY id ASC",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(map_claim).collect()
    }

    async fn get_claim(
        &self,
        id: &ResearchClaimId,
    ) -> Result<Option<ResearchClaim>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, text, origin_json, created_at_ms \
             FROM research_claims WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_claim).transpose()
    }

    async fn insert_claim(&self, value: &ResearchClaim) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_claims (id, research_case_id, text, origin_json, created_at_ms) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(&value.text)
        .bind(json_text(&value.origin)?)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_links(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        claim_id: Option<&ResearchClaimId>,
        evidence_id: Option<&ResearchEvidenceId>,
    ) -> Result<Vec<ClaimEvidenceLink>, ResearchError> {
        let mut conditions = Vec::new();
        if research_case_id.is_some() {
            conditions.push("research_case_id = ?");
        }
        if claim_id.is_some() {
            conditions.push("claim_id = ?");
        }
        if evidence_id.is_some() {
            conditions.push("evidence_id = ?");
        }
        let suffix = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        let query = format!(
            "SELECT id, research_case_id, claim_id, evidence_id, relation, rationale, \
             assessment_method, assessment_metadata_json, created_at_ms \
             FROM research_claim_evidence{suffix} ORDER BY id ASC"
        );
        let mut query = sqlx::query(&query);
        for value in [
            research_case_id.map(|value| value.as_str()),
            claim_id.map(|value| value.as_str()),
            evidence_id.map(|value| value.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            query = query.bind(value);
        }
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_link).collect()
    }

    async fn get_link(
        &self,
        id: &ClaimEvidenceLinkId,
    ) -> Result<Option<ClaimEvidenceLink>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, claim_id, evidence_id, relation, rationale, \
             assessment_method, assessment_metadata_json, created_at_ms \
             FROM research_claim_evidence WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_link).transpose()
    }

    async fn insert_link(&self, value: &ClaimEvidenceLink) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_claim_evidence \
             (id, research_case_id, claim_id, evidence_id, relation, rationale, assessment_method, \
              assessment_metadata_json, created_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(value.claim_id.as_str())
        .bind(value.evidence_id.as_str())
        .bind(enum_text(&value.relation))
        .bind(&value.rationale)
        .bind(enum_text(&value.assessment_method))
        .bind(json_text(&value.assessment_metadata)?)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_citation_occurrences(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<CitationOccurrence>, ResearchError> {
        let rows = match research_case_id {
            Some(case_id) => {
                sqlx::query(
                    "SELECT id, research_case_id, origin_json, rendered_text, created_at_ms \
                     FROM research_citation_occurrences WHERE research_case_id = ? ORDER BY id ASC",
                )
                .bind(case_id.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT id, research_case_id, origin_json, rendered_text, created_at_ms \
                     FROM research_citation_occurrences ORDER BY id ASC",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        rows.into_iter().map(map_citation_occurrence).collect()
    }

    async fn get_citation_occurrence(
        &self,
        id: &CitationOccurrenceId,
    ) -> Result<Option<CitationOccurrence>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, origin_json, rendered_text, created_at_ms \
             FROM research_citation_occurrences WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_citation_occurrence).transpose()
    }

    async fn insert_citation_occurrence(
        &self,
        value: &CitationOccurrence,
    ) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_citation_occurrences \
             (id, research_case_id, origin_json, rendered_text, created_at_ms) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(json_text(&value.origin)?)
        .bind(&value.rendered_text)
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_citation_targets(
        &self,
        citation_occurrence_id: &CitationOccurrenceId,
    ) -> Result<Vec<CitationTarget>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, citation_occurrence_id, ordinal, reference_key, cited_locator \
             FROM research_citation_targets WHERE citation_occurrence_id = ? ORDER BY ordinal ASC",
        )
        .bind(citation_occurrence_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_citation_target).collect()
    }

    async fn get_citation_target(
        &self,
        id: &CitationTargetId,
    ) -> Result<Option<CitationTarget>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, citation_occurrence_id, ordinal, reference_key, cited_locator \
             FROM research_citation_targets WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_citation_target).transpose()
    }

    async fn insert_citation_target(&self, value: &CitationTarget) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_citation_targets \
             (id, citation_occurrence_id, ordinal, reference_key, cited_locator) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.citation_occurrence_id.as_str())
        .bind(value.ordinal)
        .bind(&value.reference_key)
        .bind(&value.cited_locator)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_manuscript_citation_sync(
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

    async fn latest_manuscript_citation_sync(
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

    async fn list_manuscript_citation_sync_occurrences(
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

    async fn get_manuscript_citation_sync_occurrence(
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

    async fn list_manuscript_citation_sync_targets(
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

    async fn persist_manuscript_citation_sync(
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

    async fn list_citation_target_bindings(
        &self,
        citation_target_id: &CitationTargetId,
    ) -> Result<Vec<CitationTargetBinding>, ResearchError> {
        let rows = sqlx::query(
            "SELECT id, research_case_id, citation_target_id, source_id, source_snapshot_id, \
             extraction_id, method, created_at_ms FROM research_citation_target_bindings \
             WHERE citation_target_id = ? ORDER BY created_at_ms ASC, id ASC",
        )
        .bind(citation_target_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(map_citation_target_binding).collect()
    }

    async fn get_citation_target_binding(
        &self,
        id: &CitationTargetBindingId,
    ) -> Result<Option<CitationTargetBinding>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, citation_target_id, source_id, source_snapshot_id, \
             extraction_id, method, created_at_ms FROM research_citation_target_bindings WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_citation_target_binding).transpose()
    }

    async fn latest_citation_target_binding(
        &self,
        citation_target_id: &CitationTargetId,
    ) -> Result<Option<CitationTargetBinding>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, citation_target_id, source_id, source_snapshot_id, \
             extraction_id, method, created_at_ms FROM research_citation_target_bindings \
             WHERE citation_target_id = ? ORDER BY created_at_ms DESC, id DESC LIMIT 1",
        )
        .bind(citation_target_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_citation_target_binding).transpose()
    }

    async fn insert_citation_target_binding(
        &self,
        value: &CitationTargetBinding,
    ) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_citation_target_bindings \
             (id, research_case_id, citation_target_id, source_id, source_snapshot_id, extraction_id, method, created_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(value.citation_target_id.as_str())
        .bind(value.source_id.as_str())
        .bind(value.source_snapshot_id.as_ref().map(|id| id.as_str()))
        .bind(value.extraction_id.as_ref().map(|id| id.as_str()))
        .bind(enum_text(&value.method))
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_claim_citation_links(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        claim_id: Option<&ResearchClaimId>,
        citation_occurrence_id: Option<&CitationOccurrenceId>,
    ) -> Result<Vec<ClaimCitationLink>, ResearchError> {
        let mut conditions = Vec::new();
        if research_case_id.is_some() {
            conditions.push("research_case_id = ?");
        }
        if claim_id.is_some() {
            conditions.push("claim_id = ?");
        }
        if citation_occurrence_id.is_some() {
            conditions.push("citation_occurrence_id = ?");
        }
        let suffix = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        let query = format!(
            "SELECT id, research_case_id, claim_id, citation_occurrence_id, created_at_ms \
             FROM research_claim_citations{suffix} ORDER BY created_at_ms ASC, id ASC"
        );
        let mut query = sqlx::query(&query);
        for value in [
            research_case_id.map(|value| value.as_str()),
            claim_id.map(|value| value.as_str()),
            citation_occurrence_id.map(|value| value.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            query = query.bind(value);
        }
        let rows = query.fetch_all(&self.pool).await?;
        rows.into_iter().map(map_claim_citation_link).collect()
    }

    async fn get_claim_citation_link(
        &self,
        id: &ClaimCitationLinkId,
    ) -> Result<Option<ClaimCitationLink>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, claim_id, citation_occurrence_id, created_at_ms \
             FROM research_claim_citations WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_claim_citation_link).transpose()
    }

    async fn find_claim_citation_link(
        &self,
        claim_id: &ResearchClaimId,
        citation_occurrence_id: &CitationOccurrenceId,
    ) -> Result<Option<ClaimCitationLink>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, claim_id, citation_occurrence_id, created_at_ms \
             FROM research_claim_citations WHERE claim_id = ? AND citation_occurrence_id = ?",
        )
        .bind(claim_id.as_str())
        .bind(citation_occurrence_id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.map(map_claim_citation_link).transpose()
    }

    async fn insert_claim_citation_link(
        &self,
        value: &ClaimCitationLink,
    ) -> Result<(), ResearchError> {
        sqlx::query(
            "INSERT INTO research_claim_citations \
             (id, research_case_id, claim_id, citation_occurrence_id, created_at_ms) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(value.id.as_str())
        .bind(value.research_case_id.as_str())
        .bind(value.claim_id.as_str())
        .bind(value.citation_occurrence_id.as_str())
        .bind(value.created_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn snapshot_select(where_clause: &str) -> String {
    format!(
        "SELECT id, source_id, hash_algorithm, content_hash, captured_at_ms, capture_method, \
         origin_json, metadata_json FROM research_source_snapshots {where_clause} ORDER BY id ASC"
    )
}

fn enum_text<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .expect("research enum serialization cannot fail")
        .trim_matches('"')
        .to_owned()
}

fn json_text<T: Serialize>(value: &T) -> Result<String, ResearchError> {
    Ok(serde_json::to_string(value)?)
}

fn json_column<T: DeserializeOwned>(value: String, field: &str) -> Result<T, ResearchError> {
    serde_json::from_str(&value)
        .map_err(|error| ResearchError::Invalid(format!("invalid persisted {field}: {error}")))
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

fn map_evidence(row: sqlx::sqlite::SqliteRow) -> Result<ResearchEvidence, ResearchError> {
    Ok(ResearchEvidence {
        id: ResearchEvidenceId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        source_snapshot_id: ResearchSourceSnapshotId::parse(
            row.get::<String, _>("source_snapshot_id"),
        )?,
        verbatim_excerpt: row.get("verbatim_excerpt"),
        normalized_text: row.get("normalized_text"),
        locator: json_column(row.get("locator_json"), "evidence locator")?,
        excerpt_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("hash_algorithm")),
                "evidence hash algorithm",
            )?,
            value: row.get("excerpt_hash"),
        },
        captured_at_ms: row.get("captured_at_ms"),
        capture_method: json_column(
            format!("\"{}\"", row.get::<String, _>("capture_method")),
            "evidence capture method",
        )?,
        pdf_extraction_id: row
            .get::<Option<String>, _>("pdf_extraction_id")
            .map(ResearchPdfExtractionId::parse)
            .transpose()?,
    })
}

fn pdf_extraction_select(where_clause: &str, order_by: &str) -> String {
    format!(
        "SELECT id, source_snapshot_id, artifact_id, extractor, extractor_version, page_count, \
         hash_algorithm, extraction_hash, extracted_at_ms, status \
         FROM research_pdf_extractions {where_clause} ORDER BY {order_by}"
    )
}

fn map_pdf_extraction(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ResearchPdfExtraction, ResearchError> {
    Ok(ResearchPdfExtraction {
        id: ResearchPdfExtractionId::parse(row.get::<String, _>("id"))?,
        source_snapshot_id: ResearchSourceSnapshotId::parse(
            row.get::<String, _>("source_snapshot_id"),
        )?,
        artifact_id: row.get("artifact_id"),
        extractor: row.get("extractor"),
        extractor_version: row.get("extractor_version"),
        page_count: row.get::<i64, _>("page_count") as u32,
        extraction_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("hash_algorithm")),
                "PDF extraction hash algorithm",
            )?,
            value: row.get("extraction_hash"),
        },
        extracted_at_ms: row.get("extracted_at_ms"),
        status: json_column(
            format!("\"{}\"", row.get::<String, _>("status")),
            "PDF extraction status",
        )?,
    })
}

fn map_pdf_page(row: sqlx::sqlite::SqliteRow) -> Result<ResearchPdfPage, ResearchError> {
    Ok(ResearchPdfPage {
        extraction_id: ResearchPdfExtractionId::parse(row.get::<String, _>("extraction_id"))?,
        page: row.get::<i64, _>("page") as u32,
        text: row.get("text"),
        text_hash: ContentHash {
            algorithm: json_column(
                format!("\"{}\"", row.get::<String, _>("hash_algorithm")),
                "PDF page hash algorithm",
            )?,
            value: row.get("text_hash"),
        },
    })
}

fn map_claim(row: sqlx::sqlite::SqliteRow) -> Result<ResearchClaim, ResearchError> {
    Ok(ResearchClaim {
        id: ResearchClaimId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        text: row.get("text"),
        origin: json_column(row.get("origin_json"), "claim origin")?,
        created_at_ms: row.get("created_at_ms"),
    })
}

fn map_link(row: sqlx::sqlite::SqliteRow) -> Result<ClaimEvidenceLink, ResearchError> {
    Ok(ClaimEvidenceLink {
        id: ClaimEvidenceLinkId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        claim_id: ResearchClaimId::parse(row.get::<String, _>("claim_id"))?,
        evidence_id: ResearchEvidenceId::parse(row.get::<String, _>("evidence_id"))?,
        relation: json_column(
            format!("\"{}\"", row.get::<String, _>("relation")),
            "claim-evidence relation",
        )?,
        rationale: row.get("rationale"),
        assessment_method: json_column(
            format!("\"{}\"", row.get::<String, _>("assessment_method")),
            "assessment method",
        )?,
        assessment_metadata: json_column(
            row.get("assessment_metadata_json"),
            "assessment metadata",
        )?,
        created_at_ms: row.get("created_at_ms"),
    })
}

fn map_citation_occurrence(
    row: sqlx::sqlite::SqliteRow,
) -> Result<CitationOccurrence, ResearchError> {
    Ok(CitationOccurrence {
        id: CitationOccurrenceId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        origin: json_column(row.get("origin_json"), "citation occurrence origin")?,
        rendered_text: row.get("rendered_text"),
        created_at_ms: row.get("created_at_ms"),
    })
}

fn map_citation_target(row: sqlx::sqlite::SqliteRow) -> Result<CitationTarget, ResearchError> {
    Ok(CitationTarget {
        id: CitationTargetId::parse(row.get::<String, _>("id"))?,
        citation_occurrence_id: CitationOccurrenceId::parse(
            row.get::<String, _>("citation_occurrence_id"),
        )?,
        ordinal: row.get::<i64, _>("ordinal") as u32,
        reference_key: row.get("reference_key"),
        cited_locator: row.get("cited_locator"),
    })
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

fn map_citation_target_binding(
    row: sqlx::sqlite::SqliteRow,
) -> Result<CitationTargetBinding, ResearchError> {
    Ok(CitationTargetBinding {
        id: CitationTargetBindingId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        citation_target_id: CitationTargetId::parse(row.get::<String, _>("citation_target_id"))?,
        source_id: ResearchSourceId::parse(row.get::<String, _>("source_id"))?,
        source_snapshot_id: row
            .get::<Option<String>, _>("source_snapshot_id")
            .map(ResearchSourceSnapshotId::parse)
            .transpose()?,
        extraction_id: row
            .get::<Option<String>, _>("extraction_id")
            .map(ResearchPdfExtractionId::parse)
            .transpose()?,
        method: json_column(
            format!("\"{}\"", row.get::<String, _>("method")),
            "citation binding method",
        )?,
        created_at_ms: row.get("created_at_ms"),
    })
}

fn map_claim_citation_link(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ClaimCitationLink, ResearchError> {
    Ok(ClaimCitationLink {
        id: ClaimCitationLinkId::parse(row.get::<String, _>("id"))?,
        research_case_id: ResearchCaseId::parse(row.get::<String, _>("research_case_id"))?,
        claim_id: ResearchClaimId::parse(row.get::<String, _>("claim_id"))?,
        citation_occurrence_id: CitationOccurrenceId::parse(
            row.get::<String, _>("citation_occurrence_id"),
        )?,
        created_at_ms: row.get("created_at_ms"),
    })
}

#[allow(dead_code)]
fn _now_for_repository_tests() -> i64 {
    now_ms()
}
