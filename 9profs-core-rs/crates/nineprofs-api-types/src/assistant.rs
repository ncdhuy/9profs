use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistantDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub avatar: Option<String>,
    pub source: String,
    pub rules: String,
    pub enabled: bool,
    pub skill_ids: Vec<String>,
    pub backend_agent_id: Option<String>,
    pub created_at_ms: Option<i64>,
    pub updated_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CreateAssistantRequest {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub rules: String,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub backend_agent_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct UpdateAssistantRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar: Option<Option<String>>,
    pub rules: Option<String>,
    pub enabled: Option<bool>,
    pub skill_ids: Option<Vec<String>>,
    pub backend_agent_id: Option<Option<String>>,
}
