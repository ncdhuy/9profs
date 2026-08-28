use std::{collections::BTreeMap, sync::Arc};

use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CapturePdfExtraction, CapturePdfPage, CitationBindingMethod, CreateCitationTargetBinding,
    CreateResearchCase, CreateResearchSource, ManuscriptCitationFormat,
    ManuscriptCitationSyncCitationInput, ManuscriptCitationSyncTargetInput,
    ManuscriptReferenceCatalogCitationInput, ManuscriptReferenceCatalogTargetInput,
    ManuscriptReferenceCatalogWordSourceInput, ManuscriptReferenceCatalogZoteroInput,
    ManuscriptReferenceResolutionOutcome, ResearchArtifactStore, ResearchRepository,
    ResearchService, ResearchSource, ResearchSourceIdentityInput, ResearchSourceIdentityMethod,
    SourceKind, SqliteResearchRepository, SyncManuscriptCitations, SyncManuscriptReferenceCatalog,
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
    fixture_with_database(
        Database::in_memory().await.unwrap(),
        key,
        format,
        target_count,
        reference_source,
    )
    .await
}

async fn fixture_with_database(
    database: Database,
    key: &str,
    format: ManuscriptCitationFormat,
    target_count: u32,
    reference_source: Option<(SourceKind, String, Option<ResearchSourceIdentityInput>)>,
) -> Fixture {
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
async fn mixed_equivalent_and_unbound_targets_preserve_existing_methods() {
    let fixture = fixture(
        "ref-mixed",
        ManuscriptCitationFormat::Zotero,
        3,
        Some((
            SourceKind::ReferencePdf,
            "Reference mixed".to_owned(),
            Some(zotero_identity("ref-mixed")),
        )),
    )
    .await;
    let (snapshot, extraction) = add_pdf(
        &fixture,
        nineprofs_research::PdfExtractionStatus::Ready,
        "mixed-ready",
    )
    .await;
    let mappings = mappings(&fixture).await;
    let source_id = fixture.reference_source.as_ref().unwrap().id.clone();
    let human = fixture
        .service
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: fixture.case_id.clone(),
            citation_target_id: mappings[0].citation_target_id.clone(),
            source_id: source_id.clone(),
            source_snapshot_id: Some(snapshot.id.clone()),
            extraction_id: Some(extraction.id.clone()),
            method: CitationBindingMethod::Human,
        })
        .await
        .unwrap();
    let imported = fixture
        .service
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: fixture.case_id.clone(),
            citation_target_id: mappings[2].citation_target_id.clone(),
            source_id,
            source_snapshot_id: Some(snapshot.id),
            extraction_id: Some(extraction.id.clone()),
            method: CitationBindingMethod::Imported,
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
        ManuscriptReferenceResolutionOutcome::ResolvedExact
    ));

    let first = fixture
        .service
        .list_citation_target_bindings(mappings[0].citation_target_id.as_str())
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, human.id);
    assert_eq!(first[0].method, CitationBindingMethod::Human);

    let second = fixture
        .service
        .list_citation_target_bindings(mappings[1].citation_target_id.as_str())
        .await
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(
        second[0].method,
        CitationBindingMethod::DeterministicResolver
    );
    assert_eq!(
        second[0].source_snapshot_id,
        entry.chosen_source_snapshot_id
    );
    assert_eq!(second[0].extraction_id, Some(extraction.id));

    let third = fixture
        .service
        .list_citation_target_bindings(mappings[2].citation_target_id.as_str())
        .await
        .unwrap();
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].id, imported.id);
    assert_eq!(third[0].method, CitationBindingMethod::Imported);
}

