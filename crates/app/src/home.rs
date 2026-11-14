//! Home screen: a wrapping grid of project tiles (New Project first, then the recents).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::epaint::StrokeKind;
use egui::text::LayoutJob;
use egui::{Align2, Color32, CornerRadius, CursorIcon, FontId, Galley, Pos2, Rect, ScrollArea, Sense, Stroke, TextFormat, Ui, Vec2};
use fretscribe_ui::theme;

use crate::settings::{RecentProject, Settings};

const TILE: f32 = 176.0;
const GAP: f32 = 16.0;
const PAD: f32 = 12.0;
const THUMB_H: f32 = 84.0;
const SECS_PER_DAY: u64 = 86_400;
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HomeAction {
    NewProject,
    Open(PathBuf),
    RemoveRecent(PathBuf),
    RevealInFolder(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileStatus {
    Ok,
    MissingProject,
    MissingAudio,
}

impl TileStatus {
    fn warning(self) -> Option<&'static str> {
        match self {
            Self::Ok => None,
            Self::MissingProject => Some("Project file not found"),
            Self::MissingAudio => Some("Audio file not found"),
        }
    }
}

/// Days since 1970-01-01 to (year, month 1..=12, day 1..=31), proleptic Gregorian.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

/// UTC calendar day: "Today", "Yesterday" or "12 Nov 2026"; empty for an unknown time.
fn date_label(unix: u64, now: u64) -> String {
    let (day, today) = (unix / SECS_PER_DAY, now / SECS_PER_DAY);
    match today.checked_sub(day) {
        _ if unix == 0 => String::new(),
        Some(0) => "Today".to_owned(),
        Some(1) => "Yesterday".to_owned(),
        _ => {
            let (y, m, d) = civil_from_days(day as i64);
            format!("{d} {} {y}", MONTHS[m as usize - 1])
        }
    }
}

fn columns(available: f32) -> usize {
    (((available + GAP) / (TILE + GAP)).floor() as usize).max(1)
}

/// Mirrored waveform bars as (x offset, half height); one per byte, at least one pixel tall.
fn thumb_bars(bytes: &[u8], width: f32, height: f32) -> Vec<(f32, f32)> {
    let step = width / bytes.len().max(1) as f32;
    bytes.iter().enumerate().map(|(i, &b)| ((i as f32 + 0.5) * step, (f32::from(b) / 255.0 * height / 2.0).max(1.0))).collect()
}

fn fit_text(ui: &Ui, text: &str, font: FontId, color: Color32, width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping { max_width: width, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    job.sections[0].format = TextFormat { color, ..job.sections[0].format.clone() };
    ui.fonts(|f| f.layout_job(job))
}

fn draw_tile(ui: &mut Ui, resp: &egui::Response) -> Rect {
    let rect = resp.rect;
    let radius = CornerRadius::same(theme::PANEL_RADIUS);
    let edge = if resp.hovered() { theme::accent() } else { theme::BORDER };
    ui.painter().rect(rect, radius, theme::PANEL_BG, Stroke::new(1.0_f32, edge), StrokeKind::Inside);
    rect
}

fn new_tile(ui: &mut Ui) -> egui::Response {
    let resp = ui.allocate_exact_size(Vec2::splat(TILE), Sense::click()).1.on_hover_cursor(CursorIcon::PointingHand);
    let rect = draw_tile(ui, &resp);
    let centre = rect.center() - Vec2::new(0.0, 10.0);
    let stroke = Stroke::new(3.0_f32, theme::accent());
    ui.painter().hline(centre.x - 18.0..=centre.x + 18.0, centre.y, stroke);
    ui.painter().vline(centre.x, centre.y - 18.0..=centre.y + 18.0, stroke);
    let font = FontId::new(14.0, theme::medium());
    ui.painter().text(Pos2::new(rect.center().x, rect.bottom() - 36.0), Align2::CENTER_CENTER, "New Project", font, theme::TEXT);
    resp
}

fn recent_tile(ui: &mut Ui, recent: &RecentProject, status: TileStatus, now: u64) -> egui::Response {
    let mut resp = ui.allocate_exact_size(Vec2::splat(TILE), Sense::click()).1.on_hover_cursor(CursorIcon::PointingHand);
    let rect = draw_tile(ui, &resp);
    let inner = rect.shrink(PAD);
    let thumb = Rect::from_min_size(inner.min, Vec2::new(inner.width(), THUMB_H));
    let painter = ui.painter();
    painter.rect_filled(thumb, CornerRadius::same(theme::CONTROL_RADIUS), theme::INPUT_BG);
    let mid = thumb.center().y;
    match recent.thumbnail.as_deref().filter(|t| !t.is_empty()) {
        Some(bytes) => {
            let inset = thumb.shrink2(Vec2::new(8.0, 6.0));
            let bar = Stroke::new((inset.width() / bytes.len() as f32 * 0.6).clamp(1.0, 4.0), theme::accent().gamma_multiply(0.85));
            for (x, half) in thumb_bars(bytes, inset.width(), inset.height()) {
                painter.vline(inset.left() + x, mid - half..=mid + half, bar);
            }
        }
        None => {
            painter.hline(thumb.left() + 8.0..=thumb.right() - 8.0, mid, Stroke::new(1.0_f32, theme::BORDER_STRONG));
        }
    }
    let name_y = thumb.bottom() + 10.0;
    let name = fit_text(ui, &recent.name, FontId::new(14.0, theme::medium()), theme::TEXT, inner.width());
    let date = fit_text(ui, &date_label(recent.last_opened_unix, now), FontId::new(12.0, FontId::default().family), theme::HINT, inner.width());
    let date_y = name_y + name.size().y + 2.0;
    ui.painter().galley(Pos2::new(inner.left(), name_y), name, theme::TEXT);
    ui.painter().galley(Pos2::new(inner.left(), date_y), date, theme::HINT);
    if let Some(warning) = status.warning() {
        let centre = thumb.right_top() + Vec2::new(-16.0, 16.0);
        let painter = ui.painter();
        painter.circle_filled(centre, 10.0, theme::ERROR);
        painter.text(centre, Align2::CENTER_CENTER, "!", FontId::new(13.0, theme::medium()), Color32::WHITE);
        resp = resp.on_hover_text(warning);
    }
    resp
}

pub fn show_home(ui: &mut Ui, settings: &Settings, status: &dyn Fn(&Path) -> TileStatus, now_unix: u64) -> Option<HomeAction> {
    let mut action = None;
    let mut recents: Vec<&RecentProject> = settings.recent.iter().collect();
    recents.sort_by_key(|r| std::cmp::Reverse(r.last_opened_unix));
    ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        let cols = columns(ui.available_width());
        let left = ((ui.available_width() - (cols as f32 * (TILE + GAP) - GAP)) / 2.0).max(0.0);
        ui.spacing_mut().item_spacing = Vec2::splat(GAP);
        ui.add_space(GAP);
        let total = recents.len() + 1;
        for row in (0..total).step_by(cols) {
            ui.horizontal(|ui| {
                ui.add_space(left);
                for i in row..(row + cols).min(total) {
                    let Some(recent) = i.checked_sub(1).map(|r| recents[r]) else {
                        if new_tile(ui).clicked() {
                            action = Some(HomeAction::NewProject);
                        }
                        continue;
                    };
                    let resp = recent_tile(ui, recent, status(&recent.path), now_unix);
                    if resp.clicked() {
                        action = Some(HomeAction::Open(recent.path.clone()));
                    }
                    resp.context_menu(|ui| {
                        let items = [("Remove from list", HomeAction::RemoveRecent(recent.path.clone())), ("Reveal in folder", HomeAction::RevealInFolder(recent.path.clone()))];
                        for (label, act) in items {
                            if ui.button(label).clicked() {
                                action = Some(act);
                                ui.close_menu();
                            }
                        }
                    });
                }
            });
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(20_453), (2025, 12, 31));
        assert_eq!(civil_from_days(20_454), (2026, 1, 1));
        assert_eq!(civil_from_days(-719_468), (0, 3, 1));
    }

    #[test]
    fn labels() {
        let now = 20_763 * SECS_PER_DAY + 72_000; // 2026-11-06 20:00 UTC
        assert_eq!(date_label(now - 3600, now), "Today");
        assert_eq!(date_label(now - SECS_PER_DAY, now), "Yesterday");
        assert_eq!(date_label(now - 5 * SECS_PER_DAY, now), "1 Nov 2026");
        assert_eq!(date_label(0, now), "");
        assert_eq!(date_label(now + 5 * SECS_PER_DAY, now), "11 Nov 2026");
    }

    #[test]
    fn grid_columns() {
        assert_eq!(columns(10.0), 1);
        assert_eq!(columns(TILE), 1);
        assert_eq!(columns(2.0 * TILE + GAP), 2);
        assert_eq!(columns(2.0 * TILE + GAP - 1.0), 1);
        assert_eq!(columns(900.0), 4);
    }

    #[test]
    fn bars_are_centred_and_non_empty() {
        let bars = thumb_bars(&[0, 255], 100.0, 40.0);
        assert_eq!(bars, [(25.0, 1.0), (75.0, 20.0)]);
        assert!(thumb_bars(&[], 100.0, 40.0).is_empty());
    }
}
