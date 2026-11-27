use super::*;

const BUILTINS: [&str; 6] = ["Heat", "Cool", "Amber", "Aurora", "Ice", "Nebula"];

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
fn scan_without_files_lists_the_builtins() {
    for dir in [None, Some(tempfile::tempdir().unwrap())] {
        assert_eq!(LutSet::scan(dir.as_ref().map(|d| d.path())).names(), BUILTINS);
    }
    assert_eq!(LutSet::scan(Some(Path::new("/definitely/missing"))).names(), BUILTINS);
}

#[test]
fn scan_lists_builtins_first_and_indexes_by_name() {
    let dir = tempfile::tempdir().unwrap();
    write_lut(dir.path(), "Sun.svg", [0, 0, 200]);
    write_lut(dir.path(), "Dusk.svg", [200, 100, 0]);
    let set = LutSet::scan(Some(dir.path()));
    assert_eq!(set.names()[..6], BUILTINS);
    assert_eq!(set.names()[6..], ["Dusk", "Sun"]);
    assert_eq!(set.index_of("Sun"), 7);
    assert_eq!(set.index_of("Classic"), 0);
}

#[test]
fn created_folder_holds_templates_that_add_no_entry() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("LUT");
    create_lut_dir(&dir).unwrap();
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), BUILTINS.len());
    assert_eq!(LutSet::scan(Some(&dir)).names(), BUILTINS);
    std::fs::rename(dir.join("Ice.svg"), dir.join("ice.svg")).unwrap();
    let heat = std::fs::read_to_string(dir.join("Heat.svg")).unwrap();
    std::fs::write(dir.join("Heat.svg"), heat + "<!-- same colours -->").unwrap();
    assert_eq!(LutSet::scan(Some(&dir)).names(), BUILTINS);
}

#[test]
fn edited_builtins_are_listed_as_custom() {
    let dir = tempfile::tempdir().unwrap();
    write_lut(dir.path(), "HEAT.svg", [1, 2, 3]);
    write_lut(dir.path(), "ice.svg", [4, 5, 6]);
    write_lut(dir.path(), "Fire.svg", [7, 8, 9]);
    let set = LutSet::scan(Some(dir.path()));
    assert_eq!(set.names()[..6], BUILTINS);
    assert_eq!(set.names()[6..], ["Fire", "Heat (Custom)", "Ice (Custom)"]);
    assert_eq!(set.luts[0].as_ref(), &Lut::builtins()[0]);
    assert_eq!(set.luts[7].colors[0], [1, 2, 3, 255]);
}
