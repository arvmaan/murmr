use murmer_core::config::Config;
use murmer_core::dictionary::store::DictionaryStore;
use murmer_core::llm::client::LlmClient;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::Mutex;

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

        let dictionary =
            DictionaryStore::load(&config.dictionary.entries).unwrap_or_else(|_| {
                DictionaryStore::load(&std::collections::HashMap::new()).unwrap()
            });

        Self {
            config: Mutex::new(config),
            client: Mutex::new(client),
            dictionary: Mutex::new(dictionary),
            transcripts: Mutex::new(Vec::new()),
            recording: Arc::new(AtomicBool::new(false)),
        }
    }
}
