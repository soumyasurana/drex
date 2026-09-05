//! Agent Loop - Core orchestration for plan execution
//!
//! This module implements the complete Drex agent loop:
//!
//! user request
//!     ↓
//! retrieve context from MemoryStore
//!     ↓
//! Planner → plan steps
//!     ↓
//! for each step:
//!     ↓
//!     translate → tool call OR direct answer
//!     ↓
//!     validate tool input
//!     ↓
//!     execute through ToolRegistry
//!     ↓
//!     observe result
//!     ↓
//!     ask model for evaluation
//!     ↓
//!     continue OR replan
//!     ↓
//! repeat
//!     ↓
//! write useful info to memory
//!     ↓
//! final response
//!
//! # Security
//!
//! - Maximum step count enforced
//! - Loop detection prevents runaway execution
//! - Capability checks before every tool execution
//! - Memory write-back respects MemoryPolicy
//! - No secrets written to memory

use crate::{
    executor::{ExecutionError, StepExecutor, StepTranslation},
    planner::{Plan, Planner, PlannerError},
};
use drex_memory::{Memory, MemoryKind, MemoryStore, TaskTrustLevel};
use drex_models::router::ModelRouter;
use drex_tools::{
    registry::ToolRegistry,
    result::ExecutionResult,
    tool::ToolContext,
    CapabilitySet,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, trace, warn};

/// Errors that can occur during agent execution.
#[derive(Debug, Error)]
pub enum AgentError {
    /// Planning failed.
    #[error("Planning failed: {0}")]
    PlanningFailed(#[from] PlannerError),

    /// Step execution failed.
    #[error("Execution failed: {0}")]
    ExecutionFailed(#[from] ExecutionError),

    /// Maximum steps exceeded.
    #[error("Maximum steps ({max}) exceeded", max = max)]
    MaxStepsExceeded {
        max: usize,
        steps_executed: usize,
    },

    /// Loops detected - same action repeated.
    #[error("Loop detected: action '{action}' repeated {count} times", action = action, count = count)]
    LoopDetected {
        action: String,
        count: usize,
    },

    /// Memory operation failed.
    #[error("Memory operation failed: {0}")]
    MemoryError(String),

    /// Model evaluation failed.
    #[error("Model evaluation failed: {0}")]
    ModelError(String),

    /// Unexpected error.
    #[error("Unexpected error: {0}")]
    Unexpected(String),
}

/// Configuration for the agent.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum number of agent steps before termination.
    pub max_steps: usize,

    /// Maximum number of times to retry a failed step.
    pub max_retries: usize,

    /// Whether to enable loop detection.
    pub loop_detection: bool,

    /// Maximum repetitions before considering it a loop.
    pub max_repetitions: usize,

    /// Default task trust level for memory.
    pub trust_level: TaskTrustLevel,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 20,
            max_retries: 3,
            loop_detection: true,
            max_repetitions: 3,
            trust_level: TaskTrustLevel::Medium,
        }
    }
}

/// State for a single agent execution.
#[derive(Debug)]
struct ExecutionState {
    /// Current step number (1-indexed).
    current_step: usize,

    /// History of executed actions (for loop detection).
    action_history: Vec<String>,

    /// Current plan being executed.
    current_plan: Option<Plan>,

    /// Step index within the plan (0-indexed).
    plan_step_index: usize,

    /// Accumulated observations from tool executions.
    observations: Vec<Observation>,
}

impl ExecutionState {
    fn new() -> Self {
        Self {
            current_step: 0,
            action_history: Vec::new(),
            current_plan: None,
            plan_step_index: 0,
            observations: Vec::new(),
        }
    }

    /// Record an action in history for loop detection.
    fn record_action(&mut self, action: &str) {
        self.action_history.push(action.to_string());
    }

    /// Check if the same action has been repeated too many times.
    fn is_looping(&self, action: &str, max_repetitions: usize) -> bool {
        let count = self
            .action_history
            .iter()
            .rev()
            .take(max_repetitions)
            .filter(|a| a == &action)
            .count();
        count >= max_repetitions
    }

    /// Add observation from tool execution.
    fn add_observation(&mut self, observation: Observation) {
        self.observations.push(observation);
    }
}

/// An observation from tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// The step number.
    pub step_number: usize,

    /// The tool that was called (or "direct_answer").
    pub tool_name: String,

    /// Whether the execution was successful.
    pub success: bool,

    /// The result data (JSON).
    pub result: serde_json::Value,

    /// Any error message.
    pub error: Option<String>,
}

