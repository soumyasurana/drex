//! Structured Agent Decisions
//!
//! This module provides a strongly-typed, validated decision system for the agent.
//! Instead of parsing unstructured text from models, we use structured JSON
//! that maps to well-defined decision types.
//!
//! # Security
//!
//! All decisions are validated before execution to prevent:
//! - Invalid tool calls
//! - Malformed decisions
//! - Injection attacks through decision content
//!
//! # Decision Types
//!
//! - `FinalAnswer`: Provide a direct response to the user
//! - `ToolCall`: Execute a specific tool with validated arguments
//! - `Continue`: Continue to the next step (no action needed)
//! - `Replan`: Request a new plan due to failure or changed circumstances
//! - `Failure`: Report that the task cannot be completed

use crate::executor::ToolCall;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A structured decision from the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentDecision {
    /// Provide a final answer to the user.
    FinalAnswer(FinalAnswerDecision),

    /// Call a specific tool.
    ToolCall(ToolCallDecision),

    /// Continue to the next step without action.
    Continue(ContinueDecision),

    /// Request replanning with a reason.
    Replan(ReplanDecision),

    /// Report failure and terminate.
    Failure(FailureDecision),
}

/// Final answer decision - complete the task with a response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalAnswerDecision {
    /// The response to provide to the user.
    pub response: String,

    /// Whether this answer is based on tool execution results.
    #[serde(default)]
    pub based_on_results: bool,

    /// Optional metadata about the answer.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Tool call decision - execute a specific tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallDecision {
    /// The tool call to execute.
    #[serde(flatten)]
    pub tool_call: ToolCall,

    /// Expected outcome of this tool call.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_outcome: Option<String>,

    /// Whether this tool call is critical (failure should stop execution).
    #[serde(default)]
    pub critical: bool,
}

impl ToolCallDecision {
    /// Create a new tool call decision.
    pub fn new(tool_call: ToolCall) -> Self {
        Self {
            tool_call,
            expected_outcome: None,
            critical: false,
        }
    }

    /// Set the expected outcome.
    pub fn with_expected_outcome(mut self, outcome: impl Into<String>) -> Self {
        self.expected_outcome = Some(outcome.into());
        self
    }

    /// Mark as critical.
    pub fn critical(mut self) -> Self {
        self.critical = true;
        self
    }
}

/// Continue decision - proceed to the next step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContinueDecision {
    /// Optional reason for continuing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ContinueDecision {
    /// Create a new continue decision.
    pub fn new() -> Self {
        Self { reason: None }
    }

    /// With a reason.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

impl Default for ContinueDecision {
    fn default() -> Self {
        Self::new()
    }
}

/// Replan decision - request a new plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplanDecision {
    /// Reason for replanning.
    pub reason: String,

    /// Whether to preserve existing observations.
    #[serde(default = "default_true")]
    pub preserve_observations: bool,

    /// Additional context for the new plan.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

fn default_true() -> bool {
    true
}

impl ReplanDecision {
    /// Create a new replan decision.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            preserve_observations: true,
            context: None,
        }
    }

    /// With context.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Without preserving observations.
    pub fn without_preserve_observations(mut self) -> Self {
        self.preserve_observations = false;
        self
    }
}

/// Failure decision - report that the task cannot be completed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailureDecision {
    /// Reason for failure.
    pub reason: String,

    /// Whether this failure is recoverable (user might retry).
    #[serde(default)]
    pub recoverable: bool,

    /// Suggested next steps for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestions: Option<String>,
}

impl FailureDecision {
    /// Create a new failure decision.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            recoverable: false,
            suggestions: None,
        }
    }

    /// Mark as recoverable.
    pub fn recoverable(mut self) -> Self {
        self.recoverable = true;
        self
    }

    /// With suggestions.
    pub fn with_suggestions(mut self, suggestions: impl Into<String>) -> Self {
        self.suggestions = Some(suggestions.into());
        self
    }
}

impl AgentDecision {
    /// Create a final answer decision.
    pub fn final_answer(response: impl Into<String>) -> Self {
        Self::FinalAnswer(FinalAnswerDecision {
            response: response.into(),
            based_on_results: false,
            metadata: HashMap::new(),
        })
    }

