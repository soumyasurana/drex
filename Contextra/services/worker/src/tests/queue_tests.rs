#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use crate::job::{Job, JobKind, JobResult};
use crate::queue::{InMemoryJobQueue, JobQueue, MAX_RETRIES};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn ingestion_job() -> Job {
    Job::new(
        JobKind::DocumentIngestion,
        serde_json::json!({
            "document_id": "doc-1",
            "collection_id": "col-1",
            "source_path": "/tmp/test.txt"
        }),
    )
}

fn sweep_job() -> Job {
    Job::new(
        JobKind::MemorySweep,
        serde_json::json!({ "scope": "global" }),
    )
}

// ---------------------------------------------------------------------------
// Enqueue / dequeue roundtrip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_enqueue_dequeue_roundtrip() {
    let queue = InMemoryJobQueue::new();
    let job = ingestion_job();
    let job_id = job.id;

    queue.enqueue(&job).await.unwrap();
    assert_eq!(queue.ready_len().await, 1);

    let dequeued = match queue.dequeue(Duration::from_millis(100)).await.unwrap() {
        Some(job) => job,
        None => panic!("should have dequeued a job"),
    };

    assert_eq!(dequeued.id, job_id);
    assert_eq!(dequeued.kind, JobKind::DocumentIngestion);
    assert_eq!(dequeued.retry_count, 0);

    // After dequeue the job is in "processing", not "ready".
    assert_eq!(queue.ready_len().await, 0);
    assert_eq!(queue.processing.lock().await.len(), 1);
}

#[tokio::test]
async fn test_dequeue_returns_none_on_empty_queue() {
    let queue = InMemoryJobQueue::new();
    let result = queue.dequeue(Duration::from_millis(50)).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_enqueue_multiple_preserves_fifo_order() {
    let queue = InMemoryJobQueue::new();

    let job_a = ingestion_job();
    let job_b = sweep_job();
    let id_a = job_a.id;
    let id_b = job_b.id;

    queue.enqueue(&job_a).await.unwrap();
    queue.enqueue(&job_b).await.unwrap();

    let first = queue
        .dequeue(Duration::from_millis(50))
        .await
        .unwrap()
        .unwrap();
    let second = queue
        .dequeue(Duration::from_millis(50))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(first.id, id_a);
    assert_eq!(second.id, id_b);
}

// ---------------------------------------------------------------------------
// Ack
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_ack_removes_job_from_processing() {
    let queue = InMemoryJobQueue::new();
    let job = ingestion_job();
    let job_id = job.id.to_string();

    queue.enqueue(&job).await.unwrap();
    queue
        .dequeue(Duration::from_millis(100))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(queue.processing.lock().await.len(), 1);

    queue.ack(&job_id).await.unwrap();
    assert_eq!(queue.processing.lock().await.len(), 0);
}

// ---------------------------------------------------------------------------
// Nack / retry logic
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_nack_increments_retry_count_and_re_enqueues() {
    let queue = InMemoryJobQueue::new();
    let job = ingestion_job();

    queue.enqueue(&job).await.unwrap();
    let dequeued = queue
        .dequeue(Duration::from_millis(100))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dequeued.retry_count, 0);

    queue.nack(&dequeued).await.unwrap();

    // Should be back in the ready queue with retry_count = 1.
    // (scheduled_at will be in the future, so we peek directly.)
    let ready = queue.ready.lock().await;
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].retry_count, 1);
    assert_eq!(ready[0].id, dequeued.id);
}

#[tokio::test]
async fn test_nack_three_times_sends_to_dlq() {
    let queue = InMemoryJobQueue::new();
    let original = ingestion_job();

    // Manually build a job that is already at MAX_RETRIES.
    let exhausted = {
        let mut j = original.clone();
        j.retry_count = MAX_RETRIES;
        j
    };

    queue.enqueue(&exhausted).await.unwrap();
    // Override the ready queue entry so scheduled_at is now.
    {
        let mut ready = queue.ready.lock().await;
        if let Some(entry) = ready.front_mut() {
            entry.scheduled_at = 0;
        }
    }

    let dequeued = queue
        .dequeue(Duration::from_millis(100))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dequeued.retry_count, MAX_RETRIES);

    queue.nack(&dequeued).await.unwrap();

    // Should be in DLQ, not ready queue.
    assert_eq!(queue.ready_len().await, 0);
    let dlq = queue.drain_dlq().await;
    assert_eq!(dlq.len(), 1);
    assert_eq!(dlq[0].id, original.id);
}

#[tokio::test]
async fn test_nack_sequential_retries_increment_correctly() {
    let queue = InMemoryJobQueue::new();
    let job = ingestion_job();

    queue.enqueue(&job).await.unwrap();

    // Simulate MAX_RETRIES failed attempts.
    for expected_retry in 0..MAX_RETRIES {
        // Force scheduled_at = 0 so the job is always immediately available.
        {
            let mut ready = queue.ready.lock().await;
            if let Some(entry) = ready.front_mut() {
                entry.scheduled_at = 0;
            }
        }

        let dequeued = match queue.dequeue(Duration::from_millis(100)).await.unwrap() {
            Some(job) => job,
            None => panic!("job should be available"),
        };
        assert_eq!(dequeued.retry_count, expected_retry, "retry_count mismatch");

        queue.nack(&dequeued).await.unwrap();
    }

    // After MAX_RETRIES nacks the job should be in the DLQ.
    {
        let mut ready = queue.ready.lock().await;
        if let Some(entry) = ready.front_mut() {
            entry.scheduled_at = 0;
        }
    }

    let last = queue
        .dequeue(Duration::from_millis(100))
        .await
        .unwrap()
        .expect("last job should be available");
    assert_eq!(last.retry_count, MAX_RETRIES);

    queue.nack(&last).await.unwrap();

    assert_eq!(queue.ready_len().await, 0);
    let dlq = queue.drain_dlq().await;
    assert_eq!(dlq.len(), 1);
}

// ---------------------------------------------------------------------------
// JobResult type checks
// ---------------------------------------------------------------------------

#[test]
fn test_job_result_variants() {
    let success = JobResult::Success;
    let failure = JobResult::Failure("something went wrong".into());

    assert_eq!(success, JobResult::Success);
    assert!(matches!(failure, JobResult::Failure(_)));
}

// ---------------------------------------------------------------------------
// Job construction helpers
// ---------------------------------------------------------------------------

#[test]
fn test_job_kind_as_str() {
    assert_eq!(JobKind::DocumentIngestion.as_str(), "document_ingestion");
    assert_eq!(JobKind::EvaluationRun.as_str(), "evaluation_run");
    assert_eq!(JobKind::MemorySweep.as_str(), "memory_sweep");
}

#[test]
fn test_job_with_incremented_retry_preserves_id() {
    let job = ingestion_job();
    let original_id = job.id;
    let retried = job.with_incremented_retry(9999);
    assert_eq!(retried.id, original_id);
    assert_eq!(retried.retry_count, 1);
    assert_eq!(retried.scheduled_at, 9999);
}
