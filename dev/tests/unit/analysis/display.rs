use super::*;

#[test]
fn levels_cut_and_saturate() {
    let l = Levels::new(0.3, 0.8);
    assert_eq!(apply_levels(0.0, l), 0.0);
    assert_eq!(apply_levels(0.3, l), 0.0);
    assert_eq!(apply_levels(0.8, l), 1.0);
    assert_eq!(apply_levels(1.0, l), 1.0);
    assert!((apply_levels(0.55, l) - 0.5).abs() < 1e-6);
}

#[test]
fn clamped_levels_are_always_valid() {
    let cases = [
        (-1.0, 2.0),
        (f32::NAN, f32::NAN),
        (0.5, 0.5),
        (0.9, 0.1),
        (1.0, 1.0),
        (0.995, 1.0),
        (f32::INFINITY, f32::NEG_INFINITY),
        (0.0, 0.0),
    ];
    for (b, w) in cases {
        let l = Levels::new(b, w);
        assert!(
            (0.0..=1.0).contains(&l.black) && (0.0..=1.0).contains(&l.white),
            "{b} {w}"
        );
        assert!(
            l.white - l.black >= MIN_LEVELS_GAP - 1e-6,
            "{b} {w} -> {l:?}"
        );
    }
    assert_eq!(Levels::new(f32::NAN, f32::NAN), Levels::default());
    assert_eq!(Levels::new(1.0, 1.0).white, 1.0);
    assert_eq!(Levels::new(0.5, 0.2).clamped(), Levels::new(0.5, 0.2));
}

#[test]
fn levels_to_db() {
    let params = SpectrogramParams::default();
    assert_eq!(
        Levels::default().to_db(&params),
        (params.db_floor, params.db_ceiling)
    );
    let (b, w) = Levels::new(0.5, 1.0).to_db(&params);
    assert!((b - (-45.0)).abs() < 1e-4 && (w - 0.0).abs() < 1e-4);
}

#[test]
fn gamma_properties() {
    for i in 0..=20 {
        let v = i as f32 / 20.0;
        assert!((apply_gamma(v, 1.0) - v).abs() < 1e-6);
    }
    for g in [0.2, 0.5, 1.0, 2.0, 5.0, 100.0, -3.0] {
        assert_eq!(apply_gamma(0.0, g), 0.0);
        assert_eq!(apply_gamma(1.0, g), 1.0);
    }
    for v in [0.1, 0.4, 0.5, 0.9] {
        assert!(apply_gamma(v, 0.5) < v);
        assert!(apply_gamma(v, 2.0) > v);
    }
    for g in [0.3, 1.0, 3.0] {
        let mut last = -1.0;
        for i in 0..=100 {
            let y = apply_gamma(i as f32 / 100.0, g);
            assert!(y >= last);
            last = y;
        }
    }
    assert_eq!(apply_gamma(0.5, 100.0), apply_gamma(0.5, GAMMA_MAX));
    assert_eq!(apply_gamma(0.5, 0.001), apply_gamma(0.5, GAMMA_MIN));
    assert_eq!(apply_gamma(0.5, f32::NAN), 0.5);
}
