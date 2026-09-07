//! Conversation Management - Long-running context & session persistence
//!
//! This module provides structured conversation management for DREX, enabling:
//! - Multi-turn conversation persistence
//! - Context window management with token budgets
//! - Conversation branching and forking
//! - Session lifecycle management (create, pause, resume, archive)
//! - Message threading and history traversal
//! - Automatic memory extraction from conversations
//!
//! # Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                      ConversationManager                         │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐ │
//! │  │  Session    │  │   Message   │  │   ContextCompressor     │ │
//! │  │   Store     │  │   Thread    │  │                         │ │
//! │  └─────────────┘  └─────────────┘  └─────────────────────────┘ │
//! └─────────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        Session Types                            │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  • Active      - Currently in progress                          │
//! │  • Paused      - Suspended, can resume                        │
//! │  • Archived    - Long-term storage                             │
//! │  • Forked      - Branched from parent conversation             │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

// Token estimation constant (approximate tokens per character)
const TOKEN_ESTIMATE_RATE: f32 = 0.25;

/// A unique identifier for a conversation session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Create a new session ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for SessionId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<SessionId> for Uuid {
    fn from(session_id: SessionId) -> Self {
        session_id.0
    }
}

/// A unique identifier for a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(pub Uuid);

impl MessageId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A unique identifier for a conversation branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BranchId(pub Uuid);

impl BranchId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BranchId {
    fn default() -> Self {
        Self::new()
    }
}

/// The role of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// System message (instructions, context)
    System,
    /// User message
    User,
    /// Assistant/Agent message
    Assistant,
    /// Tool output message
    Tool,
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::User => write!(f, "user"),
            Self::Assistant => write!(f, "assistant"),
            Self::Tool => write!(f, "tool"),
        }
    }
}

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier.
    pub id: MessageId,
    /// The role of the sender.
    pub role: MessageRole,
    /// The message content.
    pub content: String,
    /// When the message was created.
    pub timestamp: DateTime<Utc>,
    /// Optional metadata (tool calls, tokens, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
    /// Parent message ID for threading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<MessageId>,
    /// Optional tool call information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallInfo>>,
}

impl Message {
    /// Create a new message.
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: MessageId::new(),
            role,
            content: content.into(),
            timestamp: Utc::now(),
            metadata: None,
            parent_id: None,
            tool_calls: None,
        }
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageRole::System, content)
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    /// Create a tool message.
    pub fn tool(content: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        let mut msg = Self::new(MessageRole::Tool, content);
        msg.metadata = Some(MessageMetadata {
            tool_call_id: Some(tool_call_id.into()),
            ..Default::default()
        });
        msg
    }

    /// With metadata.
    pub fn with_metadata(mut self, metadata: MessageMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// With parent message.
    pub fn with_parent(mut self, parent_id: MessageId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// With tool calls.
    pub fn with_tool_calls(mut self, calls: Vec<ToolCallInfo>) -> Self {
        self.tool_calls = Some(calls);
        self
    }

    /// Estimate token count for this message.
    pub fn estimated_tokens(&self) -> usize {
        let base_tokens = 4; // Base tokens per message
        let content_tokens = (self.content.len() as f32 * TOKEN_ESTIMATE_RATE) as usize;
        let tool_tokens = self.tool_calls.as_ref()
            .map(|calls| calls.len() * 10)
            .unwrap_or(0);
        base_tokens + content_tokens + tool_tokens
    }
}

/// Metadata for a message.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Token count (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<usize>,
    /// Tool call ID (for tool messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Model used (for assistant messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Additional custom metadata.
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Information about a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    /// Unique ID for this tool call.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Arguments as JSON.
    pub arguments: serde_json::Value,
}

/// Status of a conversation session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session is active and can receive messages.
    Active,
    /// Session is paused (e.g., waiting for user input).
    Paused,
    /// Session is archived (read-only).
    Archived,
    /// Session is frozen at a specific point.
    Frozen,
}

