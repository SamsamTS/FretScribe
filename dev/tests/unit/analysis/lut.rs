use std::path::PathBuf;

use image::{Rgba, RgbaImage};

use super::*;

fn lut_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LUT")
}

fn write_gradient(path: &Path, width: u32, left: [u8; 3], right: [u8; 3]) {
    let image = RgbaImage::from_fn(width, 3, |x, _| {
        let w = x as f32 / (width - 1).max(1) as f32;
        let mut px = [0u8, 0, 0, 77];
        for c in 0..3 {
            px[c] = (f32::from(left[c]) * (1.0 - w) + f32::from(right[c]) * w).round() as u8;
        }
        Rgba(px)
    });
    image.save(path).expect("write png");
}

#[test]
fn from_png_resamples_to_256_and_forces_alpha() {
    let dir = tempfile::tempdir().expect("tempdir");
    for width in [2, 100, 256, 1000] {
        let path = dir.path().join(format!("Grad{width}.png"));
        write_gradient(&path, width, [10, 20, 30], [200, 150, 100]);
        let lut = Lut::from_png(&path).expect("load");
        assert_eq!(lut.name, format!("Grad{width}"));
        assert_eq!(lut.colors.len(), 256);
        assert_eq!(lut.colors[0], [10, 20, 30, 255]);
        assert_eq!(lut.colors[255], [200, 150, 100, 255]);
        assert!(lut.colors.iter().all(|c| c[3] == 255));
        let mid = lut.colors[128];
        assert!((i32::from(mid[0]) - 105).abs() <= 2, "{mid:?}");
    }
}

#[test]
fn discover_returns_png_files_sorted_and_tolerates_problems() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_gradient(&dir.path().join("Zeta.png"), 16, [0, 0, 0], [255, 255, 255]);
    write_gradient(&dir.path().join("Alpha.png"), 16, [0, 0, 0], [255, 0, 0]);
    std::fs::write(dir.path().join("notes.txt"), "not a palette").expect("write");
    std::fs::write(dir.path().join("Broken.png"), "not a png").expect("write");
    let names: Vec<String> = discover_luts(dir.path())
        .into_iter()
        .map(|l| l.name)
        .collect();
    assert_eq!(names, ["Alpha", "Zeta"]);
    assert!(discover_luts(&dir.path().join("missing")).is_empty());
}

#[test]
fn repository_palettes_load() {
    let luts = discover_luts(&lut_dir());
    let names: Vec<&str> = luts.iter().map(|l| l.name.as_str()).collect();
    assert!(
        names.contains(&"Classic") && names.contains(&"Heat"),
        "{names:?}"
    );
    for lut in &luts {
        assert_eq!(lut.colors[0], [0, 0, 0, 255], "{}", lut.name);
    }
}

#[test]
fn color_clamps_and_hits_both_ends() {
    let lut = Lut::builtin();
    assert_eq!(lut.color(0.0), lut.colors[0]);
    assert_eq!(lut.color(1.0), lut.colors[255]);
    assert_eq!(lut.color(-3.0), lut.colors[0]);
    assert_eq!(lut.color(7.0), lut.colors[255]);
    assert_eq!(lut.color(f32::NAN), lut.colors[0]);
    assert_eq!(lut.color(0.5), lut.colors[128]);
}

#[test]
fn builtin_matches_classic_png() {
    let png = Lut::from_png(&lut_dir().join("Classic.png")).expect("Classic.png");
    assert_eq!(png, Lut::builtin());
}
