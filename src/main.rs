#[allow(dead_code)]
mod audio;
#[allow(dead_code)]
mod config;
#[allow(dead_code)]
mod dictionary;
#[allow(dead_code)]
mod input;
#[allow(dead_code)]
mod llm;
#[allow(dead_code)]
mod modes;
#[allow(dead_code)]
mod stt;
#[allow(dead_code)]
mod tray;

use anyhow::Result;
use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

/// The application state machine.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum AppState {
    Idle,
    Recording { is_command: bool },
    Processing,
}

#[derive(Parser, Debug)]
#[command(name = "murmer", about = "Local voice-to-text with LLM cleanup")]
struct Cli {
    /// Path to config file (default: ~/.config/murmer/config.toml)
    #[arg(short, long)]
    config: Option<String>,

    /// Run in verbose mode
    #[arg(short, long)]
    verbose: bool,

    /// Check system readiness (Ollama, models, paste tools)
    #[arg(long)]
    check: bool,

    /// Download a whisper model by name (e.g., "base.en", "small", "medium")
    #[arg(long, value_name = "MODEL")]
    download_model: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let filter = if cli.verbose {
        EnvFilter::new("murmer=debug")
    } else {
        EnvFilter::new("murmer=info")
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = config::load(cli.config.as_deref())?;
    tracing::debug!(?config, "loaded configuration");

    if let Some(model_name) = cli.download_model {
        return download_model(&model_name).await;
    }

    if cli.check {
        return check_system(&config).await;
    }

    run_app(config).await
}

/// Main application loop: listens for hotkeys, records, transcribes, cleans up, and pastes.
async fn run_app(config: config::Config) -> Result<()> {
    tracing::info!("murmer starting up");

    // Initialize Ollama client
    let endpoint = if config.llm.protocol.as_deref() == Some("bedrock") {
        format!(
            "bedrock:{}",
            config.llm.region.as_deref().unwrap_or("us-east-1")
        )
    } else {
        config.llm.endpoint.clone()
    };
    let ollama = llm::client::LlmClient::new(
        &endpoint,
        config.llm.api_key.as_deref(),
        config.llm.protocol.as_deref(),
    );
    match ollama.health_check().await {
        Ok(true) => tracing::info!("ollama connected at {}", config.llm.endpoint),
        Ok(false) => tracing::warn!("ollama returned non-success status"),
        Err(e) => tracing::warn!("ollama not reachable: {}. LLM cleanup will fail.", e),
    }

    // Initialize paste method
    let paste_method = input::paste::PasteMethod::from_str(&config.paste.method);
    tracing::debug!("paste method: {:?}", paste_method);

    // Set up system tray
    let (_tray, tray_handle) = tray::SystemTray::new()?;
    let tray_handle_clone = tray_handle.clone();

    // Spawn tray in background thread
    std::thread::spawn(move || {
        let (tray_inst, _) = tray::SystemTray::new().unwrap_or_else(|e| {
            tracing::error!("failed to create system tray: {}", e);
            std::process::exit(1);
        });
        if let Err(e) = tray_inst.run() {
            tracing::error!("system tray error: {}", e);
        }
    });

    // Set up hotkey listener
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<input::hotkey::HotkeyEvent>();
    let hotkey_listener =
        input::hotkey::HotkeyListener::new(&config.hotkeys.dictate, &config.hotkeys.command)?;

    // Spawn hotkey listener in background thread
    std::thread::spawn(move || {
        if let Err(e) = hotkey_listener.listen(move |event| {
            let _ = event_tx.send(event);
        }) {
            tracing::error!("hotkey listener error: {}", e);
        }
    });

    tracing::info!("murmer ready — press your hotkey to dictate");

    // Event loop
    let mut state = AppState::Idle;
    let stop_signal: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let mut recording_handle: Option<std::thread::JoinHandle<Vec<f32>>> = None;

    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match (&state, event) {
                    (AppState::Idle, input::hotkey::HotkeyEvent::DictatePressed) => {
                        tracing::info!("recording started (dictation mode)");
                        state = AppState::Recording { is_command: false };
                        tray_handle.set_state(tray::TrayState::Recording);
                        stop_signal.store(false, Ordering::Relaxed);

                        // Start recording immediately in a background thread
                        let stop = stop_signal.clone();
                        recording_handle = Some(std::thread::spawn(move || {
                            let capture = audio::capture::AudioCapture::new().unwrap();
                            capture.record_until_stopped(stop).unwrap_or_default()
                        }));
                    }
                    (AppState::Idle, input::hotkey::HotkeyEvent::CommandPressed) => {
                        tracing::info!("recording started (command mode)");
                        state = AppState::Recording { is_command: true };
                        tray_handle.set_state(tray::TrayState::Recording);
                        stop_signal.store(false, Ordering::Relaxed);

                        // Start recording immediately
                        let stop = stop_signal.clone();
                        recording_handle = Some(std::thread::spawn(move || {
                            let capture = audio::capture::AudioCapture::new().unwrap();
                            capture.record_until_stopped(stop).unwrap_or_default()
                        }));
                    }
                    (AppState::Recording { is_command }, input::hotkey::HotkeyEvent::DictateReleased | input::hotkey::HotkeyEvent::CommandReleased) => {
                        let is_command_mode = *is_command;
                        tracing::info!("recording stopped, processing...");
                        tray_handle.set_state(tray::TrayState::Processing);

                        // Signal recording to stop
                        stop_signal.store(true, Ordering::Relaxed);

                        // Collect recorded samples from the thread
                        let samples = match recording_handle.take() {
                            Some(handle) => handle.join().unwrap_or_default(),
                            None => Vec::new(),
                        };

                        if samples.is_empty() {
                            tracing::warn!("no audio recorded");
                            tray_handle.set_state(tray::TrayState::Idle);
                            state = AppState::Idle;
                            continue;
                        }

                        tracing::info!("recorded {} samples ({:.1}s)", samples.len(), samples.len() as f64 / 16000.0);

                        // Process in background
                        let config_clone = config.clone();
                        let ollama_clone = ollama.clone();
                        let paste_clone = paste_method.clone();
                        let tray_clone = tray_handle_clone.clone();

                        tokio::spawn(async move {
                            match process_samples(&config_clone, &ollama_clone, &paste_clone, is_command_mode, samples).await {
                                Ok(()) => tracing::debug!("processing complete"),
                                Err(e) => tracing::error!("processing failed: {}", e),
                            }
                            tray_clone.set_state(tray::TrayState::Idle);
                        });

                        state = AppState::Idle;
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutting down");
                break;
            }
        }
    }