#[tokio::test]
async fn mixed_targets_with_conflict_write_no_new_bindings() {
    let fixture = fixture(
        "ref-mixed-conflict",
        ManuscriptCitationFormat::Zotero,
        3,
        Some((
            SourceKind::Other,
            "Reference mixed conflict".to_owned(),
            Some(zotero_identity("ref-mixed-conflict")),
        )),
    )
    .await;
    let competing_source = fixture
        .service
        .create_source(CreateResearchSource {
            research_case_id: fixture.case_id.clone(),
            kind: SourceKind::Other,
            label: "Competing mixed source".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let mappings = mappings(&fixture).await;
    let source_id = fixture.reference_source.as_ref().unwrap().id.clone();
    let preserved = fixture
        .service
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: fixture.case_id.clone(),
            citation_target_id: mappings[0].citation_target_id.clone(),
            source_id,
            source_snapshot_id: None,
            extraction_id: None,
            method: CitationBindingMethod::Human,
        })
        .await
        .unwrap();
    let conflict = fixture
        .service
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: fixture.case_id.clone(),
            citation_target_id: mappings[2].citation_target_id.clone(),
            source_id: competing_source.id.clone(),
            source_snapshot_id: None,
            extraction_id: None,
            method: CitationBindingMethod::Imported,
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

    let first = fixture
        .service
        .list_citation_target_bindings(mappings[0].citation_target_id.as_str())
        .await
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, preserved.id);
    let second = fixture
        .service
        .list_citation_target_bindings(mappings[1].citation_target_id.as_str())
        .await
        .unwrap();
    assert!(second.is_empty());
    let third = fixture
        .service
        .list_citation_target_bindings(mappings[2].citation_target_id.as_str())
        .await
        .unwrap();
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].id, conflict.id);
}

#[tokio::test]
async fn automatic_resolution_rolls_back_a_binding_insert_failure() {
    let fixture = fixture(
        "ref-atomic",
        ManuscriptCitationFormat::Zotero,
        2,
        Some((
            SourceKind::Other,
            "Reference atomic".to_owned(),
            Some(zotero_identity("ref-atomic")),
        )),
    )
    .await;
    let entries = fixture
        .service
        .list_manuscript_reference_entries(fixture.catalog.id.as_str())
        .await
        .unwrap();
    let mappings = mappings(&fixture).await;
    let source_id = fixture.reference_source.as_ref().unwrap().id.clone();
    let resolution_run = nineprofs_research::ManuscriptReferenceResolutionRun {
        id: nineprofs_research::ManuscriptReferenceResolutionRunId::new(),
        research_case_id: fixture.case_id.clone(),
        catalog_run_id: fixture.catalog.id.clone(),
        catalog_hash: fixture.catalog.catalog_hash.clone(),
        source_state_hash: fixture.catalog.catalog_hash.clone(),
        resolver_policy_version: "test".to_owned(),
        status: nineprofs_research::ManuscriptReferenceResolutionStatus::Completed,
        entry_count: 1,
        resolved_entry_count: 1,
        candidate_entry_count: 0,
        unresolved_entry_count: 0,
        conflict_entry_count: 0,
        created_at_ms: 0,
        completed_at_ms: Some(0),
        failure_code: None,
    };
    let resolution_entry = nineprofs_research::ManuscriptReferenceResolutionEntry {
        id: nineprofs_research::ManuscriptReferenceResolutionEntryId::new(),
        resolution_run_id: resolution_run.id.clone(),
        reference_entry_id: entries[0].id.clone(),
        outcome: ManuscriptReferenceResolutionOutcome::ResolvedExact,
        match_kind: Some(
            nineprofs_research::ManuscriptReferenceResolutionMatchKind::ExactZoteroItemId,
        ),
        chosen_source_id: Some(source_id.clone()),
        chosen_source_snapshot_id: None,
        chosen_extraction_id: None,
        automatic_binding_permitted: true,
        candidate_count: 1,
    };
    let resolution_entry_id = resolution_entry.id.clone();
    let resolution_candidate = nineprofs_research::ManuscriptReferenceResolutionCandidate {
        id: nineprofs_research::ManuscriptReferenceResolutionCandidateId::new(),
        resolution_entry_id: resolution_entry_id.clone(),
        ordinal: 0,
        source_id: source_id.clone(),
        source_snapshot_id: None,
        extraction_id: None,
        match_kind: nineprofs_research::ManuscriptReferenceResolutionMatchKind::ExactZoteroItemId,
        automatic_binding_permitted: true,
    };
    let valid_binding = nineprofs_research::CitationTargetBinding {
        id: nineprofs_research::CitationTargetBindingId::new(),
        research_case_id: fixture.case_id.clone(),
        citation_target_id: mappings[0].citation_target_id.clone(),
        source_id: source_id.clone(),
        source_snapshot_id: None,
        extraction_id: None,
        method: CitationBindingMethod::DeterministicResolver,
        created_at_ms: 0,
    };
    let duplicate_id_binding = nineprofs_research::CitationTargetBinding {
        citation_target_id: mappings[1].citation_target_id.clone(),
        ..valid_binding.clone()
    };
    let repository = SqliteResearchRepository::new(fixture.database.pool().clone());
    let result = repository
        .persist_manuscript_reference_resolution_with_bindings(
            &nineprofs_research::ManuscriptReferenceResolutionWrite {
                run: resolution_run.clone(),
                entries: vec![resolution_entry],
                candidates: vec![resolution_candidate],
            },
            &[valid_binding, duplicate_id_binding],
        )
        .await;
    assert!(result.is_err());

    for (table, column, id) in [
        (
            "research_manuscript_reference_resolution_runs",
            "id",
            resolution_run.id.as_str(),
        ),
        (
            "research_manuscript_reference_resolution_entries",
            "resolution_run_id",
            resolution_run.id.as_str(),
        ),
        (
            "research_manuscript_reference_resolution_candidates",
            "resolution_entry_id",
            resolution_entry_id.as_str(),
        ),
    ] {
        let query = format!("SELECT COUNT(*) AS count FROM {table} WHERE {column} = ?");
        let count = sqlx::query(&query)
            .bind(id)
            .fetch_one(fixture.database.pool())
            .await
            .unwrap()
            .get::<i64, _>("count");
        assert_eq!(count, 0, "{table} retained rolled-back state");
    }
    let binding_count = sqlx::query(
        "SELECT COUNT(*) AS count FROM research_citation_target_bindings \
         WHERE citation_target_id IN (?, ?)",
    )
    .bind(mappings[0].citation_target_id.as_str())
    .bind(mappings[1].citation_target_id.as_str())
    .fetch_one(fixture.database.pool())
    .await
    .unwrap()
    .get::<i64, _>("count");
    assert_eq!(binding_count, 0);
}

