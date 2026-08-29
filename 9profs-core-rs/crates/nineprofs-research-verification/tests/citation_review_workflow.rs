use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CapturePdfExtraction, CapturePdfPage, CitationBindingMethod, CreateCitationTargetBinding,
    CreateResearchCase, CreateResearchSource, ManuscriptCitationFormat,
    ManuscriptCitationSyncCitationInput, ManuscriptCitationSyncTargetInput,
    ManuscriptClaimExtractionBlockInput, ManuscriptClaimExtractionIdentity,
    ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProvider,
    ManuscriptClaimExtractionProviderError, ManuscriptReferenceCatalogZoteroInput,
    ResearchArtifactStore, ResearchService, ResearchSourceIdentityInput,
    ResearchSourceIdentityMethod, SourceKind, SqliteResearchRepository, SyncManuscriptCitations,
};
use nineprofs_research_verification::{
    CitationAssessment, CitationAssessmentInput, CitationAssessmentProvider,
    CitationAssessmentProviderError, CitationAssessmentProviderIdentity, CitationExpectation,
    CitationExpectationAssessment, CitationExpectationAssessmentStatus, CitationExpectationInput,
    CitationExpectationProvider, CitationExpectationProviderError,
    CitationExpectationProviderIdentity, CitationRetrievalCandidate, CitationRetrievalError,
    CitationRetrievalProvider, CitationReviewBlockInput, CitationReviewCitationInput,
    CitationReviewError, CitationReviewItemStatus, CitationReviewRunStatus, CitationReviewService,
    CitationReviewTargetInput, CoverageAttentionState, ManuscriptClaimCoverageBridgeStatus,
    ManuscriptClaimCoverageRunStatus, ManuscriptClaimCoverageStructuralCitationState,
    SelectedCitationCandidate, StartManuscriptCitationExpectation, StartManuscriptCitationReview,
    StartManuscriptClaimCoverage,
};
use sha2::{Digest, Sha256};

struct EchoExtractor;

#[async_trait]
impl ManuscriptClaimExtractionProvider for EchoExtractor {
    fn identity(&self) -> ManuscriptClaimExtractionIdentity {
        ManuscriptClaimExtractionIdentity {
            provider: "citation-review-test".to_owned(),
            extractor_version: "1".to_owned(),
            model_id: None,
            extraction_contract_version: "1".to_owned(),
        }
    }

