use super::common::{service, snapshot_input};

use super::*;

#[tokio::test]
async fn case_source_snapshot_and_evidence_preserve_provenance() {
    let (_database, service) = service().await;
    let case = service
        .create_case(CreateResearchCase {
            title: "Review".to_owned(),
        })
        .await
        .unwrap();
    assert!(!case.id.to_string().is_empty());
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Draft".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let first = service
        .capture_snapshot(snapshot_input(source.id.clone(), b"version one"))
        .await
        .unwrap();
    let evidence = service
        .create_evidence(CreateResearchEvidence {
            research_case_id: case.id.clone(),
            source_snapshot_id: first.id.clone(),
            verbatim_excerpt: "exact words".to_owned(),
            normalized_text: Some("normalized words".to_owned()),
            locator: EvidenceLocator::TextRange { start: 2, end: 13 },
            capture_method: CaptureMethod::ActiveDocument,
        })
        .await
        .unwrap();
    let second = service
        .capture_snapshot(snapshot_input(source.id.clone(), b"version two"))
        .await
        .unwrap();

    assert_ne!(first.content_hash.value, second.content_hash.value);
    assert_eq!(evidence.source_snapshot_id, first.id);
    assert_eq!(
        service
            .get_snapshot(&evidence.source_snapshot_id.to_string())
            .await
            .unwrap()
            .content_hash
            .value,
        first.content_hash.value
    );
}

