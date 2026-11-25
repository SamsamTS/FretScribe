//! Every colour, the fonts and the sizes of the application shell. Dark theme only.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};

pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0xF0, 0xAA, 0x3C);
/// The logo's background (#1C2024) is the window colour; the other surfaces are tinted from it.
/// The playhead and seek line.
pub const PLAYHEAD: Color32 = Color32::from_rgb(255, 90, 80);
/// Open strings and region edges.
pub const ACCENT_AMBER: Color32 = Color32::from_rgb(255, 200, 90);
pub const WINDOW_BG: Color32 = Color32::from_rgb(0x1C, 0x20, 0x24);
pub const TITLE_BG: Color32 = Color32::from_rgb(0x16, 0x19, 0x1D);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x26, 0x2B, 0x31);
pub const INPUT_BG: Color32 = Color32::from_rgb(0x17, 0x1A, 0x1E);
pub const BORDER: Color32 = Color32::from_rgb(0x33, 0x3A, 0x43);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x44, 0x4C, 0x58);
/// The blue at the top of the logo icon's gradient; used for subtle glows on `WINDOW_BG`.
pub const LOGO_GLOW: Color32 = Color32::from_rgb(0x28, 0x38, 0x47);
pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xEA, 0xEE);
pub const LABEL: Color32 = Color32::from_rgb(0xD5, 0xDA, 0xE3);
pub const HINT: Color32 = Color32::from_rgb(0x9A, 0xA3, 0xB2);
pub const ERROR: Color32 = Color32::from_rgb(0xE5, 0x5B, 0x5B);
pub const HEADER_BG: Color32 = Color32::from_rgb(0x1B, 0x1E, 0x24);
pub const HEADER_HOVER: Color32 = Color32::from_rgb(0x20, 0x25, 0x2B);
pub const HEADER_SELECTED: Color32 = Color32::from_rgb(0x2A, 0x24, 0x18);
pub const CONFIDENCE_HIGH: Color32 = Color32::from_rgb(0x4F, 0xC3, 0x8A);
pub const CONFIDENCE_MID: Color32 = Color32::from_rgb(0xF0, 0xAA, 0x3C);
pub const CONFIDENCE_LOW: Color32 = Color32::from_rgb(0xE5, 0x60, 0x6E);
pub const CONTROL_RADIUS: u8 = 8;
pub const PANEL_RADIUS: u8 = 14;

pub const LOGO_PNG: &[u8] = include_bytes!("../assets/logo/logo.png");
pub const ICON_PNG: &[u8] = include_bytes!("../assets/logo/icon.png");

pub const INTER: &str = "Inter";
pub const INTER_MEDIUM: &str = "Inter-Medium";
pub const MONO: &str = "JetBrainsMono";

static ACCENT_RGB: AtomicU32 = AtomicU32::new(pack(DEFAULT_ACCENT));

const fn pack(c: Color32) -> u32 {
    (c.r() as u32) << 16 | (c.g() as u32) << 8 | c.b() as u32
}

pub fn set_accent(c: Color32) {
    ACCENT_RGB.store(pack(c), Ordering::Relaxed);
}

