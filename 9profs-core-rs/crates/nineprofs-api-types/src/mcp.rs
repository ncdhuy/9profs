use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpServerDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub startup_timeout_ms: u64,
    pub transport: McpTransportDto,
    pub status: String,
    pub last_connected: Option<i64>,
    pub error: Option<String>,
    pub supports_resources: bool,
    pub tools: Vec<McpToolDto>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum McpTransportDto {
    Stdio {
        command: String,
        args: Vec<String>,
        env_keys: Vec<String>,
    },
    Sse {
        url: String,
        header_names: Vec<String>,
    },
    StreamableHttp {
        url: String,
        header_names: Vec<String>,
    },
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum McpTransportInputDto {
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

impl fmt::Debug for McpTransportInputDto {
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

#[derive(Clone, Deserialize, Serialize)]
pub struct CreateMcpServerRequest {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    pub startup_timeout_ms: Option<u64>,
    pub transport: McpTransportInputDto,
}

impl fmt::Debug for CreateMcpServerRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CreateMcpServerRequest")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("description", &self.description)
            .field("enabled", &self.enabled)
            .field("startup_timeout_ms", &self.startup_timeout_ms)
            .field("transport", &self.transport)
            .finish()
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct UpdateMcpServerRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub startup_timeout_ms: Option<u64>,
    pub transport: Option<McpTransportInputDto>,
}

impl fmt::Debug for UpdateMcpServerRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UpdateMcpServerRequest")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("enabled", &self.enabled)
            .field("startup_timeout_ms", &self.startup_timeout_ms)
            .field("transport", &self.transport)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpToolDto {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct McpConnectionTestDto {
    pub success: bool,
    pub tool_count: usize,
    pub supports_resources: bool,
    pub error: Option<String>,
}
