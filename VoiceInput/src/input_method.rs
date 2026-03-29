use anyhow::Result;
use arboard::Clipboard;
use std::process::Command;

/// Inject text into the currently focused input field
pub async fn inject_text(text: &str) -> Result<()> {
    // Save current clipboard content
    let mut clipboard = Clipboard::new()?;
    let original_clipboard = clipboard.get_text().ok();

    // Check current input method
    let input_method = get_current_input_method();
    let need_switch = is_cjk_input_method(&input_method);

    // If using CJK input method, temporarily switch to ASCII
    if need_switch {
        switch_to_ascii_input_method()?;
    }

    // Small delay for input method switch
    if need_switch {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Set text to clipboard
    clipboard.set_text(text)?;

    // Small delay
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Simulate Ctrl+V (or Cmd+V on macOS, but we're on Linux)
    simulate_paste()?;

    // Small delay
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Restore original clipboard content
    if let Some(original) = original_clipboard {
        clipboard.set_text(original)?;
    }

    // Switch back to original input method if needed
    if need_switch {
        restore_input_method(&input_method)?;
    }

    Ok(())
}

/// Get current input method identifier
fn get_current_input_method() -> String {
    // Try fcitx5 first
    if let Ok(output) = Command::new("fcitx5-remote").output() {
        if output.status.success() {
            if let Ok(active) = String::from_utf8(output.stdout) {
                // fcitx5-remote returns the current input method name
                return format!("fcitx5:{}", active.trim());
            }
        }
    }

    // Try fcitx
    if let Ok(output) = Command::new("fcitx-remote").output() {
        if output.status.success() {
            let status = output.stdout.first().copied().unwrap_or(0);
            if status == 2 {
                return "fcitx:active".to_string();
            } else if status == 1 {
                return "fcitx:inactive".to_string();
            }
        }
    }

    // Try ibus
    if let Ok(output) = Command::new("ibus").args(["engine"]).output() {
        if output.status.success() {
            if let Ok(engine) = String::from_utf8(output.stdout) {
                return format!("ibus:{}", engine.trim());
            }
        }
    }

    "unknown".to_string()
}

/// Check if the input method is a CJK (Chinese/Japanese/Korean) input method
fn is_cjk_input_method(im: &str) -> bool {
    if im.starts_with("fcitx5:") {
        let name = im.strip_prefix("fcitx5:").unwrap_or("");
        // Common CJK input method engines for fcitx5
        return name.contains("pinyin")
            || name.contains("rime")
            || name.contains("librime")
            || name.contains("chinese")
            || name.contains("japanese")
            || name.contains("korean")
            || name.contains("hangul")
            || name.contains("mozc")
            || name.contains("anthy")
            || name.contains("zh")
            || name.contains("ja")
            || name.contains("ko");
    }

    if im.starts_with("fcitx:") {
        return im.contains("active");
    }

    if im.starts_with("ibus:") {
        let engine = im.strip_prefix("ibus:").unwrap_or("");
        // Common CJK engines for ibus
        return engine.contains("pinyin")
            || engine.contains("libpinyin")
            || engine.contains("rime")
            || engine.contains("chewing")
            || engine.contains("mozc")
            || engine.contains("anthy")
            || engine.contains("hangul")
            || engine.contains("zh")
            || engine.contains("ja")
            || engine.contains("ko");
    }

    false
}

/// Switch to ASCII input method
fn switch_to_ascii_input_method() -> Result<()> {
    // Try fcitx5 first
    if Command::new("fcitx5-remote")
        .args(["-s", "keyboard-us"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(());
    }

    // Try fcitx
    if Command::new("fcitx-remote")
        .args(["-c"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(());
    }

    // Try ibus
    if Command::new("ibus")
        .args(["engine", "xkb:us::eng"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(());
    }

    // If nothing works, just continue
    Ok(())
}

/// Restore original input method
fn restore_input_method(im: &str) -> Result<()> {
    if im.starts_with("fcitx5:") {
        let name = im.strip_prefix("fcitx5:").unwrap_or("");
        if !name.is_empty() {
            let _ = Command::new("fcitx5-remote").args(["-s", name]).status();
        }
        return Ok(());
    }

    if im.starts_with("fcitx:") && im.contains("active") {
        let _ = Command::new("fcitx-remote").args(["-o"]).status();
        return Ok(());
    }

    if im.starts_with("ibus:") {
        let engine = im.strip_prefix("ibus:").unwrap_or("");
        if !engine.is_empty() {
            let _ = Command::new("ibus").args(["engine", engine]).status();
        }
        return Ok(());
    }

    Ok(())
}

/// Simulate Ctrl+V paste
fn simulate_paste() -> Result<()> {
    // Use xdotool to simulate Ctrl+V
    Command::new("xdotool")
        .args(["key", "--clearmodifiers", "Ctrl+V"])
        .status()?;

    Ok(())
}
