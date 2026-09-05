//! Agent Error Taxonomy
//!
//! This module provides a structured error hierarchy for the DreX agent
//! system. Errors are organized by domain to enable:
//! - Precise error handling with match arms
//! - Rich error context for debugging
//! - User-friendly error messages
//! - Automatic error categorization for metrics/alerts
//!
//! # Error Hierarchy
//!
//! - `AgentError`: Top-level error enum (co-exists with legacy `AgentError`)
//!   - `ConfigurationError`: Agent setup/cfg issues
//!   - `ModelError`: LLM communication issues
//!   - `ToolError`: Tool execution errors
//!   - `PlanningError`: Plan generation issues
//!   - `ExecutionError`: Step execution failures
//!   - `DecisionError`: Decision processing errors
//!   - `ContextError`: Context/budget failures
//!   - `StateError`: Run state persistence failures
//!   - `SecurityError`: Policy violations
//!   - `RuntimeError`: Internal runtime errors
//!
//! # Error Classification
//!
//! All errors return an `ErrorKind` for categorization:
//! - `Transient`: May succeed on retry (network timeouts, DB locks)
//! - `Permanent`: Won't succeed without changes (bad config, invalid input)
//! - `Security`: Policy violations requiring audit
//! - `Fatal`: Internal consistency failures

use serde::{Deserialize, Serialize};
use std::fmt;

/// Agent error with context and severity.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{kind}: {message}")]
pub struct AgentErrorDetail {
    /// Error classification.
    pub kind: ErrorKind,
    /// Human-readable message.
    pub message: String,
    /// Source context (module::function).
    pub source: Option<String>,
    /// Inner error if wrapped.
    #[source]
    pub cause: Option<Box<AgentErrorDetail>>,
    /// Whether recovery is possible.
    pub recoverable: bool,
    /// Suggested action for recovery.
    pub suggestion: Option<String>,
}

impl AgentErrorDetail {
    /// Create a new error.
    pub fn new(kind: ErrorKind, message: impl fmt::Display) -> Self {
        Self {
            kind,
            message: message.to_string(),
            source: None,
            cause: None,
            recoverable: kind.default_recoverable(),
            suggestion: None,
        }
    }

    /// Add source context.
    pub fn at(mut self, source: impl fmt::Display) -> Self {
        self.source = Some(source.to_string());
        self
    }

    /// Mark as non-recoverable.
    pub fn fatal(mut self) -> Self {
        self.recoverable = false;
        self
    }

    /// Mark as recoverable with suggestion.
    pub fn recoverable_with(mut self, suggestion: impl fmt::Display) -> Self {
        self.recoverable = true;
        self.suggestion = Some(suggestion.to_string());
        self
    }

    /// Wrap another error as cause.
    pub fn caused_by(mut self, cause: impl Into<AgentErrorDetail>) -> Self {
        self.cause = Some(Box::new(cause.into()));
        self
    }

    /// Check if this is a security error.
    pub fn is_security(&self) -> bool {
        matches!(self.kind, ErrorKind::Security) || self.cause.as_ref().map_or(false, |c| c.is_security())
    }

    /// Check if error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        self.recoverable && !matches!(self.kind, ErrorKind::Fatal)
    }

    /// Get error severity for logging.
    pub fn severity(&self) -> Severity {
        match (self.kind, self.recoverable) {
            (ErrorKind::Fatal, _) => Severity::Critical,
            (ErrorKind::Security, _) => Severity::High,
            (ErrorKind::Permanent, false) => Severity::High,
            (ErrorKind::Transient, false) => Severity::Medium,
            _ => Severity::Low,
        }
    }

    /// Get full error chain message.
    pub fn full_message(&self) -> String {
        let mut msg = self.message.clone();
        if let Some(ref cause) = self.cause {
            msg.push_str("\nCaused by: ");
            msg.push_str(&cause.full_message());
        }
        msg
    }
}

/// Error classification for handling strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorKind {
    /// May succeed on retry (network timeouts, rate limits).
    Transient,
    /// Won't succeed without changes (bad config, invalid input).
    Permanent,
    /// Security/policy violations.
    Security,
    /// Fatal internal errors (bugs, invariants violated).
    Fatal,
}

impl ErrorKind {
    /// Default recoverability for this kind.
    fn default_recoverable(&self) -> bool {
        matches!(self, Self::Transient)
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient => write!(f, "TRANSIENT"),
            Self::Permanent => write!(f, "PERMANENT"),
            Self::Security => write!(f, "SECURITY"),
            Self::Fatal => write!(f, "FATAL"),
        }
    }
}

