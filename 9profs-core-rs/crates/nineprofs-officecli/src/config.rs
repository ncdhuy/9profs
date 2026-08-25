use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::rasterizer::RasterLimits;

pub const SUPPORTED_VERSION: &str = "1.0.144";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_OUTPUT_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct OfficeCliConfig {
    pub binary_path: Option<PathBuf>,
    pub profile_root: PathBuf,
    pub artifact_root: PathBuf,
    pub electron_path: Option<PathBuf>,
    pub rasterizer_script: Option<PathBuf>,
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub raster_limits: RasterLimits,
}

impl Default for OfficeCliConfig {
    fn default() -> Self {
        let root = std::env::temp_dir().join("9profs-officecli");
        Self {
            binary_path: None,
            profile_root: root.join("profile"),
            artifact_root: root.join("artifacts"),
            electron_path: None,
            rasterizer_script: None,
            timeout: DEFAULT_TIMEOUT,
            max_output_bytes: DEFAULT_OUTPUT_LIMIT,
            raster_limits: RasterLimits::default(),
        }
    }
}

impl OfficeCliConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        config.binary_path = std::env::var_os("NINEPROFS_OFFICECLI_PATH").map(PathBuf::from);
        if let Some(root) = std::env::var_os("NINEPROFS_OFFICECLI_PROFILE") {
            config.profile_root = PathBuf::from(root);
        }
        if let Some(root) = std::env::var_os("NINEPROFS_OFFICECLI_ARTIFACT_ROOT") {
            config.artifact_root = PathBuf::from(root);
        }
        config.electron_path = std::env::var_os("NINEPROFS_ELECTRON_PATH").map(PathBuf::from);
        config.rasterizer_script =
            std::env::var_os("NINEPROFS_HTML_RASTERIZER_SCRIPT").map(PathBuf::from);
        if let Ok(value) = std::env::var("NINEPROFS_OFFICECLI_TIMEOUT_MS")
            && let Ok(milliseconds) = value.parse::<u64>()
        {
            config.timeout = Duration::from_millis(milliseconds.max(1));
        }
        if let Ok(value) = std::env::var("NINEPROFS_OFFICECLI_MAX_OUTPUT_BYTES")
            && let Ok(bytes) = value.parse::<usize>()
        {
            config.max_output_bytes = bytes.max(1);
        }
        if let Ok(value) = std::env::var("NINEPROFS_OFFICECLI_MAX_HTML_BYTES")
            && let Ok(bytes) = value.parse::<usize>()
        {
            config.raster_limits.max_html_bytes = bytes.max(1);
        }
        if let Ok(value) = std::env::var("NINEPROFS_OFFICECLI_MAX_RASTER_DIMENSION")
            && let Ok(dimension) = value.parse::<u32>()
        {
            config.raster_limits.max_dimension = dimension.max(1);
        }
        if let Ok(value) = std::env::var("NINEPROFS_OFFICECLI_MAX_RENDERED_PAGES")
            && let Ok(pages) = value.parse::<u32>()
        {
            config.raster_limits.max_pages = pages.max(1);
        }
        if let Ok(value) = std::env::var("NINEPROFS_OFFICECLI_MAX_RENDERED_BYTES")
            && let Ok(bytes) = value.parse::<u64>()
        {
            config.raster_limits.max_total_bytes = bytes.max(1);
        }
        config
    }

    pub(crate) fn isolated_environment(&self) -> Result<BTreeMap<String, OsString>, ConfigError> {
        fs::create_dir_all(&self.profile_root).map_err(|source| ConfigError::Profile { source })?;
        fs::create_dir_all(&self.artifact_root)
            .map_err(|source| ConfigError::Artifact { source })?;

        let officecli_dir = self.profile_root.join(".officecli");
        fs::create_dir_all(&officecli_dir).map_err(|source| ConfigError::Profile { source })?;
        // OfficeCLI v1.0.144 reads AutoUpdate from this per-user file and can
        // refresh installed agent skills on a version transition. Both paths
        // stay inside this 9Profs-owned profile; real user agent directories
        // are never presented to the sidecar.
        let config = serde_json::json!({
            "AutoUpdate": false,
            "LastSkillRefreshVersion": SUPPORTED_VERSION,
        });
        fs::write(
            officecli_dir.join("config.json"),
            serde_json::to_vec(&config).expect("static OfficeCLI profile config serializes"),
        )
        .map_err(|source| ConfigError::Profile { source })?;

        let mut environment = BTreeMap::new();
        environment.insert("OFFICECLI_NO_AUTO_INSTALL".to_owned(), OsString::from("1"));
        environment.insert("OFFICECLI_NO_AUTO_RESIDENT".to_owned(), OsString::from("1"));
        environment.insert("OFFICECLI_SKIP_UPDATE".to_owned(), OsString::from("1"));
        for key in ["PATH", "SystemRoot", "SYSTEMROOT", "COMSPEC"] {
            if let Some(value) = std::env::var_os(key) {
                environment.insert(key.to_owned(), value);
            }
        }
        let profile = self.profile_root.as_os_str().to_owned();
        for key in ["HOME", "USERPROFILE"] {
            environment.insert(key.to_owned(), profile.clone());
        }
        environment.insert(
            "APPDATA".to_owned(),
            self.profile_root.join("appdata").into_os_string(),
        );
        environment.insert(
            "LOCALAPPDATA".to_owned(),
            self.profile_root.join("localappdata").into_os_string(),
        );
        environment.insert(
            "XDG_CONFIG_HOME".to_owned(),
            self.profile_root.join("config").into_os_string(),
        );
        environment.insert(
            "XDG_CACHE_HOME".to_owned(),
            self.profile_root.join("cache").into_os_string(),
        );
        Ok(environment)
    }

    pub(crate) fn binary_is_usable(&self) -> bool {
        self.binary_path
            .as_deref()
            .is_some_and(|path| path.is_file() && executable(path))
    }

    pub(crate) fn rasterizer_is_usable(&self) -> bool {
        self.electron_path
            .as_deref()
            .is_some_and(|path| path.is_file())
            && self
                .rasterizer_script
                .as_deref()
                .is_some_and(|path| path.is_file())
    }
}

