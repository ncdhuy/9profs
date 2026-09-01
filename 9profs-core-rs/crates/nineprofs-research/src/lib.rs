//! Core-owned research evidence and provenance domain.
//!
//! Evidence is an observation anchored to an immutable source snapshot. It is
//! not truth; ClaimEvidenceLink stores a separately attributed assessment.

mod artifact;
mod document_map;
mod extraction;
mod model;
mod repository;
mod review_execution;
mod review_planner;
mod service;

pub use artifact::{ArtifactUploadWriter, ResearchArtifactStore};
pub use document_map::{
    DOCUMENT_MAP_CONTRACT_VERSION, DocumentMap, DocumentMapBlock, DocumentMapBlockKind,
    DocumentMapCitation, DocumentMapFigure, DocumentMapFigureType, DocumentMapLocator,
    DocumentMapReference, DocumentMapSection, DocumentMapTable, is_document_map_current,
    is_document_map_stale,
};
pub use extraction::{
    ManuscriptClaimExtractionProvider, ManuscriptClaimExtractionProviderError,
    ManuscriptClaimInventoryProvider, ManuscriptClaimInventoryProviderError,
    RegulationRequirementCandidateExtractionProvider,
    RegulationRequirementCandidateExtractionProviderError,
};
pub(crate) use model::bounded_text;
pub use model::{
    AssessmentMethod, CaptureMethod, CapturePdfEvidence, CapturePdfExtraction, CapturePdfPage,
    CaptureSourceSnapshot, CitationBindingMethod, CitationOccurrence, CitationOccurrenceId,
    CitationOccurrenceOrigin, CitationTarget, CitationTargetBinding, CitationTargetBindingId,
    CitationTargetId, CitationTargetResolution, ClaimCitationLink, ClaimCitationLinkId,
    ClaimEvidenceLink, ClaimEvidenceLinkId, ClaimEvidenceRelation, ClaimOrigin, ClaimReviewKind,
    ContentHash, CreateCitationOccurrence, CreateCitationTarget, CreateCitationTargetBinding,
    CreateClaimCitationLink, CreateClaimEvidenceLink, CreateRegulationRequirement,
    CreateResearchCase, CreateResearchClaim, CreateResearchEvidence, CreateResearchSource,
    EvidenceLocator, ExtractManuscriptClaims, ExtractRegulationRequirementCandidates,
    HashAlgorithm, MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_CONTRACT_VERSION,
    MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_SCOPE, MAX_CASE_TITLE_BYTES, MAX_CITATION_MARKER_BYTES,
    MAX_CITATION_REFERENCE_KEY_BYTES, MAX_CITATION_TARGETS_PER_OCCURRENCE, MAX_CITED_LOCATOR_BYTES,
    MAX_CLAIM_EXTRACTION_BLOCKS, MAX_CLAIM_EXTRACTION_CITATIONS_PER_BLOCK,
    MAX_CLAIM_EXTRACTION_CONTEXT_BYTES, MAX_CLAIM_TEXT_BYTES, MAX_EVIDENCE_EXCERPT_BYTES,
    MAX_LOCATOR_BYTES, MAX_MANUSCRIPT_CITATION_OCCURRENCES,
    MAX_MANUSCRIPT_CLAIM_INVENTORY_BLOCK_TEXT_BYTES, MAX_MANUSCRIPT_CLAIM_INVENTORY_BLOCKS,
    MAX_MANUSCRIPT_CLAIM_INVENTORY_CITATIONS_PER_BLOCK,
    MAX_MANUSCRIPT_CLAIM_INVENTORY_CLAIMS_PER_BLOCK, MAX_MANUSCRIPT_CLAIM_INVENTORY_CONTEXT_BYTES,
    MAX_MANUSCRIPT_REFERENCE_CATALOG_BYTES, MAX_MANUSCRIPT_REFERENCE_CATALOG_ENTRIES,
    MAX_MANUSCRIPT_REFERENCE_CATALOG_TARGETS, MAX_MANUSCRIPT_REFERENCE_URI_BYTES,
    MAX_MANUSCRIPT_REFERENCE_URI_COUNT, MAX_METADATA_BYTES, MAX_NORMALIZED_TEXT_BYTES,
    MAX_PDF_BYTES, MAX_PDF_EXTRACTION_BYTES, MAX_PDF_PAGE_TEXT_BYTES, MAX_PDF_PAGES,
    MAX_PROVENANCE_TEXT_BYTES, MAX_RATIONALE_BYTES, MAX_REFERENCE_RESOLUTION_CANDIDATES,
    MAX_REGULATION_REQUIREMENT_CANDIDATES, MAX_REGULATION_REQUIREMENT_EXTRACTION_CONTEXT_BYTES,
    MAX_REGULATION_REQUIREMENT_EXTRACTION_PAGES, MAX_REGULATION_REQUIREMENT_RISK_FLAGS,
    MAX_RETRIEVAL_SCOPE_IDS, MAX_SNAPSHOT_CONTENT_BYTES, MAX_SOURCE_LABEL_BYTES,
    ManuscriptCitationFormat, ManuscriptCitationSyncCitationInput,
    ManuscriptCitationSyncOccurrence, ManuscriptCitationSyncOccurrenceId,
    ManuscriptCitationSyncRun, ManuscriptCitationSyncRunId, ManuscriptCitationSyncStatus,
    ManuscriptCitationSyncTarget, ManuscriptCitationSyncTargetId,
    ManuscriptCitationSyncTargetInput, ManuscriptCitationSyncWrite,
    ManuscriptClaimExtractionBlockInput, ManuscriptClaimExtractionCitationInput,
    ManuscriptClaimExtractionClaimOutput, ManuscriptClaimExtractionCoverage,
    ManuscriptClaimExtractionCoverageId, ManuscriptClaimExtractionCoverageStatus,
    ManuscriptClaimExtractionIdentity, ManuscriptClaimExtractionItem,
    ManuscriptClaimExtractionItemId, ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionRun,
    ManuscriptClaimExtractionRunId, ManuscriptClaimExtractionStatus,
    ManuscriptClaimExtractionUnassociatedCitation, ManuscriptClaimExtractionWrite,
    ManuscriptClaimInventoryBlockInput, ManuscriptClaimInventoryBlockKind,
    ManuscriptClaimInventoryCitationInput, ManuscriptClaimInventoryClaimOutput,
    ManuscriptClaimInventoryCoverage, ManuscriptClaimInventoryCoverageId,
    ManuscriptClaimInventoryCoverageStatus, ManuscriptClaimInventoryIdentity,
    ManuscriptClaimInventoryItem, ManuscriptClaimInventoryItemId, ManuscriptClaimInventoryOutput,
    ManuscriptClaimInventoryRun, ManuscriptClaimInventoryRunId, ManuscriptClaimInventoryStatus,
    ManuscriptClaimInventoryWrite, ManuscriptReferenceCatalogCitationInput,
    ManuscriptReferenceCatalogRun, ManuscriptReferenceCatalogRunId,
    ManuscriptReferenceCatalogStatus, ManuscriptReferenceCatalogTargetInput,
    ManuscriptReferenceCatalogWordSourceInput, ManuscriptReferenceCatalogWrite,
    ManuscriptReferenceCatalogZoteroInput, ManuscriptReferenceEntry, ManuscriptReferenceEntryId,
    ManuscriptReferenceResolutionCandidate, ManuscriptReferenceResolutionCandidateId,
    ManuscriptReferenceResolutionEntry, ManuscriptReferenceResolutionEntryId,
    ManuscriptReferenceResolutionMatchKind, ManuscriptReferenceResolutionOutcome,
    ManuscriptReferenceResolutionRun, ManuscriptReferenceResolutionRunId,
    ManuscriptReferenceResolutionStatus, ManuscriptReferenceResolutionWrite,
    ManuscriptReferenceTargetMapping, ManuscriptReferenceTargetMappingId, PdfExtractionStatus,
    PromoteRegulationRequirementCandidate, REFERENCE_RESOLVER_POLICY_VERSION,
    RegulationApplicability, RegulationApplicabilityVocabulary, RegulationRequirement,
    RegulationRequirementCandidate, RegulationRequirementCandidateExtraction,
    RegulationRequirementCandidateExtractionIdentity, RegulationRequirementCandidateId,
    RegulationRequirementCandidateOutput, RegulationRequirementExtractionInput,
    RegulationRequirementExtractionPage, RegulationRequirementId, RegulationReviewStatus,
    ResearchArtifact, ResearchCase, ResearchCaseId, ResearchClaim, ResearchClaimId,
    ResearchContext, ResearchError, ResearchEvidence, ResearchEvidenceId, ResearchPdfExtraction,
    ResearchPdfExtractionId, ResearchPdfPage, ResearchPdfPageBatch, ResearchRetrievalScope,
    ResearchSource, ResearchSourceId, ResearchSourceIdentity, ResearchSourceIdentityInput,
    ResearchSourceIdentityMethod, ResearchSourceSnapshot, ResearchSourceSnapshotId, SafeMetadata,
    SourceKind, SourceOrigin, StartManuscriptClaimInventory, SyncManuscriptCitations,
    SyncManuscriptReferenceCatalog, VerifiedArtifact, resolve_effective_regulation_requirements,
    validate_metadata,
};
pub use repository::{ResearchRepository, SqliteResearchRepository};
pub use review_execution::{
    Finding, FindingEvidence, FindingValidationFailure, REVIEW_TASK_EXECUTION_CONTRACT_VERSION,
    ReviewExecutionReport, ReviewTaskExecutionError, ReviewTaskExecutionResult, ReviewTaskExecutor,
    ReviewTaskValidation, validate_review_task_response,
};
pub use review_planner::{
    AuthorityPack, AuthorityPackDocument, AuthorityPackLoader, AuthorityPackSource,
    REVIEW_TASK_CONTRACT_VERSION, RegulationRequirementReference, ResolvedReviewStack,
    ReviewAuthorityReference, ReviewExecutorMode, ReviewSectionRole, ReviewTask, ReviewTaskTarget,
    classify_heading_role, load_canonical_authority_packs, plan_review_tasks,
    resolve_authority_packs, resolve_review_stack,
};
pub use service::ResearchService;
