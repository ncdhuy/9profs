use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpError {
    #[error("MCP server not found: {0}")]
    NotFound(String),
    #[error("MCP server conflict: {0}")]
    Conflict(String),
    #[error("invalid MCP configuration: {0}")]
    Invalid(String),
    #[error("MCP database operation failed")]
    Database(String),
    #[error("MCP connection failed: {0}")]
    Connection(String),
    #[error("MCP connection timed out after {0}ms")]
    Timeout(u64),
    #[error("MCP tool registry update failed: {0}")]
    ToolRegistry(String),
}

impl From<sqlx::Error> for McpError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error.to_string())
    }
}