impl SessionStatus {
    /// Check if the session can accept new messages.
    pub fn can_receive_messages(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Check if the session can be resumed.
    pub fn can_resume(&self) -> bool {
        matches!(self, Self::Paused | Self::Frozen)
    }

    /// Check if the session is read-only.
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::Archived | Self::Frozen)
    }
}

/// A conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: SessionId,
    /// Session title/name.
    pub title: String,
    /// Current status.
    pub status: SessionStatus,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last active.
    pub last_active_at: DateTime<Utc>,
    /// The message history.
    pub messages: Vec<Message>,
    /// Current token count estimate.
    pub token_count: usize,
    /// Maximum allowed tokens.
    pub max_tokens: usize,
    /// Parent session ID (if forked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session: Option<SessionId>,
    /// Fork point (message ID where fork occurred).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_point: Option<MessageId>,
    /// Child branches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branches: Option<Vec<BranchId>>,
    /// Context variables (for template substitution).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub context_vars: HashMap<String, String>,
    /// Session tags for organization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<HashSet<String>>,
}

impl Session {
    /// Create a new session with default settings.
    pub fn new(title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::new(),
            title: title.into(),
            status: SessionStatus::Active,
            created_at: now,
            last_active_at: now,
            messages: Vec::new(),
            token_count: 0,
            max_tokens: 128_000, // Default to 128k context window
            parent_session: None,
            fork_point: None,
            branches: None,
            context_vars: HashMap::new(),
            tags: None,
        }
    }

    /// Create a session with custom max tokens.
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Add a message to the session.
    pub fn add_message(&mut self, mut message: Message) {
        // Set parent to last assistant/user message for threading
        if let Some(last) = self.messages.iter().rev().find(|m| {
            matches!(m.role, MessageRole::User | MessageRole::Assistant)
        }) {
            message.parent_id = Some(last.id);
        }

        self.messages.push(message);
        self.update_token_count();
        self.last_active_at = Utc::now();
    }

    /// Add a system message.
    pub fn add_system_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::system(content));
    }

    /// Add a user message.
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::user(content));
    }

    /// Add an assistant message.
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.add_message(Message::assistant(content));
    }

    /// Get messages formatted for LLM API.
    pub fn get_messages_for_llm(&self) -> Vec<LlmMessage> {
        self.messages
            .iter()
            .map(|m| LlmMessage {
                role: match m.role {
                    MessageRole::System => "system".to_string(),
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::Tool => "tool".to_string(),
                },
                content: m.content.clone(),
            })
            .collect()
    }

    /// Get messages within token budget.
    pub fn get_messages_within_budget(&self, max_tokens: usize) -> Vec<&Message> {
        let mut result = Vec::new();
        let mut token_count = 0;

        // Start from the end (most recent) and work backwards
        for msg in self.messages.iter().rev() {
            let msg_tokens = msg.estimated_tokens();
            if token_count + msg_tokens > max_tokens {
                break;
            }
            token_count += msg_tokens;
            result.push(msg);
        }

        // Reverse to maintain chronological order
        result.reverse();
        result
    }

    /// Get the last n messages.
    pub fn get_last_n(&self, n: usize) -> Vec<&Message> {
        self.messages.iter().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect()
    }

    /// Get message by ID.
    pub fn get_message(&self, id: MessageId) -> Option<&Message> {
        self.messages.iter().find(|m| m.id == id)
    }

    /// Truncate messages to fit within token budget.
    pub fn truncate_to_budget(&mut self, max_tokens: usize) {
        let system_messages: Vec<Message> = self.messages
            .iter()
            .filter(|m| m.role == MessageRole::System)
            .cloned()
            .collect();

        let system_tokens: usize = system_messages.iter().map(|m| m.estimated_tokens()).sum();
        let available_tokens = max_tokens.saturating_sub(system_tokens);

        let mut remaining = Vec::new();
        let mut token_count = 0;

        // Keep most recent non-system messages that fit
        for msg in self.messages.iter().rev() {
            if msg.role == MessageRole::System {
                continue;
            }
            let tokens = msg.estimated_tokens();
            if token_count + tokens > available_tokens {
                break;
            }
            token_count += tokens;
            remaining.push(msg.clone());
        }

        remaining.reverse();

        self.messages = system_messages;
        self.messages.extend(remaining);
        self.update_token_count();
    }

    /// Update the token count.
    fn update_token_count(&mut self) {
        self.token_count = self.messages.iter().map(|m| m.estimated_tokens()).sum();
    }

    /// Check if session exceeds token budget.
    pub fn exceeds_budget(&self) -> bool {
        self.token_count > self.max_tokens
    }

    /// Get token usage percentage.
    pub fn token_usage_percent(&self) -> f64 {
        (self.token_count as f64 / self.max_tokens as f64) * 100.0
    }

    /// Pause the session.
    pub fn pause(&mut self) {
        if self.status == SessionStatus::Active {
            self.status = SessionStatus::Paused;
        }
    }

    /// Resume the session.
    pub fn resume(&mut self) {
        if self.status.can_resume() {
            self.status = SessionStatus::Active;
            self.last_active_at = Utc::now();
        }
    }

    /// Archive the session.
    pub fn archive(&mut self) {
        self.status = SessionStatus::Archived;
    }

    /// Fork this session at the given message.
    pub fn fork(&self, at_message: MessageId, new_title: impl Into<String>) -> Session {
        let messages: Vec<Message> = self.messages
            .iter()
            .take_while(|m| m.id != at_message)
            .chain(std::iter::once(self.messages.iter().find(|m| m.id == at_message).unwrap()))
            .cloned()
            .collect();

        let now = Utc::now();
        Session {
            id: SessionId::new(),
            title: new_title.into(),
            status: SessionStatus::Active,
            created_at: now,
            last_active_at: now,
            messages,
            token_count: 0, // Will be recalculated
            max_tokens: self.max_tokens,
            parent_session: Some(self.id),
            fork_point: Some(at_message),
            branches: None,
            context_vars: self.context_vars.clone(),
            tags: self.tags.clone(),
        }
    }

    /// Add a tag.
    pub fn add_tag(&mut self, tag: impl Into<String>) {
        self.tags.get_or_insert_with(HashSet::new).insert(tag.into());
    }

    /// Set a context variable.
    pub fn set_context_var(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.context_vars.insert(key.into(), value.into());
    }

    /// Get context variable.
    pub fn get_context_var(&self, key: &str) -> Option<&String> {
        self.context_vars.get(key)
    }

    /// Get summary statistics.
    pub fn stats(&self) -> SessionStats {
        SessionStats {
            message_count: self.messages.len(),
            user_messages: self.messages.iter().filter(|m| m.role == MessageRole::User).count(),
            assistant_messages: self.messages.iter().filter(|m| m.role == MessageRole::Assistant).count(),
            token_count: self.token_count,
            max_tokens: self.max_tokens,
            token_usage_percent: self.token_usage_percent(),
            duration_seconds: (self.last_active_at - self.created_at).num_seconds(),
        }
    }
}

