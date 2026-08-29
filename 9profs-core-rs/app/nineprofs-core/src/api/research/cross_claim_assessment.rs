use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use nineprofs_api_types::{
    ApiResponse, CreateManuscriptCrossClaimAssessmentRequest, CrossClaimAssessmentStatusDto,
    CrossClaimConsistencyAttentionReasonDto, CrossClaimConsistencyAttentionStateDto,
    CrossClaimConsistencyRelationDto, CrossClaimDifferenceDimensionDto,
    ManuscriptCrossClaimAssessmentItemDto, ManuscriptCrossClaimAssessmentRunDto,
    ManuscriptCrossClaimAssessmentRunStatusDto,
};
use nineprofs_research_verification::{
    CrossClaimAssessmentStatus, CrossClaimConsistencyAttentionReason,
    CrossClaimConsistencyAttentionState, CrossClaimConsistencyRelation,
    CrossClaimDifferenceDimension, ManuscriptCrossClaimAssessmentItem,
    ManuscriptCrossClaimAssessmentRun, ManuscriptCrossClaimAssessmentRunStatus,
    StartManuscriptCrossClaimAssessment,
};

use crate::api::proposals::authorize_trusted_decision;
use crate::api::{ApiError, AppState};

async fn start_assessment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(research_case_id): Path<String>,
    axum::Json(request): axum::Json<CreateManuscriptCrossClaimAssessmentRequest>,
) -> Result<axum::Json<ApiResponse<ManuscriptCrossClaimAssessmentRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .citation_review_service()
        .start_manuscript_cross_claim_assessment(StartManuscriptCrossClaimAssessment {
            research_case_id,
            candidate_run_id: request.candidate_run_id,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(run_dto(run))))
}

async fn get_assessment(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptCrossClaimAssessmentRunDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(run_dto(
        state
            .runtime
            .citation_review_service()
            .get_manuscript_cross_claim_assessment(&run_id)
            .await?,
    ))))
}

async fn list_items(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptCrossClaimAssessmentItemDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .citation_review_service()
            .list_manuscript_cross_claim_assessment_items(&run_id)
            .await?
            .into_iter()
            .map(item_dto)
            .collect(),
    )))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases/{research_case_id}/manuscript-cross-claim-assessments",
            post(start_assessment),
        )
        .route(
            "/api/research/manuscript-cross-claim-assessments/{run_id}",
            get(get_assessment),
        )
        .route(
            "/api/research/manuscript-cross-claim-assessments/{run_id}/items",
            get(list_items),
        )
}

fn run_dto(value: ManuscriptCrossClaimAssessmentRun) -> ManuscriptCrossClaimAssessmentRunDto {
    ManuscriptCrossClaimAssessmentRunDto {
        assessment_run_id: value.assessment_run_id,
        research_case_id: value.research_case_id,
        manuscript_source_id: value.manuscript_source_id,
        document_id: value.document_id,
        document_version: value.document_version,
        candidate_run_id: value.candidate_run_id,
        claim_inventory_run_id: value.claim_inventory_run_id,
        provider_id: value.provider_id,
        model_id: value.model_id,
        assessor_implementation_version: value.assessor_implementation_version,
        assessment_contract_version: value.assessment_contract_version,
        candidate_count: value.candidate_count,
        assessed_count: value.assessed_count,
        failed_item_count: value.failed_item_count,
        conflict_count: value.conflict_count,
        compatible_count: value.compatible_count,
        qualification_count: value.qualification_count,
        equivalent_count: value.equivalent_count,
        not_comparable_count: value.not_comparable_count,
        insufficient_context_count: value.insufficient_context_count,
        failed_assessment_count: value.failed_assessment_count,
        status: run_status_dto(value.status),
        failure_code: value.failure_code,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
    }
}

fn item_dto(value: ManuscriptCrossClaimAssessmentItem) -> ManuscriptCrossClaimAssessmentItemDto {
    ManuscriptCrossClaimAssessmentItemDto {
        assessment_item_id: value.assessment_item_id,
        assessment_run_id: value.assessment_run_id,
        candidate_id: value.candidate_id,
        left_inventory_item_id: value.left_inventory_item_id,
        right_inventory_item_id: value.right_inventory_item_id,
        left_ordinal: value.left_ordinal,
        right_ordinal: value.right_ordinal,
        assessment_status: assessment_status_dto(value.assessment_status),
        relation: value.relation.map(relation_dto),
        dimensions: value.dimensions.into_iter().map(dimension_dto).collect(),
        rationale: value.rationale,
        failure_code: value.failure_code,
        attention: attention_state_dto(value.attention),
        attention_reasons: value
            .attention_reasons
            .into_iter()
            .map(attention_reason_dto)
            .collect(),
    }
}

fn run_status_dto(
    value: ManuscriptCrossClaimAssessmentRunStatus,
) -> ManuscriptCrossClaimAssessmentRunStatusDto {
    match value {
        ManuscriptCrossClaimAssessmentRunStatus::Running => {
            ManuscriptCrossClaimAssessmentRunStatusDto::Running
        }
        ManuscriptCrossClaimAssessmentRunStatus::Completed => {
            ManuscriptCrossClaimAssessmentRunStatusDto::Completed
        }
        ManuscriptCrossClaimAssessmentRunStatus::Failed => {
            ManuscriptCrossClaimAssessmentRunStatusDto::Failed
        }
    }
}

