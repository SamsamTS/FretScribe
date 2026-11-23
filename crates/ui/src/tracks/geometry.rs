//! Lane geometry in whole physical pixels. A strip is the waveform (`WAVE_H_PX`) plus its border,
//! `STRIP_PITCH_PX` in all; a spectrogram strip is a whole number of those, so every strip top is a
//! multiple of the pitch from the top of the scrolling area.

pub const WAVE_H_PX: u32 = 66;
pub const BORDER_PX: u32 = 1;
pub const STRIP_PITCH_PX: u32 = WAVE_H_PX + BORDER_PX;
pub const SPECTRO_ROWS: u32 = 5;
pub const MAX_SPECTRO_ROWS: u32 = 16;
pub(crate) const COLLAPSE_ANIM_SEC: f32 = 0.18;
pub(crate) const RESIZE_TAU_MS: f64 = 60.0;
pub(crate) const REORDER_TAU_MS: f64 = 60.0;

/// Rows shown while the spectrogram is `expanded` (0 = collapsed, 1 = open) of `rows`; always whole.
pub fn rows_shown(rows: f32, expanded: f32) -> u32 {
    (rows * expanded.clamp(0.0, 1.0)).round() as u32
}

pub fn lane_height_px(shown_rows: u32) -> u32 {
    (shown_rows + 1) * STRIP_PITCH_PX
}

pub fn clamp_rows(rows: i64) -> u32 {
    rows.clamp(1, i64::from(MAX_SPECTRO_ROWS)) as u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneStrip {
    pub id: u64,
    pub top_px: u32,
    pub rows: u32,
}

impl LaneStrip {
    pub fn height_px(&self) -> u32 {
        lane_height_px(self.rows)
    }

    pub fn wave_top_px(&self) -> u32 {
        self.top_px + self.rows * STRIP_PITCH_PX
    }
}

/// Lanes stacked from the top, then the "+" row (one pitch tall). Returns the strips and the total
/// content height.
pub fn stack(lanes: &[(u64, u32)]) -> (Vec<LaneStrip>, u32) {
    let mut top = 0;
    let strips = lanes
        .iter()
        .map(|&(id, rows)| {
            let strip = LaneStrip { id, top_px: top, rows };
            top += strip.height_px();
            strip
        })
        .collect();
    (strips, top + STRIP_PITCH_PX)
}

/// Where a dragged lane lands: the number of other lanes whose centre is above the pointer.
pub fn reorder_index(centers_y: &[f32], dragged: usize, pointer_y: f32) -> usize {
    centers_y.iter().enumerate().filter(|&(i, &c)| i != dragged && c < pointer_y).count()
}

#[cfg(all(test, dev_tests))]
#[path = "../../../../dev/tests/unit/ui/tracks/geometry.rs"]
mod tests;
