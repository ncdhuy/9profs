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
pub enum ResearchSourceIdentityMethodDto {
    Imported,
    HumanConfirmed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchSourceIdentityDto {
    pub provider: String,
    pub external_reference: String,
    pub method: ResearchSourceIdentityMethodDto,
    pub asserted_at_ms: i64,
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
    pub identity: Option<ResearchSourceIdentityDto>,
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
#[serde(rename_all = "snake_case")]
pub enum ManuscriptCitationFormatDto {
    WordNative,
    Zotero,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptCitationSyncStatusDto {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCitationSyncRunDto {
    pub sync_run_id: String,
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub inventory_hash: ResearchContentHashDto,
    pub status: ManuscriptCitationSyncStatusDto,
    pub occurrence_count: u32,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCitationSyncOccurrenceDto {
    pub sync_occurrence_id: String,
    pub sync_run_id: String,
    pub ordinal: u32,
    pub citation_occurrence_id: String,
    pub document_block_id: String,
    pub start: u64,
    pub end: u64,
    pub format: ManuscriptCitationFormatDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCitationSyncTargetDto {
    pub sync_target_id: String,
    pub sync_occurrence_id: String,
    pub document_target_ordinal: u32,
    pub citation_target_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptReferenceCatalogStatusDto {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReferenceWordSourceDto {
    pub tag: String,
    pub title: String,
    pub author: String,
    pub year: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReferenceZoteroDto {
    pub item_id: Option<String>,
    pub uris: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReferenceCatalogRunDto {
    pub catalog_run_id: String,
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub citation_sync_run_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub catalog_hash: ResearchContentHashDto,
    pub entry_count: u32,
    pub target_mapping_count: u32,
    pub status: ManuscriptReferenceCatalogStatusDto,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReferenceEntryDto {
    pub entry_id: String,
    pub catalog_run_id: String,
    pub ordinal: u32,
    pub format: ManuscriptCitationFormatDto,
    pub reference_key: String,
    pub descriptor_hash: ResearchContentHashDto,
    pub word_source: Option<ManuscriptReferenceWordSourceDto>,
    pub zotero: Option<ManuscriptReferenceZoteroDto>,
    pub target_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReferenceTargetMappingDto {
    pub mapping_id: String,
    pub catalog_run_id: String,
    pub reference_entry_id: String,
    pub citation_occurrence_id: String,
    pub citation_target_id: String,
    pub document_target_ordinal: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptReferenceResolutionStatusDto {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptReferenceResolutionOutcomeDto {
    ResolvedExact,
    AlreadyBound,
    AmbiguousSource,
    AmbiguousSnapshotOrExtraction,
    CandidateRequiresConfirmation,
    SourceMatchedButNotVerificationReady,
    Unresolved,
    ConflictWithExistingBinding,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptReferenceResolutionMatchKindDto {
    ExactZoteroItemId,
    ExactZoteroUri,
    ReferenceKeySourceLabel,
    ReferenceTitleSourceLabel,
    MappingIntegrity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReferenceResolutionRunDto {
    pub resolution_run_id: String,
    pub research_case_id: String,
    pub catalog_run_id: String,
    pub catalog_hash: ResearchContentHashDto,
    pub source_state_hash: ResearchContentHashDto,
    pub resolver_policy_version: String,
    pub status: ManuscriptReferenceResolutionStatusDto,
    pub entry_count: u32,
    pub resolved_entry_count: u32,
    pub candidate_entry_count: u32,
    pub unresolved_entry_count: u32,
    pub conflict_entry_count: u32,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReferenceResolutionEntryDto {
    pub resolution_entry_id: String,
    pub resolution_run_id: String,
    pub reference_entry_id: String,
    pub outcome: ManuscriptReferenceResolutionOutcomeDto,
    pub match_kind: Option<ManuscriptReferenceResolutionMatchKindDto>,
    pub chosen_source_id: Option<String>,
    pub chosen_source_snapshot_id: Option<String>,
    pub chosen_extraction_id: Option<String>,
    pub automatic_binding_permitted: bool,
    pub candidate_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReferenceResolutionCandidateDto {
    pub candidate_id: String,
    pub resolution_entry_id: String,
    pub ordinal: u32,
    pub source_id: String,
    pub source_snapshot_id: Option<String>,
    pub extraction_id: Option<String>,
    pub match_kind: ManuscriptReferenceResolutionMatchKindDto,
    pub automatic_binding_permitted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimExtractionStatusDto {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimExtractionCoverageStatusDto {
    AssociatedWithClaim,
    NoVerifiableClaim,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptClaimExtractionRunDto {
    pub extraction_run_id: String,
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub citation_sync_run_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub context_hash: ResearchContentHashDto,
    pub extractor_provider: String,
    pub extractor_version: String,
    pub extractor_model_id: Option<String>,
    pub extraction_contract_version: String,
    pub status: ManuscriptClaimExtractionStatusDto,
    pub claim_count: u32,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptClaimExtractionItemDto {
    pub item_id: String,
    pub extraction_run_id: String,
    pub research_claim_id: String,
    pub document_block_id: String,
    pub source_start: u64,
    pub source_end: u64,
    pub source_excerpt: String,
    pub source_excerpt_hash: ResearchContentHashDto,
    pub ordinal: u32,
    pub claim_text: String,
    pub citation_occurrence_ids: Vec<String>,
    pub claim_citation_link_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptClaimExtractionCoverageDto {
    pub coverage_id: String,
    pub extraction_run_id: String,
    pub extraction_item_id: Option<String>,
    pub claim_citation_link_id: Option<String>,
    pub citation_occurrence_id: String,
    pub status: ManuscriptClaimExtractionCoverageStatusDto,
    pub reason: Option<String>,
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
    pub identity: Option<ResearchSourceIdentityRequest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchSourceIdentityRequest {
    pub provider: String,
    pub external_reference: String,
    pub method: ResearchSourceIdentityMethodDto,
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
pub struct ManuscriptCitationSyncTargetRequest {
    pub ordinal: u32,
    pub reference_key: String,
    #[serde(default)]
    pub cited_locator: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManuscriptCitationSyncCitationRequest {
    pub format: ManuscriptCitationFormatDto,
    pub rendered_text: String,
    pub block_id: String,
    pub start: u64,
    pub end: u64,
    pub targets: Vec<ManuscriptCitationSyncTargetRequest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncManuscriptCitationsRequest {
    pub document_id: String,
    pub document_version: i64,
    pub citations: Vec<ManuscriptCitationSyncCitationRequest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManuscriptReferenceCatalogWordSourceRequest {
    pub tag: String,
    pub title: String,
    pub author: String,
    pub year: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManuscriptReferenceCatalogZoteroRequest {
    pub item_id: Option<String>,
    #[serde(default)]
    pub uris: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManuscriptReferenceCatalogTargetRequest {
    pub citation_target_id: String,
    pub ordinal: u32,
    pub reference_key: String,
    pub word_source: Option<ManuscriptReferenceCatalogWordSourceRequest>,
    pub zotero: Option<ManuscriptReferenceCatalogZoteroRequest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManuscriptReferenceCatalogCitationRequest {
    pub citation_occurrence_id: String,
    pub block_id: String,
    pub start: u64,
    pub end: u64,
    pub format: ManuscriptCitationFormatDto,
    pub targets: Vec<ManuscriptReferenceCatalogTargetRequest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateManuscriptReferenceCatalogRequest {
    pub document_id: String,
    pub document_version: i64,
    pub citations: Vec<ManuscriptReferenceCatalogCitationRequest>,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManuscriptClaimExtractionCitationRequest {
    pub citation_occurrence_id: String,
    pub start: u64,
    pub end: u64,
    pub rendered_text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManuscriptClaimExtractionBlockRequest {
    pub block_id: String,
    pub text: String,
    pub citations: Vec<ManuscriptClaimExtractionCitationRequest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateManuscriptClaimExtractionRequest {
    pub document_id: String,
    pub document_version: i64,
    pub blocks: Vec<ManuscriptClaimExtractionBlockRequest>,
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationVerificationStatusDto {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationVerificationCandidateDto {
    pub verification_run_id: String,
    pub retrieval_chunk_id: String,
    pub research_source_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub page: u32,
    pub start: u64,
    pub end: u64,
    pub excerpt_hash: String,
    pub rank: u32,
    pub retrieval_score: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationVerificationResultDto {
    pub verification_run_id: String,
    pub overall_relation: ResearchClaimEvidenceRelationDto,
    pub rationale: String,
    pub assessor_provider: String,
    pub assessor_version: String,
    pub assessor_model_id: Option<String>,
    pub assessment_contract_version: String,
    pub completed_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationVerificationEvidenceDto {
    pub verification_run_id: String,
    pub retrieval_chunk_id: String,
    pub evidence_id: String,
    pub claim_evidence_link_id: String,
    pub relation: ResearchClaimEvidenceRelationDto,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationVerificationRunDto {
    pub run_id: String,
    pub research_case_id: String,
    pub claim_citation_link_id: String,
    pub citation_target_binding_id: String,
    pub claim_id: String,
    pub citation_occurrence_id: String,
    pub citation_target_id: String,
    pub source_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub status: CitationVerificationStatusDto,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub result: Option<CitationVerificationResultDto>,
    pub candidates: Vec<CitationVerificationCandidateDto>,
    pub evidence: Vec<CitationVerificationEvidenceDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationReviewRunStatusDto {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationReviewItemStatusDto {
    UnresolvedReference,
    AmbiguousReference,
    ReferenceRequiresConfirmation,
    SourceMatchedNotVerificationReady,
    BindingConflict,
    ReadyForVerification,
    VerificationRunning,
    VerificationCompleted,
    VerificationFailed,
    ResolutionFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartManuscriptCitationReviewRequest {
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub citation_sync_run_id: String,
    pub reference_catalog_run_id: String,
    pub reference_resolution_run_id: String,
    pub claim_extraction_run_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewRunDto {
    pub review_run_id: String,
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub citation_sync_run_id: Option<String>,
    pub reference_catalog_run_id: Option<String>,
    pub reference_resolution_run_id: Option<String>,
    pub claim_extraction_run_id: Option<String>,
    pub status: CitationReviewRunStatusDto,
    pub failure_stage: Option<String>,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewCandidateDto {
    pub candidate_id: String,
    pub resolution_entry_id: String,
    pub ordinal: u32,
    pub source_id: String,
    pub source_label: Option<String>,
    pub source_snapshot_id: Option<String>,
    pub extraction_id: Option<String>,
    pub match_kind: Option<ManuscriptReferenceResolutionMatchKindDto>,
    pub automatic_binding_permitted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewVerificationDto {
    pub verification_run_id: String,
    pub status: CitationVerificationStatusDto,
    pub failure_code: Option<String>,
    pub relation: Option<ResearchClaimEvidenceRelationDto>,
    pub rationale: Option<String>,
    pub assessor_provider: Option<String>,
    pub assessor_version: Option<String>,
    pub assessor_model_id: Option<String>,
    pub completed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewEvidenceDto {
    pub evidence_id: String,
    pub relation: ResearchClaimEvidenceRelationDto,
    pub source_snapshot_id: String,
    pub extraction_id: Option<String>,
    pub locator: ResearchEvidenceLocatorDto,
    pub verbatim_excerpt: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewItemDto {
    pub item_id: String,
    pub review_run_id: String,
    pub ordinal: u32,
    pub claim_id: String,
    pub claim_citation_link_id: String,
    pub citation_occurrence_id: String,
    pub citation_target_id: String,
    pub reference_entry_id: Option<String>,
    pub resolution_entry_id: Option<String>,
    pub resolution_outcome: Option<ManuscriptReferenceResolutionOutcomeDto>,
    pub document_block_id: String,
    pub start: u64,
    pub end: u64,
    pub rendered_text: String,
    pub reference_key: String,
    pub cited_locator: Option<String>,
    pub claim_text: String,
    pub source_excerpt: Option<String>,
    pub binding_id: Option<String>,
    pub binding_method: Option<ResearchCitationBindingMethodDto>,
    pub source_id: Option<String>,
    pub source_snapshot_id: Option<String>,
    pub extraction_id: Option<String>,
    pub status: CitationReviewItemStatusDto,
    pub failure_code: Option<String>,
    pub candidates: Vec<CitationReviewCandidateDto>,
    pub verification: Option<CitationReviewVerificationDto>,
    pub evidence: Vec<CitationReviewEvidenceDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCitationVerificationRequest {
    pub claim_citation_link_id: String,
    pub citation_target_binding_id: String,
}
