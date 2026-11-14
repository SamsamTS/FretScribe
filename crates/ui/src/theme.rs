//! Every colour, the fonts and the sizes of the application shell. Dark theme only.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};

pub const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0xF0, 0xAA, 0x3C);
pub const WINDOW_BG: Color32 = Color32::from_rgb(0x0E, 0x10, 0x13);
pub const TITLE_BG: Color32 = Color32::from_rgb(0x14, 0x16, 0x1B);
pub const PANEL_BG: Color32 = Color32::from_rgb(0x1B, 0x1E, 0x24);
pub const INPUT_BG: Color32 = Color32::from_rgb(0x12, 0x14, 0x1A);
pub const BORDER: Color32 = Color32::from_rgb(0x2C, 0x31, 0x3A);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x3A, 0x41, 0x4D);
pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xEA, 0xEE);
pub const LABEL: Color32 = Color32::from_rgb(0xD5, 0xDA, 0xE3);
pub const HINT: Color32 = Color32::from_rgb(0x9A, 0xA3, 0xB2);
pub const ERROR: Color32 = Color32::from_rgb(0xE5, 0x5B, 0x5B);
pub const CONTROL_RADIUS: u8 = 8;
pub const PANEL_RADIUS: u8 = 14;

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
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, INTER.to_owned());
    fonts.families.insert(medium(), vec![INTER_MEDIUM.to_owned(), INTER.to_owned()]);
    fonts.families.insert(mono(), vec![MONO.to_owned()]);
}

/// Installs the theme fonts and the notation font together (one `set_fonts` call).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    add_fonts(&mut fonts);
    crate::staff_view::add_fonts(&mut fonts);
    ctx.set_fonts(fonts);
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
        for (ts, size, family) in sizes {
            style.text_styles.insert(ts, FontId::new(size, family));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_round_trips_and_text_contrasts() {
        set_accent(Color32::from_rgb(10, 20, 30));
        assert_eq!(accent(), Color32::from_rgb(10, 20, 30));
        assert_eq!(accent_text_on(), Color32::WHITE);
        set_accent(DEFAULT_ACCENT);
        assert_ne!(accent_text_on(), Color32::WHITE);
    }

    #[test]
    fn fonts_register_every_family() {
        let mut fonts = FontDefinitions::default();
        add_fonts(&mut fonts);
        for f in [medium(), mono()] {
            assert!(fonts.families.contains_key(&f));
        }
        assert_eq!(fonts.families[&FontFamily::Proportional][0], INTER);
    }
}
