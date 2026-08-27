use super::evidence::{evidence_locator, evidence_locator_dto};
use crate::api::ApiError;
use crate::api::AppState;
use crate::api::proposals::authorize_trusted_decision;
use axum::Router;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::ClaimEvidenceLinkDto;
use nineprofs_api_types::CreateClaimEvidenceLinkRequest;
use nineprofs_api_types::CreateResearchClaimRequest;
use nineprofs_api_types::ResearchAssessmentMethodDto;
use nineprofs_api_types::ResearchClaimDto;
use nineprofs_api_types::ResearchClaimEvidenceRelationDto;
use nineprofs_api_types::ResearchClaimOriginDto;
use nineprofs_research::AssessmentMethod;
use nineprofs_research::ClaimEvidenceRelation;
use nineprofs_research::ClaimOrigin;
use nineprofs_research::CreateClaimEvidenceLink;
use nineprofs_research::CreateResearchClaim;
use nineprofs_research::ResearchClaim;

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchClaimsQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchLinksQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
    #[serde(rename = "claimId")]
    claim_id: Option<String>,
    #[serde(rename = "evidenceId")]
    evidence_id: Option<String>,
}

async fn list_research_claims(
    State(state): State<AppState>,
    Query(query): Query<ResearchClaimsQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchClaimDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_claims(query.research_case_id.as_deref())
            .await?
            .into_iter()
            .map(research_claim_dto)
            .collect(),
    )))
}

async fn get_research_claim(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchClaimDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_claim_dto(
        state.runtime.research_service().get_claim(&id).await?,
    ))))
}

async fn create_research_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateResearchClaimRequest>,
) -> Result<axum::Json<ApiResponse<ResearchClaimDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let claim = state
        .runtime
        .research_service()
        .create_claim(CreateResearchClaim {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            text: request.text,
            origin: claim_origin(request.origin),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_claim_dto(claim))))
}

async fn list_claim_evidence_links(
    State(state): State<AppState>,
    Query(query): Query<ResearchLinksQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ClaimEvidenceLinkDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_links(
                query.research_case_id.as_deref(),
                query.claim_id.as_deref(),
                query.evidence_id.as_deref(),
            )
            .await?
            .into_iter()
            .map(claim_evidence_link_dto)
            .collect(),
    )))
}

async fn get_claim_evidence_link(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ClaimEvidenceLinkDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(claim_evidence_link_dto(
        state.runtime.research_service().get_link(&id).await?,
    ))))
}

async fn create_claim_evidence_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateClaimEvidenceLinkRequest>,
) -> Result<axum::Json<ApiResponse<ClaimEvidenceLinkDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let link = state
        .runtime
        .research_service()
        .create_link(CreateClaimEvidenceLink {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            claim_id: nineprofs_research::ResearchClaimId::parse(request.claim_id)?,
            evidence_id: nineprofs_research::ResearchEvidenceId::parse(request.evidence_id)?,
            relation: claim_evidence_relation(request.relation),
            rationale: request.rationale,
            assessment_method: assessment_method(request.assessment_method),
            assessment_metadata: request.assessment_metadata,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(claim_evidence_link_dto(link))))
}

pub(crate) fn research_claim_dto(value: ResearchClaim) -> ResearchClaimDto {
    ResearchClaimDto {
        claim_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        text: value.text,
        origin: claim_origin_dto(value.origin),
        created_at_ms: value.created_at_ms,
    }
}

pub(crate) fn claim_evidence_link_dto(
    value: nineprofs_research::ClaimEvidenceLink,
) -> ClaimEvidenceLinkDto {
    ClaimEvidenceLinkDto {
        link_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        claim_id: value.claim_id.to_string(),
        evidence_id: value.evidence_id.to_string(),
        relation: claim_evidence_relation_dto(value.relation),
        rationale: value.rationale,
        assessment_method: assessment_method_dto(value.assessment_method),
        assessment_metadata: value.assessment_metadata,
        created_at_ms: value.created_at_ms,
    }
}

