//! Drex Voice - Speech-to-Text and Text-to-Speech
//!
//! This crate provides local voice processing capabilities:
//! - Speech-to-Text (STT) using Whisper (local, offline)
//! - Text-to-Speech (TTS) using local synthesis engines
//!
//! # Privacy
//!
//! All voice processing happens entirely on-device. No audio data
//! is ever sent to external services.
//!
//! # Architecture
//!
//! - **Audio Capture**: Capture microphone input using cpal
//! - **STT Engine**: Whisper model running locally via whisper-rs
//! - **TTS Engine**: Local speech synthesis via tts crate
//! - **Voice Loop**: Continuous listening and response mode

#![doc = include_str!("../README.md")]

pub mod audio;
pub mod stt;
pub mod tts;
pub mod voice_loop;

pub use audio::{AudioCapture, AudioConfig, AudioError, AudioSample};
pub use stt::{SpeechToText, SttEngine, SttError, SttConfig, TranscriptionResult};
pub use tts::{TextToSpeech, TtsEngine, TtsError, TtsConfig, SpeakResult};
pub use voice_loop::{VoiceLoop, VoiceLoopConfig, VoiceLoopError, VoiceSession};

/// Version of the voice crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check if STT support is available.
pub fn is_stt_available() -> bool {
    cfg!(feature = "stt")
}

/// Check if TTS support is available.
pub fn is_tts_available() -> bool {
    cfg!(feature = "tts")
}

/// Initialize the voice subsystem.
pub fn init() {
    tracing::info!("Drex Voice initialized");
    tracing::info!("STT available: {}", is_stt_available());
    tracing::info!("TTS available: {}", is_tts_available());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voice_init() {
        init();
    }

    #[test]
    fn test_stt_availability() {
        // Should return true when feature is enabled
        let available = is_stt_available();
        assert!(available || !cfg!(feature = "stt"));
    }

    #[test]
    fn test_tts_availability() {
        // Should return true when feature is enabled
        let available = is_tts_available();
        assert!(available || !cfg!(feature = "tts"));
    }
}
