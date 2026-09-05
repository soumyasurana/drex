//! Context Engine - Intelligent context assembly and budget management
//!
//! This module provides structured context construction and token budgeting
//! for agent interactions. It ensures:
//! - Explicit separation of context components
//! - Token budget management
//! - Priority-based content inclusion
//! - Context truncation strategies
//!
//! # Context Components
//!
//! The context is assembled from these distinct layers:
//! 1. System instructions (always included)
//! 2. User request (always included)
//! 3. Working context (retrieved memories)
//! 4. Task state (current run information)
//! 5. Tool definitions
//! 6. Tool results (with size limits)
//! 7. Previous observations
//! 8. Agent decisions

use crate::{AgentDecision, RunState, RunStep};
use serde::{Deserialize, Serialize};

/// A token budget for context allocation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Total tokens available.
    pub total: usize,
    /// Reserved for system instructions.
    pub system: usize,
    /// Reserved for user request.
    pub user: usize,
    /// Reserved for working context (retrieved memories).
    pub context: usize,
    /// Reserved for task state.
    pub task: usize,
    /// Reserved for tool definitions.
    pub tools: usize,
    /// Reserved for tool results.
    pub results: usize,
}

impl TokenBudget {
    /// Create a new budget with specified total.
    pub fn new(total: usize) -> Self {
        // Default allocation:
        // - System: 10%
        // - User: 10%
        // - Context: 30%
        // - Task: 5%
        // - Tools: 15%
        // - Results: 30%
        Self {
            total,
            system: total / 10,
            user: total / 10,
            context: total * 3 / 10,
            task: total / 20,
            tools: total * 15 / 100,
            results: total * 3 / 10,
        }
    }

    /// Create a conservative budget (leaves 20% buffer).
    pub fn conservative(total: usize) -> Self {
        let conservative_total = total * 8 / 10;
        Self::new(conservative_total)
    }

    /// Adjust system allocation.
    pub fn with_system(mut self, tokens: usize) -> Self {
        self.system = tokens.min(self.total);
        self
    }

    /// Adjust user allocation.
    pub fn with_user(mut self, tokens: usize) -> Self {
        self.user = tokens.min(self.total);
        self
    }

    /// Adjust context allocation.
    pub fn with_context(mut self, tokens: usize) -> Self {
        self.context = tokens.min(self.total);
        self
    }

    /// Adjust task allocation.
    pub fn with_task(mut self, tokens: usize) -> Self {
        self.task = tokens.min(self.total);
        self
    }

    /// Adjust tools allocation.
    pub fn with_tools(mut self, tokens: usize) -> Self {
        self.tools = tokens.min(self.total);
        self
    }

    /// Adjust results allocation.
    pub fn with_results(mut self, tokens: usize) -> Self {
        self.results = tokens.min(self.total);
        self
    }

    /// Calculate sum of all allocations.
    pub fn allocated(&self) -> usize {
        self.system + self.user + self.context + self.task + self.tools + self.results
    }

    /// Check if allocations exceed total.
    pub fn is_over_allocated(&self) -> bool {
        self.allocated() > self.total
    }

    /// Get remaining unallocated tokens.
    pub fn remaining(&self) -> usize {
        if self.allocated() > self.total {
            0
        } else {
            self.total - self.allocated()
        }
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self::new(4000) // Default 4K context
    }
}

/// A context component with its priority.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrioritizedItem<T> {
    /// Priority (higher = more important).
    pub priority: u8,
    /// Estimated token count.
    pub tokens: usize,
    /// The content.
    pub content: T,
}

impl<T> PrioritizedItem<T> {
    /// Create a new prioritized item.
    pub fn new(priority: u8, tokens: usize, content: T) -> Self {
        Self {
            priority,
            tokens,
            content,
        }
    }
}

/// Strategies for truncating content when over budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncationStrategy {
    /// Drop lowest priority items first.
    DropLowestPriority,
    /// Truncate each item proportionally.
    TruncateProportionally,
    /// Keep most recent items, drop oldest.
    KeepRecent,
    /// Summarize instead of truncate.
    Summarize,
}

impl Default for TruncationStrategy {
    fn default() -> Self {
        Self::DropLowestPriority
    }
}

