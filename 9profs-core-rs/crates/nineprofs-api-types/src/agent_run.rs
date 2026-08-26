use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentRunRequest {
    pub assistant_id: String,
    pub input: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActiveDocsAgentRunRequest {
    pub assistant_id: String,
    pub document_id: String,
    pub input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTaskFailureDto {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentTaskDto {
    pub task_id: String,
    pub run_id: String,
    pub backend_id: String,
    pub state: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub failure: Option<AgentTaskFailureDto>,
    pub cancellation_requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentRunContextDto {
    ActiveDocs {
        #[serde(rename = "documentId")]
        document_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRunStartedDto {
    pub run_id: String,
    pub task: AgentTaskDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<AgentRunContextDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRunDto {
    pub run_id: String,
    pub tasks: Vec<AgentTaskDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<AgentRunContextDto>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn public_agent_requests_reject_caller_supplied_tools() {
        assert!(
            serde_json::from_value::<AgentRunRequest>(json!({
                "assistant_id": "assistant",
                "input": "hello",
                "toolIds": ["office.create"]
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<ActiveDocsAgentRunRequest>(json!({
                "assistant_id": "assistant",
                "document_id": "doc-a",
                "input": "hello",
                "allowedTools": ["document.inspect_active"]
            }))
            .is_err()
        );
    }

    #[test]
    fn active_docs_context_uses_transport_safe_shape() {
        let context = AgentRunContextDto::ActiveDocs {
            document_id: "doc-a".to_owned(),
        };
        assert_eq!(
            serde_json::to_value(context).unwrap(),
            json!({
                "kind": "activeDocs",
                "documentId": "doc-a"
            })
        );
    }
}