/// Error severity for logging and monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Low => write!(f, "LOW"),
            Self::Medium => write!(f, "MEDIUM"),
            Self::High => write!(f, "HIGH"),
            Self::Critical => write!(f, "CRITICAL"),
        }
    }
}

// ===== Domain-Specific Error Types =====

/// Configuration-related errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required configuration: {0}")]
    Missing(String),
    #[error("Invalid configuration value: {key} = {value}")]
    Invalid { key: String, value: String },
    #[error("Configuration not found: {0}")]
    NotFound(String),
}

/// Model/provider communication errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ModelError {
    #[error("Model unavailable: {0}")]
    Unavailable(String),
    #[error("Rate limited: retry after {seconds}s")]
    RateLimited { seconds: u64 },
    #[error("Context length exceeded: {used}/{limit}")]
    ContextExceeded { used: usize, limit: usize },
    #[error("Invalid response format: {0}")]
    InvalidResponse(String),
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Authentication failed")]
    Authentication,
    #[error("Request timeout after {ms}ms")]
    Timeout { ms: u64 },
}

/// Tool execution errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ToolErrorKind {
    #[error("Tool not found: {0}")]
    NotFound(String),
    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Invalid tool input: {0}")]
    InvalidInput(String),
    #[error("Tool result rejected by trust boundary: {0}")]
    TrustViolation(String),
    #[error("Tool timed out after {ms}ms")]
    Timeout { ms: u64 },
}

/// Planning errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PlanningError {
    #[error("Failed to generate plan: {0}")]
    GenerationFailed(String),
    #[error("Empty plan generated")]
    EmptyPlan,
    #[error("Plan contains invalid steps: {0}")]
    InvalidSteps(String),
    #[error("Memory retrieval failed: {0}")]
    MemoryFailed(String),
}

/// Security-related errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SecurityError {
    #[error("Capability not authorized: {capability}")]
    Unauthorized { capability: String },
    #[error("Content policy violation: {policy}")]
    PolicyViolation { policy: String },
    #[error("Tool trust boundary violation: {tool}")]
    TrustBoundary { tool: String },
    #[error("Authentication required")]
    AuthenticationRequired,
    #[error("Session expired")]
    SessionExpired,
}

/// Context/budget errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ContextErrorKind {
    #[error("Token budget exceeded: {used}/{budget}")]
    BudgetExceeded { used: usize, budget: usize },
    #[error("Context section too large: {section}")]
    SectionTooLarge { section: String },
    #[error("Invalid budget configuration: {0}")]
    InvalidBudget(String),
    #[error("Context truncation failed: {0}")]
    TruncationFailed(String),
}

/// State persistence errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum StateError {
    #[error("Run not found: {0}")]
    RunNotFound(String),
    #[error("State persistence failed: {0}")]
    PersistenceFailed(String),
    #[error("Concurrent modification detected")]
    ConcurrentModification,
    #[error("Invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },
}

/// Decision processing errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DecisionErrorKind {
    #[error("Invalid decision format: {0}")]
    InvalidFormat(String),
    #[error("Decision validation failed: {0}")]
    ValidationFailed(String),
    #[error("Tool authorization denied for {tool}")]
    AuthorizationDenied { tool: String },
    #[error("Decision timeout after {ms}ms")]
    Timeout { ms: u64 },
    #[error("Decision replanning required: {0}")]
    ReplanRequired(String),
}

// ===== Error Conversion Helpers =====

impl From<ConfigError> for AgentErrorDetail {
    fn from(e: ConfigError) -> Self {
        AgentErrorDetail::new(ErrorKind::Permanent, e.to_string())
            .at("agent::config")
    }
}

impl From<ModelError> for AgentErrorDetail {
    fn from(e: ModelError) -> Self {
        let kind = match &e {
            ModelError::Unavailable(_) => ErrorKind::Transient,
            ModelError::RateLimited { .. } => ErrorKind::Transient,
            ModelError::Timeout { .. } => ErrorKind::Transient,
            _ => ErrorKind::Permanent,
        };
        AgentErrorDetail::new(kind, e.to_string())
            .at("agent::model")
    }
}

impl From<SecurityError> for AgentErrorDetail {
    fn from(e: SecurityError) -> Self {
        AgentErrorDetail::new(ErrorKind::Security, e.to_string())
            .at("agent::security")
            .recoverable_with("Check capability authorization or adjust request")
    }
}

