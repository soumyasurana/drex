//! Model request types

use crate::content::{Content, ToolDefinition};
use serde::{Deserialize, Serialize};

/// A request to a model backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    /// Identifier of the model to use.
    pub model: String,
    /// The conversation messages.
    pub messages: Vec<Message>,
    /// Generation parameters.
    pub parameters: GenerationConfig,
    /// System instructions (optional, can also be first message).
    pub system: Option<String>,
    /// Available tools/functions.
    pub tools: Vec<ToolDefinition>,
    /// How the model should choose tools.
    pub tool_choice: ToolChoice,
    /// User identifier for rate limiting/tracking.
    pub user: Option<String>,
}

impl ModelRequest {
    /// Create a new model request.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            messages: Vec::new(),
            parameters: GenerationConfig::default(),
            system: None,
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            user: None,
        }
    }

    /// Add a message to the request.
    pub fn with_message(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// Add multiple messages.
    pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
        self.messages.extend(messages);
        self
    }

    /// Set system instructions.
    pub fn with_system<S: Into<String>>(mut self, system: S) -> Self {
        self.system = Some(system.into());
        self
    }

    /// Add a tool.
    pub fn with_tool(mut self, tool: ToolDefinition) -> Self {
        self.tools.push(tool);
        self
    }

    /// Set tool choice.
    pub fn with_tool_choice(mut self, choice: ToolChoice) -> Self {
        self.tool_choice = choice;
        self
    }

    /// Set generation parameters.
    pub fn with_parameters(mut self, parameters: GenerationConfig) -> Self {
        self.parameters = parameters;
        self
    }

    /// Set user identifier.
    pub fn with_user<S: Into<String>>(mut self, user: S) -> Self {
        self.user = Some(user.into());
        self
    }

    /// Add a user message.
    pub fn user_message<S: Into<String>>(mut self, content: S) -> Self {
        self.messages.push(Message::user(Content::text(content)));
        self
    }

    /// Add an assistant message.
    pub fn assistant_message<S: Into<String>>(mut self, content: S) -> Self {
        self.messages.push(Message::assistant(Content::text(content)));
        self
    }
}

impl Default for ModelRequest {
    fn default() -> Self {
        Self {
            model: String::new(),
            messages: Vec::new(),
            parameters: GenerationConfig::default(),
            system: None,
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            user: None,
        }
    }
}

/// Generation configuration parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationConfig {
    /// Sampling temperature (0.0 to 2.0, higher = more random).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling parameter (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Maximum number of tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Sequences that will stop generation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// Penalize repeated tokens (> 1.0 = reduce repetition).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Penalize new tokens based on presence (> 1.0 = encourage new).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
}

impl GenerationConfig {
    /// Create default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature.clamp(0.0, 2.0));
        self
    }

    /// Set top_p.
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p.clamp(0.0, 1.0));
        self
    }

    /// Set max tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Add a stop sequence.
    pub fn with_stop<S: Into<String>>(mut self, stop: S) -> Self {
        self.stop.push(stop.into());
        self
    }

    /// Set frequency penalty.
    pub fn with_frequency_penalty(mut self, penalty: f32) -> Self {
        self.frequency_penalty = Some(penalty);
        self
    }

    /// Set presence penalty.
    pub fn with_presence_penalty(mut self, penalty: f32) -> Self {
        self.presence_penalty = Some(penalty);
        self
    }
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: None,
            top_p: None,
            max_tokens: None,
            stop: Vec::new(),
            frequency_penalty: None,
            presence_penalty: None,
        }
    }
}

/// The role of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System instructions.
    System,
    /// User message.
    User,
    /// Assistant message.
    Assistant,
    /// Tool result message.
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
            Self::Tool => write!(f, "tool"),
        }
    }
}

