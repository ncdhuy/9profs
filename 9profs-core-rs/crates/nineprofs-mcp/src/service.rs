use std::sync::Arc;

use nineprofs_realtime::BroadcastEventBus;
use nineprofs_tools::{ToolRegistry, ToolSource};
use uuid::Uuid;

use crate::{
    connection::{McpConnectionManager, McpConnectionTest},
    error::McpError,
    model::{
        CreateMcpServer, McpConnectionTestResult, McpServerConfig, McpServerId, McpServerSnapshot,
        McpServerStatus, UpdateMcpServer,
    },
    provider::McpToolProvider,
    repository::SqliteMcpServerRepository,
};

#[derive(Clone)]
pub struct McpService {
    repository: SqliteMcpServerRepository,
    connections: Arc<McpConnectionManager>,
    registry: ToolRegistry,
    events: Arc<BroadcastEventBus>,
}

impl McpService {
    pub fn new(
        repository: SqliteMcpServerRepository,
        registry: ToolRegistry,
        events: Arc<BroadcastEventBus>,
    ) -> Self {
        Self {
            repository,
            connections: Arc::new(McpConnectionManager::default()),
            registry,
            events,
        }
    }

    pub fn connection_manager(&self) -> Arc<McpConnectionManager> {
        Arc::clone(&self.connections)
    }

    pub async fn list(&self) -> Result<Vec<McpServerSnapshot>, McpError> {
        let configs = self.repository.list().await?;
        let mut snapshots = Vec::with_capacity(configs.len());
        for config in configs {
            snapshots.push(self.snapshot(config).await);
        }
        Ok(snapshots)
    }

    pub async fn get(&self, id: &str) -> Result<McpServerSnapshot, McpError> {
        let id = McpServerId::new(id.to_owned())?;
        Ok(self.snapshot(self.repository.get(&id).await?).await)
    }

