use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use async_trait::async_trait;
use serde::Deserialize;
use thiserror::Error;

use crate::process::{ProcessBackend, SubprocessBackend};

pub const DEFAULT_MAX_HTML_BYTES: usize = 16 * 1024 * 1024;
pub const DEFAULT_MAX_RASTER_DIMENSION: u32 = 4096;
pub const DEFAULT_MAX_RENDERED_PAGES: u32 = 64;
pub const DEFAULT_MAX_RENDERED_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
pub struct RasterLimits {
    pub max_html_bytes: usize,
    pub max_dimension: u32,
    pub max_pages: u32,
    pub max_total_bytes: u64,
}

impl Default for RasterLimits {
    fn default() -> Self {
        Self {
            max_html_bytes: DEFAULT_MAX_HTML_BYTES,
            max_dimension: DEFAULT_MAX_RASTER_DIMENSION,
            max_pages: DEFAULT_MAX_RENDERED_PAGES,
            max_total_bytes: DEFAULT_MAX_RENDERED_BYTES,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HtmlArtifact {
    pub path: PathBuf,
    pub bytes: usize,
}

impl HtmlArtifact {
    pub fn from_path(path: impl Into<PathBuf>, max_bytes: usize) -> Result<Self, RasterizerError> {
        let path = path.into();
        let metadata = fs::metadata(&path).map_err(RasterizerError::HtmlRead)?;
        let bytes = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
        if bytes == 0 {
            return Err(RasterizerError::InvalidHtml);
        }
        if bytes > max_bytes {
            return Err(RasterizerError::HtmlTooLarge { bytes, max_bytes });
        }
        let content = fs::read(&path).map_err(RasterizerError::HtmlRead)?;
        if !looks_like_html(&content) {
            return Err(RasterizerError::InvalidHtml);
        }
        Ok(Self { path, bytes })
    }
}

#[derive(Clone, Debug, Default)]
pub struct RasterRequest {
    pub prefix: String,
    pub page: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ImageArtifact {
    pub id: String,
    pub path: PathBuf,
    pub kind: String,
    pub index: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct RenderResult {
    pub artifacts: Vec<ImageArtifact>,
    pub blocked_network_requests: u32,
}

#[async_trait]
pub trait HtmlRasterizer: Send + Sync {
    async fn rasterize(
        &self,
        html: &HtmlArtifact,
        request: RasterRequest,
    ) -> Result<RenderResult, RasterizerError>;
}

pub struct ElectronHtmlRasterizer {
    backend: Arc<dyn ProcessBackend>,
    environment: Vec<(String, OsString)>,
    script: PathBuf,
    output_root: PathBuf,
    limits: RasterLimits,
    next_request: AtomicU64,
}

impl ElectronHtmlRasterizer {
    pub fn new(
        electron_path: PathBuf,
        script: PathBuf,
        output_root: PathBuf,
        environment: Vec<(String, OsString)>,
        limits: RasterLimits,
    ) -> Self {
        Self {
            backend: Arc::new(SubprocessBackend::new(electron_path)),
            environment,
            script,
            output_root,
            limits,
            next_request: AtomicU64::new(1),
        }
    }

    #[cfg(test)]
    fn with_backend(
        backend: Arc<dyn ProcessBackend>,
        output_root: PathBuf,
        limits: RasterLimits,
    ) -> Self {
        Self {
            backend,
            environment: Vec::new(),
            script: PathBuf::from("rasterizer.mjs"),
            output_root,
            limits,
            next_request: AtomicU64::new(1),
        }
    }
}

#[async_trait]
impl HtmlRasterizer for ElectronHtmlRasterizer {
    async fn rasterize(
        &self,
        html: &HtmlArtifact,
        mut request: RasterRequest,
    ) -> Result<RenderResult, RasterizerError> {
        let root = canonical_root(&self.output_root)?;
        let request_number = self.next_request.fetch_add(1, Ordering::Relaxed);
        if request.prefix.trim().is_empty() {
            request.prefix = format!("9profs-raster-{request_number}");
        }
        validate_component(&request.prefix)?;
        let mut args = vec![
            OsString::from(&self.script),
            OsString::from("--html"),
            html.path.as_os_str().to_owned(),
            OsString::from("--output-root"),
            root.as_os_str().to_owned(),
            OsString::from("--prefix"),
            OsString::from(&request.prefix),
            OsString::from("--manifest"),
            OsString::from(format!("{}.manifest.json", request.prefix)),
            OsString::from("--max-dimension"),
            OsString::from(self.limits.max_dimension.to_string()),
            OsString::from("--max-pages"),
            OsString::from(self.limits.max_pages.to_string()),
            OsString::from("--max-total-bytes"),
            OsString::from(self.limits.max_total_bytes.to_string()),
        ];
        if let Some(page) = request.page {
            args.extend([OsString::from("--page"), OsString::from(page.to_string())]);
        }
        if let Some(width) = request.width {
            args.extend([
                OsString::from("--viewport-width"),
                OsString::from(width.to_string()),
            ]);
        }
        if let Some(height) = request.height {
            args.extend([
                OsString::from("--viewport-height"),
                OsString::from(height.to_string()),
            ]);
        }
        let output = self
            .backend
            .run(&args, &self.environment, 1024 * 1024)
            .await
            .map_err(|error| RasterizerError::Process {
                message: error.to_string(),
            })?;
        if output.exit_code != Some(0) {
            return Err(RasterizerError::ProcessFailed {
                stderr: output.stderr,
            });
        }
        let manifest_path = root.join(format!("{}.manifest.json", request.prefix));
        let manifest_bytes = fs::read(&manifest_path).map_err(RasterizerError::OutputRead)?;
        let manifest: RasterManifest = serde_json::from_slice(&manifest_bytes).map_err(|_| {
            RasterizerError::InvalidManifest {
                preview: String::from_utf8_lossy(&manifest_bytes)
                    .chars()
                    .take(240)
                    .collect(),
            }
        })?;
        let _ = fs::remove_file(&manifest_path);
        if manifest.artifacts.is_empty() {
            return Err(RasterizerError::NoArtifacts);
        }
        if manifest.artifacts.len() > usize::try_from(self.limits.max_pages).unwrap_or(usize::MAX) {
            return Err(RasterizerError::PageLimitExceeded);
        }
        let mut total_bytes = 0_u64;
        let mut artifacts = Vec::with_capacity(manifest.artifacts.len());
        for item in manifest.artifacts {
            validate_component(&item.name)?;
            let path = root.join(&item.name);
            let canonical = fs::canonicalize(&path).map_err(RasterizerError::OutputRead)?;
            if !canonical.starts_with(&root) {
                return Err(RasterizerError::OutputOutsideRoot);
            }
            let metadata = fs::metadata(&canonical).map_err(RasterizerError::OutputRead)?;
            let bytes = metadata.len();
            if bytes == 0 || bytes != item.bytes || bytes > self.limits.max_total_bytes {
                return Err(RasterizerError::InvalidOutput);
            }
            total_bytes = total_bytes
                .checked_add(bytes)
                .ok_or(RasterizerError::OutputTooLarge)?;
            if total_bytes > self.limits.max_total_bytes {
                return Err(RasterizerError::OutputTooLarge);
            }
            validate_png(
                &canonical,
                item.width,
                item.height,
                self.limits.max_dimension,
            )?;
            artifacts.push(ImageArtifact {
                id: item.id,
                path: canonical,
                kind: item.kind,
                index: item.index,
                width: item.width,
                height: item.height,
                bytes,
            });
        }
        Ok(RenderResult {
            artifacts,
            blocked_network_requests: manifest.blocked_network_requests,
        })
    }
}

#[derive(Debug, Deserialize)]
struct RasterManifest {
    artifacts: Vec<RasterManifestArtifact>,
    #[serde(default)]
    blocked_network_requests: u32,
}

#[derive(Debug, Deserialize)]
struct RasterManifestArtifact {
    id: String,
    name: String,
    kind: String,
    index: u32,
    width: u32,
    height: u32,
    bytes: u64,
}

#[derive(Debug, Error)]
pub enum RasterizerError {
    #[error("could not read OfficeCLI HTML artifact")]
    HtmlRead(#[source] std::io::Error),
    #[error("OfficeCLI HTML artifact is invalid")]
    InvalidHtml,
    #[error("OfficeCLI HTML artifact exceeds {max_bytes} bytes ({bytes})")]
    HtmlTooLarge { bytes: usize, max_bytes: usize },
    #[error("rasterizer process failed: {message}")]
    Process { message: String },
    #[error("rasterizer returned a failed exit code: {stderr}")]
    ProcessFailed { stderr: String },
    #[error("rasterizer returned invalid structured output: {preview:?}")]
    InvalidManifest { preview: String },
    #[error("rasterizer returned no image artifacts")]
    NoArtifacts,
    #[error("rasterizer page limit exceeded")]
    PageLimitExceeded,
    #[error("rasterizer output could not be inspected")]
    OutputRead(#[source] std::io::Error),
    #[error("rasterizer output escaped the controlled artifact root")]
    OutputOutsideRoot,
    #[error("rasterizer output is invalid")]
    InvalidOutput,
    #[error("rasterizer output exceeds configured byte limit")]
    OutputTooLarge,
    #[error("rasterizer path component is invalid")]
    InvalidPathComponent,
    #[error("rasterizer artifact root is unavailable")]
    RootUnavailable(#[source] std::io::Error),
    #[error("rasterizer PNG is invalid")]
    InvalidPng,
    #[error("rasterizer PNG exceeds configured dimensions")]
    PngTooLarge,
}

fn looks_like_html(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    let lower = text.to_ascii_lowercase();
    lower.contains("<!doctype html>") && lower.contains("<html") && lower.contains("<body")
}

fn canonical_root(root: &Path) -> Result<PathBuf, RasterizerError> {
    fs::canonicalize(root).map_err(RasterizerError::RootUnavailable)
}

fn validate_component(component: &str) -> Result<(), RasterizerError> {
    let path = Path::new(component);
    if component.is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        })
    {
        return Err(RasterizerError::InvalidPathComponent);
    }
    Ok(())
}

fn validate_png(
    path: &Path,
    expected_width: u32,
    expected_height: u32,
    max_dimension: u32,
) -> Result<(), RasterizerError> {
    if expected_width == 0
        || expected_height == 0
        || expected_width > max_dimension
        || expected_height > max_dimension
    {
        return Err(RasterizerError::PngTooLarge);
    }
    let bytes = fs::read(path).map_err(RasterizerError::OutputRead)?;
    if bytes.len() < 24
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || u32::from_be_bytes(bytes[16..20].try_into().unwrap()) != expected_width
        || u32::from_be_bytes(bytes[20..24].try_into().unwrap()) != expected_height
    {
        return Err(RasterizerError::InvalidPng);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use tokio::time::sleep;

    use super::*;
    use crate::process::{ProcessError, ProcessOutput};

    struct Backend {
        delay: Duration,
        output: String,
    }

    #[async_trait]
    impl ProcessBackend for Backend {
        async fn run(
            &self,
            _args: &[OsString],
            _environment: &[(String, OsString)],
            _max_output_bytes: usize,
        ) -> Result<ProcessOutput, ProcessError> {
            sleep(self.delay).await;
            Ok(ProcessOutput {
                stdout: self.output.clone(),
                stderr: String::new(),
                exit_code: Some(0),
            })
        }
    }

    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "9profs-rasterizer-test-{}-{}",
            std::process::id(),
            NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn local_html_validation_rejects_non_html_and_accepts_office_shape() {
        let root = temp_root();
        let invalid = root.join("invalid.html");
        fs::write(&invalid, b"not html").unwrap();
        assert!(matches!(
            HtmlArtifact::from_path(&invalid, 1024),
            Err(RasterizerError::InvalidHtml)
        ));
        let valid = root.join("valid.html");
        fs::write(&valid, b"<!doctype html><html><body>ok</body></html>").unwrap();
        assert!(HtmlArtifact::from_path(&valid, 1024).is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn process_timeout_and_cancellation_are_safe_to_drop() {
        let root = temp_root();
        let html_path = root.join("valid.html");
        fs::write(&html_path, b"<!doctype html><html><body>ok</body></html>").unwrap();
        let html = HtmlArtifact::from_path(&html_path, 1024).unwrap();
        let rasterizer = ElectronHtmlRasterizer::with_backend(
            Arc::new(Backend {
                delay: Duration::from_secs(1),
                output: "{}".to_owned(),
            }),
            root.clone(),
            RasterLimits::default(),
        );
        let future = rasterizer.rasterize(&html, RasterRequest::default());
        assert!(
            tokio::time::timeout(Duration::from_millis(10), future)
                .await
                .is_err()
        );
        let _ = fs::remove_dir_all(root);
    }
}
