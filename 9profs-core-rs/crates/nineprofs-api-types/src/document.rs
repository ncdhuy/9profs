use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveDocumentDto {
    pub document_id: String,
    pub document_type: String,
    pub authority: String,
    pub version: u64,
    pub capabilities: Vec<String>,
    pub availability: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentProposalChangeDto {
    #[serde(rename = "type")]
    pub change_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentProposalDto {
    pub proposal_id: String,
    pub change_set_id: String,
    pub document_id: String,
    pub authority: String,
    pub base_version: u64,
    pub status: String,
    pub freshness: String,
    pub availability: String,
    pub current_version: Option<u64>,
    pub created_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub changes: Vec<DocumentProposalChangeDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    pub retryable: bool,
}
