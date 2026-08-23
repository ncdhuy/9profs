use std::{collections::BTreeMap, sync::Arc};

use nineprofs_realtime::BroadcastEventBus;
use serde_json::json;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::{
    AgentBackendDescriptor, AgentBackendSource, AgentMetadataRepository, AvailabilityState,
    BackendResolution, BuiltinAgentCatalog,
};

#[derive(Debug, Error)]
pub enum AgentRegistryError {
    #[error("agent backend `{0}` was not found")]
    NotFound(String),
    #[error("agent backend `{0}` already exists")]
    DuplicateId(String),
    #[error("invalid agent backend descriptor: {0}")]
    Invalid(String),
    #[error("agent metadata persistence failed: {0}")]
    Persistence(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct AgentRegistry {
    repository: Arc<dyn AgentMetadataRepository>,
    builtins: Vec<AgentBackendDescriptor>,
    descriptors: Arc<RwLock<BTreeMap<String, AgentBackendDescriptor>>>,
    events: Arc<BroadcastEventBus>,
}

impl AgentRegistry {
    pub fn new(
        repository: Arc<dyn AgentMetadataRepository>,
        builtins: BuiltinAgentCatalog,
        events: Arc<BroadcastEventBus>,
    ) -> Self {
        Self {
            repository,
            builtins: builtins.list().to_vec(),
            descriptors: Arc::new(RwLock::new(BTreeMap::new())),
            events,
        }
    }

    pub async fn hydrate(&self) -> Result<(), AgentRegistryError> {
        let persisted = self.repository.list().await?;
        let mut hydrated = BTreeMap::new();
        for descriptor in self.builtins.iter().cloned() {
            validate_descriptor(&descriptor)?;
            insert_unique(&mut hydrated, descriptor)?;
        }
        for descriptor in persisted {
            validate_descriptor(&descriptor)?;
            insert_unique(&mut hydrated, descriptor)?;
        }

        let count = hydrated.len();
        *self.descriptors.write().await = hydrated;
        self.publish("hydrated", json!({ "agent_count": count }));
        Ok(())
    }

    pub async fn list(&self) -> Vec<AgentBackendDescriptor> {
        let mut descriptors: Vec<_> = self.descriptors.read().await.values().cloned().collect();
        descriptors.sort_by(|left, right| {
            left.sort_order
                .cmp(&right.sort_order)
                .then_with(|| left.id.cmp(&right.id))
        });
        descriptors
    }

    pub async fn get(&self, id: &str) -> Option<AgentBackendDescriptor> {
        self.descriptors.read().await.get(id).cloned()
    }

    pub async fn resolve_assistant_backend(&self, backend_id: Option<&str>) -> BackendResolution {
        let Some(backend_id) = backend_id else {
            return BackendResolution::NotConfigured;
        };
        let Some(descriptor) = self.get(backend_id).await else {
            return BackendResolution::Missing {
                id: backend_id.to_owned(),
            };
        };
        if !descriptor.enabled || descriptor.availability == AvailabilityState::Disabled {
            return BackendResolution::Disabled { descriptor };
        }
        match descriptor.availability {
            AvailabilityState::Unknown => BackendResolution::Unknown { descriptor },
            AvailabilityState::Unavailable => BackendResolution::Unavailable { descriptor },
            AvailabilityState::Available => BackendResolution::Resolved { descriptor },
            AvailabilityState::Disabled => unreachable!("disabled availability handled above"),
        }
    }

    pub async fn register_custom(
        &self,
        descriptor: AgentBackendDescriptor,
    ) -> Result<AgentBackendDescriptor, AgentRegistryError> {
        validate_descriptor(&descriptor)?;
        if descriptor.source != AgentBackendSource::Custom {
            return Err(AgentRegistryError::Invalid(
                "only custom descriptors can be registered through this method".to_owned(),
            ));
        }
        let mut descriptors = self.descriptors.write().await;
        if descriptors.contains_key(&descriptor.id) {
            return Err(AgentRegistryError::DuplicateId(descriptor.id));
        }
        self.repository.upsert(&descriptor).await?;
        descriptors.insert(descriptor.id.clone(), descriptor.clone());
        drop(descriptors);
        self.publish("registered", json!({ "agent_id": descriptor.id }));
        Ok(descriptor)
    }

    pub async fn update_custom(
        &self,
        descriptor: AgentBackendDescriptor,
    ) -> Result<AgentBackendDescriptor, AgentRegistryError> {
        validate_descriptor(&descriptor)?;
        if descriptor.source != AgentBackendSource::Custom {
            return Err(AgentRegistryError::Invalid(
                "only custom descriptors can be updated through this method".to_owned(),
            ));
        }
        let mut descriptors = self.descriptors.write().await;
        let Some(existing) = descriptors.get(&descriptor.id) else {
            return Err(AgentRegistryError::NotFound(descriptor.id));
        };
        if existing.source != AgentBackendSource::Custom {
            return Err(AgentRegistryError::Invalid(
                "builtin and extension descriptors are read-only in Phase 2A".to_owned(),
            ));
        }
        self.repository.upsert(&descriptor).await?;
        descriptors.insert(descriptor.id.clone(), descriptor.clone());
        drop(descriptors);
        self.publish("updated", json!({ "agent_id": descriptor.id }));
        Ok(descriptor)
    }

    pub async fn set_availability(
        &self,
        id: &str,
        availability: AvailabilityState,
        reason: Option<String>,
    ) -> Result<AgentBackendDescriptor, AgentRegistryError> {
        let mut descriptors = self.descriptors.write().await;
        let Some(existing) = descriptors.get(id).cloned() else {
            return Err(AgentRegistryError::NotFound(id.to_owned()));
        };
        let mut updated = existing;
        updated.availability = availability;
        updated.availability_reason = reason;
        if updated.availability == AvailabilityState::Disabled {
            updated.enabled = false;
        }
        if updated.source == AgentBackendSource::Custom {
            self.repository.upsert(&updated).await?;
        }
        descriptors.insert(id.to_owned(), updated.clone());
        drop(descriptors);
        self.publish(
            "updated",
            json!({ "agent_id": id, "availability": updated.availability }),
        );
        Ok(updated)
    }

    fn publish(&self, change: &str, details: serde_json::Value) {
        let _ = self.events.publish(nineprofs_api_event(
            "agent.registryChanged",
            json!({ "change": change, "details": details }),
        ));
    }
}

fn nineprofs_api_event(
    name: &str,
    payload: serde_json::Value,
) -> nineprofs_api_types::EventEnvelope {
    nineprofs_api_types::EventEnvelope::new(name, payload)
}

fn insert_unique(
    descriptors: &mut BTreeMap<String, AgentBackendDescriptor>,
    descriptor: AgentBackendDescriptor,
) -> Result<(), AgentRegistryError> {
    if descriptors
        .insert(descriptor.id.clone(), descriptor.clone())
        .is_some()
    {
        return Err(AgentRegistryError::DuplicateId(descriptor.id));
    }
    Ok(())
}

fn validate_descriptor(descriptor: &AgentBackendDescriptor) -> Result<(), AgentRegistryError> {
    if descriptor.id.is_empty()
        || descriptor.id == "."
        || descriptor.id == ".."
        || descriptor.id.chars().any(|character| {
            !(character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_' | '.'))
        })
    {
        return Err(AgentRegistryError::Invalid(format!(
            "invalid backend id: {}",
            descriptor.id
        )));
    }
    if descriptor.name.trim().is_empty() || descriptor.description.trim().is_empty() {
        return Err(AgentRegistryError::Invalid(format!(
            "backend `{}` requires name and description",
            descriptor.id
        )));
    }
    if descriptor
        .capabilities
        .iter()
        .any(|capability| capability.trim().is_empty())
    {
        return Err(AgentRegistryError::Invalid(format!(
            "backend `{}` has an empty capability",
            descriptor.id
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{AgentBackendKind, BuiltinAgentCatalog, SqliteAgentMetadataRepository};
    use nineprofs_db::Database;
    use nineprofs_realtime::BroadcastEventBus;

    async fn registry() -> (AgentRegistry, Arc<BroadcastEventBus>) {
        let database = Database::in_memory().await.unwrap();
        let events = Arc::new(BroadcastEventBus::new(32));
        let registry = AgentRegistry::new(
            Arc::new(SqliteAgentMetadataRepository::new(database.pool().clone())),
            BuiltinAgentCatalog::load(),
            Arc::clone(&events),
        );
        registry.hydrate().await.unwrap();
        (registry, events)
    }

    fn custom(id: &str, sort_order: i32) -> AgentBackendDescriptor {
        AgentBackendDescriptor {
            id: id.to_owned(),
            name: id.to_owned(),
            description: "Custom backend".to_owned(),
            source: AgentBackendSource::Custom,
            kind: AgentBackendKind::Remote,
            capabilities: vec!["cancellation".to_owned()],
            availability: AvailabilityState::Unknown,
            availability_reason: None,
            enabled: true,
            sort_order,
            version: None,
            created_at_ms: None,
            updated_at_ms: None,
        }
    }

    #[tokio::test]
    async fn builtins_load_lookup_and_sort_deterministically() {
        let (registry, _) = registry().await;
        let descriptors = registry.list().await;
        assert_eq!(
            descriptors
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["nineprofs-default", "codex", "claude"]
        );
        assert!(matches!(
            registry.resolve_assistant_backend(Some("codex")).await,
            BackendResolution::Unknown { .. }
        ));
    }

    #[tokio::test]
    async fn custom_registration_rejects_duplicates_and_persists() {
        let (registry, _) = registry().await;
        registry.register_custom(custom("custom", 1)).await.unwrap();
        assert!(matches!(
            registry.register_custom(custom("custom", 1)).await,
            Err(AgentRegistryError::DuplicateId(id)) if id == "custom"
        ));
        assert_eq!(registry.get("custom").await.unwrap().name, "custom");
    }

    #[tokio::test]
    async fn custom_descriptors_hydrate_into_a_new_registry_instance() {
        let database = Database::in_memory().await.unwrap();
        let repository: Arc<dyn AgentMetadataRepository> =
            Arc::new(SqliteAgentMetadataRepository::new(database.pool().clone()));
        let events = Arc::new(BroadcastEventBus::new(32));
        let first = AgentRegistry::new(
            Arc::clone(&repository),
            BuiltinAgentCatalog::load(),
            Arc::clone(&events),
        );
        first.hydrate().await.unwrap();
        first.register_custom(custom("custom", 1)).await.unwrap();

        let second = AgentRegistry::new(repository, BuiltinAgentCatalog::load(), events);
        second.hydrate().await.unwrap();
        assert_eq!(second.get("custom").await.unwrap().id, "custom");
    }

    #[tokio::test]
    async fn availability_states_and_assistant_resolution_are_explicit() {
        let (registry, _) = registry().await;
        registry.register_custom(custom("custom", 1)).await.unwrap();
        assert!(matches!(
            registry.resolve_assistant_backend(Some("missing")).await,
            BackendResolution::Missing { id } if id == "missing"
        ));
        assert!(matches!(
            registry.resolve_assistant_backend(None).await,
            BackendResolution::NotConfigured
        ));

        registry
            .set_availability("custom", AvailabilityState::Available, None)
            .await
            .unwrap();
        assert!(matches!(
            registry.resolve_assistant_backend(Some("custom")).await,
            BackendResolution::Resolved { .. }
        ));
        registry
            .set_availability(
                "custom",
                AvailabilityState::Unavailable,
                Some("test failure".to_owned()),
            )
            .await
            .unwrap();
        assert!(matches!(
            registry.resolve_assistant_backend(Some("custom")).await,
            BackendResolution::Unavailable { descriptor }
                if descriptor.availability_reason.as_deref() == Some("test failure")
        ));
        registry
            .set_availability("custom", AvailabilityState::Disabled, None)
            .await
            .unwrap();
        assert!(matches!(
            registry.resolve_assistant_backend(Some("custom")).await,
            BackendResolution::Disabled { .. }
        ));
    }

    #[tokio::test]
    async fn registry_mutations_emit_one_transport_safe_event_each() {
        let (registry, events) = registry().await;
        let mut receiver = events.subscribe();
        registry.register_custom(custom("custom", 1)).await.unwrap();
        let event = receiver.recv().await.unwrap();
        assert_eq!(event.name, "agent.registryChanged");
        assert_eq!(event.payload["change"], "registered");
        assert_eq!(event.payload["details"]["agent_id"], "custom");
        assert_eq!(event.payload.get("task"), None);
        assert_eq!(event.payload.get("opaque"), None);
    }
}
