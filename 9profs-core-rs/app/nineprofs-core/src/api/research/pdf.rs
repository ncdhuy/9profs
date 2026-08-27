use super::common::{header_text, research_content_hash_dto, safe_upload_label};
use super::evidence::research_evidence_dto;
use super::sources::{research_snapshot_dto, research_source_dto};
use crate::api::ApiError;
use crate::api::AppState;
use crate::api::proposals::authorize_trusted_decision;
use axum::Router;
use axum::body::Body;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::routing::post;
use futures_util::StreamExt;
use nineprofs_api_types::ApiResponse;
use nineprofs_api_types::CaptureResearchPdfEvidenceRequest;
use nineprofs_api_types::CaptureResearchPdfExtractionRequest;
use nineprofs_api_types::EventEnvelope;
use nineprofs_api_types::ReferencePdfIngestionDto;
use nineprofs_api_types::ResearchArtifactDto;
use nineprofs_api_types::ResearchEvidenceDto;
use nineprofs_api_types::ResearchPdfExtractionDto;
use nineprofs_api_types::ResearchPdfExtractionStatusDto;
use nineprofs_api_types::ResearchPdfPageDto;
use nineprofs_api_types::ResearchPdfPageListDto;
use nineprofs_research::CapturePdfEvidence;
use nineprofs_research::CapturePdfExtraction;
use nineprofs_research::CapturePdfPage;
use nineprofs_research::CreateResearchSource;
use nineprofs_research::ResearchPdfExtraction;
use nineprofs_research::ResearchPdfPage;
use nineprofs_research::ResearchPdfPageBatch;
use nineprofs_research::SourceKind;
use std::collections::BTreeMap;

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchPdfPagesQuery {
    #[serde(rename = "startPage")]
    start_page: Option<u32>,
    limit: Option<u32>,
}

async fn ingest_reference_pdf(
    State(state): State<AppState>,
    Path(case_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> Result<axum::Json<ApiResponse<ReferencePdfIngestionDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    if let Some(content_type) = header_text(&headers, "content-type")? {
        let media_type = content_type.split(';').next().unwrap_or("").trim();
        if !media_type.eq_ignore_ascii_case("application/pdf") {
            return Err(ApiError::InvalidRequest(
                "reference PDF upload must use application/pdf".to_owned(),
            ));
        }
    }
    let original_filename = safe_upload_label(
        header_text(&headers, "x-nineprofs-original-filename")?.as_deref(),
        "reference.pdf",
    )?;
    let source_label = safe_upload_label(
        header_text(&headers, "x-nineprofs-source-label")?.as_deref(),
        &original_filename,
    )?;
    let service = state.runtime.research_service();
    let research_case_id = nineprofs_research::ResearchCaseId::parse(case_id)?;
    service.get_case(research_case_id.as_str()).await?;
    let store = service
        .artifact_store()
        .ok_or_else(|| ApiError::InvalidRequest("PDF artifact store is unavailable".to_owned()))?;
    let mut upload = store.begin_upload(&original_filename)?;
    let mut chunks = body.into_data_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
        upload.append(&chunk)?;
    }
    let artifact = upload.finish().await?;
    let source = service
        .create_source(CreateResearchSource {
            research_case_id,
            kind: SourceKind::ReferencePdf,
            label: source_label,
        })
        .await?;
    let snapshot = service
        .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
        .await?;
    let _ = state.runtime.event_bus().publish(EventEnvelope::new(
        "research.pdfIngested",
        serde_json::json!({
            "artifact_id": artifact.artifact_id(),
            "source_id": source.id,
            "snapshot_id": snapshot.id,
            "size_bytes": artifact.artifact().size_bytes,
            "content_hash": artifact.content_hash().value,
        }),
    ));
    Ok(axum::Json(ApiResponse::ok(ReferencePdfIngestionDto {
        artifact: research_artifact_dto(artifact.artifact().clone()),
        source: research_source_dto(source),
        snapshot: research_snapshot_dto(snapshot),
    })))
}

async fn capture_research_pdf_extraction(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CaptureResearchPdfExtractionRequest>,
) -> Result<axum::Json<ApiResponse<ResearchPdfExtractionDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let extraction = state
        .runtime
        .research_service()
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(snapshot_id)?,
            extractor: request.extractor,
            extractor_version: request.extractor_version,
            page_count: request.page_count,
            status: pdf_extraction_status(request.status),
            pages: request
                .pages
                .into_iter()
                .map(|page| CapturePdfPage {
                    page: page.page,
                    text: page.text,
                })
                .collect(),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_pdf_extraction_dto(
        extraction,
    ))))
}

async fn get_latest_research_pdf_extraction(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchPdfExtractionDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_pdf_extraction_dto(
        state
            .runtime
            .research_service()
            .latest_pdf_extraction(&snapshot_id)
            .await?,
    ))))
}

async fn get_research_pdf_extraction_by_id(
    State(state): State<AppState>,
    Path(extraction_id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchPdfExtractionDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_pdf_extraction_dto(
        state
            .runtime
            .research_service()
            .get_pdf_extraction_by_id(&extraction_id)
            .await?,
    ))))
}

async fn list_research_pdf_extractions(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchPdfExtractionDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_pdf_extractions(&snapshot_id)
            .await?
            .into_iter()
            .map(research_pdf_extraction_dto)
            .collect(),
    )))
}

