//! Planner - Natural language plan generation using ModelRouter
//!
//! This module provides the Planner component that accepts a user request
//! and retrieved memory/context, then uses ModelRouter to produce a
//! numbered natural-language plan.

use drex_memory::{Memory, MemoryId, MemoryKind, MemoryPatch, MemoryQuery, MemoryStore, MemoryStoreError};
use drex_models::{
    request::{Message, ModelRequest},
    router::{ModelRouter, TaskKind},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Errors that can occur during planning.
#[derive(Debug, Error)]
pub enum PlannerError {
    /// The model failed to generate a plan.
    #[error("Model error: {0}")]
    ModelError(String),

    /// The plan could not be parsed.
    #[error("Parse error: {0}")]
    ParseError(String),

    /// The model produced an invalid or empty response.
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Memory retrieval failed.
    #[error("Memory retrieval failed: {0}")]
    MemoryError(String),

    /// Router resolution failed.
    #[error("Router error: {0}")]
    RouterError(String),
}

/// A single step in a plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanStep {
    /// Step number (1-indexed).
    pub number: usize,

    /// Natural language description of what to do.
    pub description: String,

    /// Optional context about why this step is needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

/// A structured plan with numbered steps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    /// The original user request.
    pub request: String,

    /// Context that was retrieved and provided to the planner.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub context: Vec<String>,

    /// The numbered steps to execute.
    pub steps: Vec<PlanStep>,

    /// Whether this plan is a direct answer (no steps needed).
    #[serde(default)]
    pub is_direct_answer: bool,

    /// The direct answer content if no steps are needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_answer: Option<String>,
}

impl Plan {
    /// Create a new plan with the given request.
    pub fn new(request: impl Into<String>) -> Self {
        Self {
            request: request.into(),
            context: Vec::new(),
            steps: Vec::new(),
            is_direct_answer: false,
            direct_answer: None,
        }
    }

    /// Add a step to the plan.
    pub fn add_step(mut self, description: impl Into<String>) -> Self {
        let number = self.steps.len() + 1;
        self.steps.push(PlanStep {
            number,
            description: description.into(),
            rationale: None,
        });
        self
    }

    /// Add a step with rationale.
    pub fn add_step_with_rationale(
        mut self,
        description: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        let number = self.steps.len() + 1;
        self.steps.push(PlanStep {
            number,
            description: description.into(),
            rationale: Some(rationale.into()),
        });
        self
    }

    /// Mark this plan as a direct answer.
    pub fn with_direct_answer(mut self, answer: impl Into<String>) -> Self {
        self.is_direct_answer = true;
        self.direct_answer = Some(answer.into());
        self
    }

    /// Add context to the plan.
    pub fn with_context(mut self, context: Vec<String>) -> Self {
        self.context = context;
        self
    }

    /// Get the total number of steps.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Check if the plan is empty (no steps and not a direct answer).
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty() && !self.is_direct_answer
    }

    /// Get a step by number (1-indexed).
    pub fn get_step(&self, number: usize) -> Option<&PlanStep> {
        self.steps.iter().find(|s| s.number == number)
    }
}

/// The Planner generates natural language plans using a model backend.
pub struct Planner {
    router: Arc<ModelRouter>,
}

impl Planner {
    /// Create a new planner with the given model router.
    pub fn new(router: Arc<ModelRouter>) -> Self {
        Self { router }
    }

