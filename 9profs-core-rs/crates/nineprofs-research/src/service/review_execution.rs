use std::time::Instant;

use crate::{
    DocumentMap, Finding, ResolvedReviewStack, ReviewExecutionReport, ReviewSynthesis,
    ReviewSynthesisError, ReviewSynthesisExecutor, ReviewTask, ReviewTaskExecutionError,
    ReviewTaskExecutionResult, ReviewTaskExecutor,
};

use super::ResearchService;

impl ResearchService {
    pub async fn synthesize_review_findings(
        &self,
        findings: &[Finding],
    ) -> Result<ReviewSynthesis, ReviewSynthesisError> {
        ReviewSynthesisExecutor::from_env()
            .synthesize(findings)
            .await
    }

    pub async fn execute_review_task(
        &self,
        task: &ReviewTask,
        map: &DocumentMap,
        stack: &ResolvedReviewStack,
    ) -> Result<ReviewTaskExecutionResult, ReviewTaskExecutionError> {
        ReviewTaskExecutor::from_env()
            .execute(task, map, stack)
            .await
    }

    pub async fn execute_review_tasks(
        &self,
        tasks: &[ReviewTask],
        map: &DocumentMap,
        stack: &ResolvedReviewStack,
    ) -> Result<ReviewExecutionReport, ReviewTaskExecutionError> {
        let executor = ReviewTaskExecutor::from_env();
        let mut results = Vec::with_capacity(tasks.len());
        for task in tasks {
            let started = Instant::now();
            let started_at_ms = crate::review_orchestration::review_diagnostic_now_ms();
            match executor.execute(task, map, stack).await {
                Ok(result) => {
                    crate::review_orchestration::emit_review_diagnostic(
                        "review_task",
                        started_at_ms,
                        started,
                        "success",
                        Some(&result.task_id),
                        Some(&result.task_kind),
                        Some(&format!("{:?}", result.executor_mode)),
                        None,
                        Some(result.rejections.len()),
                    );
                    results.push(result);
                }
                Err(error) => {
                    crate::review_orchestration::emit_review_diagnostic(
                        "review_task",
                        started_at_ms,
                        started,
                        "failure",
                        Some(&task.id),
                        Some(&task.kind),
                        Some(&format!("{:?}", task.executor_mode)),
                        Some(error.diagnostic_category()),
                        None,
                    );
                    return Err(error);
                }
            }
        }
        Ok(ReviewExecutionReport {
            task_results: results,
        })
    }
}
