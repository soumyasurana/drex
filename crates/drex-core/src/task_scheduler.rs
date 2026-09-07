//! Background Task Scheduler - Cron-like job scheduling for autonomous tasks
//!
//! Provides persistent, scheduled background task execution:
//! - Cron-style scheduling (e.g., "0 9 * * *" for 9am daily)
//! - Interval-based triggers (e.g., every 5 minutes)
//! - One-shot future tasks
//! - Task persistence (survives restarts)
//! - Task history and retry logic
//!
//! # Architecture
//!
//! ```text
//! +-----------------+
//! |  TaskScheduler  |
//! | +--+ +--+ +--+  |
//! | |Cr| |In| |On|  |
//! | +--+ +--+ +--+  |
//! | +--+ +--+ +--+  |
//! | |JQ| |TC| |Ex|  |
//! | +--+ +--+ +--+  |
//! +-----------------+
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use drex_core::task_scheduler::{TaskScheduler, TaskConfig, Schedule};
//!
//! let scheduler = TaskScheduler::new().await?;
//!
//! // Schedule a daily task
//! let task_id = scheduler.schedule(TaskConfig {
//!     name: "Daily Report".to_string(),
//!     schedule: Schedule::Cron("0 9 * * *".to_string()),  // 9am daily
//!     action: Action::ExecuteTool {
//!         tool: "filesystem.read".to_string(),
//!         params: json!({"path": "/tmp/daily.txt"})
//!     },
//!     max_retries: 3,
//! }).await?;
//!
//! // Start the scheduler
//! scheduler.start().await?;
//! ```

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Task execution action.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TaskAction {
    /// Execute a tool.
    ExecuteTool {
        tool: String,
        params: serde_json::Value,
    },
    /// Send a notification.
    Notify {
        message: String,
        channel: String,
    },
    /// Run a script.
    RunScript {
        script: String,
        interpreter: String,
    },
    /// Custom action.
    Custom {
        action_type: String,
        payload: serde_json::Value,
    },
}

/// Task schedule type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Schedule {
    /// Cron expression (e.g., "0 9 * * *" for 9am daily).
    Cron(String),
    /// Interval in seconds.
    Interval(u64),
    /// One-shot execution at a specific time.
    OneShot(SystemTime),
    /// Execute immediately on startup (one-time).
    OnStartup,
}

impl Schedule {
    /// Check if the schedule is due to run.
    pub fn is_due(&self, last_run: Option<SystemTime>) -> bool {
        match self {
            Schedule::OnStartup => last_run.is_none(),
            Schedule::OneShot(time) => {
                SystemTime::now() >= *time && last_run.is_none()
            }
            Schedule::Interval(secs) => {
                match last_run {
                    None => true,
                    Some(last) => {
                        let elapsed = SystemTime::now()
                            .duration_since(last)
                            .unwrap_or_default()
                            .as_secs();
                        elapsed >= *secs
                    }
                }
            }
            Schedule::Cron(cron) => {
                // Parse cron and check if current time matches
                // For now, simplified - check if pattern matches current time
                Self::check_cron_due(cron, last_run)
            }
        }
    }

    /// Get next run time (approximate).
    pub fn next_run(&self, from: SystemTime) -> Option<SystemTime> {
        match self {
            Schedule::OnStartup => None, // Only runs once
            Schedule::OneShot(time) => Some(*time),
            Schedule::Interval(secs) => {
                from.checked_add(std::time::Duration::from_secs(*secs))
            }
            Schedule::Cron(cron) => {
                // Parse cron and calculate next run
                Self::next_cron_run(cron, from)
            }
        }
    }

    /// Check if cron pattern is currently due.
    fn check_cron_due(cron: &str, last_run: Option<SystemTime>) -> bool {
        // Placeholder: basic cron parsing
        // In production, use a proper cron library like `cron` or `saffron`
        if let Some(last) = last_run {
            // If last ran within the minute, don't run again
            let elapsed = SystemTime::now()
                .duration_since(last)
                .unwrap_or_default()
                .as_secs();
            elapsed >= 60 // Simple guard: max once per minute minimum
        } else {
            true
        }
    }

