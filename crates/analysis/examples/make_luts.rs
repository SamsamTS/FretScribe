//! Regenerates the palette PNGs of the repository's `LUT/` folder (256x1 pixels each).

use std::path::PathBuf;

use fretscribe_analysis::Lut;
use image::{Rgba, RgbaImage};

const HEAT_STOPS: [(f32, [u8; 3]); 5] = [
    (0.0, [0, 0, 0]),
    (0.25, [90, 0, 140]),
    (0.5, [225, 20, 30]),
    (0.75, [255, 150, 0]),
    (1.0, [255, 255, 255]),
];

fn main() -> Result<(), image::ImageError> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../LUT");
    std::fs::create_dir_all(&dir)?;
    for lut in [Lut::builtin(), Lut::from_stops("Heat", &HEAT_STOPS)] {
        let image = RgbaImage::from_fn(256, 1, |x, _| Rgba(lut.colors[x as usize]));
        let path = dir.join(format!("{}.png", lut.name));
        image.save(&path)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
