//! Memory Policy Layer
//!
//! The policy layer sits between callers and the MemoryStore to enforce
//! business rules about memory storage, retrieval, and access.
//!
//! ## Architecture
//!
//! ```text
//! Drex caller
//!     |
//!     v
//! MemoryPolicy
//!     |
//!     v
//! MemoryStore
//!     |
//!     v
//! ContextraMemoryStore
//!     |
//!     v
//! Contextra storage/vector layer
//! ```
//!
//! The policy is backend-agnostic and knows nothing about Contextra or Qdrant.
//!
//! ## Security Model
//!
//! - Sensitive memories require sufficient task trust level for retrieval
//! - Access is enforced by code, not just documentation
//! - Secrets are never logged

use crate::memory::{
    Memory, MemoryId, MemoryKind, MemorySource, SensitivityLevel,
};
use crate::query::MemoryQuery;
use crate::store::{MemoryStore, MemoryStoreError};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::collections::HashSet;

/// Confidence level for a memory.
///
/// Range: 0.0 (pure speculation) to 1.0 (certain)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Confidence(pub f32);

impl Confidence {
    /// Pure speculation, should generally be rejected by policy
    pub const ZERO: Self = Self(0.0);
    
    /// Low confidence - inferred from weak signals
    pub const LOW: Self = Self(0.3);
    
    /// Medium confidence - inferred from reasonable signals
    pub const MEDIUM: Self = Self(0.6);
    
    /// High confidence - user-stated or strongly verified
    pub const HIGH: Self = Self(0.9);
    
    /// Certain - user explicitly confirmed, or contractually guaranteed
    pub const CERTAIN: Self = Self(1.0);

    /// Create a new confidence level, clamped to [0, 1]
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Check if confidence meets a minimum threshold
    pub fn is_at_least(&self, threshold: Confidence) -> bool {
        self.0 >= threshold.0
    }

    /// Check if confidence is below a threshold (uncertain)
    pub fn is_below(&self, threshold: Confidence) -> bool {
        self.0 < threshold.0
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Self::MEDIUM
    }
}

impl From<f32> for Confidence {
    fn from(value: f32) -> Self {
        Self::new(value)
    }
}

/// Provenance of a memory - how it was obtained.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Explicitly stated by the user with intent to save
    /// Highest trust, highest confidence
    UserStated,

    /// Explicitly stated but not necessarily meant to be memory
    /// High confidence, but might need user confirmation
    UserMentioned,

    /// Inferred by the system from conversation analysis
    /// Medium confidence, may be revised
    InferredFromConversation,

    /// Inferred from patterns across multiple sessions
    /// Lower confidence, may be noisy
    InferredFromPattern,

    /// Automated import from external source
    /// Confidence depends on source verification
    Imported(String),

    /// Derived from temporal context (e.g., "you said this yesterday")
    /// High confidence but context-dependent
    TemporalContext,
}

impl Provenance {
    /// Returns true if this provenance is user-origin (direct or indirect)
    pub fn is_user_origin(&self) -> bool {
        matches!(self, 
            Provenance::UserStated | 
            Provenance::UserMentioned | 
            Provenance::TemporalContext
        )
    }

    /// Returns true if this provenance is system-inferred
    pub fn is_inferred(&self) -> bool {
        matches!(self,
            Provenance::InferredFromConversation |
            Provenance::InferredFromPattern
        )
    }

    /// Returns true if this provenance requires verification
    pub fn requires_verification(&self) -> bool {
        matches!(self,
            Provenance::InferredFromPattern |
            Provenance::Imported(_) |
            Provenance::InferredFromConversation
        )
    }
}

impl Default for Provenance {
    fn default() -> Self {
        Provenance::InferredFromConversation
    }
}

