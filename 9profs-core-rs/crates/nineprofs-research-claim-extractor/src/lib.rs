//! Stateless structured-model providers for bounded research extraction.
//!
//! Providers perform no retrieval and own no persistence. Regulation output is
//! explicitly a non-authoritative candidate interpretation of supplied PDF OCR.

use std::{fmt, time::Duration};

use async_trait::async_trait;
use nineprofs_research::{
    ClaimReviewKind, EvidenceLocator, ManuscriptClaimExtractionBlockInput,
    ManuscriptClaimExtractionClaimOutput, ManuscriptClaimExtractionIdentity,
    ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProvider,
    ManuscriptClaimExtractionProviderError, ManuscriptClaimExtractionUnassociatedCitation,
    ManuscriptClaimInventoryBlockInput, ManuscriptClaimInventoryClaimOutput,
    ManuscriptClaimInventoryIdentity, ManuscriptClaimInventoryOutput,
    ManuscriptClaimInventoryProvider, ManuscriptClaimInventoryProviderError,
    RegulationApplicability, RegulationRequirementCandidateExtractionIdentity,
    RegulationRequirementCandidateExtractionProvider,
    RegulationRequirementCandidateExtractionProviderError, RegulationRequirementCandidateOutput,
    RegulationRequirementExtractionInput,
};
use nineprofs_structured_model::{
    StructuredModelClient, StructuredModelConfig, StructuredModelConfigError,
    StructuredModelTransportError,
};
use serde::Deserialize;
use serde_json::{Value, json};
use thiserror::Error;

pub const CLAIM_EXTRACTION_IMPLEMENTATION_VERSION: &str = "model-manuscript-claim-extractor-v1";
pub const CLAIM_EXTRACTION_CONTRACT_VERSION: &str = "manuscript-claim-extractor-v1";
pub const CLAIM_EXTRACTION_INSTRUCTION: &str = r#"You are manuscript-claim-extractor-v1, a bounded scientific manuscript claim extractor.

Use ONLY the supplied manuscript block text. Do not use outside knowledge, add facts, correct scientific content, infer unstated results, retrieve sources, or assess whether any proposition is true. Decompose only factual or scientific propositions expressed in the supplied text.

An atomic claim is one independently verifiable proposition that can meaningfully be evaluated as one evidence question. Split independently testable propositions joined by and, but, or similar conjunctions. Preserve every material qualifier: negation, uncertainty, association versus causation, population, intervention, comparator, direction, magnitude, time, and statistical qualifier. A normalized proposition may restore a subject from the same block when the subject is explicit and recoverable; it must not add facts.

Every claim must select at least one supplied citationOccurrenceId. Select IDs only from this block. A citation may support multiple claims, and one claim may select multiple citations. Use Unicode scalar/code-point offsets into block text, not UTF-8 byte offsets or JavaScript UTF-16 offsets. sourceStart/sourceEnd must cover prose used for the proposition and must not include the citation marker atom. Do not return source excerpts, provenance fields, confidence, truth values, evidence IDs, source IDs, PDF IDs, or rationale.

If a supplied citation is not used with a clear verifiable proposition in this block, put it in unassociatedCitations with reason no_verifiable_claim. Return only the requested JSON object."#;

pub const CLAIM_INVENTORY_IMPLEMENTATION_VERSION: &str =
    "model-whole-manuscript-claim-inventory-v1";
pub const CLAIM_INVENTORY_CONTRACT_VERSION: &str = "whole-manuscript-claim-inventory-v1";
pub const REGULATION_REQUIREMENT_EXTRACTION_IMPLEMENTATION_VERSION: &str =
    "model-regulation-requirement-candidate-extractor-v1";
pub const REGULATION_REQUIREMENT_EXTRACTION_CONTRACT_VERSION: &str =
    "regulation-requirement-extraction-v0.1";
pub const REGULATION_REQUIREMENT_EXTRACTION_INSTRUCTION: &str = r#"You are regulation-requirement-extraction-v0.1, a bounded candidate extractor.

Use ONLY the supplied ResearchPdfExtraction page text, page context, document context, and allowed applicability vocabulary. Extract only requirement or policy propositions supported by the supplied OCR text. Do not add institutional policy from general knowledge, retrieve sources, resolve conflicts, or turn this output into authoritative RegulationRequirement data.

For each candidate, ocrExcerpt MUST be copied verbatim from the supplied page text. Do not repair spelling, punctuation, OCR errors, missing characters, or line breaks in ocrExcerpt. It must be a non-empty exact substring. The sourceLocator must identify a supplied page or contiguous supplied page range using PDF locators only. Do not fabricate character offsets for image-only pages. Use pdf_text_range only when offsets are supported by the supplied persisted text; otherwise use page-level pdf.

normalizedRequirement is a readable interpretation, not a repair. Do not infer missing values. Preserve modality and negation exactly: keep distinctions such as phải, không được, ít nhất, khoảng, nên, có thể, and hạn chế. Never change khoảng to exactly, nên or có thể to must, or hạn chế to prohibited. If không, không được, or không nên is damaged or ambiguous, do not guess; add a risk flag and review note. Preserve numeric constraints such as approximately 1/4, at least 50%, 1/6 - 1/5, 0,7, 1.5, and 3.5 cm. Keep must, must not, and may distinct when they appear in supplied text.

Split by independently meaningful authoritative proposition, not paragraph or bullet boundaries. Do not normalize advice such as Học viên có thể tham khảo into a mandatory requirement. Keep conditions such as missing website author, reference type, quote length, script, or keyboard availability, along with secondary-citation guidance and domain-specific citation style such as Vancouver, inside normalizedRequirement; never encode them as applicability facets.

Infer applicability only from supplied heading/text, supplied document context, and supplied vocabulary. Prefer exact supplied canonical identifiers. Use only context facets: language, research_families, artifact_types, academic_levels, study_designs, reporting_guidelines, and organization. If a category or value cannot be mapped to supplied vocabulary, leave applicability unset and add ambiguous_applicability or unresolved_applicability to riskFlags. Do not invent aliases or domain mappings.

authorityLocator is only a suggestion from explicit headings such as Phụ lục 3, §1.2, or Phụ lục 4 §3.1.1. Leave it null when unsupported. Keep applicability, trigger conditions, and future execution separate. Return only the requested JSON object."#;
pub const CLAIM_INVENTORY_INSTRUCTION: &str = r#"You are whole-manuscript-claim-inventory-v1, a bounded manuscript claim inventory extractor.

Read ONLY the supplied block text. Extract only propositions actually expressed in that text. Do not use outside knowledge, fact-check, rewrite the manuscript, add citations, infer missing results, or strengthen tentative language. This is an inventory, not a truth judgment or citation-sufficiency decision.

Return atomic claims when independently reviewable, but preserve every material qualifier: negation, modality, uncertainty, quantities, comparison direction, causal versus associative language, scope, population, intervention, comparator, and time conditions. Keep "may reduce" as "may reduce". The normalized claim must not add facts.

For every claim return Unicode scalar/code-point offsets into the supplied block text, not UTF-8 byte offsets or JavaScript UTF-16 offsets. Return no source excerpt: the trusted backend reconstructs it. A claim range must cover the prose expressing that proposition. Classify each proposition only as externalEvidence, manuscriptInternal, interpretive, nonEvidentiary, or uncertain. This classification is routing metadata, not a truth judgment and not a citation requirement decision.

