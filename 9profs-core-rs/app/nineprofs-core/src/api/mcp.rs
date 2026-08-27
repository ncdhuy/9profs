use crate::api::ApiError;
use crate::api::AppState;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::routing::get;
use axum::routing::post;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::CreateMcpServerRequest;
use nineprofs_api_types::McpConnectionTestDto;
use nineprofs_api_types::McpServerDto;
use nineprofs_api_types::McpToolDto;
use nineprofs_api_types::McpTransportDto;
use nineprofs_api_types::McpTransportInputDto;
use nineprofs_api_types::UpdateMcpServerRequest;
use nineprofs_mcp::CreateMcpServer;
use nineprofs_mcp::McpServerSnapshot;
use nineprofs_mcp::McpTransportConfig;
use nineprofs_mcp::McpTransportSummary;
use nineprofs_mcp::UpdateMcpServer;

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

pub(super) fn mcp_transport_config(transport: McpTransportInputDto) -> McpTransportConfig {
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

pub(super) fn mcp_server_dto(server: &McpServerSnapshot) -> McpServerDto {
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

pub(super) fn router() -> Router<AppState> {
    Router::new()
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
}
