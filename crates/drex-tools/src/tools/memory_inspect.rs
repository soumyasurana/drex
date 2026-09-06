//! Memory inspection utility - Report on memory store contents for cleanup analysis
//!
//! This module provides utilities to inspect the memory store and identify
//! potential cleanup candidates without performing any deletions.

use crate::error::{ToolError, ToolResult};
use crate::result::ExecutionResult;
use crate::schema::{JsonSchema, ToolSchema};
use crate::tool::{Tool, ToolContext, ToolInput, ToolMetadata};
use async_trait::async_trait;
use drex_memory::{Memory, MemoryQuery, MemorySource};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

/// A tool for inspecting memory contents and reporting cleanup candidates.
#[derive(Debug, Clone)]
pub struct MemoryInspectTool {
    metadata: ToolMetadata,
}

/// Input for memory inspection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInspectInput {
    /// Maximum number of memories to inspect (default: 1000)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Filter by source (Automatic, Explicit, or empty for all)
    #[serde(default)]
    pub filter_source: Option<String>,
    /// Show full content instead of preview
    #[serde(default)]
    pub full_content: bool,
}

fn default_limit() -> usize {
    1000
}

/// Output showing memory inspection results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInspectOutput {
    /// Total memories inspected
    pub total_inspected: usize,
    /// Memories with Automatic source
    pub automatic_count: usize,
    /// Memories with Explicit source
    pub explicit_count: usize,
    /// Potential cleanup candidates
    pub cleanup_candidates: Vec<InspectCandidate>,
    /// Sample of preserved records
    pub preserved_sample: Vec<InspectPreserved>,
}

/// A candidate for cleanup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectCandidate {
    pub id: String,
    pub source: String,
    pub content_preview: String,
    pub match_reason: String,
}

/// A preserved record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectPreserved {
    pub id: String,
    pub source: String,
    pub content_preview: String,
}

impl MemoryInspectTool {
    /// Create a new MemoryInspectTool
    pub fn new() -> Self {
        let schema = ToolSchema::builder("MemoryInspectInput", "Input for memory inspection")
            .optional_property("limit", JsonSchema::number("Maximum memories to inspect (default: 1000)"))
            .optional_property("filter_source", JsonSchema::string("Filter by source: Automatic or Explicit"))
            .optional_property("full_content", JsonSchema::boolean("Show full content instead of preview"))
            .build();

        Self {
            metadata: ToolMetadata::new(
                "memory_inspect",
                "Inspect memory store contents and identify cleanup candidates.\n\
                \n\
                This tool queries the memory store and reports:\
                - Total memory count by source\n\
                - Potential cleanup candidates matching test/debug patterns\n\
                - Content previews for review\n\
                \n\
                Safe to run anytime - performs no deletions.",
                schema,
            ),
        }
    }

    /// Check if content matches test patterns
    fn identify_test_content(content: &str) -> Option<&'static str> {
        if content.starts_with("Request:") {
            return Some("Starts with 'Request:' (plan summary)");
        }
        if content.contains("memory: Object {") {
            return Some("Contains 'memory: Object {' (debug dump)");
        }
        if content.contains("memoryObject {") {
            return Some("Contains 'memoryObject {' (serialized)");
        }
        if content.contains("DREX_MEMORY_TEST_") && !content.contains("FINAL_TEST_001") {
            return Some("Contains DREX_MEMORY_TEST_ marker (excludes FINAL_TEST_001)");
        }
        if content.contains(r#""memory":"#) {
            return Some("Contains JSON-encoded memory field");
        }
        None
    }
}

impl Default for MemoryInspectTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MemoryInspectTool {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn required_capabilities(&self) -> &crate::capability::CapabilitySet {
        use crate::capability::Capability;
        static CAPS: std::sync::OnceLock<crate::capability::CapabilitySet> =
            std::sync::OnceLock::new();
        CAPS.get_or_init(|| {
            let mut caps = crate::capability::CapabilitySet::new();
            caps.add(Capability::MemoryRead);
            caps
        })
    }

