//! Ollama backend implementation for the Drex model interface.
//!
//! This module provides an implementation of the `ModelBackend` trait
//! that communicates with a local Ollama server over HTTP.
//!
//! The client communicates with:
//! - `POST /api/generate` for simple text generation
//! - `POST /api/chat` for chat-style conversations
//!
//! # Tool Calling
//!
//! Ollama supports tool calling for models that support it. This backend
//! implements proper tool call mapping but behavior depends on the
//! specific model being used.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{
    capabilities::BackendCapability,
    content::{Content, FunctionCall, ToolCall, ToolDefinition},
    error::ModelError,
    request::{GenerationConfig, Message, ModelRequest, Role},
    response::{FinishReason, ModelResponse, TokenUsage},
    ModelBackend,
};

/// Configuration for the Ollama backend.
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    /// Base URL for the Ollama server.
    pub base_url: String,
    /// Default model to use.
    pub model: String,
    /// Request timeout.
    pub timeout: Duration,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "gemma3:4b".to_string(),
            timeout: Duration::from_secs(120),
        }
    }
}

/// Ollama model backend client.
///
/// This client communicates with a local Ollama server to generate
/// completions using locally hosted models.
pub struct OllamaBackend {
    /// HTTP client.
    client: Client,
    /// Configuration.
    config: OllamaConfig,
}

impl OllamaBackend {
    /// Create a new Ollama backend with the given configuration.
    ///
    /// # Arguments
    /// * `config` - Configuration for the Ollama backend
    pub fn new(config: OllamaConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to build HTTP client");
        Self::with_client(config, client)
    }

    /// Create a new Ollama backend with a custom HTTP client.
    ///
    /// This is useful for testing with mocked HTTP clients.
    pub fn with_client(config: OllamaConfig, client: Client) -> Self {
        Self { client, config }
    }

    /// Create from drex-config OllamaConfig.
    pub fn from_drex_config(config: &drex_config::OllamaConfig) -> Self {
        Self::new(OllamaConfig {
            base_url: config.base_url.clone(),
            model: config.default_model.clone(),
            timeout: Duration::from_secs(config.timeout_seconds),
        })
    }