Return zero claims for headings, transitions, acknowledgements, formatting text, or text without a research-relevant proposition. Return only the requested JSON object."#;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 1_024;
const MAX_REQUEST_BYTES: usize = 512 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct ClaimExtractorConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    /// Environment variable name only. The credential is resolved per call.
    pub api_key_env: String,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub max_output_tokens: u32,
}

impl fmt::Debug for ClaimExtractorConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaimExtractorConfig")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key_env", &self.api_key_env)
            .field("timeout", &self.timeout)
            .field("max_response_bytes", &self.max_response_bytes)
            .field("max_output_tokens", &self.max_output_tokens)
            .finish()
    }
}

impl Default for ClaimExtractorConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            model: String::new(),
            base_url: None,
            api_key_env: "OPENAI_API_KEY".to_owned(),
            timeout: DEFAULT_TIMEOUT,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
        }
    }
}

impl ClaimExtractorConfig {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
        api_key_env: impl Into<String>,
    ) -> Self {
        let provider = provider.into().trim().to_owned();
        let default_key_env = match provider.as_str() {
            "anthropic" => "ANTHROPIC_API_KEY",
            _ => "OPENAI_API_KEY",
        };
        let api_key_env = api_key_env.into();
        Self {
            provider: provider.clone(),
            model: model.into().trim().to_owned(),
            base_url: base_url
                .map(|value| value.trim().trim_end_matches('/').to_owned())
                .filter(|value| !value.is_empty()),
            api_key_env: if api_key_env.trim().is_empty() {
                default_key_env.to_owned()
            } else {
                api_key_env.trim().to_owned()
            },
            ..Self::default()
        }
    }

    pub fn from_env() -> Self {
        let provider = std::env::var("NINEPROFS_CLAIM_EXTRACTOR_PROVIDER").unwrap_or_default();
        let model = std::env::var("NINEPROFS_CLAIM_EXTRACTOR_MODEL").unwrap_or_default();
        let base_url = std::env::var("NINEPROFS_CLAIM_EXTRACTOR_BASE_URL").ok();
        let api_key_env = std::env::var("NINEPROFS_CLAIM_EXTRACTOR_API_KEY_ENV").unwrap_or_else(
            |_| match provider.trim() {
                "anthropic" => "ANTHROPIC_API_KEY".to_owned(),
                _ => "OPENAI_API_KEY".to_owned(),
            },
        );
        Self::new(provider, model, base_url, api_key_env)
    }

    fn credential(&self) -> Option<String> {
        (!self.api_key_env.trim().is_empty())
            .then(|| std::env::var(&self.api_key_env).ok())
            .flatten()
            .filter(|value| !value.is_empty())
    }

    fn validate(&self, credential: Option<&str>) -> Result<(), ClaimExtractorConfigError> {
        self.shared_config()
            .validate(credential)
            .map_err(map_config_error)?;
        if self.max_response_bytes > 4 * 1024 * 1024 || self.max_output_tokens > 16_384 {
            return Err(ClaimExtractorConfigError::InvalidLimits);
        }
        Ok(())
    }

    pub fn configuration_error(&self) -> Option<ClaimExtractorConfigError> {
        self.validate(self.credential().as_deref()).err()
    }

    pub fn readiness(&self) -> ClaimExtractorReadiness {
        let provider = (!self.provider.trim().is_empty()).then(|| self.provider.clone());
        let model = (!self.model.trim().is_empty()).then(|| self.model.clone());
        let status = if provider.is_none() && model.is_none() && self.base_url.is_none() {
            ClaimExtractorReadinessStatus::NotConfigured
        } else if let Some(error) = self.configuration_error() {
            ClaimExtractorReadinessStatus::InvalidConfiguration(error.to_string())
        } else {
            ClaimExtractorReadinessStatus::Ready
        };
        ClaimExtractorReadiness {
            provider,
            model,
            status,
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(
            self.readiness().status,
            ClaimExtractorReadinessStatus::Ready
        )
    }

    fn shared_config(&self) -> StructuredModelConfig {
        StructuredModelConfig {
            provider: self.provider.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            api_key_env: self.api_key_env.clone(),
            timeout: self.timeout,
            max_response_bytes: self.max_response_bytes,
            max_output_tokens: self.max_output_tokens,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClaimExtractorReadinessStatus {
    NotConfigured,
    InvalidConfiguration(String),
    Ready,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimExtractorReadiness {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub status: ClaimExtractorReadinessStatus,
}

#[derive(Debug, Error)]
pub enum ClaimExtractorConfigError {
    #[error("unsupported provider")]
    UnsupportedProvider,
    #[error("model is required")]
    MissingModel,
    #[error("credential environment variable is required")]
    MissingCredentialEnvironment,
    #[error("credential is not configured")]
    MissingCredential,
    #[error("base URL is invalid")]
    InvalidBaseUrl,
    #[error("claim extractor limits are invalid")]
    InvalidLimits,
}

#[derive(Clone)]
pub struct ModelClaimExtractionProvider {
    config: ClaimExtractorConfig,
    client: StructuredModelClient,
}

impl fmt::Debug for ModelClaimExtractionProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelClaimExtractionProvider")
            .field("config", &self.config)
            .finish()
    }
}

impl ModelClaimExtractionProvider {
    pub fn new(config: ClaimExtractorConfig) -> Self {
        let client = StructuredModelClient::new(config.shared_config());
        Self { config, client }
    }

    pub fn config(&self) -> &ClaimExtractorConfig {
        &self.config
    }

    fn request_body(
        &self,
        block: &ManuscriptClaimExtractionBlockInput,
    ) -> Result<Value, ManuscriptClaimExtractionProviderError> {
        let block_json = serde_json::to_string(block)
            .map_err(|_| ManuscriptClaimExtractionProviderError::MalformedResponse)?;
        let prompt = format!("Extract claims from this exact block JSON:\n{block_json}");
        let body = match self.config.provider.as_str() {
            "anthropic" => json!({
                "model": self.config.model,
                "max_tokens": self.config.max_output_tokens,
                "temperature": 0,
                "system": CLAIM_EXTRACTION_INSTRUCTION,
                "messages": [{"role": "user", "content": prompt}],
                "tools": [{
                    "name": "extract_manuscript_claims",
                    "description": "Return strict manuscript claim extraction output.",
                    "input_schema": extraction_schema()
                }],
                "tool_choice": {"type": "tool", "name": "extract_manuscript_claims"}
            }),
            _ => json!({
                "model": self.config.model,
                "temperature": 0,
                "max_tokens": self.config.max_output_tokens,
                "messages": [
                    {"role": "system", "content": CLAIM_EXTRACTION_INSTRUCTION},
                    {"role": "user", "content": prompt}
                ],
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "manuscript_claim_extraction",
                        "strict": true,
                        "schema": extraction_schema()
                    }
                }
            }),
        };
        if serde_json::to_vec(&body)
            .map(|bytes| bytes.len() > MAX_REQUEST_BYTES)
            .unwrap_or(true)
        {
            return Err(ManuscriptClaimExtractionProviderError::InvalidStructuredOutput);
        }
        Ok(body)
    }

    async fn request(
        &self,
        body: Value,
    ) -> Result<Vec<u8>, ManuscriptClaimExtractionProviderError> {
        let credential = self
            .config
            .credential()
            .ok_or(ManuscriptClaimExtractionProviderError::NotConfigured)?;
        self.client
            .execute_json(&body, &credential)
            .await
            .map_err(map_transport_error)
    }
}

