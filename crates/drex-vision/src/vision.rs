//! Vision Model - Process images through vision-capable models
//!
//! Provides abstraction for vision model backends like:
//! - GPT-4 Vision
//! - Claude Vision
//! - Local multimodal models (LLaVA, etc.)
//!
//! The vision model describes what's visible on screen,
//! identifying UI elements, text, and context.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

use crate::capture::{CaptureResult, CaptureError};

/// Vision model configuration.
#[derive(Debug, Clone)]
pub struct VisionConfig {
    /// Model provider (e.g., "openai", "anthropic", "local").
    pub provider: String,
    /// Model name.
    pub model_name: String,
    /// API endpoint for local models.
    pub endpoint: Option<String>,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
    /// Maximum tokens for response.
    pub max_tokens: u32,
    /// Temperature (0.0 to 2.0).
    pub temperature: f32,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            provider: "placeholder".to_string(),
            model_name: "vision".to_string(),
            endpoint: None,
            timeout_secs: 30,
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

/// Description of a UI element found in the image.
#[derive(Debug, Clone)]
pub struct ElementDescription {
    /// Element type (button, link, text, image, etc.).
    pub element_type: String,
    /// Element text content.
    pub text: Option<String>,
    /// Normalized coordinates (0.0 to 1.0, relative to image).
    pub x: f32,
    pub y: f32,
    /// Width and height (normalized 0.0 to 1.0).
    pub width: f32,
    pub height: f32,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
}

/// Vision processing result.
#[derive(Debug, Clone)]
pub struct VisionResult {
    /// Natural language description of what is visible.
    pub description: String,
    /// List of UI elements found.
    pub elements: Vec<ElementDescription>,
    /// Any text extracted via OCR.
    pub extracted_text: Option<String>,
    /// Processing time in milliseconds.
    pub duration_ms: u64,
}

/// Errors that can occur during vision processing.
#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    /// Vision model not available.
    #[error("Vision model not available: {0}")]
    NotAvailable(String),

    /// Invalid image format.
    #[error("Invalid image format")]
    InvalidImage,

    /// Image too large.
    #[error("Image too large: {0}x{1}")]
    ImageTooLarge(u32, u32),

    /// API error.
    #[error("API error: {0}")]
    ApiError(String),

    /// Timeout.
    #[error("Vision request timed out after {0}s")]
    Timeout(u64),

    /// Rate limited.
    #[error("Rate limited")]
    RateLimited,

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Capture error.
    #[error("Capture error: {0}")]
    CaptureError(#[from] CaptureError),
}

/// Vision model trait.
#[async_trait::async_trait]
pub trait VisionModel: Send + Sync {
    /// Process an image and return a description.
    async fn describe(&self, image: &CaptureResult) -> Result<VisionResult, VisionError>;

    /// Process an image file and return a description.
    async fn describe_file(&self, path: PathBuf) -> Result<VisionResult, VisionError>;

    /// Find elements matching a description.
    async fn find_element(
        &self,
        image: &CaptureResult,
        description: &str,
    ) -> Result<ElementDescription, VisionError>;

    /// Check if the model is ready.
    fn is_ready(&self) -> bool;

    /// Get model information.
    fn model_info(&self) -> (String, String);
}

/// Placeholder vision model.
pub struct PlaceholderVisionModel {
    config: VisionConfig,
}

impl PlaceholderVisionModel {
    /// Create a new placeholder vision model.
    pub fn new(config: VisionConfig) -> Result<Self, VisionError> {
        info!("Creating placeholder vision model");
        Ok(Self { config })
    }