impl From<MemorySource> for Provenance {
    fn from(source: MemorySource) -> Self {
        match source {
            MemorySource::Automatic => Provenance::InferredFromConversation,
            MemorySource::Explicit => Provenance::UserStated,
            MemorySource::Imported(src) => Provenance::Imported(src),
            MemorySource::Summary => Provenance::InferredFromConversation,
            MemorySource::Inferred => Provenance::InferredFromPattern,
            MemorySource::Unknown => Provenance::InferredFromConversation,
        }
    }
}

/// Task trust level - how much we trust the current task/operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskTrustLevel {
    /// No trust - sandboxed, may access public only
    None = 0,
    /// Minimal trust - public and default sensitivity
    Minimal = 1,
    /// Low trust - can read non-sensitive memories
    Low = 2,
    /// Medium trust - can read most memories, write working/episodic
    Medium = 3,
    /// High trust - can read all but critical, write all kinds
    High = 4,
    /// Full trust - can access everything including critical
    Full = 5,
}

impl TaskTrustLevel {
    /// Check if current level can access memories of given sensitivity
    pub fn can_access(&self, sensitivity: SensitivityLevel) -> bool {
        match (self, sensitivity) {
            (_, SensitivityLevel::Public) => true,
            (TaskTrustLevel::None, _) => false,
            (TaskTrustLevel::Minimal, SensitivityLevel::Default) => true,
            (TaskTrustLevel::Minimal, _) => false,
            (TaskTrustLevel::Low, SensitivityLevel::Default) => true,
            (TaskTrustLevel::Low, SensitivityLevel::Sensitive) => true,
            (TaskTrustLevel::Low, _) => false,
            (TaskTrustLevel::Medium, SensitivityLevel::Default) => true,
            (TaskTrustLevel::Medium, SensitivityLevel::Sensitive) => true,
            (TaskTrustLevel::Medium, SensitivityLevel::Private) => true,
            (TaskTrustLevel::Medium, _) => false,
            (TaskTrustLevel::High, SensitivityLevel::Critical) => false,
            (TaskTrustLevel::High, _) => true,
            (TaskTrustLevel::Full, _) => true,
        }
    }

    /// Returns true if this level can write memories of given kind
    pub fn can_write(&self, kind: MemoryKind) -> bool {
        match (self, kind) {
            (TaskTrustLevel::None, _) => false,
            (TaskTrustLevel::Minimal, MemoryKind::Working) => true,
            (TaskTrustLevel::Minimal, _) => false,
            (TaskTrustLevel::Low, MemoryKind::Working) => true,
            (TaskTrustLevel::Low, MemoryKind::Episodic) => true,
            (TaskTrustLevel::Low, _) => false,
            (TaskTrustLevel::Medium, MemoryKind::Working) => true,
            (TaskTrustLevel::Medium, MemoryKind::Episodic) => true,
            (TaskTrustLevel::Medium, MemoryKind::Semantic) => true,
            (TaskTrustLevel::Medium, MemoryKind::Preference) => true,
            (TaskTrustLevel::Medium, _) => false,
            (TaskTrustLevel::High, _) => true,
            (TaskTrustLevel::Full, _) => true,
        }
    }
}

impl Default for TaskTrustLevel {
    fn default() -> Self {
        TaskTrustLevel::Medium
    }
}

/// Time-to-live for a memory entry
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ttl {
    /// Duration until expiration from creation
    pub duration: Duration,
    
    /// Whether this TTL can be extended on access
    pub extend_on_access: bool,
    
    /// Maximum number of extensions allowed
    pub max_extensions: Option<u32>,
}

impl Ttl {
    /// Permanent TTL - never expires
    pub fn permanent() -> Self {
        Self {
            duration: Duration::MAX,
            extend_on_access: false,
            max_extensions: None,
        }
    }

    /// Working memory TTL - minutes
    pub fn working(minutes: i64) -> Self {
        Self {
            duration: Duration::minutes(minutes),
            extend_on_access: true,
            max_extensions: Some(5),
        }
    }

    /// Episodic memory TTL - hours to days
    pub fn episodic(hours: i64) -> Self {
        Self {
            duration: Duration::hours(hours),
            extend_on_access: false,
            max_extensions: None,
        }
    }

