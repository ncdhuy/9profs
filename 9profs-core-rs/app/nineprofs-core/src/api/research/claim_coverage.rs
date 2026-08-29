use super::citation_review::{citation_review_evidence_dto, citation_review_item_status_dto};
use super::claims::claim_evidence_relation_dto;
use super::verification::citation_verification_status_dto;
use crate::api::proposals::authorize_trusted_decision;
use crate::api::{ApiError, AppState};
use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use nineprofs_api_types::{
    ApiResponse, CreateManuscriptClaimCoverageRequest, ManuscriptClaimCoverageBridgeStatusDto,
    ManuscriptClaimCoverageItemDto, ManuscriptClaimCoverageRunDto,
    ManuscriptClaimCoverageRunStatusDto, ManuscriptClaimCoverageStructuralCitationStateDto,
    ManuscriptClaimCoverageTargetDto,
};
use nineprofs_research_verification::{
    ManuscriptClaimCoverageBridgeStatus, ManuscriptClaimCoverageItem, ManuscriptClaimCoverageRun,
    ManuscriptClaimCoverageRunStatus, ManuscriptClaimCoverageStructuralCitationState,
    ManuscriptClaimCoverageTarget, StartManuscriptClaimCoverage,
};

async fn start_coverage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(research_case_id): Path<String>,
    axum::Json(request): axum::Json<CreateManuscriptClaimCoverageRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptClaimCoverageRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .citation_review_service()
        .start_manuscript_claim_coverage(StartManuscriptClaimCoverage {
            research_case_id,
            claim_inventory_run_id: request.claim_inventory_run_id,
            citation_review_run_id: request.citation_review_run_id,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_claim_coverage_run_dto(run),
    )))
}

async fn get_coverage(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptClaimCoverageRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_claim_coverage_run_dto(
            state
                .runtime
                .citation_review_service()
                .get_manuscript_claim_coverage(&run_id)
                .await?,
        ),
    )))
}

async fn list_coverage_items(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptClaimCoverageItemDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .citation_review_service()
            .list_manuscript_claim_coverage_items(&run_id)
            .await?
            .into_iter()
            .map(manuscript_claim_coverage_item_dto)
            .collect(),
    )))
}

async fn list_coverage_targets(
    State(state): State<AppState>,
    Path((run_id, item_id)): Path<(String, String)>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptClaimCoverageTargetDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .citation_review_service()
            .list_manuscript_claim_coverage_targets(&run_id, &item_id)
            .await?
            .into_iter()
            .map(manuscript_claim_coverage_target_dto)
            .collect(),
    )))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases/{research_case_id}/manuscript-claim-coverages",
            post(start_coverage),
        )
        .route(
            "/api/research/manuscript-claim-coverages/{run_id}",
            get(get_coverage),
        )
        .route(
            "/api/research/manuscript-claim-coverages/{run_id}/items",
            get(list_coverage_items),
        )
        .route(
            "/api/research/manuscript-claim-coverages/{run_id}/items/{item_id}/targets",
            get(list_coverage_targets),
        )
}

fn manuscript_claim_coverage_run_dto(
    value: ManuscriptClaimCoverageRun,
) -> ManuscriptClaimCoverageRunDto {
    ManuscriptClaimCoverageRunDto {
        coverage_run_id: value.coverage_run_id,
        research_case_id: value.research_case_id,
        manuscript_source_id: value.manuscript_source_id,
        document_id: value.document_id,
        document_version: value.document_version,
        claim_inventory_run_id: value.claim_inventory_run_id,
        citation_review_run_id: value.citation_review_run_id,
        analysis_contract_version: value.analysis_contract_version,
        coverage_contract_version: value.coverage_contract_version,
        coverage_scope: value.coverage_scope,
        coverage_limitations: value.coverage_limitations,
        status: manuscript_claim_coverage_run_status_dto(value.status),
        item_count: value.item_count,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
    }
}

fn manuscript_claim_coverage_item_dto(
    value: ManuscriptClaimCoverageItem,
) -> ManuscriptClaimCoverageItemDto {
    ManuscriptClaimCoverageItemDto {
        coverage_item_id: value.coverage_item_id,
        coverage_run_id: value.coverage_run_id,
        inventory_item_id: value.inventory_item_id,
        ordinal: value.ordinal,
        bridge_status: manuscript_claim_coverage_bridge_status_dto(value.bridge_status),
        structural_citation_state: manuscript_claim_coverage_structural_state_dto(
            value.structural_citation_state,
        ),
        matched_claim_extraction_item_id: value.matched_claim_extraction_item_id,
        matched_research_claim_id: value.matched_research_claim_id,
        inventory_overlapping_citation_count: value.inventory_overlapping_citation_count,
        same_block_citation_count: value.same_block_citation_count,
        claim_range_citation_count: value.claim_range_citation_count,
        exact_claim_citation_link_count: value.exact_claim_citation_link_count,
        target_count: value.target_count,
        support_count: value.support_count,
        contradiction_count: value.contradiction_count,
        contextualize_count: value.contextualize_count,
        insufficient_count: value.insufficient_count,
        unverified_count: value.unverified_count,
        blocked_count: value.blocked_count,
    }
}

