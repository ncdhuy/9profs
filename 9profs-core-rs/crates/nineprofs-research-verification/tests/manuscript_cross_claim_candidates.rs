use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use nineprofs_db::Database;
use nineprofs_research::{
    ClaimReviewKind, ContentHash, CreateResearchCase, CreateResearchSource, HashAlgorithm,
    ManuscriptClaimInventoryBlockKind, ManuscriptClaimInventoryItem,
    ManuscriptClaimInventoryItemId, ManuscriptClaimInventoryRunId, ResearchService, SourceKind,
    SqliteResearchRepository,
};
use nineprofs_research_verification::{
    CitationRetrievalCandidate, CitationRetrievalError, CitationRetrievalProvider,
    CitationReviewService, CitationVerificationService, CrossClaimCandidateDiscoveryInput,
    CrossClaimCandidateDiscoveryOutput, CrossClaimCandidateDiscoveryProvider,
    CrossClaimCandidateDiscoveryProviderError, CrossClaimCandidateDiscoveryProviderIdentity,
    CrossClaimCandidateOutput, ManuscriptCrossClaimCandidateKind,
    ManuscriptCrossClaimCandidateRunStatus, StartManuscriptCrossClaimCandidates,
    build_cross_claim_batches, build_cross_claim_comparison_windows, eligible_cross_claim_pairs,
};

#[derive(Clone, Copy)]
enum ProviderMode {
    Empty,
    Distant,
    Unknown,
    SelfPair,
    Fail,
}

struct FixtureProvider {
    mode: Mutex<ProviderMode>,
    identity: Mutex<(String, String)>,
    calls: AtomicUsize,
}

impl FixtureProvider {
    fn new(mode: ProviderMode) -> Arc<Self> {
        Arc::new(Self {
            mode: Mutex::new(mode),
            identity: Mutex::new((
                "fixture-model".to_owned(),
                "cross-claim-fixture-v1".to_owned(),
            )),
            calls: AtomicUsize::new(0),
        })
    }

    fn set_mode(&self, mode: ProviderMode) {
        *self.mode.lock().expect("fixture mode lock") = mode;
    }

    fn set_identity(&self, model_id: &str, implementation_version: &str) {
        *self.identity.lock().expect("fixture identity lock") =
            (model_id.to_owned(), implementation_version.to_owned());
    }
}

#[async_trait]
impl CrossClaimCandidateDiscoveryProvider for FixtureProvider {
    fn identity(&self) -> CrossClaimCandidateDiscoveryProviderIdentity {
        let (model_id, implementation_version) =
            self.identity.lock().expect("fixture identity lock").clone();
        CrossClaimCandidateDiscoveryProviderIdentity {
            provider_id: "cross-claim-fixture".to_owned(),
            implementation_version,
            model_id: Some(model_id),
        }
    }

