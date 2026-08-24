use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::{Duration, Instant},
};

use aion_mcp::{
    config::{McpServerConfig as AionMcpServerConfig, TransportType},
    manager::McpManager,
};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};

use crate::{
    error::McpError,
    model::{
        McpRuntimeState, McpServerConfig, McpServerId, McpServerStatus, McpToolMetadata,
        McpTransportConfig,
    },
    provider::{display_tool_name, stable_tool_id},
};

pub const DEFAULT_STARTUP_TIMEOUT_MS: u64 = 30_000;

#[derive(Clone, Default)]
pub struct McpConnectionManager {
    connections: Arc<RwLock<BTreeMap<McpServerId, ConnectedMcpServer>>>,
    states: Arc<RwLock<BTreeMap<McpServerId, McpRuntimeState>>>,
    connect_lock: Arc<Mutex<()>>,
}

#[derive(Clone)]
struct ConnectedMcpServer {
    manager: Arc<McpManager>,
    secrets: Vec<String>,
}

impl McpConnectionManager {
    pub async fn connect(&self, config: &McpServerConfig) -> Result<McpRuntimeState, McpError> {
        if !config.enabled {
            return Err(McpError::Invalid(
                "disabled MCP server cannot connect".to_owned(),
            ));
        }
        let _guard = self.connect_lock.lock().await;
        self.connections.write().await.remove(&config.id);
        self.states.write().await.insert(
            config.id.clone(),
            McpRuntimeState {
                status: McpServerStatus::Connecting,
                ..McpRuntimeState::default()
            },
        );
        match connect_one(config).await {
            Ok((manager, tools, supports_resources)) => {
                let state = McpRuntimeState {
                    status: McpServerStatus::Connected,
                    last_connected: Some(nineprofs_common::now_ms()),
                    error: None,
                    supports_resources,
                    tools,
                };
                self.connections.write().await.insert(
                    config.id.clone(),
                    ConnectedMcpServer {
                        manager,
                        secrets: config_secrets(config)
                            .into_iter()
                            .map(str::to_owned)
                            .collect(),
                    },
                );
                self.states
                    .write()
                    .await
                    .insert(config.id.clone(), state.clone());
                Ok(state)
            }
            Err(error) => {
                self.states.write().await.insert(
                    config.id.clone(),
                    McpRuntimeState {
                        status: McpServerStatus::Error,
                        error: Some(error.to_string()),
                        ..McpRuntimeState::default()
                    },
                );
                Err(error)
            }
        }
    }

    pub async fn disconnect(&self, id: &McpServerId) {
        self.connections.write().await.remove(id);
        self.states.write().await.insert(
            id.clone(),
            McpRuntimeState {
                status: McpServerStatus::Disconnected,
                ..McpRuntimeState::default()
            },
        );
    }

    pub async fn test(&self, config: &McpServerConfig) -> McpConnectionTest {
        match connect_one(config).await {
            Ok((_, tools, supports_resources)) => McpConnectionTest {
                success: true,
                tool_count: tools.len(),
                supports_resources,
                error: None,
            },
            Err(error) => McpConnectionTest {
                success: false,
                tool_count: 0,
                supports_resources: false,
                error: Some(error.to_string()),
            },
        }
    }

