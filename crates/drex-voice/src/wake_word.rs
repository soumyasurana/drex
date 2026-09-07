//! Wake Word Detection - Keyword spotting for voice activation
//!
//! Provides always-on listening for activation phrases like "Hey Drex" or "Jarvis"
//! without requiring full STT processing.
//!
//! # Features
//!
//! - Continuous audio stream processing
//! - Pattern matching for wake phrases
//! - Energy-based Voice Activity Detection (VAD)
//! - Configurable wake phrases
//! - Low CPU usage when idle
//!
//! # Example
//!
//! ```rust,ignore
//! use drex_voice::wake_word::{WakeWordDetector, WakeWordConfig};
//!
//! let config = WakeWordConfig::default();
//! let detector = WakeWordDetector::new(config)?;
//!
//! // Start listening
//! detector.start().await?;
//!
//! // Wait for wake word
//! if detector.wait_for_wake_word().await? {
//!     println!("Wake word detected!");
//! }
//! ```

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use crate::audio::{AudioBuffer, AudioCapture, AudioConfig};

/// Configuration for wake word detection.
#[derive(Debug, Clone)]
pub struct WakeWordConfig {
    /// The wake phrase to listen for.
    pub wake_phrase: String,
    /// Alternative wake phrases.
    pub alternative_phrases: Vec<String>,
    /// Audio configuration.
    pub audio_config: AudioConfig,
    /// VAD energy threshold (0.0 to 1.0).
    /// Lower = more sensitive, higher = less false positives.
    pub vad_threshold: f32,
    /// Minimum speech duration to trigger analysis (milliseconds).
    pub min_speech_duration_ms: u64,
    /// Maximum speech duration (milliseconds).
    pub max_speech_duration_ms: u64,
    /// Timeout for wake word detection.
    pub detection_timeout: Duration,
    /// Whether to use fuzzy matching for wake phrases.
    pub fuzzy_match: bool,
    /// Similarity threshold for fuzzy matching (0.0 to 1.0).
    pub similarity_threshold: f32,
}

impl Default for WakeWordConfig {
    fn default() -> Self {
        Self {
            wake_phrase: "Hey Drex".to_string(),
            alternative_phrases: vec![
                "Drex".to_string(),
                "Hey".to_string(),
                "Jarvis".to_string(),
                "Computer".to_string(),
            ],
            audio_config: AudioConfig::default(),
            vad_threshold: 0.02,
            min_speech_duration_ms: 500,
            max_speech_duration_ms: 3000,
            detection_timeout: Duration::from_secs(60),
            fuzzy_match: true,
            similarity_threshold: 0.7,
        }
    }
}

/// Wake word detection result.
#[derive(Debug, Clone)]
pub struct WakeWordResult {
    /// The detected phrase.
    pub phrase: String,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
    /// Time since listening started.
    pub elapsed: Duration,
}

