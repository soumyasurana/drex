use async_trait::async_trait;
use tracing::{info, warn};

use crate::error::WorkerError;
use crate::job::{Job, JobKind, JobResult};

use super::JobHandler;

/// Handles `DocumentIngestion` jobs.
///
/// Each job payload is expected to contain at minimum:
///
/// ```json
/// {
///   "document_id": "<uuid>",
///   "collection_id": "<uuid>",
///   "source_path": "/path/to/file.pdf"
/// }
/// ```
///
/// The actual heavy-lifting (parsing → chunking → embedding → upsert) is
/// performed by `libs/ingestion::IngestionPipeline`.  Full wiring of that
/// pipeline requires the vector store and embedding provider to be
/// dependency-injected here; the current implementation logs the intent and
/// succeeds so the rest of the worker infrastructure (queue, retry, telemetry)
/// can be exercised end-to-end while provider integration is completed.
pub struct IngestionJobHandler;

#[async_trait]
impl JobHandler for IngestionJobHandler {
    fn kind(&self) -> JobKind {
        JobKind::DocumentIngestion
    }

    async fn handle(&self, job: &Job) -> Result<JobResult, WorkerError> {
        let document_id = job
            .payload
            .get("document_id")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let collection_id = job
            .payload
            .get("collection_id")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let source_path = job
            .payload
            .get("source_path")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");

        info!(
            job_id = %job.id,
            document_id,
            collection_id,
            source_path,
            "starting document ingestion"
        );

        // Validate that required fields are present.
        if document_id == "<unknown>" || collection_id == "<unknown>" {
            let reason = "ingestion payload missing document_id or collection_id".to_string();
            warn!(job_id = %job.id, reason, "ingestion job rejected");
            return Ok(JobResult::Failure(reason));
        }

        // TODO: wire libs/ingestion::IngestionPipeline once embedding provider
        // and vector store are available in this service's dependency graph.
        info!(
            job_id = %job.id,
            document_id,
            "document ingestion job accepted (pipeline wiring pending)"
        );

        Ok(JobResult::Success)
    }
}
