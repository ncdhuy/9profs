//! Explicit composition root for shared 9Profs Core infrastructure.

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use nineprofs_api_types::{HealthResponse, RuntimeInfo};
use nineprofs_assistant::{
    AssistantError, AssistantService, BuiltinAssistantCatalog, SqliteAssistantRepository,
};
use nineprofs_db::{Database, DbError, SqliteMetadataRepository};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_skills::{SkillCatalog, SkillError};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub bind_addr: SocketAddr,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub event_capacity: usize,
    pub custom_skill_roots: Vec<PathBuf>,
    /// Reserved launch-scoped secret. Authentication is intentionally not enabled in Phase 1A.
    pub session_secret: Option<Arc<str>>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let data_dir = PathBuf::from("data/9profs-core");
        Self {
            bind_addr: SocketAddr::from(([127, 0, 0, 1], 39761)),
            database_path: data_dir.join("core.db"),
            data_dir,
            event_capacity: 256,
            custom_skill_roots: Vec::new(),
            session_secret: None,
        }
    }
}

impl RuntimeConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(value) = std::env::var("NINEPROFS_CORE_ADDR") {
            if let Ok(addr) = value.parse() {
                config.bind_addr = addr;
            }
        }
        if let Ok(value) = std::env::var("NINEPROFS_CORE_DATA_DIR") {
            config.data_dir = PathBuf::from(value);
            config.database_path = config.data_dir.join("core.db");
        }
        if let Ok(value) = std::env::var("NINEPROFS_SESSION_SECRET") {
            if !value.is_empty() {
                config.session_secret = Some(Arc::from(value));
            }
        }
        if let Ok(value) = std::env::var("NINEPROFS_CUSTOM_SKILL_ROOTS") {
            config.custom_skill_roots = value
                .split(';')
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .collect();
        }

        config
    }

    pub fn session_secret_configured(&self) -> bool {
        self.session_secret.is_some()
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Database(#[from] DbError),
    #[error(transparent)]
    Skills(#[from] SkillError),
    #[error(transparent)]
    Assistant(#[from] AssistantError),
}

pub struct CoreRuntime {
    config: RuntimeConfig,
    database: Database,
    metadata_repository: SqliteMetadataRepository,
    event_bus: Arc<BroadcastEventBus>,
    skill_catalog: Arc<SkillCatalog>,
    assistant_service: AssistantService,
}

impl CoreRuntime {
    pub async fn initialize(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        let database = Database::open(&config.database_path).await?;
        Self::from_database(config, database)
    }

    pub async fn initialize_in_memory(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        let database = Database::in_memory().await?;
        Self::from_database(config, database)
    }

    fn from_database(config: RuntimeConfig, database: Database) -> Result<Self, RuntimeError> {
        let metadata_repository = database.metadata_repository();
        let event_bus = Arc::new(BroadcastEventBus::new(config.event_capacity));
        let skill_catalog = Arc::new(SkillCatalog::with_configured_roots(
            config.custom_skill_roots.clone(),
        )?);
        let assistant_service = AssistantService::new(
            SqliteAssistantRepository::new(database.pool().clone()),
            BuiltinAssistantCatalog::load().map_err(AssistantError::from)?,
            Arc::clone(&skill_catalog),
            Arc::clone(&event_bus),
        )?;
        Ok(Self {
            config,
            database,
            metadata_repository,
            event_bus,
            skill_catalog,
            assistant_service,
        })
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn metadata_repository(&self) -> &SqliteMetadataRepository {
        &self.metadata_repository
    }

    pub fn event_bus(&self) -> Arc<BroadcastEventBus> {
        Arc::clone(&self.event_bus)
    }

    pub fn skill_catalog(&self) -> Arc<SkillCatalog> {
        Arc::clone(&self.skill_catalog)
    }

    pub fn assistant_service(&self) -> &AssistantService {
        &self.assistant_service
    }

    pub fn health(&self) -> HealthResponse {
        HealthResponse::ok()
    }

    pub fn info(&self) -> RuntimeInfo {
        RuntimeInfo {
            service: "9profs-core".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol_version: "1".to_owned(),
            capabilities: vec![
                "health".to_owned(),
                "runtime".to_owned(),
                "realtime".to_owned(),
                "assistants".to_owned(),
                "skills".to_owned(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn composition_root_constructs_shared_infrastructure() {
        let runtime = CoreRuntime::initialize_in_memory(RuntimeConfig::default())
            .await
            .unwrap();

        assert_eq!(runtime.health().status, "ok");
        assert_eq!(runtime.info().service, "9profs-core");
        assert_eq!(runtime.event_bus().receiver_count(), 0);
        assert!(!runtime.config().session_secret_configured());
    }
}
