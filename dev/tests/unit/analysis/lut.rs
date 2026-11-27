use std::path::PathBuf;

use super::*;

fn lut_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LUT")
}

fn gradient_svg(width: u32, height: u32, left: &str, right: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">
  <linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" x2="{width}">
    <stop offset="0" stop-color="{left}"/><stop offset="1" stop-color="{right}"/>
  </linearGradient>
  <rect width="{width}" height="{height}" fill="url(#g)"/>
</svg>"#
    )
}

fn write(dir: &Path, name: &str, data: &str) {
    std::fs::write(dir.join(name), data).expect("write");
}

fn close(a: [u8; 4], b: [u8; 3], tolerance: u8) -> bool {
    (0..3).all(|c| a[c].abs_diff(b[c]) <= tolerance)
}

#[test]
fn svgs_of_any_size_resample_to_the_full_table() {
    for (width, height) in [(2, 1), (100, 40), (256, 1), (1000, 3)] {
        let svg = gradient_svg(width, height, "#0a141e", "#c89664");
        let lut = Lut::from_svg("Grad", svg.as_bytes()).expect("load");
        assert_eq!(lut.name, "Grad");
        assert!(close(lut.colors[0], [10, 20, 30], 2), "{:?}", lut.colors[0]);
        assert!(close(lut.colors[128], [105, 85, 65], 2), "{:?}", lut.colors[128]);
        assert!(close(lut.colors[255], [200, 150, 100], 2), "{:?}", lut.colors[255]);
        assert!(lut.colors.iter().all(|c| c[3] == 255));
    }
}

#[test]
fn invalid_data_is_an_invalid_palette() {
    for data in ["not an svg", r#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="1"/>"#] {
        assert!(matches!(
            Lut::from_svg("Bad", data.as_bytes()),
            Err(AnalysisError::InvalidPalette(_))
        ));
    }
}

#[test]
fn discover_returns_svg_files_sorted_and_tolerates_problems() {
    let dir = tempfile::tempdir().expect("tempdir");
    let svg = gradient_svg(16, 1, "#000000", "#ffffff");
    write(dir.path(), "Zeta.svg", &svg);
    write(dir.path(), "Alpha.SVG", &svg);
    write(dir.path(), "Old.png", &svg);
    write(dir.path(), "Broken.svg", "not an svg");
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
    assert_eq!(names, ["Amber", "Aurora", "Cool", "Heat", "Ice", "Nebula"]);
    for lut in &luts {
        assert_eq!(lut.colors[0], [0, 0, 0, 255], "{}", lut.name);
        assert_eq!(lut.colors[255], [255, 255, 255, 255], "{}", lut.name);
    }
}

#[test]
fn builtins_are_the_repository_heat_and_cool() {
    let names: Vec<&str> = Lut::builtins().iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Heat", "Cool"]);
    let disk = discover_luts(&lut_dir());
    for builtin in Lut::builtins() {
        assert!(disk.contains(builtin), "{}", builtin.name);
    }
    assert_eq!(Lut::builtin(), Lut::builtins()[0]);
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
