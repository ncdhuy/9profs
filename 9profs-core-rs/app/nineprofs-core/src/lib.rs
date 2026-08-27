use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Router,
    body::Body,
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::StreamExt;
use nineprofs_agent::{AgentBackendDescriptor, AgentRunContext, AgentTaskId, RunId, TaskState};
use nineprofs_api_types::{
    ActiveDocsAgentRunRequest, ActiveDocumentDto, AgentRunContextDto, AgentRunDto, AgentRunRequest,
    AgentRunStartedDto, AgentTaskDto, AgentTaskFailureDto, ApiResponse, AssistantDto,
    CaptureResearchPdfEvidenceRequest, CaptureResearchPdfExtractionRequest,
    CaptureResearchSourceSnapshotRequest, CitationOccurrenceDto, CitationTargetBindingDto,
    CitationTargetDto, CitationVerificationCandidateDto, CitationVerificationEvidenceDto,
    CitationVerificationResultDto, CitationVerificationRunDto, CitationVerificationStatusDto,
    ClaimCitationLinkDto, ClaimEvidenceLinkDto, CreateAssistantRequest,
    CreateCitationOccurrenceRequest, CreateCitationTargetBindingRequest,
    CreateCitationTargetRequest, CreateCitationVerificationRequest, CreateClaimCitationLinkRequest,
    CreateClaimEvidenceLinkRequest, CreateDocumentAgentConversationRequest,
    CreateDocumentAgentConversationRunRequest, CreateManuscriptClaimExtractionRequest,
    CreateManuscriptReferenceCatalogRequest, CreateMcpServerRequest, CreateResearchCaseRequest,
    CreateResearchClaimRequest, CreateResearchEvidenceRequest, CreateResearchSourceRequest,
    DocsAgentProfile, DocumentAgentConversationDto, DocumentProposalChangeDto, DocumentProposalDto,
    ErrorResponse, EventEnvelope, HealthResponse, ManuscriptCitationFormatDto,
    ManuscriptCitationSyncCitationRequest, ManuscriptCitationSyncOccurrenceDto,
    ManuscriptCitationSyncRunDto, ManuscriptCitationSyncStatusDto, ManuscriptCitationSyncTargetDto,
    ManuscriptCitationSyncTargetRequest, ManuscriptClaimExtractionCoverageDto,
    ManuscriptClaimExtractionCoverageStatusDto, ManuscriptClaimExtractionItemDto,
    ManuscriptClaimExtractionRunDto, ManuscriptClaimExtractionStatusDto,
    ManuscriptReferenceCatalogCitationRequest, ManuscriptReferenceCatalogRunDto,
    ManuscriptReferenceCatalogStatusDto, ManuscriptReferenceCatalogTargetRequest,
    ManuscriptReferenceEntryDto, ManuscriptReferenceTargetMappingDto,
    ManuscriptReferenceWordSourceDto, ManuscriptReferenceZoteroDto, McpConnectionTestDto,
    McpServerDto, McpToolDto, McpTransportDto, McpTransportInputDto, ReferencePdfIngestionDto,
    ResearchArtifactDto, ResearchAssessmentMethodDto, ResearchCaptureMethodDto, ResearchCaseDto,
    ResearchCitationBindingMethodDto, ResearchCitationOccurrenceOriginDto,
    ResearchCitationTargetResolutionDto, ResearchClaimDto, ResearchClaimEvidenceRelationDto,
    ResearchClaimOriginDto, ResearchContentHashDto, ResearchEvidenceDto,
    ResearchEvidenceLocatorDto, ResearchExtractionRetrievalIndexDto, ResearchHashAlgorithmDto,
    ResearchPdfExtractionDto, ResearchPdfExtractionStatusDto, ResearchPdfPageDto,
    ResearchPdfPageListDto, ResearchRetrievalCandidateDto, ResearchRetrievalIndexDto,
    ResearchRetrievalIndexStateDto, ResearchRetrievalIndexStatusDto, ResearchRetrievalReadinessDto,
    ResearchRetrievalReadinessStatusDto, ResearchRetrievalScopeDto, ResearchSourceDto,
    ResearchSourceKindDto, ResearchSourceOriginDto, ResearchSourceSnapshotDto,
    RetrieveResearchRequest, RuntimeInfo, SkillCatalogDto, SkillDto, SkillIssueDto,
    SyncManuscriptCitationsRequest, UpdateAssistantRequest, UpdateMcpServerRequest,
};
use nineprofs_assistant::{Assistant, AssistantError, CreateAssistant, UpdateAssistant};
use nineprofs_document_tools::{
    DocumentProposalView, ProposalAvailability, ProposalFreshness, ProposalStoreError,
    ProposalWorkflowError,
};
use nineprofs_documents::ActiveDocumentDescriptor;
use nineprofs_mcp::{
    CreateMcpServer, McpError, McpServerSnapshot, McpTransportConfig, McpTransportSummary,
    UpdateMcpServer,
};
use nineprofs_officecli::OfficeCliStatus;
use nineprofs_research::{
    AssessmentMethod, CaptureMethod, CapturePdfEvidence, CapturePdfExtraction, CapturePdfPage,
    CaptureSourceSnapshot, CitationBindingMethod, CitationOccurrenceOrigin, ClaimEvidenceRelation,
    ClaimOrigin, CreateCitationOccurrence, CreateCitationTarget, CreateCitationTargetBinding,
    CreateClaimCitationLink, CreateClaimEvidenceLink, CreateResearchCase, CreateResearchClaim,
    CreateResearchEvidence, CreateResearchSource, EvidenceLocator, ExtractManuscriptClaims,
    HashAlgorithm, ManuscriptClaimExtractionBlockInput, ManuscriptClaimExtractionCitationInput,
    ResearchCase, ResearchClaim, ResearchError, ResearchEvidence, ResearchPdfExtraction,
    ResearchPdfExtractionId, ResearchPdfPage, ResearchPdfPageBatch, ResearchRetrievalScope,
    ResearchSource, ResearchSourceId, ResearchSourceSnapshot, SourceKind, SourceOrigin,
};
use nineprofs_research_dify::{
    DifyCaseIndex, DifyError, DifyExtractionIndex, DifyIndexStatus, DifyReadiness,
    RetrievalCandidate, RetrievalIndexState,
};
use nineprofs_research_verification::{
    CitationVerificationError, CitationVerificationRun, CreateCitationVerification,
};
use nineprofs_runtime::{AgentExecutionServiceError, CoreRuntime};
use nineprofs_skills::{Skill, SkillSource};

#[derive(Clone)]
struct AppState {
    runtime: Arc<CoreRuntime>,
}

