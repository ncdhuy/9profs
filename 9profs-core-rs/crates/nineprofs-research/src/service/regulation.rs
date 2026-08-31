use super::{ResearchService, not_found, sha256_hash};
use crate::{
    CreateRegulationRequirement, EvidenceLocator, PromoteRegulationRequirementCandidate,
    RegulationRequirement, RegulationRequirementId, RegulationReviewStatus, ResearchContext,
    ResearchError, ResearchSource, ResearchSourceId, ResearchSourceSnapshotId, SourceKind,
};
use nineprofs_common::now_ms;

impl ResearchService {
    pub async fn create_regulation_requirement(
        &self,
        input: CreateRegulationRequirement,
    ) -> Result<RegulationRequirement, ResearchError> {
        let source = self
            .repository
            .get_source(&input.source_id)
            .await?
            .ok_or_else(|| not_found("source", input.source_id.as_str()))?;
        validate_regulation_source(&source)?;

        let snapshot = self
            .repository
            .get_snapshot(&input.source_snapshot_id)
            .await?
            .ok_or_else(|| not_found("source snapshot", input.source_snapshot_id.as_str()))?;
        if snapshot.source_id != source.id {
            return Err(ResearchError::Invalid(
                "regulation requirement snapshot must belong to source".to_owned(),
            ));
        }

        if let Some(extraction_id) = &input.pdf_extraction_id {
            let extraction = self
                .repository
                .get_pdf_extraction(extraction_id)
                .await?
                .ok_or_else(|| not_found("PDF extraction", extraction_id.as_str()))?;
            if extraction.source_snapshot_id != snapshot.id {
                return Err(ResearchError::Invalid(
                    "regulation requirement PDF extraction must belong to source snapshot"
                        .to_owned(),
                ));
            }
        }

        let source_excerpt_hash = sha256_hash(input.source_excerpt.as_bytes());
        let timestamp = now_ms();
        let value = RegulationRequirement {
            id: RegulationRequirementId::new(),
            source_id: source.id,
            source_snapshot_id: snapshot.id,
            pdf_extraction_id: input.pdf_extraction_id,
            text: input.text,
            source_excerpt: input.source_excerpt,
            source_excerpt_hash,
            source_locator: input.source_locator,
            authority_locator: input.authority_locator,
            applicability: input.applicability,
            effective_from: input.effective_from,
            effective_until: input.effective_until,
            extraction_method: input.extraction_method,
            extraction_contract_version: input.extraction_contract_version,
            review_status: RegulationReviewStatus::NeedsReview,
            active: false,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
        };
        value.validate()?;
        self.repository
            .insert_regulation_requirement(&value)
            .await?;
        Ok(value)
    }

    pub async fn promote_regulation_requirement_candidate(
        &self,
        input: PromoteRegulationRequirementCandidate,
    ) -> Result<RegulationRequirement, ResearchError> {
        let candidate = self
            .repository
            .get_regulation_requirement_candidate(&input.candidate_id)
            .await?
            .ok_or_else(|| {
                not_found(
                    "regulation requirement candidate",
                    input.candidate_id.as_str(),
                )
            })?;
        let source = self
            .repository
            .get_source(&candidate.source_id)
            .await?
            .ok_or_else(|| not_found("source", candidate.source_id.as_str()))?;
        validate_regulation_source(&source)?;

        let snapshot = self
            .repository
            .get_snapshot(&candidate.source_snapshot_id)
            .await?
            .ok_or_else(|| not_found("source snapshot", candidate.source_snapshot_id.as_str()))?;
        if snapshot.source_id != candidate.source_id {
            return Err(ResearchError::Invalid(
                "regulation requirement candidate snapshot must belong to source".to_owned(),
            ));
        }

        let extraction = self
            .repository
            .get_pdf_extraction(&candidate.pdf_extraction_id)
            .await?
            .ok_or_else(|| not_found("PDF extraction", candidate.pdf_extraction_id.as_str()))?;
        if extraction.source_snapshot_id != candidate.source_snapshot_id {
            return Err(ResearchError::Invalid(
                "regulation requirement candidate PDF extraction must belong to source snapshot"
                    .to_owned(),
            ));
        }

        if input.text.trim().is_empty() {
            return Err(ResearchError::Invalid(
                "promoted regulation requirement text must not be empty".to_owned(),
            ));
        }
        input.applicability.validate_context_facets()?;
        if let Some(locator) = &input.authority_locator {
            if !matches!(locator, EvidenceLocator::Regulation { .. }) {
                return Err(ResearchError::Invalid(
                    "promoted regulation requirement authority locator must be a regulation locator"
                        .to_owned(),
                ));
            }
            locator.validate()?;
        }

        let timestamp = now_ms();
        let value = RegulationRequirement {
            id: RegulationRequirementId::new(),
            source_id: candidate.source_id,
            source_snapshot_id: candidate.source_snapshot_id,
            pdf_extraction_id: Some(candidate.pdf_extraction_id),
            source_excerpt_hash: sha256_hash(input.source_excerpt.as_bytes()),
            text: input.text,
            source_excerpt: input.source_excerpt,
            source_locator: input.source_locator,
            authority_locator: input.authority_locator,
            applicability: input.applicability,
            effective_from: input.effective_from,
            effective_until: input.effective_until,
            extraction_method: candidate.extraction.method,
            extraction_contract_version: Some(candidate.extraction.contract_version),
            review_status: RegulationReviewStatus::Approved,
            active: input.active,
            created_at_ms: timestamp,
            updated_at_ms: timestamp,
        };
        value.validate()?;
        self.repository
            .insert_regulation_requirement(&value)
            .await?;
        Ok(value)
    }

