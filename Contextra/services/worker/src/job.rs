use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Job kind
// ---------------------------------------------------------------------------

/// The type of background work to be performed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    /// Ingest a document into the vector store asynchronously.
    DocumentIngestion,
    /// Run a long-form evaluation pipeline against a benchmark dataset.
    EvaluationRun,
    /// Periodic sweep to re-score memory importance and produce summaries.
    MemorySweep,
}

impl JobKind {
    /// Returns a stable, human-readable label used in metrics and logs.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DocumentIngestion => "document_ingestion",
            Self::EvaluationRun => "evaluation_run",
            Self::MemorySweep => "memory_sweep",
        }
    }
}

// ---------------------------------------------------------------------------
// Job envelope
// ---------------------------------------------------------------------------

/// A serialisable job envelope stored in and retrieved from the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique identifier for this job instance.
    pub id: Uuid,
    /// The kind of work to perform.
    pub kind: JobKind,
    /// Arbitrary JSON payload understood by the corresponding handler.
    pub payload: serde_json::Value,
    /// How many times this job has been attempted so far (0-based).
    pub retry_count: u32,
    /// Unix timestamp (seconds) when this job was first enqueued.
    pub enqueued_at: u64,
    /// Unix timestamp (seconds) at which this job should be processed.
    /// Workers skip jobs whose `scheduled_at` is in the future.
    pub scheduled_at: u64,
}

impl Job {
    /// Create a new job that should be processed immediately.
    pub fn new(kind: JobKind, payload: serde_json::Value) -> Self {
        let now = now_epoch_seconds();
        Self {
            id: Uuid::now_v7(),
            kind,
            payload,
            retry_count: 0,
            enqueued_at: now,
            scheduled_at: now,
        }
    }

    /// Create a new job scheduled to run at a specific future time.
    pub fn scheduled(kind: JobKind, payload: serde_json::Value, scheduled_at: u64) -> Self {
        Self {
            scheduled_at,
            ..Self::new(kind, payload)
        }
    }

    /// Increment the retry counter, returning a new job envelope ready to
    /// be re-enqueued after the appropriate backoff delay.
    pub fn with_incremented_retry(&self, next_scheduled_at: u64) -> Self {
        Self {
            id: self.id,
            kind: self.kind.clone(),
            payload: self.payload.clone(),
            retry_count: self.retry_count + 1,
            enqueued_at: self.enqueued_at,
            scheduled_at: next_scheduled_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Job result
// ---------------------------------------------------------------------------

/// The outcome of a single job execution attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobResult {
    /// The job completed successfully.
    Success,
    /// The job failed with a human-readable reason. The worker will retry or
    /// dead-letter the job depending on the current `retry_count`.
    Failure(String),
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
