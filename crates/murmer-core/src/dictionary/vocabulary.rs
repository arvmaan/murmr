//! Codebase vocabulary index.
//!
//! Scans a repository for multi-part identifiers (CamelCase, snake_case,
//! kebab-case) and ranks them by frequency. The top terms are injected into the
//! cleanup prompt so speech-to-text output is corrected to the project's real
//! symbols — e.g. "Ingested Bytes" → "IngestedBytes".
//!
//! Design for latency: indexing is done once at rest (on demand / at startup),
//! never on the dictation hot path. Only a bounded slice of the vocabulary is
//! injected into the prompt so the LLM round-trip stays fast.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Max identifiers kept in the index on disk.
const MAX_INDEX: usize = 2000;
/// Max identifiers injected into the cleanup prompt (keeps the round-trip fast).
const MAX_INJECT: usize = 150;
/// Only index files smaller than this (skip generated blobs / minified bundles).
const MAX_FILE_BYTES: u64 = 512 * 1024;
/// Source extensions worth scanning for identifiers.
const CODE_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "kt", "swift", "rb", "c", "h", "cpp",
    "hpp", "cs", "php", "scala", "sql", "proto", "graphql",
];
/// Directories never worth scanning.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".git",
    "vendor",
    "venv",
    ".venv",
    "__pycache__",
    ".next",
    "out",
];

/// A ranked codebase vocabulary, persisted to disk.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Vocabulary {
    /// Identifier → occurrence count, highest-signal first when injected.
    pub terms: Vec<(String, u32)>,
    /// The repo path this was built from (for display).
    pub source: String,
}

impl Vocabulary {
    /// Path to the persisted vocabulary file (next to the dictionary).
    pub fn path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("murmer/vocabulary.json")
    }

    /// Load the persisted vocabulary, or an empty one if none/unreadable.
    pub fn load() -> Self {
        match std::fs::read_to_string(Self::path()) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let json = serde_json::to_string(self).context("serialize vocabulary")?;
        std::fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }

    /// Scan `root` for identifiers and build a frequency-ranked vocabulary.
    /// Only multi-part identifiers are kept — single plain words aren't useful
    /// (whisper already gets those right) and would bloat the index.
    pub fn build(root: &Path) -> Result<Self> {
        let mut counts: HashMap<String, u32> = HashMap::new();
        scan_dir(root, 0, &mut counts);

        let mut terms: Vec<(String, u32)> = counts.into_iter().collect();
        // Rank by frequency (desc), then alphabetically for stable ties.
        terms.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        terms.truncate(MAX_INDEX);

        Ok(Self {
            terms,
            source: root.display().to_string(),
        })
    }

    /// A bounded prompt injection listing the project's identifiers, so the
    /// cleanup model rewrites split/misheard tokens to the real symbols.
    /// Returns `None` when empty.
    pub fn prompt_injection(&self) -> Option<String> {
        if self.terms.is_empty() {
            return None;
        }
        let list: Vec<&str> = self
            .terms
            .iter()
            .take(MAX_INJECT)
            .map(|(t, _)| t.as_str())
            .collect();
        Some(format!(
            "This project uses these identifiers. If the transcript contains one \
             of them split into words or slightly misspelled, rewrite it to match \
             exactly (e.g. \"ingested bytes\" → \"IngestedBytes\"):\n{}",
            list.join(", ")
        ))
    }
}

/// Recursively scan a directory, counting identifiers in source files.
fn scan_dir(dir: &Path, depth: usize, counts: &mut HashMap<String, u32>) {
    if depth > 12 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if path.is_dir() {
            if !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_ref()) {
                scan_dir(&path, depth + 1, counts);
            }
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !CODE_EXTS.contains(&ext) {
            continue;
        }
        if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            count_identifiers(&text, counts);
        }
    }
}

/// Extract multi-part identifiers from source text and tally them.
fn count_identifiers(text: &str, counts: &mut HashMap<String, u32>) {
    let mut token = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            token.push(ch);
        } else {
            take_identifier(&token, counts);
            token.clear();
        }
    }
    take_identifier(&token, counts);
}

/// Keep `token` only if it's a genuine multi-part identifier worth indexing.
fn take_identifier(token: &str, counts: &mut HashMap<String, u32>) {
    if is_multipart_identifier(token) {
        *counts.entry(token.to_string()).or_insert(0) += 1;
    }
}

/// True for CamelCase, snake_case, or kebab-ish identifiers of reasonable
/// length — the tokens whisper tends to split or mangle. Plain lowercase words
/// and pure numbers are rejected.
fn is_multipart_identifier(t: &str) -> bool {
    let len = t.chars().count();
    if len < 4 || len > 40 {
        return false;
    }
    if !t.chars().next().is_some_and(|c| c.is_alphabetic()) {
        return false;
    }
    if t.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return false;
    }
    let has_underscore = t.contains('_');
    // CamelCase = an uppercase letter after a lowercase one.
    let mut has_case_boundary = false;
    let mut prev_lower = false;
    for c in t.chars() {
        if c.is_ascii_uppercase() && prev_lower {
            has_case_boundary = true;
        }
        prev_lower = c.is_ascii_lowercase();
    }
    has_underscore || has_case_boundary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_camelcase() {
        assert!(is_multipart_identifier("IngestedBytes"));
        assert!(is_multipart_identifier("parseConfig"));
    }

    #[test]
    fn detects_snake_case() {
        assert!(is_multipart_identifier("message_processor"));
    }

    #[test]
    fn rejects_plain_words() {
        assert!(!is_multipart_identifier("hello"));
        assert!(!is_multipart_identifier("the"));
        assert!(!is_multipart_identifier("config")); // single lowercase word
    }

    #[test]
    fn rejects_junk() {
        assert!(!is_multipart_identifier("a"));
        assert!(!is_multipart_identifier("123"));
        assert!(!is_multipart_identifier("_"));
    }

    #[test]
    fn counts_identifiers_in_text() {
        let mut counts = HashMap::new();
        count_identifiers(
            "let ingestedBytes = readBytes(); ingestedBytes += 1;",
            &mut counts,
        );
        assert_eq!(*counts.get("ingestedBytes").unwrap(), 2);
        assert_eq!(*counts.get("readBytes").unwrap(), 1);
        assert!(!counts.contains_key("let")); // plain word skipped
    }

    #[test]
    fn injection_is_bounded_and_none_when_empty() {
        assert!(Vocabulary::default().prompt_injection().is_none());
        let vocab = Vocabulary {
            terms: (0..500).map(|i| (format!("FooBar{i}"), 1)).collect(),
            source: "x".into(),
        };
        let inj = vocab.prompt_injection().unwrap();
        // Only MAX_INJECT terms are listed.
        assert_eq!(inj.matches("FooBar").count(), MAX_INJECT);
    }
}