impl From<StateError> for AgentErrorDetail {
    fn from(e: StateError) -> Self {
        let kind = match &e {
            StateError::ConcurrentModification => ErrorKind::Transient,
            _ => ErrorKind::Permanent,
        };
        AgentErrorDetail::new(kind, e.to_string())
            .at("agent::state")
    }
}

// ===== Error Result Type =====

/// Specialized Result type for agent errors.
pub type AgentResultDetail<T> = Result<T, AgentErrorDetail>;

/// Error metrics for monitoring.
#[derive(Debug, Clone, Default)]
pub struct ErrorMetrics {
    /// Total errors by kind.
    pub by_kind: std::collections::HashMap<ErrorKind, u64>,
    /// Total errors by severity.
    pub by_severity: std::collections::HashMap<Severity, u64>,
    /// Total recoverable errors.
    pub recoverable_count: u64,
    /// Total non-recoverable errors.
    pub fatal_count: u64,
}

impl ErrorMetrics {
    /// Create new metrics.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an error.
    pub fn record(&mut self, error: &AgentErrorDetail) {
        *self.by_kind.entry(error.kind).or_insert(0) += 1;
        *self.by_severity.entry(error.severity()).or_insert(0) += 1;
        
        if error.recoverable {
            self.recoverable_count += 1;
        } else {
            self.fatal_count += 1;
        }
    }

    /// Total error count.
    pub fn total(&self) -> u64 {
        self.by_kind.values().sum()
    }

    /// Security error count.
    pub fn security_errors(&self) -> u64 {
        self.by_kind.get(&ErrorKind::Security).copied().unwrap_or(0)
    }

    /// Check if errors exceed threshold.
    pub fn exceeds_threshold(&self, kind: ErrorKind, threshold: u64) -> bool {
        self.by_kind.get(&kind).copied().unwrap_or(0) > threshold
    }
}

/// Error threshold configuration.
#[derive(Debug, Clone)]
pub struct ErrorThresholdConfig {
    /// Max transient errors before alerting.
    pub transient_alert_threshold: u64,
    /// Max security errors before escalating.
    pub security_escalation_threshold: u64,
    /// Max errors per minute.
    pub rate_limit: u64,
}

impl Default for ErrorThresholdConfig {
    fn default() -> Self {
        Self {
            transient_alert_threshold: 100,
            security_escalation_threshold: 5,
            rate_limit: 1000,
        }
    }
}

/// Error handler with threshold monitoring.
pub struct ErrorHandler {
    config: ErrorThresholdConfig,
    metrics: ErrorMetrics,
}

impl ErrorHandler {
    /// Create new error handler.
    pub fn new() -> Self {
        Self::with_config(ErrorThresholdConfig::default())
    }

    /// Create with configuration.
    pub fn with_config(config: ErrorThresholdConfig) -> Self {
        Self {
            config,
            metrics: ErrorMetrics::new(),
        }
    }

    /// Record and possibly alert on error.
    pub fn handle(&mut self, error: &AgentErrorDetail) -> ErrorAction {
        self.metrics.record(error);

        // Check thresholds
        if self.metrics.security_errors() > self.config.security_escalation_threshold {
            return ErrorAction::Escalate;
        }

        if error.is_security() {
            return ErrorAction::Log;
        }

        if !error.is_recoverable() {
            return ErrorAction::Fail;
        }

        ErrorAction::Continue
    }

    /// Get current metrics.
    pub fn metrics(&self) -> &ErrorMetrics {
        &self.metrics
    }
}

impl Default for ErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Actions for error handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorAction {
    /// Log and continue execution.
    Continue,
    /// Log with elevated level.
    Log,
    /// Escalate to security team.
    Escalate,
    /// Fail the operation.
    Fail,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_error_detail_creation() {
        let err = AgentErrorDetail::new(ErrorKind::Permanent, "test error");
        assert_eq!(err.kind, ErrorKind::Permanent);
        assert_eq!(err.message, "test error");
        // Permanent errors default to non-recoverable, Transient errors default to recoverable
    }

    #[test]
    fn agent_error_transient_is_recoverable() {
        let err = AgentErrorDetail::new(ErrorKind::Transient, "timeout");
        assert!(err.recoverable);
    }

    #[test]
    fn agent_error_permanent_default_not_recoverable() {
        let err = AgentErrorDetail::new(ErrorKind::Permanent, "config error");
        assert!(!err.recoverable);
    }

