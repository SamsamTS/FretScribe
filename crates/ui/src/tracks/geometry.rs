//! Lane geometry in whole physical pixels. A strip is the waveform (`WAVE_H_PX`) plus its border,
//! `STRIP_PITCH_PX` in all; a spectrogram strip is a whole number of those, so every strip top is a
//! multiple of the pitch from the top of the scrolling area.

pub const WAVE_H_PX: u32 = 66;
pub const BORDER_PX: u32 = 1;
pub const STRIP_PITCH_PX: u32 = WAVE_H_PX + BORDER_PX;
pub const SPECTRO_ROWS: u32 = 5;
pub const MAX_SPECTRO_ROWS: u32 = 16;
pub const COLLAPSE_ANIM_SEC: f32 = 0.18;
pub const RESIZE_ANIM_SEC: f32 = 0.18;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitch_is_wave_plus_border() {
        assert_eq!(STRIP_PITCH_PX, WAVE_H_PX + BORDER_PX);
        assert_eq!(lane_height_px(0), STRIP_PITCH_PX);
        assert_eq!(lane_height_px(SPECTRO_ROWS), (SPECTRO_ROWS + 1) * STRIP_PITCH_PX);
    }

    #[test]
    fn collapsing_steps_through_whole_rows_down_to_zero() {
        let heights: Vec<u32> = (0..=20).rev().map(|i| lane_height_px(rows_shown(5.0, i as f32 / 20.0))).collect();
        assert!(heights.windows(2).all(|w| w[0] >= w[1]));
        assert_eq!((heights[0], *heights.last().unwrap()), (6 * STRIP_PITCH_PX, STRIP_PITCH_PX));
        assert!(heights.iter().all(|h| h % STRIP_PITCH_PX == 0));
        assert_eq!(rows_shown(5.0, 2.0), 5);
        assert_eq!(rows_shown(5.0, -1.0), 0);
        assert_eq!(rows_shown(5.0, f32::NAN), 0);
    }

    #[test]
    fn every_strip_top_is_a_multiple_of_the_pitch_and_the_plus_row_closes_the_stack() {
        let (strips, content) = stack(&[(1, 5), (2, 0), (3, 3)]);
        assert!(strips.iter().all(|s| s.top_px % STRIP_PITCH_PX == 0 && s.wave_top_px() % STRIP_PITCH_PX == 0));
        assert_eq!(strips[1].top_px, 6 * STRIP_PITCH_PX);
        assert_eq!(strips[2].top_px, 7 * STRIP_PITCH_PX);
        assert_eq!(content, (7 + 4 + 1) * STRIP_PITCH_PX);
        assert_eq!(stack(&[]).1, STRIP_PITCH_PX);
    }

    #[test]
    fn resizing_is_whole_steps_within_limits() {
        assert_eq!(clamp_rows(-3), 1);
        assert_eq!(clamp_rows(7), 7);
        assert_eq!(clamp_rows(99), MAX_SPECTRO_ROWS);
    }

    #[test]
    fn reorder_counts_lanes_above_the_pointer() {
        let centers = [50.0, 150.0, 250.0];
        assert_eq!(reorder_index(&centers, 0, 400.0), 2);
        assert_eq!(reorder_index(&centers, 2, 0.0), 0);
        assert_eq!(reorder_index(&centers, 1, 160.0), 1);
    }
}
