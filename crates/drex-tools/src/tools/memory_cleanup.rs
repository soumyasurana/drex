//! Memory cleanup tool - Identifies and removes historical test/debug pollution.
//!
//! This tool is designed to safely clean up memories created before the writeback
//! policy fix that stored automatic plan summaries as "user memories".
//!
//! ## Identification Criteria
//!
//! Test/debug records are identified by:
//! - `drex_source: "Automatic"` (old write_memories used automatic source)
//! - Content patterns indicating test data or serialized plan summaries:
//!   - Starts with "Request:" (plan summaries)
//!   - Contains "memory: Object {" (debug dumps)
//!   - Contains "DREX_MEMORY_TEST_" (test markers)
//!   - But NOT "DREX_MEMORY_FINAL_TEST_001" (preserved explicitly)
//!
//! ## Safety Guarantees
//!
//! - Only deletes records matching ALL criteria
//! - Dry-run mode shows what would be deleted without making changes
//! - Never automatic; must be explicitly invoked
//! - Never touches records with `drex_source: "Explicit"` (legitimate user memories)
//! - Shows exact IDs and content previews before deletion

use crate::capability::Capability;
use crate::error::{ToolError, ToolResult};
use crate::result::ExecutionResult;
use crate::schema::{JsonSchema, ToolSchema};
use crate::tool::{Tool, ToolContext, ToolInput, ToolMetadata};
use async_trait::async_trait;
use drex_memory::{Memory, MemoryQuery, MemorySource};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, info, warn};

/// A tool for cleaning up historical test/debug memory pollution.
///
/// This tool provides explicit cleanup of memories that were incorrectly stored
/// before the writeback policy fix. It should NEVER be invoked automatically
/// and requires explicit user action.
#[derive(Debug, Clone)]
pub struct MemoryCleanupTool {
    metadata: ToolMetadata,
}

/// The action to perform
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupAction {
    /// Preview what would be cleaned (dry-run mode)
    Preview,
    /// Actually delete the identified records
    Execute,
}

/// Input structure for the MemoryCleanup tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCleanupInput {
    /// The action to perform: "preview" or "execute"
    #[serde(default = "default_cleanup_action")]
    pub action: CleanupAction,
    /// Optional: specific memory IDs to check (if empty, searches all)
    #[serde(default)]
    pub ids: Vec<String>,
    /// Optional: include additional test patterns beyond the default set
    #[serde(default)]
    pub additional_patterns: Vec<String>,
    /// Optional: force deletion even if content doesn't match patterns (use with caution)
    #[serde(default = "default_force")]
    pub force: bool,
}

fn default_cleanup_action() -> CleanupAction {
    CleanupAction::Preview
}

fn default_force() -> bool {
    false
}

/// Output structure for the cleanup operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCleanupOutput {
    /// The action that was performed
    pub action: String,
    /// Number of records identified as cleanup candidates
    pub candidates_found: usize,
    /// Number of records actually deleted (0 in preview mode)
    pub records_deleted: usize,
    /// Records that matched the criteria
    pub candidates: Vec<CleanupCandidate>,
    /// Records that were preserved (did not match criteria)
    pub preserved: Vec<PreservedRecord>,
}

/// A candidate for cleanup (would be deleted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupCandidate {
    /// Memory ID
    pub id: String,
    /// Content preview (first 100 chars)
    pub content_preview: String,
    /// Reason why this was identified
    pub reason: String,
    /// Whether this record was actually deleted
    pub deleted: bool,
}

/// A record that was preserved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreservedRecord {
    /// Memory ID
    pub id: String,
    /// Why it was preserved
    pub reason: String,
}

impl MemoryCleanupTool {
    /// Create a new MemoryCleanupTool
    pub fn new() -> Self {
        let schema = ToolSchema::builder("MemoryCleanupInput", "Input for the memory cleanup tool")
            .optional_property("action", JsonSchema::string("Action: 'preview' (default) or 'execute'"))
            .optional_property("ids", JsonSchema::array("Specific memory IDs to check (empty = search all)", JsonSchema::string("Memory ID as UUID string")))
            .optional_property("additional_patterns", JsonSchema::array("Extra test patterns to match", JsonSchema::string("Pattern string to match")))
            .optional_property("force", JsonSchema::boolean("Skip content pattern matching (dangerous)"))
            .build();

        Self {
            metadata: ToolMetadata::new(
                "memory_cleanup",
                "Clean up historical test/debug memory pollution.
\
                \n\
                This tool identifies and optionally deletes memories that were \
                incorrectly stored before the writeback policy fix. These include \
                plan summaries, debug dumps, and test markers that look like user \
                memories but are not.\
                \n\
                SAFETY: Only deletes records with ALL of these criteria:\
                - drex_source: \"Automatic\" (not Explicit user memories)\
                - Content matches test/debug patterns\
                - Excluded patterns (like FINAL_TEST) are preserved\
                \n\
                Actions:\
                - preview (default): Show what would be deleted without making changes\
                - execute: Actually delete the identified records\
                \n\
                IMPORTANT: Always run preview first to verify the selection!",
                schema,
            ),
        }
    }

