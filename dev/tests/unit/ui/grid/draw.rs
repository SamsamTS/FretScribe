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
