use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
};

use crate::{
    ToolDefinition, ToolError, ToolExecutor, ToolId, ToolInvocation, ToolProvider,
    ToolRegistration, ToolResult, ToolSet,
};

/// Concurrency-safe source of truth for available 9Profs tools.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Arc<RwLock<BTreeMap<ToolId, ToolRegistration>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, registration: ToolRegistration) -> Result<(), ToolError> {
        self.register_many(vec![registration])
    }

    fn register_many(&self, registrations: Vec<ToolRegistration>) -> Result<(), ToolError> {
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for registration in &registrations {
            validate_definition(&registration.definition)?;
            if !ids.insert(registration.definition.id.clone()) {
                return Err(ToolError::DuplicateToolId(
                    registration.definition.id.clone(),
                ));
            }
            if !names.insert(registration.definition.name.clone()) {
                return Err(ToolError::DuplicateToolName(
                    registration.definition.name.clone(),
                ));
            }
        }

        let mut tools = self.tools.write().expect("tool registry lock poisoned");
        for registration in &registrations {
            let definition = &registration.definition;
            if tools.contains_key(&definition.id) {
                return Err(ToolError::DuplicateToolId(definition.id.clone()));
            }
            if tools
                .values()
                .any(|existing| existing.definition.name == definition.name)
            {
                return Err(ToolError::DuplicateToolName(definition.name.clone()));
            }
        }

        for registration in registrations {
            tools.insert(registration.definition.id.clone(), registration);
        }
        Ok(())
    }

    pub async fn register_provider(
        &self,
        provider: &dyn ToolProvider,
    ) -> Result<Vec<ToolId>, ToolError> {
        let registrations = provider.list_tools().await?;
        let ids = registrations
            .iter()
            .map(|registration| registration.definition.id.clone())
            .collect();
        self.register_many(registrations)?;
        Ok(ids)
    }

    /// Atomically replace all registrations from one tool source.
    pub fn replace_source(
        &self,
        source: crate::ToolSource,
        registrations: Vec<ToolRegistration>,
    ) -> Result<Vec<ToolId>, ToolError> {
        let mut tools = self.tools.write().expect("tool registry lock poisoned");
        let mut replacement = tools.clone();
        replacement.retain(|_, registration| registration.definition.source != source);
        validate_against(&replacement, &registrations)?;
        for registration in registrations {
            replacement.insert(registration.definition.id.clone(), registration);
        }
        let ids = replacement
            .values()
            .filter(|registration| registration.definition.source == source)
            .map(|registration| registration.definition.id.clone())
            .collect();
        *tools = replacement;
        Ok(ids)
    }

    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .read()
            .expect("tool registry lock poisoned")
            .values()
            .map(|registration| registration.definition.clone())
            .collect()
    }

    pub fn get_definition(&self, id: &ToolId) -> Option<ToolDefinition> {
        self.tools
            .read()
            .expect("tool registry lock poisoned")
            .get(id)
            .map(|registration| registration.definition.clone())
    }

    pub fn set_enabled(&self, id: &ToolId, enabled: bool) -> Result<(), ToolError> {
        let mut tools = self.tools.write().expect("tool registry lock poisoned");
        let registration = tools
            .get_mut(id)
            .ok_or_else(|| ToolError::UnknownTool(id.clone()))?;
        registration.definition.enabled = enabled;
        Ok(())
    }

    pub fn registrations_for(
        &self,
        tool_set: &ToolSet,
    ) -> Result<Vec<ToolRegistration>, ToolError> {
        let tools = self.tools.read().expect("tool registry lock poisoned");
        tool_set
            .ids()
            .map(|id| {
                let registration = tools
                    .get(id)
                    .ok_or_else(|| ToolError::UnknownTool(id.clone()))?;
                ensure_usable(registration, tool_set)?;
                Ok(registration.clone())
            })
            .collect()
    }

    pub fn handler_for(&self, id: &ToolId, tool_set: &ToolSet) -> Result<ToolExecutor, ToolError> {
        let tools = self.tools.read().expect("tool registry lock poisoned");
        let registration = tools
            .get(id)
            .ok_or_else(|| ToolError::UnknownTool(id.clone()))?;
        ensure_usable(registration, tool_set)?;
        Ok(Arc::clone(&registration.handler))
    }

    pub async fn execute(
        &self,
        invocation: ToolInvocation,
        tool_set: &ToolSet,
    ) -> Result<ToolResult, ToolError> {
        let handler = self.handler_for(&invocation.tool_id, tool_set)?;
        handler.execute(invocation).await
    }
}

fn validate_definition(definition: &ToolDefinition) -> Result<(), ToolError> {
    if definition.id.as_str().trim().is_empty() {
        return Err(ToolError::InvalidToolId);
    }
    if definition.name.trim().is_empty() {
        return Err(ToolError::InvalidToolName);
    }
    Ok(())
}

fn validate_against(
    existing: &BTreeMap<ToolId, ToolRegistration>,
    registrations: &[ToolRegistration],
) -> Result<(), ToolError> {
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for registration in registrations {
        validate_definition(&registration.definition)?;
        let definition = &registration.definition;
        if !ids.insert(definition.id.clone()) || existing.contains_key(&definition.id) {
            return Err(ToolError::DuplicateToolId(definition.id.clone()));
        }
        if !names.insert(definition.name.clone())
            || existing
                .values()
                .any(|existing| existing.definition.name == definition.name)
        {
            return Err(ToolError::DuplicateToolName(definition.name.clone()));
        }
    }
    Ok(())
}

