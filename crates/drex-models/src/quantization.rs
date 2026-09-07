//! Model Quantization - Optimize models for faster inference and lower memory
//!
//! This module provides quantization support for model inference:
//! - 8-bit (INT8) quantization for reduced memory footprint
//! - 4-bit quantization for edge deployment
//! - Dynamic quantization based on model usage patterns
//! - Calibration data management for accuracy preservation
//! - Quantization-aware model selection

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Quantization format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuantizationFormat {
    /// No quantization (full precision FP32).
    None,
    /// Full precision FP16.
    Fp16,
    /// 8-bit integer quantization.
    Int8,
    /// 4-bit integer quantization (gguf Q4_0 style).
    Q4_0,
    /// 4-bit with higher accuracy (gguf Q4_K_M).
    Q4_K_M,
    /// 5-bit quantization (gguf Q5_K_M).
    Q5_K_M,
    /// 6-bit quantization (gguf Q6_K).
    Q6_K,
}

impl QuantizationFormat {
    /// Get bits per weight.
    pub fn bits_per_weight(&self) -> f32 {
        match self {
            Self::None => 32.0,
            Self::Fp16 => 16.0,
            Self::Int8 => 8.0,
            Self::Q4_0 => 4.0,
            Self::Q4_K_M => 4.5,
            Self::Q5_K_M => 5.5,
            Self::Q6_K => 6.0,
        }
    }

    /// Get compression ratio relative to FP32.
    pub fn compression_ratio(&self) -> f32 {
        32.0 / self.bits_per_weight()
    }

    /// Check if format supports GEMM acceleration.
    pub fn supports_acceleration(&self) -> bool {
        matches!(self, Self::Int8 | Self::Fp16)
    }

    /// Get recommended minimum model size (in MB) for this format.
    pub fn min_recommended_size_mb(&self) -> u32 {
        match self {
            Self::None => 0,
            Self::Fp16 => 100,
            Self::Int8 => 500,
            Self::Q4_0 | Self::Q4_K_M => 1000,
            Self::Q5_K_M => 2000,
            Self::Q6_K => 3000,
        }
    }

    /// Get accuracy degradation estimate (0.0 = no degradation).
    pub fn accuracy_degradation(&self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Fp16 => 0.001,
            Self::Int8 => 0.01,
            Self::Q4_0 => 0.03,
            Self::Q4_K_M => 0.02,
            Self::Q5_K_M => 0.015,
            Self::Q6_K => 0.01,
        }
    }
}

impl Default for QuantizationFormat {
    fn default() -> Self {
        Self::None
    }
}

impl std::fmt::Display for QuantizationFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "fp32"),
            Self::Fp16 => write!(f, "fp16"),
            Self::Int8 => write!(f, "int8"),
            Self::Q4_0 => write!(f, "q4_0"),
            Self::Q4_K_M => write!(f, "q4_k_m"),
            Self::Q5_K_M => write!(f, "q5_k_m"),
            Self::Q6_K => write!(f, "q6_k"),
        }
    }
}

impl std::str::FromStr for QuantizationFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fp32" | "none" | "f32" => Ok(Self::None),
            "fp16" | "f16" => Ok(Self::Fp16),
            "int8" | "q8_0" | "q8" => Ok(Self::Int8),
            "q4_0" => Ok(Self::Q4_0),
            "q4_k_m" | "q4km" => Ok(Self::Q4_K_M),
            "q5_k_m" | "q5km" => Ok(Self::Q5_K_M),
            "q6_k" | "q6k" => Ok(Self::Q6_K),
            _ => Err(format!("Unknown quantization format: {}", s)),
        }
    }
}

/// Quantization configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizationConfig {
    /// Target quantization format.
    pub format: QuantizationFormat,
    /// Calibration samples for accuracy preservation.
    pub calibration_samples: usize,
    /// Per-layer quantization (different layers may use different formats).
    pub per_layer: bool,
    /// Keep embeddings in higher precision.
    pub high_precision_embeddings: bool,
    /// Dynamic activation quantization.
    pub dynamic_activations: bool,
    /// Group size for block-wise quantization.
    pub group_size: usize,
}