pub(crate) fn claim_origin(value: ResearchClaimOriginDto) -> ClaimOrigin {
    match value {
        ResearchClaimOriginDto::Manuscript {
            document_id,
            document_version,
            locator,
        } => ClaimOrigin::Manuscript {
            document_id,
            document_version,
            locator: locator.map(evidence_locator),
        },
        ResearchClaimOriginDto::User => ClaimOrigin::User,
        ResearchClaimOriginDto::Agent => ClaimOrigin::Agent,
        ResearchClaimOriginDto::Imported { source } => ClaimOrigin::Imported { source },
    }
}

pub(crate) fn claim_origin_dto(value: ClaimOrigin) -> ResearchClaimOriginDto {
    match value {
        ClaimOrigin::Manuscript {
            document_id,
            document_version,
            locator,
        } => ResearchClaimOriginDto::Manuscript {
            document_id,
            document_version,
            locator: locator.map(evidence_locator_dto),
        },
        ClaimOrigin::User => ResearchClaimOriginDto::User,
        ClaimOrigin::Agent => ResearchClaimOriginDto::Agent,
        ClaimOrigin::Imported { source } => ResearchClaimOriginDto::Imported { source },
    }
}

pub(crate) fn claim_evidence_relation(
    value: ResearchClaimEvidenceRelationDto,
) -> ClaimEvidenceRelation {
    match value {
        ResearchClaimEvidenceRelationDto::Supports => ClaimEvidenceRelation::Supports,
        ResearchClaimEvidenceRelationDto::Contradicts => ClaimEvidenceRelation::Contradicts,
        ResearchClaimEvidenceRelationDto::Contextualizes => ClaimEvidenceRelation::Contextualizes,
        ResearchClaimEvidenceRelationDto::Insufficient => ClaimEvidenceRelation::Insufficient,
    }
}

pub(crate) fn claim_evidence_relation_dto(
    value: ClaimEvidenceRelation,
) -> ResearchClaimEvidenceRelationDto {
    match value {
        ClaimEvidenceRelation::Supports => ResearchClaimEvidenceRelationDto::Supports,
        ClaimEvidenceRelation::Contradicts => ResearchClaimEvidenceRelationDto::Contradicts,
        ClaimEvidenceRelation::Contextualizes => ResearchClaimEvidenceRelationDto::Contextualizes,
        ClaimEvidenceRelation::Insufficient => ResearchClaimEvidenceRelationDto::Insufficient,
    }
}

pub(crate) fn assessment_method(value: ResearchAssessmentMethodDto) -> AssessmentMethod {
    match value {
        ResearchAssessmentMethodDto::Human => AssessmentMethod::Human,
        ResearchAssessmentMethodDto::DeterministicChecker => AssessmentMethod::DeterministicChecker,
        ResearchAssessmentMethodDto::Agent => AssessmentMethod::Agent,
        ResearchAssessmentMethodDto::ExternalService => AssessmentMethod::ExternalService,
    }
}

pub(crate) fn assessment_method_dto(value: AssessmentMethod) -> ResearchAssessmentMethodDto {
    match value {
        AssessmentMethod::Human => ResearchAssessmentMethodDto::Human,
        AssessmentMethod::DeterministicChecker => ResearchAssessmentMethodDto::DeterministicChecker,
        AssessmentMethod::Agent => ResearchAssessmentMethodDto::Agent,
        AssessmentMethod::ExternalService => ResearchAssessmentMethodDto::ExternalService,
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/claims",
            get(list_research_claims).post(create_research_claim),
        )
        .route("/api/research/claims/{id}", get(get_research_claim))
        .route(
            "/api/research/claim-evidence",
            get(list_claim_evidence_links).post(create_claim_evidence_link),
        )
        .route(
            "/api/research/claim-evidence/{id}",
            get(get_claim_evidence_link),
        )
}
