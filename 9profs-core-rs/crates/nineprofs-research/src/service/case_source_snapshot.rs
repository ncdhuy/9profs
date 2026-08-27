use super::ResearchService;
use super::{not_found, sha256_hash};
use crate::{
    CaptureSourceSnapshot, ContentHash, CreateResearchCase, CreateResearchSource,
    MAX_CASE_TITLE_BYTES, MAX_SNAPSHOT_CONTENT_BYTES, MAX_SOURCE_LABEL_BYTES, ResearchCase,
    ResearchCaseId, ResearchError, ResearchRepository, ResearchSource, ResearchSourceId,
    ResearchSourceSnapshot, ResearchSourceSnapshotId, SafeMetadata, SourceOrigin, VerifiedArtifact,
    bounded_text, validate_metadata,
};
use nineprofs_common::now_ms;
use serde_json::json;

impl ResearchService {
    pub async fn list_cases(&self) -> Result<Vec<ResearchCase>, ResearchError> {
        self.repository.list_cases().await
    }

    pub async fn get_case(&self, id: &str) -> Result<ResearchCase, ResearchError> {
        let id = ResearchCaseId::parse(id.to_owned())?;
        self.repository
            .get_case(&id)
            .await?
            .ok_or_else(|| not_found("case", id.as_str()))
    }

    pub async fn create_case(
        &self,
        input: CreateResearchCase,
    ) -> Result<ResearchCase, ResearchError> {
        bounded_text("case title", &input.title, MAX_CASE_TITLE_BYTES)?;
        let timestamp = now_ms();
        let value = ResearchCase {
            id: ResearchCaseId::new(),
            title: input.title,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
        };
        self.repository.insert_case(&value).await?;
        self.publish("research.caseCreated", json!({ "case_id": value.id }));
        Ok(value)
    }

    pub async fn list_sources(
        &self,
        research_case_id: Option<&str>,
    ) -> Result<Vec<ResearchSource>, ResearchError> {
        let id = research_case_id
            .map(|id| ResearchCaseId::parse(id.to_owned()))
            .transpose()?;
        self.repository.list_sources(id.as_ref()).await
    }

    pub async fn get_source(&self, id: &str) -> Result<ResearchSource, ResearchError> {
        let id = ResearchSourceId::parse(id.to_owned())?;
        self.repository
            .get_source(&id)
            .await?
            .ok_or_else(|| not_found("source", id.as_str()))
    }

    pub async fn create_source(
        &self,
        input: CreateResearchSource,
    ) -> Result<ResearchSource, ResearchError> {
        let case = self
            .repository
            .get_case(&input.research_case_id)
            .await?
            .ok_or_else(|| not_found("case", input.research_case_id.as_str()))?;
        bounded_text("source label", &input.label, MAX_SOURCE_LABEL_BYTES)?;
        let value = ResearchSource {
            id: ResearchSourceId::new(),
            research_case_id: case.id,
            kind: input.kind,
            label: input.label,
            created_at_ms: now_ms(),
        };
        self.repository.insert_source(&value).await?;
        self.publish(
            "research.sourceCreated",
            json!({
                "source_id": value.id,
                "research_case_id": value.research_case_id,
                "kind": value.kind,
            }),
        );
        Ok(value)
    }

    pub async fn list_snapshots(
        &self,
        source_id: Option<&str>,
    ) -> Result<Vec<ResearchSourceSnapshot>, ResearchError> {
        let id = source_id
            .map(|id| ResearchSourceId::parse(id.to_owned()))
            .transpose()?;
        self.repository.list_snapshots(id.as_ref()).await
    }

    pub async fn get_snapshot(&self, id: &str) -> Result<ResearchSourceSnapshot, ResearchError> {
        let id = ResearchSourceSnapshotId::parse(id.to_owned())?;
        self.repository
            .get_snapshot(&id)
            .await?
            .ok_or_else(|| not_found("source snapshot", id.as_str()))
    }

    pub async fn capture_snapshot(
        &self,
        input: CaptureSourceSnapshot,
    ) -> Result<ResearchSourceSnapshot, ResearchError> {
        if input.content.is_empty() {
            return Err(ResearchError::Invalid(
                "snapshot content must not be empty".to_owned(),
            ));
        }
        if input.content.len() > MAX_SNAPSHOT_CONTENT_BYTES {
            return Err(ResearchError::Invalid(format!(
                "snapshot content exceeds {MAX_SNAPSHOT_CONTENT_BYTES} bytes"
            )));
        }
        input.origin.validate()?;
        validate_metadata(&input.metadata)?;
        if self
            .repository
            .get_source(&input.source_id)
            .await?
            .is_some_and(|source| matches!(source.kind, crate::SourceKind::ReferencePdf))
        {
            return Err(ResearchError::Invalid(
                "ReferencePdf snapshots require artifact-backed ingestion".to_owned(),
            ));
        }
        let content_hash = sha256_hash(&input.content);
        self.persist_snapshot(
            input.source_id,
            content_hash,
            input.capture_method,
            input.origin,
            input.metadata,
        )
        .await
    }

    pub async fn capture_verified_artifact_snapshot(
        &self,
        source_id: ResearchSourceId,
        artifact: &VerifiedArtifact,
        metadata: SafeMetadata,
    ) -> Result<ResearchSourceSnapshot, ResearchError> {
        self.persist_snapshot(
            source_id,
            artifact.content_hash().clone(),
            crate::CaptureMethod::UploadedArtifact,
            SourceOrigin::UploadedArtifact {
                artifact_id: artifact.artifact_id().to_owned(),
                revision_id: None,
            },
            metadata,
        )
        .await
    }

    async fn persist_snapshot(
        &self,
        source_id: ResearchSourceId,
        content_hash: ContentHash,
        capture_method: crate::CaptureMethod,
        origin: SourceOrigin,
        metadata: SafeMetadata,
    ) -> Result<ResearchSourceSnapshot, ResearchError> {
        origin.validate()?;
        validate_metadata(&metadata)?;
        if let Some(existing) = self
            .repository
            .find_snapshot_by_hash(&source_id, &content_hash)
            .await?
        {
            return Ok(existing);
        }

        if self.repository.get_source(&source_id).await?.is_none() {
            return Err(not_found("source", source_id.as_str()));
        }
        let value = ResearchSourceSnapshot {
            id: ResearchSourceSnapshotId::new(),
            source_id,
            content_hash,
            captured_at_ms: now_ms(),
            capture_method,
            origin,
            metadata,
        };
        if !self.repository.insert_snapshot(&value).await? {
            // Unique source/hash constraint makes concurrent duplicate captures
            // deterministic: return already persisted snapshot.
            return self
                .repository
                .find_snapshot_by_hash(&value.source_id, &value.content_hash)
                .await?
                .ok_or_else(|| {
                    ResearchError::Invalid(
                        "snapshot duplicate was detected but existing row was unavailable"
                            .to_owned(),
                    )
                });
        }
        self.publish(
            "research.snapshotCaptured",
            json!({ "snapshot_id": value.id, "source_id": value.source_id }),
        );
        Ok(value)
    }
}