    async fn extract(
        &self,
        block: ManuscriptClaimExtractionBlockInput,
    ) -> Result<ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProviderError> {
        let citation_occurrence_ids = block
            .citations
            .into_iter()
            .map(|citation| citation.citation_occurrence_id)
            .collect::<Vec<_>>();
        Ok(ManuscriptClaimExtractionOutput {
            claims: if citation_occurrence_ids.is_empty() {
                Vec::new()
            } else {
                vec![nineprofs_research::ManuscriptClaimExtractionClaimOutput {
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

struct FixtureRetrieval {
    candidate: Option<CitationRetrievalCandidate>,
}

#[async_trait]
impl CitationRetrievalProvider for FixtureRetrieval {
    async fn retrieve_exact_extraction(
        &self,
        _research_case_id: &str,
        _extraction_id: &str,
        _query: &str,
        _top_k: u32,
    ) -> Result<Vec<CitationRetrievalCandidate>, CitationRetrievalError> {
        Ok(self.candidate.clone().into_iter().collect())
    }
}

struct SupportsAssessor {
    fail_locator: Option<String>,
}

struct MixedAssessor;

#[async_trait]
impl CitationAssessmentProvider for SupportsAssessor {
    fn identity(&self) -> CitationAssessmentProviderIdentity {
        CitationAssessmentProviderIdentity {
            provider_id: "citation-review-test".to_owned(),
            implementation_version: "1".to_owned(),
            model_id: None,
        }
    }

    async fn assess(
        &self,
        input: CitationAssessmentInput,
    ) -> Result<CitationAssessment, CitationAssessmentProviderError> {
        if self.fail_locator.as_deref() == input.cited_locator.as_deref() {
            return Err(CitationAssessmentProviderError::Failed);
        }
        let candidate = input
            .candidates
            .first()
            .ok_or(CitationAssessmentProviderError::Failed)?;
        Ok(CitationAssessment {
            overall_relation: nineprofs_research::ClaimEvidenceRelation::Supports,
            rationale: "fixture supports the claim".to_owned(),
            selected_candidates: vec![SelectedCitationCandidate {
                retrieval_chunk_id: candidate.retrieval_chunk_id.clone(),
                relation: nineprofs_research::ClaimEvidenceRelation::Supports,
                rationale: None,
            }],
        })
    }
}

#[async_trait]
impl CitationAssessmentProvider for MixedAssessor {
    fn identity(&self) -> CitationAssessmentProviderIdentity {
        CitationAssessmentProviderIdentity {
            provider_id: "citation-review-mixed-test".to_owned(),
            implementation_version: "1".to_owned(),
            model_id: None,
        }
    }

    async fn assess(
        &self,
        input: CitationAssessmentInput,
    ) -> Result<CitationAssessment, CitationAssessmentProviderError> {
        let relation = match input.cited_locator.as_deref() {
            Some("p1") => nineprofs_research::ClaimEvidenceRelation::Supports,
            Some("p2") => nineprofs_research::ClaimEvidenceRelation::Contradicts,
            Some("p3") => nineprofs_research::ClaimEvidenceRelation::Insufficient,
            _ => return Err(CitationAssessmentProviderError::Failed),
        };
        let candidate = input
            .candidates
            .first()
            .ok_or(CitationAssessmentProviderError::Failed)?;
        Ok(CitationAssessment {
            overall_relation: relation.clone(),
            rationale: "fixture mixed relation".to_owned(),
            selected_candidates: vec![SelectedCitationCandidate {
                retrieval_chunk_id: candidate.retrieval_chunk_id.clone(),
                relation,
                rationale: None,
            }],
        })
    }
}

struct CapturingExpectationProvider {
    seen: Arc<Mutex<Vec<CitationExpectationInput>>>,
    expectation: CitationExpectation,
    fail_item_id: Option<String>,
    assessor_version: String,
}

#[async_trait]
impl CitationExpectationProvider for CapturingExpectationProvider {
    fn identity(&self) -> CitationExpectationProviderIdentity {
        CitationExpectationProviderIdentity {
            provider_id: "expectation-fixture".to_owned(),
            assessor_version: self.assessor_version.clone(),
            model_id: Some("fixture-model".to_owned()),
        }
    }

    async fn assess(
        &self,
        input: CitationExpectationInput,
    ) -> Result<CitationExpectationAssessment, CitationExpectationProviderError> {
        self.seen.lock().unwrap().push(input.clone());
        if self.fail_item_id.as_deref() == Some(input.item_id.as_str()) {
            return Err(CitationExpectationProviderError::ProviderUnavailable);
        }
        Ok(CitationExpectationAssessment {
            item_id: input.item_id,
            expectation: self.expectation.clone(),
            rationale: "fixture scholarly expectation".to_owned(),
        })
    }
}

async fn base(
    reference: Option<(String, Option<ResearchSourceIdentityInput>)>,
) -> (
    Database,
    Arc<ResearchService>,
    nineprofs_research::ResearchCase,
    nineprofs_research::ResearchSource,
    Option<nineprofs_research::ResearchSource>,
) {
    let database = Database::in_memory().await.unwrap();
    let events = Arc::new(BroadcastEventBus::new(64));
    let artifact_store = Arc::new(ResearchArtifactStore::new(
        std::env::temp_dir().join(format!(
            "9profs-citation-review-{}",
            nineprofs_common::new_id()
        )),
        database.pool().clone(),
    ));
    let research = Arc::new(
        ResearchService::new(
            SqliteResearchRepository::new(database.pool().clone()),
            events,
        )
        .with_artifact_store(artifact_store)
        .with_claim_extractor(Arc::new(EchoExtractor)),
    );
    let case = research
        .create_case(CreateResearchCase {
            title: "Citation review workflow".to_owned(),
        })
        .await
        .unwrap();
    let manuscript = research
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Draft".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let reference_source = match reference {
        Some((label, identity)) => Some(
            research
                .create_source(CreateResearchSource {
                    research_case_id: case.id.clone(),
                    kind: SourceKind::ReferencePdf,
                    label,
                    identity,
                })
                .await
                .unwrap(),
        ),
        None => None,
    };
    (database, research, case, manuscript, reference_source)
}

fn grouped_input(
    manuscript_source_id: &str,
    document_id: &str,
    document_version: i64,
    reference_key: &str,
    locator_b: Option<&str>,
) -> StartManuscriptCitationReview {
    StartManuscriptCitationReview {
        research_case_id: String::new(),
        manuscript_source_id: manuscript_source_id.to_owned(),
        document_id: document_id.to_owned(),
        document_version,
        citations: vec![CitationReviewCitationInput {
            format: ManuscriptCitationFormat::Zotero,
            rendered_text: "[1]".to_owned(),
            block_id: "block-1".to_owned(),
            start: 6,
            end: 9,
            targets: vec![
                CitationReviewTargetInput {
                    ordinal: 0,
                    reference_key: reference_key.to_owned(),
                    cited_locator: Some("p1".to_owned()),
                    word_source: None,
                    zotero: Some(ManuscriptReferenceCatalogZoteroInput {
                        item_id: Some("item-exact".to_owned()),
                        uris: Vec::new(),
                    }),
                },
                CitationReviewTargetInput {
                    ordinal: 1,
                    reference_key: reference_key.to_owned(),
                    cited_locator: locator_b.map(str::to_owned),
                    word_source: None,
                    zotero: Some(ManuscriptReferenceCatalogZoteroInput {
                        item_id: Some("item-exact".to_owned()),
                        uris: Vec::new(),
                    }),
                },
            ],
        }],
        blocks: vec![CitationReviewBlockInput {
            block_id: "block-1".to_owned(),
            text: "Claim [1]".to_owned(),
            citations: vec![
                nineprofs_research_verification::CitationReviewBlockCitationInput {
                    start: 6,
                    end: 9,
                    rendered_text: "[1]".to_owned(),
                },
            ],
        }],
    }
}

fn triple_target_input(
    manuscript_source_id: &str,
    document_id: &str,
    document_version: i64,
) -> StartManuscriptCitationReview {
    let mut input = grouped_input(
        manuscript_source_id,
        document_id,
        document_version,
        "exact",
        Some("p2"),
    );
    input.citations[0].targets.push(CitationReviewTargetInput {
        ordinal: 2,
        reference_key: "exact".to_owned(),
        cited_locator: Some("p3".to_owned()),
        word_source: None,
        zotero: Some(ManuscriptReferenceCatalogZoteroInput {
            item_id: Some("item-exact".to_owned()),
            uris: Vec::new(),
        }),
    });
    input
}

async fn review_service(
    database: &Database,
    research: Arc<ResearchService>,
    retrieval: FixtureRetrieval,
    assessor: Option<Arc<dyn CitationAssessmentProvider>>,
) -> (
    CitationReviewService,
    Arc<nineprofs_research_verification::CitationVerificationService>,
) {
    let events = Arc::new(BroadcastEventBus::new(64));
    let verification = nineprofs_research_verification::CitationVerificationService::new(
        database.pool().clone(),
        research.clone(),
        Arc::new(retrieval),
        events.clone(),
    );
    let verification = match assessor {
        Some(assessor) => verification.with_assessor(assessor),
        None => verification,
    };
    let verification = Arc::new(verification);
    let service = CitationReviewService::new(
        database.pool().clone(),
        research,
        verification.clone(),
        events,
    );
    (service, verification)
}

fn identity(item_id: &str) -> ResearchSourceIdentityInput {
    ResearchSourceIdentityInput {
        provider: "zotero".to_owned(),
        external_reference: item_id.to_owned(),
        method: ResearchSourceIdentityMethod::Imported,
    }
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
    .bind("canonical evidence".chars().count() as i64)
    .bind("canonical evidence")
    .execute(database.pool())
    .await
    .unwrap();
    extraction
}

async fn failed_pdf(research: &ResearchService, source: &nineprofs_research::ResearchSource) {
    let store = research.artifact_store().unwrap();
    let mut upload = store.begin_upload("failed-fixture.pdf".to_owned()).unwrap();
    upload.append(b"%PDF-1.7 failed fixture").unwrap();
    let artifact = upload.finish().await.unwrap();
    let snapshot = research
        .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
        .await
        .unwrap();
    research
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id,
            extractor: "fixture".to_owned(),
            extractor_version: Some("1".to_owned()),
            page_count: 1,
            status: nineprofs_research::PdfExtractionStatus::Failed,
            pages: Vec::new(),
        })
        .await
        .unwrap();
}

async fn seed_completed_inventory(
    database: &Database,
    case_id: &str,
    manuscript_source_id: &str,
    document_id: &str,
    document_version: i64,
    run_id: &str,
    items: &[(&str, &str, i64, i64, &str, i64)],
) {
    sqlx::query(
        "INSERT INTO research_manuscript_claim_inventory_runs
         (id, research_case_id, manuscript_source_id, document_id, document_version,
          document_context_hash_algorithm, document_context_hash, extractor_provider,
          extractor_version, extractor_model_id, extraction_contract_version,
          coverage_contract_version, coverage_scope, coverage_limitations_json, status,
          item_count, covered_block_count, created_at_ms, completed_at_ms, failure_code)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(run_id)
    .bind(case_id)
    .bind(manuscript_source_id)
    .bind(document_id)
    .bind(document_version)
    .bind("sha256")
    .bind("fixture-context")
    .bind("inventory-fixture")
    .bind("1")
    .bind(None::<String>)
    .bind("inventory-v1")
    .bind("inventory-coverage-v1")
    .bind("visible_paragraphs")
    .bind(r#"["tables","textboxes","footnotes"]"#)
    .bind("completed")
    .bind(items.len() as i64)
    .bind(1_i64)
    .bind(1_i64)
    .bind(1_i64)
    .bind(None::<String>)
    .execute(database.pool())
    .await
    .unwrap();

    for (ordinal, (item_id, claim_text, source_start, source_end, excerpt, overlap_count)) in
        items.iter().enumerate()
    {
        let mut digest = Sha256::new();
        digest.update(excerpt.as_bytes());
        let excerpt_hash = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        sqlx::query(
            "INSERT INTO research_manuscript_claim_inventory_items
             (id, inventory_run_id, ordinal, document_block_id, block_ordinal, block_kind,
              source_start, source_end, source_excerpt, source_excerpt_hash_algorithm,
              source_excerpt_hash, claim_text, review_kind, overlapping_citation_count)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(item_id)
        .bind(run_id)
        .bind(ordinal as i64)
        .bind("block-1")
        .bind(0_i64)
        .bind("paragraph")
        .bind(*source_start)
        .bind(*source_end)
        .bind(excerpt)
        .bind("sha256")
        .bind(excerpt_hash)
        .bind(claim_text)
        .bind("external_evidence")
        .bind(*overlap_count)
        .execute(database.pool())
        .await
        .unwrap();
    }
}

async fn seed_expectation_coverage(
    database: &Database,
    case_id: &str,
    manuscript_source_id: &str,
    inventory_run_id: &str,
    citation_review_run_id: &str,
    coverage_run_id: &str,
    inventory_item_id: &str,
    exact_link_count: i64,
    support_count: i64,
) {
    sqlx::query(
        "INSERT INTO research_manuscript_citation_review_runs
         (id, research_case_id, manuscript_source_id, document_id, document_version, status, created_at_ms, completed_at_ms)
         VALUES (?, ?, ?, 'doc-7', 7, 'completed', 1, 1)",
    )
    .bind(citation_review_run_id)
    .bind(case_id)
    .bind(manuscript_source_id)
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO research_manuscript_claim_coverage_runs
         (coverage_run_id, research_case_id, manuscript_source_id, document_id, document_version,
          claim_inventory_run_id, citation_review_run_id, analysis_contract_version,
          coverage_contract_version, coverage_scope, coverage_limitations_json, status,
          item_count, created_at_ms, completed_at_ms)
         VALUES (?, ?, ?, 'doc-7', 7, ?, ?, 'coverage-v1', 'inventory-coverage-v1',
                 'visible_paragraphs', '[\"tables\"]', 'completed', 1, 1, 1)",
    )
    .bind(coverage_run_id)
    .bind(case_id)
    .bind(manuscript_source_id)
    .bind(inventory_run_id)
    .bind(citation_review_run_id)
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO research_manuscript_claim_coverage_items
         (coverage_item_id, coverage_run_id, inventory_item_id, ordinal, bridge_status,
          structural_citation_state, inventory_overlapping_citation_count,
          same_block_citation_count, claim_range_citation_count, exact_claim_citation_link_count,
          target_count, support_count, contradiction_count, contextualize_count,
          insufficient_count, unverified_count, blocked_count)
         VALUES ('coverage-item', ?, ?, 0, 'exact_claim_bridge', 'exact_citation_linked',
                 1, 1, 1, ?, ?, ?, 0, 0, 0, 0, 0)",
    )
    .bind(coverage_run_id)
    .bind(inventory_item_id)
    .bind(exact_link_count)
    .bind(exact_link_count)
    .bind(support_count)
    .execute(database.pool())
    .await
    .unwrap();
}

async fn count_rows(database: &Database, table: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(database.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn citation_expectation_is_blind_and_idempotent() {
    let (database, research, case, manuscript, _) = base(None).await;
    seed_completed_inventory(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "doc-7",
        7,
        "expectation-inventory",
        &[("inventory-item", "Claim", 0, 5, "Claim", 1)],
    )
    .await;
    seed_expectation_coverage(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "expectation-inventory",
        "expectation-review",
        "expectation-coverage",
        "inventory-item",
        1,
        1,
    )
    .await;
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let service = service.with_expectation_assessor(Arc::new(CapturingExpectationProvider {
        seen: Arc::clone(&seen),
        expectation: CitationExpectation::ExternalEvidenceContextDependent,
        fail_item_id: None,
        assessor_version: "1".to_owned(),
    }));
    let input = StartManuscriptCitationExpectation {
        research_case_id: case.id.to_string(),
        claim_coverage_run_id: "expectation-coverage".to_owned(),
    };
    let run = service
        .start_manuscript_citation_expectation(input.clone())
        .await
        .unwrap();
    assert_eq!(run.failed_item_count, 0);
    let items = service
        .list_manuscript_citation_expectation_items(&run.expectation_run_id)
        .await
        .unwrap();
    assert_eq!(
        items[0].expectation,
        Some(CitationExpectation::ExternalEvidenceContextDependent)
    );
    assert_eq!(
        items[0].attention,
        CoverageAttentionState::ExpectationReviewNeeded
    );
    let captured = seen.lock().unwrap();
    assert_eq!(captured.len(), 1);
    let input_json = serde_json::to_string(&captured[0]).unwrap();
    assert!(input_json.contains("claimText"));
    assert!(input_json.contains("sourceExcerpt"));
    assert!(input_json.contains("reviewKind"));
    assert!(input_json.contains("blockKind"));
    assert!(!input_json.contains("supportCount"));
    assert!(!input_json.contains("citationOccurrenceId"));
    assert!(!input_json.contains("evidenceCount"));
    assert!(!input_json.contains("structuralCitationState"));
    drop(captured);

    let reused = service
        .start_manuscript_citation_expectation(input.clone())
        .await
        .unwrap();
    assert_eq!(reused.expectation_run_id, run.expectation_run_id);
    assert_eq!(
        count_rows(&database, "research_manuscript_citation_expectation_runs").await,
        1
    );

    let changed = service
        .with_expectation_assessor(Arc::new(CapturingExpectationProvider {
            seen: Arc::clone(&seen),
            expectation: CitationExpectation::ExternalEvidenceContextDependent,
            fail_item_id: None,
            assessor_version: "2".to_owned(),
        }))
        .start_manuscript_citation_expectation(input)
        .await
        .unwrap();
    assert_ne!(changed.expectation_run_id, run.expectation_run_id);
    assert_eq!(
        count_rows(&database, "research_manuscript_citation_expectation_runs").await,
        2
    );
}

#[tokio::test]
async fn citation_expectation_failures_are_item_scoped_and_retryable() {
    let (database, research, case, manuscript, _) = base(None).await;
    seed_completed_inventory(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "doc-7",
        7,
        "retry-inventory",
        &[("inventory-item", "Claim", 0, 5, "Claim", 1)],
    )
    .await;
    seed_expectation_coverage(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "retry-inventory",
        "retry-review",
        "retry-coverage",
        "inventory-item",
        0,
        0,
    )
    .await;
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    let failed = service
        .clone()
        .with_expectation_assessor(Arc::new(CapturingExpectationProvider {
            seen: Arc::new(Mutex::new(Vec::new())),
            expectation: CitationExpectation::ExternalEvidenceExpected,
            fail_item_id: Some("inventory-item".to_owned()),
            assessor_version: "1".to_owned(),
        }))
        .start_manuscript_citation_expectation(StartManuscriptCitationExpectation {
            research_case_id: case.id.to_string(),
            claim_coverage_run_id: "retry-coverage".to_owned(),
        })
        .await
        .unwrap();
    assert_eq!(failed.failed_item_count, 1);
    let failed_item = service
        .clone()
        .with_expectation_assessor(Arc::new(CapturingExpectationProvider {
            seen: Arc::new(Mutex::new(Vec::new())),
            expectation: CitationExpectation::ExternalEvidenceExpected,
            fail_item_id: None,
            assessor_version: "1".to_owned(),
        }))
        .list_manuscript_citation_expectation_items(&failed.expectation_run_id)
        .await
        .unwrap();
    assert_eq!(
        failed_item[0].assessment_status,
        CitationExpectationAssessmentStatus::AssessmentFailed
    );
    assert_eq!(
        failed_item[0].attention,
        CoverageAttentionState::AssessmentUnavailable
    );

    let retried = service
        .with_expectation_assessor(Arc::new(CapturingExpectationProvider {
            seen: Arc::new(Mutex::new(Vec::new())),
            expectation: CitationExpectation::ExternalEvidenceExpected,
            fail_item_id: None,
            assessor_version: "1".to_owned(),
        }))
        .start_manuscript_citation_expectation(StartManuscriptCitationExpectation {
            research_case_id: case.id.to_string(),
            claim_coverage_run_id: "retry-coverage".to_owned(),
        })
        .await
        .unwrap();
    assert_ne!(retried.expectation_run_id, failed.expectation_run_id);
    assert_eq!(retried.failed_item_count, 0);
    assert_eq!(
        count_rows(&database, "research_manuscript_citation_expectation_runs").await,
        2
    );
}

#[tokio::test]
async fn structural_coverage_reuses_exact_grouped_targets_and_is_idempotent() {
    let (database, research, case, manuscript, reference) =
        base(Some(("Reference".to_owned(), Some(identity("item-exact"))))).await;
    let reference = reference.unwrap();
    let extraction = ready_pdf(&database, &research, case.id.as_str(), &reference).await;
    let candidate = CitationRetrievalCandidate {
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
    };
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval {
            candidate: Some(candidate),
        },
        Some(Arc::new(SupportsAssessor { fail_locator: None })),
    )
    .await;
    let mut review_input = grouped_input(manuscript.id.as_str(), "doc-7", 7, "exact", Some("p2"));
    review_input.research_case_id = case.id.to_string();
    let review = service.start(review_input).await.unwrap();
    assert_eq!(review.status, CitationReviewRunStatus::Completed);

    seed_completed_inventory(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "doc-7",
        7,
        "coverage-inventory-exact",
        &[("inventory-item-exact", "Claim", 0, 5, "Claim", 1)],
    )
    .await;

    let side_effect_tables = [
        "research_claims",
        "research_claim_citations",
        "research_citation_target_bindings",
        "research_citation_verification_runs",
        "research_evidence",
        "research_claim_evidence",
    ];
    let mut before = Vec::new();
    for table in side_effect_tables {
        before.push((table, count_rows(&database, table).await));
    }
    let input = StartManuscriptClaimCoverage {
        research_case_id: case.id.to_string(),
        claim_inventory_run_id: "coverage-inventory-exact".to_owned(),
        citation_review_run_id: review.review_run_id.to_string(),
    };
    let coverage = service
        .start_manuscript_claim_coverage(input.clone())
        .await
        .unwrap();
    assert_eq!(coverage.status, ManuscriptClaimCoverageRunStatus::Completed);
    assert_eq!(coverage.item_count, 1);
    assert_eq!(coverage.coverage_scope, "visible_paragraphs");
    assert_eq!(
        coverage.coverage_limitations,
        [
            "tables".to_owned(),
            "textboxes".to_owned(),
            "footnotes".to_owned()
        ]
    );

    let items = service
        .list_manuscript_claim_coverage_items(&coverage.coverage_run_id)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].bridge_status,
        ManuscriptClaimCoverageBridgeStatus::ExactClaimBridge
    );
    assert_eq!(
        items[0].structural_citation_state,
        ManuscriptClaimCoverageStructuralCitationState::ExactCitationLinked
    );
    assert_eq!(items[0].matched_claim_extraction_item_id.is_some(), true);
    assert_eq!(items[0].matched_research_claim_id.is_some(), true);
    assert_eq!(items[0].same_block_citation_count, 1);
    assert_eq!(items[0].claim_range_citation_count, 0);
    assert_eq!(items[0].exact_claim_citation_link_count, 1);
    assert_eq!(items[0].target_count, 2);
    assert_eq!(items[0].support_count, 2);
    assert_eq!(items[0].unverified_count, 0);
    assert_eq!(items[0].blocked_count, 0);

    let targets = service
        .list_manuscript_claim_coverage_targets(
            &coverage.coverage_run_id,
            &items[0].coverage_item_id,
        )
        .await
        .unwrap();
    assert_eq!(targets.len(), 2);
    assert_ne!(targets[0].citation_target_id, targets[1].citation_target_id);
    for target in &targets {
        assert_eq!(
            target.claim_citation_link_id,
            targets[0].claim_citation_link_id
        );
        assert_eq!(
            target.review_status,
            CitationReviewItemStatus::VerificationCompleted
        );
        assert_eq!(
            target.relation,
            Some(nineprofs_research::ClaimEvidenceRelation::Supports)
        );
        assert_eq!(target.verification_run_id.is_some(), true);
        assert_eq!(target.evidence_count, 1);
        assert_eq!(target.evidence.len(), 1);
    }

    let reused = service
        .start_manuscript_claim_coverage(input)
        .await
        .unwrap();
    assert_eq!(reused.coverage_run_id, coverage.coverage_run_id);
    for (table, count) in before {
        assert_eq!(count_rows(&database, table).await, count, "{table}");
    }
}

#[tokio::test]
async fn structural_coverage_preserves_mixed_target_relations() {
    let (database, research, case, manuscript, reference) =
        base(Some(("Reference".to_owned(), Some(identity("item-exact"))))).await;
    let reference = reference.unwrap();
    let extraction = ready_pdf(&database, &research, case.id.as_str(), &reference).await;
    let candidate = CitationRetrievalCandidate {
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
    };
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval {
            candidate: Some(candidate),
        },
        Some(Arc::new(MixedAssessor)),
    )
    .await;
    let mut review_input = triple_target_input(manuscript.id.as_str(), "doc-7", 7);
    review_input.research_case_id = case.id.to_string();
    let review = service.start(review_input).await.unwrap();
    assert_eq!(review.status, CitationReviewRunStatus::Completed);
    seed_completed_inventory(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "doc-7",
        7,
        "coverage-inventory-mixed",
        &[("inventory-item-mixed", "Claim", 0, 5, "Claim", 1)],
    )
    .await;

    let coverage = service
        .start_manuscript_claim_coverage(StartManuscriptClaimCoverage {
            research_case_id: case.id.to_string(),
            claim_inventory_run_id: "coverage-inventory-mixed".to_owned(),
            citation_review_run_id: review.review_run_id.to_string(),
        })
        .await
        .unwrap();
    let items = service
        .list_manuscript_claim_coverage_items(&coverage.coverage_run_id)
        .await
        .unwrap();
    assert_eq!(items[0].target_count, 3);
    assert_eq!(items[0].support_count, 1);
    assert_eq!(items[0].contradiction_count, 1);
    assert_eq!(items[0].insufficient_count, 1);
    assert_eq!(items[0].unverified_count, 0);
    let targets = service
        .list_manuscript_claim_coverage_targets(
            &coverage.coverage_run_id,
            &items[0].coverage_item_id,
        )
        .await
        .unwrap();
    let mut relations = targets
        .iter()
        .map(|target| format!("{:?}", target.relation.clone().unwrap()))
        .collect::<Vec<_>>();
    relations.sort();
    assert_eq!(relations, vec!["Contradicts", "Insufficient", "Supports"]);
}

#[tokio::test]
async fn structural_coverage_keeps_unresolved_targets_distinct_from_no_evidence() {
    let (database, research, case, manuscript, _) = base(None).await;
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    let mut review_input = grouped_input(manuscript.id.as_str(), "doc-7", 7, "missing", Some("p2"));
    review_input.research_case_id = case.id.to_string();
    let review = service.start(review_input).await.unwrap();
    seed_completed_inventory(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "doc-7",
        7,
        "coverage-inventory-unresolved",
        &[("inventory-item-unresolved", "Claim", 0, 5, "Claim", 1)],
    )
    .await;

    let coverage = service
        .start_manuscript_claim_coverage(StartManuscriptClaimCoverage {
            research_case_id: case.id.to_string(),
            claim_inventory_run_id: "coverage-inventory-unresolved".to_owned(),
            citation_review_run_id: review.review_run_id.to_string(),
        })
        .await
        .unwrap();
    let items = service
        .list_manuscript_claim_coverage_items(&coverage.coverage_run_id)
        .await
        .unwrap();
    assert_eq!(
        items[0].structural_citation_state,
        ManuscriptClaimCoverageStructuralCitationState::ExactCitationLinked
    );
    assert_eq!(items[0].target_count, 2);
    assert_eq!(items[0].unverified_count, 2);
    assert_eq!(items[0].blocked_count, 2);
    assert_eq!(items[0].support_count, 0);
    let targets = service
        .list_manuscript_claim_coverage_targets(
            &coverage.coverage_run_id,
            &items[0].coverage_item_id,
        )
        .await
        .unwrap();
    assert!(targets.iter().all(|target| {
        target.review_status == CitationReviewItemStatus::UnresolvedReference
            && target.relation.is_none()
            && target.evidence_count == 0
    }));
}

#[tokio::test]
async fn structural_coverage_projects_not_ready_targets_without_calling_verification() {
    let (database, research, case, manuscript, _) =
        base(Some(("Reference".to_owned(), Some(identity("item-exact"))))).await;
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    let mut review_input = grouped_input(manuscript.id.as_str(), "doc-7", 7, "exact", Some("p2"));
    review_input.research_case_id = case.id.to_string();
    let review = service.start(review_input).await.unwrap();
    seed_completed_inventory(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "doc-7",
        7,
        "coverage-inventory-not-ready",
        &[("inventory-item-not-ready", "Claim", 0, 5, "Claim", 1)],
    )
    .await;

    let coverage = service
        .start_manuscript_claim_coverage(StartManuscriptClaimCoverage {
            research_case_id: case.id.to_string(),
            claim_inventory_run_id: "coverage-inventory-not-ready".to_owned(),
            citation_review_run_id: review.review_run_id.to_string(),
        })
        .await
        .unwrap();
    let items = service
        .list_manuscript_claim_coverage_items(&coverage.coverage_run_id)
        .await
        .unwrap();
    assert_eq!(items[0].target_count, 2);
    assert_eq!(items[0].unverified_count, 2);
    assert_eq!(items[0].blocked_count, 2);
    assert_eq!(items[0].support_count, 0);
    let targets = service
        .list_manuscript_claim_coverage_targets(
            &coverage.coverage_run_id,
            &items[0].coverage_item_id,
        )
        .await
        .unwrap();
    assert!(targets.iter().all(|target| {
        target.review_status == CitationReviewItemStatus::SourceMatchedNotVerificationReady
            && target.verification_status.is_none()
            && target.relation.is_none()
            && target.evidence_count == 0
    }));
}

#[tokio::test]
async fn structural_coverage_keeps_span_mismatch_and_observation_states_neutral() {
    let (database, research, case, manuscript, _) = base(None).await;
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    let mut review_input = grouped_input(manuscript.id.as_str(), "doc-7", 7, "missing", Some("p2"));
    review_input.research_case_id = case.id.to_string();
    let review = service.start(review_input).await.unwrap();
    assert_eq!(review.status, CitationReviewRunStatus::Completed);

    seed_completed_inventory(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "doc-7",
        7,
        "coverage-inventory-neutral",
        &[
            (
                "inventory-item-span-mismatch",
                "Different",
                0,
                5,
                "Claim",
                0,
            ),
            ("inventory-item-no-match", "Uncited", 0, 4, "Clai", 0),
            (
                "inventory-item-range-overlap",
                "Overlapping",
                0,
                7,
                "Claim [",
                1,
            ),
        ],
    )
    .await;
    let coverage = service
        .start_manuscript_claim_coverage(StartManuscriptClaimCoverage {
            research_case_id: case.id.to_string(),
            claim_inventory_run_id: "coverage-inventory-neutral".to_owned(),
            citation_review_run_id: review.review_run_id.to_string(),
        })
        .await
        .unwrap();
    let items = service
        .list_manuscript_claim_coverage_items(&coverage.coverage_run_id)
        .await
        .unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0].bridge_status,
        ManuscriptClaimCoverageBridgeStatus::SameSpanDifferentClaim
    );
    assert_eq!(
        items[0].structural_citation_state,
        ManuscriptClaimCoverageStructuralCitationState::AmbiguousClaimBridge
    );
    assert_eq!(items[0].target_count, 0);
    assert_eq!(
        items[1].bridge_status,
        ManuscriptClaimCoverageBridgeStatus::NoCitationScopedClaimMatch
    );
    assert_eq!(
        items[1].structural_citation_state,
        ManuscriptClaimCoverageStructuralCitationState::CitationObservedInBlock
    );
    assert_eq!(items[1].same_block_citation_count, 1);
    assert_eq!(items[1].claim_range_citation_count, 0);
    assert_eq!(
        items[2].structural_citation_state,
        ManuscriptClaimCoverageStructuralCitationState::CitationObservedInClaimRange
    );
    assert_eq!(items[2].inventory_overlapping_citation_count, 1);
    assert_eq!(items[2].claim_range_citation_count, 1);
    assert_eq!(items[2].target_count, 0);
}

#[tokio::test]
async fn structural_coverage_rejects_cross_case_and_source_without_persisting() {
    let (database, research, case, manuscript, _) = base(None).await;
    let other_source = research
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Other draft".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    let mut review_input = grouped_input(manuscript.id.as_str(), "doc-7", 7, "missing", Some("p2"));
    review_input.research_case_id = case.id.to_string();
    let review = service.start(review_input).await.unwrap();

    seed_completed_inventory(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "doc-7",
        7,
        "coverage-inventory-cross-case",
        &[("inventory-item-cross-case", "Claim", 0, 5, "Claim", 1)],
    )
    .await;
    let cross_case = service
        .start_manuscript_claim_coverage(StartManuscriptClaimCoverage {
            research_case_id: "different-case".to_owned(),
            claim_inventory_run_id: "coverage-inventory-cross-case".to_owned(),
            citation_review_run_id: review.review_run_id.to_string(),
        })
        .await;
    assert!(matches!(cross_case, Err(CitationReviewError::Invalid(_))));

    seed_completed_inventory(
        &database,
        case.id.as_str(),
        other_source.id.as_str(),
        "doc-7",
        7,
        "coverage-inventory-cross-source",
        &[("inventory-item-cross-source", "Claim", 0, 5, "Claim", 1)],
    )
    .await;
    let cross_source = service
        .start_manuscript_claim_coverage(StartManuscriptClaimCoverage {
            research_case_id: case.id.to_string(),
            claim_inventory_run_id: "coverage-inventory-cross-source".to_owned(),
            citation_review_run_id: review.review_run_id.to_string(),
        })
        .await;
    assert!(matches!(cross_source, Err(CitationReviewError::Invalid(_))));
    assert_eq!(
        count_rows(&database, "research_manuscript_claim_coverage_runs").await,
        0
    );
}

#[tokio::test]
async fn structural_coverage_rejects_cross_version_without_persisting_a_matrix() {
    let (database, research, case, manuscript, _) = base(None).await;
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    let mut review_input = grouped_input(manuscript.id.as_str(), "doc-8", 8, "missing", Some("p2"));
    review_input.research_case_id = case.id.to_string();
    let review = service.start(review_input).await.unwrap();
    seed_completed_inventory(
        &database,
        case.id.as_str(),
        manuscript.id.as_str(),
        "doc-7",
        7,
        "coverage-inventory-version-mismatch",
        &[("inventory-item-version-mismatch", "Claim", 0, 5, "Claim", 1)],
    )
    .await;

    let result = service
        .start_manuscript_claim_coverage(StartManuscriptClaimCoverage {
            research_case_id: case.id.to_string(),
            claim_inventory_run_id: "coverage-inventory-version-mismatch".to_owned(),
            citation_review_run_id: review.review_run_id.to_string(),
        })
        .await;
    assert!(matches!(result, Err(CitationReviewError::Invalid(_))));
    assert_eq!(
        count_rows(&database, "research_manuscript_claim_coverage_runs").await,
        0
    );
}

#[tokio::test]
async fn high_level_start_orchestrates_stages_and_preserves_grouped_order() {
    let (database, research, case, manuscript, _) = base(None).await;
    let mut input = grouped_input(manuscript.id.as_str(), "doc-7", 7, "missing", Some("p2"));
    input.research_case_id = case.id.to_string();
    let (service, _) = review_service(
        &database,
        research.clone(),
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;

    let run = service.start(input.clone()).await.unwrap();
    assert_eq!(
        run.status,
        nineprofs_research_verification::CitationReviewRunStatus::Completed
    );
    assert!(run.citation_sync_run_id.is_some());
    assert!(run.reference_catalog_run_id.is_some());
    assert!(run.reference_resolution_run_id.is_some());
    assert!(run.claim_extraction_run_id.is_some());
    assert_eq!(run.document_id, "doc-7");
    assert_eq!(run.document_version, 7);
    let sync = research
        .get_manuscript_citation_sync(run.citation_sync_run_id.as_deref().unwrap())
        .await
        .unwrap();
    assert_eq!(sync.research_case_id, case.id);
    assert_eq!(sync.manuscript_source_id, manuscript.id);
    assert_eq!(sync.document_id, "doc-7");
    assert_eq!(sync.document_version, 7);
    let catalog = research
        .get_manuscript_reference_catalog(run.reference_catalog_run_id.as_deref().unwrap())
        .await
        .unwrap();
    assert_eq!(catalog.research_case_id, case.id);
    assert_eq!(catalog.manuscript_source_id, manuscript.id);
    assert_eq!(catalog.citation_sync_run_id, sync.id);
    assert_eq!(catalog.document_id, "doc-7");
    assert_eq!(catalog.document_version, 7);
    let resolution = research
        .get_manuscript_reference_resolution(run.reference_resolution_run_id.as_deref().unwrap())
        .await
        .unwrap();
    assert_eq!(resolution.research_case_id, case.id);
    assert_eq!(resolution.catalog_run_id, catalog.id);
    let extraction = research
        .get_manuscript_claim_extraction(run.claim_extraction_run_id.as_deref().unwrap())
        .await
        .unwrap();
    assert_eq!(extraction.research_case_id, case.id);
    assert_eq!(extraction.manuscript_source_id, manuscript.id);
    assert_eq!(extraction.citation_sync_run_id, sync.id);
    assert_eq!(extraction.document_id, "doc-7");
    assert_eq!(extraction.document_version, 7);

    let items = service
        .citation_review_items(&run.review_run_id)
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].ordinal, 0);
    assert_eq!(items[1].ordinal, 1);
    assert_eq!(
        items[0].citation_occurrence_id,
        items[1].citation_occurrence_id
    );
    assert_eq!(items[0].cited_locator.as_deref(), Some("p1"));
    assert_eq!(items[1].cited_locator.as_deref(), Some("p2"));
    assert_eq!(
        items[0].status,
        nineprofs_research_verification::CitationReviewItemStatus::UnresolvedReference
    );