    /// Create a tool call decision.
    pub fn tool_call(tool_call: ToolCall) -> Self {
        Self::ToolCall(ToolCallDecision::new(tool_call))
    }

    /// Create a continue decision.
    pub fn continue_() -> Self {
        Self::Continue(ContinueDecision::new())
    }

    /// Create a replan decision.
    pub fn replan(reason: impl Into<String>) -> Self {
        Self::Replan(ReplanDecision::new(reason))
    }

    /// Create a failure decision.
    pub fn failure(reason: impl Into<String>) -> Self {
        Self::Failure(FailureDecision::new(reason))
    }

    /// Check if this is a final answer.
    pub fn is_final_answer(&self) -> bool {
        matches!(self, Self::FinalAnswer(_))
    }

    /// Check if this is a tool call.
    pub fn is_tool_call(&self) -> bool {
        matches!(self, Self::ToolCall(_))
    }

    /// Check if this is a continue.
    pub fn is_continue(&self) -> bool {
        matches!(self, Self::Continue(_))
    }

    /// Check if this is a replan.
    pub fn is_replan(&self) -> bool {
        matches!(self, Self::Replan(_))
    }

    /// Check if this is a failure.
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failure(_))
    }

    /// Check if this decision terminates execution.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::FinalAnswer(_) | Self::Failure(_))
    }

    /// Get the tool call if this is a tool call decision.
    pub fn as_tool_call(&self) -> Option<&ToolCallDecision> {
        match self {
            Self::ToolCall(tc) => Some(tc),
            _ => None,
        }
    }

    /// Get the final answer if this is a final answer decision.
    pub fn as_final_answer(&self) -> Option<&FinalAnswerDecision> {
        match self {
            Self::FinalAnswer(fa) => Some(fa),
            _ => None,
        }
    }

    /// Get the replan decision if this is a replan.
    pub fn as_replan(&self) -> Option<&ReplanDecision> {
        match self {
            Self::Replan(r) => Some(r),
            _ => None,
        }
    }

    /// Get the failure decision if this is a failure.
    pub fn as_failure(&self) -> Option<&FailureDecision> {
        match self {
            Self::Failure(f) => Some(f),
            _ => None,
        }
    }

    /// Parse a JSON string into a decision.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Convert this decision to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Create a JSON schema description for agent decisions.
    ///
    /// This can be used in prompts to instruct models on the expected format.
    pub fn schema_description() -> &'static str {
        r#"You must respond with a JSON object following this schema:

{
  "type": "final_answer|tool_call|continue|replan|failure",
  // For final_answer:
  "response": "string - the answer to provide to the user",
  "based_on_results": "boolean - whether based on tool results",

  // For tool_call:
  "tool_name": "string - name of the tool to call",
  "arguments": { /* tool-specific arguments */ },
  "expected_outcome": "string - what you expect from this call",
  "critical": "boolean - if true, failure stops execution",

  // For continue:
  "reason": "string - optional reason for continuing",

  // For replan:
  "reason": "string - why we need a new plan",
  "preserve_observations": "boolean - keep previous observations",
  "context": "string - additional context for new plan",

  // For failure:
  "reason": "string - why the task cannot be completed",
  "recoverable": "boolean - can the user retry",
  "suggestions": "string - what should the user do instead"
}

Respond ONLY with the JSON object, no markdown formatting."#
    }
}

/// Error that can occur when parsing or validating decisions.
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionError {
    /// JSON parsing failed.
    ParseError(String),

    /// The decision type is unknown.
    UnknownType(String),

    /// Required field is missing.
    MissingField(String),

    /// Field has invalid value.
    InvalidValue { field: String, reason: String },

    /// Decision validation failed.
    ValidationFailed(String),
}

impl std::fmt::Display for DecisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(e) => write!(f, "Failed to parse decision: {}", e),
            Self::UnknownType(t) => write!(f, "Unknown decision type: {}", t),
            Self::MissingField(field) => write!(f, "Missing required field: {}", field),
            Self::InvalidValue { field, reason } => {
                write!(f, "Invalid value for field '{}': {}", field, reason)
            }
            Self::ValidationFailed(reason) => write!(f, "Validation failed: {}", reason),
        }
    }
}