/// Statistics for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub message_count: usize,
    pub user_messages: usize,
    pub assistant_messages: usize,
    pub token_count: usize,
    pub max_tokens: usize,
    pub token_usage_percent: f64,
    pub duration_seconds: i64,
}

/// Message format for LLM API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
}

/// Compression strategy for long conversations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionStrategy {
    /// Summarize older messages.
    Summarize,
    /// Extract key memories.
    ExtractMemories,
    /// Keep only system + last N messages.
    SlidingWindow,
    /// Compress message content.
    TokenPruning,
}

/// Configuration for context compression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub strategy: CompressionStrategy,
    pub target_tokens: usize,
    pub preserve_system: bool,
    pub preserve_recent_count: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            strategy: CompressionStrategy::SlidingWindow,
            target_tokens: 32_000,
            preserve_system: true,
            preserve_recent_count: 10,
        }
    }
}

/// Context compressor for managing long conversations.
pub struct ContextCompressor;

impl ContextCompressor {
    /// Compress a session to fit within token budget.
    pub fn compress(session: &mut Session, config: &CompressionConfig) -> CompressionResult {
        let original_count = session.messages.len();
        let original_tokens = session.token_count;

        match config.strategy {
            CompressionStrategy::SlidingWindow => {
                // Keep system messages + last N messages
                let system: Vec<Message> = session.messages
                    .iter()
                    .filter(|m| m.role == MessageRole::System)
                    .cloned()
                    .collect();

                let recent: Vec<Message> = session.messages
                    .iter()
                    .rev()
                    .filter(|m| m.role != MessageRole::System)
                    .take(config.preserve_recent_count)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();

                session.messages = system;
                session.messages.extend(recent);
            }
            CompressionStrategy::Summarize => {
                // Mark messages for summarization
                // Actual summarization would need an LLM call
            }
            _ => {
                // Placeholder for other strategies
            }
        }

        session.update_token_count();

        CompressionResult {
            messages_removed: original_count.saturating_sub(session.messages.len()),
            tokens_saved: original_tokens.saturating_sub(session.token_count),
        }
    }
}

