use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CapturePdfExtraction, CapturePdfPage, ClaimReviewKind, CreateResearchCase,
    CreateResearchSource, ManuscriptCitationFormat, ManuscriptClaimExtractionBlockInput,
    ManuscriptClaimExtractionClaimOutput, ManuscriptClaimExtractionIdentity,
    ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProvider,
    ManuscriptClaimExtractionProviderError, ManuscriptClaimInventoryBlockInput,
    ManuscriptClaimInventoryBlockKind, ManuscriptClaimInventoryCitationInput,
    ManuscriptClaimInventoryClaimOutput, ManuscriptClaimInventoryIdentity,
    ManuscriptClaimInventoryOutput, ManuscriptClaimInventoryProvider,
    ManuscriptClaimInventoryProviderError, ManuscriptReferenceCatalogZoteroInput,
    ResearchArtifactStore, ResearchCaseId, ResearchService, ResearchSourceIdentityInput,
    ResearchSourceIdentityMethod, SourceKind, SqliteResearchRepository,
};
use nineprofs_research_verification::{
    CitationAssessment, CitationAssessmentInput, CitationAssessmentProvider,
    CitationAssessmentProviderError, CitationAssessmentProviderIdentity, CitationExpectation,
    CitationExpectationAssessment, CitationExpectationInput, CitationExpectationProvider,
    CitationExpectationProviderError, CitationExpectationProviderIdentity,
    CitationRetrievalCandidate, CitationRetrievalError, CitationRetrievalProvider,
    CitationReviewBlockInput, CitationReviewCitationInput, CitationReviewService,
    CitationReviewTargetInput, CrossClaimCandidateDiscoveryInput,
    CrossClaimCandidateDiscoveryOutput, CrossClaimCandidateDiscoveryProvider,
    CrossClaimCandidateDiscoveryProviderError, CrossClaimCandidateDiscoveryProviderIdentity,
    CrossClaimCandidateOutput, CrossClaimConsistencyAssessment,
    CrossClaimConsistencyAssessmentInput, CrossClaimConsistencyAssessmentProvider,
    CrossClaimConsistencyAssessmentProviderError, CrossClaimConsistencyAssessmentProviderIdentity,
    CrossClaimDifferenceDimension, ManuscriptClaimCoverageStructuralCitationState,
    ManuscriptCrossClaimCandidateKind, ManuscriptResearchReviewCitationObservations,
    ManuscriptResearchReviewClaimInventoryObservations, ManuscriptResearchReviewRunStatus,
    StartManuscriptResearchReview,
};
use tokio::sync::Barrier;

#[derive(Clone)]
struct MutableClaimExtractor {
    identity: Arc<Mutex<ManuscriptClaimExtractionIdentity>>,
    fail: Arc<AtomicBool>,
}

#[async_trait]
impl ManuscriptClaimExtractionProvider for MutableClaimExtractor {
    fn identity(&self) -> ManuscriptClaimExtractionIdentity {
        self.identity.lock().unwrap().clone()
    }

    async fn extract(
        &self,
        block: ManuscriptClaimExtractionBlockInput,
    ) -> Result<ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProviderError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(ManuscriptClaimExtractionProviderError::Transport);
        }
        let citation_occurrence_ids = block
            .citations
            .into_iter()
            .map(|citation| citation.citation_occurrence_id)
            .collect::<Vec<_>>();
        Ok(ManuscriptClaimExtractionOutput {
            claims: if citation_occurrence_ids.is_empty() {
                Vec::new()
            } else {
                vec![ManuscriptClaimExtractionClaimOutput {
                    claim_text: "Claim".to_owned(),
                    source_start: 0,
                    source_end: 5,
                    citation_occurrence_ids,
                }]
            },
            unassociated_citations: Vec::new(),
        })
    }
}

#[derive(Clone)]
struct MutableInventoryProvider {
    identity: Arc<Mutex<ManuscriptClaimInventoryIdentity>>,
    fail: Arc<AtomicBool>,
    gate: Option<Arc<Barrier>>,
}

#[async_trait]
impl ManuscriptClaimInventoryProvider for MutableInventoryProvider {
    fn identity(&self) -> ManuscriptClaimInventoryIdentity {
        self.identity.lock().unwrap().clone()
    }

    async fn extract(
        &self,
        block: ManuscriptClaimInventoryBlockInput,
    ) -> Result<ManuscriptClaimInventoryOutput, ManuscriptClaimInventoryProviderError> {
        if let Some(gate) = &self.gate {
            gate.wait().await;
        }
        if self.fail.load(Ordering::SeqCst) {
            return Err(ManuscriptClaimInventoryProviderError::Transport);
        }
        let has_citations = !block.citations.is_empty();
        Ok(ManuscriptClaimInventoryOutput {
            claims: if block.text.trim().is_empty() {
                Vec::new()
            } else {
                vec![ManuscriptClaimInventoryClaimOutput {
                    claim_text: if has_citations {
                        "Claim".to_owned()
                    } else {
                        block.text.clone()
                    },
                    source_start: 0,
                    source_end: if has_citations {
                        5
                    } else {
                        block.text.len() as u64
                    },
                    review_kind: ClaimReviewKind::ExternalEvidence,
                }]
            },
        })
    }
}

