use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const SAMPLE_RATE: u32 = 16000;

#[derive(Debug, Serialize)]
struct WhisperRequest {
    model: String,
    #[serde(rename = "response_format")]
    response_format: String,
    language: String,
}

#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
}

/// Transcribe audio using OpenAI-compatible Whisper API
pub async fn transcribe(audio: &[f32], sample_rate: u32, language: &str) -> Result<String> {
    // Get API settings from config or env
    let config = crate::config::AppConfig::load().unwrap_or_default();
    let api_base = if config.llm_api_base.is_empty() {
        std::env::var("WHISPER_API_BASE")
            .or_else(|_| std::env::var("OPENAI_API_BASE"))
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string())
    } else {
        config.llm_api_base.clone()
    };
    let api_key = if config.llm_api_key.is_empty() {
        std::env::var("OPENAI_API_KEY")
            .or_else(|_| std::env::var("WHISPER_API_KEY"))
            .unwrap_or_default()
    } else {
        config.llm_api_key.clone()
    };

    let client = Client::new();

    // Resample if needed
    let audio_data = if sample_rate != SAMPLE_RATE {
        crate::audio::resample(audio, sample_rate, SAMPLE_RATE)
    } else {
        audio.to_vec()
    };

    // Convert f32 to i16 PCM for WAV format
    let pcm_data: Vec<i16> = audio_data
        .iter()
        .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect();

    // Create WAV file in memory
    let wav_data = create_wav(&pcm_data, SAMPLE_RATE)?;

    // Create multipart form
    let part = reqwest::multipart::Part::bytes(wav_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", "whisper-1".to_string())
        .text("response_format", "json".to_string())
        .text("language", normalize_language(language));

    let url = format!("{}/audio/transcriptions", api_base);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(anyhow::anyhow!("Whisper API error: {}", error_text));
    }

    let result: WhisperResponse = response.json().await?;
    Ok(result.text)
}

fn normalize_language(lang: &str) -> String {
    // OpenAI Whisper uses ISO 639-1 codes
    match lang {
        "zh-CN" => "zh",
        "zh-TW" => "zh",
        "en" => "en",
        "ja" => "ja",
        "ko" => "ko",
        _ => lang,
    }
    .to_string()
}

fn create_wav(pcm_data: &[i16], sample_rate: u32) -> Result<Vec<u8>> {
    let num_channels = 1u16;
    let bits_per_sample = 16u16;
    let byte_rate = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
    let block_align = num_channels * bits_per_sample / 8;
    let data_size = pcm_data.len() * 2;
    let file_size = 36 + data_size;

    let mut wav_data = Vec::with_capacity(44 + data_size);

    // RIFF header
    wav_data.extend_from_slice(b"RIFF");
    wav_data.extend_from_slice(&(file_size as u32).to_le_bytes());
    wav_data.extend_from_slice(b"WAVE");

    // fmt chunk
    wav_data.extend_from_slice(b"fmt ");
    wav_data.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav_data.extend_from_slice(&1u16.to_le_bytes()); // audio format (PCM)
    wav_data.extend_from_slice(&num_channels.to_le_bytes());
    wav_data.extend_from_slice(&sample_rate.to_le_bytes());
    wav_data.extend_from_slice(&byte_rate.to_le_bytes());
    wav_data.extend_from_slice(&block_align.to_le_bytes());
    wav_data.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav_data.extend_from_slice(b"data");
    wav_data.extend_from_slice(&(data_size as u32).to_le_bytes());

    // PCM data
    for sample in pcm_data {
        wav_data.extend_from_slice(&sample.to_le_bytes());
    }

    Ok(wav_data)
}
