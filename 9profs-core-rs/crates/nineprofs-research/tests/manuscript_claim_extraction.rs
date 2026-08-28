use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CreateResearchCase, CreateResearchSource, ExtractManuscriptClaims, ManuscriptCitationFormat,
    ManuscriptCitationSyncCitationInput, ManuscriptCitationSyncTargetInput,
    ManuscriptClaimExtractionBlockInput, ManuscriptClaimExtractionClaimOutput,
    ManuscriptClaimExtractionCoverageStatus, ManuscriptClaimExtractionIdentity,
    ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProvider,
    ManuscriptClaimExtractionProviderError, ManuscriptClaimExtractionStatus,
    ManuscriptClaimExtractionUnassociatedCitation, ResearchError, ResearchService, SourceKind,
    SqliteResearchRepository, SyncManuscriptCitations,
};

const BLOCK_TEXT: &str = "😀 Treatment improved outcomes [1].";
const CITATION_START: u64 = 30;
const CITATION_END: u64 = 33;

struct MockExtractor {
    identity: ManuscriptClaimExtractionIdentity,
    output: ManuscriptClaimExtractionOutput,
    calls: Arc<AtomicUsize>,
    fail_on_call: Option<usize>,
}

#[async_trait]
impl ManuscriptClaimExtractionProvider for MockExtractor {
    fn identity(&self) -> ManuscriptClaimExtractionIdentity {
        self.identity.clone()
    }

    async fn extract(
        &self,
        _block: ManuscriptClaimExtractionBlockInput,
    ) -> Result<ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProviderError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_call == Some(call) {
            return Err(ManuscriptClaimExtractionProviderError::Transport);
        }
        Ok(self.output.clone())
    }
}

