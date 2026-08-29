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

pub const CROSS_CLAIM_DISCOVERY_IMPLEMENTATION_VERSION: &str =
    "model-cross-claim-candidate-discovery-v1";
pub const CROSS_CLAIM_DISCOVERY_CONTRACT_VERSION: &str = "cross-claim-candidate-discovery-v1";

pub const MAX_CROSS_CLAIM_DISCOVERY_CLAIMS: usize = 512;
pub const MAX_CROSS_CLAIM_DISCOVERY_BATCHES: usize = 32;
pub const MAX_CROSS_CLAIM_DISCOVERY_CLAIMS_PER_BATCH: usize = 16;
pub const MAX_CROSS_CLAIM_DISCOVERY_BATCH_BYTES: usize = 96 * 1024;
pub const MAX_CROSS_CLAIM_DISCOVERY_INPUT_BYTES: usize = 256 * 1024;
pub const MAX_CROSS_CLAIM_DISCOVERY_WINDOWS: usize =
    MAX_CROSS_CLAIM_DISCOVERY_BATCHES * (MAX_CROSS_CLAIM_DISCOVERY_BATCHES + 1) / 2;
pub const MAX_CROSS_CLAIM_DISCOVERY_CANDIDATES_PER_WINDOW: usize = 64;
pub const MAX_CROSS_CLAIM_DISCOVERY_CANDIDATES: usize = 8_192;
pub const MAX_CROSS_CLAIM_DISCOVERY_RATIONALE_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptCrossClaimCandidateKind {
    PotentialDirectConflict,
    PotentialQuantitativeMismatch,
    PotentialDirectionMismatch,
    PotentialModalityMismatch,
    PotentialCausalStrengthMismatch,
    PotentialScopeMismatch,
    PotentialTemporalMismatch,
    PotentialDefinitionMismatch,
    PotentialDuplicateOrRestatement,
    OtherConsistencyCandidate,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptCrossClaimCandidateRunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptCrossClaimComparisonWindowStatus {
    Pending,
    Processed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossClaimCandidateClaim {
    pub inventory_item_id: String,
    pub claim_text: String,
    pub source_excerpt: String,
    pub review_kind: ClaimReviewKind,
    pub block_kind: ManuscriptClaimInventoryBlockKind,
    pub block_ordinal: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossClaimCandidateDiscoveryInput {
    pub comparison_window_id: String,
    pub left_batch: Vec<CrossClaimCandidateClaim>,
    pub right_batch: Vec<CrossClaimCandidateClaim>,
    pub same_batch: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossClaimCandidateDiscoveryOutput {
    pub comparison_window_id: String,
    pub candidates: Vec<CrossClaimCandidateOutput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CrossClaimCandidateOutput {
    pub left_inventory_item_id: String,
    pub right_inventory_item_id: String,
    pub candidate_kind: ManuscriptCrossClaimCandidateKind,
    pub rationale: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossClaimCandidateDiscoveryProviderIdentity {
    pub provider_id: String,
    pub implementation_version: String,
    pub model_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum CrossClaimCandidateDiscoveryProviderError {
    #[error("cross-claim candidate discovery provider is not configured")]
    NotConfigured,
    #[error("cross-claim candidate discovery provider configuration is invalid")]
    InvalidConfiguration,
    #[error("cross-claim candidate discovery input is invalid")]
    InvalidInput,
    #[error("cross-claim candidate discovery input exceeded size limit")]
    InputTooLarge,
    #[error("cross-claim candidate discovery request timed out")]
    Timeout,
    #[error("cross-claim candidate discovery authorization failed")]
    Unauthorized,
    #[error("cross-claim candidate discovery rate limit exceeded")]
    RateLimited,
    #[error("cross-claim candidate discovery provider is unavailable")]
    ProviderUnavailable,
    #[error("cross-claim candidate discovery response was malformed")]
    MalformedResponse,
    #[error("cross-claim candidate discovery returned invalid structured output")]
    InvalidStructuredOutput,
    #[error("cross-claim candidate discovery response exceeded size limit")]
    ResponseTooLarge,
}

impl CrossClaimCandidateDiscoveryProviderError {
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
pub trait CrossClaimCandidateDiscoveryProvider: Send + Sync {
    fn identity(&self) -> CrossClaimCandidateDiscoveryProviderIdentity;

    async fn discover(
        &self,
        input: CrossClaimCandidateDiscoveryInput,
    ) -> Result<CrossClaimCandidateDiscoveryOutput, CrossClaimCandidateDiscoveryProviderError>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartManuscriptCrossClaimCandidates {
    pub research_case_id: String,
    pub claim_inventory_run_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCrossClaimCandidateRun {
    pub candidate_run_id: String,
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub claim_inventory_run_id: String,
    pub provider_id: String,
    pub model_id: Option<String>,
    pub discovery_implementation_version: String,
    pub discovery_contract_version: String,
    pub claim_count: u32,
    pub batch_count: u32,
    pub expected_window_count: u32,
    pub processed_window_count: u32,
    pub candidate_pair_count: u32,
    pub status: ManuscriptCrossClaimCandidateRunStatus,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCrossClaimComparisonWindow {
    pub window_id: String,
    pub candidate_run_id: String,
    pub left_batch_ordinal: u32,
    pub right_batch_ordinal: u32,
    pub same_batch: bool,
    pub status: ManuscriptCrossClaimComparisonWindowStatus,
    pub candidate_count: u32,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCrossClaimCandidate {
    pub candidate_id: String,
    pub candidate_run_id: String,
    pub comparison_window_id: String,
    pub left_inventory_item_id: String,
    pub right_inventory_item_id: String,
    pub left_ordinal: u32,
    pub right_ordinal: u32,
    pub candidate_kinds: Vec<ManuscriptCrossClaimCandidateKind>,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossClaimBatch {
    pub ordinal: u32,
    pub claims: Vec<CrossClaimCandidateClaim>,
    ordinals: Vec<u32>,
    pub serialized_input_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossClaimComparisonWindowSpec {
    pub window_id: String,
    pub left_batch_ordinal: u32,
    pub right_batch_ordinal: u32,
    pub same_batch: bool,
}

#[derive(Debug, Error)]
pub enum CrossClaimCandidateDiscoveryError {
    #[error("cross-claim candidate discovery run was not found: {0}")]
    NotFound(String),
    #[error("invalid cross-claim candidate discovery request: {0}")]
    Invalid(String),
    #[error("claim inventory must be completed")]
    InventoryNotCompleted,
    #[error("cross-claim candidate discovery provider is not configured")]
    ProviderNotConfigured,
    #[error("cross-claim candidate discovery provider failed: {0}")]
    ProviderFailed(String),
    #[error("cross-claim candidate discovery claim count limit exceeded")]
    ClaimCountLimitExceeded,
    #[error("cross-claim candidate discovery batch count limit exceeded")]
    BatchCountLimitExceeded,
    #[error("cross-claim candidate discovery batch-pair limit exceeded")]
    BatchPairLimitExceeded,
    #[error("cross-claim candidate discovery comparison input is too large")]
    ComparisonInputTooLarge,
    #[error("cross-claim candidate discovery candidate count limit exceeded")]
    CandidateCountLimitExceeded,
    #[error("cross-claim candidate discovery closed-set validation failed: {0}")]
    ClosedSetViolation(String),
    #[error("cross-claim candidate discovery run is not complete")]
    RunNotCompleted,
    #[error(transparent)]
    Research(#[from] ResearchError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl CrossClaimCandidateDiscoveryError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid_request",
            Self::InventoryNotCompleted => "inventory_not_completed",
            Self::ProviderNotConfigured => "provider_not_configured",
            Self::ProviderFailed(_) => "provider_failed",
            Self::ClaimCountLimitExceeded => "claim_count_limit_exceeded",
            Self::BatchCountLimitExceeded => "batch_count_limit_exceeded",
            Self::BatchPairLimitExceeded => "batch_pair_limit_exceeded",
            Self::ComparisonInputTooLarge => "comparison_input_too_large",
            Self::CandidateCountLimitExceeded => "candidate_count_limit_exceeded",
            Self::ClosedSetViolation(_) => "closed_set_violation",
            Self::RunNotCompleted => "run_not_completed",
            Self::Research(_) | Self::Database(_) => "internal_error",
        }
    }
}

pub fn build_cross_claim_batches(
    items: &[ManuscriptClaimInventoryItem],
) -> Result<Vec<CrossClaimBatch>, CrossClaimCandidateDiscoveryError> {
    if items.len() > MAX_CROSS_CLAIM_DISCOVERY_CLAIMS {
        return Err(CrossClaimCandidateDiscoveryError::ClaimCountLimitExceeded);
    }
    let mut ordered = items.to_vec();
    ordered.sort_by_key(|item| item.ordinal);
    if ordered
        .windows(2)
        .any(|pair| pair[0].ordinal == pair[1].ordinal)
    {
        return Err(CrossClaimCandidateDiscoveryError::Invalid(
            "inventory claim ordinals must be unique".to_owned(),
        ));
    }
    let mut ids = BTreeSet::new();
    if ordered.iter().any(|item| !ids.insert(item.id.to_string())) {
        return Err(CrossClaimCandidateDiscoveryError::Invalid(
            "inventory claim IDs must be unique".to_owned(),
        ));
    }

    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_ordinals = Vec::new();
    for item in ordered {
        let claim = claim_input(&item);
        let claim_bytes = serde_json::to_vec(std::slice::from_ref(&claim))
            .map_err(|_| CrossClaimCandidateDiscoveryError::ComparisonInputTooLarge)?
            .len();
        if claim_bytes > MAX_CROSS_CLAIM_DISCOVERY_BATCH_BYTES {
            return Err(CrossClaimCandidateDiscoveryError::ComparisonInputTooLarge);
        }
        let mut candidate_batch = current.clone();
        candidate_batch.push(claim.clone());
        let candidate_batch_bytes = serde_json::to_vec(&candidate_batch)
            .map_err(|_| CrossClaimCandidateDiscoveryError::ComparisonInputTooLarge)?
            .len();
        if !current.is_empty()
            && (current.len() >= MAX_CROSS_CLAIM_DISCOVERY_CLAIMS_PER_BATCH
                || candidate_batch_bytes > MAX_CROSS_CLAIM_DISCOVERY_BATCH_BYTES)
        {
            batches.push(CrossClaimBatch {
                ordinal: batches.len() as u32,
                claims: current,
                ordinals: current_ordinals,
                serialized_input_bytes: serde_json::to_vec(
                    &candidate_batch[..candidate_batch.len() - 1],
                )
                .map_err(|_| CrossClaimCandidateDiscoveryError::ComparisonInputTooLarge)?
                .len(),
            });
            current = Vec::new();
            current_ordinals = Vec::new();
        }
        current.push(claim);
        current_ordinals.push(item.ordinal);
    }
    if !current.is_empty() {
        let serialized_input_bytes = serde_json::to_vec(&current)
            .map_err(|_| CrossClaimCandidateDiscoveryError::ComparisonInputTooLarge)?
            .len();
        batches.push(CrossClaimBatch {
            ordinal: batches.len() as u32,
            claims: current,
            ordinals: current_ordinals,
            serialized_input_bytes,
        });
    }
    if batches.len() > MAX_CROSS_CLAIM_DISCOVERY_BATCHES {
        return Err(CrossClaimCandidateDiscoveryError::BatchCountLimitExceeded);
    }
    build_cross_claim_comparison_windows(batches.len())?;
    Ok(batches)
}

pub fn build_cross_claim_comparison_windows(
    batch_count: usize,
) -> Result<Vec<CrossClaimComparisonWindowSpec>, CrossClaimCandidateDiscoveryError> {
    if batch_count > MAX_CROSS_CLAIM_DISCOVERY_BATCHES {
        return Err(CrossClaimCandidateDiscoveryError::BatchCountLimitExceeded);
    }
    let window_count = batch_count
        .checked_mul(batch_count + 1)
        .and_then(|value| value.checked_div(2))
        .ok_or(CrossClaimCandidateDiscoveryError::BatchPairLimitExceeded)?;
    if window_count > MAX_CROSS_CLAIM_DISCOVERY_WINDOWS {
        return Err(CrossClaimCandidateDiscoveryError::BatchPairLimitExceeded);
    }
    let mut windows = Vec::with_capacity(window_count);
    for left in 0..batch_count {
        for right in left..batch_count {
            windows.push(CrossClaimComparisonWindowSpec {
                window_id: format!("cross_claim_window_{left}_{right}"),
                left_batch_ordinal: left as u32,
                right_batch_ordinal: right as u32,
                same_batch: left == right,
            });
        }
    }
    Ok(windows)
}

pub fn eligible_cross_claim_pairs(
    window: &CrossClaimComparisonWindowSpec,
    left_batch: &CrossClaimBatch,
    right_batch: &CrossClaimBatch,
) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    if window.same_batch {
        for (index, left) in left_batch.claims.iter().enumerate() {
            for right in left_batch.claims.iter().skip(index + 1) {
                pairs.push((
                    left.inventory_item_id.clone(),
                    right.inventory_item_id.clone(),
                ));
            }
        }
    } else {
        for left in &left_batch.claims {
            for right in &right_batch.claims {
                pairs.push((
                    left.inventory_item_id.clone(),
                    right.inventory_item_id.clone(),
                ));
            }
        }
    }
    pairs
}

impl super::CitationReviewService {
    pub async fn start_manuscript_cross_claim_candidates(
        &self,
        input: StartManuscriptCrossClaimCandidates,
    ) -> Result<ManuscriptCrossClaimCandidateRun, CrossClaimCandidateDiscoveryError> {
        let inventory = self
            .research_service()
            .get_manuscript_claim_inventory(&input.claim_inventory_run_id)
            .await?;
        let case = self
            .research_service()
            .get_case(&input.research_case_id)
            .await?;
        let source = self
            .research_service()
            .get_source(&inventory.manuscript_source_id.to_string())
            .await?;
        if inventory.research_case_id.to_string() != input.research_case_id
            || case.id != inventory.research_case_id
            || source.research_case_id != case.id
            || source.kind != SourceKind::Manuscript
        {
            return Err(CrossClaimCandidateDiscoveryError::Invalid(
                "claim inventory and manuscript source must belong to supplied case".to_owned(),
            ));
        }
        if !matches!(inventory.status, ManuscriptClaimInventoryStatus::Completed) {
            return Err(CrossClaimCandidateDiscoveryError::InventoryNotCompleted);
        }

        let items = self
            .research_service()
            .list_manuscript_claim_inventory_items(&inventory.id.to_string())
            .await?;
        validate_inventory_items(&inventory, &items)?;
        let batches = build_cross_claim_batches(&items)?;
        let windows = build_cross_claim_comparison_windows(batches.len())?;
        let provider = self.cross_claim_candidate_provider();
        let identity = provider.as_ref().map(|provider| provider.identity());
        let provider_id = identity
            .as_ref()
            .map(|identity| identity.provider_id.clone())
            .unwrap_or_else(|| "unconfigured".to_owned());
        let implementation_version = identity
            .as_ref()
            .map(|identity| identity.implementation_version.clone())
            .unwrap_or_else(|| CROSS_CLAIM_DISCOVERY_IMPLEMENTATION_VERSION.to_owned());
        let model_id = identity.and_then(|identity| identity.model_id);

        if let Some(existing) = self
            .find_completed_cross_claim_candidate_run(
                &inventory.id.to_string(),
                &provider_id,
                model_id.as_deref(),
                &implementation_version,
                CROSS_CLAIM_DISCOVERY_CONTRACT_VERSION,
            )
            .await?
        {
            return Ok(existing);
        }

        let run = ManuscriptCrossClaimCandidateRun {
            candidate_run_id: format!("manuscript_cross_claim_candidates_{}", new_id()),
            research_case_id: input.research_case_id,
            manuscript_source_id: inventory.manuscript_source_id.to_string(),
            document_id: inventory.document_id,
            document_version: inventory.document_version,
            claim_inventory_run_id: inventory.id.to_string(),
            provider_id,
            model_id,
            discovery_implementation_version: implementation_version,
            discovery_contract_version: CROSS_CLAIM_DISCOVERY_CONTRACT_VERSION.to_owned(),
            claim_count: items.len() as u32,
            batch_count: batches.len() as u32,
            expected_window_count: windows.len() as u32,
            processed_window_count: 0,
            candidate_pair_count: 0,
            status: ManuscriptCrossClaimCandidateRunStatus::Running,
            failure_code: None,
            created_at_ms: now_ms(),
            completed_at_ms: None,
        };
        if let Some(existing) = self
            .insert_cross_claim_candidate_run(&run, &windows)
            .await?
        {
            return Ok(existing);
        }
        self.publish_cross_claim_event(
            "research.manuscriptCrossClaimCandidateDiscoveryStarted",
            &run,
        );

        let Some(provider) = provider else {
            let error = self
                .fail_cross_claim_run(&run, None, "provider_not_configured")
                .await?;
            let failed_run = self
                .load_cross_claim_candidate_run(&run.candidate_run_id)
                .await?
                .unwrap_or(run);
            self.publish_cross_claim_event(
                "research.manuscriptCrossClaimCandidateDiscoveryFailed",
                &failed_run,
            );
            return Err(error);
        };

        let own_run_id = run.candidate_run_id.clone();
        let result = self
            .execute_cross_claim_windows(&run, &batches, &windows, provider.as_ref())
            .await;
        match result {
            Ok(run) => {
                if run.candidate_run_id == own_run_id {
                    self.publish_cross_claim_event(
                        "research.manuscriptCrossClaimCandidateDiscoveryCompleted",
                        &run,
                    );
                }
                Ok(run)
            }
            Err(error) => {
                let failed_run = match self
                    .load_cross_claim_candidate_run(&run.candidate_run_id)
                    .await
                {
                    Ok(Some(value)) => value,
                    _ => run.clone(),
                };
                self.publish_cross_claim_event(
                    "research.manuscriptCrossClaimCandidateDiscoveryFailed",
                    &failed_run,
                );
                Err(error)
            }
        }
    }

    pub async fn get_manuscript_cross_claim_candidates_run(
        &self,
        run_id: &str,
    ) -> Result<ManuscriptCrossClaimCandidateRun, CrossClaimCandidateDiscoveryError> {
        self.load_cross_claim_candidate_run(run_id)
            .await?
            .ok_or_else(|| CrossClaimCandidateDiscoveryError::NotFound(run_id.to_owned()))
    }

    pub async fn list_manuscript_cross_claim_candidate_windows(
        &self,
        run_id: &str,
    ) -> Result<Vec<ManuscriptCrossClaimComparisonWindow>, CrossClaimCandidateDiscoveryError> {
        let run = self
            .get_manuscript_cross_claim_candidates_run(run_id)
            .await?;
        self.list_cross_claim_windows(&run.candidate_run_id).await
    }

    pub async fn list_manuscript_cross_claim_candidates(
        &self,
        run_id: &str,
    ) -> Result<Vec<ManuscriptCrossClaimCandidate>, CrossClaimCandidateDiscoveryError> {
        let run = self
            .get_manuscript_cross_claim_candidates_run(run_id)
            .await?;
        if !matches!(
            run.status,
            ManuscriptCrossClaimCandidateRunStatus::Completed
        ) {
            return Err(CrossClaimCandidateDiscoveryError::RunNotCompleted);
        }
        self.list_cross_claim_candidates(&run.candidate_run_id)
            .await
    }

    async fn execute_cross_claim_windows(
        &self,
        run: &ManuscriptCrossClaimCandidateRun,
        batches: &[CrossClaimBatch],
        windows: &[CrossClaimComparisonWindowSpec],
        provider: &dyn CrossClaimCandidateDiscoveryProvider,
    ) -> Result<ManuscriptCrossClaimCandidateRun, CrossClaimCandidateDiscoveryError> {
        let mut total_candidates = run.candidate_pair_count as usize;
        for window in windows {
            let left_batch = &batches[window.left_batch_ordinal as usize];
            let right_batch = &batches[window.right_batch_ordinal as usize];
            let input = CrossClaimCandidateDiscoveryInput {
                comparison_window_id: window.window_id.clone(),
                left_batch: left_batch.claims.clone(),
                right_batch: right_batch.claims.clone(),
                same_batch: window.same_batch,
            };
            let input_bytes = serde_json::to_vec(&input)
                .map_err(|_| CrossClaimCandidateDiscoveryError::ComparisonInputTooLarge)?
                .len();
            if input_bytes > MAX_CROSS_CLAIM_DISCOVERY_INPUT_BYTES {
                return Err(self
                    .fail_cross_claim_window(run, window, "comparison_input_too_large")
                    .await?);
            }
            let output = match provider.discover(input).await {
                Ok(output) => output,
                Err(error) => {
                    return Err(self
                        .fail_cross_claim_window(run, window, error.code())
                        .await?);
                }
            };
            let candidates =
                match validate_cross_claim_output(window, left_batch, right_batch, output) {
                    Ok(candidates) => candidates,
                    Err(error) => {
                        return Err(self
                            .fail_cross_claim_window(run, window, error.code())
                            .await?);
                    }
                };
            total_candidates += candidates.len();
            if total_candidates > MAX_CROSS_CLAIM_DISCOVERY_CANDIDATES {
                return Err(self
                    .fail_cross_claim_window(run, window, "candidate_count_limit_exceeded")
                    .await?);
            }
            if let Err(error) = self
                .persist_processed_cross_claim_window(run, window, &candidates)
                .await
            {
                let _ = self
                    .fail_cross_claim_window(run, window, "persistence_failed")
                    .await;
                return Err(error);
            }
        }
        match self.complete_cross_claim_run(run).await {
            Ok(run) => Ok(run),
            Err(error) => {
                let _ = self
                    .fail_cross_claim_run(run, None, "completion_failed")
                    .await;
                Err(error)
            }
        }
    }

    async fn fail_cross_claim_window(
        &self,
        run: &ManuscriptCrossClaimCandidateRun,
        window: &CrossClaimComparisonWindowSpec,
        code: &str,
    ) -> Result<CrossClaimCandidateDiscoveryError, CrossClaimCandidateDiscoveryError> {
        sqlx::query(
            "UPDATE research_manuscript_cross_claim_comparison_windows
             SET status = 'failed', failure_code = ? WHERE window_id = ? AND candidate_run_id = ?",
        )
        .bind(code)
        .bind(&window.window_id)
        .bind(&run.candidate_run_id)
        .execute(self.pool())
        .await?;
        self.fail_cross_claim_run(run, Some(&window.window_id), code)
            .await
    }

    async fn fail_cross_claim_run(
        &self,
        run: &ManuscriptCrossClaimCandidateRun,
        _window_id: Option<&str>,
        code: &str,
    ) -> Result<CrossClaimCandidateDiscoveryError, CrossClaimCandidateDiscoveryError> {
        sqlx::query(
            "UPDATE research_manuscript_cross_claim_candidate_runs
             SET status = 'failed', failure_code = ?, completed_at_ms = ?
             WHERE candidate_run_id = ?",
        )
        .bind(code)
        .bind(now_ms())
        .bind(&run.candidate_run_id)
        .execute(self.pool())
        .await?;
        Ok(match code {
            "provider_not_configured" => CrossClaimCandidateDiscoveryError::ProviderNotConfigured,
            "comparison_input_too_large" => {
                CrossClaimCandidateDiscoveryError::ComparisonInputTooLarge
            }
            "candidate_count_limit_exceeded" => {
                CrossClaimCandidateDiscoveryError::CandidateCountLimitExceeded
            }
            value if value.starts_with("provider_") || value.starts_with("structured_output_") => {
                CrossClaimCandidateDiscoveryError::ProviderFailed(code.to_owned())
            }
            "closed_set_violation" => CrossClaimCandidateDiscoveryError::ClosedSetViolation(
                "provider returned an ineligible candidate".to_owned(),
            ),
            _ => CrossClaimCandidateDiscoveryError::ProviderFailed(code.to_owned()),
        })
    }

    async fn insert_cross_claim_candidate_run(
        &self,
        run: &ManuscriptCrossClaimCandidateRun,
        windows: &[CrossClaimComparisonWindowSpec],
    ) -> Result<Option<ManuscriptCrossClaimCandidateRun>, CrossClaimCandidateDiscoveryError> {
        let mut transaction = self.pool().begin().await?;
        let result = sqlx::query(
            "INSERT INTO research_manuscript_cross_claim_candidate_runs
             (candidate_run_id, research_case_id, manuscript_source_id, document_id, document_version,
              claim_inventory_run_id, provider_id, model_id, discovery_implementation_version,
              discovery_contract_version, claim_count, batch_count, expected_window_count,
              processed_window_count, candidate_pair_count, status, failure_code, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 'running', NULL, ?)",
        )
        .bind(&run.candidate_run_id)
        .bind(&run.research_case_id)
        .bind(&run.manuscript_source_id)
        .bind(&run.document_id)
        .bind(run.document_version)
        .bind(&run.claim_inventory_run_id)
        .bind(&run.provider_id)
        .bind(&run.model_id)
        .bind(&run.discovery_implementation_version)
        .bind(&run.discovery_contract_version)
        .bind(run.claim_count)
        .bind(run.batch_count)
        .bind(run.expected_window_count)
        .bind(run.created_at_ms)
        .execute(&mut *transaction)
        .await;
        if let Err(error) = result {
            if is_completed_identity_unique_violation(&error) {
                drop(transaction);
                return self
                    .find_completed_cross_claim_candidate_run(
                        &run.claim_inventory_run_id,
                        &run.provider_id,
                        run.model_id.as_deref(),
                        &run.discovery_implementation_version,
                        &run.discovery_contract_version,
                    )
                    .await;
            }
            return Err(error.into());
        }
        for window in windows {
            sqlx::query(
                "INSERT INTO research_manuscript_cross_claim_comparison_windows
                 (window_id, candidate_run_id, left_batch_ordinal, right_batch_ordinal, same_batch,
                  status, candidate_count)
                 VALUES (?, ?, ?, ?, ?, 'pending', 0)",
            )
            .bind(&window.window_id)
            .bind(&run.candidate_run_id)
            .bind(window.left_batch_ordinal)
            .bind(window.right_batch_ordinal)
            .bind(window.same_batch as i64)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(None)
    }

    async fn persist_processed_cross_claim_window(
        &self,
        run: &ManuscriptCrossClaimCandidateRun,
        window: &CrossClaimComparisonWindowSpec,
        candidates: &[ValidatedCrossClaimCandidate],
    ) -> Result<(), CrossClaimCandidateDiscoveryError> {
        let mut transaction = self.pool().begin().await?;
        for candidate in candidates {
            sqlx::query(
                "INSERT INTO research_manuscript_cross_claim_candidates
                 (candidate_id, candidate_run_id, comparison_window_id, left_inventory_item_id,
                  right_inventory_item_id, left_ordinal, right_ordinal, candidate_kinds_json, rationale)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("manuscript_cross_claim_candidate_{}", new_id()))
            .bind(&run.candidate_run_id)
            .bind(&window.window_id)
            .bind(&candidate.left_inventory_item_id)
            .bind(&candidate.right_inventory_item_id)
            .bind(candidate.left_ordinal)
            .bind(candidate.right_ordinal)
            .bind(serde_json::to_string(&candidate.candidate_kinds).map_err(|_| {
                CrossClaimCandidateDiscoveryError::Invalid(
                    "candidate kinds serialization failed".to_owned(),
                )
            })?)
            .bind(&candidate.rationale)
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            "UPDATE research_manuscript_cross_claim_comparison_windows
             SET status = 'processed', candidate_count = ?, failure_code = NULL
             WHERE window_id = ? AND candidate_run_id = ?",
        )
        .bind(candidates.len() as u32)
        .bind(&window.window_id)
        .bind(&run.candidate_run_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE research_manuscript_cross_claim_candidate_runs
             SET processed_window_count = processed_window_count + 1,
                 candidate_pair_count = candidate_pair_count + ?
             WHERE candidate_run_id = ?",
        )
        .bind(candidates.len() as u32)
        .bind(&run.candidate_run_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn complete_cross_claim_run(
        &self,
        run: &ManuscriptCrossClaimCandidateRun,
    ) -> Result<ManuscriptCrossClaimCandidateRun, CrossClaimCandidateDiscoveryError> {
        let result = sqlx::query(
            "UPDATE research_manuscript_cross_claim_candidate_runs
             SET status = 'completed', completed_at_ms = ?, failure_code = NULL
             WHERE candidate_run_id = ? AND processed_window_count = expected_window_count",
        )
        .bind(now_ms())
        .bind(&run.candidate_run_id)
        .execute(self.pool())
        .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if is_completed_identity_unique_violation(&error) {
                    sqlx::query(
                        "UPDATE research_manuscript_cross_claim_candidate_runs
                         SET status = 'failed', failure_code = 'duplicate_completed_identity', completed_at_ms = ?
                         WHERE candidate_run_id = ? AND status = 'running'",
                    )
                    .bind(now_ms())
                    .bind(&run.candidate_run_id)
                    .execute(self.pool())
                    .await?;
                    return self
                        .find_completed_cross_claim_candidate_run(
                            &run.claim_inventory_run_id,
                            &run.provider_id,
                            run.model_id.as_deref(),
                            &run.discovery_implementation_version,
                            &run.discovery_contract_version,
                        )
                        .await?
                        .ok_or_else(|| {
                            CrossClaimCandidateDiscoveryError::Invalid(
                                "completed candidate run identity disappeared".to_owned(),
                            )
                        });
                }
                return Err(error.into());
            }
        };
        if result.rows_affected() != 1 {
            return Err(CrossClaimCandidateDiscoveryError::Invalid(
                "completed candidate run does not have full window coverage".to_owned(),
            ));
        }
        self.get_manuscript_cross_claim_candidates_run(&run.candidate_run_id)
            .await
    }

    async fn load_cross_claim_candidate_run(
        &self,
        run_id: &str,
    ) -> Result<Option<ManuscriptCrossClaimCandidateRun>, CrossClaimCandidateDiscoveryError> {
        let row = sqlx::query(
            "SELECT candidate_run_id, research_case_id, manuscript_source_id, document_id,
             document_version, claim_inventory_run_id, provider_id, model_id,
             discovery_implementation_version, discovery_contract_version, claim_count, batch_count,
             expected_window_count, processed_window_count, candidate_pair_count, status,
             failure_code, created_at_ms, completed_at_ms
             FROM research_manuscript_cross_claim_candidate_runs WHERE candidate_run_id = ?",
        )
        .bind(run_id)
        .fetch_optional(self.pool())
        .await?;
        row.map(map_cross_claim_candidate_run).transpose()
    }

    async fn find_completed_cross_claim_candidate_run(
        &self,
        inventory_run_id: &str,
        provider_id: &str,
        model_id: Option<&str>,
        implementation_version: &str,
        contract_version: &str,
    ) -> Result<Option<ManuscriptCrossClaimCandidateRun>, CrossClaimCandidateDiscoveryError> {
        let row = sqlx::query(
            "SELECT candidate_run_id, research_case_id, manuscript_source_id, document_id,
             document_version, claim_inventory_run_id, provider_id, model_id,
             discovery_implementation_version, discovery_contract_version, claim_count, batch_count,
             expected_window_count, processed_window_count, candidate_pair_count, status,
             failure_code, created_at_ms, completed_at_ms
             FROM research_manuscript_cross_claim_candidate_runs
             WHERE claim_inventory_run_id = ? AND provider_id = ? AND model_id IS ?
             AND discovery_implementation_version = ? AND discovery_contract_version = ?
             AND status = 'completed' ORDER BY created_at_ms DESC, candidate_run_id DESC LIMIT 1",
        )
        .bind(inventory_run_id)
        .bind(provider_id)
        .bind(model_id)
        .bind(implementation_version)
        .bind(contract_version)
        .fetch_optional(self.pool())
        .await?;
        row.map(map_cross_claim_candidate_run).transpose()
    }

    async fn list_cross_claim_windows(
        &self,
        run_id: &str,
    ) -> Result<Vec<ManuscriptCrossClaimComparisonWindow>, CrossClaimCandidateDiscoveryError> {
        let rows = sqlx::query(
            "SELECT window_id, candidate_run_id, left_batch_ordinal, right_batch_ordinal,
             same_batch, status, candidate_count, failure_code
             FROM research_manuscript_cross_claim_comparison_windows
             WHERE candidate_run_id = ? ORDER BY left_batch_ordinal, right_batch_ordinal",
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(map_cross_claim_window).collect()
    }

    async fn list_cross_claim_candidates(
        &self,
        run_id: &str,
    ) -> Result<Vec<ManuscriptCrossClaimCandidate>, CrossClaimCandidateDiscoveryError> {
        let rows = sqlx::query(
            "SELECT candidate_id, candidate_run_id, comparison_window_id,
             left_inventory_item_id, right_inventory_item_id, left_ordinal, right_ordinal,
             candidate_kinds_json, rationale
             FROM research_manuscript_cross_claim_candidates
             WHERE candidate_run_id = ? ORDER BY left_ordinal, right_ordinal, candidate_id",
        )
        .bind(run_id)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(map_cross_claim_candidate).collect()
    }
}

#[derive(Clone, Debug)]
struct ValidatedCrossClaimCandidate {
    left_inventory_item_id: String,
    right_inventory_item_id: String,
    left_ordinal: u32,
    right_ordinal: u32,
    candidate_kinds: Vec<ManuscriptCrossClaimCandidateKind>,
    rationale: String,
}

fn claim_input(item: &ManuscriptClaimInventoryItem) -> CrossClaimCandidateClaim {
    CrossClaimCandidateClaim {
        inventory_item_id: item.id.to_string(),
        claim_text: item.claim_text.clone(),
        source_excerpt: item.source_excerpt.clone(),
        review_kind: item.review_kind.clone(),
        block_kind: item.block_kind.clone(),
        block_ordinal: item.block_ordinal,
    }
}

fn validate_inventory_items(
    inventory: &nineprofs_research::ManuscriptClaimInventoryRun,
    items: &[ManuscriptClaimInventoryItem],
) -> Result<(), CrossClaimCandidateDiscoveryError> {
    if items.len() != inventory.item_count as usize {
        return Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(
            "inventory item count does not match completed run".to_owned(),
        ));
    }
    let mut ids = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    for item in items {
        if item.inventory_run_id != inventory.id
            || !ids.insert(item.id.to_string())
            || !ordinals.insert(item.ordinal)
        {
            return Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(
                "inventory contains an outside-history or duplicate item".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_cross_claim_output(
    window: &CrossClaimComparisonWindowSpec,
    left_batch: &CrossClaimBatch,
    right_batch: &CrossClaimBatch,
    output: CrossClaimCandidateDiscoveryOutput,
) -> Result<Vec<ValidatedCrossClaimCandidate>, CrossClaimCandidateDiscoveryError> {
    if output.comparison_window_id != window.window_id {
        return Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(
            "comparison window ID does not match request".to_owned(),
        ));
    }
    if output.candidates.len() > MAX_CROSS_CLAIM_DISCOVERY_CANDIDATES_PER_WINDOW {
        return Err(CrossClaimCandidateDiscoveryError::CandidateCountLimitExceeded);
    }
    let mut known = BTreeMap::new();
    for claim in &left_batch.claims {
        known.insert(claim.inventory_item_id.clone(), (claim, true));
    }
    for claim in &right_batch.claims {
        known.insert(claim.inventory_item_id.clone(), (claim, false));
    }
    let mut grouped: BTreeMap<
        (String, String),
        (
            u32,
            u32,
            BTreeSet<ManuscriptCrossClaimCandidateKind>,
            String,
        ),
    > = BTreeMap::new();
    for candidate in output.candidates {
        let left = known
            .get(&candidate.left_inventory_item_id)
            .ok_or_else(|| {
                CrossClaimCandidateDiscoveryError::ClosedSetViolation(
                    "provider returned unknown left inventory item ID".to_owned(),
                )
            })?;
        let right = known
            .get(&candidate.right_inventory_item_id)
            .ok_or_else(|| {
                CrossClaimCandidateDiscoveryError::ClosedSetViolation(
                    "provider returned unknown right inventory item ID".to_owned(),
                )
            })?;
        if candidate.left_inventory_item_id == candidate.right_inventory_item_id {
            return Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(
                "provider returned a self-pair".to_owned(),
            ));
        }
        if (window.same_batch && left.1 != right.1) || (!window.same_batch && left.1 == right.1) {
            return Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(
                "provider returned a pair not eligible in this window".to_owned(),
            ));
        }
        let left_item_ordinal = claim_ordinal(left.0, left_batch, right_batch);
        let right_item_ordinal = claim_ordinal(right.0, left_batch, right_batch);
        let (left_id, right_id, left_ordinal, right_ordinal) =
            if left_item_ordinal < right_item_ordinal {
                (
                    left.0.inventory_item_id.clone(),
                    right.0.inventory_item_id.clone(),
                    left_item_ordinal,
                    right_item_ordinal,
                )
            } else {
                (
                    right.0.inventory_item_id.clone(),
                    left.0.inventory_item_id.clone(),
                    right_item_ordinal,
                    left_item_ordinal,
                )
            };
        if left_ordinal >= right_ordinal {
            return Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(
                "canonical pair ordinals are not increasing".to_owned(),
            ));
        }
        if candidate.rationale.trim().is_empty()
            || candidate.rationale.len() > MAX_CROSS_CLAIM_DISCOVERY_RATIONALE_BYTES
        {
            return Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(
                "candidate rationale is empty or too large".to_owned(),
            ));
        }
        let entry = grouped.entry((left_id, right_id)).or_insert_with(|| {
            (
                left_ordinal,
                right_ordinal,
                BTreeSet::new(),
                candidate.rationale.clone(),
            )
        });
        entry.2.insert(candidate.candidate_kind);
        if candidate.rationale < entry.3 {
            entry.3 = candidate.rationale;
        }
    }
    Ok(grouped
        .into_iter()
        .map(
            |(
                (left_inventory_item_id, right_inventory_item_id),
                (left_ordinal, right_ordinal, kinds, rationale),
            )| {
                ValidatedCrossClaimCandidate {
                    left_inventory_item_id,
                    right_inventory_item_id,
                    left_ordinal,
                    right_ordinal,
                    candidate_kinds: kinds.into_iter().collect(),
                    rationale,
                }
            },
        )
        .collect())
}

fn claim_ordinal(
    claim: &CrossClaimCandidateClaim,
    left_batch: &CrossClaimBatch,
    right_batch: &CrossClaimBatch,
) -> u32 {
    if let Some(index) = left_batch
        .claims
        .iter()
        .position(|item| item.inventory_item_id == claim.inventory_item_id)
    {
        return left_batch.ordinals[index];
    }
    right_batch
        .claims
        .iter()
        .position(|item| item.inventory_item_id == claim.inventory_item_id)
        .map(|index| right_batch.ordinals[index])
        .unwrap_or_default()
}

fn is_completed_identity_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(error)
            if error.code().as_deref() == Some("2067")
                && error
                    .message()
                    .contains("uq_research_manuscript_cross_claim_candidate_completed")
    )
}

fn map_cross_claim_candidate_run(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptCrossClaimCandidateRun, CrossClaimCandidateDiscoveryError> {
    Ok(ManuscriptCrossClaimCandidateRun {
        candidate_run_id: row.get("candidate_run_id"),
        research_case_id: row.get("research_case_id"),
        manuscript_source_id: row.get("manuscript_source_id"),
        document_id: row.get("document_id"),
        document_version: row.get("document_version"),
        claim_inventory_run_id: row.get("claim_inventory_run_id"),
        provider_id: row.get("provider_id"),
        model_id: row.get("model_id"),
        discovery_implementation_version: row.get("discovery_implementation_version"),
        discovery_contract_version: row.get("discovery_contract_version"),
        claim_count: row.get::<i64, _>("claim_count") as u32,
        batch_count: row.get::<i64, _>("batch_count") as u32,
        expected_window_count: row.get::<i64, _>("expected_window_count") as u32,
        processed_window_count: row.get::<i64, _>("processed_window_count") as u32,
        candidate_pair_count: row.get::<i64, _>("candidate_pair_count") as u32,
        status: parse_enum(row.get("status"), "cross-claim candidate run status")?,
        failure_code: row.get("failure_code"),
        created_at_ms: row.get("created_at_ms"),
        completed_at_ms: row.get("completed_at_ms"),
    })
}

fn map_cross_claim_window(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptCrossClaimComparisonWindow, CrossClaimCandidateDiscoveryError> {
    Ok(ManuscriptCrossClaimComparisonWindow {
        window_id: row.get("window_id"),
        candidate_run_id: row.get("candidate_run_id"),
        left_batch_ordinal: row.get::<i64, _>("left_batch_ordinal") as u32,
        right_batch_ordinal: row.get::<i64, _>("right_batch_ordinal") as u32,
        same_batch: row.get::<i64, _>("same_batch") != 0,
        status: parse_enum(row.get("status"), "cross-claim comparison window status")?,
        candidate_count: row.get::<i64, _>("candidate_count") as u32,
        failure_code: row.get("failure_code"),
    })
}

fn map_cross_claim_candidate(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptCrossClaimCandidate, CrossClaimCandidateDiscoveryError> {
    Ok(ManuscriptCrossClaimCandidate {
        candidate_id: row.get("candidate_id"),
        candidate_run_id: row.get("candidate_run_id"),
        comparison_window_id: row.get("comparison_window_id"),
        left_inventory_item_id: row.get("left_inventory_item_id"),
        right_inventory_item_id: row.get("right_inventory_item_id"),
        left_ordinal: row.get::<i64, _>("left_ordinal") as u32,
        right_ordinal: row.get::<i64, _>("right_ordinal") as u32,
        candidate_kinds: parse_json(row.get("candidate_kinds_json"), "candidate kinds")?,
        rationale: row.get("rationale"),
    })
}

fn parse_enum<T: for<'de> Deserialize<'de>>(
    value: String,
    label: &str,
) -> Result<T, CrossClaimCandidateDiscoveryError> {
    serde_json::from_str(&format!("\"{value}\""))
        .map_err(|_| CrossClaimCandidateDiscoveryError::Invalid(format!("invalid {label}")))
}

fn parse_json<T: for<'de> Deserialize<'de>>(
    value: String,
    label: &str,
) -> Result<T, CrossClaimCandidateDiscoveryError> {
    serde_json::from_str(&value)
        .map_err(|_| CrossClaimCandidateDiscoveryError::Invalid(format!("invalid {label}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(ordinal: u32) -> ManuscriptClaimInventoryItem {
        ManuscriptClaimInventoryItem {
            id: nineprofs_research::ManuscriptClaimInventoryItemId::new(),
            inventory_run_id: nineprofs_research::ManuscriptClaimInventoryRunId::new(),
            ordinal,
            document_block_id: format!("block-{ordinal}"),
            block_ordinal: ordinal,
            block_kind: ManuscriptClaimInventoryBlockKind::Paragraph,
            source_start: 0,
            source_end: 1,
            source_excerpt: format!("excerpt {ordinal}"),
            source_excerpt_hash: nineprofs_research::ContentHash {
                algorithm: nineprofs_research::HashAlgorithm::Sha256,
                value: "0".repeat(64),
            },
            claim_text: format!("claim {ordinal}"),
            review_kind: ClaimReviewKind::ManuscriptInternal,
            overlapping_citation_count: 0,
        }
    }

    #[test]
    fn scheduler_covers_every_unordered_pair_exactly_once() {
        let items = (0..35).map(item).collect::<Vec<_>>();
        let batches = build_cross_claim_batches(&items).unwrap();
        let windows = build_cross_claim_comparison_windows(batches.len()).unwrap();
        let mut pairs = BTreeMap::<(String, String), usize>::new();
        for window in windows {
            let left = &batches[window.left_batch_ordinal as usize];
            let right = &batches[window.right_batch_ordinal as usize];
            for pair in eligible_cross_claim_pairs(&window, left, right) {
                *pairs.entry(pair).or_default() += 1;
            }
        }
        assert_eq!(pairs.len(), items.len() * (items.len() - 1) / 2);
        assert!(pairs.values().all(|count| *count == 1));
    }

    #[test]
    fn scheduler_is_deterministic() {
        let items = (0..35).map(item).collect::<Vec<_>>();
        let first = build_cross_claim_batches(&items).unwrap();
        let second = build_cross_claim_batches(&items).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn scheduler_rejects_claim_and_input_bounds_without_truncation() {
        let too_many = (0..=MAX_CROSS_CLAIM_DISCOVERY_CLAIMS)
            .map(|ordinal| item(ordinal as u32))
            .collect::<Vec<_>>();
        assert!(matches!(
            build_cross_claim_batches(&too_many),
            Err(CrossClaimCandidateDiscoveryError::ClaimCountLimitExceeded)
        ));

        let mut oversized = item(0);
        oversized.claim_text = "x".repeat(MAX_CROSS_CLAIM_DISCOVERY_BATCH_BYTES);
        assert!(matches!(
            build_cross_claim_batches(&[oversized]),
            Err(CrossClaimCandidateDiscoveryError::ComparisonInputTooLarge)
        ));
    }

    #[test]
    fn provider_output_is_canonicalized_deduplicated_and_closed_set_validated() {
        let batches = build_cross_claim_batches(&[item(0), item(1)]).unwrap();
        let window = build_cross_claim_comparison_windows(1).unwrap().remove(0);
        let first = batches[0].claims[0].inventory_item_id.clone();
        let second = batches[0].claims[1].inventory_item_id.clone();
        let output = CrossClaimCandidateDiscoveryOutput {
            comparison_window_id: window.window_id.clone(),
            candidates: vec![
                CrossClaimCandidateOutput {
                    left_inventory_item_id: second.clone(),
                    right_inventory_item_id: first.clone(),
                    candidate_kind: ManuscriptCrossClaimCandidateKind::PotentialScopeMismatch,
                    rationale: "z rationale".to_owned(),
                },
                CrossClaimCandidateOutput {
                    left_inventory_item_id: second.clone(),
                    right_inventory_item_id: first.clone(),
                    candidate_kind: ManuscriptCrossClaimCandidateKind::PotentialDirectConflict,
                    rationale: "a rationale".to_owned(),
                },
                CrossClaimCandidateOutput {
                    left_inventory_item_id: second,
                    right_inventory_item_id: first.clone(),
                    candidate_kind: ManuscriptCrossClaimCandidateKind::PotentialDirectConflict,
                    rationale: "duplicate rationale".to_owned(),
                },
            ],
        };
        let candidates = validate_cross_claim_output(&window, &batches[0], &batches[0], output)
            .expect("valid candidate output");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].left_ordinal, 0);
        assert_eq!(candidates[0].right_ordinal, 1);
        assert_eq!(candidates[0].candidate_kinds.len(), 2);
        assert_eq!(candidates[0].rationale, "a rationale");

        let unknown = CrossClaimCandidateDiscoveryOutput {
            comparison_window_id: window.window_id.clone(),
            candidates: vec![CrossClaimCandidateOutput {
                left_inventory_item_id: first.clone(),
                right_inventory_item_id: "unknown".to_owned(),
                candidate_kind: ManuscriptCrossClaimCandidateKind::OtherConsistencyCandidate,
                rationale: "unknown ID".to_owned(),
            }],
        };
        assert!(matches!(
            validate_cross_claim_output(&window, &batches[0], &batches[0], unknown),
            Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(_))
        ));

        let self_pair = CrossClaimCandidateDiscoveryOutput {
            comparison_window_id: window.window_id.clone(),
            candidates: vec![CrossClaimCandidateOutput {
                left_inventory_item_id: first.clone(),
                right_inventory_item_id: first,
                candidate_kind: ManuscriptCrossClaimCandidateKind::PotentialDuplicateOrRestatement,
                rationale: "self pair".to_owned(),
            }],
        };
        assert!(matches!(
            validate_cross_claim_output(&window, &batches[0], &batches[0], self_pair),
            Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(_))
        ));
    }

    #[test]
    fn cross_window_ids_and_per_window_candidate_bounds_fail_closed() {
        let items = (0..17).map(item).collect::<Vec<_>>();
        let batches = build_cross_claim_batches(&items).unwrap();
        let same_batch = build_cross_claim_comparison_windows(2).unwrap()[0].clone();
        let output = CrossClaimCandidateDiscoveryOutput {
            comparison_window_id: same_batch.window_id.clone(),
            candidates: vec![CrossClaimCandidateOutput {
                left_inventory_item_id: batches[0].claims[0].inventory_item_id.clone(),
                right_inventory_item_id: batches[1].claims[0].inventory_item_id.clone(),
                candidate_kind: ManuscriptCrossClaimCandidateKind::OtherConsistencyCandidate,
                rationale: "cross-window".to_owned(),
            }],
        };
        assert!(matches!(
            validate_cross_claim_output(&same_batch, &batches[0], &batches[0], output),
            Err(CrossClaimCandidateDiscoveryError::ClosedSetViolation(_))
        ));

        let cross_batch = build_cross_claim_comparison_windows(2).unwrap()[1].clone();
        let candidate = CrossClaimCandidateOutput {
            left_inventory_item_id: batches[0].claims[0].inventory_item_id.clone(),
            right_inventory_item_id: batches[1].claims[0].inventory_item_id.clone(),
            candidate_kind: ManuscriptCrossClaimCandidateKind::OtherConsistencyCandidate,
            rationale: "too many candidates".to_owned(),
        };
        let output = CrossClaimCandidateDiscoveryOutput {
            comparison_window_id: cross_batch.window_id.clone(),
            candidates: vec![candidate; MAX_CROSS_CLAIM_DISCOVERY_CANDIDATES_PER_WINDOW + 1],
        };
        assert!(matches!(
            validate_cross_claim_output(&cross_batch, &batches[0], &batches[1], output),
            Err(CrossClaimCandidateDiscoveryError::CandidateCountLimitExceeded)
        ));
    }
}
