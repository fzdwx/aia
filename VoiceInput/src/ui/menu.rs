use anyhow::Result;
use gtk4::prelude::*;
use gtk4::{glib, AboutDialog, Application};
use gio::{Menu, MenuItem, SimpleAction};
use std::sync::Arc;

use crate::config::AppConfig;
use crate::ui::settings_dialog;

pub fn create_menu(app: &Application, config: Arc<parking_lot::RwLock<AppConfig>>) -> Result<()> {
    // Create menu
    let menu = Menu::new();

    // Language submenu
    let language_menu = Menu::new();
    for lang_info in AppConfig::available_languages() {
        let lang_item = MenuItem::new(Some(lang_info.name), None);
        lang_item.set_action_and_target_value(
            Some("app.language"),
            Some(&glib::Variant::from(lang_info.code)),
        );
        language_menu.append_item(&lang_item);
    }
    menu.append_submenu(Some("Language"), &language_menu);

    // LLM Refinement submenu
    let llm_menu = Menu::new();

    let llm_enable_item = MenuItem::new(Some("Enable LLM Refinement"), None);
    llm_enable_item.set_action_and_target_value(Some("app.llm_enable"), None);
    llm_menu.append_item(&llm_enable_item);

    let llm_settings_item = MenuItem::new(Some("Settings..."), None);
    llm_settings_item.set_action_and_target_value(Some("app.llm_settings"), None);
    llm_menu.append_item(&llm_settings_item);

    menu.append_submenu(Some("LLM Refinement"), &llm_menu);

    // Separator
    menu.append_section(None, &Menu::new());

    // About
    let about_item = MenuItem::new(Some("About"), None);
    about_item.set_action_and_target_value(Some("app.about"), None);
    menu.append_item(&about_item);

    // Quit
    let quit_item = MenuItem::new(Some("Quit"), None);
    quit_item.set_action_and_target_value(Some("app.quit"), None);
    menu.append_item(&quit_item);

    app.set_menubar(Some(&menu));

    // Create actions

    // Language action
    let config_clone = config.clone();
    let language_action = SimpleAction::new_stateful(
        "language",
        Some(glib::VariantTy::STRING),
        &glib::Variant::from(config.read().language.as_str()),
    );
    language_action.connect_activate(move |action, param| {
        if let Some(lang) = param.and_then(|p| p.get::<String>()) {
            let mut cfg = config_clone.write();
            cfg.language = lang.clone();
            if let Err(e) = cfg.save() {
                eprintln!("Failed to save config: {}", e);
            }
            action.set_state(&glib::Variant::from(lang));
            log::info!("Language changed to: {}", cfg.language);
        }
    });
    app.add_action(&language_action);

    // LLM Enable action
    let config_clone = config.clone();
    let llm_enable_action = SimpleAction::new_stateful(
        "llm_enable",
        None,
        &glib::Variant::from(config.read().llm_enabled),
    );
    llm_enable_action.connect_activate(move |action, _| {
        let mut cfg = config_clone.write();
        cfg.llm_enabled = !cfg.llm_enabled;
        if let Err(e) = cfg.save() {
            eprintln!("Failed to save config: {}", e);
        }
        action.set_state(&glib::Variant::from(cfg.llm_enabled));
        log::info!("LLM refinement enabled: {}", cfg.llm_enabled);
    });
    app.add_action(&llm_enable_action);

    // LLM Settings action
    let app_clone = app.clone();
    let config_clone = config.clone();
    let llm_settings_action = SimpleAction::new("llm_settings", None);
    llm_settings_action.connect_activate(move |_, _| {
        if let Err(e) = settings_dialog::show_settings_dialog(&app_clone, config_clone.clone()) {
            eprintln!("Failed to show settings dialog: {}", e);
        }
    });
    app.add_action(&llm_settings_action);

    // About action
    let about_action = SimpleAction::new("about", None);
    about_action.connect_activate(|_, _| {
        let dialog = AboutDialog::builder()
            .title("About Voice Input")
            .program_name("Voice Input")
            .version("0.1.0")
            .comments("A voice input method for Linux")
            .authors(vec!["Aia".to_string()])
            .build();
        dialog.present();
    });
    app.add_action(&about_action);

    // Quit action
    let quit_action = SimpleAction::new("quit", None);
    quit_action.connect_activate(|_, _| {
        std::process::exit(0);
    });
    app.add_action(&quit_action);

    Ok(())
}
