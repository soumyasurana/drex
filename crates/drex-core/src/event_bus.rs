//! Event Bus - Internal pub/sub system for autonomous triggers
//!
//! Provides a typed event bus for internal communication between
//! Drex components. Supports:
//! - Publishing events
//! - Subscribing to event types
//! - Filtering events
//! - Async event processing
//!
//! # Event Flow
//!
//! Publisher -> EventBus -> Matching Subscribers -> Handlers
//!
//! # Use Cases
//!
//! - Autonomous triggers (file changes, timers, system events)
//! - Cross-component communication
//! - Audit logging
//! - Real-time monitoring

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Event severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSeverity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

/// Core event trait.
pub trait Event: Send + Sync + Any {
    /// Event type identifier.
    fn event_type(&self) -> &'static str;

    /// Timestamp of the event.
    fn timestamp(&self) -> SystemTime;

    /// Event severity.
    fn severity(&self) -> EventSeverity;

    /// Human-readable description.
    fn description(&self) -> String;
}

/// Wrapper for event storage.
pub struct EventWrapper {
    event: Box<dyn Event>,
}

impl EventWrapper {
    /// Create a new event wrapper.
    pub fn new(event: Box<dyn Event>) -> Self {
        Self { event }
    }

    /// Get the event type.
    pub fn event_type(&self) -> &'static str {
        self.event.event_type()
    }

    /// Downcast to specific event type.
    pub fn downcast_ref<T: Event>(&self) -> Option<&T> {
        self.event.as_any().downcast_ref::<T>()
    }
}

/// Required for Any trait.
impl dyn Event {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Subscription handler trait.
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an event.
    async fn handle(&self, event: &EventWrapper);

    /// Event types this handler subscribes to.
    fn subscribed_types(&self) -> Vec<&'static str>;
}

type HandlerBox = Box<dyn EventHandler>;

/// Event bus configuration.
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// Channel buffer size.
    pub channel_buffer: usize,
    /// Max pending events.
    pub max_pending: usize,
    /// Handler timeout.
    pub handler_timeout: Duration,
    /// Enable event persistence.
    pub persist_events: bool,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            channel_buffer: 1000,
            max_pending: 10000,
            handler_timeout: Duration::from_secs(30),
            persist_events: false,
        }
    }
}

/// Event bus statistics.
#[derive(Debug, Clone, Default)]
pub struct EventBusStats {
    /// Total events published.
    pub events_published: u64,
    /// Total events delivered.
    pub events_delivered: u64,
    /// Total events dropped.
    pub events_dropped: u64,
    /// Current subscribers.
    pub subscriber_count: usize,
    /// Max latency (ms).
    pub max_latency_ms: u64,
}

/// Event bus for pub/sub communication.
pub struct EventBus {
    config: EventBusConfig,
    tx: mpsc::Sender<EventWrapper>,
    rx: RwLock<mpsc::Receiver<EventWrapper>>,
    handlers: RwLock<HashMap<&'static str, Vec<Arc<HandlerBox>>>>,
    stats: RwLock<EventBusStats>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl EventBus {
    /// Create a new event bus.
    pub fn new(config: EventBusConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.channel_buffer);

        Self {
            config,
            tx,
            rx: RwLock::new(rx),
            handlers: RwLock::new(HashMap::new()),
            stats: RwLock::new(EventBusStats::default()),
            shutdown_tx: None,
        }
    }

