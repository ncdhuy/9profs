use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use nineprofs_common::{new_id, now_ms};
use nineprofs_research::{
    ClaimReviewKind, ManuscriptClaimInventoryBlockKind, ManuscriptClaimInventoryItem,
    ManuscriptClaimInventoryStatus, ResearchError, SourceKind,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use thiserror::Error;

use super::{
    CrossClaimCandidateDiscoveryError, ManuscriptCrossClaimCandidate,
    ManuscriptCrossClaimCandidateRun, ManuscriptCrossClaimCandidateRunStatus,
};

pub const CROSS_CLAIM_CONSISTENCY_ASSESSMENT_IMPLEMENTATION_VERSION: &str =
    "model-cross-claim-consistency-assessment-v1";
pub const CROSS_CLAIM_CONSISTENCY_ASSESSMENT_CONTRACT_VERSION: &str =
    "cross-claim-consistency-assessment-v1";
pub const MAX_CROSS_CLAIM_ASSESSMENT_RATIONALE_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossClaimConsistencyRelation {
    Conflict,
    Compatible,
    QualificationOrRefinement,
    EquivalentOrRestatement,
    NotMeaningfullyComparable,
    InsufficientContext,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossClaimDifferenceDimension {
    Proposition,
    Quantitative,
    Direction,
    ModalityOrCertainty,
    CausalStrength,
    ScopeOrPopulation,
    Temporal,
    Definition,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossClaimAssessmentStatus {
    Assessed,
    AssessmentFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossClaimConsistencyAttentionState {
    NoInternalConsistencyAttentionDetected,
    ReviewSuggested,
    ContextReviewNeeded,
    AssessmentUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossClaimConsistencyAttentionReason {
    AssessedInternalConflict,
    QuantitativeConflictObserved,
    DirectionConflictObserved,
    ModalityConflictObserved,
    CausalStrengthConflictObserved,
    ScopeConflictObserved,
    TemporalConflictObserved,
    DefinitionConflictObserved,
    PropositionalConflictObserved,
    ConsistencyContextInsufficient,
    ConsistencyAssessmentFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossClaimConsistencyClaim {
    pub inventory_item_id: String,
    pub claim_text: String,
    pub source_excerpt: String,
    pub review_kind: ClaimReviewKind,
    pub block_kind: ManuscriptClaimInventoryBlockKind,
    pub block_ordinal: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossClaimConsistencyAssessmentInput {
    pub candidate_id: String,
    pub left: CrossClaimConsistencyClaim,
    pub right: CrossClaimConsistencyClaim,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossClaimConsistencyAssessment {
    pub candidate_id: String,
    pub relation: CrossClaimConsistencyRelation,
    pub dimensions: Vec<CrossClaimDifferenceDimension>,
    pub rationale: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossClaimConsistencyAssessmentProviderIdentity {
    pub provider_id: String,
    pub assessor_implementation_version: String,
    pub model_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum CrossClaimConsistencyAssessmentProviderError {
    #[error("cross-claim consistency assessor is not configured")]
    NotConfigured,
    #[error("cross-claim consistency assessor configuration is invalid")]
    InvalidConfiguration,
    #[error("cross-claim consistency assessor input is invalid")]
    InvalidInput,
    #[error("cross-claim consistency assessor input exceeded size limit")]
    InputTooLarge,
    #[error("cross-claim consistency assessor request timed out")]
    Timeout,
    #[error("cross-claim consistency assessor authorization failed")]
    Unauthorized,
    #[error("cross-claim consistency assessor rate limit exceeded")]
    RateLimited,
    #[error("cross-claim consistency assessor provider is unavailable")]
    ProviderUnavailable,
    #[error("cross-claim consistency assessor response was malformed")]
    MalformedResponse,
    #[error("cross-claim consistency assessor returned invalid structured output")]
    InvalidStructuredOutput,
    #[error("cross-claim consistency assessor response exceeded size limit")]
    ResponseTooLarge,
}

impl CrossClaimConsistencyAssessmentProviderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => "provider_not_configured",
            Self::InvalidConfiguration => "provider_configuration_invalid",
            Self::InvalidInput | Self::InputTooLarge => "provider_input_invalid",
            Self::Timeout => "provider_timeout",
            Self::Unauthorized => "provider_unauthorized",
            Self::RateLimited => "provider_rate_limited",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::MalformedResponse => "structured_output_malformed",
            Self::InvalidStructuredOutput => "structured_output_invalid",
            Self::ResponseTooLarge => "provider_response_too_large",
        }
    }
}

#[async_trait]
pub trait CrossClaimConsistencyAssessmentProvider: Send + Sync {
    fn identity(&self) -> CrossClaimConsistencyAssessmentProviderIdentity;

    async fn assess(
        &self,
        input: CrossClaimConsistencyAssessmentInput,
    ) -> Result<CrossClaimConsistencyAssessment, CrossClaimConsistencyAssessmentProviderError>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartManuscriptCrossClaimAssessment {
    pub research_case_id: String,
    pub candidate_run_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptCrossClaimAssessmentRunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCrossClaimAssessmentRun {
    pub assessment_run_id: String,
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub candidate_run_id: String,
    pub claim_inventory_run_id: String,
    pub provider_id: String,
    pub model_id: Option<String>,
    pub assessor_implementation_version: String,
    pub assessment_contract_version: String,
    pub candidate_count: u32,
    pub assessed_count: u32,
    pub failed_item_count: u32,
    pub conflict_count: u32,
    pub compatible_count: u32,
    pub qualification_count: u32,
    pub equivalent_count: u32,
    pub not_comparable_count: u32,
    pub insufficient_context_count: u32,
    pub failed_assessment_count: u32,
    pub status: ManuscriptCrossClaimAssessmentRunStatus,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCrossClaimAssessmentItem {
    pub assessment_item_id: String,
    pub assessment_run_id: String,
    pub candidate_id: String,
    pub left_inventory_item_id: String,
    pub right_inventory_item_id: String,
    pub left_ordinal: u32,
    pub right_ordinal: u32,
    pub assessment_status: CrossClaimAssessmentStatus,
    pub relation: Option<CrossClaimConsistencyRelation>,
    pub dimensions: Vec<CrossClaimDifferenceDimension>,
    pub rationale: Option<String>,
    pub failure_code: Option<String>,
    pub attention: CrossClaimConsistencyAttentionState,
    pub attention_reasons: Vec<CrossClaimConsistencyAttentionReason>,
}

#[derive(Debug, Error)]
pub enum CrossClaimConsistencyAssessmentError {
    #[error("cross-claim assessment run was not found: {0}")]
    NotFound(String),
    #[error("invalid cross-claim assessment request: {0}")]
    Invalid(String),
    #[error("cross-claim candidate discovery run is not complete")]
    CandidateRunNotCompleted,
    #[error("claim inventory is not complete")]
    InventoryNotCompleted,
    #[error("cross-claim assessment closed-set validation failed: {0}")]
    ClosedSetViolation(String),
    #[error(transparent)]
    CandidateDiscovery(#[from] CrossClaimCandidateDiscoveryError),
    #[error(transparent)]
    Research(#[from] ResearchError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl CrossClaimConsistencyAssessmentError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid_request",
            Self::CandidateRunNotCompleted => "candidate_run_not_completed",
            Self::InventoryNotCompleted => "inventory_not_completed",
            Self::ClosedSetViolation(_) => "closed_set_violation",
            Self::CandidateDiscovery(error) => error.code(),
            Self::Research(ResearchError::NotFound { .. }) => "not_found",
            Self::Research(ResearchError::Invalid(_)) => "invalid_request",
            Self::Research(_) | Self::Database(_) => "internal_error",
        }
    }
}

#[derive(Clone)]
struct PersistedAssessment {
    run: ManuscriptCrossClaimAssessmentRun,
    reused: bool,
}

impl super::CitationReviewService {
    pub async fn start_manuscript_cross_claim_assessment(
        &self,
        input: StartManuscriptCrossClaimAssessment,
    ) -> Result<ManuscriptCrossClaimAssessmentRun, CrossClaimConsistencyAssessmentError> {
        let candidate_run = self
            .get_manuscript_cross_claim_candidates_run(&input.candidate_run_id)
            .await?;
        if !matches!(
            candidate_run.status,
            ManuscriptCrossClaimCandidateRunStatus::Completed
        ) {
            return Err(CrossClaimConsistencyAssessmentError::CandidateRunNotCompleted);
        }
        let case = self
            .research_service()
            .get_case(&input.research_case_id)
            .await?;
        let source = self
            .research_service()
            .get_source(&candidate_run.manuscript_source_id)
            .await?;
        let inventory = self
            .research_service()
            .get_manuscript_claim_inventory(&candidate_run.claim_inventory_run_id)
            .await?;
        if !matches!(inventory.status, ManuscriptClaimInventoryStatus::Completed) {
            return Err(CrossClaimConsistencyAssessmentError::InventoryNotCompleted);
        }
        if candidate_run.research_case_id != input.research_case_id
            || case.id.to_string() != input.research_case_id
            || source.research_case_id != case.id
            || source.kind != SourceKind::Manuscript
            || candidate_run.manuscript_source_id != inventory.manuscript_source_id.to_string()
            || candidate_run.document_id != inventory.document_id
            || candidate_run.document_version != inventory.document_version
            || candidate_run.claim_inventory_run_id != inventory.id.to_string()
            || candidate_run.claim_count != inventory.item_count
        {
            return Err(CrossClaimConsistencyAssessmentError::Invalid(
                "candidate, source, and inventory histories are incompatible".to_owned(),
            ));
        }

        let candidates = self
            .list_manuscript_cross_claim_candidates(&candidate_run.candidate_run_id)
            .await?;
        let inventory_items = self
            .research_service()
            .list_manuscript_claim_inventory_items(&inventory.id.to_string())
            .await?;
        let inventory_by_id = inventory_items
            .into_iter()
            .map(|item| (item.id.to_string(), item))
            .collect::<BTreeMap<_, _>>();
        validate_candidate_history(&candidate_run, &inventory, &candidates, &inventory_by_id)?;

        let provider = self.cross_claim_consistency_assessor();
        let identity = provider.as_ref().map(|provider| provider.identity());
        let provider_id = identity
            .as_ref()
            .map(|value| value.provider_id.clone())
            .unwrap_or_else(|| "unconfigured".to_owned());
        let assessor_implementation_version = identity
            .as_ref()
            .map(|value| value.assessor_implementation_version.clone())
            .unwrap_or_else(|| {
                CROSS_CLAIM_CONSISTENCY_ASSESSMENT_IMPLEMENTATION_VERSION.to_owned()
            });
        let model_id = identity.and_then(|value| value.model_id);

        if let Some(existing) = self
            .find_completed_cross_claim_assessment(
                &candidate_run.candidate_run_id,
                &provider_id,
                model_id.as_deref(),
                &assessor_implementation_version,
                CROSS_CLAIM_CONSISTENCY_ASSESSMENT_CONTRACT_VERSION,
            )
            .await?
        {
            return Ok(existing);
        }

        let run = ManuscriptCrossClaimAssessmentRun {
            assessment_run_id: format!("manuscript_cross_claim_assessment_{}", new_id()),
            research_case_id: input.research_case_id,
            manuscript_source_id: candidate_run.manuscript_source_id.clone(),
            document_id: candidate_run.document_id.clone(),
            document_version: candidate_run.document_version,
            candidate_run_id: candidate_run.candidate_run_id.clone(),
            claim_inventory_run_id: candidate_run.claim_inventory_run_id.clone(),
            provider_id,
            model_id,
            assessor_implementation_version,
            assessment_contract_version: CROSS_CLAIM_CONSISTENCY_ASSESSMENT_CONTRACT_VERSION
                .to_owned(),
            candidate_count: candidates.len() as u32,
            assessed_count: 0,
            failed_item_count: 0,
            conflict_count: 0,
            compatible_count: 0,
            qualification_count: 0,
            equivalent_count: 0,
            not_comparable_count: 0,
            insufficient_context_count: 0,
            failed_assessment_count: 0,
            status: ManuscriptCrossClaimAssessmentRunStatus::Running,
            failure_code: None,
            created_at_ms: now_ms(),
            completed_at_ms: None,
        };
        if let Some(existing) = self.insert_cross_claim_assessment_run(&run).await? {
            return Ok(existing);
        }
        self.publish_cross_claim_assessment_event(
            "research.manuscriptCrossClaimAssessmentStarted",
            &run,
        );

        let mut items = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let left = inventory_by_id
                .get(&candidate.left_inventory_item_id)
                .ok_or_else(|| {
                    CrossClaimConsistencyAssessmentError::ClosedSetViolation(
                        "candidate references unknown left inventory item".to_owned(),
                    )
                })?;
            let right = inventory_by_id
                .get(&candidate.right_inventory_item_id)
                .ok_or_else(|| {
                    CrossClaimConsistencyAssessmentError::ClosedSetViolation(
                        "candidate references unknown right inventory item".to_owned(),
                    )
                })?;
            let semantic_input = CrossClaimConsistencyAssessmentInput {
                candidate_id: candidate.candidate_id.clone(),
                left: consistency_claim(left),
                right: consistency_claim(right),
            };
            let result = match provider.as_ref() {
                Some(provider) => provider.assess(semantic_input.clone()).await,
                None => Err(CrossClaimConsistencyAssessmentProviderError::NotConfigured),
            };
            let (assessment_status, relation, dimensions, rationale, failure_code) = match result {
                Ok(assessment) => match validate_assessment_output(&semantic_input, assessment) {
                    Ok(assessment) => (
                        CrossClaimAssessmentStatus::Assessed,
                        Some(assessment.relation),
                        assessment.dimensions,
                        Some(assessment.rationale),
                        None,
                    ),
                    Err(code) => (
                        CrossClaimAssessmentStatus::AssessmentFailed,
                        None,
                        Vec::new(),
                        None,
                        Some(code.to_owned()),
                    ),
                },
                Err(error) => (
                    CrossClaimAssessmentStatus::AssessmentFailed,
                    None,
                    Vec::new(),
                    None,
                    Some(error.code().to_owned()),
                ),
            };
            let (attention, attention_reasons) =
                compose_attention(assessment_status.clone(), relation.as_ref(), &dimensions);
            items.push(ManuscriptCrossClaimAssessmentItem {
                assessment_item_id: format!("manuscript_cross_claim_assessment_item_{}", new_id()),
                assessment_run_id: run.assessment_run_id.clone(),
                candidate_id: candidate.candidate_id,
                left_inventory_item_id: candidate.left_inventory_item_id,
                right_inventory_item_id: candidate.right_inventory_item_id,
                left_ordinal: candidate.left_ordinal,
                right_ordinal: candidate.right_ordinal,
                assessment_status,
                relation,
                dimensions,
                rationale,
                failure_code,
                attention,
                attention_reasons,
            });
        }

        let completed_run = summarize_run(run, &items);
        let persisted = self
            .persist_cross_claim_assessment(&completed_run, &items)
            .await?;
        if !persisted.reused {
            self.publish_cross_claim_assessment_event(
                "research.manuscriptCrossClaimAssessmentCompleted",
                &persisted.run,
            );
        }
        Ok(persisted.run)
    }

    pub async fn get_manuscript_cross_claim_assessment(
        &self,
        run_id: &str,
    ) -> Result<ManuscriptCrossClaimAssessmentRun, CrossClaimConsistencyAssessmentError> {
        self.load_cross_claim_assessment_run(run_id)
            .await?
            .ok_or_else(|| CrossClaimConsistencyAssessmentError::NotFound(run_id.to_owned()))
    }

    pub async fn list_manuscript_cross_claim_assessment_items(
        &self,
        run_id: &str,
    ) -> Result<Vec<ManuscriptCrossClaimAssessmentItem>, CrossClaimConsistencyAssessmentError> {
        let run = self.get_manuscript_cross_claim_assessment(run_id).await?;
        let rows = sqlx::query(
            "SELECT assessment_item_id, assessment_run_id, candidate_id,
             left_inventory_item_id, right_inventory_item_id, left_ordinal, right_ordinal,
             assessment_status, relation, dimensions_json, rationale, failure_code,
             attention, attention_reasons_json
             FROM research_manuscript_cross_claim_assessment_items
             WHERE assessment_run_id = ? ORDER BY left_ordinal, right_ordinal, candidate_id",
        )
        .bind(run.assessment_run_id)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(map_assessment_item).collect()
    }

    async fn insert_cross_claim_assessment_run(
        &self,
        run: &ManuscriptCrossClaimAssessmentRun,
    ) -> Result<Option<ManuscriptCrossClaimAssessmentRun>, CrossClaimConsistencyAssessmentError>
    {
        sqlx::query(
            "INSERT INTO research_manuscript_cross_claim_assessment_runs
             (assessment_run_id, research_case_id, manuscript_source_id, document_id,
              document_version, candidate_run_id, claim_inventory_run_id, provider_id, model_id,
              assessor_implementation_version, assessment_contract_version, candidate_count,
              assessed_count, failed_item_count, conflict_count, compatible_count,
              qualification_count, equivalent_count, not_comparable_count, insufficient_context_count,
              failed_assessment_count, status, failure_code, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0, 0, 0, 0, 0, 'running', NULL, ?)",
        )
        .bind(&run.assessment_run_id)
        .bind(&run.research_case_id)
        .bind(&run.manuscript_source_id)
        .bind(&run.document_id)
        .bind(run.document_version)
        .bind(&run.candidate_run_id)
        .bind(&run.claim_inventory_run_id)
        .bind(&run.provider_id)
        .bind(&run.model_id)
        .bind(&run.assessor_implementation_version)
        .bind(&run.assessment_contract_version)
        .bind(run.candidate_count)
        .bind(run.created_at_ms)
        .execute(self.pool())
        .await?;
        Ok(None)
    }

    async fn persist_cross_claim_assessment(
        &self,
        run: &ManuscriptCrossClaimAssessmentRun,
        items: &[ManuscriptCrossClaimAssessmentItem],
    ) -> Result<PersistedAssessment, CrossClaimConsistencyAssessmentError> {
        let mut tx = self.pool().begin().await?;
        for item in items {
            sqlx::query(
                "INSERT INTO research_manuscript_cross_claim_assessment_items
                 (assessment_item_id, assessment_run_id, candidate_id, left_inventory_item_id,
                  right_inventory_item_id, left_ordinal, right_ordinal, assessment_status, relation,
                  dimensions_json, rationale, failure_code, attention, attention_reasons_json)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&item.assessment_item_id)
            .bind(&item.assessment_run_id)
            .bind(&item.candidate_id)
            .bind(&item.left_inventory_item_id)
            .bind(&item.right_inventory_item_id)
            .bind(item.left_ordinal)
            .bind(item.right_ordinal)
            .bind(enum_text(&item.assessment_status))
            .bind(item.relation.as_ref().map(enum_text))
            .bind(serde_json::to_string(&item.dimensions).map_err(|error| {
                CrossClaimConsistencyAssessmentError::Invalid(format!(
                    "assessment dimensions serialization failed: {error}"
                ))
            })?)
            .bind(&item.rationale)
            .bind(&item.failure_code)
            .bind(enum_text(&item.attention))
            .bind(
                serde_json::to_string(&item.attention_reasons).map_err(|error| {
                    CrossClaimConsistencyAssessmentError::Invalid(format!(
                        "assessment attention reasons serialization failed: {error}"
                    ))
                })?,
            )
            .execute(&mut *tx)
            .await?;
        }
        let updated = sqlx::query(
            "UPDATE research_manuscript_cross_claim_assessment_runs
             SET assessed_count = ?, failed_item_count = ?, conflict_count = ?, compatible_count = ?,
                 qualification_count = ?, equivalent_count = ?, not_comparable_count = ?,
                 insufficient_context_count = ?, failed_assessment_count = ?, status = 'completed',
                 failure_code = NULL, completed_at_ms = ?
             WHERE assessment_run_id = ? AND status = 'running'",
        )
        .bind(run.assessed_count)
        .bind(run.failed_item_count)
        .bind(run.conflict_count)
        .bind(run.compatible_count)
        .bind(run.qualification_count)
        .bind(run.equivalent_count)
        .bind(run.not_comparable_count)
        .bind(run.insufficient_context_count)
        .bind(run.failed_assessment_count)
        .bind(now_ms())
        .bind(&run.assessment_run_id)
        .execute(&mut *tx)
        .await;
        let updated = match updated {
            Ok(value) => value,
            Err(error) if is_completed_identity_unique_violation(&error) => {
                tx.rollback().await?;
                self.mark_cross_claim_assessment_failed(&run.assessment_run_id)
                    .await?;
                let winner = self
                    .find_completed_cross_claim_assessment(
                        &run.candidate_run_id,
                        &run.provider_id,
                        run.model_id.as_deref(),
                        &run.assessor_implementation_version,
                        &run.assessment_contract_version,
                    )
                    .await?
                    .ok_or_else(|| {
                        CrossClaimConsistencyAssessmentError::Invalid(
                            "completed assessment identity disappeared".to_owned(),
                        )
                    })?;
                return Ok(PersistedAssessment {
                    run: winner,
                    reused: true,
                });
            }
            Err(error) => return Err(error.into()),
        };
        if updated.rows_affected() != 1 {
            return Err(CrossClaimConsistencyAssessmentError::Invalid(
                "assessment run is not running".to_owned(),
            ));
        }
        tx.commit().await?;
        Ok(PersistedAssessment {
            run: self
                .get_manuscript_cross_claim_assessment(&run.assessment_run_id)
                .await?,
            reused: false,
        })
    }

    async fn find_completed_cross_claim_assessment(
        &self,
        candidate_run_id: &str,
        provider_id: &str,
        model_id: Option<&str>,
        assessor_implementation_version: &str,
        assessment_contract_version: &str,
    ) -> Result<Option<ManuscriptCrossClaimAssessmentRun>, CrossClaimConsistencyAssessmentError>
    {
        let row = sqlx::query(
            "SELECT assessment_run_id
             FROM research_manuscript_cross_claim_assessment_runs
             WHERE candidate_run_id = ? AND provider_id = ?
               AND COALESCE(model_id, '') = COALESCE(?, '')
               AND assessor_implementation_version = ? AND assessment_contract_version = ?
               AND status = 'completed' AND failed_item_count = 0
             ORDER BY created_at_ms DESC, assessment_run_id DESC LIMIT 1",
        )
        .bind(candidate_run_id)
        .bind(provider_id)
        .bind(model_id)
        .bind(assessor_implementation_version)
        .bind(assessment_contract_version)
        .fetch_optional(self.pool())
        .await?;
        match row {
            Some(row) => {
                self.load_cross_claim_assessment_run(row.get("assessment_run_id"))
                    .await
            }
            None => Ok(None),
        }
    }

    async fn load_cross_claim_assessment_run(
        &self,
        run_id: &str,
    ) -> Result<Option<ManuscriptCrossClaimAssessmentRun>, CrossClaimConsistencyAssessmentError>
    {
        let row = sqlx::query(
            "SELECT assessment_run_id, research_case_id, manuscript_source_id, document_id,
             document_version, candidate_run_id, claim_inventory_run_id, provider_id, model_id,
             assessor_implementation_version, assessment_contract_version, candidate_count,
             assessed_count, failed_item_count, conflict_count, compatible_count,
             qualification_count, equivalent_count, not_comparable_count, insufficient_context_count,
             failed_assessment_count, status, failure_code, created_at_ms, completed_at_ms
             FROM research_manuscript_cross_claim_assessment_runs
             WHERE assessment_run_id = ?",
        )
        .bind(run_id)
        .fetch_optional(self.pool())
        .await?;
        row.map(map_assessment_run).transpose()
    }

    async fn mark_cross_claim_assessment_failed(
        &self,
        run_id: &str,
    ) -> Result<(), CrossClaimConsistencyAssessmentError> {
        sqlx::query(
            "UPDATE research_manuscript_cross_claim_assessment_runs
             SET status = 'failed', failure_code = 'duplicate_completed_identity', completed_at_ms = ?
             WHERE assessment_run_id = ? AND status = 'running'",
        )
        .bind(now_ms())
        .bind(run_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}

fn validate_candidate_history(
    run: &ManuscriptCrossClaimCandidateRun,
    inventory: &nineprofs_research::ManuscriptClaimInventoryRun,
    candidates: &[ManuscriptCrossClaimCandidate],
    inventory_by_id: &BTreeMap<String, ManuscriptClaimInventoryItem>,
) -> Result<(), CrossClaimConsistencyAssessmentError> {
    if candidates.len() != run.candidate_pair_count as usize
        || inventory_by_id.len() != inventory.item_count as usize
    {
        return Err(CrossClaimConsistencyAssessmentError::ClosedSetViolation(
            "candidate or inventory count does not match pinned history".to_owned(),
        ));
    }
    let mut candidate_ids = BTreeSet::new();
    let mut pairs = BTreeSet::new();
    for candidate in candidates {
        if candidate.candidate_run_id != run.candidate_run_id
            || !candidate_ids.insert(candidate.candidate_id.clone())
            || candidate.left_inventory_item_id == candidate.right_inventory_item_id
            || candidate.left_ordinal >= candidate.right_ordinal
            || !pairs.insert((
                candidate.left_inventory_item_id.clone(),
                candidate.right_inventory_item_id.clone(),
            ))
        {
            return Err(CrossClaimConsistencyAssessmentError::ClosedSetViolation(
                "candidate pair history is not a unique canonical set".to_owned(),
            ));
        }
        let left = inventory_by_id
            .get(&candidate.left_inventory_item_id)
            .ok_or_else(|| {
                CrossClaimConsistencyAssessmentError::ClosedSetViolation(
                    "candidate references unknown left inventory item".to_owned(),
                )
            })?;
        let right = inventory_by_id
            .get(&candidate.right_inventory_item_id)
            .ok_or_else(|| {
                CrossClaimConsistencyAssessmentError::ClosedSetViolation(
                    "candidate references unknown right inventory item".to_owned(),
                )
            })?;
        if left.inventory_run_id != inventory.id
            || right.inventory_run_id != inventory.id
            || left.ordinal != candidate.left_ordinal
            || right.ordinal != candidate.right_ordinal
        {
            return Err(CrossClaimConsistencyAssessmentError::ClosedSetViolation(
                "candidate references an outside or mismatched inventory item".to_owned(),
            ));
        }
    }
    Ok(())
}

fn consistency_claim(item: &ManuscriptClaimInventoryItem) -> CrossClaimConsistencyClaim {
    CrossClaimConsistencyClaim {
        inventory_item_id: item.id.to_string(),
        claim_text: item.claim_text.clone(),
        source_excerpt: item.source_excerpt.clone(),
        review_kind: item.review_kind.clone(),
        block_kind: item.block_kind.clone(),
        block_ordinal: item.block_ordinal,
    }
}

fn validate_assessment_output<'a>(
    input: &CrossClaimConsistencyAssessmentInput,
    assessment: CrossClaimConsistencyAssessment,
) -> Result<CrossClaimConsistencyAssessment, &'static str> {
    if assessment.candidate_id != input.candidate_id {
        return Err("closed_set_violation");
    }
    if assessment.rationale.trim().is_empty()
        || assessment.rationale.len() > MAX_CROSS_CLAIM_ASSESSMENT_RATIONALE_BYTES
    {
        return Err("structured_output_invalid");
    }
    let mut dimensions = BTreeSet::new();
    if assessment
        .dimensions
        .iter()
        .any(|dimension| !dimensions.insert(dimension))
    {
        return Err("structured_output_invalid");
    }
    Ok(assessment)
}

fn compose_attention(
    status: CrossClaimAssessmentStatus,
    relation: Option<&CrossClaimConsistencyRelation>,
    dimensions: &[CrossClaimDifferenceDimension],
) -> (
    CrossClaimConsistencyAttentionState,
    Vec<CrossClaimConsistencyAttentionReason>,
) {
    if matches!(status, CrossClaimAssessmentStatus::AssessmentFailed) {
        return (
            CrossClaimConsistencyAttentionState::AssessmentUnavailable,
            vec![CrossClaimConsistencyAttentionReason::ConsistencyAssessmentFailed],
        );
    }
    match relation {
        Some(CrossClaimConsistencyRelation::Conflict) => {
            let mut reasons = vec![CrossClaimConsistencyAttentionReason::AssessedInternalConflict];
            for dimension in dimensions {
                let reason = match dimension {
                    CrossClaimDifferenceDimension::Proposition => {
                        Some(CrossClaimConsistencyAttentionReason::PropositionalConflictObserved)
                    }
                    CrossClaimDifferenceDimension::Quantitative => {
                        Some(CrossClaimConsistencyAttentionReason::QuantitativeConflictObserved)
                    }
                    CrossClaimDifferenceDimension::Direction => {
                        Some(CrossClaimConsistencyAttentionReason::DirectionConflictObserved)
                    }
                    CrossClaimDifferenceDimension::ModalityOrCertainty => {
                        Some(CrossClaimConsistencyAttentionReason::ModalityConflictObserved)
                    }
                    CrossClaimDifferenceDimension::CausalStrength => {
                        Some(CrossClaimConsistencyAttentionReason::CausalStrengthConflictObserved)
                    }
                    CrossClaimDifferenceDimension::ScopeOrPopulation => {
                        Some(CrossClaimConsistencyAttentionReason::ScopeConflictObserved)
                    }
                    CrossClaimDifferenceDimension::Temporal => {
                        Some(CrossClaimConsistencyAttentionReason::TemporalConflictObserved)
                    }
                    CrossClaimDifferenceDimension::Definition => {
                        Some(CrossClaimConsistencyAttentionReason::DefinitionConflictObserved)
                    }
                    CrossClaimDifferenceDimension::Other => None,
                };
                if let Some(reason) = reason
                    && !reasons.contains(&reason)
                {
                    reasons.push(reason);
                }
            }
            (
                CrossClaimConsistencyAttentionState::ReviewSuggested,
                reasons,
            )
        }
        Some(CrossClaimConsistencyRelation::InsufficientContext) => (
            CrossClaimConsistencyAttentionState::ContextReviewNeeded,
            vec![CrossClaimConsistencyAttentionReason::ConsistencyContextInsufficient],
        ),
        Some(
            CrossClaimConsistencyRelation::Compatible
            | CrossClaimConsistencyRelation::QualificationOrRefinement
            | CrossClaimConsistencyRelation::EquivalentOrRestatement
            | CrossClaimConsistencyRelation::NotMeaningfullyComparable,
        ) => (
            CrossClaimConsistencyAttentionState::NoInternalConsistencyAttentionDetected,
            Vec::new(),
        ),
        None => (
            CrossClaimConsistencyAttentionState::AssessmentUnavailable,
            vec![CrossClaimConsistencyAttentionReason::ConsistencyAssessmentFailed],
        ),
    }
}

fn summarize_run(
    mut run: ManuscriptCrossClaimAssessmentRun,
    items: &[ManuscriptCrossClaimAssessmentItem],
) -> ManuscriptCrossClaimAssessmentRun {
    run.assessed_count = items
        .iter()
        .filter(|item| matches!(item.assessment_status, CrossClaimAssessmentStatus::Assessed))
        .count() as u32;
    run.failed_item_count = items
        .iter()
        .filter(|item| {
            matches!(
                item.assessment_status,
                CrossClaimAssessmentStatus::AssessmentFailed
            )
        })
        .count() as u32;
    run.failed_assessment_count = run.failed_item_count;
    for item in items {
        match item.relation.as_ref() {
            Some(CrossClaimConsistencyRelation::Conflict) => run.conflict_count += 1,
            Some(CrossClaimConsistencyRelation::Compatible) => run.compatible_count += 1,
            Some(CrossClaimConsistencyRelation::QualificationOrRefinement) => {
                run.qualification_count += 1
            }
            Some(CrossClaimConsistencyRelation::EquivalentOrRestatement) => {
                run.equivalent_count += 1
            }
            Some(CrossClaimConsistencyRelation::NotMeaningfullyComparable) => {
                run.not_comparable_count += 1
            }
            Some(CrossClaimConsistencyRelation::InsufficientContext) => {
                run.insufficient_context_count += 1
            }
            None => {}
        }
    }
    run.status = ManuscriptCrossClaimAssessmentRunStatus::Completed;
    run.completed_at_ms = Some(now_ms());
    run
}

fn is_completed_identity_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(error)
            if error.code().as_deref() == Some("2067")
                && error
                    .message()
                    .contains("uq_research_manuscript_cross_claim_assessment_completed")
    )
}

fn map_assessment_run(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptCrossClaimAssessmentRun, CrossClaimConsistencyAssessmentError> {
    Ok(ManuscriptCrossClaimAssessmentRun {
        assessment_run_id: row.get("assessment_run_id"),
        research_case_id: row.get("research_case_id"),
        manuscript_source_id: row.get("manuscript_source_id"),
        document_id: row.get("document_id"),
        document_version: row.get("document_version"),
        candidate_run_id: row.get("candidate_run_id"),
        claim_inventory_run_id: row.get("claim_inventory_run_id"),
        provider_id: row.get("provider_id"),
        model_id: row.get("model_id"),
        assessor_implementation_version: row.get("assessor_implementation_version"),
        assessment_contract_version: row.get("assessment_contract_version"),
        candidate_count: row.get::<i64, _>("candidate_count") as u32,
        assessed_count: row.get::<i64, _>("assessed_count") as u32,
        failed_item_count: row.get::<i64, _>("failed_item_count") as u32,
        conflict_count: row.get::<i64, _>("conflict_count") as u32,
        compatible_count: row.get::<i64, _>("compatible_count") as u32,
        qualification_count: row.get::<i64, _>("qualification_count") as u32,
        equivalent_count: row.get::<i64, _>("equivalent_count") as u32,
        not_comparable_count: row.get::<i64, _>("not_comparable_count") as u32,
        insufficient_context_count: row.get::<i64, _>("insufficient_context_count") as u32,
        failed_assessment_count: row.get::<i64, _>("failed_assessment_count") as u32,
        status: parse_enum(row.get("status"), "assessment run status")?,
        failure_code: row.get("failure_code"),
        created_at_ms: row.get("created_at_ms"),
        completed_at_ms: row.get("completed_at_ms"),
    })
}

fn map_assessment_item(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptCrossClaimAssessmentItem, CrossClaimConsistencyAssessmentError> {
    Ok(ManuscriptCrossClaimAssessmentItem {
        assessment_item_id: row.get("assessment_item_id"),
        assessment_run_id: row.get("assessment_run_id"),
        candidate_id: row.get("candidate_id"),
        left_inventory_item_id: row.get("left_inventory_item_id"),
        right_inventory_item_id: row.get("right_inventory_item_id"),
        left_ordinal: row.get::<i64, _>("left_ordinal") as u32,
        right_ordinal: row.get::<i64, _>("right_ordinal") as u32,
        assessment_status: parse_enum(row.get("assessment_status"), "assessment item status")?,
        relation: parse_optional_enum(row.get("relation"), "assessment relation")?,
        dimensions: parse_json(row.get("dimensions_json"), "assessment dimensions")?,
        rationale: row.get("rationale"),
        failure_code: row.get("failure_code"),
        attention: parse_enum(row.get("attention"), "assessment attention")?,
        attention_reasons: parse_json(
            row.get("attention_reasons_json"),
            "assessment attention reasons",
        )?,
    })
}

fn enum_text<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("research enum serialization")
        .as_str()
        .expect("research enum is a string")
        .to_owned()
}

fn parse_enum<T: for<'de> Deserialize<'de>>(
    value: String,
    label: &str,
) -> Result<T, CrossClaimConsistencyAssessmentError> {
    serde_json::from_str(&format!("\"{value}\""))
        .map_err(|_| CrossClaimConsistencyAssessmentError::Invalid(format!("invalid {label}")))
}

fn parse_optional_enum<T: for<'de> Deserialize<'de>>(
    value: Option<String>,
    label: &str,
) -> Result<Option<T>, CrossClaimConsistencyAssessmentError> {
    value.map(|value| parse_enum(value, label)).transpose()
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    value: String,
    label: &str,
) -> Result<T, CrossClaimConsistencyAssessmentError> {
    serde_json::from_str(&value)
        .map_err(|_| CrossClaimConsistencyAssessmentError::Invalid(format!("invalid {label}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assessment(relation: CrossClaimConsistencyRelation) -> CrossClaimConsistencyAssessment {
        CrossClaimConsistencyAssessment {
            candidate_id: "candidate-1".to_owned(),
            relation,
            dimensions: Vec::new(),
            rationale: "bounded rationale".to_owned(),
        }
    }

    fn input() -> CrossClaimConsistencyAssessmentInput {
        CrossClaimConsistencyAssessmentInput {
            candidate_id: "candidate-1".to_owned(),
            left: CrossClaimConsistencyClaim {
                inventory_item_id: "left".to_owned(),
                claim_text: "X reduces Y.".to_owned(),
                source_excerpt: "X reduces Y in this cohort.".to_owned(),
                review_kind: ClaimReviewKind::ManuscriptInternal,
                block_kind: ManuscriptClaimInventoryBlockKind::Paragraph,
                block_ordinal: 1,
            },
            right: CrossClaimConsistencyClaim {
                inventory_item_id: "right".to_owned(),
                claim_text: "X may reduce Y.".to_owned(),
                source_excerpt: "X may reduce Y in this cohort.".to_owned(),
                review_kind: ClaimReviewKind::ManuscriptInternal,
                block_kind: ManuscriptClaimInventoryBlockKind::Paragraph,
                block_ordinal: 2,
            },
        }
    }

    #[test]
    fn assessor_output_requires_exact_candidate_and_bounded_rationale() {
        let mut wrong_id = assessment(CrossClaimConsistencyRelation::Compatible);
        wrong_id.candidate_id = "other".to_owned();
        assert_eq!(
            validate_assessment_output(&input(), wrong_id),
            Err("closed_set_violation")
        );

        let mut empty = assessment(CrossClaimConsistencyRelation::Compatible);
        empty.rationale.clear();
        assert_eq!(
            validate_assessment_output(&input(), empty),
            Err("structured_output_invalid")
        );

        let mut duplicate = assessment(CrossClaimConsistencyRelation::Compatible);
        duplicate.dimensions = vec![
            CrossClaimDifferenceDimension::Quantitative,
            CrossClaimDifferenceDimension::Quantitative,
        ];
        assert_eq!(
            validate_assessment_output(&input(), duplicate),
            Err("structured_output_invalid")
        );
    }

    #[test]
    fn attention_never_infers_conflict_from_dimension() {
        for relation in [
            CrossClaimConsistencyRelation::Compatible,
            CrossClaimConsistencyRelation::QualificationOrRefinement,
            CrossClaimConsistencyRelation::EquivalentOrRestatement,
            CrossClaimConsistencyRelation::NotMeaningfullyComparable,
        ] {
            let (attention, reasons) = compose_attention(
                CrossClaimAssessmentStatus::Assessed,
                Some(&relation),
                &[CrossClaimDifferenceDimension::Quantitative],
            );
            assert_eq!(
                attention,
                CrossClaimConsistencyAttentionState::NoInternalConsistencyAttentionDetected
            );
            assert!(reasons.is_empty());
        }
    }

    #[test]
    fn relations_map_to_conservative_attention() {
        let (attention, reasons) = compose_attention(
            CrossClaimAssessmentStatus::Assessed,
            Some(&CrossClaimConsistencyRelation::Conflict),
            &[CrossClaimDifferenceDimension::Proposition],
        );
        assert_eq!(
            attention,
            CrossClaimConsistencyAttentionState::ReviewSuggested
        );
        assert!(reasons.contains(&CrossClaimConsistencyAttentionReason::AssessedInternalConflict));
        assert!(
            reasons.contains(&CrossClaimConsistencyAttentionReason::PropositionalConflictObserved)
        );

        let (attention, reasons) = compose_attention(
            CrossClaimAssessmentStatus::Assessed,
            Some(&CrossClaimConsistencyRelation::InsufficientContext),
            &[],
        );
        assert_eq!(
            attention,
            CrossClaimConsistencyAttentionState::ContextReviewNeeded
        );
        assert_eq!(
            reasons,
            vec![CrossClaimConsistencyAttentionReason::ConsistencyContextInsufficient]
        );

        let (attention, reasons) =
            compose_attention(CrossClaimAssessmentStatus::AssessmentFailed, None, &[]);
        assert_eq!(
            attention,
            CrossClaimConsistencyAttentionState::AssessmentUnavailable
        );
        assert_eq!(
            reasons,
            vec![CrossClaimConsistencyAttentionReason::ConsistencyAssessmentFailed]
        );
    }
}
