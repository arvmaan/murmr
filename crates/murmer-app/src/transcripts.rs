//! Transcript history persistence.
//!
//! History lives in memory for the UI, but is mirrored to a JSON file so it
//! survives restarts. Capped at `MAX_ENTRIES` newest-first.

use crate::state::TranscriptEntry;
use std::path::PathBuf;

/// Keep the most recent N transcripts; older ones are dropped on save.
pub const MAX_ENTRIES: usize = 200;

/// `~/.config/murmer/transcripts.json` (alongside config.toml).
pub fn transcripts_path() -> PathBuf {
    murmer_core::config::config_path()
        .parent()
        .map(|p| p.join("transcripts.json"))
        .unwrap_or_else(|| PathBuf::from("transcripts.json"))
}

/// Load persisted transcripts (newest-first). Returns empty on any error so a
/// corrupt or missing file never blocks startup.
pub fn load() -> Vec<TranscriptEntry> {
    let path = transcripts_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            tracing::warn!("could not parse {}: {}", path.display(), e);
            Vec::new()
        }),
        Err(_) => Vec::new(),
    }
}

/// Persist the transcript list (already newest-first), capped to MAX_ENTRIES.
pub fn save(entries: &[TranscriptEntry]) {
    let path = transcripts_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let capped = &entries[..entries.len().min(MAX_ENTRIES)];
    match serde_json::to_string_pretty(capped) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                tracing::warn!("failed to write {}: {}", path.display(), e);
            }
        }
        Err(e) => tracing::warn!("failed to serialize transcripts: {}", e),
    }
}
