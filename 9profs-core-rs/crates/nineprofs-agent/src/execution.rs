use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{AgentTaskId, RunId};
use nineprofs_tools::ToolSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentProviderConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key_env: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AgentProviderConfigError {
    #[error("provider is not configured")]
    MissingProvider,
    #[error("unsupported provider `{0}`")]
    UnsupportedProvider(String),
    #[error("model is not configured")]
    MissingModel,
    #[error("provider credential environment variable is not configured")]
    MissingCredentialEnvironment,
    #[error("provider credential is not configured")]
    MissingCredential,
    #[error("provider base URL is invalid")]
    InvalidBaseUrl,
}

impl AgentProviderConfig {
    pub fn from_env() -> Self {
        Self::from_values(
            std::env::var("NINEPROFS_AGENT_PROVIDER").ok(),
            std::env::var("NINEPROFS_AGENT_MODEL").ok(),
            std::env::var("NINEPROFS_AGENT_BASE_URL").ok(),
            std::env::var("NINEPROFS_AGENT_API_KEY_ENV").ok(),
        )
    }

    fn from_values(
        provider: Option<String>,
        model: Option<String>,
        base_url: Option<String>,
        api_key_env: Option<String>,
    ) -> Self {
        let provider = provider.unwrap_or_default();
        let default_key_env = match provider.as_str() {
            "anthropic" => "ANTHROPIC_API_KEY",
            _ => "OPENAI_API_KEY",
        };
        Self {
            provider,
            model: model.unwrap_or_default(),
            base_url: base_url.filter(|value| !value.trim().is_empty()),
            api_key_env: api_key_env
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| default_key_env.to_owned()),
        }
    }

    pub fn validate(&self, credential: Option<&str>) -> Result<(), AgentProviderConfigError> {
        if self.provider.trim().is_empty() {
            return Err(AgentProviderConfigError::MissingProvider);
        }
        if !matches!(self.provider.as_str(), "openai" | "anthropic") {
            return Err(AgentProviderConfigError::UnsupportedProvider(
                self.provider.clone(),
            ));
        }
        if self.model.trim().is_empty() {
            return Err(AgentProviderConfigError::MissingModel);
        }
        if self.api_key_env.trim().is_empty() {
            return Err(AgentProviderConfigError::MissingCredentialEnvironment);
        }
        if self
            .base_url
            .as_deref()
            .is_some_and(|url| !is_valid_base_url(url))
        {
            return Err(AgentProviderConfigError::InvalidBaseUrl);
        }
        if credential.is_none_or(|value| value.trim().is_empty()) {
            return Err(AgentProviderConfigError::MissingCredential);
        }
        Ok(())
    }

    pub(crate) fn configured_secret(&self) -> Result<String, AgentProviderConfigError> {
        let credential = if self.api_key_env.trim().is_empty() {
            None
        } else {
            std::env::var(self.api_key_env.trim()).ok()
        };
        self.validate(credential.as_deref())?;
        credential.ok_or(AgentProviderConfigError::MissingCredential)
    }

    pub fn configuration_reason(&self) -> Option<String> {
        self.configured_secret()
            .err()
            .map(|error| error.to_string())
    }
}

fn is_valid_base_url(value: &str) -> bool {
    let Some((scheme, remainder)) = value.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https") || remainder.is_empty() {
        return false;
    }
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty()
        && !authority.starts_with(':')
        && !authority.ends_with(':')
        && !authority.chars().any(char::is_whitespace)
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
    pub tool_set: ToolSet,
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
    fn provider_config_requires_explicit_provider_and_model() {
        let missing_provider =
            AgentProviderConfig::from_values(None, Some("gpt-4o-mini".to_owned()), None, None);
        assert_eq!(
            missing_provider.validate(Some("secret")),
            Err(AgentProviderConfigError::MissingProvider)
        );

        let missing_model =
            AgentProviderConfig::from_values(Some("anthropic".to_owned()), None, None, None);
        assert_eq!(
            missing_model.validate(Some("secret")),
            Err(AgentProviderConfigError::MissingModel)
        );
        assert_eq!(
            missing_model.configuration_reason().as_deref(),
            Some("model is not configured")
        );
    }

    #[test]
    fn provider_config_validates_supported_credentials_and_endpoints() {
        let openai = AgentProviderConfig {
            provider: "openai".to_owned(),
            model: "gpt-4o-mini".to_owned(),
            base_url: Some("https://api.openai.example/v1".to_owned()),
            api_key_env: "OPENAI_API_KEY".to_owned(),
        };
        assert!(openai.validate(Some("secret")).is_ok());

        let anthropic = AgentProviderConfig {
            provider: "anthropic".to_owned(),
            model: "claude-sonnet-test".to_owned(),
            base_url: None,
            api_key_env: "ANTHROPIC_API_KEY".to_owned(),
        };
        assert!(anthropic.validate(Some("secret")).is_ok());

        let unsupported = AgentProviderConfig {
            provider: "gemini".to_owned(),
            ..openai.clone()
        };
        assert!(matches!(
            unsupported.validate(Some("secret")),
            Err(AgentProviderConfigError::UnsupportedProvider(provider)) if provider == "gemini"
        ));

        let missing_secret = openai.clone();
        assert_eq!(
            missing_secret.validate(None),
            Err(AgentProviderConfigError::MissingCredential)
        );

        let invalid_endpoint = AgentProviderConfig {
            base_url: Some("https://".to_owned()),
            ..openai
        };
        assert_eq!(
            invalid_endpoint.validate(Some("secret")),
            Err(AgentProviderConfigError::InvalidBaseUrl)
        );
    }

    #[test]
    fn provider_config_does_not_invent_a_model_for_anthropic() {
        let config =
            AgentProviderConfig::from_values(Some("anthropic".to_owned()), None, None, None);
        assert!(config.model.is_empty());
        assert_eq!(
            config.configuration_reason().as_deref(),
            Some("model is not configured")
        );
    }

    #[test]
    fn provider_config_never_contains_or_reports_secret_value() {
        let config = AgentProviderConfig {
            provider: "openai".to_owned(),
            model: "test-model".to_owned(),
            base_url: None,
            api_key_env: "OPENAI_API_KEY".to_owned(),
        };
        let secret = "credential-value";
        let debug = format!("{config:?}");
        let error = config.validate(None).unwrap_err().to_string();
        assert!(!debug.contains(secret));
        assert!(!error.contains(secret));
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
            config.validate(Some("secret")),
            Err(AgentProviderConfigError::InvalidBaseUrl)
        );
        assert_eq!(
            config.configuration_reason().as_deref(),
            Some("provider base URL is invalid")
        );
    }
}
