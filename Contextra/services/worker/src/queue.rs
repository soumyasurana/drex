use async_trait::async_trait;
use redis::{AsyncCommands, Client};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::error::WorkerError;
use crate::job::Job;

// ---------------------------------------------------------------------------
// Retry / backoff configuration
// ---------------------------------------------------------------------------

/// Maximum number of delivery attempts before a job is dead-lettered.
pub const MAX_RETRIES: u32 = 3;
/// Base delay for exponential backoff on retry.
pub const BASE_BACKOFF: Duration = Duration::from_secs(5);
/// Hard ceiling on the computed backoff delay.
pub const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Compute the next `scheduled_at` Unix timestamp after a failed attempt.
pub fn next_scheduled_at(retry_count: u32) -> u64 {
    let base_secs = BASE_BACKOFF.as_secs();
    // 2^retry_count, clamped to avoid overflow (retry_count is small in practice)
    let multiplier = 1_u64.checked_shl(retry_count).unwrap_or(u64::MAX);
    let delay_secs = (base_secs.saturating_mul(multiplier)).min(MAX_BACKOFF.as_secs());
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_add(delay_secs)
}

// ---------------------------------------------------------------------------
// JobQueue trait
// ---------------------------------------------------------------------------

/// Abstraction over a durable, ordered job queue.
///
/// `RedisJobQueue` provides a production Redis-backed implementation;
/// `InMemoryJobQueue` provides a lightweight in-process version used in tests.
#[async_trait]
pub trait JobQueue: Send + Sync {
    /// Push a job to the tail of the ready queue.
    async fn enqueue(&self, job: &Job) -> Result<(), WorkerError>;

    /// Block until a job is available (up to `timeout`) and return it.
    ///
    /// Returns `Ok(None)` if the timeout elapsed without a job becoming ready.
    async fn dequeue(&self, timeout: Duration) -> Result<Option<Job>, WorkerError>;

    /// Acknowledge successful processing — remove the job from the in-flight
    /// tracking set so it is not replayed on restart.
    async fn ack(&self, job_id: &str) -> Result<(), WorkerError>;

    /// Negatively acknowledge a failed job.
    ///
    /// If `job.retry_count < MAX_RETRIES`, the job is re-enqueued with an
    /// incremented retry counter and an exponential backoff delay applied to
    /// `scheduled_at`.  Otherwise it is pushed to the dead-letter queue.
    async fn nack(&self, job: &Job) -> Result<(), WorkerError>;
}

// ---------------------------------------------------------------------------
// Redis key constants
// ---------------------------------------------------------------------------

/// Jobs waiting to be picked up by a worker.
const QUEUE_KEY: &str = "worker:queue";
/// Jobs currently being processed (used for at-least-once delivery).
const PROCESSING_KEY: &str = "worker:processing";
/// Jobs that exceeded `MAX_RETRIES` and need manual intervention.
const DLQ_KEY: &str = "worker:dlq";

// ---------------------------------------------------------------------------
// RedisJobQueue
// ---------------------------------------------------------------------------

/// Production Redis-backed job queue.
///
/// Uses a dedicated async connection (not the shared `RedisCache` multiplexed
/// connection) so that the blocking `BLPOP` call does not starve other Redis
/// operations.
pub struct RedisJobQueue {
    /// A connection used exclusively for blocking dequeue operations.
    dequeue_conn: Arc<Mutex<redis::aio::MultiplexedConnection>>,
    /// A separate connection used for non-blocking enqueue / ack / nack.
    cmd_conn: Arc<Mutex<redis::aio::MultiplexedConnection>>,
}

impl RedisJobQueue {
    /// Connect to Redis at `url` and return a ready `RedisJobQueue`.
    pub async fn connect(url: &str) -> Result<Self, WorkerError> {
        let client = Client::open(url)
            .map_err(|e| WorkerError::Queue(format!("failed to open Redis client: {e}")))?;

        let dequeue_conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| {
                WorkerError::Queue(format!("failed to connect to Redis (dequeue): {e}"))
            })?;

        let cmd_conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| WorkerError::Queue(format!("failed to connect to Redis (cmd): {e}")))?;

        Ok(Self {
            dequeue_conn: Arc::new(Mutex::new(dequeue_conn)),
            cmd_conn: Arc::new(Mutex::new(cmd_conn)),
        })
    }
}

#[async_trait]
impl JobQueue for RedisJobQueue {
    async fn enqueue(&self, job: &Job) -> Result<(), WorkerError> {
        let serialized = serde_json::to_string(job)?;
        let mut conn = self.cmd_conn.lock().await;
        let _: i64 = conn
            .rpush(QUEUE_KEY, &serialized)
            .await
            .map_err(|e| WorkerError::Queue(format!("RPUSH failed: {e}")))?;
        Ok(())
    }

    async fn dequeue(&self, timeout: Duration) -> Result<Option<Job>, WorkerError> {
        let timeout_secs = timeout.as_secs_f64();
        let mut conn = self.dequeue_conn.lock().await;

        // BLPOP returns Option<(key, value)>
        let result: Option<(String, String)> = conn
            .blpop(QUEUE_KEY, timeout_secs)
            .await
            .map_err(|e| WorkerError::Queue(format!("BLPOP failed: {e}")))?;

        let Some((_, raw)) = result else {
            return Ok(None);
        };

        let job: Job = serde_json::from_str(&raw)?;

        // Move to the in-flight tracking list for at-least-once semantics.
        {
            let mut cmd = self.cmd_conn.lock().await;
            let _: i64 = cmd
                .rpush(PROCESSING_KEY, &raw)
                .await
                .map_err(|e| WorkerError::Queue(format!("RPUSH processing failed: {e}")))?;
        }

        Ok(Some(job))
    }

