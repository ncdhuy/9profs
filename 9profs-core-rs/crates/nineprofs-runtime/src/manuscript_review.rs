use std::time::{Instant, SystemTime, UNIX_EPOCH};

use nineprofs_documents::DocumentBridgeError;
use nineprofs_research::{
    DocumentMap, ManuscriptReviewError, ManuscriptReviewResult, ResearchContext,
};
use thiserror::Error;

use crate::CoreRuntime;

fn diagnostic_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn emit_diagnostic(
    stage: &str,
    started_at_ms: i64,
    started: Instant,
    result: &str,
    error_category: &str,
) {
    eprintln!(
        "review_run_diagnostic stage={stage} started_at_ms={started_at_ms} ended_at_ms={} elapsed_ms={} result={result} task_id=- task_kind=- executor_mode=- error_category={error_category} rejected_finding_count=0",
        diagnostic_now_ms(),
        started.elapsed().as_millis(),
    );
}

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
        let run_started = Instant::now();
        let run_started_at_ms = diagnostic_now_ms();
        let document_map_started = Instant::now();
        let document_map_started_at_ms = diagnostic_now_ms();
        let inspection = match self.inspect_active_document(document_id).await {
            Ok(inspection) => inspection,
            Err(error) => {
                emit_diagnostic(
                    "document_map",
                    document_map_started_at_ms,
                    document_map_started,
                    "failure",
                    "document_unavailable",
                );
                emit_diagnostic(
                    "total",
                    run_started_at_ms,
                    run_started,
                    "failure",
                    "document_unavailable",
                );
                return Err(ManuscriptReviewRuntimeError::DocumentBridge(error));
            }
        };
        let active_version = i64::try_from(inspection.version)
            .map_err(|_| ManuscriptReviewRuntimeError::StaleDocumentMap)?;
        let map_value = inspection
            .document_map
            .ok_or(ManuscriptReviewRuntimeError::MissingDocumentMap);
        let map_value = match map_value {
            Ok(map_value) => map_value,
            Err(error) => {
                emit_diagnostic(
                    "document_map",
                    document_map_started_at_ms,
                    document_map_started,
                    "failure",
                    "document_unavailable",
                );
                emit_diagnostic(
                    "total",
                    run_started_at_ms,
                    run_started,
                    "failure",
                    "document_unavailable",
                );
                return Err(error);
            }
        };
        let map = serde_json::from_value::<DocumentMap>(map_value)
            .map_err(|_| ManuscriptReviewRuntimeError::InvalidDocumentMap);
        let map = match map {
            Ok(map) => map,
            Err(error) => {
                emit_diagnostic(
                    "document_map",
                    document_map_started_at_ms,
                    document_map_started,
                    "failure",
                    "document_unavailable",
                );
                emit_diagnostic(
                    "total",
                    run_started_at_ms,
                    run_started,
                    "failure",
                    "document_unavailable",
                );
                return Err(error);
            }
        };

        if map.document_id != document_id
            || map.document_id != inspection.document_id
            || map.version != active_version
        {
            emit_diagnostic(
                "document_map",
                document_map_started_at_ms,
                document_map_started,
                "failure",
                "stale_document",
            );
            emit_diagnostic(
                "total",
                run_started_at_ms,
                run_started,
                "failure",
                "stale_document",
            );
            return Err(ManuscriptReviewRuntimeError::StaleDocumentMap);
        }
        emit_diagnostic(
            "document_map",
            document_map_started_at_ms,
            document_map_started,
            "success",
            "-",
        );

        let as_of_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before Unix epoch")
            .as_millis() as i64;
        let result = self
            .research_service
            .run_manuscript_review(&map, &context, as_of_ms)
            .await
            .map_err(ManuscriptReviewRuntimeError::Review);
        emit_diagnostic(
            "total",
            run_started_at_ms,
            run_started,
            if result.is_ok() { "success" } else { "failure" },
            if result.is_ok() { "-" } else { "review" },
        );
        result
    }
}