impl std::error::Error for DecisionError {}

/// Validator for agent decisions.
pub struct DecisionValidator;

impl DecisionValidator {
    /// Validate a decision.
    pub fn validate(decision: &AgentDecision) -> Result<(), DecisionError> {
        match decision {
            AgentDecision::FinalAnswer(fa) => Self::validate_final_answer(fa),
            AgentDecision::ToolCall(tc) => Self::validate_tool_call(tc),
            AgentDecision::Continue(_) => Ok(()),
            AgentDecision::Replan(r) => Self::validate_replan(r),
            AgentDecision::Failure(f) => Self::validate_failure(f),
        }
    }

    fn validate_final_answer(fa: &FinalAnswerDecision) -> Result<(), DecisionError> {
        if fa.response.trim().is_empty() {
            return Err(DecisionError::InvalidValue {
                field: "response".to_string(),
                reason: "Response cannot be empty".to_string(),
            });
        }

        // Check for potentially dangerous content
        let trimmed = fa.response.trim();
        if trimmed.len() > 10000 {
            return Err(DecisionError::InvalidValue {
                field: "response".to_string(),
                reason: "Response exceeds maximum length (10000 chars)".to_string(),
            });
        }

        Ok(())
    }

    fn validate_tool_call(tc: &ToolCallDecision) -> Result<(), DecisionError> {
        if tc.tool_call.tool_name.trim().is_empty() {
            return Err(DecisionError::InvalidValue {
                field: "tool_name".to_string(),
                reason: "Tool name cannot be empty".to_string(),
            });
        }

        // Validate tool name format (alphanumeric, underscores, dots)
        if !tc
            .tool_call
            .tool_name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
        {
            return Err(DecisionError::InvalidValue {
                field: "tool_name".to_string(),
                reason: "Tool name must be alphanumeric with underscores or dots".to_string(),
            });
        }

        // Check arguments size
        let args_json = serde_json::to_string(&tc.tool_call.arguments).map_err(|e| {
            DecisionError::InvalidValue {
                field: "arguments".to_string(),
                reason: format!("Cannot serialize arguments: {}", e),
            }
        })?;

        if args_json.len() > 100_000 {
            return Err(DecisionError::InvalidValue {
                field: "arguments".to_string(),
                reason: "Arguments exceed maximum size (100KB)".to_string(),
            });
        }

        Ok(())
    }

    fn validate_replan(r: &ReplanDecision) -> Result<(), DecisionError> {
        if r.reason.trim().is_empty() {
            return Err(DecisionError::InvalidValue {
                field: "reason".to_string(),
                reason: "Replan reason cannot be empty".to_string(),
            });
        }

        Ok(())
    }

