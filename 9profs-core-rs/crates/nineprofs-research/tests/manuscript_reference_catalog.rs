use std::sync::Arc;

use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CreateResearchCase, CreateResearchSource, ManuscriptCitationFormat,
    ManuscriptCitationSyncCitationInput, ManuscriptCitationSyncTargetInput,
    ManuscriptReferenceCatalogCitationInput, ManuscriptReferenceCatalogTargetInput,
    ManuscriptReferenceCatalogWordSourceInput, ManuscriptReferenceCatalogZoteroInput,
    ResearchError, ResearchService, SourceKind, SqliteResearchRepository, SyncManuscriptCitations,
    SyncManuscriptReferenceCatalog,
};

async fn fixture(
    citations: Vec<ManuscriptCitationSyncCitationInput>,
) -> (
    Database,
    ResearchService,
    nineprofs_research::ManuscriptCitationSyncRun,
) {
    let database = Database::in_memory().await.unwrap();
    let service = ResearchService::new(
        SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(32)),
    );
    let case = service
        .create_case(CreateResearchCase {
            title: "Reference catalog".to_owned(),
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
    let run = service
        .sync_manuscript_citations(SyncManuscriptCitations {
            research_case_id: case.id,
            manuscript_source_id: source.id,
            document_id: "doc-1".to_owned(),
            document_version: 1,
            citations,
        })
        .await
        .unwrap();
    (database, service, run)
}

fn word_citation(key: &str, ordinal: u32) -> ManuscriptCitationSyncCitationInput {
    ManuscriptCitationSyncCitationInput {
        format: ManuscriptCitationFormat::WordNative,
        rendered_text: format!("[{key}]"),
        block_id: format!("b{ordinal}"),
        start: 1,
        end: 2,
        targets: vec![ManuscriptCitationSyncTargetInput {
            ordinal: 0,
            reference_key: key.to_owned(),
            cited_locator: None,
        }],
    }
}

async fn catalog_input(
    service: &ResearchService,
    run: &nineprofs_research::ManuscriptCitationSyncRun,
) -> SyncManuscriptReferenceCatalog {
    let occurrences = service
        .list_manuscript_citation_sync_occurrences(run.id.as_str())
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
        citations.push(ManuscriptReferenceCatalogCitationInput {
            citation_occurrence_id: occurrence.citation_occurrence_id.to_string(),
            block_id: occurrence.document_block_id.clone(),
            start: occurrence.start,
            end: occurrence.end,
            format: occurrence.format.clone(),
            targets: targets
                .into_iter()
                .zip(sync_targets)
                .map(
                    |(target, sync_target)| ManuscriptReferenceCatalogTargetInput {
                        citation_target_id: sync_target.citation_target_id.to_string(),
                        ordinal: target.ordinal,
                        reference_key: target.reference_key.clone(),
                        word_source: matches!(
                            occurrence.format,
                            ManuscriptCitationFormat::WordNative
                        )
                        .then(|| {
                            ManuscriptReferenceCatalogWordSourceInput {
                                tag: target.reference_key.clone(),
                                title: "Safe title".to_owned(),
                                author: "Safe author".to_owned(),
                                year: "2020".to_owned(),
                            }
                        }),
                        zotero: matches!(occurrence.format, ManuscriptCitationFormat::Zotero).then(
                            || ManuscriptReferenceCatalogZoteroInput {
                                item_id: Some(format!("item-{}", target.reference_key)),
                                uris: vec![format!(
                                    "zotero://select/items/{}",
                                    target.reference_key
                                )],
                            },
                        ),
                    },
                )
                .collect(),
        });
    }
    SyncManuscriptReferenceCatalog {
        citation_sync_run_id: run.id.clone(),
        document_id: run.document_id.clone(),
        document_version: run.document_version,
        citations,
    }
}

