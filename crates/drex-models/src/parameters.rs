//! Generation parameters for model backends

use serde::{Deserialize, Serialize};

/// Generation parameters for controlling model behavior.
///
/// This struct encapsulates all the common generation parameters
/// that have clear semantics across multiple backends (OpenAI, Anthropic, etc.).
/// Provider-specific parameters should be handled by individual backend implementations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationParameters {
    /// Sampling temperature (0.0 to 2.0).
    /// Higher values make output more random, lower values more deterministic.
    /// Default: None (uses backend default, typically 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Nucleus sampling parameter (0.0 to 1.0).
    /// Only sample from tokens comprising the top_p probability mass.
    /// Lower values make output more focused, higher values more diverse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// Maximum number of tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Maximum completion length as a percentage of context.
    /// Some backends support this instead of absolute token counts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_ratio: Option<f32>,

    /// Penalize repeated tokens based on frequency.
    /// > 1.0 reduces repetition, < 0.0 encourages repetition, 0.0 is neutral.
    /// Range: -2.0 to 2.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,

    /// Penalize repeated tokens based on presence.
    /// > 0.0 encourages generating new tokens, 0.0 is neutral.
    /// Range: -2.0 to 2.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,

    /// Sequences that will stop generation when encountered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,

    /// Random seed for reproducible generation (if supported by backend).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,

    /// Number of responses to generate (for n-best sampling).
    /// Not all backends support this.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,

    /// Whether to stream the response.
    /// Note: This is often handled at the API level, not as a generation parameter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

impl GenerationParameters {
    /// Create default parameters.
    pub fn new() -> Self {
        Self {
            temperature: None,
            top_p: None,
            max_tokens: None,
            max_completion_ratio: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop_sequences: Vec::new(),
            seed: None,
            n: None,
            stream: None,
        }
    }

    /// Builder: Set temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature.clamp(0.0, 2.0));
        self
    }

    /// Builder: Set top_p.
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p.clamp(0.0, 1.0));
        self
    }

    /// Builder: Set max tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Builder: Set max completion ratio.
    pub fn with_max_completion_ratio(mut self, ratio: f32) -> Self {
        self.max_completion_ratio = Some(ratio.clamp(0.0, 1.0));
        self
    }

    /// Builder: Set frequency penalty.
    pub fn with_frequency_penalty(mut self, penalty: f32) -> Self {
        // Valid range typically -2.0 to 2.0
        self.frequency_penalty = Some(penalty.clamp(-2.0, 2.0));
        self
    }

    /// Builder: Set presence penalty.
    pub fn with_presence_penalty(mut self, penalty: f32) -> Self {
        // Valid range typically -2.0 to 2.0
        self.presence_penalty = Some(penalty.clamp(-2.0, 2.0));
        self
    }

    /// Builder: Add a stop sequence.
    pub fn with_stop_sequence<S: Into<String>>(mut self, sequence: S) -> Self {
        self.stop_sequences.push(sequence.into());
        self
    }

    /// Builder: Set stop sequences.
    pub fn with_stop_sequences(mut self, sequences: Vec<String>) -> Self {
        self.stop_sequences = sequences;
        self
    }

    /// Builder: Set seed.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Builder: Set n (number of completions).
    pub fn with_n(mut self, n: u32) -> Self {
        self.n = Some(n);
        self
    }

    /// Builder: Set stream flag.
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = Some(stream);
        self
    }

    /// Check if any parameters are set.
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.top_p.is_none()
            && self.max_tokens.is_none()
            && self.max_completion_ratio.is_none()
            && self.frequency_penalty.is_none()
            && self.presence_penalty.is_none()
            && self.stop_sequences.is_empty()
            && self.seed.is_none()
            && self.n.is_none()
            && self.stream.is_none()
    }

    /// Create parameters for deterministic output.
    pub fn deterministic() -> Self {
        Self::new().with_temperature(0.0)
    }

    /// Create parameters for creative output.
    pub fn creative() -> Self {
        Self::new()
            .with_temperature(0.9)
            .with_top_p(0.95)
            .with_frequency_penalty(0.2)
    }

    /// Create parameters for balanced output.
    pub fn balanced() -> Self {
        Self::new()
            .with_temperature(0.7)
            .with_top_p(0.9)
    }
}

impl Default for GenerationParameters {
    fn default() -> Self {
        Self::new()
    }
}

/// Reasonable default parameters for different use cases.
pub mod presets {
    use super::GenerationParameters;

    /// Parameters for deterministic, consistent output.
    /// Best for: classification, extraction, structured outputs.
    pub fn deterministic() -> GenerationParameters {
        GenerationParameters::deterministic()
    }

    /// Parameters for balanced creativity and coherence.
    /// Best for: general conversation, Q&A.
    pub fn balanced() -> GenerationParameters {
        GenerationParameters::balanced()
    }

