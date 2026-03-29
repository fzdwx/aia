use anyhow::Result;
use gtk4::prelude::*;
use gtk4::{
    glib, Application, ApplicationWindow, Button, Entry, Label, Spinner, Box, Orientation,
};
use std::sync::Arc;

use crate::config::AppConfig;
use crate::llm;

pub fn show_settings_dialog(app: &Application, config: Arc<parking_lot::RwLock<AppConfig>>) -> Result<()> {
    // Create a simple settings window
    let window = ApplicationWindow::builder()
        .application(app)
        .title("LLM Settings")
        .default_width(450)
        .default_height(300)
        .resizable(false)
        .css_classes(vec!["settings-window".to_string()])
        .build();

    // Load CSS
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(include_str!("style.css"));
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Container
    let container = Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .margin_start(24)
        .margin_end(24)
        .margin_top(24)
        .margin_bottom(24)
        .build();

    // Title
    let title = Label::builder()
        .label("LLM Refinement Settings")
        .css_classes(vec!["settings-title".to_string()])
        .halign(gtk4::Align::Start)
        .build();
    container.append(&title);

    // API Base URL
    let api_base_label = Label::builder()
        .label("API Base URL")
        .css_classes(vec!["settings-label".to_string()])
        .halign(gtk4::Align::Start)
        .build();
    container.append(&api_base_label);

    let api_base_entry = Entry::builder()
        .placeholder_text("https://api.openai.com/v1")
        .css_classes(vec!["settings-entry".to_string()])
        .build();
    api_base_entry.set_text(&config.read().llm_api_base);
    container.append(&api_base_entry);

    // API Key
    let api_key_label = Label::builder()
        .label("API Key")
        .css_classes(vec!["settings-label".to_string()])
        .halign(gtk4::Align::Start)
        .build();
    container.append(&api_key_label);

    let api_key_entry = Entry::builder()
        .placeholder_text("sk-...")
        .css_classes(vec!["settings-entry".to_string()])
        .visibility(false)
        .build();
    api_key_entry.set_text(&config.read().llm_api_key);
    container.append(&api_key_entry);

    // Model
    let model_label = Label::builder()
        .label("Model")
        .css_classes(vec!["settings-label".to_string()])
        .halign(gtk4::Align::Start)
        .build();
    container.append(&model_label);

    let model_entry = Entry::builder()
        .placeholder_text("gpt-4o-mini")
        .css_classes(vec!["settings-entry".to_string()])
        .build();
    model_entry.set_text(&config.read().llm_model);
    container.append(&model_entry);

    // Buttons
    let button_box = Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(12)
        .margin_top(8)
        .build();

    let test_button = Button::builder()
        .label("Test")
        .css_classes(vec!["settings-button-secondary".to_string()])
        .build();

    let test_spinner = Spinner::new();
    let test_label = Label::builder()
        .label("")
        .css_classes(vec!["settings-label".to_string()])
        .build();

    let api_base_clone = api_base_entry.clone();
    let api_key_clone = api_key_entry.clone();
    let model_clone = model_entry.clone();
    let test_button_clone = test_button.clone();
    let test_spinner_clone = test_spinner.clone();
    let test_label_clone = test_label.clone();

    test_button.connect_clicked(move |_| {
        let api_base = api_base_clone.text().to_string();
        let api_key = api_key_clone.text().to_string();
        let model = model_clone.text().to_string();

        if api_key.is_empty() || model.is_empty() {
            test_label_clone.set_label("Please fill all fields");
            return;
        }

        test_button_clone.set_sensitive(false);
        test_spinner_clone.set_spinning(true);
        test_label_clone.set_label("Testing...");

        let test_button_inner = test_button_clone.clone();
        let test_spinner_inner = test_spinner_clone.clone();
        let test_label_inner = test_label_clone.clone();

        // Use glib::spawn_blocking_local for thread-safe GTK updates
        glib::MainContext::default().spawn_local(async move {
            let result = llm::test_connection(&api_base, &api_key, &model).await;

            test_spinner_inner.set_spinning(false);
            test_button_inner.set_sensitive(true);

            match result {
                Ok(true) => test_label_inner.set_label("✓ Connection successful"),
                Ok(false) => test_label_inner.set_label("✗ Connection failed"),
                Err(e) => test_label_inner.set_label(&format!("✗ Error: {}", e)),
            }
        });
    });

    button_box.append(&test_button);
    button_box.append(&test_spinner);
    button_box.append(&test_label);

    // Spacer
    let spacer = Box::builder()
        .orientation(Orientation::Vertical)
        .vexpand(true)
        .build();
    container.append(&spacer);
    container.append(&button_box);

    // Save button
    let save_button = Button::builder()
        .label("Save")
        .css_classes(vec!["settings-button".to_string()])
        .halign(gtk4::Align::End)
        .margin_top(8)
        .build();

    let api_base_save = api_base_entry.clone();
    let api_key_save = api_key_entry.clone();
    let model_save = model_entry.clone();
    let window_clone = window.clone();
    let config_save = config.clone();

    save_button.connect_clicked(move |_| {
        let mut cfg = config_save.write();
        cfg.llm_api_base = api_base_save.text().to_string();
        cfg.llm_api_key = api_key_save.text().to_string();
        cfg.llm_model = model_save.text().to_string();

        if let Err(e) = cfg.save() {
            eprintln!("Failed to save config: {}", e);
        } else {
            log::info!("LLM settings saved");
        }

        window_clone.close();
    });

    container.append(&save_button);

    window.set_child(Some(&container));
    window.present();

    Ok(())
}
