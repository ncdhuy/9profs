use std::{
    fmt::Write as _,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

use nineprofs_common::{new_id, now_ms};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::{
    ContentHash, HashAlgorithm, MAX_PDF_BYTES, ResearchArtifact, ResearchError, VerifiedArtifact,
};

const PDF_MEDIA_TYPE: &str = "application/pdf";
const PDF_HEADER: &[u8] = b"%PDF-";

#[derive(Clone, Debug)]
pub struct ResearchArtifactStore {
    root: PathBuf,
    pool: SqlitePool,
    max_bytes: u64,
}

impl ResearchArtifactStore {
    pub fn new(root: PathBuf, pool: SqlitePool) -> Self {
        Self {
            root,
            pool,
            max_bytes: MAX_PDF_BYTES,
        }
    }

    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    pub fn begin_upload(
        &self,
        original_filename: impl AsRef<str>,
    ) -> Result<ArtifactUploadWriter, ResearchError> {
        let original_filename = safe_filename(original_filename.as_ref())?;
        std::fs::create_dir_all(&self.root).map_err(storage_error)?;
        let upload_id = new_id();
        let temp_path = self.root.join(format!(".upload-{upload_id}.tmp"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(storage_error)?;
        Ok(ArtifactUploadWriter {
            root: self.root.clone(),
            pool: self.pool.clone(),
            max_bytes: self.max_bytes,
            temp_path,
            file: Some(file),
            original_filename,
            hasher: Sha256::new(),
            size_bytes: 0,
            header: Vec::with_capacity(PDF_HEADER.len()),
        })
    }

    pub async fn get(&self, id: &str) -> Result<Option<ResearchArtifact>, ResearchError> {
        let row = sqlx::query(
            "SELECT id, hash_algorithm, content_hash, size_bytes, media_type, original_filename, created_at_ms \
             FROM research_artifacts WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let artifact = row.map(map_artifact).transpose()?;
        if let Some(artifact) = &artifact {
            let path = self
                .root
                .join(format!("{}.pdf", artifact.content_hash.value));
            if file_size(&path)? != artifact.size_bytes
                || file_hash(&path)? != artifact.content_hash
            {
                return Err(ResearchError::Artifact(
                    "stored artifact bytes do not match metadata".to_owned(),
                ));
            }
        }
        Ok(artifact)
    }

    /// Returns the verified content-addressed path for an existing artifact.
    /// Callers use this path as input to local extraction tools; the artifact
    /// bytes remain owned and integrity-checked by this store.
    pub async fn verified_path(&self, id: &str) -> Result<Option<PathBuf>, ResearchError> {
        let Some(artifact) = self.get(id).await? else {
            return Ok(None);
        };
        Ok(Some(
            self.root
                .join(format!("{}.pdf", artifact.content_hash.value)),
        ))
    }
}

pub struct ArtifactUploadWriter {
    root: PathBuf,
    pool: SqlitePool,
    max_bytes: u64,
    temp_path: PathBuf,
    file: Option<File>,
    original_filename: String,
    hasher: Sha256,
    size_bytes: u64,
    header: Vec<u8>,
}

impl ArtifactUploadWriter {
    pub fn append(&mut self, chunk: &[u8]) -> Result<(), ResearchError> {
        let size = self
            .size_bytes
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| ResearchError::Invalid("PDF upload size overflow".to_owned()))?;
        if size > self.max_bytes {
            return Err(ResearchError::Invalid(format!(
                "PDF upload exceeds {} bytes",
                self.max_bytes
            )));
        }
        if self.header.len() < PDF_HEADER.len() {
            let remaining = PDF_HEADER.len() - self.header.len();
            self.header
                .extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        }
        self.file
            .as_mut()
            .expect("artifact upload file remains open")
            .write_all(chunk)
            .map_err(storage_error)?;
        self.hasher.update(chunk);
        self.size_bytes = size;
        Ok(())
    }

    pub async fn finish(mut self) -> Result<VerifiedArtifact, ResearchError> {
        if self.size_bytes == 0 {
            return Err(ResearchError::Invalid(
                "PDF upload must not be empty".to_owned(),
            ));
        }
        if self.header.as_slice() != PDF_HEADER {
            return Err(ResearchError::Invalid(
                "uploaded artifact is not a PDF".to_owned(),
            ));
        }
        let mut file = self.file.take().expect("artifact upload file remains open");
        file.flush().map_err(storage_error)?;
        file.sync_all().map_err(storage_error)?;
        drop(file);

        let digest = std::mem::replace(&mut self.hasher, Sha256::new()).finalize();
        let content_hash = hex_hash(&digest);
        let final_path = self.root.join(format!("{}.pdf", content_hash.value));
        let mut promoted = false;
        if final_path.exists() {
            if file_hash(&final_path)? != content_hash || file_size(&final_path)? != self.size_bytes
            {
                return Err(ResearchError::Artifact(
                    "content-addressed artifact path contains different bytes".to_owned(),
                ));
            }
            std::fs::remove_file(&self.temp_path).map_err(storage_error)?;
        } else {
            match std::fs::rename(&self.temp_path, &final_path) {
                Ok(()) => promoted = true,
                Err(_error) if final_path.exists() => {
                    if file_hash(&final_path)? != content_hash
                        || file_size(&final_path)? != self.size_bytes
                    {
                        return Err(ResearchError::Artifact(
                            "content-addressed artifact path contains different bytes".to_owned(),
                        ));
                    }
                    std::fs::remove_file(&self.temp_path).map_err(storage_error)?;
                }
                Err(error) => return Err(storage_error(error)),
            }
        }

        let artifact_id = new_id();
        let insert = sqlx::query(
            "INSERT INTO research_artifacts \
             (id, hash_algorithm, content_hash, size_bytes, media_type, original_filename, created_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(content_hash) DO NOTHING",
        )
        .bind(&artifact_id)
        .bind("sha256")
        .bind(&content_hash.value)
        .bind(self.size_bytes as i64)
        .bind(PDF_MEDIA_TYPE)
        .bind(&self.original_filename)
        .bind(now_ms())
        .execute(&self.pool)
        .await;
        if let Err(error) = insert {
            if promoted {
                let _ = std::fs::remove_file(&final_path);
            }
            return Err(error.into());
        }

        let row = sqlx::query(
            "SELECT id, hash_algorithm, content_hash, size_bytes, media_type, original_filename, created_at_ms \
             FROM research_artifacts WHERE content_hash = ?",
        )
        .bind(&content_hash.value)
        .fetch_one(&self.pool)
        .await;
        match row {
            Ok(row) => Ok(VerifiedArtifact::new(map_artifact(row)?)),
            Err(error) => {
                if promoted {
                    let _ = std::fs::remove_file(&final_path);
                }
                Err(error.into())
            }
        }
    }
}

impl Drop for ArtifactUploadWriter {
    fn drop(&mut self) {
        self.file.take();
        let _ = std::fs::remove_file(&self.temp_path);
    }
}

fn safe_filename(value: &str) -> Result<String, ResearchError> {
    let filename = value.rsplit(['/', '\\']).next().unwrap_or(value);
    crate::bounded_text("original filename", filename, crate::MAX_SOURCE_LABEL_BYTES)?;
    Ok(filename.to_owned())
}

fn map_artifact(row: sqlx::sqlite::SqliteRow) -> Result<ResearchArtifact, ResearchError> {
    Ok(ResearchArtifact {
        id: row.get("id"),
        content_hash: ContentHash {
            algorithm: HashAlgorithm::Sha256,
            value: row.get("content_hash"),
        },
        size_bytes: row.get::<i64, _>("size_bytes") as u64,
        media_type: row.get("media_type"),
        original_filename: row.get("original_filename"),
        created_at_ms: row.get("created_at_ms"),
    })
}

fn hex_hash(digest: &[u8]) -> ContentHash {
    let mut value = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut value, "{byte:02x}").expect("writing to String cannot fail");
    }
    ContentHash {
        algorithm: HashAlgorithm::Sha256,
        value,
    }
}

fn file_size(path: &PathBuf) -> Result<u64, ResearchError> {
    Ok(std::fs::metadata(path).map_err(storage_error)?.len())
}

fn file_hash(path: &PathBuf) -> Result<ContentHash, ResearchError> {
    let mut file = File::open(path).map_err(storage_error)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(storage_error)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex_hash(&hasher.finalize()))
}

fn storage_error(error: std::io::Error) -> ResearchError {
    ResearchError::Artifact(error.to_string())
}
