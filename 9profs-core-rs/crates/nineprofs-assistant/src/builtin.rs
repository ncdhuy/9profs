use serde::Deserialize;
use thiserror::Error;

use crate::{Assistant, AssistantSource};

const BUILTIN_MANIFEST: &str = include_str!("../assets/builtin-assistants/assistants.json");

#[derive(Debug, Error)]
pub enum BuiltinAssistantError {
    #[error("builtin assistant manifest is invalid: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("builtin assistant `{0}` is invalid")]
    InvalidAssistant(String),
    #[error("builtin assistant `{assistant}` references missing rules `{rule_file}`")]
    MissingRules {
        assistant: String,
        rule_file: String,
    },
}

#[derive(Debug, Deserialize)]
struct BuiltinManifest {
    assistants: Vec<BuiltinEntry>,
}

#[derive(Debug, Deserialize)]
struct BuiltinEntry {
    id: String,
    name: String,
    description: String,
    avatar: Option<String>,
    rule_file: Option<String>,
    enabled: Option<bool>,
    enabled_skills: Vec<String>,
    agent_ref: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BuiltinAssistantCatalog {
    assistants: Vec<Assistant>,
}

impl BuiltinAssistantCatalog {
    pub fn load() -> Result<Self, BuiltinAssistantError> {
        let manifest: BuiltinManifest = serde_json::from_str(BUILTIN_MANIFEST)?;
        let mut assistants = Vec::with_capacity(manifest.assistants.len());
        for entry in manifest.assistants {
            validate_id(&entry.id)
                .map_err(|_| BuiltinAssistantError::InvalidAssistant(entry.id.clone()))?;
            if entry.name.trim().is_empty() || entry.description.trim().is_empty() {
                return Err(BuiltinAssistantError::InvalidAssistant(entry.id));
            }
            let mut skill_ids = Vec::with_capacity(entry.enabled_skills.len());
            for skill_id in entry.enabled_skills {
                if skill_ids.iter().any(|existing| existing == &skill_id) {
                    return Err(BuiltinAssistantError::InvalidAssistant(entry.id));
                }
                skill_ids.push(skill_id);
            }
            let rules = entry
                .rule_file
                .as_deref()
                .map(|file| {
                    builtin_rules(file).ok_or_else(|| BuiltinAssistantError::MissingRules {
                        assistant: entry.id.clone(),
                        rule_file: file.to_owned(),
                    })
                })
                .transpose()?
                .unwrap_or_default()
                .to_owned();
            assistants.push(Assistant {
                id: entry.id,
                name: entry.name,
                description: entry.description,
                avatar: entry.avatar,
                source: AssistantSource::Builtin,
                rules,
                enabled: entry.enabled.unwrap_or(true),
                skill_ids,
                backend_agent_id: entry.agent_ref,
                created_at_ms: None,
                updated_at_ms: None,
            });
        }
        assistants.sort_by(|left, right| left.id.cmp(&right.id));
        for window in assistants.windows(2) {
            if window[0].id == window[1].id {
                return Err(BuiltinAssistantError::InvalidAssistant(
                    window[0].id.clone(),
                ));
            }
        }
        Ok(Self { assistants })
    }

    pub fn list(&self) -> &[Assistant] {
        &self.assistants
    }

    pub fn get(&self, id: &str) -> Option<&Assistant> {
        self.assistants.iter().find(|assistant| assistant.id == id)
    }
}

fn builtin_rules(file: &str) -> Option<&'static str> {
    match file {
        "rules/document-foundation.md" => Some(include_str!(
            "../assets/builtin-assistants/rules/document-foundation.md"
        )),
        "rules/writing-foundation.md" => Some(include_str!(
            "../assets/builtin-assistants/rules/writing-foundation.md"
        )),
        _ => None,
    }
}

fn validate_id(id: &str) -> Result<(), ()> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.chars().any(|char| {
            !(char.is_ascii_lowercase() || char.is_ascii_digit() || matches!(char, '-' | '_' | '.'))
        })
    {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_loads_validated_resources() {
        let catalog = BuiltinAssistantCatalog::load().unwrap();
        assert_eq!(catalog.list().len(), 2);
        assert_eq!(catalog.list()[0].source, AssistantSource::Builtin);
        assert!(!catalog.list()[0].rules.is_empty());
    }
}
