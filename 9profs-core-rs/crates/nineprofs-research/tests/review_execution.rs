use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use nineprofs_research::{
    ContentHash, DOCUMENT_MAP_CONTRACT_VERSION, DocumentMap, DocumentMapBlock,
    DocumentMapBlockKind, DocumentMapLocator, DocumentMapSection, EvidenceLocator, HashAlgorithm,
    REVIEW_TASK_CONTRACT_VERSION, RegulationApplicability, RegulationRequirement,
    RegulationReviewStatus, ResearchContext, ResearchSourceId, ResearchSourceSnapshotId,
    ResolvedReviewStack, ReviewAuthorityReference, ReviewExecutorMode, ReviewTask,
    ReviewTaskTarget, load_canonical_authority_packs, resolve_review_stack,
    validate_review_task_response,
};
fn context() -> ResearchContext {
    ResearchContext {
        language: Some("vi".to_owned()),
        research_families: vec!["MED".to_owned()],
        artifact_type: Some("master_thesis".to_owned()),
        academic_level: Some("master".to_owned()),
        organization: Some("hiu".to_owned()),
        ..ResearchContext::default()
    }
}

fn locator(block_id: &str, ordinal: u32, section_id: &str) -> DocumentMapLocator {
    DocumentMapLocator {
        document_id: "fixture-doc".to_owned(),
        version: 7,
        block_id: block_id.to_owned(),
        block_ordinal: ordinal,
        docx_index: Some(ordinal),
        section_id: Some(section_id.to_owned()),
    }
}

fn map() -> DocumentMap {
    let section_a = "section:a".to_owned();
    let section_b = "section:b".to_owned();
    let a_heading = locator("a-heading", 0, &section_a);
    let a_body = locator("a-body", 1, &section_a);
    let b_heading = locator("b-heading", 2, &section_b);
    let b_body = locator("b-body", 3, &section_b);
    DocumentMap {
        contract_version: DOCUMENT_MAP_CONTRACT_VERSION.to_owned(),
        document_id: "fixture-doc".to_owned(),
        version: 7,
        sections: vec![
            DocumentMapSection {
                id: section_a.clone(),
                heading_text: "Introduction".to_owned(),
                level: 1,
                parent_id: None,
                locator: a_heading.clone(),
                block_ids: vec!["a-heading".to_owned(), "a-body".to_owned()],
                is_deleted: false,
            },
            DocumentMapSection {
                id: section_b.clone(),
                heading_text: "Results".to_owned(),
                level: 1,
                parent_id: None,
                locator: b_heading.clone(),
                block_ids: vec!["b-heading".to_owned(), "b-body".to_owned()],
                is_deleted: false,
            },
        ],
        blocks: vec![
            DocumentMapBlock {
                id: "a-heading".to_owned(),
                ordinal: 0,
                kind: DocumentMapBlockKind::Heading,
                text: "Introduction".to_owned(),
                locator: a_heading,
                section_id: Some(section_a.clone()),
                heading_level: Some(1),
                caption: None,
                is_deleted: false,
            },
            DocumentMapBlock {
                id: "a-body".to_owned(),
                ordinal: 1,
                kind: DocumentMapBlockKind::Paragraph,
                text: "The study examines blood pressure in adults.".to_owned(),
                locator: a_body,
                section_id: Some(section_a),
                heading_level: None,
                caption: None,
                is_deleted: false,
            },
            DocumentMapBlock {
                id: "b-heading".to_owned(),
                ordinal: 2,
                kind: DocumentMapBlockKind::Heading,
                text: "Results".to_owned(),
                locator: b_heading,
                section_id: Some(section_b.clone()),
                heading_level: Some(1),
                caption: None,
                is_deleted: false,
            },
            DocumentMapBlock {
                id: "b-body".to_owned(),
                ordinal: 3,
                kind: DocumentMapBlockKind::Paragraph,
                text: "The observed difference was 4 mmHg.".to_owned(),
                locator: b_body,
                section_id: Some(section_b),
                heading_level: None,
                caption: None,
                is_deleted: false,
            },
        ],
        tables: Vec::new(),
        figures: Vec::new(),
        citations: Vec::new(),
        references: Vec::new(),
    }
}

fn stack() -> ResolvedReviewStack {
    let ctx = context();
    let packs = load_canonical_authority_packs().unwrap();
    resolve_review_stack(&ctx, &packs, &[], 0).unwrap()
}

