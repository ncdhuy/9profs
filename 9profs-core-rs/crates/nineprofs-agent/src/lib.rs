//! 9Profs-owned agent backend catalog and logical task lifecycle foundation.
//!
//! Phase 2A deliberately contains metadata, registry, identity, lifecycle, and
//! cancellation primitives only. It does not spawn processes, probe CLIs, use
//! AionRS/ACP, or execute real agents.

mod builtin;
mod model;
mod registry;
mod repository;
mod task_manager;

pub use builtin::BuiltinAgentCatalog;
pub use model::{
    AgentBackendDescriptor, AgentBackendId, AgentBackendKind, AgentBackendSource, AgentTask,
    AgentTaskId, AvailabilityState, BackendResolution, RunId, TaskFailure, TaskState,
    TaskTransitionError,
};
pub use registry::{AgentRegistry, AgentRegistryError};
pub use repository::{AgentMetadataRepository, SqliteAgentMetadataRepository};
pub use task_manager::{AgentTaskManager, AgentTaskManagerError};
