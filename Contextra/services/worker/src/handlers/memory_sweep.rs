use async_trait::async_trait;
use tracing::info;

use crate::error::WorkerError;
use crate::job::{Job, JobKind, JobResult};

use super::JobHandler;

/// Handles `MemorySweep` jobs.
///
/// A memory sweep re-scores importance for recent messages and, where the
/// conversation token count exceeds the configured limit, synthesises a new
/// rolling summary.  These jobs are typically enqueued by the `Scheduler` on a
/// configurable periodic interval rather than in response to API requests.
///
/// The actual work is performed by `libs/memory`:
/// - `ImportanceScorer::score_message` — re-evaluate and promote memories
/// - `ConversationMemory::summarize_overflow` — produce rolling summaries
///
/// Full wiring requires a vector memory store and an LLM summariser.  This
/// stub logs the sweep intent so the scheduler → queue → worker → telemetry
/// path can be validated end-to-end.
pub struct MemorySweepJobHandler;

#[async_trait]
impl JobHandler for MemorySweepJobHandler {
    fn kind(&self) -> JobKind {
        JobKind::MemorySweep
    }

    async fn handle(&self, job: &Job) -> Result<JobResult, WorkerError> {
        let user_id = job
            .payload
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let scope = job
            .payload
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("global");

        info!(
            job_id = %job.id,
            user_id,
            scope,
            "starting memory importance-scoring sweep"
        );

        // TODO: wire libs/memory::ImportanceScorer and
        // libs/memory::ConversationMemory::summarize_overflow once the session
        // store, vector memory store, and LLM provider are available here.
        info!(
            job_id = %job.id,
            "memory sweep completed (pipeline wiring pending)"
        );

        Ok(JobResult::Success)
    }
}
