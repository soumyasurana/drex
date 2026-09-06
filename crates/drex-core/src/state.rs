//! Application state management for drex-core.
//!
//! This module manages the top-level application state including configuration,
//! memory systems, and other core resources. It provides a centralized way
//! to access these components throughout the application.

use async_trait::async_trait;
use drex_memory::{
    Confidence, MemoryStore, RuleBasedPolicy, TaskTrustLevel,
};
use embeddings::OllamaEmbeddingProvider;
use storage::vector_store::QdrantVectorStore;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::health_check::HealthStatus;

/// Stub embedding provider for initialization when no external provider is available.
/// This generates deterministic embeddings based on input hash.
#[derive(Clone, Debug)]
pub struct StubEmbeddingProvider {
    dimensions: usize,
}

impl StubEmbeddingProvider {
    pub fn new(dimensions: usize) -> Self {
        Self { dimensions }
    }
}

#[async_trait]
impl embeddings::EmbeddingProvider for StubEmbeddingProvider {
    async fn embed_batch(&self, inputs: &[String]) -> Result<Vec<embeddings::Embedding>, embeddings::EmbeddingError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut results = Vec::with_capacity(inputs.len());
        for input in inputs {
            let mut hasher = DefaultHasher::new();
            input.hash(&mut hasher);
            let hash = hasher.finish();
            
            // Generate a deterministic embedding from the hash
            let mut embedding = Vec::with_capacity(self.dimensions);
            for i in 0..self.dimensions {
                let value = ((hash.wrapping_add(i as u64) % 1000) as f32) / 1000.0;
                embedding.push(value);
            }
            results.push(embedding);
        }
        Ok(results)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        "stub-embedding-provider"
    }
}

/// The top-level application state containing all initialized resources.
pub struct AppState {
    /// Application configuration
    pub config: drex_config::AppConfig,

    /// The memory policy governing storage and retrieval decisions
    pub memory_policy: Arc<RuleBasedPolicy>,

    /// The policy-enforcing memory store
    /// This uses a type-erased wrapper to allow different backend configurations
    pub memory_store: Box<dyn MemoryStore>,

    /// Current operational health status
    pub health: OperationalHealth,
}

/// Combined operational health status of all backends.
#[derive(Debug, Clone)]
pub struct OperationalHealth {
    pub postgres: HealthStatus,
    pub redis: HealthStatus,
    pub memory: HealthStatus,
}

impl OperationalHealth {
    /// Returns true if all backends are healthy.
    pub fn is_fully_healthy(&self) -> bool {
        self.postgres.is_healthy() && self.redis.is_healthy() && self.memory.is_healthy()
    }

    /// Returns true if memory is specifically healthy (critical for Drex).
    pub fn is_memory_healthy(&self) -> bool {
        self.memory.is_healthy()
    }

    /// Returns the overall health summary.
    pub fn summary(&self) -> String {
        format!(
            "PostgreSQL: {}, Redis: {}, Memory: {}",
            if self.postgres.is_healthy() { "✓" } else { "✗" },
            if self.redis.is_healthy() { "✓" } else { "✗" },
            if self.memory.is_healthy() { "✓" } else { "✗" }
        )
    }
}

/// Configuration for memory system initialization.
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// The task trust level to use for policy evaluation
    pub trust_level: TaskTrustLevel,

    /// Minimum confidence threshold for memory storage
    pub min_confidence: Confidence,

    /// Whether to use in-memory store (for testing) or Contextra
    pub use_in_memory: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            trust_level: TaskTrustLevel::Medium,
            min_confidence: Confidence::new(0.2),
            use_in_memory: false, // Default to in-memory backend for now
        }
    }
}

/// Initialize the application state with all dependencies.
///
/// This performs the full startup sequence:
/// 1. Load configuration
/// 2. Initialize memory policy
/// 3. Initialize memory store (with Contextra backend)
/// 4. Wrap with policy layer
///
/// Returns the initialized AppState or an error description.
///
/// # Errors
/// Returns an error string if critical initialization fails.
/// Non-critical failures are logged but do not cause init to fail.
pub async fn initialize_app_state(
    config: drex_config::AppConfig,
    memory_config: MemoryConfig,
) -> Result<AppState, String> {
    info!("Initializing application state...");

    // Step 1: Initialize memory policy
    info!("Initializing memory policy with trust level {:?}...", memory_config.trust_level);
    let memory_policy = Arc::new(
        RuleBasedPolicy::new(memory_config.trust_level)
            .with_min_confidence(memory_config.min_confidence)
    );

    // Step 2: Initialize memory store backend
    info!("Initializing memory store with real backends...");
    info!("Connecting to Qdrant at {} for vector storage", &config.qdrant.url);
    info!("Using Ollama at {} for embeddings", &config.ollama.base_url);

    let memory_store = match create_memory_store(memory_policy.clone(), &config).await {
        Ok(store) => {
            info!("Memory store initialized successfully with real backends");
            store
        }
        Err(e) => {
            let msg = format!("Failed to initialize memory store: {}", e);
            error!("{}", msg);
            return Err(msg);
        }
    };

    // Step 3: Verify memory system is functional
    info!("Verifying memory system health...");
    let memory_health = verify_memory_health(memory_store.as_ref()).await;

    let state = AppState {
        config,
        memory_policy,
        memory_store,
        health: OperationalHealth {
            postgres: HealthStatus::Unhealthy("Not checked yet".to_string()),
            redis: HealthStatus::Unhealthy("Not checked yet".to_string()),
            memory: memory_health.clone(),
        },
    };

    if memory_health.is_healthy() {
        info!("Application state initialized successfully");
    } else {
        warn!("Application state initialized but memory is unhealthy");
    }

    Ok(state)
}

