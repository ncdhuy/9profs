use std::fmt;

use nineprofs_common::{TimestampMs, new_id, now_ms};
use serde::{Deserialize, Serialize};

pub type AgentBackendId = String;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentBackendSource {
    Builtin,
    Custom,
    Extension,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentBackendKind {
    Embedded,
    Cli,
    Remote,
    Extension,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AvailabilityState {
    Unknown,
    Available,
    Unavailable,
    Disabled,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBackendDescriptor {
    pub id: AgentBackendId,
    pub name: String,
    pub description: String,
    pub source: AgentBackendSource,
    pub kind: AgentBackendKind,
    pub capabilities: Vec<String>,
    pub availability: AvailabilityState,
    pub availability_reason: Option<String>,
    pub enabled: bool,
    pub sort_order: i32,
    pub version: Option<String>,
    pub created_at_ms: Option<TimestampMs>,
    pub updated_at_ms: Option<TimestampMs>,
}

impl AgentBackendDescriptor {
    pub fn builtin(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        kind: AgentBackendKind,
        capabilities: impl IntoIterator<Item = impl Into<String>>,
        sort_order: i32,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            source: AgentBackendSource::Builtin,
            kind,
            capabilities: capabilities.into_iter().map(Into::into).collect(),
            availability: AvailabilityState::Unknown,
            availability_reason: None,
            enabled: true,
            sort_order,
            version: None,
            created_at_ms: None,
            updated_at_ms: None,
        }
    }

    pub fn is_resolvable(&self) -> bool {
        self.enabled && self.availability == AvailabilityState::Available
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendResolution {
    NotConfigured,
    Missing { id: AgentBackendId },
    Unknown { descriptor: AgentBackendDescriptor },
    Unavailable { descriptor: AgentBackendDescriptor },
    Disabled { descriptor: AgentBackendDescriptor },
    Resolved { descriptor: AgentBackendDescriptor },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunId(String);

impl RunId {
    pub fn new() -> Self {
        Self(new_id())
    }

    pub fn from_string(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AgentTaskId(String);

impl AgentTaskId {
    pub fn new() -> Self {
        Self(new_id())
    }

    pub fn from_string(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for AgentTaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentTaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Queued,
    Starting,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!(
            (self, next),
            (Self::Queued, Self::Starting | Self::Cancelled)
                | (
                    Self::Starting,
                    Self::Running | Self::Failed | Self::Cancelled
                )
                | (
                    Self::Running,
                    Self::Succeeded | Self::Failed | Self::Cancelled
                )
        )
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskFailure {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTask {
    pub task_id: AgentTaskId,
    pub run_id: RunId,
    pub backend_id: AgentBackendId,
    pub state: TaskState,
    pub created_at_ms: TimestampMs,
    pub updated_at_ms: TimestampMs,
    pub started_at_ms: Option<TimestampMs>,
    pub completed_at_ms: Option<TimestampMs>,
    pub failure: Option<TaskFailure>,
    pub cancellation_requested: bool,
}

impl AgentTask {
    pub fn new(task_id: AgentTaskId, run_id: RunId, backend_id: AgentBackendId) -> Self {
        let now = now_ms();
        Self {
            task_id,
            run_id,
            backend_id,
            state: TaskState::Queued,
            created_at_ms: now,
            updated_at_ms: now,
            started_at_ms: None,
            completed_at_ms: None,
            failure: None,
            cancellation_requested: false,
        }
    }

    pub fn transition(&mut self, next: TaskState) -> Result<(), TaskTransitionError> {
        if !self.state.can_transition_to(&next) {
            return Err(TaskTransitionError {
                from: self.state.clone(),
                to: next,
            });
        }
        let now = now_ms();
        if matches!(next, TaskState::Starting) && self.started_at_ms.is_none() {
            self.started_at_ms = Some(now);
        }
        if next == TaskState::Cancelled {
            self.cancellation_requested = true;
        }
        if next.is_terminal() {
            self.completed_at_ms = Some(now);
        }
        self.state = next;
        self.updated_at_ms = now;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskTransitionError {
    pub from: TaskState,
    pub to: TaskState,
}

impl fmt::Display for TaskTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid task transition: {:?} -> {:?}",
            self.from, self.to
        )
    }
}

impl std::error::Error for TaskTransitionError {}