/// A context section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContextSection {
    /// System instructions.
    System { content: String },
    /// User request.
    User { content: String },
    /// Retrieved memories.
    Memories { items: Vec<String> },
    /// Task state.
    TaskState { state: String },
    /// Tool definitions.
    ToolDefinitions { definitions: Vec<String> },
    /// Tool results.
    ToolResults { results: Vec<String> },
    /// Previous observations.
    Observations { items: Vec<String> },
    /// Recent decisions.
    Decisions { items: Vec<String> },
}

impl ContextSection {
    /// Get the section name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::System { .. } => "system",
            Self::User { .. } => "user",
            Self::Memories { .. } => "memories",
            Self::TaskState { .. } => "task_state",
            Self::ToolDefinitions { .. } => "tool_definitions",
            Self::ToolResults { .. } => "tool_results",
            Self::Observations { .. } => "observations",
            Self::Decisions { .. } => "decisions",
        }
    }

    /// Estimate token count (rough heuristic: 4 chars ≈ 1 token).
    pub fn estimated_tokens(&self) -> usize {
        let chars = match self {
            Self::System { content } => content.len(),
            Self::User { content } => content.len(),
            Self::Memories { items } => items.iter().map(|s| s.len()).sum(),
            Self::TaskState { state } => state.len(),
            Self::ToolDefinitions { definitions } => definitions.iter().map(|s| s.len()).sum(),
            Self::ToolResults { results } => results.iter().map(|s| s.len()).sum(),
            Self::Observations { items } => items.iter().map(|s| s.len()).sum(),
            Self::Decisions { items } => items.iter().map(|s| s.len()).sum(),
        };
        (chars / 4).max(1)
    }

    /// Render to string for inclusion in prompt.
    pub fn render(&self) -> String {
        match self {
            Self::System { content } => format!("<system>\n{}\n</system>\n", content),
            Self::User { content } => format!("<user>\n{}\n</user>\n", content),
            Self::Memories { items } => {
                if items.is_empty() {
                    String::new()
                } else {
                    let memories = items.join("\n\n");
                    format!("<context>\n{}\n</context>\n", memories)
                }
            }
            Self::TaskState { state } => {
                format!("<task_state>\n{}\n</task_state>\n", state)
            }
            Self::ToolDefinitions { definitions } => {
                if definitions.is_empty() {
                    String::new()
                } else {
                    let defs = definitions.join("\n\n");
                    format!("<tools>\n{}\n</tools>\n", defs)
                }
            }
            Self::ToolResults { results } => {
                if results.is_empty() {
                    String::new()
                } else {
                    let res = results.join("\n\n");
                    format!("<tool_results>\n{}\n</tool_results>\n", res)
                }
            }
            Self::Observations { items } => {
                if items.is_empty() {
                    String::new()
                } else {
                    let obs = items.join("\n\n");
                    format!("<observations>\n{}\n</observations>\n", obs)
                }
            }
            Self::Decisions { items } => {
                if items.is_empty() {
                    String::new()
                } else {
                    let decs = items.join("\n\n");
                    format!("<decisions>\n{}\n</decisions>\n", decs)
                }
            }
        }
    }
}

/// The context engine for assembling agent context.
#[derive(Debug, Clone)]
pub struct ContextEngine {
    budget: TokenBudget,
    strategy: TruncationStrategy,
}

impl ContextEngine {
    /// Create a new context engine with the given budget.
    pub fn new(budget: TokenBudget) -> Self {
        Self {
            budget,
            strategy: TruncationStrategy::default(),
        }
    }

