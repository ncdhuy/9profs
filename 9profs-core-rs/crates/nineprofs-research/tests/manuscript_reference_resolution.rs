use std::{collections::BTreeMap, sync::Arc};

use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CapturePdfExtraction, CapturePdfPage, CitationBindingMethod, CreateCitationTargetBinding,
    CreateResearchCase, CreateResearchSource, ManuscriptCitationFormat,
    ManuscriptCitationSyncCitationInput, ManuscriptCitationSyncTargetInput,
    ManuscriptReferenceCatalogCitationInput, ManuscriptReferenceCatalogTargetInput,
    ManuscriptReferenceCatalogWordSourceInput, ManuscriptReferenceCatalogZoteroInput,
    ManuscriptReferenceResolutionOutcome, ResearchArtifactStore, ResearchService, ResearchSource,
    ResearchSourceIdentityInput, ResearchSourceIdentityMethod, SourceKind,
    SqliteResearchRepository, SyncManuscriptCitations, SyncManuscriptReferenceCatalog,
};
use sqlx::Row;

struct Fixture {
    database: Database,
    service: ResearchService,
    case_id: nineprofs_research::ResearchCaseId,
    catalog: nineprofs_research::ManuscriptReferenceCatalogRun,
    reference_source: Option<ResearchSource>,
}

async fn fixture(
    key: &str,
    format: ManuscriptCitationFormat,
    target_count: u32,
    reference_source: Option<(SourceKind, String, Option<ResearchSourceIdentityInput>)>,
) -> Fixture {
    let database = Database::in_memory().await.unwrap();
    let store = Arc::new(ResearchArtifactStore::new(
        std::env::temp_dir().join(format!(
            "9profs-reference-resolution-{}-{key}",
            std::process::id()
        )),
        database.pool().clone(),
    ));
    let service = ResearchService::new(
        SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(32)),
    )
    .with_artifact_store(store);
    let case = service
        .create_case(CreateResearchCase {
            title: "Reference resolution".to_owned(),
        })
        .await
        .unwrap();
    let manuscript = service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Draft".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let targets = (0..target_count)
        .map(|ordinal| ManuscriptCitationSyncTargetInput {
            ordinal,
            reference_key: key.to_owned(),
            cited_locator: None,
        })
        .collect();
    let sync_run = service
        .sync_manuscript_citations(SyncManuscriptCitations {
            research_case_id: case.id.clone(),
            manuscript_source_id: manuscript.id,
            document_id: "doc-1".to_owned(),
            document_version: 1,
            citations: vec![ManuscriptCitationSyncCitationInput {
                format: format.clone(),
                rendered_text: format!("[{key}]"),
                block_id: "block-1".to_owned(),
                start: 1,
                end: 2,
                targets,
            }],
        })
        .await
        .unwrap();
    let catalog = service
        .sync_manuscript_reference_catalog(catalog_input(&service, &sync_run).await)
        .await
        .unwrap();
    let reference_source = if let Some((kind, label, identity)) = reference_source {
        Some(
            service
                .create_source(CreateResearchSource {
                    research_case_id: case.id.clone(),
                    kind,
                    label,
                    identity,
                })
                .await
                .unwrap(),
        )
    } else {
        None
    };
    Fixture {
        database,
        service,
        case_id: case.id,
        catalog,
        reference_source,
    }
}

