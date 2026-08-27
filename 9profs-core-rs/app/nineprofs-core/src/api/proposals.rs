use crate::api::ApiError;
use crate::api::AppState;
use axum::Router;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::routing::post;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::DocumentProposalChangeDto;
use nineprofs_api_types::DocumentProposalDto;
use nineprofs_document_tools::DocumentProposalView;
use nineprofs_document_tools::ProposalAvailability;
use nineprofs_document_tools::ProposalFreshness;

#[derive(Debug, Default, serde::Deserialize)]
struct DocumentProposalQuery {
    #[serde(rename = "documentId")]
    document_id: Option<String>,
}

async fn list_document_proposals(
    State(state): State<AppState>,
    Query(query): Query<DocumentProposalQuery>,
) -> axum::Json<ApiResponse<Vec<DocumentProposalDto>>> {
    axum::Json(ApiResponse::ok(
        state
            .runtime
            .document_tools()
            .list_proposals(query.document_id.as_deref())
            .await
            .into_iter()
            .map(document_proposal_dto)
            .collect(),
    ))
}

async fn get_document_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<DocumentProposalDto>>, ApiError> {
    let proposal = state
        .runtime
        .document_tools()
        .get_proposal(&id)
        .await
        .ok_or_else(|| ApiError::DocumentProposalNotFound(id.clone()))?;
    Ok(axum::Json(ApiResponse::ok(document_proposal_dto(proposal))))
}

pub(super) fn document_proposal_dto(proposal: DocumentProposalView) -> DocumentProposalDto {
    DocumentProposalDto {
        proposal_id: proposal.proposal_id,
        change_set_id: proposal.change_set.id,
        document_id: proposal.document_id,
        authority: proposal.change_set.target.kind,
        base_version: proposal.base_version,
        status: proposal.status,
        freshness: match proposal.freshness {
            ProposalFreshness::Fresh => "fresh",
            ProposalFreshness::Stale => "stale",
            ProposalFreshness::Unavailable => "unavailable",
        }
        .to_owned(),
        availability: match proposal.availability {
            ProposalAvailability::Available => "available",
            ProposalAvailability::Unavailable => "unavailable",
        }
        .to_owned(),
        current_version: proposal.current_version,
        created_at_ms: proposal.created_at_ms,
        summary: proposal.summary,
        changes: proposal
            .change_set
            .changes
            .into_iter()
            .map(|change| DocumentProposalChangeDto {
                change_type: change.change_type,
                payload: change.payload,
            })
            .collect(),
        decision: proposal.decision,
        outcome: proposal
            .outcome
            .and_then(|outcome| serde_json::to_value(outcome).ok()),
        failure: proposal.failure,
        retryable: proposal.retryable,
    }
}

pub(crate) const TRUSTED_DECISION_HEADER: &str = "x-nineprofs-session-secret";

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedDecisionRequest {
    #[serde(default)]
    note: Option<String>,
}

pub(super) fn authorize_trusted_decision(
    headers: &HeaderMap,
    config: &nineprofs_runtime::RuntimeConfig,
) -> Result<(), ApiError> {
    match config.session_secret.as_deref() {
        Some(expected) => {
            let provided = headers
                .get(TRUSTED_DECISION_HEADER)
                .and_then(|value| value.to_str().ok());
            if !constant_time_secret_eq(expected, provided) {
                return Err(ApiError::Unauthorized);
            }
        }
        None if !config.bind_addr.ip().is_loopback() => return Err(ApiError::Unauthorized),
        None => {}
    }
    Ok(())
}

fn constant_time_secret_eq(expected: &str, provided: Option<&str>) -> bool {
    let Some(provided) = provided else {
        return false;
    };
    let left = expected.as_bytes();
    let right = provided.as_bytes();
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(left.get(index).copied().unwrap_or_default())
            ^ usize::from(right.get(index).copied().unwrap_or_default());
    }
    difference == 0
}

fn decision_note(
    body: Option<axum::Json<TrustedDecisionRequest>>,
) -> Result<Option<String>, ApiError> {
    let note = body.map(|payload| payload.0.note).flatten();
    if note.as_ref().is_some_and(|value| value.len() > 4096) {
        return Err(ApiError::InvalidRequest(
            "decision note exceeds 4096 bytes".to_owned(),
        ));
    }
    Ok(note)
}

async fn approve_document_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Option<axum::Json<TrustedDecisionRequest>>,
) -> Result<axum::Json<ApiResponse<DocumentProposalDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let proposal = state
        .runtime
        .document_workflow()
        .approve(&id, decision_note(body)?)
        .await?;
    Ok(axum::Json(ApiResponse::ok(document_proposal_dto(proposal))))
}

async fn reject_document_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Option<axum::Json<TrustedDecisionRequest>>,
) -> Result<axum::Json<ApiResponse<DocumentProposalDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let proposal = state
        .runtime
        .document_workflow()
        .reject(&id, decision_note(body)?)
        .await?;
    Ok(axum::Json(ApiResponse::ok(document_proposal_dto(proposal))))
}

async fn retry_document_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<axum::Json<ApiResponse<DocumentProposalDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let proposal = state.runtime.document_workflow().retry(&id).await?;
    Ok(axum::Json(ApiResponse::ok(document_proposal_dto(proposal))))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/document-proposals", get(list_document_proposals))
        .route("/api/document-proposals/{id}", get(get_document_proposal))
        .route(
            "/api/document-proposals/{id}/approve",
            post(approve_document_proposal),
        )
        .route(
            "/api/document-proposals/{id}/reject",
            post(reject_document_proposal),
        )
        .route(
            "/api/document-proposals/{id}/retry",
            post(retry_document_proposal),
        )
}
