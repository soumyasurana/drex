use crate::db::PgPool;
use crate::repository::Repository;
use async_trait::async_trait;
use errors::ContextraError;
use sqlx::FromRow;
use types::{ConversationId, Message, Metadata, Role};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct ConversationRecord {
    pub id: ConversationId,
    pub title: Option<String>,
    pub metadata: Metadata,
}

pub struct ConversationRepository {
    pool: PgPool,
}

#[derive(FromRow)]
struct MessageRow {
    id: Uuid,
    conversation_id: Uuid,
    role: String,
    content: String,
    metadata: serde_json::Value,
}

impl ConversationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create_conversation(&self, id: &ConversationId) -> Result<(), ContextraError> {
        sqlx::query(
            r#"
            INSERT INTO conversations (id)
            VALUES ($1)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(Uuid::from(*id))
        .execute(self.pool.inner())
        .await
        .map_err(|e| {
            ContextraError::StorageError(format!("Failed to create conversation: {}", e))
        })?;

        Ok(())
    }

    pub async fn create_conversation_with_details(
        &self,
        id: &ConversationId,
        title: Option<&str>,
        metadata: &Metadata,
    ) -> Result<(), ContextraError> {
        let metadata_json = serde_json::to_value(metadata).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize metadata: {e}"))
        })?;

        sqlx::query(
            r#"
            INSERT INTO conversations (id, title, metadata)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO UPDATE
            SET title = EXCLUDED.title, metadata = EXCLUDED.metadata
            "#,
        )
        .bind(Uuid::from(*id))
        .bind(title)
        .bind(metadata_json)
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to create conversation: {e}")))?;

        Ok(())
    }

    pub async fn get_conversation(
        &self,
        id: &ConversationId,
    ) -> Result<Option<ConversationRecord>, ContextraError> {
        #[derive(FromRow)]
        struct ConvRow {
            id: Uuid,
            title: Option<String>,
            metadata: serde_json::Value,
        }

        let row = sqlx::query_as::<_, ConvRow>(
            r#"
            SELECT id, title, metadata
            FROM conversations
            WHERE id = $1
            "#,
        )
        .bind(Uuid::from(*id))
        .fetch_optional(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to get conversation: {e}")))?;

        match row {
            Some(r) => {
                let metadata = serde_json::from_value(r.metadata).map_err(|e| {
                    ContextraError::StorageError(format!("Failed to deserialize metadata: {e}"))
                })?;
                Ok(Some(ConversationRecord {
                    id: ConversationId::from(r.id),
                    title: r.title,
                    metadata,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn list_conversations(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ConversationRecord>, ContextraError> {
        #[derive(FromRow)]
        struct ConvRow {
            id: Uuid,
            title: Option<String>,
            metadata: serde_json::Value,
        }

        let rows = sqlx::query_as::<_, ConvRow>(
            r#"
            SELECT id, title, metadata
            FROM conversations
            ORDER BY id DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to list conversations: {e}")))?;

        let mut convs = Vec::with_capacity(rows.len());
        for r in rows {
            let metadata = serde_json::from_value(r.metadata).map_err(|e| {
                ContextraError::StorageError(format!("Failed to deserialize metadata: {e}"))
            })?;
            convs.push(ConversationRecord {
                id: ConversationId::from(r.id),
                title: r.title,
                metadata,
            });
        }

        Ok(convs)
    }

    pub async fn get_messages_by_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Vec<Message>, ContextraError> {
        let records = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT id, conversation_id, role, content, metadata
            FROM messages
            WHERE conversation_id = $1
            ORDER BY id ASC
            "#,
        )
        .bind(Uuid::from(*conversation_id))
        .fetch_all(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to fetch messages: {}", e)))?;

        let mut messages = Vec::new();
        for record in records {
            let metadata = serde_json::from_value(record.metadata).map_err(|e| {
                ContextraError::StorageError(format!("Failed to deserialize metadata: {}", e))
            })?;

            let role_str = format!("\"{}\"", record.role);
            let role: Role = serde_json::from_str(&role_str).map_err(|e| {
                ContextraError::StorageError(format!("Failed to deserialize role: {}", e))
            })?;

            messages.push(Message {
                id: record.id,
                conversation_id: ConversationId::from(record.conversation_id),
                role,
                content: record.content,
                metadata,
            });
        }

        Ok(messages)
    }
}

// Implement Repository for Messages
#[async_trait]
impl Repository<Message, Uuid> for ConversationRepository {
    async fn get(&self, id: &Uuid) -> Result<Option<Message>, ContextraError> {
        let record = sqlx::query_as::<_, MessageRow>(
            r#"
            SELECT id, conversation_id, role, content, metadata
            FROM messages
            WHERE id = $1
            "#,
        )
        .bind(*id)
        .fetch_optional(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to get message: {}", e)))?;

        if let Some(record) = record {
            let metadata = serde_json::from_value(record.metadata).map_err(|e| {
                ContextraError::StorageError(format!("Failed to deserialize metadata: {}", e))
            })?;

            let role_str = format!("\"{}\"", record.role);
            let role: Role = serde_json::from_str(&role_str).map_err(|e| {
                ContextraError::StorageError(format!("Failed to deserialize role: {}", e))
            })?;

            Ok(Some(Message {
                id: record.id,
                conversation_id: ConversationId::from(record.conversation_id),
                role,
                content: record.content,
                metadata,
            }))
        } else {
            Ok(None)
        }
    }

    async fn create(&self, entity: &Message) -> Result<(), ContextraError> {
        let metadata_json = serde_json::to_value(&entity.metadata).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize metadata: {}", e))
        })?;

        let role_json = serde_json::to_string(&entity.role).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize role: {}", e))
        })?;
        let role_str = role_json.trim_matches('"');

        sqlx::query(
            r#"
            INSERT INTO messages (id, conversation_id, role, content, metadata)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(entity.id)
        .bind(Uuid::from(entity.conversation_id))
        .bind(role_str)
        .bind(&entity.content)
        .bind(metadata_json)
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to create message: {}", e)))?;

        Ok(())
    }

    async fn update(&self, entity: &Message) -> Result<(), ContextraError> {
        let metadata_json = serde_json::to_value(&entity.metadata).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize metadata: {}", e))
        })?;

        let role_json = serde_json::to_string(&entity.role).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize role: {}", e))
        })?;
        let role_str = role_json.trim_matches('"');

        let result = sqlx::query(
            r#"
            UPDATE messages
            SET conversation_id = $1, role = $2, content = $3, metadata = $4
            WHERE id = $5
            "#,
        )
        .bind(Uuid::from(entity.conversation_id))
        .bind(role_str)
        .bind(&entity.content)
        .bind(metadata_json)
        .bind(entity.id)
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to update message: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(ContextraError::NotFound(format!(
                "Message {} not found",
                entity.id
            )));
        }

        Ok(())
    }

    async fn delete(&self, id: &Uuid) -> Result<(), ContextraError> {
        let result = sqlx::query(
            r#"
            DELETE FROM messages
            WHERE id = $1
            "#,
        )
        .bind(*id)
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to delete message: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(ContextraError::NotFound(format!(
                "Message {} not found",
                id
            )));
        }

        Ok(())
    }
}
