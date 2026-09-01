use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ConsolidatedFinding, DocumentMap, ResearchContext, ResearchError, ReviewSynthesisError,
    ReviewTaskExecutionError,
};

pub const MANUSCRIPT_REVIEW_RESULT_CONTRACT_VERSION: &str = "manuscript-review-result-v0.1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReviewResult {
    pub document_id: String,
    pub document_version: i64,
    pub synthesized_findings: Vec<ConsolidatedFinding>,
    pub summary: ManuscriptReviewSummary,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptReviewSummary {
    pub task_count: usize,
    pub raw_finding_count: usize,
    pub rejected_finding_count: usize,
    pub consolidated_finding_count: usize,
}

#[derive(Debug, Error)]
pub enum ManuscriptReviewError {
    #[error(transparent)]
    Research(#[from] ResearchError),
    #[error(transparent)]
    Execution(#[from] ReviewTaskExecutionError),
    #[error(transparent)]
    Synthesis(#[from] ReviewSynthesisError),
}

pub(crate) fn review_diagnostic_now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

pub(crate) fn emit_review_diagnostic(
    stage: &str,
    started_at_ms: i64,
    started: Instant,
    result: &str,
    task_id: Option<&str>,
    task_kind: Option<&str>,
    executor_mode: Option<&str>,
    error_category: Option<&str>,
    rejected_finding_count: Option<usize>,
) {
    let task_id = task_id.unwrap_or("-");
    let task_kind = task_kind.unwrap_or("-");
    let executor_mode = executor_mode.unwrap_or("-");
    let error_category = error_category.unwrap_or("-");
    let rejected_finding_count = rejected_finding_count.unwrap_or(0);
    eprintln!(
        "review_run_diagnostic stage={stage} started_at_ms={started_at_ms} ended_at_ms={} elapsed_ms={} result={result} task_id={task_id} task_kind={task_kind} executor_mode={executor_mode} error_category={error_category} rejected_finding_count={rejected_finding_count}",
        review_diagnostic_now_ms(),
        started.elapsed().as_millis(),
    );
}

impl crate::ResearchService {
    pub async fn run_manuscript_review(
        &self,
        map: &DocumentMap,
        context: &ResearchContext,
        as_of_ms: i64,
    ) -> Result<ManuscriptReviewResult, ManuscriptReviewError> {
        let run_started = Instant::now();
        let run_started_at_ms = review_diagnostic_now_ms();
        let context_started = Instant::now();
        let context_started_at_ms = review_diagnostic_now_ms();
        if let Err(error) = context.validate() {
            emit_review_diagnostic(
                "context_validation",
                context_started_at_ms,
                context_started,
                "failure",
                None,
                None,
                None,
                Some("context_invalid"),
                None,
            );
            emit_review_diagnostic(
                "research_total",
                run_started_at_ms,
                run_started,
                "failure",
                None,
                None,
                None,
                Some("context_invalid"),
                None,
            );
            return Err(ManuscriptReviewError::Research(error));
        }
        emit_review_diagnostic(
            "context_validation",
            context_started_at_ms,
            context_started,
            "success",
            None,
            None,
            None,
            None,
            None,
        );

        let authority_started = Instant::now();
        let authority_started_at_ms = review_diagnostic_now_ms();
        let stack = match self.resolve_review_stack(context, as_of_ms).await {
            Ok(stack) => {
                emit_review_diagnostic(
                    "authority_resolution",
                    authority_started_at_ms,
                    authority_started,
                    "success",
                    None,
                    None,
                    None,
                    None,
                    None,
                );
                stack
            }
            Err(error) => {
                emit_review_diagnostic(
                    "authority_resolution",
                    authority_started_at_ms,
                    authority_started,
                    "failure",
                    None,
                    None,
                    None,
                    Some("authority_resolution"),
                    None,
                );
                emit_review_diagnostic(
                    "research_total",
                    run_started_at_ms,
                    run_started,
                    "failure",
                    None,
                    None,
                    None,
                    Some("authority_resolution"),
                    None,
                );
                return Err(ManuscriptReviewError::Research(error));
            }
        };

        let planning_started = Instant::now();
        let planning_started_at_ms = review_diagnostic_now_ms();
        let tasks = match crate::plan_review_tasks(context, map, &stack) {
            Ok(tasks) => {
                emit_review_diagnostic(
                    "review_planning",
                    planning_started_at_ms,
                    planning_started,
                    "success",
                    None,
                    None,
                    None,
                    None,
                    None,
                );
                tasks
            }
            Err(error) => {
                emit_review_diagnostic(
                    "review_planning",
                    planning_started_at_ms,
                    planning_started,
                    "failure",
                    None,
                    None,
                    None,
                    Some("review_planning"),
                    None,
                );
                emit_review_diagnostic(
                    "research_total",
                    run_started_at_ms,
                    run_started,
                    "failure",
                    None,
                    None,
                    None,
                    Some("review_planning"),
                    None,
                );
                return Err(ManuscriptReviewError::Research(error));
            }
        };
        eprintln!(
            "review_run_diagnostic stage=review_planning task_count={}",
            tasks.len()
        );
        let execution = match self.execute_review_tasks(&tasks, map, &stack).await {
            Ok(execution) => execution,
            Err(error) => {
                emit_review_diagnostic(
                    "research_total",
                    run_started_at_ms,
                    run_started,
                    "failure",
                    None,
                    None,
                    None,
                    Some(error.diagnostic_category()),
                    None,
                );
                return Err(ManuscriptReviewError::Execution(error));
            }
        };
        let findings = execution.findings();
        let synthesis_started = Instant::now();
        let synthesis_started_at_ms = review_diagnostic_now_ms();
        let synthesis = match self.synthesize_review_findings(&findings).await {
            Ok(synthesis) => {
                emit_review_diagnostic(
                    "review_synthesis",
                    synthesis_started_at_ms,
                    synthesis_started,
                    "success",
                    None,
                    None,
                    None,
                    None,
                    None,
                );
                synthesis
            }
            Err(error) => {
                emit_review_diagnostic(
                    "review_synthesis",
                    synthesis_started_at_ms,
                    synthesis_started,
                    "failure",
                    None,
                    None,
                    None,
                    Some(error.diagnostic_category()),
                    None,
                );
                emit_review_diagnostic(
                    "research_total",
                    run_started_at_ms,
                    run_started,
                    "failure",
                    None,
                    None,
                    None,
                    Some(error.diagnostic_category()),
                    None,
                );
                return Err(ManuscriptReviewError::Synthesis(error));
            }
        };

        emit_review_diagnostic(
            "research_total",
            run_started_at_ms,
            run_started,
            "success",
            None,
            None,
            None,
            None,
            None,
        );
        Ok(ManuscriptReviewResult {
            document_id: map.document_id.clone(),
            document_version: map.version,
            summary: ManuscriptReviewSummary {
                task_count: tasks.len(),
                raw_finding_count: execution.raw_candidate_count(),
                rejected_finding_count: execution.rejected_finding_count(),
                consolidated_finding_count: synthesis.findings.len(),
            },
            synthesized_findings: synthesis.findings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_serialization_preserves_identity_order_and_provenance() {
        let result = ManuscriptReviewResult {
            document_id: "doc-1".to_owned(),
            document_version: 7,
            synthesized_findings: vec![],
            summary: ManuscriptReviewSummary {
                consolidated_finding_count: 0,
                ..ManuscriptReviewSummary::default()
            },
        };

        let json = serde_json::to_value(result).expect("result serializes");
        assert_eq!(json["documentId"], "doc-1");
        assert_eq!(json["documentVersion"], 7);
        assert_eq!(json["synthesizedFindings"].as_array().unwrap().len(), 0);
    }
}