#[async_trait]
impl ManuscriptClaimExtractionProvider for ModelClaimExtractionProvider {
    fn identity(&self) -> ManuscriptClaimExtractionIdentity {
        ManuscriptClaimExtractionIdentity {
            provider: self.config.provider.clone(),
            extractor_version: CLAIM_EXTRACTION_IMPLEMENTATION_VERSION.to_owned(),
            model_id: (!self.config.model.trim().is_empty()).then(|| self.config.model.clone()),
            extraction_contract_version: CLAIM_EXTRACTION_CONTRACT_VERSION.to_owned(),
        }
    }

    async fn extract(
        &self,
        block: ManuscriptClaimExtractionBlockInput,
    ) -> Result<ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProviderError> {
        if let Some(error) = self.config.configuration_error() {
            return if matches!(error, ClaimExtractorConfigError::MissingCredential) {
                Err(ManuscriptClaimExtractionProviderError::NotConfigured)
            } else {
                Err(ManuscriptClaimExtractionProviderError::InvalidConfiguration(error.to_string()))
            };
        }
        let response = self.request(self.request_body(&block)?).await?;
        parse_response(&self.config.provider, &response)
    }
}

#[derive(Clone)]
pub struct ModelWholeManuscriptClaimInventoryProvider {
    config: ClaimExtractorConfig,
    client: StructuredModelClient,
}

impl fmt::Debug for ModelWholeManuscriptClaimInventoryProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelWholeManuscriptClaimInventoryProvider")
            .field("config", &self.config)
            .finish()
    }
}

impl ModelWholeManuscriptClaimInventoryProvider {
    pub fn new(config: ClaimExtractorConfig) -> Self {
        let client = StructuredModelClient::new(config.shared_config());
        Self { config, client }
    }

    fn request_body(
        &self,
        block: &ManuscriptClaimInventoryBlockInput,
    ) -> Result<Value, ManuscriptClaimInventoryProviderError> {
        let block_json = serde_json::to_string(&json!({
            "blockKind": block.block_kind,
            "text": block.text,
        }))
        .map_err(|_| ManuscriptClaimInventoryProviderError::MalformedResponse)?;
        let prompt = format!("Extract claims from this exact manuscript block JSON:\n{block_json}");
        let body = match self.config.provider.as_str() {
            "anthropic" => json!({
                "model": self.config.model,
                "max_tokens": self.config.max_output_tokens,
                "temperature": 0,
                "system": CLAIM_INVENTORY_INSTRUCTION,
                "messages": [{"role": "user", "content": prompt}],
                "tools": [{
                    "name": "inventory_manuscript_claims",
                    "description": "Return strict whole-manuscript claim inventory output.",
                    "input_schema": inventory_schema()
                }],
                "tool_choice": {"type": "tool", "name": "inventory_manuscript_claims"}
            }),
            _ => json!({
                "model": self.config.model,
                "temperature": 0,
                "max_tokens": self.config.max_output_tokens,
                "messages": [
                    {"role": "system", "content": CLAIM_INVENTORY_INSTRUCTION},
                    {"role": "user", "content": prompt}
                ],
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "whole_manuscript_claim_inventory",
                        "strict": true,
                        "schema": inventory_schema()
                    }
                }
            }),
        };
        if serde_json::to_vec(&body)
            .map(|bytes| bytes.len() > MAX_REQUEST_BYTES)
            .unwrap_or(true)
        {
            return Err(ManuscriptClaimInventoryProviderError::InvalidStructuredOutput);
        }
        Ok(body)
    }

    async fn request(&self, body: Value) -> Result<Vec<u8>, ManuscriptClaimInventoryProviderError> {
        let credential = self
            .config
            .credential()
            .ok_or(ManuscriptClaimInventoryProviderError::NotConfigured)?;
        self.client
            .execute_json(&body, &credential)
            .await
            .map_err(map_inventory_transport_error)
    }
}

#[async_trait]
impl ManuscriptClaimInventoryProvider for ModelWholeManuscriptClaimInventoryProvider {
    fn identity(&self) -> ManuscriptClaimInventoryIdentity {
        ManuscriptClaimInventoryIdentity {
            provider: self.config.provider.clone(),
            extractor_version: CLAIM_INVENTORY_IMPLEMENTATION_VERSION.to_owned(),
            model_id: (!self.config.model.trim().is_empty()).then(|| self.config.model.clone()),
            extraction_contract_version: CLAIM_INVENTORY_CONTRACT_VERSION.to_owned(),
        }
    }

    async fn extract(
        &self,
        block: ManuscriptClaimInventoryBlockInput,
    ) -> Result<ManuscriptClaimInventoryOutput, ManuscriptClaimInventoryProviderError> {
        if let Some(error) = self.config.configuration_error() {
            return if matches!(error, ClaimExtractorConfigError::MissingCredential) {
                Err(ManuscriptClaimInventoryProviderError::NotConfigured)
            } else {
                Err(ManuscriptClaimInventoryProviderError::InvalidConfiguration(
                    error.to_string(),
                ))
            };
        }
        let response = self.request(self.request_body(&block)?).await?;
        parse_inventory_response(&self.config.provider, &response)
    }
}

fn map_inventory_transport_error(
    error: StructuredModelTransportError,
) -> ManuscriptClaimInventoryProviderError {
    match error {
        StructuredModelTransportError::NotConfigured => {
            ManuscriptClaimInventoryProviderError::NotConfigured
        }
        StructuredModelTransportError::InvalidConfiguration
        | StructuredModelTransportError::ClientBuildFailed => {
            ManuscriptClaimInventoryProviderError::InvalidConfiguration(
                "claim inventory extractor configuration is invalid".to_owned(),
            )
        }
        StructuredModelTransportError::Timeout => ManuscriptClaimInventoryProviderError::Timeout,
        StructuredModelTransportError::ResponseTooLarge => {
            ManuscriptClaimInventoryProviderError::ResponseTooLarge
        }
        StructuredModelTransportError::Unauthorized
        | StructuredModelTransportError::RateLimited
        | StructuredModelTransportError::ProviderUnavailable
        | StructuredModelTransportError::Transport => {
            ManuscriptClaimInventoryProviderError::Transport
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireInventoryExtraction {
    claims: Vec<WireInventoryClaim>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireInventoryClaim {
    claim_text: String,
    source_start: u64,
    source_end: u64,
    review_kind: ClaimReviewKind,
}

fn parse_inventory_response(
    provider: &str,
    bytes: &[u8],
) -> Result<ManuscriptClaimInventoryOutput, ManuscriptClaimInventoryProviderError> {
    let root: Value = serde_json::from_slice(bytes)
        .map_err(|_| ManuscriptClaimInventoryProviderError::MalformedResponse)?;
    let output = match provider {
        "anthropic" => root
            .get("content")
            .and_then(Value::as_array)
            .and_then(|content| {
                content.iter().find_map(|item| {
                    (item.get("type")?.as_str()? == "tool_use").then(|| item.get("input"))
                })
            })
            .flatten()
            .cloned()
            .ok_or(ManuscriptClaimInventoryProviderError::MalformedResponse)?,
        _ => {
            let content = root
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .ok_or(ManuscriptClaimInventoryProviderError::MalformedResponse)?;
            serde_json::from_str(content)
                .map_err(|_| ManuscriptClaimInventoryProviderError::MalformedResponse)?
        }
    };
    let wire: WireInventoryExtraction = serde_json::from_value(output)
        .map_err(|_| ManuscriptClaimInventoryProviderError::InvalidStructuredOutput)?;
    Ok(ManuscriptClaimInventoryOutput {
        claims: wire
            .claims
            .into_iter()
            .map(|claim| ManuscriptClaimInventoryClaimOutput {
                claim_text: claim.claim_text,
                source_start: claim.source_start,
                source_end: claim.source_end,
                review_kind: claim.review_kind,
            })
            .collect(),
    })
}

fn inventory_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "claims": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "claimText": {"type": "string"},
                        "sourceStart": {"type": "integer", "minimum": 0},
                        "sourceEnd": {"type": "integer", "minimum": 1},
                        "reviewKind": {"type": "string", "enum": [
                            "external_evidence", "manuscript_internal", "interpretive",
                            "non_evidentiary", "uncertain"
                        ]}
                    },
                    "required": ["claimText", "sourceStart", "sourceEnd", "reviewKind"]
                }
            }
        },
        "required": ["claims"]
    })
}

