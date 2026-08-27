//! Explicit composition root for shared 9Profs Core infrastructure.

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use nineprofs_agent::{
    AgentExecutor, AgentExecutorRegistry, AgentRegistry, AgentRegistryError, AgentTaskManager,
    AionRsExecutor, AvailabilityState, BackendResolution, BuiltinAgentCatalog,
    SqliteAgentMetadataRepository,
};
use nineprofs_api_types::{HealthResponse, RuntimeInfo};
use nineprofs_assistant::{
    AssistantError, AssistantService, BuiltinAssistantCatalog, SqliteAssistantRepository,
};
use nineprofs_db::{Database, DbError, SqliteMetadataRepository};
use nineprofs_document_tools::{DocumentProposalWorkflowService, DocumentToolProvider};
use nineprofs_documents::{
    DocumentBridgeError, DocumentBridgeService, DocumentChangeSet, DocumentInspection,
    DocumentMutationResult,
};
use nineprofs_mcp::{McpError, McpService, SqliteMcpServerRepository};
use nineprofs_officecli::{
    ArtifactResolver, OfficeCliConfig, OfficeCliRunner, OfficeCliStatus, OfficeCliToolProvider,
};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_research::{ResearchArtifactStore, ResearchService, SqliteResearchRepository};
use nineprofs_research_assessor::{
    CitationAssessorConfig, CitationAssessorReadiness, ModelCitationAssessor,
};
use nineprofs_research_dify::{DifyConfig, DifyResearchService};
use nineprofs_research_verification::{CitationAssessmentProvider, CitationVerificationService};
use nineprofs_skills::{SkillCatalog, SkillError};
use nineprofs_tools::ToolRegistry;
use thiserror::Error;

mod docs_conversation;
mod docs_profile;
mod execution;

