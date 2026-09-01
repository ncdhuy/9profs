use std::collections::{BTreeMap, BTreeSet};

use nineprofs_structured_model::{
    StructuredModelClient, StructuredModelConfig, StructuredModelProvider,
    StructuredModelTransportError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{DocumentMapLocator, Finding, FindingEvidence, ReviewAuthorityReference};

pub const REVIEW_SYNTHESIS_CONTRACT_VERSION: &str = "research-review-synthesis-v0.1";

const MAX_REVIEW_SYNTHESIS_REQUEST_BYTES: usize = 512 * 1024;
const MIN_REVIEW_SYNTHESIS_OUTPUT_TOKENS: u32 = 8_192;
const REVIEW_SYNTHESIS_INSTRUCTION: &str = r#"You are research-review-synthesis-v0.1, a bounded review-finding consolidator.

Use only the supplied validated Findings. Group Findings only when they describe substantially the same underlying manuscript problem. Preserve distinct substantive issues even when they share a section, locator, task kind, or authority pack. Complementary Findings may share one group when each adds evidence or authority for that same problem.

Do not discover a new manuscript problem. Do not add facts, evidence, manuscript locators, authorities, regulation requirements, citations, or edits. Every supplied Finding ID must appear exactly once across groups. Keep uncertainty and expert-review wording when present; do not strengthen a cautious source statement into a definitive claim. Write concise, human-comprehensible statements and reasoning based only on group members. Priority rank is an ordering only: 1 is highest, larger numbers come later. Return only the requested JSON object."#;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSynthesis {
    pub findings: Vec<ConsolidatedFinding>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsolidatedFinding {
    pub id: String,
    pub source_finding_ids: Vec<String>,
    pub statement: String,
    pub explanation: String,
    pub manuscript_locators: Vec<DocumentMapLocator>,
    pub evidence: Vec<FindingEvidence>,
    pub authority_references: Vec<ReviewAuthorityReference>,
    pub priority_rank: u32,
}

#[derive(Debug, Error)]
pub enum ReviewSynthesisError {
    #[error("review synthesis response is malformed")]
    MalformedResponse,
    #[error("review synthesis response is invalid: {0}")]
    InvalidStructuredOutput(String),
    #[error("review synthesis model is not configured")]
    NotConfigured,
    #[error("review synthesis model configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("review synthesis model transport failed: {0}")]
    Transport(String),
    #[error("review synthesis request exceeds size limit")]
    RequestTooLarge,
    #[error("review synthesis serialization failed: {0}")]
    Serialization(String),
}

impl ReviewSynthesisError {
    pub(crate) fn diagnostic_category(&self) -> &'static str {
        match self {
            Self::MalformedResponse | Self::InvalidStructuredOutput(_) => {
                "synthesis_parsing_validation"
            }
            Self::NotConfigured | Self::InvalidConfiguration(_) => "model_configuration",
            Self::Transport(message) if message.contains("timed out") => "synthesis_timeout",
            Self::Transport(_) => "synthesis_transport",
            Self::RequestTooLarge => "synthesis_input_too_large",
            Self::Serialization(_) => "synthesis_serialization",
        }
    }
}

#[derive(Clone)]
pub struct ReviewSynthesisExecutor {
    config: StructuredModelConfig,
    client: StructuredModelClient,
}

impl std::fmt::Debug for ReviewSynthesisExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReviewSynthesisExecutor")
            .field("config", &self.config)
            .finish()
    }
}

impl ReviewSynthesisExecutor {
    pub fn from_env() -> Self {
        Self::new(StructuredModelConfig::from_env())
    }

    pub fn new(config: StructuredModelConfig) -> Self {
        let client = StructuredModelClient::new(config.clone());
        Self { config, client }
    }

    pub async fn synthesize(
        &self,
        findings: &[Finding],
    ) -> Result<ReviewSynthesis, ReviewSynthesisError> {
        let known_findings = index_findings(findings)?;
        if findings.is_empty() {
            return Ok(ReviewSynthesis { findings: vec![] });
        }

        let credential = self
            .config
            .credential()
            .ok_or(ReviewSynthesisError::NotConfigured)?;
        self.config
            .validate(Some(&credential))
            .map_err(|error| ReviewSynthesisError::InvalidConfiguration(error.to_string()))?;
        let body = request_body(&self.config, findings)?;
        let response = self
            .client
            .execute_json(&body, &credential)
            .await
            .map_err(map_transport_error)?;
        let groups = parse_response(&self.config.provider, &response)?;
        build_synthesis(&known_findings, groups)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SynthesisInput<'a> {
    contract_version: &'static str,
    findings: &'a [Finding],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WireSynthesisResponse {
    groups: Vec<WireSynthesisGroup>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WireSynthesisGroup {
    source_finding_ids: Vec<String>,
    statement: String,
    explanation: String,
    priority_rank: u32,
}

fn index_findings<'a>(
    findings: &'a [Finding],
) -> Result<BTreeMap<String, &'a Finding>, ReviewSynthesisError> {
    let mut known = BTreeMap::new();
    for finding in findings {
        if finding.id.trim().is_empty() {
            return Err(ReviewSynthesisError::InvalidStructuredOutput(
                "input finding ID is empty".to_owned(),
            ));
        }
        if known.insert(finding.id.clone(), finding).is_some() {
            return Err(ReviewSynthesisError::InvalidStructuredOutput(format!(
                "duplicate input finding ID: {}",
                finding.id
            )));
        }
    }
    Ok(known)
}

