use nineprofs_common::{TimestampMs, new_id, now_ms};
use serde::{Deserialize, Serialize};

/// Generic realtime event envelope. Event names use `domain.actionName`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventEnvelope {
    pub id: String,
    pub name: String,
    pub occurred_at_ms: TimestampMs,
    pub payload: serde_json::Value,
}

impl EventEnvelope {
    pub fn new(name: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            occurred_at_ms: now_ms(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn event_envelope_round_trips() {
        let event = EventEnvelope::new("runtime.started", json!({"mode": "local"}));
        let encoded = serde_json::to_string(&event).unwrap();
        let decoded: EventEnvelope = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.name, "runtime.started");
        assert_eq!(decoded.payload["mode"], "local");
        assert!(!decoded.id.is_empty());
    }
}
