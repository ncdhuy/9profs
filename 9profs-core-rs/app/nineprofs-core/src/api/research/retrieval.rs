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
use nineprofs_api_types::ResearchExtractionRetrievalIndexDto;
use nineprofs_api_types::ResearchRetrievalCandidateDto;
use nineprofs_api_types::ResearchRetrievalIndexDto;
use nineprofs_api_types::ResearchRetrievalIndexStateDto;
use nineprofs_api_types::ResearchRetrievalIndexStatusDto;
use nineprofs_api_types::ResearchRetrievalReadinessDto;
use nineprofs_api_types::ResearchRetrievalReadinessStatusDto;
use nineprofs_api_types::ResearchRetrievalScopeDto;
use nineprofs_api_types::RetrieveResearchRequest;
use nineprofs_research::ResearchPdfExtractionId;
use nineprofs_research::ResearchRetrievalScope;
use nineprofs_research::ResearchSourceId;
use nineprofs_research_dify::DifyCaseIndex;
use nineprofs_research_dify::DifyExtractionIndex;
use nineprofs_research_dify::DifyIndexStatus;
use nineprofs_research_dify::DifyReadiness;
use nineprofs_research_dify::RetrievalCandidate;
use nineprofs_research_dify::RetrievalIndexState;

async fn get_research_retrieval_index(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchRetrievalIndexStateDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        research_retrieval_index_state_dto(state.runtime.dify_service().state(&id).await?),
    )))
}

async fn ensure_research_retrieval_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchRetrievalIndexDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    Ok(axum::Json(ApiResponse::ok(research_retrieval_index_dto(
        state.runtime.dify_service().ensure_case_index(&id).await?,
    ))))
}

async fn sync_research_retrieval_index(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((index_id, extraction_id)): Path<(String, String)>,
) -> Result<axum::Json<ApiResponse<ResearchExtractionRetrievalIndexDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    Ok(axum::Json(ApiResponse::ok(
        research_extraction_retrieval_index_dto(
            state
                .runtime
                .dify_service()
                .sync_extraction(&index_id, &extraction_id)
                .await?,
        ),
    )))
}

async fn retrieve_research_case(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(request): axum::Json<RetrieveResearchRequest>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchRetrievalCandidateDto>>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let scope = research_retrieval_scope(request.scope)?;
    scope
        .validate()
        .map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .dify_service()
            .retrieve_with_scope(&id, &scope, &request.query, request.top_k.unwrap_or(10))
            .await?
            .into_iter()
            .map(research_retrieval_candidate_dto)
            .collect(),
    )))
}

pub(crate) fn research_retrieval_scope(
    value: Option<ResearchRetrievalScopeDto>,
) -> Result<ResearchRetrievalScope, ApiError> {
    let Some(value) = value else {
        return Ok(ResearchRetrievalScope::Case);
    };
    let parse_source_ids = |ids: Vec<String>| {
        ids.into_iter()
            .map(ResearchSourceId::parse)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ApiError::InvalidRequest(error.to_string()))
    };
    let parse_extraction_ids = |ids: Vec<String>| {
        ids.into_iter()
            .map(ResearchPdfExtractionId::parse)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ApiError::InvalidRequest(error.to_string()))
    };
    match value {
        ResearchRetrievalScopeDto::Case => Ok(ResearchRetrievalScope::Case),
        ResearchRetrievalScopeDto::Sources { source_ids } => Ok(ResearchRetrievalScope::Sources {
            source_ids: parse_source_ids(source_ids)?,
        }),
        ResearchRetrievalScopeDto::Extractions { extraction_ids } => {
            Ok(ResearchRetrievalScope::Extractions {
                extraction_ids: parse_extraction_ids(extraction_ids)?,
            })
        }
    }
}

pub(crate) fn research_retrieval_index_state_dto(
    value: RetrievalIndexState,
) -> ResearchRetrievalIndexStateDto {
    ResearchRetrievalIndexStateDto {
        readiness: research_retrieval_readiness_dto(value.readiness),
        case_index: value.case_index.map(research_retrieval_index_dto),
        extraction_indexes: value
            .extraction_indexes
            .into_iter()
            .map(research_extraction_retrieval_index_dto)
            .collect(),
    }
}

