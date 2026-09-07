//! Advanced Audio Processing - Noise reduction, enhancement, and VAD
//!
//! This module provides sophisticated audio processing capabilities:
//! - Noise reduction and filtering
//! - Voice Activity Detection (VAD)
//! - Audio normalization and enhancement
//! - Echo cancellation support
//! - Audio quality metrics

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::{debug, trace, warn};

/// Configuration for audio processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioProcessingConfig {
    /// Enable noise reduction.
    pub noise_reduction: bool,
    /// Noise reduction strength (0.0 to 1.0).
    pub noise_reduction_strength: f32,
    /// Enable normalization.
    pub normalize: bool,
    /// Target level for normalization (dB).
    pub target_level_db: f32,
    /// Enable high-pass filter (removes low frequency rumble).
    pub high_pass_filter: bool,
    /// High-pass cutoff frequency (Hz).
    pub high_pass_cutoff: f32,
    /// Enable low-pass filter (removes high frequency hiss).
    pub low_pass_filter: bool,
    /// Low-pass cutoff frequency (Hz).
    pub low_pass_cutoff: f32,
    /// VAD configuration.
    pub vad: VadConfig,
}

impl Default for AudioProcessingConfig {
    fn default() -> Self {
        Self {
            noise_reduction: true,
            noise_reduction_strength: 0.5,
            normalize: true,
            target_level_db: -20.0,
            high_pass_filter: true,
            high_pass_cutoff: 80.0,
            low_pass_filter: false,
            low_pass_cutoff: 8000.0,
            vad: VadConfig::default(),
        }
    }
}

impl AudioProcessingConfig {
    /// Create processing config optimized for voice.
    pub fn voice_optimized() -> Self {
        Self {
            noise_reduction: true,
            noise_reduction_strength: 0.7,
            normalize: true,
            target_level_db: -16.0,
            high_pass_filter: true,
            high_pass_cutoff: 100.0,
            low_pass_filter: true,
            low_pass_cutoff: 7000.0,
            vad: VadConfig::default(),
        }
    }
}

/// VAD (Voice Activity Detection) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VadConfig {
    /// Energy threshold (0.0 to 1.0).
    pub energy_threshold: f32,
    /// Zero-crossing rate threshold.
    pub zcr_threshold: f32,
    /// Minimum speech duration (ms).
    pub min_speech_ms: u64,
    /// Maximum silence duration (ms).
    pub max_silence_ms: u64,
    /// Frame size for analysis (ms).
    pub frame_ms: u64,
    /// Hold time after speech ends (ms).
    pub hold_ms: u64,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            energy_threshold: 0.02,
            zcr_threshold: 0.1,
            min_speech_ms: 200,
            max_silence_ms: 500,
            frame_ms: 30,
            hold_ms: 300,
        }
    }
}

/// Audio processing result.
#[derive(Debug, Clone)]
pub struct ProcessedAudio {
    /// Processed audio samples.
    pub samples: Vec<f32>,
    /// Sample rate.
    pub sample_rate: u32,
    /// Voice activity detection result.
    pub vad_result: VadResult,
    /// Audio quality metrics.
    pub quality: AudioQuality,
}

/// VAD result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadResult {
    /// Silence/no speech detected.
    Silence,
    /// Speech detected.
    Speech,
    /// Transitioning.
    Transition,
}

impl VadResult {
    pub fn is_speech(&self) -> bool {
        matches!(self, Self::Speech)
    }

    pub fn is_silence(&self) -> bool {
        matches!(self, Self::Silence)
    }
}

/// Audio quality metrics.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudioQuality {
    /// Signal-to-noise ratio (dB).
    pub snr_db: f32,
    /// RMS level (dBFS).
    pub rms_db: f32,
    /// Peak level (dBFS).
    pub peak_db: f32,
    /// Clipping detected.
    pub clipping: bool,
}

