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
pub async fn save_config(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    config: Config,
) -> Result<(), String> {
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

    // Re-bind global hotkeys so a changed shortcut takes effect immediately.
    crate::recording::reregister(&app, &config.hotkeys.dictate, &config.hotkeys.command);

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
    let mut list = state.transcripts.lock().await;
    list.clear();
    crate::transcripts::save(&list);
    Ok(())
}

#[tauri::command]
pub async fn delete_transcript(
    state: State<'_, Arc<AppState>>,
    index: usize,
) -> Result<(), String> {
    let mut list = state.transcripts.lock().await;
    if index < list.len() {
        list.remove(index);
        crate::transcripts::save(&list);
    }
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

    // Describe the LLM target the way the active protocol actually addresses it,
    // so a Bedrock setup shows its region rather than an unused endpoint.
    let protocol = config.llm.protocol.as_deref().unwrap_or("ollama");
    let target = match protocol {
        "bedrock" => format!(
            "Bedrock ({})",
            config.llm.region.as_deref().unwrap_or("us-east-1")
        ),
        "anthropic" => "Anthropic API".to_string(),
        _ => config.llm.endpoint.clone(),
    };

    match client.health_check().await {
        Ok(true) => results.push(format!("[OK] LLM reachable — {}", target)),
        Ok(false) => results.push(format!("[WARN] LLM returned an error — {}", target)),
        Err(e) => results.push(format!("[FAIL] LLM not reachable ({}): {}", target, e)),
    }

    let model_path = &config.stt.model_path;
    if model_path.is_empty() {
        results.push("[WARN] Whisper model path not set".to_string());
    } else if std::path::Path::new(model_path).exists() {
        results.push(format!("[OK] Whisper model at {}", model_path));
    } else {
        results.push(format!("[FAIL] Whisper model not found at {}", model_path));
    }

    results.push(format!("Protocol: {}", protocol));
    results.push(format!("Cleanup model: {}", config.llm.cleanup_model));
    results.push(format!("Command model: {}", config.llm.command_model));

    Ok(results)
}