    /// Get sample elements for testing.
    fn get_sample_elements(&self) -> Vec<ElementDescription> {
        vec![
            ElementDescription {
                element_type: "button".to_string(),
                text: Some("OK".to_string()),
                x: 0.5,
                y: 0.5,
                width: 0.1,
                height: 0.05,
                confidence: 0.95,
            },
            ElementDescription {
                element_type: "text_field".to_string(),
                text: None,
                x: 0.3,
                y: 0.4,
                width: 0.4,
                height: 0.08,
                confidence: 0.9,
            },
        ]
    }
}

#[async_trait::async_trait]
impl VisionModel for PlaceholderVisionModel {
    async fn describe(&self, image: &CaptureResult) -> Result<VisionResult, VisionError> {
        let start = Instant::now();

        debug!("Processing image: {}x{}", image.width, image.height);

        // Placeholder response
        let description = format!(
            "I can see a screen capture showing various UI elements. \
             The image is {}x{} pixels. This is a placeholder description \
             as full vision model integration is not yet implemented.",
            image.width, image.height
        );

        let elements = self.get_sample_elements();

        Ok(VisionResult {
            description,
            elements,
            extracted_text: Some("Sample extracted text".to_string()),
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn describe_file(&self, path: PathBuf) -> Result<VisionResult, VisionError> {
        debug!("Loading image from: {:?}", path);

        // In real implementation, would load and process file
        let data = tokio::fs::read(&path).await?;

        let capture = CaptureResult {
            data,
            format: "png".to_string(),
            width: 1920,
            height: 1080,
            timestamp: Instant::now(),
            region: crate::capture::CaptureRegion::Display { id: 0 },
        };

        self.describe(&capture).await
    }

    async fn find_element(
        &self,
        _image: &CaptureResult,
        description: &str,
    ) -> Result<ElementDescription, VisionError> {
        debug!("Looking for element: {}", description);

        // Placeholder - always returns the OK button
        Ok(ElementDescription {
            element_type: "button".to_string(),
            text: Some("OK".to_string()),
            x: 0.5,
            y: 0.5,
            width: 0.1,
            height: 0.05,
            confidence: 0.8,
        })
    }

    fn is_ready(&self) -> bool {
        false // Placeholder not actually ready
    }

    fn model_info(&self) -> (String, String) {
        (self.config.provider.clone(), self.config.model_name.clone())
    }
}

/// Type alias for vision model.
pub type BoxedVisionModel = Arc<dyn VisionModel>;

/// Create a default vision model.
pub fn create_vision_model(config: VisionConfig) -> Result<BoxedVisionModel, VisionError> {
    let model = PlaceholderVisionModel::new(config)?;
    Ok(Arc::new(model))
}

/// Convert normalized coordinates to absolute pixel coordinates.
pub fn normalize_to_pixels(
    x: f32,
    y: f32,
    image_width: u32,
    image_height: u32,
) -> (u32, u32) {
    let px = (x * image_width as f32).clamp(0.0, image_width as f32 - 1.0) as u32;
    let py = (y * image_height as f32).clamp(0.0, image_height as f32 - 1.0) as u32;
    (px, py)
}

/// Convert pixel coordinates to normalized (0.0 to 1.0).
pub fn pixels_to_normalized(
    x: u32,
    y: u32,
    image_width: u32,
    image_height: u32,
) -> (f32, f32) {
    let nx = (x as f32 / image_width as f32).clamp(0.0, 1.0);
    let ny = (y as f32 / image_height as f32).clamp(0.0, 1.0);
    (nx, ny)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vision_config_default() {
        let config = VisionConfig::default();
        assert_eq!(config.provider, "placeholder");
        assert_eq!(config.timeout_secs, 30);
    }

    #[tokio::test]
    async fn test_placeholder_describe() {
        let config = VisionConfig::default();
        let model = PlaceholderVisionModel::new(config).unwrap();

        let capture = CaptureResult {
            data: vec![],
            format: "png".to_string(),
            width: 1920,
            height: 1080,
            timestamp: Instant::now(),
            region: crate::capture::CaptureRegion::Display { id: 0 },
        };

        let result = model.describe(&capture).await.unwrap();
        assert!(!result.description.is_empty());
        assert!(!result.elements.is_empty());
    }

    #[tokio::test]
    async fn test_find_element() {
        let config = VisionConfig::default();
        let model = PlaceholderVisionModel::new(config).unwrap();

        let capture = CaptureResult {
            data: vec![],
            format: "png".to_string(),
            width: 1920,
            height: 1080,
            timestamp: Instant::now(),
            region: crate::capture::CaptureRegion::Display { id: 0 },
        };

        let element = model.find_element(&capture, "OK button").await.unwrap();
        assert_eq!(element.element_type, "button");
    }

    #[test]
    fn test_normalize_to_pixels() {
        let (x, y) = normalize_to_pixels(0.5, 0.5, 1920, 1080);
        assert_eq!(x, 960);
        assert_eq!(y, 540);
    }

    #[test]
    fn test_pixels_to_normalized() {
        let (nx, ny) = pixels_to_normalized(960, 540, 1920, 1080);
        assert_eq!(nx, 0.5);
        assert_eq!(ny, 0.5);
    }

    #[test]
    fn test_create_vision_model() {
        let config = VisionConfig::default();
        let model = create_vision_model(config).unwrap();
        assert!(!model.is_ready());
        let (provider, name) = model.model_info();
        assert_eq!(provider, "placeholder");
    }
}
