//! Tool trait and execution context

use crate::capability::CapabilitySet;
use crate::error::ToolResult;
use crate::result::ExecutionResult;
use crate::schema::ToolSchema;
use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// Input passed to a tool during execution.
///
/// This is a flexible container that can be constructed from JSON
/// or strongly-typed inputs, making it easy to validate and execute tools.
#[derive(Debug, Clone)]
pub struct ToolInput {
    /// The raw JSON value representing the input
    pub value: Value,
}

impl ToolInput {
    /// Create a ToolInput from a JSON value
    pub fn from_json(value: Value) -> ToolResult<Self> {
        // Validate that it's an object (tools expect named parameters)
        if !value.is_object() {
            return Err(crate::error::ToolError::InvalidInput {
                tool: "unknown".to_string(),
                reason: "tool input must be a JSON object".to_string(),
            });
        }
        Ok(Self { value })
    }

    /// Create a ToolInput from any serializable type
    pub fn from_serializable<T: Serialize>(value: T) -> ToolResult<Self> {
        let json = serde_json::to_value(value)
            .map_err(|e| crate::error::ToolError::Serialization(e.to_string()))?;
        Self::from_json(json)
    }

    /// Get a field from the input by name
    pub fn get(&self, field: &str) -> Option<&Value> {
        self.value.get(field)
    }

    /// Get a required field, returning an error if missing
    pub fn require(&self, field: &str) -> ToolResult<&Value> {
        self.value
            .get(field)
            .ok_or_else(|| crate::error::ToolError::InvalidInput {
                tool: "unknown".to_string(),
                reason: format!("missing required field: {}", field),
            })
    }

    /// Get a string field
    pub fn get_string(&self, field: &str) -> Option<&str> {
        self.value.get(field).and_then(Value::as_str)
    }

    /// Get a required string field
    pub fn require_string(&self, field: &str) -> ToolResult<&str> {
        self.require(field)?
            .as_str()
            .ok_or_else(|| crate::error::ToolError::InvalidInput {
                tool: "unknown".to_string(),
                reason: format!("field '{}' must be a string", field),
            })
    }

    /// Deserialize the input into a concrete type
    pub fn parse<T: DeserializeOwned>(&self) -> ToolResult<T> {
        serde_json::from_value(self.value.clone())
            .map_err(|e| crate::error::ToolError::InvalidInput {
                tool: "unknown".to_string(),
                reason: format!("failed to parse input: {}", e),
            })
    }

    /// Check if a field exists
    pub fn has(&self, field: &str) -> bool {
        self.value.get(field).is_some()
    }
}

impl From<Value> for ToolInput {
    fn from(value: Value) -> Self {
        Self { value }
    }
}

/// Metadata about a tool, used for discovery and documentation.
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    /// The unique tool name/identifier (e.g., "echo", "http_get", "file_read")
    pub name: String,
    /// Human-readable description of what the tool does
    pub description: String,
    /// JSON Schema defining the expected input structure
    pub input_schema: ToolSchema,
}

impl ToolMetadata {
    /// Create metadata for a tool
    pub fn new(name: impl Into<String>, description: impl Into<String>, schema: ToolSchema) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema: schema,
        }
    }

    /// Get a brief (one-line) description
    pub fn brief_description(&self) -> &str {
        self.description.lines().next().unwrap_or(&self.description)
    }

    /// Create a markdown-formatted description for display
    pub fn markdown_description(&self) -> String {
        let mut output = format!("## {}\n\n{}", self.name, self.description);

        output.push_str("\n\n### Input Schema\n\n");
        if let Ok(schema_str) = serde_json::to_string_pretty(&self.input_schema) {
            output.push_str("```json\n");
            output.push_str(&schema_str);
            output.push_str("\n```");
        }

        output
    }
}

/// Context provided to tools during execution.
///
/// The context carries the capabilities granted to this execution by the
/// Drex runtime. Tools use this to verify they are authorized to perform
/// operations.
///
/// # Security
///
/// Capabilities should only be set by the trusted Drex runtime. Tools must
/// not accept capabilities from model-generated arguments.
#[derive(Debug, Clone, Default)]
pub struct ToolContext {
    /// Arbitrary context data (key-value pairs)
    data: HashMap<String, Value>,
    /// Capabilities granted to this execution
    granted_capabilities: CapabilitySet,
}

impl ToolContext {
    /// Create a new empty context with no capabilities.
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            granted_capabilities: CapabilitySet::harmless(),
        }
    }

    /// Create a context with specific granted capabilities.
    pub fn with_capabilities(capabilities: CapabilitySet) -> Self {
        Self {
            data: HashMap::new(),
            granted_capabilities: capabilities,
        }
    }

    /// Get the capabilities granted to this execution.
    pub fn capabilities(&self) -> &CapabilitySet {
        &self.granted_capabilities
    }

    /// Check if a specific capability is granted.
    pub fn has_capability(&self, capability: crate::capability::Capability) -> bool {
        self.granted_capabilities.has(capability)
    }

    /// Check if all capabilities in a set are granted.
    pub fn has_capabilities(&self, capabilities: &CapabilitySet) -> bool {
        self.granted_capabilities.has_all(capabilities)
    }

    /// Get a context value by key
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    /// Set a context value
    pub fn set(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }

    /// Check if a key exists
    pub fn has(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Get all context keys
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.keys()
    }

    /// Create a context with a request ID
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.data.insert("request_id".to_string(), id.into().into());
        self
    }

    /// Get the request ID from the context
    pub fn request_id(&self) -> Option<&str> {
        self.data.get("request_id").and_then(Value::as_str)
    }
}

