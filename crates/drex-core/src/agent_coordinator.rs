//! Multi-Agent Coordinator - Spawn and manage multiple specialized agents
//!
//! Drex can spawn specialized sub-agents for parallel task execution:
//! - Research agents for web/information gathering
//! - Coding agents for code generation
//! - Analysis agents for data processing
//! - Review agents for code review
//! - Index agents for embedding updates
//!
//! # Architecture
//!
//! ```text
//! +-----------------+
//! |  Master Agent   |
//! |   Coordinator   |
//! +--------+--------+
//!          |
//!     +----+----+----+
//!     |    |    |
//!     v    v    v
//! +---+ +---+ +---+
//! | R | | C | | V |
//! +---+ +---+ +---+
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use drex_core::agent_coordinator::{Coordinator, AgentType, Task};
//!
//! let coordinator = Coordinator::new();
//!
//! // Spawn a research agent
//! let research = coordinator.spawn(AgentType::Research).await?;
//!
//! // Assign tasks
//! let results = coordinator.execute(&research, vec![
//!     Task::new("Search for Rust async patterns"),
//!     Task::new("Find examples of error handling"),
//! ]).await?;
//!
//! coordinator.release(research).await?;
//! ```

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Type of specialized agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentType {
    /// General purpose agent (like main Drex).
    General,
    /// Research agent - information gathering.
    Research,
    /// Coding agent - code generation and modification.
    Coding,
    /// Review agent - code review and analysis.
    Review,
    /// Analysis agent - data processing and analytics.
    Analysis,
    /// Index agent - embedding and vector operations.
    Index,
    /// Execution agent - task execution and tool calling.
    Execution,
}

impl AgentType {
    /// Get display name.
    pub fn name(&self) -> &'static str {
        match self {
            AgentType::General => "General",
            AgentType::Research => "Research",
            AgentType::Coding => "Coding",
            AgentType::Review => "Review",
            AgentType::Analysis => "Analysis",
            AgentType::Index => "Index",
            AgentType::Execution => "Execution",
        }
    }

    /// Get default capabilities for this agent type.
    pub fn default_capabilities(&self) -> Vec<String> {
        match self {
            AgentType::General => vec![
                "planning".to_string(),
                "execution".to_string(),
                "memory".to_string(),
            ],
            AgentType::Research => vec![
                "web.fetch".to_string(),
                "web.search".to_string(),
                "memory.store".to_string(),
            ],
            AgentType::Coding => vec![
                "filesystem.read".to_string(),
                "filesystem.write".to_string(),
                "git.diff".to_string(),
                "terminal.execute".to_string(),
            ],
            AgentType::Review => vec![
                "filesystem.read".to_string(),
                "git.diff".to_string(),
                "git.status".to_string(),
            ],
            AgentType::Analysis => vec![
                "memory.retrieve".to_string(),
                "filesystem.read".to_string(),
            ],
            AgentType::Index => vec![
                "memory.embed".to_string(),
                "memory.store".to_string(),
            ],
            AgentType::Execution => vec![
                "terminal.execute".to_string(),
                "filesystem.read".to_string(),
                "web.fetch".to_string(),
            ],
        }
    }
}

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Agent configuration.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Agent type.
    pub agent_type: AgentType,
    /// Maximum concurrent tasks.
    pub max_concurrent: usize,
    /// Task timeout.
    pub task_timeout: Duration,
    /// Context window size.
    pub context_size: usize,
    /// Model to use.
    pub model: String,
    /// Temperature.
    pub temperature: f32,
    /// System prompt.
    pub system_prompt: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_type: AgentType::General,
            max_concurrent: 3,
            task_timeout: Duration::from_secs(120),
            context_size: 4096,
            model: "default".to_string(),
            temperature: 0.7,
            system_prompt: None,
        }
    }
}

/// Agent capabilities.
#[derive(Debug, Clone)]
pub struct AgentCapabilities {
    /// Available tools.
    pub tools: Vec<String>,
    /// Can access filesystem.
    pub filesystem: bool,
    /// Can access network.
    pub network: bool,
    /// Can execute commands.
    pub execution: bool,
}

impl Default for AgentCapabilities {
    fn default() -> Self {
        Self {
            tools: vec![],
            filesystem: true,
            network: true,
            execution: true,
        }
    }
}

