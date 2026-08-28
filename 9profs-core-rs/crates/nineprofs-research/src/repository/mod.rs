use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::{
    CitationOccurrence, CitationOccurrenceId, CitationTarget, CitationTargetBinding,
    CitationTargetBindingId, CitationTargetId, ClaimCitationLink, ClaimCitationLinkId,
    ClaimEvidenceLink, ClaimEvidenceLinkId, ContentHash, ManuscriptCitationSyncOccurrence,
    ManuscriptCitationSyncOccurrenceId, ManuscriptCitationSyncRun, ManuscriptCitationSyncRunId,
    ManuscriptCitationSyncTarget, ManuscriptCitationSyncWrite, ManuscriptClaimExtractionCoverage,
    ManuscriptClaimExtractionItem, ManuscriptClaimExtractionRun, ManuscriptClaimExtractionRunId,
    ManuscriptClaimExtractionWrite, ManuscriptReferenceCatalogRun, ManuscriptReferenceCatalogRunId,
    ManuscriptReferenceCatalogWrite, ManuscriptReferenceEntry, ManuscriptReferenceEntryId,
    ManuscriptReferenceResolutionCandidate, ManuscriptReferenceResolutionCandidateId,
    ManuscriptReferenceResolutionEntry, ManuscriptReferenceResolutionEntryId,
    ManuscriptReferenceResolutionRun, ManuscriptReferenceResolutionRunId,
    ManuscriptReferenceResolutionWrite, ManuscriptReferenceTargetMapping, ResearchCase,
    ResearchCaseId, ResearchClaim, ResearchClaimId, ResearchError, ResearchEvidence,
    ResearchEvidenceId, ResearchPdfExtraction, ResearchPdfExtractionId, ResearchPdfPage,
    ResearchSource, ResearchSourceId, ResearchSourceSnapshot, ResearchSourceSnapshotId,
};

