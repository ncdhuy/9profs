use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use nineprofs_api_types::EventEnvelope;
use nineprofs_common::now_ms;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CitationBindingMethod, ClaimEvidenceRelation, EvidenceLocator,
    ManuscriptClaimExtractionCoverageStatus, ManuscriptReferenceResolutionOutcome, ResearchError,
    ResearchService, SourceKind,
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use thiserror::Error;

use crate::{
    CitationVerificationError, CitationVerificationService, CitationVerificationStatus,
    CreateCitationVerification,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationReviewRunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationReviewItemStatus {
    UnresolvedReference,
    AmbiguousReference,
    ReferenceRequiresConfirmation,
    SourceMatchedNotVerificationReady,
    BindingConflict,
    ReadyForVerification,
    VerificationRunning,
    VerificationCompleted,
    VerificationFailed,
    ResolutionFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartManuscriptCitationReview {
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub citation_sync_run_id: String,
    pub reference_catalog_run_id: String,
    pub reference_resolution_run_id: String,
    pub claim_extraction_run_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewRun {
    pub review_run_id: String,
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub citation_sync_run_id: Option<String>,
    pub reference_catalog_run_id: Option<String>,
    pub reference_resolution_run_id: Option<String>,
    pub claim_extraction_run_id: Option<String>,
    pub status: CitationReviewRunStatus,
    pub failure_stage: Option<String>,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewCandidate {
    pub candidate_id: String,
    pub resolution_entry_id: String,
    pub ordinal: u32,
    pub source_id: String,
    pub source_label: Option<String>,
    pub source_snapshot_id: Option<String>,
    pub extraction_id: Option<String>,
    pub match_kind: Option<nineprofs_research::ManuscriptReferenceResolutionMatchKind>,
    pub automatic_binding_permitted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewVerification {
    pub verification_run_id: String,
    pub status: CitationVerificationStatus,
    pub failure_code: Option<String>,
    pub relation: Option<ClaimEvidenceRelation>,
    pub rationale: Option<String>,
    pub assessor_provider: Option<String>,
    pub assessor_version: Option<String>,
    pub assessor_model_id: Option<String>,
    pub completed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewEvidence {
    pub evidence_id: String,
    pub relation: ClaimEvidenceRelation,
    pub source_snapshot_id: String,
    pub extraction_id: Option<String>,
    pub locator: EvidenceLocator,
    pub verbatim_excerpt: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationReviewItem {
    pub item_id: String,
    pub review_run_id: String,
    pub ordinal: u32,
    pub claim_id: String,
    pub claim_citation_link_id: String,
    pub citation_occurrence_id: String,
    pub citation_target_id: String,
    pub reference_entry_id: Option<String>,
    pub resolution_entry_id: Option<String>,
    pub resolution_outcome: Option<ManuscriptReferenceResolutionOutcome>,
    pub document_block_id: String,
    pub start: u64,
    pub end: u64,
    pub rendered_text: String,
    pub reference_key: String,
    pub cited_locator: Option<String>,
    pub claim_text: String,
    pub source_excerpt: Option<String>,
    pub binding_id: Option<String>,
    pub binding_method: Option<CitationBindingMethod>,
    pub source_id: Option<String>,
    pub source_snapshot_id: Option<String>,
    pub extraction_id: Option<String>,
    pub status: CitationReviewItemStatus,
    pub failure_code: Option<String>,
    pub candidates: Vec<CitationReviewCandidate>,
    pub verification: Option<CitationReviewVerification>,
    pub evidence: Vec<CitationReviewEvidence>,
}

#[derive(Debug, Error)]
pub enum CitationReviewError {
    #[error("citation review not found: {0}")]
    NotFound(String),
    #[error("invalid citation review request: {0}")]
    Invalid(String),
    #[error(transparent)]
    Research(#[from] ResearchError),
    #[error(transparent)]
    Verification(#[from] CitationVerificationError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl CitationReviewError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid_request",
            Self::Research(ResearchError::NotFound { .. }) => "not_found",
            Self::Research(_) | Self::Verification(_) | Self::Database(_) => "internal_error",
        }
    }
}

#[derive(Clone)]
pub struct CitationReviewService {
    pool: SqlitePool,
    research: Arc<ResearchService>,
    verification: Arc<CitationVerificationService>,
    events: Arc<BroadcastEventBus>,
}

impl CitationReviewService {
    pub fn new(
        pool: SqlitePool,
        research: Arc<ResearchService>,
        verification: Arc<CitationVerificationService>,
        events: Arc<BroadcastEventBus>,
    ) -> Self {
        Self {
            pool,
            research,
            verification,
            events,
        }
    }

    pub async fn start(
        &self,
        input: StartManuscriptCitationReview,
    ) -> Result<CitationReviewRun, CitationReviewError> {
        if input.document_version < 0 || input.document_id.trim().is_empty() {
            return Err(CitationReviewError::Invalid(
                "document identity is invalid".into(),
            ));
        }
        let case = self.research.get_case(&input.research_case_id).await?;
        let source = self
            .research
            .get_source(&input.manuscript_source_id)
            .await?;
        if source.research_case_id != case.id || source.kind != SourceKind::Manuscript {
            return Err(CitationReviewError::Invalid(
                "manuscript_source_id must identify a Manuscript source in the case".into(),
            ));
        }

        let review_id = format!("citation_review_{}", nineprofs_common::new_id());
        let created_at_ms = now_ms();
        sqlx::query(
            "INSERT INTO research_manuscript_citation_review_runs
             (id, research_case_id, manuscript_source_id, document_id, document_version, status, created_at_ms)
             VALUES (?, ?, ?, ?, ?, 'running', ?)",
        )
        .bind(&review_id)
        .bind(&input.research_case_id)
        .bind(&input.manuscript_source_id)
        .bind(&input.document_id)
        .bind(input.document_version)
        .bind(created_at_ms)
        .execute(&self.pool)
        .await?;
        self.publish("research.citation_review.started", &review_id);

        let result = self.run(input, review_id.clone(), created_at_ms).await;
        match result {
            Ok(run) => Ok(run),
            Err((stage, error)) => self.fail(&review_id, &stage, &error).await,
        }
    }

    pub async fn start_manuscript_citation_review(
        &self,
        input: StartManuscriptCitationReview,
    ) -> Result<CitationReviewRun, CitationReviewError> {
        self.start(input).await
    }

    async fn run(
        &self,
        input: StartManuscriptCitationReview,
        review_id: String,
        review_created_at_ms: i64,
    ) -> Result<CitationReviewRun, (String, CitationReviewError)> {
        let sync = self
            .stage_sync(&input)
            .await
            .map_err(|e| ("citation_sync".into(), e))?;
        self.set_stage(&review_id, "citation_sync_run_id", &sync.id.to_string())
            .await
            .map_err(|e| ("persistence".into(), e))?;
        let catalog = self
            .stage_catalog(&input, &sync)
            .await
            .map_err(|e| ("reference_catalog".into(), e))?;
        self.set_stage(
            &review_id,
            "reference_catalog_run_id",
            &catalog.id.to_string(),
        )
        .await
        .map_err(|e| ("persistence".into(), e))?;
        let resolution = self
            .stage_resolution(&input, &catalog)
            .await
            .map_err(|e| ("reference_resolution".into(), e))?;
        self.set_stage(
            &review_id,
            "reference_resolution_run_id",
            &resolution.id.to_string(),
        )
        .await
        .map_err(|e| ("persistence".into(), e))?;
        let extraction = self
            .stage_extraction(&input, &sync)
            .await
            .map_err(|e| ("claim_extraction".into(), e))?;
        self.set_stage(
            &review_id,
            "claim_extraction_run_id",
            &extraction.id.to_string(),
        )
        .await
        .map_err(|e| ("persistence".into(), e))?;

        let items = self
            .project_items(
                &review_id,
                &input,
                &sync,
                &catalog,
                &resolution,
                &extraction,
            )
            .await
            .map_err(|e| ("projection".into(), e))?;
        self.insert_items(&items)
            .await
            .map_err(|e| ("persistence".into(), e))?;

        for item in &items {
            if item.status != CitationReviewItemStatus::ReadyForVerification {
                continue;
            }
            let Some(binding_id) = item.binding_id.clone() else {
                return Err((
                    "projection".into(),
                    CitationReviewError::Invalid(
                        "ready-for-verification item has no persisted binding".into(),
                    ),
                ));
            };
            sqlx::query("UPDATE research_manuscript_citation_review_items SET status = 'verification_running' WHERE id = ?")
                .bind(&item.item_id).execute(&self.pool).await
                .map_err(|e| ("persistence".into(), CitationReviewError::Database(e)))?;
            match self
                .verification
                .verify(CreateCitationVerification {
                    claim_citation_link_id: item.claim_citation_link_id.clone(),
                    citation_target_binding_id: binding_id.clone(),
                })
                .await
            {
                Ok(run) => {
                    sqlx::query("UPDATE research_manuscript_citation_review_items SET status = 'verification_completed', verification_run_id = ? WHERE id = ?")
                        .bind(&run.run_id).bind(&item.item_id).execute(&self.pool).await
                        .map_err(|e| ("persistence".into(), CitationReviewError::Database(e)))?;
                }
                Err(error) => {
                    let failed_run = self
                        .verification
                        .latest_for_link_and_binding_after(
                            &item.claim_citation_link_id,
                            &binding_id,
                            review_created_at_ms,
                        )
                        .await
                        .map_err(CitationReviewError::from)
                        .map_err(|e| ("verification".into(), e))?;
                    sqlx::query("UPDATE research_manuscript_citation_review_items SET status = 'verification_failed', failure_code = ?, verification_run_id = ? WHERE id = ?")
                        .bind(error.code())
                        .bind(failed_run.as_ref().map(|run| run.run_id.as_str()))
                        .bind(&item.item_id).execute(&self.pool).await
                        .map_err(|e| ("persistence".into(), CitationReviewError::Database(e)))?;
                }
            }
        }

        sqlx::query("UPDATE research_manuscript_citation_review_runs SET status = 'completed', completed_at_ms = ? WHERE id = ?")
            .bind(now_ms()).bind(&review_id).execute(&self.pool).await
            .map_err(|e| ("persistence".into(), CitationReviewError::Database(e)))?;
        self.publish("research.citation_review.completed", &review_id);
        self.citation_review(&review_id)
            .await
            .map_err(|e| ("persistence".into(), e))
    }

    async fn stage_sync(
        &self,
        input: &StartManuscriptCitationReview,
    ) -> Result<nineprofs_research::ManuscriptCitationSyncRun, CitationReviewError> {
        let run = self
            .research
            .get_manuscript_citation_sync(&input.citation_sync_run_id)
            .await?;
        if run.status != nineprofs_research::ManuscriptCitationSyncStatus::Completed
            || run.research_case_id.to_string() != input.research_case_id
            || run.manuscript_source_id.to_string() != input.manuscript_source_id
            || run.document_id != input.document_id
            || run.document_version != input.document_version
        {
            return Err(CitationReviewError::Invalid(
                "citation sync run is not the requested completed manuscript version".into(),
            ));
        }
        Ok(run)
    }

    async fn stage_catalog(
        &self,
        input: &StartManuscriptCitationReview,
        sync: &nineprofs_research::ManuscriptCitationSyncRun,
    ) -> Result<nineprofs_research::ManuscriptReferenceCatalogRun, CitationReviewError> {
        let run = self
            .research
            .get_manuscript_reference_catalog(&input.reference_catalog_run_id)
            .await?;
        if run.status != nineprofs_research::ManuscriptReferenceCatalogStatus::Completed
            || run.research_case_id.to_string() != input.research_case_id
            || run.manuscript_source_id.to_string() != input.manuscript_source_id
            || run.citation_sync_run_id != sync.id
            || run.document_id != input.document_id
            || run.document_version != input.document_version
        {
            return Err(CitationReviewError::Invalid(
                "reference catalog run is not the requested completed manuscript version".into(),
            ));
        }
        Ok(run)
    }

    async fn stage_resolution(
        &self,
        input: &StartManuscriptCitationReview,
        catalog: &nineprofs_research::ManuscriptReferenceCatalogRun,
    ) -> Result<nineprofs_research::ManuscriptReferenceResolutionRun, CitationReviewError> {
        let run = self
            .research
            .get_manuscript_reference_resolution(&input.reference_resolution_run_id)
            .await?;
        if run.status != nineprofs_research::ManuscriptReferenceResolutionStatus::Completed
            || run.research_case_id.to_string() != input.research_case_id
            || run.catalog_run_id != catalog.id
        {
            return Err(CitationReviewError::Invalid(
                "reference resolution run is not the requested completed catalog".into(),
            ));
        }
        Ok(run)
    }

    async fn stage_extraction(
        &self,
        input: &StartManuscriptCitationReview,
        sync: &nineprofs_research::ManuscriptCitationSyncRun,
    ) -> Result<nineprofs_research::ManuscriptClaimExtractionRun, CitationReviewError> {
        let run = self
            .research
            .get_manuscript_claim_extraction(&input.claim_extraction_run_id)
            .await?;
        if run.status != nineprofs_research::ManuscriptClaimExtractionStatus::Completed
            || run.research_case_id.to_string() != input.research_case_id
            || run.manuscript_source_id.to_string() != input.manuscript_source_id
            || run.citation_sync_run_id != sync.id
            || run.document_id != input.document_id
            || run.document_version != input.document_version
        {
            return Err(CitationReviewError::Invalid(
                "claim extraction run is not the requested completed manuscript version".into(),
            ));
        }
        Ok(run)
    }

    async fn set_stage(
        &self,
        id: &str,
        column: &str,
        value: &str,
    ) -> Result<(), CitationReviewError> {
        let query = format!(
            "UPDATE research_manuscript_citation_review_runs SET {column} = ? WHERE id = ?"
        );
        sqlx::query(&query)
            .bind(value)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn fail(
        &self,
        id: &str,
        stage: &str,
        error: &CitationReviewError,
    ) -> Result<CitationReviewRun, CitationReviewError> {
        sqlx::query("UPDATE research_manuscript_citation_review_runs SET status = 'failed', failure_stage = ?, failure_code = ?, completed_at_ms = ? WHERE id = ?")
            .bind(stage).bind(error.code()).bind(now_ms()).bind(id).execute(&self.pool).await?;
        self.publish("research.citation_review.failed", id);
        self.citation_review(id).await
    }

    async fn project_items(
        &self,
        review_id: &str,
        input: &StartManuscriptCitationReview,
        sync: &nineprofs_research::ManuscriptCitationSyncRun,
        catalog: &nineprofs_research::ManuscriptReferenceCatalogRun,
        resolution: &nineprofs_research::ManuscriptReferenceResolutionRun,
        extraction: &nineprofs_research::ManuscriptClaimExtractionRun,
    ) -> Result<Vec<CitationReviewItem>, CitationReviewError> {
        let occurrences = self
            .research
            .list_manuscript_citation_sync_occurrences(&sync.id.to_string())
            .await?;
        let mut occurrence_by_id = BTreeMap::new();
        let mut target_by_id = BTreeMap::new();
        let mut occurrence_ordinal = BTreeMap::new();
        for occurrence in occurrences {
            let occurrence_id = occurrence.citation_occurrence_id.to_string();
            occurrence_ordinal.insert(occurrence_id.clone(), occurrence.ordinal);
            occurrence_by_id.insert(occurrence_id.clone(), occurrence.clone());
            for target in self.research.list_citation_targets(&occurrence_id).await? {
                target_by_id.insert(target.id.to_string(), (occurrence_id.clone(), target));
            }
        }

        let mut mapping_by_target = BTreeMap::new();
        for entry in self
            .research
            .list_manuscript_reference_entries(&catalog.id.to_string())
            .await?
        {
            let entry_id = entry.id.to_string();
            for mapping in self
                .research
                .list_manuscript_reference_target_mappings(&entry_id)
                .await?
            {
                mapping_by_target.insert(mapping.citation_target_id.to_string(), entry_id.clone());
            }
        }
        let resolution_entries = self
            .research
            .list_manuscript_reference_resolution_entries(&resolution.id.to_string())
            .await?;
        let mut resolution_by_entry = BTreeMap::new();
        for entry in resolution_entries {
            resolution_by_entry.insert(entry.reference_entry_id.to_string(), entry);
        }

        let extraction_items = self
            .research
            .list_manuscript_claim_extraction_items(&extraction.id.to_string())
            .await?;
        let extraction_item_by_id = extraction_items
            .iter()
            .map(|item| (item.id.to_string(), item))
            .collect::<BTreeMap<_, _>>();
        let coverages = self
            .research
            .list_manuscript_claim_extraction_coverage(&extraction.id.to_string())
            .await?;
        let mut projected = Vec::new();
        let mut seen_items = BTreeSet::new();
        for coverage in coverages {
            if coverage.status != ManuscriptClaimExtractionCoverageStatus::AssociatedWithClaim {
                continue;
            }
            let Some(link_id) = coverage
                .claim_citation_link_id
                .as_ref()
                .map(ToString::to_string)
            else {
                continue;
            };
            let link = self.research.get_claim_citation_link(&link_id).await?;
            if link.research_case_id.to_string() != input.research_case_id
                || link.citation_occurrence_id != coverage.citation_occurrence_id
            {
                return Err(CitationReviewError::Invalid(
                    "claim extraction coverage does not match its claim citation link".into(),
                ));
            }
            let claim = self.research.get_claim(&link.claim_id.to_string()).await?;
            let occurrence_id = link.citation_occurrence_id.to_string();
            let occurrence = self
                .research
                .get_citation_occurrence(&occurrence_id)
                .await?;
            let sync_occurrence = occurrence_by_id.get(&occurrence_id).ok_or_else(|| {
                CitationReviewError::Invalid(
                    "claim citation is absent from the pinned citation sync".into(),
                )
            })?;
            let source_excerpt = coverage.extraction_item_id.as_ref().and_then(|id| {
                extraction_item_by_id
                    .get(&id.to_string())
                    .map(|item| item.source_excerpt.clone())
            });
            let source_start = coverage
                .extraction_item_id
                .as_ref()
                .and_then(|id| {
                    extraction_item_by_id
                        .get(&id.to_string())
                        .map(|item| item.source_start)
                })
                .unwrap_or(sync_occurrence.start);
            for target in target_by_id
                .values()
                .filter(|(id, _)| id == &occurrence_id)
                .map(|(_, target)| target)
            {
                let target_id = target.id.to_string();
                if !seen_items.insert((link.id.to_string(), target_id.clone())) {
                    continue;
                }
                let Some(entry_id) = mapping_by_target.get(&target_id) else {
                    return Err(CitationReviewError::Invalid(
                        "citation target is absent from the pinned reference catalog".into(),
                    ));
                };
                let resolution_entry = resolution_by_entry.get(entry_id).ok_or_else(|| {
                    CitationReviewError::Invalid(
                        "reference entry is absent from the pinned resolution".into(),
                    )
                })?;
                let candidates = self.review_candidates(resolution_entry).await?;
                let binding = match resolution_entry.outcome {
                    ManuscriptReferenceResolutionOutcome::ResolvedExact
                    | ManuscriptReferenceResolutionOutcome::AlreadyBound => self
                        .research
                        .latest_citation_target_binding(&target_id)
                        .await
                        .ok(),
                    _ => None,
                };
                let (status, binding) = review_item_status(&resolution_entry.outcome, binding);
                let binding = binding.filter(|binding| {
                    binding.research_case_id.to_string() == input.research_case_id
                        && binding.citation_target_id.to_string() == target_id
                });
                let (status, binding) = if status == CitationReviewItemStatus::ReadyForVerification
                    && binding.is_none()
                {
                    (CitationReviewItemStatus::BindingConflict, None)
                } else {
                    (status, binding)
                };
                let item = CitationReviewItem {
                    item_id: format!("{review_id}_item_{}", nineprofs_common::new_id()),
                    review_run_id: review_id.to_owned(),
                    ordinal: 0,
                    claim_id: claim.id.to_string(),
                    claim_citation_link_id: link.id.to_string(),
                    citation_occurrence_id: occurrence.id.to_string(),
                    citation_target_id: target_id,
                    reference_entry_id: Some(entry_id.clone()),
                    resolution_entry_id: Some(resolution_entry.id.to_string()),
                    resolution_outcome: Some(resolution_entry.outcome.clone()),
                    document_block_id: sync_occurrence.document_block_id.clone(),
                    start: sync_occurrence.start,
                    end: sync_occurrence.end,
                    rendered_text: occurrence.rendered_text.clone(),
                    reference_key: target.reference_key.clone(),
                    cited_locator: target.cited_locator.clone(),
                    claim_text: claim.text.clone(),
                    source_excerpt: source_excerpt.clone(),
                    binding_id: binding.as_ref().map(|b| b.id.to_string()),
                    binding_method: binding.as_ref().map(|b| b.method.clone()),
                    source_id: binding
                        .as_ref()
                        .map(|b| b.source_id.to_string())
                        .or_else(|| {
                            resolution_entry
                                .chosen_source_id
                                .as_ref()
                                .map(ToString::to_string)
                        }),
                    source_snapshot_id: binding
                        .as_ref()
                        .and_then(|b| b.source_snapshot_id.as_ref().map(ToString::to_string))
                        .or_else(|| {
                            resolution_entry
                                .chosen_source_snapshot_id
                                .as_ref()
                                .map(ToString::to_string)
                        }),
                    extraction_id: binding
                        .as_ref()
                        .and_then(|b| b.extraction_id.as_ref().map(ToString::to_string))
                        .or_else(|| {
                            resolution_entry
                                .chosen_extraction_id
                                .as_ref()
                                .map(ToString::to_string)
                        }),
                    status,
                    failure_code: None,
                    candidates,
                    verification: None,
                    evidence: Vec::new(),
                };
                projected.push((
                    occurrence_ordinal
                        .get(&occurrence_id)
                        .copied()
                        .unwrap_or(u32::MAX),
                    target.ordinal,
                    source_start,
                    item,
                ));
            }
        }
        projected.sort_by(|a, b| {
            (a.0, a.1, a.2, &a.3.claim_citation_link_id).cmp(&(
                b.0,
                b.1,
                b.2,
                &b.3.claim_citation_link_id,
            ))
        });
        Ok(projected
            .into_iter()
            .enumerate()
            .map(|(ordinal, (_, _, _, mut item))| {
                item.ordinal = ordinal as u32;
                item
            })
            .collect())
    }

    async fn review_candidates(
        &self,
        entry: &nineprofs_research::ManuscriptReferenceResolutionEntry,
    ) -> Result<Vec<CitationReviewCandidate>, CitationReviewError> {
        let mut candidates = Vec::new();
        for candidate in self
            .research
            .list_manuscript_reference_resolution_candidates(&entry.id.to_string())
            .await?
        {
            let source_label = self
                .research
                .get_source(&candidate.source_id.to_string())
                .await
                .ok()
                .map(|source| source.label);
            candidates.push(CitationReviewCandidate {
                candidate_id: candidate.id.to_string(),
                resolution_entry_id: entry.id.to_string(),
                ordinal: candidate.ordinal,
                source_id: candidate.source_id.to_string(),
                source_label,
                source_snapshot_id: candidate.source_snapshot_id.map(|id| id.to_string()),
                extraction_id: candidate.extraction_id.map(|id| id.to_string()),
                match_kind: Some(candidate.match_kind),
                automatic_binding_permitted: candidate.automatic_binding_permitted,
            });
        }
        Ok(candidates)
    }

    async fn insert_items(&self, items: &[CitationReviewItem]) -> Result<(), CitationReviewError> {
        let mut tx = self.pool.begin().await?;
        for item in items {
            sqlx::query("INSERT INTO research_manuscript_citation_review_items (id, review_run_id, ordinal, claim_id, claim_citation_link_id, citation_occurrence_id, citation_target_id, reference_entry_id, resolution_entry_id, resolution_outcome, binding_id, binding_method, source_id, source_snapshot_id, extraction_id, document_block_id, start, end, rendered_text, reference_key, cited_locator, claim_text, source_excerpt, status, failure_code) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(&item.item_id).bind(&item.review_run_id).bind(item.ordinal as i64).bind(&item.claim_id).bind(&item.claim_citation_link_id).bind(&item.citation_occurrence_id).bind(&item.citation_target_id).bind(&item.reference_entry_id).bind(&item.resolution_entry_id).bind(item.resolution_outcome.as_ref().map(|v| serde_json::to_string(v).unwrap().trim_matches('"').to_owned())).bind(&item.binding_id).bind(item.binding_method.as_ref().map(|v| serde_json::to_string(v).unwrap().trim_matches('"').to_owned())).bind(&item.source_id).bind(&item.source_snapshot_id).bind(&item.extraction_id).bind(&item.document_block_id).bind(item.start as i64).bind(item.end as i64).bind(&item.rendered_text).bind(&item.reference_key).bind(&item.cited_locator).bind(&item.claim_text).bind(&item.source_excerpt).bind(serde_json::to_string(&item.status).unwrap().trim_matches('"')).bind(&item.failure_code).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn citation_review(
        &self,
        review_id: &str,
    ) -> Result<CitationReviewRun, CitationReviewError> {
        let row = sqlx::query("SELECT id, research_case_id, manuscript_source_id, document_id, document_version, citation_sync_run_id, reference_catalog_run_id, reference_resolution_run_id, claim_extraction_run_id, status, failure_stage, failure_code, created_at_ms, completed_at_ms FROM research_manuscript_citation_review_runs WHERE id = ?").bind(review_id).fetch_optional(&self.pool).await?.ok_or_else(|| CitationReviewError::NotFound(review_id.to_owned()))?;
        Ok(CitationReviewRun {
            review_run_id: row.get("id"),
            research_case_id: row.get("research_case_id"),
            manuscript_source_id: row.get("manuscript_source_id"),
            document_id: row.get("document_id"),
            document_version: row.get("document_version"),
            citation_sync_run_id: row.get("citation_sync_run_id"),
            reference_catalog_run_id: row.get("reference_catalog_run_id"),
            reference_resolution_run_id: row.get("reference_resolution_run_id"),
            claim_extraction_run_id: row.get("claim_extraction_run_id"),
            status: parse_run_status(&row.get::<String, _>("status"))?,
            failure_stage: row.get("failure_stage"),
            failure_code: row.get("failure_code"),
            created_at_ms: row.get("created_at_ms"),
            completed_at_ms: row.get("completed_at_ms"),
        })
    }

    pub async fn get_manuscript_citation_review(
        &self,
        review_id: &str,
    ) -> Result<CitationReviewRun, CitationReviewError> {
        self.citation_review(review_id).await
    }

    pub async fn citation_review_items(
        &self,
        review_id: &str,
    ) -> Result<Vec<CitationReviewItem>, CitationReviewError> {
        let review = self.citation_review(review_id).await?;
        let resolution_entries = match review.reference_resolution_run_id.as_deref() {
            Some(id) => {
                self.research
                    .list_manuscript_reference_resolution_entries(id)
                    .await?
            }
            None => Vec::new(),
        };
        let resolution_entries = resolution_entries
            .into_iter()
            .map(|entry| (entry.id.to_string(), entry))
            .collect::<BTreeMap<_, _>>();
        let rows = sqlx::query("SELECT id, review_run_id, ordinal, claim_id, claim_citation_link_id, citation_occurrence_id, citation_target_id, reference_entry_id, resolution_entry_id, resolution_outcome, binding_id, binding_method, source_id, source_snapshot_id, extraction_id, document_block_id, start, end, rendered_text, reference_key, cited_locator, claim_text, source_excerpt, status, failure_code, verification_run_id FROM research_manuscript_citation_review_items WHERE review_run_id = ? ORDER BY ordinal ASC").bind(review_id).fetch_all(&self.pool).await?;
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let verification_run_id: Option<String> = row.get("verification_run_id");
            let verification = match verification_run_id.as_ref() {
                Some(id) => Some(self.verification_snapshot(id).await?),
                None => None,
            };
            let evidence = match verification_run_id.as_ref() {
                Some(id) => self.verification_evidence(id).await?,
                None => Vec::new(),
            };
            let resolution_entry_id: Option<String> = row.get("resolution_entry_id");
            let candidates = match resolution_entry_id
                .as_deref()
                .and_then(|id| resolution_entries.get(id))
            {
                Some(entry) => self.review_candidates(entry).await?,
                None => Vec::new(),
            };
            items.push(CitationReviewItem {
                item_id: row.get("id"),
                review_run_id: row.get("review_run_id"),
                ordinal: row.get::<i64, _>("ordinal") as u32,
                claim_id: row.get("claim_id"),
                claim_citation_link_id: row.get("claim_citation_link_id"),
                citation_occurrence_id: row.get("citation_occurrence_id"),
                citation_target_id: row.get("citation_target_id"),
                reference_entry_id: row.get("reference_entry_id"),
                resolution_entry_id,
                resolution_outcome: parse_optional_enum(row.get("resolution_outcome"))?,
                document_block_id: row.get("document_block_id"),
                start: row.get::<i64, _>("start") as u64,
                end: row.get::<i64, _>("end") as u64,
                rendered_text: row.get("rendered_text"),
                reference_key: row.get("reference_key"),
                cited_locator: row.get("cited_locator"),
                claim_text: row.get("claim_text"),
                source_excerpt: row.get("source_excerpt"),
                binding_id: row.get("binding_id"),
                binding_method: parse_optional_enum(row.get("binding_method"))?,
                source_id: row.get("source_id"),
                source_snapshot_id: row.get("source_snapshot_id"),
                extraction_id: row.get("extraction_id"),
                status: parse_item_status(&row.get::<String, _>("status"))?,
                failure_code: row.get("failure_code"),
                candidates,
                verification,
                evidence,
            });
        }
        Ok(items)
    }

    pub async fn list_manuscript_citation_review_items(
        &self,
        review_id: &str,
    ) -> Result<Vec<CitationReviewItem>, CitationReviewError> {
        self.citation_review_items(review_id).await
    }

    async fn verification_snapshot(
        &self,
        id: &str,
    ) -> Result<CitationReviewVerification, CitationReviewError> {
        let run = self.verification.citation_verification(id).await?;
        let result = run.result.as_ref();
        Ok(CitationReviewVerification {
            verification_run_id: run.run_id,
            status: run.status,
            failure_code: run.failure_code,
            relation: result.map(|r| r.overall_relation.clone()),
            rationale: result.map(|r| r.rationale.clone()),
            assessor_provider: result.map(|r| r.assessor_provider.clone()),
            assessor_version: result.map(|r| r.assessor_version.clone()),
            assessor_model_id: result.and_then(|r| r.assessor_model_id.clone()),
            completed_at_ms: run.completed_at_ms,
        })
    }

    async fn verification_evidence(
        &self,
        id: &str,
    ) -> Result<Vec<CitationReviewEvidence>, CitationReviewError> {
        let run = self.verification.citation_verification(id).await?;
        let mut evidence = Vec::new();
        for mapping in run.evidence {
            let value = self.research.get_evidence(&mapping.evidence_id).await?;
            evidence.push(CitationReviewEvidence {
                evidence_id: value.id.to_string(),
                relation: mapping.relation,
                source_snapshot_id: value.source_snapshot_id.to_string(),
                extraction_id: value.pdf_extraction_id.map(|id| id.to_string()),
                locator: value.locator,
                verbatim_excerpt: value.verbatim_excerpt,
            });
        }
        Ok(evidence)
    }

    fn publish(&self, event: &str, review_id: &str) {
        let _ = self.events.publish(EventEnvelope::new(
            event,
            serde_json::json!({ "reviewRunId": review_id }),
        ));
    }
}

fn review_item_status(
    outcome: &ManuscriptReferenceResolutionOutcome,
    binding: Option<nineprofs_research::CitationTargetBinding>,
) -> (
    CitationReviewItemStatus,
    Option<nineprofs_research::CitationTargetBinding>,
) {
    match outcome {
        ManuscriptReferenceResolutionOutcome::ResolvedExact
        | ManuscriptReferenceResolutionOutcome::AlreadyBound => match binding.as_ref() {
            Some(binding) if binding.extraction_id.is_some() => (
                CitationReviewItemStatus::ReadyForVerification,
                Some(binding.clone()),
            ),
            Some(binding) => (
                CitationReviewItemStatus::SourceMatchedNotVerificationReady,
                Some(binding.clone()),
            ),
            None => (CitationReviewItemStatus::BindingConflict, None),
        },
        ManuscriptReferenceResolutionOutcome::AmbiguousSource
        | ManuscriptReferenceResolutionOutcome::AmbiguousSnapshotOrExtraction => {
            (CitationReviewItemStatus::AmbiguousReference, None)
        }
        ManuscriptReferenceResolutionOutcome::CandidateRequiresConfirmation => (
            CitationReviewItemStatus::ReferenceRequiresConfirmation,
            None,
        ),
        ManuscriptReferenceResolutionOutcome::SourceMatchedButNotVerificationReady => (
            CitationReviewItemStatus::SourceMatchedNotVerificationReady,
            binding,
        ),
        ManuscriptReferenceResolutionOutcome::ConflictWithExistingBinding => {
            (CitationReviewItemStatus::BindingConflict, None)
        }
        ManuscriptReferenceResolutionOutcome::Unresolved => {
            (CitationReviewItemStatus::UnresolvedReference, None)
        }
        ManuscriptReferenceResolutionOutcome::Failed => {
            (CitationReviewItemStatus::ResolutionFailed, None)
        }
    }
}

fn parse_run_status(value: &str) -> Result<CitationReviewRunStatus, CitationReviewError> {
    match value {
        "running" => Ok(CitationReviewRunStatus::Running),
        "completed" => Ok(CitationReviewRunStatus::Completed),
        "failed" => Ok(CitationReviewRunStatus::Failed),
        _ => Err(CitationReviewError::Invalid(
            "invalid citation review status".into(),
        )),
    }
}

fn parse_item_status(value: &str) -> Result<CitationReviewItemStatus, CitationReviewError> {
    serde_json::from_str(&format!("\"{value}\""))
        .map_err(|_| CitationReviewError::Invalid("invalid citation review item status".into()))
}

fn parse_optional_enum<T: for<'de> Deserialize<'de>>(
    value: Option<String>,
) -> Result<Option<T>, CitationReviewError> {
    value
        .map(|value| {
            serde_json::from_str(&format!("\"{value}\"")).map_err(|_| {
                CitationReviewError::Invalid("invalid persisted citation review enum".into())
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nineprofs_research::ManuscriptReferenceResolutionOutcome as O;

    #[test]
    fn resolution_outcomes_preserve_review_taxonomy() {
        assert_eq!(
            review_item_status(&O::Unresolved, None).0,
            CitationReviewItemStatus::UnresolvedReference
        );
        assert_eq!(
            review_item_status(&O::AmbiguousSource, None).0,
            CitationReviewItemStatus::AmbiguousReference
        );
        assert_eq!(
            review_item_status(&O::CandidateRequiresConfirmation, None).0,
            CitationReviewItemStatus::ReferenceRequiresConfirmation
        );
        assert_eq!(
            review_item_status(&O::SourceMatchedButNotVerificationReady, None).0,
            CitationReviewItemStatus::SourceMatchedNotVerificationReady
        );
        assert_eq!(
            review_item_status(&O::AmbiguousSnapshotOrExtraction, None).0,
            CitationReviewItemStatus::AmbiguousReference
        );
        assert_eq!(
            review_item_status(&O::ConflictWithExistingBinding, None).0,
            CitationReviewItemStatus::BindingConflict
        );
        assert_eq!(
            review_item_status(&O::Failed, None).0,
            CitationReviewItemStatus::ResolutionFailed
        );
    }
}