fn build_synthesis(
    known_findings: &BTreeMap<String, &Finding>,
    groups: Vec<WireSynthesisGroup>,
) -> Result<ReviewSynthesis, ReviewSynthesisError> {
    let mut assigned = BTreeSet::new();
    let mut consolidated = Vec::with_capacity(groups.len());

    for group in groups {
        if group.source_finding_ids.is_empty() {
            return Err(ReviewSynthesisError::InvalidStructuredOutput(
                "synthesis group is empty".to_owned(),
            ));
        }
        if group.statement.trim().is_empty() {
            return Err(ReviewSynthesisError::InvalidStructuredOutput(
                "synthesized statement is empty".to_owned(),
            ));
        }
        if group.explanation.trim().is_empty() {
            return Err(ReviewSynthesisError::InvalidStructuredOutput(
                "synthesized explanation is empty".to_owned(),
            ));
        }
        if group.priority_rank == 0 {
            return Err(ReviewSynthesisError::InvalidStructuredOutput(
                "synthesis priority rank must be positive".to_owned(),
            ));
        }

        let mut source_finding_ids = group.source_finding_ids;
        source_finding_ids.sort();
        let mut members = Vec::with_capacity(source_finding_ids.len());
        for source_finding_id in &source_finding_ids {
            let Some(finding) = known_findings.get(source_finding_id) else {
                return Err(ReviewSynthesisError::InvalidStructuredOutput(format!(
                    "unknown source finding ID: {source_finding_id}"
                )));
            };
            if !assigned.insert(source_finding_id.clone()) {
                return Err(ReviewSynthesisError::InvalidStructuredOutput(format!(
                    "source finding assigned more than once: {source_finding_id}"
                )));
            }
            members.push(*finding);
        }

        if has_uncertain_source(&members) && !has_uncertainty_signal(&group.statement) {
            return Err(ReviewSynthesisError::InvalidStructuredOutput(format!(
                "synthesized statement strengthens uncertain source wording: {}",
                group.statement
            )));
        }

        let mut manuscript_locators = Vec::new();
        let mut evidence = Vec::new();
        let mut authority_references = Vec::new();
        for finding in members {
            for locator in &finding.manuscript_locators {
                push_unique(&mut manuscript_locators, locator.clone());
            }
            for item in &finding.evidence {
                push_unique(&mut evidence, item.clone());
            }
            for reference in &finding.authority_references {
                push_unique(&mut authority_references, reference.clone());
            }
        }

        consolidated.push(ConsolidatedFinding {
            id: format!("synthesis:{}", source_finding_ids[0]),
            source_finding_ids,
            statement: group.statement,
            explanation: group.explanation,
            manuscript_locators,
            evidence,
            authority_references,
            priority_rank: group.priority_rank,
        });
    }

    if assigned.len() != known_findings.len() {
        let missing = known_findings
            .keys()
            .filter(|id| !assigned.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        return Err(ReviewSynthesisError::InvalidStructuredOutput(format!(
            "source findings omitted from synthesis: {}",
            missing.join(", ")
        )));
    }

    consolidated.sort_by(|left, right| {
        left.priority_rank
            .cmp(&right.priority_rank)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(ReviewSynthesis {
        findings: consolidated,
    })
}

fn push_unique<T: Eq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

fn has_uncertain_source(findings: &[&Finding]) -> bool {
    findings
        .iter()
        .any(|finding| has_explicit_uncertainty_signal(&finding.statement))
}

fn has_explicit_uncertainty_signal(text: &str) -> bool {
    let normalized = text.to_lowercase();
    [
        "may ",
        "might ",
        "could ",
        "possibly",
        "unclear",
        "uncertain",
        "not clear",
        "not known",
        "not established",
        "cannot determine",
        "requires expert review",
        "expert review",
        "có thể",
        "có khả năng",
        "chưa rõ",
        "không rõ",
        "chưa thể",
        "cần chuyên gia",
    ]
    .iter()
    .any(|signal| normalized.contains(signal))
}

fn has_uncertainty_signal(text: &str) -> bool {
    let normalized = text.to_lowercase();
    has_explicit_uncertainty_signal(text)
        || [
            "unsupported",
            "not supported",
            "lack of evidence",
            "lack ",
            "without evidence",
            "without reporting",
            "without corresponding",
            "not reported",
            "not demonstrated",
            "not evidenced",
            "does not fully",
            "does not demonstrate",
            "does not establish",
            "not fully",
            "not directly",
            "not enough",
            "unsubstantiated",
            "insufficient",
            "exceed",
            "beyond",
            "not warranted",
            "not justified",
            "conflict with",
            "chưa được chứng minh",
            "chưa",
            "thiếu",
            "vượt quá",
            "không có",
            "không đủ",
            "không có bằng chứng",
            "chưa có bằng chứng",
            "không được hỗ trợ",
            "không thể chứng minh",
        ]
        .iter()
        .any(|signal| normalized.contains(signal))
}

fn request_body(
    config: &StructuredModelConfig,
    findings: &[Finding],
) -> Result<Value, ReviewSynthesisError> {
    let input_json = serde_json::to_string(&SynthesisInput {
        contract_version: REVIEW_SYNTHESIS_CONTRACT_VERSION,
        findings,
    })
    .map_err(|error| ReviewSynthesisError::Serialization(error.to_string()))?;
    let prompt = format!("Consolidate this exact validated Finding input JSON:\n{input_json}");
    let schema = synthesis_schema(findings.len());
    let body = match config
        .provider()
        .map_err(|error| ReviewSynthesisError::InvalidConfiguration(error.to_string()))?
    {
        StructuredModelProvider::Anthropic => json!({
            "model": config.model,
            "max_tokens": config.max_output_tokens,
            "temperature": 0,
            "system": REVIEW_SYNTHESIS_INSTRUCTION,
            "messages": [{"role": "user", "content": prompt}],
            "tools": [{
                "name": "synthesize_review_findings",
                "description": "Return strict consolidated manuscript review findings.",
                "input_schema": schema
            }],
            "tool_choice": {"type": "tool", "name": "synthesize_review_findings"}
        }),
        StructuredModelProvider::OpenAi => json!({
            "model": config.model,
            "max_completion_tokens": config.max_output_tokens.max(MIN_REVIEW_SYNTHESIS_OUTPUT_TOKENS),
            "messages": [
                {"role": "system", "content": REVIEW_SYNTHESIS_INSTRUCTION},
                {"role": "user", "content": prompt}
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "research_review_synthesis",
                    "strict": true,
                    "schema": schema
                }
            }
        }),
    };
    let body_bytes = serde_json::to_vec(&body)
        .map_err(|error| ReviewSynthesisError::Serialization(error.to_string()))?;
    if body_bytes.len() > MAX_REVIEW_SYNTHESIS_REQUEST_BYTES {
        return Err(ReviewSynthesisError::RequestTooLarge);
    }
    Ok(body)
}

fn synthesis_schema(max_groups: usize) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["groups"],
        "properties": {
            "groups": {
                "type": "array",
                "minItems": 1,
                "maxItems": max_groups,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["sourceFindingIds", "statement", "explanation", "priorityRank"],
                    "properties": {
                        "sourceFindingIds": {
                            "type": "array",
                            "minItems": 1,
                            "items": {"type": "string"}
                        },
                        "statement": {"type": "string"},
                        "explanation": {"type": "string"},
                        "priorityRank": {"type": "integer", "minimum": 1}
                    }
                }
            }
        }
    })
}

