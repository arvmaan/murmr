use anyhow::Result;

/// Silero Voice Activity Detector.
///
/// Processes audio chunks and determines if they contain speech.
/// Uses the Silero VAD ONNX model via the `ort` crate.
///
/// When compiled without the `vad` feature, a simple energy-based
/// fallback is used instead.
pub struct VoiceActivityDetector {
    threshold: f32,
    #[cfg(feature = "vad")]
    session: ort::Session,
    #[cfg(feature = "vad")]
    state: ndarray::Array2<f32>,
    #[cfg(feature = "vad")]
    sr: ndarray::Array1<i64>,
}

/// Number of samples per VAD chunk at 16kHz (32ms window).
pub const VAD_CHUNK_SIZE: usize = 512;

impl VoiceActivityDetector {
    /// Create a new VAD with the given speech detection threshold (0.0 - 1.0).
    ///
    /// Higher threshold means more confident speech detection (fewer false positives).
    /// Recommended: 0.5 for normal use, 0.3 for sensitive detection.
    #[cfg(feature = "vad")]
    pub fn new(threshold: f32, model_path: &str) -> Result<Self> {
        use anyhow::Context;

        let session = ort::Session::builder()
            .context("failed to create ONNX session builder")?
            .commit_from_file(model_path)
            .context("failed to load Silero VAD model")?;

        let state = ndarray::Array2::<f32>::zeros((2, 128));
        let sr = ndarray::Array1::<i64>::from_elem(1, 16000);

        Ok(Self {
            threshold,
            session,
            state,
            sr,
        })
    }

    /// Create a fallback energy-based VAD (no ONNX model needed).
    #[cfg(not(feature = "vad"))]
    pub fn new(threshold: f32, _model_path: &str) -> Result<Self> {
        tracing::warn!("VAD compiled without ONNX support, using energy-based fallback");
        Ok(Self { threshold })
    }

    /// The configured speech probability threshold.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Returns true if the audio chunk contains speech.
    ///
    /// Expects exactly `VAD_CHUNK_SIZE` (512) f32 samples at 16kHz.
    #[cfg(feature = "vad")]
    pub fn is_speech(&mut self, samples: &[f32]) -> Result<bool> {
        use ndarray::Array2;

        if samples.len() != VAD_CHUNK_SIZE {
            anyhow::bail!(
                "VAD expects {} samples, got {}",
                VAD_CHUNK_SIZE,
                samples.len()
            );
        }

        let input = Array2::from_shape_vec((1, VAD_CHUNK_SIZE), samples.to_vec())
            .map_err(|e| anyhow::anyhow!("failed to create input tensor: {}", e))?;

        let outputs = self.session.run(ort::inputs![
            "input" => input.view(),
            "state" => self.state.view(),
            "sr" => self.sr.view(),
        ]?)?;

        let output = outputs["output"].try_extract_tensor::<f32>()?;
        let probability = output[[0, 0]];

        // Update internal state for next call
        let new_state = outputs["stateN"].try_extract_tensor::<f32>()?;
        self.state
            .assign(&new_state.into_dimensionality::<ndarray::Ix2>()?);

        Ok(probability >= self.threshold)
    }

    /// Energy-based fallback when compiled without ONNX.
    #[cfg(not(feature = "vad"))]
    pub fn is_speech(&mut self, samples: &[f32]) -> Result<bool> {
        let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
        let rms = energy.sqrt();
        Ok(rms >= self.threshold)
    }

    /// Reset internal state between utterances.
    pub fn reset(&mut self) {
        #[cfg(feature = "vad")]
        {
            self.state.fill(0.0);
        }
    }
}

/// Filter audio samples to only keep speech segments.
///
/// Processes the input in `VAD_CHUNK_SIZE` chunks and returns only
/// chunks where speech was detected, with optional padding.
pub fn filter_speech(
    vad: &mut VoiceActivityDetector,
    samples: &[f32],
    pad_chunks: usize,
) -> Result<Vec<f32>> {
    let mut output = Vec::new();
    let mut speech_started = false;
    let mut silence_count = 0;

    for chunk in samples.chunks(VAD_CHUNK_SIZE) {
        if chunk.len() < VAD_CHUNK_SIZE {
            // Pad final chunk with zeros
            let mut padded = chunk.to_vec();
            padded.resize(VAD_CHUNK_SIZE, 0.0);
            if vad.is_speech(&padded)? {
                output.extend_from_slice(chunk);
                speech_started = true;
                silence_count = 0;
            } else if speech_started {
                silence_count += 1;
                if silence_count <= pad_chunks {
                    output.extend_from_slice(chunk);
                }
            }
        } else if vad.is_speech(chunk)? {
            output.extend_from_slice(chunk);
            speech_started = true;
            silence_count = 0;
        } else if speech_started {
            silence_count += 1;
            if silence_count <= pad_chunks {
                output.extend_from_slice(chunk);
            }
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vad_creation() {
        let vad = VoiceActivityDetector::new(0.5, "/nonexistent/model.onnx");
        // Without the vad feature, this should succeed (energy-based fallback)
        #[cfg(not(feature = "vad"))]
        assert!(vad.is_ok());
        // With the vad feature, this would fail (model not found) - that's expected
        #[cfg(feature = "vad")]
        assert!(vad.is_err());
    }

    #[test]
    fn test_threshold() {
        let vad = VoiceActivityDetector::new(0.5, "/dev/null");
        #[cfg(not(feature = "vad"))]
        assert_eq!(vad.unwrap().threshold(), 0.5);
    }

    #[test]
    #[cfg(not(feature = "vad"))]
    fn test_energy_vad_silence() {
        let mut vad = VoiceActivityDetector::new(0.01, "/dev/null").unwrap();
        let silence = vec![0.0f32; VAD_CHUNK_SIZE];
        assert!(!vad.is_speech(&silence).unwrap());
    }

    #[test]
    #[cfg(not(feature = "vad"))]
    fn test_energy_vad_speech() {
        let mut vad = VoiceActivityDetector::new(0.01, "/dev/null").unwrap();
        let loud: Vec<f32> = (0..VAD_CHUNK_SIZE)
            .map(|i| (i as f32 * 0.1).sin() * 0.5)
            .collect();
        assert!(vad.is_speech(&loud).unwrap());
    }

    #[test]
    #[cfg(not(feature = "vad"))]
    fn test_filter_speech_all_silence() {
        let mut vad = VoiceActivityDetector::new(0.01, "/dev/null").unwrap();
        let silence = vec![0.0f32; VAD_CHUNK_SIZE * 10];
        let result = filter_speech(&mut vad, &silence, 2).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    #[cfg(not(feature = "vad"))]
    fn test_filter_speech_with_audio() {
        let mut vad = VoiceActivityDetector::new(0.01, "/dev/null").unwrap();
        let mut samples = vec![0.0f32; VAD_CHUNK_SIZE * 3]; // silence
        let loud: Vec<f32> = (0..VAD_CHUNK_SIZE)
            .map(|i| (i as f32 * 0.1).sin() * 0.5)
            .collect();
        samples.extend_from_slice(&loud); // speech
        samples.extend(vec![0.0f32; VAD_CHUNK_SIZE * 3]); // silence after

        let result = filter_speech(&mut vad, &samples, 1).unwrap();
        // Should contain the speech chunk plus 1 padding chunk
        assert!(!result.is_empty());
        assert!(result.len() <= VAD_CHUNK_SIZE * 2);
    }
}
