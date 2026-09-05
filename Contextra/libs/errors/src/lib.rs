use http::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ContextraError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl ContextraError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::Conflict(_) => "CONFLICT",
            Self::RateLimited(_) => "RATE_LIMITED",
            Self::ProviderError(_) => "PROVIDER_ERROR",
            Self::StorageError(_) => "STORAGE_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::ServiceUnavailable(_) => "SERVICE_UNAVAILABLE",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::ProviderError(_) => StatusCode::BAD_GATEWAY,
            Self::StorageError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

impl From<std::io::Error> for ContextraError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => Self::Forbidden(err.to_string()),
            _ => Self::Internal(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(ContextraError::NotFound("test".into()).code(), "NOT_FOUND");
        assert_eq!(
            ContextraError::Validation("test".into()).code(),
            "VALIDATION_ERROR"
        );
        assert_eq!(
            ContextraError::Unauthorized("test".into()).code(),
            "UNAUTHORIZED"
        );
        assert_eq!(ContextraError::Forbidden("test".into()).code(), "FORBIDDEN");
        assert_eq!(ContextraError::Conflict("test".into()).code(), "CONFLICT");
        assert_eq!(
            ContextraError::RateLimited("test".into()).code(),
            "RATE_LIMITED"
        );
        assert_eq!(
            ContextraError::ProviderError("test".into()).code(),
            "PROVIDER_ERROR"
        );
        assert_eq!(
            ContextraError::StorageError("test".into()).code(),
            "STORAGE_ERROR"
        );
        assert_eq!(
            ContextraError::Internal("test".into()).code(),
            "INTERNAL_ERROR"
        );
    }

    #[test]
    fn test_http_status_mapping() {
        assert_eq!(
            ContextraError::NotFound("test".into()).http_status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ContextraError::Validation("test".into()).http_status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ContextraError::Unauthorized("test".into()).http_status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ContextraError::Forbidden("test".into()).http_status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            ContextraError::Conflict("test".into()).http_status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            ContextraError::RateLimited("test".into()).http_status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            ContextraError::ProviderError("test".into()).http_status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            ContextraError::StorageError("test".into()).http_status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ContextraError::Internal("test".into()).http_status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_io_error_conversion() {
        let not_found_io = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: ContextraError = not_found_io.into();
        assert!(matches!(err, ContextraError::NotFound(_)));

        let permission_io =
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let err2: ContextraError = permission_io.into();
        assert!(matches!(err2, ContextraError::Forbidden(_)));

        let other_io = std::io::Error::other("something else");
        let err3: ContextraError = other_io.into();
        assert!(matches!(err3, ContextraError::Internal(_)));
    }
}
