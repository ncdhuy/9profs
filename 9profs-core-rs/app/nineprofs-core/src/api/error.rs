use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use nineprofs_api_types::ErrorResponse;
use nineprofs_assistant::AssistantError;
use nineprofs_document_tools::ProposalStoreError;
use nineprofs_document_tools::ProposalWorkflowError;
use nineprofs_mcp::McpError;
use nineprofs_research::ResearchError;
use nineprofs_research_dify::DifyError;
use nineprofs_research_verification::{CitationReviewError, CitationVerificationError};
use nineprofs_runtime::AgentExecutionServiceError;

#[derive(Debug)]
pub(crate) enum ApiError {
    Assistant(AssistantError),
    Execution(AgentExecutionServiceError),
    Mcp(McpError),
    Task(nineprofs_agent::AgentTaskManagerError),
    NotFound(String),
    AgentNotFound(String),
    RunNotFound(String),
    DocumentNotFound(String),
    DocumentProposalNotFound(String),
    ConversationNotFound(String),
    ProposalWorkflow(ProposalWorkflowError),
    Research(ResearchError),
    Dify(DifyError),
    Verification(CitationVerificationError),
    CitationReview(CitationReviewError),
    InvalidRequest(String),
    Unauthorized,
}

impl From<AssistantError> for ApiError {
    fn from(error: AssistantError) -> Self {
        Self::Assistant(error)
    }
}

impl From<AgentExecutionServiceError> for ApiError {
    fn from(error: AgentExecutionServiceError) -> Self {
        Self::Execution(error)
    }
}

impl From<McpError> for ApiError {
    fn from(error: McpError) -> Self {
        Self::Mcp(error)
    }
}

impl From<nineprofs_agent::AgentTaskManagerError> for ApiError {
    fn from(error: nineprofs_agent::AgentTaskManagerError) -> Self {
        Self::Task(error)
    }
}

impl From<ProposalWorkflowError> for ApiError {
    fn from(error: ProposalWorkflowError) -> Self {
        Self::ProposalWorkflow(error)
    }
}

impl From<ResearchError> for ApiError {
    fn from(error: ResearchError) -> Self {
        Self::Research(error)
    }
}

impl From<DifyError> for ApiError {
    fn from(error: DifyError) -> Self {
        Self::Dify(error)
    }
}

impl From<CitationVerificationError> for ApiError {
    fn from(error: CitationVerificationError) -> Self {
        Self::Verification(error)
    }
}

