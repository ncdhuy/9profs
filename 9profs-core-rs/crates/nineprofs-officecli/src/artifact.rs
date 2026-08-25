use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use thiserror::Error;

use crate::operations::DocumentReference;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactKind {
    Detached,
    InspectionSnapshot,
}

#[derive(Clone, Debug)]
pub struct ResolvedDocument {
    pub id: String,
    pub path: PathBuf,
    pub kind: ArtifactKind,
}

pub trait DocumentResolver: Send + Sync {
    fn resolve(&self, reference: &DocumentReference) -> Result<ResolvedDocument, ArtifactError>;
}

#[derive(Clone, Default)]
pub struct ArtifactResolver {
    roots: Arc<Vec<PathBuf>>,
    artifacts: Arc<RwLock<BTreeMap<String, ResolvedDocument>>>,
}

impl ArtifactResolver {
    pub fn new(approved_roots: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            roots: Arc::new(approved_roots.into_iter().collect()),
            artifacts: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub fn register_detached(
        &self,
        id: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<(), ArtifactError> {
        self.register(id, path.into(), ArtifactKind::Detached)
    }

    pub fn register_inspection_snapshot(
        &self,
        id: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<(), ArtifactError> {
        self.register(id, path.into(), ArtifactKind::InspectionSnapshot)
    }

    fn register(
        &self,
        id: impl Into<String>,
        path: PathBuf,
        kind: ArtifactKind,
    ) -> Result<(), ArtifactError> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(ArtifactError::InvalidReference);
        }
        let path = canonical_document_path(&path)?;
        ensure_supported_extension(&path)?;
        ensure_contained(&path, &self.roots)?;
        self.artifacts
            .write()
            .expect("OfficeCLI artifact lock poisoned")
            .insert(id.clone(), ResolvedDocument { id, path, kind });
        Ok(())
    }

    pub fn resolve_path(&self, path: &Path) -> Result<PathBuf, ArtifactError> {
        let path = canonical_document_path(path)?;
        ensure_supported_extension(&path)?;
        ensure_contained(&path, &self.roots)?;
        Ok(path)
    }
}

impl DocumentResolver for ArtifactResolver {
    fn resolve(&self, reference: &DocumentReference) -> Result<ResolvedDocument, ArtifactError> {
        self.artifacts
            .read()
            .expect("OfficeCLI artifact lock poisoned")
            .get(&reference.artifact_id)
            .cloned()
            .ok_or(ArtifactError::UnknownArtifact)
    }
}

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("invalid document reference")]
    InvalidReference,
    #[error("document artifact is not approved")]
    UnknownArtifact,
    #[error("document artifact does not exist")]
    Missing(#[source] std::io::Error),
    #[error("document artifact path is outside approved roots")]
    OutsideApprovedRoots,
    #[error("document file type is not supported")]
    UnsupportedExtension,
}

fn canonical_document_path(path: &Path) -> Result<PathBuf, ArtifactError> {
    std::fs::canonicalize(path).map_err(|source| ArtifactError::Missing(source))
}

fn ensure_supported_extension(path: &Path) -> Result<(), ArtifactError> {
    let supported = matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("docx" | "xlsx" | "pptx")
    );
    supported
        .then_some(())
        .ok_or(ArtifactError::UnsupportedExtension)
}

fn ensure_contained(path: &Path, roots: &[PathBuf]) -> Result<(), ArtifactError> {
    for root in roots {
        let Ok(root) = std::fs::canonicalize(root) else {
            continue;
        };
        if path.starts_with(root) {
            return Ok(());
        }
    }
    Err(ArtifactError::OutsideApprovedRoots)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    fn temp_dir() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(1);
        let path = std::env::temp_dir().join(format!(
            "9profs-officecli-artifact-test-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn approved_artifact_and_supported_extension_required() {
        let root = temp_dir();
        let document = root.join("fixture.DOCX");
        fs::write(&document, b"fixture").unwrap();
        let resolver = ArtifactResolver::new([root.clone()]);
        resolver.register_detached("fixture", &document).unwrap();
        assert!(
            resolver
                .resolve(&DocumentReference {
                    artifact_id: "fixture".to_owned(),
                })
                .is_ok()
        );

        let unsupported = root.join("fixture.txt");
        fs::write(&unsupported, b"fixture").unwrap();
        assert!(matches!(
            resolver.register_detached("unsupported", unsupported),
            Err(ArtifactError::UnsupportedExtension)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unknown_reference_cannot_supply_a_path() {
        let resolver = ArtifactResolver::new([]);
        assert!(matches!(
            resolver.resolve(&DocumentReference {
                artifact_id: r"C:\secret.docx".to_owned(),
            }),
            Err(ArtifactError::UnknownArtifact)
        ));
    }
}
