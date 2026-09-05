use async_trait::async_trait;
use context::{ContextEngine, ContextPackage, ContextRequest};
use errors::ContextraError;
use ingestion::{IngestionPipeline, IngestionResult};
use memory::{
    ConversationHistoryStore, ConversationMemory, ConversationSession, HotSessionStore,
    TokenCounter,
};
use providers::{ChatMessage, ChatResponse, LLMProvider};
use retrieval::{RetrievalFilter, RetrievalMode, RetrievalRequest, Retriever};
use serde_json::json;
use std::path::{Path, PathBuf};
use tracing::{Instrument, info_span};
use types::{ConversationId, Message, Metadata, Role, UserId};

const DEFAULT_COLLECTION: &str = "default";
const DEFAULT_MODEL: &str = "gpt-4.1-mini";
const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful assistant.";
const DEFAULT_CONTEXT_LIMIT: usize = 12;
const DEFAULT_MEMORY_LIMIT: usize = 8;
const DEFAULT_TOKEN_BUDGET: usize = 8_000;

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorConfig {
    pub user_id: UserId,
    pub collection: String,
    pub model: String,
    pub system_prompt: String,
    pub context_limit: usize,
    pub memory_limit: usize,
    pub token_budget: usize,
    pub retrieval_filter: RetrievalFilter,
}