#[cfg(test)]
mod agent_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn agent_list_get_and_unknown_backend_api_work() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let router = build_router(runtime);

        let response = router
            .clone()
            .oneshot(Request::get("/api/agents").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["success"], true);
        assert!(
            payload["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|agent| { agent["id"] == "codex" && agent["availability"] == "unknown" })
        );

        let response = router
            .clone()
            .oneshot(
                Request::get("/api/agents/codex")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["data"]["id"], "codex");

        let response = router
            .oneshot(
                Request::get("/api/agents/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["code"], "not_found");
        assert_eq!(payload["error"], "agent backend not found: does-not-exist");
    }

    #[tokio::test]
    async fn document_agent_profile_is_global_and_secret_free() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let router = build_router(runtime);
        let response = router
            .oneshot(
                Request::get("/api/document-agent-profile")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["data"]["defaultAssistantId"], "document-foundation");
        assert_eq!(payload["data"]["backendId"], "nineprofs-default");
        assert!(
            payload["data"]["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|capability| capability == "document.list_active")
        );
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(!serialized.contains("api_key"));
        assert!(!serialized.contains("credential"));
        assert!(!serialized.contains("session_secret"));
    }

    #[tokio::test]
    async fn docs_conversation_api_creates_bound_safe_metadata() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        runtime
            .agent_registry()
            .set_availability(
                "nineprofs-default",
                nineprofs_agent::AvailabilityState::Available,
                None,
            )
            .await
            .unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::channel(8);
        runtime
            .document_bridge()
            .register(
                nineprofs_documents::DocumentRegistration {
                    protocol_version: nineprofs_documents::DOCUMENT_BRIDGE_PROTOCOL_VERSION
                        .to_owned(),
                    document_id: "doc-conversation".to_owned(),
                    document_type: nineprofs_documents::DOCX_DOCUMENT_TYPE.to_owned(),
                    version: 1,
                    capabilities: vec![
                        nineprofs_documents::DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        nineprofs_documents::DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();
        let router = build_router(runtime);
        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-agent-conversations")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "assistant_id": "document-foundation",
                            "document_id": "doc-conversation"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["data"]["assistantId"], "document-foundation");
        assert_eq!(payload["data"]["documentId"], "doc-conversation");
        assert_eq!(payload["data"]["state"], "idle");
        let conversation_id = payload["data"]["conversationId"].as_str().unwrap();
        assert!(conversation_id.starts_with("docs-"));
        let serialized = serde_json::to_string(&payload).unwrap();
        assert!(!serialized.contains("backend"));
        assert!(!serialized.contains("credential"));

        let response = router
            .oneshot(
                Request::get(format!(
                    "/api/document-agent-conversations/{conversation_id}"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn active_docs_agent_run_rejects_unavailable_and_unsupported_documents() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let (sender, _receiver) = tokio::sync::mpsc::channel(8);
        runtime
            .document_bridge()
            .register(
                nineprofs_documents::DocumentRegistration {
                    protocol_version: nineprofs_documents::DOCUMENT_BRIDGE_PROTOCOL_VERSION
                        .to_owned(),
                    document_id: "pdf-a".to_owned(),
                    document_type: "pdf".to_owned(),
                    version: 1,
                    capabilities: vec![
                        nineprofs_documents::DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        nineprofs_documents::DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();
        let router = build_router(runtime);

        for (document_id, code) in [
            ("missing", "active_document_unavailable"),
            ("pdf-a", "active_document_unsupported"),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::post("/api/document-agent-runs")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "assistant_id": "execution-assistant",
                                "document_id": document_id,
                                "input": "inspect document"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["code"], code);
        }
    }

    #[tokio::test]
    async fn active_docs_agent_run_requires_configured_session_secret() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("run-test-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(runtime);
        let body = serde_json::json!({
            "assistant_id": "execution-assistant",
            "document_id": "missing",
            "input": "inspect document"
        })
        .to_string();

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-agent-runs")
                    .header("content-type", "application/json")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .oneshot(
                Request::post("/api/document-agent-runs")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "run-test-secret")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[cfg(test)]
mod mcp_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn mcp_crud_redacts_secrets_and_validates_transport() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let router = build_router(runtime);
        let request = Request::post("/api/mcp/servers")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "id": "api-fixture",
                    "name": "API fixture",
                    "enabled": false,
                    "transport": {
                        "type": "stdio",
                        "command": "fixture",
                        "env": {"TOKEN": "never-return-this"}
                    }
                })
                .to_string(),
            ))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(!text.contains("never-return-this"));
        assert!(text.contains("TOKEN"));

        let response = router
            .clone()
            .oneshot(
                Request::put("/api/mcp/servers/api-fixture")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"description":"updated"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/mcp/servers/api-fixture/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = router
            .clone()
            .oneshot(
                Request::delete("/api/mcp/servers/api-fixture")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = router
            .oneshot(
                Request::post("/api/mcp/servers")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name":"invalid","transport":{"type":"sse","url":"file:///tmp"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[cfg(test)]
mod officecli_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn officecli_status_is_safe_and_core_starts_without_sidecar() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let router = build_router(runtime);
        let response = router
            .oneshot(
                Request::get("/api/officecli/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["success"], true);
        assert!(payload["data"]["supported_version"] == "1.0.144");
        assert!(payload["data"].get("binary_path").is_none());
        assert!(payload["data"].get("profile_root").is_none());
    }
}

#[cfg(test)]
mod document_proposal_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use nineprofs_documents::{
        DOCUMENT_BRIDGE_CAPABILITY_COMMIT, DOCUMENT_BRIDGE_CAPABILITY_INSPECT,
        DOCUMENT_BRIDGE_PROTOCOL_VERSION, DOCX_DOCUMENT_TYPE, DocumentRegistration,
    };
    use tokio::sync::mpsc;
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn document_metadata_and_proposal_apis_are_read_only_and_safe() {
        let runtime = Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        );
        let (sender, _receiver) = mpsc::channel(8);
        runtime
            .document_bridge()
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: "api-doc".to_owned(),
                    document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                    version: 5,
                    capabilities: vec![
                        DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();
        let router = build_router(runtime);

        let response = router
            .clone()
            .oneshot(Request::get("/api/documents").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let text = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(text.contains("api-doc"));
        assert!(!text.contains("sessionId"));

        let response = router
            .clone()
            .oneshot(
                Request::get("/api/document-proposals?documentId=api-doc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["data"], serde_json::json!([]));

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-proposals")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

        let response = router
            .oneshot(
                Request::get("/api/document-proposals/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn trusted_decision_endpoints_require_session_secret_without_echoing_it() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("approval-test-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(runtime);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-proposals/missing/reject")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/document-proposals/missing/reject")
                    .header(TRUSTED_DECISION_HEADER, "wrong-secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .oneshot(
                Request::post("/api/document-proposals/missing/reject")
                    .header(TRUSTED_DECISION_HEADER, "approval-test-secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(
            !String::from_utf8(body.to_vec())
                .unwrap()
                .contains("approval-test-secret")
        );
    }
}

#[cfg(test)]
mod research_pdf_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn reference_pdf_api_streams_authenticates_and_derives_exact_evidence() {
        let root = std::env::temp_dir().join(format!(
            "9profs-core-reference-pdf-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.data_dir = root.clone();
        config.database_path = root.join("core.db");
        config.session_secret = Some(Arc::from("api-test-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let case = runtime
            .research_service()
            .create_case(CreateResearchCase {
                title: "Reference PDF API".to_owned(),
            })
            .await
            .unwrap();
        let router = build_router(runtime);
        let pdf = b"%PDF-1.7\nfixture".to_vec();
        let upload_path = format!("/api/research/cases/{}/reference-pdfs", case.id.as_str());

        let response = router
            .clone()
            .oneshot(
                Request::post(&upload_path)
                    .header("content-type", "application/pdf")
                    .body(Body::from(pdf.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post(&upload_path)
                    .header("content-type", "application/pdf")
                    .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                    .header(
                        "x-nineprofs-original-filename",
                        r"C:\\imports\\reference.pdf",
                    )
                    .header("x-nineprofs-source-label", "Reference PDF")
                    .body(Body::from(pdf))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(body["data"]["artifact"]["artifactId"].is_string());
        assert!(body["data"]["snapshot"]["contentHash"]["value"].is_string());
        assert!(body.get("path").is_none());
        assert!(!body.to_string().contains("C:\\\\imports"));
        let snapshot_id = body["data"]["snapshot"]["snapshotId"].as_str().unwrap();

        let page_text = "Điều trị giảm tử vong 😀 20%.";
        let response = router
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/research/snapshots/{snapshot_id}/pdf-extraction"
                ))
                .header("content-type", "application/json")
                .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                .body(Body::from(
                    serde_json::json!({
                        "extractor": "pdfjs",
                        "extractorVersion": "api-test",
                        "pageCount": 1,
                        "status": "ready",
                        "pages": [{ "page": 1, "text": page_text }]
                    })
                    .to_string(),
                ))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let extraction_id = body["data"]["extractionId"].as_str().unwrap();

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/pdf-extractions/{extraction_id}/pages/1"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let page: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(page["data"]["text"], page_text);

        let start = page_text[..page_text.find("giảm tử vong").unwrap()]
            .chars()
            .count() as u64;
        let end = start + "giảm tử vong".chars().count() as u64;
        let response = router
            .oneshot(
                Request::post("/api/research/pdf-evidence")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                    .body(Body::from(
                        serde_json::json!({
                            "researchCaseId": case.id.as_str(),
                            "sourceSnapshotId": snapshot_id,
                            "extractionId": extraction_id,
                            "page": 1,
                            "start": start,
                            "end": end
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let evidence: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(evidence["data"]["verbatimExcerpt"], "giảm tử vong");
        assert_eq!(evidence["data"]["locator"]["kind"], "pdf_text_range");
        assert_eq!(evidence["data"]["pdfExtractionId"], extraction_id);
        assert_eq!(evidence["data"]["sourceSnapshotId"], snapshot_id);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn pdf_page_api_paginates_one_extraction_without_gaps() {
        let root = std::env::temp_dir().join(format!(
            "9profs-core-pdf-pagination-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.data_dir = root.clone();
        config.database_path = root.join("core.db");
        config.session_secret = Some(Arc::from("api-test-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let case = runtime
            .research_service()
            .create_case(CreateResearchCase {
                title: "PDF pagination API".to_owned(),
            })
            .await
            .unwrap();
        let router = build_router(runtime);
        let upload_path = format!("/api/research/cases/{}/reference-pdfs", case.id.as_str());
        let response = router
            .clone()
            .oneshot(
                Request::post(&upload_path)
                    .header("content-type", "application/pdf")
                    .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                    .body(Body::from(b"%PDF-1.7\npagination fixture".to_vec()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let upload: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let snapshot_id = upload["data"]["snapshot"]["snapshotId"]
            .as_str()
            .unwrap()
            .to_owned();

        let pages: Vec<_> = (1..=120)
            .map(|page| serde_json::json!({ "page": page, "text": format!("page {page}") }))
            .collect();
        let response = router
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/research/snapshots/{snapshot_id}/pdf-extraction"
                ))
                .header("content-type", "application/json")
                .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                .body(Body::from(
                    serde_json::json!({
                        "extractor": "pdfjs",
                        "extractorVersion": "pagination-test",
                        "pageCount": 120,
                        "status": "ready",
                        "pages": pages
                    })
                    .to_string(),
                ))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let extraction: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let extraction_id = extraction["data"]["extractionId"]
            .as_str()
            .unwrap()
            .to_owned();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let response = router
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/research/snapshots/{snapshot_id}/pdf-extraction"
                ))
                .header("content-type", "application/json")
                .header(TRUSTED_DECISION_HEADER, "api-test-secret")
                .body(Body::from(
                    serde_json::json!({
                        "extractor": "pdfjs",
                        "extractorVersion": "pagination-test-newer",
                        "pageCount": 1,
                        "status": "ready",
                        "pages": [{ "page": 1, "text": "newer revision" }]
                    })
                    .to_string(),
                ))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let newer_extraction: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let newer_extraction_id = newer_extraction["data"]["extractionId"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_ne!(newer_extraction_id, extraction_id);

        let response = router
            .clone()
            .oneshot(
                Request::get(format!("/api/research/pdf-extractions/{extraction_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let exact: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(exact["data"]["extractionId"], extraction_id);

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/source-snapshots/{snapshot_id}/pdf-extractions"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let listed: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(listed["data"].as_array().unwrap().len(), 2);
        assert_eq!(listed["data"][0]["extractionId"], extraction_id);

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/snapshots/{snapshot_id}/pdf-extraction"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let latest: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(latest["data"]["extractionId"], newer_extraction_id);

        let mut all_pages = Vec::new();
        for (start_page, expected_end, has_more, next_start_page) in [
            (1, 50, true, Some(51)),
            (51, 100, true, Some(101)),
            (101, 120, false, None),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::get(format!(
                        "/api/research/pdf-extractions/{extraction_id}/pages?startPage={start_page}&limit=50"
                    ))
                    .body(Body::empty())
                    .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let payload: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            assert_eq!(payload["data"]["startPage"], start_page);
            assert_eq!(payload["data"]["limit"], 50);
            assert_eq!(payload["data"]["hasMore"], has_more);
            if let Some(next_start_page) = next_start_page {
                assert_eq!(payload["data"]["nextStartPage"], next_start_page);
            } else {
                assert!(payload["data"]["nextStartPage"].is_null());
            }
            let response_pages = payload["data"]["data"].as_array().unwrap();
            assert_eq!(response_pages.first().unwrap()["page"], start_page);
            assert_eq!(response_pages.last().unwrap()["page"], expected_end);
            all_pages.extend(
                response_pages
                    .iter()
                    .map(|page| page["page"].as_u64().unwrap() as u32),
            );
        }
        assert_eq!(all_pages, (1..=120).collect::<Vec<_>>());
        std::fs::remove_dir_all(root).unwrap();
    }
}

pub fn build_router(runtime: Arc<CoreRuntime>) -> Router {
    let state = AppState { runtime };
    Router::new()
        .route("/api/health", get(health))
        .route("/api/runtime", get(runtime_info))
        .route("/api/documents", get(list_documents))
        .route("/api/documents/{id}", get(get_document))
        .route("/api/document-proposals", get(list_document_proposals))
        .route("/api/document-proposals/{id}", get(get_document_proposal))
        .route(
            "/api/document-proposals/{id}/approve",
            post(approve_document_proposal),
        )
        .route(
            "/api/document-proposals/{id}/reject",
            post(reject_document_proposal),
        )
        .route(
            "/api/document-proposals/{id}/retry",
            post(retry_document_proposal),
        )
        .route("/api/officecli/status", get(officecli_status))
        .route("/api/agents", get(list_agents))
        .route("/api/agents/{id}", get(get_agent))
        .route("/api/document-agent-profile", get(document_agent_profile))
        .route("/api/agent-runs", post(create_agent_run))
        .route(
            "/api/document-agent-runs",
            post(create_active_docs_agent_run),
        )
        .route(
            "/api/document-agent-conversations",
            post(create_document_agent_conversation),
        )
        .route(
            "/api/document-agent-conversations/{id}",
            get(get_document_agent_conversation),
        )
        .route(
            "/api/document-agent-conversations/{id}/runs",
            post(create_document_agent_conversation_run),
        )
        .route("/api/agent-runs/{run_id}", get(get_agent_run))
        .route("/api/agent-runs/{run_id}/tasks", get(list_agent_run_tasks))
        .route("/api/agent-tasks/{task_id}/cancel", post(cancel_agent_task))
        .route(
            "/api/assistants",
            get(list_assistants).post(create_assistant),
        )
        .route(
            "/api/assistants/{id}",
            get(get_assistant)
                .put(update_assistant)
                .delete(delete_assistant),
        )
        .route("/api/skills", get(list_skills))
        .route("/api/skills/{id}", get(get_skill))
        .route("/api/skills/scan", post(scan_skills))
        .route(
            "/api/mcp/servers",
            get(list_mcp_servers).post(create_mcp_server),
        )
        .route(
            "/api/mcp/servers/{id}",
            get(get_mcp_server)
                .put(update_mcp_server)
                .delete(delete_mcp_server),
        )
        .route("/api/mcp/servers/{id}/connect", post(connect_mcp_server))
        .route(
            "/api/mcp/servers/{id}/disconnect",
            post(disconnect_mcp_server),
        )
        .route("/api/mcp/servers/{id}/test", post(test_mcp_server))
        .route("/api/mcp/servers/{id}/tools", get(list_mcp_tools))
        .route(
            "/api/research/cases",
            get(list_research_cases).post(create_research_case),
        )
        .route("/api/research/cases/{id}", get(get_research_case))
        .route(
            "/api/research/sources",
            get(list_research_sources).post(create_research_source),
        )
        .route("/api/research/sources/{id}", get(get_research_source))
        .route(
            "/api/research/snapshots",
            get(list_research_snapshots).post(capture_research_snapshot),
        )
        .route("/api/research/snapshots/{id}", get(get_research_snapshot))
        .route(
            "/api/research/cases/{id}/reference-pdfs",
            post(ingest_reference_pdf),
        )
        .route(
            "/api/research/snapshots/{id}/pdf-extraction",
            get(get_latest_research_pdf_extraction).post(capture_research_pdf_extraction),
        )
        .route(
            "/api/research/source-snapshots/{id}/pdf-extractions",
            get(list_research_pdf_extractions),
        )
        .route(
            "/api/research/pdf-extractions/{id}",
            get(get_research_pdf_extraction_by_id),
        )
        .route(
            "/api/research/pdf-extractions/{id}/pages",
            get(list_research_pdf_pages),
        )
        .route(
            "/api/research/pdf-extractions/{id}/pages/{page}",
            get(get_research_pdf_page),
        )
        .route(
            "/api/research/pdf-evidence",
            post(capture_research_pdf_evidence),
        )
        .route(
            "/api/research/evidence",
            get(list_research_evidence).post(create_research_evidence),
        )
        .route("/api/research/evidence/{id}", get(get_research_evidence))
        .route(
            "/api/research/claims",
            get(list_research_claims).post(create_research_claim),
        )
        .route("/api/research/claims/{id}", get(get_research_claim))
        .route(
            "/api/research/claim-evidence",
            get(list_claim_evidence_links).post(create_claim_evidence_link),
        )
        .route(
            "/api/research/claim-evidence/{id}",
            get(get_claim_evidence_link),
        )
        .route(
            "/api/research/citation-occurrences",
            get(list_citation_occurrences).post(create_citation_occurrence),
        )
        .route(
            "/api/research/citation-occurrences/{id}",
            get(get_citation_occurrence),
        )
        .route(
            "/api/research/citation-occurrences/{id}/targets",
            get(list_citation_targets).post(create_citation_target),
        )
        .route(
            "/api/research/cases/{case_id}/manuscripts/{manuscript_source_id}/citations/sync",
            post(sync_manuscript_citations),
        )
        .route(
            "/api/research/cases/{case_id}/manuscripts/{manuscript_source_id}/citations/sync/latest",
            get(latest_manuscript_citation_sync),
        )
        .route(
            "/api/research/manuscript-citation-sync-runs/{id}",
            get(get_manuscript_citation_sync),
        )
        .route(
            "/api/research/manuscript-citation-sync-runs/{id}/occurrences",
            get(list_manuscript_citation_sync_occurrences),
        )
        .route(
            "/api/research/manuscript-citation-sync-occurrences/{id}/targets",
            get(list_manuscript_citation_sync_targets),
        )
        .route(
            "/api/research/manuscript-citation-syncs/{sync_run_id}/reference-catalog",
            get(get_manuscript_reference_catalog_for_sync)
                .post(create_manuscript_reference_catalog),
        )
        .route(
            "/api/research/cases/{case_id}/manuscripts/{manuscript_source_id}/reference-catalog/latest",
            get(latest_manuscript_reference_catalog),
        )
        .route(
            "/api/research/manuscript-reference-catalog-runs/{id}",
            get(get_manuscript_reference_catalog),
        )
        .route(
            "/api/research/manuscript-reference-catalog-runs/{id}/entries",
            get(list_manuscript_reference_entries),
        )
        .route(
            "/api/research/manuscript-reference-entries/{id}/mappings",
            get(list_manuscript_reference_target_mappings),
        )
        .route(
            "/api/research/manuscript-citation-syncs/{sync_run_id}/claim-extractions",
            get(list_manuscript_claim_extractions).post(create_manuscript_claim_extraction),
        )
        .route(
            "/api/research/manuscript-claim-extractions/{id}",
            get(get_manuscript_claim_extraction),
        )
        .route(
            "/api/research/manuscript-claim-extractions/{id}/items",
            get(list_manuscript_claim_extraction_items),
        )
        .route(
            "/api/research/manuscript-claim-extractions/{id}/coverage",
            get(list_manuscript_claim_extraction_coverage),
        )
        .route(
            "/api/research/citation-targets/{id}",
            get(get_citation_target),
        )
        .route(
            "/api/research/citation-targets/{id}/bindings",
            get(list_citation_target_bindings).post(create_citation_target_binding),
        )
        .route(
            "/api/research/citation-target-bindings/{id}",
            get(get_citation_target_binding),
        )
        .route(
            "/api/research/citation-targets/{id}/latest-binding",
            get(get_latest_citation_target_binding),
        )
        .route(
            "/api/research/claim-citations",
            get(list_claim_citation_links).post(create_claim_citation_link),
        )
        .route(
            "/api/research/claim-citations/{id}",
            get(get_claim_citation_link),
        )
        .route(
            "/api/research/citation-verifications",
            post(create_citation_verification),
        )
        .route(
            "/api/research/citation-verifications/{id}",
            get(get_citation_verification),
        )
        .route(
            "/api/research/claims/{claim_id}/citation-verifications",
            get(list_claim_citation_verifications),
        )
        .route(
            "/api/research/cases/{id}/retrieval-index",
            get(get_research_retrieval_index),
        )
        .route(
            "/api/research/cases/{id}/retrieval-index/dify",
            post(ensure_research_retrieval_index),
        )
        .route(
            "/api/research/retrieval-indexes/{index_id}/extractions/{extraction_id}/sync",
            post(sync_research_retrieval_index),
        )
        .route(
            "/api/research/cases/{id}/retrieve",
            post(retrieve_research_case),
        )
        .route("/ws", get(websocket))
        .route("/ws/documents", get(document_websocket))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> axum::Json<ApiResponse<HealthResponse>> {
    axum::Json(ApiResponse::ok(state.runtime.health()))
}

async fn officecli_status(
    State(state): State<AppState>,
) -> axum::Json<ApiResponse<OfficeCliStatus>> {
    axum::Json(ApiResponse::ok(state.runtime.officecli_status()))
}

async fn runtime_info(State(state): State<AppState>) -> axum::Json<ApiResponse<RuntimeInfo>> {
    axum::Json(ApiResponse::ok(state.runtime.info()))
}

async fn list_agents(
    State(state): State<AppState>,
) -> axum::Json<ApiResponse<Vec<AgentBackendDescriptor>>> {
    axum::Json(ApiResponse::ok(state.runtime.agent_registry().list().await))
}

async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<AgentBackendDescriptor>>, ApiError> {
    let descriptor = state
        .runtime
        .agent_registry()
        .get(&id)
        .await
        .ok_or_else(|| ApiError::AgentNotFound(id.clone()))?;
    Ok(axum::Json(ApiResponse::ok(descriptor)))
}

async fn document_agent_profile(
    State(state): State<AppState>,
) -> axum::Json<ApiResponse<DocsAgentProfile>> {
    axum::Json(ApiResponse::ok(state.runtime.docs_agent_profile().await))
}

async fn websocket(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    nineprofs_realtime::websocket_upgrade(upgrade, state.runtime.event_bus())
}

async fn document_websocket(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    nineprofs_documents::websocket_upgrade(upgrade, state.runtime.document_bridge())
}

async fn list_documents(
    State(state): State<AppState>,
) -> axum::Json<ApiResponse<Vec<ActiveDocumentDto>>> {
    axum::Json(ApiResponse::ok(
        state
            .runtime
            .document_bridge()
            .list()
            .await
            .into_iter()
            .map(active_document_dto)
            .collect(),
    ))
}

async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ActiveDocumentDto>>, ApiError> {
    let document = state
        .runtime
        .document_bridge()
        .get(&id)
        .await
        .ok_or_else(|| ApiError::DocumentNotFound(id.clone()))?;
    Ok(axum::Json(ApiResponse::ok(active_document_dto(document))))
}

#[derive(Debug, Default, serde::Deserialize)]
struct DocumentProposalQuery {
    #[serde(rename = "documentId")]
    document_id: Option<String>,
}

async fn list_document_proposals(
    State(state): State<AppState>,
    Query(query): Query<DocumentProposalQuery>,
) -> axum::Json<ApiResponse<Vec<DocumentProposalDto>>> {
    axum::Json(ApiResponse::ok(
        state
            .runtime
            .document_tools()
            .list_proposals(query.document_id.as_deref())
            .await
            .into_iter()
            .map(document_proposal_dto)
            .collect(),
    ))
}

async fn get_document_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<DocumentProposalDto>>, ApiError> {
    let proposal = state
        .runtime
        .document_tools()
        .get_proposal(&id)
        .await
        .ok_or_else(|| ApiError::DocumentProposalNotFound(id.clone()))?;
    Ok(axum::Json(ApiResponse::ok(document_proposal_dto(proposal))))
}

fn active_document_dto(document: ActiveDocumentDescriptor) -> ActiveDocumentDto {
    ActiveDocumentDto {
        document_id: document.document_id,
        document_type: document.document_type,
        authority: document.authority,
        version: document.version,
        capabilities: document.capabilities,
        availability: "available".to_owned(),
    }
}

fn document_proposal_dto(proposal: DocumentProposalView) -> DocumentProposalDto {
    DocumentProposalDto {
        proposal_id: proposal.proposal_id,
        change_set_id: proposal.change_set.id,
        document_id: proposal.document_id,
        authority: proposal.change_set.target.kind,
        base_version: proposal.base_version,
        status: proposal.status,
        freshness: match proposal.freshness {
            ProposalFreshness::Fresh => "fresh",
            ProposalFreshness::Stale => "stale",
            ProposalFreshness::Unavailable => "unavailable",
        }
        .to_owned(),
        availability: match proposal.availability {
            ProposalAvailability::Available => "available",
            ProposalAvailability::Unavailable => "unavailable",
        }
        .to_owned(),
        current_version: proposal.current_version,
        created_at_ms: proposal.created_at_ms,
        summary: proposal.summary,
        changes: proposal
            .change_set
            .changes
            .into_iter()
            .map(|change| DocumentProposalChangeDto {
                change_type: change.change_type,
                payload: change.payload,
            })
            .collect(),
        decision: proposal.decision,
        outcome: proposal
            .outcome
            .and_then(|outcome| serde_json::to_value(outcome).ok()),
        failure: proposal.failure,
        retryable: proposal.retryable,
    }
}

const TRUSTED_DECISION_HEADER: &str = "x-nineprofs-session-secret";

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedDecisionRequest {
    #[serde(default)]
    note: Option<String>,
}

fn authorize_trusted_decision(
    headers: &HeaderMap,
    config: &nineprofs_runtime::RuntimeConfig,
) -> Result<(), ApiError> {
    match config.session_secret.as_deref() {
        Some(expected) => {
            let provided = headers
                .get(TRUSTED_DECISION_HEADER)
                .and_then(|value| value.to_str().ok());
            if !constant_time_secret_eq(expected, provided) {
                return Err(ApiError::Unauthorized);
            }
        }
        None if !config.bind_addr.ip().is_loopback() => return Err(ApiError::Unauthorized),
        None => {}
    }
    Ok(())
}

fn constant_time_secret_eq(expected: &str, provided: Option<&str>) -> bool {
    let Some(provided) = provided else {
        return false;
    };
    let left = expected.as_bytes();
    let right = provided.as_bytes();
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(left.get(index).copied().unwrap_or_default())
            ^ usize::from(right.get(index).copied().unwrap_or_default());
    }
    difference == 0
}

fn decision_note(
    body: Option<axum::Json<TrustedDecisionRequest>>,
) -> Result<Option<String>, ApiError> {
    let note = body.map(|payload| payload.0.note).flatten();
    if note.as_ref().is_some_and(|value| value.len() > 4096) {
        return Err(ApiError::InvalidRequest(
            "decision note exceeds 4096 bytes".to_owned(),
        ));
    }
    Ok(note)
}

async fn approve_document_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Option<axum::Json<TrustedDecisionRequest>>,
) -> Result<axum::Json<ApiResponse<DocumentProposalDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let proposal = state
        .runtime
        .document_workflow()
        .approve(&id, decision_note(body)?)
        .await?;
    Ok(axum::Json(ApiResponse::ok(document_proposal_dto(proposal))))
}

async fn reject_document_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Option<axum::Json<TrustedDecisionRequest>>,
) -> Result<axum::Json<ApiResponse<DocumentProposalDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let proposal = state
        .runtime
        .document_workflow()
        .reject(&id, decision_note(body)?)
        .await?;
    Ok(axum::Json(ApiResponse::ok(document_proposal_dto(proposal))))
}

async fn retry_document_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<axum::Json<ApiResponse<DocumentProposalDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let proposal = state.runtime.document_workflow().retry(&id).await?;
    Ok(axum::Json(ApiResponse::ok(document_proposal_dto(proposal))))
}

#[derive(Debug)]
enum ApiError {
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

async fn create_agent_run(
    State(state): State<AppState>,
    axum::Json(request): axum::Json<AgentRunRequest>,
) -> Result<axum::Json<ApiResponse<AgentRunStartedDto>>, ApiError> {
    let started = state
        .runtime
        .execution_service()
        .start_run(&request.assistant_id, &request.input)
        .await?;
    Ok(axum::Json(ApiResponse::ok(AgentRunStartedDto {
        run_id: started.run_id.to_string(),
        task: task_dto(&started.task),
        context: None,
    })))
}

async fn create_active_docs_agent_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<ActiveDocsAgentRunRequest>,
) -> Result<axum::Json<ApiResponse<AgentRunStartedDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let started = state
        .runtime
        .execution_service()
        .start_active_docs_run(&request.assistant_id, &request.document_id, &request.input)
        .await?;
    Ok(axum::Json(ApiResponse::ok(AgentRunStartedDto {
        run_id: started.run_id.to_string(),
        task: task_dto(&started.task),
        context: agent_run_context_dto(started.context.as_ref()),
    })))
}

async fn create_document_agent_conversation(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateDocumentAgentConversationRequest>,
) -> Result<axum::Json<ApiResponse<DocumentAgentConversationDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let conversation = state
        .runtime
        .execution_service()
        .create_docs_agent_conversation(&request.assistant_id, &request.document_id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        document_agent_conversation_dto(conversation),
    )))
}

async fn get_document_agent_conversation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<DocumentAgentConversationDto>>, ApiError> {
    let conversation = state
        .runtime
        .execution_service()
        .docs_agent_conversation(&id)
        .ok_or_else(|| ApiError::ConversationNotFound(id))?;
    Ok(axum::Json(ApiResponse::ok(
        document_agent_conversation_dto(conversation),
    )))
}

async fn create_document_agent_conversation_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateDocumentAgentConversationRunRequest>,
) -> Result<axum::Json<ApiResponse<AgentRunStartedDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let started = state
        .runtime
        .execution_service()
        .start_docs_agent_conversation_run(&id, &request.input)
        .await?;
    Ok(axum::Json(ApiResponse::ok(AgentRunStartedDto {
        run_id: started.run_id.to_string(),
        task: task_dto(&started.task),
        context: agent_run_context_dto(started.context.as_ref()),
    })))
}

async fn get_agent_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<AgentRunDto>>, ApiError> {
    let run = RunId::from_string(run_id.clone());
    let execution = state.runtime.execution_service();
    let tasks = execution.tasks_for_run(&run).await;
    if tasks.is_empty() {
        return Err(ApiError::RunNotFound(run_id));
    }
    let context = execution.context_for_run(&run).await;
    Ok(axum::Json(ApiResponse::ok(AgentRunDto {
        run_id: run.to_string(),
        tasks: tasks.iter().map(task_dto).collect(),
        context: agent_run_context_dto(context.as_ref()),
    })))
}

async fn list_agent_run_tasks(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<AgentTaskDto>>>, ApiError> {
    let run = RunId::from_string(run_id.clone());
    let tasks = state.runtime.execution_service().tasks_for_run(&run).await;
    if tasks.is_empty() {
        return Err(ApiError::RunNotFound(run_id));
    }
    Ok(axum::Json(ApiResponse::ok(
        tasks.iter().map(task_dto).collect(),
    )))
}

async fn cancel_agent_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<axum::Json<ApiResponse<AgentTaskDto>>, ApiError> {
    let task = state
        .runtime
        .execution_service()
        .cancel(&AgentTaskId::from_string(task_id))
        .await?;
    Ok(axum::Json(ApiResponse::ok(task_dto(&task))))
}

fn document_agent_conversation_dto(
    conversation: nineprofs_runtime::DocsAgentConversationMetadata,
) -> DocumentAgentConversationDto {
    DocumentAgentConversationDto {
        conversation_id: conversation.conversation_id,
        assistant_id: conversation.assistant_id,
        document_id: conversation.document_id,
        state: conversation.state.as_str().to_owned(),
        turn_count: conversation.turn_count,
        created_at_ms: conversation.created_at_ms,
        updated_at_ms: conversation.updated_at_ms,
    }
}

fn task_dto(task: &nineprofs_agent::AgentTask) -> AgentTaskDto {
    AgentTaskDto {
        task_id: task.task_id.to_string(),
        run_id: task.run_id.to_string(),
        backend_id: task.backend_id.clone(),
        state: match task.state {
            TaskState::Queued => "queued",
            TaskState::Starting => "starting",
            TaskState::Running => "running",
            TaskState::Succeeded => "succeeded",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
        }
        .to_owned(),
        created_at_ms: task.created_at_ms,
        updated_at_ms: task.updated_at_ms,
        started_at_ms: task.started_at_ms,
        completed_at_ms: task.completed_at_ms,
        failure: task.failure.as_ref().map(|failure| AgentTaskFailureDto {
            code: failure.code.clone(),
            message: failure.message.clone(),
        }),
        cancellation_requested: task.cancellation_requested,
    }
}

fn agent_run_context_dto(context: Option<&AgentRunContext>) -> Option<AgentRunContextDto> {
    context.map(|context| match context {
        AgentRunContext::ActiveDocs { document_id } => AgentRunContextDto::ActiveDocs {
            document_id: document_id.clone(),
        },
    })
}

async fn list_assistants(
    State(state): State<AppState>,
) -> Result<axum::Json<ApiResponse<Vec<AssistantDto>>>, ApiError> {
    let assistants = state
        .runtime
        .assistant_service()
        .list()
        .await?
        .iter()
        .map(assistant_dto)
        .collect();
    Ok(axum::Json(ApiResponse::ok(assistants)))
}

async fn get_assistant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<AssistantDto>>, ApiError> {
    let assistant = state.runtime.assistant_service().get(&id).await?;
    Ok(axum::Json(ApiResponse::ok(assistant_dto(&assistant))))
}

async fn create_assistant(
    State(state): State<AppState>,
    axum::Json(request): axum::Json<CreateAssistantRequest>,
) -> Result<axum::Json<ApiResponse<AssistantDto>>, ApiError> {
    let assistant = state
        .runtime
        .assistant_service()
        .create(CreateAssistant {
            id: request.id,
            name: request.name,
            description: request.description,
            avatar: request.avatar,
            rules: request.rules,
            enabled: request.enabled,
            skill_ids: request.skill_ids,
            backend_agent_id: request.backend_agent_id,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(assistant_dto(&assistant))))
}

async fn update_assistant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Json(request): axum::Json<UpdateAssistantRequest>,
) -> Result<axum::Json<ApiResponse<AssistantDto>>, ApiError> {
    let assistant = state
        .runtime
        .assistant_service()
        .update(
            &id,
            UpdateAssistant {
                name: request.name,
                description: request.description,
                avatar: request.avatar,
                rules: request.rules,
                enabled: request.enabled,
                skill_ids: request.skill_ids,
                backend_agent_id: request.backend_agent_id,
            },
        )
        .await?;
    Ok(axum::Json(ApiResponse::ok(assistant_dto(&assistant))))
}

async fn delete_assistant(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<()>>, ApiError> {
    state.runtime.assistant_service().delete(&id).await?;
    Ok(axum::Json(ApiResponse::ok(())))
}

async fn list_skills(State(state): State<AppState>) -> axum::Json<ApiResponse<SkillCatalogDto>> {
    axum::Json(ApiResponse::ok(skill_catalog_dto(
        state.runtime.skill_catalog().scan(),
        false,
    )))
}

async fn get_skill(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<SkillDto>>, ApiError> {
    let skill = state
        .runtime
        .skill_catalog()
        .resolve(&id)
        .ok_or_else(|| ApiError::NotFound(id.clone()))?;
    Ok(axum::Json(ApiResponse::ok(skill_dto(&skill, true))))
}

async fn scan_skills(State(state): State<AppState>) -> axum::Json<ApiResponse<SkillCatalogDto>> {
    let catalog = skill_catalog_dto(state.runtime.skill_catalog().scan(), false);
    let _ = state.runtime.event_bus().publish(EventEnvelope::new(
        "skill.catalogChanged",
        serde_json::json!({ "skill_count": catalog.skills.len(), "issue_count": catalog.issues.len() }),
    ));
    axum::Json(ApiResponse::ok(catalog))
}

async fn list_mcp_servers(
    State(state): State<AppState>,
) -> Result<axum::Json<ApiResponse<Vec<McpServerDto>>>, ApiError> {
    let servers = state
        .runtime
        .mcp_service()
        .list()
        .await?
        .iter()
        .map(mcp_server_dto)
        .collect();
    Ok(axum::Json(ApiResponse::ok(servers)))
}

async fn get_mcp_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<McpServerDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(mcp_server_dto(
        &state.runtime.mcp_service().get(&id).await?,
    ))))
}

async fn create_mcp_server(
    State(state): State<AppState>,
    axum::Json(request): axum::Json<CreateMcpServerRequest>,
) -> Result<axum::Json<ApiResponse<McpServerDto>>, ApiError> {
    let server = state
        .runtime
        .mcp_service()
        .create(CreateMcpServer {
            id: request.id,
            name: request.name,
            description: request.description,
            enabled: request.enabled,
            startup_timeout_ms: request.startup_timeout_ms,
            transport: mcp_transport_config(request.transport),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(mcp_server_dto(&server))))
}

async fn update_mcp_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::Json(request): axum::Json<UpdateMcpServerRequest>,
) -> Result<axum::Json<ApiResponse<McpServerDto>>, ApiError> {
    let server = state
        .runtime
        .mcp_service()
        .update(
            &id,
            UpdateMcpServer {
                name: request.name,
                description: request.description,
                enabled: request.enabled,
                startup_timeout_ms: request.startup_timeout_ms,
                transport: request.transport.map(mcp_transport_config),
            },
        )
        .await?;
    Ok(axum::Json(ApiResponse::ok(mcp_server_dto(&server))))
}

async fn delete_mcp_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<()>>, ApiError> {
    state.runtime.mcp_service().delete(&id).await?;
    Ok(axum::Json(ApiResponse::ok(())))
}

async fn connect_mcp_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<McpServerDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(mcp_server_dto(
        &state.runtime.mcp_service().connect(&id).await?,
    ))))
}

async fn disconnect_mcp_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<McpServerDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(mcp_server_dto(
        &state.runtime.mcp_service().disconnect(&id).await?,
    ))))
}

async fn test_mcp_server(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<McpConnectionTestDto>>, ApiError> {
    let result = state.runtime.mcp_service().test(&id).await?;
    Ok(axum::Json(ApiResponse::ok(McpConnectionTestDto {
        success: result.success,
        tool_count: result.tool_count,
        supports_resources: result.supports_resources,
        error: result.error,
    })))
}

async fn list_mcp_tools(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<McpToolDto>>>, ApiError> {
    let tools = state
        .runtime
        .mcp_service()
        .tools(&id)
        .await?
        .into_iter()
        .map(|tool| McpToolDto {
            id: tool.id,
            name: tool.name,
            display_name: tool.display_name,
            description: tool.description,
            input_schema: tool.input_schema,
        })
        .collect();
    Ok(axum::Json(ApiResponse::ok(tools)))
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchSourcesQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchSnapshotsQuery {
    #[serde(rename = "sourceId")]
    source_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchEvidenceQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
    #[serde(rename = "sourceSnapshotId")]
    source_snapshot_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchClaimsQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchLinksQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
    #[serde(rename = "claimId")]
    claim_id: Option<String>,
    #[serde(rename = "evidenceId")]
    evidence_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct CitationOccurrencesQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ClaimCitationsQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
    #[serde(rename = "claimId")]
    claim_id: Option<String>,
    #[serde(rename = "citationOccurrenceId")]
    citation_occurrence_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchPdfPagesQuery {
    #[serde(rename = "startPage")]
    start_page: Option<u32>,
    limit: Option<u32>,
}

async fn list_research_cases(
    State(state): State<AppState>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchCaseDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_cases()
            .await?
            .into_iter()
            .map(research_case_dto)
            .collect(),
    )))
}

async fn get_research_case(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchCaseDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_case_dto(
        state.runtime.research_service().get_case(&id).await?,
    ))))
}

async fn create_research_case(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateResearchCaseRequest>,
) -> Result<axum::Json<ApiResponse<ResearchCaseDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    Ok(axum::Json(ApiResponse::ok(research_case_dto(
        state
            .runtime
            .research_service()
            .create_case(CreateResearchCase {
                title: request.title,
            })
            .await?,
    ))))
}

async fn list_research_sources(
    State(state): State<AppState>,
    Query(query): Query<ResearchSourcesQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchSourceDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_sources(query.research_case_id.as_deref())
            .await?
            .into_iter()
            .map(research_source_dto)
            .collect(),
    )))
}

async fn get_research_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchSourceDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_source_dto(
        state.runtime.research_service().get_source(&id).await?,
    ))))
}