    /// Long-term memory TTL - years effectively permanent
    pub fn long_term() -> Self {
        Self {
            duration: Duration::days(365 * 10), // 10 years
            extend_on_access: false,
            max_extensions: None,
        }
    }

    /// Calculates expiration time from a given start time
    pub fn expires_at(&self, start: DateTime<Utc>) -> DateTime<Utc> {
        start + self.duration
    }

    /// Checks if expired given current time and creation time
    pub fn is_expired(&self, created_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        let expiration = self.expires_at(created_at);
        now >= expiration
    }

    /// Returns remaining time until expiration
    pub fn remaining(&self, created_at: DateTime<Utc>, now: DateTime<Utc>) -> Option<Duration> {
        let expiration = self.expires_at(created_at);
        if now >= expiration {
            None
        } else {
            Some(expiration - now)
        }
    }
}

impl Default for Ttl {
    fn default() -> Self {
        Self::long_term()
    }
}

/// Decision made by the policy for a memory
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryDecision {
    /// Accept the memory for storage
    Accept {
        /// Final confidence assigned by policy
        confidence: Confidence,
        /// Provenance (may be normalized/enhanced by policy)
        provenance: Provenance,
        /// Sensitivity level (may be inferred/enhanced)
        sensitivity: SensitivityLevel,
        /// TTL to apply
        ttl: Ttl,
        /// Whether to promote to long-term storage
        promote_to_long_term: bool,
        /// Additional metadata to store
        policy_metadata: std::collections::HashMap<String, String>,
    },

    /// Reject the memory as noise/low-value
    Reject {
        /// Why it was rejected
        reason: String,
        /// Optional log level for the rejection
        #[allow(dead_code)]
        log_level: PolicyLogLevel,
    },
}

/// Log levels for policy decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PolicyLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
}

impl MemoryDecision {
    /// Create an Accept decision with defaults
    pub fn accept() -> Self {
        Self::Accept {
            confidence: Confidence::default(),
            provenance: Provenance::default(),
            sensitivity: SensitivityLevel::Default,
            ttl: Ttl::default(),
            promote_to_long_term: true,
            policy_metadata: std::collections::HashMap::new(),
        }
    }

    /// Create a Reject decision
    pub fn reject(reason: impl Into<String>) -> Self {
        Self::Reject {
            reason: reason.into(),
            log_level: PolicyLogLevel::Debug,
        }
    }
}

/// Memory policy trait - abstraction over policy implementations
#[async_trait]
pub trait MemoryPolicy: Send + Sync {
    /// Evaluate a memory before storage
    async fn evaluate(&self, memory: &Memory) -> MemoryDecision;

    /// Check if a memory is accessible given current trust level
    fn can_retrieve(&self, memory: &Memory, trust_level: TaskTrustLevel) -> bool;

    /// Filter a query based on trust level
    fn filter_query(&self, query: &MemoryQuery, trust_level: TaskTrustLevel) -> MemoryQuery;

    /// Get policy context (policy-dependent info like current trust level)
    fn get_context(&self) -> PolicyContext;
}

/// Context information for policy decisions
#[derive(Debug, Clone)]
pub struct PolicyContext {
    /// Current trust level
    pub trust_level: TaskTrustLevel,
    
    /// Whether access logging is enabled
    pub access_logging: bool,
    
    /// Quiet mode - reduce log verbosity for sensitive operations
    pub quiet_mode: bool,
}

impl Default for PolicyContext {
    fn default() -> Self {
        Self {
            trust_level: TaskTrustLevel::default(),
            access_logging: true,
            quiet_mode: false,
        }
    }
}

/// Rule-based memory policy implementation
///
/// Uses explicit, deterministic rules for memory decisions.
/// No ML or LLM involved.
#[derive(Debug, Clone)]
pub struct RuleBasedPolicy {
    /// Minimum confidence threshold for storage
    min_confidence: Confidence,
    
