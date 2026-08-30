use std::{collections::BTreeMap, sync::Arc};

use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CaptureSourceSnapshot, CreateRegulationRequirement, CreateResearchCase, CreateResearchSource,
    EvidenceLocator, RegulationApplicability, RegulationRequirement, RegulationReviewStatus,
    ResearchContext, ResearchService, SourceKind, SourceOrigin,
    resolve_effective_regulation_requirements,
};

async fn fixture() -> (Database, ResearchService, String, String) {
    let database = Database::in_memory().await.unwrap();
    let service = ResearchService::new(
        nineprofs_research::SqliteResearchRepository::new(database.pool().clone()),
        Arc::new(BroadcastEventBus::new(32)),
    );
    let case = service
        .create_case(CreateResearchCase {
            title: "Regulation test case".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id,
            kind: SourceKind::Regulation,
            label: "Institution regulation".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let snapshot = service
        .capture_snapshot(CaptureSourceSnapshot {
            source_id: source.id.clone(),
            content: b"Regulation source snapshot".to_vec(),
            capture_method: nineprofs_research::CaptureMethod::ExternalImport,
            origin: SourceOrigin::ExternalImport {
                provider: "test".to_owned(),
                external_reference: "regulation-test".to_owned(),
            },
            metadata: BTreeMap::new(),
        })
        .await
        .unwrap();
    (
        database,
        service,
        source.id.to_string(),
        snapshot.id.to_string(),
    )
}

fn applicability(facets: &[(&str, &[&str])]) -> RegulationApplicability {
    RegulationApplicability {
        facets: facets
            .iter()
            .map(|(facet, values)| {
                (
                    (*facet).to_owned(),
                    values.iter().map(|value| (*value).to_owned()).collect(),
                )
            })
            .collect(),
    }
}

fn requirement_input(
    source_id: &str,
    snapshot_id: &str,
    text: &str,
) -> CreateRegulationRequirement {
    CreateRegulationRequirement {
        source_id: nineprofs_research::ResearchSourceId::parse(source_id).unwrap(),
        source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(snapshot_id)
            .unwrap(),
        pdf_extraction_id: None,
        text: text.to_owned(),
        source_excerpt: format!("Authoritative wording for {text}"),
        source_locator: EvidenceLocator::PdfTextRange {
            page: 1,
            start: 0,
            end: 32,
        },
        authority_locator: Some(EvidenceLocator::Regulation {
            article: "Article 1".to_owned(),
            section: Some("Section 1".to_owned()),
            clause: None,
        }),
        applicability: RegulationApplicability::default(),
        effective_from: None,
        effective_until: None,
        extraction_method: "manual".to_owned(),
        extraction_contract_version: Some("regulation-v0.1".to_owned()),
    }
}

async fn create_requirement(
    service: &ResearchService,
    source_id: &str,
    snapshot_id: &str,
    text: &str,
) -> RegulationRequirement {
    service
        .create_regulation_requirement(requirement_input(source_id, snapshot_id, text))
        .await
        .unwrap()
}

#[test]
fn research_context_accepts_empty_multiple_and_unknown_identifiers() {
    let context = ResearchContext {
        research_families: vec!["MED".to_owned(), "future_family".to_owned()],
        study_designs: vec!["new_design".to_owned()],
        ..ResearchContext::default()
    };
    assert!(context.validate().is_ok());
    assert!(ResearchContext::default().research_families.is_empty());
}

#[test]
fn applicability_is_empty_universal_and_uses_and_or_intersection_semantics() {
    let context = ResearchContext {
        artifact_type: Some("master_thesis".to_owned()),
        research_families: vec!["MED".to_owned(), "public_health".to_owned()],
        ..ResearchContext::default()
    };
    assert!(RegulationApplicability::default().matches(&context));
    assert!(
        applicability(&[("artifact_types", &["master_thesis", "phd_dissertation"])])
            .matches(&context)
    );
    assert!(
        applicability(&[
            ("artifact_types", &["phd_dissertation", "master_thesis"]),
            ("research_families", &["LAW", "MED"]),
        ])
        .matches(&context)
    );
    assert!(!applicability(&[("artifact_types", &["scientific_article"])]).matches(&context));
    assert!(!applicability(&[("research_families", &["LAW"])]).matches(&context));
    assert!(!applicability(&[("future_facet", &["future_value"])]).matches(&context));
}

#[test]
fn effective_resolution_is_pure_and_filters_lifecycle_scope_and_time() {
    let base = RegulationRequirement {
        id: nineprofs_research::RegulationRequirementId::new(),
        source_id: nineprofs_research::ResearchSourceId::new(),
        source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::new(),
        pdf_extraction_id: None,
        text: "Requirement".to_owned(),
        source_excerpt: "Source wording".to_owned(),
        source_excerpt_hash: nineprofs_research::ContentHash {
            algorithm: nineprofs_research::HashAlgorithm::Sha256,
            value: "hash".to_owned(),
        },
        source_locator: EvidenceLocator::TextRange { start: 0, end: 4 },
        authority_locator: None,
        applicability: RegulationApplicability::default(),
        effective_from: Some(10),
        effective_until: Some(20),
        extraction_method: "manual".to_owned(),
        extraction_contract_version: None,
        review_status: RegulationReviewStatus::Approved,
        active: true,
        created_at_ms: 1,
        updated_at_ms: 1,
    };
    let mut wrong_status = base.clone();
    wrong_status.id = nineprofs_research::RegulationRequirementId::new();
    wrong_status.review_status = RegulationReviewStatus::NeedsReview;
    let mut inactive = base.clone();
    inactive.id = nineprofs_research::RegulationRequirementId::new();
    inactive.active = false;
    let mut out_of_scope = base.clone();
    out_of_scope.id = nineprofs_research::RegulationRequirementId::new();
    out_of_scope.applicability = applicability(&[("research_families", &["LAW"])]);

    let selected = resolve_effective_regulation_requirements(
        &[base.clone(), wrong_status, inactive, out_of_scope],
        &ResearchContext {
            research_families: vec!["MED".to_owned()],
            ..ResearchContext::default()
        },
        15,
    );
    assert_eq!(selected, vec![base]);
    assert!(
        resolve_effective_regulation_requirements(&selected, &ResearchContext::default(), 21,)
            .is_empty()
    );
}

#[tokio::test]
async fn requirement_persistence_provenance_and_lifecycle_round_trip() {
    let (_database, service, source_id, snapshot_id) = fixture().await;
    let requirement = create_requirement(&service, &source_id, &snapshot_id, "Use consent").await;

    let loaded = service
        .get_regulation_requirement(requirement.id.as_str())
        .await
        .unwrap();
    assert_eq!(loaded, requirement);
    assert_eq!(
        loaded.source_excerpt,
        "Authoritative wording for Use consent"
    );
    assert_eq!(
        loaded.source_excerpt_hash.algorithm,
        nineprofs_research::HashAlgorithm::Sha256
    );
    assert_eq!(loaded.source_locator, requirement.source_locator);
    assert_eq!(
        service
            .list_regulation_requirements(Some(&source_id), Some(&snapshot_id))
            .await
            .unwrap(),
        vec![requirement.clone()]
    );

    assert!(
        service
            .set_regulation_requirement_active(requirement.id.as_str(), true)
            .await
            .is_err()
    );
    let approved = service
        .update_regulation_requirement_review_status(
            requirement.id.as_str(),
            RegulationReviewStatus::Approved,
        )
        .await
        .unwrap();
    assert!(!approved.active);
    let active = service
        .set_regulation_requirement_active(requirement.id.as_str(), true)
        .await
        .unwrap();
    assert!(active.active);
    let inactive = service
        .set_regulation_requirement_active(requirement.id.as_str(), false)
        .await
        .unwrap();
    assert!(!inactive.active);
}

#[tokio::test]
async fn rejected_requirement_cannot_be_activated() {
    let (_database, service, source_id, snapshot_id) = fixture().await;
    let requirement = create_requirement(&service, &source_id, &snapshot_id, "Reject me").await;
    let rejected = service
        .update_regulation_requirement_review_status(
            requirement.id.as_str(),
            RegulationReviewStatus::Rejected,
        )
        .await
        .unwrap();
    assert!(!rejected.active);
    assert!(
        service
            .set_regulation_requirement_active(requirement.id.as_str(), true)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn requirement_requires_regulation_source_and_matching_snapshot() {
    let (_database, service, source_id, snapshot_id) = fixture().await;
    let case = service
        .create_case(CreateResearchCase {
            title: "Second source".to_owned(),
        })
        .await
        .unwrap();
    let manuscript = service
        .create_source(CreateResearchSource {
            research_case_id: case.id,
            kind: SourceKind::Manuscript,
            label: "Manuscript".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let manuscript_snapshot = service
        .capture_snapshot(CaptureSourceSnapshot {
            source_id: manuscript.id.clone(),
            content: b"manuscript".to_vec(),
            capture_method: nineprofs_research::CaptureMethod::ExternalImport,
            origin: SourceOrigin::ExternalImport {
                provider: "test".to_owned(),
                external_reference: "manuscript".to_owned(),
            },
            metadata: BTreeMap::new(),
        })
        .await
        .unwrap();
    assert!(
        service
            .create_regulation_requirement(requirement_input(
                manuscript.id.as_str(),
                manuscript_snapshot.id.as_str(),
                "Invalid source",
            ))
            .await
            .is_err()
    );

    let other_case = service
        .create_case(CreateResearchCase {
            title: "Other regulation source".to_owned(),
        })
        .await
        .unwrap();
    let other_source = service
        .create_source(CreateResearchSource {
            research_case_id: other_case.id,
            kind: SourceKind::Regulation,
            label: "Other regulation".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let other_snapshot = service
        .capture_snapshot(CaptureSourceSnapshot {
            source_id: other_source.id,
            content: b"other regulation".to_vec(),
            capture_method: nineprofs_research::CaptureMethod::ExternalImport,
            origin: SourceOrigin::ExternalImport {
                provider: "test".to_owned(),
                external_reference: "other-regulation".to_owned(),
            },
            metadata: BTreeMap::new(),
        })
        .await
        .unwrap();
    assert!(
        service
            .create_regulation_requirement(requirement_input(
                &source_id,
                other_snapshot.id.as_str(),
                "Mismatched snapshot",
            ))
            .await
            .is_err()
    );
    assert_ne!(snapshot_id, other_snapshot.id.as_str());
}
