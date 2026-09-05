//! Backend capability definitions

use serde::{Deserialize, Serialize};

/// Capabilities that a model backend may support.
///
/// These capabilities describe what features are available on a backend.
/// Callers can query capabilities to adapt their behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendCapability {
    /// Basic text generation.
    TextGeneration,

    /// Streaming responses (chunked, real-time).
    Streaming,

    /// Tool/function calling.
    ToolCalling,

    /// Function calling with streaming.
    StreamingToolCalls,

    /// JSON mode / structured output.
    JsonMode,

    /// Multimodal input (images, audio).
    Vision,

    /// Image generation (for models that support it).
    ImageGeneration,

    /// Reasoning/thinking mode.
    Reasoning,

    /// Extended context window (> 100k tokens).
    ExtendedContext,

    /// System/few-shot prompting.
    SystemPrompt,

    /// Stop sequences.
    StopSequences,

    /// Frequency/presence penalties.
    LogitBias,

    /// Seed for reproducibility.
    SeedControl,

    /// Parallel tool calls (calling multiple tools at once).
    ParallelToolCalls,

    /// Response logprobs.
    Logprobs,
}

impl BackendCapability {
    /// Get a human-readable description of this capability.
    pub fn description(&self) -> &'static str {
        match self {
            Self::TextGeneration => "Generate text responses",
            Self::Streaming => "Stream partial responses as they're generated",
            Self::ToolCalling => "Call functions/tools as part of response",
            Self::StreamingToolCalls => "Stream tool calls in real-time",
            Self::JsonMode => "Generate valid JSON output",
            Self::Vision => "Accept image inputs",
            Self::ImageGeneration => "Generate images from text",
            Self::Reasoning => "Show reasoning/thinking process",
            Self::ExtendedContext => "Support very long context windows",
            Self::SystemPrompt => "Accept system/few-shot prompts",
            Self::StopSequences => "Stop generation at specified sequences",
            Self::LogitBias => "Bias token selection probabilities",
            Self::SeedControl => "Set random seed for reproducibility",
            Self::ParallelToolCalls => "Call multiple tools in parallel",
            Self::Logprobs => "Return token log probabilities",
        }
    }

    /// Check if this capability is considered fundamental (most providers support it).
    pub fn is_fundamental(&self) -> bool {
        matches!(
            self,
            Self::TextGeneration | Self::SystemPrompt | Self::StopSequences
        )
    }

    /// Check if this capability requires special handling.
    pub fn is_advanced(&self) -> bool {
        matches!(
            self,
            Self::StreamingToolCalls
                | Self::JsonMode
                | Self::Vision
                | Self::ImageGeneration
                | Self::Reasoning
                | Self::ParallelToolCalls
                | Self::Logprobs
        )
    }
}

impl std::fmt::Display for BackendCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TextGeneration => write!(f, "text_generation"),
            Self::Streaming => write!(f, "streaming"),
            Self::ToolCalling => write!(f, "tool_calling"),
            Self::StreamingToolCalls => write!(f, "streaming_tool_calls"),
            Self::JsonMode => write!(f, "json_mode"),
            Self::Vision => write!(f, "vision"),
            Self::ImageGeneration => write!(f, "image_generation"),
            Self::Reasoning => write!(f, "reasoning"),
            Self::ExtendedContext => write!(f, "extended_context"),
            Self::SystemPrompt => write!(f, "system_prompt"),
            Self::StopSequences => write!(f, "stop_sequences"),
            Self::LogitBias => write!(f, "logit_bias"),
            Self::SeedControl => write!(f, "seed_control"),
            Self::ParallelToolCalls => write!(f, "parallel_tool_calls"),
            Self::Logprobs => write!(f, "logprobs"),
        }
    }
}

/// A set of backend capabilities.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    capabilities: Vec<BackendCapability>,
}

impl CapabilitySet {
    /// Create a new capability set from a list.
    pub fn new(capabilities: Vec<BackendCapability>) -> Self {
        Self { capabilities }
    }

    /// Empty capability set.
    pub fn empty() -> Self {
        Self::default()
    }

    /// All capabilities.
    pub fn all() -> Self {
        Self::new(vec![
            BackendCapability::TextGeneration,
            BackendCapability::Streaming,
            BackendCapability::ToolCalling,
            BackendCapability::StreamingToolCalls,
            BackendCapability::JsonMode,
            BackendCapability::Vision,
            BackendCapability::ImageGeneration,
            BackendCapability::Reasoning,
            BackendCapability::ExtendedContext,
            BackendCapability::SystemPrompt,
            BackendCapability::StopSequences,
            BackendCapability::LogitBias,
            BackendCapability::SeedControl,
            BackendCapability::ParallelToolCalls,
            BackendCapability::Logprobs,
        ])
    }

    /// Fundamental capabilities (text generation basics).
    pub fn fundamental() -> Self {
        Self::new(vec![
            BackendCapability::TextGeneration,
            BackendCapability::SystemPrompt,
            BackendCapability::StopSequences,
        ])
    }

