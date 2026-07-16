mod audio;
mod config;
mod input;
mod llm;
mod stt;
mod tray;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "murmer", about = "Local voice-to-text with LLM cleanup")]
struct Cli {
    /// Path to config file (default: ~/.config/murmer/config.toml)
    #[arg(short, long)]
    config: Option<String>,

    /// Run in verbose mode
    #[arg(short, long)]
    verbose: bool,
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

    tracing::info!("murmer starting up");

    let config = config::load(cli.config.as_deref())?;
    tracing::debug!(?config, "loaded configuration");

    // TODO: Initialize components and run event loop
    // 1. Validate Ollama connectivity
    // 2. Load whisper model
    // 3. Register global hotkeys
    // 4. Start system tray
    // 5. Run event loop

    tracing::info!("murmer ready — press your hotkey to dictate");

    // Placeholder: wait for ctrl+c
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");

    Ok(())
}