fn map_config_error(error: StructuredModelConfigError) -> ClaimExtractorConfigError {
    match error {
        StructuredModelConfigError::MissingProvider
        | StructuredModelConfigError::UnsupportedProvider => {
            ClaimExtractorConfigError::UnsupportedProvider
        }
        StructuredModelConfigError::MissingModel => ClaimExtractorConfigError::MissingModel,
        StructuredModelConfigError::MissingCredentialEnvironment => {
            ClaimExtractorConfigError::MissingCredentialEnvironment
        }
        StructuredModelConfigError::MissingCredential => {
            ClaimExtractorConfigError::MissingCredential
        }
        StructuredModelConfigError::InvalidBaseUrl => ClaimExtractorConfigError::InvalidBaseUrl,
        StructuredModelConfigError::InvalidLimits => ClaimExtractorConfigError::InvalidLimits,
    }
}

fn map_transport_error(
    error: StructuredModelTransportError,
) -> ManuscriptClaimExtractionProviderError {
    match error {
        StructuredModelTransportError::NotConfigured => {
            ManuscriptClaimExtractionProviderError::NotConfigured
        }
        StructuredModelTransportError::InvalidConfiguration
        | StructuredModelTransportError::ClientBuildFailed => {
            ManuscriptClaimExtractionProviderError::InvalidConfiguration(
                "claim extractor configuration is invalid".to_owned(),
            )
        }
        StructuredModelTransportError::Timeout => ManuscriptClaimExtractionProviderError::Timeout,
        StructuredModelTransportError::ResponseTooLarge => {
            ManuscriptClaimExtractionProviderError::ResponseTooLarge
        }
        StructuredModelTransportError::Unauthorized
        | StructuredModelTransportError::RateLimited
        | StructuredModelTransportError::ProviderUnavailable
        | StructuredModelTransportError::Transport => {
            ManuscriptClaimExtractionProviderError::Transport
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireExtraction {
    claims: Vec<WireClaim>,
    #[serde(default)]
    unassociated_citations: Vec<WireUnassociatedCitation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireClaim {
    claim_text: String,
    source_start: u64,
    source_end: u64,
    citation_occurrence_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireUnassociatedCitation {
    citation_occurrence_id: String,
    reason: String,
}

fn parse_response(
    provider: &str,
    bytes: &[u8],
) -> Result<ManuscriptClaimExtractionOutput, ManuscriptClaimExtractionProviderError> {
    let root: Value = serde_json::from_slice(bytes)
        .map_err(|_| ManuscriptClaimExtractionProviderError::MalformedResponse)?;
    let output = match provider {
        "anthropic" => root
            .get("content")
            .and_then(Value::as_array)
            .and_then(|content| {
                content.iter().find_map(|item| {
                    (item.get("type")?.as_str()? == "tool_use").then(|| item.get("input"))
                })
            })
            .flatten()
            .cloned()
            .ok_or(ManuscriptClaimExtractionProviderError::MalformedResponse)?,
        _ => {
            let content = root
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .ok_or(ManuscriptClaimExtractionProviderError::MalformedResponse)?;
            serde_json::from_str(content)
                .map_err(|_| ManuscriptClaimExtractionProviderError::MalformedResponse)?
        }
    };
    let wire: WireExtraction = serde_json::from_value(output)
        .map_err(|_| ManuscriptClaimExtractionProviderError::InvalidStructuredOutput)?;
    Ok(ManuscriptClaimExtractionOutput {
        claims: wire
            .claims
            .into_iter()
            .map(|claim| ManuscriptClaimExtractionClaimOutput {
                claim_text: claim.claim_text,
                source_start: claim.source_start,
                source_end: claim.source_end,
                citation_occurrence_ids: claim.citation_occurrence_ids,
            })
            .collect(),
        unassociated_citations: wire
            .unassociated_citations
            .into_iter()
            .map(|citation| ManuscriptClaimExtractionUnassociatedCitation {
                citation_occurrence_id: citation.citation_occurrence_id,
                reason: citation.reason,
            })
            .collect(),
    })
}

fn extraction_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "claims": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "claimText": {"type": "string"},
                        "sourceStart": {"type": "integer", "minimum": 0},
                        "sourceEnd": {"type": "integer", "minimum": 1},
                        "citationOccurrenceIds": {"type": "array", "items": {"type": "string"}, "minItems": 1}
                    },
                    "required": ["claimText", "sourceStart", "sourceEnd", "citationOccurrenceIds"]
                }
            },
            "unassociatedCitations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "citationOccurrenceId": {"type": "string"},
                        "reason": {"type": "string"}
                    },
                    "required": ["citationOccurrenceId", "reason"]
                }
            }
        },
        "required": ["claims", "unassociatedCitations"]
    })
}

#[derive(Clone, PartialEq, Eq)]
pub struct RegulationRequirementCandidateExtractorConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key_env: String,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub max_output_tokens: u32,
}

impl fmt::Debug for RegulationRequirementCandidateExtractorConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegulationRequirementCandidateExtractorConfig")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key_env", &self.api_key_env)
            .field("timeout", &self.timeout)
            .field("max_response_bytes", &self.max_response_bytes)
            .field("max_output_tokens", &self.max_output_tokens)
            .finish()
    }
}

impl Default for RegulationRequirementCandidateExtractorConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            model: String::new(),
            base_url: None,
            api_key_env: "OPENAI_API_KEY".to_owned(),
            timeout: DEFAULT_TIMEOUT,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
        }
    }
}

