//! Echo tool - a simple demonstration tool

use crate::error::{ToolError, ToolResult};
use crate::result::ExecutionResult;
use crate::schema::ToolSchema;
use crate::tool::{Tool, ToolContext, ToolInput, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A simple echo tool that returns the input message.
///
/// This is a trivial tool demonstrating the tool interface. It simply
/// echoes back the "message" field from its input.
///
/// # Example
///
/// ```
/// use drex_tools::tools::EchoTool;
/// use drex_tools::{Tool, ToolContext, ToolInput};
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let tool = EchoTool::new();
/// let ctx = ToolContext::new();
/// let input = ToolInput::from_json(serde_json::json!({"message": "hello"}))?;
/// let result = tool.execute(&ctx, input).await?;
/// assert!(result.status.is_success());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct EchoTool {
    metadata: ToolMetadata,
}

/// Input structure for the Echo tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoInput {
    /// The message to echo back
    pub message: String,
}

/// Output structure for the Echo tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoOutput {
    /// The echoed message
    pub echoed: String,
    /// Timestamp of when the echo occurred
    pub timestamp: String,
}

impl EchoTool {
    /// Create a new EchoTool
    pub fn new() -> Self {
        let schema = ToolSchema::builder("EchoInput", "Input for the echo tool")
            .required_string("message", "The message to echo back to the caller")
            .build();

        Self {
            metadata: ToolMetadata::new(
                "echo",
                "A simple echo tool that returns the input message.\n\
                \n\
                This tool demonstrates the basic tool interface and can be used for testing.\n\
                It accepts a 'message' field and returns it unchanged along with a timestamp.",
                schema,
            ),
        }
    }

    /// Execute the echo tool with a typed input
    pub async fn execute_typed(&self, input: EchoInput) -> ToolResult<EchoOutput> {
        let output = EchoOutput {
            echoed: input.message.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        Ok(output)
    }
}

impl Default for EchoTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for EchoTool {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, _ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        // Validate required field
        let message = input
            .require_string("message")
            .map_err(|e| ToolError::InvalidInput {
                tool: self.name().to_string(),
                reason: e.to_string(),
            })?;

        // Build output
        let output = EchoOutput {
            echoed: message.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        Ok(ExecutionResult::success(json!(output)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_tool_creation() {
        let tool = EchoTool::new();
        assert_eq!(tool.name(), "echo");
        assert!(tool.description().contains("echo"));
    }

    #[test]
    fn echo_tool_metadata() {
        let tool = EchoTool::new();
        let metadata = tool.metadata();

        assert_eq!(metadata.name, "echo");
        assert!(metadata.input_schema.is_required("message"));
    }

    #[tokio::test]
    async fn echo_tool_execution_success() {
        let tool = EchoTool::new();
        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({"message": "hello world"})).unwrap();

        let result = tool.execute(&ctx, input).await.unwrap();

        assert!(result.status.is_success());
        assert!(result.data().is_some());

        let data = result.data().unwrap();
        assert_eq!(data["echoed"], "hello world");
        assert!(data["timestamp"].as_str().is_some());
    }

    #[tokio::test]
    async fn echo_tool_rejects_missing_message() {
        let tool = EchoTool::new();
        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({})).unwrap();

        let result = tool.execute(&ctx, input).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput { .. }));
        assert!(err.to_string().contains("message"));
    }

    #[tokio::test]
    async fn echo_tool_rejects_non_string_message() {
        let tool = EchoTool::new();
        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({"message": 42})).unwrap();

        let result = tool.execute(&ctx, input).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn echo_tool_rejects_extra_fields() {
        let tool = EchoTool::new();
        let _ctx = ToolContext::new();
        let input =
            ToolInput::from_json(json!({"message": "hello", "extra": "field"})).unwrap();

        // Validate should reject extra fields when additional_properties is false
        let validation = tool.validate_input(&input);
        assert!(validation.is_err());
    }

    #[tokio::test]
    async fn echo_tool_typed_execution() {
        let tool = EchoTool::new();
        let input = EchoInput {
            message: "test message".to_string(),
        };

        let result = tool.execute_typed(input).await.unwrap();
        assert_eq!(result.echoed, "test message");
        assert!(!result.timestamp.is_empty());
    }

    #[test]
    fn echo_input_output_serialization() {
        let input = EchoInput {
            message: "test".to_string(),
        };
        let input_json = serde_json::to_string(&input).unwrap();
        assert!(input_json.contains("test"));

        let output = EchoOutput {
            echoed: "test".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        let output_json = serde_json::to_string(&output).unwrap();
        assert!(output_json.contains("test"));
        assert!(output_json.contains("echoed"));
        assert!(output_json.contains("timestamp"));
    }
}
