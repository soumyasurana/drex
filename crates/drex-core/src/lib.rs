//! Drex Core - Main application runtime
//!
//! Drex Core is responsible for:
//! - Configuration loading
//! - Logging/tracing initialization
//! - Health checks for all backends
//! - Application state management
//! - Memory system integration
//! - Graceful shutdown handling

pub mod health_check;
pub mod state;

/// Re-export commonly used types
pub use health_check::HealthStatus;
pub use state::{AppState, MemoryConfig, OperationalHealth, initialize_app_state};