impl RegulationRequirementCandidateExtractorConfig {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        base_url: Option<String>,
        api_key_env: impl Into<String>,
    ) -> Self {
        let provider = provider.into().trim().to_owned();
        let api_key_env = api_key_env.into();
        Self {
            provider: provider.clone(),
            model: model.into().trim().to_owned(),
            base_url: base_url
                .map(|value| value.trim().trim_end_matches('/').to_owned())
                .filter(|value| !value.is_empty()),
            api_key_env: if api_key_env.trim().is_empty() {
                match provider.as_str() {
                    "anthropic" => "ANTHROPIC_API_KEY".to_owned(),
                    _ => "OPENAI_API_KEY".to_owned(),
                }
            } else {
                api_key_env.trim().to_owned()
            },
            ..Self::default()
        }
    }

    pub fn from_env() -> Self {
        let provider = std::env::var("NINEPROFS_REGULATION_REQUIREMENT_EXTRACTOR_PROVIDER")
            .unwrap_or_default();
        let model =
            std::env::var("NINEPROFS_REGULATION_REQUIREMENT_EXTRACTOR_MODEL").unwrap_or_default();
        let base_url = std::env::var("NINEPROFS_REGULATION_REQUIREMENT_EXTRACTOR_BASE_URL").ok();
        let api_key_env = std::env::var("NINEPROFS_REGULATION_REQUIREMENT_EXTRACTOR_API_KEY_ENV")
            .unwrap_or_default();
        Self::new(provider, model, base_url, api_key_env)
    }

    fn credential(&self) -> Option<String> {
        std::env::var(&self.api_key_env)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }

    fn configuration_error(&self) -> Option<RegulationRequirementCandidateExtractorConfigError> {
        self.shared_config()
            .validate(self.credential().as_deref())
            .err()
            .map(map_regulation_config_error)
    }

    fn shared_config(&self) -> StructuredModelConfig {
        StructuredModelConfig {
            provider: self.provider.clone(),
            model: self.model.clone(),
            base_url: self.base_url.clone(),
            api_key_env: self.api_key_env.clone(),
            timeout: self.timeout,
            max_response_bytes: self.max_response_bytes,
            max_output_tokens: self.max_output_tokens,
        }
    }
}

#[derive(Debug, Error)]
pub enum RegulationRequirementCandidateExtractorConfigError {
    #[error("unsupported provider")]
    UnsupportedProvider,
    #[error("model is required")]
    MissingModel,
    #[error("credential environment variable is required")]
    MissingCredentialEnvironment,
    #[error("credential is not configured")]
    MissingCredential,
    #[error("base URL is invalid")]
    InvalidBaseUrl,
    #[error("regulation requirement extractor limits are invalid")]
    InvalidLimits,
}

#[derive(Clone)]
pub struct ModelRegulationRequirementCandidateExtractor {
    config: RegulationRequirementCandidateExtractorConfig,
    client: StructuredModelClient,
}

impl fmt::Debug for ModelRegulationRequirementCandidateExtractor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelRegulationRequirementCandidateExtractor")
            .field("config", &self.config)
            .finish()
    }
}

impl ModelRegulationRequirementCandidateExtractor {
    pub fn new(config: RegulationRequirementCandidateExtractorConfig) -> Self {
        let client = StructuredModelClient::new(config.shared_config());
        Self { config, client }
    }

    pub fn config(&self) -> &RegulationRequirementCandidateExtractorConfig {
        &self.config
    }

    fn request_body(
        &self,
        input: &RegulationRequirementExtractionInput,
    ) -> Result<Value, RegulationRequirementCandidateExtractionProviderError> {
        input
            .validate()
            .map_err(|_| RegulationRequirementCandidateExtractionProviderError::InvalidInput)?;
        let input_json = serde_json::to_string(input)
            .map_err(|_| RegulationRequirementCandidateExtractionProviderError::InvalidInput)?;
        let prompt = format!(
            "Extract regulation requirement candidates from this exact bounded PDF extraction JSON:\n{input_json}"
        );
        let body = match self.config.provider.as_str() {
            "anthropic" => json!({
                "model": self.config.model,
                "max_tokens": self.config.max_output_tokens,
                "temperature": 0,
                "system": REGULATION_REQUIREMENT_EXTRACTION_INSTRUCTION,
                "messages": [{"role": "user", "content": prompt}],
                "tools": [{
                    "name": "regulation_requirement_extraction",
                    "description": "Return strict non-authoritative regulation requirement candidates.",
                    "input_schema": regulation_requirement_extraction_schema()
                }],
                "tool_choice": {"type": "tool", "name": "regulation_requirement_extraction"}
            }),
            _ => json!({
                "model": self.config.model,
                "temperature": 0,
                "max_tokens": self.config.max_output_tokens,
                "messages": [
                    {"role": "system", "content": REGULATION_REQUIREMENT_EXTRACTION_INSTRUCTION},
                    {"role": "user", "content": prompt}
                ],
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "regulation_requirement_extraction",
                        "strict": true,
                        "schema": regulation_requirement_extraction_schema()
                    }
                }
            }),
        };
        if serde_json::to_vec(&body)
            .map(|bytes| bytes.len() > MAX_REQUEST_BYTES)
            .unwrap_or(true)
        {
            return Err(RegulationRequirementCandidateExtractionProviderError::InvalidInput);
        }
        Ok(body)
    }
}

