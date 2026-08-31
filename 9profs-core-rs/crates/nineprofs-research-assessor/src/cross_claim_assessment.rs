use std::{fmt, time::Duration};

use async_trait::async_trait;
use nineprofs_research_verification::{
    CROSS_CLAIM_CONSISTENCY_ASSESSMENT_IMPLEMENTATION_VERSION, CrossClaimConsistencyAssessment,
    CrossClaimConsistencyAssessmentInput, CrossClaimConsistencyAssessmentProvider,
    CrossClaimConsistencyAssessmentProviderError, CrossClaimConsistencyAssessmentProviderIdentity,
    CrossClaimConsistencyRelation, CrossClaimDifferenceDimension,
    MAX_CROSS_CLAIM_ASSESSMENT_RATIONALE_BYTES,
};
use nineprofs_structured_model::{
    StructuredModelClient, StructuredModelConfig, StructuredModelProvider,
    StructuredModelTransportError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 512;
const MAX_REQUEST_BYTES: usize = 256 * 1024;
const MAX_INPUT_FIELD_BYTES: usize = 32 * 1024;

pub const CROSS_CLAIM_CONSISTENCY_ASSESSOR_INSTRUCTION: &str = r#"You are cross-claim-consistency-assessor-v1, a bounded manuscript-internal consistency assessor.

Assess ONLY the relationship between the two supplied manuscript claims. The candidate was selected for review, but its discovery hypothesis, kinds, rationale, comparison window, citation context, and evidence context are intentionally unavailable and must not be inferred. Do not fact-check either claim. Do not use outside knowledge, the web, literature, retrieval, tools, or hidden evidence. Do not judge citation quality, evidence coverage, citation expectation, truth, or global correctness. Do not assign attention, warning, severity, confidence, or a recommendation.

Preserve the claims' negation, direction, quantity, modality or certainty, causal strength, scope or population, temporal qualification, and definitions. A difference on one of those dimensions is not automatically a conflict: it may be compatible, a qualification or refinement, equivalent, not meaningfully comparable, or insufficient context. Use conflict only when the supplied claim wording itself supports an internal inconsistency.

Choose exactly one relation: conflict, compatible, qualification_or_refinement, equivalent_or_restatement, not_meaningfully_comparable, or insufficient_context. Return zero or more unique difference dimensions from: proposition, quantitative, direction, modality_or_certainty, causal_strength, scope_or_population, temporal, definition, other. Dimensions describe a material difference; they do not by themselves determine the relation. Return a concise, audit-safe rationale grounded only in the supplied claim text and manuscript excerpts. Do not reveal hidden reasoning. Return one pure JSON object matching the requested schema."#;

#[derive(Clone, PartialEq, Eq)]
pub struct CrossClaimConsistencyAssessorConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    /// Environment variable name only. The credential is resolved per call.
    pub api_key_env: String,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub max_output_tokens: u32,
}

impl fmt::Debug for CrossClaimConsistencyAssessorConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CrossClaimConsistencyAssessorConfig")
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

impl Default for CrossClaimConsistencyAssessorConfig {
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

impl CrossClaimConsistencyAssessorConfig {
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
pub struct ModelCrossClaimConsistencyAssessor {
    config: CrossClaimConsistencyAssessorConfig,
    client: StructuredModelClient,
}

impl fmt::Debug for ModelCrossClaimConsistencyAssessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelCrossClaimConsistencyAssessor")
            .field("config", &self.config)
            .finish()
    }
}

impl ModelCrossClaimConsistencyAssessor {
    pub fn new(config: CrossClaimConsistencyAssessorConfig) -> Self {
        let client = StructuredModelClient::new(config.shared_config());
        Self { config, client }
    }

    pub fn config(&self) -> &CrossClaimConsistencyAssessorConfig {
        &self.config
    }

