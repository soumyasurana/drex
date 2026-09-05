//! Tool registry for discovering and executing tools

use crate::capability::CapabilitySet;
use crate::error::{ToolError, ToolResult};
use crate::result::ExecutionResult;
use crate::tool::{BoxedTool, Tool, ToolContext, ToolInput, ToolMetadata};
use std::collections::hash_map::Iter;
use std::collections::HashMap;

/// A registry for tools, enabling discovery and lookup by name.
///
/// The registry maintains a mapping from tool names to tool instances.
/// Tools can be registered at runtime or loaded from configuration.
///
/// # Example
///
/// ```
/// use drex_tools::{ToolRegistry, ToolContext};
/// use drex_tools::tools::EchoTool;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut registry = ToolRegistry::new();
/// registry.register(Box::new(EchoTool::new()))?;
///
/// let tool = registry.get("echo")?;
/// assert_eq!(tool.name(), "echo");
/// # Ok(())
/// # }
/// ```
pub struct ToolRegistry {
    /// Map of tool name to tool instance
    tools: HashMap<String, Box<dyn Tool>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .field("count", &self.tools.len())
            .finish()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Create a new, empty registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool in the registry.
    ///
    /// # Errors
    /// Returns an error if a tool with the same name is already registered.
    ///
    /// # Example
    ///
    /// ```
    /// use drex_tools::ToolRegistry;
    /// use drex_tools::tools::EchoTool;
    ///
    /// let mut registry = ToolRegistry::new();
    /// assert!(registry.register(Box::new(EchoTool::new())).is_ok());
    /// ```
    pub fn register(&mut self, tool: Box<dyn Tool>) -> ToolResult<()> {
        let name = tool.name().to_string();

        if self.tools.contains_key(&name) {
            return Err(ToolError::Duplicate(name));
        }

        self.tools.insert(name, tool);
        Ok(())
    }

    /// Get a tool by name.
    ///
    /// # Errors
    /// Returns an error if the tool is not found in the registry.
    ///
    /// # Example
    ///
    /// ```
    /// use drex_tools::ToolRegistry;
    /// use drex_tools::tools::EchoTool;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut registry = ToolRegistry::new();
    /// registry.register(Box::new(EchoTool::new()))?;
    ///
    /// let tool = registry.get("echo")?;
    /// assert_eq!(tool.name(), "echo");
    /// # Ok(())
    /// # }
    /// ```
    pub fn get(&self, name: &str) -> ToolResult<&dyn Tool> {
        self.tools
            .get(name)
            .map(|t| t.as_ref())
            .ok_or_else(|| ToolError::NotFound(name.to_string()))
    }

    /// Check if a tool is registered.
    ///
    /// # Example
    ///
    /// ```
    /// use drex_tools::ToolRegistry;
    /// use drex_tools::tools::EchoTool;
    ///
    /// let mut registry = ToolRegistry::new();
    /// assert!(!registry.contains("echo"));
    ///
    /// registry.register(Box::new(EchoTool::new())).unwrap();
    /// assert!(registry.contains("echo"));
    /// ```
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Remove a tool from the registry.
    ///
    /// Returns the removed tool if it existed, None otherwise.
    pub fn remove(&mut self, name: &str) -> Option<BoxedTool> {
        self.tools.remove(name)
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// List all registered tool names.
    ///
    /// Returns an iterator over tool names in alphabetical order.
    ///
    /// # Example
    ///
    /// ```
    /// use drex_tools::ToolRegistry;
    /// use drex_tools::tools::EchoTool;
    ///
    /// let mut registry = ToolRegistry::new();
    /// registry.register(Box::new(EchoTool::new())).unwrap();
    ///
    /// let names: Vec<_> = registry.list_names().collect();
    /// assert!(names.contains(&"echo"));
    /// ```
    pub fn list_names(&self) -> impl Iterator<Item = &str> {
        let mut names: Vec<_> = self.tools.keys().map(|s| s.as_str()).collect();
        names.sort();
        names.into_iter()
    }

    /// Get metadata for all registered tools.
    ///
    /// Returns an iterator over tool metadata.
    pub fn list_metadata(&self) -> impl Iterator<Item = &ToolMetadata> {
        self.tools.values().map(|t| t.metadata())
    }

    /// Clear all registered tools.
    pub fn clear(&mut self) {
        self.tools.clear();
    }

    /// Iterate over all tools.
    pub fn iter(&self) -> Iter<'_, String, BoxedTool> {
        self.tools.iter()
    }

    /// Get metadata for a specific tool.
    ///
    /// # Errors
    /// Returns an error if the tool is not found.
    pub fn metadata(&self, name: &str) -> ToolResult<&ToolMetadata> {
        self.get(name).map(|t| t.metadata())
    }

