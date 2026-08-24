use std::sync::Arc;

use nineprofs_agent::{
    AgentExecutionError, AgentExecutionEvent, AgentExecutionRequest, AgentExecutorRegistry,
    AgentProviderConfig, AgentTask, AgentTaskManager, BackendResolution, ExecutionLimits, RunId,
    TaskFailure,
};
use nineprofs_api_types::EventEnvelope;
use nineprofs_assistant::{AssistantError, AssistantService};
use nineprofs_realtime::BroadcastEventBus;
use nineprofs_skills::{Skill, SkillCatalog};
use serde_json::json;
use thiserror::Error;
use tokio::sync::{mpsc, watch};

#[derive(Debug, Error)]
pub enum AgentExecutionServiceError {
    #[error(transparent)]
    Assistant(#[from] AssistantError),
    #[error("agent run input must not be empty")]
    EmptyInput,
    #[error("assistant is disabled: {0}")]
    AssistantDisabled(String),
    #[error("assistant has no configured agent backend")]
    BackendNotConfigured,
    #[error("agent backend `{0}` was not found")]
    BackendMissing(String),
    #[error("agent backend `{0}` is not available: {1}")]
    BackendUnavailable(String, String),
    #[error("agent backend `{0}` is disabled")]
    BackendDisabled(String),
    #[error("agent backend `{0}` has no executor")]
    ExecutorMissing(String),
    #[error("skill is missing: {0}")]
    MissingSkill(String),
    #[error(transparent)]
    Task(#[from] nineprofs_agent::AgentTaskManagerError),
}

#[derive(Clone)]
pub struct AgentExecutionService {
    assistants: Arc<AssistantService>,
    skills: Arc<SkillCatalog>,
    registry: Arc<nineprofs_agent::AgentRegistry>,
    executors: AgentExecutorRegistry,
    tasks: AgentTaskManager,
    events: Arc<BroadcastEventBus>,
    provider: AgentProviderConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRunStarted {
    pub run_id: RunId,
    pub task: AgentTask,
}

impl AgentExecutionService {
    pub fn new(
        assistants: Arc<AssistantService>,
        skills: Arc<SkillCatalog>,
        registry: Arc<nineprofs_agent::AgentRegistry>,
        executors: AgentExecutorRegistry,
        tasks: AgentTaskManager,
        events: Arc<BroadcastEventBus>,
        provider: AgentProviderConfig,
    ) -> Self {
        Self {
            assistants,
            skills,
            registry,
            executors,
            tasks,
            events,
            provider,
        }
    }

    pub async fn start_run(
        &self,
        assistant_id: &str,
        input: &str,
    ) -> Result<AgentRunStarted, AgentExecutionServiceError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AgentExecutionServiceError::EmptyInput);
        }
        let assistant = self.assistants.get(assistant_id).await?;
        if !assistant.enabled {
            return Err(AgentExecutionServiceError::AssistantDisabled(assistant.id));
        }

        let descriptor = match self
            .registry
            .resolve_assistant_backend(assistant.backend_agent_id.as_deref())
            .await
        {
            BackendResolution::NotConfigured => {
                return Err(AgentExecutionServiceError::BackendNotConfigured);
            }
            BackendResolution::Missing { id } => {
                return Err(AgentExecutionServiceError::BackendMissing(id));
            }
            BackendResolution::Unknown { descriptor }
            | BackendResolution::Unavailable { descriptor } => {
                return Err(AgentExecutionServiceError::BackendUnavailable(
                    descriptor.id,
                    descriptor
                        .availability_reason
                        .unwrap_or_else(|| "availability is unknown".to_owned()),
                ));
            }
            BackendResolution::Disabled { descriptor } => {
                return Err(AgentExecutionServiceError::BackendDisabled(descriptor.id));
            }
            BackendResolution::Resolved { descriptor } => descriptor,
        };
        let executor = self
            .executors
            .get(&descriptor.id)
            .ok_or_else(|| AgentExecutionServiceError::ExecutorMissing(descriptor.id.clone()))?;

        let mut resolved_skills = Vec::with_capacity(assistant.skill_ids.len());
        for skill_id in &assistant.skill_ids {
            resolved_skills.push(
                self.skills
                    .resolve(skill_id)
                    .ok_or_else(|| AgentExecutionServiceError::MissingSkill(skill_id.clone()))?,
            );
        }

        let run_id = RunId::new();
        let task = self
            .tasks
            .register_new(run_id.clone(), descriptor.id.clone())
            .await?;
        let cancellation = self.tasks.cancellation(&task.task_id).await?;
        let request = AgentExecutionRequest {
            run_id: run_id.clone(),
            task_id: task.task_id.clone(),
            backend_id: descriptor.id,
            assistant_id: assistant.id,
            input: input.to_owned(),
            workspace_root: None,
            provider: self.provider.clone(),
            system_instructions: build_system_instructions(&assistant.rules, &resolved_skills),
            limits: ExecutionLimits::default(),
        };

        let tasks = self.tasks.clone();
        let events = Arc::clone(&self.events);
        tokio::spawn(async move {
            run_task(tasks, events, executor, request, cancellation).await;
        });

        Ok(AgentRunStarted { run_id, task })
    }

