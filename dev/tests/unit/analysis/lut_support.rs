use super::*;

impl Lut {
    /// A linear ramp, weakest first.
    pub(crate) fn ramp(name: &str, from: [u8; 3], to: [u8; 3]) -> Self {
        let colors = std::array::from_fn(|i| {
            let w = i as f32 / (LUT_SIZE - 1) as f32;
            let mix = |c: usize| (f32::from(from[c]) * (1.0 - w) + f32::from(to[c]) * w).round() as u8;
            [mix(0), mix(1), mix(2), 255]
        });
        Self {
            name: name.to_owned(),
            colors,
        }
    }

    /// Black to white: the palette index is the grey level.
    pub(crate) fn grey() -> Self {
        Self::ramp("Grey", [0, 0, 0], [255, 255, 255])
    }
}
