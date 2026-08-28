use super::common::research_content_hash_dto;
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
use nineprofs_api_types::CaptureResearchSourceSnapshotRequest;
use nineprofs_api_types::CreateResearchSourceRequest;
use nineprofs_api_types::ResearchCaptureMethodDto;
use nineprofs_api_types::ResearchSourceDto;
use nineprofs_api_types::ResearchSourceIdentityDto;
use nineprofs_api_types::ResearchSourceIdentityMethodDto;
use nineprofs_api_types::ResearchSourceIdentityRequest;
use nineprofs_api_types::ResearchSourceKindDto;
use nineprofs_api_types::ResearchSourceOriginDto;
use nineprofs_api_types::ResearchSourceSnapshotDto;
use nineprofs_research::CaptureMethod;
use nineprofs_research::CaptureSourceSnapshot;
use nineprofs_research::CreateResearchSource;
use nineprofs_research::ResearchSource;
use nineprofs_research::ResearchSourceIdentity;
use nineprofs_research::ResearchSourceIdentityInput;
use nineprofs_research::ResearchSourceIdentityMethod;
use nineprofs_research::ResearchSourceSnapshot;
use nineprofs_research::SourceKind;
use nineprofs_research::SourceOrigin;

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchSourcesQuery {
    #[serde(rename = "researchCaseId")]
    research_case_id: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ResearchSnapshotsQuery {
    #[serde(rename = "sourceId")]
    source_id: Option<String>,
}

async fn list_research_sources(
    State(state): State<AppState>,
    Query(query): Query<ResearchSourcesQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchSourceDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_sources(query.research_case_id.as_deref())
            .await?
            .into_iter()
            .map(research_source_dto)
            .collect(),
    )))
}

async fn get_research_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchSourceDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_source_dto(
        state.runtime.research_service().get_source(&id).await?,
    ))))
}

async fn create_research_source(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CreateResearchSourceRequest>,
) -> Result<axum::Json<ApiResponse<ResearchSourceDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let source = state
        .runtime
        .research_service()
        .create_source(CreateResearchSource {
            research_case_id: nineprofs_research::ResearchCaseId::parse(request.research_case_id)?,
            kind: source_kind(request.kind),
            label: request.label,
            identity: request.identity.map(source_identity),
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_source_dto(source))))
}

async fn list_research_snapshots(
    State(state): State<AppState>,
    Query(query): Query<ResearchSnapshotsQuery>,
) -> Result<axum::Json<ApiResponse<Vec<ResearchSourceSnapshotDto>>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(
        state
            .runtime
            .research_service()
            .list_snapshots(query.source_id.as_deref())
            .await?
            .into_iter()
            .map(research_snapshot_dto)
            .collect(),
    )))
}

async fn get_research_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ResearchSourceSnapshotDto>>, ApiError> {
    Ok(axum::Json(ApiResponse::ok(research_snapshot_dto(
        state.runtime.research_service().get_snapshot(&id).await?,
    ))))
}

async fn capture_research_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<CaptureResearchSourceSnapshotRequest>,
) -> Result<axum::Json<ApiResponse<ResearchSourceSnapshotDto>>, ApiError> {
    authorize_trusted_decision(&headers, state.runtime.config())?;
    let snapshot = state
        .runtime
        .research_service()
        .capture_snapshot(CaptureSourceSnapshot {
            source_id: nineprofs_research::ResearchSourceId::parse(request.source_id)?,
            content: request.content.into_bytes(),
            capture_method: capture_method(request.capture_method),
            origin: source_origin(request.origin),
            metadata: request.metadata,
        })
        .await?;
    Ok(axum::Json(ApiResponse::ok(research_snapshot_dto(snapshot))))
}

pub(crate) fn research_source_dto(value: ResearchSource) -> ResearchSourceDto {
    ResearchSourceDto {
        source_id: value.id.to_string(),
        research_case_id: value.research_case_id.to_string(),
        kind: source_kind_dto(value.kind),
        label: value.label,
        identity: value.identity.map(source_identity_dto),
        created_at_ms: value.created_at_ms,
    }
}

fn source_identity(value: ResearchSourceIdentityRequest) -> ResearchSourceIdentityInput {
    ResearchSourceIdentityInput {
        provider: value.provider,
        external_reference: value.external_reference,
        method: source_identity_method(value.method),
    }
}

fn source_identity_method(value: ResearchSourceIdentityMethodDto) -> ResearchSourceIdentityMethod {
    match value {
        ResearchSourceIdentityMethodDto::Imported => ResearchSourceIdentityMethod::Imported,
        ResearchSourceIdentityMethodDto::HumanConfirmed => {
            ResearchSourceIdentityMethod::HumanConfirmed
        }
    }
}

fn source_identity_method_dto(
    value: ResearchSourceIdentityMethod,
) -> ResearchSourceIdentityMethodDto {
    match value {
        ResearchSourceIdentityMethod::Imported => ResearchSourceIdentityMethodDto::Imported,
        ResearchSourceIdentityMethod::HumanConfirmed => {
            ResearchSourceIdentityMethodDto::HumanConfirmed
        }
    }
}

fn source_identity_dto(value: ResearchSourceIdentity) -> ResearchSourceIdentityDto {
    ResearchSourceIdentityDto {
        provider: value.provider,
        external_reference: value.external_reference,
        method: source_identity_method_dto(value.method),
        asserted_at_ms: value.asserted_at_ms,
    }
}

