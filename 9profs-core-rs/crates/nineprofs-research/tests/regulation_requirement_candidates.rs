use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use nineprofs_db::Database;
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{
    CapturePdfExtraction, CapturePdfPage, EvidenceLocator, ExtractRegulationRequirementCandidates,
    PdfExtractionStatus, RegulationApplicability, RegulationRequirementCandidate,
    RegulationRequirementCandidateExtractionIdentity,
    RegulationRequirementCandidateExtractionProvider,
    RegulationRequirementCandidateExtractionProviderError, RegulationRequirementCandidateOutput,
    ResearchArtifactStore, ResearchError, ResearchPdfExtraction, ResearchPdfExtractionId,
    ResearchService, SourceKind,
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
        facets: BTreeMap::from([(
            "artifact_types".to_owned(),
            vec!["unknown_alias".to_owned()],
        )]),
    };
    let (_database, service, source_id, snapshot_id, extraction) = fixture(unsupported).await;
    let result = service
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
    assert!(matches!(result, Err(ResearchError::Invalid(_))));
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
