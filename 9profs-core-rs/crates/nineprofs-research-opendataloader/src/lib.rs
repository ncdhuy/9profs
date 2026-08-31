//! Local OpenDataLoader PDF extraction adapter.
//!
//! OpenDataLoader OCR is derived extraction evidence. The immutable research
//! source snapshot remains authoritative regulation text.
//!
//! The configured local hybrid backend is expected to be provisioned with
//! force OCR and `vi,en`; this adapter does not install, start, or upload to it.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use reqwest::Url;
use serde_json::Value;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    time,
};

pub const EXTRACTOR_NAME: &str = "opendataloader-pdf";
const DEFAULT_HYBRID_BACKEND: &str = "docling-fast";
const DEFAULT_HYBRID_MODE: &str = "full";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct OpenDataLoaderConfig {
    pub executable: PathBuf,
    pub extractor_version: Option<String>,
    pub hybrid_backend: String,
    pub hybrid_mode: String,
    pub hybrid_url: Option<String>,
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

impl OpenDataLoaderConfig {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            extractor_version: None,
            hybrid_backend: DEFAULT_HYBRID_BACKEND.to_owned(),
            hybrid_mode: DEFAULT_HYBRID_MODE.to_owned(),
            hybrid_url: None,
            timeout: DEFAULT_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }

    pub fn from_env() -> Option<Self> {
        let executable = std::env::var_os("NINEPROFS_OPENDATALOADER_PATH")?;
        if executable.is_empty() {
            return None;
        }
        let mut config = Self::new(PathBuf::from(executable));
        if let Some(value) = non_empty_env("NINEPROFS_OPENDATALOADER_VERSION") {
            config.extractor_version = Some(value);
        }
        if let Some(value) = non_empty_env("NINEPROFS_OPENDATALOADER_HYBRID_BACKEND") {
            config.hybrid_backend = value;
        }
        if let Some(value) = non_empty_env("NINEPROFS_OPENDATALOADER_HYBRID_MODE") {
            config.hybrid_mode = value;
        }
        config.hybrid_url = non_empty_env("NINEPROFS_OPENDATALOADER_HYBRID_URL");
        if let Some(value) = parse_env_u64("NINEPROFS_OPENDATALOADER_TIMEOUT_MS") {
            config.timeout = Duration::from_millis(value.clamp(100, 600_000));
        }
        if let Some(value) = parse_env_usize("NINEPROFS_OPENDATALOADER_MAX_OUTPUT_BYTES") {
            config.max_output_bytes = value.max(1);
        }
        Some(config)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractedPdfPage {
    pub page_number: u32,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenDataLoaderPdfExtraction {
    pub extractor_version: Option<String>,
    pub page_count: u32,
    pub pages: Vec<ExtractedPdfPage>,
}

#[derive(Debug, Error)]
pub enum OpenDataLoaderError {
    #[error("OpenDataLoader configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("OpenDataLoader process could not start")]
    Spawn(#[source] std::io::Error),
    #[error("OpenDataLoader process failed with exit code {exit_code:?}: {stderr}")]
    ProcessFailed {
        exit_code: Option<i32>,
        stderr: String,
    },
    #[error("OpenDataLoader process timed out")]
    Timeout,
    #[error("OpenDataLoader output exceeded configured limit")]
    OutputTooLarge,
    #[error("OpenDataLoader output is empty")]
    EmptyOutput,
    #[error("OpenDataLoader JSON output is invalid")]
    InvalidJson(#[source] serde_json::Error),
    #[error("OpenDataLoader JSON output is invalid: {0}")]
    InvalidOutput(String),
    #[error("OpenDataLoader output is missing page metadata")]
    MissingPageMetadata,
    #[error("OpenDataLoader output contains invalid page number: {0}")]
    InvalidPageNumber(String),
    #[error("research PDF extraction persistence failed: {0}")]
    Research(#[from] nineprofs_research::ResearchError),
}

#[derive(Clone)]
pub struct OpenDataLoaderPdfProvider {
    config: OpenDataLoaderConfig,
    executor: Arc<dyn ProcessExecutor>,
}

impl OpenDataLoaderPdfProvider {
    pub fn new(config: OpenDataLoaderConfig) -> Result<Self, OpenDataLoaderError> {
        validate_config(&config)?;
        Ok(Self {
            config,
            executor: Arc::new(CommandExecutor),
        })
    }

    pub async fn extract(
        &self,
        pdf_path: impl AsRef<Path>,
    ) -> Result<OpenDataLoaderPdfExtraction, OpenDataLoaderError> {
        let pdf_path = pdf_path.as_ref();
        let output = self
            .executor
            .execute(
                &self.config.executable,
                &command_args(&self.config, pdf_path),
                self.config.timeout,
                self.config.max_output_bytes,
            )
            .await?;
        if output.exit_code != Some(0) {
            return Err(OpenDataLoaderError::ProcessFailed {
                exit_code: output.exit_code,
                stderr: output.stderr,
            });
        }
        if output.truncated {
            return Err(OpenDataLoaderError::OutputTooLarge);
        }
        if output.stdout.iter().all(u8::is_ascii_whitespace) {
            return Err(OpenDataLoaderError::EmptyOutput);
        }
        let mut extraction =
            normalize_json(std::str::from_utf8(&output.stdout).map_err(|_| {
                OpenDataLoaderError::InvalidOutput("output is not UTF-8".to_owned())
            })?)?;
        extraction.extractor_version = self.config.extractor_version.clone();
        Ok(extraction)
    }

    pub async fn extract_and_capture(
        &self,
        pdf_path: impl AsRef<Path>,
        source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId,
        research: &nineprofs_research::ResearchService,
    ) -> Result<nineprofs_research::ResearchPdfExtraction, OpenDataLoaderError> {
        let extraction = self.extract(pdf_path).await?;
        let status = if extraction
            .pages
            .iter()
            .any(|page| !page.text.trim().is_empty())
        {
            nineprofs_research::PdfExtractionStatus::Ready
        } else {
            nineprofs_research::PdfExtractionStatus::NoExtractableText
        };
        Ok(research
            .capture_pdf_extraction(nineprofs_research::CapturePdfExtraction {
                source_snapshot_id,
                extractor: EXTRACTOR_NAME.to_owned(),
                extractor_version: extraction.extractor_version,
                page_count: extraction.page_count,
                status,
                pages: extraction
                    .pages
                    .into_iter()
                    .map(|page| nineprofs_research::CapturePdfPage {
                        page: page.page_number,
                        text: page.text,
                    })
                    .collect(),
            })
            .await?)
    }

    #[cfg(test)]
    fn with_executor(
        config: OpenDataLoaderConfig,
        executor: Arc<dyn ProcessExecutor>,
    ) -> Result<Self, OpenDataLoaderError> {
        validate_config(&config)?;
        Ok(Self { config, executor })
    }
}

pub fn normalize_json(json: &str) -> Result<OpenDataLoaderPdfExtraction, OpenDataLoaderError> {
    let root: Value = serde_json::from_str(json).map_err(OpenDataLoaderError::InvalidJson)?;
    let object = root
        .as_object()
        .ok_or_else(|| OpenDataLoaderError::InvalidOutput("root must be an object".to_owned()))?;
    let page_count = object
        .get("number of pages")
        .ok_or(OpenDataLoaderError::MissingPageMetadata)
        .and_then(|value| parse_page_count(value))?;
    let kids = object
        .get("kids")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            OpenDataLoaderError::InvalidOutput("root kids must be an array".to_owned())
        })?;
    let mut page_texts = vec![Vec::new(); page_count as usize];
    for element in kids {
        visit_element(element, None, page_count, &mut page_texts)?;
    }
    Ok(OpenDataLoaderPdfExtraction {
        extractor_version: None,
        page_count,
        pages: page_texts
            .into_iter()
            .enumerate()
            .map(|(index, parts)| ExtractedPdfPage {
                page_number: index as u32 + 1,
                text: parts.join("\n"),
            })
            .collect(),
    })
}

fn visit_element(
    value: &Value,
    inherited_page: Option<u32>,
    page_count: u32,
    page_texts: &mut [Vec<String>],
) -> Result<(), OpenDataLoaderError> {
    let object = value.as_object().ok_or_else(|| {
        OpenDataLoaderError::InvalidOutput("element must be an object".to_owned())
    })?;
    let page = match object.get("page number") {
        Some(value) => Some(parse_page_number(value, page_count)?),
        None => inherited_page,
    };
    if let Some(content) = object.get("content") {
        let content = content.as_str().ok_or_else(|| {
            OpenDataLoaderError::InvalidOutput("element content must be a string".to_owned())
        })?;
        let page = page.ok_or(OpenDataLoaderError::MissingPageMetadata)?;
        page_texts[page as usize - 1].push(content.to_owned());
    }
    for key in ["kids", "rows", "cells", "list items"] {
        if let Some(children) = object.get(key) {
            let children = children.as_array().ok_or_else(|| {
                OpenDataLoaderError::InvalidOutput(format!("{key} must be an array"))
            })?;
            for child in children {
                visit_element(child, page, page_count, page_texts)?;
            }
        }
    }
    Ok(())
}

fn parse_page_count(value: &Value) -> Result<u32, OpenDataLoaderError> {
    let page_count = value.as_u64().ok_or_else(|| {
        OpenDataLoaderError::InvalidPageNumber("number of pages must be an integer".to_owned())
    })?;
    let page_count = u32::try_from(page_count).map_err(|_| {
        OpenDataLoaderError::InvalidPageNumber("number of pages is out of range".to_owned())
    })?;
    if page_count == 0 {
        return Err(OpenDataLoaderError::InvalidPageNumber(
            "number of pages must be positive".to_owned(),
        ));
    }
    Ok(page_count)
}

fn parse_page_number(value: &Value, page_count: u32) -> Result<u32, OpenDataLoaderError> {
    let page = value.as_u64().ok_or_else(|| {
        OpenDataLoaderError::InvalidPageNumber("page number must be an integer".to_owned())
    })?;
    let page = u32::try_from(page).map_err(|_| {
        OpenDataLoaderError::InvalidPageNumber("page number is out of range".to_owned())
    })?;
    if page == 0 || page > page_count {
        return Err(OpenDataLoaderError::InvalidPageNumber(page.to_string()));
    }
    Ok(page)
}

fn validate_config(config: &OpenDataLoaderConfig) -> Result<(), OpenDataLoaderError> {
    if config.executable.as_os_str().is_empty() {
        return Err(OpenDataLoaderError::InvalidConfiguration(
            "executable path is empty".to_owned(),
        ));
    }
    if config.hybrid_backend.trim().is_empty() {
        return Err(OpenDataLoaderError::InvalidConfiguration(
            "hybrid backend is empty".to_owned(),
        ));
    }
    if !matches!(config.hybrid_mode.as_str(), "full" | "auto") {
        return Err(OpenDataLoaderError::InvalidConfiguration(
            "hybrid mode must be full or auto".to_owned(),
        ));
    }
    if config
        .extractor_version
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(OpenDataLoaderError::InvalidConfiguration(
            "extractor version is empty".to_owned(),
        ));
    }
    if config.timeout.is_zero() || config.max_output_bytes == 0 {
        return Err(OpenDataLoaderError::InvalidConfiguration(
            "timeout and output limit must be positive".to_owned(),
        ));
    }
    if let Some(url) = config.hybrid_url.as_deref() {
        let parsed = Url::parse(url).map_err(|_| {
            OpenDataLoaderError::InvalidConfiguration("hybrid URL is invalid".to_owned())
        })?;
        let is_local = parsed.scheme() == "http"
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
        if !is_local {
            return Err(OpenDataLoaderError::InvalidConfiguration(
                "hybrid URL must point to a localhost HTTP backend".to_owned(),
            ));
        }
    }
    Ok(())
}

fn command_args(config: &OpenDataLoaderConfig, pdf_path: &Path) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("--quiet"),
        OsString::from("--format"),
        OsString::from("json"),
        OsString::from("--to-stdout"),
        OsString::from("--image-output"),
        OsString::from("off"),
        OsString::from("--keep-line-breaks"),
        OsString::from("--hybrid"),
        OsString::from(&config.hybrid_backend),
        OsString::from("--hybrid-mode"),
        OsString::from(&config.hybrid_mode),
    ];
    if let Some(url) = &config.hybrid_url {
        args.extend([OsString::from("--hybrid-url"), OsString::from(url)]);
    }
    args.push(pdf_path.as_os_str().to_owned());
    args
}

#[derive(Debug)]
struct ProcessOutput {
    stdout: Vec<u8>,
    stderr: String,
    exit_code: Option<i32>,
    truncated: bool,
}

#[async_trait]
trait ProcessExecutor: Send + Sync {
    async fn execute(
        &self,
        executable: &Path,
        args: &[OsString],
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Result<ProcessOutput, OpenDataLoaderError>;
}

struct CommandExecutor;

#[async_trait]
impl ProcessExecutor for CommandExecutor {
    async fn execute(
        &self,
        executable: &Path,
        args: &[OsString],
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Result<ProcessOutput, OpenDataLoaderError> {
        let mut child = Command::new(executable)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(OpenDataLoaderError::Spawn)?;
        let stdout = child.stdout.take().ok_or_else(|| {
            OpenDataLoaderError::InvalidOutput("stdout pipe was unavailable".to_owned())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            OpenDataLoaderError::InvalidOutput("stderr pipe was unavailable".to_owned())
        })?;
        let future = async move {
            let stdout_task = tokio::spawn(read_limited(stdout, max_output_bytes));
            let stderr_task = tokio::spawn(read_limited(stderr, max_output_bytes));
            let status = child
                .wait()
                .await
                .map_err(|error| OpenDataLoaderError::InvalidOutput(error.to_string()))?;
            let (stdout, stdout_truncated) = stdout_task.await.map_err(|_| {
                OpenDataLoaderError::InvalidOutput("stdout reader failed".to_owned())
            })??;
            let (stderr, stderr_truncated) = stderr_task.await.map_err(|_| {
                OpenDataLoaderError::InvalidOutput("stderr reader failed".to_owned())
            })??;
            Ok(ProcessOutput {
                stdout,
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                exit_code: status.code(),
                truncated: stdout_truncated || stderr_truncated,
            })
        };
        time::timeout(timeout, future)
            .await
            .map_err(|_| OpenDataLoaderError::Timeout)?
    }
}

async fn read_limited<R>(
    mut reader: R,
    max_output_bytes: usize,
) -> Result<(Vec<u8>, bool), OpenDataLoaderError>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::with_capacity(max_output_bytes.min(8192));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader
            .read(&mut buffer)
            .await
            .map_err(|error| OpenDataLoaderError::InvalidOutput(error.to_string()))?;
        if count == 0 {
            break;
        }
        let remaining = max_output_bytes.saturating_sub(output.len());
        if count > remaining {
            truncated = true;
        }
        output.extend_from_slice(&buffer[..count.min(remaining)]);
    }
    Ok((output, truncated))
}

fn non_empty_env(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    (!value.trim().is_empty()).then_some(value)
}

fn parse_env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse().ok()
}

