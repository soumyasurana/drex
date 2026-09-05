//! Integration tests for filesystem and terminal tools with authorization
//!
//! These tests verify the complete authorization flow:
//! - Tool requiring filesystem.read executes when granted
//! - Tool requires terminal.execute executed when granted
//! - Tools rejected when capabilities not granted
//! - Authorization happens before filesystem/process access

use drex_tools::capability::{Capability, CapabilitySet};
use drex_tools::error::ToolError;
use drex_tools::registry::{AuthorizedToolRegistry, ToolRegistry};
use drex_tools::tool::{ToolContext, ToolInput};
use drex_tools::tools::{
    FileSystemConfig, FileSystemReadTool, TerminalConfig, TerminalExecuteTool,
};
use serde_json::json;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn filesystem_tool_executes_when_authorized() {
    // Create a temp directory for testing
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a test file
    let test_file = root.join("test.txt");
    tokio::fs::write(&test_file, "hello from auth test").await.unwrap();

    // Set up registry
    let mut registry = ToolRegistry::new();
    let fs_config = FileSystemConfig::new(root);
    registry.register(Box::new(FileSystemReadTool::new(fs_config)))
        .unwrap();

    // Create authorized registry with filesystem.read capability
    let granted = CapabilitySet::from(vec![Capability::FileSystemRead]);
    let authorized = AuthorizedToolRegistry::new(&registry, granted);

    // Execute - should succeed
    let ctx = ToolContext::new();
    let input = ToolInput::from_json(json!({"path": "test.txt"})).unwrap();

    let result = authorized.execute("filesystem.read", &ctx, input).await;
    assert!(result.is_ok(), "Expected success, got: {:?}", result);

    let output = result.unwrap();
    assert!(output.status.is_success());
    assert!(output.data().unwrap()["content"].as_str().unwrap().contains("hello from auth"));
}

#[tokio::test]
async fn filesystem_tool_rejected_without_authorization() {
    // Create a temp directory for testing
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a test file
    let test_file = root.join("secret.txt");
    tokio::fs::write(&test_file, "secret data").await.unwrap();

    // Set up registry
    let mut registry = ToolRegistry::new();
    let fs_config = FileSystemConfig::new(root);
    registry.register(Box::new(FileSystemReadTool::new(fs_config)))
        .unwrap();

    // Create authorized registry WITHOUT filesystem.read capability
    let authorized = AuthorizedToolRegistry::harmless(&registry);

    // Execute - should fail with Unauthorized
    let ctx = ToolContext::new();
    let input = ToolInput::from_json(json!({"path": "secret.txt"})).unwrap();

    let result = authorized.execute("filesystem.read", &ctx, input).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(err, ToolError::Unauthorized { .. }));
    assert!(err.to_string().contains("filesystem.read"));
    assert!(err.to_string().contains("missing capabilities"));
}

#[tokio::test]
async fn terminal_tool_executes_when_authorized() {
    // Set up registry
    let mut registry = ToolRegistry::new();
    let term_config = TerminalConfig::new().with_timeout(Duration::from_secs(5));
    registry.register(Box::new(TerminalExecuteTool::new(term_config)))
        .unwrap();

    // Create authorized registry with terminal.execute capability
    let granted = CapabilitySet::from(vec![Capability::TerminalExecute]);
    let authorized = AuthorizedToolRegistry::new(&registry, granted);

    // Execute - should succeed
    let ctx = ToolContext::new();
    let input = ToolInput::from_json(json!({"command": "echo", "args": ["auth works"]}))
        .unwrap();

    let result = authorized.execute("terminal.execute", &ctx, input).await;
    assert!(result.is_ok(), "Expected success, got: {:?}", result);

    let output = result.unwrap();
    assert!(output.status.is_success());
    assert!(!output.data().unwrap()["timed_out"].as_bool().unwrap());
    assert!(
        output.data().unwrap()["stdout"]
            .as_str()
            .unwrap()
            .contains("auth works")
    );
}

#[tokio::test]
async fn terminal_tool_rejected_without_authorization() {
    // Set up registry
    let mut registry = ToolRegistry::new();
    let term_config = TerminalConfig::new();
    registry.register(Box::new(TerminalExecuteTool::new(term_config)))
        .unwrap();

    // Create authorized registry WITHOUT terminal.execute capability
    let authorized = AuthorizedToolRegistry::harmless(&registry);

    // Execute - should fail before process spawn
    let ctx = ToolContext::new();
    let input = ToolInput::from_json(json!({"command": "echo", "args": ["never runs"]}))
        .unwrap();

    let result = authorized.execute("terminal.execute", &ctx, input).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(err, ToolError::Unauthorized { .. }));
    assert!(err.to_string().contains("terminal.execute"));
}

