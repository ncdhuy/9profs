use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{AgentTaskId, RunId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentProviderConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key_env: String,
}

impl AgentProviderConfig {
    pub fn from_env() -> Self {
        let provider = std::env::var("NINEPROFS_AGENT_PROVIDER")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "openai".to_owned());
        let default_key_env = match provider.as_str() {
            "anthropic" => "ANTHROPIC_API_KEY",
            _ => "OPENAI_API_KEY",
        };
        Self {
            provider,
            model: std::env::var("NINEPROFS_AGENT_MODEL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "gpt-4o-mini".to_owned()),
            base_url: std::env::var("NINEPROFS_AGENT_BASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            api_key_env: std::env::var("NINEPROFS_AGENT_API_KEY_ENV")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| default_key_env.to_owned()),
        }
    }

    pub fn configuration_reason(&self) -> Option<String> {
        if !matches!(self.provider.as_str(), "openai" | "anthropic") {
            return Some(format!("unsupported provider `{}`", self.provider));
        }
        if self.model.trim().is_empty() {
            return Some("model is not configured".to_owned());
        }
        if self
            .base_url
            .as_deref()
            .is_some_and(|url| !(url.starts_with("http://") || url.starts_with("https://")))
        {
            return Some("provider base URL is invalid".to_owned());
        }
        match std::env::var(&self.api_key_env) {
            Ok(value) if !value.trim().is_empty() => None,
            _ => Some(format!("missing provider secret in {}", self.api_key_env)),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionLimits {
    pub max_output_tokens: Option<u32>,
    pub max_turns: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExecutionRequest {
    pub run_id: RunId,
    pub task_id: AgentTaskId,
    pub backend_id: String,
    pub assistant_id: String,
    pub input: String,
    pub workspace_root: Option<PathBuf>,
    pub provider: AgentProviderConfig,
    pub system_instructions: String,
    pub limits: ExecutionLimits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentExecutionEvent {
    OutputStarted,
    OutputDelta { delta: String },
    OutputCompleted { output: String },
    Error { code: String, message: String },
}

pub type AgentEventSink = mpsc::UnboundedSender<AgentExecutionEvent>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentExecutionResult {
    pub output: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AgentExecutionError {
    #[error("agent execution configuration is unavailable: {0}")]
    Configuration(String),
    #[error("agent execution backend is unavailable: {0}")]
    Unavailable(String),
    #[error("agent execution was cancelled")]
    Cancelled,
    #[error("agent execution failed")]
    Failed,
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    fn backend_id(&self) -> &str;

    async fn execute(
        &self,
        request: AgentExecutionRequest,
        event_sink: AgentEventSink,
        cancellation: tokio::sync::watch::Receiver<bool>,
    ) -> Result<AgentExecutionResult, AgentExecutionError>;
}

#[derive(Clone, Default)]
pub struct AgentExecutorRegistry {
    executors: Arc<BTreeMap<String, Arc<dyn AgentExecutor>>>,
}

impl AgentExecutorRegistry {
    pub fn new(executors: impl IntoIterator<Item = Arc<dyn AgentExecutor>>) -> Self {
        let executors = executors
            .into_iter()
            .map(|executor| (executor.backend_id().to_owned(), executor))
            .collect();
        Self {
            executors: Arc::new(executors),
        }
    }

    pub fn get(&self, backend_id: &str) -> Option<Arc<dyn AgentExecutor>> {
        self.executors.get(backend_id).cloned()
    }

    pub fn ids(&self) -> Vec<String> {
        self.executors.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_config_never_contains_secret_value() {
        let config = AgentProviderConfig {
            provider: "openai".to_owned(),
            model: "test-model".to_owned(),
            base_url: None,
            api_key_env: "OPENAI_API_KEY".to_owned(),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("sk-"));
        assert_eq!(config.api_key_env, "OPENAI_API_KEY");
    }

    #[test]
    fn provider_config_rejects_invalid_base_url_before_secret_lookup() {
        let config = AgentProviderConfig {
            provider: "openai".to_owned(),
            model: "test-model".to_owned(),
            base_url: Some("not-a-url".to_owned()),
            api_key_env: "NINEPROFS_TEST_MISSING_KEY".to_owned(),
        };
        assert_eq!(
            config.configuration_reason().as_deref(),
            Some("provider base URL is invalid")
        );
    }
}
