use super::*;

fn pixel(buffer: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let at = (y * width + x) * 4;
    [buffer[at], buffer[at + 1], buffer[at + 2], buffer[at + 3]]
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-5, "{a} != {b}");
}

/// Block `k` alternates between `+(k+1)/100` and `-(k+1)/100`.
fn stepped(blocks: usize) -> Vec<f32> {
    (0..blocks * BASE_BLOCK)
        .map(|i| {
            let amp = ((i / BASE_BLOCK) + 1) as f32 / 100.0;
            if i % 2 == 0 {
                amp
            } else {
                -amp
            }
        })
        .collect()
}

fn reference(samples: &[f32]) -> Peak {
    let min = samples.iter().copied().fold(f32::INFINITY, f32::min);
    let max = samples.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mean_sq = samples
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum::<f64>()
        / samples.len() as f64;
    (min, max, mean_sq.sqrt() as f32)
}

fn noise(len: usize) -> Vec<f32> {
    let mut state = 0x2545_f491_u32;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 * 2.0 - 1.0
        })
        .collect()
}

#[test]
fn base_level_min_max_rms_match_a_known_signal() {
    let pyramid = WaveformPyramid::build(&stepped(10), 6400);
    let peaks = pyramid.peaks((0.0, 0.1), 10);
    assert_eq!(peaks.len(), 10);
    for (k, &(min, max, rms)) in peaks.iter().enumerate() {
        let amp = (k + 1) as f32 / 100.0;
        close(min, -amp);
        close(max, amp);
        close(rms, amp);
    }
}

#[test]
fn whole_file_in_one_column_is_exact_for_any_length() {
    for len in [1, 63, 64, 65, 1000, 4097, 100_000] {
        let signal = noise(len);
        let pyramid = WaveformPyramid::build(&signal, 1000);
        let (min, max, rms) = pyramid.peaks((0.0, len as f64 / 1000.0), 1)[0];
        let (rmin, rmax, rrms) = reference(&signal);
        assert_eq!((min, max), (rmin, rmax), "len {len}");
        close(rms, rrms);
    }
}

#[test]
fn coarse_columns_keep_the_global_extremes_and_energy() {
    let signal = noise(50_000);
    let pyramid = WaveformPyramid::build(&signal, 1000);
    let (rmin, rmax, _) = reference(&signal);
    for width in [2, 7, 100, 333, 2000] {
        let peaks = pyramid.peaks((0.0, 50.0), width);
        assert_eq!(peaks.len(), width);
        let min = peaks.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let max = peaks.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        assert_eq!((min, max), (rmin, rmax), "width {width}");
        assert!(peaks.iter().all(|p| p.0 <= p.2 && p.2 <= p.1.max(-p.0)));
    }
}

#[test]
fn zooming_in_past_one_block_never_leaves_gaps() {
    let pyramid = WaveformPyramid::build(&stepped(4), 6400);
    let peaks = pyramid.peaks((0.0, 0.04), 400);
    assert!(peaks.iter().all(|p| p.1 > 0.0 && p.0 < 0.0));
    close(peaks[0].1, 0.01);
    close(peaks[399].1, 0.04);
}

#[test]
fn ranges_outside_the_audio_are_silent_columns() {
    let signal = vec![0.5; 100];
    let pyramid = WaveformPyramid::build(&signal, 100);
    let silent = (0.0, 0.0, 0.0);

    let before = pyramid.peaks((-1.0, 1.0), 4);
    assert_eq!(&before[..2], [silent; 2]);
    assert!(before[2..].iter().all(|p| p.1 == 0.5));

    let after = pyramid.peaks((0.5, 1.5), 4);
    assert!(after[..2].iter().all(|p| p.1 == 0.5));
    assert_eq!(&after[2..], [silent; 2]);

    assert_eq!(pyramid.peaks((5.0, 6.0), 3), [silent; 3]);
    assert_eq!(pyramid.peaks((-9.0, -8.0), 3), [silent; 3]);
}

#[test]
fn zero_width_and_invalid_ranges() {
    let pyramid = WaveformPyramid::build(&noise(1000), 1000);
    assert!(pyramid.peaks((0.0, 1.0), 0).is_empty());
    let silent = (0.0, 0.0, 0.0);
    assert_eq!(pyramid.peaks((1.0, 1.0), 2), [silent; 2]);
    assert_eq!(pyramid.peaks((2.0, 1.0), 2), [silent; 2]);
    assert_eq!(pyramid.peaks((f64::NAN, 1.0), 2), [silent; 2]);
    assert_eq!(pyramid.peaks((0.0, f64::INFINITY), 2), [silent; 2]);
}

