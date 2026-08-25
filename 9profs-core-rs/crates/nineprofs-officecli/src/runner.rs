use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use tokio::{sync::Notify, time::sleep};

use crate::{
    artifact::{ArtifactError, DocumentResolver},
    config::{OfficeCliAvailability, OfficeCliConfig, OfficeCliStatus, SUPPORTED_VERSION},
    operations::OfficeCliOperation,
    process::{ProcessBackend, SubprocessBackend},
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
}

pub struct OfficeCliRunner {
    pub(crate) config: OfficeCliConfig,
    pub(crate) environment: Vec<(String, OsString)>,
    pub(crate) backend: Arc<dyn ProcessBackend>,
    pub(crate) status: OfficeCliStatus,
    pub(crate) next_artifact: AtomicU64,
}

impl OfficeCliRunner {
    pub async fn initialize(config: OfficeCliConfig) -> Self {
        let environment = config.isolated_environment().unwrap_or_default();
        let configured = config.binary_path.is_some();
        let backend = config
            .binary_path
            .clone()
            .map(SubprocessBackend::new)
            .map(|backend| Arc::new(backend) as Arc<dyn ProcessBackend>)
            .unwrap_or_else(|| Arc::new(SubprocessBackend::new(PathBuf::new())));
        let mut runner = Self {
            backend,
            config,
            environment: environment.into_iter().collect(),
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
        let (screenshot_output, artifact) =
            if matches!(operation, OfficeCliOperation::Screenshot(_)) {
                let number = self.next_artifact.fetch_add(1, Ordering::Relaxed);
                let id = format!("office-screenshot-{number}");
                let path = self.config.artifact_root.join(format!("{id}.png"));
                (
                    Some(path),
                    Some(ArtifactReference {
                        id,
                        kind: "office-screenshot",
                    }),
                )
            } else {
                (None, None)
            };
        let args = operation.args(&document.path, screenshot_output.as_deref());
        let output = self
            .run_process(&args, self.config.timeout, cancellation)
            .await?;
        if output.exit_code != Some(0) {
            return Err(OfficeCliError::ProcessFailed);
        }
        if let Some(path) = screenshot_output.as_deref()
            && !path.is_file()
        {
            return Err(OfficeCliError::ArtifactOutputUnavailable);
        }
        let data = serde_json::from_str(&output.stdout).map_err(|_| OfficeCliError::InvalidJson)?;
        Ok(OfficeCliResponse {
            operation: operation.name().to_owned(),
            document_id: document.id,
            data,
            artifact,
        })
    }

    async fn run_process(
        &self,
        args: &[OsString],
        timeout: Duration,
        cancellation: Option<OfficeCliCancellation>,
    ) -> Result<crate::process::ProcessOutput, OfficeCliError> {
        let future = self
            .backend
            .run(args, &self.environment, self.config.max_output_bytes);
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
    OfficeCliRunner {
        config,
        environment: Vec::new(),
        backend,
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

    #[test]
    fn version_mismatch_status_is_not_available() {
        let status = OfficeCliStatus::unavailable(true, Some("1.0.143".to_owned()));
        assert_eq!(status.availability, OfficeCliAvailability::VersionMismatch);
        assert!(!status.is_available());
    }
}
