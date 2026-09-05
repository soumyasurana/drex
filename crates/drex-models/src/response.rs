//! Model response types

use crate::content::{Content, ToolCall};
use serde::{Deserialize, Serialize};

/// A response from a model backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelResponse {
    /// Unique identifier for this response.
    pub id: String,
    /// The model that generated this response.
    pub model: String,
    /// Provider that generated this response (e.g., "openai", "anthropic").
    pub provider: String,
    /// The generated content.
    pub content: Content,
    /// Tool calls requested by the model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Reason the generation finished.
    pub finish_reason: FinishReason,
    /// Token usage information.
    pub usage: Option<TokenUsage>,
    /// When the response was created (Unix timestamp).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

impl ModelResponse {
    /// Create a new model response.
    pub fn new(
        id: impl Into<String>,
        model: impl Into<String>,
        provider: impl Into<String>,
        content: Content,
    ) -> Self {
        Self {
            id: id.into(),
            model: model.into(),
            provider: provider.into(),
            content,
            tool_calls: Vec::new(),
            finish_reason: FinishReason::Stop,
            usage: None,
            created_at: None,
        }
    }

    /// Add a tool call to the response.
    pub fn with_tool_call(mut self, tool_call: ToolCall) -> Self {
        self.tool_calls.push(tool_call);
        self
    }

    /// Set the finish reason.
    pub fn with_finish_reason(mut self, reason: FinishReason) -> Self {
        self.finish_reason = reason;
        self
    }

    /// Set token usage.
    pub fn with_usage(mut self, usage: TokenUsage) -> Self {
        self.usage = Some(usage);
        self
    }

    /// Set creation timestamp.
    pub fn with_created_at(mut self, timestamp: i64) -> Self {
        self.created_at = Some(timestamp);
        self
    }

    /// Check if the model wants to call tools.
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    /// Check if the response is complete (not truncated, content-filtered, etc.).
    pub fn is_complete(&self) -> bool {
        matches!(self.finish_reason, FinishReason::Stop | FinishReason::ToolCalls)
    }

    /// Get the text content if available.
    pub fn text(&self) -> Option<String> {
        Some(self.content.to_text())
    }

    /// Get text content, returning empty string if not text.
    pub fn text_or_default(&self) -> String {
        self.text().unwrap_or_default()
    }
}

/// Token usage statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens in the prompt.
    pub prompt_tokens: u32,
    /// Tokens in the completion.
    pub completion_tokens: u32,
    /// Total tokens used.
    pub total_tokens: u32,
}

impl TokenUsage {
    /// Create token usage with all fields.
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        }
    }

    /// Create token usage with just total (when detailed breakdown unavailable).
    pub fn total_only(total_tokens: u32) -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: total_tokens,
            total_tokens,
        }
    }

    /// Check if this has detailed token counts.
    pub fn has_details(&self) -> bool {
        self.prompt_tokens > 0 || (self.completion_tokens > 0 && self.total_tokens > 0)
    }
}

/// Reason why the model stopped generating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// The model completed the response naturally.
    Stop,
    /// The response hit the maximum token limit.
    Length,
    /// The model called one or more tools.
    ToolCalls,
    /// The response was filtered by content policy.
    ContentFilter,
    /// The model is waiting for user confirmation/prompt.
    AwaitingPrompt,
    /// The response was incomplete or interrupted.
    Incomplete,
    /// Unknown or provider-specific reason.
    Unknown,
}

impl std::fmt::Display for FinishReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stop => write!(f, "stop"),
            Self::Length => write!(f, "length"),
            Self::ToolCalls => write!(f, "tool_calls"),
            Self::ContentFilter => write!(f, "content_filter"),
            Self::AwaitingPrompt => write!(f, "awaiting_prompt"),
            Self::Incomplete => write!(f, "incomplete"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl Default for FinishReason {
    fn default() -> Self {
        Self::Stop
    }
}

impl FinishReason {
    /// Check if the finish reason indicates a tool call.
    pub fn is_tool_call(&self) -> bool {
        matches!(self, Self::ToolCalls)
    }

    /// Check if the response was truncated.
    pub fn is_truncated(&self) -> bool {
        matches!(self, Self::Length)
        || matches!(self, Self::ContentFilter)
        || matches!(self, Self:: Incomplete)
    }
}

/// A chunk of a streaming response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Unique identifier for this chunk (same as response id).
    pub id: String,
    /// The model.
    pub model: String,
    /// Delta content (incremental text/tool calls).
    pub delta: ContentDelta,
    /// Whether this is the final chunk.
    pub done: bool,
    /// Finish reason if this is the final chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<FinishReason>,
    /// Usage information (often only present on last chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

/// Delta content for streaming.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ContentDelta {
    /// Role (usually only present on first chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<crate::request::Role>,
    /// Text content delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool call deltas.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallDelta>,
}

