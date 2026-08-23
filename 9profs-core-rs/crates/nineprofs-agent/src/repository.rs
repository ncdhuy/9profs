use async_trait::async_trait;
use nineprofs_common::now_ms;
use sqlx::{Row, SqlitePool};

use crate::{AgentBackendDescriptor, AgentBackendKind, AgentBackendSource, AvailabilityState};

#[async_trait]
pub trait AgentMetadataRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<AgentBackendDescriptor>, sqlx::Error>;
    async fn upsert(&self, descriptor: &AgentBackendDescriptor) -> Result<(), sqlx::Error>;
}

#[derive(Clone, Debug)]
pub struct SqliteAgentMetadataRepository {
    pool: SqlitePool,
}

impl SqliteAgentMetadataRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentMetadataRepository for SqliteAgentMetadataRepository {
    async fn list(&self) -> Result<Vec<AgentBackendDescriptor>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, name, description, source, kind, capabilities_json, availability, \
             availability_reason, enabled, sort_order, version, created_at_ms, updated_at_ms \
             FROM agent_backends ORDER BY sort_order ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(map_row).collect()
    }

    async fn upsert(&self, descriptor: &AgentBackendDescriptor) -> Result<(), sqlx::Error> {
        let now = now_ms();
        sqlx::query(
            "INSERT INTO agent_backends \
             (id, name, description, source, kind, capabilities_json, availability, availability_reason, \
              enabled, sort_order, version, created_at_ms, updated_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, description = excluded.description, \
              source = excluded.source, kind = excluded.kind, capabilities_json = excluded.capabilities_json, \
              availability = excluded.availability, availability_reason = excluded.availability_reason, \
              enabled = excluded.enabled, sort_order = excluded.sort_order, version = excluded.version, \
              updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&descriptor.id)
        .bind(&descriptor.name)
        .bind(&descriptor.description)
        .bind(source_name(&descriptor.source))
        .bind(kind_name(&descriptor.kind))
        .bind(serde_json::to_string(&descriptor.capabilities).expect("capabilities are serializable"))
        .bind(availability_name(&descriptor.availability))
        .bind(&descriptor.availability_reason)
        .bind(if descriptor.enabled { 1_i64 } else { 0_i64 })
        .bind(descriptor.sort_order)
        .bind(&descriptor.version)
        .bind(descriptor.created_at_ms.unwrap_or(now))
        .bind(descriptor.updated_at_ms.unwrap_or(now))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn map_row(row: sqlx::sqlite::SqliteRow) -> Result<AgentBackendDescriptor, sqlx::Error> {
    let capabilities_json: String = row.get("capabilities_json");
    let capabilities = serde_json::from_str(&capabilities_json)
        .map_err(|error| sqlx::Error::Decode(Box::new(error)))?;
    Ok(AgentBackendDescriptor {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        source: parse_source(&row.get::<String, _>("source"))?,
        kind: parse_kind(&row.get::<String, _>("kind"))?,
        capabilities,
        availability: parse_availability(&row.get::<String, _>("availability"))?,
        availability_reason: row.get("availability_reason"),
        enabled: row.get::<i64, _>("enabled") != 0,
        sort_order: row.get("sort_order"),
        version: row.get("version"),
        created_at_ms: Some(row.get("created_at_ms")),
        updated_at_ms: Some(row.get("updated_at_ms")),
    })
}

fn source_name(source: &AgentBackendSource) -> &'static str {
    match source {
        AgentBackendSource::Builtin => "builtin",
        AgentBackendSource::Custom => "custom",
        AgentBackendSource::Extension => "extension",
    }
}

fn kind_name(kind: &AgentBackendKind) -> &'static str {
    match kind {
        AgentBackendKind::Embedded => "embedded",
        AgentBackendKind::Cli => "cli",
        AgentBackendKind::Remote => "remote",
        AgentBackendKind::Extension => "extension",
    }
}

fn availability_name(availability: &AvailabilityState) -> &'static str {
    match availability {
        AvailabilityState::Unknown => "unknown",
        AvailabilityState::Available => "available",
        AvailabilityState::Unavailable => "unavailable",
        AvailabilityState::Disabled => "disabled",
    }
}

fn parse_source(value: &str) -> Result<AgentBackendSource, sqlx::Error> {
    match value {
        "builtin" => Ok(AgentBackendSource::Builtin),
        "custom" => Ok(AgentBackendSource::Custom),
        "extension" => Ok(AgentBackendSource::Extension),
        _ => Err(decode_error(format!(
            "invalid agent backend source: {value}"
        ))),
    }
}

fn parse_kind(value: &str) -> Result<AgentBackendKind, sqlx::Error> {
    match value {
        "embedded" => Ok(AgentBackendKind::Embedded),
        "cli" => Ok(AgentBackendKind::Cli),
        "remote" => Ok(AgentBackendKind::Remote),
        "extension" => Ok(AgentBackendKind::Extension),
        _ => Err(decode_error(format!("invalid agent backend kind: {value}"))),
    }
}

fn parse_availability(value: &str) -> Result<AvailabilityState, sqlx::Error> {
    match value {
        "unknown" => Ok(AvailabilityState::Unknown),
        "available" => Ok(AvailabilityState::Available),
        "unavailable" => Ok(AvailabilityState::Unavailable),
        "disabled" => Ok(AvailabilityState::Disabled),
        _ => Err(decode_error(format!(
            "invalid agent backend availability: {value}"
        ))),
    }
}

fn decode_error(message: String) -> sqlx::Error {
    sqlx::Error::Decode(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nineprofs_db::Database;

    #[tokio::test]
    async fn sqlite_metadata_round_trips_descriptors() {
        let database = Database::in_memory().await.unwrap();
        let repository = SqliteAgentMetadataRepository::new(database.pool().clone());
        let descriptor = AgentBackendDescriptor {
            id: "custom".to_owned(),
            name: "Custom".to_owned(),
            description: "Description".to_owned(),
            source: AgentBackendSource::Custom,
            kind: AgentBackendKind::Remote,
            capabilities: vec!["streaming".to_owned()],
            availability: AvailabilityState::Unknown,
            availability_reason: Some("not checked".to_owned()),
            enabled: true,
            sort_order: 4,
            version: Some("1.0".to_owned()),
            created_at_ms: Some(1),
            updated_at_ms: Some(2),
        };

        repository.upsert(&descriptor).await.unwrap();
        assert_eq!(repository.list().await.unwrap(), vec![descriptor]);
    }
}
