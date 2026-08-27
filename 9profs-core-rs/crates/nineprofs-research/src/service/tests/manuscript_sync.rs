use super::common::service;

use super::*;

#[tokio::test]
async fn manuscript_citation_sync_is_idempotent_versioned_and_transactional() {
    let (_database, service) = service().await;
    let case = service
        .create_case(CreateResearchCase {
            title: "Manuscript review".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Draft".to_owned(),
        })
        .await
        .unwrap();

    let input = crate::SyncManuscriptCitations {
        research_case_id: case.id.clone(),
        manuscript_source_id: source.id.clone(),
        document_id: "doc-1".to_owned(),
        document_version: 1,
        citations: vec![crate::ManuscriptCitationSyncCitationInput {
            format: crate::ManuscriptCitationFormat::Zotero,
            rendered_text: "[12,13]".to_owned(),
            block_id: "b7".to_owned(),
            start: 13,
            end: 20,
            targets: vec![
                crate::ManuscriptCitationSyncTargetInput {
                    ordinal: 1,
                    reference_key: "12".to_owned(),
                    cited_locator: None,
                },
                crate::ManuscriptCitationSyncTargetInput {
                    ordinal: 2,
                    reference_key: "13".to_owned(),
                    cited_locator: Some("table:0:cell:1:2".to_owned()),
                },
            ],
        }],
    };
    let first = service
        .sync_manuscript_citations(input.clone())
        .await
        .unwrap();
    assert_eq!(first.status, crate::ManuscriptCitationSyncStatus::Completed);
    assert_eq!(first.occurrence_count, 1);
    assert_eq!(first.document_version, 1);

    let repeated = service
        .sync_manuscript_citations(input.clone())
        .await
        .unwrap();
    assert_eq!(repeated.id, first.id);
    assert_eq!(
        service
            .list_manuscript_citation_sync_occurrences(first.id.as_str())
            .await
            .unwrap()
            .len(),
        1
    );
    let sync_occurrence = service
        .list_manuscript_citation_sync_occurrences(first.id.as_str())
        .await
        .unwrap()
        .pop()
        .unwrap();
    let sync_targets = service
        .list_manuscript_citation_sync_targets(sync_occurrence.id.as_str())
        .await
        .unwrap();
    assert_eq!(
        sync_targets
            .iter()
            .map(|target| target.document_target_ordinal)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    let occurrence = service
        .get_citation_occurrence(sync_occurrence.citation_occurrence_id.as_str())
        .await
        .unwrap();
    assert_eq!(occurrence.rendered_text, "[12,13]");
    assert!(matches!(
        occurrence.origin,
        CitationOccurrenceOrigin::Manuscript { document_version, .. }
            if document_version == "1"
    ));
    assert_eq!(
        service
            .list_citation_targets(occurrence.id.as_str())
            .await
            .unwrap()
            .into_iter()
            .map(|target| target.reference_key)
            .collect::<Vec<_>>(),
        vec!["12", "13"]
    );

    let mut changed = input.clone();
    changed.citations[0].rendered_text = "[12]".to_owned();
    assert!(matches!(
        service.sync_manuscript_citations(changed).await,
        Err(ResearchError::ManuscriptCitationSyncConflict { .. })
    ));

    let mut next_version = input;
    next_version.document_version = 2;
    next_version.citations.clear();
    let second = service
        .sync_manuscript_citations(next_version)
        .await
        .unwrap();
    assert_ne!(second.id, first.id);
    assert_eq!(second.occurrence_count, 0);
    assert_eq!(
        service
            .latest_manuscript_citation_sync(case.id.as_str(), source.id.as_str())
            .await
            .unwrap()
            .id,
        second.id
    );
    assert_eq!(
        service
            .list_citation_occurrences(Some(case.id.as_str()))
            .await
            .unwrap()
            .len(),
        1
    );

    let invalid = crate::SyncManuscriptCitations {
        research_case_id: case.id.clone(),
        manuscript_source_id: source.id.clone(),
        document_id: "doc-invalid".to_owned(),
        document_version: 1,
        citations: vec![crate::ManuscriptCitationSyncCitationInput {
            format: crate::ManuscriptCitationFormat::WordNative,
            rendered_text: "[1]".to_owned(),
            block_id: "b8".to_owned(),
            start: 4,
            end: 4,
            targets: Vec::new(),
        }],
    };
    assert!(matches!(
        service.sync_manuscript_citations(invalid).await,
        Err(ResearchError::Invalid(message)) if message.contains("start < end")
    ));
    assert!(
        service
            .latest_manuscript_citation_sync(case.id.as_str(), source.id.as_str())
            .await
            .unwrap()
            .id
            == second.id
    );
}
