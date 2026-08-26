use std::sync::Arc;

use nineprofs_api_types::EventEnvelope;
use nineprofs_common::now_ms;
use nineprofs_realtime::BroadcastEventBus;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    CaptureSourceSnapshot, ClaimEvidenceLink, ContentHash, CreateClaimEvidenceLink,
    CreateResearchCase, CreateResearchClaim, CreateResearchEvidence, CreateResearchSource,
    HashAlgorithm, MAX_CASE_TITLE_BYTES, MAX_CLAIM_TEXT_BYTES, MAX_EVIDENCE_EXCERPT_BYTES,
    MAX_NORMALIZED_TEXT_BYTES, MAX_RATIONALE_BYTES, MAX_SNAPSHOT_CONTENT_BYTES,
    MAX_SOURCE_LABEL_BYTES, ResearchCase, ResearchCaseId, ResearchClaim, ResearchClaimId,
    ResearchError, ResearchEvidence, ResearchEvidenceId, ResearchRepository, ResearchSource,
    ResearchSourceId, ResearchSourceSnapshot, ResearchSourceSnapshotId, bounded_text,
    validate_metadata,
};

#[derive(Clone)]
pub struct ResearchService {
    repository: crate::SqliteResearchRepository,
    events: Arc<BroadcastEventBus>,
}

impl ResearchService {
    pub fn new(
        repository: crate::SqliteResearchRepository,
        events: Arc<BroadcastEventBus>,
    ) -> Self {
        Self { repository, events }
    }

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
        let content_hash = sha256_hash(&input.content);
        if let Some(existing) = self
            .repository
            .find_snapshot_by_hash(&input.source_id, &content_hash)
            .await?
        {
            return Ok(existing);
        }

