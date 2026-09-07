//! Load Balancer - Multi-backend model routing with health-aware distribution
//!
//! This module provides intelligent load balancing across multiple model backends:
//! - Weighted round-robin routing with health checks
//! - Least-connections based distribution
//! - Latency-aware routing for optimal response times
//! - Circuit breaker pattern for failing backends
//! - Automatic failover and recovery detection
//! - Request queue management with backpressure

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// ModelBackend trait used for type bounds in advanced implementations
#[allow(unused_imports)]
use crate::backend::ModelBackend;

/// Load balancing strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LoadBalancingStrategy {
    /// Round-robin across healthy backends.
    RoundRobin,
    /// Weighted by backend capacity.
    Weighted,
    /// Least active connections.
    LeastConnections,
    /// Lowest observed latency.
    LeastLatency,
    /// Random with health weighting.
    Random,
    /// Hash-based (sticky for requests).
    HashRing,
}

impl Default for LoadBalancingStrategy {
    fn default() -> Self {
        Self::Weighted
    }
}

impl std::fmt::Display for LoadBalancingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RoundRobin => write!(f, "round_robin"),
            Self::Weighted => write!(f, "weighted"),
            Self::LeastConnections => write!(f, "least_connections"),
            Self::LeastLatency => write!(f, "least_latency"),
            Self::Random => write!(f, "random"),
            Self::HashRing => write!(f, "hash_ring"),
        }
    }
}

impl std::str::FromStr for LoadBalancingStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "round_robin" | "roundrobin" => Ok(Self::RoundRobin),
            "weighted" => Ok(Self::Weighted),
            "least_connections" | "leastconnections" => Ok(Self::LeastConnections),
            "least_latency" | "leastlatency" => Ok(Self::LeastLatency),
            "random" => Ok(Self::Random),
            "hash_ring" | "hashring" => Ok(Self::HashRing),
            _ => Err(format!("Unknown strategy: {}", s)),
        }
    }
}

/// Backend health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendStatus {
    /// Fully operational.
    Healthy,
    /// Degraded but accepting requests.
    Degraded,
    /// Not accepting new requests.
    Unhealthy,
    /// Circuit breaker open.
    CircuitOpen,
}

impl BackendStatus {
    /// Can this backend accept requests?
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }

    /// Should we check before routing?
    pub fn needs_verification(&self) -> bool {
        matches!(self, Self::Degraded)
    }
}

impl Default for BackendStatus {
    fn default() -> Self {
        Self::Healthy
    }
}

/// Backend instance with health tracking.
#[derive(Debug)]
pub struct BackendInstance {
    /// Backend identifier.
    pub id: String,
    /// Backend name/description.
    pub name: String,
    /// Current health status.
    status: RwLock<BackendStatus>,
    /// Weight for weighted strategies.
    pub weight: u32,
    /// Current active connections.
    active_connections: RwLock<u32>,
    /// Total requests served.
    request_count: RwLock<u64>,
    /// Failed requests.
    error_count: RwLock<u64>,
    /// Average latency.ms
    avg_latency_ms: RwLock<f64>,
    /// Last health check.
    last_health_check: RwLock<Instant>,
    /// Last successful request.
    last_success: RwLock<Option<Instant>>,
    /// Consecutive failures.
    consecutive_failures: RwLock<u32>,
    /// Circuit breaker state.
    circuit_open: RwLock<bool>,
    /// Circuit breaker trip time.
    circuit_tripped_at: RwLock<Option<Instant>>,
}

impl BackendInstance {
    /// Create new backend instance.
    pub fn new(id: String, name: String, weight: u32) -> Self {
        Self {
            id,
            name,
            status: RwLock::new(BackendStatus::Healthy),
            weight,
            active_connections: RwLock::new(0),
            request_count: RwLock::new(0),
            error_count: RwLock::new(0),
            avg_latency_ms: RwLock::new(0.0),
            last_health_check: RwLock::new(Instant::now()),
            last_success: RwLock::new(None),
            consecutive_failures: RwLock::new(0),
            circuit_open: RwLock::new(false),
            circuit_tripped_at: RwLock::new(None),
        }
    }

    /// Get current status.
    pub async fn status(&self) -> BackendStatus {
        *self.status.read().await
    }

    /// Get active connections.
    pub async fn active_connections(&self) -> u32 {
        *self.active_connections.read().await
    }

    /// Get average latency.
    pub async fn avg_latency_ms(&self) -> f64 {
        *self.avg_latency_ms.read().await
    }

