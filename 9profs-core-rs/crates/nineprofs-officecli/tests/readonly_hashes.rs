use std::{fs, path::PathBuf, sync::Arc};

use nineprofs_officecli::{
    ArtifactResolver, DocumentReference, GetRequest, IssuesRequest, OfficeCliConfig,
    OfficeCliOperation, OfficeCliRunner, QueryRequest, ValidateRequest, ViewRequest,
};

#[tokio::test]
async fn docx_read_only_operations_preserve_bytes() {
    let Some(_) = std::env::var_os("NINEPROFS_OFFICECLI_PATH") else {
        eprintln!("skipped: NINEPROFS_OFFICECLI_PATH is not configured");
        return;
    };
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("fixtures/generated/simple.docx");
    if !fixture.is_file() {
        eprintln!("skipped: DOCX fixture is unavailable");
        return;
    }

    let resolver = Arc::new(ArtifactResolver::new([fixture
        .parent()
        .unwrap()
        .to_path_buf()]));
    resolver.register_detached("simple-docx", &fixture).unwrap();
    let runner = OfficeCliRunner::initialize(OfficeCliConfig::from_env()).await;
    if !runner.is_available() {
        eprintln!("skipped: configured OfficeCLI is unavailable or version-mismatched");
        return;
    }

    let document = DocumentReference {
        artifact_id: "simple-docx".to_owned(),
    };
    let operations = [
        OfficeCliOperation::ViewText(ViewRequest {
            document: document.clone(),
            start: None,
            end: None,
            limit: Some(50),
        }),
        OfficeCliOperation::ViewAnnotated(ViewRequest {
            document: document.clone(),
            start: None,
            end: None,
            limit: Some(50),
        }),
        OfficeCliOperation::ViewOutline(ViewRequest {
            document: document.clone(),
            start: None,
            end: None,
            limit: None,
        }),
        OfficeCliOperation::ViewStats(ViewRequest {
            document: document.clone(),
            start: None,
            end: None,
            limit: None,
        }),
        OfficeCliOperation::ViewIssues(IssuesRequest {
            document: document.clone(),
            issue_type: None,
            limit: Some(50),
        }),
        OfficeCliOperation::Get(GetRequest {
            document: document.clone(),
            selector: "/body".to_owned(),
        }),
        OfficeCliOperation::Query(QueryRequest {
            document: document.clone(),
            selector: "/body".to_owned(),
            limit: Some(50),
        }),
        OfficeCliOperation::Validate(ValidateRequest { document }),
    ];

    for operation in operations {
        let before = fs::read(&fixture).unwrap();
        runner
            .execute_readonly(operation, resolver.as_ref(), None)
            .await
            .unwrap();
        let after = fs::read(&fixture).unwrap();
        assert_eq!(before, after, "read-only operation changed fixture bytes");
    }
}
