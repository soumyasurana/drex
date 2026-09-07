//! Learning & Adaptation - Persistent Memory and Behavioral Learning
//!
//! This module provides mechanisms for DREX to learn from interactions,
//! adapt behavior based on feedback, and improve over time.
//!
//! # Learning Types
//!
//! 1. **Preference Learning**: Learn user preferences from choices and feedback
//! 2. **Skill Acquisition**: Learn new tool usage patterns and workflows
//! 3. **Error Correction**: Learn from mistakes to avoid repeating them
//! 4. **Contextual Memory**: Associate contexts with successful strategies
//! 5. **Feedback Integration**: Incorporate explicit user feedback
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         LearningEngine                       │
//! ├─────────────────────────────────────────────────────────────┤
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
//! │  │ Preference │  │   Skill     │  │   Feedback Loop     │  │
//! │  │   Store    │  │  Learner    │  │                     │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────────┘  │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// A unique identifier for a learned preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PreferenceId(pub uuid::Uuid);

impl PreferenceId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for PreferenceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PreferenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Types of things that can be learned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningType {
    /// User preference for output format, style, etc.
    UserPreference,
    /// Tool usage pattern or workflow.
    ToolPattern,
    /// Error to avoid in the future.
    ErrorPattern,
    /// Successful strategy for a context.
    SuccessStrategy,
    /// Contextual association.
    ContextAssociation,
}

impl fmt::Display for LearningType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserPreference => write!(f, "user_preference"),
            Self::ToolPattern => write!(f, "tool_pattern"),
            Self::ErrorPattern => write!(f, "error_pattern"),
            Self::SuccessStrategy => write!(f, "success_strategy"),
            Self::ContextAssociation => write!(f, "context_association"),
        }
    }
}

/// A learned preference or pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedItem {
    /// Unique identifier.
    pub id: PreferenceId,
    /// Type of learning.
    pub learning_type: LearningType,
    /// Context where this applies (e.g., "code_generation", "file_operations").
    pub context: String,
    /// The key/pattern being learned.
    pub key: String,
    /// The value/preference.
    pub value: String,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f32,
    /// When this was first learned.
    pub created_at: DateTime<Utc>,
    /// When this was last reinforced.
    pub last_used: DateTime<Utc>,
    /// How many times this has been reinforced.
    pub reinforcement_count: u32,
    /// Whether this is currently active.
    pub active: bool,
    /// Optional metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl LearnedItem {
    /// Create a new learned item.
    pub fn new(
        learning_type: LearningType,
        context: impl Into<String>,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: PreferenceId::new(),
            learning_type,
            context: context.into(),
            key: key.into(),
            value: value.into(),
            confidence: 0.5, // Start with moderate confidence
            created_at: now,
            last_used: now,
            reinforcement_count: 1,
            active: true,
            metadata: None,
        }
    }

    /// Reinforce this learning (increase confidence).
    pub fn reinforce(&mut self) {
        self.reinforcement_count += 1;
        self.last_used = Utc::now();
        // Confidence increases with reinforcement but asymptotically approaches 1.0
        self.confidence = 1.0 - (1.0 - self.confidence) * 0.9;
    }

    /// Reduce confidence (e.g., on negative feedback).
    pub fn penalize(&mut self) {
        self.confidence *= 0.8;
        if self.confidence < 0.1 {
            self.active = false;
        }
    }

    /// Check if this learning is stale (unused for a long time).
    pub fn is_stale(&self, days: i64) -> bool {
        let age = Utc::now() - self.last_used;
        age.num_days() > days
    }

    /// Format as memory content for storage.
    pub fn to_memory_content(&self) -> String {
        format!(
            "[{}] {}: {} = {} (confidence: {:.2}, reinforced {} times)",
            self.learning_type,
            self.context,
            self.key,
            self.value,
            self.confidence,
            self.reinforcement_count
        )
    }
}

