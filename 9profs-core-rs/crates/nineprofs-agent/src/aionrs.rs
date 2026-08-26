use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use aion_agent::{
    engine::AgentEngine,
    output::OutputSink,
    session::{Session, SessionManager},
};
use aion_config::config::CliArgs;
use async_trait::async_trait;
use tokio::sync::watch;

use crate::{
    AgentEventSink, AgentExecutionError, AgentExecutionEvent, AgentExecutionRequest,
    AgentExecutionResult, AgentExecutor, AgentProviderConfig, AgentRunContext, RunId,
    aionrs_tools::build_aionrs_tool_registry,
};
use nineprofs_tools::{ToolInvocationContext, ToolInvocationScope, ToolRegistry};

pub const NINEPROFS_DEFAULT_BACKEND_ID: &str = "nineprofs-default";

const MAX_CONVERSATIONS: usize = 32;
const MAX_IDLE: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const MAX_SESSION_BYTES: usize = 4 * 1024 * 1024;

struct StoredConversation {
    session: Session,
    directory: PathBuf,
    last_used: Instant,
    active: bool,
}

struct AionRsConversationStore {
    root: PathBuf,
    states: Mutex<HashMap<String, StoredConversation>>,
}

impl AionRsConversationStore {
    fn new() -> Self {
        Self {
            root: std::env::temp_dir()
                .join("9profs-core-docs-agent")
                .join(RunId::new().to_string()),
            states: Mutex::new(HashMap::new()),
        }
    }

    fn begin(
        self: &Arc<Self>,
        conversation_id: &str,
        provider: &str,
        model: &str,
        cwd: &str,
    ) -> Result<AionRsConversationLease, AgentExecutionError> {
        if !is_safe_conversation_id(conversation_id) {
            return Err(AgentExecutionError::Configuration(
                "conversation state identity is invalid".to_owned(),
            ));
        }

        let mut states = self
            .states
            .lock()
            .expect("AionRS conversation store lock poisoned");
        let expired_ids: Vec<_> = states
            .iter()
            .filter(|(_, state)| !state.active && state.last_used.elapsed() > MAX_IDLE)
            .map(|(id, _)| id.clone())
            .collect();
        for expired_id in expired_ids {
            if let Some(expired) = states.remove(&expired_id) {
                let _ = std::fs::remove_dir_all(expired.directory);
            }
        }
        if let Some(state) = states.get(conversation_id)
            && state.active
        {
            return Err(AgentExecutionError::Configuration(
                "conversation is already running".to_owned(),
            ));
        }

        if !states.contains_key(conversation_id) && states.len() >= MAX_CONVERSATIONS {
            let Some(evicted_id) = states
                .iter()
                .filter(|(_, state)| !state.active)
                .min_by_key(|(_, state)| state.last_used)
                .map(|(id, _)| id.clone())
            else {
                return Err(AgentExecutionError::Configuration(
                    "conversation state store is at capacity".to_owned(),
                ));
            };
            if let Some(evicted) = states.remove(&evicted_id) {
                let _ = std::fs::remove_dir_all(evicted.directory);
            }
        }

        let (session, directory) = if let Some(state) = states.get(conversation_id) {
            (state.session.clone(), state.directory.clone())
        } else {
            let directory = self.root.join(conversation_id);
            let manager = SessionManager::new(directory.clone(), 1);
            let session = manager
                .create(provider, model, cwd, Some(conversation_id))
                .map_err(|_| {
                    AgentExecutionError::Configuration(
                        "AionRS conversation state could not be initialized".to_owned(),
                    )
                })?;
            (session, directory)
        };

        let manager = SessionManager::new(directory.clone(), 1);
        manager.save(&session).map_err(|_| {
            AgentExecutionError::Configuration(
                "AionRS conversation state could not be checkpointed".to_owned(),
            )
        })?;
        states.insert(
            conversation_id.to_owned(),
            StoredConversation {
                session: session.clone(),
                directory: directory.clone(),
                last_used: Instant::now(),
                active: true,
            },
        );
        drop(states);

        Ok(AionRsConversationLease {
            store: Arc::clone(self),
            conversation_id: conversation_id.to_owned(),
            previous: session,
            directory,
            committed: false,
        })
    }

