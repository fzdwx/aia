use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow};
use tokio::sync::mpsc;

mod audio;
mod config;
mod input_method;
mod keyboard;
mod llm;
mod ui;
mod whisper;

use config::AppConfig;
use keyboard::KeyEvent;

static RECORDING: AtomicBool = AtomicBool::new(false);

// Commands from background threads to main thread
enum UiCommand {
    ShowWindow,
    HideWindow,
    ShowText(String),
    UpdateRms(f32),
}

fn main() -> Result<()> {
    env_logger::init();

    // Load config
    let config = AppConfig::load()?;
    let config = Arc::new(parking_lot::RwLock::new(config));

    // Create GTK application
    let app = Application::builder()
        .application_id("com.aia.voiceinput")
        .build();

    let config_clone = config.clone();
    app.connect_activate(move |app| {
        if let Err(e) = build_ui(app, config_clone.clone()) {
            eprintln!("Error building UI: {}", e);
        }
    });

    // Run with args
    let args: Vec<String> = std::env::args().collect();
    app.run_with_args(&args);

    Ok(())
}

fn build_ui(app: &Application, config: Arc<parking_lot::RwLock<AppConfig>>) -> Result<()> {
    // Create menu
    ui::menu::create_menu(app, config.clone())?;

    // Create a hidden window (required for menu to work)
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Voice Input")
        .default_width(1)
        .default_height(1)
        .decorated(false)
        .build();

    window.present();
    window.hide();

    // Create floating window (hidden initially)
    let floating_window = ui::FloatingWindow::new(app, config.clone());

    // Channels
    let (audio_tx, audio_rx) = mpsc::channel::<audio::AudioChunk>(32);
    let (rms_tx, rms_rx) = async_channel::bounded::<f32>(64);
    let (ui_tx, ui_rx) = async_channel::bounded::<UiCommand>(64);
    let (key_tx, key_rx) = mpsc::channel::<KeyEvent>(16);

    // Spawn keyboard listener in background thread
    let key_tx_clone = key_tx.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if let Err(e) = keyboard::listen(key_tx_clone).await {
                eprintln!("Keyboard listener error: {}", e);
            }
        });
    });

    // Handle UI commands on main thread
    let fw = floating_window.clone();
    glib::idle_add_local(move || {
        // Process UI commands
        while let Ok(cmd) = ui_rx.try_recv() {
            match cmd {
                UiCommand::ShowWindow => fw.show(),
                UiCommand::HideWindow => fw.hide_with_animation(),
                UiCommand::ShowText(text) => fw.show_with_text(&text),
                UiCommand::UpdateRms(rms) => fw.update_rms(rms),
            }
        }

        // Process RMS updates
        while let Ok(rms) = rms_rx.try_recv() {
            fw.update_rms(rms);
        }

        glib::ControlFlow::Continue
    });

    // Handle keyboard events in background
    let recording_flag = Arc::new(AtomicBool::new(false));
    let recording_flag_clone = recording_flag.clone();
    let audio_tx_clone = audio_tx.clone();
    let rms_tx_clone = rms_tx.clone();
    let ui_tx_clone = ui_tx.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut is_recording = false;
            let mut audio_handle: Option<tokio::task::JoinHandle<()>> = None;
            let mut key_rx = key_rx;

            while let Some(event) = key_rx.recv().await {
                match event {
                    KeyEvent::FnPressed => {
                        if !is_recording {
                            is_recording = true;
                            recording_flag_clone.store(true, Ordering::SeqCst);
                            RECORDING.store(true, Ordering::SeqCst);

                            let _ = ui_tx_clone.send(UiCommand::ShowWindow).await;

                            // Start audio recording
                            let audio_tx_inner = audio_tx_clone.clone();
                            let rms_tx_inner = rms_tx_clone.clone();
                            let recording = recording_flag_clone.clone();

                            audio_handle = Some(tokio::spawn(async move {
                                if let Err(e) = audio::record(audio_tx_inner, rms_tx_inner, recording).await {
                                    eprintln!("Audio recording error: {}", e);
                                }
                            }));
                        }
                    }
                    KeyEvent::FnReleased => {
                        if is_recording {
                            is_recording = false;
                            recording_flag_clone.store(false, Ordering::SeqCst);
                            RECORDING.store(false, Ordering::SeqCst);

                            if let Some(handle) = audio_handle.take() {
                                handle.abort();
                            }

                            drop(audio_tx_clone.clone());
                            let _ = ui_tx_clone.send(UiCommand::HideWindow).await;
                        }
                    }
                }
            }
        });
    });

    // Process audio to text in background
    let config_clone3 = config.clone();
    let ui_tx_clone2 = ui_tx.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut audio_chunks = Vec::new();
            let mut rx = audio_rx;

            while let Some(chunk) = rx.recv().await {
                audio_chunks.push(chunk);
            }

            if audio_chunks.is_empty() {
                return;
            }

            let _ = ui_tx_clone2.send(UiCommand::ShowText("Transcribing...".to_string())).await;

            let lang = config_clone3.read().language.clone();
            let all_audio: Vec<f32> = audio_chunks.iter().flat_map(|c| c.data.clone()).collect();

            match whisper::transcribe(&all_audio, 16000, &lang).await {
                Ok(text) => {
                    let text = text.trim().to_string();
                    if text.is_empty() {
                        return;
                    }

                    // LLM refinement if enabled
                    let final_text = {
                        let cfg = config_clone3.read();
                        let (api_base, api_key, model, enabled) = (
                            cfg.llm_api_base.clone(),
                            cfg.llm_api_key.clone(),
                            cfg.llm_model.clone(),
                            cfg.llm_enabled,
                        );
                        drop(cfg);
                        
                        if enabled && !api_key.is_empty() && !model.is_empty() {
                            let _ = ui_tx_clone2.send(UiCommand::ShowText("Refining...".to_string())).await;
                            tokio::time::sleep(Duration::from_millis(100)).await;

                            match llm::refine_text(&api_base, &api_key, &model, &text).await {
                                Ok(refined) => refined,
                                Err(e) => {
                                    eprintln!("LLM refinement error: {}", e);
                                    text
                                }
                            }
                        } else {
                            text
                        }
                    };

                    let _ = ui_tx_clone2.send(UiCommand::ShowText(final_text.clone())).await;
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    let _ = ui_tx_clone2.send(UiCommand::HideWindow).await;

                    if let Err(e) = input_method::inject_text(&final_text).await {
                        eprintln!("Text injection error: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Transcription error: {}", e);
                    let _ = ui_tx_clone2.send(UiCommand::HideWindow).await;
                }
            }
        });
    });

    Ok(())
}