#[derive(Clone)]
struct EmptyRetrieval {
    candidate: Arc<Mutex<Option<CitationRetrievalCandidate>>>,
}

#[async_trait]
impl CitationRetrievalProvider for EmptyRetrieval {
    async fn retrieve_exact_extraction(
        &self,
        _research_case_id: &str,
        _extraction_id: &str,
        _query: &str,
        _top_k: u32,
    ) -> Result<
        Vec<nineprofs_research_verification::CitationRetrievalCandidate>,
        CitationRetrievalError,
    > {
        Ok(self.candidate.lock().unwrap().clone().into_iter().collect())
    }
}

#[derive(Clone)]
struct MutableExpectationProvider {
    identity: Arc<Mutex<CitationExpectationProviderIdentity>>,
    fail: Arc<AtomicBool>,
}

#[async_trait]
impl CitationExpectationProvider for MutableExpectationProvider {
    fn identity(&self) -> CitationExpectationProviderIdentity {
        self.identity.lock().unwrap().clone()
    }

    async fn assess(
        &self,
        input: CitationExpectationInput,
    ) -> Result<CitationExpectationAssessment, CitationExpectationProviderError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(CitationExpectationProviderError::ProviderUnavailable);
        }
        Ok(CitationExpectationAssessment {
            item_id: input.item_id,
            expectation: CitationExpectation::ExternalEvidenceExpected,
            rationale: "fixture expectation".into(),
        })
    }
}

#[derive(Clone)]
struct MutableCandidateProvider {
    identity: Arc<Mutex<CrossClaimCandidateDiscoveryProviderIdentity>>,
}

#[async_trait]
impl CrossClaimCandidateDiscoveryProvider for MutableCandidateProvider {
    fn identity(&self) -> CrossClaimCandidateDiscoveryProviderIdentity {
        self.identity.lock().unwrap().clone()
    }

    async fn discover(
        &self,
        input: CrossClaimCandidateDiscoveryInput,
    ) -> Result<CrossClaimCandidateDiscoveryOutput, CrossClaimCandidateDiscoveryProviderError> {
        let Some(left) = input.left_batch.first() else {
            return Ok(CrossClaimCandidateDiscoveryOutput {
                comparison_window_id: input.comparison_window_id,
                candidates: Vec::new(),
            });
        };
        let Some(right) = input
            .right_batch
            .iter()
            .find(|claim| claim.inventory_item_id != left.inventory_item_id)
        else {
            return Ok(CrossClaimCandidateDiscoveryOutput {
                comparison_window_id: input.comparison_window_id,
                candidates: Vec::new(),
            });
        };
        Ok(CrossClaimCandidateDiscoveryOutput {
            comparison_window_id: input.comparison_window_id,
            candidates: vec![CrossClaimCandidateOutput {
                left_inventory_item_id: left.inventory_item_id.clone(),
                right_inventory_item_id: right.inventory_item_id.clone(),
                candidate_kind: ManuscriptCrossClaimCandidateKind::PotentialDirectConflict,
                rationale: "fixture candidate".into(),
            }],
        })
    }
}

#[derive(Clone)]
struct MutableAssessmentProvider {
    identity: Arc<Mutex<CrossClaimConsistencyAssessmentProviderIdentity>>,
    fail: Arc<AtomicBool>,
}

#[async_trait]
impl CrossClaimConsistencyAssessmentProvider for MutableAssessmentProvider {
    fn identity(&self) -> CrossClaimConsistencyAssessmentProviderIdentity {
        self.identity.lock().unwrap().clone()
    }

    async fn assess(
        &self,
        input: CrossClaimConsistencyAssessmentInput,
    ) -> Result<CrossClaimConsistencyAssessment, CrossClaimConsistencyAssessmentProviderError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(CrossClaimConsistencyAssessmentProviderError::ProviderUnavailable);
        }
        Ok(CrossClaimConsistencyAssessment {
            candidate_id: input.candidate_id,
            relation: nineprofs_research_verification::CrossClaimConsistencyRelation::Compatible,
            dimensions: vec![CrossClaimDifferenceDimension::Other],
            rationale: "fixture assessment".into(),
        })
    }
}

#[derive(Clone)]
struct Fixture {
    database: Arc<Database>,
    research: Arc<ResearchService>,
    service: Arc<CitationReviewService>,
    case_id: String,
    source_id: String,
    extractor_identity: Arc<Mutex<ManuscriptClaimExtractionIdentity>>,
    expectation_identity: Arc<Mutex<CitationExpectationProviderIdentity>>,
    citation_identity: Option<Arc<Mutex<CitationAssessmentProviderIdentity>>>,
    inventory_fail: Arc<AtomicBool>,
    expectation_fail: Arc<AtomicBool>,
    assessment_fail: Arc<AtomicBool>,
    retrieval_candidate: Arc<Mutex<Option<CitationRetrievalCandidate>>>,
    citation_fail: Option<Arc<AtomicBool>>,
    events: Arc<BroadcastEventBus>,
}