    /// Create a markdown listing of all tools.
    ///
    /// Useful for generating documentation.
    pub fn generate_markdown_docs(&self) -> String {
        let mut output = String::from("# Available Tools\n\n");

        let mut tools: Vec<_> = self.tools.values().collect();
        tools.sort_by(|a, b| a.name().cmp(b.name()));

        for tool in tools {
            output.push_str(&tool.metadata().markdown_description());
            output.push_str("\n\n---\n\n");
        }

        output
    }
}

/// A tool registry wrapper that enforces capability-based authorization.
///
/// This is the recommended way to execute tools, as it ensures authorization
/// checks are performed before execution.
///
/// # Example
///
/// ```
/// use drex_tools::{ToolRegistry, AuthorizedToolRegistry, ToolContext};
/// use drex_tools::tools::EchoTool;
/// use drex_tools::capability::CapabilitySet;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create and populate the registry
/// let mut registry = ToolRegistry::new();
/// registry.register(Box::new(EchoTool::new()))?;
///
/// // Wrap with authorization using granted capabilities
/// let granted = CapabilitySet::harmless();
/// let authorized = AuthorizedToolRegistry::new(&registry, granted);
///
/// // Execute with authorization check
/// let ctx = ToolContext::new();
/// let input = drex_tools::ToolInput::from_json(
///     serde_json::json!({"message": "hello"})
/// )?;
/// let result = authorized.execute("echo", &ctx, input).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct AuthorizedToolRegistry<'a> {
    registry: &'a ToolRegistry,
    granted_capabilities: CapabilitySet,
}

impl<'a> AuthorizedToolRegistry<'a> {
    /// Create an authorized registry with the given granted capabilities.
    pub fn new(registry: &'a ToolRegistry, granted: CapabilitySet) -> Self {
        Self {
            registry,
            granted_capabilities: granted,
        }
    }

    /// Create an authorized registry with no capabilities (harmless tools only).
    pub fn harmless(registry: &'a ToolRegistry) -> Self {
        Self::new(registry, CapabilitySet::harmless())
    }

    /// Execute a tool with authorization enforcement.
    ///
    /// This method:
    /// 1. Looks up the tool by name
    /// 2. Checks if the granted capabilities include all required capabilities
    /// 3. If authorized, executes the tool
    /// 4. If not authorized, returns an error before executing
    ///
    /// # Errors
    /// Returns an error if:
    /// - The tool is not found
    /// - The tool requires capabilities that are not granted
    pub async fn execute(
        &self,
        tool_name: &str,
        ctx: &ToolContext,
        input: ToolInput,
    ) -> ToolResult<ExecutionResult> {
        // Get the tool
        let tool = self.registry.get(tool_name)?;

        // Check authorization BEFORE executing
        let required = tool.required_capabilities();
        if !required.is_empty() {
            let missing = self.granted_capabilities.missing(required);
            if !missing.is_empty() {
                return Err(ToolError::Unauthorized {
                    tool: tool_name.to_string(),
                    missing,
                });
            }
        }

        // Authorized - execute the tool
        tool.execute(ctx, input).await
    }

    /// Check if a tool can be executed with current capabilities.
    ///
    /// Returns `true` if the tool exists and all required capabilities
    /// are granted.
    pub fn can_execute(&self, tool_name: &str) -> bool {
        if let Ok(tool) = self.registry.get(tool_name) {
            self.granted_capabilities.has_all(tool.required_capabilities())
        } else {
            false
        }
    }

    /// Get the granted capabilities.
    pub fn capabilities(&self) -> &CapabilitySet {
        &self.granted_capabilities
    }

    /// Get the underlying registry.
    pub fn registry(&self) -> &ToolRegistry {
        self.registry
    }

    /// List all tools that can be executed with current capabilities.
    pub fn list_executable(&self) -> Vec<&ToolMetadata> {
        self.registry
            .list_metadata()
            .filter(|m| {
                if let Ok(tool) = self.registry.get(&m.name) {
                    self.granted_capabilities.has_all(tool.required_capabilities())
                } else {
                    false
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::EchoTool;

    #[test]
    fn registry_new_is_empty() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn registry_register_tool() {
        let mut registry = ToolRegistry::new();
        assert!(registry.register(Box::new(EchoTool::new())).is_ok());
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("echo"));
    }

    #[test]
    fn registry_rejects_duplicate() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();

        let result = registry.register(Box::new(EchoTool::new()));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ToolError::Duplicate(_)));
    }

    #[test]
    fn registry_get_existing() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();

