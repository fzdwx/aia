use anyhow::Result;
use evdev::{Device, InputEventKind, Key};
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyEvent {
    FnPressed,
    FnReleased,
}

pub async fn listen(tx: Sender<KeyEvent>) -> Result<()> {
    // Find all keyboard devices
    let devices = find_keyboard_devices()?;

    if devices.is_empty() {
        return Err(anyhow::anyhow!("No keyboard devices found"));
    }

    log::info!("Found {} keyboard devices", devices.len());

    // Spawn a task for each keyboard device
    let mut handles = Vec::new();

    for mut device in devices {
        let tx_clone = tx.clone();
        let handle = tokio::spawn(async move {
            let mut fn_pressed = false;

            loop {
                match device.fetch_events() {
                    Ok(events) => {
                        for event in events {
                            match event.kind() {
                                InputEventKind::Key(key) => {
                                    // Fn key is usually KEY_FN or KEY_F24 on Linux
                                    if key == Key::KEY_FN || key == Key::KEY_F24 {
                                        let value = event.value();
                                        if value == 1 && !fn_pressed {
                                            fn_pressed = true;
                                            let _ = tx_clone.send(KeyEvent::FnPressed).await;
                                        } else if value == 0 && fn_pressed {
                                            fn_pressed = false;
                                            let _ = tx_clone.send(KeyEvent::FnReleased).await;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error fetching events: {}", e);
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        });
        handles.push(handle);
    }

    // Wait for any handle to finish (they shouldn't)
    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

fn find_keyboard_devices() -> Result<Vec<Device>> {
    let mut devices = Vec::new();

    // Look for keyboard devices in /dev/input/
    for entry in std::fs::read_dir("/dev/input")? {
        let entry = entry?;
        let path = entry.path();

        if !path.starts_with("/dev/input/event") {
            continue;
        }

        match Device::open(&path) {
            Ok(device) => {
                // Check if it's a keyboard device
                let name = device.name().unwrap_or("unknown");
                log::debug!("Found device: {} at {:?}", name, path);

                // Check if device has KEY_FN capability
                let supported_keys = device.supported_keys();
                if let Some(keys) = supported_keys {
                    if keys.contains(Key::KEY_FN) {
                        log::info!("Found device with Fn key: {}", name);
                        devices.push(device);
                    }
                }
            }
            Err(e) => {
                log::debug!("Could not open {:?}: {}", path, e);
            }
        }
    }

    Ok(devices)
}
