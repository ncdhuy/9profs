use std::{collections::BTreeSet, sync::Arc};

use nineprofs_api_types::EventEnvelope;
use nineprofs_common::now_ms;
use nineprofs_realtime::BroadcastEventBus;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    CapturePdfEvidence, CapturePdfExtraction, CaptureSourceSnapshot, CitationOccurrence,
    CitationOccurrenceId, CitationOccurrenceOrigin, CitationTarget, CitationTargetBinding,
    CitationTargetBindingId, CitationTargetId, CitationTargetResolution, ClaimCitationLink,
    ClaimEvidenceLink, ContentHash, CreateCitationOccurrence, CreateCitationTarget,
    CreateCitationTargetBinding, CreateClaimCitationLink, CreateClaimEvidenceLink,
    CreateResearchCase, CreateResearchClaim, CreateResearchEvidence, CreateResearchSource,
    EvidenceLocator, HashAlgorithm, MAX_CASE_TITLE_BYTES, MAX_CITATION_MARKER_BYTES,
    MAX_CITATION_REFERENCE_KEY_BYTES, MAX_CITATION_TARGETS_PER_OCCURRENCE, MAX_CITED_LOCATOR_BYTES,
    MAX_CLAIM_TEXT_BYTES, MAX_EVIDENCE_EXCERPT_BYTES, MAX_MANUSCRIPT_CITATION_OCCURRENCES,
    MAX_NORMALIZED_TEXT_BYTES, MAX_PDF_EXTRACTION_BYTES, MAX_PDF_PAGE_TEXT_BYTES, MAX_PDF_PAGES,
    MAX_PROVENANCE_TEXT_BYTES, MAX_RATIONALE_BYTES, MAX_SNAPSHOT_CONTENT_BYTES,
    MAX_SOURCE_LABEL_BYTES, ManuscriptCitationSyncOccurrence, ManuscriptCitationSyncOccurrenceId,
    ManuscriptCitationSyncRun, ManuscriptCitationSyncStatus, ManuscriptCitationSyncTarget,
    ManuscriptCitationSyncWrite, PdfExtractionStatus, ResearchCase, ResearchCaseId, ResearchClaim,
    ResearchClaimId, ResearchError, ResearchEvidence, ResearchEvidenceId, ResearchPdfExtraction,
    ResearchPdfExtractionId, ResearchPdfPage, ResearchPdfPageBatch, ResearchRepository,
    ResearchSource, ResearchSourceId, ResearchSourceSnapshot, ResearchSourceSnapshotId,
    SafeMetadata, SourceKind, SourceOrigin, SyncManuscriptCitations, VerifiedArtifact,
    bounded_text, validate_metadata,
};

#[derive(Clone)]
pub struct ResearchService {
    repository: crate::SqliteResearchRepository,
    events: Arc<BroadcastEventBus>,
    artifact_store: Option<Arc<crate::ResearchArtifactStore>>,
}

impl ResearchService {
    pub fn new(
        repository: crate::SqliteResearchRepository,
        events: Arc<BroadcastEventBus>,
    ) -> Self {
        Self {
            repository,
            events,
            artifact_store: None,
        }
    }

    pub fn with_artifact_store(mut self, store: Arc<crate::ResearchArtifactStore>) -> Self {
        self.artifact_store = Some(store);
        self
    }