async fn create_research_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateResearchSourceRequest>,
) -> Result<axum::Json<ApiResponse<ResearchSourceDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let source = state
        .runtime
        .research_service()
        .create_source(CreateResearchSource {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            kind: source_kind(request.kind),
            label: request.label,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_source_dto(source))))
}

async fn list_research_snapshots(
    State(state): State<AppState>,
    Query(query): Query<ResearchSnapshotsQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchSourceSnapshotDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_snapshots(query.source_id.as_deref())
            .await?
            .into_iter()
            .map(research_snapshot_dto)
            .collect(),
    )))
}

async fn get_research_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchSourceSnapshotDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_snapshot_dto(
        state.runtime.research_service().get_snapshot(&id).await?,
    ))))
}

async fn ingest_reference_pdf(
    State(state): State<AppState>,
    Path(case_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<axum::Json<ApiResponse<ReferencePdfIngestionDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    if let Some(content_type) = header_text(&headers, "content-type")? {
        let media_type = content_type.split(';').next().unwrap_or("").trim();
        if !media_type.eq_ignore_ascii_case("application/pdf") {
            return Err(ApiError::InvalidRequest(
                "reference PDF upload must use application/pdf".to_owned(),
            ));
        }
    }
    let original_filename = safe_upload_label(
        header_text(&headers, "x-nineprofs-original-filename")?.as_deref(),
        "reference.pdf",
    )?;
    let source_label = safe_upload_label(
        header_text(&headers, "x-nineprofs-source-label")?.as_deref(),
        &original_filename,
    )?;
    let service = state.runtime.research_service();
    let research_case_id = nineprofs_research::ResearchCaseId::parse(case_id)?;
    service.get_case(research_case_id.as_str()).await?;
    let store = service
        .artifact_store()
        .ok_or_else(|| ApiError::InvalidRequest("PDF artifact store is unavailable".to_owned()))?;
    let mut upload = store.begin_upload(&original_filename)?;
    let mut chunks = body.into_data_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
        upload.append(&chunk)?;
    }
    let artifact = upload.finish().await?;
    let source = service
        .create_source(CreateResearchSource {
            research_case_id,
            kind: SourceKind::ReferencePdf,
            label: source_label,
        })
        .await?;
    let snapshot = service
        .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
        .await?;
    let _ = state.runtime.event_bus().publish(EventEnvelope::new(
        "research.pdfIngested",
        serde_json::json!({
            "artifact_id": artifact.artifact_id(),
            "source_id": source.id,
            "snapshot_id": snapshot.id,
            "size_bytes": artifact.artifact().size_bytes,
            "content_hash": artifact.content_hash().value,
        }),
    ));
    Ok(axum::Json(ApiResponse::ok(ReferencePdfIngestionDto {
        artifact: research_artifact_dto(artifact.artifact().clone()),
        source: research_source_dto(source),
        snapshot: research_snapshot_dto(snapshot),
    })))
}