    /// Parameters for creative, varied output.
    /// Best for: brainstorming, creative writing.
    pub fn creative() -> GenerationParameters {
        GenerationParameters::creative()
    }

    /// Parameters for code generation.
    pub fn code() -> GenerationParameters {
        GenerationParameters::new()
            .with_temperature(0.2)
            .with_max_tokens(2048)
    }

    /// Parameters for summarization.
    pub fn summarize() -> GenerationParameters {
        GenerationParameters::new()
            .with_temperature(0.3)
            .with_max_tokens(500)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A parameter validation error.
pub struct ParameterError {
    pub parameter: &'static str,
    pub reason: &'static str,
}

impl GenerationParameters {
    /// Validate the parameters and return any errors.
    pub fn validate(&self) -> Vec<ParameterError> {
        let mut errors = Vec::new();

        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                errors.push(ParameterError {
                    parameter: "temperature",
                    reason: "must be between 0.0 and 2.0",
                });
            }
        }

        if let Some(top_p) = self.top_p {
            if !(0.0..=1.0).contains(&top_p) {
                errors.push(ParameterError {
                    parameter: "top_p",
                    reason: "must be between 0.0 and 1.0",
                });
            }
        }

        if let Some(ratio) = self.max_completion_ratio {
            if !(0.0..=1.0).contains(&ratio) {
                errors.push(ParameterError {
                    parameter: "max_completion_ratio",
                    reason: "must be between 0.0 and 1.0",
                });
            }
        }

        if let Some(penalty) = self.frequency_penalty {
            if !(-2.0..=2.0).contains(&penalty) {
                errors.push(ParameterError {
                    parameter: "frequency_penalty",
                    reason: "must be between -2.0 and 2.0",
                });
            }
        }

        if let Some(penalty) = self.presence_penalty {
            if !(-2.0..=2.0).contains(&penalty) {
                errors.push(ParameterError {
                    parameter: "presence_penalty",
                    reason: "must be between -2.0 and 2.0",
                });
            }
        }

        errors
    }

    /// Check if parameters are valid.
    pub fn is_valid(&self) -> bool {
        self.validate().is_empty()
    }
}

impl std::fmt::Display for ParameterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.parameter, self.reason)
    }
}

impl std::error::Error for ParameterError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_parameters_builder() {
        let params = GenerationParameters::new()
            .with_temperature(0.8)
            .with_max_tokens(100)
            .with_stop_sequence("END")
            .with_seed(42);

        assert_eq!(params.temperature, Some(0.8));
        assert_eq!(params.max_tokens, Some(100));
        assert_eq!(params.stop_sequences, vec!["END"]);
        assert_eq!(params.seed, Some(42));
    }

    #[test]
    fn temperature_clamping() {
        let params = GenerationParameters::new().with_temperature(-0.5);
        assert_eq!(params.temperature, Some(0.0));

        let params = GenerationParameters::new().with_temperature(3.0);
        assert_eq!(params.temperature, Some(2.0));
    }

    #[test]
    fn top_p_clamping() {
        let params = GenerationParameters::new().with_top_p(-0.5);
        assert_eq!(params.top_p, Some(0.0));

        let params = GenerationParameters::new().with_top_p(1.5);
        assert_eq!(params.top_p, Some(1.0));
    }

    #[test]
    fn presets_deterministic() {
        let params = presets::deterministic();
        assert_eq!(params.temperature, Some(0.0));
    }

    #[test]
    fn presets_creative() {
        let params = presets::creative();
        assert!(params.temperature.unwrap() > 0.5);
        assert!(params.top_p.is_some());
    }

    #[test]
    fn is_empty() {
        let params = GenerationParameters::new();
        assert!(params.is_empty());

        let params = GenerationParameters::new().with_temperature(0.5);
        assert!(!params.is_empty());
    }

    #[test]
    fn validate_temperature() {
        let params = GenerationParameters::new().with_temperature(3.0);
        // Clamped to 2.0, so no validation error
        assert!(params.is_valid());

        // After clamping, value is valid
        assert_eq!(params.temperature, Some(2.0));
    }

    #[test]
    fn parameter_error_display() {
        let err = ParameterError {
            parameter: "temperature",
            reason: "must be between 0.0 and 2.0",
        };
        assert_eq!(err.to_string(), "temperature: must be between 0.0 and 2.0");
    }

    #[test]
    fn serialization_roundtrip() {
        let params = GenerationParameters::creative();
        let json = serde_json::to_string(&params).unwrap();
        let deserialized: GenerationParameters = serde_json::from_str(&json).unwrap();

        assert_eq!(params.temperature, deserialized.temperature);
        assert_eq!(params.top_p, deserialized.top_p);
        assert_eq!(params.max_tokens, deserialized.max_tokens);
    }
}
