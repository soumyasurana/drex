//! Computer Control Tool - Safe computer interaction with explicit authorization
//!
//! This tool provides access to computer control operations (mouse, keyboard,
//! screen capture) only when the --allow-control flag is explicitly provided.
//! Without this flag, the tool returns an authorization error.

use crate::{
    capability::CapabilitySet,
    error::{ToolError, ToolResult},
    result::ExecutionResult,
    schema::{ToolSchema},
    tool::{Tool, ToolContext, ToolInput},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Configuration for computer control authorization.
#[derive(Debug, Clone)]
pub struct ComputerControlConfig {
    /// Whether control is explicitly authorized.
    authorized: bool,
    /// Operation timeout in seconds.
    timeout_seconds: u64,
}

impl ComputerControlConfig {
    /// Create a new config requiring authorization.
    pub fn new() -> Self {
        Self {
            authorized: false,
            timeout_seconds: 30,
        }
    }

    /// Authorize computer control.
    pub fn authorize(mut self) -> Self {
        self.authorized = true;
        self
    }

    /// Check if control is authorized.
    pub fn is_authorized(&self) -> bool {
        self.authorized
    }

    /// Get the timeout.
    pub fn timeout(&self) -> u64 {
        self.timeout_seconds
    }
}

impl Default for ComputerControlConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Input for click operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickInput {
    /// X coordinate
    pub x: i32,
    /// Y coordinate
    pub y: i32,
    /// Button (left, right, middle)
    #[serde(default = "default_button")]
    pub button: String,
}

fn default_button() -> String {
    "left".to_string()
}

/// Output from click operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickOutput {
    /// Whether the operation succeeded
    pub success: bool,
    /// Message describing the result
    pub message: String,
}

/// Input for type operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeInput {
    /// Text to type
    pub text: String,
}

/// Output from type operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeOutput {
    /// Whether the operation succeeded
    pub success: bool,
    /// Number of characters typed
    pub characters_typed: usize,
    /// Message describing the result
    pub message: String,
}

/// Input for screen capture operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureInput {
    /// Optional region to capture (defaults to full screen)
    pub region: Option<CaptureRegion>,
}

/// Region for screen capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureRegion {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Output from screen capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureOutput {
    /// Whether the operation succeeded
    pub success: bool,
    /// Base64 encoded image data
    pub image_data: Option<String>,
    /// Message describing the result
    pub message: String,
}

/// Unified computer control input
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ComputerControlInput {
    Click(ClickInput),
    Type(TypeInput),
    CaptureScreen(CaptureInput),
}

/// Unified computer control output
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ComputerControlOutput {
    Click(ClickOutput),
    Type(TypeOutput),
    CaptureScreen(CaptureOutput),
}

/// Computer control tool that requires explicit authorization.
pub struct ComputerControlTool {
    config: ComputerControlConfig,
    metadata: crate::tool::ToolMetadata,
}

impl ComputerControlTool {
    /// Create a new computer control tool.
    pub fn new(config: ComputerControlConfig) -> Self {
        use crate::schema::ToolSchema;
        let schema = ToolSchema::builder("ComputerControlInput", "Input for computer control actions")
            .required_string("action", "The control action to perform: click, type, or capture_screen")
            .build();

        let metadata = crate::tool::ToolMetadata::new(
            "computer_control",
            "Control the computer (mouse, keyboard, screen capture). Requires --allow-control flag.",
            schema,
        );

        Self { config, metadata }
    }

    /// Check authorization and return error if not authorized.
    fn check_authorization(&self) -> ToolResult<()> {
        if !self.config.is_authorized() {
            return Err(ToolError::ExecutionFailed {
                tool: "computer_control".to_string(),
                reason: "Computer control not authorized. Use --allow-control flag to enable.".to_string(),
            });
        }
        Ok(())
    }

    /// Execute click operation (placeholder implementation).
    async fn execute_click(&self, input: ClickInput) -> ToolResult<ClickOutput> {
        // In a real implementation, this would use drex-vision's ComputerController
        // For now, we return a placeholder response indicating authorization is working
        Ok(ClickOutput {
            success: true,
            message: format!(
                "Click at ({}, {}) with '{}' button (placeholder - not actually executed)",
                input.x, input.y, input.button
            ),
        })
    }

    /// Execute type operation (placeholder implementation).
    async fn execute_type(&self, input: TypeInput) -> ToolResult<TypeOutput> {
        Ok(TypeOutput {
            success: true,
            characters_typed: input.text.len(),
            message: format!(
                "Typed {} characters (placeholder - not actually executed)",
                input.text.len()
            ),
        })
    }

