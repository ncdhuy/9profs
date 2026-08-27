use super::common::research_content_hash_dto;
use super::sources::{capture_method, capture_method_dto};
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
use nineprofs_api_types::CreateResearchEvidenceRequest;
use nineprofs_api_types::ResearchEvidenceDto;
use nineprofs_api_types::ResearchEvidenceLocatorDto;
use nineprofs_research::CreateResearchEvidence;
use nineprofs_research::EvidenceLocator;
use nineprofs_research::ResearchEvidence;

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchEvidenceQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
    #[serde(rename = "sourceSnapshotId")]
    source_snapshot_id: Option<String>,
}

async fn list_research_evidence(
    State(state): State<AppState>,
    Query(query): Query<ResearchEvidenceQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchEvidenceDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_evidence(
                query.research_case_id.as_deref(),
                query.source_snapshot_id.as_deref(),
            )
            .await?
            .into_iter()
            .map(research_evidence_dto)
            .collect(),
    )))
}

async fn get_research_evidence(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchEvidenceDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_evidence_dto(
        state.runtime.research_service().get_evidence(&id).await?,
    ))))
}

async fn create_research_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateResearchEvidenceRequest>,
) -> Result<axum::Json<ApiResponse<ResearchEvidenceDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let evidence = state
        .runtime
        .research_service()
        .create_evidence(CreateResearchEvidence {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(
                request.source_snapshot_id,
            )?,
            verbatim_excerpt: request.verbatim_excerpt,
            normalized_text: request.normalized_text,
            locator: evidence_locator(request.locator),
            capture_method: capture_method(request.capture_method),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_evidence_dto(evidence))))
}

pub(crate) fn research_evidence_dto(value: ResearchEvidence) -> ResearchEvidenceDto {
    ResearchEvidenceDto {
        evidence_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        source_snapshot_id: value.source_snapshot_id.to_string(),
        verbatim_excerpt: value.verbatim_excerpt,
        normalized_text: value.normalized_text,
        locator: evidence_locator_dto(value.locator),
        excerpt_hash: research_content_hash_dto(value.excerpt_hash),
        captured_at_ms: value.captured_at_ms,
        capture_method: capture_method_dto(value.capture_method),
        pdf_extraction_id: value.pdf_extraction_id.map(|id| id.to_string()),
    }
}

pub(crate) fn evidence_locator(value: ResearchEvidenceLocatorDto) -> EvidenceLocator {
    match value {
        ResearchEvidenceLocatorDto::TextRange { start, end } => {
            EvidenceLocator::TextRange { start, end }
        }
        ResearchEvidenceLocatorDto::Pdf { page, end_page } => {
            EvidenceLocator::Pdf { page, end_page }
        }
        ResearchEvidenceLocatorDto::PdfTextRange { page, start, end } => {
            EvidenceLocator::PdfTextRange { page, start, end }
        }
        ResearchEvidenceLocatorDto::Manuscript {
            block_id,
            start,
            end,
        } => EvidenceLocator::Manuscript {
            block_id,
            start,
            end,
        },
        ResearchEvidenceLocatorDto::Spreadsheet { sheet, range } => {
            EvidenceLocator::Spreadsheet { sheet, range }
        }
        ResearchEvidenceLocatorDto::Web {
            fragment,
            start,
            end,
        } => EvidenceLocator::Web {
            fragment,
            start,
            end,
        },
        ResearchEvidenceLocatorDto::Regulation {
            article,
            section,
            clause,
        } => EvidenceLocator::Regulation {
            article,
            section,
            clause,
        },
    }
}

pub(crate) fn evidence_locator_dto(value: EvidenceLocator) -> ResearchEvidenceLocatorDto {
    match value {
        EvidenceLocator::TextRange { start, end } => {
            ResearchEvidenceLocatorDto::TextRange { start, end }
        }
        EvidenceLocator::Pdf { page, end_page } => {
            ResearchEvidenceLocatorDto::Pdf { page, end_page }
        }
        EvidenceLocator::PdfTextRange { page, start, end } => {
            ResearchEvidenceLocatorDto::PdfTextRange { page, start, end }
        }
        EvidenceLocator::Manuscript {
            block_id,
            start,
            end,
        } => ResearchEvidenceLocatorDto::Manuscript {
            block_id,
            start,
            end,
        },
        EvidenceLocator::Spreadsheet { sheet, range } => {
            ResearchEvidenceLocatorDto::Spreadsheet { sheet, range }
        }
        EvidenceLocator::Web {
            fragment,
            start,
            end,
        } => ResearchEvidenceLocatorDto::Web {
            fragment,
            start,
            end,
        },
        EvidenceLocator::Regulation {
            article,
            section,
            clause,
        } => ResearchEvidenceLocatorDto::Regulation {
            article,
            section,
            clause,
        },
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/evidence",
            get(list_research_evidence).post(create_research_evidence),
        )
        .route("/api/research/evidence/{id}", get(get_research_evidence))
}
