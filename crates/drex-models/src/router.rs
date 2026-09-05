//! Model Router - Task-based backend selection for LLM inference
//!
//! The ModelRouter sits above [`ModelBackend`] implementations and selects a backend
//! based on an explicit [`TaskKind`]. This enables different use cases to use
//! appropriate models without hard-coding provider-specific logic throughout the codebase.
//!
//! # Design Principles
//!
//! 1. **Explicit routing**: Given a [`TaskKind`], the router resolves to a configured backend.
//!    No heuristic guessing or model selection is performed.
//!
//! 2. **No silent fallback**: If no backend is configured for a task kind, or if the backend
//!    cannot satisfy required capabilities, the router returns a clear error. Silent fallback
//!    to alternative backends is intentionally forbidden by design - this ensures predictable
//!    behavior and prevents accidentally using inappropriate models for critical tasks.
//!
//! 3. **Capability-aware**: The router checks that the selected backend actually supports
//!    the capabilities required by the task kind before proceeding.
//!
//! 4. **Configurable**: Backend assignments are configuration-driven via [`RouterConfig`],
//!    loaded through the existing [`drex-config`] infrastructure.
//!
//! # Example
//!
//! ```rust,no_run
//! use drex_models::router::{ModelRouter, RouterConfig, TaskKind};
//! use drex_models::backends::OllamaBackend;
//! use drex_config::OllamaConfig;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create and configure the router
//! let mut router = ModelRouter::new();
//!
//! // Register Ollama backend for Fast tasks
//! let ollama_fast = OllamaBackend::from_drex_config(&OllamaConfig {
//!     base_url: "http://localhost:11434".to_string(),
//!     default_model: "gemma3:4b".to_string(),
//!     timeout_seconds: 120,
//! });
//! router.register(TaskKind::Fast, Box::new(ollama_fast));
//!
//! // Register a different Ollama configuration for Main tasks
//! let ollama_main = OllamaBackend::from_drex_config(&OllamaConfig {
//!     base_url: "http://localhost:11434".to_string(),
//!     default_model: "gemma3:12b".to_string(), // Larger model for main tasks
//!     timeout_seconds: 120,
//! });
//! router.register(TaskKind::Main, Box::new(ollama_main));
//!
//! // Route a request to the appropriate backend
//! let backend = router.resolve(TaskKind::Fast)?;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;

use crate::{
    capabilities::{BackendCapability, CapabilitySet},
    error::ModelError,
    ModelBackend,
};

/// Task kinds that map to different model requirements.
///
/// Each task kind represents a distinct use case with specific capability requirements.
/// The router maps these task kinds to appropriate backends based on configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    /// Fast, low-latency tasks (e.g., completions, quick summaries).
    /// Requires: TextGeneration, SystemPrompt
    Fast,

    /// Main/general-purpose tasks (e.g., chat, reasoning).
    /// Requires: TextGeneration, SystemPrompt, ToolCalling
    Main,

    /// Coding tasks requiring structured output and tool use.
    /// Requires: TextGeneration, SystemPrompt, ToolCalling, JsonMode
    Coding,

    /// Vision tasks requiring image understanding.
    /// Requires: TextGeneration, SystemPrompt, Vision
    ///
    /// # Note
    /// Not all backends support vision. If no vision-capable backend
    /// is configured, the router will return an error rather than
    /// silently falling back to a non-vision backend.
    Vision,

    /// Speech/audio tasks requiring audio understanding.
    /// Requires: TextGeneration, SystemPrompt
    ///
    /// # Note
    /// Currently, speech support is limited. Most backends do not
    /// support audio input, so this task kind will likely result
    /// in routing errors unless specifically configured.
    Speech,
}

