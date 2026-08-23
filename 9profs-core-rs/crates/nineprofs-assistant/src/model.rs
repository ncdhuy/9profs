use nineprofs_common::TimestampMs;
use serde::{Deserialize, Serialize};

pub type AgentBackendId = String;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AssistantSource {
    Builtin,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Assistant {
    pub id: String,
    pub name: String,
    pub description: String,
    pub avatar: Option<String>,
    pub source: AssistantSource,
    pub rules: String,
    pub enabled: bool,
    pub skill_ids: Vec<String>,
    pub backend_agent_id: Option<AgentBackendId>,
    pub created_at_ms: Option<TimestampMs>,
    pub updated_at_ms: Option<TimestampMs>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct CreateAssistant {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub avatar: Option<String>,
    pub rules: String,
    pub enabled: Option<bool>,
    pub skill_ids: Vec<String>,
    pub backend_agent_id: Option<AgentBackendId>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct UpdateAssistant {
    pub name: Option<String>,
    pub description: Option<String>,
    pub avatar: Option<Option<String>>,
    pub rules: Option<String>,
    pub enabled: Option<bool>,
    pub skill_ids: Option<Vec<String>>,
    pub backend_agent_id: Option<Option<AgentBackendId>>,
}