    pub async fn get_regulation_requirement(
        &self,
        id: &str,
    ) -> Result<RegulationRequirement, ResearchError> {
        let id = RegulationRequirementId::parse(id.to_owned())?;
        self.repository
            .get_regulation_requirement(&id)
            .await?
            .ok_or_else(|| not_found("regulation requirement", id.as_str()))
    }

    pub async fn list_regulation_requirements(
        &self,
        source_id: Option<&str>,
        source_snapshot_id: Option<&str>,
    ) -> Result<Vec<RegulationRequirement>, ResearchError> {
        let source_id = source_id
            .map(|id| ResearchSourceId::parse(id.to_owned()))
            .transpose()?;
        let source_snapshot_id = source_snapshot_id
            .map(|id| ResearchSourceSnapshotId::parse(id.to_owned()))
            .transpose()?;
        self.repository
            .list_regulation_requirements(source_id.as_ref(), source_snapshot_id.as_ref())
            .await
    }

    pub async fn update_regulation_requirement_review_status(
        &self,
        id: &str,
        status: RegulationReviewStatus,
    ) -> Result<RegulationRequirement, ResearchError> {
        let id = RegulationRequirementId::parse(id.to_owned())?;
        let current = self
            .repository
            .get_regulation_requirement(&id)
            .await?
            .ok_or_else(|| not_found("regulation requirement", id.as_str()))?;
        if current.active && !matches!(status, RegulationReviewStatus::Approved) {
            return Err(ResearchError::Invalid(
                "active regulation requirement must be approved before review status changes"
                    .to_owned(),
            ));
        }
        if !self
            .repository
            .update_regulation_requirement_review_status(&id, &status, now_ms())
            .await?
        {
            return Err(not_found("regulation requirement", id.as_str()));
        }
        self.get_regulation_requirement(id.as_str()).await
    }

    pub async fn set_regulation_requirement_active(
        &self,
        id: &str,
        active: bool,
    ) -> Result<RegulationRequirement, ResearchError> {
        let id = RegulationRequirementId::parse(id.to_owned())?;
        let current = self
            .repository
            .get_regulation_requirement(&id)
            .await?
            .ok_or_else(|| not_found("regulation requirement", id.as_str()))?;
        if active && !matches!(current.review_status, RegulationReviewStatus::Approved) {
            return Err(ResearchError::Invalid(
                "active regulation requirement must be approved".to_owned(),
            ));
        }
        if !self
            .repository
            .set_regulation_requirement_active(&id, active, now_ms())
            .await?
        {
            return Err(not_found("regulation requirement", id.as_str()));
        }
        self.get_regulation_requirement(id.as_str()).await
    }

    pub async fn resolve_effective_regulation_requirements(
        &self,
        source_id: Option<&str>,
        context: &ResearchContext,
        as_of_ms: i64,
    ) -> Result<Vec<RegulationRequirement>, ResearchError> {
        context.validate()?;
        let requirements = self.list_regulation_requirements(source_id, None).await?;
        Ok(crate::resolve_effective_regulation_requirements(
            &requirements,
            context,
            as_of_ms,
        ))
    }
}

fn validate_regulation_source(source: &ResearchSource) -> Result<(), ResearchError> {
    if !matches!(source.kind, SourceKind::Regulation) {
        return Err(ResearchError::Invalid(
            "regulation requirement source must have Regulation kind".to_owned(),
        ));
    }
    Ok(())
}
