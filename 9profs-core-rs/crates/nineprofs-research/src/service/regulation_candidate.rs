use super::{ResearchService, not_found};
use crate::{
    EvidenceLocator, ExtractRegulationRequirementCandidates, MAX_EVIDENCE_EXCERPT_BYTES,
    MAX_NORMALIZED_TEXT_BYTES, MAX_PROVENANCE_TEXT_BYTES, MAX_REGULATION_REQUIREMENT_CANDIDATES,
    PdfExtractionStatus, RegulationRequirementCandidate, RegulationRequirementCandidateExtraction,
    RegulationRequirementCandidateExtractionIdentity, RegulationRequirementCandidateId,
    RegulationRequirementCandidateOutput, RegulationRequirementExtractionInput,
    RegulationRequirementExtractionPage, ResearchError, ResearchSourceId, ResearchSourceSnapshotId,
    SourceKind, bounded_text,
};
use nineprofs_common::now_ms;

impl ResearchService {
    pub async fn extract_regulation_requirement_candidates(
        &self,
        request: ExtractRegulationRequirementCandidates,
    ) -> Result<Vec<RegulationRequirementCandidate>, ResearchError> {
        let Some(provider) = self.regulation_requirement_candidate_extractor.clone() else {
            return Err(ResearchError::RegulationRequirementCandidateExtractorNotConfigured);
        };
        if request.start_page == 0 || request.end_page < request.start_page {
            return Err(ResearchError::Invalid(
                "regulation requirement extraction page range is invalid".to_owned(),
            ));
        }
        let requested_page_count = u64::from(request.end_page)
            .saturating_sub(u64::from(request.start_page))
            .saturating_add(1);
        if requested_page_count > crate::MAX_REGULATION_REQUIREMENT_EXTRACTION_PAGES as u64 {
            return Err(ResearchError::Invalid(format!(
                "regulation requirement extraction cannot contain more than {} pages",
                crate::MAX_REGULATION_REQUIREMENT_EXTRACTION_PAGES
            )));
        }

        let source = self
            .repository
            .get_source(&request.source_id)
            .await?
            .ok_or_else(|| not_found("source", request.source_id.as_str()))?;
        if !matches!(source.kind, SourceKind::Regulation) {
            return Err(ResearchError::Invalid(
                "regulation requirement candidate source must have Regulation kind".to_owned(),
            ));
        }
        let snapshot = self
            .repository
            .get_snapshot(&request.source_snapshot_id)
            .await?
            .ok_or_else(|| not_found("source snapshot", request.source_snapshot_id.as_str()))?;
        if snapshot.source_id != source.id {
            return Err(ResearchError::Invalid(
                "regulation requirement candidate snapshot must belong to source".to_owned(),
            ));
        }
        let extraction = self
            .repository
            .get_pdf_extraction(&request.pdf_extraction_id)
            .await?
            .ok_or_else(|| not_found("PDF extraction", request.pdf_extraction_id.as_str()))?;
        if extraction.source_snapshot_id != snapshot.id {
            return Err(ResearchError::Invalid(
                "regulation requirement candidate PDF extraction must belong to source snapshot"
                    .to_owned(),
            ));
        }
        if !matches!(extraction.status, PdfExtractionStatus::Ready)
            || request.end_page > extraction.page_count
        {
            return Err(ResearchError::Invalid(
                "regulation requirement candidate requires ready PDF extraction pages".to_owned(),
            ));
        }

        let pages = self
            .repository
            .list_pdf_pages(
                &request.pdf_extraction_id,
                request.start_page,
                requested_page_count as u32,
            )
            .await?;
        let input = RegulationRequirementExtractionInput {
            source_id: request.source_id,
            source_snapshot_id: request.source_snapshot_id,
            pdf_extraction_id: request.pdf_extraction_id,
            start_page: request.start_page,
            end_page: request.end_page,
            pages: pages
                .into_iter()
                .map(|page| RegulationRequirementExtractionPage {
                    page: page.page,
                    text: page.text,
                    heading_context: None,
                })
                .collect(),
            institution: request.institution,
            document_title: request.document_title,
            known_artifact_scope: request.known_artifact_scope,
            allowed_applicability_vocabulary: request.allowed_applicability_vocabulary,
        };
        input.validate()?;
        let page_text = input.page_text();
        let identity = provider.identity();
        validate_identity(&identity)?;
        let outputs = provider
            .extract(input.clone())
            .await
            .map_err(map_extraction_error)?;
        if outputs.len() > MAX_REGULATION_REQUIREMENT_CANDIDATES {
            return Err(ResearchError::Invalid(format!(
                "regulation requirement extraction returned more than {MAX_REGULATION_REQUIREMENT_CANDIDATES} candidates"
            )));
        }

        let mut candidates = Vec::with_capacity(outputs.len());
        for output in outputs {
            validate_output(
                &output,
                &page_text,
                request.start_page,
                request.end_page,
                &input,
            )?;
            let candidate = RegulationRequirementCandidate {
                id: RegulationRequirementCandidateId::new(),
                source_id: input.source_id.clone(),
                source_snapshot_id: input.source_snapshot_id.clone(),
                pdf_extraction_id: input.pdf_extraction_id.clone(),
                source_locator: output.source_locator,
                authority_locator_suggestion: output.authority_locator,
                ocr_excerpt: output.ocr_excerpt,
                normalized_requirement: output.normalized_requirement,
                applicability_suggestion: output.applicability,
                extraction: RegulationRequirementCandidateExtraction {
                    method: "llm".to_owned(),
                    contract_version: identity.extraction_contract_version.clone(),
                    provider: identity.provider.clone(),
                    extractor_version: identity.extractor_version.clone(),
                    model_id: identity.model_id.clone(),
                },
                risk_flags: output.risk_flags,
                review_notes: output.review_notes,
                created_at_ms: now_ms(),
            };
            candidate.validate()?;
            self.repository
                .insert_regulation_requirement_candidate(&candidate)
                .await?;
            candidates.push(candidate);
        }
        Ok(candidates)
    }

