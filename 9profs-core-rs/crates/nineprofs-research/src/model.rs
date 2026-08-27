use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use nineprofs_common::{TimestampMs, new_id};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_CASE_TITLE_BYTES: usize = 512;
pub const MAX_SOURCE_LABEL_BYTES: usize = 1_024;
pub const MAX_SNAPSHOT_CONTENT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_EVIDENCE_EXCERPT_BYTES: usize = 64 * 1024;
pub const MAX_NORMALIZED_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_CLAIM_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_RATIONALE_BYTES: usize = 16 * 1024;
pub const MAX_METADATA_BYTES: usize = 8 * 1024;
pub const MAX_LOCATOR_BYTES: usize = 4 * 1024;
pub const MAX_PROVENANCE_TEXT_BYTES: usize = 1_024;
pub const MAX_CITATION_MARKER_BYTES: usize = 4 * 1024;
pub const MAX_CITATION_REFERENCE_KEY_BYTES: usize = MAX_PROVENANCE_TEXT_BYTES;
pub const MAX_CITED_LOCATOR_BYTES: usize = MAX_PROVENANCE_TEXT_BYTES;
pub const MAX_CITATION_TARGETS_PER_OCCURRENCE: usize = 128;
pub const MAX_PDF_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PDF_PAGES: u32 = 10_000;
pub const MAX_PDF_PAGE_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_PDF_EXTRACTION_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RETRIEVAL_SCOPE_IDS: usize = 16;

#[derive(Debug, Error)]
pub enum ResearchError {
    #[error("research {entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },
    #[error("invalid research input: {0}")]
    Invalid(String),
    #[error("research database query failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("research serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("research artifact storage failed: {0}")]
    Artifact(String),
}

macro_rules! id_type {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(new_id())
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, ResearchError> {
                let value = value.into();
                validate_identity($label, &value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

id_type!(ResearchCaseId, "case ID");
id_type!(ResearchSourceId, "source ID");
id_type!(ResearchSourceSnapshotId, "source snapshot ID");
id_type!(ResearchPdfExtractionId, "PDF extraction ID");
id_type!(ResearchEvidenceId, "evidence ID");
id_type!(ResearchClaimId, "claim ID");
id_type!(ClaimEvidenceLinkId, "claim-evidence link ID");
id_type!(CitationOccurrenceId, "citation occurrence ID");
id_type!(CitationTargetId, "citation target ID");
id_type!(CitationTargetBindingId, "citation target binding ID");
id_type!(ClaimCitationLinkId, "claim-citation link ID");

/// Provider-neutral retrieval boundary. Provider adapters translate these
/// canonical identities into provider-specific filters.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResearchRetrievalScope {
    Case,
    Sources {
        source_ids: Vec<ResearchSourceId>,
    },
    Extractions {
        extraction_ids: Vec<ResearchPdfExtractionId>,
    },
}

impl ResearchRetrievalScope {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::Case => Ok(()),
            Self::Sources { source_ids } => validate_retrieval_scope_ids(source_ids),
            Self::Extractions { extraction_ids } => validate_retrieval_scope_ids(extraction_ids),
        }
    }
}

