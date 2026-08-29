use std::collections::BTreeMap;

use nineprofs_common::{new_id, now_ms};
use nineprofs_research::{
    ClaimEvidenceRelation, ClaimReviewKind, MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_CONTRACT_VERSION,
    ManuscriptClaimExtractionIdentity, ManuscriptClaimInventoryBlockInput,
    ManuscriptClaimInventoryBlockKind, ManuscriptClaimInventoryIdentity,
    ManuscriptClaimInventoryItem, ManuscriptClaimInventoryStatus,
    REFERENCE_RESOLVER_POLICY_VERSION, ResearchCaseId, ResearchError, ResearchSourceId, SourceKind,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use sqlx::Row;
use thiserror::Error;

use crate::{
    ASSESSMENT_CONTRACT_VERSION, CROSS_CLAIM_CONSISTENCY_ASSESSMENT_CONTRACT_VERSION,
    CROSS_CLAIM_CONSISTENCY_ASSESSMENT_IMPLEMENTATION_VERSION,
    CROSS_CLAIM_DISCOVERY_CONTRACT_VERSION, CROSS_CLAIM_DISCOVERY_IMPLEMENTATION_VERSION,
    CitationAssessmentProviderIdentity, CitationExpectationProviderIdentity,
    CitationReviewBlockInput, CitationReviewCitationInput, CitationReviewError, CitationReviewItem,
    CitationReviewItemStatus, CitationReviewRunStatus, CitationReviewService,
    CitationVerificationStatus, CoverageAttentionReason, CoverageAttentionState,
    CrossClaimAssessmentStatus, CrossClaimCandidateDiscoveryProviderIdentity,
    CrossClaimConsistencyAssessmentProviderIdentity, CrossClaimConsistencyAttentionReason,
    CrossClaimConsistencyAttentionState, CrossClaimConsistencyRelation,
    CrossClaimDifferenceDimension, MANUSCRIPT_CITATION_EXPECTATION_CONTRACT_VERSION,
    MANUSCRIPT_CLAIM_COVERAGE_ANALYSIS_CONTRACT_VERSION, ManuscriptCitationExpectationRun,
    ManuscriptCitationExpectationRunStatus, ManuscriptClaimCoverageBridgeStatus,
    ManuscriptClaimCoverageRun, ManuscriptClaimCoverageRunStatus,
    ManuscriptClaimCoverageStructuralCitationState, ManuscriptClaimCoverageTarget,
    ManuscriptCrossClaimAssessmentRun, ManuscriptCrossClaimAssessmentRunStatus,
    ManuscriptCrossClaimCandidateRun, ManuscriptCrossClaimCandidateRunStatus,
    StartManuscriptCitationExpectation, StartManuscriptCitationReview,
    StartManuscriptClaimCoverage, StartManuscriptCrossClaimAssessment,
    StartManuscriptCrossClaimCandidates,
};

pub const MANUSCRIPT_RESEARCH_REVIEW_CONTRACT_VERSION: &str = "manuscript-research-review-v1";
pub const MANUSCRIPT_RESEARCH_REVIEW_EXECUTION_IDENTITY_HASH_ALGORITHM: &str = "sha256";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewCitationExecutionIdentity {
    pub claim_extractor: Option<ManuscriptClaimExtractionIdentity>,
    pub citation_assessor: Option<CitationAssessmentProviderIdentity>,
    pub citation_assessment_contract_version: String,
    pub reference_resolution_policy_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewInventoryExecutionIdentity {
    pub inventory_extractor: Option<ManuscriptClaimInventoryIdentity>,
    pub coverage_contract_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewExecutionIdentity {
    pub input_hash_algorithm: String,
    pub input_hash: String,
    pub citation_review: ManuscriptResearchReviewCitationExecutionIdentity,
    pub claim_inventory: ManuscriptResearchReviewInventoryExecutionIdentity,
    pub claim_coverage_analysis_contract_version: String,
    pub citation_expectation: CitationExpectationProviderIdentity,
    pub citation_expectation_contract_version: String,
    pub cross_claim_candidate: CrossClaimCandidateDiscoveryProviderIdentity,
    pub cross_claim_candidate_contract_version: String,
    pub cross_claim_assessment: CrossClaimConsistencyAssessmentProviderIdentity,
    pub cross_claim_assessment_contract_version: String,
    pub review_contract_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewCitationObservations {
    pub citations: Vec<CitationReviewCitationInput>,
    pub citation_blocks: Vec<CitationReviewBlockInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewClaimInventoryObservations {
    pub whole_manuscript_blocks: Vec<ManuscriptClaimInventoryBlockInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartManuscriptResearchReview {
    #[serde(default)]
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub citation_review_observations: ManuscriptResearchReviewCitationObservations,
    pub claim_inventory_observations: ManuscriptResearchReviewClaimInventoryObservations,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptResearchReviewRunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptResearchReviewFailureStage {
    CitationReview,
    ClaimInventory,
    ClaimCoverage,
    CitationExpectation,
    CrossClaimDiscovery,
    CrossClaimAssessment,
    Projection,
    Persistence,
}

impl ManuscriptResearchReviewFailureStage {
    fn as_str(&self) -> &'static str {
        match self {
            Self::CitationReview => "citation_review",
            Self::ClaimInventory => "claim_inventory",
            Self::ClaimCoverage => "claim_coverage",
            Self::CitationExpectation => "citation_expectation",
            Self::CrossClaimDiscovery => "cross_claim_discovery",
            Self::CrossClaimAssessment => "cross_claim_assessment",
            Self::Projection => "projection",
            Self::Persistence => "persistence",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewSummary {
    pub total_inventory_claims: u32,
    pub coverage_review_suggested_count: u32,
    pub expectation_review_needed_count: u32,
    pub assessment_unavailable_count: u32,
    pub claims_with_support_count: u32,
    pub claims_with_contradiction_count: u32,
    pub claims_with_blocked_verification_count: u32,
    pub claims_with_unverified_verification_count: u32,
    pub consistency_assessed_count: u32,
    pub consistency_conflict_count: u32,
    pub consistency_compatible_count: u32,
    pub consistency_qualification_count: u32,
    pub consistency_equivalent_count: u32,
    pub consistency_not_comparable_count: u32,
    pub consistency_insufficient_context_count: u32,
    pub consistency_assessment_failure_count: u32,
    pub coverage_contract_version: String,
    pub coverage_scope: String,
    pub coverage_limitations: Vec<String>,
    pub candidate_claim_count: u32,
    pub candidate_batch_count: u32,
    pub candidate_expected_window_count: u32,
    pub candidate_processed_window_count: u32,
    pub candidate_pair_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewRun {
    pub review_run_id: String,
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub input_hash_algorithm: String,
    pub input_hash: String,
    pub execution_identity_hash_algorithm: Option<String>,
    pub execution_identity_hash: Option<String>,
    pub citation_review_run_id: Option<String>,
    pub claim_inventory_run_id: Option<String>,
    pub claim_coverage_run_id: Option<String>,
    pub citation_expectation_run_id: Option<String>,
    pub cross_claim_candidate_run_id: Option<String>,
    pub cross_claim_assessment_run_id: Option<String>,
    pub review_contract_version: String,
    pub status: ManuscriptResearchReviewRunStatus,
    pub failure_stage: Option<String>,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub summary: Option<ManuscriptResearchReviewSummary>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewClaimTarget {
    pub coverage_target_id: String,
    pub claim_citation_link_id: String,
    pub citation_occurrence_id: String,
    pub citation_target_id: String,
    pub citation_review_item_id: String,
    pub binding_id: Option<String>,
    pub source_id: Option<String>,
    pub source_snapshot_id: Option<String>,
    pub extraction_id: Option<String>,
    pub verification_run_id: Option<String>,
    pub review_status: CitationReviewItemStatus,
    pub failure_code: Option<String>,
    pub verification_status: Option<CitationVerificationStatus>,
    pub verification_failure_code: Option<String>,
    pub relation: Option<ClaimEvidenceRelation>,
    pub rationale: Option<String>,
    pub evidence_count: u32,
    pub evidence: Vec<crate::CitationReviewEvidence>,
    pub citation_review_item: CitationReviewItem,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewClaimItem {
    pub whole_review_run_id: String,
    pub inventory_item_id: String,
    pub ordinal: u32,
    pub document_block_id: String,
    pub block_ordinal: u32,
    pub block_kind: ManuscriptClaimInventoryBlockKind,
    pub source_start: u64,
    pub source_end: u64,
    pub source_excerpt: String,
    pub claim_text: String,
    pub claim_review_kind: ClaimReviewKind,
    pub bridge_status: ManuscriptClaimCoverageBridgeStatus,
    pub structural_citation_state: ManuscriptClaimCoverageStructuralCitationState,
    pub same_block_citation_count: u32,
    pub exact_claim_citation_link_count: u32,
    pub target_count: u32,
    pub assessment_status: crate::CitationExpectationAssessmentStatus,
    pub expectation: Option<crate::CitationExpectation>,
    pub expectation_rationale: Option<String>,
    pub attention_state: CoverageAttentionState,
    pub attention_reasons: Vec<CoverageAttentionReason>,
    pub support_count: u32,
    pub contradiction_count: u32,
    pub contextualize_count: u32,
    pub insufficient_count: u32,
    pub blocked_count: u32,
    pub unverified_count: u32,
    pub targets: Vec<ManuscriptResearchReviewClaimTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewConsistencyClaim {
    pub inventory_item_id: String,
    pub ordinal: u32,
    pub document_block_id: String,
    pub block_ordinal: u32,
    pub block_kind: ManuscriptClaimInventoryBlockKind,
    pub source_start: u64,
    pub source_end: u64,
    pub source_excerpt: String,
    pub claim_text: String,
    pub claim_review_kind: ClaimReviewKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptResearchReviewConsistencyItem {
    pub whole_review_run_id: String,
    pub assessment_item_id: String,
    pub candidate_id: String,
    pub left: ManuscriptResearchReviewConsistencyClaim,
    pub right: ManuscriptResearchReviewConsistencyClaim,
    pub assessment_status: CrossClaimAssessmentStatus,
    pub relation: Option<CrossClaimConsistencyRelation>,
    pub dimensions: Vec<CrossClaimDifferenceDimension>,
    pub rationale: Option<String>,
    pub failure_code: Option<String>,
    pub attention_state: CrossClaimConsistencyAttentionState,
    pub attention_reasons: Vec<CrossClaimConsistencyAttentionReason>,
}

#[derive(Debug, Error)]
pub enum ManuscriptResearchReviewError {
    #[error("manuscript research review run was not found: {0}")]
    NotFound(String),
    #[error("invalid manuscript research review request: {0}")]
    Invalid(String),
    #[error(transparent)]
    Research(#[from] ResearchError),
    #[error(transparent)]
    CitationReview(#[from] CitationReviewError),
    #[error(transparent)]
    CrossClaimCandidates(#[from] crate::CrossClaimCandidateDiscoveryError),
    #[error(transparent)]
    CrossClaimAssessment(#[from] crate::CrossClaimConsistencyAssessmentError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl ManuscriptResearchReviewError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid_request",
            Self::Research(ResearchError::NotFound { .. }) => "not_found",
            Self::Research(ResearchError::Invalid(_)) => "invalid_request",
            Self::Research(_)
            | Self::CitationReview(_)
            | Self::CrossClaimCandidates(_)
            | Self::CrossClaimAssessment(_)
            | Self::Database(_) => "internal_error",
        }
    }
}

impl CitationReviewService {
    pub async fn start_manuscript_research_review(
        &self,
        input: StartManuscriptResearchReview,
    ) -> Result<ManuscriptResearchReviewRun, ManuscriptResearchReviewError> {
        validate_input(&input)?;
        self.validate_manuscript_source(&input).await?;
        let input_hash = input_hash(&input)?;
        let execution_identity = self.execution_identity(&input_hash);
        let execution_identity_hash = execution_identity_hash(&execution_identity)?;

        if let Some(existing_id) = self
            .find_completed_review(&input_hash, &execution_identity_hash)
            .await?
        {
            let existing = self.load_review(&existing_id).await?;
            if self
                .can_reuse_completed_review(&existing, &execution_identity_hash)
                .await
            {
                return self.get_manuscript_research_review(&existing_id).await;
            }
        }

        let review_run_id = new_id();
        let created_at_ms = now_ms();
        sqlx::query(
            "INSERT INTO research_manuscript_research_review_runs
             (review_run_id, research_case_id, manuscript_source_id, document_id,
              document_version, input_hash_algorithm, input_hash,
              execution_identity_hash_algorithm, execution_identity_hash,
              review_contract_version, status, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'running', ?)",
        )
        .bind(&review_run_id)
        .bind(&input.research_case_id)
        .bind(&input.manuscript_source_id)
        .bind(&input.document_id)
        .bind(input.document_version)
        .bind(MANUSCRIPT_RESEARCH_REVIEW_EXECUTION_IDENTITY_HASH_ALGORITHM)
        .bind(&input_hash)
        .bind(MANUSCRIPT_RESEARCH_REVIEW_EXECUTION_IDENTITY_HASH_ALGORITHM)
        .bind(&execution_identity_hash)
        .bind(MANUSCRIPT_RESEARCH_REVIEW_CONTRACT_VERSION)
        .bind(created_at_ms)
        .execute(self.pool())
        .await?;

        let running = self.load_review(&review_run_id).await?;
        self.publish_manuscript_research_review_event(
            "research.manuscriptResearchReviewStarted",
            &running,
        );

        let citation_review = match self
            .start_manuscript_citation_review(StartManuscriptCitationReview {
                research_case_id: input.research_case_id.clone(),
                manuscript_source_id: input.manuscript_source_id.clone(),
                document_id: input.document_id.clone(),
                document_version: input.document_version,
                citations: input.citation_review_observations.citations.clone(),
                blocks: input.citation_review_observations.citation_blocks.clone(),
            })
            .await
        {
            Ok(value) if matches!(value.status, CitationReviewRunStatus::Completed) => value,
            Ok(value) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::CitationReview,
                        value.failure_code.as_deref().unwrap_or("stage_failed"),
                    )
                    .await;
            }
            Err(error) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::CitationReview,
                        error.code(),
                    )
                    .await;
            }
        };
        if self
            .persist_stage_id(
                &review_run_id,
                "citation_review_run_id",
                &citation_review.review_run_id,
            )
            .await
            .is_err()
        {
            return self
                .fail_review(
                    &review_run_id,
                    ManuscriptResearchReviewFailureStage::Persistence,
                    "stage_identity_persistence_failed",
                )
                .await;
        }

        let inventory = match self
            .research_service()
            .start_manuscript_claim_inventory(nineprofs_research::StartManuscriptClaimInventory {
                research_case_id: ResearchCaseId::parse(input.research_case_id.clone())
                    .map_err(ManuscriptResearchReviewError::Research)?,
                manuscript_source_id: ResearchSourceId::parse(input.manuscript_source_id.clone())
                    .map_err(ManuscriptResearchReviewError::Research)?,
                document_id: input.document_id.clone(),
                document_version: input.document_version,
                blocks: input
                    .claim_inventory_observations
                    .whole_manuscript_blocks
                    .clone(),
            })
            .await
        {
            Ok(value) if matches!(value.status, ManuscriptClaimInventoryStatus::Completed) => value,
            Ok(value) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::ClaimInventory,
                        value.failure_code.as_deref().unwrap_or("stage_failed"),
                    )
                    .await;
            }
            Err(error) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::ClaimInventory,
                        research_error_code(&error),
                    )
                    .await;
            }
        };
        if self
            .persist_stage_id(
                &review_run_id,
                "claim_inventory_run_id",
                inventory.id.as_str(),
            )
            .await
            .is_err()
        {
            return self
                .fail_review(
                    &review_run_id,
                    ManuscriptResearchReviewFailureStage::Persistence,
                    "stage_identity_persistence_failed",
                )
                .await;
        }

        let coverage = match self
            .start_manuscript_claim_coverage(StartManuscriptClaimCoverage {
                research_case_id: input.research_case_id.clone(),
                claim_inventory_run_id: inventory.id.to_string(),
                citation_review_run_id: citation_review.review_run_id.clone(),
            })
            .await
        {
            Ok(value) if matches!(value.status, ManuscriptClaimCoverageRunStatus::Completed) => {
                value
            }
            Ok(_) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::ClaimCoverage,
                        "stage_failed",
                    )
                    .await;
            }
            Err(error) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::ClaimCoverage,
                        error.code(),
                    )
                    .await;
            }
        };
        if self
            .persist_stage_id(
                &review_run_id,
                "claim_coverage_run_id",
                &coverage.coverage_run_id,
            )
            .await
            .is_err()
        {
            return self
                .fail_review(
                    &review_run_id,
                    ManuscriptResearchReviewFailureStage::Persistence,
                    "stage_identity_persistence_failed",
                )
                .await;
        }

        let expectation = match self
            .start_manuscript_citation_expectation(StartManuscriptCitationExpectation {
                research_case_id: input.research_case_id.clone(),
                claim_coverage_run_id: coverage.coverage_run_id.clone(),
            })
            .await
        {
            Ok(value)
                if matches!(
                    value.status,
                    ManuscriptCitationExpectationRunStatus::Completed
                ) =>
            {
                value
            }
            Ok(_) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::CitationExpectation,
                        "stage_failed",
                    )
                    .await;
            }
            Err(error) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::CitationExpectation,
                        error.code(),
                    )
                    .await;
            }
        };
        if self
            .persist_stage_id(
                &review_run_id,
                "citation_expectation_run_id",
                &expectation.expectation_run_id,
            )
            .await
            .is_err()
        {
            return self
                .fail_review(
                    &review_run_id,
                    ManuscriptResearchReviewFailureStage::Persistence,
                    "stage_identity_persistence_failed",
                )
                .await;
        }

        let candidate = match self
            .start_manuscript_cross_claim_candidates(StartManuscriptCrossClaimCandidates {
                research_case_id: input.research_case_id.clone(),
                claim_inventory_run_id: inventory.id.to_string(),
            })
            .await
        {
            Ok(value)
                if matches!(
                    value.status,
                    ManuscriptCrossClaimCandidateRunStatus::Completed
                ) =>
            {
                value
            }
            Ok(_) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::CrossClaimDiscovery,
                        "stage_failed",
                    )
                    .await;
            }
            Err(error) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::CrossClaimDiscovery,
                        error.code(),
                    )
                    .await;
            }
        };
        if self
            .persist_stage_id(
                &review_run_id,
                "cross_claim_candidate_run_id",
                &candidate.candidate_run_id,
            )
            .await
            .is_err()
        {
            return self
                .fail_review(
                    &review_run_id,
                    ManuscriptResearchReviewFailureStage::Persistence,
                    "stage_identity_persistence_failed",
                )
                .await;
        }

        let assessment = match self
            .start_manuscript_cross_claim_assessment(StartManuscriptCrossClaimAssessment {
                research_case_id: input.research_case_id.clone(),
                candidate_run_id: candidate.candidate_run_id.clone(),
            })
            .await
        {
            Ok(value)
                if matches!(
                    value.status,
                    ManuscriptCrossClaimAssessmentRunStatus::Completed
                ) =>
            {
                value
            }
            Ok(_) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::CrossClaimAssessment,
                        "stage_failed",
                    )
                    .await;
            }
            Err(error) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::CrossClaimAssessment,
                        error.code(),
                    )
                    .await;
            }
        };
        if self
            .persist_stage_id(
                &review_run_id,
                "cross_claim_assessment_run_id",
                &assessment.assessment_run_id,
            )
            .await
            .is_err()
        {
            return self
                .fail_review(
                    &review_run_id,
                    ManuscriptResearchReviewFailureStage::Persistence,
                    "stage_identity_persistence_failed",
                )
                .await;
        }

        let completed = self.load_review(&review_run_id).await?;
        if let Err(error) = self
            .validate_completed_children(
                &completed,
                &citation_review,
                &inventory,
                &coverage,
                &expectation,
                &candidate,
                &assessment,
            )
            .await
        {
            return self
                .fail_review(
                    &review_run_id,
                    ManuscriptResearchReviewFailureStage::Projection,
                    error.code(),
                )
                .await;
        }
        let summary = match self.build_summary(&completed).await {
            Ok(value) => value,
            Err(error) => {
                return self
                    .fail_review(
                        &review_run_id,
                        ManuscriptResearchReviewFailureStage::Projection,
                        error.code(),
                    )
                    .await;
            }
        };
        let completion = sqlx::query(
            "UPDATE research_manuscript_research_review_runs
             SET status = 'completed', completed_at_ms = ? WHERE review_run_id = ? AND status = 'running'",
        )
        .bind(now_ms())
        .bind(&review_run_id)
        .execute(self.pool())
        .await;
        if let Err(error) = completion {
            if is_completed_identity_unique_conflict(&error) {
                if let Some(winner_id) = self
                    .find_completed_review(&input_hash, &execution_identity_hash)
                    .await?
                {
                    let winner = self.load_review(&winner_id).await?;
                    if self
                        .can_reuse_completed_review(&winner, &execution_identity_hash)
                        .await
                    {
                        sqlx::query(
                            "UPDATE research_manuscript_research_review_runs
                             SET status = 'failed', failure_stage = 'persistence',
                                 failure_code = 'completed_idempotency_race_lost', completed_at_ms = ?
                             WHERE review_run_id = ? AND status = 'running'",
                        )
                        .bind(now_ms())
                        .bind(&review_run_id)
                        .execute(self.pool())
                        .await?;
                        return self.get_manuscript_research_review(&winner_id).await;
                    }
                }
            }
            return self
                .fail_review(
                    &review_run_id,
                    ManuscriptResearchReviewFailureStage::Persistence,
                    "completion_persistence_failed",
                )
                .await;
        }
        let mut completed = self.load_review(&review_run_id).await?;
        completed.summary = Some(summary);
        self.publish_manuscript_research_review_event(
            "research.manuscriptResearchReviewCompleted",
            &completed,
        );
        Ok(completed)
    }

    pub async fn get_manuscript_research_review(
        &self,
        review_run_id: &str,
    ) -> Result<ManuscriptResearchReviewRun, ManuscriptResearchReviewError> {
        let mut run = self.load_review(review_run_id).await?;
        if matches!(run.status, ManuscriptResearchReviewRunStatus::Completed) {
            run.summary = Some(self.build_summary(&run).await?);
        }
        Ok(run)
    }

    pub async fn list_manuscript_research_review_claims(
        &self,
        review_run_id: &str,
    ) -> Result<Vec<ManuscriptResearchReviewClaimItem>, ManuscriptResearchReviewError> {
        let run = self.completed_review(review_run_id).await?;
        self.project_claims(&run).await
    }

    pub async fn list_manuscript_research_review_consistency(
        &self,
        review_run_id: &str,
    ) -> Result<Vec<ManuscriptResearchReviewConsistencyItem>, ManuscriptResearchReviewError> {
        let run = self.completed_review(review_run_id).await?;
        self.project_consistency(&run).await
    }

    async fn completed_review(
        &self,
        review_run_id: &str,
    ) -> Result<ManuscriptResearchReviewRun, ManuscriptResearchReviewError> {
        let run = self.load_review(review_run_id).await?;
        if !matches!(run.status, ManuscriptResearchReviewRunStatus::Completed) {
            return Err(ManuscriptResearchReviewError::Invalid(
                "whole review is not complete".into(),
            ));
        }
        Ok(run)
    }

    async fn validate_manuscript_source(
        &self,
        input: &StartManuscriptResearchReview,
    ) -> Result<(), ManuscriptResearchReviewError> {
        let case = self
            .research_service()
            .get_case(&input.research_case_id)
            .await?;
        let source = self
            .research_service()
            .get_source(&input.manuscript_source_id)
            .await?;
        if source.research_case_id.as_str() != case.id.as_str() {
            return Err(ManuscriptResearchReviewError::Invalid(
                "manuscript source does not belong to research case".into(),
            ));
        }
        if !matches!(source.kind, SourceKind::Manuscript) {
            return Err(ManuscriptResearchReviewError::Invalid(
                "research source must have manuscript kind".into(),
            ));
        }
        Ok(())
    }

    async fn find_completed_review(
        &self,
        input_hash: &str,
        execution_identity_hash: &str,
    ) -> Result<Option<String>, ManuscriptResearchReviewError> {
        Ok(sqlx::query_scalar(
            "SELECT review_run_id FROM research_manuscript_research_review_runs
             WHERE input_hash_algorithm = 'sha256' AND input_hash = ?
               AND execution_identity_hash_algorithm = 'sha256'
               AND execution_identity_hash = ?
               AND review_contract_version = ? AND status = 'completed'
             ORDER BY completed_at_ms DESC, review_run_id DESC LIMIT 1",
        )
        .bind(input_hash)
        .bind(execution_identity_hash)
        .bind(MANUSCRIPT_RESEARCH_REVIEW_CONTRACT_VERSION)
        .fetch_optional(self.pool())
        .await?)
    }

    async fn load_review(
        &self,
        review_run_id: &str,
    ) -> Result<ManuscriptResearchReviewRun, ManuscriptResearchReviewError> {
        let row = sqlx::query(
            "SELECT review_run_id, research_case_id, manuscript_source_id, document_id,
             document_version, input_hash_algorithm, input_hash,
             execution_identity_hash_algorithm, execution_identity_hash,
             citation_review_run_id, claim_inventory_run_id,
             claim_coverage_run_id, citation_expectation_run_id,
             cross_claim_candidate_run_id, cross_claim_assessment_run_id,
             review_contract_version, status, failure_stage, failure_code,
             created_at_ms, completed_at_ms
             FROM research_manuscript_research_review_runs WHERE review_run_id = ?",
        )
        .bind(review_run_id)
        .fetch_optional(self.pool())
        .await?
        .ok_or_else(|| ManuscriptResearchReviewError::NotFound(review_run_id.into()))?;
        Ok(ManuscriptResearchReviewRun {
            review_run_id: row.get("review_run_id"),
            research_case_id: row.get("research_case_id"),
            manuscript_source_id: row.get("manuscript_source_id"),
            document_id: row.get("document_id"),
            document_version: row.get("document_version"),
            input_hash_algorithm: row.get("input_hash_algorithm"),
            input_hash: row.get("input_hash"),
            execution_identity_hash_algorithm: row.get("execution_identity_hash_algorithm"),
            execution_identity_hash: row.get("execution_identity_hash"),
            citation_review_run_id: row.get("citation_review_run_id"),
            claim_inventory_run_id: row.get("claim_inventory_run_id"),
            claim_coverage_run_id: row.get("claim_coverage_run_id"),
            citation_expectation_run_id: row.get("citation_expectation_run_id"),
            cross_claim_candidate_run_id: row.get("cross_claim_candidate_run_id"),
            cross_claim_assessment_run_id: row.get("cross_claim_assessment_run_id"),
            review_contract_version: row.get("review_contract_version"),
            status: parse_enum(row.get("status"), "whole review status")?,
            failure_stage: row.get("failure_stage"),
            failure_code: row.get("failure_code"),
            created_at_ms: row.get("created_at_ms"),
            completed_at_ms: row.get("completed_at_ms"),
            summary: None,
        })
    }

    async fn persist_stage_id(
        &self,
        review_run_id: &str,
        column: &str,
        child_run_id: &str,
    ) -> Result<(), sqlx::Error> {
        let sql = match column {
            "citation_review_run_id" => {
                "UPDATE research_manuscript_research_review_runs SET citation_review_run_id = ? WHERE review_run_id = ?"
            }
            "claim_inventory_run_id" => {
                "UPDATE research_manuscript_research_review_runs SET claim_inventory_run_id = ? WHERE review_run_id = ?"
            }
            "claim_coverage_run_id" => {
                "UPDATE research_manuscript_research_review_runs SET claim_coverage_run_id = ? WHERE review_run_id = ?"
            }
            "citation_expectation_run_id" => {
                "UPDATE research_manuscript_research_review_runs SET citation_expectation_run_id = ? WHERE review_run_id = ?"
            }
            "cross_claim_candidate_run_id" => {
                "UPDATE research_manuscript_research_review_runs SET cross_claim_candidate_run_id = ? WHERE review_run_id = ?"
            }
            "cross_claim_assessment_run_id" => {
                "UPDATE research_manuscript_research_review_runs SET cross_claim_assessment_run_id = ? WHERE review_run_id = ?"
            }
            _ => return Err(sqlx::Error::Protocol("unknown stage identity".into())),
        };
        sqlx::query(sql)
            .bind(child_run_id)
            .bind(review_run_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    async fn fail_review(
        &self,
        review_run_id: &str,
        stage: ManuscriptResearchReviewFailureStage,
        code: &str,
    ) -> Result<ManuscriptResearchReviewRun, ManuscriptResearchReviewError> {
        let bounded_code = bounded_failure_code(code);
        sqlx::query(
            "UPDATE research_manuscript_research_review_runs
             SET status = 'failed', failure_stage = ?, failure_code = ?, completed_at_ms = ?
             WHERE review_run_id = ? AND status = 'running'",
        )
        .bind(stage.as_str())
        .bind(bounded_code)
        .bind(now_ms())
        .bind(review_run_id)
        .execute(self.pool())
        .await?;
        let run = self.load_review(review_run_id).await?;
        self.publish_manuscript_research_review_event(
            "research.manuscriptResearchReviewFailed",
            &run,
        );
        Ok(run)
    }

    async fn can_reuse_completed_review(
        &self,
        run: &ManuscriptResearchReviewRun,
        execution_identity_hash: &str,
    ) -> bool {
        if run.execution_identity_hash_algorithm.as_deref()
            != Some(MANUSCRIPT_RESEARCH_REVIEW_EXECUTION_IDENTITY_HASH_ALGORITHM)
            || run.execution_identity_hash.as_deref() != Some(execution_identity_hash)
        {
            return false;
        }
        let (
            Some(citation_review_id),
            Some(inventory_id),
            Some(coverage_id),
            Some(expectation_id),
            Some(candidate_id),
            Some(assessment_id),
        ) = (
            run.citation_review_run_id.as_deref(),
            run.claim_inventory_run_id.as_deref(),
            run.claim_coverage_run_id.as_deref(),
            run.citation_expectation_run_id.as_deref(),
            run.cross_claim_candidate_run_id.as_deref(),
            run.cross_claim_assessment_run_id.as_deref(),
        )
        else {
            return false;
        };
        let Ok(citation) = self
            .get_manuscript_citation_review(citation_review_id)
            .await
        else {
            return false;
        };
        let Ok(coverage) = self.get_manuscript_claim_coverage(coverage_id).await else {
            return false;
        };
        if !matches!(citation.status, CitationReviewRunStatus::Completed)
            || !matches!(coverage.status, ManuscriptClaimCoverageRunStatus::Completed)
            || coverage.analysis_contract_version
                != MANUSCRIPT_CLAIM_COVERAGE_ANALYSIS_CONTRACT_VERSION
        {
            return false;
        }
        let (
            Some(citation_sync_id),
            Some(reference_catalog_id),
            Some(reference_resolution_id),
            Some(claim_extraction_id),
        ) = (
            citation.citation_sync_run_id.as_deref(),
            citation.reference_catalog_run_id.as_deref(),
            citation.reference_resolution_run_id.as_deref(),
            citation.claim_extraction_run_id.as_deref(),
        )
        else {
            return false;
        };
        let Ok(citation_sync) = self
            .research_service()
            .get_manuscript_citation_sync(citation_sync_id)
            .await
        else {
            return false;
        };
        let Ok(reference_catalog) = self
            .research_service()
            .get_manuscript_reference_catalog(reference_catalog_id)
            .await
        else {
            return false;
        };
        let Ok(reference_resolution) = self
            .research_service()
            .get_manuscript_reference_resolution(reference_resolution_id)
            .await
        else {
            return false;
        };
        let Ok(claim_extraction) = self
            .research_service()
            .get_manuscript_claim_extraction(claim_extraction_id)
            .await
        else {
            return false;
        };
        if !matches!(
            citation_sync.status,
            nineprofs_research::ManuscriptCitationSyncStatus::Completed
        ) || !matches!(
            reference_catalog.status,
            nineprofs_research::ManuscriptReferenceCatalogStatus::Completed
        ) || !matches!(
            reference_resolution.status,
            nineprofs_research::ManuscriptReferenceResolutionStatus::Completed
        ) || !matches!(
            claim_extraction.status,
            nineprofs_research::ManuscriptClaimExtractionStatus::Completed
        ) || citation_sync.research_case_id.as_str() != run.research_case_id
            || citation_sync.manuscript_source_id.as_str() != run.manuscript_source_id
            || citation_sync.document_id != run.document_id
            || citation_sync.document_version != run.document_version
            || reference_catalog.research_case_id.as_str() != run.research_case_id
            || reference_catalog.manuscript_source_id.as_str() != run.manuscript_source_id
            || reference_catalog.citation_sync_run_id != citation_sync.id
            || reference_catalog.document_id != run.document_id
            || reference_catalog.document_version != run.document_version
            || reference_resolution.catalog_run_id != reference_catalog.id
            || reference_resolution.resolver_policy_version != REFERENCE_RESOLVER_POLICY_VERSION
            || claim_extraction.research_case_id.as_str() != run.research_case_id
            || claim_extraction.manuscript_source_id.as_str() != run.manuscript_source_id
            || claim_extraction.citation_sync_run_id != citation_sync.id
            || claim_extraction.document_id != run.document_id
            || claim_extraction.document_version != run.document_version
        {
            return false;
        }
        let Some(claim_extractor_identity) = self.research_service().claim_extractor_identity()
        else {
            return false;
        };
        if claim_extraction.extractor_provider != claim_extractor_identity.provider
            || claim_extraction.extractor_version != claim_extractor_identity.extractor_version
            || claim_extraction.extractor_model_id != claim_extractor_identity.model_id
            || claim_extraction.extraction_contract_version
                != claim_extractor_identity.extraction_contract_version
        {
            return false;
        }
        let Ok(citation_items) = self
            .list_manuscript_citation_review_items(citation_review_id)
            .await
        else {
            return false;
        };
        if citation_items.iter().any(|item| {
            let Some(verification) = item.verification.as_ref() else {
                return false;
            };
            let has_assessor_identity = verification.assessor_provider.is_some()
                || verification.assessor_version.is_some()
                || verification.assessor_model_id.is_some();
            if verification.assessment_contract_version.as_deref()
                != Some(ASSESSMENT_CONTRACT_VERSION)
            {
                return true;
            }
            let Some(citation_identity) = self.citation_assessor_identity() else {
                return has_assessor_identity;
            };
            !has_assessor_identity
                || verification.assessor_provider.as_deref()
                    != Some(citation_identity.provider_id.as_str())
                || verification.assessor_version.as_deref()
                    != Some(citation_identity.implementation_version.as_str())
                || verification.assessor_model_id != citation_identity.model_id
        }) {
            return false;
        }
        let Ok(inventory) = self
            .research_service()
            .get_manuscript_claim_inventory(inventory_id)
            .await
        else {
            return false;
        };
        let Some(identity) = self.research_service().claim_inventory_identity() else {
            return false;
        };
        if !inventory_identity_matches(&inventory, &identity) {
            return false;
        }
        let Ok(expectation) = self
            .get_manuscript_citation_expectation(expectation_id)
            .await
        else {
            return false;
        };
        if !matches!(
            expectation.status,
            ManuscriptCitationExpectationRunStatus::Completed
        ) {
            return false;
        }
        let (provider_id, assessor_version, model_id) = self.expectation_identity();
        if expectation.expectation_contract_version
            != MANUSCRIPT_CITATION_EXPECTATION_CONTRACT_VERSION
            || expectation.provider_id != provider_id
            || expectation.assessor_version != assessor_version
            || expectation.model_id != model_id
        {
            return false;
        }
        let Ok(candidate) = self
            .get_manuscript_cross_claim_candidates_run(candidate_id)
            .await
        else {
            return false;
        };
        if !matches!(
            candidate.status,
            ManuscriptCrossClaimCandidateRunStatus::Completed
        ) {
            return false;
        }
        let (provider_id, implementation_version, model_id) = self.candidate_identity();
        if candidate.discovery_contract_version != CROSS_CLAIM_DISCOVERY_CONTRACT_VERSION
            || candidate.provider_id != provider_id
            || candidate.discovery_implementation_version != implementation_version
            || candidate.model_id != model_id
        {
            return false;
        }
        let Ok(assessment) = self
            .get_manuscript_cross_claim_assessment(assessment_id)
            .await
        else {
            return false;
        };
        if !matches!(
            assessment.status,
            ManuscriptCrossClaimAssessmentRunStatus::Completed
        ) {
            return false;
        }
        let (provider_id, implementation_version, model_id) = self.assessment_identity();
        if assessment.assessment_contract_version
            != CROSS_CLAIM_CONSISTENCY_ASSESSMENT_CONTRACT_VERSION
            || assessment.provider_id != provider_id
            || assessment.assessor_implementation_version != implementation_version
            || assessment.model_id != model_id
        {
            return false;
        }
        self.validate_completed_children(
            run,
            &citation,
            &inventory,
            &coverage,
            &expectation,
            &candidate,
            &assessment,
        )
        .await
        .is_ok()
    }

    fn execution_identity(&self, input_hash: &str) -> ManuscriptResearchReviewExecutionIdentity {
        ManuscriptResearchReviewExecutionIdentity {
            input_hash_algorithm: MANUSCRIPT_RESEARCH_REVIEW_EXECUTION_IDENTITY_HASH_ALGORITHM
                .to_owned(),
            input_hash: input_hash.to_owned(),
            citation_review: ManuscriptResearchReviewCitationExecutionIdentity {
                claim_extractor: self.research_service().claim_extractor_identity(),
                citation_assessor: self.citation_assessor_identity(),
                citation_assessment_contract_version: ASSESSMENT_CONTRACT_VERSION.to_owned(),
                reference_resolution_policy_version: REFERENCE_RESOLVER_POLICY_VERSION.to_owned(),
            },
            claim_inventory: ManuscriptResearchReviewInventoryExecutionIdentity {
                inventory_extractor: self.research_service().claim_inventory_identity(),
                coverage_contract_version: MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_CONTRACT_VERSION
                    .to_owned(),
            },
            claim_coverage_analysis_contract_version:
                MANUSCRIPT_CLAIM_COVERAGE_ANALYSIS_CONTRACT_VERSION.to_owned(),
            citation_expectation: self.expectation_provider_identity(),
            citation_expectation_contract_version: MANUSCRIPT_CITATION_EXPECTATION_CONTRACT_VERSION
                .to_owned(),
            cross_claim_candidate: self.candidate_provider_identity(),
            cross_claim_candidate_contract_version: CROSS_CLAIM_DISCOVERY_CONTRACT_VERSION
                .to_owned(),
            cross_claim_assessment: self.assessment_provider_identity(),
            cross_claim_assessment_contract_version:
                CROSS_CLAIM_CONSISTENCY_ASSESSMENT_CONTRACT_VERSION.to_owned(),
            review_contract_version: MANUSCRIPT_RESEARCH_REVIEW_CONTRACT_VERSION.to_owned(),
        }
    }

    fn expectation_provider_identity(&self) -> CitationExpectationProviderIdentity {
        self.expectation_assessor()
            .map(|provider| provider.identity())
            .unwrap_or_else(|| CitationExpectationProviderIdentity {
                provider_id: "unconfigured".into(),
                assessor_version: "unconfigured".into(),
                model_id: None,
            })
    }

    fn candidate_provider_identity(&self) -> CrossClaimCandidateDiscoveryProviderIdentity {
        self.cross_claim_candidate_provider()
            .map(|provider| provider.identity())
            .unwrap_or_else(|| CrossClaimCandidateDiscoveryProviderIdentity {
                provider_id: "unconfigured".into(),
                implementation_version: CROSS_CLAIM_DISCOVERY_IMPLEMENTATION_VERSION.into(),
                model_id: None,
            })
    }

    fn assessment_provider_identity(&self) -> CrossClaimConsistencyAssessmentProviderIdentity {
        self.cross_claim_consistency_assessor()
            .map(|provider| provider.identity())
            .unwrap_or_else(|| CrossClaimConsistencyAssessmentProviderIdentity {
                provider_id: "unconfigured".into(),
                assessor_implementation_version:
                    CROSS_CLAIM_CONSISTENCY_ASSESSMENT_IMPLEMENTATION_VERSION.into(),
                model_id: None,
            })
    }

    fn expectation_identity(&self) -> (String, String, Option<String>) {
        let identity = self.expectation_provider_identity();
        (
            identity.provider_id,
            identity.assessor_version,
            identity.model_id,
        )
    }

    fn candidate_identity(&self) -> (String, String, Option<String>) {
        let identity = self.candidate_provider_identity();
        (
            identity.provider_id,
            identity.implementation_version,
            identity.model_id,
        )
    }

    fn assessment_identity(&self) -> (String, String, Option<String>) {
        let identity = self.assessment_provider_identity();
        (
            identity.provider_id,
            identity.assessor_implementation_version,
            identity.model_id,
        )
    }

    async fn validate_completed_children(
        &self,
        whole: &ManuscriptResearchReviewRun,
        citation: &crate::CitationReviewRun,
        inventory: &nineprofs_research::ManuscriptClaimInventoryRun,
        coverage: &ManuscriptClaimCoverageRun,
        expectation: &ManuscriptCitationExpectationRun,
        candidate: &ManuscriptCrossClaimCandidateRun,
        assessment: &ManuscriptCrossClaimAssessmentRun,
    ) -> Result<(), ManuscriptResearchReviewError> {
        if whole.review_contract_version != MANUSCRIPT_RESEARCH_REVIEW_CONTRACT_VERSION
            || whole.citation_review_run_id.as_deref() != Some(citation.review_run_id.as_str())
            || whole.claim_inventory_run_id.as_deref() != Some(inventory.id.as_str())
            || whole.claim_coverage_run_id.as_deref() != Some(coverage.coverage_run_id.as_str())
            || whole.citation_expectation_run_id.as_deref()
                != Some(expectation.expectation_run_id.as_str())
            || whole.cross_claim_candidate_run_id.as_deref()
                != Some(candidate.candidate_run_id.as_str())
            || whole.cross_claim_assessment_run_id.as_deref()
                != Some(assessment.assessment_run_id.as_str())
            || citation.research_case_id != whole.research_case_id
            || citation.manuscript_source_id != whole.manuscript_source_id
            || citation.document_id != whole.document_id
            || citation.document_version != whole.document_version
            || inventory.research_case_id.as_str() != whole.research_case_id
            || inventory.manuscript_source_id.as_str() != whole.manuscript_source_id
            || inventory.document_id != whole.document_id
            || inventory.document_version != whole.document_version
            || coverage.research_case_id != whole.research_case_id
            || coverage.manuscript_source_id != whole.manuscript_source_id
            || coverage.document_id != whole.document_id
            || coverage.document_version != whole.document_version
            || coverage.claim_inventory_run_id != inventory.id.to_string()
            || coverage.citation_review_run_id != citation.review_run_id
            || expectation.research_case_id != whole.research_case_id
            || expectation.claim_coverage_run_id != coverage.coverage_run_id
            || candidate.research_case_id != whole.research_case_id
            || candidate.manuscript_source_id != whole.manuscript_source_id
            || candidate.document_id != whole.document_id
            || candidate.document_version != whole.document_version
            || candidate.claim_inventory_run_id != inventory.id.to_string()
            || assessment.research_case_id != whole.research_case_id
            || assessment.manuscript_source_id != whole.manuscript_source_id
            || assessment.document_id != whole.document_id
            || assessment.document_version != whole.document_version
            || assessment.candidate_run_id != candidate.candidate_run_id
            || assessment.claim_inventory_run_id != inventory.id.to_string()
        {
            return Err(ManuscriptResearchReviewError::Invalid(
                "pinned child histories are incompatible".into(),
            ));
        }
        Ok(())
    }

    async fn build_summary(
        &self,
        run: &ManuscriptResearchReviewRun,
    ) -> Result<ManuscriptResearchReviewSummary, ManuscriptResearchReviewError> {
        let claims = self.project_claims(run).await?;
        let consistency = self.project_consistency(run).await?;
        let coverage = self
            .get_manuscript_claim_coverage(run.claim_coverage_run_id.as_deref().unwrap())
            .await?;
        let candidate = self
            .get_manuscript_cross_claim_candidates_run(
                run.cross_claim_candidate_run_id.as_deref().unwrap(),
            )
            .await?;
        let assessment = self
            .get_manuscript_cross_claim_assessment(
                run.cross_claim_assessment_run_id.as_deref().unwrap(),
            )
            .await?;
        Ok(ManuscriptResearchReviewSummary {
            total_inventory_claims: claims.len() as u32,
            coverage_review_suggested_count: claims
                .iter()
                .filter(|item| {
                    matches!(
                        item.attention_state,
                        CoverageAttentionState::ReviewSuggested
                    )
                })
                .count() as u32,
            expectation_review_needed_count: claims
                .iter()
                .filter(|item| {
                    matches!(
                        item.attention_state,
                        CoverageAttentionState::ExpectationReviewNeeded
                    )
                })
                .count() as u32,
            assessment_unavailable_count: claims
                .iter()
                .filter(|item| {
                    matches!(
                        item.attention_state,
                        CoverageAttentionState::AssessmentUnavailable
                    )
                })
                .count() as u32,
            claims_with_support_count: claims.iter().filter(|item| item.support_count > 0).count()
                as u32,
            claims_with_contradiction_count: claims
                .iter()
                .filter(|item| item.contradiction_count > 0)
                .count() as u32,
            claims_with_blocked_verification_count: claims
                .iter()
                .filter(|item| item.blocked_count > 0)
                .count() as u32,
            claims_with_unverified_verification_count: claims
                .iter()
                .filter(|item| item.unverified_count > 0)
                .count() as u32,
            consistency_assessed_count: assessment.assessed_count,
            consistency_conflict_count: consistency
                .iter()
                .filter(|item| {
                    matches!(item.relation, Some(CrossClaimConsistencyRelation::Conflict))
                })
                .count() as u32,
            consistency_compatible_count: consistency
                .iter()
                .filter(|item| {
                    matches!(
                        item.relation,
                        Some(CrossClaimConsistencyRelation::Compatible)
                    )
                })
                .count() as u32,
            consistency_qualification_count: consistency
                .iter()
                .filter(|item| {
                    matches!(
                        item.relation,
                        Some(CrossClaimConsistencyRelation::QualificationOrRefinement)
                    )
                })
                .count() as u32,
            consistency_equivalent_count: consistency
                .iter()
                .filter(|item| {
                    matches!(
                        item.relation,
                        Some(CrossClaimConsistencyRelation::EquivalentOrRestatement)
                    )
                })
                .count() as u32,
            consistency_not_comparable_count: consistency
                .iter()
                .filter(|item| {
                    matches!(
                        item.relation,
                        Some(CrossClaimConsistencyRelation::NotMeaningfullyComparable)
                    )
                })
                .count() as u32,
            consistency_insufficient_context_count: consistency
                .iter()
                .filter(|item| {
                    matches!(
                        item.relation,
                        Some(CrossClaimConsistencyRelation::InsufficientContext)
                    )
                })
                .count() as u32,
            consistency_assessment_failure_count: assessment.failed_item_count,
            coverage_contract_version: coverage.coverage_contract_version,
            coverage_scope: coverage.coverage_scope,
            coverage_limitations: coverage.coverage_limitations,
            candidate_claim_count: candidate.claim_count,
            candidate_batch_count: candidate.batch_count,
            candidate_expected_window_count: candidate.expected_window_count,
            candidate_processed_window_count: candidate.processed_window_count,
            candidate_pair_count: candidate.candidate_pair_count,
        })
    }

    async fn project_claims(
        &self,
        run: &ManuscriptResearchReviewRun,
    ) -> Result<Vec<ManuscriptResearchReviewClaimItem>, ManuscriptResearchReviewError> {
        let inventory_run_id = run.claim_inventory_run_id.as_deref().ok_or_else(|| {
            ManuscriptResearchReviewError::Invalid("inventory stage is missing".into())
        })?;
        let coverage_run_id = run.claim_coverage_run_id.as_deref().ok_or_else(|| {
            ManuscriptResearchReviewError::Invalid("coverage stage is missing".into())
        })?;
        let expectation_run_id = run.citation_expectation_run_id.as_deref().ok_or_else(|| {
            ManuscriptResearchReviewError::Invalid("expectation stage is missing".into())
        })?;
        let citation_run_id = run.citation_review_run_id.as_deref().ok_or_else(|| {
            ManuscriptResearchReviewError::Invalid("citation stage is missing".into())
        })?;

        let inventory_items = self
            .research_service()
            .list_manuscript_claim_inventory_items(inventory_run_id)
            .await?;
        let coverage_items = self
            .list_manuscript_claim_coverage_items(coverage_run_id)
            .await?;
        let expectation_items = self
            .list_manuscript_citation_expectation_items(expectation_run_id)
            .await?;
        let citation_items = self
            .list_manuscript_citation_review_items(citation_run_id)
            .await?;
        let citation_by_id = citation_items
            .into_iter()
            .map(|item| (item.item_id.clone(), item))
            .collect::<BTreeMap<_, _>>();
        let coverage_by_inventory = coverage_items
            .iter()
            .map(|item| (item.inventory_item_id.clone(), item))
            .collect::<BTreeMap<_, _>>();
        let expectation_by_inventory = expectation_items
            .iter()
            .map(|item| (item.inventory_item_id.clone(), item))
            .collect::<BTreeMap<_, _>>();
        let mut targets_by_inventory: BTreeMap<String, Vec<ManuscriptResearchReviewClaimTarget>> =
            BTreeMap::new();
        for coverage_item in &coverage_items {
            for target in self
                .list_manuscript_claim_coverage_targets(
                    coverage_run_id,
                    &coverage_item.coverage_item_id,
                )
                .await?
            {
                let citation_review_item = citation_by_id
                    .get(&target.citation_review_item_id)
                    .cloned()
                    .ok_or_else(|| {
                        ManuscriptResearchReviewError::Invalid(
                            "coverage target is outside citation review".into(),
                        )
                    })?;
                targets_by_inventory
                    .entry(coverage_item.inventory_item_id.clone())
                    .or_default()
                    .push(project_target(target, citation_review_item));
            }
        }

        inventory_items
            .into_iter()
            .map(|item| {
                let coverage = coverage_by_inventory.get(item.id.as_str()).ok_or_else(|| {
                    ManuscriptResearchReviewError::Invalid(
                        "inventory item is missing coverage projection".into(),
                    )
                })?;
                let expectation =
                    expectation_by_inventory
                        .get(item.id.as_str())
                        .ok_or_else(|| {
                            ManuscriptResearchReviewError::Invalid(
                                "inventory item is missing expectation projection".into(),
                            )
                        })?;
                Ok(ManuscriptResearchReviewClaimItem {
                    whole_review_run_id: run.review_run_id.clone(),
                    inventory_item_id: item.id.to_string(),
                    ordinal: item.ordinal,
                    document_block_id: item.document_block_id,
                    block_ordinal: item.block_ordinal,
                    block_kind: item.block_kind,
                    source_start: item.source_start,
                    source_end: item.source_end,
                    source_excerpt: item.source_excerpt,
                    claim_text: item.claim_text,
                    claim_review_kind: item.review_kind,
                    bridge_status: coverage.bridge_status.clone(),
                    structural_citation_state: coverage.structural_citation_state.clone(),
                    same_block_citation_count: coverage.same_block_citation_count,
                    exact_claim_citation_link_count: coverage.exact_claim_citation_link_count,
                    target_count: coverage.target_count,
                    assessment_status: expectation.assessment_status.clone(),
                    expectation: expectation.expectation.clone(),
                    expectation_rationale: expectation.rationale.clone(),
                    attention_state: expectation.attention.clone(),
                    attention_reasons: expectation.attention_reasons.clone(),
                    support_count: coverage.support_count,
                    contradiction_count: coverage.contradiction_count,
                    contextualize_count: coverage.contextualize_count,
                    insufficient_count: coverage.insufficient_count,
                    blocked_count: coverage.blocked_count,
                    unverified_count: coverage.unverified_count,
                    targets: targets_by_inventory
                        .remove(item.id.as_str())
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    async fn project_consistency(
        &self,
        run: &ManuscriptResearchReviewRun,
    ) -> Result<Vec<ManuscriptResearchReviewConsistencyItem>, ManuscriptResearchReviewError> {
        let candidate_run_id = run.cross_claim_candidate_run_id.as_deref().ok_or_else(|| {
            ManuscriptResearchReviewError::Invalid("candidate stage is missing".into())
        })?;
        let assessment_run_id = run
            .cross_claim_assessment_run_id
            .as_deref()
            .ok_or_else(|| {
                ManuscriptResearchReviewError::Invalid("assessment stage is missing".into())
            })?;
        let candidates = self
            .list_manuscript_cross_claim_candidates(candidate_run_id)
            .await?;
        let assessments = self
            .list_manuscript_cross_claim_assessment_items(assessment_run_id)
            .await?;
        let candidate_by_id = candidates
            .into_iter()
            .map(|candidate| (candidate.candidate_id.clone(), candidate))
            .collect::<BTreeMap<_, _>>();
        let inventory_run_id = run.claim_inventory_run_id.as_deref().unwrap();
        let inventory_items = self
            .research_service()
            .list_manuscript_claim_inventory_items(inventory_run_id)
            .await?;
        let inventory_by_id = inventory_items
            .into_iter()
            .map(|item| (item.id.to_string(), item))
            .collect::<BTreeMap<_, _>>();

        assessments
            .into_iter()
            .map(|assessment| {
                let candidate = candidate_by_id
                    .get(&assessment.candidate_id)
                    .ok_or_else(|| {
                        ManuscriptResearchReviewError::Invalid(
                            "assessment item is outside candidate discovery".into(),
                        )
                    })?;
                let left = inventory_by_id
                    .get(&candidate.left_inventory_item_id)
                    .ok_or_else(|| {
                        ManuscriptResearchReviewError::Invalid(
                            "candidate left claim is outside inventory".into(),
                        )
                    })?;
                let right = inventory_by_id
                    .get(&candidate.right_inventory_item_id)
                    .ok_or_else(|| {
                        ManuscriptResearchReviewError::Invalid(
                            "candidate right claim is outside inventory".into(),
                        )
                    })?;
                if assessment.left_inventory_item_id != candidate.left_inventory_item_id
                    || assessment.right_inventory_item_id != candidate.right_inventory_item_id
                {
                    return Err(ManuscriptResearchReviewError::Invalid(
                        "assessment item does not match candidate pair".into(),
                    ));
                }
                Ok(ManuscriptResearchReviewConsistencyItem {
                    whole_review_run_id: run.review_run_id.clone(),
                    assessment_item_id: assessment.assessment_item_id,
                    candidate_id: assessment.candidate_id,
                    left: project_consistency_claim(left),
                    right: project_consistency_claim(right),
                    assessment_status: assessment.assessment_status,
                    relation: assessment.relation,
                    dimensions: assessment.dimensions,
                    rationale: assessment.rationale,
                    failure_code: assessment.failure_code,
                    attention_state: assessment.attention,
                    attention_reasons: assessment.attention_reasons,
                })
            })
            .collect()
    }
}

fn validate_input(
    input: &StartManuscriptResearchReview,
) -> Result<(), ManuscriptResearchReviewError> {
    if input.research_case_id.trim().is_empty()
        || input.manuscript_source_id.trim().is_empty()
        || input.document_id.trim().is_empty()
    {
        return Err(ManuscriptResearchReviewError::Invalid(
            "case, manuscript source, and document identifiers are required".into(),
        ));
    }
    if input.document_version < 0 {
        return Err(ManuscriptResearchReviewError::Invalid(
            "document version must be non-negative".into(),
        ));
    }
    Ok(())
}

fn input_hash(
    input: &StartManuscriptResearchReview,
) -> Result<String, ManuscriptResearchReviewError> {
    let bytes = serde_json::to_vec(input).map_err(|_| {
        ManuscriptResearchReviewError::Invalid("review input cannot be hashed".into())
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn execution_identity_hash(
    identity: &ManuscriptResearchReviewExecutionIdentity,
) -> Result<String, ManuscriptResearchReviewError> {
    let bytes = serde_json::to_vec(identity).map_err(|_| {
        ManuscriptResearchReviewError::Invalid("review execution identity cannot be hashed".into())
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn is_completed_identity_unique_conflict(error: &sqlx::Error) -> bool {
    let sqlx::Error::Database(database_error) = error else {
        return false;
    };
    database_error.code().as_deref() == Some("2067")
        && database_error
            .message()
            .contains("research_manuscript_research_review_runs")
        && database_error.message().contains("execution_identity_hash")
}

fn parse_enum<T: DeserializeOwned>(
    value: String,
    label: &str,
) -> Result<T, ManuscriptResearchReviewError> {
    serde_json::from_str(&format!("\"{value}\""))
        .map_err(|_| ManuscriptResearchReviewError::Invalid(format!("invalid {label}")))
}

fn bounded_failure_code(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '_' || *character == '-'
        })
        .take(96)
        .collect::<String>()
}

fn research_error_code(error: &ResearchError) -> &'static str {
    match error {
        ResearchError::NotFound { .. } => "not_found",
        ResearchError::Invalid(_) => "invalid_request",
        _ => "internal_error",
    }
}

fn inventory_identity_matches(
    run: &nineprofs_research::ManuscriptClaimInventoryRun,
    identity: &ManuscriptClaimInventoryIdentity,
) -> bool {
    run.extractor_provider == identity.provider
        && run.extractor_version == identity.extractor_version
        && run.extractor_model_id == identity.model_id
        && run.extraction_contract_version == identity.extraction_contract_version
}

fn project_target(
    target: ManuscriptClaimCoverageTarget,
    citation_review_item: CitationReviewItem,
) -> ManuscriptResearchReviewClaimTarget {
    ManuscriptResearchReviewClaimTarget {
        coverage_target_id: target.coverage_target_id,
        claim_citation_link_id: target.claim_citation_link_id,
        citation_occurrence_id: target.citation_occurrence_id,
        citation_target_id: target.citation_target_id,
        citation_review_item_id: target.citation_review_item_id,
        binding_id: target.binding_id,
        source_id: target.source_id,
        source_snapshot_id: target.source_snapshot_id,
        extraction_id: target.extraction_id,
        verification_run_id: target.verification_run_id,
        review_status: target.review_status,
        failure_code: target.failure_code,
        verification_status: target.verification_status,
        verification_failure_code: target.verification_failure_code,
        relation: target.relation,
        rationale: target.rationale,
        evidence_count: target.evidence_count,
        evidence: target.evidence,
        citation_review_item,
    }
}

fn project_consistency_claim(
    item: &ManuscriptClaimInventoryItem,
) -> ManuscriptResearchReviewConsistencyClaim {
    ManuscriptResearchReviewConsistencyClaim {
        inventory_item_id: item.id.to_string(),
        ordinal: item.ordinal,
        document_block_id: item.document_block_id.clone(),
        block_ordinal: item.block_ordinal,
        block_kind: item.block_kind.clone(),
        source_start: item.source_start,
        source_end: item.source_end,
        source_excerpt: item.source_excerpt.clone(),
        claim_text: item.claim_text.clone(),
        claim_review_kind: item.review_kind.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> ManuscriptResearchReviewExecutionIdentity {
        ManuscriptResearchReviewExecutionIdentity {
            input_hash_algorithm: "sha256".into(),
            input_hash: "input-a".into(),
            citation_review: ManuscriptResearchReviewCitationExecutionIdentity {
                claim_extractor: Some(ManuscriptClaimExtractionIdentity {
                    provider: "extractor".into(),
                    extractor_version: "extractor-v1".into(),
                    model_id: Some("extractor-model-a".into()),
                    extraction_contract_version: "extract-v1".into(),
                }),
                citation_assessor: Some(CitationAssessmentProviderIdentity {
                    provider_id: "citation-assessor".into(),
                    implementation_version: "citation-v1".into(),
                    model_id: Some("citation-model-a".into()),
                }),
                citation_assessment_contract_version: "citation-contract-v1".into(),
                reference_resolution_policy_version: "resolver-v1".into(),
            },
            claim_inventory: ManuscriptResearchReviewInventoryExecutionIdentity {
                inventory_extractor: Some(ManuscriptClaimInventoryIdentity {
                    provider: "inventory".into(),
                    extractor_version: "inventory-v1".into(),
                    model_id: Some("inventory-model-a".into()),
                    extraction_contract_version: "inventory-extract-v1".into(),
                }),
                coverage_contract_version: "inventory-coverage-v1".into(),
            },
            claim_coverage_analysis_contract_version: "coverage-analysis-v1".into(),
            citation_expectation: CitationExpectationProviderIdentity {
                provider_id: "expectation".into(),
                assessor_version: "expectation-v1".into(),
                model_id: Some("expectation-model-a".into()),
            },
            citation_expectation_contract_version: "expectation-contract-v1".into(),
            cross_claim_candidate: CrossClaimCandidateDiscoveryProviderIdentity {
                provider_id: "discovery".into(),
                implementation_version: "discovery-v1".into(),
                model_id: Some("discovery-model-a".into()),
            },
            cross_claim_candidate_contract_version: "discovery-contract-v1".into(),
            cross_claim_assessment: CrossClaimConsistencyAssessmentProviderIdentity {
                provider_id: "consistency".into(),
                assessor_implementation_version: "consistency-v1".into(),
                model_id: Some("consistency-model-a".into()),
            },
            cross_claim_assessment_contract_version: "consistency-contract-v1".into(),
            review_contract_version: "review-v1".into(),
        }
    }

    #[test]
    fn execution_identity_hash_is_stable_and_changes_for_each_semantic_surface() {
        let baseline = identity();
        let baseline_hash = execution_identity_hash(&baseline).unwrap();
        assert_eq!(baseline_hash, execution_identity_hash(&baseline).unwrap());

        let mut changed = baseline.clone();
        changed
            .citation_review
            .claim_extractor
            .as_mut()
            .unwrap()
            .model_id = Some("extractor-model-b".into());
        assert_ne!(baseline_hash, execution_identity_hash(&changed).unwrap());

        let mut changed = baseline.clone();
        changed
            .citation_review
            .citation_assessor
            .as_mut()
            .unwrap()
            .model_id = Some("citation-model-b".into());
        assert_ne!(baseline_hash, execution_identity_hash(&changed).unwrap());

        let mut changed = baseline.clone();
        changed
            .claim_inventory
            .inventory_extractor
            .as_mut()
            .unwrap()
            .model_id = Some("inventory-model-b".into());
        assert_ne!(baseline_hash, execution_identity_hash(&changed).unwrap());

        let mut changed = baseline.clone();
        changed.claim_inventory.coverage_contract_version = "inventory-coverage-v2".into();
        assert_ne!(baseline_hash, execution_identity_hash(&changed).unwrap());

        let mut changed = baseline.clone();
        changed.citation_expectation.model_id = Some("expectation-model-b".into());
        assert_ne!(baseline_hash, execution_identity_hash(&changed).unwrap());

        let mut changed = baseline.clone();
        changed.cross_claim_candidate.model_id = Some("discovery-model-b".into());
        assert_ne!(baseline_hash, execution_identity_hash(&changed).unwrap());

        let mut changed = baseline.clone();
        changed.cross_claim_assessment.model_id = Some("consistency-model-b".into());
        assert_ne!(baseline_hash, execution_identity_hash(&changed).unwrap());

        let mut changed = baseline;
        changed.review_contract_version = "review-v2".into();
        assert_ne!(baseline_hash, execution_identity_hash(&changed).unwrap());
    }
}