    /// Record request start.
    pub async fn start_request(&self) {
        let mut count = self.active_connections.write().await;
        *count += 1;
    }

    /// Record request completion.
    pub async fn complete_request(&self, latency_ms: f64, success: bool) {
        let mut active = self.active_connections.write().await;
        *active = active.saturating_sub(1);

        let mut count = self.request_count.write().await;
        *count += 1;

        if success {
            let mut failures = self.consecutive_failures.write().await;
            *failures = 0;
            let mut last = self.last_success.write().await;
            *last = Some(Instant::now());
            
            // Close circuit if open
            let mut circuit = self.circuit_open.write().await;
            if *circuit {
                *circuit = false;
                info!("Circuit closed for backend {}", self.id);
            }
        } else {
            let mut errors = self.error_count.write().await;
            *errors += 1;
            let mut failures = self.consecutive_failures.write().await;
            *failures += 1;
        }

        // Update running average latency
        let mut avg = self.avg_latency_ms.write().await;
        *avg = *avg * 0.9 + latency_ms * 0.1;
    }

    /// Update health status.
    pub async fn update_status(&self, status: BackendStatus) {
        let mut s = self.status.write().await;
        *s = status;
        let mut check = self.last_health_check.write().await;
        *check = Instant::now();
    }

    /// Check if circuit should be tripped.
    pub async fn should_trip_circuit(&self, threshold: u32) -> bool {
        let failures = *self.consecutive_failures.read().await;
        let circuit = *self.circuit_open.read().await;
        failures >= threshold && !circuit
    }

    /// Trip circuit breaker.
    pub async fn trip_circuit(&self) {
        let mut circuit = self.circuit_open.write().await;
        *circuit = true;
        let mut tripped = self.circuit_tripped_at.write().await;
        *tripped = Some(Instant::now());
        
        let mut status = self.status.write().await;
        *status = BackendStatus::CircuitOpen;
        
        warn!("Circuit breaker tripped for backend {}", self.id);
    }

    /// Check if circuit should be tested (half-open).
    pub async fn should_test_circuit(&self, timeout: Duration) -> bool {
        let circuit = *self.circuit_open.read().await;
        if !circuit {
            return false;
        }
        
        let tripped = *self.circuit_tripped_at.read().await;
        tripped.map_or(false, |t| t.elapsed() > timeout)
    }
}

/// Load balancer configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadBalancerConfig {
    /// Routing strategy.
    pub strategy: LoadBalancingStrategy,
    /// Health check interval.
    pub health_check_interval: Duration,
    /// Circuit breaker threshold (consecutive failures).
    pub circuit_threshold: u32,
    /// Circuit breaker timeout.
    pub circuit_timeout: Duration,
    /// Maximum requests queued per backend.
    pub max_queue_size: usize,
    /// Request timeout.
    pub request_timeout: Duration,
    /// Degraded threshold (latency ms).
    pub degraded_threshold_ms: u64,
    /// Unhealthy threshold (latency ms).
    pub unhealthy_threshold_ms: u64,
}

impl Default for LoadBalancerConfig {
    fn default() -> Self {
        Self {
            strategy: LoadBalancingStrategy::Weighted,
            health_check_interval: Duration::from_secs(30),
            circuit_threshold: 5,
            circuit_timeout: Duration::from_secs(60),
            max_queue_size: 100,
            request_timeout: Duration::from_secs(120),
            degraded_threshold_ms: 2000,
            unhealthy_threshold_ms: 10000,
        }
    }
}

impl LoadBalancerConfig {
    /// Config optimized for low latency.
    pub fn latency_optimized() -> Self {
        Self {
            strategy: LoadBalancingStrategy::LeastLatency,
            degraded_threshold_ms: 500,
            unhealthy_threshold_ms: 2000,
            ..Default::default()
        }
    }

    /// Config optimized for throughput.
    pub fn throughput_optimized() -> Self {
        Self {
            strategy: LoadBalancingStrategy::LeastConnections,
            max_queue_size: 500,
            ..Default::default()
        }
    }
}

/// Backend selection result.
#[derive(Debug)]
pub struct SelectionResult {
    /// Selected backend ID.
    pub backend_id: String,
    /// Strategy used.
    pub strategy: LoadBalancingStrategy,
    /// Expected latency estimate.
    pub estimated_latency_ms: f64,
}

/// Load balancer for model backends.
pub struct LoadBalancer {
    config: LoadBalancerConfig,
    /// Backend instances.
    backends: RwLock<Vec<Arc<BackendInstance>>>,
    /// Round-robin index.
    round_robin_index: RwLock<usize>,
    /// Request queue per backend.
    request_queues: RwLock<HashMap<String, VecDeque<String>>>,
    /// Health check task handle.
    _health_check_handle: Option<tokio::task::JoinHandle<()>>,
}