fn parse_response(
    provider: &str,
    response: &[u8],
) -> Result<Vec<WireSynthesisGroup>, ReviewSynthesisError> {
    let root: Value =
        serde_json::from_slice(response).map_err(|_| ReviewSynthesisError::MalformedResponse)?;
    let output = match StructuredModelProvider::parse(provider)
        .map_err(|error| ReviewSynthesisError::InvalidConfiguration(error.to_string()))?
    {
        StructuredModelProvider::Anthropic => root
            .get("content")
            .and_then(Value::as_array)
            .and_then(|content| {
                content.iter().find_map(|item| {
                    (item.get("type")?.as_str()? == "tool_use"
                        && item.get("name")?.as_str()? == "synthesize_review_findings")
                        .then(|| item.get("input"))
                })
            })
            .flatten()
            .cloned()
            .ok_or(ReviewSynthesisError::MalformedResponse)?,
        StructuredModelProvider::OpenAi => {
            let Some(content) = root
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
            else {
                return Err(ReviewSynthesisError::MalformedResponse);
            };
            serde_json::from_str(content).map_err(|_| ReviewSynthesisError::MalformedResponse)?
        }
    };
    let wire: WireSynthesisResponse = serde_json::from_value(output)
        .map_err(|error| ReviewSynthesisError::InvalidStructuredOutput(error.to_string()))?;
    Ok(wire.groups)
}

fn map_transport_error(error: StructuredModelTransportError) -> ReviewSynthesisError {
    match error {
        StructuredModelTransportError::NotConfigured => ReviewSynthesisError::NotConfigured,
        StructuredModelTransportError::InvalidConfiguration
        | StructuredModelTransportError::ClientBuildFailed => {
            ReviewSynthesisError::InvalidConfiguration(error.to_string())
        }
        StructuredModelTransportError::ResponseTooLarge
        | StructuredModelTransportError::Timeout
        | StructuredModelTransportError::Unauthorized
        | StructuredModelTransportError::RateLimited
        | StructuredModelTransportError::ProviderUnavailable
        | StructuredModelTransportError::Transport => {
            ReviewSynthesisError::Transport(error.to_string())
        }
    }
}
