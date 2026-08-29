use super::claims::claim_evidence_relation_dto;
use super::evidence::evidence_locator_dto;
use super::verification::citation_verification_status_dto;
use crate::api::proposals::authorize_trusted_decision;
use crate::api::{ApiError, AppState};
use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use nineprofs_api_types::{
    ApiResponse, CitationReviewCandidateDto, CitationReviewEvidenceDto, CitationReviewItemDto,
    CitationReviewItemStatusDto, CitationReviewRunDto, CitationReviewRunStatusDto,
    CitationReviewTargetRequest, CitationReviewVerificationDto,
    StartManuscriptCitationReviewRequest,
};
use nineprofs_research::{
    ManuscriptCitationFormat, ManuscriptReferenceCatalogWordSourceInput,
    ManuscriptReferenceCatalogZoteroInput,
};
use nineprofs_research_verification::{
    CitationReviewBlockCitationInput, CitationReviewBlockInput, CitationReviewCandidate,
    CitationReviewCitationInput, CitationReviewEvidence, CitationReviewItem,
    CitationReviewItemStatus, CitationReviewRun, CitationReviewRunStatus,
    CitationReviewTargetInput, CitationReviewVerification, StartManuscriptCitationReview,
};

async fn start_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(research_case_id): Path<String>,
    axum::Json(request): axum::Json<StartManuscriptCitationReviewRequest>,
) -> Result<axum::Json<ApiResponse<CitationReviewRunDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let run = state
        .runtime
        .citation_review_service()
        .start(StartManuscriptCitationReview {
            research_case_id,
            manuscript_source_id: request.manuscript_source_id,
            document_id: request.document_id,
            document_version: request.document_version,
            citations: request
                .citations
                .into_iter()
                .map(|citation| CitationReviewCitationInput {
                    format: manuscript_citation_format(citation.format),
                    rendered_text: citation.rendered_text,
                    block_id: citation.block_id,
                    start: citation.start,
                    end: citation.end,
                    targets: citation
                        .targets
                        .into_iter()
                        .map(citation_review_target_input)
                        .collect(),
                })
                .collect(),
            blocks: request
                .blocks
                .into_iter()
                .map(|block| CitationReviewBlockInput {
                    block_id: block.block_id,
                    text: block.text,
                    citations: block
                        .citations
                        .into_iter()
                        .map(|citation| CitationReviewBlockCitationInput {
                            start: citation.start,
                            end: citation.end,
                            rendered_text: citation.rendered_text,
                        })
                        .collect(),
                })
                .collect(),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_review_run_dto(run))))
}

async fn get_review(
    State(state): State<AppState>,
    Path(review_id): Path<String>,
) -> Result<axum::Json<ApiResponse<CitationReviewRunDto>>, ApiError> {
    let run = state
        .runtime
        .citation_review_service()
        .citation_review(&review_id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(citation_review_run_dto(run))))
}

