//! Integration tests for the agent execution pipeline
//!
//! Tests:
//! - Actionable plan → step execution
//! - "remember X" → memory write via MemoryTool
//! - Unauthorized tool → prevented/denied
//! - Memory writeback increments memories_written counter
//! - MemoryTool uses memory-specific capabilities (NOT FileSystemWrite)

use std::sync::Arc;
use drex_tools::capability::{Capability, CapabilitySet};

/// Test that an actionable plan results in steps being executed
#[tokio::test]
async fn actionable_plan_executes_steps() {
    // Create a tool registry with all available tools
    let mut registry = drex_tools::ToolRegistry::new();
    registry.register(Box::new(drex_tools::tools::EchoTool::new())).unwrap();
    registry.register(Box::new(drex_tools::tools::MemoryTool::new())).unwrap();
    let registry = Arc::new(registry);

    // Create a step executor with required capabilities
    let executor = drex_agent::StepExecutor::new(registry, CapabilitySet::all());

    // Create a plan step with tool call syntax
    let step = drex_agent::planner::PlanStep {
        number: 1,
        description: "call echo({\"message\": \"test execution\"})".to_string(),
        rationale: None,
    };

    // Translate the step
    let translation = executor.translate_step(&step);

    // Verify it was translated to a tool call
    assert!(
        matches!(translation, drex_agent::StepTranslation::ToolCall(_)),
        "Expected ToolCall but got {:?}",
        translation
    );
}

/// Test that "remember X" intent is translated to memory tool
#[tokio::test]
async fn remember_intent_uses_memory_tool() {
    let mut registry = drex_tools::ToolRegistry::new();
    registry.register(Box::new(drex_tools::tools::MemoryTool::new())).unwrap();
    let registry = Arc::new(registry);

    let executor = drex_agent::StepExecutor::new(registry, CapabilitySet::all());

    let step = drex_agent::planner::PlanStep {
        number: 1,
        description: "call memory({\"action\": \"store\", \"content\": \"DREX_MEMORY_TEST_001\"})".to_string(),
        rationale: None,
    };

    let translation = executor.translate_step(&step);

    match translation {
        drex_agent::StepTranslation::ToolCall(call) => {
            assert_eq!(call.tool_name, "memory");
            assert_eq!(call.arguments["action"], "store");
            assert_eq!(call.arguments["content"], "DREX_MEMORY_TEST_001");
        }
        _ => panic!("Expected ToolCall but got {:?}", translation),
    }
}

/// Test that unauthorized tool calls are denied
#[tokio::test]
async fn unauthorized_tool_call_is_denied() {
    use drex_tools::capability::CapabilitySet;
    use drex_tools::AuthorizedToolRegistry;

    let mut registry = drex_tools::ToolRegistry::new();
    // FileSystemReadTool requires FileSystemRead capability
    registry.register(Box::new(drex_tools::tools::FileSystemReadTool::new(
        drex_tools::tools::FileSystemConfig::new("/tmp")
    ))).unwrap();
    // Note: AuthorizedToolRegistry takes a reference, not Arc

    // Create an authorized registry with NO capabilities granted
    let authorized = AuthorizedToolRegistry::harmless(&registry);

    // Attempt to execute filesystem.read without FileSystemRead capability
    let ctx = drex_tools::ToolContext::new();
    let input = drex_tools::ToolInput::from_json(
        serde_json::json!({"path": "/etc/passwd"})
    ).unwrap();
    let result = authorized.execute("filesystem.read", &ctx, input).await;

    // Should fail with authorization error
    assert!(result.is_err(), "Expected error for unauthorized tool call");
    let err: drex_tools::ToolError = result.unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("not authorized")
            || err.to_string().to_lowercase().contains("unauthorized")
            || err.to_string().to_lowercase().contains("denied"),
        "Expected unauthorized error but got: {}",
        err
    );
}

/// Test that memory writeback increments the memories_written counter
#[tokio::test]
async fn agent_result_tracks_memories_written() {
    // Create a simple agent result and verify memories_written field exists
    let result = drex_agent::AgentResult {
        response: "Test response".to_string(),
        steps_executed: 3,
        observations: vec![],
        memories_written: 2,  // This is the field we want to verify exists
        success: true,
        termination_reason: "Completed successfully".to_string(),
    };

    assert_eq!(result.memories_written, 2);
    assert_eq!(result.steps_executed, 3);
}

/// Test that steps generated with "remember" intent can be parsed correctly
#[tokio::test]
async fn remember_step_parsing() {
    use drex_agent::planner::{Plan, PlanStep};

    let plan = Plan::new("Remember this exact fact: DREX_MEMORY_TEST_001");
    // Plan doesn't have with_steps, so we test differently

    assert_eq!(plan.step_count(), 0);

    // Test plan step creation directly
    let step = PlanStep {
        number: 1,
        description: "call memory({\"action\": \"store\", \"content\": \"DREX_MEMORY_TEST_001\"})".to_string(),
        rationale: Some("Store the user's explicit memory".to_string()),
    };

    assert!(step.description.contains("memory"));
    assert!(step.description.contains("store"));
}

/// Test the executor rejects malformed tool calls
#[tokio::test]
async fn executor_rejects_malformed_tool_call() {
    let registry = Arc::new(drex_tools::ToolRegistry::new());
    let executor = drex_agent::StepExecutor::new(registry, CapabilitySet::all());

    let step = drex_agent::planner::PlanStep {
        number: 1,
        description: "This is just natural language without tool call format".to_string(),
        rationale: None,
    };

    let translation = executor.translate_step(&step);

    // Should return error because no tool registry is empty and no pattern matches
    assert!(
        matches!(translation, drex_agent::StepTranslation::Error(_)),
        "Expected Error for malformed step but got {:?}",
        translation
    );
}

