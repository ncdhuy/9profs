//! Assistant catalog, rules, persistence, and skill assignment.
//!
//! This crate intentionally stops at metadata and content management. Agent
//! execution, backend probing, MCP, and conversation state belong to later
//! phases.

mod builtin;
mod model;
mod repository;
mod service;

pub use builtin::{BuiltinAssistantCatalog, BuiltinAssistantError};
pub use model::{AgentBackendId, Assistant, AssistantSource, CreateAssistant, UpdateAssistant};
pub use repository::{AssistantRepository, SqliteAssistantRepository};
pub use service::{AssistantError, AssistantService};
