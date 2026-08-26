use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use nineprofs_agent::{AgentBackendDescriptor, AgentRunContext, AgentTaskId, RunId, TaskState};
use nineprofs_api_types::{
    ActiveDocsAgentRunRequest, ActiveDocumentDto, AgentRunContextDto, AgentRunDto, AgentRunRequest,
    AgentRunStartedDto, AgentTaskDto, AgentTaskFailureDto, ApiResponse, AssistantDto,
    CreateAssistantRequest, CreateMcpServerRequest, DocsAgentProfile, DocumentProposalChangeDto,
    DocumentProposalDto, ErrorResponse, EventEnvelope, HealthResponse, McpConnectionTestDto,
    McpServerDto, McpToolDto, McpTransportDto, McpTransportInputDto, RuntimeInfo, SkillCatalogDto,
    SkillDto, SkillIssueDto, UpdateAssistantRequest, UpdateMcpServerRequest,
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
    ProposalWorkflow(ProposalWorkflowError),
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