    let mut inconsistent = input.clone();
    inconsistent.citations[0].rendered_text = "[different live citation]".to_owned();
    let failed = service.start(inconsistent).await.unwrap();
    assert_eq!(
        failed.status,
        nineprofs_research_verification::CitationReviewRunStatus::Failed
    );
    assert_eq!(failed.failure_stage.as_deref(), Some("citation_sync"));

    let mut next_input = grouped_input(manuscript.id.as_str(), "doc-8", 8, "missing", Some("p2"));
    next_input.research_case_id = case.id.to_string();
    let next_run = service.start(next_input).await.unwrap();
    assert_eq!(next_run.document_id, "doc-8");
    assert_eq!(next_run.document_version, 8);
    assert_ne!(run.citation_sync_run_id, next_run.citation_sync_run_id);
    assert_ne!(
        run.reference_catalog_run_id,
        next_run.reference_catalog_run_id
    );
    assert_ne!(
        run.reference_resolution_run_id,
        next_run.reference_resolution_run_id
    );
    assert_ne!(
        run.claim_extraction_run_id,
        next_run.claim_extraction_run_id
    );
}

#[tokio::test]
async fn exact_ready_pdf_verification_exposes_canonical_evidence_and_partial_failure() {
    let (database, research, case, manuscript, reference) =
        base(Some(("Reference".to_owned(), Some(identity("item-exact"))))).await;
    let reference = reference.unwrap();
    let extraction = ready_pdf(&database, &research, case.id.as_str(), &reference).await;
    let extraction_id = extraction.id.to_string();
    let mut input = grouped_input(manuscript.id.as_str(), "doc-7", 7, "exact", Some("p2"));
    input.research_case_id = case.id.to_string();
    let candidate = CitationRetrievalCandidate {
        retrieval_chunk_id: "chunk-1".to_owned(),
        research_source_id: reference.id.to_string(),
        source_snapshot_id: extraction.source_snapshot_id.to_string(),
        extraction_id: extraction_id.clone(),
        page: 1,
        start: 0,
        end: "canonical evidence".chars().count() as u64,
        verbatim_excerpt: "canonical evidence".to_owned(),
        retrieval_score: 1.0,
        provider: "fixture-retrieval".to_owned(),
        rank: 1,
    };
    let (service, _verification) = review_service(
        &database,
        research,
        FixtureRetrieval {
            candidate: Some(candidate),
        },
        Some(Arc::new(SupportsAssessor {
            fail_locator: Some("p2".to_owned()),
        })),
    )
    .await;

    let run = service.start(input).await.unwrap();
    assert_eq!(
        run.status,
        nineprofs_research_verification::CitationReviewRunStatus::Completed
    );
    let items = service
        .citation_review_items(&run.review_run_id)
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].status,
        nineprofs_research_verification::CitationReviewItemStatus::VerificationCompleted
    );
    assert!(
        items[0]
            .verification
            .as_ref()
            .is_some_and(|v| v.verification_run_id.len() > 0)
    );
    assert_eq!(
        items[0]
            .verification
            .as_ref()
            .and_then(|v| v.relation.clone()),
        Some(nineprofs_research::ClaimEvidenceRelation::Supports)
    );
    assert_eq!(
        items[0].evidence[0].relation,
        nineprofs_research::ClaimEvidenceRelation::Supports
    );
    assert_eq!(items[0].evidence[0].verbatim_excerpt, "canonical evidence");
    assert_eq!(
        items[0].evidence[0].source_snapshot_id,
        extraction.source_snapshot_id.to_string()
    );
    assert_eq!(
        items[0].evidence[0].extraction_id.as_deref(),
        Some(extraction_id.as_str())
    );
    assert_eq!(
        items[1].status,
        nineprofs_research_verification::CitationReviewItemStatus::VerificationFailed
    );
    assert_eq!(items[1].failure_code.as_deref(), Some("assessor_failed"));
    for item in &items {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM research_citation_verification_runs WHERE claim_citation_link_id = ? AND citation_target_id = ?",
        )
        .bind(&item.claim_citation_link_id)
        .bind(&item.citation_target_id)
        .fetch_one(database.pool())
        .await
        .unwrap();
        assert_eq!(
            count, 1,
            "one review item must start at most one verification"
        );
    }
}

