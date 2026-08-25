use std::{collections::BTreeMap, fs, sync::Arc};

use nineprofs_officecli::{
    ArtifactResolver, CreateDocumentRequest, DetachedMutationRequest, DetachedMutationService,
    DocumentReference, DocumentResolver, OfficeCliAvailability, OfficeCliConfig, OfficeCliRunner,
    OfficeDocumentType, OfficeMutation, SUPPORTED_VERSION,
};

const QUALIFICATION_ENV: &str = "NINEPROFS_OFFICECLI_MUTATION_QUALIFICATION";

#[tokio::test]
async fn pinned_officecli_create_and_detached_mutation_qualification() {
    if std::env::var_os(QUALIFICATION_ENV).as_deref() != Some(std::ffi::OsStr::new("1")) {
        eprintln!("skipped: set {QUALIFICATION_ENV}=1 for explicit real qualification");
        return;
    }

    let config = OfficeCliConfig::from_env();
    assert!(
        config
            .binary_path
            .as_deref()
            .is_some_and(|path| path.is_file()),
        "qualification requires NINEPROFS_OFFICECLI_PATH"
    );
    let runner = Arc::new(OfficeCliRunner::initialize(config.clone()).await);
    assert_eq!(
        runner.status().availability,
        OfficeCliAvailability::Available
    );
    assert_eq!(
        runner.status().detected_version.as_deref(),
        Some(SUPPORTED_VERSION)
    );
    assert!(runner.can_render(), "9Profs HTML rasterizer is unavailable");

    let root = config.artifact_root.clone();
    fs::create_dir_all(&root).unwrap();
    let resolver = Arc::new(ArtifactResolver::new([root.clone()]));
    let service = DetachedMutationService::new(runner, resolver.clone());

    qualify_docx(&service, &resolver).await;
    qualify_xlsx(&service, &resolver).await;
    qualify_pptx(&service, &resolver).await;
}

async fn qualify_docx(service: &DetachedMutationService, resolver: &ArtifactResolver) {
    let created = service
        .create(
            CreateDocumentRequest {
                document_type: OfficeDocumentType::Docx,
                logical_name: Some("mutation-qualification-docx".to_owned()),
                operations: vec![OfficeMutation::Add {
                    parent: "/body".to_owned(),
                    element_type: "p".to_owned(),
                    properties: properties([("text", "Initial DOCX")]),
                }],
            },
            None,
        )
        .await
        .expect("DOCX create, validate, and render must pass");
    qualify_mutation(
        service,
        resolver,
        created.revision.reference,
        OfficeMutation::Set {
            selector: "/body/p[1]".to_owned(),
            properties: properties([("text", "Mutated DOCX")]),
        },
        "DOCX",
    )
    .await;
}

async fn qualify_xlsx(service: &DetachedMutationService, resolver: &ArtifactResolver) {
    let created = service
        .create(
            CreateDocumentRequest {
                document_type: OfficeDocumentType::Xlsx,
                logical_name: Some("mutation-qualification-xlsx".to_owned()),
                operations: vec![OfficeMutation::Set {
                    selector: "/Sheet1/A1".to_owned(),
                    properties: properties([("value", "Initial XLSX")]),
                }],
            },
            None,
        )
        .await
        .expect("XLSX create, validate, and render must pass");
    qualify_mutation(
        service,
        resolver,
        created.revision.reference,
        OfficeMutation::Set {
            selector: "/Sheet1/A1".to_owned(),
            properties: properties([("value", "Mutated XLSX")]),
        },
        "XLSX",
    )
    .await;
}

async fn qualify_pptx(service: &DetachedMutationService, resolver: &ArtifactResolver) {
    let created = service
        .create(
            CreateDocumentRequest {
                document_type: OfficeDocumentType::Pptx,
                logical_name: Some("mutation-qualification-pptx".to_owned()),
                operations: vec![OfficeMutation::Add {
                    parent: "/".to_owned(),
                    element_type: "slide".to_owned(),
                    properties: properties([("title", "Initial PPTX")]),
                }],
            },
            None,
        )
        .await
        .expect("PPTX create, validate, and render must pass");
    qualify_mutation(
        service,
        resolver,
        created.revision.reference,
        OfficeMutation::Set {
            selector: "/slide[1]/shape[1]".to_owned(),
            properties: properties([("text", "Mutated PPTX")]),
        },
        "PPTX",
    )
    .await;
}

async fn qualify_mutation(
    service: &DetachedMutationService,
    resolver: &ArtifactResolver,
    base: DocumentReference,
    operation: OfficeMutation,
    label: &str,
) {
    let base_path = resolver.resolve(&base).unwrap().path;
    let before = fs::read(&base_path).unwrap();
    let result = service
        .mutate_detached(
            DetachedMutationRequest {
                document: base,
                operations: vec![operation],
                base_revision_id: None,
            },
            None,
        )
        .await
        .unwrap_or_else(|error| panic!("{label} detached mutation must pass: {error}"));
    assert_eq!(
        before,
        fs::read(&base_path).unwrap(),
        "{label} base changed"
    );
    assert_eq!(result.operations_applied, 1);
    assert!(!result.render.artifacts.is_empty());
    let revision_path = resolver.resolve(&result.revision.reference).unwrap().path;
    assert_ne!(
        before,
        fs::read(revision_path).unwrap(),
        "{label} did not change"
    );
}

fn properties<const N: usize>(items: [(&str, &str); N]) -> BTreeMap<String, String> {
    items
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}