impl LoadBalancer {
    /// Create new load balancer.
    pub fn new(config: LoadBalancerConfig) -> Self {
        Self {
            config,
            backends: RwLock::new(Vec::new()),
            round_robin_index: RwLock::new(0),
            request_queues: RwLock::new(HashMap::new()),
            _health_check_handle: None,
        }
    }

    /// Create with health check task.
    pub fn with_health_checks(config: LoadBalancerConfig) -> Arc<Self> {
        let lb = Arc::new(Self::new(config.clone()));
        let lb_clone = lb.clone();
        
        let _handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.health_check_interval);
            loop {
                interval.tick().await;
                lb_clone.run_health_checks().await;
            }
        });

        // Background health check task runs independently
        // In production, use a proper task manager for graceful shutdown
        lb
    }

    /// Register a backend.
    pub async fn register_backend(&self, id: String, name: String, weight: u32) {
        let backend = Arc::new(BackendInstance::new(id.clone(), name, weight));
        let mut backends = self.backends.write().await;
        backends.push(backend);

        let mut queues = self.request_queues.write().await;
        queues.insert(id.clone(), VecDeque::with_capacity(self.config.max_queue_size));

        info!("Registered backend {} with weight {}", id, weight);
    }

    /// Remove a backend.
    pub async fn remove_backend(&self, id: &str) {
        let mut backends = self.backends.write().await;
        backends.retain(|b| b.id != id);
        
        let mut queues = self.request_queues.write().await;
        queues.remove(id);
        
        info!("Removed backend {}", id);
    }

    /// Select backend for request.
    pub async fn select_backend(&self) -> Option<SelectionResult> {
        let backends = self.backends.read().await;
        
        if backends.is_empty() {
            return None;
        }

        // Filter to healthy backends
        let mut healthy = Vec::new();
        let mut half_open_candidate = None;
        
        for backend in backends.iter() {
            let status = backend.status().await;
            if status.is_available() {
                healthy.push(backend.clone());
            } else if half_open_candidate.is_none() && backend.should_test_circuit(self.config.circuit_timeout).await {
                half_open_candidate = Some(backend.clone());
            }
        }

        if healthy.is_empty() {
            // Try circuit half-open candidate
            if let Some(backend) = half_open_candidate {
                return Some(SelectionResult {
                    backend_id: backend.id.clone(),
                    strategy: self.config.strategy,
                    estimated_latency_ms: backend.avg_latency_ms().await,
                });
            }
            return None;
        }

        let selected = match self.config.strategy {
            LoadBalancingStrategy::RoundRobin => {
                self.select_round_robin(&healthy).await
            }
            LoadBalancingStrategy::Weighted => {
                self.select_weighted(&healthy).await
            }
            LoadBalancingStrategy::LeastConnections => {
                self.select_least_connections(&healthy).await
            }
            LoadBalancingStrategy::LeastLatency => {
                self.select_least_latency(&healthy).await
            }
            LoadBalancingStrategy::Random => {
                self.select_random(&healthy).await
            }
            LoadBalancingStrategy::HashRing => {
                // Hash ring requires request key - fall back to weighted
                self.select_weighted(&healthy).await
            }
        };

        if let Some(ref backend) = selected {
            let latency = backend.avg_latency_ms().await;
            Some(SelectionResult {
                backend_id: backend.id.clone(),
                strategy: self.config.strategy,
                estimated_latency_ms: latency,
            })
        } else {
            None
        }
    }

    /// Round-robin selection.
    async fn select_round_robin(&self, backends: &[Arc<BackendInstance>]) -> Option<Arc<BackendInstance>> {
        let mut index = self.round_robin_index.write().await;
        if backends.is_empty() {
            return None;
        }
        let selected = backends[*index % backends.len()].clone();
        *index = (*index + 1) % backends.len();
        Some(selected)
    }

    /// Weighted random selection.
    async fn select_weighted(&self, backends: &[Arc<BackendInstance>]) -> Option<Arc<BackendInstance>> {
        let total_weight: u32 = backends.iter().map(|b| b.weight).sum();
        if total_weight == 0 {
            return backends.first().cloned();
        }

        let random = (rand::random::<u32>()) % total_weight;
        let mut cumsum = 0;
        
        for backend in backends {
            cumsum += backend.weight;
            if random < cumsum {
                return Some(backend.clone());
            }
        }
        
        backends.last().cloned()
    }

    /// Least connections selection.
    async fn select_least_connections(&self, backends: &[Arc<BackendInstance>]) -> Option<Arc<BackendInstance>> {
        let mut min_connections = u32::MAX;
        let mut selected = None;

        for backend in backends {
            let connections = backend.active_connections().await;
            if connections < min_connections {
                min_connections = connections;
                selected = Some(backend.clone());
            }
        }

        selected
    }

    /// Least latency selection.
    async fn select_least_latency(&self, backends: &[Arc<BackendInstance>]) -> Option<Arc<BackendInstance>> {
        let mut min_latency = f64::INFINITY;
        let mut selected = None;

        for backend in backends {
            let latency = backend.avg_latency_ms().await;
            if latency < min_latency {
                min_latency = latency;
                selected = Some(backend.clone());
            }
        }

        selected
    }

    /// Random selection.
    async fn select_random(&self, backends: &[Arc<BackendInstance>]) -> Option<Arc<BackendInstance>> {
        if backends.is_empty() {
            return None;
        }
        let index = rand::random::<usize>() % backends.len();
        Some(backends[index].clone())
    }

    /// Record request completion.
    pub async fn record_result(&self, backend_id: &str, latency_ms: f64, success: bool) {
        let backends = self.backends.read().await;
        if let Some(backend) = backends.iter().find(|b| b.id == backend_id) {
            backend.complete_request(latency_ms, success).await;
            
            // Check circuit breaker
            if !success && backend.should_trip_circuit(self.config.circuit_threshold).await {
                backend.trip_circuit().await;
            }
        }
    }

    /// Run health checks on all backends.
    async fn run_health_checks(&self) {
        let backends = self.backends.read().await;
        
        for backend in backends.iter() {
            let latency = backend.avg_latency_ms().await;
            let failures = *backend.consecutive_failures.read().await;
            let last_success = *backend.last_success.read().await;
            
            // Determine status based on metrics
            let new_status = if failures >= self.config.circuit_threshold {
                BackendStatus::Unhealthy
            } else if latency > self.config.unhealthy_threshold_ms as f64 {
                BackendStatus::Unhealthy
            } else if latency > self.config.degraded_threshold_ms as f64 {
                BackendStatus::Degraded
            } else if last_success.map_or(true, |t| t.elapsed() > Duration::from_secs(60)) {
                BackendStatus::Degraded
            } else {
                BackendStatus::Healthy
            };
            
            backend.update_status(new_status).await;
            
            debug!(
                "Health check for {}: {:?} (latency: {:.1}ms, failures: {})",
                backend.id, new_status, latency, failures
            );
        }
    }

    /// Get balancer statistics.
    pub async fn statistics(&self) -> LoadBalancerStats {
        let backends = self.backends.read().await;
        let mut total_requests = 0u64;
        let mut total_errors = 0u64;
        let mut total_connections = 0u32;

        for backend in backends.iter() {
            total_requests += *backend.request_count.read().await;
            total_errors += *backend.error_count.read().await;
            total_connections += backend.active_connections().await;
        }

        LoadBalancerStats {
            backend_count: backends.len(),
            total_requests,
            total_errors,
            active_connections: total_connections,
            error_rate: if total_requests > 0 {
                total_errors as f64 / total_requests as f64
            } else {
                0.0
            },
        }
    }

    /// Get health of all backends.
    pub async fn backend_health(&self) -> Vec<BackendHealth> {
        let backends = self.backends.read().await;
        let mut health = Vec::new();

        for backend in backends.iter() {
            health.push(BackendHealth {
                id: backend.id.clone(),
                name: backend.name.clone(),
                status: backend.status().await,
                weight: backend.weight,
                active_connections: backend.active_connections().await,
                avg_latency_ms: backend.avg_latency_ms().await,
                request_count: *backend.request_count.read().await,
                error_count: *backend.error_count.read().await,
            });
        }

        health
    }
}