/// Result of compression.
#[derive(Debug, Clone)]
pub struct CompressionResult {
    pub messages_removed: usize,
    pub tokens_saved: usize,
}

/// Filter criteria for session queries.
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    pub status: Option<SessionStatus>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub has_tag: Option<String>,
    pub search_text: Option<String>,
}

/// Errors in conversation management.
#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("Session not found: {0}")]
    SessionNotFound(SessionId),
    #[error("Session is archived: {0}")]
    SessionArchived(SessionId),
    #[error("Session is frozen: {0}")]
    SessionFrozen(SessionId),
    #[error("Session exceeds token limit: {current}/{max}")]
    TokenLimitExceeded { current: usize, max: usize },
    #[error("Message not found: {0}")]
    MessageNotFound(MessageId),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Compression failed: {0}")]
    CompressionFailed(String),
}

/// In-memory storage for sessions (for testing and short-term use).
#[derive(Clone)]
pub struct InMemorySessionStore {
    sessions: Arc<RwLock<HashMap<SessionId, Session>>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create(&self, session: Session) -> Result<(), ConversationError> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id, session);
        Ok(())
    }

    pub async fn get(&self, id: SessionId) -> Result<Session, ConversationError> {
        let sessions = self.sessions.read().await;
        sessions.get(&id)
            .cloned()
            .ok_or(ConversationError::SessionNotFound(id))
    }

    pub async fn update(&self, session: Session) -> Result<(), ConversationError> {
        let mut sessions = self.sessions.write().await;
        if !sessions.contains_key(&session.id) {
            return Err(ConversationError::SessionNotFound(session.id));
        }
        sessions.insert(session.id, session);
        Ok(())
    }

    pub async fn delete(&self, id: SessionId) -> Result<(), ConversationError> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(&id)
            .ok_or(ConversationError::SessionNotFound(id))?;
        Ok(())
    }

    pub async fn list(&self, filter: Option<SessionFilter>) -> Result<Vec<Session>, ConversationError> {
        let sessions = self.sessions.read().await;
        let mut result: Vec<Session> = sessions.values().cloned().collect();

        if let Some(filter) = filter {
            result.retain(|s| {
                if let Some(status) = filter.status {
                    if s.status != status {
                        return false;
                    }
                }
                if let Some(after) = filter.created_after {
                    if s.created_at < after {
                        return false;
                    }
                }
                if let Some(before) = filter.created_before {
                    if s.created_at > before {
                        return false;
                    }
                }
                if let Some(ref tag) = filter.has_tag {
                    if !s.tags.as_ref().map(|t| t.contains(tag)).unwrap_or(false) {
                        return false;
                    }
                }
                if let Some(ref text) = filter.search_text {
                    if !s.title.to_lowercase().contains(&text.to_lowercase()) {
                        return false;
                    }
                }
                true
            });
        }

        // Sort by last active (most recent first)
        result.sort_by(|a, b| b.last_active_at.cmp(&a.last_active_at));
        Ok(result)
    }

    pub async fn clear(&self) {
        let mut sessions = self.sessions.write().await;
        sessions.clear();
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for persistent session storage.
#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    async fn create(&self, session: Session) -> Result<(), ConversationError>;
    async fn get(&self, id: SessionId) -> Result<Session, ConversationError>;
    async fn update(&self, session: Session) -> Result<(), ConversationError>;
    async fn delete(&self, id: SessionId) -> Result<(), ConversationError>;
    async fn list(&self, filter: Option<SessionFilter>) -> Result<Vec<Session>, ConversationError>;
}

