use crate::db::PgPool;
use errors::ContextraError;
use serde_json::Value;
use sqlx::FromRow;
use types::{Collection, CollectionId, Metadata};
use uuid::Uuid;

pub struct CollectionRepository {
    pool: PgPool,
}

#[derive(FromRow)]
struct CollectionRow {
    id: Uuid,
    name: String,
    metadata: Value,
}

impl CollectionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, collection: &Collection) -> Result<(), ContextraError> {
        let metadata_json = serde_json::to_value(&collection.metadata).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize metadata: {e}"))
        })?;

        sqlx::query(
            r#"
            INSERT INTO collections (id, name, metadata)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(Uuid::from(collection.id))
        .bind(&collection.name)
        .bind(metadata_json)
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to create collection: {e}")))?;

        Ok(())
    }

    pub async fn get(&self, id: CollectionId) -> Result<Option<Collection>, ContextraError> {
        let row = sqlx::query_as::<_, CollectionRow>(
            r#"
            SELECT id, name, metadata
            FROM collections
            WHERE id = $1
            "#,
        )
        .bind(Uuid::from(id))
        .fetch_optional(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to get collection: {e}")))?;

        match row {
            Some(r) => {
                let metadata = decode_metadata(r.metadata)?;
                Ok(Some(Collection {
                    id: CollectionId::from(r.id),
                    name: r.name,
                    metadata,
                }))
            }
            None => Ok(None),
        }
    }

    /// Returns up to `limit` collections starting at `offset` (offset-based
    /// pagination).  Fetch `limit + 1` rows in callers to detect `has_more`.
    pub async fn list(&self, limit: i64, offset: i64) -> Result<Vec<Collection>, ContextraError> {
        let rows = sqlx::query_as::<_, CollectionRow>(
            r#"
            SELECT id, name, metadata
            FROM collections
            ORDER BY name, id
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to list collections: {e}")))?;

        let mut collections = Vec::with_capacity(rows.len());
        for r in rows {
            let metadata = decode_metadata(r.metadata)?;
            collections.push(Collection {
                id: CollectionId::from(r.id),
                name: r.name,
                metadata,
            });
        }
        Ok(collections)
    }
}

fn decode_metadata(value: Value) -> Result<Metadata, ContextraError> {
    serde_json::from_value(value)
        .map_err(|e| ContextraError::StorageError(format!("Failed to deserialize metadata: {e}")))
}
