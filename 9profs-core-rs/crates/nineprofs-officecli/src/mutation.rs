use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ArtifactError, ArtifactReference, DocumentReference, DocumentResolver, OfficeCliCancellation,
    OfficeCliError, OfficeCliRunner, OfficeDocumentType, OfficeMutation, ScreenshotRequest,
};

const MAX_OPERATIONS: usize = 64;
const MAX_SERIALIZED_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_LOGICAL_NAME_BYTES: usize = 256;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateDocumentRequest {
    pub document_type: OfficeDocumentType,
    pub logical_name: Option<String>,
    #[serde(default)]
    pub operations: Vec<OfficeMutation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DetachedMutationRequest {
    pub document: DocumentReference,
    pub operations: Vec<OfficeMutation>,
    #[serde(default)]
    pub base_revision_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ValidationDiagnostic {
    pub severity: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ValidationSummary {
    pub structural_valid: bool,
    pub diagnostics: Vec<ValidationDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RenderSummary {
    pub artifacts: Vec<ArtifactReference>,
    pub blocked_network_requests: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactRevision {
    pub artifact_id: String,
    pub revision_id: String,
    pub parent_revision_id: Option<String>,
    pub document_type: OfficeDocumentType,
    pub content_hash: String,
    pub created_at_ms: u128,
    pub reference: DocumentReference,
    pub logical_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MutationResult {
    pub revision: ArtifactRevision,
    pub operations_requested: usize,
    pub operations_applied: usize,
    pub validation: ValidationSummary,
    pub render: RenderSummary,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
pub struct DetachedMutationService {
    runner: Arc<OfficeCliRunner>,
    resolver: Arc<dyn DocumentResolver>,
    next_revision: Arc<AtomicU64>,
}

impl DetachedMutationService {
    pub fn new(runner: Arc<OfficeCliRunner>, resolver: Arc<dyn DocumentResolver>) -> Self {
        Self {
            runner,
            resolver,
            next_revision: Arc::new(AtomicU64::new(1)),
        }
    }

    pub async fn create(
        &self,
        request: CreateDocumentRequest,
        cancellation: Option<OfficeCliCancellation>,
    ) -> Result<MutationResult, DetachedMutationError> {
        validate_operations(&request.operations)?;
        validate_logical_name(request.logical_name.as_deref())?;
        let root = self.controlled_root()?;
        fs::create_dir_all(&root).map_err(DetachedMutationError::WorkingCopy)?;
        let working = self.unique_path(&root, request.document_type);
        let result = self
            .run_transaction(
                working.clone(),
                request.document_type,
                None,
                request.logical_name,
                request.operations,
                true,
                cancellation,
            )
            .await;
        if result.is_err() {
            remove_working(&working);
        }
        result
    }

    pub async fn mutate_detached(
        &self,
        request: DetachedMutationRequest,
        cancellation: Option<OfficeCliCancellation>,
    ) -> Result<MutationResult, DetachedMutationError> {
        validate_operations(&request.operations)?;
        let writable = self
            .resolver
            .resolve_writable(&request.document)
            .map_err(DetachedMutationError::Artifact)?;
        let current = self
            .resolver
            .resolve_writable(&request.document)
            .map_err(DetachedMutationError::Artifact)?;
        if current.document.path != writable.document.path {
            return Err(DetachedMutationError::Artifact(
                ArtifactError::OutsideApprovedRoots,
            ));
        }

        let document_type = document_type(&current.document.path)?;
        let working = self.unique_path(
            current.document.path.parent().ok_or_else(|| {
                DetachedMutationError::WorkingCopy(std::io::Error::other(
                    "detached artifact has no parent directory",
                ))
            })?,
            document_type,
        );
        fs::copy(&current.document.path, &working).map_err(DetachedMutationError::WorkingCopy)?;
        let result = self
            .run_transaction(
                working.clone(),
                document_type,
                request.base_revision_id,
                None,
                request.operations,
                false,
                cancellation,
            )
            .await;
        if result.is_err() {
            remove_working(&working);
        }
        result
    }

    fn controlled_root(&self) -> Result<PathBuf, DetachedMutationError> {
        self.resolver
            .writable_root()
            .or_else(|| Some(self.runner.config.artifact_root.clone()))
            .ok_or(DetachedMutationError::NoControlledRoot)
    }

    fn unique_path(&self, parent: &Path, document_type: OfficeDocumentType) -> PathBuf {
        let id = self.next_revision.fetch_add(1, Ordering::Relaxed);
        parent.join(format!(
            "9profs-officecli-working-{}-{id}.{}",
            std::process::id(),
            document_type.extension()
        ))
    }

    async fn run_transaction(
        &self,
        working: PathBuf,
        document_type: OfficeDocumentType,
        parent_revision_id: Option<String>,
        logical_name: Option<String>,
        operations: Vec<OfficeMutation>,
        create: bool,
        cancellation: Option<OfficeCliCancellation>,
    ) -> Result<MutationResult, DetachedMutationError> {
        if is_cancelled(cancellation.as_ref()) {
            return Err(DetachedMutationError::Cancelled);
        }
        if create {
            self.runner
                .create_document(&working, cancellation.clone())
                .await
                .map_err(DetachedMutationError::OfficeCli)?;
        }

        let mut applied = 0;
        for operation in &operations {
            if is_cancelled(cancellation.as_ref()) {
                return Err(DetachedMutationError::Cancelled);
            }
            self.runner
                .execute_mutation(&working, operation, cancellation.clone())
                .await
                .map_err(DetachedMutationError::OfficeCli)?;
            applied += 1;
        }
        self.runner
            .save_path(&working, cancellation.clone())
            .await
            .map_err(DetachedMutationError::OfficeCli)?;

        let validation_data = self
            .runner
            .validate_path(&working, cancellation.clone())
            .await
            .map_err(DetachedMutationError::OfficeCli)?;
        let validation = summarize_validation(&validation_data);
        if !validation.structural_valid {
            return Err(DetachedMutationError::Validation { validation });
        }

        if is_cancelled(cancellation.as_ref()) {
            return Err(DetachedMutationError::Cancelled);
        }
        let rendered = self
            .runner
            .render_path(
                &working,
                ScreenshotRequest {
                    document: DocumentReference {
                        artifact_id: "working-revision".to_owned(),
                    },
                    page: None,
                    width: None,
                    height: None,
                },
                cancellation.clone(),
            )
            .await
            .map_err(DetachedMutationError::OfficeCli)?;
        if rendered.artifacts.is_empty() {
            self.runner.cleanup_render_artifacts(&rendered);
            return Err(DetachedMutationError::OfficeCli(
                OfficeCliError::ArtifactOutputUnavailable,
            ));
        }
        if is_cancelled(cancellation.as_ref()) {
            self.runner.cleanup_render_artifacts(&rendered);
            return Err(DetachedMutationError::Cancelled);
        }
        let render = RenderSummary {
            artifacts: rendered.artifacts.clone(),
            blocked_network_requests: rendered
                .data
                .get("blocked_network_requests")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default() as u32,
        };

        let revision = match self.promote(&working, document_type, parent_revision_id, logical_name)
        {
            Ok(revision) => revision,
            Err(error) => {
                self.runner.cleanup_render_artifacts(&rendered);
                return Err(error);
            }
        };
        Ok(MutationResult {
            revision,
            operations_requested: operations.len(),
            operations_applied: applied,
            validation,
            render,
            warnings: Vec::new(),
        })
    }

    fn promote(
        &self,
        working: &Path,
        document_type: OfficeDocumentType,
        parent_revision_id: Option<String>,
        logical_name: Option<String>,
    ) -> Result<ArtifactRevision, DetachedMutationError> {
        let parent = working.parent().ok_or_else(|| {
            DetachedMutationError::WorkingCopy(std::io::Error::other(
                "working copy has no parent directory",
            ))
        })?;
        let id = self.next_revision.fetch_add(1, Ordering::Relaxed);
        let artifact_id = format!("artifact-revision-{}-{id}", std::process::id());
        let revision_id = format!("revision-{}-{id}", std::process::id());
        let content_hash = content_hash(working).map_err(DetachedMutationError::Promotion)?;
        let mut destination = parent.join(format!("{artifact_id}.{}", document_type.extension()));
        let mut collision = 0u32;
        while destination.exists() {
            collision = collision.saturating_add(1);
            destination = parent.join(format!(
                "{artifact_id}-{collision}.{}",
                document_type.extension()
            ));
        }
        fs::rename(working, &destination).map_err(DetachedMutationError::Promotion)?;
        if let Err(error) = self
            .resolver
            .register_detached_revision(artifact_id.clone(), destination.clone())
        {
            remove_working(&destination);
            return Err(DetachedMutationError::Artifact(error));
        }
        Ok(ArtifactRevision {
            artifact_id: artifact_id.clone(),
            revision_id,
            parent_revision_id,
            document_type,
            content_hash,
            created_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            reference: DocumentReference { artifact_id },
            logical_name,
        })
    }
}

#[derive(Debug, Error)]
pub enum DetachedMutationError {
    #[error("OfficeCLI artifact is not writable")]
    Artifact(#[source] ArtifactError),
    #[error("OfficeCLI mutation operation is invalid: {0}")]
    InvalidOperation(String),
    #[error("OfficeCLI mutation argument limit exceeded")]
    ArgumentLimitExceeded,
    #[error("OfficeCLI mutation cancelled")]
    Cancelled,
    #[error("no controlled artifact root is available")]
    NoControlledRoot,
    #[error("OfficeCLI mutation failed")]
    OfficeCli(#[source] OfficeCliError),
    #[error("OfficeCLI structural validation failed")]
    Validation { validation: ValidationSummary },
    #[error("working copy could not be prepared")]
    WorkingCopy(#[source] std::io::Error),
    #[error("working artifact could not be promoted")]
    Promotion(#[source] std::io::Error),
}

fn validate_operations(operations: &[OfficeMutation]) -> Result<(), DetachedMutationError> {
    if operations.len() > MAX_OPERATIONS {
        return Err(DetachedMutationError::ArgumentLimitExceeded);
    }
    let bytes = serde_json::to_vec(operations)
        .map_err(|error| DetachedMutationError::InvalidOperation(error.to_string()))?;
    if bytes.len() > MAX_SERIALIZED_ARGUMENT_BYTES {
        return Err(DetachedMutationError::ArgumentLimitExceeded);
    }
    for operation in operations {
        operation
            .validate()
            .map_err(|error| DetachedMutationError::InvalidOperation(error.to_string()))?;
    }
    Ok(())
}

fn validate_logical_name(name: Option<&str>) -> Result<(), DetachedMutationError> {
    if let Some(name) = name
        && (name.is_empty()
            || name.len() > MAX_LOGICAL_NAME_BYTES
            || name.contains('\0')
            || name.contains('/')
            || name.contains('\\'))
    {
        return Err(DetachedMutationError::InvalidOperation(
            "logical name is invalid".to_owned(),
        ));
    }
    Ok(())
}

fn document_type(path: &Path) -> Result<OfficeDocumentType, DetachedMutationError> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("docx") => Ok(OfficeDocumentType::Docx),
        Some("xlsx") => Ok(OfficeDocumentType::Xlsx),
        Some("pptx") => Ok(OfficeDocumentType::Pptx),
        _ => Err(DetachedMutationError::Artifact(
            ArtifactError::UnsupportedExtension,
        )),
    }
}

fn content_hash(path: &Path) -> Result<String, std::io::Error> {
    let bytes = fs::read(path)?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn remove_working(path: &Path) {
    let _ = fs::remove_file(path);
}

fn is_cancelled(cancellation: Option<&OfficeCliCancellation>) -> bool {
    cancellation.is_some_and(OfficeCliCancellation::is_cancelled)
}

fn summarize_validation(value: &serde_json::Value) -> ValidationSummary {
    let mut diagnostics = Vec::new();
    let mut structural_valid = true;
    if let serde_json::Value::Object(object) = value {
        if let Some(valid) = object.get("valid").and_then(serde_json::Value::as_bool) {
            structural_valid = valid;
        }
        for key in ["errors", "issues", "diagnostics"] {
            if let Some(items) = object.get(key).and_then(serde_json::Value::as_array) {
                for item in items {
                    let message = item
                        .get("description")
                        .or_else(|| item.get("message"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| item.to_string());
                    diagnostics.push(ValidationDiagnostic {
                        severity: if key == "errors" {
                            "error".to_owned()
                        } else {
                            "issue".to_owned()
                        },
                        message,
                    });
                }
                if key != "diagnostics" && !items.is_empty() {
                    structural_valid = false;
                }
            }
        }
    } else if let Some(text) = value.as_str()
        && text.to_ascii_lowercase().contains("error")
    {
        structural_valid = false;
        diagnostics.push(ValidationDiagnostic {
            severity: "error".to_owned(),
            message: text.to_owned(),
        });
    }
    ValidationSummary {
        structural_valid,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs, path::PathBuf, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use serde_json::json;
    use tokio::time::sleep;

    use super::*;
    use crate::{
        ArtifactResolver, OfficeCliAvailability, OfficeCliConfig, OfficeCliStatus,
        process::{ProcessBackend, ProcessError, ProcessOutput},
        rasterizer::{HtmlArtifact, HtmlRasterizer, ImageArtifact, RasterRequest, RenderResult},
        runner::test_runner_with_rasterizer,
    };

    struct Backend {
        delay: Duration,
        fail_mutation: bool,
        fail_validation: bool,
    }

    #[async_trait]
    impl ProcessBackend for Backend {
        async fn run(
            &self,
            args: &[OsString],
            _environment: &[(String, OsString)],
            _max_output_bytes: usize,
        ) -> Result<ProcessOutput, ProcessError> {
            sleep(self.delay).await;
            let args = args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            if self.fail_mutation && args.iter().any(|arg| arg == "set") {
                return Ok(ProcessOutput {
                    stdout: "{}".to_owned(),
                    stderr: "failed".to_owned(),
                    exit_code: Some(1),
                });
            }
            if self.fail_validation && args.iter().any(|arg| arg == "validate") {
                return Ok(ProcessOutput {
                    stdout: json!({
                        "valid": false,
                        "errors": [{"message": "invalid package"}]
                    })
                    .to_string(),
                    stderr: String::new(),
                    exit_code: Some(0),
                });
            }
            if args.iter().any(|arg| arg == "set")
                && let Some(path) = args
                    .get(2)
                    .and_then(|path| (!path.is_empty()).then_some(path))
            {
                fs::write(path, b"mutated-by-test").unwrap();
            }
            let stdout = if args.iter().any(|arg| arg == "validate") {
                json!({"valid": true, "errors": []}).to_string()
            } else if args.iter().any(|arg| arg == "view") {
                "<!doctype html><html><body>render</body></html>".to_owned()
            } else {
                "{}".to_owned()
            };
            Ok(ProcessOutput {
                stdout,
                stderr: String::new(),
                exit_code: Some(0),
            })
        }
    }

    struct Rasterizer {
        fail: bool,
    }

    #[async_trait]
    impl HtmlRasterizer for Rasterizer {
        async fn rasterize(
            &self,
            _html: &HtmlArtifact,
            request: RasterRequest,
        ) -> Result<RenderResult, crate::RasterizerError> {
            if self.fail {
                return Err(crate::RasterizerError::InvalidHtml);
            }
            Ok(RenderResult {
                artifacts: vec![ImageArtifact {
                    id: format!("{}-1", request.prefix),
                    path: PathBuf::new(),
                    kind: "page".to_owned(),
                    index: 1,
                    width: 1,
                    height: 1,
                    bytes: 1,
                }],
                blocked_network_requests: 0,
            })
        }
    }

    fn root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("9profs-officecli-mutation-{name}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn runner(root: &Path, backend: Backend) -> Arc<OfficeCliRunner> {
        runner_with_rasterizer(root, backend, false)
    }

    fn runner_with_rasterizer(
        root: &Path,
        backend: Backend,
        fail_render: bool,
    ) -> Arc<OfficeCliRunner> {
        let config = OfficeCliConfig {
            artifact_root: root.join("renders"),
            timeout: Duration::from_millis(100),
            ..OfficeCliConfig::default()
        };
        fs::create_dir_all(&config.artifact_root).unwrap();
        Arc::new(test_runner_with_rasterizer(
            config,
            Arc::new(backend),
            OfficeCliStatus::available(),
            Some(Arc::new(Rasterizer { fail: fail_render })),
        ))
    }

    fn set_operation() -> OfficeMutation {
        OfficeMutation::Set {
            selector: "/body/p[1]".to_owned(),
            properties: [("text".to_owned(), "changed".to_owned())]
                .into_iter()
                .collect(),
        }
    }

    #[tokio::test]
    async fn detached_mutation_preserves_base_and_publishes_new_revision() {
        let root = root("success");
        let base = root.join("base.docx");
        fs::write(&base, b"base-bytes").unwrap();
        let resolver = Arc::new(ArtifactResolver::new([root.clone()]));
        resolver.register_detached("base", &base).unwrap();
        let service = DetachedMutationService::new(
            runner(
                &root,
                Backend {
                    delay: Duration::ZERO,
                    fail_mutation: false,
                    fail_validation: false,
                },
            ),
            resolver.clone(),
        );
        let result = service
            .mutate_detached(
                DetachedMutationRequest {
                    document: DocumentReference {
                        artifact_id: "base".to_owned(),
                    },
                    operations: vec![set_operation()],
                    base_revision_id: None,
                },
                None,
            )
            .await
            .unwrap();
        assert_eq!(fs::read(&base).unwrap(), b"base-bytes");
        assert_eq!(result.operations_applied, 1);
        assert!(resolver.resolve(&result.revision.reference).is_ok());
        assert_ne!(
            fs::read(resolver.resolve(&result.revision.reference).unwrap().path).unwrap(),
            b"base-bytes"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn failed_mutation_rolls_back_working_revision_and_base() {
        let root = root("failure");
        let base = root.join("base.docx");
        fs::write(&base, b"base-bytes").unwrap();
        let resolver = Arc::new(ArtifactResolver::new([root.clone()]));
        resolver.register_detached("base", &base).unwrap();
        let service = DetachedMutationService::new(
            runner(
                &root,
                Backend {
                    delay: Duration::ZERO,
                    fail_mutation: true,
                    fail_validation: false,
                },
            ),
            resolver,
        );
        assert!(
            service
                .mutate_detached(
                    DetachedMutationRequest {
                        document: DocumentReference {
                            artifact_id: "base".to_owned(),
                        },
                        operations: vec![set_operation()],
                        base_revision_id: None,
                    },
                    None,
                )
                .await
                .is_err()
        );
        assert_eq!(fs::read(&base).unwrap(), b"base-bytes");
        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().is_file())
                .count(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cancellation_publish_nothing_and_cleans_working_copy() {
        let root = root("cancel");
        let base = root.join("base.docx");
        fs::write(&base, b"base-bytes").unwrap();
        let resolver = Arc::new(ArtifactResolver::new([root.clone()]));
        resolver.register_detached("base", &base).unwrap();
        let service = DetachedMutationService::new(
            runner(
                &root,
                Backend {
                    delay: Duration::from_millis(50),
                    fail_mutation: false,
                    fail_validation: false,
                },
            ),
            resolver,
        );
        let cancellation = OfficeCliCancellation::new();
        cancellation.cancel();
        assert!(matches!(
            service
                .mutate_detached(
                    DetachedMutationRequest {
                        document: DocumentReference {
                            artifact_id: "base".to_owned(),
                        },
                        operations: vec![set_operation()],
                        base_revision_id: None,
                    },
                    Some(cancellation),
                )
                .await,
            Err(DetachedMutationError::Cancelled)
        ));
        assert_eq!(fs::read(&base).unwrap(), b"base-bytes");

        let in_flight_cancellation = OfficeCliCancellation::new();
        let cancellation_signal = in_flight_cancellation.clone();
        let canceller = tokio::spawn(async move {
            sleep(Duration::from_millis(10)).await;
            cancellation_signal.cancel();
        });
        assert!(matches!(
            service
                .mutate_detached(
                    DetachedMutationRequest {
                        document: DocumentReference {
                            artifact_id: "base".to_owned(),
                        },
                        operations: vec![set_operation()],
                        base_revision_id: None,
                    },
                    Some(in_flight_cancellation),
                )
                .await,
            Err(DetachedMutationError::OfficeCli(OfficeCliError::Cancelled))
        ));
        canceller.await.unwrap();
        assert_eq!(fs::read(&base).unwrap(), b"base-bytes");
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn timeout_publish_nothing_and_cleans_working_copy() {
        let root = root("timeout");
        let base = root.join("base.docx");
        fs::write(&base, b"base-bytes").unwrap();
        let resolver = Arc::new(ArtifactResolver::new([root.clone()]));
        resolver.register_detached("base", &base).unwrap();
        let service = DetachedMutationService::new(
            runner(
                &root,
                Backend {
                    delay: Duration::from_millis(250),
                    fail_mutation: false,
                    fail_validation: false,
                },
            ),
            resolver,
        );
        let result = service
            .mutate_detached(
                DetachedMutationRequest {
                    document: DocumentReference {
                        artifact_id: "base".to_owned(),
                    },
                    operations: vec![set_operation()],
                    base_revision_id: None,
                },
                None,
            )
            .await;
        assert!(matches!(
            result,
            Err(DetachedMutationError::OfficeCli(OfficeCliError::Timeout))
        ));
        assert_eq!(fs::read(&base).unwrap(), b"base-bytes");
        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().is_file())
                .count(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn render_failure_publish_nothing() {
        let root = root("render-failure");
        let base = root.join("base.docx");
        fs::write(&base, b"base-bytes").unwrap();
        let resolver = Arc::new(ArtifactResolver::new([root.clone()]));
        resolver.register_detached("base", &base).unwrap();
        let service = DetachedMutationService::new(
            runner_with_rasterizer(
                &root,
                Backend {
                    delay: Duration::ZERO,
                    fail_mutation: false,
                    fail_validation: false,
                },
                true,
            ),
            resolver,
        );
        assert!(matches!(
            service
                .mutate_detached(
                    DetachedMutationRequest {
                        document: DocumentReference {
                            artifact_id: "base".to_owned(),
                        },
                        operations: vec![set_operation()],
                        base_revision_id: None,
                    },
                    None,
                )
                .await,
            Err(DetachedMutationError::OfficeCli(
                OfficeCliError::Rasterizer(_)
            ))
        ));
        assert_eq!(fs::read(&base).unwrap(), b"base-bytes");
        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().is_file())
                .count(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn same_base_can_publish_independent_copy_on_write_revisions() {
        let root = root("concurrent");
        let base = root.join("base.docx");
        fs::write(&base, b"base-bytes").unwrap();
        let resolver = Arc::new(ArtifactResolver::new([root.clone()]));
        resolver.register_detached("base", &base).unwrap();
        let service = DetachedMutationService::new(
            runner(
                &root,
                Backend {
                    delay: Duration::from_millis(5),
                    fail_mutation: false,
                    fail_validation: false,
                },
            ),
            resolver.clone(),
        );
        let (left, right) = tokio::join!(
            service.mutate_detached(
                DetachedMutationRequest {
                    document: DocumentReference {
                        artifact_id: "base".to_owned(),
                    },
                    operations: vec![set_operation()],
                    base_revision_id: Some("base-revision".to_owned()),
                },
                None,
            ),
            service.mutate_detached(
                DetachedMutationRequest {
                    document: DocumentReference {
                        artifact_id: "base".to_owned(),
                    },
                    operations: vec![set_operation()],
                    base_revision_id: Some("base-revision".to_owned()),
                },
                None,
            )
        );
        let left = left.unwrap();
        let right = right.unwrap();
        assert_ne!(left.revision.revision_id, right.revision.revision_id);
        assert_eq!(
            left.revision.parent_revision_id.as_deref(),
            Some("base-revision")
        );
        assert_eq!(fs::read(&base).unwrap(), b"base-bytes");
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn validation_failure_publish_nothing() {
        let root = root("validation-failure");
        let base = root.join("base.docx");
        fs::write(&base, b"base-bytes").unwrap();
        let resolver = Arc::new(ArtifactResolver::new([root.clone()]));
        resolver.register_detached("base", &base).unwrap();
        let service = DetachedMutationService::new(
            runner(
                &root,
                Backend {
                    delay: Duration::ZERO,
                    fail_mutation: false,
                    fail_validation: true,
                },
            ),
            resolver,
        );
        assert!(matches!(
            service
                .mutate_detached(
                    DetachedMutationRequest {
                        document: DocumentReference {
                            artifact_id: "base".to_owned(),
                        },
                        operations: vec![set_operation()],
                        base_revision_id: None,
                    },
                    None,
                )
                .await,
            Err(DetachedMutationError::Validation { .. })
        ));
        assert_eq!(fs::read(&base).unwrap(), b"base-bytes");
        assert_eq!(
            fs::read_dir(&root)
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().is_file())
                .count(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validation_rejects_host_paths_and_raw_xml_shapes() {
        let operation = OfficeMutation::Set {
            selector: r"C:\secret.docx".to_owned(),
            properties: Default::default(),
        };
        assert!(operation.validate().is_err());
    }

    #[test]
    fn validation_summary_fails_nonempty_error_list() {
        let summary = summarize_validation(&serde_json::json!({
            "valid": false,
            "errors": [{"message": "bad package"}]
        }));
        assert!(!summary.structural_valid);
        assert_eq!(summary.diagnostics.len(), 1);
    }
}
