use super::*;

const P: u32 = 67;

#[test]
fn pitch_follows_the_display_scale_in_whole_pixels() {
    assert_eq!([1.0, 1.25, 1.5, 2.0].map(strip_pitch_px), [67, 84, 101, 134]);
    assert_eq!(lane_height_px(0, P), P);
    assert_eq!(lane_height_px(SPECTRO_ROWS, P), (SPECTRO_ROWS + 1) * P);
}

#[test]
fn collapsing_steps_through_whole_rows_down_to_zero() {
    let heights: Vec<u32> = (0..=20).rev().map(|i| lane_height_px(rows_shown(5.0, i as f32 / 20.0), P)).collect();
    assert!(heights.windows(2).all(|w| w[0] >= w[1]));
    assert_eq!((heights[0], *heights.last().unwrap()), (6 * P, P));
    assert!(heights.iter().all(|h| h % P == 0));
    assert_eq!(rows_shown(5.0, 2.0), 5);
    assert_eq!(rows_shown(5.0, -1.0), 0);
    assert_eq!(rows_shown(5.0, f32::NAN), 0);
}

#[test]
fn every_strip_top_is_a_multiple_of_the_pitch_and_the_plus_row_closes_the_stack() {
    for p in [P, 84, 101, 134] {
        let (strips, content) = stack(&[(1, 5), (2, 0), (3, 3)], p);
        assert!(strips.iter().all(|s| s.top_px % p == 0 && s.wave_top_px() % p == 0));
        assert_eq!(strips[1].top_px, 6 * p);
        assert_eq!(strips[2].top_px, 7 * p);
        assert_eq!(content, (7 + 4 + 1) * p);
        assert_eq!(stack(&[], p).1, p);
    }
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
