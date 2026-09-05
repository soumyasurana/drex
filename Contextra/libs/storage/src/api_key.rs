use crate::db::PgPool;
use async_trait::async_trait;
use errors::ContextraError;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use types::{OrgId, UserId};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub key_id: String,
    pub key_hash: String,
    pub user_id: UserId,
    pub org_id: OrgId,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[async_trait]
pub trait ApiKeyStore: Send + Sync {
    async fn find_by_key_id(&self, key_id: &str) -> Result<Option<ApiKeyRecord>, ContextraError>;
}

pub struct PgApiKeyStore {
    pool: PgPool,
}

#[derive(FromRow)]
struct ApiKeyRow {
    key_id: String,
    key_hash: String,
    user_id: Uuid,
    org_id: Uuid,
    scopes: serde_json::Value,
}

impl PgApiKeyStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, record: &ApiKeyRecord) -> Result<(), ContextraError> {
        let scopes = serde_json::to_value(&record.scopes).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize API key scopes: {e}"))
        })?;

        sqlx::query(
            r#"
            INSERT INTO api_keys (key_id, key_hash, user_id, org_id, scopes)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (key_id) DO UPDATE
            SET key_hash = EXCLUDED.key_hash,
                user_id = EXCLUDED.user_id,
                org_id = EXCLUDED.org_id,
                scopes = EXCLUDED.scopes
            "#,
        )
        .bind(&record.key_id)
        .bind(&record.key_hash)
        .bind(Uuid::from(record.user_id))
        .bind(Uuid::from(record.org_id))
        .bind(scopes)
        .execute(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to persist API key: {e}")))?;

        Ok(())
    }
}

#[async_trait]
impl ApiKeyStore for PgApiKeyStore {
    async fn find_by_key_id(&self, key_id: &str) -> Result<Option<ApiKeyRecord>, ContextraError> {
        let row = sqlx::query_as::<_, ApiKeyRow>(
            r#"
            SELECT key_id, key_hash, user_id, org_id, scopes
            FROM api_keys
            WHERE key_id = $1
            "#,
        )
        .bind(key_id)
        .fetch_optional(self.pool.inner())
        .await
        .map_err(|e| ContextraError::StorageError(format!("Failed to fetch API key: {e}")))?;

        match row {
            Some(row) => {
                let scopes = serde_json::from_value(row.scopes).map_err(|e| {
                    ContextraError::StorageError(format!(
                        "Failed to deserialize API key scopes: {e}"
                    ))
                })?;

                Ok(Some(ApiKeyRecord {
                    key_id: row.key_id,
                    key_hash: row.key_hash,
                    user_id: UserId::from(row.user_id),
                    org_id: OrgId::from(row.org_id),
                    scopes,
                }))
            }
            None => Ok(None),
        }
    }
}