/// Feedback from user or system about performance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feedback {
    /// Explicit positive feedback.
    Positive,
    /// Explicit negative feedback.
    Negative,
    /// Implicit positive (task completed successfully).
    Success,
    /// Implicit negative (task failed or had issues).
    Failure,
    /// Neutral observation.
    Neutral,
}

impl Feedback {
    /// Get numeric score for this feedback.
    pub fn score(&self) -> f32 {
        match self {
            Self::Positive => 1.0,
            Self::Success => 0.5,
            Self::Neutral => 0.0,
            Self::Failure => -0.5,
            Self::Negative => -1.0,
        }
    }
}

/// A learning event to process.
#[derive(Debug, Clone)]
pub struct LearningEvent {
    /// Type of learning.
    pub learning_type: LearningType,
    /// Context for this learning.
    pub context: String,
    /// Key being learned.
    pub key: String,
    /// Value learned.
    pub value: String,
    /// User or system feedback.
    pub feedback: Feedback,
    /// Optional source (tool call, user input, etc.).
    pub source: Option<String>,
}

/// Configuration for the learning engine.
#[derive(Debug, Clone)]
pub struct LearningConfig {
    /// Minimum confidence to consider a learning valid.
    pub min_confidence: f32,
    /// Days before a learning becomes stale.
    pub stale_days: i64,
    /// Whether to enable learning.
    pub enabled: bool,
    /// Maximum number of items to retain per context.
    pub max_per_context: usize,
}

impl Default for LearningConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.3,
            stale_days: 30,
            enabled: true,
            max_per_context: 100,
        }
    }
}

/// Error types for learning.
#[derive(Debug, Error)]
pub enum LearningError {
    #[error("Learning is disabled")]
    Disabled,
    #[error("Item not found: {0}")]
    ItemNotFound(PreferenceId),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Invalid confidence value: {0}")]
    InvalidConfidence(f32),
}

/// In-memory storage for learned items.
pub struct InMemoryLearningStore {
    items: RwLock<HashMap<PreferenceId, LearnedItem>>,
    by_context: RwLock<HashMap<String, Vec<PreferenceId>>>,
}

impl InMemoryLearningStore {
    pub fn new() -> Self {
        Self {
            items: RwLock::new(HashMap::new()),
            by_context: RwLock::new(HashMap::new()),
        }
    }

    pub async fn store(&self, item: LearnedItem) -> Result<(), LearningError> {
        let mut items = self.items.write().await;
        let mut by_context = self.by_context.write().await;

        by_context
            .entry(item.context.clone())
            .or_default()
            .push(item.id);
        items.insert(item.id, item);

        Ok(())
    }

    pub async fn get(&self, id: PreferenceId) -> Result<LearnedItem, LearningError> {
        let items = self.items.read().await;
        items
            .get(&id)
            .cloned()
            .ok_or(LearningError::ItemNotFound(id))
    }

    pub async fn update(&self, item: LearnedItem) -> Result<(), LearningError> {
        let mut items = self.items.write().await;
        if !items.contains_key(&item.id) {
            return Err(LearningError::ItemNotFound(item.id));
        }
        items.insert(item.id, item);
        Ok(())
    }

    pub async fn find_by_context(&self, context: &str) -> Vec<LearnedItem> {
        let items = self.items.read().await;
        let by_context = self.by_context.read().await;

        by_context
            .get(context)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| items.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn find_by_key(&self, context: &str, key: &str) -> Vec<LearnedItem> {
        let items = self.items.read().await;

        items
            .values()
            .filter(|item| item.context == context && item.key == key && item.active)
            .cloned()
            .collect()
    }

    pub async fn get_preference(&self, context: &str, key: &str) -> Option<String> {
        let matches = self.find_by_key(context, key).await;

        // Return the highest confidence matching value
        matches
            .into_iter()
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap())
            .map(|item| item.value)
    }

    pub async fn cleanup_stale(&self, days: i64) -> usize {
        let mut items = self.items.write().await;
        let mut by_context = self.by_context.write().await;

        let stale_ids: Vec<_> = items
            .values()
            .filter(|item| item.is_stale(days))
            .map(|item| item.id)
            .collect();

        for id in &stale_ids {
            if let Some(item) = items.remove(id) {
                if let Some(ctx) = by_context.get_mut(&item.context) {
                    ctx.retain(|&x| x != *id);
                }
            }
        }

        stale_ids.len()
    }

