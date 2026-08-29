use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use nineprofs_research_verification::{
    CitationExpectation, CitationExpectationAssessment, CitationExpectationInput,
    CitationExpectationProvider, CitationExpectationProviderError,
    CitationExpectationProviderIdentity, MAX_EXPECTATION_RATIONALE_BYTES,
};
use nineprofs_structured_model::{
    StructuredModelClient, StructuredModelConfig, StructuredModelProvider,
    StructuredModelTransportError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const EXPECTATION_ASSESSMENT_IMPLEMENTATION_VERSION: &str = "model-citation-expectation-v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 512;
const MAX_REQUEST_BYTES: usize = 256 * 1024;
const MAX_INPUT_FIELD_BYTES: usize = 32 * 1024;
const EXPECTATION_INSTRUCTION: &str = r#"You are citation-expectation-v1, a bounded scholarly-writing assessor.

Judge only whether the supplied proposition as written would ordinarily call for external scholarly source or evidence support under general scholarly-writing norms. ClaimReviewKind is context, not a rule. Do not assess truth, factual correctness, citation presence, citation quality, evidence coverage, verification state, or source adequacy. Do not retrieve, search, or use outside knowledge. Manuscript-internal results generally need internal data or method support, not an automatically required external citation.

Return one pure JSON object matching the requested schema. Use a concise, audit-safe rationale explaining the scholarly expectation. Never return citation IDs, evidence IDs, source IDs, page ranges, recommendations, or hidden reasoning."#;

#[derive(Clone, PartialEq, Eq)]
pub struct CitationExpectationAssessorConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    /// Environment variable name only. The credential is resolved per call.
    pub api_key_env: String,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub max_output_tokens: u32,
}

impl fmt::Debug for CitationExpectationAssessorConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CitationExpectationAssessorConfig")
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

impl Default for CitationExpectationAssessorConfig {
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

impl CitationExpectationAssessorConfig {
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
            provider,
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
        let provider =
            std::env::var("NINEPROFS_CITATION_EXPECTATION_ASSESSOR_PROVIDER").unwrap_or_default();
        let model =
            std::env::var("NINEPROFS_CITATION_EXPECTATION_ASSESSOR_MODEL").unwrap_or_default();
        let base_url = std::env::var("NINEPROFS_CITATION_EXPECTATION_ASSESSOR_BASE_URL").ok();
        let api_key_env = std::env::var("NINEPROFS_CITATION_EXPECTATION_ASSESSOR_API_KEY_ENV")
            .unwrap_or_default();
        let mut config = Self::new(provider, model, base_url, api_key_env);
        if let Ok(value) = std::env::var("NINEPROFS_CITATION_EXPECTATION_ASSESSOR_TIMEOUT_MS")
            && let Ok(milliseconds) = value.parse::<u64>()
        {
            config.timeout = Duration::from_millis(milliseconds.clamp(100, 120_000));
        }
        config
    }

    pub fn is_ready(&self) -> bool {
        StructuredModelProvider::parse(&self.provider).is_ok()
            && !self.model.trim().is_empty()
            && std::env::var(self.api_key_env.trim())
                .ok()
                .is_some_and(|value| !value.trim().is_empty())
    }

    fn credential(&self) -> Option<String> {
        std::env::var(self.api_key_env.trim()).ok()
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

#[derive(Clone)]
pub struct ModelCitationExpectationAssessor {
    config: CitationExpectationAssessorConfig,
    client: StructuredModelClient,
}

impl fmt::Debug for ModelCitationExpectationAssessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCitationExpectationAssessor")
            .field("config", &self.config)
            .finish()
    }
}

impl ModelCitationExpectationAssessor {
    pub fn new(config: CitationExpectationAssessorConfig) -> Self {
        let client = StructuredModelClient::new(config.shared_config());
        Self { config, client }
    }

    pub fn config(&self) -> &CitationExpectationAssessorConfig {
        &self.config
    }

