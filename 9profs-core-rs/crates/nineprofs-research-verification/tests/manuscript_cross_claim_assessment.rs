use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use async_trait::async_trait;
use nineprofs_db::Database;
use nineprofs_research::{
    CreateResearchCase, CreateResearchSource, ResearchService, SourceKind, SqliteResearchRepository,
};
use nineprofs_research_verification::{
    CitationRetrievalCandidate, CitationRetrievalError, CitationRetrievalProvider,
    CitationReviewService, CitationVerificationService, CrossClaimAssessmentStatus,
    CrossClaimConsistencyAssessment, CrossClaimConsistencyAssessmentInput,
    CrossClaimConsistencyAssessmentProvider, CrossClaimConsistencyAssessmentProviderError,
    CrossClaimConsistencyAssessmentProviderIdentity, CrossClaimConsistencyAttentionReason,
    CrossClaimConsistencyAttentionState, CrossClaimConsistencyRelation,
    CrossClaimDifferenceDimension, ManuscriptCrossClaimAssessmentRunStatus,
    StartManuscriptCrossClaimAssessment,
};

struct FixtureProvider {
    fail: AtomicBool,
    calls: AtomicUsize,
    inputs: Mutex<Vec<CrossClaimConsistencyAssessmentInput>>,
}

impl FixtureProvider {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            fail: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            inputs: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl CrossClaimConsistencyAssessmentProvider for FixtureProvider {
    fn identity(&self) -> CrossClaimConsistencyAssessmentProviderIdentity {
        CrossClaimConsistencyAssessmentProviderIdentity {
            provider_id: "assessment-fixture".to_owned(),
            assessor_implementation_version: "assessment-fixture-v1".to_owned(),
            model_id: Some("fixture-model".to_owned()),
        }
    }

    async fn assess(
        &self,
        input: CrossClaimConsistencyAssessmentInput,
    ) -> Result<CrossClaimConsistencyAssessment, CrossClaimConsistencyAssessmentProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inputs.lock().expect("input lock").push(input.clone());
        if self.fail.load(Ordering::SeqCst) {
            return Err(CrossClaimConsistencyAssessmentProviderError::ProviderUnavailable);
        }
        let conflict = input.candidate_id == "candidate-1";
        Ok(CrossClaimConsistencyAssessment {
            candidate_id: input.candidate_id,
            relation: if conflict {
                CrossClaimConsistencyRelation::Conflict
            } else {
                CrossClaimConsistencyRelation::Compatible
            },
            dimensions: if conflict {
                vec![CrossClaimDifferenceDimension::Direction]
            } else {
                vec![CrossClaimDifferenceDimension::Quantitative]
            },
            rationale: "fixture assessed only the supplied claim wording".to_owned(),
        })
    }
}

struct EmptyRetrieval;

#[async_trait]
impl CitationRetrievalProvider for EmptyRetrieval {
    async fn retrieve_exact_extraction(
        &self,
        _research_case_id: &str,
        _extraction_id: &str,
        _query: &str,
        _top_k: u32,
    ) -> Result<Vec<CitationRetrievalCandidate>, CitationRetrievalError> {
        Ok(Vec::new())
    }
}

async fn fixture() -> (
    Database,
    CitationReviewService,
    String,
    Arc<FixtureProvider>,
) {
    let database = Database::in_memory().await.expect("in-memory database");
    let events = Arc::new(nineprofs_realtime::BroadcastEventBus::new(16));
    let research = Arc::new(ResearchService::new(
        SqliteResearchRepository::new(database.pool().clone()),
        events.clone(),
    ));
    let case = research
        .create_case(CreateResearchCase {
            title: "Cross-claim assessment fixture".to_owned(),
        })
        .await
        .expect("fixture case");
    let source = research
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Fixture manuscript".to_owned(),
            identity: None,
        })
        .await
        .expect("fixture source");
    seed_history(&database, case.id.as_str(), source.id.as_str()).await;
    let verification = Arc::new(CitationVerificationService::new(
        database.pool().clone(),
        research.clone(),
        Arc::new(EmptyRetrieval),
        events.clone(),
    ));
    let provider = FixtureProvider::new();
    let service =
        CitationReviewService::new(database.pool().clone(), research, verification, events)
            .with_cross_claim_consistency_assessor(provider.clone());
    (database, service, case.id.to_string(), provider)
}

