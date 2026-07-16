use anyhow::Result;

pub struct AudioCapture {
    sample_rate: u32,
}

impl AudioCapture {
    pub fn new(sample_rate: u32) -> Result<Self> {
        Ok(Self { sample_rate })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Start capturing audio, returning samples via the callback.
    /// Captures mono 16kHz PCM f32 samples suitable for whisper.
    pub async fn start<F>(&self, _on_samples: F) -> Result<()>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        // TODO: implement cpal capture
        // 1. Enumerate devices, select default input
        // 2. Open stream at 16kHz mono f32
        // 3. Feed samples to callback
        todo!("implement audio capture")
    }

    pub fn stop(&self) -> Result<()> {
        // TODO: stop the cpal stream
        todo!("implement audio stop")
    }
}
