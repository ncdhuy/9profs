use std::{collections::BTreeSet, sync::Arc};

use nineprofs_api_types::EventEnvelope;
use nineprofs_common::{new_id, now_ms};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_skills::{SkillCatalog, SkillError};
use serde_json::json;
use thiserror::Error;

use crate::{
    Assistant, AssistantSource, BuiltinAssistantCatalog, CreateAssistant,
    SqliteAssistantRepository, UpdateAssistant, builtin::BuiltinAssistantError,
    repository::AssistantRepository,
};

#[derive(Debug, Error)]
pub enum AssistantError {
    #[error("assistant not found: {0}")]
    NotFound(String),
    #[error("invalid assistant: {0}")]
    Invalid(String),
    #[error("assistant is builtin and read-only: {0}")]
    BuiltinReadOnly(String),
    #[error("skill is missing: {0}")]
    MissingSkill(String),
    #[error("skill is assigned more than once: {0}")]
    DuplicateSkill(String),
    #[error("database query failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("builtin assistant catalog failed: {0}")]
    Builtin(#[from] BuiltinAssistantError),
    #[error("skill catalog failed: {0}")]
    Skills(String),
}

pub struct AssistantService {
    repository: SqliteAssistantRepository,
    builtins: BuiltinAssistantCatalog,
    skills: Arc<SkillCatalog>,
    events: Arc<BroadcastEventBus>,
}

impl AssistantService {
    pub fn new(
        repository: SqliteAssistantRepository,
        builtins: BuiltinAssistantCatalog,
        skills: Arc<SkillCatalog>,
        events: Arc<BroadcastEventBus>,
    ) -> Result<Self, AssistantError> {
        for assistant in builtins.list() {
            validate_skill_ids(&assistant.skill_ids, &skills)?;
        }
        Ok(Self {
            repository,
            builtins,
            skills,
            events,
        })
    }

    pub fn skills(&self) -> &SkillCatalog {
        &self.skills
    }

    pub async fn list(&self) -> Result<Vec<Assistant>, AssistantError> {
        let mut assistants = self.builtins.list().to_vec();
        assistants.extend(self.repository.list().await?);
        assistants.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(assistants)
    }

    pub async fn get(&self, id: &str) -> Result<Assistant, AssistantError> {
        validate_id(id)?;
        if let Some(assistant) = self.builtins.get(id) {
            return Ok(assistant.clone());
        }
        self.repository
            .get(id)
            .await?
            .ok_or_else(|| AssistantError::NotFound(id.to_owned()))
    }

    pub async fn create(&self, input: CreateAssistant) -> Result<Assistant, AssistantError> {
        let id = input.id.unwrap_or_else(new_id);
        validate_id(&id)?;
        let name = required_text("name", input.name)?;
        let description = required_text("description", input.description)?;
        let skill_ids = validate_skill_ids(&input.skill_ids, &self.skills)?;
        if self.builtins.get(&id).is_some() || self.repository.get(&id).await?.is_some() {
            return Err(AssistantError::Invalid(format!(
                "assistant id already exists: {id}"
            )));
        }
        let assistant = Assistant {
            id,
            name,
            description,
            avatar: input.avatar,
            source: AssistantSource::Custom,
            rules: input.rules,
            enabled: input.enabled.unwrap_or(true),
            skill_ids,
            backend_agent_id: input.backend_agent_id,
            created_at_ms: Some(now_ms()),
            updated_at_ms: Some(now_ms()),
        };
        self.repository.create(&assistant).await?;
        self.publish("assistant.created", &assistant.id);
        Ok(assistant)
    }

    pub async fn update(
        &self,
        id: &str,
        input: UpdateAssistant,
    ) -> Result<Assistant, AssistantError> {
        validate_id(id)?;
        if self.builtins.get(id).is_some() {
            return Err(AssistantError::BuiltinReadOnly(id.to_owned()));
        }
        let mut assistant = self
            .repository
            .get(id)
            .await?
            .ok_or_else(|| AssistantError::NotFound(id.to_owned()))?;
        if let Some(name) = input.name {
            assistant.name = required_text("name", name)?;
        }
        if let Some(description) = input.description {
            assistant.description = required_text("description", description)?;
        }
        if let Some(avatar) = input.avatar {
            assistant.avatar = avatar;
        }
        if let Some(rules) = input.rules {
            assistant.rules = rules;
        }
        if let Some(enabled) = input.enabled {
            assistant.enabled = enabled;
        }
        if let Some(skill_ids) = input.skill_ids {
            assistant.skill_ids = validate_skill_ids(&skill_ids, &self.skills)?;
        }
        if let Some(backend_agent_id) = input.backend_agent_id {
            assistant.backend_agent_id = backend_agent_id;
        }
        assistant.updated_at_ms = Some(now_ms());
        if !self.repository.update(&assistant).await? {
            return Err(AssistantError::NotFound(id.to_owned()));
        }
        self.publish("assistant.updated", id);
        Ok(assistant)
    }

