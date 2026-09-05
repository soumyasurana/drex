//! Model backend trait definition

use crate::error::ModelError;
use crate::request::ModelRequest;
use crate::response::ModelResponse;
use async_trait::async_trait;

/// Trait for model backends.
///
/// This trait defines the interface that Drex uses to communicate with
/// various LLM providers (OpenAI, Anthropic, Ollama, etc.).
///
/// Implementations should handle:
/// - Authentication and configuration
/// - Request serialization for the specific provider
/// - Response deserialization
/// - Error mapping to consistent `ModelError` variants
#[async_trait]
pub trait ModelBackend: Send + Sync {
    /// Complete a model request and return a full response.
    ///
    /// This is the primary method for non-streaming interactions.
    /// It sends the request to the backend and waits for a complete response.
    ///
    /// # Arguments
    /// * `request` - The model request containing messages, tools, and parameters
    ///
    /// # Returns
    /// * `Ok(ModelResponse)` - The complete model response
    /// * `Err(ModelError)` - If the request fails
    async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError>;

    /// Check if this backend supports a specific capability.
    ///
    /// Backends should return `true` for capabilities they support.
    /// This allows callers to adapt requests accordingly.
    ///
    /// # Arguments
    /// * `capability` - The capability to check
    ///
    /// # Returns
    /// `true` if the backend supports the capability, `false` otherwise.
    fn supports(&self, capability: super::capabilities::BackendCapability) -> bool;

    /// Get the name of this backend provider.
    ///
    /// Should return a short identifier like "openai", "anthropic", etc.
    fn provider_name(&self) -> &str;

    /// Get the model identifier being used.
    fn model(&self) -> &str;

    /// Check if the backend is healthy and available.
    ///
    /// This can be used to verify connectivity before making requests.
    /// Default implementation returns `true`.
    async fn health_check(&self) -> crate::Result<()> {
        Ok(())
    }
}

/// Extension trait for backend implementations.
///
/// Provides convenient methods that work with any `ModelBackend`.
pub trait ModelBackendExt: ModelBackend {
    /// Create a request builder for convenient request construction.
    fn request_builder(&self) -> crate::request::ModelRequest {
        crate::request::ModelRequest::new(self.model())
    }
}

impl<T: ModelBackend> ModelBackendExt for T {}

/// Type alias for results from backend operations.
pub type BackendResult<T> = std::result::Result<T, ModelError>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capabilities::BackendCapability;
    use crate::content::Content;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Mock backend for testing.
    pub struct MockBackend {
        provider: String,
        model: String,
        call_count: Arc<AtomicUsize>,
    }

    impl MockBackend {
        pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
            Self {
                provider: provider.into(),
                model: model.into(),
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        pub fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ModelBackend for MockBackend {
        async fn complete(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(ModelResponse::new(
                "mock-id",
                &self.model,
                &self.provider,
                Content::text("Hello from mock"),
            ))
        }

        fn supports(&self, capability: BackendCapability) -> bool {
            matches!(
                capability,
                BackendCapability::TextGeneration | BackendCapability::ToolCalling
            )
        }

        fn provider_name(&self) -> &str {
            &self.provider
        }

        fn model(&self) -> &str {
            &self.model
        }
    }

    #[tokio::test]
    async fn mock_backend_complete() {
        let backend = MockBackend::new("mock", "gpt-4");
        let request = ModelRequest::new("gpt-4").user_message("Hello");

        let response = backend.complete(request).await.unwrap();
        assert_eq!(response.provider, "mock");
        assert_eq!(response.model, "gpt-4");
        assert_eq!(backend.call_count(), 1);
    }

    #[tokio::test]
    async fn backend_health_check_default() {
        let backend = MockBackend::new("mock", "model");
        // Default implementation returns Ok
        assert!(backend.health_check().await.is_ok());
    }

    #[test]
    fn backend_supports() {
        let backend = MockBackend::new("mock", "model");
        assert!(backend.supports(BackendCapability::TextGeneration));
        assert!(backend.supports(BackendCapability::ToolCalling));
        assert!(!backend.supports(BackendCapability::Streaming));
    }

    #[test]
    fn backend_provider_name() {
        let backend = MockBackend::new("openai", "gpt-4");
        assert_eq!(backend.provider_name(), "openai");
    }

    #[tokio::test]
    async fn request_builder() {
        let backend = MockBackend::new("mock", "model");
        let request = backend.request_builder().user_message("Hello");
        assert_eq!(request.model, "model");
    }
}
