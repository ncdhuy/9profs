use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CapturePdfExtraction, CapturePdfPage, EvidenceLocator, ExtractRegulationRequirementCandidates,
    PdfExtractionStatus, PromoteRegulationRequirementCandidate, RegulationApplicability,
    RegulationRequirementCandidate, RegulationRequirementCandidateExtractionIdentity,
    RegulationRequirementCandidateExtractionProvider,
    RegulationRequirementCandidateExtractionProviderError, RegulationRequirementCandidateId,
    RegulationRequirementCandidateOutput, ResearchArtifactStore, ResearchError,
    ResearchPdfExtraction, ResearchPdfExtractionId, ResearchService, SourceKind,
};

const PAGE_TEXT: &str = "Yêu cầu: phải; không được; ít nhất 50%; khoảng 1/4; khoảng 1/6 - 1/5; 0,7; 1.5; 3.5 cm. Nếu website không có tác giả, dùng tên tổ chức.";

#[derive(Clone)]
struct FakeExtractor {
    output: Vec<RegulationRequirementCandidateOutput>,
}

#[async_trait]
impl RegulationRequirementCandidateExtractionProvider for FakeExtractor {
    fn identity(&self) -> RegulationRequirementCandidateExtractionIdentity {
        RegulationRequirementCandidateExtractionIdentity {
            provider: "fake".to_owned(),
            extractor_version: "fake-regulation-extractor-v1".to_owned(),
            model_id: Some("fake-model".to_owned()),
            extraction_contract_version: "regulation-requirement-extraction-v0.1".to_owned(),
        }
    }

    async fn extract(
        &self,
        _input: nineprofs_research::RegulationRequirementExtractionInput,
    ) -> Result<
        Vec<RegulationRequirementCandidateOutput>,
        RegulationRequirementCandidateExtractionProviderError,
    > {
        Ok(self.output.clone())
    }
}

fn output(ocr_excerpt: &str, normalized_requirement: &str) -> RegulationRequirementCandidateOutput {
    RegulationRequirementCandidateOutput {
        ocr_excerpt: ocr_excerpt.to_owned(),
        normalized_requirement: normalized_requirement.to_owned(),
        source_locator: EvidenceLocator::Pdf {
            page: 1,
            end_page: None,
        },
        authority_locator: Some(EvidenceLocator::Regulation {
            article: "Phụ lục 3".to_owned(),
            section: Some("1.2".to_owned()),
            clause: None,
        }),
        applicability: RegulationApplicability::default(),
        risk_flags: Vec::new(),
        review_notes: None,
    }
}

