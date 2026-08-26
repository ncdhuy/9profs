use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use aion_agent::{engine::AgentEngine, output::OutputSink};
use aion_config::config::CliArgs;
use async_trait::async_trait;
use tokio::sync::watch;

use crate::{
    AgentEventSink, AgentExecutionError, AgentExecutionEvent, AgentExecutionRequest,
    AgentExecutionResult, AgentExecutor, AgentProviderConfig, AgentRunContext,
    aionrs_tools::build_aionrs_tool_registry,
};
use nineprofs_tools::{ToolInvocationContext, ToolInvocationScope, ToolRegistry};

pub const NINEPROFS_DEFAULT_BACKEND_ID: &str = "nineprofs-default";

pub struct AionRsExecutor {
    provider: AgentProviderConfig,
    tools: ToolRegistry,
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
        Self { provider, tools }
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

    fn emit_tool_call(&self, _tool_use_id: &str, _name: &str, _input: &str) {}

    fn emit_tool_result(&self, _tool_use_id: &str, _name: &str, _is_error: bool, _content: &str) {}

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
        config.system_prompt = Some(request.system_instructions);
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
        let mut engine = AgentEngine::new(config, aionrs_tools, output, workspace);
        let run = engine.run(&request.input, request.task_id.as_str());
        let result = tokio::select! {
            result = run => result.map_err(|_| AgentExecutionError::Failed)?,
            changed = wait_for_cancellation(&mut cancellation) => {
                changed?;
                engine.abort_current_turn("cancelled by 9Profs");
                return Err(AgentExecutionError::Cancelled);
            }
        };
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
            }
        }
        assert!(saw_started && saw_delta && saw_completed);
    }
}