#[tokio::test]
async fn word_hints_and_exact_target_mapping_persist_without_bindings() {
    let (_database, service, run) = fixture(vec![word_citation("Smith2020", 7)]).await;
    let before_bindings = service
        .list_citation_target_bindings("missing-target")
        .await
        .unwrap_or_default()
        .len();
    let input = catalog_input(&service, &run).await;
    let catalog = service
        .sync_manuscript_reference_catalog(input)
        .await
        .unwrap();
    let entries = service
        .list_manuscript_reference_entries(catalog.id.as_str())
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].format, ManuscriptCitationFormat::WordNative);
    assert_eq!(entries[0].word_title.as_deref(), Some("Safe title"));
    assert_eq!(entries[0].target_count, 1);
    assert_eq!(
        service
            .list_manuscript_reference_target_mappings(entries[0].id.as_str())
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        service
            .list_citation_target_bindings("missing-target")
            .await
            .unwrap_or_default()
            .len(),
        before_bindings
    );
}

#[tokio::test]
async fn repeated_and_grouped_references_deduplicate_by_format_and_key() {
    let (_database, service, run) = fixture(vec![
        word_citation("Smith2020", 1),
        word_citation("Smith2020", 2),
        word_citation("Smith2020", 3),
        word_citation("Smith2020", 4),
        word_citation("Smith2020", 5),
        ManuscriptCitationSyncCitationInput {
            format: ManuscriptCitationFormat::Zotero,
            rendered_text: "[12,13,14]".to_owned(),
            block_id: "b6".to_owned(),
            start: 1,
            end: 2,
            targets: vec![
                ManuscriptCitationSyncTargetInput {
                    ordinal: 0,
                    reference_key: "12".to_owned(),
                    cited_locator: None,
                },
                ManuscriptCitationSyncTargetInput {
                    ordinal: 1,
                    reference_key: "13".to_owned(),
                    cited_locator: None,
                },
                ManuscriptCitationSyncTargetInput {
                    ordinal: 2,
                    reference_key: "14".to_owned(),
                    cited_locator: None,
                },
            ],
        },
    ])
    .await;
    let catalog = service
        .sync_manuscript_reference_catalog(catalog_input(&service, &run).await)
        .await
        .unwrap();
    let entries = service
        .list_manuscript_reference_entries(catalog.id.as_str())
        .await
        .unwrap();
    assert_eq!(entries.len(), 4);
    assert_eq!(catalog.target_mapping_count, 8);
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.reference_key == "Smith2020")
            .unwrap()
            .target_count,
        5
    );
}

#[tokio::test]
async fn word_reference_without_source_metadata_is_persisted_without_guesses() {
    let (_database, service, run) = fixture(vec![word_citation("Smith2020", 1)]).await;
    let mut input = catalog_input(&service, &run).await;
    input.citations[0].targets[0].word_source = None;
    let catalog = service
        .sync_manuscript_reference_catalog(input)
        .await
        .unwrap();
    let entry = service
        .list_manuscript_reference_entries(catalog.id.as_str())
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(entry.reference_key, "Smith2020");
    assert!(entry.word_tag.is_none());
    assert!(entry.word_title.is_none());
    assert!(entry.word_author.is_none());
    assert!(entry.word_year.is_none());
}

#[tokio::test]
async fn word_and_zotero_key_collision_stays_distinct_and_zotero_hints_survive() {
    let (_database, service, run) = fixture(vec![
        word_citation("12", 1),
        ManuscriptCitationSyncCitationInput {
            format: ManuscriptCitationFormat::Zotero,
            rendered_text: "[12]".to_owned(),
            block_id: "b2".to_owned(),
            start: 1,
            end: 2,
            targets: vec![ManuscriptCitationSyncTargetInput {
                ordinal: 0,
                reference_key: "12".to_owned(),
                cited_locator: None,
            }],
        },
    ])
    .await;
    let catalog = service
        .sync_manuscript_reference_catalog(catalog_input(&service, &run).await)
        .await
        .unwrap();
    let entries = service
        .list_manuscript_reference_entries(catalog.id.as_str())
        .await
        .unwrap();
    assert_eq!(entries.len(), 2);
    let zotero = entries
        .iter()
        .find(|entry| entry.format == ManuscriptCitationFormat::Zotero)
        .unwrap();
    assert_eq!(zotero.zotero_item_id.as_deref(), Some("item-12"));
    assert_eq!(zotero.zotero_uris.len(), 1);
}

