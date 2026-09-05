//! Voice Loop - Continuous voice interaction

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::audio::AudioConfig;
use crate::stt::{create_stt_engine, SpeechToText, SttConfig, SttError, SttEngine, TranscriptionResult};
use crate::tts::{create_tts_engine, TextToSpeech, TtsConfig, TtsError, TtsEngine};

/// Voice loop configuration.
#[derive(Debug, Clone)]
pub struct VoiceLoopConfig {
    /// STT configuration.
    pub stt_config: SttConfig,
    /// TTS configuration.
    pub tts_config: TtsConfig,
    /// Audio capture configuration.
    pub audio_config: AudioConfig,
    /// Activation phrase (say this to start listening).
    pub activation_phrase: Option<String>,
    /// Recording threshold in dB (0.0 = silence, 1.0 = max).
    pub recording_threshold: f32,
    /// Timeout waiting for speech (seconds).
    pub speech_timeout_secs: u64,
    /// Max response wait time (seconds).
    pub response_timeout_secs: u64,
}

impl Default for VoiceLoopConfig {
    fn default() -> Self {
        Self {
            stt_config: SttConfig::default(),
            tts_config: TtsConfig::default(),
            audio_config: AudioConfig::default(),
            activation_phrase: Some("Hey Drex".to_string()),
            recording_threshold: 0.05,
            speech_timeout_secs: 10,
            response_timeout_secs: 30,
        }
    }
}

/// Errors that can occur in the voice loop.
#[derive(Debug, thiserror::Error)]
pub enum VoiceLoopError {
    /// STT error.
    #[error("STT error: {0}")]
    SttError(#[from] SttError),

    /// TTS error.
    #[error("TTS error: {0}")]
    TtsError(#[from] TtsError),

    /// Audio error.
    #[error("Audio error: {0}")]
    AudioError(#[from] crate::audio::AudioError),

    /// Agent processing error.
    #[error("Agent error: {0}")]
    AgentError(String),

    /// Timeout waiting for input.
    #[error("Timeout waiting for speech")]
    SpeechTimeout,

    /// Timeout waiting for response.
    #[error("Timeout waiting for response")]
    ResponseTimeout,

    /// Voice loop cancelled.
    #[error("Voice loop cancelled")]
    Cancelled,
}

/// Voice session state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoiceSession {
    /// Waiting for activation phrase.
    Waiting = 0,
    /// Listening for user input.
    Listening = 1,
    /// Processing the request.
    Processing = 2,
    /// Speaking the response.
    Speaking = 3,
    /// Session ended.
    Ended = 4,
}

/// Events from the voice loop.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// State changed.
    StateChanged { from: VoiceSession, to: VoiceSession },
    /// Heard activation phrase.
    Activated,
    /// Heard user input.
    Heard { text: String, confidence: f32 },
    /// Agent responded.
    Responded { text: String },
    /// Speaking started.
    SpeakingStarted,
    /// Speaking ended.
    SpeakingEnded,
    /// Error occurred.
    Error { message: String },
    /// Session ended.
    SessionEnded { reason: String },
}

/// Voice loop handler.
pub struct VoiceLoop {
    config: VoiceLoopConfig,
    stt: SttEngine,
    tts: TtsEngine,
    state: std::sync::atomic::AtomicU8,
    event_tx: Option<mpsc::Sender<VoiceEvent>>,
}

impl VoiceLoop {
    /// Create a new voice loop.
    pub fn new(config: VoiceLoopConfig) -> Result<Self, VoiceLoopError> {
        info!("Creating voice loop");
        let stt = create_stt_engine(config.stt_config.clone())?;
        let tts = create_tts_engine(config.tts_config.clone())?;
        Ok(Self {
            config,
            stt,
            tts,
            state: std::sync::atomic::AtomicU8::new(0),
            event_tx: None,
        })
    }

    /// Create a voice loop with event channel.
    pub fn with_events(
        config: VoiceLoopConfig,
        event_tx: mpsc::Sender<VoiceEvent>,
    ) -> Result<Self, VoiceLoopError> {
        let mut this = Self::new(config)?;
        this.event_tx = Some(event_tx);
        Ok(this)
    }

    /// Get current state.
    pub fn current_state(&self) -> VoiceSession {
        match self.state.load(std::sync::atomic::Ordering::SeqCst) {
            0 => VoiceSession::Waiting,
            1 => VoiceSession::Listening,
            2 => VoiceSession::Processing,
            3 => VoiceSession::Speaking,
            4 => VoiceSession::Ended,
            _ => VoiceSession::Waiting,
        }
    }

    /// Set state and emit event.
    fn set_state(&self, new_state: VoiceSession) {
        let old_state = self.current_state();
        if old_state != new_state {
            self.state.store(new_state as u8, std::sync::atomic::Ordering::SeqCst);
            self.emit(VoiceEvent::StateChanged {
                from: old_state,
                to: new_state,
            });
        }
    }

