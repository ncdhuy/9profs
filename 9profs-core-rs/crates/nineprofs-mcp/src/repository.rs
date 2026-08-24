use nineprofs_common::now_ms;
use sqlx::{Row, SqlitePool};

use crate::{
    error::McpError,
    model::{McpServerConfig, McpServerId, McpTransportConfig},
};

#[derive(Clone, Debug)]
pub struct SqliteMcpServerRepository {
    pool: SqlitePool,
}

impl SqliteMcpServerRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<McpServerConfig>, McpError> {
        let rows = sqlx::query(
            "SELECT id, name, description, enabled, startup_timeout_ms, transport_json, created_at_ms, updated_at_ms \
             FROM mcp_servers ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_config).collect()
    }

    pub async fn get(&self, id: &McpServerId) -> Result<McpServerConfig, McpError> {
        let row = sqlx::query(
            "SELECT id, name, description, enabled, startup_timeout_ms, transport_json, created_at_ms, updated_at_ms \
             FROM mcp_servers WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| McpError::NotFound(id.to_string()))?;
        row_to_config(row)
    }

    pub async fn create(&self, mut config: McpServerConfig) -> Result<McpServerConfig, McpError> {
        config.validate()?;
        let now = now_ms();
        config.created_at_ms = now;
        config.updated_at_ms = now;
        let transport_json = serde_json::to_string(&config.transport).map_err(|_| {
            McpError::Invalid("transport configuration is not serializable".to_owned())
        })?;
        sqlx::query(
            "INSERT INTO mcp_servers \
             (id, name, description, enabled, startup_timeout_ms, transport_json, created_at_ms, updated_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(config.id.as_str())
        .bind(&config.name)
        .bind(&config.description)
        .bind(config.enabled)
        .bind(config.startup_timeout_ms as i64)
        .bind(transport_json)
        .bind(config.created_at_ms)
        .bind(config.updated_at_ms)
        .execute(&self.pool)
        .await
        .map_err(|error| {
            if error.to_string().contains("UNIQUE") {
                McpError::Conflict(config.name.clone())
            } else {
                McpError::Database(error.to_string())
            }
        })?;
        Ok(config)
    }

    pub async fn update(&self, mut config: McpServerConfig) -> Result<McpServerConfig, McpError> {
        config.validate()?;
        let previous = self.get(&config.id).await?;
        config.created_at_ms = previous.created_at_ms;
        config.updated_at_ms = now_ms();
        let transport_json = serde_json::to_string(&config.transport).map_err(|_| {
            McpError::Invalid("transport configuration is not serializable".to_owned())
        })?;
        sqlx::query(
            "UPDATE mcp_servers SET name = ?, description = ?, enabled = ?, startup_timeout_ms = ?, transport_json = ?, updated_at_ms = ? \
             WHERE id = ?",
        )
        .bind(&config.name)
        .bind(&config.description)
        .bind(config.enabled)
        .bind(config.startup_timeout_ms as i64)
        .bind(transport_json)
        .bind(config.updated_at_ms)
        .bind(config.id.as_str())
        .execute(&self.pool)
        .await
        .map_err(|error| {
            if error.to_string().contains("UNIQUE") {
                McpError::Conflict(config.name.clone())
            } else {
                McpError::Database(error.to_string())
            }
        })?;
        Ok(config)
    }

    pub async fn delete(&self, id: &McpServerId) -> Result<(), McpError> {
        let result = sqlx::query("DELETE FROM mcp_servers WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(McpError::NotFound(id.to_string()));
        }
        Ok(())
    }
}

fn row_to_config(row: sqlx::sqlite::SqliteRow) -> Result<McpServerConfig, McpError> {
    let id = McpServerId::new(row.get::<String, _>("id"))?;
    let transport =
        serde_json::from_str::<McpTransportConfig>(&row.get::<String, _>("transport_json"))
            .map_err(|_| {
                McpError::Invalid("stored MCP transport configuration is invalid".to_owned())
            })?;
    Ok(McpServerConfig {
        id,
        name: row.get("name"),
        description: row.get("description"),
        enabled: row.get("enabled"),
        startup_timeout_ms: row.get::<i64, _>("startup_timeout_ms") as u64,
        transport,
        created_at_ms: row.get("created_at_ms"),
        updated_at_ms: row.get("updated_at_ms"),
    })
}