    /// Default test/debug content patterns to match
    fn default_test_patterns() -> Vec<String> {
        vec![
            "Request:".to_string(),
            "memory: Object {".to_string(),
            "memoryObject {".to_string(),
            r#""memory":"#.to_string(),
        ]
    }

    /// Patterns that should ALWAYS be preserved (even if they match test patterns)
    fn preservation_patterns() -> Vec<String> {
        vec![
            "DREX_MEMORY_FINAL_TEST_001".to_string(),
        ]
    }

    /// Check if content matches any test pattern
    fn matches_test_pattern(content: &str, additional_patterns: &[String]) -> Option<String> {
        // Check default patterns
        if content.contains("Request:") {
            return Some("Contains pattern: 'Request:'".to_string());
        }
        if content.contains("memory: Object {") {
            return Some("Contains pattern: 'memory: Object {'".to_string());
        }
        if content.contains("memoryObject {") {
            return Some("Contains pattern: 'memoryObject {'".to_string());
        }
        if content.contains(r#""memory":"#) {
            return Some(r#"Contains JSON pattern: "memory":"#.to_string());
        }

        // Check additional patterns if provided
        for pattern in additional_patterns {
            if content.contains(pattern) {
                return Some(format!("Contains additional pattern: '{}'", pattern));
            }
        }

        // Check for DREX_MEMORY_TEST_ patterns (but not FINAL_TEST)
        if content.contains("DREX_MEMORY_TEST_")
            && !content.contains("DREX_MEMORY_FINAL_TEST_001")
        {
            return Some("Contains DREX_MEMORY_TEST_ marker".to_string());
        }

        None
    }

    /// Check if content should be preserved
    fn should_preserve(&self, content: &str) -> Option<String> {
        if content.contains("DREX_MEMORY_FINAL_TEST_001") {
            return Some("Contains preservation pattern: 'DREX_MEMORY_FINAL_TEST_001'".to_string());
        }
        None
    }

    /// Evaluate a single memory for cleanup candidacy
    fn evaluate_memory(&self, memory: &Memory, additional_patterns: &[String]) -> EvaluationResult {
        // Check 1: Must be Automatic source (old records)
        if memory.metadata.source != MemorySource::Automatic {
            return EvaluationResult::Preserved {
                reason: format!("Source is {:?} (not Automatic)", memory.metadata.source),
            };
        }

        // Check 2: Preservation patterns (always keep these)
        if let Some(reason) = self.should_preserve(&memory.content) {
            return EvaluationResult::Preserved { reason };
        }

        // Check 3: Must match test patterns
        if let Some(reason) = Self::matches_test_pattern(&memory.content, additional_patterns) {
            return EvaluationResult::Candidate(CleanupCandidate {
                id: memory.id.to_string(),
                content_preview: if memory.content.len() > 100 {
                    format!("{}...", &memory.content[..100])
                } else {
                    memory.content.clone()
                },
                reason: format!("Automatic source + {}", reason),
                deleted: false,
            });
        }

        // No match - preserve
        EvaluationResult::Preserved {
            reason: "Does not match cleanup criteria".to_string(),
        }
    }

    /// Execute the cleanup operation
    async fn run_cleanup(
        &self,
        ctx: &ToolContext,
        input: &MemoryCleanupInput,
    ) -> ToolResult<MemoryCleanupOutput> {
        let memory_store = ctx.memory_store().ok_or_else(|| ToolError::ExecutionFailed {
            tool: self.name().to_string(),
            reason: "Memory store not available".to_string(),
        })?;

        // Retrieve memories to evaluate
        let memories = if input.ids.is_empty() {
            // Search all memories - use a broad query
            let query = MemoryQuery::search("").limit(10000);
            memory_store
                .retrieve(&query)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    tool: self.name().to_string(),
                    reason: format!("Failed to retrieve memories: {}", e),
                })?
        } else {
            // Check specific IDs
            let mut memories = Vec::new();
            for id_str in &input.ids {
                if let Ok(uuid) = uuid::Uuid::parse_str(id_str) {
                    let id = drex_memory::MemoryId::from(uuid);
                    if let Ok(Some(memory)) = memory_store.get(id).await {
                        memories.push(memory);
                    } else {
                        warn!("Memory ID not found: {}", id_str);
                    }
                } else {
                    return Err(ToolError::InvalidInput {
                        tool: self.name().to_string(),
                        reason: format!("Invalid UUID: {}", id_str),
                    });
                }
            }
            memories
        };

