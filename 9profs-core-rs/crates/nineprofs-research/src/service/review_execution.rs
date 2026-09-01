use crate::{
    DocumentMap, ResolvedReviewStack, ReviewExecutionReport, ReviewTask, ReviewTaskExecutionError,
    ReviewTaskExecutionResult, ReviewTaskExecutor,
};

use super::ResearchService;

impl ResearchService {
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
