use async_trait::async_trait;
use errors::ContextraError;
use futures_util::{Stream, StreamExt};
use rand::Rng;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use settings::ProvidersSettings;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

const OPENAI_BASE_URL: &str = "https://api.openai.com";
const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
const ANTHROPIC_MESSAGES_PATH: &str = "/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_RETRIES: usize = 3;
const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_millis(250);
const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(4);

pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatChunk, ProviderError>> + Send>>;

#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError>;

    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream, ProviderError>;

    fn supports_function_calling(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(ChatRole::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(ChatRole::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(ChatRole::Assistant, content)
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: Some(content.into()),
            name: None,
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }

    pub fn new(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatTool {
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    Auto,
    None,
    Required,
    #[serde(rename = "function")]
    Function {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ChatTool>,
    #[serde(default, skip_serializing)]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            top_p: None,
            max_tokens: None,
            stop: Vec::new(),
            tools: Vec::new(),
            tool_choice: None,
            user: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub message: ChatMessage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatChunk {
    pub id: String,
    pub model: String,
    pub delta: ChatDelta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ChatRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallDelta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionCallDelta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCallDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("missing provider configuration: {0}")]
    MissingConfiguration(String),

    #[error("unsupported provider: {0}")]
    UnsupportedProvider(String),

    #[error("invalid provider request: {0}")]
    InvalidRequest(String),

    #[error("provider authentication failed: {0}")]
    Authentication(String),

    #[error("provider rate limit exceeded: {0}")]
    RateLimited(String),

    #[error("provider returned HTTP {status}: {body}")]
    HttpStatus { status: StatusCode, body: String },

    #[error("provider network error: {0}")]
    Network(String),

    #[error("provider timeout: {0}")]
    Timeout(String),

    #[error("failed to decode provider response: {0}")]
    Decode(String),

    #[error("provider stream error: {0}")]
    Stream(String),
}

impl From<ProviderError> for ContextraError {
    fn from(error: ProviderError) -> Self {
        match error {
            ProviderError::MissingConfiguration(message)
            | ProviderError::InvalidRequest(message)
            | ProviderError::UnsupportedProvider(message) => Self::Validation(message),
            ProviderError::Authentication(message) => Self::Unauthorized(message),
            ProviderError::RateLimited(message) => Self::RateLimited(message),
            ProviderError::HttpStatus { status, body } if status == StatusCode::UNAUTHORIZED => {
                Self::Unauthorized(body)
            }
            ProviderError::HttpStatus { status, body } if status == StatusCode::FORBIDDEN => {
                Self::Forbidden(body)
            }
            ProviderError::HttpStatus { status, body }
                if status == StatusCode::TOO_MANY_REQUESTS =>
            {
                Self::RateLimited(body)
            }
            other => Self::ProviderError(other.to_string()),
        }
    }
}

impl From<reqwest::Error> for ProviderError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout(error.to_string())
        } else {
            Self::Network(error.to_string())
        }
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(error: serde_json::Error) -> Self {
        Self::Decode(error.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct ProviderFactory {
    settings: ProvidersSettings,
    client: Client,
}

impl ProviderFactory {
    pub fn new(settings: ProvidersSettings) -> Self {
        Self {
            settings,
            client: Client::new(),
        }
    }

    pub fn create_llm_provider(&self, name: &str) -> Result<Arc<dyn LLMProvider>, ProviderError> {
        match normalize_provider_name(name).as_str() {
            "openai" => {
                let api_key = self
                    .settings
                    .openai_api_key
                    .clone()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or_else(|| {
                        ProviderError::MissingConfiguration(
                            "providers.openai_api_key is required".to_string(),
                        )
                    })?;
                Ok(Arc::new(OpenAIProvider::with_client(
                    api_key,
                    self.client.clone(),
                )))
            }
            "anthropic" | "claude" => {
                let api_key = self
                    .settings
                    .anthropic_api_key
                    .clone()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or_else(|| {
                        ProviderError::MissingConfiguration(
                            "providers.anthropic_api_key is required".to_string(),
                        )
                    })?;
                Ok(Arc::new(AnthropicProvider::with_client(
                    api_key,
                    self.client.clone(),
                )))
            }
            "gemini" | "google" => {
                let api_key = self
                    .settings
                    .gemini_api_key
                    .clone()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or_else(|| {
                        ProviderError::MissingConfiguration(
                            "providers.gemini_api_key is required".to_string(),
                        )
                    })?;
                Ok(Arc::new(GeminiProvider::with_client(
                    api_key,
                    self.client.clone(),
                )))
            }
            provider => Err(ProviderError::UnsupportedProvider(provider.to_string())),
        }
    }

    pub fn create_configured_llm_provider(&self) -> Result<Arc<dyn LLMProvider>, ProviderError> {
        let provider = self
            .settings
            .provider
            .as_deref()
            .filter(|provider| !provider.trim().is_empty())
            .unwrap_or("openai");

        self.create_llm_provider(provider)
    }

    pub fn create_registry(&self) -> ProviderRegistry {
        let mut registry = ProviderRegistry::new();
        if setting_is_present(&self.settings.openai_api_key)
            && let Ok(provider) = self.create_llm_provider("openai")
        {
            registry.register_llm("openai", provider);
        }
        if setting_is_present(&self.settings.anthropic_api_key)
            && let Ok(provider) = self.create_llm_provider("anthropic")
        {
            registry.register_llm("anthropic", provider);
        }
        if setting_is_present(&self.settings.gemini_api_key)
            && let Ok(provider) = self.create_llm_provider("gemini")
        {
            registry.register_llm("gemini", provider);
        }
        registry
    }
}

#[derive(Clone, Default)]
pub struct ProviderRegistry {
    llm_providers: HashMap<String, Arc<dyn LLMProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_settings(settings: &ProvidersSettings) -> Self {
        ProviderFactory::new(settings.clone()).create_registry()
    }

    pub fn register_llm(
        &mut self,
        name: impl Into<String>,
        provider: Arc<dyn LLMProvider>,
    ) -> Option<Arc<dyn LLMProvider>> {
        self.llm_providers
            .insert(normalize_provider_name(&name.into()), provider)
    }

    pub fn get_llm(&self, name: &str) -> Option<Arc<dyn LLMProvider>> {
        self.llm_providers
            .get(&normalize_provider_name(name))
            .cloned()
    }

    pub fn contains_llm(&self, name: &str) -> bool {
        self.llm_providers
            .contains_key(&normalize_provider_name(name))
    }

    pub fn llm_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.llm_providers.keys().cloned().collect();
        names.sort();
        names
    }
}

#[derive(Debug, Clone)]
pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    client: Client,
    max_retries: usize,
    initial_backoff: Duration,
    max_backoff: Duration,
}

impl OpenAIProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_client(api_key, Client::new())
    }

    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            ..Self::with_client(api_key, Client::new())
        }
    }

    fn with_client(api_key: impl Into<String>, client: Client) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: OPENAI_BASE_URL.to_string(),
            client,
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
        }
    }

    pub fn with_retry_config(
        mut self,
        max_retries: usize,
        initial_backoff: Duration,
        max_backoff: Duration,
    ) -> Self {
        self.max_retries = max_retries;
        self.initial_backoff = initial_backoff;
        self.max_backoff = max_backoff;
        self
    }

    fn chat_completions_url(&self) -> String {
        format!("{}{}", self.base_url, CHAT_COMPLETIONS_PATH)
    }

    async fn send_chat_request(
        &self,
        payload: &OpenAIChatRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        for attempt in 0..=self.max_retries {
            let result = self
                .client
                .post(self.chat_completions_url())
                .bearer_auth(&self.api_key)
                .json(payload)
                .send()
                .await;

            match result {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let body = response
                        .text()
                        .await
                        .unwrap_or_else(|error| format!("failed to read error body: {error}"));

                    let provider_error = map_status_error(status, body);
                    if !is_retryable_status(status) || attempt == self.max_retries {
                        return Err(provider_error);
                    }
                }
                Err(error) => {
                    let provider_error = ProviderError::from(error);
                    if !is_retryable_request_error(&provider_error) || attempt == self.max_retries {
                        return Err(provider_error);
                    }
                }
            }

            tokio::time::sleep(self.retry_delay(attempt)).await;
        }

        Err(ProviderError::Network(
            "retry loop exited unexpectedly".to_string(),
        ))
    }

    fn retry_delay(&self, attempt: usize) -> Duration {
        let multiplier = 2_u32.saturating_pow(attempt.min(16) as u32);
        let base = self.initial_backoff.saturating_mul(multiplier);
        let capped = base.min(self.max_backoff);
        let jitter = rand::thread_rng().gen_range(0.5..=1.5);
        Duration::from_secs_f64(capped.as_secs_f64() * jitter)
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let payload = OpenAIChatRequest::from_chat_request(request, false);
        let response = self.send_chat_request(&payload).await?;
        let completion = response
            .json::<OpenAIChatResponse>()
            .await
            .map_err(ProviderError::from)?;

        completion.try_into_chat_response()
    }

    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream, ProviderError> {
        let payload = OpenAIChatRequest::from_chat_request(request, true);
        let response = self.send_chat_request(&payload).await?;
        let byte_stream = response.bytes_stream();

        Ok(Box::pin(async_stream::try_stream! {
            let mut stream = byte_stream;
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(ProviderError::from)?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(event_end) = buffer.find("\n\n") {
                    let event = buffer[..event_end].to_string();
                    buffer = buffer[event_end + 2..].to_string();

                    if let Some(data) = parse_sse_data(&event) {
                        if data == "[DONE]" {
                            return;
                        }

                        let openai_chunk = serde_json::from_str::<OpenAIStreamResponse>(&data)
                            .map_err(ProviderError::from)?;

                        for chunk in openai_chunk.into_chat_chunks() {
                            yield chunk;
                        }
                    }
                }
            }

            if !buffer.trim().is_empty() {
                Err(ProviderError::Stream(
                    "stream ended with an incomplete SSE event".to_string(),
                ))?;
            }
        }))
    }

    fn supports_function_calling(&self) -> bool {
        true
    }
}

