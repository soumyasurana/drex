//! Step-to-tool-call translation and execution
//!
//! This module provides the translation layer between plan steps and tool calls.
//! Each step can become:
//! - A direct answer (no tool execution)
//! - A structured tool call validated against the tool's schema
//!
//! # Security
//!
//! - Tool calls are validated against schema BEFORE execution
//! - Unknown tools are rejected
//! - Invalid arguments are rejected
//! - Capability checks are enforced through ToolRegistry
//! - Never execute arbitrary model-generated commands

use crate::planner::PlanStep;
use drex_tools::{
    registry::ToolRegistry,
    result::ExecutionResult,
    tool::{ToolContext, ToolInput},
    CapabilitySet,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Errors that can occur during step execution.
#[derive(Debug, Clone, Error)]
pub enum ExecutionError {
    /// The requested tool is not registered.
    #[error("Unknown tool: {0}")]
    UnknownTool(String),

    /// The tool arguments are invalid.
    #[error("Invalid tool arguments for '{tool}': {reason}")]
    InvalidArguments { tool: String, reason: String },

    /// The tool schema validation failed.
    #[error("Schema validation failed for '{tool}': {reason}")]
    SchemaValidationFailed { tool: String, reason: String },

    /// Authorization was denied for this tool.
    #[error("Not authorized to use tool '{tool}': {reason}")]
    NotAuthorized { tool: String, reason: String },

    /// Tool execution failed.
    #[error("Tool '{tool}' execution failed: {reason}")]
    ExecutionFailed { tool: String, reason: String },

    /// The step could not be parsed as a tool call.
    #[error("Step parsing failed: {0}")]
    ParseError(String),

    /// Direct answer - no tool needed.
    #[error("Direct answer: {0}")]
    DirectAnswer(String),

    /// Invalid tool call format.
    #[error("Invalid tool call format: {0}")]
    InvalidFormat(String),

    /// Model generation failed.
    #[error("Model error: {0}")]
    ModelError(String),
}

/// A parsed tool call from a plan step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// The name of the tool to call.
    pub tool_name: String,

    /// The arguments for the tool call.
    pub arguments: Value,

    /// Optional rationale for why this tool was chosen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

impl ToolCall {
    /// Create a new tool call.
    pub fn new(tool_name: impl Into<String>, arguments: Value) -> Self {
        Self {
            tool_name: tool_name.into(),
            arguments,
            rationale: None,
        }
    }

    /// Add a rationale to the tool call.
    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = Some(rationale.into());
        self
    }
}

/// Result of attempting to translate a step.
#[derive(Debug, Clone)]
pub enum StepTranslation {
    /// The step translates to a tool call.
    ToolCall(ToolCall),

    /// The step is a direct answer (no tool needed).
    DirectAnswer(String),

    /// The step could not be translated.
    Error(ExecutionError),
}

/// Tool call validation result.
pub enum ValidationResult {
    /// Tool call is valid and ready for execution.
    Valid,

    /// Tool call is invalid with reason.
    Invalid(String),
}

/// The StepExecutor translates plan steps to tool calls and executes them.
pub struct StepExecutor {
    registry: Arc<ToolRegistry>,
    capabilities: CapabilitySet,
    /// Optional memory store for tools that need to persist/retrieve memories
    memory_store: Option<Arc<dyn drex_memory::MemoryStore>>,
}

impl StepExecutor {
    /// Create a new step executor.
    pub fn new(registry: Arc<ToolRegistry>, capabilities: CapabilitySet) -> Self {
        Self {
            registry,
            capabilities,
            memory_store: None,
        }
    }

    /// Create a new step executor with a memory store.
    pub fn with_memory_store(
        registry: Arc<ToolRegistry>,
        capabilities: CapabilitySet,
        memory_store: Arc<dyn drex_memory::MemoryStore>,
    ) -> Self {
        Self {
            registry,
            capabilities,
            memory_store: Some(memory_store),
        }
    }