async fn catalog_input(
    service: &ResearchService,
    sync_run: &nineprofs_research::ManuscriptCitationSyncRun,
) -> SyncManuscriptReferenceCatalog {
    let occurrences = service
        .list_manuscript_citation_sync_occurrences(sync_run.id.as_str())
        .await
        .unwrap();
    let mut citations = Vec::new();
    for occurrence in occurrences {
        let sync_targets = service
            .list_manuscript_citation_sync_targets(occurrence.id.as_str())
            .await
            .unwrap();
        let targets = service
            .list_citation_targets(occurrence.citation_occurrence_id.as_str())
            .await
            .unwrap();
        let format = occurrence.format.clone();
        citations.push(ManuscriptReferenceCatalogCitationInput {
            citation_occurrence_id: occurrence.citation_occurrence_id.to_string(),
            block_id: occurrence.document_block_id,
            start: occurrence.start,
            end: occurrence.end,
            format: format.clone(),
            targets: targets
                .into_iter()
                .zip(sync_targets)
                .map(
                    |(target, sync_target)| ManuscriptReferenceCatalogTargetInput {
                        citation_target_id: sync_target.citation_target_id.to_string(),
                        ordinal: target.ordinal,
                        reference_key: target.reference_key.clone(),
                        word_source: matches!(&format, ManuscriptCitationFormat::WordNative).then(
                            || ManuscriptReferenceCatalogWordSourceInput {
                                tag: target.reference_key.clone(),
                                title: "Safe title".to_owned(),
                                author: "Safe author".to_owned(),
                                year: "2020".to_owned(),
                            },
                        ),
                        zotero: matches!(&format, ManuscriptCitationFormat::Zotero).then(|| {
                            ManuscriptReferenceCatalogZoteroInput {
                                item_id: Some(format!("item-{}", target.reference_key)),
                                uris: vec![format!(
                                    "zotero://select/items/{}",
                                    target.reference_key
                                )],
                            }
                        }),
                    },
                )
                .collect(),
        });
    }
    SyncManuscriptReferenceCatalog {
        citation_sync_run_id: sync_run.id.clone(),
        document_id: sync_run.document_id.clone(),
        document_version: sync_run.document_version,
        citations,
    }
}

fn zotero_identity(key: &str) -> ResearchSourceIdentityInput {
    ResearchSourceIdentityInput {
        provider: "zotero".to_owned(),
        external_reference: format!("item-{key}"),
        method: ResearchSourceIdentityMethod::Imported,
    }
}

async fn add_pdf(
    fixture: &Fixture,
    status: nineprofs_research::PdfExtractionStatus,
    suffix: &str,
) -> (
    nineprofs_research::ResearchSourceSnapshot,
    nineprofs_research::ResearchPdfExtraction,
) {
    let store = fixture.service.artifact_store().unwrap();
    let mut upload = store.begin_upload(format!("{suffix}.pdf")).unwrap();
    upload
        .append(format!("%PDF-1.7\n{suffix}").as_bytes())
        .unwrap();
    let artifact = upload.finish().await.unwrap();
    let source = fixture.reference_source.as_ref().unwrap();
    let snapshot = fixture
        .service
        .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
        .await
        .unwrap();
    let pages = if matches!(&status, nineprofs_research::PdfExtractionStatus::Ready) {
        vec![CapturePdfPage {
            page: 1,
            text: "ready text".to_owned(),
        }]
    } else {
        Vec::new()
    };
    let extraction = fixture
        .service
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id.clone(),
            extractor: "fixture".to_owned(),
            extractor_version: Some("1".to_owned()),
            page_count: 1,
            status,
            pages,
        })
        .await
        .unwrap();
    (snapshot, extraction)
}

async fn mappings(fixture: &Fixture) -> Vec<nineprofs_research::ManuscriptReferenceTargetMapping> {
    let entry = fixture
        .service
        .list_manuscript_reference_entries(fixture.catalog.id.as_str())
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    fixture
        .service
        .list_manuscript_reference_target_mappings(entry.id.as_str())
        .await
        .unwrap()
}