    /// Publish an event to the bus.
    pub async fn publish<E: Event + 'static>(&self, event: E) -> Result<(), &'static str> {
        let wrapper = EventWrapper::new(Box::new(event));

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.events_published += 1;

            if self.tx.send(wrapper).await.is_ok() {
                stats.events_delivered += 1;
                Ok(())
            } else {
                stats.events_dropped += 1;
                Err("Event channel full")
            }
        }
    }

    /// Subscribe to event types with a handler.
    pub async fn subscribe(&self, handler: HandlerBox) {
        let types = handler.subscribed_types();
        let handler_arc = Arc::new(handler);

        let mut handlers = self.handlers.write().await;
        for event_type in types {
            handlers
                .entry(event_type)
                .or_insert_with(Vec::new)
                .push(handler_arc.clone());
        }

        let mut stats = self.stats.write().await;
        stats.subscriber_count += 1;

        info!("Subscribed handler to {:?}", handler_arc.subscribed_types());
    }

    /// Unsubscribe all handlers of a type.
    pub async fn unsubscribe(&self, event_type: &'static str) {
        let mut handlers = self.handlers.write().await;
        handlers.remove(event_type);

        let mut stats = self.stats.write().await;
        if stats.subscriber_count > 0 {
            stats.subscriber_count -= 1;
        }
    }

    /// Start processing events.
    pub fn start(&self) -> JoinHandle<()> {
        let rx_clone_task = async move {
            // Event processing loop would go here
            // For now, just consume the receiver
            loop {
                // In full implementation, would dispatch to handlers
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };

        tokio::spawn(rx_clone_task)
    }

    /// Get current statistics.
    pub async fn stats(&self) -> EventBusStats {
        self.stats.read().await.clone()
    }

    /// Shutdown the event bus.
    pub async fn shutdown(&self) {
        info!("Shutting down event bus");
        // In real implementation, would signal the event loop to stop
        // For now, just log the intention
    }
}

/// Autonomous trigger types.
#[derive(Debug, Clone)]
pub enum TriggerType {
    /// Timer-based trigger.
    Timer { interval_secs: u64 },
    /// File system watcher.
    FileWatcher { path: String, pattern: String },
    /// System event.
    SystemEvent { event_name: String },
    /// External webhook.
    Webhook { endpoint: String },
    /// Memory-based trigger.
    MemoryTrigger { query: String },
}

/// Autonomous trigger definition.
#[derive(Debug, Clone)]
pub struct AutonomousTrigger {
    /// Trigger ID.
    pub id: String,
    /// Trigger name.
    pub name: String,
    /// What type of trigger.
    pub trigger_type: TriggerType,
    /// Whether enabled.
    pub enabled: bool,
    /// Action to execute.
    pub action: String,
    /// Max executions per hour (rate limiting).
    pub max_per_hour: u32,
    /// Last triggered time.
    pub last_triggered: Option<SystemTime>,
}

/// Trigger manager.
pub struct TriggerManager {
    triggers: RwLock<HashMap<String, AutonomousTrigger>>,
    event_bus: Arc<EventBus>,
}

impl TriggerManager {
    /// Create a new trigger manager.
    pub fn new(event_bus: Arc<EventBus>) -> Self {
        Self {
            triggers: RwLock::new(HashMap::new()),
            event_bus,
        }
    }

    /// Register a new trigger.
    pub async fn register(&self, trigger: AutonomousTrigger) {
        let trigger_id = trigger.id.clone();
        let mut triggers = self.triggers.write().await;
        triggers.insert(trigger_id.clone(), trigger);
        info!("Registered trigger: {}", trigger_id);
    }

    /// Unregister a trigger.
    pub async fn unregister(&self, trigger_id: &str) {
        let mut triggers = self.triggers.write().await;
        triggers.remove(trigger_id);
        info!("Unregistered trigger: {}", trigger_id);
    }

    /// Get a trigger by ID.
    pub async fn get(&self, trigger_id: &str) -> Option<AutonomousTrigger> {
        let triggers = self.triggers.read().await;
        triggers.get(trigger_id).cloned()
    }

    /// List all triggers.
    pub async fn list(&self) -> Vec<AutonomousTrigger> {
        let triggers = self.triggers.read().await;
        triggers.values().cloned().collect()
    }

    /// Enable/disable a trigger.
    pub async fn set_enabled(&self, trigger_id: &str, enabled: bool) {
        let mut triggers = self.triggers.write().await;
        if let Some(trigger) = triggers.get_mut(trigger_id) {
            trigger.enabled = enabled;
            info!("Trigger {}: enabled={}", trigger_id, enabled);
        }
    }

