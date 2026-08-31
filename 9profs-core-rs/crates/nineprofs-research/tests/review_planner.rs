use nineprofs_research::{
    ContentHash, DocumentMap, DocumentMapBlock, DocumentMapBlockKind, DocumentMapLocator,
    DocumentMapSection, EvidenceLocator, HashAlgorithm, RegulationApplicability,
    RegulationRequirement, RegulationReviewStatus, ResearchContext, ResearchSourceId,
    ResearchSourceSnapshotId, ReviewAuthorityReference, ReviewExecutorMode, ReviewSectionRole,
    classify_heading_role, load_canonical_authority_packs, plan_review_tasks, resolve_review_stack,
};

fn target_context() -> ResearchContext {
    ResearchContext {
        language: Some("vi".to_owned()),
        research_families: vec!["MED".to_owned()],
        artifact_type: Some("master_thesis".to_owned()),
        academic_level: Some("master".to_owned()),
        organization: Some("hiu".to_owned()),
        ..ResearchContext::default()
    }
}

fn applicability(entries: &[(&str, &str)]) -> RegulationApplicability {
    RegulationApplicability {
        facets: entries
            .iter()
            .map(|(facet, value)| ((*facet).to_owned(), vec![(*value).to_owned()]))
            .collect(),
    }
}

