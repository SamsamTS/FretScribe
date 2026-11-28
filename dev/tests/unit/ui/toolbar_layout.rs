use super::Group::*;
use super::*;

/// The full and collapsed widths of the design mockup.
fn width(group: Group, collapsed: bool) -> f32 {
    let (full, chip) = match group {
        Project => (296.0, 0.0),
        History => (86.0, 0.0),
        Tuning => (626.0, 180.0),
        Spectrogram => (518.0, 110.0),
        Playback => (388.0, 0.0),
        Listening => (453.0, 136.0),
        Grid => (388.0, 150.0),
    };
    if collapsed {
        chip
    } else {
        full
    }
}

fn fit(window: f32) -> Fit<'static> {
    Fit { window, width: &width }
}

fn rows(solved: &Solved) -> Vec<Row> {
    solved.lines.iter().map(|l| l.sides.clone()).collect()
}

fn default_collapsed() -> Vec<Group> {
    ToolbarState::default().closed
}

#[test]
fn default_state_at_the_check_widths() {
    let state = ToolbarState::default();
    let one = vec![[vec![Project, History, Tuning, Playback, Listening], vec![Spectrogram, Grid]]];
    let joined = vec![[vec![Project, History, Tuning], vec![Spectrogram, Grid]], [vec![Playback, Listening], vec![]]];
    for (window, expected, grid_collapsed) in [
        (1920.0, &one, false),
        (1676.0, &one, false),
        (1675.0, &joined, false),
        (1536.0, &joined, false),
        (1132.0, &joined, false),
        (1131.0, &state.rows, false),
        (964.0, &state.rows, false),
        (963.0, &state.rows, true),
        (900.0, &state.rows, true),
    ] {
        let solved = fit(window).solve(&state);
        assert_eq!(&rows(&solved), expected, "{window}");
        let mut collapsed = default_collapsed();
        collapsed.extend(grid_collapsed.then_some(Grid));
        assert_eq!(solved.collapsed, collapsed, "{window}");
    }
}

#[test]
fn an_overfull_row_hands_its_rightmost_groups_over_before_collapsing() {
    let mut state = ToolbarState::default();
    state.expand(Tuning);
    // Row 1 needs 1180 pt with Tuning open; its right side is the last to move.
    let solved = fit(1100.0).solve(&state);
    assert!(!solved.collapsed.contains(&Tuning));
    assert_eq!(rows(&solved), vec![[vec![Project, History, Tuning], vec![]], [vec![Playback, Listening], vec![Spectrogram, Grid]]]);
}

#[test]
fn the_group_the_user_expanded_is_the_last_to_collapse() {
    let mut state = ToolbarState::default();
    state.closed.clear();
    let narrow = fit(1200.0);
    assert!(narrow.solve(&state).collapsed.contains(&Tuning));
    state.expand(Tuning);
    let solved = narrow.solve(&state);
    assert!(!solved.collapsed.contains(&Tuning) && solved.collapsed.contains(&Spectrogram), "{solved:?}");
    // Of two expanded groups, the older one collapses first.
    state.expand(Listening);
    let solved = fit(1000.0).solve(&state);
    assert!(solved.collapsed.contains(&Tuning) && !solved.collapsed.contains(&Listening), "{solved:?}");
}

#[test]
fn a_user_collapsed_group_stays_collapsed_and_expand_is_offered_only_when_it_would_hold() {
    let mut state = ToolbarState::default();
    state.collapse(Grid);
    assert!(fit(2000.0).solve(&state).collapsed.contains(&Grid));
    assert!(fit(900.0).fits(&state.rows, Some(Listening)));
    // With Tuning open, row 1 needs 1180 pt and no spill rescues it below that.
    assert!(fit(1180.0).fits(&state.rows, Some(Tuning)));
    assert!(!fit(1000.0).fits(&state.rows, Some(Tuning)));
    state.expand(Grid);
    assert!(!state.closed.contains(&Grid) && state.expanded == [Grid]);
    state.collapse(Grid);
    assert!(state.expanded.is_empty());
}