impl Default for QuantizationConfig {
    fn default() -> Self {
        Self {
            format: QuantizationFormat::Q4_K_M,
            calibration_samples: 128,
            per_layer: true,
            high_precision_embeddings: true,
            dynamic_activations: true,
            group_size: 128,
        }
    }
}

impl QuantizationConfig {
    /// Create config for edge deployment (maximum compression).
    pub fn edge_optimized() -> Self {
        Self {
            format: QuantizationFormat::Q4_0,
            calibration_samples: 64,
            per_layer: false,
            high_precision_embeddings: true,
            dynamic_activations: false,
            group_size: 64,
        }
    }

    /// Create config for accuracy-critical scenarios.
    pub fn accuracy_optimized() -> Self {
        Self {
            format: QuantizationFormat::Int8,
            calibration_samples: 256,
            per_layer: true,
            high_precision_embeddings: true,
            dynamic_activations: true,
            group_size: 128,
        }
    }

    /// Estimate memory savings for a model size.
    pub fn estimate_memory_reduction(&self, original_size_mb: f32) -> MemoryEstimate {
        let compression = self.format.compression_ratio();
        let quantized_size = original_size_mb / compression;
        
        // Account for calibration data and overhead
        let overhead_mb = if self.per_layer { 10.0 } else { 5.0 };
        
        MemoryEstimate {
            original_mb: original_size_mb,
            quantized_mb: quantized_size + overhead_mb,
            reduction_percent: (1.0 - (quantized_size + overhead_mb) / original_size_mb) * 100.0,
        }
    }
}

/// Memory estimation result.
#[derive(Debug, Clone, Copy)]
pub struct MemoryEstimate {
    /// Original model size in MB.
    pub original_mb: f32,
    /// Quantized model size in MB.
    pub quantized_mb: f32,
    /// Percentage size reduction.
    pub reduction_percent: f32,
}

/// Model quantization manager.
pub struct QuantizationManager {
    config: QuantizationConfig,
    /// Cached calibration data per model.
    calibration_cache: HashMap<String, CalibrationData>,
    /// Performance metrics per quantization format.
    performance_data: HashMap<QuantizationFormat, PerformanceMetrics>,
}

impl QuantizationManager {
    /// Create new quantization manager.
    pub fn new(config: QuantizationConfig) -> Self {
        Self {
            config,
            calibration_cache: HashMap::new(),
            performance_data: HashMap::new(),
        }
    }

    /// Get optimal quantization format for a model.
    pub fn recommend_format(&self, model_size_mb: u32, use_case: UseCase) -> QuantizationFormat {
        match use_case {
            UseCase::EdgeDevice => QuantizationFormat::Q4_0,
            UseCase::Balanced => {
                if model_size_mb < 2000 {
                    QuantizationFormat::Q5_K_M
                } else {
                    QuantizationFormat::Q4_K_M
                }
            }
            UseCase::AccuracyCritical => {
                if model_size_mb < 500 {
                    QuantizationFormat::Fp16
                } else {
                    QuantizationFormat::Int8
                }
            }
            UseCase::MaximumSpeed => QuantizationFormat::Int8,
        }
    }

    /// Select best available format based on hardware capabilities.
    pub fn select_for_hardware(
        &self,
        preferred: QuantizationFormat,
        has_int8_accel: bool,
        has_fp16_accel: bool,
    ) -> QuantizationFormat {
        if preferred.supports_acceleration() {
            // Preferred format supports acceleration - use it
            return preferred;
        }

        // Fall back to nearest accelerated format
        match (has_int8_accel, has_fp16_accel) {
            (true, _) => QuantizationFormat::Int8,
            (false, true) => QuantizationFormat::Fp16,
            (false, false) => preferred, // CPU only - use preferred
        }
    }

