use errors::ContextraError;
use handlebars::Handlebars;
use memory::LongTermMemory;
use providers::{ChatMessage, ChatRequest, ChatRole};
use retrieval::RankedChunk;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use types::{Message, Role};

const DEFAULT_PROMPT_TOKEN_BUDGET: usize = 8_000;
const DEFAULT_MODEL: &str = "gpt-4.1-mini";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TemplateId {
    pub name: String,
    pub version: String,
}

impl TemplateId {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }

    fn registry_name(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub id: TemplateId,
    pub source: String,
}

impl PromptTemplate {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            id: TemplateId::new(name, version),
            source: source.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PromptTemplateRegistry {
    handlebars: Handlebars<'static>,
    templates: HashMap<String, PromptTemplate>,
}

impl PromptTemplateRegistry {
    pub fn new() -> Self {
        Self {
            handlebars: Handlebars::new(),
            templates: HashMap::new(),
        }
    }

    pub fn load_from_dir(path: impl AsRef<Path>) -> Result<Self, ContextraError> {
        let path = path.as_ref();
        let mut registry = Self::new();

        let partials_dir = path.join("partials");
        if partials_dir.exists() {
            for entry in fs::read_dir(&partials_dir)? {
                let entry = entry?;
                let entry_path = entry.path();
                if is_template_file(&entry_path) {
                    let name = file_stem(&entry_path)?;
                    let source = fs::read_to_string(&entry_path)?;
                    registry.register_partial(&name, &source)?;
                }
            }
        }

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path.is_file() && is_template_file(&entry_path) {
                let (name, version) = parse_template_filename(&entry_path)?;
                let source = fs::read_to_string(&entry_path)?;
                registry.register(PromptTemplate::new(name, version, source))?;
            }
        }

        Ok(registry)
    }

    pub fn register(&mut self, template: PromptTemplate) -> Result<(), ContextraError> {
        let registry_name = template.id.registry_name();
        self.handlebars
            .register_template_string(&registry_name, &template.source)
            .map_err(|error| {
                ContextraError::Validation(format!("failed to register prompt template: {error}"))
            })?;
        self.templates.insert(registry_name, template);
        Ok(())
    }

    pub fn register_partial(&mut self, name: &str, source: &str) -> Result<(), ContextraError> {
        self.handlebars
            .register_partial(name, source)
            .map_err(|error| {
                ContextraError::Validation(format!("failed to register prompt partial: {error}"))
            })
    }

    pub fn render(
        &self,
        name: &str,
        version: &str,
        data: &Value,
    ) -> Result<String, ContextraError> {
        let registry_name = TemplateId::new(name, version).registry_name();
        self.handlebars
            .render(&registry_name, data)
            .map_err(|error| {
                ContextraError::Validation(format!("failed to render prompt template: {error}"))
            })
    }

    pub fn has_template(&self, name: &str, version: &str) -> bool {
        self.templates
            .contains_key(&TemplateId::new(name, version).registry_name())
    }
}