    fn commit(
        &self,
        conversation_id: &str,
        session: Session,
        directory: &PathBuf,
    ) -> Result<(), AgentExecutionError> {
        let state_size = serde_json::to_vec(&session)
            .map_err(|_| {
                AgentExecutionError::Configuration(
                    "conversation state could not be measured".to_owned(),
                )
            })?
            .len();
        if state_size > MAX_SESSION_BYTES {
            return Err(AgentExecutionError::Configuration(
                "conversation state exceeded the Core memory limit".to_owned(),
            ));
        }
        SessionManager::new(directory.clone(), 1)
            .save(&session)
            .map_err(|_| {
                AgentExecutionError::Configuration(
                    "conversation state could not be committed".to_owned(),
                )
            })?;
        let mut states = self
            .states
            .lock()
            .expect("AionRS conversation store lock poisoned");
        if let Some(state) = states.get_mut(conversation_id) {
            state.session = session;
            state.last_used = Instant::now();
            state.active = false;
        }
        Ok(())
    }

    fn rollback(&self, conversation_id: &str, session: &Session, directory: &PathBuf) {
        let _ = SessionManager::new(directory.clone(), 1).save(session);
        if let Ok(mut states) = self.states.lock()
            && let Some(state) = states.get_mut(conversation_id)
        {
            state.session = session.clone();
            state.last_used = Instant::now();
            state.active = false;
        }
    }
}

impl Drop for AionRsConversationStore {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        if let Some(parent) = self.root.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

struct AionRsConversationLease {
    store: Arc<AionRsConversationStore>,
    conversation_id: String,
    previous: Session,
    directory: PathBuf,
    committed: bool,
}

impl AionRsConversationLease {
    fn commit(&mut self, session: Session) -> Result<(), AgentExecutionError> {
        self.store
            .commit(&self.conversation_id, session, &self.directory)?;
        self.committed = true;
        Ok(())
    }

    fn rollback(&mut self) {
        self.store
            .rollback(&self.conversation_id, &self.previous, &self.directory);
        self.committed = true;
    }
}

impl Drop for AionRsConversationLease {
    fn drop(&mut self) {
        if !self.committed {
            self.store
                .rollback(&self.conversation_id, &self.previous, &self.directory);
        }
    }
}

fn is_safe_conversation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

pub struct AionRsExecutor {
    provider: AgentProviderConfig,
    tools: ToolRegistry,
    conversations: Arc<AionRsConversationStore>,
}

impl AionRsExecutor {
    pub fn from_env() -> Self {
        Self::new(AgentProviderConfig::from_env())
    }

    pub fn new(provider: AgentProviderConfig) -> Self {
        Self::with_tools(provider, ToolRegistry::new())
    }

    pub fn from_env_with_tools(tools: ToolRegistry) -> Self {
        Self::with_tools(AgentProviderConfig::from_env(), tools)
    }

    pub fn with_tools(provider: AgentProviderConfig, tools: ToolRegistry) -> Self {
        Self {
            provider,
            tools,
            conversations: Arc::new(AionRsConversationStore::new()),
        }
    }

    pub fn provider(&self) -> &AgentProviderConfig {
        &self.provider
    }

    pub fn availability_reason(&self) -> Option<String> {
        self.provider.configuration_reason()
    }
}

struct AionRsOutputSink {
    events: AgentEventSink,
    started: AtomicBool,
}

impl AionRsOutputSink {
    fn new(events: AgentEventSink) -> Self {
        Self {
            events,
            started: AtomicBool::new(false),
        }
    }
}

impl OutputSink for AionRsOutputSink {
    fn emit_text_delta(&self, text: &str, _msg_id: &str) {
        if text.is_empty() {
            return;
        }
        let _ = self.events.send(AgentExecutionEvent::OutputDelta {
            delta: text.to_owned(),
        });
    }

    fn emit_thinking(&self, _text: &str, _msg_id: &str) {}

    fn emit_tool_call(&self, tool_use_id: &str, name: &str, _input: &str) {
        if tool_use_id.is_empty() || name.is_empty() {
            return;
        }
        let _ = self.events.send(AgentExecutionEvent::ToolStarted {
            tool_call_id: tool_use_id.to_owned(),
            name: name.to_owned(),
        });
    }

    fn emit_tool_result(&self, tool_use_id: &str, name: &str, is_error: bool, _content: &str) {
        if tool_use_id.is_empty() || name.is_empty() {
            return;
        }
        let _ = self.events.send(AgentExecutionEvent::ToolCompleted {
            tool_call_id: tool_use_id.to_owned(),
            name: name.to_owned(),
            is_error,
        });
    }

    fn emit_stream_start(&self, _msg_id: &str) {
        if !self.started.swap(true, Ordering::AcqRel) {
            let _ = self.events.send(AgentExecutionEvent::OutputStarted);
        }
    }

