use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SampleRate, StreamConfig};
use hound::{WavSpec, WavWriter};
use serde::Serialize;
use std::io::Cursor;
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use thiserror::Error;

/// Target sample rate for whisper.cpp input.
const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Arc handles needed by the audio level emitter thread.
pub type LevelEmitterHandles = (Arc<AtomicBool>, Arc<Mutex<f32>>, Arc<Mutex<Option<String>>>);

/// Preferred sample rates to try when negotiating with the device.
/// Ordered by preference: whisper-native first, then common rates.
const PREFERRED_RATES: &[u32] = &[16_000, 44_100, 48_000];

#[derive(Error, Debug)]
pub enum AudioError {
    #[error("No audio input device available")]
    NoDevice,
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Failed to get default stream config: {0}")]
    ConfigError(String),
    #[error("Failed to build audio stream: {0}")]
    StreamError(String),
    #[error("Recording is not active")]
    NotRecording,
    #[error("Recording is already active")]
    AlreadyRecording,
    #[error("WAV encoding error: {0}")]
    WavError(String),
    #[error("Recording thread panicked")]
    ThreadPanic,
    #[error("Device disconnected during recording")]
    DeviceDisconnected,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Shared recording state that is Send + Sync (no cpal Stream stored here).
pub struct RecordingState {
    samples: Arc<Mutex<Vec<f32>>>,
    is_recording: Arc<AtomicBool>,
    peak_level: Arc<Mutex<f32>>,
    sample_rate: Mutex<u32>,
    /// Set by the stream error callback when the device disconnects or errors.
    recording_error: Arc<Mutex<Option<String>>>,
    /// Handle to the recording thread for panic detection.
    thread_handle: Mutex<Option<JoinHandle<()>>>,
}

/// The AudioRecorder manages recording lifecycle.
/// The cpal Stream is NOT stored — it lives on a dedicated thread.
pub struct AudioRecorder;

impl RecordingState {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            is_recording: Arc::new(AtomicBool::new(false)),
            peak_level: Arc::new(Mutex::new(0.0)),
            sample_rate: Mutex::new(TARGET_SAMPLE_RATE),
            recording_error: Arc::new(Mutex::new(None)),
            thread_handle: Mutex::new(None),
        }
    }

    pub fn get_level(&self) -> f32 {
        *self.peak_level.lock().unwrap()
    }

    #[allow(dead_code)]
    pub fn is_recording(&self) -> bool {
        self.is_recording.load(Ordering::SeqCst)
    }

    /// Returns the error message if the recording thread hit a device error.
    pub fn get_error(&self) -> Option<String> {
        self.recording_error.lock().unwrap().clone()
    }

    /// Returns cloned Arc handles for the audio level emitter thread.
    pub fn level_emitter_handles(&self) -> LevelEmitterHandles {
        (
            Arc::clone(&self.is_recording),
            Arc::clone(&self.peak_level),
            Arc::clone(&self.recording_error),
        )
    }
}

impl AudioRecorder {
    pub fn list_devices() -> Result<Vec<AudioDevice>, AudioError> {
        let host = cpal::default_host();
        let default_device = host.default_input_device();
        let default_name = default_device
            .as_ref()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();

        let mut devices = Vec::new();
        if let Ok(input_devices) = host.input_devices() {
            for (i, device) in input_devices.enumerate() {
                let name = device.name().unwrap_or_else(|_| format!("Device {}", i));
                let is_default = name == default_name;
                devices.push(AudioDevice {
                    id: name.clone(),
                    name,
                    is_default,
                });
            }
        }

        if devices.is_empty() {
            return Err(AudioError::NoDevice);
        }

        Ok(devices)
    }

    /// Start recording on a background thread. The cpal Stream lives
    /// on that thread and is dropped when stop() flips is_recording to false.
    pub fn start(state: &RecordingState, device_id: Option<String>) -> Result<(), AudioError> {
        if state.is_recording.load(Ordering::SeqCst) {
            return Err(AudioError::AlreadyRecording);
        }

        // Clean up any previous thread handle (shouldn't block — thread should be done)
        {
            let mut handle = state.thread_handle.lock().unwrap();
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }

        // Clear previous state
        {
            state.samples.lock().unwrap().clear();
            *state.recording_error.lock().unwrap() = None;
            *state.peak_level.lock().unwrap() = 0.0;
        }

        let host = cpal::default_host();
        let device = match &device_id {
            Some(id) => host
                .input_devices()
                .map_err(|e| AudioError::DeviceNotFound(e.to_string()))?
                .find(|d| d.name().map(|n| n == *id).unwrap_or(false))
                .ok_or_else(|| AudioError::DeviceNotFound(id.clone()))?,
            None => host.default_input_device().ok_or(AudioError::NoDevice)?,
        };

        // Negotiate sample rate: try preferred rates, fall back to device default
        let (config, sample_format) = negotiate_config(&device)?;
        let sr = config.sample_rate.0;
        *state.sample_rate.lock().unwrap() = sr;
        let channels = config.channels as usize;

        let samples = Arc::clone(&state.samples);
        let peak_level = Arc::clone(&state.peak_level);
        let is_recording = Arc::clone(&state.is_recording);
        let recording_error = Arc::clone(&state.recording_error);

        // Mark recording before spawning thread
        is_recording.store(true, Ordering::SeqCst);

        let is_recording_thread = Arc::clone(&is_recording);
        let recording_error_thread = Arc::clone(&recording_error);

        let handle = std::thread::Builder::new()
            .name("whisperi-audio".to_string())
            .spawn(move || {
                // Wrap in catch_unwind so is_recording is always reset on panic
                let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                    run_recording_thread(
                        &device,
                        RecordingThreadParams {
                            config,
                            sample_format,
                            channels,
                            samples,
                            peak_level,
                            is_recording: Arc::clone(&is_recording_thread),
                            recording_error: Arc::clone(&recording_error_thread),
                        },
                    );
                }));