async fn fixture(with_citation_assessor: bool, inventory_gate: Option<Arc<Barrier>>) -> Fixture {
    let database = Arc::new(Database::in_memory().await.unwrap());
    let events = Arc::new(BroadcastEventBus::new(256));
    let extractor_identity = Arc::new(Mutex::new(ManuscriptClaimExtractionIdentity {
        provider: "whole-review-extractor".into(),
        extractor_version: "extractor-v1".into(),
        model_id: Some("extractor-model-a".into()),
        extraction_contract_version: "extractor-contract-v1".into(),
    }));
    let inventory_identity = Arc::new(Mutex::new(ManuscriptClaimInventoryIdentity {
        provider: "whole-review-inventory".into(),
        extractor_version: "inventory-v1".into(),
        model_id: Some("inventory-model-a".into()),
        extraction_contract_version: "inventory-contract-v1".into(),
    }));
    let expectation_identity = Arc::new(Mutex::new(CitationExpectationProviderIdentity {
        provider_id: "whole-review-expectation".into(),
        assessor_version: "expectation-v1".into(),
        model_id: Some("expectation-model-a".into()),
    }));
    let candidate_identity = Arc::new(Mutex::new(CrossClaimCandidateDiscoveryProviderIdentity {
        provider_id: "whole-review-discovery".into(),
        implementation_version: "discovery-v1".into(),
        model_id: Some("discovery-model-a".into()),
    }));
    let assessment_identity = Arc::new(Mutex::new(
        CrossClaimConsistencyAssessmentProviderIdentity {
            provider_id: "whole-review-assessment".into(),
            assessor_implementation_version: "assessment-v1".into(),
            model_id: Some("assessment-model-a".into()),
        },
    ));
    let inventory_fail = Arc::new(AtomicBool::new(false));
    let expectation_fail = Arc::new(AtomicBool::new(false));
    let assessment_fail = Arc::new(AtomicBool::new(false));
    let retrieval_candidate = Arc::new(Mutex::new(None));
    let citation_fail = with_citation_assessor.then(|| Arc::new(AtomicBool::new(false)));
    let artifact_store = Arc::new(ResearchArtifactStore::new(
        std::env::temp_dir().join(format!(
            "9profs-whole-review-{}",
            nineprofs_common::new_id()
        )),
        database.pool().clone(),
    ));
    let research = Arc::new(
        ResearchService::new(
            SqliteResearchRepository::new(database.pool().clone()),
            events.clone(),
        )
        .with_artifact_store(artifact_store)
        .with_claim_extractor(Arc::new(MutableClaimExtractor {
            identity: extractor_identity.clone(),
            fail: Arc::new(AtomicBool::new(false)),
        }))
        .with_claim_inventory_extractor(Arc::new(MutableInventoryProvider {
            identity: inventory_identity.clone(),
            fail: inventory_fail.clone(),
            gate: inventory_gate,
        })),
    );
    let case = research
        .create_case(CreateResearchCase {
            title: "Whole review fixture".into(),
        })
        .await
        .unwrap();
    let source = research
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Draft".into(),
            identity: None,
        })
        .await
        .unwrap();
    let citation_identity = with_citation_assessor.then(|| {
        Arc::new(Mutex::new(CitationAssessmentProviderIdentity {
            provider_id: "whole-review-citation".into(),
            implementation_version: "citation-v1".into(),
            model_id: Some("citation-model-a".into()),
        }))
    });
    let citation_assessor: Option<Arc<dyn CitationAssessmentProvider>> =
        citation_identity.as_ref().map(|identity| {
            Arc::new(MutableCitationAssessor {
                identity: identity.clone(),
                fail: citation_fail.as_ref().unwrap().clone(),
            }) as Arc<dyn CitationAssessmentProvider>
        });
    let verification = nineprofs_research_verification::CitationVerificationService::new(
        database.pool().clone(),
        research.clone(),
        Arc::new(EmptyRetrieval {
            candidate: retrieval_candidate.clone(),
        }),
        events.clone(),
    );
    let verification = match citation_assessor {
        Some(assessor) => verification.with_assessor(assessor),
        None => verification,
    };
    let service = CitationReviewService::new(
        database.pool().clone(),
        research.clone(),
        Arc::new(verification),
        events.clone(),
    )
    .with_expectation_assessor(Arc::new(MutableExpectationProvider {
        identity: expectation_identity.clone(),
        fail: expectation_fail.clone(),
    }))
    .with_cross_claim_candidate_provider(Arc::new(MutableCandidateProvider {
        identity: candidate_identity.clone(),
    }))
    .with_cross_claim_consistency_assessor(Arc::new(MutableAssessmentProvider {
        identity: assessment_identity.clone(),
        fail: assessment_fail.clone(),
    }));
    Fixture {
        database,
        research,
        service: Arc::new(service),
        case_id: case.id.to_string(),
        source_id: source.id.to_string(),
        extractor_identity,
        expectation_identity,
        citation_identity,
        inventory_fail,
        expectation_fail,
        assessment_fail,
        retrieval_candidate,
        citation_fail,
        events,
    }
}