fn parse_env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use async_trait::async_trait;
    use nineprofs_db::Database;
    use nineprofs_realtime::BroadcastEventBus;
    use nineprofs_research::{
        CreateResearchCase, CreateResearchSource, ResearchArtifactStore, ResearchService,
        SourceKind, SqliteResearchRepository,
    };
    use serde_json::json;

    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/scanned-regulation.json");
    const DUPLICATE_TABLE_ROW_IDS_FIXTURE: &str =
        include_str!("../tests/fixtures/duplicate-table-row-ids.json");

    #[derive(Clone)]
    struct FakeExecutor {
        output: Arc<Mutex<Option<ProcessOutput>>>,
        args: Arc<Mutex<Vec<OsString>>>,
    }

    #[async_trait]
    impl ProcessExecutor for FakeExecutor {
        async fn execute(
            &self,
            _executable: &Path,
            args: &[OsString],
            _timeout: Duration,
            _max_output_bytes: usize,
        ) -> Result<ProcessOutput, OpenDataLoaderError> {
            *self.args.lock().unwrap() = args.to_vec();
            Ok(self.output.lock().unwrap().take().unwrap())
        }
    }

    fn provider_with_output(
        output: ProcessOutput,
    ) -> (OpenDataLoaderPdfProvider, Arc<Mutex<Vec<OsString>>>) {
        let args = Arc::new(Mutex::new(Vec::new()));
        let executor = Arc::new(FakeExecutor {
            output: Arc::new(Mutex::new(Some(output))),
            args: Arc::clone(&args),
        });
        let provider = OpenDataLoaderPdfProvider::with_executor(
            OpenDataLoaderConfig {
                executable: PathBuf::from("configured/opendataloader-pdf"),
                extractor_version: Some("2.5.5".to_owned()),
                hybrid_backend: "docling-fast".to_owned(),
                hybrid_mode: "full".to_owned(),
                hybrid_url: Some("http://127.0.0.1:5502".to_owned()),
                timeout: Duration::from_secs(2),
                max_output_bytes: 1024 * 1024,
            },
            executor,
        )
        .unwrap();
        (provider, args)
    }

    #[test]
    fn normalizes_provider_json_without_changing_policy_tokens() {
        let extraction = normalize_json(FIXTURE).unwrap();
        assert_eq!(extraction.page_count, 3);
        assert_eq!(extraction.pages[1].text, "");
        for token in [
            "không",
            "không được",
            "ít nhất",
            "khoảng",
            "1/4",
            "1/6 - 1/5",
            "0,7",
            "50%",
            "1.5",
            "3.5 cm",
            "13 hoặc 14",
        ] {
            assert!(extraction.pages[0].text.contains(token), "missing {token}");
        }
    }

    #[test]
    fn rejects_invalid_json_and_page_numbers_but_accepts_duplicate_provider_ids() {
        assert!(matches!(
            normalize_json("{"),
            Err(OpenDataLoaderError::InvalidJson(_))
        ));
        let invalid_page = json!({
            "number of pages": 1,
            "kids": [{"type": "paragraph", "id": 1, "page number": 0, "content": "x"}]
        });
        assert!(matches!(
            normalize_json(&invalid_page.to_string()),
            Err(OpenDataLoaderError::InvalidPageNumber(_))
        ));
        let extraction = normalize_json(DUPLICATE_TABLE_ROW_IDS_FIXTURE).unwrap();
        assert_eq!(extraction.page_count, 1);
        assert_eq!(extraction.pages[0].text, "Dòng thứ nhất\nDòng thứ hai");
    }

    #[test]
    fn rejects_missing_page_metadata() {
        let value = json!({
            "number of pages": 1,
            "kids": [{"type": "paragraph", "id": 1, "content": "x"}]
        });
        assert!(matches!(
            normalize_json(&value.to_string()),
            Err(OpenDataLoaderError::MissingPageMetadata)
        ));
    }

    #[tokio::test]
    async fn invokes_configured_local_json_full_ocr_boundary() {
        let (provider, args) = provider_with_output(ProcessOutput {
            stdout: FIXTURE.as_bytes().to_vec(),
            stderr: String::new(),
            exit_code: Some(0),
            truncated: false,
        });
        let extraction = provider.extract("trusted.pdf").await.unwrap();
        assert_eq!(extraction.extractor_version.as_deref(), Some("2.5.5"));
        let args = args
            .lock()
            .unwrap()
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|pair| pair == ["--format", "json"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--hybrid-mode", "full"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--hybrid-url", "http://127.0.0.1:5502"])
        );
        assert!(!args.iter().any(|arg| arg == "markdown"));
    }

    #[tokio::test]
    async fn rejects_empty_and_failed_provider_output() {
        let (provider, _) = provider_with_output(ProcessOutput {
            stdout: Vec::new(),
            stderr: String::new(),
            exit_code: Some(0),
            truncated: false,
        });
        assert!(matches!(
            provider.extract("trusted.pdf").await,
            Err(OpenDataLoaderError::EmptyOutput)
        ));

        let (provider, _) = provider_with_output(ProcessOutput {
            stdout: Vec::new(),
            stderr: "backend unavailable".to_owned(),
            exit_code: Some(1),
            truncated: false,
        });
        assert!(matches!(
            provider.extract("trusted.pdf").await,
            Err(OpenDataLoaderError::ProcessFailed {
                exit_code: Some(1),
                ..
            })
        ));
    }

    #[test]
    fn rejects_invalid_configuration_before_process_invocation() {
        let mut config = OpenDataLoaderConfig::new("opendataloader-pdf");
        config.hybrid_mode = "unsupported".to_owned();
        assert!(matches!(
            OpenDataLoaderPdfProvider::new(config),
            Err(OpenDataLoaderError::InvalidConfiguration(message)) if message.contains("hybrid mode")
        ));
    }

    #[test]
    fn rejects_external_hybrid_endpoint() {
        let mut config = OpenDataLoaderConfig::new("opendataloader-pdf");
        config.hybrid_url = Some("https://documents.example.test".to_owned());
        assert!(matches!(
            OpenDataLoaderPdfProvider::new(config),
            Err(OpenDataLoaderError::InvalidConfiguration(message)) if message.contains("localhost")
        ));
    }

    #[tokio::test]
    async fn persists_normalized_pages_through_existing_research_path() {
        let database = Database::in_memory().await.unwrap();
        let root =
            std::env::temp_dir().join(format!("9profs-opendataloader-{}", std::process::id()));
        let store = Arc::new(ResearchArtifactStore::new(
            root.clone(),
            database.pool().clone(),
        ));
        let service = ResearchService::new(
            SqliteResearchRepository::new(database.pool().clone()),
            Arc::new(BroadcastEventBus::new(8)),
        )
        .with_artifact_store(Arc::clone(&store));
        let mut upload = store.begin_upload("regulation.pdf").unwrap();
        upload.append(b"%PDF-1.7\nfixture").unwrap();
        let artifact = upload.finish().await.unwrap();
        let case = service
            .create_case(CreateResearchCase {
                title: "adapter test".to_owned(),
            })
            .await
            .unwrap();
        let source = service
            .create_source(CreateResearchSource {
                research_case_id: case.id,
                kind: SourceKind::ReferencePdf,
                label: "regulation".to_owned(),
                identity: None,
            })
            .await
            .unwrap();
        let snapshot = service
            .capture_verified_artifact_snapshot(source.id, &artifact, BTreeMap::new())
            .await
            .unwrap();
        let path = store
            .verified_path(artifact.artifact_id())
            .await
            .unwrap()
            .unwrap();
        let (provider, _) = provider_with_output(ProcessOutput {
            stdout: FIXTURE.as_bytes().to_vec(),
            stderr: String::new(),
            exit_code: Some(0),
            truncated: false,
        });
        let extraction = provider
            .extract_and_capture(path, snapshot.id, &service)
            .await
            .unwrap();
        let pages = service
            .list_all_pdf_pages_for_indexing(extraction.id.as_str())
            .await
            .unwrap();
        assert_eq!(extraction.extractor, EXTRACTOR_NAME);
        assert_eq!(extraction.extractor_version, "2.5.5");
        assert_eq!(extraction.page_count, 3);
        assert_eq!(pages[1].text, "");
        assert!(pages[0].text.contains("không được"));
        let _ = std::fs::remove_dir_all(root);
    }
}