pub(crate) fn research_snapshot_dto(value: ResearchSourceSnapshot) -> ResearchSourceSnapshotDto {
    ResearchSourceSnapshotDto {
        snapshot_id: value.id.to_string(),
        source_id: value.source_id.to_string(),
        content_hash: research_content_hash_dto(value.content_hash),
        captured_at_ms: value.captured_at_ms,
        capture_method: capture_method_dto(value.capture_method),
        origin: source_origin_dto(value.origin),
        metadata: value.metadata,
    }
}

pub(crate) fn source_kind(value: ResearchSourceKindDto) -> SourceKind {
    match value {
        ResearchSourceKindDto::ReferencePdf => SourceKind::ReferencePdf,
        ResearchSourceKindDto::Manuscript => SourceKind::Manuscript,
        ResearchSourceKindDto::Dataset => SourceKind::Dataset,
        ResearchSourceKindDto::Web => SourceKind::Web,
        ResearchSourceKindDto::Regulation => SourceKind::Regulation,
        ResearchSourceKindDto::Other => SourceKind::Other,
    }
}

pub(crate) fn source_kind_dto(value: SourceKind) -> ResearchSourceKindDto {
    match value {
        SourceKind::ReferencePdf => ResearchSourceKindDto::ReferencePdf,
        SourceKind::Manuscript => ResearchSourceKindDto::Manuscript,
        SourceKind::Dataset => ResearchSourceKindDto::Dataset,
        SourceKind::Web => ResearchSourceKindDto::Web,
        SourceKind::Regulation => ResearchSourceKindDto::Regulation,
        SourceKind::Other => ResearchSourceKindDto::Other,
    }
}

pub(crate) fn capture_method(value: ResearchCaptureMethodDto) -> CaptureMethod {
    match value {
        ResearchCaptureMethodDto::UserProvided => CaptureMethod::UserProvided,
        ResearchCaptureMethodDto::UploadedArtifact => CaptureMethod::UploadedArtifact,
        ResearchCaptureMethodDto::ActiveDocument => CaptureMethod::ActiveDocument,
        ResearchCaptureMethodDto::OfficeCli => CaptureMethod::OfficeCli,
        ResearchCaptureMethodDto::WebRetrieval => CaptureMethod::WebRetrieval,
        ResearchCaptureMethodDto::ExternalImport => CaptureMethod::ExternalImport,
    }
}

pub(crate) fn capture_method_dto(value: CaptureMethod) -> ResearchCaptureMethodDto {
    match value {
        CaptureMethod::UserProvided => ResearchCaptureMethodDto::UserProvided,
        CaptureMethod::UploadedArtifact => ResearchCaptureMethodDto::UploadedArtifact,
        CaptureMethod::ActiveDocument => ResearchCaptureMethodDto::ActiveDocument,
        CaptureMethod::OfficeCli => ResearchCaptureMethodDto::OfficeCli,
        CaptureMethod::WebRetrieval => ResearchCaptureMethodDto::WebRetrieval,
        CaptureMethod::ExternalImport => ResearchCaptureMethodDto::ExternalImport,
    }
}

pub(crate) fn source_origin(value: ResearchSourceOriginDto) -> SourceOrigin {
    match value {
        ResearchSourceOriginDto::UploadedArtifact {
            artifact_id,
            revision_id,
        } => SourceOrigin::UploadedArtifact {
            artifact_id,
            revision_id,
        },
        ResearchSourceOriginDto::ActiveDocumentSnapshot {
            document_id,
            document_version,
        } => SourceOrigin::ActiveDocumentSnapshot {
            document_id,
            document_version,
        },
        ResearchSourceOriginDto::OfficeCliArtifactRevision {
            artifact_id,
            revision_id,
        } => SourceOrigin::OfficeCliArtifactRevision {
            artifact_id,
            revision_id,
        },
        ResearchSourceOriginDto::WebRetrieval {
            url,
            retrieved_at_ms,
        } => SourceOrigin::WebRetrieval {
            url,
            retrieved_at_ms,
        },
        ResearchSourceOriginDto::ExternalImport {
            provider,
            external_reference,
        } => SourceOrigin::ExternalImport {
            provider,
            external_reference,
        },
    }
}

pub(crate) fn source_origin_dto(value: SourceOrigin) -> ResearchSourceOriginDto {
    match value {
        SourceOrigin::UploadedArtifact {
            artifact_id,
            revision_id,
        } => ResearchSourceOriginDto::UploadedArtifact {
            artifact_id,
            revision_id,
        },
        SourceOrigin::ActiveDocumentSnapshot {
            document_id,
            document_version,
        } => ResearchSourceOriginDto::ActiveDocumentSnapshot {
            document_id,
            document_version,
        },
        SourceOrigin::OfficeCliArtifactRevision {
            artifact_id,
            revision_id,
        } => ResearchSourceOriginDto::OfficeCliArtifactRevision {
            artifact_id,
            revision_id,
        },
        SourceOrigin::WebRetrieval {
            url,
            retrieved_at_ms,
        } => ResearchSourceOriginDto::WebRetrieval {
            url,
            retrieved_at_ms,
        },
        SourceOrigin::ExternalImport {
            provider,
            external_reference,
        } => ResearchSourceOriginDto::ExternalImport {
            provider,
            external_reference,
        },
    }
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/research/sources",
            get(list_research_sources).post(create_research_source),
        )
        .route("/api/research/sources/{id}", get(get_research_source))
        .route(
            "/api/research/snapshots",
            get(list_research_snapshots).post(capture_research_snapshot),
        )
        .route("/api/research/snapshots/{id}", get(get_research_snapshot))
}
