//! Advanced Reasoning - Chain-of-Thought, Self-Reflection, and Multi-Step Reasoning
//!
//! This module provides sophisticated reasoning capabilities for the agent:
//! - Chain-of-Thought (CoT) generation and parsing
//! - Step-by-step reasoning with validation
//! - Self-reflection and error correction
//! - Reasoning trace analysis
//! - Alternative path exploration
//! - Confidence scoring per reasoning step

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::{debug, info, warn};

/// A single reasoning step in a chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Step index.
    pub index: usize,
    /// Step content/thought.
    pub thought: String,
    /// Supporting evidence or observations.
    pub evidence: Vec<String>,
    /// Confidence in this step (0.0 to 1.0).
    pub confidence: f32,
    /// Whether this step is a conclusion.
    pub is_conclusion: bool,
    /// Whether this step was verified.
    pub verified: bool,
    /// Alternative paths considered.
    pub alternatives: Vec<String>,
}

impl ReasoningStep {
    /// Create a new reasoning step.
    pub fn new(index: usize, thought: impl Into<String>) -> Self {
        Self {
            index,
            thought: thought.into(),
            evidence: Vec::new(),
            confidence: 0.5,
            is_conclusion: false,
            verified: false,
            alternatives: Vec::new(),
        }
    }

    /// Add evidence to this step.
    pub fn with_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence.push(evidence.into());
        self
    }

    /// Set confidence.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Mark as conclusion.
    pub fn as_conclusion(mut self) -> Self {
        self.is_conclusion = true;
        self
    }

    /// Mark as verified.
    pub fn verified(mut self) -> Self {
        self.verified = true;
        self
    }
}

/// Complete chain of reasoning.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReasoningChain {
    /// Steps in the chain.
    pub steps: Vec<ReasoningStep>,
    /// Final conclusion if reached.
    pub conclusion: Option<String>,
    /// Overall confidence.
    pub overall_confidence: f32,
    /// Whether chain is complete.
    pub is_complete: bool,
    /// Chain metadata.
    pub metadata: ChainMetadata,
}

impl ReasoningChain {
    /// Create new empty chain.
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            conclusion: None,
            overall_confidence: 0.0,
            is_complete: false,
            metadata: ChainMetadata::default(),
        }
    }

    /// Add a step to the chain.
    pub fn add_step(&mut self, mut step: ReasoningStep) {
        step.index = self.steps.len();
        self.steps.push(step);
        self.recalculate_confidence();
    }

    /// Finalize with a conclusion.
    pub fn conclude(mut self, conclusion: impl Into<String>) -> Self {
        self.conclusion = Some(conclusion.into());
        self.is_complete = true;
        self.recalculate_confidence();
        self
    }

    /// Recalculate overall confidence.
    fn recalculate_confidence(&mut self) {
        if self.steps.is_empty() {
            self.overall_confidence = 0.0;
            return;
        }

        // Weight later steps more heavily
        let total_weight: f32 = self.steps.iter().enumerate()
            .map(|(i, _)| (i + 1) as f32)
            .sum();
        
        let weighted_sum: f32 = self.steps.iter().enumerate()
            .map(|(i, s)| s.confidence * (i + 1) as f32)
            .sum();

        self.overall_confidence = weighted_sum / total_weight;
    }

    /// Validate the chain for logical consistency.
    pub fn validate(&self) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check for empty steps
        for step in &self.steps {
            if step.thought.trim().is_empty() {
                errors.push(format!("Step {} has empty thought", step.index));
            }
            if step.confidence < 0.3 {
                warnings.push(format!("Step {} has low confidence ({:.2})", step.index, step.confidence));
            }
        }

        // Check chain completeness
        if !self.is_complete && self.conclusion.is_none() {
            warnings.push("Chain is incomplete".to_string());
        }

        // Check overall confidence
        if self.overall_confidence < 0.5 {
            warnings.push(format!("Overall confidence is low ({:.2})", self.overall_confidence));
        }

        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    /// Get reasoning as a formatted string.
    pub fn format_reasoning(&self) -> String {
        let mut output = String::new();
        for step in &self.steps {
            output.push_str(&format!("Step {}:", step.index + 1));
            output.push_str(&format!("  Thought: {}\n", step.thought));
            if !step.evidence.is_empty() {
                output.push_str("  Evidence:\n");
                for ev in &step.evidence {
                    output.push_str(&format!("    - {}\n", ev));
                }
            }
            output.push_str(&format!("  Confidence: {:.2}\n", step.confidence));
            if step.verified {
                output.push_str("  [Verified]\n");
            }
            output.push('\n');
        }

        if let Some(ref conclusion) = self.conclusion {
            output.push_str(&format!("Conclusion: {}\n", conclusion));
        }

        output.push_str(&format!("Overall Confidence: {:.2}\n", self.overall_confidence));
        output
    }

    /// Get last step.
    pub fn last_step(&self) -> Option<&ReasoningStep> {
        self.steps.last()
    }

    /// Get step count.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }
}

