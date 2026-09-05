use async_trait::async_trait;

use crate::error::WorkerError;
use crate::job::{Job, JobKind, JobResult};

pub mod evaluation;
pub mod ingestion;
pub mod memory_sweep;

// ---------------------------------------------------------------------------
// JobHandler trait
// ---------------------------------------------------------------------------

/// A typed handler for one family of background jobs.
#[async_trait]
pub trait JobHandler: Send + Sync {
    /// Execute the job and return a `JobResult`.
    ///
    /// Implementations must be idempotent where possible — the worker may
    /// re-deliver a job after a crash before an `ack` was recorded.
    async fn handle(&self, job: &Job) -> Result<JobResult, WorkerError>;

    /// The job kind this handler is responsible for.
    fn kind(&self) -> JobKind;
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// Routes an incoming job to the correct `JobHandler` by `JobKind`.
pub struct Dispatcher {
    handlers: Vec<Box<dyn JobHandler>>,
}

impl Dispatcher {
    pub fn new(handlers: Vec<Box<dyn JobHandler>>) -> Self {
        Self { handlers }
    }

    /// Dispatch `job` to the matching handler, returning an error if no
    /// handler is registered for its `JobKind`.
    pub async fn dispatch(&self, job: &Job) -> Result<JobResult, WorkerError> {
        for handler in &self.handlers {
            if handler.kind() == job.kind {
                return handler.handle(job).await;
            }
        }
        Err(WorkerError::Handler(format!(
            "no handler registered for job kind {:?}",
            job.kind
        )))
    }
}
