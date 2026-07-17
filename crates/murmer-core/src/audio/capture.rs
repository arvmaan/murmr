#[cfg(feature = "audio")]
use anyhow::Context;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(feature = "audio")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Target sample rate for whisper.cpp input.
pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Audio capture from the default input device.
/// Captures mono 16kHz PCM f32 samples suitable for whisper.
pub struct AudioCapture {
    sample_rate: u32,
    recording: Arc<AtomicBool>,
}

impl AudioCapture {
    /// Create a new audio capture instance.
    pub fn new() -> Result<Self> {
        Ok(Self {
            sample_rate: WHISPER_SAMPLE_RATE,
            recording: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The sample rate this capture produces.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Whether recording is currently active.
    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }

    /// Get a handle to signal recording should stop.
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.recording.clone()
    }

    /// Record audio until the stop signal is set.
    /// Returns collected mono f32 samples at 16kHz.
    #[cfg(feature = "audio")]
    pub fn record_until_stopped(&self, stop_signal: Arc<AtomicBool>) -> Result<Vec<f32>> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no audio input device found")?;

        tracing::debug!("using input device: {:?}", device.name());

        let supported_config = device
            .default_input_config()
            .context("failed to get default input config")?;

        tracing::debug!("device config: {:?}", supported_config);

        let device_sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels() as usize;

        let samples: Arc<std::sync::Mutex<Vec<f32>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
        let samples_clone = samples.clone();

        self.recording.store(true, Ordering::Relaxed);
        let recording_flag = self.recording.clone();

        let stream_config = cpal::StreamConfig {
            channels: supported_config.channels(),
            sample_rate: supported_config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let stream = device
            .build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buffer = samples_clone.lock().unwrap_or_else(|e| e.into_inner());
                    // Convert to mono by averaging channels
                    for chunk in data.chunks(channels) {
                        let mono_sample: f32 = chunk.iter().sum::<f32>() / channels as f32;
                        buffer.push(mono_sample);
                    }
                },
                |err| {
                    tracing::error!("audio stream error: {}", err);
                },
                None,
            )
            .context("failed to build audio input stream")?;

        stream.play().context("failed to start audio stream")?;

        // Wait until stop signal
        while !stop_signal.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        recording_flag.store(false, Ordering::Relaxed);
        drop(stream);

        let raw_samples = match Arc::try_unwrap(samples) {
            Ok(mutex) => mutex.into_inner().unwrap_or_default(),
            Err(arc) => arc.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        };

        // Resample to 16kHz if needed
        if device_sample_rate != WHISPER_SAMPLE_RATE {
            Ok(resample(
                &raw_samples,
                device_sample_rate,
                WHISPER_SAMPLE_RATE,
            ))
        } else {
            Ok(raw_samples)
        }
    }

    /// Placeholder for when audio feature is disabled.
    #[cfg(not(feature = "audio"))]
    pub fn record_until_stopped(&self, _stop_signal: Arc<AtomicBool>) -> Result<Vec<f32>> {
        anyhow::bail!("audio capture not available (compiled without 'audio' feature)")
    }
}

/// Simple linear interpolation resampling.
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_pos = i as f64 * ratio;
        let src_idx = src_pos as usize;
        let frac = (src_pos - src_idx as f64) as f32;

        if src_idx + 1 < samples.len() {
            let sample = samples[src_idx] * (1.0 - frac) + samples[src_idx + 1] * frac;
            output.push(sample);
        } else if src_idx < samples.len() {
            output.push(samples[src_idx]);
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_capture_creation() {
        let capture = AudioCapture::new().unwrap();
        assert_eq!(capture.sample_rate(), WHISPER_SAMPLE_RATE);
        assert!(!capture.is_recording());
    }

    #[test]
    fn test_resample_same_rate() {
        let samples = vec![1.0, 2.0, 3.0, 4.0];
        let result = resample(&samples, 16000, 16000);
        assert_eq!(result, samples);
    }

    #[test]
    fn test_resample_downsample() {
        let samples: Vec<f32> = (0..48000).map(|i| (i as f32) / 48000.0).collect();
        let result = resample(&samples, 48000, 16000);
        // Output should be approximately 1/3 the length
        assert!((result.len() as f64 - 16000.0).abs() < 2.0);
    }

    #[test]
    fn test_resample_empty() {
        let result = resample(&[], 44100, 16000);
        assert!(result.is_empty());
    }

    #[test]
    fn test_stop_handle() {
        let capture = AudioCapture::new().unwrap();
        let handle = capture.stop_handle();
        assert!(!handle.load(Ordering::Relaxed));
        handle.store(true, Ordering::Relaxed);
        assert!(handle.load(Ordering::Relaxed));
    }
}
