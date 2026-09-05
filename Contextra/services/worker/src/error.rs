use thiserror::Error;

/// All errors that can occur within the worker service.
#[derive(Debug, Error)]
pub enum WorkerError {
    /// The job payload could not be serialized or deserialized.
    #[error("job serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// A Redis operation failed.
    #[error("queue backend error: {0}")]
    Queue(String),

    /// A job handler returned an application-level failure.
    #[error("job handler error: {0}")]
    Handler(String),

    /// The worker was shut down before the job could be processed.
    #[error("worker is shutting down")]
    ShuttingDown,
}

impl From<redis::RedisError> for WorkerError {
    fn from(err: redis::RedisError) -> Self {
        Self::Queue(err.to_string())
    }
}