impl TaskKind {
    /// Get the human-readable name for this task kind.
    pub fn name(&self) -> &'static str {
        match self {
            TaskKind::Fast => "fast",
            TaskKind::Main => "main",
            TaskKind::Coding => "coding",
            TaskKind::Vision => "vision",
            TaskKind::Speech => "speech",
        }
    }

    /// Get the required capabilities for this task kind.
    ///
    /// These are the minimum capabilities a backend must support
    /// to be eligible for routing to this task kind.
    pub fn required_capabilities(&self) -> CapabilitySet {
        match self {
            TaskKind::Fast => CapabilitySet::new(vec![
                BackendCapability::TextGeneration,
                BackendCapability::SystemPrompt,
            ]),
            TaskKind::Main => CapabilitySet::new(vec![
                BackendCapability::TextGeneration,
                BackendCapability::SystemPrompt,
                BackendCapability::ToolCalling,
            ]),
            TaskKind::Coding => CapabilitySet::new(vec![
                BackendCapability::TextGeneration,
                BackendCapability::SystemPrompt,
                BackendCapability::ToolCalling,
                BackendCapability::JsonMode,
            ]),
            TaskKind::Vision => CapabilitySet::new(vec![
                BackendCapability::TextGeneration,
                BackendCapability::SystemPrompt,
                BackendCapability::Vision,
            ]),
            TaskKind::Speech => CapabilitySet::new(vec![
                BackendCapability::TextGeneration,
                BackendCapability::SystemPrompt,
                // Note: Speech-specific capabilities would go here when defined
            ]),
        }
    }

    /// Check if this task kind is likely to be supported by a given capability set.
    pub fn can_be_satisfied_by(&self, capabilities: &CapabilitySet) -> bool {
        let required = self.required_capabilities();
        required.list().iter().all(|cap| capabilities.has(*cap))
    }
}

impl std::fmt::Display for TaskKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A type-erased [`ModelBackend`] stored in the router.
///
/// Backends are boxed trait objects that can be downcast for testing purposes.
/// They do not implement Clone since trait objects cannot be cloned in general.
/// The router stores them directly in a HashMap.
pub type BoxedBackend = Box<dyn ModelBackend>;

/// The Model Router.
///
/// Holds a registry of backends mapped to [`TaskKind`] variants.
/// Provides explicit, deterministic routing without silent fallback.
///
/// # Thread Safety
///
/// The router is designed to be shared across tasks. It uses interior mutability
/// only for registration, while resolution is immutable. In practice, backends
/// should be registered at startup and then the router can be shared immutably.
pub struct ModelRouter {
    /// Mapping from task kind to backend.
    ///
    /// Each task kind maps to exactly one backend. This ensures deterministic
    /// behavior - the same task kind always routes to the same backend.
    routes: HashMap<TaskKind, BoxedBackend>,
}

impl ModelRouter {
    /// Create a new, empty ModelRouter.
    ///
    /// # Example
    ///
    /// ```rust
    /// use drex_models::router::ModelRouter;
    ///
    /// let router = ModelRouter::new();
    /// assert!(!router.has_route_for(drex_models::router::TaskKind::Fast));
    /// ```
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Create a router from a configuration and pre-instantiated backends.
    ///
    /// This is the recommended way to build a router, as it validates
    /// that all configured routes point to valid backends.
    ///
    /// # Arguments
    /// * `config` - The router configuration
    /// * `backends` - A map of backend names to backend instances.
    ///                Note: Backends are consumed from this map. Each backend can only
    ///                be assigned to one task kind. If you need the same backend for
    ///                multiple tasks, wrap it in an Arc and clone the Arc before passing.
    ///
    /// # Errors
    /// Returns an error if the configuration references a backend that doesn't exist,
    /// or if a backend doesn't support the capabilities required by its assigned task.
    pub fn from_config(
        config: &RouterConfig,
        mut backends: HashMap<String, BoxedBackend>,
    ) -> Result<Self, ModelError> {
        let mut router = Self::new();
        let mut used_backends: HashMap<String, ()> = HashMap::new();

        // Validate all routing entries before consuming any backends
        for routing in &config.routing {
            if !backends.contains_key(&routing.backend) {
                return Err(ModelError::invalid_request(format!(
                    "Router configuration references unknown backend: {}",
                    routing.backend
                )));
            }

            // Check for duplicate task kind assignments
            if router.has_route_for(routing.task_kind) {
                return Err(ModelError::invalid_request(format!(
                    "Duplicate task kind '{}' in router configuration",
                    routing.task_kind
                )));
            }

            // Validate backend supports required capabilities
            let required = routing.task_kind.required_capabilities();
            let backend = backends.get(&routing.backend).unwrap();
            let mut unsupported = Vec::new();
            for cap in required.list() {
                if !backend.supports(*cap) {
                    unsupported.push(cap.to_string());
                }
            }

            if !unsupported.is_empty() {
                return Err(ModelError::unsupported(format!(
                    "Backend '{}' does not support capabilities required for task kind '{}': {}",
                    routing.backend,
                    routing.task_kind,
                    unsupported.join(", ")
                )));
            }
        }

        // Now consume backends and build the router
        for routing in &config.routing {
            if let Some(backend) = backends.remove(&routing.backend) {
                used_backends.insert(routing.backend.clone(), ());
                router.register(routing.task_kind, backend);
            }
        }

        Ok(router)
    }

