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
use nineprofs_api_types::CitationOccurrenceDto;
use nineprofs_api_types::CitationTargetBindingDto;
use nineprofs_api_types::CitationTargetDto;
use nineprofs_api_types::ClaimCitationLinkDto;
use nineprofs_api_types::CreateCitationOccurrenceRequest;
use nineprofs_api_types::CreateCitationTargetBindingRequest;
use nineprofs_api_types::CreateCitationTargetRequest;
use nineprofs_api_types::CreateClaimCitationLinkRequest;
use nineprofs_api_types::ResearchCitationBindingMethodDto;
use nineprofs_api_types::ResearchCitationOccurrenceOriginDto;
use nineprofs_api_types::ResearchCitationTargetResolutionDto;
use nineprofs_research::CitationBindingMethod;
use nineprofs_research::CitationOccurrenceOrigin;
use nineprofs_research::CreateCitationOccurrence;
use nineprofs_research::CreateCitationTarget;
use nineprofs_research::CreateCitationTargetBinding;
use nineprofs_research::CreateClaimCitationLink;
use nineprofs_research::ResearchError;

#[derive(Debug, Default, serde::Deserialize)]
struct CitationOccurrencesQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ClaimCitationsQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
    #[serde(rename = "claimId")]
    claim_id: Option<String>,
    #[serde(rename = "citationOccurrenceId")]
    citation_occurrence_id: Option<String>,
}

async fn list_citation_occurrences(
    State(state): State<AppState>,
    Query(query): Query<CitationOccurrencesQuery>,
) -> Result<axum::Json<ApiResponse<Vec<CitationOccurrenceDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_citation_occurrences(query.research_case_id.as_deref())
            .await?
            .into_iter()
            .map(citation_occurrence_dto)
            .collect(),
    )))
}

async fn get_citation_occurrence(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationOccurrenceDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(citation_occurrence_dto(
        state
            .runtime
            .research_service()
            .get_citation_occurrence(&id)
            .await?,
    ))))
}

async fn create_citation_occurrence(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateCitationOccurrenceRequest>,
) -> Result<axum::Json<ApiResponse<CitationOccurrenceDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let occurrence = state
        .runtime
        .research_service()
        .create_citation_occurrence(CreateCitationOccurrence {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            origin: citation_occurrence_origin(request.origin)?,
            rendered_text: request.rendered_text,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_occurrence_dto(
        occurrence,
    ))))
}

async fn list_citation_targets(
    State(state): State<AppState>,
    Path(occurrence_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<CitationTargetDto>>>, ApiError> {
    let service = state.runtime.research_service();
    let mut targets = Vec::new();
    for target in service.list_citation_targets(&occurrence_id).await? {
        let resolution = service
            .citation_target_resolution(target.id.as_str())
            .await?;
        targets.push(citation_target_dto(target, resolution));
    }
    Ok(axum::Json(ApiResponse::ok(targets)))
}

async fn create_citation_target(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(occurrence_id): Path<String>,
    axum::Json(request): axum::Json<CreateCitationTargetRequest>,
) -> Result<axum::Json<ApiResponse<CitationTargetDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let route_occurrence_id = nineprofs_research::CitationOccurrenceId::parse(occurrence_id)?;
    let request_occurrence_id =
        nineprofs_research::CitationOccurrenceId::parse(request.citation_occurrence_id)?;
    if route_occurrence_id != request_occurrence_id {
        return Err(ResearchError::Invalid(
            "citation target occurrence does not match route".to_owned(),
        )
        .into());
    }
    let target = state
        .runtime
        .research_service()
        .create_citation_target(CreateCitationTarget {
            citation_occurrence_id: route_occurrence_id,
            ordinal: request.ordinal,
            reference_key: request.reference_key,
            cited_locator: request.cited_locator,
        })
        .await?;
    let resolution = state
        .runtime
        .research_service()
        .citation_target_resolution(target.id.as_str())
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_target_dto(
        target, resolution,
    ))))
}

async fn get_citation_target(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationTargetDto>>, ApiError> {
    let service = state.runtime.research_service();
    let target = service.get_citation_target(&id).await?;
    let resolution = service
        .citation_target_resolution(target.id.as_str())
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_target_dto(
        target, resolution,
    ))))
}

async fn list_citation_target_bindings(
    State(state): State<AppState>,
    Path(target_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<CitationTargetBindingDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_citation_target_bindings(&target_id)
            .await?
            .into_iter()
            .map(citation_target_binding_dto)
            .collect(),
    )))
}

