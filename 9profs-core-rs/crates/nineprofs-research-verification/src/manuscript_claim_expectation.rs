use std::collections::BTreeMap;

use async_trait::async_trait;
use nineprofs_common::{new_id, now_ms};
use nineprofs_research::{
    ClaimReviewKind, ManuscriptClaimInventoryBlockKind, ManuscriptClaimInventoryStatus,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sqlx::Row;
use thiserror::Error;

use crate::{
    CitationReviewError, ManuscriptClaimCoverageBridgeStatus, ManuscriptClaimCoverageItem,
    ManuscriptClaimCoverageRun, ManuscriptClaimCoverageStructuralCitationState,
};

pub const MANUSCRIPT_CITATION_EXPECTATION_CONTRACT_VERSION: &str =
    "manuscript-citation-expectation-v1";
pub const MAX_EXPECTATION_RATIONALE_BYTES: usize = 4 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationExpectation {
    ExternalEvidenceExpected,
    ExternalEvidenceContextDependent,
    ManuscriptInternalSupport,
    NoExternalCitationExpected,
    Uncertain,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptCitationExpectationRunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationExpectationAssessmentStatus {
    Assessed,
    AssessmentFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageAttentionState {
    NoCoverageAttentionDetected,
    ReviewSuggested,
    ExpectationReviewNeeded,
    AssessmentUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageAttentionReason {
    ExpectedExternalEvidenceNoExactCitationLink,
    AmbiguousClaimCitationBridge,
    CitationVerificationBlocked,
    CitationVerificationIncomplete,
    CitationVerificationInsufficient,
    CitationVerificationContextualizes,
    ExpectedExternalEvidenceNoSupportingVerification,
    ContradictoryEvidenceObserved,
    MixedEvidenceRelations,
    ExpectationContextDependent,
    ExpectationUncertain,
    ExpectationAssessmentFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationExpectationInput {
    pub item_id: String,
    pub claim_text: String,
    pub source_excerpt: String,
    pub review_kind: ClaimReviewKind,
    pub block_kind: ManuscriptClaimInventoryBlockKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationExpectationAssessment {
    pub item_id: String,
    pub expectation: CitationExpectation,
    pub rationale: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationExpectationProviderIdentity {
    pub provider_id: String,
    pub assessor_version: String,
    pub model_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum CitationExpectationProviderError {
    #[error("citation expectation assessor is not configured")]
    NotConfigured,
    #[error("citation expectation assessor configuration is invalid")]
    InvalidConfiguration,
    #[error("citation expectation assessor input is invalid")]
    InvalidInput,
    #[error("citation expectation assessor input exceeded size limit")]
    InputTooLarge,
    #[error("citation expectation assessor request timed out")]
    Timeout,
    #[error("citation expectation assessor authorization failed")]
    Unauthorized,
    #[error("citation expectation assessor rate limit exceeded")]
    RateLimited,
    #[error("citation expectation assessor provider is unavailable")]
    ProviderUnavailable,
    #[error("citation expectation assessor response was malformed")]
    MalformedResponse,
    #[error("citation expectation assessor returned invalid structured output")]
    InvalidStructuredOutput,
    #[error("citation expectation assessor response exceeded size limit")]
    ResponseTooLarge,
}

impl CitationExpectationProviderError {
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
pub trait CitationExpectationProvider: Send + Sync {
    fn identity(&self) -> CitationExpectationProviderIdentity;

    async fn assess(
        &self,
        input: CitationExpectationInput,
    ) -> Result<CitationExpectationAssessment, CitationExpectationProviderError>;
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartManuscriptCitationExpectation {
    pub research_case_id: String,
    pub claim_coverage_run_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCitationExpectationRun {
    pub expectation_run_id: String,
    pub research_case_id: String,
    pub claim_coverage_run_id: String,
    pub provider_id: String,
    pub assessor_version: String,
    pub model_id: Option<String>,
    pub expectation_contract_version: String,
    pub coverage_contract_version: String,
    pub coverage_scope: String,
    pub coverage_limitations: Vec<String>,
    pub status: ManuscriptCitationExpectationRunStatus,
    pub item_count: u32,
    pub failed_item_count: u32,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptCitationExpectationItem {
    pub expectation_item_id: String,
    pub expectation_run_id: String,
    pub coverage_item_id: String,
    pub inventory_item_id: String,
    pub ordinal: u32,
    pub claim_text: String,
    pub source_excerpt: String,
    pub review_kind: ClaimReviewKind,
    pub block_kind: ManuscriptClaimInventoryBlockKind,
    pub assessment_status: CitationExpectationAssessmentStatus,
    pub expectation: Option<CitationExpectation>,
    pub attention: CoverageAttentionState,
    pub attention_reasons: Vec<CoverageAttentionReason>,
    pub rationale: Option<String>,
    pub failure_code: Option<String>,
}

#[derive(Clone)]
struct BuiltExpectation {
    run: ManuscriptCitationExpectationRun,
    items: Vec<ManuscriptCitationExpectationItem>,
}

impl super::CitationReviewService {
    pub async fn start_manuscript_citation_expectation(
        &self,
        input: StartManuscriptCitationExpectation,
    ) -> Result<ManuscriptCitationExpectationRun, CitationReviewError> {
        let coverage = self
            .get_manuscript_claim_coverage(&input.claim_coverage_run_id)
            .await?;
        let inventory = self
            .research_service()
            .get_manuscript_claim_inventory(&coverage.claim_inventory_run_id)
            .await?;
        validate_compatibility(&input, &coverage, &inventory)?;

        let identity = self
            .expectation_assessor()
            .map(|provider| provider.identity());
        let provider_id = identity
            .as_ref()
            .map(|value| value.provider_id.as_str())
            .unwrap_or("unconfigured")
            .to_owned();
        let assessor_version = identity
            .as_ref()
            .map(|value| value.assessor_version.as_str())
            .unwrap_or("unconfigured")
            .to_owned();
        let model_id = identity.and_then(|value| value.model_id);

        if let Some(existing) = self
            .find_completed_manuscript_citation_expectation(
                &input.research_case_id,
                &input.claim_coverage_run_id,
                &provider_id,
                &assessor_version,
                model_id.as_deref(),
            )
            .await?
        {
            return Ok(existing);
        }

        let coverage_items = self
            .list_manuscript_claim_coverage_items(&coverage.coverage_run_id)
            .await?;
        let inventory_items = self
            .research_service()
            .list_manuscript_claim_inventory_items(&coverage.claim_inventory_run_id)
            .await?;
        let inventory_by_id = inventory_items
            .into_iter()
            .map(|item| (item.id.to_string(), item))
            .collect::<BTreeMap<_, _>>();
        let provider = self.expectation_assessor();
        let expectation_run_id = format!("manuscript_citation_expectation_{}", new_id());
        let created_at_ms = now_ms();
        let mut items = Vec::with_capacity(coverage_items.len());

        for coverage_item in coverage_items {
            let inventory_item = inventory_by_id
                .get(&coverage_item.inventory_item_id)
                .ok_or_else(|| {
                    CitationReviewError::Invalid(
                        "coverage item references missing inventory item".to_owned(),
                    )
                })?;
            if inventory_item.inventory_run_id.to_string() != coverage.claim_inventory_run_id {
                return Err(CitationReviewError::Invalid(
                    "coverage item crosses inventory history".to_owned(),
                ));
            }

            let semantic_input = CitationExpectationInput {
                item_id: inventory_item.id.to_string(),
                claim_text: inventory_item.claim_text.clone(),
                source_excerpt: inventory_item.source_excerpt.clone(),
                review_kind: inventory_item.review_kind.clone(),
                block_kind: inventory_item.block_kind.clone(),
            };
            let result = match provider.as_ref() {
                Some(provider) => provider.assess(semantic_input.clone()).await,
                None => Err(CitationExpectationProviderError::NotConfigured),
            };

            let (assessment_status, expectation, rationale, failure_code) = match result {
                Ok(assessment)
                    if assessment.item_id == semantic_input.item_id
                        && assessment.rationale.len() <= MAX_EXPECTATION_RATIONALE_BYTES =>
                {
                    (
                        CitationExpectationAssessmentStatus::Assessed,
                        Some(assessment.expectation),
                        Some(assessment.rationale),
                        None,
                    )
                }
                Ok(assessment) if assessment.item_id != semantic_input.item_id => (
                    CitationExpectationAssessmentStatus::AssessmentFailed,
                    None,
                    None,
                    Some("closed_set_violation".to_owned()),
                ),
                Ok(_) => (
                    CitationExpectationAssessmentStatus::AssessmentFailed,
                    None,
                    None,
                    Some("rationale_too_large".to_owned()),
                ),
                Err(error) => (
                    CitationExpectationAssessmentStatus::AssessmentFailed,
                    None,
                    None,
                    Some(error.code().to_owned()),
                ),
            };

            let (attention, attention_reasons) = match expectation.as_ref() {
                Some(expectation) => compose_attention(&coverage_item, expectation, false),
                None => compose_attention(&coverage_item, &CitationExpectation::Uncertain, true),
            };
            items.push(ManuscriptCitationExpectationItem {
                expectation_item_id: format!("manuscript_citation_expectation_item_{}", new_id()),
                expectation_run_id: expectation_run_id.clone(),
                coverage_item_id: coverage_item.coverage_item_id,
                inventory_item_id: inventory_item.id.to_string(),
                ordinal: inventory_item.ordinal,
                claim_text: inventory_item.claim_text.clone(),
                source_excerpt: inventory_item.source_excerpt.clone(),
                review_kind: inventory_item.review_kind.clone(),
                block_kind: inventory_item.block_kind.clone(),
                assessment_status,
                expectation,
                attention,
                attention_reasons,
                rationale,
                failure_code,
            });
        }

        let run = ManuscriptCitationExpectationRun {
            expectation_run_id,
            research_case_id: input.research_case_id,
            claim_coverage_run_id: coverage.coverage_run_id,
            provider_id,
            assessor_version,
            model_id,
            expectation_contract_version: MANUSCRIPT_CITATION_EXPECTATION_CONTRACT_VERSION
                .to_owned(),
            coverage_contract_version: coverage.coverage_contract_version,
            coverage_scope: coverage.coverage_scope,
            coverage_limitations: coverage.coverage_limitations,
            status: ManuscriptCitationExpectationRunStatus::Completed,
            item_count: items.len() as u32,
            failed_item_count: items
                .iter()
                .filter(|item| {
                    matches!(
                        item.assessment_status,
                        CitationExpectationAssessmentStatus::AssessmentFailed
                    )
                })
                .count() as u32,
            created_at_ms,
            completed_at_ms: Some(now_ms()),
        };
        let built = BuiltExpectation { run, items };
        self.persist_manuscript_citation_expectation(&built).await
    }

    pub async fn get_manuscript_citation_expectation(
        &self,
        expectation_run_id: &str,
    ) -> Result<ManuscriptCitationExpectationRun, CitationReviewError> {
        let row = sqlx::query(
            "SELECT expectation_run_id, research_case_id, claim_coverage_run_id,
             provider_id, assessor_version, model_id, expectation_contract_version,
             coverage_contract_version, coverage_scope, coverage_limitations_json, status,
             item_count, failed_item_count, created_at_ms, completed_at_ms
             FROM research_manuscript_citation_expectation_runs
             WHERE expectation_run_id = ?",
        )
        .bind(expectation_run_id)
        .fetch_optional(self.pool())
        .await?
        .ok_or_else(|| CitationReviewError::NotFound(expectation_run_id.to_owned()))?;
        Ok(ManuscriptCitationExpectationRun {
            expectation_run_id: row.get("expectation_run_id"),
            research_case_id: row.get("research_case_id"),
            claim_coverage_run_id: row.get("claim_coverage_run_id"),
            provider_id: row.get("provider_id"),
            assessor_version: row.get("assessor_version"),
            model_id: row.get("model_id"),
            expectation_contract_version: row.get("expectation_contract_version"),
            coverage_contract_version: row.get("coverage_contract_version"),
            coverage_scope: row.get("coverage_scope"),
            coverage_limitations: parse_json(
                row.get("coverage_limitations_json"),
                "expectation coverage limitations",
            )?,
            status: parse_enum(row.get::<String, _>("status"), "expectation run status")?,
            item_count: row.get::<i64, _>("item_count") as u32,
            failed_item_count: row.get::<i64, _>("failed_item_count") as u32,
            created_at_ms: row.get("created_at_ms"),
            completed_at_ms: row.get("completed_at_ms"),
        })
    }

    pub async fn list_manuscript_citation_expectation_items(
        &self,
        expectation_run_id: &str,
    ) -> Result<Vec<ManuscriptCitationExpectationItem>, CitationReviewError> {
        let run = self
            .get_manuscript_citation_expectation(expectation_run_id)
            .await?;
        let rows = sqlx::query(
            "SELECT expectation_item_id, expectation_run_id, coverage_item_id,
             inventory_item_id, ordinal, claim_text, source_excerpt, review_kind,
             block_kind, assessment_status, expectation, attention, attention_reasons_json,
             rationale, failure_code
             FROM research_manuscript_citation_expectation_items
             WHERE expectation_run_id = ? ORDER BY ordinal ASC",
        )
        .bind(&run.expectation_run_id)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(map_expectation_item).collect()
    }

    async fn find_completed_manuscript_citation_expectation(
        &self,
        research_case_id: &str,
        claim_coverage_run_id: &str,
        provider_id: &str,
        assessor_version: &str,
        model_id: Option<&str>,
    ) -> Result<Option<ManuscriptCitationExpectationRun>, CitationReviewError> {
        let row = sqlx::query(
            "SELECT expectation_run_id
             FROM research_manuscript_citation_expectation_runs
             WHERE research_case_id = ? AND claim_coverage_run_id = ?
               AND provider_id = ? AND assessor_version = ?
               AND COALESCE(model_id, '') = COALESCE(?, '')
               AND expectation_contract_version = ?
               AND status = 'completed' AND failed_item_count = 0
             ORDER BY created_at_ms DESC LIMIT 1",
        )
        .bind(research_case_id)
        .bind(claim_coverage_run_id)
        .bind(provider_id)
        .bind(assessor_version)
        .bind(model_id)
        .bind(MANUSCRIPT_CITATION_EXPECTATION_CONTRACT_VERSION)
        .fetch_optional(self.pool())
        .await?;
        match row {
            Some(row) => self
                .get_manuscript_citation_expectation(row.get("expectation_run_id"))
                .await
                .map(Some),
            None => Ok(None),
        }
    }

    async fn persist_manuscript_citation_expectation(
        &self,
        built: &BuiltExpectation,
    ) -> Result<ManuscriptCitationExpectationRun, CitationReviewError> {
        let mut tx = self.pool().begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO research_manuscript_citation_expectation_runs
             (expectation_run_id, research_case_id, claim_coverage_run_id, provider_id,
              assessor_version, model_id, expectation_contract_version,
              coverage_contract_version, coverage_scope, coverage_limitations_json,
              status, item_count, failed_item_count, created_at_ms, completed_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT DO NOTHING",
        )
        .bind(&built.run.expectation_run_id)
        .bind(&built.run.research_case_id)
        .bind(&built.run.claim_coverage_run_id)
        .bind(&built.run.provider_id)
        .bind(&built.run.assessor_version)
        .bind(&built.run.model_id)
        .bind(&built.run.expectation_contract_version)
        .bind(&built.run.coverage_contract_version)
        .bind(&built.run.coverage_scope)
        .bind(
            serde_json::to_string(&built.run.coverage_limitations).map_err(|error| {
                CitationReviewError::Invalid(format!(
                    "expectation coverage limitations serialization failed: {error}"
                ))
            })?,
        )
        .bind(enum_text(&built.run.status))
        .bind(built.run.item_count as i64)
        .bind(built.run.failed_item_count as i64)
        .bind(built.run.created_at_ms)
        .bind(built.run.completed_at_ms)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() == 0 {
            tx.rollback().await?;
            return self
                .find_completed_manuscript_citation_expectation(
                    &built.run.research_case_id,
                    &built.run.claim_coverage_run_id,
                    &built.run.provider_id,
                    &built.run.assessor_version,
                    built.run.model_id.as_deref(),
                )
                .await?
                .ok_or_else(|| {
                    CitationReviewError::Invalid(
                        "expectation run uniqueness conflict did not resolve to completed history"
                            .to_owned(),
                    )
                });
        }

        for item in &built.items {
            sqlx::query(
                "INSERT INTO research_manuscript_citation_expectation_items
                 (expectation_item_id, expectation_run_id, coverage_item_id, inventory_item_id,
                  ordinal, claim_text, source_excerpt, review_kind, block_kind,
                  assessment_status, expectation, attention, attention_reasons_json,
                  rationale, failure_code)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&item.expectation_item_id)
            .bind(&item.expectation_run_id)
            .bind(&item.coverage_item_id)
            .bind(&item.inventory_item_id)
            .bind(item.ordinal as i64)
            .bind(&item.claim_text)
            .bind(&item.source_excerpt)
            .bind(enum_text(&item.review_kind))
            .bind(enum_text(&item.block_kind))
            .bind(enum_text(&item.assessment_status))
            .bind(item.expectation.as_ref().map(enum_text))
            .bind(enum_text(&item.attention))
            .bind(
                serde_json::to_string(&item.attention_reasons).map_err(|error| {
                    CitationReviewError::Invalid(format!(
                        "expectation attention reason serialization failed: {error}"
                    ))
                })?,
            )
            .bind(&item.rationale)
            .bind(&item.failure_code)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.get_manuscript_citation_expectation(&built.run.expectation_run_id)
            .await
    }
}

fn validate_compatibility(
    input: &StartManuscriptCitationExpectation,
    coverage: &ManuscriptClaimCoverageRun,
    inventory: &nineprofs_research::ManuscriptClaimInventoryRun,
) -> Result<(), CitationReviewError> {
    if !matches!(
        coverage.status,
        crate::ManuscriptClaimCoverageRunStatus::Completed
    ) || !matches!(inventory.status, ManuscriptClaimInventoryStatus::Completed)
    {
        return Err(CitationReviewError::Invalid(
            "citation expectation requires completed coverage and inventory runs".to_owned(),
        ));
    }
    if input.research_case_id != coverage.research_case_id
        || inventory.research_case_id.to_string() != coverage.research_case_id
        || inventory.id.to_string() != coverage.claim_inventory_run_id
        || inventory.manuscript_source_id.to_string() != coverage.manuscript_source_id
        || inventory.document_id != coverage.document_id
        || inventory.document_version != coverage.document_version
    {
        return Err(CitationReviewError::Invalid(
            "citation expectation histories are incompatible".to_owned(),
        ));
    }
    Ok(())
}

fn compose_attention(
    coverage: &ManuscriptClaimCoverageItem,
    expectation: &CitationExpectation,
    assessment_failed: bool,
) -> (CoverageAttentionState, Vec<CoverageAttentionReason>) {
    let mut reasons = Vec::new();
    let mut add = |reason| {
        if !reasons.contains(&reason) {
            reasons.push(reason);
        }
    };
    if matches!(
        coverage.bridge_status,
        ManuscriptClaimCoverageBridgeStatus::SameSpanDifferentClaim
            | ManuscriptClaimCoverageBridgeStatus::MultipleExactCandidates
            | ManuscriptClaimCoverageBridgeStatus::InvalidCrossHistory
    ) || matches!(
        coverage.structural_citation_state,
        ManuscriptClaimCoverageStructuralCitationState::AmbiguousClaimBridge
    ) {
        add(CoverageAttentionReason::AmbiguousClaimCitationBridge);
    }
    if coverage.contradiction_count > 0 {
        add(CoverageAttentionReason::ContradictoryEvidenceObserved);
        if coverage.support_count > 0 {
            add(CoverageAttentionReason::MixedEvidenceRelations);
        }
    }
    if assessment_failed {
        add(CoverageAttentionReason::ExpectationAssessmentFailed);
        return (CoverageAttentionState::AssessmentUnavailable, reasons);
    }

    match expectation {
        CitationExpectation::ExternalEvidenceExpected => {
            if coverage.exact_claim_citation_link_count == 0 {
                add(CoverageAttentionReason::ExpectedExternalEvidenceNoExactCitationLink);
            } else {
                if coverage.blocked_count > 0 {
                    add(CoverageAttentionReason::CitationVerificationBlocked);
                }
                if coverage.unverified_count > 0 {
                    add(CoverageAttentionReason::CitationVerificationIncomplete);
                }
                if coverage.insufficient_count > 0 {
                    add(CoverageAttentionReason::CitationVerificationInsufficient);
                }
                if coverage.contextualize_count > 0 {
                    add(CoverageAttentionReason::CitationVerificationContextualizes);
                }
                if coverage.support_count == 0
                    && coverage.blocked_count == 0
                    && coverage.unverified_count == 0
                    && coverage.insufficient_count == 0
                    && coverage.contextualize_count == 0
                {
                    add(CoverageAttentionReason::ExpectedExternalEvidenceNoSupportingVerification);
                }
            }
        }
        CitationExpectation::ExternalEvidenceContextDependent => {
            add(CoverageAttentionReason::ExpectationContextDependent);
        }
        CitationExpectation::Uncertain => {
            add(CoverageAttentionReason::ExpectationUncertain);
        }
        CitationExpectation::ManuscriptInternalSupport
        | CitationExpectation::NoExternalCitationExpected => {}
    }

    let state = match expectation {
        CitationExpectation::ExternalEvidenceContextDependent | CitationExpectation::Uncertain => {
            CoverageAttentionState::ExpectationReviewNeeded
        }
        _ if reasons.is_empty() => CoverageAttentionState::NoCoverageAttentionDetected,
        _ => CoverageAttentionState::ReviewSuggested,
    };
    (state, reasons)
}

fn map_expectation_item(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptCitationExpectationItem, CitationReviewError> {
    Ok(ManuscriptCitationExpectationItem {
        expectation_item_id: row.get("expectation_item_id"),
        expectation_run_id: row.get("expectation_run_id"),
        coverage_item_id: row.get("coverage_item_id"),
        inventory_item_id: row.get("inventory_item_id"),
        ordinal: row.get::<i64, _>("ordinal") as u32,
        claim_text: row.get("claim_text"),
        source_excerpt: row.get("source_excerpt"),
        review_kind: parse_enum(row.get::<String, _>("review_kind"), "review kind")?,
        block_kind: parse_enum(row.get::<String, _>("block_kind"), "block kind")?,
        assessment_status: parse_enum(
            row.get::<String, _>("assessment_status"),
            "expectation assessment status",
        )?,
        expectation: row
            .get::<Option<String>, _>("expectation")
            .map(|value| parse_enum(value, "citation expectation"))
            .transpose()?,
        attention: parse_enum(row.get::<String, _>("attention"), "coverage attention")?,
        attention_reasons: parse_json(
            row.get("attention_reasons_json"),
            "coverage attention reasons",
        )?,
        rationale: row.get("rationale"),
        failure_code: row.get("failure_code"),
    })
}

fn enum_text<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .expect("research enum serialization must be infallible")
        .trim_matches('"')
        .to_owned()
}

fn parse_enum<T>(value: String, label: &str) -> Result<T, CitationReviewError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(serde_json::Value::String(value))
        .map_err(|error| CitationReviewError::Invalid(format!("invalid {label}: {error}")))
}

fn parse_json<T>(value: String, label: &str) -> Result<T, CitationReviewError>
where
    T: DeserializeOwned,
{
    serde_json::from_str(&value)
        .map_err(|error| CitationReviewError::Invalid(format!("invalid {label}: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage() -> ManuscriptClaimCoverageItem {
        ManuscriptClaimCoverageItem {
            coverage_item_id: "coverage-item".to_owned(),
            coverage_run_id: "coverage-run".to_owned(),
            inventory_item_id: "inventory-item".to_owned(),
            ordinal: 0,
            bridge_status: ManuscriptClaimCoverageBridgeStatus::ExactClaimBridge,
            structural_citation_state:
                ManuscriptClaimCoverageStructuralCitationState::NoCitationObservedInBlock,
            matched_claim_extraction_item_id: None,
            matched_research_claim_id: None,
            inventory_overlapping_citation_count: 0,
            same_block_citation_count: 0,
            claim_range_citation_count: 0,
            exact_claim_citation_link_count: 0,
            target_count: 0,
            support_count: 0,
            contradiction_count: 0,
            contextualize_count: 0,
            insufficient_count: 0,
            unverified_count: 0,
            blocked_count: 0,
        }
    }

    fn reasons(
        coverage: &ManuscriptClaimCoverageItem,
        expectation: CitationExpectation,
    ) -> Vec<CoverageAttentionReason> {
        compose_attention(coverage, &expectation, false).1
    }

    #[test]
    fn expected_external_without_exact_link_is_review_signal() {
        let mut value = coverage();
        value.same_block_citation_count = 1;
        let (attention, reasons) = compose_attention(
            &value,
            &CitationExpectation::ExternalEvidenceExpected,
            false,
        );
        assert_eq!(attention, CoverageAttentionState::ReviewSuggested);
        assert!(
            reasons.contains(&CoverageAttentionReason::ExpectedExternalEvidenceNoExactCitationLink)
        );
    }

    #[test]
    fn exact_support_has_no_missing_or_no_support_reason() {
        let mut value = coverage();
        value.exact_claim_citation_link_count = 1;
        value.target_count = 1;
        value.support_count = 1;
        let reasons = reasons(&value, CitationExpectation::ExternalEvidenceExpected);
        assert!(reasons.is_empty());
    }

    #[test]
    fn insufficient_contextualized_blocked_and_incomplete_stay_distinct() {
        let mut insufficient = coverage();
        insufficient.exact_claim_citation_link_count = 1;
        insufficient.target_count = 1;
        insufficient.insufficient_count = 1;
        assert!(
            reasons(&insufficient, CitationExpectation::ExternalEvidenceExpected)
                .contains(&CoverageAttentionReason::CitationVerificationInsufficient)
        );
        assert!(
            !reasons(&insufficient, CitationExpectation::ExternalEvidenceExpected).contains(
                &CoverageAttentionReason::ExpectedExternalEvidenceNoSupportingVerification
            )
        );

        let mut contextualized = insufficient.clone();
        contextualized.insufficient_count = 0;
        contextualized.contextualize_count = 1;
        assert!(
            reasons(
                &contextualized,
                CitationExpectation::ExternalEvidenceExpected
            )
            .contains(&CoverageAttentionReason::CitationVerificationContextualizes)
        );
        assert!(
            !reasons(
                &contextualized,
                CitationExpectation::ExternalEvidenceExpected
            )
            .contains(&CoverageAttentionReason::ExpectedExternalEvidenceNoSupportingVerification)
        );

        let mut blocked = insufficient.clone();
        blocked.insufficient_count = 0;
        blocked.blocked_count = 1;
        assert!(
            reasons(&blocked, CitationExpectation::ExternalEvidenceExpected)
                .contains(&CoverageAttentionReason::CitationVerificationBlocked)
        );
        assert!(
            !reasons(&blocked, CitationExpectation::ExternalEvidenceExpected).contains(
                &CoverageAttentionReason::ExpectedExternalEvidenceNoSupportingVerification
            )
        );

        let mut incomplete = insufficient;
        incomplete.insufficient_count = 0;
        incomplete.unverified_count = 1;
        assert!(
            reasons(&incomplete, CitationExpectation::ExternalEvidenceExpected)
                .contains(&CoverageAttentionReason::CitationVerificationIncomplete)
        );
    }

    #[test]
    fn contradiction_is_preserved_and_mixed_is_explicit() {
        let mut value = coverage();
        value.exact_claim_citation_link_count = 1;
        value.target_count = 2;
        value.support_count = 1;
        value.contradiction_count = 1;
        let (attention, reasons) = compose_attention(
            &value,
            &CitationExpectation::NoExternalCitationExpected,
            false,
        );
        assert_eq!(attention, CoverageAttentionState::ReviewSuggested);
        assert!(reasons.contains(&CoverageAttentionReason::ContradictoryEvidenceObserved));
        assert!(reasons.contains(&CoverageAttentionReason::MixedEvidenceRelations));
    }

    #[test]
    fn ambiguous_bridge_is_independent_review_signal() {
        let mut value = coverage();
        value.bridge_status = ManuscriptClaimCoverageBridgeStatus::SameSpanDifferentClaim;
        let reasons = reasons(&value, CitationExpectation::ManuscriptInternalSupport);
        assert!(reasons.contains(&CoverageAttentionReason::AmbiguousClaimCitationBridge));
    }

    #[test]
    fn internal_and_no_external_expectations_do_not_require_citation() {
        let value = coverage();
        for expectation in [
            CitationExpectation::ManuscriptInternalSupport,
            CitationExpectation::NoExternalCitationExpected,
        ] {
            let (attention, reasons) = compose_attention(&value, &expectation, false);
            assert_eq!(
                attention,
                CoverageAttentionState::NoCoverageAttentionDetected
            );
            assert!(
                !reasons.contains(
                    &CoverageAttentionReason::ExpectedExternalEvidenceNoExactCitationLink
                )
            );
        }
    }

    #[test]
    fn context_dependent_and_uncertain_require_expectation_review() {
        for (expectation, reason) in [
            (
                CitationExpectation::ExternalEvidenceContextDependent,
                CoverageAttentionReason::ExpectationContextDependent,
            ),
            (
                CitationExpectation::Uncertain,
                CoverageAttentionReason::ExpectationUncertain,
            ),
        ] {
            let (attention, reasons) = compose_attention(&coverage(), &expectation, false);
            assert_eq!(attention, CoverageAttentionState::ExpectationReviewNeeded);
            assert!(reasons.contains(&reason));
            assert!(
                !reasons.contains(
                    &CoverageAttentionReason::ExpectedExternalEvidenceNoExactCitationLink
                )
            );
        }
    }

    #[test]
    fn assessment_failure_is_unavailable_not_uncertain() {
        let (attention, reasons) =
            compose_attention(&coverage(), &CitationExpectation::Uncertain, true);
        assert_eq!(attention, CoverageAttentionState::AssessmentUnavailable);
        assert!(reasons.contains(&CoverageAttentionReason::ExpectationAssessmentFailed));
    }
}
