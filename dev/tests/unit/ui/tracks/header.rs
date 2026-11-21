use super::*;

#[test]
fn volume_readout_in_decibels() {
    assert_eq!(volume_text(1.0), "+0.0 dB");
    assert_eq!(volume_text(0.5), "-6.0 dB");
    assert_eq!(volume_text(1.5), "+3.5 dB");
    assert_eq!(volume_text(0.0), "-inf dB");
}

#[test]
fn inaudible_tracks_are_dimmed() {
    assert_eq!(opacity_for(true), 1.0);
    assert!(opacity_for(false) < 0.6);
}
