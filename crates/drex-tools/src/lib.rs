//! Drex Tools - Tool system for discovering and executing capabilities
//!
//! This crate provides the foundational tool system that allows Drex to discover
//! and execute capabilities in later phases. The design is intentionally minimal
//! and extensible to support future permission systems (Phase 3.2) without
//! requiring breaking changes.
//!
//! # Core Components
//!
//! - [`Tool`] trait: The abstraction for any executable capability
//! - [`ToolRegistry`]: Registry for discovering and looking up tools by name
//! - [`ToolContext`]: Minimal execution context (extensible for future phases)
//! - [`ToolResult`]: Structured result type supporting both data and errors
//!
//! # Design Principles
//!
//! 1. **Provider Agnostic**: Tools are independent of specific LLM providers
//! 2. **Structured Results**: Tools return typed results, not just strings
//! 3. **JSON Schema**: Tools expose their input requirements as JSON Schema
//! 4. **Async First**: All tool execution is async
//! 5. **Extensible Context**: ToolContext is minimal now but can grow
//!
//! # Example
//!
//! ```rust
//! use drex_tools::{Tool, ToolRegistry, ToolContext, ToolInput};
//! use drex_tools::tools::EchoTool;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a registry and register tools
//! let mut registry = ToolRegistry::new();
//! registry.register(Box::new(EchoTool::new()))?;
//!
//! // Create execution context
//! let ctx = ToolContext::new();
//!
//! // Execute a tool
//! let input = ToolInput::from_json(serde_json::json!({"message": "hello"}))?;
//! let result = registry.get("echo")?.execute(&ctx, input).await;
//! # Ok(())
//! # }
//! ```

pub mod capability;
pub mod error;
pub mod registry;
pub mod result;
pub mod schema;
pub mod tool;
pub mod tools;

pub use capability::{Capability, CapabilitySet};
pub use error::{ToolError, ToolResult};
pub use registry::{ToolRegistry, AuthorizedToolRegistry};
pub use result::{ExecutionResult, ExecutionStatus};
pub use schema::{JsonSchema, ToolSchema};
pub use tool::{Tool, ToolContext, ToolInput, ToolMetadata};
