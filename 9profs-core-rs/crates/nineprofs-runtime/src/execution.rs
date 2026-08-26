use std::{collections::HashMap, sync::Arc};

use futures_util::FutureExt;
use nineprofs_agent::{
    AgentExecutionError, AgentExecutionEvent, AgentExecutionRequest, AgentExecutorRegistry,
    AgentProviderConfig, AgentProviderConfigError, AgentRunContext, AgentTask, AgentTaskManager,
    BackendResolution, ExecutionLimits, RunId, TaskFailure, TaskState,
};
use nineprofs_api_types::EventEnvelope;
use nineprofs_assistant::{AssistantError, AssistantService};
use nineprofs_document_tools::docs_active_tool_set;
use nineprofs_documents::{
    ActiveDocumentDescriptor, ConnectionState, DOCUMENT_BRIDGE_CAPABILITY_COMMIT,
    DOCUMENT_BRIDGE_CAPABILITY_INSPECT, DOCX_DOCUMENT_TYPE, DocumentBridgeService,
    GENOFFICE_ACTIVE_AUTHORITY,
};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_skills::{Skill, SkillCatalog};
use nineprofs_tools::{ToolRegistry, ToolSet};
use serde_json::json;
use thiserror::Error;
use tokio::sync::{RwLock, mpsc, watch};

use crate::docs_conversation::{
    DocsAgentConversationMetadata, DocsAgentConversationSeed, DocsAgentConversationStore,
    DocsAgentConversationStoreError,
};

#[derive(Debug, Error)]
pub enum AgentExecutionServiceError {
    #[error(transparent)]
    Assistant(#[from] AssistantError),
    #[error("agent run input must not be empty")]
    EmptyInput,
    #[error("assistant is disabled: {0}")]
    AssistantDisabled(String),
    #[error("assistant has no configured agent backend")]
    BackendNotConfigured,
    #[error("agent backend `{0}` was not found")]
    BackendMissing(String),
    #[error("agent backend `{0}` is not available: {1}")]
    BackendUnavailable(String, String),
    #[error("agent backend `{0}` is disabled")]
    BackendDisabled(String),
    #[error("agent backend `{0}` has no executor")]
    ExecutorMissing(String),
    #[error("skill is missing: {0}")]
    MissingSkill(String),
    #[error("active document is unavailable: {0}")]
    ActiveDocumentUnavailable(String),
    #[error("active document is unsupported: {0}")]
    ActiveDocumentUnsupported(String),
    #[error("required Docs tool is not registered: {0}")]
    RequiredToolMissing(String),
    #[error("Docs agent conversation is not found: {0}")]
    ConversationNotFound(String),
    #[error("Docs agent conversation is busy: {0}")]
    ConversationBusy(String),
    #[error("Docs agent conversation is unavailable: {0}")]
    ConversationUnavailable(String),
    #[error("Docs agent conversation store is at capacity")]
    ConversationCapacity,
    #[error("Docs agent conversation reached its turn limit")]
    ConversationTurnLimit,
    #[error(transparent)]
    Task(#[from] nineprofs_agent::AgentTaskManagerError),
}

#[derive(Clone)]
pub struct AgentExecutionService {
    assistants: Arc<AssistantService>,
    skills: Arc<SkillCatalog>,
    registry: Arc<nineprofs_agent::AgentRegistry>,
    executors: AgentExecutorRegistry,
    tasks: AgentTaskManager,
    events: Arc<BroadcastEventBus>,
    provider: AgentProviderConfig,
    document_bridge: Arc<DocumentBridgeService>,
    run_contexts: Arc<RwLock<HashMap<RunId, AgentRunContext>>>,
    tools: ToolRegistry,
    docs_conversations: DocsAgentConversationStore,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRunStarted {
    pub run_id: RunId,
    pub task: AgentTask,
    pub context: Option<AgentRunContext>,
}

struct PreparedDocsAgent {
    assistant_id: String,
    backend_id: String,
    executor: Arc<dyn nineprofs_agent::AgentExecutor>,
    system_instructions: String,
}

impl AgentExecutionService {
    pub fn new(
        assistants: Arc<AssistantService>,
        skills: Arc<SkillCatalog>,
        registry: Arc<nineprofs_agent::AgentRegistry>,
        executors: AgentExecutorRegistry,
        tasks: AgentTaskManager,
        events: Arc<BroadcastEventBus>,
        provider: AgentProviderConfig,
        document_bridge: Arc<DocumentBridgeService>,
        tools: ToolRegistry,
    ) -> Self {
        Self {
            assistants,
            skills,
            registry,
            executors,
            tasks,
            events,
            provider,
            document_bridge,
            run_contexts: Arc::new(RwLock::new(HashMap::new())),
            tools,
            docs_conversations: DocsAgentConversationStore::new(),
        }
    }

    pub async fn start_run(
        &self,
        assistant_id: &str,
        input: &str,
    ) -> Result<AgentRunStarted, AgentExecutionServiceError> {
        self.start_run_with_context(assistant_id, input, ToolSet::default(), None)
            .await
    }

    pub async fn start_run_with_tool_set(
        &self,
        assistant_id: &str,
        input: &str,
        tool_set: ToolSet,
    ) -> Result<AgentRunStarted, AgentExecutionServiceError> {
        self.start_run_with_context(assistant_id, input, tool_set, None)
            .await
    }

    pub async fn start_active_docs_run(
        &self,
        assistant_id: &str,
        document_id: &str,
        input: &str,
    ) -> Result<AgentRunStarted, AgentExecutionServiceError> {
        let descriptor = self.document_bridge.get(document_id).await.ok_or_else(|| {
            AgentExecutionServiceError::ActiveDocumentUnavailable(document_id.to_owned())
        })?;
        validate_active_docs_document(&descriptor)?;
        self.start_run_with_context(
            assistant_id,
            input,
            docs_active_tool_set(),
            Some(AgentRunContext::ActiveDocs {
                document_id: document_id.to_owned(),
            }),
        )
        .await
    }

    pub async fn create_docs_agent_conversation(
        &self,
        assistant_id: &str,
        document_id: &str,
    ) -> Result<DocsAgentConversationMetadata, AgentExecutionServiceError> {
        let descriptor = self.document_bridge.get(document_id).await.ok_or_else(|| {
            AgentExecutionServiceError::ActiveDocumentUnavailable(document_id.to_owned())
        })?;
        validate_active_docs_document(&descriptor)?;
        let prepared = self.prepare_docs_agent(assistant_id, document_id).await?;
        self.ensure_docs_tools()?;
        self.docs_conversations
            .create(DocsAgentConversationSeed {
                assistant_id: prepared.assistant_id,
                document_id: document_id.to_owned(),
                backend_id: prepared.backend_id,
                system_instructions: prepared.system_instructions,
                tool_set: docs_active_tool_set(),
            })
            .map_err(map_conversation_error)
    }

    pub fn docs_agent_conversation(
        &self,
        conversation_id: &str,
    ) -> Option<DocsAgentConversationMetadata> {
        self.docs_conversations.get(conversation_id)
    }

    pub async fn start_docs_agent_conversation_run(
        &self,
        conversation_id: &str,
        input: &str,
    ) -> Result<AgentRunStarted, AgentExecutionServiceError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AgentExecutionServiceError::EmptyInput);
        }
        let metadata = self
            .docs_conversations
            .get(conversation_id)
            .ok_or_else(|| {
                AgentExecutionServiceError::ConversationNotFound(conversation_id.to_owned())
            })?;
        let descriptor = match self.document_bridge.get(&metadata.document_id).await {
            Some(descriptor) => descriptor,
            None => {
                self.docs_conversations.mark_unavailable(conversation_id);
                return Err(AgentExecutionServiceError::ConversationUnavailable(
                    conversation_id.to_owned(),
                ));
            }
        };
        if let Err(error) = validate_active_docs_document(&descriptor) {
            self.docs_conversations.mark_unavailable(conversation_id);
            return Err(AgentExecutionServiceError::ConversationUnavailable(
                error.to_string(),
            ));
        }
        let turn = self
            .docs_conversations
            .begin(conversation_id)
            .map_err(map_conversation_error)?;
        let prepared = match self.prepare_bound_docs_agent(&turn).await {
            Ok(prepared) => prepared,
            Err(error) => {
                self.docs_conversations.finish(conversation_id, false);
                return Err(error);
            }
        };
        let started = self
            .start_prepared_run(
                input,
                prepared,
                docs_active_tool_set(),
                Some(AgentRunContext::ActiveDocs {
                    document_id: turn.document_id,
                }),
                Some(turn.conversation_id),
            )
            .await;
        if started.is_err() {
            self.docs_conversations.finish(conversation_id, false);
        }
        started
    }