    pub async fn assess_model(
        &self,
        input: CrossClaimConsistencyAssessmentInput,
    ) -> Result<CrossClaimConsistencyAssessment, CrossClaimConsistencyAssessmentProviderError> {
        validate_input(&input)?;
        let credential = self
            .config
            .credential()
            .ok_or(CrossClaimConsistencyAssessmentProviderError::NotConfigured)?;
        let provider = StructuredModelProvider::parse(&self.config.provider)
            .map_err(|_| CrossClaimConsistencyAssessmentProviderError::InvalidConfiguration)?;
        if self.config.model.trim().is_empty() {
            return Err(CrossClaimConsistencyAssessmentProviderError::InvalidConfiguration);
        }
        let prompt = build_prompt(&input)
            .map_err(|_| CrossClaimConsistencyAssessmentProviderError::InvalidInput)?;
        let body = self.request_body(&prompt, provider);
        if serde_json::to_vec(&body)
            .map_err(|_| CrossClaimConsistencyAssessmentProviderError::InvalidInput)?
            .len()
            > MAX_REQUEST_BYTES
        {
            return Err(CrossClaimConsistencyAssessmentProviderError::InputTooLarge);
        }
        let bytes = self
            .client
            .execute_json(&body, &credential)
            .await
            .map_err(map_transport_error)?;
        let value = match provider {
            StructuredModelProvider::OpenAi => parse_openai_response(&bytes)?,
            StructuredModelProvider::Anthropic => parse_anthropic_response(&bytes)?,
        };
        let output: StructuredAssessment = serde_json::from_value(value).map_err(|error| {
            if error.is_syntax() || error.is_eof() {
                CrossClaimConsistencyAssessmentProviderError::MalformedResponse
            } else {
                CrossClaimConsistencyAssessmentProviderError::InvalidStructuredOutput
            }
        })?;
        if output.candidate_id != input.candidate_id
            || output.rationale.trim().is_empty()
            || output.rationale.len() > MAX_CROSS_CLAIM_ASSESSMENT_RATIONALE_BYTES
            || has_duplicate_dimensions(&output.dimensions)
        {
            return Err(CrossClaimConsistencyAssessmentProviderError::InvalidStructuredOutput);
        }
        Ok(CrossClaimConsistencyAssessment {
            candidate_id: output.candidate_id,
            relation: output.relation,
            dimensions: output.dimensions,
            rationale: output.rationale,
        })
    }