fn stack_with_requirement() -> ResolvedReviewStack {
    let ctx = context();
    let packs = load_canonical_authority_packs().unwrap();
    let requirement = RegulationRequirement {
        id: nineprofs_research::RegulationRequirementId::parse("format-1".to_owned()).unwrap(),
        source_id: ResearchSourceId::parse("source-format-1".to_owned()).unwrap(),
        source_snapshot_id: ResearchSourceSnapshotId::parse("snapshot-format-1".to_owned())
            .unwrap(),
        pdf_extraction_id: None,
        text: "The manuscript must identify its study design.".to_owned(),
        source_excerpt: "study design requirement".to_owned(),
        source_excerpt_hash: ContentHash {
            algorithm: HashAlgorithm::Sha256,
            value: "hash-format-1".to_owned(),
        },
        source_locator: EvidenceLocator::TextRange { start: 0, end: 10 },
        authority_locator: Some(EvidenceLocator::Regulation {
            article: "Article 1".to_owned(),
            section: Some("presentation".to_owned()),
            clause: Some("1".to_owned()),
        }),
        applicability: RegulationApplicability::default(),
        effective_from: None,
        effective_until: None,
        extraction_method: "fixture".to_owned(),
        extraction_contract_version: Some("fixture-v1".to_owned()),
        review_status: RegulationReviewStatus::Approved,
        active: true,
        created_at_ms: 1,
        updated_at_ms: 1,
    };
    resolve_review_stack(&ctx, &packs, &[requirement], 0).unwrap()
}

fn requirement_task(map: &DocumentMap, stack: &ResolvedReviewStack) -> ReviewTask {
    let requirement = &stack.regulation_requirements[0];
    let mut task = pack_task(map, stack);
    task.id = "review.test.regulation".to_owned();
    task.authority_references
        .push(ReviewAuthorityReference::RegulationRequirement {
            reference: nineprofs_research::RegulationRequirementReference {
                requirement_id: requirement.id.clone(),
                source_id: requirement.source_id.clone(),
                source_snapshot_id: requirement.source_snapshot_id.clone(),
                authority_locator: requirement.authority_locator.clone(),
                normalized_requirement: requirement.text.clone(),
            },
        });
    task
}

fn pack_task(map: &DocumentMap, stack: &ResolvedReviewStack) -> ReviewTask {
    let pack = stack
        .authority_packs
        .iter()
        .find(|pack| pack.id == "research.core")
        .unwrap();
    ReviewTask {
        contract_version: REVIEW_TASK_CONTRACT_VERSION.to_owned(),
        id: "review.test.semantic".to_owned(),
        kind: "semantic".to_owned(),
        executor_mode: ReviewExecutorMode::Semantic,
        target: ReviewTaskTarget {
            document_map_contract_version: map.contract_version.clone(),
            document_id: map.document_id.clone(),
            document_version: map.version,
            section_ids: vec!["section:a".to_owned()],
            locators: vec![map.blocks[1].locator.clone()],
        },
        instruction: "Review the supplied manuscript content for a defensible research issue."
            .to_owned(),
        authority_references: vec![ReviewAuthorityReference::AuthorityPack {
            pack_id: pack.id.clone(),
            version: pack.version.clone(),
            source: pack.source.clone(),
            content_paths: pack
                .knowledge
                .iter()
                .chain(pack.review_guidance.iter())
                .map(|document| document.path.clone())
                .collect(),
        }],
    }
}

fn openai_response(findings: Value) -> Vec<u8> {
    let content = serde_json::to_string(&findings).unwrap();
    serde_json::to_vec(&json!({
        "choices": [{"message": {"content": content}}]
    }))
    .unwrap()
}

fn candidate(
    locator: &DocumentMapLocator,
    authority_id: &str,
    statement: &str,
    evidence: Option<&str>,
) -> Value {
    json!({
        "statement": statement,
        "explanation": "The supplied manuscript text supports this concern.",
        "manuscriptLocators": [locator],
        "evidence": evidence.map(|excerpt| json!([{"locator": locator, "excerpt": excerpt}])).unwrap_or_else(|| json!([])),
        "authorityIds": [authority_id]
    })
}