    pub async fn task(&self, task_id: &nineprofs_agent::AgentTaskId) -> Option<AgentTask> {
        self.tasks.get(task_id).await
    }

    pub async fn tasks_for_run(&self, run_id: &RunId) -> Vec<AgentTask> {
        self.tasks.list_for_run(run_id).await
    }

    pub async fn cancel(
        &self,
        task_id: &nineprofs_agent::AgentTaskId,
    ) -> Result<AgentTask, nineprofs_agent::AgentTaskManagerError> {
        self.tasks.cancel(task_id).await
    }
}

pub fn build_system_instructions(rules: &str, skills: &[Skill]) -> String {
    let mut sections = Vec::new();
    if !rules.trim().is_empty() {
        sections.push(format!("[Assistant Rules]\n{}", rules.trim()));
    }
    for skill in skills {
        sections.push(format!("[Skill: {}]\n{}", skill.id, skill.content.trim()));
    }
    sections.join("\n\n")
}

async fn run_task(
    tasks: AgentTaskManager,
    events: Arc<BroadcastEventBus>,
    executor: Arc<dyn nineprofs_agent::AgentExecutor>,
    request: AgentExecutionRequest,
    cancellation: watch::Receiver<bool>,
) {
    if *cancellation.borrow() || tasks.start(&request.task_id).await.is_err() {
        return;
    }
    if *cancellation.borrow() {
        let _ = tasks.cancel(&request.task_id).await;
        return;
    }
    if tasks.mark_running(&request.task_id).await.is_err() {
        return;
    }

    let (event_sink, mut event_receiver) = mpsc::unbounded_channel();
    let task_id = request.task_id.clone();
    let run_id = request.run_id.clone();
    let cancellation_state = cancellation.clone();
    let execution =
        tokio::spawn(async move { executor.execute(request, event_sink, cancellation).await });
    tokio::pin!(execution);

    let mut event_state = ExecutionEventState::default();
    let result = loop {
        tokio::select! {
            event = event_receiver.recv() => {
                if let Some(event) = event {
                    if !*cancellation_state.borrow() {
                        publish_execution_event(&events, &run_id, &task_id, event, &mut event_state);
                    }
                }
            }
            result = &mut execution => break result,
        }
    };
    while let Ok(event) = event_receiver.try_recv() {
        if !*cancellation_state.borrow() {
            publish_execution_event(&events, &run_id, &task_id, event, &mut event_state);
        }
    }

    let outcome = match result {
        Ok(outcome) => outcome,
        Err(_) => Err(AgentExecutionError::Failed),
    };
    match outcome {
        Ok(_) => {
            if !is_terminal(&tasks, &task_id).await {
                let _ = tasks.complete(&task_id).await;
            }
        }
        Err(AgentExecutionError::Cancelled) => {
            if !is_terminal(&tasks, &task_id).await {
                let _ = tasks.cancel(&task_id).await;
            }
        }
        Err(error) => {
            if *cancellation_state.borrow() {
                if !is_terminal(&tasks, &task_id).await {
                    let _ = tasks.cancel(&task_id).await;
                }
            } else {
                if !event_state.error_emitted {
                    publish_execution_event(
                        &events,
                        &run_id,
                        &task_id,
                        AgentExecutionEvent::Error {
                            code: "execution_failed".to_owned(),
                            message: error.to_string(),
                        },
                        &mut event_state,
                    );
                }
                if !is_terminal(&tasks, &task_id).await {
                    let _ = tasks
                        .fail(
                            &task_id,
                            TaskFailure {
                                code: "execution_failed".to_owned(),
                                message: error.to_string(),
                            },
                        )
                        .await;
                }
            }
        }
    }
}

