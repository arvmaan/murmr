//! Recording pipeline for the desktop app.
//!
//! Ports the CLI's hotkey → capture → transcribe → LLM → paste flow into the
//! Tauri app, and additionally:
//!   - runs murmr's mode engine (voice-template prompts) on dictation,
//!   - emits `recording-started` / `recording-stopped` / `transcript-added` /
//!     `processing-error` events (which drive the pill and the transcript UI),
//!   - records each result into the shared transcript history.

use crate::state::{AppState, TranscriptEntry};
use murmer_core::{audio, input, llm, modes, stt};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio::sync::mpsc;

/// Register the global hotkeys and start the recording state-machine loop.
///
/// We use `tauri-plugin-global-shortcut` rather than a raw `rdev` event tap:
/// rdev's tap callback runs on a background thread and, while translating a key,
/// calls macOS Text Input Source APIs that assert they run on the main thread —
/// which crashes the app (EXC_BREAKPOINT in dispatch_assert_queue). The plugin
/// registers on the main run loop and reports Pressed/Released, which is exactly
/// what push-to-talk needs.
pub fn start(app: AppHandle, dictate: String, command: String) {
    let state = app.state::<Arc<AppState>>().inner().clone();

    // One channel for the app's lifetime. Registered shortcuts send into `tx`;
    // the state machine consumes `rx`. Re-registration (on settings save) reuses
    // this same `tx`, so changing a hotkey never orphans the loop.
    let (tx, rx) = mpsc::unbounded_channel::<input::hotkey::HotkeyEvent>();
    let _ = state.hotkey_tx.set(tx.clone());

    register_shortcuts(&app, tx, &dictate, &command);

    // Drive the state machine on Tauri's async runtime. (Using Tauri's spawn
    // rather than tokio::spawn so this works when called from `setup`, before a
    // bare tokio reactor is necessarily entered.)
    tauri::async_runtime::spawn(event_loop(app, state, rx));
}

/// Re-register the global shortcuts after a settings change. Reuses the existing
/// hotkey channel so the running state machine keeps working.
pub fn reregister(app: &AppHandle, dictate: &str, command: &str) {
    let state = app.state::<Arc<AppState>>();
    let Some(tx) = state.hotkey_tx.get().cloned() else {
        return; // loop not started yet; nothing to do
    };
    let _ = app.global_shortcut().unregister_all();
    register_shortcuts(app, tx, dictate, command);
}

/// Parse the combos and bind them on the global-shortcut plugin, forwarding
/// press/release into `tx`.
fn register_shortcuts(
    app: &AppHandle,
    tx: mpsc::UnboundedSender<input::hotkey::HotkeyEvent>,
    dictate: &str,
    command: &str,
) {
    let (dictate_sc, command_sc) = match (Shortcut::from_str(dictate), Shortcut::from_str(command))
    {
        (Ok(d), Ok(c)) => (d, c),
        (d, c) => {
            if let Err(e) = d {
                tracing::error!("invalid dictate hotkey '{}': {}", dictate, e);
            }
            if let Err(e) = c {
                tracing::error!("invalid command hotkey '{}': {}", command, e);
            }
            return;
        }
    };
    let (dictate_id, command_id) = (dictate_sc.id(), command_sc.id());

    if let Err(e) = app.global_shortcut().on_shortcuts(
        [dictate_sc, command_sc],
        move |_app, shortcut, event| {
            use input::hotkey::HotkeyEvent as E;
            let ev = match (event.state, shortcut.id()) {
                (ShortcutState::Pressed, i) if i == dictate_id => E::DictatePressed,
                (ShortcutState::Released, i) if i == dictate_id => E::DictateReleased,
                (ShortcutState::Pressed, i) if i == command_id => E::CommandPressed,
                (ShortcutState::Released, i) if i == command_id => E::CommandReleased,
                _ => return,
            };
            let _ = tx.send(ev);
        },
    ) {
        tracing::error!("failed to register global shortcuts: {}", e);
        return;
    }

    tracing::info!(
        "recording ready — dictate: {}, command: {}",
        dictate,
        command
    );
}

