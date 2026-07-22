# murmr

**Speak sloppy, prompt sharp.**

murmr turns a rambled, half-formed thought into a **sharp, structured prompt** you can
paste straight into a coding agent like Claude. Hold a hotkey, talk, release — murmr
transcribes locally with whisper and compiles your speech into a well-formed prompt.

The point is **not** to do the work for you. It's to build the prompt that gets the
work done well. You stay the driver; murmr just makes your ask precise.

### The core idea: talk → get a prompt, not an answer

Hold the **command** hotkey and mumble a request:

> _"can you help me figure out why the login page is really slow, i think it's the
> api calls but not sure, dig into it and fix it"_

murmr doesn't try to debug anything. It hands you a prompt, ready to paste into Claude:

```
TASK: Investigate and fix the performance issues on the login page, with focus on
API call optimization.
CONTEXT: The login page is slow; API calls are suspected as the primary bottleneck
but the root cause needs confirmation.
CONSTRAINTS: Preserve existing login functionality and security. Don't break the auth
flow beyond the performance improvements.
DELIVERABLE: A faster login page with the bottleneck identified and resolved, plus
before/after measurements.
```

### Voice-triggered prompt templates

Start your speech with a **trigger phrase** and murmr compiles it into a rigorous,
purpose-built prompt. For example, say:

> _"loop this: get the integration tests passing"_

and murmr produces a persistence-gated long-horizon brief:

```
OBJECTIVE: Get the integration tests passing.
SUCCESS PREDICATE: Every test in the integration suite passes on a clean run. This
is a property of the finished artifact, not of your confidence in it.
DOES NOT COUNT:
- Deleting, skipping, or weakening tests to make the suite green.
- A pass you cannot reproduce on a fresh run.
- Fixing some tests while leaving others broken.
VERIFICATION: Re-run the full suite after each fix. A flaky pass does not count.
PERSISTENCE: Assume a solution exists. Do not stop because it's hard or slow; stop
only when the predicate holds under verification.
RETURN: Return only the passing suite — no partial progress, plans, or excuses.
```

Built-in modes: **loop** (persistence), **review** (adversarial audit), **spec**
(specification), **fan** (parallel search), and **command** (general task prompt). Add
your own in Settings.

## How it works

```
[Hotkey] → Record → whisper STT → LLM (compile to prompt) → Clipboard + paste at cursor
```

- **Command** (`Super+Shift+L`): speak any task; murmr compiles it into a structured
  TASK / CONTEXT / CONSTRAINTS / DELIVERABLE prompt. It never does the task itself.
- **Dictate** (`Super+Shift+K`): plain dictation — strips fillers, fixes
  punctuation/capitalization, honors self-corrections. If your speech starts with a
  mode trigger ("loop this", "review this"…), it compiles that template instead.

While recording, a **pill** drops from the top of the screen with a live waveform and
timer; it switches to "Transcribing…" while the LLM works, then copies the result to
your clipboard.

## Download & install (macOS)

There's no notarized release yet, so you build the app locally (one command) and
grant it two permissions. Takes about five minutes.

```bash
# 1. Prerequisites (one-time)
#    - Rust:        https://rustup.rs
#    - Tauri CLI:   cargo install tauri-cli --version "^2"

# 2. Clone and download a whisper model (~150 MB for base.en)
git clone https://github.com/arvmaan/murmr.git && cd murmr
cargo run -p murmer-core --bin murmer --features bedrock -- --download-model base.en

# 3. Create your config at ~/.config/murmer/config.toml (see Configuration below)

# 4. Build, sign, and install the app to /Applications
./scripts/bundle-macos.sh --install
```

### Grant permissions (required, one-time)

murmr is a menu-bar app. On first launch it shows a welcome banner listing the two
permissions it needs — grant them in **System Settings → Privacy & Security**, then
**quit and relaunch murmr** (macOS only reads these at launch):

| Permission | Why |
|------------|-----|
| **Input Monitoring** | detect the global hotkey |
| **Accessibility** | auto-paste at your cursor (optional — see note) |
| **Microphone** | record your voice (prompted automatically) |