/// Conversation manager for high-level operations.
pub struct ConversationManager<S: SessionStore> {
    store: S,
    default_max_tokens: usize,
    compression_threshold: f64,
}

impl<S: SessionStore> ConversationManager<S> {
    /// Create a new conversation manager.
    pub fn new(store: S) -> Self {
        Self {
            store,
            default_max_tokens: 128_000,
            compression_threshold: 0.9, // Compress at 90% capacity
        }
    }

    /// With custom configuration.
    pub fn with_config(mut self, max_tokens: usize, compression_threshold: f64) -> Self {
        self.default_max_tokens = max_tokens;
        self.compression_threshold = compression_threshold.clamp(0.0, 1.0);
        self
    }

    /// Create a new session.
    pub async fn create_session(&self, title: impl Into<String>) -> Result<Session, ConversationError> {
        let session = Session::new(title)
            .with_max_tokens(self.default_max_tokens);

        self.store.create(session.clone()).await?;
        info!(session_id = %session.id, "Created new session");
        Ok(session)
    }

    /// Get a session.
    pub async fn get_session(&self, id: SessionId) -> Result<Session, ConversationError> {
        self.store.get(id).await
    }

    /// Add a message to a session with automatic compression.
    pub async fn add_message(
        &self,
        session_id: SessionId,
        message: Message,
    ) -> Result<(), ConversationError> {
        let mut session = self.store.get(session_id).await?;

        if !session.status.can_receive_messages() {
            return Err(match session.status {
                SessionStatus::Archived => ConversationError::SessionArchived(session_id),
                SessionStatus::Frozen => ConversationError::SessionFrozen(session_id),
                _ => ConversationError::SessionNotFound(session_id),
            });
        }

        // Check if we need to compress before adding
        let projected_tokens = session.token_count + message.estimated_tokens();
        let threshold = (self.default_max_tokens as f64 * self.compression_threshold) as usize;

        if projected_tokens > threshold {
            info!(session_id = %session_id, "Compressing session before adding message");
            let config = CompressionConfig {
                target_tokens: self.default_max_tokens / 2,
                ..Default::default()
            };
            ContextCompressor::compress(&mut session, &config);
        }

        session.add_message(message);

        if session.exceeds_budget() {
            return Err(ConversationError::TokenLimitExceeded {
                current: session.token_count,
                max: session.max_tokens,
            });
        }

        self.store.update(session).await?;
        Ok(())
    }

    /// Get messages for LLM API (with budget management).
    pub async fn get_llm_messages(
        &self,
        session_id: SessionId,
        max_tokens: Option<usize>,
    ) -> Result<Vec<LlmMessage>, ConversationError> {
        let session = self.store.get(session_id).await?;
        let max = max_tokens.unwrap_or(session.max_tokens);
        let messages = session.get_messages_within_budget(max);
        Ok(messages.into_iter().map(|m| LlmMessage {
            role: m.role.to_string(),
            content: m.content.clone(),
            // Note: Tool calls would need to be added here for OpenAI format
        }).collect())
    }