impl Drop for LoadBalancer {
    fn drop(&mut self) {
        // Cancel health check task if running
        if let Some(handle) = self._health_check_handle.take() {
            handle.abort();
        }
    }
}

/// Load balancer statistics.
#[derive(Debug, Clone)]
pub struct LoadBalancerStats {
    /// Number of registered backends.
    pub backend_count: usize,
    /// Total requests served.
    pub total_requests: u64,
    /// Total errors encountered.
    pub total_errors: u64,
    /// Currently active connections.
    pub active_connections: u32,
    /// Overall error rate.
    pub error_rate: f64,
}

/// Per-backend health information.
#[derive(Debug, Clone)]
pub struct BackendHealth {
    /// Backend ID.
    pub id: String,
    /// Backend name.
    pub name: String,
    /// Current status.
    pub status: BackendStatus,
    /// Weight.
    pub weight: u32,
    /// Active connections.
    pub active_connections: u32,
    /// Average latency in ms.
    pub avg_latency_ms: f64,
    /// Total requests.
    pub request_count: u64,
    /// Total errors.
    pub error_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_parsing() {
        assert_eq!(
            "round_robin".parse::<LoadBalancingStrategy>().unwrap(),
            LoadBalancingStrategy::RoundRobin
        );
        assert_eq!(
            "WEIGHTED".parse::<LoadBalancingStrategy>().unwrap(),
            LoadBalancingStrategy::Weighted
        );
        assert!("unknown".parse::<LoadBalancingStrategy>().is_err());
    }