    /// Estimate inference speedup from quantization.
    pub fn estimate_speedup(&self, format: QuantizationFormat) -> f32 {
        let base_metrics = self.performance_data.get(&QuantizationFormat::None);
        let quantized_metrics = self.performance_data.get(&format);

        match (base_metrics, quantized_metrics) {
            (Some(base), Some(q)) => base.tokens_per_sec / q.tokens_per_sec,
            _ => format.compression_ratio().sqrt(), // Rough estimate
        }
    }

    /// Clear calibration cache.
    pub fn clear_cache(&mut self) {
        self.calibration_cache.clear();
    }
}

/// Calibration data for quantization.
#[derive(Debug, Clone)]
pub struct CalibrationData {
    /// Model identifier.
    pub model_id: String,
    /// Calibration samples (input, output pairs).
    pub samples: Vec<(String, String)>,
    /// Layer-wise activation ranges.
    pub activation_ranges: HashMap<String, (f32, f32)>,
    /// Computed at.
    pub computed_at: std::time::SystemTime,
}

/// Performance metrics for a quantization format.
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Tokens per second.
    pub tokens_per_sec: f32,
    /// Memory usage in MB.
    pub memory_mb: f32,
    /// Accuracy on benchmark ( perplexity or similar).
    pub accuracy_score: f32,
}

/// Use case for quantization recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseCase {
    /// Edge device with limited resources.
    EdgeDevice,
    /// Balanced quality and performance.
    Balanced,
    /// Accuracy is critical.
    AccuracyCritical,
    /// Maximum inference speed.
    MaximumSpeed,
}

/// Quantized model manifest for model registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedModel {
    /// Original model ID.
    pub original_model_id: String,
    /// Quantization format.
    pub format: QuantizationFormat,
    /// File path or URI.
    pub path: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Accuracy metrics.
    pub accuracy: Option<ModelAccuracy>,
    /// Creation timestamp.
    pub created_at: std::time::SystemTime,
}

/// Accuracy metrics for quantized model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAccuracy {
    /// Perplexity (lower is better).
    pub perplexity: Option<f32>,
    /// Benchmark score (higher is better).
    pub benchmark_score: Option<f32>,
    /// Comparison to full precision (1.0 = same, 0.9 = 10% worse).
    pub relative_to_fp32: f32,
}

/// Quantized model registry.
pub struct QuantizedModelRegistry {
    models: HashMap<String, Vec<QuantizedModel>>,
}