/// Agent identity.
#[derive(Debug, Clone)]
pub struct Agent {
    /// Agent ID.
    pub id: String,
    /// Agent configuration.
    pub config: AgentConfig,
    /// Created at.
    pub created_at: std::time::SystemTime,
    /// Current status.
    pub status: AgentStatus,
    /// Capabilities.
    pub capabilities: AgentCapabilities,
    /// Assigned tasks.
    pub assigned_tasks: Vec<String>,
}

/// Agent status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// Available for work.
    Idle,
    /// Currently working.
    Busy,
    /// Temporarily paused.
    Paused,
    /// Terminated.
    Terminated,
}

impl Agent {
    /// Create a new agent.
    pub fn new(config: AgentConfig) -> Self {
        let id = format!("{}-{}", config.agent_type.name().to_lowercase(), uuid::Uuid::new_v4());
        Self {
            id,
            config,
            created_at: std::time::SystemTime::now(),
            status: AgentStatus::Idle,
            capabilities: AgentCapabilities::default(),
            assigned_tasks: vec![],
        }
    }

    /// Check if available.
    pub fn is_available(&self) -> bool {
        self.status == AgentStatus::Idle
    }
}

/// Task for an agent.
#[derive(Debug, Clone)]
pub struct Task {
    /// Task ID.
    pub id: String,
    /// Task description.
    pub description: String,
    /// Task priority (0=highest).
    pub priority: u32,
    /// Task deadline (if any).
    pub deadline: Option<std::time::SystemTime>,
    /// Task context (json).
    pub context: serde_json::Value,
}

impl Task {
    /// Create a new task.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            id: format!("task-{}", uuid::Uuid::new_v4()),
            description: description.into(),
            priority: 0,
            deadline: None,
            context: serde_json::json!({}),
        }
    }

    /// With priority.
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// With deadline.
    pub fn with_deadline(mut self, deadline: std::time::SystemTime) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

/// Task result.
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Task ID.
    pub task_id: String,
    /// Agent ID that executed.
    pub agent_id: String,
    /// Success/failure.
    pub success: bool,
    /// Result data.
    pub result: Option<String>,
    /// Error (if any).
    pub error: Option<String>,
    /// Execution time.
    pub duration_ms: u64,
}

impl TaskResult {
    /// Create success result.
    pub fn success(task_id: String, agent_id: String, result: String, duration_ms: u64) -> Self {
        Self {
            task_id,
            agent_id,
            success: true,
            result: Some(result),
            error: None,
            duration_ms,
        }
    }

    /// Create failure result.
    pub fn failure(task_id: String, agent_id: String, error: String, duration_ms: u64) -> Self {
        Self {
            task_id,
            agent_id,
            success: false,
            result: None,
            error: Some(error),
            duration_ms,
        }
    }
}

/// Multi-agent coordination mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinationMode {
    /// Sequential execution (one after another).
    Sequential,
    /// Parallel execution (all at once).
    Parallel,
    /// Hierarchical (master delegates to workers).
    Hierarchical,
}

/// Coordinator configuration.
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Maximum total agents.
    pub max_agents: usize,
    /// Maximum agents per type.
    pub max_per_type: HashMap<AgentType, usize>,
    /// Default coordination mode.
    pub default_mode: CoordinationMode,
    /// Agent idle timeout.
    pub agent_idle_timeout: Duration,
    /// Enable auto-scaling.
    pub auto_scale: bool,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        let mut max_per_type = HashMap::new();
        max_per_type.insert(AgentType::Research, 3);
        max_per_type.insert(AgentType::Coding, 2);
        max_per_type.insert(AgentType::Review, 2);
        max_per_type.insert(AgentType::Analysis, 2);

        Self {
            max_agents: 10,
            max_per_type,
            default_mode: CoordinationMode::Parallel,
            agent_idle_timeout: Duration::from_secs(300),
            auto_scale: true,
        }
    }
}

/// Errors that can occur in the coordinator.
#[derive(Debug, thiserror::Error)]
pub enum CoordinatorError {
    /// Max agents reached.
    #[error("Maximum agents reached ({0})")]
    MaxAgentsReached(usize),

    /// Max agents of type reached.
    #[error("Maximum {0} agents reached ({1})")]
    MaxTypeReached(AgentType, usize),

    /// Agent not found.
    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    /// Agent busy.
    #[error("Agent {} is busy", 0)]
    AgentBusy(String),