pub(crate) fn research_retrieval_readiness_dto(
    value: DifyReadiness,
) -> ResearchRetrievalReadinessDto {
    ResearchRetrievalReadinessDto {
        provider: value.provider.to_owned(),
        qualification_target: value.qualification_target.to_owned(),
        configured: value.configured,
        status: match value.status {
            nineprofs_research_dify::DifyReadinessStatus::NotConfigured => {
                ResearchRetrievalReadinessStatusDto::NotConfigured
            }
            nineprofs_research_dify::DifyReadinessStatus::Configured => {
                ResearchRetrievalReadinessStatusDto::Configured
            }
            nineprofs_research_dify::DifyReadinessStatus::Unreachable => {
                ResearchRetrievalReadinessStatusDto::Unreachable
            }
            nineprofs_research_dify::DifyReadinessStatus::Reachable => {
                ResearchRetrievalReadinessStatusDto::Reachable
            }
            nineprofs_research_dify::DifyReadinessStatus::Unauthorized => {
                ResearchRetrievalReadinessStatusDto::Unauthorized
            }
            nineprofs_research_dify::DifyReadinessStatus::Ready => {
                ResearchRetrievalReadinessStatusDto::Ready
            }
        },
        reachable: value.reachable,
        authorized: value.authorized,
        ready: value.ready,
    }
}

pub(crate) fn research_retrieval_index_dto(value: DifyCaseIndex) -> ResearchRetrievalIndexDto {
    ResearchRetrievalIndexDto {
        index_id: value.index_id,
        research_case_id: value.research_case_id,
        dataset_id: value.dataset_id,
        status: dify_index_status_dto(value.status),
        failure_code: value.failure_code,
        created_at_ms: value.created_at_ms,
        updated_at_ms: value.updated_at_ms,
    }
}

pub(crate) fn research_extraction_retrieval_index_dto(
    value: DifyExtractionIndex,
) -> ResearchExtractionRetrievalIndexDto {
    ResearchExtractionRetrievalIndexDto {
        index_id: value.index_id,
        case_index_id: value.case_index_id,
        research_case_id: value.research_case_id,
        extraction_id: value.extraction_id,
        source_snapshot_id: value.source_snapshot_id,
        document_id: value.document_id,
        metadata_qualified: value.metadata_qualified,
        chunker_version: value.chunker_version,
        status: dify_index_status_dto(value.status),
        failure_code: value.failure_code,
        created_at_ms: value.created_at_ms,
        updated_at_ms: value.updated_at_ms,
    }
}

pub(crate) fn research_retrieval_candidate_dto(
    value: RetrievalCandidate,
) -> ResearchRetrievalCandidateDto {
    ResearchRetrievalCandidateDto {
        retrieval_chunk_id: value.retrieval_chunk_id,
        research_source_id: value.research_source_id,
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        page: value.page,
        start: value.start,
        end: value.end,
        verbatim_excerpt: value.verbatim_excerpt,
        retrieval_score: value.retrieval_score,
        provider: value.provider.to_owned(),
        rank: value.rank,
    }
}

pub(crate) fn dify_index_status_dto(value: DifyIndexStatus) -> ResearchRetrievalIndexStatusDto {
    match value {
        DifyIndexStatus::NotConfigured => ResearchRetrievalIndexStatusDto::NotConfigured,
        DifyIndexStatus::Provisioning => ResearchRetrievalIndexStatusDto::Provisioning,
        DifyIndexStatus::Ready => ResearchRetrievalIndexStatusDto::Ready,
        DifyIndexStatus::Syncing => ResearchRetrievalIndexStatusDto::Syncing,
        DifyIndexStatus::Failed => ResearchRetrievalIndexStatusDto::Failed,
        DifyIndexStatus::Degraded => ResearchRetrievalIndexStatusDto::Degraded,
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases/{id}/retrieval-index",
            get(get_research_retrieval_index),
        )
        .route(
            "/api/research/cases/{id}/retrieval-index/dify",
            post(ensure_research_retrieval_index),
        )
        .route(
            "/api/research/retrieval-indexes/{index_id}/extractions/{extraction_id}/sync",
            post(sync_research_retrieval_index),
        )
        .route(
            "/api/research/cases/{id}/retrieve",
            post(retrieve_research_case),
        )
}
