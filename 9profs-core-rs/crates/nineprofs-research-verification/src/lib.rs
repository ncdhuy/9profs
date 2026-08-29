//! Deterministic, provenance-safe citation verification orchestration.
//!
//! Retrieval candidates are immutable audit inputs. Only assessor-selected
//! canonical ranges are promoted to ResearchEvidence.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use async_trait::async_trait;
use nineprofs_common::{new_id, now_ms};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    AssessmentMethod, CapturePdfEvidence, ClaimEvidenceRelation, ResearchClaim, ResearchError,
    ResearchRetrievalScope, ResearchService,
};
use nineprofs_research_dify::{DifyError, DifyResearchService};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use thiserror::Error;

mod citation_review;
pub use citation_review::*;
mod manuscript_claim_coverage;
pub use manuscript_claim_coverage::*;
mod manuscript_claim_expectation;
pub use manuscript_claim_expectation::*;
mod manuscript_cross_claim_candidates;
pub use manuscript_cross_claim_candidates::*;
mod manuscript_cross_claim_assessment;
pub use manuscript_cross_claim_assessment::*;
mod manuscript_research_review;
pub use manuscript_research_review::*;

pub const DEFAULT_TOP_K: u32 = 8;
pub const MAX_TOP_K: u32 = 16;
pub const ASSESSMENT_CONTRACT_VERSION: &str = "citation-assessment-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationVerificationStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateCitationVerification {
    pub claim_citation_link_id: String,
    pub citation_target_binding_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationVerificationCandidate {
    pub verification_run_id: String,
    pub retrieval_chunk_id: String,
    pub research_source_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub page: u32,
    pub start: u64,
    pub end: u64,
    pub excerpt_hash: String,
    pub rank: u32,
    pub retrieval_score: f64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationVerificationResult {
    pub verification_run_id: String,
    pub overall_relation: ClaimEvidenceRelation,
    pub rationale: String,
    pub assessor_provider: String,
    pub assessor_version: String,
    pub assessor_model_id: Option<String>,
    pub assessment_contract_version: String,
    pub completed_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationVerificationEvidence {
    pub verification_run_id: String,
    pub retrieval_chunk_id: String,
    pub evidence_id: String,
    pub claim_evidence_link_id: String,
    pub relation: ClaimEvidenceRelation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationVerificationRun {
    pub run_id: String,
    pub research_case_id: String,
    pub claim_citation_link_id: String,
    pub citation_target_binding_id: String,
    pub claim_id: String,
    pub citation_occurrence_id: String,
    pub citation_target_id: String,
    pub source_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub status: CitationVerificationStatus,
    pub failure_code: Option<String>,
    pub created_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub result: Option<CitationVerificationResult>,
    pub candidates: Vec<CitationVerificationCandidate>,
    pub evidence: Vec<CitationVerificationEvidence>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationRetrievalCandidate {
    pub retrieval_chunk_id: String,
    pub research_source_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub page: u32,
    pub start: u64,
    pub end: u64,
    pub verbatim_excerpt: String,
    pub retrieval_score: f64,
    pub provider: String,
    pub rank: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationAssessmentCandidate {
    pub retrieval_chunk_id: String,
    pub research_source_id: String,
    pub source_snapshot_id: String,
    pub extraction_id: String,
    pub page: u32,
    pub start: u64,
    pub end: u64,
    pub verbatim_excerpt: String,
    pub retrieval_score: f64,
    pub retrieval_rank: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationAssessmentInput {
    pub claim_id: String,
    pub claim_text: String,
    pub citation_occurrence_id: String,
    pub citation_target_id: String,
    pub reference_key: String,
    pub cited_locator: Option<String>,
    pub candidates: Vec<CitationAssessmentCandidate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedCitationCandidate {
    pub retrieval_chunk_id: String,
    pub relation: ClaimEvidenceRelation,
    pub rationale: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationAssessment {
    pub overall_relation: ClaimEvidenceRelation,
    pub rationale: String,
    pub selected_candidates: Vec<SelectedCitationCandidate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationAssessmentProviderIdentity {
    pub provider_id: String,
    pub implementation_version: String,
    pub model_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum CitationAssessmentProviderError {
    #[error("citation assessor is not configured")]
    NotConfigured,
    #[error("citation assessor configuration is invalid")]
    InvalidConfiguration,
    #[error("citation assessor request timed out")]
    Timeout,
    #[error("citation assessor authorization failed")]
    Unauthorized,
    #[error("citation assessor rate limit exceeded")]
    RateLimited,
    #[error("citation assessor provider is unavailable")]
    ProviderUnavailable,
    #[error("citation assessor response was malformed")]
    MalformedResponse,
    #[error("citation assessor returned invalid structured output")]
    InvalidStructuredOutput,
    #[error("citation assessor response exceeded size limit")]
    ResponseTooLarge,
    #[error("citation assessor input exceeded size limit")]
    InputTooLarge,
    #[error("citation assessor input is invalid")]
    InvalidInput,
    #[error("citation assessor failed")]
    Failed,
}

#[derive(Debug, Error)]
pub enum CitationRetrievalError {
    #[error("retrieval is not configured")]
    NotConfigured,
    #[error("retrieval index is not ready")]
    IndexNotReady,
    #[error("retrieval failed")]
    Failed,
}

#[async_trait]
pub trait CitationRetrievalProvider: Send + Sync {
    async fn retrieve_exact_extraction(
        &self,
        research_case_id: &str,
        extraction_id: &str,
        query: &str,
        top_k: u32,
    ) -> Result<Vec<CitationRetrievalCandidate>, CitationRetrievalError>;
}

#[async_trait]
impl CitationRetrievalProvider for DifyResearchService {
    async fn retrieve_exact_extraction(
        &self,
        research_case_id: &str,
        extraction_id: &str,
        query: &str,
        top_k: u32,
    ) -> Result<Vec<CitationRetrievalCandidate>, CitationRetrievalError> {
        let extraction_id =
            nineprofs_research::ResearchPdfExtractionId::parse(extraction_id.to_owned())
                .map_err(|_| CitationRetrievalError::Failed)?;
        self.retrieve_with_scope(
            research_case_id,
            &ResearchRetrievalScope::Extractions {
                extraction_ids: vec![extraction_id],
            },
            query,
            top_k,
        )
        .await
        .map(|candidates| {
            candidates
                .into_iter()
                .map(|candidate| CitationRetrievalCandidate {
                    retrieval_chunk_id: candidate.retrieval_chunk_id,
                    research_source_id: candidate.research_source_id,
                    source_snapshot_id: candidate.source_snapshot_id,
                    extraction_id: candidate.extraction_id,
                    page: candidate.page,
                    start: candidate.start,
                    end: candidate.end,
                    verbatim_excerpt: candidate.verbatim_excerpt,
                    retrieval_score: candidate.retrieval_score,
                    provider: candidate.provider.to_owned(),
                    rank: candidate.rank,
                })
                .collect()
        })
        .map_err(map_dify_retrieval_error)
    }
}

#[async_trait]
pub trait CitationAssessmentProvider: Send + Sync {
    fn identity(&self) -> CitationAssessmentProviderIdentity;

    fn assessment_method(&self) -> AssessmentMethod {
        AssessmentMethod::ExternalService
    }

    async fn assess(
        &self,
        input: CitationAssessmentInput,
    ) -> Result<CitationAssessment, CitationAssessmentProviderError>;
}

#[derive(Debug, Error)]
pub enum CitationVerificationError {
    #[error("claim-citation link was not found")]
    ClaimCitationLinkNotFound,
    #[error("claim was not found")]
    ClaimNotFound,
    #[error("citation occurrence was not found")]
    CitationOccurrenceNotFound,
    #[error("citation target was not found")]
    CitationTargetNotFound,
    #[error("citation target binding was not found")]
    CitationBindingNotFound,
    #[error("claim, citation, target, and binding chain does not match")]
    CitationChainMismatch,
    #[error("citation binding is not ready for exact PDF verification")]
    BindingNotPdfReady,
    #[error("retrieval is not configured")]
    RetrievalNotConfigured,
    #[error("retrieval index is not ready")]
    RetrievalIndexNotReady,
    #[error("retrieval failed")]
    RetrievalFailed,
    #[error("citation assessor is not configured")]
    AssessorNotConfigured,
    #[error("citation assessor failed")]
    AssessorFailed,
    #[error("citation assessor returned invalid output")]
    AssessorInvalidOutput,
    #[error("citation assessor selected an unknown candidate")]
    CandidateUnknown,
    #[error("citation candidate integrity check failed")]
    CandidateIntegrityFailed,
    #[error("evidence promotion failed")]
    EvidencePromotionFailed,
    #[error("citation verification run was not found")]
    NotFound,
    #[error("verification persistence is invalid: {0}")]
    PersistenceInvalid(String),
    #[error(transparent)]
    Research(#[from] ResearchError),
    #[error("verification database query failed: {0}")]
    Database(#[from] sqlx::Error),
}

impl CitationVerificationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ClaimCitationLinkNotFound => "claim_citation_link_not_found",
            Self::ClaimNotFound => "claim_not_found",
            Self::CitationOccurrenceNotFound => "citation_occurrence_not_found",
            Self::CitationTargetNotFound => "citation_target_not_found",
            Self::CitationBindingNotFound => "citation_binding_not_found",
            Self::CitationChainMismatch => "citation_chain_mismatch",
            Self::BindingNotPdfReady => "binding_not_pdf_ready",
            Self::RetrievalNotConfigured => "retrieval_not_configured",
            Self::RetrievalIndexNotReady => "retrieval_index_not_ready",
            Self::RetrievalFailed => "retrieval_failed",
            Self::AssessorNotConfigured => "assessor_not_configured",
            Self::AssessorFailed => "assessor_failed",
            Self::AssessorInvalidOutput => "assessor_invalid_output",
            Self::CandidateUnknown => "candidate_unknown",
            Self::CandidateIntegrityFailed => "candidate_integrity_failed",
            Self::EvidencePromotionFailed => "evidence_promotion_failed",
            Self::NotFound => "citation_verification_not_found",
            Self::PersistenceInvalid(_) => "internal_error",
            Self::Research(_) | Self::Database(_) => "internal_error",
        }
    }
}

#[derive(Clone)]
pub struct CitationVerificationService {
    pool: SqlitePool,
    research: Arc<ResearchService>,
    retrieval: Arc<dyn CitationRetrievalProvider>,
    assessor: Option<Arc<dyn CitationAssessmentProvider>>,
    events: Arc<BroadcastEventBus>,
    top_k: u32,
}

impl CitationVerificationService {
    pub fn new<R>(
        pool: SqlitePool,
        research: Arc<ResearchService>,
        retrieval: Arc<R>,
        events: Arc<BroadcastEventBus>,
    ) -> Self
    where
        R: CitationRetrievalProvider + 'static,
    {
        Self {
            pool,
            research,
            retrieval,
            assessor: None,
            events,
            top_k: DEFAULT_TOP_K,
        }
    }

    pub fn with_assessor(mut self, assessor: Arc<dyn CitationAssessmentProvider>) -> Self {
        self.assessor = Some(assessor);
        self
    }

    pub fn with_top_k(mut self, top_k: u32) -> Self {
        self.top_k = top_k.clamp(1, MAX_TOP_K);
        self
    }

    pub(crate) fn assessor_identity(&self) -> Option<CitationAssessmentProviderIdentity> {
        self.assessor.as_ref().map(|assessor| assessor.identity())
    }

    pub async fn verify(
        &self,
        input: CreateCitationVerification,
    ) -> Result<CitationVerificationRun, CitationVerificationError> {
        let link = self
            .research
            .get_claim_citation_link(&input.claim_citation_link_id)
            .await
            .map_err(map_claim_citation_link_error)?;
        let claim = self
            .research
            .get_claim(link.claim_id.as_str())
            .await
            .map_err(map_claim_error)?;
        let occurrence = self
            .research
            .get_citation_occurrence(link.citation_occurrence_id.as_str())
            .await
            .map_err(map_occurrence_error)?;
        let binding = self
            .research
            .get_citation_target_binding(&input.citation_target_binding_id)
            .await
            .map_err(map_binding_error)?;
        let target = self
            .research
            .get_citation_target(binding.citation_target_id.as_str())
            .await
            .map_err(map_target_error)?;

        if link.research_case_id != claim.research_case_id
            || link.research_case_id != occurrence.research_case_id
            || link.citation_occurrence_id != target.citation_occurrence_id
            || binding.citation_target_id != target.id
            || binding.research_case_id != link.research_case_id
        {
            return Err(CitationVerificationError::CitationChainMismatch);
        }

        let snapshot_id = binding
            .source_snapshot_id
            .clone()
            .ok_or(CitationVerificationError::BindingNotPdfReady)?;
        let extraction_id = binding
            .extraction_id
            .clone()
            .ok_or(CitationVerificationError::BindingNotPdfReady)?;
        if !binding.pdf_verification_ready() {
            return Err(CitationVerificationError::BindingNotPdfReady);
        }
        let source = self.research.get_source(binding.source_id.as_str()).await?;
        let snapshot = self.research.get_snapshot(snapshot_id.as_str()).await?;
        let extraction = self
            .research
            .get_pdf_extraction_by_id(extraction_id.as_str())
            .await?;
        if source.research_case_id != link.research_case_id
            || !matches!(source.kind, nineprofs_research::SourceKind::ReferencePdf)
            || snapshot.source_id != source.id
            || extraction.source_snapshot_id != snapshot.id
            || !matches!(
                extraction.status,
                nineprofs_research::PdfExtractionStatus::Ready
            )
        {
            return Err(CitationVerificationError::BindingNotPdfReady);
        }

        let run = CitationVerificationRun {
            run_id: format!("citation_verification_{}", new_id()),
            research_case_id: link.research_case_id.to_string(),
            claim_citation_link_id: link.id.to_string(),
            citation_target_binding_id: binding.id.to_string(),
            claim_id: claim.id.to_string(),
            citation_occurrence_id: occurrence.id.to_string(),
            citation_target_id: target.id.to_string(),
            source_id: source.id.to_string(),
            source_snapshot_id: snapshot.id.to_string(),
            extraction_id: extraction.id.to_string(),
            status: CitationVerificationStatus::Running,
            failure_code: None,
            created_at_ms: now_ms(),
            completed_at_ms: None,
            result: None,
            candidates: Vec::new(),
            evidence: Vec::new(),
        };
        self.insert_run(&run).await?;
        self.publish_started(&run);

        let outcome = self.execute(&run, &claim, &occurrence, &target).await;
        match outcome {
            Ok(run) => {
                self.publish_completed(&run);
                Ok(run)
            }
            Err(error) => {
                if let Err(persistence_error) = self.mark_failed(&run.run_id, error.code()).await {
                    return Err(CitationVerificationError::Database(persistence_error));
                }
                self.publish_failed(&run, error.code());
                Err(error)
            }
        }
    }

    async fn execute(
        &self,
        run: &CitationVerificationRun,
        claim: &ResearchClaim,
        occurrence: &nineprofs_research::CitationOccurrence,
        target: &nineprofs_research::CitationTarget,
    ) -> Result<CitationVerificationRun, CitationVerificationError> {
        let assessor = self
            .assessor
            .as_ref()
            .ok_or(CitationVerificationError::AssessorNotConfigured)?;
        let retrieved = self
            .retrieval
            .retrieve_exact_extraction(
                run.research_case_id.as_str(),
                run.extraction_id.as_str(),
                claim.text.as_str(),
                self.top_k,
            )
            .await
            .map_err(|error| match error {
                CitationRetrievalError::NotConfigured => {
                    CitationVerificationError::RetrievalNotConfigured
                }
                CitationRetrievalError::IndexNotReady => {
                    CitationVerificationError::RetrievalIndexNotReady
                }
                CitationRetrievalError::Failed => CitationVerificationError::RetrievalFailed,
            })?;
        let canonical = self.validate_candidates(run, retrieved).await?;
        self.insert_candidates(&canonical).await?;

        let assessment = assessor
            .assess(CitationAssessmentInput {
                claim_id: claim.id.to_string(),
                claim_text: claim.text.clone(),
                citation_occurrence_id: occurrence.id.to_string(),
                citation_target_id: target.id.to_string(),
                reference_key: target.reference_key.clone(),
                cited_locator: target.cited_locator.clone(),
                candidates: canonical
                    .iter()
                    .map(|candidate| CitationAssessmentCandidate {
                        retrieval_chunk_id: candidate.0.retrieval_chunk_id.clone(),
                        research_source_id: candidate.0.research_source_id.clone(),
                        source_snapshot_id: candidate.0.source_snapshot_id.clone(),
                        extraction_id: candidate.0.extraction_id.clone(),
                        page: candidate.0.page,
                        start: candidate.0.start,
                        end: candidate.0.end,
                        verbatim_excerpt: candidate.1.clone(),
                        retrieval_score: candidate.0.retrieval_score,
                        retrieval_rank: candidate.0.rank,
                    })
                    .collect(),
            })
            .await
            .map_err(|error| match error {
                CitationAssessmentProviderError::NotConfigured
                | CitationAssessmentProviderError::InvalidConfiguration => {
                    CitationVerificationError::AssessorNotConfigured
                }
                CitationAssessmentProviderError::MalformedResponse
                | CitationAssessmentProviderError::InvalidStructuredOutput
                | CitationAssessmentProviderError::ResponseTooLarge => {
                    CitationVerificationError::AssessorInvalidOutput
                }
                CitationAssessmentProviderError::Timeout
                | CitationAssessmentProviderError::Unauthorized
                | CitationAssessmentProviderError::RateLimited
                | CitationAssessmentProviderError::ProviderUnavailable
                | CitationAssessmentProviderError::InputTooLarge
                | CitationAssessmentProviderError::InvalidInput
                | CitationAssessmentProviderError::Failed => {
                    CitationVerificationError::AssessorFailed
                }
            })?;
        validate_assessment(&assessment, &canonical)?;

        let identity = assessor.identity();
        let method = assessor.assessment_method();
        let mut selected_ranges = BTreeSet::new();
        let candidate_by_id: BTreeMap<_, _> = canonical
            .iter()
            .map(|(audit, excerpt)| (audit.retrieval_chunk_id.clone(), (audit, excerpt)))
            .collect();
        let mut selections = Vec::new();
        for selected in &assessment.selected_candidates {
            let (audit, _) = candidate_by_id
                .get(&selected.retrieval_chunk_id)
                .ok_or(CitationVerificationError::CandidateUnknown)?;
            let range = (
                audit.extraction_id.clone(),
                audit.page,
                audit.start,
                audit.end,
            );
            if selected_ranges.insert(range) {
                // Revalidate every selected range before promoting any evidence, so a
                // stale later candidate cannot leave a partially promoted run.
                self.revalidate_candidate(run, audit).await?;
                selections.push((*audit, selected));
            }
        }
        let research_case_id =
            nineprofs_research::ResearchCaseId::parse(run.research_case_id.clone())?;
        let source_snapshot_id =
            nineprofs_research::ResearchSourceSnapshotId::parse(run.source_snapshot_id.clone())?;
        let extraction_id =
            nineprofs_research::ResearchPdfExtractionId::parse(run.extraction_id.clone())?;
        let claim_id = nineprofs_research::ResearchClaimId::parse(run.claim_id.clone())?;
        let mut promoted = Vec::new();
        for (audit, selected) in selections {
            let evidence = self
                .research
                .capture_pdf_evidence(CapturePdfEvidence {
                    research_case_id: research_case_id.clone(),
                    source_snapshot_id: source_snapshot_id.clone(),
                    extraction_id: extraction_id.clone(),
                    page: audit.page,
                    start: audit.start,
                    end: audit.end,
                })
                .await
                .map_err(|_| CitationVerificationError::EvidencePromotionFailed)?;
            let link = self
                .research
                .create_link(nineprofs_research::CreateClaimEvidenceLink {
                    research_case_id: research_case_id.clone(),
                    claim_id: claim_id.clone(),
                    evidence_id: evidence.id.clone(),
                    relation: selected.relation.clone(),
                    rationale: selected
                        .rationale
                        .clone()
                        .or_else(|| Some(assessment.rationale.clone())),
                    assessment_method: method.clone(),
                    assessment_metadata: BTreeMap::from([
                        ("verification_run_id".to_owned(), run.run_id.clone()),
                        ("assessor_provider".to_owned(), identity.provider_id.clone()),
                        (
                            "assessor_version".to_owned(),
                            identity.implementation_version.clone(),
                        ),
                        (
                            "assessment_contract_version".to_owned(),
                            ASSESSMENT_CONTRACT_VERSION.to_owned(),
                        ),
                    ]),
                })
                .await
                .map_err(|_| CitationVerificationError::EvidencePromotionFailed)?;
            let mapping = CitationVerificationEvidence {
                verification_run_id: run.run_id.clone(),
                retrieval_chunk_id: audit.retrieval_chunk_id.clone(),
                evidence_id: evidence.id.to_string(),
                claim_evidence_link_id: link.id.to_string(),
                relation: selected.relation.clone(),
            };
            self.insert_mapping(&mapping).await?;
            promoted.push(mapping);
        }

        let completed_at_ms = now_ms();
        let result = CitationVerificationResult {
            verification_run_id: run.run_id.clone(),
            overall_relation: assessment.overall_relation,
            rationale: assessment.rationale,
            assessor_provider: identity.provider_id,
            assessor_version: identity.implementation_version,
            assessor_model_id: identity.model_id,
            assessment_contract_version: ASSESSMENT_CONTRACT_VERSION.to_owned(),
            completed_at_ms,
        };
        self.insert_result_and_complete(&result, completed_at_ms)
            .await?;
        let mut completed = self.load_run(&run.run_id).await?;
        completed.evidence = promoted;
        Ok(completed)
    }

    async fn validate_candidates(
        &self,
        run: &CitationVerificationRun,
        candidates: Vec<CitationRetrievalCandidate>,
    ) -> Result<Vec<(CitationVerificationCandidate, String)>, CitationVerificationError> {
        let mut seen = BTreeSet::new();
        let mut canonical = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if !seen.insert(candidate.retrieval_chunk_id.clone())
                || candidate.extraction_id != run.extraction_id
                || candidate.source_snapshot_id != run.source_snapshot_id
                || candidate.research_source_id != run.source_id
                || candidate.rank == 0
                || !candidate.retrieval_score.is_finite()
            {
                return Err(CitationVerificationError::CandidateIntegrityFailed);
            }
            let page = self
                .research
                .get_pdf_page(&candidate.extraction_id, candidate.page)
                .await
                .map_err(|_| CitationVerificationError::CandidateIntegrityFailed)?;
            let excerpt = unicode_slice(&page.text, candidate.start, candidate.end)
                .ok_or(CitationVerificationError::CandidateIntegrityFailed)?;
            if excerpt != candidate.verbatim_excerpt {
                return Err(CitationVerificationError::CandidateIntegrityFailed);
            }
            canonical.push((
                CitationVerificationCandidate {
                    verification_run_id: run.run_id.clone(),
                    retrieval_chunk_id: candidate.retrieval_chunk_id,
                    research_source_id: candidate.research_source_id,
                    source_snapshot_id: candidate.source_snapshot_id,
                    extraction_id: candidate.extraction_id,
                    page: candidate.page,
                    start: candidate.start,
                    end: candidate.end,
                    excerpt_hash: sha256_hex(excerpt.as_bytes()),
                    rank: candidate.rank,
                    retrieval_score: candidate.retrieval_score,
                },
                excerpt,
            ));
        }
        Ok(canonical)
    }

    async fn revalidate_candidate(
        &self,
        run: &CitationVerificationRun,
        audit: &CitationVerificationCandidate,
    ) -> Result<(), CitationVerificationError> {
        if audit.extraction_id != run.extraction_id
            || audit.source_snapshot_id != run.source_snapshot_id
            || audit.research_source_id != run.source_id
        {
            return Err(CitationVerificationError::CandidateIntegrityFailed);
        }
        let page = self
            .research
            .get_pdf_page(&audit.extraction_id, audit.page)
            .await
            .map_err(|_| CitationVerificationError::CandidateIntegrityFailed)?;
        let excerpt = unicode_slice(&page.text, audit.start, audit.end)
            .ok_or(CitationVerificationError::CandidateIntegrityFailed)?;
        if sha256_hex(excerpt.as_bytes()) != audit.excerpt_hash {
            return Err(CitationVerificationError::CandidateIntegrityFailed);
        }
        Ok(())
    }

    async fn insert_run(&self, run: &CitationVerificationRun) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO research_citation_verification_runs
             (id, research_case_id, claim_citation_link_id, citation_target_binding_id, claim_id,
              citation_occurrence_id, citation_target_id, source_id, source_snapshot_id, extraction_id,
              status, failure_code, created_at_ms, completed_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&run.run_id).bind(&run.research_case_id).bind(&run.claim_citation_link_id)
        .bind(&run.citation_target_binding_id).bind(&run.claim_id).bind(&run.citation_occurrence_id)
        .bind(&run.citation_target_id).bind(&run.source_id).bind(&run.source_snapshot_id)
        .bind(&run.extraction_id).bind(status_text(&run.status)).bind(&run.failure_code)
        .bind(run.created_at_ms).bind(run.completed_at_ms)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn insert_candidates(
        &self,
        candidates: &[(CitationVerificationCandidate, String)],
    ) -> Result<(), sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        for (candidate, _) in candidates {
            sqlx::query(
                "INSERT INTO research_citation_verification_candidates
                 (verification_run_id, retrieval_chunk_id, research_source_id, source_snapshot_id,
                  extraction_id, page, start_offset, end_offset, excerpt_hash, rank, retrieval_score)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&candidate.verification_run_id).bind(&candidate.retrieval_chunk_id)
            .bind(&candidate.research_source_id).bind(&candidate.source_snapshot_id)
            .bind(&candidate.extraction_id).bind(candidate.page as i64).bind(candidate.start as i64)
            .bind(candidate.end as i64).bind(&candidate.excerpt_hash).bind(candidate.rank as i64)
            .bind(candidate.retrieval_score).execute(&mut *transaction).await?;
        }
        transaction.commit().await
    }

    async fn insert_mapping(
        &self,
        mapping: &CitationVerificationEvidence,
    ) -> Result<(), CitationVerificationError> {
        sqlx::query(
            "INSERT INTO research_citation_verification_evidence
             (verification_run_id, retrieval_chunk_id, evidence_id, claim_evidence_link_id, relation)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&mapping.verification_run_id).bind(&mapping.retrieval_chunk_id)
        .bind(&mapping.evidence_id).bind(&mapping.claim_evidence_link_id)
        .bind(relation_text(&mapping.relation)).execute(&self.pool).await?;
        Ok(())
    }

    async fn insert_result_and_complete(
        &self,
        result: &CitationVerificationResult,
        completed_at_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO research_citation_verification_results
             (verification_run_id, overall_relation, rationale, assessor_provider, assessor_version,
              assessor_model_id, assessment_contract_version, completed_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&result.verification_run_id)
        .bind(relation_text(&result.overall_relation))
        .bind(&result.rationale)
        .bind(&result.assessor_provider)
        .bind(&result.assessor_version)
        .bind(&result.assessor_model_id)
        .bind(&result.assessment_contract_version)
        .bind(result.completed_at_ms)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE research_citation_verification_runs
             SET status = 'completed', failure_code = NULL, completed_at_ms = ? WHERE id = ?",
        )
        .bind(completed_at_ms)
        .bind(&result.verification_run_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await
    }

    async fn mark_failed(&self, run_id: &str, code: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE research_citation_verification_runs SET status = 'failed', failure_code = ?, completed_at_ms = ? WHERE id = ?",
        )
        .bind(code).bind(now_ms()).bind(run_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn citation_verification(
        &self,
        run_id: &str,
    ) -> Result<CitationVerificationRun, CitationVerificationError> {
        self.load_run(run_id).await
    }

    pub async fn get_citation_verification(
        &self,
        run_id: &str,
    ) -> Result<CitationVerificationRun, CitationVerificationError> {
        self.citation_verification(run_id).await
    }

    pub async fn claim_citation_verifications(
        &self,
        claim_id: &str,
    ) -> Result<Vec<CitationVerificationRun>, CitationVerificationError> {
        let rows = sqlx::query(
            "SELECT id FROM research_citation_verification_runs WHERE claim_id = ? ORDER BY created_at_ms ASC, id ASC",
        ).bind(claim_id).fetch_all(&self.pool).await?;
        let mut runs = Vec::with_capacity(rows.len());
        for row in rows {
            runs.push(self.load_run(&row.get::<String, _>("id")).await?);
        }
        Ok(runs)
    }

    pub async fn latest_for_link_and_binding(
        &self,
        claim_citation_link_id: &str,
        citation_target_binding_id: &str,
    ) -> Result<Option<CitationVerificationRun>, CitationVerificationError> {
        let row = sqlx::query(
            "SELECT id FROM research_citation_verification_runs
             WHERE claim_citation_link_id = ? AND citation_target_binding_id = ?
             ORDER BY created_at_ms DESC, id DESC LIMIT 1",
        )
        .bind(claim_citation_link_id)
        .bind(citation_target_binding_id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => Ok(Some(self.load_run(&row.get::<String, _>("id")).await?)),
            None => Ok(None),
        }
    }

    pub async fn latest_for_link_and_binding_after(
        &self,
        claim_citation_link_id: &str,
        citation_target_binding_id: &str,
        created_after_ms: i64,
    ) -> Result<Option<CitationVerificationRun>, CitationVerificationError> {
        let row = sqlx::query(
            "SELECT id FROM research_citation_verification_runs
             WHERE claim_citation_link_id = ? AND citation_target_binding_id = ?
               AND created_at_ms >= ?
             ORDER BY created_at_ms DESC, id DESC LIMIT 1",
        )
        .bind(claim_citation_link_id)
        .bind(citation_target_binding_id)
        .bind(created_after_ms)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => Ok(Some(self.load_run(&row.get::<String, _>("id")).await?)),
            None => Ok(None),
        }
    }

    async fn load_run(
        &self,
        run_id: &str,
    ) -> Result<CitationVerificationRun, CitationVerificationError> {
        let row = sqlx::query(
            "SELECT id, research_case_id, claim_citation_link_id, citation_target_binding_id, claim_id,
             citation_occurrence_id, citation_target_id, source_id, source_snapshot_id, extraction_id,
             status, failure_code, created_at_ms, completed_at_ms
             FROM research_citation_verification_runs WHERE id = ?",
        ).bind(run_id).fetch_optional(&self.pool).await?
            .ok_or(CitationVerificationError::NotFound)?;
        let run = CitationVerificationRun {
            run_id: row.get("id"),
            research_case_id: row.get("research_case_id"),
            claim_citation_link_id: row.get("claim_citation_link_id"),
            citation_target_binding_id: row.get("citation_target_binding_id"),
            claim_id: row.get("claim_id"),
            citation_occurrence_id: row.get("citation_occurrence_id"),
            citation_target_id: row.get("citation_target_id"),
            source_id: row.get("source_id"),
            source_snapshot_id: row.get("source_snapshot_id"),
            extraction_id: row.get("extraction_id"),
            status: parse_status(row.get("status"))?,
            failure_code: row.get("failure_code"),
            created_at_ms: row.get("created_at_ms"),
            completed_at_ms: row.get("completed_at_ms"),
            result: self.load_result(run_id).await?,
            candidates: self.load_candidates(run_id).await?,
            evidence: self.load_evidence(run_id).await?,
        };
        Ok(run)
    }

    async fn load_result(
        &self,
        run_id: &str,
    ) -> Result<Option<CitationVerificationResult>, CitationVerificationError> {
        let Some(row) = sqlx::query(
            "SELECT verification_run_id, overall_relation, rationale, assessor_provider, assessor_version,
             assessor_model_id, assessment_contract_version, completed_at_ms
             FROM research_citation_verification_results WHERE verification_run_id = ?",
        ).bind(run_id).fetch_optional(&self.pool).await? else { return Ok(None) };
        Ok(Some(CitationVerificationResult {
            verification_run_id: row.get("verification_run_id"),
            overall_relation: parse_relation(row.get("overall_relation"))?,
            rationale: row.get("rationale"),
            assessor_provider: row.get("assessor_provider"),
            assessor_version: row.get("assessor_version"),
            assessor_model_id: row.get("assessor_model_id"),
            assessment_contract_version: row.get("assessment_contract_version"),
            completed_at_ms: row.get("completed_at_ms"),
        }))
    }

    async fn load_candidates(
        &self,
        run_id: &str,
    ) -> Result<Vec<CitationVerificationCandidate>, CitationVerificationError> {
        let rows = sqlx::query(
            "SELECT verification_run_id, retrieval_chunk_id, research_source_id, source_snapshot_id,
             extraction_id, page, start_offset, end_offset, excerpt_hash, rank, retrieval_score
             FROM research_citation_verification_candidates WHERE verification_run_id = ?
             ORDER BY rank ASC, retrieval_chunk_id ASC",
        ).bind(run_id).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                Ok(CitationVerificationCandidate {
                    verification_run_id: row.get("verification_run_id"),
                    retrieval_chunk_id: row.get("retrieval_chunk_id"),
                    research_source_id: row.get("research_source_id"),
                    source_snapshot_id: row.get("source_snapshot_id"),
                    extraction_id: row.get("extraction_id"),
                    page: row.get::<i64, _>("page") as u32,
                    start: row.get::<i64, _>("start_offset") as u64,
                    end: row.get::<i64, _>("end_offset") as u64,
                    excerpt_hash: row.get("excerpt_hash"),
                    rank: row.get::<i64, _>("rank") as u32,
                    retrieval_score: row.get("retrieval_score"),
                })
            })
            .collect()
    }

    async fn load_evidence(
        &self,
        run_id: &str,
    ) -> Result<Vec<CitationVerificationEvidence>, CitationVerificationError> {
        let rows = sqlx::query(
            "SELECT verification_run_id, retrieval_chunk_id, evidence_id, claim_evidence_link_id, relation
             FROM research_citation_verification_evidence WHERE verification_run_id = ?
             ORDER BY retrieval_chunk_id ASC",
        ).bind(run_id).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| {
                Ok(CitationVerificationEvidence {
                    verification_run_id: row.get("verification_run_id"),
                    retrieval_chunk_id: row.get("retrieval_chunk_id"),
                    evidence_id: row.get("evidence_id"),
                    claim_evidence_link_id: row.get("claim_evidence_link_id"),
                    relation: parse_relation(row.get("relation"))?,
                })
            })
            .collect()
    }

    fn publish_started(&self, run: &CitationVerificationRun) {
        self.publish("research.citationVerificationStarted", run, None, None);
    }

    fn publish_completed(&self, run: &CitationVerificationRun) {
        self.publish(
            "research.citationVerificationCompleted",
            run,
            Some("completed"),
            run.result
                .as_ref()
                .map(|result| relation_text(&result.overall_relation).to_owned()),
        );
    }

    fn publish_failed(&self, run: &CitationVerificationRun, code: &str) {
        self.publish(
            "research.citationVerificationFailed",
            run,
            Some("failed"),
            Some(code.to_owned()),
        );
    }

    fn publish(
        &self,
        name: &str,
        run: &CitationVerificationRun,
        status: Option<&str>,
        outcome: Option<String>,
    ) {
        let mut payload = serde_json::json!({
            "verificationRunId": run.run_id,
            "claimId": run.claim_id,
            "citationTargetBindingId": run.citation_target_binding_id,
        });
        if let Some(status) = status {
            payload["status"] = serde_json::Value::String(status.to_owned());
        }
        if let Some(outcome) = outcome {
            payload["outcome"] = serde_json::Value::String(outcome);
        }
        let _ = self
            .events
            .publish(nineprofs_api_types::EventEnvelope::new(name, payload));
    }
}

fn validate_assessment(
    assessment: &CitationAssessment,
    candidates: &[(CitationVerificationCandidate, String)],
) -> Result<(), CitationVerificationError> {
    if assessment.rationale.len() > nineprofs_research::MAX_RATIONALE_BYTES {
        return Err(CitationVerificationError::AssessorInvalidOutput);
    }
    let known: BTreeSet<_> = candidates
        .iter()
        .map(|(candidate, _)| candidate.retrieval_chunk_id.as_str())
        .collect();
    let mut selected = BTreeSet::new();
    for candidate in &assessment.selected_candidates {
        if !known.contains(candidate.retrieval_chunk_id.as_str()) {
            return Err(CitationVerificationError::CandidateUnknown);
        }
        if !selected.insert(candidate.retrieval_chunk_id.as_str())
            || candidate
                .rationale
                .as_ref()
                .is_some_and(|value| value.len() > nineprofs_research::MAX_RATIONALE_BYTES)
        {
            return Err(CitationVerificationError::AssessorInvalidOutput);
        }
    }
    if matches!(
        assessment.overall_relation,
        ClaimEvidenceRelation::Supports
            | ClaimEvidenceRelation::Contradicts
            | ClaimEvidenceRelation::Contextualizes
    ) && assessment.selected_candidates.is_empty()
    {
        return Err(CitationVerificationError::AssessorInvalidOutput);
    }
    if matches!(
        assessment.overall_relation,
        ClaimEvidenceRelation::Insufficient
    ) && assessment.selected_candidates.iter().any(|candidate| {
        !matches!(
            candidate.relation,
            ClaimEvidenceRelation::Insufficient | ClaimEvidenceRelation::Contextualizes
        )
    }) {
        return Err(CitationVerificationError::AssessorInvalidOutput);
    }
    Ok(())
}

fn map_dify_retrieval_error(error: DifyError) -> CitationRetrievalError {
    match error {
        DifyError::NotConfigured => CitationRetrievalError::NotConfigured,
        DifyError::IndexingFailed => CitationRetrievalError::IndexNotReady,
        DifyError::Invalid(_) | DifyError::Research(_) => CitationRetrievalError::Failed,
        DifyError::Unreachable
        | DifyError::Unauthorized
        | DifyError::RateLimited
        | DifyError::ProviderNotInitialized
        | DifyError::Timeout
        | DifyError::MalformedResponse
        | DifyError::RemoteNotFound
        | DifyError::IndexDrift
        | DifyError::Integrity
        | DifyError::Database(_) => CitationRetrievalError::Failed,
    }
}

fn map_claim_citation_link_error(error: ResearchError) -> CitationVerificationError {
    if matches!(error, ResearchError::NotFound { .. }) {
        CitationVerificationError::ClaimCitationLinkNotFound
    } else {
        error.into()
    }
}
fn map_claim_error(error: ResearchError) -> CitationVerificationError {
    if matches!(error, ResearchError::NotFound { .. }) {
        CitationVerificationError::ClaimNotFound
    } else {
        error.into()
    }
}
fn map_occurrence_error(error: ResearchError) -> CitationVerificationError {
    if matches!(error, ResearchError::NotFound { .. }) {
        CitationVerificationError::CitationOccurrenceNotFound
    } else {
        error.into()
    }
}
fn map_binding_error(error: ResearchError) -> CitationVerificationError {
    if matches!(error, ResearchError::NotFound { .. }) {
        CitationVerificationError::CitationBindingNotFound
    } else {
        error.into()
    }
}
fn map_target_error(error: ResearchError) -> CitationVerificationError {
    if matches!(error, ResearchError::NotFound { .. }) {
        CitationVerificationError::CitationTargetNotFound
    } else {
        error.into()
    }
}

fn status_text(status: &CitationVerificationStatus) -> &'static str {
    match status {
        CitationVerificationStatus::Running => "running",
        CitationVerificationStatus::Completed => "completed",
        CitationVerificationStatus::Failed => "failed",
    }
}
fn parse_status(value: String) -> Result<CitationVerificationStatus, CitationVerificationError> {
    match value.as_str() {
        "running" => Ok(CitationVerificationStatus::Running),
        "completed" => Ok(CitationVerificationStatus::Completed),
        "failed" => Ok(CitationVerificationStatus::Failed),
        _ => Err(CitationVerificationError::PersistenceInvalid(
            "unknown verification status".to_owned(),
        )),
    }
}
fn relation_text(relation: &ClaimEvidenceRelation) -> &'static str {
    match relation {
        ClaimEvidenceRelation::Supports => "supports",
        ClaimEvidenceRelation::Contradicts => "contradicts",
        ClaimEvidenceRelation::Contextualizes => "contextualizes",
        ClaimEvidenceRelation::Insufficient => "insufficient",
    }
}
fn parse_relation(value: String) -> Result<ClaimEvidenceRelation, CitationVerificationError> {
    match value.as_str() {
        "supports" => Ok(ClaimEvidenceRelation::Supports),
        "contradicts" => Ok(ClaimEvidenceRelation::Contradicts),
        "contextualizes" => Ok(ClaimEvidenceRelation::Contextualizes),
        "insufficient" => Ok(ClaimEvidenceRelation::Insufficient),
        _ => Err(CitationVerificationError::PersistenceInvalid(
            "unknown verification relation".to_owned(),
        )),
    }
}
fn unicode_slice(text: &str, start: u64, end: u64) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?;
    (start < end && end <= chars.len()).then(|| chars[start..end].iter().collect())
}
fn sha256_hex(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    format!("{:x}", hasher.finalize())
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::*;

    fn identity(id: &str) -> CitationAssessmentProviderIdentity {
        CitationAssessmentProviderIdentity {
            provider_id: id.to_owned(),
            implementation_version: "test-1".to_owned(),
            model_id: None,
        }
    }

    pub struct FixedSupportsAssessor;
    pub struct FixedContradictsAssessor;
    pub struct FailingAssessor;
    pub struct InvalidCandidateAssessor;

    macro_rules! fixed_assessor {
        ($name:ident, $id:literal, $relation:expr) => {
            #[async_trait]
            impl CitationAssessmentProvider for $name {
                fn identity(&self) -> CitationAssessmentProviderIdentity {
                    identity($id)
                }
                fn assessment_method(&self) -> AssessmentMethod {
                    AssessmentMethod::DeterministicChecker
                }
                async fn assess(
                    &self,
                    input: CitationAssessmentInput,
                ) -> Result<CitationAssessment, CitationAssessmentProviderError> {
                    Ok(CitationAssessment {
                        overall_relation: $relation.clone(),
                        rationale: "fixed test assessment".to_owned(),
                        selected_candidates: input
                            .candidates
                            .first()
                            .map(|candidate| SelectedCitationCandidate {
                                retrieval_chunk_id: candidate.retrieval_chunk_id.clone(),
                                relation: $relation.clone(),
                                rationale: None,
                            })
                            .into_iter()
                            .collect(),
                    })
                }
            }
        };
    }
    fixed_assessor!(
        FixedSupportsAssessor,
        "fixed-supports",
        ClaimEvidenceRelation::Supports
    );
    fixed_assessor!(
        FixedContradictsAssessor,
        "fixed-contradicts",
        ClaimEvidenceRelation::Contradicts
    );

    #[async_trait]
    impl CitationAssessmentProvider for FailingAssessor {
        fn identity(&self) -> CitationAssessmentProviderIdentity {
            identity("failing")
        }
        async fn assess(
            &self,
            _input: CitationAssessmentInput,
        ) -> Result<CitationAssessment, CitationAssessmentProviderError> {
            Err(CitationAssessmentProviderError::Failed)
        }
    }

    #[async_trait]
    impl CitationAssessmentProvider for InvalidCandidateAssessor {
        fn identity(&self) -> CitationAssessmentProviderIdentity {
            identity("invalid-candidate")
        }
        async fn assess(
            &self,
            _input: CitationAssessmentInput,
        ) -> Result<CitationAssessment, CitationAssessmentProviderError> {
            Ok(CitationAssessment {
                overall_relation: ClaimEvidenceRelation::Supports,
                rationale: "invalid test assessment".to_owned(),
                selected_candidates: vec![SelectedCitationCandidate {
                    retrieval_chunk_id: "unknown".to_owned(),
                    relation: ClaimEvidenceRelation::Supports,
                    rationale: None,
                }],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assessment_rejects_unknown_candidate_and_empty_scientific_result() {
        let candidate = CitationVerificationCandidate {
            verification_run_id: "run".to_owned(),
            retrieval_chunk_id: "known".to_owned(),
            research_source_id: "source".to_owned(),
            source_snapshot_id: "snapshot".to_owned(),
            extraction_id: "extraction".to_owned(),
            page: 1,
            start: 0,
            end: 1,
            excerpt_hash: "hash".to_owned(),
            rank: 1,
            retrieval_score: 0.1,
        };
        let unknown = CitationAssessment {
            overall_relation: ClaimEvidenceRelation::Supports,
            rationale: "x".to_owned(),
            selected_candidates: vec![SelectedCitationCandidate {
                retrieval_chunk_id: "unknown".to_owned(),
                relation: ClaimEvidenceRelation::Supports,
                rationale: None,
            }],
        };
        assert!(matches!(
            validate_assessment(&unknown, &[(candidate.clone(), "x".to_owned())]),
            Err(CitationVerificationError::CandidateUnknown)
        ));
        let empty = CitationAssessment {
            overall_relation: ClaimEvidenceRelation::Supports,
            rationale: "x".to_owned(),
            selected_candidates: Vec::new(),
        };
        assert!(matches!(
            validate_assessment(&empty, &[(candidate, "x".to_owned())]),
            Err(CitationVerificationError::AssessorInvalidOutput)
        ));
    }

    #[test]
    fn assessment_preserves_structured_contradiction_relation() {
        let candidate = CitationVerificationCandidate {
            verification_run_id: "run".to_owned(),
            retrieval_chunk_id: "chunk".to_owned(),
            research_source_id: "source".to_owned(),
            source_snapshot_id: "snapshot".to_owned(),
            extraction_id: "extraction".to_owned(),
            page: 1,
            start: 0,
            end: 1,
            excerpt_hash: "hash".to_owned(),
            rank: 1,
            retrieval_score: 1.0,
        };
        let assessment = CitationAssessment {
            overall_relation: ClaimEvidenceRelation::Contradicts,
            rationale: "The cited result points in the opposite direction.".to_owned(),
            selected_candidates: vec![SelectedCitationCandidate {
                retrieval_chunk_id: "chunk".to_owned(),
                relation: ClaimEvidenceRelation::Contradicts,
                rationale: None,
            }],
        };

        assert!(validate_assessment(&assessment, &[(candidate, "x".to_owned())]).is_ok());
    }
}