    /// Register a backend for a specific task kind.
    ///
    /// If a backend is already registered for this task kind, it is replaced.
    ///
    /// # Arguments
    /// * `task_kind` - The task kind to register this backend for
    /// * `backend` - The backend implementation
    pub fn register(&mut self, task_kind: TaskKind, backend: BoxedBackend) {
        self.routes.insert(task_kind, backend);
    }

    /// Resolve a task kind to its configured backend.
    ///
    /// # Errors
    /// Returns [`ModelError::Unsupported`] if no backend is configured for the task kind.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use drex_models::router::{ModelRouter, TaskKind};
    /// use drex_models::backends::OllamaBackend;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut router = ModelRouter::new();
    /// // Configure an Ollama backend for Fast tasks
    /// let backend = OllamaBackend::new(drex_models::backends::ollama::OllamaConfig {
    ///     base_url: "http://localhost:11434".to_string(),
    ///     model: "gemma3:4b".to_string(),
    ///     timeout: std::time::Duration::from_secs(120),
    /// });
    /// router.register(TaskKind::Fast, Box::new(backend));
    ///
    /// let resolved = router.resolve(TaskKind::Fast)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn resolve(&self, task_kind: TaskKind) -> Result<&dyn ModelBackend, ModelError> {
        self.routes
            .get(&task_kind)
            .map(|b| b.as_ref())
            .ok_or_else(|| {
                ModelError::unsupported(format!(
                    "No backend configured for task kind: {}. \
                     Configure a backend for this task kind in the router configuration.",
                    task_kind
                ))
            })
    }

    /// Check if a route exists for a task kind.
    pub fn has_route_for(&self, task_kind: TaskKind) -> bool {
        self.routes.contains_key(&task_kind)
    }

    /// Get all configured task kinds.
    pub fn configured_tasks(&self) -> Vec<TaskKind> {
        self.routes.keys().copied().collect()
    }

    /// Remove a route for a task kind.
    pub fn unregister(&mut self, task_kind: TaskKind) -> Option<BoxedBackend> {
        self.routes.remove(&task_kind)
    }

    /// Clear all routes.
    pub fn clear(&mut self) {
        self.routes.clear();
    }

    /// Get the number of configured routes.
    pub fn route_count(&self) -> usize {
        self.routes.len()
    }
}

impl Default for ModelRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for a single routing entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingConfig {
    /// The task kind to route.
    pub task_kind: TaskKind,
    /// The backend name to route to.
    pub backend: String,
}

/// Configuration for the ModelRouter.
///
/// This configuration can be loaded from the application's config files
/// and is separate from the backend-specific configurations.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RouterConfig {
    /// List of task kind to backend name mappings.
    pub routing: Vec<RoutingConfig>,
}

impl RouterConfig {
    /// Create an empty router configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a routing entry.
    pub fn route(mut self, task_kind: TaskKind, backend: impl Into<String>) -> Self {
        self.routing.push(RoutingConfig {
            task_kind,
            backend: backend.into(),
        });
        self
    }

