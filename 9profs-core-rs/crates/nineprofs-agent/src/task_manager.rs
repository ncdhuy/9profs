use std::{collections::HashMap, sync::Arc};

use nineprofs_realtime::BroadcastEventBus;
use serde_json::json;
use thiserror::Error;
use tokio::sync::{RwLock, watch};

use crate::{AgentTask, AgentTaskId, RunId, TaskFailure, TaskState};

struct ManagedTask {
    task: AgentTask,
    cancellation: watch::Sender<bool>,
}

#[derive(Clone)]
pub struct AgentTaskManager {
    tasks: Arc<RwLock<HashMap<AgentTaskId, ManagedTask>>>,
    events: Arc<BroadcastEventBus>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AgentTaskManagerError {
    #[error("agent task `{0}` already exists")]
    DuplicateTask(AgentTaskId),
    #[error("agent task `{0}` was not found")]
    NotFound(AgentTaskId),
    #[error("agent task `{0}` is still active")]
    ActiveTask(AgentTaskId),
    #[error("{0}")]
    InvalidTransition(String),
}

impl AgentTaskManager {
    pub fn new(events: Arc<BroadcastEventBus>) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            events,
        }
    }

    pub async fn register(
        &self,
        task_id: AgentTaskId,
        run_id: RunId,
        backend_id: impl Into<String>,
    ) -> Result<AgentTask, AgentTaskManagerError> {
        let mut tasks = self.tasks.write().await;
        if tasks.contains_key(&task_id) {
            return Err(AgentTaskManagerError::DuplicateTask(task_id));
        }
        let task = AgentTask::new(task_id.clone(), run_id, backend_id.into());
        let (cancellation, _) = watch::channel(false);
        tasks.insert(
            task_id,
            ManagedTask {
                task: task.clone(),
                cancellation,
            },
        );
        drop(tasks);
        self.publish("agent.taskQueued", &task);
        Ok(task)
    }

    pub async fn register_new(
        &self,
        run_id: RunId,
        backend_id: impl Into<String>,
    ) -> Result<AgentTask, AgentTaskManagerError> {
        self.register(AgentTaskId::new(), run_id, backend_id).await
    }

    pub async fn cancellation(
        &self,
        task_id: &AgentTaskId,
    ) -> Result<watch::Receiver<bool>, AgentTaskManagerError> {
        self.tasks
            .read()
            .await
            .get(task_id)
            .map(|managed| managed.cancellation.subscribe())
            .ok_or_else(|| AgentTaskManagerError::NotFound(task_id.clone()))
    }

    pub async fn get(&self, task_id: &AgentTaskId) -> Option<AgentTask> {
        self.tasks
            .read()
            .await
            .get(task_id)
            .map(|managed| managed.task.clone())
    }

    pub async fn list_for_run(&self, run_id: &RunId) -> Vec<AgentTask> {
        let mut tasks: Vec<_> = self
            .tasks
            .read()
            .await
            .values()
            .filter(|managed| &managed.task.run_id == run_id)
            .map(|managed| managed.task.clone())
            .collect();
        tasks.sort_by(|left, right| {
            left.created_at_ms
                .cmp(&right.created_at_ms)
                .then_with(|| left.task_id.cmp(&right.task_id))
        });
        tasks
    }

    pub async fn start(&self, task_id: &AgentTaskId) -> Result<AgentTask, AgentTaskManagerError> {
        self.transition(
            task_id,
            TaskState::Starting,
            None,
            Some("agent.taskStarted"),
        )
        .await
    }

    pub async fn mark_running(
        &self,
        task_id: &AgentTaskId,
    ) -> Result<AgentTask, AgentTaskManagerError> {
        self.transition(task_id, TaskState::Running, None, None)
            .await
    }

    pub async fn complete(
        &self,
        task_id: &AgentTaskId,
    ) -> Result<AgentTask, AgentTaskManagerError> {
        self.transition(
            task_id,
            TaskState::Succeeded,
            None,
            Some("agent.taskCompleted"),
        )
        .await
    }

    pub async fn fail(
        &self,
        task_id: &AgentTaskId,
        failure: TaskFailure,
    ) -> Result<AgentTask, AgentTaskManagerError> {
        self.transition(
            task_id,
            TaskState::Failed,
            Some(failure),
            Some("agent.taskFailed"),
        )
        .await
    }

    pub async fn cancel(&self, task_id: &AgentTaskId) -> Result<AgentTask, AgentTaskManagerError> {
        let mut tasks = self.tasks.write().await;
        let managed = tasks
            .get_mut(task_id)
            .ok_or_else(|| AgentTaskManagerError::NotFound(task_id.clone()))?;
        managed
            .task
            .transition(TaskState::Cancelled)
            .map_err(|error| AgentTaskManagerError::InvalidTransition(error.to_string()))?;
        let _ = managed.cancellation.send(true);
        let task = managed.task.clone();
        drop(tasks);
        self.publish("agent.taskCancelled", &task);
        Ok(task)
    }

    pub async fn remove(&self, task_id: &AgentTaskId) -> Result<AgentTask, AgentTaskManagerError> {
        let mut tasks = self.tasks.write().await;
        let Some(managed) = tasks.get(task_id) else {
            return Err(AgentTaskManagerError::NotFound(task_id.clone()));
        };
        if !managed.task.state.is_terminal() {
            return Err(AgentTaskManagerError::ActiveTask(task_id.clone()));
        }
        Ok(tasks.remove(task_id).expect("task checked above").task)
    }

    pub async fn cleanup_terminal(&self) -> usize {
        let mut tasks = self.tasks.write().await;
        let before = tasks.len();
        tasks.retain(|_, managed| !managed.task.state.is_terminal());
        before - tasks.len()
    }

    async fn transition(
        &self,
        task_id: &AgentTaskId,
        state: TaskState,
        failure: Option<TaskFailure>,
        event_name: Option<&'static str>,
    ) -> Result<AgentTask, AgentTaskManagerError> {
        let mut tasks = self.tasks.write().await;
        let managed = tasks
            .get_mut(task_id)
            .ok_or_else(|| AgentTaskManagerError::NotFound(task_id.clone()))?;
        managed
            .task
            .transition(state)
            .map_err(|error| AgentTaskManagerError::InvalidTransition(error.to_string()))?;
        managed.task.failure = failure;
        let task = managed.task.clone();
        drop(tasks);
        if let Some(event_name) = event_name {
            self.publish(event_name, &task);
        }
        Ok(task)
    }

    fn publish(&self, name: &str, task: &AgentTask) {
        let _ = self.events.publish(nineprofs_api_types::EventEnvelope::new(
            name,
            json!({ "task": task }),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nineprofs_realtime::BroadcastEventBus;

    struct FakeExecutor {
        manager: AgentTaskManager,
        task_id: AgentTaskId,
    }

    impl FakeExecutor {
        async fn complete(self) {
            self.manager.start(&self.task_id).await.unwrap();
            self.manager.mark_running(&self.task_id).await.unwrap();
            self.manager.complete(&self.task_id).await.unwrap();
        }

        async fn fail(self) {
            self.manager.start(&self.task_id).await.unwrap();
            self.manager.mark_running(&self.task_id).await.unwrap();
            self.manager
                .fail(
                    &self.task_id,
                    TaskFailure {
                        code: "fake_failure".to_owned(),
                        message: "fake executor failed".to_owned(),
                    },
                )
                .await
                .unwrap();
        }

        async fn cancel(self) {
            self.manager.start(&self.task_id).await.unwrap();
            self.manager.mark_running(&self.task_id).await.unwrap();
            let mut cancellation = self.manager.cancellation(&self.task_id).await.unwrap();
            self.manager.cancel(&self.task_id).await.unwrap();
            cancellation.changed().await.unwrap();
            assert!(*cancellation.borrow());
        }
    }

    fn manager() -> AgentTaskManager {
        AgentTaskManager::new(Arc::new(BroadcastEventBus::new(32)))
    }

    #[tokio::test]
    async fn complete_failure_and_cancellation_paths_work() {
        let manager = manager();
        let complete = manager.register_new(RunId::new(), "codex").await.unwrap();
        FakeExecutor {
            manager: manager.clone(),
            task_id: complete.task_id.clone(),
        }
        .complete()
        .await;
        assert_eq!(
            manager.get(&complete.task_id).await.unwrap().state,
            TaskState::Succeeded
        );

        let failed = manager.register_new(RunId::new(), "codex").await.unwrap();
        FakeExecutor {
            manager: manager.clone(),
            task_id: failed.task_id.clone(),
        }
        .fail()
        .await;
        assert_eq!(
            manager.get(&failed.task_id).await.unwrap().state,
            TaskState::Failed
        );

        let cancelled = manager.register_new(RunId::new(), "codex").await.unwrap();
        FakeExecutor {
            manager: manager.clone(),
            task_id: cancelled.task_id.clone(),
        }
        .cancel()
        .await;
        assert_eq!(
            manager.get(&cancelled.task_id).await.unwrap().state,
            TaskState::Cancelled
        );
    }

    #[tokio::test]
    async fn transitions_are_deterministic_and_terminal_states_are_immutable() {
        let manager = manager();
        let task = manager.register_new(RunId::new(), "codex").await.unwrap();
        assert!(manager.complete(&task.task_id).await.is_err());
        manager.start(&task.task_id).await.unwrap();
        manager.mark_running(&task.task_id).await.unwrap();
        manager.complete(&task.task_id).await.unwrap();
        assert!(manager.mark_running(&task.task_id).await.is_err());
        assert_eq!(
            manager.remove(&task.task_id).await.unwrap().state,
            TaskState::Succeeded
        );
        assert_eq!(manager.cleanup_terminal().await, 0);
    }

    #[tokio::test]
    async fn duplicate_ids_and_parallel_tasks_per_run_are_handled() {
        let manager = manager();
        let run_id = RunId::new();
        let first = manager
            .register(AgentTaskId::from_string("task-1"), run_id.clone(), "codex")
            .await
            .unwrap();
        let second = manager
            .register_new(run_id.clone(), "claude")
            .await
            .unwrap();
        assert!(matches!(
            manager
                .register(AgentTaskId::from_string("task-1"), run_id.clone(), "codex")
                .await,
            Err(AgentTaskManagerError::DuplicateTask(_))
        ));
        let tasks = manager.list_for_run(&run_id).await;
        assert_eq!(tasks.len(), 2);
        assert_ne!(first.task_id, second.task_id);
    }

    #[tokio::test]
    async fn concurrent_status_reads_are_safe() {
        let manager = manager();
        let task = manager.register_new(RunId::new(), "codex").await.unwrap();
        let mut readers = Vec::new();
        for _ in 0..8 {
            let manager = manager.clone();
            let task_id = task.task_id.clone();
            readers.push(tokio::spawn(async move {
                for _ in 0..16 {
                    assert_eq!(
                        manager.get(&task_id).await.unwrap().state,
                        TaskState::Queued
                    );
                }
            }));
        }
        for reader in readers {
            reader.await.unwrap();
        }
    }

    #[tokio::test]
    async fn lifecycle_events_are_emitted_once_per_public_transition() {
        let manager = manager();
        let mut events = manager.events.subscribe();
        let task = manager.register_new(RunId::new(), "codex").await.unwrap();
        manager.start(&task.task_id).await.unwrap();
        manager.mark_running(&task.task_id).await.unwrap();
        manager.complete(&task.task_id).await.unwrap();
        let names = [
            events.recv().await.unwrap().name,
            events.recv().await.unwrap().name,
            events.recv().await.unwrap().name,
        ];
        assert_eq!(
            names,
            [
                "agent.taskQueued",
                "agent.taskStarted",
                "agent.taskCompleted"
            ]
        );
    }
}
