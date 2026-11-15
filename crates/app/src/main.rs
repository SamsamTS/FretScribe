#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use fretscribe_app::app::App;
use fretscribe_app::settings::{settings_dir, Settings, WindowMode};

fn icon() -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(fretscribe_ui::theme::ICON_PNG, image::ImageFormat::Png).ok()?.into_rgba8();
    Some(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() })
}

fn main() -> eframe::Result {
    env_logger::init();
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let settings = settings_dir().map(|d| Settings::load(&d.join("settings.json"))).unwrap_or_default();
    let size = settings.window_size.unwrap_or((1480.0, 900.0));
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([size.0, size.1]).with_min_inner_size([900.0, 600.0]).with_drag_and_drop(true).with_title("FretScribe");
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(icon);
    }
    viewport = match settings.window_mode {
        WindowMode::Normal => viewport,
        WindowMode::Maximized => viewport.with_maximized(true),
        WindowMode::Fullscreen => viewport.with_fullscreen(true),
    };
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("FretScribe", options, Box::new(move |cc| Ok(Box::new(App::new(cc, open)))))
}