impl Default for PromptTemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptBlockKind {
    RetrievedContext,
    Memory,
    Conversation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptBlock {
    pub kind: PromptBlockKind,
    pub content: String,
    pub priority: f32,
    pub recency: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OptimizedPromptContext {
    pub retrieved_context: Vec<RankedChunk>,
    pub memories: Vec<LongTermMemory>,
    pub conversation_history: Vec<Message>,
    pub token_count: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ApproximatePromptTokenizer;

impl ApproximatePromptTokenizer {
    pub fn count(&self, text: &str) -> usize {
        let wordish = text
            .split_whitespace()
            .map(|token| token.chars().count().div_ceil(4).max(1))
            .sum::<usize>();
        wordish.max(text.chars().count().div_ceil(4))
    }

    pub fn count_message(&self, message: &Message) -> usize {
        role_cost(&message.role) + self.count(&message.content)
    }
}

#[derive(Debug, Clone)]
pub struct PromptOptimizer {
    token_budget: usize,
    tokenizer: ApproximatePromptTokenizer,
}

impl PromptOptimizer {
    pub fn new(token_budget: usize) -> Self {
        Self {
            token_budget: token_budget.max(1),
            tokenizer: ApproximatePromptTokenizer,
        }
    }

    pub fn token_budget(&self) -> usize {
        self.token_budget
    }

    pub fn optimize(
        &self,
        retrieved_context: &[RankedChunk],
        memories: &[LongTermMemory],
        conversation_history: &[Message],
    ) -> OptimizedPromptContext {
        let mut selected_retrieved = Vec::new();
        let mut selected_memories = Vec::new();
        let mut selected_conversation = Vec::new();
        let mut used_tokens = 0_usize;

        let mut chunks = retrieved_context.to_vec();
        chunks.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });

        for chunk in chunks {
            let tokens = self.tokenizer.count(&chunk.content);
            if fits_budget(used_tokens, tokens, self.token_budget) {
                used_tokens += tokens;
                selected_retrieved.push(chunk);
            }
        }

        let mut memory_blocks = memories.to_vec();
        memory_blocks.sort_by(|left, right| {
            right
                .importance
                .partial_cmp(&left.importance)
                .unwrap_or(Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });

        for memory in memory_blocks {
            let tokens = self.tokenizer.count(&memory.content);
            if fits_budget(used_tokens, tokens, self.token_budget) {
                used_tokens += tokens;
                selected_memories.push(memory);
            }
        }

        let mut selected_reversed = Vec::new();
        for message in conversation_history.iter().rev() {
            let tokens = self.tokenizer.count_message(message);
            if fits_budget(used_tokens, tokens, self.token_budget) {
                used_tokens += tokens;
                selected_reversed.push(message.clone());
            }
        }
        selected_reversed.reverse();
        selected_conversation.extend(selected_reversed);

        OptimizedPromptContext {
            retrieved_context: selected_retrieved,
            memories: selected_memories,
            conversation_history: selected_conversation,
            token_count: used_tokens,
        }
    }
}

impl Default for PromptOptimizer {
    fn default() -> Self {
        Self::new(DEFAULT_PROMPT_TOKEN_BUDGET)
    }
}

#[derive(Debug, Clone)]
pub struct PromptBuilder {
    model: String,
    system_prompt: String,
    optimizer: PromptOptimizer,
}

impl PromptBuilder {
    pub fn new(model: impl Into<String>, system_prompt: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system_prompt: system_prompt.into(),
            optimizer: PromptOptimizer::default(),
        }
    }

    pub fn with_optimizer(mut self, optimizer: PromptOptimizer) -> Self {
        self.optimizer = optimizer;
        self
    }

    pub fn build(
        &self,
        retrieved_context: &[RankedChunk],
        memories: &[LongTermMemory],
        conversation_history: &[Message],
    ) -> ChatRequest {
        let optimized = self
            .optimizer
            .optimize(retrieved_context, memories, conversation_history);
        self.build_from_optimized(optimized)
    }

    pub fn build_from_optimized(&self, optimized: OptimizedPromptContext) -> ChatRequest {
        let mut messages = Vec::new();
        messages.push(ChatMessage::system(self.system_prompt.clone()));

        let context = render_context_message(&optimized.retrieved_context, &optimized.memories);
        if !context.is_empty() {
            messages.push(ChatMessage::system(context));
        }

        messages.extend(
            optimized
                .conversation_history
                .into_iter()
                .map(chat_message_from_memory_message),
        );

        ChatRequest::new(self.model.clone(), messages)
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new(DEFAULT_MODEL, "You are a helpful assistant.")
    }
}

fn is_template_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("hbs" | "handlebars")
    )
}

fn file_stem(path: &Path) -> Result<String, ContextraError> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToString::to_string)
        .ok_or_else(|| {
            ContextraError::Validation(format!("invalid prompt template path '{}'", path.display()))
        })
}

fn parse_template_filename(path: &Path) -> Result<(String, String), ContextraError> {
    let stem = file_stem(path)?;
    let Some((name, version)) = stem.rsplit_once('@') else {
        return Err(ContextraError::Validation(format!(
            "prompt template '{}' must be named name@version.hbs",
            path.display()
        )));
    };

    if name.is_empty() || version.is_empty() {
        return Err(ContextraError::Validation(format!(
            "prompt template '{}' has an empty name or version",
            path.display()
        )));
    }

    Ok((name.to_string(), version.to_string()))
}

fn fits_budget(used: usize, next: usize, budget: usize) -> bool {
    used.saturating_add(next) <= budget
}

fn role_cost(role: &Role) -> usize {
    match role {
        Role::System => 4,
        Role::User | Role::Assistant => 3,
        Role::Tool => 5,
    }
}

fn chat_message_from_memory_message(message: Message) -> ChatMessage {
    match message.role {
        Role::System => ChatMessage::system(message.content),
        Role::User => ChatMessage::user(message.content),
        Role::Assistant => ChatMessage::assistant(message.content),
        Role::Tool => ChatMessage {
            role: ChatRole::Tool,
            content: Some(message.content),
            name: None,
            tool_call_id: message
                .metadata
                .get("tool_call_id")
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
            tool_calls: Vec::new(),
        },
    }
}