/// Errors that can occur during wake word detection.
#[derive(Debug, thiserror::Error)]
pub enum WakeWordError {
    /// Audio error.
    #[error("Audio error: {0}")]
    AudioError(#[from] crate::audio::AudioError),

    /// Timeout waiting for wake word.
    #[error("Wake word detection timed out")]
    Timeout,

    /// Detection cancelled.
    #[error("Wake word detection cancelled")]
    Cancelled,

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Wake word detector state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetectorState {
    /// Idle, not listening.
    Idle,
    /// Listening for wake word.
    Listening,
    /// Speech detected, analyzing.
    Analyzing,
    /// Wake word detected.
    Detected,
    /// Stopped.
    Stopped,
}

/// Wake word detector.
pub struct WakeWordDetector {
    config: WakeWordConfig,
    state: Arc<Mutex<DetectorState>>,
    cancel_tx: Option<mpsc::Sender<()>>,
}

impl WakeWordDetector {
    /// Create a new wake word detector.
    pub fn new(config: WakeWordConfig) -> Result<Self, WakeWordError> {
        info!("Creating wake word detector for phrase: '{}'", config.wake_phrase);
        Ok(Self {
            config,
            state: Arc::new(Mutex::new(DetectorState::Idle)),
            cancel_tx: None,
        })
    }

    /// Create a detector with default configuration.
    pub fn default() -> Result<Self, WakeWordError> {
        Self::new(WakeWordConfig::default())
    }

    /// Get current state.
    pub async fn state(&self) -> DetectorState {
        *self.state.lock().await
    }

    /// Set state.
    async fn set_state(&self, new_state: DetectorState) {
        let mut state = self.state.lock().await;
        *state = new_state;
    }

    /// Check if listening.
    pub async fn is_listening(&self) -> bool {
        matches!(self.state().await, DetectorState::Listening | DetectorState::Analyzing)
    }

    /// Calculate audio energy (RMS) for VAD.
    fn calculate_energy(audio: &AudioBuffer) -> f32 {
        if audio.is_empty() {
            return 0.0;
        }
        let sum_squares: f32 = audio.iter().map(|s| s * s).sum();
        (sum_squares / audio.len() as f32).sqrt()
    }

    /// Check if audio contains speech (simple energy-based VAD).
    fn detect_speech(&self, audio: &AudioBuffer) -> bool {
        let energy = Self::calculate_energy(audio);
        energy > self.config.vad_threshold
    }

    /// Calculate string similarity (0.0 to 1.0).
    fn string_similarity(a: &str, b: &str) -> f32 {
        let a_lower = a.to_lowercase();
        let b_lower = b.to_lowercase();
        
        // Exact match
        if a_lower == b_lower {
            return 1.0;
        }
        
        // Contains match
        if a_lower.contains(&b_lower) || b_lower.contains(&a_lower) {
            return 0.9;
        }
        
        // Simple Levenshtein-like distance
        let longer = if a_lower.len() > b_lower.len() { &a_lower } else { &b_lower };
        let shorter = if a_lower.len() > b_lower.len() { &b_lower } else { &a_lower };
        
        if longer.is_empty() {
            return 1.0;
        }
        
        let distance = Self::levenshtein_distance(&a_lower, &b_lower);
        1.0 - (distance as f32 / longer.len() as f32)
    }

    /// Calculate Levenshtein distance.
    fn levenshtein_distance(a: &str, b: &str) -> usize {
        let a_len = a.chars().count();
        let b_len = b.chars().count();
        
        if a_len == 0 { return b_len; }
        if b_len == 0 { return a_len; }
        
        let mut matrix = vec![vec![0; b_len + 1]; a_len + 1];
        
        for i in 0..=a_len {
            matrix[i][0] = i;
        }
        for j in 0..=b_len {
            matrix[0][j] = j;
        }
        
        for (i, a_char) in a.chars().enumerate() {
            for (j, b_char) in b.chars().enumerate() {
                let cost = if a_char == b_char { 0 } else { 1 };
                matrix[i + 1][j + 1] = *[
                    matrix[i][j + 1] + 1,      // deletion
                    matrix[i + 1][j] + 1,      // insertion
                    matrix[i][j] + cost,       // substitution
                ].iter().min().unwrap();
            }
        }
        
        matrix[a_len][b_len]
    }

    /// Match detected text against wake phrases.
    fn match_wake_phrase(&self, text: &str) -> Option<WakeWordResult> {
        let text_lower = text.trim().to_lowercase();
        let phrases = std::iter::once(&self.config.wake_phrase)
            .chain(self.config.alternative_phrases.iter());
        
        for phrase in phrases {
            let similarity = Self::string_similarity(&text_lower, &phrase.to_lowercase());
            
            if self.config.fuzzy_match && similarity >= self.config.similarity_threshold {
                return Some(WakeWordResult {
                    phrase: phrase.clone(),
                    confidence: similarity,
                    elapsed: Duration::default(), // Will be set by caller
                });
            } else if !self.config.fuzzy_match && text_lower.contains(&phrase.to_lowercase()) {
                return Some(WakeWordResult {
                    phrase: phrase.clone(),
                    confidence: 1.0,
                    elapsed: Duration::default(),
                });
            }
        }
        
        None
    }

    /// Start listening for the wake word.
    pub async fn start(&mut self) -> Result<WakeWordResult, WakeWordError> {
        info!("Starting wake word detection for '{}'", self.config.wake_phrase);
        self.set_state(DetectorState::Listening).await;
        
        let (cancel_tx, mut cancel_rx) = mpsc::channel(1);
        self.cancel_tx = Some(cancel_tx);
        
        let start_time = std::time::Instant::now();
        let capture = AudioCapture::new(self.config.audio_config.clone());
        
        // Start continuous recording
        let mut audio_rx = capture.start_recording().await?;
        
        let mut speech_buffer: AudioBuffer = Vec::new();
        let mut speech_start: Option<std::time::Instant> = None;
        
        let result = loop {
            tokio::select! {
                // Check for cancellation
                _ = cancel_rx.recv() => {
                    self.set_state(DetectorState::Stopped).await;
                    return Err(WakeWordError::Cancelled);
                }
                
                // Timeout
                _ = tokio::time::sleep(self.config.detection_timeout), if self.config.detection_timeout.as_secs() > 0 => {
                    self.set_state(DetectorState::Idle).await;
                    return Err(WakeWordError::Timeout);
                }
                
                // Process audio chunks
                Some(chunk) = audio_rx.recv() => {
                    let is_speech = self.detect_speech(&chunk);
                    
                    if is_speech {
                        if speech_start.is_none() {
                            speech_start = Some(std::time::Instant::now());
                            self.set_state(DetectorState::Analyzing).await;
                            debug!("Speech detected, starting analysis");
                        }
                        speech_buffer.extend(chunk);
                        
                        // Check if speech duration exceeds max
                        if let Some(start) = speech_start {
                            let duration = start.elapsed().as_millis() as u64;
                            if duration >= self.config.max_speech_duration_ms {
                                // Process accumulated speech
                                let result = self.process_speech_buffer(&speech_buffer, start_time.elapsed()).await;
                                if let Some(r) = result {
                                    break r;
                                }
                                // Reset for next utterance
                                speech_buffer.clear();
                                speech_start = None;
                                self.set_state(DetectorState::Listening).await;
                            }
                        }
                    } else if !speech_buffer.is_empty() {
                        // Speech ended
                        if let Some(start) = speech_start {
                            let duration = start.elapsed().as_millis() as u64;
                            if duration >= self.config.min_speech_duration_ms {
                                // Process accumulated speech
                                let result = self.process_speech_buffer(&speech_buffer, start_time.elapsed()).await;
                                if let Some(r) = result {
                                    break r;
                                }
                            }
                        }
                        // Reset
                        speech_buffer.clear();
                        speech_start = None;
                        self.set_state(DetectorState::Listening).await;
                    }
                }
            }
        };
        
        self.set_state(DetectorState::Detected).await;
        Ok(result)
    }

    /// Process accumulated speech buffer.
    async fn process_speech_buffer(
        &self,
        buffer: &AudioBuffer,
        elapsed: Duration,
    ) -> Option<WakeWordResult> {
        // In a real implementation, this would do light-weight STT
        // For now, we'll just do a placeholder that checks buffer characteristics
        debug!("Processing speech buffer: {} samples", buffer.len());
        
        // Placeholder: in real impl, run Whisper or similar on the buffer
        // and then match_wake_phrase() on the result
        
        // For now, just return None to continue listening
        None
    }

    /// Stop listening.
    pub async fn stop(&mut self) {
        info!("Stopping wake word detection");
        if let Some(tx) = self.cancel_tx.take() {
            let _ = tx.send(()).await;
        }
        self.set_state(DetectorState::Stopped).await;
    }
}

/// Convenience function to wait for wake word.
pub async fn wait_for_wake_word(config: Option<WakeWordConfig>) -> Result<WakeWordResult, WakeWordError> {
    let config = config.unwrap_or_default();
    let mut detector = WakeWordDetector::new(config)?;
    detector.start().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wake_word_config_default() {
        let config = WakeWordConfig::default();
        assert_eq!(config.wake_phrase, "Hey Drex");
        assert!(!config.alternative_phrases.is_empty());
    }

    #[test]
    fn test_string_similarity() {
        assert_eq!(WakeWordDetector::string_similarity("Hey Drex", "Hey Drex"), 1.0);
        assert!(WakeWordDetector::string_similarity("Hey Drex", "Hey") > 0.5);
        assert!(WakeWordDetector::string_similarity("Drex", "Hey Drex") > 0.5);
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(WakeWordDetector::levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(WakeWordDetector::levenshtein_distance("", ""), 0);
        assert_eq!(WakeWordDetector::levenshtein_distance("a", ""), 1);
    }

    #[test]
    fn test_calculate_energy() {
        let silence = vec![0.0; 100];
        assert_eq!(WakeWordDetector::calculate_energy(&silence), 0.0);
        
        let signal = vec![0.5; 100];
        assert!(WakeWordDetector::calculate_energy(&signal) > 0.0);
    }

    #[test]
    fn test_match_wake_phrase_exact() {
        let config = WakeWordConfig::default();
        let detector = WakeWordDetector::new(config).unwrap();
        
        let result = detector.match_wake_phrase("Hey Drex");
        assert!(result.is_some());
        assert_eq!(result.unwrap().phrase, "Hey Drex");
    }

    #[test]
    fn test_match_wake_phrase_fuzzy() {
        let mut config = WakeWordConfig::default();
        config.fuzzy_match = true;
        let detector = WakeWordDetector::new(config).unwrap();
        
        // Should match similar phrases
        let result = detector.match_wake_phrase("hey drex");
        assert!(result.is_some());
    }
}