/// Chain metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChainMetadata {
    /// Created at timestamp.
    pub created_at: Option<std::time::SystemTime>,
    /// Completed at timestamp.
    pub completed_at: Option<std::time::SystemTime>,
    /// Reasoning strategy used.
    pub strategy: String,
    /// Domain/topic.
    pub domain: Option<String>,
    /// Max steps allowed.
    pub max_steps: Option<usize>,
}

/// Validation result.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed.
    pub is_valid: bool,
    /// Validation errors.
    pub errors: Vec<String>,
    /// Validation warnings.
    pub warnings: Vec<String>,
}

/// Self-reflection on a reasoning step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reflection {
    /// What was examined.
    pub examined: String,
    /// Findings from reflection.
    pub findings: Vec<String>,
    /// Suggested corrections.
    pub corrections: Vec<String>,
    /// Condifence after reflection.
    pub confidence_after: f32,
    /// Whether step needs revision.
    pub needs_revision: bool,
}

impl Reflection {
    /// Create new reflection.
    pub fn new(examined: impl Into<String>) -> Self {
        Self {
            examined: examined.into(),
            findings: Vec::new(),
            corrections: Vec::new(),
            confidence_after: 0.5,
            needs_revision: false,
        }
    }

    /// Add a finding.
    pub fn with_finding(mut self, finding: impl Into<String>) -> Self {
        self.findings.push(finding.into());
        self
    }

    /// Suggest a correction.
    pub fn with_correction(mut self, correction: impl Into<String>) -> Self {
        self.corrections.push(correction.into());
        self.needs_revision = true;
        self
    }
}

/// Reasoning strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReasoningStrategy {
    /// Standard step-by-step.
    ChainOfThought,
    /// Tree of alternatives.
    TreeOfThoughts,
    /// Step back and abstract.
    StepBack,
    /// Verify each step.
    Verification,
    /// Explore and backtrack.
    StepWise,
    /// Minimal steps to answer.
    Direct,
}

impl std::fmt::Display for ReasoningStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChainOfThought => write!(f, "cot"),
            Self::TreeOfThoughts => write!(f, "tot"),
            Self::StepBack => write!(f, "step_back"),
            Self::Verification => write!(f, "verification"),
            Self::StepWise => write!(f, "step_wise"),
            Self::Direct => write!(f, "direct"),
        }
    }
}

/// Reasoning engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningConfig {
    /// Default strategy.
    pub default_strategy: ReasoningStrategy,
    /// Max steps allowed.
    pub max_steps: usize,
    /// Min confidence threshold.
    pub min_confidence: f32,
    /// Enable self-reflection.
    pub enable_reflection: bool,
    /// Reflection interval (steps).
    pub reflection_interval: usize,
    /// Max alternative paths.
    pub max_alternatives: usize,
    /// Timeout for reasoning.
    pub timeout_secs: u64,
}

