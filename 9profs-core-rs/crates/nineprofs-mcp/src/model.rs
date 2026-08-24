use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::McpError;

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct McpServerId(String);

impl McpServerId {
    pub fn new(value: impl Into<String>) -> Result<Self, McpError> {
        let value = value.into();
        if value.trim().is_empty()
            || value.chars().any(|character| {
                character.is_control() || matches!(character, '/' | '\\' | ':' | ' ')
            })
        {
            return Err(McpError::Invalid(
                "server ID must be a stable path-safe value".to_owned(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for McpServerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("McpServerId").field(&self.0).finish()
    }
}

impl fmt::Display for McpServerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum McpTransportConfig {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    Sse {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
    StreamableHttp {
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

impl fmt::Debug for McpTransportConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdio { command, args, env } => formatter
                .debug_struct("Stdio")
                .field("command", command)
                .field("args", args)
                .field("env_keys", &env.keys().collect::<Vec<_>>())
                .finish(),
            Self::Sse { url, headers } => formatter
                .debug_struct("Sse")
                .field("url", url)
                .field("header_names", &headers.keys().collect::<Vec<_>>())
                .finish(),
            Self::StreamableHttp { url, headers } => formatter
                .debug_struct("StreamableHttp")
                .field("url", url)
                .field("header_names", &headers.keys().collect::<Vec<_>>())
                .finish(),
        }
    }
}

impl McpTransportConfig {
    pub fn validate(&self) -> Result<(), McpError> {
        match self {
            Self::Stdio { command, env, .. } => {
                if command.trim().is_empty() {
                    return Err(McpError::Invalid(
                        "stdio command must not be empty".to_owned(),
                    ));
                }
                if env.keys().any(|key| key.trim().is_empty()) {
                    return Err(McpError::Invalid(
                        "stdio environment names must not be empty".to_owned(),
                    ));
                }
            }
            Self::Sse { url, headers } | Self::StreamableHttp { url, headers } => {
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(McpError::Invalid(
                        "HTTP MCP URL must use http or https".to_owned(),
                    ));
                }
                if headers.keys().any(|key| key.trim().is_empty()) {
                    return Err(McpError::Invalid(
                        "HTTP header names must not be empty".to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn summary(&self) -> McpTransportSummary {
        match self {
            Self::Stdio { command, args, env } => McpTransportSummary::Stdio {
                command: command.clone(),
                args: args.clone(),
                env_keys: env.keys().cloned().collect(),
            },
            Self::Sse { url, headers } => McpTransportSummary::Sse {
                url: url.clone(),
                header_names: headers.keys().cloned().collect(),
            },
            Self::StreamableHttp { url, headers } => McpTransportSummary::StreamableHttp {
                url: url.clone(),
                header_names: headers.keys().cloned().collect(),
            },
        }
    }
}

#[derive(Clone)]
pub struct McpServerConfig {
    pub id: McpServerId,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub startup_timeout_ms: u64,
    pub transport: McpTransportConfig,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl fmt::Debug for McpServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("McpServerConfig")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("description", &self.description)
            .field("enabled", &self.enabled)
            .field("transport", &self.transport)
            .field("created_at_ms", &self.created_at_ms)
            .field("updated_at_ms", &self.updated_at_ms)
            .finish()
    }
}

impl McpServerConfig {
    pub fn validate(&self) -> Result<(), McpError> {
        if self.name.trim().is_empty() {
            return Err(McpError::Invalid(
                "server name must not be empty".to_owned(),
            ));
        }
        if !(1..=300_000).contains(&self.startup_timeout_ms) {
            return Err(McpError::Invalid(
                "startup timeout must be between 1ms and 300000ms".to_owned(),
            ));
        }
        self.transport.validate()
    }
}

#[derive(Clone, Debug)]
pub struct CreateMcpServer {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub startup_timeout_ms: Option<u64>,
    pub transport: McpTransportConfig,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateMcpServer {
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub startup_timeout_ms: Option<u64>,
    pub transport: Option<McpTransportConfig>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpServerStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpToolMetadata {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip)]
    pub external_network: bool,
}

#[derive(Clone, Debug)]
pub struct McpRuntimeState {
    pub status: McpServerStatus,
    pub last_connected: Option<i64>,
    pub error: Option<String>,
    pub supports_resources: bool,
    pub tools: Vec<McpToolMetadata>,
}

impl Default for McpRuntimeState {
    fn default() -> Self {
        Self {
            status: McpServerStatus::Disconnected,
            last_connected: None,
            error: None,
            supports_resources: false,
            tools: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub enum McpTransportSummary {
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        args: Vec<String>,
        env_keys: Vec<String>,
    },
    #[serde(rename = "sse")]
    Sse {
        url: String,
        header_names: Vec<String>,
    },
    #[serde(rename = "streamable-http")]
    StreamableHttp {
        url: String,
        header_names: Vec<String>,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct McpServerSnapshot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub startup_timeout_ms: u64,
    pub transport: McpTransportSummary,
    pub status: McpServerStatus,
    pub last_connected: Option<i64>,
    pub error: Option<String>,
    pub supports_resources: bool,
    pub tools: Vec<McpToolMetadata>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct McpConnectionTestResult {
    pub success: bool,
    pub tool_count: usize,
    pub supports_resources: bool,
    pub error: Option<String>,
}