    fn emit_stream_end(
        &self,
        _msg_id: &str,
        _turns: usize,
        _input_tokens: u64,
        _output_tokens: u64,
        _cache_creation_tokens: u64,
        _cache_read_tokens: u64,
    ) {
    }

    fn emit_error(&self, _msg: &str) {
        // AgentEngine::run returns the terminal provider error. The executor
        // maps that outcome once so the runtime does not publish duplicate
        // agent.error events.
    }

    fn emit_info(&self, _msg: &str) {}
}

#[async_trait]
impl AgentExecutor for AionRsExecutor {
    fn backend_id(&self) -> &str {
        NINEPROFS_DEFAULT_BACKEND_ID
    }

    async fn execute(
        &self,
        request: AgentExecutionRequest,
        event_sink: AgentEventSink,
        mut cancellation: watch::Receiver<bool>,
    ) -> Result<AgentExecutionResult, AgentExecutionError> {
        if request.input.trim().is_empty() {
            return Err(AgentExecutionError::Configuration(
                "input must not be empty".to_owned(),
            ));
        }
        if *cancellation.borrow() {
            return Err(AgentExecutionError::Cancelled);
        }

        let secret = request
            .provider
            .configured_secret()
            .map_err(|error| AgentExecutionError::Configuration(error.to_string()))?;

        let cli_args = CliArgs {
            provider: Some(request.provider.provider.clone()),
            api_key: Some(secret),
            base_url: request.provider.base_url.clone(),
            model: Some(request.provider.model.clone()),
            max_tokens: request.limits.max_output_tokens,
            thinking: None,
            thinking_budget: None,
            max_turns: request.limits.max_turns,
            max_tool_call_malformed_turns: None,
            max_tool_call_failure_turns: None,
            system_prompt: Some(request.system_instructions.clone()),
            profile: None,
            auto_approve: false,
            project_dir: request.workspace_root.clone(),
        };
        let mut config = aion_config::config::Config::resolve(&cli_args).map_err(|_| {
            AgentExecutionError::Configuration("AionRS configuration invalid".to_owned())
        })?;
        config.system_prompt = Some(request.system_instructions.clone());
        config.session.enabled = false;
        config.mcp.servers.clear();
        config.tools.allow_list.clear();
        config.tools.auto_approve = false;
        config.tools.skills.allow.clear();
        config.tools.skills.deny.clear();
        config.hooks = Default::default();
        config.shell = Default::default();

        let workspace = request.workspace_root.unwrap_or_else(|| PathBuf::from("."));
        let output: Arc<dyn OutputSink> = Arc::new(AionRsOutputSink::new(event_sink.clone()));
        let mut invocation_context =
            ToolInvocationContext::new(request.run_id.as_str(), request.task_id.as_str());
        if let Some(AgentRunContext::ActiveDocs { document_id }) = request.context.as_ref() {
            invocation_context =
                invocation_context.with_scope(ToolInvocationScope::ActiveDocument {
                    document_id: document_id.clone(),
                });
        }
        let aionrs_tools =
            build_aionrs_tool_registry(&self.tools, &request.tool_set, invocation_context)
                .map_err(|error| AgentExecutionError::Configuration(error.to_string()))?;
        let mut conversation_lease =
            if let Some(conversation_id) = request.conversation_id.as_deref() {
                let workspace_string = workspace.to_string_lossy().into_owned();
                let lease = self.conversations.begin(
                    conversation_id,
                    &request.provider.provider,
                    &request.provider.model,
                    &workspace_string,
                )?;
                config.session.enabled = true;
                config.session.directory = lease.directory.to_string_lossy().into_owned();
                Some(lease)
            } else {
                None
            };
        let mut engine = if let Some(lease) = conversation_lease.as_ref() {
            AgentEngine::resume(
                config,
                aionrs_tools,
                output,
                lease.previous.clone(),
                workspace,
            )
        } else {
            AgentEngine::new(config, aionrs_tools, output, workspace)
        };
        let run = engine.run(&request.input, request.task_id.as_str());
        let result = tokio::select! {
            result = run => match result {
                Ok(result) => result,
                Err(_) => {
                    if let Some(lease) = conversation_lease.as_mut() {
                        lease.rollback();
                    }
                    return Err(AgentExecutionError::Failed);
                }
            },
            changed = wait_for_cancellation(&mut cancellation) => {
                changed?;
                engine.abort_current_turn("cancelled by 9Profs");
                if let Some(lease) = conversation_lease.as_mut() {
                    lease.rollback();
                }
                return Err(AgentExecutionError::Cancelled);
            }
        };
        if let Some(lease) = conversation_lease.as_mut() {
            let session_id = lease.previous.id.clone();
            let session = SessionManager::new(lease.directory.clone(), 1)
                .load(&session_id)
                .map_err(|_| AgentExecutionError::Failed)?;
            lease.commit(session)?;
        }
        let _ = event_sink.send(AgentExecutionEvent::OutputCompleted {
            output: result.text.clone(),
        });
        Ok(AgentExecutionResult {
            output: result.text,
        })
    }
}

async fn wait_for_cancellation(
    cancellation: &mut watch::Receiver<bool>,
) -> Result<(), AgentExecutionError> {
    loop {
        cancellation
            .changed()
            .await
            .map_err(|_| AgentExecutionError::Cancelled)?;
        if *cancellation.borrow() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aion_types::message::{ContentBlock, Message, Role};
    use nineprofs_tools::ToolSet;

    #[test]
    fn executor_exposes_only_nineprofs_backend_id() {
        let executor = AionRsExecutor::new(AgentProviderConfig {
            provider: "openai".to_owned(),
            model: "test".to_owned(),
            base_url: None,
            api_key_env: "OPENAI_API_KEY".to_owned(),
        });
        assert_eq!(executor.backend_id(), NINEPROFS_DEFAULT_BACKEND_ID);
        assert!(executor.availability_reason().is_some());
    }

    #[test]
    fn phase_2b1_tool_surface_is_empty() {
        let names = aion_tools::registry::ToolRegistry::new().tool_names();
        assert!(names.is_empty());
        for forbidden in [
            "shell",
            "filesystem",
            "write_file",
            "edit_file",
            "exec_command",
            "mcp",
            "sub_agent",
            "skills",
        ] {
            assert!(!names.iter().any(|name| name == forbidden));
        }
    }

    #[test]
    fn availability_requires_valid_openai_and_anthropic_configuration() {
        let openai_key = "NINEPROFS_TEST_AIONRS_OPENAI_KEY";
        unsafe { std::env::set_var(openai_key, "test-secret") };
        let openai = AionRsExecutor::new(AgentProviderConfig {
            provider: "openai".to_owned(),
            model: "gpt-4o-mini".to_owned(),
            base_url: None,
            api_key_env: openai_key.to_owned(),
        });
        assert!(openai.availability_reason().is_none());
        unsafe { std::env::remove_var(openai_key) };

        let anthropic_key = "NINEPROFS_TEST_AIONRS_ANTHROPIC_KEY";
        unsafe { std::env::set_var(anthropic_key, "test-secret") };
        let anthropic = AionRsExecutor::new(AgentProviderConfig {
            provider: "anthropic".to_owned(),
            model: "claude-sonnet-test".to_owned(),
            base_url: None,
            api_key_env: anthropic_key.to_owned(),
        });
        assert!(anthropic.availability_reason().is_none());
        unsafe { std::env::remove_var(anthropic_key) };
    }

    #[tokio::test]
    async fn real_provider_smoke_is_opt_in() {
        if std::env::var("NINEPROFS_RUN_REAL_AGENT_SMOKE").as_deref() != Ok("1") {
            return;
        }

        let executor = AionRsExecutor::from_env();
        if executor.availability_reason().is_some() {
            return;
        }

        let events = std::sync::Arc::new(nineprofs_realtime::BroadcastEventBus::new(16));
        let tasks = crate::AgentTaskManager::new(events);
        let run_id = crate::RunId::new();
        let task = tasks
            .register_new(run_id.clone(), NINEPROFS_DEFAULT_BACKEND_ID)
            .await
            .unwrap();
        let cancellation = tasks.cancellation(&task.task_id).await.unwrap();
        tasks.start(&task.task_id).await.unwrap();
        tasks.mark_running(&task.task_id).await.unwrap();

        let (event_sink, mut event_receiver) = tokio::sync::mpsc::unbounded_channel();
        let request = AgentExecutionRequest {
            run_id,
            task_id: task.task_id.clone(),
            backend_id: NINEPROFS_DEFAULT_BACKEND_ID.to_owned(),
            assistant_id: "real-provider-smoke".to_owned(),
            input: "Reply with exactly: NINEPROFS_OK".to_owned(),
            workspace_root: None,
            provider: executor.provider().clone(),
            system_instructions: "Reply concisely and follow the requested exact output."
                .to_owned(),
            limits: crate::ExecutionLimits::default(),
            tool_set: ToolSet::default(),
            context: None,
            conversation_id: None,
        };
        let result = executor
            .execute(request, event_sink, cancellation)
            .await
            .unwrap();
        assert!(result.output.contains("NINEPROFS_OK"));
        tasks.complete(&task.task_id).await.unwrap();
        assert_eq!(
            tasks.get(&task.task_id).await.unwrap().state,
            crate::TaskState::Succeeded
        );

        let mut saw_started = false;
        let mut saw_delta = false;
        let mut saw_completed = false;
        while let Ok(event) = event_receiver.try_recv() {
            match event {
                AgentExecutionEvent::OutputStarted => saw_started = true,
                AgentExecutionEvent::OutputDelta { .. } => saw_delta = true,
                AgentExecutionEvent::OutputCompleted { .. } => saw_completed = true,
                AgentExecutionEvent::Error { .. } => {}
                AgentExecutionEvent::ToolStarted { .. }
                | AgentExecutionEvent::ToolCompleted { .. } => {}
            }
        }
        assert!(saw_started && saw_delta && saw_completed);
    }

    #[test]
    fn output_sink_emits_tool_lifecycle_without_input_or_output_payloads() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let sink = AionRsOutputSink::new(sender);
        sink.emit_tool_call("call-1", "document.inspect_active", "manuscript contents");
        sink.emit_tool_result(
            "call-1",
            "document.inspect_active",
            true,
            "document contents and credentials",
        );

        assert_eq!(
            receiver.try_recv().unwrap(),
            AgentExecutionEvent::ToolStarted {
                tool_call_id: "call-1".to_owned(),
                name: "document.inspect_active".to_owned(),
            }
        );
        assert_eq!(
            receiver.try_recv().unwrap(),
            AgentExecutionEvent::ToolCompleted {
                tool_call_id: "call-1".to_owned(),
                name: "document.inspect_active".to_owned(),
                is_error: true,
            }
        );
    }

    #[test]
    fn conversation_checkpoint_rollback_is_core_owned_and_ephemeral() {
        let store = Arc::new(AionRsConversationStore::new());
        let mut lease = store
            .begin("docs-test", "openai", "test-model", ".")
            .unwrap();
        let directory = lease.directory.clone();
        assert!(directory.starts_with(std::env::temp_dir()));
        assert!(!directory.starts_with(std::env::current_dir().unwrap()));
        assert!(directory.exists());
        lease.rollback();
        let states = store.states.lock().unwrap();
        assert!(!states.get("docs-test").unwrap().active);
        assert!(states.get("docs-test").unwrap().session.messages.is_empty());
        drop(states);
        drop(lease);
        drop(store.clone());
        assert!(directory.exists());
        drop(store);
        assert!(!directory.exists());
    }

    #[test]
    fn conversation_checkpoint_preserves_real_message_roles_across_turns() {
        let store = Arc::new(AionRsConversationStore::new());
        let mut first = store
            .begin("docs-continuity", "openai", "test-model", ".")
            .unwrap();
        let mut first_session = first.previous.clone();
        first_session.messages.push(Message::now(
            Role::User,
            vec![ContentBlock::Text {
                text: "My marker is ALPHA.".to_owned(),
            }],
        ));
        first_session.messages.push(Message::now(
            Role::Assistant,
            vec![ContentBlock::Text {
                text: "Acknowledged.".to_owned(),
            }],
        ));
        first.commit(first_session).unwrap();

        let mut second = store
            .begin("docs-continuity", "openai", "test-model", ".")
            .unwrap();
        assert_eq!(second.previous.messages.len(), 2);
        assert!(matches!(second.previous.messages[0].role, Role::User));
        assert!(matches!(second.previous.messages[1].role, Role::Assistant));
        let mut partial = second.previous.clone();
        partial.messages.push(Message::now(
            Role::User,
            vec![ContentBlock::Text {
                text: "failed follow-up".to_owned(),
            }],
        ));
        assert_eq!(partial.messages.len(), 3);
        second.rollback();
        let retry = store
            .begin("docs-continuity", "openai", "test-model", ".")
            .unwrap();
        assert_eq!(retry.previous.messages.len(), 2);
        assert!(
            !retry
                .previous
                .messages
                .iter()
                .any(|message| message.content.iter().any(|block| matches!(
                    block,
                    ContentBlock::Text { text } if text == "failed follow-up"
                )))
        );
        drop(retry);
        drop(store);
    }
}
