use async_trait::async_trait;
use thiserror::Error;

use crate::memory::{Memory, MemoryId, MemoryPatch};
use crate::query::MemoryQuery;

/// Errors that can occur when interacting with a memory store.
#[derive(Debug, Error)]
pub enum MemoryStoreError {
    /// The requested memory was not found.
    #[error("Memory not found: {0}")]
    NotFound(MemoryId),

    /// The store backend is unavailable or experiencing issues.
    #[error("Storage backend error: {0}")]
    StorageError(String),

    /// Invalid operation parameters.
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// The operation is not supported by this store implementation.
    #[error("Operation not supported: {0}")]
    UnsupportedOperation(String),

    /// Multiple memories matched when exactly one was expected.
    #[error("Expected single memory but found {count} matching query")]
    MultipleMatches { count: usize },

    /// A conflict occurred (e.g., duplicate ID on insert).
    #[error("Conflict: {0}")]
    Conflict(String),
}

/// Result type alias for memory store operations.
pub type Result<T> = std::result::Result<T, MemoryStoreError>;

/// A backend-agnostic memory storage interface.
///
/// Implementations may delegate to various backends:
/// - PostgreSQL for structured storage
/// - Redis for working/session memory
/// - Qdrant for vector-semantic search
/// - Contextra's MemoryStore for long-term vector memory
///
/// ## Contract
///
/// - `store`: Persist a memory, returning its ID.
/// - `retrieve`: Query memories, returning matching entries.
/// - `forget`: Permanently delete a memory by ID.
/// - `update`: Modify specific fields of an existing memory.
///
/// ## Deletion Support ⚠️
///
/// **CRITICAL**: Not all backends support true deletion:
///
/// | Backend              | store | retrieve | forget | update |
/// |---------------------|-------|----------|--------|--------|
/// | PostgreSQL (direct) | ✓     | ✓        | ✓      | ✓      |
/// | Redis (direct)        | ✓     | ✓        | ✓      | ✓*     |
/// | Qdrant (direct)       | ✓     | ✓        | ✓      | N/A    |
/// | Contextra MemoryStore | ✓     | ✓        | ✗      | ✗      |
///
/// *Redis update is really delete + recreate
///
/// Contextra's `MemoryStore` trait only exposes `remember()` and `recall()`.
/// Implementations using Contextra **MUST** return `UnsupportedOperation` for
/// `forget` and `update` operations to honestly represent this limitation.
///
/// Future implementations that bypass Contextra's MemoryStore (using VectorStore
/// directly) CAN implement full deletion support.
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Store a new memory in the backend.
    ///
    /// # Arguments
    /// * `memory` - The memory to store
    ///
    /// # Returns
    /// The `MemoryId` assigned to the stored memory. This may be the same as
    /// `memory.id` if pre-assigned, or a newly generated ID.
    ///
    /// # Errors
    /// - `StorageError` if the backend is unavailable
    /// - `Conflict` if a memory with the same ID already exists
    async fn store(&self, memory: Memory) -> Result<MemoryId>;

    /// Retrieve memories matching the query.
    ///
    /// # Arguments
    /// * `query` - Query parameters for filtering and searching
    ///
    /// # Returns
    /// A vector of matching memories, ordered by relevance if semantic search
    /// is being performed, otherwise by recency.
    ///
    /// # Errors
    /// - `StorageError` if the backend is unavailable
    /// - `InvalidOperation` if the query parameters are invalid
    async fn retrieve(&self, query: &MemoryQuery) -> Result<Vec<Memory>>;

    /// Retrieve a single memory by ID.
    ///
    /// Convenience method that wraps `retrieve` with an ID query.
    async fn get(&self, id: MemoryId) -> Result<Option<Memory>> {
        let query = MemoryQuery::by_id(id);
        let results = self.retrieve(&query).await?;
        Ok(results.into_iter().next())
    }

    /// Permanently delete a memory by ID.
    ///
    /// # Arguments
    /// * `id` - The ID of the memory to delete
    ///
    /// # Errors
    /// - `NotFound` if the memory doesn't exist
    /// - `UnsupportedOperation` if the backend doesn't support deletion
    /// - `StorageError` if the backend operation failed
    ///
    /// ## Implementation Note
    ///
    /// This is a **hard delete** - the memory is permanently removed.
    /// Some backends may not support this operation (see trait-level docs).
    async fn forget(&self, id: MemoryId) -> Result<()>;

    /// Update specific fields of an existing memory.
    ///
    /// # Arguments
    /// * `id` - The ID of the memory to update
    /// * `patch` - The fields to update (None = no change)
    ///
    /// # Errors
    /// - `NotFound` if the memory doesn't exist
    /// - `UnsupportedOperation` if the backend doesn't support updates
    /// - `StorageError` if the backend operation failed
    ///
    /// ## Implementation Note
    ///
    /// Some backends (like vector stores) may not support partial updates.
    /// In those cases, implementations may need to:
    /// 1. Delete the old record
    /// 2. Create a new record with merged data
    /// 3. Return a potentially new MemoryId
    ///
    /// Or simply return `UnsupportedOperation`.
    async fn update(&self, id: MemoryId, patch: MemoryPatch) -> Result<Memory>;
}

