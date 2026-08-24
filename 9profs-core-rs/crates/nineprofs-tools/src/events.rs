use serde::{Deserialize, Serialize};

use crate::{ToolError, ToolId, ToolInvocation, ToolResult};

/// Transport-safe future tool lifecycle events. The runtime does not emit
/// these until a production 9Profs tool is executing.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "name", content = "payload")]
pub enum ToolEvent {
    #[serde(rename = "tool.started")]
    Started { invocation: ToolInvocation },
    #[serde(rename = "tool.completed")]
    Completed { tool_id: ToolId, result: ToolResult },
    #[serde(rename = "tool.failed")]
    Failed { tool_id: ToolId, error: ToolError },
}

impl ToolEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Started { .. } => "tool.started",
            Self::Completed { .. } => "tool.completed",
            Self::Failed { .. } => "tool.failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn events_use_transport_safe_names() {
        let event = ToolEvent::Started {
            invocation: ToolInvocation::new("echo", json!({"value": 1})),
        };
        assert_eq!(event.name(), "tool.started");
        assert_eq!(
            serde_json::to_value(event).unwrap()["name"],
            json!("tool.started")
        );
    }
}
