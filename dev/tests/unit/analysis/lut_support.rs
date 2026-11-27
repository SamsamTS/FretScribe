use super::*;

impl Lut {
    pub(crate) fn from_stops(name: &str, stops: &[(f32, [u8; 3])]) -> Self {
        let mut colors = [[0, 0, 0, 255]; LUT_SIZE];
        for (i, color) in colors.iter_mut().enumerate() {
            *color = gradient_at(stops, i as f32 / (LUT_SIZE - 1) as f32);
        }
        Self {
            name: name.to_owned(),
            colors,
        }
    }

    /// Black to white: the palette index is the grey level.
    pub(crate) fn grey() -> Self {
        Self::from_stops("Grey", &[(0.0, [0, 0, 0]), (1.0, [255, 255, 255])])
    }
}

fn gradient_at(stops: &[(f32, [u8; 3])], t: f32) -> [u8; 4] {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return [0, 0, 0, 255];
    };
    let rgb = if t <= first.0 {
        first.1
    } else if t >= last.0 {
        last.1
    } else {
        let upper = stops
            .iter()
            .position(|s| s.0 >= t)
            .unwrap_or(stops.len() - 1)
            .max(1);
        let (a, b) = (stops[upper - 1], stops[upper]);
        let w = (t - a.0) / (b.0 - a.0).max(f32::EPSILON);
        std::array::from_fn(|c| {
            (f32::from(a.1[c]) * (1.0 - w) + f32::from(b.1[c]) * w).round() as u8
        })
    };
    [rgb[0], rgb[1], rgb[2], 255]
}
