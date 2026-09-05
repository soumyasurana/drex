use crate::cache::Cache;
use errors::ContextraError;
use serde::{Serialize, de::DeserializeOwned};
use std::marker::PhantomData;
use std::time::Duration;
use types::ConversationId;

pub struct SessionStore<C, S> {
    cache: C,
    ttl: Duration,
    key_prefix: String,
    _state: PhantomData<S>,
}

impl<C, S> SessionStore<C, S> {
    pub fn new(cache: C, ttl: Duration) -> Self {
        Self::with_prefix(cache, ttl, "session:conversation")
    }

    pub fn with_prefix(cache: C, ttl: Duration, key_prefix: impl Into<String>) -> Self {
        Self {
            cache,
            ttl,
            key_prefix: key_prefix.into(),
            _state: PhantomData,
        }
    }

    fn key(&self, conversation_id: &ConversationId) -> String {
        format!("{}:{}", self.key_prefix, conversation_id)
    }
}

impl<C, S> SessionStore<C, S>
where
    C: Cache,
{
    pub async fn get(&self, conversation_id: &ConversationId) -> Result<Option<S>, ContextraError>
    where
        S: DeserializeOwned + Send,
    {
        self.cache.get(&self.key(conversation_id)).await
    }

    pub async fn set(
        &self,
        conversation_id: &ConversationId,
        state: &S,
    ) -> Result<(), ContextraError>
    where
        S: Serialize + Send + Sync,
    {
        self.cache
            .set_with_ttl(&self.key(conversation_id), state, self.ttl)
            .await
    }

    pub async fn delete(&self, conversation_id: &ConversationId) -> Result<(), ContextraError> {
        self.cache.delete(&self.key(conversation_id)).await
    }

    pub async fn exists(&self, conversation_id: &ConversationId) -> Result<bool, ContextraError> {
        self.cache.exists(&self.key(conversation_id)).await
    }
}