    pub async fn state(&self, id: &McpServerId) -> McpRuntimeState {
        self.states
            .read()
            .await
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn connected_tools(&self) -> Vec<(McpServerId, McpToolMetadata)> {
        self.states
            .read()
            .await
            .iter()
            .filter(|(_, state)| state.status == McpServerStatus::Connected)
            .flat_map(|(id, state)| {
                state
                    .tools
                    .iter()
                    .cloned()
                    .map(|tool| (id.clone(), tool))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub async fn call_tool(
        &self,
        server_id: &McpServerId,
        tool_name: &str,
        arguments: Value,
    ) -> Result<String, McpError> {
        let connection = self
            .connections
            .read()
            .await
            .get(server_id)
            .cloned()
            .ok_or_else(|| {
                McpError::Connection(format!("server `{server_id}` is not connected"))
            })?;
        match connection
            .manager
            .call_tool(server_id.as_str(), tool_name, arguments)
            .await
        {
            Ok(output) => Ok(output),
            Err(error) => {
                let secrets = connection
                    .secrets
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                self.connections.write().await.remove(server_id);
                self.states.write().await.insert(
                    server_id.clone(),
                    McpRuntimeState {
                        status: McpServerStatus::Error,
                        error: Some(redact_message(&error.to_string(), &secrets)),
                        ..McpRuntimeState::default()
                    },
                );
                Err(McpError::Connection(redact_message(
                    &error.to_string(),
                    &secrets,
                )))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct McpConnectionTest {
    pub success: bool,
    pub tool_count: usize,
    pub supports_resources: bool,
    pub error: Option<String>,
}

async fn connect_one(
    config: &McpServerConfig,
) -> Result<(Arc<McpManager>, Vec<McpToolMetadata>, bool), McpError> {
    let timeout_ms = config.startup_timeout_ms;
    let aion_config = to_aion_config(config)?;
    let mut configs = HashMap::new();
    configs.insert(config.id.as_str().to_owned(), aion_config);
    let started_at = Instant::now();
    let manager = tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        McpManager::connect_all(&configs),
    )
    .await
    .map_err(|_| McpError::Timeout(timeout_ms))?
    .map_err(|error| {
        McpError::Connection(redact_message(&error.to_string(), &config_secrets(config)))
    })?;
    let manager = Arc::new(manager);
    // AionRS connect_all intentionally isolates per-server failures and can
    // return an empty manager. Probe a reserved impossible tool to distinguish
    // that case from a healthy server that advertises zero tools.
    if let Err(error) = manager
        .call_tool(
            config.id.as_str(),
            "__9profs_connection_probe__",
            Value::Object(Default::default()),
        )
        .await
    {
        let message = error.to_string();
        if message.to_ascii_lowercase().contains("server not found") {
            if started_at.elapsed() >= Duration::from_millis(timeout_ms) {
                return Err(McpError::Timeout(timeout_ms));
            }
            return Err(McpError::Connection(redact_message(
                "configured MCP server did not connect",
                &config_secrets(config),
            )));
        }
    }
    let mut tools = manager
        .all_tools()
        .into_iter()
        .filter(|(server, _)| *server == config.id.as_str())
        .map(|(_, tool)| McpToolMetadata {
            id: stable_tool_id(&config.id, &tool.name),
            name: tool.name.clone(),
            display_name: display_tool_name(&config.id, &tool.name),
            description: tool.description.clone().unwrap_or_default(),
            input_schema: tool.input_schema.clone(),
            external_network: matches!(
                &config.transport,
                McpTransportConfig::Sse { .. } | McpTransportConfig::StreamableHttp { .. }
            ),
        })
        .collect::<Vec<_>>();
    tools.sort_by(|left, right| left.name.cmp(&right.name));
    let supports_resources = manager.server_supports_resources(config.id.as_str());
    Ok((manager, tools, supports_resources))
}

fn to_aion_config(config: &McpServerConfig) -> Result<AionMcpServerConfig, McpError> {
    let (transport, command, args, env, url, headers) = match &config.transport {
        McpTransportConfig::Stdio { command, args, env } => (
            TransportType::Stdio,
            Some(command.clone()),
            Some(args.clone()),
            Some(env.clone().into_iter().collect()),
            None,
            None,
        ),
        McpTransportConfig::Sse { url, headers } => (
            TransportType::Sse,
            None,
            None,
            None,
            Some(url.clone()),
            Some(headers.clone().into_iter().collect()),
        ),
        McpTransportConfig::StreamableHttp { url, headers } => (
            TransportType::StreamableHttp,
            None,
            None,
            None,
            Some(url.clone()),
            Some(headers.clone().into_iter().collect()),
        ),
    };
    Ok(AionMcpServerConfig {
        transport,
        command,
        args,
        env,
        url,
        headers,
        deferred: Some(false),
        startup_timeout_ms: Some(transport_timeout(config)),
    })
}

fn transport_timeout(config: &McpServerConfig) -> u64 {
    config.startup_timeout_ms.max(1)
}

fn config_secrets(config: &McpServerConfig) -> Vec<&str> {
    match &config.transport {
        McpTransportConfig::Stdio { env, .. } => env.values().map(String::as_str).collect(),
        McpTransportConfig::Sse { headers, .. }
        | McpTransportConfig::StreamableHttp { headers, .. } => {
            headers.values().map(String::as_str).collect()
        }
    }
}

fn redact_message(message: &str, secrets: &[&str]) -> String {
    secrets.iter().fold(message.to_owned(), |message, secret| {
        if secret.is_empty() {
            message
        } else {
            message.replace(secret, "[REDACTED]")
        }
    })
}