#[tokio::test]
async fn exact_ready_pdf_resolution_binds_all_targets_and_is_idempotent() {
    let fixture = fixture(
        "ref-a",
        ManuscriptCitationFormat::Zotero,
        2,
        Some((
            SourceKind::ReferencePdf,
            "Reference A".to_owned(),
            Some(zotero_identity("ref-a")),
        )),
    )
    .await;
    let (_snapshot, extraction) = add_pdf(
        &fixture,
        nineprofs_research::PdfExtractionStatus::Ready,
        "exact-ready",
    )
    .await;
    let run = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let entries = fixture
        .service
        .list_manuscript_reference_resolution_entries(run.id.as_str())
        .await
        .unwrap();
    assert!(matches!(
        entries[0].outcome,
        ManuscriptReferenceResolutionOutcome::ResolvedExact
    ));
    assert_eq!(
        entries[0].chosen_source_id,
        fixture.reference_source.as_ref().map(|s| s.id.clone())
    );
    assert_eq!(entries[0].chosen_extraction_id, Some(extraction.id.clone()));

    for mapping in mappings(&fixture).await {
        let bindings = fixture
            .service
            .list_citation_target_bindings(mapping.citation_target_id.as_str())
            .await
            .unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].method,
            CitationBindingMethod::DeterministicResolver
        );
        assert_eq!(
            bindings[0].source_id,
            fixture.reference_source.as_ref().unwrap().id
        );
        assert_eq!(
            bindings[0].source_snapshot_id,
            entries[0].chosen_source_snapshot_id
        );
        assert_eq!(bindings[0].extraction_id, Some(extraction.id.clone()));
    }
    let repeat = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    assert_eq!(repeat.id, run.id);
    let evidence_count = sqlx::query("SELECT COUNT(*) AS count FROM research_evidence")
        .fetch_one(fixture.database.pool())
        .await
        .unwrap()
        .get::<i64, _>("count");
    let verification_count =
        sqlx::query("SELECT COUNT(*) AS count FROM research_citation_verification_runs")
            .fetch_one(fixture.database.pool())
            .await
            .unwrap()
            .get::<i64, _>("count");
    let claim_evidence_count = sqlx::query("SELECT COUNT(*) AS count FROM research_claim_evidence")
        .fetch_one(fixture.database.pool())
        .await
        .unwrap()
        .get::<i64, _>("count");
    assert_eq!(evidence_count, 0);
    assert_eq!(verification_count, 0);
    assert_eq!(claim_evidence_count, 0);
}

#[tokio::test]
async fn equivalent_existing_binding_is_reported_without_duplicate() {
    let fixture = fixture(
        "ref-b",
        ManuscriptCitationFormat::Zotero,
        1,
        Some((
            SourceKind::Other,
            "Reference B".to_owned(),
            Some(zotero_identity("ref-b")),
        )),
    )
    .await;
    let mapping = mappings(&fixture).await.remove(0);
    fixture
        .service
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: fixture.case_id.clone(),
            citation_target_id: mapping.citation_target_id.clone(),
            source_id: fixture.reference_source.as_ref().unwrap().id.clone(),
            source_snapshot_id: None,
            extraction_id: None,
            method: CitationBindingMethod::Human,
        })
        .await
        .unwrap();
    let run = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let entry = fixture
        .service
        .list_manuscript_reference_resolution_entries(run.id.as_str())
        .await
        .unwrap()
        .remove(0);
    assert!(matches!(
        entry.outcome,
        ManuscriptReferenceResolutionOutcome::AlreadyBound
    ));
    assert_eq!(
        fixture
            .service
            .list_citation_target_bindings(mapping.citation_target_id.as_str())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn conflicting_existing_binding_fails_closed() {
    let fixture = fixture(
        "ref-c",
        ManuscriptCitationFormat::Zotero,
        1,
        Some((
            SourceKind::Other,
            "Reference C".to_owned(),
            Some(zotero_identity("ref-c")),
        )),
    )
    .await;
    let competing_source = fixture
        .service
        .create_source(CreateResearchSource {
            research_case_id: fixture.case_id.clone(),
            kind: SourceKind::Other,
            label: "Competing source".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let mapping = mappings(&fixture).await.remove(0);
    fixture
        .service
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: fixture.case_id.clone(),
            citation_target_id: mapping.citation_target_id.clone(),
            source_id: competing_source.id.clone(),
            source_snapshot_id: None,
            extraction_id: None,
            method: CitationBindingMethod::Human,
        })
        .await
        .unwrap();
    let run = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let entry = fixture
        .service
        .list_manuscript_reference_resolution_entries(run.id.as_str())
        .await
        .unwrap()
        .remove(0);
    assert!(matches!(
        entry.outcome,
        ManuscriptReferenceResolutionOutcome::ConflictWithExistingBinding
    ));
    let bindings = fixture
        .service
        .list_citation_target_bindings(mapping.citation_target_id.as_str())
        .await
        .unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].source_id, competing_source.id);
}