    async fn prepare_docs_agent(
        &self,
        assistant_id: &str,
        document_id: &str,
    ) -> Result<PreparedDocsAgent, AgentExecutionServiceError> {
        let assistant = self.assistants.get(assistant_id).await?;
        if !assistant.enabled {
            return Err(AgentExecutionServiceError::AssistantDisabled(assistant.id));
        }
        let descriptor =
            resolve_backend(&self.registry, assistant.backend_agent_id.as_deref()).await?;
        let executor = self
            .executors
            .get(&descriptor.id)
            .ok_or_else(|| AgentExecutionServiceError::ExecutorMissing(descriptor.id.clone()))?;
        let mut resolved_skills = Vec::with_capacity(assistant.skill_ids.len());
        for skill_id in &assistant.skill_ids {
            resolved_skills.push(
                self.skills
                    .resolve(skill_id)
                    .ok_or_else(|| AgentExecutionServiceError::MissingSkill(skill_id.clone()))?,
            );
        }
        Ok(PreparedDocsAgent {
            assistant_id: assistant.id,
            backend_id: descriptor.id,
            executor,
            system_instructions: build_system_instructions_with_context(
                &assistant.rules,
                &resolved_skills,
                Some(&AgentRunContext::ActiveDocs {
                    document_id: document_id.to_owned(),
                }),
            ),
        })
    }

    async fn prepare_bound_docs_agent(
        &self,
        turn: &crate::docs_conversation::DocsAgentConversationTurn,
    ) -> Result<PreparedDocsAgent, AgentExecutionServiceError> {
        let assistant = self.assistants.get(&turn.assistant_id).await?;
        if !assistant.enabled {
            return Err(AgentExecutionServiceError::AssistantDisabled(assistant.id));
        }
        let descriptor = resolve_backend(&self.registry, Some(&turn.backend_id)).await?;
        if descriptor.id != turn.backend_id {
            return Err(AgentExecutionServiceError::BackendMissing(
                turn.backend_id.clone(),
            ));
        }
        let executor = self
            .executors
            .get(&turn.backend_id)
            .ok_or_else(|| AgentExecutionServiceError::ExecutorMissing(turn.backend_id.clone()))?;
        Ok(PreparedDocsAgent {
            assistant_id: turn.assistant_id.clone(),
            backend_id: turn.backend_id.clone(),
            executor,
            // Rules and ordered skills were snapshotted when the conversation was created.
            system_instructions: turn.system_instructions.clone(),
        })
    }

    fn ensure_docs_tools(&self) -> Result<(), AgentExecutionServiceError> {
        for required in crate::REQUIRED_DOCS_AGENT_TOOLS {
            if !self
                .tools
                .list_definitions()
                .iter()
                .any(|definition| definition.id.to_string() == required)
            {
                return Err(AgentExecutionServiceError::RequiredToolMissing(
                    required.to_owned(),
                ));
            }
        }
        Ok(())
    }