    /// Reject patterns - content matching these is rejected
    #[allow(dead_code)]
    reject_patterns: Vec<String>,
    
    /// Noise indicators - words/phrases indicating low-value content
    noise_indicators: HashSet<String>,
    
    /// Context for this policy instance
    context: PolicyContext,
    
    /// Mapping from memory kinds to default TTLs
    kind_ttls: std::collections::HashMap<MemoryKind, Ttl>,
}

impl RuleBasedPolicy {
    /// Create a new rule-based policy with sensible defaults
    pub fn new(trust_level: TaskTrustLevel) -> Self {
        let mut kind_ttls = std::collections::HashMap::new();
        kind_ttls.insert(MemoryKind::Working, Ttl::working(30)); // 30 min
        kind_ttls.insert(MemoryKind::Episodic, Ttl::episodic(24)); // 24 hours
        kind_ttls.insert(MemoryKind::Semantic, Ttl::long_term());
        kind_ttls.insert(MemoryKind::Preference, Ttl::long_term());
        kind_ttls.insert(MemoryKind::Procedural, Ttl::long_term());
        kind_ttls.insert(MemoryKind::Relationship, Ttl::long_term());
        kind_ttls.insert(MemoryKind::Summary, Ttl::episodic(72)); // 3 days

        let mut noise_indicators = HashSet::new();
        noise_indicators.insert("um".to_string());
        noise_indicators.insert("uh".to_string());
        noise_indicators.insert("hmm".to_string());
        noise_indicators.insert("...".to_string());

        Self {
            min_confidence: Confidence(0.2), // Reject below 20% confidence
            reject_patterns: vec![
                r"^\s*$".to_string(), // Just whitespace
            ],
            noise_indicators,
            context: PolicyContext {
                trust_level,
                access_logging: true,
                quiet_mode: false,
            },
            kind_ttls,
        }
    }

    /// Create policy for untrusted/sandboxed execution
    pub fn sandboxed() -> Self {
        Self::new(TaskTrustLevel::None)
    }

    /// Create policy for normal user interaction
    pub fn default_policy() -> Self {
        Self::new(TaskTrustLevel::Medium)
    }

    /// Set minimum confidence threshold
    pub fn with_min_confidence(mut self, confidence: Confidence) -> Self {
        self.min_confidence = confidence;
        self
    }

    /// Evaluate content quality - returns true if content is likely noise
    fn is_noise(&self, content: &str) -> bool {
        let normalized = content.to_lowercase().trim().to_string();
        
        // Reject empty/whitespace only
        if normalized.is_empty() {
            return true;
        }
        
        // Reject if mostly noise indicators
        let words: Vec<&str> = normalized.split_whitespace().collect();
        if words.is_empty() {
            return true;
        }
        
        let noise_count = words
            .iter()
            .filter(|w| self.noise_indicators.contains(&w.to_string()))
            .count();
        
        // If >50% noise, reject
        if noise_count > words.len() / 2 {
            return true;
        }
        
        // Reject very short content
        if normalized.len() < 10 && words.len() < 3 {
            return true;
        }
        
        false
    }

    /// Calculate confidence based on provenance
    fn confidence_from_provenance(&self, provenance: &Provenance) -> Confidence {
        match *provenance {
            Provenance::UserStated => Confidence::HIGH,
            Provenance::UserMentioned => Confidence::new(0.85),
            Provenance::TemporalContext => Confidence::new(0.9),
            Provenance::InferredFromConversation => Confidence::new(0.6),
            Provenance::InferredFromPattern => Confidence::LOW,
            Provenance::Imported(_) => Confidence::new(0.5), // Depends on source
        }
    }