async fn capture_research_pdf_extraction(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CaptureResearchPdfExtractionRequest>,
) -> Result<axum::Json<ApiResponse<ResearchPdfExtractionDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let extraction = state
        .runtime
        .research_service()
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(snapshot_id)?,
            extractor: request.extractor,
            extractor_version: request.extractor_version,
            page_count: request.page_count,
            status: pdf_extraction_status(request.status),
            pages: request
                .pages
                .into_iter()
                .map(|page| CapturePdfPage {
                    page: page.page,
                    text: page.text,
                })
                .collect(),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_pdf_extraction_dto(
        extraction,
    ))))
}

async fn get_latest_research_pdf_extraction(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchPdfExtractionDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_pdf_extraction_dto(
        state
            .runtime
            .research_service()
            .latest_pdf_extraction(&snapshot_id)
            .await?,
    ))))
}

async fn get_research_pdf_extraction_by_id(
    State(state): State<AppState>,
    Path(extraction_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchPdfExtractionDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_pdf_extraction_dto(
        state
            .runtime
            .research_service()
            .get_pdf_extraction_by_id(&extraction_id)
            .await?,
    ))))
}

async fn list_research_pdf_extractions(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchPdfExtractionDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_pdf_extractions(&snapshot_id)
            .await?
            .into_iter()
            .map(research_pdf_extraction_dto)
            .collect(),
    )))
}

async fn list_research_pdf_pages(
    State(state): State<AppState>,
    Path(extraction_id): Path<String>,
    Query(query): Query<ResearchPdfPagesQuery>,
) -> Result<axum::Json<ApiResponse<ResearchPdfPageListDto>>, ApiError> {
    let batch = state
        .runtime
        .research_service()
        .list_pdf_pages(
            &extraction_id,
            query.start_page.unwrap_or(1),
            query.limit.unwrap_or(50),
        )
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_pdf_page_list_dto(
        batch,
    ))))
}

async fn get_research_pdf_page(
    State(state): State<AppState>,
    Path((extraction_id, page)): Path<(String, u32)>,
) -> Result<axum::Json<ApiResponse<ResearchPdfPageDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_pdf_page_dto(
        state
            .runtime
            .research_service()
            .get_pdf_page(&extraction_id, page)
            .await?,
    ))))
}

async fn capture_research_pdf_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CaptureResearchPdfEvidenceRequest>,
) -> Result<axum::Json<ApiResponse<ResearchEvidenceDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let evidence = state
        .runtime
        .research_service()
        .capture_pdf_evidence(CapturePdfEvidence {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(
                request.source_snapshot_id,
            )?,
            extraction_id: nineprofs_research::ResearchPdfExtractionId::parse(
                request.extraction_id,
            )?,
            page: request.page,
            start: request.start,
            end: request.end,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_evidence_dto(evidence))))
}

async fn capture_research_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CaptureResearchSourceSnapshotRequest>,
) -> Result<axum::Json<ApiResponse<ResearchSourceSnapshotDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let snapshot = state
        .runtime
        .research_service()
        .capture_snapshot(CaptureSourceSnapshot {
            source_id: nineprofs_research::ResearchSourceId::parse(request.source_id)?,
            content: request.content.into_bytes(),
            capture_method: capture_method(request.capture_method),
            origin: source_origin(request.origin),
            metadata: request.metadata,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_snapshot_dto(snapshot))))
}

async fn list_research_evidence(
    State(state): State<AppState>,
    Query(query): Query<ResearchEvidenceQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchEvidenceDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_evidence(
                query.research_case_id.as_deref(),
                query.source_snapshot_id.as_deref(),
            )
            .await?
            .into_iter()
            .map(research_evidence_dto)
            .collect(),
    )))
}

async fn get_research_evidence(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchEvidenceDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_evidence_dto(
        state.runtime.research_service().get_evidence(&id).await?,
    ))))
}

async fn create_research_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateResearchEvidenceRequest>,
) -> Result<axum::Json<ApiResponse<ResearchEvidenceDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let evidence = state
        .runtime
        .research_service()
        .create_evidence(CreateResearchEvidence {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(
                request.source_snapshot_id,
            )?,
            verbatim_excerpt: request.verbatim_excerpt,
            normalized_text: request.normalized_text,
            locator: evidence_locator(request.locator),
            capture_method: capture_method(request.capture_method),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_evidence_dto(evidence))))
}

async fn list_research_claims(
    State(state): State<AppState>,
    Query(query): Query<ResearchClaimsQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchClaimDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_claims(query.research_case_id.as_deref())
            .await?
            .into_iter()
            .map(research_claim_dto)
            .collect(),
    )))
}

async fn get_research_claim(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchClaimDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_claim_dto(
        state.runtime.research_service().get_claim(&id).await?,
    ))))
}

async fn create_research_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateResearchClaimRequest>,
) -> Result<axum::Json<ApiResponse<ResearchClaimDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let claim = state
        .runtime
        .research_service()
        .create_claim(CreateResearchClaim {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            text: request.text,
            origin: claim_origin(request.origin),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_claim_dto(claim))))
}

async fn list_claim_evidence_links(
    State(state): State<AppState>,
    Query(query): Query<ResearchLinksQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ClaimEvidenceLinkDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_links(
                query.research_case_id.as_deref(),
                query.claim_id.as_deref(),
                query.evidence_id.as_deref(),
            )
            .await?
            .into_iter()
            .map(claim_evidence_link_dto)
            .collect(),
    )))
}

async fn get_claim_evidence_link(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ClaimEvidenceLinkDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(claim_evidence_link_dto(
        state.runtime.research_service().get_link(&id).await?,
    ))))
}

async fn create_claim_evidence_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateClaimEvidenceLinkRequest>,
) -> Result<axum::Json<ApiResponse<ClaimEvidenceLinkDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let link = state
        .runtime
        .research_service()
        .create_link(CreateClaimEvidenceLink {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            claim_id: nineprofs_research::ResearchClaimId::parse(request.claim_id)?,
            evidence_id: nineprofs_research::ResearchEvidenceId::parse(request.evidence_id)?,
            relation: claim_evidence_relation(request.relation),
            rationale: request.rationale,
            assessment_method: assessment_method(request.assessment_method),
            assessment_metadata: request.assessment_metadata,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(claim_evidence_link_dto(link))))
}

async fn list_citation_occurrences(
    State(state): State<AppState>,
    Query(query): Query<CitationOccurrencesQuery>,
) -> Result<axum::Json<ApiResponse<Vec<CitationOccurrenceDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_citation_occurrences(query.research_case_id.as_deref())
            .await?
            .into_iter()
            .map(citation_occurrence_dto)
            .collect(),
    )))
}

async fn get_citation_occurrence(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationOccurrenceDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(citation_occurrence_dto(
        state
            .runtime
            .research_service()
            .get_citation_occurrence(&id)
            .await?,
    ))))
}

async fn create_citation_occurrence(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateCitationOccurrenceRequest>,
) -> Result<axum::Json<ApiResponse<CitationOccurrenceDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let occurrence = state
        .runtime
        .research_service()
        .create_citation_occurrence(CreateCitationOccurrence {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            origin: citation_occurrence_origin(request.origin)?,
            rendered_text: request.rendered_text,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_occurrence_dto(
        occurrence,
    ))))
}

async fn sync_manuscript_citations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((case_id, manuscript_source_id)): Path<(String, String)>,
    axum::Json(request): axum::Json<SyncManuscriptCitationsRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptCitationSyncRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .research_service()
        .sync_manuscript_citations(nineprofs_research::SyncManuscriptCitations {
            research_case_id: nineprofs_research::ResearchCaseId::parse(case_id)?,
            manuscript_source_id: nineprofs_research::ResearchSourceId::parse(
                manuscript_source_id,
            )?,
            document_id: request.document_id,
            document_version: request.document_version,
            citations: request
                .citations
                .into_iter()
                .map(manuscript_citation_sync_citation)
                .collect::<Result<Vec<_>, _>>()?,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_citation_sync_run_dto(run),
    )))
}

async fn latest_manuscript_citation_sync(
    State(state): State<AppState>,
    Path((case_id, manuscript_source_id)): Path<(String, String)>,
) -> Result<axum::Json<ApiResponse<ManuscriptCitationSyncRunDto>>, ApiError> {
    let run = state
        .runtime
        .research_service()
        .latest_manuscript_citation_sync(&case_id, &manuscript_source_id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_citation_sync_run_dto(run),
    )))
}

async fn get_manuscript_citation_sync(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptCitationSyncRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_citation_sync_run_dto(
            state
                .runtime
                .research_service()
                .get_manuscript_citation_sync(&id)
                .await?,
        ),
    )))
}

async fn list_manuscript_citation_sync_occurrences(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptCitationSyncOccurrenceDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_citation_sync_occurrences(&id)
            .await?
            .into_iter()
            .map(manuscript_citation_sync_occurrence_dto)
            .collect(),
    )))
}

async fn list_manuscript_citation_sync_targets(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptCitationSyncTargetDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_citation_sync_targets(&id)
            .await?
            .into_iter()
            .map(manuscript_citation_sync_target_dto)
            .collect(),
    )))
}

async fn create_manuscript_reference_catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sync_run_id): Path<String>,
    axum::Json(request): axum::Json<CreateManuscriptReferenceCatalogRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceCatalogRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .research_service()
        .sync_manuscript_reference_catalog(nineprofs_research::SyncManuscriptReferenceCatalog {
            citation_sync_run_id: nineprofs_research::ManuscriptCitationSyncRunId::parse(
                sync_run_id,
            )?,
            document_id: request.document_id,
            document_version: request.document_version,
            citations: request
                .citations
                .into_iter()
                .map(manuscript_reference_catalog_citation)
                .collect::<Result<Vec<_>, _>>()?,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_reference_catalog_run_dto(run),
    )))
}

async fn get_manuscript_reference_catalog_for_sync(
    State(state): State<AppState>,
    Path(sync_run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceCatalogRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_reference_catalog_run_dto(
            state
                .runtime
                .research_service()
                .manuscript_reference_catalog_for_sync(&sync_run_id)
                .await?,
        ),
    )))
}

async fn latest_manuscript_reference_catalog(
    State(state): State<AppState>,
    Path((case_id, manuscript_source_id)): Path<(String, String)>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceCatalogRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_reference_catalog_run_dto(
            state
                .runtime
                .research_service()
                .latest_manuscript_reference_catalog(&case_id, &manuscript_source_id)
                .await?,
        ),
    )))
}

async fn get_manuscript_reference_catalog(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceCatalogRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_reference_catalog_run_dto(
            state
                .runtime
                .research_service()
                .get_manuscript_reference_catalog(&id)
                .await?,
        ),
    )))
}

async fn list_manuscript_reference_entries(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptReferenceEntryDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_reference_entries(&id)
            .await?
            .into_iter()
            .map(manuscript_reference_entry_dto)
            .collect(),
    )))
}

async fn list_manuscript_reference_target_mappings(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptReferenceTargetMappingDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_reference_target_mappings(&id)
            .await?
            .into_iter()
            .map(manuscript_reference_target_mapping_dto)
            .collect(),
    )))
}

async fn create_manuscript_claim_extraction(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sync_run_id): Path<String>,
    axum::Json(request): axum::Json<CreateManuscriptClaimExtractionRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptClaimExtractionRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .research_service()
        .extract_manuscript_claims(ExtractManuscriptClaims {
            citation_sync_run_id: nineprofs_research::ManuscriptCitationSyncRunId::parse(
                sync_run_id,
            )?,
            document_id: request.document_id,
            document_version: request.document_version,
            blocks: request
                .blocks
                .into_iter()
                .map(|block| ManuscriptClaimExtractionBlockInput {
                    block_id: block.block_id,
                    text: block.text,
                    citations: block
                        .citations
                        .into_iter()
                        .map(|citation| ManuscriptClaimExtractionCitationInput {
                            citation_occurrence_id: citation.citation_occurrence_id,
                            start: citation.start,
                            end: citation.end,
                            rendered_text: citation.rendered_text,
                        })
                        .collect(),
                })
                .collect(),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_claim_extraction_run_dto(run),
    )))
}

async fn list_manuscript_claim_extractions(
    State(state): State<AppState>,
    Path(sync_run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptClaimExtractionRunDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_claim_extractions(Some(&sync_run_id))
            .await?
            .into_iter()
            .map(manuscript_claim_extraction_run_dto)
            .collect(),
    )))
}

async fn get_manuscript_claim_extraction(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptClaimExtractionRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_claim_extraction_run_dto(
            state
                .runtime
                .research_service()
                .get_manuscript_claim_extraction(&id)
                .await?,
        ),
    )))
}

async fn list_manuscript_claim_extraction_items(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptClaimExtractionItemDto>>>, ApiError> {
    let service = state.runtime.research_service();
    let items = service.list_manuscript_claim_extraction_items(&id).await?;
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let claim = service.get_claim(item.research_claim_id.as_str()).await?;
        let links = service
            .list_claim_citation_links(None, Some(item.research_claim_id.as_str()), None)
            .await?;
        result.push(manuscript_claim_extraction_item_dto(
            item,
            claim.text,
            links
                .iter()
                .map(|link| link.citation_occurrence_id.to_string())
                .collect(),
            links.iter().map(|link| link.id.to_string()).collect(),
        ));
    }
    Ok(axum::Json(ApiResponse::ok(result)))
}