    async fn start_prepared_run(
        &self,
        input: &str,
        prepared: PreparedDocsAgent,
        tool_set: ToolSet,
        context: Option<AgentRunContext>,
        conversation_id: Option<String>,
    ) -> Result<AgentRunStarted, AgentExecutionServiceError> {
        let run_id = RunId::new();
        let task = self
            .tasks
            .register_new(run_id.clone(), prepared.backend_id.clone())
            .await?;
        let cancellation = self.tasks.cancellation(&task.task_id).await?;
        if let Some(context_value) = context.as_ref() {
            self.run_contexts
                .write()
                .await
                .insert(run_id.clone(), context_value.clone());
        }
        let request = AgentExecutionRequest {
            run_id: run_id.clone(),
            task_id: task.task_id.clone(),
            backend_id: prepared.backend_id,
            assistant_id: prepared.assistant_id,
            input: input.to_owned(),
            workspace_root: None,
            provider: self.provider.clone(),
            system_instructions: prepared.system_instructions,
            limits: ExecutionLimits::default(),
            tool_set,
            context: context.clone(),
            conversation_id: conversation_id.clone(),
        };
        let tasks = self.tasks.clone();
        let events = Arc::clone(&self.events);
        let conversations = self.docs_conversations.clone();
        let task_id = task.task_id.clone();
        tokio::spawn(async move {
            let task_run = std::panic::AssertUnwindSafe(run_task(
                tasks.clone(),
                events,
                prepared.executor,
                request,
                cancellation,
            ))
            .catch_unwind()
            .await;
            if task_run.is_err() {
                let _ = tasks
                    .fail(
                        &task_id,
                        TaskFailure {
                            code: "task_panicked".to_owned(),
                            message: "agent task panicked before completion".to_owned(),
                        },
                    )
                    .await;
            }
            if let Some(conversation_id) = conversation_id {
                let successful = tasks
                    .get(&task_id)
                    .await
                    .is_some_and(|task| task.state == TaskState::Succeeded);
                conversations.finish(&conversation_id, successful);
            }
        });
        Ok(AgentRunStarted {
            run_id,
            task,
            context,
        })
    }

    async fn start_run_with_context(
        &self,
        assistant_id: &str,
        input: &str,
        tool_set: ToolSet,
        context: Option<AgentRunContext>,
    ) -> Result<AgentRunStarted, AgentExecutionServiceError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AgentExecutionServiceError::EmptyInput);
        }
        let assistant = self.assistants.get(assistant_id).await?;
        if !assistant.enabled {
            return Err(AgentExecutionServiceError::AssistantDisabled(assistant.id));
        }

        let descriptor = match self
            .registry
            .resolve_assistant_backend(assistant.backend_agent_id.as_deref())
            .await
        {
            BackendResolution::NotConfigured => {
                return Err(AgentExecutionServiceError::BackendNotConfigured);
            }
            BackendResolution::Missing { id } => {
                return Err(AgentExecutionServiceError::BackendMissing(id));
            }
            BackendResolution::Unknown { descriptor }
            | BackendResolution::Unavailable { descriptor } => {
                return Err(AgentExecutionServiceError::BackendUnavailable(
                    descriptor.id,
                    descriptor
                        .availability_reason
                        .unwrap_or_else(|| "availability is unknown".to_owned()),
                ));
            }
            BackendResolution::Disabled { descriptor } => {
                return Err(AgentExecutionServiceError::BackendDisabled(descriptor.id));
            }
            BackendResolution::Resolved { descriptor } => descriptor,
        };
        let executor = self
            .executors
            .get(&descriptor.id)
            .ok_or_else(|| AgentExecutionServiceError::ExecutorMissing(descriptor.id.clone()))?;

        let mut resolved_skills = Vec::with_capacity(assistant.skill_ids.len());
        for skill_id in &assistant.skill_ids {
            resolved_skills.push(
                self.skills
                    .resolve(skill_id)
                    .ok_or_else(|| AgentExecutionServiceError::MissingSkill(skill_id.clone()))?,
            );
        }

        let run_id = RunId::new();
        let task = self
            .tasks
            .register_new(run_id.clone(), descriptor.id.clone())
            .await?;
        let cancellation = self.tasks.cancellation(&task.task_id).await?;
        let system_instructions = build_system_instructions_with_context(
            &assistant.rules,
            &resolved_skills,
            context.as_ref(),
        );
        if let Some(context_value) = context.as_ref() {
            self.run_contexts
                .write()
                .await
                .insert(run_id.clone(), context_value.clone());
        }
        let request = AgentExecutionRequest {
            run_id: run_id.clone(),
            task_id: task.task_id.clone(),
            backend_id: descriptor.id,
            assistant_id: assistant.id,
            input: input.to_owned(),
            workspace_root: None,
            provider: self.provider.clone(),
            system_instructions,
            limits: ExecutionLimits::default(),
            tool_set,
            context: context.clone(),
            conversation_id: None,
        };

        let tasks = self.tasks.clone();
        let events = Arc::clone(&self.events);
        tokio::spawn(async move {
            run_task(tasks, events, executor, request, cancellation).await;
        });

        Ok(AgentRunStarted {
            run_id,
            task,
            context,
        })
    }

    pub async fn task(&self, task_id: &nineprofs_agent::AgentTaskId) -> Option<AgentTask> {
        self.tasks.get(task_id).await
    }

    pub async fn tasks_for_run(&self, run_id: &RunId) -> Vec<AgentTask> {
        self.tasks.list_for_run(run_id).await
    }

    pub async fn cancel(
        &self,
        task_id: &nineprofs_agent::AgentTaskId,
    ) -> Result<AgentTask, nineprofs_agent::AgentTaskManagerError> {
        self.tasks.cancel(task_id).await
    }

    pub async fn context_for_run(&self, run_id: &RunId) -> Option<AgentRunContext> {
        self.run_contexts.read().await.get(run_id).cloned()
    }

    pub fn has_executor(&self, backend_id: &str) -> bool {
        self.executors.contains(backend_id)
    }

    pub fn provider_configuration_error(&self) -> Option<AgentProviderConfigError> {
        self.provider.configuration_error()
    }
}

pub fn build_system_instructions(rules: &str, skills: &[Skill]) -> String {
    let mut sections = Vec::new();
    if !rules.trim().is_empty() {
        sections.push(format!("[Assistant Rules]\n{}", rules.trim()));
    }
    for skill in skills {
        sections.push(format!("[Skill: {}]\n{}", skill.id, skill.content.trim()));
    }
    sections.join("\n\n")
}

fn build_system_instructions_with_context(
    rules: &str,
    skills: &[Skill],
    context: Option<&AgentRunContext>,
) -> String {
    let mut instructions = build_system_instructions(rules, skills);
    if let Some(AgentRunContext::ActiveDocs { document_id }) = context {
        instructions.push_str(&format!(
            "\n\n[Core Docs Run Policy]\nYou are working with one active DOCX document {document_id}.\nInspect it before proposing changes. Use its returned current version as baseVersion. Changes are proposals only and require user approval. You cannot commit, approve, reject, or retry changes."
        ));
    }
    instructions
}

