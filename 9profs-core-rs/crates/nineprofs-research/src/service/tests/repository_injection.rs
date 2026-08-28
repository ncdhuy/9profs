use std::sync::Arc;

use async_trait::async_trait;
use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;

use super::ResearchService;
use crate::{
    CitationOccurrence, CitationOccurrenceId, CitationTarget, CitationTargetBinding,
    CitationTargetBindingId, CitationTargetId, ClaimCitationLink, ClaimCitationLinkId,
    ClaimEvidenceLink, ClaimEvidenceLinkId, ContentHash, ManuscriptCitationSyncOccurrence,
    ManuscriptCitationSyncOccurrenceId, ManuscriptCitationSyncRun, ManuscriptCitationSyncRunId,
    ManuscriptCitationSyncTarget, ManuscriptCitationSyncWrite, ManuscriptClaimExtractionCoverage,
    ManuscriptClaimExtractionItem, ManuscriptClaimExtractionRun, ManuscriptClaimExtractionRunId,
    ManuscriptClaimExtractionWrite, ManuscriptClaimInventoryCoverage,
    ManuscriptClaimInventoryCoverageId, ManuscriptClaimInventoryItem,
    ManuscriptClaimInventoryItemId, ManuscriptClaimInventoryRun, ManuscriptClaimInventoryRunId,
    ManuscriptClaimInventoryWrite, ManuscriptReferenceCatalogRun, ManuscriptReferenceCatalogRunId,
    ManuscriptReferenceCatalogWrite, ManuscriptReferenceEntry, ManuscriptReferenceEntryId,
    ManuscriptReferenceResolutionCandidate, ManuscriptReferenceResolutionCandidateId,
    ManuscriptReferenceResolutionEntry, ManuscriptReferenceResolutionEntryId,
    ManuscriptReferenceResolutionRun, ManuscriptReferenceResolutionRunId,
    ManuscriptReferenceResolutionWrite, ManuscriptReferenceTargetMapping, ResearchCase,
    ResearchCaseId, ResearchClaim, ResearchClaimId, ResearchError, ResearchEvidence,
    ResearchEvidenceId, ResearchPdfExtraction, ResearchPdfExtractionId, ResearchPdfPage,
    ResearchRepository, ResearchSource, ResearchSourceId, ResearchSourceSnapshot,
    ResearchSourceSnapshotId, SqliteResearchRepository,
};

struct FaultInjectingRepository {
    inner: Arc<dyn ResearchRepository>,
}

impl FaultInjectingRepository {
    fn new(inner: Arc<dyn ResearchRepository>) -> Self {
        Self { inner }
    }
}

macro_rules! impl_delegating_repository {
    (
        fail $fail_name:ident($($fail_arg:ident: $fail_ty:ty),*) -> $fail_ret:ty;
        $(
            $name:ident($($arg:ident: $ty:ty),*) -> $ret:ty;
        )*
    ) => {
        #[async_trait]
        impl ResearchRepository for FaultInjectingRepository {
            async fn $fail_name(
                &self,
                $($fail_arg: $fail_ty),*
            ) -> $fail_ret {
                Err(ResearchError::Invalid("injected repository failure".to_owned()))
            }

            $(
                async fn $name(&self, $($arg: $ty),*) -> $ret {
                    self.inner.$name($($arg),*).await
                }
            )*
        }
    };
}

