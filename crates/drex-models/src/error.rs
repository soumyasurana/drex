//! Model layer error types

use thiserror::Error;

/// Errors that can occur when interacting with model backends.
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ModelError {
    /// The request was malformed or contained invalid parameters.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// Authentication or configuration failure.
    /// This indicates missing API keys, invalid credentials, or misconfiguration.
    #[error("authentication failed: {0}")]
    Authentication(String),

    /// Connection or backend failure.
    /// The backend could not be reached or failed to respond.
    #[error("backend connection failed: {0}")]
    Connection(String),

    /// Rate limiting from the backend.
    #[error("rate limited: {0}")]
    RateLimited(String),

    /// Error returned by the provider/model.
    /// This includes cases where the model refuses to generate content
    /// or the provider returns an error for the specific request.
    #[error("provider error: {0}")]
    Provider(String),

    /// Serialization or protocol error.
    /// The response could not be parsed or the request could not be serialized.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Unsupported capability requested.
    /// The backend does not support a feature required by the request.
    #[error("unsupported capability: {0}")]
    Unsupported(String),

    /// Timeout waiting for response.
    #[error("request timed out")]
    Timeout,

    /// Unexpected or internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl ModelError {
    /// Create an invalid request error.
    pub fn invalid_request<S: Into<String>>(msg: S) -> Self {
        Self::InvalidRequest(msg.into())
    }

    /// Create an authentication error.
    pub fn authentication<S: Into<String>>(msg: S) -> Self {
        Self::Authentication(msg.into())
    }

    /// Create a connection error.
    pub fn connection<S: Into<String>>(msg: S) -> Self {
        Self::Connection(msg.into())
    }

    /// Create a rate limited error.
    pub fn rate_limited<S: Into<String>>(msg: S) -> Self {
        Self::RateLimited(msg.into())
    }

    /// Create a provider error.
    pub fn provider<S: Into<String>>(msg: S) -> Self {
        Self::Provider(msg.into())
    }

    /// Create a serialization error.
    pub fn serialization<S: Into<String>>(msg: S) -> Self {
        Self::Serialization(msg.into())
    }

    /// Create an unsupported capability error.
    pub fn unsupported<S: Into<String>>(msg: S) -> Self {
        Self::Unsupported(msg.into())
    }

    /// Create an internal error.
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Self::Internal(msg.into())
    }

    /// Check if this error suggests retrying might succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Connection(_) | Self::RateLimited(_) | Self::Timeout
        )
    }

    /// Check if this is an authentication error.
    pub fn is_authentication(&self) -> bool {
        matches!(self, Self::Authentication(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_creation() {
        let err = ModelError::invalid_request("test");
        assert!(matches!(err, ModelError::InvalidRequest(_)));
        assert_eq!(err.to_string(), "invalid request: test");

        let err = ModelError::authentication("no key");
        assert!(matches!(err, ModelError::Authentication(_)));

        let err = ModelError::connection("timeout");
        assert!(matches!(err, ModelError::Connection(_)));

        let err = ModelError::rate_limited("too many requests");
        assert!(matches!(err, ModelError::RateLimited(_)));

        let err = ModelError::provider("model overloaded");
        assert!(matches!(err, ModelError::Provider(_)));

        let err = ModelError::serialization("invalid json");
        assert!(matches!(err, ModelError::Serialization(_)));

        let err = ModelError::unsupported("vision");
        assert!(matches!(err, ModelError::Unsupported(_)));

        let err = ModelError::Timeout;
        assert!(matches!(err, ModelError::Timeout));

        let err = ModelError::internal("unexpected");
        assert!(matches!(err, ModelError::Internal(_)));
    }

    #[test]
    fn retryable_errors() {
        assert!(ModelError::connection("timeout").is_retryable());
        assert!(ModelError::rate_limited("rate limit").is_retryable());
        assert!(ModelError::Timeout.is_retryable());

        assert!(!ModelError::invalid_request("bad").is_retryable());
        assert!(!ModelError::authentication("no key").is_retryable());
        assert!(!ModelError::unsupported("feature").is_retryable());
    }

    #[test]
    fn authentication_errors() {
        assert!(ModelError::authentication("invalid").is_authentication());
        assert!(!ModelError::connection("timeout").is_authentication());
    }
}