                if result.is_err() {
                    log::error!("Audio recording thread panicked");
                    set_recording_error(&recording_error_thread, "Recording thread panicked unexpectedly".to_string());
                }

                // Always reset is_recording, even after panic
                is_recording_thread.store(false, Ordering::SeqCst);
            })
            .map_err(|e| AudioError::StreamError(format!("Failed to spawn thread: {}", e)))?;

        *state.thread_handle.lock().unwrap() = Some(handle);
        Ok(())
    }

    pub fn stop(state: &RecordingState) -> Result<Vec<u8>, AudioError> {
        if !state.is_recording.load(Ordering::SeqCst) {
            // Check if the thread panicked
            let mut handle = state.thread_handle.lock().unwrap();
            if let Some(h) = handle.take() {
                if h.join().is_err() {
                    return Err(AudioError::ThreadPanic);
                }
            }
            // Check if a device error was stored
            if let Some(err_msg) = state.get_error() {
                return Err(AudioError::StreamError(err_msg));
            }
            return Err(AudioError::NotRecording);
        }

        // Signal the recording thread to stop
        state.is_recording.store(false, Ordering::SeqCst);

        // Wait for the thread to finish (with timeout)
        {
            let mut handle = state.thread_handle.lock().unwrap();
            if let Some(h) = handle.take() {
                // The thread should stop quickly since we flipped is_recording
                match h.join() {
                    Ok(()) => {}
                    Err(_) => return Err(AudioError::ThreadPanic),
                }
            }
        }

        // Check for device errors that happened during recording
        if let Some(err_msg) = state.get_error() {
            // Still try to return whatever audio was captured before the error
            log::warn!("Device error during recording: {}", err_msg);
        }

        let samples = state.samples.lock().unwrap().clone();

        if samples.is_empty() {
            if state.get_error().is_some() {
                return Err(AudioError::DeviceDisconnected);
            }
            return Err(AudioError::NotRecording);
        }

        let sr = *state.sample_rate.lock().unwrap();

        // Resample to 16kHz mono if needed
        let mono_16k = if sr != TARGET_SAMPLE_RATE {
            resample(&samples, sr, TARGET_SAMPLE_RATE)
        } else {
            samples
        };

        encode_wav(&mono_16k, TARGET_SAMPLE_RATE)
    }
}

/// Negotiate the best stream config for the device.
/// Tries preferred sample rates first, then falls back to the device default.
fn negotiate_config(
    device: &cpal::Device,
) -> Result<(StreamConfig, SampleFormat), AudioError> {
    let supported_configs = device
        .supported_input_configs()
        .map_err(|e| AudioError::ConfigError(e.to_string()))?;

    let supported: Vec<_> = supported_configs.collect();

    // Try each preferred sample rate
    for &rate in PREFERRED_RATES {
        for cfg in &supported {
            if cfg.min_sample_rate().0 <= rate && cfg.max_sample_rate().0 >= rate {
                let config = StreamConfig {
                    channels: cfg.channels(),
                    sample_rate: SampleRate(rate),
                    buffer_size: cpal::BufferSize::Default,
                };
                return Ok((config, cfg.sample_format()));
            }
        }
    }

    // Fall back to device default config
    let default = device
        .default_input_config()
        .map_err(|e| AudioError::ConfigError(e.to_string()))?;

    let sample_format = default.sample_format();
    let config: StreamConfig = default.into();
    Ok((config, sample_format))
}

/// Parameters for the recording thread, bundled to satisfy clippy::too_many_arguments.
struct RecordingThreadParams {
    config: StreamConfig,
    sample_format: SampleFormat,
    channels: usize,
    samples: Arc<Mutex<Vec<f32>>>,
    peak_level: Arc<Mutex<f32>>,
    is_recording: Arc<AtomicBool>,
    recording_error: Arc<Mutex<Option<String>>>,
}

/// Store an error message in the shared recording error state.
fn set_recording_error(recording_error: &Mutex<Option<String>>, msg: String) {
    if let Ok(mut err) = recording_error.lock() {
        *err = Some(msg);
    }
}