async fn fixture() -> (
    Database,
    ResearchService,
    nineprofs_research::ManuscriptCitationSyncRun,
    String,
) {
    let database = Database::in_memory().await.unwrap();
    let service = ResearchService::new(
        SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(64)),
    );
    let case = service
        .create_case(CreateResearchCase {
            title: "Claim extraction".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id,
            kind: SourceKind::Manuscript,
            label: "Draft".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let sync_run = service
        .sync_manuscript_citations(SyncManuscriptCitations {
            research_case_id: source.research_case_id.clone(),
            manuscript_source_id: source.id,
            document_id: "doc-1".to_owned(),
            document_version: 1,
            citations: vec![ManuscriptCitationSyncCitationInput {
                format: ManuscriptCitationFormat::Zotero,
                rendered_text: "[1]".to_owned(),
                block_id: "b7".to_owned(),
                start: CITATION_START,
                end: CITATION_END,
                targets: vec![ManuscriptCitationSyncTargetInput {
                    ordinal: 0,
                    reference_key: "ref-1".to_owned(),
                    cited_locator: None,
                }],
            }],
        })
        .await
        .unwrap();
    let occurrence_id = service
        .list_manuscript_citation_sync_occurrences(sync_run.id.as_str())
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .citation_occurrence_id
        .to_string();
    (database, service, sync_run, occurrence_id)
}

fn extraction_input(
    sync_run: &nineprofs_research::ManuscriptCitationSyncRun,
    occurrence_id: &str,
) -> ExtractManuscriptClaims {
    ExtractManuscriptClaims {
        citation_sync_run_id: sync_run.id.clone(),
        document_id: sync_run.document_id.clone(),
        document_version: sync_run.document_version,
        blocks: vec![ManuscriptClaimExtractionBlockInput {
            block_id: "b7".to_owned(),
            text: BLOCK_TEXT.to_owned(),
            citations: vec![nineprofs_research::ManuscriptClaimExtractionCitationInput {
                citation_occurrence_id: occurrence_id.to_owned(),
                start: CITATION_START,
                end: CITATION_END,
                rendered_text: "[1]".to_owned(),
            }],
        }],
    }
}

fn mock(
    occurrence_id: &str,
    output: ManuscriptClaimExtractionOutput,
    version: &str,
    model_id: Option<&str>,
    calls: Arc<AtomicUsize>,
    fail_on_call: Option<usize>,
) -> MockExtractor {
    MockExtractor {
        identity: ManuscriptClaimExtractionIdentity {
            provider: "test".to_owned(),
            extractor_version: version.to_owned(),
            model_id: model_id.map(str::to_owned),
            extraction_contract_version: "test-contract-v1".to_owned(),
        },
        output: if output.claims.is_empty() && output.unassociated_citations.is_empty() {
            ManuscriptClaimExtractionOutput {
                claims: Vec::new(),
                unassociated_citations: vec![ManuscriptClaimExtractionUnassociatedCitation {
                    citation_occurrence_id: occurrence_id.to_owned(),
                    reason: "no independently verifiable proposition".to_owned(),
                }],
            }
        } else {
            output
        },
        calls,
        fail_on_call,
    }
}

#[tokio::test]
async fn extraction_reconstructs_unicode_source_and_is_identity_idempotent() {
    let (_database, service, sync_run, occurrence_id) = fixture().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let output = ManuscriptClaimExtractionOutput {
        claims: vec![ManuscriptClaimExtractionClaimOutput {
            claim_text: "Treatment improved outcomes".to_owned(),
            source_start: 2,
            source_end: 29,
            citation_occurrence_ids: vec![occurrence_id.clone()],
        }],
        unassociated_citations: Vec::new(),
    };
    let service = service.with_claim_extractor(Arc::new(mock(
        &occurrence_id,
        output,
        "extractor-v1",
        Some("model-a"),
        Arc::clone(&calls),
        None,
    )));

    let input = extraction_input(&sync_run, &occurrence_id);
    let first = service
        .extract_manuscript_claims(input.clone())
        .await
        .unwrap();
    assert_eq!(first.status, ManuscriptClaimExtractionStatus::Completed);
    assert_eq!(first.claim_count, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let items = service
        .list_manuscript_claim_extraction_items(first.id.as_str())
        .await
        .unwrap();
    assert_eq!(items[0].source_excerpt, "Treatment improved outcomes");
    assert_eq!(items[0].source_start, 2);
    assert_eq!(items[0].source_end, 29);
    let coverage = service
        .list_manuscript_claim_extraction_coverage(first.id.as_str())
        .await
        .unwrap();
    assert!(matches!(
        coverage[0].status,
        ManuscriptClaimExtractionCoverageStatus::AssociatedWithClaim
    ));
    assert!(coverage[0].extraction_item_id.is_some());
    assert!(coverage[0].claim_citation_link_id.is_some());
    assert_eq!(
        service
            .list_claims(Some(first.research_case_id.as_str()))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        service
            .list_claim_citation_links(Some(first.research_case_id.as_str()), None, None)
            .await
            .unwrap()
            .len(),
        1
    );

    let repeated = service.extract_manuscript_claims(input).await.unwrap();
    assert_eq!(repeated.id, first.id);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let changed_occurrence_id = occurrence_id.clone();
    let changed_identity = service.clone().with_claim_extractor(Arc::new(mock(
        &changed_occurrence_id,
        ManuscriptClaimExtractionOutput {
            claims: vec![ManuscriptClaimExtractionClaimOutput {
                claim_text: "Treatment improved outcomes".to_owned(),
                source_start: 2,
                source_end: 29,
                citation_occurrence_ids: vec![changed_occurrence_id.clone()],
            }],
            unassociated_citations: Vec::new(),
        },
        "extractor-v2",
        Some("model-b"),
        Arc::new(AtomicUsize::new(0)),
        None,
    )));
    let second = changed_identity
        .extract_manuscript_claims(extraction_input(&sync_run, &changed_occurrence_id))
        .await
        .unwrap();
    assert_ne!(second.id, first.id);
    let runs = service
        .list_manuscript_claim_extractions(Some(sync_run.id.as_str()))
        .await
        .unwrap();
    assert_eq!(runs.len(), 2);
}

#[tokio::test]
async fn stale_sync_context_is_rejected_before_provider_call() {
    let (_database, service, sync_run, occurrence_id) = fixture().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let service = service.with_claim_extractor(Arc::new(mock(
        &occurrence_id,
        ManuscriptClaimExtractionOutput {
            claims: Vec::new(),
            unassociated_citations: Vec::new(),
        },
        "extractor-v1",
        None,
        Arc::clone(&calls),
        None,
    )));
    let mut input = extraction_input(&sync_run, &occurrence_id);
    input.document_version = 2;

    assert!(matches!(
        service.extract_manuscript_claims(input).await,
        Err(ResearchError::ManuscriptClaimExtractionStale)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(
        service
            .list_manuscript_claim_extractions(Some(sync_run.id.as_str()))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn provider_failure_commits_failed_run_without_partial_claims() {
    let (_database, service, sync_run, occurrence_id) = fixture().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let service = service.with_claim_extractor(Arc::new(mock(
        &occurrence_id,
        ManuscriptClaimExtractionOutput {
            claims: vec![ManuscriptClaimExtractionClaimOutput {
                claim_text: "Treatment improved outcomes".to_owned(),
                source_start: 2,
                source_end: 29,
                citation_occurrence_ids: vec![occurrence_id.clone()],
            }],
            unassociated_citations: Vec::new(),
        },
        "extractor-v1",
        Some("model-a"),
        calls,
        Some(1),
    )));

    let result = service
        .extract_manuscript_claims(extraction_input(&sync_run, &occurrence_id))
        .await
        .unwrap_err();
    assert!(matches!(
        result,
        ResearchError::ManuscriptClaimExtractionFailed(code) if code == "transport_failure"
    ));
    let runs = service
        .list_manuscript_claim_extractions(Some(sync_run.id.as_str()))
        .await
        .unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, ManuscriptClaimExtractionStatus::Failed);
    assert_eq!(runs[0].claim_count, 0);
    assert_eq!(
        service
            .list_claims(Some(runs[0].research_case_id.as_str()))
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn no_claim_output_records_explicit_coverage_without_fake_claim() {
    let (_database, service, sync_run, occurrence_id) = fixture().await;
    let service = service.with_claim_extractor(Arc::new(mock(
        &occurrence_id,
        ManuscriptClaimExtractionOutput {
            claims: Vec::new(),
            unassociated_citations: Vec::new(),
        },
        "extractor-v1",
        None,
        Arc::new(AtomicUsize::new(0)),
        None,
    )));
    let run = service
        .extract_manuscript_claims(extraction_input(&sync_run, &occurrence_id))
        .await
        .unwrap();
    assert_eq!(run.claim_count, 0);
    let coverage = service
        .list_manuscript_claim_extraction_coverage(run.id.as_str())
        .await
        .unwrap();
    assert!(matches!(
        coverage[0].status,
        ManuscriptClaimExtractionCoverageStatus::NoVerifiableClaim
    ));
    assert!(coverage[0].extraction_item_id.is_none());
    assert!(coverage[0].claim_citation_link_id.is_none());
    assert_eq!(
        service
            .list_claims(Some(run.research_case_id.as_str()))
            .await
            .unwrap()
            .len(),
        0
    );
}