    /// Execute screen capture (placeholder implementation).
    async fn execute_capture(&self, _input: CaptureInput) -> ToolResult<CaptureOutput> {
        Ok(CaptureOutput {
            success: true,
            image_data: None,
            message: "Screen capture (placeholder - not actually executed)".to_string(),
        })
    }
}

#[async_trait]
impl Tool for ComputerControlTool {
    fn metadata(&self) -> &crate::tool::ToolMetadata {
        &self.metadata
    }

    fn required_capabilities(&self) -> &CapabilitySet {
        // This tool requires ComputerControl capability
        use crate::capability::Capability;
        use std::sync::OnceLock;
        static CAPS: OnceLock<CapabilitySet> = OnceLock::new();
        CAPS.get_or_init(|| {
            let mut caps = CapabilitySet::new();
            caps.add(Capability::ComputerControl);
            caps
        })
    }

    async fn execute(&self, _ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        // First check authorization
        self.check_authorization()?;

        // Get the action field
        let action = input.get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput {
                tool: "computer_control".to_string(),
                reason: "Missing 'action' field. Use 'click', 'type', or 'capture_screen'".to_string(),
            })?;

        // Parse input based on action
        let output = match action {
            "click" => {
                let click_input: ClickInput = input.parse().map_err(|e| ToolError::InvalidInput {
                    tool: "computer_control".to_string(),
                    reason: format!("Invalid click input: {}", e),
                })?;
                ComputerControlOutput::Click(self.execute_click(click_input).await?)
            }
            "type" => {
                let type_input: TypeInput = input.parse().map_err(|e| ToolError::InvalidInput {
                    tool: "computer_control".to_string(),
                    reason: format!("Invalid type input: {}", e),
                })?;
                ComputerControlOutput::Type(self.execute_type(type_input).await?)
            }
            "capture_screen" => {
                let capture_input: CaptureInput = input.parse().map_err(|e| ToolError::InvalidInput {
                    tool: "computer_control".to_string(),
                    reason: format!("Invalid capture input: {}", e),
                })?;
                ComputerControlOutput::CaptureScreen(self.execute_capture(capture_input).await?)
            }
            _ => {
                return Err(ToolError::InvalidInput {
                    tool: "computer_control".to_string(),
                    reason: format!("Unknown action: {}. Use 'click', 'type', or 'capture_screen'", action),
                });
            }
        };

        Ok(ExecutionResult::success(json!(output)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unauthorized_is_blocked() {
        let tool = ComputerControlTool::new(ComputerControlConfig::new());
        let input = ToolInput::from_json(json!({
            "action": "click",
            "x": 100,
            "y": 100
        })).unwrap();
        let ctx = ToolContext::new();

        let result = tool.execute(&ctx, input).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("--allow-control"));
    }

    #[tokio::test]
    async fn test_authorized_allows_click() {
        let tool = ComputerControlTool::new(ComputerControlConfig::new().authorize());
        let input = ToolInput::from_json(json!({
            "action": "click",
            "x": 100,
            "y": 100
        })).unwrap();
        let ctx = ToolContext::new();

        let result = tool.execute(&ctx, input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_authorized_allows_type() {
        let tool = ComputerControlTool::new(ComputerControlConfig::new().authorize());
        let input = ToolInput::from_json(json!({
            "action": "type",
            "text": "Hello World"
        })).unwrap();
        let ctx = ToolContext::new();

        let result = tool.execute(&ctx, input).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        let data = output.data.unwrap();
        // Parse the output to verify
        let typed: ComputerControlOutput = serde_json::from_value(data).unwrap();
        match typed {
            ComputerControlOutput::Type(out) => {
                assert_eq!(out.characters_typed, 11);
            }
            _ => panic!("Expected Type output"),
        }
    }

    #[tokio::test]
    async fn test_authorized_allows_capture() {
        let tool = ComputerControlTool::new(ComputerControlConfig::new().authorize());
        let input = ToolInput::from_json(json!({"action": "capture_screen"})).unwrap();
        let ctx = ToolContext::new();

        let result = tool.execute(&ctx, input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_unknown_action_rejected() {
        let tool = ComputerControlTool::new(ComputerControlConfig::new().authorize());
        let input = ToolInput::from_json(json!({"action": "unknown_action"})).unwrap();
        let ctx = ToolContext::new();

        let result = tool.execute(&ctx, input).await;
        assert!(result.is_err());
    }
}
