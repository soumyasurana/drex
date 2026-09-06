use async_trait::async_trait;
use embeddings::EmbeddingProvider;
use errors::ContextraError;
use providers::{ChatMessage, ChatRequest, LLMProvider};
use retrieval::RetrievalFilter;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use storage::cache::Cache;
use storage::conversation::ConversationRepository;
use storage::repository::Repository;
use storage::session_store::SessionStore;
use storage::vector_store::{SearchResult as VectorSearchResult, VectorRecord, VectorStore};
use types::{ConversationId, Message, Metadata, Role, UserId};
use uuid::Uuid;

const DEFAULT_CONTEXT_TOKEN_LIMIT: usize = 4_000;
const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(60 * 60);
const DEFAULT_MEMORY_COLLECTION: &str = "long-term-memory";
const DEFAULT_IMPORTANCE_THRESHOLD: f32 = 0.6;

#[async_trait]
pub trait ConversationHistoryStore: Send + Sync {
    async fn create_conversation(&self, id: &ConversationId) -> Result<(), ContextraError>;

    async fn append_message(&self, message: &Message) -> Result<(), ContextraError>;

    async fn messages(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Vec<Message>, ContextraError>;
}

#[async_trait]
impl ConversationHistoryStore for ConversationRepository {
    async fn create_conversation(&self, id: &ConversationId) -> Result<(), ContextraError> {
        ConversationRepository::create_conversation(self, id).await
    }

    async fn append_message(&self, message: &Message) -> Result<(), ContextraError> {
        self.create(message).await
    }

    async fn messages(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Vec<Message>, ContextraError> {
        self.get_messages_by_conversation(conversation_id).await
    }
}

#[async_trait]
pub trait HotSessionStore<S>: Send + Sync {
    async fn get(&self, conversation_id: &ConversationId) -> Result<Option<S>, ContextraError>;

    async fn set(&self, conversation_id: &ConversationId, state: &S) -> Result<(), ContextraError>;

    async fn delete(&self, conversation_id: &ConversationId) -> Result<(), ContextraError>;

    async fn exists(&self, conversation_id: &ConversationId) -> Result<bool, ContextraError>;
}

#[async_trait]
impl<C, S> HotSessionStore<S> for SessionStore<C, S>
where
    C: Cache,
    S: Serialize + DeserializeOwned + Send + Sync,
{
    async fn get(&self, conversation_id: &ConversationId) -> Result<Option<S>, ContextraError> {
        SessionStore::get(self, conversation_id).await
    }

    async fn set(&self, conversation_id: &ConversationId, state: &S) -> Result<(), ContextraError> {
        SessionStore::set(self, conversation_id, state).await
    }

    async fn delete(&self, conversation_id: &ConversationId) -> Result<(), ContextraError> {
        SessionStore::delete(self, conversation_id).await
    }

    async fn exists(&self, conversation_id: &ConversationId) -> Result<bool, ContextraError> {
        SessionStore::exists(self, conversation_id).await
    }
}

pub trait TokenCounter: Send + Sync {
    fn count_tokens(&self, text: &str) -> usize;

    fn count_message_tokens(&self, message: &Message) -> usize {
        role_token_cost(&message.role) + self.count_tokens(&message.content)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ApproximateTokenCounter;

impl TokenCounter for ApproximateTokenCounter {
    fn count_tokens(&self, text: &str) -> usize {
        let wordish = text
            .split_whitespace()
            .map(|token| token.chars().count().div_ceil(4).max(1))
            .sum::<usize>();

        wordish.max(text.chars().count().div_ceil(4))
    }
}

#[derive(Debug, Clone)]
pub struct SlidingWindowTruncator<T = ApproximateTokenCounter> {
    max_tokens: usize,
    counter: T,
    preserve_system_messages: bool,
}

impl SlidingWindowTruncator<ApproximateTokenCounter> {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens: max_tokens.max(1),
            counter: ApproximateTokenCounter,
            preserve_system_messages: true,
        }
    }
}

impl<T> SlidingWindowTruncator<T>
where
    T: TokenCounter,
{
    pub fn with_counter(max_tokens: usize, counter: T) -> Self {
        Self {
            max_tokens: max_tokens.max(1),
            counter,
            preserve_system_messages: true,
        }
    }

    pub fn preserve_system_messages(mut self, preserve: bool) -> Self {
        self.preserve_system_messages = preserve;
        self
    }

    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    pub fn token_count(&self, messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|message| self.counter.count_message_tokens(message))
            .sum()
    }

