use murmer_core::config::Config;
use murmer_core::dictionary::store::DictionaryStore;
use murmer_core::input::hotkey::HotkeyEvent;
use murmer_core::llm::client::LlmClient;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::{mpsc, Mutex};

/// A single transcript entry shown in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub timestamp: String,
    pub raw_text: String,
    pub cleaned_text: String,
    pub mode_used: Option<String>,
}

/// Shared application state managed by Tauri.
pub struct AppState {
    pub config: Mutex<Config>,
    pub client: Mutex<LlmClient>,
    pub dictionary: Mutex<DictionaryStore>,
    pub transcripts: Mutex<Vec<TranscriptEntry>>,
    pub recording: Arc<AtomicBool>,
    /// Stop-flag handed to the active capture thread, so the release event can
    /// signal it to finish. `None` when not recording.
    pub record_stop: Mutex<Option<Arc<AtomicBool>>>,
    /// Sender for hotkey events, set once when the recording loop starts. Lets
    /// re-registered shortcuts (after a settings save) feed the same loop.
    pub hotkey_tx: OnceLock<mpsc::UnboundedSender<HotkeyEvent>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let endpoint = if config.llm.protocol.as_deref() == Some("bedrock") {
            format!(
                "bedrock:{}",
                config.llm.region.as_deref().unwrap_or("us-east-1")
            )
        } else {
            config.llm.endpoint.clone()
        };

        let client = LlmClient::new(
            &endpoint,
            config.llm.api_key.as_deref(),
            config.llm.protocol.as_deref(),
        );

        let dictionary = DictionaryStore::load(&config.dictionary.entries)
            .unwrap_or_else(|_| DictionaryStore::load(&std::collections::HashMap::new()).unwrap());

        Self {
            config: Mutex::new(config),
            client: Mutex::new(client),
            dictionary: Mutex::new(dictionary),
            transcripts: Mutex::new(crate::transcripts::load()),
            recording: Arc::new(AtomicBool::new(false)),
            record_stop: Mutex::new(None),
            hotkey_tx: OnceLock::new(),
        }
    }
}
