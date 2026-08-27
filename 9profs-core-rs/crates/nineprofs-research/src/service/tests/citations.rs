use super::common::service;

use super::*;

#[tokio::test]
async fn citations_support_grouped_targets_many_to_many_links_and_unresolved_targets() {
    let (database, service) = service().await;
    let case = service
        .create_case(CreateResearchCase {
            title: "Citation review".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Web,
            label: "Reference".to_owned(),
        })
        .await
        .unwrap();
    let occurrence = service
        .create_citation_occurrence(CreateCitationOccurrence {
            research_case_id: case.id.clone(),
            origin: CitationOccurrenceOrigin::Manuscript {
                document_id: "document-1".to_owned(),
                document_version: "version-1".to_owned(),
                locator: Some(EvidenceLocator::Manuscript {
                    block_id: "paragraph-1".to_owned(),
                    start: Some(2),
                    end: Some(8),
                }),
            },
            rendered_text: "[12,13,14]".to_owned(),
        })
        .await
        .unwrap();
    let second_occurrence = service
        .create_citation_occurrence(CreateCitationOccurrence {
            research_case_id: case.id.clone(),
            origin: CitationOccurrenceOrigin::Imported {
                source: "fixture".to_owned(),
            },
            rendered_text: "[15]".to_owned(),
        })
        .await
        .unwrap();
    let mut targets = Vec::new();
    for (ordinal, reference_key) in ["12", "13", "14"].into_iter().enumerate() {
        targets.push(
            service
                .create_citation_target(CreateCitationTarget {
                    citation_occurrence_id: occurrence.id.clone(),
                    ordinal: ordinal as u32,
                    reference_key: reference_key.to_owned(),
                    cited_locator: (ordinal == 1).then(|| "p. 42".to_owned()),
                })
                .await
                .unwrap(),
        );
    }
    assert_eq!(
        targets
            .iter()
            .map(|target| target.reference_key.as_str())
            .collect::<Vec<_>>(),
        vec!["12", "13", "14"]
    );
    assert!(matches!(
        service
            .create_citation_target(CreateCitationTarget {
                citation_occurrence_id: occurrence.id.clone(),
                ordinal: 1,
                reference_key: "duplicate".to_owned(),
                cited_locator: None,
            })
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("ordinal already exists")
    ));
    assert_eq!(
        service
            .citation_target_resolution(targets[1].id.as_str())
            .await
            .unwrap(),
        crate::CitationTargetResolution::Unresolved
    );

    let binding = service
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: case.id.clone(),
            citation_target_id: targets[0].id.clone(),
            source_id: source.id,
            source_snapshot_id: None,
            extraction_id: None,
            method: crate::CitationBindingMethod::DeterministicResolver,
        })
        .await
        .unwrap();
    assert_eq!(
        binding.resolution(),
        crate::CitationTargetResolution::SourceBound
    );
    assert!(!binding.pdf_verification_ready());
    assert_eq!(
        service
            .citation_target_resolution(targets[0].id.as_str())
            .await
            .unwrap(),
        crate::CitationTargetResolution::SourceBound
    );
    assert!(
        service
            .list_citation_target_bindings(targets[1].id.as_str())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service
            .create_citation_target_binding(CreateCitationTargetBinding {
                research_case_id: case.id.clone(),
                citation_target_id: targets[0].id.clone(),
                source_id: binding.source_id.clone(),
                source_snapshot_id: None,
                extraction_id: None,
                method: crate::CitationBindingMethod::DeterministicResolver,
            })
            .await
            .unwrap()
            .id,
        binding.id
    );

    let claim_one = service
        .create_claim(CreateResearchClaim {
            research_case_id: case.id.clone(),
            text: "Claim one".to_owned(),
            origin: ClaimOrigin::User,
        })
        .await
        .unwrap();
    let claim_two = service
        .create_claim(CreateResearchClaim {
            research_case_id: case.id.clone(),
            text: "Claim two".to_owned(),
            origin: ClaimOrigin::User,
        })
        .await
        .unwrap();
    let link_one = service
        .create_claim_citation_link(CreateClaimCitationLink {
            research_case_id: case.id.clone(),
            claim_id: claim_one.id.clone(),
            citation_occurrence_id: occurrence.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        service
            .create_claim_citation_link(CreateClaimCitationLink {
                research_case_id: case.id.clone(),
                claim_id: claim_one.id.clone(),
                citation_occurrence_id: occurrence.id.clone(),
            })
            .await
            .unwrap()
            .id,
        link_one.id
    );
    service
        .create_claim_citation_link(CreateClaimCitationLink {
            research_case_id: case.id.clone(),
            claim_id: claim_one.id.clone(),
            citation_occurrence_id: second_occurrence.id.clone(),
        })
        .await
        .unwrap();
    service
        .create_claim_citation_link(CreateClaimCitationLink {
            research_case_id: case.id.clone(),
            claim_id: claim_two.id.clone(),
            citation_occurrence_id: occurrence.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        service
            .list_claim_citation_links(Some(case.id.as_str()), None, None)
            .await
            .unwrap()
            .len(),
        3
    );
    assert!(
        service
            .list_evidence(Some(case.id.as_str()), None)
            .await
            .unwrap()
            .is_empty()
    );

    let recreated = ResearchService::new(
        crate::SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(64)),
    );
    let persisted_targets = recreated
        .list_citation_targets(occurrence.id.as_str())
        .await
        .unwrap();
    assert_eq!(
        persisted_targets
            .iter()
            .map(|target| target.ordinal)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(
        recreated
            .list_claim_citation_links(None, Some(claim_one.id.as_str()), None)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn exact_pdf_bindings_pin_history_and_reject_cross_case_or_broken_chains() {
    let database = Database::in_memory().await.unwrap();
    let root = std::env::temp_dir().join(format!(
        "9profs-research-citation-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let store = Arc::new(crate::ResearchArtifactStore::new(
        root.clone(),
        database.pool().clone(),
    ));
    let service = ResearchService::new(
        crate::SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(64)),
    )
    .with_artifact_store(Arc::clone(&store));
    let mut upload = store.begin_upload("reference.pdf").unwrap();
    upload.append(b"%PDF-1.7\ncitation fixture").unwrap();
    let artifact = upload.finish().await.unwrap();
    let case = service
        .create_case(CreateResearchCase {
            title: "PDF citation review".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::ReferencePdf,
            label: "Reference PDF".to_owned(),
        })
        .await
        .unwrap();
    let snapshot = service
        .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
        .await
        .unwrap();
    let extraction_one = service
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id.clone(),
            extractor: "pdfjs".to_owned(),
            extractor_version: Some("1".to_owned()),
            page_count: 1,
            status: PdfExtractionStatus::Ready,
            pages: vec![crate::CapturePdfPage {
                page: 1,
                text: "First extraction".to_owned(),
            }],
        })
        .await
        .unwrap();
    let occurrence = service
        .create_citation_occurrence(CreateCitationOccurrence {
            research_case_id: case.id.clone(),
            origin: CitationOccurrenceOrigin::Imported {
                source: "fixture".to_owned(),
            },
            rendered_text: "[12]".to_owned(),
        })
        .await
        .unwrap();
    let target = service
        .create_citation_target(CreateCitationTarget {
            citation_occurrence_id: occurrence.id,
            ordinal: 0,
            reference_key: "12".to_owned(),
            cited_locator: Some("p. 42".to_owned()),
        })
        .await
        .unwrap();
    let binding_one = service
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: case.id.clone(),
            citation_target_id: target.id.clone(),
            source_id: source.id.clone(),
            source_snapshot_id: Some(snapshot.id.clone()),
            extraction_id: Some(extraction_one.id.clone()),
            method: crate::CitationBindingMethod::Human,
        })
        .await
        .unwrap();
    assert!(binding_one.pdf_verification_ready());

    let extraction_two = service
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id.clone(),
            extractor: "pdfjs".to_owned(),
            extractor_version: Some("2".to_owned()),
            page_count: 1,
            status: PdfExtractionStatus::Ready,
            pages: vec![crate::CapturePdfPage {
                page: 1,
                text: "Second extraction".to_owned(),
            }],
        })
        .await
        .unwrap();
    let binding_two = service
        .create_citation_target_binding(CreateCitationTargetBinding {
            research_case_id: case.id.clone(),
            citation_target_id: target.id.clone(),
            source_id: source.id.clone(),
            source_snapshot_id: Some(snapshot.id.clone()),
            extraction_id: Some(extraction_two.id.clone()),
            method: crate::CitationBindingMethod::Imported,
        })
        .await
        .unwrap();
    assert_ne!(binding_one.id, binding_two.id);
    assert_eq!(
        service
            .get_citation_target_binding(binding_one.id.as_str())
            .await
            .unwrap()
            .extraction_id,
        Some(extraction_one.id.clone())
    );
    assert_eq!(
        service
            .latest_citation_target_binding(target.id.as_str())
            .await
            .unwrap()
            .id,
        binding_two.id
    );

    let other_source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::ReferencePdf,
            label: "Other reference PDF".to_owned(),
        })
        .await
        .unwrap();
    let other_snapshot = service
        .capture_verified_artifact_snapshot(other_source.id.clone(), &artifact, BTreeMap::new())
        .await
        .unwrap();
    let other_extraction = service
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: other_snapshot.id.clone(),
            extractor: "pdfjs".to_owned(),
            extractor_version: Some("other".to_owned()),
            page_count: 1,
            status: PdfExtractionStatus::Ready,
            pages: vec![crate::CapturePdfPage {
                page: 1,
                text: "Other extraction".to_owned(),
            }],
        })
        .await
        .unwrap();
    assert!(matches!(
        service
            .create_citation_target_binding(CreateCitationTargetBinding {
                research_case_id: case.id.clone(),
                citation_target_id: target.id.clone(),
                source_id: source.id.clone(),
                source_snapshot_id: Some(snapshot.id.clone()),
                extraction_id: Some(other_extraction.id),
                method: crate::CitationBindingMethod::DeterministicResolver,
            })
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("does not belong to source snapshot")
    ));

    let other_case = service
        .create_case(CreateResearchCase {
            title: "Other case".to_owned(),
        })
        .await
        .unwrap();
    let other_claim = service
        .create_claim(CreateResearchClaim {
            research_case_id: other_case.id.clone(),
            text: "Other claim".to_owned(),
            origin: ClaimOrigin::User,
        })
        .await
        .unwrap();
    assert!(matches!(
        service
            .create_citation_target_binding(CreateCitationTargetBinding {
                research_case_id: other_case.id.clone(),
                citation_target_id: target.id.clone(),
                source_id: source.id.clone(),
                source_snapshot_id: Some(snapshot.id.clone()),
                extraction_id: Some(extraction_one.id.clone()),
                method: crate::CitationBindingMethod::Agent,
            })
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("same research case")
    ));
    assert!(matches!(
        service
            .create_claim_citation_link(CreateClaimCitationLink {
                research_case_id: other_case.id,
                claim_id: other_claim.id,
                citation_occurrence_id: target.citation_occurrence_id,
            })
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("same research case")
    ));
    std::fs::remove_dir_all(root).unwrap();
}