impl Default for ReasoningConfig {
    fn default() -> Self {
        Self {
            default_strategy: ReasoningStrategy::ChainOfThought,
            max_steps: 10,
            min_confidence: 0.7,
            enable_reflection: true,
            reflection_interval: 3,
            max_alternatives: 3,
            timeout_secs: 60,
        }
    }
}

/// Reasoning engine.
pub struct ReasoningEngine {
    config: ReasoningConfig,
    /// Active chains.
    chains: VecDeque<ReasoningChain>,
    /// Chain history.
    history: Vec<ReasoningChain>,
}

impl ReasoningEngine {
    /// Create new reasoning engine.
    pub fn new(config: ReasoningConfig) -> Self {
        Self {
            config,
            chains: VecDeque::new(),
            history: Vec::new(),
        }
    }

    /// Start a new reasoning chain.
    pub fn start_chain(&mut self, strategy: Option<ReasoningStrategy>) -> ReasoningChain {
        let mut chain = ReasoningChain::new();
        chain.metadata.strategy = strategy.unwrap_or(self.config.default_strategy).to_string();
        chain.metadata.created_at = Some(std::time::SystemTime::now());
        chain.metadata.max_steps = Some(self.config.max_steps);
        chain
    }

    /// Add reasoning step with validation.
    pub fn add_reasoning_step(
        &self,
        chain: &mut ReasoningChain,
        thought: impl Into<String>,
        confidence: f32,
    ) -> Result<(), ReasoningError> {
        if chain.steps.len() >= self.config.max_steps {
            return Err(ReasoningError::MaxStepsExceeded);
        }

        let step = ReasoningStep::new(chain.steps.len(), thought)
            .with_confidence(confidence);
        
        chain.add_step(step);
        
        // Trigger reflection if enabled and at interval
        if self.config.enable_reflection && 
           chain.steps.len() % self.config.reflection_interval == 0 {
            // In real implementation, would trigger async reflection
            debug!("Reflection point reached at step {}", chain.steps.len());
        }

        Ok(())
    }

    /// Generate reasoning prompt for model.
    pub fn generate_reasoning_prompt(&self, query: &str, strategy: ReasoningStrategy) -> String {
        let base = format!("Question: {}\n\n", query);
        
        let reasoning_instruction = match strategy {
            ReasoningStrategy::ChainOfThought => {
                "Let's work through this step by step:\n\n".to_string()
            }
            ReasoningStrategy::TreeOfThoughts => {
                "Let's explore multiple approaches:\n".to_string()
            }
            ReasoningStrategy::StepBack => {
                "First, let's take a step back and consider the general principles:\n".to_string()
            }
            ReasoningStrategy::Verification => {
                "Let's verify each step carefully:\n".to_string()
            }
            ReasoningStrategy::StepWise => {
                "Let's proceed step by step with verification:\n".to_string()
            }
            ReasoningStrategy::Direct => {
                "Answer directly:\n".to_string()
            }
        };

        base + &reasoning_instruction
    }

    /// Parse CoT response into steps.
    pub fn parse_chain_of_thought(&self, response: &str) -> Vec<ReasoningStep> {
        let mut steps = Vec::new();
        let mut current_thought = String::new();
        
        for (_i, line) in response.lines().enumerate() {
            let trimmed = line.trim();
            
            // Detect step boundaries
            if trimmed.starts_with("Step") || 
               trimmed.starts_with("Firstly")
               || trimmed.starts_with("Therefore")
               || trimmed.starts_with("So,")
               || trimmed.starts_with("In conclusion")
               || trimmed.starts_with("Answer:")
            {
                if !current_thought.is_empty() {
                    steps.push(ReasoningStep::new(steps.len(), &current_thought));
                    current_thought.clear();
                }
            }
            
            // Accumulate thought content
            if !trimmed.is_empty() {
                if !current_thought.is_empty() {
                    current_thought.push(' ');
                }
                current_thought.push_str(trimmed);
            }
        }
        
        // Add final thought
        if !current_thought.is_empty() && current_thought.len() > 10 {
            steps.push(ReasoningStep::new(steps.len(), current_thought));
        }
        
        steps
    }