    async fn ack(&self, job_id: &str) -> Result<(), WorkerError> {
        // We stored the raw JSON in the processing list; we need to remove by
        // job id. The cleanest approach without a second copy is to scan the
        // processing list, find the entry matching the id, and remove it.
        // For simplicity we use LRANGE + LREM with a pattern-less match.
        let mut conn = self.cmd_conn.lock().await;
        let entries: Vec<String> = conn
            .lrange(PROCESSING_KEY, 0, -1)
            .await
            .map_err(|e| WorkerError::Queue(format!("LRANGE failed: {e}")))?;

        for entry in entries {
            // Parse enough to check the id without full deserialization errors
            if let Ok(partial) = serde_json::from_str::<serde_json::Value>(&entry)
                && partial.get("id").and_then(|v| v.as_str()) == Some(job_id)
            {
                let _: i64 = conn
                    .lrem(PROCESSING_KEY, 1, &entry)
                    .await
                    .map_err(|e| WorkerError::Queue(format!("LREM ack failed: {e}")))?;
                break;
            }
        }
        Ok(())
    }

    async fn nack(&self, job: &Job) -> Result<(), WorkerError> {
        // Remove from the in-flight list first.
        self.ack(&job.id.to_string()).await?;

        if job.retry_count >= MAX_RETRIES {
            // Dead-letter the job.
            let serialized = serde_json::to_string(job)?;
            let mut conn = self.cmd_conn.lock().await;
            let _: i64 = conn
                .rpush(DLQ_KEY, &serialized)
                .await
                .map_err(|e| WorkerError::Queue(format!("RPUSH DLQ failed: {e}")))?;
            tracing::warn!(
                job_id = %job.id,
                job_type = job.kind.as_str(),
                retry_count = job.retry_count,
                "job exceeded max retries, dead-lettered"
            );
        } else {
            // Re-enqueue with incremented retry and backoff.
            let next_at = next_scheduled_at(job.retry_count);
            let retry_job = job.with_incremented_retry(next_at);
            let serialized = serde_json::to_string(&retry_job)?;
            let mut conn = self.cmd_conn.lock().await;
            let _: i64 = conn
                .rpush(QUEUE_KEY, &serialized)
                .await
                .map_err(|e| WorkerError::Queue(format!("RPUSH retry failed: {e}")))?;
            tracing::info!(
                job_id = %retry_job.id,
                job_type = retry_job.kind.as_str(),
                retry_count = retry_job.retry_count,
                "job re-enqueued for retry"
            );
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// InMemoryJobQueue — for unit tests
// ---------------------------------------------------------------------------

/// An in-process, mutex-backed queue that satisfies the same `JobQueue` trait.
///
/// This does **not** persist state across test runs and is never meant for
/// production use. It also applies retry / DLQ logic identically to
/// `RedisJobQueue` so behaviour is tested faithfully.
#[derive(Debug, Default)]
pub struct InMemoryJobQueue {
    /// Ready jobs waiting to be dequeued.
    pub ready: Arc<Mutex<VecDeque<Job>>>,
    /// Jobs currently being processed (indexed by job id string).
    pub processing: Arc<Mutex<Vec<Job>>>,
    /// Jobs that exceeded `MAX_RETRIES`.
    pub dead_letter: Arc<Mutex<Vec<Job>>>,
}

impl InMemoryJobQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drain all items from the dead-letter queue and return them (for test assertions).
    pub async fn drain_dlq(&self) -> Vec<Job> {
        let mut dlq = self.dead_letter.lock().await;
        dlq.drain(..).collect()
    }

    /// Snapshot the ready queue without consuming it.
    pub async fn ready_len(&self) -> usize {
        self.ready.lock().await.len()
    }
}

#[async_trait]
impl JobQueue for InMemoryJobQueue {
    async fn enqueue(&self, job: &Job) -> Result<(), WorkerError> {
        self.ready.lock().await.push_back(job.clone());
        Ok(())
    }

    async fn dequeue(&self, timeout: Duration) -> Result<Option<Job>, WorkerError> {
        // Poll with 10ms intervals up to `timeout` to simulate blocking behaviour.
        let deadline = std::time::Instant::now() + timeout.min(Duration::from_millis(500));

        loop {
            let now = std::time::Instant::now();
            {
                let mut ready = self.ready.lock().await;
                // Skip jobs whose scheduled_at is in the future.
                if let Some(pos) = ready.iter().position(|job| {
                    let now_secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    job.scheduled_at <= now_secs
                }) && let Some(job) = ready.remove(pos)
                {
                    self.processing.lock().await.push(job.clone());
                    return Ok(Some(job));
                }
            }
            if now >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn ack(&self, job_id: &str) -> Result<(), WorkerError> {
        let mut processing = self.processing.lock().await;
        processing.retain(|job| job.id.to_string() != job_id);
        Ok(())
    }

    async fn nack(&self, job: &Job) -> Result<(), WorkerError> {
        self.ack(&job.id.to_string()).await?;

        if job.retry_count >= MAX_RETRIES {
            self.dead_letter.lock().await.push(job.clone());
        } else {
            let next_at = next_scheduled_at(job.retry_count);
            let retry_job = job.with_incremented_retry(next_at);
            self.ready.lock().await.push_back(retry_job);
        }
        Ok(())
    }
}
