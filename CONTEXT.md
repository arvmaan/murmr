# murmer — Context for Local Development

## What is this?

murmer is a local voice dictation tool. You speak into your mic, it transcribes via whisper.cpp, cleans up via LLM, and pastes at your cursor. The differentiating feature is **voice-triggered prompt templates** — say "loop this: get tests passing" and it compiles your casual speech into a rigorous long-horizon prompt with success predicates, non-counting outcomes, and verification gates.

Tagline: "Speak sloppy, prompt sharp."

## Current State (as of 2026-07-17)

### Working end-to-end on Mac:
- Super+Shift+K (hold) → speak → release → whisper transcribes → Bedrock cleans up → pastes at cursor via pbcopy + osascript Cmd+V
- Super+Shift+L for command mode
- Config at `~/.config/murmer/config.toml` (pass with `-c` flag since macOS dirs::config_dir() points to ~/Library/Application Support/)
- Uses AWS Bedrock (us-west-2, account 009516626117) for LLM
- Also supports Ollama, OpenAI-compatible, and Anthropic APIs

### Repo Structure (Cargo workspace):
```
crates/
  murmer-core/     — library crate, all the logic (117 tests pass)
    src/
      lib.rs       — pub exports all modules
      cli.rs       — CLI binary (bin target)
      audio/       — cpal capture, VAD
      stt/         — whisper-rs
      llm/         — LlmClient (Ollama, OpenAI, Anthropic, Bedrock protocols)
      modes/       — prompt template engine (registry, extractor, context, engine)
      dictionary/  — adaptive vocabulary learning
      input/       — hotkeys (rdev), paste (wtype/xdotool/pbcopy+osascript)
      config.rs    — TOML config with all settings
      tray.rs      — system tray (ksni, Linux only)
  murmer-app/      — Tauri v2 desktop app (NOT YET BUILDING)
    src/
      main.rs      — Tauri entry, tray, setup
      commands.rs  — IPC commands (get/save config, transcripts, dictionary, modes, check)
      state.rs     — shared app state
    tauri.conf.json
    Entitlements.plist
ui/                — vanilla HTML/CSS/JS frontend
  index.html       — two tabs: Transcripts, Settings
  style.css        — dark theme
  app.js           — Tauri invoke calls, event listeners
```

### What needs to happen next (Tauri app):
1. Run `cargo tauri dev --features bedrock` — will likely have compile errors from Tauri v2 API specifics
2. Fix errors iteratively until the window opens
3. Wire up global hotkey registration in Tauri (using tauri-plugin-global-shortcut)
4. Wire up the recording flow: hotkey press → start audio capture thread → hotkey release → stop → process → emit event → UI updates
5. Generate placeholder icons for the tray
6. Test the full flow: tray icon appears, hotkey records, transcript shows in UI

### Build Commands:
```bash
# CLI only (works today):
cargo run -p murmer-core --bin murmer --features bedrock -- -c ~/.config/murmer/config.toml

# Tauri app (needs fixing):
cargo install tauri-cli --version "^2"
cargo tauri dev --features bedrock
cargo tauri build --features bedrock   # produces .dmg
```

### Config (~/. config/murmer/config.toml):
```toml
[llm]
protocol = "bedrock"
region = "us-west-2"
cleanup_model = "us.anthropic.claude-haiku-4-5-20251001-v1:0"
command_model = "us.anthropic.claude-sonnet-4-20250514-v1:0"

[hotkeys]
dictate = "Super+Shift+K"
command = "Super+Shift+L"

[dictionary]
entries = { "MP" = "MetricsProcessor", "LPCP" = "LogProcessingControlPlane" }
```

### Key Design Decisions:
- Feature flags for hardware deps: `audio`, `stt`, `hotkeys`, `tray`, `bedrock`
- LlmClient auto-detects protocol from endpoint URL or explicit config
- No VAD filtering when Silero ONNX model not present (just passes raw audio to whisper)
- macOS paste: pbcopy + osascript simulating Cmd+V (needs Accessibility permission)
- Recording happens in a thread spawned on key PRESS, stopped on key RELEASE
- Template modes: trigger phrases matched case-insensitively, slots extracted by LLM into JSON, templates filled, context vars resolved

### Voice Template Modes (built-in):
- **loop** — "loop this: ..." → persistence-gated brief
- **review** — "review this: ..." → adversarial review brief
- **spec** — "spec this: ..." → pseudo-formal specification
- **fan** — "fan out: ..." → parallel search orchestration
- **command** — "translate/summarize/rewrite/explain ..." → direct LLM passthrough

### GitHub: github.com/arvmaan/murmr (private)

### Ralph loop prompt for Tauri fixing:
```
/ralph-loop:ralph-loop "Fix the Tauri v2 build errors in murmer-app. Run cargo tauri dev --features bedrock and fix whatever fails until the app window opens with the settings and transcript UI. The recording flow should work: hotkey press starts recording, release stops it, processes through whisper and LLM, pastes result and adds to transcript list. System tray icon should appear. Read .ralph-prompt-tauri for full spec." --max-iterations 20 --completion-promise DONE
```
