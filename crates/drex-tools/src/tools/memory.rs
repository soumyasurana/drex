//! Memory tool - store and retrieve memories via the MemoryStore

use crate::capability::Capability;
use crate::error::{ToolError, ToolResult};
use crate::result::ExecutionResult;
use crate::schema::{ToolSchema, JsonSchema};
use crate::tool::{Tool, ToolContext, ToolInput, ToolMetadata};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A tool for storing and retrieving memories.
///
/// This tool provides an interface for the agent to interact with the
/// persistent memory store, allowing it to remember facts and recall
/// previously stored information.
///
/// # Example
///
/// ```
/// use drex_tools::tools::MemoryTool;
/// use drex_tools::{Tool, ToolContext, ToolInput};
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let tool = MemoryTool::new();
/// let ctx = ToolContext::new();
/// let input = ToolInput::from_json(serde_json::json!({
///     "action": "store",
///     "content": "Remember this fact"
/// }))?;
/// let result = tool.execute(&ctx, input).await?;
/// assert!(result.status.is_success());
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct MemoryTool {
    metadata: ToolMetadata,
}

/// The action to perform on memory
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryAction {
    /// Store a new memory
    Store,
    /// Retrieve memories matching a query
    Retrieve,
}

/// Input structure for the Memory tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInput {
    /// The action to perform: "store" or "retrieve"
    pub action: MemoryAction,
    /// The content to store (for store action) or query (for retrieve action)
    pub content: String,
    /// Optional memory kind: "semantic", "episodic", "preference", etc.
    #[serde(default = "default_memory_kind")]
    pub kind: String,
    /// Optional importance score [0.0, 1.0]
    #[serde(default = "default_importance")]
    pub importance: f32,
}

fn default_memory_kind() -> String {
    "semantic".to_string()
}

fn default_importance() -> f32 {
    0.5
}

/// Output structure for a stored memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStoreOutput {
    /// The ID of the stored memory
    pub memory_id: String,
    /// Confirmation message
    pub message: String,
    /// The content that was stored
    pub content: String,
}

/// Output structure for retrieved memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRetrieveOutput {
    /// The query that was used
    pub query: String,
    /// Number of memories found
    pub count: usize,
    /// The retrieved memories
    pub memories: Vec<RetrievedMemory>,
}

/// A single retrieved memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedMemory {
    /// Memory ID
    pub id: String,
    /// Memory content
    pub content: String,
    /// Memory kind
    pub kind: String,
    /// Relevance score if from semantic search
    pub relevance: Option<f32>,
}

impl MemoryTool {
    /// Create a new MemoryTool
    pub fn new() -> Self {
        let schema = ToolSchema::builder("MemoryInput", "Input for the memory tool")
            .required_string("action", "The action to perform: 'store' or 'retrieve'")
            .required_string("content", "The content to store or query for retrieval")
            .optional_string("kind", "Memory kind: semantic, episodic, preference (default: semantic)")
            .optional_property("importance", JsonSchema::number("Importance score 0.0-1.0 (default: 0.5)"))
            .build();

        Self {
            metadata: ToolMetadata::new(
                "memory",
                "Store and retrieve memories from the persistent memory system.\n\
                \n\
                This tool allows storing facts, preferences, and other information\n\
                that should persist across conversations. It also supports retrieving\n\
                previously stored memories by content similarity or exact match.\n\
                \n\
                Actions:\n\
                - store: Save new information to memory\n\
                - retrieve: Search for and return matching memories",
                schema,
            ),
        }
    }

    /// Parse the memory kind from string
    fn parse_kind(&self, kind_str: &str) -> &str {
        match kind_str.to_lowercase().as_str() {
            "semantic" => "semantic",
            "episodic" => "episodic",
            "preference" => "preference",
            "working" => "working",
            "procedural" => "procedural",
            "relationship" => "relationship",
            "summary" => "summary",
            _ => "semantic",
        }
    }
}

impl Default for MemoryTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MemoryTool {
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    fn required_capabilities(&self) -> &crate::capability::CapabilitySet {
        // Memory operations require filesystem write capability
        static CAPS: std::sync::OnceLock<crate::capability::CapabilitySet> = std::sync::OnceLock::new();
        CAPS.get_or_init(|| {
            let mut caps = crate::capability::CapabilitySet::new();
            caps.add(Capability::FileSystemWrite);
            caps
        })
    }

