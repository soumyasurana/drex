mod health_check;

use drex_config::AppConfig;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Load configuration
    let config = match AppConfig::load() {
        Ok(cfg) => {
            println!("Configuration loaded successfully");
            cfg
        }
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize tracing with configured log level
    let log_level = &config.log_level;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .init();

    info!(
        app_name = %config.app_name,
        environment = %config.environment,
        "drex core starting"
    );

    // Run health checks
    let all_healthy = health_check::run_health_checks(&config).await;

    if all_healthy {
        info!("All systems operational");
    } else {
        error!("Some systems are unavailable, but continuing to run");
    }

    // Set up shutdown signal handler
    let shutdown = tokio::signal::ctrl_c();

    info!("Drex is running. Press Ctrl+C to shut down.");

    // Wait for shutdown signal
    match shutdown.await {
        Ok(()) => {
            info!("Shutdown signal received, gracefully shutting down...");
        }
        Err(e) => {
            error!("Failed to listen for shutdown signal: {}", e);
        }
    }

    info!("Drex shut down complete");
}