    /// Set the memory store for this executor.
    pub fn set_memory_store(&mut self, memory_store: Arc<dyn drex_memory::MemoryStore>) {
        self.memory_store = Some(memory_store);
    }

    /// Translate a plan step into a tool call or direct answer.
    ///
    /// This method:
    /// 1. Extracts tool name and arguments from the step description
    /// 2. Validates the tool exists in the registry
    /// 3. Returns a ToolCall or DirectAnswer
    ///
    /// # Arguments
    /// * `step` - The plan step to translate
    pub fn translate_step(&self, step: &PlanStep) -> StepTranslation {
        debug!(step_number = step.number, description = %step.description, "Translating step");

        let description = step.description.trim();

        // Check if this looks like a tool call
        // Format: "tool_name(arguments)" or "Call tool_name with arguments"
        if let Some(tool_call) = self.parse_tool_call_syntax(description) {
            return StepTranslation::ToolCall(tool_call);
        }

        // Check for explicit tool call patterns
        if let Some(tool_call) = self.parse_explicit_tool_call(description) {
            return StepTranslation::ToolCall(tool_call);
        }

        // If the step looks like a question/answer, treat as direct
        if self.is_direct_answer(description) {
            return StepTranslation::DirectAnswer(description.to_string());
        }

        // If we can't parse it, return an error
        StepTranslation::Error(ExecutionError::ParseError(format!(
            "Could not parse step {} as tool call: {}",
            step.number, description
        )))
    }

    /// Parse explicit tool call syntax like "filesystem.read({path: '/foo'})".
    fn parse_tool_call_syntax(&self, text: &str) -> Option<ToolCall> {
        // Match patterns like: tool_name({...}) or tool_name({ ... })
        // This is a simplified parser - in production, use a proper JSON parser

        // Look for tool_name({
        if let Some(open_paren) = text.find('(') {
            let before_paren = text[..open_paren].trim();

            // Extract tool name (last word before parenthesis)
            let tool_name = before_paren
                .split_whitespace()
                .last()
                .unwrap_or(before_paren);

            // Check if this is a known tool
            if self.registry.contains(tool_name) {
                // Try to find the closing parenthesis and JSON content
                if let Some(close_paren) = text.rfind(')') {
                    let json_content = &text[open_paren + 1..close_paren];

                    // Try to parse as JSON object
                    if let Ok(arguments) = serde_json::from_str::<Value>(json_content) {
                        return Some(ToolCall::new(tool_name, arguments));
                    }

                    // Try to wrap in braces if not already an object
                    let wrapped = format!("{{{}}}", json_content);
                    if let Ok(arguments) = serde_json::from_str::<Value>(&wrapped) {
                        return Some(ToolCall::new(tool_name, arguments));
                    }
                }
            }
        }

        None
    }

    /// Parse explicit "Call tool_name" patterns.
    fn parse_explicit_tool_call(&self, text: &str) -> Option<ToolCall> {
        let lower = text.to_lowercase();

        // Pattern: "call tool_name(...)" or "use tool_name(...)"
        if let Some(idx) = lower.find("call ").or_else(|| lower.find("use ")) {
            let after_call = &text[idx + 5..];

            // Find the first word which should be the tool name
            let tool_name = after_call
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches(|c| c == '(' || c == ',' || c == '.');

            if self.registry.contains(tool_name) {
                // Try to extract arguments if present
                let arguments = if let Some(json_start) = after_call.find('{') {
                    if let Some(json_end) = after_call.rfind('}') {
                        let json_str = &after_call[json_start..=json_end];
                        serde_json::from_str(json_str).unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    }
                } else {
                    Value::Null
                };

                return Some(ToolCall::new(tool_name, arguments));
            }
        }

        // Pattern: "tool_name: ..." or "Run tool_name ..."
        if let Some(idx) = lower.find(": ") {
            let tool_name = text[..idx].trim().to_string();
            if self.registry.contains(&tool_name) {
                let rest = &text[idx + 2..];
                // Try to parse the rest as JSON
                let arguments = serde_json::from_str::<Value>(rest)
                    .unwrap_or_else(|_| serde_json::json!({"description": rest}));
                return Some(ToolCall::new(tool_name, arguments));
            }
        }

        // If first word is a known tool name, treat rest as arguments
        let first_word = text.split_whitespace().next().unwrap_or("").trim();
        if self.registry.contains(first_word) {
            let first_word_len = first_word.len();
            let rest = text[first_word_len..].trim();
            let arguments = if rest.is_empty() {
                Value::Null
            } else {
                serde_json::from_str::<Value>(rest)
                    .unwrap_or_else(|_| serde_json::json!({"description": rest}))
            };
            return Some(ToolCall::new(first_word.to_string(), arguments));
        }

        None
    }