    fn validate_failure(f: &FailureDecision) -> Result<(), DecisionError> {
        if f.reason.trim().is_empty() {
            return Err(DecisionError::InvalidValue {
                field: "reason".to_string(),
                reason: "Failure reason cannot be empty".to_string(),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decision_final_answer() {
        let decision = AgentDecision::final_answer("Hello, world!");
        assert!(decision.is_final_answer());
        assert!(!decision.is_tool_call());
        assert!(decision.is_terminal());

        let fa = decision.as_final_answer().unwrap();
        assert_eq!(fa.response, "Hello, world!");
    }

    #[test]
    fn decision_tool_call() {
        let tool_call = ToolCall {
            tool_name: "echo".to_string(),
            arguments: json!({"message": "hello"}),
            rationale: None,
        };
        let decision = AgentDecision::tool_call(tool_call);

        assert!(!decision.is_final_answer());
        assert!(decision.is_tool_call());
        assert!(!decision.is_terminal());

        let tc = decision.as_tool_call().unwrap();
        assert_eq!(tc.tool_call.tool_name, "echo");
    }

    #[test]
    fn decision_replan() {
        let decision = AgentDecision::replan("Previous plan failed");
        assert!(decision.is_replan());

        let r = decision.as_replan().unwrap();
        assert_eq!(r.reason, "Previous plan failed");
        assert!(r.preserve_observations);
    }

    #[test]
    fn decision_failure() {
        let decision = AgentDecision::failure("Cannot access resource");
        assert!(decision.is_failure());
        assert!(decision.is_terminal());

        let f = decision.as_failure().unwrap();
        assert_eq!(f.reason, "Cannot access resource");
        assert!(!f.recoverable);
    }

    #[test]
    fn decision_serialization_roundtrip() {
        let original = AgentDecision::final_answer("Test response");
        let json = original.to_json().unwrap();
        let parsed = AgentDecision::from_json(&json).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn parse_tool_call_from_json() {
        let json = r#"{
            "type": "tool_call",
            "tool_name": "echo",
            "arguments": {"message": "hello"},
            "critical": true
        }"#;

        let decision: AgentDecision = serde_json::from_str(json).unwrap();
        assert!(decision.is_tool_call());

        let tc = decision.as_tool_call().unwrap();
        assert_eq!(tc.tool_call.tool_name, "echo");
        assert!(tc.critical);
    }

    #[test]
    fn parse_final_answer_from_json() {
        let json = r#"{
            "type": "final_answer",
            "response": "The answer is 42.",
            "based_on_results": true
        }"#;

        let decision: AgentDecision = serde_json::from_str(json).unwrap();
        let fa = decision.as_final_answer().unwrap();
        assert_eq!(fa.response, "The answer is 42.");
        assert!(fa.based_on_results);
    }

    #[test]
    fn validator_rejects_empty_response() {
        let decision = AgentDecision::FinalAnswer(FinalAnswerDecision {
            response: "   ".to_string(),
            based_on_results: false,
            metadata: HashMap::new(),
        });

        let result = DecisionValidator::validate(&decision);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("empty"));
    }

    #[test]
    fn validator_rejects_empty_tool_name() {
        let tool_call = ToolCall {
            tool_name: "".to_string(),
            arguments: json!({}),
            rationale: None,
        };
        let decision = AgentDecision::tool_call(tool_call);

        let result = DecisionValidator::validate(&decision);
        assert!(result.is_err());
    }

    #[test]
    fn validator_rejects_invalid_tool_name() {
        let tool_call = ToolCall {
            tool_name: "filesystem; rm -rf /".to_string(),
            arguments: json!({}),
            rationale: None,
        };
        let decision = AgentDecision::tool_call(tool_call);

        let result = DecisionValidator::validate(&decision);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("alphanumeric"));
    }

    #[test]
    fn validator_accepts_valid_decisions() {
        let decisions = vec![
            AgentDecision::final_answer("Valid response"),
            AgentDecision::continue_(),
            AgentDecision::replan("Need new plan"),
            AgentDecision::failure("Cannot proceed"),
        ];

        for decision in decisions {
            assert!(DecisionValidator::validate(&decision).is_ok());
        }
    }

    #[test]
    fn decision_schema_description_not_empty() {
        let schema = AgentDecision::schema_description();
        assert!(!schema.is_empty());
        assert!(schema.contains("type"));
        assert!(schema.contains("final_answer"));
        assert!(schema.contains("tool_call"));
    }

    #[test]
    fn tool_call_builder_pattern() {
        let tool_call = ToolCall {
            tool_name: "test".to_string(),
            arguments: json!({}),
            rationale: None,
        };

        let decision = ToolCallDecision::new(tool_call)
            .with_expected_outcome("Success")
            .critical()
            .tool_call;

        assert_eq!(decision.tool_name, "test");
    }

    #[test]
    fn replan_builder_pattern() {
        let replan = ReplanDecision::new("Failed")
            .with_context("Additional info")
            .without_preserve_observations();

        assert_eq!(replan.reason, "Failed");
        assert_eq!(replan.context, Some("Additional info".to_string()));
        assert!(!replan.preserve_observations);
    }

    #[test]
    fn failure_builder_pattern() {
        let failure = FailureDecision::new("Error")
            .recoverable()
            .with_suggestions("Try again later");

        assert!(failure.recoverable);
        assert_eq!(failure.suggestions, Some("Try again later".to_string()));
    }
}
