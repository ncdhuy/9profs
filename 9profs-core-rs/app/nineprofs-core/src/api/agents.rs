use crate::api::ApiError;
use crate::api::AppState;
use crate::api::proposals::authorize_trusted_decision;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::routing::post;
use nineprofs_agent::AgentBackendDescriptor;
use nineprofs_agent::AgentRunContext;
use nineprofs_agent::AgentTaskId;
use nineprofs_agent::RunId;
use nineprofs_agent::TaskState;
use nineprofs_api_types::ActiveDocsAgentRunRequest;
use nineprofs_api_types::AgentRunContextDto;
use nineprofs_api_types::AgentRunDto;
use nineprofs_api_types::AgentRunRequest;
use nineprofs_api_types::AgentRunStartedDto;
use nineprofs_api_types::AgentTaskDto;
use nineprofs_api_types::AgentTaskFailureDto;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::CreateDocumentAgentConversationRequest;
use nineprofs_api_types::CreateDocumentAgentConversationRunRequest;
use nineprofs_api_types::DocsAgentProfile;
use nineprofs_api_types::DocumentAgentConversationDto;

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

pub(super) fn document_agent_conversation_dto(
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

pub(super) fn task_dto(task: &nineprofs_agent::AgentTask) -> AgentTaskDto {
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

pub(super) fn agent_run_context_dto(
    context: Option<&AgentRunContext>,
) -> Option<AgentRunContextDto> {
    context.map(|context| match context {
        AgentRunContext::ActiveDocs { document_id } => AgentRunContextDto::ActiveDocs {
            document_id: document_id.clone(),
        },
    })
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
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
}
