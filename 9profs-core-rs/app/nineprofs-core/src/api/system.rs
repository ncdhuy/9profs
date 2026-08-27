use crate::api::AppState;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::HealthResponse;
use nineprofs_api_types::RuntimeInfo;

async fn health(State(state): State<AppState>) -> axum::Json<ApiResponse<HealthResponse>> {
    axum::Json(ApiResponse::ok(state.runtime.health()))
}

async fn runtime_info(State(state): State<AppState>) -> axum::Json<ApiResponse<RuntimeInfo>> {
    axum::Json(ApiResponse::ok(state.runtime.info()))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/runtime", get(runtime_info))
}