async fn resolve_backend(
    registry: &Arc<nineprofs_agent::AgentRegistry>,
    backend_id: Option<&str>,
) -> Result<nineprofs_agent::AgentBackendDescriptor, AgentExecutionServiceError> {
    match registry.resolve_assistant_backend(backend_id).await {
        BackendResolution::NotConfigured => Err(AgentExecutionServiceError::BackendNotConfigured),
        BackendResolution::Missing { id } => Err(AgentExecutionServiceError::BackendMissing(id)),
        BackendResolution::Unknown { descriptor }
        | BackendResolution::Unavailable { descriptor } => {
            Err(AgentExecutionServiceError::BackendUnavailable(
                descriptor.id,
                descriptor
                    .availability_reason
                    .unwrap_or_else(|| "availability is unknown".to_owned()),
            ))
        }
        BackendResolution::Disabled { descriptor } => {
            Err(AgentExecutionServiceError::BackendDisabled(descriptor.id))
        }
        BackendResolution::Resolved { descriptor } => Ok(descriptor),
    }
}

fn map_conversation_error(error: DocsAgentConversationStoreError) -> AgentExecutionServiceError {
    match error {
        DocsAgentConversationStoreError::NotFound(id) => {
            AgentExecutionServiceError::ConversationNotFound(id)
        }
        DocsAgentConversationStoreError::Busy(id) => {
            AgentExecutionServiceError::ConversationBusy(id)
        }
        DocsAgentConversationStoreError::Unavailable(id) => {
            AgentExecutionServiceError::ConversationUnavailable(id)
        }
        DocsAgentConversationStoreError::TurnLimit => {
            AgentExecutionServiceError::ConversationTurnLimit
        }
        DocsAgentConversationStoreError::Capacity => {
            AgentExecutionServiceError::ConversationCapacity
        }
    }
}

fn validate_active_docs_document(
    descriptor: &ActiveDocumentDescriptor,
) -> Result<(), AgentExecutionServiceError> {
    if descriptor.document_type != DOCX_DOCUMENT_TYPE
        || descriptor.authority != GENOFFICE_ACTIVE_AUTHORITY
        || !matches!(descriptor.connection_state, ConnectionState::Connected)
        || !descriptor
            .capabilities
            .iter()
            .any(|value| value == DOCUMENT_BRIDGE_CAPABILITY_INSPECT)
        || !descriptor
            .capabilities
            .iter()
            .any(|value| value == DOCUMENT_BRIDGE_CAPABILITY_COMMIT)
    {
        return Err(AgentExecutionServiceError::ActiveDocumentUnsupported(
            descriptor.document_id.clone(),
        ));
    }
    Ok(())
}

async fn run_task(
    tasks: AgentTaskManager,
    events: Arc<BroadcastEventBus>,
    executor: Arc<dyn nineprofs_agent::AgentExecutor>,
    request: AgentExecutionRequest,
    cancellation: watch::Receiver<bool>,
) {
    if *cancellation.borrow() || tasks.start(&request.task_id).await.is_err() {
        return;
    }
    if *cancellation.borrow() {
        let _ = tasks.cancel(&request.task_id).await;
        return;
    }
    if tasks.mark_running(&request.task_id).await.is_err() {
        return;
    }

    let (event_sink, mut event_receiver) = mpsc::unbounded_channel();
    let task_id = request.task_id.clone();
    let run_id = request.run_id.clone();
    let cancellation_state = cancellation.clone();
    let execution =
        tokio::spawn(async move { executor.execute(request, event_sink, cancellation).await });
    tokio::pin!(execution);

    let mut event_state = ExecutionEventState::default();
    let result = loop {
        tokio::select! {
            event = event_receiver.recv() => {
                if let Some(event) = event {
                    if !*cancellation_state.borrow() {
                        publish_execution_event(&events, &run_id, &task_id, event, &mut event_state);
                    }
                }
            }
            result = &mut execution => break result,
        }
    };
    while let Ok(event) = event_receiver.try_recv() {
        if !*cancellation_state.borrow() {
            publish_execution_event(&events, &run_id, &task_id, event, &mut event_state);
        }
    }

    let outcome = match result {
        Ok(outcome) => outcome,
        Err(_) => Err(AgentExecutionError::Failed),
    };
    match outcome {
        Ok(_) => {
            if !is_terminal(&tasks, &task_id).await {
                let _ = tasks.complete(&task_id).await;
            }
        }
        Err(AgentExecutionError::Cancelled) => {
            if !is_terminal(&tasks, &task_id).await {
                let _ = tasks.cancel(&task_id).await;
            }
        }
        Err(error) => {
            if *cancellation_state.borrow() {
                if !is_terminal(&tasks, &task_id).await {
                    let _ = tasks.cancel(&task_id).await;
                }
            } else {
                if !event_state.error_emitted {
                    publish_execution_event(
                        &events,
                        &run_id,
                        &task_id,
                        AgentExecutionEvent::Error {
                            code: "execution_failed".to_owned(),
                            message: error.to_string(),
                        },
                        &mut event_state,
                    );
                }
                if !is_terminal(&tasks, &task_id).await {
                    let _ = tasks
                        .fail(
                            &task_id,
                            TaskFailure {
                                code: "execution_failed".to_owned(),
                                message: error.to_string(),
                            },
                        )
                        .await;
                }
            }
        }
    }
}

#[derive(Default)]
struct ExecutionEventState {
    output_started: bool,
    output_completed: bool,
    error_emitted: bool,
}

async fn is_terminal(tasks: &AgentTaskManager, task_id: &nineprofs_agent::AgentTaskId) -> bool {
    tasks
        .get(task_id)
        .await
        .is_some_and(|task| task.state.is_terminal())
}

