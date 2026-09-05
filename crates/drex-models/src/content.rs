//! Content types for messages, tool calls, and multimodal data

use serde::{Deserialize, Serialize};

/// A tool call requested by the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call.
    pub id: String,
    /// The function being called.
    pub function: FunctionCall,
}

/// A function call requested by the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Name of the function to call.
    pub name: String,
    /// Arguments as a JSON string.
    pub arguments: String,
}

impl FunctionCall {
    /// Parse the arguments as JSON.
    pub fn parse_arguments(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_str(&self.arguments)
    }
}

/// The result of a tool call to be returned to the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallResult {
    /// The ID of the tool call this result is for.
    pub tool_call_id: String,
    /// The name of the function that was called.
    pub name: String,
    /// The result content.
    pub content: String,
}

/// Definition of a tool/function available to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// The function definition.
    pub function: FunctionDefinition,
}

impl ToolDefinition {
    /// Create a new tool definition.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            function: FunctionDefinition {
                name: name.into(),
                description: Some(description.into()),
                parameters: None,
            },
        }
    }

    /// Add parameters schema (JSON Schema).
    pub fn with_parameters(mut self, parameters: serde_json::Value) -> Self {
        self.function.parameters = Some(parameters);
        self
    }
}

/// Definition of a function available to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Name of the function.
    pub name: String,
    /// Description of what the function does.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for the function parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// Content that can be part of a message.
///
/// This enum supports both simple text and structured content
/// designed for future multimodal expansion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Content {
    /// Simple text content.
    #[serde(rename = "text")]
    Text(String),
    /// Structured content with multiple parts.
    #[serde(rename = "parts")]
    Parts(Vec<ContentPart>),
}

impl Content {
    /// Create text content.
    pub fn text<S: Into<String>>(text: S) -> Self {
        Self::Text(text.into())
    }

    /// Create content from multiple parts.
    pub fn parts(parts: Vec<ContentPart>) -> Self {
        Self::Parts(parts)
    }

    /// Check if this content is empty.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(text) => text.is_empty(),
            Self::Parts(parts) => parts.is_empty(),
        }
    }

    /// Get the text representation of this content.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            _ => None,
        }
    }

    /// Convert to a single text string (concatenates all text parts).
    pub fn to_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Parts(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }
}

impl Default for Content {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl From<String> for Content {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&str> for Content {
    fn from(text: &str) -> Self {
        Self::Text(text.to_string())
    }
}

/// A part of structured content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    /// Text content part.
    #[serde(rename = "text")]
    Text { text: String },
    // Future: Image, Audio, etc.
}

impl ContentPart {
    /// Create a text content part.
    pub fn text<S: Into<String>>(text: S) -> Self {
        Self::Text { text: text.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_call_creation() {
        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "get_weather".to_string(),
                arguments: r#"{"location": "San Francisco"}"#.to_string(),
            },
        };
        assert_eq!(tool_call.id, "call_123");
        assert_eq!(tool_call.function.name, "get_weather");
    }

    #[test]
    fn function_call_parse_arguments() {
        let func_call = FunctionCall {
            name: "test".to_string(),
            arguments: r#"{"key": "value"}"#.to_string(),
        };
        let args = func_call.parse_arguments().unwrap();
        assert_eq!(args["key"], "value");
    }

    #[test]
    fn tool_definition_builder() {
        let tool = ToolDefinition::new("get_weather", "Get weather info")
            .with_parameters(serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }));
        assert_eq!(tool.function.name, "get_weather");
        assert!(tool.function.parameters.is_some());
    }

    #[test]
    fn content_text() {
        let content = Content::text("hello");
        assert_eq!(content.as_text(), Some("hello"));
        assert_eq!(content.to_text(), "hello");
    }

    #[test]
    fn content_parts() {
        let parts = vec![ContentPart::text("Hello "), ContentPart::text("World")];
        let content = Content::parts(parts);
        assert_eq!(content.to_text(), "Hello World");
    }

    #[test]
    fn content_empty() {
        let content = Content::text("");
        assert!(content.is_empty());
    }

    #[test]
    fn content_from_string() {
        let content: Content = "hello".into();
        assert_eq!(content.as_text(), Some("hello"));
    }

    #[test]
    fn tool_call_result_creation() {
        let result = ToolCallResult {
            tool_call_id: "call_123".to_string(),
            name: "get_weather".to_string(),
            content: "72°F".to_string(),
        };
        assert_eq!(result.tool_call_id, "call_123");
        assert_eq!(result.content, "72°F");
    }
}