async fn list_manuscript_claim_extraction_coverage(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptClaimExtractionCoverageDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_manuscript_claim_extraction_coverage(&id)
            .await?
            .into_iter()
            .map(manuscript_claim_extraction_coverage_dto)
            .collect(),
    )))
}

async fn list_citation_targets(
    State(state): State<AppState>,
    Path(occurrence_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<CitationTargetDto>>>, ApiError> {
    let service = state.runtime.research_service();
    let mut targets = Vec::new();
    for target in service.list_citation_targets(&occurrence_id).await? {
        let resolution = service
            .citation_target_resolution(target.id.as_str())
            .await?;
        targets.push(citation_target_dto(target, resolution));
    }
    Ok(axum::Json(ApiResponse::ok(targets)))
}

async fn create_citation_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(occurrence_id): Path<String>,
    axum::Json(request): axum::Json<CreateCitationTargetRequest>,
) -> Result<axum::Json<ApiResponse<CitationTargetDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let route_occurrence_id = nineprofs_research::CitationOccurrenceId::parse(occurrence_id)?;
    let request_occurrence_id =
        nineprofs_research::CitationOccurrenceId::parse(request.citation_occurrence_id)?;
    if route_occurrence_id != request_occurrence_id {
        return Err(ResearchError::Invalid(
            "citation target occurrence does not match route".to_owned(),
        )
        .into());
    }
    let target = state
        .runtime
        .research_service()
        .create_citation_target(CreateCitationTarget {
            citation_occurrence_id: route_occurrence_id,
            ordinal: request.ordinal,
            reference_key: request.reference_key,
            cited_locator: request.cited_locator,
        })
        .await?;
    let resolution = state
        .runtime
        .research_service()
        .citation_target_resolution(target.id.as_str())
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_target_dto(
        target, resolution,
    ))))
}

async fn get_citation_target(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationTargetDto>>, ApiError> {
    let service = state.runtime.research_service();
    let target = service.get_citation_target(&id).await?;
    let resolution = service
        .citation_target_resolution(target.id.as_str())
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_target_dto(
        target, resolution,
    ))))
}

async fn list_citation_target_bindings(
    State(state): State<AppState>,
    Path(target_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<CitationTargetBindingDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_citation_target_bindings(&target_id)
            .await?
            .into_iter()
            .map(citation_target_binding_dto)
            .collect(),
    )))
}

async fn create_citation_target_binding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_id): Path<String>,
    axum::Json(request): axum::Json<CreateCitationTargetBindingRequest>,
) -> Result<axum::Json<ApiResponse<CitationTargetBindingDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let route_target_id = nineprofs_research::CitationTargetId::parse(target_id)?;
    let request_target_id =
        nineprofs_research::CitationTargetId::parse(request.citation_target_id)?;
    if route_target_id != request_target_id {
        return Err(ResearchError::Invalid(
            "citation binding target does not match route".to_owned(),
        )
        .into());
    }
    let binding = state
        .runtime
        .research_service()
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            citation_target_id: route_target_id,
            source_id: nineprofs_research::ResearchSourceId::parse(request.source_id)?,
            source_snapshot_id: request
                .source_snapshot_id
                .map(nineprofs_research::ResearchSourceSnapshotId::parse)
                .transpose()?,
            extraction_id: request
                .extraction_id
                .map(nineprofs_research::ResearchPdfExtractionId::parse)
                .transpose()?,
            method: citation_binding_method(request.method),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_target_binding_dto(
        binding,
    ))))
}

async fn get_citation_target_binding(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationTargetBindingDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(citation_target_binding_dto(
        state
            .runtime
            .research_service()
            .get_citation_target_binding(&id)
            .await?,
    ))))
}

async fn get_latest_citation_target_binding(
    State(state): State<AppState>,
    Path(target_id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationTargetBindingDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(citation_target_binding_dto(
        state
            .runtime
            .research_service()
            .latest_citation_target_binding(&target_id)
            .await?,
    ))))
}

async fn list_claim_citation_links(
    State(state): State<AppState>,
    Query(query): Query<ClaimCitationsQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ClaimCitationLinkDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_claim_citation_links(
                query.research_case_id.as_deref(),
                query.claim_id.as_deref(),
                query.citation_occurrence_id.as_deref(),
            )
            .await?
            .into_iter()
            .map(claim_citation_link_dto)
            .collect(),
    )))
}

async fn create_claim_citation_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateClaimCitationLinkRequest>,
) -> Result<axum::Json<ApiResponse<ClaimCitationLinkDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let link = state
        .runtime
        .research_service()
        .create_claim_citation_link(CreateClaimCitationLink {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            claim_id: nineprofs_research::ResearchClaimId::parse(request.claim_id)?,
            citation_occurrence_id: nineprofs_research::CitationOccurrenceId::parse(
                request.citation_occurrence_id,
            )?,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(claim_citation_link_dto(link))))
}

async fn get_claim_citation_link(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ClaimCitationLinkDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(claim_citation_link_dto(
        state
            .runtime
            .research_service()
            .get_claim_citation_link(&id)
            .await?,
    ))))
}

async fn create_citation_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateCitationVerificationRequest>,
) -> Result<axum::Json<ApiResponse<CitationVerificationRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .citation_verification_service()
        .verify(CreateCitationVerification {
            claim_citation_link_id: request.claim_citation_link_id,
            citation_target_binding_id: request.citation_target_binding_id,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_verification_run_dto(
        run,
    ))))
}

async fn get_citation_verification(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationVerificationRunDto>>, ApiError> {
    let run = state
        .runtime
        .citation_verification_service()
        .citation_verification(&id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_verification_run_dto(
        run,
    ))))
}

async fn list_claim_citation_verifications(
    State(state): State<AppState>,
    Path(claim_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<CitationVerificationRunDto>>>, ApiError> {
    let runs = state
        .runtime
        .citation_verification_service()
        .claim_citation_verifications(&claim_id)
        .await?
        .into_iter()
        .map(citation_verification_run_dto)
        .collect();
    Ok(axum::Json(ApiResponse::ok(runs)))
}

async fn get_research_retrieval_index(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchRetrievalIndexStateDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        research_retrieval_index_state_dto(state.runtime.dify_service().state(&id).await?),
    )))
}

async fn ensure_research_retrieval_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchRetrievalIndexDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    Ok(axum::Json(ApiResponse::ok(research_retrieval_index_dto(
        state.runtime.dify_service().ensure_case_index(&id).await?,
    ))))
}

async fn sync_research_retrieval_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((index_id, extraction_id)): Path<(String, String)>,
) -> Result<axum::Json<ApiResponse<ResearchExtractionRetrievalIndexDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    Ok(axum::Json(ApiResponse::ok(
        research_extraction_retrieval_index_dto(
            state
                .runtime
                .dify_service()
                .sync_extraction(&index_id, &extraction_id)
                .await?,
        ),
    )))
}

async fn retrieve_research_case(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(request): axum::Json<RetrieveResearchRequest>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchRetrievalCandidateDto>>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let scope = research_retrieval_scope(request.scope)?;
    scope
        .validate()
        .map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .dify_service()
            .retrieve_with_scope(&id, &scope, &request.query, request.top_k.unwrap_or(10))
            .await?
            .into_iter()
            .map(research_retrieval_candidate_dto)
            .collect(),
    )))
}

fn research_retrieval_scope(
    value: Option<ResearchRetrievalScopeDto>,
) -> Result<ResearchRetrievalScope, ApiError> {
    let Some(value) = value else {
        return Ok(ResearchRetrievalScope::Case);
    };
    let parse_source_ids = |ids: Vec<String>| {
        ids.into_iter()
            .map(ResearchSourceId::parse)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ApiError::InvalidRequest(error.to_string()))
    };
    let parse_extraction_ids = |ids: Vec<String>| {
        ids.into_iter()
            .map(ResearchPdfExtractionId::parse)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ApiError::InvalidRequest(error.to_string()))
    };
    match value {
        ResearchRetrievalScopeDto::Case => Ok(ResearchRetrievalScope::Case),
        ResearchRetrievalScopeDto::Sources { source_ids } => Ok(ResearchRetrievalScope::Sources {
            source_ids: parse_source_ids(source_ids)?,
        }),
        ResearchRetrievalScopeDto::Extractions { extraction_ids } => {
            Ok(ResearchRetrievalScope::Extractions {
                extraction_ids: parse_extraction_ids(extraction_ids)?,
            })
        }
    }
}

fn header_text(headers: &HeaderMap, name: &str) -> Result<Option<String>, ApiError> {
    headers
        .get(name)
        .map(|value| {
            value
                .to_str()
                .map(str::to_owned)
                .map_err(|_| ApiError::InvalidRequest(format!("invalid {name} header")))
        })
        .transpose()
}

fn safe_upload_label(value: Option<&str>, fallback: &str) -> Result<String, ApiError> {
    let value = value.unwrap_or(fallback);
    let label = value.rsplit(['/', '\\']).next().unwrap_or(value).trim();
    if label.is_empty() || label.len() > nineprofs_research::MAX_SOURCE_LABEL_BYTES {
        return Err(ApiError::InvalidRequest(
            "PDF filename/label is empty or too long".to_owned(),
        ));
    }
    if label.chars().any(char::is_control) {
        return Err(ApiError::InvalidRequest(
            "PDF filename/label contains control characters".to_owned(),
        ));
    }
    Ok(label.to_owned())
}

fn research_artifact_dto(value: nineprofs_research::ResearchArtifact) -> ResearchArtifactDto {
    ResearchArtifactDto {
        artifact_id: value.id,
        content_hash: research_content_hash_dto(value.content_hash),
        size_bytes: value.size_bytes,
        media_type: value.media_type,
        original_filename: value.original_filename,
        created_at_ms: value.created_at_ms,
    }
}

fn research_pdf_extraction_dto(value: ResearchPdfExtraction) -> ResearchPdfExtractionDto {
    ResearchPdfExtractionDto {
        extraction_id: value.id.to_string(),
        source_snapshot_id: value.source_snapshot_id.to_string(),
        artifact_id: value.artifact_id,
        extractor: value.extractor,
        extractor_version: value.extractor_version,
        page_count: value.page_count,
        extraction_hash: research_content_hash_dto(value.extraction_hash),
        extracted_at_ms: value.extracted_at_ms,
        status: pdf_extraction_status_dto(value.status),
    }
}

fn research_pdf_page_dto(value: ResearchPdfPage) -> ResearchPdfPageDto {
    ResearchPdfPageDto {
        extraction_id: value.extraction_id.to_string(),
        page: value.page,
        text: value.text,
        text_hash: research_content_hash_dto(value.text_hash),
    }
}

fn research_pdf_page_list_dto(value: ResearchPdfPageBatch) -> ResearchPdfPageListDto {
    let ResearchPdfPageBatch {
        pages,
        start_page,
        limit,
        has_more,
        next_start_page,
    } = value;
    ResearchPdfPageListDto {
        data: pages.into_iter().map(research_pdf_page_dto).collect(),
        start_page,
        limit,
        has_more,
        next_start_page,
    }
}

fn research_retrieval_index_state_dto(
    value: RetrievalIndexState,
) -> ResearchRetrievalIndexStateDto {
    ResearchRetrievalIndexStateDto {
        readiness: research_retrieval_readiness_dto(value.readiness),
        case_index: value.case_index.map(research_retrieval_index_dto),
        extraction_indexes: value
            .extraction_indexes
            .into_iter()
            .map(research_extraction_retrieval_index_dto)
            .collect(),
    }
}

fn research_retrieval_readiness_dto(value: DifyReadiness) -> ResearchRetrievalReadinessDto {
    ResearchRetrievalReadinessDto {
        provider: value.provider.to_owned(),
        qualification_target: value.qualification_target.to_owned(),
        configured: value.configured,
        status: match value.status {
            nineprofs_research_dify::DifyReadinessStatus::NotConfigured => {
                ResearchRetrievalReadinessStatusDto::NotConfigured
            }
            nineprofs_research_dify::DifyReadinessStatus::Configured => {
                ResearchRetrievalReadinessStatusDto::Configured
            }
            nineprofs_research_dify::DifyReadinessStatus::Unreachable => {
                ResearchRetrievalReadinessStatusDto::Unreachable
            }
            nineprofs_research_dify::DifyReadinessStatus::Reachable => {
                ResearchRetrievalReadinessStatusDto::Reachable
            }
            nineprofs_research_dify::DifyReadinessStatus::Unauthorized => {
                ResearchRetrievalReadinessStatusDto::Unauthorized
            }
            nineprofs_research_dify::DifyReadinessStatus::Ready => {
                ResearchRetrievalReadinessStatusDto::Ready
            }
        },
        reachable: value.reachable,
        authorized: value.authorized,
        ready: value.ready,
    }
}

fn research_retrieval_index_dto(value: DifyCaseIndex) -> ResearchRetrievalIndexDto {
    ResearchRetrievalIndexDto {
        index_id: value.index_id,
        research_case_id: value.research_case_id,
        dataset_id: value.dataset_id,
        status: dify_index_status_dto(value.status),
        failure_code: value.failure_code,
        created_at_ms: value.created_at_ms,
        updated_at_ms: value.updated_at_ms,
    }
}

fn research_extraction_retrieval_index_dto(
    value: DifyExtractionIndex,
) -> ResearchExtractionRetrievalIndexDto {
    ResearchExtractionRetrievalIndexDto {
        index_id: value.index_id,
        case_index_id: value.case_index_id,
        research_case_id: value.research_case_id,
        extraction_id: value.extraction_id,
        source_snapshot_id: value.source_snapshot_id,
        document_id: value.document_id,
        metadata_qualified: value.metadata_qualified,
        chunker_version: value.chunker_version,
        status: dify_index_status_dto(value.status),
        failure_code: value.failure_code,
        created_at_ms: value.created_at_ms,
        updated_at_ms: value.updated_at_ms,
    }
}

fn research_retrieval_candidate_dto(value: RetrievalCandidate) -> ResearchRetrievalCandidateDto {
    ResearchRetrievalCandidateDto {
        retrieval_chunk_id: value.retrieval_chunk_id,
        research_source_id: value.research_source_id,
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        page: value.page,
        start: value.start,
        end: value.end,
        verbatim_excerpt: value.verbatim_excerpt,
        retrieval_score: value.retrieval_score,
        provider: value.provider.to_owned(),
        rank: value.rank,
    }
}

fn dify_index_status_dto(value: DifyIndexStatus) -> ResearchRetrievalIndexStatusDto {
    match value {
        DifyIndexStatus::NotConfigured => ResearchRetrievalIndexStatusDto::NotConfigured,
        DifyIndexStatus::Provisioning => ResearchRetrievalIndexStatusDto::Provisioning,
        DifyIndexStatus::Ready => ResearchRetrievalIndexStatusDto::Ready,
        DifyIndexStatus::Syncing => ResearchRetrievalIndexStatusDto::Syncing,
        DifyIndexStatus::Failed => ResearchRetrievalIndexStatusDto::Failed,
        DifyIndexStatus::Degraded => ResearchRetrievalIndexStatusDto::Degraded,
    }
}

fn pdf_extraction_status(
    value: ResearchPdfExtractionStatusDto,
) -> nineprofs_research::PdfExtractionStatus {
    match value {
        ResearchPdfExtractionStatusDto::Ready => nineprofs_research::PdfExtractionStatus::Ready,
        ResearchPdfExtractionStatusDto::NoExtractableText => {
            nineprofs_research::PdfExtractionStatus::NoExtractableText
        }
        ResearchPdfExtractionStatusDto::Failed => nineprofs_research::PdfExtractionStatus::Failed,
        ResearchPdfExtractionStatusDto::PasswordRequired => {
            nineprofs_research::PdfExtractionStatus::PasswordRequired
        }
    }
}

