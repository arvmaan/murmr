# murmer

Voice-to-text that just works, locally.

A single native binary for Linux that turns speech into clean, formatted text at your cursor. No Electron, no cloud, no meetings, no notes. Just press a key, talk, and get polished text.

## How it works

```
[Hotkey] → Record → whisper.cpp STT → Ollama LLM cleanup → Paste at cursor
```

**Cleanup mode**: Press hotkey, speak naturally, release. murmer removes fillers ("um", "uh", "like"), fixes punctuation and capitalization, normalizes numbers, and pastes clean text where your cursor is.

**Command mode**: Press a different hotkey, speak an instruction ("translate this to Spanish", "summarize the above", "rewrite more formally"), and murmer sends your instruction to the LLM and pastes the result.

## Design principles

- **Local only** — zero network calls, ever. Audio never leaves your machine.
- **Lightweight** — single Rust binary (~15MB). No Electron, no bundled runtime.
- **Linux-first** — Wayland and X11, PipeWire and PulseAudio. Tested on GNOME, KDE, Hyprland.
- **Fast** — persistent Ollama connection, tiny cleanup model (0.6–1.7B), sub-second latency.
- **Single-purpose** — dictation and paste. That's it.
- **Work-safe** — no telemetry, no analytics, no cloud integrations, auditable source.

## Prerequisites

- [Ollama](https://ollama.ai) running locally with a model pulled:
  ```bash
  ollama pull qwen3:1.7b    # recommended for cleanup
  ollama pull phi4-mini      # recommended for command mode
  ```
- A whisper.cpp compatible GGUF model (murmer downloads whisper-base by default on first run)
- Linux with PipeWire or PulseAudio
- `wtype` (Wayland) or `xdotool` (X11) for paste-at-cursor

## Installation

```bash
# From source
cargo install --path .

# Or build manually
cargo build --release
./target/release/murmer
```

## Configuration

Config lives at `~/.config/murmer/config.toml`:

```toml
[hotkeys]
dictate = "Super+Shift+D"      # Push-to-talk for dictation
command = "Super+Shift+C"      # Push-to-talk for command mode

[stt]
model_path = "~/.local/share/murmer/models/ggml-base.en.bin"
language = "en"

[llm]
endpoint = "http://localhost:11434"
cleanup_model = "qwen3:1.7b"
command_model = "phi4-mini"

[llm.cleanup_prompt]
system = """Clean up this dictated text. Remove filler words, fix punctuation and capitalization, normalize numbers. Do NOT change meaning or add content. Output only the cleaned text."""

[paste]
method = "auto"  # auto-detects wayland vs x11
```

## Architecture

```
src/
├── main.rs           # Entry point, tray setup, event loop
├── audio/
│   ├── capture.rs    # cpal audio capture (PipeWire/PulseAudio/ALSA)
│   └── vad.rs        # Silero VAD via ort (ONNX Runtime)
├── stt/
│   └── whisper.rs    # whisper-rs bindings to whisper.cpp
├── llm/
│   ├── client.rs     # Ollama HTTP client (reqwest)
│   └── prompts.rs    # Cleanup and command system prompts
├── input/
│   ├── hotkey.rs     # Global hotkey capture (evdev/rdev)
│   └── paste.rs      # Paste-at-cursor (wtype/xdotool)
├── config.rs         # TOML config parsing
└── tray.rs           # System tray indicator (ksni)
```

## License

MIT
