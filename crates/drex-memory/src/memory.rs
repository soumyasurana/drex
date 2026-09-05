use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A unique identifier for a memory entry.
///
/// Uses UUID v7 for time-sortable, lexicographically ordered IDs
/// that embed creation timestamp without extra index overhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryId(pub Uuid);

impl MemoryId {
    /// Generate a new memory ID with the current timestamp.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Uuid> for MemoryId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<MemoryId> for Uuid {
    fn from(id: MemoryId) -> Self {
        id.0
    }
}

impl std::fmt::Display for MemoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Classification of memory by cognitive type.
///
/// This taxonomy supports the full spectrum of human-like memory systems.
/// Not all kinds are natively supported by all backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// Working memory: transient, attention-focused state.
    /// Lifetime: seconds to minutes. Context-window local.
    ///
    /// **Contextra mapping**: Session cache (Redis)
    Working,

    /// Episodic memory: specific events with temporal/spatial context.
    /// Lifetime: hours to days. Personal experiences.
    ///
    /// **Contextra mapping**: Conversation message history (PostgreSQL)
    Episodic,

    /// Semantic memory: facts and concepts without personal context.
    /// Lifetime: essentially permanent. General knowledge.
    ///
    /// **Contextra mapping**: `LongTermMemoryKind::Fact` with vector embedding
    Semantic,

    /// Preference memory: user likes, dislikes, and choices.
    /// Lifetime: long-term. Updated as preferences evolve.
    ///
    /// **Contextra mapping**: `LongTermMemoryKind::Preference`
    Preference,

    /// Procedural memory: skills, habits, "how-to" knowledge.
    /// Lifetime: long-term. (Implicit, hard to verbalize).
    ///
    /// **Contextra mapping**: N/A - embed as semantic or store externally
    Procedural,

    /// Relationship memory: connections between entities (people, concepts).
    /// Lifetime: long-term. Graph-like associations.
    ///
    /// **Contextra mapping**: N/A - embed as semantic facts
    Relationship,

    /// Summary memory: condensed representations of conversations/events.
    /// Lifetime: long-term. Compression of episodic data.
    ///
    /// **Contextra mapping**: `LongTermMemoryKind::Summary`
    Summary,
}

impl MemoryKind {
    /// Returns true if this kind maps directly to a Contextra `LongTermMemoryKind`.
    pub fn is_contextra_native(&self) -> bool {
        matches!(self, MemoryKind::Semantic | MemoryKind::Preference | MemoryKind::Summary)
    }

    /// Returns the Contextra name for this kind, if natively supported.
    pub fn contextra_name(&self) -> Option<&'static str> {
        match self {
            MemoryKind::Semantic => Some("fact"),
            MemoryKind::Preference => Some("preference"),
            MemoryKind::Summary => Some("summary"),
            _ => None,
        }
    }
}

impl Default for MemoryKind {
    fn default() -> Self {
        Self::Semantic
    }
}

/// Metadata attached to a memory entry.
///
/// Includes provenance tracking, confidence scoring, and sensitivity labels
/// for privacy and data governance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryMetadata {
    /// When this memory was originally created.
    pub created_at: DateTime<Utc>,

    /// When this memory was last modified (if ever).
    pub updated_at: Option<DateTime<Utc>>,

    /// Source of this memory: conversation, manual, import, etc.
    pub source: MemorySource,

    /// Confidence score [0.0, 1.0]: how certain we are about this memory.
    /// 1.0 = certain, 0.0 = uncertain/speculative.
    pub confidence: f32,

    /// Sensitivity level for access control and PII handling.
    pub sensitivity: SensitivityLevel,

    /// User ID this memory belongs to (for multi-tenant isolation).
    pub user_id: Option<String>,

    /// Conversation/session ID if derived from a specific interaction.
    pub session_id: Option<String>,

    /// Additional key-value metadata for extensibility.
    #[serde(default)]
    pub tags: HashMap<String, String>,
}

