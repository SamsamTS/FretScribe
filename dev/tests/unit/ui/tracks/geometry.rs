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
