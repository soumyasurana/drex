use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::{error, info, warn};

use telemetry::metrics::{increment_job_count, increment_job_retry_count, record_job_duration};

use crate::error::WorkerError;
use crate::handlers::Dispatcher;
use crate::job::JobResult;
use crate::queue::JobQueue;

/// Timeout passed to `queue.dequeue()` in each loop iteration.
/// Short enough to check the shutdown signal regularly.
const DEQUEUE_TIMEOUT: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// WorkerPool
// ---------------------------------------------------------------------------

/// Spawns `concurrency` worker tasks that each loop on `queue.dequeue()`,
/// dispatch the job to the appropriate handler, record telemetry, and
/// ack/nack accordingly.
///
/// Shutdown is signalled by dropping (or sending `true` into) the
/// `shutdown_tx` watch sender — all workers will finish their current job
/// and then exit cleanly.
pub struct WorkerPool<Q: JobQueue + 'static> {
    queue: Arc<Q>,
    dispatcher: Arc<Dispatcher>,
    concurrency: usize,
}

impl<Q: JobQueue + 'static> WorkerPool<Q> {
    /// Create a new pool.
    ///
    /// - `queue`       — the job queue implementation to read from.
    /// - `dispatcher`  — routes jobs to their handlers.
    /// - `concurrency` — number of concurrent worker tasks to spawn.
    pub fn new(queue: Arc<Q>, dispatcher: Arc<Dispatcher>, concurrency: usize) -> Self {
        Self {
            queue,
            dispatcher,
            concurrency: concurrency.max(1),
        }
    }

    /// Start all worker tasks and block until `shutdown_rx` receives `true`.
    ///
    /// Returns once all workers have exited.
    pub async fn run(&self, mut shutdown_rx: watch::Receiver<bool>) {
        info!(concurrency = self.concurrency, "worker pool starting");

        let mut handles = Vec::with_capacity(self.concurrency);

        for worker_id in 0..self.concurrency {
            let queue = Arc::clone(&self.queue);
            let dispatcher = Arc::clone(&self.dispatcher);
            let shutdown = shutdown_rx.clone();

            let handle = tokio::spawn(worker_loop(worker_id, queue, dispatcher, shutdown));
            handles.push(handle);
        }

        // Wait for the shutdown signal.
        let _ = shutdown_rx.wait_for(|v| *v).await;
        info!("shutdown signal received, waiting for workers to finish");

        // All workers watch the same receiver — they will exit on next loop
        // iteration.  We just await their completion.
        for handle in handles {
            let _ = handle.await;
        }

        info!("all workers stopped");
    }
}

// ---------------------------------------------------------------------------
// Individual worker loop
// ---------------------------------------------------------------------------

async fn worker_loop<Q: JobQueue>(
    worker_id: usize,
    queue: Arc<Q>,
    dispatcher: Arc<Dispatcher>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    info!(worker_id, "worker started");

    loop {
        // Check for shutdown before blocking on dequeue.
        if *shutdown_rx.borrow() {
            break;
        }

        let dequeue_result = tokio::select! {
            biased;
            // Honour shutdown even while waiting for a job.
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
                continue;
            }
            result = queue.dequeue(DEQUEUE_TIMEOUT) => result,
        };

        let job = match dequeue_result {
            Err(WorkerError::ShuttingDown) => break,
            Err(e) => {
                error!(worker_id, error = %e, "dequeue error");
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            Ok(None) => {
                // Timeout elapsed with no job available — loop again.
                continue;
            }
            Ok(Some(job)) => job,
        };

        let job_type = job.kind.as_str();
        let job_id = job.id;
        info!(worker_id, %job_id, job_type, retry_count = job.retry_count, "dequeued job");

        let start = Instant::now();
        let result = dispatcher.dispatch(&job).await;
        let elapsed = start.elapsed();

        match result {
            Ok(JobResult::Success) => {
                record_job_duration(job_type, "success", elapsed);
                increment_job_count(job_type, "success");
                info!(worker_id, %job_id, job_type, elapsed_ms = elapsed.as_millis(), "job succeeded");

                if let Err(e) = queue.ack(&job_id.to_string()).await {
                    error!(worker_id, %job_id, error = %e, "ack failed");
                }
            }

            Ok(JobResult::Failure(reason)) => {
                record_job_duration(job_type, "failure", elapsed);
                increment_job_count(job_type, "failure");
                warn!(worker_id, %job_id, job_type, reason, "job failed");

                if job.retry_count < crate::queue::MAX_RETRIES {
                    increment_job_retry_count(job_type);
                } else {
                    increment_job_count(job_type, "dead_lettered");
                }

                if let Err(e) = queue.nack(&job).await {
                    error!(worker_id, %job_id, error = %e, "nack failed");
                }
            }

            Err(e) => {
                record_job_duration(job_type, "failure", elapsed);
                increment_job_count(job_type, "failure");
                error!(worker_id, %job_id, job_type, error = %e, "handler error");

                if job.retry_count < crate::queue::MAX_RETRIES {
                    increment_job_retry_count(job_type);
                } else {
                    increment_job_count(job_type, "dead_lettered");
                }

                if let Err(nack_err) = queue.nack(&job).await {
                    error!(worker_id, %job_id, error = %nack_err, "nack failed after handler error");
                }
            }
        }
    }

    info!(worker_id, "worker stopped");
}