#[test]
fn nothing_is_clipped_when_even_all_collapsed_does_not_fit() {
    let state = ToolbarState { rows: vec![[Group::ALL.to_vec(), vec![]], Row::default()], ..ToolbarState::default() };
    let narrow = fit(900.0);
    let solved = narrow.solve(&state);
    assert!(solved.lines.len() > 1);
    assert_eq!(solved.collapsed, Group::COLLAPSIBLE);
    for line in &solved.lines {
        for (g, x) in narrow.place(&line.sides, &solved.collapsed) {
            assert!(x >= MARGIN && x + width(g, solved.collapsed.contains(&g)) <= 900.0 - MARGIN, "{g:?} {x}");
        }
    }
    assert_eq!(solved.lines.iter().flat_map(|l| l.sides.concat()).collect::<Vec<_>>(), Group::ALL);
}

#[test]
fn empty_rows_are_hidden_except_while_editing() {
    let state = ToolbarState { rows: vec![[vec![Project, History, Tuning, Spectrogram], vec![]], Row::default()], ..ToolbarState::default() };
    let mut state = state;
    state.rows[0][1] = vec![Playback, Listening, Grid];
    assert_eq!(fit(2000.0).solve(&state).lines.len(), 1);
    assert_eq!(fit(2000.0).editing(&state).lines.len(), 2);
}

#[test]
fn dropping_moves_only_the_dragged_group() {
    let state = ToolbarState::default();
    let wide = fit(1536.0);
    // The far left of the second line.
    let target = wide.drop_target(&state, Grid, 1, 5.0).unwrap();
    assert!(target.ok);
    assert_eq!(target.rows, vec![state.rows[0].clone(), [vec![Grid, Playback, Listening], vec![]]]);
    // The right half of the first line, past its last group.
    let target = wide.drop_target(&state, History, 0, 1530.0).unwrap();
    assert_eq!(target.rows[0], [vec![Project, Tuning], vec![Spectrogram, History]]);
    // Onto itself: unchanged.
    let target = wide.drop_target(&state, Project, 0, 20.0).unwrap();
    assert_eq!(target.rows, state.rows);
}

#[test]
fn a_drop_that_cannot_fit_is_refused() {
    let state = ToolbarState { rows: vec![[vec![Project, History, Tuning, Spectrogram, Listening], vec![]], [vec![Playback, Grid], vec![]]], ..ToolbarState::default() };
    // At 900 pt a spill still saves the drop; at 650 two rows cannot hold the groups.
    assert!(fit(900.0).drop_target(&state, Playback, 0, 890.0).unwrap().ok);
    assert!(!fit(650.0).drop_target(&state, Playback, 0, 640.0).unwrap().ok);
}

#[test]
fn saved_data_that_is_wrong_falls_back_to_the_default_arrangement() {
    let default = ToolbarState::default();
    for json in [
        r#"{"rows":[[["Project"],[]],[[],[]]]}"#,
        r#"{"rows":[[["Project","History","Tuning","Spectrogram","Playback","Listening","Grid","Grid"],[]],[[],[]]]}"#,
        r#"{"rows":[[["Project","History","Tuning","Spectrogram","Playback","Listening","Mixer"],[]],[[],[]]]}"#,
        r#"{"rows":[[["Project","History","Tuning","Spectrogram","Playback","Listening","Grid"],[]]]}"#,
        r#"{"rows":7}"#,
    ] {
        let mut state: ToolbarState = serde_json::from_str(json).unwrap();
        state.sanitise();
        assert_eq!(state, default, "{json}");
    }
    let mut state: ToolbarState = serde_json::from_str(r#"{"closed":["Project","Grid","Grid"],"expanded":["Grid","Tuning","Nope"]}"#).unwrap();
    state.sanitise();
    assert_eq!((state.closed, state.expanded), (vec![Grid], vec![]));
    let mut state = default.clone();
    state.rows = moved(&state.rows, Grid, 0, 0, 0);
    state.expand(Tuning);
    let mut back: ToolbarState = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    back.sanitise();
    assert_eq!(back, state);
}
