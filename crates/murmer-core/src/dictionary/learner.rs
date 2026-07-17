use anyhow::Result;
use std::collections::HashMap;

use super::store::DictionaryStore;

/// Learns new dictionary entries by detecting repeated terms and
/// cross-referencing against context sources.
pub struct DictionaryLearner {
    /// Frequency counts for potential terms seen in transcriptions.
    term_counts: HashMap<String, u32>,
    /// Threshold for auto-learning (number of occurrences needed).
    suggestion_threshold: u32,
}

impl DictionaryLearner {
    /// Create a new learner with the given threshold.
    pub fn new(suggestion_threshold: u32) -> Self {
        Self {
            term_counts: HashMap::new(),
            suggestion_threshold,
        }
    }

    /// Process a transcription and potentially learn new terms.
    /// Returns a list of newly learned terms (if any).
    pub fn process_transcription(
        &mut self,
        text: &str,
        store: &mut DictionaryStore,
    ) -> Result<Vec<String>> {
        let candidates = find_candidates(text);
        let mut newly_learned = Vec::new();

        for term in candidates {
            // Skip if already in dictionary
            if store.lookup(&term).is_some() {
                store.record_usage(&term);
                continue;
            }

            // Increment frequency
            let count = self.term_counts.entry(term.clone()).or_insert(0);
            *count += 1;

            // Check if we've hit the threshold
            if *count >= self.suggestion_threshold {
                // Try to find an expansion from context
                if let Some(expansion) = find_expansion_from_context(&term) {
                    tracing::info!(
                        "dictionary learned: {} = {} (seen {} times)",
                        term,
                        expansion,
                        count
                    );
                    store.learn(term.clone(), expansion, "auto-context".to_string());
                    newly_learned.push(term);
                } else {
                    tracing::debug!(
                        "dictionary candidate '{}' seen {} times but no expansion found",
                        term,
                        count
                    );
                }
            }
        }

        if !newly_learned.is_empty() {
            store.save().ok();
        }

        Ok(newly_learned)
    }

    /// Get the current frequency counts for potential terms.
    pub fn term_counts(&self) -> &HashMap<String, u32> {
        &self.term_counts
    }
}

/// Find candidate terms in transcription text.
/// Candidates are: all-uppercase tokens of 2-6 chars (acronyms),
/// or CamelCase tokens that look like identifiers.
fn find_candidates(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
        if clean.is_empty() {
            continue;
        }

        // All-uppercase acronyms (2-6 chars)
        if clean.len() >= 2
            && clean.len() <= 6
            && clean
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && clean.chars().any(|c| c.is_ascii_uppercase())
        {
            candidates.push(clean.to_string());
        }
    }

    candidates
}

/// Try to find an expansion for a term by searching context sources.
fn find_expansion_from_context(term: &str) -> Option<String> {
    // Try git log for the term
    if let Some(expansion) = search_git_log(term) {
        return Some(expansion);
    }

    // Try file/directory names in cwd
    if let Some(expansion) = search_filenames(term) {
        return Some(expansion);
    }

    None
}

/// Search recent git log for a term's expansion.
fn search_git_log(term: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["log", "--oneline", "-50", "--all"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let log_text = String::from_utf8_lossy(&output.stdout);
    find_expansion_in_text(term, &log_text)
}

/// Search file/directory names for a term's expansion.
fn search_filenames(term: &str) -> Option<String> {
    let output = std::process::Command::new("find")
        .args([".", "-maxdepth", "3", "-name", &format!("*{}*", term)])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let filenames = String::from_utf8_lossy(&output.stdout);
    // Look for a filename or path segment that expands the acronym
    for line in filenames.lines() {
        let segments: Vec<&str> = line.split('/').collect();
        for segment in segments {
            if is_expansion_of(term, segment) {
                return Some(segment.to_string());
            }
        }
    }
    None
}

/// Search text for a word that could be the expansion of an acronym.
fn find_expansion_in_text(term: &str, text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
        if is_expansion_of(term, clean) {
            return Some(clean.to_string());
        }
    }
    None
}

