// Enables `dev_tests` when the optional dev/tests/unit folder exists (see docs/CONVENTIONS.md).
fn main() {
    let unit = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../dev/tests/unit");
    println!("cargo::rustc-check-cfg=cfg(dev_tests)");
    if unit.exists() {
        println!("cargo::rustc-cfg=dev_tests");
    }
    println!("cargo::rerun-if-changed={}", unit.display());
}
