use super::common::{service, snapshot_input};

use super::*;

#[tokio::test]
async fn claim_without_link_has_no_assessment_and_relations_are_categorical() {
    let (_database, service) = service().await;
    let case = service
        .create_case(CreateResearchCase {
            title: "Review".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(CreateResearchSource {
            research_case_id: case.id.clone(),
            kind: SourceKind::Web,
            label: "Web source".to_owned(),
        })
        .await
        .unwrap();
    let snapshot = service
        .capture_snapshot(snapshot_input(source.id, b"source"))
        .await
        .unwrap();
    let evidence = service
        .create_evidence(CreateResearchEvidence {
            research_case_id: case.id.clone(),
            source_snapshot_id: snapshot.id,
            verbatim_excerpt: "source says X".to_owned(),
            normalized_text: None,
            locator: EvidenceLocator::Web {
                fragment: Some("#section".to_owned()),
                start: None,
                end: None,
            },
            capture_method: CaptureMethod::WebRetrieval,
        })
        .await
        .unwrap();
    let claim = service
        .create_claim(CreateResearchClaim {
            research_case_id: case.id.clone(),
            text: "Claim X".to_owned(),
            origin: ClaimOrigin::User,
        })
        .await
        .unwrap();
    assert!(
        service
            .list_links(Some(case.id.as_str()), None, None)
            .await
            .unwrap()
            .is_empty()
    );
    let relations = [
        ClaimEvidenceRelation::Supports,
        ClaimEvidenceRelation::Contradicts,
        ClaimEvidenceRelation::Contextualizes,
        ClaimEvidenceRelation::Insufficient,
    ];
    for relation in relations {
        service
            .create_link(CreateClaimEvidenceLink {
                research_case_id: case.id.clone(),
                claim_id: claim.id.clone(),
                evidence_id: evidence.id.clone(),
                relation,
                rationale: Some("The excerpt is assessed against the claim.".to_owned()),
                assessment_method: AssessmentMethod::Human,
                assessment_metadata: BTreeMap::new(),
            })
            .await
            .unwrap();
    }
    assert_eq!(
        service
            .list_links(Some(case.id.as_str()), None, None)
            .await
            .unwrap()
            .len(),
        4
    );
}