fn pdf_extraction_status_dto(
    value: nineprofs_research::PdfExtractionStatus,
) -> ResearchPdfExtractionStatusDto {
    match value {
        nineprofs_research::PdfExtractionStatus::Ready => ResearchPdfExtractionStatusDto::Ready,
        nineprofs_research::PdfExtractionStatus::NoExtractableText => {
            ResearchPdfExtractionStatusDto::NoExtractableText
        }
        nineprofs_research::PdfExtractionStatus::Failed => ResearchPdfExtractionStatusDto::Failed,
        nineprofs_research::PdfExtractionStatus::PasswordRequired => {
            ResearchPdfExtractionStatusDto::PasswordRequired
        }
    }
}

fn research_case_dto(value: ResearchCase) -> ResearchCaseDto {
    ResearchCaseDto {
        case_id: value.id.to_string(),
        title: value.title,
        created_at_ms: value.created_at_ms,
        updated_at_ms: value.updated_at_ms,
    }
}

fn research_source_dto(value: ResearchSource) -> ResearchSourceDto {
    ResearchSourceDto {
        source_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        kind: source_kind_dto(value.kind),
        label: value.label,
        created_at_ms: value.created_at_ms,
    }
}

fn research_snapshot_dto(value: ResearchSourceSnapshot) -> ResearchSourceSnapshotDto {
    ResearchSourceSnapshotDto {
        snapshot_id: value.id.to_string(),
        source_id: value.source_id.to_string(),
        content_hash: research_content_hash_dto(value.content_hash),
        captured_at_ms: value.captured_at_ms,
        capture_method: capture_method_dto(value.capture_method),
        origin: source_origin_dto(value.origin),
        metadata: value.metadata,
    }
}

fn research_evidence_dto(value: ResearchEvidence) -> ResearchEvidenceDto {
    ResearchEvidenceDto {
        evidence_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        source_snapshot_id: value.source_snapshot_id.to_string(),
        verbatim_excerpt: value.verbatim_excerpt,
        normalized_text: value.normalized_text,
        locator: evidence_locator_dto(value.locator),
        excerpt_hash: research_content_hash_dto(value.excerpt_hash),
        captured_at_ms: value.captured_at_ms,
        capture_method: capture_method_dto(value.capture_method),
        pdf_extraction_id: value.pdf_extraction_id.map(|id| id.to_string()),
    }
}

fn research_claim_dto(value: ResearchClaim) -> ResearchClaimDto {
    ResearchClaimDto {
        claim_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        text: value.text,
        origin: claim_origin_dto(value.origin),
        created_at_ms: value.created_at_ms,
    }
}

fn claim_evidence_link_dto(value: nineprofs_research::ClaimEvidenceLink) -> ClaimEvidenceLinkDto {
    ClaimEvidenceLinkDto {
        link_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        claim_id: value.claim_id.to_string(),
        evidence_id: value.evidence_id.to_string(),
        relation: claim_evidence_relation_dto(value.relation),
        rationale: value.rationale,
        assessment_method: assessment_method_dto(value.assessment_method),
        assessment_metadata: value.assessment_metadata,
        created_at_ms: value.created_at_ms,
    }
}

fn citation_occurrence_dto(value: nineprofs_research::CitationOccurrence) -> CitationOccurrenceDto {
    CitationOccurrenceDto {
        occurrence_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        origin: citation_occurrence_origin_dto(value.origin),
        rendered_text: value.rendered_text,
        created_at_ms: value.created_at_ms,
    }
}

fn citation_target_dto(
    value: nineprofs_research::CitationTarget,
    resolution: nineprofs_research::CitationTargetResolution,
) -> CitationTargetDto {
    CitationTargetDto {
        target_id: value.id.to_string(),
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        ordinal: value.ordinal,
        reference_key: value.reference_key,
        cited_locator: value.cited_locator,
        resolution: citation_target_resolution_dto(resolution),
    }
}

fn manuscript_citation_sync_run_dto(
    value: nineprofs_research::ManuscriptCitationSyncRun,
) -> ManuscriptCitationSyncRunDto {
    ManuscriptCitationSyncRunDto {
        sync_run_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        manuscript_source_id: value.manuscript_source_id.to_string(),
        document_id: value.document_id,
        document_version: value.document_version,
        inventory_hash: research_content_hash_dto(value.inventory_hash),
        status: manuscript_citation_sync_status_dto(value.status),
        occurrence_count: value.occurrence_count,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
        failure_code: value.failure_code,
    }
}

fn manuscript_citation_sync_occurrence_dto(
    value: nineprofs_research::ManuscriptCitationSyncOccurrence,
) -> ManuscriptCitationSyncOccurrenceDto {
    ManuscriptCitationSyncOccurrenceDto {
        sync_occurrence_id: value.id.to_string(),
        sync_run_id: value.sync_run_id.to_string(),
        ordinal: value.ordinal,
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        document_block_id: value.document_block_id,
        start: value.start,
        end: value.end,
        format: manuscript_citation_sync_format_dto(value.format),
    }
}

fn manuscript_citation_sync_target_dto(
    value: nineprofs_research::ManuscriptCitationSyncTarget,
) -> ManuscriptCitationSyncTargetDto {
    ManuscriptCitationSyncTargetDto {
        sync_target_id: value.id.to_string(),
        sync_occurrence_id: value.sync_occurrence_id.to_string(),
        document_target_ordinal: value.document_target_ordinal,
        citation_target_id: value.citation_target_id.to_string(),
    }
}

fn manuscript_reference_catalog_run_dto(
    value: nineprofs_research::ManuscriptReferenceCatalogRun,
) -> ManuscriptReferenceCatalogRunDto {
    ManuscriptReferenceCatalogRunDto {
        catalog_run_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        manuscript_source_id: value.manuscript_source_id.to_string(),
        citation_sync_run_id: value.citation_sync_run_id.to_string(),
        document_id: value.document_id,
        document_version: value.document_version,
        catalog_hash: research_content_hash_dto(value.catalog_hash),
        entry_count: value.entry_count,
        target_mapping_count: value.target_mapping_count,
        status: manuscript_reference_catalog_status_dto(value.status),
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
        failure_code: value.failure_code,
    }
}

fn manuscript_reference_catalog_status_dto(
    value: nineprofs_research::ManuscriptReferenceCatalogStatus,
) -> ManuscriptReferenceCatalogStatusDto {
    match value {
        nineprofs_research::ManuscriptReferenceCatalogStatus::Running => {
            ManuscriptReferenceCatalogStatusDto::Running
        }
        nineprofs_research::ManuscriptReferenceCatalogStatus::Completed => {
            ManuscriptReferenceCatalogStatusDto::Completed
        }
        nineprofs_research::ManuscriptReferenceCatalogStatus::Failed => {
            ManuscriptReferenceCatalogStatusDto::Failed
        }
    }
}

fn manuscript_reference_entry_dto(
    value: nineprofs_research::ManuscriptReferenceEntry,
) -> ManuscriptReferenceEntryDto {
    ManuscriptReferenceEntryDto {
        entry_id: value.id.to_string(),
        catalog_run_id: value.catalog_run_id.to_string(),
        ordinal: value.ordinal,
        format: manuscript_citation_sync_format_dto(value.format),
        reference_key: value.reference_key,
        descriptor_hash: research_content_hash_dto(value.descriptor_hash),
        word_source: value.word_tag.map(|tag| ManuscriptReferenceWordSourceDto {
            tag,
            title: value.word_title.unwrap_or_default(),
            author: value.word_author.unwrap_or_default(),
            year: value.word_year.unwrap_or_default(),
        }),
        zotero: if value.zotero_item_id.is_some() || !value.zotero_uris.is_empty() {
            Some(ManuscriptReferenceZoteroDto {
                item_id: value.zotero_item_id,
                uris: value.zotero_uris,
            })
        } else {
            None
        },
        target_count: value.target_count,
    }
}

fn manuscript_reference_target_mapping_dto(
    value: nineprofs_research::ManuscriptReferenceTargetMapping,
) -> ManuscriptReferenceTargetMappingDto {
    ManuscriptReferenceTargetMappingDto {
        mapping_id: value.id.to_string(),
        catalog_run_id: value.catalog_run_id.to_string(),
        reference_entry_id: value.reference_entry_id.to_string(),
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        citation_target_id: value.citation_target_id.to_string(),
        document_target_ordinal: value.document_target_ordinal,
    }
}

fn manuscript_citation_sync_format_dto(
    value: nineprofs_research::ManuscriptCitationFormat,
) -> ManuscriptCitationFormatDto {
    match value {
        nineprofs_research::ManuscriptCitationFormat::WordNative => {
            ManuscriptCitationFormatDto::WordNative
        }
        nineprofs_research::ManuscriptCitationFormat::Zotero => ManuscriptCitationFormatDto::Zotero,
    }
}

fn manuscript_citation_sync_status_dto(
    value: nineprofs_research::ManuscriptCitationSyncStatus,
) -> ManuscriptCitationSyncStatusDto {
    match value {
        nineprofs_research::ManuscriptCitationSyncStatus::Running => {
            ManuscriptCitationSyncStatusDto::Running
        }
        nineprofs_research::ManuscriptCitationSyncStatus::Completed => {
            ManuscriptCitationSyncStatusDto::Completed
        }
        nineprofs_research::ManuscriptCitationSyncStatus::Failed => {
            ManuscriptCitationSyncStatusDto::Failed
        }
    }
}

fn manuscript_claim_extraction_run_dto(
    value: nineprofs_research::ManuscriptClaimExtractionRun,
) -> ManuscriptClaimExtractionRunDto {
    ManuscriptClaimExtractionRunDto {
        extraction_run_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        manuscript_source_id: value.manuscript_source_id.to_string(),
        citation_sync_run_id: value.citation_sync_run_id.to_string(),
        document_id: value.document_id,
        document_version: value.document_version,
        context_hash: research_content_hash_dto(value.context_hash),
        extractor_provider: value.extractor_provider,
        extractor_version: value.extractor_version,
        extractor_model_id: value.extractor_model_id,
        extraction_contract_version: value.extraction_contract_version,
        status: manuscript_claim_extraction_status_dto(value.status),
        claim_count: value.claim_count,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
        failure_code: value.failure_code,
    }
}

fn manuscript_claim_extraction_item_dto(
    value: nineprofs_research::ManuscriptClaimExtractionItem,
    claim_text: String,
    citation_occurrence_ids: Vec<String>,
    claim_citation_link_ids: Vec<String>,
) -> ManuscriptClaimExtractionItemDto {
    ManuscriptClaimExtractionItemDto {
        item_id: value.id.to_string(),
        extraction_run_id: value.extraction_run_id.to_string(),
        research_claim_id: value.research_claim_id.to_string(),
        document_block_id: value.document_block_id,
        source_start: value.source_start,
        source_end: value.source_end,
        source_excerpt: value.source_excerpt,
        source_excerpt_hash: research_content_hash_dto(value.source_excerpt_hash),
        ordinal: value.ordinal,
        claim_text,
        citation_occurrence_ids,
        claim_citation_link_ids,
    }
}

fn manuscript_claim_extraction_status_dto(
    value: nineprofs_research::ManuscriptClaimExtractionStatus,
) -> ManuscriptClaimExtractionStatusDto {
    match value {
        nineprofs_research::ManuscriptClaimExtractionStatus::Running => {
            ManuscriptClaimExtractionStatusDto::Running
        }
        nineprofs_research::ManuscriptClaimExtractionStatus::Completed => {
            ManuscriptClaimExtractionStatusDto::Completed
        }
        nineprofs_research::ManuscriptClaimExtractionStatus::Failed => {
            ManuscriptClaimExtractionStatusDto::Failed
        }
    }
}

fn manuscript_claim_extraction_coverage_dto(
    value: nineprofs_research::ManuscriptClaimExtractionCoverage,
) -> ManuscriptClaimExtractionCoverageDto {
    ManuscriptClaimExtractionCoverageDto {
        coverage_id: value.id.to_string(),
        extraction_run_id: value.extraction_run_id.to_string(),
        extraction_item_id: value.extraction_item_id.map(|id| id.to_string()),
        claim_citation_link_id: value.claim_citation_link_id.map(|id| id.to_string()),
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        status: match value.status {
            nineprofs_research::ManuscriptClaimExtractionCoverageStatus::AssociatedWithClaim => {
                ManuscriptClaimExtractionCoverageStatusDto::AssociatedWithClaim
            }
            nineprofs_research::ManuscriptClaimExtractionCoverageStatus::NoVerifiableClaim => {
                ManuscriptClaimExtractionCoverageStatusDto::NoVerifiableClaim
            }
        },
        reason: value.reason,
    }
}

fn manuscript_citation_sync_citation(
    value: ManuscriptCitationSyncCitationRequest,
) -> Result<nineprofs_research::ManuscriptCitationSyncCitationInput, ApiError> {
    Ok(nineprofs_research::ManuscriptCitationSyncCitationInput {
        format: match value.format {
            ManuscriptCitationFormatDto::WordNative => {
                nineprofs_research::ManuscriptCitationFormat::WordNative
            }
            ManuscriptCitationFormatDto::Zotero => {
                nineprofs_research::ManuscriptCitationFormat::Zotero
            }
        },
        rendered_text: value.rendered_text,
        block_id: value.block_id,
        start: value.start,
        end: value.end,
        targets: value
            .targets
            .into_iter()
            .map(|target: ManuscriptCitationSyncTargetRequest| {
                nineprofs_research::ManuscriptCitationSyncTargetInput {
                    ordinal: target.ordinal,
                    reference_key: target.reference_key,
                    cited_locator: target.cited_locator,
                }
            })
            .collect(),
    })
}

fn manuscript_reference_catalog_citation(
    value: ManuscriptReferenceCatalogCitationRequest,
) -> Result<nineprofs_research::ManuscriptReferenceCatalogCitationInput, ApiError> {
    Ok(
        nineprofs_research::ManuscriptReferenceCatalogCitationInput {
            citation_occurrence_id: value.citation_occurrence_id,
            block_id: value.block_id,
            start: value.start,
            end: value.end,
            format: match value.format {
                ManuscriptCitationFormatDto::WordNative => {
                    nineprofs_research::ManuscriptCitationFormat::WordNative
                }
                ManuscriptCitationFormatDto::Zotero => {
                    nineprofs_research::ManuscriptCitationFormat::Zotero
                }
            },
            targets: value
                .targets
                .into_iter()
                .map(manuscript_reference_catalog_target)
                .collect::<Result<Vec<_>, _>>()?,
        },
    )
}

fn manuscript_reference_catalog_target(
    value: ManuscriptReferenceCatalogTargetRequest,
) -> Result<nineprofs_research::ManuscriptReferenceCatalogTargetInput, ApiError> {
    Ok(nineprofs_research::ManuscriptReferenceCatalogTargetInput {
        citation_target_id: value.citation_target_id,
        ordinal: value.ordinal,
        reference_key: value.reference_key,
        word_source: value.word_source.map(|source| {
            nineprofs_research::ManuscriptReferenceCatalogWordSourceInput {
                tag: source.tag,
                title: source.title,
                author: source.author,
                year: source.year,
            }
        }),
        zotero: value.zotero.map(|zotero| {
            nineprofs_research::ManuscriptReferenceCatalogZoteroInput {
                item_id: zotero.item_id,
                uris: zotero.uris,
            }
        }),
    })
}

fn citation_target_binding_dto(
    value: nineprofs_research::CitationTargetBinding,
) -> CitationTargetBindingDto {
    let resolution = value.resolution();
    let pdf_verification_ready = value.pdf_verification_ready();
    CitationTargetBindingDto {
        binding_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        citation_target_id: value.citation_target_id.to_string(),
        source_id: value.source_id.to_string(),
        source_snapshot_id: value.source_snapshot_id.map(|id| id.to_string()),
        extraction_id: value.extraction_id.map(|id| id.to_string()),
        method: citation_binding_method_dto(value.method),
        resolution: citation_target_resolution_dto(resolution),
        pdf_verification_ready,
        created_at_ms: value.created_at_ms,
    }
}

fn claim_citation_link_dto(value: nineprofs_research::ClaimCitationLink) -> ClaimCitationLinkDto {
    ClaimCitationLinkDto {
        link_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        claim_id: value.claim_id.to_string(),
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        created_at_ms: value.created_at_ms,
    }
}

