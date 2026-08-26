use nineprofs_agent::{AgentProviderConfigError, BackendResolution};
use nineprofs_api_types::{DocsAgentAvailability, DocsAgentProfile, DocsAgentReadiness};
use nineprofs_assistant::Assistant;
use nineprofs_document_tools::{
    DOCUMENT_INSPECT_ACTIVE, DOCUMENT_LIST_ACTIVE, DOCUMENT_PROPOSE_ACTIVE_CHANGES,
};
use nineprofs_tools::ToolDefinition;

use crate::CoreRuntime;

pub const DEFAULT_DOCS_ASSISTANT_ID: &str = "document-foundation";
pub const REQUIRED_DOCS_AGENT_TOOLS: [&str; 3] = [
    DOCUMENT_LIST_ACTIVE,
    DOCUMENT_INSPECT_ACTIVE,
    DOCUMENT_PROPOSE_ACTIVE_CHANGES,
];
const ACTIVE_DOCS_RUN_CAPABILITY: &str = "activeDocsAgentRun";

const REASON_ASSISTANT_MISSING: &str = "default Docs assistant is missing";
const REASON_ASSISTANT_DISABLED: &str = "default Docs assistant is disabled";
const REASON_BACKEND_NOT_CONFIGURED: &str = "default Docs assistant has no backend";
const REASON_BACKEND_MISSING: &str = "configured Docs backend is missing";
const REASON_BACKEND_UNAVAILABLE: &str = "configured Docs backend is unavailable";
const REASON_BACKEND_DISABLED: &str = "configured Docs backend is disabled";
const REASON_EXECUTOR_MISSING: &str = "configured Docs backend executor is missing";
const REASON_PROVIDER_NOT_CONFIGURED: &str = "Core agent provider is not configured";
const REASON_PROVIDER_INVALID: &str = "Core agent provider configuration is invalid";
const REASON_REQUIRED_TOOL_MISSING: &str = "required Docs tool is not registered";

impl CoreRuntime {
    pub async fn docs_agent_profile(&self) -> DocsAgentProfile {
        self.docs_agent_profile_for(DEFAULT_DOCS_ASSISTANT_ID).await
    }

    pub(crate) async fn docs_agent_profile_for(&self, assistant_id: &str) -> DocsAgentProfile {
        let assistant = self.assistant_service.get(assistant_id).await.ok();
        let backend_resolution = match assistant
            .as_ref()
            .and_then(|assistant| assistant.backend_agent_id.as_deref())
        {
            Some(backend_id) => Some(
                self.agent_registry
                    .resolve_assistant_backend(Some(backend_id))
                    .await,
            ),
            None => None,
        };
        let provider_error = self.execution_service.provider_configuration_error();
        let executor_available = backend_resolution
            .as_ref()
            .and_then(|resolution| match resolution {
                BackendResolution::Resolved { descriptor } => {
                    Some(self.execution_service.has_executor(&descriptor.id))
                }
                _ => None,
            })
            .unwrap_or(false);
        let registered_tools = self.tool_registry.list_definitions();
        let required_tools_available = required_docs_tools_available(&registered_tools);
        let (readiness, reason) = classify_docs_agent_readiness(
            assistant.as_ref(),
            backend_resolution.as_ref(),
            executor_available,
            provider_error.as_ref(),
            required_tools_available,
        );

        DocsAgentProfile {
            default_assistant_id: assistant_id.to_owned(),
            readiness,
            reason: reason.map(str::to_owned),
            backend_id: assistant
                .as_ref()
                .and_then(|assistant| assistant.backend_agent_id.clone()),
            assistant_availability: match assistant.as_ref() {
                None => DocsAgentAvailability::Missing,
                Some(assistant) if !assistant.enabled => DocsAgentAvailability::Disabled,
                Some(_) => DocsAgentAvailability::Available,
            },
            backend_availability: backend_resolution
                .as_ref()
                .map(backend_availability)
                .unwrap_or_else(|| {
                    if assistant.is_some() {
                        DocsAgentAvailability::NotConfigured
                    } else {
                        DocsAgentAvailability::Missing
                    }
                }),
            provider_ready: provider_error.is_none(),
            capabilities: REQUIRED_DOCS_AGENT_TOOLS
                .iter()
                .filter(|tool_id| {
                    registered_tools
                        .iter()
                        .any(|definition| definition.enabled && definition.id.as_str() == **tool_id)
                })
                .map(|tool_id| (*tool_id).to_owned())
                .chain(required_tools_available.then_some(ACTIVE_DOCS_RUN_CAPABILITY.to_owned()))
                .collect(),
            supports_active_docs_runs: matches!(
                classify_docs_agent_readiness(
                    assistant.as_ref(),
                    backend_resolution.as_ref(),
                    executor_available,
                    provider_error.as_ref(),
                    required_tools_available,
                )
                .0,
                DocsAgentReadiness::Ready
            ),
        }
    }
}

