use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Stable, transport-neutral identity for one 9Profs tool.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ToolId(String);

impl ToolId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ToolId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for ToolId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ToolId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Where a tool originates. Providers for future sources are intentionally
/// not implemented by this foundation crate.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    Builtin,
    Mcp,
    #[serde(rename = "officecli")]
    OfficeCli,
    Research,
    Extension,
}

/// Coarse effect/risk metadata for future policy decisions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    Read,
    Write,
    Execute,
    ExternalNetwork,
}

/// Coarse policy metadata. Per-user and per-run permission workflows remain
/// outside Phase 2C0; authorization is supplied explicitly through [`ToolSet`].
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolPolicy {
    pub effects: BTreeSet<ToolEffect>,
    pub requires_confirmation: bool,
}

impl ToolPolicy {
    pub fn read_only() -> Self {
        Self {
            effects: BTreeSet::from([ToolEffect::Read]),
            requires_confirmation: false,
        }
    }

    pub fn with_effects(effects: impl IntoIterator<Item = ToolEffect>) -> Self {
        Self {
            effects: effects.into_iter().collect(),
            requires_confirmation: false,
        }
    }
}

/// Metadata advertised by the 9Profs runtime.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolDefinition {
    pub id: ToolId,
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub source: ToolSource,
    pub policy: ToolPolicy,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolInvocationContext {
    pub run_id: String,
    pub task_id: String,
}

impl ToolInvocationContext {
    pub fn new(run_id: impl Into<String>, task_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            task_id: task_id.into(),
        }
    }
}

/// Generic structured input passed to a tool handler.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolInvocation {
    pub tool_id: ToolId,
    pub arguments: Value,
    pub context: Option<ToolInvocationContext>,
}

impl ToolInvocation {
    pub fn new(tool_id: impl Into<ToolId>, arguments: Value) -> Self {
        Self {
            tool_id: tool_id.into(),
            arguments,
            context: None,
        }
    }

    pub fn with_context(mut self, context: ToolInvocationContext) -> Self {
        self.context = Some(context);
        self
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolResult {
    pub output: Value,
}

impl ToolResult {
    pub fn new(output: Value) -> Self {
        Self { output }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Error, Serialize)]
pub enum ToolError {
    #[error("tool definition has an empty ID")]
    InvalidToolId,
    #[error("tool definition has an empty name")]
    InvalidToolName,
    #[error("tool ID is already registered: {0}")]
    DuplicateToolId(ToolId),
    #[error("tool name is already registered: {0}")]
    DuplicateToolName(String),
    #[error("unknown tool: {0}")]
    UnknownTool(ToolId),
    #[error("tool is disabled: {0}")]
    ToolDisabled(ToolId),
    #[error("tool is not authorized for this run: {0}")]
    ToolNotAuthorized(ToolId),
    #[error("tool provider failed: {0}")]
    Provider(String),
    #[error("tool handler failed: {0}")]
    Handler(String),
}

/// Explicit per-run authorization. Empty by default, even when the registry
/// contains enabled tools.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolSet(BTreeSet<ToolId>);

impl ToolSet {
    pub fn from_ids(ids: impl IntoIterator<Item = impl Into<ToolId>>) -> Self {
        Self(ids.into_iter().map(Into::into).collect())
    }

    pub fn allow(&mut self, id: impl Into<ToolId>) {
        self.0.insert(id.into());
    }

    pub fn contains(&self, id: &ToolId) -> bool {
        self.0.contains(id)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn ids(&self) -> impl Iterator<Item = &ToolId> {
        self.0.iter()
    }
}