async fn seed_history(database: &Database, case_id: &str, source_id: &str) {
    sqlx::query(
        "INSERT INTO research_manuscript_claim_inventory_runs
         (id, research_case_id, manuscript_source_id, document_id, document_version,
          document_context_hash_algorithm, document_context_hash, extractor_provider,
          extractor_version, extractor_model_id, extraction_contract_version,
          coverage_contract_version, coverage_scope, coverage_limitations_json, status,
          item_count, covered_block_count, created_at_ms, completed_at_ms, failure_code)
          VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'completed', 3, 3, 1, 2, NULL)",
    )
    .bind("inventory-run")
    .bind(case_id)
    .bind(source_id)
    .bind("fixture-document")
    .bind(3_i64)
    .bind("sha256")
    .bind("fixture-context")
    .bind("fixture-extractor")
    .bind("v1")
    .bind(None::<String>)
    .bind("inventory-v1")
    .bind("coverage-v1")
    .bind("visible_blocks")
    .bind("[]")
    .execute(database.pool())
    .await
    .expect("inventory run");

    for ordinal in 0..3_i64 {
        sqlx::query(
            "INSERT INTO research_manuscript_claim_inventory_items
             (id, inventory_run_id, ordinal, document_block_id, block_ordinal, block_kind,
              source_start, source_end, source_excerpt, source_excerpt_hash_algorithm,
              source_excerpt_hash, claim_text, review_kind, overlapping_citation_count)
             VALUES (?, 'inventory-run', ?, ?, ?, 'paragraph', ?, ?, ?, 'sha256', ?, ?, 'manuscript_internal', 0)",
        )
        .bind(format!("inventory-item-{ordinal}"))
        .bind(ordinal)
        .bind(format!("block-{ordinal}"))
        .bind(ordinal)
        .bind(ordinal * 10)
        .bind(ordinal * 10 + 9)
        .bind(format!("manuscript excerpt {ordinal}"))
        .bind("0".repeat(64))
        .bind(format!("claim wording {ordinal}"))
        .execute(database.pool())
        .await
        .expect("inventory item");
    }

    sqlx::query(
        "INSERT INTO research_manuscript_cross_claim_candidate_runs
         (candidate_run_id, research_case_id, manuscript_source_id, document_id, document_version,
          claim_inventory_run_id, provider_id, model_id, discovery_implementation_version,
          discovery_contract_version, claim_count, batch_count, expected_window_count,
          processed_window_count, candidate_pair_count, status, failure_code, created_at_ms,
          completed_at_ms)
         VALUES ('candidate-run', ?, ?, 'fixture-document', 3, 'inventory-run', 'discovery-fixture',
                 'discovery-model', 'discovery-v1', 'discovery-contract-v1', 3, 1, 1, 1, 2,
                 'completed', NULL, 3, 4)",
    )
    .bind(case_id)
    .bind(source_id)
    .execute(database.pool())
    .await
    .expect("candidate run");
    sqlx::query(
        "INSERT INTO research_manuscript_cross_claim_comparison_windows
         (window_id, candidate_run_id, left_batch_ordinal, right_batch_ordinal, same_batch,
          status, candidate_count, failure_code)
         VALUES ('window-0', 'candidate-run', 0, 0, 1, 'processed', 2, NULL)",
    )
    .execute(database.pool())
    .await
    .expect("candidate window");
    for (candidate_id, left_id, right_id, left_ordinal, right_ordinal) in [
        (
            "candidate-1",
            "inventory-item-0",
            "inventory-item-1",
            0_i64,
            1_i64,
        ),
        (
            "candidate-2",
            "inventory-item-1",
            "inventory-item-2",
            1_i64,
            2_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO research_manuscript_cross_claim_candidates
             (candidate_id, candidate_run_id, comparison_window_id, left_inventory_item_id,
              right_inventory_item_id, left_ordinal, right_ordinal, candidate_kinds_json, rationale)
             VALUES (?, 'candidate-run', 'window-0', ?, ?, ?, ?,
                     '[\"potential_direct_conflict\"]', 'discovery rationale')",
        )
        .bind(candidate_id)
        .bind(left_id)
        .bind(right_id)
        .bind(left_ordinal)
        .bind(right_ordinal)
        .execute(database.pool())
        .await
        .expect("candidate");
    }
}