    Ok(())
}

/// Process recorded samples: VAD filter, transcribe, LLM cleanup/mode, paste.
async fn process_samples(
    config: &config::Config,
    ollama: &llm::client::LlmClient,
    paste_method: &input::paste::PasteMethod,
    is_command: bool,
    samples: Vec<f32>,
) -> Result<()> {
    // Filter with VAD
    let vad_model_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("murmer/models/silero_vad.onnx");
    let filtered =
        match audio::vad::VoiceActivityDetector::new(0.5, vad_model_path.to_str().unwrap_or("")) {
            Ok(mut vad_instance) => {
                let speech = audio::vad::filter_speech(&mut vad_instance, &samples, 3)?;
                if speech.is_empty() {
                    tracing::info!("no speech detected in recording");
                    return Ok(());
                }
                speech
            }
            Err(e) => {
                tracing::debug!("VAD unavailable ({}), using raw audio", e);
                samples
            }
        };

    // Transcribe
    let whisper = stt::whisper::WhisperStt::new(&config.stt.model_path, &config.stt.language)?;
    let raw_text = whisper.transcribe(&filtered)?;

    if raw_text.trim().is_empty() {
        tracing::info!("transcription produced empty text");
        return Ok(());
    }
    tracing::debug!("raw transcription: {:?}", raw_text);

    // LLM processing
    let final_text = if is_command {
        let messages = llm::prompts::command_messages(&raw_text);
        ollama.chat(&config.llm.command_model, messages).await?
    } else {
        let custom_prompt = config
            .llm
            .cleanup_prompt
            .as_ref()
            .map(|p| p.system.as_str());
        let messages = llm::prompts::cleanup_messages(&raw_text, custom_prompt);
        ollama.chat(&config.llm.cleanup_model, messages).await?
    };

    tracing::debug!("final text: {:?}", final_text);

    // Paste at cursor
    input::paste::paste_text(&final_text, paste_method)?;
    tracing::info!("text pasted successfully");

    Ok(())
}

