use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

use nineprofs_officecli::{
    ArtifactResolver, DocumentReference, GetRequest, IssuesRequest, OfficeCliAvailability,
    OfficeCliCancellation, OfficeCliConfig, OfficeCliError, OfficeCliOperation, OfficeCliRunner,
    QueryRequest, SUPPORTED_VERSION, ScreenshotRequest, ValidateRequest, ViewRequest,
};

const QUALIFICATION_ENV: &str = "NINEPROFS_OFFICECLI_QUALIFICATION";

#[tokio::test]
async fn pinned_officecli_real_qualification() {
    if std::env::var_os(QUALIFICATION_ENV).as_deref() != Some(std::ffi::OsStr::new("1")) {
        eprintln!("skipped: set {QUALIFICATION_ENV}=1 for explicit real qualification");
        return;
    }

    let config = OfficeCliConfig::from_env();
    let binary = config
        .binary_path
        .as_deref()
        .expect("qualification requires NINEPROFS_OFFICECLI_PATH");
    assert!(
        binary.is_file(),
        "configured OfficeCLI binary is unavailable"
    );
    assert_eq!(command_version(&config, binary), SUPPORTED_VERSION);

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let docx = repo_root.join("fixtures/generated/simple.docx");
    let pptx = repo_root.join("packages/pptx-engine/tests/fixtures/01_standard_business.pptx");
    assert!(docx.is_file(), "DOCX qualification fixture is unavailable");
    assert!(pptx.is_file(), "PPTX qualification fixture is unavailable");
    fs::create_dir_all(&config.artifact_root).unwrap();
    let xlsx = config.artifact_root.join("qualification-source.xlsx");
    create_xlsx_fixture(&config, binary, &xlsx);
    let invalid_docx = config.artifact_root.join("qualification-invalid.docx");
    fs::write(&invalid_docx, b"not an Office document").unwrap();

    let runner = OfficeCliRunner::initialize(config.clone()).await;
    let status = runner.status();
    assert_eq!(status.availability, OfficeCliAvailability::Available);
    assert_eq!(status.detected_version.as_deref(), Some(SUPPORTED_VERSION));
    assert!(runner.can_render(), "9Profs HTML rasterizer is unavailable");

    let resolver = Arc::new(ArtifactResolver::new([
        docx.parent().unwrap().to_path_buf(),
        xlsx.parent().unwrap().to_path_buf(),
        pptx.parent().unwrap().to_path_buf(),
    ]));
    resolver
        .register_detached("qualification-docx", &docx)
        .unwrap();
    resolver
        .register_detached("qualification-xlsx", &xlsx)
        .unwrap();
    resolver
        .register_detached("qualification-pptx", &pptx)
        .unwrap();
    resolver
        .register_detached("qualification-invalid", &invalid_docx)
        .unwrap();

    qualify_document(
        &runner,
        resolver.as_ref(),
        &docx,
        "qualification-docx",
        "/body",
        &config.artifact_root,
        1,
    )
    .await;
    qualify_document(
        &runner,
        resolver.as_ref(),
        &xlsx,
        "qualification-xlsx",
        "/",
        &config.artifact_root,
        1,
    )
    .await;
    qualify_document(
        &runner,
        resolver.as_ref(),
        &pptx,
        "qualification-pptx",
        "/slide[1]",
        &config.artifact_root,
        1,
    )
    .await;

    let mut timeout_config = config.clone();
    timeout_config.timeout = Duration::from_millis(1);
    let timeout_runner = OfficeCliRunner::initialize(timeout_config).await;
    let timeout = timeout_runner
        .execute_readonly(
            OfficeCliOperation::ViewText(ViewRequest {
                document: reference("qualification-docx"),
                start: None,
                end: None,
                limit: Some(50),
            }),
            resolver.as_ref(),
            None,
        )
        .await;
    assert!(matches!(timeout, Err(OfficeCliError::Timeout)));

    let cancellation = OfficeCliCancellation::new();
    cancellation.cancel();
    let cancelled = runner
        .execute_readonly(
            OfficeCliOperation::ViewText(ViewRequest {
                document: reference("qualification-docx"),
                start: None,
                end: None,
                limit: None,
            }),
            resolver.as_ref(),
            Some(cancellation),
        )
        .await;
    assert!(matches!(cancelled, Err(OfficeCliError::Cancelled)));

    let failed_screenshot = runner
        .execute_readonly(
            OfficeCliOperation::Screenshot(ScreenshotRequest {
                document: reference("qualification-invalid"),
                page: None,
                width: None,
                height: None,
            }),
            resolver.as_ref(),
            None,
        )
        .await;
    assert!(matches!(
        failed_screenshot,
        Err(OfficeCliError::ProcessFailed | OfficeCliError::ArtifactOutputUnavailable)
    ));
}

