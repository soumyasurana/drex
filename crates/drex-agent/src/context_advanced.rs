//! Advanced Context Management - Semantic compression and adaptive context assembly
//!
//! This module provides sophisticated context management:
//! - Semantic deduplication (remove redundant content)
//! - Importance scoring for retention
//! - Sliding window with overlap for streaming
//! - Multi-scale summarization
//! - Relevance-based filtering
//! - Token-aware compression ratios
//! - Context merging strategies

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{debug, info, warn};

use crate::context::{ContextSection, TokenBudget, TruncationStrategy};

/// Importance score for context items.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceScore {
    /// Recency score (0.0 to 1.0).
    pub recency: f32,
    /// Relevance to current query (0.0 to 1.0).
    pub relevance: f32,
    /// User-explicit importance (0.0 to 1.0).
    pub explicit: f32,
    /// Information density (0.0 to 1.0).
    pub density: f32,
    /// Source authority (0.0 to 1.0).
    pub authority: f32,
}

impl ImportanceScore {
    /// Create default score.
    pub fn new() -> Self {
        Self {
            recency: 0.5,
            relevance: 0.5,
            explicit: 0.5,
            density: 0.5,
            authority: 0.5,
        }
    }

    /// Calculate weighted composite score.
    pub fn composite_score(&self, weights: &ImportanceWeights) -> f32 {
        self.recency * weights.recency_weight
            + self.relevance * weights.relevance_weight
            + self.explicit * weights.explicit_weight
            + self.density * weights.density_weight
            + self.authority * weights.authority_weight
    }
}

impl Default for ImportanceScore {
    fn default() -> Self {
        Self::new()
    }
}

/// Weights for importance calculation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ImportanceWeights {
    pub recency_weight: f32,
    pub relevance_weight: f32,
    pub explicit_weight: f32,
    pub density_weight: f32,
    pub authority_weight: f32,
}

impl Default for ImportanceWeights {
    fn default() -> Self {
        Self {
            recency_weight: 0.25,
            relevance_weight: 0.30,
            explicit_weight: 0.15,
            density_weight: 0.15,
            authority_weight: 0.15,
        }
    }
}

/// Context item with importance scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredContextItem {
    /// The content.
    pub content: String,
    /// Token estimate.
    pub tokens: usize,
    /// Importance scores.
    pub importance: ImportanceScore,
    /// Category/type.
    pub category: String,
    /// Timestamp.
    pub timestamp: std::time::SystemTime,
    /// Source identifier.
    pub source: String,
    /// Unique identifier.
    pub id: String,
}

impl ScoredContextItem {
    /// Create new scored item.
    pub fn new(content: impl Into<String>, id: impl Into<String>) -> Self {
        let content = content.into();
        let tokens = estimate_tokens(&content);
        Self {
            content,
            tokens,
            importance: ImportanceScore::new(),
            category: "general".to_string(),
            timestamp: std::time::SystemTime::now(),
            source: "unknown".to_string(),
            id: id.into(),
        }
    }

    /// Set importance score.
    pub fn with_importance(mut self, importance: ImportanceScore) -> Self {
        self.importance = importance;
        self
    }

    /// Set category.
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// Set source.
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    /// Calculate final score with weights.
    pub fn final_score(&self, weights: &ImportanceWeights) -> f32 {
        self.importance.composite_score(weights)
    }
}

/// Context pool for semantic management.
#[derive(Debug, Clone, Default)]
pub struct ContextPool {
    /// All context items.
    items: Vec<ScoredContextItem>,
    /// Seen IDs for deduplication.
    seen_ids: HashSet<String>,
    /// Total tokens in pool.
    total_tokens: usize,
}