        info!(
            total_memories = memories.len(),
            "Evaluating memories for cleanup"
        );

        // Evaluate each memory
        let mut candidates = Vec::new();
        let mut preserved = Vec::new();

        for memory in &memories {
            match self.evaluate_memory(memory, &input.additional_patterns) {
                EvaluationResult::Candidate(mut candidate) => {
                    // Force mode bypasses content matching (but still requires Automatic source)
                    if input.force && memory.metadata.source == MemorySource::Automatic {
                        candidate.reason = format!("{} (forced)", candidate.reason);
                        candidates.push(candidate);
                    } else if !input.force {
                        candidates.push(candidate);
                    }
                }
                EvaluationResult::Preserved { reason } => {
                    preserved.push(PreservedRecord {
                        id: memory.id.to_string(),
                        reason,
                    });
                }
            }
        }

        let candidates_found = candidates.len();

        // Execute deletion if requested
        let records_deleted = if matches!(input.action, CleanupAction::Execute) {
            let mut deleted_count = 0;
            for candidate in &mut candidates {
                let id = match uuid::Uuid::parse_str(&candidate.id) {
                    Ok(uuid) => drex_memory::MemoryId::from(uuid),
                    Err(_) => {
                        warn!("Invalid UUID during deletion: {}", candidate.id);
                        continue;
                    }
                };

                match memory_store.forget(id).await {
                    Ok(()) => {
                        candidate.deleted = true;
                        deleted_count += 1;
                        debug!("Deleted memory: {}", candidate.id);
                    }
                    Err(e) => {
                        warn!("Failed to delete memory {}: {}", candidate.id, e);
                    }
                }
            }
            deleted_count
        } else {
            0
        };

        let action_str = match input.action {
            CleanupAction::Preview => "preview",
            CleanupAction::Execute => "execute",
        };

        Ok(MemoryCleanupOutput {
            action: action_str.to_string(),
            candidates_found,
            records_deleted,
            candidates,
            preserved,
        })
    }
}

/// Result of evaluating a single memory
enum EvaluationResult {
    Candidate(CleanupCandidate),
    Preserved { reason: String },
}

impl Default for MemoryCleanupTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MemoryCleanupTool {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn required_capabilities(&self) -> &crate::capability::CapabilitySet {
        static CAPS: std::sync::OnceLock<crate::capability::CapabilitySet> =
            std::sync::OnceLock::new();
        CAPS.get_or_init(|| {
            let mut caps = crate::capability::CapabilitySet::new();
            caps.add(Capability::MemoryRead);
            caps.add(Capability::MemoryWrite);
            caps
        })
    }

