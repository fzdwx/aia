use gtk4::prelude::*;
use gtk4::{glib, Application, ApplicationWindow, DrawingArea, Label};
use gtk4::gdk;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::config::AppConfig;

const WAVE_BAR_WEIGHTS: [f32; 5] = [0.5, 0.8, 1.0, 0.75, 0.55];
const WAVE_BAR_WIDTH: i32 = 44;
const WAVE_BAR_HEIGHT: i32 = 32;

#[derive(Clone)]
pub struct FloatingWindow {
    window: ApplicationWindow,
    drawing_area: DrawingArea,
    label: Label,
    rms_values: Arc<parking_lot::RwLock<[f32; 5]>>,
    smoothed_rms: Arc<parking_lot::RwLock<f32>>,
    visible: Arc<AtomicBool>,
}

impl FloatingWindow {
    pub fn new(app: &Application, _config: Arc<parking_lot::RwLock<AppConfig>>) -> Self {
        // Create window
        let window = ApplicationWindow::builder()
            .application(app)
            .decorated(false)
            .resizable(false)
            .default_width(200)
            .default_height(60)
            .css_classes(vec!["floating-window".to_string()])
            .build();

        // Load CSS
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(include_str!("style.css"));
        gtk4::style_context_add_provider_for_display(
            &gdk::Display::default().unwrap(),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // Create container box
        let container = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(12)
            .margin_start(16)
            .margin_end(16)
            .margin_top(12)
            .margin_bottom(12)
            .css_classes(vec!["capsule".to_string()])
            .build();

        // Create waveform widget
        let rms_values = Arc::new(parking_lot::RwLock::new([0.0; 5]));
        let smoothed_rms = Arc::new(parking_lot::RwLock::new(0.0));
        let visible = Arc::new(AtomicBool::new(false));
        
        let drawing_area = DrawingArea::builder()
            .width_request(WAVE_BAR_WIDTH)
            .height_request(WAVE_BAR_HEIGHT)
            .build();

        let rms_values_clone = rms_values.clone();
        drawing_area.set_draw_func(move |_, cr, _width, height| {
            let values = rms_values_clone.read();

            // Clear
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
            let _ = cr.paint();

            // Draw waveform bars
            let bar_width = 4.0;
            let bar_gap = 4.0;
            let max_height = height as f64 - 4.0;

            for (i, &value) in values.iter().enumerate() {
                let x = 4.0 + i as f64 * (bar_width + bar_gap);
                let bar_height = (value as f64 * max_height * 3.0).max(4.0).min(max_height);
                let y = (height as f64 - bar_height) / 2.0;

                // Draw rounded rectangle
                let radius = 2.0;
                cr.new_path();
                let _ = cr.arc(x + radius, y + radius, radius, std::f64::consts::PI, -std::f64::consts::FRAC_PI_2);
                cr.line_to(x + bar_width - radius, y);
                let _ = cr.arc(x + bar_width - radius, y + radius, radius, -std::f64::consts::FRAC_PI_2, 0.0);
                cr.line_to(x + bar_width, y + bar_height - radius);
                let _ = cr.arc(x + bar_width - radius, y + bar_height - radius, radius, 0.0, std::f64::consts::FRAC_PI_2);
                cr.line_to(x + radius, y + bar_height);
                let _ = cr.arc(x + radius, y + bar_height - radius, radius, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
                cr.close_path();
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.9);
                let _ = cr.fill();
            }
        });

        // Waveform container with fixed size
        let waveform_container = gtk4::Box::builder()
            .width_request(WAVE_BAR_WIDTH)
            .height_request(WAVE_BAR_HEIGHT)
            .build();
        waveform_container.append(&drawing_area);

        // Create label
        let label = Label::builder()
            .label("Listening...")
            .width_chars(10)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .css_classes(vec!["transcript-label".to_string()])
            .build();

        container.append(&waveform_container);
        container.append(&label);

        window.set_child(Some(&container));

        Self {
            window,
            drawing_area,
            label,
            rms_values,
            smoothed_rms,
            visible,
        }
    }

    fn position_window(&self) {
        let display = gdk::Display::default().unwrap();
        let surface = self.window.surface();
        
        if let Some(surface) = surface {
            if let Some(monitor) = display.monitor_at_surface(&surface) {
                let geometry = monitor.geometry();
                let width = 400;
                let height = 60;

                let x = geometry.x() + (geometry.width() - width) / 2;
                let y = geometry.y() + geometry.height() - height - 100;

                self.window.set_default_width(width);
                self.window.set_default_height(height);
                self.window.present();
            }
        }
    }

    pub fn show(&self) {
        self.visible.store(true, Ordering::SeqCst);

        // Reset waveform
        *self.rms_values.write() = [0.0; 5];
        *self.smoothed_rms.write() = 0.0;

        // Reset label
        self.label.set_label("Listening...");

        // Position window
        self.position_window();

        // Show with animation
        self.window.set_opacity(0.0);
        self.window.present();
    }

    pub fn show_with_text(&self, text: &str) {
        self.visible.store(true, Ordering::SeqCst);
        self.label.set_label(text);
        self.position_window();
        self.window.present();
    }

    pub fn hide_with_animation(&self) {
        self.visible.store(false, Ordering::SeqCst);

        let win = self.window.clone();
        glib::idle_add_local(move || {
            let opacity = win.opacity();
            if opacity > 0.0 {
                win.set_opacity((opacity - 0.15).max(0.0));
                glib::ControlFlow::Continue
            } else {
                win.hide();
                glib::ControlFlow::Break
            }
        });
    }

    pub fn update_rms(&self, rms: f32) {
        if !self.visible.load(Ordering::SeqCst) {
            return;
        }

        // Smooth the RMS value
        let mut smoothed = self.smoothed_rms.write();
        *smoothed = *smoothed * 0.85 + rms * 0.15;
        let smooth_rms = *smoothed;

        // Calculate individual bar heights with weights and jitter
        let mut values = self.rms_values.write();
        use rand::Rng;
        let mut rng = rand::thread_rng();

        for (i, value) in values.iter_mut().enumerate() {
            let weight = WAVE_BAR_WEIGHTS[i];
            let jitter: f32 = rng.gen_range(-0.04..0.04);
            let target = smooth_rms * weight * (1.0 + jitter);

            // Smooth transition (attack/release)
            if target > *value {
                // Attack (40%)
                *value = *value + (target - *value) * 0.4;
            } else {
                // Release (15%)
                *value = *value + (target - *value) * 0.15;
            }
        }

        // Trigger redraw
        self.drawing_area.queue_draw();
    }
}
