use async_trait::async_trait;
use tracing::{info, warn};

use crate::error::WorkerError;
use crate::job::{Job, JobKind, JobResult};

use super::JobHandler;

/// Handles `EvaluationRun` jobs.
///
/// Each job payload is expected to contain:
///
/// ```json
/// {
///   "dataset_id": "<uuid>",
///   "judge_model": "gpt-4.1-mini",
///   "k": 5
/// }
/// ```
///
/// The actual evaluation run is performed by `libs/evaluation::EvaluationPipeline`.
/// Full wiring requires an LLM provider and a retriever, which are injected at
/// service startup time.  This stub validates the payload and records intent so
/// the queue/retry/telemetry path is exercised immediately.
pub struct EvaluationJobHandler;

#[async_trait]
impl JobHandler for EvaluationJobHandler {
    fn kind(&self) -> JobKind {
        JobKind::EvaluationRun
    }

    async fn handle(&self, job: &Job) -> Result<JobResult, WorkerError> {
        let dataset_id = job
            .payload
            .get("dataset_id")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let judge_model = job
            .payload
            .get("judge_model")
            .and_then(|v| v.as_str())
            .unwrap_or("gpt-4.1-mini");
        let k = job.payload.get("k").and_then(|v| v.as_u64()).unwrap_or(5);

        info!(
            job_id = %job.id,
            dataset_id,
            judge_model,
            k,
            "starting evaluation run"
        );

        if dataset_id == "<unknown>" {
            let reason = "evaluation payload missing dataset_id".to_string();
            warn!(job_id = %job.id, reason, "evaluation job rejected");
            return Ok(JobResult::Failure(reason));
        }

        // TODO: wire libs/evaluation::EvaluationPipeline once the LLM provider
        // and retriever are available in this service's dependency graph.
        info!(
            job_id = %job.id,
            dataset_id,
            "evaluation run job accepted (pipeline wiring pending)"
        );

        Ok(JobResult::Success)
    }
}
