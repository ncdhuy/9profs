use crate::{
    DocumentMap, ResearchContext, ResearchError, ResolvedReviewStack, ReviewTask,
    load_canonical_authority_packs, plan_review_tasks, resolve_review_stack,
};

use super::ResearchService;

impl ResearchService {
    pub async fn resolve_review_stack(
        &self,
        context: &ResearchContext,
        as_of_ms: i64,
    ) -> Result<ResolvedReviewStack, ResearchError> {
        let packs = load_canonical_authority_packs()?;
        let requirements = self.list_regulation_requirements(None, None).await?;
        resolve_review_stack(context, &packs, &requirements, as_of_ms)
    }

    pub async fn plan_review_tasks(
        &self,
        context: &ResearchContext,
        map: &DocumentMap,
        as_of_ms: i64,
    ) -> Result<Vec<ReviewTask>, ResearchError> {
        let stack = self.resolve_review_stack(context, as_of_ms).await?;
        plan_review_tasks(context, map, &stack)
    }
}