struct MutableCitationAssessor {
    identity: Arc<Mutex<CitationAssessmentProviderIdentity>>,
    fail: Arc<AtomicBool>,
}

#[async_trait]
impl CitationAssessmentProvider for MutableCitationAssessor {
    fn identity(&self) -> CitationAssessmentProviderIdentity {
        self.identity.lock().unwrap().clone()
    }

    async fn assess(
        &self,
        input: CitationAssessmentInput,
    ) -> Result<CitationAssessment, CitationAssessmentProviderError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(CitationAssessmentProviderError::Failed);
        }
        let candidate = input
            .candidates
            .first()
            .ok_or(CitationAssessmentProviderError::Failed)?;
        Ok(CitationAssessment {
            overall_relation: nineprofs_research::ClaimEvidenceRelation::Supports,
            rationale: "fixture supports the claim".into(),
            selected_candidates: vec![nineprofs_research_verification::SelectedCitationCandidate {
                retrieval_chunk_id: candidate.retrieval_chunk_id.clone(),
                relation: nineprofs_research::ClaimEvidenceRelation::Supports,
                rationale: None,
            }],
        })
    }
}

fn input(fixture: &Fixture, block_count: usize) -> StartManuscriptResearchReview {
    StartManuscriptResearchReview {
        research_case_id: fixture.case_id.clone(),
        manuscript_source_id: fixture.source_id.clone(),
        document_id: "document-1".into(),
        document_version: 7,
        citation_review_observations: ManuscriptResearchReviewCitationObservations {
            citations: Vec::<CitationReviewCitationInput>::new(),
            citation_blocks: Vec::<CitationReviewBlockInput>::new(),
        },
        claim_inventory_observations: ManuscriptResearchReviewClaimInventoryObservations {
            whole_manuscript_blocks: (0..block_count)
                .map(|ordinal| ManuscriptClaimInventoryBlockInput {
                    block_id: format!("block-{ordinal}"),
                    block_ordinal: ordinal as u32,
                    block_kind: ManuscriptClaimInventoryBlockKind::Paragraph,
                    text: format!("Claim {ordinal} needs evidence."),
                    citations: Vec::new(),
                })
                .collect(),
        },
    }
}

fn input_with_citation(fixture: &Fixture) -> StartManuscriptResearchReview {
    let mut input = input(fixture, 1);
    let block_text = "Claim 0 needs evidence. [1]".to_owned();
    input.claim_inventory_observations.whole_manuscript_blocks[0].text = block_text.clone();
    input.claim_inventory_observations.whole_manuscript_blocks[0]
        .citations
        .push(ManuscriptClaimInventoryCitationInput {
            start: 24,
            end: 27,
            rendered_text: "[1]".to_owned(),
        });
    input.citation_review_observations = ManuscriptResearchReviewCitationObservations {
        citations: vec![CitationReviewCitationInput {
            format: ManuscriptCitationFormat::Zotero,
            rendered_text: "[1]".to_owned(),
            block_id: "block-0".to_owned(),
            start: 24,
            end: 27,
            targets: vec![CitationReviewTargetInput {
                ordinal: 0,
                reference_key: "exact".to_owned(),
                cited_locator: Some("p1".to_owned()),
                word_source: None,
                zotero: Some(ManuscriptReferenceCatalogZoteroInput {
                    item_id: Some("item-exact".to_owned()),
                    uris: Vec::new(),
                }),
            }],
        }],
        citation_blocks: vec![CitationReviewBlockInput {
            block_id: "block-0".to_owned(),
            text: block_text,
            citations: vec![
                nineprofs_research_verification::CitationReviewBlockCitationInput {
                    start: 24,
                    end: 27,
                    rendered_text: "[1]".to_owned(),
                },
            ],
        }],
    };
    input
}