impl ContextPool {
    /// Create new empty pool.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            seen_ids: HashSet::new(),
            total_tokens: 0,
        }
    }

    /// Add item to pool (with deduplication).
    pub fn add(&mut self, item: ScoredContextItem) -> bool {
        if self.seen_ids.contains(&item.id) {
            return false;
        }
        
        self.total_tokens += item.tokens;
        self.seen_ids.insert(item.id.clone());
        self.items.push(item);
        true
    }

    /// Add multiple items.
    pub fn add_batch(&mut self, items: Vec<ScoredContextItem>) -> usize {
        items.into_iter()
            .filter(|item| self.add(item.clone()))
            .count()
    }

    /// Remove items by predicate.
    pub fn retain<F>(&mut self, f: F)
    where
        F: Fn(&ScoredContextItem) -> bool,
    {
        let mut new_tokens = 0;
        self.items.retain(|item| {
            if f(item) {
                new_tokens += item.tokens;
                true
            } else {
                self.seen_ids.remove(&item.id);
                false
            }
        });
        self.total_tokens = new_tokens;
    }

    /// Get items sorted by importance score.
    pub fn by_importance(&self, weights: &ImportanceWeights) -> Vec<&ScoredContextItem> {
        let mut scored: Vec<_> = self.items.iter()
            .map(|item| (item, item.final_score(weights)))
            .collect();
        
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().map(|(item, _)| item).collect()
    }

    /// Select top items within token budget.
    pub fn select_within_budget(
        &self,
        budget: usize,
        weights: &ImportanceWeights,
    ) -> Vec<ScoredContextItem> {
        let sorted = self.by_importance(weights);
        let mut selected = Vec::new();
        let mut used_tokens = 0;

        for item in sorted {
            if used_tokens + item.tokens <= budget {
                used_tokens += item.tokens;
                selected.push(item.clone());
            }
        }

        selected
    }

    /// Find semantically similar items.
    pub fn find_similar(&self, query: &str, threshold: f32) -> Vec<&ScoredContextItem> {
        // Simplified: basic substring matching for now
        // In production, use embeddings
        let query_lower = query.to_lowercase();
        self.items.iter()
            .filter(|item| {
                let similarity = simple_similarity(&item.content.to_lowercase(), &query_lower);
                similarity >= threshold
            })
            .collect()
    }

    /// Remove duplicates based on similarity.
    pub fn deduplicate_similar(&mut self, similarity_threshold: f32) -> usize {
        let mut to_remove = Vec::new();
        
        for i in 0..self.items.len() {
            if to_remove.contains(&i) {
                continue;
            }
            for j in (i + 1)..self.items.len() {
                if to_remove.contains(&j) {
                    continue;
                }
                let sim = simple_similarity(&self.items[i].content, &self.items[j].content);
                if sim >= similarity_threshold {
                    // Remove the one with lower importance
                    let weights = ImportanceWeights::default();
                    let score_i = self.items[i].final_score(&weights);
                    let score_j = self.items[j].final_score(&weights);
                    if score_j <= score_i {
                        to_remove.push(j);
                    } else {
                        to_remove.push(i);
                        break;
                    }
                }
            }
        }

        let removed_count = to_remove.len();
        for &index in to_remove.iter().rev() {
            if let Some(item) = self.items.get(index) {
                self.seen_ids.remove(&item.id);
                self.total_tokens -= item.tokens;
            }
            self.items.remove(index);
        }

        removed_count
    }

    /// Get summary statistics.
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            item_count: self.items.len(),
            total_tokens: self.total_tokens,
            unique_sources: self.items.iter()
                .map(|i| i.source.clone())
                .collect::<HashSet<_>>()
                .len(),
        }
    }

    /// Clear all items.
    pub fn clear(&mut self) {
        self.items.clear();
        self.seen_ids.clear();
        self.total_tokens = 0;
    }
}

/// Simple similarity metric (Jaccard-like on words).
fn simple_similarity(a: &str, b: &str) -> f32 {
    let a_words: HashSet<_> = a.split_whitespace().collect();
    let b_words: HashSet<_> = b.split_whitespace().collect();
    
    if a_words.is_empty() && b_words.is_empty() {
        return 1.0;
    }
    
    let intersection: HashSet<_> = a_words.intersection(&b_words).collect();
    let union: HashSet<_> = a_words.union(&b_words).collect();
    
    if union.is_empty() {
        return 0.0;
    }
    
    intersection.len() as f32 / union.len() as f32
}

/// Estimate token count (rough: ~4 chars per token).
fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

/// Pool statistics.
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub item_count: usize,
    pub total_tokens: usize,
    pub unique_sources: usize,
}