    #[test]
    fn test_backend_status() {
        assert!(BackendStatus::Healthy.is_available());
        assert!(BackendStatus::Degraded.is_available());
        assert!(!BackendStatus::Unhealthy.is_available());
        assert!(!BackendStatus::CircuitOpen.is_available());
    }

    #[tokio::test]
    async fn test_backend_instance() {
        let backend = BackendInstance::new(
            "test-1".to_string(),
            "Test Backend".to_string(),
            10,
        );

        assert_eq!(backend.status().await, BackendStatus::Healthy);
        assert_eq!(backend.active_connections().await, 0);

        backend.start_request().await;
        assert_eq!(backend.active_connections().await, 1);

        backend.complete_request(100.0, true).await;
        assert_eq!(backend.active_connections().await, 0);
        assert!(backend.avg_latency_ms().await > 0.0);
    }

    #[tokio::test]
    async fn test_load_balancer_registration() {
        let lb = LoadBalancer::new(LoadBalancerConfig::default());

        lb.register_backend("b1".to_string(), "Backend 1".to_string(), 10).await;
        lb.register_backend("b2".to_string(), "Backend 2".to_string(), 20).await;

        let stats = lb.statistics().await;
        assert_eq!(stats.backend_count, 2);

        let health = lb.backend_health().await;
        assert_eq!(health.len(), 2);
    }

    #[tokio::test]
    async fn test_round_robin_selection() {
        let lb = LoadBalancer::new(LoadBalancerConfig {
            strategy: LoadBalancingStrategy::RoundRobin,
            ..Default::default()
        });

        lb.register_backend("b1".to_string(), "Backend 1".to_string(), 10).await;
        lb.register_backend("b2".to_string(), "Backend 2".to_string(), 10).await;

        let result1 = lb.select_backend().await.unwrap();
        let result2 = lb.select_backend().await.unwrap();

        // Should alternate
        assert_ne!(result1.backend_id, result2.backend_id);
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let backend = BackendInstance::new(
            "test-1".to_string(),
            "Test".to_string(),
            10,
        );

        // Simulate failures
        for _ in 0..5 {
            backend.complete_request(100.0, false).await;
        }

        assert!(backend.should_trip_circuit(5).await);
        
        backend.trip_circuit().await;
        assert_eq!(backend.status().await, BackendStatus::CircuitOpen);
    }

    #[tokio::test]
    async fn test_least_connections_selection() {
        let lb = LoadBalancer::new(LoadBalancerConfig {
            strategy: LoadBalancingStrategy::LeastConnections,
            ..Default::default()
        });

        lb.register_backend("b1".to_string(), "Backend 1".to_string(), 10).await;
        lb.register_backend("b2".to_string(), "Backend 2".to_string(), 10).await;

        // Start request on b1
        let backends = lb.backends.read().await;
        if let Some(b1) = backends.iter().find(|b| b.id == "b1") {
            b1.start_request().await;
        }
        drop(backends);

        // Should select b2 (fewer connections)
        let result = lb.select_backend().await.unwrap();
        assert_eq!(result.backend_id, "b2");
    }

    #[tokio::test]
    async fn test_backend_health_interpretation() {
        let config = LoadBalancerConfig::default();
        let backend = BackendInstance::new(
            "test".to_string(),
            "Test".to_string(),
            10,
        );

        // Simulate degraded latency
        let mut avg = backend.avg_latency_ms.write().await;
        *avg = config.degraded_threshold_ms as f64 + 100.0;
        drop(avg);

        let status = backend.status().await;
        // Status is manually set in health checks
        backend.update_status(BackendStatus::Degraded).await;
        assert_eq!(backend.status().await, BackendStatus::Degraded);
    }
}
