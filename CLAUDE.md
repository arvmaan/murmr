# murmr — Development Guide

## What is this?

A voice dictation tool in Rust. Hotkey → local whisper STT → LLM cleanup → paste at
cursor. Its signature feature is **voice-triggered prompt templates** (say "loop
this: …" to compile casual speech into a rigorous long-horizon prompt).

Two deliverables share one workspace:
- **murmer-core** — library with all the logic, plus a headless CLI (`bin: murmer`).
- **murmer-app** — Tauri v2 macOS desktop app (menu-bar agent + recording pill).

The LLM backend is pluggable (Bedrock / Anthropic / OpenAI-compatible / Ollama),
auto-detected from config. Development currently targets macOS + Bedrock; the CLI and
core still work cross-platform and with Ollama.

## Build & Test

```bash
cargo build                              # debug build
cargo test -p murmer-core --no-default-features   # core tests
cargo clippy
cargo fmt --check

# Desktop app:
cargo tauri dev --features bedrock       # dev
cargo tauri build --features bedrock     # .app + .dmg
```

Note: 5 tests in `input::paste` read `WAYLAND_DISPLAY`/`DISPLAY` env vars and fail on
macOS regardless of changes — this is a known environment artifact, not a regression.

## Architecture

```
crates/
  murmer-core/src/
    audio/{capture,vad}.rs   # cpal capture (16kHz mono f32), Silero VAD (optional)
    stt/whisper.rs           # whisper-rs transcription
    llm/{client,prompts}.rs  # multi-protocol LLM client + system prompts
    modes/                   # voice-template engine:
      registry.rs            #   built-in modes + trigger matching
      extractor.rs           #   LLM slot extraction + fill + unfilled-slot safety net
      context.rs             #   {{context:...}} resolution (clipboard, git diff, file, shell)
      engine.rs              #   orchestrates extract → fill → resolve → strip
    dictionary/              # adaptive vocabulary
    input/{hotkey,paste}.rs  # rdev hotkeys; wtype/xdotool/pbcopy+osascript paste
    config.rs                # TOML config; config_path() prefers ~/.config/murmer
  murmer-app/src/
    main.rs                  # Tauri entry, tray, pill window, activation policy, reopen
    recording.rs             # hotkey (global-shortcut plugin) → capture → transcribe → LLM → paste
    commands.rs              # IPC commands (get/save config, transcripts, dictionary, modes, check)
    state.rs                 # shared AppState (Mutex-wrapped)
    transcripts.rs           # transcript history persistence (~/.config/murmer/transcripts.json)
ui/                          # vanilla HTML/CSS/JS, no build step
  index.html style.css app.js   # main window
  pill.html  pill.css  pill.js   # recording pill overlay
```

## Key design decisions & gotchas

1. **Hotkeys use `tauri-plugin-global-shortcut`, NOT rdev, in the app.** rdev's event
   tap runs on a background thread and calls macOS Text Input Source APIs that assert
   main-thread → crash. The plugin registers on the main run loop and reports
   Pressed/Released (needed for push-to-talk). The core's rdev listener is still used
   by the CLI.
2. **Tray callbacks fire off the main thread on macOS.** Any window/activation-policy
   call from a tray handler must go through `run_on_main_thread`, or it silently
   no-ops. See `show_main_window` / `show_pill`.
3. **Menu-bar agent.** `LSUIElement` + `ActivationPolicy::Accessory`. `show_main_window`
   flips to `Regular` so the window can surface; closing drops back to `Accessory`.
   A `Reopen` handler re-shows the window when the app is opened again from Finder.
4. **The pill is a persistent second window** (transparent, always-on-top,
   non-focusable), shown/hidden by the backend. Its JS resets the timer on the
   `visibilitychange` event (not at load) so it always starts at 0:00. Backend emits
   pill-only events (`pill:record`/`pill:process`) distinct from the recording
   lifecycle events to avoid a feedback loop.
5. **Config path.** `config_path()` prefers `~/.config/murmer/config.toml` even on
   macOS (where `dirs::config_dir()` would point at Application Support), because
   that's the documented location and keeps the CLI and app in sync.
6. **Prompt templates never leak `{{slots}}`.** The extractor fills every declared
   slot (with fallbacks) and `strip_unfilled_placeholders` runs last as a safety net.
7. **AWS creds are read at startup.** If Bedrock calls fail with "service error",
   refresh credentials and relaunch the app.
8. **GUI apps discard stderr** — logs go to `~/Library/Logs/murmr/murmr.log`.

## Feature flags (murmer-core)

`audio`, `stt`, `hotkeys`, `vad`, `tray` (Linux), `bedrock`. The app enables
`audio,stt,hotkeys` by default and `bedrock` when built `--features bedrock`.
