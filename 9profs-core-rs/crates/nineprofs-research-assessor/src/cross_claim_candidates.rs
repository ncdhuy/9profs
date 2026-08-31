use std::{fmt, time::Duration};

use async_trait::async_trait;
use nineprofs_research_verification::{
    CrossClaimCandidateDiscoveryInput, CrossClaimCandidateDiscoveryOutput,
    CrossClaimCandidateDiscoveryProvider, CrossClaimCandidateDiscoveryProviderError,
    MAX_CROSS_CLAIM_DISCOVERY_CANDIDATES_PER_WINDOW, MAX_CROSS_CLAIM_DISCOVERY_INPUT_BYTES,
    MAX_CROSS_CLAIM_DISCOVERY_RATIONALE_BYTES,
};
use nineprofs_structured_model::{
    StructuredModelClient, StructuredModelConfig, StructuredModelConfigError,
    StructuredModelProvider, StructuredModelTransportError,
};
use serde::Deserialize;
use serde_json::{Value, json};

pub const CROSS_CLAIM_DISCOVERY_MODEL_VERSION: &str = "model-cross-claim-candidate-discovery-v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 1_024;

const CROSS_CLAIM_DISCOVERY_INSTRUCTION: &str = r#"You are cross-claim-candidate-discovery-v1.

You are NOT deciding truth, factual correctness, or a final contradiction. Select only pairs of supplied manuscript claims that may warrant a later consistency comparison. Use only the supplied claims and no outside knowledge, web search, literature, retrieval, tools, or evidence. Never invent claims or IDs, and never return an ID outside the supplied comparison window.

Do not select a pair merely because both claims share a broad topic. Prefer propositions that may differ materially in stated fact or value, direction, quantity, modality or certainty, causal or associative strength, population or scope, timing, definition, or that appear to restate one another in a way worth checking. A pair may be valid under different scopes. “May conflict” is not “does conflict.”

Return one pure JSON object matching the requested schema. Return at most the permitted number of candidates. Each rationale must be concise and explain only why the pair may be worth comparing. Candidate kinds are potential discovery reasons, not final consistency relations."#;

#[derive(Clone, PartialEq, Eq)]
pub struct CrossClaimCandidateDiscoveryConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    /// Environment variable name only. The credential is resolved per call.
    pub api_key_env: String,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub max_output_tokens: u32,
}

impl fmt::Debug for CrossClaimCandidateDiscoveryConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CrossClaimCandidateDiscoveryConfig")
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

impl Default for CrossClaimCandidateDiscoveryConfig {
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

impl CrossClaimCandidateDiscoveryConfig {
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
        let config = StructuredModelConfig::from_env();
        Self {
            provider: config.provider,
            model: config.model,
            base_url: config.base_url,
            api_key_env: config.api_key_env,
            timeout: config.timeout,
            ..Self::default()
        }
    }

    pub fn is_ready(&self) -> bool {
        StructuredModelProvider::parse(&self.provider).is_ok()
            && !self.model.trim().is_empty()
            && std::env::var(self.api_key_env.trim())
                .ok()
                .is_some_and(|value| !value.trim().is_empty())
    }

    fn credential(&self) -> Option<String> {
        self.shared_config().credential()
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
pub struct ModelCrossClaimCandidateDiscovery {
    config: CrossClaimCandidateDiscoveryConfig,
    client: StructuredModelClient,
}

impl fmt::Debug for ModelCrossClaimCandidateDiscovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCrossClaimCandidateDiscovery")
            .field("config", &self.config)
            .finish()
    }
}

impl ModelCrossClaimCandidateDiscovery {
    pub fn new(config: CrossClaimCandidateDiscoveryConfig) -> Self {
        let client = StructuredModelClient::new(config.shared_config());
        Self { config, client }
    }

    pub fn config(&self) -> &CrossClaimCandidateDiscoveryConfig {
        &self.config
    }