fn manuscript_claim_coverage_target_dto(
    value: ManuscriptClaimCoverageTarget,
) -> ManuscriptClaimCoverageTargetDto {
    ManuscriptClaimCoverageTargetDto {
        coverage_target_id: value.coverage_target_id,
        coverage_item_id: value.coverage_item_id,
        claim_citation_link_id: value.claim_citation_link_id,
        citation_occurrence_id: value.citation_occurrence_id,
        citation_target_id: value.citation_target_id,
        citation_review_item_id: value.citation_review_item_id,
        binding_id: value.binding_id,
        source_id: value.source_id,
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        verification_run_id: value.verification_run_id,
        review_status: citation_review_item_status_dto(value.review_status),
        failure_code: value.failure_code,
        verification_status: value
            .verification_status
            .map(citation_verification_status_dto),
        verification_failure_code: value.verification_failure_code,
        relation: value.relation.map(claim_evidence_relation_dto),
        rationale: value.rationale,
        evidence_count: value.evidence_count,
        evidence: value
            .evidence
            .into_iter()
            .map(citation_review_evidence_dto)
            .collect(),
    }
}

fn manuscript_claim_coverage_run_status_dto(
    value: ManuscriptClaimCoverageRunStatus,
) -> ManuscriptClaimCoverageRunStatusDto {
    match value {
        ManuscriptClaimCoverageRunStatus::Running => ManuscriptClaimCoverageRunStatusDto::Running,
        ManuscriptClaimCoverageRunStatus::Completed => {
            ManuscriptClaimCoverageRunStatusDto::Completed
        }
        ManuscriptClaimCoverageRunStatus::Failed => ManuscriptClaimCoverageRunStatusDto::Failed,
    }
}

fn manuscript_claim_coverage_bridge_status_dto(
    value: ManuscriptClaimCoverageBridgeStatus,
) -> ManuscriptClaimCoverageBridgeStatusDto {
    match value {
        ManuscriptClaimCoverageBridgeStatus::ExactClaimBridge => {
            ManuscriptClaimCoverageBridgeStatusDto::ExactClaimBridge
        }
        ManuscriptClaimCoverageBridgeStatus::NoCitationScopedClaimMatch => {
            ManuscriptClaimCoverageBridgeStatusDto::NoCitationScopedClaimMatch
        }
        ManuscriptClaimCoverageBridgeStatus::SameSpanDifferentClaim => {
            ManuscriptClaimCoverageBridgeStatusDto::SameSpanDifferentClaim
        }
        ManuscriptClaimCoverageBridgeStatus::MultipleExactCandidates => {
            ManuscriptClaimCoverageBridgeStatusDto::MultipleExactCandidates
        }
        ManuscriptClaimCoverageBridgeStatus::InvalidCrossHistory => {
            ManuscriptClaimCoverageBridgeStatusDto::InvalidCrossHistory
        }
    }
}

fn manuscript_claim_coverage_structural_state_dto(
    value: ManuscriptClaimCoverageStructuralCitationState,
) -> ManuscriptClaimCoverageStructuralCitationStateDto {
    match value {
        ManuscriptClaimCoverageStructuralCitationState::ExactCitationLinked => {
            ManuscriptClaimCoverageStructuralCitationStateDto::ExactCitationLinked
        }
        ManuscriptClaimCoverageStructuralCitationState::CitationObservedInClaimRange => {
            ManuscriptClaimCoverageStructuralCitationStateDto::CitationObservedInClaimRange
        }
        ManuscriptClaimCoverageStructuralCitationState::CitationObservedInBlock => {
            ManuscriptClaimCoverageStructuralCitationStateDto::CitationObservedInBlock
        }
        ManuscriptClaimCoverageStructuralCitationState::NoCitationObservedInBlock => {
            ManuscriptClaimCoverageStructuralCitationStateDto::NoCitationObservedInBlock
        }
        ManuscriptClaimCoverageStructuralCitationState::AmbiguousClaimBridge => {
            ManuscriptClaimCoverageStructuralCitationStateDto::AmbiguousClaimBridge
        }
    }
}
