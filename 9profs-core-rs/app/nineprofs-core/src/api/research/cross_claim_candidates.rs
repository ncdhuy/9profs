use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use nineprofs_api_types::{
    ApiResponse, CreateManuscriptCrossClaimCandidatesRequest, ManuscriptCrossClaimCandidateDto,
    ManuscriptCrossClaimCandidateKindDto, ManuscriptCrossClaimCandidateRunDto,
    ManuscriptCrossClaimCandidateRunStatusDto, ManuscriptCrossClaimComparisonWindowDto,
    ManuscriptCrossClaimComparisonWindowStatusDto,
};
use nineprofs_research_verification::{
    ManuscriptCrossClaimCandidate, ManuscriptCrossClaimCandidateKind,
    ManuscriptCrossClaimCandidateRun, ManuscriptCrossClaimCandidateRunStatus,
    ManuscriptCrossClaimComparisonWindow, ManuscriptCrossClaimComparisonWindowStatus,
    StartManuscriptCrossClaimCandidates,
};

use crate::api::proposals::authorize_trusted_decision;
use crate::api::{ApiError, AppState};

async fn start_candidates(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(research_case_id): Path<String>,
    axum::Json(request): axum::Json<CreateManuscriptCrossClaimCandidatesRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptCrossClaimCandidateRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .citation_review_service()
        .start_manuscript_cross_claim_candidates(StartManuscriptCrossClaimCandidates {
            research_case_id,
            claim_inventory_run_id: request.claim_inventory_run_id,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(candidate_run_dto(run))))
}

async fn get_candidates_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptCrossClaimCandidateRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(candidate_run_dto(
        state
            .runtime
            .citation_review_service()
            .get_manuscript_cross_claim_candidates_run(&run_id)
            .await?,
    ))))
}

async fn list_candidates(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptCrossClaimCandidateDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .citation_review_service()
            .list_manuscript_cross_claim_candidates(&run_id)
            .await?
            .into_iter()
            .map(candidate_dto)
            .collect(),
    )))
}

async fn list_windows(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptCrossClaimComparisonWindowDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .citation_review_service()
            .list_manuscript_cross_claim_candidate_windows(&run_id)
            .await?
            .into_iter()
            .map(window_dto)
            .collect(),
    )))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases/{research_case_id}/manuscript-cross-claim-candidates",
            post(start_candidates),
        )
        .route(
            "/api/research/manuscript-cross-claim-candidates/{run_id}",
            get(get_candidates_run),
        )
        .route(
            "/api/research/manuscript-cross-claim-candidates/{run_id}/candidates",
            get(list_candidates),
        )
        .route(
            "/api/research/manuscript-cross-claim-candidates/{run_id}/windows",
            get(list_windows),
        )
}

fn candidate_run_dto(
    value: ManuscriptCrossClaimCandidateRun,
) -> ManuscriptCrossClaimCandidateRunDto {
    ManuscriptCrossClaimCandidateRunDto {
        candidate_run_id: value.candidate_run_id,
        research_case_id: value.research_case_id,
        manuscript_source_id: value.manuscript_source_id,
        document_id: value.document_id,
        document_version: value.document_version,
        claim_inventory_run_id: value.claim_inventory_run_id,
        provider_id: value.provider_id,
        model_id: value.model_id,
        discovery_implementation_version: value.discovery_implementation_version,
        discovery_contract_version: value.discovery_contract_version,
        claim_count: value.claim_count,
        batch_count: value.batch_count,
        expected_window_count: value.expected_window_count,
        processed_window_count: value.processed_window_count,
        candidate_pair_count: value.candidate_pair_count,
        status: candidate_run_status_dto(value.status),
        failure_code: value.failure_code,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
    }
}

fn window_dto(
    value: ManuscriptCrossClaimComparisonWindow,
) -> ManuscriptCrossClaimComparisonWindowDto {
    ManuscriptCrossClaimComparisonWindowDto {
        window_id: value.window_id,
        candidate_run_id: value.candidate_run_id,
        left_batch_ordinal: value.left_batch_ordinal,
        right_batch_ordinal: value.right_batch_ordinal,
        same_batch: value.same_batch,
        status: window_status_dto(value.status),
        candidate_count: value.candidate_count,
        failure_code: value.failure_code,
    }
}