    pub async fn discover_model(
        &self,
        input: CrossClaimCandidateDiscoveryInput,
    ) -> Result<CrossClaimCandidateDiscoveryOutput, CrossClaimCandidateDiscoveryProviderError> {
        validate_input(&input)?;
        let credential = self
            .config
            .credential()
            .ok_or(CrossClaimCandidateDiscoveryProviderError::NotConfigured)?;
        StructuredModelProvider::parse(&self.config.provider)
            .map_err(|_| CrossClaimCandidateDiscoveryProviderError::InvalidConfiguration)?;
        if self.config.model.trim().is_empty() {
            return Err(CrossClaimCandidateDiscoveryProviderError::InvalidConfiguration);
        }
        let prompt = serde_json::to_string(&input)
            .map_err(|_| CrossClaimCandidateDiscoveryProviderError::InvalidInput)?;
        let body = self.request_body(&prompt);
        if serde_json::to_vec(&body)
            .map_err(|_| CrossClaimCandidateDiscoveryProviderError::InvalidInput)?
            .len()
            > MAX_CROSS_CLAIM_DISCOVERY_INPUT_BYTES
        {
            return Err(CrossClaimCandidateDiscoveryProviderError::InputTooLarge);
        }
        let bytes = self
            .client
            .execute_json(&body, &credential)
            .await
            .map_err(map_transport_error)?;
        let value = match self.config.provider.as_str() {
            "openai" => parse_openai_response(&bytes)?,
            "anthropic" => parse_anthropic_response(&bytes)?,
            _ => return Err(CrossClaimCandidateDiscoveryProviderError::InvalidConfiguration),
        };
        let output: CrossClaimCandidateDiscoveryOutput =
            serde_json::from_value(value).map_err(|error| {
                if error.is_syntax() || error.is_eof() {
                    CrossClaimCandidateDiscoveryProviderError::MalformedResponse
                } else {
                    CrossClaimCandidateDiscoveryProviderError::InvalidStructuredOutput
                }
            })?;
        if output.candidates.len() > MAX_CROSS_CLAIM_DISCOVERY_CANDIDATES_PER_WINDOW
            || output.candidates.iter().any(|candidate| {
                candidate.rationale.len() > MAX_CROSS_CLAIM_DISCOVERY_RATIONALE_BYTES
            })
        {
            return Err(CrossClaimCandidateDiscoveryProviderError::InvalidStructuredOutput);
        }
        Ok(output)
    }

