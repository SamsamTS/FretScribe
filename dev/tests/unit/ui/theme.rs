use super::*;

#[test]
fn accent_round_trips_and_text_contrasts() {
    set_accent(Color32::from_rgb(10, 20, 30));
    assert_eq!(accent(), Color32::from_rgb(10, 20, 30));
    assert_eq!(accent_text_on(), Color32::WHITE);
    set_accent(DEFAULT_ACCENT);
    assert_ne!(accent_text_on(), Color32::WHITE);
}

#[test]
fn fonts_register_every_family() {
    let mut fonts = FontDefinitions::default();
    add_fonts(&mut fonts);
    for f in [medium(), mono()] {
        assert!(fonts.families.contains_key(&f));
    }
    assert_eq!(fonts.families[&FontFamily::Proportional][0], INTER);
}