async fn get_review_items(
    State(state): State<AppState>,
    Path(review_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<CitationReviewItemDto>>>, ApiError> {
    let items = state
        .runtime
        .citation_review_service()
        .citation_review_items(&review_id)
        .await?;
    Ok(axum::Json(ApiResponse::ok(
        items.into_iter().map(citation_review_item_dto).collect(),
    )))
}

fn manuscript_citation_format(
    value: nineprofs_api_types::ManuscriptCitationFormatDto,
) -> ManuscriptCitationFormat {
    match value {
        nineprofs_api_types::ManuscriptCitationFormatDto::WordNative => {
            ManuscriptCitationFormat::WordNative
        }
        nineprofs_api_types::ManuscriptCitationFormatDto::Zotero => {
            ManuscriptCitationFormat::Zotero
        }
    }
}

fn citation_review_target_input(value: CitationReviewTargetRequest) -> CitationReviewTargetInput {
    CitationReviewTargetInput {
        ordinal: value.ordinal,
        reference_key: value.reference_key,
        cited_locator: value.cited_locator,
        word_source: value
            .word_source
            .map(|source| ManuscriptReferenceCatalogWordSourceInput {
                tag: source.tag,
                title: source.title,
                author: source.author,
                year: source.year,
            }),
        zotero: value
            .zotero
            .map(|source| ManuscriptReferenceCatalogZoteroInput {
                item_id: source.item_id,
                uris: source.uris,
            }),
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases/{research_case_id}/manuscript-citation-reviews",
            post(start_review),
        )
        .route(
            "/api/research/manuscript-citation-reviews/{review_id}",
            get(get_review),
        )
        .route(
            "/api/research/manuscript-citation-reviews/{review_id}/items",
            get(get_review_items),
        )
}

pub(crate) fn citation_review_run_dto(value: CitationReviewRun) -> CitationReviewRunDto {
    CitationReviewRunDto {
        review_run_id: value.review_run_id,
        research_case_id: value.research_case_id,
        manuscript_source_id: value.manuscript_source_id,
        document_id: value.document_id,
        document_version: value.document_version,
        citation_sync_run_id: value.citation_sync_run_id,
        reference_catalog_run_id: value.reference_catalog_run_id,
        reference_resolution_run_id: value.reference_resolution_run_id,
        claim_extraction_run_id: value.claim_extraction_run_id,
        status: citation_review_run_status_dto(value.status),
        failure_stage: value.failure_stage,
        failure_code: value.failure_code,
        created_at_ms: value.created_at_ms,
        completed_at_ms: value.completed_at_ms,
    }
}

pub(crate) fn citation_review_item_dto(value: CitationReviewItem) -> CitationReviewItemDto {
    CitationReviewItemDto {
        item_id: value.item_id,
        review_run_id: value.review_run_id,
        ordinal: value.ordinal,
        claim_id: value.claim_id,
        claim_citation_link_id: value.claim_citation_link_id,
        citation_occurrence_id: value.citation_occurrence_id,
        citation_target_id: value.citation_target_id,
        reference_entry_id: value.reference_entry_id,
        resolution_entry_id: value.resolution_entry_id,
        resolution_outcome: value.resolution_outcome.map(resolution_outcome_dto),
        document_block_id: value.document_block_id,
        start: value.start,
        end: value.end,
        rendered_text: value.rendered_text,
        reference_key: value.reference_key,
        cited_locator: value.cited_locator,
        claim_text: value.claim_text,
        source_excerpt: value.source_excerpt,
        binding_id: value.binding_id,
        binding_method: value
            .binding_method
            .map(super::citations::citation_binding_method_dto),
        source_id: value.source_id,
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        status: citation_review_item_status_dto(value.status),
        failure_code: value.failure_code,
        candidates: value
            .candidates
            .into_iter()
            .map(citation_review_candidate_dto)
            .collect(),
        verification: value.verification.map(citation_review_verification_dto),
        evidence: value
            .evidence
            .into_iter()
            .map(citation_review_evidence_dto)
            .collect(),
    }
}

fn citation_review_candidate_dto(value: CitationReviewCandidate) -> CitationReviewCandidateDto {
    CitationReviewCandidateDto {
        candidate_id: value.candidate_id,
        resolution_entry_id: value.resolution_entry_id,
        ordinal: value.ordinal,
        source_id: value.source_id,
        source_label: value.source_label,
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        match_kind: value.match_kind.map(resolution_match_kind_dto),
        automatic_binding_permitted: value.automatic_binding_permitted,
    }
}

pub(crate) fn citation_review_verification_dto(
    value: CitationReviewVerification,
) -> CitationReviewVerificationDto {
    CitationReviewVerificationDto {
        verification_run_id: value.verification_run_id,
        status: citation_verification_status_dto(value.status),
        failure_code: value.failure_code,
        relation: value.relation.map(claim_evidence_relation_dto),
        rationale: value.rationale,
        assessor_provider: value.assessor_provider,
        assessor_version: value.assessor_version,
        assessor_model_id: value.assessor_model_id,
        completed_at_ms: value.completed_at_ms,
    }
}

pub(crate) fn citation_review_evidence_dto(
    value: CitationReviewEvidence,
) -> CitationReviewEvidenceDto {
    CitationReviewEvidenceDto {
        evidence_id: value.evidence_id,
        relation: claim_evidence_relation_dto(value.relation),
        source_snapshot_id: value.source_snapshot_id,
        extraction_id: value.extraction_id,
        locator: evidence_locator_dto(value.locator),
        verbatim_excerpt: value.verbatim_excerpt,
    }
}

fn citation_review_run_status_dto(value: CitationReviewRunStatus) -> CitationReviewRunStatusDto {
    match value {
        CitationReviewRunStatus::Running => CitationReviewRunStatusDto::Running,
        CitationReviewRunStatus::Completed => CitationReviewRunStatusDto::Completed,
        CitationReviewRunStatus::Failed => CitationReviewRunStatusDto::Failed,
    }
}

pub(crate) fn citation_review_item_status_dto(
    value: CitationReviewItemStatus,
) -> CitationReviewItemStatusDto {
    match value {
        CitationReviewItemStatus::UnresolvedReference => {
            CitationReviewItemStatusDto::UnresolvedReference
        }
        CitationReviewItemStatus::AmbiguousReference => {
            CitationReviewItemStatusDto::AmbiguousReference
        }
        CitationReviewItemStatus::ReferenceRequiresConfirmation => {
            CitationReviewItemStatusDto::ReferenceRequiresConfirmation
        }
        CitationReviewItemStatus::SourceMatchedNotVerificationReady => {
            CitationReviewItemStatusDto::SourceMatchedNotVerificationReady
        }
        CitationReviewItemStatus::BindingConflict => CitationReviewItemStatusDto::BindingConflict,
        CitationReviewItemStatus::ReadyForVerification => {
            CitationReviewItemStatusDto::ReadyForVerification
        }
        CitationReviewItemStatus::VerificationRunning => {
            CitationReviewItemStatusDto::VerificationRunning
        }
        CitationReviewItemStatus::VerificationCompleted => {
            CitationReviewItemStatusDto::VerificationCompleted
        }
        CitationReviewItemStatus::VerificationFailed => {
            CitationReviewItemStatusDto::VerificationFailed
        }
        CitationReviewItemStatus::ResolutionFailed => CitationReviewItemStatusDto::ResolutionFailed,
    }
}

fn resolution_outcome_dto(
    value: nineprofs_research::ManuscriptReferenceResolutionOutcome,
) -> nineprofs_api_types::ManuscriptReferenceResolutionOutcomeDto {
    use nineprofs_api_types::ManuscriptReferenceResolutionOutcomeDto as D;
    match value {
        nineprofs_research::ManuscriptReferenceResolutionOutcome::ResolvedExact => D::ResolvedExact,
        nineprofs_research::ManuscriptReferenceResolutionOutcome::AlreadyBound => D::AlreadyBound,
        nineprofs_research::ManuscriptReferenceResolutionOutcome::AmbiguousSource => D::AmbiguousSource,
        nineprofs_research::ManuscriptReferenceResolutionOutcome::AmbiguousSnapshotOrExtraction => D::AmbiguousSnapshotOrExtraction,
        nineprofs_research::ManuscriptReferenceResolutionOutcome::CandidateRequiresConfirmation => D::CandidateRequiresConfirmation,
        nineprofs_research::ManuscriptReferenceResolutionOutcome::SourceMatchedButNotVerificationReady => D::SourceMatchedButNotVerificationReady,
        nineprofs_research::ManuscriptReferenceResolutionOutcome::Unresolved => D::Unresolved,
        nineprofs_research::ManuscriptReferenceResolutionOutcome::ConflictWithExistingBinding => D::ConflictWithExistingBinding,
        nineprofs_research::ManuscriptReferenceResolutionOutcome::Failed => D::Failed,
    }
}

fn resolution_match_kind_dto(
    value: nineprofs_research::ManuscriptReferenceResolutionMatchKind,
) -> nineprofs_api_types::ManuscriptReferenceResolutionMatchKindDto {
    use nineprofs_api_types::ManuscriptReferenceResolutionMatchKindDto as D;
    match value {
        nineprofs_research::ManuscriptReferenceResolutionMatchKind::ExactZoteroItemId => {
            D::ExactZoteroItemId
        }
        nineprofs_research::ManuscriptReferenceResolutionMatchKind::ExactZoteroUri => {
            D::ExactZoteroUri
        }
        nineprofs_research::ManuscriptReferenceResolutionMatchKind::ReferenceKeySourceLabel => {
            D::ReferenceKeySourceLabel
        }
        nineprofs_research::ManuscriptReferenceResolutionMatchKind::ReferenceTitleSourceLabel => {
            D::ReferenceTitleSourceLabel
        }
        nineprofs_research::ManuscriptReferenceResolutionMatchKind::MappingIntegrity => {
            D::MappingIntegrity
        }
    }
}