async fn list_research_pdf_pages(
    State(state): State<AppState>,
    Path(extraction_id): Path<String>,
    Query(query): Query<ResearchPdfPagesQuery>,
) -> Result<axum::Json<ApiResponse<ResearchPdfPageListDto>>, ApiError> {
    let batch = state
        .runtime
        .research_service()
        .list_pdf_pages(
            &extraction_id,
            query.start_page.unwrap_or(1),
            query.limit.unwrap_or(50),
        )
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_pdf_page_list_dto(
        batch,
    ))))
}

async fn get_research_pdf_page(
    State(state): State<AppState>,
    Path((extraction_id, page)): Path<(String, u32)>,
) -> Result<axum::Json<ApiResponse<ResearchPdfPageDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_pdf_page_dto(
        state
            .runtime
            .research_service()
            .get_pdf_page(&extraction_id, page)
            .await?,
    ))))
}

async fn capture_research_pdf_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CaptureResearchPdfEvidenceRequest>,
) -> Result<axum::Json<ApiResponse<ResearchEvidenceDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let evidence = state
        .runtime
        .research_service()
        .capture_pdf_evidence(CapturePdfEvidence {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(
                request.source_snapshot_id,
            )?,
            extraction_id: nineprofs_research::ResearchPdfExtractionId::parse(
                request.extraction_id,
            )?,
            page: request.page,
            start: request.start,
            end: request.end,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_evidence_dto(evidence))))
}

pub(crate) fn research_artifact_dto(
    value: nineprofs_research::ResearchArtifact,
) -> ResearchArtifactDto {
    ResearchArtifactDto {
        artifact_id: value.id,
        content_hash: research_content_hash_dto(value.content_hash),
        size_bytes: value.size_bytes,
        media_type: value.media_type,
        original_filename: value.original_filename,
        created_at_ms: value.created_at_ms,
    }
}

pub(crate) fn research_pdf_extraction_dto(
    value: ResearchPdfExtraction,
) -> ResearchPdfExtractionDto {
    ResearchPdfExtractionDto {
        extraction_id: value.id.to_string(),
        source_snapshot_id: value.source_snapshot_id.to_string(),
        artifact_id: value.artifact_id,
        extractor: value.extractor,
        extractor_version: value.extractor_version,
        page_count: value.page_count,
        extraction_hash: research_content_hash_dto(value.extraction_hash),
        extracted_at_ms: value.extracted_at_ms,
        status: pdf_extraction_status_dto(value.status),
    }
}

pub(crate) fn research_pdf_page_dto(value: ResearchPdfPage) -> ResearchPdfPageDto {
    ResearchPdfPageDto {
        extraction_id: value.extraction_id.to_string(),
        page: value.page,
        text: value.text,
        text_hash: research_content_hash_dto(value.text_hash),
    }
}

pub(crate) fn research_pdf_page_list_dto(value: ResearchPdfPageBatch) -> ResearchPdfPageListDto {
    let ResearchPdfPageBatch {
        pages,
        start_page,
        limit,
        has_more,
        next_start_page,
    } = value;
    ResearchPdfPageListDto {
        data: pages.into_iter().map(research_pdf_page_dto).collect(),
        start_page,
        limit,
        has_more,
        next_start_page,
    }
}

pub(crate) fn pdf_extraction_status(
    value: ResearchPdfExtractionStatusDto,
) -> nineprofs_research::PdfExtractionStatus {
    match value {
        ResearchPdfExtractionStatusDto::Ready => nineprofs_research::PdfExtractionStatus::Ready,
        ResearchPdfExtractionStatusDto::NoExtractableText => {
            nineprofs_research::PdfExtractionStatus::NoExtractableText
        }
        ResearchPdfExtractionStatusDto::Failed => nineprofs_research::PdfExtractionStatus::Failed,
        ResearchPdfExtractionStatusDto::PasswordRequired => {
            nineprofs_research::PdfExtractionStatus::PasswordRequired
        }
    }
}

pub(crate) fn pdf_extraction_status_dto(
    value: nineprofs_research::PdfExtractionStatus,
) -> ResearchPdfExtractionStatusDto {
    match value {
        nineprofs_research::PdfExtractionStatus::Ready => ResearchPdfExtractionStatusDto::Ready,
        nineprofs_research::PdfExtractionStatus::NoExtractableText => {
            ResearchPdfExtractionStatusDto::NoExtractableText
        }
        nineprofs_research::PdfExtractionStatus::Failed => ResearchPdfExtractionStatusDto::Failed,
        nineprofs_research::PdfExtractionStatus::PasswordRequired => {
            ResearchPdfExtractionStatusDto::PasswordRequired
        }
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/cases/{id}/reference-pdfs",
            post(ingest_reference_pdf),
        )
        .route(
            "/api/research/snapshots/{id}/pdf-extraction",
            get(get_latest_research_pdf_extraction).post(capture_research_pdf_extraction),
        )
        .route(
            "/api/research/source-snapshots/{id}/pdf-extractions",
            get(list_research_pdf_extractions),
        )
        .route(
            "/api/research/pdf-extractions/{id}",
            get(get_research_pdf_extraction_by_id),
        )
        .route(
            "/api/research/pdf-extractions/{id}/pages",
            get(list_research_pdf_pages),
        )
        .route(
            "/api/research/pdf-extractions/{id}/pages/{page}",
            get(get_research_pdf_page),
        )
        .route(
            "/api/research/pdf-evidence",
            post(capture_research_pdf_evidence),
        )
}
