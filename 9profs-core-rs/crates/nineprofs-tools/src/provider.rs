use std::sync::Arc;

use async_trait::async_trait;

use crate::{ToolDefinition, ToolError, ToolInvocation, ToolResult};

/// Async execution boundary owned by 9Profs.
#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn execute(&self, invocation: ToolInvocation) -> Result<ToolResult, ToolError>;
}

/// Shared executable handler stored by the registry.
pub type ToolExecutor = Arc<dyn ToolHandler>;

#[derive(Clone)]
pub struct ToolRegistration {
    pub definition: ToolDefinition,
    pub handler: ToolExecutor,
}

/// Contribution boundary for future builtin, MCP, OfficeCLI, research, and
/// extension providers.
#[async_trait]
pub trait ToolProvider: Send + Sync {
    async fn list_tools(&self) -> Result<Vec<ToolRegistration>, ToolError>;
}