    async fn execute(&self, ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        let inspect_input: MemoryInspectInput = input.parse().map_err(|e| {
            ToolError::InvalidInput {
                tool: self.name().to_string(),
                reason: format!("failed to parse input: {}", e),
            }
        })?;

        let memory_store = ctx.memory_store().ok_or_else(|| ToolError::ExecutionFailed {
            tool: self.name().to_string(),
            reason: "Memory store not available".to_string(),
        })?;

        // Query memories
        let limit = inspect_input.limit.max(1).min(10000);
        let query = MemoryQuery::search("").limit(limit);
        let memories = memory_store
            .retrieve(&query)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                tool: self.name().to_string(),
                reason: format!("Failed to retrieve memories: {}", e),
            })?;

        info!(count = memories.len(), "Retrieved memories for inspection");

        let mut automatic_count = 0;
        let mut explicit_count = 0;
        let mut cleanup_candidates = Vec::new();
        let mut preserved_sample = Vec::new();

        for memory in &memories {
            // Count by source
            match memory.metadata.source {
                MemorySource::Automatic => automatic_count += 1,
                _ => explicit_count += 1, // All other sources are treated as "user-facing"
            }

            // Check filter
            if let Some(ref filter) = inspect_input.filter_source {
                let source_str = format!("{:?}", memory.metadata.source);
                if !source_str.to_lowercase().contains(&filter.to_lowercase()) {
                    continue;
                }
            }

            // Content preview
            let content_preview = if inspect_input.full_content {
                memory.content.clone()
            } else if memory.content.len() > 150 {
                format!("{}...", &memory.content[..150])
            } else {
                memory.content.clone()
            };

            // Check if it's a cleanup candidate
            if let Some(reason) = Self::identify_test_content(&memory.content) {
                // Only candidates with Automatic source
                if memory.metadata.source == MemorySource::Automatic {
                    cleanup_candidates.push(InspectCandidate {
                        id: memory.id.to_string(),
                        source: "Automatic".to_string(),
                        content_preview: content_preview.clone(),
                        match_reason: reason.to_string(),
                    });
                }
            }

            // Sample of preserved records (non-Automatic sources or preserved Automatic)
            if preserved_sample.len() < 5 {
                // Prioritize non-Automatic sources for the sample
                if memory.metadata.source != MemorySource::Automatic {
                    preserved_sample.push(InspectPreserved {
                        id: memory.id.to_string(),
                        source: format!("{:?}", memory.metadata.source),
                        content_preview,
                    });
                }
            }
        }

        // Add some Automatic records that are NOT candidates to show what gets preserved
        for memory in &memories {
            if preserved_sample.len() >= 10 {
                break;
            }
            if memory.metadata.source == MemorySource::Automatic {
                let content_preview = if memory.content.len() > 150 {
                    format!("{}...", &memory.content[..150])
                } else {
                    memory.content.clone()
                };

                // Skip if it's already a candidate
                if Self::identify_test_content(&memory.content).is_some() {
                    continue;
                }

                preserved_sample.push(InspectPreserved {
                    id: memory.id.to_string(),
                    source: "Automatic (preserved)".to_string(),
                    content_preview,
                });
            }
        }

        let output = MemoryInspectOutput {
            total_inspected: memories.len(),
            automatic_count,
            explicit_count,
            cleanup_candidates,
            preserved_sample,
        };

        Ok(ExecutionResult::success(json!(output)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_test_content_request() {
        let content = "Request: create a file\nResult: success";
        let result = MemoryInspectTool::identify_test_content(content);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Request:"));
    }

    #[test]
    fn test_identify_test_content_memory_object() {
        let content = r#"memory: Object { content: "test" }"#;
        let result = MemoryInspectTool::identify_test_content(content);
        assert!(result.is_some());
    }

    #[test]
    fn test_identify_test_content_drex_test() {
        let content = "DREX_MEMORY_TEST_123 some test data";
        let result = MemoryInspectTool::identify_test_content(content);
        assert!(result.is_some());
    }

    #[test]
    fn test_no_match_for_legitimate_content() {
        let content = "The user likes to code in Rust";
        let result = MemoryInspectTool::identify_test_content(content);
        assert!(result.is_none());
    }

    #[test]
    fn test_inspect_output_serialization() {
        let output = MemoryInspectOutput {
            total_inspected: 10,
            automatic_count: 3,
            explicit_count: 7,
            cleanup_candidates: vec![InspectCandidate {
                id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
                source: "Automatic".to_string(),
                content_preview: "Request: test".to_string(),
                match_reason: "Test pattern".to_string(),
            }],
            preserved_sample: vec![InspectPreserved {
                id: "123e4567-e89b-12d3-a456-426614174001".to_string(),
                source: "Explicit".to_string(),
                content_preview: "User memory".to_string(),
            }],
        };

        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(json["total_inspected"], 10);
        assert_eq!(json["automatic_count"], 3);
        assert_eq!(json["explicit_count"], 7);
    }
}
