use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
    #[serde(default)]
    pub stt: SttConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub paste: PasteConfig,
    #[serde(default)]
    pub modes: Vec<ModeConfig>,
    #[serde(default)]
    pub dictionary: DictionaryConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HotkeyConfig {
    #[serde(default = "default_dictate_hotkey")]
    pub dictate: String,
    #[serde(default = "default_command_hotkey")]
    pub command: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SttConfig {
    #[serde(default = "default_model_path")]
    pub model_path: String,
    #[serde(default = "default_language")]
    pub language: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LlmConfig {
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_cleanup_model")]
    pub cleanup_model: String,
    #[serde(default = "default_command_model")]
    pub command_model: String,
    #[serde(default)]
    pub cleanup_prompt: Option<CleanupPrompt>,
    /// API key for OpenAI-compatible services (env: MURMER_API_KEY)
    #[serde(default)]
    pub api_key: Option<String>,
    /// Protocol: "ollama", "openai", "anthropic", or "bedrock". Auto-detected if not set.
    #[serde(default)]
    pub protocol: Option<String>,
    /// AWS region for Bedrock (default: us-east-1)
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CleanupPrompt {
    pub system: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PasteConfig {
    #[serde(default = "default_paste_method")]
    pub method: String,
    /// When true, show the transcribed text in the pill and wait for the user
    /// to confirm (press the hotkey again / Enter) before pasting. Default off.
    #[serde(default)]
    pub preview_before_paste: bool,
}

/// A user-defined or built-in prompt template mode.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ModeConfig {
    pub name: String,
    pub triggers: Vec<String>,
    #[serde(default)]
    pub description: String,
    pub template: String,
    #[serde(default)]
    pub output: Option<String>,
}

/// Configuration for the adaptive dictionary.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct DictionaryConfig {
    #[serde(default)]
    pub entries: HashMap<String, String>,
    #[serde(default)]
    pub learning: DictionaryLearningConfig,
}

/// Settings for the dictionary learning feature.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DictionaryLearningConfig {
    #[serde(default = "default_learning_enabled")]
    pub enabled: bool,
    #[serde(default = "default_suggestion_threshold")]
    pub suggestion_threshold: u32,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            dictate: default_dictate_hotkey(),
            command: default_command_hotkey(),
        }
    }
}

impl Default for SttConfig {
    fn default() -> Self {
        Self {
            model_path: default_model_path(),
            language: default_language(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            endpoint: default_endpoint(),
            cleanup_model: default_cleanup_model(),
            command_model: default_command_model(),
            cleanup_prompt: None,
            api_key: None,
            protocol: None,
            region: None,
        }
    }
}

impl Default for PasteConfig {
    fn default() -> Self {
        Self {
            method: default_paste_method(),
            preview_before_paste: false,
        }
    }
}

impl Default for DictionaryLearningConfig {
    fn default() -> Self {
        Self {
            enabled: default_learning_enabled(),
            suggestion_threshold: default_suggestion_threshold(),
        }
    }
}

fn default_dictate_hotkey() -> String {
    "Super+Shift+K".to_string()
}

fn default_command_hotkey() -> String {
    "Super+Shift+L".to_string()
}

fn default_model_path() -> String {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("murmer/models/ggml-base.en.bin");
    data_dir.to_string_lossy().to_string()
}

fn default_language() -> String {
    "en".to_string()
}

fn default_endpoint() -> String {
    "http://localhost:11434".to_string()
}

fn default_cleanup_model() -> String {
    "qwen3:1.7b".to_string()
}

fn default_command_model() -> String {
    "phi4-mini".to_string()
}

fn default_paste_method() -> String {
    "auto".to_string()
}

fn default_learning_enabled() -> bool {
    true
}

fn default_suggestion_threshold() -> u32 {
    3
}

/// The default dictation cleanup system prompt. Delegates to the canonical
/// definition in `llm::prompts` so there is a single source of truth.
pub fn default_cleanup_system_prompt() -> &'static str {
    crate::llm::prompts::DEFAULT_CLEANUP_PROMPT
}

