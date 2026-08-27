use crate::api::ApiError;
use crate::api::AppState;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::routing::get;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::AssistantDto;
use nineprofs_api_types::CreateAssistantRequest;
use nineprofs_api_types::UpdateAssistantRequest;
use nineprofs_assistant::Assistant;
use nineprofs_assistant::CreateAssistant;
use nineprofs_assistant::UpdateAssistant;

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

pub(super) fn assistant_dto(assistant: &Assistant) -> AssistantDto {
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

pub(super) fn router() -> Router<AppState> {
    Router::new()
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
}