fn validate(
    task: &ReviewTask,
    map: &DocumentMap,
    stack: &ResolvedReviewStack,
    response: Value,
) -> nineprofs_research::ReviewTaskValidation {
    validate_review_task_response("openai", &openai_response(response), task, map, stack).unwrap()
}

#[test]
fn valid_semantic_response_becomes_attributable_finding() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let validation = validate(
        &task,
        &map,
        &stack,
        json!({"findings": [candidate(&map.blocks[1].locator, "pack:research.core", "The research rationale is underdeveloped.", Some("study examines blood pressure"))]}),
    );
    assert_eq!(validation.findings.len(), 1);
    assert_eq!(validation.rejections.len(), 0);
    let finding = &validation.findings[0];
    assert_eq!(finding.task_id, task.id);
    assert_eq!(finding.task_kind, task.kind);
    assert_eq!(
        finding.manuscript_locators,
        vec![map.blocks[1].locator.clone()]
    );
    assert_eq!(finding.authority_references.len(), 1);
    assert_eq!(finding.evidence[0].excerpt, "study examines blood pressure");
}

#[test]
fn valid_locator_and_authority_are_accepted() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let validation = validate(
        &task,
        &map,
        &stack,
        json!({"findings": [candidate(&map.blocks[1].locator, "pack:research.core", "A supported concern", None)]}),
    );
    assert_eq!(validation.findings.len(), 1);
}

#[test]
fn out_of_scope_and_nonexistent_locators_are_rejected() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let mut nonexistent = map.blocks[1].locator.clone();
    nonexistent.block_id = "missing".to_owned();
    let validation = validate(
        &task,
        &map,
        &stack,
        json!({"findings": [
            candidate(&map.blocks[3].locator, "pack:research.core", "Out of scope", None),
            candidate(&nonexistent, "pack:research.core", "Missing target", None)
        ]}),
    );
    assert_eq!(validation.findings.len(), 0);
    assert_eq!(validation.rejections.len(), 2);
    assert!(
        validation
            .rejections
            .iter()
            .any(|item| item.reason.contains("scope"))
    );
    assert!(
        validation
            .rejections
            .iter()
            .any(|item| item.reason.contains("does not exist"))
    );
}

#[test]
fn invented_or_unrouted_authority_is_rejected() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let validation = validate(
        &task,
        &map,
        &stack,
        json!({"findings": [candidate(&map.blocks[1].locator, "pack:editorial.vi", "Unsupported authority", None)]}),
    );
    assert_eq!(validation.findings.len(), 0);
    assert_eq!(validation.rejections.len(), 1);
    assert!(validation.rejections[0].reason.contains("not routed"));
}

#[test]
fn grounded_evidence_is_accepted_and_fabricated_evidence_is_rejected() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let valid = validate(
        &task,
        &map,
        &stack,
        json!({"findings": [candidate(&map.blocks[1].locator, "pack:research.core", "Grounded concern", Some("blood pressure"))]}),
    );
    assert_eq!(valid.findings.len(), 1);
    let fabricated = validate(
        &task,
        &map,
        &stack,
        json!({"findings": [candidate(&map.blocks[1].locator, "pack:research.core", "Fabricated concern", Some("invented manuscript quotation"))]}),
    );
    assert_eq!(fabricated.findings.len(), 0);
    assert!(fabricated.rejections[0].reason.contains("not grounded"));
}

#[test]
fn empty_statement_is_rejected() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let validation = validate(
        &task,
        &map,
        &stack,
        json!({"findings": [candidate(&map.blocks[1].locator, "pack:research.core", "", None)]}),
    );
    assert_eq!(validation.findings.len(), 0);
    assert!(validation.rejections[0].reason.contains("statement"));
}

#[test]
fn stale_document_task_version_is_rejected() {
    let map = map();
    let stack = stack();
    let mut task = pack_task(&map, &stack);
    task.target.document_version += 1;
    let error = validate_review_task_response(
        "openai",
        &openai_response(json!({"findings": []})),
        &task,
        &map,
        &stack,
    )
    .unwrap_err();
    assert!(error.to_string().contains("version"));
}

#[test]
fn invalid_candidate_does_not_destroy_independent_valid_candidate() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let validation = validate(
        &task,
        &map,
        &stack,
        json!({"findings": [
            candidate(&map.blocks[1].locator, "pack:research.core", "Valid concern", None),
            candidate(&map.blocks[1].locator, "pack:not-routed", "Invalid concern", None)
        ]}),
    );
    assert_eq!(validation.findings.len(), 1);
    assert_eq!(validation.rejections.len(), 1);
}

