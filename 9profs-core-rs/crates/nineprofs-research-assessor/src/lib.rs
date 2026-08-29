//! Stateless, provenance-safe model assessment for citation verification.
//!
//! This adapter only evaluates canonical candidates supplied by 5C2A. It does
//! not retrieve evidence, create provenance, use tools, or retain provider
//! requests and responses.

use std::{collections::BTreeSet, fmt, time::Duration};

use async_trait::async_trait;
use nineprofs_research::{AssessmentMethod, ClaimEvidenceRelation, MAX_RATIONALE_BYTES};
use nineprofs_research_verification::{
    CitationAssessment, CitationAssessmentInput, CitationAssessmentProvider,
    CitationAssessmentProviderError, MAX_TOP_K, SelectedCitationCandidate,
};
use nineprofs_structured_model::{
    StructuredModelClient, StructuredModelConfig, StructuredModelConfigError,
    StructuredModelTransportError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

mod citation_expectation;
pub use citation_expectation::*;
mod cross_claim_candidates;
pub use cross_claim_candidates::*;

pub const ASSESSMENT_IMPLEMENTATION_VERSION: &str = "model-citation-assessor-v1";
pub const ASSESSMENT_INSTRUCTION: &str = r#"You are citation-assessor-v1, a bounded scientific citation assessor.

Evaluate ONLY whether the supplied passages from the EXACT cited reference bear on the supplied claim. Do not use outside knowledge. Do not infer evidence from absent passages. Do not decide whether the claim is globally true. Do not use retrieval ranking metadata as evidentiary confidence. Choose retrievalChunkId values only from the supplied candidates.

Relations:
- supports: a passage materially supports the proposition as stated.
- contradicts: a passage materially conflicts with the proposition as stated.
- contextualizes: a relevant passage adds qualification or background but does not directly establish or negate the proposition.
- insufficient: the supplied cited-source passages do not adequately support or contradict the proposition. Related topic alone is insufficient.

Return one pure JSON object matching the requested schema. Return concise audit-safe rationale only; never return page numbers, ranges, EvidenceIds, source IDs, or other provenance fields. The canonical candidate metadata owns location and provenance."#;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 512;
const MAX_CANDIDATE_EXCERPT_BYTES: usize = 16 * 1024;
const MAX_TOTAL_EXCERPT_BYTES: usize = 128 * 1024;
const MAX_REQUEST_BYTES: usize = 256 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct CitationAssessorConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    /// Environment variable name only. The credential is resolved per call.
    pub api_key_env: String,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub max_output_tokens: u32,
}

impl fmt::Debug for CitationAssessorConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CitationAssessorConfig")
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

impl Default for CitationAssessorConfig {
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

impl CitationAssessorConfig {
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
        Self {
            provider,
            model: model.into().trim().to_owned(),
            base_url: base_url
                .map(|value| value.trim().trim_end_matches('/').to_owned())
                .filter(|value| !value.is_empty()),
            api_key_env: {
                let value = api_key_env.into();
                if value.trim().is_empty() {
                    default_key_env.to_owned()
                } else {
                    value.trim().to_owned()
                }
            },
            ..Self::default()
        }
    }

    pub fn from_env() -> Self {
        let provider = std::env::var("NINEPROFS_CITATION_ASSESSOR_PROVIDER")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let model = std::env::var("NINEPROFS_CITATION_ASSESSOR_MODEL").unwrap_or_default();
        let base_url = std::env::var("NINEPROFS_CITATION_ASSESSOR_BASE_URL").ok();
        let api_key_env = std::env::var("NINEPROFS_CITATION_ASSESSOR_API_KEY_ENV")
            .ok()
            .unwrap_or_default();
        let mut config = Self::new(provider, model, base_url, api_key_env);
        if let Ok(value) = std::env::var("NINEPROFS_CITATION_ASSESSOR_TIMEOUT_MS")
            && let Ok(milliseconds) = value.parse::<u64>()
        {
            config.timeout = Duration::from_millis(milliseconds.clamp(100, 120_000));
        }
        config
    }

    pub fn validate(&self, credential: Option<&str>) -> Result<(), CitationAssessorConfigError> {
        self.shared_config()
            .validate(Some("validation-placeholder"))
            .map(|_| ())
            .map_err(|error| map_config_error(error, &self.provider))?;
        if credential.is_none_or(|value| value.trim().is_empty()) {
            return Err(CitationAssessorConfigError::MissingCredential);
        }
        Ok(())
    }

    pub fn configuration_error(&self) -> Option<CitationAssessorConfigError> {
        self.validate(self.configured_credential().as_deref()).err()
    }

    pub fn configuration_reason(&self) -> Option<String> {
        self.configuration_error().map(|error| error.to_string())
    }

