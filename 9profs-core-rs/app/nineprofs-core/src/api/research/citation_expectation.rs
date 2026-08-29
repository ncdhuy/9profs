use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use nineprofs_api_types::{
    ApiResponse, CitationExpectationAssessmentStatusDto, CitationExpectationDto,
    CoverageAttentionReasonDto, CoverageAttentionStateDto,
    CreateManuscriptCitationExpectationRequest, ManuscriptCitationExpectationItemDto,
    ManuscriptCitationExpectationRunDto, ManuscriptCitationExpectationRunStatusDto,
};
use nineprofs_research_verification::{
    CitationExpectation, CitationExpectationAssessmentStatus, CoverageAttentionReason,
    CoverageAttentionState, ManuscriptCitationExpectationItem, ManuscriptCitationExpectationRun,
    ManuscriptCitationExpectationRunStatus, StartManuscriptCitationExpectation,
};

use crate::api::proposals::authorize_trusted_decision;
use crate::api::{ApiError, AppState};

async fn start_expectation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(research_case_id): Path<String>,
    axum::Json(request): axum::Json<CreateManuscriptCitationExpectationRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptCitationExpectationRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .citation_review_service()
        .start_manuscript_citation_expectation(StartManuscriptCitationExpectation {
            research_case_id,
            claim_coverage_run_id: request.claim_coverage_run_id,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        manuscript_citation_expectation_run_dto(run),
    )))
}

async fn get_expectation(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptCitationExpectationRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        manuscript_citation_expectation_run_dto(
            state
                .runtime
                .citation_review_service()
                .get_manuscript_citation_expectation(&run_id)
                .await?,
        ),
    )))
}

async fn list_expectation_items(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptCitationExpectationItemDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .citation_review_service()
            .list_manuscript_citation_expectation_items(&run_id)
            .await?
            .into_iter()
            .map(manuscript_citation_expectation_item_dto)
            .collect(),
    )))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases/{research_case_id}/manuscript-citation-expectations",
            post(start_expectation),
        )
        .route(
            "/api/research/manuscript-citation-expectations/{run_id}",
            get(get_expectation),
        )
        .route(
            "/api/research/manuscript-citation-expectations/{run_id}/items",
            get(list_expectation_items),
        )
}

fn manuscript_citation_expectation_run_dto(
    value: ManuscriptCitationExpectationRun,
) -> ManuscriptCitationExpectationRunDto {
    ManuscriptCitationExpectationRunDto {
        expectation_run_id: value.expectation_run_id,
        research_case_id: value.research_case_id,
        claim_coverage_run_id: value.claim_coverage_run_id,
        provider_id: value.provider_id,
        assessor_version: value.assessor_version,
        model_id: value.model_id,
        expectation_contract_version: value.expectation_contract_version,
        coverage_contract_version: value.coverage_contract_version,
        coverage_scope: value.coverage_scope,
        coverage_limitations: value.coverage_limitations,
        status: manuscript_citation_expectation_run_status_dto(value.status),
        item_count: value.item_count,
        failed_item_count: value.failed_item_count,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
    }
}

fn manuscript_citation_expectation_item_dto(
    value: ManuscriptCitationExpectationItem,
) -> ManuscriptCitationExpectationItemDto {
    ManuscriptCitationExpectationItemDto {
        expectation_item_id: value.expectation_item_id,
        expectation_run_id: value.expectation_run_id,
        coverage_item_id: value.coverage_item_id,
        inventory_item_id: value.inventory_item_id,
        ordinal: value.ordinal,
        claim_text: value.claim_text,
        source_excerpt: value.source_excerpt,
        review_kind: claim_review_kind_dto(value.review_kind),
        block_kind: manuscript_claim_inventory_block_kind_dto(value.block_kind),
        assessment_status: citation_expectation_assessment_status_dto(value.assessment_status),
        expectation: value.expectation.map(citation_expectation_dto),
        attention: coverage_attention_state_dto(value.attention),
        attention_reasons: value
            .attention_reasons
            .into_iter()
            .map(coverage_attention_reason_dto)
            .collect(),
        rationale: value.rationale,
        failure_code: value.failure_code,
    }
}

fn citation_expectation_dto(value: CitationExpectation) -> CitationExpectationDto {
    match value {
        CitationExpectation::ExternalEvidenceExpected => {
            CitationExpectationDto::ExternalEvidenceExpected
        }
        CitationExpectation::ExternalEvidenceContextDependent => {
            CitationExpectationDto::ExternalEvidenceContextDependent
        }
        CitationExpectation::ManuscriptInternalSupport => {
            CitationExpectationDto::ManuscriptInternalSupport
        }
        CitationExpectation::NoExternalCitationExpected => {
            CitationExpectationDto::NoExternalCitationExpected
        }
        CitationExpectation::Uncertain => CitationExpectationDto::Uncertain,
    }
}

fn citation_expectation_assessment_status_dto(
    value: CitationExpectationAssessmentStatus,
) -> CitationExpectationAssessmentStatusDto {
    match value {
        CitationExpectationAssessmentStatus::Assessed => {
            CitationExpectationAssessmentStatusDto::Assessed
        }
        CitationExpectationAssessmentStatus::AssessmentFailed => {
            CitationExpectationAssessmentStatusDto::AssessmentFailed
        }
    }
}