impl ContentDelta {
    /// Create an empty delta.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create a text delta.
    pub fn text<S: Into<String>>(text: S) -> Self {
        Self {
            content: Some(text.into()),
            ..Default::default()
        }
    }

    /// Check if this delta is empty.
    pub fn is_empty(&self) -> bool {
        self.content.is_none() && self.role.is_none() && self.tool_calls.is_empty()
    }
}

/// Delta for tool calls in streaming.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Index of this tool call.
    pub index: u32,
    /// Tool call ID (may be None on partial chunks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Function name (may be None on partial chunks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    /// Function arguments JSON (may be partial).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::FunctionCall;

    #[test]
    fn model_response_builder() {
        let response = ModelResponse::new(
            "resp_123",
            "gpt-4",
            "openai",
            Content::text("Hello!"),
        )
        .with_finish_reason(FinishReason::Stop);

        assert_eq!(response.id, "resp_123");
        assert_eq!(response.model, "gpt-4");
        assert_eq!(response.provider, "openai");
        assert_eq!(response.text(), Some("Hello!".to_string()));
        assert_eq!(response.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn response_with_tool_calls() {
        let tool_call = ToolCall {
            id: "call_1".to_string(),
            function: FunctionCall {
                name: "get_weather".to_string(),
                arguments: r#"{"location": "SF"}"#.to_string(),
            },
        };

        let response = ModelResponse::new("id", "model", "provider", Content::text(""))
            .with_tool_call(tool_call)
            .with_finish_reason(FinishReason::ToolCalls);

        assert!(response.has_tool_calls());
        assert_eq!(response.tool_calls.len(), 1);
        assert!(response.finish_reason.is_tool_call());
    }

    #[test]
    fn token_usage_new() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
        assert!(usage.has_details());
    }

    #[test]
    fn token_usage_total_only() {
        let usage = TokenUsage::total_only(100);
        assert_eq!(usage.total_tokens, 100);
        assert_eq!(usage.prompt_tokens, 0);
        // total_only sets completion_tokens = total_tokens which passes has_details()
        assert!(usage.has_details());
    }

    #[test]
    fn finish_reason_display() {
        assert_eq!(FinishReason::Stop.to_string(), "stop");
        assert_eq!(FinishReason::Length.to_string(), "length");
        assert_eq!(FinishReason::ToolCalls.to_string(), "tool_calls");
    }

    #[test]
    fn finish_reason_is_tool_call() {
        assert!(FinishReason::ToolCalls.is_tool_call());
        assert!(!FinishReason::Stop.is_tool_call());
    }

    #[test]
    fn finish_reason_is_truncated() {
        assert!(FinishReason::Length.is_truncated());
        assert!(FinishReason::ContentFilter.is_truncated());
        assert!(FinishReason::Incomplete.is_truncated());
        assert!(!FinishReason::Stop.is_truncated());
    }

    #[test]
    fn response_is_complete() {
        let resp = ModelResponse::new("id", "model", "provider", Content::text("hi"))
            .with_finish_reason(FinishReason::Stop);
        assert!(resp.is_complete());

        let resp = ModelResponse::new("id", "model", "provider", Content::text(""))
            .with_finish_reason(FinishReason::ToolCalls);
        assert!(resp.is_complete());

        let resp = ModelResponse::new("id", "model", "provider", Content::text(""))
            .with_finish_reason(FinishReason::Length);
        assert!(!resp.is_complete());
    }

    #[test]
    fn content_delta() {
        let delta = ContentDelta::text("Hello");
        assert_eq!(delta.content, Some("Hello".to_string()));
        assert!(!delta.is_empty());

        let empty = ContentDelta::empty();
        assert!(empty.is_empty());
    }

    #[test]
    fn serialization_roundtrip() {
        let response = ModelResponse::new("id", "model", "provider", Content::text("test"))
            .with_usage(TokenUsage::new(10, 5));

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ModelResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(response.id, deserialized.id);
        assert_eq!(response.model, deserialized.model);
        assert_eq!(response.content.to_text(), deserialized.content.to_text());
    }
}