pub use docs_conversation::{
    DocsAgentConversationMetadata, DocsAgentConversationSeed, DocsAgentConversationState,
    DocsAgentConversationStoreError, DocsAgentConversationTurn, MAX_DOCS_AGENT_CONVERSATIONS,
    MAX_DOCS_AGENT_TURNS, MAX_IDLE_DOCS_AGENT_CONVERSATIONS,
};
pub use docs_profile::{DEFAULT_DOCS_ASSISTANT_ID, REQUIRED_DOCS_AGENT_TOOLS};
pub use execution::{
    AgentExecutionService, AgentExecutionServiceError, AgentRunStarted, build_system_instructions,
};

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub bind_addr: SocketAddr,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub event_capacity: usize,
    pub custom_skill_roots: Vec<PathBuf>,
    /// Reserved launch-scoped secret. Authentication is intentionally not enabled in Phase 1A.
    pub session_secret: Option<Arc<str>>,
    /// Launch-scoped Dify credentials. Never persisted or exposed through DTOs.
    pub dify: Option<DifyConfig>,
    /// Launch-scoped citation assessor configuration. Credential value is never stored.
    pub citation_assessor: CitationAssessorConfig,
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
            dify: None,
            citation_assessor: CitationAssessorConfig::default(),
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
        config.dify = DifyConfig::from_env();
        config.citation_assessor = CitationAssessorConfig::from_env();
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
    #[error(transparent)]
    AgentRegistry(#[from] AgentRegistryError),
    #[error(transparent)]
    Mcp(#[from] McpError),
    #[error(transparent)]
    Dify(#[from] nineprofs_research_dify::DifyError),
    #[error("tool registry initialization failed: {0}")]
    ToolRegistry(String),
}

pub struct CoreRuntime {
    config: RuntimeConfig,
    database: Database,
    metadata_repository: SqliteMetadataRepository,
    event_bus: Arc<BroadcastEventBus>,
    document_bridge: Arc<DocumentBridgeService>,
    document_tools: Arc<DocumentToolProvider>,
    document_workflow: Arc<DocumentProposalWorkflowService>,
    skill_catalog: Arc<SkillCatalog>,
    assistant_service: Arc<AssistantService>,
    agent_registry: Arc<AgentRegistry>,
    task_manager: AgentTaskManager,
    execution_service: Arc<AgentExecutionService>,
    tool_registry: ToolRegistry,
    mcp_service: Arc<McpService>,
    officecli_runner: Arc<OfficeCliRunner>,
    research_service: Arc<ResearchService>,
    dify_service: Arc<DifyResearchService>,
    citation_verification_service: Arc<CitationVerificationService>,
}

impl CoreRuntime {
    pub async fn initialize(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        let database = Database::open(&config.database_path).await?;
        Self::from_database(config, database).await
    }

    pub async fn initialize_in_memory(config: RuntimeConfig) -> Result<Self, RuntimeError> {
        let database = Database::in_memory().await?;
        Self::from_database(config, database).await
    }

    async fn from_database(
        config: RuntimeConfig,
        database: Database,
    ) -> Result<Self, RuntimeError> {
        let metadata_repository = database.metadata_repository();
        let event_bus = Arc::new(BroadcastEventBus::new(config.event_capacity));
        let artifact_store = Arc::new(ResearchArtifactStore::new(
            config.data_dir.join("research-artifacts"),
            database.pool().clone(),
        ));
        let research_service = Arc::new(
            ResearchService::new(
                SqliteResearchRepository::new(database.pool().clone()),
                Arc::clone(&event_bus),
            )
            .with_artifact_store(artifact_store),
        );
        let dify_service = Arc::new(DifyResearchService::new(
            database.pool().clone(),
            Arc::clone(&research_service),
            Arc::clone(&event_bus),
            config.dify.clone(),
        )?);
        let mut citation_verification_service = CitationVerificationService::new(
            database.pool().clone(),
            Arc::clone(&research_service),
            Arc::clone(&dify_service),
            Arc::clone(&event_bus),
        );
        if config.citation_assessor.is_ready() {
            let assessor: Arc<dyn CitationAssessmentProvider> =
                Arc::new(ModelCitationAssessor::new(config.citation_assessor.clone()));
            citation_verification_service = citation_verification_service.with_assessor(assessor);
        }
        let citation_verification_service = Arc::new(citation_verification_service);
        let document_bridge = Arc::new(DocumentBridgeService::new(
            nineprofs_documents::DocumentBridgeConfig {
                session_secret: config.session_secret.clone(),
                ..Default::default()
            },
            Arc::clone(&event_bus),
        ));
        let document_tools = Arc::new(DocumentToolProvider::new(
            Arc::clone(&document_bridge),
            Arc::clone(&event_bus),
        ));
        let document_workflow = Arc::new(DocumentProposalWorkflowService::new(
            Arc::clone(&document_bridge),
            document_tools.proposal_store(),
            Arc::clone(&event_bus),
        ));
        let agent_registry = Arc::new(AgentRegistry::new(
            Arc::new(SqliteAgentMetadataRepository::new(database.pool().clone())),
            BuiltinAgentCatalog::load(),
            Arc::clone(&event_bus),
        ));
        agent_registry.hydrate().await?;
        let tool_registry = ToolRegistry::new();
        tool_registry
            .register_provider(document_tools.as_ref())
            .await
            .map_err(|error| RuntimeError::ToolRegistry(error.to_string()))?;
        let mcp_service = Arc::new(McpService::new(
            SqliteMcpServerRepository::new(database.pool().clone()),
            tool_registry.clone(),
            Arc::clone(&event_bus),
        ));
        let officecli_runner =
            Arc::new(OfficeCliRunner::initialize(OfficeCliConfig::from_env()).await);
        let officecli_provider = OfficeCliToolProvider::new(
            Arc::clone(&officecli_runner),
            Arc::new(ArtifactResolver::new([config.data_dir.clone()])),
        );
        if officecli_runner.is_available() {
            let _ = tool_registry.register_provider(&officecli_provider).await;
        }
        let aionrs_executor = Arc::new(AionRsExecutor::from_env_with_tools(tool_registry.clone()));
        let provider = aionrs_executor.provider().clone();
        let availability = match aionrs_executor.availability_reason() {
            Some(reason) => (AvailabilityState::Unavailable, Some(reason)),
            None => (AvailabilityState::Available, None),
        };
        agent_registry
            .set_availability("nineprofs-default", availability.0, availability.1)
            .await?;
        let skill_catalog = Arc::new(SkillCatalog::with_configured_roots(
            config.custom_skill_roots.clone(),
        )?);
        let assistant_service = Arc::new(AssistantService::new(
            SqliteAssistantRepository::new(database.pool().clone()),
            BuiltinAssistantCatalog::load().map_err(AssistantError::from)?,
            Arc::clone(&skill_catalog),
            Arc::clone(&event_bus),
        )?);
        let task_manager = AgentTaskManager::new(Arc::clone(&event_bus));
        let executor: Arc<dyn AgentExecutor> = aionrs_executor;
        let executor_registry = AgentExecutorRegistry::new([executor]);
        let execution_service = Arc::new(AgentExecutionService::new(
            Arc::clone(&assistant_service),
            Arc::clone(&skill_catalog),
            Arc::clone(&agent_registry),
            executor_registry,
            task_manager.clone(),
            Arc::clone(&event_bus),
            provider,
            Arc::clone(&document_bridge),
            tool_registry.clone(),
        ));
        Ok(Self {
            config,
            database,
            metadata_repository,
            event_bus,
            document_bridge,
            document_tools,
            document_workflow,
            skill_catalog,
            assistant_service,
            agent_registry,
            task_manager,
            execution_service,
            tool_registry,
            mcp_service,
            officecli_runner,
            research_service,
            dify_service,
            citation_verification_service,
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

    pub fn document_bridge(&self) -> Arc<DocumentBridgeService> {
        Arc::clone(&self.document_bridge)
    }

    pub fn document_tools(&self) -> Arc<DocumentToolProvider> {
        Arc::clone(&self.document_tools)
    }

    pub fn document_workflow(&self) -> Arc<DocumentProposalWorkflowService> {
        Arc::clone(&self.document_workflow)
    }

    pub async fn inspect_active_document(
        &self,
        document_id: &str,
    ) -> Result<DocumentInspection, DocumentBridgeError> {
        self.document_bridge
            .inspect_active_document(document_id)
            .await
    }

    pub async fn commit_approved_change_set(
        &self,
        change_set: DocumentChangeSet,
    ) -> Result<DocumentMutationResult, DocumentBridgeError> {
        self.document_bridge
            .commit_approved_change_set(change_set)
            .await
    }

    pub fn skill_catalog(&self) -> Arc<SkillCatalog> {
        Arc::clone(&self.skill_catalog)
    }

    pub fn assistant_service(&self) -> &AssistantService {
        self.assistant_service.as_ref()
    }

    pub fn assistant_service_arc(&self) -> Arc<AssistantService> {
        Arc::clone(&self.assistant_service)
    }

    pub fn agent_registry(&self) -> Arc<AgentRegistry> {
        Arc::clone(&self.agent_registry)
    }

    pub fn task_manager(&self) -> AgentTaskManager {
        self.task_manager.clone()
    }

    pub fn execution_service(&self) -> Arc<AgentExecutionService> {
        Arc::clone(&self.execution_service)
    }

    pub fn tool_registry(&self) -> ToolRegistry {
        self.tool_registry.clone()
    }

    pub fn mcp_service(&self) -> Arc<McpService> {
        Arc::clone(&self.mcp_service)
    }

    pub fn officecli_status(&self) -> OfficeCliStatus {
        self.officecli_runner.status()
    }

    pub fn research_service(&self) -> Arc<ResearchService> {
        Arc::clone(&self.research_service)
    }

    pub fn dify_service(&self) -> Arc<DifyResearchService> {
        Arc::clone(&self.dify_service)
    }

    pub fn citation_verification_service(&self) -> Arc<CitationVerificationService> {
        Arc::clone(&self.citation_verification_service)
    }

    pub fn citation_assessor_readiness(&self) -> CitationAssessorReadiness {
        self.config.citation_assessor.readiness()
    }

    pub async fn resolve_assistant_backend(
        &self,
        assistant_id: &str,
    ) -> Result<BackendResolution, RuntimeError> {
        let assistant = self.assistant_service.get(assistant_id).await?;
        Ok(self
            .agent_registry
            .resolve_assistant_backend(assistant.backend_agent_id.as_deref())
            .await)
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
                "documents".to_owned(),
                "agents".to_owned(),
                "assistants".to_owned(),
                "skills".to_owned(),
                "agent-execution".to_owned(),
                "mcp-tools".to_owned(),
                "officecli-tools".to_owned(),
                "research".to_owned(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nineprofs_tools::{ToolId, ToolSet};

    #[tokio::test]
    async fn composition_root_constructs_shared_infrastructure() {
        let runtime = CoreRuntime::initialize_in_memory(RuntimeConfig::default())
            .await
            .unwrap();

        assert_eq!(runtime.health().status, "ok");
        assert_eq!(runtime.info().service, "9profs-core");
        assert_eq!(runtime.event_bus().receiver_count(), 0);
        assert!(!runtime.config().session_secret_configured());
        assert!(matches!(
            runtime.citation_assessor_readiness().status,
            nineprofs_research_assessor::CitationAssessorReadinessStatus::NotConfigured
        ));
    }

    #[tokio::test]
    async fn active_document_tools_are_registered_but_default_runs_receive_none() {
        let runtime = CoreRuntime::initialize_in_memory(RuntimeConfig::default())
            .await
            .unwrap();
        let document_tools: Vec<_> = runtime
            .tool_registry()
            .list_definitions()
            .into_iter()
            .filter(|definition| definition.name.starts_with("document."))
            .collect();
        assert_eq!(
            document_tools
                .iter()
                .map(|definition| definition.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "document.inspect_active",
                "document.list_active",
                "document.propose_active_changes"
            ]
        );
        assert!(
            runtime
                .tool_registry()
                .registrations_for(&ToolSet::default())
                .unwrap()
                .is_empty()
        );
        let explicit = runtime
            .tool_registry()
            .registrations_for(&ToolSet::from_ids([
                ToolId::new("document.list_active"),
                ToolId::new("document.inspect_active"),
                ToolId::new("document.propose_active_changes"),
            ]))
            .unwrap();
        assert_eq!(explicit.len(), 3);
        assert!(
            document_tools
                .iter()
                .all(|definition| !definition.name.contains("commit"))
        );
    }

    #[tokio::test]
    async fn default_backend_availability_matches_provider_configuration() {
        let runtime = CoreRuntime::initialize_in_memory(RuntimeConfig::default())
            .await
            .unwrap();
        let descriptor = runtime
            .agent_registry()
            .get("nineprofs-default")
            .await
            .unwrap();
        let reason = AionRsExecutor::from_env().availability_reason();

        match reason {
            Some(reason) => {
                assert_eq!(descriptor.availability, AvailabilityState::Unavailable);
                assert_eq!(
                    descriptor.availability_reason.as_deref(),
                    Some(reason.as_str())
                );
            }
            None => {
                assert_eq!(descriptor.availability, AvailabilityState::Available);
                assert_eq!(descriptor.availability_reason, None);
            }
        }
    }

    #[tokio::test]
    async fn assistant_backend_resolution_preserves_missing_and_disabled_states() {
        let runtime = CoreRuntime::initialize_in_memory(RuntimeConfig::default())
            .await
            .unwrap();
        runtime
            .assistant_service()
            .create(nineprofs_assistant::CreateAssistant {
                id: Some("backend-assistant".to_owned()),
                name: "Backend assistant".to_owned(),
                description: "Resolution test".to_owned(),
                backend_agent_id: Some("codex".to_owned()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert!(matches!(
            runtime
                .resolve_assistant_backend("backend-assistant")
                .await
                .unwrap(),
            BackendResolution::Unknown { .. }
        ));
        runtime
            .agent_registry()
            .set_availability(
                "codex",
                nineprofs_agent::AvailabilityState::Disabled,
                Some("disabled for test".to_owned()),
            )
            .await
            .unwrap();
        assert!(matches!(
            runtime
                .resolve_assistant_backend("backend-assistant")
                .await
                .unwrap(),
            BackendResolution::Disabled { .. }
        ));

        runtime
            .assistant_service()
            .update(
                "backend-assistant",
                nineprofs_assistant::UpdateAssistant {
                    backend_agent_id: Some(Some("missing".to_owned())),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            runtime
                .resolve_assistant_backend("backend-assistant")
                .await
                .unwrap(),
            BackendResolution::Missing { id } if id == "missing"
        ));
    }
}
