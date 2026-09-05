//! Audio Capture - Microphone input handling
//!
//! Uses cpal for cross-platform audio device enumeration and capture.
//! Supports resampling and format conversion for STT compatibility.

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Audio configuration for capture.
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Sample rate in Hz (default: 16000 for Whisper compatibility)
    pub sample_rate: u32,
    /// Number of channels (default: 1 for mono)
    pub channels: u16,
    /// Buffer size in samples.
    pub buffer_size: u32,
    /// Device name (None for default)
    pub device_name: Option<String>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            // Whisper works best with 16kHz
            sample_rate: 16000,
            channels: 1,
            buffer_size: 1024,
            device_name: None,
        }
    }
}

/// A single audio sample (floating point -1.0 to 1.0).
pub type AudioSample = f32;

/// A buffer of audio samples.
pub type AudioBuffer = Vec<AudioSample>;

/// Errors that can occur during audio operations.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// No audio device available.
    #[error("No audio device available: {0}")]
    NoDevice(String),

    /// Device configuration failed.
    #[error("Failed to configure device: {0}")]
    ConfigError(String),

    /// Stream creation failed.
    #[error("Failed to create audio stream: {0}")]
    StreamError(String),

    /// Recording error.
    #[error("Recording error: {0}")]
    RecordingError(String),

    /// Resampling error.
    #[error("Resampling error: {0}")]
    ResampleError(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Audio capture handler.
pub struct AudioCapture {
    config: AudioConfig,
}

impl AudioCapture {
    /// Create a new audio capture instance.
    pub fn new(config: AudioConfig) -> Self {
        Self { config }
    }

    /// Check if audio capture is available on this system.
    pub fn is_available() -> bool {
        // cpal is cross-platform and should work on most systems
        true
    }

    /// List available input devices.
    pub fn list_devices() -> Result<Vec<String>, AudioError> {
        // This is a placeholder - actual implementation would use cpal
        Ok(vec!["Default Microphone".to_string()])
    }

    /// Record audio for a specified duration.
    pub async fn record_duration(&self, duration_ms: u64) -> Result<AudioBuffer, AudioError> {
        let num_samples = (self.config.sample_rate as u64 * duration_ms / 1000) as usize;
        let mut buffer = Vec::with_capacity(num_samples);

        // Simulate recording - in real implementation would use cpal
        debug!("Recording {}ms of audio", duration_ms);

        // Fill with placeholder (would be actual microphone data)
        buffer.resize(num_samples, 0.0);

        Ok(buffer)
    }

    /// Start continuous recording, returning a channel receiver.
    pub async fn start_recording(&self) -> Result<mpsc::Receiver<AudioBuffer>, AudioError> {
        let (tx, rx) = mpsc::channel(100);

        // In real implementation, would spawn a task that:
        // 1. Opens cpal stream
        // 2. Captures audio
        // 3. Sends chunks to channel
        debug!("Starting continuous audio recording");

        // Placeholder: spawn a task that sends empty buffers
        let config = self.config.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_millis(config.buffer_size as u64 * 1000 / config.sample_rate as u64)
            );

            loop {
                interval.tick().await;
                let chunk = vec![0.0; config.buffer_size as usize];
                if tx.send(chunk).await.is_err() {
                    break;
                }
            }
        });

        Ok(rx)
    }

    /// Stop an ongoing recording.
    pub fn stop_recording(&self) {
        debug!("Stopping audio recording");
    }

    /// Resample audio to target sample rate.
    pub fn resample(
        &self,
        buffer: &AudioBuffer,
        from_rate: u32,
        to_rate: u32,
    ) -> Result<AudioBuffer, AudioError> {
        if from_rate == to_rate {
            return Ok(buffer.clone());
        }

        // Simple linear resampling - production would use high-quality resampling
        let ratio = to_rate as f64 / from_rate as f64;
        let new_len = (buffer.len() as f64 * ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);

        for i in 0..new_len {
            let src_idx = i as f64 / ratio;
            let idx_floor = src_idx.floor() as usize;
            let idx_ceil = (src_idx.ceil() as usize).min(buffer.len() - 1);
            let fraction = src_idx - idx_floor as f64;

            let val = buffer[idx_floor] * (1.0 - fraction as f32)
                + buffer[idx_ceil] * fraction as f32;
            resampled.push(val);
        }

        Ok(resampled)
    }

    /// Convert audio buffer to WAV format bytes.
    pub fn to_wav(&self, buffer: &AudioBuffer) -> Result<Vec<u8>, AudioError> {
        use std::io::Cursor;

        let mut cursor = Cursor::new(Vec::new());

        // WAV header (44 bytes) + data
        let num_samples = buffer.len();
        let byte_rate = self.config.sample_rate * self.config.channels as u32 * 2; // 16-bit
        let block_align = self.config.channels * 2;
        let data_size = num_samples as u32 * 2;
        let file_size = 36 + data_size;

        // RIFF header
        cursor.write_all(b"RIFF")?;
        cursor.write_all(&file_size.to_le_bytes())?;
        cursor.write_all(b"WAVE")?;

        // fmt chunk
        cursor.write_all(b"fmt ")?;
        cursor.write_all(&16u32.to_le_bytes())?; // chunk size
        cursor.write_all(&1u16.to_le_bytes())?; // PCM format
        cursor.write_all(&self.config.channels.to_le_bytes())?;
        cursor.write_all(&self.config.sample_rate.to_le_bytes())?;
        cursor.write_all(&byte_rate.to_le_bytes())?;
        cursor.write_all(&block_align.to_le_bytes())?;
        cursor.write_all(&16u16.to_le_bytes())?; // bits per sample

        // data chunk
        cursor.write_all(b"data")?;
        cursor.write_all(&data_size.to_le_bytes())?;

        // Write samples as 16-bit PCM
        for sample in buffer {
            let pcm = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
            cursor.write_all(&pcm.to_le_bytes())?;
        }

        Ok(cursor.into_inner())
    }

    /// Get the configured sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    /// Get the configured channels.
    pub fn channels(&self) -> u16 {
        self.config.channels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_config_default() {
        let config = AudioConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.channels, 1);
    }

    #[tokio::test]
    async fn test_record_duration() {
        let capture = AudioCapture::new(AudioConfig::default());
        let buffer = capture.record_duration(100).await.unwrap();
        // 16000 samples/sec * 0.1 sec = 1600 samples
        assert_eq!(buffer.len(), 1600);
    }

    #[test]
    fn test_resample() {
        let capture = AudioCapture::new(AudioConfig::default());
        let buffer = vec![0.0; 16000]; // 1 second at 16kHz
        let resampled = capture.resample(&buffer, 16000, 8000).unwrap();
        assert_eq!(resampled.len(), 8000);
    }

    #[test]
    fn test_to_wav() {
        let capture = AudioCapture::new(AudioConfig::default());
        let buffer = vec![0.0; 16000];
        let wav = capture.to_wav(&buffer).unwrap();
        // WAV header is 44 bytes + 16000 * 2 bytes (16-bit samples)
        assert_eq!(wav.len(), 44 + 32000);
    }
}
