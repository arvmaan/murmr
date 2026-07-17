use anyhow::Result;
use std::path::Path;

/// Whisper speech-to-text engine using whisper.cpp via whisper-rs bindings.
pub struct WhisperStt {
    language: String,
    #[cfg(feature = "stt")]
    ctx: whisper_rs::WhisperContext,
}

impl std::fmt::Debug for WhisperStt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhisperStt")
            .field("language", &self.language)
            .finish()
    }
}

impl WhisperStt {
    /// Create a new WhisperStt instance, loading the model from disk.
    ///
    /// # Errors
    /// Returns an error if the model file doesn't exist or can't be loaded.
    #[cfg(feature = "stt")]
    pub fn new(model_path: &str, language: &str) -> Result<Self> {
        let path = Path::new(model_path);
        if !path.exists() {
            anyhow::bail!(
                "whisper model not found at {}. Run: murmer --download-model base.en",
                model_path
            );
        }

        let ctx = whisper_rs::WhisperContext::new_with_params(
            model_path,
            whisper_rs::WhisperContextParameters::default(),
        )
        .map_err(|e| anyhow::anyhow!("failed to load whisper model: {:?}", e))?;

        Ok(Self {
            language: language.to_string(),
            ctx,
        })
    }

    /// Placeholder when STT feature is disabled.
    #[cfg(not(feature = "stt"))]
    pub fn new(model_path: &str, language: &str) -> Result<Self> {
        let path = Path::new(model_path);
        if !path.exists() {
            anyhow::bail!(
                "whisper model not found at {}. Run: murmer --download-model base.en",
                model_path
            );
        }
        Ok(Self {
            language: language.to_string(),
        })
    }

    /// The configured language for transcription.
    pub fn language(&self) -> &str {
        &self.language
    }

    /// Transcribe audio samples (mono 16kHz f32) to text.
    ///
    /// # Arguments
    /// * `samples` - PCM f32 audio at 16kHz mono
    ///
    /// # Returns
    /// The transcribed text with segments joined.
    #[cfg(feature = "stt")]
    pub fn transcribe(&self, samples: &[f32]) -> Result<String> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow::anyhow!("failed to create whisper state: {:?}", e))?;

        let mut params =
            whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });

        params.set_language(Some(&self.language));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_single_segment(false);

        state
            .full(params, samples)
            .map_err(|e| anyhow::anyhow!("whisper transcription failed: {:?}", e))?;

        let num_segments = state
            .full_n_segments()
            .map_err(|e| anyhow::anyhow!("failed to get segment count: {:?}", e))?;

        let mut text = String::new();
        for i in 0..num_segments {
            let segment = state
                .full_get_segment_text(i)
                .map_err(|e| anyhow::anyhow!("failed to get segment {}: {:?}", i, e))?;
            text.push_str(segment.trim());
            if i < num_segments - 1 {
                text.push(' ');
            }
        }

        Ok(text)
    }

    /// Placeholder when STT feature is disabled.
    #[cfg(not(feature = "stt"))]
    pub fn transcribe(&self, _samples: &[f32]) -> Result<String> {
        anyhow::bail!("STT not available (compiled without 'stt' feature)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_model_errors() {
        let result = WhisperStt::new("/nonexistent/model.bin", "en");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
        assert!(err.contains("--download-model"));
    }

    #[test]
    fn test_language_accessor() {
        // Create a temp file to pass the existence check
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let result = WhisperStt::new(tmp.path().to_str().unwrap(), "fr");
        // Without stt feature, this succeeds (placeholder). With it, fails to parse model.
        #[cfg(not(feature = "stt"))]
        {
            let stt = result.unwrap();
            assert_eq!(stt.language(), "fr");
        }
        #[cfg(feature = "stt")]
        {
            // A temp file is not a valid whisper model, so loading should fail
            assert!(result.is_err());
        }
    }

    #[test]
    #[cfg(not(feature = "stt"))]
    fn test_transcribe_without_feature() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let stt = WhisperStt::new(tmp.path().to_str().unwrap(), "en").unwrap();
        let result = stt.transcribe(&[0.0; 16000]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not available"));
    }
}