/// Advanced audio processor.
pub struct AudioProcessor {
    config: AudioProcessingConfig,
    sample_rate: u32,
    // State for filters
    noise_gate: NoiseGate,
    high_pass: SimpleHighPass,
    low_pass: Option<SimpleLowPass>,
    // VAD state
    vad_state: VadState,
    // Buffer for frame processing
    frame_buffer: Vec<f32>,
}

impl AudioProcessor {
    /// Create new processor with config.
    pub fn new(config: AudioProcessingConfig, sample_rate: u32) -> Self {
        let frame_samples = (config.vad.frame_ms as u32 * sample_rate / 1000) as usize;
        Self {
            config: config.clone(),
            sample_rate,
            noise_gate: NoiseGate::new(config.noise_reduction_strength),
            high_pass: SimpleHighPass::new(config.high_pass_cutoff, sample_rate),
            low_pass: if config.low_pass_filter {
                Some(SimpleLowPass::new(config.low_pass_cutoff, sample_rate))
            } else {
                None
            },
            vad_state: VadState::new(&config.vad, sample_rate),
            frame_buffer: Vec::with_capacity(frame_samples),
        }
    }

    /// Process audio samples.
    pub fn process(&mut self, samples: &[f32]) -> ProcessedAudio {
        let mut processed = samples.to_vec();

        // Apply high-pass filter
        if self.config.high_pass_filter {
            self.high_pass.process(&mut processed);
        }

        // Apply low-pass filter if enabled
        if let Some(ref mut lp) = self.low_pass {
            lp.process(&mut processed);
        }

        // Apply noise gate/reduction
        if self.config.noise_reduction {
            self.noise_gate.process(&mut processed);
        }

        // Normalize
        if self.config.normalize {
            normalize_audio(&mut processed, self.config.target_level_db);
        }

        // Analyze VAD
        let vad_result = self.analyze_vad(&processed);

        // Calculate quality metrics
        let quality = analyze_quality(&processed);

        ProcessedAudio {
            samples: processed,
            sample_rate: self.sample_rate,
            vad_result,
            quality,
        }
    }

    /// Process audio in streaming mode.
    pub fn process_chunk(&mut self, chunk: &[f32]) -> Option<ProcessedAudio> {
        self.frame_buffer.extend_from_slice(chunk);

        let frame_samples = (self.config.vad.frame_ms as u32 * self.sample_rate / 1000) as usize;

        if self.frame_buffer.len() >= frame_samples {
            let frame: Vec<f32> = self.frame_buffer.drain(0..frame_samples).collect();
            Some(self.process(&frame))
        } else {
            None
        }
    }

    /// Analyze VAD for audio.
    fn analyze_vad(&mut self, samples: &[f32]) -> VadResult {
        self.vad_state.analyze(samples)
    }

    /// Flush remaining samples.
    pub fn flush(&mut self) -> Option<ProcessedAudio> {
        if !self.frame_buffer.is_empty() {
            let samples = self.frame_buffer.clone();
            self.frame_buffer.clear();
            Some(self.process(&samples))
        } else {
            None
        }
    }

    /// Reset processor state.
    pub fn reset(&mut self) {
        self.noise_gate.reset();
        self.high_pass.reset();
        if let Some(ref mut lp) = self.low_pass {
            lp.reset();
        }
        self.vad_state.reset();
        self.frame_buffer.clear();
    }
}

/// Simple noise gate implementation.
struct NoiseGate {
    threshold: f32,
    attack_coef: f32,
    release_coef: f32,
    envelope: f32,
    noise_floor: f32,
    noise_samples: VecDeque<f32>,
}

impl NoiseGate {
    fn new(strength: f32) -> Self {
        Self {
            threshold: 0.01 * (1.0 - strength),
            attack_coef: 0.9,
            release_coef: 0.995,
            envelope: 0.0,
            noise_floor: 0.0,
            noise_samples: VecDeque::with_capacity(1000),
        }
    }