impl OrchestratorConfig {
    pub fn new(user_id: UserId) -> Self {
        Self {
            user_id,
            collection: DEFAULT_COLLECTION.to_string(),
            model: DEFAULT_MODEL.to_string(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            context_limit: DEFAULT_CONTEXT_LIMIT,
            memory_limit: DEFAULT_MEMORY_LIMIT,
            token_budget: DEFAULT_TOKEN_BUDGET,
            retrieval_filter: RetrievalFilter::default(),
        }
    }

    pub fn with_collection(mut self, collection: impl Into<String>) -> Self {
        self.collection = collection.into();
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

    pub fn with_limits(
        mut self,
        context_limit: usize,
        memory_limit: usize,
        token_budget: usize,
    ) -> Self {
        self.context_limit = context_limit.max(1);
        self.memory_limit = memory_limit.max(1);
        self.token_budget = token_budget.max(1);
        self
    }

    pub fn with_retrieval_filter(mut self, filter: RetrievalFilter) -> Self {
        self.retrieval_filter = filter;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionDocument {
    pub path: PathBuf,
}

impl IngestionDocument {
    pub fn path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl<P> From<P> for IngestionDocument
where
    P: AsRef<Path>,
{
    fn from(path: P) -> Self {
        Self::path(path.as_ref().to_path_buf())
    }
}

#[async_trait]
pub trait ContextWorkflow: Send + Sync {
    async fn assemble_context(
        &self,
        request: ContextRequest,
    ) -> Result<ContextPackage, ContextraError>;
}

#[async_trait]
impl<R, M, C> ContextWorkflow for ContextEngine<R, M, C>
where
    R: Retriever + Send + Sync,
    M: memory::MemoryStore + Send + Sync,
    C: context::ConversationContextStore + Send + Sync,
{
    async fn assemble_context(
        &self,
        request: ContextRequest,
    ) -> Result<ContextPackage, ContextraError> {
        self.assemble(request).await
    }
}

#[async_trait]
pub trait ChatMemoryUpdater: Send + Sync {
    async fn append_message(
        &self,
        conversation_id: ConversationId,
        role: Role,
        content: String,
        metadata: Metadata,
    ) -> Result<Message, ContextraError>;
}

#[async_trait]
impl<P, H, T> ChatMemoryUpdater for ConversationMemory<P, H, T>
where
    P: ConversationHistoryStore,
    H: HotSessionStore<ConversationSession>,
    T: TokenCounter,
{
    async fn append_message(
        &self,
        conversation_id: ConversationId,
        role: Role,
        content: String,
        metadata: Metadata,
    ) -> Result<Message, ContextraError> {
        ConversationMemory::append_message(self, conversation_id, role, content, metadata).await
    }
}

#[async_trait(?Send)]
pub trait IngestionWorkflow {
    async fn ingest_document(
        &self,
        document: IngestionDocument,
    ) -> Result<IngestionResult, ContextraError>;
}

#[async_trait(?Send)]
impl<P, C, E, S> IngestionWorkflow for IngestionPipeline<P, C, E, S>
where
    P: ingestion::Parser + Send + Sync,
    C: ingestion::Chunker + Send + Sync,
    E: embeddings::EmbeddingProvider + Send + Sync,
    S: storage::vector_store::VectorStore + Send + Sync,
{
    async fn ingest_document(
        &self,
        document: IngestionDocument,
    ) -> Result<IngestionResult, ContextraError> {
        self.ingest_path(document.path).await
    }
}

#[derive(Debug, Clone)]
pub struct Orchestrator<C, P, M, I> {
    context: C,
    provider: P,
    memory: M,
    ingestion: I,
    config: OrchestratorConfig,
}

impl<C, P, M, I> Orchestrator<C, P, M, I> {
    pub fn new(
        context: C,
        provider: P,
        memory: M,
        ingestion: I,
        config: OrchestratorConfig,
    ) -> Self {
        Self {
            context,
            provider,
            memory,
            ingestion,
            config,
        }
    }

    pub fn config(&self) -> &OrchestratorConfig {
        &self.config
    }
}

impl<C, P, M, I> Orchestrator<C, P, M, I>
where
    C: ContextWorkflow,
    P: LLMProvider,
    M: ChatMemoryUpdater,
    I: IngestionWorkflow,
{
    pub async fn execute_chat(
        &self,
        conversation_id: ConversationId,
        user_message: impl Into<String> + Send,
    ) -> Result<ChatResponse, ContextraError> {
        let user_message = user_message.into();
        let span = info_span!(
            "orchestration.execute_chat",
            conversation_id = %conversation_id,
            user_id = %self.config.user_id
        );

        async move {
            if user_message.trim().is_empty() {
                return Err(ContextraError::Validation(
                    "user_message cannot be empty".to_string(),
                ));
            }

            let context_request = self.context_request(conversation_id, &user_message);
            let mut package = self.context.assemble_context(context_request).await?;
            package
                .chat_request
                .messages
                .push(ChatMessage::user(user_message.clone()));

            let response = self
                .provider
                .chat(package.chat_request.clone())
                .await
                .map_err(ContextraError::from)?;

            self.memory
                .append_message(
                    conversation_id,
                    Role::User,
                    user_message,
                    workflow_metadata("execute_chat.user"),
                )
                .await?;

            if let Some(content) = response.message.content.clone() {
                self.memory
                    .append_message(
                        conversation_id,
                        Role::Assistant,
                        content,
                        workflow_metadata("execute_chat.assistant"),
                    )
                    .await?;
            }

            Ok(response)
        }
        .instrument(span)
        .await
    }

    pub async fn execute_ingestion(
        &self,
        document: IngestionDocument,
    ) -> Result<IngestionResult, ContextraError> {
        let span = info_span!(
            "orchestration.execute_ingestion",
            source_path = %document.path.display()
        );

        async move { self.ingestion.ingest_document(document).await }
            .instrument(span)
            .await
    }

    fn context_request(
        &self,
        conversation_id: ConversationId,
        user_message: &str,
    ) -> ContextRequest {
        let retrieval = RetrievalRequest {
            query: user_message.to_string(),
            collection: self.config.collection.clone(),
            mode: RetrievalMode::Hybrid,
            limit: self.config.context_limit,
            filter: self.config.retrieval_filter.clone(),
        };

        ContextRequest::new(
            user_message,
            self.config.user_id,
            conversation_id,
            self.config.collection.clone(),
        )
        .with_retrieval(retrieval)
        .with_context_limit(self.config.context_limit)
        .with_memory_limit(self.config.memory_limit)
        .with_token_budget(self.config.token_budget)
        .with_prompt(self.config.model.clone(), self.config.system_prompt.clone())
    }
}

fn workflow_metadata(stage: &str) -> Metadata {
    let mut metadata = Metadata::new();
    metadata.insert("workflow".to_string(), json!("orchestration"));
    metadata.insert("stage".to_string(), json!(stage));
    metadata
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use context::ContextPackage;
    use futures_util::stream;
    use ingestion::IngestionResult;
    use providers::{ChatRequest, ChatRole, ChatStream, ProviderError};
    use std::sync::{Arc, Mutex};
    use types::{CollectionId, Document, DocumentId};
    use uuid::Uuid;

    #[tokio::test]
    async fn execute_chat_assembles_context_calls_provider_and_updates_memory()
    -> Result<(), Box<dyn std::error::Error>> {
        let user_id = UserId::new();
        let conversation_id = ConversationId::new();
        let context =
            MockContextWorkflow::new(context_package(user_id, conversation_id, "context model"));
        let provider = MockProvider::new("assistant answer");
        let memory = MockMemoryUpdater::default();
        let ingestion = MockIngestionWorkflow::default();
        let orchestrator = Orchestrator::new(
            context.clone(),
            provider.clone(),
            memory.clone(),
            ingestion,
            OrchestratorConfig::new(user_id)
                .with_collection("docs")
                .with_prompt("mock-model", "System base")
                .with_limits(5, 3, 128),
        );

        let response = orchestrator
            .execute_chat(conversation_id, "What did we decide?")
            .await?;

        assert_eq!(
            response.message.content.as_deref(),
            Some("assistant answer")
        );

        let context_requests = context.requests.lock().unwrap();
        assert_eq!(context_requests.len(), 1);
        assert_eq!(context_requests[0].query, "What did we decide?");
        assert_eq!(context_requests[0].retrieval.collection, "docs");
        assert_eq!(context_requests[0].context_limit, 5);

        let provider_requests = provider.requests.lock().unwrap();
        assert_eq!(provider_requests.len(), 1);
        let sent = &provider_requests[0];
        assert_eq!(sent.model, "context model");
        assert_eq!(sent.messages.last().unwrap().role, ChatRole::User);
        assert_eq!(
            sent.messages.last().unwrap().content.as_deref(),
            Some("What did we decide?")
        );

        let writes = memory.writes.lock().unwrap();
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].role, Role::User);
        assert_eq!(writes[0].content, "What did we decide?");
        assert_eq!(writes[1].role, Role::Assistant);
        assert_eq!(writes[1].content, "assistant answer");
        Ok(())
    }

    #[tokio::test]
    async fn execute_chat_propagates_provider_errors_and_skips_memory_update()
    -> Result<(), Box<dyn std::error::Error>> {
        let user_id = UserId::new();
        let conversation_id = ConversationId::new();
        let context = MockContextWorkflow::new(context_package(user_id, conversation_id, "model"));
        let provider = MockProvider::failing(ProviderError::InvalidRequest("bad".to_string()));
        let memory = MockMemoryUpdater::default();
        let ingestion = MockIngestionWorkflow::default();
        let orchestrator = Orchestrator::new(
            context,
            provider,
            memory.clone(),
            ingestion,
            OrchestratorConfig::new(user_id),
        );

        let error = orchestrator
            .execute_chat(conversation_id, "hello")
            .await
            .unwrap_err();

        assert!(matches!(error, ContextraError::Validation(_)));
        assert!(memory.writes.lock().unwrap().is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn execute_ingestion_delegates_to_pipeline_and_returns_result()
    -> Result<(), Box<dyn std::error::Error>> {
        let user_id = UserId::new();
        let conversation_id = ConversationId::new();
        let expected = ingestion_result("parsed content");
        let ingestion = MockIngestionWorkflow::new(expected.clone());
        let orchestrator = Orchestrator::new(
            MockContextWorkflow::new(context_package(user_id, conversation_id, "model")),
            MockProvider::new("unused"),
            MockMemoryUpdater::default(),
            ingestion.clone(),
            OrchestratorConfig::new(user_id),
        );

        let result = orchestrator
            .execute_ingestion(IngestionDocument::path("/tmp/source.md"))
            .await?;

        assert_eq!(result, expected);
        assert_eq!(
            ingestion.documents.lock().unwrap().as_slice(),
            &[IngestionDocument::path("/tmp/source.md")]
        );
        Ok(())
    }

    #[derive(Debug, Clone)]
    struct MockContextWorkflow {
        package: ContextPackage,
        requests: Arc<Mutex<Vec<ContextRequest>>>,
    }

    impl MockContextWorkflow {
        fn new(package: ContextPackage) -> Self {
            Self {
                package,
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl ContextWorkflow for MockContextWorkflow {
        async fn assemble_context(
            &self,
            request: ContextRequest,
        ) -> Result<ContextPackage, ContextraError> {
            self.requests.lock().unwrap().push(request);
            Ok(self.package.clone())
        }
    }

    #[derive(Debug, Clone)]
    struct MockProvider {
        response: MockProviderResponse,
        requests: Arc<Mutex<Vec<ChatRequest>>>,
    }

    #[derive(Debug, Clone)]
    enum MockProviderResponse {
        Ok(String),
        InvalidRequest(String),
    }

    impl MockProvider {
        fn new(content: impl Into<String>) -> Self {
            Self {
                response: MockProviderResponse::Ok(content.into()),
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn failing(error: ProviderError) -> Self {
            let response = match error {
                ProviderError::InvalidRequest(message) => {
                    MockProviderResponse::InvalidRequest(message)
                }
                other => MockProviderResponse::InvalidRequest(other.to_string()),
            };

            Self {
                response,
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl LLMProvider for MockProvider {
        async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
            self.requests.lock().unwrap().push(request.clone());
            match &self.response {
                MockProviderResponse::Ok(content) => Ok(ChatResponse {
                    id: "response-id".to_string(),
                    model: request.model,
                    message: ChatMessage::assistant(content.clone()),
                    finish_reason: Some("stop".to_string()),
                    usage: None,
                }),
                MockProviderResponse::InvalidRequest(message) => {
                    Err(ProviderError::InvalidRequest(message.clone()))
                }
            }
        }

        async fn chat_stream(&self, _request: ChatRequest) -> Result<ChatStream, ProviderError> {
            Ok(Box::pin(stream::empty()))
        }

        fn supports_function_calling(&self) -> bool {
            false
        }
    }

    #[derive(Debug, Clone, Default)]
    struct MockMemoryUpdater {
        writes: Arc<Mutex<Vec<MemoryWrite>>>,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct MemoryWrite {
        conversation_id: ConversationId,
        role: Role,
        content: String,
        metadata: Metadata,
    }

    #[async_trait]
    impl ChatMemoryUpdater for MockMemoryUpdater {
        async fn append_message(
            &self,
            conversation_id: ConversationId,
            role: Role,
            content: String,
            metadata: Metadata,
        ) -> Result<Message, ContextraError> {
            self.writes.lock().unwrap().push(MemoryWrite {
                conversation_id,
                role: role.clone(),
                content: content.clone(),
                metadata: metadata.clone(),
            });
            Ok(Message {
                id: Uuid::now_v7(),
                conversation_id,
                role,
                content,
                metadata,
            })
        }
    }

    #[derive(Debug, Clone, Default)]
    struct MockIngestionWorkflow {
        result: Option<IngestionResult>,
        documents: Arc<Mutex<Vec<IngestionDocument>>>,
    }

    impl MockIngestionWorkflow {
        fn new(result: IngestionResult) -> Self {
            Self {
                result: Some(result),
                documents: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait(?Send)]
    impl IngestionWorkflow for MockIngestionWorkflow {
        async fn ingest_document(
            &self,
            document: IngestionDocument,
        ) -> Result<IngestionResult, ContextraError> {
            self.documents.lock().unwrap().push(document);
            self.result
                .clone()
                .ok_or_else(|| ContextraError::Internal("missing mock result".to_string()))
        }
    }

    fn context_package(
        user_id: UserId,
        conversation_id: ConversationId,
        model: &str,
    ) -> ContextPackage {
        let chat_request = ChatRequest::new(
            model.to_string(),
            vec![
                ChatMessage::system("System base"),
                ChatMessage::system("Retrieved context:\n1. context"),
            ],
        );

        ContextPackage {
            query: "query".to_string(),
            user_id,
            conversation_id,
            ranked_items: Vec::new(),
            retrieved_context: Vec::new(),
            memories: Vec::new(),
            conversation_history: Vec::new(),
            optimized: prompts::OptimizedPromptContext::default(),
            chat_request,
        }
    }

    fn ingestion_result(content: &str) -> IngestionResult {
        let document = Document {
            id: DocumentId::new(),
            collection_id: CollectionId::new(),
            content: content.to_string(),
            metadata: Metadata::new(),
        };
        let chunk = types::Chunk {
            id: Uuid::now_v7(),
            document_id: document.id,
            content: content.to_string(),
            metadata: Metadata::new(),
        };

        IngestionResult {
            document,
            chunks: vec![chunk],
        }
    }
}
