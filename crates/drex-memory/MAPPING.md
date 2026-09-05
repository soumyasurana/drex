# Drex Memory to Contextra Mapping

This document details the complete mapping between Drex's memory abstraction and Contextra's storage infrastructure.

## Concept Mapping

### Memory Kinds

| Drex `MemoryKind`      | Contextra `LongTermMemoryKind` | Storage Backend | Notes                             |
|-----------------------|-------------------------------|-----------------|-----------------------------------|
| `Working`             | N/A                           | Redis           | Session cache, ephemeral          |
| `Episodic`            | N/A                           | PostgreSQL      | Message history via `ConversationRepository` |
| `Semantic`            | `Fact`                        | Qdrant          | Vector-embedded facts             |
| `Preference`          | `Preference`                  | Qdrant          | User preferences                  |
| `Procedural`          | N/A                           | ???             | Not natively supported            |
| `Relationship`        | N/A                           | ???             | Not natively supported            |
| `Summary`             | `Summary`                     | Qdrant          | Condensed conversation summaries  |

### Storage Operations

| Drex Operation        | Contextra Equivalent          | Status | Notes                             |
|----------------------|-------------------------------|--------|-----------------------------------|
| `MemoryStore::store`  | `MemoryStore::remember`       | ✓      | Maps cleanly                      |
| `MemoryStore::retrieve` | `MemoryStore::recall`     | ✓      | Maps cleanly                      |
| `MemoryStore::get`    | —                             | ✓      | Default impl, no Contextra equiv |
| `MemoryStore::forget` | **NOT AVAILABLE**             | ✗      | See below                         |
| `MemoryStore::update` | **NOT AVAILABLE**             | ✗      | See below                         |

## ⚠️ Critical: Deletion Support Gap

**The Most Important Fact About Contextra Integration**

Contextra's `MemoryStore` trait (defined in `libs/memory/src/lib.rs`, line 484) **only exposes two methods**:

```rust
#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn remember(&self, memory: LongTermMemory) -> Result<(), ContextraError>;

    async fn recall(
        &self,
        user_id: UserId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<LongTermMemory>, ContextraError>;
}
```

**THERE IS NO `delete` METHOD. THERE IS NO `update` METHOD.**

### What Contextra DOES Support

The underlying storage layer **does** support deletion:

| Repository/Store      | Deletion Method                  | Backend         | Works? |
|----------------------|----------------------------------|-----------------|--------|
| `DocumentRepository` | `delete(&self, id)`              | PostgreSQL      | ✓ Hard delete via `DELETE FROM` |
| `ConversationRepository`| `delete_message(&self, id)`     | PostgreSQL      | ✓ Hard delete via `DELETE FROM` |
| `Cache`              | `delete(&self, key)`             | Redis           | ✓ Redis `DEL` command |
| `VectorStore` trait  | `delete_by_id(&self, ids)`      | Qdrant          | ✓ Qdrant `delete_points` |
| `Repository` trait   | `delete(&self, id)`              | Generic         | ✓ Generic CRUD delete |

### Why This Matters

1. **Contextra MemoryStore is a wrapper around VectorStore**, but it **does not expose** the `delete_by_id` method.

2. **Drex's `MemoryStore` contract includes `forget` and `update`**, which are critical for:
   - User privacy requests ("delete everything about X")
   - Memory correction ("actually, I don't prefer that anymore")
   - Data retention policies (GDPR, CCPA)

3. **The gap must be documented**, not hidden.

### Drex's Honest Approach

```rust
// When backed by Contextra's MemoryStore:

impl MemoryStore for ContextraMemoryStore {
    async fn forget(&self, id: MemoryId) -> Result<()> {
        // Contextra's MemoryStore trait doesn't expose delete
        Err(MemoryStoreError::UnsupportedOperation(
            "Contextra MemoryStore does not support deletion. ".to_string() +
            "Use VectorStore backend directly for delete support."
        ))
    }

    async fn update(&self, id: MemoryId, patch: MemoryPatch) -> Result<Memory> {
        // To update, we'd need to delete + re-insert, but delete isn't exposed
        Err(MemoryStoreError::UnsupportedOperation(
            "Contextra MemoryStore does not support updates".to_string()
        ))
    }
}
```

This is **intentional** - rather than silently failing or pretending to work, we make the limitation explicit.

## Future Path to Full Support

To support delete/update, Drex can:

1. **Bypass Contextra MemoryStore**, use `VectorStore` directly:
   ```rust
   // Use storage::vector_store::VectorStore instead
   vector_store.delete_by_id("long-term-memory", &[id]).await?;
   ```

2. **Extend Contextra's MemoryStore trait** with delete/update methods

3. **Use PostgreSQL rows** as source of truth, use Qdrant only for vector search

4. **Soft delete** in metadata, filter out in recall (partial workaround)

## Metadata Mapping

| Drex Field               | Contextra Field      | Notes                           |
|-------------------------|---------------------|---------------------------------|
| `MemoryMetadata::user_id` | `LongTermMemory::user_id` | Direct mapping |
| `MemoryMetadata::created_at` | N/A (in ID)      | UUID v7 embeds timestamp        |
| `MemoryMetadata::source` | Vertex in payload     | Serialized as string            |
| `MemoryMetadata::confidence` | N/A              | Drex-specific                   |
| `MemoryMetadata::sensitivity` | N/A             | Drex-specific                   |
| `MemoryImportance`      | `LongTermMemory::importance` | Direct mapping |

## Validation Checklist

When implementing a Contextra backend:

- [ ] `store()` maps to `remember()` correctly
- [ ] `retrieve()` maps to `recall()` correctly  
- [ ] `forget()` returns `UnsupportedOperation` with clear message
- [ ] `update()` returns `UnsupportedOperation` with clear message
- [ ] Memory kinds map appropriately
- [ ] User isolation is maintained
- [ ] Importance scores translate correctly

## Testing Recommendations

```rust
#[tokio::test]
async fn test_contextra_forget_unsupported() {
    let store = ContextraMemoryStore::new(/* ... */);
    let id = MemoryId::new();

    let result = store.forget(id).await;
    assert!(matches!(
        result,
        Err(MemoryStoreError::UnsupportedOperation(_))
    ));
}
```
