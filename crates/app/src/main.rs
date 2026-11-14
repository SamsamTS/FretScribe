#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use fretscribe_app::app::App;
use fretscribe_app::settings::{settings_dir, Settings};

/// A plain rounded accent-coloured square; the packaged build can replace it with a real icon.
fn icon() -> egui::IconData {
    let n = 64_u32;
    let accent = fretscribe_ui::theme::DEFAULT_ACCENT;
    let mut rgba = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = ((x.min(n - 1 - x)) as f32, (y.min(n - 1 - y)) as f32);
            let inside = dx.min(dy) >= 4.0 || (dx - 12.0).hypot(dy - 12.0) <= 12.0 || dx >= 12.0 && dy >= 4.0 || dy >= 12.0 && dx >= 4.0;
            rgba.extend_from_slice(&[accent.r(), accent.g(), accent.b(), if inside { 255 } else { 0 }]);
        }
    }
    egui::IconData { rgba, width: n, height: n }
}

fn main() -> eframe::Result {
    env_logger::init();
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let size = settings_dir().map(|d| Settings::load(&d.join("settings.json"))).and_then(|s| s.window_size).unwrap_or((1480.0, 900.0));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([size.0, size.1]).with_min_inner_size([900.0, 600.0]).with_icon(icon()).with_drag_and_drop(true).with_title("FretScribe"),
        ..Default::default()
    };
    eframe::run_native("FretScribe", options, Box::new(move |cc| Ok(Box::new(App::new(cc, open)))))
}