#[tokio::test]
async fn confirmed_candidate_advances_the_next_review_without_rewriting_history() {
    let (database, research, case, manuscript, _reference) =
        base(Some(("item-exact".to_owned(), None))).await;
    let mut input = grouped_input(
        manuscript.id.as_str(),
        "doc-confirm",
        1,
        "item-exact",
        Some("p2"),
    );
    input.research_case_id = case.id.to_string();
    let (service, _) = review_service(
        &database,
        research.clone(),
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;

    let first = service.start(input.clone()).await.unwrap();
    let first_items = service
        .citation_review_items(&first.review_run_id)
        .await
        .unwrap();
    assert_eq!(
        first_items[0].status,
        nineprofs_research_verification::CitationReviewItemStatus::ReferenceRequiresConfirmation
    );
    let resolution_entry_id = first_items[0].resolution_entry_id.clone().unwrap();
    let candidate_id = first_items[0].candidates[0].candidate_id.clone();
    let resolution_run_id = first.reference_resolution_run_id.clone().unwrap();
    let historical = research
        .list_manuscript_reference_resolution_entries(&resolution_run_id)
        .await
        .unwrap()
        .into_iter()
        .find(|entry| entry.id.to_string() == resolution_entry_id)
        .unwrap();
    let bindings = research
        .confirm_manuscript_reference_candidate(
            &resolution_run_id,
            &resolution_entry_id,
            &candidate_id,
        )
        .await
        .unwrap();
    assert_eq!(bindings.len(), 2);
    assert!(
        bindings
            .iter()
            .all(|binding| { binding.method == nineprofs_research::CitationBindingMethod::Human })
    );
    let unchanged = research
        .get_manuscript_reference_resolution(&resolution_run_id)
        .await
        .unwrap();
    let unchanged_entry = research
        .list_manuscript_reference_resolution_entries(unchanged.id.as_str())
        .await
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == historical.id)
        .unwrap();
    assert_eq!(unchanged_entry.outcome, historical.outcome);

    let second = service.start(input).await.unwrap();
    assert_ne!(first.review_run_id, second.review_run_id);
    let second_items = service
        .citation_review_items(&second.review_run_id)
        .await
        .unwrap();
    assert!(second_items.iter().all(|item| {
        item.resolution_outcome
            == Some(nineprofs_research::ManuscriptReferenceResolutionOutcome::CandidateRequiresConfirmation)
            && item.binding_method == Some(nineprofs_research::CitationBindingMethod::Human)
            && item.status
                == nineprofs_research_verification::CitationReviewItemStatus::SourceMatchedNotVerificationReady
    }));
}

