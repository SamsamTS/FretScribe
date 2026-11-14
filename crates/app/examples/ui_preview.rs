//! Home screen and new project form with fake data. `ui_preview [--form] [--screenshot out.png]`.

use eframe::egui;
use fretscribe_app::form_ui::FormState;
use fretscribe_app::home::{show_home, TileStatus};
use fretscribe_app::settings::{RecentProject, Settings};
use fretscribe_ui::theme;

const NOW: u64 = 1_794_000_000;

struct Preview {
    settings: Settings,
    form: Option<FormState>,
    shot: Option<std::path::PathBuf>,
    frames: u32,
}

impl eframe::App for Preview {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| match &mut self.form {
            Some(form) => drop(form.show(ui)),
            None => drop(show_home(ui, &self.settings, &|p| if p.ends_with("c") { TileStatus::MissingAudio } else { TileStatus::Ok }, NOW)),
        });
        self.frames += 1;
        let Some(path) = self.shot.clone() else { return };
        if self.frames == 5 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ctx.request_repaint();
        let image = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }));
        if let Some(img) = image {
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let _ = image::save_buffer(path, &rgba, img.width() as u32, img.height() as u32, image::ColorType::Rgba8);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let form = args.iter().any(|a| a == "--form").then(|| {
        let mut f = FormState::new("D:\\Music\\Beatles\\Blackbird (guitar stem).flac".into(), "Blackbird - The White Album by The Beatles".into(), true);
        f.add_track("D:\\Music\\Beatles\\Blackbird (full mix).mp3".into());
        f.set_bpm_candidates(vec![(118.0, 0.9), (59.0, 0.5), (236.0, 0.3)]);
        f.set_reference_estimate(4.0, 441.0, 0.8);
        f
    });
    let thumb = |n: usize| (0..48).map(|i| ((i * 37 + n * 11) % 200 + 30) as u8).collect::<Vec<_>>();
    let names = ["Blackbird", "A very long project name that cannot possibly fit", "Dust in the Wind", "Hotel California", "Tears in Heaven", "Wonderwall"];
    let recent = names.iter().enumerate().map(|(i, n)| RecentProject {
        path: format!("/p/{}", (b'a' + i as u8) as char).into(),
        name: (*n).to_owned(),
        last_opened_unix: NOW - [3600, 86_400, 400_000, 9_000_000, 30_000_000, 0][i],
        thumbnail: (i != 4).then(|| thumb(i)),
    });
    let settings = Settings { recent: recent.collect(), ..Settings::default() };
    let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(Into::into);
    eframe::run_native(
        "FretScribe preview",
        eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 760.0]), ..Default::default() },
        Box::new(|cc| {
            theme::install_fonts(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(Preview { settings, form, shot, frames: 0 }))
        }),
    )
}