    /// Pause a session.
    pub async fn pause_session(&self, session_id: SessionId) -> Result<(), ConversationError> {
        let mut session = self.store.get(session_id).await?;
        session.pause();
        self.store.update(session).await?;
        info!(session_id = %session_id, "Session paused");
        Ok(())
    }

    /// Resume a session.
    pub async fn resume_session(&self, session_id: SessionId) -> Result<(), ConversationError> {
        let mut session = self.store.get(session_id).await?;
        session.resume();
        self.store.update(session).await?;
        info!(session_id = %session_id, "Session resumed");
        Ok(())
    }

    /// Archive a session.
    pub async fn archive_session(&self, session_id: SessionId) -> Result<(), ConversationError> {
        let mut session = self.store.get(session_id).await?;
        session.archive();
        self.store.update(session).await?;
        info!(session_id = %session_id, "Session archived");
        Ok(())
    }

    /// Fork a session at a specific message.
    pub async fn fork_session(
        &self,
        session_id: SessionId,
        at_message: MessageId,
        new_title: impl Into<String>,
    ) -> Result<Session, ConversationError> {
        let parent = self.store.get(session_id).await?;

        if parent.get_message(at_message).is_none() {
            return Err(ConversationError::MessageNotFound(at_message));
        }

        let forked = parent.fork(at_message, new_title);
        self.store.create(forked.clone()).await?;

        info!(session_id = %forked.id, parent_id = %session_id, "Session forked");
        Ok(forked)
    }

    /// List sessions with optional filtering.
    pub async fn list_sessions(
        &self,
        filter: Option<SessionFilter>,
    ) -> Result<Vec<Session>, ConversationError> {
        self.store.list(filter).await
    }

    /// Delete a session.
    pub async fn delete_session(&self, session_id: SessionId) -> Result<(), ConversationError> {
        self.store.delete(session_id).await?;
        info!(session_id = %session_id, "Session deleted");
        Ok(())
    }

    /// Compress a session.
    pub async fn compress_session(
        &self,
        session_id: SessionId,
        config: CompressionConfig,
    ) -> Result<CompressionResult, ConversationError> {
        let mut session = self.store.get(session_id).await?;
        let result = ContextCompressor::compress(&mut session, &config);
        self.store.update(session).await?;
        Ok(result)
    }

    /// Search sessions by text.
    pub async fn search_sessions(
        &self,
        query: &str,
    ) -> Result<Vec<Session>, ConversationError> {
        let filter = SessionFilter {
            search_text: Some(query.to_string()),
            ..Default::default()
        };
        self.store.list(Some(filter)).await
    }

    /// Get session statistics.
    pub async fn session_stats(&self, session_id: SessionId) -> Result<SessionStats, ConversationError> {
        let session = self.store.get(session_id).await?;
        Ok(session.stats())
    }
}