fn coverage_attention_state_dto(value: CoverageAttentionState) -> CoverageAttentionStateDto {
    match value {
        CoverageAttentionState::NoCoverageAttentionDetected => {
            CoverageAttentionStateDto::NoCoverageAttentionDetected
        }
        CoverageAttentionState::ReviewSuggested => CoverageAttentionStateDto::ReviewSuggested,
        CoverageAttentionState::ExpectationReviewNeeded => {
            CoverageAttentionStateDto::ExpectationReviewNeeded
        }
        CoverageAttentionState::AssessmentUnavailable => {
            CoverageAttentionStateDto::AssessmentUnavailable
        }
    }
}

fn coverage_attention_reason_dto(value: CoverageAttentionReason) -> CoverageAttentionReasonDto {
    match value {
        CoverageAttentionReason::ExpectedExternalEvidenceNoExactCitationLink => {
            CoverageAttentionReasonDto::ExpectedExternalEvidenceNoExactCitationLink
        }
        CoverageAttentionReason::AmbiguousClaimCitationBridge => {
            CoverageAttentionReasonDto::AmbiguousClaimCitationBridge
        }
        CoverageAttentionReason::CitationVerificationBlocked => {
            CoverageAttentionReasonDto::CitationVerificationBlocked
        }
        CoverageAttentionReason::CitationVerificationIncomplete => {
            CoverageAttentionReasonDto::CitationVerificationIncomplete
        }
        CoverageAttentionReason::CitationVerificationInsufficient => {
            CoverageAttentionReasonDto::CitationVerificationInsufficient
        }
        CoverageAttentionReason::CitationVerificationContextualizes => {
            CoverageAttentionReasonDto::CitationVerificationContextualizes
        }
        CoverageAttentionReason::ExpectedExternalEvidenceNoSupportingVerification => {
            CoverageAttentionReasonDto::ExpectedExternalEvidenceNoSupportingVerification
        }
        CoverageAttentionReason::ContradictoryEvidenceObserved => {
            CoverageAttentionReasonDto::ContradictoryEvidenceObserved
        }
        CoverageAttentionReason::MixedEvidenceRelations => {
            CoverageAttentionReasonDto::MixedEvidenceRelations
        }
        CoverageAttentionReason::ExpectationContextDependent => {
            CoverageAttentionReasonDto::ExpectationContextDependent
        }
        CoverageAttentionReason::ExpectationUncertain => {
            CoverageAttentionReasonDto::ExpectationUncertain
        }
        CoverageAttentionReason::ExpectationAssessmentFailed => {
            CoverageAttentionReasonDto::ExpectationAssessmentFailed
        }
    }
}

fn manuscript_citation_expectation_run_status_dto(
    value: ManuscriptCitationExpectationRunStatus,
) -> ManuscriptCitationExpectationRunStatusDto {
    match value {
        ManuscriptCitationExpectationRunStatus::Running => {
            ManuscriptCitationExpectationRunStatusDto::Running
        }
        ManuscriptCitationExpectationRunStatus::Completed => {
            ManuscriptCitationExpectationRunStatusDto::Completed
        }
        ManuscriptCitationExpectationRunStatus::Failed => {
            ManuscriptCitationExpectationRunStatusDto::Failed
        }
    }
}

fn claim_review_kind_dto(
    value: nineprofs_research::ClaimReviewKind,
) -> nineprofs_api_types::ClaimReviewKindDto {
    match value {
        nineprofs_research::ClaimReviewKind::ExternalEvidence => {
            nineprofs_api_types::ClaimReviewKindDto::ExternalEvidence
        }
        nineprofs_research::ClaimReviewKind::ManuscriptInternal => {
            nineprofs_api_types::ClaimReviewKindDto::ManuscriptInternal
        }
        nineprofs_research::ClaimReviewKind::Interpretive => {
            nineprofs_api_types::ClaimReviewKindDto::Interpretive
        }
        nineprofs_research::ClaimReviewKind::NonEvidentiary => {
            nineprofs_api_types::ClaimReviewKindDto::NonEvidentiary
        }
        nineprofs_research::ClaimReviewKind::Uncertain => {
            nineprofs_api_types::ClaimReviewKindDto::Uncertain
        }
    }
}

fn manuscript_claim_inventory_block_kind_dto(
    value: nineprofs_research::ManuscriptClaimInventoryBlockKind,
) -> nineprofs_api_types::ManuscriptClaimInventoryBlockKindDto {
    match value {
        nineprofs_research::ManuscriptClaimInventoryBlockKind::Paragraph => {
            nineprofs_api_types::ManuscriptClaimInventoryBlockKindDto::Paragraph
        }
        nineprofs_research::ManuscriptClaimInventoryBlockKind::Heading => {
            nineprofs_api_types::ManuscriptClaimInventoryBlockKindDto::Heading
        }
        nineprofs_research::ManuscriptClaimInventoryBlockKind::ListItem => {
            nineprofs_api_types::ManuscriptClaimInventoryBlockKindDto::ListItem
        }
    }
}