fn citation_verification_run_dto(value: CitationVerificationRun) -> CitationVerificationRunDto {
    CitationVerificationRunDto {
        run_id: value.run_id,
        research_case_id: value.research_case_id,
        claim_citation_link_id: value.claim_citation_link_id,
        citation_target_binding_id: value.citation_target_binding_id,
        claim_id: value.claim_id,
        citation_occurrence_id: value.citation_occurrence_id,
        citation_target_id: value.citation_target_id,
        source_id: value.source_id,
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        status: citation_verification_status_dto(value.status),
        failure_code: value.failure_code,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
        result: value.result.map(citation_verification_result_dto),
        candidates: value
            .candidates
            .into_iter()
            .map(citation_verification_candidate_dto)
            .collect(),
        evidence: value
            .evidence
            .into_iter()
            .map(citation_verification_evidence_dto)
            .collect(),
    }
}

fn citation_verification_candidate_dto(
    value: nineprofs_research_verification::CitationVerificationCandidate,
) -> CitationVerificationCandidateDto {
    CitationVerificationCandidateDto {
        verification_run_id: value.verification_run_id,
        retrieval_chunk_id: value.retrieval_chunk_id,
        research_source_id: value.research_source_id,
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        page: value.page,
        start: value.start,
        end: value.end,
        excerpt_hash: value.excerpt_hash,
        rank: value.rank,
        retrieval_score: value.retrieval_score,
    }
}

fn citation_verification_result_dto(
    value: nineprofs_research_verification::CitationVerificationResult,
) -> CitationVerificationResultDto {
    CitationVerificationResultDto {
        verification_run_id: value.verification_run_id,
        overall_relation: claim_evidence_relation_dto(value.overall_relation),
        rationale: value.rationale,
        assessor_provider: value.assessor_provider,
        assessor_version: value.assessor_version,
        assessor_model_id: value.assessor_model_id,
        assessment_contract_version: value.assessment_contract_version,
        completed_at_ms: value.completed_at_ms,
    }
}

fn citation_verification_evidence_dto(
    value: nineprofs_research_verification::CitationVerificationEvidence,
) -> CitationVerificationEvidenceDto {
    CitationVerificationEvidenceDto {
        verification_run_id: value.verification_run_id,
        retrieval_chunk_id: value.retrieval_chunk_id,
        evidence_id: value.evidence_id,
        claim_evidence_link_id: value.claim_evidence_link_id,
        relation: claim_evidence_relation_dto(value.relation),
    }
}

fn citation_verification_status_dto(
    value: nineprofs_research_verification::CitationVerificationStatus,
) -> CitationVerificationStatusDto {
    match value {
        nineprofs_research_verification::CitationVerificationStatus::Running => {
            CitationVerificationStatusDto::Running
        }
        nineprofs_research_verification::CitationVerificationStatus::Completed => {
            CitationVerificationStatusDto::Completed
        }
        nineprofs_research_verification::CitationVerificationStatus::Failed => {
            CitationVerificationStatusDto::Failed
        }
    }
}

fn source_kind(value: ResearchSourceKindDto) -> SourceKind {
    match value {
        ResearchSourceKindDto::ReferencePdf => SourceKind::ReferencePdf,
        ResearchSourceKindDto::Manuscript => SourceKind::Manuscript,
        ResearchSourceKindDto::Dataset => SourceKind::Dataset,
        ResearchSourceKindDto::Web => SourceKind::Web,
        ResearchSourceKindDto::Regulation => SourceKind::Regulation,
        ResearchSourceKindDto::Other => SourceKind::Other,
    }
}

fn source_kind_dto(value: SourceKind) -> ResearchSourceKindDto {
    match value {
        SourceKind::ReferencePdf => ResearchSourceKindDto::ReferencePdf,
        SourceKind::Manuscript => ResearchSourceKindDto::Manuscript,
        SourceKind::Dataset => ResearchSourceKindDto::Dataset,
        SourceKind::Web => ResearchSourceKindDto::Web,
        SourceKind::Regulation => ResearchSourceKindDto::Regulation,
        SourceKind::Other => ResearchSourceKindDto::Other,
    }
}

fn capture_method(value: ResearchCaptureMethodDto) -> CaptureMethod {
    match value {
        ResearchCaptureMethodDto::UserProvided => CaptureMethod::UserProvided,
        ResearchCaptureMethodDto::UploadedArtifact => CaptureMethod::UploadedArtifact,
        ResearchCaptureMethodDto::ActiveDocument => CaptureMethod::ActiveDocument,
        ResearchCaptureMethodDto::OfficeCli => CaptureMethod::OfficeCli,
        ResearchCaptureMethodDto::WebRetrieval => CaptureMethod::WebRetrieval,
        ResearchCaptureMethodDto::ExternalImport => CaptureMethod::ExternalImport,
    }
}

fn capture_method_dto(value: CaptureMethod) -> ResearchCaptureMethodDto {
    match value {
        CaptureMethod::UserProvided => ResearchCaptureMethodDto::UserProvided,
        CaptureMethod::UploadedArtifact => ResearchCaptureMethodDto::UploadedArtifact,
        CaptureMethod::ActiveDocument => ResearchCaptureMethodDto::ActiveDocument,
        CaptureMethod::OfficeCli => ResearchCaptureMethodDto::OfficeCli,
        CaptureMethod::WebRetrieval => ResearchCaptureMethodDto::WebRetrieval,
        CaptureMethod::ExternalImport => ResearchCaptureMethodDto::ExternalImport,
    }
}

fn source_origin(value: ResearchSourceOriginDto) -> SourceOrigin {
    match value {
        ResearchSourceOriginDto::UploadedArtifact {
            artifact_id,
            revision_id,
        } => SourceOrigin::UploadedArtifact {
            artifact_id,
            revision_id,
        },
        ResearchSourceOriginDto::ActiveDocumentSnapshot {
            document_id,
            document_version,
        } => SourceOrigin::ActiveDocumentSnapshot {
            document_id,
            document_version,
        },
        ResearchSourceOriginDto::OfficeCliArtifactRevision {
            artifact_id,
            revision_id,
        } => SourceOrigin::OfficeCliArtifactRevision {
            artifact_id,
            revision_id,
        },
        ResearchSourceOriginDto::WebRetrieval {
            url,
            retrieved_at_ms,
        } => SourceOrigin::WebRetrieval {
            url,
            retrieved_at_ms,
        },
        ResearchSourceOriginDto::ExternalImport {
            provider,
            external_reference,
        } => SourceOrigin::ExternalImport {
            provider,
            external_reference,
        },
    }
}

fn source_origin_dto(value: SourceOrigin) -> ResearchSourceOriginDto {
    match value {
        SourceOrigin::UploadedArtifact {
            artifact_id,
            revision_id,
        } => ResearchSourceOriginDto::UploadedArtifact {
            artifact_id,
            revision_id,
        },
        SourceOrigin::ActiveDocumentSnapshot {
            document_id,
            document_version,
        } => ResearchSourceOriginDto::ActiveDocumentSnapshot {
            document_id,
            document_version,
        },
        SourceOrigin::OfficeCliArtifactRevision {
            artifact_id,
            revision_id,
        } => ResearchSourceOriginDto::OfficeCliArtifactRevision {
            artifact_id,
            revision_id,
        },
        SourceOrigin::WebRetrieval {
            url,
            retrieved_at_ms,
        } => ResearchSourceOriginDto::WebRetrieval {
            url,
            retrieved_at_ms,
        },
        SourceOrigin::ExternalImport {
            provider,
            external_reference,
        } => ResearchSourceOriginDto::ExternalImport {
            provider,
            external_reference,
        },
    }
}

fn evidence_locator(value: ResearchEvidenceLocatorDto) -> EvidenceLocator {
    match value {
        ResearchEvidenceLocatorDto::TextRange { start, end } => {
            EvidenceLocator::TextRange { start, end }
        }
        ResearchEvidenceLocatorDto::Pdf { page, end_page } => {
            EvidenceLocator::Pdf { page, end_page }
        }
        ResearchEvidenceLocatorDto::PdfTextRange { page, start, end } => {
            EvidenceLocator::PdfTextRange { page, start, end }
        }
        ResearchEvidenceLocatorDto::Manuscript {
            block_id,
            start,
            end,
        } => EvidenceLocator::Manuscript {
            block_id,
            start,
            end,
        },
        ResearchEvidenceLocatorDto::Spreadsheet { sheet, range } => {
            EvidenceLocator::Spreadsheet { sheet, range }
        }
        ResearchEvidenceLocatorDto::Web {
            fragment,
            start,
            end,
        } => EvidenceLocator::Web {
            fragment,
            start,
            end,
        },
        ResearchEvidenceLocatorDto::Regulation {
            article,
            section,
            clause,
        } => EvidenceLocator::Regulation {
            article,
            section,
            clause,
        },
    }
}

fn evidence_locator_dto(value: EvidenceLocator) -> ResearchEvidenceLocatorDto {
    match value {
        EvidenceLocator::TextRange { start, end } => {
            ResearchEvidenceLocatorDto::TextRange { start, end }
        }
        EvidenceLocator::Pdf { page, end_page } => {
            ResearchEvidenceLocatorDto::Pdf { page, end_page }
        }
        EvidenceLocator::PdfTextRange { page, start, end } => {
            ResearchEvidenceLocatorDto::PdfTextRange { page, start, end }
        }
        EvidenceLocator::Manuscript {
            block_id,
            start,
            end,
        } => ResearchEvidenceLocatorDto::Manuscript {
            block_id,
            start,
            end,
        },
        EvidenceLocator::Spreadsheet { sheet, range } => {
            ResearchEvidenceLocatorDto::Spreadsheet { sheet, range }
        }
        EvidenceLocator::Web {
            fragment,
            start,
            end,
        } => ResearchEvidenceLocatorDto::Web {
            fragment,
            start,
            end,
        },
        EvidenceLocator::Regulation {
            article,
            section,
            clause,
        } => ResearchEvidenceLocatorDto::Regulation {
            article,
            section,
            clause,
        },
    }
}

fn citation_occurrence_origin(
    value: ResearchCitationOccurrenceOriginDto,
) -> Result<CitationOccurrenceOrigin, ApiError> {
    Ok(match value {
        ResearchCitationOccurrenceOriginDto::Manuscript {
            document_id,
            document_version,
            locator,
        } => CitationOccurrenceOrigin::Manuscript {
            document_id,
            document_version,
            locator: locator.map(evidence_locator),
        },
        ResearchCitationOccurrenceOriginDto::ManuscriptSnapshot {
            source_snapshot_id,
            locator,
        } => CitationOccurrenceOrigin::ManuscriptSnapshot {
            source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(
                source_snapshot_id,
            )?,
            locator: locator.map(evidence_locator),
        },
        ResearchCitationOccurrenceOriginDto::Imported { source } => {
            CitationOccurrenceOrigin::Imported { source }
        }
    })
}

fn citation_occurrence_origin_dto(
    value: CitationOccurrenceOrigin,
) -> ResearchCitationOccurrenceOriginDto {
    match value {
        CitationOccurrenceOrigin::Manuscript {
            document_id,
            document_version,
            locator,
        } => ResearchCitationOccurrenceOriginDto::Manuscript {
            document_id,
            document_version,
            locator: locator.map(evidence_locator_dto),
        },
        CitationOccurrenceOrigin::ManuscriptSnapshot {
            source_snapshot_id,
            locator,
        } => ResearchCitationOccurrenceOriginDto::ManuscriptSnapshot {
            source_snapshot_id: source_snapshot_id.to_string(),
            locator: locator.map(evidence_locator_dto),
        },
        CitationOccurrenceOrigin::Imported { source } => {
            ResearchCitationOccurrenceOriginDto::Imported { source }
        }
    }
}

fn citation_binding_method(value: ResearchCitationBindingMethodDto) -> CitationBindingMethod {
    match value {
        ResearchCitationBindingMethodDto::Human => CitationBindingMethod::Human,
        ResearchCitationBindingMethodDto::Imported => CitationBindingMethod::Imported,
        ResearchCitationBindingMethodDto::DeterministicResolver => {
            CitationBindingMethod::DeterministicResolver
        }
        ResearchCitationBindingMethodDto::Agent => CitationBindingMethod::Agent,
    }
}

fn citation_binding_method_dto(value: CitationBindingMethod) -> ResearchCitationBindingMethodDto {
    match value {
        CitationBindingMethod::Human => ResearchCitationBindingMethodDto::Human,
        CitationBindingMethod::Imported => ResearchCitationBindingMethodDto::Imported,
        CitationBindingMethod::DeterministicResolver => {
            ResearchCitationBindingMethodDto::DeterministicResolver
        }
        CitationBindingMethod::Agent => ResearchCitationBindingMethodDto::Agent,
    }
}

fn citation_target_resolution_dto(
    value: nineprofs_research::CitationTargetResolution,
) -> ResearchCitationTargetResolutionDto {
    match value {
        nineprofs_research::CitationTargetResolution::Unresolved => {
            ResearchCitationTargetResolutionDto::Unresolved
        }
        nineprofs_research::CitationTargetResolution::SourceBound => {
            ResearchCitationTargetResolutionDto::SourceBound
        }
        nineprofs_research::CitationTargetResolution::PdfExtractionBound => {
            ResearchCitationTargetResolutionDto::PdfExtractionBound
        }
    }
}

fn claim_origin(value: ResearchClaimOriginDto) -> ClaimOrigin {
    match value {
        ResearchClaimOriginDto::Manuscript {
            document_id,
            document_version,
            locator,
        } => ClaimOrigin::Manuscript {
            document_id,
            document_version,
            locator: locator.map(evidence_locator),
        },
        ResearchClaimOriginDto::User => ClaimOrigin::User,
        ResearchClaimOriginDto::Agent => ClaimOrigin::Agent,
        ResearchClaimOriginDto::Imported { source } => ClaimOrigin::Imported { source },
    }
}

fn claim_origin_dto(value: ClaimOrigin) -> ResearchClaimOriginDto {
    match value {
        ClaimOrigin::Manuscript {
            document_id,
            document_version,
            locator,
        } => ResearchClaimOriginDto::Manuscript {
            document_id,
            document_version,
            locator: locator.map(evidence_locator_dto),
        },
        ClaimOrigin::User => ResearchClaimOriginDto::User,
        ClaimOrigin::Agent => ResearchClaimOriginDto::Agent,
        ClaimOrigin::Imported { source } => ResearchClaimOriginDto::Imported { source },
    }
}

fn claim_evidence_relation(value: ResearchClaimEvidenceRelationDto) -> ClaimEvidenceRelation {
    match value {
        ResearchClaimEvidenceRelationDto::Supports => ClaimEvidenceRelation::Supports,
        ResearchClaimEvidenceRelationDto::Contradicts => ClaimEvidenceRelation::Contradicts,
        ResearchClaimEvidenceRelationDto::Contextualizes => ClaimEvidenceRelation::Contextualizes,
        ResearchClaimEvidenceRelationDto::Insufficient => ClaimEvidenceRelation::Insufficient,
    }
}

fn claim_evidence_relation_dto(value: ClaimEvidenceRelation) -> ResearchClaimEvidenceRelationDto {
    match value {
        ClaimEvidenceRelation::Supports => ResearchClaimEvidenceRelationDto::Supports,
        ClaimEvidenceRelation::Contradicts => ResearchClaimEvidenceRelationDto::Contradicts,
        ClaimEvidenceRelation::Contextualizes => ResearchClaimEvidenceRelationDto::Contextualizes,
        ClaimEvidenceRelation::Insufficient => ResearchClaimEvidenceRelationDto::Insufficient,
    }
}

fn assessment_method(value: ResearchAssessmentMethodDto) -> AssessmentMethod {
    match value {
        ResearchAssessmentMethodDto::Human => AssessmentMethod::Human,
        ResearchAssessmentMethodDto::DeterministicChecker => AssessmentMethod::DeterministicChecker,
        ResearchAssessmentMethodDto::Agent => AssessmentMethod::Agent,
        ResearchAssessmentMethodDto::ExternalService => AssessmentMethod::ExternalService,
    }
}

fn assessment_method_dto(value: AssessmentMethod) -> ResearchAssessmentMethodDto {
    match value {
        AssessmentMethod::Human => ResearchAssessmentMethodDto::Human,
        AssessmentMethod::DeterministicChecker => ResearchAssessmentMethodDto::DeterministicChecker,
        AssessmentMethod::Agent => ResearchAssessmentMethodDto::Agent,
        AssessmentMethod::ExternalService => ResearchAssessmentMethodDto::ExternalService,
    }
}

