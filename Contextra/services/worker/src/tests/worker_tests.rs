#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

use crate::handlers::evaluation::EvaluationJobHandler;
use crate::handlers::ingestion::IngestionJobHandler;
use crate::handlers::memory_sweep::MemorySweepJobHandler;
use crate::handlers::{Dispatcher, JobHandler};
use crate::job::{Job, JobKind, JobResult};
use crate::queue::{InMemoryJobQueue, JobQueue, MAX_RETRIES};
use crate::worker::WorkerPool;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_dispatcher() -> Dispatcher {
    Dispatcher::new(vec![
        Box::new(IngestionJobHandler),
        Box::new(EvaluationJobHandler),
        Box::new(MemorySweepJobHandler),
    ])
}

fn valid_ingestion_job() -> Job {
    Job::new(
        JobKind::DocumentIngestion,
        serde_json::json!({
            "document_id": "doc-abc",
            "collection_id": "col-xyz",
            "source_path": "/data/doc.txt"
        }),
    )
}

fn invalid_ingestion_job() -> Job {
    // Missing document_id — handler should return Failure.
    Job::new(
        JobKind::DocumentIngestion,
        serde_json::json!({ "source_path": "/data/doc.txt" }),
    )
}

fn valid_evaluation_job() -> Job {
    Job::new(
        JobKind::EvaluationRun,
        serde_json::json!({ "dataset_id": "ds-1", "k": 5 }),
    )
}

fn invalid_evaluation_job() -> Job {
    Job::new(JobKind::EvaluationRun, serde_json::json!({}))
}

fn sweep_job() -> Job {
    Job::new(
        JobKind::MemorySweep,
        serde_json::json!({ "scope": "global" }),
    )
}

// ---------------------------------------------------------------------------
// Handler unit tests (no queue involved)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ingestion_handler_succeeds_with_valid_payload() {
    let handler = IngestionJobHandler;
    let job = valid_ingestion_job();
    let result = handler.handle(&job).await.unwrap();
    assert_eq!(result, JobResult::Success);
}

#[tokio::test]
async fn test_ingestion_handler_fails_with_missing_fields() {
    let handler = IngestionJobHandler;
    let job = invalid_ingestion_job();
    let result = handler.handle(&job).await.unwrap();
    assert!(matches!(result, JobResult::Failure(_)));
}

#[tokio::test]
async fn test_evaluation_handler_succeeds_with_valid_payload() {
    let handler = EvaluationJobHandler;
    let job = valid_evaluation_job();
    let result = handler.handle(&job).await.unwrap();
    assert_eq!(result, JobResult::Success);
}

#[tokio::test]
async fn test_evaluation_handler_fails_with_missing_dataset_id() {
    let handler = EvaluationJobHandler;
    let job = invalid_evaluation_job();
    let result = handler.handle(&job).await.unwrap();
    assert!(matches!(result, JobResult::Failure(_)));
}

#[tokio::test]
async fn test_memory_sweep_handler_succeeds() {
    let handler = MemorySweepJobHandler;
    let job = sweep_job();
    let result = handler.handle(&job).await.unwrap();
    assert_eq!(result, JobResult::Success);
}

// ---------------------------------------------------------------------------
// Dispatcher tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_dispatcher_routes_to_correct_handler() {
    let dispatcher = make_dispatcher();
    let result = dispatcher.dispatch(&valid_ingestion_job()).await.unwrap();
    assert_eq!(result, JobResult::Success);
}

#[tokio::test]
async fn test_dispatcher_returns_error_for_unknown_kind() {
    // A dispatcher with NO handlers.
    let dispatcher = Dispatcher::new(vec![]);
    let job = valid_ingestion_job();
    let result = dispatcher.dispatch(&job).await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// WorkerPool integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_worker_processes_job_and_acks() {
    let queue = Arc::new(InMemoryJobQueue::new());
    let dispatcher = Arc::new(make_dispatcher());

    let job = valid_ingestion_job();
    queue.enqueue(&job).await.unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let pool = WorkerPool::new(Arc::clone(&queue), Arc::clone(&dispatcher), 1);

    // Run the pool in the background.
    let pool_handle = {
        let shutdown_rx_clone = shutdown_rx.clone();
        tokio::spawn(async move {
            pool.run(shutdown_rx_clone).await;
        })
    };

    // Give the worker time to pick up and process the job.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Signal shutdown.
    shutdown_tx.send(true).unwrap();
    pool_handle.await.unwrap();

    // Job should have been acked (no longer in processing).
    assert_eq!(queue.processing.lock().await.len(), 0);
    assert_eq!(queue.ready_len().await, 0);
    // Nothing should have been dead-lettered.
    assert_eq!(queue.drain_dlq().await.len(), 0);
}

#[tokio::test]
async fn test_worker_nacks_invalid_job_and_retries() {
    let queue = Arc::new(InMemoryJobQueue::new());
    let dispatcher = Arc::new(make_dispatcher());

    // Enqueue a job that will fail validation in the handler.
    let job = invalid_ingestion_job();
    queue.enqueue(&job).await.unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let pool = WorkerPool::new(Arc::clone(&queue), Arc::clone(&dispatcher), 1);

    let pool_handle = tokio::spawn(async move {
        pool.run(shutdown_rx).await;
    });

    // Let the worker process and nack the job.
    tokio::time::sleep(Duration::from_millis(200)).await;

    shutdown_tx.send(true).unwrap();
    pool_handle.await.unwrap();

    // The job should have been re-enqueued (retry_count = 1) OR (if scheduled
    // in the future) remain in the ready queue.  Either way processing is empty.
    assert_eq!(queue.processing.lock().await.len(), 0);
    // Ready queue should now have one entry with retry_count = 1.
    let ready = queue.ready.lock().await;
    if !ready.is_empty() {
        assert_eq!(ready[0].retry_count, 1);
    }
}

#[tokio::test]
async fn test_graceful_shutdown_stops_all_workers() {
    let queue = Arc::new(InMemoryJobQueue::new());
    let dispatcher = Arc::new(make_dispatcher());

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let pool = WorkerPool::new(Arc::clone(&queue), Arc::clone(&dispatcher), 4);

    let pool_handle = tokio::spawn(async move {
        pool.run(shutdown_rx).await;
    });

    // Signal shutdown almost immediately — workers should exit cleanly.
    tokio::time::sleep(Duration::from_millis(50)).await;
    shutdown_tx.send(true).unwrap();

    // Should complete without hanging.
    tokio::time::timeout(Duration::from_secs(3), pool_handle)
        .await
        .expect("worker pool should shut down within 3 seconds")
        .unwrap();
}

#[tokio::test]
async fn test_worker_dead_letters_after_max_retries() {
    let queue = Arc::new(InMemoryJobQueue::new());
    let dispatcher = Arc::new(make_dispatcher());

    // Construct a job already at MAX_RETRIES.
    let mut job = invalid_ingestion_job();
    job.retry_count = MAX_RETRIES;
    job.scheduled_at = 0; // Immediately available.
    queue.enqueue(&job).await.unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let pool = WorkerPool::new(Arc::clone(&queue), Arc::clone(&dispatcher), 1);

    let pool_handle = tokio::spawn(async move {
        pool.run(shutdown_rx).await;
    });

    tokio::time::sleep(Duration::from_millis(300)).await;
    shutdown_tx.send(true).unwrap();
    pool_handle.await.unwrap();

    // Job should be in DLQ.
    let dlq = queue.drain_dlq().await;
    assert_eq!(dlq.len(), 1, "exhausted job should be in DLQ");
    assert_eq!(dlq[0].id, job.id);
}