impl From<CitationReviewError> for ApiError {
    fn from(error: CitationReviewError) -> Self {
        Self::CitationReview(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::NotFound(id) => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("skill not found: {id}"),
            ),
            Self::AgentNotFound(id) => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("agent backend not found: {id}"),
            ),
            Self::RunNotFound(id) => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("agent run not found: {id}"),
            ),
            Self::DocumentNotFound(id) => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("active document not found: {id}"),
            ),
            Self::DocumentProposalNotFound(id) => (
                StatusCode::NOT_FOUND,
                "not_found",
                format!("document proposal not found: {id}"),
            ),
            Self::ConversationNotFound(id) => (
                StatusCode::NOT_FOUND,
                "conversation_not_found",
                format!("Docs agent conversation not found: {id}"),
            ),
            Self::ProposalWorkflow(error) => match error {
                ProposalWorkflowError::Store(ProposalStoreError::NotFound(id)) => (
                    StatusCode::NOT_FOUND,
                    "not_found",
                    format!("document proposal not found: {id}"),
                ),
                ProposalWorkflowError::Store(ProposalStoreError::InvalidState {
                    proposal_id,
                    action,
                    status,
                }) => (
                    StatusCode::CONFLICT,
                    "proposal_state_conflict",
                    format!(
                        "document proposal {proposal_id} cannot be {action} from status {status}"
                    ),
                ),
                ProposalWorkflowError::Stale { requested, current } => (
                    StatusCode::CONFLICT,
                    "proposal_stale",
                    format!(
                        "active document version is stale: requested {requested}, current {current}"
                    ),
                ),
                ProposalWorkflowError::Unavailable(id) => (
                    StatusCode::CONFLICT,
                    "proposal_unavailable",
                    format!("active document is unavailable: {id}"),
                ),
                ProposalWorkflowError::UnsupportedDocument(message) => {
                    (StatusCode::CONFLICT, "proposal_unsupported", message)
                }
                ProposalWorkflowError::Store(error) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    error.to_string(),
                ),
            },
            Self::Research(error) => match error {
                ResearchError::NotFound { .. } => {
                    (StatusCode::NOT_FOUND, "not_found", error.to_string())
                }
                ResearchError::ManuscriptCitationSyncConflict { .. } => (
                    StatusCode::CONFLICT,
                    "manuscript_citation_sync_conflict",
                    error.to_string(),
                ),
                ResearchError::ManuscriptReferenceCatalogStale => (
                    StatusCode::CONFLICT,
                    "reference_catalog_stale",
                    error.to_string(),
                ),
                ResearchError::ManuscriptReferenceCatalogConflict { .. } => (
                    StatusCode::CONFLICT,
                    "reference_catalog_conflict",
                    error.to_string(),
                ),
                ResearchError::ManuscriptReferenceDescriptorConflict { .. } => (
                    StatusCode::CONFLICT,
                    "reference_descriptor_conflict",
                    error.to_string(),
                ),
                ResearchError::ManuscriptClaimExtractorNotConfigured => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "extractor_not_configured",
                    error.to_string(),
                ),
                ResearchError::ManuscriptClaimExtractorInvalidConfiguration(_) => (
                    StatusCode::BAD_REQUEST,
                    "invalid_configuration",
                    error.to_string(),
                ),
                ResearchError::ManuscriptClaimExtractionStale => (
                    StatusCode::CONFLICT,
                    "citation_sync_stale",
                    error.to_string(),
                ),
                ResearchError::ManuscriptClaimExtractionFailed(_) => (
                    StatusCode::BAD_GATEWAY,
                    "claim_extraction_failed",
                    error.to_string(),
                ),
                ResearchError::ManuscriptClaimInventoryExtractorNotConfigured => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "extractor_not_configured",
                    error.to_string(),
                ),
                ResearchError::ManuscriptClaimInventoryExtractorInvalidConfiguration(_) => (
                    StatusCode::BAD_REQUEST,
                    "invalid_configuration",
                    error.to_string(),
                ),
                ResearchError::ManuscriptClaimInventoryFailed(_) => (
                    StatusCode::BAD_GATEWAY,
                    "claim_inventory_failed",
                    error.to_string(),
                ),
                ResearchError::Invalid(_) => (
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    error.to_string(),
                ),
                ResearchError::Database(_)
                | ResearchError::Serialization(_)
                | ResearchError::Artifact(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    error.to_string(),
                ),
            },
            Self::Dify(error) => {
                let status = match error {
                    DifyError::Invalid(_) => StatusCode::BAD_REQUEST,
                    DifyError::NotConfigured => StatusCode::SERVICE_UNAVAILABLE,
                    DifyError::Unauthorized => StatusCode::BAD_GATEWAY,
                    DifyError::RemoteNotFound => StatusCode::NOT_FOUND,
                    DifyError::IndexDrift | DifyError::Integrity | DifyError::IndexingFailed => {
                        StatusCode::CONFLICT
                    }
                    DifyError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
                    DifyError::Unreachable
                    | DifyError::ProviderNotInitialized
                    | DifyError::Timeout
                    | DifyError::MalformedResponse
                    | DifyError::Database(_)
                    | DifyError::Research(_) => StatusCode::BAD_GATEWAY,
                };
                (status, "dify_error", error.to_string())
            }
            Self::Verification(error) => {
                let status = match error {
                    CitationVerificationError::ClaimCitationLinkNotFound
                    | CitationVerificationError::ClaimNotFound
                    | CitationVerificationError::CitationOccurrenceNotFound
                    | CitationVerificationError::CitationTargetNotFound
                    | CitationVerificationError::CitationBindingNotFound
                    | CitationVerificationError::NotFound => StatusCode::NOT_FOUND,
                    CitationVerificationError::CitationChainMismatch
                    | CitationVerificationError::BindingNotPdfReady
                    | CitationVerificationError::RetrievalIndexNotReady
                    | CitationVerificationError::AssessorInvalidOutput
                    | CitationVerificationError::CandidateUnknown
                    | CitationVerificationError::CandidateIntegrityFailed => StatusCode::CONFLICT,
                    CitationVerificationError::RetrievalNotConfigured
                    | CitationVerificationError::AssessorNotConfigured => {
                        StatusCode::SERVICE_UNAVAILABLE
                    }
                    CitationVerificationError::RetrievalFailed
                    | CitationVerificationError::AssessorFailed => StatusCode::BAD_GATEWAY,
                    CitationVerificationError::EvidencePromotionFailed
                    | CitationVerificationError::PersistenceInvalid(_)
                    | CitationVerificationError::Research(_)
                    | CitationVerificationError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
                };
                (status, error.code(), error.to_string())
            }
            Self::CitationReview(error) => {
                let status = match error {
                    CitationReviewError::NotFound(_) => StatusCode::NOT_FOUND,
                    CitationReviewError::Invalid(_) => StatusCode::BAD_REQUEST,
                    CitationReviewError::Research(ResearchError::NotFound { .. }) => {
                        StatusCode::NOT_FOUND
                    }
                    CitationReviewError::Research(ResearchError::Invalid(_)) => {
                        StatusCode::BAD_REQUEST
                    }
                    CitationReviewError::Research(_)
                    | CitationReviewError::Verification(_)
                    | CitationReviewError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
                };
                (status, error.code(), error.to_string())
            }
            Self::InvalidRequest(message) => (StatusCode::BAD_REQUEST, "invalid_request", message),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "trusted document decision authentication required".to_owned(),
            ),
            Self::Task(error) => (
                if matches!(error, nineprofs_agent::AgentTaskManagerError::NotFound(_)) {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::CONFLICT
                },
                "task_error",
                error.to_string(),
            ),
            Self::Execution(error) => {
                let (status, code) = match &error {
                    AgentExecutionServiceError::Assistant(AssistantError::NotFound(_)) => {
                        (StatusCode::NOT_FOUND, "not_found")
                    }
                    AgentExecutionServiceError::ActiveDocumentUnavailable(_) => {
                        (StatusCode::BAD_REQUEST, "active_document_unavailable")
                    }
                    AgentExecutionServiceError::ActiveDocumentUnsupported(_) => {
                        (StatusCode::BAD_REQUEST, "active_document_unsupported")
                    }
                    AgentExecutionServiceError::ConversationNotFound(_) => {
                        (StatusCode::NOT_FOUND, "conversation_not_found")
                    }
                    AgentExecutionServiceError::ConversationBusy(_) => {
                        (StatusCode::CONFLICT, "conversation_busy")
                    }
                    AgentExecutionServiceError::ConversationUnavailable(_) => {
                        (StatusCode::CONFLICT, "conversation_unavailable")
                    }
                    AgentExecutionServiceError::ConversationCapacity => {
                        (StatusCode::SERVICE_UNAVAILABLE, "conversation_capacity")
                    }
                    AgentExecutionServiceError::ConversationTurnLimit => {
                        (StatusCode::CONFLICT, "conversation_turn_limit")
                    }
                    AgentExecutionServiceError::RequiredToolMissing(_) => {
                        (StatusCode::SERVICE_UNAVAILABLE, "required_tool_missing")
                    }
                    AgentExecutionServiceError::BackendUnavailable(_, _) => {
                        (StatusCode::SERVICE_UNAVAILABLE, "agent_execution_error")
                    }
                    AgentExecutionServiceError::BackendMissing(_)
                    | AgentExecutionServiceError::BackendNotConfigured
                    | AgentExecutionServiceError::BackendDisabled(_)
                    | AgentExecutionServiceError::ExecutorMissing(_) => {
                        (StatusCode::BAD_REQUEST, "agent_execution_error")
                    }
                    _ => (StatusCode::BAD_REQUEST, "agent_execution_error"),
                };
                (status, code, error.to_string())
            }
            Self::Mcp(error) => {
                let (status, code) = match &error {
                    McpError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
                    McpError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
                    McpError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
                    McpError::Connection(_) => {
                        (StatusCode::SERVICE_UNAVAILABLE, "mcp_connection_error")
                    }
                    McpError::Timeout(_) => (StatusCode::GATEWAY_TIMEOUT, "mcp_connection_timeout"),
                    McpError::Database(_) | McpError::ToolRegistry(_) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                    }
                };
                (status, code, error.to_string())
            }
            Self::Assistant(error) => match &error {
                AssistantError::NotFound(_) => {
                    (StatusCode::NOT_FOUND, "not_found", error.to_string())
                }
                AssistantError::BuiltinReadOnly(_) => {
                    (StatusCode::CONFLICT, "builtin_read_only", error.to_string())
                }
                AssistantError::Invalid(_)
                | AssistantError::MissingSkill(_)
                | AssistantError::DuplicateSkill(_) => (
                    StatusCode::BAD_REQUEST,
                    "invalid_request",
                    error.to_string(),
                ),
                AssistantError::Database(_)
                | AssistantError::Builtin(_)
                | AssistantError::Skills(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    error.to_string(),
                ),
            },
        };
        (status, axum::Json(ErrorResponse::new(message, code))).into_response()
    }
}
