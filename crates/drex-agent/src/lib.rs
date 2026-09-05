//! Drex Agent - Planning and Execution Orchestration
//!
//! This crate provides the agent loop implementation for Drex, including:
//! - Planning (natural language plan generation)
//! - Step-to-tool translation
//! - Plan execution with observation and replanning
//! - Memory integration
//!
//! # Architecture
//!
//! The agent is built on top of:
//! - `drex-models`: For model routing and backend abstraction
//! - `drex-tools`: For tool execution via ToolRegistry
//! - `drex-memory`: For context retrieval and memory storage

#![doc = include_str!("../README.md")]

pub mod agent;
pub mod executor;
pub mod planner;

pub use agent::{
    Agent, AgentConfig, AgentError, AgentResult, AgentTrace, Observation, TraceEntry,
};
pub use executor::{ExecutionError, StepExecutor, StepTranslation, ToolCall, ValidationResult};
pub use planner::{Plan, PlanStep, Planner, PlannerError};
