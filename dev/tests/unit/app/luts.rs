use super::*;

fn write_lut(dir: &Path, name: &str, rgb: [u8; 3]) {
    let [r, g, b] = rgb;
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="rgb({r},{g},{b})"/></svg>"#);
    std::fs::write(dir.join(name), svg).unwrap();
}

#[test]
fn lut_dir_prefers_exe_then_workspace() {
    let (exe, ws) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    assert_eq!(lut_dir(exe.path(), ws.path()), None);
    std::fs::create_dir(ws.path().join("LUT")).unwrap();
    assert_eq!(lut_dir(exe.path(), ws.path()), Some(ws.path().join("LUT")));
    std::fs::create_dir(exe.path().join("LUT")).unwrap();
    assert_eq!(lut_dir(exe.path(), ws.path()), Some(exe.path().join("LUT")));
}

#[test]
fn scan_falls_back_to_builtin() {
    for dir in [None, Some(tempfile::tempdir().unwrap())] {
        let set = LutSet::scan(dir.as_ref().map(|d| d.path()));
        assert!(set.builtin_fallback);
        assert_eq!(set.names(), ["Heat", "Cool"]);
    }
    assert!(LutSet::scan(Some(Path::new("/definitely/missing"))).builtin_fallback);
}

#[test]
fn scan_lists_builtins_first_and_indexes_by_name() {
    let dir = tempfile::tempdir().unwrap();
    write_lut(dir.path(), "Ice.svg", [0, 0, 200]);
    write_lut(dir.path(), "Amber.svg", [200, 100, 0]);
    let set = LutSet::scan(Some(dir.path()));
    assert!(!set.builtin_fallback);
    assert_eq!(set.names(), ["Heat", "Cool", "Amber", "Ice"]);
    assert_eq!(set.index_of("Ice"), 3);
    assert_eq!(set.index_of("Classic"), 0);
}

#[test]
fn unchanged_copies_of_builtins_add_no_entry() {
    let dir = tempfile::tempdir().unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LUT");
    let heat = std::fs::read_to_string(repo.join("Heat.svg")).unwrap();
    std::fs::write(dir.path().join("heat.svg"), heat + "<!-- same colours -->").unwrap();
    std::fs::copy(repo.join("Cool.svg"), dir.path().join("Cool.svg")).unwrap();
    let set = LutSet::scan(Some(dir.path()));
    assert!(!set.builtin_fallback);
    assert_eq!(set.names(), ["Heat", "Cool"]);
}

#[test]
fn edited_builtins_are_listed_as_custom() {
    let dir = tempfile::tempdir().unwrap();
    write_lut(dir.path(), "HEAT.svg", [1, 2, 3]);
    write_lut(dir.path(), "cool.svg", [4, 5, 6]);
    write_lut(dir.path(), "Dusk.svg", [7, 8, 9]);
    write_lut(dir.path(), "Ice.svg", [9, 9, 9]);
    let set = LutSet::scan(Some(dir.path()));
    assert_eq!(set.names(), ["Heat", "Cool", "Cool (Custom)", "Dusk", "Heat (Custom)", "Ice"]);
    assert_eq!(*set.luts[0], Lut::builtins()[0]);
    assert_eq!(set.luts[4].colors[0], [1, 2, 3, 255]);
}