    pub fn artifact_store(&self) -> Option<Arc<crate::ResearchArtifactStore>> {
        self.artifact_store.clone()
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

    /// Trusted ingestion seam. `artifact` can only be produced by the artifact
    /// store after it has streamed and hashed the original bytes.
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

    pub async fn capture_pdf_extraction(
        &self,
        input: CapturePdfExtraction,
    ) -> Result<ResearchPdfExtraction, ResearchError> {
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
        if !matches!(source.kind, crate::SourceKind::ReferencePdf) {
            return Err(ResearchError::Invalid(
                "PDF extraction requires a ReferencePdf source".to_owned(),
            ));
        }
        let artifact_id = match &snapshot.origin {
            SourceOrigin::UploadedArtifact { artifact_id, .. } => artifact_id.clone(),
            _ => {
                return Err(ResearchError::Invalid(
                    "PDF extraction requires an uploaded artifact snapshot".to_owned(),
                ));
            }
        };
        let store = self.artifact_store.as_ref().ok_or_else(|| {
            ResearchError::Invalid("PDF artifact store is unavailable".to_owned())
        })?;
        let artifact = store
            .get(&artifact_id)
            .await?
            .ok_or_else(|| not_found("research artifact", &artifact_id))?;
        if artifact.content_hash != snapshot.content_hash {
            return Err(ResearchError::Invalid(
                "PDF artifact hash does not match source snapshot".to_owned(),
            ));
        }
        bounded_text("PDF extractor", &input.extractor, MAX_PROVENANCE_TEXT_BYTES)?;
        let extractor_version = input
            .extractor_version
            .unwrap_or_else(|| "unspecified".to_owned());
        bounded_text(
            "PDF extractor version",
            &extractor_version,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        if input.page_count > MAX_PDF_PAGES {
            return Err(ResearchError::Invalid(format!(
                "PDF page count exceeds {MAX_PDF_PAGES}"
            )));
        }
        match input.status {
            PdfExtractionStatus::Ready | PdfExtractionStatus::NoExtractableText => {
                if input.page_count == 0 || input.pages.len() != input.page_count as usize {
                    return Err(ResearchError::Invalid(
                        "PDF extraction pages must cover page count exactly".to_owned(),
                    ));
                }
            }
            PdfExtractionStatus::Failed | PdfExtractionStatus::PasswordRequired => {
                if !input.pages.is_empty() {
                    return Err(ResearchError::Invalid(
                        "failed PDF extraction must not contain page text".to_owned(),
                    ));
                }
            }
        }
        let mut pages = Vec::with_capacity(input.pages.len());
        let mut total_bytes = 0usize;
        for (index, page) in input.pages.into_iter().enumerate() {
            let expected_page = index as u32 + 1;
            if page.page != expected_page {
                return Err(ResearchError::Invalid(
                    "PDF extraction pages must use 1-based contiguous numbering".to_owned(),
                ));
            }
            bounded_text("PDF page text", &page.text, MAX_PDF_PAGE_TEXT_BYTES).or_else(
                |error| {
                    if page.text.is_empty() {
                        Ok(())
                    } else {
                        Err(error)
                    }
                },
            )?;
            total_bytes = total_bytes
                .checked_add(page.text.len())
                .ok_or_else(|| ResearchError::Invalid("PDF extraction size overflow".to_owned()))?;
            if total_bytes > MAX_PDF_EXTRACTION_BYTES {
                return Err(ResearchError::Invalid(format!(
                    "PDF extraction exceeds {MAX_PDF_EXTRACTION_BYTES} bytes"
                )));
            }
            pages.push(ResearchPdfPage {
                extraction_id: ResearchPdfExtractionId::new(),
                page: page.page,
                text_hash: sha256_hash(page.text.as_bytes()),
                text: page.text,
            });
        }
        let has_extractable_text = pages.iter().any(|page| !page.text.trim().is_empty());
        match input.status {
            PdfExtractionStatus::NoExtractableText if has_extractable_text => {
                return Err(ResearchError::Invalid(
                    "no_extractable_text extraction contains text".to_owned(),
                ));
            }
            PdfExtractionStatus::Ready if !has_extractable_text => {
                return Err(ResearchError::Invalid(
                    "ready PDF extraction contains no extractable text".to_owned(),
                ));
            }
            _ => {}
        }
        let extraction_hash = pdf_extraction_hash(&pages);
        if let Some(existing) = self
            .repository
            .find_pdf_extraction(
                &input.source_snapshot_id,
                &input.extractor,
                &extractor_version,
                &extraction_hash,
            )
            .await?
        {
            return Ok(existing);
        }
        let extraction_id = ResearchPdfExtractionId::new();
        for page in &mut pages {
            page.extraction_id = extraction_id.clone();
        }
        let extraction = ResearchPdfExtraction {
            id: extraction_id,
            source_snapshot_id: input.source_snapshot_id,
            artifact_id,
            extractor: input.extractor,
            extractor_version,
            page_count: input.page_count,
            extraction_hash,
            extracted_at_ms: now_ms(),
            status: input.status,
        };
        if !self
            .repository
            .insert_pdf_extraction_with_pages(&extraction, &pages)
            .await?
        {
            return self
                .repository
                .find_pdf_extraction(
                    &extraction.source_snapshot_id,
                    &extraction.extractor,
                    &extraction.extractor_version,
                    &extraction.extraction_hash,
                )
                .await?
                .ok_or_else(|| {
                    ResearchError::Invalid(
                        "PDF extraction duplicate was detected but existing row was unavailable"
                            .to_owned(),
                    )
                });
        }
        self.publish(
            if matches!(
                extraction.status,
                PdfExtractionStatus::Failed | PdfExtractionStatus::PasswordRequired
            ) {
                "research.pdfExtractionFailed"
            } else {
                "research.pdfExtractionReady"
            },
            json!({
                "extraction_id": extraction.id,
                "snapshot_id": extraction.source_snapshot_id,
                "page_count": extraction.page_count,
                "status": extraction.status,
                "extraction_hash": extraction.extraction_hash.value,
            }),
        );
        Ok(extraction)
    }

    async fn get_pdf_extraction_value(
        &self,
        extraction_id: &ResearchPdfExtractionId,
    ) -> Result<ResearchPdfExtraction, ResearchError> {
        self.repository
            .get_pdf_extraction(extraction_id)
            .await?
            .ok_or_else(|| not_found("PDF extraction", extraction_id.as_str()))
    }

    pub async fn get_pdf_extraction_by_id(
        &self,
        extraction_id: &str,
    ) -> Result<ResearchPdfExtraction, ResearchError> {
        let extraction_id = ResearchPdfExtractionId::parse(extraction_id.to_owned())?;
        self.get_pdf_extraction_value(&extraction_id).await
    }

    pub async fn list_pdf_extractions(
        &self,
        snapshot_id: &str,
    ) -> Result<Vec<ResearchPdfExtraction>, ResearchError> {
        let snapshot_id = ResearchSourceSnapshotId::parse(snapshot_id.to_owned())?;
        self.repository.list_pdf_extractions(&snapshot_id).await
    }

    /// Compatibility selector for the legacy snapshot-level read route.
    /// Deterministically returns the extraction with the greatest
    /// `(extracted_at_ms, id)` tuple. New workflows must use an exact ID.
    pub async fn latest_pdf_extraction(
        &self,
        snapshot_id: &str,
    ) -> Result<ResearchPdfExtraction, ResearchError> {
        let snapshot_id = ResearchSourceSnapshotId::parse(snapshot_id.to_owned())?;
        self.repository
            .latest_pdf_extraction(&snapshot_id)
            .await?
            .ok_or_else(|| not_found("PDF extraction", snapshot_id.as_str()))
    }

    pub async fn get_pdf_extraction_for_snapshot(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        expected_snapshot_id: &ResearchSourceSnapshotId,
    ) -> Result<ResearchPdfExtraction, ResearchError> {
        let extraction = self.get_pdf_extraction_value(extraction_id).await?;
        if extraction.source_snapshot_id != *expected_snapshot_id {
            return Err(ResearchError::Invalid(
                "PDF extraction does not belong to source snapshot".to_owned(),
            ));
        }
        Ok(extraction)
    }

    pub async fn require_ready_pdf_extraction(
        &self,
        extraction_id: &str,
    ) -> Result<ResearchPdfExtraction, ResearchError> {
        let extraction = self.get_pdf_extraction_by_id(extraction_id).await?;
        if !matches!(extraction.status, PdfExtractionStatus::Ready) {
            return Err(ResearchError::Invalid(format!(
                "PDF extraction {} is not ready: {:?}",
                extraction.id.as_str(),
                extraction.status
            )));
        }
        Ok(extraction)
    }

    pub async fn list_pdf_pages(
        &self,
        extraction_id: &str,
        start_page: u32,
        limit: u32,
    ) -> Result<ResearchPdfPageBatch, ResearchError> {
        if start_page == 0 {
            return Err(ResearchError::Invalid(
                "PDF start page must be positive".to_owned(),
            ));
        }
        let extraction_id = ResearchPdfExtractionId::parse(extraction_id.to_owned())?;
        self.get_pdf_extraction_value(&extraction_id).await?;
        self.list_pdf_pages_value(&extraction_id, start_page, limit)
            .await
    }

    async fn list_pdf_pages_value(
        &self,
        extraction_id: &ResearchPdfExtractionId,
        start_page: u32,
        limit: u32,
    ) -> Result<ResearchPdfPageBatch, ResearchError> {
        let limit = limit.clamp(1, 50);
        let mut pages = self
            .repository
            .list_pdf_pages(extraction_id, start_page, limit + 1)
            .await?;
        let has_more = pages.len() > limit as usize;
        if has_more {
            pages.truncate(limit as usize);
        }
        let next_start_page = if has_more {
            let last_page = pages
                .last()
                .ok_or_else(|| ResearchError::Invalid("PDF page batch is empty".to_owned()))?
                .page;
            Some(last_page.checked_add(1).ok_or_else(|| {
                ResearchError::Invalid("PDF next start page exceeds the supported range".to_owned())
            })?)
        } else {
            None
        };
        Ok(ResearchPdfPageBatch {
            pages,
            start_page,
            limit,
            has_more,
            next_start_page,
        })
    }

    pub async fn list_all_pdf_pages_for_indexing(
        &self,
        extraction_id: &str,
    ) -> Result<Vec<ResearchPdfPage>, ResearchError> {
        let extraction = self.require_ready_pdf_extraction(extraction_id).await?;
        let extraction_id = ResearchPdfExtractionId::parse(extraction_id.to_owned())?;
        let mut pages = Vec::with_capacity(extraction.page_count as usize);
        let mut start_page = 1;
        loop {
            let batch = self
                .list_pdf_pages_value(&extraction_id, start_page, 50)
                .await?;
            pages.extend(batch.pages);
            if !batch.has_more {
                break;
            }
            start_page = batch.next_start_page.ok_or_else(|| {
                ResearchError::Invalid("PDF page batch is missing its continuation".to_owned())
            })?;
        }
        if pages.len() != extraction.page_count as usize {
            return Err(ResearchError::Invalid(
                "PDF extraction page enumeration is incomplete".to_owned(),
            ));
        }
        Ok(pages)
    }

    pub async fn get_pdf_page(
        &self,
        extraction_id: &str,
        page: u32,
    ) -> Result<ResearchPdfPage, ResearchError> {
        let extraction_id = ResearchPdfExtractionId::parse(extraction_id.to_owned())?;
        if page == 0 {
            return Err(ResearchError::Invalid(
                "PDF page must be positive".to_owned(),
            ));
        }
        self.repository
            .get_pdf_page(&extraction_id, page)
            .await?
            .ok_or_else(|| not_found("PDF page", &format!("{}:{page}", extraction_id.as_str())))
    }

    pub async fn capture_pdf_evidence(
        &self,
        input: CapturePdfEvidence,
    ) -> Result<ResearchEvidence, ResearchError> {
        let _extraction = self
            .get_pdf_extraction_for_snapshot(&input.extraction_id, &input.source_snapshot_id)
            .await?;
        let page = self
            .repository
            .get_pdf_page(&input.extraction_id, input.page)
            .await?
            .ok_or_else(|| {
                not_found(
                    "PDF page",
                    &format!("{}:{}", input.extraction_id, input.page),
                )
            })?;
        let excerpt = unicode_slice(&page.text, input.start, input.end)?;
        self.create_evidence_internal(
            CreateResearchEvidence {
                research_case_id: input.research_case_id,
                source_snapshot_id: input.source_snapshot_id,
                verbatim_excerpt: excerpt,
                normalized_text: None,
                locator: crate::EvidenceLocator::PdfTextRange {
                    page: input.page,
                    start: input.start,
                    end: input.end,
                },
                capture_method: crate::CaptureMethod::UploadedArtifact,
            },
            Some(input.extraction_id),
        )
        .await
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
        self.create_evidence_internal(input, None).await
    }

    async fn create_evidence_internal(
        &self,
        input: CreateResearchEvidence,
        pdf_extraction_id: Option<ResearchPdfExtractionId>,
    ) -> Result<ResearchEvidence, ResearchError> {
        if matches!(input.locator, crate::EvidenceLocator::PdfTextRange { .. })
            && pdf_extraction_id.is_none()
        {
            return Err(ResearchError::Invalid(
                "PDF text evidence must be captured from a stored page range".to_owned(),
            ));
        }
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
            pdf_extraction_id,
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

    pub async fn list_citation_occurrences(
        &self,
        research_case_id: Option<&str>,
    ) -> Result<Vec<CitationOccurrence>, ResearchError> {
        let case_id = research_case_id
            .map(|id| ResearchCaseId::parse(id.to_owned()))
            .transpose()?;
        self.repository
            .list_citation_occurrences(case_id.as_ref())
            .await
    }

    pub async fn get_citation_occurrence(
        &self,
        id: &str,
    ) -> Result<CitationOccurrence, ResearchError> {
        let id = CitationOccurrenceId::parse(id.to_owned())?;
        self.repository
            .get_citation_occurrence(&id)
            .await?
            .ok_or_else(|| not_found("citation occurrence", id.as_str()))
    }

    pub async fn create_citation_occurrence(
        &self,
        input: CreateCitationOccurrence,
    ) -> Result<CitationOccurrence, ResearchError> {
        self.ensure_case(&input.research_case_id).await?;
        bounded_text(
            "citation marker",
            &input.rendered_text,
            MAX_CITATION_MARKER_BYTES,
        )?;
        input.origin.validate()?;
        if let CitationOccurrenceOrigin::ManuscriptSnapshot {
            source_snapshot_id, ..
        } = &input.origin
        {
            let snapshot = self
                .repository
                .get_snapshot(source_snapshot_id)
                .await?
                .ok_or_else(|| not_found("source snapshot", source_snapshot_id.as_str()))?;
            let source = self
                .repository
                .get_source(&snapshot.source_id)
                .await?
                .ok_or_else(|| not_found("source", snapshot.source_id.as_str()))?;
            if source.research_case_id != input.research_case_id {
                return Err(ResearchError::Invalid(
                    "citation occurrence snapshot must belong to same research case".to_owned(),
                ));
            }
            if !matches!(source.kind, crate::SourceKind::Manuscript) {
                return Err(ResearchError::Invalid(
                    "manuscript citation snapshot requires a Manuscript source".to_owned(),
                ));
            }
        }
        let value = CitationOccurrence {
            id: CitationOccurrenceId::new(),
            research_case_id: input.research_case_id,
            origin: input.origin,
            rendered_text: input.rendered_text,
            created_at_ms: now_ms(),
        };
        self.repository.insert_citation_occurrence(&value).await?;
        self.publish(
            "research.citationOccurrenceCreated",
            json!({
                "citation_occurrence_id": value.id,
                "research_case_id": value.research_case_id,
            }),
        );
        Ok(value)
    }

    pub async fn sync_manuscript_citations(
        &self,
        input: SyncManuscriptCitations,
    ) -> Result<ManuscriptCitationSyncRun, ResearchError> {
        self.ensure_case(&input.research_case_id).await?;
        let source = self
            .repository
            .get_source(&input.manuscript_source_id)
            .await?
            .ok_or_else(|| not_found("source", input.manuscript_source_id.as_str()))?;
        if source.research_case_id != input.research_case_id {
            return Err(ResearchError::Invalid(
                "manuscript source must belong to same research case".to_owned(),
            ));
        }
        if !matches!(&source.kind, SourceKind::Manuscript) {
            return Err(ResearchError::Invalid(
                "manuscript citation sync requires a Manuscript source".to_owned(),
            ));
        }
        if input.document_version < 0 {
            return Err(ResearchError::Invalid(
                "document version must not be negative".to_owned(),
            ));
        }
        CitationOccurrenceOrigin::Manuscript {
            document_id: input.document_id.clone(),
            document_version: input.document_version.to_string(),
            locator: None,
        }
        .validate()?;
        if input.citations.len() > MAX_MANUSCRIPT_CITATION_OCCURRENCES {
            return Err(ResearchError::Invalid(format!(
                "manuscript citation inventory cannot contain more than {MAX_MANUSCRIPT_CITATION_OCCURRENCES} occurrences"
            )));
        }

        let inventory_hash = sha256_hash(&serde_json::to_vec(&input.citations)?);
        let timestamp = now_ms();
        let run_id = crate::ManuscriptCitationSyncRunId::new();
        let mut citation_occurrences = Vec::with_capacity(input.citations.len());
        let mut citation_targets = Vec::new();
        let mut sync_occurrences = Vec::with_capacity(input.citations.len());
        let mut sync_targets = Vec::new();

        for (ordinal, citation) in input.citations.iter().enumerate() {
            bounded_text(
                "citation marker",
                &citation.rendered_text,
                MAX_CITATION_MARKER_BYTES,
            )?;
            bounded_text(
                "document block id",
                &citation.block_id,
                MAX_PROVENANCE_TEXT_BYTES,
            )?;
            if citation.start >= citation.end {
                return Err(ResearchError::Invalid(
                    "manuscript citation locator must have start < end".to_owned(),
                ));
            }
            let locator = EvidenceLocator::Manuscript {
                block_id: citation.block_id.clone(),
                start: Some(citation.start),
                end: Some(citation.end),
            };
            locator.validate()?;
            let occurrence_id = CitationOccurrenceId::new();
            let occurrence = CitationOccurrence {
                id: occurrence_id.clone(),
                research_case_id: input.research_case_id.clone(),
                origin: CitationOccurrenceOrigin::Manuscript {
                    document_id: input.document_id.clone(),
                    document_version: input.document_version.to_string(),
                    locator: Some(locator),
                },
                rendered_text: citation.rendered_text.clone(),
                created_at_ms: timestamp,
            };
            occurrence.origin.validate()?;
            citation_occurrences.push(occurrence);

            let sync_occurrence_id = crate::ManuscriptCitationSyncOccurrenceId::new();
            sync_occurrences.push(ManuscriptCitationSyncOccurrence {
                id: sync_occurrence_id.clone(),
                sync_run_id: run_id.clone(),
                ordinal: ordinal as u32,
                citation_occurrence_id: occurrence_id.clone(),
                document_block_id: citation.block_id.clone(),
                start: citation.start,
                end: citation.end,
                format: citation.format.clone(),
            });

            let mut target_ordinals = BTreeSet::new();
            for target in &citation.targets {
                if !target_ordinals.insert(target.ordinal) {
                    return Err(ResearchError::Invalid(
                        "manuscript citation target ordinals must be unique".to_owned(),
                    ));
                }
                if target_ordinals.len() > MAX_CITATION_TARGETS_PER_OCCURRENCE {
                    return Err(ResearchError::Invalid(format!(
                        "citation occurrence cannot contain more than {MAX_CITATION_TARGETS_PER_OCCURRENCE} targets"
                    )));
                }
                bounded_text(
                    "citation reference key",
                    &target.reference_key,
                    MAX_CITATION_REFERENCE_KEY_BYTES,
                )?;
                if let Some(cited_locator) = &target.cited_locator {
                    bounded_text("cited locator", cited_locator, MAX_CITED_LOCATOR_BYTES)?;
                }
                let target_id = CitationTargetId::new();
                citation_targets.push(CitationTarget {
                    id: target_id.clone(),
                    citation_occurrence_id: occurrence_id.clone(),
                    ordinal: target.ordinal,
                    reference_key: target.reference_key.clone(),
                    cited_locator: target.cited_locator.clone(),
                });
                sync_targets.push(ManuscriptCitationSyncTarget {
                    id: crate::ManuscriptCitationSyncTargetId::new(),
                    sync_occurrence_id: sync_occurrence_id.clone(),
                    document_target_ordinal: target.ordinal,
                    citation_target_id: target_id,
                });
            }
        }

        let run = ManuscriptCitationSyncRun {
            id: run_id,
            research_case_id: input.research_case_id,
            manuscript_source_id: input.manuscript_source_id,
            document_id: input.document_id,
            document_version: input.document_version,
            inventory_hash,
            status: ManuscriptCitationSyncStatus::Completed,
            occurrence_count: citation_occurrences.len() as u32,
            created_at_ms: timestamp,
            completed_at_ms: Some(timestamp),
            failure_code: None,
        };
        let result = self
            .repository
            .persist_manuscript_citation_sync(&ManuscriptCitationSyncWrite {
                run,
                citation_occurrences,
                citation_targets,
                sync_occurrences,
                sync_targets,
            })
            .await?;
        self.publish(
            "research.manuscriptCitationSyncCompleted",
            json!({
                "sync_run_id": result.id,
                "research_case_id": result.research_case_id,
                "manuscript_source_id": result.manuscript_source_id,
                "document_id": result.document_id,
                "document_version": result.document_version,
                "occurrence_count": result.occurrence_count,
                "status": result.status,
            }),
        );
        Ok(result)
    }

    pub async fn get_manuscript_citation_sync(
        &self,
        id: &str,
    ) -> Result<ManuscriptCitationSyncRun, ResearchError> {
        let id = crate::ManuscriptCitationSyncRunId::parse(id.to_owned())?;
        self.repository
            .get_manuscript_citation_sync(&id)
            .await?
            .ok_or_else(|| not_found("manuscript citation sync run", id.as_str()))
    }

    pub async fn latest_manuscript_citation_sync(
        &self,
        research_case_id: &str,
        manuscript_source_id: &str,
    ) -> Result<ManuscriptCitationSyncRun, ResearchError> {
        let case_id = ResearchCaseId::parse(research_case_id.to_owned())?;
        let source_id = ResearchSourceId::parse(manuscript_source_id.to_owned())?;
        self.ensure_case(&case_id).await?;
        let source = self
            .repository
            .get_source(&source_id)
            .await?
            .ok_or_else(|| not_found("source", source_id.as_str()))?;
        if source.research_case_id != case_id {
            return Err(ResearchError::Invalid(
                "manuscript source must belong to same research case".to_owned(),
            ));
        }
        if !matches!(&source.kind, SourceKind::Manuscript) {
            return Err(ResearchError::Invalid(
                "manuscript citation sync requires a Manuscript source".to_owned(),
            ));
        }
        self.repository
            .latest_manuscript_citation_sync(&case_id, &source_id)
            .await?
            .ok_or_else(|| not_found("manuscript citation sync run", source_id.as_str()))
    }

    pub async fn list_manuscript_citation_sync_occurrences(
        &self,
        sync_run_id: &str,
    ) -> Result<Vec<ManuscriptCitationSyncOccurrence>, ResearchError> {
        let run_id = crate::ManuscriptCitationSyncRunId::parse(sync_run_id.to_owned())?;
        self.get_manuscript_citation_sync(run_id.as_str()).await?;
        self.repository
            .list_manuscript_citation_sync_occurrences(&run_id)
            .await
    }

    pub async fn get_manuscript_citation_sync_occurrence(
        &self,
        id: &str,
    ) -> Result<ManuscriptCitationSyncOccurrence, ResearchError> {
        let id = ManuscriptCitationSyncOccurrenceId::parse(id.to_owned())?;
        self.repository
            .get_manuscript_citation_sync_occurrence(&id)
            .await?
            .ok_or_else(|| not_found("manuscript citation sync occurrence", id.as_str()))
    }

    pub async fn list_manuscript_citation_sync_targets(
        &self,
        sync_occurrence_id: &str,
    ) -> Result<Vec<ManuscriptCitationSyncTarget>, ResearchError> {
        let occurrence_id =
            ManuscriptCitationSyncOccurrenceId::parse(sync_occurrence_id.to_owned())?;
        self.get_manuscript_citation_sync_occurrence(occurrence_id.as_str())
            .await?;
        self.repository
            .list_manuscript_citation_sync_targets(&occurrence_id)
            .await
    }

    pub async fn list_citation_targets(
        &self,
        citation_occurrence_id: &str,
    ) -> Result<Vec<CitationTarget>, ResearchError> {
        let occurrence_id = CitationOccurrenceId::parse(citation_occurrence_id.to_owned())?;
        self.get_citation_occurrence(occurrence_id.as_str()).await?;
        self.repository.list_citation_targets(&occurrence_id).await
    }

    pub async fn get_citation_target(&self, id: &str) -> Result<CitationTarget, ResearchError> {
        let id = CitationTargetId::parse(id.to_owned())?;
        self.repository
            .get_citation_target(&id)
            .await?
            .ok_or_else(|| not_found("citation target", id.as_str()))
    }

    pub async fn create_citation_target(
        &self,
        input: CreateCitationTarget,
    ) -> Result<CitationTarget, ResearchError> {
        self.get_citation_occurrence(input.citation_occurrence_id.as_str())
            .await?;
        let existing = self
            .repository
            .list_citation_targets(&input.citation_occurrence_id)
            .await?;
        if existing.len() >= MAX_CITATION_TARGETS_PER_OCCURRENCE {
            return Err(ResearchError::Invalid(format!(
                "citation occurrence cannot contain more than {MAX_CITATION_TARGETS_PER_OCCURRENCE} targets"
            )));
        }
        if existing
            .iter()
            .any(|target| target.ordinal == input.ordinal)
        {
            return Err(ResearchError::Invalid(
                "citation target ordinal already exists in occurrence".to_owned(),
            ));
        }
        bounded_text(
            "citation reference key",
            &input.reference_key,
            MAX_CITATION_REFERENCE_KEY_BYTES,
        )?;
        if let Some(cited_locator) = &input.cited_locator {
            bounded_text("cited locator", cited_locator, MAX_CITED_LOCATOR_BYTES)?;
        }
        let value = CitationTarget {
            id: CitationTargetId::new(),
            citation_occurrence_id: input.citation_occurrence_id,
            ordinal: input.ordinal,
            reference_key: input.reference_key,
            cited_locator: input.cited_locator,
        };
        self.repository.insert_citation_target(&value).await?;
        Ok(value)
    }

    pub async fn citation_target_resolution(
        &self,
        target_id: &str,
    ) -> Result<CitationTargetResolution, ResearchError> {
        let target = self.get_citation_target(target_id).await?;
        Ok(self
            .repository
            .latest_citation_target_binding(&target.id)
            .await?
            .map(|binding| binding.resolution())
            .unwrap_or(CitationTargetResolution::Unresolved))
    }

    pub async fn list_citation_target_bindings(
        &self,
        citation_target_id: &str,
    ) -> Result<Vec<CitationTargetBinding>, ResearchError> {
        let target_id = CitationTargetId::parse(citation_target_id.to_owned())?;
        self.get_citation_target(target_id.as_str()).await?;
        self.repository
            .list_citation_target_bindings(&target_id)
            .await
    }

    pub async fn get_citation_target_binding(
        &self,
        id: &str,
    ) -> Result<CitationTargetBinding, ResearchError> {
        let id = CitationTargetBindingId::parse(id.to_owned())?;
        self.repository
            .get_citation_target_binding(&id)
            .await?
            .ok_or_else(|| not_found("citation target binding", id.as_str()))
    }

    pub async fn latest_citation_target_binding(
        &self,
        citation_target_id: &str,
    ) -> Result<CitationTargetBinding, ResearchError> {
        let target_id = CitationTargetId::parse(citation_target_id.to_owned())?;
        self.get_citation_target(target_id.as_str()).await?;
        self.repository
            .latest_citation_target_binding(&target_id)
            .await?
            .ok_or_else(|| not_found("citation target binding", target_id.as_str()))
    }

    pub async fn create_citation_target_binding(
        &self,
        input: CreateCitationTargetBinding,
    ) -> Result<CitationTargetBinding, ResearchError> {
        self.ensure_case(&input.research_case_id).await?;
        let target = self
            .repository
            .get_citation_target(&input.citation_target_id)
            .await?
            .ok_or_else(|| not_found("citation target", input.citation_target_id.as_str()))?;
        let occurrence = self
            .repository
            .get_citation_occurrence(&target.citation_occurrence_id)
            .await?
            .ok_or_else(|| {
                not_found(
                    "citation occurrence",
                    target.citation_occurrence_id.as_str(),
                )
            })?;
        if occurrence.research_case_id != input.research_case_id {
            return Err(ResearchError::Invalid(
                "citation target must belong to same research case as binding".to_owned(),
            ));
        }

        let source = self
            .repository
            .get_source(&input.source_id)
            .await?
            .ok_or_else(|| not_found("source", input.source_id.as_str()))?;
        if source.research_case_id != input.research_case_id {
            return Err(ResearchError::Invalid(
                "citation binding source must belong to same research case".to_owned(),
            ));
        }

        if input.extraction_id.is_some() && input.source_snapshot_id.is_none() {
            return Err(ResearchError::Invalid(
                "PDF citation binding requires its source snapshot".to_owned(),
            ));
        }
        if let Some(snapshot_id) = &input.source_snapshot_id {
            let snapshot = self
                .repository
                .get_snapshot(snapshot_id)
                .await?
                .ok_or_else(|| not_found("source snapshot", snapshot_id.as_str()))?;
            if snapshot.source_id != input.source_id {
                return Err(ResearchError::Invalid(
                    "citation binding snapshot does not belong to source".to_owned(),
                ));
            }
        }
        if let Some(extraction_id) = &input.extraction_id {
            let snapshot_id = input.source_snapshot_id.as_ref().ok_or_else(|| {
                ResearchError::Invalid(
                    "PDF citation binding requires its source snapshot".to_owned(),
                )
            })?;
            let extraction = self
                .repository
                .get_pdf_extraction(extraction_id)
                .await?
                .ok_or_else(|| not_found("PDF extraction", extraction_id.as_str()))?;
            if extraction.source_snapshot_id != *snapshot_id {
                return Err(ResearchError::Invalid(
                    "citation binding extraction does not belong to source snapshot".to_owned(),
                ));
            }
            if !matches!(source.kind, crate::SourceKind::ReferencePdf) {
                return Err(ResearchError::Invalid(
                    "PDF citation binding requires a ReferencePdf source".to_owned(),
                ));
            }
            if !matches!(extraction.status, PdfExtractionStatus::Ready) {
                return Err(ResearchError::Invalid(
                    "PDF citation binding requires a ready extraction".to_owned(),
                ));
            }
        }

        let existing = self
            .repository
            .list_citation_target_bindings(&input.citation_target_id)
            .await?;
        if let Some(existing) = existing.into_iter().find(|binding| {
            binding.research_case_id == input.research_case_id
                && binding.source_id == input.source_id
                && binding.source_snapshot_id == input.source_snapshot_id
                && binding.extraction_id == input.extraction_id
                && binding.method == input.method
        }) {
            return Ok(existing);
        }

        let value = CitationTargetBinding {
            id: CitationTargetBindingId::new(),
            research_case_id: input.research_case_id,
            citation_target_id: input.citation_target_id,
            source_id: input.source_id,
            source_snapshot_id: input.source_snapshot_id,
            extraction_id: input.extraction_id,
            method: input.method,
            created_at_ms: now_ms(),
        };
        self.repository
            .insert_citation_target_binding(&value)
            .await?;
        self.publish(
            "research.citationTargetBound",
            json!({
                "binding_id": value.id,
                "citation_target_id": value.citation_target_id,
                "research_case_id": value.research_case_id,
                "source_id": value.source_id,
                "source_snapshot_id": value.source_snapshot_id,
                "extraction_id": value.extraction_id,
                "method": value.method,
            }),
        );
        Ok(value)
    }

    pub async fn list_claim_citation_links(
        &self,
        research_case_id: Option<&str>,
        claim_id: Option<&str>,
        citation_occurrence_id: Option<&str>,
    ) -> Result<Vec<ClaimCitationLink>, ResearchError> {
        let case_id = research_case_id
            .map(|id| ResearchCaseId::parse(id.to_owned()))
            .transpose()?;
        let claim_id = claim_id
            .map(|id| ResearchClaimId::parse(id.to_owned()))
            .transpose()?;
        let occurrence_id = citation_occurrence_id
            .map(|id| CitationOccurrenceId::parse(id.to_owned()))
            .transpose()?;
        self.repository
            .list_claim_citation_links(case_id.as_ref(), claim_id.as_ref(), occurrence_id.as_ref())
            .await
    }

    pub async fn get_claim_citation_link(
        &self,
        id: &str,
    ) -> Result<ClaimCitationLink, ResearchError> {
        let id = crate::ClaimCitationLinkId::parse(id.to_owned())?;
        self.repository
            .get_claim_citation_link(&id)
            .await?
            .ok_or_else(|| not_found("claim-citation link", id.as_str()))
    }

    pub async fn create_claim_citation_link(
        &self,
        input: CreateClaimCitationLink,
    ) -> Result<ClaimCitationLink, ResearchError> {
        self.ensure_case(&input.research_case_id).await?;
        let claim = self
            .repository
            .get_claim(&input.claim_id)
            .await?
            .ok_or_else(|| not_found("claim", input.claim_id.as_str()))?;
        let occurrence = self
            .repository
            .get_citation_occurrence(&input.citation_occurrence_id)
            .await?
            .ok_or_else(|| {
                not_found("citation occurrence", input.citation_occurrence_id.as_str())
            })?;
        if claim.research_case_id != input.research_case_id
            || occurrence.research_case_id != input.research_case_id
        {
            return Err(ResearchError::Invalid(
                "claim and citation occurrence must belong to same research case".to_owned(),
            ));
        }
        if let Some(existing) = self
            .repository
            .find_claim_citation_link(&input.claim_id, &input.citation_occurrence_id)
            .await?
        {
            return Ok(existing);
        }
        let value = ClaimCitationLink {
            id: crate::ClaimCitationLinkId::new(),
            research_case_id: input.research_case_id,
            claim_id: input.claim_id,
            citation_occurrence_id: input.citation_occurrence_id,
            created_at_ms: now_ms(),
        };
        self.repository.insert_claim_citation_link(&value).await?;
        self.publish(
            "research.claimCitationLinked",
            json!({
                "link_id": value.id,
                "research_case_id": value.research_case_id,
                "claim_id": value.claim_id,
                "citation_occurrence_id": value.citation_occurrence_id,
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

fn pdf_extraction_hash(pages: &[ResearchPdfPage]) -> ContentHash {
    let mut hasher = Sha256::new();
    for page in pages {
        hasher.update(page.page.to_string().as_bytes());
        hasher.update([0]);
        hasher.update(page.text.len().to_string().as_bytes());
        hasher.update([0]);
        hasher.update(page.text.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    sha256_digest(&digest)
}

fn sha256_digest(digest: &[u8]) -> ContentHash {
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

fn unicode_slice(text: &str, start: u64, end: u64) -> Result<String, ResearchError> {
    if start > end {
        return Err(ResearchError::Invalid(
            "PDF text range start must not exceed end".to_owned(),
        ));
    }
    let start = usize::try_from(start)
        .map_err(|_| ResearchError::Invalid("PDF text range is too large".to_owned()))?;
    let end = usize::try_from(end)
        .map_err(|_| ResearchError::Invalid("PDF text range is too large".to_owned()))?;
    let mut offsets = text
        .char_indices()
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    offsets.push(text.len());
    if end >= offsets.len() || start >= offsets.len() {
        return Err(ResearchError::Invalid(
            "PDF text range exceeds stored page text".to_owned(),
        ));
    }
    bounded_text(
        "PDF evidence excerpt",
        &text[offsets[start]..offsets[end]],
        MAX_EVIDENCE_EXCERPT_BYTES,
    )?;
    Ok(text[offsets[start]..offsets[end]].to_owned())
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
                kind: SourceKind::Manuscript,
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

    #[tokio::test]
    async fn streamed_pdf_artifact_snapshot_extraction_and_exact_unicode_evidence_are_anchored() {
        let database = Database::in_memory().await.unwrap();
        let root = std::env::temp_dir().join(format!("9profs-research-pdf-{}", now_ms()));
        let store = Arc::new(crate::ResearchArtifactStore::new(
            root.clone(),
            database.pool().clone(),
        ));
        let service = ResearchService::new(
            crate::SqliteResearchRepository::new(database.pool().clone()),
            Arc::new(BroadcastEventBus::new(64)),
        )
        .with_artifact_store(Arc::clone(&store));

        let mut upload = store
            .begin_upload(r"C:\Users\person\reference.pdf")
            .unwrap();
        upload.append(b"%PDF-1.7\nfixture bytes").unwrap();
        let artifact = upload.finish().await.unwrap();
        assert_eq!(artifact.artifact().media_type, "application/pdf");
        assert_eq!(artifact.artifact().size_bytes, 22);
        let stored_path = root.join(format!("{}.pdf", artifact.content_hash().value));
        assert_eq!(
            std::fs::read(stored_path).unwrap(),
            b"%PDF-1.7\nfixture bytes"
        );

        let mut duplicate = store.begin_upload("duplicate.pdf").unwrap();
        duplicate.append(b"%PDF-1.7\nfixture bytes").unwrap();
        let duplicate = duplicate.finish().await.unwrap();
        assert_eq!(duplicate.artifact_id(), artifact.artifact_id());

        let mut revised = store.begin_upload("revised.pdf").unwrap();
        revised.append(b"%PDF-1.7\nrevised bytes").unwrap();
        let revised = revised.finish().await.unwrap();
        assert_ne!(revised.artifact_id(), artifact.artifact_id());
        assert_eq!(
            std::fs::read(root.join(format!("{}.pdf", artifact.content_hash().value))).unwrap(),
            b"%PDF-1.7\nfixture bytes"
        );

        let temp_upload_count = || {
            std::fs::read_dir(&root)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().starts_with(".upload-"))
                .count()
        };
        let temp_uploads_before = temp_upload_count();
        let mut invalid_upload = store.begin_upload("invalid.pdf").unwrap();
        invalid_upload.append(b"not a PDF").unwrap();
        assert!(invalid_upload.finish().await.is_err());
        assert_eq!(temp_upload_count(), temp_uploads_before);

        let case = service
            .create_case(CreateResearchCase {
                title: "PDF evidence".to_owned(),
            })
            .await
            .unwrap();
        let source = service
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::ReferencePdf,
                label: "Reference".to_owned(),
            })
            .await
            .unwrap();
        let snapshot = service
            .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
            .await
            .unwrap();
        assert_eq!(snapshot.content_hash, *artifact.content_hash());

        let page_text = "Điều trị giảm tử vong 😀 20%.";
        let extraction = service
            .capture_pdf_extraction(CapturePdfExtraction {
                source_snapshot_id: snapshot.id.clone(),
                extractor: "pdfjs".to_owned(),
                extractor_version: Some("test".to_owned()),
                page_count: 2,
                status: PdfExtractionStatus::Ready,
                pages: vec![
                    crate::CapturePdfPage {
                        page: 1,
                        text: page_text.to_owned(),
                    },
                    crate::CapturePdfPage {
                        page: 2,
                        text: "Second page".to_owned(),
                    },
                ],
            })
            .await
            .unwrap();
        let start_byte = page_text.find("giảm tử vong").unwrap();
        let start = page_text[..start_byte].chars().count() as u64;
        let end = start + "giảm tử vong".chars().count() as u64;
        let evidence = service
            .capture_pdf_evidence(CapturePdfEvidence {
                research_case_id: case.id.clone(),
                source_snapshot_id: snapshot.id.clone(),
                extraction_id: extraction.id.clone(),
                page: 1,
                start,
                end,
            })
            .await
            .unwrap();
        assert_eq!(evidence.verbatim_excerpt, "giảm tử vong");
        assert_eq!(evidence.pdf_extraction_id, Some(extraction.id.clone()));
        assert_eq!(
            service.list_evidence(None, None).await.unwrap()[0].pdf_extraction_id,
            Some(extraction.id.clone())
        );
        assert!(matches!(
            service
                .create_evidence(CreateResearchEvidence {
                    research_case_id: case.id,
                    source_snapshot_id: snapshot.id,
                    verbatim_excerpt: "eliminated mortality".to_owned(),
                    normalized_text: None,
                    locator: EvidenceLocator::PdfTextRange { page: 1, start, end },
                    capture_method: CaptureMethod::UploadedArtifact,
                })
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("stored page range")
        ));
        std::fs::write(
            root.join(format!("{}.pdf", artifact.content_hash().value)),
            b"%PDF-1.7\ntampered bytes",
        )
        .unwrap();
        assert!(matches!(
            store.get(artifact.artifact_id()).await,
            Err(ResearchError::Artifact(message)) if message.contains("do not match metadata")
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn pdf_extraction_access_is_exact_ordered_paginated_and_ready_only() {
        let database = Database::in_memory().await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "9profs-research-pdf-access-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let store = Arc::new(crate::ResearchArtifactStore::new(
            root.clone(),
            database.pool().clone(),
        ));
        let service = ResearchService::new(
            crate::SqliteResearchRepository::new(database.pool().clone()),
            Arc::new(BroadcastEventBus::new(64)),
        )
        .with_artifact_store(Arc::clone(&store));
        let mut upload = store.begin_upload("access.pdf").unwrap();
        upload.append(b"%PDF-1.7\naccess fixture").unwrap();
        let artifact = upload.finish().await.unwrap();
        let case = service
            .create_case(CreateResearchCase {
                title: "PDF access".to_owned(),
            })
            .await
            .unwrap();
        let source = service
            .create_source(CreateResearchSource {
                research_case_id: case.id,
                kind: SourceKind::ReferencePdf,
                label: "Access fixture".to_owned(),
            })
            .await
            .unwrap();
        let snapshot = service
            .capture_verified_artifact_snapshot(source.id, &artifact, BTreeMap::new())
            .await
            .unwrap();

        let no_text = service
            .capture_pdf_extraction(CapturePdfExtraction {
                source_snapshot_id: snapshot.id.clone(),
                extractor: "pdfjs".to_owned(),
                extractor_version: Some("no-text".to_owned()),
                page_count: 1,
                status: PdfExtractionStatus::NoExtractableText,
                pages: vec![crate::CapturePdfPage {
                    page: 1,
                    text: String::new(),
                }],
            })
            .await
            .unwrap();
        assert!(matches!(
            service
                .require_ready_pdf_extraction(no_text.id.as_str())
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("not ready")
        ));

        let pages = |prefix: &str| {
            (1..=120)
                .map(|page| crate::CapturePdfPage {
                    page,
                    text: format!("{prefix} page {page}"),
                })
                .collect::<Vec<_>>()
        };
        let extraction_one = service
            .capture_pdf_extraction(CapturePdfExtraction {
                source_snapshot_id: snapshot.id.clone(),
                extractor: "pdfjs".to_owned(),
                extractor_version: Some("1".to_owned()),
                page_count: 120,
                status: PdfExtractionStatus::Ready,
                pages: pages("revision-one"),
            })
            .await
            .unwrap();
        while now_ms() <= extraction_one.extracted_at_ms {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        let extraction_two = service
            .capture_pdf_extraction(CapturePdfExtraction {
                source_snapshot_id: snapshot.id.clone(),
                extractor: "pdfjs".to_owned(),
                extractor_version: Some("2".to_owned()),
                page_count: 120,
                status: PdfExtractionStatus::Ready,
                pages: pages("revision-two"),
            })
            .await
            .unwrap();
        assert!(extraction_two.extracted_at_ms > extraction_one.extracted_at_ms);
        assert_ne!(
            extraction_one.extraction_hash,
            extraction_two.extraction_hash
        );

        let listed = service
            .list_pdf_extractions(snapshot.id.as_str())
            .await
            .unwrap();
        assert_eq!(listed.len(), 3);
        assert!(listed.windows(2).all(|pair| {
            (pair[0].extracted_at_ms, &pair[0].id) <= (pair[1].extracted_at_ms, &pair[1].id)
        }));
        assert!(listed.iter().any(|value| value.id == no_text.id));
        assert!(listed.iter().any(|value| value.id == extraction_one.id));
        assert!(listed.iter().any(|value| value.id == extraction_two.id));
        assert_eq!(
            service
                .get_pdf_extraction_by_id(extraction_one.id.as_str())
                .await
                .unwrap(),
            extraction_one
        );
        assert_eq!(
            service
                .get_pdf_extraction_by_id(extraction_two.id.as_str())
                .await
                .unwrap(),
            extraction_two
        );
        assert_eq!(
            service
                .latest_pdf_extraction(snapshot.id.as_str())
                .await
                .unwrap()
                .id,
            extraction_two.id
        );
        assert!(matches!(
            service
                .get_pdf_extraction_for_snapshot(
                    &extraction_one.id,
                    &ResearchSourceSnapshotId::new()
                )
                .await,
            Err(ResearchError::Invalid(message))
                if message.contains("does not belong to source snapshot")
        ));

        let first = service
            .list_pdf_pages(extraction_one.id.as_str(), 1, 500)
            .await
            .unwrap();
        assert_eq!(
            first.pages.iter().map(|page| page.page).collect::<Vec<_>>(),
            (1..=50).collect::<Vec<_>>()
        );
        assert_eq!(first.start_page, 1);
        assert_eq!(first.limit, 50);
        assert!(first.has_more);
        assert_eq!(first.next_start_page, Some(51));
        assert!(
            first
                .pages
                .iter()
                .all(|page| page.extraction_id == extraction_one.id)
        );

        let middle = service
            .list_pdf_pages(extraction_one.id.as_str(), 51, 50)
            .await
            .unwrap();
        assert_eq!(
            middle
                .pages
                .iter()
                .map(|page| page.page)
                .collect::<Vec<_>>(),
            (51..=100).collect::<Vec<_>>()
        );
        assert_eq!(middle.next_start_page, Some(101));
        assert!(
            middle
                .pages
                .iter()
                .all(|page| page.extraction_id == extraction_one.id)
        );

        let last = service
            .list_pdf_pages(extraction_one.id.as_str(), 101, 50)
            .await
            .unwrap();
        assert_eq!(
            last.pages.iter().map(|page| page.page).collect::<Vec<_>>(),
            (101..=120).collect::<Vec<_>>()
        );
        assert!(!last.has_more);
        assert_eq!(last.next_start_page, None);
        assert!(
            last.pages
                .iter()
                .all(|page| page.extraction_id == extraction_one.id)
        );

        let all = service
            .list_all_pdf_pages_for_indexing(extraction_two.id.as_str())
            .await
            .unwrap();
        assert_eq!(all.len(), 120);
        assert_eq!(
            all.iter().map(|page| page.page).collect::<Vec<_>>(),
            (1..=120).collect::<Vec<_>>()
        );
        assert!(all.iter().all(|page| {
            page.extraction_id == extraction_two.id && page.text.starts_with("revision-two")
        }));
        assert_eq!(all[0].text_hash, sha256_hash(all[0].text.as_bytes()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn citations_support_grouped_targets_many_to_many_links_and_unresolved_targets() {
        let (database, service) = service().await;
        let case = service
            .create_case(CreateResearchCase {
                title: "Citation review".to_owned(),
            })
            .await
            .unwrap();
        let source = service
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::Web,
                label: "Reference".to_owned(),
            })
            .await
            .unwrap();
        let occurrence = service
            .create_citation_occurrence(CreateCitationOccurrence {
                research_case_id: case.id.clone(),
                origin: CitationOccurrenceOrigin::Manuscript {
                    document_id: "document-1".to_owned(),
                    document_version: "version-1".to_owned(),
                    locator: Some(EvidenceLocator::Manuscript {
                        block_id: "paragraph-1".to_owned(),
                        start: Some(2),
                        end: Some(8),
                    }),
                },
                rendered_text: "[12,13,14]".to_owned(),
            })
            .await
            .unwrap();
        let second_occurrence = service
            .create_citation_occurrence(CreateCitationOccurrence {
                research_case_id: case.id.clone(),
                origin: CitationOccurrenceOrigin::Imported {
                    source: "fixture".to_owned(),
                },
                rendered_text: "[15]".to_owned(),
            })
            .await
            .unwrap();
        let mut targets = Vec::new();
        for (ordinal, reference_key) in ["12", "13", "14"].into_iter().enumerate() {
            targets.push(
                service
                    .create_citation_target(CreateCitationTarget {
                        citation_occurrence_id: occurrence.id.clone(),
                        ordinal: ordinal as u32,
                        reference_key: reference_key.to_owned(),
                        cited_locator: (ordinal == 1).then(|| "p. 42".to_owned()),
                    })
                    .await
                    .unwrap(),
            );
        }
        assert_eq!(
            targets
                .iter()
                .map(|target| target.reference_key.as_str())
                .collect::<Vec<_>>(),
            vec!["12", "13", "14"]
        );
        assert!(matches!(
            service
                .create_citation_target(CreateCitationTarget {
                    citation_occurrence_id: occurrence.id.clone(),
                    ordinal: 1,
                    reference_key: "duplicate".to_owned(),
                    cited_locator: None,
                })
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("ordinal already exists")
        ));
        assert_eq!(
            service
                .citation_target_resolution(targets[1].id.as_str())
                .await
                .unwrap(),
            crate::CitationTargetResolution::Unresolved
        );

        let binding = service
            .create_citation_target_binding(CreateCitationTargetBinding {
                research_case_id: case.id.clone(),
                citation_target_id: targets[0].id.clone(),
                source_id: source.id,
                source_snapshot_id: None,
                extraction_id: None,
                method: crate::CitationBindingMethod::DeterministicResolver,
            })
            .await
            .unwrap();
        assert_eq!(
            binding.resolution(),
            crate::CitationTargetResolution::SourceBound
        );
        assert!(!binding.pdf_verification_ready());
        assert_eq!(
            service
                .citation_target_resolution(targets[0].id.as_str())
                .await
                .unwrap(),
            crate::CitationTargetResolution::SourceBound
        );
        assert!(
            service
                .list_citation_target_bindings(targets[1].id.as_str())
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            service
                .create_citation_target_binding(CreateCitationTargetBinding {
                    research_case_id: case.id.clone(),
                    citation_target_id: targets[0].id.clone(),
                    source_id: binding.source_id.clone(),
                    source_snapshot_id: None,
                    extraction_id: None,
                    method: crate::CitationBindingMethod::DeterministicResolver,
                })
                .await
                .unwrap()
                .id,
            binding.id
        );

        let claim_one = service
            .create_claim(CreateResearchClaim {
                research_case_id: case.id.clone(),
                text: "Claim one".to_owned(),
                origin: ClaimOrigin::User,
            })
            .await
            .unwrap();
        let claim_two = service
            .create_claim(CreateResearchClaim {
                research_case_id: case.id.clone(),
                text: "Claim two".to_owned(),
                origin: ClaimOrigin::User,
            })
            .await
            .unwrap();
        let link_one = service
            .create_claim_citation_link(CreateClaimCitationLink {
                research_case_id: case.id.clone(),
                claim_id: claim_one.id.clone(),
                citation_occurrence_id: occurrence.id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(
            service
                .create_claim_citation_link(CreateClaimCitationLink {
                    research_case_id: case.id.clone(),
                    claim_id: claim_one.id.clone(),
                    citation_occurrence_id: occurrence.id.clone(),
                })
                .await
                .unwrap()
                .id,
            link_one.id
        );
        service
            .create_claim_citation_link(CreateClaimCitationLink {
                research_case_id: case.id.clone(),
                claim_id: claim_one.id.clone(),
                citation_occurrence_id: second_occurrence.id.clone(),
            })
            .await
            .unwrap();
        service
            .create_claim_citation_link(CreateClaimCitationLink {
                research_case_id: case.id.clone(),
                claim_id: claim_two.id.clone(),
                citation_occurrence_id: occurrence.id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(
            service
                .list_claim_citation_links(Some(case.id.as_str()), None, None)
                .await
                .unwrap()
                .len(),
            3
        );
        assert!(
            service
                .list_evidence(Some(case.id.as_str()), None)
                .await
                .unwrap()
                .is_empty()
        );

        let recreated = ResearchService::new(
            crate::SqliteResearchRepository::new(database.pool().clone()),
            Arc::new(BroadcastEventBus::new(64)),
        );
        let persisted_targets = recreated
            .list_citation_targets(occurrence.id.as_str())
            .await
            .unwrap();
        assert_eq!(
            persisted_targets
                .iter()
                .map(|target| target.ordinal)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(
            recreated
                .list_claim_citation_links(None, Some(claim_one.id.as_str()), None)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn exact_pdf_bindings_pin_history_and_reject_cross_case_or_broken_chains() {
        let database = Database::in_memory().await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "9profs-research-citation-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let store = Arc::new(crate::ResearchArtifactStore::new(
            root.clone(),
            database.pool().clone(),
        ));
        let service = ResearchService::new(
            crate::SqliteResearchRepository::new(database.pool().clone()),
            Arc::new(BroadcastEventBus::new(64)),
        )
        .with_artifact_store(Arc::clone(&store));
        let mut upload = store.begin_upload("reference.pdf").unwrap();
        upload.append(b"%PDF-1.7\ncitation fixture").unwrap();
        let artifact = upload.finish().await.unwrap();
        let case = service
            .create_case(CreateResearchCase {
                title: "PDF citation review".to_owned(),
            })
            .await
            .unwrap();
        let source = service
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::ReferencePdf,
                label: "Reference PDF".to_owned(),
            })
            .await
            .unwrap();
        let snapshot = service
            .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
            .await
            .unwrap();
        let extraction_one = service
            .capture_pdf_extraction(CapturePdfExtraction {
                source_snapshot_id: snapshot.id.clone(),
                extractor: "pdfjs".to_owned(),
                extractor_version: Some("1".to_owned()),
                page_count: 1,
                status: PdfExtractionStatus::Ready,
                pages: vec![crate::CapturePdfPage {
                    page: 1,
                    text: "First extraction".to_owned(),
                }],
            })
            .await
            .unwrap();
        let occurrence = service
            .create_citation_occurrence(CreateCitationOccurrence {
                research_case_id: case.id.clone(),
                origin: CitationOccurrenceOrigin::Imported {
                    source: "fixture".to_owned(),
                },
                rendered_text: "[12]".to_owned(),
            })
            .await
            .unwrap();
        let target = service
            .create_citation_target(CreateCitationTarget {
                citation_occurrence_id: occurrence.id,
                ordinal: 0,
                reference_key: "12".to_owned(),
                cited_locator: Some("p. 42".to_owned()),
            })
            .await
            .unwrap();
        let binding_one = service
            .create_citation_target_binding(CreateCitationTargetBinding {
                research_case_id: case.id.clone(),
                citation_target_id: target.id.clone(),
                source_id: source.id.clone(),
                source_snapshot_id: Some(snapshot.id.clone()),
                extraction_id: Some(extraction_one.id.clone()),
                method: crate::CitationBindingMethod::Human,
            })
            .await
            .unwrap();
        assert!(binding_one.pdf_verification_ready());

        let extraction_two = service
            .capture_pdf_extraction(CapturePdfExtraction {
                source_snapshot_id: snapshot.id.clone(),
                extractor: "pdfjs".to_owned(),
                extractor_version: Some("2".to_owned()),
                page_count: 1,
                status: PdfExtractionStatus::Ready,
                pages: vec![crate::CapturePdfPage {
                    page: 1,
                    text: "Second extraction".to_owned(),
                }],
            })
            .await
            .unwrap();
        let binding_two = service
            .create_citation_target_binding(CreateCitationTargetBinding {
                research_case_id: case.id.clone(),
                citation_target_id: target.id.clone(),
                source_id: source.id.clone(),
                source_snapshot_id: Some(snapshot.id.clone()),
                extraction_id: Some(extraction_two.id.clone()),
                method: crate::CitationBindingMethod::Imported,
            })
            .await
            .unwrap();
        assert_ne!(binding_one.id, binding_two.id);
        assert_eq!(
            service
                .get_citation_target_binding(binding_one.id.as_str())
                .await
                .unwrap()
                .extraction_id,
            Some(extraction_one.id.clone())
        );
        assert_eq!(
            service
                .latest_citation_target_binding(target.id.as_str())
                .await
                .unwrap()
                .id,
            binding_two.id
        );

        let other_source = service
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::ReferencePdf,
                label: "Other reference PDF".to_owned(),
            })
            .await
            .unwrap();
        let other_snapshot = service
            .capture_verified_artifact_snapshot(other_source.id.clone(), &artifact, BTreeMap::new())
            .await
            .unwrap();
        let other_extraction = service
            .capture_pdf_extraction(CapturePdfExtraction {
                source_snapshot_id: other_snapshot.id.clone(),
                extractor: "pdfjs".to_owned(),
                extractor_version: Some("other".to_owned()),
                page_count: 1,
                status: PdfExtractionStatus::Ready,
                pages: vec![crate::CapturePdfPage {
                    page: 1,
                    text: "Other extraction".to_owned(),
                }],
            })
            .await
            .unwrap();
        assert!(matches!(
            service
                .create_citation_target_binding(CreateCitationTargetBinding {
                    research_case_id: case.id.clone(),
                    citation_target_id: target.id.clone(),
                    source_id: source.id.clone(),
                    source_snapshot_id: Some(snapshot.id.clone()),
                    extraction_id: Some(other_extraction.id),
                    method: crate::CitationBindingMethod::DeterministicResolver,
                })
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("does not belong to source snapshot")
        ));

        let other_case = service
            .create_case(CreateResearchCase {
                title: "Other case".to_owned(),
            })
            .await
            .unwrap();
        let other_claim = service
            .create_claim(CreateResearchClaim {
                research_case_id: other_case.id.clone(),
                text: "Other claim".to_owned(),
                origin: ClaimOrigin::User,
            })
            .await
            .unwrap();
        assert!(matches!(
            service
                .create_citation_target_binding(CreateCitationTargetBinding {
                    research_case_id: other_case.id.clone(),
                    citation_target_id: target.id.clone(),
                    source_id: source.id.clone(),
                    source_snapshot_id: Some(snapshot.id.clone()),
                    extraction_id: Some(extraction_one.id.clone()),
                    method: crate::CitationBindingMethod::Agent,
                })
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("same research case")
        ));
        assert!(matches!(
            service
                .create_claim_citation_link(CreateClaimCitationLink {
                    research_case_id: other_case.id,
                    claim_id: other_claim.id,
                    citation_occurrence_id: target.citation_occurrence_id,
                })
                .await,
            Err(ResearchError::Invalid(message)) if message.contains("same research case")
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn manuscript_citation_sync_is_idempotent_versioned_and_transactional() {
        let (_database, service) = service().await;
        let case = service
            .create_case(CreateResearchCase {
                title: "Manuscript review".to_owned(),
            })
            .await
            .unwrap();
        let source = service
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::Manuscript,
                label: "Draft".to_owned(),
            })
            .await
            .unwrap();

        let input = crate::SyncManuscriptCitations {
            research_case_id: case.id.clone(),
            manuscript_source_id: source.id.clone(),
            document_id: "doc-1".to_owned(),
            document_version: 1,
            citations: vec![crate::ManuscriptCitationSyncCitationInput {
                format: crate::ManuscriptCitationFormat::Zotero,
                rendered_text: "[12,13]".to_owned(),
                block_id: "b7".to_owned(),
                start: 13,
                end: 20,
                targets: vec![
                    crate::ManuscriptCitationSyncTargetInput {
                        ordinal: 1,
                        reference_key: "12".to_owned(),
                        cited_locator: None,
                    },
                    crate::ManuscriptCitationSyncTargetInput {
                        ordinal: 2,
                        reference_key: "13".to_owned(),
                        cited_locator: Some("table:0:cell:1:2".to_owned()),
                    },
                ],
            }],
        };
        let first = service
            .sync_manuscript_citations(input.clone())
            .await
            .unwrap();
        assert_eq!(first.status, crate::ManuscriptCitationSyncStatus::Completed);
        assert_eq!(first.occurrence_count, 1);
        assert_eq!(first.document_version, 1);

        let repeated = service
            .sync_manuscript_citations(input.clone())
            .await
            .unwrap();
        assert_eq!(repeated.id, first.id);
        assert_eq!(
            service
                .list_manuscript_citation_sync_occurrences(first.id.as_str())
                .await
                .unwrap()
                .len(),
            1
        );
        let sync_occurrence = service
            .list_manuscript_citation_sync_occurrences(first.id.as_str())
            .await
            .unwrap()
            .pop()
            .unwrap();
        let sync_targets = service
            .list_manuscript_citation_sync_targets(sync_occurrence.id.as_str())
            .await
            .unwrap();
        assert_eq!(
            sync_targets
                .iter()
                .map(|target| target.document_target_ordinal)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        let occurrence = service
            .get_citation_occurrence(sync_occurrence.citation_occurrence_id.as_str())
            .await
            .unwrap();
        assert_eq!(occurrence.rendered_text, "[12,13]");
        assert!(matches!(
            occurrence.origin,
            CitationOccurrenceOrigin::Manuscript { document_version, .. }
                if document_version == "1"
        ));
        assert_eq!(
            service
                .list_citation_targets(occurrence.id.as_str())
                .await
                .unwrap()
                .into_iter()
                .map(|target| target.reference_key)
                .collect::<Vec<_>>(),
            vec!["12", "13"]
        );

        let mut changed = input.clone();
        changed.citations[0].rendered_text = "[12]".to_owned();
        assert!(matches!(
            service.sync_manuscript_citations(changed).await,
            Err(ResearchError::ManuscriptCitationSyncConflict { .. })
        ));

        let mut next_version = input;
        next_version.document_version = 2;
        next_version.citations.clear();
        let second = service
            .sync_manuscript_citations(next_version)
            .await
            .unwrap();
        assert_ne!(second.id, first.id);
        assert_eq!(second.occurrence_count, 0);
        assert_eq!(
            service
                .latest_manuscript_citation_sync(case.id.as_str(), source.id.as_str())
                .await
                .unwrap()
                .id,
            second.id
        );
        assert_eq!(
            service
                .list_citation_occurrences(Some(case.id.as_str()))
                .await
                .unwrap()
                .len(),
            1
        );

        let invalid = crate::SyncManuscriptCitations {
            research_case_id: case.id.clone(),
            manuscript_source_id: source.id.clone(),
            document_id: "doc-invalid".to_owned(),
            document_version: 1,
            citations: vec![crate::ManuscriptCitationSyncCitationInput {
                format: crate::ManuscriptCitationFormat::WordNative,
                rendered_text: "[1]".to_owned(),
                block_id: "b8".to_owned(),
                start: 4,
                end: 4,
                targets: Vec::new(),
            }],
        };
        assert!(matches!(
            service.sync_manuscript_citations(invalid).await,
            Err(ResearchError::Invalid(message)) if message.contains("start < end")
        ));
        assert!(
            service
                .latest_manuscript_citation_sync(case.id.as_str(), source.id.as_str())
                .await
                .unwrap()
                .id
                == second.id
        );
    }
}