async fn ready_pdf(
    database: &Database,
    research: &ResearchService,
    case_id: &str,
    source: &nineprofs_research::ResearchSource,
) -> nineprofs_research::ResearchPdfExtraction {
    let store = research.artifact_store().unwrap();
    let mut upload = store.begin_upload("fixture.pdf".to_owned()).unwrap();
    upload.append(b"%PDF-1.7 fixture").unwrap();
    let artifact = upload.finish().await.unwrap();
    let snapshot = research
        .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
        .await
        .unwrap();
    let extraction = research
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id,
            extractor: "fixture".to_owned(),
            extractor_version: Some("1".to_owned()),
            page_count: 1,
            status: nineprofs_research::PdfExtractionStatus::Ready,
            pages: vec![CapturePdfPage {
                page: 1,
                text: "canonical evidence".to_owned(),
            }],
        })
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO research_dify_case_indexes (id, research_case_id, dataset_id, status, created_at_ms, updated_at_ms) VALUES (?, ?, ?, 'ready', 1, 1)",
    )
    .bind("case-index-1")
    .bind(case_id)
    .bind("dataset-1")
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO research_dify_extraction_indexes (id, case_index_id, research_case_id, extraction_id, source_snapshot_id, chunker_version, status, created_at_ms, updated_at_ms) VALUES (?, ?, ?, ?, ?, 'fixture', 'ready', 1, 1)",
    )
    .bind("extraction-index-1")
    .bind("case-index-1")
    .bind(case_id)
    .bind(extraction.id.to_string())
    .bind(extraction.source_snapshot_id.to_string())
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO research_retrieval_chunks (id, extraction_index_id, research_case_id, research_source_id, source_snapshot_id, extraction_id, page, start_offset, end_offset, text, hash_algorithm, text_hash) VALUES (?, ?, ?, ?, ?, ?, 1, 0, ?, ?, 'sha256', 'fixture-hash')",
    )
    .bind("chunk-1")
    .bind("extraction-index-1")
    .bind(case_id)
    .bind(source.id.to_string())
    .bind(extraction.source_snapshot_id.to_string())
    .bind(extraction.id.to_string())
    .bind("retrieval evidence".chars().count() as i64)
    .bind("retrieval evidence")
    .execute(database.pool())
    .await
    .unwrap();
    extraction
}

async fn prepare_citation_fixture(
    fixture: &Fixture,
) -> (
    nineprofs_research::ResearchSource,
    nineprofs_research::ResearchPdfExtraction,
) {
    let reference = fixture
        .research
        .create_source(CreateResearchSource {
            research_case_id: ResearchCaseId::parse(fixture.case_id.clone()).unwrap(),
            kind: SourceKind::ReferencePdf,
            label: "Reference".to_owned(),
            identity: Some(ResearchSourceIdentityInput {
                provider: "zotero".to_owned(),
                external_reference: "item-exact".to_owned(),
                method: ResearchSourceIdentityMethod::Imported,
            }),
        })
        .await
        .unwrap();
    let extraction = ready_pdf(
        &fixture.database,
        fixture.research.as_ref(),
        fixture.case_id.as_str(),
        &reference,
    )
    .await;
    *fixture.retrieval_candidate.lock().unwrap() = Some(CitationRetrievalCandidate {
        retrieval_chunk_id: "chunk-1".to_owned(),
        research_source_id: reference.id.to_string(),
        source_snapshot_id: extraction.source_snapshot_id.to_string(),
        extraction_id: extraction.id.to_string(),
        page: 1,
        start: 0,
        end: 18,
        verbatim_excerpt: "canonical evidence".to_owned(),
        retrieval_score: 1.0,
        provider: "fixture-retrieval".to_owned(),
        rank: 1,
    });
    (reference, extraction)
}

async fn count(database: &Database, table: &str, where_clause: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table} {where_clause}"))
        .fetch_one(database.pool())
        .await
        .unwrap()
}

async fn insert_review(
    database: &Database,
    id: &str,
    execution_hash: &str,
    status: &str,
) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO research_manuscript_research_review_runs
         (review_run_id, research_case_id, manuscript_source_id, document_id,
          document_version, input_hash_algorithm, input_hash,
          execution_identity_hash_algorithm, execution_identity_hash,
          citation_review_run_id, claim_inventory_run_id, claim_coverage_run_id,
          citation_expectation_run_id, cross_claim_candidate_run_id,
          cross_claim_assessment_run_id,
          review_contract_version, status, created_at_ms)
         VALUES (?, 'missing-case', 'missing-source', 'doc', 1, 'sha256', 'same-input',
                 'sha256', ?, 'citation-review', 'inventory', 'coverage',
                 'expectation', 'candidate', 'assessment', 'review-v1', ?, 1)",
    )
    .bind(id)
    .bind(execution_hash)
    .bind(status)
    .execute(database.pool())
    .await
}

fn completion_events(
    receiver: &mut tokio::sync::broadcast::Receiver<nineprofs_api_types::EventEnvelope>,
) -> usize {
    workflow_event_counts(receiver).0
}

fn workflow_event_counts(
    receiver: &mut tokio::sync::broadcast::Receiver<nineprofs_api_types::EventEnvelope>,
) -> (usize, usize) {
    let mut completed = 0;
    let mut failed = 0;
    while let Ok(event) = receiver.try_recv() {
        if event.name == "research.manuscriptResearchReviewCompleted" {
            completed += 1;
        } else if event.name == "research.manuscriptResearchReviewFailed" {
            failed += 1;
        }
    }
    (completed, failed)
}

