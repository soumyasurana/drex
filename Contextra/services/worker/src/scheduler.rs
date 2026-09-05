use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;
use tracing::{error, info};

use crate::job::{Job, JobKind};
use crate::queue::JobQueue;

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

/// Periodically enqueues background sweep jobs into the queue.
///
/// Currently schedules `MemorySweep` jobs on a fixed interval, but additional
/// job kinds can be added by extending `scheduled_jobs()`.
pub struct Scheduler<Q: JobQueue + 'static> {
    queue: Arc<Q>,
    /// How often a `MemorySweep` job should be enqueued.
    sweep_interval: Duration,
}

impl<Q: JobQueue + 'static> Scheduler<Q> {
    /// Create a new `Scheduler`.
    ///
    /// - `queue`          — where sweep jobs are pushed.
    /// - `sweep_interval` — period between memory-sweep jobs.  Defaults to
    ///   15 minutes if you call `Scheduler::default_interval()`.
    pub fn new(queue: Arc<Q>, sweep_interval: Duration) -> Self {
        Self {
            queue,
            sweep_interval,
        }
    }

    /// The recommended production sweep interval (15 minutes).
    pub fn default_interval() -> Duration {
        Duration::from_secs(15 * 60)
    }

    /// Run the scheduler until `shutdown_rx` receives `true`.
    pub async fn run(&self, mut shutdown_rx: watch::Receiver<bool>) {
        info!(
            sweep_interval_secs = self.sweep_interval.as_secs(),
            "scheduler starting"
        );

        let mut interval = tokio::time::interval(self.sweep_interval);
        // Skip ticks that were missed while the worker was busy.
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
                _ = interval.tick() => {
                    for job in Self::scheduled_jobs() {
                        if let Err(e) = self.queue.enqueue(&job).await {
                            error!(
                                job_type = job.kind.as_str(),
                                error = %e,
                                "scheduler failed to enqueue job"
                            );
                        } else {
                            info!(job_type = job.kind.as_str(), %job.id, "scheduler enqueued periodic job");
                        }
                    }
                }
            }
        }

        info!("scheduler stopped");
    }

    /// Returns the list of jobs that should be enqueued on each tick.
    fn scheduled_jobs() -> Vec<Job> {
        vec![Job::new(
            JobKind::MemorySweep,
            serde_json::json!({ "scope": "global" }),
        )]
    }
}
