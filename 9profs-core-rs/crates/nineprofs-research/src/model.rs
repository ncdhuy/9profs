use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use nineprofs_common::{TimestampMs, new_id};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_CASE_TITLE_BYTES: usize = 512;
pub const MAX_SOURCE_LABEL_BYTES: usize = 1_024;
pub const MAX_SNAPSHOT_CONTENT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_EVIDENCE_EXCERPT_BYTES: usize = 64 * 1024;
pub const MAX_NORMALIZED_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_CLAIM_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_RATIONALE_BYTES: usize = 16 * 1024;
pub const MAX_METADATA_BYTES: usize = 8 * 1024;
pub const MAX_LOCATOR_BYTES: usize = 4 * 1024;
pub const MAX_PROVENANCE_TEXT_BYTES: usize = 1_024;
pub const MAX_CITATION_MARKER_BYTES: usize = 4 * 1024;
pub const MAX_CITATION_REFERENCE_KEY_BYTES: usize = MAX_PROVENANCE_TEXT_BYTES;
pub const MAX_CITED_LOCATOR_BYTES: usize = MAX_PROVENANCE_TEXT_BYTES;
pub const MAX_CITATION_TARGETS_PER_OCCURRENCE: usize = 128;
pub const MAX_MANUSCRIPT_CITATION_OCCURRENCES: usize = 4_096;
pub const MAX_PDF_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_PDF_PAGES: u32 = 10_000;
pub const MAX_PDF_PAGE_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_PDF_EXTRACTION_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_RETRIEVAL_SCOPE_IDS: usize = 16;
pub const MAX_CLAIM_EXTRACTION_BLOCKS: usize = 4_096;
pub const MAX_CLAIM_EXTRACTION_CONTEXT_BYTES: usize = 512 * 1024;
pub const MAX_CLAIM_EXTRACTION_CITATIONS_PER_BLOCK: usize = 128;
pub const MAX_MANUSCRIPT_CLAIM_INVENTORY_BLOCKS: usize = 4_096;
pub const MAX_MANUSCRIPT_CLAIM_INVENTORY_BLOCK_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_MANUSCRIPT_CLAIM_INVENTORY_CONTEXT_BYTES: usize = 512 * 1024;
pub const MAX_MANUSCRIPT_CLAIM_INVENTORY_CLAIMS_PER_BLOCK: usize = 256;
pub const MAX_MANUSCRIPT_CLAIM_INVENTORY_CITATIONS_PER_BLOCK: usize = 128;
pub const MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_CONTRACT_VERSION: &str =
    "manuscript-claim-inventory-coverage-v1";
pub const MANUSCRIPT_CLAIM_INVENTORY_COVERAGE_SCOPE: &str = "paragraph,heading,list_item";
pub const MAX_MANUSCRIPT_REFERENCE_CATALOG_ENTRIES: usize = 4_096;
pub const MAX_MANUSCRIPT_REFERENCE_CATALOG_TARGETS: usize = 65_536;
pub const MAX_MANUSCRIPT_REFERENCE_CATALOG_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MANUSCRIPT_REFERENCE_URI_COUNT: usize = 16;
pub const MAX_MANUSCRIPT_REFERENCE_URI_BYTES: usize = 4 * 1024;
pub const MAX_REFERENCE_RESOLUTION_CANDIDATES: usize = 256;
pub const MAX_REGULATION_REQUIREMENT_EXTRACTION_PAGES: usize = 8;
pub const MAX_REGULATION_REQUIREMENT_EXTRACTION_CONTEXT_BYTES: usize = 512 * 1024;
pub const MAX_REGULATION_REQUIREMENT_CANDIDATES: usize = 256;
pub const MAX_REGULATION_REQUIREMENT_RISK_FLAGS: usize = 32;
pub const REFERENCE_RESOLVER_POLICY_VERSION: &str = "5c3b3b-v1";

#[derive(Debug, Error)]
pub enum ResearchError {
    #[error("research {entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },
    #[error("invalid research input: {0}")]
    Invalid(String),
    #[error("research database query failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("research serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("research artifact storage failed: {0}")]
    Artifact(String),
    #[error(
        "manuscript citation sync already exists for case {research_case_id}, source {manuscript_source_id}, document {document_id}, version {document_version}"
    )]
    ManuscriptCitationSyncConflict {
        research_case_id: String,
        manuscript_source_id: String,
        document_id: String,
        document_version: i64,
    },
    #[error("manuscript claim extractor is not configured")]
    ManuscriptClaimExtractorNotConfigured,
    #[error("manuscript claim extractor configuration is invalid: {0}")]
    ManuscriptClaimExtractorInvalidConfiguration(String),
    #[error("citation sync is stale for manuscript claim extraction")]
    ManuscriptClaimExtractionStale,
    #[error("manuscript claim extraction failed: {0}")]
    ManuscriptClaimExtractionFailed(String),
    #[error("manuscript claim inventory extractor is not configured")]
    ManuscriptClaimInventoryExtractorNotConfigured,
    #[error("manuscript claim inventory extractor configuration is invalid: {0}")]
    ManuscriptClaimInventoryExtractorInvalidConfiguration(String),
    #[error("manuscript claim inventory failed: {0}")]
    ManuscriptClaimInventoryFailed(String),
    #[error("regulation requirement candidate extractor is not configured")]
    RegulationRequirementCandidateExtractorNotConfigured,
    #[error("regulation requirement candidate extractor configuration is invalid: {0}")]
    RegulationRequirementCandidateExtractorInvalidConfiguration(String),
    #[error("regulation requirement candidate extraction failed: {0}")]
    RegulationRequirementCandidateExtractionFailed(String),
    #[error("manuscript reference catalog is stale for citation sync run")]
    ManuscriptReferenceCatalogStale,
    #[error(
        "manuscript reference catalog already exists for citation sync run {citation_sync_run_id}"
    )]
    ManuscriptReferenceCatalogConflict { citation_sync_run_id: String },
    #[error("reference descriptor conflicts for {format} reference {reference_key}")]
    ManuscriptReferenceDescriptorConflict {
        format: String,
        reference_key: String,
    },
}

