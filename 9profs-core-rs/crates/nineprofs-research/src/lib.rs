//! Core-owned research evidence and provenance domain.
//!
//! Evidence is an observation anchored to an immutable source snapshot. It is
//! not truth; ClaimEvidenceLink stores a separately attributed assessment.

mod artifact;
mod model;
mod repository;
mod service;

pub use artifact::{ArtifactUploadWriter, ResearchArtifactStore};
pub(crate) use model::bounded_text;
pub use model::{
    AssessmentMethod, CaptureMethod, CapturePdfEvidence, CapturePdfExtraction, CapturePdfPage,
    CaptureSourceSnapshot, ClaimEvidenceLink, ClaimEvidenceLinkId, ClaimEvidenceRelation,
    ClaimOrigin, ContentHash, CreateClaimEvidenceLink, CreateResearchCase, CreateResearchClaim,
    CreateResearchEvidence, CreateResearchSource, EvidenceLocator, HashAlgorithm,
    MAX_CASE_TITLE_BYTES, MAX_CLAIM_TEXT_BYTES, MAX_EVIDENCE_EXCERPT_BYTES, MAX_LOCATOR_BYTES,
    MAX_METADATA_BYTES, MAX_NORMALIZED_TEXT_BYTES, MAX_PDF_BYTES, MAX_PDF_EXTRACTION_BYTES,
    MAX_PDF_PAGE_TEXT_BYTES, MAX_PDF_PAGES, MAX_PROVENANCE_TEXT_BYTES, MAX_RATIONALE_BYTES,
    MAX_SNAPSHOT_CONTENT_BYTES, MAX_SOURCE_LABEL_BYTES, PdfExtractionStatus, ResearchArtifact,
    ResearchCase, ResearchCaseId, ResearchClaim, ResearchClaimId, ResearchError, ResearchEvidence,
    ResearchEvidenceId, ResearchPdfExtraction, ResearchPdfExtractionId, ResearchPdfPage,
    ResearchPdfPageBatch, ResearchSource, ResearchSourceId, ResearchSourceSnapshot,
    ResearchSourceSnapshotId, SafeMetadata, SourceKind, SourceOrigin, VerifiedArtifact,
    validate_metadata,
};
pub use repository::{ResearchRepository, SqliteResearchRepository};
pub use service::ResearchService;
