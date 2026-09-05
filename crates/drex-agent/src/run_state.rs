//! Agent Run State - Persistent tracking of agent execution
//!
//! This module provides structured tracking for agent runs, enabling:
//! - Long-running task persistence
//! - Recovery from failures
//! - Progress tracking
//! - Audit logging
//! - Parent/child relationships between runs
//!
//! # Run Lifecycle
//!
//! ```text
//! Pending -> Running -> { Completed | Failed | Cancelled }
//!    |          |
//!    +----------+-> Paused (optional)
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A unique identifier for an agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub Uuid);

impl RunId {
    /// Create a new run ID.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Create from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for RunId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<RunId> for Uuid {
    fn from(run_id: RunId) -> Self {
        run_id.0
    }
}

/// Status of an agent run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Run is pending and has not started yet.
    Pending,
    /// Run is currently executing.
    Running,
    /// Run is paused and can be resumed.
    Paused,
    /// Run completed successfully.
    Completed,
    /// Run failed with an error.
    Failed,
    /// Run was cancelled by user or system.
    Cancelled,
}

impl RunStatus {
    /// Check if the run is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Check if the run is active (not terminal and not pending).
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }

    /// Human-readable description.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Pending => "Waiting to start",
            Self::Running => "Currently executing",
            Self::Paused => "Paused and can be resumed",
            Self::Completed => "Completed successfully",
            Self::Failed => "Failed with error",
            Self::Cancelled => "Cancelled",
        }
    }
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// A step in an agent run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStep {
    /// Step number (1-indexed within the run).
    pub number: usize,

    /// Description of the step.
    pub description: String,

    /// Timestamp when the step started.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub started_at: DateTime<Utc>,

    /// Timestamp when the step completed (if applicable).
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub completed_at: Option<DateTime<Utc>>,

    /// Tool that was called (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,

    /// Whether the step succeeded.
    pub success: bool,

    /// Error message if the step failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Result data from the step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

/// Progress information for a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunProgress {
    /// Current step number.
    pub current_step: usize,

    /// Total steps in the plan (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_steps: Option<usize>,

    /// Percentage complete (0-100).
    pub percent_complete: u8,

    /// Estimated time remaining in seconds (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_seconds_remaining: Option<u64>,
}

impl RunProgress {
    /// Create progress with known total steps.
    pub fn with_total(current: usize, total: usize) -> Self {
        let percent_complete = if total > 0 {
            ((current as f64 / total as f64) * 100.0) as u8
        } else {
            0
        };

        Self {
            current_step: current,
            total_steps: Some(total),
            percent_complete,
            estimated_seconds_remaining: None,
        }
    }

    /// Create progress without a known total.
    pub fn unknown(current: usize) -> Self {
        Self {
            current_step: current,
            total_steps: None,
            percent_complete: 0,
            estimated_seconds_remaining: None,
        }
    }
}