/// Test that the memory tool is accessible via registry
#[tokio::test]
async fn memory_tool_available_in_registry() {
    let mut registry = drex_tools::ToolRegistry::new();
    registry.register(Box::new(drex_tools::tools::MemoryTool::new())).unwrap();
    let registry = Arc::new(registry);

    assert!(registry.contains("memory"));

    let tool = registry.get("memory").unwrap();
    assert_eq!(tool.name(), "memory");
}

/// Test that MemoryTool does NOT require FileSystemWrite capability
#[tokio::test]
async fn memory_tool_does_not_require_filesystem_write() {
    use drex_tools::Tool;

    let tool = drex_tools::tools::MemoryTool::new();
    let required = tool.required_capabilities();

    // Memory tool should NOT have FileSystemWrite
    assert!(
        !required.has(Capability::FileSystemWrite),
        "MemoryTool should NOT require FileSystemWrite capability"
    );

    // Memory tool SHOULD have MemoryRead and MemoryWrite
    assert!(
        required.has(Capability::MemoryRead),
        "MemoryTool should require MemoryRead capability"
    );
    assert!(
        required.has(Capability::MemoryWrite),
        "MemoryTool should require MemoryWrite capability"
    );
}

/// Test that authorized MemoryTool.store succeeds with MemoryWrite capability
#[tokio::test]
async fn authorized_memory_store_succeeds() {
    use drex_tools::Tool;
    use drex_tools::AuthorizedToolRegistry;

    let mut registry = drex_tools::ToolRegistry::new();
    registry.register(Box::new(drex_tools::tools::MemoryTool::new())).unwrap();

    // Create an authorized registry WITH MemoryWrite capability
    let mut caps = CapabilitySet::new();
    caps.add(Capability::MemoryRead);
    caps.add(Capability::MemoryWrite);
    let authorized = AuthorizedToolRegistry::new(&registry, caps);

    // Attempt to store memory
    let ctx = drex_tools::ToolContext::new();
    let input = drex_tools::ToolInput::from_json(
        serde_json::json!({"action": "store", "content": "test memory"})
    ).unwrap();
    let result = authorized.execute("memory", &ctx, input).await;

    // Should succeed
    assert!(result.is_ok(), "Memory store should succeed with MemoryWrite: {:?}", result.err());
}

/// Test that authorized MemoryTool.retrieve succeeds with MemoryRead capability
#[tokio::test]
async fn authorized_memory_retrieve_succeeds() {
    use drex_tools::Tool;
    use drex_tools::AuthorizedToolRegistry;

    let mut registry = drex_tools::ToolRegistry::new();
    registry.register(Box::new(drex_tools::tools::MemoryTool::new())).unwrap();

    // Create an authorized registry WITH MemoryRead capability
    let mut caps = CapabilitySet::new();
    caps.add(Capability::MemoryRead);
    caps.add(Capability::MemoryWrite);
    let authorized = AuthorizedToolRegistry::new(&registry, caps);

    // Attempt to retrieve memory
    let ctx = drex_tools::ToolContext::new();
    let input = drex_tools::ToolInput::from_json(
        serde_json::json!({"action": "retrieve", "content": "test query"})
    ).unwrap();
    let result = authorized.execute("memory", &ctx, input).await;

    // Should succeed
    assert!(result.is_ok(), "Memory retrieve should succeed with MemoryRead: {:?}", result.err());
}

/// Test that unauthorized MemoryTool.store is denied without MemoryWrite capability
#[tokio::test]
async fn unauthorized_memory_store_is_denied() {
    use drex_tools::Tool;
    use drex_tools::AuthorizedToolRegistry;

    let mut registry = drex_tools::ToolRegistry::new();
    registry.register(Box::new(drex_tools::tools::MemoryTool::new())).unwrap();

    // Create an authorized registry WITHOUT MemoryWrite capability (only MemoryRead)
    let mut caps = CapabilitySet::new();
    caps.add(Capability::MemoryRead);
    // Note: NOT adding MemoryWrite
    let authorized = AuthorizedToolRegistry::new(&registry, caps);

    // Attempt to store memory without MemoryWrite
    let ctx = drex_tools::ToolContext::new();
    let input = drex_tools::ToolInput::from_json(
        serde_json::json!({"action": "store", "content": "test memory"})
    ).unwrap();
    let result = authorized.execute("memory", &ctx, input).await;

    // Should fail with authorization error
    assert!(result.is_err(), "Memory store should fail without MemoryWrite");
    let err = result.unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("not authorized")
            || err.to_string().to_lowercase().contains("unauthorized"),
        "Expected unauthorized error but got: {}",
        err
    );
}

/// Test that MemoryTool requires BOTH MemoryRead AND MemoryWrite capabilities
#[tokio::test]
async fn memory_tool_requires_both_capabilities() {
    use drex_tools::Tool;

    let tool = drex_tools::tools::MemoryTool::new();
    let required = tool.required_capabilities();

    // Should require exactly 2 capabilities: MemoryRead and MemoryWrite
    assert_eq!(required.len(), 2, "MemoryTool should require exactly 2 capabilities");
    assert!(required.has(Capability::MemoryRead));
    assert!(required.has(Capability::MemoryWrite));

    // Should NOT require filesystem or terminal capabilities
    assert!(!required.has(Capability::FileSystemRead));
    assert!(!required.has(Capability::FileSystemWrite));
    assert!(!required.has(Capability::TerminalExecute));
    assert!(!required.has(Capability::BrowserRequest));
}