    /// Perform self-reflection on a chain.
    pub fn reflect(&self, chain: &ReasoningChain) -> Vec<Reflection> {
        let mut reflections = Vec::new();
        
        for step in &chain.steps {
            let mut reflection = Reflection::new(&step.thought);
            
            // Check for logical gaps
            if step.evidence.is_empty() && !step.is_conclusion {
                reflection = reflection.with_finding("Step lacks supporting evidence");
            }
            
            // Check confidence
            if step.confidence < self.config.min_confidence {
                reflection = reflection.with_correction(
                    format!("Consider revising - confidence is only {:.1}%", step.confidence * 100.0)
                );
            }
            
            // Check for alternatives
            if step.alternatives.is_empty() && step.index > 0 {
                reflection = reflection.with_finding("No alternative paths considered");
            }
            
            if !reflection.findings.is_empty() {
                reflections.push(reflection);
            }
        }
        
        reflections
    }

    /// Score reasoning quality.
    pub fn score_reasoning(&self, chain: &ReasoningChain) -> ReasoningScore {
        let mut score = ReasoningScore::default();
        
        // Step completeness
        if chain.is_complete {
            score.completeness = 1.0;
        }
        
        // Confidence score
        if chain.overall_confidence >= self.config.min_confidence {
            score.confidence_score = 1.0;
        } else {
            score.confidence_score = chain.overall_confidence / self.config.min_confidence;
        }
        
        // Evidence quality
        let total_evidence: usize = chain.steps.iter().map(|s| s.evidence.len()).sum();
        let avg_evidence = total_evidence as f32 / chain.steps.len().max(1) as f32;
        score.evidence_quality = (avg_evidence / 2.0).min(1.0);
        
        // Logical flow
        let validation = chain.validate();
        score.logical_flow = if validation.errors.is_empty() { 1.0 } else { 0.0 };
        
        // Calculate overall
        score.overall = (score.completeness * 0.3)
                      + (score.confidence_score * 0.3)
                      + (score.evidence_quality * 0.2)
                      + (score.logical_flow * 0.2);
        
        score
    }

    /// Get reasoning statistics.
    pub fn statistics(&self) -> ReasoningStats {
        ReasoningStats {
            active_chains: self.chains.len(),
            total_chains: self.history.len(),
            avg_steps: if self.history.is_empty() {
                0.0
            } else {
                self.history.iter().map(|c| c.steps.len()).sum::<usize>() as f32 
                    / self.history.len() as f32
            },
        }
    }

    /// Archive a completed chain.
    pub fn archive_chain(&mut self, chain: ReasoningChain) {
        let mut chain = chain;
        chain.metadata.completed_at = Some(std::time::SystemTime::now());
        self.history.push(chain);
        
        if self.history.len() > 100 {
            self.history.remove(0);
        }
    }
}

impl Default for ReasoningEngine {
    fn default() -> Self {
        Self::new(ReasoningConfig::default())
    }
}

/// Reasoning error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ReasoningError {
    #[error("Maximum reasoning steps exceeded")]
    MaxStepsExceeded,
    #[error("Confidence too low: {0}")]
    LowConfidence(f32),
    #[error("Reasoning timed out")]
    Timeout,
}

/// Reasoning quality score.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReasoningScore {
    /// Completeness (0.0 to 1.0).
    pub completeness: f32,
    /// Confidence score.
    pub confidence_score: f32,
    /// Evidence quality.
    pub evidence_quality: f32,
    /// Logical flow.
    pub logical_flow: f32,
    /// Overall score.
    pub overall: f32,
}

