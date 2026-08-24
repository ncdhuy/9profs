use aion_protocol::events::ToolCategory;
use aion_tools::{Tool, registry::ToolRegistry as AionRsToolRegistry};
use aion_types::tool::{JsonSchema, ToolResult as AionRsToolResult};
use async_trait::async_trait;
use nineprofs_tools::{
    ToolDefinition, ToolEffect, ToolError, ToolExecutor, ToolInvocation, ToolInvocationContext,
    ToolRegistry, ToolSet,
};
use serde_json::Value;

/// Build the AionRS registry from only the 9Profs-authorized tools.
///
/// AionRS types and dispatch stay entirely in this adapter. The 9Profs
/// registry remains the source of truth for definitions, enabled state, and
/// per-run authorization.
pub(crate) fn build_aionrs_tool_registry(
    registry: &ToolRegistry,
    tool_set: &ToolSet,
    context: ToolInvocationContext,
) -> Result<AionRsToolRegistry, ToolError> {
    let mut aionrs_registry = AionRsToolRegistry::new();
    for registration in registry.registrations_for(tool_set)? {
        aionrs_registry.register(Box::new(AionRsToolAdapter {
            definition: registration.definition,
            handler: registration.handler,
            context: context.clone(),
        }));
    }
    Ok(aionrs_registry)
}

struct AionRsToolAdapter {
    definition: ToolDefinition,
    handler: ToolExecutor,
    context: ToolInvocationContext,
}

#[async_trait]
impl Tool for AionRsToolAdapter {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn input_schema(&self) -> JsonSchema {
        self.definition.input_schema.clone()
    }

    fn is_concurrency_safe(&self, _input: &Value) -> bool {
        false
    }

    async fn execute(&self, input: Value) -> AionRsToolResult {
        let invocation = ToolInvocation {
            tool_id: self.definition.id.clone(),
            arguments: input,
            context: Some(self.context.clone()),
        };
        match self.handler.execute(invocation).await {
            Ok(result) => AionRsToolResult {
                content: result_content(result.output),
                is_error: false,
            },
            Err(error) => AionRsToolResult {
                content: error.to_string(),
                is_error: true,
            },
        }
    }

    fn category(&self) -> ToolCategory {
        if self.definition.policy.effects.contains(&ToolEffect::Write) {
            ToolCategory::Edit
        } else if self
            .definition
            .policy
            .effects
            .iter()
            .any(|effect| matches!(effect, ToolEffect::Execute | ToolEffect::ExternalNetwork))
        {
            ToolCategory::Exec
        } else {
            ToolCategory::Info
        }
    }
}

fn result_content(output: Value) -> String {
    match output {
        Value::String(value) => value,
        value => serde_json::to_string(&value).unwrap_or_else(|_| "null".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use nineprofs_tools::{
        ToolHandler, ToolId, ToolPolicy, ToolProvider, ToolRegistration, ToolResult, ToolSource,
    };

    struct EchoHandler;

    #[async_trait]
    impl ToolHandler for EchoHandler {
        async fn execute(&self, invocation: ToolInvocation) -> Result<ToolResult, ToolError> {
            Ok(ToolResult::new(invocation.arguments))
        }
    }

    fn echo_registration() -> ToolRegistration {
        ToolRegistration {
            definition: ToolDefinition {
                id: ToolId::new("echo"),
                name: "echo".to_owned(),
                description: "Return supplied JSON".to_owned(),
                input_schema: json!({"type": "object"}),
                source: ToolSource::Builtin,
                policy: ToolPolicy::read_only(),
                enabled: true,
            },
            handler: Arc::new(EchoHandler),
        }
    }

    #[test]
    fn empty_tool_set_builds_empty_aionrs_registry() {
        let registry = ToolRegistry::new();
        registry.register(echo_registration()).unwrap();
        let aionrs = build_aionrs_tool_registry(
            &registry,
            &ToolSet::default(),
            ToolInvocationContext::new("run", "task"),
        )
        .unwrap();
        assert!(aionrs.tool_names().is_empty());
    }

    #[tokio::test]
    async fn explicit_tool_set_builds_only_authorized_tool_and_maps_result() {
        let registry = ToolRegistry::new();
        registry.register(echo_registration()).unwrap();
        let aionrs = build_aionrs_tool_registry(
            &registry,
            &ToolSet::from_ids([ToolId::new("echo")]),
            ToolInvocationContext::new("run", "task"),
        )
        .unwrap();
        assert_eq!(aionrs.tool_names(), vec!["echo".to_owned()]);

        let result = aionrs
            .get("echo")
            .unwrap()
            .execute(json!({"value": 9}))
            .await;
        assert_eq!(result.content, r#"{"value":9}"#);
        assert!(!result.is_error);
    }

    struct EmptyProvider;

    #[async_trait]
    impl ToolProvider for EmptyProvider {
        async fn list_tools(&self) -> Result<Vec<ToolRegistration>, ToolError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn adapter_does_not_discover_tools_from_aionrs() {
        let registry = ToolRegistry::new();
        registry.register_provider(&EmptyProvider).await.unwrap();
        assert!(
            build_aionrs_tool_registry(
                &registry,
                &ToolSet::default(),
                ToolInvocationContext::new("run", "task"),
            )
            .unwrap()
            .tool_names()
            .is_empty()
        );
    }

    #[test]
    fn mcp_registration_stays_out_of_aionrs_until_explicitly_authorized() {
        let registry = ToolRegistry::new();
        let mut registration = echo_registration();
        registration.definition.id = ToolId::new("mcp/fixture/echo");
        registration.definition.name = "mcp_fixture_echo".to_owned();
        registration.definition.source = ToolSource::Mcp;
        registry.register(registration).unwrap();

        let empty = build_aionrs_tool_registry(
            &registry,
            &ToolSet::default(),
            ToolInvocationContext::new("run", "task"),
        )
        .unwrap();
        assert!(empty.tool_names().is_empty());

        let authorized = build_aionrs_tool_registry(
            &registry,
            &ToolSet::from_ids([ToolId::new("mcp/fixture/echo")]),
            ToolInvocationContext::new("run", "task"),
        )
        .unwrap();
        assert_eq!(authorized.tool_names(), vec!["mcp_fixture_echo".to_owned()]);
    }
}