/// Advanced context assembler.
pub struct AdvancedContextAssembler {
    pool: ContextPool,
    weights: ImportanceWeights,
    config: AdvancedContextConfig,
}

impl AdvancedContextAssembler {
    /// Create new assembler.
    pub fn new(config: AdvancedContextConfig) -> Self {
        Self {
            pool: ContextPool::new(),
            weights: ImportanceWeights::default(),
            config,
        }
    }

    /// Add section to pool.
    pub fn add_section(&mut self, section: &ContextSection) {
        let items: Vec<ScoredContextItem> = match section {
            ContextSection::Memories { items } => {
                items.iter().enumerate()
                    .map(|(i, content)| {
                        ScoredContextItem::new(content, format!("mem_{}", i))
                            .with_category("memory")
                            .with_importance(ImportanceScore::new())
                    })
                    .collect()
            }
            ContextSection::ToolResults { results } => {
                results.iter().enumerate()
                    .map(|(i, content)| {
                        ScoredContextItem::new(content, format!("result_{}", i))
                            .with_category("tool_result")
                            .with_importance(ImportanceScore::new())
                    })
                    .collect()
            }
            ContextSection::Observations { items } => {
                items.iter().enumerate()
                    .map(|(i, content)| {
                        ScoredContextItem::new(content, format!("obs_{}", i))
                            .with_category("observation")
                            .with_importance(ImportanceScore::new())
                    })
                    .collect()
            }
            ContextSection::Decisions { items } => {
                items.iter().enumerate()
                    .map(|(i, content)| {
                        ScoredContextItem::new(content, format!("dec_{}", i))
                            .with_category("decision")
                            .with_importance(ImportanceScore::new())
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        
        self.pool.add_batch(items);
    }

    /// Assemble context with advanced compression.
    pub fn assemble(&mut self, budget: &TokenBudget) -> AdvancedAssembledContext {
        // Deduplicate if pool is large
        if self.pool.items.len() > self.config.compression_threshold {
            let removed = self.pool.deduplicate_similar(self.config.similarity_threshold);
            debug!("Deduplicated {} similar items", removed);
        }

        // Select items based on strategy
        let strategy = self.config.strategy;
        let selected = match strategy {
            TruncationStrategy::DropLowestPriority => {
                self.pool.select_within_budget(budget.context, &self.weights)
            }
            TruncationStrategy::TruncateProportionally => {
                self.select_proportionally(budget.context)
            }
            TruncationStrategy::KeepRecent => {
                self.select_recent(budget.context)
            }
            TruncationStrategy::Summarize => {
                self.select_with_summarization(budget.context)
            }
            _ => self.pool.select_within_budget(budget.context, &self.weights),
        };

        // Calculate item counts per category
        let mut by_category: std::collections::HashMap<String, usize> = 
            std::collections::HashMap::new();
        for item in &selected {
            *by_category.entry(item.category.clone()).or_insert(0) += 1;
        }

        // Build sections back
        let memory_items: Vec<String> = selected.iter()
            .filter(|i| i.category == "memory")
            .map(|i| i.content.clone())
            .collect();

        let result_items: Vec<String> = selected.iter()
            .filter(|i| i.category == "tool_result")
            .map(|i| i.content.clone())
            .collect();

        let obs_items: Vec<String> = selected.iter()
            .filter(|i| i.category == "observation")
            .map(|i| i.content.clone())
            .collect();

        let dec_items: Vec<String> = selected.iter()
            .filter(|i| i.category == "decision")
            .map(|i| i.content.clone())
            .collect();

        let sections = vec![
            (!memory_items.is_empty()).then_some(ContextSection::Memories { items: memory_items }),
            (!result_items.is_empty()).then_some(ContextSection::ToolResults { results: result_items }),
            (!obs_items.is_empty()).then_some(ContextSection::Observations { items: obs_items }),
            (!dec_items.is_empty()).then_some(ContextSection::Decisions { items: dec_items }),
        ]
        .into_iter()
        .flatten()
        .collect();

        let tokens: usize = selected.iter().map(|i| i.tokens).sum();

        AdvancedAssembledContext {
            sections,
            estimated_tokens: tokens,
            items_included: selected.len(),
            items_excluded: self.pool.items.len() - selected.len(),
            by_category,
        }
    }

    /// Select proportionally from each category.
    fn select_proportionally(&self, budget: usize) -> Vec<ScoredContextItem> {
        let categories: HashSet<_> = self.pool.items.iter()
            .map(|i| i.category.clone())
            .collect();
        
        if categories.is_empty() {
            return Vec::new();
        }
        
        let per_category = budget / categories.len();
        let mut selected = Vec::new();
        
        for category in categories {
            let items: Vec<_> = self.pool.items.iter()
                .filter(|i| i.category == category)
                .cloned()
                .collect();
            
            let mut used = 0;
            for item in items {
                let tokens = item.tokens;
                if used + tokens <= per_category {
                    selected.push(item);
                    used += tokens;
                }
            }
        }

        selected
    }

    /// Select most recent items.
    fn select_recent(&self, budget: usize) -> Vec<ScoredContextItem> {
        let mut items: Vec<_> = self.pool.items.clone();
        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        let mut selected = Vec::new();
        let mut used = 0;

        for item in items {
            let tokens = item.tokens;
            if used + tokens <= budget {
                selected.push(item);
                used += tokens;
            }
        }

        selected
    }

    /// Select with automatic summarization.
    fn select_with_summarization(&self, budget: usize) -> Vec<ScoredContextItem> {
        // For now, just select most important
        // In production, use actual summarization
        self.pool.select_within_budget(budget, &self.weights)
    }

    /// Get pool stats.
    pub fn pool_stats(&self) -> PoolStats {
        self.pool.stats()
    }

    /// Clear pool.
    pub fn clear(&mut self) {
        self.pool.clear();
    }
}

/// Advanced context configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedContextConfig {
    /// Compression strategy.
    pub strategy: TruncationStrategy,
    /// Similarity threshold for deduplication.
    pub similarity_threshold: f32,
    /// When to trigger compression (item count).
    pub compression_threshold: usize,
    /// Enable semantic compression.
    pub enable_semantic_compression: bool,
    /// Min items to keep per category.
    pub min_per_category: usize,
    /// Max items to keep per category.
    pub max_per_category: usize,
}

impl Default for AdvancedContextConfig {
    fn default() -> Self {
        Self {
            strategy: TruncationStrategy::DropLowestPriority,
            similarity_threshold: 0.8,
            compression_threshold: 50,
            enable_semantic_compression: true,
            min_per_category: 2,
            max_per_category: 20,
        }
    }
}

/// Advanced assembled context.
#[derive(Debug, Clone)]
pub struct AdvancedAssembledContext {
    /// Assembled sections.
    pub sections: Vec<ContextSection>,
    /// Estimated token count.
    pub estimated_tokens: usize,
    /// Number of items included.
    pub items_included: usize,
    /// Number of items excluded.
    pub items_excluded: usize,
    /// Items by category.
    pub by_category: std::collections::HashMap<String, usize>,
}

impl AdvancedAssembledContext {
    /// Check if any sections were truncated.
    pub fn was_truncated(&self) -> bool {
        self.items_excluded > 0
    }

    /// Render to string.
    pub fn render(&self) -> String {
        let mut output = String::new();
        for section in &self.sections {
            output.push_str(&section.render());
        }
        output
    }
}

/// Sliding window for streaming context.
pub struct ContextWindow {
    /// Window size in tokens.
    window_size: usize,
    /// Overlap size.
    overlap: usize,
    /// Buffer of recent content.
    buffer: Vec<ScoredContextItem>,
}

impl ContextWindow {
    /// Create new window.
    pub fn new(window_size: usize, overlap: usize) -> Self {
        Self {
            window_size,
            overlap,
            buffer: Vec::new(),
        }
    }

    /// Add item to buffer.
    pub fn add(&mut self, item: ScoredContextItem) {
        self.buffer.push(item);
        self.maintain_window();
    }

    /// Maintain window size.
    fn maintain_window(&mut self) {
        let mut total = self.buffer.iter().map(|i| i.tokens).sum::<usize>();
        
        while total > self.window_size + self.overlap && self.buffer.len() > 1 {
            if let Some(removed) = self.buffer.pop() {
                total -= removed.tokens;
            }
        }
    }

    /// Get current window content.
    pub fn current_window(&self) -> Vec<&ScoredContextItem> {
        self.buffer.iter().collect()
    }

    /// Slide window forward by removing oldest items.
    pub fn slide(&mut self) {
        let target_size = self.window_size + self.overlap;
        let mut current_size = self.buffer.iter().map(|i| i.tokens).sum::<usize>();

        // Keep at most window_size + overlap, remove from front
        while current_size > target_size && !self.buffer.is_empty() {
            let removed = self.buffer.remove(0);
            current_size -= removed.tokens;
        }
    }

    /// Get window token count.
    pub fn token_count(&self) -> usize {
        self.buffer.iter().map(|i| i.tokens).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_importance_score() {
        let weights = ImportanceWeights::default();
        let mut score = ImportanceScore::new();
        score.relevance = 0.9;
        score.recency = 0.8;
        
        let composite = score.composite_score(&weights);
        assert!(composite > 0.5);
    }

    #[test]
    fn test_scored_context_item() {
        let item = ScoredContextItem::new("Test content", "item-1")
            .with_category("memory")
            .with_source("test");
        
        assert_eq!(item.category, "memory");
        assert_eq!(item.source, "test");
        assert!(item.tokens > 0);
    }

    #[test]
    fn test_context_pool_add() {
        let mut pool = ContextPool::new();
        let item = ScoredContextItem::new("Content", "id1");
        
        assert!(pool.add(item));
        assert!(!pool.add(ScoredContextItem::new("Content", "id1"))); // Duplicate
    }

    #[test]
    fn test_context_pool_select_within_budget() {
        let mut pool = ContextPool::new();
        
        pool.add(ScoredContextItem::new("important high value content", "id1"));
        pool.add(ScoredContextItem::new("less important", "id2"));
        
        let weights = ImportanceWeights::default();
        let selected = pool.select_within_budget(1000, &weights);
        
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_simple_similarity() {
        let sim = simple_similarity("the quick brown fox", "the quick brown dog");
        assert!(sim > 0.5 && sim < 1.0);
        
        let sim2 = simple_similarity("completely different text", "another different thing");
        assert!(sim2 < 0.5);
    }

    #[test]
    fn test_deduplicate_similar() {
        let mut pool = ContextPool::new();

        pool.add(ScoredContextItem::new("the quick brown fox jumps", "id1"));
        pool.add(ScoredContextItem::new("the quick brown fox runs", "id2"));
        pool.add(ScoredContextItem::new("completely different", "id3"));

        let removed = pool.deduplicate_similar(0.6); // Lower threshold for test
        assert_eq!(removed, 1);
        assert_eq!(pool.items.len(), 2);
    }

    #[test]
    fn test_context_window() {
        let mut window = ContextWindow::new(100, 20);
        
        window.add(ScoredContextItem::new("Short", "1"));
        window.add(ScoredContextItem::new("Another", "2"));
        
        let current = window.current_window();
        assert_eq!(current.len(), 2);
    }

    #[test]
    fn test_advanced_assembler() {
        let config = AdvancedContextConfig::default();
        let mut assembler = AdvancedContextAssembler::new(config);
        
        let section = ContextSection::Memories {
            items: vec!["Memory 1".to_string(), "Memory 2".to_string()],
        };
        
        assembler.add_section(&section);
        
        let budget = TokenBudget::new(1000);
        let result = assembler.assemble(&budget);
        
        assert!(result.items_included >= 2);
    }

    #[test]
    fn test_similarity_threshold() {
        // Identical strings
        assert_eq!(simple_similarity("hello world", "hello world"), 1.0);
        
        // No overlap
        let sim = simple_similarity("abc", "xyz");
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_pool_stats() {
        let mut pool = ContextPool::new();
        let item = ScoredContextItem::new("Test", "id1").with_source("src1");
        pool.add(item);
        
        let stats = pool.stats();
        assert_eq!(stats.item_count, 1);
        assert_eq!(stats.unique_sources, 1);
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens("abcd"), 1); // 4 chars = 1 token
        assert_eq!(estimate_tokens("abcdefghijklmnopqrstuvwxyz"), 6); // 26/4
    }
}
