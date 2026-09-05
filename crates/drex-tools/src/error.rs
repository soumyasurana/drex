//! Error types for the tool system

use crate::capability::Capability;
use std::fmt;

/// Errors that can occur in the tool system.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    /// Tool with this name is already registered
    Duplicate(String),
    /// Requested tool is not found in the registry
    NotFound(String),
    /// Tool execution failed
    ExecutionFailed { tool: String, reason: String },
    /// Invalid input for the tool
    InvalidInput { tool: String, reason: String },
    /// JSON parsing/serialization error
    Serialization(String),
    /// Tool execution not authorized - missing required capabilities
    Unauthorized {
        tool: String,
        missing: Vec<Capability>,
    },
}

impl ToolError {
    /// Create an unauthorized error for a tool with missing capabilities.
    pub fn unauthorized(tool: impl Into<String>, missing: Vec<Capability>) -> Self {
        Self::Unauthorized {
            tool: tool.into(),
            missing,
        }
    }

    /// Check if this error is an authorization error.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized { .. })
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(name) => write!(f, "tool '{}' is already registered", name),
            Self::NotFound(name) => {
                write!(f, "tool '{}' not found in registry", name)
            }
            Self::ExecutionFailed { tool, reason } => {
                write!(f, "tool '{}' execution failed: {}", tool, reason)
            }
            Self::InvalidInput { tool, reason } => {
                write!(f, "invalid input for tool '{}': {}", tool, reason)
            }
            Self::Serialization(msg) => write!(f, "serialization error: {}", msg),
            Self::Unauthorized { tool, missing } => {
                let caps: Vec<_> = missing.iter().map(|c| c.as_str()).collect();
                write!(
                    f,
                    "tool '{}' execution not authorized - missing capabilities: {}",
                    tool,
                    caps.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for ToolError {}

/// Type alias for tool results
pub type ToolResult<T> = Result<T, ToolError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_duplicate() {
        let err = ToolError::Duplicate("test".to_string());
        assert!(err.to_string().contains("test"));
        assert!(err.to_string().contains("already registered"));
    }

    #[test]
    fn error_display_not_found() {
        let err = ToolError::NotFound("unknown".to_string());
        assert!(err.to_string().contains("unknown"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn error_display_execution_failed() {
        let err = ToolError::ExecutionFailed {
            tool: "test".to_string(),
            reason: "something went wrong".to_string(),
        };
        assert!(err.to_string().contains("test"));
        assert!(err.to_string().contains("execution failed"));
        assert!(err.to_string().contains("something went wrong"));
    }

    #[test]
    fn error_display_invalid_input() {
        let err = ToolError::InvalidInput {
            tool: "echo".to_string(),
            reason: "missing required field".to_string(),
        };
        assert!(err.to_string().contains("echo"));
        assert!(err.to_string().contains("invalid input"));
        assert!(err.to_string().contains("missing required field"));
    }

    #[test]
    fn error_display_serialization() {
        let err = ToolError::Serialization("invalid json".to_string());
        assert!(err.to_string().contains("serialization error"));
        assert!(err.to_string().contains("invalid json"));
    }

    #[test]
    fn error_display_unauthorized() {
        let err = ToolError::unauthorized(
            "file_read",
            vec![Capability::FileSystemRead, Capability::FileSystemWrite],
        );
        let msg = err.to_string();
        assert!(msg.contains("file_read"));
        assert!(msg.contains("not authorized"));
        assert!(msg.contains("filesystem.read"));
        assert!(msg.contains("filesystem.write"));
    }

    #[test]
    fn error_is_unauthorized() {
        let auth_err = ToolError::unauthorized("test", vec![]);
        assert!(auth_err.is_unauthorized());

        let other_err = ToolError::NotFound("test".to_string());
        assert!(!other_err.is_unauthorized());
    }
}