/// Check if `candidate` is a plausible expansion of `acronym`.
/// E.g., "MP" expands to "MetricsProcessor" (first letters match).
fn is_expansion_of(acronym: &str, candidate: &str) -> bool {
    if candidate.len() < acronym.len() * 2 {
        return false;
    }

    // Split CamelCase or snake_case into parts
    let parts = split_identifier(candidate);
    if parts.len() < acronym.len() {
        return false;
    }

    // Check if first letters of parts match the acronym
    let first_letters: String = parts
        .iter()
        .filter_map(|p| p.chars().next())
        .map(|c| c.to_ascii_lowercase())
        .collect();

    first_letters.starts_with(&acronym.to_lowercase())
}

/// Split an identifier into parts (handles CamelCase and snake_case).
fn split_identifier(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();

    for c in s.chars() {
        if c == '_' || c == '-' {
            if !current.is_empty() {
                parts.push(current.clone());
                current.clear();
            }
        } else if c.is_ascii_uppercase() && !current.is_empty() {
            parts.push(current.clone());
            current.clear();
            current.push(c);
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_candidates_acronyms() {
        let text = "the MP service is running on LPCP and it connects to CW";
        let candidates = find_candidates(text);
        assert!(candidates.contains(&"MP".to_string()));
        assert!(candidates.contains(&"LPCP".to_string()));
        assert!(candidates.contains(&"CW".to_string()));
    }

    #[test]
    fn test_find_candidates_ignores_short() {
        let text = "I am here";
        let candidates = find_candidates(text);
        assert!(candidates.is_empty()); // "I" is too short
    }

    #[test]
    fn test_find_candidates_ignores_lowercase() {
        let text = "the service is good";
        let candidates = find_candidates(text);
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_find_candidates_with_punctuation() {
        let text = "check the MP, it's on LPCP.";
        let candidates = find_candidates(text);
        assert!(candidates.contains(&"MP".to_string()));
        assert!(candidates.contains(&"LPCP".to_string()));
    }

    #[test]
    fn test_is_expansion_of() {
        assert!(is_expansion_of("MP", "MetricsProcessor"));
        assert!(is_expansion_of("mp", "MetricsProcessor"));
        assert!(is_expansion_of("LPCP", "LogProcessingControlPlane"));
        assert!(is_expansion_of("CW", "CloudWatch"));
        assert!(!is_expansion_of("MP", "Logger"));
        assert!(!is_expansion_of("MP", "M")); // too short
    }

    #[test]
    fn test_is_expansion_of_snake_case() {
        assert!(is_expansion_of("MP", "metrics_processor"));
        assert!(is_expansion_of("LPCP", "log_processing_control_plane"));
    }

    #[test]
    fn test_split_identifier_camel() {
        let parts = split_identifier("MetricsProcessor");
        assert_eq!(parts, vec!["Metrics", "Processor"]);
    }

    #[test]
    fn test_split_identifier_snake() {
        let parts = split_identifier("metrics_processor");
        assert_eq!(parts, vec!["metrics", "processor"]);
    }

    #[test]
    fn test_split_identifier_mixed() {
        let parts = split_identifier("LogProcessing_ControlPlane");
        assert_eq!(parts, vec!["Log", "Processing", "Control", "Plane"]);
    }

    #[test]
    fn test_learner_basic_flow() {
        let mut learner = DictionaryLearner::new(3);
        let mut store = DictionaryStore::load(&HashMap::new()).unwrap();

        // First two occurrences — not enough
        learner
            .process_transcription("the MP is down", &mut store)
            .unwrap();
        learner
            .process_transcription("check MP status", &mut store)
            .unwrap();
        assert!(store.lookup("MP").is_none());
        assert_eq!(*learner.term_counts().get("MP").unwrap(), 2);
    }

    #[test]
    fn test_learner_skips_known() {
        let mut entries = HashMap::new();
        entries.insert("MP".to_string(), "MetricsProcessor".to_string());
        let mut store = DictionaryStore::load(&entries).unwrap();
        let mut learner = DictionaryLearner::new(3);

        // Known term should not be re-learned
        learner
            .process_transcription("check MP", &mut store)
            .unwrap();
        assert!(!learner.term_counts().contains_key("MP"));
    }

    #[test]
    fn test_find_expansion_in_text() {
        let text = "fixed MetricsProcessor timeout issue";
        let result = find_expansion_in_text("MP", text);
        assert_eq!(result, Some("MetricsProcessor".to_string()));
    }

    #[test]
    fn test_find_expansion_in_text_not_found() {
        let text = "fixed timeout issue in the service";
        let result = find_expansion_in_text("MP", text);
        assert_eq!(result, None);
    }
}
