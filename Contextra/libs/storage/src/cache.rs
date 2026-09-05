use async_trait::async_trait;
use errors::ContextraError;
use redis::{AsyncCommands, Client};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

#[async_trait]
pub trait Cache: Send + Sync {
    async fn get<T>(&self, key: &str) -> Result<Option<T>, ContextraError>
    where
        T: DeserializeOwned + Send;

    async fn set_with_ttl<T>(
        &self,
        key: &str,
        value: &T,
        ttl: Duration,
    ) -> Result<(), ContextraError>
    where
        T: Serialize + Send + Sync;

    async fn delete(&self, key: &str) -> Result<(), ContextraError>;

    async fn exists(&self, key: &str) -> Result<bool, ContextraError>;
}

#[derive(Clone)]
pub struct RedisCache {
    connection: Arc<Mutex<redis::aio::MultiplexedConnection>>,
}

impl RedisCache {
    pub async fn connect(url: &str) -> Result<Self, ContextraError> {
        let client = Client::open(url).map_err(|e| {
            ContextraError::StorageError(format!("Failed to create Redis client: {e}"))
        })?;

        let connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| {
                ContextraError::StorageError(format!("Failed to connect to Redis: {e}"))
            })?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }
}

#[async_trait]
impl Cache for RedisCache {
    async fn get<T>(&self, key: &str) -> Result<Option<T>, ContextraError>
    where
        T: DeserializeOwned + Send,
    {
        let mut connection = self.connection.lock().await;
        let serialized: Option<String> = connection
            .get(key)
            .await
            .map_err(|e| ContextraError::StorageError(format!("Redis get failed: {e}")))?;

        match serialized {
            Some(value) => serde_json::from_str(&value).map(Some).map_err(|e| {
                ContextraError::StorageError(format!(
                    "Failed to deserialize cached value for key '{key}': {e}"
                ))
            }),
            None => Ok(None),
        }
    }

    async fn set_with_ttl<T>(
        &self,
        key: &str,
        value: &T,
        ttl: Duration,
    ) -> Result<(), ContextraError>
    where
        T: Serialize + Send + Sync,
    {
        let serialized = serde_json::to_string(value).map_err(|e| {
            ContextraError::StorageError(format!("Failed to serialize cached value: {e}"))
        })?;
        let ttl_seconds = ttl.as_secs().max(1);

        let mut connection = self.connection.lock().await;
        let _: () = connection
            .set_ex(key, serialized, ttl_seconds)
            .await
            .map_err(|e| ContextraError::StorageError(format!("Redis set failed: {e}")))?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), ContextraError> {
        let mut connection = self.connection.lock().await;
        let _: i64 = connection
            .del(key)
            .await
            .map_err(|e| ContextraError::StorageError(format!("Redis delete failed: {e}")))?;

        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, ContextraError> {
        let mut connection = self.connection.lock().await;
        connection
            .exists(key)
            .await
            .map_err(|e| ContextraError::StorageError(format!("Redis exists failed: {e}")))
    }
}
