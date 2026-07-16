use anyhow::{Context, Result};
use std::path::Path;

pub struct WhisperStt {
    language: String,
}

impl WhisperStt {
    pub fn new(model_path: &str, language: &str) -> Result<Self> {
        let path = Path::new(model_path);
        if !path.exists() {
            anyhow::bail!(
                "whisper model not found at {}. Run: murmer --download-model",
                model_path
            );
        }

        // TODO: Initialize whisper-rs context from model file
        // let ctx = whisper_rs::WhisperContext::new_with_params(model_path, params)?;

        Ok(Self {
            language: language.to_string(),
        })
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    /// Transcribe audio samples (mono 16kHz f32) to text.
    pub fn transcribe(&self, _samples: &[f32]) -> Result<String> {
        // TODO: Run whisper inference
        // 1. Create WhisperState from context
        // 2. Set params (language, no_timestamps, single_segment)
        // 3. Run full transcription
        // 4. Collect segments into single string
        todo!("implement whisper transcription")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_model_errors() {
        let result = WhisperStt::new("/nonexistent/model.bin", "en");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
