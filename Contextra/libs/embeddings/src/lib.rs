use async_trait::async_trait;
use errors::ContextraError;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use storage::cache::Cache;
use thiserror::Error;

const OPENAI_BASE_URL: &str = "https://api.openai.com";
const OPENAI_EMBEDDINGS_PATH: &str = "/v1/embeddings";
const OLLAMA_BASE_URL: &str = "http://localhost:11434";
const OLLAMA_EMBEDDINGS_PATH: &str = "/api/embed";
const OPENAI_SAFE_BATCH_SIZE: usize = 2048;
const OLLAMA_SAFE_BATCH_SIZE: usize = 96;
const DEFAULT_MAX_CONCURRENT_BATCHES: usize = 4;
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 30);

pub type Embedding = Vec<f32>;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError>;

    fn dimensions(&self) -> usize;

    fn model_name(&self) -> &str;
}

#[derive(Debug, Error)]
pub enum EmbeddingError {
    #[error("invalid embedding request: {0}")]
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

    #[error("embedding cache error: {0}")]
    Cache(String),
}

impl From<EmbeddingError> for ContextraError {
    fn from(error: EmbeddingError) -> Self {
        match error {
            EmbeddingError::InvalidRequest(message) => Self::Validation(message),
            EmbeddingError::Authentication(message) => Self::Unauthorized(message),
            EmbeddingError::RateLimited(message) => Self::RateLimited(message),
            EmbeddingError::HttpStatus { status, body } if status == StatusCode::UNAUTHORIZED => {
                Self::Unauthorized(body)
            }
            EmbeddingError::HttpStatus { status, body } if status == StatusCode::FORBIDDEN => {
                Self::Forbidden(body)
            }
            EmbeddingError::HttpStatus { status, body }
                if status == StatusCode::TOO_MANY_REQUESTS =>
            {
                Self::RateLimited(body)
            }
            EmbeddingError::Cache(message) => Self::StorageError(message),
            other => Self::ProviderError(other.to_string()),
        }
    }
}

impl From<reqwest::Error> for EmbeddingError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout(error.to_string())
        } else if error.is_decode() {
            Self::Decode(error.to_string())
        } else {
            Self::Network(error.to_string())
        }
    }
}

impl From<serde_json::Error> for EmbeddingError {
    fn from(error: serde_json::Error) -> Self {
        Self::Decode(error.to_string())
    }
}

impl From<ContextraError> for EmbeddingError {
    fn from(error: ContextraError) -> Self {
        Self::Cache(error.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct OpenAIEmbeddingProvider {
    api_key: String,
    model: String,
    dimensions: usize,
    base_url: String,
    client: Client,
}

impl OpenAIEmbeddingProvider {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>, dimensions: usize) -> Self {
        Self::with_client(api_key, model, dimensions, Client::new())
    }

    pub fn with_base_url(
        api_key: impl Into<String>,
        model: impl Into<String>,
        dimensions: usize,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            ..Self::with_client(api_key, model, dimensions, Client::new())
        }
    }

    fn with_client(
        api_key: impl Into<String>,
        model: impl Into<String>,
        dimensions: usize,
        client: Client,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            dimensions,
            base_url: OPENAI_BASE_URL.to_string(),
            client,
        }
    }

    pub async fn embed_all(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
        embed_batched(
            self,
            inputs,
            OPENAI_SAFE_BATCH_SIZE,
            DEFAULT_MAX_CONCURRENT_BATCHES,
        )
        .await
    }

    fn embeddings_url(&self) -> String {
        format!("{}{}", self.base_url, OPENAI_EMBEDDINGS_PATH)
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbeddingProvider {
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
        validate_inputs(inputs)?;

        let response = self
            .client
            .post(self.embeddings_url())
            .bearer_auth(&self.api_key)
            .json(&OpenAIEmbeddingRequest {
                model: &self.model,
                input: inputs,
            })
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|error| format!("failed to read error body: {error}"));
            return Err(map_status_error(status, body));
        }