    /// Emit an event.
    fn emit(&self, event: VoiceEvent) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.try_send(event);
        }
    }

    /// Check if the session is active.
    pub fn is_active(&self) -> bool {
        self.current_state() != VoiceSession::Ended
    }

    /// Start the voice loop (async, runs until stopped).
    pub async fn run<F, Fut>(&self, mut process_fn: F) -> Result<(), VoiceLoopError>
    where
        F: FnMut(String) -> Fut + Send + Sync,
        Fut: std::future::Future<Output = Result<String, String>> + Send,
    {
        self.set_state(VoiceSession::Waiting);
        loop {
            match self.current_state() {
                VoiceSession::Waiting => {
                    if let Err(e) = self.wait_for_activation().await {
                        self.emit(VoiceEvent::Error {
                            message: format!("Activation failed: {:?}", e),
                        });
                        continue;
                    }
                    self.set_state(VoiceSession::Listening);
                }
                VoiceSession::Listening => {
                    match self.listen_for_input().await {
                        Ok(transcription) => {
                            if transcription.stop_requested {
                                self.emit(VoiceEvent::SessionEnded {
                                    reason: "User said stop".to_string(),
                                });
                                self.set_state(VoiceSession::Ended);
                                break;
                            }
                            self.emit(VoiceEvent::Heard {
                                text: transcription.text.clone(),
                                confidence: transcription.confidence,
                            });
                            self.set_state(VoiceSession::Processing);
                            let response = match tokio::time::timeout(
                                tokio::time::Duration::from_secs(self.config.response_timeout_secs),
                                process_fn(transcription.text),
                            )
                            .await
                            {
                                Ok(Ok(response)) => response,
                                Ok(Err(e)) => {
                                    self.emit(VoiceEvent::Error {
                                        message: format!("Agent error: {}", e),
                                    });
                                    format!("Sorry, I encountered an error: {}", e)
                                }
                                Err(_) => {
                                    self.emit(VoiceEvent::Error {
                                        message: "Response timeout".to_string(),
                                    });
                                    "Sorry, that took too long.".to_string()
                                }
                            };
                            self.emit(VoiceEvent::Responded {
                                text: response.clone(),
                            });
                            self.set_state(VoiceSession::Speaking);
                            self.speak(&response).await?;
                            self.set_state(VoiceSession::Waiting);
                        }
                        Err(e) => {
                            self.emit(VoiceEvent::Error {
                                message: format!("Listen failed: {:?}", e),
                            });
                            self.set_state(VoiceSession::Waiting);
                        }
                    }
                }
                VoiceSession::Processing => {
                    debug!("In processing state");
                }
                VoiceSession::Speaking => {
                    debug!("In speaking state");
                }
                VoiceSession::Ended => {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Wait for activation phrase (simplified implementation).
    async fn wait_for_activation(&self) -> Result<(), VoiceLoopError> {
        debug!("Waiting for activation phrase");
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        self.emit(VoiceEvent::Activated);
        Ok(())
    }

    /// Listen for user input.
    async fn listen_for_input(&self) -> Result<TranscriptionResult, VoiceLoopError> {
        debug!("Listening for user input");
        let duration_ms = self.config.stt_config.max_duration_secs * 1000;
        let result = self.stt.transcribe_from_mic(duration_ms).await?;
        Ok(result)
    }

    /// Speak a response.
    async fn speak(&self, text: &str) -> Result<(), VoiceLoopError> {
        if text.is_empty() {
            return Ok(());
        }
        debug!("Speaking: '{}'", text.chars().take(50).collect::<String>());
        self.emit(VoiceEvent::SpeakingStarted);
        self.tts.speak(text).await?;
        self.emit(VoiceEvent::SpeakingEnded);
        Ok(())
    }

    /// Stop the voice loop (can be called from another task).
    pub fn stop(&self) {
        debug!("Stopping voice loop");
        self.set_state(VoiceSession::Ended);
    }
}

/// Create a new voice loop with default configuration.
pub fn create_voice_loop() -> Result<VoiceLoop, VoiceLoopError> {
    VoiceLoop::new(VoiceLoopConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_loop_config_default() {
        let config = VoiceLoopConfig::default();
        assert_eq!(config.activation_phrase, Some("Hey Drex".to_string()));
        assert_eq!(config.speech_timeout_secs, 10);
        assert_eq!(config.response_timeout_secs, 30);
    }

    #[tokio::test]
    async fn test_voice_loop_state_transitions() {
        let config = VoiceLoopConfig::default();
        let (tx, mut rx) = mpsc::channel(10);
        let loop_ = VoiceLoop::with_events(config, tx).unwrap();
        assert_eq!(loop_.current_state(), VoiceSession::Waiting);
        loop_.set_state(VoiceSession::Listening);
        assert_eq!(loop_.current_state(), VoiceSession::Listening);
        let event = rx.try_recv().unwrap();
        match event {
            VoiceEvent::StateChanged { from, to } => {
                assert_eq!(from, VoiceSession::Waiting);
                assert_eq!(to, VoiceSession::Listening);
            }
            _ => panic!("Expected state changed event"),
        }
    }

    #[test]
    fn test_voice_session_enum() {
        assert_eq!(VoiceSession::Waiting as u8, 0);
        assert_eq!(VoiceSession::Listening as u8, 1);
        assert_eq!(VoiceSession::Processing as u8, 2);
        assert_eq!(VoiceSession::Speaking as u8, 3);
        assert_eq!(VoiceSession::Ended as u8, 4);
    }

    #[tokio::test]
    async fn test_create_voice_loop() {
        let result = create_voice_loop();
        assert!(result.is_ok());
    }
}
