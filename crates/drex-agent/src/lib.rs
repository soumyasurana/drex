//! Drex Agent - Planning and Execution Orchestration
//!
//! This crate provides the agent loop implementation for Drex, including:
//! - Planning (natural language plan generation)
//! - Step-to-tool translation
//! - Plan execution with observation and replanning
//! - Memory integration
//! - Structured decision making
//!
//! # Architecture
//!
//! The agent is built on top of:
//! - `drex-models`: For model routing and backend abstraction
//! - `drex-tools`: For tool execution via ToolRegistry
//! - `drex-memory`: For context retrieval and memory storage

#![doc = include_str!("../README.md")]

pub mod agent;
pub mod agent_error;
pub mod context;
pub mod decision;
pub mod executor;
pub mod planner;
pub mod run_state;
pub mod security_audit;

pub use agent::{
    Agent, AgentConfig, AgentError, AgentResult, AgentTrace, Observation, TraceEntry,
};
pub use agent_error::{
    AgentErrorDetail, AgentResultDetail, ConfigError, ContextErrorKind, DecisionErrorKind,
    ErrorAction, ErrorHandler, ErrorKind, ErrorMetrics, ErrorThresholdConfig, ModelError,
    PlanningError, SecurityError, Severity, StateError, ToolErrorKind,
};
pub use context::{
    AssembledContext, ContextEngine, ContextEngineConfig, ContextError, ContextSection,
    PrioritizedItem, TokenBudget, TruncationStrategy,
};
pub use decision::{
    AgentDecision, ContinueDecision, DecisionError, DecisionValidator, FailureDecision,
    FinalAnswerDecision, ReplanDecision, ToolCallDecision,
};
pub use executor::{ExecutionError, StepExecutor, StepTranslation, ToolCall, ValidationResult};
pub use security_audit::{
    checks, SecurityAuditor, SecurityCategory, SecurityReport, SecurityTest, PROMPT_INJECTION_PAYLOADS,
    TOOL_INJECTION_PAYLOADS,
};
pub use planner::{Plan, PlanStep, Planner, PlannerError};
pub use run_state::{
    InMemoryRunStateStore, RunFilter, RunId, RunProgress, RunState, RunStateError, RunStateStore,
    RunStatus, RunStep,
};