#[tokio::test]
async fn filesystem_tool_rejects_traversal_before_access() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Set up registry
    let mut registry = ToolRegistry::new();
    let fs_config = FileSystemConfig::new(root);
    registry.register(Box::new(FileSystemReadTool::new(fs_config)))
        .unwrap();

    // Grant the capability
    let granted = CapabilitySet::from(vec![Capability::FileSystemRead]);
    let authorized = AuthorizedToolRegistry::new(&registry, granted);

    // Try traversal attack
    let ctx = ToolContext::new();
    let input = ToolInput::from_json(json!({"path": "../etc/passwd"})).unwrap();

    let result = authorized.execute("filesystem.read", &ctx, input).await;
    assert!(result.is_err());

    // Should get ExecutionFailed with path traversal error
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("traversal") || msg.contains("outside allowed"),
        "Expected traversal error, got: {}",
        msg
    );
}

#[tokio::test]
async fn terminal_tool_enforces_timeout() {
    let mut registry = ToolRegistry::new();
    // Very short timeout for testing
    let term_config = TerminalConfig::new().with_timeout(Duration::from_millis(100));
    registry.register(Box::new(TerminalExecuteTool::new(term_config)))
        .unwrap();

    let granted = CapabilitySet::from(vec![Capability::TerminalExecute]);
    let authorized = AuthorizedToolRegistry::new(&registry, granted);

    let ctx = ToolContext::new();
    // Use sleep command (Unix-specific)
    #[cfg(unix)]
    {
        let input = ToolInput::from_json(json!({"command": "sleep", "args": ["10"]})).unwrap();

        let result = authorized.execute("terminal.execute", &ctx, input).await;
        assert!(result.is_ok(), "Expected Ok with timeout flag, got: {:?}", result);

        let output = result.unwrap();
        assert!(output.data().unwrap()["timed_out"].as_bool().unwrap());
    }
}

#[tokio::test]
async fn mixed_capability_registration() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create test file
    tokio::fs::write(root.join("data.txt"), "test data")
        .await
        .unwrap();

    // Set up registry with both tools
    let mut registry = ToolRegistry::new();

    let fs_config = FileSystemConfig::new(root);
    registry.register(Box::new(FileSystemReadTool::new(fs_config)))
        .unwrap();

    let term_config = TerminalConfig::new().with_timeout(Duration::from_secs(2));
    registry.register(Box::new(TerminalExecuteTool::new(term_config)))
        .unwrap();

    // Grant only filesystem.read
    let granted = CapabilitySet::from(vec![Capability::FileSystemRead]);
    let authorized = AuthorizedToolRegistry::new(&registry, granted);

    // Filesystem should work
    let fs_input = ToolInput::from_json(json!({"path": "data.txt"})).unwrap();
    let fs_result = authorized.execute("filesystem.read", &ToolContext::new(), fs_input).await;
    assert!(fs_result.is_ok());

    // Terminal should fail
    let term_input = ToolInput::from_json(json!({"command": "echo", "args": ["x"]})).unwrap();
    let term_result = authorized.execute("terminal.execute", &ToolContext::new(), term_input).await;
    assert!(term_result.is_err());
    assert!(matches!(
        term_result.unwrap_err(),
        ToolError::Unauthorized { .. }
    ));
}

#[test]
fn list_executable_shows_only_authorized() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Set up registry
    let mut registry = ToolRegistry::new();

    let fs_config = FileSystemConfig::new(root);
    registry
        .register(Box::new(FileSystemReadTool::new(fs_config)))
        .unwrap();

    let term_config = TerminalConfig::new();
    registry
        .register(Box::new(TerminalExecuteTool::new(term_config)))
        .unwrap();

    // Grant only filesystem.read
    let granted = CapabilitySet::from(vec![Capability::FileSystemRead]);
    let authorized = AuthorizedToolRegistry::new(&registry, granted);

    let executable = authorized.list_executable();
    let names: Vec<_> = executable.iter().map(|m| m.name.as_str()).collect();

    assert!(names.contains(&"filesystem.read"));
    assert!(!names.contains(&"terminal.execute"));
}