/// Create a memory store with the given policy and configuration.
/// This helper function centralizes store creation logic.
///
/// Creates real storage backends connected to Qdrant and Ollama.
async fn create_memory_store(
    policy: Arc<RuleBasedPolicy>,
    config: &drex_config::AppConfig,
) -> Result<Box<dyn MemoryStore>, String> {
    // Connect to Qdrant for vector storage
    let vector_store = QdrantVectorStore::connect(&config.qdrant.url, config.qdrant.api_key.clone())
        .map_err(|e| format!("Failed to connect to Qdrant: {}", e))?;

    // Create Ollama embedding provider
    // Default to nomic-embed-text which is a good embedding model for Ollama
    let embedding_model = std::env::var("DREX_EMBEDDING_MODEL")
        .unwrap_or_else(|_| "nomic-embed-text".to_string());
    let embedding_provider = OllamaEmbeddingProvider::with_base_url(
        &embedding_model,
        768, // nomic-embed-text outputs 768 dimensions
        &config.ollama.base_url,
    );

    // Create the memory store
    let vector_memory_store = memory::VectorMemoryStore::new(vector_store, embedding_provider);

    // Ensure the collection exists before using the store
    info!("Creating Qdrant collection 'long-term-memory' if needed...");
    vector_memory_store.create_collection().await
        .map_err(|e| format!("Failed to create Qdrant collection: {}", e))?;

    let raw_store = drex_memory::ContextraMemoryStore::new(vector_memory_store);

    info!("Memory store created with Qdrant (at {}) and Ollama (at {})",
        config.qdrant.url, config.ollama.base_url);

    Ok(Box::new(drex_memory::PolicyEnforcingStore::new(
        (*policy).clone(),
        raw_store,
    )))
}

/// Verify memory system is functional by performing a test operation.
async fn verify_memory_health(store: &dyn MemoryStore) -> HealthStatus {
    use drex_memory::Memory;
    use drex_memory::MemoryKind;

    // Step 1: Attempt to store a test memory
    let test_memory = Memory::new(
        MemoryKind::Working,
        "Drex memory health check",
    )
    .with_importance(0.1); // Low importance - health check data

    let id = match store.store(test_memory).await {
        Ok(id) => id,
        Err(e) => {
            let msg = format!("Failed to store test memory: {}", e);
            error!("{}", msg);
            return HealthStatus::Unhealthy(msg);
        }
    };

    // Step 2: Attempt to retrieve it
    match store.get(id).await {
        Ok(Some(_)) => {
            // Success - clean up
            match store.forget(id).await {
                Ok(()) => {
                    info!("Memory health check passed");
                    HealthStatus::Healthy
                }
                Err(e) => {
                    // Warning but still healthy - the core operations work
                    warn!("Memory cleanup failed: {}", e);
                    info!("Memory health check passed (with cleanup warning)");
                    HealthStatus::Healthy
                }
            }
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

/// Update the operational health status within the app state.
impl AppState {
    pub fn update_health(&mut self, postgres: HealthStatus, redis: HealthStatus, memory: HealthStatus) {
        self.health = OperationalHealth {
            postgres,
            redis,
            memory,
        };
    }

    /// Log the current operational health status.
    pub fn log_health_status(&self) {
        match self.health.is_fully_healthy() {
            true => {
                info!(
                    postgres_healthy = self.health.postgres.is_healthy(),
                    redis_healthy = self.health.redis.is_healthy(),
                    memory_healthy = self.health.memory.is_healthy(),
                    "All systems operational"
                );
            }
            false => {
                warn!(
                    postgres_healthy = self.health.postgres.is_healthy(),
                    redis_healthy = self.health.redis.is_healthy(),
                    memory_healthy = self.health.memory.is_healthy(),
                    "Some systems are unavailable"
                );

                if !self.health.postgres.is_healthy() {
                    error!("PostgreSQL is not available");
                }
                if !self.health.redis.is_healthy() {
                    error!("Redis is not available");
                }
                if !self.health.memory.is_healthy() {
                    error!("Memory system is not available");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_health_fully_healthy() {
        let health = OperationalHealth {
            postgres: HealthStatus::Healthy,
            redis: HealthStatus::Healthy,
            memory: HealthStatus::Healthy,
        };
        assert!(health.is_fully_healthy());
    }

    #[test]
    fn operational_health_partially_unhealthy() {
        let health = OperationalHealth {
            postgres: HealthStatus::Healthy,
            redis: HealthStatus::Unhealthy("test".to_string()),
            memory: HealthStatus::Healthy,
        };
        assert!(!health.is_fully_healthy());
        assert!(health.is_memory_healthy());
    }

    #[test]
    fn operational_health_summary() {
        let health = OperationalHealth {
            postgres: HealthStatus::Healthy,
            redis: HealthStatus::Healthy,
            memory: HealthStatus::Healthy,
        };
        assert_eq!(health.summary(), "PostgreSQL: ✓, Redis: ✓, Memory: ✓");
    }
}
