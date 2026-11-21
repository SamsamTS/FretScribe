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

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/grid/draw.rs"]
mod tests;
