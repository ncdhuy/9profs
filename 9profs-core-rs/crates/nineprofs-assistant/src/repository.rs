use async_trait::async_trait;
use nineprofs_common::now_ms;
use sqlx::{Row, Sqlite, SqlitePool};

use crate::{Assistant, AssistantSource};

#[async_trait]
pub trait AssistantRepository: Send + Sync {
    async fn list(&self) -> Result<Vec<Assistant>, sqlx::Error>;
    async fn get(&self, id: &str) -> Result<Option<Assistant>, sqlx::Error>;
    async fn create(&self, assistant: &Assistant) -> Result<(), sqlx::Error>;
    async fn update(&self, assistant: &Assistant) -> Result<bool, sqlx::Error>;
    async fn delete(&self, id: &str) -> Result<bool, sqlx::Error>;
}

#[derive(Clone, Debug)]
pub struct SqliteAssistantRepository {
    pool: SqlitePool,
}

impl SqliteAssistantRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn load_skills(&self, id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT skill_id FROM assistant_skill_assignments WHERE assistant_id = ? ORDER BY sort_order ASC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|row| row.get("skill_id")).collect())
    }

    async fn map_row(&self, row: sqlx::sqlite::SqliteRow) -> Result<Assistant, sqlx::Error> {
        let id: String = row.get("id");
        Ok(Assistant {
            skill_ids: self.load_skills(&id).await?,
            id,
            name: row.get("name"),
            description: row.get("description"),
            avatar: row.get("avatar"),
            source: AssistantSource::Custom,
            rules: row.get("rules"),
            enabled: row.get::<i64, _>("enabled") != 0,
            backend_agent_id: row.get("backend_agent_id"),
            created_at_ms: Some(row.get("created_at_ms")),
            updated_at_ms: Some(row.get("updated_at_ms")),
        })
    }
}

#[async_trait]
impl AssistantRepository for SqliteAssistantRepository {
    async fn list(&self) -> Result<Vec<Assistant>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, name, description, avatar, rules, enabled, backend_agent_id, created_at_ms, updated_at_ms \
             FROM assistants ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut assistants = Vec::with_capacity(rows.len());
        for row in rows {
            assistants.push(self.map_row(row).await?);
        }
        Ok(assistants)
    }

    async fn get(&self, id: &str) -> Result<Option<Assistant>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, name, description, avatar, rules, enabled, backend_agent_id, created_at_ms, updated_at_ms \
             FROM assistants WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(row) => Ok(Some(self.map_row(row).await?)),
            None => Ok(None),
        }
    }

    async fn create(&self, assistant: &Assistant) -> Result<(), sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO assistants \
             (id, name, description, avatar, rules, enabled, backend_agent_id, created_at_ms, updated_at_ms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&assistant.id)
        .bind(&assistant.name)
        .bind(&assistant.description)
        .bind(&assistant.avatar)
        .bind(&assistant.rules)
        .bind(if assistant.enabled { 1_i64 } else { 0_i64 })
        .bind(&assistant.backend_agent_id)
        .bind(assistant.created_at_ms.unwrap_or_else(now_ms))
        .bind(assistant.updated_at_ms.unwrap_or_else(now_ms))
        .execute(&mut *transaction)
        .await?;
        insert_skill_assignments(&mut transaction, assistant).await?;
        transaction.commit().await
    }

    async fn update(&self, assistant: &Assistant) -> Result<bool, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        let result = sqlx::query(
            "UPDATE assistants SET name = ?, description = ?, avatar = ?, rules = ?, enabled = ?, \
             backend_agent_id = ?, updated_at_ms = ? WHERE id = ?",
        )
        .bind(&assistant.name)
        .bind(&assistant.description)
        .bind(&assistant.avatar)
        .bind(&assistant.rules)
        .bind(if assistant.enabled { 1_i64 } else { 0_i64 })
        .bind(&assistant.backend_agent_id)
        .bind(assistant.updated_at_ms.unwrap_or_else(now_ms))
        .bind(&assistant.id)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() == 0 {
            transaction.rollback().await?;
            return Ok(false);
        }
        sqlx::query("DELETE FROM assistant_skill_assignments WHERE assistant_id = ?")
            .bind(&assistant.id)
            .execute(&mut *transaction)
            .await?;
        insert_skill_assignments(&mut transaction, assistant).await?;
        transaction.commit().await?;
        Ok(true)
    }

    async fn delete(&self, id: &str) -> Result<bool, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM assistant_skill_assignments WHERE assistant_id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        let result = sqlx::query("DELETE FROM assistants WHERE id = ?")
            .bind(id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(result.rows_affected() > 0)
    }
}

async fn insert_skill_assignments(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    assistant: &Assistant,
) -> Result<(), sqlx::Error> {
    for (sort_order, skill_id) in assistant.skill_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO assistant_skill_assignments (assistant_id, skill_id, sort_order) VALUES (?, ?, ?)",
        )
        .bind(&assistant.id)
        .bind(skill_id)
        .bind(sort_order as i64)
        .execute(&mut **transaction)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nineprofs_db::Database;

    #[tokio::test]
    async fn custom_assistant_and_skill_order_round_trip() {
        let database = Database::in_memory().await.unwrap();
        let repository = SqliteAssistantRepository::new(database.pool().clone());
        let assistant = Assistant {
            id: "custom".to_owned(),
            name: "Custom".to_owned(),
            description: "Description".to_owned(),
            avatar: None,
            source: AssistantSource::Custom,
            rules: "Rules".to_owned(),
            enabled: true,
            skill_ids: vec!["second".to_owned(), "first".to_owned()],
            backend_agent_id: Some("codex".to_owned()),
            created_at_ms: Some(1),
            updated_at_ms: Some(1),
        };
        repository.create(&assistant).await.unwrap();
        let stored = repository.get("custom").await.unwrap().unwrap();
        assert_eq!(stored.skill_ids, ["second", "first"]);
        assert_eq!(stored.backend_agent_id.as_deref(), Some("codex"));
        assert!(repository.delete("custom").await.unwrap());
        assert!(repository.get("custom").await.unwrap().is_none());
    }
}
