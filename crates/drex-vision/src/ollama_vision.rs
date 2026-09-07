//! Ollama Vision Model - Integration with multimodal models via Ollama API
//!
//! Supports vision-capable models like:
//! - LLaVA (Large Language and Vision Assistant)
//! - BakLLaVA
//! - Moondream
//! - Llama 3.2 Vision
//!
//! These models can process images and provide:
//! - Natural language descriptions
//! - OCR capabilities
//! - UI element detection

use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::capture::CaptureResult;
use crate::vision::{ElementDescription, VisionConfig, VisionError, VisionModel, VisionResult};

/// Ollama vision model client.
pub struct OllamaVisionModel {
    config: VisionConfig,
    client: reqwest::Client,
    base_url: String,
}

impl OllamaVisionModel {
    /// Create a new Ollama vision model client.
    pub fn new(config: VisionConfig) -> Result<Self, VisionError> {
        let base_url = config
            .endpoint
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".to_string());

        info!("Creating Ollama vision model client for {}", config.model_name);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| VisionError::ApiError(e.to_string()))?;

        Ok(Self {
            config,
            client,
            base_url,
        })
    }

    /// Check if Ollama is available and the model is loaded.
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<serde_json::Value>().await {
                        Ok(body) => {
                            if let Some(models) = body.get("models").and_then(|m| m.as_array()) {
                                return models.iter().any(|m| {
                                    m.get("name")
                                        .and_then(|n| n.as_str())
                                        .map(|name| name.starts_with(&self.config.model_name))
                                        .unwrap_or(false)
                                });
                            }
                        }
                        Err(e) => warn!("Failed to parse Ollama response: {}", e),
                    }
                }
                false
            }
            Err(e) => {
                warn!("Failed to connect to Ollama: {}", e);
                false
            }
        }
    }

    /// Generate a description of the image using Ollama.
    async fn generate_description(&self, image_base64: &str) -> Result<String, VisionError> {
        let url = format!("{}/api/generate", self.base_url);

        let system_prompt = "You are a helpful assistant that describes images accurately. \
            Provide a concise description of what is visible in the image, \
            including any text, UI elements, buttons, and layout. \
            Be specific about locations when relevant.";

        let request_body = serde_json::json!({
            "model": self.config.model_name,
            "system": system_prompt,
            "prompt": "Describe this image in detail:",
            "stream": false,
            "images": [image_base64],
            "options": {
                "temperature": self.config.temperature,
                "num_predict": self.config.max_tokens,
            }
        });

        debug!("Sending vision request to Ollama");

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    VisionError::Timeout(self.config.timeout_secs)
                } else {
                    VisionError::ApiError(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_default();
            return Err(VisionError::ApiError(format!(
                "Ollama returned {}: {}",
                status, text
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| VisionError::ApiError(e.to_string()))?;

        let description = result
            .get("response")
            .and_then(|r| r.as_str())
            .ok_or_else(|| VisionError::ApiError("Missing response in Ollama output".to_string()))?;

        Ok(description.to_string())
    }

    /// Perform OCR by asking the vision model to extract text.
    async fn perform_ocr(&self, image_base64: &str) -> Result<String, VisionError> {
        let url = format!("{}/api/generate", self.base_url);

        let request_body = serde_json::json!({
            "model": self.config.model_name,
            "prompt": "Extract all the text visible in this image. Return only the text content, nothing else:",
            "stream": false,
            "images": [image_base64],
            "options": {
                "temperature": 0.1, // Low temperature for OCR
                "num_predict": 2000,
            }
        });

        debug!("Sending OCR request to Ollama");

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| VisionError::ApiError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(VisionError::ApiError(format!(
                "Ollama OCR returned {}: {}",
                status, text
            )));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| VisionError::ApiError(e.to_string()))?;

        let text = result
            .get("response")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        Ok(text)
    }

    /// Parse elements from description (basic heuristic approach).
    fn parse_elements(&self, description: &str) -> Vec<ElementDescription> {
        // This is a simplified element extraction based on keywords
        // In a more sophisticated implementation, you'd use:
        // - Model-specific prompting to get structured output
        // - Computer vision models for actual bounding box detection
        let mut elements = Vec::new();
        let lower_desc = description.to_lowercase();

        // Detect common UI elements
        if lower_desc.contains("button") || lower_desc.contains("click") {
            elements.push(ElementDescription {
                element_type: "button".to_string(),
                text: self.extract_button_text(description),
                x: 0.5,
                y: 0.5,
                width: 0.15,
                height: 0.08,
                confidence: 0.7,
            });
        }

        if lower_desc.contains("text field") || lower_desc.contains("input") {
            elements.push(ElementDescription {
                element_type: "text_field".to_string(),
                text: None,
                x: 0.3,
                y: 0.4,
                width: 0.4,
                height: 0.08,
                confidence: 0.6,
            });
        }

        if lower_desc.contains("menu") || lower_desc.contains("navigation") {
            elements.push(ElementDescription {
                element_type: "menu".to_string(),
                text: None,
                x: 0.1,
                y: 0.1,
                width: 0.8,
                height: 0.1,
                confidence: 0.65,
            });
        }

        elements
    }

    /// Try to extract button text from description using simple heuristics.
    fn extract_button_text(&self, description: &str) -> Option<String> {
        // Simple heuristic: look for text following "button" or "labeled"
        let lower = description.to_lowercase();
        
        // Check for "X button" pattern
        if let Some(idx) = lower.find(" button") {
            let before = &description[..idx];
            if let Some(word_start) = before.rsplit(' ').next() {
                let word: &str = word_start.trim_matches(|c: char| !c.is_alphanumeric());
                if !word.is_empty() && word.len() < 20 {
                    return Some(word.to_string());
                }
            }
        }
        
        // Check for common button labels
        let common_labels = ["OK", "Cancel", "Submit", "Save", "Close", "Next", "Back"];
        for label in &common_labels {
            if lower.contains(&label.to_lowercase()) {
                return Some(label.to_string());
            }
        }

        None
    }
}

#[async_trait::async_trait]
impl VisionModel for OllamaVisionModel {
    async fn describe(&self, image: &CaptureResult) -> Result<VisionResult, VisionError> {
        let start = Instant::now();

        debug!(
            "Processing image: {}x{} ({} bytes)",
            image.width,
            image.height,
            image.data.len()
        );

        // Convert image to base64
        let image_base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &image.data);

        // Generate description from Ollama
        let description = self.generate_description(&image_base64).await?;

        // Parse elements from description
        let elements = self.parse_elements(&description);

        // Perform OCR using Ollama
        let extracted_text = match self.perform_ocr(&image_base64).await {
            Ok(text) if !text.is_empty() => Some(text),
            _ => None,
        };

        let duration_ms = start.elapsed().as_millis() as u64;
        info!("Vision processing completed in {}ms", duration_ms);

        Ok(VisionResult {
            description,
            elements,
            extracted_text,
            duration_ms,
        })
    }

    async fn describe_file(&self, path: PathBuf) -> Result<VisionResult, VisionError> {
        debug!("Loading image from: {:?}", path);

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
        image: &CaptureResult,
        description: &str,
    ) -> Result<ElementDescription, VisionError> {
        debug!("Looking for element: {}", description);

        // Get full vision result
        let result = self.describe(image).await?;

        // Try to find matching element
        let search_lower = description.to_lowercase();
        for element in &result.elements {
            let elem_desc = format!(
                "{} {}",
                element.element_type,
                element.text.as_deref().unwrap_or("")
            )
            .to_lowercase();

            if elem_desc.contains(&search_lower) {
                return Ok(element.clone());
            }
        }

        // If no specific match, return a generic element
        warn!("No specific element found for: {}", description);
        Ok(ElementDescription {
            element_type: "unknown".to_string(),
            text: None,
            x: 0.5,
            y: 0.5,
            width: 0.1,
            height: 0.1,
            confidence: 0.5,
        })
    }

    fn is_ready(&self) -> bool {
        // Check if Ollama is available (blocking call)
        // In production, use proper health check
        let runtime = tokio::runtime::Runtime::new();
        match runtime {
            Ok(rt) => rt.block_on(self.is_available()),
            Err(_) => false,
        }
    }

    fn model_info(&self) -> (String, String) {
        ("ollama".to_string(), self.config.model_name.clone())
    }
}
