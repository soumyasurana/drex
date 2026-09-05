use crate::db::PgPool;
use crate::repository::Repository;
use async_trait::async_trait;
use errors::ContextraError;
use sqlx::FromRow;
use types::{Chunk, CollectionId, Document, DocumentId};
use uuid::Uuid;

pub struct DocumentRepository {
    pool: PgPool,
}

#[derive(FromRow)]
struct ChunkRow {
    id: Uuid,
    document_id: Uuid,
    content: String,
    metadata: serde_json::Value,
}

#[derive(FromRow)]
struct DocumentRow {
    id: Uuid,
    collection_id: Uuid,
    content: String,
    metadata: serde_json::Value,
}

impl DocumentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_chunk(&self, chunk: &Chunk) -> Result<(), ContextraError> {
        let metadata_json = serde_json::to_value(&chunk.metadata).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize metadata: {}", e))
        })?;

        sqlx::query(
            r#"
            INSERT INTO chunks (id, document_id, content, metadata)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(chunk.id)
        .bind(Uuid::from(chunk.document_id))
        .bind(&chunk.content)
        .bind(metadata_json)
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to create chunk: {}", e)))?;

        Ok(())
    }

    pub async fn get_chunks_by_document(
        &self,
        document_id: &DocumentId,
    ) -> Result<Vec<Chunk>, ContextraError> {
        let records = sqlx::query_as::<_, ChunkRow>(
            r#"
            SELECT id, document_id, content, metadata
            FROM chunks
            WHERE document_id = $1
            "#,
        )
        .bind(Uuid::from(*document_id))
        .fetch_all(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to fetch chunks: {}", e)))?;

        let mut chunks = Vec::new();
        for record in records {
            let metadata = serde_json::from_value(record.metadata).map_err(|e| {
                ContextraError::StorageError(format!("Failed to deserialize metadata: {}", e))
            })?;

            chunks.push(Chunk {
                id: record.id,
                document_id: DocumentId::from(record.document_id),
                content: record.content,
                metadata,
            });
        }

        Ok(chunks)
    }

    pub async fn list(
        &self,
        collection_id: Option<CollectionId>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Document>, ContextraError> {
        let records = if let Some(cid) = collection_id {
            sqlx::query_as::<_, DocumentRow>(
                r#"
                SELECT id, collection_id, content, metadata
                FROM documents
                WHERE collection_id = $1
                ORDER BY id
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(Uuid::from(cid))
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool.inner())
            .await
        } else {
            sqlx::query_as::<_, DocumentRow>(
                r#"
                SELECT id, collection_id, content, metadata
                FROM documents
                ORDER BY id
                LIMIT $1 OFFSET $2
                "#,
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool.inner())
            .await
        }
        .map_err(|e| ContextraError::StorageError(format!("Failed to list documents: {}", e)))?;

        let mut documents = Vec::new();
        for record in records {
            let metadata = serde_json::from_value(record.metadata).map_err(|e| {
                ContextraError::StorageError(format!("Failed to deserialize metadata: {}", e))
            })?;

            documents.push(Document {
                id: DocumentId::from(record.id),
                collection_id: CollectionId::from(record.collection_id),
                content: record.content,
                metadata,
            });
        }

        Ok(documents)
    }
}

#[async_trait]
impl Repository<Document, DocumentId> for DocumentRepository {
    async fn get(&self, id: &DocumentId) -> Result<Option<Document>, ContextraError> {
        let record = sqlx::query_as::<_, DocumentRow>(
            r#"
            SELECT id, collection_id, content, metadata
            FROM documents
            WHERE id = $1
            "#,
        )
        .bind(Uuid::from(*id))
        .fetch_optional(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to get document: {}", e)))?;

        if let Some(record) = record {
            let metadata = serde_json::from_value(record.metadata).map_err(|e| {
                ContextraError::StorageError(format!("Failed to deserialize metadata: {}", e))
            })?;

            Ok(Some(Document {
                id: DocumentId::from(record.id),
                collection_id: CollectionId::from(record.collection_id),
                content: record.content,
                metadata,
            }))
        } else {
            Ok(None)
        }
    }

    async fn create(&self, entity: &Document) -> Result<(), ContextraError> {
        let metadata_json = serde_json::to_value(&entity.metadata).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize metadata: {}", e))
        })?;

        sqlx::query(
            r#"
            INSERT INTO documents (id, collection_id, content, metadata)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(Uuid::from(entity.id))
        .bind(Uuid::from(entity.collection_id))
        .bind(&entity.content)
        .bind(metadata_json)
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to create document: {}", e)))?;

        Ok(())
    }

    async fn update(&self, entity: &Document) -> Result<(), ContextraError> {
        let metadata_json = serde_json::to_value(&entity.metadata).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize metadata: {}", e))
        })?;

        let result = sqlx::query(
            r#"
            UPDATE documents
            SET collection_id = $1, content = $2, metadata = $3
            WHERE id = $4
            "#,
        )
        .bind(Uuid::from(entity.collection_id))
        .bind(&entity.content)
        .bind(metadata_json)
        .bind(Uuid::from(entity.id))
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to update document: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(ContextraError::NotFound(format!(
                "Document {} not found",
                entity.id
            )));
        }

        Ok(())
    }

    async fn delete(&self, id: &DocumentId) -> Result<(), ContextraError> {
        let result = sqlx::query(
            r#"
            DELETE FROM documents
            WHERE id = $1
            "#,
        )
        .bind(Uuid::from(*id))
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to delete document: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(ContextraError::NotFound(format!(
                "Document {} not found",
                id
            )));
        }

        Ok(())
    }
}