    /// Set the truncation strategy.
    pub fn with_strategy(mut self, strategy: TruncationStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Assemble context from sections.
    pub fn assemble(&self, sections: Vec<ContextSection>) -> AssembledContext {
        let mut result = String::new();
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        let mut total_tokens = 0;

        // Always include system and user
        for section in &sections {
            match section {
                ContextSection::System { .. } | ContextSection::User { .. } => {
                    let tokens = section.estimated_tokens();
                    result.push_str(&section.render());
                    included.push(section.name());
                    total_tokens += tokens;
                }
                _ => {}
            }
        }

        // Process other sections based on budget
        for section in sections {
            match section {
                ContextSection::System { .. } | ContextSection::User { .. } => continue,
                _ => {
                    let tokens = section.estimated_tokens();
                    let budget_remaining = self.budget.remaining();

                    if total_tokens + tokens <= budget_remaining {
                        result.push_str(&section.render());
                        included.push(section.name());
                        total_tokens += tokens;
                    } else {
                        excluded.push(section.name());
                    }
                }
            }
        }

        AssembledContext {
            content: result.trim().to_string(),
            included_sections: included,
            excluded_sections: excluded,
            estimated_tokens: total_tokens,
        }
    }

    /// Build system instructions.
    pub fn build_system_instructions(capabilities: &[String]) -> ContextSection {
        let content = format!(
            "You are Drex, an AI assistant. You have access to the following capabilities:\n{}\n\n\
             You must respond with structured JSON decisions following the decision schema.",
            capabilities.join("\n")
        );
        ContextSection::System { content }
    }

    /// Build user request section.
    pub fn build_user_request(request: &str) -> ContextSection {
        ContextSection::User {
            content: request.to_string(),
        }
    }

    /// Build memories section.
    pub fn build_memories(memories: Vec<String>) -> ContextSection {
        ContextSection::Memories { items: memories }
    }

    /// Build task state section.
    pub fn build_task_state(run_state: &RunState) -> ContextSection {
        let state = format!(
            "Run ID: {}\nStatus: {}\nSteps completed: {}\nProgress: {}%",
            run_state.id,
            run_state.status,
            run_state.steps.len(),
            run_state
                .progress
                .as_ref()
                .map(|p| p.percent_complete)
                .unwrap_or(0)
        );
        ContextSection::TaskState { state }
    }

    /// Build observations section.
    pub fn build_observations(steps: &[RunStep]) -> ContextSection {
        let items = steps
            .iter()
            .map(|s| {
                format!(
                    "Step {}: {} (success: {})",
                    s.number, s.description, s.success
                )
            })
            .collect();
        ContextSection::Observations { items }
    }

    /// Build decisions section.
    pub fn build_decisions(decisions: &[AgentDecision]) -> ContextSection {
        let items = decisions
            .iter()
            .map(|d| match d {
                AgentDecision::ToolCall(tc) => format!(
                    "Called {}: expected {}",
                    tc.tool_call.tool_name,
                    tc.expected_outcome.as_deref().unwrap_or("success")
                ),
                AgentDecision::FinalAnswer(fa) => format!("Answer: {}", fa.response),
                AgentDecision::Replan(r) => format!("Replan: {}", r.reason),
                AgentDecision::Failure(f) => format!("Failed: {}", f.reason),
                AgentDecision::Continue(c) => {
                    format!("Continue: {}", c.reason.as_deref().unwrap_or(""))
                }
            })
            .collect();
        ContextSection::Decisions { items }
    }

    /// Get the current budget.
    pub fn budget(&self) -> &TokenBudget {
        &self.budget
    }

    /// Get the truncation strategy.
    pub fn strategy(&self) -> TruncationStrategy {
        self.strategy
    }
}

/// Assembled context result.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledContext {
    /// The assembled context content.
    pub content: String,
    /// Sections that were included.
    pub included_sections: Vec<&'static str>,
    /// Sections that were excluded.
    pub excluded_sections: Vec<&'static str>,
    /// Estimated token count.
    pub estimated_tokens: usize,
}

impl AssembledContext {
    /// Check if any sections were excluded.
    pub fn was_truncated(&self) -> bool {
        !self.excluded_sections.is_empty()
    }

    /// Get the content for model consumption.
    pub fn for_model(&self) -> &str {
        &self.content
    }
}

/// Configuration for the context engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEngineConfig {
    /// Default context window size.
    pub default_context_window: usize,
    /// Budget allocation percentages.
    pub budget_allocation: BudgetAllocation,
    /// Default truncation strategy.
    pub default_strategy: TruncationStrategy,
}

/// Percentage allocations for budget categories.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BudgetAllocation {
    pub system: u8,
    pub user: u8,
    pub context: u8,
    pub task: u8,
    pub tools: u8,
    pub results: u8,
}

impl Default for BudgetAllocation {
    fn default() -> Self {
        Self {
            system: 10,
            user: 10,
            context: 30,
            task: 5,
            tools: 15,
            results: 30,
        }
    }
}

