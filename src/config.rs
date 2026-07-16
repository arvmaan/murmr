use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub hotkeys: HotkeyConfig,
    #[serde(default)]
    pub stt: SttConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub paste: PasteConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HotkeyConfig {
    #[serde(default = "default_dictate_hotkey")]
    pub dictate: String,
    #[serde(default = "default_command_hotkey")]
    pub command: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SttConfig {
    #[serde(default = "default_model_path")]
    pub model_path: String,
    #[serde(default = "default_language")]
    pub language: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_cleanup_model")]
    pub cleanup_model: String,
    #[serde(default = "default_command_model")]
    pub command_model: String,
    #[serde(default)]
    pub cleanup_prompt: Option<CleanupPrompt>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CleanupPrompt {
    pub system: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PasteConfig {
    #[serde(default = "default_paste_method")]
    pub method: String,
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
        }
    }
}

impl Default for PasteConfig {
    fn default() -> Self {
        Self {
            method: default_paste_method(),
        }
    }
}

fn default_dictate_hotkey() -> String {
    "Super+Shift+D".to_string()
}

fn default_command_hotkey() -> String {
    "Super+Shift+C".to_string()
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

pub fn default_cleanup_system_prompt() -> &'static str {
    "Clean up this dictated text. Remove filler words (um, uh, like, you know), \
     fix punctuation and capitalization, normalize numbers and dates, \
     honor self-corrections (e.g. 'no wait, Friday' means use Friday). \
     Do NOT change meaning, add content, or explain. Output ONLY the cleaned text."
}

pub fn config_path() -> PathBuf {
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
        };
        assert_eq!(config.llm.endpoint, "http://localhost:11434");
        assert_eq!(config.llm.cleanup_model, "qwen3:1.7b");
        assert_eq!(config.paste.method, "auto");
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
}
