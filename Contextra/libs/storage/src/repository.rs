use async_trait::async_trait;
use errors::ContextraError;

#[async_trait]
pub trait Repository<T, Id> {
    async fn get(&self, id: &Id) -> Result<Option<T>, ContextraError>;
    async fn create(&self, entity: &T) -> Result<(), ContextraError>;
    async fn update(&self, entity: &T) -> Result<(), ContextraError>;
    async fn delete(&self, id: &Id) -> Result<(), ContextraError>;
}
