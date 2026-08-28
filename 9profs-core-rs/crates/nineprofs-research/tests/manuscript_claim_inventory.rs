use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    ClaimReviewKind, CreateResearchCase, CreateResearchSource, ManuscriptClaimInventoryBlockInput,
    ManuscriptClaimInventoryBlockKind, ManuscriptClaimInventoryCitationInput,
    ManuscriptClaimInventoryClaimOutput, ManuscriptClaimInventoryCoverageStatus,
    ManuscriptClaimInventoryIdentity, ManuscriptClaimInventoryOutput,
    ManuscriptClaimInventoryProvider, ManuscriptClaimInventoryProviderError,
    ManuscriptClaimInventoryStatus, ResearchError, ResearchService, SourceKind,
    SqliteResearchRepository, StartManuscriptClaimInventory,
};

struct MockInventoryProvider {
    identity: ManuscriptClaimInventoryIdentity,
    outputs: HashMap<String, ManuscriptClaimInventoryOutput>,
    calls: Arc<AtomicUsize>,
    fail: bool,
}

#[async_trait]
impl ManuscriptClaimInventoryProvider for MockInventoryProvider {
    fn identity(&self) -> ManuscriptClaimInventoryIdentity {
        self.identity.clone()
    }

    async fn extract(
        &self,
        block: ManuscriptClaimInventoryBlockInput,
    ) -> Result<ManuscriptClaimInventoryOutput, ManuscriptClaimInventoryProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(ManuscriptClaimInventoryProviderError::Transport);
        }
        Ok(self
            .outputs
            .get(&block.block_id)
            .cloned()
            .unwrap_or(ManuscriptClaimInventoryOutput { claims: Vec::new() }))
    }
}

async fn fixture() -> (Database, ResearchService, String, String) {
    let database = Database::in_memory().await.unwrap();
    let service = ResearchService::new(
        SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(64)),
    );
    let case = service
        .create_case(CreateResearchCase {
            title: "Whole-manuscript inventory".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Draft".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    (
        database,
        service,
        case.id.to_string(),
        source.id.to_string(),
    )
}

fn block(
    block_id: &str,
    block_ordinal: u32,
    text: &str,
    citations: Vec<ManuscriptClaimInventoryCitationInput>,
) -> ManuscriptClaimInventoryBlockInput {
    ManuscriptClaimInventoryBlockInput {
        block_id: block_id.to_owned(),
        block_ordinal,
        block_kind: ManuscriptClaimInventoryBlockKind::Paragraph,
        text: text.to_owned(),
        citations,
    }
}

fn claim(
    text: &str,
    source: &str,
    claim_text: &str,
    review_kind: ClaimReviewKind,
) -> ManuscriptClaimInventoryClaimOutput {
    let start = source
        .match_indices(text)
        .next()
        .map(|(byte, _)| source[..byte].chars().count() as u64)
        .unwrap_or_else(|| panic!("missing test excerpt {text:?}"));
    ManuscriptClaimInventoryClaimOutput {
        claim_text: claim_text.to_owned(),
        source_start: start,
        source_end: start + text.chars().count() as u64,
        review_kind,
    }
}

fn inventory_input(
    case_id: &str,
    source_id: &str,
    document_version: i64,
    blocks: Vec<ManuscriptClaimInventoryBlockInput>,
) -> StartManuscriptClaimInventory {
    StartManuscriptClaimInventory {
        research_case_id: nineprofs_research::ResearchCaseId::parse(case_id.to_owned()).unwrap(),
        manuscript_source_id: nineprofs_research::ResearchSourceId::parse(source_id.to_owned())
            .unwrap(),
        document_id: "doc-1".to_owned(),
        document_version,
        blocks,
    }
}

fn provider(
    outputs: HashMap<String, ManuscriptClaimInventoryOutput>,
    calls: Arc<AtomicUsize>,
    fail: bool,
) -> MockInventoryProvider {
    MockInventoryProvider {
        identity: ManuscriptClaimInventoryIdentity {
            provider: "test".to_owned(),
            extractor_version: "inventory-v1".to_owned(),
            model_id: Some("model-a".to_owned()),
            extraction_contract_version: "contract-v1".to_owned(),
        },
        outputs,
        calls,
        fail,
    }
}

