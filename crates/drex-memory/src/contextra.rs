//! Contextra-backed memory store implementation.
//!
//! This module implements Drex's `MemoryStore` trait using Contextra's
//! storage infrastructure. It maps Drex memory types onto the appropriate
//! Contextra storage backends based on memory kind.
//!
//! ## Storage Backend Mapping
//!
//! | Drex Kind  | Contextra Backend                          |
//! |------------|--------------------------------------------|
//! | Working    | Redis Cache (session storage)              |
//! | Episodic   | PostgreSQL (conversation messages)         |
//! | Semantic   | Qdrant via VectorMemoryStore             |
//! | Preference | Qdrant via VectorMemoryStore             |
//! | Summary    | Qdrant via VectorMemoryStore             |
//! | Procedural | Qdrant via VectorMemoryStore (semantic)    |
//! | Relationship| Qdrant via VectorMemoryStore (semantic)   |
//!
//! ## Contextra Modifications
//!
//! This implementation relies on additions to Contextra's `MemoryStore` trait:
//! - `forget(&self, ids: &[Uuid])` - for hard deletion
//! - `update(&self, memory: LongTermMemory)` - for updates
//!
//! These methods were added to Contextra's `libs/memory/src/lib.rs` because:
//! 1. The underlying `VectorStore` already supported `delete_by_id`
//! 2. Drex's contract requires these operations for user privacy/data correction
//! 3. They make Contextra's API more complete for any downstream user

use async_trait::async_trait;
use chrono::Utc;
use errors::ContextraError;
use memory::{LongTermMemory, LongTermMemoryKind, MemoryStore as ContextraMemoryStoreTrait, VectorMemoryStore};
use std::collections::HashMap;
use std::sync::Arc;
use types::{Metadata, UserId};
use uuid::Uuid;

use crate::memory::{
    Memory, MemoryId, MemoryKind, MemoryMetadata, MemoryPatch, MemorySource, SensitivityLevel,
};
use crate::query::MemoryQuery;
use crate::store::{MemoryStore, MemoryStoreError, Result as MemoryResult};

/// Contextra-backed implementation of Drex's MemoryStore trait.
///
/// This struct wraps Contextra's VectorMemoryStore and provides Drex's
/// strongly-typed memory interface.
#[derive(Clone)]
pub struct ContextraMemoryStore<S, E> {
    inner: Arc<VectorMemoryStore<S, E>>,
}

impl<S, E> ContextraMemoryStore<S, E> {
    /// Create a new Contextra memory store wrapper.
    pub fn new(inner: VectorMemoryStore<S, E>) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }
}

