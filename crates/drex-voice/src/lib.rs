//! Drex Voice - Speech-to-Text and Text-to-Speech (Placeholder Implementation)
//!
//! This crate provides voice processing capabilities for Drex.
//! The current implementation is a placeholder that will be replaced with
//! full Whisper-based STT and local TTS in the next iteration.
//!
//! Full implementation depends on:
//! - whisper-rs: For offline STT using Whisper models
//! - cpal: For cross-platform audio capture
//! - tts: For local text-to-speech synthesis
//!
//! These dependencies require system libraries (ALSA on Linux) which
//! may not be available in all environments.

pub mod audio;
pub mod stt;
pub mod tts;
pub mod voice_loop;

pub use audio::{AudioConfig, AudioError, AudioSample, AudioBuffer};
pub use stt::{SttConfig, SttError, TranscriptionResult, create_stt_engine, SttEngine, SpeechToText};
pub use tts::{TtsConfig, TtsError, SpeakResult, create_tts_engine, TtsEngine, TextToSpeech};
pub use voice_loop::{VoiceLoop, VoiceLoopConfig, VoiceLoopError, VoiceSession, create_voice_loop};

/// Version of the voice crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check if STT support is available.
pub fn is_stt_available() -> bool {
    false // Placeholder
}

/// Check if TTS support is available.
pub fn is_tts_available() -> bool {
    false // Placeholder
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stt_not_available() {
        assert!(!is_stt_available());
    }

    #[test]
    fn test_tts_not_available() {
        assert!(!is_tts_available());
    }
}