        if self
            .repository
            .get_source(&input.source_id)
            .await?
            .is_none()
        {
            return Err(not_found("source", input.source_id.as_str()));
        }
        let value = ResearchSourceSnapshot {
            id: ResearchSourceSnapshotId::new(),
            source_id: input.source_id,
            content_hash,
            captured_at_ms: now_ms(),
            capture_method: input.capture_method,
            origin: input.origin,
            metadata: input.metadata,
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

    pub async fn list_evidence(
        &self,
        research_case_id: Option<&str>,
        source_snapshot_id: Option<&str>,
    ) -> Result<Vec<ResearchEvidence>, ResearchError> {
        let case_id = research_case_id
            .map(|id| ResearchCaseId::parse(id.to_owned()))
            .transpose()?;
        let snapshot_id = source_snapshot_id
            .map(|id| ResearchSourceSnapshotId::parse(id.to_owned()))
            .transpose()?;
        self.repository
            .list_evidence(case_id.as_ref(), snapshot_id.as_ref())
            .await
    }

    pub async fn get_evidence(&self, id: &str) -> Result<ResearchEvidence, ResearchError> {
        let id = ResearchEvidenceId::parse(id.to_owned())?;
        self.repository
            .get_evidence(&id)
            .await?
            .ok_or_else(|| not_found("evidence", id.as_str()))
    }

    pub async fn create_evidence(
        &self,
        input: CreateResearchEvidence,
    ) -> Result<ResearchEvidence, ResearchError> {
        self.ensure_case(&input.research_case_id).await?;
        let snapshot = self
            .repository
            .get_snapshot(&input.source_snapshot_id)
            .await?
            .ok_or_else(|| not_found("source snapshot", input.source_snapshot_id.as_str()))?;
        let source = self
            .repository
            .get_source(&snapshot.source_id)
            .await?
            .ok_or_else(|| not_found("source", snapshot.source_id.as_str()))?;
        if source.research_case_id != input.research_case_id {
            return Err(ResearchError::Invalid(
                "evidence source snapshot belongs to another research case".to_owned(),
            ));
        }
        bounded_text(
            "verbatim excerpt",
            &input.verbatim_excerpt,
            MAX_EVIDENCE_EXCERPT_BYTES,
        )?;
        if let Some(normalized_text) = &input.normalized_text {
            bounded_text(
                "normalized text",
                normalized_text,
                MAX_NORMALIZED_TEXT_BYTES,
            )?;
        }
        input.locator.validate()?;
        let value = ResearchEvidence {
            id: ResearchEvidenceId::new(),
            research_case_id: input.research_case_id,
            source_snapshot_id: input.source_snapshot_id,
            excerpt_hash: sha256_hash(input.verbatim_excerpt.as_bytes()),
            verbatim_excerpt: input.verbatim_excerpt,
            normalized_text: input.normalized_text,
            locator: input.locator,
            captured_at_ms: now_ms(),
            capture_method: input.capture_method,
        };
        self.repository.insert_evidence(&value).await?;
        self.publish(
            "research.evidenceCaptured",
            json!({
                "evidence_id": value.id,
                "research_case_id": value.research_case_id,
                "source_snapshot_id": value.source_snapshot_id,
            }),
        );
        Ok(value)
    }

    pub async fn list_claims(
        &self,
        research_case_id: Option<&str>,
    ) -> Result<Vec<ResearchClaim>, ResearchError> {
        let id = research_case_id
            .map(|id| ResearchCaseId::parse(id.to_owned()))
            .transpose()?;
        self.repository.list_claims(id.as_ref()).await
    }

    pub async fn get_claim(&self, id: &str) -> Result<ResearchClaim, ResearchError> {
        let id = ResearchClaimId::parse(id.to_owned())?;
        self.repository
            .get_claim(&id)
            .await?
            .ok_or_else(|| not_found("claim", id.as_str()))
    }

    pub async fn create_claim(
        &self,
        input: CreateResearchClaim,
    ) -> Result<ResearchClaim, ResearchError> {
        self.ensure_case(&input.research_case_id).await?;
        bounded_text("claim text", &input.text, MAX_CLAIM_TEXT_BYTES)?;
        input.origin.validate()?;
        let value = ResearchClaim {
            id: ResearchClaimId::new(),
            research_case_id: input.research_case_id,
            text: input.text,
            origin: input.origin,
            created_at_ms: now_ms(),
        };
        self.repository.insert_claim(&value).await?;
        self.publish(
            "research.claimCreated",
            json!({ "claim_id": value.id, "research_case_id": value.research_case_id }),
        );
        Ok(value)
    }

    pub async fn list_links(
        &self,
        research_case_id: Option<&str>,
        claim_id: Option<&str>,
        evidence_id: Option<&str>,
    ) -> Result<Vec<ClaimEvidenceLink>, ResearchError> {
        let case_id = research_case_id
            .map(|id| ResearchCaseId::parse(id.to_owned()))
            .transpose()?;
        let claim_id = claim_id
            .map(|id| ResearchClaimId::parse(id.to_owned()))
            .transpose()?;
        let evidence_id = evidence_id
            .map(|id| ResearchEvidenceId::parse(id.to_owned()))
            .transpose()?;
        self.repository
            .list_links(case_id.as_ref(), claim_id.as_ref(), evidence_id.as_ref())
            .await
    }

    pub async fn get_link(&self, id: &str) -> Result<ClaimEvidenceLink, ResearchError> {
        let id = crate::ClaimEvidenceLinkId::parse(id.to_owned())?;
        self.repository
            .get_link(&id)
            .await?
            .ok_or_else(|| not_found("claim-evidence link", id.as_str()))
    }

    pub async fn create_link(
        &self,
        input: CreateClaimEvidenceLink,
    ) -> Result<ClaimEvidenceLink, ResearchError> {
        self.ensure_case(&input.research_case_id).await?;
        let claim = self
            .repository
            .get_claim(&input.claim_id)
            .await?
            .ok_or_else(|| not_found("claim", input.claim_id.as_str()))?;
        let evidence = self
            .repository
            .get_evidence(&input.evidence_id)
            .await?
            .ok_or_else(|| not_found("evidence", input.evidence_id.as_str()))?;
        if claim.research_case_id != input.research_case_id
            || evidence.research_case_id != input.research_case_id
        {
            return Err(ResearchError::Invalid(
                "claim and evidence must belong to same research case as assessment".to_owned(),
            ));
        }
        if let Some(rationale) = &input.rationale {
            bounded_text("assessment rationale", rationale, MAX_RATIONALE_BYTES)?;
        }
        validate_metadata(&input.assessment_metadata)?;
        let value = ClaimEvidenceLink {
            id: crate::ClaimEvidenceLinkId::new(),
            research_case_id: input.research_case_id,
            claim_id: input.claim_id,
            evidence_id: input.evidence_id,
            relation: input.relation,
            rationale: input.rationale,
            assessment_method: input.assessment_method,
            assessment_metadata: input.assessment_metadata,
            created_at_ms: now_ms(),
        };
        self.repository.insert_link(&value).await?;
        self.publish(
            "research.assessmentCreated",
            json!({
                "link_id": value.id,
                "research_case_id": value.research_case_id,
                "claim_id": value.claim_id,
                "evidence_id": value.evidence_id,
                "relation": value.relation,
                "assessment_method": value.assessment_method,
            }),
        );
        Ok(value)
    }

    async fn ensure_case(&self, id: &ResearchCaseId) -> Result<(), ResearchError> {
        if self.repository.get_case(id).await?.is_none() {
            return Err(not_found("case", id.as_str()));
        }
        Ok(())
    }

    fn publish(&self, name: &str, payload: serde_json::Value) {
        let _ = self.events.publish(EventEnvelope::new(name, payload));
    }
}

fn not_found(entity: &'static str, id: &str) -> ResearchError {
    ResearchError::NotFound {
        entity,
        id: id.to_owned(),
    }
}

fn sha256_hash(value: &[u8]) -> ContentHash {
    let digest = Sha256::digest(value);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    ContentHash {
        algorithm: HashAlgorithm::Sha256,
        value: hex,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::MAX_METADATA_BYTES;
    use nineprofs_db::Database;

    use crate::{
        AssessmentMethod, CaptureMethod, ClaimEvidenceRelation, ClaimOrigin, EvidenceLocator,
        SourceKind, SourceOrigin,
    };

    use super::*;

    async fn service() -> (Database, ResearchService) {
        let database = Database::in_memory().await.unwrap();
        let service = ResearchService::new(
            crate::SqliteResearchRepository::new(database.pool().clone()),
            Arc::new(BroadcastEventBus::new(64)),
        );
        (database, service)
    }

    fn origin() -> SourceOrigin {
        SourceOrigin::UploadedArtifact {
            artifact_id: "artifact-1".to_owned(),
            revision_id: Some("revision-1".to_owned()),
        }
    }

    fn snapshot_input(source_id: ResearchSourceId, content: &[u8]) -> CaptureSourceSnapshot {
        CaptureSourceSnapshot {
            source_id,
            content: content.to_vec(),
            capture_method: CaptureMethod::UploadedArtifact,
            origin: origin(),
            metadata: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn case_source_snapshot_and_evidence_preserve_provenance() {
        let (_database, service) = service().await;
        let case = service
            .create_case(CreateResearchCase {
                title: "Review".to_owned(),
            })
            .await
            .unwrap();
        assert!(!case.id.to_string().is_empty());
        let source = service
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::Manuscript,
                label: "Draft".to_owned(),
            })
            .await
            .unwrap();
        let first = service
            .capture_snapshot(snapshot_input(source.id.clone(), b"version one"))
            .await
            .unwrap();
        let evidence = service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case.id.clone(),
                source_snapshot_id: first.id.clone(),
                verbatim_excerpt: "exact words".to_owned(),
                normalized_text: Some("normalized words".to_owned()),
                locator: EvidenceLocator::TextRange { start: 2, end: 13 },
                capture_method: CaptureMethod::ActiveDocument,
            })
            .await
            .unwrap();
        let second = service
            .capture_snapshot(snapshot_input(source.id.clone(), b"version two"))
            .await
            .unwrap();

        assert_ne!(first.content_hash.value, second.content_hash.value);
        assert_eq!(evidence.source_snapshot_id, first.id);
        assert_eq!(
            service
                .get_snapshot(&evidence.source_snapshot_id.to_string())
                .await
                .unwrap()
                .content_hash
                .value,
            first.content_hash.value
        );
    }

