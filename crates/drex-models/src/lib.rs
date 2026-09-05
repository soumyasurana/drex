//! Drex Models - Backend-agnostic model interface for LLM providers
//!
//! This crate defines the abstraction layer between Drex's agent logic and
//! various LLM backends. It provides:
//!
//! - `ModelBackend` trait for provider-agnostic model interaction
//! - `ModelRequest`/`ModelResponse` types for structured model I/O
//! - `GenerationParameters` for controlling model behavior
//! - `ModelError` for typed error handling
//! - `BackendCapability` for capability discovery

pub mod backend;
pub mod error;
pub mod request;
pub mod response;
pub mod parameters;
pub mod capabilities;
pub mod content;
pub mod backends;
pub mod router;

pub use backend::ModelBackend;
pub use error::ModelError;
pub use request::{ModelRequest, Role, ToolChoice};
pub use response::{ModelResponse, FinishReason, TokenUsage};
pub use parameters::GenerationParameters;
pub use capabilities::BackendCapability;
pub use content::{
    Content, ContentPart, ToolCall, ToolCallResult, ToolDefinition, FunctionDefinition,
};
pub use router::{ModelRouter, RouterConfig, TaskKind, RoutingConfig};

/// Re-export common result type for consistency
pub type Result<T> = std::result::Result<T, ModelError>;