impl_delegating_repository! {
    fail get_case(id: &ResearchCaseId) -> Result<Option<ResearchCase>, ResearchError>;
    list_cases() -> Result<Vec<ResearchCase>, ResearchError>;
    insert_case(value: &ResearchCase) -> Result<(), ResearchError>;
    list_sources(research_case_id: Option<&ResearchCaseId>) -> Result<Vec<ResearchSource>, ResearchError>;
    get_source(id: &ResearchSourceId) -> Result<Option<ResearchSource>, ResearchError>;
    insert_source(value: &ResearchSource) -> Result<(), ResearchError>;
    list_snapshots(source_id: Option<&ResearchSourceId>) -> Result<Vec<ResearchSourceSnapshot>, ResearchError>;
    get_snapshot(id: &ResearchSourceSnapshotId) -> Result<Option<ResearchSourceSnapshot>, ResearchError>;
    find_snapshot_by_hash(source_id: &ResearchSourceId, content_hash: &ContentHash) -> Result<Option<ResearchSourceSnapshot>, ResearchError>;
    insert_snapshot(value: &ResearchSourceSnapshot) -> Result<bool, ResearchError>;
    get_pdf_extraction(id: &ResearchPdfExtractionId) -> Result<Option<ResearchPdfExtraction>, ResearchError>;
    latest_pdf_extraction(source_snapshot_id: &ResearchSourceSnapshotId) -> Result<Option<ResearchPdfExtraction>, ResearchError>;
    list_pdf_extractions(source_snapshot_id: &ResearchSourceSnapshotId) -> Result<Vec<ResearchPdfExtraction>, ResearchError>;
    find_pdf_extraction(source_snapshot_id: &ResearchSourceSnapshotId, extractor: &str, extractor_version: &str, extraction_hash: &ContentHash) -> Result<Option<ResearchPdfExtraction>, ResearchError>;
    insert_pdf_extraction(value: &ResearchPdfExtraction) -> Result<bool, ResearchError>;
    insert_pdf_extraction_with_pages(extraction: &ResearchPdfExtraction, pages: &[ResearchPdfPage]) -> Result<bool, ResearchError>;
    list_pdf_pages(extraction_id: &ResearchPdfExtractionId, start_page: u32, limit: u32) -> Result<Vec<ResearchPdfPage>, ResearchError>;
    get_pdf_page(extraction_id: &ResearchPdfExtractionId, page: u32) -> Result<Option<ResearchPdfPage>, ResearchError>;
    insert_pdf_page(value: &ResearchPdfPage) -> Result<(), ResearchError>;
    list_evidence(research_case_id: Option<&ResearchCaseId>, source_snapshot_id: Option<&ResearchSourceSnapshotId>) -> Result<Vec<ResearchEvidence>, ResearchError>;
    get_evidence(id: &ResearchEvidenceId) -> Result<Option<ResearchEvidence>, ResearchError>;
    insert_evidence(value: &ResearchEvidence) -> Result<(), ResearchError>;
    list_claims(research_case_id: Option<&ResearchCaseId>) -> Result<Vec<ResearchClaim>, ResearchError>;
    get_claim(id: &ResearchClaimId) -> Result<Option<ResearchClaim>, ResearchError>;
    insert_claim(value: &ResearchClaim) -> Result<(), ResearchError>;
    list_links(research_case_id: Option<&ResearchCaseId>, claim_id: Option<&ResearchClaimId>, evidence_id: Option<&ResearchEvidenceId>) -> Result<Vec<ClaimEvidenceLink>, ResearchError>;
    get_link(id: &ClaimEvidenceLinkId) -> Result<Option<ClaimEvidenceLink>, ResearchError>;
    insert_link(value: &ClaimEvidenceLink) -> Result<(), ResearchError>;
    list_citation_occurrences(research_case_id: Option<&ResearchCaseId>) -> Result<Vec<CitationOccurrence>, ResearchError>;
    get_citation_occurrence(id: &CitationOccurrenceId) -> Result<Option<CitationOccurrence>, ResearchError>;
    insert_citation_occurrence(value: &CitationOccurrence) -> Result<(), ResearchError>;
    list_citation_targets(citation_occurrence_id: &CitationOccurrenceId) -> Result<Vec<CitationTarget>, ResearchError>;
    get_citation_target(id: &CitationTargetId) -> Result<Option<CitationTarget>, ResearchError>;
    insert_citation_target(value: &CitationTarget) -> Result<(), ResearchError>;
    get_manuscript_citation_sync(id: &ManuscriptCitationSyncRunId) -> Result<Option<ManuscriptCitationSyncRun>, ResearchError>;
    latest_manuscript_citation_sync(research_case_id: &ResearchCaseId, manuscript_source_id: &ResearchSourceId) -> Result<Option<ManuscriptCitationSyncRun>, ResearchError>;
    list_manuscript_citation_sync_occurrences(sync_run_id: &ManuscriptCitationSyncRunId) -> Result<Vec<ManuscriptCitationSyncOccurrence>, ResearchError>;
    get_manuscript_citation_sync_occurrence(id: &ManuscriptCitationSyncOccurrenceId) -> Result<Option<ManuscriptCitationSyncOccurrence>, ResearchError>;
    list_manuscript_citation_sync_targets(sync_occurrence_id: &ManuscriptCitationSyncOccurrenceId) -> Result<Vec<ManuscriptCitationSyncTarget>, ResearchError>;
    persist_manuscript_citation_sync(value: &ManuscriptCitationSyncWrite) -> Result<ManuscriptCitationSyncRun, ResearchError>;
    get_manuscript_reference_catalog_run(id: &ManuscriptReferenceCatalogRunId) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError>;
    get_manuscript_reference_catalog_for_sync(citation_sync_run_id: &ManuscriptCitationSyncRunId) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError>;
    latest_manuscript_reference_catalog(research_case_id: &ResearchCaseId, manuscript_source_id: &ResearchSourceId) -> Result<Option<ManuscriptReferenceCatalogRun>, ResearchError>;
    list_manuscript_reference_entries(catalog_run_id: &ManuscriptReferenceCatalogRunId) -> Result<Vec<ManuscriptReferenceEntry>, ResearchError>;
    get_manuscript_reference_entry(id: &ManuscriptReferenceEntryId) -> Result<Option<ManuscriptReferenceEntry>, ResearchError>;
    list_manuscript_reference_target_mappings(reference_entry_id: &ManuscriptReferenceEntryId) -> Result<Vec<ManuscriptReferenceTargetMapping>, ResearchError>;
    persist_manuscript_reference_catalog(value: &ManuscriptReferenceCatalogWrite) -> Result<ManuscriptReferenceCatalogRun, ResearchError>;
    get_manuscript_reference_resolution_run(id: &ManuscriptReferenceResolutionRunId) -> Result<Option<ManuscriptReferenceResolutionRun>, ResearchError>;
    get_manuscript_reference_resolution_for_catalog(catalog_run_id: &ManuscriptReferenceCatalogRunId, catalog_hash: &ContentHash, source_state_hash: &ContentHash, resolver_policy_version: &str) -> Result<Option<ManuscriptReferenceResolutionRun>, ResearchError>;
    list_manuscript_reference_resolution_entries(resolution_run_id: &ManuscriptReferenceResolutionRunId) -> Result<Vec<ManuscriptReferenceResolutionEntry>, ResearchError>;
    get_manuscript_reference_resolution_entry(id: &ManuscriptReferenceResolutionEntryId) -> Result<Option<ManuscriptReferenceResolutionEntry>, ResearchError>;
    list_manuscript_reference_resolution_candidates(resolution_entry_id: &ManuscriptReferenceResolutionEntryId) -> Result<Vec<ManuscriptReferenceResolutionCandidate>, ResearchError>;
    get_manuscript_reference_resolution_candidate(id: &ManuscriptReferenceResolutionCandidateId) -> Result<Option<ManuscriptReferenceResolutionCandidate>, ResearchError>;
    persist_manuscript_reference_resolution(value: &ManuscriptReferenceResolutionWrite) -> Result<ManuscriptReferenceResolutionRun, ResearchError>;
    persist_manuscript_reference_resolution_with_bindings(value: &ManuscriptReferenceResolutionWrite, bindings: &[CitationTargetBinding]) -> Result<(ManuscriptReferenceResolutionRun, Vec<CitationTargetBinding>), ResearchError>;
    list_citation_target_bindings(citation_target_id: &CitationTargetId) -> Result<Vec<CitationTargetBinding>, ResearchError>;
    get_citation_target_binding(id: &CitationTargetBindingId) -> Result<Option<CitationTargetBinding>, ResearchError>;
    latest_citation_target_binding(citation_target_id: &CitationTargetId) -> Result<Option<CitationTargetBinding>, ResearchError>;
    insert_citation_target_binding(value: &CitationTargetBinding) -> Result<(), ResearchError>;
    insert_citation_target_bindings(values: &[CitationTargetBinding]) -> Result<(), ResearchError>;
    list_claim_citation_links(research_case_id: Option<&ResearchCaseId>, claim_id: Option<&ResearchClaimId>, citation_occurrence_id: Option<&CitationOccurrenceId>) -> Result<Vec<ClaimCitationLink>, ResearchError>;
    get_claim_citation_link(id: &ClaimCitationLinkId) -> Result<Option<ClaimCitationLink>, ResearchError>;
    find_claim_citation_link(claim_id: &ResearchClaimId, citation_occurrence_id: &CitationOccurrenceId) -> Result<Option<ClaimCitationLink>, ResearchError>;
    insert_claim_citation_link(value: &ClaimCitationLink) -> Result<(), ResearchError>;
    get_manuscript_claim_extraction_run(id: &ManuscriptClaimExtractionRunId) -> Result<Option<ManuscriptClaimExtractionRun>, ResearchError>;
    find_completed_manuscript_claim_extraction(citation_sync_run_id: &ManuscriptCitationSyncRunId, context_hash: &ContentHash, extractor_provider: &str, extractor_version: &str, extractor_model_id: Option<&str>, extraction_contract_version: &str) -> Result<Option<ManuscriptClaimExtractionRun>, ResearchError>;
    list_manuscript_claim_extraction_runs(citation_sync_run_id: Option<&ManuscriptCitationSyncRunId>) -> Result<Vec<ManuscriptClaimExtractionRun>, ResearchError>;
    list_manuscript_claim_extraction_items(extraction_run_id: &ManuscriptClaimExtractionRunId) -> Result<Vec<ManuscriptClaimExtractionItem>, ResearchError>;
    list_manuscript_claim_extraction_coverage(extraction_run_id: &ManuscriptClaimExtractionRunId) -> Result<Vec<ManuscriptClaimExtractionCoverage>, ResearchError>;
    persist_manuscript_claim_extraction(value: &ManuscriptClaimExtractionWrite) -> Result<ManuscriptClaimExtractionRun, ResearchError>;
    get_manuscript_claim_inventory_run(id: &ManuscriptClaimInventoryRunId) -> Result<Option<ManuscriptClaimInventoryRun>, ResearchError>;
    find_completed_manuscript_claim_inventory(research_case_id: &ResearchCaseId, manuscript_source_id: &ResearchSourceId, document_id: &str, document_version: i64, document_context_hash: &ContentHash, extractor_provider: &str, extractor_version: &str, extractor_model_id: Option<&str>, extraction_contract_version: &str) -> Result<Option<ManuscriptClaimInventoryRun>, ResearchError>;
    list_manuscript_claim_inventory_items(inventory_run_id: &ManuscriptClaimInventoryRunId) -> Result<Vec<ManuscriptClaimInventoryItem>, ResearchError>;
    list_manuscript_claim_inventory_coverage(inventory_run_id: &ManuscriptClaimInventoryRunId) -> Result<Vec<ManuscriptClaimInventoryCoverage>, ResearchError>;
    persist_manuscript_claim_inventory(value: &ManuscriptClaimInventoryWrite) -> Result<ManuscriptClaimInventoryRun, ResearchError>;
}

#[test]
fn research_service_remains_clone_send_sync() {
    fn assert_bounds<T: Clone + Send + Sync>() {}

    assert_bounds::<ResearchService>();
}

#[tokio::test]
async fn service_accepts_object_safe_repository_and_preserves_injected_error() {
    let database = Database::in_memory().await.unwrap();
    let sqlite: Arc<dyn ResearchRepository> =
        Arc::new(SqliteResearchRepository::new(database.pool().clone()));
    let repository: Arc<dyn ResearchRepository> = Arc::new(FaultInjectingRepository::new(sqlite));
    let service =
        ResearchService::with_repository(repository, Arc::new(BroadcastEventBus::new(64)));
    let case_id = ResearchCaseId::new();

    let error = service.get_case(case_id.as_str()).await.unwrap_err();

    assert!(matches!(
        error,
        ResearchError::Invalid(message) if message == "injected repository failure"
    ));
}