#[test]
fn empty_input_and_zero_sample_rate_are_inert() {
    let empty = WaveformPyramid::build(&[], 44_100);
    assert_eq!(empty.duration_sec(), 0.0);
    assert_eq!(empty.peaks((0.0, 1.0), 3), [(0.0, 0.0, 0.0); 3]);
    let no_rate = WaveformPyramid::build(&[0.5; 10], 0);
    assert_eq!(no_rate.duration_sec(), 0.0);
    assert_eq!(no_rate.peaks((0.0, 1.0), 2), [(0.0, 0.0, 0.0); 2]);
}

#[test]
fn non_finite_samples_count_as_silence() {
    let signal = [f32::NAN, 0.5, f32::INFINITY, -0.25];
    let (min, max, rms) = WaveformPyramid::build(&signal, 4).peaks((0.0, 1.0), 1)[0];
    assert_eq!((min, max), (-0.25, 0.5));
    assert!(rms.is_finite());
}

#[test]
fn ten_minutes_of_audio_builds_and_stays_exact() {
    let rate = 44_100;
    let len = 600 * rate as usize;
    let signal: Vec<f32> = (0..len).map(|i| (i % 1000) as f32 / 1000.0 - 0.5).collect();
    let pyramid = WaveformPyramid::build(&signal, rate);
    let (min, max, _) = pyramid.peaks((0.0, 600.0), 1)[0];
    assert_eq!((min, max), (-0.5, 0.499));
    assert_eq!(pyramid.peaks((100.0, 200.0), 1600).len(), 1600);
}

#[test]
fn render_buffer_size_and_background() {
    let lut = Lut::from_stops("Tint", &[(0.0, [10, 20, 30]), (1.0, [250, 250, 250])]);
    let peaks = [(0.0, 0.0, 0.0); 7];
    let buffer = render_rgba(&peaks, 11, &lut);
    assert_eq!(buffer.len(), 7 * 11 * 4);
    assert_eq!(pixel(&buffer, 7, 3, 0), [10, 20, 30, 255]);
    assert_eq!(pixel(&buffer, 7, 6, 10), [10, 20, 30, 255]);
    assert!(render_rgba(&[], 10, &lut).is_empty());
    assert!(render_rgba(&peaks, 0, &lut).is_empty());
}

#[test]
fn silence_leaves_only_background_and_centre_line() {
    let lut = Lut::grey();
    let (width, height) = (5, 9);
    let buffer = render_rgba(&vec![(0.0, 0.0, 0.0); width], height, &lut);
    let centre = lut.color(CENTRE_LINE_LEVEL);
    for y in 0..height as usize {
        for x in 0..width {
            let expected = if y == height as usize / 2 {
                centre
            } else {
                lut.colors[0]
            };
            assert_eq!(pixel(&buffer, width, x, y), expected, "({x}, {y})");
        }
    }
    assert_ne!(centre, lut.colors[0]);
}

#[test]
fn full_scale_reaches_top_and_bottom_rows() {
    let lut = Lut::grey();
    for height in [8, 9, 300] {
        let buffer = render_rgba(&[(-1.0, 1.0, 0.7), (0.0, 0.0, 0.0)], height, &lut);
        let last = height as usize - 1;
        assert_ne!(pixel(&buffer, 2, 0, 0), lut.colors[0], "height {height}");
        assert_ne!(pixel(&buffer, 2, 0, last), lut.colors[0], "height {height}");
        assert_eq!(pixel(&buffer, 2, 1, 0), lut.colors[0]);
    }
}

#[test]
fn envelope_is_limited_to_min_max_and_the_centre_is_brighter() {
    let lut = Lut::grey();
    let height = 100;
    let buffer = render_rgba(&[(-0.5, 0.5, 0.2)], height, &lut);
    let at = |y| pixel(&buffer, 1, 0, y)[0];
    assert_eq!(at(0), 0);
    assert_eq!(at(24), 0);
    assert!(at(26) > 0 && at(73) > 0);
    assert_eq!(at(76), 0);
    assert_eq!(at(99), 0);
    assert!(at(50) > at(30), "centre {} tip {}", at(50), at(30));
}

#[test]
fn louder_columns_are_brighter() {
    let lut = Lut::grey();
    let buffer = render_with_smoothing(&[(-0.2, 0.2, 0.1), (-0.8, 0.8, 0.1)], 100, &lut, 0);
    let top_of = |x: usize| (0..100).find(|&y| pixel(&buffer, 2, x, y)[0] > 0).unwrap();
    assert!(top_of(1) < top_of(0));
    assert!(pixel(&buffer, 2, 1, top_of(1))[0] > pixel(&buffer, 2, 0, top_of(0))[0]);
}

#[test]
fn garbage_peaks_do_not_panic() {
    let lut = Lut::grey();
    let peaks = [
        (f32::NAN, f32::INFINITY, -3.0),
        (5.0, -5.0, 9.0),
        (0.3, 0.1, 0.0),
    ];
    let buffer = render_rgba(&peaks, 16, &lut);
    assert_eq!(buffer.len(), 3 * 16 * 4);
}