    /// Check and execute due triggers.
    pub async fn check_triggers(&self) -> Vec<(String, String)> {
        let triggers = self.triggers.read().await;
        let mut executed = Vec::new();

        for trigger in triggers.values() {
            if !trigger.enabled {
                continue;
            }

            match &trigger.trigger_type {
                TriggerType::Timer { interval_secs } => {
                    let should_trigger = trigger
                        .last_triggered
                        .map(|t| {
                            t.elapsed().unwrap_or(Duration::MAX)
                                >= Duration::from_secs(*interval_secs)
                        })
                        .unwrap_or(true);

                    if should_trigger {
                        executed.push((trigger.id.clone(), trigger.action.clone()));
                    }
                }
                _ => {}
            }
        }

        executed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestEvent {
        timestamp: SystemTime,
        message: String,
    }

    impl Event for TestEvent {
        fn event_type(&self) -> &'static str {
            "test.event"
        }

        fn timestamp(&self) -> SystemTime {
            self.timestamp
        }

        fn severity(&self) -> EventSeverity {
            EventSeverity::Info
        }

        fn description(&self) -> String {
            self.message.clone()
        }
    }

    struct TestHandler;

    #[async_trait::async_trait]
    impl EventHandler for TestHandler {
        async fn handle(&self, _event: &EventWrapper) {
            // Handler implementation
        }

        fn subscribed_types(&self) -> Vec<&'static str> {
            vec!["test.event"]
        }
    }

    #[tokio::test]
    async fn test_event_bus_publish() {
        let config = EventBusConfig::default();
        let bus = EventBus::new(config);

        let event = TestEvent {
            timestamp: SystemTime::now(),
            message: "Hello".to_string(),
        };

        assert!(bus.publish(event).await.is_ok());
    }

    #[tokio::test]
    async fn test_event_bus_stats() {
        let config = EventBusConfig::default();
        let bus = EventBus::new(config);

        let stats = bus.stats().await;
        assert_eq!(stats.events_published, 0);
    }

    #[test]
    fn test_event_bus_config_default() {
        let config = EventBusConfig::default();
        assert_eq!(config.channel_buffer, 1000);
        assert!(!config.persist_events);
    }

    #[tokio::test]
    async fn test_trigger_manager_register() {
        let event_bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let manager = TriggerManager::new(event_bus);

        let trigger = AutonomousTrigger {
            id: "test-1".to_string(),
            name: "Test Trigger".to_string(),
            trigger_type: TriggerType::Timer { interval_secs: 60 },
            enabled: true,
            action: "echo test".to_string(),
            max_per_hour: 10,
            last_triggered: None,
        };

        manager.register(trigger).await;

        let retrieved = manager.get("test-1").await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_trigger_manager_check() {
        let event_bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let manager = TriggerManager::new(event_bus);

        let trigger = AutonomousTrigger {
            id: "test-1".to_string(),
            name: "Test Trigger".to_string(),
            trigger_type: TriggerType::Timer { interval_secs: 0 },
            enabled: true,
            action: "echo test".to_string(),
            max_per_hour: 10,
            last_triggered: None,
        };

        manager.register(trigger).await;

        let due = manager.check_triggers().await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].0, "test-1");
    }

    #[tokio::test]
    async fn test_trigger_disabled() {
        let event_bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let manager = TriggerManager::new(event_bus);

        let trigger = AutonomousTrigger {
            id: "test-1".to_string(),
            name: "Test Trigger".to_string(),
            trigger_type: TriggerType::Timer { interval_secs: 0 },
            enabled: false,
            action: "echo test".to_string(),
            max_per_hour: 10,
            last_triggered: None,
        };

        manager.register(trigger).await;

        let due = manager.check_triggers().await;
        assert!(due.is_empty());
    }
}