fn render_context_message(chunks: &[RankedChunk], memories: &[LongTermMemory]) -> String {
    let mut sections = Vec::new();
    if !chunks.is_empty() {
        let body = chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| format!("{}. {}", index + 1, chunk.content))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("Retrieved context:\n{body}"));
    }

    if !memories.is_empty() {
        let body = memories
            .iter()
            .enumerate()
            .map(|(index, memory)| format!("{}. {}", index + 1, memory.content))
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("Relevant memory:\n{body}"));
    }

    sections.join("\n\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;
    use types::{ConversationId, Metadata, UserId};
    use uuid::Uuid;

    #[test]
    fn template_registry_loads_versioned_templates_and_partials()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        let prompts_dir = dir.path();
        fs::create_dir(prompts_dir.join("partials"))?;
        fs::write(
            prompts_dir.join("partials").join("signature.hbs"),
            "Regards, {{name}}",
        )?;
        fs::write(
            prompts_dir.join("greeting@v1.hbs"),
            "Hello {{user}}. {{> signature name=sender}}",
        )?;

        let registry = PromptTemplateRegistry::load_from_dir(prompts_dir)?;
        let rendered = registry.render(
            "greeting",
            "v1",
            &json!({
                "user": "Soumya",
                "sender": "Contextra"
            }),
        )?;

        assert!(registry.has_template("greeting", "v1"));
        assert_eq!(rendered, "Hello Soumya. Regards, Contextra");
        Ok(())
    }

    #[test]
    fn optimizer_prioritizes_ranked_chunks_and_recent_turns_under_budget() {
        let optimizer = PromptOptimizer::new(26);
        let user_id = UserId::new();
        let conversation_id = ConversationId::new();
        let chunks = vec![
            ranked_chunk(
                1,
                0.1,
                "low value context should be dropped because it is verbose, expensive, repetitive, low signal, and not useful enough for the final answer",
            ),
            ranked_chunk(2, 0.9, "high value"),
            ranked_chunk(3, 0.8, "second best"),
        ];
        let memories = vec![LongTermMemory::new(
            user_id,
            memory::LongTermMemoryKind::Preference,
            "prefers short answers",
            0.9,
            Metadata::new(),
        )];
        let turns = vec![
            message(
                conversation_id,
                Role::User,
                "older message dropped by budget",
            ),
            message(conversation_id, Role::Assistant, "recent answer"),
            message(conversation_id, Role::User, "latest question"),
        ];

        let optimized = optimizer.optimize(&chunks, &memories, &turns);

        assert_eq!(optimized.retrieved_context[0].content, "high value");
        assert_eq!(optimized.retrieved_context[1].content, "second best");
        assert!(
            optimized
                .retrieved_context
                .iter()
                .all(|chunk| !chunk.content.starts_with("low value context"))
        );
        assert_eq!(optimized.memories.len(), 1);
        assert_eq!(optimized.conversation_history.len(), 2);
        assert_eq!(optimized.conversation_history[0].content, "recent answer");
        assert_eq!(optimized.conversation_history[1].content, "latest question");
        assert!(optimized.token_count <= optimizer.token_budget());
    }

    #[test]
    fn prompt_builder_composes_provider_chat_request() {
        let builder = PromptBuilder::new("mock-model", "System base")
            .with_optimizer(PromptOptimizer::new(64));
        let conversation_id = ConversationId::new();
        let request = builder.build(
            &[ranked_chunk(1, 1.0, "retrieved material")],
            &[],
            &[message(conversation_id, Role::User, "hello")],
        );

        assert_eq!(request.model, "mock-model");
        assert_eq!(request.messages[0].role, ChatRole::System);
        assert_eq!(request.messages[1].role, ChatRole::System);
        assert_eq!(request.messages[2].role, ChatRole::User);
        assert!(
            request.messages[1]
                .content
                .as_deref()
                .unwrap()
                .contains("retrieved material")
        );
    }

    fn ranked_chunk(id: u128, score: f32, content: &str) -> RankedChunk {
        RankedChunk {
            id: Uuid::from_u128(id),
            score,
            content: content.to_string(),
            semantic_score: None,
            keyword_score: None,
            metadata_score: None,
            fusion_score: None,
            rerank_score: None,
            embedding: None,
            payload: Metadata::new(),
        }
    }

    fn message(conversation_id: ConversationId, role: Role, content: &str) -> Message {
        Message {
            id: Uuid::now_v7(),
            conversation_id,
            role,
            content: content.to_string(),
            metadata: Metadata::new(),
        }
    }
}