#[async_trait]
impl RegulationRequirementCandidateExtractionProvider
    for ModelRegulationRequirementCandidateExtractor
{
    fn identity(&self) -> RegulationRequirementCandidateExtractionIdentity {
        RegulationRequirementCandidateExtractionIdentity {
            provider: self.config.provider.clone(),
            extractor_version: REGULATION_REQUIREMENT_EXTRACTION_IMPLEMENTATION_VERSION.to_owned(),
            model_id: (!self.config.model.trim().is_empty()).then(|| self.config.model.clone()),
            extraction_contract_version: REGULATION_REQUIREMENT_EXTRACTION_CONTRACT_VERSION
                .to_owned(),
        }
    }

    async fn extract(
        &self,
        input: RegulationRequirementExtractionInput,
    ) -> Result<
        Vec<RegulationRequirementCandidateOutput>,
        RegulationRequirementCandidateExtractionProviderError,
    > {
        if let Some(error) = self.config.configuration_error() {
            return if matches!(
                error,
                RegulationRequirementCandidateExtractorConfigError::MissingCredential
            ) {
                Err(RegulationRequirementCandidateExtractionProviderError::NotConfigured)
            } else {
                Err(
                    RegulationRequirementCandidateExtractionProviderError::InvalidConfiguration(
                        error.to_string(),
                    ),
                )
            };
        }
        let body = self.request_body(&input)?;
        let credential = self
            .config
            .credential()
            .ok_or(RegulationRequirementCandidateExtractionProviderError::NotConfigured)?;
        let response = self
            .client
            .execute_json(&body, &credential)
            .await
            .map_err(map_regulation_transport_error)?;
        parse_regulation_response(&self.config.provider, &response)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireRegulationExtraction {
    candidates: Vec<WireRegulationCandidate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireRegulationCandidate {
    ocr_excerpt: String,
    normalized_requirement: String,
    source_locator: WirePdfLocator,
    authority_locator: Option<WireAuthorityLocator>,
    applicability: RegulationApplicability,
    risk_flags: Vec<String>,
    review_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WirePdfLocator {
    kind: String,
    page: u32,
    end_page: Option<u32>,
    start: Option<u64>,
    end: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireAuthorityLocator {
    article: String,
    section: Option<String>,
    clause: Option<String>,
}

fn parse_regulation_response(
    provider: &str,
    bytes: &[u8],
) -> Result<
    Vec<RegulationRequirementCandidateOutput>,
    RegulationRequirementCandidateExtractionProviderError,
> {
    let root: Value = serde_json::from_slice(bytes)
        .map_err(|_| RegulationRequirementCandidateExtractionProviderError::MalformedResponse)?;
    let output = match provider {
        "anthropic" => root
            .get("content")
            .and_then(Value::as_array)
            .and_then(|content| {
                content.iter().find_map(|item| {
                    (item.get("type")?.as_str()? == "tool_use"
                        && item.get("name")?.as_str()? == "regulation_requirement_extraction")
                        .then(|| item.get("input"))
                })
            })
            .flatten()
            .cloned()
            .ok_or(RegulationRequirementCandidateExtractionProviderError::MalformedResponse)?,
        _ => {
            let content = root
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
                .ok_or(RegulationRequirementCandidateExtractionProviderError::MalformedResponse)?;
            serde_json::from_str(content).map_err(|_| {
                RegulationRequirementCandidateExtractionProviderError::InvalidStructuredOutput
            })?
        }
    };
    let wire: WireRegulationExtraction = serde_json::from_value(output).map_err(|_| {
        RegulationRequirementCandidateExtractionProviderError::InvalidStructuredOutput
    })?;
    wire.candidates
        .into_iter()
        .map(map_wire_regulation_candidate)
        .collect()
}

fn map_wire_regulation_candidate(
    candidate: WireRegulationCandidate,
) -> Result<
    RegulationRequirementCandidateOutput,
    RegulationRequirementCandidateExtractionProviderError,
> {
    let source_locator = match candidate.source_locator.kind.as_str() {
        "pdf"
            if candidate.source_locator.start.is_none()
                && candidate.source_locator.end.is_none() =>
        {
            EvidenceLocator::Pdf {
                page: candidate.source_locator.page,
                end_page: candidate.source_locator.end_page,
            }
        }
        "pdf_text_range"
            if candidate.source_locator.end_page.is_none()
                && candidate.source_locator.start.is_some()
                && candidate.source_locator.end.is_some() =>
        {
            EvidenceLocator::PdfTextRange {
                page: candidate.source_locator.page,
                start: candidate.source_locator.start.expect("checked start"),
                end: candidate.source_locator.end.expect("checked end"),
            }
        }
        _ => {
            return Err(
                RegulationRequirementCandidateExtractionProviderError::InvalidStructuredOutput,
            );
        }
    };
    let authority_locator =
        candidate
            .authority_locator
            .map(|locator| EvidenceLocator::Regulation {
                article: locator.article,
                section: locator.section,
                clause: locator.clause,
            });
    Ok(RegulationRequirementCandidateOutput {
        ocr_excerpt: candidate.ocr_excerpt,
        normalized_requirement: candidate.normalized_requirement,
        source_locator,
        authority_locator,
        applicability: candidate.applicability,
        risk_flags: candidate.risk_flags,
        review_notes: candidate.review_notes,
    })
}

fn regulation_requirement_extraction_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["candidates"],
        "properties": {
            "candidates": {
                "type": "array",
                "maxItems": 256,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": [
                        "ocrExcerpt", "normalizedRequirement", "sourceLocator",
                        "authorityLocator", "applicability", "riskFlags", "reviewNotes"
                    ],
                    "properties": {
                        "ocrExcerpt": {"type": "string", "minLength": 1},
                        "normalizedRequirement": {"type": "string", "minLength": 1},
                        "sourceLocator": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": ["kind", "page", "endPage", "start", "end"],
                            "properties": {
                                "kind": {"type": "string", "enum": ["pdf", "pdf_text_range"]},
                                "page": {"type": "integer", "minimum": 1},
                                "endPage": {"anyOf": [{"type": "integer", "minimum": 1}, {"type": "null"}]},
                                "start": {"anyOf": [{"type": "integer", "minimum": 0}, {"type": "null"}]},
                                "end": {"anyOf": [{"type": "integer", "minimum": 1}, {"type": "null"}]}
                            }
                        },
                        "authorityLocator": {
                            "anyOf": [
                                {"type": "null"},
                                {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["article", "section", "clause"],
                                    "properties": {
                                        "article": {"type": "string"},
                                        "section": {"anyOf": [{"type": "string"}, {"type": "null"}]},
                                        "clause": {"anyOf": [{"type": "string"}, {"type": "null"}]}
                                    }
                                }
                            ]
                        },
                        "applicability": {
                            "type": "object",
                            "additionalProperties": false,
                            "required": [
                                "language", "research_families", "artifact_types",
                                "academic_levels", "study_designs", "reporting_guidelines",
                                "organization"
                            ],
                            "properties": {
                                "language": {"type": "array", "items": {"type": "string"}},
                                "research_families": {"type": "array", "items": {"type": "string"}},
                                "artifact_types": {"type": "array", "items": {"type": "string"}},
                                "academic_levels": {"type": "array", "items": {"type": "string"}},
                                "study_designs": {"type": "array", "items": {"type": "string"}},
                                "reporting_guidelines": {"type": "array", "items": {"type": "string"}},
                                "organization": {"type": "array", "items": {"type": "string"}}
                            }
                        },
                        "riskFlags": {"type": "array", "items": {"type": "string"}},
                        "reviewNotes": {"anyOf": [{"type": "string"}, {"type": "null"}]}
                    }
                }
            }
        }
    })
}

fn map_regulation_config_error(
    error: StructuredModelConfigError,
) -> RegulationRequirementCandidateExtractorConfigError {
    match error {
        StructuredModelConfigError::MissingProvider
        | StructuredModelConfigError::UnsupportedProvider => {
            RegulationRequirementCandidateExtractorConfigError::UnsupportedProvider
        }
        StructuredModelConfigError::MissingModel => {
            RegulationRequirementCandidateExtractorConfigError::MissingModel
        }
        StructuredModelConfigError::MissingCredentialEnvironment => {
            RegulationRequirementCandidateExtractorConfigError::MissingCredentialEnvironment
        }
        StructuredModelConfigError::MissingCredential => {
            RegulationRequirementCandidateExtractorConfigError::MissingCredential
        }
        StructuredModelConfigError::InvalidBaseUrl => {
            RegulationRequirementCandidateExtractorConfigError::InvalidBaseUrl
        }
        StructuredModelConfigError::InvalidLimits => {
            RegulationRequirementCandidateExtractorConfigError::InvalidLimits
        }
    }
}

