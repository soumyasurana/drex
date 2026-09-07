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
pub mod context_advanced;
pub mod conversation;
pub mod decision;
pub mod executor;
pub mod injection_detector;
pub mod learning;
pub mod planner;
pub mod reasoning;
pub mod run_state;
pub mod security_audit;
pub mod semantic_detector;

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
pub use context_advanced::{
    AdvancedContextAssembler, AdvancedContextConfig, AdvancedAssembledContext,
    ContextPool, ScoredContextItem, ImportanceScore, ImportanceWeights,
    ContextWindow, PoolStats,
};
pub use conversation::{
    BranchId, CompressionConfig, CompressionResult, CompressionStrategy, ConversationError,
    ConversationManager, ContextCompressor, InMemorySessionStore, LlmMessage, Message,
    MessageId, MessageMetadata, MessageRole, Session, SessionFilter, SessionId, SessionStats,
    SessionStatus, SessionStore, ToolCallInfo,
};
pub use decision::{
    AgentDecision, ContinueDecision, DecisionError, DecisionValidator, FailureDecision,
    FinalAnswerDecision, ReplanDecision, ToolCallDecision,
};
pub use executor::{ExecutionError, StepExecutor, StepTranslation, ToolCall, ValidationResult};
pub use injection_detector::{
    Action, DetectionError, DetectionResult, DetectorConfig, InjectionDetector, InjectionSeverity,
    Threat, ThreatCategory,
};
pub use semantic_detector::{
    SemanticAnalyzer, SemanticConfig, SemanticResult, IntentConfidence, Intent,
    SemanticAnomaly, AnomalyType, HybridDetector, DetectionDecision, RiskLevel,
};
pub use learning::{
    Feedback, InMemoryLearningStore, LearnedItem, LearningConfig, LearningEngine, LearningError,
    LearningEvent, LearningStats, LearningType, PreferenceId,
};
pub use planner::{Plan, PlanStep, Planner, PlannerError};
pub use reasoning::{
    ReasoningChain, ReasoningStep, Reflection, ReasoningEngine, ReasoningConfig,
    ReasoningStrategy, ReasoningScore, ReasoningPath, TreeOfThoughts,
    ReasoningStats, ReasoningError,
};
pub use security_audit::{
    checks, SecurityAuditor, SecurityCategory, SecurityReport, SecurityTest, PROMPT_INJECTION_PAYLOADS,
    TOOL_INJECTION_PAYLOADS,
};
pub use run_state::{
    InMemoryRunStateStore, RunFilter, RunId, RunProgress, RunState, RunStateError, RunStateStore,
    RunStatus, RunStep,
};
