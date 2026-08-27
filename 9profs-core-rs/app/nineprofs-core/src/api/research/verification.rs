use super::claims::claim_evidence_relation_dto;
use crate::api::ApiError;
use crate::api::AppState;
use crate::api::proposals::authorize_trusted_decision;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::routing::post;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::CitationVerificationCandidateDto;
use nineprofs_api_types::CitationVerificationEvidenceDto;
use nineprofs_api_types::CitationVerificationResultDto;
use nineprofs_api_types::CitationVerificationRunDto;
use nineprofs_api_types::CitationVerificationStatusDto;
use nineprofs_api_types::CreateCitationVerificationRequest;
use nineprofs_research_verification::CitationVerificationRun;
use nineprofs_research_verification::CreateCitationVerification;

async fn create_citation_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateCitationVerificationRequest>,
) -> Result<axum::Json<ApiResponse<CitationVerificationRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .citation_verification_service()
        .verify(CreateCitationVerification {
            claim_citation_link_id: request.claim_citation_link_id,
            citation_target_binding_id: request.citation_target_binding_id,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_verification_run_dto(
        run,
    ))))
}

async fn get_citation_verification(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationVerificationRunDto>>, ApiError> {
    let run = state
        .runtime
        .citation_verification_service()
        .citation_verification(&id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_verification_run_dto(
        run,
    ))))
}

async fn list_claim_citation_verifications(
    State(state): State<AppState>,
    Path(claim_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<CitationVerificationRunDto>>>, ApiError> {
    let runs = state
        .runtime
        .citation_verification_service()
        .claim_citation_verifications(&claim_id)
        .await?
        .into_iter()
        .map(citation_verification_run_dto)
        .collect();
    Ok(axum::Json(ApiResponse::ok(runs)))
}

pub(crate) fn citation_verification_run_dto(
    value: CitationVerificationRun,
) -> CitationVerificationRunDto {
    CitationVerificationRunDto {
        run_id: value.run_id,
        research_case_id: value.research_case_id,
        claim_citation_link_id: value.claim_citation_link_id,
        citation_target_binding_id: value.citation_target_binding_id,
        claim_id: value.claim_id,
        citation_occurrence_id: value.citation_occurrence_id,
        citation_target_id: value.citation_target_id,
        source_id: value.source_id,
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        status: citation_verification_status_dto(value.status),
        failure_code: value.failure_code,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
        result: value.result.map(citation_verification_result_dto),
        candidates: value
            .candidates
            .into_iter()
            .map(citation_verification_candidate_dto)
            .collect(),
        evidence: value
            .evidence
            .into_iter()
            .map(citation_verification_evidence_dto)
            .collect(),
    }
}

pub(crate) fn citation_verification_candidate_dto(
    value: nineprofs_research_verification::CitationVerificationCandidate,
) -> CitationVerificationCandidateDto {
    CitationVerificationCandidateDto {
        verification_run_id: value.verification_run_id,
        retrieval_chunk_id: value.retrieval_chunk_id,
        research_source_id: value.research_source_id,
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        page: value.page,
        start: value.start,
        end: value.end,
        excerpt_hash: value.excerpt_hash,
        rank: value.rank,
        retrieval_score: value.retrieval_score,
    }
}

pub(crate) fn citation_verification_result_dto(
    value: nineprofs_research_verification::CitationVerificationResult,
) -> CitationVerificationResultDto {
    CitationVerificationResultDto {
        verification_run_id: value.verification_run_id,
        overall_relation: claim_evidence_relation_dto(value.overall_relation),
        rationale: value.rationale,
        assessor_provider: value.assessor_provider,
        assessor_version: value.assessor_version,
        assessor_model_id: value.assessor_model_id,
        assessment_contract_version: value.assessment_contract_version,
        completed_at_ms: value.completed_at_ms,
    }
}

pub(crate) fn citation_verification_evidence_dto(
    value: nineprofs_research_verification::CitationVerificationEvidence,
) -> CitationVerificationEvidenceDto {
    CitationVerificationEvidenceDto {
        verification_run_id: value.verification_run_id,
        retrieval_chunk_id: value.retrieval_chunk_id,
        evidence_id: value.evidence_id,
        claim_evidence_link_id: value.claim_evidence_link_id,
        relation: claim_evidence_relation_dto(value.relation),
    }
}

pub(crate) fn citation_verification_status_dto(
    value: nineprofs_research_verification::CitationVerificationStatus,
) -> CitationVerificationStatusDto {
    match value {
        nineprofs_research_verification::CitationVerificationStatus::Running => {
            CitationVerificationStatusDto::Running
        }
        nineprofs_research_verification::CitationVerificationStatus::Completed => {
            CitationVerificationStatusDto::Completed
        }
        nineprofs_research_verification::CitationVerificationStatus::Failed => {
            CitationVerificationStatusDto::Failed
        }
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/citation-verifications",
            post(create_citation_verification),
        )
        .route(
            "/api/research/citation-verifications/{id}",
            get(get_citation_verification),
        )
        .route(
            "/api/research/claims/{claim_id}/citation-verifications",
            get(list_claim_citation_verifications),
        )
}