fn map_regulation_transport_error(
    error: StructuredModelTransportError,
) -> RegulationRequirementCandidateExtractionProviderError {
    match error {
        StructuredModelTransportError::NotConfigured => {
            RegulationRequirementCandidateExtractionProviderError::NotConfigured
        }
        StructuredModelTransportError::InvalidConfiguration
        | StructuredModelTransportError::ClientBuildFailed => {
            RegulationRequirementCandidateExtractionProviderError::InvalidConfiguration(
                "regulation requirement extractor configuration is invalid".to_owned(),
            )
        }
        StructuredModelTransportError::Timeout => {
            RegulationRequirementCandidateExtractionProviderError::Timeout
        }
        StructuredModelTransportError::ResponseTooLarge => {
            RegulationRequirementCandidateExtractionProviderError::ResponseTooLarge
        }
        StructuredModelTransportError::Unauthorized
        | StructuredModelTransportError::RateLimited
        | StructuredModelTransportError::ProviderUnavailable
        | StructuredModelTransportError::Transport => {
            RegulationRequirementCandidateExtractionProviderError::Transport
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nineprofs_research::RegulationRequirementExtractionPage;
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex, Once},
        time::Duration,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    const TEST_KEY_ENV: &str = "NINEPROFS_CLAIM_EXTRACTOR_TEST_KEY";
    const TEST_KEY: &str = "claim-extractor-test-secret";
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn install_test_key() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| unsafe {
            std::env::set_var(TEST_KEY_ENV, TEST_KEY);
        });
    }

    fn block() -> ManuscriptClaimExtractionBlockInput {
        ManuscriptClaimExtractionBlockInput {
            block_id: "block-1".to_owned(),
            text: "Treatment improved outcomes.".to_owned(),
            citations: Vec::new(),
        }
    }

    fn config(provider: &str, base_url: String) -> ClaimExtractorConfig {
        install_test_key();
        ClaimExtractorConfig::new(provider, "test-model", Some(base_url), TEST_KEY_ENV)
    }

    struct MockServer {
        url: String,
        request: Arc<Mutex<Option<String>>>,
        task: tokio::task::JoinHandle<()>,
    }

    async fn mock_server(body: String) -> MockServer {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let request = Arc::new(Mutex::new(None));
        let request_for_task = Arc::clone(&request);
        let task = tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut received = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let Ok(count) = stream.read(&mut chunk).await else {
                    return;
                };
                if count == 0 {
                    return;
                }
                received.extend_from_slice(&chunk[..count]);
                let Some(header_end) = received.windows(4).position(|value| value == b"\r\n\r\n")
                else {
                    continue;
                };
                let content_length = String::from_utf8_lossy(&received[..header_end])
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length:")
                            .or_else(|| line.strip_prefix("content-length:"))
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or_default();
                if received.len() >= header_end + 4 + content_length {
                    break;
                }
            }
            *request_for_task.lock().unwrap() =
                Some(String::from_utf8_lossy(&received).into_owned());
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
        MockServer {
            url: format!("http://{address}/custom/v1"),
            request,
            task,
        }
    }

    #[test]
    fn default_configuration_is_not_configured() {
        assert!(matches!(
            ClaimExtractorConfig::default().readiness().status,
            ClaimExtractorReadinessStatus::NotConfigured
        ));
    }

    #[test]
    fn output_rejects_unknown_fields() {
        let response = br#"{"choices":[{"message":{"content":"{\"claims\":[],\"unassociatedCitations\":[],\"sourceExcerpt\":\"bad\"}"}}]}"#;
        assert!(matches!(
            parse_response("openai", response),
            Err(ManuscriptClaimExtractionProviderError::InvalidStructuredOutput)
        ));
    }

    #[test]
    fn configuration_validation_preserves_provider_and_limit_errors() {
        assert!(matches!(
            ClaimExtractorConfig::default().configuration_error(),
            Some(ClaimExtractorConfigError::UnsupportedProvider)
        ));
        assert_eq!(
            ClaimExtractorConfig::new("openai", "", None, TEST_KEY_ENV)
                .configuration_error()
                .map(|error| error.to_string()),
            Some("model is required".to_owned())
        );
        assert_eq!(
            ClaimExtractorConfig::new("openai", "test-model", None, "MISSING_KEY")
                .configuration_error()
                .map(|error| error.to_string()),
            Some("credential is not configured".to_owned())
        );
        install_test_key();
        assert_eq!(
            ClaimExtractorConfig::new(
                "openai",
                "test-model",
                Some("not-a-url".to_owned()),
                TEST_KEY_ENV
            )
            .configuration_error()
            .map(|error| error.to_string()),
            Some("base URL is invalid".to_owned())
        );
        let mut invalid_limits =
            ClaimExtractorConfig::new("openai", "test-model", None, TEST_KEY_ENV);
        invalid_limits.timeout = Duration::ZERO;
        assert!(matches!(
            invalid_limits.configuration_error(),
            Some(ClaimExtractorConfigError::InvalidLimits)
        ));
        invalid_limits.timeout = Duration::from_secs(1);
        invalid_limits.max_response_bytes = 0;
        assert!(matches!(
            invalid_limits.configuration_error(),
            Some(ClaimExtractorConfigError::InvalidLimits)
        ));
    }

    #[test]
    fn provider_defaults_choose_provider_specific_key_names() {
        assert_eq!(
            ClaimExtractorConfig::new("openai", "model", None, "").api_key_env,
            "OPENAI_API_KEY"
        );
        assert_eq!(
            ClaimExtractorConfig::new("anthropic", "model", None, "").api_key_env,
            "ANTHROPIC_API_KEY"
        );
    }

    #[test]
    fn from_env_uses_only_claim_extractor_prefix() {
        let _guard = ENV_LOCK.lock().unwrap();
        let names = [
            "NINEPROFS_CLAIM_EXTRACTOR_PROVIDER",
            "NINEPROFS_CLAIM_EXTRACTOR_MODEL",
            "NINEPROFS_CLAIM_EXTRACTOR_BASE_URL",
            "NINEPROFS_CLAIM_EXTRACTOR_API_KEY_ENV",
            "NINEPROFS_CITATION_ASSESSOR_PROVIDER",
            "NINEPROFS_CITATION_ASSESSOR_MODEL",
        ];
        let previous = names
            .iter()
            .map(|name| (*name, std::env::var(name).ok()))
            .collect::<Vec<_>>();
        unsafe {
            std::env::set_var("NINEPROFS_CLAIM_EXTRACTOR_PROVIDER", "anthropic");
            std::env::set_var("NINEPROFS_CLAIM_EXTRACTOR_MODEL", "claim-model");
            std::env::remove_var("NINEPROFS_CLAIM_EXTRACTOR_BASE_URL");
            std::env::set_var("NINEPROFS_CLAIM_EXTRACTOR_API_KEY_ENV", TEST_KEY_ENV);
            std::env::set_var("NINEPROFS_CITATION_ASSESSOR_PROVIDER", "openai");
            std::env::set_var("NINEPROFS_CITATION_ASSESSOR_MODEL", "citation-model");
        }
        let config = ClaimExtractorConfig::from_env();
        for (name, value) in previous {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.model, "claim-model");
        assert_eq!(config.api_key_env, TEST_KEY_ENV);
    }

    #[tokio::test]
    async fn openai_contract_uses_shared_transport_and_task_schema() {
        let response = r#"{"choices":[{"message":{"content":"{\"claims\":[],\"unassociatedCitations\":[]}"}}]}"#;
        let server = mock_server(response.to_owned()).await;
        let provider = ModelClaimExtractionProvider::new(config("openai", server.url.clone()));
        let result = provider.extract(block()).await.unwrap();
        assert!(result.claims.is_empty());
        server.task.await.unwrap();
        let request = server.request.lock().unwrap().clone().unwrap();
        let request_lower = request.to_ascii_lowercase();
        assert!(request_lower.starts_with("post /custom/v1/chat/completions "));
        assert!(request_lower.contains("authorization: bearer "));
        assert!(request.contains("\"model\":\"test-model\""));
        assert!(request.contains("\"json_schema\""));
        assert!(request.contains("Use ONLY the supplied manuscript block text"));
    }

    #[tokio::test]
    async fn anthropic_contract_uses_shared_transport_and_task_tool() {
        let response = r#"{"content":[{"type":"tool_use","name":"extract_manuscript_claims","input":{"claims":[],"unassociatedCitations":[]}}]}"#;
        let server = mock_server(response.to_owned()).await;
        let provider = ModelClaimExtractionProvider::new(config("anthropic", server.url.clone()));
        let result = provider.extract(block()).await.unwrap();
        assert!(result.claims.is_empty());
        server.task.await.unwrap();
        let request = server.request.lock().unwrap().clone().unwrap();
        let request_lower = request.to_ascii_lowercase();
        assert!(request_lower.starts_with("post /custom/v1/messages "));
        assert!(request_lower.contains("x-api-key: "));
        assert!(request_lower.contains("anthropic-version: 2023-06-01"));
        assert!(!request_lower.contains("authorization:"));
        assert!(request.contains("\"model\":\"test-model\""));
        assert!(request.contains("\"name\":\"extract_manuscript_claims\""));
        assert!(request.contains("Use ONLY the supplied manuscript block text"));
    }

    #[tokio::test]
    async fn response_size_bound_maps_to_existing_claim_error() {
        let response = r#"{"choices":[{"message":{"content":"{}"}}]}"#;
        let server = mock_server(response.to_owned()).await;
        let mut extractor_config = config("openai", server.url.clone());
        extractor_config.max_response_bytes = 8;
        let provider = ModelClaimExtractionProvider::new(extractor_config);
        assert!(matches!(
            provider.extract(block()).await,
            Err(ManuscriptClaimExtractionProviderError::ResponseTooLarge)
        ));
        server.task.await.unwrap();
    }

    fn regulation_input() -> RegulationRequirementExtractionInput {
        RegulationRequirementExtractionInput {
            source_id: nineprofs_research::ResearchSourceId::new(),
            source_snapshot_id: nineprofs_research::ResearchSourceSnapshotId::new(),
            pdf_extraction_id: nineprofs_research::ResearchPdfExtractionId::new(),
            start_page: 2,
            end_page: 2,
            pages: vec![RegulationRequirementExtractionPage {
                page: 2,
                text: "phải; không được; khoảng; nên; có thể; hạn chế; approximately 1/4; at least 50%; must; must not; may; secondary-citation guidance; domain-specific citation style: Vancouver; If a website has no author, use the organization name; lề trên ... lề dưới ...".to_owned(),
                heading_context: Some("Phụ lục 3 §1.2".to_owned()),
            }],
            institution: Some("HIU".to_owned()),
            document_title: Some("Academic writing requirements".to_owned()),
            known_artifact_scope: Some("master thesis".to_owned()),
            allowed_applicability_vocabulary: BTreeMap::from([(
                "artifact_types".to_owned(),
                vec!["master_thesis".to_owned()],
            )]),
        }
    }

    fn regulation_config() -> RegulationRequirementCandidateExtractorConfig {
        RegulationRequirementCandidateExtractorConfig::new(
            "openai",
            "test-model",
            Some("http://127.0.0.1:1/v1".to_owned()),
            TEST_KEY_ENV,
        )
    }

    #[test]
    fn regulation_prompt_preserves_fail_closed_contract() {
        let provider = ModelRegulationRequirementCandidateExtractor::new(regulation_config());
        let body = provider.request_body(&regulation_input()).unwrap();
        let serialized = serde_json::to_string(&body).unwrap();
        for phrase in [
            "ocrExcerpt MUST be copied verbatim",
            "Do not infer missing values",
            "Preserve modality and negation exactly",
            "approximately 1/4, at least 50%",
            "secondary-citation guidance",
            "domain-specific citation style such as Vancouver",
            "Keep conditions such as missing website author",
            "Do not invent aliases or domain mappings",
            "phải, không được, ít nhất, khoảng, nên, có thể, and hạn chế",
        ] {
            assert!(
                serialized.contains(phrase),
                "missing prompt phrase: {phrase}"
            );
        }
    }

    #[test]
    fn regulation_structured_output_preserves_modality_and_unicode() {
        let response = serde_json::json!({
            "choices": [{"message": {"content": serde_json::json!({
                "candidates": [
                    {"ocrExcerpt": "phải", "normalizedRequirement": "phải", "sourceLocator": {"kind": "pdf", "page": 2, "endPage": null, "start": null, "end": null}, "authorityLocator": null, "applicability": {}, "riskFlags": [], "reviewNotes": null},
                    {"ocrExcerpt": "không được", "normalizedRequirement": "không được", "sourceLocator": {"kind": "pdf", "page": 2, "endPage": null, "start": null, "end": null}, "authorityLocator": null, "applicability": {}, "riskFlags": ["modality_sensitive"], "reviewNotes": "Giữ nguyên phủ định."},
                    {"ocrExcerpt": "khoảng", "normalizedRequirement": "khoảng", "sourceLocator": {"kind": "pdf", "page": 2, "endPage": null, "start": null, "end": null}, "authorityLocator": null, "applicability": {}, "riskFlags": [], "reviewNotes": null},
                    {"ocrExcerpt": "nên", "normalizedRequirement": "nên", "sourceLocator": {"kind": "pdf", "page": 2, "endPage": null, "start": null, "end": null}, "authorityLocator": null, "applicability": {}, "riskFlags": [], "reviewNotes": null},
                    {"ocrExcerpt": "có thể", "normalizedRequirement": "có thể", "sourceLocator": {"kind": "pdf", "page": 2, "endPage": null, "start": null, "end": null}, "authorityLocator": null, "applicability": {}, "riskFlags": [], "reviewNotes": null},
                    {"ocrExcerpt": "hạn chế", "normalizedRequirement": "hạn chế", "sourceLocator": {"kind": "pdf", "page": 2, "endPage": null, "start": null, "end": null}, "authorityLocator": null, "applicability": {}, "riskFlags": [], "reviewNotes": null}
                ]
            }).to_string()}}]
        });
        let outputs = parse_regulation_response("openai", response.to_string().as_bytes()).unwrap();
        assert_eq!(
            outputs
                .iter()
                .map(|output| output.normalized_requirement.as_str())
                .collect::<Vec<_>>(),
            vec!["phải", "không được", "khoảng", "nên", "có thể", "hạn chế"]
        );
    }

    #[test]
    fn regulation_structured_output_rejects_unknown_fields() {
        let response =
            br#"{"choices":[{"message":{"content":"{\"candidates\":[],\"approved\":true}"}}]}"#;
        assert!(matches!(
            parse_regulation_response("openai", response),
            Err(RegulationRequirementCandidateExtractionProviderError::InvalidStructuredOutput)
        ));
    }

    #[test]
    fn regulation_structured_output_rejects_missing_required_fields() {
        let response = br#"{"choices":[{"message":{"content":"{\"candidates\":[{\"ocrExcerpt\":\"ph\\u00e1i\"}]}"}}]}"#;
        assert!(matches!(
            parse_regulation_response("openai", response),
            Err(RegulationRequirementCandidateExtractionProviderError::InvalidStructuredOutput)
        ));
    }
}
