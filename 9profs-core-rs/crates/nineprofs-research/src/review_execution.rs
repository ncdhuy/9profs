use std::collections::BTreeSet;

use nineprofs_structured_model::{
    StructuredModelClient, StructuredModelConfig, StructuredModelProvider,
    StructuredModelTransportError,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    AuthorityPack, AuthorityPackDocument, DocumentMap, DocumentMapBlock, DocumentMapBlockKind,
    DocumentMapLocator, EvidenceLocator, REVIEW_TASK_CONTRACT_VERSION, RegulationRequirement,
    ResearchContext, ResolvedReviewStack, ReviewAuthorityReference, ReviewExecutorMode, ReviewTask,
    is_document_map_current,
};

pub const REVIEW_TASK_EXECUTION_CONTRACT_VERSION: &str = "research-review-task-execution-v0.1";

const MAX_REVIEW_TASK_REQUEST_BYTES: usize = 512 * 1024;
const MAX_REVIEW_TASK_FINDINGS: usize = 32;
const MIN_REVIEW_TASK_OUTPUT_TOKENS: u32 = 8_192;

const REVIEW_TASK_EXECUTION_INSTRUCTION: &str = r#"You are research-review-task-execution-v0.1, a bounded manuscript review executor.

Use only the supplied ReviewTask, bounded manuscript blocks, ResearchContext, and routed authority material. Identify substantive issues only. Do not invent missing scientific facts, citations, requirements, or data. Distinguish manuscript observation from inference in the explanation. Ground every issue in supplied manuscript content and use only supplied authority IDs. Do not propose edits or replacement text. Avoid duplicate findings within this task. Return zero findings when no defensible issue is observed. Prefer meaningful issues over cosmetic noise. Keep each finding independently understandable.

