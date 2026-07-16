use anyhow::Result;

pub struct VoiceActivityDetector {
    threshold: f32,
}

impl VoiceActivityDetector {
    pub fn new(threshold: f32) -> Result<Self> {
        // TODO: Load Silero VAD ONNX model via ort
        Ok(Self { threshold })
    }

    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Returns true if the audio chunk contains speech.
    pub fn is_speech(&mut self, _samples: &[f32]) -> Result<bool> {
        // TODO: Run Silero VAD inference
        // 1. Prepare input tensor (512 or 1536 samples at 16kHz)
        // 2. Run ONNX session
        // 3. Compare output probability to threshold
        todo!("implement VAD inference")
    }

    pub fn reset(&mut self) {
        // TODO: Reset internal VAD state between utterances
    }
}
