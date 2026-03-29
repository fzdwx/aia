use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use cpal::Sample;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

pub struct AudioChunk {
    pub data: Vec<f32>,
}

pub fn record_sync(
    audio_tx: std::sync::mpsc::Sender<AudioChunk>,
    rms_tx: async_channel::Sender<f32>,
    recording: Arc<AtomicBool>,
) -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow::anyhow!("No input device found"))?;

    let supported_config = device
        .default_input_config()
        .map_err(|e| anyhow::anyhow!("Failed to get default input config: {}", e))?;

    let sample_format = supported_config.sample_format();
    let config: StreamConfig = supported_config.into();

    log::info!(
        "Recording with config: {} channels, {} Hz, {:?}",
        config.channels,
        config.sample_rate.0,
        sample_format
    );

    let sample_rate = config.sample_rate.0;
    let channels = config.channels;
    let chunk_size = (sample_rate as usize / 10) * channels as usize; // 100ms chunks

    let (inner_audio_tx, inner_audio_rx) = std::sync::mpsc::channel::<Vec<f32>>();
    let (inner_rms_tx, inner_rms_rx) = std::sync::mpsc::channel::<f32>();

    let err_fn = |err| eprintln!("Audio stream error: {}", err);

    let stream = match sample_format {
        SampleFormat::I16 => build_input_stream::<i16>(
            &device,
            &config,
            inner_audio_tx,
            inner_rms_tx,
            channels,
            chunk_size,
            err_fn,
        )?,
        SampleFormat::I32 => build_input_stream::<i32>(
            &device,
            &config,
            inner_audio_tx,
            inner_rms_tx,
            channels,
            chunk_size,
            err_fn,
        )?,
        SampleFormat::F32 => build_input_stream::<f32>(
            &device,
            &config,
            inner_audio_tx,
            inner_rms_tx,
            channels,
            chunk_size,
            err_fn,
        )?,
        format => {
            return Err(anyhow::anyhow!("Unsupported sample format: {:?}", format));
        }
    };

    stream.play()?;

    // Process audio data in this thread
    while recording.load(Ordering::SeqCst) {
        // Check for audio chunks
        match inner_audio_rx.try_recv() {
            Ok(chunk) => {
                if audio_tx.send(AudioChunk { data: chunk }).is_err() {
                    break;
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
        }

        // Check for RMS updates
        match inner_rms_rx.try_recv() {
            Ok(rms) => {
                // Use try_send to avoid blocking
                let _ = rms_tx.send_blocking(rms);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    drop(stream);
    Ok(())
}

fn build_input_stream<T>(
    device: &Device,
    config: &StreamConfig,
    audio_tx: std::sync::mpsc::Sender<Vec<f32>>,
    rms_tx: std::sync::mpsc::Sender<f32>,
    channels: u16,
    chunk_size: usize,
    err_fn: impl Fn(cpal::StreamError) + Send + 'static,
) -> Result<Stream>
where
    T: Sample + cpal::SizedSample + Send + 'static,
    T::Float: Into<f32>,
{
    let mut buffer = Vec::with_capacity(chunk_size * 2);
    let mut rms_buffer = Vec::new();

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Convert to f32
            let samples: Vec<f32> = data.iter().map(|s| s.to_float_sample().into()).collect();

            // Collect mono samples (average channels)
            let mono: Vec<f32> = samples
                .chunks(channels as usize)
                .map(|chunk| {
                    let sum: f32 = chunk.iter().sum();
                    sum / channels as f32
                })
                .collect();

            buffer.extend_from_slice(&mono);

            // Calculate RMS for visualization
            rms_buffer.extend_from_slice(&mono);
            if rms_buffer.len() >= 1024 {
                let rms = calculate_rms(&rms_buffer);
                let _ = rms_tx.send(rms);
                rms_buffer.clear();
            }

            // Send chunk when ready
            if buffer.len() >= chunk_size {
                let _ = audio_tx.send(buffer.clone());
                buffer.clear();
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

fn calculate_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// Resample audio to target sample rate
pub fn resample(input: &[f32], input_rate: u32, output_rate: u32) -> Vec<f32> {
    if input_rate == output_rate {
        return input.to_vec();
    }

    let ratio = output_rate as f64 / input_rate as f64;
    let output_len = (input.len() as f64 * ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 / ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < input.len() {
            input[idx] * (1.0 - frac as f32) + input[idx + 1] * frac as f32
        } else if idx < input.len() {
            input[idx]
        } else {
            0.0
        };
        output.push(sample);
    }

    output
}