/// Statistics.
#[derive(Debug, Clone)]
pub struct ReasoningStats {
    /// Active chain count.
    pub active_chains: usize,
    /// Total chains processed.
    pub total_chains: usize,
    /// Average steps per chain.
    pub avg_steps: f32,
}

/// Alternative path in tree reasoning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningPath {
    /// Path steps.
    pub steps: Vec<ReasoningStep>,
    /// Path confidence.
    pub confidence: f32,
    /// Whether this is the best path.
    pub is_best: bool,
    /// Path evaluation score.
    pub evaluation: f32,
}

impl ReasoningPath {
    /// Create new path.
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            confidence: 0.5,
            is_best: false,
            evaluation: 0.0,
        }
    }

    /// Add step to path.
    pub fn add_step(&mut self, step: ReasoningStep) {
        self.steps.push(step);
        self.update_confidence();
    }

    /// Update path confidence.
    fn update_confidence(&mut self) {
        if self.steps.is_empty() {
            self.confidence = 0.0;
            return;
        }
        let sum: f32 = self.steps.iter().map(|s| s.confidence).sum();
        self.confidence = sum / self.steps.len() as f32;
    }
}

impl Default for ReasoningPath {
    fn default() -> Self {
        Self::new()
    }
}

/// Tree of thoughts reasoning structure.
#[derive(Debug, Clone, Default)]
pub struct TreeOfThoughts {
    /// All paths explored.
    pub paths: Vec<ReasoningPath>,
    /// Currently active path.
    pub active_path: Option<usize>,
    /// Best path index.
    pub best_path: Option<usize>,
}

