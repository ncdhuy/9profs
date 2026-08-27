//! Stateless structured-model provider for manuscript claim decomposition.
//!
//! This crate only turns one bounded, citation-bearing manuscript block into
//! structured propositions. It performs no retrieval and owns no persistence.

use std::{fmt, time::Duration};

use async_trait::async_trait;
use nineprofs_research::{
    ManuscriptClaimExtractionBlockInput, ManuscriptClaimExtractionClaimOutput,
    ManuscriptClaimExtractionIdentity, ManuscriptClaimExtractionOutput,
    ManuscriptClaimExtractionProvider, ManuscriptClaimExtractionProviderError,
    ManuscriptClaimExtractionUnassociatedCitation,
};
use reqwest::{Client, StatusCode};
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

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 1_024;
const MAX_REQUEST_BYTES: usize = 512 * 1024;
const ANTHROPIC_VERSION: &str = "2023-06-01";

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
        if !matches!(self.provider.as_str(), "openai" | "anthropic") {
            return Err(ClaimExtractorConfigError::UnsupportedProvider);
        }
        if self.model.trim().is_empty() {
            return Err(ClaimExtractorConfigError::MissingModel);
        }
        if self.api_key_env.trim().is_empty() {
            return Err(ClaimExtractorConfigError::MissingCredentialEnvironment);
        }
        if credential.is_none() {
            return Err(ClaimExtractorConfigError::MissingCredential);
        }
        if let Some(base_url) = &self.base_url {
            let url = base_url
                .parse::<reqwest::Url>()
                .map_err(|_| ClaimExtractorConfigError::InvalidBaseUrl)?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err(ClaimExtractorConfigError::InvalidBaseUrl);
            }
        }
        if self.timeout.is_zero()
            || self.max_response_bytes == 0
            || self.max_output_tokens == 0
            || self.max_response_bytes > 4 * 1024 * 1024
            || self.max_output_tokens > 16_384
        {
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
    client: Client,
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
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("claim extractor HTTP client configuration is valid");
        Self { config, client }
    }

    pub fn config(&self) -> &ClaimExtractorConfig {
        &self.config
    }

    fn endpoint(&self) -> String {
        let base =
            self.config
                .base_url
                .clone()
                .unwrap_or_else(|| match self.config.provider.as_str() {
                    "anthropic" => "https://api.anthropic.com/v1".to_owned(),
                    _ => "https://api.openai.com/v1".to_owned(),
                });
        match self.config.provider.as_str() {
            "anthropic" => format!("{base}/messages"),
            _ => format!("{base}/chat/completions"),
        }
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
        let mut request = self.client.post(self.endpoint()).json(&body);
        request = match self.config.provider.as_str() {
            "anthropic" => request
                .header("x-api-key", credential)
                .header("anthropic-version", ANTHROPIC_VERSION),
            _ => request.bearer_auth(credential),
        };
        let mut response = request.send().await.map_err(map_request_error)?;
        if response
            .content_length()
            .is_some_and(|length| length > self.config.max_response_bytes as u64)
        {
            return Err(ManuscriptClaimExtractionProviderError::ResponseTooLarge);
        }
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(map_request_error)? {
            if bytes.len().saturating_add(chunk.len()) > self.config.max_response_bytes {
                return Err(ManuscriptClaimExtractionProviderError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        if status != StatusCode::OK {
            return Err(ManuscriptClaimExtractionProviderError::Transport);
        }
        Ok(bytes)
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

fn map_request_error(error: reqwest::Error) -> ManuscriptClaimExtractionProviderError {
    if error.is_timeout() {
        ManuscriptClaimExtractionProviderError::Timeout
    } else {
        ManuscriptClaimExtractionProviderError::Transport
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
