use std::time::{SystemTime, UNIX_EPOCH};

use nineprofs_documents::DocumentBridgeError;
use nineprofs_research::{
    DocumentMap, ManuscriptReviewError, ManuscriptReviewResult, ResearchContext,
};
use thiserror::Error;

use crate::CoreRuntime;

#[derive(Debug, Error)]
pub enum ManuscriptReviewRuntimeError {
    #[error(transparent)]
    DocumentBridge(#[from] DocumentBridgeError),
    #[error("active document does not expose a document map")]
    MissingDocumentMap,
    #[error("active document document map is invalid")]
    InvalidDocumentMap,
    #[error("active document document map does not match the active version")]
    StaleDocumentMap,
    #[error(transparent)]
    Review(#[from] ManuscriptReviewError),
}

impl CoreRuntime {
    pub async fn run_manuscript_review(
        &self,
        document_id: &str,
        context: ResearchContext,
    ) -> Result<ManuscriptReviewResult, ManuscriptReviewRuntimeError> {
        let inspection = self.inspect_active_document(document_id).await?;
        let active_version = i64::try_from(inspection.version)
            .map_err(|_| ManuscriptReviewRuntimeError::StaleDocumentMap)?;
        let map_value = inspection
            .document_map
            .ok_or(ManuscriptReviewRuntimeError::MissingDocumentMap)?;
        let map: DocumentMap = serde_json::from_value(map_value)
            .map_err(|_| ManuscriptReviewRuntimeError::InvalidDocumentMap)?;

        if map.document_id != document_id
            || map.document_id != inspection.document_id
            || map.version != active_version
        {
            return Err(ManuscriptReviewRuntimeError::StaleDocumentMap);
        }

        let as_of_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before Unix epoch")
            .as_millis() as i64;
        self.research_service
            .run_manuscript_review(&map, &context, as_of_ms)
            .await
            .map_err(ManuscriptReviewRuntimeError::Review)
    }
}
