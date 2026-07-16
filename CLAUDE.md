# murmer — Development Guide

## What is this?

A minimal, native Linux-first voice dictation tool in Rust. Hotkey → local whisper.cpp STT → local Ollama LLM cleanup → paste at cursor. Single binary, no Electron, no cloud.

## Build & Test

```bash
cargo build            # debug build
cargo build --release  # release build (LTO, stripped)
cargo test             # run all tests
cargo clippy           # lints
cargo fmt --check      # formatting check
```

## Architecture

The app is structured as independent modules that compose in main.rs:

- `audio/capture.rs` — cpal-based mic capture (16kHz mono f32)
- `audio/vad.rs` — Silero VAD via ONNX runtime (filters silence)
- `stt/whisper.rs` — whisper-rs bindings for local transcription
- `llm/client.rs` — Ollama HTTP API client (chat completions)
- `llm/prompts.rs` — system prompts for cleanup and command modes
- `input/hotkey.rs` — global hotkey capture (evdev/rdev)
- `input/paste.rs` — paste-at-cursor via wtype (Wayland) or xdotool (X11)
- `config.rs` — TOML config loading with sensible defaults
- `tray.rs` — system tray indicator (ksni)
- `main.rs` — orchestrates everything: event loop, state machine

## Key Design Decisions

1. **Ollama as LLM backend** — we don't embed an LLM. We talk to Ollama over HTTP. This keeps the binary small and lets users choose their model.
2. **Push-to-talk** — hotkey press starts recording, release stops. No always-on listening.
3. **Two modes** — dictate (cleanup + paste) and command (instruction → LLM → paste result).
4. **Clipboard-based paste** — copy to clipboard, then simulate Ctrl+V. More reliable than character-by-character typing across different apps.
5. **No GUI settings** — config.toml is the interface. Power users only for now.

## Testing Strategy

- Unit tests for config parsing, prompt construction, Ollama client (with mockito)
- Integration tests for the full pipeline (mock audio → STT → LLM → paste) 
- The audio, hotkey, and paste modules need real hardware so their tests are behind `#[cfg(test)]` feature flags or are manual

## Dependencies to have installed for development

- Rust toolchain (rustup)
- System libs: `alsa-lib-devel` (or `libasound2-dev`), `pkg-config`
- For testing paste: `wtype`, `wl-clipboard` (Wayland) or `xdotool`, `xclip` (X11)
- Ollama running locally for integration tests