For each finding, use exact supplied manuscript locators and quote manuscript evidence only when the excerpt is copied from the supplied block text. Use authority IDs exactly as supplied: pack:<pack_id> for authority packs and requirement:<requirement_id> for institutional requirements. Do not return any authority ID that is not supplied. Return only the requested JSON object."#;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub task_id: String,
    pub task_kind: String,
    pub manuscript_locators: Vec<DocumentMapLocator>,
    pub statement: String,
    pub explanation: String,
    pub evidence: Vec<FindingEvidence>,
    pub authority_references: Vec<ReviewAuthorityReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingEvidence {
    pub locator: DocumentMapLocator,
    pub excerpt: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingValidationFailure {
    pub candidate_index: usize,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTaskValidation {
    pub raw_candidate_count: usize,
    pub findings: Vec<Finding>,
    pub rejections: Vec<FindingValidationFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTaskExecutionResult {
    pub task_id: String,
    pub task_kind: String,
    pub executor_mode: ReviewExecutorMode,
    pub provider: String,
    pub model: String,
    pub manuscript_block_count: usize,
    pub authority_pack_ids: Vec<String>,
    pub regulation_requirement_ids: Vec<String>,
    pub input_bytes: usize,
    pub raw_candidate_count: usize,
    pub findings: Vec<Finding>,
    pub rejections: Vec<FindingValidationFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewExecutionReport {
    pub task_results: Vec<ReviewTaskExecutionResult>,
}

impl ReviewExecutionReport {
    pub fn findings(&self) -> Vec<Finding> {
        self.task_results
            .iter()
            .flat_map(|result| result.findings.iter().cloned())
            .collect()
    }

    pub fn raw_candidate_count(&self) -> usize {
        self.task_results
            .iter()
            .map(|result| result.raw_candidate_count)
            .sum()
    }

    pub fn rejected_finding_count(&self) -> usize {
        self.task_results
            .iter()
            .map(|result| result.rejections.len())
            .sum()
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ReviewTaskExecutionError {
    #[error("review task is invalid: {0}")]
    InvalidTask(String),
    #[error("review task response is malformed")]
    MalformedResponse,
    #[error("review task response is invalid: {0}")]
    InvalidStructuredOutput(String),
    #[error("review task executor mode is unsupported for MVP: {0:?}")]
    UnsupportedExecutorMode(ReviewExecutorMode),
    #[error("review task model is not configured")]
    NotConfigured,
    #[error("review task model configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("review task model transport failed: {0}")]
    Transport(String),
    #[error("review task request exceeds size limit")]
    RequestTooLarge,
    #[error("review task serialization failed: {0}")]
    Serialization(String),
}

#[derive(Clone)]
pub struct ReviewTaskExecutor {
    config: StructuredModelConfig,
    client: StructuredModelClient,
}

impl std::fmt::Debug for ReviewTaskExecutor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReviewTaskExecutor")
            .field("config", &self.config)
            .finish()
    }
}

impl ReviewTaskExecutor {
    pub fn from_env() -> Self {
        Self::new(StructuredModelConfig::from_env())
    }

    pub fn new(config: StructuredModelConfig) -> Self {
        let client = StructuredModelClient::new(config.clone());
        Self { config, client }
    }

    pub async fn execute(
        &self,
        task: &ReviewTask,
        map: &DocumentMap,
        stack: &ResolvedReviewStack,
    ) -> Result<ReviewTaskExecutionResult, ReviewTaskExecutionError> {
        if task.executor_mode != ReviewExecutorMode::Semantic {
            return Err(ReviewTaskExecutionError::UnsupportedExecutorMode(
                task.executor_mode.clone(),
            ));
        }

        let assembly = assemble_task(task, map, stack)?;
        let credential = self
            .config
            .credential()
            .ok_or(ReviewTaskExecutionError::NotConfigured)?;
        self.config
            .validate(Some(&credential))
            .map_err(|error| ReviewTaskExecutionError::InvalidConfiguration(error.to_string()))?;
        let body = request_body(&self.config, &assembly.input)?;
        let response = self
            .client
            .execute_json(&body, &credential)
            .await
            .map_err(map_transport_error)?;
        let validation =
            validate_review_task_response(&self.config.provider, &response, task, map, stack)?;

        Ok(ReviewTaskExecutionResult {
            task_id: task.id.clone(),
            task_kind: task.kind.clone(),
            executor_mode: task.executor_mode.clone(),
            provider: self.config.provider.clone(),
            model: self.config.model.clone(),
            manuscript_block_count: assembly.input.manuscript.len(),
            authority_pack_ids: assembly.authority_pack_ids,
            regulation_requirement_ids: assembly.regulation_requirement_ids,
            input_bytes: assembly.input_bytes,
            raw_candidate_count: validation.raw_candidate_count,
            findings: validation.findings,
            rejections: validation.rejections,
        })
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewTaskExecutionInput {
    contract_version: String,
    task: ReviewTask,
    research_context: ResearchContext,
    manuscript: Vec<ManuscriptBlockInput>,
    allowed_authority_ids: Vec<String>,
    authority_packs: Vec<AuthorityPackInput>,
    regulation_requirements: Vec<RegulationRequirementInput>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ManuscriptBlockInput {
    id: String,
    ordinal: u32,
    kind: DocumentMapBlockKind,
    locator: DocumentMapLocator,
    text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthorityPackInput {
    reference: ReviewAuthorityReference,
    documents: Vec<AuthorityPackDocument>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RegulationRequirementInput {
    reference: ReviewAuthorityReference,
    requirement_id: String,
    text: String,
    source_excerpt: String,
    authority_locator: Option<EvidenceLocator>,
}

struct TaskAssembly {
    input: ReviewTaskExecutionInput,
    input_bytes: usize,
    authority_pack_ids: Vec<String>,
    regulation_requirement_ids: Vec<String>,
    scope_section_ids: BTreeSet<String>,
    document_scope: bool,
}

fn assemble_task(
    task: &ReviewTask,
    map: &DocumentMap,
    stack: &ResolvedReviewStack,
) -> Result<TaskAssembly, ReviewTaskExecutionError> {
    validate_task_identity(task, map)?;

    let visible_sections = map
        .sections
        .iter()
        .filter(|section| !section.is_deleted)
        .collect::<Vec<_>>();
    let visible_section_ids = visible_sections
        .iter()
        .map(|section| section.id.clone())
        .collect::<BTreeSet<_>>();
    let mut scope_section_ids = task
        .target
        .section_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if scope_section_ids
        .iter()
        .any(|id| !visible_section_ids.contains(id))
    {
        return Err(ReviewTaskExecutionError::InvalidTask(
            "task scope contains an unknown manuscript section".to_owned(),
        ));
    }
    loop {
        let descendants = visible_sections
            .iter()
            .filter(|section| {
                section
                    .parent_id
                    .as_ref()
                    .is_some_and(|parent| scope_section_ids.contains(parent))
            })
            .map(|section| section.id.clone())
            .collect::<Vec<_>>();
        let before = scope_section_ids.len();
        scope_section_ids.extend(descendants);
        if scope_section_ids.len() == before {
            break;
        }
    }

    let document_scope = (task.target.section_ids.is_empty() && task.target.locators.is_empty())
        || (!visible_section_ids.is_empty() && scope_section_ids == visible_section_ids);
    let scope_blocks = map
        .blocks
        .iter()
        .filter(|block| {
            !block.is_deleted
                && (document_scope
                    || task
                        .target
                        .locators
                        .iter()
                        .any(|locator| locator == &block.locator)
                    || block
                        .section_id
                        .as_ref()
                        .is_some_and(|section_id| scope_section_ids.contains(section_id)))
        })
        .cloned()
        .collect::<Vec<_>>();
    if scope_blocks.is_empty() {
        return Err(ReviewTaskExecutionError::InvalidTask(
            "task scope contains no visible manuscript blocks".to_owned(),
        ));
    }

    let mut authority_packs = Vec::new();
    let mut regulation_requirements = Vec::new();
    let mut authority_pack_ids = Vec::new();
    let mut regulation_requirement_ids = Vec::new();
    let mut allowed_authority_ids = Vec::new();

    for reference in &task.authority_references {
        match reference {
            ReviewAuthorityReference::AuthorityPack {
                pack_id,
                version,
                source,
                content_paths,
            } => {
                let pack = stack
                    .authority_packs
                    .iter()
                    .find(|pack| pack.id == *pack_id && pack.version == *version)
                    .ok_or_else(|| {
                        ReviewTaskExecutionError::InvalidTask(format!(
                            "task references unrouted authority pack: {pack_id}"
                        ))
                    })?;
                if pack.source != *source {
                    return Err(ReviewTaskExecutionError::InvalidTask(format!(
                        "task authority pack provenance does not match resolved pack: {pack_id}"
                    )));
                }
                let documents = pack_documents(pack, content_paths)?;
                authority_packs.push(AuthorityPackInput {
                    reference: reference.clone(),
                    documents,
                });
                authority_pack_ids.push(pack_id.clone());
                allowed_authority_ids.push(format!("pack:{pack_id}"));
            }
            ReviewAuthorityReference::RegulationRequirement { reference } => {
                let requirement = stack
                    .regulation_requirements
                    .iter()
                    .find(|requirement| requirement_matches(reference, requirement))
                    .ok_or_else(|| {
                        ReviewTaskExecutionError::InvalidTask(format!(
                            "task references unknown regulation requirement: {}",
                            reference.requirement_id
                        ))
                    })?;
                regulation_requirements.push(RegulationRequirementInput {
                    reference: ReviewAuthorityReference::RegulationRequirement {
                        reference: reference.clone(),
                    },
                    requirement_id: requirement.id.to_string(),
                    text: requirement.text.clone(),
                    source_excerpt: requirement.source_excerpt.clone(),
                    authority_locator: requirement.authority_locator.clone(),
                });
                regulation_requirement_ids.push(requirement.id.to_string());
                allowed_authority_ids.push(format!("requirement:{}", requirement.id));
            }
        }
    }

    let manuscript = scope_blocks
        .iter()
        .map(|block| ManuscriptBlockInput {
            id: block.id.clone(),
            ordinal: block.ordinal,
            kind: block.kind.clone(),
            locator: block.locator.clone(),
            text: block.text.clone(),
        })
        .collect::<Vec<_>>();
    let input = ReviewTaskExecutionInput {
        contract_version: REVIEW_TASK_EXECUTION_CONTRACT_VERSION.to_owned(),
        task: task.clone(),
        research_context: stack.research_context.clone(),
        manuscript,
        allowed_authority_ids,
        authority_packs,
        regulation_requirements,
    };
    let input_bytes = serde_json::to_vec(&input)
        .map_err(|error| ReviewTaskExecutionError::Serialization(error.to_string()))?;
    if input_bytes.len() > MAX_REVIEW_TASK_REQUEST_BYTES {
        return Err(ReviewTaskExecutionError::RequestTooLarge);
    }

    Ok(TaskAssembly {
        input,
        input_bytes: input_bytes.len(),
        authority_pack_ids,
        regulation_requirement_ids,
        scope_section_ids,
        document_scope,
    })
}

fn validate_task_identity(
    task: &ReviewTask,
    map: &DocumentMap,
) -> Result<(), ReviewTaskExecutionError> {
    if task.contract_version != REVIEW_TASK_CONTRACT_VERSION {
        return Err(ReviewTaskExecutionError::InvalidTask(format!(
            "unsupported review task contract version: {}",
            task.contract_version
        )));
    }
    if task.id.trim().is_empty()
        || task.kind.trim().is_empty()
        || task.instruction.trim().is_empty()
    {
        return Err(ReviewTaskExecutionError::InvalidTask(
            "task identity and instruction must be non-empty".to_owned(),
        ));
    }
    if task.target.document_map_contract_version != map.contract_version {
        return Err(ReviewTaskExecutionError::InvalidTask(
            "task document map contract version does not match supplied map".to_owned(),
        ));
    }
    if !is_document_map_current(map, &task.target.document_id, task.target.document_version) {
        return Err(ReviewTaskExecutionError::InvalidTask(
            "task targets a stale or different document map version".to_owned(),
        ));
    }
    for locator in &task.target.locators {
        if find_block(map, locator).is_none() {
            return Err(ReviewTaskExecutionError::InvalidTask(format!(
                "task scope locator does not exist: {}",
                locator.block_id
            )));
        }
    }
    Ok(())
}

fn pack_documents(
    pack: &AuthorityPack,
    content_paths: &[String],
) -> Result<Vec<AuthorityPackDocument>, ReviewTaskExecutionError> {
    if content_paths.is_empty() {
        return Err(ReviewTaskExecutionError::InvalidTask(format!(
            "authority pack {} routes no content",
            pack.id
        )));
    }
    content_paths
        .iter()
        .map(|path| {
            pack.knowledge
                .iter()
                .chain(pack.review_guidance.iter())
                .find(|document| document.path == *path)
                .cloned()
                .ok_or_else(|| {
                    ReviewTaskExecutionError::InvalidTask(format!(
                        "authority pack {} does not contain routed content: {path}",
                        pack.id
                    ))
                })
        })
        .collect()
}

fn requirement_matches(
    reference: &crate::RegulationRequirementReference,
    requirement: &RegulationRequirement,
) -> bool {
    reference.requirement_id == requirement.id
        && reference.source_id == requirement.source_id
        && reference.source_snapshot_id == requirement.source_snapshot_id
        && reference.authority_locator == requirement.authority_locator
        && reference.normalized_requirement == requirement.text
}

fn authority_id(reference: &ReviewAuthorityReference) -> String {
    match reference {
        ReviewAuthorityReference::AuthorityPack { pack_id, .. } => format!("pack:{pack_id}"),
        ReviewAuthorityReference::RegulationRequirement { reference } => {
            format!("requirement:{}", reference.requirement_id)
        }
    }
}

fn find_block<'a>(
    map: &'a DocumentMap,
    locator: &DocumentMapLocator,
) -> Option<&'a DocumentMapBlock> {
    map.blocks
        .iter()
        .find(|block| !block.is_deleted && block.locator == *locator)
}

fn locator_in_scope(
    locator: &DocumentMapLocator,
    assembly: &TaskAssembly,
    map: &DocumentMap,
) -> bool {
    if assembly.document_scope
        || assembly
            .input
            .task
            .target
            .locators
            .iter()
            .any(|candidate| candidate == locator)
    {
        return true;
    }
    find_block(map, locator)
        .and_then(|block| block.section_id.as_ref())
        .is_some_and(|section_id| assembly.scope_section_ids.contains(section_id))
}

pub fn validate_review_task_response(
    provider: &str,
    response: &[u8],
    task: &ReviewTask,
    map: &DocumentMap,
    stack: &ResolvedReviewStack,
) -> Result<ReviewTaskValidation, ReviewTaskExecutionError> {
    let assembly = assemble_task(task, map, stack)?;
    let candidates = parse_response(provider, response)?;
    if candidates.findings.len() > MAX_REVIEW_TASK_FINDINGS {
        return Err(ReviewTaskExecutionError::InvalidStructuredOutput(format!(
            "response contains more than {MAX_REVIEW_TASK_FINDINGS} findings"
        )));
    }

    let allowed_authorities = task
        .authority_references
        .iter()
        .map(|reference| (authority_id(reference), reference))
        .collect::<Vec<_>>();
    let mut findings = Vec::new();
    let mut rejections = Vec::new();
    for (candidate_index, candidate) in candidates.findings.into_iter().enumerate() {
        match validate_candidate(
            candidate_index,
            candidate,
            task,
            map,
            stack,
            &assembly,
            &allowed_authorities,
        ) {
            Ok(finding) => findings.push(finding),
            Err(reason) => rejections.push(FindingValidationFailure {
                candidate_index,
                reason,
            }),
        }
    }
    Ok(ReviewTaskValidation {
        raw_candidate_count: findings.len() + rejections.len(),
        findings,
        rejections,
    })
}

fn validate_candidate(
    candidate_index: usize,
    candidate: WireFindingCandidate,
    task: &ReviewTask,
    map: &DocumentMap,
    stack: &ResolvedReviewStack,
    assembly: &TaskAssembly,
    allowed_authorities: &[(String, &ReviewAuthorityReference)],
) -> Result<Finding, String> {
    if candidate.statement.trim().is_empty() {
        return Err("finding statement is empty".to_owned());
    }
    if candidate.explanation.trim().is_empty() {
        return Err("finding explanation is empty".to_owned());
    }
    if candidate.manuscript_locators.is_empty() {
        return Err("finding has no manuscript locator".to_owned());
    }

    for locator in &candidate.manuscript_locators {
        if find_block(map, locator).is_none() {
            return Err(format!(
                "manuscript locator does not exist: {}",
                locator.block_id
            ));
        }
        if !locator_in_scope(locator, assembly, map) {
            return Err(format!(
                "manuscript locator is outside task scope: {}",
                locator.block_id
            ));
        }
    }

    if candidate.authority_ids.is_empty() {
        return Err("substantive finding has no authority reference".to_owned());
    }
    let mut seen_authorities = BTreeSet::new();
    let mut authority_references = Vec::new();
    for authority_id in &candidate.authority_ids {
        if !seen_authorities.insert(authority_id.clone()) {
            return Err(format!("duplicate authority reference: {authority_id}"));
        }
        let Some((_, reference)) = allowed_authorities
            .iter()
            .find(|(allowed_id, _)| allowed_id == authority_id)
        else {
            return Err(format!(
                "authority reference is not routed to task: {authority_id}"
            ));
        };
        if let ReviewAuthorityReference::RegulationRequirement { reference } = reference
            && !stack
                .regulation_requirements
                .iter()
                .any(|requirement| requirement_matches(reference, requirement))
        {
            return Err(format!(
                "regulation requirement is absent from resolved stack: {}",
                reference.requirement_id
            ));
        }
        authority_references.push((*reference).clone());
    }

    let mut evidence = Vec::new();
    for item in candidate.evidence {
        let block = find_block(map, &item.locator)
            .ok_or_else(|| format!("evidence locator does not exist: {}", item.locator.block_id))?;
        if !locator_in_scope(&item.locator, assembly, map) {
            return Err(format!(
                "evidence locator is outside task scope: {}",
                item.locator.block_id
            ));
        }
        if item.excerpt.trim().is_empty() {
            return Err("evidence excerpt is empty".to_owned());
        }
        if !block.text.contains(&item.excerpt) {
            return Err(format!(
                "evidence excerpt is not grounded in manuscript block: {}",
                item.locator.block_id
            ));
        }
        evidence.push(FindingEvidence {
            locator: item.locator,
            excerpt: item.excerpt,
        });
    }

    Ok(Finding {
        id: format!("{}:{candidate_index}", task.id),
        task_id: task.id.clone(),
        task_kind: task.kind.clone(),
        manuscript_locators: candidate.manuscript_locators,
        statement: candidate.statement,
        explanation: candidate.explanation,
        evidence,
        authority_references,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireFindingResponse {
    findings: Vec<WireFindingCandidate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireFindingCandidate {
    statement: String,
    explanation: String,
    manuscript_locators: Vec<DocumentMapLocator>,
    evidence: Vec<WireFindingEvidence>,
    authority_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireFindingEvidence {
    locator: DocumentMapLocator,
    excerpt: String,
}

struct ParsedResponse {
    findings: Vec<WireFindingCandidate>,
}

fn parse_response(
    provider: &str,
    response: &[u8],
) -> Result<ParsedResponse, ReviewTaskExecutionError> {
    let root: Value = serde_json::from_slice(response)
        .map_err(|_| ReviewTaskExecutionError::MalformedResponse)?;
    let output = match StructuredModelProvider::parse(provider)
        .map_err(|error| ReviewTaskExecutionError::InvalidConfiguration(error.to_string()))?
    {
        StructuredModelProvider::Anthropic => root
            .get("content")
            .and_then(Value::as_array)
            .and_then(|content| {
                content.iter().find_map(|item| {
                    (item.get("type")?.as_str()? == "tool_use"
                        && item.get("name")?.as_str()? == "execute_review_task")
                        .then(|| item.get("input"))
                })
            })
            .flatten()
            .cloned()
            .ok_or(ReviewTaskExecutionError::MalformedResponse)?,
        StructuredModelProvider::OpenAi => {
            let Some(content) = root
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|choices| choices.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|message| message.get("content"))
                .and_then(Value::as_str)
            else {
                let top_level_keys = root
                    .as_object()
                    .map(|object| object.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                let choice_keys = root
                    .get("choices")
                    .and_then(Value::as_array)
                    .and_then(|choices| choices.first())
                    .and_then(Value::as_object)
                    .map(|object| object.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                eprintln!(
                    "review execution response shape: top_level_keys={top_level_keys:?}, choice_keys={choice_keys:?}"
                );
                return Err(ReviewTaskExecutionError::MalformedResponse);
            };
            serde_json::from_str(content)
                .map_err(|_| ReviewTaskExecutionError::MalformedResponse)?
        }
    };
    let wire: WireFindingResponse = serde_json::from_value(output)
        .map_err(|error| ReviewTaskExecutionError::InvalidStructuredOutput(error.to_string()))?;
    Ok(ParsedResponse {
        findings: wire.findings,
    })
}

fn request_body(
    config: &StructuredModelConfig,
    input: &ReviewTaskExecutionInput,
) -> Result<Value, ReviewTaskExecutionError> {
    let input_json = serde_json::to_string(input)
        .map_err(|error| ReviewTaskExecutionError::Serialization(error.to_string()))?;
    let prompt = format!("Execute this exact bounded review task input JSON:\n{input_json}");
    let schema = finding_schema();
    let body = match config
        .provider()
        .map_err(|error| ReviewTaskExecutionError::InvalidConfiguration(error.to_string()))?
    {
        StructuredModelProvider::Anthropic => json!({
            "model": config.model,
            "max_tokens": config.max_output_tokens,
            "temperature": 0,
            "system": REVIEW_TASK_EXECUTION_INSTRUCTION,
            "messages": [{"role": "user", "content": prompt}],
            "tools": [{
                "name": "execute_review_task",
                "description": "Return strict manuscript review findings.",
                "input_schema": schema
            }],
            "tool_choice": {"type": "tool", "name": "execute_review_task"}
        }),
        StructuredModelProvider::OpenAi => json!({
            "model": config.model,
            "max_completion_tokens": config.max_output_tokens.max(MIN_REVIEW_TASK_OUTPUT_TOKENS),
            "messages": [
                {"role": "system", "content": REVIEW_TASK_EXECUTION_INSTRUCTION},
                {"role": "user", "content": prompt}
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "research_review_task_execution",
                    "strict": true,
                    "schema": schema
                }
            }
        }),
    };
    let body_bytes = serde_json::to_vec(&body)
        .map_err(|error| ReviewTaskExecutionError::Serialization(error.to_string()))?;
    if body_bytes.len() > MAX_REVIEW_TASK_REQUEST_BYTES {
        return Err(ReviewTaskExecutionError::RequestTooLarge);
    }
    Ok(body)
}

fn locator_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["documentId", "version", "blockId", "blockOrdinal", "docxIndex", "sectionId"],
        "properties": {
            "documentId": {"type": "string"},
            "version": {"type": "integer"},
            "blockId": {"type": "string"},
            "blockOrdinal": {"type": "integer", "minimum": 0},
            "docxIndex": {"type": ["integer", "null"], "minimum": 0},
            "sectionId": {"type": ["string", "null"]}
        }
    })
}

fn finding_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["findings"],
        "properties": {
            "findings": {
                "type": "array",
                "maxItems": MAX_REVIEW_TASK_FINDINGS,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["statement", "explanation", "manuscriptLocators", "evidence", "authorityIds"],
                    "properties": {
                        "statement": {"type": "string"},
                        "explanation": {"type": "string"},
                        "manuscriptLocators": {"type": "array", "minItems": 1, "items": locator_schema()},
                        "evidence": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "additionalProperties": false,
                                "required": ["locator", "excerpt"],
                                "properties": {
                                    "locator": locator_schema(),
                                    "excerpt": {"type": "string"}
                                }
                            }
                        },
                        "authorityIds": {"type": "array", "minItems": 1, "items": {"type": "string"}}
                    }
                }
            }
        }
    })
}

fn map_transport_error(error: StructuredModelTransportError) -> ReviewTaskExecutionError {
    match error {
        StructuredModelTransportError::NotConfigured => ReviewTaskExecutionError::NotConfigured,
        StructuredModelTransportError::InvalidConfiguration
        | StructuredModelTransportError::ClientBuildFailed => {
            ReviewTaskExecutionError::InvalidConfiguration(error.to_string())
        }
        StructuredModelTransportError::ResponseTooLarge => {
            ReviewTaskExecutionError::Transport(error.to_string())
        }
        StructuredModelTransportError::Timeout
        | StructuredModelTransportError::Unauthorized
        | StructuredModelTransportError::RateLimited
        | StructuredModelTransportError::ProviderUnavailable
        | StructuredModelTransportError::Transport => {
            ReviewTaskExecutionError::Transport(error.to_string())
        }
    }
}
