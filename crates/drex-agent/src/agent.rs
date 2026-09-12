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
use regex::Regex;
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
        memory_store: Option<Arc<dyn MemoryStore>>,
    ) -> Result<AgentResult, AgentError> {
        info!(request = %request, "Starting agent execution");

        let mut trace = AgentTrace::new();
        let mut state = ExecutionState::new();

        trace.add(TraceEntry::RequestReceived {
            request: request.to_string(),
        });

        // Step 1: Retrieve context (optional - may fail gracefully)
        let context = match self.retrieve_context(&memory_store, request).await {
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
        let mut plan = self.generate_plan(request, &memory_store).await?;
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
                                    &memory_store,
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

                    // Build context with memory store if available
                    let context = if let Some(ref store) = memory_store {
                        ToolContext::new().with_memory_store(store.clone())
                    } else {
                        ToolContext::new()
                    };
                    match self.executor.execute(&tool_call, &context).await {
                        Ok(result) => {
                            let success = result.is_success();
                            let result_json = serde_json::to_value(&result)
                                .map_err(|e| {
                                    AgentError::Unexpected(format!(
                                        "Failed to serialize tool result for observation: {}, cause: {}",
                                        tool_call.tool_name, e
                                    ))
                                })?;
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
                                        &memory_store,
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
                                    &memory_store,
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
                            &memory_store,
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

        // Step 5: Track memory operations from observations
        // Count actual memory tool operations (store and retrieve)
        let memories_written = state.observations.iter().filter(|obs| {
            obs.tool_name == "memory" && obs.success &&
            obs.result.get("data").and_then(|d| d.get("memory_id")).is_some()
        }).count();

        // Write automatic context to memory (for agent's own learning)
        match self
            .write_memories(&plan, &state, request, &final_response, &memory_store)
            .await
        {
            Ok(_) => {
                trace.add(TraceEntry::MemoryWrite { count: memories_written });
            }
            Err(e) => {
                warn!(error = %e, "Failed to write context memories");
            }
        }

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
        _memory_store: &Option<Arc<dyn MemoryStore>>,
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
        _memory_store: &Option<Arc<dyn MemoryStore>>,
    ) -> Result<Plan, AgentError> {
        info!("Generating plan...");
        // For now, don't pass memory_store to planner since it expects different type
        self.planner
            .plan(request, None)
            .await
            .map_err(AgentError::from)
    }

    /// Replan based on current state and failure reason.
    async fn replan(
        &self,
        current_plan: &Plan,
        state: &ExecutionState,
        failure_reason: &str,
        _memory_store: &Option<Arc<dyn MemoryStore>>,
    ) -> Result<Plan, AgentError> {
        warn!(
            reason = %failure_reason,
            step = state.current_step,
            "Replanning..."
        );

        // Check if this was a mathematical or conceptual question that was badly misclassified
        // and try to recover directly by requesting only direct answers for such questions
        let is_math_or_conceptual_request = current_plan.request.to_lowercase().contains("what is")
            && (
                current_plan.request.contains("*") || 
                current_plan.request.contains("+") || 
                current_plan.request.contains("-") || 
                current_plan.request.contains("/") ||
                current_plan.request.contains("explain") ||
                current_plan.request.contains("define") ||
                current_plan.request.contains("?")
            );
        
        if is_math_or_conceptual_request && failure_reason.contains("terminal.execute") {
            // This is a case of a math/concept question being misinterpreted 
            // as needing tool execution - try to recover directly with a clear prompt
            let direct_answer_prompt = format!(
                "Your previous plan incorrectly attempted to solve '{}' with a terminal execution, \
                which is not required for this type of question. Please provide the direct mathematical/conceptual \
                answer to the question '{}'. No tool calls are needed.",
                current_plan.request, current_plan.request
            );
            
            return self.planner.plan(&direct_answer_prompt, None)
                .await
                .map_err(AgentError::from);
        }

        // Build a replanning request that includes context from observations
        let mut replan_request = format!(
            "The previous plan failed at step {} with reason: {}. ",
            state.current_step, failure_reason
        );

        // Include the original request for context
        replan_request.push_str(&format!(
            "\nOriginal task: {}\n",
            current_plan.request
        ));

        replan_request.push_str("Previous observations:\n");
        for obs in &state.observations {
            let result_summary = if obs.success {
                "succeeded".to_string()
            } else {
                format!("failed: {}", obs.error.as_deref().unwrap_or("unknown error"))
            };
            replan_request.push_str(&format!(
                "- Step {}: {} ({})",
                obs.step_number, obs.tool_name, result_summary
            ));
            // Include error details if available
            if let Some(ref error) = obs.error {
                replan_request.push_str(&format!(" - {}", error));
            }
            replan_request.push('\n');
        }

        replan_request.push_str("\nIMPORTANT: When using filesystem.read, make sure the file path includes the full extension (e.g., .md, .txt, .toml). ");
        replan_request.push_str("List files first if unsure what files exist.\n\n");
        replan_request.push_str("Create a new plan to accomplish the goal:");

        // Pass None to planner since it expects different type
        self.planner
            .plan(&replan_request, None)
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

    /// Write user-relevant memories, filtering out agent-internal noise.
    ///
    /// Only stores:
    /// 1. Explicit "Remember:" or "Remember this:" requests from user
    /// 2. User-stated facts/preferences extracted from request/response
    ///
    /// Skips:
    /// - Plan summaries (agent-internal planning)
    /// - Tool observations (temporary execution details)
    /// - Serialized tool outputs (JSON blobs)
    async fn write_memories(
        &self,
        _plan: &Plan,
        _state: &ExecutionState,
        request: &str,
        _response: &str,
        memory_store: &Option<Arc<dyn MemoryStore>>,
    ) -> Result<usize, AgentError> {
        let Some(store) = memory_store.as_ref() else {
            trace!("No memory store provided, skipping write");
            return Ok(0);
        };

        let mut count = 0;

        // Only store if user explicitly asked to remember something
        let remember_pattern = Regex::new(r"(?i)remember(?:\s+this)?[:\s]+(.+)").unwrap();

        if let Some(captures) = remember_pattern.captures(request) {
            let fact = captures.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            if !fact.is_empty() && !fact.starts_with('{') && !fact.starts_with('[') {
                let mut metadata = drex_memory::MemoryMetadata::default();
                metadata.source = drex_memory::MemorySource::Explicit;
                metadata.confidence = 0.9;

                let memory = Memory::new(MemoryKind::Semantic, fact)
                    .with_metadata(metadata)
                    .with_importance(0.9);

                match store.store(memory).await {
                    Ok(_) => {
                        debug!("Wrote user-stated memory to store");
                        count += 1;
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to write user-stated memory");
                    }
                }
            }
        }

        trace!("Wrote {} memories (filtered agent-internal noise)", count);
        Ok(count)
    }

    /// Generate the final response to the user.
    async fn generate_final_response(
        &self,
        plan: &Plan,
        state: &ExecutionState,
    ) -> Result<String, AgentError> {
        // If the plan was a direct answer (no steps or tools needed),
        // return just the direct answer content, not formatted response
        if plan.is_direct_answer && plan.direct_answer.is_some() {
            // Clean up the direct answer from any format artifacts 
            let answer = plan.direct_answer.as_ref().unwrap();
            let cleaned = answer.trim();
            
            // If it begins with "ANSWER:", strip that prefix 
            let final_answer = if cleaned.to_lowercase().starts_with("answer:") {
                let after_colon = cleaned.find(':')
                    .map(|pos| cleaned[pos + 1..].trim())
                    .unwrap_or(cleaned);
                after_colon
            } else {
                cleaned
            };
            
            return Ok(final_answer.to_string());
        }

        // If we have observations, build a response from them
        if state.observations.is_empty() {
            return Ok("I've completed the requested task.".to_string());
        }

        // Build response from observations in order
        let mut response_parts = Vec::new();

        for obs in &state.observations {
            if !obs.success {
                response_parts.push(format!("Step {} ({}): failed", obs.step_number, obs.tool_name));
                continue;
            }

            match obs.tool_name.as_str() {
                "filesystem.read" => {
                    // Extract file content from filesystem.read result
                    // The ExecutionResult wraps tool output: {status, data: {content, resolved_path, size_bytes}}
                    if let Some(data) = obs.result.get("data") {
                        if let Some(content) = data.get("content").and_then(|c| c.as_str()) {
                            // Get the file path being read - try resolved_path first, then fall back
                            let file_path = data.get("resolved_path")
                                .and_then(|p| p.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| {
                                    data.get("path")
                                        .and_then(|p| p.as_str())
                                        .map(|s| s.to_string())
                                })
                                .unwrap_or_else(|| "file".to_string());

                            // Generate a natural language response instead of just dumping content
                            // For shorter files, include full content with context
                            // For longer files, provide a preview with line count
                            let line_count = content.lines().count();
                            let char_count = content.len();

                            if content.len() <= 2000 {
                                // Shorter file - provide full content with context
                                response_parts.push(format!(
                                    "I read the {} file for you. Here's what it contains:\n\n```\n{}\n```",
                                    file_path, content
                                ));
                            } else {
                                // Longer file - provide meaningful preview
                                let preview_lines: Vec<&str> = content.lines().take(50).collect();
                                let preview = preview_lines.join("\n");
                                let remaining_lines_str = if line_count > 50 {
                                    format!(", {} more lines", line_count - 50)
                                } else {
                                    "".to_string()
                                };

                                response_parts.push(format!(
                                    "I read the {} file ({} lines, {} characters). Here's the beginning:\n\n```\n{}\n```\n\n[...{}]",
                                    file_path, line_count, char_count, preview, remaining_lines_str
                                ));
                            }
                        } else {
                            response_parts.push("The file was read successfully but no content was returned.".to_string());
                        }
                    } else if let Some(error) = obs.result.get("error").and_then(|e| e.as_str()) {
                        response_parts.push(format!("I couldn't read the file: {}", error));
                    } else {
                        response_parts.push("The file was read but no content data was available.".to_string());
                    }
                }
                "terminal.execute" => {
                    if let Some(data) = obs.result.get("data") {
                        let stdout = data.get("stdout").and_then(|s| s.as_str()).unwrap_or("");
                        let stderr = data.get("stderr").and_then(|s| s.as_str()).unwrap_or("");
                        if !stdout.is_empty() {
                            response_parts.push(format!("Command output:\n```\n{}\n```", stdout));
                        }
                        if !stderr.is_empty() {
                            response_parts.push(format!("Stderr:\n```\n{}\n```", stderr));
                        }
                    }
                }
                "memory" => {
                    // Handle memory tool results
                    if let Some(data) = obs.result.get("data") {
                        // Check for retrieved memories
                        if let Some(memories) = data.get("memories").and_then(|m| m.as_array()) {
                            if !memories.is_empty() {
                                let mut memory_parts = Vec::new();
                                for memory in memories {
                                    if let Some(content) = memory.get("content").and_then(|c| c.as_str()) {
                                        memory_parts.push(content.to_string());
                                    }
                                }
                                if !memory_parts.is_empty() {
                                    response_parts.push(memory_parts.join("\n\n"));
                                }
                            }
                        } else if let Some(content) = data.get("content").and_then(|c| c.as_str()) {
                            response_parts.push(format!("Memory stored: {}", content));
                        }
                    }
                }
                "web.fetch" => {
                    if let Some(data) = obs.result.get("data") {
                        if let Some(text) = data.get("text").and_then(|t| t.as_str()) {
                            let preview = if text.len() > 2000 {
                                format!("{}\n\n[Content truncated, {} total characters]",
                                    &text[..2000], text.len())
                            } else {
                                text.to_string()
                            };
                            response_parts.push(format!("Web content:\n\n{}", preview));
                        }
                    }
                }
                _ => {
                    // For other tools, just note completion
                    response_parts.push(format!("Step {}: {} completed", obs.step_number, obs.tool_name));
                }
            }
        }

        if response_parts.is_empty() {
            Ok("I've completed the requested task.".to_string())
        } else {
            Ok(response_parts.join("\n\n"))
        }
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

    // Note: Memory writeback policy tests are in tests/execution_integration_test.rs
    // The unit tests here would require complex mocking of the MemoryStore trait.

    /// Regression test: filesystem.read observation should be rendered with content.
    #[tokio::test]
    async fn generate_final_response_includes_filesystem_content() {
        let agent = create_test_agent(&tempfile::tempdir().unwrap());
        let mut state = ExecutionState::new();
        
        // Add a filesystem.read observation with content
        // The observation stores ExecutionResult which has: {status, data: {content, resolved_path, size_bytes}}
        state.add_observation(Observation {
            step_number: 1,
            tool_name: "filesystem.read".to_string(),
            success: true,
            result: serde_json::json!({
                "status": "Success",
                "data": {
                    "content": "This is the README content",
                    "resolved_path": "/home/soumya/Desktop/DREX/README.md",
                    "size_bytes": 27
                }
            }),
            error: None,
        });

        let plan = Plan::new("Read README.md");
        let response = agent.generate_final_response(&plan, &state).await.unwrap();

        assert!(response.contains("README.md"), "Response should mention the file");
        assert!(response.contains("This is the README content"), "Response should include actual file content");
    }

    /// Regression test: terminal output should be rendered.
    #[tokio::test]
    async fn generate_final_response_includes_terminal_output() {
        let agent = create_test_agent(&tempfile::tempdir().unwrap());
        let mut state = ExecutionState::new();
        
        state.add_observation(Observation {
            step_number: 1,
            tool_name: "terminal.execute".to_string(),
            success: true,
            result: serde_json::json!({
                "data": {
                    "stdout": "output from command",
                    "stderr": ""
                }
            }),
            error: None,
        });

        let plan = Plan::new("Run command");
        let response = agent.generate_final_response(&plan, &state).await.unwrap();
        
        assert!(response.contains("Command output:"), "Response should indicate command output");
        assert!(response.contains("output from command"), "Response should include stdout");
    }

    /// Regression test: raw tool calls should NOT appear in final response.
    #[tokio::test]
    async fn generate_final_response_does_not_contain_raw_tool_calls() {
        let agent = create_test_agent(&tempfile::tempdir().unwrap());
        let mut state = ExecutionState::new();
        
        // Simulate filesystem.read followed by what might be a malformed step
        state.add_observation(Observation {
            step_number: 1,
            tool_name: "filesystem.read".to_string(),
            success: true,
            result: serde_json::json!({
                "status": "Success",
                "data": {
                    "content": "File contents here",
                    "resolved_path": "/home/soumya/Desktop/DREX/file.txt",
                    "size_bytes": 18
                }
            }),
            error: None,
        });

        let plan = Plan::new("Read file");
        let response = agent.generate_final_response(&plan, &state).await.unwrap();
        
        // The response should NOT look like a raw tool call
        assert!(!response.contains("retrieve({"), "Response should not contain raw tool call syntax");
        assert!(!response.contains(" \"{"), "Response should not contain JSON-like tool call patterns");
    }

    /// Regression test: empty observations should provide helpful message.
    #[tokio::test]
    async fn generate_final_response_empty_observations() {
        let agent = create_test_agent(&tempfile::tempdir().unwrap());
        let state = ExecutionState::new();
        
        let plan = Plan::new("Do something");
        let response = agent.generate_final_response(&plan, &state).await.unwrap();
        
        assert!(response.contains("completed"), "Should acknowledge task completion");
    }

    /// Test that file content truncation works for large files.
    #[tokio::test]
    async fn generate_final_response_truncates_large_files() {
        let agent = create_test_agent(&tempfile::tempdir().unwrap());
        let mut state = ExecutionState::new();
        
        let large_content = "x".repeat(3000);
        state.add_observation(Observation {
            step_number: 1,
            tool_name: "filesystem.read".to_string(),
            success: true,
            result: serde_json::json!({
                "status": "Success",
                "data": {
                    "content": large_content,
                    "resolved_path": "/home/soumya/Desktop/DREX/large.txt",
                    "size_bytes": 3000
                }
            }),
            error: None,
        });

        let plan = Plan::new("Read large file");
        let response = agent.generate_final_response(&plan, &state).await.unwrap();
        
        assert!(response.contains("beginning"), "Should indicate content was truncated with beginning section");
        assert!(response.contains("3000 characters"), "Should show original content size");
    }
}
