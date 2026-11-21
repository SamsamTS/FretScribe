use super::*;

impl Lut {
    /// Black to white: the palette index is the grey level.
    pub(crate) fn grey() -> Self {
        Self::from_stops("Grey", &[(0.0, [0, 0, 0]), (1.0, [255, 255, 255])])
    }
}