fn validate_retrieval_scope_ids<T: Ord>(ids: &[T]) -> Result<(), ResearchError> {
    if ids.is_empty() {
        return Err(ResearchError::Invalid(
            "retrieval scope must contain at least one identity".to_owned(),
        ));
    }
    if ids.len() > MAX_RETRIEVAL_SCOPE_IDS {
        return Err(ResearchError::Invalid(format!(
            "retrieval scope cannot contain more than {MAX_RETRIEVAL_SCOPE_IDS} identities"
        )));
    }
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(ResearchError::Invalid(
            "retrieval scope identities must be unique".to_owned(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchCase {
    pub id: ResearchCaseId,
    pub title: String,
    pub created_at_ms: TimestampMs,
    pub updated_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    ReferencePdf,
    Manuscript,
    Dataset,
    Web,
    Regulation,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchSource {
    pub id: ResearchSourceId,
    pub research_case_id: ResearchCaseId,
    pub kind: SourceKind,
    pub label: String,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgorithm {
    Sha256,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentHash {
    pub algorithm: HashAlgorithm,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMethod {
    UserProvided,
    UploadedArtifact,
    ActiveDocument,
    OfficeCli,
    WebRetrieval,
    ExternalImport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceOrigin {
    UploadedArtifact {
        artifact_id: String,
        revision_id: Option<String>,
    },
    ActiveDocumentSnapshot {
        document_id: String,
        document_version: String,
    },
    OfficeCliArtifactRevision {
        artifact_id: String,
        revision_id: String,
    },
    WebRetrieval {
        url: String,
        retrieved_at_ms: TimestampMs,
    },
    ExternalImport {
        provider: String,
        external_reference: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchSourceSnapshot {
    pub id: ResearchSourceSnapshotId,
    pub source_id: ResearchSourceId,
    pub content_hash: ContentHash,
    pub captured_at_ms: TimestampMs,
    pub capture_method: CaptureMethod,
    pub origin: SourceOrigin,
    pub metadata: SafeMetadata,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchArtifact {
    pub id: String,
    pub content_hash: ContentHash,
    pub size_bytes: u64,
    pub media_type: String,
    pub original_filename: String,
    pub created_at_ms: TimestampMs,
}

/// Hash and identity returned only by the trusted artifact writer.
#[derive(Clone, Debug)]
pub struct VerifiedArtifact {
    artifact: ResearchArtifact,
}

impl VerifiedArtifact {
    pub(crate) fn new(artifact: ResearchArtifact) -> Self {
        Self { artifact }
    }

    pub fn artifact(&self) -> &ResearchArtifact {
        &self.artifact
    }

    pub fn artifact_id(&self) -> &str {
        &self.artifact.id
    }

    pub fn content_hash(&self) -> &ContentHash {
        &self.artifact.content_hash
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfExtractionStatus {
    Ready,
    NoExtractableText,
    Failed,
    PasswordRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchPdfExtraction {
    pub id: ResearchPdfExtractionId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub artifact_id: String,
    pub extractor: String,
    pub extractor_version: String,
    pub page_count: u32,
    pub extraction_hash: ContentHash,
    pub extracted_at_ms: TimestampMs,
    pub status: PdfExtractionStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchPdfPage {
    pub extraction_id: ResearchPdfExtractionId,
    pub page: u32,
    pub text: String,
    pub text_hash: ContentHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResearchPdfPageBatch {
    pub pages: Vec<ResearchPdfPage>,
    pub start_page: u32,
    pub limit: u32,
    pub has_more: bool,
    pub next_start_page: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvidenceLocator {
    TextRange {
        start: u64,
        end: u64,
    },
    Pdf {
        page: u32,
        end_page: Option<u32>,
    },
    /// Page-relative Unicode scalar/code-point offsets. They are not UTF-8 byte
    /// offsets and not JavaScript UTF-16 indexes.
    PdfTextRange {
        page: u32,
        start: u64,
        end: u64,
    },
    Manuscript {
        block_id: String,
        start: Option<u64>,
        end: Option<u64>,
    },
    Spreadsheet {
        sheet: String,
        range: String,
    },
    Web {
        fragment: Option<String>,
        start: Option<u64>,
        end: Option<u64>,
    },
    Regulation {
        article: String,
        section: Option<String>,
        clause: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchEvidence {
    pub id: ResearchEvidenceId,
    pub research_case_id: ResearchCaseId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub verbatim_excerpt: String,
    pub normalized_text: Option<String>,
    pub locator: EvidenceLocator,
    pub excerpt_hash: ContentHash,
    pub captured_at_ms: TimestampMs,
    pub capture_method: CaptureMethod,
    pub pdf_extraction_id: Option<ResearchPdfExtractionId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClaimOrigin {
    Manuscript {
        document_id: String,
        document_version: String,
        locator: Option<EvidenceLocator>,
    },
    User,
    Agent,
    Imported {
        source: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchClaim {
    pub id: ResearchClaimId,
    pub research_case_id: ResearchCaseId,
    pub text: String,
    pub origin: ClaimOrigin,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CitationOccurrenceOrigin {
    Manuscript {
        document_id: String,
        document_version: String,
        locator: Option<EvidenceLocator>,
    },
    /// Future immutable manuscript provenance seam. Phase 5C1 stores the
    /// explicit snapshot identity but does not create manuscript snapshots.
    ManuscriptSnapshot {
        source_snapshot_id: ResearchSourceSnapshotId,
        locator: Option<EvidenceLocator>,
    },
    Imported {
        source: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CitationOccurrence {
    pub id: CitationOccurrenceId,
    pub research_case_id: ResearchCaseId,
    pub origin: CitationOccurrenceOrigin,
    pub rendered_text: String,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CitationTarget {
    pub id: CitationTargetId,
    pub citation_occurrence_id: CitationOccurrenceId,
    pub ordinal: u32,
    pub reference_key: String,
    pub cited_locator: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationBindingMethod {
    Human,
    Imported,
    DeterministicResolver,
    Agent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CitationTargetBinding {
    pub id: CitationTargetBindingId,
    pub research_case_id: ResearchCaseId,
    pub citation_target_id: CitationTargetId,
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: Option<ResearchSourceSnapshotId>,
    pub extraction_id: Option<ResearchPdfExtractionId>,
    pub method: CitationBindingMethod,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationTargetResolution {
    Unresolved,
    SourceBound,
    PdfExtractionBound,
}

impl CitationTargetBinding {
    pub fn resolution(&self) -> CitationTargetResolution {
        if self.extraction_id.is_some() {
            CitationTargetResolution::PdfExtractionBound
        } else {
            CitationTargetResolution::SourceBound
        }
    }

    pub fn pdf_verification_ready(&self) -> bool {
        matches!(
            self.resolution(),
            CitationTargetResolution::PdfExtractionBound
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClaimCitationLink {
    pub id: ClaimCitationLinkId,
    pub research_case_id: ResearchCaseId,
    pub claim_id: ResearchClaimId,
    pub citation_occurrence_id: CitationOccurrenceId,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimEvidenceRelation {
    Supports,
    Contradicts,
    Contextualizes,
    Insufficient,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentMethod {
    Human,
    DeterministicChecker,
    Agent,
    ExternalService,
}

pub type SafeMetadata = BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClaimEvidenceLink {
    pub id: ClaimEvidenceLinkId,
    pub research_case_id: ResearchCaseId,
    pub claim_id: ResearchClaimId,
    pub evidence_id: ResearchEvidenceId,
    pub relation: ClaimEvidenceRelation,
    pub rationale: Option<String>,
    pub assessment_method: AssessmentMethod,
    pub assessment_metadata: SafeMetadata,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug)]
pub struct CreateResearchCase {
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct CreateResearchSource {
    pub research_case_id: ResearchCaseId,
    pub kind: SourceKind,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct CaptureSourceSnapshot {
    pub source_id: ResearchSourceId,
    pub content: Vec<u8>,
    pub capture_method: CaptureMethod,
    pub origin: SourceOrigin,
    pub metadata: SafeMetadata,
}

#[derive(Clone, Debug)]
pub struct CapturePdfExtraction {
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub extractor: String,
    pub extractor_version: Option<String>,
    pub page_count: u32,
    pub status: PdfExtractionStatus,
    pub pages: Vec<CapturePdfPage>,
}

#[derive(Clone, Debug)]
pub struct CapturePdfPage {
    pub page: u32,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct CapturePdfEvidence {
    pub research_case_id: ResearchCaseId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub extraction_id: ResearchPdfExtractionId,
    pub page: u32,
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug)]
pub struct CreateResearchEvidence {
    pub research_case_id: ResearchCaseId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub verbatim_excerpt: String,
    pub normalized_text: Option<String>,
    pub locator: EvidenceLocator,
    pub capture_method: CaptureMethod,
}

#[derive(Clone, Debug)]
pub struct CreateResearchClaim {
    pub research_case_id: ResearchCaseId,
    pub text: String,
    pub origin: ClaimOrigin,
}

#[derive(Clone, Debug)]
pub struct CreateClaimEvidenceLink {
    pub research_case_id: ResearchCaseId,
    pub claim_id: ResearchClaimId,
    pub evidence_id: ResearchEvidenceId,
    pub relation: ClaimEvidenceRelation,
    pub rationale: Option<String>,
    pub assessment_method: AssessmentMethod,
    pub assessment_metadata: SafeMetadata,
}

#[derive(Clone, Debug)]
pub struct CreateCitationOccurrence {
    pub research_case_id: ResearchCaseId,
    pub origin: CitationOccurrenceOrigin,
    pub rendered_text: String,
}

#[derive(Clone, Debug)]
pub struct CreateCitationTarget {
    pub citation_occurrence_id: CitationOccurrenceId,
    pub ordinal: u32,
    pub reference_key: String,
    pub cited_locator: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CreateCitationTargetBinding {
    pub research_case_id: ResearchCaseId,
    pub citation_target_id: CitationTargetId,
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: Option<ResearchSourceSnapshotId>,
    pub extraction_id: Option<ResearchPdfExtractionId>,
    pub method: CitationBindingMethod,
}

#[derive(Clone, Debug)]
pub struct CreateClaimCitationLink {
    pub research_case_id: ResearchCaseId,
    pub claim_id: ResearchClaimId,
    pub citation_occurrence_id: CitationOccurrenceId,
}

impl SourceOrigin {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::UploadedArtifact {
                artifact_id,
                revision_id,
            } => {
                safe_provenance_text("artifact_id", artifact_id)?;
                if let Some(revision_id) = revision_id {
                    safe_provenance_text("revision_id", revision_id)?;
                }
            }
            Self::ActiveDocumentSnapshot {
                document_id,
                document_version,
            } => {
                safe_provenance_text("document_id", document_id)?;
                safe_provenance_text("document_version", document_version)?;
            }
            Self::OfficeCliArtifactRevision {
                artifact_id,
                revision_id,
            } => {
                safe_provenance_text("artifact_id", artifact_id)?;
                safe_provenance_text("revision_id", revision_id)?;
            }
            Self::WebRetrieval {
                url,
                retrieved_at_ms,
            } => {
                safe_provenance_text("url", url)?;
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(ResearchError::Invalid(
                        "web origin URL must use http or https".to_owned(),
                    ));
                }
                let lowered = url.to_ascii_lowercase();
                if url.contains('@')
                    || [
                        "authorization",
                        "bearer ",
                        "cookie=",
                        "password=",
                        "secret=",
                        "token=",
                    ]
                    .iter()
                    .any(|marker| lowered.contains(marker))
                {
                    return Err(ResearchError::Invalid(
                        "web origin URL must not contain credentials or authorization".to_owned(),
                    ));
                }
                if *retrieved_at_ms <= 0 {
                    return Err(ResearchError::Invalid(
                        "web origin retrieval time must be positive".to_owned(),
                    ));
                }
            }
            Self::ExternalImport {
                provider,
                external_reference,
            } => {
                safe_provenance_text("provider", provider)?;
                safe_provenance_text("external_reference", external_reference)?;
            }
        }
        Ok(())
    }
}

impl EvidenceLocator {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::TextRange { start, end } => validate_range(*start, *end)?,
            Self::Pdf { page, end_page } => {
                if *page == 0 || end_page.is_some_and(|end_page| end_page < *page) {
                    return Err(ResearchError::Invalid(
                        "PDF locator page range is invalid".to_owned(),
                    ));
                }
            }
            Self::PdfTextRange { page, start, end } => {
                if *page == 0 {
                    return Err(ResearchError::Invalid(
                        "PDF text locator page must be positive".to_owned(),
                    ));
                }
                validate_range(*start, *end)?;
            }
            Self::Manuscript {
                block_id,
                start,
                end,
            } => {
                bounded_text("block_id", block_id, MAX_PROVENANCE_TEXT_BYTES)?;
                if let (Some(start), Some(end)) = (start, end) {
                    validate_range(*start, *end)?;
                }
            }
            Self::Spreadsheet { sheet, range } => {
                bounded_text("sheet", sheet, MAX_PROVENANCE_TEXT_BYTES)?;
                bounded_text("range", range, MAX_PROVENANCE_TEXT_BYTES)?;
            }
            Self::Web {
                fragment,
                start,
                end,
            } => {
                if let Some(fragment) = fragment {
                    bounded_text("fragment", fragment, MAX_PROVENANCE_TEXT_BYTES)?;
                }
                if let (Some(start), Some(end)) = (start, end) {
                    validate_range(*start, *end)?;
                }
            }
            Self::Regulation {
                article,
                section,
                clause,
            } => {
                bounded_text("article", article, MAX_PROVENANCE_TEXT_BYTES)?;
                if let Some(section) = section {
                    bounded_text("section", section, MAX_PROVENANCE_TEXT_BYTES)?;
                }
                if let Some(clause) = clause {
                    bounded_text("clause", clause, MAX_PROVENANCE_TEXT_BYTES)?;
                }
            }
        }
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_LOCATOR_BYTES {
            return Err(ResearchError::Invalid(format!(
                "locator exceeds {MAX_LOCATOR_BYTES} bytes"
            )));
        }
        Ok(())
    }
}

impl ClaimOrigin {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::Manuscript {
                document_id,
                document_version,
                locator,
            } => {
                safe_provenance_text("document_id", document_id)?;
                safe_provenance_text("document_version", document_version)?;
                if let Some(locator) = locator {
                    locator.validate()?;
                }
            }
            Self::User | Self::Agent => {}
            Self::Imported { source } => safe_provenance_text("source", source)?,
        }
        Ok(())
    }
}

impl CitationOccurrenceOrigin {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::Manuscript {
                document_id,
                document_version,
                locator,
            } => {
                safe_provenance_text("document_id", document_id)?;
                safe_provenance_text("document_version", document_version)?;
                if let Some(locator) = locator {
                    locator.validate()?;
                }
            }
            Self::ManuscriptSnapshot {
                source_snapshot_id,
                locator,
            } => {
                if let Some(locator) = locator {
                    locator.validate()?;
                }
                let _ = source_snapshot_id;
            }
            Self::Imported { source } => safe_provenance_text("source", source)?,
        }
        Ok(())
    }
}

pub fn validate_metadata(metadata: &SafeMetadata) -> Result<(), ResearchError> {
    let bytes = serde_json::to_vec(metadata)?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(ResearchError::Invalid(format!(
            "metadata exceeds {MAX_METADATA_BYTES} bytes"
        )));
    }
    for (key, value) in metadata {
        bounded_text("metadata key", key, MAX_PROVENANCE_TEXT_BYTES)?;
        bounded_text("metadata value", value, MAX_PROVENANCE_TEXT_BYTES)?;
        let normalized = key.to_ascii_lowercase().replace(['-', ' '], "_");
        if [
            "authorization",
            "api_key",
            "apikey",
            "bearer",
            "cookie",
            "credential",
            "password",
            "secret",
            "session_token",
            "token",
        ]
        .iter()
        .any(|secret_key| normalized.contains(secret_key))
        {
            return Err(ResearchError::Invalid(format!(
                "metadata key is not allowed for provenance: {key}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn bounded_text(
    field: &str,
    value: &str,
    max_bytes: usize,
) -> Result<(), ResearchError> {
    if value.trim().is_empty() {
        return Err(ResearchError::Invalid(format!("{field} must not be empty")));
    }
    if value.len() > max_bytes {
        return Err(ResearchError::Invalid(format!(
            "{field} exceeds {max_bytes} bytes"
        )));
    }
    if value
        .chars()
        .any(|character| character == '\0' || character == '\u{7f}')
    {
        return Err(ResearchError::Invalid(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

fn validate_identity(field: &str, value: &str) -> Result<(), ResearchError> {
    bounded_text(field, value, MAX_PROVENANCE_TEXT_BYTES)?;
    if value == "."
        || value == ".."
        || value
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '/' | '\\' | ':'))
    {
        return Err(ResearchError::Invalid(format!("invalid {field}: {value}")));
    }
    Ok(())
}

fn safe_provenance_text(field: &str, value: &str) -> Result<(), ResearchError> {
    bounded_text(field, value, MAX_PROVENANCE_TEXT_BYTES)?;
    let lowered = value.to_ascii_lowercase();
    if [
        "authorization:",
        "api_key=",
        "apikey=",
        "bearer ",
        "cookie=",
        "password=",
        "secret=",
        "token=",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
    {
        return Err(ResearchError::Invalid(format!(
            "{field} contains secret-like transport data"
        )));
    }
    Ok(())
}

fn validate_range(start: u64, end: u64) -> Result<(), ResearchError> {
    if start > end {
        return Err(ResearchError::Invalid(
            "locator range start must not exceed end".to_owned(),
        ));
    }
    Ok(())
}