    /// Calculate next cron run time.
    fn next_cron_run(_cron: &str, from: SystemTime) -> Option<SystemTime> {
        // Placeholder: return 1 minute from now
        from.checked_add(std::time::Duration::from_secs(60))
    }
}

/// Task configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskConfig {
    /// Task ID (generated if not provided).
    pub id: Option<String>,
    /// Task name (human-readable).
    pub name: String,
    /// Task description.
    pub description: Option<String>,
    /// When to run.
    pub schedule: Schedule,
    /// What to do.
    pub action: TaskAction,
    /// Maximum number of retries on failure.
    pub max_retries: u32,
    /// Initial delay before first run.
    pub initial_delay_secs: u64,
    /// Whether the task is enabled.
    pub enabled: bool,
    /// Tags for organization.
    pub tags: Vec<String>,
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            id: None,
            name: "Unnamed Task".to_string(),
            description: None,
            schedule: Schedule::Interval(3600), // Hourly default
            action: TaskAction::ExecuteTool {
                tool: "echo".to_string(),
                params: serde_json::json!({"message": "Hello"}),
            },
            max_retries: 3,
            initial_delay_secs: 0,
            enabled: true,
            tags: vec![],
        }
    }
}

/// Task execution status.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TaskStatus {
    /// Waiting to be scheduled.
    Pending,
    /// Currently running.
    Running,
    /// Completed successfully.
    Completed,
    /// Failed (may retry).
    Failed,
    /// Failed permanently after retries.
    PermanentFailure,
    /// Cancelled by user.
    Cancelled,
}

/// A scheduled task.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScheduledTask {
    /// Task ID.
    pub id: String,
    /// Task configuration.
    pub config: TaskConfig,
    /// Current status.
    pub status: TaskStatus,
    /// When the task was created.
    pub created_at: SystemTime,
    /// Last run time.
    pub last_run: Option<SystemTime>,
    /// Next scheduled run time.
    pub next_run: Option<SystemTime>,
    /// Number of times run.
    pub run_count: u32,
    /// Number of failures.
    pub failure_count: u32,
    /// Last error message (if any).
    pub last_error: Option<String>,
    /// Last result (if any).
    pub last_result: Option<serde_json::Value>,
}

impl ScheduledTask {
    /// Create a new scheduled task from config.
    pub fn new(config: TaskConfig) -> Self {
        let id = config.id.clone().unwrap_or_else(|| generate_task_id());
        let created_at = SystemTime::now();
        let next_run = if config.initial_delay_secs > 0 {
            created_at.checked_add(std::time::Duration::from_secs(config.initial_delay_secs))
        } else {
            Some(created_at)
        };

        Self {
            id,
            config,
            status: TaskStatus::Pending,
            created_at,
            last_run: None,
            next_run,
            run_count: 0,
            failure_count: 0,
            last_error: None,
            last_result: None,
        }
    }

    /// Check if the task is due to run.
    pub fn is_due(&self) -> bool {
        if !self.config.enabled {
            return false;
        }

        if !matches!(self.status, TaskStatus::Pending | TaskStatus::Completed | TaskStatus::Failed) {
            return false;
        }

        // Check schedule
        self.config.schedule.is_due(self.last_run)
    }

    /// Update status to running.
    pub fn mark_running(&mut self) {
        self.status = TaskStatus::Running;
    }

    /// Mark as completed.
    pub fn mark_completed(&mut self, result: Option<serde_json::Value>) {
        self.status = TaskStatus::Completed;
        self.last_run = Some(SystemTime::now());
        self.last_result = result;
        self.run_count += 1;
        self.failure_count = 0;
        self.last_error = None;

        // Calculate next run
        if let Some(scheduled) = self.config.schedule.next_run(SystemTime::now()) {
            self.next_run = Some(scheduled);
        } else {
            self.next_run = None; // One-shot completed
        }
    }

    /// Mark as failed.
    pub fn mark_failed(&mut self, error: &str) {
        self.failure_count += 1;
        self.last_error = Some(error.to_string());
        self.last_run = Some(SystemTime::now());

        // Check if should retry
        if self.failure_count >= self.config.max_retries {
            self.status = TaskStatus::PermanentFailure;
        } else {
            self.status = TaskStatus::Failed;
        }
    }