/// A message in the conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// The sender role.
    pub role: Role,
    /// The message content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    /// Tool calls (present when assistant requests tool execution).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<crate::content::ToolCall>,
    /// Tool call ID (present for tool role messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional name identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    /// Create a new message.
    pub fn new(role: Role, content: impl Into<Content>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: None,
            name: None,
        }
    }

    /// Create a system message.
    pub fn system(content: impl Into<Content>) -> Self {
        Self::new(Role::System, content)
    }

    /// Create a user message.
    pub fn user(content: impl Into<Content>) -> Self {
        Self::new(Role::User, content)
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<Content>) -> Self {
        Self::new(Role::Assistant, content)
    }

    /// Create a tool message.
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<Content>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            name: None,
        }
    }

    /// Create a message with name.
    pub fn with_name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Create a message with tool calls (for assistant role).
    pub fn with_tool_calls(mut self, tool_calls: Vec<crate::content::ToolCall>) -> Self {
        self.tool_calls = tool_calls;
        self
    }

    /// Get the text content if available.
    pub fn text_content(&self) -> Option<String> {
        self.content.as_ref().map(|c| c.to_text())
    }
}

/// How the model should choose whether to call tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    /// Let the model decide whether to call tools.
    Auto,
    /// Never call tools.
    None,
    /// The model must call one or more tools.
    Required,
}

impl Default for ToolChoice {
    fn default() -> Self {
        Self::Auto
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{ToolDefinition, Content};

    #[test]
    fn model_request_builder() {
        let request = ModelRequest::new("gpt-4")
            .with_system("You are a helpful assistant")
            .user_message("Hello")
            .with_parameters(GenerationConfig::new().with_temperature(0.7))
            .with_user("user-123");

        assert_eq!(request.model, "gpt-4");
        assert_eq!(request.system, Some("You are a helpful assistant".to_string()));
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.parameters.temperature, Some(0.7));
        assert_eq!(request.user, Some("user-123".to_string()));
    }

    #[test]
    fn message_builder() {
        let msg = Message::user("Hello");
        assert!(matches!(msg.role, Role::User));
        
        let msg = Message::system("System prompt");
        assert!(matches!(msg.role, Role::System));
        
        let msg = Message::assistant("Hi there");
        assert!(matches!(msg.role, Role::Assistant));
        
        let msg = Message::tool("call_123", "result");
        assert!(matches!(msg.role, Role::Tool));
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn generation_config_builder() {
        let config = GenerationConfig::new()
            .with_temperature(0.8)
            .with_max_tokens(100)
            .with_stop("END");

        assert_eq!(config.temperature, Some(0.8));
        assert_eq!(config.max_tokens, Some(100));
        assert_eq!(config.stop, vec!["END"]);
    }

    #[test]
    fn tool_choice_default() {
        assert_eq!(ToolChoice::default(), ToolChoice::Auto);
    }

    #[test]
    fn role_display() {
        assert_eq!(Role::System.to_string(), "system");
        assert_eq!(Role::User.to_string(), "user");
        assert_eq!(Role::Assistant.to_string(), "assistant");
        assert_eq!(Role::Tool.to_string(), "tool");
    }

    #[test]
    fn serialization_roundtrip() {
        let request = ModelRequest::new("test-model")
            .user_message("Hello");
        
        let json = serde_json::to_string(&request).unwrap();
        let deserialized: ModelRequest = serde_json::from_str(&json).unwrap();
        
        assert_eq!(request.model, deserialized.model);
        assert_eq!(request.messages.len(), deserialized.messages.len());
    }

    #[test]
    fn request_with_tools() {
        let tool = ToolDefinition::new("get_weather", "Get weather info");
        let request = ModelRequest::new("gpt-4")
            .with_tool(tool)
            .with_tool_choice(ToolChoice::Required);

        assert_eq!(request.tools.len(), 1);
        assert_eq!(request.tool_choice, ToolChoice::Required);
    }

    #[test]
    fn message_with_tool_calls() {
        let tool_call = crate::content::ToolCall {
            id: "call_1".to_string(),
            function: crate::content::FunctionCall {
                name: "test".to_string(),
                arguments: "{}".to_string(),
            },
        };
        
        let msg = Message::assistant("").with_tool_calls(vec![tool_call]);
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id, "call_1");
    }

    #[test]
    fn message_text_content() {
        let msg = Message::user(Content::text("Hello world"));
        assert_eq!(msg.text_content(), Some("Hello world".to_string()));
    }
}