        let body = response.json::<OpenAIEmbeddingResponse>().await?;
        body.into_embeddings()
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

#[derive(Debug, Serialize)]
struct OpenAIEmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingResponse {
    data: Vec<OpenAIEmbeddingData>,
}

impl OpenAIEmbeddingResponse {
    fn into_embeddings(mut self) -> Result<Vec<Embedding>, EmbeddingError> {
        self.data.sort_by_key(|item| item.index);
        Ok(self.data.into_iter().map(|item| item.embedding).collect())
    }
}

#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingData {
    index: usize,
    embedding: Embedding,
}

#[derive(Debug, Clone)]
pub struct OllamaEmbeddingProvider {
    model: String,
    dimensions: usize,
    base_url: String,
    client: Client,
}

impl OllamaEmbeddingProvider {
    pub fn new(model: impl Into<String>, dimensions: usize) -> Self {
        Self::with_client(model, dimensions, Client::new())
    }

    pub fn with_base_url(
        model: impl Into<String>,
        dimensions: usize,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            ..Self::with_client(model, dimensions, Client::new())
        }
    }

    fn with_client(model: impl Into<String>, dimensions: usize, client: Client) -> Self {
        Self {
            model: model.into(),
            dimensions,
            base_url: OLLAMA_BASE_URL.to_string(),
            client,
        }
    }

    pub async fn embed_all(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
        embed_batched(
            self,
            inputs,
            OLLAMA_SAFE_BATCH_SIZE,
            DEFAULT_MAX_CONCURRENT_BATCHES,
        )
        .await
    }

    fn embeddings_url(&self) -> String {
        format!("{}{}", self.base_url, OLLAMA_EMBEDDINGS_PATH)
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
        validate_inputs(inputs)?;

        let response = self
            .client
            .post(self.embeddings_url())
            .json(&OllamaEmbeddingRequest {
                model: &self.model,
                input: inputs,
            })
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|error| format!("failed to read error body: {error}"));
            return Err(map_status_error(status, body));
        }

        let body = response.json::<OllamaEmbeddingResponse>().await?;
        Ok(body.embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

#[derive(Debug, Serialize)]
struct OllamaEmbeddingRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embeddings: Vec<Embedding>,
}

#[derive(Debug, Clone)]
pub struct EmbeddingCache<C> {
    cache: C,
    ttl: Duration,
    namespace: String,
}

impl<C> EmbeddingCache<C>
where
    C: Cache,
{
    pub fn new(cache: C) -> Self {
        Self {
            cache,
            ttl: DEFAULT_CACHE_TTL,
            namespace: "contextra:embeddings".to_string(),
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    pub fn key(&self, model: &str, input: &str) -> String {
        embedding_cache_key(&self.namespace, model, input)
    }

    pub async fn get(&self, model: &str, input: &str) -> Result<Option<Embedding>, EmbeddingError> {
        Ok(self.cache.get(&self.key(model, input)).await?)
    }

    pub async fn set(
        &self,
        model: &str,
        input: &str,
        embedding: &Embedding,
    ) -> Result<(), EmbeddingError> {
        Ok(self
            .cache
            .set_with_ttl(&self.key(model, input), embedding, self.ttl)
            .await?)
    }
}

#[derive(Debug, Clone)]
pub struct CachedEmbeddingProvider<P, C> {
    provider: P,
    cache: EmbeddingCache<C>,
}

impl<P, C> CachedEmbeddingProvider<P, C>
where
    P: EmbeddingProvider,
    C: Cache,
{
    pub fn new(provider: P, cache: EmbeddingCache<C>) -> Self {
        Self { provider, cache }
    }
}

#[async_trait]
impl<P, C> EmbeddingProvider for CachedEmbeddingProvider<P, C>
where
    P: EmbeddingProvider,
    C: Cache,
{
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
        validate_inputs(inputs)?;

        let model = self.provider.model_name();
        let mut results = vec![None; inputs.len()];
        let mut miss_positions: HashMap<String, Vec<usize>> = HashMap::new();
        let mut miss_inputs = Vec::new();

        for (index, input) in inputs.iter().enumerate() {
            if let Some(embedding) = self.cache.get(model, input).await? {
                results[index] = Some(embedding);
            } else {
                if let Some(positions) = miss_positions.get_mut(input) {
                    positions.push(index);
                } else {
                    miss_positions.insert(input.clone(), vec![index]);
                    miss_inputs.push(input.clone());
                }
            }
        }

        if !miss_inputs.is_empty() {
            let embeddings = self.provider.embed_batch(&miss_inputs).await?;
            validate_embedding_count(miss_inputs.len(), embeddings.len())?;

            for (input, embedding) in miss_inputs.iter().zip(embeddings) {
                self.cache.set(model, input, &embedding).await?;
                if let Some(positions) = miss_positions.get(input) {
                    for position in positions {
                        results[*position] = Some(embedding.clone());
                    }
                }
            }
        }

        results
            .into_iter()
            .enumerate()
            .map(|(index, embedding)| {
                embedding.ok_or_else(|| {
                    EmbeddingError::Cache(format!("missing embedding result at index {index}"))
                })
            })
            .collect()
    }

    fn dimensions(&self) -> usize {
        self.provider.dimensions()
    }

    fn model_name(&self) -> &str {
        self.provider.model_name()
    }
}

pub async fn embed_batched<P>(
    provider: &P,
    inputs: &[String],
    max_batch_size: usize,
    _max_concurrent_batches: usize,
) -> Result<Vec<Embedding>, EmbeddingError>
where
    P: EmbeddingProvider + ?Sized,
{
    validate_inputs(inputs)?;
    if max_batch_size == 0 {
        return Err(EmbeddingError::InvalidRequest(
            "max_batch_size must be greater than zero".to_string(),
        ));
    }

    let mut chunk_results = Vec::new();
    for (index, chunk) in inputs.chunks(max_batch_size).enumerate() {
        let embeddings = provider.embed_batch(chunk).await?;
        validate_embedding_count(chunk.len(), embeddings.len())?;
        chunk_results.push((index, embeddings));
    }

    chunk_results.sort_by_key(|(index, _)| *index);

    Ok(chunk_results
        .into_iter()
        .flat_map(|(_, embeddings)| embeddings)
        .collect())
}

pub fn embedding_cache_key(namespace: &str, model: &str, input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model.as_bytes());
    hasher.update([0]);
    hasher.update(input.as_bytes());
    format!(
        "{}:{}",
        namespace.trim_end_matches(':'),
        hex::encode(hasher.finalize())
    )
}

