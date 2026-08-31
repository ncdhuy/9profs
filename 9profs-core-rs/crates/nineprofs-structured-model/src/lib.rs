//! Bounded, launch-scoped HTTP transport for structured model adapters.
//!
//! Semantic adapters own prompts, request schemas, response parsing, and
//! domain error mapping. This crate owns provider configuration and wire
//! transport only.

use std::{fmt, time::Duration};

use reqwest::{Client, StatusCode, header::CONTENT_TYPE};
use serde_json::Value;
use thiserror::Error;

pub const ANTHROPIC_VERSION: &str = "2023-06-01";

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_RESPONSE_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 1_024;
const DEFAULT_API_KEY_ENV: &str = "OPENAI_API_KEY";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructuredModelProvider {
    OpenAi,
    Anthropic,
}

impl StructuredModelProvider {
    pub fn parse(value: &str) -> Result<Self, StructuredModelConfigError> {
        match value.trim() {
            "openai" => Ok(Self::OpenAi),
            "anthropic" => Ok(Self::Anthropic),
            "" => Err(StructuredModelConfigError::MissingProvider),
            _ => Err(StructuredModelConfigError::UnsupportedProvider),
        }
    }

    fn default_base_url(self) -> &'static str {
        match self {
            Self::OpenAi => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com/v1",
        }
    }

    fn endpoint_path(self) -> &'static str {
        match self {
            Self::OpenAi => "chat/completions",
            Self::Anthropic => "messages",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct StructuredModelConfig {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    /// Environment variable name only. The credential is resolved per call.
    pub api_key_env: String,
    pub timeout: Duration,
    pub max_response_bytes: usize,
    pub max_output_tokens: u32,
}

impl fmt::Debug for StructuredModelConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StructuredModelConfig")
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

impl StructuredModelConfig {
    pub fn from_env() -> Self {
        let provider = std::env::var("NINEPROFS_MODEL_PROVIDER")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let model = std::env::var("NINEPROFS_MODEL_MODEL")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let base_url = std::env::var("NINEPROFS_MODEL_BASE_URL")
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_owned())
            .filter(|value| !value.is_empty());
        let api_key_env = std::env::var("NINEPROFS_MODEL_API_KEY_ENV")
            .map(|value| value.trim().to_owned())
            .unwrap_or_else(|_| DEFAULT_API_KEY_ENV.to_owned());
        let timeout = std::env::var("NINEPROFS_MODEL_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(|milliseconds| Duration::from_millis(milliseconds.clamp(100, 120_000)))
            .unwrap_or(DEFAULT_TIMEOUT);
        Self {
            provider,
            model,
            base_url,
            api_key_env,
            timeout,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS,
        }
    }

    pub fn credential(&self) -> Option<String> {
        (!self.api_key_env.trim().is_empty())
            .then(|| std::env::var(self.api_key_env.trim()).ok())
            .flatten()
            .filter(|value| !value.trim().is_empty())
    }

    pub fn provider(&self) -> Result<StructuredModelProvider, StructuredModelConfigError> {
        StructuredModelProvider::parse(&self.provider)
    }

    pub fn validate(
        &self,
        credential: Option<&str>,
    ) -> Result<StructuredModelProvider, StructuredModelConfigError> {
        let provider = self.provider()?;
        if self.model.trim().is_empty() {
            return Err(StructuredModelConfigError::MissingModel);
        }
        if self.api_key_env.trim().is_empty() {
            return Err(StructuredModelConfigError::MissingCredentialEnvironment);
        }
        if credential.is_none_or(|value| value.trim().is_empty()) {
            return Err(StructuredModelConfigError::MissingCredential);
        }
        if self
            .base_url
            .as_deref()
            .is_some_and(|value| !is_valid_base_url(value))
        {
            return Err(StructuredModelConfigError::InvalidBaseUrl);
        }
        if self.timeout.is_zero() || self.max_response_bytes == 0 || self.max_output_tokens == 0 {
            return Err(StructuredModelConfigError::InvalidLimits);
        }
        Ok(provider)
    }

    pub fn endpoint(&self) -> Result<String, StructuredModelConfigError> {
        let provider = self.provider()?;
        let base_url = self
            .base_url
            .as_deref()
            .unwrap_or(provider.default_base_url())
            .trim_end_matches('/');
        Ok(format!("{base_url}/{}", provider.endpoint_path()))
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum StructuredModelConfigError {
    #[error("provider is not configured")]
    MissingProvider,
    #[error("provider is unsupported")]
    UnsupportedProvider,
    #[error("model is not configured")]
    MissingModel,
    #[error("credential environment variable is not configured")]
    MissingCredentialEnvironment,
    #[error("credential is not configured")]
    MissingCredential,
    #[error("base URL is invalid")]
    InvalidBaseUrl,
    #[error("model limits are invalid")]
    InvalidLimits,
}

#[derive(Clone)]
pub struct StructuredModelClient {
    config: StructuredModelConfig,
    client: Option<Client>,
}

impl fmt::Debug for StructuredModelClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StructuredModelClient")
            .field("config", &self.config)
            .finish()
    }
}

impl StructuredModelClient {
    pub fn new(config: StructuredModelConfig) -> Self {
        let client = Client::builder().timeout(config.timeout).build().ok();
        Self { config, client }
    }

    pub async fn execute_json(
        &self,
        body: &Value,
        credential: &str,
    ) -> Result<Vec<u8>, StructuredModelTransportError> {
        if credential.trim().is_empty() {
            return Err(StructuredModelTransportError::NotConfigured);
        }
        let provider = self
            .config
            .provider()
            .map_err(|_| StructuredModelTransportError::InvalidConfiguration)?;
        let endpoint = self
            .config
            .endpoint()
            .map_err(|_| StructuredModelTransportError::InvalidConfiguration)?;
        let client = self
            .client
            .as_ref()
            .ok_or(StructuredModelTransportError::ClientBuildFailed)?;
        let request = client
            .post(endpoint)
            .header(CONTENT_TYPE, "application/json")
            .json(body);
        let request = match provider {
            StructuredModelProvider::OpenAi => request.bearer_auth(credential),
            StructuredModelProvider::Anthropic => request
                .header("x-api-key", credential)
                .header("anthropic-version", ANTHROPIC_VERSION),
        };
        let response = request.send().await.map_err(map_request_error)?;
        if response
            .content_length()
            .is_some_and(|length| length > self.config.max_response_bytes as u64)
        {
            return Err(StructuredModelTransportError::ResponseTooLarge);
        }
        let status = response.status();
        let bytes = read_bounded_response(response, self.config.max_response_bytes).await?;
        if !status.is_success() {
            return Err(normalize_status(status));
        }
        Ok(bytes)
    }
}

async fn read_bounded_response(
    mut response: reqwest::Response,
    max_response_bytes: usize,
) -> Result<Vec<u8>, StructuredModelTransportError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(map_request_error)? {
        if bytes.len().saturating_add(chunk.len()) > max_response_bytes {
            return Err(StructuredModelTransportError::ResponseTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn map_request_error(error: reqwest::Error) -> StructuredModelTransportError {
    if error.is_timeout() {
        StructuredModelTransportError::Timeout
    } else if error.is_connect() {
        StructuredModelTransportError::ProviderUnavailable
    } else {
        StructuredModelTransportError::Transport
    }
}

fn normalize_status(status: StatusCode) -> StructuredModelTransportError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            StructuredModelTransportError::Unauthorized
        }
        StatusCode::TOO_MANY_REQUESTS => StructuredModelTransportError::RateLimited,
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => {
            StructuredModelTransportError::Timeout
        }
        status if status.is_server_error() => StructuredModelTransportError::ProviderUnavailable,
        _ => StructuredModelTransportError::Transport,
    }
}

fn is_valid_base_url(value: &str) -> bool {
    let Ok(url) = value.parse::<reqwest::Url>() else {
        return false;
    };
    matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum StructuredModelTransportError {
    #[error("structured model provider is not configured")]
    NotConfigured,
    #[error("structured model provider configuration is invalid")]
    InvalidConfiguration,
    #[error("structured model HTTP client could not be built")]
    ClientBuildFailed,
    #[error("structured model request timed out")]
    Timeout,
    #[error("structured model provider authorization failed")]
    Unauthorized,
    #[error("structured model provider rate limit exceeded")]
    RateLimited,
    #[error("structured model provider is unavailable")]
    ProviderUnavailable,
    #[error("structured model response exceeded size limit")]
    ResponseTooLarge,
    #[error("structured model transport failed")]
    Transport,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Mutex, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn config(base_url: String) -> StructuredModelConfig {
        StructuredModelConfig {
            provider: "openai".to_owned(),
            model: "test-model".to_owned(),
            base_url: Some(base_url),
            api_key_env: "TEST_KEY".to_owned(),
            timeout: Duration::from_secs(1),
            max_response_bytes: 256,
            max_output_tokens: 32,
        }
    }

    async fn response_server(
        status: u16,
        body: &str,
        delay: Duration,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        let task = tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let Ok(count) = stream.read(&mut chunk).await else {
                    return;
                };
                if count == 0 {
                    return;
                }
                request.extend_from_slice(&chunk[..count]);
                let Some(header_end) = request.windows(4).position(|value| value == b"\r\n\r\n")
                else {
                    continue;
                };
                let content_length = String::from_utf8_lossy(&request[..header_end])
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length:")
                            .or_else(|| line.strip_prefix("content-length:"))
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or_default();
                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            let reason = if status == 200 { "OK" } else { "Error" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
        (format!("http://{address}/v1"), task)
    }

    #[test]
    fn config_debug_contains_reference_not_secret() {
        let debug = format!(
            "{:?}",
            StructuredModelConfig {
                provider: "openai".to_owned(),
                model: "model".to_owned(),
                base_url: None,
                api_key_env: "TEST_KEY".to_owned(),
                timeout: Duration::from_secs(1),
                max_response_bytes: 1,
                max_output_tokens: 1,
            }
        );
        assert!(debug.contains("TEST_KEY"));
        assert!(!debug.contains("secret-value"));
    }

    #[test]
    fn from_env_reads_shared_model_settings_and_named_credential() {
        let _guard = ENV_LOCK.lock().unwrap();
        let names = [
            "NINEPROFS_MODEL_PROVIDER",
            "NINEPROFS_MODEL_MODEL",
            "NINEPROFS_MODEL_BASE_URL",
            "NINEPROFS_MODEL_API_KEY_ENV",
            "NINEPROFS_MODEL_TIMEOUT_MS",
            "OPENAI_API_KEY",
            "OPENAI_KEY",
        ];
        let previous = names
            .iter()
            .map(|name| (*name, std::env::var(name).ok()))
            .collect::<Vec<_>>();
        unsafe {
            std::env::set_var("NINEPROFS_MODEL_PROVIDER", "openai");
            std::env::set_var("NINEPROFS_MODEL_MODEL", "shared-model");
            std::env::set_var("NINEPROFS_MODEL_BASE_URL", "https://example.test/v1/");
            std::env::set_var("NINEPROFS_MODEL_API_KEY_ENV", "OPENAI_API_KEY");
            std::env::set_var("NINEPROFS_MODEL_TIMEOUT_MS", "120000");
            std::env::set_var("OPENAI_API_KEY", "shared-secret");
            std::env::set_var("OPENAI_KEY", "stale-secret");
        }
        let config = StructuredModelConfig::from_env();
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "shared-model");
        assert_eq!(config.base_url.as_deref(), Some("https://example.test/v1"));
        assert_eq!(config.api_key_env, "OPENAI_API_KEY");
        assert_eq!(config.timeout, Duration::from_secs(120));
        assert_eq!(config.credential().as_deref(), Some("shared-secret"));
        assert!(!format!("{config:?}").contains("shared-secret"));

        unsafe {
            std::env::remove_var("NINEPROFS_MODEL_API_KEY_ENV");
            std::env::remove_var("OPENAI_API_KEY");
        }
        let without_shared_credential = StructuredModelConfig::from_env();
        assert_eq!(without_shared_credential.api_key_env, "OPENAI_API_KEY");
        assert!(without_shared_credential.credential().is_none());

        for (name, value) in previous {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(name, value),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    #[test]
    fn resolves_provider_default_endpoints() {
        let mut config = config("http://unused".to_owned());
        config.base_url = None;
        assert_eq!(
            config.endpoint().unwrap(),
            "https://api.openai.com/v1/chat/completions"
        );
        config.provider = "anthropic".to_owned();
        assert_eq!(
            config.endpoint().unwrap(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[tokio::test]
    async fn normalizes_http_statuses_without_body_leakage() {
        for (status, expected) in [
            (401, StructuredModelTransportError::Unauthorized),
            (403, StructuredModelTransportError::Unauthorized),
            (429, StructuredModelTransportError::RateLimited),
            (500, StructuredModelTransportError::ProviderUnavailable),
        ] {
            let (url, task) = response_server(status, "provider-secret", Duration::ZERO).await;
            let client = StructuredModelClient::new(config(url));
            let error = client
                .execute_json(&serde_json::json!({"model": "test-model"}), "secret-value")
                .await
                .unwrap_err();
            assert_eq!(error, expected);
            assert!(!error.to_string().contains("provider-secret"));
            assert!(!error.to_string().contains("secret-value"));
            task.await.unwrap();
        }
    }

    #[tokio::test]
    async fn bounds_response_before_returning_body() {
        let (url, task) = response_server(200, "123456789", Duration::ZERO).await;
        let mut model_config = config(url);
        model_config.max_response_bytes = 4;
        let error = StructuredModelClient::new(model_config)
            .execute_json(&serde_json::json!({}), "secret-value")
            .await
            .unwrap_err();
        assert_eq!(error, StructuredModelTransportError::ResponseTooLarge);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn maps_request_timeout() {
        let (url, task) = response_server(200, "{}", Duration::from_millis(100)).await;
        let mut model_config = config(url);
        model_config.timeout = Duration::from_millis(10);
        let error = StructuredModelClient::new(model_config)
            .execute_json(&serde_json::json!({}), "secret-value")
            .await
            .unwrap_err();
        assert_eq!(error, StructuredModelTransportError::Timeout);
        task.await.unwrap();
    }
}