async fn qualify_document(
    runner: &OfficeCliRunner,
    resolver: &ArtifactResolver,
    path: &Path,
    id: &str,
    selector: &str,
    artifact_root: &Path,
    minimum_artifacts: usize,
) {
    let operations = [
        OfficeCliOperation::ViewText(ViewRequest {
            document: reference(id),
            start: None,
            end: None,
            limit: Some(50),
        }),
        OfficeCliOperation::ViewAnnotated(ViewRequest {
            document: reference(id),
            start: None,
            end: None,
            limit: Some(50),
        }),
        OfficeCliOperation::ViewOutline(ViewRequest {
            document: reference(id),
            start: None,
            end: None,
            limit: None,
        }),
        OfficeCliOperation::ViewStats(ViewRequest {
            document: reference(id),
            start: None,
            end: None,
            limit: None,
        }),
        OfficeCliOperation::ViewIssues(IssuesRequest {
            document: reference(id),
            issue_type: None,
            limit: Some(50),
        }),
        OfficeCliOperation::Get(GetRequest {
            document: reference(id),
            selector: selector.to_owned(),
        }),
        OfficeCliOperation::Query(QueryRequest {
            document: reference(id),
            selector: "*".to_owned(),
        }),
        OfficeCliOperation::Validate(ValidateRequest {
            document: reference(id),
        }),
        OfficeCliOperation::Screenshot(ScreenshotRequest {
            document: reference(id),
            page: Some(1),
            width: None,
            height: None,
        }),
    ];

    for (operation_index, operation) in operations.into_iter().enumerate() {
        let before = fs::read(path).unwrap();
        let response = runner
            .execute_readonly(operation, resolver, None)
            .await
            .unwrap_or_else(|error| {
                panic!("real OfficeCLI operation #{operation_index} failed: {error:?}")
            });
        assert_eq!(
            before,
            fs::read(path).unwrap(),
            "read-only operation changed bytes"
        );
        assert!(response.data.is_object() || response.data.is_string());
        if response.operation == "screenshot" {
            assert!(response.artifact.is_some());
            assert!(response.artifacts.len() >= minimum_artifacts);
            assert!(!response.artifacts.is_empty());
            let root = fs::canonicalize(artifact_root).unwrap();
            for artifact in &response.artifacts {
                let output = artifact_root.join(format!("{}.png", artifact.id));
                let canonical = fs::canonicalize(&output).unwrap();
                assert!(canonical.starts_with(&root), "PNG escaped artifact root");
                let bytes = fs::read(&canonical).unwrap();
                assert!(bytes.len() > 100, "PNG artifact is empty or trivial");
                assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
                let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
                let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
                assert!(width > 0 && height > 0, "PNG dimensions are empty");
            }
        }
    }
}

fn reference(id: &str) -> DocumentReference {
    DocumentReference {
        artifact_id: id.to_owned(),
    }
}

fn isolated_command(config: &OfficeCliConfig, binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env("OFFICECLI_NO_AUTO_INSTALL", "1")
        .env("OFFICECLI_NO_AUTO_RESIDENT", "1")
        .env("OFFICECLI_SKIP_UPDATE", "1")
        .env("HOME", &config.profile_root)
        .env("USERPROFILE", &config.profile_root)
        .env("APPDATA", config.profile_root.join("appdata"))
        .env("LOCALAPPDATA", config.profile_root.join("localappdata"))
        .env("XDG_CONFIG_HOME", config.profile_root.join("config"))
        .env("XDG_CACHE_HOME", config.profile_root.join("cache"));
    for key in ["PATH", "SystemRoot", "SYSTEMROOT", "COMSPEC"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
}

fn command_version(config: &OfficeCliConfig, binary: &Path) -> String {
    let output = isolated_command(config, binary)
        .arg("--version")
        .output()
        .expect("OfficeCLI --version must start");
    assert!(output.status.success(), "OfficeCLI --version failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn create_xlsx_fixture(config: &OfficeCliConfig, binary: &Path, path: &Path) {
    let _ = fs::remove_file(path);
    let output = isolated_command(config, binary)
        .args(["--json", "create"])
        .arg(path)
        .args(["--locale", "en-US"])
        .output()
        .expect("OfficeCLI XLSX fixture creation must start");
    assert!(
        output.status.success(),
        "OfficeCLI XLSX fixture creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let close = isolated_command(config, binary)
        .args(["--json", "close"])
        .arg(path)
        .output()
        .expect("OfficeCLI XLSX fixture close must start");
    assert!(
        close.status.success(),
        "OfficeCLI XLSX fixture close failed: {}",
        String::from_utf8_lossy(&close.stderr)
    );
    assert!(path.is_file(), "OfficeCLI XLSX fixture was not created");
}