        let tool = registry.get("echo");
        assert!(tool.is_ok());
        assert_eq!(tool.unwrap().name(), "echo");
    }

    #[test]
    fn registry_get_unknown() {
        let registry = ToolRegistry::new();
        let result = registry.get("unknown");
        assert!(result.is_err());
        match result {
            Err(ToolError::NotFound(name)) => assert_eq!(name, "unknown"),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn registry_remove() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();
        assert!(registry.contains("echo"));

        let removed = registry.remove("echo");
        assert!(removed.is_some());
        assert!(!registry.contains("echo"));

        let not_removed = registry.remove("echo");
        assert!(not_removed.is_none());
    }

    #[test]
    fn registry_clear() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();
        assert!(!registry.is_empty());

        registry.clear();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn registry_list_names() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();

        let names: Vec<_> = registry.list_names().collect();
        assert_eq!(names, vec!["echo"]);
    }

    #[test]
    fn registry_list_metadata() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();

        let metadatas: Vec<_> = registry.list_metadata().collect();
        assert_eq!(metadatas.len(), 1);
        assert_eq!(metadatas[0].name, "echo");
    }

    #[test]
    fn registry_metadata_method() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();

        let metadata = registry.metadata("echo");
        assert!(metadata.is_ok());
        assert_eq!(metadata.unwrap().name, "echo");

        let not_found = registry.metadata("unknown");
        assert!(not_found.is_err());
    }
}

// =========================================================================
// AuthorizedToolRegistry Tests
// =========================================================================

#[cfg(test)]
mod authorization_tests {
    use super::*;
    use crate::capability::Capability;
    use crate::schema::ToolSchema;
    use crate::tool::{Tool, ToolContext, ToolInput, ToolMetadata};
    use crate::tools::EchoTool;
    use async_trait::async_trait;
    use serde_json::json;