    pub async fn clear(&self) {
        let mut items = self.items.write().await;
        let mut by_context = self.by_context.write().await;
        items.clear();
        by_context.clear();
    }
}

impl Default for InMemoryLearningStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Learning engine for processing events and managing learned items.
pub struct LearningEngine {
    config: LearningConfig,
    store: Arc<InMemoryLearningStore>,
}

impl LearningEngine {
    pub fn new() -> Self {
        Self::with_config(LearningConfig::default())
    }

    pub fn with_config(config: LearningConfig) -> Self {
        Self {
            config,
            store: Arc::new(InMemoryLearningStore::new()),
        }
    }

    /// Process a learning event.
    pub async fn learn(&self, event: LearningEvent) -> Result<LearnedItem, LearningError> {
        if !self.config.enabled {
            return Err(LearningError::Disabled);
        }

        // Check if we already have a similar learning
        let existing = self.store.find_by_key(&event.context, &event.key).await;

        if let Some(mut item) = existing.into_iter().next() {
            // Reinforce or penalize existing learning
            if event.feedback.score() > 0.0 {
                item.reinforce();
                info!(
                    context = %event.context,
                    key = %event.key,
                    confidence = item.confidence,
                    "Reinforced learning"
                );
            } else if event.feedback.score() < 0.0 {
                item.penalize();
                warn!(
                    context = %event.context,
                    key = %event.key,
                    confidence = item.confidence,
                    "Penalized learning"
                );
            }

            self.store.update(item.clone()).await?;
            Ok(item)
        } else {
            // Create new learning
            let mut item = LearnedItem::new(
                event.learning_type,
                &event.context,
                &event.key,
                &event.value,
            );

            // Adjust initial confidence based on feedback
            item.confidence = 0.5 + event.feedback.score() * 0.3;
            item.confidence = item.confidence.clamp(0.0, 1.0);

            // Check capacity
            let context_items = self.store.find_by_context(&event.context).await;
            if context_items.len() >= self.config.max_per_context {
                // Remove lowest confidence item
                if let Some(oldest) = context_items
                    .iter()
                    .filter(|i| i.confidence < item.confidence)
                    .min_by_key(|i| i.reinforcement_count)
                {
                    // Just mark as inactive rather than delete
                    let mut to_update = oldest.clone();
                    to_update.active = false;
                    self.store.update(to_update).await?;
                }
            }

            self.store.store(item.clone()).await?;
            info!(
                context = %event.context,
                key = %event.key,
                value = %event.value,
                "Created new learning"
            );

            Ok(item)
        }
    }

    /// Get a learned preference for a context/key.
    pub async fn get_preference(&self, context: &str, key: &str) -> Option<String> {
        self.store.get_preference(context, key).await
    }

    /// Get all learnings for a context.
    pub async fn get_context_learnings(&self, context: &str) -> Vec<LearnedItem> {
        self.store
            .find_by_context(context)
            .await
            .into_iter()
            .filter(|item| item.confidence >= self.config.min_confidence)
            .collect()
    }

    /// Apply feedback to an existing learning.
    pub async fn feedback(&self, id: PreferenceId, feedback: Feedback) -> Result<(), LearningError> {
        let mut item = self.store.get(id).await?;

        if feedback.score() > 0.0 {
            item.reinforce();
        } else if feedback.score() < 0.0 {
            item.penalize();
        }

        self.store.update(item).await
    }

    /// Get learnings formatted as context for LLM.
    pub async fn get_formatted_context(&self, context: &str) -> String {
        let learnings = self.get_context_learnings(context).await;

        if learnings.is_empty() {
            return String::new();
        }

        let mut result = format!("Learned preferences for {}:\n", context);
        for item in learnings {
            if item.confidence >= 0.7 {
                result.push_str(&format!(
                    "- {}: {} (conf: {:.0}%)\n",
                    item.key, item.value, item.confidence * 100.0
                ));
            }
        }

        result
    }