/// Implement SessionStore for InMemorySessionStore
#[async_trait::async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create(&self, session: Session) -> Result<(), ConversationError> {
        InMemorySessionStore::create(self, session).await
    }

    async fn get(&self, id: SessionId) -> Result<Session, ConversationError> {
        InMemorySessionStore::get(self, id).await
    }

    async fn update(&self, session: Session) -> Result<(), ConversationError> {
        InMemorySessionStore::update(self, session).await
    }

    async fn delete(&self, id: SessionId) -> Result<(), ConversationError> {
        InMemorySessionStore::delete(self, id).await
    }

    async fn list(&self, filter: Option<SessionFilter>) -> Result<Vec<Session>, ConversationError> {
        InMemorySessionStore::list(self, filter).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(msg.estimated_tokens() > 0);
    }

    #[test]
    fn test_session_creation() {
        let session = Session::new("Test Session");
        assert_eq!(session.title, "Test Session");
        assert_eq!(session.status, SessionStatus::Active);
        assert!(session.messages.is_empty());
    }

    #[test]
    fn test_session_add_message() {
        let mut session = Session::new("Test");
        session.add_user_message("Hello");
        session.add_assistant_message("Hi there!");

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].role, MessageRole::User);
        assert_eq!(session.messages[1].role, MessageRole::Assistant);
    }

    #[test]
    fn test_session_fork() {
        let mut parent = Session::new("Parent");
        parent.add_user_message("Message 1");
        parent.add_assistant_message("Message 2");
        parent.add_user_message("Message 3");

        let fork_point = parent.messages[1].id;
        let child = parent.fork(fork_point, "Child");

        assert_eq!(child.messages.len(), 2);
        assert_eq!(child.parent_session, Some(parent.id));
        assert_eq!(child.fork_point, Some(fork_point));
    }

    #[test]
    fn test_session_within_budget() {
        let mut session = Session::new("Test").with_max_tokens(100);

        // Add many short messages
        for i in 0..50 {
            session.add_user_message(format!("Short {}", i));
        }

        // Should be able to get messages within budget
        let budget = 50; // tokens
        let messages = session.get_messages_within_budget(budget);
        assert!(!messages.is_empty());
    }

    #[tokio::test]
    async fn test_in_memory_store() {
        let store = InMemorySessionStore::new();
        let session = Session::new("Test");

        store.create(session.clone()).await.unwrap();
        let retrieved = store.get(session.id).await.unwrap();
        assert_eq!(retrieved.title, "Test");

        store.delete(session.id).await.unwrap();
        assert!(store.get(session.id).await.is_err());
    }

    #[tokio::test]
    async fn test_conversation_manager() {
        let store = InMemorySessionStore::new();
        let manager = ConversationManager::new(store);

        let session = manager.create_session("Test Session").await.unwrap();
        assert_eq!(session.title, "Test Session");

        manager.add_message(session.id, Message::user("Hello")).await.unwrap();
        manager.add_message(session.id, Message::assistant("Hi!")).await;

        let messages = manager.get_llm_messages(session.id, None).await.unwrap();
        assert_eq!(messages.len(), 2);

        manager.archive_session(session.id).await.unwrap();
        let archived = manager.get_session(session.id).await.unwrap();
        assert_eq!(archived.status, SessionStatus::Archived);
    }

    #[tokio::test]
    async fn test_session_pause_resume() {
        let store = InMemorySessionStore::new();
        let manager = ConversationManager::new(store);

        let session = manager.create_session("Test").await.unwrap();
        manager.pause_session(session.id).await.unwrap();

        let paused = manager.get_session(session.id).await.unwrap();
        assert_eq!(paused.status, SessionStatus::Paused);

        // Should not be able to add messages to paused session
        let result = manager.add_message(session.id, Message::user("Test")).await;
        assert!(result.is_err());

        manager.resume_session(session.id).await.unwrap();
        let resumed = manager.get_session(session.id).await.unwrap();
        assert_eq!(resumed.status, SessionStatus::Active);

        // Should be able to add messages now
        manager.add_message(session.id, Message::user("Test")).await.unwrap();
    }

    #[tokio::test]
    async fn test_session_filtering() {
        let store = InMemorySessionStore::new();
        let manager = ConversationManager::new(store.clone());

        let s1 = manager.create_session("SearchTest").await.unwrap();
        let _s2 = manager.create_session("Other").await.unwrap();

        let mut s1_mut = s1.clone();
        s1_mut.add_tag("important");
        store.update(s1_mut).await.unwrap();

        let results = manager.search_sessions("Search").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, s1.id);
    }

    #[test]
    fn test_message_with_tool_calls() {
        let tool_calls = vec![
            ToolCallInfo {
                id: "call_1".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({"path": "/tmp/test.txt"}),
            }
        ];

        let msg = Message::assistant("I'll read that file for you.")
            .with_tool_calls(tool_calls);

        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_token_estimation() {
        let msg = Message::system("This is a system message with some content.".to_string());
        let tokens = msg.estimated_tokens();

        // Base 4 tokens + content
        assert!(tokens >= 4);
        // Content is ~45 chars * 0.25 = ~11 tokens + base
        assert!(tokens >= 10);
    }
}