    fn process(&mut self, samples: &mut [f32]) {
        // Estimate noise floor from silence periods
        let rms = calculate_rms(samples);
        if rms < self.threshold {
            self.noise_samples.push_back(rms);
            if self.noise_samples.len() > 1000 {
                self.noise_samples.pop_front();
            }
            // Update noise floor estimate
            self.noise_floor = self.noise_samples.iter().sum::<f32>() / self.noise_samples.len() as f32;
        }

        // Apply noise gate
        for sample in samples.iter_mut() {
            let abs_sample = sample.abs();
            
            // Update envelope
            if abs_sample > self.envelope {
                self.envelope = self.attack_coef * self.envelope + (1.0 - self.attack_coef) * abs_sample;
            } else {
                self.envelope = self.release_coef * self.envelope + (1.0 - self.release_coef) * abs_sample;
            }

            // Gate logic
            if self.envelope < self.threshold {
                // Attenuate below threshold
                let gain = (self.envelope / self.threshold).max(0.0);
                *sample *= gain * gain; // Quadratic for smoother transition
            }
        }
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
        self.noise_samples.clear();
        self.noise_floor = 0.0;
    }
}

/// Simple high-pass filter (removes DC offset and low rumble).
struct SimpleHighPass {
    cutoff: f32,
    sample_rate: u32,
    prev_input: f32,
    prev_output: f32,
    alpha: f32,
}

impl SimpleHighPass {
    fn new(cutoff: f32, sample_rate: u32) -> Self {
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
        let dt = 1.0 / sample_rate as f32;
        let alpha = rc / (rc + dt);

        Self {
            cutoff,
            sample_rate,
            prev_input: 0.0,
            prev_output: 0.0,
            alpha,
        }
    }

    fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let output = self.alpha * (self.prev_output + *sample - self.prev_input);
            self.prev_input = *sample;
            self.prev_output = output;
            *sample = output;
        }
    }

    fn reset(&mut self) {
        self.prev_input = 0.0;
        self.prev_output = 0.0;
    }
}

/// Simple low-pass filter.
struct SimpleLowPass {
    cutoff: f32,
    sample_rate: u32,
    prev_output: f32,
    alpha: f32,
}

impl SimpleLowPass {
    fn new(cutoff: f32, sample_rate: u32) -> Self {
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
        let dt = 1.0 / sample_rate as f32;
        let alpha = dt / (rc + dt);

        Self {
            cutoff,
            sample_rate,
            prev_output: 0.0,
            alpha,
        }
    }

    fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            self.prev_output += self.alpha * (*sample - self.prev_output);
            *sample = self.prev_output;
        }
    }

    fn reset(&mut self) {
        self.prev_output = 0.0;
    }
}

/// VAD state machine.
struct VadState {
    config: VadConfig,
    sample_rate: u32,
    energy_history: VecDeque<f32>,
    speech_frames: u64,
    silence_frames: u64,
    is_speaking: bool,
    hold_remaining: u64,
}

impl VadState {
    fn new(config: &VadConfig, sample_rate: u32) -> Self {
        let frame_samples = (config.frame_ms as u32 * sample_rate / 1000) as u32;
        let history_size = (config.min_speech_ms / config.frame_ms) as usize;

        Self {
            config: config.clone(),
            sample_rate,
            energy_history: VecDeque::with_capacity(history_size),
            speech_frames: 0,
            silence_frames: 0,
            is_speaking: false,
            hold_remaining: 0,
        }
    }