/// The complete state of an agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunState {
    /// Unique run identifier.
    pub id: RunId,

    /// Parent run ID (if this is a child run).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<RunId>,

    /// The task being executed.
    pub task: String,

    /// Current status.
    pub status: RunStatus,

    /// When the run was created.
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,

    /// When the run started executing.
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub started_at: Option<DateTime<Utc>>,

    /// When the run completed, failed, or was cancelled.
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub completed_at: Option<DateTime<Utc>>,

    /// Steps executed in this run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<RunStep>,

    /// Current progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<RunProgress>,

    /// Final response (if completed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_response: Option<String>,

    /// Error message (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Number of memories written.
    #[serde(default)]
    pub memories_written: usize,

    /// Number of retries performed.
    #[serde(default)]
    pub retries: usize,

    /// Additional metadata.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl RunState {
    /// Create a new run state.
    pub fn new(task: impl Into<String>) -> Self {
        Self {
            id: RunId::new(),
            parent_id: None,
            task: task.into(),
            status: RunStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            steps: Vec::new(),
            progress: None,
            final_response: None,
            error: None,
            memories_written: 0,
            retries: 0,
            metadata: HashMap::new(),
        }
    }

    /// Create a child run state.
    pub fn child_of(parent_id: RunId, task: impl Into<String>) -> Self {
        Self {
            id: RunId::new(),
            parent_id: Some(parent_id),
            task: task.into(),
            status: RunStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            steps: Vec::new(),
            progress: None,
            final_response: None,
            error: None,
            memories_written: 0,
            retries: 0,
            metadata: HashMap::new(),
        }
    }

    /// Mark the run as started.
    pub fn start(&mut self) {
        self.status = RunStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Mark the run as completed.
    pub fn complete(&mut self, response: impl Into<String>) {
        self.status = RunStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.final_response = Some(response.into());
    }

    /// Mark the run as failed.
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = RunStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error = Some(error.into());
    }

    /// Mark the run as cancelled.
    pub fn cancel(&mut self) {
        self.status = RunStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// Pause the run.
    pub fn pause(&mut self) {
        if self.status == RunStatus::Running {
            self.status = RunStatus::Paused;
        }
    }

    /// Resume the run.
    pub fn resume(&mut self) {
        if self.status == RunStatus::Paused {
            self.status = RunStatus::Running;
        }
    }

    /// Add a step.
    pub fn add_step(&mut self, step: RunStep) {
        self.steps.push(step);
    }

    /// Update progress.
    pub fn update_progress(&mut self, progress: RunProgress) {
        self.progress = Some(progress);
    }

    /// Get the duration of the run (if completed or in progress).
    pub fn duration(&self) -> Option<chrono::Duration> {
        let end_time = self.completed_at.unwrap_or_else(|| Utc::now());

        self.started_at
            .map(|start| end_time.signed_duration_since(start))
    }

    /// Get the run age (time since creation).
    pub fn age(&self) -> chrono::Duration {
        Utc::now().signed_duration_since(self.created_at)
    }

    /// Check if the run can be cancelled.
    pub fn can_cancel(&self) -> bool {
        matches!(self.status, RunStatus::Pending | RunStatus::Running | RunStatus::Paused)
    }

    /// Check if the run can be resumed.
    pub fn can_resume(&self) -> bool {
        matches!(self.status, RunStatus::Paused)
    }

    /// Get the last step.
    pub fn last_step(&self) -> Option<&RunStep> {
        self.steps.last()
    }

    /// Add metadata.
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Store for persisting run states.
#[async_trait::async_trait]
pub trait RunStateStore: Send + Sync {
    /// Store a run state.
    async fn store(&self, state: &RunState) -> Result<(), RunStateError>;

    /// Retrieve a run state by ID.
    async fn get(&self, id: RunId) -> Result<Option<RunState>, RunStateError>;

    /// List all runs with optional filtering.
    async fn list(&self, filter: RunFilter) -> Result<Vec<RunState>, RunStateError>;

    /// Update an existing run state.
    async fn update(&self, state: &RunState) -> Result<(), RunStateError>;

    /// Delete a run state.
    async fn delete(&self, id: RunId) -> Result<(), RunStateError>;
}

/// Filter for listing runs.
#[derive(Debug, Clone, Default)]
pub struct RunFilter {
    /// Filter by status.
    pub status: Option<RunStatus>,
    /// Filter by parent ID.
    pub parent_id: Option<RunId>,
    /// Filter by created after timestamp.
    pub created_after: Option<DateTime<Utc>>,
    /// Filter by created before timestamp.
    pub created_before: Option<DateTime<Utc>>,
    /// Maximum results.
    pub limit: Option<usize>,
}

impl RunFilter {
    /// Create a new filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by status.
    pub fn with_status(mut self, status: RunStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Filter by parent ID.
    pub fn with_parent(mut self, parent_id: RunId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Filter by created after.
    pub fn created_after(mut self, timestamp: DateTime<Utc>) -> Self {
        self.created_after = Some(timestamp);
        self
    }

    /// Filter by created before.
    pub fn created_before(mut self, timestamp: DateTime<Utc>) -> Self {
        self.created_before = Some(timestamp);
        self
    }

    /// Set limit.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Error that can occur with run state operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RunStateError {
    #[error("Run not found: {0}")]
    NotFound(RunId),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition { from: RunStatus, to: RunStatus },
}

/// In-memory implementation of RunStateStore (for testing).
#[derive(Debug, Default)]
pub struct InMemoryRunStateStore {
    runs: std::sync::Mutex<HashMap<RunId, RunState>>,
}

#[async_trait::async_trait]
impl RunStateStore for InMemoryRunStateStore {
    async fn store(&self, state: &RunState) -> Result<(), RunStateError> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|e| RunStateError::Storage(e.to_string()))?;
        runs.insert(state.id, state.clone());
        Ok(())
    }

    async fn get(&self, id: RunId) -> Result<Option<RunState>, RunStateError> {
        let runs = self
            .runs
            .lock()
            .map_err(|e| RunStateError::Storage(e.to_string()))?;
        Ok(runs.get(&id).cloned())
    }

    async fn list(&self, filter: RunFilter) -> Result<Vec<RunState>, RunStateError> {
        let runs = self
            .runs
            .lock()
            .map_err(|e| RunStateError::Storage(e.to_string()))?;

        let mut results: Vec<_> = runs
            .values()
            .filter(|run| {
                if let Some(status) = filter.status {
                    if run.status != status {
                        return false;
                    }
                }
                if let Some(parent_id) = filter.parent_id {
                    if run.parent_id != Some(parent_id) {
                        return false;
                    }
                }
                if let Some(after) = filter.created_after {
                    if run.created_at < after {
                        return false;
                    }
                }
                if let Some(before) = filter.created_before {
                    if run.created_at > before {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Sort by created_at descending
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn update(&self, state: &RunState) -> Result<(), RunStateError> {
        self.store(state).await
    }

    async fn delete(&self, id: RunId) -> Result<(), RunStateError> {
        let mut runs = self
            .runs
            .lock()
            .map_err(|e| RunStateError::Storage(e.to_string()))?;
        runs.remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_id_new() {
        let id1 = RunId::new();
        let id2 = RunId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn run_status_is_terminal() {
        assert!(RunStatus::Completed.is_terminal());
        assert!(RunStatus::Failed.is_terminal());
        assert!(RunStatus::Cancelled.is_terminal());
        assert!(!RunStatus::Running.is_terminal());
        assert!(!RunStatus::Pending.is_terminal());
    }

    #[test]
    fn run_status_is_active() {
        assert!(RunStatus::Running.is_active());
        assert!(RunStatus::Paused.is_active());
        assert!(!RunStatus::Completed.is_active());
    }

    #[test]
    fn run_state_lifecycle() {
        let mut state = RunState::new("Test task");

        assert_eq!(state.status, RunStatus::Pending);
        assert!(state.started_at.is_none());

        state.start();
        assert_eq!(state.status, RunStatus::Running);
        assert!(state.started_at.is_some());

        state.complete("Done");
        assert_eq!(state.status, RunStatus::Completed);
        assert_eq!(state.final_response, Some("Done".to_string()));
    }

    #[test]
    fn run_state_child() {
        let parent = RunState::new("Parent task");
        let child = RunState::child_of(parent.id, "Child task");

        assert_eq!(child.parent_id, Some(parent.id));
        assert_ne!(child.id, parent.id);
    }

    #[test]
    fn run_state_pause_resume() {
        let mut state = RunState::new("Task");
        state.start();

        state.pause();
        assert_eq!(state.status, RunStatus::Paused);

        state.resume();
        assert_eq!(state.status, RunStatus::Running);
    }

    #[test]
    fn run_progress_with_total() {
        let progress = RunProgress::with_total(5, 10);
        assert_eq!(progress.current_step, 5);
        assert_eq!(progress.total_steps, Some(10));
        assert_eq!(progress.percent_complete, 50);
    }

    #[test]
    fn run_progress_unknown() {
        let progress = RunProgress::unknown(3);
        assert_eq!(progress.current_step, 3);
        assert_eq!(progress.total_steps, None);
        assert_eq!(progress.percent_complete, 0);
    }

    #[test]
    fn run_step_creation() {
        let step = RunStep {
            number: 1,
            description: "Test step".to_string(),
            started_at: Utc::now(),
            completed_at: None,
            tool_name: Some("echo".to_string()),
            success: true,
            error: None,
            result: None,
        };

        assert!(step.success);
        assert_eq!(step.tool_name, Some("echo".to_string()));
    }

    #[test]
    fn run_filter_building() {
        let filter = RunFilter::new()
            .with_status(RunStatus::Running)
            .created_after(Utc::now() - chrono::Duration::hours(1))
            .with_limit(10);

        assert_eq!(filter.status, Some(RunStatus::Running));
        assert_eq!(filter.limit, Some(10));
    }

    #[tokio::test]
    async fn in_memory_store_basic() {
        let store = InMemoryRunStateStore::default();
        let state = RunState::new("Test");

        store.store(&state).await.unwrap();

        let retrieved = store.get(state.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().task, "Test");
    }

    #[tokio::test]
    async fn in_memory_store_list() {
        let store = InMemoryRunStateStore::default();

        let mut active = RunState::new("Active");
        active.start();
        store.store(&active).await.unwrap();

        let mut completed = RunState::new("Completed");
        completed.complete("Done");
        store.store(&completed).await.unwrap();

        let running = store
            .list(RunFilter::new().with_status(RunStatus::Running))
            .await
            .unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].task, "Active");
    }

    #[tokio::test]
    async fn in_memory_store_delete() {
        let store = InMemoryRunStateStore::default();
        let state = RunState::new("To delete");

        store.store(&state).await.unwrap();
        store.delete(state.id).await.unwrap();

        let retrieved = store.get(state.id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn run_state_serialization() {
        let state = RunState::new("Test task").with_metadata("key", "value");
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: RunState = serde_json::from_str(&json).unwrap();

        assert_eq!(state.id, deserialized.id);
        assert_eq!(state.task, deserialized.task);
    }

    #[test]
    fn run_status_serialization() {
        let statuses = vec![
            RunStatus::Pending,
            RunStatus::Running,
            RunStatus::Completed,
            RunStatus::Failed,
            RunStatus::Cancelled,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: RunStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn run_status_description() {
        assert!(!RunStatus::Running.description().is_empty());
        assert!(!RunStatus::Completed.description().is_empty());
    }

    #[test]
    fn run_state_can_cancel() {
        let mut state = RunState::new("Task");
        assert!(state.can_cancel()); // Pending

        state.start();
        assert!(state.can_cancel()); // Running

        state.complete("Done");
        assert!(!state.can_cancel()); // Completed
    }
}
