use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchSourceKindDto {
    ReferencePdf,
    Manuscript,
    Dataset,
    Web,
    Regulation,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchCaptureMethodDto {
    UserProvided,
    UploadedArtifact,
    ActiveDocument,
    OfficeCli,
    WebRetrieval,
    ExternalImport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchHashAlgorithmDto {
    Sha256,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchContentHashDto {
    pub algorithm: ResearchHashAlgorithmDto,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResearchSourceOriginDto {
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
        retrieved_at_ms: i64,
    },
    ExternalImport {
        provider: String,
        external_reference: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResearchEvidenceLocatorDto {
    TextRange {
        start: u64,
        end: u64,
    },
    Pdf {
        page: u32,
        end_page: Option<u32>,
    },
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
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResearchClaimOriginDto {
    Manuscript {
        document_id: String,
        document_version: String,
        locator: Option<ResearchEvidenceLocatorDto>,
    },
    User,
    Agent,
    Imported {
        source: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchClaimEvidenceRelationDto {
    Supports,
    Contradicts,
    Contextualizes,
    Insufficient,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchAssessmentMethodDto {
    Human,
    DeterministicChecker,
    Agent,
    ExternalService,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchCaseDto {
    pub case_id: String,
    pub title: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchSourceDto {
    pub source_id: String,
    pub research_case_id: String,
    pub kind: ResearchSourceKindDto,
    pub label: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchSourceSnapshotDto {
    pub snapshot_id: String,
    pub source_id: String,
    pub content_hash: ResearchContentHashDto,
    pub captured_at_ms: i64,
    pub capture_method: ResearchCaptureMethodDto,
    pub origin: ResearchSourceOriginDto,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchPdfExtractionStatusDto {
    Ready,
    NoExtractableText,
    Failed,
    PasswordRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchArtifactDto {
    pub artifact_id: String,
    pub content_hash: ResearchContentHashDto,
    pub size_bytes: u64,
    pub media_type: String,
    pub original_filename: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchPdfExtractionDto {
    pub extraction_id: String,
    pub source_snapshot_id: String,
    pub artifact_id: String,
    pub extractor: String,
    pub extractor_version: String,
    pub page_count: u32,
    pub extraction_hash: ResearchContentHashDto,
    pub extracted_at_ms: i64,
    pub status: ResearchPdfExtractionStatusDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchPdfPageDto {
    pub extraction_id: String,
    pub page: u32,
    pub text: String,
    pub text_hash: ResearchContentHashDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchPdfPageListDto {
    pub data: Vec<ResearchPdfPageDto>,
    pub start_page: u32,
    pub limit: u32,
    pub has_more: bool,
    pub next_start_page: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencePdfIngestionDto {
    pub artifact: ResearchArtifactDto,
    pub source: ResearchSourceDto,
    pub snapshot: ResearchSourceSnapshotDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchEvidenceDto {
    pub evidence_id: String,
    pub research_case_id: String,
    pub source_snapshot_id: String,
    pub verbatim_excerpt: String,
    pub normalized_text: Option<String>,
    pub locator: ResearchEvidenceLocatorDto,
    pub excerpt_hash: ResearchContentHashDto,
    pub captured_at_ms: i64,
    pub capture_method: ResearchCaptureMethodDto,
    pub pdf_extraction_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchClaimDto {
    pub claim_id: String,
    pub research_case_id: String,
    pub text: String,
    pub origin: ResearchClaimOriginDto,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimEvidenceLinkDto {
    pub link_id: String,
    pub research_case_id: String,
    pub claim_id: String,
    pub evidence_id: String,
    pub relation: ResearchClaimEvidenceRelationDto,
    pub rationale: Option<String>,
    pub assessment_method: ResearchAssessmentMethodDto,
    pub assessment_metadata: BTreeMap<String, String>,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateResearchCaseRequest {
    pub title: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateResearchSourceRequest {
    pub research_case_id: String,
    pub kind: ResearchSourceKindDto,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureResearchSourceSnapshotRequest {
    pub source_id: String,
    pub content: String,
    pub capture_method: ResearchCaptureMethodDto,
    pub origin: ResearchSourceOriginDto,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchPdfPageInput {
    pub page: u32,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureResearchPdfExtractionRequest {
    pub extractor: String,
    pub extractor_version: Option<String>,
    pub page_count: u32,
    pub status: ResearchPdfExtractionStatusDto,
    pub pages: Vec<ResearchPdfPageInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureResearchPdfEvidenceRequest {
    pub research_case_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub page: u32,
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateResearchEvidenceRequest {
    pub research_case_id: String,
    pub source_snapshot_id: String,
    pub verbatim_excerpt: String,
    pub normalized_text: Option<String>,
    pub locator: ResearchEvidenceLocatorDto,
    pub capture_method: ResearchCaptureMethodDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateResearchClaimRequest {
    pub research_case_id: String,
    pub text: String,
    pub origin: ResearchClaimOriginDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateClaimEvidenceLinkRequest {
    pub research_case_id: String,
    pub claim_id: String,
    pub evidence_id: String,
    pub relation: ResearchClaimEvidenceRelationDto,
    pub rationale: Option<String>,
    pub assessment_method: ResearchAssessmentMethodDto,
    #[serde(default)]
    pub assessment_metadata: BTreeMap<String, String>,
}