#[tokio::test]
async fn assessment_is_blind_deterministic_and_idempotent() {
    let (database, service, case_id, provider) = fixture().await;
    let run = service
        .start_manuscript_cross_claim_assessment(StartManuscriptCrossClaimAssessment {
            research_case_id: case_id.clone(),
            candidate_run_id: "candidate-run".to_owned(),
        })
        .await
        .expect("assessment run");
    assert!(matches!(
        run.status,
        ManuscriptCrossClaimAssessmentRunStatus::Completed
    ));
    assert_eq!(run.candidate_count, 2);
    assert_eq!(run.assessed_count, 2);
    assert_eq!(run.failed_item_count, 0);
    assert_eq!(run.conflict_count, 1);
    assert_eq!(run.compatible_count, 1);

    let items = service
        .list_manuscript_cross_claim_assessment_items(&run.assessment_run_id)
        .await
        .expect("assessment items");
    assert_eq!(items.len(), 2);
    let conflict = items
        .iter()
        .find(|item| item.candidate_id == "candidate-1")
        .unwrap();
    assert_eq!(
        conflict.assessment_status,
        CrossClaimAssessmentStatus::Assessed
    );
    assert_eq!(
        conflict.attention,
        CrossClaimConsistencyAttentionState::ReviewSuggested
    );
    assert!(
        conflict
            .attention_reasons
            .contains(&CrossClaimConsistencyAttentionReason::DirectionConflictObserved)
    );
    let compatible = items
        .iter()
        .find(|item| item.candidate_id == "candidate-2")
        .unwrap();
    assert_eq!(
        compatible.attention,
        CrossClaimConsistencyAttentionState::NoInternalConsistencyAttentionDetected
    );
    assert!(compatible.attention_reasons.is_empty());

    let inputs = provider.inputs.lock().expect("input lock");
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0].candidate_id, "candidate-1");
    assert_eq!(inputs[0].left.claim_text, "claim wording 0");
    assert_eq!(inputs[0].right.claim_text, "claim wording 1");
    let serialized = serde_json::to_value(&inputs[0]).expect("semantic input JSON");
    assert!(serialized.get("candidateKinds").is_none());
    assert!(serialized.get("rationale").is_none());
    assert!(serialized.get("citation").is_none());
    assert!(serialized.get("evidence").is_none());
    drop(inputs);
    let calls = provider.calls.load(Ordering::SeqCst);
    let reused = service
        .start_manuscript_cross_claim_assessment(StartManuscriptCrossClaimAssessment {
            research_case_id: case_id,
            candidate_run_id: "candidate-run".to_owned(),
        })
        .await
        .expect("same successful identity should be reused");
    assert_eq!(reused.assessment_run_id, run.assessment_run_id);
    assert_eq!(provider.calls.load(Ordering::SeqCst), calls);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM research_manuscript_cross_claim_assessment_runs",
        )
        .fetch_one(database.pool())
        .await
        .unwrap(),
        1
    );
}

#[tokio::test]
async fn item_failures_are_persisted_and_retryable() {
    let (database, service, case_id, provider) = fixture().await;
    provider.fail.store(true, Ordering::SeqCst);
    let failed = service
        .start_manuscript_cross_claim_assessment(StartManuscriptCrossClaimAssessment {
            research_case_id: case_id.clone(),
            candidate_run_id: "candidate-run".to_owned(),
        })
        .await
        .expect("item failures complete the run");
    assert_eq!(failed.failed_item_count, 2);
    assert_eq!(failed.failed_assessment_count, 2);
    let failed_items = service
        .list_manuscript_cross_claim_assessment_items(&failed.assessment_run_id)
        .await
        .expect("failed assessment items");
    assert!(failed_items.iter().all(|item| {
        matches!(
            item.assessment_status,
            CrossClaimAssessmentStatus::AssessmentFailed
        ) && matches!(
            item.attention,
            CrossClaimConsistencyAttentionState::AssessmentUnavailable
        ) && item.failure_code.as_deref() == Some("provider_unavailable")
    }));

    provider.fail.store(false, Ordering::SeqCst);
    let retried = service
        .start_manuscript_cross_claim_assessment(StartManuscriptCrossClaimAssessment {
            research_case_id: case_id,
            candidate_run_id: "candidate-run".to_owned(),
        })
        .await
        .expect("failed item run is retryable");
    assert_ne!(retried.assessment_run_id, failed.assessment_run_id);
    assert_eq!(retried.failed_item_count, 0);
    assert_eq!(retried.assessed_count, 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM research_manuscript_cross_claim_assessment_runs",
        )
        .fetch_one(database.pool())
        .await
        .unwrap(),
        2
    );
}