/// The actual recording loop that runs on the dedicated thread.
fn run_recording_thread(device: &cpal::Device, params: RecordingThreadParams) {
    let RecordingThreadParams {
        config,
        sample_format,
        channels,
        samples,
        peak_level,
        is_recording,
        recording_error,
    } = params;
    let error_flag = Arc::clone(&recording_error);
    let is_rec_err = Arc::clone(&is_recording);

    let err_callback = move |err: cpal::StreamError| {
        log::error!("Audio stream error: {}", err);
        set_recording_error(&error_flag, err.to_string());
        is_rec_err.store(false, Ordering::SeqCst);
    };

    let stream_result = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        SampleFormat::I16 => build_stream::<i16>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        SampleFormat::U16 => build_stream::<u16>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        SampleFormat::I8 => build_stream::<i8>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        SampleFormat::U8 => build_stream::<u8>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        SampleFormat::I32 => build_stream::<i32>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        SampleFormat::U32 => build_stream::<u32>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        SampleFormat::I64 => build_stream::<i64>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        SampleFormat::U64 => build_stream::<u64>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        SampleFormat::F64 => build_stream::<f64>(
            device, &config, Arc::clone(&samples), Arc::clone(&peak_level), channels, err_callback,
        ),
        _ => {
            set_recording_error(&recording_error, format!("Unsupported sample format: {:?}", sample_format));
            return;
        }
    };

    let stream = match stream_result {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to build stream: {}", e);
            set_recording_error(&recording_error, e.to_string());
            return;
        }
    };

    if let Err(e) = stream.play() {
        log::error!("Failed to play stream: {}", e);
        set_recording_error(&recording_error, e.to_string());
        return;
    }

    // Keep the stream alive until recording is stopped
    while is_recording.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Stream is dropped here, stopping recording
    drop(stream);
}

fn build_stream<T: cpal::Sample + cpal::SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    peak_level: Arc<Mutex<f32>>,
    channels: usize,
    err_callback: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, AudioError>
where
    f32: cpal::FromSample<T>,
{
    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mut mono_samples = Vec::with_capacity(data.len() / channels);
                let mut peak: f32 = 0.0;

                for frame in data.chunks(channels) {
                    let sample: f32 = frame
                        .iter()
                        .map(|s| <f32 as cpal::Sample>::from_sample(*s))
                        .sum::<f32>()
                        / channels as f32;
                    mono_samples.push(sample);
                    peak = peak.max(sample.abs());
                }

                if let Ok(mut level) = peak_level.lock() {
                    *level = peak;
                }
                if let Ok(mut buf) = samples.lock() {
                    buf.extend_from_slice(&mono_samples);
                }
            },
            err_callback,
            None,
        )
        .map_err(|e| AudioError::StreamError(e.to_string()))?;

    Ok(stream)
}

/// Simple linear resampling from source rate to target rate
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < samples.len() {
            samples[idx] as f64 * (1.0 - frac) + samples[idx + 1] as f64 * frac
        } else if idx < samples.len() {
            samples[idx] as f64
        } else {
            0.0
        };

        output.push(sample as f32);
    }

    output
}

/// Encode f32 samples as a WAV byte buffer (16-bit PCM, 16kHz, mono)
fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, AudioError> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer =
            WavWriter::new(&mut cursor, spec).map_err(|e| AudioError::WavError(e.to_string()))?;

        for &sample in samples {
            let clamped = sample.clamp(-1.0, 1.0);
            let int_sample = (clamped * i16::MAX as f32) as i16;
            writer
                .write_sample(int_sample)
                .map_err(|e| AudioError::WavError(e.to_string()))?;
        }

        writer
            .finalize()
            .map_err(|e| AudioError::WavError(e.to_string()))?;
    }

    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_same_rate() {
        let input = vec![0.0, 0.5, 1.0, -1.0];
        let output = resample(&input, 16000, 16000);
        assert_eq!(input, output);
    }

    #[test]
    fn test_resample_downsample() {
        // 48kHz to 16kHz should produce ~1/3 the samples
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 / 4800.0).sin()).collect();
        let output = resample(&input, 48000, 16000);
        assert_eq!(output.len(), 1600);
    }

    #[test]
    fn test_encode_wav_produces_valid_header() {
        let samples = vec![0.0f32; 16000];
        let wav = encode_wav(&samples, 16000).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
    }

    #[test]
    fn test_list_devices_returns_something_or_no_device_error() {
        let result = AudioRecorder::list_devices();
        assert!(result.is_ok() || matches!(result, Err(AudioError::NoDevice)));
    }

    #[test]
    fn test_recording_state_defaults() {
        let state = RecordingState::new();
        assert!(!state.is_recording());
        assert_eq!(state.get_level(), 0.0);
        assert!(state.get_error().is_none());
    }

    #[test]
    fn test_stop_without_start_returns_not_recording() {
        let state = RecordingState::new();
        let result = AudioRecorder::stop(&state);
        assert!(matches!(result, Err(AudioError::NotRecording)));
    }
}