    /// Get the configured backend name for a task kind, if any.
    pub fn get_backend_for(&self, task_kind: TaskKind) -> Option<&str> {
        self.routing
            .iter()
            .find(|r| r.task_kind == task_kind)
            .map(|r| r.backend.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Content;
    use crate::request::ModelRequest;
    use crate::response::ModelResponse;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A mock backend for testing that can be configured with specific capabilities.
    struct MockBackend {
        name: String,
        capabilities: CapabilitySet,
        call_count: Arc<AtomicUsize>,
    }

    impl MockBackend {
        fn new(name: impl Into<String>, capabilities: CapabilitySet) -> Self {
            Self {
                name: name.into(),
                capabilities,
                call_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ModelBackend for MockBackend {
        async fn complete(&self, _request: ModelRequest) -> Result<ModelResponse, ModelError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(ModelResponse::new(
                "test-id",
                &self.name,
                "mock",
                Content::text("test response"),
            ))
        }

        fn supports(&self, capability: BackendCapability) -> bool {
            self.capabilities.has(capability)
        }

        fn provider_name(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            &self.name
        }
    }

    /// A cloneable mock backend wrapper.
    #[derive(Clone)]
    struct CloneableMockBackend {
        inner: Arc<MockBackend>,
    }

    impl CloneableMockBackend {
        fn new(name: impl Into<String>, capabilities: CapabilitySet) -> Self {
            Self {
                inner: Arc::new(MockBackend::new(name, capabilities)),
            }
        }

        fn call_count(&self) -> usize {
            self.inner.call_count()
        }
    }

    #[async_trait]
    impl ModelBackend for CloneableMockBackend {
        async fn complete(&self, request: ModelRequest) -> Result<ModelResponse, ModelError> {
            self.inner.complete(request).await
        }

        fn supports(&self, capability: BackendCapability) -> bool {
            self.inner.supports(capability)
        }

        fn provider_name(&self) -> &str {
            self.inner.provider_name()
        }

        fn model(&self) -> &str {
            self.inner.model()
        }
    }

    // =========================================================================
    // TaskKind Tests
    // =========================================================================

    #[test]
    fn task_kind_names() {
        assert_eq!(TaskKind::Fast.name(), "fast");
        assert_eq!(TaskKind::Main.name(), "main");
        assert_eq!(TaskKind::Coding.name(), "coding");
        assert_eq!(TaskKind::Vision.name(), "vision");
        assert_eq!(TaskKind::Speech.name(), "speech");
    }

    #[test]
    fn task_kind_display() {
        assert_eq!(TaskKind::Fast.to_string(), "fast");
        assert_eq!(TaskKind::Main.to_string(), "main");
    }

    #[test]
    fn task_kind_required_capabilities() {
        // Fast requires minimal capabilities
        let fast = TaskKind::Fast.required_capabilities();
        assert!(fast.has(BackendCapability::TextGeneration));
        assert!(fast.has(BackendCapability::SystemPrompt));
        assert!(!fast.has(BackendCapability::ToolCalling));

        // Main requires tool calling
        let main = TaskKind::Main.required_capabilities();
        assert!(main.has(BackendCapability::ToolCalling));

        // Coding requires JSON mode
        let coding = TaskKind::Coding.required_capabilities();
        assert!(coding.has(BackendCapability::JsonMode));

        // Vision requires vision capability
        let vision = TaskKind::Vision.required_capabilities();
        assert!(vision.has(BackendCapability::Vision));
    }

    #[test]
    fn task_kind_can_be_satisfied() {
        let full_capabilities = CapabilitySet::all();
        assert!(TaskKind::Fast.can_be_satisfied_by(&full_capabilities));
        assert!(TaskKind::Main.can_be_satisfied_by(&full_capabilities));
        assert!(TaskKind::Coding.can_be_satisfied_by(&full_capabilities));
        assert!(TaskKind::Vision.can_be_satisfied_by(&full_capabilities));

        // Text only cannot satisfy vision
        let text_only = CapabilitySet::new(vec![
            BackendCapability::TextGeneration,
            BackendCapability::SystemPrompt,
        ]);
        assert!(!TaskKind::Vision.can_be_satisfied_by(&text_only));
    }

    // =========================================================================
    // ModelRouter Tests - Registration and Resolution
    // =========================================================================

    #[test]
    fn router_new_is_empty() {
        let router = ModelRouter::new();
        assert!(!router.has_route_for(TaskKind::Fast));
        assert!(!router.has_route_for(TaskKind::Main));
        assert_eq!(router.route_count(), 0);
    }

    #[test]
    fn router_register_and_resolve() {
        let mut router = ModelRouter::new();
        let backend = CloneableMockBackend::new("test-model", CapabilitySet::all());

        router.register(TaskKind::Fast, Box::new(backend.clone()));

        assert!(router.has_route_for(TaskKind::Fast));
        assert_eq!(router.route_count(), 1);

        let resolved = router.resolve(TaskKind::Fast).unwrap();
        assert_eq!(resolved.model(), "test-model");
    }

    #[test]
    fn router_resolve_missing_produces_error() {
        let router = ModelRouter::new();

        let result = router.resolve(TaskKind::Fast);
        assert!(result.is_err());

        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error, got Ok"),
        };
        assert!(matches!(err, ModelError::Unsupported(_)));
        let msg = err.to_string();
        assert!(msg.contains("No backend configured"));
        assert!(msg.contains("task kind: fast"));
    }

    #[test]
    fn router_resolve_missing_shows_correct_task_kind() {
        let router = ModelRouter::new();

        // Test each task kind produces the right error message
        for task in [TaskKind::Main, TaskKind::Coding, TaskKind::Vision, TaskKind::Speech] {
            let err = match router.resolve(task) {
                Err(e) => e,
                Ok(_) => panic!("Expected error for task {:?}", task),
            };
            let msg = err.to_string();
            assert!(msg.contains(&task.to_string()), "Error should mention {}", task);
        }
    }

    #[test]
    fn router_unregister() {
        let mut router = ModelRouter::new();
        let backend = CloneableMockBackend::new("test", CapabilitySet::all());

        router.register(TaskKind::Fast, Box::new(backend));
        assert!(router.has_route_for(TaskKind::Fast));

        router.unregister(TaskKind::Fast);
        assert!(!router.has_route_for(TaskKind::Fast));
        assert_eq!(router.route_count(), 0);
    }

    #[test]
    fn router_clear() {
        let mut router = ModelRouter::new();
        let backend = CloneableMockBackend::new("test", CapabilitySet::all());

        router.register(TaskKind::Fast, Box::new(backend.clone()));
        router.register(TaskKind::Main, Box::new(backend));
        assert_eq!(router.route_count(), 2);

        router.clear();
        assert_eq!(router.route_count(), 0);
        assert!(!router.has_route_for(TaskKind::Fast));
        assert!(!router.has_route_for(TaskKind::Main));
    }

    #[test]
    fn router_configured_tasks() {
        let mut router = ModelRouter::new();
        let backend = CloneableMockBackend::new("test", CapabilitySet::all());

        router.register(TaskKind::Fast, Box::new(backend.clone()));
        router.register(TaskKind::Main, Box::new(backend));

        let tasks = router.configured_tasks();
        assert_eq!(tasks.len(), 2);
        assert!(tasks.contains(&TaskKind::Fast));
        assert!(tasks.contains(&TaskKind::Main));
    }

    #[test]
    fn router_register_overwrites() {
        let mut router = ModelRouter::new();
        let backend1 = CloneableMockBackend::new("first", CapabilitySet::all());
        let backend2 = CloneableMockBackend::new("second", CapabilitySet::all());

        router.register(TaskKind::Fast, Box::new(backend1));
        router.register(TaskKind::Fast, Box::new(backend2));

        let resolved = router.resolve(TaskKind::Fast).unwrap();
        assert_eq!(resolved.model(), "second");
    }

    // =========================================================================
    // ModelRouter Tests - Multiple Task Kinds
    // =========================================================================

    fn create_test_router() -> (ModelRouter, CloneableMockBackend, CloneableMockBackend) {
        let mut router = ModelRouter::new();

        // Ollama-like backend (supports text, tools, but not vision)
        let ollama_like = CloneableMockBackend::new(
            "gemma3:4b",
            CapabilitySet::new(vec![
                BackendCapability::TextGeneration,
                BackendCapability::SystemPrompt,
                BackendCapability::ToolCalling,
                BackendCapability::StopSequences,
            ]),
        );

        // Hypothetical vision backend
        let vision_backend = CloneableMockBackend::new(
            "vision-model",
            CapabilitySet::new(vec![
                BackendCapability::TextGeneration,
                BackendCapability::SystemPrompt,
                BackendCapability::Vision,
            ]),
        );

        router.register(TaskKind::Fast, Box::new(ollama_like.clone()));
        router.register(TaskKind::Main, Box::new(ollama_like.clone()));
        router.register(TaskKind::Coding, Box::new(ollama_like.clone()));
        router.register(TaskKind::Vision, Box::new(vision_backend.clone()));

        (router, ollama_like, vision_backend)
    }

    #[test]
    fn router_routes_to_appropriate_backend() {
        let (router, _, vision_backend) = create_test_router();

        // Fast/Main/Coding should go to Ollama-like backend
        let fast_resolved = router.resolve(TaskKind::Fast).unwrap();
        assert_eq!(fast_resolved.model(), "gemma3:4b");

        let main_resolved = router.resolve(TaskKind::Main).unwrap();
        assert_eq!(main_resolved.model(), "gemma3:4b");

        let coding_resolved = router.resolve(TaskKind::Coding).unwrap();
        assert_eq!(coding_resolved.model(), "gemma3:4b");

        // Vision should go to vision backend
        let vision_resolved = router.resolve(TaskKind::Vision).unwrap();
        assert_eq!(vision_resolved.model(), "vision-model");
    }

    #[test]
    fn router_demonstrates_no_silent_fallback() {
        let mut router = ModelRouter::new();

        // Only register Fast, try to resolve Main
        let backend = CloneableMockBackend::new("fast-backend", CapabilitySet::all());
        router.register(TaskKind::Fast, Box::new(backend));

        // Resolving Fast works
        assert!(router.resolve(TaskKind::Fast).is_ok());

        // Resolving Main fails - no fallback to Fast happens
        let err = match router.resolve(TaskKind::Main) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unregistered task"),
        };
        assert!(err.to_string().contains("No backend configured"));
    }

    // =========================================================================
    // ModelRouter Tests - Configuration-based
    // =========================================================================

    #[test]
    fn router_from_config_success() {
        let config = RouterConfig::new()
            .route(TaskKind::Fast, "fast-backend")
            .route(TaskKind::Main, "main-backend");

        let mut backends: HashMap<String, BoxedBackend> = HashMap::new();
        backends.insert(
            "fast-backend".to_string(),
            Box::new(CloneableMockBackend::new(
                "fast",
                CapabilitySet::new(vec![
                    BackendCapability::TextGeneration,
                    BackendCapability::SystemPrompt,
                ]),
            )),
        );
        backends.insert(
            "main-backend".to_string(),
            Box::new(CloneableMockBackend::new(
                "main",
                CapabilitySet::new(vec![
                    BackendCapability::TextGeneration,
                    BackendCapability::SystemPrompt,
                    BackendCapability::ToolCalling,
                ]),
            )),
        );

        let router = ModelRouter::from_config(&config, backends).unwrap();

        assert!(router.has_route_for(TaskKind::Fast));
        assert!(router.has_route_for(TaskKind::Main));
        assert!(!router.has_route_for(TaskKind::Coding));
    }

    #[test]
    fn router_from_config_missing_backend() {
        let config = RouterConfig::new().route(TaskKind::Fast, "nonexistent");
        let backends: HashMap<String, BoxedBackend> = HashMap::new();

        let result = ModelRouter::from_config(&config, backends);
        assert!(result.is_err());

        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error, got Ok"),
        };
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn router_from_config_unsupported_capability() {
        // Config tries to route Coding to a backend that doesn't support JsonMode
        let config = RouterConfig::new().route(TaskKind::Coding, "llama-backend");

        let mut backends: HashMap<String, BoxedBackend> = HashMap::new();
        backends.insert(
            "llama-backend".to_string(),
            Box::new(CloneableMockBackend::new(
                "llama",
                CapabilitySet::new(vec![
                    BackendCapability::TextGeneration,
                    BackendCapability::SystemPrompt,
                    BackendCapability::ToolCalling,
                    // Note: Missing JsonMode
                ]),
            )),
        );

        let result = ModelRouter::from_config(&config, backends);
        assert!(result.is_err());

        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error, got Ok"),
        };
        let msg = err.to_string();
        assert!(msg.contains("llama-backend"));
        assert!(msg.contains("coding"));
        assert!(msg.contains("json_mode"));
    }

    // =========================================================================
    // RouterConfig Tests
    // =========================================================================

    #[test]
    fn router_config_builder() {
        let config = RouterConfig::new()
            .route(TaskKind::Fast, "ollama")
            .route(TaskKind::Main, "openai");

        assert_eq!(config.routing.len(), 2);
        assert_eq!(config.get_backend_for(TaskKind::Fast), Some("ollama"));
        assert_eq!(config.get_backend_for(TaskKind::Main), Some("openai"));
        assert_eq!(config.get_backend_for(TaskKind::Coding), None);
    }

    #[test]
    fn router_config_serialization() {
        let config = RouterConfig::new()
            .route(TaskKind::Fast, "ollama")
            .route(TaskKind::Main, "ollama");

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RouterConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.routing.len(), 2);
        assert_eq!(deserialized.get_backend_for(TaskKind::Fast), Some("ollama"));
    }

    #[test]
    fn router_config_deserialize_from_json() {
        let json = r#"
            {
                "routing": [
                    {"task_kind": "fast", "backend": "ollama"},
                    {"task_kind": "vision", "backend": "gpt-4o"}
                ]
            }
        "#;

        let config: RouterConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.routing.len(), 2);
        assert_eq!(config.get_backend_for(TaskKind::Fast), Some("ollama"));
        assert_eq!(config.get_backend_for(TaskKind::Vision), Some("gpt-4o"));
    }
}