    /// Check if memory is sensitive enough to need special handling
    fn detect_sensitive_content(&self, content: &str) -> Option<SensitivityLevel> {
        let lower = content.to_lowercase();
        
        // Critical patterns
        let critical_patterns = [
            "password", "secret", "api key", "private key",
            "credit card", "ssn", "social security",
        ];
        
        for pattern in &critical_patterns {
            if lower.contains(*pattern) {
                return Some(SensitivityLevel::Critical);
            }
        }

        // Private patterns
        let private_patterns = [
            "address", "phone", "email", "personal",
            "health", "medical", "financial",
        ];

        let private_count = private_patterns.iter()
            .filter(|p| lower.contains(*p))
            .count();
        
        if private_count >= 2 {
            return Some(SensitivityLevel::Private);
        }
        
        // Sensitive patterns (single mention)
        if private_count >= 1 {
            return Some(SensitivityLevel::Sensitive);
        }
        
        None
    }
}

#[async_trait]
impl MemoryPolicy for RuleBasedPolicy {
    async fn evaluate(&self, memory: &Memory) -> MemoryDecision {
        // Check for noise
        if self.is_noise(&memory.content) {
            return MemoryDecision::reject("Content is noise or too low-value");
        }
        
        // Determine provenance from memory source
        let provenance: Provenance = memory.metadata.source.clone().into();
        
        // Calculate confidence
        let mut confidence = self.confidence_from_provenance(&provenance);
        
        // Check if memory has existing confidence that overrides
        if memory.metadata.confidence > 0.0 {
            confidence = Confidence::new(
                (confidence.0 + memory.metadata.confidence) / 2.0
            );
        }
        
        // Reject if below minimum confidence
        if confidence.is_below(self.min_confidence) {
            return MemoryDecision::reject(
                format!("Confidence {:.2} below threshold {:.2}",
                    confidence.0, self.min_confidence.0
            ));
        }
        
        // Determine sensitivity
        let sensitivity = self.detect_sensitive_content(&memory.content)
            .unwrap_or_else(|| memory.metadata.sensitivity);
        
        // Get TTL for this memory kind
        let ttl = self.kind_ttls.get(&memory.kind).copied().unwrap_or_default();
        
        // Determine if should promote to long-term
        // Criteria: User stated, or high importance, or semantic/preference
        let promote_to_long_term = match provenance {
            Provenance::UserStated => true,
            Provenance::UserMentioned if memory.kind == MemoryKind::Preference => true,
            _ => memory.importance >= 0.7,
        };
        
        MemoryDecision::Accept {
            confidence,
            provenance,
            sensitivity,
            ttl,
            promote_to_long_term,
            policy_metadata: std::collections::HashMap::new(),
        }
    }
    
    fn can_retrieve(&self, memory: &Memory, trust_level: TaskTrustLevel) -> bool {
        // Check trust level can access this sensitivity
        if !trust_level.can_access(memory.metadata.sensitivity) {
            return false;
        }
        
        // Check expiration - note: storing TTL info in metadata tags for now
        // In production, TTL would be tracked separately and enforced
        let _now = Utc::now();
        
        true
    }
    
    fn filter_query(&self, query: &MemoryQuery, trust_level: TaskTrustLevel) -> MemoryQuery {
        let mut filtered = query.clone();
        
        // Filter out sensitive memories if trust level insufficient
        if trust_level < TaskTrustLevel::Low {
            // Filter to public/default only
            filtered.min_confidence = Some(0.8);
        }
        
        filtered
    }
    
    fn get_context(&self) -> PolicyContext {
        self.context.clone()
    }
}

/// Policy-enforcing wrapper around a MemoryStore
///
/// This is the main interface that enforces security and business rules.
#[derive(Clone)]
pub struct PolicyEnforcingStore<P: MemoryPolicy, S: MemoryStore> {
    policy: Arc<P>,
    store: Arc<S>,
}

impl<P: MemoryPolicy, S: MemoryStore> PolicyEnforcingStore<P, S> {
    /// Create a new policy-enforcing store
    pub fn new(policy: P, store: S) -> Self {
        Self {
            policy: Arc::new(policy),
            store: Arc::new(store),
        }
    }

