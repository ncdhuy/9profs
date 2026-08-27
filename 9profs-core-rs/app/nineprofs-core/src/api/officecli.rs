use crate::api::AppState;
use axum::Router;
use axum::extract::State;
use axum::routing::get;
use nineprofs_api_types::ApiResponse;
use nineprofs_officecli::OfficeCliStatus;

async fn officecli_status(
    State(state): State<AppState>,
) -> axum::Json<ApiResponse<OfficeCliStatus>> {
    axum::Json(ApiResponse::ok(state.runtime.officecli_status()))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/officecli/status", get(officecli_status))
}