/// Check system readiness: Ollama, whisper model, paste tools.
async fn check_system(config: &config::Config) -> Result<()> {
    println!("murmer system check");
    println!("==================");

    // Check Ollama
    let endpoint = if config.llm.protocol.as_deref() == Some("bedrock") {
        format!(
            "bedrock:{}",
            config.llm.region.as_deref().unwrap_or("us-east-1")
        )
    } else {
        config.llm.endpoint.clone()
    };
    let ollama = llm::client::LlmClient::new(
        &endpoint,
        config.llm.api_key.as_deref(),
        config.llm.protocol.as_deref(),
    );
    match ollama.health_check().await {
        Ok(true) => println!("[OK] Ollama reachable at {}", config.llm.endpoint),
        Ok(false) => println!(
            "[WARN] Ollama returned error status at {}",
            config.llm.endpoint
        ),
        Err(e) => println!(
            "[FAIL] Ollama not reachable at {}: {}",
            config.llm.endpoint, e
        ),
    }

    // Check whisper model
    let model_path = &config.stt.model_path;
    if std::path::Path::new(model_path).exists() {
        println!("[OK] Whisper model found at {}", model_path);
    } else {
        println!(
            "[FAIL] Whisper model not found at {}. Run: murmer --download-model base.en",
            model_path
        );
    }

    // Check paste tools
    let paste_method = input::paste::PasteMethod::from_str(&config.paste.method);
    match &paste_method {
        input::paste::PasteMethod::Auto => match input::paste::PasteMethod::detect() {
            Ok(input::paste::PasteMethod::Wtype) => {
                check_command("wl-copy");
                check_command("wtype");
            }
            Ok(input::paste::PasteMethod::Xdotool) => {
                check_command("xclip");
                check_command("xdotool");
            }
            Ok(_) => {}
            Err(e) => println!("[FAIL] {}", e),
        },
        input::paste::PasteMethod::Wtype => {
            check_command("wl-copy");
            check_command("wtype");
        }
        input::paste::PasteMethod::Xdotool => {
            check_command("xclip");
            check_command("xdotool");
        }
    }

    // Check VAD model
    let vad_path = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("murmer/models/silero_vad.onnx");
    if vad_path.exists() {
        println!("[OK] Silero VAD model found at {}", vad_path.display());
    } else {
        println!(
            "[INFO] Silero VAD model not at {} (will use energy-based fallback)",
            vad_path.display()
        );
    }

    println!("\nConfiguration:");
    println!("  Cleanup model: {}", config.llm.cleanup_model);
    println!("  Command model: {}", config.llm.command_model);
    println!("  Dictate hotkey: {}", config.hotkeys.dictate);
    println!("  Command hotkey: {}", config.hotkeys.command);
    println!("  Paste method: {:?}", paste_method);

    Ok(())
}

/// Check if a command is available on PATH.
fn check_command(name: &str) {
    match std::process::Command::new("which").arg(name).output() {
        Ok(output) if output.status.success() => println!("[OK] {} found", name),
        _ => println!("[FAIL] {} not found (install it)", name),
    }
}

/// Download a whisper model from Hugging Face.
async fn download_model(model_name: &str) -> Result<()> {
    let filename = match model_name {
        "tiny" => "ggml-tiny.bin",
        "tiny.en" => "ggml-tiny.en.bin",
        "base" => "ggml-base.bin",
        "base.en" => "ggml-base.en.bin",
        "small" => "ggml-small.bin",
        "small.en" => "ggml-small.en.bin",
        "medium" => "ggml-medium.bin",
        "medium.en" => "ggml-medium.en.bin",
        "large-v3" => "ggml-large-v3.bin",
        other => anyhow::bail!(
            "unknown model: '{}'. Available: tiny, tiny.en, base, base.en, small, small.en, medium, medium.en, large-v3",
            other
        ),
    };

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        filename
    );

    let dest_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("murmer/models");

    std::fs::create_dir_all(&dest_dir)?;
    let dest_path = dest_dir.join(filename);

    if dest_path.exists() {
        println!("Model already exists at {}", dest_path.display());
        return Ok(());
    }

    println!("Downloading {} to {}", filename, dest_path.display());
    println!("URL: {}", url);

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("download failed: {}", e))?;

    if !resp.status().is_success() {
        anyhow::bail!("download failed: HTTP {}", resp.status());
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| anyhow::anyhow!("failed to read response: {}", e))?;

    std::fs::write(&dest_path, &bytes)?;
    println!(
        "Downloaded {} ({:.1} MB)",
        filename,
        bytes.len() as f64 / 1_048_576.0
    );

    Ok(())
}