#[derive(Debug, Error)]
pub(crate) enum ConfigError {
    #[error("could not prepare isolated OfficeCLI profile")]
    Profile { source: std::io::Error },
    #[error("could not prepare OfficeCLI artifact directory")]
    Artifact { source: std::io::Error },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OfficeCliAvailability {
    Available,
    Unavailable,
    VersionMismatch,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OfficeCliStatus {
    pub configured: bool,
    pub availability: OfficeCliAvailability,
    pub supported_version: String,
    pub detected_version: Option<String>,
    pub capabilities: Vec<String>,
}

impl OfficeCliStatus {
    pub(crate) fn unavailable(configured: bool, detected_version: Option<String>) -> Self {
        Self {
            configured,
            availability: if detected_version.is_some() {
                OfficeCliAvailability::VersionMismatch
            } else {
                OfficeCliAvailability::Unavailable
            },
            supported_version: SUPPORTED_VERSION.to_owned(),
            detected_version,
            capabilities: Vec::new(),
        }
    }

    pub(crate) fn available() -> Self {
        Self {
            configured: true,
            availability: OfficeCliAvailability::Available,
            supported_version: SUPPORTED_VERSION.to_owned(),
            detected_version: Some(SUPPORTED_VERSION.to_owned()),
            capabilities: vec![
                "view_text".to_owned(),
                "view_annotated".to_owned(),
                "view_outline".to_owned(),
                "view_stats".to_owned(),
                "view_issues".to_owned(),
                "get".to_owned(),
                "query".to_owned(),
                "validate".to_owned(),
                "screenshot".to_owned(),
                "create".to_owned(),
                "mutate_detached".to_owned(),
            ],
        }
    }

    pub fn is_available(&self) -> bool {
        self.availability == OfficeCliAvailability::Available
    }
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn isolated_profile_disables_updates_without_using_real_home() {
        let root =
            std::env::temp_dir().join(format!("9profs-officecli-isolation-{}", std::process::id()));
        let config = OfficeCliConfig {
            profile_root: root.join("profile"),
            artifact_root: root.join("artifacts"),
            ..OfficeCliConfig::default()
        };

        let environment = config
            .isolated_environment()
            .expect("isolated profile should be created");
        let profile = config.profile_root.as_os_str();

        for key in ["HOME", "USERPROFILE"] {
            assert_eq!(environment.get(key).map(OsString::as_os_str), Some(profile));
        }
        assert_eq!(
            environment.get("APPDATA").map(OsString::as_os_str),
            Some(config.profile_root.join("appdata").as_os_str())
        );
        assert_eq!(
            environment
                .get("OFFICECLI_NO_AUTO_INSTALL")
                .map(OsString::as_os_str),
            Some(OsStr::new("1"))
        );
        assert_eq!(
            environment
                .get("OFFICECLI_NO_AUTO_RESIDENT")
                .map(OsString::as_os_str),
            Some(OsStr::new("1"))
        );
        assert_eq!(
            environment
                .get("OFFICECLI_SKIP_UPDATE")
                .map(OsString::as_os_str),
            Some(OsStr::new("1"))
        );

        if let Some(real_home) =
            std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
        {
            assert_ne!(config.profile_root, Path::new(&real_home));
        }

        let profile_config =
            std::fs::read_to_string(config.profile_root.join(".officecli/config.json"))
                .expect("profile config should be written inside the isolated root");
        let profile_config: serde_json::Value =
            serde_json::from_str(&profile_config).expect("profile config should be JSON");
        assert_eq!(profile_config["AutoUpdate"], false);
        assert_eq!(profile_config["LastSkillRefreshVersion"], SUPPORTED_VERSION);

        let _ = std::fs::remove_dir_all(root);
    }
}
