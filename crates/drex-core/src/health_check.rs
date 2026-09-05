use crate::state::AppState;
use drex_config::AppConfig;
// AsyncCommands trait is used via the cmd! macro
use sqlx::postgres::PgPoolOptions;
use tracing::{error, info, warn};

/// The result of a health check.
#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    #[allow(dead_code)]
    Unhealthy(String),
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    #[allow(dead_code)]
    pub fn is_unhealthy(&self) -> bool {
        matches!(self, HealthStatus::Unhealthy(_))
    }
}

/// Check PostgreSQL connectivity.
///
/// Attempts to connect to the configured database and execute `SELECT 1`.
/// Returns `Healthy` if the query succeeds, `Unhealthy` with error details otherwise.
pub async fn check_postgres(config: &AppConfig) -> HealthStatus {
    info!("Checking PostgreSQL connectivity...");

    let pool_result = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(3))
        .connect(&config.database.url)
        .await;

    match pool_result {
        Ok(pool) => {
            match sqlx::query("SELECT 1").fetch_one(&pool).await {
                Ok(_) => {
                    info!("PostgreSQL is reachable");
                    HealthStatus::Healthy
                }
                Err(e) => {
                    let msg = format!("PostgreSQL query failed: {}", e);
                    error!("{}", msg);
                    HealthStatus::Unhealthy(msg)
                }
            }
        }
        Err(e) => {
            let msg = format!("Failed to connect to PostgreSQL: {}", e);
            error!("{}", msg);
            HealthStatus::Unhealthy(msg)
        }
    }
}

/// Check Redis connectivity.
///
/// Attempts to connect to the configured Redis and execute `PING`.
/// Returns `Healthy` if PONG is received, `Unhealthy` with error details otherwise.
pub async fn check_redis(config: &AppConfig) -> HealthStatus {
    info!("Checking Redis connectivity...");

    let client_result = redis::Client::open(config.redis.url.clone());

    match client_result {
        Ok(client) => {
            // Note: get_async_connection is deprecated, but we're using it for simplicity
            // In production, consider get_multiplexed_async_connection
            #[allow(deprecated)]
            match client.get_async_connection().await {
                Ok(mut conn) => {
                    match redis::cmd("PING").query_async::<_, String>(&mut conn).await {
                        Ok(response) if response == "PONG" => {
                            info!("Redis is reachable");
                            HealthStatus::Healthy
                        }
                        Ok(other) => {
                            let msg = format!("Redis responded unexpectedly: {}", other);
                            warn!("{}", msg);
                            HealthStatus::Unhealthy(msg)
                        }
                        Err(e) => {
                            let msg = format!("Redis PING failed: {}", e);
                            error!("{}", msg);
                            HealthStatus::Unhealthy(msg)
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to get Redis connection: {}", e);
                    error!("{}", msg);
                    HealthStatus::Unhealthy(msg)
                }
            }
        }
        Err(e) => {
            let msg = format!("Failed to create Redis client: {}", e);
            error!("{}", msg);
            HealthStatus::Unhealthy(msg)
        }
    }
}

/// Run all health checks and report the overall status.
///
/// This function checks PostgreSQL and Redis connectivity, logs the results,
/// and returns whether all backends are healthy. The application continues
/// running even if backends are unhealthy.
pub async fn run_health_checks(config: &AppConfig) -> bool {
    info!("Running startup health checks...");

    let postgres_status = check_postgres(config).await;
    let redis_status = check_redis(config).await;

    let postgres_healthy = postgres_status.is_healthy();
    let redis_healthy = redis_status.is_healthy();

    // Log summary
    info!("Health check results:");
    info!("  PostgreSQL: {}", if postgres_healthy {
        "healthy"
    } else {
        "unhealthy"
    });
    info!("  Redis: {}", if redis_healthy { "healthy" } else { "unhealthy" });

    if postgres_healthy && redis_healthy {
        info!("Drex is ready");
        true
    } else {
        warn!("Some backends are unavailable. Drex will continue running with degraded functionality.");
        if !postgres_healthy {
            error!("PostgreSQL is not available. Database-dependent features will fail.");
        }
        if !redis_healthy {
            error!("Redis is not available. Caching and session features will fail.");
        }
        false
    }
}

/// Check memory system connectivity and health.
///
/// Verifies that the memory backend (Contextra via VectorMemoryStore)
/// is reachable and can perform basic operations.
/// Returns `Healthy` if the memory system responds, `Unhealthy` with error details otherwise.
pub async fn check_memory(app_state: Option<&AppState>) -> HealthStatus {
    use drex_memory::Memory;
    use drex_memory::MemoryKind;

    info!("Checking memory system health...");

    // If we don't have an initialized state, we can't check memory
    let app_state = match app_state {
        Some(state) => state,
        None => {
            let msg = "Memory system not initialized".to_string();
            warn!("{}", msg);
            return HealthStatus::Unhealthy(msg);
        }
    };

    // Perform a test memory operation
    // Store a test memory, then retrieve it, then delete it
    let test_memory = Memory::new(
        MemoryKind::Working,
        "Drex memory health check test data",
    );

    // Try to store
    let id = match app_state.memory_store.store(test_memory.clone()).await {
        Ok(id) => id,
        Err(e) => {
            let msg = format!("Memory store operation failed: {}", e);
            error!("{}", msg);
            return HealthStatus::Unhealthy(msg);
        }
    };

    // Try to retrieve
    match app_state.memory_store.get(id).await {
        Ok(Some(_)) => {
            // Success - now clean up
            match app_state.memory_store.forget(id).await {
                Ok(()) => {}
                Err(e) => {
                    warn!("Failed to clean up test memory during health check: {}", e);
                }
            }
            info!("Memory system is reachable and operational");
            HealthStatus::Healthy
        }
        Ok(None) => {
            let msg = "Memory retrieval returned no data".to_string();
            error!("{}", msg);
            HealthStatus::Unhealthy(msg)
        }
        Err(e) => {
            let msg = format!("Memory retrieval failed: {}", e);
            error!("{}", msg);
            HealthStatus::Unhealthy(msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_status_is_healthy() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(!HealthStatus::Unhealthy("error".to_string()).is_healthy());
    }

    #[test]
    fn health_status_is_unhealthy() {
        assert!(!HealthStatus::Healthy.is_unhealthy());
        assert!(HealthStatus::Unhealthy("error".to_string()).is_unhealthy());
    }
}