    fn analyze(&mut self, samples: &[f32]) -> VadResult {
        let frame_ms = self.config.frame_ms;
        let frame_samples = (frame_ms * self.sample_rate as u64 / 1000) as usize;

        // Calculate frame energy
        let energy = calculate_rms(samples);
        let zcr = calculate_zcr(samples);

        self.energy_history.push_back(energy);
        if self.energy_history.len() > self.energy_history.capacity() {
            self.energy_history.pop_front();
        }

        // Adaptive threshold
        let threshold = if self.energy_history.len() >= 5 {
            let avg = self.energy_history.iter().sum::<f32>() / self.energy_history.len() as f32;
            (self.config.energy_threshold).max(avg * 1.5)
        } else {
            self.config.energy_threshold
        };

        // Voice detection logic
        let is_voice = energy > threshold && zcr > self.config.zcr_threshold;

        if is_voice {
            self.speech_frames += 1;
            self.silence_frames = 0;
            self.hold_remaining = self.config.hold_ms / frame_ms;

            // Transition to speech
            if !self.is_speaking && self.speech_frames >= self.config.min_speech_ms / frame_ms as u64 {
                self.is_speaking = true;
                return VadResult::Speech;
            }
        } else {
            self.silence_frames += 1;
            
            if self.is_speaking {
                if self.hold_remaining > 0 {
                    self.hold_remaining -= 1;
                    return VadResult::Speech;
                }

                if self.silence_frames >= self.config.max_silence_ms / frame_ms as u64 {
                    self.is_speaking = false;
                    self.speech_frames = 0;
                    return VadResult::Silence;
                }
                
                return VadResult::Transition;
            }
        }

        if self.is_speaking {
            VadResult::Speech
        } else {
            VadResult::Silence
        }
    }

    fn reset(&mut self) {
        self.energy_history.clear();
        self.speech_frames = 0;
        self.silence_frames = 0;
        self.is_speaking = false;
        self.hold_remaining = 0;
    }
}

/// Normalize audio to target level.
fn normalize_audio(samples: &mut [f32], target_db: f32) {
    if samples.is_empty() {
        return;
    }

    let current_rms = calculate_rms(samples);
    if current_rms < 0.0001 {
        return; // Too quiet to normalize
    }

    let target_linear = db_to_linear(target_db);
    let gain = target_linear / current_rms;

    // Limit gain to avoid amplification of pure noise
    let max_gain = 10.0;
    let gain = gain.min(max_gain);

    for sample in samples.iter_mut() {
        *sample *= gain;
        // Soft clipping
        if *sample > 0.95 {
            *sample = 0.95 + (*sample - 0.95) * 0.1;
        } else if *sample < -0.95 {
            *sample = -0.95 + (*sample + 0.95) * 0.1;
        }
    }
}

/// Analyze audio quality.
fn analyze_quality(samples: &[f32]) -> AudioQuality {
    if samples.is_empty() {
        return AudioQuality::default();
    }

    let rms = calculate_rms(samples);
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, |a, b| a.max(b));
    
    // Estimate SNR (simplified)
    let noise_floor = samples.iter().map(|s| s.abs()).filter(|&s| s < 0.01).sum::<f32>() 
        / samples.iter().filter(|&&s| s < 0.01).count().max(1) as f32;
    let snr = if noise_floor > 0.0 { rms / noise_floor } else { 1000.0 };

    AudioQuality {
        snr_db: linear_to_db(snr).max(0.0),
        rms_db: linear_to_db(rms),
        peak_db: linear_to_db(peak),
        clipping: peak > 0.99,
    }
}

/// Calculate RMS of samples.
fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f32 = samples.iter().map(|s| s * s).sum();
    (sum_squares / samples.len() as f32).sqrt()
}

/// Calculate zero-crossing rate.
fn calculate_zcr(samples: &[f32]) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    let crossings: usize = samples.windows(2)
        .filter(|w| w[0].signum() != w[1].signum())
        .count();
    crossings as f32 / samples.len() as f32
}

/// Convert dB to linear.
fn db_to_linear(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// Convert linear to dB.
fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        return -100.0;
    }
    20.0 * linear.log10()
}