    /// Cleanup stale learnings.
    pub async fn cleanup(&self) -> usize {
        self.store.cleanup_stale(self.config.stale_days).await
    }

    /// Get learning statistics.
    pub async fn stats(&self) -> LearningStats {
        let items = self.store.items.read().await;

        LearningStats {
            total_items: items.len(),
            by_type: items.values().fold(HashMap::new(), |mut acc, item| {
                *acc.entry(item.learning_type).or_insert(0) += 1;
                acc
            }),
            active_items: items.values().filter(|i| i.active).count(),
            high_confidence: items.values().filter(|i| i.confidence >= 0.8).count(),
        }
    }
}

impl Default for LearningEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about learning.
#[derive(Debug, Clone)]
pub struct LearningStats {
    pub total_items: usize,
    pub by_type: HashMap<LearningType, usize>,
    pub active_items: usize,
    pub high_confidence: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_preference_learning() {
        let engine = LearningEngine::new();

        let event = LearningEvent {
            learning_type: LearningType::UserPreference,
            context: "code_generation".to_string(),
            key: "language".to_string(),
            value: "rust".to_string(),
            feedback: Feedback::Positive,
            source: None,
        };

        let item = engine.learn(event).await.unwrap();
        assert_eq!(item.key, "language");
        assert_eq!(item.value, "rust");
        assert!(item.confidence > 0.5);

        // Get preference
        let pref = engine.get_preference("code_generation", "language").await;
        assert_eq!(pref, Some("rust".to_string()));
    }

    #[tokio::test]
    async fn test_reinforcement() {
        let engine = LearningEngine::new();

        let event = LearningEvent {
            learning_type: LearningType::UserPreference,
            context: "test".to_string(),
            key: "format".to_string(),
            value: "json".to_string(),
            feedback: Feedback::Positive,
            source: None,
        };

        let item1 = engine.learn(event.clone()).await.unwrap();
        let item2 = engine.learn(event).await.unwrap();

        assert_eq!(item1.id, item2.id);
        assert!(item2.confidence > item1.confidence);
        assert_eq!(item2.reinforcement_count, 2);
    }

    #[tokio::test]
    async fn test_penalize() {
        let engine = LearningEngine::new();

        // First learn positively
        let event1 = LearningEvent {
            learning_type: LearningType::UserPreference,
            context: "test".to_string(),
            key: "style".to_string(),
            value: "verbose".to_string(),
            feedback: Feedback::Positive,
            source: None,
        };
        let item = engine.learn(event1).await.unwrap();
        let conf1 = item.confidence;

        // Then penalize
        engine.feedback(item.id, Feedback::Negative).await.unwrap();
        let item = engine.store.get(item.id).await.unwrap();
        assert!(item.confidence < conf1);
    }

    #[tokio::test]
    async fn test_stale_cleanup() {
        // Create store and manually add stale item
        let store = InMemoryLearningStore::new();
        
        // Create an item with last_used in the past
        let mut item = LearnedItem::new(
            LearningType::UserPreference,
            "test",
            "old",
            "value",
        );
        // Set last_used to 31 days ago
        item.last_used = Utc::now() - chrono::Duration::days(31);
        store.store(item).await.unwrap();

        // Cleanup with 30 days threshold
        let removed = store.cleanup_stale(30).await;
        assert_eq!(removed, 1);
    }

    #[tokio::test]
    async fn test_disabled_learning() {
        let engine = LearningEngine::with_config(LearningConfig {
            enabled: false,
            ..Default::default()
        });

        let event = LearningEvent {
            learning_type: LearningType::UserPreference,
            context: "test".to_string(),
            key: "key".to_string(),
            value: "value".to_string(),
            feedback: Feedback::Positive,
            source: None,
        };

        let result = engine.learn(event).await;
        assert!(matches!(result, Err(LearningError::Disabled)));
    }
}