#[derive(Debug, Serialize)]
struct OpenAIChatRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAITool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<String>,
}

impl OpenAIChatRequest {
    fn from_chat_request(request: ChatRequest, stream: bool) -> Self {
        Self {
            model: request.model,
            messages: request
                .messages
                .into_iter()
                .map(OpenAIMessage::from)
                .collect(),
            stream,
            temperature: request.temperature,
            top_p: request.top_p,
            max_tokens: request.max_tokens,
            stop: request.stop,
            tools: request.tools.into_iter().map(OpenAITool::from).collect(),
            tool_choice: request.tool_choice.map(tool_choice_to_openai_value),
            user: request.user,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIMessage {
    role: ChatRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OpenAIToolCall>,
}

impl From<ChatMessage> for OpenAIMessage {
    fn from(message: ChatMessage) -> Self {
        Self {
            role: message.role,
            content: message.content,
            name: message.name,
            tool_call_id: message.tool_call_id,
            tool_calls: message
                .tool_calls
                .into_iter()
                .map(OpenAIToolCall::from)
                .collect(),
        }
    }
}

impl From<OpenAIMessage> for ChatMessage {
    fn from(message: OpenAIMessage) -> Self {
        Self {
            role: message.role,
            content: message.content,
            name: message.name,
            tool_call_id: message.tool_call_id,
            tool_calls: message.tool_calls.into_iter().map(ToolCall::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionDefinition,
}

impl From<ChatTool> for OpenAITool {
    fn from(tool: ChatTool) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: tool.function,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionCall,
}

impl From<ToolCall> for OpenAIToolCall {
    fn from(tool_call: ToolCall) -> Self {
        Self {
            id: tool_call.id,
            tool_type: "function".to_string(),
            function: tool_call.function,
        }
    }
}

impl From<OpenAIToolCall> for ToolCall {
    fn from(tool_call: OpenAIToolCall) -> Self {
        Self {
            id: tool_call.id,
            function: tool_call.function,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIChatResponse {
    id: String,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<TokenUsage>,
}

impl OpenAIChatResponse {
    fn try_into_chat_response(self) -> Result<ChatResponse, ProviderError> {
        let choice = self.choices.into_iter().next().ok_or_else(|| {
            ProviderError::Decode("OpenAI response did not include any choices".to_string())
        })?;

        Ok(ChatResponse {
            id: self.id,
            model: self.model,
            message: choice.message.into(),
            finish_reason: choice.finish_reason,
            usage: self.usage,
        })
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamResponse {
    id: String,
    model: String,
    choices: Vec<OpenAIStreamChoice>,
    usage: Option<TokenUsage>,
}

impl OpenAIStreamResponse {
    fn into_chat_chunks(self) -> Vec<ChatChunk> {
        self.choices
            .into_iter()
            .map(|choice| ChatChunk {
                id: self.id.clone(),
                model: self.model.clone(),
                delta: choice.delta.into(),
                finish_reason: choice.finish_reason,
                usage: self.usage.clone(),
            })
            .collect()
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAIStreamDelta {
    role: Option<ChatRole>,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAIToolCallDelta>,
}

impl From<OpenAIStreamDelta> for ChatDelta {
    fn from(delta: OpenAIStreamDelta) -> Self {
        Self {
            role: delta.role,
            content: delta.content,
            tool_calls: delta
                .tool_calls
                .into_iter()
                .map(ToolCallDelta::from)
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCallDelta {
    index: u32,
    id: Option<String>,
    function: Option<FunctionCallDelta>,
}

impl From<OpenAIToolCallDelta> for ToolCallDelta {
    fn from(delta: OpenAIToolCallDelta) -> Self {
        Self {
            index: delta.index,
            id: delta.id,
            function: delta.function,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    client: Client,
    max_retries: usize,
    initial_backoff: Duration,
    max_backoff: Duration,
}

impl AnthropicProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_client(api_key, Client::new())
    }

    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            ..Self::with_client(api_key, Client::new())
        }
    }

    fn with_client(api_key: impl Into<String>, client: Client) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: ANTHROPIC_BASE_URL.to_string(),
            client,
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
        }
    }

    pub fn with_retry_config(
        mut self,
        max_retries: usize,
        initial_backoff: Duration,
        max_backoff: Duration,
    ) -> Self {
        self.max_retries = max_retries;
        self.initial_backoff = initial_backoff;
        self.max_backoff = max_backoff;
        self
    }

    fn messages_url(&self) -> String {
        format!("{}{}", self.base_url, ANTHROPIC_MESSAGES_PATH)
    }

    async fn send_messages_request(
        &self,
        payload: &AnthropicChatRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        for attempt in 0..=self.max_retries {
            let result = self
                .client
                .post(self.messages_url())
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .json(payload)
                .send()
                .await;

            match result {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let body = response
                        .text()
                        .await
                        .unwrap_or_else(|error| format!("failed to read error body: {error}"));

                    let provider_error = map_status_error(status, body);
                    if !is_retryable_status(status) || attempt == self.max_retries {
                        return Err(provider_error);
                    }
                }
                Err(error) => {
                    let provider_error = ProviderError::from(error);
                    if !is_retryable_request_error(&provider_error) || attempt == self.max_retries {
                        return Err(provider_error);
                    }
                }
            }

            tokio::time::sleep(self.retry_delay(attempt)).await;
        }

        Err(ProviderError::Network(
            "retry loop exited unexpectedly".to_string(),
        ))
    }

    fn retry_delay(&self, attempt: usize) -> Duration {
        let multiplier = 2_u32.saturating_pow(attempt.min(16) as u32);
        let base = self.initial_backoff.saturating_mul(multiplier);
        let capped = base.min(self.max_backoff);
        let jitter = rand::thread_rng().gen_range(0.5..=1.5);
        Duration::from_secs_f64(capped.as_secs_f64() * jitter)
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let payload = AnthropicChatRequest::from_chat_request(request, false);
        let response = self.send_messages_request(&payload).await?;
        let completion = response
            .json::<AnthropicChatResponse>()
            .await
            .map_err(ProviderError::from)?;

        completion.try_into_chat_response()
    }

    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream, ProviderError> {
        let payload = AnthropicChatRequest::from_chat_request(request, true);
        let response = self.send_messages_request(&payload).await?;
        let byte_stream = response.bytes_stream();

        Ok(Box::pin(async_stream::try_stream! {
            let mut stream = byte_stream;
            let mut buffer = String::new();
            let mut id = String::new();
            let mut model = payload.model.clone();

            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(ProviderError::from)?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(event_end) = buffer.find("\n\n") {
                    let event = buffer[..event_end].to_string();
                    buffer = buffer[event_end + 2..].to_string();

                    if let Some(data) = parse_sse_data(&event) {
                        let anthropic_event = serde_json::from_str::<AnthropicStreamEvent>(&data)
                            .map_err(ProviderError::from)?;

                        if let Some(chunk) = anthropic_event.into_chat_chunk(&mut id, &mut model) {
                            yield chunk;
                        }
                    }
                }
            }

            if !buffer.trim().is_empty() {
                Err(ProviderError::Stream(
                    "stream ended with an incomplete SSE event".to_string(),
                ))?;
            }
        }))
    }

    fn supports_function_calling(&self) -> bool {
        true
    }
}

#[derive(Debug, Serialize)]
struct AnthropicChatRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<AnthropicToolChoice>,
}

impl AnthropicChatRequest {
    fn from_chat_request(request: ChatRequest, stream: bool) -> Self {
        let mut system_messages = Vec::new();
        let mut messages = Vec::new();
        let tool_choice = request.tool_choice;
        let disable_tools = matches!(tool_choice, Some(ToolChoice::None));

        for message in request.messages {
            if message.role == ChatRole::System {
                if let Some(content) = message.content {
                    system_messages.push(content);
                }
            } else {
                messages.push(AnthropicMessage::from(message));
            }
        }

        Self {
            model: request.model,
            max_tokens: request.max_tokens.unwrap_or(1024),
            messages,
            stream,
            system: (!system_messages.is_empty()).then(|| system_messages.join("\n\n")),
            temperature: request.temperature,
            top_p: request.top_p,
            stop_sequences: request.stop,
            tools: if disable_tools {
                Vec::new()
            } else {
                request.tools.into_iter().map(AnthropicTool::from).collect()
            },
            tool_choice: tool_choice.and_then(tool_choice_to_anthropic),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnthropicMessage {
    role: AnthropicRole,
    content: Vec<AnthropicContentBlock>,
}

impl From<ChatMessage> for AnthropicMessage {
    fn from(message: ChatMessage) -> Self {
        let role = match message.role {
            ChatRole::Assistant => AnthropicRole::Assistant,
            ChatRole::System | ChatRole::User | ChatRole::Tool => AnthropicRole::User,
        };

        let mut content = Vec::new();
        match message.role {
            ChatRole::Tool => content.push(AnthropicContentBlock::ToolResult {
                tool_use_id: message.tool_call_id.unwrap_or_default(),
                content: message.content.unwrap_or_default(),
            }),
            ChatRole::Assistant => {
                if let Some(text) = message.content.filter(|text| !text.is_empty()) {
                    content.push(AnthropicContentBlock::Text { text });
                }
                content.extend(
                    message
                        .tool_calls
                        .into_iter()
                        .map(AnthropicContentBlock::from),
                );
            }
            ChatRole::System | ChatRole::User => {
                content.push(AnthropicContentBlock::Text {
                    text: message.content.unwrap_or_default(),
                });
            }
        }

        Self { role, content }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AnthropicRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

impl From<ToolCall> for AnthropicContentBlock {
    fn from(tool_call: ToolCall) -> Self {
        Self::ToolUse {
            id: tool_call.id,
            name: tool_call.function.name,
            input: parse_json_arguments(&tool_call.function.arguments),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct AnthropicTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: Value,
}

impl From<ChatTool> for AnthropicTool {
    fn from(tool: ChatTool) -> Self {
        Self {
            name: tool.function.name,
            description: tool.function.description,
            input_schema: tool
                .function
                .parameters
                .unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} })),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicToolChoice {
    Auto,
    Any,
    Tool { name: String },
}

#[derive(Debug, Deserialize)]
struct AnthropicChatResponse {
    id: String,
    model: String,
    content: Vec<AnthropicContentBlock>,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsage>,
}

impl AnthropicChatResponse {
    fn try_into_chat_response(self) -> Result<ChatResponse, ProviderError> {
        let mut text = String::new();
        let mut tool_calls = Vec::new();

        for block in self.content {
            match block {
                AnthropicContentBlock::Text { text: block_text } => text.push_str(&block_text),
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        function: FunctionCall {
                            name,
                            arguments: input.to_string(),
                        },
                    });
                }
                AnthropicContentBlock::ToolResult { .. } => {}
            }
        }

        Ok(ChatResponse {
            id: self.id,
            model: self.model,
            message: ChatMessage {
                role: ChatRole::Assistant,
                content: (!text.is_empty()).then_some(text),
                name: None,
                tool_call_id: None,
                tool_calls,
            },
            finish_reason: self.stop_reason,
            usage: self.usage.map(TokenUsage::from),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

impl From<AnthropicUsage> for TokenUsage {
    fn from(usage: AnthropicUsage) -> Self {
        Self {
            prompt_tokens: usage.input_tokens,
            completion_tokens: usage.output_tokens,
            total_tokens: usage.input_tokens + usage.output_tokens,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamEvent {
    MessageStart {
        message: AnthropicStreamMessage,
    },
    ContentBlockStart {
        index: u32,
        content_block: AnthropicContentBlock,
    },
    ContentBlockDelta {
        index: u32,
        delta: AnthropicStreamDelta,
    },
    MessageDelta {
        delta: AnthropicMessageDelta,
        usage: Option<AnthropicUsage>,
    },
    ContentBlockStop {
        #[serde(rename = "index")]
        _index: u32,
    },
    MessageStop,
    Ping,
}

impl AnthropicStreamEvent {
    fn into_chat_chunk(self, id: &mut String, model: &mut String) -> Option<ChatChunk> {
        match self {
            Self::MessageStart { message } => {
                *id = message.id;
                *model = message.model;
                Some(ChatChunk {
                    id: id.clone(),
                    model: model.clone(),
                    delta: ChatDelta {
                        role: Some(ChatRole::Assistant),
                        content: None,
                        tool_calls: Vec::new(),
                    },
                    finish_reason: None,
                    usage: message.usage.map(TokenUsage::from),
                })
            }
            Self::ContentBlockStart {
                index,
                content_block,
            } => match content_block {
                AnthropicContentBlock::Text { text } => (!text.is_empty()).then(|| ChatChunk {
                    id: id.clone(),
                    model: model.clone(),
                    delta: ChatDelta {
                        role: None,
                        content: Some(text),
                        tool_calls: Vec::new(),
                    },
                    finish_reason: None,
                    usage: None,
                }),
                AnthropicContentBlock::ToolUse {
                    id: tool_id,
                    name,
                    input,
                } => Some(ChatChunk {
                    id: id.clone(),
                    model: model.clone(),
                    delta: ChatDelta {
                        role: None,
                        content: None,
                        tool_calls: vec![ToolCallDelta {
                            index,
                            id: Some(tool_id),
                            function: Some(FunctionCallDelta {
                                name: Some(name),
                                arguments: (input != Value::Null).then(|| input.to_string()),
                            }),
                        }],
                    },
                    finish_reason: None,
                    usage: None,
                }),
                AnthropicContentBlock::ToolResult { .. } => None,
            },
            Self::ContentBlockDelta { index, delta } => match delta {
                AnthropicStreamDelta::TextDelta { text } => Some(ChatChunk {
                    id: id.clone(),
                    model: model.clone(),
                    delta: ChatDelta {
                        role: None,
                        content: Some(text),
                        tool_calls: Vec::new(),
                    },
                    finish_reason: None,
                    usage: None,
                }),
                AnthropicStreamDelta::InputJsonDelta { partial_json } => Some(ChatChunk {
                    id: id.clone(),
                    model: model.clone(),
                    delta: ChatDelta {
                        role: None,
                        content: None,
                        tool_calls: vec![ToolCallDelta {
                            index,
                            id: None,
                            function: Some(FunctionCallDelta {
                                name: None,
                                arguments: Some(partial_json),
                            }),
                        }],
                    },
                    finish_reason: None,
                    usage: None,
                }),
            },
            Self::MessageDelta { delta, usage } => Some(ChatChunk {
                id: id.clone(),
                model: model.clone(),
                delta: ChatDelta::default(),
                finish_reason: delta.stop_reason,
                usage: usage.map(TokenUsage::from),
            }),
            Self::ContentBlockStop { .. } | Self::MessageStop | Self::Ping => None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamMessage {
    id: String,
    model: String,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageDelta {
    stop_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GeminiProvider {
    api_key: String,
    base_url: String,
    client: Client,
    max_retries: usize,
    initial_backoff: Duration,
    max_backoff: Duration,
}

impl GeminiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_client(api_key, Client::new())
    }

    pub fn with_base_url(api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            ..Self::with_client(api_key, Client::new())
        }
    }

    fn with_client(api_key: impl Into<String>, client: Client) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: GEMINI_BASE_URL.to_string(),
            client,
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
        }
    }

    pub fn with_retry_config(
        mut self,
        max_retries: usize,
        initial_backoff: Duration,
        max_backoff: Duration,
    ) -> Self {
        self.max_retries = max_retries;
        self.initial_backoff = initial_backoff;
        self.max_backoff = max_backoff;
        self
    }

    fn generate_content_url(&self, model: &str, stream: bool) -> String {
        let method = if stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let model = model.trim_start_matches("models/");
        let alt = if stream { "&alt=sse" } else { "" };
        format!(
            "{}/v1beta/models/{}:{}?key={}{}",
            self.base_url, model, method, self.api_key, alt
        )
    }

    async fn send_generate_request(
        &self,
        model: &str,
        stream: bool,
        payload: &GeminiChatRequest,
    ) -> Result<reqwest::Response, ProviderError> {
        for attempt in 0..=self.max_retries {
            let result = self
                .client
                .post(self.generate_content_url(model, stream))
                .json(payload)
                .send()
                .await;

            match result {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let body = response
                        .text()
                        .await
                        .unwrap_or_else(|error| format!("failed to read error body: {error}"));

                    let provider_error = map_status_error(status, body);
                    if !is_retryable_status(status) || attempt == self.max_retries {
                        return Err(provider_error);
                    }
                }
                Err(error) => {
                    let provider_error = ProviderError::from(error);
                    if !is_retryable_request_error(&provider_error) || attempt == self.max_retries {
                        return Err(provider_error);
                    }
                }
            }

            tokio::time::sleep(self.retry_delay(attempt)).await;
        }

        Err(ProviderError::Network(
            "retry loop exited unexpectedly".to_string(),
        ))
    }

    fn retry_delay(&self, attempt: usize) -> Duration {
        let multiplier = 2_u32.saturating_pow(attempt.min(16) as u32);
        let base = self.initial_backoff.saturating_mul(multiplier);
        let capped = base.min(self.max_backoff);
        let jitter = rand::thread_rng().gen_range(0.5..=1.5);
        Duration::from_secs_f64(capped.as_secs_f64() * jitter)
    }
}

#[async_trait]
impl LLMProvider for GeminiProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let model = request.model.clone();
        let payload = GeminiChatRequest::from_chat_request(request);
        let response = self.send_generate_request(&model, false, &payload).await?;
        let completion = response
            .json::<GeminiChatResponse>()
            .await
            .map_err(ProviderError::from)?;

        completion.try_into_chat_response(&model)
    }

    async fn chat_stream(&self, request: ChatRequest) -> Result<ChatStream, ProviderError> {
        let model = request.model.clone();
        let payload = GeminiChatRequest::from_chat_request(request);
        let response = self.send_generate_request(&model, true, &payload).await?;
        let byte_stream = response.bytes_stream();

        Ok(Box::pin(async_stream::try_stream! {
            let mut stream = byte_stream;
            let mut buffer = String::new();

            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(ProviderError::from)?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(event_end) = buffer.find("\n\n") {
                    let event = buffer[..event_end].to_string();
                    buffer = buffer[event_end + 2..].to_string();

                    if let Some(data) = parse_sse_data(&event) {
                        if data == "[DONE]" {
                            return;
                        }

                        let gemini_chunk = serde_json::from_str::<GeminiChatResponse>(&data)
                            .map_err(ProviderError::from)?;

                        for chunk in gemini_chunk.into_chat_chunks(&model) {
                            yield chunk;
                        }
                    }
                }
            }

            if !buffer.trim().is_empty() {
                Err(ProviderError::Stream(
                    "stream ended with an incomplete SSE event".to_string(),
                ))?;
            }
        }))
    }

    fn supports_function_calling(&self) -> bool {
        true
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiChatRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<GeminiToolConfig>,
}

impl GeminiChatRequest {
    fn from_chat_request(request: ChatRequest) -> Self {
        let mut system_parts = Vec::new();
        let mut contents = Vec::new();

        for message in request.messages {
            if message.role == ChatRole::System {
                if let Some(content) = message.content {
                    system_parts.push(GeminiPart::Text { text: content });
                }
            } else {
                contents.push(GeminiContent::from(message));
            }
        }

        let generation_config = (request.temperature.is_some()
            || request.top_p.is_some()
            || request.max_tokens.is_some()
            || !request.stop.is_empty())
        .then_some(GeminiGenerationConfig {
            temperature: request.temperature,
            top_p: request.top_p,
            max_output_tokens: request.max_tokens,
            stop_sequences: request.stop,
        });

        let tool_config = request.tool_choice.map(tool_choice_to_gemini_config);

        Self {
            contents,
            system_instruction: (!system_parts.is_empty()).then_some(GeminiContent {
                role: None,
                parts: system_parts,
            }),
            generation_config,
            tools: request.tools.into_iter().map(GeminiTool::from).collect(),
            tool_config,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeminiContent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

impl From<ChatMessage> for GeminiContent {
    fn from(message: ChatMessage) -> Self {
        let role = match message.role {
            ChatRole::Assistant => "model",
            ChatRole::System | ChatRole::User | ChatRole::Tool => "user",
        };

        let mut parts = Vec::new();
        match message.role {
            ChatRole::Tool => parts.push(GeminiPart::FunctionResponse {
                function_response: GeminiFunctionResponse {
                    name: message
                        .name
                        .or_else(|| message.tool_call_id.clone())
                        .unwrap_or_else(|| "tool".to_string()),
                    response: parse_json_arguments(&message.content.unwrap_or_default()),
                },
            }),
            ChatRole::Assistant => {
                if let Some(text) = message.content.filter(|text| !text.is_empty()) {
                    parts.push(GeminiPart::Text { text });
                }
                parts.extend(message.tool_calls.into_iter().map(GeminiPart::from));
            }
            ChatRole::System | ChatRole::User => {
                parts.push(GeminiPart::Text {
                    text: message.content.unwrap_or_default(),
                });
            }
        }

        Self {
            role: Some(role.to_string()),
            parts,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

impl From<ToolCall> for GeminiPart {
    fn from(tool_call: ToolCall) -> Self {
        Self::FunctionCall {
            function_call: GeminiFunctionCall {
                name: tool_call.function.name,
                args: parse_json_arguments(&tool_call.function.arguments),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiTool {
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

impl From<ChatTool> for GeminiTool {
    fn from(tool: ChatTool) -> Self {
        Self {
            function_declarations: vec![GeminiFunctionDeclaration {
                name: tool.function.name,
                description: tool.function.description,
                parameters: tool.function.parameters,
            }],
        }
    }
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiToolConfig {
    function_calling_config: GeminiFunctionCallingConfig,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiFunctionCallingConfig {
    mode: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    allowed_function_names: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiChatResponse {
    #[serde(default)]
    response_id: Option<String>,
    #[serde(default)]
    model_version: Option<String>,
    candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    usage_metadata: Option<GeminiUsageMetadata>,
}

impl GeminiChatResponse {
    fn try_into_chat_response(self, request_model: &str) -> Result<ChatResponse, ProviderError> {
        let choice = self.candidates.into_iter().next().ok_or_else(|| {
            ProviderError::Decode("Gemini response did not include any candidates".to_string())
        })?;

        Ok(ChatResponse {
            id: self
                .response_id
                .unwrap_or_else(|| format!("gemini-{request_model}")),
            model: self
                .model_version
                .unwrap_or_else(|| request_model.to_string()),
            message: choice.content.into_chat_message(),
            finish_reason: choice.finish_reason,
            usage: self.usage_metadata.map(TokenUsage::from),
        })
    }

    fn into_chat_chunks(self, request_model: &str) -> Vec<ChatChunk> {
        let id = self
            .response_id
            .unwrap_or_else(|| format!("gemini-{request_model}"));
        let model = self
            .model_version
            .unwrap_or_else(|| request_model.to_string());
        let usage = self.usage_metadata.map(TokenUsage::from);

        self.candidates
            .into_iter()
            .map(|candidate| ChatChunk {
                id: id.clone(),
                model: model.clone(),
                delta: candidate.content.into_chat_delta(),
                finish_reason: candidate.finish_reason,
                usage: usage.clone(),
            })
            .collect()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiCandidate {
    content: GeminiContent,
    finish_reason: Option<String>,
}

impl GeminiContent {
    fn into_chat_message(self) -> ChatMessage {
        let delta = self.into_chat_delta();
        ChatMessage {
            role: ChatRole::Assistant,
            content: delta.content,
            name: None,
            tool_call_id: None,
            tool_calls: delta
                .tool_calls
                .into_iter()
                .map(|delta| ToolCall {
                    id: delta
                        .id
                        .unwrap_or_else(|| format!("gemini-tool-call-{}", delta.index)),
                    function: FunctionCall {
                        name: delta
                            .function
                            .as_ref()
                            .and_then(|function| function.name.clone())
                            .unwrap_or_default(),
                        arguments: delta
                            .function
                            .and_then(|function| function.arguments)
                            .unwrap_or_else(|| "{}".to_string()),
                    },
                })
                .collect(),
        }
    }

    fn into_chat_delta(self) -> ChatDelta {
        let mut text = String::new();
        let mut tool_calls = Vec::new();

        for (index, part) in self.parts.into_iter().enumerate() {
            match part {
                GeminiPart::Text { text: part_text } => text.push_str(&part_text),
                GeminiPart::FunctionCall { function_call } => {
                    tool_calls.push(ToolCallDelta {
                        index: index as u32,
                        id: Some(format!("gemini-tool-call-{index}")),
                        function: Some(FunctionCallDelta {
                            name: Some(function_call.name),
                            arguments: Some(function_call.args.to_string()),
                        }),
                    });
                }
                GeminiPart::FunctionResponse { .. } => {}
            }
        }

        ChatDelta {
            role: Some(ChatRole::Assistant),
            content: (!text.is_empty()).then_some(text),
            tool_calls,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsageMetadata {
    #[serde(default)]
    prompt_token_count: u32,
    #[serde(default)]
    candidates_token_count: u32,
    #[serde(default)]
    total_token_count: u32,
}

impl From<GeminiUsageMetadata> for TokenUsage {
    fn from(usage: GeminiUsageMetadata) -> Self {
        let total_tokens = if usage.total_token_count == 0 {
            usage.prompt_token_count + usage.candidates_token_count
        } else {
            usage.total_token_count
        };

        Self {
            prompt_tokens: usage.prompt_token_count,
            completion_tokens: usage.candidates_token_count,
            total_tokens,
        }
    }
}

fn normalize_provider_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn setting_is_present(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
}

fn tool_choice_to_openai_value(choice: ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => Value::String("auto".to_string()),
        ToolChoice::None => Value::String("none".to_string()),
        ToolChoice::Required => Value::String("required".to_string()),
        ToolChoice::Function { name } => serde_json::json!({
            "type": "function",
            "function": {
                "name": name,
            },
        }),
    }
}

fn tool_choice_to_anthropic(choice: ToolChoice) -> Option<AnthropicToolChoice> {
    match choice {
        ToolChoice::Auto => Some(AnthropicToolChoice::Auto),
        ToolChoice::None => None,
        ToolChoice::Required => Some(AnthropicToolChoice::Any),
        ToolChoice::Function { name } => Some(AnthropicToolChoice::Tool { name }),
    }
}

fn tool_choice_to_gemini_config(choice: ToolChoice) -> GeminiToolConfig {
    let function_calling_config = match choice {
        ToolChoice::Auto => GeminiFunctionCallingConfig {
            mode: "AUTO".to_string(),
            allowed_function_names: Vec::new(),
        },
        ToolChoice::None => GeminiFunctionCallingConfig {
            mode: "NONE".to_string(),
            allowed_function_names: Vec::new(),
        },
        ToolChoice::Required => GeminiFunctionCallingConfig {
            mode: "ANY".to_string(),
            allowed_function_names: Vec::new(),
        },
        ToolChoice::Function { name } => GeminiFunctionCallingConfig {
            mode: "ANY".to_string(),
            allowed_function_names: vec![name],
        },
    };

    GeminiToolConfig {
        function_calling_config,
    }
}

fn parse_json_arguments(arguments: &str) -> Value {
    serde_json::from_str(arguments).unwrap_or_else(|_| Value::String(arguments.to_string()))
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn is_retryable_request_error(error: &ProviderError) -> bool {
    matches!(error, ProviderError::Timeout(_) | ProviderError::Network(_))
}

fn map_status_error(status: StatusCode, body: String) -> ProviderError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::Authentication(body),
        StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimited(body),
        _ => ProviderError::HttpStatus { status, body },
    }
}

fn parse_sse_data(event: &str) -> Option<String> {
    let data_lines: Vec<&str> = event
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect();

    if data_lines.is_empty() {
        None
    } else {
        Some(data_lines.join("\n"))
    }
}

// Tests
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode as AxumStatusCode, Uri};
    use axum::response::{IntoResponse, Response};
    use axum::{Json, Router};
    use serde_json::json;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    #[derive(Clone)]
    struct MockServerState {
        calls: Arc<AtomicUsize>,
        mode: MockMode,
    }

    #[derive(Clone)]
    enum MockMode {
        OpenAIChat,
        OpenAIStream,
        OpenAIRetry,
        AnthropicChat,
        AnthropicStream,
        GeminiChat,
        GeminiStream,
        Conformance,
    }

    #[tokio::test]
    async fn openai_chat_completion_maps_response() -> Result<(), Box<dyn std::error::Error>> {
        let server = TestServer::start(MockMode::OpenAIChat).await?;
        let provider = OpenAIProvider::with_base_url("test-key", server.base_url())
            .with_retry_config(0, Duration::from_millis(1), Duration::from_millis(1));

        let response = provider
            .chat(ChatRequest::new(
                "gpt-test",
                vec![ChatMessage::user("Hello")],
            ))
            .await?;

        assert_eq!(response.id, "chatcmpl-test");
        assert_eq!(response.model, "gpt-test");
        assert_eq!(response.message.role, ChatRole::Assistant);
        assert_eq!(
            response.message.content.as_deref(),
            Some("Hello from OpenAI")
        );
        assert_eq!(response.finish_reason.as_deref(), Some("stop"));
        assert_eq!(response.usage.map(|usage| usage.total_tokens), Some(8));
        assert_eq!(server.calls(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn openai_chat_stream_parses_sse_chunks() -> Result<(), Box<dyn std::error::Error>> {
        let server = TestServer::start(MockMode::OpenAIStream).await?;
        let provider = OpenAIProvider::with_base_url("test-key", server.base_url())
            .with_retry_config(0, Duration::from_millis(1), Duration::from_millis(1));

        let mut stream = provider
            .chat_stream(ChatRequest::new(
                "gpt-test",
                vec![ChatMessage::user("Hello")],
            ))
            .await?;

        let first = stream.next().await.ok_or("missing first stream chunk")??;
        let second = stream.next().await.ok_or("missing second stream chunk")??;

        assert_eq!(first.delta.role, Some(ChatRole::Assistant));
        assert_eq!(first.delta.content.as_deref(), Some("Hel"));
        assert_eq!(second.delta.content.as_deref(), Some("lo"));
        assert_eq!(second.finish_reason.as_deref(), Some("stop"));
        assert!(stream.next().await.is_none());
        assert_eq!(server.calls(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn openai_retries_429_and_5xx() -> Result<(), Box<dyn std::error::Error>> {
        let server = TestServer::start(MockMode::OpenAIRetry).await?;
        let provider = OpenAIProvider::with_base_url("test-key", server.base_url())
            .with_retry_config(3, Duration::from_millis(1), Duration::from_millis(1));

        let response = provider
            .chat(ChatRequest::new(
                "gpt-test",
                vec![ChatMessage::user("Hello")],
            ))
            .await?;

        assert_eq!(response.message.content.as_deref(), Some("Recovered"));
        assert_eq!(server.calls(), 3);

        Ok(())
    }

    #[tokio::test]
    async fn anthropic_chat_completion_maps_response_and_tools()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = TestServer::start(MockMode::AnthropicChat).await?;
        let provider = AnthropicProvider::with_base_url("test-key", server.base_url())
            .with_retry_config(0, Duration::from_millis(1), Duration::from_millis(1));

        let response = provider.chat(tool_request("claude-test")).await?;

        assert_eq!(response.id, "msg-test");
        assert_eq!(response.model, "claude-test");
        assert_eq!(response.message.role, ChatRole::Assistant);
        assert_eq!(
            response.message.content.as_deref(),
            Some("Hello from Anthropic")
        );
        assert_eq!(response.message.tool_calls.len(), 1);
        assert_eq!(response.message.tool_calls[0].id, "toolu_1");
        assert_eq!(response.message.tool_calls[0].function.name, "lookup");
        assert_eq!(
            response.message.tool_calls[0].function.arguments,
            r#"{"query":"rust"}"#
        );
        assert_eq!(response.finish_reason.as_deref(), Some("tool_use"));
        assert_eq!(response.usage.map(|usage| usage.total_tokens), Some(8));
        assert_eq!(server.calls(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn anthropic_chat_stream_parses_sse_chunks_and_tool_deltas()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = TestServer::start(MockMode::AnthropicStream).await?;
        let provider = AnthropicProvider::with_base_url("test-key", server.base_url())
            .with_retry_config(0, Duration::from_millis(1), Duration::from_millis(1));

        let mut stream = provider.chat_stream(tool_request("claude-test")).await?;

        let first = stream.next().await.ok_or("missing first stream chunk")??;
        let second = stream.next().await.ok_or("missing second stream chunk")??;
        let third = stream.next().await.ok_or("missing third stream chunk")??;
        let fourth = stream.next().await.ok_or("missing fourth stream chunk")??;
        let fifth = stream.next().await.ok_or("missing fifth stream chunk")??;

        assert_eq!(first.delta.role, Some(ChatRole::Assistant));
        assert_eq!(second.delta.content.as_deref(), Some("Hel"));
        assert_eq!(third.delta.content.as_deref(), Some("lo"));
        assert_eq!(
            fourth.delta.tool_calls[0]
                .function
                .as_ref()
                .unwrap()
                .name
                .as_deref(),
            Some("lookup")
        );
        assert_eq!(
            fifth.delta.tool_calls[0]
                .function
                .as_ref()
                .unwrap()
                .arguments
                .as_deref(),
            Some(r#"{"query":"rust"}"#)
        );

        let final_chunk = stream.next().await.ok_or("missing final stream chunk")??;
        assert_eq!(final_chunk.finish_reason.as_deref(), Some("tool_use"));
        assert!(stream.next().await.is_none());
        assert_eq!(server.calls(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn gemini_chat_completion_maps_response_and_tools()
    -> Result<(), Box<dyn std::error::Error>> {
        let server = TestServer::start(MockMode::GeminiChat).await?;
        let provider = GeminiProvider::with_base_url("test-key", server.base_url())
            .with_retry_config(0, Duration::from_millis(1), Duration::from_millis(1));

        let response = provider.chat(tool_request("gemini-test")).await?;

        assert_eq!(response.id, "gemini-response-test");
        assert_eq!(response.model, "gemini-test");
        assert_eq!(response.message.role, ChatRole::Assistant);
        assert_eq!(
            response.message.content.as_deref(),
            Some("Hello from Gemini")
        );
        assert_eq!(response.message.tool_calls.len(), 1);
        assert_eq!(response.message.tool_calls[0].id, "gemini-tool-call-1");
        assert_eq!(response.message.tool_calls[0].function.name, "lookup");
        assert_eq!(
            response.message.tool_calls[0].function.arguments,
            r#"{"query":"rust"}"#
        );
        assert_eq!(response.finish_reason.as_deref(), Some("STOP"));
        assert_eq!(response.usage.map(|usage| usage.total_tokens), Some(8));
        assert_eq!(server.calls(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn gemini_chat_stream_parses_sse_chunks() -> Result<(), Box<dyn std::error::Error>> {
        let server = TestServer::start(MockMode::GeminiStream).await?;
        let provider = GeminiProvider::with_base_url("test-key", server.base_url())
            .with_retry_config(0, Duration::from_millis(1), Duration::from_millis(1));

        let mut stream = provider.chat_stream(tool_request("gemini-test")).await?;

        let first = stream.next().await.ok_or("missing first stream chunk")??;
        let second = stream.next().await.ok_or("missing second stream chunk")??;

        assert_eq!(first.delta.role, Some(ChatRole::Assistant));
        assert_eq!(first.delta.content.as_deref(), Some("Hel"));
        assert_eq!(second.delta.content.as_deref(), Some("lo"));
        assert_eq!(second.finish_reason.as_deref(), Some("STOP"));
        assert!(stream.next().await.is_none());
        assert_eq!(server.calls(), 1);

        Ok(())
    }

    #[test]
    fn factory_and_registry_create_configured_providers_from_settings()
    -> Result<(), Box<dyn std::error::Error>> {
        let settings = ProvidersSettings {
            provider: Some("anthropic".to_string()),
            openai_api_key: Some("test-key".to_string()),
            anthropic_api_key: Some("test-key".to_string()),
            gemini_api_key: Some("test-key".to_string()),
        };

        let factory = ProviderFactory::new(settings.clone());
        let provider = factory.create_llm_provider("OpenAI")?;
        assert!(provider.supports_function_calling());
        let provider = factory.create_configured_llm_provider()?;
        assert!(provider.supports_function_calling());

        let registry = ProviderRegistry::from_settings(&settings);
        assert!(registry.contains_llm("openai"));
        assert!(registry.contains_llm("anthropic"));
        assert!(registry.contains_llm("gemini"));
        assert_eq!(
            registry.llm_names(),
            vec![
                "anthropic".to_string(),
                "gemini".to_string(),
                "openai".to_string()
            ]
        );

        Ok(())
    }

    #[test]
    fn provider_error_maps_into_context_error() {
        let context_error =
            ContextraError::from(ProviderError::RateLimited("slow down".to_string()));
        assert!(matches!(context_error, ContextraError::RateLimited(_)));

        let context_error =
            ContextraError::from(ProviderError::Authentication("bad key".to_string()));
        assert!(matches!(context_error, ContextraError::Unauthorized(_)));
    }

    mod provider_conformance_tests {
        use super::*;

        #[tokio::test]
        async fn every_registered_provider_supports_shared_chat_contract()
        -> Result<(), Box<dyn std::error::Error>> {
            let server = TestServer::start(MockMode::Conformance).await?;
            let mut registry = ProviderRegistry::new();
            registry.register_llm(
                "openai",
                Arc::new(
                    OpenAIProvider::with_base_url("test-key", server.base_url()).with_retry_config(
                        0,
                        Duration::from_millis(1),
                        Duration::from_millis(1),
                    ),
                ),
            );
            registry.register_llm(
                "anthropic",
                Arc::new(
                    AnthropicProvider::with_base_url("test-key", server.base_url())
                        .with_retry_config(0, Duration::from_millis(1), Duration::from_millis(1)),
                ),
            );
            registry.register_llm(
                "gemini",
                Arc::new(
                    GeminiProvider::with_base_url("test-key", server.base_url()).with_retry_config(
                        0,
                        Duration::from_millis(1),
                        Duration::from_millis(1),
                    ),
                ),
            );

            assert_eq!(
                registry.llm_names(),
                vec![
                    "anthropic".to_string(),
                    "gemini".to_string(),
                    "openai".to_string()
                ]
            );

            for name in registry.llm_names() {
                let provider = registry.get_llm(&name).ok_or("missing provider")?;
                assert!(provider.supports_function_calling(), "{name}");

                let response = provider.chat(basic_request("gpt-test")).await?;
                assert_eq!(response.message.role, ChatRole::Assistant, "{name}");
                assert_eq!(
                    response.message.content.as_deref(),
                    Some("Conformance ok"),
                    "{name}"
                );
                assert_eq!(
                    response.usage.map(|usage| usage.total_tokens),
                    Some(8),
                    "{name}"
                );

                let mut stream = provider.chat_stream(basic_request("gpt-test")).await?;
                let mut content = String::new();
                let mut finish_reason = None;
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;
                    if let Some(delta_content) = chunk.delta.content {
                        content.push_str(&delta_content);
                    }
                    finish_reason = chunk.finish_reason.or(finish_reason);
                }

                assert_eq!(content, "Hello", "{name}");
                assert!(finish_reason.is_some(), "{name}");
            }

            assert_eq!(server.calls(), 6);

            Ok(())
        }
    }

    struct TestServer {
        address: SocketAddr,
        calls: Arc<AtomicUsize>,
        shutdown: Option<oneshot::Sender<()>>,
    }

    impl TestServer {
        async fn start(mode: MockMode) -> Result<Self, Box<dyn std::error::Error>> {
            let calls = Arc::new(AtomicUsize::new(0));
            let state = MockServerState {
                calls: Arc::clone(&calls),
                mode,
            };

            let app = Router::new()
                .fallback(mock_provider_request)
                .with_state(state);
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let (shutdown_sender, shutdown_receiver) = oneshot::channel();

            tokio::spawn(async move {
                let _ = axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = shutdown_receiver.await;
                    })
                    .await;
            });

            Ok(Self {
                address,
                calls,
                shutdown: Some(shutdown_sender),
            })
        }

        fn base_url(&self) -> String {
            format!("http://{}", self.address)
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
        }
    }

    async fn mock_provider_request(
        State(state): State<MockServerState>,
        headers: HeaderMap,
        uri: Uri,
        Json(body): Json<Value>,
    ) -> Response {
        let call_number = state.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let path = uri.path();

        match state.mode {
            MockMode::OpenAIChat | MockMode::OpenAIStream | MockMode::OpenAIRetry => {
                mock_openai_request(state.mode, call_number, headers, body)
            }
            MockMode::AnthropicChat | MockMode::AnthropicStream => {
                mock_anthropic_request(state.mode, headers, body)
            }
            MockMode::GeminiChat | MockMode::GeminiStream => {
                mock_gemini_request(state.mode, uri, body)
            }
            MockMode::Conformance if path == CHAT_COMPLETIONS_PATH => {
                mock_openai_conformance_request(body)
            }
            MockMode::Conformance if path == ANTHROPIC_MESSAGES_PATH => {
                mock_anthropic_conformance_request(body)
            }
            MockMode::Conformance
                if path.contains(":generateContent") || path.contains(":streamGenerateContent") =>
            {
                mock_gemini_conformance_request(path, body)
            }
            MockMode::Conformance => (
                AxumStatusCode::NOT_FOUND,
                format!("unexpected path: {path}"),
            )
                .into_response(),
        }
    }

    fn mock_openai_request(
        mode: MockMode,
        call_number: usize,
        headers: HeaderMap,
        body: Value,
    ) -> Response {
        assert_eq!(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-key")
        );
        assert_eq!(body.get("model").and_then(Value::as_str), Some("gpt-test"));
        assert_eq!(
            body.get("stream").and_then(Value::as_bool),
            Some(matches!(mode, MockMode::OpenAIStream))
        );

        match mode {
            MockMode::OpenAIChat => Json(openai_chat_response("Hello from OpenAI")).into_response(),
            MockMode::OpenAIStream => Response::builder()
                .status(AxumStatusCode::OK)
                .header("content-type", "text/event-stream")
                .body(Body::from(openai_stream_response()))
                .unwrap_or_else(|error| {
                    (
                        AxumStatusCode::INTERNAL_SERVER_ERROR,
                        format!("failed to build response: {error}"),
                    )
                        .into_response()
                }),
            MockMode::OpenAIRetry if call_number == 1 => {
                (AxumStatusCode::TOO_MANY_REQUESTS, "rate limited").into_response()
            }
            MockMode::OpenAIRetry if call_number == 2 => {
                (AxumStatusCode::INTERNAL_SERVER_ERROR, "temporary failure").into_response()
            }
            MockMode::OpenAIRetry => Json(openai_chat_response("Recovered")).into_response(),
            _ => unreachable!("mock_openai_request called with non-OpenAI mode"),
        }
    }

    fn mock_anthropic_request(mode: MockMode, headers: HeaderMap, body: Value) -> Response {
        assert_eq!(
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("test-key")
        );
        assert_eq!(
            headers
                .get("anthropic-version")
                .and_then(|value| value.to_str().ok()),
            Some(ANTHROPIC_VERSION)
        );
        assert_eq!(
            body.get("model").and_then(Value::as_str),
            Some("claude-test")
        );
        assert_eq!(
            body.get("system").and_then(Value::as_str),
            Some("You are helpful")
        );
        assert_eq!(
            body.pointer("/messages/0/content/0/text")
                .and_then(Value::as_str),
            Some("Hello")
        );
        assert_eq!(
            body.pointer("/tools/0/name").and_then(Value::as_str),
            Some("lookup")
        );
        assert_eq!(
            body.pointer("/tool_choice/type").and_then(Value::as_str),
            Some("tool")
        );
        assert_eq!(
            body.pointer("/tool_choice/name").and_then(Value::as_str),
            Some("lookup")
        );

        match mode {
            MockMode::AnthropicChat => {
                Json(anthropic_chat_response("Hello from Anthropic", true)).into_response()
            }
            MockMode::AnthropicStream => sse_response(anthropic_stream_response()),
            _ => unreachable!("mock_anthropic_request called with non-Anthropic mode"),
        }
    }

    fn mock_gemini_request(mode: MockMode, uri: Uri, body: Value) -> Response {
        assert!(
            uri.query()
                .is_some_and(|query| query.starts_with("key=test-key"))
        );
        assert!(uri.path().contains("/v1beta/models/gemini-test:"));
        assert_eq!(
            body.pointer("/systemInstruction/parts/0/text")
                .and_then(Value::as_str),
            Some("You are helpful")
        );
        assert_eq!(
            body.pointer("/contents/0/parts/0/text")
                .and_then(Value::as_str),
            Some("Hello")
        );
        assert_eq!(
            body.pointer("/tools/0/functionDeclarations/0/name")
                .and_then(Value::as_str),
            Some("lookup")
        );
        assert_eq!(
            body.pointer("/toolConfig/functionCallingConfig/mode")
                .and_then(Value::as_str),
            Some("ANY")
        );
        assert_eq!(
            body.pointer("/toolConfig/functionCallingConfig/allowedFunctionNames/0")
                .and_then(Value::as_str),
            Some("lookup")
        );

        match mode {
            MockMode::GeminiChat => {
                Json(gemini_chat_response("Hello from Gemini", true)).into_response()
            }
            MockMode::GeminiStream => sse_response(gemini_stream_response()),
            _ => unreachable!("mock_gemini_request called with non-Gemini mode"),
        }
    }

    fn mock_openai_conformance_request(body: Value) -> Response {
        let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
        if stream {
            sse_response(openai_stream_response())
        } else {
            Json(openai_chat_response("Conformance ok")).into_response()
        }
    }

    fn mock_anthropic_conformance_request(body: Value) -> Response {
        let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
        if stream {
            sse_response(anthropic_text_stream_response())
        } else {
            Json(anthropic_chat_response("Conformance ok", false)).into_response()
        }
    }

    fn mock_gemini_conformance_request(path: &str, _body: Value) -> Response {
        if path.contains(":streamGenerateContent") {
            sse_response(gemini_stream_response())
        } else {
            Json(gemini_chat_response("Conformance ok", false)).into_response()
        }
    }

    fn sse_response(body: String) -> Response {
        Response::builder()
            .status(AxumStatusCode::OK)
            .header("content-type", "text/event-stream")
            .body(Body::from(body))
            .unwrap_or_else(|error| {
                (
                    AxumStatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to build response: {error}"),
                )
                    .into_response()
            })
    }

    fn basic_request(model: &str) -> ChatRequest {
        ChatRequest::new(model, vec![ChatMessage::user("Hello")])
    }

    fn tool_request(model: &str) -> ChatRequest {
        let mut request = ChatRequest::new(
            model,
            vec![
                ChatMessage::system("You are helpful"),
                ChatMessage::user("Hello"),
            ],
        );
        request.max_tokens = Some(64);
        request.tools = vec![ChatTool {
            function: FunctionDefinition {
                name: "lookup".to_string(),
                description: Some("Look up a topic".to_string()),
                parameters: Some(json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" }
                    },
                    "required": ["query"]
                })),
            },
        }];
        request.tool_choice = Some(ToolChoice::Function {
            name: "lookup".to_string(),
        });
        request
    }

    fn openai_chat_response(content: &str) -> Value {
        json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-test",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": content
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 3,
                "completion_tokens": 5,
                "total_tokens": 8
            }
        })
    }

    fn openai_stream_response() -> String {
        [
            r#"data: {"id":"chatcmpl-test","model":"gpt-test","choices":[{"index":0,"delta":{"role":"assistant","content":"Hel"},"finish_reason":null}]}"#,
            "",
            r#"data: {"id":"chatcmpl-test","model":"gpt-test","choices":[{"index":0,"delta":{"content":"lo"},"finish_reason":"stop"}]}"#,
            "",
            "data: [DONE]",
            "",
            "",
        ]
        .join("\n")
    }

    fn anthropic_chat_response(content: &str, include_tool: bool) -> Value {
        let mut content_blocks = vec![json!({
            "type": "text",
            "text": content
        })];
        if include_tool {
            content_blocks.push(json!({
                "type": "tool_use",
                "id": "toolu_1",
                "name": "lookup",
                "input": { "query": "rust" }
            }));
        }

        json!({
            "id": "msg-test",
            "type": "message",
            "role": "assistant",
            "model": "claude-test",
            "content": content_blocks,
            "stop_reason": if include_tool { "tool_use" } else { "end_turn" },
            "usage": {
                "input_tokens": 3,
                "output_tokens": 5
            }
        })
    }

    fn anthropic_stream_response() -> String {
        [
            r#"data: {"type":"message_start","message":{"id":"msg-stream","type":"message","role":"assistant","model":"claude-test","usage":{"input_tokens":3,"output_tokens":0}}}"#,
            "",
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            "",
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}"#,
            "",
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#,
            "",
            r#"data: {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"lookup","input":null}}"#,
            "",
            r#"data: {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"query\":\"rust\"}"}}"#,
            "",
            r#"data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"input_tokens":3,"output_tokens":5}}"#,
            "",
            r#"data: {"type":"message_stop"}"#,
            "",
            "",
        ]
        .join("\n")
    }

    fn anthropic_text_stream_response() -> String {
        [
            r#"data: {"type":"message_start","message":{"id":"msg-stream","type":"message","role":"assistant","model":"claude-test","usage":{"input_tokens":3,"output_tokens":0}}}"#,
            "",
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}"#,
            "",
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#,
            "",
            r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":3,"output_tokens":5}}"#,
            "",
            r#"data: {"type":"message_stop"}"#,
            "",
            "",
        ]
        .join("\n")
    }

    fn gemini_chat_response(content: &str, include_tool: bool) -> Value {
        let mut parts = vec![json!({ "text": content })];
        if include_tool {
            parts.push(json!({
                "functionCall": {
                    "name": "lookup",
                    "args": { "query": "rust" }
                }
            }));
        }

        json!({
            "responseId": "gemini-response-test",
            "modelVersion": "gemini-test",
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": parts
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 3,
                "candidatesTokenCount": 5,
                "totalTokenCount": 8
            }
        })
    }

    fn gemini_stream_response() -> String {
        [
            r#"data: {"responseId":"gemini-response-test","modelVersion":"gemini-test","candidates":[{"content":{"role":"model","parts":[{"text":"Hel"}]},"finishReason":null}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2,"totalTokenCount":5}}"#,
            "",
            r#"data: {"responseId":"gemini-response-test","modelVersion":"gemini-test","candidates":[{"content":{"role":"model","parts":[{"text":"lo"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":5,"totalTokenCount":8}}"#,
            "",
            "data: [DONE]",
            "",
            "",
        ]
        .join("\n")
    }
}