mod case_source_snapshot;
mod citation;
mod common;
mod evidence_claims;
mod manuscript_citation_sync;
mod manuscript_claim_extraction;
mod pdf;
mod reference_catalog;
mod reference_resolution;

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

    async fn get_manuscript_reference_catalog_run(
        &self,
        id: &ManuscriptReferenceCatalogRunId,
    ) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError>;
    async fn get_manuscript_reference_catalog_for_sync(
        &self,
        citation_sync_run_id: &ManuscriptCitationSyncRunId,
    ) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError>;
    async fn latest_manuscript_reference_catalog(
        &self,
        research_case_id: &ResearchCaseId,
        manuscript_source_id: &ResearchSourceId,
    ) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError>;
    async fn list_manuscript_reference_entries(
        &self,
        catalog_run_id: &ManuscriptReferenceCatalogRunId,
    ) -> Result<Vec<ManuscriptReferenceEntry>, ResearchError>;
    async fn get_manuscript_reference_entry(
        &self,
        id: &ManuscriptReferenceEntryId,
    ) -> Result<Option<ManuscriptReferenceEntry>, ResearchError>;
    async fn list_manuscript_reference_target_mappings(
        &self,
        reference_entry_id: &ManuscriptReferenceEntryId,
    ) -> Result<Vec<ManuscriptReferenceTargetMapping>, ResearchError>;
    async fn persist_manuscript_reference_catalog(
        &self,
        value: &ManuscriptReferenceCatalogWrite,
    ) -> Result<ManuscriptReferenceCatalogRun, ResearchError>;

    async fn get_manuscript_reference_resolution_run(
        &self,
        id: &ManuscriptReferenceResolutionRunId,
    ) -> Result<Option<ManuscriptReferenceResolutionRun>, ResearchError>;
    async fn get_manuscript_reference_resolution_for_catalog(
        &self,
        catalog_run_id: &ManuscriptReferenceCatalogRunId,
        catalog_hash: &ContentHash,
        source_state_hash: &ContentHash,
        resolver_policy_version: &str,
    ) -> Result<Option<ManuscriptReferenceResolutionRun>, ResearchError>;
    async fn list_manuscript_reference_resolution_entries(
        &self,
        resolution_run_id: &ManuscriptReferenceResolutionRunId,
    ) -> Result<Vec<ManuscriptReferenceResolutionEntry>, ResearchError>;
    async fn get_manuscript_reference_resolution_entry(
        &self,
        id: &ManuscriptReferenceResolutionEntryId,
    ) -> Result<Option<ManuscriptReferenceResolutionEntry>, ResearchError>;
    async fn list_manuscript_reference_resolution_candidates(
        &self,
        resolution_entry_id: &ManuscriptReferenceResolutionEntryId,
    ) -> Result<Vec<ManuscriptReferenceResolutionCandidate>, ResearchError>;
    async fn get_manuscript_reference_resolution_candidate(
        &self,
        id: &ManuscriptReferenceResolutionCandidateId,
    ) -> Result<Option<ManuscriptReferenceResolutionCandidate>, ResearchError>;
    async fn persist_manuscript_reference_resolution(
        &self,
        value: &ManuscriptReferenceResolutionWrite,
    ) -> Result<ManuscriptReferenceResolutionRun, ResearchError>;

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
    async fn insert_citation_target_bindings(
        &self,
        values: &[CitationTargetBinding],
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

    async fn get_manuscript_claim_extraction_run(
        &self,
        id: &ManuscriptClaimExtractionRunId,
    ) -> Result<Option<ManuscriptClaimExtractionRun>, ResearchError>;
    async fn find_completed_manuscript_claim_extraction(
        &self,
        citation_sync_run_id: &ManuscriptCitationSyncRunId,
        context_hash: &ContentHash,
        extractor_provider: &str,
        extractor_version: &str,
        extractor_model_id: Option<&str>,
        extraction_contract_version: &str,
    ) -> Result<Option<ManuscriptClaimExtractionRun>, ResearchError>;
    async fn list_manuscript_claim_extraction_runs(
        &self,
        citation_sync_run_id: Option<&ManuscriptCitationSyncRunId>,
    ) -> Result<Vec<ManuscriptClaimExtractionRun>, ResearchError>;
    async fn list_manuscript_claim_extraction_items(
        &self,
        extraction_run_id: &ManuscriptClaimExtractionRunId,
    ) -> Result<Vec<ManuscriptClaimExtractionItem>, ResearchError>;
    async fn list_manuscript_claim_extraction_coverage(
        &self,
        extraction_run_id: &ManuscriptClaimExtractionRunId,
    ) -> Result<Vec<ManuscriptClaimExtractionCoverage>, ResearchError>;
    async fn persist_manuscript_claim_extraction(
        &self,
        value: &ManuscriptClaimExtractionWrite,
    ) -> Result<ManuscriptClaimExtractionRun, ResearchError>;
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
        self.list_cases().await
    }

    async fn get_case(&self, id: &ResearchCaseId) -> Result<Option<ResearchCase>, ResearchError> {
        self.get_case(id).await
    }

    async fn insert_case(&self, value: &ResearchCase) -> Result<(), ResearchError> {
        self.insert_case(value).await
    }

    async fn list_sources(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchSource>, ResearchError> {
        self.list_sources(research_case_id).await
    }

    async fn get_source(
        &self,
        id: &ResearchSourceId,
    ) -> Result<Option<ResearchSource>, ResearchError> {
        self.get_source(id).await
    }

    async fn insert_source(&self, value: &ResearchSource) -> Result<(), ResearchError> {
        self.insert_source(value).await
    }

    async fn list_snapshots(
        &self,
        source_id: Option<&ResearchSourceId>,
    ) -> Result<Vec<ResearchSourceSnapshot>, ResearchError> {
        self.list_snapshots(source_id).await
    }

    async fn get_snapshot(
        &self,
        id: &ResearchSourceSnapshotId,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError> {
        self.get_snapshot(id).await
    }

    async fn find_snapshot_by_hash(
        &self,
        source_id: &ResearchSourceId,
        content_hash: &ContentHash,
    ) -> Result<Option<ResearchSourceSnapshot>, ResearchError> {
        self.find_snapshot_by_hash(source_id, content_hash).await
    }

    async fn insert_snapshot(&self, value: &ResearchSourceSnapshot) -> Result<bool, ResearchError> {
        self.insert_snapshot(value).await
    }

    async fn get_pdf_extraction(
        &self,
        id: &ResearchPdfExtractionId,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError> {
        self.get_pdf_extraction(id).await
    }

    async fn latest_pdf_extraction(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError> {
        self.latest_pdf_extraction(source_snapshot_id).await
    }

    async fn list_pdf_extractions(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
    ) -> Result<Vec<ResearchPdfExtraction>, ResearchError> {
        self.list_pdf_extractions(source_snapshot_id).await
    }

    async fn find_pdf_extraction(
        &self,
        source_snapshot_id: &ResearchSourceSnapshotId,
        extractor: &str,
        extractor_version: &str,
        extraction_hash: &ContentHash,
    ) -> Result<Option<ResearchPdfExtraction>, ResearchError> {
        self.find_pdf_extraction(
            source_snapshot_id,
            extractor,
            extractor_version,
            extraction_hash,
        )
        .await
    }

    async fn insert_pdf_extraction(
        &self,
        value: &ResearchPdfExtraction,
    ) -> Result<bool, ResearchError> {
        self.insert_pdf_extraction(value).await
    }

    async fn insert_pdf_extraction_with_pages(
        &self,
        extraction: &ResearchPdfExtraction,
        pages: &[ResearchPdfPage],
    ) -> Result<bool, ResearchError> {
        self.insert_pdf_extraction_with_pages(extraction, pages)
            .await
    }

    async fn list_pdf_pages(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        start_page: u32,
        limit: u32,
    ) -> Result<Vec<ResearchPdfPage>, ResearchError> {
        self.list_pdf_pages(extraction_id, start_page, limit).await
    }

    async fn get_pdf_page(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        page: u32,
    ) -> Result<Option<ResearchPdfPage>, ResearchError> {
        self.get_pdf_page(extraction_id, page).await
    }

    async fn insert_pdf_page(&self, value: &ResearchPdfPage) -> Result<(), ResearchError> {
        self.insert_pdf_page(value).await
    }

    async fn list_evidence(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        source_snapshot_id: Option<&ResearchSourceSnapshotId>,
    ) -> Result<Vec<ResearchEvidence>, ResearchError> {
        self.list_evidence(research_case_id, source_snapshot_id)
            .await
    }

    async fn get_evidence(
        &self,
        id: &ResearchEvidenceId,
    ) -> Result<Option<ResearchEvidence>, ResearchError> {
        self.get_evidence(id).await
    }

    async fn insert_evidence(&self, value: &ResearchEvidence) -> Result<(), ResearchError> {
        self.insert_evidence(value).await
    }

    async fn list_claims(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<ResearchClaim>, ResearchError> {
        self.list_claims(research_case_id).await
    }

    async fn get_claim(
        &self,
        id: &ResearchClaimId,
    ) -> Result<Option<ResearchClaim>, ResearchError> {
        self.get_claim(id).await
    }

    async fn insert_claim(&self, value: &ResearchClaim) -> Result<(), ResearchError> {
        self.insert_claim(value).await
    }

    async fn list_links(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        claim_id: Option<&ResearchClaimId>,
        evidence_id: Option<&ResearchEvidenceId>,
    ) -> Result<Vec<ClaimEvidenceLink>, ResearchError> {
        self.list_links(research_case_id, claim_id, evidence_id)
            .await
    }

    async fn get_link(
        &self,
        id: &ClaimEvidenceLinkId,
    ) -> Result<Option<ClaimEvidenceLink>, ResearchError> {
        self.get_link(id).await
    }

    async fn insert_link(&self, value: &ClaimEvidenceLink) -> Result<(), ResearchError> {
        self.insert_link(value).await
    }

    async fn list_citation_occurrences(
        &self,
        research_case_id: Option<&ResearchCaseId>,
    ) -> Result<Vec<CitationOccurrence>, ResearchError> {
        self.list_citation_occurrences(research_case_id).await
    }

    async fn get_citation_occurrence(
        &self,
        id: &CitationOccurrenceId,
    ) -> Result<Option<CitationOccurrence>, ResearchError> {
        self.get_citation_occurrence(id).await
    }

    async fn insert_citation_occurrence(
        &self,
        value: &CitationOccurrence,
    ) -> Result<(), ResearchError> {
        self.insert_citation_occurrence(value).await
    }

    async fn list_citation_targets(
        &self,
        citation_occurrence_id: &CitationOccurrenceId,
    ) -> Result<Vec<CitationTarget>, ResearchError> {
        self.list_citation_targets(citation_occurrence_id).await
    }

    async fn get_citation_target(
        &self,
        id: &CitationTargetId,
    ) -> Result<Option<CitationTarget>, ResearchError> {
        self.get_citation_target(id).await
    }

    async fn insert_citation_target(&self, value: &CitationTarget) -> Result<(), ResearchError> {
        self.insert_citation_target(value).await
    }

    async fn get_manuscript_citation_sync(
        &self,
        id: &ManuscriptCitationSyncRunId,
    ) -> Result<Option<ManuscriptCitationSyncRun>, ResearchError> {
        self.get_manuscript_citation_sync(id).await
    }

    async fn latest_manuscript_citation_sync(
        &self,
        research_case_id: &ResearchCaseId,
        manuscript_source_id: &ResearchSourceId,
    ) -> Result<Option<ManuscriptCitationSyncRun>, ResearchError> {
        self.latest_manuscript_citation_sync(research_case_id, manuscript_source_id)
            .await
    }

    async fn list_manuscript_citation_sync_occurrences(
        &self,
        sync_run_id: &ManuscriptCitationSyncRunId,
    ) -> Result<Vec<ManuscriptCitationSyncOccurrence>, ResearchError> {
        self.list_manuscript_citation_sync_occurrences(sync_run_id)
            .await
    }

    async fn get_manuscript_citation_sync_occurrence(
        &self,
        id: &ManuscriptCitationSyncOccurrenceId,
    ) -> Result<Option<ManuscriptCitationSyncOccurrence>, ResearchError> {
        self.get_manuscript_citation_sync_occurrence(id).await
    }

    async fn list_manuscript_citation_sync_targets(
        &self,
        sync_occurrence_id: &ManuscriptCitationSyncOccurrenceId,
    ) -> Result<Vec<ManuscriptCitationSyncTarget>, ResearchError> {
        self.list_manuscript_citation_sync_targets(sync_occurrence_id)
            .await
    }

    async fn persist_manuscript_citation_sync(
        &self,
        value: &ManuscriptCitationSyncWrite,
    ) -> Result<ManuscriptCitationSyncRun, ResearchError> {
        self.persist_manuscript_citation_sync(value).await
    }

    async fn get_manuscript_reference_catalog_run(
        &self,
        id: &ManuscriptReferenceCatalogRunId,
    ) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError> {
        self.get_manuscript_reference_catalog_run(id).await
    }

    async fn get_manuscript_reference_catalog_for_sync(
        &self,
        citation_sync_run_id: &ManuscriptCitationSyncRunId,
    ) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError> {
        self.get_manuscript_reference_catalog_for_sync(citation_sync_run_id)
            .await
    }

    async fn latest_manuscript_reference_catalog(
        &self,
        research_case_id: &ResearchCaseId,
        manuscript_source_id: &ResearchSourceId,
    ) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError> {
        self.latest_manuscript_reference_catalog(research_case_id, manuscript_source_id)
            .await
    }

    async fn list_manuscript_reference_entries(
        &self,
        catalog_run_id: &ManuscriptReferenceCatalogRunId,
    ) -> Result<Vec<ManuscriptReferenceEntry>, ResearchError> {
        self.list_manuscript_reference_entries(catalog_run_id).await
    }

    async fn get_manuscript_reference_entry(
        &self,
        id: &ManuscriptReferenceEntryId,
    ) -> Result<Option<ManuscriptReferenceEntry>, ResearchError> {
        self.get_manuscript_reference_entry(id).await
    }

    async fn list_manuscript_reference_target_mappings(
        &self,
        reference_entry_id: &ManuscriptReferenceEntryId,
    ) -> Result<Vec<ManuscriptReferenceTargetMapping>, ResearchError> {
        self.list_manuscript_reference_target_mappings(reference_entry_id)
            .await
    }

    async fn persist_manuscript_reference_catalog(
        &self,
        value: &ManuscriptReferenceCatalogWrite,
    ) -> Result<ManuscriptReferenceCatalogRun, ResearchError> {
        self.persist_manuscript_reference_catalog(value).await
    }

    async fn get_manuscript_reference_resolution_run(
        &self,
        id: &ManuscriptReferenceResolutionRunId,
    ) -> Result<Option<ManuscriptReferenceResolutionRun>, ResearchError> {
        self.get_manuscript_reference_resolution_run(id).await
    }

    async fn get_manuscript_reference_resolution_for_catalog(
        &self,
        catalog_run_id: &ManuscriptReferenceCatalogRunId,
        catalog_hash: &ContentHash,
        source_state_hash: &ContentHash,
        resolver_policy_version: &str,
    ) -> Result<Option<ManuscriptReferenceResolutionRun>, ResearchError> {
        self.get_manuscript_reference_resolution_for_catalog(
            catalog_run_id,
            catalog_hash,
            source_state_hash,
            resolver_policy_version,
        )
        .await
    }

    async fn list_manuscript_reference_resolution_entries(
        &self,
        resolution_run_id: &ManuscriptReferenceResolutionRunId,
    ) -> Result<Vec<ManuscriptReferenceResolutionEntry>, ResearchError> {
        self.list_manuscript_reference_resolution_entries(resolution_run_id)
            .await
    }

    async fn get_manuscript_reference_resolution_entry(
        &self,
        id: &ManuscriptReferenceResolutionEntryId,
    ) -> Result<Option<ManuscriptReferenceResolutionEntry>, ResearchError> {
        self.get_manuscript_reference_resolution_entry(id).await
    }

    async fn list_manuscript_reference_resolution_candidates(
        &self,
        resolution_entry_id: &ManuscriptReferenceResolutionEntryId,
    ) -> Result<Vec<ManuscriptReferenceResolutionCandidate>, ResearchError> {
        self.list_manuscript_reference_resolution_candidates(resolution_entry_id)
            .await
    }

    async fn get_manuscript_reference_resolution_candidate(
        &self,
        id: &ManuscriptReferenceResolutionCandidateId,
    ) -> Result<Option<ManuscriptReferenceResolutionCandidate>, ResearchError> {
        self.get_manuscript_reference_resolution_candidate(id).await
    }

    async fn persist_manuscript_reference_resolution(
        &self,
        value: &ManuscriptReferenceResolutionWrite,
    ) -> Result<ManuscriptReferenceResolutionRun, ResearchError> {
        self.persist_manuscript_reference_resolution(value).await
    }

    async fn list_citation_target_bindings(
        &self,
        citation_target_id: &CitationTargetId,
    ) -> Result<Vec<CitationTargetBinding>, ResearchError> {
        self.list_citation_target_bindings(citation_target_id).await
    }

    async fn get_citation_target_binding(
        &self,
        id: &CitationTargetBindingId,
    ) -> Result<Option<CitationTargetBinding>, ResearchError> {
        self.get_citation_target_binding(id).await
    }

    async fn latest_citation_target_binding(
        &self,
        citation_target_id: &CitationTargetId,
    ) -> Result<Option<CitationTargetBinding>, ResearchError> {
        self.latest_citation_target_binding(citation_target_id)
            .await
    }

    async fn insert_citation_target_binding(
        &self,
        value: &CitationTargetBinding,
    ) -> Result<(), ResearchError> {
        self.insert_citation_target_binding(value).await
    }

    async fn insert_citation_target_bindings(
        &self,
        values: &[CitationTargetBinding],
    ) -> Result<(), ResearchError> {
        self.insert_citation_target_bindings(values).await
    }

    async fn list_claim_citation_links(
        &self,
        research_case_id: Option<&ResearchCaseId>,
        claim_id: Option<&ResearchClaimId>,
        citation_occurrence_id: Option<&CitationOccurrenceId>,
    ) -> Result<Vec<ClaimCitationLink>, ResearchError> {
        self.list_claim_citation_links(research_case_id, claim_id, citation_occurrence_id)
            .await
    }

    async fn get_claim_citation_link(
        &self,
        id: &ClaimCitationLinkId,
    ) -> Result<Option<ClaimCitationLink>, ResearchError> {
        self.get_claim_citation_link(id).await
    }

    async fn find_claim_citation_link(
        &self,
        claim_id: &ResearchClaimId,
        citation_occurrence_id: &CitationOccurrenceId,
    ) -> Result<Option<ClaimCitationLink>, ResearchError> {
        self.find_claim_citation_link(claim_id, citation_occurrence_id)
            .await
    }

    async fn insert_claim_citation_link(
        &self,
        value: &ClaimCitationLink,
    ) -> Result<(), ResearchError> {
        self.insert_claim_citation_link(value).await
    }

    async fn get_manuscript_claim_extraction_run(
        &self,
        id: &ManuscriptClaimExtractionRunId,
    ) -> Result<Option<ManuscriptClaimExtractionRun>, ResearchError> {
        self.get_manuscript_claim_extraction_run(id).await
    }

    async fn find_completed_manuscript_claim_extraction(
        &self,
        citation_sync_run_id: &ManuscriptCitationSyncRunId,
        context_hash: &ContentHash,
        extractor_provider: &str,
        extractor_version: &str,
        extractor_model_id: Option<&str>,
        extraction_contract_version: &str,
    ) -> Result<Option<ManuscriptClaimExtractionRun>, ResearchError> {
        self.find_completed_manuscript_claim_extraction(
            citation_sync_run_id,
            context_hash,
            extractor_provider,
            extractor_version,
            extractor_model_id,
            extraction_contract_version,
        )
        .await
    }

    async fn list_manuscript_claim_extraction_runs(
        &self,
        citation_sync_run_id: Option<&ManuscriptCitationSyncRunId>,
    ) -> Result<Vec<ManuscriptClaimExtractionRun>, ResearchError> {
        self.list_manuscript_claim_extraction_runs(citation_sync_run_id)
            .await
    }

    async fn list_manuscript_claim_extraction_items(
        &self,
        extraction_run_id: &ManuscriptClaimExtractionRunId,
    ) -> Result<Vec<ManuscriptClaimExtractionItem>, ResearchError> {
        self.list_manuscript_claim_extraction_items(extraction_run_id)
            .await
    }

    async fn list_manuscript_claim_extraction_coverage(
        &self,
        extraction_run_id: &ManuscriptClaimExtractionRunId,
    ) -> Result<Vec<ManuscriptClaimExtractionCoverage>, ResearchError> {
        self.list_manuscript_claim_extraction_coverage(extraction_run_id)
            .await
    }

    async fn persist_manuscript_claim_extraction(
        &self,
        value: &ManuscriptClaimExtractionWrite,
    ) -> Result<ManuscriptClaimExtractionRun, ResearchError> {
        self.persist_manuscript_claim_extraction(value).await
    }
}