impl MemoryMetadata {
    /// Create metadata for an automatically captured memory.
    pub fn automatic(user_id: impl Into<String>) -> Self {
        Self {
            created_at: Utc::now(),
            updated_at: None,
            source: MemorySource::Automatic,
            confidence: 0.75,
            sensitivity: SensitivityLevel::Default,
            user_id: Some(user_id.into()),
            session_id: None,
            tags: HashMap::new(),
        }
    }

    /// Create metadata for a user-explicit memory.
    pub fn explicit(user_id: impl Into<String>) -> Self {
        let mut meta = Self::automatic(user_id);
        meta.source = MemorySource::Explicit;
        meta.confidence = 0.95;
        meta
    }

    /// Create metadata for an imported memory.
    pub fn imported(user_id: impl Into<String>, source: impl Into<String>) -> Self {
        let mut meta = Self::automatic(user_id);
        meta.source = MemorySource::Imported(source.into());
        meta.confidence = 0.8;
        meta
    }

    /// Set the confidence score.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the sensitivity level.
    pub fn with_sensitivity(mut self, sensitivity: SensitivityLevel) -> Self {
        self.sensitivity = sensitivity;
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
}

impl Default for MemoryMetadata {
    fn default() -> Self {
        Self {
            created_at: Utc::now(),
            updated_at: None,
            source: MemorySource::Unknown,
            confidence: 0.5,
            sensitivity: SensitivityLevel::Default,
            user_id: None,
            session_id: None,
            tags: HashMap::new(),
        }
    }
}

/// Source of a memory entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemorySource {
    /// Captured automatically by the system (e.g., from conversation).
    Automatic,

    /// Explicitly provided by the user (e.g., "remember that I...").
    Explicit,

    /// Imported from an external source.
    Imported(String),

    /// Derived from a summary or compression process.
    Summary,

    /// Inferred from patterns or analysis.
    Inferred,

    /// Source unknown or not tracked.
    Unknown,
}

/// Sensitivity level for access control and PII handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityLevel {
    /// Public, non-sensitive information.
    Public,

    /// Default sensitivity - standard handling.
    Default,

    /// Sensitive - requires user confirmation before sharing.
    Sensitive,

    /// Private - encrypted at rest, strict access control.
    Private,

    /// Critical - highest protection, audit all access.
    Critical,
}

impl Default for SensitivityLevel {
    fn default() -> Self {
        Self::Default
    }
}

/// A complete memory entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    /// Unique identifier for this memory.
    pub id: MemoryId,

    /// Classification kind.
    pub kind: MemoryKind,

    /// Memory content (textual representation).
    pub content: String,

    /// Importance score [0.0, 1.0] for eviction/retention decisions.
    /// 1.0 = critical, never forget. 0.0 = trivial, low priority.
    pub importance: f32,

    /// Metadata and provenance.
    pub metadata: MemoryMetadata,
}

impl Memory {
    /// Create a new memory with automatic ID generation.
    pub fn new(kind: MemoryKind, content: impl Into<String>) -> Self {
        Self {
            id: MemoryId::new(),
            kind,
            content: content.into(),
            importance: 0.5,
            metadata: MemoryMetadata::default(),
        }
    }

    /// Set the importance score.
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    /// Set the metadata.
    pub fn with_metadata(mut self, metadata: MemoryMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

/// A patch for updating an existing memory.
///
/// Fields are optional - only specified fields will be updated.
/// Setting a field to `Some(None)` clears it (where applicable).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryPatch {
    /// New content (if updating).
    pub content: Option<String>,

    /// New importance score.
    pub importance: Option<f32>,

    /// Merge or replace tags.
    pub tags: Option<HashMap<String, String>>,

    /// Update confidence.
    pub confidence: Option<f32>,

    /// Update sensitivity.
    pub sensitivity: Option<SensitivityLevel>,
}

impl MemoryPatch {
    /// Create an empty patch.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Update the content.
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Update the importance.
    pub fn importance(mut self, importance: f32) -> Self {
        self.importance = Some(importance);
        self
    }