    pub async fn assess_model(
        &self,
        input: CitationExpectationInput,
    ) -> Result<CitationExpectationAssessment, CitationExpectationProviderError> {
        validate_input(&input)?;
        let credential = self
            .config
            .credential()
            .ok_or(CitationExpectationProviderError::NotConfigured)?;
        StructuredModelProvider::parse(&self.config.provider)
            .map_err(|_| CitationExpectationProviderError::InvalidConfiguration)?;
        if self.config.model.trim().is_empty() {
            return Err(CitationExpectationProviderError::InvalidConfiguration);
        }
        let prompt =
            build_prompt(&input).map_err(|_| CitationExpectationProviderError::InvalidInput)?;
        let body = self.request_body(&prompt);
        if serde_json::to_vec(&body)
            .map_err(|_| CitationExpectationProviderError::InvalidInput)?
            .len()
            > MAX_REQUEST_BYTES
        {
            return Err(CitationExpectationProviderError::InputTooLarge);
        }
        let bytes = self
            .client
            .execute_json(&body, &credential)
            .await
            .map_err(map_transport_error)?;
        let value = match self.config.provider.as_str() {
            "openai" => parse_openai_response(&bytes)?,
            "anthropic" => parse_anthropic_response(&bytes)?,
            _ => return Err(CitationExpectationProviderError::InvalidConfiguration),
        };
        let output: StructuredExpectation = serde_json::from_value(value).map_err(|error| {
            if error.is_syntax() || error.is_eof() {
                CitationExpectationProviderError::MalformedResponse
            } else {
                CitationExpectationProviderError::InvalidStructuredOutput
            }
        })?;
        if output.item_id != input.item_id
            || output.rationale.len() > MAX_EXPECTATION_RATIONALE_BYTES
        {
            return Err(CitationExpectationProviderError::InvalidStructuredOutput);
        }
        Ok(CitationExpectationAssessment {
            item_id: output.item_id,
            expectation: output.expectation,
            rationale: output.rationale,
        })
    }

    fn request_body(&self, prompt: &str) -> Value {
        match self.config.provider.as_str() {
            "anthropic" => json!({
                "model": self.config.model,
                "system": EXPECTATION_INSTRUCTION,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": self.config.max_output_tokens,
                "temperature": 0,
                "tools": [{
                    "name": "citation_expectation",
                    "description": "Return the strict citation expectation object.",
                    "input_schema": assessment_schema()
                }],
                "tool_choice": {"type": "tool", "name": "citation_expectation"}
            }),
            _ => json!({
                "model": self.config.model,
                "messages": [
                    {"role": "system", "content": EXPECTATION_INSTRUCTION},
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0,
                "max_tokens": self.config.max_output_tokens,
                "response_format": {"type": "json_object"}
            }),
        }
    }
}

#[async_trait]
impl CitationExpectationProvider for ModelCitationExpectationAssessor {
    fn identity(&self) -> CitationExpectationProviderIdentity {
        CitationExpectationProviderIdentity {
            provider_id: self.config.provider.clone(),
            assessor_version: EXPECTATION_ASSESSMENT_IMPLEMENTATION_VERSION.to_owned(),
            model_id: (!self.config.model.trim().is_empty()).then(|| self.config.model.clone()),
        }
    }

    async fn assess(
        &self,
        input: CitationExpectationInput,
    ) -> Result<CitationExpectationAssessment, CitationExpectationProviderError> {
        self.assess_model(input).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StructuredExpectation {
    item_id: String,
    expectation: CitationExpectation,
    rationale: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptInput<'a> {
    item_id: &'a str,
    claim_text: &'a str,
    source_excerpt: &'a str,
    review_kind: &'a nineprofs_research::ClaimReviewKind,
    block_kind: &'a nineprofs_research::ManuscriptClaimInventoryBlockKind,
}

fn build_prompt(input: &CitationExpectationInput) -> Result<String, serde_json::Error> {
    serde_json::to_string(&PromptInput {
        item_id: &input.item_id,
        claim_text: &input.claim_text,
        source_excerpt: &input.source_excerpt,
        review_kind: &input.review_kind,
        block_kind: &input.block_kind,
    })
}

fn validate_input(
    input: &CitationExpectationInput,
) -> Result<(), CitationExpectationProviderError> {
    if input.item_id.trim().is_empty()
        || input.claim_text.trim().is_empty()
        || input.source_excerpt.trim().is_empty()
    {
        return Err(CitationExpectationProviderError::InvalidInput);
    }
    if input.item_id.len() > 512
        || input.claim_text.len() > MAX_INPUT_FIELD_BYTES
        || input.source_excerpt.len() > MAX_INPUT_FIELD_BYTES
    {
        return Err(CitationExpectationProviderError::InputTooLarge);
    }
    Ok(())
}

fn assessment_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["itemId", "expectation", "rationale"],
        "properties": {
            "itemId": {"type": "string"},
            "expectation": {"type": "string", "enum": [
                "external_evidence_expected", "external_evidence_context_dependent",
                "manuscript_internal_support", "no_external_citation_expected", "uncertain"
            ]},
            "rationale": {"type": "string", "maxLength": MAX_EXPECTATION_RATIONALE_BYTES}
        }
    })
}

