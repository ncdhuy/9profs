use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use nineprofs_tools::{
    ToolDefinition, ToolEffect, ToolError, ToolHandler, ToolInvocation, ToolPolicy, ToolProvider,
    ToolRegistration, ToolResult, ToolSource,
};
use serde_json::Value;

use crate::{
    connection::McpConnectionManager,
    model::{McpServerId, McpToolMetadata},
};

#[derive(Clone)]
pub struct McpToolProvider {
    connections: Arc<McpConnectionManager>,
    registry: Option<nineprofs_tools::ToolRegistry>,
}

impl McpToolProvider {
    pub fn new(connections: Arc<McpConnectionManager>) -> Self {
        Self {
            connections,
            registry: None,
        }
    }

    pub(crate) fn with_registry(
        connections: Arc<McpConnectionManager>,
        registry: nineprofs_tools::ToolRegistry,
    ) -> Self {
        Self {
            connections,
            registry: Some(registry),
        }
    }
}

#[async_trait]
impl ToolProvider for McpToolProvider {
    async fn list_tools(&self) -> Result<Vec<ToolRegistration>, ToolError> {
        Ok(self
            .connections
            .connected_tools()
            .await
            .into_iter()
            .map(|(server_id, metadata)| {
                registration(
                    Arc::clone(&self.connections),
                    self.registry.clone(),
                    server_id,
                    metadata,
                )
            })
            .collect())
    }
}

fn registration(
    connections: Arc<McpConnectionManager>,
    registry: Option<nineprofs_tools::ToolRegistry>,
    server_id: McpServerId,
    mut metadata: McpToolMetadata,
) -> ToolRegistration {
    let id = stable_tool_id(&server_id, &metadata.name);
    let name = display_tool_name(&server_id, &metadata.name);
    metadata.id = id.clone();
    metadata.display_name = name.clone();
    ToolRegistration {
        definition: ToolDefinition {
            id: id.into(),
            name,
            description: metadata.description.clone(),
            input_schema: metadata.input_schema.clone(),
            source: ToolSource::Mcp,
            policy: mcp_policy(metadata.external_network),
            enabled: true,
        },
        handler: Arc::new(McpToolHandler {
            connections,
            registry,
            server_id,
            tool_name: metadata.name,
        }),
    }
}

fn mcp_policy(external_network: bool) -> ToolPolicy {
    let mut effects = BTreeSet::from([ToolEffect::Execute]);
    if external_network {
        effects.insert(ToolEffect::ExternalNetwork);
    }
    ToolPolicy {
        effects,
        requires_confirmation: true,
    }
}

struct McpToolHandler {
    connections: Arc<McpConnectionManager>,
    registry: Option<nineprofs_tools::ToolRegistry>,
    server_id: McpServerId,
    tool_name: String,
}

#[async_trait]
impl ToolHandler for McpToolHandler {
    async fn execute(&self, invocation: ToolInvocation) -> Result<ToolResult, ToolError> {
        let result = self
            .connections
            .call_tool(&self.server_id, &self.tool_name, invocation.arguments)
            .await
            .map(|output| ToolResult::new(Value::String(output)));
        if result.is_err()
            && let Some(registry) = &self.registry
        {
            let provider = McpToolProvider::new(Arc::clone(&self.connections));
            if let Ok(registrations) = provider.list_tools().await {
                let _ = registry.replace_source(ToolSource::Mcp, registrations);
            }
        }
        result.map_err(|error| ToolError::Handler(error.to_string()))
    }
}

pub fn stable_tool_id(server_id: &McpServerId, tool_name: &str) -> String {
    format!("mcp/{}/{}", server_id.as_str(), path_component(tool_name))
}

pub fn display_tool_name(server_id: &McpServerId, tool_name: &str) -> String {
    format!(
        "mcp_{}_{}",
        hex_component(server_id.as_str()),
        hex_component(tool_name)
    )
}

fn hex_component(value: &str) -> String {
    value.bytes().map(|byte| format!("{byte:02x}")).collect()
}

fn path_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                format!("{}", byte as char).into_bytes()
            } else {
                format!("%{byte:02X}").into_bytes()
            }
        })
        .map(char::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use nineprofs_tools::ToolEffect;

    use super::mcp_policy;

    #[test]
    fn mcp_policy_is_conservative_and_marks_remote_networks() {
        let local = mcp_policy(false);
        assert!(local.requires_confirmation);
        assert!(local.effects.contains(&ToolEffect::Execute));
        assert!(!local.effects.contains(&ToolEffect::ExternalNetwork));

        let remote = mcp_policy(true);
        assert!(remote.effects.contains(&ToolEffect::ExternalNetwork));
    }
}