#[tokio::test]
async fn inventory_reconstructs_unicode_ranges_and_preserves_modality() {
    let (_database, service, case_id, source_id) = fixture().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let text = "😀 Treatment A may reduce mortality.";
    let excerpt = "Treatment A may reduce mortality.";
    let mut outputs = HashMap::new();
    outputs.insert(
        "b1".to_owned(),
        ManuscriptClaimInventoryOutput {
            claims: vec![claim(
                excerpt,
                text,
                "Treatment A may reduce mortality.",
                ClaimReviewKind::ExternalEvidence,
            )],
        },
    );
    let service = service.with_claim_inventory_extractor(Arc::new(provider(
        outputs,
        Arc::clone(&calls),
        false,
    )));

    let run = service
        .start_manuscript_claim_inventory(inventory_input(
            &case_id,
            &source_id,
            1,
            vec![block("b1", 0, text, Vec::new())],
        ))
        .await
        .unwrap();
    assert_eq!(run.status, ManuscriptClaimInventoryStatus::Completed);
    assert_eq!(run.item_count, 1);
    let items = service
        .list_manuscript_claim_inventory_items(run.id.as_str())
        .await
        .unwrap();
    assert_eq!(items[0].source_excerpt, excerpt);
    assert_eq!(items[0].source_start, 2);
    assert_eq!(items[0].source_end, 35);
    assert_eq!(items[0].review_kind, ClaimReviewKind::ExternalEvidence);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn inventory_covers_cited_uncited_and_no_claim_blocks() {
    let (_database, service, case_id, source_id) = fixture().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let mut outputs = HashMap::new();
    outputs.insert(
        "b1".to_owned(),
        ManuscriptClaimInventoryOutput {
            claims: vec![claim(
                "Cited statement",
                "Cited statement [1].",
                "Cited statement",
                ClaimReviewKind::ExternalEvidence,
            )],
        },
    );
    outputs.insert(
        "b2".to_owned(),
        ManuscriptClaimInventoryOutput {
            claims: vec![claim(
                "An uncited statement.",
                "An uncited statement.",
                "An uncited statement.",
                ClaimReviewKind::ManuscriptInternal,
            )],
        },
    );
    let service = service.with_claim_inventory_extractor(Arc::new(provider(outputs, calls, false)));
    let run = service
        .start_manuscript_claim_inventory(inventory_input(
            &case_id,
            &source_id,
            1,
            vec![
                block(
                    "b1",
                    0,
                    "Cited statement [1].",
                    vec![ManuscriptClaimInventoryCitationInput {
                        start: 16,
                        end: 19,
                        rendered_text: "[1]".to_owned(),
                    }],
                ),
                block("b2", 1, "An uncited statement.", Vec::new()),
                block("b3", 2, "No proposition here.", Vec::new()),
            ],
        ))
        .await
        .unwrap();
    assert_eq!(run.item_count, 2);
    assert_eq!(run.covered_block_count, 3);
    let items = service
        .list_manuscript_claim_inventory_items(run.id.as_str())
        .await
        .unwrap();
    assert_eq!(items[0].overlapping_citation_count, 0);
    let coverage = service
        .list_manuscript_claim_inventory_coverage(run.id.as_str())
        .await
        .unwrap();
    assert_eq!(coverage.len(), 3);
    assert_eq!(
        coverage[0].status,
        ManuscriptClaimInventoryCoverageStatus::Processed
    );
    assert_eq!(
        coverage[1].status,
        ManuscriptClaimInventoryCoverageStatus::Processed
    );
    assert_eq!(
        coverage[2].status,
        ManuscriptClaimInventoryCoverageStatus::NoClaims
    );
}

#[tokio::test]
async fn inventory_allows_atomic_overlapping_claims_and_deduplicates_exact_repeats() {
    let (_database, service, case_id, source_id) = fixture().await;
    let text = "A causes B and C.";
    let mut outputs = HashMap::new();
    let first = claim(
        "A causes B",
        text,
        "A causes B",
        ClaimReviewKind::Interpretive,
    );
    let second = claim(
        "causes B and C",
        text,
        "causes B and C",
        ClaimReviewKind::Uncertain,
    );
    outputs.insert(
        "b1".to_owned(),
        ManuscriptClaimInventoryOutput {
            claims: vec![first.clone(), first, second],
        },
    );
    let service = service.with_claim_inventory_extractor(Arc::new(provider(
        outputs,
        Arc::new(AtomicUsize::new(0)),
        false,
    )));
    let run = service
        .start_manuscript_claim_inventory(inventory_input(
            &case_id,
            &source_id,
            1,
            vec![block("b1", 0, text, Vec::new())],
        ))
        .await
        .unwrap();
    let items = service
        .list_manuscript_claim_inventory_items(run.id.as_str())
        .await
        .unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].review_kind, ClaimReviewKind::Interpretive);
    assert_eq!(items[1].review_kind, ClaimReviewKind::Uncertain);
}