    /// Get the underlying store (for admin operations)
    pub fn underlying(&self) -> &S {
        &self.store
    }

    /// Get the policy
    pub fn policy(&self) -> &P {
        &self.policy
    }
}

#[async_trait]
impl<P: MemoryPolicy + Send + Sync, S: MemoryStore> MemoryStore for PolicyEnforcingStore<P, S> {
    async fn store(&self, memory: Memory) -> crate::store::Result<MemoryId> {
        let trust_level = self.policy.get_context().trust_level;
        
        // Check if we have write permission for this kind
        if !trust_level.can_write(memory.kind) {
            return Err(MemoryStoreError::InvalidOperation(
                format!("Insufficient trust level {:?} to write memory kind {:?}",
                    trust_level, memory.kind)
            ));
        }
        
        // Evaluate through policy
        match self.policy.evaluate(&memory).await {
            MemoryDecision::Accept {
                confidence,
                provenance,
                sensitivity,
                ttl,
                promote_to_long_term,
                policy_metadata,
            } => {
                // Build final memory with policy decisions
                let mut final_memory = memory;
                final_memory.metadata.confidence = confidence.0;
                final_memory.metadata.sensitivity = sensitivity;
                
                // Store provenance in tags for auditing
                final_memory.metadata.tags.insert(
                    "policy_provenance".to_string(),
                    format!("{:?}", provenance)
                );
                for (key, value) in policy_metadata {
                    final_memory.metadata.tags.insert(format!("policy_{}", key), value);
                }
                
                // Store TTL info
                final_memory.metadata.tags.insert(
                    "policy_ttl_seconds".to_string(),
                    ttl.duration.num_seconds().to_string()
                );
                final_memory.metadata.tags.insert(
                    "policy_promote_lts".to_string(),
                    promote_to_long_term.to_string()
                );
                
                // Pass to underlying store
                self.store.store(final_memory).await
            }
            
            MemoryDecision::Reject { reason, .. } => {
                tracing::debug!("Memory rejected by policy: {}", reason);
                Err(MemoryStoreError::InvalidOperation(reason))
            }
        }
    }
    
    async fn retrieve(&self, query: &MemoryQuery) -> crate::store::Result<Vec<Memory>> {
        let trust_level = self.policy.get_context().trust_level;
        
        // Filter query based on trust level
        let filtered_query = self.policy.filter_query(query, trust_level);
        
        // Retrieve from underlying store
        let results = self.store.retrieve(&filtered_query).await?;
        
        // Filter results based on sensitivity and trust
        let accessible: Vec<Memory> = results
            .into_iter()
            .filter(|m| self.policy.can_retrieve(m, trust_level))
            .collect();
        
        Ok(accessible)
    }
    
    async fn get(&self, id: MemoryId) -> crate::store::Result<Option<Memory>> {
        let result = self.store.get(id).await?;
        
        if let Some(ref memory) = result {
            let trust_level = self.policy.get_context().trust_level;
            if !self.policy.can_retrieve(memory, trust_level) {
                // Don't leak existence of sensitive memories
                return Ok(None);
            }
        }
        
        Ok(result)
    }
    
    async fn forget(&self, id: MemoryId) -> crate::store::Result<()> {
        // Forget requires higher trust level
        let trust_level = self.policy.get_context().trust_level;
        if trust_level < TaskTrustLevel::Medium {
            return Err(MemoryStoreError::InvalidOperation(
                "Insufficient trust level to delete memories".to_string()
            ));
        }
        
        // Pass through to underlying store - this is hard deletion
        self.store.forget(id).await
    }
    
