use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateDocumentAgentConversationRequest {
    pub assistant_id: String,
    pub document_id: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateDocumentAgentConversationRunRequest {
    pub input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentAgentConversationDto {
    pub conversation_id: String,
    pub assistant_id: String,
    pub document_id: String,
    pub state: String,
    pub turn_count: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn conversation_requests_reject_tool_and_backend_fields() {
        assert!(
            serde_json::from_value::<CreateDocumentAgentConversationRequest>(json!({
                "assistant_id": "document-foundation",
                "document_id": "doc-a",
                "toolIds": ["document.inspect_active"]
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<CreateDocumentAgentConversationRunRequest>(json!({
                "input": "inspect",
                "backendId": "nineprofs-default"
            }))
            .is_err()
        );
    }

    #[test]
    fn conversation_metadata_uses_safe_camel_case_shape() {
        let dto = DocumentAgentConversationDto {
            conversation_id: "docs-test".to_owned(),
            assistant_id: "document-foundation".to_owned(),
            document_id: "doc-a".to_owned(),
            state: "idle".to_owned(),
            turn_count: 2,
            created_at_ms: 1,
            updated_at_ms: 2,
        };
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            json!({
                "conversationId": "docs-test",
                "assistantId": "document-foundation",
                "documentId": "doc-a",
                "state": "idle",
                "turnCount": 2,
                "createdAtMs": 1,
                "updatedAtMs": 2
            })
        );
    }
}