async fn fixture(
    output: RegulationRequirementCandidateOutput,
) -> (
    Database,
    ResearchService,
    nineprofs_research::ResearchSourceId,
    nineprofs_research::ResearchSourceSnapshotId,
    ResearchPdfExtraction,
) {
    let database = Database::in_memory().await.unwrap();
    let repository = nineprofs_research::SqliteResearchRepository::new(database.pool().clone());
    let artifact_store = Arc::new(ResearchArtifactStore::new(
        std::env::temp_dir().join(format!(
            "9profs-regulation-candidate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )),
        database.pool().clone(),
    ));
    let service = ResearchService::new(repository, Arc::new(BroadcastEventBus::new(32)))
        .with_artifact_store(artifact_store.clone())
        .with_regulation_requirement_candidate_extractor(Arc::new(FakeExtractor {
            output: vec![output],
        }));
    let case = service
        .create_case(nineprofs_research::CreateResearchCase {
            title: "Candidate extraction test".to_owned(),
        })
        .await
        .unwrap();
    let source = service
        .create_source(nineprofs_research::CreateResearchSource {
            research_case_id: case.id,
            kind: SourceKind::Regulation,
            label: "Institution regulation".to_owned(),
            identity: None,
        })
        .await
        .unwrap();
    let mut upload = artifact_store.begin_upload("regulation.pdf").unwrap();
    upload
        .append(b"%PDF-1.7\nregulation candidate fixture")
        .unwrap();
    let artifact = upload.finish().await.unwrap();
    let snapshot = service
        .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
        .await
        .unwrap();
    let extraction = service
        .capture_pdf_extraction(CapturePdfExtraction {
            source_snapshot_id: snapshot.id.clone(),
            extractor: "test-ocr".to_owned(),
            extractor_version: Some("test-ocr-v1".to_owned()),
            page_count: 1,
            status: PdfExtractionStatus::Ready,
            pages: vec![CapturePdfPage {
                page: 1,
                text: PAGE_TEXT.to_owned(),
            }],
        })
        .await
        .unwrap();
    (database, service, source.id, snapshot.id, extraction)
}

fn request(
    source_id: nineprofs_research::ResearchSourceId,
    snapshot_id: nineprofs_research::ResearchSourceSnapshotId,
    extraction_id: ResearchPdfExtractionId,
    vocabulary: BTreeMap<String, Vec<String>>,
) -> ExtractRegulationRequirementCandidates {
    ExtractRegulationRequirementCandidates {
        source_id,
        source_snapshot_id: snapshot_id,
        pdf_extraction_id: extraction_id,
        start_page: 1,
        end_page: 1,
        institution: Some("HIU".to_owned()),
        document_title: Some("Regulation".to_owned()),
        known_artifact_scope: Some("master_thesis".to_owned()),
        allowed_applicability_vocabulary: vocabulary,
    }
}

async fn persisted_candidate(
    output: RegulationRequirementCandidateOutput,
    vocabulary: BTreeMap<String, Vec<String>>,
) -> (Database, ResearchService, RegulationRequirementCandidate) {
    let mut output = output;
    output.ocr_excerpt = PAGE_TEXT.to_owned();
    let (database, service, source_id, snapshot_id, extraction) = fixture(output).await;
    let candidate = service
        .extract_regulation_requirement_candidates(request(
            source_id,
            snapshot_id,
            extraction.id,
            vocabulary,
        ))
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    (database, service, candidate)
}

fn verified_input(
    candidate: &RegulationRequirementCandidate,
    applicability: RegulationApplicability,
    authority_locator: Option<EvidenceLocator>,
    active: bool,
) -> PromoteRegulationRequirementCandidate {
    PromoteRegulationRequirementCandidate {
        candidate_id: candidate.id.clone(),
        text: "Human verified requirement".to_owned(),
        source_excerpt: "Human verified excerpt".to_owned(),
        source_locator: EvidenceLocator::PdfTextRange {
            page: 1,
            start: 0,
            end: 10,
        },
        authority_locator,
        applicability,
        effective_from: Some(100),
        effective_until: Some(200),
        active,
    }
}

#[tokio::test]
async fn candidate_persistence_preserves_provenance_and_separates_ocr_from_normalization() {
    let (_database, service, source_id, snapshot_id, extraction) = fixture(output(
        "không được; ít nhất 50%; khoảng 1/4",
        "Không được sửa OCR; giữ ít nhất 50% và khoảng 1/4.",
    ))
    .await;
    let candidates = service
        .extract_regulation_requirement_candidates(request(
            source_id.clone(),
            snapshot_id.clone(),
            extraction.id.clone(),
            BTreeMap::new(),
        ))
        .await
        .unwrap();
    let candidate = candidates.first().unwrap();
    assert_ne!(
        candidate.id.as_str(),
        nineprofs_research::RegulationRequirementId::new().as_str()
    );
    assert_eq!(candidate.source_id, source_id);
    assert_eq!(candidate.source_snapshot_id, snapshot_id);
    assert_eq!(candidate.pdf_extraction_id, extraction.id);
    assert_eq!(candidate.ocr_excerpt, "không được; ít nhất 50%; khoảng 1/4");
    assert_ne!(candidate.ocr_excerpt, candidate.normalized_requirement);
    assert_eq!(candidate.extraction.method, "llm");

    let loaded = service
        .get_regulation_requirement_candidate(candidate.id.as_str())
        .await
        .unwrap();
    assert_eq!(loaded, *candidate);
    assert_eq!(
        service
            .list_regulation_requirement_candidates(
                Some(source_id.as_str()),
                Some(snapshot_id.as_str()),
                Some(extraction.id.as_str()),
            )
            .await
            .unwrap(),
        vec![candidate.clone()]
    );
    let json = serde_json::to_string(candidate).unwrap();
    let round_trip: RegulationRequirementCandidate = serde_json::from_str(&json).unwrap();
    assert_eq!(round_trip, *candidate);
}

#[tokio::test]
async fn exact_unicode_ocr_excerpt_and_numeric_text_are_accepted() {
    let excerpt = "phải; không được; ít nhất 50%; khoảng 1/4; khoảng 1/6 - 1/5; 0,7; 1.5; 3.5 cm";
    let (_database, service, source_id, snapshot_id, extraction) = fixture(output(
        excerpt,
        "phải; không được; ít nhất 50%; khoảng 1/4; khoảng 1/6 - 1/5; 0,7; 1.5; 3.5 cm",
    ))
    .await;
    let result = service
        .extract_regulation_requirement_candidates(request(
            source_id,
            snapshot_id,
            extraction.id,
            BTreeMap::new(),
        ))
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn modified_fabricated_and_empty_ocr_excerpts_are_rejected() {
    for excerpt in ["không duoc", "not present", "   "] {
        let (_database, service, source_id, snapshot_id, extraction) =
            fixture(output(excerpt, "A normalized requirement")).await;
        let result = service
            .extract_regulation_requirement_candidates(request(
                source_id,
                snapshot_id,
                extraction.id,
                BTreeMap::new(),
            ))
            .await;
        assert!(
            matches!(result, Err(ResearchError::Invalid(_))),
            "{excerpt:?}"
        );
    }
}

#[tokio::test]
async fn applicability_uses_supplied_canonical_values_and_keeps_conditions_semantic() {
    let mut canonical = output(
        "Nếu website không có tác giả, dùng tên tổ chức",
        "Nếu website không có tác giả, dùng tên tổ chức.",
    );
    canonical.applicability = RegulationApplicability {
        facets: BTreeMap::from([(
            "artifact_types".to_owned(),
            vec!["master_thesis".to_owned()],
        )]),
    };
    let (_database, service, source_id, snapshot_id, extraction) = fixture(canonical).await;
    let accepted = service
        .extract_regulation_requirement_candidates(request(
            source_id.clone(),
            snapshot_id.clone(),
            extraction.id.clone(),
            BTreeMap::from([(
                "artifact_types".to_owned(),
                vec!["master_thesis".to_owned()],
            )]),
        ))
        .await
        .unwrap();
    assert!(
        accepted[0]
            .applicability_suggestion
            .facets
            .get("website_without_author")
            .is_none()
    );
    assert!(accepted[0].normalized_requirement.contains("website"));

    let mut unsupported = output("phải", "phải");
    unsupported.applicability = RegulationApplicability {
        facets: BTreeMap::from([
            (
                "artifact_types".to_owned(),
                vec!["master_thesis".to_owned()],
            ),
            ("reporting_guidelines".to_owned(), vec!["APA".to_owned()]),
        ]),
    };
    let (_database, service, source_id, snapshot_id, extraction) = fixture(unsupported).await;
    let accepted = service
        .extract_regulation_requirement_candidates(request(
            source_id,
            snapshot_id,
            extraction.id,
            BTreeMap::from([(
                "artifact_types".to_owned(),
                vec!["master_thesis".to_owned()],
            )]),
        ))
        .await;
    let candidate = accepted.unwrap().pop().unwrap();
    assert_eq!(
        candidate.applicability_suggestion.facets,
        BTreeMap::from([(
            "artifact_types".to_owned(),
            vec!["master_thesis".to_owned()],
        )])
    );
    assert!(
        candidate
            .risk_flags
            .contains(&"unresolved_applicability".to_owned())
    );
    assert!(
        candidate
            .review_notes
            .as_deref()
            .is_some_and(|notes| notes.contains("outside the supplied vocabulary"))
    );
}

#[tokio::test]
async fn invalid_authority_locator_suggestion_is_unset_and_flagged() {
    let mut invalid = output("phải", "phải");
    invalid.authority_locator = Some(EvidenceLocator::Regulation {
        article: String::new(),
        section: Some("3.3.1".to_owned()),
        clause: None,
    });
    let (_database, service, source_id, snapshot_id, extraction) = fixture(invalid).await;
    let candidate = service
        .extract_regulation_requirement_candidates(request(
            source_id,
            snapshot_id,
            extraction.id,
            BTreeMap::new(),
        ))
        .await
        .unwrap()
        .pop()
        .unwrap();

    assert!(candidate.authority_locator_suggestion.is_none());
    assert!(
        candidate
            .risk_flags
            .contains(&"invalid_authority_locator_suggestion".to_owned())
    );
    assert!(
        candidate
            .review_notes
            .as_deref()
            .is_some_and(|notes| notes.contains("Authority locator suggestion was unset"))
    );
}

#[test]
fn extraction_applicability_vocabulary_validation_remains_strict() {
    let applicability = RegulationApplicability {
        facets: BTreeMap::from([("reporting_guidelines".to_owned(), vec!["APA".to_owned()])]),
    };
    let vocabulary = BTreeMap::from([(
        "artifact_types".to_owned(),
        vec!["master_thesis".to_owned()],
    )]);

    assert!(matches!(
        applicability.validate_for_extraction(&vocabulary),
        Err(ResearchError::Invalid(_))
    ));
}

#[tokio::test]
async fn source_locator_must_stay_inside_requested_page_range() {
    let mut invalid = output("phải", "phải");
    invalid.source_locator = EvidenceLocator::Pdf {
        page: 2,
        end_page: None,
    };
    let (_database, service, source_id, snapshot_id, extraction) = fixture(invalid).await;
    let result = service
        .extract_regulation_requirement_candidates(request(
            source_id,
            snapshot_id,
            extraction.id,
            BTreeMap::new(),
        ))
        .await;
    assert!(matches!(result, Err(ResearchError::Invalid(_))));
}

#[tokio::test]
async fn verified_candidate_promotes_atomically_without_mutating_candidate() {
    let mut candidate_output = output(PAGE_TEXT, "Machine interpretation");
    candidate_output.applicability = RegulationApplicability {
        facets: BTreeMap::from([(
            "artifact_types".to_owned(),
            vec!["master_thesis".to_owned()],
        )]),
    };
    let vocabulary = BTreeMap::from([(
        "artifact_types".to_owned(),
        vec!["master_thesis".to_owned()],
    )]);
    let (_database, service, candidate) = persisted_candidate(candidate_output, vocabulary).await;
    let original_candidate = candidate.clone();
    let authority_locator = Some(EvidenceLocator::Regulation {
        article: "Verified Article".to_owned(),
        section: Some("2".to_owned()),
        clause: Some("a".to_owned()),
    });
    let promoted = service
        .promote_regulation_requirement_candidate(verified_input(
            &candidate,
            RegulationApplicability {
                facets: BTreeMap::from([(
                    "artifact_types".to_owned(),
                    vec!["phd_dissertation".to_owned()],
                )]),
            },
            authority_locator.clone(),
            true,
        ))
        .await
        .unwrap();

    assert_eq!(
        promoted.review_status,
        nineprofs_research::RegulationReviewStatus::Approved
    );
    assert!(promoted.active);
    assert_eq!(promoted.source_id, candidate.source_id);
    assert_eq!(promoted.source_snapshot_id, candidate.source_snapshot_id);
    assert_eq!(
        promoted.pdf_extraction_id,
        Some(candidate.pdf_extraction_id.clone())
    );
    assert_eq!(promoted.extraction_method, candidate.extraction.method);
    assert_eq!(
        promoted.extraction_contract_version,
        Some(candidate.extraction.contract_version.clone())
    );
    assert_eq!(promoted.text, "Human verified requirement");
    assert_eq!(promoted.source_excerpt, "Human verified excerpt");
    assert_eq!(
        promoted.source_excerpt_hash.value,
        "547befa9d90a56e6e83f7ace64802cefd72ee97644306d4971c6036ca2f27a0c"
    );
    assert_eq!(
        promoted.source_locator,
        verified_input(&candidate, RegulationApplicability::default(), None, false,).source_locator
    );
    assert_eq!(promoted.authority_locator, authority_locator);
    assert_eq!(
        promoted.applicability.facets.get("artifact_types"),
        Some(&vec!["phd_dissertation".to_owned()])
    );
    assert_eq!(promoted.effective_from, Some(100));
    assert_eq!(promoted.effective_until, Some(200));
    assert_eq!(
        service
            .get_regulation_requirement_candidate(candidate.id.as_str())
            .await
            .unwrap(),
        original_candidate
    );
}

#[tokio::test]
async fn promotion_does_not_reuse_fail_closed_advisory_metadata() {
    let mut invalid = output(
        "khÃ´ng Ä‘Æ°á»£c; Ã­t nháº¥t 50%; khoáº£ng 1/4",
        "Machine interpretation",
    );
    invalid.authority_locator = Some(EvidenceLocator::TextRange { start: 0, end: 4 });
    invalid.applicability = RegulationApplicability {
        facets: BTreeMap::from([("unsupported".to_owned(), vec!["value".to_owned()])]),
    };
    let (_database, service, candidate) = persisted_candidate(invalid, BTreeMap::new()).await;
    assert!(candidate.authority_locator_suggestion.is_none());
    assert!(candidate.applicability_suggestion.facets.is_empty());
    assert!(
        candidate
            .risk_flags
            .contains(&"invalid_authority_locator_suggestion".to_owned())
    );
    assert!(
        candidate
            .risk_flags
            .contains(&"unresolved_applicability".to_owned())
    );

    let promoted = service
        .promote_regulation_requirement_candidate(verified_input(
            &candidate,
            RegulationApplicability::default(),
            None,
            false,
        ))
        .await
        .unwrap();
    assert_eq!(promoted.authority_locator, None);
    assert!(promoted.applicability.facets.is_empty());
    assert_eq!(
        promoted.review_status,
        nineprofs_research::RegulationReviewStatus::Approved
    );
    assert!(!promoted.active);
}

#[tokio::test]
async fn promotion_rejects_invalid_authoritative_values() {
    let (_database, service, candidate) = persisted_candidate(
        output(
            "khÃ´ng Ä‘Æ°á»£c; Ã­t nháº¥t 50%; khoáº£ng 1/4",
            "Machine interpretation",
        ),
        BTreeMap::new(),
    )
    .await;

    let invalid_applicability = service
        .promote_regulation_requirement_candidate(verified_input(
            &candidate,
            RegulationApplicability {
                facets: BTreeMap::from([("future_facet".to_owned(), vec!["value".to_owned()])]),
            },
            None,
            true,
        ))
        .await;
    assert!(matches!(
        invalid_applicability,
        Err(ResearchError::Invalid(_))
    ));

    let invalid_authority_locator = service
        .promote_regulation_requirement_candidate(verified_input(
            &candidate,
            RegulationApplicability::default(),
            Some(EvidenceLocator::TextRange { start: 0, end: 4 }),
            true,
        ))
        .await;
    assert!(matches!(
        invalid_authority_locator,
        Err(ResearchError::Invalid(_))
    ));

    let mut empty_text = verified_input(&candidate, RegulationApplicability::default(), None, true);
    empty_text.text = "  ".to_owned();
    assert!(matches!(
        service
            .promote_regulation_requirement_candidate(empty_text)
            .await,
        Err(ResearchError::Invalid(_))
    ));
    assert!(
        service
            .list_regulation_requirements(
                Some(candidate.source_id.as_str()),
                Some(candidate.source_snapshot_id.as_str()),
            )
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn promotion_missing_candidate_fails_cleanly() {
    let (_database, service, candidate) = persisted_candidate(
        output(
            "khÃ´ng Ä‘Æ°á»£c; Ã­t nháº¥t 50%; khoáº£ng 1/4",
            "Machine interpretation",
        ),
        BTreeMap::new(),
    )
    .await;
    let mut input = verified_input(&candidate, RegulationApplicability::default(), None, false);
    input.candidate_id = RegulationRequirementCandidateId::new();

    assert!(matches!(
        service
            .promote_regulation_requirement_candidate(input)
            .await,
        Err(ResearchError::NotFound {
            entity: "regulation requirement candidate",
            ..
        })
    ));
}