    /// Task timeout.
    #[error("Task {} timed out", 0)]
    TaskTimeout(String),

    /// Execution error.
    #[error("Execution error: {0}")]
    ExecutionError(String),
}

/// Multi-agent coordinator.
pub struct Coordinator {
    config: CoordinatorConfig,
    agents: Arc<RwLock<HashMap<String, Agent>>>,
    running: Arc<Mutex<bool>>,
    // Task channel for async communication
    task_tx: mpsc::Sender<(String, Task)>,
    result_rx: Mutex<mpsc::Receiver<TaskResult>>,
}

impl Coordinator {
    /// Create a new coordinator.
    pub fn new(config: CoordinatorConfig) -> Self {
        let (task_tx, _task_rx) = mpsc::channel::<(String, Task)>(100);
        let (result_tx, result_rx) = mpsc::channel::<TaskResult>(100);

        Self {
            config,
            agents: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(Mutex::new(false)),
            task_tx,
            result_rx: Mutex::new(result_rx),
        }
    }

    /// Create with default configuration.
    pub fn default() -> Self {
        Self::new(CoordinatorConfig::default())
    }

    /// Get agent count.
    pub async fn agent_count(&self) -> usize {
        let agents = self.agents.read().await;
        agents.len()
    }

    /// Get count by type.
    pub async fn count_by_type(&self, agent_type: AgentType) -> usize {
        let agents = self.agents.read().await;
        agents.values().filter(|a| a.config.agent_type == agent_type).count()
    }

    /// Spawn a new agent.
    pub async fn spawn(&self, agent_type: AgentType) -> Result<String, CoordinatorError> {
        // Check total agents limit
        let total_count = self.agent_count().await;
        if total_count >= self.config.max_agents {
            return Err(CoordinatorError::MaxAgentsReached(self.config.max_agents));
        }

        // Check per-type limit
        let type_count = self.count_by_type(agent_type).await;
        if let Some(max) = self.config.max_per_type.get(&agent_type) {
            if type_count >= *max {
                return Err(CoordinatorError::MaxTypeReached(agent_type, *max));
            }
        }

        let config = AgentConfig {
            agent_type,
            ..Default::default()
        };

        let agent = Agent::new(config);
        let agent_id = agent.id.clone();

        info!("Spawned {} agent: {}", agent_type.name(), agent_id);

        let mut agents = self.agents.write().await;
        agents.insert(agent_id.clone(), agent);

        Ok(agent_id)
    }

    /// Release an agent.
    pub async fn release(&self, agent_id: &str) -> Result<(), CoordinatorError> {
        let mut agents = self.agents.write().await;
        if agents.remove(agent_id).is_some() {
            info!("Released agent: {}", agent_id);
            Ok(())
        } else {
            Err(CoordinatorError::AgentNotFound(agent_id.to_string()))
        }
    }

    /// Get an agent.
    pub async fn get_agent(&self, agent_id: &str) -> Option<Agent> {
        let agents = self.agents.read().await;
        agents.get(agent_id).cloned()
    }

    /// List all agents.
    pub async fn list_agents(&self) -> Vec<Agent> {
        let agents = self.agents.read().await;
        agents.values().cloned().collect()
    }

    /// List available (idle) agents.
    pub async fn list_available(&self) -> Vec<Agent> {
        self.list_agents().await.into_iter()
            .filter(|a| a.is_available())
            .collect()
    }

    /// List agents by type.
    pub async fn list_by_type(&self, agent_type: AgentType) -> Vec<Agent> {
        self.list_agents().await.into_iter()
            .filter(|a| a.config.agent_type == agent_type)
            .collect()
    }

    /// Find available agent of specific type.
    pub async fn find_available(&self, agent_type: AgentType) -> Option<Agent> {
        self.list_by_type(agent_type).await.into_iter()
            .find(|a| a.is_available())
    }

    /// Assign task to agent.
    pub async fn assign(&self, agent_id: &str, task: Task) -> Result<(), CoordinatorError> {
        let mut agents = self.agents.write().await;
        
        if let Some(agent) = agents.get_mut(agent_id) {
            if !agent.is_available() {
                return Err(CoordinatorError::AgentBusy(agent_id.to_string()));
            }
            
            agent.status = AgentStatus::Busy;
            agent.assigned_tasks.push(task.id.clone());
            info!("Assigned task {} to agent {}", task.id, agent_id);
            Ok(())
        } else {
            Err(CoordinatorError::AgentNotFound(agent_id.to_string()))
        }
    }

