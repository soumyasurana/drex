//! Drex Core - Main application runtime
//!
//! Drex Core is responsible for:
//! - Configuration loading
//! - Logging/tracing initialization
//! - Health checks for all backends
//! - Application state management
//! - Memory system integration
//! - Graceful shutdown handling

pub mod audit;
pub mod agent_coordinator;
pub mod credential_audit;
pub mod event_bus;
pub mod execution_mode;
pub mod health_check;
pub mod security;
pub mod state;
pub mod task_scheduler;

/// Re-export commonly used types
pub use audit::{
    AuditError, AuditEvent, AuditResult as AuditTrailResult, AuditScope,
    AuditSeverity, AuditStats, AuditTrail, IntegrityReport, IntegrityViolation,
    ViolationType,
};
pub use credential_audit::{
    CredentialAuditReport, CredentialRiskLevel, CredentialSanitizer,
    CredentialType, EnvCredentialScanner, SecureValue,
};
pub use event_bus::{
    AutonomousTrigger, Event, EventBus, EventBusConfig, EventBusStats,
    EventHandler, EventSeverity, EventWrapper, TriggerManager, TriggerType,
};
pub use health_check::HealthStatus;
pub use security::{
    run_security_audit, AuditResult, AuditTrailEntry, CredentialIsolationStatus,
    EncryptionStatus, NetworkBoundaryStatus, SandboxConfig, SecurityAuditor,
    SecurityAuditSummary, SecurityFinding, SecurityLevel, SecuritySeverity,
};
pub use state::{AppState, MemoryConfig, OperationalHealth, initialize_app_state};
pub use task_scheduler::{
    Schedule, ScheduledTask, SchedulerConfig, SchedulerStats, TaskAction,
    TaskConfig, TaskExecutor, TaskExecutionResult, TaskScheduler, TaskSchedulerError,
    TaskStatus, start_scheduler,
};
pub use agent_coordinator::{
    Agent, AgentCapabilities, AgentConfig, AgentStatus, AgentType, Coordinator,
    CoordinatorConfig, CoordinatorError, CoordinationMode, CoordinatorStats,
    Task, TaskResult as AgentTaskResult,
};
pub use execution_mode::{
    AuditLevel, CapabilityRequirements, ExecutionMode, ExecutionModeConfig, Operation,
    OperationStats, PermissionChecker, PermissionError, PermissionResult, PermissionSet,
    RiskLevel, ToolCapabilityRegistry,
};