fn requirement(
    id: &str,
    status: RegulationReviewStatus,
    active: bool,
    requirement_applicability: RegulationApplicability,
    effective_from: Option<i64>,
    effective_until: Option<i64>,
) -> RegulationRequirement {
    RegulationRequirement {
        id: nineprofs_research::RegulationRequirementId::parse(id.to_owned()).unwrap(),
        source_id: ResearchSourceId::parse(format!("source-{id}")).unwrap(),
        source_snapshot_id: ResearchSourceSnapshotId::parse(format!("snapshot-{id}")).unwrap(),
        pdf_extraction_id: None,
        text: format!("normalized requirement {id}"),
        source_excerpt: format!("source excerpt {id}"),
        source_excerpt_hash: ContentHash {
            algorithm: HashAlgorithm::Sha256,
            value: format!("hash-{id}"),
        },
        source_locator: EvidenceLocator::TextRange { start: 0, end: 10 },
        authority_locator: Some(EvidenceLocator::Regulation {
            article: format!("Article {id}"),
            section: Some("presentation".to_owned()),
            clause: Some("1".to_owned()),
        }),
        applicability: requirement_applicability,
        effective_from,
        effective_until,
        extraction_method: "fixture".to_owned(),
        extraction_contract_version: Some("fixture-v1".to_owned()),
        review_status: status,
        active,
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

fn map_with_headings(headings: &[&str]) -> DocumentMap {
    let mut sections = Vec::new();
    let mut blocks = Vec::new();
    for (section_ordinal, heading) in headings.iter().enumerate() {
        let heading_ordinal = (section_ordinal * 2) as u32;
        let body_ordinal = heading_ordinal + 1;
        let heading_id = format!("b{heading_ordinal}");
        let body_id = format!("b{body_ordinal}");
        let section_id = format!("section:{heading_id}");
        let heading_locator = DocumentMapLocator {
            document_id: "fixture-08-multi-section-paper".to_owned(),
            version: 0,
            block_id: heading_id.clone(),
            block_ordinal: heading_ordinal,
            docx_index: Some(heading_ordinal),
            section_id: Some(section_id.clone()),
        };
        let body_locator = DocumentMapLocator {
            document_id: "fixture-08-multi-section-paper".to_owned(),
            version: 0,
            block_id: body_id.clone(),
            block_ordinal: body_ordinal,
            docx_index: Some(body_ordinal),
            section_id: Some(section_id.clone()),
        };
        sections.push(DocumentMapSection {
            id: section_id.clone(),
            heading_text: (*heading).to_owned(),
            level: 1,
            parent_id: None,
            locator: heading_locator.clone(),
            block_ids: vec![heading_id.clone(), body_id.clone()],
            is_deleted: false,
        });
        blocks.push(DocumentMapBlock {
            id: heading_id,
            ordinal: heading_ordinal,
            kind: DocumentMapBlockKind::Heading,
            text: (*heading).to_owned(),
            locator: heading_locator,
            section_id: Some(section_id.clone()),
            heading_level: Some(1),
            caption: None,
            is_deleted: false,
        });
        blocks.push(DocumentMapBlock {
            id: body_id,
            ordinal: body_ordinal,
            kind: DocumentMapBlockKind::Paragraph,
            text: "bounded section content".to_owned(),
            locator: body_locator,
            section_id: Some(section_id),
            heading_level: None,
            caption: None,
            is_deleted: false,
        });
    }
    DocumentMap {
        contract_version: nineprofs_research::DOCUMENT_MAP_CONTRACT_VERSION.to_owned(),
        document_id: "fixture-08-multi-section-paper".to_owned(),
        version: 0,
        sections,
        blocks,
        tables: Vec::new(),
        figures: Vec::new(),
        citations: Vec::new(),
        references: Vec::new(),
    }
}

#[test]
fn canonical_packs_load_with_manifest_and_markdown_identity() {
    let packs = load_canonical_authority_packs().unwrap();
    let ids = packs
        .iter()
        .map(|pack| pack.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "artifact.master-thesis",
            "domain.med",
            "editorial.vi",
            "research.core"
        ]
    );
    assert!(packs.iter().all(|pack| {
        !pack.version.is_empty()
            && !pack.source.manifest_hash.value.is_empty()
            && pack
                .knowledge
                .iter()
                .chain(pack.review_guidance.iter())
                .all(|document| {
                    !document.content.trim().is_empty() && !document.content_hash.value.is_empty()
                })
    }));
}

#[test]
fn target_context_resolves_expected_packs_and_filters_irrelevant_applicability() {
    let packs = load_canonical_authority_packs().unwrap();
    let resolved = nineprofs_research::resolve_authority_packs(&packs, &target_context()).unwrap();
    assert_eq!(
        resolved
            .iter()
            .map(|pack| pack.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "artifact.master-thesis",
            "domain.med",
            "editorial.vi",
            "research.core"
        ]
    );

    let irrelevant = ResearchContext {
        language: Some("en".to_owned()),
        research_families: vec!["LAW".to_owned()],
        artifact_type: Some("doctoral_dissertation".to_owned()),
        ..ResearchContext::default()
    };
    let resolved = nineprofs_research::resolve_authority_packs(&packs, &irrelevant).unwrap();
    assert_eq!(
        resolved
            .iter()
            .map(|pack| pack.id.as_str())
            .collect::<Vec<_>>(),
        vec!["research.core"]
    );
}

#[test]
fn stack_uses_existing_effective_regulation_resolver() {
    let context = target_context();
    let matching = applicability(&[
        ("organization", "hiu"),
        ("artifact_type", "master_thesis"),
        ("research_family", "MED"),
    ]);
    let requirements = vec![
        requirement(
            "good",
            RegulationReviewStatus::Approved,
            true,
            matching.clone(),
            Some(10),
            Some(20),
        ),
        requirement(
            "needs-review",
            RegulationReviewStatus::NeedsReview,
            false,
            matching.clone(),
            None,
            None,
        ),
        requirement(
            "inactive",
            RegulationReviewStatus::Approved,
            false,
            matching.clone(),
            None,
            None,
        ),
        requirement(
            "wrong-context",
            RegulationReviewStatus::Approved,
            true,
            applicability(&[("organization", "other")]),
            None,
            None,
        ),
        requirement(
            "future",
            RegulationReviewStatus::Approved,
            true,
            matching.clone(),
            Some(21),
            None,
        ),
        requirement(
            "expired",
            RegulationReviewStatus::Approved,
            true,
            matching,
            None,
            Some(9),
        ),
    ];
    let stack = resolve_review_stack(
        &context,
        &load_canonical_authority_packs().unwrap(),
        &requirements,
        15,
    )
    .unwrap();
    assert_eq!(
        stack
            .regulation_requirements
            .iter()
            .map(|requirement| requirement.id.as_str())
            .collect::<Vec<_>>(),
        vec!["good"]
    );
}

#[test]
fn planner_routes_deterministically_to_small_coarse_tasks_with_provenance() {
    let context = target_context();
    let requirements = vec![requirement(
        "format-1",
        RegulationReviewStatus::Approved,
        true,
        applicability(&[("organization", "hiu")]),
        None,
        None,
    )];
    let stack = resolve_review_stack(
        &context,
        &load_canonical_authority_packs().unwrap(),
        &requirements,
        15,
    )
    .unwrap();
    let map = map_with_headings(&[
        "CHƯƠNG 1. TỔNG QUAN",
        "CHƯƠNG 2. PHƯƠNG PHÁP NGHIÊN CỨU",
        "CHƯƠNG 3. KẾT QUẢ",
        "CHƯƠNG 4. BÀN LUẬN",
        "CHƯƠNG 5. KẾT LUẬN",
    ]);

    let first = plan_review_tasks(&context, &map, &stack).unwrap();
    let second = plan_review_tasks(&context, &map, &stack).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.len(), 9);
    assert!(first.len() < 12);
    assert_eq!(
        first
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "review.manuscript.research-coherence",
            "review.section.methodology.section:b2",
            "review.section.results.section:b4",
            "review.section.discussion.section:b6",
            "review.section.conclusion.section:b8",
            "review.manuscript.cross-section",
            "review.vi.language",
            "review.vi.terminology",
            "review.regulation.presentation",
        ]
    );
    assert!(first.iter().all(|task| {
        task.contract_version == nineprofs_research::REVIEW_TASK_CONTRACT_VERSION
            && task.target.document_map_contract_version
                == nineprofs_research::DOCUMENT_MAP_CONTRACT_VERSION
            && task.target.document_id == map.document_id
            && task.target.document_version == map.version
            && task.target.locators.iter().all(|locator| {
                locator.document_id == map.document_id && locator.version == map.version
            })
    }));

    let methodology = first
        .iter()
        .find(|task| task.kind == "review.section.methodology")
        .unwrap();
    assert_eq!(methodology.target.locators.len(), 2);
    assert_eq!(methodology.executor_mode, ReviewExecutorMode::Semantic);
    let methodology_pack_ids = methodology
        .authority_references
        .iter()
        .filter_map(|reference| match reference {
            ReviewAuthorityReference::AuthorityPack { pack_id, .. } => Some(pack_id.as_str()),
            ReviewAuthorityReference::RegulationRequirement { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        methodology_pack_ids,
        vec!["research.core", "domain.med", "artifact.master-thesis"]
    );

    let language = first
        .iter()
        .find(|task| task.id == "review.vi.language")
        .unwrap();
    let language_pack_ids = language
        .authority_references
        .iter()
        .filter_map(|reference| match reference {
            ReviewAuthorityReference::AuthorityPack { pack_id, .. } => Some(pack_id.as_str()),
            ReviewAuthorityReference::RegulationRequirement { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        language_pack_ids,
        vec!["editorial.vi", "artifact.master-thesis"]
    );

    let regulation = first
        .iter()
        .find(|task| task.id == "review.regulation.presentation")
        .unwrap();
    assert_eq!(regulation.executor_mode, ReviewExecutorMode::Hybrid);
    assert!(matches!(
        regulation.authority_references.as_slice(),
        [ReviewAuthorityReference::RegulationRequirement { reference }] if reference.requirement_id.as_str() == "format-1"
    ));
}

#[test]
fn heading_routing_is_deterministic_and_unknown_headings_fail_safe() {
    assert_eq!(
        classify_heading_role("  CHƯƠNG 2. PHƯƠNG PHÁP NGHIÊN CỨU  "),
        ReviewSectionRole::Methodology
    );
    assert_eq!(
        classify_heading_role("3. KẾT QUẢ"),
        ReviewSectionRole::Results
    );
    assert_eq!(
        classify_heading_role("Bàn luận"),
        ReviewSectionRole::Discussion
    );
    assert_eq!(
        classify_heading_role("Kết luận"),
        ReviewSectionRole::Conclusion
    );
    assert_eq!(
        classify_heading_role("CHƯƠNG 6. PHỤ LỤC"),
        ReviewSectionRole::Unclassified
    );

    let context = target_context();
    let stack = resolve_review_stack(
        &context,
        &load_canonical_authority_packs().unwrap(),
        &[],
        15,
    )
    .unwrap();
    let tasks =
        plan_review_tasks(&context, &map_with_headings(&["CHƯƠNG 6. PHỤ LỤC"]), &stack).unwrap();
    assert_eq!(
        tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "review.manuscript.research-coherence",
            "review.vi.language",
            "review.vi.terminology"
        ]
    );
}

#[test]
fn no_tasks_emitted_for_empty_document_map() {
    let context = target_context();
    let stack = resolve_review_stack(
        &context,
        &load_canonical_authority_packs().unwrap(),
        &[],
        15,
    )
    .unwrap();
    let mut map = map_with_headings(&["Kết luận"]);
    map.blocks.clear();
    map.sections.clear();
    assert!(
        plan_review_tasks(&context, &map, &stack)
            .unwrap()
            .is_empty()
    );
}