#[tokio::test]
async fn weak_metadata_creates_candidate_only_and_confirmation_uses_human_binding() {
    let fixture = fixture(
        "Smith2020",
        ManuscriptCitationFormat::WordNative,
        1,
        Some((SourceKind::Other, "Safe title".to_owned(), None)),
    )
    .await;
    let mapping = mappings(&fixture).await.remove(0);
    let run = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let entry = fixture
        .service
        .list_manuscript_reference_resolution_entries(run.id.as_str())
        .await
        .unwrap()
        .remove(0);
    assert!(matches!(
        entry.outcome,
        ManuscriptReferenceResolutionOutcome::CandidateRequiresConfirmation
    ));
    assert!(
        fixture
            .service
            .list_citation_target_bindings(mapping.citation_target_id.as_str())
            .await
            .unwrap()
            .is_empty()
    );
    let candidate = fixture
        .service
        .list_manuscript_reference_resolution_candidates(entry.id.as_str())
        .await
        .unwrap()
        .remove(0);
    assert!(
        fixture
            .service
            .confirm_manuscript_reference_candidate(
                run.id.as_str(),
                entry.id.as_str(),
                nineprofs_research::ManuscriptReferenceResolutionCandidateId::new().as_str(),
            )
            .await
            .is_err()
    );
    let bindings = fixture
        .service
        .confirm_manuscript_reference_candidate(
            run.id.as_str(),
            entry.id.as_str(),
            candidate.id.as_str(),
        )
        .await
        .unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].method, CitationBindingMethod::Human);
}

