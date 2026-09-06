use async_trait::async_trait;
use errors::ContextraError;
use memory::{LongTermMemory, MemoryStore};
use prompts::{OptimizedPromptContext, PromptBuilder, PromptOptimizer};
use providers::{ChatRequest, ChatResponse, LLMProvider};
use retrieval::{RankedChunk, RetrievalRequest, Retriever};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use types::{ConversationId, Message, Metadata, UserId};
use uuid::Uuid;

const DEFAULT_CONTEXT_LIMIT: usize = 12;
const DEFAULT_MEMORY_LIMIT: usize = 8;
const DEFAULT_TOKEN_BUDGET: usize = 8_000;
const DEFAULT_MODEL: &str = "gpt-4.1-mini";
const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful assistant.";

#[async_trait]
pub trait ConversationContextStore: Send + Sync {
    async fn context_window(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Vec<Message>, ContextraError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextRequest {
    pub query: String,
    pub user_id: UserId,
    pub conversation_id: ConversationId,
    pub retrieval: RetrievalRequest,
    #[serde(default = "default_memory_limit")]
    pub memory_limit: usize,
    #[serde(default = "default_context_limit")]
    pub context_limit: usize,
    #[serde(default = "default_token_budget")]
    pub token_budget: usize,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

impl ContextRequest {
    pub fn new(
        query: impl Into<String>,
        user_id: UserId,
        conversation_id: ConversationId,
        collection: impl Into<String>,
    ) -> Self {
        let query = query.into();
        Self {
            retrieval: RetrievalRequest::hybrid(
                query.clone(),
                collection.into(),
                DEFAULT_CONTEXT_LIMIT,
            ),
            query,
            user_id,
            conversation_id,
            memory_limit: DEFAULT_MEMORY_LIMIT,
            context_limit: DEFAULT_CONTEXT_LIMIT,
            token_budget: DEFAULT_TOKEN_BUDGET,
            model: DEFAULT_MODEL.to_string(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
        }
    }

    pub fn with_retrieval(mut self, retrieval: RetrievalRequest) -> Self {
        self.retrieval = retrieval;
        self
    }

    pub fn with_memory_limit(mut self, memory_limit: usize) -> Self {
        self.memory_limit = memory_limit.max(1);
        self
    }

    pub fn with_context_limit(mut self, context_limit: usize) -> Self {
        self.context_limit = context_limit.max(1);
        self.retrieval.limit = self.context_limit;
        self
    }

    pub fn with_token_budget(mut self, token_budget: usize) -> Self {
        self.token_budget = token_budget.max(1);
        self
    }

    pub fn with_prompt(
        mut self,
        model: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> Self {
        self.model = model.into();
        self.system_prompt = system_prompt.into();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextItemKind {
    RetrievedChunk,
    LongTermMemory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContextItemSource {
    RetrievedChunk(RankedChunk),
    LongTermMemory(LongTermMemory),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedContextItem {
    pub id: Uuid,
    pub kind: ContextItemKind,
    pub content: String,
    pub score: f32,
    pub retrieval_score: f32,
    pub recency_score: f32,
    pub memory_importance: f32,
    #[serde(default)]
    pub metadata: Metadata,
    pub source: ContextItemSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextPackage {
    pub query: String,
    pub user_id: UserId,
    pub conversation_id: ConversationId,
    pub ranked_items: Vec<RankedContextItem>,
    pub retrieved_context: Vec<RankedChunk>,
    pub memories: Vec<LongTermMemory>,
    pub conversation_history: Vec<Message>,
    pub optimized: OptimizedPromptContext,
    pub chat_request: ChatRequest,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextRankingWeights {
    pub retrieval: f32,
    pub recency: f32,
    pub memory_importance: f32,
}

impl ContextRankingWeights {
    pub fn normalized(self) -> Self {
        let retrieval = self.retrieval.max(0.0);
        let recency = self.recency.max(0.0);
        let memory_importance = self.memory_importance.max(0.0);
        let total = (retrieval + recency + memory_importance).max(f32::EPSILON);

        Self {
            retrieval: retrieval / total,
            recency: recency / total,
            memory_importance: memory_importance / total,
        }
    }
}

impl Default for ContextRankingWeights {
    fn default() -> Self {
        Self {
            retrieval: 0.55,
            recency: 0.20,
            memory_importance: 0.25,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextRanker {
    weights: ContextRankingWeights,
}

impl ContextRanker {
    pub fn new(weights: ContextRankingWeights) -> Self {
        Self {
            weights: weights.normalized(),
        }
    }

    pub fn rank(
        &self,
        retrieved: Vec<RankedChunk>,
        memories: Vec<LongTermMemory>,
    ) -> Vec<RankedContextItem> {
        let max_retrieval = retrieved
            .iter()
            .map(|chunk| chunk.score)
            .fold(0.0_f32, f32::max);
        let retrieval_count = retrieved.len();
        let memory_count = memories.len();

        let mut items = retrieved
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| {
                let retrieval_score = normalize_by_max(chunk.score, max_retrieval);
                let recency_score = recency_score(&chunk.payload, index, retrieval_count);
                let score = self.combined_score(retrieval_score, recency_score, 0.0);

                RankedContextItem {
                    id: chunk.id,
                    kind: ContextItemKind::RetrievedChunk,
                    content: chunk.content.clone(),
                    score,
                    retrieval_score,
                    recency_score,
                    memory_importance: 0.0,
                    metadata: chunk.payload.clone(),
                    source: ContextItemSource::RetrievedChunk(chunk),
                }
            })
            .collect::<Vec<_>>();

        items.extend(memories.into_iter().enumerate().map(|(index, memory)| {
            let memory_importance = memory.importance.clamp(0.0, 1.0);
            let recency_score = recency_score(&memory.metadata, index, memory_count);
            let score = self.combined_score(0.0, recency_score, memory_importance);

            RankedContextItem {
                id: memory.id,
                kind: ContextItemKind::LongTermMemory,
                content: memory.content.clone(),
                score,
                retrieval_score: 0.0,
                recency_score,
                memory_importance,
                metadata: memory.metadata.clone(),
                source: ContextItemSource::LongTermMemory(memory),
            }
        }));

        items.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });
        items
    }

    fn combined_score(
        &self,
        retrieval_score: f32,
        recency_score: f32,
        memory_importance: f32,
    ) -> f32 {
        (retrieval_score * self.weights.retrieval)
            + (recency_score * self.weights.recency)
            + (memory_importance * self.weights.memory_importance)
    }
}

impl Default for ContextRanker {
    fn default() -> Self {
        Self::new(ContextRankingWeights::default())
    }
}

#[derive(Debug, Clone)]
pub struct ContextEngine<R, M, C> {
    retriever: R,
    memory_store: M,
    conversation_store: C,
    ranker: ContextRanker,
}

impl<R, M, C> ContextEngine<R, M, C> {
    pub fn new(retriever: R, memory_store: M, conversation_store: C) -> Self {
        Self {
            retriever,
            memory_store,
            conversation_store,
            ranker: ContextRanker::default(),
        }
    }

    pub fn with_ranker(mut self, ranker: ContextRanker) -> Self {
        self.ranker = ranker;
        self
    }
}

impl<R, M, C> ContextEngine<R, M, C>
where
    R: Retriever,
    M: MemoryStore,
    C: ConversationContextStore,
{
    pub async fn assemble(
        &self,
        mut request: ContextRequest,
    ) -> Result<ContextPackage, ContextraError> {
        if request.query.trim().is_empty() {
            return Err(ContextraError::Validation(
                "context query cannot be empty".to_string(),
            ));
        }

        request.context_limit = request.context_limit.max(1);
        request.memory_limit = request.memory_limit.max(1);
        request.token_budget = request.token_budget.max(1);
        request.retrieval.limit = request.retrieval.limit.max(request.context_limit);

        let retrieved_documents = self.retriever.retrieve(request.retrieval.clone()).await?;
        let retrieved = retrieved_documents
            .into_iter()
            .map(RankedChunk::from_retrieved)
            .collect::<Vec<_>>();
        let memories = self
            .memory_store
            .recall(request.user_id, &request.query, request.memory_limit)
            .await?;
        let conversation_history = self
            .conversation_store
            .context_window(&request.conversation_id)
            .await?;

        let mut ranked_items = self.ranker.rank(retrieved, memories);
        ranked_items.truncate(request.context_limit);

        let (ranked_retrieved, ranked_memories) = split_ranked_items(&ranked_items);
        let optimizer = PromptOptimizer::new(request.token_budget);
        let optimized =
            optimizer.optimize(&ranked_retrieved, &ranked_memories, &conversation_history);
        let builder = PromptBuilder::new(request.model.clone(), request.system_prompt.clone())
            .with_optimizer(optimizer);
        let chat_request = builder.build_from_optimized(optimized.clone());

        Ok(ContextPackage {
            query: request.query,
            user_id: request.user_id,
            conversation_id: request.conversation_id,
            ranked_items,
            retrieved_context: ranked_retrieved,
            memories: ranked_memories,
            conversation_history,
            optimized,
            chat_request,
        })
    }

    pub async fn execute<P>(
        &self,
        request: ContextRequest,
        provider: &P,
    ) -> Result<(ContextPackage, ChatResponse), ContextraError>
    where
        P: LLMProvider,
    {
        let package = self.assemble(request).await?;
        let response = provider
            .chat(package.chat_request.clone())
            .await
            .map_err(ContextraError::from)?;
        Ok((package, response))
    }
}

fn split_ranked_items(items: &[RankedContextItem]) -> (Vec<RankedChunk>, Vec<LongTermMemory>) {
    let mut chunks = Vec::new();
    let mut memories = Vec::new();

    for item in items {
        match &item.source {
            ContextItemSource::RetrievedChunk(chunk) => {
                let mut chunk = chunk.clone();
                chunk.score = item.score;
                chunks.push(chunk);
            }
            ContextItemSource::LongTermMemory(memory) => memories.push(memory.clone()),
        }
    }

    (chunks, memories)
}

fn recency_score(metadata: &Metadata, index: usize, total: usize) -> f32 {
    if let Some(score) = metadata
        .get("recency")
        .and_then(|value| value.as_f64())
        .map(|value| value as f32)
    {
        return score.clamp(0.0, 1.0);
    }

    for key in [
        "updated_at_epoch_seconds",
        "created_at_epoch_seconds",
        "timestamp_epoch_seconds",
    ] {
        if let Some(epoch) = metadata
            .get(key)
            .and_then(|value| value.as_f64())
            .map(|value| value as f32)
        {
            return (epoch / (epoch + 86_400.0)).clamp(0.0, 1.0);
        }
    }

    if total <= 1 {
        1.0
    } else {
        1.0 - (index as f32 / (total - 1) as f32)
    }
}

fn normalize_by_max(score: f32, max_score: f32) -> f32 {
    if max_score > 0.0 {
        (score / max_score).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn default_context_limit() -> usize {
    DEFAULT_CONTEXT_LIMIT
}

fn default_memory_limit() -> usize {
    DEFAULT_MEMORY_LIMIT
}

fn default_token_budget() -> usize {
    DEFAULT_TOKEN_BUDGET
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

fn default_system_prompt() -> String {
    DEFAULT_SYSTEM_PROMPT.to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use futures_util::stream;
    use memory::{LongTermMemoryKind, MemoryStore};
    use providers::{ChatMessage, ChatResponse, ChatRole, ChatStream, ProviderError, TokenUsage};
    use retrieval::{RetrievalMode, RetrievedDocument};
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use types::Role;

    #[tokio::test]
    async fn engine_assembles_ranked_and_compressed_context_package()
    -> Result<(), Box<dyn std::error::Error>> {
        let user_id = UserId::new();
        let conversation_id = ConversationId::new();
        let retrieval = MockRetriever::new(vec![
            retrieved_document(
                1,
                0.4,
                "stale lower scoring retrieval context that should lose to memory",
                0.1,
            ),
            retrieved_document(2, 0.9, "high signal retrieved architecture note", 0.8),
        ]);
        let memory = MockMemoryStore::new(vec![
            LongTermMemory::new(
                user_id,
                LongTermMemoryKind::Preference,
                "User prefers concise implementation notes",
                0.95,
                metadata_with_recency(0.9),
            ),
            LongTermMemory::new(
                user_id,
                LongTermMemoryKind::Fact,
                "Low importance memory that should be trimmed by budget",
                0.1,
                metadata_with_recency(0.2),
            ),
        ]);
        let conversation = MockConversationStore::new(vec![
            message(conversation_id, Role::User, "older turn"),
            message(conversation_id, Role::Assistant, "recent answer"),
            message(conversation_id, Role::User, "assemble context please"),
        ]);
        let engine = ContextEngine::new(retrieval, memory, conversation);
        let request = ContextRequest::new(
            "How should the Context Engine work?",
            user_id,
            conversation_id,
            "docs",
        )
        .with_context_limit(3)
        .with_memory_limit(2)
        .with_token_budget(80)
        .with_prompt("mock-model", "System base");

        let package = engine.assemble(request).await?;

        assert_eq!(package.chat_request.model, "mock-model");
        assert_eq!(package.ranked_items.len(), 3);
        assert_eq!(
            package.ranked_items[0].content,
            "high signal retrieved architecture note"
        );
        assert!(
            package
                .ranked_items
                .iter()
                .any(|item| item.kind == ContextItemKind::LongTermMemory)
        );
        assert!(
            package
                .optimized
                .retrieved_context
                .iter()
                .any(|chunk| chunk.content.contains("architecture note"))
        );
        assert!(package.optimized.token_count <= 80);
        assert_eq!(package.chat_request.messages[0].role, ChatRole::System);
        assert!(package.chat_request.messages.iter().any(|message| {
            message.content.as_deref().is_some_and(|content| {
                content.contains("Retrieved context") && content.contains("Relevant memory")
            })
        }));
        Ok(())
    }

    #[tokio::test]
    async fn engine_executes_provider_with_final_context_package()
    -> Result<(), Box<dyn std::error::Error>> {
        let user_id = UserId::new();
        let conversation_id = ConversationId::new();
        let retrieval = MockRetriever::new(vec![retrieved_document(
            7,
            1.0,
            "provider execution context",
            1.0,
        )]);
        let memory = MockMemoryStore::new(vec![LongTermMemory::new(
            user_id,
            LongTermMemoryKind::Fact,
            "provider should receive memory too",
            0.8,
            metadata_with_recency(0.7),
        )]);
        let conversation = MockConversationStore::new(vec![message(
            conversation_id,
            Role::User,
            "run the provider",
        )]);
        let provider = MockProvider::default();
        let engine = ContextEngine::new(retrieval, memory, conversation);
        let request = ContextRequest::new("run", user_id, conversation_id, "docs")
            .with_prompt("provider-model", "System base");

        let (package, response) = engine.execute(request, &provider).await?;

        assert_eq!(response.message.content.as_deref(), Some("mocked response"));
        assert_eq!(package.chat_request.model, "provider-model");
        let captured = provider.captured.lock().unwrap();
        let sent = captured.as_ref().unwrap();
        assert_eq!(sent.model, "provider-model");
        assert!(sent.messages.iter().any(|message| {
            message
                .content
                .as_deref()
                .is_some_and(|content| content.contains("provider execution context"))
        }));
        Ok(())
    }

    #[derive(Debug, Clone)]
    struct MockRetriever {
        documents: Vec<RetrievedDocument>,
    }

    impl MockRetriever {
        fn new(documents: Vec<RetrievedDocument>) -> Self {
            Self { documents }
        }
    }

    #[async_trait]
    impl Retriever for MockRetriever {
        async fn retrieve(
            &self,
            request: RetrievalRequest,
        ) -> Result<Vec<RetrievedDocument>, ContextraError> {
            assert_eq!(request.mode, RetrievalMode::Hybrid);
            Ok(self.documents.clone())
        }
    }

    #[derive(Debug, Clone)]
    struct MockMemoryStore {
        memories: Vec<LongTermMemory>,
    }

    impl MockMemoryStore {
        fn new(memories: Vec<LongTermMemory>) -> Self {
            Self { memories }
        }
    }

    #[async_trait]
    impl MemoryStore for MockMemoryStore {
        async fn remember(&self, _memory: LongTermMemory) -> Result<(), ContextraError> {
            Ok(())
        }

        async fn recall(
            &self,
            _user_id: UserId,
            _query: &str,
            limit: usize,
        ) -> Result<Vec<LongTermMemory>, ContextraError> {
            Ok(self.memories.iter().take(limit).cloned().collect())
        }

        async fn forget(&self, _ids: &[Uuid]) -> Result<(), ContextraError> {
            Ok(())
        }

        async fn update(&self, _memory: LongTermMemory) -> Result<(), ContextraError> {
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<LongTermMemory>, ContextraError> {
            Ok(self.memories.iter().find(|m| m.id == id).cloned())
        }
    }

    #[derive(Debug, Clone)]
    struct MockConversationStore {
        messages: Vec<Message>,
    }

    impl MockConversationStore {
        fn new(messages: Vec<Message>) -> Self {
            Self { messages }
        }
    }

    #[async_trait]
    impl ConversationContextStore for MockConversationStore {
        async fn context_window(
            &self,
            _conversation_id: &ConversationId,
        ) -> Result<Vec<Message>, ContextraError> {
            Ok(self.messages.clone())
        }
    }

    #[derive(Debug, Clone, Default)]
    struct MockProvider {
        captured: Arc<Mutex<Option<ChatRequest>>>,
    }

    #[async_trait]
    impl LLMProvider for MockProvider {
        async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
            *self.captured.lock().unwrap() = Some(request.clone());
            Ok(ChatResponse {
                id: "mock-response".to_string(),
                model: request.model,
                message: ChatMessage::assistant("mocked response"),
                finish_reason: Some("stop".to_string()),
                usage: Some(TokenUsage {
                    prompt_tokens: request.messages.len() as u32,
                    completion_tokens: 2,
                    total_tokens: request.messages.len() as u32 + 2,
                }),
            })
        }

        async fn chat_stream(&self, _request: ChatRequest) -> Result<ChatStream, ProviderError> {
            Ok(Box::pin(stream::empty()))
        }

        fn supports_function_calling(&self) -> bool {
            false
        }
    }

    fn retrieved_document(id: u128, score: f32, content: &str, recency: f32) -> RetrievedDocument {
        RetrievedDocument {
            id: Uuid::from_u128(id),
            score,
            semantic_score: Some(score),
            keyword_score: None,
            metadata_score: None,
            fusion_score: None,
            payload: {
                let mut metadata = metadata_with_recency(recency);
                metadata.insert("content".to_string(), json!(content));
                metadata
            },
        }
    }

    fn metadata_with_recency(recency: f32) -> Metadata {
        let mut metadata = Metadata::new();
        metadata.insert("recency".to_string(), json!(recency));
        metadata
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
}