> **Note on auto-paste:** because this is a locally-signed build, macOS may not honor
> Accessibility for the synthetic ⌘V after a rebuild. That's fine — murmr always
> copies the transcript to your **clipboard**, so you can just press **⌘V** wherever
> you want it. Auto-paste is a convenience, not a requirement.

Then hold **⌘⇧K**, speak, and release. See [INSTALL.md](INSTALL.md) for the full
guide and troubleshooting.

## Voice template modes

Built-in modes match a trigger phrase at the start of your speech, then compile the
rest into a rigorous prompt:

| Mode | Triggers (start of speech) | Turns speech into… |
|------|----------|--------------------|
| **loop** | "loop this", "ralph this", "iterate on" | a persistence-gated brief with a success predicate + verification gate |
| **review** | "review this", "audit" | an adversarial review brief with a failure-mode checklist |
| **spec** | "spec this", "specify" | a pseudo-formal specification (definitions, predicate, non-counting outcomes) |
| **fan** | "fan out", "parallel" | a diverse parallel-search orchestration brief |

The **command** hotkey (`Super+Shift+L`) is the general case — it compiles any spoken
task into a TASK / CONTEXT / CONSTRAINTS / DELIVERABLE prompt without needing a
trigger word, and never executes the task.

Modes are plain config — override a built-in or add your own in Settings (or
`config.toml`).

## LLM backends

murmr auto-detects the protocol from your config. Supported:

- **AWS Bedrock** — uses your AWS credentials (no API key), just set the region
- **Anthropic** — API key
- **OpenAI-compatible** — endpoint + API key
- **Ollama** — local, no key (fully offline with local whisper)

## Repo layout (Cargo workspace)

```
crates/
  murmer-core/            # library: all the logic; also ships a headless CLI (bin: murmer)
    src/
      audio/              # cpal capture, Silero VAD
      stt/                # whisper-rs transcription
      llm/                # LlmClient (Ollama/OpenAI/Anthropic/Bedrock) + prompts
      modes/              # voice-template engine: registry, extractor, context, engine
      dictionary/         # adaptive vocabulary learning
      input/              # hotkeys (rdev), paste (wtype/xdotool/pbcopy+osascript)
      config.rs           # TOML config
  murmer-app/             # Tauri v2 desktop app (macOS)
    src/
      main.rs             # entry, tray, pill window, reopen handling
      recording.rs        # hotkey → capture → transcribe → LLM → paste pipeline
      commands.rs         # IPC commands for the UI
      state.rs            # shared app state
      transcripts.rs      # transcript history persistence
ui/                       # vanilla HTML/CSS/JS frontend (no build step)
  index.html style.css app.js   # main window (Transcripts / Settings)
  pill.html  pill.css  pill.js   # recording pill overlay
```

## Building

```bash
# Headless CLI (works cross-platform):
cargo run -p murmer-core --bin murmer --features bedrock -- -c ~/.config/murmer/config.toml

# macOS desktop app (dev):
cargo tauri dev --features bedrock

# macOS desktop app bundle (.app + .dmg):
cargo tauri build --features bedrock
```

See [INSTALL.md](INSTALL.md) for the full macOS install + permissions guide.

## Configuration

Config lives at `~/.config/murmer/config.toml` (used on macOS too — the app prefers
this XDG-style path). Transcript history persists to `~/.config/murmer/transcripts.json`.

```toml
[llm]
protocol = "bedrock"          # bedrock | anthropic | openai | ollama
region = "us-west-2"          # bedrock
cleanup_model = "us.anthropic.claude-haiku-4-5-20251001-v1:0"
command_model = "us.anthropic.claude-sonnet-4-20250514-v1:0"

[hotkeys]
dictate = "Super+Shift+K"
command = "Super+Shift+L"

[stt]
model_path = ""               # defaults to ~/Library/Application Support/murmer/models/ggml-base.en.bin on macOS
language = "en"

[dictionary]
entries = { "k8s" = "Kubernetes", "pg" = "Postgres" }
```

## Design principles

- **Local STT** — audio is transcribed on-device with whisper.
- **Bring your own LLM** — cloud (Bedrock/Anthropic/OpenAI) or fully local (Ollama).
- **Push-to-talk** — hold to record, release to process. No always-on listening.
- **Prompt templates as a first-class voice primitive** — the signature feature.

## License

MIT
