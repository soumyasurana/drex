use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::info;

use worker::handlers::Dispatcher;
use worker::handlers::evaluation::EvaluationJobHandler;
use worker::handlers::ingestion::IngestionJobHandler;
use worker::handlers::memory_sweep::MemorySweepJobHandler;
use worker::queue::RedisJobQueue;
use worker::scheduler::Scheduler;
use worker::worker::WorkerPool;

/// Default number of concurrent worker tasks when `WORKER_CONCURRENCY` is not set.
const DEFAULT_CONCURRENCY: usize = 4;

/// Default memory-sweep interval when `WORKER_SWEEP_INTERVAL_SECS` is not set.
const DEFAULT_SWEEP_INTERVAL_SECS: u64 = 15 * 60;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // -----------------------------------------------------------------------
    // Telemetry
    // -----------------------------------------------------------------------
    let log_level = std::env::var("WORKER_LOG_LEVEL").unwrap_or_else(|_| "info".into());
    let telemetry_settings = telemetry::TelemetrySettings {
        service_name: "contextra-worker".into(),
        log_level,
        otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
    };
    telemetry::init_telemetry(&telemetry_settings);

    // -----------------------------------------------------------------------
    // Configuration from environment
    // -----------------------------------------------------------------------
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".into());

    let concurrency = std::env::var("WORKER_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_CONCURRENCY);

    let sweep_interval_secs = std::env::var("WORKER_SWEEP_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SWEEP_INTERVAL_SECS);

    info!(
        concurrency,
        sweep_interval_secs, redis_url, "starting contextra worker"
    );

    // -----------------------------------------------------------------------
    // Queue
    // -----------------------------------------------------------------------
    let queue = Arc::new(RedisJobQueue::connect(&redis_url).await?);

    // -----------------------------------------------------------------------
    // Dispatcher
    // -----------------------------------------------------------------------
    let dispatcher = Arc::new(Dispatcher::new(vec![
        Box::new(IngestionJobHandler),
        Box::new(EvaluationJobHandler),
        Box::new(MemorySweepJobHandler),
    ]));

    // -----------------------------------------------------------------------
    // Shutdown channel
    // -----------------------------------------------------------------------
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // -----------------------------------------------------------------------
    // Spawn worker pool
    // -----------------------------------------------------------------------
    let pool = WorkerPool::new(Arc::clone(&queue), Arc::clone(&dispatcher), concurrency);
    let pool_rx = shutdown_rx.clone();
    let pool_handle = tokio::spawn(async move {
        pool.run(pool_rx).await;
    });

    // -----------------------------------------------------------------------
    // Spawn scheduler
    // -----------------------------------------------------------------------
    let scheduler = Scheduler::new(Arc::clone(&queue), Duration::from_secs(sweep_interval_secs));
    let sched_rx = shutdown_rx.clone();
    let sched_handle = tokio::spawn(async move {
        scheduler.run(sched_rx).await;
    });

    // -----------------------------------------------------------------------
    // Graceful shutdown on Ctrl-C / SIGTERM
    // -----------------------------------------------------------------------
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("received Ctrl-C, initiating graceful shutdown");
        }
    }

    shutdown_tx.send(true)?;

    // Wait for all tasks to finish.
    let _ = tokio::join!(pool_handle, sched_handle);

    info!("worker exited cleanly");
    Ok(())
}