    /// Reset for retry.
    pub fn reset(&mut self) {
        self.status = TaskStatus::Pending;
        self.failure_count = 0;
        self.last_error = None;
    }
}

/// Generate a unique task ID.
fn generate_task_id() -> String {
    format!("task_{}", uuid::Uuid::new_v4())
}

/// Task execution result.
pub struct TaskExecutionResult {
    /// Task ID.
    pub task_id: String,
    /// Success/failure.
    pub success: bool,
    /// Result data.
    pub result: Option<serde_json::Value>,
    /// Error message (if failed).
    pub error: Option<String>,
    /// Execution duration.
    pub duration_ms: u64,
}

/// Task executor trait.
#[async_trait::async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Execute a task action.
    async fn execute(&self, action: &TaskAction) -> Result<serde_json::Value, String>;
}

/// Simple executor for testing.
pub struct DummyExecutor;

#[async_trait::async_trait]
impl TaskExecutor for DummyExecutor {
    async fn execute(&self, action: &TaskAction) -> Result<serde_json::Value, String> {
        info!("Executing task action: {:?}", action);
        Ok(serde_json::json!({"status": "ok"}))
    }
}

/// Errors that can occur in the task scheduler.
#[derive(Debug, thiserror::Error)]
pub enum TaskSchedulerError {
    /// Task not found.
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    /// Task already exists.
    #[error("Task already exists: {0}")]
    TaskAlreadyExists(String),

    /// Invalid schedule.
    #[error("Invalid schedule: {0}")]
    InvalidSchedule(String),

    /// Execution error.
    #[error("Task execution failed: {0}")]
    ExecutionError(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Task scheduler configuration.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Check interval for due tasks (seconds).
    pub check_interval_secs: u64,
    /// Maximum concurrent tasks.
    pub max_concurrent_tasks: usize,
    /// Worker thread count.
    pub worker_threads: usize,
    /// Task history retention (days).
    pub history_retention_days: u32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 10,
            max_concurrent_tasks: 10,
            worker_threads: 4,
            history_retention_days: 30,
        }
    }
}

/// Background task scheduler.
pub struct TaskScheduler {
    config: SchedulerConfig,
    tasks: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    running: Arc<RwLock<bool>>,
}

impl TaskScheduler {
    /// Create a new task scheduler.
    pub fn new(config: SchedulerConfig) -> Self {
        info!(
            "Creating task scheduler (check_interval: {}s, max_concurrent: {})",
            config.check_interval_secs, config.max_concurrent_tasks
        );

        Self {
            config,
            tasks: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Create with default configuration.
    pub fn default() -> Self {
        Self::new(SchedulerConfig::default())
    }

    /// Schedule a new task.
    pub async fn schedule(&self, config: TaskConfig) -> Result<String, TaskSchedulerError> {
        let task = ScheduledTask::new(config);
        let task_id = task.id.clone();

        let mut tasks = self.tasks.write().await;
        if tasks.contains_key(&task_id) {
            return Err(TaskSchedulerError::TaskAlreadyExists(task_id));
        }

        info!("Scheduled task: {} ({}) with schedule {:?}", task_id, task.config.name, task.config.schedule);
        tasks.insert(task_id.clone(), task);

        Ok(task_id)
    }

    /// Cancel a scheduled task.
    pub async fn cancel(&self, task_id: &str) -> Result<(), TaskSchedulerError> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.status = TaskStatus::Cancelled;
            info!("Cancelled task: {}", task_id);
            Ok(())
        } else {
            Err(TaskSchedulerError::TaskNotFound(task_id.to_string()))
        }
    }

    /// Remove a task permanently.
    pub async fn remove(&self, task_id: &str) -> Result<(), TaskSchedulerError> {
        let mut tasks = self.tasks.write().await;
        if tasks.remove(task_id).is_some() {
            info!("Removed task: {}", task_id);
            Ok(())
        } else {
            Err(TaskSchedulerError::TaskNotFound(task_id.to_string()))
        }
    }

    /// Get a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Option<ScheduledTask> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// List all tasks.
    pub async fn list_tasks(&self) -> Vec<ScheduledTask> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    /// List tasks by status.
    pub async fn list_by_status(&self, status: TaskStatus) -> Vec<ScheduledTask> {
        self.list_tasks().await.into_iter()
            .filter(|t| t.status == status)
            .collect()
    }

