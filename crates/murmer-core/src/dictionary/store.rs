use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Persistent dictionary store with static and learned entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryStore {
    /// Static entries from config (term → expansion).
    pub static_entries: HashMap<String, String>,
    /// Learned entries with metadata.
    pub learned_entries: HashMap<String, LearnedEntry>,
    /// Path to the persistent JSON file.
    #[serde(skip)]
    file_path: PathBuf,
}

/// A learned dictionary entry with frequency and confidence tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedEntry {
    pub expansion: String,
    pub frequency: u32,
    pub confidence: f32,
    pub source: String,
}

impl DictionaryStore {
    /// Load or create a dictionary store.
    pub fn load(static_entries: &HashMap<String, String>) -> Result<Self> {
        let file_path = data_path();
        let mut store = if file_path.exists() {
            let content = std::fs::read_to_string(&file_path)
                .with_context(|| format!("failed to read dictionary: {}", file_path.display()))?;
            serde_json::from_str(&content).unwrap_or_else(|_| DictionaryStore {
                static_entries: HashMap::new(),
                learned_entries: HashMap::new(),
                file_path: file_path.clone(),
            })
        } else {
            DictionaryStore {
                static_entries: HashMap::new(),
                learned_entries: HashMap::new(),
                file_path: file_path.clone(),
            }
        };

        store.file_path = file_path;
        // Merge static entries from config (config always wins)
        store.static_entries = static_entries.clone();

        Ok(store)
    }

    /// Save the store to disk.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&self.file_path, content)?;
        Ok(())
    }

    /// Look up a term in both static and learned entries.
    pub fn lookup(&self, term: &str) -> Option<&str> {
        self.static_entries
            .get(term)
            .map(|s| s.as_str())
            .or_else(|| self.learned_entries.get(term).map(|e| e.expansion.as_str()))
    }

    /// Add or update a learned entry.
    pub fn learn(&mut self, term: String, expansion: String, source: String) {
        let entry = self
            .learned_entries
            .entry(term)
            .or_insert_with(|| LearnedEntry {
                expansion: expansion.clone(),
                frequency: 0,
                confidence: 0.0,
                source: source.clone(),
            });
        entry.frequency += 1;
        entry.confidence = (entry.frequency as f32 / 5.0).min(1.0);
        entry.expansion = expansion;
        entry.source = source;
    }

    /// Increment frequency for a known term.
    pub fn record_usage(&mut self, term: &str) {
        if let Some(entry) = self.learned_entries.get_mut(term) {
            entry.frequency += 1;
            entry.confidence = (entry.frequency as f32 / 5.0).min(1.0);
        }
    }

    /// Get all entries (static + learned) as a flat map.
    pub fn all_entries(&self) -> HashMap<&str, &str> {
        let mut map: HashMap<&str, &str> = HashMap::new();
        for (k, v) in &self.learned_entries {
            map.insert(k.as_str(), v.expansion.as_str());
        }
        // Static entries override learned ones
        for (k, v) in &self.static_entries {
            map.insert(k.as_str(), v.as_str());
        }
        map
    }

    /// Generate the dictionary injection for the cleanup prompt.
    pub fn prompt_injection(&self) -> Option<String> {
        let entries = self.all_entries();
        if entries.is_empty() {
            return None;
        }

        let mut lines: Vec<String> = entries
            .iter()
            .map(|(term, expansion)| format!("  {} = {}", term, expansion))
            .collect();
        lines.sort();

        Some(format!(
            "The speaker uses these abbreviations and terms. Always expand them:\n{}",
            lines.join("\n")
        ))
    }
}

/// Path to the dictionary data file.
fn data_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("murmer/dictionary.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_store() {
        let store = DictionaryStore::load(&HashMap::new()).unwrap();
        assert!(store.lookup("MP").is_none());
        assert!(store.all_entries().is_empty());
        assert!(store.prompt_injection().is_none());
    }

    #[test]
    fn test_static_entries() {
        let mut entries = HashMap::new();
        entries.insert("MP".to_string(), "MetricsProcessor".to_string());
        entries.insert("CW".to_string(), "CloudWatch".to_string());

        let store = DictionaryStore::load(&entries).unwrap();
        assert_eq!(store.lookup("MP"), Some("MetricsProcessor"));
        assert_eq!(store.lookup("CW"), Some("CloudWatch"));
        assert_eq!(store.lookup("UNKNOWN"), None);
    }

    #[test]
    fn test_learn_entry() {
        let mut store = DictionaryStore::load(&HashMap::new()).unwrap();
        store.learn(
            "LPCP".to_string(),
            "LogProcessingControlPlane".to_string(),
            "git_log".to_string(),
        );

        assert_eq!(store.lookup("LPCP"), Some("LogProcessingControlPlane"));
        let entry = store.learned_entries.get("LPCP").unwrap();
        assert_eq!(entry.frequency, 1);
        assert_eq!(entry.source, "git_log");
    }

    #[test]
    fn test_learn_increments_frequency() {
        let mut store = DictionaryStore::load(&HashMap::new()).unwrap();
        store.learn(
            "MP".to_string(),
            "MetricsProcessor".to_string(),
            "git".to_string(),
        );
        store.learn(
            "MP".to_string(),
            "MetricsProcessor".to_string(),
            "git".to_string(),
        );
        store.learn(
            "MP".to_string(),
            "MetricsProcessor".to_string(),
            "git".to_string(),
        );

        let entry = store.learned_entries.get("MP").unwrap();
        assert_eq!(entry.frequency, 3);
        assert!((entry.confidence - 0.6).abs() < 0.01);
    }

    #[test]
    fn test_record_usage() {
        let mut store = DictionaryStore::load(&HashMap::new()).unwrap();
        store.learn(
            "MP".to_string(),
            "MetricsProcessor".to_string(),
            "git".to_string(),
        );
        store.record_usage("MP");
        assert_eq!(store.learned_entries.get("MP").unwrap().frequency, 2);
    }

    #[test]
    fn test_static_overrides_learned() {
        let mut static_entries = HashMap::new();
        static_entries.insert("MP".to_string(), "StaticValue".to_string());

        let mut store = DictionaryStore::load(&static_entries).unwrap();
        store.learn(
            "MP".to_string(),
            "LearnedValue".to_string(),
            "test".to_string(),
        );

        // Static always wins
        assert_eq!(store.lookup("MP"), Some("StaticValue"));
    }

    #[test]
    fn test_prompt_injection() {
        let mut entries = HashMap::new();
        entries.insert("MP".to_string(), "MetricsProcessor".to_string());
        let store = DictionaryStore::load(&entries).unwrap();

        let injection = store.prompt_injection().unwrap();
        assert!(injection.contains("MP = MetricsProcessor"));
        assert!(injection.contains("abbreviations"));
    }

    #[test]
    fn test_save_and_reload() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let file_path = tmp_dir.path().join("dict.json");

        let mut store = DictionaryStore {
            static_entries: HashMap::new(),
            learned_entries: HashMap::new(),
            file_path: file_path.clone(),
        };
        store.learn("X".to_string(), "Expansion".to_string(), "test".to_string());
        store.save().unwrap();

        let content = std::fs::read_to_string(&file_path).unwrap();
        let reloaded: DictionaryStore = serde_json::from_str(&content).unwrap();
        assert_eq!(
            reloaded.learned_entries.get("X").unwrap().expansion,
            "Expansion"
        );
    }
}