#[test]
fn renders_with_the_repository_palettes() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LUT");
    for lut in crate::lut::discover_luts(&dir) {
        let buffer = render_rgba(&[(-0.9, 0.9, 0.4)], 64, &lut);
        assert_eq!(pixel(&buffer, 1, 0, 0), lut.colors[0]);
        assert_ne!(pixel(&buffer, 1, 0, 3), lut.colors[0]);
        assert_ne!(pixel(&buffer, 1, 0, 3), pixel(&buffer, 1, 0, 32));
        assert_eq!(buffer[3], 255);
    }
}

fn drawn_mask(buffer: &[u8], lut: &Lut) -> Vec<bool> {
    buffer
        .chunks_exact(4)
        .map(|p| p != lut.colors[0].as_slice())
        .collect()
}

fn alternating(columns: usize) -> Vec<Peak> {
    (0..columns)
        .map(|x| {
            if x % 2 == 0 {
                (-0.9, 0.9, 0.4)
            } else {
                (-0.3, 0.3, 0.1)
            }
        })
        .collect()
}

#[test]
fn smoothing_changes_colours_but_not_the_contour() {
    let lut = Lut::grey();
    let peaks = alternating(60);
    let sharp = render_with_smoothing(&peaks, 80, &lut, 0);
    let smooth = render_with_smoothing(&peaks, 80, &lut, 15);
    assert_ne!(sharp, smooth);
    assert_eq!(drawn_mask(&sharp, &lut), drawn_mask(&smooth, &lut));
}

#[test]
fn smoothing_evens_out_neighbouring_columns() {
    let lut = Lut::grey();
    let peaks = alternating(60);
    let row = 40;
    let spread = |smoothing| {
        let buffer = render_with_smoothing(&peaks, 80, &lut, smoothing);
        let a = i32::from(pixel(&buffer, 60, 30, row)[0]);
        let b = i32::from(pixel(&buffer, 60, 31, row)[0]);
        (a, b)
    };
    let (a, b) = spread(0);
    assert!(a > 0 && b > 0 && (a - b).abs() > 40, "{a} {b}");
    let (a, b) = spread(15);
    assert!((a - b).abs() <= 20, "{a} {b}");
}

#[test]
fn smoothing_of_zero_or_one_pixel_is_the_identity() {
    let lut = Lut::grey();
    let peaks = alternating(20);
    let off = render_with_smoothing(&peaks, 50, &lut, 0);
    assert_eq!(render_with_smoothing(&peaks, 50, &lut, 1), off);
}

#[test]
fn silence_does_not_dim_the_edges_of_a_note() {
    let lut = Lut::grey();
    let mut peaks = vec![(0.0, 0.0, 0.0); 20];
    peaks.extend(vec![(-0.6, 0.6, 0.3); 20]);
    peaks.extend(vec![(0.0, 0.0, 0.0); 20]);
    let sharp = render_with_smoothing(&peaks, 60, &lut, 0);
    let smooth = render_with_smoothing(&peaks, 60, &lut, 15);
    assert_eq!(sharp, smooth);
}

#[test]
fn envelope_and_body_each_fade_from_the_centre_to_their_tips() {
    let lut = Lut::grey();
    let buffer = render_with_smoothing(&[(-0.8, 0.8, 0.4)], 100, &lut, 0);
    let at = |y: usize| i32::from(pixel(&buffer, 1, 0, y)[0]);
    // Body rows 30..=69, envelope-only rows 10..=29 and 70..=89.
    assert!((30..49).all(|y| at(y) <= at(y + 1)), "upper body");
    assert!((50..69).all(|y| at(y) >= at(y + 1)), "lower body");
    assert!((10..29).all(|y| at(y) <= at(y + 1)), "upper envelope");
    assert!((70..89).all(|y| at(y) >= at(y + 1)), "lower envelope");
    assert!(at(49) > at(30) + 25 && at(50) > at(69) + 25);
    assert!(at(29) > at(10) + 20 && at(70) > at(89) + 20);
    assert!((10..=89).all(|y| at(y) == at(99 - y)));
}

#[test]
fn body_and_envelope_stay_distinct_where_they_meet() {
    let lut = Lut::grey();
    for (peak, rms) in [(0.8, 0.4), (1.0, 0.5), (0.3, 0.1)] {
        let buffer = render_with_smoothing(&[(-peak, peak, rms)], 200, &lut, 0);
        let at = |y: usize| i32::from(pixel(&buffer, 1, 0, y)[0]);
        // Inside a part, neighbouring rows differ by a few levels; the body edge is a step.
        let step = (1..100).map(|y| at(y) - at(y - 1)).max().unwrap_or(0);
        assert!(step > 15, "peak {peak} rms {rms}: largest step {step}");
    }
}

#[test]
fn one_sided_columns_do_not_misbehave() {
    let lut = Lut::grey();
    let buffer = render_rgba(&[(0.2, 0.7, 0.4), (-0.7, -0.2, 0.4)], 64, &lut);
    assert_eq!(buffer.len(), 2 * 64 * 4);
}
