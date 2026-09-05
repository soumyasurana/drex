//! Drex Core - Main application runtime
//!
//! Drex Core is responsible for:
//! - Configuration loading
//! - Logging/tracing initialization
//! - Health checks for all backends
//! - Application state management
//! - Memory system integration
//! - Graceful shutdown handling

pub mod event_bus;
pub mod health_check;
pub mod security;
pub mod state;

/// Re-export commonly used types
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