    /// Check if a capability is present.
    pub fn has(&self, capability: BackendCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Check if all specified capabilities are present.
    pub fn has_all(&self, capabilities: &[BackendCapability]) -> bool {
        capabilities.iter().all(|c| self.has(*c))
    }

    /// Check if any of the specified capabilities is present.
    pub fn has_any(&self, capabilities: &[BackendCapability]) -> bool {
        capabilities.iter().any(|c| self.has(*c))
    }

    /// Add a capability.
    pub fn add(&mut self, capability: BackendCapability) {
        if !self.has(capability) {
            self.capabilities.push(capability);
        }
    }

    /// Remove a capability.
    pub fn remove(&mut self, capability: BackendCapability) {
        self.capabilities.retain(|c| *c != capability);
    }

    /// List all capabilities.
    pub fn list(&self) -> &[BackendCapability] {
        &self.capabilities
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }

    /// Get capability count.
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// Find capabilities that are in other but not in self.
    pub fn missing(&self, other: &CapabilitySet) -> Vec<BackendCapability> {
        other
            .capabilities
            .iter()
            .filter(|c| !self.has(**c))
            .copied()
            .collect()
    }
}

impl From<Vec<BackendCapability>> for CapabilitySet {
    fn from(capabilities: Vec<BackendCapability>) -> Self {
        Self::new(capabilities)
    }
}

impl From<&[BackendCapability]> for CapabilitySet {
    fn from(capabilities: &[BackendCapability]) -> Self {
        Self::new(capabilities.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_description() {
        assert!(!BackendCapability::TextGeneration.description().is_empty());
        assert!(!BackendCapability::Streaming.description().is_empty());
    }

    #[test]
    fn capability_is_fundamental() {
        assert!(BackendCapability::TextGeneration.is_fundamental());
        assert!(BackendCapability::SystemPrompt.is_fundamental());
        assert!(!BackendCapability::Vision.is_fundamental());
    }

    #[test]
    fn capability_is_advanced() {
        assert!(BackendCapability::Vision.is_advanced());
        assert!(BackendCapability::JsonMode.is_advanced());
        assert!(!BackendCapability::TextGeneration.is_advanced());
        assert!(!BackendCapability::SystemPrompt.is_advanced());
    }

    #[test]
    fn capability_display() {
        assert_eq!(
            BackendCapability::TextGeneration.to_string(),
            "text_generation"
        );
        assert_eq!(BackendCapability::Streaming.to_string(), "streaming");
    }

    #[test]
    fn capability_set_fundamental() {
        let set = CapabilitySet::fundamental();
        assert!(set.has(BackendCapability::TextGeneration));
        assert!(set.has(BackendCapability::SystemPrompt));
        assert!(!set.has(BackendCapability::Vision));
    }

    #[test]
    fn capability_set_has() {
        let set = CapabilitySet::new(vec![
            BackendCapability::TextGeneration,
            BackendCapability::Streaming,
        ]);

        assert!(set.has(BackendCapability::TextGeneration));
        assert!(set.has(BackendCapability::Streaming));
        assert!(!set.has(BackendCapability::Vision));
    }

    #[test]
    fn capability_set_has_all() {
        let set = CapabilitySet::new(vec![
            BackendCapability::TextGeneration,
            BackendCapability::Streaming,
        ]);

        assert!(set.has_all(&[
            BackendCapability::TextGeneration,
            BackendCapability::Streaming
        ]));
        assert!(!set.has_all(&[
            BackendCapability::TextGeneration,
            BackendCapability::Vision
        ]));
    }

    #[test]
    fn capability_set_has_any() {
        let set = CapabilitySet::new(vec![BackendCapability::TextGeneration]);

        assert!(set.has_any(&[BackendCapability::TextGeneration, BackendCapability::Vision]));
        assert!(!set.has_any(&[BackendCapability::Vision, BackendCapability::Streaming]));
    }

    #[test]
    fn capability_set_add_remove() {
        let mut set = CapabilitySet::empty();
        set.add(BackendCapability::Vision);
        assert!(set.has(BackendCapability::Vision));

        set.remove(BackendCapability::Vision);
        assert!(!set.has(BackendCapability::Vision));
    }

    #[test]
    fn capability_set_missing() {
        let a = CapabilitySet::new(vec![BackendCapability::TextGeneration]);
        let b = CapabilitySet::new(vec![
            BackendCapability::TextGeneration,
            BackendCapability::Vision,
        ]);

        let missing = a.missing(&b);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], BackendCapability::Vision);
    }

    #[test]
    fn capability_set_from_vec() {
        let set: CapabilitySet = vec![BackendCapability::Streaming].into();
        assert!(set.has(BackendCapability::Streaming));
    }

    #[test]
    fn capability_serialization() {
        let cap = BackendCapability::ToolCalling;
        let json = serde_json::to_string(&cap).unwrap();
        assert!(json.contains("tool_calling"));

        let deserialized: BackendCapability = serde_json::from_str(&json).unwrap();
        assert_eq!(cap, deserialized);
    }
}
