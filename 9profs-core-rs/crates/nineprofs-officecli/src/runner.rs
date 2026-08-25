use std::{
    ffi::{OsStr, OsString},
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;
use tokio::{sync::Notify, time::sleep};

use crate::{
    artifact::{ArtifactError, DocumentResolver},
    config::{OfficeCliAvailability, OfficeCliConfig, OfficeCliStatus, SUPPORTED_VERSION},
    operations::OfficeCliOperation,
    process::{ProcessBackend, SubprocessBackend},
    rasterizer::{ElectronHtmlRasterizer, HtmlArtifact, HtmlRasterizer, RasterRequest},
};

#[derive(Clone)]
pub struct OfficeCliCancellation {
    inner: Arc<CancellationInner>,
}

struct CancellationInner {
    cancelled: std::sync::atomic::AtomicBool,
    notify: Notify,
}

impl OfficeCliCancellation {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CancellationInner {
                cancelled: std::sync::atomic::AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.inner.notify.notified().await;
    }
}

impl Default for OfficeCliCancellation {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ArtifactReference {
    pub id: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct OfficeCliResponse {
    pub operation: String,
    pub document_id: String,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactReference>,
}

pub struct OfficeCliRunner {
    pub(crate) config: OfficeCliConfig,
    pub(crate) environment: Vec<(String, OsString)>,
    pub(crate) backend: Arc<dyn ProcessBackend>,
    pub(crate) rasterizer: Option<Arc<dyn HtmlRasterizer>>,
    pub(crate) status: OfficeCliStatus,
    pub(crate) next_artifact: AtomicU64,
}

impl OfficeCliRunner {
    pub async fn initialize(config: OfficeCliConfig) -> Self {
        let environment = config.isolated_environment().unwrap_or_default();
        let environment: Vec<_> = environment.into_iter().collect();
        let configured = config.binary_path.is_some();
        let backend = config
            .binary_path
            .clone()
            .map(SubprocessBackend::new)
            .map(|backend| Arc::new(backend) as Arc<dyn ProcessBackend>)
            .unwrap_or_else(|| Arc::new(SubprocessBackend::new(PathBuf::new())));
        let rasterizer: Option<Arc<dyn HtmlRasterizer>> = if config.rasterizer_is_usable() {
            Some(Arc::new(ElectronHtmlRasterizer::new(
                config.electron_path.clone().expect("checked above"),
                config.rasterizer_script.clone().expect("checked above"),
                config.artifact_root.clone(),
                rasterizer_environment(&environment),
                config.raster_limits,
            )))
        } else {
            None
        };
        let mut runner = Self {
            backend,
            rasterizer,
            config,
            environment,
            status: OfficeCliStatus::unavailable(configured, None),
            next_artifact: AtomicU64::new(1),
        };
        if !runner.config.binary_is_usable() {
            return runner;
        }
        match runner
            .run_process(&[OsString::from("--version")], Duration::from_secs(5), None)
            .await
        {
            Ok(output) if output.exit_code == Some(0) => {
                let detected =
                    detect_version(&output.stdout).or_else(|| detect_version(&output.stderr));
                runner.status = if detected.as_deref() == Some(SUPPORTED_VERSION) {
                    OfficeCliStatus::available()
                } else {
                    OfficeCliStatus::unavailable(true, detected)
                };
            }
            Ok(output) => {
                runner.status = OfficeCliStatus::unavailable(true, detect_version(&output.stdout));
            }
            Err(_) => {}
        }
        runner
    }

    pub fn status(&self) -> OfficeCliStatus {
        self.status.clone()
    }

    pub fn is_available(&self) -> bool {
        self.status.availability == OfficeCliAvailability::Available
    }

    pub fn can_render(&self) -> bool {
        self.is_available() && self.rasterizer.is_some()
    }

    pub async fn execute_readonly(
        &self,
        operation: OfficeCliOperation,
        resolver: &dyn DocumentResolver,
        cancellation: Option<OfficeCliCancellation>,
    ) -> Result<OfficeCliResponse, OfficeCliError> {
        if !self.is_available() {
            return Err(match self.status.availability {
                OfficeCliAvailability::VersionMismatch => OfficeCliError::VersionMismatch,
                OfficeCliAvailability::Unavailable | OfficeCliAvailability::Available => {
                    OfficeCliError::Unavailable
                }
            });
        }
        let document = resolver
            .resolve(operation.document())
            .map_err(OfficeCliError::Artifact)?;
        if let OfficeCliOperation::Screenshot(request) = &operation {
            return self
                .execute_render(&operation, &document, request.clone(), cancellation)
                .await;
        }
        let args = operation.args(&document.path, None);
        let output = self
            .run_process(&args, self.config.timeout, cancellation)
            .await?;
        if output.exit_code != Some(0) {
            return Err(OfficeCliError::ProcessFailed);
        }
        let data = serde_json::from_str(&output.stdout).map_err(|_| OfficeCliError::InvalidJson)?;
        Ok(OfficeCliResponse {
            operation: operation.name().to_owned(),
            document_id: document.id,
            data,
            artifact: None,
            artifacts: Vec::new(),
        })
    }

    async fn execute_render(
        &self,
        operation: &OfficeCliOperation,
        document: &crate::artifact::ResolvedDocument,
        request: crate::operations::ScreenshotRequest,
        cancellation: Option<OfficeCliCancellation>,
    ) -> Result<OfficeCliResponse, OfficeCliError> {
        let Some(_) = self.rasterizer.as_ref() else {
            return Err(OfficeCliError::RenderUnavailable);
        };
        let number = self.next_artifact.fetch_add(1, Ordering::Relaxed);
        let prefix = format!("office-render-{}-{number}", std::process::id());
        let html_path =
            controlled_output_path(&self.config.artifact_root, &format!("{prefix}.html"))?;
        let future = self.render_pipeline(operation, document, request, prefix, html_path);
        tokio::pin!(future);
        let timeout = sleep(self.config.timeout);
        tokio::pin!(timeout);
        match cancellation {
            Some(cancellation) => {
                tokio::select! {
                    result = &mut future => result,
                    _ = &mut timeout => Err(OfficeCliError::Timeout),
                    _ = cancellation.cancelled() => Err(OfficeCliError::Cancelled),
                }
            }
            None => {
                tokio::select! {
                    result = &mut future => result,
                    _ = &mut timeout => Err(OfficeCliError::Timeout),
                }
            }
        }
    }

    async fn render_pipeline(
        &self,
        operation: &OfficeCliOperation,
        document: &crate::artifact::ResolvedDocument,
        request: crate::operations::ScreenshotRequest,
        prefix: String,
        html_path: PathBuf,
    ) -> Result<OfficeCliResponse, OfficeCliError> {
        let args = operation.args(&document.path, Some(&html_path));
        let output = self
            .run_backend(&self.backend, &args)
            .await
            .map_err(OfficeCliError::Process)?;
        if output.exit_code != Some(0) {
            return Err(OfficeCliError::ProcessFailed);
        }
        let html_bytes = output.stdout.len();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&html_path)
            .map_err(OfficeCliError::HtmlOutput)?;
        file.write_all(output.stdout.as_bytes())
            .map_err(OfficeCliError::HtmlOutput)?;
        let html = HtmlArtifact::from_path(&html_path, self.config.raster_limits.max_html_bytes)
            .map_err(OfficeCliError::Rasterizer)?;
        let result = self
            .rasterizer
            .as_ref()
            .expect("render availability checked")
            .rasterize(
                &html,
                RasterRequest {
                    prefix,
                    page: request.page,
                    width: request.width,
                    height: request.height,
                },
            )
            .await
            .map_err(OfficeCliError::Rasterizer)?;
        let artifacts: Vec<_> = result
            .artifacts
            .into_iter()
            .map(|artifact| ArtifactReference {
                id: artifact.id,
                kind: "office-render",
            })
            .collect();
        let _ = std::fs::remove_file(&html_path);
        let artifact = artifacts.first().cloned();
        Ok(OfficeCliResponse {
            operation: operation.name().to_owned(),
            document_id: document.id.clone(),
            data: json!({
                "format": "png",
                "html_bytes": html_bytes,
                "artifact_count": artifacts.len(),
                "blocked_network_requests": result.blocked_network_requests,
            }),
            artifact,
            artifacts,
        })
    }

    async fn run_process(
        &self,
        args: &[OsString],
        timeout: Duration,
        cancellation: Option<OfficeCliCancellation>,
    ) -> Result<crate::process::ProcessOutput, OfficeCliError> {
        let future = self.run_backend(&self.backend, args);
        tokio::pin!(future);
        let timeout = sleep(timeout);
        tokio::pin!(timeout);
        match cancellation {
            Some(cancellation) => {
                tokio::select! {
                    result = &mut future => result.map_err(OfficeCliError::Process),
                    _ = &mut timeout => Err(OfficeCliError::Timeout),
                    _ = cancellation.cancelled() => Err(OfficeCliError::Cancelled),
                }
            }
            None => {
                tokio::select! {
                    result = &mut future => result.map_err(OfficeCliError::Process),
                    _ = &mut timeout => Err(OfficeCliError::Timeout),
                }
            }
        }
    }

    async fn run_backend(
        &self,
        backend: &Arc<dyn ProcessBackend>,
        args: &[OsString],
    ) -> Result<crate::process::ProcessOutput, crate::process::ProcessError> {
        backend
            .run(args, &self.environment, self.config.max_output_bytes)
            .await
    }
}

#[derive(Debug, Error)]
pub enum OfficeCliError {
    #[error("OfficeCLI is unavailable")]
    Unavailable,
    #[error("OfficeCLI version does not match the pinned version")]
    VersionMismatch,
    #[error("OfficeCLI document artifact is not approved")]
    Artifact(#[source] ArtifactError),
    #[error("OfficeCLI process failed")]
    Process(#[source] crate::process::ProcessError),
    #[error("OfficeCLI process timed out")]
    Timeout,
    #[error("OfficeCLI process was cancelled")]
    Cancelled,
    #[error("OfficeCLI returned a failed exit code")]
    ProcessFailed,
    #[error("OfficeCLI returned invalid structured output")]
    InvalidJson,
    #[error("OfficeCLI did not produce the requested artifact")]
    ArtifactOutputUnavailable,
    #[error("OfficeCLI HTML output could not be written")]
    HtmlOutput(#[source] std::io::Error),
    #[error("OfficeCLI HTML rasterizer is unavailable")]
    RenderUnavailable,
    #[error("OfficeCLI HTML rasterizer failed")]
    Rasterizer(#[source] crate::rasterizer::RasterizerError),
}

fn controlled_output_path(root: &Path, name: &str) -> Result<PathBuf, OfficeCliError> {
    let root = std::fs::canonicalize(root).map_err(OfficeCliError::HtmlOutput)?;
    let path = root.join(name);
    if path.parent() != Some(root.as_path()) {
        return Err(OfficeCliError::Rasterizer(
            crate::rasterizer::RasterizerError::OutputOutsideRoot,
        ));
    }
    Ok(path)
}

fn rasterizer_environment(isolated: &[(String, OsString)]) -> Vec<(String, OsString)> {
    let safe_keys = [
        "APPDATA",
        "COMSPEC",
        "CONTEXT_MODE_PLATFORM",
        "HOMEDRIVE",
        "HOMEPATH",
        "LANG",
        "LOCALAPPDATA",
        "NODE_EXTRA_CA_CERTS",
        "NO_COLOR",
        "PATH",
        "PATHEXT",
        "PROGRAMDATA",
        "PROGRAMFILES",
        "PROGRAMFILES(X86)",
        "PROGRAMW6432",
        "PYTHONDONTWRITEBYTECODE",
        "PYTHONUNBUFFERED",
        "PYTHONUTF8",
        "SYSTEMDRIVE",
        "SYSTEMROOT",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USERDOMAIN",
        "USERNAME",
        "USERPROFILE",
    ];
    let mut environment = std::env::vars_os()
        .filter(|(key, _)| safe_keys.iter().any(|allowed| key == OsStr::new(allowed)))
        .collect::<std::collections::BTreeMap<_, _>>();
    for (key, value) in isolated {
        if key != "USERPROFILE" {
            environment.insert(OsString::from(key), value.clone());
        }
    }
    environment.insert(
        OsString::from("ELECTRON_NO_ATTACH_CONSOLE"),
        OsString::from("1"),
    );
    environment
        .into_iter()
        .map(|(key, value)| (key.to_string_lossy().into_owned(), value))
        .collect()
}

fn detect_version(output: &str) -> Option<String> {
    output.split_whitespace().find_map(|token| {
        let candidate =
            token.trim_matches(|character: char| !character.is_ascii_digit() && character != '.');
        let mut parts = candidate.split('.');
        (parts.clone().count() == 3
            && parts.all(|part| {
                !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
            }))
        .then(|| candidate.to_owned())
    })
}

#[cfg(test)]
pub(crate) fn test_runner(
    config: OfficeCliConfig,
    backend: Arc<dyn ProcessBackend>,
    status: OfficeCliStatus,
) -> OfficeCliRunner {
    test_runner_with_rasterizer(config, backend, status, None)
}

#[cfg(test)]
fn test_runner_with_rasterizer(
    config: OfficeCliConfig,
    backend: Arc<dyn ProcessBackend>,
    status: OfficeCliStatus,
    rasterizer: Option<Arc<dyn HtmlRasterizer>>,
) -> OfficeCliRunner {
    OfficeCliRunner {
        config,
        environment: Vec::new(),
        backend,
        rasterizer,
        status,
        next_artifact: AtomicU64::new(1),
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use tokio::time::sleep;

    use super::*;
    use crate::{
        artifact::{ArtifactKind, DocumentResolver, ResolvedDocument},
        operations::{DocumentReference, OfficeCliOperation, ViewRequest},
        process::{ProcessError, ProcessOutput},
    };

    #[derive(Clone)]
    struct FakeBackend {
        delay: Duration,
    }

    #[async_trait]
    impl ProcessBackend for FakeBackend {
        async fn run(
            &self,
            _args: &[OsString],
            _environment: &[(String, OsString)],
            _max_output_bytes: usize,
        ) -> Result<ProcessOutput, ProcessError> {
            sleep(self.delay).await;
            Ok(ProcessOutput {
                stdout: "{}".to_owned(),
                stderr: "bounded".to_owned(),
                exit_code: Some(0),
            })
        }
    }

    struct HtmlBackend;

    #[async_trait]
    impl ProcessBackend for HtmlBackend {
        async fn run(
            &self,
            _args: &[OsString],
            _environment: &[(String, OsString)],
            _max_output_bytes: usize,
        ) -> Result<ProcessOutput, ProcessError> {
            Ok(ProcessOutput {
                stdout: "<!doctype html><html><body>render</body></html>".to_owned(),
                stderr: String::new(),
                exit_code: Some(0),
            })
        }
    }

    struct DelayedRasterizer {
        delay: Duration,
    }

    #[async_trait]
    impl HtmlRasterizer for DelayedRasterizer {
        async fn rasterize(
            &self,
            _html: &HtmlArtifact,
            _request: RasterRequest,
        ) -> Result<crate::RenderResult, crate::RasterizerError> {
            sleep(self.delay).await;
            Ok(crate::RenderResult::default())
        }
    }

    struct Resolver;

    impl DocumentResolver for Resolver {
        fn resolve(
            &self,
            reference: &DocumentReference,
        ) -> Result<ResolvedDocument, crate::ArtifactError> {
            Ok(ResolvedDocument {
                id: reference.artifact_id.clone(),
                path: PathBuf::from("approved.docx"),
                kind: ArtifactKind::Detached,
            })
        }
    }

    fn operation() -> OfficeCliOperation {
        OfficeCliOperation::ViewText(ViewRequest {
            document: DocumentReference {
                artifact_id: "detached-doc".to_owned(),
            },
            start: None,
            end: None,
            limit: None,
        })
    }

    fn render_operation() -> OfficeCliOperation {
        OfficeCliOperation::Screenshot(crate::operations::ScreenshotRequest {
            document: DocumentReference {
                artifact_id: "detached-doc".to_owned(),
            },
            page: None,
            width: None,
            height: None,
        })
    }

    #[test]
    fn version_parser_accepts_pinned_version_only() {
        assert_eq!(
            detect_version("OfficeCLI 1.0.144"),
            Some("1.0.144".to_owned())
        );
        assert_eq!(
            detect_version("OfficeCLI 1.0.143"),
            Some("1.0.143".to_owned())
        );
        assert_eq!(detect_version("OfficeCLI development"), None);
    }

    #[tokio::test]
    async fn missing_binary_is_explicitly_unavailable() {
        let mut config = OfficeCliConfig::default();
        config.binary_path = None;
        config.profile_root = std::env::temp_dir().join("9profs-officecli-missing-profile");
        config.artifact_root = std::env::temp_dir().join("9profs-officecli-missing-artifacts");
        let runner = OfficeCliRunner::initialize(config.clone()).await;
        assert_eq!(
            runner.status().availability,
            OfficeCliAvailability::Unavailable
        );
        assert!(!runner.is_available());
        assert!(config.profile_root.join(".officecli/config.json").is_file());
        let _ = std::fs::remove_dir_all(config.profile_root);
        let _ = std::fs::remove_dir_all(config.artifact_root);
    }

    #[tokio::test]
    async fn timeout_and_cancellation_stop_waiting_for_process() {
        let mut config = OfficeCliConfig::default();
        config.timeout = Duration::from_millis(10);
        let runner = test_runner(
            config,
            Arc::new(FakeBackend {
                delay: Duration::from_secs(1),
            }),
            OfficeCliStatus::available(),
        );
        assert!(matches!(
            runner.execute_readonly(operation(), &Resolver, None).await,
            Err(OfficeCliError::Timeout)
        ));

        let cancellation = OfficeCliCancellation::new();
        cancellation.cancel();
        assert!(matches!(
            runner
                .execute_readonly(operation(), &Resolver, Some(cancellation))
                .await,
            Err(OfficeCliError::Cancelled)
        ));
    }

    #[tokio::test]
    async fn render_timeout_and_cancellation_stop_rasterization() {
        let artifact_root = std::env::temp_dir().join(format!(
            "9profs-officecli-render-cancel-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&artifact_root).unwrap();

        let mut config = OfficeCliConfig::default();
        config.artifact_root = artifact_root.clone();
        config.timeout = Duration::from_millis(10);
        let runner = test_runner_with_rasterizer(
            config,
            Arc::new(HtmlBackend),
            OfficeCliStatus::available(),
            Some(Arc::new(DelayedRasterizer {
                delay: Duration::from_secs(1),
            })),
        );
        assert!(matches!(
            runner
                .execute_readonly(render_operation(), &Resolver, None)
                .await,
            Err(OfficeCliError::Timeout)
        ));

        let cancel_artifact_root = artifact_root.with_file_name(format!(
            "9profs-officecli-render-cancel-root-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&cancel_artifact_root).unwrap();
        let mut cancel_config = OfficeCliConfig::default();
        cancel_config.artifact_root = cancel_artifact_root.clone();
        cancel_config.timeout = Duration::from_secs(5);
        let cancel_runner = test_runner_with_rasterizer(
            cancel_config,
            Arc::new(HtmlBackend),
            OfficeCliStatus::available(),
            Some(Arc::new(DelayedRasterizer {
                delay: Duration::from_secs(1),
            })),
        );
        let cancellation = OfficeCliCancellation::new();
        let trigger = cancellation.clone();
        let cancel_task = tokio::spawn(async move {
            sleep(Duration::from_millis(10)).await;
            trigger.cancel();
        });
        let cancellation_result = cancel_runner
            .execute_readonly(render_operation(), &Resolver, Some(cancellation))
            .await;
        assert!(matches!(
            cancellation_result,
            Err(OfficeCliError::Cancelled)
        ));
        cancel_task.await.unwrap();
        let _ = std::fs::remove_dir_all(artifact_root);
        let _ = std::fs::remove_dir_all(cancel_artifact_root);
    }

    #[tokio::test]
    async fn configured_electron_rasterizer_renders_local_html() {
        if std::env::var_os("NINEPROFS_RASTERIZER_QUALIFICATION").as_deref()
            != Some(std::ffi::OsStr::new("1"))
        {
            return;
        }
        let config = OfficeCliConfig::from_env();
        let environment: Vec<_> = config.isolated_environment().unwrap().into_iter().collect();
        let root = std::fs::canonicalize(&config.artifact_root).unwrap();
        let html_path = root.join("rasterizer-local.html");
        std::fs::write(
            &html_path,
            b"<!doctype html><html><body style=\"width:320px;height:180px;background:#234;color:white\">local</body></html>",
        )
        .unwrap();
        let rasterizer = ElectronHtmlRasterizer::new(
            config.electron_path.clone().unwrap(),
            config.rasterizer_script.clone().unwrap(),
            root.clone(),
            rasterizer_environment(&environment),
            config.raster_limits,
        );
        let html =
            HtmlArtifact::from_path(&html_path, config.raster_limits.max_html_bytes).unwrap();
        let result = rasterizer
            .rasterize(
                &html,
                RasterRequest {
                    prefix: "rasterizer-local".to_owned(),
                    ..RasterRequest::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(result.artifacts.len(), 1);
        assert!(result.artifacts[0].bytes > 100);
    }

    #[test]
    fn version_mismatch_status_is_not_available() {
        let status = OfficeCliStatus::unavailable(true, Some("1.0.143".to_owned()));
        assert_eq!(status.availability, OfficeCliAvailability::VersionMismatch);
        assert!(!status.is_available());
    }
}
