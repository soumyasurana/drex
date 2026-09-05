# Drex Voice

Speech-to-text and text-to-speech for Drex agent - local, private, offline.

## Features

- **STT**: Whisper-based speech recognition - uses local models, never sends audio to the cloud
- **TTS**: Local text-to-speech using system voices
- **Voice Loop**: Continuous conversational mode - listen, process, speak, repeat
- **Privacy First**: All processing happens on-device

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
