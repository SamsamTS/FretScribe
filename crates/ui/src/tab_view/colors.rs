//! String colours: the one place that maps a string index to a colour (index 0 = lowest string).
//! From the highest string: red, yellow, blue, orange, green, purple, pink, then the same seven
//! darker (at most 14 strings; more strings repeat the darker shade).

use egui::Color32;

const PALETTE: [[u8; 3]; 7] = [[235, 75, 75], [240, 220, 70], [90, 150, 245], [245, 150, 50], [80, 200, 110], [170, 110, 230], [240, 130, 190]];
const DARKER: f32 = 0.62;

pub fn string_color(index: usize, count: usize) -> Color32 {
    let rank = count.saturating_sub(1).saturating_sub(index);
    let [r, g, b] = PALETTE[rank % PALETTE.len()];
    let k = if rank >= PALETTE.len() { DARKER } else { 1.0 };
    Color32::from_rgb((f32::from(r) * k) as u8, (f32::from(g) * k) as u8, (f32::from(b) * k) as u8)
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/tab_view/colors.rs"]
mod tests;