    /// Mark agent as complete.
    pub async fn complete(&self, agent_id: &str, _task_id: &str) -> Result<(), CoordinatorError> {
        let mut agents = self.agents.write().await;
        
        if let Some(agent) = agents.get_mut(agent_id) {
            agent.status = AgentStatus::Idle;
            info!("Agent {} is now idle", agent_id);
            Ok(())
        } else {
            Err(CoordinatorError::AgentNotFound(agent_id.to_string()))
        }
    }

    /// Execute tasks with an agent.
    pub async fn execute(
        &self,
        agent_id: &str,
        tasks: Vec<Task>,
    ) -> Result<Vec<TaskResult>, CoordinatorError> {
        let agent_id = agent_id.to_string();
        let mut results = Vec::new();
        
        for task in tasks {
            self.assign(&agent_id, task.clone()).await?;
            
            let start = std::time::Instant::now();
            let result = self.execute_task(&agent_id, &task).await;
            let duration_ms = start.elapsed().as_millis() as u64;
            
            let task_result = match result {
                Ok(res) => TaskResult::success(task.id.clone(), agent_id.clone(), res, duration_ms),
                Err(e) => TaskResult::failure(task.id.clone(), agent_id.clone(), e, duration_ms),
            };
            
            self.complete(&agent_id, &task.id).await?;
            results.push(task_result);
        }
        
        Ok(results)
    }

    /// Execute a single task (placeholder implementation).
    async fn execute_task(&self, agent_id: &str, task: &Task) -> Result<String, String> {
        info!("Agent {} executing: {}", agent_id, task.description);
        
        // Placeholder: in real implementation, this would dispatch to the agent
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        Ok(format!("Executed: {}", task.description))
    }

    /// Execute tasks in parallel with spawn.
    pub async fn execute_parallel(
        self: Arc<Self>,
        agent_type: AgentType,
        tasks: Vec<Task>,
    ) -> Result<Vec<TaskResult>, CoordinatorError> {
        let mut handles = Vec::new();

        for task in tasks {
            let coordinator = self.clone();
            
            // Find or spawn agent
            let agent_id = if let Some(agent) = coordinator.find_available(agent_type).await {
                agent.id
            } else {
                coordinator.spawn(agent_type).await?
            };

            let handle: JoinHandle<Result<TaskResult, CoordinatorError>> = tokio::spawn(async move {
                coordinator.assign(&agent_id, task.clone()).await?;
                
                let start = std::time::Instant::now();
                let result = coordinator.execute_task(&agent_id, &task).await;
                let duration_ms = start.elapsed().as_millis() as u64;
                
                coordinator.complete(&agent_id, &task.id).await?;
                
                Ok(match result {
                    Ok(res) => TaskResult::success(task.id.clone(), agent_id, res, duration_ms),
                    Err(e) => TaskResult::failure(task.id.clone(), agent_id, e, duration_ms),
                })
            });

            handles.push(handle);
        }

        // Wait for all tasks
        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(Ok(result)) => results.push(result),
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(CoordinatorError::ExecutionError(e.to_string())),
            }
        }

        Ok(results)
    }

    /// Delegate work and collect results.
    pub async fn delegate(
        &self,
        subtasks: Vec<(AgentType, Vec<Task>)>,
    ) -> Result<HashMap<String, Vec<TaskResult>>, CoordinatorError> {
        let mut results: HashMap<String, Vec<TaskResult>> = HashMap::new();

        for (agent_type, tasks) in subtasks {
            let agent_results = self.execute_with_type(agent_type, tasks).await?;
            results.insert(agent_type.name().to_string(), agent_results);
        }

        Ok(results)
    }

    /// Execute tasks, auto-spawning agents as needed.
    async fn execute_with_type(
        &self,
        agent_type: AgentType,
        tasks: Vec<Task>,
    ) -> Result<Vec<TaskResult>, CoordinatorError> {
        let mut results = Vec::new();

        for task in tasks {
            // Get or create agent
            let agent_id = match self.find_available(agent_type).await {
                Some(agent) => agent.id,
                None => self.spawn(agent_type).await?,
            };

            results.extend(self.execute(&agent_id, vec![task]).await?);
        }

        Ok(results)
    }

    /// Get coordinator stats.
    pub async fn stats(&self) -> CoordinatorStats {
        let agents = self.list_agents().await;
        
        CoordinatorStats {
            total_agents: agents.len(),
            idle: agents.iter().filter(|a| a.status == AgentStatus::Idle).count(),
            busy: agents.iter().filter(|a| a.status == AgentStatus::Busy).count(),
            by_type: agents.iter()
                .map(|a| a.config.agent_type)
                .fold(HashMap::new(), |mut acc, t| {
                    *acc.entry(t).or_insert(0) += 1;
                    acc
                }),
        }
    }
}