fn publish_execution_event(
    events: &BroadcastEventBus,
    run_id: &RunId,
    task_id: &nineprofs_agent::AgentTaskId,
    event: AgentExecutionEvent,
    state: &mut ExecutionEventState,
) {
    let (name, details) = match event {
        AgentExecutionEvent::OutputStarted => {
            if state.output_started || state.output_completed {
                return;
            }
            state.output_started = true;
            ("agent.outputStarted", json!({}))
        }
        AgentExecutionEvent::OutputDelta { delta } => {
            if delta.is_empty() || state.output_completed {
                return;
            }
            ("agent.outputDelta", json!({ "delta": delta }))
        }
        AgentExecutionEvent::OutputCompleted { output } => {
            if state.output_completed {
                return;
            }
            state.output_completed = true;
            ("agent.outputCompleted", json!({ "output": output }))
        }
        AgentExecutionEvent::Error { code, message } => {
            if state.error_emitted {
                return;
            }
            state.error_emitted = true;
            ("agent.error", json!({ "code": code, "message": message }))
        }
        AgentExecutionEvent::ToolStarted { tool_call_id, name } => {
            if state.output_completed || state.error_emitted {
                return;
            }
            (
                "agent.toolStarted",
                json!({ "tool_call_id": tool_call_id, "tool": safe_tool_name(&name) }),
            )
        }
        AgentExecutionEvent::ToolCompleted {
            tool_call_id,
            name,
            is_error,
        } => {
            if state.output_completed || state.error_emitted {
                return;
            }
            (
                "agent.toolCompleted",
                json!({
                    "tool_call_id": tool_call_id,
                    "tool": safe_tool_name(&name),
                    "is_error": is_error,
                }),
            )
        }
    };
    let _ = events.publish(EventEnvelope::new(
        name,
        json!({ "run_id": run_id, "task_id": task_id, "details": details }),
    ));
}