    pub fn truncate(&self, messages: &[Message]) -> Vec<Message> {
        let mut selected_reversed = Vec::new();
        let mut used_tokens = 0_usize;

        for message in messages.iter().rev() {
            if self.preserve_system_messages && matches!(message.role, Role::System) {
                continue;
            }

            let tokens = self.counter.count_message_tokens(message);
            if used_tokens + tokens <= self.max_tokens || selected_reversed.is_empty() {
                used_tokens += tokens;
                selected_reversed.push(message.clone());
            } else {
                break;
            }
        }

        selected_reversed.reverse();

        if self.preserve_system_messages {
            let mut preserved = messages
                .iter()
                .filter(|message| matches!(message.role, Role::System))
                .cloned()
                .collect::<Vec<_>>();
            preserved.extend(selected_reversed);
            preserved
        } else {
            selected_reversed
        }
    }
}

impl Default for SlidingWindowTruncator<ApproximateTokenCounter> {
    fn default() -> Self {
        Self::new(DEFAULT_CONTEXT_TOKEN_LIMIT)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationSession {
    pub conversation_id: ConversationId,
    pub created_at_epoch_seconds: u64,
    pub last_active_epoch_seconds: u64,
    pub expires_at_epoch_seconds: u64,
    pub message_count: usize,
}

impl ConversationSession {
    fn new(conversation_id: ConversationId, ttl: Duration) -> Self {
        let now = now_epoch_seconds();
        Self {
            conversation_id,
            created_at_epoch_seconds: now,
            last_active_epoch_seconds: now,
            expires_at_epoch_seconds: now + ttl.as_secs().max(1),
            message_count: 0,
        }
    }

    fn touch(&mut self, ttl: Duration, message_count: usize) {
        let now = now_epoch_seconds();
        self.last_active_epoch_seconds = now;
        self.expires_at_epoch_seconds = now + ttl.as_secs().max(1);
        self.message_count = message_count;
    }
}

#[derive(Debug, Clone)]
pub struct SessionManager<H, S = ConversationSession> {
    hot_store: H,
    ttl: Duration,
    _state: std::marker::PhantomData<S>,
}

impl<H> SessionManager<H, ConversationSession> {
    pub fn new(hot_store: H) -> Self {
        Self::with_ttl(hot_store, DEFAULT_SESSION_TTL)
    }

    pub fn with_ttl(hot_store: H, ttl: Duration) -> Self {
        Self {
            hot_store,
            ttl,
            _state: std::marker::PhantomData,
        }
    }
}

impl<H> SessionManager<H, ConversationSession>
where
    H: HotSessionStore<ConversationSession>,
{
    pub async fn create(&self) -> Result<ConversationSession, ContextraError> {
        let session = ConversationSession::new(ConversationId::new(), self.ttl);
        self.hot_store
            .set(&session.conversation_id, &session)
            .await?;
        Ok(session)
    }

    pub async fn resume(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<ConversationSession>, ContextraError> {
        self.hot_store.get(conversation_id).await
    }

    pub async fn touch(
        &self,
        conversation_id: &ConversationId,
        message_count: usize,
    ) -> Result<ConversationSession, ContextraError> {
        let mut session = self
            .hot_store
            .get(conversation_id)
            .await?
            .unwrap_or_else(|| ConversationSession::new(*conversation_id, self.ttl));
        session.touch(self.ttl, message_count);
        self.hot_store.set(conversation_id, &session).await?;
        Ok(session)
    }

    pub async fn expire(&self, conversation_id: &ConversationId) -> Result<(), ContextraError> {
        self.hot_store.delete(conversation_id).await
    }

    pub async fn exists(&self, conversation_id: &ConversationId) -> Result<bool, ContextraError> {
        self.hot_store.exists(conversation_id).await
    }
}

#[derive(Debug, Clone)]
pub struct ConversationMemory<P, H, T = ApproximateTokenCounter> {
    durable_store: P,
    session_manager: SessionManager<H>,
    truncator: SlidingWindowTruncator<T>,
}

impl<P, H> ConversationMemory<P, H, ApproximateTokenCounter> {
    pub fn new(durable_store: P, hot_store: H) -> Self {
        Self {
            durable_store,
            session_manager: SessionManager::new(hot_store),
            truncator: SlidingWindowTruncator::default(),
        }
    }
}

impl<P, H, T> ConversationMemory<P, H, T>
where
    T: TokenCounter,
{
    pub fn with_truncator(
        durable_store: P,
        hot_store: H,
        truncator: SlidingWindowTruncator<T>,
    ) -> Self {
        Self {
            durable_store,
            session_manager: SessionManager::new(hot_store),
            truncator,
        }
    }

    pub fn with_session_ttl(mut self, ttl: Duration) -> Self {
        self.session_manager.ttl = ttl;
        self
    }

    pub fn session_manager(&self) -> &SessionManager<H> {
        &self.session_manager
    }

    pub fn truncator(&self) -> &SlidingWindowTruncator<T> {
        &self.truncator
    }
}

impl<P, H, T> ConversationMemory<P, H, T>
where
    P: ConversationHistoryStore,
    H: HotSessionStore<ConversationSession>,
    T: TokenCounter,
{
    pub async fn create_session(&self) -> Result<ConversationSession, ContextraError> {
        let session = self.session_manager.create().await?;
        self.durable_store
            .create_conversation(&session.conversation_id)
            .await?;
        Ok(session)
    }

    pub async fn append_message(
        &self,
        conversation_id: ConversationId,
        role: Role,
        content: impl Into<String>,
        metadata: Metadata,
    ) -> Result<Message, ContextraError> {
        let message = Message {
            id: Uuid::now_v7(),
            conversation_id,
            role,
            content: content.into(),
            metadata,
        };

        self.durable_store.append_message(&message).await?;
        let message_count = self.durable_store.messages(&conversation_id).await?.len();
        self.session_manager
            .touch(&conversation_id, message_count)
            .await?;

        Ok(message)
    }

    pub async fn history(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Vec<Message>, ContextraError> {
        self.durable_store.messages(conversation_id).await
    }

    pub async fn context_window(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Vec<Message>, ContextraError> {
        let messages = self.history(conversation_id).await?;
        Ok(self.truncator.truncate(&messages))
    }

    pub async fn resume_session(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<ConversationSession>, ContextraError> {
        self.session_manager.resume(conversation_id).await
    }

    pub async fn expire_session(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<(), ContextraError> {
        self.session_manager.expire(conversation_id).await
    }

    pub async fn summarize_overflow<S>(
        &self,
        conversation_id: &ConversationId,
        summarizer: &S,
        previous_summary: Option<&str>,
    ) -> Result<Option<ConversationSummary>, ContextraError>
    where
        S: Summarizer,
    {
        let messages = self.history(conversation_id).await?;
        if self.truncator.token_count(&messages) <= self.truncator.max_tokens() {
            return Ok(None);
        }

        let retained = self.truncator.truncate(&messages);
        let retained_ids = retained
            .iter()
            .map(|message| message.id)
            .collect::<std::collections::HashSet<_>>();
        let overflow = messages
            .into_iter()
            .filter(|message| {
                !matches!(message.role, Role::System) && !retained_ids.contains(&message.id)
            })
            .collect::<Vec<_>>();

        if overflow.is_empty() {
            return Ok(None);
        }

        summarizer
            .summarize(conversation_id, previous_summary, &overflow)
            .await
            .map(Some)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LongTermMemoryKind {
    Fact,
    Preference,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongTermMemory {
    pub id: Uuid,
    pub user_id: UserId,
    pub kind: LongTermMemoryKind,
    pub content: String,
    pub importance: f32,
    #[serde(default)]
    pub metadata: Metadata,
}

impl LongTermMemory {
    pub fn new(
        user_id: UserId,
        kind: LongTermMemoryKind,
        content: impl Into<String>,
        importance: f32,
        metadata: Metadata,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            user_id,
            kind,
            content: content.into(),
            importance: importance.clamp(0.0, 1.0),
            metadata,
        }
    }
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn remember(&self, memory: LongTermMemory) -> Result<(), ContextraError>;

    async fn recall(
        &self,
        user_id: UserId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<LongTermMemory>, ContextraError>;

    /// Permanently delete memories by their IDs.
    ///
    /// # Arguments
    /// * `ids` - The UUIDs of the memories to delete
    ///
    /// # Errors
    /// Returns `ContextraError::NotFound` if any ID doesn't exist.
    /// Returns `ContextraError::StorageError` if the underlying storage fails.
    async fn forget(&self, ids: &[Uuid]) -> Result<(), ContextraError>;

    /// Update an existing memory in place.
    ///
    /// Note: This replaces the memory content entirely. For partial updates,
    /// callers should read-modify-write.
    ///
    /// # Arguments
    /// * `memory` - The updated memory (must have the same ID as existing)
    ///
    /// # Errors
    /// Returns `ContextraError::NotFound` if the memory doesn't exist.
    /// Returns `ContextraError::StorageError` if the underlying storage fails.
    async fn update(&self, memory: LongTermMemory) -> Result<(), ContextraError>;
}

#[derive(Debug, Clone)]
pub struct VectorMemoryStore<S, E> {
    vector_store: S,
    embedding_provider: E,
    collection_name: String,
}

impl<S, E> VectorMemoryStore<S, E> {
    pub fn new(vector_store: S, embedding_provider: E) -> Self {
        Self {
            vector_store,
            embedding_provider,
            collection_name: DEFAULT_MEMORY_COLLECTION.to_string(),
        }
    }

    pub fn with_collection(mut self, collection_name: impl Into<String>) -> Self {
        self.collection_name = collection_name.into();
        self
    }
}

impl<S, E> VectorMemoryStore<S, E>
where
    S: VectorStore,
    E: EmbeddingProvider,
{
    pub async fn create_collection(&self) -> Result<(), ContextraError> {
        self.vector_store
            .create_collection(&self.collection_name, self.embedding_provider.dimensions())
            .await
    }
}

#[async_trait]
impl<S, E> MemoryStore for VectorMemoryStore<S, E>
where
    S: VectorStore + Send + Sync,
    E: EmbeddingProvider + Send + Sync,
{
    async fn remember(&self, memory: LongTermMemory) -> Result<(), ContextraError> {
        if memory.content.trim().is_empty() {
            return Err(ContextraError::Validation(
                "long-term memory content cannot be empty".to_string(),
            ));
        }

        let embeddings = self
            .embedding_provider
            .embed_batch(std::slice::from_ref(&memory.content))
            .await
            .map_err(ContextraError::from)?;
        
        let embedding = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| {
                ContextraError::ProviderError(
                    "embedding provider returned no memory embedding".to_string(),
                )
            })?;
        
        // Validate embedding dimensions
        if embedding.is_empty() {
            return Err(ContextraError::ProviderError(
                "embedding provider returned empty (0-dimensional) embedding".to_string()
            ));
        }

        let payload = memory_payload(&memory);
        self.vector_store
            .upsert_vectors(
                &self.collection_name,
                &[VectorRecord {
                    id: memory.id,
                    embedding,
                    payload,
                }],
            )
            .await
    }

    async fn recall(
        &self,
        user_id: UserId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<LongTermMemory>, ContextraError> {
        let embeddings = self
            .embedding_provider
            .embed_batch(&[query.to_string()])
            .await
            .map_err(ContextraError::from)?;
        
        let embedding = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| {
                ContextraError::ProviderError(
                    "embedding provider returned no memory query embedding".to_string(),
                )
            })?;
        
        if embedding.is_empty() {
            return Err(ContextraError::ProviderError(
                "embedding provider returned empty (0-dimensional) query embedding".to_string()
            ));
        }
        
        let filter = RetrievalFilter {
            user_id: Some(user_id.to_string()),
            ..RetrievalFilter::default()
        };

        self.vector_store
            .search(
                &self.collection_name,
                &embedding,
                limit.max(1).saturating_mul(4),
            )
            .await?
            .into_iter()
            .filter(|result| filter.matches(&result.payload))
            .take(limit.max(1))
            .map(long_term_memory_from_vector_result)
            .collect()
    }

    async fn forget(&self, ids: &[Uuid]) -> Result<(), ContextraError> {
        if ids.is_empty() {
            return Ok(());
        }
        self.vector_store
            .delete_by_id(&self.collection_name, ids)
            .await
    }

    async fn update(&self, memory: LongTermMemory) -> Result<(), ContextraError> {
        if memory.content.trim().is_empty() {
            return Err(ContextraError::Validation(
                "long-term memory content cannot be empty".to_string(),
            ));
        }

        let embedding = self
            .embedding_provider
            .embed_batch(std::slice::from_ref(&memory.content))
            .await
            .map_err(ContextraError::from)?
            .into_iter()
            .next()
            .ok_or_else(|| {
                ContextraError::ProviderError(
                    "embedding provider returned no memory embedding".to_string(),
                )
            })?;

        let payload = memory_payload(&memory);
        self.vector_store
            .upsert_vectors(
                &self.collection_name,
                &[VectorRecord {
                    id: memory.id,
                    embedding,
                    payload,
                }],
            )
            .await
    }
}

#[derive(Debug, Clone)]
pub struct ImportanceScorer {
    promotion_threshold: f32,
}

impl ImportanceScorer {
    pub fn new(promotion_threshold: f32) -> Self {
        Self {
            promotion_threshold: promotion_threshold.clamp(0.0, 1.0),
        }
    }

    pub fn score_message(&self, message: &Message) -> f32 {
        let normalized = message.content.to_lowercase();
        let mut score: f32 = 0.0;

        if matches!(message.role, Role::User) {
            score += 0.15;
        }
        if contains_any(
            &normalized,
            &["remember", "do not forget", "don't forget", "always"],
        ) {
            score += 0.45;
        }
        if contains_any(
            &normalized,
            &["prefer", "preference", "i like", "i dislike", "favorite"],
        ) {
            score += 0.35;
        }
        if contains_any(
            &normalized,
            &["my name is", "i am", "i work", "my role", "my email"],
        ) {
            score += 0.3;
        }
        if contains_any(&normalized, &["temporary", "for now", "just this once"]) {
            score -= 0.35;
        }
        if message.content.split_whitespace().count() >= 8 {
            score += 0.1;
        }

        score.clamp(0.0, 1.0)
    }

    pub fn should_promote(&self, message: &Message) -> bool {
        self.score_message(message) >= self.promotion_threshold
    }

    pub fn classify(&self, message: &Message) -> LongTermMemoryKind {
        let normalized = message.content.to_lowercase();
        if contains_any(
            &normalized,
            &["prefer", "preference", "i like", "i dislike", "favorite"],
        ) {
            LongTermMemoryKind::Preference
        } else {
            LongTermMemoryKind::Fact
        }
    }

    pub fn promote(&self, user_id: UserId, message: &Message) -> Option<LongTermMemory> {
        let importance = self.score_message(message);
        (importance >= self.promotion_threshold).then(|| {
            LongTermMemory::new(
                user_id,
                self.classify(message),
                message.content.clone(),
                importance,
                Metadata::new(),
            )
        })
    }
}

impl Default for ImportanceScorer {
    fn default() -> Self {
        Self::new(DEFAULT_IMPORTANCE_THRESHOLD)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub conversation_id: ConversationId,
    pub content: String,
    pub summarized_message_ids: Vec<Uuid>,
}

#[async_trait]
pub trait Summarizer: Send + Sync {
    async fn summarize(
        &self,
        conversation_id: &ConversationId,
        previous_summary: Option<&str>,
        messages: &[Message],
    ) -> Result<ConversationSummary, ContextraError>;
}

#[derive(Debug, Clone)]
pub struct ProviderSummarizer<P> {
    provider: P,
    model: String,
    max_tokens: u32,
}

impl<P> ProviderSummarizer<P> {
    pub fn new(provider: P, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
            max_tokens: 512,
        }
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens.max(1);
        self
    }
}

#[async_trait]
impl<P> Summarizer for ProviderSummarizer<P>
where
    P: LLMProvider + Send + Sync,
{
    async fn summarize(
        &self,
        conversation_id: &ConversationId,
        previous_summary: Option<&str>,
        messages: &[Message],
    ) -> Result<ConversationSummary, ContextraError> {
        if messages.is_empty() {
            return Err(ContextraError::Validation(
                "cannot summarize an empty message set".to_string(),
            ));
        }

        let mut request = ChatRequest::new(
            self.model.clone(),
            vec![
                ChatMessage::system(
                    "Condense older conversation turns into a concise running memory summary. Preserve stable facts, user preferences, decisions, and unresolved tasks.",
                ),
                ChatMessage::user(summary_prompt(previous_summary, messages)),
            ],
        );
        request.temperature = Some(0.1);
        request.max_tokens = Some(self.max_tokens);

        let response = self
            .provider
            .chat(request)
            .await
            .map_err(ContextraError::from)?;
        let content = response.message.content.unwrap_or_default();

        Ok(ConversationSummary {
            conversation_id: *conversation_id,
            content,
            summarized_message_ids: messages.iter().map(|message| message.id).collect(),
        })
    }
}

fn memory_payload(memory: &LongTermMemory) -> Metadata {
    let mut payload = memory.metadata.clone();
    payload.insert("memory_id".to_string(), serde_json::json!(memory.id));
    payload.insert(
        "user_id".to_string(),
        serde_json::json!(memory.user_id.to_string()),
    );
    payload.insert(
        "kind".to_string(),
        serde_json::json!(match memory.kind {
            LongTermMemoryKind::Fact => "fact",
            LongTermMemoryKind::Preference => "preference",
            LongTermMemoryKind::Summary => "summary",
        }),
    );
    payload.insert("content".to_string(), serde_json::json!(memory.content));
    payload.insert(
        "importance".to_string(),
        serde_json::json!(memory.importance),
    );
    payload
}

fn long_term_memory_from_vector_result(
    result: VectorSearchResult,
) -> Result<LongTermMemory, ContextraError> {
    let payload = result.payload;
    let id = payload
        .get("memory_id")
        .and_then(|value| value.as_str())
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|error| ContextraError::StorageError(error.to_string()))?
        .unwrap_or(result.id);
    let user_id = payload
        .get("user_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ContextraError::StorageError("memory payload missing user_id".to_string()))?
        .parse::<UserId>()
        .map_err(|error| ContextraError::StorageError(error.to_string()))?;
    let kind = match payload.get("kind").and_then(|value| value.as_str()) {
        Some("preference") => LongTermMemoryKind::Preference,
        Some("summary") => LongTermMemoryKind::Summary,
        _ => LongTermMemoryKind::Fact,
    };
    let content = payload
        .get("content")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ContextraError::StorageError("memory payload missing content".to_string()))?
        .to_string();
    let importance = payload
        .get("importance")
        .and_then(|value| value.as_f64())
        .unwrap_or_default() as f32;

    Ok(LongTermMemory {
        id,
        user_id,
        kind,
        content,
        importance,
        metadata: payload,
    })
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn summary_prompt(previous_summary: Option<&str>, messages: &[Message]) -> String {
    let previous = previous_summary.unwrap_or("No previous summary.");
    let transcript = messages
        .iter()
        .map(|message| {
            format!(
                "{:?}: {}",
                message.role,
                message.content.replace('\n', " ").trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Previous running summary:\n{previous}\n\nOlder turns to condense:\n{transcript}\n\nReturn only the updated running summary."
    )
}

fn role_token_cost(role: &Role) -> usize {
    match role {
        Role::System => 4,
        Role::User | Role::Assistant => 3,
        Role::Tool => 5,
    }
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use providers::{ChatResponse, ChatStream, ProviderError};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Debug, Clone, Copy)]
    struct WordTokenCounter;

    impl TokenCounter for WordTokenCounter {
        fn count_tokens(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }

        fn count_message_tokens(&self, message: &Message) -> usize {
            self.count_tokens(&message.content)
        }
    }

    #[test]
    fn sliding_window_truncates_old_messages_by_token_count() {
        let conversation_id = ConversationId::new();
        let messages = vec![
            message(conversation_id, Role::System, "system prompt remains"),
            message(conversation_id, Role::User, "one two three"),
            message(conversation_id, Role::Assistant, "four five six"),
            message(conversation_id, Role::User, "seven eight"),
            message(conversation_id, Role::Assistant, "nine ten"),
        ];
        let truncator = SlidingWindowTruncator::with_counter(7, WordTokenCounter);

        let window = truncator.truncate(&messages);

        assert_eq!(window.len(), 4);
        assert_eq!(window[0].role, Role::System);
        assert_eq!(window[1].content, "four five six");
        assert_eq!(window[2].content, "seven eight");
        assert_eq!(window[3].content, "nine ten");
        assert_eq!(truncator.token_count(&window[1..]), 7);
    }

    #[tokio::test]
    async fn session_lifecycle_create_append_resume_expire()
    -> Result<(), Box<dyn std::error::Error>> {
        let durable = InMemoryConversationStore::default();
        let hot = InMemoryHotSessionStore::default();
        let memory = ConversationMemory::with_truncator(
            durable.clone(),
            hot.clone(),
            SlidingWindowTruncator::with_counter(10, WordTokenCounter),
        );

        let session = memory.create_session().await?;
        assert!(
            memory
                .session_manager()
                .exists(&session.conversation_id)
                .await?
        );

        memory
            .append_message(
                session.conversation_id,
                Role::User,
                "hello there",
                Metadata::new(),
            )
            .await?;
        memory
            .append_message(
                session.conversation_id,
                Role::Assistant,
                "general kenobi",
                Metadata::new(),
            )
            .await?;

        let resumed = memory
            .resume_session(&session.conversation_id)
            .await?
            .ok_or_else(|| std::io::Error::other("session should exist"))?;
        assert_eq!(resumed.conversation_id, session.conversation_id);
        assert_eq!(resumed.message_count, 2);

        let history = memory.history(&session.conversation_id).await?;
        assert_eq!(history.len(), 2);

        memory.expire_session(&session.conversation_id).await?;
        assert!(
            memory
                .resume_session(&session.conversation_id)
                .await?
                .is_none()
        );
        assert_eq!(memory.history(&session.conversation_id).await?.len(), 2);

        Ok(())
    }

    #[test]
    fn importance_scoring_respects_promotion_thresholds() {
        let conversation_id = ConversationId::new();
        let user_id = UserId::new();
        let scorer = ImportanceScorer::new(0.6);

        let preference = message(
            conversation_id,
            Role::User,
            "Please remember that I prefer concise Rust examples",
        );
        let ephemeral = message(
            conversation_id,
            Role::User,
            "For now use the temporary value just this once",
        );

        assert!(scorer.score_message(&preference) >= 0.6);
        assert!(scorer.should_promote(&preference));
        assert_eq!(scorer.classify(&preference), LongTermMemoryKind::Preference);
        assert!(scorer.promote(user_id, &preference).is_some());

        assert!(scorer.score_message(&ephemeral) < 0.6);
        assert!(!scorer.should_promote(&ephemeral));
        assert!(scorer.promote(user_id, &ephemeral).is_none());
    }

    #[tokio::test]
    async fn summarizer_invoked_when_token_window_is_exceeded()
    -> Result<(), Box<dyn std::error::Error>> {
        let durable = InMemoryConversationStore::default();
        let hot = InMemoryHotSessionStore::default();
        let memory = ConversationMemory::with_truncator(
            durable.clone(),
            hot,
            SlidingWindowTruncator::with_counter(4, WordTokenCounter),
        );
        let session = memory.create_session().await?;

        memory
            .append_message(
                session.conversation_id,
                Role::User,
                "old preference one",
                Metadata::new(),
            )
            .await?;
        memory
            .append_message(
                session.conversation_id,
                Role::Assistant,
                "old answer two",
                Metadata::new(),
            )
            .await?;
        memory
            .append_message(
                session.conversation_id,
                Role::User,
                "latest request",
                Metadata::new(),
            )
            .await?;

        let provider = MockLlmProvider::default();
        let summarizer = ProviderSummarizer::new(provider.clone(), "mock-summary");

        let summary = memory
            .summarize_overflow(
                &session.conversation_id,
                &summarizer,
                Some("existing facts"),
            )
            .await?
            .ok_or_else(|| std::io::Error::other("summary should be produced"))?;

        assert_eq!(summary.conversation_id, session.conversation_id);
        assert_eq!(summary.content, "updated running summary");
        assert_eq!(summary.summarized_message_ids.len(), 2);
        assert_eq!(*provider.calls.lock().await, 1);
        let prompt = provider
            .last_prompt
            .lock()
            .await
            .clone()
            .ok_or_else(|| std::io::Error::other("prompt should be captured"))?;
        assert!(prompt.contains("existing facts"));
        assert!(prompt.contains("old preference one"));
        assert!(!prompt.contains("latest request"));

        Ok(())
    }

    #[derive(Debug, Clone, Default)]
    struct InMemoryConversationStore {
        state: Arc<Mutex<HashMap<ConversationId, Vec<Message>>>>,
    }

    #[async_trait]
    impl ConversationHistoryStore for InMemoryConversationStore {
        async fn create_conversation(&self, id: &ConversationId) -> Result<(), ContextraError> {
            self.state.lock().await.entry(*id).or_default();
            Ok(())
        }

        async fn append_message(&self, message: &Message) -> Result<(), ContextraError> {
            let mut state = self.state.lock().await;
            let messages = state.get_mut(&message.conversation_id).ok_or_else(|| {
                ContextraError::NotFound(format!(
                    "conversation {} not found",
                    message.conversation_id
                ))
            })?;
            messages.push(message.clone());
            Ok(())
        }

        async fn messages(
            &self,
            conversation_id: &ConversationId,
        ) -> Result<Vec<Message>, ContextraError> {
            Ok(self
                .state
                .lock()
                .await
                .get(conversation_id)
                .cloned()
                .unwrap_or_default())
        }
    }

    #[derive(Debug, Clone, Default)]
    struct InMemoryHotSessionStore {
        state: Arc<Mutex<HashMap<ConversationId, ConversationSession>>>,
    }

    #[async_trait]
    impl HotSessionStore<ConversationSession> for InMemoryHotSessionStore {
        async fn get(
            &self,
            conversation_id: &ConversationId,
        ) -> Result<Option<ConversationSession>, ContextraError> {
            Ok(self.state.lock().await.get(conversation_id).cloned())
        }

        async fn set(
            &self,
            conversation_id: &ConversationId,
            state: &ConversationSession,
        ) -> Result<(), ContextraError> {
            self.state
                .lock()
                .await
                .insert(*conversation_id, state.clone());
            Ok(())
        }

        async fn delete(&self, conversation_id: &ConversationId) -> Result<(), ContextraError> {
            self.state.lock().await.remove(conversation_id);
            Ok(())
        }

        async fn exists(&self, conversation_id: &ConversationId) -> Result<bool, ContextraError> {
            Ok(self.state.lock().await.contains_key(conversation_id))
        }
    }

    #[derive(Debug, Clone, Default)]
    struct MockLlmProvider {
        calls: Arc<Mutex<usize>>,
        last_prompt: Arc<Mutex<Option<String>>>,
    }

    #[async_trait]
    impl LLMProvider for MockLlmProvider {
        async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
            *self.calls.lock().await += 1;
            *self.last_prompt.lock().await = request
                .messages
                .iter()
                .rev()
                .find_map(|message| message.content.clone());

            Ok(ChatResponse {
                id: "summary-response".to_string(),
                model: request.model,
                message: ChatMessage::assistant("updated running summary"),
                finish_reason: Some("stop".to_string()),
                usage: None,
            })
        }

        async fn chat_stream(&self, _request: ChatRequest) -> Result<ChatStream, ProviderError> {
            Err(ProviderError::UnsupportedProvider(
                "mock stream unsupported".to_string(),
            ))
        }

        fn supports_function_calling(&self) -> bool {
            false
        }
    }

    fn message(conversation_id: ConversationId, role: Role, content: &str) -> Message {
        Message {
            id: Uuid::now_v7(),
            conversation_id,
            role,
            content: content.to_string(),
            metadata: Metadata::new(),
        }
    }

    /// Integration test for Ollama-based embedding generation.
    /// 
    /// This test requires:
    /// - Ollama running at http://localhost:11434
    /// - nomic-embed-text model pulled
    /// - Qdrant running at http://localhost:6333
    /// 
    /// Run with: cargo test -p memory --lib vector_memory_store_with_ollama -- --ignored
    #[tokio::test]
    #[ignore = "Requires Ollama and Qdrant to be running"]
    async fn vector_memory_store_with_ollama() -> Result<(), Box<dyn std::error::Error>> {
        use embeddings::OllamaEmbeddingProvider;
        use storage::vector_store::QdrantVectorStore;
        use uuid::Uuid;

        // Connect to Qdrant
        let vector_store = QdrantVectorStore::connect("http://localhost:6333", None)?;
        
        // Create Ollama embedding provider
        let embedding_provider = OllamaEmbeddingProvider::with_base_url(
            "nomic-embed-text",
            768,
            "http://localhost:11434",
        );
        
        // Create VectorMemoryStore with unique collection
        let memory_store = VectorMemoryStore::new(vector_store, embedding_provider);
        
        // Ensure collection exists
        memory_store.create_collection().await?;

        // Try to remember something
        let memory = LongTermMemory::new(
            UserId::new(),
            LongTermMemoryKind::Fact,
            "Test memory content for Ollama integration",
            0.8,
            Metadata::new(),
        );

        memory_store.remember(memory).await
            .map_err(|e| format!("Failed to store memory: {}", e))?;

        // Verify we can recall it
        let results = memory_store
            .recall(UserId::new(), "test memory", 5)
            .await?;
        
        assert!(!results.is_empty(), "Should find at least one result");
        
        Ok(())
    }
}