pub fn accent() -> Color32 {
    let v = ACCENT_RGB.load(Ordering::Relaxed);
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// Dark text for filled accent buttons, white when the accent itself is dark.
pub fn accent_text_on() -> Color32 {
    let a = accent();
    let luma = 0.299 * f32::from(a.r()) + 0.587 * f32::from(a.g()) + 0.114 * f32::from(a.b());
    if luma > 140.0 { Color32::from_rgb(0x1A, 0x12, 0x04) } else { Color32::WHITE }
}

pub fn accent_hover() -> Color32 {
    accent().gamma_multiply(0.9).blend(Color32::from_white_alpha(30))
}

pub fn focus_ring() -> Stroke {
    Stroke::new(2.0_f32, accent())
}

pub fn medium() -> FontFamily {
    FontFamily::Name(INTER_MEDIUM.into())
}

pub fn mono() -> FontFamily {
    FontFamily::Name(MONO.into())
}

/// egui's bundled fonts, which the application does not use.
fn fonts_default_names() -> [&'static str; 3] {
    ["Hack", "Ubuntu-Light", "NotoEmoji-Regular"]
}

/// Adds Inter, Inter Medium and JetBrains Mono (and leaves egui's defaults as fallbacks).
pub fn add_fonts(fonts: &mut FontDefinitions) {
    let faces: [(&str, &'static [u8]); 3] = [
        (INTER, include_bytes!("../assets/Inter_400Regular.ttf")),
        (INTER_MEDIUM, include_bytes!("../assets/Inter_500Medium.ttf")),
        (MONO, include_bytes!("../assets/JetBrainsMono_400Regular.ttf")),
    ];
    for (name, bytes) in faces {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    // Only Inter and JetBrains Mono: egui's Hack, Ubuntu and Noto fonts are dropped, `FontFamily::Monospace`
    // is JetBrains Mono too. Only egui's small symbol font stays as a fallback for the arrows of its own menus.
    const SYMBOLS: &str = "emoji-icon-font";
    let fallback = fonts.font_data.contains_key(SYMBOLS).then(|| SYMBOLS.to_owned());
    let family = |first: &[&str]| first.iter().map(|f| (*f).to_owned()).chain(fallback.clone()).collect::<Vec<_>>();
    fonts.families.insert(FontFamily::Proportional, family(&[INTER]));
    fonts.families.insert(FontFamily::Monospace, family(&[MONO]));
    fonts.families.insert(medium(), family(&[INTER_MEDIUM, INTER]));
    fonts.families.insert(mono(), family(&[MONO]));
    fonts.font_data.retain(|name, _| !fonts_default_names().contains(&name.as_str()));
}

/// Installs the theme fonts and the notation font together (one `set_fonts` call).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    add_fonts(&mut fonts);
    crate::staff_view::add_fonts(&mut fonts);
    ctx.set_fonts(fonts);
}

/// Frame of dialog windows: the window colour (like the toolbar), not the lighter panel grey.
/// A non-collapsible window in the dialog frame; callers add anchor/resizable as needed.
pub fn dialog<'a>(ctx: &egui::Context, title: impl Into<egui::WidgetText>) -> egui::Window<'a> {
    egui::Window::new(title).frame(dialog_frame(ctx)).collapsible(false)
}

pub fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::window(&ctx.style()).fill(WINDOW_BG)
}

/// Sets `Visuals` and `Style` from this module; call at start and after `set_accent`.
pub fn apply(ctx: &egui::Context) {
    let accent = accent();
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = WINDOW_BG;
    visuals.window_fill = PANEL_BG;
    visuals.extreme_bg_color = INPUT_BG;
    visuals.faint_bg_color = PANEL_BG;
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.window_corner_radius = CornerRadius::same(PANEL_RADIUS);
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = accent.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0_f32, accent);
    let radius = CornerRadius::same(CONTROL_RADIUS);
    let widgets = &mut visuals.widgets;
    for (w, fill, edge) in [
        (&mut widgets.noninteractive, WINDOW_BG, BORDER),
        (&mut widgets.inactive, PANEL_BG, BORDER),
        (&mut widgets.hovered, BORDER, BORDER_STRONG),
        (&mut widgets.active, BORDER_STRONG, accent),
        (&mut widgets.open, BORDER, BORDER_STRONG),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0_f32, edge);
        w.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.corner_radius = radius;
    }
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        let sizes = [
            (TextStyle::Body, 14.0, FontFamily::Proportional),
            (TextStyle::Button, 14.0, medium()),
            (TextStyle::Heading, 22.0, medium()),
            (TextStyle::Small, 12.0, FontFamily::Proportional),
            (TextStyle::Monospace, 12.5, mono()),
        ];
        style.interaction.selectable_labels = false;
        for (ts, size, family) in sizes {
            style.text_styles.insert(ts, FontId::new(size, family));
        }
    });
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/ui/theme.rs"]
mod tests;

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/ui/theme_font_tests.rs"]
mod font_tests;
