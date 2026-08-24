//! 9Profs-owned tool metadata, authorization, and execution runtime.
//!
//! This crate deliberately has no transport, process, filesystem, MCP, or
//! vendor-specific behavior. Adapters translate these contracts into an
//! execution engine's native tool types.

mod events;
mod model;
mod provider;
mod registry;

pub use events::ToolEvent;
pub use model::{
    ToolDefinition, ToolEffect, ToolError, ToolId, ToolInvocation, ToolInvocationContext,
    ToolPolicy, ToolResult, ToolSet, ToolSource,
};
pub use provider::{ToolExecutor, ToolHandler, ToolProvider, ToolRegistration};
pub use registry::ToolRegistry;
