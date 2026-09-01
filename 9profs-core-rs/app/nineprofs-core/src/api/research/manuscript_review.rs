use crate::api::proposals::authorize_trusted_decision;
use crate::api::{ApiError, AppState};
use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use nineprofs_api_types::ApiResponse;
use nineprofs_research::ResearchContext;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunManuscriptReviewRequest {
    document_id: String,
    context: ResearchContextRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResearchContextRequest {
    language: Option<String>,
    #[serde(default)]
    research_families: Vec<String>,
    artifact_type: Option<String>,
    academic_level: Option<String>,
    #[serde(default)]
    study_designs: Vec<String>,
    #[serde(default)]
    reporting_guidelines: Vec<String>,
    organization: Option<String>,
}

impl From<ResearchContextRequest> for ResearchContext {
    fn from(request: ResearchContextRequest) -> Self {
        Self {
            language: request.language,
            research_families: request.research_families,
            artifact_type: request.artifact_type,
            academic_level: request.academic_level,
            study_designs: request.study_designs,
            reporting_guidelines: request.reporting_guidelines,
            organization: request.organization,
        }
    }
}

async fn run_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<RunManuscriptReviewRequest>,
) -> Result<axum::Json<ApiResponse<nineprofs_research::ManuscriptReviewResult>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    if request.document_id.trim().is_empty() {
        return Err(ApiError::InvalidRequest(
            "document_id is required".to_owned(),
        ));
    }

    let result = state
        .runtime
        .run_manuscript_review(&request.document_id, request.context.into())
        .await?;
    Ok(axum::Json(ApiResponse::ok(result)))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/api/research/manuscript-reviews", post(run_review))
}