fn parse_openai_response(bytes: &[u8]) -> Result<Value, CitationExpectationProviderError> {
    let body: Value = serde_json::from_slice(bytes)
        .map_err(|_| CitationExpectationProviderError::MalformedResponse)?;
    let content = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .ok_or(CitationExpectationProviderError::MalformedResponse)?;
    match content {
        Value::String(value) => serde_json::from_str(value)
            .map_err(|_| CitationExpectationProviderError::InvalidStructuredOutput),
        value => Ok(value.clone()),
    }
}

fn parse_anthropic_response(bytes: &[u8]) -> Result<Value, CitationExpectationProviderError> {
    let body: Value = serde_json::from_slice(bytes)
        .map_err(|_| CitationExpectationProviderError::MalformedResponse)?;
    body.get("content")
        .and_then(Value::as_array)
        .and_then(|content| {
            content.iter().find_map(|block| {
                (block.get("type").and_then(Value::as_str) == Some("tool_use")
                    && block.get("name").and_then(Value::as_str) == Some("citation_expectation"))
                .then(|| block.get("input"))
                .flatten()
            })
        })
        .cloned()
        .ok_or(CitationExpectationProviderError::MalformedResponse)
}

fn map_transport_error(error: StructuredModelTransportError) -> CitationExpectationProviderError {
    match error {
        StructuredModelTransportError::NotConfigured => {
            CitationExpectationProviderError::NotConfigured
        }
        StructuredModelTransportError::InvalidConfiguration
        | StructuredModelTransportError::ClientBuildFailed => {
            CitationExpectationProviderError::InvalidConfiguration
        }
        StructuredModelTransportError::Timeout => CitationExpectationProviderError::Timeout,
        StructuredModelTransportError::Unauthorized => {
            CitationExpectationProviderError::Unauthorized
        }
        StructuredModelTransportError::RateLimited => CitationExpectationProviderError::RateLimited,
        StructuredModelTransportError::ProviderUnavailable
        | StructuredModelTransportError::Transport => {
            CitationExpectationProviderError::ProviderUnavailable
        }
        StructuredModelTransportError::ResponseTooLarge => {
            CitationExpectationProviderError::ResponseTooLarge
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> CitationExpectationInput {
        CitationExpectationInput {
            item_id: "item-1".to_owned(),
            claim_text: "The intervention reduced readmission.".to_owned(),
            source_excerpt: "The intervention reduced readmission in the cohort.".to_owned(),
            review_kind: nineprofs_research::ClaimReviewKind::ExternalEvidence,
            block_kind: nineprofs_research::ManuscriptClaimInventoryBlockKind::Paragraph,
        }
    }

    #[test]
    fn prompt_contains_only_blind_semantic_input() {
        let prompt = build_prompt(&input()).expect("prompt should serialize");
        let value: Value = serde_json::from_str(&prompt).expect("prompt should be JSON");
        let mut keys = value
            .as_object()
            .expect("prompt object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        let mut expected = vec![
            "itemId",
            "claimText",
            "sourceExcerpt",
            "reviewKind",
            "blockKind",
        ];
        expected.sort();
        assert_eq!(keys, expected);
        assert!(!prompt.contains("supportCount"));
        assert!(!prompt.contains("citationOccurrenceId"));
        assert!(!prompt.contains("evidenceCount"));
        assert!(!prompt.contains("structuralCitationState"));
    }

    #[test]
    fn schema_and_parser_enforce_closed_set_contract() {
        let schema = assessment_schema();
        let expectations = schema["properties"]["expectation"]["enum"]
            .as_array()
            .expect("expectation enum");
        assert_eq!(expectations.len(), 5);
        assert!(
            serde_json::from_value::<StructuredExpectation>(json!({
                "itemId": "item-1",
                "expectation": "not-a-real-value",
                "rationale": "bounded"
            }))
            .is_err()
        );
    }

    #[test]
    fn provider_response_parsers_extract_expected_structured_object() {
        let expected = json!({
            "itemId": "item-1",
            "expectation": "external_evidence_expected",
            "rationale": "The proposition is an empirical result."
        });
        let openai = json!({
            "choices": [{"message": {"content": expected.to_string()}}]
        });
        assert_eq!(
            parse_openai_response(&serde_json::to_vec(&openai).unwrap()).unwrap(),
            expected
        );

        let anthropic = json!({
            "content": [{
                "type": "tool_use",
                "name": "citation_expectation",
                "input": expected
            }]
        });
        assert_eq!(
            parse_anthropic_response(&serde_json::to_vec(&anthropic).unwrap()).unwrap(),
            expected
        );
    }
}
