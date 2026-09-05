use async_trait::async_trait;
use embeddings::{Embedding, EmbeddingError, EmbeddingProvider};
use evaluation::{BenchmarkDataset, EvaluationPipeline};
use ingestion::{FixedSizeChunker, IngestionPipeline, MarkdownParser, PlainTextParser};
use providers::{ChatMessage, ChatRequest, ChatResponse, ChatStream, LLMProvider, ProviderError};
use std::path::Path;
use storage::vector_store::InMemoryVectorStore;
use types::CollectionId;

#[derive(Debug, Clone, Copy)]
pub struct CliEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for CliEmbeddingProvider {
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
        Ok(inputs.iter().map(|_| vec![0.1; 1536]).collect())
    }

    fn dimensions(&self) -> usize {
        1536
    }

    fn model_name(&self) -> &str {
        "cli-mock-embeddings"
    }
}

#[derive(Debug, Clone)]
pub struct CliLLMProvider;

#[async_trait]
impl LLMProvider for CliLLMProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let last_msg = request
            .messages
            .last()
            .and_then(|m| m.content.clone())
            .unwrap_or_default();

        Ok(ChatResponse {
            id: "cli-chat-res".to_string(),
            model: request.model,
            message: ChatMessage::assistant(format!(
                "Contextra Local Engine response for: '{last_msg}'"
            )),
            finish_reason: Some("stop".to_string()),
            usage: None,
        })
    }

    async fn chat_stream(&self, _request: ChatRequest) -> Result<ChatStream, ProviderError> {
        Err(ProviderError::InvalidRequest(
            "streaming not implemented in CLI local mode".into(),
        ))
    }

    fn supports_function_calling(&self) -> bool {
        false
    }
}

pub struct LocalEngine {
    vector_store: InMemoryVectorStore,
}

impl Default for LocalEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalEngine {
    pub fn new() -> Self {
        Self {
            vector_store: InMemoryVectorStore::new(),
        }
    }

    pub async fn list_collections(&self) -> Result<Vec<(String, String)>, String> {
        let collections = vec![
            (
                "default-collection".to_string(),
                "Default Collection".to_string(),
            ),
            ("sample-docs".to_string(), "Sample Documents".to_string()),
        ];
        Ok(collections)
    }

    pub async fn ingest(&self, path_str: &str) -> Result<String, String> {
        let path = Path::new(path_str);
        if !path.exists() {
            return Err(format!("File or directory path does not exist: {path_str}"));
        }

        let provider = CliEmbeddingProvider;
        let collection_id = CollectionId::new();

        let result = if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let pipeline = IngestionPipeline::new(
                MarkdownParser,
                FixedSizeChunker::new(512, 64).map_err(|e| e.to_string())?,
                provider,
                self.vector_store.clone(),
                "default-collection",
                collection_id,
            );
            pipeline
                .ingest_path(path)
                .await
                .map_err(|e| e.to_string())?
        } else {
            let pipeline = IngestionPipeline::new(
                PlainTextParser,
                FixedSizeChunker::new(512, 64).map_err(|e| e.to_string())?,
                provider,
                self.vector_store.clone(),
                "default-collection",
                collection_id,
            );
            pipeline
                .ingest_path(path)
                .await
                .map_err(|e| e.to_string())?
        };

        Ok(format!(
            "Document ID: {}\nCollection ID: {}\nChunks ingested: {}",
            result.document.id,
            result.document.collection_id,
            result.chunks.len()
        ))
    }

    pub async fn chat(&self, user_message: &str) -> Result<String, String> {
        let provider = CliLLMProvider;
        let request = ChatRequest::new("cli-mock-model", vec![ChatMessage::user(user_message)]);

        let response = provider
            .chat(request)
            .await
            .map_err(|e| format!("Local chat execution failed: {e}"))?;

        Ok(response.message.content.unwrap_or_default())
    }

    pub async fn run_eval(&self, dataset_path: Option<&str>, k: usize) -> Result<String, String> {
        let dataset_json = if let Some(path) = dataset_path {
            std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read dataset file '{path}': {e}"))?
        } else {
            r#"{
                "retrieval": [
                    {
                        "query": "What is Contextra?",
                        "expected_relevant_chunk_ids": ["doc-1#0"],
                        "retrieved_chunk_ids": ["doc-1#0", "doc-2#1"]
                    }
                ],
                "generation": [
                    {
                        "query": "What is Contextra?",
                        "answer": "Contextra is a context engineering platform.",
                        "reference_answer": "Contextra is a context engineering platform for AI applications.",
                        "min_words": 3,
                        "max_words": 20
                    }
                ]
            }"#.to_string()
        };

        let dataset = BenchmarkDataset::from_json_str(&dataset_json)
            .map_err(|e| format!("Failed to parse benchmark dataset: {e}"))?;

        let judge = CliLLMProvider;
        let pipeline = EvaluationPipeline::new(judge);
        let report = pipeline
            .evaluate(&dataset, k)
            .await
            .map_err(|e| format!("Evaluation failed: {e}"))?;

        let output = format!(
            "--- Evaluation Report ---\n\
             Retrieval Precision@{k}: {:.4}\n\
             Retrieval Recall@{k}:    {:.4}\n\
             Retrieval MRR:           {:.4}\n\
             Generation Overall Score: {:.4}\n\
             Retrieval Cases: {}\n\
             Generation Cases: {}",
            report.retrieval_summary.precision_at_k,
            report.retrieval_summary.recall_at_k,
            report.retrieval_summary.mrr,
            report.generation_summary.overall_score,
            report.retrieval_cases.len(),
            report.generation_cases.len()
        );

        Ok(output)
    }
}