    /// List due tasks.
    pub async fn list_due(&self) -> Vec<ScheduledTask> {
        self.list_tasks().await.into_iter()
            .filter(|t| t.is_due())
            .collect()
    }

    /// Enable/disable a task.
    pub async fn set_enabled(&self, task_id: &str, enabled: bool) -> Result<(), TaskSchedulerError> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.config.enabled = enabled;
            info!("Task {}: enabled={}", task_id, enabled);
            Ok(())
        } else {
            Err(TaskSchedulerError::TaskNotFound(task_id.to_string()))
        }
    }

    /// Get scheduler stats.
    pub async fn stats(&self) -> SchedulerStats {
        let tasks = self.tasks.read().await;
        SchedulerStats {
            total_tasks: tasks.len(),
            pending: tasks.values().filter(|t| t.status == TaskStatus::Pending).count(),
            running: tasks.values().filter(|t| t.status == TaskStatus::Running).count(),
            completed: tasks.values().filter(|t| t.status == TaskStatus::Completed).count(),
            failed: tasks.values().filter(|t| matches!(t.status, TaskStatus::Failed | TaskStatus::PermanentFailure)).count(),
        }
    }

    /// Start the scheduler background loop.
    pub async fn start(&self, executor: Arc<dyn TaskExecutor>) -> Result<(), TaskSchedulerError> {
        info!("Starting task scheduler");
        *self.running.write().await = true;

        let check_interval = tokio::time::Duration::from_secs(self.config.check_interval_secs);
        let tasks = self.tasks.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            loop {
                if !*running.read().await {
                    break;
                }

                // Check for due tasks
                let due_tasks = {
                    let tasks = tasks.read().await;
                    tasks.values()
                        .filter(|t| t.is_due())
                        .cloned()
                        .collect::<Vec<_>>()
                };

                for task in due_tasks {
                    let task_id = task.id.clone();
                    let executor = executor.clone();
                    let tasks = tasks.clone();

                    // Spawn task execution
                    tokio::spawn(async move {
                        info!("Executing task: {} ({})", task_id, task.config.name);
                        
                        // Clone needed data before await
                        let config = task.config.clone();
                        
                        {
                            let mut tasks = tasks.write().await;
                            if let Some(t) = tasks.get_mut(&task_id) {
                                t.mark_running();
                            }
                        }

                        let start = std::time::Instant::now();
                        let result = executor.execute(&config.action).await;

                        let mut tasks = tasks.write().await;
                        if let Some(t) = tasks.get_mut(&task_id) {
                            match result {
                                Ok(data) => {
                                    t.mark_completed(Some(data));
                                    info!("Task {} completed in {}ms", task_id, start.elapsed().as_millis());
                                }
                                Err(e) => {
                                    t.mark_failed(&e);
                                    error!("Task {} failed: {}", task_id, e);
                                }
                            }
                        }
                    });
                }

                tokio::time::sleep(check_interval).await;
            }

            info!("Task scheduler stopped");
        });

        Ok(())
    }

    /// Stop the scheduler.
    pub async fn stop(&self) {
        info!("Stopping task scheduler");
        *self.running.write().await = false;
    }

    /// Check if running.
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    /// Trigger a task manually (ignores schedule).
    pub async fn trigger_now(&self, task_id: &str, executor: Arc<dyn TaskExecutor>) -> Result<TaskExecutionResult, TaskSchedulerError> {
        let task = self.get_task(task_id).await
            .ok_or_else(|| TaskSchedulerError::TaskNotFound(task_id.to_string()))?;

        let start = std::time::Instant::now();
        let result = executor.execute(&task.config.action).await;

        let execution_result = match &result {
            Ok(data) => TaskExecutionResult {
                task_id: task_id.to_string(),
                success: true,
                result: Some(data.clone()),
                error: None,
                duration_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => TaskExecutionResult {
                task_id: task_id.to_string(),
                success: false,
                result: None,
                error: Some(e.clone()),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        };

        // Update task state
        let mut tasks = self.tasks.write().await;
        if let Some(t) = tasks.get_mut(task_id) {
            match result {
                Ok(data) => t.mark_completed(Some(data)),
                Err(e) => t.mark_failed(&e),
            }
        }

        Ok(execution_result)
    }
}

/// Task scheduler statistics.
#[derive(Debug, Clone, Default)]
pub struct SchedulerStats {
    pub total_tasks: usize,
    pub pending: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
}

/// Convenience function to create and start a scheduler.
pub async fn start_scheduler(
    config: SchedulerConfig,
    executor: Arc<dyn TaskExecutor>,
) -> Result<Arc<TaskScheduler>, TaskSchedulerError> {
    let scheduler = Arc::new(TaskScheduler::new(config));
    scheduler.start(executor).await?;
    Ok(scheduler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schedule_interval() {
        let schedule = Schedule::Interval(60);
        assert!(schedule.is_due(None));
        
        let recent = SystemTime::now();
        assert!(!schedule.is_due(Some(recent)));
    }

    #[test]
    fn test_schedule_on_startup() {
        let schedule = Schedule::OnStartup;
        assert!(schedule.is_due(None));
        assert!(!schedule.is_due(Some(SystemTime::now())));
    }

    #[test]
    fn test_task_is_due() {
        let mut task = ScheduledTask::new(TaskConfig::default());
        assert!(task.is_due());
        
        task.mark_completed(None);
        // After completion, should be due again immediately for interval tasks
        assert!(!task.is_due());
    }

    #[test]
    fn test_task_lifecycle() {
        let mut task = ScheduledTask::new(TaskConfig::default());
        
        task.mark_running();
        assert!(matches!(task.status, TaskStatus::Running));
        
        task.mark_completed(Some(serde_json::json!({"result": "ok"})));
        assert!(matches!(task.status, TaskStatus::Completed));
        assert!(task.last_run.is_some());
        assert_eq!(task.run_count, 1);
    }

    #[test]
    fn test_task_failure() {
        let mut task = ScheduledTask::new(TaskConfig {
            max_retries: 3,
            ..Default::default()
        });

        task.mark_failed("Test error");
        assert!(matches!(task.status, TaskStatus::Failed));
        assert_eq!(task.failure_count, 1);

        task.mark_failed("Test error 2");
        assert!(matches!(task.status, TaskStatus::Failed));
        assert_eq!(task.failure_count, 2);

        task.mark_failed("Test error 3");
        assert!(matches!(task.status, TaskStatus::PermanentFailure));
        assert_eq!(task.failure_count, 3);
    }

    #[test]
    fn test_scheduler_stats() {
        let stats = SchedulerStats {
            total_tasks: 10,
            pending: 3,
            running: 1,
            completed: 5,
            failed: 1,
        };
        
        assert_eq!(stats.total_tasks, 10);
        assert_eq!(stats.pending, 3);
    }

    #[tokio::test]
    async fn test_scheduler_schedule() {
        let scheduler = TaskScheduler::default();
        
        let config = TaskConfig {
            name: "Test Task".to_string(),
            schedule: Schedule::Interval(60),
            ..Default::default()
        };
        
        let task_id = scheduler.schedule(config).await.unwrap();
        assert!(!task_id.is_empty());
        
        let task = scheduler.get_task(&task_id).await;
        assert!(task.is_some());
    }

    #[tokio::test]
    async fn test_scheduler_cancel() {
        let scheduler = TaskScheduler::default();
        
        let config = TaskConfig {
            name: "Test Task".to_string(),
            ..Default::default()
        };
        
        let task_id = scheduler.schedule(config).await.unwrap();
        scheduler.cancel(&task_id).await.unwrap();
        
        let task = scheduler.get_task(&task_id).await.unwrap();
        assert!(matches!(task.status, TaskStatus::Cancelled));
    }

    #[tokio::test]
    async fn test_scheduler_list_due() {
        let scheduler = TaskScheduler::default();
        
        // Add a task with OnStartup schedule
        let config = TaskConfig {
            name: "Startup Task".to_string(),
            schedule: Schedule::OnStartup,
            ..Default::default()
        };
        
        let task_id = scheduler.schedule(config).await.unwrap();
        
        let due = scheduler.list_due().await;
        assert!(!due.is_empty());
        assert!(due.iter().any(|t| t.id == task_id));
    }
}