impl TreeOfThoughts {
    /// Create new tree.
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            active_path: None,
            best_path: None,
        }
    }

    /// Start a new path.
    pub fn start_path(&mut self) -> usize {
        let index = self.paths.len();
        self.paths.push(ReasoningPath::new());
        self.active_path = Some(index);
        index
    }

    /// Branch from existing path.
    pub fn branch_from(&mut self, from_index: usize) -> Option<usize> {
        if from_index >= self.paths.len() {
            return None;
        }
        
        let new_path = ReasoningPath {
            steps: self.paths[from_index].steps.clone(),
            confidence: self.paths[from_index].confidence,
            is_best: false,
            evaluation: 0.0,
        };
        
        let new_index = self.paths.len();
        self.paths.push(new_path);
        self.active_path = Some(new_index);
        Some(new_index)
    }

    /// Evaluate all paths.
    pub fn evaluate_paths(&mut self) {
        // Find highest confidence path
        let best = self.paths.iter()
            .enumerate()
            .max_by(|a, b| a.1.confidence.partial_cmp(&b.1.confidence).unwrap_or(std::cmp::Ordering::Equal));
        
        if let Some((idx, _)) = best {
            self.best_path = Some(idx);
            // Mark best
            for (i, path) in self.paths.iter_mut().enumerate() {
                path.is_best = i == idx;
            }
        }
    }

    /// Get best path.
    pub fn get_best_path(&self) -> Option<&ReasoningPath> {
        self.best_path.and_then(|idx| self.paths.get(idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reasoning_step_creation() {
        let step = ReasoningStep::new(0, "Test thought")
            .with_confidence(0.8)
            .with_evidence("Evidence 1");
        
        assert_eq!(step.index, 0);
        assert_eq!(step.thought, "Test thought");
        assert!((step.confidence - 0.8).abs() < 0.01);
        assert_eq!(step.evidence.len(), 1);
    }

    #[test]
    fn test_chain_building() {
        let mut chain = ReasoningChain::new();
        
        chain.add_step(ReasoningStep::new(0, "Step 1").with_confidence(0.7));
        chain.add_step(ReasoningStep::new(1, "Step 2").with_confidence(0.8));
        
        assert_eq!(chain.steps.len(), 2);
        assert!(chain.overall_confidence > 0.0);
    }

    #[test]
    fn test_chain_conclusion() {
        let mut chain = ReasoningChain::new();
        chain.add_step(ReasoningStep::new(0, "Analysis").with_confidence(0.9));
        
        let chain = chain.conclude("Final answer");
        assert!(chain.is_complete);
        assert_eq!(chain.conclusion, Some("Final answer".to_string()));
    }

    #[test]
    fn test_chain_validation() {
        let mut chain = ReasoningChain::new();
        chain.add_step(ReasoningStep::new(0, "Valid step").with_confidence(0.7));
        
        let validation = chain.validate();
        assert!(validation.is_valid);
    }

    #[test]
    fn test_chain_validation_errors() {
        let mut chain = ReasoningChain::new();
        chain.add_step(ReasoningStep::new(0, "").with_confidence(0.7)); // Empty thought
        
        let validation = chain.validate();
        assert!(!validation.is_valid);
        assert!(!validation.errors.is_empty());
    }

    #[test]
    fn test_parse_chain_of_thought() {
        let engine = ReasoningEngine::default();
        let response = "Step 1: First think about this.\nStep 2: Then consider that.\nTherefore: Conclusion";
        
        let steps = engine.parse_chain_of_thought(response);
        assert!(!steps.is_empty());
    }

    #[test]
    fn test_reflection() {
        let reflection = Reflection::new("Test thought")
            .with_finding("Issue found")
            .with_correction("Fix this");
        
        assert_eq!(reflection.findings.len(), 1);
        assert_eq!(reflection.corrections.len(), 1);
        assert!(reflection.needs_revision);
    }

    #[test]
    fn test_reasoning_engine() {
        let mut engine = ReasoningEngine::default();
        let chain = engine.start_chain(None);
        
        assert!(chain.steps.is_empty());
        assert!(!chain.is_complete);
    }

    #[test]
    fn test_add_step_limit() {
        let config = ReasoningConfig {
            max_steps: 2,
            ..Default::default()
        };
        let engine = ReasoningEngine::new(config);
        let mut chain = ReasoningChain::new();
        
        // Add steps up to limit
        assert!(engine.add_reasoning_step(&mut chain, "Step 1", 0.5).is_ok());
        assert!(engine.add_reasoning_step(&mut chain, "Step 2", 0.6).is_ok());
        
        // This should fail
        let result = engine.add_reasoning_step(&mut chain, "Step 3", 0.7);
        assert!(result.is_err());
    }

    #[test]
    fn test_tree_of_thoughts() {
        let mut tree = TreeOfThoughts::new();
        
        let path1 = tree.start_path();
        tree.paths[path1].add_step(ReasoningStep::new(0, "Path 1 step 1"));
        tree.paths[path1].add_step(ReasoningStep::new(1, "Path 1 step 2"));
        
        let path2 = tree.branch_from(path1).unwrap();
        tree.paths[path2].add_step(ReasoningStep::new(2, "Path 2 step"));
        
        tree.evaluate_paths();
        
        assert!(tree.get_best_path().is_some());
    }

    #[test]
    fn test_reasoning_score() {
        let engine = ReasoningEngine::default();
        let mut chain = ReasoningChain::new();
        chain.add_step(ReasoningStep::new(0, "Step 1").with_confidence(0.9).with_evidence("E1"));
        chain.add_step(ReasoningStep::new(1, "Step 2").with_confidence(0.8).with_evidence("E2"));
        let chain = chain.conclude("Done");
        
        let score = engine.score_reasoning(&chain);
        assert!(score.overall > 0.0);
    }

    #[test]
    fn test_reasoning_prompt_generation() {
        let engine = ReasoningEngine::default();
        let prompt = engine.generate_reasoning_prompt("What is 2+2?", ReasoningStrategy::ChainOfThought);
        
        assert!(prompt.contains("2+2"));
        assert!(prompt.contains("step by step"));
    }

}