impl QuantizedModelRegistry {
    /// Create new registry.
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }

    /// Register a quantized model.
    pub fn register(&mut self, model: QuantizedModel) {
        self.models
            .entry(model.original_model_id.clone())
            .or_default()
            .push(model);
    }

    /// Find best quantized version for requirements.
    pub fn find_best(
        &self,
        original_model_id: &str,
        max_degradation: f32,
        target_format: Option<QuantizationFormat>,
    ) -> Option<&QuantizedModel> {
        let variants = self.models.get(original_model_id)?;

        variants
            .iter()
            .filter(|m| {
                if let Some(ref accuracy) = m.accuracy {
                    accuracy.relative_to_fp32 >= 1.0 - max_degradation
                } else {
                    true // No accuracy data, assume acceptable
                }
            })
            .filter(|m| target_format.map_or(true, |f| m.format == f))
            .min_by_key(|m| m.size_bytes)
    }

    /// List available formats for a model.
    pub fn available_formats(&self, original_model_id: &str) -> Vec<QuantizationFormat> {
        self.models
            .get(original_model_id)
            .map(|variants| {
                variants
                    .iter()
                    .map(|m| m.format)
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Default for QuantizedModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse quantization level from model filename (e.g., "model-q4_k_m.gguf").
pub fn parse_quantization_from_filename(filename: &str) -> Option<QuantizationFormat> {
    let lower = filename.to_lowercase();
    
    // Check for GGUF quantization suffixes
    if lower.contains("q4_0") {
        Some(QuantizationFormat::Q4_0)
    } else if lower.contains("q4_k_m") {
        Some(QuantizationFormat::Q4_K_M)
    } else if lower.contains("q4_k") {
        Some(QuantizationFormat::Q4_K_M)
    } else if lower.contains("q5_k_m") {
        Some(QuantizationFormat::Q5_K_M)
    } else if lower.contains("q5_k") {
        Some(QuantizationFormat::Q5_K_M)
    } else if lower.contains("q6_k") {
        Some(QuantizationFormat::Q6_K)
    } else if lower.contains("q8_0") || lower.contains("int8") {
        Some(QuantizationFormat::Int8)
    } else if lower.contains("fp16") {
        Some(QuantizationFormat::Fp16)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantization_format_bits() {
        assert_eq!(QuantizationFormat::None.bits_per_weight(), 32.0);
        assert_eq!(QuantizationFormat::Fp16.bits_per_weight(), 16.0);
        assert_eq!(QuantizationFormat::Int8.bits_per_weight(), 8.0);
        assert_eq!(QuantizationFormat::Q4_0.bits_per_weight(), 4.0);
    }

    #[test]
    fn test_compression_ratio() {
        assert_eq!(QuantizationFormat::Int8.compression_ratio(), 4.0);
        assert_eq!(QuantizationFormat::Q4_0.compression_ratio(), 8.0);
        assert_eq!(QuantizationFormat::Fp16.compression_ratio(), 2.0);
    }

    #[test]
    fn test_format_parsing() {
        assert_eq!(
            "q4_k_m".parse::<QuantizationFormat>().unwrap(),
            QuantizationFormat::Q4_K_M
        );
        assert_eq!(
            "INT8".parse::<QuantizationFormat>().unwrap(),
            QuantizationFormat::Int8
        );
        assert!("unknown".parse::<QuantizationFormat>().is_err());
    }

    #[test]
    fn test_memory_estimate() {
        let config = QuantizationConfig::default();
        let estimate = config.estimate_memory_reduction(1000.0);
        
        assert!(estimate.quantized_mb < estimate.original_mb);
        assert!(estimate.reduction_percent > 0.0);
    }

    #[test]
    fn test_recommend_format() {
        let manager = QuantizationManager::new(QuantizationConfig::default());
        
        let edge = manager.recommend_format(1000, UseCase::EdgeDevice);
        assert_eq!(edge, QuantizationFormat::Q4_0);
        
        let accuracy = manager.recommend_format(1000, UseCase::AccuracyCritical);
        assert_eq!(accuracy, QuantizationFormat::Int8);
    }

    #[test]
    fn test_registry() {
        let mut registry = QuantizedModelRegistry::new();
        let model = QuantizedModel {
            original_model_id: "llama-7b".to_string(),
            format: QuantizationFormat::Q4_K_M,
            path: "/models/llama-7b-q4.gguf".to_string(),
            size_bytes: 3_800_000_000,
            accuracy: Some(ModelAccuracy {
                perplexity: Some(8.5),
                benchmark_score: Some(0.85),
                relative_to_fp32: 0.98,
            }),
            created_at: std::time::SystemTime::now(),
        };
        
        registry.register(model);
        
        let available = registry.available_formats("llama-7b");
        assert_eq!(available.len(), 1);
        
        let best = registry.find_best("llama-7b", 0.05, None);
        assert!(best.is_some());
    }

    #[test]
    fn test_parse_quantization_from_filename() {
        assert_eq!(
            parse_quantization_from_filename("model-q4_k_m.gguf"),
            Some(QuantizationFormat::Q4_K_M)
        );
        assert_eq!(
            parse_quantization_from_filename("llama-2-7b-q4_k_m.gguf"),
            Some(QuantizationFormat::Q4_K_M)
        );
        assert_eq!(
            parse_quantization_from_filename("model-Q8_0.gguf"),
            Some(QuantizationFormat::Int8)
        );
        assert_eq!(
            parse_quantization_from_filename("model-fp16.bin"),
            Some(QuantizationFormat::Fp16)
        );
        assert_eq!(
            parse_quantization_from_filename("model.bin"),
            None
        );
    }

    #[test]
    fn test_accuracy_degradation() {
        // Higher quantization should have less degradation
        assert!(
            QuantizationFormat::Fp16.accuracy_degradation() < 
            QuantizationFormat::Q4_0.accuracy_degradation()
        );
    }
}
