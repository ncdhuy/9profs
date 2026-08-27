use crate::api::ApiError;
use crate::api::AppState;
use crate::api::proposals::authorize_trusted_decision;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::CreateResearchCaseRequest;
use nineprofs_api_types::ResearchCaseDto;
use nineprofs_research::CreateResearchCase;
use nineprofs_research::ResearchCase;

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

pub(crate) fn research_case_dto(value: ResearchCase) -> ResearchCaseDto {
    ResearchCaseDto {
        case_id: value.id.to_string(),
        title: value.title,
        created_at_ms: value.created_at_ms,
        updated_at_ms: value.updated_at_ms,
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases",
            get(list_research_cases).post(create_research_case),
        )
        .route("/api/research/cases/{id}", get(get_research_case))
}
