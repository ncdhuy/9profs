use std::time::{SystemTime, UNIX_EPOCH};

pub type TimestampMs = i64;

pub fn now_ms() -> TimestampMs {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch")
        .as_millis() as TimestampMs
}