#[tokio::test]
async fn ambiguous_source_never_auto_binds() {
    let fixture = fixture(
        "ref-d",
        ManuscriptCitationFormat::Zotero,
        1,
        Some((
            SourceKind::Other,
            "Reference D1".to_owned(),
            Some(zotero_identity("ref-d")),
        )),
    )
    .await;
    fixture
        .service
        .create_source(CreateResearchSource {
            research_case_id: fixture.case_id.clone(),
            kind: SourceKind::Other,
            label: "Reference D2".to_owned(),
            identity: Some(zotero_identity("ref-d")),
        })
        .await
        .unwrap();
    let mapping = mappings(&fixture).await.remove(0);
    let run = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let entry = fixture
        .service
        .list_manuscript_reference_resolution_entries(run.id.as_str())
        .await
        .unwrap()
        .remove(0);
    assert!(matches!(
        entry.outcome,
        ManuscriptReferenceResolutionOutcome::AmbiguousSource
    ));
    assert!(
        fixture
            .service
            .list_citation_target_bindings(mapping.citation_target_id.as_str())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn multiple_ready_pdf_chains_are_explicitly_ambiguous() {
    let fixture = fixture(
        "ref-e",
        ManuscriptCitationFormat::Zotero,
        1,
        Some((
            SourceKind::ReferencePdf,
            "Reference E".to_owned(),
            Some(zotero_identity("ref-e")),
        )),
    )
    .await;
    add_pdf(
        &fixture,
        nineprofs_research::PdfExtractionStatus::Ready,
        "ambiguous-one",
    )
    .await;
    add_pdf(
        &fixture,
        nineprofs_research::PdfExtractionStatus::Ready,
        "ambiguous-two",
    )
    .await;
    let mapping = mappings(&fixture).await.remove(0);
    let run = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let entry = fixture
        .service
        .list_manuscript_reference_resolution_entries(run.id.as_str())
        .await
        .unwrap()
        .remove(0);
    assert!(matches!(
        entry.outcome,
        ManuscriptReferenceResolutionOutcome::AmbiguousSnapshotOrExtraction
    ));
    assert_eq!(entry.candidate_count, 2);
    assert!(
        fixture
            .service
            .list_citation_target_bindings(mapping.citation_target_id.as_str())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn exact_source_without_ready_extraction_is_not_verification_ready() {
    let fixture = fixture(
        "ref-f",
        ManuscriptCitationFormat::Zotero,
        1,
        Some((
            SourceKind::ReferencePdf,
            "Reference F".to_owned(),
            Some(zotero_identity("ref-f")),
        )),
    )
    .await;
    add_pdf(
        &fixture,
        nineprofs_research::PdfExtractionStatus::Failed,
        "not-ready",
    )
    .await;
    let mapping = mappings(&fixture).await.remove(0);
    let run = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let entry = fixture
        .service
        .list_manuscript_reference_resolution_entries(run.id.as_str())
        .await
        .unwrap()
        .remove(0);
    assert!(matches!(
        entry.outcome,
        ManuscriptReferenceResolutionOutcome::SourceMatchedButNotVerificationReady
    ));
    assert!(
        fixture
            .service
            .list_citation_target_bindings(mapping.citation_target_id.as_str())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn sources_from_another_case_are_not_candidates() {
    let fixture = fixture("ref-g", ManuscriptCitationFormat::Zotero, 1, None).await;
    let other_case = fixture
        .service
        .create_case(CreateResearchCase {
            title: "Other case".to_owned(),
        })
        .await
        .unwrap();
    fixture
        .service
        .create_source(CreateResearchSource {
            research_case_id: other_case.id,
            kind: SourceKind::Other,
            label: "Cross-case".to_owned(),
            identity: Some(zotero_identity("ref-g")),
        })
        .await
        .unwrap();
    let run = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let entry = fixture
        .service
        .list_manuscript_reference_resolution_entries(run.id.as_str())
        .await
        .unwrap()
        .remove(0);
    assert!(matches!(
        entry.outcome,
        ManuscriptReferenceResolutionOutcome::Unresolved
    ));
}

#[tokio::test]
async fn invalid_mapping_fails_closed_without_binding() {
    let fixture = fixture(
        "ref-h",
        ManuscriptCitationFormat::Zotero,
        1,
        Some((
            SourceKind::Other,
            "Reference H".to_owned(),
            Some(zotero_identity("ref-h")),
        )),
    )
    .await;
    let mapping = mappings(&fixture).await.remove(0);
    let entry_id = fixture
        .service
        .list_manuscript_reference_entries(fixture.catalog.id.as_str())
        .await
        .unwrap()
        .remove(0)
        .id;
    sqlx::query(
        "DELETE FROM research_manuscript_reference_target_mappings WHERE reference_entry_id = ?",
    )
    .bind(entry_id.as_str())
    .execute(fixture.database.pool())
    .await
    .unwrap();
    let run = fixture
        .service
        .resolve_manuscript_references(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let resolution_entry = fixture
        .service
        .list_manuscript_reference_resolution_entries(run.id.as_str())
        .await
        .unwrap()
        .remove(0);
    assert!(matches!(
        resolution_entry.outcome,
        ManuscriptReferenceResolutionOutcome::Failed
    ));
    assert!(
        fixture
            .service
            .list_citation_target_bindings(mapping.citation_target_id.as_str())
            .await
            .unwrap()
            .is_empty()
    );
}