    /// Generate a plan for the given user request.
    ///
    /// This method:
    /// 1. Retrieves relevant context from memory
    /// 2. Constructs a planning prompt with the context
    /// 3. Sends it to the model via ModelRouter (TaskKind::Main)
    /// 4. Parses the model's response into a structured Plan
    ///
    /// # Arguments
    /// * `request` - The user's request
    /// * `memory_store` - Optional memory store reference for context retrieval
    ///
    /// # Returns
    /// A structured plan with numbered steps, or an error.
pub async fn plan(
    &self,
    request: &str,
    memory_store: Option<&dyn MemoryStore>,
) -> Result<Plan, PlannerError> {
    info!(request = %request, "Generating plan");

    // Step 1: Retrieve relevant context from memory
    let context = if let Some(store) = memory_store {
        match self.retrieve_context(store, request).await {
            Ok(ctx) => {
                debug!(context_count = ctx.len(), "Retrieved context from memory");
                ctx
            }
            Err(e) => {
                warn!(error = %e, "Failed to retrieve context, proceeding without");
                Vec::new()
            }
        }
    } else {
        debug!("No memory store provided, skipping context retrieval");
        Vec::new()
    };

    // Step 2: Resolve the model backend
    let backend = self
        .router
        .resolve(TaskKind::Main)
        .map_err(|e| PlannerError::RouterError(e.to_string()))?;

    // Step 3: Build the planning prompt
    let prompt = self.build_planning_prompt(request, &context);
    debug!(prompt_length = prompt.len(), "Built planning prompt");

    // Step 4: Send request to model
    let model_request = ModelRequest::new("gemma3:4b")
        .with_message(Message::user(prompt.clone()));
    let response = backend
        .complete(model_request)
        .await
        .map_err(|e| PlannerError::ModelError(e.to_string()))?;

    // Step 5: Extract and log the raw model output
    let raw_output = response.content.as_text().unwrap_or("").to_string();
    info!(
        response_length = raw_output.len(),
        "Received plan from model"
    );
    debug!(raw_plan = %raw_output, "Raw model-generated plan");

    // Step 6: Parse the response into a structured plan
    let plan = self.parse_plan_response(request, &context, &raw_output)?;

    info!(
        step_count = plan.step_count(),
        is_direct_answer = plan.is_direct_answer,
        "Plan generation complete"
    );

    Ok(plan)
}

    /// Retrieve relevant context from memory.
    async fn retrieve_context(
        &self,
        store: &dyn MemoryStore,
        request: &str,
    ) -> Result<Vec<String>, PlannerError> {
        // Create a query based on the request
        // Use semantic search with a reasonable limit
        let query = MemoryQuery::search(request).limit(5);

        let memories = store
            .retrieve(&query)
            .await
            .map_err(|e| PlannerError::MemoryError(e.to_string()))?;

        let context: Vec<String> = memories
            .into_iter()
            .map(|m| match m.kind {
                MemoryKind::Semantic => m.content,
                _ => format!("[{:?}] {}", m.kind, m.content),
            })
            .collect();

        Ok(context)
    }

    /// Build the planning prompt.
    fn build_planning_prompt(&self, request: &str, context: &[String]) -> String {
        let mut prompt = String::new();

        prompt.push_str(
            "You are a helpful planning assistant. Given a user request, create a clear, numbered plan of steps to accomplish the task.\n\n",
        );

        if !context.is_empty() {
            prompt.push_str("Relevant context from previous interactions:\n");
            for (i, ctx) in context.iter().enumerate() {
                prompt.push_str(&format!("{}. {}\n", i + 1, ctx));
            }
            prompt.push('\n');
        }

        prompt.push_str("User request:\n");
        prompt.push_str(request);
        prompt.push_str("\n\n");

        prompt.push_str(
            "Create a plan with numbered steps using the available tools.\n\n",
        );

        prompt.push_str("Available tools and their use:\n");
        prompt.push_str("- echo({\"message\": \"<text>\"}) - For testing or echoing information\n");
        prompt.push_str("- filesystem.read({\"path\": \"<file_path>\"}) - To read a file\n");
        prompt.push_str("- terminal.execute({\"command\": \"<cmd>\"}) - To run shell commands\n");
        prompt.push_str("- git.status({\"path\": \"<path>\"}) - To check git repository status\n");
        prompt.push_str("- git.diff({\"path\": \"<path>\"}) - To see git changes\n");
        prompt.push_str("- web.fetch({\"url\": \"<url>\"}) - To fetch web pages\n");
        prompt.push_str("- memory({\"action\": \"store\", \"content\": \"<text>\"}) - To store a memory\n");
        prompt.push_str("- memory({\"action\": \"retrieve\", \"content\": \"<query>\"}) - To retrieve memories\n\n");

        prompt.push_str(
            "If this is a simple question that doesn't require multiple steps, simply provide the answer directly.\n\n",
        );

        prompt.push_str("IMPORTANT: Return your response in this exact format:\n\n");

        prompt.push_str("FORMAT 1 - For multi-step tasks (use tool call syntax):\n");
        prompt.push_str("Step 1: call tool_name({\"param\": \"value\"})\n");
        prompt.push_str("Step 2: call another_tool({\"param\": \"value\"})\n");
        prompt.push_str("...and so on\n\n");

        prompt.push_str("FORMAT 2 - For direct answers:\n");
        prompt.push_str("ANSWER: <your direct response>\n\n");

        prompt.push_str("Begin your response:\n");

        prompt
    }

