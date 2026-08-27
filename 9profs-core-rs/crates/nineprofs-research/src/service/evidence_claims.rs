use super::ResearchService;
use super::{not_found, sha256_hash};
use crate::{
    ClaimEvidenceLink, CreateClaimEvidenceLink, CreateResearchClaim, CreateResearchEvidence,
    MAX_CLAIM_TEXT_BYTES, MAX_EVIDENCE_EXCERPT_BYTES, MAX_NORMALIZED_TEXT_BYTES,
    MAX_RATIONALE_BYTES, ResearchCaseId, ResearchClaim, ResearchClaimId, ResearchError,
    ResearchEvidence, ResearchEvidenceId, ResearchPdfExtractionId, ResearchRepository,
    ResearchSourceSnapshotId, bounded_text, validate_metadata,
};
use nineprofs_common::now_ms;
use serde_json::json;

impl ResearchService {
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

    pub(super) async fn create_evidence_internal(
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
}
