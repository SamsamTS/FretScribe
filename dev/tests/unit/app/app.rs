use super::*;

#[test]
fn home_status_cache_expires_on_age_or_list_change() {
    let mut settings = Settings::default();
    let entry = |p: &str| RecentProject { path: p.into(), name: p.into(), ..Default::default() };
    settings.recent = vec![entry("a.fretscribe")];
    let map = HashMap::from([(PathBuf::from("a.fretscribe"), TileStatus::Ok)]);
    assert!(home_status_fresh(Duration::from_secs(1), &map, &settings));
    assert!(!home_status_fresh(HOME_STATUS_TTL, &map, &settings));
    settings.recent.push(entry("b.fretscribe"));
    assert!(!home_status_fresh(Duration::ZERO, &map, &settings));
    settings.recent = vec![entry("c.fretscribe")];
    assert!(!home_status_fresh(Duration::ZERO, &map, &settings));
}

#[test]
fn missing_project_file_is_reported_without_parsing() {
    assert_eq!(tile_status(Path::new("/nonexistent/x.fretscribe")), TileStatus::MissingProject);
}
