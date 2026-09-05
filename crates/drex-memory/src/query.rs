use serde::{Deserialize, Serialize};

use crate::memory::{MemoryId, MemoryKind, MemorySource};

/// Query parameters for retrieving memories.
///
/// Supports filtering by kind, user, time range, and text search.
/// All fields are optional - omitting means "match all".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// Filter by specific memory IDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<MemoryId>,

    /// Filter by memory kinds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<MemoryKind>,

    /// Search query text (semantic search if supported).
    pub query_text: Option<String>,

    /// Filter by source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<MemorySource>,

    /// Filter by user ID.
    pub user_id: Option<String>,

    /// Filter by session ID.
    pub session_id: Option<String>,

    /// Minimum importance threshold [0.0, 1.0].
    pub min_importance: Option<f32>,

    /// Created after this timestamp.
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,

    /// Created before this timestamp.
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,

    /// Maximum number of results.
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Offset for pagination.
    #[serde(default)]
    pub offset: usize,

    /// Minimum confidence threshold [0.0, 1.0].
    pub min_confidence: Option<f32>,

    /// Include deleted memories (if soft-deletion is supported).
    #[serde(default)]
    pub include_deleted: bool,
}

fn default_limit() -> usize {
    20
}

impl MemoryQuery {
    /// Create an empty query matching all memories.
    pub fn all() -> Self {
        Self::default()
    }

    /// Query for a specific memory ID.
    pub fn by_id(id: MemoryId) -> Self {
        Self {
            ids: vec![id],
            ..Default::default()
        }
    }

    /// Query for a specific kind.
    pub fn by_kind(kind: MemoryKind) -> Self {
        Self {
            kinds: vec![kind],
            ..Default::default()
        }
    }

    /// Query for kinds matching any of the provided values.
    pub fn by_kinds(kinds: Vec<MemoryKind>) -> Self {
        Self {
            kinds,
            ..Default::default()
        }
    }

    /// Semantic text search query.
    pub fn search(text: impl Into<String>) -> Self {
        Self {
            query_text: Some(text.into()),
            ..Default::default()
        }
    }

    /// Filter by user ID.
    pub fn for_user(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Filter by session ID.
    pub fn for_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set minimum importance threshold.
    pub fn min_importance(mut self, importance: f32) -> Self {
        self.min_importance = Some(importance.clamp(0.0, 1.0));
        self
    }

    /// Set minimum confidence threshold.
    pub fn min_confidence(mut self, confidence: f32) -> Self {
        self.min_confidence = Some(confidence.clamp(0.0, 1.0));
        self
    }

    /// Set created after filter.
    pub fn created_after(mut self, when: chrono::DateTime<chrono::Utc>) -> Self {
        self.created_after = Some(when);
        self
    }

    /// Set created before filter.
    pub fn created_before(mut self, when: chrono::DateTime<chrono::Utc>) -> Self {
        self.created_before = Some(when);
        self
    }

    /// Set result limit (max).
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    /// Set result offset (for pagination).
    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// Include deleted memories in results.
    pub fn include_deleted(mut self, include: bool) -> Self {
        self.include_deleted = include;
        self
    }

    /// Returns true if this query filters by a specific ID (single or list).
    pub fn is_id_lookup(&self) -> bool {
        !self.ids.is_empty()
    }

    /// Returns true if this query has no text-based search.
    pub fn is_exact_match(&self) -> bool {
        self.query_text.is_none()
    }
}

impl Default for MemoryQuery {
    fn default() -> Self {
        Self {
            ids: Vec::new(),
            kinds: Vec::new(),
            query_text: None,
            sources: Vec::new(),
            user_id: None,
            session_id: None,
            min_importance: None,
            created_after: None,
            created_before: None,
            limit: default_limit(),
            offset: 0,
            min_confidence: None,
            include_deleted: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_default() {
        let q = MemoryQuery::all();
        assert!(q.ids.is_empty());
        assert!(q.kinds.is_empty());
        assert!(q.query_text.is_none());
        assert_eq!(q.limit, 20);
        assert_eq!(q.offset, 0);
    }

    #[test]
    fn query_by_id() {
        let id = MemoryId::new();
        let q = MemoryQuery::by_id(id);
        assert_eq!(q.ids, vec![id]);
        assert!(q.is_id_lookup());
    }

    #[test]
    fn query_by_kind() {
        let q = MemoryQuery::by_kind(MemoryKind::Semantic);
        assert_eq!(q.kinds, vec![MemoryKind::Semantic]);
    }

    #[test]
    fn query_search() {
        let q = MemoryQuery::search("rust programming");
        assert_eq!(q.query_text, Some("rust programming".to_string()));
        assert!(!q.is_exact_match());
    }

    #[test]
    fn query_builder_chaining() {
        let q = MemoryQuery::all()
            .for_user("user-123")
            .for_session("session-456")
            .min_importance(0.8)
            .min_confidence(0.9)
            .limit(10)
            .offset(20);

        assert_eq!(q.user_id, Some("user-123".to_string()));
        assert_eq!(q.session_id, Some("session-456".to_string()));
        assert_eq!(q.min_importance, Some(0.8));
        assert_eq!(q.min_confidence, Some(0.9));
        assert_eq!(q.limit, 10);
        assert_eq!(q.offset, 20);
    }

    #[test]
    fn query_limit_bounds() {
        let q = MemoryQuery::all().limit(0);
        assert_eq!(q.limit, 1); // clamped to at least 1
    }

    use crate::memory::MemoryId;
}