#[tokio::test]
async fn concurrent_equivalent_resolutions_converge_on_one_run_and_bindings() {
    let database_path = std::env::temp_dir().join(format!(
        "9profs-reference-resolution-concurrent-{}.sqlite",
        nineprofs_research::ResearchCaseId::new()
    ));
    let fixture = fixture_with_database(
        Database::open(&database_path).await.unwrap(),
        "ref-concurrent",
        ManuscriptCitationFormat::Zotero,
        2,
        Some((
            SourceKind::Other,
            "Reference concurrent".to_owned(),
            Some(zotero_identity("ref-concurrent")),
        )),
    )
    .await;
    let mappings = mappings(&fixture).await;
    let catalog_id = fixture.catalog.id.to_string();
    let left = fixture.service.clone();
    let right = fixture.service.clone();
    let (first, second) = tokio::join!(
        left.resolve_manuscript_references(&catalog_id),
        right.resolve_manuscript_references(&catalog_id),
    );
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.id, second.id);

    let run_count = sqlx::query(
        "SELECT COUNT(*) AS count FROM research_manuscript_reference_resolution_runs \
         WHERE catalog_run_id = ?",
    )
    .bind(fixture.catalog.id.as_str())
    .fetch_one(fixture.database.pool())
    .await
    .unwrap()
    .get::<i64, _>("count");
    assert_eq!(run_count, 1);
    for mapping in mappings {
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
    }

    drop(left);
    drop(right);
    fixture.database.close().await;
    drop(fixture);
    std::fs::remove_file(database_path).ok();
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