fn research_content_hash_dto(value: nineprofs_research::ContentHash) -> ResearchContentHashDto {
    ResearchContentHashDto {
        algorithm: match value.algorithm {
            HashAlgorithm::Sha256 => ResearchHashAlgorithmDto::Sha256,
        },
        value: value.value,
    }
}

fn mcp_transport_config(transport: McpTransportInputDto) -> McpTransportConfig {
    match transport {
        McpTransportInputDto::Stdio { command, args, env } => {
            McpTransportConfig::Stdio { command, args, env }
        }
        McpTransportInputDto::Sse { url, headers } => McpTransportConfig::Sse { url, headers },
        McpTransportInputDto::StreamableHttp { url, headers } => {
            McpTransportConfig::StreamableHttp { url, headers }
        }
    }
}

fn mcp_server_dto(server: &McpServerSnapshot) -> McpServerDto {
    McpServerDto {
        id: server.id.clone(),
        name: server.name.clone(),
        description: server.description.clone(),
        enabled: server.enabled,
        startup_timeout_ms: server.startup_timeout_ms,
        transport: match &server.transport {
            McpTransportSummary::Stdio {
                command,
                args,
                env_keys,
            } => McpTransportDto::Stdio {
                command: command.clone(),
                args: args.clone(),
                env_keys: env_keys.clone(),
            },
            McpTransportSummary::Sse { url, header_names } => McpTransportDto::Sse {
                url: url.clone(),
                header_names: header_names.clone(),
            },
            McpTransportSummary::StreamableHttp { url, header_names } => {
                McpTransportDto::StreamableHttp {
                    url: url.clone(),
                    header_names: header_names.clone(),
                }
            }
        },
        status: serde_json::to_value(&server.status)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "disconnected".to_owned()),
        last_connected: server.last_connected,
        error: server.error.clone(),
        supports_resources: server.supports_resources,
        tools: server
            .tools
            .iter()
            .map(|tool| McpToolDto {
                id: tool.id.clone(),
                name: tool.name.clone(),
                display_name: tool.display_name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
            })
            .collect(),
        created_at_ms: server.created_at_ms,
        updated_at_ms: server.updated_at_ms,
    }
}

fn assistant_dto(assistant: &Assistant) -> AssistantDto {
    AssistantDto {
        id: assistant.id.clone(),
        name: assistant.name.clone(),
        description: assistant.description.clone(),
        avatar: assistant.avatar.clone(),
        source: match assistant.source {
            nineprofs_assistant::AssistantSource::Builtin => "builtin".to_owned(),
            nineprofs_assistant::AssistantSource::Custom => "custom".to_owned(),
        },
        rules: assistant.rules.clone(),
        enabled: assistant.enabled,
        skill_ids: assistant.skill_ids.clone(),
        backend_agent_id: assistant.backend_agent_id.clone(),
        created_at_ms: assistant.created_at_ms,
        updated_at_ms: assistant.updated_at_ms,
    }
}

fn skill_catalog_dto(scan: nineprofs_skills::SkillScan, include_content: bool) -> SkillCatalogDto {
    SkillCatalogDto {
        skills: scan
            .skills
            .iter()
            .map(|skill| skill_dto(skill, include_content))
            .collect(),
        issues: scan
            .issues
            .iter()
            .map(|issue| SkillIssueDto {
                root: issue.root.display().to_string(),
                path: issue.path.as_ref().map(|path| path.display().to_string()),
                message: issue.message.clone(),
            })
            .collect(),
    }
}

fn skill_dto(skill: &Skill, include_content: bool) -> SkillDto {
    SkillDto {
        id: skill.id.clone(),
        name: skill.name.clone(),
        description: skill.description.clone(),
        source: match skill.source {
            SkillSource::Builtin => "builtin".to_owned(),
            SkillSource::Custom => "custom".to_owned(),
            SkillSource::Extension => "extension".to_owned(),
        },
        location: skill.location.display_path(),
        content: include_content.then(|| skill.content.clone()),
    }
}

#[cfg(test)]
mod research_api_tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::*;

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn research_writes_use_launch_secret_and_reads_are_safe() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("research-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(Arc::clone(&runtime));

        let request = Request::post("/api/research/cases")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"title":"Secure review"}"#))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let request = Request::post("/api/research/cases")
            .header("content-type", "application/json")
            .header(TRUSTED_DECISION_HEADER, "research-secret")
            .body(Body::from(r#"{"title":"Secure review"}"#))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let case = json_body(response).await;
        let case_id = case["data"]["caseId"].as_str().unwrap().to_owned();

        let response = router
            .oneshot(
                Request::get("/api/research/cases")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = json_body(response).await;
        assert_eq!(payload["data"][0]["caseId"], case_id);
        assert_eq!(payload["data"][0]["title"], "Secure review");
    }

    #[tokio::test]
    async fn citation_routes_require_launch_secret_and_round_trip_targets() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("citation-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(Arc::clone(&runtime));

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/citation-verifications")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"claimCitationLinkId":"claim-link","citationTargetBindingId":"binding"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/cases")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "citation-secret")
                    .body(Body::from(r#"{"title":"Citation API review"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let case_id = json_body(response).await["data"]["caseId"]
            .as_str()
            .unwrap()
            .to_owned();

        let occurrence_body = serde_json::json!({
            "researchCaseId": case_id,
            "origin": {"kind": "imported", "source": "citation-api-test"},
            "renderedText": "[12, 13]"
        });
        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/citation-occurrences")
                    .header("content-type", "application/json")
                    .body(Body::from(occurrence_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/citation-occurrences")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "citation-secret")
                    .body(Body::from(occurrence_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let occurrence_status = response.status();
        let occurrence_payload = json_body(response).await;
        assert_eq!(
            occurrence_status,
            StatusCode::OK,
            "unexpected occurrence HTTP response: {occurrence_payload}"
        );
        assert!(
            occurrence_payload["data"]["occurrenceId"].is_string(),
            "unexpected occurrence response: {occurrence_payload}"
        );
        let occurrence_id = occurrence_payload["data"]["occurrenceId"]
            .as_str()
            .unwrap()
            .to_owned();

        let target_body = serde_json::json!({
            "citationOccurrenceId": occurrence_id,
            "ordinal": 0,
            "referenceKey": "12",
            "citedLocator": "p. 42"
        });
        let response = router
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/research/citation-occurrences/{occurrence_id}/targets"
                ))
                .header("content-type", "application/json")
                .header(TRUSTED_DECISION_HEADER, "citation-secret")
                .body(Body::from(target_body.to_string()))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let target_payload = json_body(response).await;
        assert_eq!(target_payload["data"]["referenceKey"], "12");
        assert_eq!(target_payload["data"]["resolution"], "unresolved");

        let response = router
            .oneshot(
                Request::get(format!(
                    "/api/research/citation-occurrences/{occurrence_id}/targets"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let targets = json_body(response).await;
        assert_eq!(targets["data"].as_array().unwrap().len(), 1);
        assert_eq!(targets["data"][0]["referenceKey"], "12");
    }

    #[tokio::test]
    async fn manuscript_citation_sync_routes_are_trusted_idempotent_and_readable() {
        let mut config = nineprofs_runtime::RuntimeConfig::default();
        config.session_secret = Some(Arc::from("sync-secret"));
        let runtime = Arc::new(CoreRuntime::initialize_in_memory(config).await.unwrap());
        let router = build_router(Arc::clone(&runtime));

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/cases")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(r#"{"title":"Sync API review"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let case_id = json_body(response).await["data"]["caseId"]
            .as_str()
            .unwrap()
            .to_owned();

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/research/sources")
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(
                        serde_json::json!({
                            "researchCaseId": case_id,
                            "kind": "manuscript",
                            "label": "Draft"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let source_id = json_body(response).await["data"]["sourceId"]
            .as_str()
            .unwrap()
            .to_owned();

        let sync_path =
            format!("/api/research/cases/{case_id}/manuscripts/{source_id}/citations/sync");
        let sync_body = serde_json::json!({
            "documentId": "doc-1",
            "documentVersion": 3,
            "citations": [{
                "format": "zotero",
                "renderedText": "[12]",
                "blockId": "b7",
                "start": 13,
                "end": 17,
                "targets": [{"ordinal": 1, "referenceKey": "12", "citedLocator": "table:0"}]
            }]
        });
        let response = router
            .clone()
            .oneshot(
                Request::post(&sync_path)
                    .header("content-type", "application/json")
                    .body(Body::from(sync_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = router
            .clone()
            .oneshot(
                Request::post(&sync_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(sync_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let first = json_body(response).await["data"].clone();
        assert_eq!(first["status"], "completed");
        assert_eq!(first["occurrenceCount"], 1);
        let run_id = first["syncRunId"].as_str().unwrap().to_owned();

        let response = router
            .clone()
            .oneshot(
                Request::post(&sync_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(sync_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["data"]["syncRunId"], run_id);

        let mut changed_body = sync_body.clone();
        changed_body["citations"][0]["renderedText"] = serde_json::json!("[13]");
        let response = router
            .clone()
            .oneshot(
                Request::post(&sync_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(changed_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            json_body(response).await["code"],
            "manuscript_citation_sync_conflict"
        );

        let latest_path = format!("{sync_path}/latest");
        let response = router
            .clone()
            .oneshot(Request::get(latest_path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(json_body(response).await["data"]["syncRunId"], run_id);

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/manuscript-citation-sync-runs/{run_id}/occurrences"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let sync_occurrence = json_body(response).await["data"][0].clone();
        assert_eq!(sync_occurrence["start"], 13);
        assert_eq!(sync_occurrence["end"], 17);
        assert_eq!(sync_occurrence["format"], "zotero");
        let sync_occurrence_id = sync_occurrence["syncOccurrenceId"]
            .as_str()
            .unwrap()
            .to_owned();

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/manuscript-citation-sync-occurrences/{sync_occurrence_id}/targets"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let target = json_body(response).await["data"][0].clone();
        assert_eq!(target["documentTargetOrdinal"], 1);
        let citation_target_id = target["citationTargetId"].as_str().unwrap();

        let catalog_path =
            format!("/api/research/manuscript-citation-syncs/{run_id}/reference-catalog");
        let catalog_body = serde_json::json!({
            "documentId": "doc-1",
            "documentVersion": 3,
            "citations": [{
                "citationOccurrenceId": sync_occurrence["citationOccurrenceId"],
                "blockId": "b7",
                "start": 13,
                "end": 17,
                "format": "zotero",
                "targets": [{
                    "citationTargetId": citation_target_id,
                    "ordinal": 1,
                    "referenceKey": "12",
                    "zotero": {"itemId": "12", "uris": []}
                }]
            }]
        });

        let response = router
            .clone()
            .oneshot(
                Request::post(&catalog_path)
                    .header("content-type", "application/json")
                    .body(Body::from(catalog_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let mut override_body = catalog_body.clone();
        override_body["researchCaseId"] = serde_json::json!(case_id);
        let response = router
            .clone()
            .oneshot(
                Request::post(&catalog_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(override_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let response = router
            .clone()
            .oneshot(
                Request::post(&catalog_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(catalog_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let catalog = json_body(response).await["data"].clone();
        assert_eq!(catalog["status"], "completed");
        assert_eq!(catalog["entryCount"], 1);
        assert_eq!(catalog["targetMappingCount"], 1);
        let catalog_run_id = catalog["catalogRunId"].as_str().unwrap().to_owned();

        let response = router
            .clone()
            .oneshot(
                Request::post(&catalog_path)
                    .header("content-type", "application/json")
                    .header(TRUSTED_DECISION_HEADER, "sync-secret")
                    .body(Body::from(catalog_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"]["catalogRunId"],
            catalog_run_id
        );

        let response = router
            .clone()
            .oneshot(Request::get(&catalog_path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"]["catalogRunId"],
            catalog_run_id
        );

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/cases/{case_id}/manuscripts/{source_id}/reference-catalog/latest"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"]["catalogRunId"],
            catalog_run_id
        );

        let response = router
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/research/manuscript-reference-catalog-runs/{catalog_run_id}/entries"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let entry = json_body(response).await["data"][0].clone();
        assert_eq!(entry["format"], "zotero");
        assert_eq!(entry["referenceKey"], "12");
        let entry_id = entry["entryId"].as_str().unwrap();

        let response = router
            .oneshot(
                Request::get(format!(
                    "/api/research/manuscript-reference-entries/{entry_id}/mappings"
                ))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"][0]["citationTargetId"],
            citation_target_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;
    use tower::ServiceExt;

    async fn test_runtime() -> Arc<CoreRuntime> {
        Arc::new(
            CoreRuntime::initialize_in_memory(nineprofs_runtime::RuntimeConfig::default())
                .await
                .unwrap(),
        )
    }

    async fn test_router() -> Router {
        let runtime = test_runtime().await;
        build_router(runtime)
    }

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn health_endpoint_returns_stable_payload() {
        let response = test_router()
            .await
            .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["status"], "ok");
        assert_eq!(json["data"]["service"], "9profs-core");
    }

    #[tokio::test]
    async fn websocket_endpoint_accepts_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, test_router().await).await.unwrap();
        });

        let (mut socket, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
        socket.close(None).await.unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn assistant_list_get_and_custom_crud_endpoints_work() {
        let router = test_router().await;
        let response = router
            .clone()
            .oneshot(Request::get("/api/assistants").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = json_body(response).await;
        assert_eq!(payload["success"], true);
        assert!(
            payload["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["source"] == "builtin")
        );

        let create = Request::post("/api/assistants")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({
                    "id": "api-custom",
                    "name": "API Custom",
                    "description": "Created through API",
                    "rules": "Custom API rules",
                    "skill_ids": ["writing-foundation", "document-foundation"],
                    "backend_agent_id": "codex"
                })
                .to_string(),
            ))
            .unwrap();
        let response = router.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = json_body(response).await;
        assert_eq!(payload["data"]["source"], "custom");
        assert_eq!(
            payload["data"]["skill_ids"],
            serde_json::json!(["writing-foundation", "document-foundation"])
        );
        assert_eq!(payload["data"]["backend_agent_id"], "codex");

        let update = Request::put("/api/assistants/api-custom")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "rules": "Updated API rules" }).to_string(),
            ))
            .unwrap();
        let response = router.clone().oneshot(update).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_body(response).await["data"]["rules"],
            "Updated API rules"
        );

        let response = router
            .clone()
            .oneshot(
                Request::delete("/api/assistants/api-custom")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = router
            .oneshot(
                Request::get("/api/assistants/api-custom")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn skill_list_get_invalid_id_and_scan_event_work() {
        let runtime = test_runtime().await;
        let mut events = runtime.event_bus().subscribe();
        let router = build_router(Arc::clone(&runtime));

        let response = router
            .clone()
            .oneshot(Request::get("/api/skills").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = json_body(response).await;
        assert!(
            payload["data"]["skills"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["id"] == "document-foundation")
        );

        let response = router
            .clone()
            .oneshot(
                Request::get("/api/skills/document-foundation")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            json_body(response).await["data"]["content"]
                .as_str()
                .unwrap()
                .contains("Document Foundation")
        );

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/assistants")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "id": "../invalid",
                            "name": "Invalid",
                            "description": "Invalid"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let response = router
            .oneshot(
                Request::post("/api/skills/scan")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(events.recv().await.unwrap().name, "skill.catalogChanged");
    }

    #[tokio::test]
    async fn agent_run_routes_validate_and_report_invalid_ids() {
        let router = test_router().await;

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/agent-runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "assistant_id": "missing-assistant",
                            "input": "hello"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = router
            .clone()
            .oneshot(
                Request::post("/api/agent-runs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "assistant_id": "missing-assistant",
                            "input": "   "
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        for path in [
            "/api/agent-runs/not-a-run",
            "/api/agent-runs/not-a-run/tasks",
            "/api/agent-tasks/not-a-task/cancel",
        ] {
            let response = router
                .clone()
                .oneshot(if path.ends_with("cancel") {
                    Request::post(path).body(Body::empty()).unwrap()
                } else {
                    Request::get(path).body(Body::empty()).unwrap()
                })
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        }
    }
}