fn safe_tool_name(name: &str) -> String {
    name.chars().take(128).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocsAgentConversationState;
    use async_trait::async_trait;
    use nineprofs_agent::{
        AgentEventSink, AgentExecutionResult, AgentExecutor, AgentRegistry, AvailabilityState,
        BuiltinAgentCatalog, SqliteAgentMetadataRepository, TaskState,
    };
    use nineprofs_assistant::{
        BuiltinAssistantCatalog, CreateAssistant, SqliteAssistantRepository,
    };
    use nineprofs_db::Database;
    use nineprofs_document_tools::DocumentToolProvider;
    use nineprofs_documents::{
        DOCUMENT_BRIDGE_CAPABILITY_COMMIT, DOCUMENT_BRIDGE_CAPABILITY_INSPECT,
        DOCUMENT_BRIDGE_PROTOCOL_VERSION, DOCX_DOCUMENT_TYPE, DocumentRegistration,
    };
    use nineprofs_realtime::BroadcastEventBus;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    struct FakeExecutor {
        backend_id: &'static str,
    }

    struct CapturingExecutor {
        captured: Arc<std::sync::Mutex<Option<AgentExecutionRequest>>>,
    }

    #[async_trait]
    impl AgentExecutor for CapturingExecutor {
        fn backend_id(&self) -> &str {
            "nineprofs-default"
        }

        async fn execute(
            &self,
            request: AgentExecutionRequest,
            sink: AgentEventSink,
            _cancellation: watch::Receiver<bool>,
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            *self.captured.lock().unwrap() = Some(request);
            let _ = sink.send(AgentExecutionEvent::OutputStarted);
            let _ = sink.send(AgentExecutionEvent::OutputCompleted {
                output: "captured".to_owned(),
            });
            Ok(AgentExecutionResult {
                output: "captured".to_owned(),
            })
        }
    }

    #[async_trait]
    impl AgentExecutor for FakeExecutor {
        fn backend_id(&self) -> &str {
            self.backend_id
        }

        async fn execute(
            &self,
            _request: AgentExecutionRequest,
            sink: AgentEventSink,
            _cancellation: watch::Receiver<bool>,
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            let _ = sink.send(AgentExecutionEvent::OutputStarted);
            let _ = sink.send(AgentExecutionEvent::OutputDelta {
                delta: "NINEPROFS_OK".to_owned(),
            });
            let _ = sink.send(AgentExecutionEvent::OutputCompleted {
                output: "NINEPROFS_OK".to_owned(),
            });
            Ok(AgentExecutionResult {
                output: "NINEPROFS_OK".to_owned(),
            })
        }
    }

    struct BlockingExecutor;

    #[async_trait]
    impl AgentExecutor for BlockingExecutor {
        fn backend_id(&self) -> &str {
            "blocking"
        }

        async fn execute(
            &self,
            _request: AgentExecutionRequest,
            sink: AgentEventSink,
            mut cancellation: watch::Receiver<bool>,
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            let _ = sink.send(AgentExecutionEvent::OutputStarted);
            let _ = sink.send(AgentExecutionEvent::OutputDelta {
                delta: "partial".to_owned(),
            });
            loop {
                cancellation
                    .changed()
                    .await
                    .map_err(|_| AgentExecutionError::Cancelled)?;
                if *cancellation.borrow() {
                    return Err(AgentExecutionError::Cancelled);
                }
            }
        }
    }

    struct LateSuccessExecutor;

    #[async_trait]
    impl AgentExecutor for LateSuccessExecutor {
        fn backend_id(&self) -> &str {
            "late-success"
        }

        async fn execute(
            &self,
            _request: AgentExecutionRequest,
            sink: AgentEventSink,
            mut cancellation: watch::Receiver<bool>,
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            cancellation
                .changed()
                .await
                .map_err(|_| AgentExecutionError::Cancelled)?;
            let _ = sink.send(AgentExecutionEvent::OutputCompleted {
                output: "late success".to_owned(),
            });
            Ok(AgentExecutionResult {
                output: "late success".to_owned(),
            })
        }
    }

    struct FailingExecutor;

    #[async_trait]
    impl AgentExecutor for FailingExecutor {
        fn backend_id(&self) -> &str {
            "failing"
        }

        async fn execute(
            &self,
            _request: AgentExecutionRequest,
            _sink: AgentEventSink,
            _cancellation: watch::Receiver<bool>,
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            Err(AgentExecutionError::Failed)
        }
    }

    fn test_request(task: &AgentTask) -> AgentExecutionRequest {
        AgentExecutionRequest {
            run_id: task.run_id.clone(),
            task_id: task.task_id.clone(),
            backend_id: task.backend_id.clone(),
            assistant_id: "test-assistant".to_owned(),
            input: "test input".to_owned(),
            workspace_root: None,
            provider: AgentProviderConfig::from_env(),
            system_instructions: "test instructions".to_owned(),
            limits: ExecutionLimits::default(),
            tool_set: ToolSet::default(),
            context: None,
            conversation_id: None,
        }
    }

    #[test]
    fn ordered_rules_and_skills_materialize_without_aionrs_discovery() {
        let executors = AgentExecutorRegistry::new([
            Arc::new(FakeExecutor { backend_id: "fake" }) as Arc<dyn AgentExecutor>
        ]);
        assert_eq!(executors.ids(), vec!["fake".to_owned()]);

        let skills = vec![
            Skill {
                id: "first".to_owned(),
                name: "First".to_owned(),
                description: "".to_owned(),
                source: nineprofs_skills::SkillSource::Builtin,
                location: nineprofs_skills::SkillLocation::Embedded {
                    path: "first".to_owned(),
                },
                content: "first content".to_owned(),
            },
            Skill {
                id: "second".to_owned(),
                name: "Second".to_owned(),
                description: "".to_owned(),
                source: nineprofs_skills::SkillSource::Builtin,
                location: nineprofs_skills::SkillLocation::Embedded {
                    path: "second".to_owned(),
                },
                content: "second content".to_owned(),
            },
        ];
        let instructions = build_system_instructions("Reply concisely.", &skills);
        assert!(
            instructions.find("first content").unwrap()
                < instructions.find("second content").unwrap()
        );
        assert!(instructions.contains("[Assistant Rules]"));
    }

    #[tokio::test]
    async fn execution_events_translate_to_transport_owned_names() {
        let events = BroadcastEventBus::new(16);
        let mut receiver = events.subscribe();
        let run_id = RunId::new();
        let task_id = nineprofs_agent::AgentTaskId::new();
        let mut state = ExecutionEventState::default();
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputStarted,
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputDelta {
                delta: String::new(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputDelta {
                delta: "delta".to_owned(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::ToolStarted {
                tool_call_id: "call-1".to_owned(),
                name: "document.inspect_active".to_owned(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::ToolCompleted {
                tool_call_id: "call-1".to_owned(),
                name: "document.inspect_active".to_owned(),
                is_error: false,
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputCompleted {
                output: "complete".to_owned(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputCompleted {
                output: "duplicate".to_owned(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::Error {
                code: "provider_error".to_owned(),
                message: "provider failed".to_owned(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::Error {
                code: "provider_error".to_owned(),
                message: "duplicate provider failure".to_owned(),
            },
            &mut state,
        );

        assert_eq!(receiver.recv().await.unwrap().name, "agent.outputStarted");
        assert_eq!(receiver.recv().await.unwrap().name, "agent.outputDelta");

        let tool_started = receiver.recv().await.unwrap();
        let tool_completed = receiver.recv().await.unwrap();
        assert_eq!(tool_started.name, "agent.toolStarted");
        assert_eq!(
            tool_started.payload["details"]["tool"],
            "document.inspect_active"
        );
        assert_eq!(tool_completed.name, "agent.toolCompleted");
        assert_eq!(tool_completed.payload["details"]["tool_call_id"], "call-1");
        assert!(
            !tool_completed.payload["details"]
                .to_string()
                .contains("document contents")
        );

        let names = [
            receiver.recv().await.unwrap().name,
            receiver.recv().await.unwrap().name,
        ];
        assert_eq!(
            names,
            ["agent.outputCompleted".to_owned(), "agent.error".to_owned(),]
        );
    }

    async fn test_service_with_executor(
        availability: AvailabilityState,
        executor: Arc<dyn AgentExecutor>,
    ) -> AgentExecutionService {
        let database = Database::in_memory().await.unwrap();
        let events = Arc::new(BroadcastEventBus::new(64));
        let document_bridge = Arc::new(DocumentBridgeService::new(
            Default::default(),
            Arc::clone(&events),
        ));
        let skills = Arc::new(
            SkillCatalog::with_configured_roots(Vec::<std::path::PathBuf>::new()).unwrap(),
        );
        let assistants = Arc::new(
            AssistantService::new(
                SqliteAssistantRepository::new(database.pool().clone()),
                BuiltinAssistantCatalog::load().unwrap(),
                Arc::clone(&skills),
                Arc::clone(&events),
            )
            .unwrap(),
        );
        assistants
            .create(CreateAssistant {
                id: Some("execution-assistant".to_owned()),
                name: "Execution assistant".to_owned(),
                description: "test assistant".to_owned(),
                backend_agent_id: Some("nineprofs-default".to_owned()),
                rules: "test assistant rule".to_owned(),
                ..CreateAssistant::default()
            })
            .await
            .unwrap();

        let registry = Arc::new(AgentRegistry::new(
            Arc::new(SqliteAgentMetadataRepository::new(database.pool().clone())),
            BuiltinAgentCatalog::load(),
            Arc::clone(&events),
        ));
        registry.hydrate().await.unwrap();
        registry
            .set_availability("nineprofs-default", availability, None)
            .await
            .unwrap();

        let executor_registry = AgentExecutorRegistry::new([executor]);
        let tasks = AgentTaskManager::new(Arc::clone(&events));
        let tools = ToolRegistry::new();
        tools
            .register_provider(&DocumentToolProvider::new(
                Arc::clone(&document_bridge),
                Arc::clone(&events),
            ))
            .await
            .unwrap();
        AgentExecutionService::new(
            assistants,
            skills,
            registry,
            executor_registry,
            tasks,
            events,
            AgentProviderConfig::from_env(),
            document_bridge,
            tools,
        )
    }

    async fn test_service(availability: AvailabilityState) -> AgentExecutionService {
        test_service_with_executor(
            availability,
            Arc::new(FakeExecutor {
                backend_id: "nineprofs-default",
            }),
        )
        .await
    }

    #[tokio::test]
    async fn execution_service_resolves_assistant_and_fake_executor() {
        let service = test_service(AvailabilityState::Available).await;
        let started = service
            .start_run("execution-assistant", "Reply with exactly: NINEPROFS_OK")
            .await
            .unwrap();

        for _ in 0..100 {
            if let Some(task) = service.task(&started.task.task_id).await {
                if task.state == TaskState::Succeeded {
                    return;
                }
            }
            tokio::task::yield_now().await;
        }
        panic!("fake execution did not reach succeeded state");
    }

    #[tokio::test]
    async fn generic_start_run_remains_toolless() {
        let captured = Arc::new(std::sync::Mutex::new(None));
        let service = test_service_with_executor(
            AvailabilityState::Available,
            Arc::new(CapturingExecutor {
                captured: Arc::clone(&captured),
            }),
        )
        .await;

        service
            .start_run("execution-assistant", "generic input")
            .await
            .unwrap();

        for _ in 0..100 {
            if captured.lock().unwrap().is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let request = captured.lock().unwrap().clone().unwrap();
        assert!(request.tool_set.is_empty());
        assert!(request.context.is_none());
    }

    #[tokio::test]
    async fn active_docs_run_validates_context_policy_and_assistant_setup() {
        let captured = Arc::new(std::sync::Mutex::new(None));
        let service = test_service_with_executor(
            AvailabilityState::Available,
            Arc::new(CapturingExecutor {
                captured: Arc::clone(&captured),
            }),
        )
        .await;
        let (sender, _receiver) = mpsc::channel(8);
        service
            .document_bridge
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: "doc-a".to_owned(),
                    document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                    version: 7,
                    capabilities: vec![
                        DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();

        let started = service
            .start_active_docs_run("execution-assistant", "doc-a", "inspect document")
            .await
            .unwrap();
        assert_eq!(
            started.context,
            Some(AgentRunContext::ActiveDocs {
                document_id: "doc-a".to_owned(),
            })
        );

        for _ in 0..100 {
            if captured.lock().unwrap().is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let request = captured.lock().unwrap().clone().unwrap();
        assert_eq!(request.context, started.context);
        assert_eq!(
            request
                .tool_set
                .ids()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "document.inspect_active",
                "document.list_active",
                "document.propose_active_changes"
            ]
        );
        for forbidden in [
            "office.create",
            "office.mutate_detached",
            "mcp.example",
            "document.commit",
            "document.approve",
            "document.reject",
            "document.retry",
        ] {
            assert!(!request.tool_set.contains(&forbidden.into()));
        }
        assert!(request.system_instructions.contains("[Assistant Rules]"));
        assert!(
            request
                .system_instructions
                .contains("Changes are proposals only and require user approval")
        );
        assert!(
            request
                .system_instructions
                .contains("cannot commit, approve, reject, or retry")
        );

        assert!(matches!(
            service
                .start_active_docs_run("execution-assistant", "missing", "input")
                .await,
            Err(AgentExecutionServiceError::ActiveDocumentUnavailable(id)) if id == "missing"
        ));

        let (sender, _receiver) = mpsc::channel(8);
        service
            .document_bridge
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: "pdf-a".to_owned(),
                    document_type: "pdf".to_owned(),
                    version: 1,
                    capabilities: vec![
                        DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();
        assert!(matches!(
            service
                .start_active_docs_run("execution-assistant", "pdf-a", "input")
                .await,
            Err(AgentExecutionServiceError::ActiveDocumentUnsupported(id)) if id == "pdf-a"
        ));
    }

    #[tokio::test]
    async fn docs_conversation_reuses_binding_and_refreshes_each_turn_identity() {
        let captured = Arc::new(std::sync::Mutex::new(None));
        let service = test_service_with_executor(
            AvailabilityState::Available,
            Arc::new(CapturingExecutor {
                captured: Arc::clone(&captured),
            }),
        )
        .await;
        let (sender, _receiver) = mpsc::channel(8);
        service
            .document_bridge
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: "conversation-doc".to_owned(),
                    document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                    version: 1,
                    capabilities: vec![
                        DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();

        let conversation = service
            .create_docs_agent_conversation("execution-assistant", "conversation-doc")
            .await
            .unwrap();
        assert!(conversation.conversation_id.starts_with("docs-"));

        let first = service
            .start_docs_agent_conversation_run(&conversation.conversation_id, "My marker is ALPHA.")
            .await
            .unwrap();
        for _ in 0..100 {
            if service
                .task(&first.task.task_id)
                .await
                .is_some_and(|task| task.state == TaskState::Succeeded)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
        let first_request = captured.lock().unwrap().take().unwrap();
        assert_eq!(
            first_request.conversation_id,
            Some(conversation.conversation_id.clone())
        );
        assert_eq!(
            first_request.context,
            Some(AgentRunContext::ActiveDocs {
                document_id: "conversation-doc".to_owned(),
            })
        );
        assert_eq!(
            first_request
                .tool_set
                .ids()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "document.inspect_active",
                "document.list_active",
                "document.propose_active_changes",
            ]
        );

        for _ in 0..100 {
            if service
                .docs_agent_conversation(&conversation.conversation_id)
                .is_some_and(|metadata| metadata.turn_count == 1)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
        let second = service
            .start_docs_agent_conversation_run(
                &conversation.conversation_id,
                "What marker did I give you?",
            )
            .await
            .unwrap();
        for _ in 0..100 {
            if captured.lock().unwrap().is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let second_request = captured.lock().unwrap().take().unwrap();
        assert_eq!(
            second_request.conversation_id,
            Some(conversation.conversation_id)
        );
        assert_ne!(first_request.run_id, second_request.run_id);
        assert_ne!(first_request.task_id, second_request.task_id);
        assert_eq!(
            second_request
                .tool_set
                .ids()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "document.inspect_active",
                "document.list_active",
                "document.propose_active_changes",
            ]
        );
        assert_eq!(second.context, first.context);
    }

    #[tokio::test]
    async fn docs_conversation_becomes_unavailable_when_bound_document_is_replaced() {
        let service = test_service(AvailabilityState::Available).await;
        let (sender, _receiver) = mpsc::channel(8);
        service
            .document_bridge
            .register(
                DocumentRegistration {
                    protocol_version: DOCUMENT_BRIDGE_PROTOCOL_VERSION.to_owned(),
                    document_id: "replace-a".to_owned(),
                    document_type: DOCX_DOCUMENT_TYPE.to_owned(),
                    version: 1,
                    capabilities: vec![
                        DOCUMENT_BRIDGE_CAPABILITY_INSPECT.to_owned(),
                        DOCUMENT_BRIDGE_CAPABILITY_COMMIT.to_owned(),
                    ],
                },
                sender,
            )
            .await
            .unwrap();
        let conversation = service
            .create_docs_agent_conversation("execution-assistant", "replace-a")
            .await
            .unwrap();
        let session_id = service
            .document_bridge
            .get("replace-a")
            .await
            .unwrap()
            .session_id;
        service
            .document_bridge
            .unregister("replace-a", &session_id)
            .await
            .unwrap();

        assert!(matches!(
            service
                .start_docs_agent_conversation_run(&conversation.conversation_id, "follow-up")
                .await,
            Err(AgentExecutionServiceError::ConversationUnavailable(id)) if id == conversation.conversation_id
        ));
        assert_eq!(
            service
                .docs_agent_conversation(&conversation.conversation_id)
                .unwrap()
                .state,
            DocsAgentConversationState::Unavailable
        );
    }

    #[tokio::test]
    async fn execution_service_rejects_missing_assistant_and_unavailable_backend() {
        let service = test_service(AvailabilityState::Available).await;
        assert!(matches!(
            service.start_run("missing-assistant", "input").await,
            Err(AgentExecutionServiceError::Assistant(
                nineprofs_assistant::AssistantError::NotFound(_)
            ))
        ));

        let unavailable = test_service(AvailabilityState::Unavailable).await;
        assert!(matches!(
            unavailable
                .start_run("execution-assistant", "input")
                .await,
            Err(AgentExecutionServiceError::BackendUnavailable(id, _)) if id == "nineprofs-default"
        ));

        service
            .assistants
            .create(CreateAssistant {
                id: Some("missing-backend-assistant".to_owned()),
                name: "Missing backend assistant".to_owned(),
                description: "test assistant".to_owned(),
                backend_agent_id: Some("missing-backend".to_owned()),
                ..CreateAssistant::default()
            })
            .await
            .unwrap();
        assert!(matches!(
            service
                .start_run("missing-backend-assistant", "input")
                .await,
            Err(AgentExecutionServiceError::BackendMissing(id)) if id == "missing-backend"
        ));

        service
            .assistants
            .create(CreateAssistant {
                id: Some("unconfigured-assistant".to_owned()),
                name: "Unconfigured assistant".to_owned(),
                description: "test assistant".to_owned(),
                ..CreateAssistant::default()
            })
            .await
            .unwrap();
        assert!(matches!(
            service.start_run("unconfigured-assistant", "input").await,
            Err(AgentExecutionServiceError::BackendNotConfigured)
        ));
    }

    #[tokio::test]
    async fn run_task_maps_execution_failure_to_failed_terminal_state() {
        let events = Arc::new(BroadcastEventBus::new(32));
        let mut receiver = events.subscribe();
        let tasks = AgentTaskManager::new(Arc::clone(&events));
        let task = tasks.register_new(RunId::new(), "failing").await.unwrap();
        let cancellation = tasks.cancellation(&task.task_id).await.unwrap();

        run_task(
            tasks.clone(),
            events,
            Arc::new(FailingExecutor),
            test_request(&task),
            cancellation,
        )
        .await;

        let task = tasks.get(&task.task_id).await.unwrap();
        assert_eq!(task.state, TaskState::Failed);
        assert_eq!(task.failure.as_ref().unwrap().code, "execution_failed");
        assert_eq!(
            std::iter::from_fn(|| receiver.try_recv().ok())
                .filter(|event| event.name == "agent.error")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn run_task_cancellation_stops_executor_and_leaves_terminal_task() {
        let events = Arc::new(BroadcastEventBus::new(32));
        let mut receiver = events.subscribe();
        let tasks = AgentTaskManager::new(Arc::clone(&events));
        let task = tasks.register_new(RunId::new(), "blocking").await.unwrap();
        let cancellation = tasks.cancellation(&task.task_id).await.unwrap();
        let task_id = task.task_id.clone();
        let execution = tokio::spawn(run_task(
            tasks.clone(),
            events,
            Arc::new(BlockingExecutor),
            test_request(&task),
            cancellation,
        ));

        for _ in 0..100 {
            if tasks.get(&task_id).await.unwrap().state == TaskState::Running {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(tasks.get(&task_id).await.unwrap().state, TaskState::Running);
        tasks.cancel(&task_id).await.unwrap();
        execution.await.unwrap();

        let task = tasks.get(&task_id).await.unwrap();
        assert_eq!(task.state, TaskState::Cancelled);
        assert!(task.state.is_terminal());
        assert!(
            !std::iter::from_fn(|| receiver.try_recv().ok())
                .any(|event| event.name == "agent.outputCompleted")
        );
    }

    #[tokio::test]
    async fn cancellation_wins_against_late_success_completion() {
        let events = Arc::new(BroadcastEventBus::new(32));
        let mut receiver = events.subscribe();
        let tasks = AgentTaskManager::new(Arc::clone(&events));
        let task = tasks
            .register_new(RunId::new(), "late-success")
            .await
            .unwrap();
        let task_id = task.task_id.clone();
        let cancellation = tasks.cancellation(&task_id).await.unwrap();
        let execution = tokio::spawn(run_task(
            tasks.clone(),
            events,
            Arc::new(LateSuccessExecutor),
            test_request(&task),
            cancellation,
        ));

        for _ in 0..100 {
            if tasks.get(&task_id).await.unwrap().state == TaskState::Running {
                break;
            }
            tokio::task::yield_now().await;
        }
        tasks.cancel(&task_id).await.unwrap();
        execution.await.unwrap();

        assert_eq!(
            tasks.get(&task_id).await.unwrap().state,
            TaskState::Cancelled
        );
        assert!(
            !std::iter::from_fn(|| receiver.try_recv().ok())
                .any(|event| event.name == "agent.outputCompleted")
        );
    }
}
