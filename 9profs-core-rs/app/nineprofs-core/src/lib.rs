use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use nineprofs_agent::AgentBackendDescriptor;
use nineprofs_api_types::{
    ApiResponse, AssistantDto, CreateAssistantRequest, ErrorResponse, EventEnvelope,
    HealthResponse, RuntimeInfo, SkillCatalogDto, SkillDto, SkillIssueDto, UpdateAssistantRequest,
};
use nineprofs_assistant::{Assistant, AssistantError, CreateAssistant, UpdateAssistant};
use nineprofs_runtime::CoreRuntime;
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
}

pub fn build_router(runtime: Arc<CoreRuntime>) -> Router {
    let state = AppState { runtime };
    Router::new()
        .route("/api/health", get(health))
        .route("/api/runtime", get(runtime_info))
        .route("/api/agents", get(list_agents))
        .route("/api/agents/{id}", get(get_agent))
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
        .route("/ws", get(websocket))
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> axum::Json<ApiResponse<HealthResponse>> {
    axum::Json(ApiResponse::ok(state.runtime.health()))
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

async fn websocket(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    nineprofs_realtime::websocket_upgrade(upgrade, state.runtime.event_bus())
}

#[derive(Debug)]
enum ApiError {
    Assistant(AssistantError),
    NotFound(String),
    AgentNotFound(String),
}

impl From<AssistantError> for ApiError {
    fn from(error: AssistantError) -> Self {
        Self::Assistant(error)
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
    use futures_util::SinkExt;
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
}