#[tokio::test]
async fn whole_review_orchestrates_all_stages_and_pins_dependencies() {
    let fixture = fixture(false, None).await;
    let run = fixture
        .service
        .start_manuscript_research_review(input(&fixture, 1))
        .await
        .unwrap();
    assert!(matches!(
        run.status,
        ManuscriptResearchReviewRunStatus::Completed
    ));
    assert!(run.citation_review_run_id.is_some());
    assert!(run.claim_inventory_run_id.is_some());
    assert!(run.claim_coverage_run_id.is_some());
    assert!(run.citation_expectation_run_id.is_some());
    assert!(run.cross_claim_candidate_run_id.is_some());
    assert!(run.cross_claim_assessment_run_id.is_some());
    assert_eq!(run.input_hash_algorithm, "sha256");
    assert_eq!(
        run.execution_identity_hash_algorithm.as_deref(),
        Some("sha256")
    );
    assert!(run.execution_identity_hash.is_some());

    let citation = fixture
        .service
        .get_manuscript_citation_review(run.citation_review_run_id.as_ref().unwrap())
        .await
        .unwrap();
    let coverage = fixture
        .service
        .get_manuscript_claim_coverage(run.claim_coverage_run_id.as_ref().unwrap())
        .await
        .unwrap();
    let candidate = fixture
        .service
        .get_manuscript_cross_claim_candidates_run(
            run.cross_claim_candidate_run_id.as_ref().unwrap(),
        )
        .await
        .unwrap();
    let assessment = fixture
        .service
        .get_manuscript_cross_claim_assessment(run.cross_claim_assessment_run_id.as_ref().unwrap())
        .await
        .unwrap();
    assert_eq!(
        coverage.claim_inventory_run_id,
        run.claim_inventory_run_id.clone().unwrap()
    );
    assert_eq!(coverage.citation_review_run_id, citation.review_run_id);
    assert_eq!(
        candidate.claim_inventory_run_id,
        run.claim_inventory_run_id.clone().unwrap()
    );
    assert_eq!(assessment.candidate_run_id, candidate.candidate_run_id);
    assert_eq!(
        assessment.claim_inventory_run_id,
        run.claim_inventory_run_id.clone().unwrap()
    );
    assert_eq!(citation.document_id, run.document_id);
    assert_eq!(candidate.document_version, run.document_version);
}

#[tokio::test]
async fn zero_citation_whole_review_keeps_neutral_coverage_and_external_expectation() {
    let fixture = fixture(false, None).await;
    let run = fixture
        .service
        .start_manuscript_research_review(input(&fixture, 2))
        .await
        .unwrap();
    let claims = fixture
        .service
        .list_manuscript_research_review_claims(&run.review_run_id)
        .await
        .unwrap();
    assert_eq!(claims.len(), 2);
    assert!(claims.iter().all(|claim| {
        claim.structural_citation_state
            == ManuscriptClaimCoverageStructuralCitationState::NoCitationObservedInBlock
            && matches!(
                claim.attention_state,
                nineprofs_research_verification::CoverageAttentionState::ReviewSuggested
            )
            && matches!(
                claim.expectation,
                Some(CitationExpectation::ExternalEvidenceExpected)
            )
    }));
    assert_eq!(
        count(&fixture.database, "research_citation_occurrences", "").await,
        0
    );
    assert_eq!(
        count(&fixture.database, "research_claim_citations", "").await,
        0
    );
    assert_eq!(
        count(&fixture.database, "research_citation_targets", "").await,
        0
    );
}

#[tokio::test]
async fn whole_review_projection_uses_canonical_research_evidence_provenance() {
    let fixture = fixture(true, None).await;
    let (reference, extraction) = prepare_citation_fixture(&fixture).await;
    let run = fixture
        .service
        .start_manuscript_research_review(input_with_citation(&fixture))
        .await
        .unwrap();
    assert!(matches!(
        run.status,
        ManuscriptResearchReviewRunStatus::Completed
    ));

    let claims = fixture
        .service
        .list_manuscript_research_review_claims(&run.review_run_id)
        .await
        .unwrap();
    dbg!(&claims);
    let target = claims[0].targets.first().expect("exact citation target");
    let evidence = target.evidence.first().expect("verified evidence");
    let (stored_source_snapshot_id, stored_excerpt) = sqlx::query_as::<_, (String, String)>(
        "SELECT source_snapshot_id, verbatim_excerpt FROM research_evidence WHERE id = ?",
    )
    .bind(&evidence.evidence_id)
    .fetch_one(fixture.database.pool())
    .await
    .unwrap();

    assert_eq!(
        target.source_id.as_deref(),
        Some(reference.id.to_string().as_str())
    );
    assert_eq!(
        target.source_snapshot_id.as_deref(),
        Some(extraction.source_snapshot_id.to_string().as_str())
    );
    assert_eq!(
        target.extraction_id.as_deref(),
        Some(extraction.id.to_string().as_str())
    );
    assert_eq!(evidence.source_snapshot_id, stored_source_snapshot_id);
    assert_eq!(evidence.verbatim_excerpt, stored_excerpt);
    assert_eq!(evidence.verbatim_excerpt, "canonical evidence");
}

