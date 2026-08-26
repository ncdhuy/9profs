use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nineprofs_tools::ToolSet;
use thiserror::Error;

pub const MAX_DOCS_AGENT_CONVERSATIONS: usize = 32;
pub const MAX_IDLE_DOCS_AGENT_CONVERSATIONS: usize = 24;
pub const MAX_DOCS_AGENT_TURNS: u32 = 100;
const IDLE_CONVERSATION_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocsAgentConversationState {
    Idle,
    Running,
    Unavailable,
}

impl DocsAgentConversationState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocsAgentConversationMetadata {
    pub conversation_id: String,
    pub assistant_id: String,
    pub document_id: String,
    pub state: DocsAgentConversationState,
    pub turn_count: u32,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug)]
pub struct DocsAgentConversationSeed {
    pub assistant_id: String,
    pub document_id: String,
    pub backend_id: String,
    pub system_instructions: String,
    pub tool_set: ToolSet,
}

#[derive(Clone, Debug)]
pub struct DocsAgentConversationTurn {
    pub conversation_id: String,
    pub assistant_id: String,
    pub document_id: String,
    pub backend_id: String,
    pub system_instructions: String,
    pub tool_set: ToolSet,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DocsAgentConversationStoreError {
    #[error("Docs agent conversation not found: {0}")]
    NotFound(String),
    #[error("Docs agent conversation is busy: {0}")]
    Busy(String),
    #[error("Docs agent conversation is unavailable: {0}")]
    Unavailable(String),
    #[error("Docs agent conversation reached its turn limit")]
    TurnLimit,
    #[error("Docs agent conversation store is at capacity")]
    Capacity,
}

struct StoredConversation {
    metadata: DocsAgentConversationMetadata,
    backend_id: String,
    system_instructions: String,
    tool_set: ToolSet,
    last_touched: Instant,
}

#[derive(Clone, Default)]
pub struct DocsAgentConversationStore {
    entries: Arc<Mutex<HashMap<String, StoredConversation>>>,
}

impl DocsAgentConversationStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(
        &self,
        seed: DocsAgentConversationSeed,
    ) -> Result<DocsAgentConversationMetadata, DocsAgentConversationStoreError> {
        let mut entries = self
            .entries
            .lock()
            .expect("conversation store lock poisoned");
        prune_expired(&mut entries);
        if entries.len() >= MAX_DOCS_AGENT_CONVERSATIONS {
            evict_oldest_idle(&mut entries).ok_or(DocsAgentConversationStoreError::Capacity)?;
        }

        let now = now_ms();
        let conversation_id = format!("docs-{}", nineprofs_agent::RunId::new());
        let metadata = DocsAgentConversationMetadata {
            conversation_id: conversation_id.clone(),
            assistant_id: seed.assistant_id,
            document_id: seed.document_id,
            state: DocsAgentConversationState::Idle,
            turn_count: 0,
            created_at_ms: now,
            updated_at_ms: now,
        };
        entries.insert(
            conversation_id,
            StoredConversation {
                metadata: metadata.clone(),
                backend_id: seed.backend_id,
                system_instructions: seed.system_instructions,
                tool_set: seed.tool_set,
                last_touched: Instant::now(),
            },
        );
        enforce_idle_limit(&mut entries);
        Ok(metadata)
    }

    pub fn get(&self, conversation_id: &str) -> Option<DocsAgentConversationMetadata> {
        let mut entries = self
            .entries
            .lock()
            .expect("conversation store lock poisoned");
        prune_expired(&mut entries);
        entries
            .get(conversation_id)
            .map(|entry| entry.metadata.clone())
    }

    pub fn begin(
        &self,
        conversation_id: &str,
    ) -> Result<DocsAgentConversationTurn, DocsAgentConversationStoreError> {
        let mut entries = self
            .entries
            .lock()
            .expect("conversation store lock poisoned");
        prune_expired(&mut entries);
        let entry = entries
            .get_mut(conversation_id)
            .ok_or_else(|| DocsAgentConversationStoreError::NotFound(conversation_id.to_owned()))?;
        match entry.metadata.state {
            DocsAgentConversationState::Running => {
                return Err(DocsAgentConversationStoreError::Busy(
                    conversation_id.to_owned(),
                ));
            }
            DocsAgentConversationState::Unavailable => {
                return Err(DocsAgentConversationStoreError::Unavailable(
                    conversation_id.to_owned(),
                ));
            }
            DocsAgentConversationState::Idle => {}
        }
        if entry.metadata.turn_count >= MAX_DOCS_AGENT_TURNS {
            return Err(DocsAgentConversationStoreError::TurnLimit);
        }
        entry.metadata.state = DocsAgentConversationState::Running;
        entry.metadata.updated_at_ms = now_ms();
        entry.last_touched = Instant::now();
        Ok(DocsAgentConversationTurn {
            conversation_id: entry.metadata.conversation_id.clone(),
            assistant_id: entry.metadata.assistant_id.clone(),
            document_id: entry.metadata.document_id.clone(),
            backend_id: entry.backend_id.clone(),
            system_instructions: entry.system_instructions.clone(),
            tool_set: entry.tool_set.clone(),
        })
    }