macro_rules! id_type {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        pub struct $name(String);

        impl $name {
            pub fn new() -> Self {
                Self(new_id())
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, ResearchError> {
                let value = value.into();
                validate_identity($label, &value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

id_type!(ResearchCaseId, "case ID");
id_type!(ResearchSourceId, "source ID");
id_type!(ResearchSourceSnapshotId, "source snapshot ID");
id_type!(ResearchPdfExtractionId, "PDF extraction ID");
id_type!(RegulationRequirementId, "regulation requirement ID");
id_type!(
    RegulationRequirementCandidateId,
    "regulation requirement candidate ID"
);
id_type!(ResearchEvidenceId, "evidence ID");
id_type!(ResearchClaimId, "claim ID");
id_type!(ClaimEvidenceLinkId, "claim-evidence link ID");
id_type!(CitationOccurrenceId, "citation occurrence ID");
id_type!(CitationTargetId, "citation target ID");
id_type!(CitationTargetBindingId, "citation target binding ID");
id_type!(ClaimCitationLinkId, "claim-citation link ID");
id_type!(
    ManuscriptCitationSyncRunId,
    "manuscript citation sync run ID"
);
id_type!(
    ManuscriptCitationSyncOccurrenceId,
    "manuscript citation sync occurrence ID"
);
id_type!(
    ManuscriptCitationSyncTargetId,
    "manuscript citation sync target ID"
);
id_type!(
    ManuscriptClaimExtractionRunId,
    "manuscript claim extraction run ID"
);
id_type!(
    ManuscriptClaimExtractionItemId,
    "manuscript claim extraction item ID"
);
id_type!(
    ManuscriptClaimExtractionCoverageId,
    "manuscript claim extraction coverage ID"
);
id_type!(
    ManuscriptClaimInventoryRunId,
    "manuscript claim inventory run ID"
);
id_type!(
    ManuscriptClaimInventoryItemId,
    "manuscript claim inventory item ID"
);
id_type!(
    ManuscriptClaimInventoryCoverageId,
    "manuscript claim inventory coverage ID"
);
id_type!(
    ManuscriptReferenceCatalogRunId,
    "manuscript reference catalog run ID"
);
id_type!(ManuscriptReferenceEntryId, "manuscript reference entry ID");
id_type!(
    ManuscriptReferenceTargetMappingId,
    "manuscript reference target mapping ID"
);
id_type!(
    ManuscriptReferenceResolutionRunId,
    "manuscript reference resolution run ID"
);
id_type!(
    ManuscriptReferenceResolutionEntryId,
    "manuscript reference resolution entry ID"
);
id_type!(
    ManuscriptReferenceResolutionCandidateId,
    "manuscript reference resolution candidate ID"
);

/// Provider-neutral retrieval boundary. Provider adapters translate these
/// canonical identities into provider-specific filters.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResearchRetrievalScope {
    Case,
    Sources {
        source_ids: Vec<ResearchSourceId>,
    },
    Extractions {
        extraction_ids: Vec<ResearchPdfExtractionId>,
    },
}

impl ResearchRetrievalScope {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::Case => Ok(()),
            Self::Sources { source_ids } => validate_retrieval_scope_ids(source_ids),
            Self::Extractions { extraction_ids } => validate_retrieval_scope_ids(extraction_ids),
        }
    }
}

fn validate_retrieval_scope_ids<T: Ord>(ids: &[T]) -> Result<(), ResearchError> {
    if ids.is_empty() {
        return Err(ResearchError::Invalid(
            "retrieval scope must contain at least one identity".to_owned(),
        ));
    }
    if ids.len() > MAX_RETRIEVAL_SCOPE_IDS {
        return Err(ResearchError::Invalid(format!(
            "retrieval scope cannot contain more than {MAX_RETRIEVAL_SCOPE_IDS} identities"
        )));
    }
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(ResearchError::Invalid(
            "retrieval scope identities must be unique".to_owned(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchCase {
    pub id: ResearchCaseId,
    pub title: String,
    pub created_at_ms: TimestampMs,
    pub updated_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    ReferencePdf,
    Manuscript,
    Dataset,
    Web,
    Regulation,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchSource {
    pub id: ResearchSourceId,
    pub research_case_id: ResearchCaseId,
    pub kind: SourceKind,
    pub label: String,
    #[serde(default)]
    pub identity: Option<ResearchSourceIdentity>,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchSourceIdentityMethod {
    Imported,
    HumanConfirmed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchSourceIdentity {
    pub provider: String,
    pub external_reference: String,
    pub method: ResearchSourceIdentityMethod,
    pub asserted_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchSourceIdentityInput {
    pub provider: String,
    pub external_reference: String,
    pub method: ResearchSourceIdentityMethod,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgorithm {
    Sha256,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentHash {
    pub algorithm: HashAlgorithm,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMethod {
    UserProvided,
    UploadedArtifact,
    ActiveDocument,
    OfficeCli,
    WebRetrieval,
    ExternalImport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceOrigin {
    UploadedArtifact {
        artifact_id: String,
        revision_id: Option<String>,
    },
    ActiveDocumentSnapshot {
        document_id: String,
        document_version: String,
    },
    OfficeCliArtifactRevision {
        artifact_id: String,
        revision_id: String,
    },
    WebRetrieval {
        url: String,
        retrieved_at_ms: TimestampMs,
    },
    ExternalImport {
        provider: String,
        external_reference: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchSourceSnapshot {
    pub id: ResearchSourceSnapshotId,
    pub source_id: ResearchSourceId,
    pub content_hash: ContentHash,
    pub captured_at_ms: TimestampMs,
    pub capture_method: CaptureMethod,
    pub origin: SourceOrigin,
    pub metadata: SafeMetadata,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchContext {
    pub language: Option<String>,
    #[serde(default)]
    pub research_families: Vec<String>,
    pub artifact_type: Option<String>,
    pub academic_level: Option<String>,
    #[serde(default)]
    pub study_designs: Vec<String>,
    #[serde(default)]
    pub reporting_guidelines: Vec<String>,
    pub organization: Option<String>,
}

impl ResearchContext {
    pub fn validate(&self) -> Result<(), ResearchError> {
        validate_optional_context_value("language", self.language.as_deref())?;
        validate_context_values("research family", &self.research_families)?;
        validate_optional_context_value("artifact type", self.artifact_type.as_deref())?;
        validate_optional_context_value("academic level", self.academic_level.as_deref())?;
        validate_context_values("study design", &self.study_designs)?;
        validate_context_values("reporting guideline", &self.reporting_guidelines)?;
        validate_optional_context_value("organization", self.organization.as_deref())?;
        Ok(())
    }

    fn values_for(&self, facet: &str) -> Option<Vec<&str>> {
        match normalize_identifier(facet).as_str() {
            "language" | "languages" => Some(
                self.language
                    .as_deref()
                    .map(|value| vec![value])
                    .unwrap_or_default(),
            ),
            "research_family" | "research_families" => {
                Some(self.research_families.iter().map(String::as_str).collect())
            }
            "artifact_type" | "artifact_types" => Some(
                self.artifact_type
                    .as_deref()
                    .map(|value| vec![value])
                    .unwrap_or_default(),
            ),
            "academic_level" | "academic_levels" => Some(
                self.academic_level
                    .as_deref()
                    .map(|value| vec![value])
                    .unwrap_or_default(),
            ),
            "study_design" | "study_designs" => {
                Some(self.study_designs.iter().map(String::as_str).collect())
            }
            "reporting_guideline" | "reporting_guidelines" => Some(
                self.reporting_guidelines
                    .iter()
                    .map(String::as_str)
                    .collect(),
            ),
            "organization" | "organizations" => Some(
                self.organization
                    .as_deref()
                    .map(|value| vec![value])
                    .unwrap_or_default(),
            ),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegulationApplicability {
    #[serde(flatten)]
    pub facets: BTreeMap<String, Vec<String>>,
}

impl RegulationApplicability {
    pub fn validate(&self) -> Result<(), ResearchError> {
        for (facet, values) in &self.facets {
            bounded_text("applicability facet", facet, MAX_PROVENANCE_TEXT_BYTES)?;
            if values.is_empty() {
                return Err(ResearchError::Invalid(format!(
                    "applicability facet must contain at least one value: {facet}"
                )));
            }
            for value in values {
                bounded_text("applicability value", value, MAX_PROVENANCE_TEXT_BYTES)?;
            }
        }
        Ok(())
    }

    pub fn matches(&self, context: &ResearchContext) -> bool {
        self.facets.iter().all(|(facet, accepted_values)| {
            let Some(context_values) = context.values_for(facet) else {
                return false;
            };
            !accepted_values.is_empty()
                && context_values.iter().any(|context_value| {
                    accepted_values.iter().any(|accepted_value| {
                        normalize_identifier(accepted_value) == normalize_identifier(context_value)
                    })
                })
        })
    }

    pub fn validate_context_facets(&self) -> Result<(), ResearchError> {
        self.validate()?;
        for facet in self.facets.keys() {
            if canonical_applicability_facet(facet).is_none() {
                return Err(ResearchError::Invalid(format!(
                    "unsupported regulation applicability facet: {facet}"
                )));
            }
        }
        Ok(())
    }

    pub fn validate_for_extraction(
        &self,
        vocabulary: &RegulationApplicabilityVocabulary,
    ) -> Result<(), ResearchError> {
        self.validate_context_facets()?;
        validate_regulation_applicability_vocabulary(vocabulary)?;
        if self.facets.is_empty() {
            return Ok(());
        }
        if vocabulary.is_empty() {
            return Err(ResearchError::Invalid(
                "regulation applicability vocabulary is required for non-empty suggestions"
                    .to_owned(),
            ));
        }
        for (facet, values) in &self.facets {
            let canonical = canonical_applicability_facet(facet).expect("validated facet");
            let allowed = vocabulary.iter().find_map(|(key, values)| {
                (canonical_applicability_facet(key) == Some(canonical)).then_some(values)
            });
            let Some(allowed) = allowed else {
                return Err(ResearchError::Invalid(format!(
                    "regulation applicability facet is not in supplied vocabulary: {facet}"
                )));
            };
            for value in values {
                if !allowed.iter().any(|candidate| candidate == value) {
                    return Err(ResearchError::Invalid(format!(
                        "unsupported regulation applicability value for {facet}: {value}"
                    )));
                }
            }
        }
        Ok(())
    }
}

pub type RegulationApplicabilityVocabulary = BTreeMap<String, Vec<String>>;

fn canonical_applicability_facet(facet: &str) -> Option<&'static str> {
    match normalize_identifier(facet).as_str() {
        "language" | "languages" => Some("language"),
        "research_family" | "research_families" => Some("research_families"),
        "artifact_type" | "artifact_types" => Some("artifact_types"),
        "academic_level" | "academic_levels" => Some("academic_levels"),
        "study_design" | "study_designs" => Some("study_designs"),
        "reporting_guideline" | "reporting_guidelines" => Some("reporting_guidelines"),
        "organization" | "organizations" => Some("organization"),
        _ => None,
    }
}

fn validate_regulation_applicability_vocabulary(
    vocabulary: &RegulationApplicabilityVocabulary,
) -> Result<(), ResearchError> {
    let mut seen = BTreeSet::new();
    for (facet, values) in vocabulary {
        let Some(canonical) = canonical_applicability_facet(facet) else {
            return Err(ResearchError::Invalid(format!(
                "unsupported regulation applicability vocabulary facet: {facet}"
            )));
        };
        if !seen.insert(canonical) {
            return Err(ResearchError::Invalid(format!(
                "duplicate regulation applicability vocabulary facet: {facet}"
            )));
        }
        if values.is_empty() {
            return Err(ResearchError::Invalid(format!(
                "regulation applicability vocabulary facet must contain at least one value: {facet}"
            )));
        }
        for value in values {
            bounded_text(
                "regulation applicability vocabulary value",
                value,
                MAX_PROVENANCE_TEXT_BYTES,
            )?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegulationReviewStatus {
    NeedsReview,
    Approved,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegulationRequirement {
    pub id: RegulationRequirementId,
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub pdf_extraction_id: Option<ResearchPdfExtractionId>,
    pub text: String,
    pub source_excerpt: String,
    pub source_excerpt_hash: ContentHash,
    pub source_locator: EvidenceLocator,
    pub authority_locator: Option<EvidenceLocator>,
    pub applicability: RegulationApplicability,
    pub effective_from: Option<TimestampMs>,
    pub effective_until: Option<TimestampMs>,
    pub extraction_method: String,
    pub extraction_contract_version: Option<String>,
    pub review_status: RegulationReviewStatus,
    pub active: bool,
    pub created_at_ms: TimestampMs,
    pub updated_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegulationRequirementCandidateExtraction {
    pub method: String,
    pub contract_version: String,
    pub provider: String,
    pub extractor_version: String,
    pub model_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegulationRequirementCandidate {
    pub id: RegulationRequirementCandidateId,
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub pdf_extraction_id: ResearchPdfExtractionId,
    pub source_locator: EvidenceLocator,
    pub authority_locator_suggestion: Option<EvidenceLocator>,
    pub ocr_excerpt: String,
    pub normalized_requirement: String,
    pub applicability_suggestion: RegulationApplicability,
    pub extraction: RegulationRequirementCandidateExtraction,
    pub risk_flags: Vec<String>,
    pub review_notes: Option<String>,
    pub created_at_ms: TimestampMs,
}

impl RegulationRequirementCandidate {
    pub fn validate(&self) -> Result<(), ResearchError> {
        bounded_text(
            "regulation requirement candidate OCR excerpt",
            &self.ocr_excerpt,
            MAX_EVIDENCE_EXCERPT_BYTES,
        )?;
        bounded_text(
            "regulation requirement candidate normalized requirement",
            &self.normalized_requirement,
            MAX_NORMALIZED_TEXT_BYTES,
        )?;
        if !matches!(
            self.source_locator,
            EvidenceLocator::Pdf { .. } | EvidenceLocator::PdfTextRange { .. }
        ) {
            return Err(ResearchError::Invalid(
                "regulation requirement candidate source locator must be a PDF locator".to_owned(),
            ));
        }
        self.source_locator.validate()?;
        if let Some(locator) = &self.authority_locator_suggestion {
            if !matches!(locator, EvidenceLocator::Regulation { .. }) {
                return Err(ResearchError::Invalid(
                    "regulation requirement candidate authority locator suggestion must be a regulation locator"
                        .to_owned(),
                ));
            }
            locator.validate()?;
        }
        self.applicability_suggestion.validate_context_facets()?;
        bounded_text(
            "regulation requirement candidate extraction method",
            &self.extraction.method,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        bounded_text(
            "regulation requirement candidate contract version",
            &self.extraction.contract_version,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        bounded_text(
            "regulation requirement candidate provider",
            &self.extraction.provider,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        bounded_text(
            "regulation requirement candidate extractor version",
            &self.extraction.extractor_version,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        if let Some(model_id) = &self.extraction.model_id {
            bounded_text(
                "regulation requirement candidate model",
                model_id,
                MAX_PROVENANCE_TEXT_BYTES,
            )?;
        }
        if self.risk_flags.len() > MAX_REGULATION_REQUIREMENT_RISK_FLAGS {
            return Err(ResearchError::Invalid(format!(
                "regulation requirement candidate cannot contain more than {MAX_REGULATION_REQUIREMENT_RISK_FLAGS} risk flags"
            )));
        }
        for flag in &self.risk_flags {
            bounded_text(
                "regulation requirement candidate risk flag",
                flag,
                MAX_PROVENANCE_TEXT_BYTES,
            )?;
        }
        if let Some(notes) = &self.review_notes {
            bounded_text(
                "regulation requirement candidate review notes",
                notes,
                MAX_RATIONALE_BYTES,
            )?;
        }
        Ok(())
    }
}

impl RegulationRequirement {
    pub fn validate(&self) -> Result<(), ResearchError> {
        bounded_text("requirement text", &self.text, MAX_NORMALIZED_TEXT_BYTES)?;
        bounded_text(
            "requirement source excerpt",
            &self.source_excerpt,
            MAX_EVIDENCE_EXCERPT_BYTES,
        )?;
        self.source_locator.validate()?;
        if let Some(locator) = &self.authority_locator {
            locator.validate()?;
        }
        self.applicability.validate()?;
        bounded_text(
            "requirement extraction method",
            &self.extraction_method,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        if let Some(version) = &self.extraction_contract_version {
            bounded_text(
                "requirement extraction contract version",
                version,
                MAX_PROVENANCE_TEXT_BYTES,
            )?;
        }
        if let (Some(from), Some(until)) = (self.effective_from, self.effective_until) {
            if from > until {
                return Err(ResearchError::Invalid(
                    "requirement effective_from must not exceed effective_until".to_owned(),
                ));
            }
        }
        if self.active && !matches!(self.review_status, RegulationReviewStatus::Approved) {
            return Err(ResearchError::Invalid(
                "active regulation requirement must be approved".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn is_temporally_effective(&self, as_of_ms: TimestampMs) -> bool {
        self.effective_from.is_none_or(|from| as_of_ms >= from)
            && self.effective_until.is_none_or(|until| as_of_ms <= until)
    }
}

pub fn resolve_effective_regulation_requirements(
    requirements: &[RegulationRequirement],
    context: &ResearchContext,
    as_of_ms: TimestampMs,
) -> Vec<RegulationRequirement> {
    requirements
        .iter()
        .filter(|requirement| {
            matches!(requirement.review_status, RegulationReviewStatus::Approved)
                && requirement.active
                && requirement.applicability.matches(context)
                && requirement.is_temporally_effective(as_of_ms)
        })
        .cloned()
        .collect()
}

fn validate_optional_context_value(field: &str, value: Option<&str>) -> Result<(), ResearchError> {
    if let Some(value) = value {
        bounded_text(field, value, MAX_PROVENANCE_TEXT_BYTES)?;
    }
    Ok(())
}

fn validate_context_values(field: &str, values: &[String]) -> Result<(), ResearchError> {
    for value in values {
        bounded_text(field, value, MAX_PROVENANCE_TEXT_BYTES)?;
    }
    Ok(())
}

fn normalize_identifier(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[derive(Clone, Debug)]
pub struct CreateRegulationRequirement {
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub pdf_extraction_id: Option<ResearchPdfExtractionId>,
    pub text: String,
    pub source_excerpt: String,
    pub source_locator: EvidenceLocator,
    pub authority_locator: Option<EvidenceLocator>,
    pub applicability: RegulationApplicability,
    pub effective_from: Option<TimestampMs>,
    pub effective_until: Option<TimestampMs>,
    pub extraction_method: String,
    pub extraction_contract_version: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PromoteRegulationRequirementCandidate {
    pub candidate_id: RegulationRequirementCandidateId,
    pub text: String,
    pub source_excerpt: String,
    pub source_locator: EvidenceLocator,
    pub authority_locator: Option<EvidenceLocator>,
    pub applicability: RegulationApplicability,
    pub effective_from: Option<TimestampMs>,
    pub effective_until: Option<TimestampMs>,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct ExtractRegulationRequirementCandidates {
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub pdf_extraction_id: ResearchPdfExtractionId,
    pub start_page: u32,
    pub end_page: u32,
    pub institution: Option<String>,
    pub document_title: Option<String>,
    pub known_artifact_scope: Option<String>,
    pub allowed_applicability_vocabulary: RegulationApplicabilityVocabulary,
}

#[derive(Clone, Debug, Serialize)]
pub struct RegulationRequirementExtractionPage {
    pub page: u32,
    pub text: String,
    pub heading_context: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RegulationRequirementExtractionInput {
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub pdf_extraction_id: ResearchPdfExtractionId,
    pub start_page: u32,
    pub end_page: u32,
    pub pages: Vec<RegulationRequirementExtractionPage>,
    pub institution: Option<String>,
    pub document_title: Option<String>,
    pub known_artifact_scope: Option<String>,
    pub allowed_applicability_vocabulary: RegulationApplicabilityVocabulary,
}

impl RegulationRequirementExtractionInput {
    pub fn validate(&self) -> Result<(), ResearchError> {
        if self.start_page == 0 || self.end_page < self.start_page {
            return Err(ResearchError::Invalid(
                "regulation requirement extraction page range is invalid".to_owned(),
            ));
        }
        let page_count = (self.end_page - self.start_page + 1) as usize;
        if page_count > MAX_REGULATION_REQUIREMENT_EXTRACTION_PAGES {
            return Err(ResearchError::Invalid(format!(
                "regulation requirement extraction cannot contain more than {MAX_REGULATION_REQUIREMENT_EXTRACTION_PAGES} pages"
            )));
        }
        if self.pages.len() != page_count
            || self
                .pages
                .iter()
                .enumerate()
                .any(|(index, page)| page.page != self.start_page + index as u32)
        {
            return Err(ResearchError::Invalid(
                "regulation requirement extraction pages must cover requested contiguous range"
                    .to_owned(),
            ));
        }
        for page in &self.pages {
            if page.text.len() > MAX_PDF_PAGE_TEXT_BYTES
                || page
                    .text
                    .chars()
                    .any(|character| character == '\0' || character == '\u{7f}')
            {
                return Err(ResearchError::Invalid(format!(
                    "regulation requirement extraction page {} text is invalid",
                    page.page
                )));
            }
            if let Some(heading) = &page.heading_context {
                bounded_text(
                    "regulation requirement extraction heading context",
                    heading,
                    MAX_PROVENANCE_TEXT_BYTES,
                )?;
            }
        }
        for (name, value) in [
            ("institution", self.institution.as_deref()),
            ("document title", self.document_title.as_deref()),
            ("known artifact scope", self.known_artifact_scope.as_deref()),
        ] {
            if let Some(value) = value {
                bounded_text(name, value, MAX_PROVENANCE_TEXT_BYTES)?;
            }
        }
        validate_regulation_applicability_vocabulary(&self.allowed_applicability_vocabulary)?;
        let serialized = serde_json::to_vec(self)?;
        if serialized.len() > MAX_REGULATION_REQUIREMENT_EXTRACTION_CONTEXT_BYTES {
            return Err(ResearchError::Invalid(format!(
                "regulation requirement extraction input exceeds {MAX_REGULATION_REQUIREMENT_EXTRACTION_CONTEXT_BYTES} bytes"
            )));
        }
        Ok(())
    }

    pub fn page_text(&self) -> String {
        self.pages
            .iter()
            .map(|page| page.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegulationRequirementCandidateExtractionIdentity {
    pub provider: String,
    pub extractor_version: String,
    pub model_id: Option<String>,
    pub extraction_contract_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RegulationRequirementCandidateOutput {
    pub ocr_excerpt: String,
    pub normalized_requirement: String,
    pub source_locator: EvidenceLocator,
    pub authority_locator: Option<EvidenceLocator>,
    pub applicability: RegulationApplicability,
    pub risk_flags: Vec<String>,
    pub review_notes: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchArtifact {
    pub id: String,
    pub content_hash: ContentHash,
    pub size_bytes: u64,
    pub media_type: String,
    pub original_filename: String,
    pub created_at_ms: TimestampMs,
}

/// Hash and identity returned only by the trusted artifact writer.
#[derive(Clone, Debug)]
pub struct VerifiedArtifact {
    artifact: ResearchArtifact,
}

impl VerifiedArtifact {
    pub(crate) fn new(artifact: ResearchArtifact) -> Self {
        Self { artifact }
    }

    pub fn artifact(&self) -> &ResearchArtifact {
        &self.artifact
    }

    pub fn artifact_id(&self) -> &str {
        &self.artifact.id
    }

    pub fn content_hash(&self) -> &ContentHash {
        &self.artifact.content_hash
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PdfExtractionStatus {
    Ready,
    NoExtractableText,
    Failed,
    PasswordRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchPdfExtraction {
    pub id: ResearchPdfExtractionId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub artifact_id: String,
    pub extractor: String,
    pub extractor_version: String,
    pub page_count: u32,
    pub extraction_hash: ContentHash,
    pub extracted_at_ms: TimestampMs,
    pub status: PdfExtractionStatus,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchPdfPage {
    pub extraction_id: ResearchPdfExtractionId,
    pub page: u32,
    pub text: String,
    pub text_hash: ContentHash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResearchPdfPageBatch {
    pub pages: Vec<ResearchPdfPage>,
    pub start_page: u32,
    pub limit: u32,
    pub has_more: bool,
    pub next_start_page: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EvidenceLocator {
    TextRange {
        start: u64,
        end: u64,
    },
    Pdf {
        page: u32,
        end_page: Option<u32>,
    },
    /// Page-relative Unicode scalar/code-point offsets. They are not UTF-8 byte
    /// offsets and not JavaScript UTF-16 indexes.
    PdfTextRange {
        page: u32,
        start: u64,
        end: u64,
    },
    Manuscript {
        block_id: String,
        start: Option<u64>,
        end: Option<u64>,
    },
    Spreadsheet {
        sheet: String,
        range: String,
    },
    Web {
        fragment: Option<String>,
        start: Option<u64>,
        end: Option<u64>,
    },
    Regulation {
        article: String,
        section: Option<String>,
        clause: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchEvidence {
    pub id: ResearchEvidenceId,
    pub research_case_id: ResearchCaseId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub verbatim_excerpt: String,
    pub normalized_text: Option<String>,
    pub locator: EvidenceLocator,
    pub excerpt_hash: ContentHash,
    pub captured_at_ms: TimestampMs,
    pub capture_method: CaptureMethod,
    pub pdf_extraction_id: Option<ResearchPdfExtractionId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClaimOrigin {
    Manuscript {
        document_id: String,
        document_version: String,
        locator: Option<EvidenceLocator>,
    },
    User,
    Agent,
    Imported {
        source: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResearchClaim {
    pub id: ResearchClaimId,
    pub research_case_id: ResearchCaseId,
    pub text: String,
    pub origin: ClaimOrigin,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CitationOccurrenceOrigin {
    Manuscript {
        document_id: String,
        document_version: String,
        locator: Option<EvidenceLocator>,
    },
    /// Future immutable manuscript provenance seam. Phase 5C1 stores the
    /// explicit snapshot identity but does not create manuscript snapshots.
    ManuscriptSnapshot {
        source_snapshot_id: ResearchSourceSnapshotId,
        locator: Option<EvidenceLocator>,
    },
    Imported {
        source: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CitationOccurrence {
    pub id: CitationOccurrenceId,
    pub research_case_id: ResearchCaseId,
    pub origin: CitationOccurrenceOrigin,
    pub rendered_text: String,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CitationTarget {
    pub id: CitationTargetId,
    pub citation_occurrence_id: CitationOccurrenceId,
    pub ordinal: u32,
    pub reference_key: String,
    pub cited_locator: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptCitationFormat {
    WordNative,
    Zotero,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptCitationSyncStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptCitationSyncRun {
    pub id: ManuscriptCitationSyncRunId,
    pub research_case_id: ResearchCaseId,
    pub manuscript_source_id: ResearchSourceId,
    pub document_id: String,
    pub document_version: i64,
    pub inventory_hash: ContentHash,
    pub status: ManuscriptCitationSyncStatus,
    pub occurrence_count: u32,
    pub created_at_ms: TimestampMs,
    pub completed_at_ms: Option<TimestampMs>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptCitationSyncOccurrence {
    pub id: ManuscriptCitationSyncOccurrenceId,
    pub sync_run_id: ManuscriptCitationSyncRunId,
    pub ordinal: u32,
    pub citation_occurrence_id: CitationOccurrenceId,
    pub document_block_id: String,
    pub start: u64,
    pub end: u64,
    pub format: ManuscriptCitationFormat,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptCitationSyncTarget {
    pub id: ManuscriptCitationSyncTargetId,
    pub sync_occurrence_id: ManuscriptCitationSyncOccurrenceId,
    pub document_target_ordinal: u32,
    pub citation_target_id: CitationTargetId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptReferenceCatalogStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptReferenceResolutionStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptReferenceResolutionOutcome {
    ResolvedExact,
    AlreadyBound,
    AmbiguousSource,
    AmbiguousSnapshotOrExtraction,
    CandidateRequiresConfirmation,
    SourceMatchedButNotVerificationReady,
    Unresolved,
    ConflictWithExistingBinding,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptReferenceResolutionMatchKind {
    ExactZoteroItemId,
    ExactZoteroUri,
    ReferenceKeySourceLabel,
    ReferenceTitleSourceLabel,
    MappingIntegrity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceResolutionRun {
    pub id: ManuscriptReferenceResolutionRunId,
    pub research_case_id: ResearchCaseId,
    pub catalog_run_id: ManuscriptReferenceCatalogRunId,
    pub catalog_hash: ContentHash,
    pub source_state_hash: ContentHash,
    pub resolver_policy_version: String,
    pub status: ManuscriptReferenceResolutionStatus,
    pub entry_count: u32,
    pub resolved_entry_count: u32,
    pub candidate_entry_count: u32,
    pub unresolved_entry_count: u32,
    pub conflict_entry_count: u32,
    pub created_at_ms: TimestampMs,
    pub completed_at_ms: Option<TimestampMs>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceResolutionEntry {
    pub id: ManuscriptReferenceResolutionEntryId,
    pub resolution_run_id: ManuscriptReferenceResolutionRunId,
    pub reference_entry_id: ManuscriptReferenceEntryId,
    pub outcome: ManuscriptReferenceResolutionOutcome,
    pub match_kind: Option<ManuscriptReferenceResolutionMatchKind>,
    pub chosen_source_id: Option<ResearchSourceId>,
    pub chosen_source_snapshot_id: Option<ResearchSourceSnapshotId>,
    pub chosen_extraction_id: Option<ResearchPdfExtractionId>,
    pub automatic_binding_permitted: bool,
    pub candidate_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceResolutionCandidate {
    pub id: ManuscriptReferenceResolutionCandidateId,
    pub resolution_entry_id: ManuscriptReferenceResolutionEntryId,
    pub ordinal: u32,
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: Option<ResearchSourceSnapshotId>,
    pub extraction_id: Option<ResearchPdfExtractionId>,
    pub match_kind: ManuscriptReferenceResolutionMatchKind,
    pub automatic_binding_permitted: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceCatalogRun {
    pub id: ManuscriptReferenceCatalogRunId,
    pub research_case_id: ResearchCaseId,
    pub manuscript_source_id: ResearchSourceId,
    pub citation_sync_run_id: ManuscriptCitationSyncRunId,
    pub document_id: String,
    pub document_version: i64,
    pub catalog_hash: ContentHash,
    pub entry_count: u32,
    pub target_mapping_count: u32,
    pub status: ManuscriptReferenceCatalogStatus,
    pub created_at_ms: TimestampMs,
    pub completed_at_ms: Option<TimestampMs>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceEntry {
    pub id: ManuscriptReferenceEntryId,
    pub catalog_run_id: ManuscriptReferenceCatalogRunId,
    pub ordinal: u32,
    pub format: ManuscriptCitationFormat,
    pub reference_key: String,
    pub descriptor_hash: ContentHash,
    pub word_tag: Option<String>,
    pub word_title: Option<String>,
    pub word_author: Option<String>,
    pub word_year: Option<String>,
    pub zotero_item_id: Option<String>,
    pub zotero_uris: Vec<String>,
    pub target_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceTargetMapping {
    pub id: ManuscriptReferenceTargetMappingId,
    pub catalog_run_id: ManuscriptReferenceCatalogRunId,
    pub reference_entry_id: ManuscriptReferenceEntryId,
    pub citation_occurrence_id: CitationOccurrenceId,
    pub citation_target_id: CitationTargetId,
    pub document_target_ordinal: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationBindingMethod {
    Human,
    Imported,
    DeterministicResolver,
    Agent,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CitationTargetBinding {
    pub id: CitationTargetBindingId,
    pub research_case_id: ResearchCaseId,
    pub citation_target_id: CitationTargetId,
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: Option<ResearchSourceSnapshotId>,
    pub extraction_id: Option<ResearchPdfExtractionId>,
    pub method: CitationBindingMethod,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationTargetResolution {
    Unresolved,
    SourceBound,
    PdfExtractionBound,
}

impl CitationTargetBinding {
    pub fn resolution(&self) -> CitationTargetResolution {
        if self.extraction_id.is_some() {
            CitationTargetResolution::PdfExtractionBound
        } else {
            CitationTargetResolution::SourceBound
        }
    }

    pub fn pdf_verification_ready(&self) -> bool {
        matches!(
            self.resolution(),
            CitationTargetResolution::PdfExtractionBound
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClaimCitationLink {
    pub id: ClaimCitationLinkId,
    pub research_case_id: ResearchCaseId,
    pub claim_id: ResearchClaimId,
    pub citation_occurrence_id: CitationOccurrenceId,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimExtractionStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimExtractionCoverageStatus {
    AssociatedWithClaim,
    NoVerifiableClaim,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimExtractionRun {
    pub id: ManuscriptClaimExtractionRunId,
    pub research_case_id: ResearchCaseId,
    pub manuscript_source_id: ResearchSourceId,
    pub citation_sync_run_id: ManuscriptCitationSyncRunId,
    pub document_id: String,
    pub document_version: i64,
    pub context_hash: ContentHash,
    pub extractor_provider: String,
    pub extractor_version: String,
    pub extractor_model_id: Option<String>,
    pub extraction_contract_version: String,
    pub status: ManuscriptClaimExtractionStatus,
    pub claim_count: u32,
    pub created_at_ms: TimestampMs,
    pub completed_at_ms: Option<TimestampMs>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimExtractionItem {
    pub id: ManuscriptClaimExtractionItemId,
    pub extraction_run_id: ManuscriptClaimExtractionRunId,
    pub research_claim_id: ResearchClaimId,
    pub document_block_id: String,
    pub source_start: u64,
    pub source_end: u64,
    pub source_excerpt: String,
    pub source_excerpt_hash: ContentHash,
    pub ordinal: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimExtractionCoverage {
    pub id: ManuscriptClaimExtractionCoverageId,
    pub extraction_run_id: ManuscriptClaimExtractionRunId,
    pub extraction_item_id: Option<ManuscriptClaimExtractionItemId>,
    pub claim_citation_link_id: Option<ClaimCitationLinkId>,
    pub citation_occurrence_id: CitationOccurrenceId,
    pub status: ManuscriptClaimExtractionCoverageStatus,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimInventoryStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimInventoryCoverageStatus {
    Processed,
    NoClaims,
    Excluded,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptClaimInventoryBlockKind {
    Paragraph,
    Heading,
    ListItem,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimReviewKind {
    ExternalEvidence,
    ManuscriptInternal,
    Interpretive,
    NonEvidentiary,
    Uncertain,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimInventoryRun {
    pub id: ManuscriptClaimInventoryRunId,
    pub research_case_id: ResearchCaseId,
    pub manuscript_source_id: ResearchSourceId,
    pub document_id: String,
    pub document_version: i64,
    pub document_context_hash: ContentHash,
    pub extractor_provider: String,
    pub extractor_version: String,
    pub extractor_model_id: Option<String>,
    pub extraction_contract_version: String,
    pub coverage_contract_version: String,
    pub coverage_scope: String,
    pub coverage_limitations: Vec<String>,
    pub status: ManuscriptClaimInventoryStatus,
    pub item_count: u32,
    pub covered_block_count: u32,
    pub created_at_ms: TimestampMs,
    pub completed_at_ms: Option<TimestampMs>,
    pub failure_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimInventoryItem {
    pub id: ManuscriptClaimInventoryItemId,
    pub inventory_run_id: ManuscriptClaimInventoryRunId,
    pub ordinal: u32,
    pub document_block_id: String,
    pub block_ordinal: u32,
    pub block_kind: ManuscriptClaimInventoryBlockKind,
    pub source_start: u64,
    pub source_end: u64,
    pub source_excerpt: String,
    pub source_excerpt_hash: ContentHash,
    pub claim_text: String,
    pub review_kind: ClaimReviewKind,
    pub overlapping_citation_count: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimInventoryCoverage {
    pub id: ManuscriptClaimInventoryCoverageId,
    pub inventory_run_id: ManuscriptClaimInventoryRunId,
    pub document_block_id: String,
    pub block_ordinal: u32,
    pub block_kind: ManuscriptClaimInventoryBlockKind,
    pub status: ManuscriptClaimInventoryCoverageStatus,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimEvidenceRelation {
    Supports,
    Contradicts,
    Contextualizes,
    Insufficient,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentMethod {
    Human,
    DeterministicChecker,
    Agent,
    ExternalService,
}

pub type SafeMetadata = BTreeMap<String, String>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClaimEvidenceLink {
    pub id: ClaimEvidenceLinkId,
    pub research_case_id: ResearchCaseId,
    pub claim_id: ResearchClaimId,
    pub evidence_id: ResearchEvidenceId,
    pub relation: ClaimEvidenceRelation,
    pub rationale: Option<String>,
    pub assessment_method: AssessmentMethod,
    pub assessment_metadata: SafeMetadata,
    pub created_at_ms: TimestampMs,
}

#[derive(Clone, Debug)]
pub struct CreateResearchCase {
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct CreateResearchSource {
    pub research_case_id: ResearchCaseId,
    pub kind: SourceKind,
    pub label: String,
    pub identity: Option<ResearchSourceIdentityInput>,
}

#[derive(Clone, Debug)]
pub struct CaptureSourceSnapshot {
    pub source_id: ResearchSourceId,
    pub content: Vec<u8>,
    pub capture_method: CaptureMethod,
    pub origin: SourceOrigin,
    pub metadata: SafeMetadata,
}

#[derive(Clone, Debug)]
pub struct CapturePdfExtraction {
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub extractor: String,
    pub extractor_version: Option<String>,
    pub page_count: u32,
    pub status: PdfExtractionStatus,
    pub pages: Vec<CapturePdfPage>,
}

#[derive(Clone, Debug)]
pub struct CapturePdfPage {
    pub page: u32,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct CapturePdfEvidence {
    pub research_case_id: ResearchCaseId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub extraction_id: ResearchPdfExtractionId,
    pub page: u32,
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug)]
pub struct CreateResearchEvidence {
    pub research_case_id: ResearchCaseId,
    pub source_snapshot_id: ResearchSourceSnapshotId,
    pub verbatim_excerpt: String,
    pub normalized_text: Option<String>,
    pub locator: EvidenceLocator,
    pub capture_method: CaptureMethod,
}

#[derive(Clone, Debug)]
pub struct CreateResearchClaim {
    pub research_case_id: ResearchCaseId,
    pub text: String,
    pub origin: ClaimOrigin,
}

#[derive(Clone, Debug)]
pub struct CreateClaimEvidenceLink {
    pub research_case_id: ResearchCaseId,
    pub claim_id: ResearchClaimId,
    pub evidence_id: ResearchEvidenceId,
    pub relation: ClaimEvidenceRelation,
    pub rationale: Option<String>,
    pub assessment_method: AssessmentMethod,
    pub assessment_metadata: SafeMetadata,
}

#[derive(Clone, Debug)]
pub struct CreateCitationOccurrence {
    pub research_case_id: ResearchCaseId,
    pub origin: CitationOccurrenceOrigin,
    pub rendered_text: String,
}

#[derive(Clone, Debug)]
pub struct CreateCitationTarget {
    pub citation_occurrence_id: CitationOccurrenceId,
    pub ordinal: u32,
    pub reference_key: String,
    pub cited_locator: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ManuscriptCitationSyncTargetInput {
    pub ordinal: u32,
    pub reference_key: String,
    pub cited_locator: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ManuscriptCitationSyncCitationInput {
    pub format: ManuscriptCitationFormat,
    pub rendered_text: String,
    pub block_id: String,
    pub start: u64,
    pub end: u64,
    pub targets: Vec<ManuscriptCitationSyncTargetInput>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncManuscriptCitations {
    pub research_case_id: ResearchCaseId,
    pub manuscript_source_id: ResearchSourceId,
    pub document_id: String,
    pub document_version: i64,
    pub citations: Vec<ManuscriptCitationSyncCitationInput>,
}

#[derive(Clone, Debug)]
pub struct ManuscriptCitationSyncWrite {
    pub run: ManuscriptCitationSyncRun,
    pub citation_occurrences: Vec<CitationOccurrence>,
    pub citation_targets: Vec<CitationTarget>,
    pub sync_occurrences: Vec<ManuscriptCitationSyncOccurrence>,
    pub sync_targets: Vec<ManuscriptCitationSyncTarget>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceCatalogWordSourceInput {
    pub tag: String,
    pub title: String,
    pub author: String,
    pub year: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceCatalogZoteroInput {
    pub item_id: Option<String>,
    pub uris: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceCatalogTargetInput {
    pub citation_target_id: String,
    pub ordinal: u32,
    pub reference_key: String,
    pub word_source: Option<ManuscriptReferenceCatalogWordSourceInput>,
    pub zotero: Option<ManuscriptReferenceCatalogZoteroInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptReferenceCatalogCitationInput {
    pub citation_occurrence_id: String,
    pub block_id: String,
    pub start: u64,
    pub end: u64,
    pub format: ManuscriptCitationFormat,
    pub targets: Vec<ManuscriptReferenceCatalogTargetInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SyncManuscriptReferenceCatalog {
    pub citation_sync_run_id: ManuscriptCitationSyncRunId,
    pub document_id: String,
    pub document_version: i64,
    pub citations: Vec<ManuscriptReferenceCatalogCitationInput>,
}

#[derive(Clone, Debug)]
pub struct ManuscriptReferenceCatalogWrite {
    pub run: ManuscriptReferenceCatalogRun,
    pub entries: Vec<ManuscriptReferenceEntry>,
    pub mappings: Vec<ManuscriptReferenceTargetMapping>,
}

#[derive(Clone, Debug)]
pub struct ManuscriptReferenceResolutionWrite {
    pub run: ManuscriptReferenceResolutionRun,
    pub entries: Vec<ManuscriptReferenceResolutionEntry>,
    pub candidates: Vec<ManuscriptReferenceResolutionCandidate>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimExtractionCitationInput {
    pub citation_occurrence_id: String,
    pub start: u64,
    pub end: u64,
    pub rendered_text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimExtractionBlockInput {
    pub block_id: String,
    pub text: String,
    pub citations: Vec<ManuscriptClaimExtractionCitationInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExtractManuscriptClaims {
    pub citation_sync_run_id: ManuscriptCitationSyncRunId,
    pub document_id: String,
    pub document_version: i64,
    pub blocks: Vec<ManuscriptClaimExtractionBlockInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimExtractionIdentity {
    pub provider: String,
    pub extractor_version: String,
    pub model_id: Option<String>,
    pub extraction_contract_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimExtractionClaimOutput {
    pub claim_text: String,
    pub source_start: u64,
    pub source_end: u64,
    pub citation_occurrence_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimExtractionUnassociatedCitation {
    pub citation_occurrence_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimExtractionOutput {
    pub claims: Vec<ManuscriptClaimExtractionClaimOutput>,
    #[serde(default)]
    pub unassociated_citations: Vec<ManuscriptClaimExtractionUnassociatedCitation>,
}

#[derive(Clone, Debug)]
pub struct ManuscriptClaimExtractionWrite {
    pub run: ManuscriptClaimExtractionRun,
    pub claims: Vec<ResearchClaim>,
    pub links: Vec<ClaimCitationLink>,
    pub items: Vec<ManuscriptClaimExtractionItem>,
    pub coverage: Vec<ManuscriptClaimExtractionCoverage>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimInventoryCitationInput {
    pub start: u64,
    pub end: u64,
    pub rendered_text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimInventoryBlockInput {
    pub block_id: String,
    pub block_ordinal: u32,
    pub block_kind: ManuscriptClaimInventoryBlockKind,
    pub text: String,
    #[serde(default)]
    pub citations: Vec<ManuscriptClaimInventoryCitationInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct StartManuscriptClaimInventory {
    pub research_case_id: ResearchCaseId,
    pub manuscript_source_id: ResearchSourceId,
    pub document_id: String,
    pub document_version: i64,
    pub blocks: Vec<ManuscriptClaimInventoryBlockInput>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimInventoryIdentity {
    pub provider: String,
    pub extractor_version: String,
    pub model_id: Option<String>,
    pub extraction_contract_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimInventoryClaimOutput {
    pub claim_text: String,
    pub source_start: u64,
    pub source_end: u64,
    pub review_kind: ClaimReviewKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ManuscriptClaimInventoryOutput {
    pub claims: Vec<ManuscriptClaimInventoryClaimOutput>,
}

#[derive(Clone, Debug)]
pub struct ManuscriptClaimInventoryWrite {
    pub run: ManuscriptClaimInventoryRun,
    pub items: Vec<ManuscriptClaimInventoryItem>,
    pub coverage: Vec<ManuscriptClaimInventoryCoverage>,
}

#[derive(Clone, Debug)]
pub struct CreateCitationTargetBinding {
    pub research_case_id: ResearchCaseId,
    pub citation_target_id: CitationTargetId,
    pub source_id: ResearchSourceId,
    pub source_snapshot_id: Option<ResearchSourceSnapshotId>,
    pub extraction_id: Option<ResearchPdfExtractionId>,
    pub method: CitationBindingMethod,
}

#[derive(Clone, Debug)]
pub struct CreateClaimCitationLink {
    pub research_case_id: ResearchCaseId,
    pub claim_id: ResearchClaimId,
    pub citation_occurrence_id: CitationOccurrenceId,
}

impl SourceOrigin {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::UploadedArtifact {
                artifact_id,
                revision_id,
            } => {
                safe_provenance_text("artifact_id", artifact_id)?;
                if let Some(revision_id) = revision_id {
                    safe_provenance_text("revision_id", revision_id)?;
                }
            }
            Self::ActiveDocumentSnapshot {
                document_id,
                document_version,
            } => {
                safe_provenance_text("document_id", document_id)?;
                safe_provenance_text("document_version", document_version)?;
            }
            Self::OfficeCliArtifactRevision {
                artifact_id,
                revision_id,
            } => {
                safe_provenance_text("artifact_id", artifact_id)?;
                safe_provenance_text("revision_id", revision_id)?;
            }
            Self::WebRetrieval {
                url,
                retrieved_at_ms,
            } => {
                safe_provenance_text("url", url)?;
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(ResearchError::Invalid(
                        "web origin URL must use http or https".to_owned(),
                    ));
                }
                let lowered = url.to_ascii_lowercase();
                if url.contains('@')
                    || [
                        "authorization",
                        "bearer ",
                        "cookie=",
                        "password=",
                        "secret=",
                        "token=",
                    ]
                    .iter()
                    .any(|marker| lowered.contains(marker))
                {
                    return Err(ResearchError::Invalid(
                        "web origin URL must not contain credentials or authorization".to_owned(),
                    ));
                }
                if *retrieved_at_ms <= 0 {
                    return Err(ResearchError::Invalid(
                        "web origin retrieval time must be positive".to_owned(),
                    ));
                }
            }
            Self::ExternalImport {
                provider,
                external_reference,
            } => {
                safe_provenance_text("provider", provider)?;
                safe_provenance_text("external_reference", external_reference)?;
            }
        }
        Ok(())
    }
}

impl ResearchSourceIdentityInput {
    pub fn validate(&self) -> Result<(), ResearchError> {
        bounded_text(
            "source identity provider",
            &self.provider,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        bounded_text(
            "source identity external reference",
            &self.external_reference,
            MAX_PROVENANCE_TEXT_BYTES,
        )?;
        Ok(())
    }
}

impl EvidenceLocator {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::TextRange { start, end } => validate_range(*start, *end)?,
            Self::Pdf { page, end_page } => {
                if *page == 0 || end_page.is_some_and(|end_page| end_page < *page) {
                    return Err(ResearchError::Invalid(
                        "PDF locator page range is invalid".to_owned(),
                    ));
                }
            }
            Self::PdfTextRange { page, start, end } => {
                if *page == 0 {
                    return Err(ResearchError::Invalid(
                        "PDF text locator page must be positive".to_owned(),
                    ));
                }
                validate_range(*start, *end)?;
            }
            Self::Manuscript {
                block_id,
                start,
                end,
            } => {
                bounded_text("block_id", block_id, MAX_PROVENANCE_TEXT_BYTES)?;
                if let (Some(start), Some(end)) = (start, end) {
                    validate_range(*start, *end)?;
                }
            }
            Self::Spreadsheet { sheet, range } => {
                bounded_text("sheet", sheet, MAX_PROVENANCE_TEXT_BYTES)?;
                bounded_text("range", range, MAX_PROVENANCE_TEXT_BYTES)?;
            }
            Self::Web {
                fragment,
                start,
                end,
            } => {
                if let Some(fragment) = fragment {
                    bounded_text("fragment", fragment, MAX_PROVENANCE_TEXT_BYTES)?;
                }
                if let (Some(start), Some(end)) = (start, end) {
                    validate_range(*start, *end)?;
                }
            }
            Self::Regulation {
                article,
                section,
                clause,
            } => {
                bounded_text("article", article, MAX_PROVENANCE_TEXT_BYTES)?;
                if let Some(section) = section {
                    bounded_text("section", section, MAX_PROVENANCE_TEXT_BYTES)?;
                }
                if let Some(clause) = clause {
                    bounded_text("clause", clause, MAX_PROVENANCE_TEXT_BYTES)?;
                }
            }
        }
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_LOCATOR_BYTES {
            return Err(ResearchError::Invalid(format!(
                "locator exceeds {MAX_LOCATOR_BYTES} bytes"
            )));
        }
        Ok(())
    }
}

impl ClaimOrigin {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::Manuscript {
                document_id,
                document_version,
                locator,
            } => {
                safe_provenance_text("document_id", document_id)?;
                safe_provenance_text("document_version", document_version)?;
                if let Some(locator) = locator {
                    locator.validate()?;
                }
            }
            Self::User | Self::Agent => {}
            Self::Imported { source } => safe_provenance_text("source", source)?,
        }
        Ok(())
    }
}

impl CitationOccurrenceOrigin {
    pub fn validate(&self) -> Result<(), ResearchError> {
        match self {
            Self::Manuscript {
                document_id,
                document_version,
                locator,
            } => {
                safe_provenance_text("document_id", document_id)?;
                safe_provenance_text("document_version", document_version)?;
                if let Some(locator) = locator {
                    locator.validate()?;
                }
            }
            Self::ManuscriptSnapshot {
                source_snapshot_id,
                locator,
            } => {
                if let Some(locator) = locator {
                    locator.validate()?;
                }
                let _ = source_snapshot_id;
            }
            Self::Imported { source } => safe_provenance_text("source", source)?,
        }
        Ok(())
    }
}

pub fn validate_metadata(metadata: &SafeMetadata) -> Result<(), ResearchError> {
    let bytes = serde_json::to_vec(metadata)?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(ResearchError::Invalid(format!(
            "metadata exceeds {MAX_METADATA_BYTES} bytes"
        )));
    }
    for (key, value) in metadata {
        bounded_text("metadata key", key, MAX_PROVENANCE_TEXT_BYTES)?;
        bounded_text("metadata value", value, MAX_PROVENANCE_TEXT_BYTES)?;
        let normalized = key.to_ascii_lowercase().replace(['-', ' '], "_");
        if [
            "authorization",
            "api_key",
            "apikey",
            "bearer",
            "cookie",
            "credential",
            "password",
            "secret",
            "session_token",
            "token",
        ]
        .iter()
        .any(|secret_key| normalized.contains(secret_key))
        {
            return Err(ResearchError::Invalid(format!(
                "metadata key is not allowed for provenance: {key}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn bounded_text(
    field: &str,
    value: &str,
    max_bytes: usize,
) -> Result<(), ResearchError> {
    if value.trim().is_empty() {
        return Err(ResearchError::Invalid(format!("{field} must not be empty")));
    }
    if value.len() > max_bytes {
        return Err(ResearchError::Invalid(format!(
            "{field} exceeds {max_bytes} bytes"
        )));
    }
    if value
        .chars()
        .any(|character| character == '\0' || character == '\u{7f}')
    {
        return Err(ResearchError::Invalid(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

fn validate_identity(field: &str, value: &str) -> Result<(), ResearchError> {
    bounded_text(field, value, MAX_PROVENANCE_TEXT_BYTES)?;
    if value == "."
        || value == ".."
        || value
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '/' | '\\' | ':'))
    {
        return Err(ResearchError::Invalid(format!("invalid {field}: {value}")));
    }
    Ok(())
}

fn safe_provenance_text(field: &str, value: &str) -> Result<(), ResearchError> {
    bounded_text(field, value, MAX_PROVENANCE_TEXT_BYTES)?;
    let lowered = value.to_ascii_lowercase();
    if [
        "authorization:",
        "api_key=",
        "apikey=",
        "bearer ",
        "cookie=",
        "password=",
        "secret=",
        "token=",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
    {
        return Err(ResearchError::Invalid(format!(
            "{field} contains secret-like transport data"
        )));
    }
    Ok(())
}

fn validate_range(start: u64, end: u64) -> Result<(), ResearchError> {
    if start > end {
        return Err(ResearchError::Invalid(
            "locator range start must not exceed end".to_owned(),
        ));
    }
    Ok(())
}
