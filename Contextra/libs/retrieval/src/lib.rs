use async_trait::async_trait;
use embeddings::EmbeddingProvider;
use errors::ContextraError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use storage::vector_store::{SearchResult as VectorSearchResult, VectorStore};
use types::Metadata;
use uuid::Uuid;

const DEFAULT_RESULT_LIMIT: usize = 10;
const DEFAULT_CANDIDATE_MULTIPLIER: usize = 8;
const DEFAULT_CANDIDATE_FLOOR: usize = 64;
const KEYWORD_K1: f32 = 1.5;
const KEYWORD_B: f32 = 0.75;

const DEFAULT_TEXT_FIELDS: &[&str] = &[
    "content", "text", "title", "summary", "heading", "filename", "source",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryIntent {
    Search,
    Question,
    Summary,
    Comparison,
    Code,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QueryEntityKind {
    Email,
    Hashtag,
    QuotedPhrase,
    FileType,
    Identifier,
    ProperNoun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryEntity {
    pub kind: QueryEntityKind,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzedQuery {
    pub original: String,
    pub normalized: String,
    pub intent: QueryIntent,
    pub entities: Vec<QueryEntity>,
    pub selected_collections: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRule {
    collection: String,
    terms: Vec<String>,
}

impl CollectionRule {
    pub fn new(
        collection: impl Into<String>,
        terms: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            collection: collection.into(),
            terms: terms
                .into_iter()
                .map(|term| normalize_query_text(&term.into()))
                .filter(|term| !term.is_empty())
                .collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryAnalyzer {
    default_collections: Vec<String>,
    collection_rules: Vec<CollectionRule>,
}

impl QueryAnalyzer {
    pub fn new(default_collections: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            default_collections: default_collections.into_iter().map(Into::into).collect(),
            collection_rules: Vec::new(),
        }
    }

    pub fn with_collection_rule(mut self, rule: CollectionRule) -> Self {
        self.collection_rules.push(rule);
        self
    }

    pub fn analyze(&self, query: &str) -> AnalyzedQuery {
        let normalized = normalize_query_text(query);
        let intent = detect_intent(query, &normalized);
        let entities = extract_entities(query);
        let selected_collections = self.select_collections(&normalized, &entities);

        AnalyzedQuery {
            original: query.to_string(),
            normalized,
            intent,
            entities,
            selected_collections,
        }
    }

    fn select_collections(&self, normalized: &str, entities: &[QueryEntity]) -> Vec<String> {
        let mut selected = Vec::new();
        let entity_terms: HashSet<&str> = entities
            .iter()
            .map(|entity| entity.value.as_str())
            .collect();

        for rule in &self.collection_rules {
            let matches_rule = rule
                .terms
                .iter()
                .any(|term| normalized.contains(term) || entity_terms.contains(term.as_str()));

            if matches_rule && !selected.contains(&rule.collection) {
                selected.push(rule.collection.clone());
            }
        }

        if selected.is_empty() {
            self.default_collections.clone()
        } else {
            selected
        }
    }
}

impl Default for QueryAnalyzer {
    fn default() -> Self {
        Self::new(["default"])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpandedQuery {
    pub original: String,
    pub terms: Vec<String>,
}

impl ExpandedQuery {
    pub fn as_search_text(&self) -> String {
        self.terms.join(" ")
    }
}

pub trait TermExpansionProvider: Send + Sync {
    fn expand_term(&self, term: &str) -> Vec<String>;
}

#[derive(Debug, Clone, Default)]
pub struct StaticTermExpansionProvider {
    terms: HashMap<String, Vec<String>>,
}

impl StaticTermExpansionProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_terms(
        mut self,
        term: impl Into<String>,
        expansions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.terms.insert(
            normalize_query_text(&term.into()),
            expansions
                .into_iter()
                .map(|expansion| normalize_query_text(&expansion.into()))
                .filter(|expansion| !expansion.is_empty())
                .collect(),
        );
        self
    }
}

impl TermExpansionProvider for StaticTermExpansionProvider {
    fn expand_term(&self, term: &str) -> Vec<String> {
        self.terms
            .get(term)
            .cloned()
            .unwrap_or_else(|| default_expansions(term))
    }
}

#[derive(Debug, Clone)]
pub struct QueryExpander<P = StaticTermExpansionProvider> {
    provider: P,
    max_terms: usize,
}

impl QueryExpander<StaticTermExpansionProvider> {
    pub fn new() -> Self {
        Self {
            provider: StaticTermExpansionProvider::new(),
            max_terms: 32,
        }
    }

    pub fn with_static_terms(
        mut self,
        term: impl Into<String>,
        expansions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.provider = self.provider.with_terms(term, expansions);
        self
    }
}

impl Default for QueryExpander<StaticTermExpansionProvider> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> QueryExpander<P>
where
    P: TermExpansionProvider,
{
    pub fn with_provider(provider: P) -> Self {
        Self {
            provider,
            max_terms: 32,
        }
    }

    pub fn with_max_terms(mut self, max_terms: usize) -> Self {
        self.max_terms = max_terms.max(1);
        self
    }

    pub fn expand(&self, analyzed: &AnalyzedQuery) -> ExpandedQuery {
        let mut seen = HashSet::new();
        let mut terms = Vec::new();

        for token in tokenize(&analyzed.normalized) {
            self.push_term(&mut terms, &mut seen, &token);
            if terms.len() >= self.max_terms {
                break;
            }

            for expansion in self.provider.expand_term(&token) {
                self.push_term(&mut terms, &mut seen, &expansion);
                if terms.len() >= self.max_terms {
                    break;
                }
            }
        }

        for entity in &analyzed.entities {
            if terms.len() >= self.max_terms {
                break;
            }
            self.push_term(&mut terms, &mut seen, &normalize_query_text(&entity.value));
        }

        ExpandedQuery {
            original: analyzed.original.clone(),
            terms,
        }
    }

    fn push_term(&self, terms: &mut Vec<String>, seen: &mut HashSet<String>, term: &str) {
        let normalized = normalize_query_text(term);
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            terms.push(normalized);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RetrievalFilter {
    #[serde(default)]
    pub collection: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub file_type: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub metadata: Vec<MetadataCondition>,
}

impl RetrievalFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.collection.is_none()
            && self.user_id.is_none()
            && self.organization_id.is_none()
            && self.language.is_none()
            && self.file_type.is_none()
            && self.tags.is_empty()
            && self.permissions.is_empty()
            && self.metadata.is_empty()
    }

    pub fn matches(&self, payload: &Metadata) -> bool {
        self.matches_scalar(payload, "collection", self.collection.as_deref())
            && self.matches_scalar(payload, "user_id", self.user_id.as_deref())
            && self.matches_scalar(payload, "organization_id", self.organization_id.as_deref())
            && self.matches_scalar(payload, "language", self.language.as_deref())
            && self.matches_scalar(payload, "file_type", self.file_type.as_deref())
            && self.matches_all_values(payload, "tags", &self.tags)
            && self.matches_any_value(payload, "permissions", &self.permissions)
            && self
                .metadata
                .iter()
                .all(|condition| condition.matches(payload))
    }

    fn matches_scalar(&self, payload: &Metadata, key: &str, expected: Option<&str>) -> bool {
        match expected {
            Some(expected) => metadata_value_matches(payload.get(key), expected),
            None => true,
        }
    }

    fn matches_all_values(&self, payload: &Metadata, key: &str, expected: &[String]) -> bool {
        expected
            .iter()
            .all(|value| metadata_value_contains(payload.get(key), value))
    }

    fn matches_any_value(&self, payload: &Metadata, key: &str, expected: &[String]) -> bool {
        expected.is_empty()
            || expected
                .iter()
                .any(|value| metadata_value_contains(payload.get(key), value))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataCondition {
    pub field: String,
    pub operator: FilterOperator,
    pub value: Value,
}

impl MetadataCondition {
    pub fn equals(field: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            field: field.into(),
            operator: FilterOperator::Equals,
            value: value.into(),
        }
    }

    pub fn contains(field: impl Into<String>, value: impl Into<Value>) -> Self {
        Self {
            field: field.into(),
            operator: FilterOperator::Contains,
            value: value.into(),
        }
    }

    pub fn matches(&self, payload: &Metadata) -> bool {
        let Some(actual) = payload.get(&self.field) else {
            return false;
        };

        match self.operator {
            FilterOperator::Equals => values_equal(actual, &self.value),
            FilterOperator::NotEquals => !values_equal(actual, &self.value),
            FilterOperator::Contains => match self.value.as_str() {
                Some(expected) => metadata_value_contains(Some(actual), expected),
                None => false,
            },
            FilterOperator::GreaterThan => compare_numbers(actual, &self.value)
                .is_some_and(|ordering| ordering == Ordering::Greater),
            FilterOperator::GreaterThanOrEqual => compare_numbers(actual, &self.value)
                .is_some_and(|ordering| matches!(ordering, Ordering::Greater | Ordering::Equal)),
            FilterOperator::LessThan => compare_numbers(actual, &self.value)
                .is_some_and(|ordering| ordering == Ordering::Less),
            FilterOperator::LessThanOrEqual => compare_numbers(actual, &self.value)
                .is_some_and(|ordering| matches!(ordering, Ordering::Less | Ordering::Equal)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    Contains,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalMode {
    #[default]
    Semantic,
    Keyword,
    Metadata,
    Hybrid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalRequest {
    pub query: String,
    pub collection: String,
    #[serde(default)]
    pub mode: RetrievalMode,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub filter: RetrievalFilter,
}

impl RetrievalRequest {
    pub fn semantic(query: impl Into<String>, collection: impl Into<String>, limit: usize) -> Self {
        Self {
            query: query.into(),
            collection: collection.into(),
            mode: RetrievalMode::Semantic,
            limit,
            filter: RetrievalFilter::default(),
        }
    }

    pub fn keyword(query: impl Into<String>, collection: impl Into<String>, limit: usize) -> Self {
        Self {
            query: query.into(),
            collection: collection.into(),
            mode: RetrievalMode::Keyword,
            limit,
            filter: RetrievalFilter::default(),
        }
    }

    pub fn hybrid(query: impl Into<String>, collection: impl Into<String>, limit: usize) -> Self {
        Self {
            query: query.into(),
            collection: collection.into(),
            mode: RetrievalMode::Hybrid,
            limit,
            filter: RetrievalFilter::default(),
        }
    }

    pub fn metadata(query: impl Into<String>, collection: impl Into<String>, limit: usize) -> Self {
        Self {
            query: query.into(),
            collection: collection.into(),
            mode: RetrievalMode::Metadata,
            limit,
            filter: RetrievalFilter::default(),
        }
    }
}

fn default_limit() -> usize {
    DEFAULT_RESULT_LIMIT
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievedDocument {
    pub id: Uuid,
    pub score: f32,
    pub semantic_score: Option<f32>,
    pub keyword_score: Option<f32>,
    #[serde(default)]
    pub metadata_score: Option<f32>,
    #[serde(default)]
    pub fusion_score: Option<f32>,
    #[serde(default)]
    pub payload: Metadata,
}

#[async_trait]
pub trait Retriever: Send + Sync {
    async fn retrieve(
        &self,
        request: RetrievalRequest,
    ) -> Result<Vec<RetrievedDocument>, ContextraError>;
}

#[derive(Debug, Clone)]
pub struct HybridRetriever<R> {
    retriever: R,
    rrf_k: f32,
    candidate_multiplier: usize,
}

impl<R> HybridRetriever<R> {
    pub fn new(retriever: R) -> Self {
        Self {
            retriever,
            rrf_k: 60.0,
            candidate_multiplier: 4,
        }
    }

    pub fn with_rrf_k(mut self, rrf_k: f32) -> Self {
        self.rrf_k = rrf_k.max(1.0);
        self
    }

    pub fn with_candidate_multiplier(mut self, multiplier: usize) -> Self {
        self.candidate_multiplier = multiplier.max(1);
        self
    }

    fn candidate_limit(&self, request_limit: usize) -> usize {
        request_limit
            .max(1)
            .saturating_mul(self.candidate_multiplier)
    }
}

#[async_trait]
impl<R> Retriever for HybridRetriever<R>
where
    R: Retriever + Send + Sync,
{
    async fn retrieve(
        &self,
        request: RetrievalRequest,
    ) -> Result<Vec<RetrievedDocument>, ContextraError> {
        let limit = request.limit.max(1);
        let candidate_limit = self.candidate_limit(limit);

        let mut semantic_request = request.clone();
        semantic_request.mode = RetrievalMode::Semantic;
        semantic_request.limit = candidate_limit;

        let mut keyword_request = request.clone();
        keyword_request.mode = RetrievalMode::Keyword;
        keyword_request.limit = candidate_limit;

        let mut lists = vec![
            self.retriever.retrieve(semantic_request).await?,
            self.retriever.retrieve(keyword_request).await?,
        ];

        if !request.filter.is_empty() {
            let mut metadata_request = request;
            metadata_request.mode = RetrievalMode::Metadata;
            metadata_request.limit = candidate_limit;
            lists.push(self.retriever.retrieve(metadata_request).await?);
        }

        let mut fused = reciprocal_rank_fusion(lists, self.rrf_k);
        fused.truncate(limit);
        Ok(fused)
    }
}

#[derive(Debug, Clone)]
pub struct VectorRetriever<S, E> {
    store: S,
    embeddings: E,
    text_fields: Vec<String>,
    candidate_multiplier: usize,
    candidate_floor: usize,
    hybrid_semantic_weight: f32,
    hybrid_keyword_weight: f32,
}

impl<S, E> VectorRetriever<S, E> {
    pub fn new(store: S, embeddings: E) -> Self {
        Self {
            store,
            embeddings,
            text_fields: DEFAULT_TEXT_FIELDS
                .iter()
                .map(|field| (*field).to_string())
                .collect(),
            candidate_multiplier: DEFAULT_CANDIDATE_MULTIPLIER,
            candidate_floor: DEFAULT_CANDIDATE_FLOOR,
            hybrid_semantic_weight: 0.6,
            hybrid_keyword_weight: 0.4,
        }
    }

    pub fn with_text_fields(mut self, fields: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.text_fields = fields.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_candidate_pool(mut self, multiplier: usize, floor: usize) -> Self {
        self.candidate_multiplier = multiplier.max(1);
        self.candidate_floor = floor.max(1);
        self
    }

    pub fn with_hybrid_weights(mut self, semantic: f32, keyword: f32) -> Self {
        self.hybrid_semantic_weight = semantic.max(0.0);
        self.hybrid_keyword_weight = keyword.max(0.0);
        self
    }

    fn candidate_limit(&self, request_limit: usize) -> usize {
        request_limit
            .max(1)
            .saturating_mul(self.candidate_multiplier)
            .max(self.candidate_floor)
    }
}

#[async_trait]
impl<S, E> Retriever for VectorRetriever<S, E>
where
    S: VectorStore + Send + Sync,
    E: EmbeddingProvider + Send + Sync,
{
    async fn retrieve(
        &self,
        request: RetrievalRequest,
    ) -> Result<Vec<RetrievedDocument>, ContextraError> {
        if request.query.trim().is_empty() {
            return Err(ContextraError::Validation(
                "retrieval query cannot be empty".to_string(),
            ));
        }
        if request.collection.trim().is_empty() {
            return Err(ContextraError::Validation(
                "retrieval collection cannot be empty".to_string(),
            ));
        }

        let limit = request.limit.max(1);
        let query_embedding = self.embed_query(&request.query).await?;
        let vector_limit = match request.mode {
            RetrievalMode::Semantic => self.candidate_limit(limit),
            RetrievalMode::Keyword | RetrievalMode::Metadata | RetrievalMode::Hybrid => {
                self.candidate_limit(limit)
            }
        };
        let candidates = self
            .store
            .search(&request.collection, &query_embedding, vector_limit)
            .await?;
        let filtered = candidates
            .into_iter()
            .filter(|candidate| request.filter.matches(&candidate.payload))
            .collect::<Vec<_>>();

        let mut results = match request.mode {
            RetrievalMode::Semantic => semantic_results(filtered),
            RetrievalMode::Keyword => keyword_results(&request.query, filtered, &self.text_fields),
            RetrievalMode::Metadata => metadata_results(&request.filter, filtered),
            RetrievalMode::Hybrid => hybrid_results(
                &request.query,
                filtered,
                &self.text_fields,
                self.hybrid_semantic_weight,
                self.hybrid_keyword_weight,
            ),
        };

        sort_retrieved(&mut results);
        results.truncate(limit);
        Ok(results)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedChunk {
    pub id: Uuid,
    pub score: f32,
    pub content: String,
    #[serde(default)]
    pub semantic_score: Option<f32>,
    #[serde(default)]
    pub keyword_score: Option<f32>,
    #[serde(default)]
    pub metadata_score: Option<f32>,
    #[serde(default)]
    pub fusion_score: Option<f32>,
    #[serde(default)]
    pub rerank_score: Option<f32>,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
    #[serde(default)]
    pub payload: Metadata,
}

impl RankedChunk {
    pub fn from_retrieved(document: RetrievedDocument) -> Self {
        let text_fields = DEFAULT_TEXT_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect::<Vec<_>>();
        let content = payload_text(&document.payload, &text_fields);
        let embedding = payload_embedding(&document.payload, "embedding");

        Self {
            id: document.id,
            score: document.score,
            content,
            semantic_score: document.semantic_score,
            keyword_score: document.keyword_score,
            metadata_score: document.metadata_score,
            fusion_score: document.fusion_score,
            rerank_score: None,
            embedding,
            payload: document.payload,
        }
    }
}

#[async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(
        &self,
        query: &str,
        chunks: Vec<RankedChunk>,
    ) -> Result<Vec<RankedChunk>, ContextraError>;
}

#[derive(Debug, Clone)]
pub struct LocalReranker {
    text_fields: Vec<String>,
    candidate_weight: f32,
    lexical_weight: f32,
}

impl LocalReranker {
    pub fn new() -> Self {
        Self {
            text_fields: DEFAULT_TEXT_FIELDS
                .iter()
                .map(|field| (*field).to_string())
                .collect(),
            candidate_weight: 0.35,
            lexical_weight: 0.65,
        }
    }

    pub fn with_text_fields(mut self, fields: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.text_fields = fields.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_weights(mut self, candidate_weight: f32, lexical_weight: f32) -> Self {
        self.candidate_weight = candidate_weight.max(0.0);
        self.lexical_weight = lexical_weight.max(0.0);
        self
    }
}

impl Default for LocalReranker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Reranker for LocalReranker {
    async fn rerank(
        &self,
        query: &str,
        chunks: Vec<RankedChunk>,
    ) -> Result<Vec<RankedChunk>, ContextraError> {
        let query_tokens = tokenize(&normalize_query_text(query));
        let query_terms = query_tokens.iter().cloned().collect::<HashSet<_>>();
        let max_candidate_score = chunks
            .iter()
            .map(|chunk| chunk.score)
            .fold(0.0_f32, f32::max);
        let total_weight = (self.candidate_weight + self.lexical_weight).max(f32::EPSILON);

        let mut reranked = chunks
            .into_iter()
            .map(|mut chunk| {
                let text = if chunk.content.is_empty() {
                    payload_text(&chunk.payload, &self.text_fields)
                } else {
                    chunk.content.clone()
                };
                let text_tokens = tokenize(&normalize_query_text(&text));
                let lexical_score = lexical_relevance(&query_terms, &text_tokens);
                let candidate_score = if max_candidate_score > 0.0 {
                    chunk.score / max_candidate_score
                } else {
                    0.0
                };
                let rerank_score = ((candidate_score * self.candidate_weight)
                    + (lexical_score * self.lexical_weight))
                    / total_weight;

                chunk.rerank_score = Some(rerank_score);
                chunk.score = rerank_score;
                chunk
            })
            .collect::<Vec<_>>();

        sort_ranked_chunks(&mut reranked);
        Ok(reranked)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteRerankerProvider {
    Cohere,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteReranker {
    provider: RemoteRerankerProvider,
    model: String,
}

impl RemoteReranker {
    pub fn cohere(model: impl Into<String>) -> Self {
        Self {
            provider: RemoteRerankerProvider::Cohere,
            model: model.into(),
        }
    }

    pub fn custom(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: RemoteRerankerProvider::Custom(provider.into()),
            model: model.into(),
        }
    }
}

#[async_trait]
impl Reranker for RemoteReranker {
    async fn rerank(
        &self,
        _query: &str,
        _chunks: Vec<RankedChunk>,
    ) -> Result<Vec<RankedChunk>, ContextraError> {
        Err(ContextraError::ProviderError(format!(
            "remote reranker provider {:?} with model '{}' is not configured yet",
            self.provider, self.model
        )))
    }
}

#[derive(Debug, Clone)]
pub struct Deduplicator {
    similarity_threshold: f32,
}

impl Deduplicator {
    pub fn new(similarity_threshold: f32) -> Self {
        Self {
            similarity_threshold: similarity_threshold.clamp(0.0, 1.0),
        }
    }

    pub fn deduplicate(&self, chunks: Vec<RankedChunk>) -> Vec<RankedChunk> {
        let mut kept = Vec::new();
        let mut exact_texts = HashSet::new();

        for chunk in chunks {
            let normalized_text = normalize_query_text(&chunk.content);
            if !normalized_text.is_empty() && !exact_texts.insert(normalized_text) {
                continue;
            }

            if kept
                .iter()
                .any(|kept_chunk| self.is_near_duplicate(&chunk, kept_chunk))
            {
                continue;
            }

            kept.push(chunk);
        }

        kept
    }

    fn is_near_duplicate(&self, left: &RankedChunk, right: &RankedChunk) -> bool {
        let (Some(left_embedding), Some(right_embedding)) =
            (left.embedding.as_deref(), right.embedding.as_deref())
        else {
            return false;
        };

        cosine_similarity(left_embedding, right_embedding) >= self.similarity_threshold
    }
}

impl Default for Deduplicator {
    fn default() -> Self {
        Self::new(0.97)
    }
}

#[derive(Debug, Clone)]
pub struct RetrievalPipeline<Ret, Rerank, Expand = StaticTermExpansionProvider> {
    analyzer: QueryAnalyzer,
    expander: QueryExpander<Expand>,
    retriever: Ret,
    reranker: Rerank,
    deduplicator: Deduplicator,
    default_collection: String,
    filter: RetrievalFilter,
    limit: usize,
}

impl<Ret, Rerank> RetrievalPipeline<Ret, Rerank, StaticTermExpansionProvider> {
    pub fn new(
        retriever: Ret,
        reranker: Rerank,
        default_collection: impl Into<String>,
        limit: usize,
    ) -> Self {
        let default_collection = default_collection.into();
        Self {
            analyzer: QueryAnalyzer::new([default_collection.clone()]),
            expander: QueryExpander::new(),
            retriever,
            reranker,
            deduplicator: Deduplicator::default(),
            default_collection,
            filter: RetrievalFilter::default(),
            limit: limit.max(1),
        }
    }
}

impl<Ret, Rerank, Expand> RetrievalPipeline<Ret, Rerank, Expand>
where
    Expand: TermExpansionProvider,
{
    pub fn with_analyzer(mut self, analyzer: QueryAnalyzer) -> Self {
        self.analyzer = analyzer;
        self
    }

    pub fn with_expander<NextExpand>(
        self,
        expander: QueryExpander<NextExpand>,
    ) -> RetrievalPipeline<Ret, Rerank, NextExpand>
    where
        NextExpand: TermExpansionProvider,
    {
        RetrievalPipeline {
            analyzer: self.analyzer,
            expander,
            retriever: self.retriever,
            reranker: self.reranker,
            deduplicator: self.deduplicator,
            default_collection: self.default_collection,
            filter: self.filter,
            limit: self.limit,
        }
    }

    pub fn with_filter(mut self, filter: RetrievalFilter) -> Self {
        self.filter = filter;
        self
    }

    pub fn with_deduplicator(mut self, deduplicator: Deduplicator) -> Self {
        self.deduplicator = deduplicator;
        self
    }
}

impl<Ret, Rerank, Expand> RetrievalPipeline<Ret, Rerank, Expand>
where
    Ret: Retriever + Send + Sync,
    Rerank: Reranker + Send + Sync,
    Expand: TermExpansionProvider + Send + Sync,
{
    pub async fn run(&self, query: &str) -> Result<Vec<RankedChunk>, ContextraError> {
        let analyzed = self.analyzer.analyze(query);
        let expanded = self.expander.expand(&analyzed);
        let expanded_query = if expanded.terms.is_empty() {
            analyzed.normalized.clone()
        } else {
            expanded.as_search_text()
        };
        let collection = analyzed
            .selected_collections
            .first()
            .cloned()
            .unwrap_or_else(|| self.default_collection.clone());

        let request = RetrievalRequest {
            query: expanded_query,
            collection,
            mode: RetrievalMode::Hybrid,
            limit: self.limit.saturating_mul(3),
            filter: self.filter.clone(),
        };

        let candidates = self.retriever.retrieve(request).await?;
        let ranked = candidates
            .into_iter()
            .map(RankedChunk::from_retrieved)
            .collect::<Vec<_>>();
        let reranked = self.reranker.rerank(query, ranked).await?;
        let mut deduplicated = self.deduplicator.deduplicate(reranked);
        deduplicated.truncate(self.limit);
        Ok(deduplicated)
    }
}

impl<S, E> VectorRetriever<S, E>
where
    E: EmbeddingProvider + Send + Sync,
{
    async fn embed_query(&self, query: &str) -> Result<Vec<f32>, ContextraError> {
        let embeddings = self
            .embeddings
            .embed_batch(&[query.to_string()])
            .await
            .map_err(ContextraError::from)?;

        embeddings.into_iter().next().ok_or_else(|| {
            ContextraError::ProviderError(
                "embedding provider returned no query embedding".to_string(),
            )
        })
    }
}

fn semantic_results(candidates: Vec<VectorSearchResult>) -> Vec<RetrievedDocument> {
    candidates
        .into_iter()
        .map(|candidate| RetrievedDocument {
            id: candidate.id,
            score: candidate.score,
            semantic_score: Some(candidate.score),
            keyword_score: None,
            metadata_score: None,
            fusion_score: None,
            payload: candidate.payload,
        })
        .collect()
}

fn keyword_results(
    query: &str,
    candidates: Vec<VectorSearchResult>,
    text_fields: &[String],
) -> Vec<RetrievedDocument> {
    let keyword_scores = bm25_scores(query, &candidates, text_fields);

    candidates
        .into_iter()
        .filter_map(|candidate| {
            let score = keyword_scores.get(&candidate.id).copied().unwrap_or(0.0);
            (score > 0.0).then_some(RetrievedDocument {
                id: candidate.id,
                score,
                semantic_score: None,
                keyword_score: Some(score),
                metadata_score: None,
                fusion_score: None,
                payload: candidate.payload,
            })
        })
        .collect()
}

fn metadata_results(
    filter: &RetrievalFilter,
    candidates: Vec<VectorSearchResult>,
) -> Vec<RetrievedDocument> {
    candidates
        .into_iter()
        .filter_map(|candidate| {
            let metadata_score = metadata_signal_score(filter, &candidate.payload);
            (metadata_score > 0.0).then_some(RetrievedDocument {
                id: candidate.id,
                score: metadata_score,
                semantic_score: None,
                keyword_score: None,
                metadata_score: Some(metadata_score),
                fusion_score: None,
                payload: candidate.payload,
            })
        })
        .collect()
}

fn hybrid_results(
    query: &str,
    candidates: Vec<VectorSearchResult>,
    text_fields: &[String],
    semantic_weight: f32,
    keyword_weight: f32,
) -> Vec<RetrievedDocument> {
    let keyword_scores = bm25_scores(query, &candidates, text_fields);
    let max_keyword_score = keyword_scores.values().copied().fold(0.0_f32, f32::max);
    let total_weight = (semantic_weight + keyword_weight).max(f32::EPSILON);

    candidates
        .into_iter()
        .map(|candidate| {
            let keyword_score = keyword_scores.get(&candidate.id).copied().unwrap_or(0.0);
            let normalized_keyword = if max_keyword_score > 0.0 {
                keyword_score / max_keyword_score
            } else {
                0.0
            };
            let semantic_score = normalize_similarity(candidate.score);
            let score = ((semantic_score * semantic_weight)
                + (normalized_keyword * keyword_weight))
                / total_weight;

            RetrievedDocument {
                id: candidate.id,
                score,
                semantic_score: Some(candidate.score),
                keyword_score: Some(keyword_score),
                metadata_score: None,
                fusion_score: None,
                payload: candidate.payload,
            }
        })
        .collect()
}

fn reciprocal_rank_fusion(
    lists: Vec<Vec<RetrievedDocument>>,
    rrf_k: f32,
) -> Vec<RetrievedDocument> {
    let mut fused: HashMap<Uuid, RetrievedDocument> = HashMap::new();
    let mut fusion_scores: HashMap<Uuid, f32> = HashMap::new();

    for list in lists {
        for (rank, document) in list.into_iter().enumerate() {
            let rank_score = 1.0 / (rrf_k + rank as f32 + 1.0);
            *fusion_scores.entry(document.id).or_default() += rank_score;

            fused
                .entry(document.id)
                .and_modify(|existing| merge_document_signals(existing, &document))
                .or_insert(document);
        }
    }

    let mut results = fused
        .into_iter()
        .map(|(id, mut document)| {
            let fusion_score = fusion_scores.get(&id).copied().unwrap_or_default();
            document.score = fusion_score;
            document.fusion_score = Some(fusion_score);
            document
        })
        .collect::<Vec<_>>();

    sort_retrieved(&mut results);
    results
}

fn merge_document_signals(existing: &mut RetrievedDocument, incoming: &RetrievedDocument) {
    existing.semantic_score = max_optional_score(existing.semantic_score, incoming.semantic_score);
    existing.keyword_score = max_optional_score(existing.keyword_score, incoming.keyword_score);
    existing.metadata_score = max_optional_score(existing.metadata_score, incoming.metadata_score);

    if incoming.score > existing.score {
        existing.score = incoming.score;
        existing.payload = incoming.payload.clone();
    }
}

fn max_optional_score(left: Option<f32>, right: Option<f32>) -> Option<f32> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn metadata_signal_score(filter: &RetrievalFilter, payload: &Metadata) -> f32 {
    if filter.is_empty() || !filter.matches(payload) {
        return 0.0;
    }

    let mut matched = 0_usize;
    let mut total = 0_usize;

    for (key, expected) in [
        ("collection", filter.collection.as_deref()),
        ("user_id", filter.user_id.as_deref()),
        ("organization_id", filter.organization_id.as_deref()),
        ("language", filter.language.as_deref()),
        ("file_type", filter.file_type.as_deref()),
    ]
    .into_iter()
    .filter_map(|(key, expected)| expected.map(|expected| (key, expected)))
    {
        total += 1;
        if metadata_value_matches(payload.get(key), expected) {
            matched += 1;
        }
    }

    for tag in &filter.tags {
        total += 1;
        if metadata_value_contains(payload.get("tags"), tag) {
            matched += 1;
        }
    }

    if !filter.permissions.is_empty() {
        total += 1;
        if filter
            .permissions
            .iter()
            .any(|permission| metadata_value_contains(payload.get("permissions"), permission))
        {
            matched += 1;
        }
    }

    for condition in &filter.metadata {
        total += 1;
        if condition.matches(payload) {
            matched += 1;
        }
    }

    if total == 0 {
        0.0
    } else {
        matched as f32 / total as f32
    }
}

fn bm25_scores(
    query: &str,
    candidates: &[VectorSearchResult],
    text_fields: &[String],
) -> HashMap<Uuid, f32> {
    let query_tokens = tokenize(&normalize_query_text(query));
    if query_tokens.is_empty() || candidates.is_empty() {
        return HashMap::new();
    }

    let query_terms: HashSet<String> = query_tokens.iter().cloned().collect();
    let documents = candidates
        .iter()
        .map(|candidate| {
            let text = payload_text(&candidate.payload, text_fields);
            let tokens = tokenize(&normalize_query_text(&text));
            (candidate.id, tokens)
        })
        .collect::<Vec<_>>();

    let average_length = documents
        .iter()
        .map(|(_, tokens)| tokens.len() as f32)
        .sum::<f32>()
        / documents.len() as f32;
    let average_length = average_length.max(1.0);

    let mut document_frequency: HashMap<String, usize> = HashMap::new();
    for (_, tokens) in &documents {
        let unique_terms = tokens.iter().cloned().collect::<HashSet<_>>();
        for term in unique_terms.intersection(&query_terms) {
            *document_frequency.entry(term.clone()).or_default() += 1;
        }
    }

    let document_count = documents.len() as f32;
    documents
        .into_iter()
        .map(|(id, tokens)| {
            let document_length = tokens.len() as f32;
            let mut frequencies: HashMap<String, usize> = HashMap::new();
            for token in tokens {
                if query_terms.contains(&token) {
                    *frequencies.entry(token).or_default() += 1;
                }
            }

            let score = frequencies
                .into_iter()
                .map(|(term, term_frequency)| {
                    let df = *document_frequency.get(&term).unwrap_or(&0) as f32;
                    let idf = ((document_count - df + 0.5) / (df + 0.5) + 1.0).ln();
                    let tf = term_frequency as f32;
                    let denominator = tf
                        + (KEYWORD_K1
                            * (1.0 - KEYWORD_B + KEYWORD_B * (document_length / average_length)));
                    idf * ((tf * (KEYWORD_K1 + 1.0)) / denominator)
                })
                .sum::<f32>();

            (id, score)
        })
        .collect()
}

fn sort_retrieved(results: &mut [RetrievedDocument]) {
    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn sort_ranked_chunks(chunks: &mut [RankedChunk]) {
    chunks.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn lexical_relevance(query_terms: &HashSet<String>, text_tokens: &[String]) -> f32 {
    if query_terms.is_empty() || text_tokens.is_empty() {
        return 0.0;
    }

    let text_terms = text_tokens.iter().cloned().collect::<HashSet<_>>();
    let overlap = query_terms.intersection(&text_terms).count() as f32;
    overlap / query_terms.len() as f32
}

fn payload_text(payload: &Metadata, fields: &[String]) -> String {
    fields
        .iter()
        .filter_map(|field| payload.get(field))
        .flat_map(value_text)
        .collect::<Vec<_>>()
        .join(" ")
}

fn value_text(value: &Value) -> Vec<String> {
    match value {
        Value::String(text) => vec![text.clone()],
        Value::Array(items) => items.iter().flat_map(value_text).collect(),
        Value::Object(map) => map.values().flat_map(value_text).collect(),
        Value::Number(number) => vec![number.to_string()],
        Value::Bool(value) => vec![value.to_string()],
        Value::Null => Vec::new(),
    }
}

fn payload_embedding(payload: &Metadata, field: &str) -> Option<Vec<f32>> {
    payload.get(field)?.as_array().map(|values| {
        values
            .iter()
            .filter_map(|value| value.as_f64().map(|number| number as f32))
            .collect::<Vec<_>>()
    })
}

fn normalize_similarity(score: f32) -> f32 {
    ((score + 1.0) / 2.0).clamp(0.0, 1.0)
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }

    let (dot, left_norm, right_norm) = left.iter().zip(right.iter()).fold(
        (0.0_f32, 0.0_f32, 0.0_f32),
        |(dot, left_norm, right_norm), (left, right)| {
            (
                dot + (left * right),
                left_norm + (left * left),
                right_norm + (right * right),
            )
        },
    );

    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

fn detect_intent(original: &str, normalized: &str) -> QueryIntent {
    let first_token = normalized.split_whitespace().next().unwrap_or_default();

    if normalized.contains("compare ")
        || normalized.contains(" vs ")
        || normalized.contains(" versus ")
    {
        QueryIntent::Comparison
    } else if ["summarize", "summary", "recap", "tldr"].contains(&first_token) {
        QueryIntent::Summary
    } else if [
        "implement",
        "create",
        "update",
        "delete",
        "fix",
        "write",
        "run",
    ]
    .contains(&first_token)
    {
        QueryIntent::Command
    } else if [
        "code",
        "function",
        "struct",
        "trait",
        "rust",
        "python",
        "typescript",
        "javascript",
    ]
    .iter()
    .any(|term| normalized.contains(term))
    {
        QueryIntent::Code
    } else if original.trim_end().ends_with('?')
        || ["what", "why", "how", "when", "where", "who", "which"].contains(&first_token)
    {
        QueryIntent::Question
    } else {
        QueryIntent::Search
    }
}

fn extract_entities(query: &str) -> Vec<QueryEntity> {
    let mut entities = Vec::new();
    let mut seen = HashSet::new();

    for phrase in quoted_phrases(query) {
        push_entity(
            &mut entities,
            &mut seen,
            QueryEntityKind::QuotedPhrase,
            phrase,
        );
    }

    for raw in query.split_whitespace() {
        let token = raw.trim_matches(|character: char| {
            character.is_ascii_punctuation()
                && character != '#'
                && character != '@'
                && character != '.'
        });
        if token.is_empty() {
            continue;
        }

        if token.contains('@') && token.contains('.') {
            push_entity(
                &mut entities,
                &mut seen,
                QueryEntityKind::Email,
                token.to_lowercase(),
            );
        } else if let Some(tag) = token.strip_prefix('#') {
            if !tag.is_empty() {
                push_entity(
                    &mut entities,
                    &mut seen,
                    QueryEntityKind::Hashtag,
                    tag.to_lowercase(),
                );
            }
        } else if token.starts_with('.')
            && token.len() > 1
            && token[1..].chars().all(|c| c.is_ascii_alphanumeric())
        {
            push_entity(
                &mut entities,
                &mut seen,
                QueryEntityKind::FileType,
                token[1..].to_lowercase(),
            );
        } else if looks_like_identifier(token) {
            push_entity(
                &mut entities,
                &mut seen,
                QueryEntityKind::Identifier,
                token.to_string(),
            );
        }
    }

    for phrase in proper_noun_phrases(query) {
        push_entity(
            &mut entities,
            &mut seen,
            QueryEntityKind::ProperNoun,
            phrase,
        );
    }

    entities
}

fn push_entity(
    entities: &mut Vec<QueryEntity>,
    seen: &mut HashSet<(QueryEntityKind, String)>,
    kind: QueryEntityKind,
    value: String,
) {
    let value = value.trim().to_string();
    if !value.is_empty() && seen.insert((kind.clone(), value.clone())) {
        entities.push(QueryEntity { kind, value });
    }
}

fn quoted_phrases(query: &str) -> Vec<String> {
    let mut phrases = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for character in query.chars() {
        if character == '"' {
            if in_quote && !current.trim().is_empty() {
                phrases.push(current.trim().to_string());
            }
            current.clear();
            in_quote = !in_quote;
        } else if in_quote {
            current.push(character);
        }
    }

    phrases
}

fn proper_noun_phrases(query: &str) -> Vec<String> {
    let mut phrases = Vec::new();
    let mut current = Vec::new();

    for raw in query.split_whitespace() {
        let token = raw.trim_matches(|character: char| character.is_ascii_punctuation());
        if token
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
        {
            current.push(token.to_string());
        } else if current.len() > 1 {
            phrases.push(current.join(" "));
            current.clear();
        } else {
            current.clear();
        }
    }

    if current.len() > 1 {
        phrases.push(current.join(" "));
    }

    phrases
}

fn looks_like_identifier(token: &str) -> bool {
    token
        .chars()
        .any(|character| character == '_' || character == '-' || character == '/')
        || token.chars().any(|character| character.is_ascii_digit())
            && token
                .chars()
                .any(|character| character.is_ascii_alphabetic())
}

fn normalize_query_text(query: &str) -> String {
    query
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric()
                || character == '_'
                || character == '#'
                || character == '@'
            {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokenize(normalized: &str) -> Vec<String> {
    normalized
        .split_whitespace()
        .filter(|token| token.len() > 1 && !is_stop_word(token))
        .map(str::to_string)
        .collect()
}

fn is_stop_word(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "for"
            | "from"
            | "in"
            | "is"
            | "it"
            | "of"
            | "on"
            | "or"
            | "that"
            | "the"
            | "this"
            | "to"
            | "with"
    )
}

fn default_expansions(term: &str) -> Vec<String> {
    match term {
        "bug" => vec!["error", "issue", "defect"],
        "fix" => vec!["repair", "resolve", "patch"],
        "auth" => vec!["authentication", "authorization", "login"],
        "login" => vec!["signin", "authentication", "session"],
        "vector" => vec!["embedding", "semantic", "similarity"],
        "embedding" => vec!["vector", "semantic", "representation"],
        "rust" => vec!["cargo", "crate", "trait"],
        "summary" => vec!["summarize", "recap", "overview"],
        "document" => vec!["file", "record", "source"],
        _ => Vec::new(),
    }
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn metadata_value_matches(actual: Option<&Value>, expected: &str) -> bool {
    actual.is_some_and(|value| match value {
        Value::String(actual) => actual.eq_ignore_ascii_case(expected),
        Value::Number(number) => number.to_string() == expected,
        Value::Bool(value) => value.to_string() == expected,
        Value::Array(items) => items
            .iter()
            .any(|item| metadata_value_matches(Some(item), expected)),
        Value::Object(_) | Value::Null => false,
    })
}

fn metadata_value_contains(actual: Option<&Value>, expected: &str) -> bool {
    actual.is_some_and(|value| match value {
        Value::String(actual) => actual.eq_ignore_ascii_case(expected),
        Value::Array(items) => items
            .iter()
            .any(|item| metadata_value_contains(Some(item), expected)),
        Value::Object(map) => map
            .values()
            .any(|item| metadata_value_contains(Some(item), expected)),
        Value::Number(number) => number.to_string() == expected,
        Value::Bool(value) => value.to_string() == expected,
        Value::Null => false,
    })
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::String(left), Value::String(right)) => left.eq_ignore_ascii_case(right),
        _ => left == right,
    }
}

fn compare_numbers(left: &Value, right: &Value) -> Option<Ordering> {
    let left = left.as_f64()?;
    let right = right.as_f64()?;
    left.partial_cmp(&right)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use embeddings::{Embedding, EmbeddingError};
    use serde_json::json;
    use storage::vector_store::{InMemoryVectorStore, VectorRecord};

    #[test]
    fn query_analyzer_normalizes_detects_entities_and_selects_collection() {
        let analyzer = QueryAnalyzer::new(["general"])
            .with_collection_rule(CollectionRule::new("engineering", ["rust", "cargo", "bug"]));

        let analyzed = analyzer.analyze(r#"Fix Rust bug BUG-123 for "Login Flow" #auth"#);

        assert_eq!(
            analyzed.normalized,
            "fix rust bug bug 123 for login flow #auth"
        );
        assert_eq!(analyzed.intent, QueryIntent::Command);
        assert_eq!(analyzed.selected_collections, vec!["engineering"]);
        assert!(analyzed.entities.iter().any(|entity| {
            entity.kind == QueryEntityKind::QuotedPhrase && entity.value == "Login Flow"
        }));
        assert!(
            analyzed.entities.iter().any(|entity| {
                entity.kind == QueryEntityKind::Hashtag && entity.value == "auth"
            })
        );
        assert!(analyzed.entities.iter().any(|entity| {
            entity.kind == QueryEntityKind::Identifier && entity.value == "BUG-123"
        }));
    }

    #[test]
    fn query_expander_combines_default_and_pluggable_terms() {
        let analyzer = QueryAnalyzer::default();
        let analyzed = analyzer.analyze("auth vector");
        let expander = QueryExpander::new()
            .with_static_terms("auth", ["oauth", "identity"])
            .with_max_terms(8);

        let expanded = expander.expand(&analyzed);

        assert!(expanded.terms.contains(&"auth".to_string()));
        assert!(expanded.terms.contains(&"oauth".to_string()));
        assert!(expanded.terms.contains(&"identity".to_string()));
        assert!(expanded.terms.contains(&"embedding".to_string()));
    }

    #[test]
    fn retrieval_filter_matches_metadata() {
        let filter = RetrievalFilter {
            collection: Some("docs".to_string()),
            user_id: Some("user-1".to_string()),
            organization_id: None,
            language: Some("en".to_string()),
            file_type: Some("md".to_string()),
            tags: vec!["rust".to_string()],
            permissions: vec!["read".to_string()],
            metadata: vec![MetadataCondition {
                field: "version".to_string(),
                operator: FilterOperator::GreaterThanOrEqual,
                value: json!(2),
            }],
        };

        let payload = metadata([
            ("collection", json!("docs")),
            ("user_id", json!("user-1")),
            ("language", json!("en")),
            ("file_type", json!("md")),
            ("tags", json!(["rust", "retrieval"])),
            ("permissions", json!(["read", "write"])),
            ("version", json!(3)),
        ]);

        assert!(filter.matches(&payload));
    }

    #[tokio::test]
    async fn vector_retriever_semantic_search_uses_vector_store()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = seeded_store().await?;
        let retriever = VectorRetriever::new(store, MockEmbeddingProvider);

        let results = retriever
            .retrieve(RetrievalRequest::semantic("ownership", "docs", 2))
            .await?;

        assert_eq!(
            results[0].payload.get("title"),
            Some(&json!("Rust Ownership"))
        );
        assert!(results[0].semantic_score.is_some());
        assert!(results[0].keyword_score.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn vector_retriever_keyword_search_scores_payload_text()
    -> Result<(), Box<dyn std::error::Error>> {
        let store = seeded_store().await?;
        let retriever = VectorRetriever::new(store, MockEmbeddingProvider);

        let results = retriever
            .retrieve(RetrievalRequest::keyword(
                "redis cache invalidation",
                "docs",
                2,
            ))
            .await?;

        assert_eq!(results[0].payload.get("title"), Some(&json!("Cache Guide")));
        assert!(results[0].keyword_score.unwrap() > 0.0);
        assert!(results[0].semantic_score.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn vector_retriever_applies_metadata_filter() -> Result<(), Box<dyn std::error::Error>> {
        let store = seeded_store().await?;
        let retriever = VectorRetriever::new(store, MockEmbeddingProvider);
        let mut request = RetrievalRequest::semantic("ownership", "docs", 5);
        request.filter.tags = vec!["cache".to_string()];

        let results = retriever.retrieve(request).await?;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].payload.get("title"), Some(&json!("Cache Guide")));
        Ok(())
    }

    #[tokio::test]
    async fn hybrid_retriever_fuses_semantic_keyword_and_metadata_rankings()
    -> Result<(), Box<dyn std::error::Error>> {
        let retriever = HybridRetriever::new(MockModeRetriever).with_rrf_k(1.0);
        let mut request = RetrievalRequest::hybrid("cache", "docs", 3);
        request.filter.tags = vec!["preferred".to_string()];

        let results = retriever.retrieve(request).await?;

        assert_eq!(results[0].id, Uuid::from_u128(2));
        assert_eq!(results[0].semantic_score, Some(0.8));
        assert_eq!(results[0].keyword_score, Some(0.9));
        assert_eq!(results[0].metadata_score, Some(1.0));
        assert!(results[0].fusion_score.is_some());
        Ok(())
    }

    #[test]
    fn deduplicator_removes_exact_duplicates_and_respects_cosine_threshold() {
        let chunks = vec![
            ranked_chunk(Uuid::from_u128(1), "same text", 0.9, vec![1.0, 0.0]),
            ranked_chunk(Uuid::from_u128(2), "same text", 0.8, vec![0.0, 1.0]),
            ranked_chunk(
                Uuid::from_u128(3),
                "same meaning with different words",
                0.7,
                vec![0.8, 0.2],
            ),
            ranked_chunk(Uuid::from_u128(4), "different chunk", 0.6, vec![0.0, 1.0]),
        ];

        let aggressive = Deduplicator::new(0.95).deduplicate(chunks.clone());
        assert_eq!(
            aggressive.iter().map(|chunk| chunk.id).collect::<Vec<_>>(),
            vec![Uuid::from_u128(1), Uuid::from_u128(4)]
        );

        let conservative = Deduplicator::new(0.99).deduplicate(chunks);
        assert_eq!(
            conservative
                .iter()
                .map(|chunk| chunk.id)
                .collect::<Vec<_>>(),
            vec![Uuid::from_u128(1), Uuid::from_u128(3), Uuid::from_u128(4)]
        );
    }

    #[tokio::test]
    async fn retrieval_pipeline_analyzes_retrieves_reranks_and_deduplicates()
    -> Result<(), Box<dyn std::error::Error>> {
        let pipeline =
            RetrievalPipeline::new(MockPipelineRetriever, MockPipelineReranker, "docs", 2)
                .with_deduplicator(Deduplicator::new(0.95));

        let results = pipeline.run("redis cache invalidation").await?;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, Uuid::from_u128(10));
        assert_eq!(results[0].rerank_score, Some(1.0));
        assert_eq!(results[1].id, Uuid::from_u128(12));
        Ok(())
    }

    async fn seeded_store() -> Result<InMemoryVectorStore, ContextraError> {
        let store = InMemoryVectorStore::new();
        store.create_collection("docs", 3).await?;
        store
            .upsert_vectors(
                "docs",
                &[
                    VectorRecord {
                        id: Uuid::from_u128(1),
                        embedding: vec![1.0, 0.0, 0.0],
                        payload: metadata([
                            ("title", json!("Rust Ownership")),
                            (
                                "content",
                                json!("Rust ownership uses borrowing lifetimes and move semantics"),
                            ),
                            ("tags", json!(["rust", "language"])),
                        ]),
                    },
                    VectorRecord {
                        id: Uuid::from_u128(2),
                        embedding: vec![0.0, 1.0, 0.0],
                        payload: metadata([
                            ("title", json!("Cache Guide")),
                            (
                                "content",
                                json!(
                                    "Redis cache invalidation cache TTL and cache stampede control"
                                ),
                            ),
                            ("tags", json!(["cache", "operations"])),
                        ]),
                    },
                    VectorRecord {
                        id: Uuid::from_u128(3),
                        embedding: vec![0.0, 0.0, 1.0],
                        payload: metadata([
                            ("title", json!("Deployment Notes")),
                            (
                                "content",
                                json!("Rollout checklist for service deployments"),
                            ),
                            ("tags", json!(["deploy"])),
                        ]),
                    },
                ],
            )
            .await?;
        Ok(store)
    }

    fn metadata(items: impl IntoIterator<Item = (&'static str, Value)>) -> Metadata {
        items
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect()
    }

    fn retrieved_document(
        id: Uuid,
        title: &'static str,
        content: &'static str,
        score: f32,
        semantic_score: Option<f32>,
        keyword_score: Option<f32>,
        metadata_score: Option<f32>,
    ) -> RetrievedDocument {
        RetrievedDocument {
            id,
            score,
            semantic_score,
            keyword_score,
            metadata_score,
            fusion_score: None,
            payload: metadata([("title", json!(title)), ("content", json!(content))]),
        }
    }

    fn ranked_chunk(
        id: Uuid,
        content: &'static str,
        score: f32,
        embedding: Vec<f32>,
    ) -> RankedChunk {
        RankedChunk {
            id,
            score,
            content: content.to_string(),
            semantic_score: None,
            keyword_score: None,
            metadata_score: None,
            fusion_score: None,
            rerank_score: None,
            embedding: Some(embedding),
            payload: metadata([("content", json!(content))]),
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct MockModeRetriever;

    #[async_trait]
    impl Retriever for MockModeRetriever {
        async fn retrieve(
            &self,
            request: RetrievalRequest,
        ) -> Result<Vec<RetrievedDocument>, ContextraError> {
            let results = match request.mode {
                RetrievalMode::Semantic => vec![
                    retrieved_document(
                        Uuid::from_u128(1),
                        "A",
                        "semantic first",
                        0.9,
                        Some(0.9),
                        None,
                        None,
                    ),
                    retrieved_document(
                        Uuid::from_u128(2),
                        "B",
                        "semantic second",
                        0.8,
                        Some(0.8),
                        None,
                        None,
                    ),
                    retrieved_document(
                        Uuid::from_u128(3),
                        "C",
                        "semantic third",
                        0.7,
                        Some(0.7),
                        None,
                        None,
                    ),
                ],
                RetrievalMode::Keyword => vec![
                    retrieved_document(
                        Uuid::from_u128(2),
                        "B",
                        "keyword first",
                        0.9,
                        None,
                        Some(0.9),
                        None,
                    ),
                    retrieved_document(
                        Uuid::from_u128(3),
                        "C",
                        "keyword second",
                        0.8,
                        None,
                        Some(0.8),
                        None,
                    ),
                    retrieved_document(
                        Uuid::from_u128(1),
                        "A",
                        "keyword third",
                        0.7,
                        None,
                        Some(0.7),
                        None,
                    ),
                ],
                RetrievalMode::Metadata => vec![
                    retrieved_document(
                        Uuid::from_u128(2),
                        "B",
                        "metadata first",
                        1.0,
                        None,
                        None,
                        Some(1.0),
                    ),
                    retrieved_document(
                        Uuid::from_u128(1),
                        "A",
                        "metadata second",
                        0.8,
                        None,
                        None,
                        Some(0.8),
                    ),
                ],
                RetrievalMode::Hybrid => Vec::new(),
            };

            Ok(results)
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct MockPipelineRetriever;

    #[async_trait]
    impl Retriever for MockPipelineRetriever {
        async fn retrieve(
            &self,
            _request: RetrievalRequest,
        ) -> Result<Vec<RetrievedDocument>, ContextraError> {
            Ok(vec![
                RetrievedDocument {
                    id: Uuid::from_u128(10),
                    score: 0.2,
                    semantic_score: Some(0.2),
                    keyword_score: Some(0.1),
                    metadata_score: None,
                    fusion_score: Some(0.2),
                    payload: metadata([
                        ("content", json!("Redis cache invalidation guide")),
                        ("embedding", json!([1.0, 0.0])),
                    ]),
                },
                RetrievedDocument {
                    id: Uuid::from_u128(11),
                    score: 0.9,
                    semantic_score: Some(0.9),
                    keyword_score: None,
                    metadata_score: None,
                    fusion_score: Some(0.9),
                    payload: metadata([
                        ("content", json!("Redis cache invalidation guide")),
                        ("embedding", json!([1.0, 0.0])),
                    ]),
                },
                RetrievedDocument {
                    id: Uuid::from_u128(12),
                    score: 0.8,
                    semantic_score: Some(0.8),
                    keyword_score: None,
                    metadata_score: None,
                    fusion_score: Some(0.8),
                    payload: metadata([
                        ("content", json!("Deployment checklist")),
                        ("embedding", json!([0.0, 1.0])),
                    ]),
                },
            ])
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct MockPipelineReranker;

    #[async_trait]
    impl Reranker for MockPipelineReranker {
        async fn rerank(
            &self,
            _query: &str,
            chunks: Vec<RankedChunk>,
        ) -> Result<Vec<RankedChunk>, ContextraError> {
            let mut reranked = chunks
                .into_iter()
                .map(|mut chunk| {
                    let score = if chunk.id == Uuid::from_u128(10) {
                        1.0
                    } else if chunk.id == Uuid::from_u128(11) {
                        0.95
                    } else {
                        0.5
                    };
                    chunk.score = score;
                    chunk.rerank_score = Some(score);
                    chunk
                })
                .collect::<Vec<_>>();

            sort_ranked_chunks(&mut reranked);
            Ok(reranked)
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct MockEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<Embedding>, EmbeddingError> {
            Ok(inputs
                .iter()
                .map(|input| {
                    let normalized = normalize_query_text(input);
                    if normalized.contains("ownership") || normalized.contains("rust") {
                        vec![1.0, 0.0, 0.0]
                    } else if normalized.contains("cache") || normalized.contains("redis") {
                        vec![0.0, 1.0, 0.0]
                    } else {
                        vec![0.0, 0.0, 1.0]
                    }
                })
                .collect())
        }

        fn dimensions(&self) -> usize {
            3
        }

        fn model_name(&self) -> &str {
            "mock"
        }
    }
}
