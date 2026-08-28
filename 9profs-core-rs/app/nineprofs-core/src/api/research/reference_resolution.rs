use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use nineprofs_api_types::{
    ApiResponse, ManuscriptReferenceResolutionCandidateDto, ManuscriptReferenceResolutionEntryDto,
    ManuscriptReferenceResolutionMatchKindDto, ManuscriptReferenceResolutionOutcomeDto,
    ManuscriptReferenceResolutionRunDto, ManuscriptReferenceResolutionStatusDto,
    ResearchContentHashDto, ResearchHashAlgorithmDto,
};
use nineprofs_research::{
    ContentHash, HashAlgorithm, ManuscriptReferenceResolutionCandidate,
    ManuscriptReferenceResolutionEntry, ManuscriptReferenceResolutionMatchKind,
    ManuscriptReferenceResolutionOutcome, ManuscriptReferenceResolutionRun,
    ManuscriptReferenceResolutionStatus,
};

use crate::api::proposals::authorize_trusted_decision;
use crate::api::research::citations::citation_target_binding_dto;
use crate::api::{ApiError, AppState};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/manuscript-reference-catalog-runs/{catalog_run_id}/resolution",
            post(resolve_manuscript_references),
        )
        .route(
            "/api/research/manuscript-reference-resolution-runs/{id}",
            get(get_resolution_run),
        )
        .route(
            "/api/research/manuscript-reference-resolution-runs/{id}/entries",
            get(list_resolution_entries),
        )
        .route(
            "/api/research/manuscript-reference-resolution-entries/{id}/candidates",
            get(list_resolution_candidates),
        )
        .route(
            "/api/research/manuscript-reference-resolution-runs/{run_id}/entries/{entry_id}/candidates/{candidate_id}/confirm",
            post(confirm_resolution_candidate),
        )
}

async fn resolve_manuscript_references(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(catalog_run_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceResolutionRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .research_service()
        .resolve_manuscript_references(&catalog_run_id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(resolution_run_dto(run))))
}

async fn get_resolution_run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ManuscriptReferenceResolutionRunDto>>, ApiError> {
    let run = state
        .runtime
        .research_service()
        .get_manuscript_reference_resolution(&id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(resolution_run_dto(run))))
}

async fn list_resolution_entries(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptReferenceResolutionEntryDto>>>, ApiError> {
    let entries = state
        .runtime
        .research_service()
        .list_manuscript_reference_resolution_entries(&id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        entries.into_iter().map(resolution_entry_dto).collect(),
    )))
}

async fn list_resolution_candidates(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ManuscriptReferenceResolutionCandidateDto>>>, ApiError> {
    let candidates = state
        .runtime
        .research_service()
        .list_manuscript_reference_resolution_candidates(&id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        candidates
            .into_iter()
            .map(resolution_candidate_dto)
            .collect(),
    )))
}

async fn confirm_resolution_candidate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((run_id, entry_id, candidate_id)): Path<(String, String, String)>,
) -> Result<axum::Json<ApiResponse<Vec<nineprofs_api_types::CitationTargetBindingDto>>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let bindings = state
        .runtime
        .research_service()
        .confirm_manuscript_reference_candidate(&run_id, &entry_id, &candidate_id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        bindings
            .into_iter()
            .map(citation_target_binding_dto)
            .collect(),
    )))
}

fn resolution_run_dto(
    value: ManuscriptReferenceResolutionRun,
) -> ManuscriptReferenceResolutionRunDto {
    ManuscriptReferenceResolutionRunDto {
        resolution_run_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        catalog_run_id: value.catalog_run_id.to_string(),
        catalog_hash: content_hash_dto(value.catalog_hash),
        source_state_hash: content_hash_dto(value.source_state_hash),
        resolver_policy_version: value.resolver_policy_version,
        status: resolution_status_dto(value.status),
        entry_count: value.entry_count,
        resolved_entry_count: value.resolved_entry_count,
        candidate_entry_count: value.candidate_entry_count,
        unresolved_entry_count: value.unresolved_entry_count,
        conflict_entry_count: value.conflict_entry_count,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
        failure_code: value.failure_code,
    }
}

fn resolution_entry_dto(
    value: ManuscriptReferenceResolutionEntry,
) -> ManuscriptReferenceResolutionEntryDto {
    ManuscriptReferenceResolutionEntryDto {
        resolution_entry_id: value.id.to_string(),
        resolution_run_id: value.resolution_run_id.to_string(),
        reference_entry_id: value.reference_entry_id.to_string(),
        outcome: resolution_outcome_dto(value.outcome),
        match_kind: value.match_kind.map(resolution_match_kind_dto),
        chosen_source_id: value.chosen_source_id.map(|id| id.to_string()),
        chosen_source_snapshot_id: value.chosen_source_snapshot_id.map(|id| id.to_string()),
        chosen_extraction_id: value.chosen_extraction_id.map(|id| id.to_string()),
        automatic_binding_permitted: value.automatic_binding_permitted,
        candidate_count: value.candidate_count,
    }
}

