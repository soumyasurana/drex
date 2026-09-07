# Drex Voice

Speech-to-text and text-to-speech for Drex agent - local, private, offline.

## Features

- **STT**: Whisper-based speech recognition - uses local models, never sends audio to the cloud
- **TTS**: Local text-to-speech using system voices
- **Voice Loop**: Continuous conversational mode - listen, process, speak, repeat
- **Wake Word Detection**: Listen for activation phrases like "Hey Drex" or "Jarvis"
- **Privacy First**: All processing happens on-device

## Wake Word Detection

The voice system supports always-on wake word detection for hands-free activation:

### Default Wake Words

- "Hey Drex" (primary)
- "Drex"
- "Jarvis"
- "Computer"

### Configuration

```rust
use drex_voice::wake_word::{WakeWordDetector, WakeWordConfig};

let config = WakeWordConfig {
    wake_phrase: "Hey Drex".to_string(),
    alternative_phrases: vec!["Drex".to_string(), "Jarvis".to_string()],
    vad_threshold: 0.02,  // Energy threshold for speech detection
    fuzzy_match: true,    // Enable fuzzy matching for imperfect recognition
    similarity_threshold: 0.7,  // Minimum similarity score (0.0 to 1.0)
    ..WakeWordConfig::default()
};

let mut detector = WakeWordDetector::new(config)?;
let result = detector.start().await?;
println!("Detected: {} (confidence: {:.2})", result.phrase, result.confidence);
```

### Features

- **Voice Activity Detection (VAD)**: Energy-based speech detection
- **Fuzzy Matching**: Tolerates minor variations in pronunciation
- **Multiple Wake Words**: Configure multiple activation phrases
- **Low CPU Usage**: Efficient processing when idle

## Usage

```rust
use drex_voice::{VoiceLoop, VoiceLoopConfig, create_voice_loop};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create voice loop with default config
    let voice_loop = create_voice_loop()?;

    // Run with your processing function
    voice_loop.run(|user_input| async move {
        // Process input and return response
        Ok(format!("You said: {}", user_input))
    }).await?;

    Ok(())
}
```

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Microphone│────▶│ AudioCapture│────▶│    STT      │
│             │     │   (cpal)    │     │  (Whisper)  │
└─────────────┘     └─────────────┘     └──────┬──────┘
                                               │
                                               ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Speakers  │◀────│    TTS      │◀────│   Agent     │
│             │     │ (System)    │     │   Response  │
└─────────────┘     └─────────────┘     └─────────────┘
```

## Configuration

The voice system requires a Whisper model file. Set the path in configuration:

```rust
let config = VoiceLoopConfig {
    stt_config: SttConfig {
        model_path: Some(PathBuf::from("/path/to/whisper/base.bin")),
        language: "en".to_string(),
        ..Default::default()
    },
    ..Default::default()
};
```

## Voice Commands

Once activated, you can speak naturally to Drex. Say one of these to stop:
- "Stop"
- "Quit"
- "Exit"
- "Goodbye"
- "That's all"

## Testing

```bash
cargo test -p drex-voice
```

## Dependencies

The STT backend is based on Whisper (via whisper-rs):
- Run entirely locally
- No cloud API calls
- Supports 99 languages

The TTS backend uses local system text-to-speech APIs via the `tts` crate.