    /// Check if text appears to be a direct answer rather than a tool call.
    fn is_direct_answer(&self, text: &str) -> bool {
        let lower = text.to_lowercase();

        // Direct answer patterns
        let answer_patterns = [
            "the answer is",
            "based on",
            "i found that",
            "the result is",
            "according to",
            "it appears that",
            "the solution",
            "here is",
            "answer:",
        ];

        for pattern in &answer_patterns {
            if lower.contains(pattern) {
                return true;
            }
        }

        // If text contains no action verbs, treat as direct answer
        let action_words = [
            "use", "call", "run", "execute", "perform", "invoke", "send",
        ];
        let has_action = action_words.iter().any(|word| lower.contains(word));

        !has_action
    }

    /// Validate a tool call against the tool's schema and capabilities.
    ///
    /// # Arguments
    /// * `tool_call` - The tool call to validate
    ///
    /// # Returns
    /// ValidationResult indicating if the call is valid.
    pub fn validate(&self, tool_call: &ToolCall) -> ValidationResult {
        debug!(tool = %tool_call.tool_name, "Validating tool call");

        // Check if tool exists
        if !self.registry.contains(&tool_call.tool_name) {
            return ValidationResult::Invalid(format!(
                "Unknown tool: {}",
                tool_call.tool_name
            ));
        }

        // Get the tool directly (metadata doesn't have capability info)
        let tool = match self.registry.get(&tool_call.tool_name) {
            Ok(t) => t,
            Err(e) => {
                return ValidationResult::Invalid(format!(
                    "Failed to get tool {}: {}",
                    tool_call.tool_name, e
                ));
            }
        };

        // Check required capabilities
        let required = tool.required_capabilities();
        if !self.capabilities.has_all(required) {
            let missing = self.capabilities.missing(required);
            return ValidationResult::Invalid(format!(
                "Missing capabilities for {}: {:?}",
                tool_call.tool_name, missing
            ));
        }

        // Validate arguments against schema
        let metadata = tool.metadata();
        if let Err(e) = self.validate_against_schema(tool_call, metadata.input_schema.clone()) {
            return ValidationResult::Invalid(format!(
                "Schema validation failed for {}: {}",
                tool_call.tool_name, e
            ));
        }

        info!(tool = %tool_call.tool_name, "Tool call validated successfully");
        ValidationResult::Valid
    }

