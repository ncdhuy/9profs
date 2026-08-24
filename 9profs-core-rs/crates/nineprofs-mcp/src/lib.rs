//! 9Profs-owned MCP configuration, connection, discovery, and tool-provider boundary.
//!
//! AionRS is used only for MCP protocol and transport mechanics. Discovered
//! tools are converted into normal 9Profs registrations and enter the shared
//! [`nineprofs_tools::ToolRegistry`] before any agent can see them.

mod connection;
mod error;
mod model;
mod provider;
mod repository;
mod service;

pub use connection::{DEFAULT_STARTUP_TIMEOUT_MS, McpConnectionManager};
pub use error::McpError;
pub use model::{
    CreateMcpServer, McpConnectionTestResult, McpRuntimeState, McpServerConfig, McpServerId,
    McpServerSnapshot, McpServerStatus, McpToolMetadata, McpTransportConfig, McpTransportSummary,
    UpdateMcpServer,
};
pub use provider::{McpToolProvider, display_tool_name, stable_tool_id};
pub use repository::SqliteMcpServerRepository;
pub use service::McpService;
