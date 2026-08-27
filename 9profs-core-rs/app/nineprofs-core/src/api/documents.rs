use crate::api::ApiError;
use crate::api::AppState;
use axum::Router;
use axum::extract::Path;
use axum::extract::State;
use axum::routing::get;
use nineprofs_api_types::ActiveDocumentDto;
use nineprofs_api_types::ApiResponse;
use nineprofs_documents::ActiveDocumentDescriptor;

async fn list_documents(
    State(state): State<AppState>,
) -> axum::Json<ApiResponse<Vec<ActiveDocumentDto>>> {
    axum::Json(ApiResponse::ok(
        state
            .runtime
            .document_bridge()
            .list()
            .await
            .into_iter()
            .map(active_document_dto)
            .collect(),
    ))
}

async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::Json<ApiResponse<ActiveDocumentDto>>, ApiError> {
    let document = state
        .runtime
        .document_bridge()
        .get(&id)
        .await
        .ok_or_else(|| ApiError::DocumentNotFound(id.clone()))?;
    Ok(axum::Json(ApiResponse::ok(active_document_dto(document))))
}

pub(super) fn active_document_dto(document: ActiveDocumentDescriptor) -> ActiveDocumentDto {
    ActiveDocumentDto {
        document_id: document.document_id,
        document_type: document.document_type,
        authority: document.authority,
        version: document.version,
        capabilities: document.capabilities,
        availability: "available".to_owned(),
    }
}
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/documents", get(list_documents))
        .route("/api/documents/{id}", get(get_document))
}
