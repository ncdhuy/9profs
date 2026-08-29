//! Transport DTOs shared by HTTP and WebSocket boundaries.
//!
//! This crate intentionally has no web-framework dependency.

mod agent_run;
mod assistant;
mod docs_agent_profile;
mod document;
mod document_agent_conversation;
mod mcp;
mod research;
mod response;
mod runtime;
mod skill;
mod websocket;

pub use agent_run::{
    ActiveDocsAgentRunRequest, AgentRunContextDto, AgentRunDto, AgentRunRequest,
    AgentRunStartedDto, AgentTaskDto, AgentTaskFailureDto,
};
pub use assistant::{AssistantDto, CreateAssistantRequest, UpdateAssistantRequest};
pub use docs_agent_profile::{DocsAgentAvailability, DocsAgentProfile, DocsAgentReadiness};
pub use document::{ActiveDocumentDto, DocumentProposalChangeDto, DocumentProposalDto};
pub use document_agent_conversation::{
    CreateDocumentAgentConversationRequest, CreateDocumentAgentConversationRunRequest,
    DocumentAgentConversationDto,
};
pub use mcp::{
    CreateMcpServerRequest, McpConnectionTestDto, McpServerDto, McpToolDto, McpTransportDto,
    McpTransportInputDto, UpdateMcpServerRequest,
};
pub use research::{
    CaptureResearchPdfEvidenceRequest, CaptureResearchPdfExtractionRequest,
    CaptureResearchSourceSnapshotRequest, CitationExpectationAssessmentStatusDto,
    CitationExpectationDto, CitationOccurrenceDto, CitationReviewBlockCitationRequest,
    CitationReviewBlockRequest, CitationReviewCandidateDto, CitationReviewCitationRequest,
    CitationReviewEvidenceDto, CitationReviewItemDto, CitationReviewItemStatusDto,
    CitationReviewRunDto, CitationReviewRunStatusDto, CitationReviewTargetRequest,
    CitationReviewVerificationDto, CitationTargetBindingDto, CitationTargetDto,
    CitationVerificationCandidateDto, CitationVerificationEvidenceDto,
    CitationVerificationResultDto, CitationVerificationRunDto, CitationVerificationStatusDto,
    ClaimCitationLinkDto, ClaimEvidenceLinkDto, ClaimReviewKindDto, CoverageAttentionReasonDto,
    CoverageAttentionStateDto, CreateCitationOccurrenceRequest, CreateCitationTargetBindingRequest,
    CreateCitationTargetRequest, CreateCitationVerificationRequest, CreateClaimCitationLinkRequest,
    CreateClaimEvidenceLinkRequest, CreateManuscriptCitationExpectationRequest,
    CreateManuscriptClaimCoverageRequest, CreateManuscriptClaimExtractionRequest,
    CreateManuscriptClaimInventoryRequest, CreateManuscriptReferenceCatalogRequest,
    CreateResearchCaseRequest, CreateResearchClaimRequest, CreateResearchEvidenceRequest,
    CreateResearchSourceRequest, ManuscriptCitationExpectationItemDto,
    ManuscriptCitationExpectationRunDto, ManuscriptCitationExpectationRunStatusDto,
    ManuscriptCitationFormatDto, ManuscriptCitationSyncCitationRequest,
    ManuscriptCitationSyncOccurrenceDto, ManuscriptCitationSyncRunDto,
    ManuscriptCitationSyncStatusDto, ManuscriptCitationSyncTargetDto,
    ManuscriptCitationSyncTargetRequest, ManuscriptClaimCoverageBridgeStatusDto,
    ManuscriptClaimCoverageItemDto, ManuscriptClaimCoverageRunDto,
    ManuscriptClaimCoverageRunStatusDto, ManuscriptClaimCoverageStructuralCitationStateDto,
    ManuscriptClaimCoverageTargetDto, ManuscriptClaimExtractionBlockRequest,
    ManuscriptClaimExtractionCitationRequest, ManuscriptClaimExtractionCoverageDto,
    ManuscriptClaimExtractionCoverageStatusDto, ManuscriptClaimExtractionItemDto,
    ManuscriptClaimExtractionRunDto, ManuscriptClaimExtractionStatusDto,
    ManuscriptClaimInventoryBlockKindDto, ManuscriptClaimInventoryBlockRequest,
    ManuscriptClaimInventoryCitationRequest, ManuscriptClaimInventoryCoverageDto,
    ManuscriptClaimInventoryCoverageStatusDto, ManuscriptClaimInventoryItemDto,
    ManuscriptClaimInventoryRunDto, ManuscriptClaimInventoryStatusDto,
    ManuscriptReferenceCatalogCitationRequest, ManuscriptReferenceCatalogRunDto,
    ManuscriptReferenceCatalogStatusDto, ManuscriptReferenceCatalogTargetRequest,
    ManuscriptReferenceCatalogWordSourceRequest, ManuscriptReferenceCatalogZoteroRequest,
    ManuscriptReferenceEntryDto, ManuscriptReferenceResolutionCandidateDto,
    ManuscriptReferenceResolutionEntryDto, ManuscriptReferenceResolutionMatchKindDto,
    ManuscriptReferenceResolutionOutcomeDto, ManuscriptReferenceResolutionRunDto,
    ManuscriptReferenceResolutionStatusDto, ManuscriptReferenceTargetMappingDto,
    ManuscriptReferenceWordSourceDto, ManuscriptReferenceZoteroDto, ReferencePdfIngestionDto,
    ResearchArtifactDto, ResearchAssessmentMethodDto, ResearchCaptureMethodDto, ResearchCaseDto,
    ResearchCitationBindingMethodDto, ResearchCitationOccurrenceOriginDto,
    ResearchCitationTargetResolutionDto, ResearchClaimDto, ResearchClaimEvidenceRelationDto,
    ResearchClaimOriginDto, ResearchContentHashDto, ResearchEvidenceDto,
    ResearchEvidenceLocatorDto, ResearchExtractionRetrievalIndexDto, ResearchHashAlgorithmDto,
    ResearchPdfExtractionDto, ResearchPdfExtractionStatusDto, ResearchPdfPageDto,
    ResearchPdfPageListDto, ResearchRetrievalCandidateDto, ResearchRetrievalIndexDto,
    ResearchRetrievalIndexStateDto, ResearchRetrievalIndexStatusDto, ResearchRetrievalReadinessDto,
    ResearchRetrievalReadinessStatusDto, ResearchRetrievalScopeDto, ResearchSourceDto,
    ResearchSourceIdentityDto, ResearchSourceIdentityMethodDto, ResearchSourceIdentityRequest,
    ResearchSourceKindDto, ResearchSourceOriginDto, ResearchSourceSnapshotDto,
    RetrieveResearchRequest, StartManuscriptCitationReviewRequest, SyncManuscriptCitationsRequest,
};
pub use response::{ApiResponse, ErrorResponse};
pub use runtime::{HealthResponse, RuntimeInfo};
pub use skill::{SkillCatalogDto, SkillDto, SkillIssueDto};
pub use websocket::EventEnvelope;