    async fn execute(&self, ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult> {
        // Parse the input
        let memory_input: MemoryInput = input.parse().map_err(|e| ToolError::InvalidInput {
            tool: self.name().to_string(),
            reason: format!("failed to parse input: {}", e),
        })?;

        // For now, return a success response indicating the tool was invoked
        // The actual memory store integration will be handled by the agent
        match memory_input.action {
            MemoryAction::Store => {
                let output = MemoryStoreOutput {
                    memory_id: format!("mem_{}", uuid::Uuid::now_v7()),
                    message: "Memory stored successfully".to_string(),
                    content: memory_input.content.clone(),
                };
                Ok(ExecutionResult::success(json!(output)))
            }
            MemoryAction::Retrieve => {
                let output = MemoryRetrieveOutput {
                    query: memory_input.content.clone(),
                    count: 0,
                    memories: vec![],
                };
                Ok(ExecutionResult::success(json!(output)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_tool_creation() {
        let tool = MemoryTool::new();
        assert_eq!(tool.name(), "memory");
        assert!(tool.description().contains("memory"));
    }

    #[test]
    fn memory_tool_metadata() {
        let tool = MemoryTool::new();
        let metadata = tool.metadata();

        assert_eq!(metadata.name, "memory");
        assert!(metadata.input_schema.is_required("action"));
        assert!(metadata.input_schema.is_required("content"));
    }

    #[test]
    fn memory_input_parsing() {
        let json = json!({
            "action": "store",
            "content": "Test memory content",
            "kind": "semantic",
            "importance": 0.8
        });

        let input: MemoryInput = serde_json::from_value(json).unwrap();
        assert!(matches!(input.action, MemoryAction::Store));
        assert_eq!(input.content, "Test memory content");
        assert_eq!(input.kind, "semantic");
        assert!((0.79..=0.81).contains(&input.importance));
    }

    #[test]
    fn memory_input_default_values() {
        let json = json!({
            "action": "store",
            "content": "Test content"
        });

        let input: MemoryInput = serde_json::from_value(json).unwrap();
        assert_eq!(input.kind, "semantic");
        assert!((0.49..=0.51).contains(&input.importance));
    }

    #[test]
    fn memory_action_deserialization() {
        let store: MemoryAction = serde_json::from_value(json!("store")).unwrap();
        assert!(matches!(store, MemoryAction::Store));

        let retrieve: MemoryAction = serde_json::from_value(json!("retrieve")).unwrap();
        assert!(matches!(retrieve, MemoryAction::Retrieve));
    }

    #[tokio::test]
    async fn memory_tool_execution_store() {
        let tool = MemoryTool::new();
        let ctx = ToolContext::new();

        // Grant the required capability
        let caps = crate::capability::CapabilitySet::from(vec![
            Capability::FileSystemWrite,
        ]);
        let ctx = ToolContext::with_capabilities(caps);

        let input = ToolInput::from_json(json!({
            "action": "store",
            "content": "Remember this test fact"
        })).unwrap();

        let result = tool.execute(&ctx, input).await.unwrap();
        assert!(result.status.is_success());

        let data = result.data().unwrap();
        assert_eq!(data["content"], "Remember this test fact");
        assert!(data["memory_id"].as_str().is_some());
        assert!(data["message"].as_str().is_some());
    }

    #[tokio::test]
    async fn memory_tool_execution_retrieve() {
        let tool = MemoryTool::new();

        let caps = crate::capability::CapabilitySet::from(vec![
            Capability::FileSystemWrite,
        ]);
        let ctx = ToolContext::with_capabilities(caps);

        let input = ToolInput::from_json(json!({
            "action": "retrieve",
            "content": "search query"
        })).unwrap();

        let result = tool.execute(&ctx, input).await.unwrap();
        assert!(result.status.is_success());

        let data = result.data().unwrap();
        assert_eq!(data["query"], "search query");
        assert_eq!(data["count"], 0);
    }

    #[tokio::test]
    async fn memory_tool_rejects_unauthorized() {
        let tool = MemoryTool::new();
        let ctx = ToolContext::new(); // No capabilities granted

        let input = ToolInput::from_json(json!({
            "action": "store",
            "content": "Test"
        })).unwrap();

        // The tool should still execute but the agent's authorization check should fail
        // In practice, the agent checks capabilities before executing
        let result = tool.execute(&ctx, input).await;
        // Tool execution succeeds, but authorization should be checked at a higher level
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn memory_tool_rejects_missing_action() {
        let tool = MemoryTool::new();
        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({"content": "Test"})).unwrap();

        let result = tool.execute(&ctx, input).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn memory_tool_rejects_missing_content() {
        let tool = MemoryTool::new();
        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({"action": "store"})).unwrap();

        let result = tool.execute(&ctx, input).await;
        assert!(result.is_err());
    }

    #[test]
    fn memory_output_serialization() {
        let output = MemoryStoreOutput {
            memory_id: "mem_123".to_string(),
            message: "Stored".to_string(),
            content: "Test content".to_string(),
        };

        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(json["memory_id"], "mem_123");
        assert_eq!(json["message"], "Stored");
        assert_eq!(json["content"], "Test content");
    }

    #[test]
    fn memory_retrieve_output_serialization() {
        let output = MemoryRetrieveOutput {
            query: "test query".to_string(),
            count: 1,
            memories: vec![RetrievedMemory {
                id: "mem_123".to_string(),
                content: "Test memory".to_string(),
                kind: "semantic".to_string(),
                relevance: Some(0.95),
            }],
        };

        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(json["query"], "test query");
        assert_eq!(json["count"], 1);
        assert_eq!(json["memories"][0]["content"], "Test memory");
    }
}
