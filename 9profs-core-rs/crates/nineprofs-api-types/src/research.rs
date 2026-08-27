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
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResearchCitationOccurrenceOriginDto {
    Manuscript {
        document_id: String,
        document_version: String,
        locator: Option<ResearchEvidenceLocatorDto>,
    },
    ManuscriptSnapshot {
        source_snapshot_id: String,
        locator: Option<ResearchEvidenceLocatorDto>,
    },
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
#[serde(rename_all = "snake_case")]
pub enum ResearchCitationBindingMethodDto {
    Human,
    Imported,
    DeterministicResolver,
    Agent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchCitationTargetResolutionDto {
    Unresolved,
    SourceBound,
    PdfExtractionBound,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationOccurrenceDto {
    pub occurrence_id: String,
    pub research_case_id: String,
    pub origin: ResearchCitationOccurrenceOriginDto,
    pub rendered_text: String,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationTargetDto {
    pub target_id: String,
    pub citation_occurrence_id: String,
    pub ordinal: u32,
    pub reference_key: String,
    pub cited_locator: Option<String>,
    pub resolution: ResearchCitationTargetResolutionDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationTargetBindingDto {
    pub binding_id: String,
    pub research_case_id: String,
    pub citation_target_id: String,
    pub source_id: String,
    pub source_snapshot_id: Option<String>,
    pub extraction_id: Option<String>,
    pub method: ResearchCitationBindingMethodDto,
    pub resolution: ResearchCitationTargetResolutionDto,
    pub pdf_verification_ready: bool,
    pub created_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimCitationLinkDto {
    pub link_id: String,
    pub research_case_id: String,
    pub claim_id: String,
    pub citation_occurrence_id: String,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCitationOccurrenceRequest {
    pub research_case_id: String,
    pub origin: ResearchCitationOccurrenceOriginDto,
    pub rendered_text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCitationTargetRequest {
    pub citation_occurrence_id: String,
    pub ordinal: u32,
    pub reference_key: String,
    pub cited_locator: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCitationTargetBindingRequest {
    pub research_case_id: String,
    pub citation_target_id: String,
    pub source_id: String,
    pub source_snapshot_id: Option<String>,
    pub extraction_id: Option<String>,
    pub method: ResearchCitationBindingMethodDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateClaimCitationLinkRequest {
    pub research_case_id: String,
    pub claim_id: String,
    pub citation_occurrence_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchRetrievalIndexStatusDto {
    NotConfigured,
    Provisioning,
    Ready,
    Syncing,
    Failed,
    Degraded,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchRetrievalReadinessDto {
    pub provider: String,
    pub qualification_target: String,
    pub configured: bool,
    pub status: ResearchRetrievalReadinessStatusDto,
    pub reachable: bool,
    pub authorized: bool,
    pub ready: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchRetrievalReadinessStatusDto {
    NotConfigured,
    Configured,
    Unreachable,
    Reachable,
    Unauthorized,
    Ready,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchRetrievalIndexDto {
    pub index_id: String,
    pub research_case_id: String,
    pub dataset_id: String,
    pub status: ResearchRetrievalIndexStatusDto,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchExtractionRetrievalIndexDto {
    pub index_id: String,
    pub case_index_id: String,
    pub research_case_id: String,
    pub extraction_id: String,
    pub source_snapshot_id: String,
    pub document_id: Option<String>,
    pub metadata_qualified: bool,
    pub chunker_version: String,
    pub status: ResearchRetrievalIndexStatusDto,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ResearchRetrievalScopeDto {
    Case,
    Sources { source_ids: Vec<String> },
    Extractions { extraction_ids: Vec<String> },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchRetrievalIndexStateDto {
    pub readiness: ResearchRetrievalReadinessDto,
    pub case_index: Option<ResearchRetrievalIndexDto>,
    pub extraction_indexes: Vec<ResearchExtractionRetrievalIndexDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetrieveResearchRequest {
    pub query: String,
    pub top_k: Option<u32>,
    pub scope: Option<ResearchRetrievalScopeDto>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchRetrievalCandidateDto {
    pub retrieval_chunk_id: String,
    pub research_source_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub page: u32,
    pub start: u64,
    pub end: u64,
    pub verbatim_excerpt: String,
    pub retrieval_score: f64,
    pub provider: String,
    pub rank: u32,
}