fn ensure_usable(registration: &ToolRegistration, tool_set: &ToolSet) -> Result<(), ToolError> {
    let definition = &registration.definition;
    if !definition.enabled {
        return Err(ToolError::ToolDisabled(definition.id.clone()));
    }
    if !tool_set.contains(&definition.id) {
        return Err(ToolError::ToolNotAuthorized(definition.id.clone()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::{ToolEffect, ToolPolicy, ToolSource};

    struct EchoHandler;

    #[async_trait]
    impl crate::ToolHandler for EchoHandler {
        async fn execute(&self, invocation: ToolInvocation) -> Result<ToolResult, ToolError> {
            Ok(ToolResult::new(invocation.arguments))
        }
    }

    struct FailingHandler;

    #[async_trait]
    impl crate::ToolHandler for FailingHandler {
        async fn execute(&self, _invocation: ToolInvocation) -> Result<ToolResult, ToolError> {
            Err(ToolError::Handler("deterministic failure".to_owned()))
        }
    }

    fn registration(id: &str, handler: impl crate::ToolHandler + 'static) -> ToolRegistration {
        ToolRegistration {
            definition: ToolDefinition {
                id: ToolId::new(id),
                name: id.to_owned(),
                description: format!("test {id}"),
                input_schema: json!({"type": "object"}),
                source: ToolSource::Builtin,
                policy: ToolPolicy::with_effects([ToolEffect::Read]),
                enabled: true,
            },
            handler: Arc::new(handler),
        }
    }

    #[test]
    fn registration_lookup_order_and_duplicate_detection_are_stable() {
        let registry = ToolRegistry::new();
        registry
            .register(registration("zeta", EchoHandler))
            .unwrap();
        registry
            .register(registration("alpha", EchoHandler))
            .unwrap();

        let ids: Vec<_> = registry
            .list_definitions()
            .into_iter()
            .map(|definition| definition.id)
            .collect();
        assert_eq!(ids, vec![ToolId::new("alpha"), ToolId::new("zeta")]);
        assert!(matches!(
            registry.register(registration("alpha", EchoHandler)),
            Err(ToolError::DuplicateToolId(id)) if id == ToolId::new("alpha")
        ));
        assert!(matches!(
            registry.register(ToolRegistration {
                definition: ToolDefinition { name: "zeta".to_owned(), id: ToolId::new("other"), ..registration("other", EchoHandler).definition },
                handler: Arc::new(EchoHandler),
            }),
            Err(ToolError::DuplicateToolName(name)) if name == "zeta"
        ));
    }

    #[test]
    fn disabled_tools_remain_listed_but_not_executable() {
        let registry = ToolRegistry::new();
        let id = ToolId::new("echo");
        registry
            .register(registration(id.as_str(), EchoHandler))
            .unwrap();
        registry.set_enabled(&id, false).unwrap();
        assert!(!registry.list_definitions()[0].enabled);
        assert!(matches!(
            registry.handler_for(&id, &ToolSet::from_ids([id.clone()])),
            Err(ToolError::ToolDisabled(actual)) if actual == id
        ));
    }

    #[tokio::test]
    async fn default_deny_explicit_authorization_and_handler_errors_work() {
        let registry = ToolRegistry::new();
        let echo = ToolId::new("echo");
        let failing = ToolId::new("failing");
        registry
            .register(registration(echo.as_str(), EchoHandler))
            .unwrap();
        registry
            .register(registration(failing.as_str(), FailingHandler))
            .unwrap();

        let invocation = ToolInvocation::new(echo.clone(), json!({"value": 7}));
        assert!(matches!(
            registry.execute(invocation.clone(), &ToolSet::default()).await,
            Err(ToolError::ToolNotAuthorized(id)) if id == echo
        ));
        let result = registry
            .execute(invocation, &ToolSet::from_ids([echo.clone()]))
            .await
            .unwrap();
        assert_eq!(result.output, json!({"value": 7}));
        assert!(matches!(
            registry
                .execute(ToolInvocation::new(failing.clone(), json!({})), &ToolSet::from_ids([failing.clone()]))
                .await,
            Err(ToolError::Handler(message)) if message == "deterministic failure"
        ));
        assert!(matches!(
            registry
                .execute(ToolInvocation::new("missing", json!({})), &ToolSet::from_ids([ToolId::new("missing")]))
                .await,
            Err(ToolError::UnknownTool(id)) if id == ToolId::new("missing")
        ));
    }

    struct TestProvider;

    #[async_trait]
    impl crate::ToolProvider for TestProvider {
        async fn list_tools(&self) -> Result<Vec<ToolRegistration>, ToolError> {
            Ok(vec![registration("provider-echo", EchoHandler)])
        }
    }

    #[tokio::test]
    async fn provider_registration_contributes_to_registry() {
        let registry = ToolRegistry::new();
        assert_eq!(
            registry.register_provider(&TestProvider).await.unwrap(),
            vec![ToolId::new("provider-echo")]
        );
        assert_eq!(registry.list_definitions()[0].source, ToolSource::Builtin);
    }

    #[test]
    fn replacing_one_source_removes_stale_registrations_atomically() {
        let registry = ToolRegistry::new();
        let mut old = registration("old", EchoHandler);
        old.definition.source = ToolSource::Mcp;
        registry.replace_source(ToolSource::Mcp, vec![old]).unwrap();

        let mut current = registration("current", EchoHandler);
        current.definition.source = ToolSource::Mcp;
        registry
            .replace_source(ToolSource::Mcp, vec![current])
            .unwrap();
        let ids: Vec<_> = registry
            .list_definitions()
            .into_iter()
            .map(|definition| definition.id)
            .collect();
        assert_eq!(ids, vec![ToolId::new("current")]);
    }
}
