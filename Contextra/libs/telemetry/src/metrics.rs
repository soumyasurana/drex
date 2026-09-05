use metrics::{counter, histogram};
use std::time::Duration;

/// Record the duration of an HTTP request
pub fn record_request_duration(method: &str, path: &str, duration: Duration) {
    histogram!(
        "http_requests_duration_seconds",
        "method" => method.to_string(),
        "path" => path.to_string()
    )
    .record(duration.as_secs_f64());
}

/// Increment the count of an HTTP error
pub fn increment_error_count(method: &str, path: &str, error_type: &str) {
    counter!(
        "http_requests_errors_total",
        "method" => method.to_string(),
        "path" => path.to_string(),
        "error_type" => error_type.to_string()
    )
    .increment(1);
}

// ---------------------------------------------------------------------------
// Worker / job-queue metrics
// ---------------------------------------------------------------------------

/// Record how long a background job took to execute.
///
/// `job_type` — e.g. `"document_ingestion"`, `"evaluation_run"`, `"memory_sweep"`
/// `status`   — `"success"` or `"failure"`
pub fn record_job_duration(job_type: &str, status: &str, duration: Duration) {
    histogram!(
        "worker_job_duration_seconds",
        "job_type" => job_type.to_string(),
        "status"   => status.to_string()
    )
    .record(duration.as_secs_f64());
}

/// Increment the total job counter.
///
/// `status` — `"success"`, `"failure"`, or `"dead_lettered"`
pub fn increment_job_count(job_type: &str, status: &str) {
    counter!(
        "worker_jobs_total",
        "job_type" => job_type.to_string(),
        "status"   => status.to_string()
    )
    .increment(1);
}

/// Increment the retry counter for a job type.
pub fn increment_job_retry_count(job_type: &str) {
    counter!(
        "worker_jobs_retried_total",
        "job_type" => job_type.to_string()
    )
    .increment(1);
}
