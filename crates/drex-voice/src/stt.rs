//! Speech-to-Text - Whisper-based local STT
//!
//! Uses whisper-rs for local, offline speech recognition.
//! All processing happens on-device for maximum privacy.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

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

/// Whisper-based STT engine.
pub struct WhisperEngine {
    config: SttConfig,
    // In real implementation, would hold whisper_rs::WhisperContext
    _placeholder: (),
}

impl WhisperEngine {
    /// Create a new Whisper engine.
    pub fn new(config: SttConfig) -> Result<Self, SttError> {
        // In real implementation, would load whisper model here
        // let model_path = config.model_path.as_ref()
        //     .ok_or_else(|| SttError::ModelNotFound("No model path configured".to_string()))?;
        // let ctx = whisper_rs::WhisperContext::new(model_path, ...)?;

        info!("Creating Whisper STT engine");

        Ok(Self {
            config,
            _placeholder: (),
        })
    }

    /// Download a model if not present.
    pub async fn download_model(&self, _model_name: &str) -> Result<PathBuf, SttError> {
        // In real implementation, would download from huggingface or similar
        warn!("Model download not yet implemented");
        Err(SttError::ModelNotFound("Model download not implemented".to_string()))
    }

    /// Check if a stop phrase was said.
    fn check_stop_phrase(&self, text: &str) -> bool {
        let stop_phrases = ["stop", "quit", "exit", "goodbye", "that's all"];
        let lower = text.to_lowercase();
        stop_phrases.iter().any(|p| lower.contains(p))
    }
}

#[async_trait::async_trait]
impl SpeechToText for WhisperEngine {
    async fn transcribe(&self, audio: &AudioBuffer) -> Result<TranscriptionResult, SttError> {
        let start = std::time::Instant::now();

        // In real implementation, would:
        // 1. Convert audio to 16kHz mono f32 if needed
        // 2. Run whisper inference
        // 3. Return result with confidence

        debug!("Transcribing {} audio samples", audio.len());

        // Placeholder implementation
        let text = "This is a placeholder transcription".to_string();
        let duration_ms = start.elapsed().as_millis() as u64;

        let stop_requested = self.check_stop_phrase(&text);

        Ok(TranscriptionResult {
            text,
            confidence: 0.95,
            language: Some(self.config.language.clone()),
            duration_ms,
            stop_requested,
        })
    }

    async fn transcribe_from_mic(&self, duration_ms: u64) -> Result<TranscriptionResult, SttError> {
        // Check duration limit
        if duration_ms > self.config.max_duration_secs * 1000 {
            return Err(SttError::AudioTooLong(duration_ms / 1000, self.config.max_duration_secs));
        }

        // Capture audio
        let capture = AudioCapture::new(AudioConfig::default());
        let audio = capture.record_duration(duration_ms).await?;

        // Transcribe
        self.transcribe(&audio).await
    }

    fn is_ready(&self) -> bool {
        // In real implementation, check if model is loaded
        true
    }

    fn supported_languages(&self) -> Vec<String> {
        // Whisper supports 99 languages
        vec![
            "auto".to_string(),
            "en".to_string(),
            "es".to_string(),
            "fr".to_string(),
            "de".to_string(),
            "it".to_string(),
            "pt".to_string(),
            "ru".to_string(),
            "zh".to_string(),
            "ja".to_string(),
            "ko".to_string(),
        ]
    }
}

/// Convenience type alias for the STT engine.
pub type SttEngine = Arc<dyn SpeechToText>;

/// Create a default STT engine.
pub fn create_stt_engine(config: SttConfig) -> Result<SttEngine, SttError> {
    let engine = WhisperEngine::new(config)?;
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
    async fn test_whisper_transcribe() {
        let config = SttConfig::default();
        let engine = WhisperEngine::new(config).unwrap();

        let audio = vec![0.0; 16000]; // 1 second of silence
        let result = engine.transcribe(&audio).await.unwrap();

        assert!(!result.text.is_empty());
        assert!(result.confidence > 0.0);
    }

    #[tokio::test]
    async fn test_whisper_transcribe_from_mic() {
        let config = SttConfig::default();
        let engine = WhisperEngine::new(config).unwrap();

        let result = engine.transcribe_from_mic(100).await.unwrap();

        assert!(!result.text.is_empty());
    }

    #[tokio::test]
    async fn test_stop_phrase_detection() {
        let config = SttConfig::default();
        let engine = WhisperEngine::new(config).unwrap();

        // Create audio that will generate a stop phrase
        let audio = vec![0.0; 16000];
        let mut result = engine.transcribe(&audio).await.unwrap();
        result.text = "Please stop listening".to_string();

        assert!(engine.check_stop_phrase(&result.text));
    }

    #[test]
    fn test_supported_languages() {
        let config = SttConfig::default();
        let engine = WhisperEngine::new(config).unwrap();

        let langs = engine.supported_languages();
        assert!(langs.contains(&"en".to_string()));
        assert!(langs.contains(&"auto".to_string()));
    }

    #[test]
    fn test_create_stt_engine() {
        let config = SttConfig::default();
        let engine = create_stt_engine(config).unwrap();
        assert!(engine.is_ready());
    }
}