pub fn config_path() -> PathBuf {
    // Prefer the XDG-style ~/.config/murmer/config.toml. On macOS
    // `dirs::config_dir()` resolves to ~/Library/Application Support, but murmer
    // has always documented and used ~/.config. If a config already exists there,
    // that is the canonical file for both reading and writing so the app and the
    // CLI (`-c ~/.config/murmer/config.toml`) never diverge.
    if let Some(home) = dirs::home_dir() {
        let xdg = home.join(".config/murmer/config.toml");
        if xdg.exists() {
            return xdg;
        }
    }

    // Fall back to the platform config dir. When neither exists yet, prefer
    // ~/.config so a freshly-created config lands in the documented location.
    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        return home.join(".config/murmer/config.toml");
    }

    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("murmer/config.toml")
}

pub fn load(path: Option<&str>) -> Result<Config> {
    let config_file = path.map(PathBuf::from).unwrap_or_else(config_path);

    if config_file.exists() {
        let content = std::fs::read_to_string(&config_file)
            .with_context(|| format!("failed to read config: {}", config_file.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", config_file.display()))?;
        Ok(config)
    } else {
        tracing::info!("no config file found, using defaults");
        Ok(Config {
            hotkeys: HotkeyConfig::default(),
            stt: SttConfig::default(),
            llm: LlmConfig::default(),
            paste: PasteConfig::default(),
            modes: Vec::new(),
            dictionary: DictionaryConfig::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config {
            hotkeys: HotkeyConfig::default(),
            stt: SttConfig::default(),
            llm: LlmConfig::default(),
            paste: PasteConfig::default(),
            modes: Vec::new(),
            dictionary: DictionaryConfig::default(),
        };
        assert_eq!(config.llm.endpoint, "http://localhost:11434");
        assert_eq!(config.llm.cleanup_model, "qwen3:1.7b");
        assert_eq!(config.paste.method, "auto");
        assert!(config.dictionary.learning.enabled);
        assert_eq!(config.dictionary.learning.suggestion_threshold, 3);
    }

    #[test]
    fn test_parse_config_toml() {
        let toml_str = r#"
[hotkeys]
dictate = "Ctrl+Alt+D"
command = "Ctrl+Alt+C"

[stt]
language = "en"

[llm]
endpoint = "http://localhost:11434"
cleanup_model = "qwen3:0.6b"
command_model = "qwen3:4b"

[paste]
method = "wtype"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.hotkeys.dictate, "Ctrl+Alt+D");
        assert_eq!(config.llm.cleanup_model, "qwen3:0.6b");
        assert_eq!(config.paste.method, "wtype");
    }

    #[test]
    fn test_parse_modes_config() {
        let toml_str = r#"
[[modes]]
name = "test-mode"
triggers = ["test this", "try this"]
description = "A test mode"
template = "TASK: {{objective}}\nDONE"
output = "clipboard"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.modes.len(), 1);
        assert_eq!(config.modes[0].name, "test-mode");
        assert_eq!(config.modes[0].triggers, vec!["test this", "try this"]);
        assert_eq!(config.modes[0].output, Some("clipboard".to_string()));
    }

    #[test]
    fn test_parse_dictionary_config() {
        let toml_str = r#"
[dictionary]
entries = { "MP" = "MetricsProcessor", "LPCP" = "LogProcessingControlPlane" }

[dictionary.learning]
enabled = true
suggestion_threshold = 5
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.dictionary.entries.get("MP").unwrap(),
            "MetricsProcessor"
        );
        assert_eq!(
            config.dictionary.entries.get("LPCP").unwrap(),
            "LogProcessingControlPlane"
        );
        assert!(config.dictionary.learning.enabled);
        assert_eq!(config.dictionary.learning.suggestion_threshold, 5);
    }
}