    pub async fn get_regulation_requirement_candidate(
        &self,
        id: &str,
    ) -> Result<RegulationRequirementCandidate, ResearchError> {
        let id = RegulationRequirementCandidateId::parse(id.to_owned())?;
        self.repository
            .get_regulation_requirement_candidate(&id)
            .await?
            .ok_or_else(|| not_found("regulation requirement candidate", id.as_str()))
    }

    pub async fn list_regulation_requirement_candidates(
        &self,
        source_id: Option<&str>,
        source_snapshot_id: Option<&str>,
        pdf_extraction_id: Option<&str>,
    ) -> Result<Vec<RegulationRequirementCandidate>, ResearchError> {
        let source_id = source_id
            .map(|id| ResearchSourceId::parse(id.to_owned()))
            .transpose()?;
        let source_snapshot_id = source_snapshot_id
            .map(|id| ResearchSourceSnapshotId::parse(id.to_owned()))
            .transpose()?;
        let pdf_extraction_id = pdf_extraction_id
            .map(|id| crate::ResearchPdfExtractionId::parse(id.to_owned()))
            .transpose()?;
        self.repository
            .list_regulation_requirement_candidates(
                source_id.as_ref(),
                source_snapshot_id.as_ref(),
                pdf_extraction_id.as_ref(),
            )
            .await
    }
}

fn map_extraction_error(
    error: crate::RegulationRequirementCandidateExtractionProviderError,
) -> ResearchError {
    match error {
        crate::RegulationRequirementCandidateExtractionProviderError::NotConfigured => {
            ResearchError::RegulationRequirementCandidateExtractorNotConfigured
        }
        crate::RegulationRequirementCandidateExtractionProviderError::InvalidConfiguration(
            message,
        ) => ResearchError::RegulationRequirementCandidateExtractorInvalidConfiguration(message),
        error => ResearchError::RegulationRequirementCandidateExtractionFailed(error.to_string()),
    }
}