    async fn discover(
        &self,
        input: CrossClaimCandidateDiscoveryInput,
    ) -> Result<CrossClaimCandidateDiscoveryOutput, CrossClaimCandidateDiscoveryProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mode = *self.mode.lock().expect("fixture mode lock");
        let first_left = input
            .left_batch
            .first()
            .expect("fixture window has a left claim");
        let candidate = |left: String, right: String, kind| CrossClaimCandidateOutput {
            left_inventory_item_id: left,
            right_inventory_item_id: right,
            candidate_kind: kind,
            rationale: "fixture candidate needs later consistency review".to_owned(),
        };
        let candidates = match mode {
            ProviderMode::Empty => Vec::new(),
            ProviderMode::Distant => {
                let has_first = input
                    .left_batch
                    .iter()
                    .chain(&input.right_batch)
                    .any(|claim| claim.inventory_item_id == "inventory-item-000");
                let has_last = input
                    .left_batch
                    .iter()
                    .chain(&input.right_batch)
                    .any(|claim| claim.inventory_item_id == "inventory-item-034");
                if has_first && has_last {
                    vec![
                        candidate(
                            "inventory-item-034".to_owned(),
                            "inventory-item-000".to_owned(),
                            ManuscriptCrossClaimCandidateKind::PotentialScopeMismatch,
                        ),
                        candidate(
                            "inventory-item-034".to_owned(),
                            "inventory-item-000".to_owned(),
                            ManuscriptCrossClaimCandidateKind::PotentialDirectConflict,
                        ),
                    ]
                } else {
                    Vec::new()
                }
            }
            ProviderMode::Unknown => vec![candidate(
                first_left.inventory_item_id.clone(),
                "outside-history".to_owned(),
                ManuscriptCrossClaimCandidateKind::OtherConsistencyCandidate,
            )],
            ProviderMode::SelfPair => vec![candidate(
                first_left.inventory_item_id.clone(),
                first_left.inventory_item_id.clone(),
                ManuscriptCrossClaimCandidateKind::PotentialDuplicateOrRestatement,
            )],
            ProviderMode::Fail => {
                return Err(CrossClaimCandidateDiscoveryProviderError::ProviderUnavailable);
            }
        };
        Ok(CrossClaimCandidateDiscoveryOutput {
            comparison_window_id: input.comparison_window_id,
            candidates,
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

async fn fixture(
    claim_count: usize,
    mode: ProviderMode,
) -> (
    Database,
    CitationReviewService,
    String,
    String,
    Arc<FixtureProvider>,
) {
    let database = Database::in_memory().await.expect("in-memory database");
    let events = Arc::new(nineprofs_realtime::BroadcastEventBus::new(32));
    let research = Arc::new(ResearchService::new(
        SqliteResearchRepository::new(database.pool().clone()),
        events.clone(),
    ));
    let case = research
        .create_case(CreateResearchCase {
            title: "Cross-claim candidate fixture".to_owned(),
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
    let inventory_run_id = "inventory-fixture-run".to_owned();
    seed_inventory(
        &database,
        case.id.as_str(),
        source.id.as_str(),
        &inventory_run_id,
        claim_count,
    )
    .await;
    let verification = Arc::new(CitationVerificationService::new(
        database.pool().clone(),
        research.clone(),
        Arc::new(EmptyRetrieval),
        events.clone(),
    ));
    let provider = FixtureProvider::new(mode);
    let service =
        CitationReviewService::new(database.pool().clone(), research, verification, events)
            .with_cross_claim_candidate_provider(provider.clone());
    (
        database,
        service,
        case.id.to_string(),
        inventory_run_id,
        provider,
    )
}

async fn seed_inventory(
    database: &Database,
    case_id: &str,
    source_id: &str,
    run_id: &str,
    claim_count: usize,
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
    .bind(source_id)
    .bind("fixture-document")
    .bind(7_i64)
    .bind("sha256")
    .bind("fixture-context")
    .bind("cross-claim-fixture")
    .bind("1")
    .bind(None::<String>)
    .bind("inventory-v1")
    .bind("inventory-coverage-v1")
    .bind("visible_blocks")
    .bind(r#"["tables","textboxes"]"#)
    .bind("completed")
    .bind(claim_count as i64)
    .bind(claim_count as i64)
    .bind(1_i64)
    .bind(2_i64)
    .bind(None::<String>)
    .execute(database.pool())
    .await
    .expect("fixture inventory run");

    for ordinal in 0..claim_count {
        let item_id = format!("inventory-item-{ordinal:03}");
        let excerpt_hash = "0".repeat(64);
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
        .bind(format!("fixture-block-{ordinal:03}"))
        .bind(ordinal as i64)
        .bind("paragraph")
        .bind((ordinal * 10) as i64)
        .bind((ordinal * 10 + 9) as i64)
        .bind(format!("excerpt {ordinal}"))
        .bind("sha256")
        .bind(excerpt_hash)
        .bind(format!("claim {ordinal}"))
        .bind("manuscript_internal")
        .bind(0_i64)
        .execute(database.pool())
        .await
        .expect("fixture inventory item");
    }
}

async fn count(database: &Database, table: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(database.pool())
        .await
        .expect("count fixture rows")
}

async fn provenance_counts(database: &Database) -> Vec<i64> {
    let mut counts = Vec::new();
    for table in [
        "research_claims",
        "research_claim_citations",
        "research_citation_target_bindings",
        "research_citation_verification_runs",
        "research_evidence",
        "research_claim_evidence",
        "research_manuscript_citation_expectation_runs",
        "research_manuscript_claim_coverage_runs",
    ] {
        counts.push(count(database, table).await);
    }
    counts
}

#[tokio::test]
async fn distant_candidates_are_canonicalized_deduplicated_and_idempotent() {
    let (database, service, case_id, inventory_run_id, provider) =
        fixture(35, ProviderMode::Distant).await;
    let before_provenance = provenance_counts(&database).await;

    let run = service
        .start_manuscript_cross_claim_candidates(StartManuscriptCrossClaimCandidates {
            research_case_id: case_id.clone(),
            claim_inventory_run_id: inventory_run_id.clone(),
        })
        .await
        .expect("distant candidate should be accepted");
    assert_eq!(
        run.status,
        ManuscriptCrossClaimCandidateRunStatus::Completed
    );
    assert_eq!(run.batch_count, 3);
    assert_eq!(run.expected_window_count, 6);
    assert_eq!(run.processed_window_count, run.expected_window_count);
    assert_eq!(run.candidate_pair_count, 1);

    let candidates = service
        .list_manuscript_cross_claim_candidates(&run.candidate_run_id)
        .await
        .expect("completed candidates");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].left_inventory_item_id, "inventory-item-000");
    assert_eq!(candidates[0].right_inventory_item_id, "inventory-item-034");
    assert_eq!(candidates[0].left_ordinal, 0);
    assert_eq!(candidates[0].right_ordinal, 34);
    assert_eq!(candidates[0].candidate_kinds.len(), 2);
    assert_eq!(
        candidates[0].candidate_kinds[0],
        ManuscriptCrossClaimCandidateKind::PotentialDirectConflict
    );
    assert_eq!(
        candidates[0].candidate_kinds[1],
        ManuscriptCrossClaimCandidateKind::PotentialScopeMismatch
    );

    let windows = service
        .list_manuscript_cross_claim_candidate_windows(&run.candidate_run_id)
        .await
        .expect("window audit");
    assert_eq!(windows.len(), 6);
    assert!(windows.iter().all(|window| matches!(
        window.status,
        nineprofs_research_verification::ManuscriptCrossClaimComparisonWindowStatus::Processed
    )));
    assert_eq!(
        windows
            .iter()
            .map(|window| window.candidate_count)
            .sum::<u32>(),
        1
    );

    let calls_after_first_run = provider.calls.load(Ordering::SeqCst);
    let reused = service
        .start_manuscript_cross_claim_candidates(StartManuscriptCrossClaimCandidates {
            research_case_id: case_id,
            claim_inventory_run_id: inventory_run_id,
        })
        .await
        .expect("same completed identity should be reused");
    assert_eq!(reused.candidate_run_id, run.candidate_run_id);
    assert_eq!(provider.calls.load(Ordering::SeqCst), calls_after_first_run);
    assert_eq!(provenance_counts(&database).await, before_provenance);
}

#[tokio::test]
async fn zero_candidate_windows_are_processed_and_failure_retry_keeps_history() {
    let (database, service, case_id, inventory_run_id, provider) =
        fixture(17, ProviderMode::Fail).await;
    let first_error = service
        .start_manuscript_cross_claim_candidates(StartManuscriptCrossClaimCandidates {
            research_case_id: case_id.clone(),
            claim_inventory_run_id: inventory_run_id.clone(),
        })
        .await
        .expect_err("provider failure should fail the run");
    assert!(matches!(
        first_error,
        nineprofs_research_verification::CrossClaimCandidateDiscoveryError::ProviderFailed(_)
    ));
    let failed_run_id = sqlx::query_scalar::<_, String>(
        "SELECT candidate_run_id FROM research_manuscript_cross_claim_candidate_runs
         WHERE status = 'failed' ORDER BY created_at_ms DESC LIMIT 1",
    )
    .fetch_one(database.pool())
    .await
    .expect("failed run history");
    assert!(
        service
            .list_manuscript_cross_claim_candidates(&failed_run_id)
            .await
            .is_err()
    );

    provider.set_mode(ProviderMode::Empty);
    let completed = service
        .start_manuscript_cross_claim_candidates(StartManuscriptCrossClaimCandidates {
            research_case_id: case_id,
            claim_inventory_run_id: inventory_run_id,
        })
        .await
        .expect("same identity should be retryable");
    assert_eq!(
        completed.status,
        ManuscriptCrossClaimCandidateRunStatus::Completed
    );
    assert_eq!(
        completed.processed_window_count,
        completed.expected_window_count
    );
    assert_eq!(completed.candidate_pair_count, 0);
    let windows = service
        .list_manuscript_cross_claim_candidate_windows(&completed.candidate_run_id)
        .await
        .expect("completed zero-candidate windows");
    assert!(windows.iter().all(|window| window.candidate_count == 0));
    assert_eq!(
        count(&database, "research_manuscript_cross_claim_candidate_runs").await,
        2
    );
}

#[tokio::test]
async fn unknown_and_self_pairs_fail_closed_without_completed_runs() {
    for mode in [ProviderMode::Unknown, ProviderMode::SelfPair] {
        let (database, service, case_id, inventory_run_id, _) = fixture(2, mode).await;
        let error = service
            .start_manuscript_cross_claim_candidates(StartManuscriptCrossClaimCandidates {
                research_case_id: case_id,
                claim_inventory_run_id: inventory_run_id,
            })
            .await
            .expect_err("invalid provider output should fail closed");
        assert!(matches!(
            error,
            nineprofs_research_verification::CrossClaimCandidateDiscoveryError::ClosedSetViolation(
                _
            )
        ));
        assert_eq!(
            count(&database, "research_manuscript_cross_claim_candidate_runs").await,
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM research_manuscript_cross_claim_candidate_runs LIMIT 1",
            )
            .fetch_one(database.pool())
            .await
            .expect("failed run status"),
            "failed"
        );
    }
}

#[tokio::test]
async fn provider_or_contract_identity_change_creates_distinct_completed_history() {
    let (database, service, case_id, inventory_run_id, provider) =
        fixture(2, ProviderMode::Empty).await;
    let first = service
        .start_manuscript_cross_claim_candidates(StartManuscriptCrossClaimCandidates {
            research_case_id: case_id.clone(),
            claim_inventory_run_id: inventory_run_id.clone(),
        })
        .await
        .expect("first identity");

    provider.set_identity("fixture-model-v2", "cross-claim-fixture-v2");
    let second = service
        .start_manuscript_cross_claim_candidates(StartManuscriptCrossClaimCandidates {
            research_case_id: case_id,
            claim_inventory_run_id: inventory_run_id,
        })
        .await
        .expect("changed identity");
    assert_ne!(first.candidate_run_id, second.candidate_run_id);
    assert_eq!(
        count(&database, "research_manuscript_cross_claim_candidate_runs").await,
        2
    );
}

#[tokio::test]
async fn concurrent_equivalent_discovery_converges_to_one_completed_run() {
    let (database, service, case_id, inventory_run_id, _) = fixture(2, ProviderMode::Empty).await;
    let input = || StartManuscriptCrossClaimCandidates {
        research_case_id: case_id.clone(),
        claim_inventory_run_id: inventory_run_id.clone(),
    };
    let other_service = service.clone();
    let (first, second) = tokio::join!(
        service.start_manuscript_cross_claim_candidates(input()),
        other_service.start_manuscript_cross_claim_candidates(input()),
    );
    let first = first.expect("first concurrent discovery");
    let second = second.expect("second concurrent discovery");
    assert_eq!(first.candidate_run_id, second.candidate_run_id);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM research_manuscript_cross_claim_candidate_runs
             WHERE status = 'completed'",
        )
        .fetch_one(database.pool())
        .await
        .expect("completed run count"),
        1
    );
}

fn item(ordinal: u32) -> ManuscriptClaimInventoryItem {
    ManuscriptClaimInventoryItem {
        id: ManuscriptClaimInventoryItemId::new(),
        inventory_run_id: ManuscriptClaimInventoryRunId::new(),
        ordinal,
        document_block_id: format!("block-{ordinal}"),
        block_ordinal: ordinal,
        block_kind: ManuscriptClaimInventoryBlockKind::Paragraph,
        source_start: 0,
        source_end: 10,
        source_excerpt: format!("excerpt {ordinal}"),
        source_excerpt_hash: ContentHash {
            algorithm: HashAlgorithm::Sha256,
            value: "0".repeat(64),
        },
        claim_text: format!("claim {ordinal}"),
        review_kind: ClaimReviewKind::ManuscriptInternal,
        overlapping_citation_count: 3,
    }
}

#[test]
fn scheduler_covers_each_inventory_pair_once_without_self_pairs() {
    let items = (0..35).map(item).collect::<Vec<_>>();
    let batches = build_cross_claim_batches(&items).expect("bounded inventory should schedule");
    let windows = build_cross_claim_comparison_windows(batches.len()).expect("windows should fit");
    let mut pairs = BTreeMap::<(String, String), usize>::new();

    for window in windows {
        let left = &batches[window.left_batch_ordinal as usize];
        let right = &batches[window.right_batch_ordinal as usize];
        for (left_id, right_id) in eligible_cross_claim_pairs(&window, left, right) {
            assert_ne!(left_id, right_id);
            *pairs.entry((left_id, right_id)).or_default() += 1;
        }
    }

    assert_eq!(pairs.len(), 35 * 34 / 2);
    assert!(pairs.values().all(|count| *count == 1));
}

#[test]
fn provider_projection_excludes_offsets_citations_and_attention() {
    let batches = build_cross_claim_batches(&[item(0), item(1)]).expect("claims should schedule");
    let value = serde_json::to_value(&batches[0].claims[0]).expect("claim should serialize");
    let object = value
        .as_object()
        .expect("claim projection should be an object");

    assert!(object.contains_key("inventoryItemId"));
    assert!(object.contains_key("claimText"));
    assert!(object.contains_key("sourceExcerpt"));
    assert!(object.contains_key("reviewKind"));
    assert!(object.contains_key("blockKind"));
    assert!(object.contains_key("blockOrdinal"));
    for prohibited in [
        "ordinal",
        "sourceStart",
        "sourceEnd",
        "sourceExcerptHash",
        "overlappingCitationCount",
        "citations",
        "evidence",
        "attention",
    ] {
        assert!(
            !object.contains_key(prohibited),
            "unexpected field: {prohibited}"
        );
    }
}
