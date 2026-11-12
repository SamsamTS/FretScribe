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

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    const PROTAN: [[f64; 3]; 3] = [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]];
    #[allow(dead_code)]
    const DEUTAN: [[f64; 3]; 3] = [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]];
    const NORMAL: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    fn linear(c: u8) -> f64 {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }

    /// CIE Lab of the colour as seen through the simulation matrix (Vienot 1999).
    fn lab(c: Color32, m: &[[f64; 3]; 3]) -> [f64; 3] {
        let l = [linear(c.r()), linear(c.g()), linear(c.b())];
        let s: Vec<f64> = m.iter().map(|row| row.iter().zip(&l).map(|(a, b)| a * b).sum()).collect();
        let (x, y, z) = ((0.4124 * s[0] + 0.3576 * s[1] + 0.1805 * s[2]) / 0.95047, 0.2126 * s[0] + 0.7152 * s[1] + 0.0722 * s[2], (0.0193 * s[0] + 0.1192 * s[1] + 0.9505 * s[2]) / 1.08883);
        let f = |t: f64| if t > 0.008856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
        [116.0 * f(y) - 16.0, 500.0 * (f(x) - f(y)), 200.0 * (f(y) - f(z))]
    }

    #[test]
    fn colours_are_distinct_in_normal_vision_and_follow_the_requested_order() {
        let colors: Vec<Color32> = (0..14).map(|i| string_color(13 - i, 14)).collect();
        for m in [&NORMAL] {
            for (i, a) in colors.iter().enumerate() {
                for b in &colors[i + 1..] {
                    let (la, lb) = (lab(*a, m), lab(*b, m));
                    let d = la.iter().zip(&lb).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt();
                    assert!(d > 12.0, "{a:?} {b:?} {d}");
                }
            }
        }
        assert_eq!(string_color(5, 6), Color32::from_rgb(235, 75, 75), "the highest string is red");
        assert_eq!(string_color(4, 6), Color32::from_rgb(240, 220, 70));
        assert_eq!(string_color(0, 7), string_color(0, 7));
        assert_ne!(string_color(6, 14), string_color(13, 14));
    }

    #[test]
    fn the_same_rank_keeps_its_colour_whatever_the_string_count() {
        assert_eq!(string_color(3, 4), string_color(5, 6));
        assert_ne!(string_color(0, 8), string_color(7, 8));
    }
}