fn validate_identity(
    identity: &RegulationRequirementCandidateExtractionIdentity,
) -> Result<(), ResearchError> {
    for (field, value) in [
        ("regulation candidate provider", identity.provider.as_str()),
        (
            "regulation candidate extractor version",
            identity.extractor_version.as_str(),
        ),
        (
            "regulation candidate extraction contract version",
            identity.extraction_contract_version.as_str(),
        ),
    ] {
        bounded_text(field, value, MAX_PROVENANCE_TEXT_BYTES)?;
    }
    if let Some(model_id) = &identity.model_id {
        bounded_text(
            "regulation candidate model",
            model_id,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
    }
    Ok(())
}

fn validate_output(
    output: &RegulationRequirementCandidateOutput,
    page_text: &str,
    start_page: u32,
    end_page: u32,
    input: &RegulationRequirementExtractionInput,
) -> Result<(), ResearchError> {
    if output.ocr_excerpt.trim().is_empty() {
        return Err(ResearchError::Invalid(
            "regulation requirement candidate OCR excerpt must not be empty".to_owned(),
        ));
    }
    if !page_text.contains(&output.ocr_excerpt) {
        return Err(ResearchError::Invalid(
            "regulation requirement candidate OCR excerpt is not an exact supplied page-text substring"
                .to_owned(),
        ));
    }
    if output.ocr_excerpt.len() > MAX_EVIDENCE_EXCERPT_BYTES {
        return Err(ResearchError::Invalid(
            "regulation requirement candidate OCR excerpt exceeds limit".to_owned(),
        ));
    }
    if output.normalized_requirement.trim().is_empty()
        || output.normalized_requirement.len() > MAX_NORMALIZED_TEXT_BYTES
    {
        return Err(ResearchError::Invalid(
            "regulation requirement candidate normalized requirement is invalid".to_owned(),
        ));
    }
    output
        .applicability
        .validate_for_extraction(&input.allowed_applicability_vocabulary)?;
    validate_locator(&output.source_locator, start_page, end_page, input)?;
    if let Some(locator) = &output.authority_locator
        && !matches!(locator, EvidenceLocator::Regulation { .. })
    {
        return Err(ResearchError::Invalid(
            "regulation requirement candidate authority locator suggestion must be a regulation locator"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_locator(
    locator: &EvidenceLocator,
    start_page: u32,
    end_page: u32,
    input: &RegulationRequirementExtractionInput,
) -> Result<(), ResearchError> {
    locator.validate()?;
    match locator {
        EvidenceLocator::Pdf {
            page: locator_start_page,
            end_page: locator_end,
        } => {
            if *locator_start_page < start_page
                || *locator_start_page > end_page
                || locator_end.is_some_and(|locator_end_page| {
                    locator_end_page < *locator_start_page || locator_end_page > end_page
                })
            {
                return Err(ResearchError::Invalid(
                    "regulation requirement candidate PDF locator is outside requested page range"
                        .to_owned(),
                ));
            }
        }
        EvidenceLocator::PdfTextRange { page, start, end } => {
            if *page < start_page || *page > end_page {
                return Err(ResearchError::Invalid(
                    "regulation requirement candidate PDF text locator is outside requested page range"
                        .to_owned(),
                ));
            }
            let page_text = &input.pages[(*page - start_page) as usize].text;
            let length = page_text.chars().count() as u64;
            if *end > length || *start >= *end {
                return Err(ResearchError::Invalid(
                    "regulation requirement candidate PDF text locator is outside page text"
                        .to_owned(),
                ));
            }
        }
        _ => {
            return Err(ResearchError::Invalid(
                "regulation requirement candidate source locator must be a PDF locator".to_owned(),
            ));
        }
    }
    Ok(())
}
