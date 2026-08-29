use std::collections::BTreeMap;

use nineprofs_research::{
    ClaimEvidenceRelation, ManuscriptCitationSyncStatus, ManuscriptClaimExtractionCoverageStatus,
    ManuscriptClaimExtractionStatus, ManuscriptClaimInventoryStatus,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sqlx::Row;

use crate::{
    CitationReviewError, CitationReviewEvidence, CitationReviewItemStatus, CitationReviewRun,
    CitationReviewRunStatus, CitationVerificationStatus,
};

pub const MANUSCRIPT_CLAIM_COVERAGE_ANALYSIS_CONTRACT_VERSION: &str =
    "manuscript-claim-coverage-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimCoverageRunStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimCoverageBridgeStatus {
    ExactClaimBridge,
    NoCitationScopedClaimMatch,
    SameSpanDifferentClaim,
    MultipleExactCandidates,
    InvalidCrossHistory,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimCoverageStructuralCitationState {
    ExactCitationLinked,
    CitationObservedInClaimRange,
    CitationObservedInBlock,
    NoCitationObservedInBlock,
    AmbiguousClaimBridge,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartManuscriptClaimCoverage {
    pub research_case_id: String,
    pub claim_inventory_run_id: String,
    pub citation_review_run_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptClaimCoverageRun {
    pub coverage_run_id: String,
    pub research_case_id: String,
    pub manuscript_source_id: String,
    pub document_id: String,
    pub document_version: i64,
    pub claim_inventory_run_id: String,
    pub citation_review_run_id: String,
    pub analysis_contract_version: String,
    pub coverage_contract_version: String,
    pub coverage_scope: String,
    pub coverage_limitations: Vec<String>,
    pub status: ManuscriptClaimCoverageRunStatus,
    pub item_count: u32,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptClaimCoverageItem {
    pub coverage_item_id: String,
    pub coverage_run_id: String,
    pub inventory_item_id: String,
    pub ordinal: u32,
    pub bridge_status: ManuscriptClaimCoverageBridgeStatus,
    pub structural_citation_state: ManuscriptClaimCoverageStructuralCitationState,
    pub matched_claim_extraction_item_id: Option<String>,
    pub matched_research_claim_id: Option<String>,
    pub inventory_overlapping_citation_count: u32,
    pub same_block_citation_count: u32,
    pub claim_range_citation_count: u32,
    pub exact_claim_citation_link_count: u32,
    pub target_count: u32,
    pub support_count: u32,
    pub contradiction_count: u32,
    pub contextualize_count: u32,
    pub insufficient_count: u32,
    pub unverified_count: u32,
    pub blocked_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManuscriptClaimCoverageTarget {
    pub coverage_target_id: String,
    pub coverage_item_id: String,
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
    pub evidence: Vec<CitationReviewEvidence>,
}

#[derive(Clone, Debug)]
struct BuiltCoverage {
    run: ManuscriptClaimCoverageRun,
    items: Vec<ManuscriptClaimCoverageItem>,
    targets: Vec<ManuscriptClaimCoverageTarget>,
}

impl super::CitationReviewService {
    pub async fn start_manuscript_claim_coverage(
        &self,
        input: StartManuscriptClaimCoverage,
    ) -> Result<ManuscriptClaimCoverageRun, CitationReviewError> {
        if let Some(existing) = self
            .find_completed_manuscript_claim_coverage(
                &input.research_case_id,
                &input.claim_inventory_run_id,
                &input.citation_review_run_id,
            )
            .await?
        {
            return Ok(existing);
        }

        let built = self.build_manuscript_claim_coverage(&input).await?;
        self.persist_manuscript_claim_coverage(&built).await
    }

    pub async fn get_manuscript_claim_coverage(
        &self,
        coverage_run_id: &str,
    ) -> Result<ManuscriptClaimCoverageRun, CitationReviewError> {
        let row = sqlx::query(
            "SELECT coverage_run_id, research_case_id, manuscript_source_id, document_id,
             document_version, claim_inventory_run_id, citation_review_run_id,
             analysis_contract_version, coverage_contract_version, coverage_scope,
             coverage_limitations_json, status, item_count, created_at_ms, completed_at_ms
             FROM research_manuscript_claim_coverage_runs WHERE coverage_run_id = ?",
        )
        .bind(coverage_run_id)
        .fetch_optional(self.pool())
        .await?
        .ok_or_else(|| CitationReviewError::NotFound(coverage_run_id.to_owned()))?;

        Ok(ManuscriptClaimCoverageRun {
            coverage_run_id: row.get("coverage_run_id"),
            research_case_id: row.get("research_case_id"),
            manuscript_source_id: row.get("manuscript_source_id"),
            document_id: row.get("document_id"),
            document_version: row.get("document_version"),
            claim_inventory_run_id: row.get("claim_inventory_run_id"),
            citation_review_run_id: row.get("citation_review_run_id"),
            analysis_contract_version: row.get("analysis_contract_version"),
            coverage_contract_version: row.get("coverage_contract_version"),
            coverage_scope: row.get("coverage_scope"),
            coverage_limitations: parse_json(
                row.get("coverage_limitations_json"),
                "coverage limitations",
            )?,
            status: parse_enum(row.get::<String, _>("status"), "coverage run status")?,
            item_count: row.get::<i64, _>("item_count") as u32,
            created_at_ms: row.get("created_at_ms"),
            completed_at_ms: row.get("completed_at_ms"),
        })
    }

    pub async fn list_manuscript_claim_coverage_items(
        &self,
        coverage_run_id: &str,
    ) -> Result<Vec<ManuscriptClaimCoverageItem>, CitationReviewError> {
        let run = self.get_manuscript_claim_coverage(coverage_run_id).await?;
        let rows = sqlx::query(
            "SELECT coverage_item_id, coverage_run_id, inventory_item_id, ordinal,
             bridge_status, structural_citation_state, matched_claim_extraction_item_id,
             matched_research_claim_id, inventory_overlapping_citation_count,
             same_block_citation_count, claim_range_citation_count,
             exact_claim_citation_link_count, target_count, support_count,
             contradiction_count, contextualize_count, insufficient_count,
             unverified_count, blocked_count
             FROM research_manuscript_claim_coverage_items
             WHERE coverage_run_id = ? ORDER BY ordinal ASC",
        )
        .bind(&run.coverage_run_id)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(map_coverage_item).collect()
    }

    pub async fn list_manuscript_claim_coverage_targets(
        &self,
        coverage_run_id: &str,
        coverage_item_id: &str,
    ) -> Result<Vec<ManuscriptClaimCoverageTarget>, CitationReviewError> {
        let run = self.get_manuscript_claim_coverage(coverage_run_id).await?;
        let item_exists = sqlx::query(
            "SELECT 1 FROM research_manuscript_claim_coverage_items
             WHERE coverage_run_id = ? AND coverage_item_id = ?",
        )
        .bind(&run.coverage_run_id)
        .bind(coverage_item_id)
        .fetch_optional(self.pool())
        .await?
        .is_some();
        if !item_exists {
            return Err(CitationReviewError::NotFound(coverage_item_id.to_owned()));
        }

        let review_items = self
            .citation_review_items(&run.citation_review_run_id)
            .await?;
        let review_by_id = review_items
            .into_iter()
            .map(|item| (item.item_id.clone(), item))
            .collect::<BTreeMap<_, _>>();
        let rows = sqlx::query(
            "SELECT coverage_target_id, coverage_item_id, claim_citation_link_id,
             citation_occurrence_id, citation_target_id, citation_review_item_id,
             binding_id, source_id, source_snapshot_id, extraction_id,
             verification_run_id, review_status, failure_code, verification_status,
             verification_failure_code, relation, rationale, evidence_count
             FROM research_manuscript_claim_coverage_targets
             WHERE coverage_item_id = ? ORDER BY citation_target_id ASC",
        )
        .bind(coverage_item_id)
        .fetch_all(self.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                let mut target = map_coverage_target(row)?;
                let Some(review_item) = review_by_id.get(&target.citation_review_item_id) else {
                    return Err(CitationReviewError::Invalid(
                        "coverage target references missing citation review item".to_owned(),
                    ));
                };
                target.evidence = review_item.evidence.clone();
                Ok(target)
            })
            .collect()
    }

    async fn find_completed_manuscript_claim_coverage(
        &self,
        research_case_id: &str,
        claim_inventory_run_id: &str,
        citation_review_run_id: &str,
    ) -> Result<Option<ManuscriptClaimCoverageRun>, CitationReviewError> {
        let row = sqlx::query(
            "SELECT coverage_run_id FROM research_manuscript_claim_coverage_runs
             WHERE research_case_id = ? AND claim_inventory_run_id = ?
             AND citation_review_run_id = ?
             AND analysis_contract_version = ? AND status = 'completed'
             ORDER BY created_at_ms DESC, coverage_run_id DESC LIMIT 1",
        )
        .bind(research_case_id)
        .bind(claim_inventory_run_id)
        .bind(citation_review_run_id)
        .bind(MANUSCRIPT_CLAIM_COVERAGE_ANALYSIS_CONTRACT_VERSION)
        .fetch_optional(self.pool())
        .await?;
        match row {
            Some(row) => self
                .get_manuscript_claim_coverage(&row.get::<String, _>("coverage_run_id"))
                .await
                .map(Some),
            None => Ok(None),
        }
    }

    async fn build_manuscript_claim_coverage(
        &self,
        input: &StartManuscriptClaimCoverage,
    ) -> Result<BuiltCoverage, CitationReviewError> {
        let inventory = self
            .research_service()
            .get_manuscript_claim_inventory(&input.claim_inventory_run_id)
            .await?;
        let review = self.citation_review(&input.citation_review_run_id).await?;
        let sync_id = review.citation_sync_run_id.as_deref().ok_or_else(|| {
            CitationReviewError::Invalid(
                "completed citation review has no citation sync run".to_owned(),
            )
        })?;
        let extraction_id = review.claim_extraction_run_id.as_deref().ok_or_else(|| {
            CitationReviewError::Invalid(
                "completed citation review has no claim extraction run".to_owned(),
            )
        })?;
        let sync = self
            .research_service()
            .get_manuscript_citation_sync(sync_id)
            .await?;
        let extraction = self
            .research_service()
            .get_manuscript_claim_extraction(extraction_id)
            .await?;
        validate_compatibility(input, &inventory, &review, &sync, &extraction)?;

        let inventory_items = self
            .research_service()
            .list_manuscript_claim_inventory_items(&input.claim_inventory_run_id)
            .await?;
        let extraction_items = self
            .research_service()
            .list_manuscript_claim_extraction_items(extraction_id)
            .await?;
        let extraction_coverage = self
            .research_service()
            .list_manuscript_claim_extraction_coverage(extraction_id)
            .await?;
        let sync_occurrences = self
            .research_service()
            .list_manuscript_citation_sync_occurrences(sync_id)
            .await?;
        let review_items = self.citation_review_items(&review.review_run_id).await?;

        let mut claims = BTreeMap::new();
        for extraction_item in &extraction_items {
            if extraction_item.extraction_run_id != extraction.id {
                return Err(CitationReviewError::Invalid(
                    "claim extraction item crosses extraction history".to_owned(),
                ));
            }
            claims.insert(
                extraction_item.research_claim_id.to_string(),
                self.research_service()
                    .get_claim(extraction_item.research_claim_id.as_str())
                    .await?,
            );
        }
        let sync_by_occurrence = sync_occurrences
            .iter()
            .map(|occurrence| {
                if occurrence.sync_run_id != sync.id {
                    return Err(CitationReviewError::Invalid(
                        "citation occurrence crosses citation sync history".to_owned(),
                    ));
                }
                Ok((occurrence.citation_occurrence_id.to_string(), occurrence))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let review_by_link_target = review_items
            .iter()
            .map(|item| {
                (
                    (
                        item.claim_citation_link_id.clone(),
                        item.citation_target_id.clone(),
                    ),
                    item,
                )
            })
            .collect::<BTreeMap<_, _>>();

        let mut items = Vec::with_capacity(inventory_items.len());
        let mut targets = Vec::new();
        for inventory_item in inventory_items {
            let same_span = extraction_items
                .iter()
                .filter(|candidate| {
                    candidate.document_block_id == inventory_item.document_block_id
                        && candidate.source_start == inventory_item.source_start
                        && candidate.source_end == inventory_item.source_end
                        && candidate.source_excerpt == inventory_item.source_excerpt
                        && candidate.source_excerpt_hash == inventory_item.source_excerpt_hash
                })
                .collect::<Vec<_>>();
            let bridge_status = match same_span.as_slice() {
                [] => ManuscriptClaimCoverageBridgeStatus::NoCitationScopedClaimMatch,
                [candidate] => {
                    let claim = claims
                        .get(candidate.research_claim_id.as_str())
                        .ok_or_else(|| {
                            CitationReviewError::Invalid(
                                "claim extraction item has no ResearchClaim".to_owned(),
                            )
                        })?;
                    if claim.text == inventory_item.claim_text {
                        ManuscriptClaimCoverageBridgeStatus::ExactClaimBridge
                    } else {
                        ManuscriptClaimCoverageBridgeStatus::SameSpanDifferentClaim
                    }
                }
                _ => ManuscriptClaimCoverageBridgeStatus::MultipleExactCandidates,
            };
            let exact_candidate =
                if bridge_status == ManuscriptClaimCoverageBridgeStatus::ExactClaimBridge {
                    same_span.first().copied()
                } else {
                    None
                };
            let matched_claim = exact_candidate
                .and_then(|candidate| claims.get(candidate.research_claim_id.as_str()));
            let same_block_citation_count = sync_occurrences
                .iter()
                .filter(|occurrence| {
                    occurrence.document_block_id == inventory_item.document_block_id
                })
                .count() as u32;
            let claim_range_citation_count = sync_occurrences
                .iter()
                .filter(|occurrence| {
                    occurrence.document_block_id == inventory_item.document_block_id
                        && occurrence.start < inventory_item.source_end
                        && inventory_item.source_start < occurrence.end
                })
                .count() as u32;
            let mut coverage_item = ManuscriptClaimCoverageItem {
                coverage_item_id: format!(
                    "manuscript_claim_coverage_item_{}",
                    nineprofs_common::new_id()
                ),
                coverage_run_id: String::new(),
                inventory_item_id: inventory_item.id.to_string(),
                ordinal: inventory_item.ordinal,
                bridge_status,
                structural_citation_state:
                    ManuscriptClaimCoverageStructuralCitationState::NoCitationObservedInBlock,
                matched_claim_extraction_item_id: exact_candidate
                    .map(|candidate| candidate.id.to_string()),
                matched_research_claim_id: matched_claim.map(|claim| claim.id.to_string()),
                inventory_overlapping_citation_count: inventory_item.overlapping_citation_count,
                same_block_citation_count,
                claim_range_citation_count,
                exact_claim_citation_link_count: 0,
                target_count: 0,
                support_count: 0,
                contradiction_count: 0,
                contextualize_count: 0,
                insufficient_count: 0,
                unverified_count: 0,
                blocked_count: 0,
            };

            let mut exact_links = Vec::new();
            if let Some(extraction_item) = exact_candidate {
                for coverage in extraction_coverage.iter().filter(|coverage| {
                    coverage.extraction_item_id.as_ref() == Some(&extraction_item.id)
                }) {
                    if coverage.status
                        != ManuscriptClaimExtractionCoverageStatus::AssociatedWithClaim
                    {
                        continue;
                    }
                    let link_id = coverage.claim_citation_link_id.as_ref().ok_or_else(|| {
                        CitationReviewError::Invalid(
                            "associated claim extraction coverage has no ClaimCitationLink"
                                .to_owned(),
                        )
                    })?;
                    let link = self
                        .research_service()
                        .get_claim_citation_link(link_id.as_str())
                        .await?;
                    if link.claim_id != extraction_item.research_claim_id
                        || link.research_case_id != inventory.research_case_id
                        || link.citation_occurrence_id.to_string()
                            != coverage.citation_occurrence_id.to_string()
                    {
                        return Err(CitationReviewError::Invalid(
                            "ClaimCitationLink crosses claim extraction history".to_owned(),
                        ));
                    }
                    if !sync_by_occurrence.contains_key(&link.citation_occurrence_id.to_string()) {
                        return Err(CitationReviewError::Invalid(
                            "ClaimCitationLink occurrence is outside selected citation sync"
                                .to_owned(),
                        ));
                    }
                    if !exact_links
                        .iter()
                        .any(|id: &String| id == &link.id.to_string())
                    {
                        exact_links.push(link.id.to_string());
                    }
                }
            }
            coverage_item.exact_claim_citation_link_count = exact_links.len() as u32;
            let ambiguous = !matches!(
                coverage_item.bridge_status,
                ManuscriptClaimCoverageBridgeStatus::ExactClaimBridge
                    | ManuscriptClaimCoverageBridgeStatus::NoCitationScopedClaimMatch
            );
            coverage_item.structural_citation_state = if ambiguous {
                ManuscriptClaimCoverageStructuralCitationState::AmbiguousClaimBridge
            } else if coverage_item.exact_claim_citation_link_count > 0 {
                ManuscriptClaimCoverageStructuralCitationState::ExactCitationLinked
            } else if claim_range_citation_count > 0 {
                ManuscriptClaimCoverageStructuralCitationState::CitationObservedInClaimRange
            } else if same_block_citation_count > 0 {
                ManuscriptClaimCoverageStructuralCitationState::CitationObservedInBlock
            } else {
                ManuscriptClaimCoverageStructuralCitationState::NoCitationObservedInBlock
            };

            for link_id in exact_links {
                let link = self
                    .research_service()
                    .get_claim_citation_link(&link_id)
                    .await?;
                let occurrence_id = link.citation_occurrence_id.to_string();
                let occurrence = sync_by_occurrence.get(&occurrence_id).ok_or_else(|| {
                    CitationReviewError::Invalid(
                        "linked citation occurrence is outside selected sync".to_owned(),
                    )
                })?;
                let citation_targets = self
                    .research_service()
                    .list_citation_targets(&occurrence_id)
                    .await?;
                for citation_target in citation_targets {
                    let review_item = review_by_link_target
                        .get(&(link_id.clone(), citation_target.id.to_string()))
                        .ok_or_else(|| {
                            CitationReviewError::Invalid(
                                "exact ClaimCitationLink target is absent from Citation Review"
                                    .to_owned(),
                            )
                        })?;
                    if review_item.claim_id != link.claim_id.to_string()
                        || review_item.citation_occurrence_id != occurrence_id
                        || review_item.citation_target_id != citation_target.id.to_string()
                    {
                        return Err(CitationReviewError::Invalid(
                            "Citation Review target crosses selected claim link".to_owned(),
                        ));
                    }
                    coverage_item.target_count += 1;
                    if let Some(verification) = review_item.verification.as_ref() {
                        if verification.status == CitationVerificationStatus::Completed {
                            match verification.relation {
                                Some(ClaimEvidenceRelation::Supports) => {
                                    coverage_item.support_count += 1
                                }
                                Some(ClaimEvidenceRelation::Contradicts) => {
                                    coverage_item.contradiction_count += 1
                                }
                                Some(ClaimEvidenceRelation::Contextualizes) => {
                                    coverage_item.contextualize_count += 1
                                }
                                Some(ClaimEvidenceRelation::Insufficient) => {
                                    coverage_item.insufficient_count += 1
                                }
                                None => coverage_item.unverified_count += 1,
                            }
                        } else {
                            coverage_item.unverified_count += 1;
                        }
                    } else {
                        coverage_item.unverified_count += 1;
                    }
                    if is_blocked_status(&review_item.status) {
                        coverage_item.blocked_count += 1;
                    }
                    targets.push(ManuscriptClaimCoverageTarget {
                        coverage_target_id: format!(
                            "manuscript_claim_coverage_target_{}",
                            nineprofs_common::new_id()
                        ),
                        coverage_item_id: coverage_item.coverage_item_id.clone(),
                        claim_citation_link_id: link_id.clone(),
                        citation_occurrence_id: occurrence.citation_occurrence_id.to_string(),
                        citation_target_id: citation_target.id.to_string(),
                        citation_review_item_id: review_item.item_id.clone(),
                        binding_id: review_item.binding_id.clone(),
                        source_id: review_item.source_id.clone(),
                        source_snapshot_id: review_item.source_snapshot_id.clone(),
                        extraction_id: review_item.extraction_id.clone(),
                        verification_run_id: review_item
                            .verification
                            .as_ref()
                            .map(|v| v.verification_run_id.clone()),
                        review_status: review_item.status.clone(),
                        failure_code: review_item.failure_code.clone(),
                        verification_status: review_item
                            .verification
                            .as_ref()
                            .map(|v| v.status.clone()),
                        verification_failure_code: review_item
                            .verification
                            .as_ref()
                            .and_then(|v| v.failure_code.clone()),
                        relation: review_item
                            .verification
                            .as_ref()
                            .and_then(|v| v.relation.clone()),
                        rationale: review_item
                            .verification
                            .as_ref()
                            .and_then(|v| v.rationale.clone()),
                        evidence_count: review_item.evidence.len() as u32,
                        evidence: review_item.evidence.clone(),
                    });
                }
            }
            items.push(coverage_item);
        }

        let coverage_run_id = format!("manuscript_claim_coverage_{}", nineprofs_common::new_id());
        for item in &mut items {
            item.coverage_run_id = coverage_run_id.clone();
        }
        let created_at_ms = nineprofs_common::now_ms();
        Ok(BuiltCoverage {
            run: ManuscriptClaimCoverageRun {
                coverage_run_id,
                research_case_id: inventory.research_case_id.to_string(),
                manuscript_source_id: inventory.manuscript_source_id.to_string(),
                document_id: inventory.document_id,
                document_version: inventory.document_version,
                claim_inventory_run_id: inventory.id.to_string(),
                citation_review_run_id: review.review_run_id,
                analysis_contract_version: MANUSCRIPT_CLAIM_COVERAGE_ANALYSIS_CONTRACT_VERSION
                    .to_owned(),
                coverage_contract_version: inventory.coverage_contract_version,
                coverage_scope: inventory.coverage_scope,
                coverage_limitations: inventory.coverage_limitations,
                status: ManuscriptClaimCoverageRunStatus::Completed,
                item_count: items.len() as u32,
                created_at_ms,
                completed_at_ms: Some(created_at_ms),
            },
            items,
            targets,
        })
    }

    async fn persist_manuscript_claim_coverage(
        &self,
        built: &BuiltCoverage,
    ) -> Result<ManuscriptClaimCoverageRun, CitationReviewError> {
        let mut tx = self.pool().begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO research_manuscript_claim_coverage_runs
             (coverage_run_id, research_case_id, manuscript_source_id, document_id,
              document_version, claim_inventory_run_id, citation_review_run_id,
              analysis_contract_version, coverage_contract_version, coverage_scope,
              coverage_limitations_json, status, item_count, created_at_ms, completed_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT DO NOTHING",
        )
        .bind(&built.run.coverage_run_id)
        .bind(&built.run.research_case_id)
        .bind(&built.run.manuscript_source_id)
        .bind(&built.run.document_id)
        .bind(built.run.document_version)
        .bind(&built.run.claim_inventory_run_id)
        .bind(&built.run.citation_review_run_id)
        .bind(&built.run.analysis_contract_version)
        .bind(&built.run.coverage_contract_version)
        .bind(&built.run.coverage_scope)
        .bind(json_text(&built.run.coverage_limitations))
        .bind(enum_text(&built.run.status))
        .bind(built.run.item_count as i64)
        .bind(built.run.created_at_ms)
        .bind(built.run.completed_at_ms)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() == 0 {
            tx.rollback().await?;
            return self
                .find_completed_manuscript_claim_coverage(
                    &built.run.research_case_id,
                    &built.run.claim_inventory_run_id,
                    &built.run.citation_review_run_id,
                )
                .await?
                .ok_or_else(|| {
                    CitationReviewError::Invalid("coverage identity conflict".to_owned())
                });
        }

        for item in &built.items {
            sqlx::query(
                "INSERT INTO research_manuscript_claim_coverage_items
                 (coverage_item_id, coverage_run_id, inventory_item_id, ordinal,
                  bridge_status, structural_citation_state, matched_claim_extraction_item_id,
                  matched_research_claim_id, inventory_overlapping_citation_count,
                  same_block_citation_count, claim_range_citation_count,
                  exact_claim_citation_link_count, target_count, support_count,
                  contradiction_count, contextualize_count, insufficient_count,
                  unverified_count, blocked_count)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&item.coverage_item_id)
            .bind(&item.coverage_run_id)
            .bind(&item.inventory_item_id)
            .bind(item.ordinal as i64)
            .bind(enum_text(&item.bridge_status))
            .bind(enum_text(&item.structural_citation_state))
            .bind(&item.matched_claim_extraction_item_id)
            .bind(&item.matched_research_claim_id)
            .bind(item.inventory_overlapping_citation_count as i64)
            .bind(item.same_block_citation_count as i64)
            .bind(item.claim_range_citation_count as i64)
            .bind(item.exact_claim_citation_link_count as i64)
            .bind(item.target_count as i64)
            .bind(item.support_count as i64)
            .bind(item.contradiction_count as i64)
            .bind(item.contextualize_count as i64)
            .bind(item.insufficient_count as i64)
            .bind(item.unverified_count as i64)
            .bind(item.blocked_count as i64)
            .execute(&mut *tx)
            .await?;
        }
        for target in &built.targets {
            sqlx::query(
                "INSERT INTO research_manuscript_claim_coverage_targets
                 (coverage_target_id, coverage_item_id, claim_citation_link_id,
                  citation_occurrence_id, citation_target_id, citation_review_item_id,
                  binding_id, source_id, source_snapshot_id, extraction_id,
                  verification_run_id, review_status, failure_code, verification_status,
                  verification_failure_code, relation, rationale, evidence_count)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&target.coverage_target_id)
            .bind(&target.coverage_item_id)
            .bind(&target.claim_citation_link_id)
            .bind(&target.citation_occurrence_id)
            .bind(&target.citation_target_id)
            .bind(&target.citation_review_item_id)
            .bind(&target.binding_id)
            .bind(&target.source_id)
            .bind(&target.source_snapshot_id)
            .bind(&target.extraction_id)
            .bind(&target.verification_run_id)
            .bind(enum_text(&target.review_status))
            .bind(&target.failure_code)
            .bind(target.verification_status.as_ref().map(enum_text))
            .bind(&target.verification_failure_code)
            .bind(target.relation.as_ref().map(enum_text))
            .bind(&target.rationale)
            .bind(target.evidence_count as i64)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.get_manuscript_claim_coverage(&built.run.coverage_run_id)
            .await
    }
}

fn validate_compatibility(
    input: &StartManuscriptClaimCoverage,
    inventory: &nineprofs_research::ManuscriptClaimInventoryRun,
    review: &CitationReviewRun,
    sync: &nineprofs_research::ManuscriptCitationSyncRun,
    extraction: &nineprofs_research::ManuscriptClaimExtractionRun,
) -> Result<(), CitationReviewError> {
    if !matches!(&inventory.status, ManuscriptClaimInventoryStatus::Completed)
        || !matches!(&review.status, CitationReviewRunStatus::Completed)
        || !matches!(&sync.status, ManuscriptCitationSyncStatus::Completed)
        || !matches!(
            &extraction.status,
            ManuscriptClaimExtractionStatus::Completed
        )
    {
        return Err(CitationReviewError::Invalid(
            "coverage requires completed inventory, citation review, sync, and extraction runs"
                .to_owned(),
        ));
    }
    if input.research_case_id != inventory.research_case_id.to_string()
        || input.research_case_id != review.research_case_id
        || inventory.research_case_id.to_string() != review.research_case_id
        || inventory.manuscript_source_id.to_string() != review.manuscript_source_id
        || inventory.document_id != review.document_id
        || inventory.document_version != review.document_version
        || sync.research_case_id != inventory.research_case_id
        || sync.manuscript_source_id != inventory.manuscript_source_id
        || sync.document_id != inventory.document_id
        || sync.document_version != inventory.document_version
        || extraction.research_case_id != inventory.research_case_id
        || extraction.manuscript_source_id != inventory.manuscript_source_id
        || extraction.document_id != inventory.document_id
        || extraction.document_version != inventory.document_version
        || extraction.citation_sync_run_id != sync.id
    {
        return Err(CitationReviewError::Invalid(
            "inventory and citation review histories are incompatible".to_owned(),
        ));
    }
    if review.citation_sync_run_id.as_deref() != Some(sync.id.as_str())
        || review.claim_extraction_run_id.as_deref() != Some(extraction.id.as_str())
    {
        return Err(CitationReviewError::Invalid(
            "citation review does not pin selected citation histories".to_owned(),
        ));
    }
    Ok(())
}

fn is_blocked_status(status: &CitationReviewItemStatus) -> bool {
    matches!(
        status,
        CitationReviewItemStatus::UnresolvedReference
            | CitationReviewItemStatus::AmbiguousReference
            | CitationReviewItemStatus::ReferenceRequiresConfirmation
            | CitationReviewItemStatus::SourceMatchedNotVerificationReady
            | CitationReviewItemStatus::BindingConflict
            | CitationReviewItemStatus::ResolutionFailed
    )
}

fn map_coverage_item(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptClaimCoverageItem, CitationReviewError> {
    Ok(ManuscriptClaimCoverageItem {
        coverage_item_id: row.get("coverage_item_id"),
        coverage_run_id: row.get("coverage_run_id"),
        inventory_item_id: row.get("inventory_item_id"),
        ordinal: row.get::<i64, _>("ordinal") as u32,
        bridge_status: parse_enum(row.get::<String, _>("bridge_status"), "bridge status")?,
        structural_citation_state: parse_enum(
            row.get::<String, _>("structural_citation_state"),
            "structural citation state",
        )?,
        matched_claim_extraction_item_id: row.get("matched_claim_extraction_item_id"),
        matched_research_claim_id: row.get("matched_research_claim_id"),
        inventory_overlapping_citation_count: row
            .get::<i64, _>("inventory_overlapping_citation_count")
            as u32,
        same_block_citation_count: row.get::<i64, _>("same_block_citation_count") as u32,
        claim_range_citation_count: row.get::<i64, _>("claim_range_citation_count") as u32,
        exact_claim_citation_link_count: row.get::<i64, _>("exact_claim_citation_link_count")
            as u32,
        target_count: row.get::<i64, _>("target_count") as u32,
        support_count: row.get::<i64, _>("support_count") as u32,
        contradiction_count: row.get::<i64, _>("contradiction_count") as u32,
        contextualize_count: row.get::<i64, _>("contextualize_count") as u32,
        insufficient_count: row.get::<i64, _>("insufficient_count") as u32,
        unverified_count: row.get::<i64, _>("unverified_count") as u32,
        blocked_count: row.get::<i64, _>("blocked_count") as u32,
    })
}

fn map_coverage_target(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ManuscriptClaimCoverageTarget, CitationReviewError> {
    Ok(ManuscriptClaimCoverageTarget {
        coverage_target_id: row.get("coverage_target_id"),
        coverage_item_id: row.get("coverage_item_id"),
        claim_citation_link_id: row.get("claim_citation_link_id"),
        citation_occurrence_id: row.get("citation_occurrence_id"),
        citation_target_id: row.get("citation_target_id"),
        citation_review_item_id: row.get("citation_review_item_id"),
        binding_id: row.get("binding_id"),
        source_id: row.get("source_id"),
        source_snapshot_id: row.get("source_snapshot_id"),
        extraction_id: row.get("extraction_id"),
        verification_run_id: row.get("verification_run_id"),
        review_status: parse_enum(row.get::<String, _>("review_status"), "review status")?,
        failure_code: row.get("failure_code"),
        verification_status: row
            .get::<Option<String>, _>("verification_status")
            .map(|value| parse_enum(value, "verification status"))
            .transpose()?,
        verification_failure_code: row.get("verification_failure_code"),
        relation: row
            .get::<Option<String>, _>("relation")
            .map(|value| parse_enum(value, "evidence relation"))
            .transpose()?,
        rationale: row.get("rationale"),
        evidence_count: row.get::<i64, _>("evidence_count") as u32,
        evidence: Vec::new(),
    })
}

fn json_text<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("coverage enums serialize")
}

fn enum_text<T: Serialize>(value: &T) -> String {
    json_text(value).trim_matches('"').to_owned()
}

fn parse_json<T: DeserializeOwned>(value: String, name: &str) -> Result<T, CitationReviewError> {
    serde_json::from_str(&value)
        .map_err(|_| CitationReviewError::Invalid(format!("invalid persisted {name}")))
}

fn parse_enum<T: DeserializeOwned>(value: String, name: &str) -> Result<T, CitationReviewError> {
    serde_json::from_value(serde_json::Value::String(value))
        .map_err(|_| CitationReviewError::Invalid(format!("invalid persisted {name}")))
}