#[tokio::test]
async fn inventory_is_idempotent_by_source_document_version_and_provider_identity() {
    let (_database, service, case_id, source_id) = fixture().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let service = service.with_claim_inventory_extractor(Arc::new(provider(
        HashMap::new(),
        Arc::clone(&calls),
        false,
    )));
    let input = inventory_input(
        &case_id,
        &source_id,
        1,
        vec![block("b1", 0, "No claims.", Vec::new())],
    );
    let first = service
        .start_manuscript_claim_inventory(input.clone())
        .await
        .unwrap();
    let repeated = service
        .start_manuscript_claim_inventory(input)
        .await
        .unwrap();
    assert_eq!(first.id, repeated.id);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let changed = service
        .start_manuscript_claim_inventory(inventory_input(
            &case_id,
            &source_id,
            2,
            vec![block("b1", 0, "No claims.", Vec::new())],
        ))
        .await
        .unwrap();
    assert_ne!(first.id, changed.id);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn inventory_fails_closed_for_invalid_ranges_and_provider_failures() {
    let (database, service, case_id, source_id) = fixture().await;
    let invalid_calls = Arc::new(AtomicUsize::new(0));
    let mut outputs = HashMap::new();
    outputs.insert(
        "b1".to_owned(),
        ManuscriptClaimInventoryOutput {
            claims: vec![ManuscriptClaimInventoryClaimOutput {
                claim_text: "invalid".to_owned(),
                source_start: 8,
                source_end: 20,
                review_kind: ClaimReviewKind::Uncertain,
            }],
        },
    );
    let invalid_service = service
        .clone()
        .with_claim_inventory_extractor(Arc::new(provider(
            outputs,
            Arc::clone(&invalid_calls),
            false,
        )));
    let error = invalid_service
        .start_manuscript_claim_inventory(inventory_input(
            &case_id,
            &source_id,
            1,
            vec![block("b1", 0, "short", Vec::new())],
        ))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ResearchError::ManuscriptClaimInventoryFailed(code) if code == "invalid_structured_output"
    ));
    let failed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM research_manuscript_claim_inventory_runs WHERE status = 'failed'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(failed_count, 1);

    let failure_calls = Arc::new(AtomicUsize::new(0));
    let failing_service = service.with_claim_inventory_extractor(Arc::new(provider(
        HashMap::new(),
        Arc::clone(&failure_calls),
        true,
    )));
    let error = failing_service
        .start_manuscript_claim_inventory(inventory_input(
            &case_id,
            &source_id,
            2,
            vec![block("b1", 0, "short", Vec::new())],
        ))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        ResearchError::ManuscriptClaimInventoryFailed(code) if code == "transport_failure"
    ));
    assert_eq!(failure_calls.load(Ordering::SeqCst), 1);
    let item_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM research_manuscript_claim_inventory_items")
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(item_count, 0);
}

#[tokio::test]
async fn inventory_rejects_cross_case_sources_and_has_no_evidence_side_effects() {
    let (_database, service, case_id, _source_id) = fixture().await;
    let other_case = service
        .create_case(CreateResearchCase {
            title: "Other case".to_owned(),
        })
        .await
        .unwrap();
    let other_source = service
        .create_source(CreateResearchSource {
            research_case_id: other_case.id,
            kind: SourceKind::Manuscript,
            label: "Other draft".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = service.with_claim_inventory_extractor(Arc::new(provider(
        HashMap::new(),
        Arc::clone(&calls),
        false,
    )));
    let error = service
        .start_manuscript_claim_inventory(inventory_input(
            &case_id,
            other_source.id.as_str(),
            1,
            vec![block("b1", 0, "No claims.", Vec::new())],
        ))
        .await
        .unwrap_err();
    assert!(matches!(error, ResearchError::Invalid(_)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(
        service
            .list_claims(Some(&case_id))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .list_links(Some(&case_id), None, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .list_evidence(Some(&case_id), None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .list_citation_occurrences(Some(&case_id))
            .await
            .unwrap()
            .is_empty()
    );
}