    /// Update tags.
    pub fn tags(mut self, tags: HashMap<String, String>) -> Self {
        self.tags = Some(tags);
        self
    }

    /// Update confidence.
    pub fn confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence);
        self
    }

    /// Update sensitivity.
    pub fn sensitivity(mut self, sensitivity: SensitivityLevel) -> Self {
        self.sensitivity = Some(sensitivity);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_id_generation() {
        let id1 = MemoryId::new();
        let id2 = MemoryId::new();
        assert_ne!(id1, id2);

        // UUID v7 starts with timestamp prefix
        let uuid: Uuid = id1.into();
        assert_eq!(uuid.get_version_num(), 7);
    }

    #[test]
    fn memory_id_roundtrip() {
        let id = MemoryId::new();
        let uuid: Uuid = id.into();
        let back: MemoryId = uuid.into();
        assert_eq!(id, back);
    }

    #[test]
    fn memory_basic_construction() {
        let memory = Memory::new(MemoryKind::Semantic, "Rust is a systems language");
        assert_eq!(memory.kind, MemoryKind::Semantic);
        assert_eq!(memory.content, "Rust is a systems language");
        assert!((0.49..=0.51).contains(&memory.importance)); // default ~0.5
    }

    #[test]
    fn memory_with_importance() {
        let memory = Memory::new(MemoryKind::Preference, "I like dark mode")
            .with_importance(0.95);
        assert!((0.94..=0.96).contains(&memory.importance));
    }

    #[test]
    fn importance_clamped() {
        let memory = Memory::new(MemoryKind::Semantic, "Test")
            .with_importance(1.5)
            .with_importance(-0.5);
        assert_eq!(memory.importance, 0.0);
    }

    #[test]
    fn metadata_automatic() {
        let meta = MemoryMetadata::automatic("user-123");
        assert_eq!(meta.source, MemorySource::Automatic);
        assert_eq!(meta.user_id, Some("user-123".to_string()));
        assert!((0.74..=0.76).contains(&meta.confidence));
    }

    #[test]
    fn metadata_explicit() {
        let meta = MemoryMetadata::explicit("user-456");
        assert_eq!(meta.source, MemorySource::Explicit);
        assert!((0.94..=0.96).contains(&meta.confidence));
    }

    #[test]
    fn metadata_with_tags() {
        let meta = MemoryMetadata::automatic("u1")
            .with_tag("source", "slack")
            .with_tag("channel", "general");
        assert_eq!(meta.tags.get("source"), Some(&"slack".to_string()));
        assert_eq!(meta.tags.get("channel"), Some(&"general".to_string()));
    }

    #[test]
    fn memory_kind_contextra_mapping() {
        assert!(MemoryKind::Semantic.is_contextra_native());
        assert!(MemoryKind::Preference.is_contextra_native());
        assert!(MemoryKind::Summary.is_contextra_native());
        assert!(!MemoryKind::Episodic.is_contextra_native());
        assert!(!MemoryKind::Working.is_contextra_native());

        assert_eq!(MemoryKind::Semantic.contextra_name(), Some("fact"));
        assert_eq!(MemoryKind::Preference.contextra_name(), Some("preference"));
        assert_eq!(MemoryKind::Summary.contextra_name(), Some("summary"));
        assert_eq!(MemoryKind::Episodic.contextra_name(), None);
    }

    #[test]
    fn patch_builder() {
        let patch = MemoryPatch::empty()
            .content("Updated content")
            .importance(0.9)
            .confidence(0.95);

        assert_eq!(patch.content, Some("Updated content".to_string()));
        assert_eq!(patch.importance, Some(0.9));
        assert_eq!(patch.confidence, Some(0.95));
    }
}