#[tokio::test]
async fn citation_item_failure_remains_a_completed_whole_review_result() {
    let fixture = fixture(true, None).await;
    prepare_citation_fixture(&fixture).await;
    fixture
        .citation_fail
        .as_ref()
        .unwrap()
        .store(true, Ordering::SeqCst);
    let run = fixture
        .service
        .start_manuscript_research_review(input_with_citation(&fixture))
        .await
        .unwrap();

    assert!(matches!(
        run.status,
        ManuscriptResearchReviewRunStatus::Completed
    ));
    let citation = fixture
        .service
        .get_manuscript_citation_review(run.citation_review_run_id.as_ref().unwrap())
        .await
        .unwrap();
    let items = fixture
        .service
        .list_manuscript_citation_review_items(&citation.review_run_id)
        .await
        .unwrap();
    assert!(matches!(
        items[0].status,
        nineprofs_research_verification::CitationReviewItemStatus::VerificationFailed
    ));
}

#[tokio::test]
async fn same_config_sequential_start_reuses_one_completed_whole_review() {
    let fixture = fixture(false, None).await;
    let mut events = fixture.events.subscribe();
    let request = input(&fixture, 1);
    let first = fixture
        .service
        .start_manuscript_research_review(request.clone())
        .await
        .unwrap();
    let (completed_events, failed_events) = workflow_event_counts(&mut events);
    assert_eq!(completed_events, 1);
    assert_eq!(failed_events, 0);
    let second = fixture
        .service
        .start_manuscript_research_review(request)
        .await
        .unwrap();
    assert_eq!(first.review_run_id, second.review_run_id);
    assert_eq!(completion_events(&mut events), 0);
    assert_eq!(
        count(
            &fixture.database,
            "research_manuscript_research_review_runs",
            "WHERE status = 'completed'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn expectation_model_change_creates_a_new_completed_history() {
    let fixture = fixture(false, None).await;
    let request = input(&fixture, 1);
    let first = fixture
        .service
        .start_manuscript_research_review(request.clone())
        .await
        .unwrap();
    fixture.expectation_identity.lock().unwrap().model_id = Some("expectation-model-b".into());
    let second = fixture
        .service
        .start_manuscript_research_review(request)
        .await
        .unwrap();
    assert_ne!(first.review_run_id, second.review_run_id);
    assert!(matches!(
        first.status,
        ManuscriptResearchReviewRunStatus::Completed
    ));
    assert!(matches!(
        second.status,
        ManuscriptResearchReviewRunStatus::Completed
    ));
    assert_ne!(
        first.citation_expectation_run_id,
        second.citation_expectation_run_id
    );
    assert_eq!(
        count(
            &fixture.database,
            "research_manuscript_research_review_runs",
            "WHERE status = 'completed'"
        )
        .await,
        2
    );
}

#[tokio::test]
async fn nested_claim_extractor_model_change_creates_a_new_completed_history() {
    let fixture = fixture(false, None).await;
    let request = input(&fixture, 1);
    let first = fixture
        .service
        .start_manuscript_research_review(request.clone())
        .await
        .unwrap();
    let first_citation = fixture
        .service
        .get_manuscript_citation_review(first.citation_review_run_id.as_ref().unwrap())
        .await
        .unwrap();
    let first_extraction_id = first_citation.claim_extraction_run_id.clone().unwrap();
    fixture.extractor_identity.lock().unwrap().model_id = Some("extractor-model-b".into());
    let second = fixture
        .service
        .start_manuscript_research_review(request)
        .await
        .unwrap();
    let second_citation = fixture
        .service
        .get_manuscript_citation_review(second.citation_review_run_id.as_ref().unwrap())
        .await
        .unwrap();
    assert_ne!(first.review_run_id, second.review_run_id);
    assert_ne!(
        first_extraction_id,
        second_citation.claim_extraction_run_id.unwrap()
    );
    assert_eq!(
        count(
            &fixture.database,
            "research_manuscript_research_review_runs",
            "WHERE status = 'completed'"
        )
        .await,
        2
    );
}

#[tokio::test]
async fn equivalent_concurrent_starts_converge_without_failed_whole_review() {
    let fixture = fixture(false, Some(Arc::new(Barrier::new(2)))).await;
    let mut events = fixture.events.subscribe();
    let left = fixture.service.clone();
    let right = fixture.service.clone();
    let left_input = input(&fixture, 1);
    let right_input = left_input.clone();
    let (left, right) = tokio::join!(
        left.start_manuscript_research_review(left_input),
        right.start_manuscript_research_review(right_input),
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.review_run_id, right.review_run_id);
    let (completed_events, failed_events) = workflow_event_counts(&mut events);
    assert_eq!(completed_events, 1);
    assert_eq!(failed_events, 0);
    assert_eq!(
        count(
            &fixture.database,
            "research_manuscript_research_review_runs",
            "WHERE status = 'completed'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &fixture.database,
            "research_manuscript_research_review_runs",
            "WHERE status = 'failed'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn systemic_inventory_failure_stops_later_whole_review_stages() {
    let fixture = fixture(false, None).await;
    fixture.inventory_fail.store(true, Ordering::SeqCst);
    let run = fixture
        .service
        .start_manuscript_research_review(input(&fixture, 1))
        .await
        .unwrap();
    assert!(matches!(
        run.status,
        ManuscriptResearchReviewRunStatus::Failed
    ));
    assert_eq!(run.failure_stage.as_deref(), Some("claim_inventory"));
    assert_eq!(run.claim_inventory_run_id, None);
    assert_eq!(run.claim_coverage_run_id, None);
    assert_eq!(run.citation_expectation_run_id, None);
    assert_eq!(run.cross_claim_candidate_run_id, None);
    assert_eq!(run.cross_claim_assessment_run_id, None);
}

#[tokio::test]
async fn failed_whole_review_attempt_can_retry_and_remains_historical() {
    let fixture = fixture(false, None).await;
    let request = input(&fixture, 1);
    fixture.inventory_fail.store(true, Ordering::SeqCst);
    let failed = fixture
        .service
        .start_manuscript_research_review(request.clone())
        .await
        .unwrap();
    fixture.inventory_fail.store(false, Ordering::SeqCst);
    let completed = fixture
        .service
        .start_manuscript_research_review(request)
        .await
        .unwrap();
    assert!(matches!(
        failed.status,
        ManuscriptResearchReviewRunStatus::Failed
    ));
    assert!(matches!(
        completed.status,
        ManuscriptResearchReviewRunStatus::Completed
    ));
    assert_ne!(failed.review_run_id, completed.review_run_id);
    assert_eq!(
        count(
            &fixture.database,
            "research_manuscript_research_review_runs",
            "WHERE status = 'failed'"
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &fixture.database,
            "research_manuscript_research_review_runs",
            "WHERE status = 'completed'"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn item_level_expectation_and_cross_claim_failures_still_complete_whole_review() {
    let fixture = fixture(false, None).await;
    fixture.expectation_fail.store(true, Ordering::SeqCst);
    fixture.assessment_fail.store(true, Ordering::SeqCst);
    let run = fixture
        .service
        .start_manuscript_research_review(input(&fixture, 2))
        .await
        .unwrap();
    assert!(matches!(
        run.status,
        ManuscriptResearchReviewRunStatus::Completed
    ));
    let claims = fixture
        .service
        .list_manuscript_research_review_claims(&run.review_run_id)
        .await
        .unwrap();
    assert!(claims.iter().all(|claim| {
        matches!(
            claim.assessment_status,
            nineprofs_research_verification::CitationExpectationAssessmentStatus::AssessmentFailed
        )
    }));
    let consistency = fixture
        .service
        .list_manuscript_research_review_consistency(&run.review_run_id)
        .await
        .unwrap();
    assert_eq!(consistency.len(), 1);
    assert!(matches!(
        consistency[0].assessment_status,
        nineprofs_research_verification::CrossClaimAssessmentStatus::AssessmentFailed
    ));
}

#[tokio::test]
async fn citation_assessor_model_change_changes_execution_identity() {
    let fixture = fixture(true, None).await;
    let first = fixture
        .service
        .start_manuscript_research_review(input(&fixture, 1))
        .await
        .unwrap();
    let first_hash = first.execution_identity_hash.clone();
    fixture
        .citation_identity
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .model_id = Some("citation-model-b".into());
    let second = fixture
        .service
        .start_manuscript_research_review(input(&fixture, 1))
        .await
        .unwrap();
    assert_ne!(first.review_run_id, second.review_run_id);
    assert_ne!(first_hash, second.execution_identity_hash);
}

#[tokio::test]
async fn citation_assessor_change_with_applicable_item_creates_new_completed_history() {
    let fixture = fixture(true, None).await;
    let (_, _) = prepare_citation_fixture(&fixture).await;
    let first = fixture
        .service
        .start_manuscript_research_review(input_with_citation(&fixture))
        .await
        .unwrap();
    fixture
        .citation_identity
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .model_id = Some("citation-model-b".into());
    let second = fixture
        .service
        .start_manuscript_research_review(input_with_citation(&fixture))
        .await
        .unwrap();

    assert_ne!(first.review_run_id, second.review_run_id);
    assert_ne!(first.citation_review_run_id, second.citation_review_run_id);
    assert!(matches!(
        first.status,
        ManuscriptResearchReviewRunStatus::Completed
    ));
    assert!(matches!(
        second.status,
        ManuscriptResearchReviewRunStatus::Completed
    ));
}

#[tokio::test]
async fn migration_allows_distinct_execution_identities_but_not_duplicate_completed_identity() {
    let database = Database::in_memory().await.unwrap();
    // Foreign keys are disabled only for this isolated index-level test; no domain row is created.
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(database.pool())
        .await
        .unwrap();
    insert_review(&database, "review-a", "execution-a", "completed")
        .await
        .unwrap();
    insert_review(&database, "review-b", "execution-b", "completed")
        .await
        .unwrap();
    let duplicate = insert_review(&database, "review-c", "execution-a", "completed").await;
    assert!(duplicate.is_err());
    insert_review(&database, "review-failed", "execution-a", "failed")
        .await
        .unwrap();
    assert_eq!(
        count(
            &database,
            "research_manuscript_research_review_runs",
            "WHERE status = 'completed'"
        )
        .await,
        2
    );
    assert_eq!(
        count(
            &database,
            "research_manuscript_research_review_runs",
            "WHERE status = 'failed'"
        )
        .await,
        1
    );
}