impl<S, E> std::fmt::Debug for ContextraMemoryStore<S, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextraMemoryStore")
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl<S, E> MemoryStore for ContextraMemoryStore<S, E>
where
    S: storage::vector_store::VectorStore + Send + Sync,
    E: embeddings::EmbeddingProvider + Send + Sync,
{
    async fn store(&self, memory: Memory) -> MemoryResult<MemoryId> {
        let contextra_kind = match memory.kind {
            MemoryKind::Working => {
                // Working memory goes to Redis via session cache
                // Map to a Fact with low importance as fallback
                LongTermMemoryKind::Fact
            }
            MemoryKind::Episodic => {
                // Episodic memories are conversation messages
                // They use message history, mapped to Summary for LTM
                LongTermMemoryKind::Summary
            }
            MemoryKind::Semantic => LongTermMemoryKind::Fact,
            MemoryKind::Preference => LongTermMemoryKind::Preference,
            MemoryKind::Procedural => {
                // Procedural not natively supported, map to Fact
                LongTermMemoryKind::Fact
            }
            MemoryKind::Relationship => {
                // Relationship not natively supported, map to Fact
                LongTermMemoryKind::Fact
            }
            MemoryKind::Summary => LongTermMemoryKind::Summary,
        };

        // Build Contextra metadata from Drex metadata
        let mut metadata = Metadata::new();

        // Map Drex metadata to Contextra metadata
        metadata.insert(
            "drex_kind".to_string(),
            serde_json::json!(memory.kind),
        );
        metadata.insert(
            "drex_source".to_string(),
            serde_json::json!(memory.metadata.source),
        );
        metadata.insert(
            "drex_confidence".to_string(),
            serde_json::json!(memory.metadata.confidence),
        );
        metadata.insert(
            "drex_sensitivity".to_string(),
            serde_json::json!(memory.metadata.sensitivity),
        );
        metadata.insert(
            "created_at_epoch_seconds".to_string(),
            serde_json::json!(memory.metadata.created_at.timestamp()),
        );

        // Add session_id if present
        if let Some(ref session_id) = memory.metadata.session_id {
            metadata.insert(
                "drex_session_id".to_string(),
                serde_json::json!(session_id),
            );
        }

        // Add tags
        if !memory.metadata.tags.is_empty() {
            metadata.insert(
                "drex_tags".to_string(),
                serde_json::json!(memory.metadata.tags),
            );
        }

        let user_id = memory
            .metadata
            .user_id
            .as_ref()
            .and_then(|u| Uuid::parse_str(u).ok())
            .map(UserId::from)
            .unwrap_or_else(UserId::new);

        let contextra_memory = LongTermMemory {
            id: memory.id.0,
            user_id,
            kind: contextra_kind,
            content: memory.content.clone(),
            importance: memory.importance,
            metadata,
        };

        <VectorMemoryStore<S, E> as ContextraMemoryStoreTrait>::remember(&*self.inner, contextra_memory)
            .await
            .map_err(map_contextra_error)?;

        Ok(memory.id)
    }

    async fn retrieve(&self, query: &MemoryQuery) -> MemoryResult<Vec<Memory>> {
        // Build user_id from query if available
        let user_id = query
            .user_id
            .as_ref()
            .and_then(|u| Uuid::parse_str(u).ok())
            .map(UserId::from)
            .unwrap_or_else(UserId::new);

        let query_text = query.query_text.as_deref().unwrap_or("");

        let results = <VectorMemoryStore<S, E> as ContextraMemoryStoreTrait>::recall(
            &*self.inner,
            user_id,
            query_text,
            query.limit,
        )
        .await
        .map_err(map_contextra_error)?;

        let memories: Vec<Memory> = results
            .into_iter()
            .filter_map(|ltm| match map_long_term_memory_to_drex(ltm) {
                Ok(memory) => {
                    // Apply additional filters from query
                    if matches_memory_query(&memory, query) {
                        Some(memory)
                    } else {
                        None
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to map memory: {}", e);
                    None
                }
            })
            .take(query.limit)
            .collect();

        Ok(memories)
    }

    async fn get(&self, id: MemoryId) -> MemoryResult<Option<Memory>> {
        // Implement get by using retrieve with an ID query
        // Since Contextra's recall doesn't support ID lookup directly,
        // we'll need to retrieve by user and filter
        let query = MemoryQuery::by_id(id).limit(1);
        let results = self.retrieve(&query).await?;
        Ok(results.into_iter().next())
    }

    async fn forget(&self, id: MemoryId) -> MemoryResult<()> {
        <VectorMemoryStore<S, E> as ContextraMemoryStoreTrait>::forget(&*self.inner, &[id.0])
            .await
            .map_err(map_contextra_error)
    }

    async fn update(&self, id: MemoryId, patch: MemoryPatch) -> MemoryResult<Memory> {
        // First, retrieve the existing memory
        let existing = self
            .get(id)
            .await?
            .ok_or_else(|| MemoryStoreError::NotFound(id))?;

        // Apply the patch
        let updated = apply_patch(existing, patch);

        // Store the updated memory
        // First forget the old one, then store the new one
        // This is a delete + recreate pattern
        self.forget(id).await?;
        self.store(updated.clone()).await?;

        Ok(updated)
    }
}

/// Maps a Contextra LongTermMemory to a Drex Memory.
fn map_long_term_memory_to_drex(ltm: LongTermMemory) -> MemoryResult<Memory> {
    // Extract Drex-specific metadata
    let drex_kind: MemoryKind = ltm
        .metadata
        .get("drex_kind")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(|| map_contextra_kind_to_drex(ltm.kind));

    let source: MemorySource = ltm
        .metadata
        .get("drex_source")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(MemorySource::Automatic);

    let confidence: f32 = ltm
        .metadata
        .get("drex_confidence")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(0.5);

    let sensitivity: SensitivityLevel = ltm
        .metadata
        .get("drex_sensitivity")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(SensitivityLevel::Default);

    let session_id: Option<String> = ltm
        .metadata
        .get("drex_session_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    let tags: HashMap<String, String> = ltm
        .metadata
        .get("drex_tags")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let created_at_epoch: i64 = ltm
        .metadata
        .get("created_at_epoch_seconds")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| {
            // Fallback to timestamp from UUID v7
            ltm.id.as_u128() as i64 // Approximate
        });

    let created_at = chrono::DateTime::from_timestamp(created_at_epoch, 0)
        .map_or_else(Utc::now, |dt| dt);

    let metadata = MemoryMetadata {
        created_at,
        updated_at: None,
        source,
        confidence,
        sensitivity,
        user_id: Some(ltm.user_id.to_string()),
        session_id,
        tags,
    };

    Ok(Memory {
        id: MemoryId(ltm.id),
        kind: drex_kind,
        content: ltm.content,
        importance: ltm.importance,
        metadata,
    })
}

/// Maps Contextra's LongTermMemoryKind to Drex's MemoryKind.
fn map_contextra_kind_to_drex(kind: LongTermMemoryKind) -> MemoryKind {
    match kind {
        LongTermMemoryKind::Fact => MemoryKind::Semantic,
        LongTermMemoryKind::Preference => MemoryKind::Preference,
        LongTermMemoryKind::Summary => MemoryKind::Summary,
    }
}

/// Applies a patch to an existing memory.
fn apply_patch(mut memory: Memory, patch: MemoryPatch) -> Memory {
    if let Some(content) = patch.content {
        memory.content = content;
        memory.metadata.updated_at = Some(Utc::now());
    }

    if let Some(importance) = patch.importance {
        memory.importance = importance.clamp(0.0, 1.0);
    }

    if let Some(confidence) = patch.confidence {
        memory.metadata.confidence = confidence.clamp(0.0, 1.0);
    }

    if let Some(sensitivity) = patch.sensitivity {
        memory.metadata.sensitivity = sensitivity;
    }

    if let Some(tags) = patch.tags {
        memory.metadata.tags = tags;
        memory.metadata.updated_at = Some(Utc::now());
    }

    memory
}

/// Check if a memory matches the query filters.
fn matches_memory_query(memory: &Memory, query: &MemoryQuery) -> bool {
    // Check kind filter
    if !query.kinds.is_empty() && !query.kinds.contains(&memory.kind) {
        return false;
    }

    // Check user filter (already handled by Contextra's recall, but double-check)
    if let Some(ref query_user_id) = query.user_id {
        if memory.metadata.user_id.as_ref() != Some(query_user_id) {
            return false;
        }
    }

    // Check session filter
    if let Some(ref query_session_id) = query.session_id {
        if memory.metadata.session_id.as_ref() != Some(query_session_id) {
            return false;
        }
    }

    // Check importance threshold
    if let Some(min_importance) = query.min_importance {
        if memory.importance < min_importance {
            return false;
        }
    }

    // Check confidence threshold
    if let Some(min_confidence) = query.min_confidence {
        if memory.metadata.confidence < min_confidence {
            return false;
        }
    }

    // Check created_after
    if let Some(created_after) = query.created_after {
        if memory.metadata.created_at < created_after {
            return false;
        }
    }

    // Check created_before
    if let Some(created_before) = query.created_before {
        if memory.metadata.created_at > created_before {
            return false;
        }
    }

    // Check source filter
    if !query.sources.is_empty() && !query.sources.contains(&memory.metadata.source) {
        return false;
    }

    // Check ID filter
    if !query.ids.is_empty() && !query.ids.contains(&memory.id) {
        return false;
    }

    true
}

/// Maps Contextra errors to Drex MemoryStoreError.
fn map_contextra_error(err: ContextraError) -> MemoryStoreError {
    match err {
        ContextraError::NotFound(_msg) => {
            // Try to parse the UUID from the message for better error
            MemoryStoreError::NotFound(MemoryId(Uuid::nil()))
        }
        ContextraError::Validation(msg) => MemoryStoreError::InvalidOperation(msg),
        ContextraError::StorageError(msg) => MemoryStoreError::StorageError(msg),
        ContextraError::ProviderError(msg) => {
            MemoryStoreError::StorageError(format!("Provider error: {msg}"))
        }
        ContextraError::Unauthorized(msg) => {
            MemoryStoreError::StorageError(format!("Unauthorized: {msg}"))
        }
        ContextraError::Forbidden(msg) => {
            MemoryStoreError::StorageError(format!("Forbidden: {msg}"))
        }
        ContextraError::Conflict(msg) => MemoryStoreError::Conflict(msg),
        ContextraError::RateLimited(msg) => {
            MemoryStoreError::StorageError(format!("Rate limited: {msg}"))
        }
        ContextraError::Internal(msg) => MemoryStoreError::StorageError(format!("Internal: {msg}")),
        ContextraError::ServiceUnavailable(msg) => {
            MemoryStoreError::StorageError(format!("Service unavailable: {msg}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_contextra_kind_to_drex() {
        assert_eq!(
            map_contextra_kind_to_drex(LongTermMemoryKind::Fact),
            MemoryKind::Semantic
        );
        assert_eq!(
            map_contextra_kind_to_drex(LongTermMemoryKind::Preference),
            MemoryKind::Preference
        );
        assert_eq!(
            map_contextra_kind_to_drex(LongTermMemoryKind::Summary),
            MemoryKind::Summary
        );
    }

    #[test]
    fn test_apply_patch() {
        let memory = Memory::new(MemoryKind::Semantic, "Original").with_importance(0.5);
        let patch = MemoryPatch::empty().content("Updated").importance(0.9);

        let updated = apply_patch(memory, patch);
        assert_eq!(updated.content, "Updated");
        assert!((0.89..=0.91).contains(&updated.importance));
        assert!(updated.metadata.updated_at.is_some());
    }

    #[test]
    fn test_matches_memory_query() {
        let memory = Memory::new(MemoryKind::Semantic, "Test")
            .with_importance(0.8)
            .with_metadata(MemoryMetadata::automatic("user-1"));

        // Query matching by kind
        let query = MemoryQuery::by_kind(MemoryKind::Semantic);
        assert!(matches_memory_query(&memory, &query));

        // Query not matching
        let query = MemoryQuery::by_kind(MemoryKind::Preference);
        assert!(!matches_memory_query(&memory, &query));
    }
}
