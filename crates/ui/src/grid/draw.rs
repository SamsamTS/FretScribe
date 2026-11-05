//! Pixel-exact line rendering: every line is a rectangle whose edges sit on physical pixels, so it
//! stays crisp at any zoom and display scale.

use egui::{pos2, Color32, Mesh, Rect, Shape};

/// A vertical line `width_px` physical pixels wide, centred as close to `x` as the pixel grid
/// allows.
pub fn line_rect(x: f32, y_min: f32, y_max: f32, pixels_per_point: f32, width_px: f32) -> Rect {
    let left = (x * pixels_per_point - width_px / 2.0).round();
    Rect::from_min_max(
        pos2(left / pixels_per_point, y_min),
        pos2((left + width_px) / pixels_per_point, y_max),
    )
}

/// Sizes of a dotted line, in physical pixels.
#[derive(Clone, Copy, Debug)]
pub struct Dots {
    pub width_px: f32,
    pub dot_px: f32,
    pub gap_px: f32,
}

pub struct LineBatch {
    mesh: Mesh,
    pixels_per_point: f32,
}

impl LineBatch {
    pub fn new(pixels_per_point: f32) -> Self {
        Self {
            mesh: Mesh::default(),
            pixels_per_point: pixels_per_point.max(f32::EPSILON),
        }
    }

    pub fn solid(&mut self, x: f32, y_min: f32, y_max: f32, width_px: f32, color: Color32) {
        let rect = line_rect(x, y_min, y_max, self.pixels_per_point, width_px);
        self.mesh.add_colored_rect(rect, color);
    }

    pub fn dotted(&mut self, x: f32, y_min: f32, y_max: f32, dots: Dots, color: Color32) {
        let Dots { width_px, dot_px, gap_px } = dots;
        let ppp = self.pixels_per_point;
        let column = line_rect(x, y_min, y_max, ppp, width_px);
        let bottom = (y_max * ppp).round();
        let step = (dot_px + gap_px).max(1.0);
        let mut y = (y_min * ppp).round();
        while y < bottom {
            let end = (y + dot_px).min(bottom);
            let dot = Rect::from_min_max(pos2(column.min.x, y / ppp), pos2(column.max.x, end / ppp));
            self.mesh.add_colored_rect(dot, color);
            y += step;
        }
    }

    pub fn into_shape(self) -> Shape {
        Shape::mesh(self.mesh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOTS: Dots = Dots { width_px: 1.0, dot_px: 2.0, gap_px: 3.0 };

    #[test]
    fn lines_sit_on_whole_pixels_with_the_requested_width() {
        for ppp in [1.0_f32, 1.25, 1.5, 2.0] {
            for width in [1.0_f32, 2.0, 3.0] {
                let mut x = 10.0_f32;
                while x < 12.0 {
                    let r = line_rect(x, 0.0, 10.0, ppp, width);
                    let (left, right) = (r.min.x * ppp, r.max.x * ppp);
                    assert!((left - left.round()).abs() < 1e-3, "ppp {ppp} x {x}");
                    assert!((right - left - width).abs() < 1e-3, "ppp {ppp} w {width}");
                    assert!((r.center().x - x).abs() <= 0.5 / ppp + 1e-3);
                    x += 0.037;
                }
            }
        }
    }

    #[test]
    fn one_pixel_line_covers_the_pixel_under_the_position() {
        let r = line_rect(10.3, 0.0, 5.0, 1.0, 1.0);
        assert_eq!((r.min.x, r.max.x), (10.0, 11.0));
        let r = line_rect(10.7, 0.0, 5.0, 1.0, 1.0);
        assert_eq!((r.min.x, r.max.x), (10.0, 11.0));
        let r = line_rect(10.6, 0.0, 5.0, 1.0, 2.0);
        assert_eq!((r.min.x, r.max.x), (10.0, 12.0));
    }

    #[test]
    fn dotted_lines_alternate_dots_and_gaps() {
        let mut batch = LineBatch::new(1.0);
        batch.dotted(5.0, 0.0, 20.0, DOTS, Color32::WHITE);
        let Shape::Mesh(mesh) = batch.into_shape() else { panic!("expected a mesh") };
        assert_eq!(mesh.vertices.len(), 4 * 4, "dots at y = 0, 5, 10, 15");
        let tops: Vec<f32> = mesh.vertices.chunks(4).map(|q| q[0].pos.y).collect();
        assert_eq!(tops, [0.0, 5.0, 10.0, 15.0]);
    }

    #[test]
    fn last_dot_is_cut_at_the_bottom() {
        let mut batch = LineBatch::new(1.0);
        batch.dotted(5.0, 0.0, 6.0, DOTS, Color32::WHITE);
        let Shape::Mesh(mesh) = batch.into_shape() else { panic!("expected a mesh") };
        let bottoms: Vec<f32> = mesh.vertices.chunks(4).map(|q| q[2].pos.y).collect();
        assert_eq!(bottoms, [2.0, 6.0]);
    }
}
