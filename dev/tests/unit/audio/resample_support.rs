use super::*;

impl Prepared {
    pub fn from_channels(left: &[f32], right: &[f32]) -> Self {
        let samples = left.iter().zip(right).flat_map(|(&l, &r)| [l, r]).collect();
        Self { samples }
    }
}