/// The recording state machine: press starts capture, release processes it.
async fn event_loop(
    app: AppHandle,
    state: Arc<AppState>,
    mut rx: mpsc::UnboundedReceiver<input::hotkey::HotkeyEvent>,
) {
    use input::hotkey::HotkeyEvent as E;

    let mut is_command = false;
    let mut recording_handle: Option<std::thread::JoinHandle<Vec<f32>>> = None;

    while let Some(event) = rx.recv().await {
        let recording = state.recording.load(Ordering::Relaxed);
        // Holding a key auto-repeats the Press event ~12x/sec; only the state
        // transitions below are logged, not every repeat.
        match event {
            E::DictatePressed | E::CommandPressed if !recording => {
                is_command = matches!(event, E::CommandPressed);
                state.recording.store(true, Ordering::Relaxed);
                tracing::info!(
                    "recording started ({})",
                    if is_command { "command" } else { "dictate" }
                );
                let _ = app.emit("recording-started", ());

                let stop = Arc::new(AtomicBool::new(false));
                state.record_stop.lock().await.replace(stop.clone());
                recording_handle =
                    Some(std::thread::spawn(
                        move || match audio::capture::AudioCapture::new() {
                            Ok(capture) => capture.record_until_stopped(stop).unwrap_or_default(),
                            Err(e) => {
                                tracing::error!("audio capture init failed: {}", e);
                                Vec::new()
                            }
                        },
                    ));
            }
            E::DictateReleased | E::CommandReleased if recording => {
                tracing::info!("recording stopped, processing...");
                let _ = app.emit("recording-stopped", ());

                // Signal the capture thread to stop and collect samples.
                if let Some(stop) = state.record_stop.lock().await.take() {
                    stop.store(true, Ordering::Relaxed);
                }
                let samples = match recording_handle.take() {
                    Some(h) => tokio::task::spawn_blocking(move || h.join().unwrap_or_default())
                        .await
                        .unwrap_or_default(),
                    None => Vec::new(),
                };
                state.recording.store(false, Ordering::Relaxed);

                if samples.is_empty() {
                    tracing::warn!("no audio recorded");
                    let _ = app.emit("processing-error", "no audio recorded");
                    continue;
                }

                // Process off the event loop so the next hotkey stays responsive.
                let app2 = app.clone();
                let state2 = state.clone();
                let cmd = is_command;
                tokio::spawn(async move {
                    if let Err(e) = process(&app2, &state2, cmd, samples).await {
                        tracing::error!("processing failed: {}", e);
                        let _ = app2.emit("processing-error", e.to_string());
                    }
                });
            }
            _ => {}
        }
    }
}

/// VAD-filter, transcribe, run mode engine or cleanup, paste, and record.
async fn process(
    app: &AppHandle,
    state: &Arc<AppState>,
    is_command: bool,
    samples: Vec<f32>,
) -> anyhow::Result<()> {
    let config = state.config.lock().await.clone();

    // Transcribe (VAD is optional; falls back to raw samples).
    let filtered = filter_vad(samples);
    let model_path = config.stt.model_path.clone();
    let language = config.stt.language.clone();
    let raw_text = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let whisper = stt::whisper::WhisperStt::new(&model_path, &language)?;
        Ok(whisper.transcribe(&filtered)?)
    })
    .await??;

    if raw_text.trim().is_empty() {
        tracing::info!("empty transcription");
        let _ = app.emit("processing-error", "no speech detected");
        return Ok(());
    }

    let client = state.client.lock().await.clone();
    let last_output = state
        .transcripts
        .lock()
        .await
        .first()
        .map(|t| t.cleaned_text.clone());

    // Dictation runs through the mode engine first (voice-template prompts);
    // if no trigger matches it falls back to plain cleanup. Command mode is a
    // direct LLM passthrough.
    let (final_text, mode_used) = if is_command {
        let messages = llm::prompts::command_messages(&raw_text);
        let out = client.chat(&config.llm.command_model, messages).await?;
        (out, Some("command".to_string()))
    } else {
        match modes::engine::process_dictation(&raw_text, &config, &client, last_output.as_deref())
            .await?
        {
            Some(result) => {
                let name = result.mode_name.clone();
                // Route non-paste outputs (clipboard/file/exec) as configured.
                let _ = modes::engine::route_output(&result);
                (result.text, Some(name))
            }
            None => {
                let custom = config
                    .llm
                    .cleanup_prompt
                    .as_ref()
                    .map(|p| p.system.as_str());
                let messages = llm::prompts::cleanup_messages(&raw_text, custom);
                let out = client.chat(&config.llm.cleanup_model, messages).await?;
                (out, None)
            }
        }
    };

    // Paste at the cursor.
    let paste_method = input::paste::PasteMethod::from_str(&config.paste.method);
    let to_paste = final_text.clone();
    tokio::task::spawn_blocking(move || input::paste::paste_text(&to_paste, &paste_method))
        .await??;

    // Record into history and notify the UI.
    let entry = TranscriptEntry {
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        raw_text,
        cleaned_text: final_text,
        mode_used,
    };
    {
        let mut list = state.transcripts.lock().await;
        list.insert(0, entry.clone());
        list.truncate(crate::transcripts::MAX_ENTRIES);
        crate::transcripts::save(&list);
    }
    let _ = app.emit("transcript-added", entry);

    tracing::info!("processing complete");
    Ok(())
}

/// Filter samples through Silero VAD when the model is present; otherwise pass
/// raw audio through. Mirrors the CLI behavior.
fn filter_vad(samples: Vec<f32>) -> Vec<f32> {
    let vad_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("murmer/models/silero_vad.onnx");
    if !vad_path.exists() {
        return samples;
    }
    match audio::vad::VoiceActivityDetector::new(0.5, vad_path.to_str().unwrap_or("")) {
        Ok(mut vad) => match audio::vad::filter_speech(&mut vad, &samples, 3) {
            Ok(speech) if !speech.is_empty() => speech,
            _ => samples,
        },
        Err(e) => {
            tracing::debug!("VAD init failed ({}), skipping", e);
            samples
        }
    }
}