fn required_docs_tools_available(definitions: &[ToolDefinition]) -> bool {
    REQUIRED_DOCS_AGENT_TOOLS.iter().all(|tool_id| {
        definitions
            .iter()
            .any(|definition| definition.enabled && definition.id.as_str() == *tool_id)
    })
}

fn backend_availability(resolution: &BackendResolution) -> DocsAgentAvailability {
    match resolution {
        BackendResolution::NotConfigured => DocsAgentAvailability::NotConfigured,
        BackendResolution::Missing { .. } => DocsAgentAvailability::Missing,
        BackendResolution::Unknown { .. } | BackendResolution::Unavailable { .. } => {
            DocsAgentAvailability::Unavailable
        }
        BackendResolution::Disabled { .. } => DocsAgentAvailability::Disabled,
        BackendResolution::Resolved { .. } => DocsAgentAvailability::Available,
    }
}

fn classify_docs_agent_readiness(
    assistant: Option<&Assistant>,
    backend_resolution: Option<&BackendResolution>,
    executor_available: bool,
    provider_error: Option<&AgentProviderConfigError>,
    required_tools_available: bool,
) -> (DocsAgentReadiness, Option<&'static str>) {
    let Some(assistant) = assistant else {
        return (
            DocsAgentReadiness::AssistantMissing,
            Some(REASON_ASSISTANT_MISSING),
        );
    };
    if !assistant.enabled {
        return (
            DocsAgentReadiness::AssistantDisabled,
            Some(REASON_ASSISTANT_DISABLED),
        );
    }

    let Some(backend_resolution) = backend_resolution else {
        return (
            DocsAgentReadiness::BackendNotConfigured,
            Some(REASON_BACKEND_NOT_CONFIGURED),
        );
    };
    match backend_resolution {
        BackendResolution::NotConfigured => (
            DocsAgentReadiness::BackendNotConfigured,
            Some(REASON_BACKEND_NOT_CONFIGURED),
        ),
        BackendResolution::Missing { .. } => (
            DocsAgentReadiness::BackendMissing,
            Some(REASON_BACKEND_MISSING),
        ),
        BackendResolution::Unknown { .. } | BackendResolution::Unavailable { .. } => {
            if let Some(provider_error) = provider_error {
                return provider_readiness(provider_error);
            }
            (
                DocsAgentReadiness::BackendUnavailable,
                Some(REASON_BACKEND_UNAVAILABLE),
            )
        }
        BackendResolution::Disabled { .. } => (
            DocsAgentReadiness::BackendDisabled,
            Some(REASON_BACKEND_DISABLED),
        ),
        BackendResolution::Resolved { .. } => {
            if !executor_available {
                return (
                    DocsAgentReadiness::ExecutorMissing,
                    Some(REASON_EXECUTOR_MISSING),
                );
            }
            if let Some(provider_error) = provider_error {
                return provider_readiness(provider_error);
            }
            if !required_tools_available {
                return (
                    DocsAgentReadiness::RequiredToolMissing,
                    Some(REASON_REQUIRED_TOOL_MISSING),
                );
            }
            (DocsAgentReadiness::Ready, None)
        }
    }
}