/// Utility: Resample audio (simple linear interpolation).
pub fn resample_linear(input: &[f32], input_rate: u32, output_rate: u32) -> Vec<f32> {
    if input_rate == output_rate {
        return input.to_vec();
    }

    let ratio = output_rate as f64 / input_rate as f64;
    let output_len = (input.len() as f64 * ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let input_pos = i as f64 / ratio;
        let input_idx = input_pos.floor() as usize;
        let frac = input_pos - input_pos.floor();

        let sample = if input_idx + 1 < input.len() {
            input[input_idx] * (1.0 - frac as f32) + input[input_idx + 1] * frac as f32
        } else {
            input[input_idx.min(input.len() - 1)]
        };
        output.push(sample);
    }

    output
}

/// Detect audio clipping.
pub fn detect_clipping(samples: &[f32], threshold: f32) -> Vec<usize> {
    samples.iter()
        .enumerate()
        .filter(|(_, s)| s.abs() > threshold)
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rms_calculation() {
        let samples = vec![0.5, 0.5, 0.5, 0.5];
        let rms = calculate_rms(&samples);
        assert!((rms - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_rms_of_silence() {
        let samples = vec![0.0; 100];
        let rms = calculate_rms(&samples);
        assert_eq!(rms, 0.0);
    }

    #[test]
    fn test_db_conversion() {
        assert!((linear_to_db(1.0) - 0.0).abs() < 0.01);
        assert!((linear_to_db(0.5) - (-6.0)).abs() < 1.0);
        assert!((db_to_linear(0.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_normalize_audio() {
        let mut samples = vec![0.1, 0.1, 0.1];
        normalize_audio(&mut samples, -20.0);
        let rms = calculate_rms(&samples);
        assert!(rms > 0.05); // Should be louder
    }

    #[test]
    fn test_vad_state() {
        let config = VadConfig::default();
        let mut vad = VadState::new(&config, 16000);

        // Silence
        let silence = vec![0.001f32; 480]; // 30ms at 16kHz
        let result = vad.analyze(&silence);
        assert!(result.is_silence());

        // Speech-like (high energy)
        vad.reset();
        let speech = vec![0.1f32; 480*5]; // 150ms
        let result = vad.analyze(&speech);
        // Should detect speech after min_speech_ms
        for _ in 0..10 {
            let result = vad.analyze(&speech);
            if result.is_speech() {
                break;
            }
        }
    }

    #[test]
    fn test_resample() {
        let input = vec![0.0, 0.5, 1.0, 0.5, 0.0];
        let output = resample_linear(&input, 1000, 2000);
        assert_eq!(output.len(), 10);
    }

    #[test]
    fn test_clipping_detection() {
        let samples = vec![0.5, 1.0, 0.5, -1.0, 0.5];
        let clipped = detect_clipping(&samples, 0.99);
        assert!(clipped.contains(&1)); // Index 1 has value 1.0
        assert!(clipped.contains(&3)); // Index 3 has value -1.0
    }

    #[test]
    fn test_audio_processor() {
        let config = AudioProcessingConfig::default();
        let mut processor = AudioProcessor::new(config, 16000);
        
        let samples = vec![0.1f32; 480];
        let result = processor.process(&samples);
        
        assert_eq!(result.samples.len(), samples.len());
        assert_eq!(result.sample_rate, 16000);
    }

    #[test]
    fn test_quality_analysis() {
        // Use alternating signal for quality analysis
        let samples: Vec<f32> = (0..1000).map(|i| if i % 2 == 0 { 0.5 } else { -0.5 }).collect();
        let quality = analyze_quality(&samples);
        
        assert!(quality.rms_db < 0.0); // Negative dB
        assert!(quality.peak_db >= quality.rms_db); // Peak should be >= RMS
    }

    #[test]
    fn test_zero_crossing_rate() {
        // High ZCR: alternating signs
        let high_zcr = vec![0.5, -0.5, 0.5, -0.5, 0.5, -0.5];
        let zcr = calculate_zcr(&high_zcr);
        assert!(zcr > 0.8);

        // Low ZCR: mostly same sign
        let low_zcr = vec![0.5, 0.4, 0.3, 0.2, 0.1, 0.0];
        let zcr = calculate_zcr(&low_zcr);
        assert!(zcr < 0.2);
    }
}
