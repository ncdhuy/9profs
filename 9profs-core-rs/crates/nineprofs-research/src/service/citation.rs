use super::ResearchService;
use super::not_found;
use crate::{
    CitationOccurrence, CitationOccurrenceId, CitationOccurrenceOrigin, CitationTarget,
    CitationTargetBinding, CitationTargetBindingId, CitationTargetId, CitationTargetResolution,
    ClaimCitationLink, CreateCitationOccurrence, CreateCitationTarget, CreateCitationTargetBinding,
    CreateClaimCitationLink, MAX_CITATION_MARKER_BYTES, MAX_CITATION_REFERENCE_KEY_BYTES,
    MAX_CITATION_TARGETS_PER_OCCURRENCE, MAX_CITED_LOCATOR_BYTES, PdfExtractionStatus,
    ResearchCaseId, ResearchClaimId, ResearchError, ResearchRepository, bounded_text,
};
use nineprofs_common::now_ms;
use serde_json::json;

impl ResearchService {
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
}