    fn request_body(&self, prompt: &str) -> Value {
        match self.config.provider.as_str() {
            "anthropic" => json!({
                "model": self.config.model,
                "system": CROSS_CLAIM_DISCOVERY_INSTRUCTION,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": self.config.max_output_tokens,
                "temperature": 0,
                "tools": [{
                    "name": "cross_claim_candidate_discovery",
                    "description": "Return the strict cross-claim candidate discovery object.",
                    "input_schema": discovery_schema()
                }],
                "tool_choice": {"type": "tool", "name": "cross_claim_candidate_discovery"}
            }),
            _ => json!({
                "model": self.config.model,
                "messages": [
                    {"role": "system", "content": CROSS_CLAIM_DISCOVERY_INSTRUCTION},
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
impl CrossClaimCandidateDiscoveryProvider for ModelCrossClaimCandidateDiscovery {
    fn identity(
        &self,
    ) -> nineprofs_research_verification::CrossClaimCandidateDiscoveryProviderIdentity {
        nineprofs_research_verification::CrossClaimCandidateDiscoveryProviderIdentity {
            provider_id: self.config.provider.clone(),
            implementation_version: CROSS_CLAIM_DISCOVERY_MODEL_VERSION.to_owned(),
            model_id: (!self.config.model.trim().is_empty()).then(|| self.config.model.clone()),
        }
    }

    async fn discover(
        &self,
        input: CrossClaimCandidateDiscoveryInput,
    ) -> Result<CrossClaimCandidateDiscoveryOutput, CrossClaimCandidateDiscoveryProviderError> {
        self.discover_model(input).await
    }
}

fn validate_input(
    input: &CrossClaimCandidateDiscoveryInput,
) -> Result<(), CrossClaimCandidateDiscoveryProviderError> {
    if input.comparison_window_id.trim().is_empty()
        || input.left_batch.is_empty()
        || input.right_batch.is_empty()
    {
        return Err(CrossClaimCandidateDiscoveryProviderError::InvalidInput);
    }
    if serde_json::to_vec(input)
        .map_err(|_| CrossClaimCandidateDiscoveryProviderError::InvalidInput)?
        .len()
        > MAX_CROSS_CLAIM_DISCOVERY_INPUT_BYTES
    {
        return Err(CrossClaimCandidateDiscoveryProviderError::InputTooLarge);
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

fn parse_openai_response(bytes: &[u8]) -> Result<Value, CrossClaimCandidateDiscoveryProviderError> {
    let response: OpenAiResponse = serde_json::from_slice(bytes)
        .map_err(|_| CrossClaimCandidateDiscoveryProviderError::MalformedResponse)?;
    let content = response
        .choices
        .into_iter()
        .next()
        .and_then(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty())
        .ok_or(CrossClaimCandidateDiscoveryProviderError::MalformedResponse)?;
    parse_structured_json(&content)
}

fn parse_anthropic_response(
    bytes: &[u8],
) -> Result<Value, CrossClaimCandidateDiscoveryProviderError> {
    let response: AnthropicResponse = serde_json::from_slice(bytes)
        .map_err(|_| CrossClaimCandidateDiscoveryProviderError::MalformedResponse)?;
    response
        .content
        .into_iter()
        .find(|block| {
            block.block_type == "tool_use"
                && block.name.as_deref() == Some("cross_claim_candidate_discovery")
        })
        .and_then(|block| block.input)
        .ok_or(CrossClaimCandidateDiscoveryProviderError::MalformedResponse)
}

fn parse_structured_json(text: &str) -> Result<Value, CrossClaimCandidateDiscoveryProviderError> {
    let text = text.trim();
    let json_text = if let Some(content) = text.strip_prefix("```json") {
        content
            .strip_suffix("```")
            .map(str::trim)
            .ok_or(CrossClaimCandidateDiscoveryProviderError::MalformedResponse)?
    } else if let Some(content) = text.strip_prefix("```") {
        content
            .strip_suffix("```")
            .map(str::trim)
            .ok_or(CrossClaimCandidateDiscoveryProviderError::MalformedResponse)?
    } else {
        text
    };
    serde_json::from_str(json_text).map_err(|error| {
        if error.is_syntax() || error.is_eof() {
            CrossClaimCandidateDiscoveryProviderError::MalformedResponse
        } else {
            CrossClaimCandidateDiscoveryProviderError::InvalidStructuredOutput
        }
    })
}

fn discovery_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["comparisonWindowId", "candidates"],
        "properties": {
            "comparisonWindowId": {"type": "string"},
            "candidates": {
                "type": "array",
                "maxItems": MAX_CROSS_CLAIM_DISCOVERY_CANDIDATES_PER_WINDOW,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["leftInventoryItemId", "rightInventoryItemId", "candidateKind", "rationale"],
                    "properties": {
                        "leftInventoryItemId": {"type": "string"},
                        "rightInventoryItemId": {"type": "string"},
                        "candidateKind": {"type": "string"},
                        "rationale": {"type": "string", "maxLength": MAX_CROSS_CLAIM_DISCOVERY_RATIONALE_BYTES}
                    }
                }
            }
        }
    })
}

fn map_transport_error(
    error: StructuredModelTransportError,
) -> CrossClaimCandidateDiscoveryProviderError {
    match error {
        StructuredModelTransportError::NotConfigured => {
            CrossClaimCandidateDiscoveryProviderError::NotConfigured
        }
        StructuredModelTransportError::InvalidConfiguration
        | StructuredModelTransportError::ClientBuildFailed => {
            CrossClaimCandidateDiscoveryProviderError::InvalidConfiguration
        }
        StructuredModelTransportError::Timeout => {
            CrossClaimCandidateDiscoveryProviderError::Timeout
        }
        StructuredModelTransportError::Unauthorized => {
            CrossClaimCandidateDiscoveryProviderError::Unauthorized
        }
        StructuredModelTransportError::RateLimited => {
            CrossClaimCandidateDiscoveryProviderError::RateLimited
        }
        StructuredModelTransportError::ProviderUnavailable
        | StructuredModelTransportError::Transport => {
            CrossClaimCandidateDiscoveryProviderError::ProviderUnavailable
        }
        StructuredModelTransportError::ResponseTooLarge => {
            CrossClaimCandidateDiscoveryProviderError::ResponseTooLarge
        }
    }
}

#[allow(dead_code)]
fn _map_config_error(
    error: StructuredModelConfigError,
) -> CrossClaimCandidateDiscoveryProviderError {
    match error {
        StructuredModelConfigError::MissingProvider
        | StructuredModelConfigError::UnsupportedProvider
        | StructuredModelConfigError::MissingModel
        | StructuredModelConfigError::MissingCredentialEnvironment
        | StructuredModelConfigError::MissingCredential
        | StructuredModelConfigError::InvalidBaseUrl
        | StructuredModelConfigError::InvalidLimits => {
            CrossClaimCandidateDiscoveryProviderError::InvalidConfiguration
        }
    }
}