fn assessment_status_dto(value: CrossClaimAssessmentStatus) -> CrossClaimAssessmentStatusDto {
    match value {
        CrossClaimAssessmentStatus::Assessed => CrossClaimAssessmentStatusDto::Assessed,
        CrossClaimAssessmentStatus::AssessmentFailed => {
            CrossClaimAssessmentStatusDto::AssessmentFailed
        }
    }
}

fn relation_dto(value: CrossClaimConsistencyRelation) -> CrossClaimConsistencyRelationDto {
    match value {
        CrossClaimConsistencyRelation::Conflict => CrossClaimConsistencyRelationDto::Conflict,
        CrossClaimConsistencyRelation::Compatible => CrossClaimConsistencyRelationDto::Compatible,
        CrossClaimConsistencyRelation::QualificationOrRefinement => {
            CrossClaimConsistencyRelationDto::QualificationOrRefinement
        }
        CrossClaimConsistencyRelation::EquivalentOrRestatement => {
            CrossClaimConsistencyRelationDto::EquivalentOrRestatement
        }
        CrossClaimConsistencyRelation::NotMeaningfullyComparable => {
            CrossClaimConsistencyRelationDto::NotMeaningfullyComparable
        }
        CrossClaimConsistencyRelation::InsufficientContext => {
            CrossClaimConsistencyRelationDto::InsufficientContext
        }
    }
}

fn dimension_dto(value: CrossClaimDifferenceDimension) -> CrossClaimDifferenceDimensionDto {
    match value {
        CrossClaimDifferenceDimension::Proposition => CrossClaimDifferenceDimensionDto::Proposition,
        CrossClaimDifferenceDimension::Quantitative => {
            CrossClaimDifferenceDimensionDto::Quantitative
        }
        CrossClaimDifferenceDimension::Direction => CrossClaimDifferenceDimensionDto::Direction,
        CrossClaimDifferenceDimension::ModalityOrCertainty => {
            CrossClaimDifferenceDimensionDto::ModalityOrCertainty
        }
        CrossClaimDifferenceDimension::CausalStrength => {
            CrossClaimDifferenceDimensionDto::CausalStrength
        }
        CrossClaimDifferenceDimension::ScopeOrPopulation => {
            CrossClaimDifferenceDimensionDto::ScopeOrPopulation
        }
        CrossClaimDifferenceDimension::Temporal => CrossClaimDifferenceDimensionDto::Temporal,
        CrossClaimDifferenceDimension::Definition => CrossClaimDifferenceDimensionDto::Definition,
        CrossClaimDifferenceDimension::Other => CrossClaimDifferenceDimensionDto::Other,
    }
}

fn attention_state_dto(
    value: CrossClaimConsistencyAttentionState,
) -> CrossClaimConsistencyAttentionStateDto {
    match value {
        CrossClaimConsistencyAttentionState::NoInternalConsistencyAttentionDetected => {
            CrossClaimConsistencyAttentionStateDto::NoInternalConsistencyAttentionDetected
        }
        CrossClaimConsistencyAttentionState::ReviewSuggested => {
            CrossClaimConsistencyAttentionStateDto::ReviewSuggested
        }
        CrossClaimConsistencyAttentionState::ContextReviewNeeded => {
            CrossClaimConsistencyAttentionStateDto::ContextReviewNeeded
        }
        CrossClaimConsistencyAttentionState::AssessmentUnavailable => {
            CrossClaimConsistencyAttentionStateDto::AssessmentUnavailable
        }
    }
}

fn attention_reason_dto(
    value: CrossClaimConsistencyAttentionReason,
) -> CrossClaimConsistencyAttentionReasonDto {
    match value {
        CrossClaimConsistencyAttentionReason::AssessedInternalConflict => {
            CrossClaimConsistencyAttentionReasonDto::AssessedInternalConflict
        }
        CrossClaimConsistencyAttentionReason::QuantitativeConflictObserved => {
            CrossClaimConsistencyAttentionReasonDto::QuantitativeConflictObserved
        }
        CrossClaimConsistencyAttentionReason::DirectionConflictObserved => {
            CrossClaimConsistencyAttentionReasonDto::DirectionConflictObserved
        }
        CrossClaimConsistencyAttentionReason::ModalityConflictObserved => {
            CrossClaimConsistencyAttentionReasonDto::ModalityConflictObserved
        }
        CrossClaimConsistencyAttentionReason::CausalStrengthConflictObserved => {
            CrossClaimConsistencyAttentionReasonDto::CausalStrengthConflictObserved
        }
        CrossClaimConsistencyAttentionReason::ScopeConflictObserved => {
            CrossClaimConsistencyAttentionReasonDto::ScopeConflictObserved
        }
        CrossClaimConsistencyAttentionReason::TemporalConflictObserved => {
            CrossClaimConsistencyAttentionReasonDto::TemporalConflictObserved
        }
        CrossClaimConsistencyAttentionReason::DefinitionConflictObserved => {
            CrossClaimConsistencyAttentionReasonDto::DefinitionConflictObserved
        }
        CrossClaimConsistencyAttentionReason::PropositionalConflictObserved => {
            CrossClaimConsistencyAttentionReasonDto::PropositionalConflictObserved
        }
        CrossClaimConsistencyAttentionReason::ConsistencyContextInsufficient => {
            CrossClaimConsistencyAttentionReasonDto::ConsistencyContextInsufficient
        }
        CrossClaimConsistencyAttentionReason::ConsistencyAssessmentFailed => {
            CrossClaimConsistencyAttentionReasonDto::ConsistencyAssessmentFailed
        }
    }
}