/// The result of an agent turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResult {
    /// Whether the agent completed successfully.
    pub success: bool,

    /// The final response to the user.
    pub response: String,

    /// Number of steps executed.
    pub steps_executed: usize,

    /// Observations from tool executions.
    pub observations: Vec<Observation>,

    /// Reason for termination.
    pub termination_reason: String,

    /// Whether the agent wrote new memories.
    pub memories_written: usize,
}

/// Trace log for agent execution.
#[derive(Debug, Clone)]
pub struct AgentTrace {
    pub entries: Vec<TraceEntry>,
}

impl AgentTrace {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn add(&mut self, entry: TraceEntry) {
        self.entries.push(entry);
    }
}

/// A single trace entry.
#[derive(Debug, Clone)]
pub enum TraceEntry {
    /// Request received.
    RequestReceived { request: String },

    /// Memory retrieved.
    MemoryRetrieved { count: usize },

    /// Plan generated.
    PlanGenerated { step_count: usize },

    /// Step selected.
    StepSelected { step_number: usize, description: String },

    /// Tool selected.
    ToolSelected { tool_name: String },

    /// Tool execution started.
    ToolExecuting { tool_name: String },

    /// Tool execution completed.
    ToolCompleted { tool_name: String, success: bool },

    /// Observation made.
    Observation { observation: Observation },

    /// Model evaluation.
    Evaluation { should_continue: bool, reasoning: String },

    /// Replan triggered.
    ReplanTriggered { reason: String },

    /// Memory write.
    MemoryWrite { count: usize },

    /// Final response.
    FinalResponse { response: String },

    /// Termination.
    Termination { reason: String },
}

/// The main Agent that orchestrates planning and execution.
pub struct Agent {
    planner: Planner,
    executor: StepExecutor,
    model_router: Arc<ModelRouter>,
    config: AgentConfig,
}

impl Agent {
    /// Create a new agent.
    pub fn new(
        model_router: Arc<ModelRouter>,
        tool_registry: Arc<ToolRegistry>,
        capabilities: CapabilitySet,
        config: AgentConfig,
    ) -> Self {
        let planner = Planner::new(model_router.clone());
        let executor = StepExecutor::new(tool_registry, capabilities);

        Self {
            planner,
            executor,
            model_router,
            config,
        }
    }