    /// A mock tool that requires specific capabilities
    struct MockSecureTool {
        metadata: ToolMetadata,
        required: CapabilitySet,
        should_execute: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl MockSecureTool {
        fn new(name: &str, required: CapabilitySet) -> Self {
            Self {
                metadata: ToolMetadata::new(
                    name,
                    format!("Tool {} requiring {:?}", name, required),
                    ToolSchema::default(),
                ),
                required,
                should_execute: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }

        fn was_executed(&self) -> bool {
            self.should_execute.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl Tool for MockSecureTool {
        fn metadata(&self) -> &ToolMetadata {
            &self.metadata
        }

        fn required_capabilities(&self) -> &CapabilitySet {
            &self.required
        }

        async fn execute(
            &self,
            _ctx: &ToolContext,
            _input: ToolInput,
        ) -> ToolResult<ExecutionResult> {
            self.should_execute
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(ExecutionResult::success(json!({"executed": true})))
        }
    }

    #[tokio::test]
    async fn echo_executes_with_no_capabilities() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();

        // Echo requires no capabilities
        let authorized = AuthorizedToolRegistry::harmless(&registry);
        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({"message": "hello"})).unwrap();

        let result = authorized.execute("echo", &ctx, input).await;
        assert!(result.is_ok());
        assert!(result.unwrap().status.is_success());
    }

    #[tokio::test]
    async fn tool_with_fs_read_executes_when_granted() {
        let mut registry = ToolRegistry::new();
        let fs_tool = MockSecureTool::new(
            "file_read",
            CapabilitySet::from(vec![Capability::FileSystemRead]),
        );
        let tool_name = fs_tool.name().to_string();
        registry.register(Box::new(fs_tool)).unwrap();

        // Grant the capability
        let granted = CapabilitySet::from(vec![Capability::FileSystemRead]);
        let authorized = AuthorizedToolRegistry::new(&registry, granted);

        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({})).unwrap();

        let result = authorized.execute(&tool_name, &ctx, input).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn tool_with_fs_read_rejected_without_grant() {
        let mut registry = ToolRegistry::new();
        let fs_tool = MockSecureTool::new(
            "file_read",
            CapabilitySet::from(vec![Capability::FileSystemRead]),
        );
        let tool_name = fs_tool.name().to_string();
        registry.register(Box::new(fs_tool)).unwrap();

        // No capabilities granted
        let authorized = AuthorizedToolRegistry::harmless(&registry);

        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({})).unwrap();

        let result = authorized.execute(&tool_name, &ctx, input).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ToolError::Unauthorized { .. }));
    }

    #[tokio::test]
    async fn tool_with_multiple_caps_fails_if_any_missing() {
        let mut registry = ToolRegistry::new();
        let multi_tool = MockSecureTool::new(
            "dangerous",
            CapabilitySet::from(vec![
                Capability::FileSystemRead,
                Capability::FileSystemWrite,
            ]),
        );
        let tool_name = multi_tool.name().to_string();
        registry.register(Box::new(multi_tool)).unwrap();

        // Only grant read, not write
        let granted = CapabilitySet::from(vec![Capability::FileSystemRead]);
        let authorized = AuthorizedToolRegistry::new(&registry, granted);

        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({})).unwrap();

        let result = authorized.execute(&tool_name, &ctx, input).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, ToolError::Unauthorized { .. }));
        let msg = err.to_string();
        assert!(msg.contains("filesystem.write"));
    }

    #[tokio::test]
    async fn tool_with_multiple_caps_succeeds_when_all_granted() {
        let mut registry = ToolRegistry::new();
        let mock_tool = MockSecureTool::new(
            "full_access",
            CapabilitySet::from(vec![
                Capability::FileSystemRead,
                Capability::FileSystemWrite,
                Capability::FileSystemRead,
            ]),
        );
        let tool_name = mock_tool.name().to_string();
        registry.register(Box::new(mock_tool)).unwrap();

        // Grant all required
        let granted = CapabilitySet::from(vec![
            Capability::FileSystemRead,
            Capability::FileSystemWrite,
        ]);
        let authorized = AuthorizedToolRegistry::new(&registry, granted);

        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({})).unwrap();

        let result = authorized.execute(&tool_name, &ctx, input).await;
        assert!(result.is_ok());
    }

    #[test]
    fn can_execute_returns_true_when_authorized() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();

        let authorized = AuthorizedToolRegistry::harmless(&registry);
        assert!(authorized.can_execute("echo"));
    }

    #[test]
    fn can_execute_returns_false_when_unauthorized() {
        let mut registry = ToolRegistry::new();
        let fs_tool = MockSecureTool::new(
            "file_read",
            CapabilitySet::from(vec![Capability::FileSystemRead]),
        );
        registry.register(Box::new(fs_tool)).unwrap();

        let authorized = AuthorizedToolRegistry::harmless(&registry);
        assert!(!authorized.can_execute("file_read"));
    }

    #[tokio::test]
    async fn unauthorized_tool_never_executes() {
        let mut registry = ToolRegistry::new();
        let fs_tool = MockSecureTool::new(
            "file_delete",
            CapabilitySet::from(vec![Capability::FileSystemWrite]),
        );
        let tool_name = fs_tool.name().to_string();
        let execution_flag = fs_tool.should_execute.clone();
        registry.register(Box::new(fs_tool)).unwrap();

        // Try to execute without permission
        let authorized = AuthorizedToolRegistry::harmless(&registry);
        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({})).unwrap();

        let result = authorized.execute(&tool_name, &ctx, input).await;
        assert!(result.is_err());

        // Verify the tool was never executed
        assert!(!execution_flag.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn unauthorized_error_lists_missing_capabilities() {
        let mut registry = ToolRegistry::new();
        let multi_tool = MockSecureTool::new(
            "complex",
            CapabilitySet::from(vec![
                Capability::FileSystemRead,
                Capability::TerminalExecute,
            ]),
        );
        let tool_name = multi_tool.name().to_string();
        registry.register(Box::new(multi_tool)).unwrap();

        // Grant nothing
        let authorized = AuthorizedToolRegistry::harmless(&registry);

        let ctx = ToolContext::new();
        let input = ToolInput::from_json(json!({})).unwrap();

        let result = authorized.execute(&tool_name, &ctx, input).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not authorized"));
        assert!(msg.contains("filesystem.read"));
        assert!(msg.contains("terminal.execute"));
    }

    #[test]
    fn list_executable_filters_unauthorized_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();
        registry.register(Box::new(MockSecureTool::new(
            "file_read",
            CapabilitySet::from(vec![Capability::FileSystemRead]),
        ))).unwrap();
        registry.register(Box::new(MockSecureTool::new(
            "file_write",
            CapabilitySet::from(vec![Capability::FileSystemWrite]),
        ))).unwrap();

        // Only grant read
        let granted = CapabilitySet::from(vec![Capability::FileSystemRead]);
        let authorized = AuthorizedToolRegistry::new(&registry, granted);

        let executable: Vec<_> = authorized.list_executable();
        let names: Vec<_> = executable.iter().map(|m| m.name.as_str()).collect();

        // Echo requires nothing - executable
        assert!(names.contains(&"echo"));
        // file_read requires read - executable with our grant
        assert!(names.contains(&"file_read"));
        // file_write requires write - not executable
        assert!(!names.contains(&"file_write"));
    }
}