#[cfg(test)]
mod tests {
    use crate::memory::{MemoryKind, MemoryMetadata};

    use super::*;

    #[test]
    fn error_display_not_found() {
        let id = MemoryId::new();
        let err = MemoryStoreError::NotFound(id);
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn error_display_unsupported() {
        let err = MemoryStoreError::UnsupportedOperation("delete not available".to_string());
        assert!(err.to_string().contains("not supported"));
    }

    #[test]
    fn error_display_multiple() {
        let err = MemoryStoreError::MultipleMatches { count: 5 };
        assert!(err.to_string().contains("5 matching"));
    }

    // Mock implementation for testing trait behavior
    #[allow(dead_code)]
    struct MockStore;

    #[async_trait]
    impl MemoryStore for MockStore {
        async fn store(&self, memory: Memory) -> Result<MemoryId> {
            Ok(memory.id)
        }

        async fn retrieve(&self, query: &MemoryQuery) -> Result<Vec<Memory>> {
            let _ = query;
            Ok(vec![])
        }

        async fn forget(&self, _id: MemoryId) -> Result<()> {
            Err(MemoryStoreError::UnsupportedOperation(
                "mock store does not support deletion".to_string(),
            ))
        }

        async fn update(&self, _id: MemoryId, _patch: MemoryPatch) -> Result<Memory> {
            Err(MemoryStoreError::UnsupportedOperation(
                "mock store does not support updates".to_string(),
            ))
        }
    }

    #[tokio::test]
    async fn trait_default_get_returns_none_when_empty() {
        struct EmptyStore;

        #[async_trait]
        impl MemoryStore for EmptyStore {
            async fn store(&self, _memory: Memory) -> Result<MemoryId> {
                unimplemented!()
            }

            async fn retrieve(&self, _query: &MemoryQuery) -> Result<Vec<Memory>> {
                Ok(vec![])
            }

            async fn forget(&self, _id: MemoryId) -> Result<()> {
                unimplemented!()
            }

            async fn update(&self, _id: MemoryId, _patch: MemoryPatch) -> Result<Memory> {
                unimplemented!()
            }
        }

        let store = EmptyStore;
        let id = MemoryId::new();
        let result = store.get(id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn trait_default_get_returns_single_match() {
        struct SingleStore;

        #[async_trait]
        impl MemoryStore for SingleStore {
            async fn store(&self, _memory: Memory) -> Result<MemoryId> {
                unimplemented!()
            }

            async fn retrieve(&self, _query: &MemoryQuery) -> Result<Vec<Memory>> {
                Ok(vec![
                    Memory::new(MemoryKind::Semantic, "test memory")
                        .with_metadata(MemoryMetadata::automatic("user-1")),
                ])
            }

            async fn forget(&self, _id: MemoryId) -> Result<()> {
                unimplemented!()
            }

            async fn update(&self, _id: MemoryId, _patch: MemoryPatch) -> Result<Memory> {
                unimplemented!()
            }
        }

        let store = SingleStore;
        let id = MemoryId::new();
        let result = store.get(id).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, "test memory");
    }
}
