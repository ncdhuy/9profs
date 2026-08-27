use super::ResearchService;
use super::{not_found, sha256_hash};
use crate::{
    CapturePdfEvidence, CapturePdfExtraction, ContentHash, CreateResearchEvidence, HashAlgorithm,
    MAX_EVIDENCE_EXCERPT_BYTES, MAX_PDF_EXTRACTION_BYTES, MAX_PDF_PAGE_TEXT_BYTES, MAX_PDF_PAGES,
    MAX_PROVENANCE_TEXT_BYTES, PdfExtractionStatus, ResearchError, ResearchEvidence,
    ResearchPdfExtraction, ResearchPdfExtractionId, ResearchPdfPage, ResearchPdfPageBatch,
    ResearchRepository, ResearchSourceSnapshotId, SourceOrigin, bounded_text,
};
use nineprofs_common::now_ms;
use serde_json::json;
use sha2::{Digest, Sha256};

impl ResearchService {
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
