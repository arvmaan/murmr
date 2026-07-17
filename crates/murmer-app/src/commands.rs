use crate::state::{AppState, TranscriptEntry};
use murmer_core::config::{Config, ModeConfig};
use murmer_core::llm::client::LlmClient;
use murmer_core::modes::registry::ModeRegistry;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_config(state: State<'_, Arc<AppState>>) -> Result<Config, String> {
    let config = state.config.lock().await;
    Ok(config.clone())
}

#[tauri::command]
pub async fn save_config(state: State<'_, Arc<AppState>>, config: Config) -> Result<(), String> {
    // Save to disk
    let config_path = murmer_core::config::config_path();
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let toml_str = toml::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, toml_str).map_err(|e| e.to_string())?;

    // Reinitialize client
    let endpoint = if config.llm.protocol.as_deref() == Some("bedrock") {
        format!(
            "bedrock:{}",
            config.llm.region.as_deref().unwrap_or("us-east-1")
        )
    } else {
        config.llm.endpoint.clone()
    };
    let new_client = LlmClient::new(
        &endpoint,
        config.llm.api_key.as_deref(),
        config.llm.protocol.as_deref(),
    );

    *state.client.lock().await = new_client;
    *state.config.lock().await = config;

    Ok(())
}

#[tauri::command]
pub async fn get_transcripts(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<TranscriptEntry>, String> {
    let transcripts = state.transcripts.lock().await;
    Ok(transcripts.clone())
}

#[tauri::command]
pub async fn clear_transcripts(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.transcripts.lock().await.clear();
    Ok(())
}

#[tauri::command]
pub async fn get_dictionary(
    state: State<'_, Arc<AppState>>,
) -> Result<HashMap<String, String>, String> {
    let dict = state.dictionary.lock().await;
    let entries: HashMap<String, String> = dict
        .all_entries()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Ok(entries)
}

#[tauri::command]
pub async fn add_dictionary_entry(
    state: State<'_, Arc<AppState>>,
    term: String,
    expansion: String,
) -> Result<(), String> {
    let mut dict = state.dictionary.lock().await;
    dict.learn(term, expansion, "manual".to_string());
    dict.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn remove_dictionary_entry(
    state: State<'_, Arc<AppState>>,
    term: String,
) -> Result<(), String> {
    let mut dict = state.dictionary.lock().await;
    dict.learned_entries.remove(&term);
    dict.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_modes(state: State<'_, Arc<AppState>>) -> Result<Vec<ModeConfig>, String> {
    let config = state.config.lock().await;
    let registry = ModeRegistry::new(&config.modes);
    Ok(registry.all_modes().to_vec())
}

#[tauri::command]
pub async fn check_system(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    let config = state.config.lock().await;
    let client = state.client.lock().await;
    let mut results = Vec::new();

    match client.health_check().await {
        Ok(true) => results.push(format!("[OK] LLM reachable at {}", config.llm.endpoint)),
        Ok(false) => results.push(format!("[WARN] LLM returned error at {}", config.llm.endpoint)),
        Err(e) => results.push(format!("[FAIL] LLM not reachable: {}", e)),
    }

    let model_path = &config.stt.model_path;
    if std::path::Path::new(model_path).exists() {
        results.push(format!("[OK] Whisper model at {}", model_path));
    } else {
        results.push(format!("[FAIL] Whisper model not found at {}", model_path));
    }

    results.push(format!("Cleanup model: {}", config.llm.cleanup_model));
    results.push(format!("Command model: {}", config.llm.command_model));

    Ok(results)
}