    fn request_body(&self, prompt: &str, provider: StructuredModelProvider) -> Value {
        match provider {
            StructuredModelProvider::Anthropic => json!({
                "model": self.config.model,
                "system": CROSS_CLAIM_CONSISTENCY_ASSESSOR_INSTRUCTION,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": self.config.max_output_tokens,
                "temperature": 0,
                "tools": [{
                    "name": "cross_claim_consistency_assessment",
                    "description": "Return the strict cross-claim consistency assessment object.",
                    "input_schema": assessment_schema()
                }],
                "tool_choice": {"type": "tool", "name": "cross_claim_consistency_assessment"}
            }),
            StructuredModelProvider::OpenAi => json!({
                "model": self.config.model,
                "messages": [
                    {"role": "system", "content": CROSS_CLAIM_CONSISTENCY_ASSESSOR_INSTRUCTION},
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
impl CrossClaimConsistencyAssessmentProvider for ModelCrossClaimConsistencyAssessor {
    fn identity(&self) -> CrossClaimConsistencyAssessmentProviderIdentity {
        CrossClaimConsistencyAssessmentProviderIdentity {
            provider_id: self.config.provider.clone(),
            assessor_implementation_version:
                CROSS_CLAIM_CONSISTENCY_ASSESSMENT_IMPLEMENTATION_VERSION.to_owned(),
            model_id: (!self.config.model.trim().is_empty()).then(|| self.config.model.clone()),
        }
    }

    async fn assess(
        &self,
        input: CrossClaimConsistencyAssessmentInput,
    ) -> Result<CrossClaimConsistencyAssessment, CrossClaimConsistencyAssessmentProviderError> {
        self.assess_model(input).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StructuredAssessment {
    candidate_id: String,
    relation: CrossClaimConsistencyRelation,
    dimensions: Vec<CrossClaimDifferenceDimension>,
    rationale: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptInput<'a> {
    candidate_id: &'a str,
    left: PromptClaim<'a>,
    right: PromptClaim<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptClaim<'a> {
    inventory_item_id: &'a str,
    claim_text: &'a str,
    source_excerpt: &'a str,
    review_kind: &'a nineprofs_research::ClaimReviewKind,
    block_kind: &'a nineprofs_research::ManuscriptClaimInventoryBlockKind,
    block_ordinal: u32,
}

fn build_prompt(input: &CrossClaimConsistencyAssessmentInput) -> Result<String, serde_json::Error> {
    serde_json::to_string(&PromptInput {
        candidate_id: &input.candidate_id,
        left: PromptClaim {
            inventory_item_id: &input.left.inventory_item_id,
            claim_text: &input.left.claim_text,
            source_excerpt: &input.left.source_excerpt,
            review_kind: &input.left.review_kind,
            block_kind: &input.left.block_kind,
            block_ordinal: input.left.block_ordinal,
        },
        right: PromptClaim {
            inventory_item_id: &input.right.inventory_item_id,
            claim_text: &input.right.claim_text,
            source_excerpt: &input.right.source_excerpt,
            review_kind: &input.right.review_kind,
            block_kind: &input.right.block_kind,
            block_ordinal: input.right.block_ordinal,
        },
    })
}

fn validate_input(
    input: &CrossClaimConsistencyAssessmentInput,
) -> Result<(), CrossClaimConsistencyAssessmentProviderError> {
    if input.candidate_id.trim().is_empty()
        || input.left.inventory_item_id.trim().is_empty()
        || input.right.inventory_item_id.trim().is_empty()
        || input.left.claim_text.trim().is_empty()
        || input.right.claim_text.trim().is_empty()
    {
        return Err(CrossClaimConsistencyAssessmentProviderError::InvalidInput);
    }
    if input.candidate_id.len() > 512
        || input.left.inventory_item_id.len() > 512
        || input.right.inventory_item_id.len() > 512
        || input.left.claim_text.len() > MAX_INPUT_FIELD_BYTES
        || input.right.claim_text.len() > MAX_INPUT_FIELD_BYTES
        || input.left.source_excerpt.len() > MAX_INPUT_FIELD_BYTES
        || input.right.source_excerpt.len() > MAX_INPUT_FIELD_BYTES
    {
        return Err(CrossClaimConsistencyAssessmentProviderError::InputTooLarge);
    }
    Ok(())
}

fn has_duplicate_dimensions(dimensions: &[CrossClaimDifferenceDimension]) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    dimensions.iter().any(|dimension| !seen.insert(dimension))
}

fn assessment_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["candidateId", "relation", "dimensions", "rationale"],
        "properties": {
            "candidateId": {"type": "string"},
            "relation": {"type": "string", "enum": [
                "conflict", "compatible", "qualification_or_refinement",
                "equivalent_or_restatement", "not_meaningfully_comparable", "insufficient_context"
            ]},
            "dimensions": {"type": "array", "uniqueItems": true, "items": {"type": "string", "enum": [
                "proposition", "quantitative", "direction", "modality_or_certainty",
                "causal_strength", "scope_or_population", "temporal", "definition", "other"
            ]}},
            "rationale": {"type": "string", "minLength": 1, "maxLength": MAX_CROSS_CLAIM_ASSESSMENT_RATIONALE_BYTES}
        }
    })
}

fn parse_openai_response(
    bytes: &[u8],
) -> Result<Value, CrossClaimConsistencyAssessmentProviderError> {
    let body: Value = serde_json::from_slice(bytes)
        .map_err(|_| CrossClaimConsistencyAssessmentProviderError::MalformedResponse)?;
    let content = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .ok_or(CrossClaimConsistencyAssessmentProviderError::MalformedResponse)?;
    match content {
        Value::String(value) => serde_json::from_str(value)
            .map_err(|_| CrossClaimConsistencyAssessmentProviderError::InvalidStructuredOutput),
        value => Ok(value.clone()),
    }
}

fn parse_anthropic_response(
    bytes: &[u8],
) -> Result<Value, CrossClaimConsistencyAssessmentProviderError> {
    let body: Value = serde_json::from_slice(bytes)
        .map_err(|_| CrossClaimConsistencyAssessmentProviderError::MalformedResponse)?;
    body.get("content")
        .and_then(Value::as_array)
        .and_then(|content| {
            content.iter().find_map(|block| {
                (block.get("type").and_then(Value::as_str) == Some("tool_use")
                    && block.get("name").and_then(Value::as_str)
                        == Some("cross_claim_consistency_assessment"))
                .then(|| block.get("input"))
                .flatten()
            })
        })
        .cloned()
        .ok_or(CrossClaimConsistencyAssessmentProviderError::MalformedResponse)
}

fn map_transport_error(
    error: StructuredModelTransportError,
) -> CrossClaimConsistencyAssessmentProviderError {
    match error {
        StructuredModelTransportError::NotConfigured => {
            CrossClaimConsistencyAssessmentProviderError::NotConfigured
        }
        StructuredModelTransportError::InvalidConfiguration
        | StructuredModelTransportError::ClientBuildFailed => {
            CrossClaimConsistencyAssessmentProviderError::InvalidConfiguration
        }
        StructuredModelTransportError::Timeout => {
            CrossClaimConsistencyAssessmentProviderError::Timeout
        }
        StructuredModelTransportError::Unauthorized => {
            CrossClaimConsistencyAssessmentProviderError::Unauthorized
        }
        StructuredModelTransportError::RateLimited => {
            CrossClaimConsistencyAssessmentProviderError::RateLimited
        }
        StructuredModelTransportError::ProviderUnavailable
        | StructuredModelTransportError::Transport => {
            CrossClaimConsistencyAssessmentProviderError::ProviderUnavailable
        }
        StructuredModelTransportError::ResponseTooLarge => {
            CrossClaimConsistencyAssessmentProviderError::ResponseTooLarge
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nineprofs_research::{ClaimReviewKind, ManuscriptClaimInventoryBlockKind};

    fn input() -> CrossClaimConsistencyAssessmentInput {
        CrossClaimConsistencyAssessmentInput {
            candidate_id: "candidate-1".to_owned(),
            left: nineprofs_research_verification::CrossClaimConsistencyClaim {
                inventory_item_id: "item-1".to_owned(),
                claim_text: "A increases B.".to_owned(),
                source_excerpt: "Results paragraph.".to_owned(),
                review_kind: ClaimReviewKind::ManuscriptInternal,
                block_kind: ManuscriptClaimInventoryBlockKind::Paragraph,
                block_ordinal: 1,
            },
            right: nineprofs_research_verification::CrossClaimConsistencyClaim {
                inventory_item_id: "item-2".to_owned(),
                claim_text: "A does not increase B.".to_owned(),
                source_excerpt: "Discussion paragraph.".to_owned(),
                review_kind: ClaimReviewKind::Interpretive,
                block_kind: ManuscriptClaimInventoryBlockKind::Paragraph,
                block_ordinal: 2,
            },
        }
    }

    #[test]
    fn prompt_contains_only_semantic_claim_fields() {
        let prompt = build_prompt(&input()).expect("prompt");
        assert!(prompt.contains("candidateId"));
        assert!(prompt.contains("claimText"));
        assert!(!prompt.contains("candidateKinds"));
        assert!(!prompt.contains("rationale"));
        assert!(!prompt.contains("citation"));
        assert!(!prompt.contains("evidence"));
        assert!(!prompt.contains("comparisonWindow"));
    }

    #[test]
    fn schema_is_closed_and_relation_set_is_exact() {
        let schema = assessment_schema();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["relation"]["enum"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
        assert_eq!(
            schema["properties"]["dimensions"]["items"]["enum"]
                .as_array()
                .unwrap()
                .len(),
            9
        );
    }

    #[test]
    fn provider_response_parsers_accept_only_supported_wire_shapes() {
        let openai = br#"{"choices":[{"message":{"content":"{\"candidateId\":\"candidate-1\",\"relation\":\"compatible\",\"dimensions\":[],\"rationale\":\"same proposition\"}"}}]}"#;
        let value = parse_openai_response(openai).expect("openai response");
        assert_eq!(value["candidateId"], "candidate-1");

        let anthropic = br#"{"content":[{"type":"tool_use","name":"cross_claim_consistency_assessment","input":{"candidateId":"candidate-1","relation":"compatible","dimensions":[],"rationale":"same proposition"}}]}"#;
        let value = parse_anthropic_response(anthropic).expect("anthropic response");
        assert_eq!(value["relation"], "compatible");
        assert!(parse_anthropic_response(br#"{"content":[{"type":"text","text":"{}"}]}"#).is_err());
    }
}