#[tokio::test]
async fn same_source_duplicate_snapshot_returns_existing_and_other_sources_stay_distinct() {
    let (_database, service) = service().await;
    let case = service
        .create_case(CreateResearchCase {
            title: "Review".to_owned(),
        })
        .await
        .unwrap();
    let case_id = case.id.clone();
    let first_source = service
        .create_source(CreateResearchSource {
            research_case_id: case_id.clone(),
            kind: SourceKind::Dataset,
            label: "First".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let second_source = service
        .create_source(CreateResearchSource {
            research_case_id: case_id.clone(),
            kind: SourceKind::Dataset,
            label: "Second".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let one = service
        .capture_snapshot(snapshot_input(first_source.id.clone(), b"same"))
        .await
        .unwrap();
    let duplicate = service
        .capture_snapshot(snapshot_input(first_source.id, b"same"))
        .await
        .unwrap();
    let other_source = service
        .capture_snapshot(snapshot_input(second_source.id, b"same"))
        .await
        .unwrap();
    assert_eq!(one.id, duplicate.id);
    assert_ne!(one.id, other_source.id);
    let first_evidence = service
        .create_evidence(CreateResearchEvidence {
            research_case_id: case_id.clone(),
            source_snapshot_id: one.id,
            verbatim_excerpt: "same words".to_owned(),
            normalized_text: None,
            locator: EvidenceLocator::TextRange { start: 0, end: 10 },
            capture_method: CaptureMethod::UploadedArtifact,
        })
        .await
        .unwrap();
    let second_evidence = service
        .create_evidence(CreateResearchEvidence {
            research_case_id: case_id,
            source_snapshot_id: other_source.id,
            verbatim_excerpt: "same words".to_owned(),
            normalized_text: None,
            locator: EvidenceLocator::TextRange { start: 0, end: 10 },
            capture_method: CaptureMethod::UploadedArtifact,
        })
        .await
        .unwrap();
    assert_eq!(first_evidence.excerpt_hash, second_evidence.excerpt_hash);
    assert_ne!(first_evidence.id, second_evidence.id);
}

#[tokio::test]
async fn persistence_round_trip_survives_service_recreation() {
    let database = Database::in_memory().await.unwrap();
    let events = Arc::new(BroadcastEventBus::new(64));
    let first_service = ResearchService::new(
        crate::SqliteResearchRepository::new(database.pool().clone()),
        Arc::clone(&events),
    );
    let case = first_service
        .create_case(CreateResearchCase {
            title: "Persistent".to_owned(),
        })
        .await
        .unwrap();
    let source = first_service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Manuscript,
            label: "Reference".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let snapshot = first_service
        .capture_snapshot(snapshot_input(source.id, b"captured"))
        .await
        .unwrap();
    let evidence = first_service
        .create_evidence(CreateResearchEvidence {
            research_case_id: case.id.clone(),
            source_snapshot_id: snapshot.id,
            verbatim_excerpt: "verbatim".to_owned(),
            normalized_text: None,
            locator: EvidenceLocator::Pdf {
                page: 4,
                end_page: Some(5),
            },
            capture_method: CaptureMethod::UploadedArtifact,
        })
        .await
        .unwrap();
    let claim = first_service
        .create_claim(CreateResearchClaim {
            research_case_id: case.id.clone(),
            text: "claim".to_owned(),
            origin: ClaimOrigin::Imported {
                source: "fixture".to_owned(),
            },
        })
        .await
        .unwrap();
    let link = first_service
        .create_link(CreateClaimEvidenceLink {
            research_case_id: case.id.clone(),
            claim_id: claim.id,
            evidence_id: evidence.id,
            relation: ClaimEvidenceRelation::Contextualizes,
            rationale: None,
            assessment_method: AssessmentMethod::DeterministicChecker,
            assessment_metadata: BTreeMap::from([("score".to_owned(), "0.5".to_owned())]),
        })
        .await
        .unwrap();

    let restarted = ResearchService::new(
        crate::SqliteResearchRepository::new(database.pool().clone()),
        events,
    );
    assert_eq!(restarted.get_case(case.id.as_str()).await.unwrap(), case);
    assert_eq!(
        restarted
            .list_evidence(Some(case.id.as_str()), None)
            .await
            .unwrap()[0]
            .excerpt_hash,
        evidence.excerpt_hash
    );
    assert_eq!(restarted.get_link(link.id.as_str()).await.unwrap(), link);
}

#[tokio::test]
async fn foreign_references_and_secret_metadata_are_rejected() {
    let (_database, service) = service().await;
    let case = service
        .create_case(CreateResearchCase {
            title: "Review".to_owned(),
        })
        .await
        .unwrap();
    let case_id = case.id.clone();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case_id.clone(),
            kind: SourceKind::Other,
            label: "Source".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    assert!(matches!(
        service
            .create_source(CreateResearchSource {
                research_case_id: ResearchCaseId::parse("missing-case").unwrap(),
                kind: SourceKind::Other,
                label: "Invalid".to_owned(),
                identity: None,
            })
            .await,
        Err(ResearchError::NotFound { entity: "case", .. })
    ));
    assert!(matches!(
        service
            .capture_snapshot(snapshot_input(
                ResearchSourceId::parse("missing-source").unwrap(),
                b"content"
            ))
            .await,
        Err(ResearchError::NotFound {
            entity: "source",
            ..
        })
    ));
    let mut input = snapshot_input(source.id.clone(), b"content");
    input
        .metadata
        .insert("authorization".to_owned(), "secret".to_owned());
    assert!(
        matches!(service.capture_snapshot(input).await, Err(ResearchError::Invalid(message)) if message.contains("metadata key"))
    );
    let mut oversized_metadata = snapshot_input(source.id.clone(), b"content-2");
    oversized_metadata
        .metadata
        .insert("note".to_owned(), "x".repeat(MAX_METADATA_BYTES));
    assert!(matches!(
        service.capture_snapshot(oversized_metadata).await,
        Err(ResearchError::Invalid(message)) if message.contains("metadata exceeds")
    ));
    assert!(matches!(
        service.get_source("missing").await,
        Err(ResearchError::NotFound {
            entity: "source",
            ..
        })
    ));
    let snapshot = service
        .capture_snapshot(snapshot_input(source.id, b"content"))
        .await
        .unwrap();
    assert!(matches!(
        service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case_id.clone(),
                source_snapshot_id: ResearchSourceSnapshotId::parse("missing-snapshot").unwrap(),
                verbatim_excerpt: "excerpt".to_owned(),
                normalized_text: None,
                locator: EvidenceLocator::TextRange { start: 0, end: 1 },
                capture_method: CaptureMethod::UploadedArtifact,
            })
            .await,
        Err(ResearchError::NotFound {
            entity: "source snapshot",
            ..
        })
    ));
    assert!(matches!(
        service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case_id.clone(),
                source_snapshot_id: snapshot.id.clone(),
                verbatim_excerpt: "excerpt".to_owned(),
                normalized_text: None,
                locator: EvidenceLocator::Pdf {
                    page: 0,
                    end_page: None,
                },
                capture_method: CaptureMethod::UploadedArtifact,
        })
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("page range")
    ));
    assert!(matches!(
        service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case_id.clone(),
                source_snapshot_id: snapshot.id.clone(),
                verbatim_excerpt: "x".repeat(MAX_EVIDENCE_EXCERPT_BYTES + 1),
                normalized_text: None,
                locator: EvidenceLocator::TextRange { start: 0, end: 1 },
                capture_method: CaptureMethod::UploadedArtifact,
            })
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("verbatim excerpt")
    ));
    assert!(matches!(
        service
            .create_claim(CreateResearchClaim {
                research_case_id: case_id.clone(),
                text: "x".repeat(MAX_CLAIM_TEXT_BYTES + 1),
                origin: ClaimOrigin::Agent,
            })
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("claim text")
    ));
    assert!(serde_json::from_str::<ClaimEvidenceRelation>("\"unknown\"").is_err());

    let second_case = service
        .create_case(CreateResearchCase {
            title: "Second".to_owned(),
        })
        .await
        .unwrap();
    let second_claim = service
        .create_claim(CreateResearchClaim {
            research_case_id: second_case.id.clone(),
            text: "second claim".to_owned(),
            origin: ClaimOrigin::Agent,
        })
        .await
        .unwrap();
    let evidence = service
        .create_evidence(CreateResearchEvidence {
            research_case_id: case_id.clone(),
            source_snapshot_id: snapshot.id,
            verbatim_excerpt: "excerpt".to_owned(),
            normalized_text: None,
            locator: EvidenceLocator::TextRange { start: 0, end: 1 },
            capture_method: CaptureMethod::UploadedArtifact,
        })
        .await
        .unwrap();
    assert!(matches!(
        service
            .create_link(CreateClaimEvidenceLink {
                research_case_id: second_case.id,
                claim_id: second_claim.id,
                evidence_id: evidence.id,
                relation: ClaimEvidenceRelation::Supports,
                rationale: None,
                assessment_method: AssessmentMethod::Human,
                assessment_metadata: BTreeMap::new(),
            })
            .await,
        Err(ResearchError::Invalid(message)) if message.contains("same research case")
    ));
}