    /// Parse the model response into a structured Plan.
    fn parse_plan_response(
        &self,
        request: &str,
        context: &[String],
        raw_output: &str,
    ) -> Result<Plan, PlannerError> {
        let trimmed = raw_output.trim();

        if trimmed.is_empty() {
            return Err(PlannerError::InvalidResponse(
                "Model returned empty response".to_string(),
            ));
        }

        // Check for direct answer format
        if let Some(answer) = trimmed.strip_prefix("ANSWER:") {
            return Ok(Plan::new(request)
                .with_context(context.to_vec())
                .with_direct_answer(answer.trim()));
        }

        // Check if output starts with ANSWER (case insensitive)
        let lower = trimmed.to_lowercase();
        if lower.starts_with("answer:") {
            if let Some(idx) = trimmed.find(':') {
                let answer = trimmed[idx + 1..].trim();
                return Ok(Plan::new(request)
                    .with_context(context.to_vec())
                    .with_direct_answer(answer));
            }
        }

        // Parse numbered steps
        let mut plan = Plan::new(request).with_context(context.to_vec());
        let mut steps = Vec::new();

        for line in trimmed.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Match patterns like "Step 1:", "1.", "1)", "Step 1 -", etc.
            let step_regex = regex::Regex::new(
                r"^(?i)(?:step\s+)?(\d+)[:)\-.\s]+(?:\s+)?(.+)$"
            ).ok();

            if let Some(regex) = step_regex {
                if let Some(caps) = regex.captures(line) {
                    if let (Some(num), Some(desc)) = (caps.get(1), caps.get(2)) {
                        let number: usize = num.as_str().parse().unwrap_or(steps.len() + 1);
                        let description = desc.as_str().trim().to_string();
                        if !description.is_empty() {
                            steps.push(PlanStep {
                                number,
                                description,
                                rationale: None,
                            });
                        }
                    }
                }
            }
        }