/// Coordinator statistics.
#[derive(Debug, Clone)]
pub struct CoordinatorStats {
    pub total_agents: usize,
    pub idle: usize,
    pub busy: usize,
    pub by_type: HashMap<AgentType, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_name() {
        assert_eq!(AgentType::Research.name(), "Research");
        assert_eq!(AgentType::Coding.name(), "Coding");
    }

    #[test]
    fn test_agent_type_capabilities() {
        let caps = AgentType::Research.default_capabilities();
        assert!(caps.iter().any(|c| c.contains("web")));
    }

    #[tokio::test]
    async fn test_coordinator_spawn() {
        let coordinator = Coordinator::default();
        let agent_id = coordinator.spawn(AgentType::Research).await.unwrap();
        assert!(!agent_id.is_empty());
        
        let agent = coordinator.get_agent(&agent_id).await;
        assert!(agent.is_some());
    }

    #[tokio::test]
    async fn test_coordinator_release() {
        let coordinator = Coordinator::default();
        let id = coordinator.spawn(AgentType::General).await.unwrap();
        coordinator.release(&id).await.unwrap();
        
        let agent = coordinator.get_agent(&id).await;
        assert!(agent.is_none());
    }

    #[tokio::test]
    async fn test_max_agents_limit() {
        let config = CoordinatorConfig {
            max_agents: 1,
            ..Default::default()
        };
        let coordinator = Coordinator::new(config);
        
        let _ = coordinator.spawn(AgentType::General).await.unwrap();
        let result = coordinator.spawn(AgentType::General).await;
        
        assert!(matches!(result, Err(CoordinatorError::MaxAgentsReached(1))));
    }

    #[tokio::test]
    async fn test_assign_and_complete() {
        let coordinator = Coordinator::default();
        let agent_id = coordinator.spawn(AgentType::Research).await.unwrap();
        
        let task = Task::new("Test task");
        coordinator.assign(&agent_id, task.clone()).await.unwrap();
        
        let agent = coordinator.get_agent(&agent_id).await.unwrap();
        assert!(matches!(agent.status, AgentStatus::Busy));
        
        coordinator.complete(&agent_id, &task.id).await.unwrap();
        
        let agent = coordinator.get_agent(&agent_id).await.unwrap();
        assert!(matches!(agent.status, AgentStatus::Idle));
    }

    #[tokio::test]
    async fn test_execute_tasks() {
        let coordinator = Coordinator::default();
        let agent_id = coordinator.spawn(AgentType::General).await.unwrap();
        
        let tasks = vec![
            Task::new("Task 1"),
            Task::new("Task 2"),
        ];
        
        let results = coordinator.execute(&agent_id, tasks).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
    }

    #[tokio::test]
    async fn test_find_available() {
        let coordinator = Coordinator::default();
        let id1 = coordinator.spawn(AgentType::Research).await.unwrap();
        let _ = coordinator.spawn(AgentType::Research).await.unwrap();
        
        // Make first one busy
        coordinator.assign(&id1, Task::new("busy task")).await.unwrap();
        
        let available = coordinator.find_available(AgentType::Research).await;
        assert!(available.is_some());
        assert_ne!(available.unwrap().id, id1);
    }

    #[test]
    fn test_task_defaults() {
        let task = Task::new("Test");
        assert_eq!(task.priority, 0);
        assert!(task.deadline.is_none());
    }

    #[test]
    fn test_task_result_success() {
        let result = TaskResult::success("t1".to_string(), "a1".to_string(), "done".to_string(), 100);
        assert!(result.success);
        assert_eq!(result.result.unwrap(), "done");
    }

    #[test]
    fn test_task_result_failure() {
        let result = TaskResult::failure("t1".to_string(), "a1".to_string(), "error".to_string(), 50);
        assert!(!result.success);
        assert_eq!(result.error.unwrap(), "error");
    }
}
