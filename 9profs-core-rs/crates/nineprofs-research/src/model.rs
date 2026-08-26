use std::{collections::BTreeMap, fmt};

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
id_type!(ResearchEvidenceId, "evidence ID");
id_type!(ResearchClaimId, "claim ID");
id_type!(ClaimEvidenceLinkId, "claim-evidence link ID");

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