    #[tokio::test]
    async fn same_source_duplicate_snapshot_returns_existing_and_other_sources_stay_distinct() {
        let (_database, service) = service().await;
        let case = service
            .create_case(CreateResearchCase {
                title: "Review".to_owned(),
            })
            .await
            .unwrap();
        let case_id = case.id.clone();
        let first_source = service
            .create_source(CreateResearchSource {
                research_case_id: case_id.clone(),
                kind: SourceKind::Dataset,
                label: "First".to_owned(),
            })
            .await
            .unwrap();
        let second_source = service
            .create_source(CreateResearchSource {
                research_case_id: case_id.clone(),
                kind: SourceKind::Dataset,
                label: "Second".to_owned(),
            })
            .await
            .unwrap();
        let one = service
            .capture_snapshot(snapshot_input(first_source.id.clone(), b"same"))
            .await
            .unwrap();
        let duplicate = service
            .capture_snapshot(snapshot_input(first_source.id, b"same"))
            .await
            .unwrap();
        let other_source = service
            .capture_snapshot(snapshot_input(second_source.id, b"same"))
            .await
            .unwrap();
        assert_eq!(one.id, duplicate.id);
        assert_ne!(one.id, other_source.id);
        let first_evidence = service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case_id.clone(),
                source_snapshot_id: one.id,
                verbatim_excerpt: "same words".to_owned(),
                normalized_text: None,
                locator: EvidenceLocator::TextRange { start: 0, end: 10 },
                capture_method: CaptureMethod::UploadedArtifact,
            })
            .await
            .unwrap();
        let second_evidence = service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case_id,
                source_snapshot_id: other_source.id,
                verbatim_excerpt: "same words".to_owned(),
                normalized_text: None,
                locator: EvidenceLocator::TextRange { start: 0, end: 10 },
                capture_method: CaptureMethod::UploadedArtifact,
            })
            .await
            .unwrap();
        assert_eq!(first_evidence.excerpt_hash, second_evidence.excerpt_hash);
        assert_ne!(first_evidence.id, second_evidence.id);
    }

    #[tokio::test]
    async fn claim_without_link_has_no_assessment_and_relations_are_categorical() {
        let (_database, service) = service().await;
        let case = service
            .create_case(CreateResearchCase {
                title: "Review".to_owned(),
            })
            .await
            .unwrap();
        let source = service
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::Web,
                label: "Web source".to_owned(),
            })
            .await
            .unwrap();
        let snapshot = service
            .capture_snapshot(snapshot_input(source.id, b"source"))
            .await
            .unwrap();
        let evidence = service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case.id.clone(),
                source_snapshot_id: snapshot.id,
                verbatim_excerpt: "source says X".to_owned(),
                normalized_text: None,
                locator: EvidenceLocator::Web {
                    fragment: Some("#section".to_owned()),
                    start: None,
                    end: None,
                },
                capture_method: CaptureMethod::WebRetrieval,
            })
            .await
            .unwrap();
        let claim = service
            .create_claim(CreateResearchClaim {
                research_case_id: case.id.clone(),
                text: "Claim X".to_owned(),
                origin: ClaimOrigin::User,
            })
            .await
            .unwrap();
        assert!(
            service
                .list_links(Some(case.id.as_str()), None, None)
                .await
                .unwrap()
                .is_empty()
        );
        let relations = [
            ClaimEvidenceRelation::Supports,
            ClaimEvidenceRelation::Contradicts,
            ClaimEvidenceRelation::Contextualizes,
            ClaimEvidenceRelation::Insufficient,
        ];
        for relation in relations {
            service
                .create_link(CreateClaimEvidenceLink {
                    research_case_id: case.id.clone(),
                    claim_id: claim.id.clone(),
                    evidence_id: evidence.id.clone(),
                    relation,
                    rationale: Some("The excerpt is assessed against the claim.".to_owned()),
                    assessment_method: AssessmentMethod::Human,
                    assessment_metadata: BTreeMap::new(),
                })
                .await
                .unwrap();
        }
        assert_eq!(
            service
                .list_links(Some(case.id.as_str()), None, None)
                .await
                .unwrap()
                .len(),
            4
        );
    }

    #[tokio::test]
    async fn persistence_round_trip_survives_service_recreation() {
        let database = Database::in_memory().await.unwrap();
        let events = Arc::new(BroadcastEventBus::new(64));
        let first_service = ResearchService::new(
            crate::SqliteResearchRepository::new(database.pool().clone()),
            Arc::clone(&events),
        );
        let case = first_service
            .create_case(CreateResearchCase {
                title: "Persistent".to_owned(),
            })
            .await
            .unwrap();
        let source = first_service
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::ReferencePdf,
                label: "Reference".to_owned(),
            })
            .await
            .unwrap();
        let snapshot = first_service
            .capture_snapshot(snapshot_input(source.id, b"captured"))
            .await
            .unwrap();
        let evidence = first_service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case.id.clone(),
                source_snapshot_id: snapshot.id,
                verbatim_excerpt: "verbatim".to_owned(),
                normalized_text: None,
                locator: EvidenceLocator::Pdf {
                    page: 4,
                    end_page: Some(5),
                },
                capture_method: CaptureMethod::UploadedArtifact,
            })
            .await
            .unwrap();
        let claim = first_service
            .create_claim(CreateResearchClaim {
                research_case_id: case.id.clone(),
                text: "claim".to_owned(),
                origin: ClaimOrigin::Imported {
                    source: "fixture".to_owned(),
                },
            })
            .await
            .unwrap();
        let link = first_service
            .create_link(CreateClaimEvidenceLink {
                research_case_id: case.id.clone(),
                claim_id: claim.id,
                evidence_id: evidence.id,
                relation: ClaimEvidenceRelation::Contextualizes,
                rationale: None,
                assessment_method: AssessmentMethod::DeterministicChecker,
                assessment_metadata: BTreeMap::from([("score".to_owned(), "0.5".to_owned())]),
            })
            .await
            .unwrap();

        let restarted = ResearchService::new(
            crate::SqliteResearchRepository::new(database.pool().clone()),
            events,
        );
        assert_eq!(restarted.get_case(case.id.as_str()).await.unwrap(), case);
        assert_eq!(
            restarted
                .list_evidence(Some(case.id.as_str()), None)
                .await
                .unwrap()[0]
                .excerpt_hash,
            evidence.excerpt_hash
        );
        assert_eq!(restarted.get_link(link.id.as_str()).await.unwrap(), link);
    }

    #[tokio::test]
    async fn foreign_references_and_secret_metadata_are_rejected() {
        let (_database, service) = service().await;
        let case = service
            .create_case(CreateResearchCase {
                title: "Review".to_owned(),
            })
            .await
            .unwrap();
        let case_id = case.id.clone();
        let source = service
            .create_source(CreateResearchSource {
                research_case_id: case_id.clone(),
                kind: SourceKind::Other,
                label: "Source".to_owned(),
            })
            .await
            .unwrap();
        assert!(matches!(
            service
                .create_source(CreateResearchSource {
                    research_case_id: ResearchCaseId::parse("missing-case").unwrap(),
                    kind: SourceKind::Other,
                    label: "Invalid".to_owned(),
                })
                .await,
            Err(ResearchError::NotFound { entity: "case", .. })
        ));
        assert!(matches!(
            service
                .capture_snapshot(snapshot_input(
                    ResearchSourceId::parse("missing-source").unwrap(),
                    b"content"
                ))
                .await,
            Err(ResearchError::NotFound {
                entity: "source",
                ..
            })
        ));
        let mut input = snapshot_input(source.id.clone(), b"content");
        input
            .metadata
            .insert("authorization".to_owned(), "secret".to_owned());
        assert!(
            matches!(service.capture_snapshot(input).await, Err(ResearchError::Invalid(message)) if message.contains("metadata key"))
        );
        let mut oversized_metadata = snapshot_input(source.id.clone(), b"content-2");
        oversized_metadata
            .metadata
            .insert("note".to_owned(), "x".repeat(MAX_METADATA_BYTES));
        assert!(matches!(
            service.capture_snapshot(oversized_metadata).await,
            Err(ResearchError::Invalid(message)) if message.contains("metadata exceeds")
        ));
        assert!(matches!(
            service.get_source("missing").await,
            Err(ResearchError::NotFound {
                entity: "source",
                ..
            })
        ));
        let snapshot = service
            .capture_snapshot(snapshot_input(source.id, b"content"))
            .await
            .unwrap();
        assert!(matches!(
            service
                .create_evidence(CreateResearchEvidence {
                    research_case_id: case_id.clone(),
                    source_snapshot_id: ResearchSourceSnapshotId::parse("missing-snapshot")
                        .unwrap(),
                    verbatim_excerpt: "excerpt".to_owned(),
                    normalized_text: None,
                    locator: EvidenceLocator::TextRange { start: 0, end: 1 },
                    capture_method: CaptureMethod::UploadedArtifact,
                })
                .await,
            Err(ResearchError::NotFound {
                entity: "source snapshot",
                ..
            })
        ));
        assert!(matches!(
            service
                .create_evidence(CreateResearchEvidence {
                    research_case_id: case_id.clone(),
                    source_snapshot_id: snapshot.id.clone(),
                    verbatim_excerpt: "excerpt".to_owned(),
                    normalized_text: None,
                    locator: EvidenceLocator::Pdf {
                        page: 0,
                        end_page: None,
                    },
                    capture_method: CaptureMethod::UploadedArtifact,
            })
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("page range")
        ));
        assert!(matches!(
            service
                .create_evidence(CreateResearchEvidence {
                    research_case_id: case_id.clone(),
                    source_snapshot_id: snapshot.id.clone(),
                    verbatim_excerpt: "x".repeat(MAX_EVIDENCE_EXCERPT_BYTES + 1),
                    normalized_text: None,
                    locator: EvidenceLocator::TextRange { start: 0, end: 1 },
                    capture_method: CaptureMethod::UploadedArtifact,
                })
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("verbatim excerpt")
        ));
        assert!(matches!(
            service
                .create_claim(CreateResearchClaim {
                    research_case_id: case_id.clone(),
                    text: "x".repeat(MAX_CLAIM_TEXT_BYTES + 1),
                    origin: ClaimOrigin::Agent,
                })
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("claim text")
        ));
        assert!(serde_json::from_str::<ClaimEvidenceRelation>("\"unknown\"").is_err());

        let second_case = service
            .create_case(CreateResearchCase {
                title: "Second".to_owned(),
            })
            .await
            .unwrap();
        let second_claim = service
            .create_claim(CreateResearchClaim {
                research_case_id: second_case.id.clone(),
                text: "second claim".to_owned(),
                origin: ClaimOrigin::Agent,
            })
            .await
            .unwrap();
        let evidence = service
            .create_evidence(CreateResearchEvidence {
                research_case_id: case_id.clone(),
                source_snapshot_id: snapshot.id,
                verbatim_excerpt: "excerpt".to_owned(),
                normalized_text: None,
                locator: EvidenceLocator::TextRange { start: 0, end: 1 },
                capture_method: CaptureMethod::UploadedArtifact,
            })
            .await
            .unwrap();
        assert!(matches!(
            service
                .create_link(CreateClaimEvidenceLink {
                    research_case_id: second_case.id,
                    claim_id: second_claim.id,
                    evidence_id: evidence.id,
                    relation: ClaimEvidenceRelation::Supports,
                    rationale: None,
                    assessment_method: AssessmentMethod::Human,
                    assessment_metadata: BTreeMap::new(),
                })
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("same research case")
        ));
    }
}