#[tokio::test]
async fn unsafe_zotero_uri_hints_are_rejected_without_persisting_a_catalog() {
    let (_database, service, run) = fixture(vec![ManuscriptCitationSyncCitationInput {
        format: ManuscriptCitationFormat::Zotero,
        rendered_text: "[12]".to_owned(),
        block_id: "b1".to_owned(),
        start: 1,
        end: 2,
        targets: vec![ManuscriptCitationSyncTargetInput {
            ordinal: 0,
            reference_key: "12".to_owned(),
            cited_locator: None,
        }],
    }])
    .await;
    let mut host_path = catalog_input(&service, &run).await;
    host_path.citations[0].targets[0]
        .zotero
        .as_mut()
        .unwrap()
        .uris[0] = "file:///tmp/private.pdf".to_owned();
    assert!(matches!(
        service.sync_manuscript_reference_catalog(host_path).await,
        Err(ResearchError::Invalid(message)) if message.contains("host path")
    ));

    let mut credentials = catalog_input(&service, &run).await;
    credentials.citations[0].targets[0]
        .zotero
        .as_mut()
        .unwrap()
        .uris[0] = "https://user:password@example.test/item".to_owned();
    assert!(matches!(
        service.sync_manuscript_reference_catalog(credentials).await,
        Err(ResearchError::Invalid(message)) if message.contains("credentials")
    ));
    assert!(matches!(
        service
            .manuscript_reference_catalog_for_sync(run.id.as_str())
            .await,
        Err(ResearchError::NotFound { .. })
    ));
}

#[tokio::test]
async fn descriptor_conflict_and_wrong_target_are_rejected_without_catalog() {
    let (_database, service, run) = fixture(vec![
        word_citation("Smith2020", 1),
        word_citation("Smith2020", 2),
    ])
    .await;
    let mut input = catalog_input(&service, &run).await;
    input.citations[1].targets[0]
        .word_source
        .as_mut()
        .unwrap()
        .year = "2021".to_owned();
    assert!(matches!(
        service.sync_manuscript_reference_catalog(input).await,
        Err(ResearchError::ManuscriptReferenceDescriptorConflict { .. })
    ));
    let mut wrong_target = catalog_input(&service, &run).await;
    let wrong_target_id = wrong_target.citations[1].targets[0]
        .citation_target_id
        .clone();
    wrong_target.citations[0].targets[0].citation_target_id = wrong_target_id;
    assert!(matches!(
        service.sync_manuscript_reference_catalog(wrong_target).await,
        Err(ResearchError::Invalid(message)) if message.contains("does not match sync target")
    ));
    assert!(matches!(
        service
            .manuscript_reference_catalog_for_sync(run.id.as_str())
            .await,
        Err(ResearchError::NotFound { .. })
    ));
}

#[tokio::test]
async fn catalog_is_idempotent_and_stale_or_key_mismatched_inputs_fail() {
    let (_database, service, run) = fixture(vec![word_citation("Smith2020", 1)]).await;
    let input = catalog_input(&service, &run).await;
    let first = service
        .sync_manuscript_reference_catalog(input.clone())
        .await
        .unwrap();
    let second = service
        .sync_manuscript_reference_catalog(input.clone())
        .await
        .unwrap();
    assert_eq!(first.id, second.id);

    let mut changed_catalog = input.clone();
    changed_catalog.citations[0].targets[0]
        .word_source
        .as_mut()
        .unwrap()
        .title = "Different title".to_owned();
    assert!(matches!(
        service
            .sync_manuscript_reference_catalog(changed_catalog)
            .await,
        Err(ResearchError::ManuscriptReferenceCatalogConflict { .. })
    ));

    let mut stale = input.clone();
    stale.document_version = 2;
    assert!(matches!(
        service.sync_manuscript_reference_catalog(stale).await,
        Err(ResearchError::ManuscriptReferenceCatalogStale)
    ));

    let mut key_mismatch = input;
    key_mismatch.citations[0].targets[0].reference_key = "Jones2021".to_owned();
    assert!(matches!(
        service
            .sync_manuscript_reference_catalog(key_mismatch)
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("does not match sync target")
    ));
}