fn candidate_dto(value: ManuscriptCrossClaimCandidate) -> ManuscriptCrossClaimCandidateDto {
    ManuscriptCrossClaimCandidateDto {
        candidate_id: value.candidate_id,
        candidate_run_id: value.candidate_run_id,
        comparison_window_id: value.comparison_window_id,
        left_inventory_item_id: value.left_inventory_item_id,
        right_inventory_item_id: value.right_inventory_item_id,
        left_ordinal: value.left_ordinal,
        right_ordinal: value.right_ordinal,
        candidate_kinds: value
            .candidate_kinds
            .into_iter()
            .map(candidate_kind_dto)
            .collect(),
        rationale: value.rationale,
    }
}

fn candidate_run_status_dto(
    value: ManuscriptCrossClaimCandidateRunStatus,
) -> ManuscriptCrossClaimCandidateRunStatusDto {
    match value {
        ManuscriptCrossClaimCandidateRunStatus::Running => {
            ManuscriptCrossClaimCandidateRunStatusDto::Running
        }
        ManuscriptCrossClaimCandidateRunStatus::Completed => {
            ManuscriptCrossClaimCandidateRunStatusDto::Completed
        }
        ManuscriptCrossClaimCandidateRunStatus::Failed => {
            ManuscriptCrossClaimCandidateRunStatusDto::Failed
        }
    }
}

fn window_status_dto(
    value: ManuscriptCrossClaimComparisonWindowStatus,
) -> ManuscriptCrossClaimComparisonWindowStatusDto {
    match value {
        ManuscriptCrossClaimComparisonWindowStatus::Pending => {
            ManuscriptCrossClaimComparisonWindowStatusDto::Pending
        }
        ManuscriptCrossClaimComparisonWindowStatus::Processed => {
            ManuscriptCrossClaimComparisonWindowStatusDto::Processed
        }
        ManuscriptCrossClaimComparisonWindowStatus::Failed => {
            ManuscriptCrossClaimComparisonWindowStatusDto::Failed
        }
    }
}

fn candidate_kind_dto(
    value: ManuscriptCrossClaimCandidateKind,
) -> ManuscriptCrossClaimCandidateKindDto {
    match value {
        ManuscriptCrossClaimCandidateKind::PotentialDirectConflict => {
            ManuscriptCrossClaimCandidateKindDto::PotentialDirectConflict
        }
        ManuscriptCrossClaimCandidateKind::PotentialQuantitativeMismatch => {
            ManuscriptCrossClaimCandidateKindDto::PotentialQuantitativeMismatch
        }
        ManuscriptCrossClaimCandidateKind::PotentialDirectionMismatch => {
            ManuscriptCrossClaimCandidateKindDto::PotentialDirectionMismatch
        }
        ManuscriptCrossClaimCandidateKind::PotentialModalityMismatch => {
            ManuscriptCrossClaimCandidateKindDto::PotentialModalityMismatch
        }
        ManuscriptCrossClaimCandidateKind::PotentialCausalStrengthMismatch => {
            ManuscriptCrossClaimCandidateKindDto::PotentialCausalStrengthMismatch
        }
        ManuscriptCrossClaimCandidateKind::PotentialScopeMismatch => {
            ManuscriptCrossClaimCandidateKindDto::PotentialScopeMismatch
        }
        ManuscriptCrossClaimCandidateKind::PotentialTemporalMismatch => {
            ManuscriptCrossClaimCandidateKindDto::PotentialTemporalMismatch
        }
        ManuscriptCrossClaimCandidateKind::PotentialDefinitionMismatch => {
            ManuscriptCrossClaimCandidateKindDto::PotentialDefinitionMismatch
        }
        ManuscriptCrossClaimCandidateKind::PotentialDuplicateOrRestatement => {
            ManuscriptCrossClaimCandidateKindDto::PotentialDuplicateOrRestatement
        }
        ManuscriptCrossClaimCandidateKind::OtherConsistencyCandidate => {
            ManuscriptCrossClaimCandidateKindDto::OtherConsistencyCandidate
        }
    }
}
