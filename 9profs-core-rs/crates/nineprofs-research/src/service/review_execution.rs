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
            results.push(executor.execute(task, map, stack).await?);
        }
        Ok(ReviewExecutionReport {
            task_results: results,
        })
    }
}