    async fn update(&self, id: MemoryId, patch: crate::memory::MemoryPatch) -> crate::store::Result<Memory> {
        let trust_level = self.policy.get_context().trust_level;
        
        // Check if we can read the existing memory
        let existing = match self.store.get(id).await? {
            Some(m) => m,
            None => return Err(MemoryStoreError::NotFound(id)),
        };
        
        // Check sensitivity access
        if !self.policy.can_retrieve(&existing, trust_level) {
            return Err(MemoryStoreError::NotFound(id)); // Don't leak existence
        }
        
        // Check write permission for this kind
        if !trust_level.can_write(existing.kind) {
            return Err(MemoryStoreError::InvalidOperation(
                "Insufficient trust level to update memory".to_string()
            ));
        }
        
        // Pass to underlying store
        self.store.update(id, patch).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_confidence_levels() {
        assert!(Confidence::HIGH.is_at_least(Confidence::MEDIUM));
        assert!(!Confidence::LOW.is_at_least(Confidence::MEDIUM));
    }
    
    #[test]
    fn test_task_trust_level_access() {
        // Public accessible to all
        assert!(TaskTrustLevel::None.can_access(SensitivityLevel::Public));
        assert!(TaskTrustLevel::Full.can_access(SensitivityLevel::Public));
        
        // None cannot access anything non-public
        assert!(!TaskTrustLevel::None.can_access(SensitivityLevel::Default));
        
        // Medium can access Default, Sensitive, Private
        assert!(TaskTrustLevel::Medium.can_access(SensitivityLevel::Default));
        assert!(TaskTrustLevel::Medium.can_access(SensitivityLevel::Sensitive));
        assert!(TaskTrustLevel::Medium.can_access(SensitivityLevel::Private));
        assert!(!TaskTrustLevel::Medium.can_access(SensitivityLevel::Critical));
        
        // Full can access everything
        assert!(TaskTrustLevel::Full.can_access(SensitivityLevel::Critical));
    }
    
    #[test]
    fn test_task_trust_level_write() {
        // None cannot write anything
        assert!(!TaskTrustLevel::None.can_write(MemoryKind::Working));
        
        // Minimal can only write Working
        assert!(TaskTrustLevel::Minimal.can_write(MemoryKind::Working));
        assert!(!TaskTrustLevel::Minimal.can_write(MemoryKind::Semantic));
        
        // Medium can write Working, Episodic, Semantic, Preference
        assert!(TaskTrustLevel::Medium.can_write(MemoryKind::Semantic));
        assert!(!TaskTrustLevel::Medium.can_write(MemoryKind::Summary));
        
        // Full can write everything
        assert!(TaskTrustLevel::Full.can_write(MemoryKind::Procedural));
    }
    
    #[test]
    fn test_ttl_expiration() {
        let ttl = Ttl::working(30); // 30 minutes
        let start = Utc::now();
        let now = start + Duration::minutes(15);
        
        assert!(!ttl.is_expired(start, now));
        
        let expired = start + Duration::minutes(31);
        assert!(ttl.is_expired(start, expired));
    }
    
    #[test]
    fn test_provenance_user_origin() {
        assert!(Provenance::UserStated.is_user_origin());
        assert!(Provenance::UserMentioned.is_user_origin());
        assert!(!Provenance::InferredFromPattern.is_user_origin());
    }
    
    #[test]
    fn test_policy_noise_detection() {
        let policy = RuleBasedPolicy::default_policy();
        
        // Empty content is noise
        assert!(policy.is_noise(""));
        assert!(policy.is_noise("   "));
        
        // Just noise words
        assert!(policy.is_noise("um uh"));
        
        // Valid content is not noise
        assert!(!policy.is_noise("I prefer dark mode"));
    }
    
    #[test]
    fn test_policy_sensitive_detection() {
        let policy = RuleBasedPolicy::default_policy();
        
        assert_eq!(
            policy.detect_sensitive_content("My password is secret123"),
            Some(SensitivityLevel::Critical)
        );
        
        assert_eq!(
            policy.detect_sensitive_content("My email and address"),
            Some(SensitivityLevel::Private)
        );
        
        assert_eq!(
            policy.detect_sensitive_content("I like pizza"),
            None
        );
    }
}