/// Error from context operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ContextError {
    #[error("Token budget exceeded: {used}/{budget}")]
    BudgetExceeded { used: usize, budget: usize },

    #[error("Invalid budget configuration: {0}")]
    InvalidBudget(String),

    #[error("Context section too large: {section} ({tokens} tokens)")]
    SectionTooLarge {
        section: String,
        tokens: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_budget_new() {
        let budget = TokenBudget::new(10000);
        assert_eq!(budget.total, 10000);
        assert!(budget.system > 0);
        assert!(budget.user > 0);
        assert!(budget.context > 0);
        assert_eq!(budget.allocated(), budget.system + budget.user + budget.context + budget.task + budget.tools + budget.results);
    }

    #[test]
    fn token_budget_conservative() {
        let budget = TokenBudget::conservative(10000);
        assert!(budget.total <= 8000); // Should be 80% of original
    }

    #[test]
    fn token_budget_is_over_allocated() {
        let mut budget = TokenBudget::new(100);
        budget.system = 200;
        assert!(budget.is_over_allocated());
    }

    #[test]
    fn token_budget_remaining() {
        let budget = TokenBudget::conservative(1000);
        assert!(budget.remaining() >= budget.total - budget.allocated());
    }

    #[test]
    fn context_section_name() {
        assert_eq!(ContextSection::System { content: String::new() }.name(), "system");
        assert_eq!(ContextSection::User { content: String::new() }.name(), "user");
    }

    #[test]
    fn context_section_estimated_tokens() {
        let section = ContextSection::System {
            content: "word ".repeat(100), // 500 chars
        };
        assert_eq!(section.estimated_tokens(), 125); // 500 / 4
    }

    #[test]
    fn context_section_render_system() {
        let section = ContextSection::System {
            content: "instructions".to_string(),
        };
        let rendered = section.render();
        assert!(rendered.contains("<system>"));
        assert!(rendered.contains("instructions"));
        assert!(rendered.contains("</system>"));
    }

    #[test]
    fn context_engine_assemble() {
        let engine = ContextEngine::new(TokenBudget::new(10000));
        let sections = vec![
            ContextSection::System { content: "Test system".to_string() },
            ContextSection::User { content: "Test user".to_string() },
        ];

        let assembled = engine.assemble(sections);
        assert!(!assembled.content.is_empty());
        assert!(assembled.content.contains("system"));
        assert!(assembled.content.contains("user"));
        assert!(!assembled.was_truncated());
    }

    #[test]
    fn context_engine_truncates_over_budget() {
        let engine = ContextEngine::new(TokenBudget::new(100));
        let sections = vec![
            ContextSection::System { content: "system".to_string() },
            ContextSection::User { content: "user".to_string() },
            ContextSection::Memories { items: vec!["x".repeat(1000); 10] }, // Large memories
        ];

        let assembled = engine.assemble(sections);
        assert!(assembled.was_truncated());
        assert!(assembled.excluded_sections.contains(&"memories"));
    }

    #[test]
    fn context_engine_build_system_instructions() {
        let caps: Vec<String> = vec!["read_files".to_string(), "execute_shell".to_string()];
        let section = ContextEngine::build_system_instructions(&caps);
        match section {
            ContextSection::System { content } => {
                assert!(content.contains("read_files"));
                assert!(content.contains("execute_shell"));
            }
            _ => panic!("Expected system section"),
        }
    }

    #[test]
    fn context_engine_build_user_request() {
        let section = ContextEngine::build_user_request("Test request");
        match section {
            ContextSection::User { content } => {
                assert_eq!(content, "Test request");
            }
            _ => panic!("Expected user section"),
        }
    }

    #[test]
    fn context_engine_build_memories() {
        let memories = vec!["Memory 1".to_string(), "Memory 2".to_string()];
        let section = ContextEngine::build_memories(memories);
        match section {
            ContextSection::Memories { items } => {
                assert_eq!(items.len(), 2);
            }
            _ => panic!("Expected memories section"),
        }
    }

    #[test]
    fn assembled_context_was_truncated() {
        let ctx = AssembledContext {
            content: "test".to_string(),
            included_sections: vec!["system"],
            excluded_sections: vec![],
            estimated_tokens: 1,
        };
        assert!(!ctx.was_truncated());

        let ctx = AssembledContext {
            content: "test".to_string(),
            included_sections: vec!["system"],
            excluded_sections: vec!["memories"],
            estimated_tokens: 1,
        };
        assert!(ctx.was_truncated());
    }

    #[test]
    fn budget_allocation_default() {
        let alloc = BudgetAllocation::default();
        let total = alloc.system + alloc.user + alloc.context + alloc.task + alloc.tools + alloc.results;
        assert_eq!(total, 100); // Should sum to 100%
    }
}
