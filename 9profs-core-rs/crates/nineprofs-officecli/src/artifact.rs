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
    GenOfficeActive,
    ReadOnly,
    NewlyCreated,
}

#[derive(Clone, Debug)]
pub struct ResolvedDocument {
    pub id: String,
    pub path: PathBuf,
    pub kind: ArtifactKind,
}

pub trait DocumentResolver: Send + Sync {
    fn resolve(&self, reference: &DocumentReference) -> Result<ResolvedDocument, ArtifactError>;

    fn resolve_writable(
        &self,
        reference: &DocumentReference,
    ) -> Result<WritableDetachedArtifact, ArtifactError> {
        let document = self.resolve(reference)?;
        match document.kind {
            ArtifactKind::Detached | ArtifactKind::NewlyCreated => {
                Ok(WritableDetachedArtifact { document })
            }
            ArtifactKind::InspectionSnapshot
            | ArtifactKind::GenOfficeActive
            | ArtifactKind::ReadOnly => Err(ArtifactError::NotWritable),
        }
    }

    fn writable_root(&self) -> Option<PathBuf> {
        None
    }

    fn register_detached_revision(&self, _id: String, _path: PathBuf) -> Result<(), ArtifactError> {
        Err(ArtifactError::RegistrationUnavailable)
    }
}

#[derive(Clone, Debug)]
pub struct WritableDetachedArtifact {
    pub document: ResolvedDocument,
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

    pub fn register_genoffice_active(
        &self,
        id: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<(), ArtifactError> {
        self.register(id, path.into(), ArtifactKind::GenOfficeActive)
    }

    pub fn register_read_only(
        &self,
        id: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<(), ArtifactError> {
        self.register(id, path.into(), ArtifactKind::ReadOnly)
    }

    pub fn register_newly_created(
        &self,
        id: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<(), ArtifactError> {
        self.register(id, path.into(), ArtifactKind::NewlyCreated)
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
        let mut artifact = self
            .artifacts
            .read()
            .expect("OfficeCLI artifact lock poisoned")
            .get(&reference.artifact_id)
            .cloned()
            .ok_or(ArtifactError::UnknownArtifact)?;
        artifact.path = canonical_document_path(&artifact.path)?;
        ensure_supported_extension(&artifact.path)?;
        ensure_contained(&artifact.path, &self.roots)?;
        Ok(artifact)
    }

    fn resolve_writable(
        &self,
        reference: &DocumentReference,
    ) -> Result<WritableDetachedArtifact, ArtifactError> {
        let document = self.resolve(reference)?;
        match document.kind {
            ArtifactKind::Detached | ArtifactKind::NewlyCreated => {
                Ok(WritableDetachedArtifact { document })
            }
            ArtifactKind::InspectionSnapshot
            | ArtifactKind::GenOfficeActive
            | ArtifactKind::ReadOnly => Err(ArtifactError::NotWritable),
        }
    }

    fn writable_root(&self) -> Option<PathBuf> {
        self.roots
            .iter()
            .find_map(|root| std::fs::canonicalize(root).ok())
    }

    fn register_detached_revision(&self, id: String, path: PathBuf) -> Result<(), ArtifactError> {
        self.register(id, path, ArtifactKind::Detached)
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
    #[error("document artifact is not writable")]
    NotWritable,
    #[error("document resolver cannot register a new artifact revision")]
    RegistrationUnavailable,
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
    fn writable_resolution_rejects_snapshot_active_and_read_only_artifacts() {
        let root = temp_dir();
        for name in ["snapshot", "active", "read-only"] {
            fs::write(root.join(format!("{name}.docx")), b"fixture").unwrap();
        }
        let resolver = ArtifactResolver::new([root.clone()]);
        resolver
            .register_inspection_snapshot("snapshot", root.join("snapshot.docx"))
            .unwrap();
        resolver
            .register_genoffice_active("active", root.join("active.docx"))
            .unwrap();
        resolver
            .register_read_only("read-only", root.join("read-only.docx"))
            .unwrap();
        for id in ["snapshot", "active", "read-only"] {
            assert!(matches!(
                resolver.resolve_writable(&DocumentReference {
                    artifact_id: id.to_owned(),
                }),
                Err(ArtifactError::NotWritable)
            ));
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn newly_created_artifact_is_writable() {
        let root = temp_dir();
        let path = root.join("created.docx");
        fs::write(&path, b"fixture").unwrap();
        let resolver = ArtifactResolver::new([root.clone()]);
        resolver.register_newly_created("created", &path).unwrap();
        assert!(
            resolver
                .resolve_writable(&DocumentReference {
                    artifact_id: "created".to_owned(),
                })
                .is_ok()
        );
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

    #[test]
    fn registered_artifact_is_revalidated_after_move() {
        let root = temp_dir();
        let document = root.join("registered.docx");
        fs::write(&document, b"fixture").unwrap();
        let moved = root.join("moved.docx");
        let resolver = ArtifactResolver::new([root.clone()]);
        resolver.register_detached("fixture", &document).unwrap();
        fs::rename(&document, &moved).unwrap();

        assert!(matches!(
            resolver.resolve(&DocumentReference {
                artifact_id: "fixture".to_owned(),
            }),
            Err(ArtifactError::Missing(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn registered_artifact_rejects_post_registration_escape_link() {
        let root = temp_dir();
        let approved = root.join("approved");
        let outside = root.join("outside");
        fs::create_dir_all(&approved).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let registered = approved.join("registered.docx");
        let escaped = outside.join("escaped.docx");
        fs::write(&registered, b"approved").unwrap();
        fs::write(&escaped, b"outside").unwrap();
        let resolver = ArtifactResolver::new([approved]);
        resolver.register_detached("fixture", &registered).unwrap();
        fs::remove_file(&registered).unwrap();

        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&escaped, &registered).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&escaped, &registered).is_ok();
        if !linked {
            let _ = fs::remove_dir_all(root);
            return;
        }

        assert!(matches!(
            resolver.resolve(&DocumentReference {
                artifact_id: "fixture".to_owned(),
            }),
            Err(ArtifactError::OutsideApprovedRoots)
        ));
        let _ = fs::remove_dir_all(root);
    }
}