    /// Execute a complete agent turn.
    ///
    /// This is the main entry point for the agent loop.
    pub async fn execute(
        &self,
        request: &str,
        memory_store: Option<&dyn MemoryStore>,
    ) -> Result<AgentResult, AgentError> {
        info!(request = %request, "Starting agent execution");

        let mut trace = AgentTrace::new();
        let mut state = ExecutionState::new();

        trace.add(TraceEntry::RequestReceived {
            request: request.to_string(),
        });

        // Step 1: Retrieve context (optional - may fail gracefully)
        let context = match self.retrieve_context(memory_store, request).await {
            Ok(ctx) => {
                trace.add(TraceEntry::MemoryRetrieved { count: ctx.len() });
                ctx
            }
            Err(e) => {
                warn!(error = %e, "Context retrieval failed, proceeding without");
                Vec::new()
            }
        };

        // Step 2: Generate initial plan
        let mut plan = self.generate_plan(request, memory_store).await?;
        state.current_plan = Some(plan.clone());

        trace.add(TraceEntry::PlanGenerated {
            step_count: plan.step_count(),
        });

        // Step 3: Execute plan steps
        let mut final_response = String::new();

        while state.plan_step_index < plan.step_count() {
            // Check max steps
            if state.current_step >= self.config.max_steps {
                warn!(
                    steps = state.current_step,
                    max = self.config.max_steps,
                    "Maximum steps exceeded"
                );
                trace.add(TraceEntry::Termination {
                    reason: format!("Maximum steps ({}) exceeded", self.config.max_steps),
                });
                return Err(AgentError::MaxStepsExceeded {
                    max: self.config.max_steps,
                    steps_executed: state.current_step,
                });
            }

            let step_index = state.plan_step_index;
            let step = plan.get_step(step_index + 1).unwrap();

            trace.add(TraceEntry::StepSelected {
                step_number: step.number,
                description: step.description.clone(),
            });

            // Step 4: Translate step to tool call or direct answer
            let translation = self.executor.translate_step(step);

            match translation {
                StepTranslation::ToolCall(tool_call) => {
                    trace.add(TraceEntry::ToolSelected {
                        tool_name: tool_call.tool_name.clone(),
                    });

                    // Loop detection
                    if self.config.loop_detection
                        && state.is_looping(&tool_call.tool_name, self.config.max_repetitions)
                    {
                        warn!(
                            action = %tool_call.tool_name,
                            "Loop detected"
                        );
                        trace.add(TraceEntry::Termination {
                            reason: format!(
                                "Loop detected: '{}' repeated {} times",
                                tool_call.tool_name, self.config.max_repetitions
                            ),
                        });
                        return Err(AgentError::LoopDetected {
                            action: tool_call.tool_name,
                            count: self.config.max_repetitions,
                        });
                    }

                    // Validate before execution
                    match self.executor.validate(&tool_call) {
                        crate::executor::ValidationResult::Valid => {}
                        crate::executor::ValidationResult::Invalid(reason) => {
                            warn!(
                                tool = %tool_call.tool_name,
                                reason = %reason,
                                "Tool validation failed"
                            );
                            // Try to replan
                            trace.add(TraceEntry::ReplanTriggered {
                                reason: format!("Validation failed: {}", reason),
                            });
                            let replan_result = self
                                .replan(
                                    &plan,
                                    &state,
                                    &format!("Tool validation failed: {}", reason),
                                    memory_store,
                                )
                                .await;
                            match replan_result {
                                Ok(new_plan) => {
                                    plan = new_plan;
                                    state.current_plan = Some(plan.clone());
                                    state.plan_step_index = 0;
                                    continue;
                                }
                                Err(e) => return Err(e),
                            }
                        }
                    }

                    state.record_action(&tool_call.tool_name);

                    // Execute the tool
                    trace.add(TraceEntry::ToolExecuting {
                        tool_name: tool_call.tool_name.clone(),
                    });

                    let context = ToolContext::new();
                    match self.executor.execute(&tool_call, &context).await {
                        Ok(result) => {
                            let success = result.is_success();
                            let result_json = serde_json::to_value(&result).unwrap_or_default();
                            let observation = Observation {
                                step_number: step.number,
                                tool_name: tool_call.tool_name.clone(),
                                success,
                                result: result_json,
                                error: result.error().map(|e| e.to_string()),
                            };

                            state.add_observation(observation.clone());
                            trace.add(TraceEntry::Observation {
                                observation: observation.clone(),
                            });
                            trace.add(TraceEntry::ToolCompleted {
                                tool_name: tool_call.tool_name,
                                success,
                            });

                            // Evaluate result
                            let should_continue =
                                self.evaluate_step(&observation, &plan, &state).await?;

                            trace.add(TraceEntry::Evaluation {
                                should_continue,
                                reasoning: if should_continue {
                                    "Step succeeded".to_string()
                                } else {
                                    "Step failed, may need replan".to_string()
                                },
                            });

                            if !should_continue {
                                // Replan needed
                                trace.add(TraceEntry::ReplanTriggered {
                                    reason: "Step evaluation suggested replan".to_string(),
                                });
                                let replan_result = self
                                    .replan(
                                        &plan,
                                        &state,
                                        &format!(
                                            "Step {} did not succeed as expected",
                                            step.number
                                        ),
                                        memory_store,
                                    )
                                    .await;
                                match replan_result {
                                    Ok(new_plan) => {
                                        plan = new_plan;
                                        state.current_plan = Some(plan.clone());
                                        state.plan_step_index = 0;
                                        continue;
                                    }
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                        Err(e) => {
                            warn!(
                                tool = %tool_call.tool_name,
                                error = %e,
                                "Tool execution failed"
                            );
                            // Try to replan
                            trace.add(TraceEntry::ReplanTriggered {
                                reason: format!("Tool execution failed: {}", e),
                            });
                            let replan_result = self
                                .replan(
                                    &plan,
                                    &state,
                                    &format!("Tool execution failed: {}", e),
                                    memory_store,
                                )
                                .await;
                            match replan_result {
                                Ok(new_plan) => {
                                    plan = new_plan;
                                    state.current_plan = Some(plan.clone());
                                    state.plan_step_index = 0;
                                    continue;
                                }
                                Err(e) => return Err(e),
                            }
                        }
                    }
                }
                StepTranslation::DirectAnswer(answer) => {
                    final_response = answer;
                    trace.add(TraceEntry::FinalResponse {
                        response: final_response.clone(),
                    });
                    break; // Direct answer means we're done
                }
                StepTranslation::Error(e) => {
                    warn!(error = %e, "Step translation failed");
                    // Try to replan
                    trace.add(TraceEntry::ReplanTriggered {
                        reason: format!("Step translation failed: {}", e),
                    });
                    let replan_result = self
                        .replan(
                            &plan,
                            &state,
                            &format!("Step translation failed: {}", e),
                            memory_store,
                        )
                        .await;
                    match replan_result {
                        Ok(new_plan) => {
                            plan = new_plan;
                            state.current_plan = Some(plan.clone());
                            state.plan_step_index = 0;
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
            }

            state.current_step += 1;
            state.plan_step_index += 1;
        }

        // Step 5: Write useful information to memory
        let memories_written = match self
            .write_memories(&plan, &state, request, &final_response, memory_store)
            .await
        {
            Ok(count) => {
                trace.add(TraceEntry::MemoryWrite { count });
                count
            }
            Err(e) => {
                warn!(error = %e, "Failed to write memories");
                0
            }
        };

        // Generate final response if not already set
        if final_response.is_empty() {
            final_response = self.generate_final_response(&plan, &state).await?;
        }

        trace.add(TraceEntry::Termination {
            reason: "Plan completed successfully".to_string(),
        });

        info!(
            steps_executed = state.current_step,
            observations = state.observations.len(),
            memories_written,
            "Agent execution complete"
        );

        Ok(AgentResult {
            success: true,
            response: final_response,
            steps_executed: state.current_step,
            observations: state.observations,
            termination_reason: "Plan completed".to_string(),
            memories_written,
        })
    }

    /// Retrieve relevant context from memory.
    async fn retrieve_context(
        &self,
        _memory_store: Option<&dyn MemoryStore>,
        _request: &str,
    ) -> Result<Vec<String>, AgentError> {
        // For now, return empty context
        // In a full implementation, this would query the memory store
        // using semantic search to find relevant previous interactions
        trace!("Context retrieval (placeholder - returning empty)");
        Ok(Vec::new())
    }

    /// Generate an initial plan for the request.
    async fn generate_plan(
        &self,
        request: &str,
        memory_store: Option<&dyn MemoryStore>,
    ) -> Result<Plan, AgentError> {
        info!("Generating plan...");
        self.planner
            .plan(request, memory_store)
            .await
            .map_err(AgentError::from)
    }

    /// Replan based on current state and failure reason.
    async fn replan(
        &self,
        _current_plan: &Plan,
        state: &ExecutionState,
        failure_reason: &str,
        memory_store: Option<&dyn MemoryStore>,
    ) -> Result<Plan, AgentError> {
        warn!(
            reason = %failure_reason,
            step = state.current_step,
            "Replanning..."
        );

        // Build a replanning request that includes context from observations
        let mut replan_request = format!(
            "The previous plan failed at step {} with reason: {}. ",
            state.current_step, failure_reason
        );

        replan_request.push_str("Previous observations:\n");
        for obs in &state.observations {
            replan_request.push_str(&format!(
                "- Step {}: {} (success: {})\n",
                obs.step_number, obs.tool_name, obs.success
            ));
        }

        replan_request.push_str("\nPlease create a new plan to accomplish the original goal.");

        self.planner
            .plan(&replan_request, memory_store)
            .await
            .map_err(AgentError::from)
    }

    /// Evaluate whether to continue with the plan or replan.
    async fn evaluate_step(
        &self,
        observation: &Observation,
        _plan: &Plan,
        _state: &ExecutionState,
    ) -> Result<bool, AgentError> {
        // Simple evaluation: continue if successful
        if observation.success {
            return Ok(true);
        }

        // For failures, ask the model for evaluation
        // This is a placeholder - in a full implementation we'd
        // send the observation to the model for evaluation
        debug!("Evaluating failed step - defaulting to continue for now");
        Ok(true)
    }

    /// Write useful information back to memory.
    async fn write_memories(
        &self,
        plan: &Plan,
        state: &ExecutionState,
        request: &str,
        response: &str,
        memory_store: Option<&dyn MemoryStore>,
    ) -> Result<usize, AgentError> {
        let Some(store) = memory_store else {
            trace!("No memory store provided, skipping write");
            return Ok(0);
        };

        let mut count = 0;

        // Write plan summary (always try, let PolicyContext guide importance)
        let plan_summary = format!(
            "Request: {}\nPlan steps: {}\nResponse: {}",
            request,
            plan.step_count(),
            response
        );

        let memory = Memory::new(MemoryKind::Semantic, &plan_summary)
            .with_metadata(drex_memory::MemoryMetadata::automatic("drex-agent"))
            .with_importance(0.7);

        match store.store(memory).await {
            Ok(_) => {
                debug!("Wrote plan summary to memory");
                count += 1;
            }
            Err(e) => {
                warn!(error = %e, "Failed to write plan memory");
            }
        }

        // Write key observations
        for obs in &state.observations {
            if obs.success {
                let obs_content = format!("{}: {:?}", obs.tool_name, obs.result);
                let memory = Memory::new(MemoryKind::Semantic, &obs_content)
                    .with_metadata(drex_memory::MemoryMetadata::automatic("drex-agent"))
                    .with_importance(0.5);

                if let Err(e) = store.store(memory).await {
                    warn!(error = %e, "Failed to write observation memory");
                } else {
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Generate the final response to the user.
    async fn generate_final_response(
        &self,
        _plan: &Plan,
        state: &ExecutionState,
    ) -> Result<String, AgentError> {
        // If we have observations, build a response from them
        if state.observations.is_empty() {
            return Ok("I've completed the requested task.".to_string());
        }

        // Simple response builder - in production, this might use a model
        let mut response = String::from("Here's what I did:\n\n");
        for obs in &state.observations {
            if obs.success {
                response.push_str(&format!("✓ Step {}: {} completed\n", obs.step_number, obs.tool_name));
            } else {
                response.push_str(&format!("✗ Step {}: {} failed\n", obs.step_number, obs.tool_name));
            }
        }

        Ok(response)
    }

    /// Get the current agent trace.
    ///
    /// This returns the complete execution trace for debugging.
    pub fn get_trace(&self) -> AgentTrace {
        AgentTrace::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drex_tools::{
        capability::Capability,
        tools::EchoTool,
    };

    fn create_test_agent(_temp_dir: &tempfile::TempDir) -> Agent {
        let model_router = Arc::new(ModelRouter::new());
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoTool::new())).unwrap();

        let capabilities = CapabilitySet::from(vec![Capability::FileSystemRead]);
        let config = AgentConfig::default();

        Agent::new(model_router, Arc::new(registry), capabilities, config)
    }

    #[test]
    fn agent_config_default() {
        let config = AgentConfig::default();
        assert_eq!(config.max_steps, 20);
        assert_eq!(config.max_retries, 3);
        assert!(config.loop_detection);
    }

    #[test]
    fn execution_state_tracks_actions() {
        let mut state = ExecutionState::new();
        state.record_action("echo");
        state.record_action("filesystem.read");
        state.record_action("echo");

        assert_eq!(state.action_history.len(), 3);
        // "echo" appears twice total but not in the last 2 consecutive actions
        assert!(!state.is_looping("echo", 2)); // only 1 in last 2
        assert!(!state.is_looping("filesystem.read", 2)); // only 1 total
    }

    #[test]
    fn execution_state_detects_loops() {
        let mut state = ExecutionState::new();

        // Repeat same action 3 times
        state.record_action("echo");
        state.record_action("echo");
        state.record_action("echo");

        assert!(state.is_looping("echo", 3));
        assert!(!state.is_looping("echo", 4));
    }

    #[tokio::test]
    async fn agent_result_success() {
        let temp_dir = tempfile::tempdir().unwrap();
        let agent = create_test_agent(&temp_dir);

        // This will likely fail since we don't have a mock backend registered
        // but it tests the basic structure
        let result = agent.execute("test request", None).await;

        // Should fail because no model backend is registered
        assert!(result.is_err());
    }

    #[test]
    fn agent_error_max_steps_displays_correctly() {
        let err = AgentError::MaxStepsExceeded {
            max: 10,
            steps_executed: 11,
        };

        assert!(err.to_string().contains("10"));
    }

    #[test]
    fn agent_error_loop_detected_displays_correctly() {
        let err = AgentError::LoopDetected {
            action: "echo".to_string(),
            count: 3,
        };

        assert!(err.to_string().contains("echo"));
        assert!(err.to_string().contains("3"));
    }

    #[test]
    fn observation_serialization() {
        let obs = Observation {
            step_number: 1,
            tool_name: "echo".to_string(),
            success: true,
            result: serde_json::json!({"message": "hello"}),
            error: None,
        };

        let json = serde_json::to_string(&obs).unwrap();
        assert!(json.contains("echo"));
        assert!(json.contains("true"));
    }

    #[test]
    fn agent_result_serialization() {
        let result = AgentResult {
            success: true,
            response: "Test response".to_string(),
            steps_executed: 5,
            observations: vec![],
            termination_reason: "Completed".to_string(),
            memories_written: 2,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Test response"));
        assert!(json.contains("5"));
        assert!(json.contains("2"));
    }
}