fn provider_readiness(
    error: &AgentProviderConfigError,
) -> (DocsAgentReadiness, Option<&'static str>) {
    match error {
        AgentProviderConfigError::UnsupportedProvider(_)
        | AgentProviderConfigError::InvalidBaseUrl => (
            DocsAgentReadiness::ProviderInvalid,
            Some(REASON_PROVIDER_INVALID),
        ),
        AgentProviderConfigError::MissingProvider
        | AgentProviderConfigError::MissingModel
        | AgentProviderConfigError::MissingCredentialEnvironment
        | AgentProviderConfigError::MissingCredential => (
            DocsAgentReadiness::ProviderNotConfigured,
            Some(REASON_PROVIDER_NOT_CONFIGURED),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nineprofs_agent::{AgentBackendDescriptor, AgentBackendKind, AgentBackendSource};

    fn assistant(enabled: bool, backend_agent_id: Option<&str>) -> Assistant {
        Assistant {
            id: DEFAULT_DOCS_ASSISTANT_ID.to_owned(),
            name: "Document Foundation".to_owned(),
            description: "test".to_owned(),
            avatar: None,
            source: nineprofs_assistant::AssistantSource::Builtin,
            rules: "rules".to_owned(),
            enabled,
            skill_ids: vec!["document-foundation".to_owned()],
            backend_agent_id: backend_agent_id.map(str::to_owned),
            created_at_ms: None,
            updated_at_ms: None,
        }
    }

    fn resolved_backend() -> BackendResolution {
        BackendResolution::Resolved {
            descriptor: AgentBackendDescriptor {
                id: "nineprofs-default".to_owned(),
                name: "9Profs Default".to_owned(),
                description: "test".to_owned(),
                source: AgentBackendSource::Builtin,
                kind: AgentBackendKind::Embedded,
                capabilities: vec![],
                availability: nineprofs_agent::AvailabilityState::Available,
                availability_reason: None,
                enabled: true,
                sort_order: 0,
                version: None,
                created_at_ms: None,
                updated_at_ms: None,
            },
        }
    }

    #[test]
    fn readiness_classifier_covers_safe_failure_states() {
        let enabled = assistant(true, Some("nineprofs-default"));
        let backend = resolved_backend();
        let cases = [
            (
                "assistant missing",
                None,
                None,
                false,
                None,
                true,
                DocsAgentReadiness::AssistantMissing,
            ),
            (
                "assistant disabled",
                Some(&assistant(false, Some("nineprofs-default"))),
                None,
                false,
                None,
                true,
                DocsAgentReadiness::AssistantDisabled,
            ),
            (
                "backend not configured",
                Some(&assistant(true, None)),
                None,
                false,
                None,
                true,
                DocsAgentReadiness::BackendNotConfigured,
            ),
            (
                "backend missing",
                Some(&enabled),
                Some(&BackendResolution::Missing {
                    id: "missing".to_owned(),
                }),
                false,
                None,
                true,
                DocsAgentReadiness::BackendMissing,
            ),
            (
                "backend unavailable",
                Some(&enabled),
                Some(&BackendResolution::Unavailable {
                    descriptor: match backend.clone() {
                        BackendResolution::Resolved { descriptor } => descriptor,
                        _ => unreachable!(),
                    },
                }),
                false,
                None,
                true,
                DocsAgentReadiness::BackendUnavailable,
            ),
            (
                "backend disabled",
                Some(&enabled),
                Some(&BackendResolution::Disabled {
                    descriptor: match backend.clone() {
                        BackendResolution::Resolved { descriptor } => descriptor,
                        _ => unreachable!(),
                    },
                }),
                false,
                None,
                true,
                DocsAgentReadiness::BackendDisabled,
            ),
            (
                "executor missing",
                Some(&enabled),
                Some(&backend),
                false,
                None,
                true,
                DocsAgentReadiness::ExecutorMissing,
            ),
            (
                "provider missing",
                Some(&enabled),
                Some(&backend),
                true,
                Some(&AgentProviderConfigError::MissingCredential),
                true,
                DocsAgentReadiness::ProviderNotConfigured,
            ),
            (
                "provider invalid",
                Some(&enabled),
                Some(&backend),
                true,
                Some(&AgentProviderConfigError::InvalidBaseUrl),
                true,
                DocsAgentReadiness::ProviderInvalid,
            ),
            (
                "required tool missing",
                Some(&enabled),
                Some(&backend),
                true,
                None,
                false,
                DocsAgentReadiness::RequiredToolMissing,
            ),
            (
                "ready",
                Some(&enabled),
                Some(&backend),
                true,
                None,
                true,
                DocsAgentReadiness::Ready,
            ),
        ];

        for (name, assistant, backend, executor, provider, tools, expected) in cases {
            let (actual, reason) =
                classify_docs_agent_readiness(assistant, backend, executor, provider, tools);
            assert_eq!(actual, expected, "{name}");
            if actual == DocsAgentReadiness::Ready {
                assert_eq!(reason, None, "{name}");
            } else {
                assert!(reason.is_some(), "{name}");
                assert!(!reason.unwrap().contains("secret"), "{name}");
            }
        }
    }

    #[test]
    fn required_docs_tools_fail_closed_when_not_registered() {
        assert!(!required_docs_tools_available(&[]));
    }
}