        // Also try simpler parsing for lines that just start with numbers
        if steps.is_empty() {
            for line in trimmed.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // Try to find any line that starts with a number
                if let Some(first_char) = line.chars().next() {
                    if first_char.is_ascii_digit() {
                        // Find where the number ends
                        let mut num_end = 1;
                        while num_end < line.len()
                            && line.chars().nth(num_end).map_or(false, |c| c.is_ascii_digit())
                        {
                            num_end += 1;
                        }

                        if let Ok(number) = line[..num_end].parse::<usize>() {
                            // Skip punctuation after the number
                            let mut desc_start = num_end;
                            while desc_start < line.len()
                                && line
                                    .chars()
                                    .nth(desc_start)
                                    .map_or(false, |c| matches!(c, '.' | ')' | ':' | '-' | ' '))
                            {
                                desc_start += 1;
                            }

                            let description = line[desc_start..].trim().to_string();
                            if !description.is_empty() {
                                steps.push(PlanStep {
                                    number,
                                    description,
                                    rationale: None,
                                });
                            }
                        }
                    }
                }
            }
        }

        // If we found steps, sort them by number and assign sequential numbers
        if !steps.is_empty() {
            steps.sort_by_key(|s| s.number);

            // Re-number steps sequentially
            for (i, step) in steps.iter_mut().enumerate() {
                step.number = i + 1;
            }

            plan.steps = steps;
            return Ok(plan);
        }

        // If no steps were parsed but there's content, treat it as a single implicit step
        if !trimmed.is_empty() {
            return Ok(plan.add_step(trimmed));
        }

        Err(PlannerError::ParseError(
            "Could not parse model response into plan steps".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drex_models::{
        capabilities::{BackendCapability, CapabilitySet},
        content::Content,
        request::ModelRequest,
        response::ModelResponse,
    };
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock backend for testing planner
    struct MockPlannerBackend {
        response: String,
        call_count: AtomicUsize,
    }

    impl MockPlannerBackend {
        fn new(response: impl Into<String>) -> Self {
            Self {
                response: response.into(),
                call_count: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl drex_models::ModelBackend for MockPlannerBackend {
        async fn complete(&self, _request: ModelRequest) -> Result<ModelResponse, drex_models::error::ModelError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(ModelResponse::new(
                "test-id",
                "mock-model",
                "mock",
                Content::text(&self.response),
            ))
        }

        fn supports(&self, _capability: BackendCapability) -> bool {
            true
        }

        fn provider_name(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-planner"
        }
    }

    #[test]
    fn plan_new_is_empty() {
        let plan = Plan::new("Test request");
        assert_eq!(plan.request, "Test request");
        assert!(plan.steps.is_empty());
        assert!(!plan.is_direct_answer);
    }

    #[test]
    fn plan_add_step() {
        let plan = Plan::new("Test request")
            .add_step("First step")
            .add_step("Second step");

        assert_eq!(plan.step_count(), 2);
        assert_eq!(plan.steps[0].number, 1);
        assert_eq!(plan.steps[0].description, "First step");
        assert_eq!(plan.steps[1].number, 2);
        assert_eq!(plan.steps[1].description, "Second step");
    }

    #[test]
    fn plan_direct_answer() {
        let plan = Plan::new("What is 2+2?")
            .with_direct_answer("2+2 equals 4.");

        assert!(plan.is_direct_answer);
        assert_eq!(plan.direct_answer, Some("2+2 equals 4.".to_string()));
        assert!(plan.steps.is_empty());
    }

    #[test]
    fn plan_with_context() {
        let plan = Plan::new("Test request")
            .with_context(vec!["Previous context 1".to_string(), "Previous context 2".to_string()])
            .add_step("Do something");

        assert_eq!(plan.context.len(), 2);
        assert_eq!(plan.context[0], "Previous context 1");
    }

    #[test]
    fn plan_get_step() {
        let plan = Plan::new("Test")
            .add_step("Step one")
            .add_step("Step two");

        assert!(plan.get_step(1).is_some());
        assert!(plan.get_step(2).is_some());
        assert!(plan.get_step(3).is_none());

        let step1 = plan.get_step(1).unwrap();
        assert_eq!(step1.description, "Step one");
    }

    #[tokio::test]
    async fn planner_generates_plan_with_steps() {
        let mock_response = "Step 1: Search for relevant files\nStep 2: Read the files\nStep 3: Summarize content";
        let backend = MockPlannerBackend::new(mock_response);

        let mut router = ModelRouter::new();
        router.register(TaskKind::Main, Box::new(backend));

        let planner = Planner::new(Arc::new(router));

        // Plan without memory store
        let plan = planner.plan("Find and summarize documentation", None::<&dyn MemoryStore>).await.unwrap();

        assert_eq!(plan.step_count(), 3);
        assert!(!plan.is_direct_answer);
        assert_eq!(plan.steps[0].description, "Search for relevant files");
    }

    #[tokio::test]
    async fn planner_handles_direct_answer() {
        let mock_response = "ANSWER: The answer is 42.";
        let backend = MockPlannerBackend::new(mock_response);

        let mut router = ModelRouter::new();
        router.register(TaskKind::Main, Box::new(backend));

        let planner = Planner::new(Arc::new(router));

        let plan = planner.plan("What is the meaning of life?", None::<&dyn MemoryStore>).await.unwrap();

        assert!(plan.is_direct_answer);
        assert_eq!(plan.direct_answer, Some("The answer is 42.".to_string()));
    }

    #[tokio::test]
    async fn planner_parses_numbered_list() {
        let mock_response = "1. First step\n2. Second step\n3. Third step";
        let backend = MockPlannerBackend::new(mock_response);

        let mut router = ModelRouter::new();
        router.register(TaskKind::Main, Box::new(backend));

        let planner = Planner::new(Arc::new(router));

        let plan = planner.plan("Do something", None::<&dyn MemoryStore>).await.unwrap();

        assert_eq!(plan.step_count(), 3);
        assert_eq!(plan.steps[0].number, 1);
        assert_eq!(plan.steps[0].description, "First step");
        assert_eq!(plan.steps[2].description, "Third step");
    }

    #[tokio::test]
    async fn planner_handles_empty_response() {
        let mock_response = "";
        let backend = MockPlannerBackend::new(mock_response);

        let mut router = ModelRouter::new();
        router.register(TaskKind::Main, Box::new(backend));

        let planner = Planner::new(Arc::new(router));

        let result = planner.plan("Do something", None::<&dyn MemoryStore>).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[tokio::test]
    async fn planner_preserves_request_in_output() {
        let mock_response = "Step 1: Do the thing";
        let backend = MockPlannerBackend::new(mock_response);

        let mut router = ModelRouter::new();
        router.register(TaskKind::Main, Box::new(backend));

        let planner = Planner::new(Arc::new(router));

        let request = "A very specific request about something";
        let plan = planner.plan(request, None::<&dyn MemoryStore>).await.unwrap();

        assert_eq!(plan.request, request);
    }

    #[tokio::test]
    async fn planner_handles_malformed_steps() {
        // Response without clear step format should still create a single step
        let mock_response = "This is just some text without clear steps";
        let backend = MockPlannerBackend::new(mock_response);

        let mut router = ModelRouter::new();
        router.register(TaskKind::Main, Box::new(backend));

        let planner = Planner::new(Arc::new(router));

        let plan = planner.plan("Do something", None::<&dyn MemoryStore>).await.unwrap();

        // Should create a single implicit step
        assert_eq!(plan.step_count(), 1);
    }

    // Mock memory store for testing context retrieval
    struct MockMemoryStore;

    #[async_trait]
    impl MemoryStore for MockMemoryStore {
        async fn store(&self, _memory: Memory) -> std::result::Result<MemoryId, MemoryStoreError> {
            unimplemented!()
        }

        async fn retrieve(&self, _query: &MemoryQuery) -> std::result::Result<Vec<Memory>, MemoryStoreError> {
            // Return some mock memories
            Ok(vec![
                Memory::new(MemoryKind::Semantic, "Previous context about testing"),
            ])
        }

        async fn forget(&self, _id: MemoryId) -> std::result::Result<(), MemoryStoreError> {
            unimplemented!()
        }

        async fn update(&self, _id: MemoryId, _patch: MemoryPatch) -> std::result::Result<Memory, MemoryStoreError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn planner_retrieves_context_from_memory() {
        let mock_response = "Step 1: Use the context";
        let backend = MockPlannerBackend::new(mock_response);

        let mut router = ModelRouter::new();
        router.register(TaskKind::Main, Box::new(backend));

        let planner = Planner::new(Arc::new(router));
        let memory_store = MockMemoryStore;

        let plan = planner.plan("Do something", Some(&memory_store)).await.unwrap();

        // Context should be populated
        assert!(!plan.context.is_empty());
        assert!(plan.context[0].contains("Previous context"));
    }
}