    pub async fn delete(&self, id: &str) -> Result<(), AssistantError> {
        validate_id(id)?;
        if self.builtins.get(id).is_some() {
            return Err(AssistantError::BuiltinReadOnly(id.to_owned()));
        }
        if !self.repository.delete(id).await? {
            return Err(AssistantError::NotFound(id.to_owned()));
        }
        self.publish("assistant.deleted", id);
        Ok(())
    }

    pub fn scan_skills(&self) -> nineprofs_skills::SkillScan {
        self.skills.scan()
    }

    fn publish(&self, name: &str, assistant_id: &str) {
        let _ = self.events.publish(EventEnvelope::new(
            name,
            json!({ "assistant_id": assistant_id }),
        ));
    }
}

fn required_text(field: &'static str, value: String) -> Result<String, AssistantError> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(AssistantError::Invalid(format!(
            "{field} must not be empty"
        )));
    }
    Ok(value)
}

fn validate_id(id: &str) -> Result<(), AssistantError> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id
            .chars()
            .any(|char| char.is_control() || char.is_whitespace() || matches!(char, '/' | '\\'))
    {
        return Err(AssistantError::Invalid(format!(
            "invalid assistant id: {id}"
        )));
    }
    Ok(())
}

fn validate_skill_ids(
    ids: &[String],
    skills: &SkillCatalog,
) -> Result<Vec<String>, AssistantError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            return Err(AssistantError::DuplicateSkill(id.clone()));
        }
        if skills.resolve(id).is_none() {
            return Err(AssistantError::MissingSkill(id.clone()));
        }
    }
    Ok(ids.to_vec())
}

impl From<SkillError> for AssistantError {
    fn from(error: SkillError) -> Self {
        Self::Skills(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use nineprofs_db::Database;
    use nineprofs_skills::SkillCatalog;

    use super::*;

    async fn service() -> AssistantService {
        let database = Database::in_memory().await.unwrap();
        let skills = Arc::new(SkillCatalog::with_configured_roots(Vec::<PathBuf>::new()).unwrap());
        AssistantService::new(
            SqliteAssistantRepository::new(database.pool().clone()),
            BuiltinAssistantCatalog::load().unwrap(),
            skills,
            Arc::new(BroadcastEventBus::new(8)),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn custom_crud_preserves_rules_backend_and_skill_order() {
        let service = service().await;
        let created = service
            .create(CreateAssistant {
                id: Some("custom-assistant".to_owned()),
                name: "Custom".to_owned(),
                description: "Custom description".to_owned(),
                avatar: None,
                rules: "Custom rules".to_owned(),
                enabled: Some(true),
                skill_ids: vec![
                    "writing-foundation".to_owned(),
                    "document-foundation".to_owned(),
                ],
                backend_agent_id: Some("codex".to_owned()),
            })
            .await
            .unwrap();
        assert_eq!(created.source, AssistantSource::Custom);
        assert_eq!(
            created.skill_ids,
            ["writing-foundation", "document-foundation"]
        );
        assert_eq!(created.backend_agent_id.as_deref(), Some("codex"));
        assert_eq!(
            service.get("custom-assistant").await.unwrap().rules,
            "Custom rules"
        );

        let updated = service
            .update(
                "custom-assistant",
                UpdateAssistant {
                    rules: Some("Updated rules".to_owned()),
                    ..UpdateAssistant::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.rules, "Updated rules");
        service.delete("custom-assistant").await.unwrap();
        assert!(matches!(
            service.get("custom-assistant").await,
            Err(AssistantError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn missing_and_duplicate_skills_are_rejected() {
        let service = service().await;
        let duplicate = service
            .create(CreateAssistant {
                name: "Invalid".to_owned(),
                description: "Invalid".to_owned(),
                skill_ids: vec![
                    "writing-foundation".to_owned(),
                    "writing-foundation".to_owned(),
                ],
                ..CreateAssistant::default()
            })
            .await;
        assert!(matches!(duplicate, Err(AssistantError::DuplicateSkill(_))));

        let missing = service
            .create(CreateAssistant {
                name: "Invalid".to_owned(),
                description: "Invalid".to_owned(),
                skill_ids: vec!["does-not-exist".to_owned()],
                ..CreateAssistant::default()
            })
            .await;
        assert!(matches!(missing, Err(AssistantError::MissingSkill(_))));
    }

    #[tokio::test]
    async fn builtin_assistants_are_read_only() {
        let service = service().await;
        let builtin = service.get("document-foundation").await.unwrap();
        assert_eq!(builtin.source, AssistantSource::Builtin);
        assert!(matches!(
            service.delete(&builtin.id).await,
            Err(AssistantError::BuiltinReadOnly(_))
        ));
    }
}