#[derive(Default)]
struct ExecutionEventState {
    output_started: bool,
    output_completed: bool,
    error_emitted: bool,
}

async fn is_terminal(tasks: &AgentTaskManager, task_id: &nineprofs_agent::AgentTaskId) -> bool {
    tasks
        .get(task_id)
        .await
        .is_some_and(|task| task.state.is_terminal())
}

fn publish_execution_event(
    events: &BroadcastEventBus,
    run_id: &RunId,
    task_id: &nineprofs_agent::AgentTaskId,
    event: AgentExecutionEvent,
    state: &mut ExecutionEventState,
) {
    let (name, details) = match event {
        AgentExecutionEvent::OutputStarted => {
            if state.output_started || state.output_completed {
                return;
            }
            state.output_started = true;
            ("agent.outputStarted", json!({}))
        }
        AgentExecutionEvent::OutputDelta { delta } => {
            if delta.is_empty() || state.output_completed {
                return;
            }
            ("agent.outputDelta", json!({ "delta": delta }))
        }
        AgentExecutionEvent::OutputCompleted { output } => {
            if state.output_completed {
                return;
            }
            state.output_completed = true;
            ("agent.outputCompleted", json!({ "output": output }))
        }
        AgentExecutionEvent::Error { code, message } => {
            if state.error_emitted {
                return;
            }
            state.error_emitted = true;
            ("agent.error", json!({ "code": code, "message": message }))
        }
    };
    let _ = events.publish(EventEnvelope::new(
        name,
        json!({ "run_id": run_id, "task_id": task_id, "details": details }),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use nineprofs_agent::{
        AgentEventSink, AgentExecutionResult, AgentExecutor, AgentRegistry, AvailabilityState,
        BuiltinAgentCatalog, SqliteAgentMetadataRepository, TaskState,
    };
    use nineprofs_assistant::{
        BuiltinAssistantCatalog, CreateAssistant, SqliteAssistantRepository,
    };
    use nineprofs_db::Database;
    use nineprofs_realtime::BroadcastEventBus;
    use std::sync::Arc;

    struct FakeExecutor {
        backend_id: &'static str,
    }

    #[async_trait]
    impl AgentExecutor for FakeExecutor {
        fn backend_id(&self) -> &str {
            self.backend_id
        }

        async fn execute(
            &self,
            _request: AgentExecutionRequest,
            sink: AgentEventSink,
            _cancellation: watch::Receiver<bool>,
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            let _ = sink.send(AgentExecutionEvent::OutputStarted);
            let _ = sink.send(AgentExecutionEvent::OutputDelta {
                delta: "NINEPROFS_OK".to_owned(),
            });
            let _ = sink.send(AgentExecutionEvent::OutputCompleted {
                output: "NINEPROFS_OK".to_owned(),
            });
            Ok(AgentExecutionResult {
                output: "NINEPROFS_OK".to_owned(),
            })
        }
    }

    struct BlockingExecutor;

    #[async_trait]
    impl AgentExecutor for BlockingExecutor {
        fn backend_id(&self) -> &str {
            "blocking"
        }

        async fn execute(
            &self,
            _request: AgentExecutionRequest,
            sink: AgentEventSink,
            mut cancellation: watch::Receiver<bool>,
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            let _ = sink.send(AgentExecutionEvent::OutputStarted);
            let _ = sink.send(AgentExecutionEvent::OutputDelta {
                delta: "partial".to_owned(),
            });
            loop {
                cancellation
                    .changed()
                    .await
                    .map_err(|_| AgentExecutionError::Cancelled)?;
                if *cancellation.borrow() {
                    return Err(AgentExecutionError::Cancelled);
                }
            }
        }
    }

    struct LateSuccessExecutor;

    #[async_trait]
    impl AgentExecutor for LateSuccessExecutor {
        fn backend_id(&self) -> &str {
            "late-success"
        }

        async fn execute(
            &self,
            _request: AgentExecutionRequest,
            sink: AgentEventSink,
            mut cancellation: watch::Receiver<bool>,
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            cancellation
                .changed()
                .await
                .map_err(|_| AgentExecutionError::Cancelled)?;
            let _ = sink.send(AgentExecutionEvent::OutputCompleted {
                output: "late success".to_owned(),
            });
            Ok(AgentExecutionResult {
                output: "late success".to_owned(),
            })
        }
    }

    struct FailingExecutor;

    #[async_trait]
    impl AgentExecutor for FailingExecutor {
        fn backend_id(&self) -> &str {
            "failing"
        }

        async fn execute(
            &self,
            _request: AgentExecutionRequest,
            _sink: AgentEventSink,
            _cancellation: watch::Receiver<bool>,
        ) -> Result<AgentExecutionResult, AgentExecutionError> {
            Err(AgentExecutionError::Failed)
        }
    }

    fn test_request(task: &AgentTask) -> AgentExecutionRequest {
        AgentExecutionRequest {
            run_id: task.run_id.clone(),
            task_id: task.task_id.clone(),
            backend_id: task.backend_id.clone(),
            assistant_id: "test-assistant".to_owned(),
            input: "test input".to_owned(),
            workspace_root: None,
            provider: AgentProviderConfig::from_env(),
            system_instructions: "test instructions".to_owned(),
            limits: ExecutionLimits::default(),
        }
    }

    #[test]
    fn ordered_rules_and_skills_materialize_without_aionrs_discovery() {
        let executors = AgentExecutorRegistry::new([
            Arc::new(FakeExecutor { backend_id: "fake" }) as Arc<dyn AgentExecutor>
        ]);
        assert_eq!(executors.ids(), vec!["fake".to_owned()]);

        let skills = vec![
            Skill {
                id: "first".to_owned(),
                name: "First".to_owned(),
                description: "".to_owned(),
                source: nineprofs_skills::SkillSource::Builtin,
                location: nineprofs_skills::SkillLocation::Embedded {
                    path: "first".to_owned(),
                },
                content: "first content".to_owned(),
            },
            Skill {
                id: "second".to_owned(),
                name: "Second".to_owned(),
                description: "".to_owned(),
                source: nineprofs_skills::SkillSource::Builtin,
                location: nineprofs_skills::SkillLocation::Embedded {
                    path: "second".to_owned(),
                },
                content: "second content".to_owned(),
            },
        ];
        let instructions = build_system_instructions("Reply concisely.", &skills);
        assert!(
            instructions.find("first content").unwrap()
                < instructions.find("second content").unwrap()
        );
        assert!(instructions.contains("[Assistant Rules]"));
    }

    #[tokio::test]
    async fn execution_events_translate_to_transport_owned_names() {
        let events = BroadcastEventBus::new(16);
        let mut receiver = events.subscribe();
        let run_id = RunId::new();
        let task_id = nineprofs_agent::AgentTaskId::new();
        let mut state = ExecutionEventState::default();
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputStarted,
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputDelta {
                delta: String::new(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputDelta {
                delta: "delta".to_owned(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputCompleted {
                output: "complete".to_owned(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::OutputCompleted {
                output: "duplicate".to_owned(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::Error {
                code: "provider_error".to_owned(),
                message: "provider failed".to_owned(),
            },
            &mut state,
        );
        publish_execution_event(
            &events,
            &run_id,
            &task_id,
            AgentExecutionEvent::Error {
                code: "provider_error".to_owned(),
                message: "duplicate provider failure".to_owned(),
            },
            &mut state,
        );

        let names = [
            receiver.recv().await.unwrap().name,
            receiver.recv().await.unwrap().name,
            receiver.recv().await.unwrap().name,
            receiver.recv().await.unwrap().name,
        ];
        assert_eq!(
            names,
            [
                "agent.outputStarted".to_owned(),
                "agent.outputDelta".to_owned(),
                "agent.outputCompleted".to_owned(),
                "agent.error".to_owned(),
            ]
        );
    }

    async fn test_service(availability: AvailabilityState) -> AgentExecutionService {
        let database = Database::in_memory().await.unwrap();
        let events = Arc::new(BroadcastEventBus::new(64));
        let skills = Arc::new(
            SkillCatalog::with_configured_roots(Vec::<std::path::PathBuf>::new()).unwrap(),
        );
        let assistants = Arc::new(
            AssistantService::new(
                SqliteAssistantRepository::new(database.pool().clone()),
                BuiltinAssistantCatalog::load().unwrap(),
                Arc::clone(&skills),
                Arc::clone(&events),
            )
            .unwrap(),
        );
        assistants
            .create(CreateAssistant {
                id: Some("execution-assistant".to_owned()),
                name: "Execution assistant".to_owned(),
                description: "test assistant".to_owned(),
                backend_agent_id: Some("nineprofs-default".to_owned()),
                ..CreateAssistant::default()
            })
            .await
            .unwrap();

        let registry = Arc::new(AgentRegistry::new(
            Arc::new(SqliteAgentMetadataRepository::new(database.pool().clone())),
            BuiltinAgentCatalog::load(),
            Arc::clone(&events),
        ));
        registry.hydrate().await.unwrap();
        registry
            .set_availability("nineprofs-default", availability, None)
            .await
            .unwrap();

        let executor_registry = AgentExecutorRegistry::new([Arc::new(FakeExecutor {
            backend_id: "nineprofs-default",
        }) as Arc<dyn AgentExecutor>]);
        let tasks = AgentTaskManager::new(Arc::clone(&events));
        AgentExecutionService::new(
            assistants,
            skills,
            registry,
            executor_registry,
            tasks,
            events,
            AgentProviderConfig::from_env(),
        )
    }

    #[tokio::test]
    async fn execution_service_resolves_assistant_and_fake_executor() {
        let service = test_service(AvailabilityState::Available).await;
        let started = service
            .start_run("execution-assistant", "Reply with exactly: NINEPROFS_OK")
            .await
            .unwrap();

        for _ in 0..100 {
            if let Some(task) = service.task(&started.task.task_id).await {
                if task.state == TaskState::Succeeded {
                    return;
                }
            }
            tokio::task::yield_now().await;
        }
        panic!("fake execution did not reach succeeded state");
    }

    #[tokio::test]
    async fn execution_service_rejects_missing_assistant_and_unavailable_backend() {
        let service = test_service(AvailabilityState::Available).await;
        assert!(matches!(
            service.start_run("missing-assistant", "input").await,
            Err(AgentExecutionServiceError::Assistant(
                nineprofs_assistant::AssistantError::NotFound(_)
            ))
        ));

        let unavailable = test_service(AvailabilityState::Unavailable).await;
        assert!(matches!(
            unavailable
                .start_run("execution-assistant", "input")
                .await,
            Err(AgentExecutionServiceError::BackendUnavailable(id, _)) if id == "nineprofs-default"
        ));

        service
            .assistants
            .create(CreateAssistant {
                id: Some("missing-backend-assistant".to_owned()),
                name: "Missing backend assistant".to_owned(),
                description: "test assistant".to_owned(),
                backend_agent_id: Some("missing-backend".to_owned()),
                ..CreateAssistant::default()
            })
            .await
            .unwrap();
        assert!(matches!(
            service
                .start_run("missing-backend-assistant", "input")
                .await,
            Err(AgentExecutionServiceError::BackendMissing(id)) if id == "missing-backend"
        ));

        service
            .assistants
            .create(CreateAssistant {
                id: Some("unconfigured-assistant".to_owned()),
                name: "Unconfigured assistant".to_owned(),
                description: "test assistant".to_owned(),
                ..CreateAssistant::default()
            })
            .await
            .unwrap();
        assert!(matches!(
            service.start_run("unconfigured-assistant", "input").await,
            Err(AgentExecutionServiceError::BackendNotConfigured)
        ));
    }

    #[tokio::test]
    async fn run_task_maps_execution_failure_to_failed_terminal_state() {
        let events = Arc::new(BroadcastEventBus::new(32));
        let mut receiver = events.subscribe();
        let tasks = AgentTaskManager::new(Arc::clone(&events));
        let task = tasks.register_new(RunId::new(), "failing").await.unwrap();
        let cancellation = tasks.cancellation(&task.task_id).await.unwrap();

        run_task(
            tasks.clone(),
            events,
            Arc::new(FailingExecutor),
            test_request(&task),
            cancellation,
        )
        .await;

        let task = tasks.get(&task.task_id).await.unwrap();
        assert_eq!(task.state, TaskState::Failed);
        assert_eq!(task.failure.as_ref().unwrap().code, "execution_failed");
        assert_eq!(
            std::iter::from_fn(|| receiver.try_recv().ok())
                .filter(|event| event.name == "agent.error")
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn run_task_cancellation_stops_executor_and_leaves_terminal_task() {
        let events = Arc::new(BroadcastEventBus::new(32));
        let mut receiver = events.subscribe();
        let tasks = AgentTaskManager::new(Arc::clone(&events));
        let task = tasks.register_new(RunId::new(), "blocking").await.unwrap();
        let cancellation = tasks.cancellation(&task.task_id).await.unwrap();
        let task_id = task.task_id.clone();
        let execution = tokio::spawn(run_task(
            tasks.clone(),
            events,
            Arc::new(BlockingExecutor),
            test_request(&task),
            cancellation,
        ));

        for _ in 0..100 {
            if tasks.get(&task_id).await.unwrap().state == TaskState::Running {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(tasks.get(&task_id).await.unwrap().state, TaskState::Running);
        tasks.cancel(&task_id).await.unwrap();
        execution.await.unwrap();

        let task = tasks.get(&task_id).await.unwrap();
        assert_eq!(task.state, TaskState::Cancelled);
        assert!(task.state.is_terminal());
        assert!(
            !std::iter::from_fn(|| receiver.try_recv().ok())
                .any(|event| event.name == "agent.outputCompleted")
        );
    }

    #[tokio::test]
    async fn cancellation_wins_against_late_success_completion() {
        let events = Arc::new(BroadcastEventBus::new(32));
        let mut receiver = events.subscribe();
        let tasks = AgentTaskManager::new(Arc::clone(&events));
        let task = tasks
            .register_new(RunId::new(), "late-success")
            .await
            .unwrap();
        let task_id = task.task_id.clone();
        let cancellation = tasks.cancellation(&task_id).await.unwrap();
        let execution = tokio::spawn(run_task(
            tasks.clone(),
            events,
            Arc::new(LateSuccessExecutor),
            test_request(&task),
            cancellation,
        ));

        for _ in 0..100 {
            if tasks.get(&task_id).await.unwrap().state == TaskState::Running {
                break;
            }
            tokio::task::yield_now().await;
        }
        tasks.cancel(&task_id).await.unwrap();
        execution.await.unwrap();

        assert_eq!(
            tasks.get(&task_id).await.unwrap().state,
            TaskState::Cancelled
        );
        assert!(
            !std::iter::from_fn(|| receiver.try_recv().ok())
                .any(|event| event.name == "agent.outputCompleted")
        );
    }
}