    /// Get the chat API URL.
    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.config.base_url.trim_end_matches('/'))
    }

    /// Get the generate API URL.
    fn generate_url(&self) -> String {
        format!("{}/api/generate", self.config.base_url.trim_end_matches('/'))
    }

    /// Convert Drex messages to Ollama format.
    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => OllamaRole::System,
                    Role::User => OllamaRole::User,
                    Role::Assistant => OllamaRole::Assistant,
                    Role::Tool => OllamaRole::Tool,
                };

                let content = m
                    .content
                    .as_ref()
                    .map(|c| c.to_text())
                    .unwrap_or_default();

                OllamaMessage {
                    role,
                    content,
                    // Tool calls in messages are only present in assistant responses,
                    // we don't send them to the API (the API returns them to us)
                    tool_calls: None,
                    tool_call_id: m.tool_call_id.clone(),
                }
            })
            .collect()
    }

    /// Convert Drex tool definitions to Ollama format.
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Option<Vec<OllamaTool>> {
        if tools.is_empty() {
            None
        } else {
            Some(tools.iter().map(|t| t.into()).collect())
        }
    }

    /// Map Ollama response to Drex ModelResponse.
    fn map_response(&self, ollama_resp: OllamaChatResponse, model: String) -> ModelResponse {
        let content = Content::text(&ollama_resp.message.content);

        // Map tool calls if present
        let tool_calls: Vec<ToolCall> = ollama_resp
            .message
            .tool_calls
            .map(|calls| calls.into_iter().map(Into::into).collect())
            .unwrap_or_default();

        // Map finish reason - Ollama reports "stop" or null
        let finish_reason = if ollama_resp.done {
            FinishReason::Stop
        } else {
            FinishReason::Unknown
        };

        // Create token usage from Ollama's eval_count and prompt_eval_count
        let usage = ollama_resp
            .prompt_eval_count
            .zip(ollama_resp.eval_count)
            .map(|(prompt_tokens, completion_tokens)| {
                TokenUsage::new(prompt_tokens, completion_tokens)
            });

        // Convert ISO 8601 timestamp to Unix timestamp
        let created_at_unix = ollama_resp.created_at.as_ref().and_then(|ts| {
            chrono::DateTime::parse_from_rfc3339(ts)
                .ok()
                .map(|dt| dt.timestamp())
        });

        ModelResponse {
            id: ollama_resp
                .created_at
                .as_ref()
                .map(|t| format!("ollama-{}", t))
                .unwrap_or_else(|| format!("ollama-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0))),
            model: ollama_resp.model.unwrap_or(model),
            provider: "ollama".to_string(),
            content,
            tool_calls,
            finish_reason,
            usage,
            created_at: created_at_unix,
        }
    }
}

#[async_trait]
impl ModelBackend for OllamaBackend {
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
        // Determine which API to use based on presence of messages
        let model = if request.model.is_empty() {
            self.config.model.clone()
        } else {
            request.model.clone()
        };

        // Always use chat API when we have messages
        let messages = if request.messages.is_empty() {
            vec![OllamaMessage {
                role: OllamaRole::User,
                content: request
                    .system
                    .clone()
                    .unwrap_or_default(),
                tool_calls: None,
                tool_call_id: None,
            }]
        } else {
            self.convert_messages(&request.messages)
        };

        let tools = self.convert_tools(&request.tools);

        let ollama_request = OllamaChatRequest {
            model,
            messages,
            tools,
            stream: Some(false),
            options: Some(RequestOptions::from(&request.parameters)),
        };

        let response = self
            .client
            .post(self.chat_url())
            .json(&ollama_request)
            .send()
            .await;

        let response = match response {
            Ok(resp) => resp,
            Err(e) if e.is_timeout() => {
                return Err(ModelError::connection(format!(
                    "Ollama request timed out after {:?}",
                    self.config.timeout
                )));
            }
            Err(e) if e.is_connect() => {
                return Err(ModelError::connection(format!(
                    "Cannot connect to Ollama at {}. Is Ollama running?",
                    self.config.base_url
                )));
            }
            Err(e) => {
                return Err(ModelError::connection(format!(
                    "HTTP request failed: {}",
                    e
                )));
            }
        };

        // Check for HTTP errors
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "(failed to read body)".to_string());

            return Err(match status.as_u16() {
                400 => ModelError::invalid_request(format!("{}: {}", status, body)),
                404 => ModelError::provider(format!("Model not found: {}", body)),
                429 => ModelError::rate_limited(format!("Rate limited: {}", body)),
                500 | 502 | 503 | 504 => ModelError::provider(format!(
                    "Ollama server error: {}",
                    body
                )),
                _ => ModelError::provider(format!("HTTP {}: {}", status, body)),
            });
        }

        // Parse response
        let response_body: OllamaChatResponse = match response.json().await {
            Ok(body) => body,
            Err(e) => {
                return Err(ModelError::serialization(format!(
                    "Failed to parse Ollama response: {}",
                    e
                )));
            }
        };

        Ok(self.map_response(response_body, request.model))
    }

    fn supports(&self, capability: BackendCapability) -> bool {
        // Ollama supports these capabilities
        matches!(
            capability,
            BackendCapability::TextGeneration
                | BackendCapability::SystemPrompt
                | BackendCapability::StopSequences
        )
        // Note: ToolCalling support varies by model - we don't universally claim it
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    async fn health_check(&self) -> crate::Result<()> {
        // Try a simple GET to the base URL
        let response = self
            .client
            .get(format!("{}/api/tags", self.config.base_url.trim_end_matches('/')))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => Ok(()),
            Ok(resp) => Err(ModelError::connection(format!(
                "Ollama returned status {}",
                resp.status()
            ))),
            Err(e) => Err(ModelError::connection(format!(
                "Cannot connect to Ollama: {}",
                e
            ))),
        }
    }
}

// ============================================================================
// Ollama API Types
// ============================================================================

/// Request body for Ollama chat API.
#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<RequestOptions>,
}

/// Ollama chat message.
#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    /// The role of the message sender.
    role: OllamaRole,
    /// The message content.
    content: String,
    /// Tool calls requested by the assistant.
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
    /// The ID of the tool call (for tool role messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Ollama role.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OllamaRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Tool in Ollama format.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OllamaFunction,
}

/// Function definition in Ollama format.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

impl From<&ToolDefinition> for OllamaTool {
    fn from(tool: &ToolDefinition) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: OllamaFunction {
                name: tool.function.name.clone(),
                description: tool.function.description.clone(),
                parameters: tool.function.parameters.clone(),
            },
        }
    }
}

/// Ollama chat response.
#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    /// Model used for generation.
    model: Option<String>,
    /// Response time (ISO 8601 timestamp string).
    created_at: Option<String>,
    /// Response message.
    message: OllamaMessage,
    /// Whether the response is complete.
    done: bool,
    /// Total time spent in nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    total_duration: Option<i64>,
    /// Time spent loading model in nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    load_duration: Option<i64>,
    /// Time spent evaluating prompt in nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_eval_duration: Option<i64>,
    /// Time spent generating response in nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    eval_duration: Option<i64>,
    /// Number of tokens in prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_eval_count: Option<u32>,
    /// Number of tokens generated.
    #[serde(skip_serializing_if = "Option::is_none")]
    eval_count: Option<u32>,
}

/// Ollama tool call.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaToolCall {
    #[serde(rename = "function")]
    function: OllamaToolCallFunction,
}