/// The core trait for any executable tool.
///
/// Tools are the building blocks of Drex's capability system. Each tool
/// provides a well-defined, typed interface for performing a specific task.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool's metadata (name, description, schema)
    fn metadata(&self) -> &ToolMetadata;

    /// Get the tool name (convenience method)
    fn name(&self) -> &str {
        &self.metadata().name
    }

    /// Get the tool description (convenience method)
    fn description(&self) -> &str {
        &self.metadata().description
    }

    /// Get the capabilities required by this tool.
    ///
    /// Returns an empty set for harmless tools that don't require
    /// any permissions (like `echo`).
    fn required_capabilities(&self) -> &CapabilitySet {
        // Default: no capabilities required (harmless tool)
        static EMPTY: std::sync::OnceLock<CapabilitySet> = std::sync::OnceLock::new();
        EMPTY.get_or_init(CapabilitySet::harmless)
    }

    /// Execute the tool with the given input
    ///
    /// # Arguments
    /// * `ctx` - Execution context (super-minimal in Phase 3.1)
    /// * `input` - The input data, validated against the tool's schema
    ///
    /// # Returns
    /// The execution result, which may contain structured data or an error
    async fn execute(&self, ctx: &ToolContext, input: ToolInput) -> ToolResult<ExecutionResult>;

    /// Check if the context has all required capabilities.
    ///
    /// Returns `Ok(())` if authorized, or an error listing missing capabilities.
    fn check_authorization(&self, ctx: &ToolContext) -> ToolResult<()> {
        let required = self.required_capabilities();
        if required.is_empty() {
            return Ok(());
        }

        let missing = ctx.capabilities().missing(required);
        if missing.is_empty() {
            Ok(())
        } else {
            Err(crate::error::ToolError::Unauthorized {
                tool: self.name().to_string(),
                missing,
            })
        }
    }

    /// Validate that the input conforms to the tool's schema
    ///
    /// This is called automatically before execution, but can also be
    /// used to pre-validate inputs before attempting execution.
    fn validate_input(&self, input: &ToolInput) -> ToolResult<()> {
        let schema = &self.metadata().input_schema;

        // Check required fields
        for field in &schema.required {
            if !input.has(field) {
                return Err(crate::error::ToolError::InvalidInput {
                    tool: self.name().to_string(),
                    reason: format!("missing required field: {}", field),
                });
            }
        }

        // Check that all fields are in the schema (no additional properties)
        if schema.additional_properties == Some(false) {
            if let Some(obj) = input.value.as_object() {
                for key in obj.keys() {
                    if !schema.properties.contains_key(key) {
                        return Err(crate::error::ToolError::InvalidInput {
                            tool: self.name().to_string(),
                            reason: format!("unexpected field: {}", key),
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// A boxed tool type for storage in registries
pub type BoxedTool = Box<dyn Tool>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_input_from_json_object() {
        let value = serde_json::json!({"message": "hello"});
        let input = ToolInput::from_json(value).unwrap();
        assert_eq!(input.get_string("message"), Some("hello"));
    }

    #[test]
    fn tool_input_rejects_non_object() {
        let value = serde_json::json!("not an object");
        let result = ToolInput::from_json(value);
        assert!(result.is_err());
    }

    #[test]
    fn tool_input_require_field() {
        let value = serde_json::json!({"message": "hello"});
        let input = ToolInput::from_json(value).unwrap();

        assert!(input.require("message").is_ok());
        assert!(input.require("missing").is_err());
    }

    #[test]
    fn tool_input_require_string() {
        let value = serde_json::json!({"message": "hello", "number": 42});
        let input = ToolInput::from_json(value).unwrap();

        assert_eq!(input.require_string("message").unwrap(), "hello");
        assert!(input.require_string("number").is_err()); // Not a string
        assert!(input.require_string("missing").is_err());
    }

    #[test]
    fn tool_context_basic() {
        let ctx = ToolContext::new()
            .set("key1", "value1")
            .set("key2", 42);

        assert!(ctx.has("key1"));
        assert!(ctx.has("key2"));
        assert!(!ctx.has("missing"));

        assert_eq!(ctx.get("key1").and_then(Value::as_str), Some("value1"));
        assert_eq!(ctx.get("key2").and_then(Value::as_i64), Some(42));
    }

    #[test]
    fn tool_context_request_id() {
        let ctx = ToolContext::new().with_request_id("abc123");
        assert_eq!(ctx.request_id(), Some("abc123"));
    }

    #[test]
    fn tool_metadata_markdown() {
        let schema = ToolSchema::builder("Echo", "Echo a message").build();
        let metadata = ToolMetadata::new("echo", "Echo tool\n\nMore details", schema);

        let markdown = metadata.markdown_description();
        assert!(markdown.contains("## echo"));
        assert!(markdown.contains("Echo tool"));
        assert!(markdown.contains("### Input Schema"));
    }

    #[test]
    fn tool_metadata_brief_description() {
        let schema = ToolSchema::builder("Echo", "Echo a message").build();
        let metadata = ToolMetadata::new("echo", "First line\nSecond line", schema);
        assert_eq!(metadata.brief_description(), "First line");
    }
}