fn validate_inputs(inputs: &[String]) -> Result<(), EmbeddingError> {
    if inputs.is_empty() {
        Err(EmbeddingError::InvalidRequest(
            "embedding input batch cannot be empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn validate_embedding_count(expected: usize, actual: usize) -> Result<(), EmbeddingError> {
    if expected == actual {
        Ok(())
    } else {
        Err(EmbeddingError::Decode(format!(
            "provider returned {actual} embeddings for {expected} inputs"
        )))
    }
}

fn map_status_error(status: StatusCode, body: String) -> EmbeddingError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => EmbeddingError::Authentication(body),
        StatusCode::TOO_MANY_REQUESTS => EmbeddingError::RateLimited(body),
        _ => EmbeddingError::HttpStatus { status, body },
    }
}

#[async_trait]
impl<T: EmbeddingProvider + ?Sized + Send + Sync> EmbeddingProvider for Arc<T> {
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
        self.as_ref().embed_batch(inputs).await
    }

    fn dimensions(&self) -> usize {
        self.as_ref().dimensions()
    }

    fn model_name(&self) -> &str {
        self.as_ref().model_name()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex;

    #[derive(Clone, Default)]
    struct MemoryCache {
        values: Arc<Mutex<HashMap<String, String>>>,
    }

    #[async_trait]
    impl Cache for MemoryCache {
        async fn get<T>(&self, key: &str) -> Result<Option<T>, ContextraError>
        where
            T: DeserializeOwned + Send,
        {
            let values = self.values.lock().await;
            values
                .get(key)
                .map(|value| serde_json::from_str(value))
                .transpose()
                .map_err(|error| ContextraError::StorageError(error.to_string()))
        }

        async fn set_with_ttl<T>(
            &self,
            key: &str,
            value: &T,
            _ttl: Duration,
        ) -> Result<(), ContextraError>
        where
            T: Serialize + Send + Sync,
        {
            let mut values = self.values.lock().await;
            values.insert(
                key.to_string(),
                serde_json::to_string(value)
                    .map_err(|error| ContextraError::StorageError(error.to_string()))?,
            );
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<(), ContextraError> {
            self.values.lock().await.remove(key);
            Ok(())
        }

        async fn exists(&self, key: &str) -> Result<bool, ContextraError> {
            Ok(self.values.lock().await.contains_key(key))
        }
    }

    #[derive(Clone)]
    struct MockProvider {
        model: String,
        dimensions: usize,
        calls: Arc<AtomicUsize>,
        batches: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                model: "mock-embedding".to_string(),
                dimensions: 3,
                calls: Arc::new(AtomicUsize::new(0)),
                batches: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        async fn batches(&self) -> Vec<Vec<String>> {
            self.batches.lock().await.clone()
        }
    }

    #[async_trait]
    impl EmbeddingProvider for MockProvider {
        async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.batches.lock().await.push(inputs.to_vec());
            Ok(inputs.iter().map(|input| embedding_for(input)).collect())
        }

        fn dimensions(&self) -> usize {
            self.dimensions
        }

        fn model_name(&self) -> &str {
            &self.model
        }
    }

    #[tokio::test]
    async fn embedding_cache_hits_avoid_provider_calls() -> Result<(), Box<dyn std::error::Error>> {
        let cache = EmbeddingCache::new(MemoryCache::default());
        let provider = MockProvider::new();
        let cached_provider = CachedEmbeddingProvider::new(provider.clone(), cache);
        let inputs = vec!["alpha".to_string(), "beta".to_string()];

        let first = cached_provider.embed_batch(&inputs).await?;
        let second = cached_provider.embed_batch(&inputs).await?;

        assert_eq!(first, second);
        assert_eq!(provider.calls(), 1);
        assert_eq!(provider.batches().await, vec![inputs]);

        Ok(())
    }

    #[tokio::test]
    async fn embedding_cache_misses_only_request_uncached_inputs()
    -> Result<(), Box<dyn std::error::Error>> {
        let cache = EmbeddingCache::new(MemoryCache::default());
        cache
            .set("mock-embedding", "cached", &embedding_for("cached"))
            .await?;
        let provider = MockProvider::new();
        let cached_provider = CachedEmbeddingProvider::new(provider.clone(), cache);

        let inputs = vec![
            "cached".to_string(),
            "fresh".to_string(),
            "cached".to_string(),
        ];
        let embeddings = cached_provider.embed_batch(&inputs).await?;

        assert_eq!(
            embeddings,
            vec![
                embedding_for("cached"),
                embedding_for("fresh"),
                embedding_for("cached")
            ]
        );
        assert_eq!(provider.calls(), 1);
        assert_eq!(provider.batches().await, vec![vec!["fresh".to_string()]]);

        Ok(())
    }

    #[tokio::test]
    async fn embedding_cache_deduplicates_uncached_inputs_in_same_batch()
    -> Result<(), Box<dyn std::error::Error>> {
        let cache = EmbeddingCache::new(MemoryCache::default());
        let provider = MockProvider::new();
        let cached_provider = CachedEmbeddingProvider::new(provider.clone(), cache);

        let inputs = vec![
            "fresh".to_string(),
            "fresh".to_string(),
            "other".to_string(),
        ];
        let embeddings = cached_provider.embed_batch(&inputs).await?;

        assert_eq!(
            embeddings,
            vec![
                embedding_for("fresh"),
                embedding_for("fresh"),
                embedding_for("other")
            ]
        );
        assert_eq!(provider.calls(), 1);
        assert_eq!(
            provider.batches().await,
            vec![vec!["fresh".to_string(), "other".to_string()]]
        );

        Ok(())
    }

    #[tokio::test]
    async fn batching_chunks_inputs_and_preserves_output_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let provider = MockProvider::new();
        let inputs = ["a", "b", "c", "d", "e"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();

        let embeddings = embed_batched(&provider, &inputs, 2, 2).await?;

        assert_eq!(
            embeddings,
            inputs
                .iter()
                .map(|input| embedding_for(input))
                .collect::<Vec<_>>()
        );
        assert_eq!(provider.calls(), 3);
        assert_eq!(
            provider.batches().await,
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()],
                vec!["e".to_string()]
            ]
        );

        Ok(())
    }

    fn embedding_for(input: &str) -> Embedding {
        vec![
            input.len() as f32,
            input.bytes().next().unwrap_or_default() as f32,
            1.0,
        ]
    }
}