/// Function call in Ollama tool call.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct OllamaToolCallFunction {
    name: String,
    arguments: serde_json::Value,
}

impl From<OllamaToolCall> for ToolCall {
    fn from(call: OllamaToolCall) -> Self {
        ToolCall {
            // Ollama doesn't provide call IDs, so we generate a simple timestamp-based one
            id: format!("call-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            function: FunctionCall {
                name: call.function.name,
                arguments: serde_json::to_string(&call.function.arguments)
                    .unwrap_or_default(),
            },
        }
    }
}

/// Request options for Ollama.
#[derive(Debug, Serialize, Default)]
struct RequestOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i32>,
}

impl From<&GenerationConfig> for RequestOptions {
    fn from(config: &GenerationConfig) -> Self {
        Self {
            temperature: config.temperature,
            top_p: config.top_p,
            stop: if config.stop.is_empty() {
                None
            } else {
                Some(config.stop.clone())
            },
            num_predict: config.max_tokens.map(|m| m as i32),
            frequency_penalty: config.frequency_penalty,
            presence_penalty: config.presence_penalty,
            seed: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::Client;

    fn create_test_backend() -> OllamaBackend {
        let config = OllamaConfig::default();
        let client = Client::new();
        OllamaBackend::with_client(config, client)
    }

    #[test]
    fn test_config_default() {
        let config = OllamaConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.model, "gemma3:4b");
    }

    #[test]
    fn test_chat_url() {
        let backend = create_test_backend();
        assert_eq!(backend.chat_url(), "http://localhost:11434/api/chat");
    }

    #[test]
    fn test_convert_messages() {
        let backend = create_test_backend();
        let messages = vec![
            Message::system("System prompt"),
            Message::user("Hello"),
        ];

        let ollama_msgs = backend.convert_messages(&messages);
        assert_eq!(ollama_msgs.len(), 2);
        assert!(matches!(ollama_msgs[0].role, OllamaRole::System));
        assert_eq!(ollama_msgs[0].content, "System prompt");
        assert!(matches!(ollama_msgs[1].role, OllamaRole::User));
        assert_eq!(ollama_msgs[1].content, "Hello");
    }

    #[test]
    fn test_map_response() {
        let backend = create_test_backend();
        let ollama_resp = OllamaChatResponse {
            model: Some("gemma3:4b".to_string()),
            created_at: Some("2026-09-06T09:33:22.640339223Z".to_string()),
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: "Hello there".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            done: true,
            total_duration: Some(1000000000),
            load_duration: Some(500000000),
            prompt_eval_duration: Some(100000000),
            eval_duration: Some(400000000),
            prompt_eval_count: Some(10),
            eval_count: Some(20),
        };

        let response = backend.map_response(ollama_resp, "test-model".to_string());
        assert_eq!(response.model, "gemma3:4b");
        assert_eq!(response.provider, "ollama");
        assert_eq!(response.content.to_text(), "Hello there");
        assert!(matches!(response.finish_reason, FinishReason::Stop));
        assert!(response.usage.is_some());

        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 20);
    }

    #[test]
    fn test_map_tool_calls() {
        let backend = create_test_backend();
        let ollama_resp = OllamaChatResponse {
            model: Some("model".to_string()),
            created_at: None,
            message: OllamaMessage {
                role: OllamaRole::Assistant,
                content: "".to_string(),
                tool_calls: Some(vec![OllamaToolCall {
                    function: OllamaToolCallFunction {
                        name: "get_weather".to_string(),
                        arguments: serde_json::json!({"location": "San Francisco"}),
                    },
                }]),
                tool_call_id: None,
            },
            done: true,
            total_duration: None,
            load_duration: None,
            prompt_eval_duration: None,
            eval_duration: None,
            prompt_eval_count: None,
            eval_count: None,
        };

        let response = backend.map_response(ollama_resp, "model".to_string());
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].function.name, "get_weather");
    }

    #[test]
    fn test_supports() {
        let backend = create_test_backend();
        assert!(backend.supports(BackendCapability::TextGeneration));
        assert!(backend.supports(BackendCapability::SystemPrompt));
        assert!(!backend.supports(BackendCapability::Streaming));
    }

    #[test]
    fn test_request_options_from_config() {
        let config = GenerationConfig::new()
            .with_temperature(0.7)
            .with_max_tokens(100)
            .with_frequency_penalty(0.5);

        let options = RequestOptions::from(&config);
        assert_eq!(options.temperature, Some(0.7));
        assert_eq!(options.num_predict, Some(100));
        assert_eq!(options.frequency_penalty, Some(0.5));
    }

    #[tokio::test]
    async fn test_provider_name() {
        let backend = create_test_backend();
        assert_eq!(backend.provider_name(), "ollama");
    }

    #[tokio::test]
    async fn test_model() {
        let backend = create_test_backend();
        assert_eq!(backend.model(), "gemma3:4b");
    }
}