    async fn execute(&self, ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        let cleanup_input: MemoryCleanupInput = input.parse().map_err(|e| {
            ToolError::InvalidInput {
                tool: self.name().to_string(),
                reason: format!("failed to parse input: {}", e),
            }
        })?;

        let output = self.run_cleanup(ctx, &cleanup_input).await?;

        Ok(ExecutionResult::success(json!(output)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use drex_memory::{Memory, MemoryId, MemoryKind, MemoryMetadata, MemoryQuery, MemoryStore};
    use serde_json;

    struct MockMemoryStore {
        memories: std::sync::Mutex<Vec<Memory>>,
        deleted_ids: std::sync::Mutex<Vec<MemoryId>>,
    }

    impl MockMemoryStore {
        fn new(memories: Vec<Memory>) -> Self {
            Self {
                memories: std::sync::Mutex::new(memories),
                deleted_ids: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl MemoryStore for MockMemoryStore {
        async fn store(
            &self,
            _memory: Memory,
        ) -> Result<MemoryId, drex_memory::MemoryStoreError> {
            unimplemented!()
        }

        async fn retrieve(
            &self,
            _query: &MemoryQuery,
        ) -> Result<Vec<Memory>, drex_memory::MemoryStoreError> {
            Ok(self.memories.lock().unwrap().clone())
        }

        async fn forget(&self, id: MemoryId) -> Result<(), drex_memory::MemoryStoreError> {
            self.deleted_ids.lock().unwrap().push(id);
            Ok(())
        }

        async fn update(
            &self,
            _id: MemoryId,
            _patch: drex_memory::MemoryPatch,
        ) -> Result<Memory, drex_memory::MemoryStoreError> {
            unimplemented!()
        }
    }

    fn create_test_memory(content: &str, source: MemorySource) -> Memory {
        Memory::new(MemoryKind::Semantic, content)
            .with_metadata(MemoryMetadata {
                created_at: chrono::Utc::now(),
                updated_at: None,
                source,
                confidence: 0.5,
                sensitivity: drex_memory::SensitivityLevel::Default,
                user_id: Some("test-user".to_string()),
                session_id: None,
                tags: std::collections::HashMap::new(),
            })
    }

    #[test]
    fn test_default_test_patterns() {
        let patterns = MemoryCleanupTool::default_test_patterns();
        assert!(patterns.iter().any(|p| p.contains("Request:")));
        assert!(patterns.iter().any(|p| p.contains("memory: Object")));
    }

    #[test]
    fn test_matches_test_pattern_request() {
        let content = "Request: create a file\nResult: success";
        let patterns: Vec<String> = vec![];
        let result = MemoryCleanupTool::matches_test_pattern(content, &patterns);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Request:"));
    }

    #[test]
    fn test_matches_test_pattern_memory_object() {
        let content = r#"memory: Object { content: "test" }"#;
        let patterns: Vec<String> = vec![];
        let result = MemoryCleanupTool::matches_test_pattern(content, &patterns);
        assert!(result.is_some());
        assert!(result.unwrap().contains("memory: Object"));
    }

    #[test]
    fn test_matches_test_pattern_drex_test() {
        let content = "DREX_MEMORY_TEST_123 some test data";
        let patterns: Vec<String> = vec![];
        let result = MemoryCleanupTool::matches_test_pattern(content, &patterns);
        assert!(result.is_some());
        assert!(result.unwrap().contains("DREX_MEMORY_TEST_"));
    }

    #[test]
    fn test_preservation_pattern_final_test() {
        let tool = MemoryCleanupTool::new();
        let content = "DREX_MEMORY_FINAL_TEST_001 should be preserved";
        let result = tool.should_preserve(content);
        assert!(result.is_some());
        assert!(result.unwrap().contains("FINAL_TEST_001"));
    }

    #[test]
    fn test_no_match_for_legitimate_content() {
        let content = "The user likes to code in Rust";
        let patterns: Vec<String> = vec![];
        let result = MemoryCleanupTool::matches_test_pattern(content, &patterns);
        assert!(result.is_none());
    }

    #[test]
    fn test_evaluate_memory_automatic_source_with_request() {
        let tool = MemoryCleanupTool::new();
        let memory = create_test_memory(
            "Request: create a function\nResult: done",
            MemorySource::Automatic,
        );

        let result = tool.evaluate_memory(&memory, &[]);
        assert!(
            matches!(result, EvaluationResult::Candidate(_)),
            "Should be a candidate"
        );
    }

    #[test]
    fn test_evaluate_memory_explicit_source_preserved() {
        let tool = MemoryCleanupTool::new();
        let memory = create_test_memory(
            "Request: create a function", // Even with test pattern
            MemorySource::Explicit,       // But Explicit source
        );

        let result = tool.evaluate_memory(&memory, &[]);
        assert!(
            matches!(
                &result,
                EvaluationResult::Preserved { reason } if reason.contains("Explicit")
            ),
            "Should be preserved"
        );
    }

    #[test]
    fn test_evaluate_memory_final_test_preserved() {
        let tool = MemoryCleanupTool::new();
        let memory = create_test_memory(
            "DREX_MEMORY_FINAL_TEST_001 explicit user test",
            MemorySource::Automatic,
        );

        let result = tool.evaluate_memory(&memory, &[]);
        assert!(
            matches!(
                &result,
                EvaluationResult::Preserved { reason } if reason.contains("FINAL_TEST_001")
            ),
            "Should be preserved"
        );
    }

    #[test]
    fn test_evaluate_memory_drex_test_candidate() {
        let tool = MemoryCleanupTool::new();
        let memory = create_test_memory(
            "DREX_MEMORY_TEST_999 some test marker",
            MemorySource::Automatic,
        );

        let result = tool.evaluate_memory(&memory, &[]);
        assert!(
            matches!(result, EvaluationResult::Candidate(_)),
            "Should be a candidate"
        );
    }

    #[test]
    fn test_cleanup_output_serialization() {
        let output = MemoryCleanupOutput {
            action: "preview".to_string(),
            candidates_found: 2,
            records_deleted: 0,
            candidates: vec![CleanupCandidate {
                id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
                content_preview: "Request: test".to_string(),
                reason: "Test".to_string(),
                deleted: false,
            }],
            preserved: vec![PreservedRecord {
                id: "123e4567-e89b-12d3-a456-426614174001".to_string(),
                reason: "Explicit user memory".to_string(),
            }],
        };

        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(json["action"], "preview");
        assert_eq!(json["candidates_found"], 2);
        assert_eq!(json["records_deleted"], 0);
    }
}
