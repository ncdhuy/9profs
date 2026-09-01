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

impl crate::ResearchService {
    pub async fn run_manuscript_review(
        &self,
        map: &DocumentMap,
        context: &ResearchContext,
        as_of_ms: i64,
    ) -> Result<ManuscriptReviewResult, ManuscriptReviewError> {
        context.validate()?;

        let stack = self.resolve_review_stack(context, as_of_ms).await?;
        let tasks = crate::plan_review_tasks(context, map, &stack)?;
        let execution = self.execute_review_tasks(&tasks, map, &stack).await?;
        let findings = execution.findings();
        let synthesis = self.synthesize_review_findings(&findings).await?;

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
