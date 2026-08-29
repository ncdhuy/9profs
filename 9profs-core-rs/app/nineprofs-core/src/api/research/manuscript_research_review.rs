use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use nineprofs_api_types::ApiResponse;
use nineprofs_research::{
    ManuscriptClaimInventoryBlockInput, ManuscriptClaimInventoryCitationInput,
};
use nineprofs_research_verification::{
    CitationReviewBlockInput, CitationReviewCitationInput,
    ManuscriptResearchReviewCitationObservations,
    ManuscriptResearchReviewClaimInventoryObservations, StartManuscriptResearchReview,
};
use serde::Deserialize;

use crate::api::proposals::authorize_trusted_decision;
use crate::api::{ApiError, AppState};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartManuscriptResearchReviewRequest {
    manuscript_source_id: String,
    document_id: String,
    document_version: i64,
    citation_review_observations: CitationObservationsRequest,
    claim_inventory_observations: ClaimInventoryObservationsRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CitationObservationsRequest {
    citations: Vec<CitationReviewCitationInput>,
    citation_blocks: Vec<CitationReviewBlockInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaimInventoryObservationsRequest {
    whole_manuscript_blocks: Vec<ClaimInventoryBlockRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaimInventoryBlockRequest {
    block_id: String,
    block_ordinal: u32,
    block_kind: nineprofs_research::ManuscriptClaimInventoryBlockKind,
    text: String,
    #[serde(default)]
    citations: Vec<ClaimInventoryCitationRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaimInventoryCitationRequest {
    start: u64,
    end: u64,
    rendered_text: String,
}

async fn start_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(research_case_id): Path<String>,
    axum::Json(request): axum::Json<StartManuscriptResearchReviewRequest>,
) -> Result<
    axum::Json<ApiResponse<nineprofs_research_verification::ManuscriptResearchReviewRun>>,
    ApiError,
> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let request = StartManuscriptResearchReview {
        research_case_id,
        manuscript_source_id: request.manuscript_source_id,
        document_id: request.document_id,
        document_version: request.document_version,
        citation_review_observations: ManuscriptResearchReviewCitationObservations {
            citations: request.citation_review_observations.citations,
            citation_blocks: request.citation_review_observations.citation_blocks,
        },
        claim_inventory_observations: ManuscriptResearchReviewClaimInventoryObservations {
            whole_manuscript_blocks: request
                .claim_inventory_observations
                .whole_manuscript_blocks
                .into_iter()
                .map(|block| ManuscriptClaimInventoryBlockInput {
                    block_id: block.block_id,
                    block_ordinal: block.block_ordinal,
                    block_kind: block.block_kind,
                    text: block.text,
                    citations: block
                        .citations
                        .into_iter()
                        .map(|citation| ManuscriptClaimInventoryCitationInput {
                            start: citation.start,
                            end: citation.end,
                            rendered_text: citation.rendered_text,
                        })
                        .collect(),
                })
                .collect(),
        },
    };
    let run = state
        .runtime
        .citation_review_service()
        .start_manuscript_research_review(request)
        .await?;
    Ok(axum::Json(ApiResponse::ok(run)))
}

async fn get_review(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<
    axum::Json<ApiResponse<nineprofs_research_verification::ManuscriptResearchReviewRun>>,
    ApiError,
> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .citation_review_service()
            .get_manuscript_research_review(&run_id)
            .await?,
    )))
}

async fn list_claims(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<
    axum::Json<
        ApiResponse<Vec<nineprofs_research_verification::ManuscriptResearchReviewClaimItem>>,
    >,
    ApiError,
> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .citation_review_service()
            .list_manuscript_research_review_claims(&run_id)
            .await?,
    )))
}

async fn list_consistency(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<
    axum::Json<
        ApiResponse<Vec<nineprofs_research_verification::ManuscriptResearchReviewConsistencyItem>>,
    >,
    ApiError,
> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .citation_review_service()
            .list_manuscript_research_review_consistency(&run_id)
            .await?,
    )))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases/{research_case_id}/manuscript-research-reviews",
            post(start_review),
        )
        .route(
            "/api/research/manuscript-research-reviews/{run_id}",
            get(get_review),
        )
        .route(
            "/api/research/manuscript-research-reviews/{run_id}/claims",
            get(list_claims),
        )
        .route(
            "/api/research/manuscript-research-reviews/{run_id}/consistency",
            get(list_consistency),
        )
}
