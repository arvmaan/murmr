# murmer Implementation — Ralph Loop Prompt

Use this with: `/ralph-loop` in the murmer project directory.

## The prompt to feed:

```
Implement murmer — a local voice dictation tool for Linux.

## Context

You are in the murmer project directory. Read CLAUDE.md and README.md for full context. The skeleton is already scaffolded with module stubs containing TODO markers. Your job is to replace every TODO with working implementations, add tests, and ensure `cargo build --release` and `cargo test` pass cleanly.

## Implementation Order (do these in sequence, verifying each phase builds before moving on)

### Phase 1: Core plumbing (no hardware needed)
1. `src/config.rs` — already done, verify tests pass
2. `src/llm/client.rs` — finish the Ollama HTTP client. Add integration test behind `#[cfg(feature = "integration")]` that hits a real Ollama if available, plus unit tests with mockito for the happy path and error cases
3. `src/llm/prompts.rs` — already done, verify tests pass
4. `src/input/paste.rs` — finish paste implementation. Unit test the PasteMethod detection logic (mock env vars)

### Phase 2: Audio + STT
5. `src/audio/capture.rs` — implement cpal capture. Use default input device, 16kHz mono f32. Provide a `record_until_stopped()` async method that collects samples into a Vec<f32> and returns them when signaled to stop (via a tokio::sync::oneshot or AtomicBool). Add cfg-gated test that records 1 second of silence.
6. `src/audio/vad.rs` — implement Silero VAD. Download the silero_vad.onnx model on first run (or document where to get it). Use the `ort` crate. The VAD should process 512-sample chunks and return speech probability. Add unit test with synthetic silence (all zeros → no speech).
7. `src/stt/whisper.rs` — implement via whisper-rs. Load model from config path, transcribe f32 samples, return String. Add error handling for missing model file (already started). Integration test behind feature flag.

### Phase 3: Input + Orchestration
8. `src/input/hotkey.rs` — implement using `rdev` for global key listening. Parse hotkey strings like "Super+Shift+D" into key combinations. Detect press/release events for push-to-talk. Add unit tests for hotkey string parsing.
9. `src/tray.rs` — implement system tray with ksni. Show recording state (idle/recording/processing). Menu with Quit item.
10. `src/main.rs` — wire everything together into an event loop state machine:
    - State: Idle → Recording → Processing → Idle
    - On dictate hotkey press: start recording
    - On dictate hotkey release: stop recording → run VAD filter → transcribe → LLM cleanup → paste
    - On command hotkey press/release: same but use command mode prompt
    - Handle errors gracefully (log and continue, don't crash)

### Phase 4: Polish
11. Add a `--check` CLI flag that validates the setup (Ollama reachable, model exists, paste tool available) and prints a status report
12. Add a model download helper: `murmer --download-model base.en` that fetches the whisper GGUF from huggingface
13. Ensure `cargo clippy` passes with no warnings
14. Ensure `cargo fmt --check` passes
15. Run `cargo test` — all tests must pass

## Quality Standards

- Every public function has a doc comment
- Every module has at least one unit test
- Error messages are actionable ("whisper model not found at X. Run: murmer --download-model")
- No unwrap() in non-test code — use anyhow/thiserror for error propagation
- No unsafe code unless absolutely required (and document why)
- Follow Rust 2021 idioms (impl blocks, ? operator, iterators over manual loops)

## Verification

After each phase, run:
```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
cargo build --release
```

Fix any issues before moving to the next phase.

## Completion Criteria

Output <promise>DONE</promise> when ALL of the following are true:
- `cargo build --release` succeeds
- `cargo test` passes (all tests green)
- `cargo clippy -- -D warnings` passes
- No TODO markers remain in source files (grep -r "todo!" src/ returns nothing)
- The binary runs: `./target/release/murmer --help` prints usage
- The `--check` flag works: `./target/release/murmer --check` reports status

If you get stuck on a hardware-dependent feature (audio capture, hotkeys) that can't work in this environment, implement it as completely as possible, mark the integration test with appropriate cfg flags, and move on. The code should compile and the non-hardware tests should pass.
```

## Usage

```bash
cd /local/home/arvinmaa/murmer
/ralph-loop "<paste the prompt above>" --max-iterations 30 --completion-promise "DONE"
```

Or more concisely, reference this file:

```bash
cd /local/home/arvinmaa/murmer
/ralph-loop "$(cat RALPH_PROMPT.md | sed -n '/^```$/,/^```$/{ /^```$/d; p; }' | head -n -0)" --max-iterations 30 --completion-promise "DONE"
```