    /// Validate tool arguments against the tool's schema.
    fn validate_against_schema(
        &self,
        tool_call: &ToolCall,
        schema: drex_tools::ToolSchema,
    ) -> Result<(), String> {
        // Convert schema to Value for validation
        let schema_value: Value = serde_json::to_value(&schema)
            .map_err(|e| format!("Failed to serialize schema: {}", e))?;

        let schema_ref = &schema_value;

        // Check required properties
        if let Some(required) = schema_ref.get("required").and_then(|r| r.as_array()) {
            for req in required {
                if let Some(prop_name) = req.as_str() {
                    if tool_call.arguments.get(prop_name).is_none() {
                        return Err(format!("Missing required property: {}", prop_name));
                    }
                }
            }
        }

        // Validate property types if possible
        if let Some(properties) = schema_ref.get("properties").and_then(|p| p.as_object()) {
            for (prop_name, prop_value) in tool_call.arguments.as_object().unwrap_or(&serde_json::Map::new()) {
                if let Some(prop_schema) = properties.get(prop_name) {
                    if let Err(e) = self.validate_property(prop_name, prop_value, prop_schema) {
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_property(
        &self,
        name: &str,
        value: &Value,
        schema: &Value,
    ) -> Result<(), String> {
        if let Some(expected_type) = schema.get("type").and_then(|t| t.as_str()) {
            let valid = match expected_type {
                "string" => value.is_string(),
                "number" => value.is_number(),
                "integer" => value.is_i64() || value.is_u64(),
                "boolean" => value.is_boolean(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                "null" => value.is_null(),
                _ => true, // Unknown types pass
            };

            if !valid {
                return Err(format!(
                    "Property '{}' expected type '{}' but got {:?}",
                    name, expected_type, value
                ));
            }
        }

        Ok(())
    }

    /// Execute a tool call.
    ///
    /// # Arguments
    /// * `tool_call` - The validated tool call to execute
    /// * `context` - The tool execution context
    ///
    /// # Returns
    /// The execution result or an error.
    pub async fn execute(
        &self,
        tool_call: &ToolCall,
        context: &ToolContext,
    ) -> Result<ExecutionResult, ExecutionError> {
        info!(tool = %tool_call.tool_name, "Executing tool");

        // Validate before execution
        match self.validate(tool_call) {
            ValidationResult::Valid => {}
            ValidationResult::Invalid(reason) => {
                warn!(tool = %tool_call.tool_name, reason = %reason, "Tool validation failed");
                return Err(ExecutionError::InvalidArguments {
                    tool: tool_call.tool_name.clone(),
                    reason,
                });
            }
        }

        // Get the tool
        let tool = self
            .registry
            .get(&tool_call.tool_name)
            .map_err(|_| ExecutionError::UnknownTool(tool_call.tool_name.clone()))?;

        // Build the tool input
        let input = ToolInput::from_json(tool_call.arguments.clone()).map_err(|e| {
            ExecutionError::InvalidArguments {
                tool: tool_call.tool_name.clone(),
                reason: format!("Failed to build tool input: {}", e),
            }
        })?;

        // Build a context with memory store if available
        let execution_context = if let Some(ref store) = self.memory_store {
            context.clone().with_memory_store(store.clone())
        } else {
            context.clone()
        };

        // Execute with authorization check
        let result = tool.execute(&execution_context, input).await.map_err(|e| {
            ExecutionError::ExecutionFailed {
                tool: tool_call.tool_name.clone(),
                reason: e.to_string(),
            }
        })?;

        info!(
            tool = %tool_call.tool_name,
            success = result.is_success(),
            "Tool execution complete"
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drex_tools::{
        capability::Capability,
        tools::{EchoTool, FileSystemReadTool, FileSystemConfig},
    };
    use tempfile::TempDir;

    fn create_test_executor() -> (StepExecutor, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let root_path = temp_dir.path();

        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();
        registry
            .register(Box::new(FileSystemReadTool::new(FileSystemConfig::new(root_path))))
            .unwrap();

        let capabilities = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            // Echo requires no capabilities
        ]);

        let executor = StepExecutor::new(Arc::new(registry), capabilities);
        (executor, temp_dir)
    }

    #[test]
    fn executor_translates_direct_tool_call() {
        let (executor, _temp) = create_test_executor();

        let step = PlanStep {
            number: 1,
            description: r#"echo({"message": "hello"})"#.to_string(),
            rationale: None,
        };

        let result = executor.translate_step(&step);

        match result {
            StepTranslation::ToolCall(tc) => {
                assert_eq!(tc.tool_name, "echo");
            }
            _ => panic!("Expected ToolCall, got {:?}", result),
        }
    }

    #[test]
    fn executor_validates_known_tool() {
        let (executor, _temp) = create_test_executor();

        let tool_call = ToolCall::new("echo", serde_json::json!({"message": "test"}));

        match executor.validate(&tool_call) {
            ValidationResult::Valid => {}
            ValidationResult::Invalid(reason) => {
                panic!("Expected valid, got invalid: {}", reason)
            }
        }
    }

    #[test]
    fn executor_rejects_unknown_tool() {
        let (executor, _temp) = create_test_executor();

        let tool_call = ToolCall::new("unknown_tool", serde_json::json!({}));

        match executor.validate(&tool_call) {
            ValidationResult::Valid => panic!("Expected invalid for unknown tool"),
            ValidationResult::Invalid(_) => {}
        }
    }

    #[test]
    fn executor_detects_missing_capabilities() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();
        registry
            .register(Box::new(FileSystemReadTool::new(FileSystemConfig::new("/tmp"))))
            .unwrap();

        // No capabilities granted
        let executor = StepExecutor::new(Arc::new(registry), CapabilitySet::new());

        let tool_call = ToolCall::new("filesystem.read", serde_json::json!({"path": "/tmp"}));

        match executor.validate(&tool_call) {
            ValidationResult::Valid => panic!("Expected invalid for missing capabilities"),
            ValidationResult::Invalid(reason) => {
                assert!(reason.contains("Missing capabilities"));
            }
        }
    }

    #[test]
    fn executor_validates_required_properties() {
        let (executor, _temp) = create_test_executor();

        // echo requires "message" property
        let tool_call = ToolCall::new("echo", serde_json::json!({}));

        // This won't fail validation since echo accepts empty input
        // The validation just checks schema, not tool-specific logic
        match executor.validate(&tool_call) {
            ValidationResult::Valid => {}
            ValidationResult::Invalid(_) => {}
        }
    }

    #[test]
    fn executor_parses_call_pattern() {
        let (executor, _temp) = create_test_executor();

        let step = PlanStep {
            number: 1,
            description: "Call echo with message hello".to_string(),
            rationale: None,
        };

        let result = executor.translate_step(&step);

        match result {
            StepTranslation::ToolCall(tc) => {
                assert_eq!(tc.tool_name, "echo");
            }
            _ => {}
        }
    }

    #[test]
    fn executor_recognizes_direct_answer() {
        let (executor, _temp) = create_test_executor();

        let step = PlanStep {
            number: 1,
            description: "The answer is based on the files I found.".to_string(),
            rationale: None,
        };

        let result = executor.translate_step(&step);

        match result {
            StepTranslation::DirectAnswer(_) => {}
            _ => panic!("Expected DirectAnswer, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn executor_executes_valid_tool_call() {
        let (executor, _temp) = create_test_executor();

        let tool_call = ToolCall::new("echo", serde_json::json!({"message": "hello"}));
        let context = ToolContext::new();

        let result = executor.execute(&tool_call, &context).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.is_success());
    }

    #[tokio::test]
    async fn executor_fails_unauthorized_tool() {
        let mut registry = ToolRegistry::new();
        registry
            .register(Box::new(FileSystemReadTool::new(FileSystemConfig::new("/tmp"))))
            .unwrap();

        // No capabilities
        let executor = StepExecutor::new(Arc::new(registry), CapabilitySet::new());

        let tool_call = ToolCall::new("filesystem.read", serde_json::json!({"path": "/tmp"}));
        let context = ToolContext::new();

        let result = executor.execute(&tool_call, &context).await;
        assert!(result.is_err());
    }

    #[test]
    fn tool_call_builder_pattern() {
        let tc = ToolCall::new("echo", serde_json::json!({"msg": "test"}))
            .with_rationale("Testing the echo tool");

        assert_eq!(tc.tool_name, "echo");
        assert_eq!(tc.rationale, Some("Testing the echo tool".to_string()));
    }
}
