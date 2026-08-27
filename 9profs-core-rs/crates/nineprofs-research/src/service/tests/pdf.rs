use super::*;

#[tokio::test]
async fn streamed_pdf_artifact_snapshot_extraction_and_exact_unicode_evidence_are_anchored() {
    let database = Database::in_memory().await.unwrap();
    let root = std::env::temp_dir().join(format!("9profs-research-pdf-{}", now_ms()));
    let store = Arc::new(crate::ResearchArtifactStore::new(
        root.clone(),
        database.pool().clone(),
    ));
    let service = ResearchService::new(
        crate::SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(64)),
    )
    .with_artifact_store(Arc::clone(&store));

    let mut upload = store
        .begin_upload(r"C:\Users\person\reference.pdf")
        .unwrap();
    upload.append(b"%PDF-1.7\nfixture bytes").unwrap();
    let artifact = upload.finish().await.unwrap();
    assert_eq!(artifact.artifact().media_type, "application/pdf");
    assert_eq!(artifact.artifact().size_bytes, 22);
    let stored_path = root.join(format!("{}.pdf", artifact.content_hash().value));
    assert_eq!(
        std::fs::read(stored_path).unwrap(),
        b"%PDF-1.7\nfixture bytes"
    );

    let mut duplicate = store.begin_upload("duplicate.pdf").unwrap();
    duplicate.append(b"%PDF-1.7\nfixture bytes").unwrap();
    let duplicate = duplicate.finish().await.unwrap();
    assert_eq!(duplicate.artifact_id(), artifact.artifact_id());

    let mut revised = store.begin_upload("revised.pdf").unwrap();
    revised.append(b"%PDF-1.7\nrevised bytes").unwrap();
    let revised = revised.finish().await.unwrap();
    assert_ne!(revised.artifact_id(), artifact.artifact_id());
    assert_eq!(
        std::fs::read(root.join(format!("{}.pdf", artifact.content_hash().value))).unwrap(),
        b"%PDF-1.7\nfixture bytes"
    );

    let temp_upload_count = || {
        std::fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".upload-"))
            .count()
    };
    let temp_uploads_before = temp_upload_count();
    let mut invalid_upload = store.begin_upload("invalid.pdf").unwrap();
    invalid_upload.append(b"not a PDF").unwrap();
    assert!(invalid_upload.finish().await.is_err());
    assert_eq!(temp_upload_count(), temp_uploads_before);

    let case = service
        .create_case(CreateResearchCase {
            title: "PDF evidence".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::ReferencePdf,
            label: "Reference".to_owned(),
        })
        .await
        .unwrap();
    let snapshot = service
        .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(snapshot.content_hash, *artifact.content_hash());

    let page_text = "Điều trị giảm tử vong 😀 20%.";
    let extraction = service
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id.clone(),
            extractor: "pdfjs".to_owned(),
            extractor_version: Some("test".to_owned()),
            page_count: 2,
            status: PdfExtractionStatus::Ready,
            pages: vec![
                crate::CapturePdfPage {
                    page: 1,
                    text: page_text.to_owned(),
                },
                crate::CapturePdfPage {
                    page: 2,
                    text: "Second page".to_owned(),
                },
            ],
        })
        .await
        .unwrap();
    let start_byte = page_text.find("giảm tử vong").unwrap();
    let start = page_text[..start_byte].chars().count() as u64;
    let end = start + "giảm tử vong".chars().count() as u64;
    let evidence = service
        .capture_pdf_evidence(CapturePdfEvidence {
            research_case_id: case.id.clone(),
            source_snapshot_id: snapshot.id.clone(),
            extraction_id: extraction.id.clone(),
            page: 1,
            start,
            end,
        })
        .await
        .unwrap();
    assert_eq!(evidence.verbatim_excerpt, "giảm tử vong");
    assert_eq!(evidence.pdf_extraction_id, Some(extraction.id.clone()));
    assert_eq!(
        service.list_evidence(None, None).await.unwrap()[0].pdf_extraction_id,
        Some(extraction.id.clone())
    );
    assert!(matches!(
        service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case.id,
                source_snapshot_id: snapshot.id,
                verbatim_excerpt: "eliminated mortality".to_owned(),
                normalized_text: None,
                locator: EvidenceLocator::PdfTextRange { page: 1, start, end },
                capture_method: CaptureMethod::UploadedArtifact,
            })
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("stored page range")
    ));
    std::fs::write(
        root.join(format!("{}.pdf", artifact.content_hash().value)),
        b"%PDF-1.7\ntampered bytes",
    )
    .unwrap();
    assert!(matches!(
        store.get(artifact.artifact_id()).await,
        Err(ResearchError::Artifact(message)) if message.contains("do not match metadata")
    ));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn pdf_extraction_access_is_exact_ordered_paginated_and_ready_only() {
    let database = Database::in_memory().await.unwrap();
    let root = std::env::temp_dir().join(format!(
        "9profs-research-pdf-access-{}-{}",
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
    let mut upload = store.begin_upload("access.pdf").unwrap();
    upload.append(b"%PDF-1.7\naccess fixture").unwrap();
    let artifact = upload.finish().await.unwrap();
    let case = service
        .create_case(CreateResearchCase {
            title: "PDF access".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id,
            kind: SourceKind::ReferencePdf,
            label: "Access fixture".to_owned(),
        })
        .await
        .unwrap();
    let snapshot = service
        .capture_verified_artifact_snapshot(source.id, &artifact, BTreeMap::new())
        .await
        .unwrap();

    let no_text = service
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id.clone(),
            extractor: "pdfjs".to_owned(),
            extractor_version: Some("no-text".to_owned()),
            page_count: 1,
            status: PdfExtractionStatus::NoExtractableText,
            pages: vec![crate::CapturePdfPage {
                page: 1,
                text: String::new(),
            }],
        })
        .await
        .unwrap();
    assert!(matches!(
        service
            .require_ready_pdf_extraction(no_text.id.as_str())
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("not ready")
    ));

    let pages = |prefix: &str| {
        (1..=120)
            .map(|page| crate::CapturePdfPage {
                page,
                text: format!("{prefix} page {page}"),
            })
            .collect::<Vec<_>>()
    };
    let extraction_one = service
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id.clone(),
            extractor: "pdfjs".to_owned(),
            extractor_version: Some("1".to_owned()),
            page_count: 120,
            status: PdfExtractionStatus::Ready,
            pages: pages("revision-one"),
        })
        .await
        .unwrap();
    while now_ms() <= extraction_one.extracted_at_ms {
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    let extraction_two = service
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id.clone(),
            extractor: "pdfjs".to_owned(),
            extractor_version: Some("2".to_owned()),
            page_count: 120,
            status: PdfExtractionStatus::Ready,
            pages: pages("revision-two"),
        })
        .await
        .unwrap();
    assert!(extraction_two.extracted_at_ms > extraction_one.extracted_at_ms);
    assert_ne!(
        extraction_one.extraction_hash,
        extraction_two.extraction_hash
    );

    let listed = service
        .list_pdf_extractions(snapshot.id.as_str())
        .await
        .unwrap();
    assert_eq!(listed.len(), 3);
    assert!(listed.windows(2).all(|pair| {
        (pair[0].extracted_at_ms, &pair[0].id) <= (pair[1].extracted_at_ms, &pair[1].id)
    }));
    assert!(listed.iter().any(|value| value.id == no_text.id));
    assert!(listed.iter().any(|value| value.id == extraction_one.id));
    assert!(listed.iter().any(|value| value.id == extraction_two.id));
    assert_eq!(
        service
            .get_pdf_extraction_by_id(extraction_one.id.as_str())
            .await
            .unwrap(),
        extraction_one
    );
    assert_eq!(
        service
            .get_pdf_extraction_by_id(extraction_two.id.as_str())
            .await
            .unwrap(),
        extraction_two
    );
    assert_eq!(
        service
            .latest_pdf_extraction(snapshot.id.as_str())
            .await
            .unwrap()
            .id,
        extraction_two.id
    );
    assert!(matches!(
        service
            .get_pdf_extraction_for_snapshot(
                &extraction_one.id,
                &ResearchSourceSnapshotId::new()
            )
            .await,
        Err(ResearchError::Invalid(message))
            if message.contains("does not belong to source snapshot")
    ));

    let first = service
        .list_pdf_pages(extraction_one.id.as_str(), 1, 500)
        .await
        .unwrap();
    assert_eq!(
        first.pages.iter().map(|page| page.page).collect::<Vec<_>>(),
        (1..=50).collect::<Vec<_>>()
    );
    assert_eq!(first.start_page, 1);
    assert_eq!(first.limit, 50);
    assert!(first.has_more);
    assert_eq!(first.next_start_page, Some(51));
    assert!(
        first
            .pages
            .iter()
            .all(|page| page.extraction_id == extraction_one.id)
    );

    let middle = service
        .list_pdf_pages(extraction_one.id.as_str(), 51, 50)
        .await
        .unwrap();
    assert_eq!(
        middle
            .pages
            .iter()
            .map(|page| page.page)
            .collect::<Vec<_>>(),
        (51..=100).collect::<Vec<_>>()
    );
    assert_eq!(middle.next_start_page, Some(101));
    assert!(
        middle
            .pages
            .iter()
            .all(|page| page.extraction_id == extraction_one.id)
    );

    let last = service
        .list_pdf_pages(extraction_one.id.as_str(), 101, 50)
        .await
        .unwrap();
    assert_eq!(
        last.pages.iter().map(|page| page.page).collect::<Vec<_>>(),
        (101..=120).collect::<Vec<_>>()
    );
    assert!(!last.has_more);
    assert_eq!(last.next_start_page, None);
    assert!(
        last.pages
            .iter()
            .all(|page| page.extraction_id == extraction_one.id)
    );

    let all = service
        .list_all_pdf_pages_for_indexing(extraction_two.id.as_str())
        .await
        .unwrap();
    assert_eq!(all.len(), 120);
    assert_eq!(
        all.iter().map(|page| page.page).collect::<Vec<_>>(),
        (1..=120).collect::<Vec<_>>()
    );
    assert!(all.iter().all(|page| {
        page.extraction_id == extraction_two.id && page.text.starts_with("revision-two")
    }));
    assert_eq!(all[0].text_hash, sha256_hash(all[0].text.as_bytes()));
    std::fs::remove_dir_all(root).unwrap();
}