    pub async fn create(&self, input: CreateMcpServer) -> Result<McpServerSnapshot, McpError> {
        let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let config = McpServerConfig {
            id: McpServerId::new(id)?,
            name: input.name,
            description: input.description,
            enabled: input.enabled,
            startup_timeout_ms: input
                .startup_timeout_ms
                .unwrap_or(crate::DEFAULT_STARTUP_TIMEOUT_MS),
            transport: input.transport,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        Ok(self.snapshot(self.repository.create(config).await?).await)
    }

    pub async fn update(
        &self,
        id: &str,
        input: UpdateMcpServer,
    ) -> Result<McpServerSnapshot, McpError> {
        let id = McpServerId::new(id.to_owned())?;
        let mut config = self.repository.get(&id).await?;
        let was_connected = self.connections.state(&id).await.status == McpServerStatus::Connected;
        if let Some(name) = input.name {
            config.name = name;
        }
        if let Some(description) = input.description {
            config.description = description;
        }
        if let Some(enabled) = input.enabled {
            config.enabled = enabled;
        }
        if let Some(timeout) = input.startup_timeout_ms {
            config.startup_timeout_ms = timeout;
        }
        if let Some(transport) = input.transport {
            config.transport = transport;
        }
        self.repository.update(config).await?;
        if was_connected {
            self.connections.disconnect(&id).await;
            self.refresh_registry().await?;
            if self.repository.get(&id).await?.enabled {
                let _ = self.connect(id.as_str()).await;
            }
        } else if !self.repository.get(&id).await?.enabled {
            self.connections.disconnect(&id).await;
            self.refresh_registry().await?;
        }
        self.get(id.as_str()).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), McpError> {
        let id = McpServerId::new(id.to_owned())?;
        self.repository.get(&id).await?;
        self.connections.disconnect(&id).await;
        self.repository.delete(&id).await?;
        self.refresh_registry().await
    }

    pub async fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<McpServerSnapshot, McpError> {
        let id = McpServerId::new(id.to_owned())?;
        let mut config = self.repository.get(&id).await?;
        config.enabled = enabled;
        self.repository.update(config).await?;
        if !enabled {
            self.connections.disconnect(&id).await;
            self.refresh_registry().await?;
            self.publish(
                "mcp.serverDisconnected",
                &id,
                serde_json::json!({ "reason": "disabled" }),
            );
        }
        self.get(id.as_str()).await
    }

    pub async fn connect(&self, id: &str) -> Result<McpServerSnapshot, McpError> {
        let id = McpServerId::new(id.to_owned())?;
        let config = self.repository.get(&id).await?;
        match self.connections.connect(&config).await {
            Ok(state) => {
                self.refresh_registry().await?;
                self.publish(
                    "mcp.serverConnected",
                    &id,
                    serde_json::json!({ "tool_count": state.tools.len(), "supports_resources": state.supports_resources }),
                );
                self.publish_tools_changed(&id, state.tools.len());
                self.get(id.as_str()).await
            }
            Err(error) => {
                self.refresh_registry().await?;
                self.publish(
                    "mcp.serverError",
                    &id,
                    serde_json::json!({ "error": error.to_string() }),
                );
                Err(error)
            }
        }
    }

    pub async fn disconnect(&self, id: &str) -> Result<McpServerSnapshot, McpError> {
        let id = McpServerId::new(id.to_owned())?;
        self.repository.get(&id).await?;
        self.connections.disconnect(&id).await;
        self.refresh_registry().await?;
        self.publish("mcp.serverDisconnected", &id, serde_json::json!({}));
        self.publish_tools_changed(&id, 0);
        self.get(id.as_str()).await
    }

    pub async fn test(&self, id: &str) -> Result<McpConnectionTestResult, McpError> {
        let id = McpServerId::new(id.to_owned())?;
        let result = self
            .connections
            .test(&self.repository.get(&id).await?)
            .await;
        Ok(McpConnectionTestResult {
            success: result.success,
            tool_count: result.tool_count,
            supports_resources: result.supports_resources,
            error: result.error,
        })
    }

    pub async fn tools(&self, id: &str) -> Result<Vec<crate::model::McpToolMetadata>, McpError> {
        Ok(self.get(id).await?.tools)
    }

    async fn snapshot(&self, config: McpServerConfig) -> McpServerSnapshot {
        let state = self.connections.state(&config.id).await;
        McpServerSnapshot {
            id: config.id.to_string(),
            name: config.name,
            description: config.description,
            enabled: config.enabled,
            startup_timeout_ms: config.startup_timeout_ms,
            transport: config.transport.summary(),
            status: state.status,
            last_connected: state.last_connected,
            error: state.error,
            supports_resources: state.supports_resources,
            tools: state.tools,
            created_at_ms: config.created_at_ms,
            updated_at_ms: config.updated_at_ms,
        }
    }

    async fn refresh_registry(&self) -> Result<(), McpError> {
        let provider =
            McpToolProvider::with_registry(Arc::clone(&self.connections), self.registry.clone());
        let registrations = nineprofs_tools::ToolProvider::list_tools(&provider)
            .await
            .map_err(|error| McpError::ToolRegistry(error.to_string()))?;
        self.registry
            .replace_source(ToolSource::Mcp, registrations)
            .map(|_| ())
            .map_err(|error| McpError::ToolRegistry(error.to_string()))
    }

    fn publish(&self, name: &str, id: &McpServerId, details: serde_json::Value) {
        let _ = self.events.publish(nineprofs_api_types::EventEnvelope::new(
            name,
            serde_json::json!({ "server_id": id.to_string(), "details": details }),
        ));
    }

    fn publish_tools_changed(&self, id: &McpServerId, tool_count: usize) {
        self.publish(
            "mcp.toolsChanged",
            id,
            serde_json::json!({ "tool_count": tool_count }),
        );
    }
}

impl From<McpConnectionTest> for McpConnectionTestResult {
    fn from(result: McpConnectionTest) -> Self {
        Self {
            success: result.success,
            tool_count: result.tool_count,
            supports_resources: result.supports_resources,
            error: result.error,
        }
    }
}
