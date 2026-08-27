//! Core-owned research evidence and provenance domain.
//!
//! Evidence is an observation anchored to an immutable source snapshot. It is
//! not truth; ClaimEvidenceLink stores a separately attributed assessment.

mod artifact;
mod extraction;
mod model;
mod repository;
mod service;

pub use artifact::{ArtifactUploadWriter, ResearchArtifactStore};
pub use extraction::{ManuscriptClaimExtractionProvider, ManuscriptClaimExtractionProviderError};
pub(crate) use model::bounded_text;
pub use model::{
    AssessmentMethod, CaptureMethod, CapturePdfEvidence, CapturePdfExtraction, CapturePdfPage,
    CaptureSourceSnapshot, CitationBindingMethod, CitationOccurrence, CitationOccurrenceId,
    CitationOccurrenceOrigin, CitationTarget, CitationTargetBinding, CitationTargetBindingId,
    CitationTargetId, CitationTargetResolution, ClaimCitationLink, ClaimCitationLinkId,
    ClaimEvidenceLink, ClaimEvidenceLinkId, ClaimEvidenceRelation, ClaimOrigin, ContentHash,
    CreateCitationOccurrence, CreateCitationTarget, CreateCitationTargetBinding,
    CreateClaimCitationLink, CreateClaimEvidenceLink, CreateResearchCase, CreateResearchClaim,
    CreateResearchEvidence, CreateResearchSource, EvidenceLocator, ExtractManuscriptClaims,
    HashAlgorithm, MAX_CASE_TITLE_BYTES, MAX_CITATION_MARKER_BYTES,
    MAX_CITATION_REFERENCE_KEY_BYTES, MAX_CITATION_TARGETS_PER_OCCURRENCE, MAX_CITED_LOCATOR_BYTES,
    MAX_CLAIM_EXTRACTION_BLOCKS, MAX_CLAIM_EXTRACTION_CITATIONS_PER_BLOCK,
    MAX_CLAIM_EXTRACTION_CONTEXT_BYTES, MAX_CLAIM_TEXT_BYTES, MAX_EVIDENCE_EXCERPT_BYTES,
    MAX_LOCATOR_BYTES, MAX_MANUSCRIPT_CITATION_OCCURRENCES, MAX_MANUSCRIPT_REFERENCE_CATALOG_BYTES,
    MAX_MANUSCRIPT_REFERENCE_CATALOG_ENTRIES, MAX_MANUSCRIPT_REFERENCE_CATALOG_TARGETS,
    MAX_MANUSCRIPT_REFERENCE_URI_BYTES, MAX_MANUSCRIPT_REFERENCE_URI_COUNT, MAX_METADATA_BYTES,
    MAX_NORMALIZED_TEXT_BYTES, MAX_PDF_BYTES, MAX_PDF_EXTRACTION_BYTES, MAX_PDF_PAGE_TEXT_BYTES,
    MAX_PDF_PAGES, MAX_PROVENANCE_TEXT_BYTES, MAX_RATIONALE_BYTES, MAX_RETRIEVAL_SCOPE_IDS,
    MAX_SNAPSHOT_CONTENT_BYTES, MAX_SOURCE_LABEL_BYTES, ManuscriptCitationFormat,
    ManuscriptCitationSyncCitationInput, ManuscriptCitationSyncOccurrence,
    ManuscriptCitationSyncOccurrenceId, ManuscriptCitationSyncRun, ManuscriptCitationSyncRunId,
    ManuscriptCitationSyncStatus, ManuscriptCitationSyncTarget, ManuscriptCitationSyncTargetId,
    ManuscriptCitationSyncTargetInput, ManuscriptCitationSyncWrite,
    ManuscriptClaimExtractionBlockInput, ManuscriptClaimExtractionCitationInput,
    ManuscriptClaimExtractionClaimOutput, ManuscriptClaimExtractionCoverage,
    ManuscriptClaimExtractionCoverageId, ManuscriptClaimExtractionCoverageStatus,
    ManuscriptClaimExtractionIdentity, ManuscriptClaimExtractionItem,
    ManuscriptClaimExtractionItemId, ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionRun,
    ManuscriptClaimExtractionRunId, ManuscriptClaimExtractionStatus,
    ManuscriptClaimExtractionUnassociatedCitation, ManuscriptClaimExtractionWrite,
    ManuscriptReferenceCatalogCitationInput, ManuscriptReferenceCatalogRun,
    ManuscriptReferenceCatalogRunId, ManuscriptReferenceCatalogStatus,
    ManuscriptReferenceCatalogTargetInput, ManuscriptReferenceCatalogWordSourceInput,
    ManuscriptReferenceCatalogWrite, ManuscriptReferenceCatalogZoteroInput,
    ManuscriptReferenceEntry, ManuscriptReferenceEntryId, ManuscriptReferenceTargetMapping,
    ManuscriptReferenceTargetMappingId, PdfExtractionStatus, ResearchArtifact, ResearchCase,
    ResearchCaseId, ResearchClaim, ResearchClaimId, ResearchError, ResearchEvidence,
    ResearchEvidenceId, ResearchPdfExtraction, ResearchPdfExtractionId, ResearchPdfPage,
    ResearchPdfPageBatch, ResearchRetrievalScope, ResearchSource, ResearchSourceId,
    ResearchSourceSnapshot, ResearchSourceSnapshotId, SafeMetadata, SourceKind, SourceOrigin,
    SyncManuscriptCitations, SyncManuscriptReferenceCatalog, VerifiedArtifact, validate_metadata,
};
pub use repository::{ResearchRepository, SqliteResearchRepository};
pub use service::ResearchService;
