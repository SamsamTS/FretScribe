//! String colours: the one place that maps a string index to a colour (low string first).
//! Okabe-Ito based, lightened for the dark background, neighbours alternate warm and cool; the same
//! index always gets the same colour, and indices past the palette repeat it brighter.

use egui::Color32;

const PALETTE: [[u8; 3]; 8] = [
    [240, 110, 70],
    [86, 180, 233],
    [240, 228, 66],
    [204, 121, 167],
    [0, 190, 140],
    [230, 159, 0],
    [130, 150, 255],
    [200, 200, 200],
];

pub fn string_color(index: usize, _count: usize) -> Color32 {
    let [r, g, b] = PALETTE[index % PALETTE.len()];
    if index < PALETTE.len() {
        return Color32::from_rgb(r, g, b);
    }
    let lift = |c: u8| c.saturating_add(((255 - u16::from(c)) / 3) as u8);
    Color32::from_rgb(lift(r), lift(g), lift(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROTAN: [[f64; 3]; 3] = [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]];
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
    fn colours_stay_distinct_for_colour_blind_viewers() {
        let colors: Vec<Color32> = (0..8).map(|i| string_color(i, 8)).collect();
        for m in [&NORMAL, &PROTAN, &DEUTAN] {
            for (i, a) in colors.iter().enumerate() {
                for b in &colors[i + 1..] {
                    let (la, lb) = (lab(*a, m), lab(*b, m));
                    let d = la.iter().zip(&lb).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt();
                    assert!(d > 15.0, "{a:?} {b:?} {d}");
                }
            }
        }
    }

    #[test]
    fn indices_are_stable_and_extend_past_the_palette() {
        assert_eq!(string_color(2, 6), string_color(2, 8));
        assert_ne!(string_color(8, 9), string_color(0, 9));
        assert_ne!(string_color(8, 9), string_color(1, 9));
    }
}
