use super::*;

#[test]
fn only_inter_and_jetbrains_mono_back_the_text_families() {
    let mut fonts = FontDefinitions::default();
    add_fonts(&mut fonts);
    for family in [FontFamily::Proportional, FontFamily::Monospace, medium(), mono()] {
        let names = &fonts.families[&family];
        assert!(names.iter().all(|n| fonts.font_data.contains_key(n)), "{family:?} names a missing font");
        let first = &names[0];
        assert!([INTER, INTER_MEDIUM, MONO].contains(&first.as_str()), "{family:?} starts with {first}");
        assert!(names.iter().all(|n| [INTER, INTER_MEDIUM, MONO, "emoji-icon-font"].contains(&n.as_str())), "{names:?}");
    }
    assert!(!fonts.font_data.contains_key("Hack") && !fonts.font_data.contains_key("Ubuntu-Light"));
}
