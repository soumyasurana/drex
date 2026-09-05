//! Speech-to-Text - Whisper-based local STT

use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::audio::{AudioBuffer, AudioCapture, AudioConfig};

/// STT configuration.
#[derive(Debug, Clone)]
pub struct SttConfig {
    /// Path to the Whisper model file.
    pub model_path: Option<PathBuf>,
    /// Language code (e.g., "en", "auto" for auto-detect)
    pub language: String,
    /// Enable translation to English.
    pub translate: bool,
    /// Maximum audio duration to process (seconds).
    pub max_duration_secs: u64,
    /// Beam search width.
    pub beam_width: i32,
    /// Best of N samples.
    pub best_of: i32,
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            model_path: None,
            language: "auto".to_string(),
            translate: false,
            max_duration_secs: 30,
            beam_width: 5,
            best_of: 5,
        }
    }
}

/// Transcription result.
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    /// The transcribed text.
    pub text: String,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
    /// Language detected (if applicable).
    pub language: Option<String>,
    /// Processing duration in milliseconds.
    pub duration_ms: u64,
    /// Whether the user requested to stop listening.
    pub stop_requested: bool,
}

/// Errors that can occur during STT operations.
#[derive(Debug, thiserror::Error)]
pub enum SttError {
    /// Model not found.
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    /// Model loading failed.
    #[error("Failed to load model: {0}")]
    ModelLoadError(String),

    /// Transcription failed.
    #[error("Transcription failed: {0}")]
    TranscriptionError(String),

    /// Audio error.
    #[error("Audio error: {0}")]
    AudioError(#[from] crate::audio::AudioError),

    /// No speech detected.
    #[error("No speech detected")]
    NoSpeechDetected,

    /// Audio too long.
    #[error("Audio exceeds maximum duration: {0}s > {1}s")]
    AudioTooLong(u64, u64),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// STT engine trait.
#[async_trait::async_trait]
pub trait SpeechToText: Send + Sync {
    /// Transcribe audio buffer to text.
    async fn transcribe(&self, audio: &AudioBuffer) -> Result<TranscriptionResult, SttError>;

    /// Transcribe from microphone input for a duration.
    async fn transcribe_from_mic(&self, duration_ms: u64) -> Result<TranscriptionResult, SttError>;

    /// Check if the engine is ready.
    fn is_ready(&self) -> bool;

    /// Get supported languages.
    fn supported_languages(&self) -> Vec<String>;
}

/// Placeholder STT engine.
pub struct PlaceholderSttEngine {
    config: SttConfig,
}

impl PlaceholderSttEngine {
    /// Create a new placeholder engine.
    pub fn new(config: SttConfig) -> Result<Self, SttError> {
        info!("Creating placeholder STT engine");
        Ok(Self { config })
    }

    fn check_stop_phrase(&self, text: &str) -> bool {
        let stop_phrases = ["stop", "quit", "exit", "goodbye", "that's all"];
        let lower = text.to_lowercase();
        stop_phrases.iter().any(|p| lower.contains(p))
    }
}

#[async_trait::async_trait]
impl SpeechToText for PlaceholderSttEngine {
    async fn transcribe(&self, audio: &AudioBuffer) -> Result<TranscriptionResult, SttError> {
        let start = std::time::Instant::now();
        debug!("Transcribing {} audio samples", audio.len());
        let text = "Placeholder transcription - STT not fully implemented yet".to_string();
        let duration_ms = start.elapsed().as_millis() as u64;
        let stop_requested = self.check_stop_phrase(&text);
        Ok(TranscriptionResult {
            text,
            confidence: 0.0,
            language: Some(self.config.language.clone()),
            duration_ms,
            stop_requested,
        })
    }

    async fn transcribe_from_mic(&self, duration_ms: u64) -> Result<TranscriptionResult, SttError> {
        if duration_ms > self.config.max_duration_secs * 1000 {
            return Err(SttError::AudioTooLong(duration_ms / 1000, self.config.max_duration_secs));
        }
        let capture = AudioCapture::new(AudioConfig::default());
        let audio = capture.record_duration(duration_ms).await?;
        self.transcribe(&audio).await
    }

    fn is_ready(&self) -> bool {
        false
    }

    fn supported_languages(&self) -> Vec<String> {
        vec!["en".to_string()]
    }
}

/// Type alias for the STT engine.
pub type SttEngine = Arc<dyn SpeechToText>;

/// Create a default STT engine (placeholder).
pub fn create_stt_engine(config: SttConfig) -> Result<SttEngine, SttError> {
    let engine = PlaceholderSttEngine::new(config)?;
    Ok(Arc::new(engine))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stt_config_default() {
        let config = SttConfig::default();
        assert_eq!(config.language, "auto");
        assert_eq!(config.max_duration_secs, 30);
    }

    #[tokio::test]
    async fn test_placeholder_transcribe() {
        let config = SttConfig::default();
        let engine = PlaceholderSttEngine::new(config).unwrap();
        let audio = vec![0.0; 16000];
        let result = engine.transcribe(&audio).await.unwrap();
        assert!(!result.text.is_empty());
        assert!(!result.stop_requested);
    }

    #[tokio::test]
    async fn test_stop_phrase_detection() {
        let config = SttConfig::default();
        let engine = PlaceholderSttEngine::new(config).unwrap();
        assert!(engine.check_stop_phrase("Please stop listening"));
        assert!(!engine.check_stop_phrase("Continue please"));
    }

    #[test]
    fn test_supported_languages() {
        let config = SttConfig::default();
        let engine = PlaceholderSttEngine::new(config).unwrap();
        let langs = engine.supported_languages();
        assert_eq!(langs, vec!["en"]);
    }

    #[test]
    fn test_create_stt_engine() {
        let config = SttConfig::default();
        let engine = create_stt_engine(config).unwrap();
        assert!(!engine.is_ready());
    }
}