    #[test]
    fn agent_error_with_source() {
        let err = AgentErrorDetail::new(ErrorKind::Transient, "network failed")
            .at("agent::execute");
        assert_eq!(err.source, Some("agent::execute".to_string()));
    }

    #[test]
    fn agent_error_fatal() {
        let err = AgentErrorDetail::new(ErrorKind::Permanent, "fatal")
            .fatal();
        assert!(!err.recoverable);
    }

    #[test]
    fn agent_error_caused_by() {
        let inner = AgentErrorDetail::new(ErrorKind::Transient, "inner");
        let outer = AgentErrorDetail::new(ErrorKind::Permanent, "outer")
            .caused_by(inner);
        assert!(outer.cause.is_some());
        assert_eq!(outer.cause.unwrap().message, "inner");
    }

    #[test]
    fn model_error_transient_detection() {
        let err = AgentErrorDetail::from(ModelError::Timeout { ms: 5000 });
        assert_eq!(err.kind, ErrorKind::Transient);
    }

    #[test]
    fn model_error_permanent_detection() {
        let err = AgentErrorDetail::from(ModelError::Authentication);
        assert_eq!(err.kind, ErrorKind::Permanent);
    }

    #[test]
    fn security_error_categorization() {
        let err = AgentErrorDetail::from(SecurityError::PolicyViolation {
            policy: "content".to_string(),
        });
        assert_eq!(err.kind, ErrorKind::Security);
        assert!(err.is_security());
    }

    #[test]
    fn severity_calculation() {
        let fatal = AgentErrorDetail::new(ErrorKind::Fatal, "invariant");
        assert_eq!(fatal.severity(), Severity::Critical);

        let security = AgentErrorDetail::new(ErrorKind::Security, "policy");
        assert_eq!(security.severity(), Severity::High);

        let normal = AgentErrorDetail::new(ErrorKind::Transient, "timeout");
        assert_eq!(normal.severity(), Severity::Low);
    }

    #[test]
    fn error_metrics_recording() {
        let mut metrics = ErrorMetrics::new();
        let err = AgentErrorDetail::new(ErrorKind::Permanent, "test");
        
        metrics.record(&err);
        assert_eq!(metrics.total(), 1);
        assert_eq!(metrics.by_kind.get(&ErrorKind::Permanent), Some(&1));
    }

    #[test]
    fn error_handler_continue_on_transient() {
        let mut handler = ErrorHandler::new();
        let err = AgentErrorDetail::new(ErrorKind::Transient, "timeout");
        
        assert_eq!(handler.handle(&err), ErrorAction::Continue);
    }

    #[test]
    fn error_handler_security_alert() {
        let mut handler = ErrorHandler::new();
        
        // Exceed threshold
        for _ in 0..10 {
            handler.handle(&AgentErrorDetail::new(
                ErrorKind::Security,
                "violation"
            ));
        }
        
        let action = handler.handle(&AgentErrorDetail::new(
            ErrorKind::Security,
            "another"
        ));
        assert_eq!(action, ErrorAction::Escalate);
    }

    #[test]
    fn error_threshold_exceeded() {
        let mut metrics = ErrorMetrics::new();
        metrics.by_kind.insert(ErrorKind::Transient, 10);
        
        assert!(metrics.exceeds_threshold(ErrorKind::Transient, 5));
        assert!(!metrics.exceeds_threshold(ErrorKind::Transient, 15));
    }

    #[test]
    fn error_full_message_chain() {
        let inner = AgentErrorDetail::new(ErrorKind::Transient, "database connection failed");
        let outer = AgentErrorDetail::new(ErrorKind::Permanent, "plan execution failed")
            .caused_by(inner);
        
        let msg = outer.full_message();
        assert!(msg.contains("plan execution failed"));
        assert!(msg.contains("database connection failed"));
        assert!(msg.contains("Caused by"));
    }

    #[test]
    fn error_kind_default_recoverable() {
        assert!(ErrorKind::Transient.default_recoverable());
        assert!(!ErrorKind::Fatal.default_recoverable());
        assert!(!ErrorKind::Security.default_recoverable());
    }

    #[test]
    fn config_error_conversion() {
        let err = ConfigError::Missing("api_key".to_string());
        let detail: AgentErrorDetail = err.into();
        assert_eq!(detail.kind, ErrorKind::Permanent);
        assert!(detail.message.contains("api_key"));
    }

    #[test]
    fn state_error_concurrent_is_transient() {
        let err = StateError::ConcurrentModification;
        let detail: AgentErrorDetail = err.into();
        assert_eq!(detail.kind, ErrorKind::Transient);
    }
}