#[tokio::test]
async fn unresolved_ambiguous_and_not_ready_items_are_not_verified() {
    let (database, research, case, manuscript, _) = base(None).await;
    for label in ["ambiguous", "ambiguous"] {
        research
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::ReferencePdf,
                label: label.to_owned(),
                identity: None,
            })
            .await
            .unwrap();
    }
    let mut ambiguous = grouped_input(
        manuscript.id.as_str(),
        "doc-ambiguous",
        1,
        "ambiguous",
        Some("p2"),
    );
    ambiguous.research_case_id = case.id.to_string();
    let (service, _) = review_service(
        &database,
        research.clone(),
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    let ambiguous_run = service.start(ambiguous).await.unwrap();
    let ambiguous_items = service
        .citation_review_items(&ambiguous_run.review_run_id)
        .await
        .unwrap();
    assert!(ambiguous_items.iter().all(|item| {
        item.status == nineprofs_research_verification::CitationReviewItemStatus::AmbiguousReference
            && item.verification.is_none()
    }));

    let (database, research, case, manuscript, reference) =
        base(Some(("Reference".to_owned(), Some(identity("item-exact"))))).await;
    let reference = reference.unwrap();
    failed_pdf(&research, &reference).await;
    let mut not_ready = grouped_input(
        manuscript.id.as_str(),
        "doc-not-ready",
        1,
        "not-ready",
        Some("p2"),
    );
    not_ready.research_case_id = case.id.to_string();
    let (service, _) = review_service(
        &database,
        research,
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    let not_ready_run = service.start(not_ready).await.unwrap();
    let not_ready_items = service
        .citation_review_items(&not_ready_run.review_run_id)
        .await
        .unwrap();
    assert!(not_ready_items.iter().all(|item| {
        item.status == nineprofs_research_verification::CitationReviewItemStatus::SourceMatchedNotVerificationReady
            && item.verification.is_none()
    }));
}

#[tokio::test]
async fn cross_case_source_is_rejected_and_conflicting_binding_fails_closed() {
    let (database, research, case, manuscript, _reference) =
        base(Some(("Reference".to_owned(), Some(identity("item-exact"))))).await;
    let other_case = research
        .create_case(CreateResearchCase {
            title: "Other case".to_owned(),
        })
        .await
        .unwrap();
    let other_source = research
        .create_source(CreateResearchSource {
            research_case_id: other_case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Other draft".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let _other_reference = research
        .create_source(CreateResearchSource {
            research_case_id: other_case.id.clone(),
            kind: SourceKind::ReferencePdf,
            label: "Other reference".to_owned(),
            identity: Some(identity("item-other")),
        })
        .await
        .unwrap();
    let conflict_source = research
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::ReferencePdf,
            label: "Conflicting source".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let mut cross_case = grouped_input(
        other_source.id.as_str(),
        "doc-cross",
        1,
        "exact",
        Some("p2"),
    );
    cross_case.research_case_id = case.id.to_string();
    let (service, _) = review_service(
        &database,
        research.clone(),
        FixtureRetrieval { candidate: None },
        None,
    )
    .await;
    assert!(service.start(cross_case).await.is_err());

    let mut cross_case_reference = grouped_input(
        manuscript.id.as_str(),
        "doc-cross-reference",
        1,
        "item-other",
        Some("p2"),
    );
    cross_case_reference.research_case_id = case.id.to_string();
    for target in &mut cross_case_reference.citations[0].targets {
        target.zotero.as_mut().unwrap().item_id = Some("item-other".to_owned());
    }
    let cross_case_reference_run = service.start(cross_case_reference).await.unwrap();
    let cross_case_reference_items = service
        .citation_review_items(&cross_case_reference_run.review_run_id)
        .await
        .unwrap();
    assert!(cross_case_reference_items.iter().all(|item| {
        item.status
            == nineprofs_research_verification::CitationReviewItemStatus::UnresolvedReference
            && item.source_id.is_none()
            && item.verification.is_none()
    }));

    let sync = research
        .sync_manuscript_citations(SyncManuscriptCitations {
            research_case_id: case.id.clone(),
            manuscript_source_id: manuscript.id.clone(),
            document_id: "doc-conflict".to_owned(),
            document_version: 1,
            citations: vec![ManuscriptCitationSyncCitationInput {
                format: ManuscriptCitationFormat::Zotero,
                rendered_text: "[1]".to_owned(),
                block_id: "block-1".to_owned(),
                start: 6,
                end: 9,
                targets: vec![
                    ManuscriptCitationSyncTargetInput {
                        ordinal: 0,
                        reference_key: "exact".to_owned(),
                        cited_locator: Some("p1".to_owned()),
                    },
                    ManuscriptCitationSyncTargetInput {
                        ordinal: 1,
                        reference_key: "exact".to_owned(),
                        cited_locator: Some("p2".to_owned()),
                    },
                ],
            }],
        })
        .await
        .unwrap();
    let target = research
        .list_manuscript_citation_sync_occurrences(sync.id.as_str())
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let target_id = research
        .list_citation_targets(target.citation_occurrence_id.as_str())
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .id;
    research
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: case.id.clone(),
            citation_target_id: target_id,
            source_id: conflict_source.id,
            source_snapshot_id: None,
            extraction_id: None,
            method: CitationBindingMethod::Human,
        })
        .await
        .unwrap();
    let mut conflict_input = grouped_input(
        manuscript.id.as_str(),
        "doc-conflict",
        1,
        "exact",
        Some("p2"),
    );
    conflict_input.research_case_id = case.id.to_string();
    let conflict_run = service.start(conflict_input).await.unwrap();
    let conflict_items = service
        .citation_review_items(&conflict_run.review_run_id)
        .await
        .unwrap();
    assert!(conflict_items.iter().all(|item| {
        item.status == nineprofs_research_verification::CitationReviewItemStatus::BindingConflict
            && item.verification.is_none()
    }));
}