fn resolution_candidate_dto(
    value: ManuscriptReferenceResolutionCandidate,
) -> ManuscriptReferenceResolutionCandidateDto {
    ManuscriptReferenceResolutionCandidateDto {
        candidate_id: value.id.to_string(),
        resolution_entry_id: value.resolution_entry_id.to_string(),
        ordinal: value.ordinal,
        source_id: value.source_id.to_string(),
        source_snapshot_id: value.source_snapshot_id.map(|id| id.to_string()),
        extraction_id: value.extraction_id.map(|id| id.to_string()),
        match_kind: resolution_match_kind_dto(value.match_kind),
        automatic_binding_permitted: value.automatic_binding_permitted,
    }
}

fn content_hash_dto(value: ContentHash) -> ResearchContentHashDto {
    ResearchContentHashDto {
        algorithm: match value.algorithm {
            HashAlgorithm::Sha256 => ResearchHashAlgorithmDto::Sha256,
        },
        value: value.value,
    }
}

fn resolution_status_dto(
    value: ManuscriptReferenceResolutionStatus,
) -> ManuscriptReferenceResolutionStatusDto {
    match value {
        ManuscriptReferenceResolutionStatus::Running => {
            ManuscriptReferenceResolutionStatusDto::Running
        }
        ManuscriptReferenceResolutionStatus::Completed => {
            ManuscriptReferenceResolutionStatusDto::Completed
        }
        ManuscriptReferenceResolutionStatus::Failed => {
            ManuscriptReferenceResolutionStatusDto::Failed
        }
    }
}

fn resolution_outcome_dto(
    value: ManuscriptReferenceResolutionOutcome,
) -> ManuscriptReferenceResolutionOutcomeDto {
    match value {
        ManuscriptReferenceResolutionOutcome::ResolvedExact => {
            ManuscriptReferenceResolutionOutcomeDto::ResolvedExact
        }
        ManuscriptReferenceResolutionOutcome::AlreadyBound => {
            ManuscriptReferenceResolutionOutcomeDto::AlreadyBound
        }
        ManuscriptReferenceResolutionOutcome::AmbiguousSource => {
            ManuscriptReferenceResolutionOutcomeDto::AmbiguousSource
        }
        ManuscriptReferenceResolutionOutcome::AmbiguousSnapshotOrExtraction => {
            ManuscriptReferenceResolutionOutcomeDto::AmbiguousSnapshotOrExtraction
        }
        ManuscriptReferenceResolutionOutcome::CandidateRequiresConfirmation => {
            ManuscriptReferenceResolutionOutcomeDto::CandidateRequiresConfirmation
        }
        ManuscriptReferenceResolutionOutcome::SourceMatchedButNotVerificationReady => {
            ManuscriptReferenceResolutionOutcomeDto::SourceMatchedButNotVerificationReady
        }
        ManuscriptReferenceResolutionOutcome::Unresolved => {
            ManuscriptReferenceResolutionOutcomeDto::Unresolved
        }
        ManuscriptReferenceResolutionOutcome::ConflictWithExistingBinding => {
            ManuscriptReferenceResolutionOutcomeDto::ConflictWithExistingBinding
        }
        ManuscriptReferenceResolutionOutcome::Failed => {
            ManuscriptReferenceResolutionOutcomeDto::Failed
        }
    }
}

fn resolution_match_kind_dto(
    value: ManuscriptReferenceResolutionMatchKind,
) -> ManuscriptReferenceResolutionMatchKindDto {
    match value {
        ManuscriptReferenceResolutionMatchKind::ExactZoteroItemId => {
            ManuscriptReferenceResolutionMatchKindDto::ExactZoteroItemId
        }
        ManuscriptReferenceResolutionMatchKind::ExactZoteroUri => {
            ManuscriptReferenceResolutionMatchKindDto::ExactZoteroUri
        }
        ManuscriptReferenceResolutionMatchKind::ReferenceKeySourceLabel => {
            ManuscriptReferenceResolutionMatchKindDto::ReferenceKeySourceLabel
        }
        ManuscriptReferenceResolutionMatchKind::ReferenceTitleSourceLabel => {
            ManuscriptReferenceResolutionMatchKindDto::ReferenceTitleSourceLabel
        }
        ManuscriptReferenceResolutionMatchKind::MappingIntegrity => {
            ManuscriptReferenceResolutionMatchKindDto::MappingIntegrity
        }
    }
}
