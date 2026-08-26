use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocsAgentReadiness {
    Ready,
    AssistantMissing,
    AssistantDisabled,
    BackendNotConfigured,
    BackendMissing,
    BackendUnavailable,
    BackendDisabled,
    ExecutorMissing,
    ProviderNotConfigured,
    ProviderInvalid,
    RequiredToolMissing,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocsAgentAvailability {
    NotConfigured,
    Missing,
    Disabled,
    Unavailable,
    Available,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocsAgentProfile {
    pub default_assistant_id: String,
    pub readiness: DocsAgentReadiness,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<String>,
    pub assistant_availability: DocsAgentAvailability,
    pub backend_availability: DocsAgentAvailability,
    pub provider_ready: bool,
    pub capabilities: Vec<String>,
    pub supports_active_docs_runs: bool,
}