async fn create_citation_target_binding(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_id): Path<String>,
    axum::Json(request): axum::Json<CreateCitationTargetBindingRequest>,
) -> Result<axum::Json<ApiResponse<CitationTargetBindingDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let route_target_id = nineprofs_research::CitationTargetId::parse(target_id)?;
    let request_target_id =
        nineprofs_research::CitationTargetId::parse(request.citation_target_id)?;
    if route_target_id != request_target_id {
        return Err(ResearchError::Invalid(
            "citation binding target does not match route".to_owned(),
        )
        .into());
    }
    let binding = state
        .runtime
        .research_service()
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            citation_target_id: route_target_id,
            source_id: nineprofs_research::ResearchSourceId::parse(request.source_id)?,
            source_snapshot_id: request
                .source_snapshot_id
                .map(nineprofs_research::ResearchSourceSnapshotId::parse)
                .transpose()?,
            extraction_id: request
                .extraction_id
                .map(nineprofs_research::ResearchPdfExtractionId::parse)
                .transpose()?,
            method: citation_binding_method(request.method),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_target_binding_dto(
        binding,
    ))))
}

async fn get_citation_target_binding(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationTargetBindingDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(citation_target_binding_dto(
        state
            .runtime
            .research_service()
            .get_citation_target_binding(&id)
            .await?,
    ))))
}

async fn get_latest_citation_target_binding(
    State(state): State<AppState>,
    Path(target_id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationTargetBindingDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(citation_target_binding_dto(
        state
            .runtime
            .research_service()
            .latest_citation_target_binding(&target_id)
            .await?,
    ))))
}

async fn list_claim_citation_links(
    State(state): State<AppState>,
    Query(query): Query<ClaimCitationsQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ClaimCitationLinkDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_claim_citation_links(
                query.research_case_id.as_deref(),
                query.claim_id.as_deref(),
                query.citation_occurrence_id.as_deref(),
            )
            .await?
            .into_iter()
            .map(claim_citation_link_dto)
            .collect(),
    )))
}

async fn create_claim_citation_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateClaimCitationLinkRequest>,
) -> Result<axum::Json<ApiResponse<ClaimCitationLinkDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let link = state
        .runtime
        .research_service()
        .create_claim_citation_link(CreateClaimCitationLink {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            claim_id: nineprofs_research::ResearchClaimId::parse(request.claim_id)?,
            citation_occurrence_id: nineprofs_research::CitationOccurrenceId::parse(
                request.citation_occurrence_id,
            )?,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(claim_citation_link_dto(link))))
}

async fn get_claim_citation_link(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ClaimCitationLinkDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(claim_citation_link_dto(
        state
            .runtime
            .research_service()
            .get_claim_citation_link(&id)
            .await?,
    ))))
}

pub(crate) fn citation_occurrence_dto(
    value: nineprofs_research::CitationOccurrence,
) -> CitationOccurrenceDto {
    CitationOccurrenceDto {
        occurrence_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        origin: citation_occurrence_origin_dto(value.origin),
        rendered_text: value.rendered_text,
        created_at_ms: value.created_at_ms,
    }
}

pub(crate) fn citation_target_dto(
    value: nineprofs_research::CitationTarget,
    resolution: nineprofs_research::CitationTargetResolution,
) -> CitationTargetDto {
    CitationTargetDto {
        target_id: value.id.to_string(),
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        ordinal: value.ordinal,
        reference_key: value.reference_key,
        cited_locator: value.cited_locator,
        resolution: citation_target_resolution_dto(resolution),
    }
}

pub(crate) fn citation_target_binding_dto(
    value: nineprofs_research::CitationTargetBinding,
) -> CitationTargetBindingDto {
    let resolution = value.resolution();
    let pdf_verification_ready = value.pdf_verification_ready();
    CitationTargetBindingDto {
        binding_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        citation_target_id: value.citation_target_id.to_string(),
        source_id: value.source_id.to_string(),
        source_snapshot_id: value.source_snapshot_id.map(|id| id.to_string()),
        extraction_id: value.extraction_id.map(|id| id.to_string()),
        method: citation_binding_method_dto(value.method),
        resolution: citation_target_resolution_dto(resolution),
        pdf_verification_ready,
        created_at_ms: value.created_at_ms,
    }
}

pub(crate) fn claim_citation_link_dto(
    value: nineprofs_research::ClaimCitationLink,
) -> ClaimCitationLinkDto {
    ClaimCitationLinkDto {
        link_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        claim_id: value.claim_id.to_string(),
        citation_occurrence_id: value.citation_occurrence_id.to_string(),
        created_at_ms: value.created_at_ms,
    }
}