    pub fn finish(&self, conversation_id: &str, successful: bool) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let Some(entry) = entries.get_mut(conversation_id) else {
            return;
        };
        if entry.metadata.state != DocsAgentConversationState::Unavailable {
            entry.metadata.state = DocsAgentConversationState::Idle;
        }
        if successful {
            entry.metadata.turn_count = entry.metadata.turn_count.saturating_add(1);
        }
        entry.metadata.updated_at_ms = now_ms();
        entry.last_touched = Instant::now();
        enforce_idle_limit(&mut entries);
    }

    pub fn mark_unavailable(&self, conversation_id: &str) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if let Some(entry) = entries.get_mut(conversation_id) {
            entry.metadata.state = DocsAgentConversationState::Unavailable;
            entry.metadata.updated_at_ms = now_ms();
            entry.last_touched = Instant::now();
        }
    }
}

fn prune_expired(entries: &mut HashMap<String, StoredConversation>) {
    entries.retain(|_, entry| {
        entry.metadata.state == DocsAgentConversationState::Running
            || entry.last_touched.elapsed() <= IDLE_CONVERSATION_TTL
    });
}

fn enforce_idle_limit(entries: &mut HashMap<String, StoredConversation>) {
    while entries
        .values()
        .filter(|entry| entry.metadata.state != DocsAgentConversationState::Running)
        .count()
        > MAX_IDLE_DOCS_AGENT_CONVERSATIONS
    {
        if evict_oldest_idle(entries).is_none() {
            break;
        }
    }
}

fn evict_oldest_idle(entries: &mut HashMap<String, StoredConversation>) -> Option<String> {
    let id = entries
        .iter()
        .filter(|(_, entry)| entry.metadata.state != DocsAgentConversationState::Running)
        .min_by_key(|(_, entry)| entry.last_touched)
        .map(|(id, _)| id.clone())?;
    entries.remove(&id);
    Some(id)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(id: &str) -> DocsAgentConversationSeed {
        DocsAgentConversationSeed {
            assistant_id: id.to_owned(),
            document_id: "doc-a".to_owned(),
            backend_id: "nineprofs-default".to_owned(),
            system_instructions: "snapshot".to_owned(),
            tool_set: ToolSet::from_ids([
                "document.list_active",
                "document.inspect_active",
                "document.propose_active_changes",
            ]),
        }
    }

    #[test]
    fn core_generates_bound_identity_and_preserves_snapshot() {
        let store = DocsAgentConversationStore::new();
        let created = store.create(seed("assistant-a")).unwrap();
        assert!(created.conversation_id.starts_with("docs-"));
        assert_eq!(created.document_id, "doc-a");
        let turn = store.begin(&created.conversation_id).unwrap();
        assert_eq!(turn.assistant_id, "assistant-a");
        assert_eq!(turn.system_instructions, "snapshot");
        let tool_ids: Vec<_> = turn
            .tool_set
            .ids()
            .map(|id| id.as_str().to_owned())
            .collect();
        assert_eq!(
            tool_ids,
            vec![
                "document.inspect_active",
                "document.list_active",
                "document.propose_active_changes"
            ]
        );
        store.finish(&created.conversation_id, true);
        assert_eq!(store.get(&created.conversation_id).unwrap().turn_count, 1);
        let next_turn = store.begin(&created.conversation_id).unwrap();
        assert_eq!(next_turn.document_id, "doc-a");
        assert_eq!(next_turn.assistant_id, "assistant-a");
    }

    #[test]
    fn busy_and_unavailable_states_are_typed_and_bound() {
        let store = DocsAgentConversationStore::new();
        let created = store.create(seed("assistant-a")).unwrap();
        assert!(matches!(store.begin(&created.conversation_id), Ok(_)));
        assert!(matches!(
            store.begin(&created.conversation_id),
            Err(DocsAgentConversationStoreError::Busy(_))
        ));
        store.mark_unavailable(&created.conversation_id);
        store.finish(&created.conversation_id, true);
        assert_eq!(
            store.get(&created.conversation_id).unwrap().state,
            DocsAgentConversationState::Unavailable
        );
        assert!(matches!(
            store.begin(&created.conversation_id),
            Err(DocsAgentConversationStoreError::Unavailable(_))
        ));
    }
}