#[test]
fn zero_findings_is_valid() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let validation = validate(&task, &map, &stack, json!({"findings": []}));
    assert!(validation.findings.is_empty());
    assert!(validation.rejections.is_empty());
}

#[test]
fn validation_is_deterministic() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let response = json!({"findings": [candidate(&map.blocks[1].locator, "pack:research.core", "Stable concern", Some("adults"))]});
    let first = validate(&task, &map, &stack, response.clone());
    let second = validate(&task, &map, &stack, response);
    assert_eq!(first, second);
    assert_eq!(first.findings[0].id, "review.test.semantic:0");
}

#[tokio::test(flavor = "current_thread")]
async fn executor_uses_shared_transport_and_maps_mocked_response_to_findings() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let response_body = openai_response(json!({
        "findings": [candidate(
            &map.blocks[1].locator,
            "pack:research.core",
            "Mocked executor concern",
            Some("blood pressure")
        )]
    }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut chunk = [0_u8; 4096];
            let bytes_read = stream.read(&mut chunk).await.unwrap();
            if bytes_read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..bytes_read]);
            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let content_length = String::from_utf8_lossy(&request[..header_end])
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or_default();
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_body.len()
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(&response_body).await.unwrap();
    });

    unsafe {
        std::env::set_var("NINEPROFS_REVIEW_EXECUTION_TEST_KEY", "test-key");
    }
    let config = nineprofs_structured_model::StructuredModelConfig {
        provider: "openai".to_owned(),
        model: "mock-model".to_owned(),
        base_url: Some(format!("http://{address}/v1")),
        api_key_env: "NINEPROFS_REVIEW_EXECUTION_TEST_KEY".to_owned(),
        timeout: Duration::from_secs(5),
        max_response_bytes: 256 * 1024,
        max_output_tokens: 1_024,
    };
    let result = nineprofs_research::ReviewTaskExecutor::new(config)
        .execute(&task, &map, &stack)
        .await
        .unwrap();
    unsafe {
        std::env::remove_var("NINEPROFS_REVIEW_EXECUTION_TEST_KEY");
    }
    server.await.unwrap();

    assert_eq!(result.provider, "openai");
    assert_eq!(result.model, "mock-model");
    assert_eq!(result.raw_candidate_count, 1);
    assert_eq!(result.findings.len(), 1);
    assert_eq!(result.findings[0].task_id, task.id);
}

#[test]
fn regulation_requirement_reference_must_exist_in_resolved_stack() {
    let map = map();
    let stack = stack();
    let task = pack_task(&map, &stack);
    let mut task_with_invented_requirement = task.clone();
    task_with_invented_requirement.authority_references.push(
        ReviewAuthorityReference::RegulationRequirement {
            reference: nineprofs_research::RegulationRequirementReference {
                requirement_id: nineprofs_research::RegulationRequirementId::parse(
                    "invented-1".to_owned(),
                )
                .unwrap(),
                source_id: nineprofs_research::ResearchSourceId::parse(
                    "source-invented-1".to_owned(),
                )
                .unwrap(),
                source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::parse(
                    "snapshot-invented-1".to_owned(),
                )
                .unwrap(),
                authority_locator: Some(EvidenceLocator::TextRange { start: 0, end: 1 }),
                normalized_requirement: "invented requirement".to_owned(),
            },
        },
    );
    let error = validate_review_task_response(
        "openai",
        &openai_response(json!({"findings": []})),
        &task_with_invented_requirement,
        &map,
        &stack,
    )
    .unwrap_err();
    assert!(error.to_string().contains("unknown regulation requirement"));
}

#[test]
fn valid_regulation_requirement_reference_is_accepted() {
    let map = map();
    let stack = stack_with_requirement();
    let task = requirement_task(&map, &stack);
    let validation = validate(
        &task,
        &map,
        &stack,
        json!({"findings": [candidate(&map.blocks[1].locator, "requirement:format-1", "The study design is not stated clearly.", None)]}),
    );
    assert_eq!(validation.findings.len(), 1);
    assert_eq!(validation.rejections.len(), 0);
}