    pub fn readiness(&self) -> CitationAssessorReadiness {
        let provider = (!self.provider.trim().is_empty()).then(|| self.provider.clone());
        let model = (!self.model.trim().is_empty()).then(|| self.model.clone());
        let status = if provider.is_none() && model.is_none() && self.base_url.is_none() {
            CitationAssessorReadinessStatus::NotConfigured
        } else if self.configuration_error().is_some() {
            CitationAssessorReadinessStatus::InvalidConfiguration
        } else {
            CitationAssessorReadinessStatus::Ready
        };
        CitationAssessorReadiness {
            provider,
            model,
            ready: matches!(status, CitationAssessorReadinessStatus::Ready),
            status,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.readiness().ready
    }

    fn configured_credential(&self) -> Option<String> {
        if self.api_key_env.trim().is_empty() {
            None
        } else {
            std::env::var(self.api_key_env.trim()).ok()
        }
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

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CitationAssessorConfigError {
    #[error("citation assessor provider is not configured")]
    MissingProvider,
    #[error("unsupported citation assessor provider `{0}`")]
    UnsupportedProvider(String),
    #[error("citation assessor model is not configured")]
    MissingModel,
    #[error("citation assessor credential environment variable is not configured")]
    MissingCredentialEnvironment,
    #[error("citation assessor credential is not configured")]
    MissingCredential,
    #[error("citation assessor base URL is invalid")]
    InvalidBaseUrl,
    #[error("citation assessor limits are invalid")]
    InvalidLimits,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CitationAssessorReadinessStatus {
    NotConfigured,
    InvalidConfiguration,
    Ready,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CitationAssessorReadiness {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub status: CitationAssessorReadinessStatus,
    pub ready: bool,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CitationAssessorError {
    #[error("citation assessor is not configured")]
    NotConfigured,
    #[error("citation assessor configuration is invalid")]
    InvalidConfiguration,
    #[error("citation assessor input is invalid")]
    InvalidInput,
    #[error("citation assessor input exceeded size limit")]
    InputTooLarge,
    #[error("citation assessor request timed out")]
    Timeout,
    #[error("citation assessor authorization failed")]
    Unauthorized,
    #[error("citation assessor rate limit exceeded")]
    RateLimited,
    #[error("citation assessor provider is unavailable")]
    ProviderUnavailable,
    #[error("citation assessor response was malformed")]
    MalformedResponse,
    #[error("citation assessor returned invalid structured output")]
    InvalidStructuredOutput,
    #[error("citation assessor response exceeded size limit")]
    ResponseTooLarge,
}

#[derive(Clone)]
pub struct ModelCitationAssessor {
    config: CitationAssessorConfig,
    client: StructuredModelClient,
}

impl fmt::Debug for ModelCitationAssessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCitationAssessor")
            .field("config", &self.config)
            .finish()
    }
}

impl ModelCitationAssessor {
    pub fn new(config: CitationAssessorConfig) -> Self {
        let client = StructuredModelClient::new(config.shared_config());
        Self { config, client }
    }

    pub fn config(&self) -> &CitationAssessorConfig {
        &self.config
    }

    pub fn readiness(&self) -> CitationAssessorReadiness {
        self.config.readiness()
    }

    pub async fn assess_model(
        &self,
        input: CitationAssessmentInput,
    ) -> Result<CitationAssessment, CitationAssessorError> {
        validate_input(&input)?;
        if input.candidates.is_empty() {
            return Ok(CitationAssessment {
                overall_relation: ClaimEvidenceRelation::Insufficient,
                rationale: "No canonical passages were retrieved from the cited source.".to_owned(),
                selected_candidates: Vec::new(),
            });
        }

        let credential = self
            .config
            .configured_credential()
            .ok_or(CitationAssessorError::NotConfigured)?;
        self.config
            .validate(Some(credential.as_str()))
            .map_err(|_| CitationAssessorError::InvalidConfiguration)?;

        let prompt = build_prompt(&input).map_err(|_| CitationAssessorError::InvalidInput)?;
        let body = self.request_body(&prompt);
        let body_bytes =
            serde_json::to_vec(&body).map_err(|_| CitationAssessorError::InvalidInput)?;
        if body_bytes.len() > MAX_REQUEST_BYTES {
            return Err(CitationAssessorError::InputTooLarge);
        }
        let bytes = self.send(body, &credential).await?;
        let structured = match self.config.provider.as_str() {
            "openai" => parse_openai_response(&bytes)?,
            "anthropic" => parse_anthropic_response(&bytes)?,
            _ => return Err(CitationAssessorError::InvalidConfiguration),
        };
        structured.into_assessment(&input)
    }

    fn request_body(&self, prompt: &str) -> Value {
        match self.config.provider.as_str() {
            "anthropic" => json!({
                "model": self.config.model,
                "system": ASSESSMENT_INSTRUCTION,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": self.config.max_output_tokens,
                "temperature": 0,
                "tools": [{
                    "name": "citation_assessment",
                    "description": "Return the strict citation assessment object.",
                    "input_schema": assessment_schema()
                }],
                "tool_choice": {"type": "tool", "name": "citation_assessment"}
            }),
            _ => json!({
                "model": self.config.model,
                "messages": [
                    {"role": "system", "content": ASSESSMENT_INSTRUCTION},
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0,
                "max_tokens": self.config.max_output_tokens,
                "response_format": {"type": "json_object"}
            }),
        }
    }

    async fn send(&self, body: Value, credential: &str) -> Result<Vec<u8>, CitationAssessorError> {
        self.client
            .execute_json(&body, credential)
            .await
            .map_err(map_transport_error)
    }
}

#[async_trait]
impl CitationAssessmentProvider for ModelCitationAssessor {
    fn identity(&self) -> nineprofs_research_verification::CitationAssessmentProviderIdentity {
        nineprofs_research_verification::CitationAssessmentProviderIdentity {
            provider_id: self.config.provider.clone(),
            implementation_version: ASSESSMENT_IMPLEMENTATION_VERSION.to_owned(),
            model_id: (!self.config.model.trim().is_empty()).then(|| self.config.model.clone()),
        }
    }

    fn assessment_method(&self) -> AssessmentMethod {
        AssessmentMethod::ExternalService
    }

    async fn assess(
        &self,
        input: CitationAssessmentInput,
    ) -> Result<CitationAssessment, CitationAssessmentProviderError> {
        self.assess_model(input).await.map_err(Into::into)
    }
}

impl From<CitationAssessorError> for CitationAssessmentProviderError {
    fn from(error: CitationAssessorError) -> Self {
        match error {
            CitationAssessorError::NotConfigured => Self::NotConfigured,
            CitationAssessorError::InvalidConfiguration => Self::InvalidConfiguration,
            CitationAssessorError::InvalidInput => Self::InvalidInput,
            CitationAssessorError::InputTooLarge => Self::InputTooLarge,
            CitationAssessorError::Timeout => Self::Timeout,
            CitationAssessorError::Unauthorized => Self::Unauthorized,
            CitationAssessorError::RateLimited => Self::RateLimited,
            CitationAssessorError::ProviderUnavailable => Self::ProviderUnavailable,
            CitationAssessorError::MalformedResponse => Self::MalformedResponse,
            CitationAssessorError::InvalidStructuredOutput => Self::InvalidStructuredOutput,
            CitationAssessorError::ResponseTooLarge => Self::ResponseTooLarge,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptInput<'a> {
    claim: PromptClaim<'a>,
    citation: PromptCitation<'a>,
    candidates: Vec<PromptCandidate<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptClaim<'a> {
    id: &'a str,
    text: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptCitation<'a> {
    occurrence_id: &'a str,
    target_id: &'a str,
    reference_key: &'a str,
    cited_locator: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptCandidate<'a> {
    retrieval_chunk_id: &'a str,
    research_source_id: &'a str,
    source_snapshot_id: &'a str,
    extraction_id: &'a str,
    page: u32,
    start: u64,
    end: u64,
    verbatim_excerpt: &'a str,
    /// Retrieval score intentionally omitted: rank is ordering metadata only.
    retrieval_rank: u32,
}

fn build_prompt(input: &CitationAssessmentInput) -> Result<String, serde_json::Error> {
    serde_json::to_string(&PromptInput {
        claim: PromptClaim {
            id: &input.claim_id,
            text: &input.claim_text,
        },
        citation: PromptCitation {
            occurrence_id: &input.citation_occurrence_id,
            target_id: &input.citation_target_id,
            reference_key: &input.reference_key,
            cited_locator: input.cited_locator.as_deref(),
        },
        candidates: input
            .candidates
            .iter()
            .map(|candidate| PromptCandidate {
                retrieval_chunk_id: &candidate.retrieval_chunk_id,
                research_source_id: &candidate.research_source_id,
                source_snapshot_id: &candidate.source_snapshot_id,
                extraction_id: &candidate.extraction_id,
                page: candidate.page,
                start: candidate.start,
                end: candidate.end,
                verbatim_excerpt: &candidate.verbatim_excerpt,
                retrieval_rank: candidate.retrieval_rank,
            })
            .collect(),
    })
}

fn validate_input(input: &CitationAssessmentInput) -> Result<(), CitationAssessorError> {
    if input.claim_text.is_empty() || input.reference_key.is_empty() {
        return Err(CitationAssessorError::InvalidInput);
    }
    if input.claim_text.len() > nineprofs_research::MAX_CLAIM_TEXT_BYTES
        || input.reference_key.len() > nineprofs_research::MAX_CITATION_REFERENCE_KEY_BYTES
        || input
            .cited_locator
            .as_ref()
            .is_some_and(|value| value.len() > nineprofs_research::MAX_CITED_LOCATOR_BYTES)
        || input.candidates.len() > MAX_TOP_K as usize
    {
        return Err(CitationAssessorError::InputTooLarge);
    }
    let mut total_excerpt_bytes = 0usize;
    for candidate in &input.candidates {
        if candidate.verbatim_excerpt.len() > MAX_CANDIDATE_EXCERPT_BYTES {
            return Err(CitationAssessorError::InputTooLarge);
        }
        total_excerpt_bytes = total_excerpt_bytes
            .checked_add(candidate.verbatim_excerpt.len())
            .ok_or(CitationAssessorError::InputTooLarge)?;
    }
    if total_excerpt_bytes > MAX_TOTAL_EXCERPT_BYTES {
        return Err(CitationAssessorError::InputTooLarge);
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StructuredAssessment {
    overall_relation: ClaimEvidenceRelation,
    rationale: String,
    selected_candidates: Vec<StructuredSelectedCandidate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StructuredSelectedCandidate {
    retrieval_chunk_id: String,
    relation: ClaimEvidenceRelation,
    rationale: Option<String>,
}

impl StructuredAssessment {
    fn into_assessment(
        self,
        input: &CitationAssessmentInput,
    ) -> Result<CitationAssessment, CitationAssessorError> {
        if self.rationale.len() > MAX_RATIONALE_BYTES
            || self.selected_candidates.len() > input.candidates.len()
        {
            return Err(CitationAssessorError::InvalidStructuredOutput);
        }
        let known: BTreeSet<_> = input
            .candidates
            .iter()
            .map(|candidate| candidate.retrieval_chunk_id.as_str())
            .collect();
        let mut selected = BTreeSet::new();
        let selected_candidates = self
            .selected_candidates
            .into_iter()
            .map(|candidate| {
                if !known.contains(candidate.retrieval_chunk_id.as_str())
                    || !selected.insert(candidate.retrieval_chunk_id.clone())
                    || candidate
                        .rationale
                        .as_ref()
                        .is_some_and(|value| value.len() > MAX_RATIONALE_BYTES)
                {
                    return Err(CitationAssessorError::InvalidStructuredOutput);
                }
                Ok(SelectedCitationCandidate {
                    retrieval_chunk_id: candidate.retrieval_chunk_id,
                    relation: candidate.relation,
                    rationale: candidate.rationale,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if matches!(
            self.overall_relation,
            ClaimEvidenceRelation::Supports
                | ClaimEvidenceRelation::Contradicts
                | ClaimEvidenceRelation::Contextualizes
        ) && selected_candidates.is_empty()
        {
            return Err(CitationAssessorError::InvalidStructuredOutput);
        }
        if matches!(self.overall_relation, ClaimEvidenceRelation::Insufficient)
            && selected_candidates.iter().any(|candidate| {
                matches!(
                    candidate.relation,
                    ClaimEvidenceRelation::Supports | ClaimEvidenceRelation::Contradicts
                )
            })
        {
            return Err(CitationAssessorError::InvalidStructuredOutput);
        }
        Ok(CitationAssessment {
            overall_relation: self.overall_relation,
            rationale: self.rationale,
            selected_candidates,
        })
    }
}

fn parse_openai_response(bytes: &[u8]) -> Result<StructuredAssessment, CitationAssessorError> {
    let response: OpenAiResponse =
        serde_json::from_slice(bytes).map_err(|_| CitationAssessorError::MalformedResponse)?;
    let content = response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty())
        .ok_or(CitationAssessorError::MalformedResponse)?;
    parse_structured_json(&content)
}

fn parse_anthropic_response(bytes: &[u8]) -> Result<StructuredAssessment, CitationAssessorError> {
    let response: AnthropicResponse =
        serde_json::from_slice(bytes).map_err(|_| CitationAssessorError::MalformedResponse)?;
    let input = response
        .content
        .into_iter()
        .find(|block| {
            block.block_type == "tool_use" && block.name.as_deref() == Some("citation_assessment")
        })
        .and_then(|block| block.input)
        .ok_or(CitationAssessorError::MalformedResponse)?;
    serde_json::from_value(input).map_err(|_| CitationAssessorError::InvalidStructuredOutput)
}

fn parse_structured_json(text: &str) -> Result<StructuredAssessment, CitationAssessorError> {
    let text = text.trim();
    let json_text = if let Some(content) = text.strip_prefix("```json") {
        content
            .strip_suffix("```")
            .map(str::trim)
            .ok_or(CitationAssessorError::MalformedResponse)?
    } else if let Some(content) = text.strip_prefix("```") {
        content
            .strip_suffix("```")
            .map(str::trim)
            .ok_or(CitationAssessorError::MalformedResponse)?
    } else {
        text
    };
    serde_json::from_str(json_text).map_err(|error| {
        if error.is_syntax() || error.is_eof() {
            CitationAssessorError::MalformedResponse
        } else {
            CitationAssessorError::InvalidStructuredOutput
        }
    })
}

fn assessment_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["overallRelation", "rationale", "selectedCandidates"],
        "properties": {
            "overallRelation": {"type": "string", "enum": ["supports", "contradicts", "contextualizes", "insufficient"]},
            "rationale": {"type": "string", "maxLength": MAX_RATIONALE_BYTES},
            "selectedCandidates": {
                "type": "array",
                "maxItems": MAX_TOP_K,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["retrievalChunkId", "relation"],
                    "properties": {
                        "retrievalChunkId": {"type": "string"},
                        "relation": {"type": "string", "enum": ["supports", "contradicts", "contextualizes", "insufficient"]},
                        "rationale": {"type": ["string", "null"], "maxLength": MAX_RATIONALE_BYTES}
                    }
                }
            }
        }
    })
}

fn map_config_error(
    error: StructuredModelConfigError,
    provider: &str,
) -> CitationAssessorConfigError {
    match error {
        StructuredModelConfigError::MissingProvider => CitationAssessorConfigError::MissingProvider,
        StructuredModelConfigError::UnsupportedProvider => {
            CitationAssessorConfigError::UnsupportedProvider(provider.to_owned())
        }
        StructuredModelConfigError::MissingModel => CitationAssessorConfigError::MissingModel,
        StructuredModelConfigError::MissingCredentialEnvironment => {
            CitationAssessorConfigError::MissingCredentialEnvironment
        }
        StructuredModelConfigError::MissingCredential => {
            CitationAssessorConfigError::MissingCredential
        }
        StructuredModelConfigError::InvalidBaseUrl => CitationAssessorConfigError::InvalidBaseUrl,
        StructuredModelConfigError::InvalidLimits => CitationAssessorConfigError::InvalidLimits,
    }
}

fn map_transport_error(error: StructuredModelTransportError) -> CitationAssessorError {
    match error {
        StructuredModelTransportError::NotConfigured => CitationAssessorError::NotConfigured,
        StructuredModelTransportError::InvalidConfiguration
        | StructuredModelTransportError::ClientBuildFailed => {
            CitationAssessorError::InvalidConfiguration
        }
        StructuredModelTransportError::Timeout => CitationAssessorError::Timeout,
        StructuredModelTransportError::Unauthorized => CitationAssessorError::Unauthorized,
        StructuredModelTransportError::RateLimited => CitationAssessorError::RateLimited,
        StructuredModelTransportError::ProviderUnavailable
        | StructuredModelTransportError::Transport => CitationAssessorError::ProviderUnavailable,
        StructuredModelTransportError::ResponseTooLarge => CitationAssessorError::ResponseTooLarge,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use nineprofs_db::Database;
    use nineprofs_realtime::BroadcastEventBus;
    use nineprofs_research::{
        CapturePdfExtraction, CapturePdfPage, CitationBindingMethod, CitationOccurrenceOrigin,
        ClaimOrigin, CreateCitationOccurrence, CreateCitationTarget, CreateCitationTargetBinding,
        CreateClaimCitationLink, CreateResearchCase, CreateResearchClaim, CreateResearchSource,
        PdfExtractionStatus, ResearchArtifactStore, SourceKind, SqliteResearchRepository,
    };
    use nineprofs_research_verification::CitationAssessmentCandidate;
    use nineprofs_research_verification::{
        CitationRetrievalCandidate, CitationRetrievalError, CitationRetrievalProvider,
        CitationVerificationService, CitationVerificationStatus,
    };
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex, Once},
        time::Duration,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    const TEST_KEY_ENV: &str = "NINEPROFS_CITATION_ASSESSOR_TEST_KEY";
    const TEST_KEY: &str = "citation-assessor-test-secret";
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn install_test_key() {
        static ONCE: Once = Once::new();
        ONCE.call_once(|| unsafe {
            std::env::set_var(TEST_KEY_ENV, TEST_KEY);
        });
    }

    fn input() -> CitationAssessmentInput {
        CitationAssessmentInput {
            claim_id: "claim-1".to_owned(),
            claim_text: "Drug A reduced mortality.".to_owned(),
            citation_occurrence_id: "occurrence-1".to_owned(),
            citation_target_id: "target-1".to_owned(),
            reference_key: "ref-a".to_owned(),
            cited_locator: Some("p. 42".to_owned()),
            candidates: vec![CitationAssessmentCandidate {
                retrieval_chunk_id: "chunk-1".to_owned(),
                research_source_id: "source-1".to_owned(),
                source_snapshot_id: "snapshot-1".to_owned(),
                extraction_id: "extraction-1".to_owned(),
                page: 42,
                start: 0,
                end: 54,
                verbatim_excerpt: "Patients receiving Drug A had significantly lower mortality."
                    .to_owned(),
                retrieval_score: 0.99,
                retrieval_rank: 1,
            }],
        }
    }

    fn config(provider: &str, base_url: String) -> CitationAssessorConfig {
        install_test_key();
        CitationAssessorConfig::new(provider, "test-model", Some(base_url), TEST_KEY_ENV)
    }

    struct MockServer {
        url: String,
        capture: std::sync::Arc<std::sync::Mutex<Option<(String, String)>>>,
        task: tokio::task::JoinHandle<()>,
    }

    struct FixtureRetrieval {
        candidate: CitationRetrievalCandidate,
    }

    #[async_trait]
    impl CitationRetrievalProvider for FixtureRetrieval {
        async fn retrieve_exact_extraction(
            &self,
            _research_case_id: &str,
            _extraction_id: &str,
            _query: &str,
            _top_k: u32,
        ) -> Result<Vec<CitationRetrievalCandidate>, CitationRetrievalError> {
            Ok(vec![self.candidate.clone()])
        }
    }

    async fn mock_server(body: String) -> MockServer {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let capture = std::sync::Arc::new(std::sync::Mutex::new(None));
        let capture_for_task = std::sync::Arc::clone(&capture);
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            let mut body_start;
            loop {
                let count = stream.read(&mut chunk).await.unwrap();
                if count == 0 {
                    return;
                }
                request.extend_from_slice(&chunk[..count]);
                if let Some(index) = request.windows(4).position(|value| value == b"\r\n\r\n") {
                    body_start = index + 4;
                    let content_length = String::from_utf8_lossy(&request[..index])
                        .lines()
                        .find_map(|line| {
                            line.strip_prefix("Content-Length:")
                                .or_else(|| line.strip_prefix("content-length:"))
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= body_start + content_length {
                        break;
                    }
                }
            }
            let head = String::from_utf8_lossy(&request[..body_start]).into_owned();
            let request_path = head
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or_default()
                .to_owned();
            let request_body = String::from_utf8_lossy(&request[body_start..]).into_owned();
            *capture_for_task.lock().unwrap() = Some((request_path, head + &request_body));
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        MockServer {
            url: format!("http://{address}/custom/v1"),
            capture,
            task,
        }
    }

    #[tokio::test]
    async fn openai_contract_uses_bounded_json_and_redacts_secret() {
        let response = json!({
            "choices": [{"message": {"content": "{\"overallRelation\":\"supports\",\"rationale\":\"The passage reports lower mortality.\",\"selectedCandidates\":[{\"retrievalChunkId\":\"chunk-1\",\"relation\":\"supports\"}]}"}}]
        })
        .to_string();
        let server = mock_server(response).await;
        let assessor = ModelCitationAssessor::new(config("openai", server.url.clone()));
        let result = assessor.assess_model(input()).await.unwrap();
        assert_eq!(result.overall_relation, ClaimEvidenceRelation::Supports);
        assert_eq!(result.selected_candidates[0].retrieval_chunk_id, "chunk-1");
        let debug = format!("{assessor:?}");
        assert!(!debug.contains(TEST_KEY));
        server.task.await.unwrap();
        let (_, request) = server.capture.lock().unwrap().clone().unwrap();
        let request_lower = request.to_ascii_lowercase();
        assert!(request_lower.contains("authorization: bearer "));
        assert!(request.contains("response_format"));
        assert!(request.contains("\"temperature\":0"));
        assert!(!request.contains("retrievalScore"));
        assert!(request.contains("Do not use outside knowledge"));
    }

    #[tokio::test]
    async fn anthropic_contract_uses_tool_schema_and_native_auth() {
        let response = json!({
            "content": [{"type": "tool_use", "name": "citation_assessment", "input": {
                "overallRelation": "contextualizes",
                "rationale": "The passage gives population context.",
                "selectedCandidates": [{"retrievalChunkId": "chunk-1", "relation": "contextualizes"}]
            }}]
        })
        .to_string();
        let server = mock_server(response).await;
        let assessor = ModelCitationAssessor::new(config("anthropic", server.url.clone()));
        let result = assessor.assess_model(input()).await.unwrap();
        assert_eq!(
            result.overall_relation,
            ClaimEvidenceRelation::Contextualizes
        );
        server.task.await.unwrap();
        let (_, request) = server.capture.lock().unwrap().clone().unwrap();
        let request_lower = request.to_ascii_lowercase();
        assert!(request_lower.contains("x-api-key: "));
        assert!(request_lower.contains("anthropic-version: 2023-06-01"));
        assert!(!request_lower.contains("authorization:"));
        assert!(request.contains("\"tools\""));
        assert!(request.contains("\"tool_choice\""));
    }

    #[tokio::test]
    async fn model_assessor_promotes_only_selected_canonical_candidate_in_orchestrator() {
        let response = json!({
            "choices": [{"message": {"content": "{\"overallRelation\":\"supports\",\"rationale\":\"The cited passage reports lower mortality.\",\"selectedCandidates\":[{\"retrievalChunkId\":\"chunk-1\",\"relation\":\"supports\"}]}"}}]
        })
        .to_string();
        let server = mock_server(response).await;
        let database = Database::in_memory().await.unwrap();
        let root = std::env::temp_dir().join(format!("9profs-assessor-{}", std::process::id()));
        let store = Arc::new(ResearchArtifactStore::new(
            root.clone(),
            database.pool().clone(),
        ));
        let research = Arc::new(
            nineprofs_research::ResearchService::new(
                SqliteResearchRepository::new(database.pool().clone()),
                Arc::new(BroadcastEventBus::new(32)),
            )
            .with_artifact_store(Arc::clone(&store)),
        );
        let mut upload = store.begin_upload("reference.pdf").unwrap();
        upload.append(b"%PDF-1.7\nfixture").unwrap();
        let artifact = upload.finish().await.unwrap();
        let case = research
            .create_case(CreateResearchCase {
                title: "Assessor integration".to_owned(),
            })
            .await
            .unwrap();
        let source = research
            .create_source(CreateResearchSource {
                research_case_id: case.id.clone(),
                kind: SourceKind::ReferencePdf,
                label: "Reference".to_owned(),
                identity: None,
            })
            .await
            .unwrap();
        let snapshot = research
            .capture_verified_artifact_snapshot(source.id.clone(), &artifact, BTreeMap::new())
            .await
            .unwrap();
        let excerpt = "Patients receiving Drug A had significantly lower mortality.";
        let extraction = research
            .capture_pdf_extraction(CapturePdfExtraction {
                source_snapshot_id: snapshot.id.clone(),
                extractor: "fixture".to_owned(),
                extractor_version: Some("1".to_owned()),
                page_count: 1,
                status: PdfExtractionStatus::Ready,
                pages: vec![CapturePdfPage {
                    page: 1,
                    text: excerpt.to_owned(),
                }],
            })
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO research_dify_case_indexes
             (id, research_case_id, dataset_id, status, failure_code, created_at_ms, updated_at_ms)
             VALUES (?, ?, ?, 'ready', NULL, 0, 0)",
        )
        .bind("case-index-1")
        .bind(case.id.as_str())
        .bind("dataset-1")
        .execute(database.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO research_dify_extraction_indexes
             (id, case_index_id, research_case_id, extraction_id, source_snapshot_id,
              document_id, chunker_version, status, failure_code, created_at_ms, updated_at_ms)
             VALUES (?, ?, ?, ?, ?, NULL, 'fixture-v1', 'ready', NULL, 0, 0)",
        )
        .bind("extraction-index-1")
        .bind("case-index-1")
        .bind(case.id.as_str())
        .bind(extraction.id.as_str())
        .bind(snapshot.id.as_str())
        .execute(database.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO research_retrieval_chunks
             (id, extraction_index_id, research_case_id, research_source_id, source_snapshot_id,
              extraction_id, page, start_offset, end_offset, text, hash_algorithm, text_hash)
             VALUES (?, ?, ?, ?, ?, ?, 1, 0, ?, ?, 'sha256', 'fixture-hash')",
        )
        .bind("chunk-1")
        .bind("extraction-index-1")
        .bind(case.id.as_str())
        .bind(source.id.as_str())
        .bind(snapshot.id.as_str())
        .bind(extraction.id.as_str())
        .bind(excerpt.chars().count() as i64)
        .bind(excerpt)
        .execute(database.pool())
        .await
        .unwrap();
        let claim = research
            .create_claim(CreateResearchClaim {
                research_case_id: case.id.clone(),
                text: "Drug A reduced mortality.".to_owned(),
                origin: ClaimOrigin::User,
            })
            .await
            .unwrap();
        let occurrence = research
            .create_citation_occurrence(CreateCitationOccurrence {
                research_case_id: case.id.clone(),
                origin: CitationOccurrenceOrigin::Imported {
                    source: "fixture".to_owned(),
                },
                rendered_text: "[ref-a]".to_owned(),
            })
            .await
            .unwrap();
        let target = research
            .create_citation_target(CreateCitationTarget {
                citation_occurrence_id: occurrence.id.clone(),
                ordinal: 0,
                reference_key: "ref-a".to_owned(),
                cited_locator: Some("p. 1".to_owned()),
            })
            .await
            .unwrap();
        let binding = research
            .create_citation_target_binding(CreateCitationTargetBinding {
                research_case_id: case.id.clone(),
                citation_target_id: target.id.clone(),
                source_id: source.id.clone(),
                source_snapshot_id: Some(snapshot.id.clone()),
                extraction_id: Some(extraction.id.clone()),
                method: CitationBindingMethod::Imported,
            })
            .await
            .unwrap();
        let link = research
            .create_claim_citation_link(CreateClaimCitationLink {
                research_case_id: case.id.clone(),
                claim_id: claim.id.clone(),
                citation_occurrence_id: occurrence.id.clone(),
            })
            .await
            .unwrap();
        let retrieval = FixtureRetrieval {
            candidate: CitationRetrievalCandidate {
                retrieval_chunk_id: "chunk-1".to_owned(),
                research_source_id: source.id.to_string(),
                source_snapshot_id: snapshot.id.to_string(),
                extraction_id: extraction.id.to_string(),
                page: 1,
                start: 0,
                end: excerpt.chars().count() as u64,
                verbatim_excerpt: excerpt.to_owned(),
                retrieval_score: 0.99,
                provider: "fixture".to_owned(),
                rank: 1,
            },
        };
        let assessor = Arc::new(ModelCitationAssessor::new(config(
            "openai",
            server.url.clone(),
        )));
        let service = CitationVerificationService::new(
            database.pool().clone(),
            research.clone(),
            Arc::new(retrieval),
            Arc::new(BroadcastEventBus::new(32)),
        )
        .with_assessor(assessor);
        let run = service
            .verify(
                nineprofs_research_verification::CreateCitationVerification {
                    claim_citation_link_id: link.id.to_string(),
                    citation_target_binding_id: binding.id.to_string(),
                },
            )
            .await
            .unwrap();

        assert_eq!(run.status, CitationVerificationStatus::Completed);
        assert_eq!(run.evidence.len(), 1);
        assert_eq!(run.evidence[0].retrieval_chunk_id, "chunk-1");
        assert_eq!(
            run.result.as_ref().unwrap().assessor_model_id.as_deref(),
            Some("test-model")
        );
        assert_eq!(
            research
                .list_links(None, Some(claim.id.as_str()), None)
                .await
                .unwrap()
                .len(),
            1
        );
        server.task.await.unwrap();
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test]
    async fn empty_candidates_are_deterministic_without_network_or_credentials() {
        let mut value = input();
        value.candidates.clear();
        let assessor = ModelCitationAssessor::new(CitationAssessorConfig::default());
        let result = assessor.assess_model(value).await.unwrap();
        assert_eq!(result.overall_relation, ClaimEvidenceRelation::Insufficient);
        assert!(result.selected_candidates.is_empty());
    }

    #[test]
    fn readiness_distinguishes_missing_and_invalid_configuration() {
        let not_configured = CitationAssessorConfig::default().readiness();
        assert_eq!(
            not_configured.status,
            CitationAssessorReadinessStatus::NotConfigured
        );
        let invalid = CitationAssessorConfig::new(
            "openai",
            "test-model",
            Some("not-a-url".to_owned()),
            "NINEPROFS_CITATION_ASSESSOR_MISSING_KEY",
        )
        .readiness();
        assert_eq!(
            invalid.status,
            CitationAssessorReadinessStatus::InvalidConfiguration
        );
    }

    #[test]
    fn configuration_validation_preserves_provider_and_limit_errors() {
        assert_eq!(
            CitationAssessorConfig::default().configuration_error(),
            Some(CitationAssessorConfigError::MissingProvider)
        );
        assert!(matches!(
            CitationAssessorConfig::new("unsupported", "test-model", None, TEST_KEY_ENV,)
                .configuration_error(),
            Some(CitationAssessorConfigError::UnsupportedProvider(_))
        ));
        assert_eq!(
            CitationAssessorConfig::new("openai", "", None, TEST_KEY_ENV).configuration_error(),
            Some(CitationAssessorConfigError::MissingModel)
        );
        install_test_key();
        assert_eq!(
            CitationAssessorConfig::new("openai", "test-model", None, "MISSING_KEY")
                .configuration_error(),
            Some(CitationAssessorConfigError::MissingCredential)
        );
        assert_eq!(
            config("openai", "not-a-url".to_owned()).configuration_error(),
            Some(CitationAssessorConfigError::InvalidBaseUrl)
        );
        let mut invalid_limits = config("openai", "http://127.0.0.1".to_owned());
        invalid_limits.timeout = Duration::ZERO;
        assert_eq!(
            invalid_limits.configuration_error(),
            Some(CitationAssessorConfigError::InvalidLimits)
        );
        invalid_limits.timeout = Duration::from_secs(1);
        invalid_limits.max_response_bytes = 0;
        assert_eq!(
            invalid_limits.configuration_error(),
            Some(CitationAssessorConfigError::InvalidLimits)
        );
    }

    #[test]
    fn provider_defaults_choose_provider_specific_key_names() {
        assert_eq!(
            CitationAssessorConfig::new("openai", "model", None, "").api_key_env,
            "OPENAI_API_KEY"
        );
        assert_eq!(
            CitationAssessorConfig::new("anthropic", "model", None, "").api_key_env,
            "ANTHROPIC_API_KEY"
        );
    }

    #[test]
    fn from_env_uses_only_citation_assessor_prefix() {
        let _guard = ENV_LOCK.lock().unwrap();
        let names = [
            "NINEPROFS_CITATION_ASSESSOR_PROVIDER",
            "NINEPROFS_CITATION_ASSESSOR_MODEL",
            "NINEPROFS_CITATION_ASSESSOR_BASE_URL",
            "NINEPROFS_CITATION_ASSESSOR_API_KEY_ENV",
            "NINEPROFS_CLAIM_EXTRACTOR_PROVIDER",
            "NINEPROFS_CLAIM_EXTRACTOR_MODEL",
        ];
        let previous = names
            .iter()
            .map(|name| (*name, std::env::var(name).ok()))
            .collect::<Vec<_>>();
        unsafe {
            std::env::set_var("NINEPROFS_CITATION_ASSESSOR_PROVIDER", "openai");
            std::env::set_var("NINEPROFS_CITATION_ASSESSOR_MODEL", "citation-model");
            std::env::remove_var("NINEPROFS_CITATION_ASSESSOR_BASE_URL");
            std::env::set_var("NINEPROFS_CITATION_ASSESSOR_API_KEY_ENV", TEST_KEY_ENV);
            std::env::set_var("NINEPROFS_CLAIM_EXTRACTOR_PROVIDER", "anthropic");
            std::env::set_var("NINEPROFS_CLAIM_EXTRACTOR_MODEL", "claim-model");
        }
        let config = CitationAssessorConfig::from_env();
        for (name, value) in previous {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "citation-model");
        assert_eq!(config.api_key_env, TEST_KEY_ENV);
    }

    #[test]
    fn strict_parser_rejects_unknown_fields_relations_and_candidates() {
        let extra = r#"{"overallRelation":"supports","rationale":"x","selectedCandidates":[],"evidenceId":"fake"}"#;
        assert!(matches!(
            parse_structured_json(extra),
            Err(CitationAssessorError::InvalidStructuredOutput)
        ));
        let unknown_relation =
            r#"{"overallRelation":"partially_supported","rationale":"x","selectedCandidates":[]}"#;
        assert!(matches!(
            parse_structured_json(unknown_relation),
            Err(CitationAssessorError::InvalidStructuredOutput)
        ));
        let unknown_candidate = StructuredAssessment {
            overall_relation: ClaimEvidenceRelation::Supports,
            rationale: "x".to_owned(),
            selected_candidates: vec![StructuredSelectedCandidate {
                retrieval_chunk_id: "unknown".to_owned(),
                relation: ClaimEvidenceRelation::Supports,
                rationale: None,
            }],
        };
        assert!(matches!(
            unknown_candidate.into_assessment(&input()),
            Err(CitationAssessorError::InvalidStructuredOutput)
        ));
    }

    #[test]
    fn input_bounds_reject_without_truncating_canonical_text() {
        let mut value = input();
        value.candidates[0].verbatim_excerpt = "x".repeat(MAX_CANDIDATE_EXCERPT_BYTES + 1);
        assert!(matches!(
            validate_input(&value),
            Err(CitationAssessorError::InputTooLarge)
        ));
    }

    #[test]
    fn provider_identity_and_method_are_external_and_secret_free() {
        let assessor = ModelCitationAssessor::new(CitationAssessorConfig::new(
            "openai",
            "model-a",
            None,
            "OPENAI_API_KEY",
        ));
        assert_eq!(assessor.identity().provider_id, "openai");
        assert_eq!(assessor.identity().model_id.as_deref(), Some("model-a"));
        assert_eq!(
            assessor.assessment_method(),
            AssessmentMethod::ExternalService
        );
        assert!(!format!("{assessor:?}").contains("OPENAI_API_KEY="));
    }

    #[test]
    fn fenced_json_compatibility_stays_narrow() {
        let assessment = parse_structured_json(
            "```json\n{\"overallRelation\":\"insufficient\",\"rationale\":\"No result.\",\"selectedCandidates\":[]}\n```",
        )
        .unwrap();
        assert_eq!(
            assessment.overall_relation,
            ClaimEvidenceRelation::Insufficient
        );
        assert!(matches!(
            parse_structured_json("prefix {\"overallRelation\":\"insufficient\"}"),
            Err(CitationAssessorError::MalformedResponse
                | CitationAssessorError::InvalidStructuredOutput)
        ));
    }

    #[test]
    fn config_debug_contains_name_not_credential() {
        let config = CitationAssessorConfig::new("openai", "model", None, TEST_KEY_ENV);
        let debug = format!("{config:?}");
        assert!(debug.contains(TEST_KEY_ENV));
        assert!(!debug.contains(TEST_KEY));
    }

    #[test]
    fn timeout_is_bounded_for_test_configuration() {
        let mut config = CitationAssessorConfig::default();
        config.provider = "openai".to_owned();
        config.model = "model".to_owned();
        config.timeout = Duration::ZERO;
        assert_eq!(
            config.configuration_error(),
            Some(CitationAssessorConfigError::InvalidLimits)
        );
    }

    #[tokio::test]
    async fn response_size_limit_rejects_before_parsing() {
        let server = mock_server("{}".repeat(32)).await;
        let mut assessor_config = config("openai", server.url.clone());
        assessor_config.max_response_bytes = 8;
        let assessor = ModelCitationAssessor::new(assessor_config);
        assert!(matches!(
            assessor.assess_model(input()).await,
            Err(CitationAssessorError::ResponseTooLarge)
        ));
        server.task.await.unwrap();
    }
}