pub(crate) fn citation_occurrence_origin(
    value: ResearchCitationOccurrenceOriginDto,
) -> Result<CitationOccurrenceOrigin, ApiError> {
    Ok(match value {
        ResearchCitationOccurrenceOriginDto::Manuscript {
            document_id,
            document_version,
            locator,
        } => CitationOccurrenceOrigin::Manuscript {
            document_id,
            document_version,
            locator: locator.map(evidence_locator),
        },
        ResearchCitationOccurrenceOriginDto::ManuscriptSnapshot {
            source_snapshot_id,
            locator,
        } => CitationOccurrenceOrigin::ManuscriptSnapshot {
            source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(
                source_snapshot_id,
            )?,
            locator: locator.map(evidence_locator),
        },
        ResearchCitationOccurrenceOriginDto::Imported { source } => {
            CitationOccurrenceOrigin::Imported { source }
        }
    })
}

pub(crate) fn citation_occurrence_origin_dto(
    value: CitationOccurrenceOrigin,
) -> ResearchCitationOccurrenceOriginDto {
    match value {
        CitationOccurrenceOrigin::Manuscript {
            document_id,
            document_version,
            locator,
        } => ResearchCitationOccurrenceOriginDto::Manuscript {
            document_id,
            document_version,
            locator: locator.map(evidence_locator_dto),
        },
        CitationOccurrenceOrigin::ManuscriptSnapshot {
            source_snapshot_id,
            locator,
        } => ResearchCitationOccurrenceOriginDto::ManuscriptSnapshot {
            source_snapshot_id: source_snapshot_id.to_string(),
            locator: locator.map(evidence_locator_dto),
        },
        CitationOccurrenceOrigin::Imported { source } => {
            ResearchCitationOccurrenceOriginDto::Imported { source }
        }
    }
}

pub(crate) fn citation_binding_method(
    value: ResearchCitationBindingMethodDto,
) -> CitationBindingMethod {
    match value {
        ResearchCitationBindingMethodDto::Human => CitationBindingMethod::Human,
        ResearchCitationBindingMethodDto::Imported => CitationBindingMethod::Imported,
        ResearchCitationBindingMethodDto::DeterministicResolver => {
            CitationBindingMethod::DeterministicResolver
        }
        ResearchCitationBindingMethodDto::Agent => CitationBindingMethod::Agent,
    }
}

pub(crate) fn citation_binding_method_dto(
    value: CitationBindingMethod,
) -> ResearchCitationBindingMethodDto {
    match value {
        CitationBindingMethod::Human => ResearchCitationBindingMethodDto::Human,
        CitationBindingMethod::Imported => ResearchCitationBindingMethodDto::Imported,
        CitationBindingMethod::DeterministicResolver => {
            ResearchCitationBindingMethodDto::DeterministicResolver
        }
        CitationBindingMethod::Agent => ResearchCitationBindingMethodDto::Agent,
    }
}

pub(crate) fn citation_target_resolution_dto(
    value: nineprofs_research::CitationTargetResolution,
) -> ResearchCitationTargetResolutionDto {
    match value {
        nineprofs_research::CitationTargetResolution::Unresolved => {
            ResearchCitationTargetResolutionDto::Unresolved
        }
        nineprofs_research::CitationTargetResolution::SourceBound => {
            ResearchCitationTargetResolutionDto::SourceBound
        }
        nineprofs_research::CitationTargetResolution::PdfExtractionBound => {
            ResearchCitationTargetResolutionDto::PdfExtractionBound
        }
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/citation-occurrences",
            get(list_citation_occurrences).post(create_citation_occurrence),
        )
        .route(
            "/api/research/citation-occurrences/{id}",
            get(get_citation_occurrence),
        )
        .route(
            "/api/research/citation-occurrences/{id}/targets",
            get(list_citation_targets).post(create_citation_target),
        )
        .route(
            "/api/research/citation-targets/{id}",
            get(get_citation_target),
        )
        .route(
            "/api/research/citation-targets/{id}/bindings",
            get(list_citation_target_bindings).post(create_citation_target_binding),
        )
        .route(
            "/api/research/citation-target-bindings/{id}",
            get(get_citation_target_binding),
        )
        .route(
            "/api/research/citation-targets/{id}/latest-binding",
            get(get_latest_citation_target_binding),
        )
        .route(
            "/api/research/claim-citations",
            get(list_claim_citation_links).post(create_claim_citation_link),
        )
        .route(
            "/api/research/claim-citations/{id}",
            get(get_claim_citation_link),
        )
}
