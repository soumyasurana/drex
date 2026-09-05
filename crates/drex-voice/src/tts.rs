//! Text-to-Speech - Local TTS synthesis

use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info};

/// TTS configuration.
#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// Voice identifier.
    pub voice: Option<String>,
    /// Speech rate (0.5 to 2.0, 1.0 is normal).
    pub rate: f32,
    /// Volume (0.0 to 1.0).
    pub volume: f32,
    /// Pitch adjustment (-1.0 to 1.0).
    pub pitch: f32,
    /// Output device (None for default).
    pub output_device: Option<String>,
    /// Save audio to file instead of playing.
    pub output_file: Option<PathBuf>,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            voice: None,
            rate: 1.0,
            volume: 1.0,
            pitch: 0.0,
            output_device: None,
            output_file: None,
        }
    }
}

/// Result of a speak operation.
#[derive(Debug, Clone)]
pub struct SpeakResult {
    /// Duration of the audio in milliseconds.
    pub duration_ms: u64,
    /// Number of characters spoken.
    pub char_count: usize,
    /// Whether the operation completed successfully.
    pub success: bool,
}

/// Errors that can occur during TTS operations.
#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    /// No TTS backend available.
    #[error("No TTS backend available: {0}")]
    NoBackend(String),

    /// Engine initialization failed.
    #[error("Failed to initialize TTS engine: {0}")]
    InitError(String),

    /// Synthesis failed.
    #[error("Speech synthesis failed: {0}")]
    SynthesisError(String),

    /// Audio playback failed.
    #[error("Audio playback failed: {0}")]
    PlaybackError(String),

    /// Voice not found.
    #[error("Voice not found: {0}")]
    VoiceNotFound(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// TTS engine trait.
#[async_trait::async_trait]
pub trait TextToSpeech: Send + Sync {
    /// Speak the given text.
    async fn speak(&self, text: &str) -> Result<SpeakResult, TtsError>;

    /// Synthesize speech to a file.
    async fn synthesize_to_file(&self, text: &str, path: &PathBuf) -> Result<SpeakResult, TtsError>;

    /// Stop any ongoing speech.
    fn stop(&self);

    /// Check if speaking.
    fn is_speaking(&self) -> bool;

    /// List available voices.
    fn list_voices(&self) -> Vec<String>;

    /// Set the voice.
    fn set_voice(&mut self, voice: &str) -> Result<(), TtsError>;

    /// Set the speech rate.
    fn set_rate(&mut self, rate: f32);

    /// Set the volume.
    fn set_volume(&mut self, volume: f32);
}

/// Placeholder TTS engine.
pub struct PlaceholderTtsEngine {
    config: TtsConfig,
    speaking: std::sync::atomic::AtomicBool,
}

impl PlaceholderTtsEngine {
    /// Create a new placeholder TTS engine.
    pub fn new(config: TtsConfig) -> Result<Self, TtsError> {
        info!("Creating placeholder TTS engine");
        Ok(Self {
            config,
            speaking: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn estimate_duration(&self, text: &str) -> u64 {
        let chars_per_sec = 12.5 * self.config.rate;
        let duration_sec = text.len() as f32 / chars_per_sec;
        (duration_sec * 1000.0) as u64
    }
}

#[async_trait::async_trait]
impl TextToSpeech for PlaceholderTtsEngine {
    async fn speak(&self, text: &str) -> Result<SpeakResult, TtsError> {
        if text.is_empty() {
            return Ok(SpeakResult {
                duration_ms: 0,
                char_count: 0,
                success: true,
            });
        }
        debug!("Placeholder speak: '{}'", text);
        self.speaking.store(true, std::sync::atomic::Ordering::SeqCst);
        let duration_ms = self.estimate_duration(text);
        tokio::time::sleep(tokio::time::Duration::from_millis(duration_ms)).await;
        self.speaking.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(SpeakResult {
            duration_ms,
            char_count: text.len(),
            success: true,
        })
    }

    async fn synthesize_to_file(&self, _text: &str, path: &PathBuf) -> Result<SpeakResult, TtsError> {
        debug!("Placeholder synthesize to: {:?}", path);
        tokio::fs::write(path, b"placeholder audio data").await?;
        Ok(SpeakResult {
            duration_ms: 0,
            char_count: _text.len(),
            success: true,
        })
    }

    fn stop(&self) {
        debug!("Stopping TTS");
        self.speaking.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    fn is_speaking(&self) -> bool {
        self.speaking.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn list_voices(&self) -> Vec<String> {
        vec!["default".to_string()]
    }

    fn set_voice(&mut self, voice: &str) -> Result<(), TtsError> {
        debug!("Setting voice to: {}", voice);
        self.config.voice = Some(voice.to_string());
        Ok(())
    }

    fn set_rate(&mut self, rate: f32) {
        self.config.rate = rate.clamp(0.5, 2.0);
    }

    fn set_volume(&mut self, volume: f32) {
        self.config.volume = volume.clamp(0.0, 1.0);
    }
}

/// Type alias for the TTS engine.
pub type TtsEngine = Arc<dyn TextToSpeech>;

/// Create a default TTS engine (placeholder).
pub fn create_tts_engine(config: TtsConfig) -> Result<TtsEngine, TtsError> {
    let engine = PlaceholderTtsEngine::new(config)?;
    Ok(Arc::new(engine))
}

/// Preprocess text for TTS.
pub fn preprocess_text(text: &str) -> String {
    let mut result = text.to_string();
    result = result.replace("**", "").replace("*", "");
    result = result.replace("__", "").replace("_", "");
    result = result.replace("```", "");
    result.split_whitespace()
        .map(|word| {
            if word.starts_with("http://") || word.starts_with("https://") {
                "a link".to_string()
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tts_config_default() {
        let config = TtsConfig::default();
        assert_eq!(config.rate, 1.0);
        assert_eq!(config.volume, 1.0);
    }

    #[tokio::test]
    async fn test_placeholder_tts_speak() {
        let config = TtsConfig::default();
        let engine = PlaceholderTtsEngine::new(config).unwrap();
        let result = engine.speak("Hello world").await.unwrap();
        assert!(result.success);
        assert!(result.duration_ms > 0);
        assert_eq!(result.char_count, 11);
    }

    #[tokio::test]
    async fn test_placeholder_tts_empty_text() {
        let config = TtsConfig::default();
        let engine = PlaceholderTtsEngine::new(config).unwrap();
        let result = engine.speak("").await.unwrap();
        assert!(result.success);
        assert_eq!(result.duration_ms, 0);
    }

    #[tokio::test]
    async fn test_placeholder_tts_synthesize_to_file() {
        let config = TtsConfig::default();
        let engine = PlaceholderTtsEngine::new(config).unwrap();
        let temp_path = PathBuf::from("/tmp/test_tts_output.wav");
        let result = engine.synthesize_to_file("Hello", &temp_path).await.unwrap();
        assert!(result.success);
        let _ = tokio::fs::remove_file(&temp_path).await;
    }

    #[test]
    fn test_placeholder_tts_list_voices() {
        let config = TtsConfig::default();
        let engine = PlaceholderTtsEngine::new(config).unwrap();
        let voices = engine.list_voices();
        assert_eq!(voices, vec!["default"]);
    }

    #[test]
    fn test_preprocess_text() {
        let input = "Check **this** link: https://example.com and `code`";
        let output = preprocess_text(input);
        assert!(!output.contains("**"));
        assert!(!output.contains("https://"));
    }

    #[test]
    fn test_create_tts_engine() {
        let config = TtsConfig::default();
        let engine = create_tts_engine(config).unwrap();
        assert!(!engine.is_speaking());
    }
}
