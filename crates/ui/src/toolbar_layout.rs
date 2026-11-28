//! Where the editor toolbar's groups go. The saved arrangement is two rows with a left and a right
//! side; `solve` joins, spills and auto-collapses them to fit the window.

use serde::{Deserialize, Deserializer, Serialize};

pub const GAP: f32 = 10.0;
pub const MARGIN: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Group {
    Project,
    History,
    Tuning,
    Spectrogram,
    Playback,
    Listening,
    Grid,
}

impl Group {
    pub const ALL: [Self; 7] = [Self::Project, Self::History, Self::Tuning, Self::Spectrogram, Self::Playback, Self::Listening, Self::Grid];
    /// The groups that can collapse to a button, the first to do so first.
    pub const COLLAPSIBLE: [Self; 4] = [Self::Tuning, Self::Spectrogram, Self::Listening, Self::Grid];

    pub fn title(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::History => "History",
            Self::Tuning => "Tuning",
            Self::Spectrogram => "Spectrogram",
            Self::Playback => "Playback",
            Self::Listening => "Listening",
            Self::Grid => "Notes & grid",
        }
    }
}

/// The left and right side of a row.
pub type Row = [Vec<Group>; 2];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolbarState {
    /// Two rows holding every group once.
    #[serde(deserialize_with = "or_default_rows")]
    pub rows: Vec<Row>,
    /// Collapsed by the user.
    #[serde(deserialize_with = "or_empty")]
    pub closed: Vec<Group>,
    /// Expanded by the user, oldest first: the last to collapse automatically.
    #[serde(deserialize_with = "or_empty")]
    pub expanded: Vec<Group>,
}

fn default_rows() -> Vec<Row> {
    use Group::*;
    vec![[vec![Project, History, Tuning], vec![Spectrogram]], [vec![Playback, Listening], vec![Grid]]]
}

fn or_default_rows<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Row>, D::Error> {
    Ok(serde_json::from_value(serde_json::Value::deserialize(d)?).unwrap_or_else(|_| default_rows()))
}

fn or_empty<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Group>, D::Error> {
    Ok(serde_json::from_value(serde_json::Value::deserialize(d)?).unwrap_or_default())
}

impl Default for ToolbarState {
    fn default() -> Self {
        Self { rows: default_rows(), closed: vec![Group::Tuning, Group::Spectrogram, Group::Listening], expanded: Vec::new() }
    }
}

impl ToolbarState {
    pub fn sanitise(&mut self) {
        let groups: Vec<Group> = self.rows.iter().flatten().flatten().copied().collect();
        if self.rows.len() != 2 || groups.len() != Group::ALL.len() || !Group::ALL.iter().all(|g| groups.contains(g)) {
            self.rows = default_rows();
        }
        for list in [&mut self.closed, &mut self.expanded] {
            let mut seen = Vec::new();
            list.retain(|g| Group::COLLAPSIBLE.contains(g) && !seen.contains(g) && {
                seen.push(*g);
                true
            });
        }
        self.expanded.retain(|g| !self.closed.contains(g));
    }

    pub fn collapse(&mut self, group: Group) {
        self.expanded.retain(|g| *g != group);
        if !self.closed.contains(&group) {
            self.closed.push(group);
        }
    }

    pub fn expand(&mut self, group: Group) {
        self.closed.retain(|g| *g != group);
        self.expanded.retain(|g| *g != group);
        self.expanded.push(group);
    }
}

/// `rows` with `group` moved to place `index` of a side (`index` counts the side without `group`).
pub fn moved(rows: &[Row], group: Group, row: usize, side: usize, index: usize) -> Vec<Row> {
    let mut rows = without(rows, &[group]);
    let list = &mut rows[row][side];
    list.insert(index.min(list.len()), group);
    rows
}

fn without(rows: &[Row], groups: &[Group]) -> Vec<Row> {
    let mut rows = rows.to_vec();
    rows.iter_mut().flatten().for_each(|side| side.retain(|g| !groups.contains(g)));
    rows
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub sides: Row,
    /// The saved row this line shows (only meaningful while editing).
    pub saved: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Solved {
    pub lines: Vec<Line>,
    pub collapsed: Vec<Group>,
}

/// `width(group, collapsed)` is the width a group takes.
pub struct Fit<'a> {
    pub window: f32,
    pub width: &'a dyn Fn(Group, bool) -> f32,
}

impl Fit<'_> {
    fn span<'g>(&self, groups: impl IntoIterator<Item = &'g Group>, collapsed: &[Group]) -> f32 {
        let (mut sum, mut n) = (0.0, 0_u8);
        for &g in groups {
            sum += (self.width)(g, collapsed.contains(&g));
            n += 1;
        }
        sum + GAP * f32::from(n.saturating_sub(1))
    }

    fn row_fits(&self, row: &Row, collapsed: &[Group]) -> bool {
        self.span(row.iter().flatten(), collapsed) + 2.0 * MARGIN <= self.window
    }

    fn all_fit(&self, rows: &[Row], collapsed: &[Group]) -> bool {
        rows.iter().all(|r| self.row_fits(r, collapsed))
    }

    /// The saved rows, or else the one that is too wide hands its rightmost groups (of the left side,
    /// then the right), one at a time, to the other; they keep their side and order.
    fn spill(&self, home: &[Row], collapsed: &[Group]) -> Option<Vec<Row>> {
        if self.all_fit(home, collapsed) {
            return Some(home.to_vec());
        }
        let over: Vec<bool> = home.iter().map(|r| !self.row_fits(r, collapsed)).collect();
        if over[0] == over[1] {
            return None;
        }
        let from = usize::from(over[1]);
        let ids: Vec<Group> = home[from].concat();
        (1..ids.len()).find_map(|n| {
            let tail = &ids[ids.len() - n..];
            let mut rows = without(home, tail);
            for side in 0..2 {
                let mine = home[from][side].iter().copied().filter(|g| tail.contains(g));
                // They arrive at the inner end of the destination side.
                let list = &mut rows[1 - from][side];
                let at = if side == 0 { list.len() } else { 0 };
                list.splice(at..at, mine);
            }
            self.all_fit(&rows, collapsed).then_some(rows)
        })
    }

    /// From widest to narrowest: one row, the second row's right side joined to the first, the saved rows.
    fn arrange(&self, home: &[Row], collapsed: &[Group]) -> Option<Vec<Row>> {
        let [[al, ar], [bl, br]] = [&home[0], &home[1]];
        let joined_right = [ar.as_slice(), br].concat();
        let one = vec![[[al.as_slice(), bl].concat(), joined_right.clone()]];
        let two = vec![[al.clone(), joined_right], [bl.clone(), Vec::new()]];
        [one, two].into_iter().find(|rows| self.all_fit(rows, collapsed)).or_else(|| self.spill(home, collapsed))
    }

    /// Rows are rearranged before anything collapses; once something must, nothing moves back up.
    fn try_solve(&self, home: &[Row], closed: &[Group], expanded: &[Group]) -> Option<(Vec<Row>, Vec<Group>)> {
        let mut collapsed = closed.to_vec();
        let mut rows = self.arrange(home, &collapsed);
        let mut auto = Vec::new();
        let order = Group::COLLAPSIBLE.iter().filter(|g| !expanded.contains(g)).chain(expanded);
        for &g in order {
            if rows.is_some() {
                break;
            }
            if !collapsed.contains(&g) {
                collapsed.push(g);
                auto.push(g);
                rows = self.spill(home, &collapsed);
            }
        }
        let mut rows = rows?;
        for g in auto.into_iter().rev() {
            let without_g: Vec<Group> = collapsed.iter().copied().filter(|c| *c != g).collect();
            if let Some(open) = self.spill(home, &without_g) {
                (rows, collapsed) = (open, without_g);
            }
        }
        Some((rows, collapsed))
    }

    /// Splits each row into as many lines as it needs, so nothing is ever clipped.
    fn wrap(&self, rows: &[Row], collapsed: &[Group]) -> Vec<Line> {
        let mut lines = Vec::new();
        for (saved, row) in rows.iter().enumerate() {
            let mut line = Line { sides: Row::default(), saved };
            for (side, groups) in row.iter().enumerate() {
                for &g in groups {
                    let held = line.sides.iter().flatten().chain([&g]);
                    if self.span(held, collapsed) + 2.0 * MARGIN > self.window && line.sides.iter().any(|s| !s.is_empty()) {
                        lines.push(std::mem::replace(&mut line, Line { sides: Row::default(), saved }));
                    }
                    line.sides[side].push(g);
                }
            }
            lines.push(line);
        }
        lines
    }

    pub fn solve(&self, state: &ToolbarState) -> Solved {
        let (mut lines, collapsed) = match self.try_solve(&state.rows, &state.closed, &state.expanded) {
            Some((rows, collapsed)) => (rows.into_iter().enumerate().map(|(saved, sides)| Line { sides, saved }).collect(), collapsed),
            None => (self.wrap(&state.rows, &Group::COLLAPSIBLE), Group::COLLAPSIBLE.to_vec()),
        };
        lines.retain(|l| l.sides.iter().any(|s| !s.is_empty()));
        Solved { lines, collapsed }
    }

    /// While rearranging: the saved two rows (empty ones included), collapsing what they need to.
    pub fn editing(&self, state: &ToolbarState) -> Solved {
        let mut collapsed = state.closed.clone();
        for g in Group::COLLAPSIBLE {
            if !self.all_fit(&state.rows, &collapsed) && !collapsed.contains(&g) {
                collapsed.push(g);
            }
        }
        Solved { lines: self.wrap(&state.rows, &collapsed), collapsed }
    }

    /// Whether `rows` can be shown without wrapping with every other group collapsed, `open` expanded.
    pub fn fits(&self, rows: &[Row], open: Option<Group>) -> bool {
        let closed: Vec<Group> = Group::COLLAPSIBLE.into_iter().filter(|g| Some(*g) != open).collect();
        self.try_solve(rows, &closed, &[]).is_some_and(|(_, collapsed)| open.is_none_or(|g| !collapsed.contains(&g)))
    }

    /// The left edge of each group of a line, left side first.
    pub fn place(&self, sides: &Row, collapsed: &[Group]) -> Vec<(Group, f32)> {
        let mut placed = Vec::new();
        let starts = [MARGIN, self.window - MARGIN - self.span(&sides[1], collapsed)];
        for (groups, mut x) in sides.iter().zip(starts) {
            for &g in groups {
                placed.push((g, x));
                x += (self.width)(g, collapsed.contains(&g)) + GAP;
            }
        }
        placed
    }

    /// Where dropping `group` at `x` on line `line` of the editing view would put it.
    pub fn drop_target(&self, state: &ToolbarState, group: Group, line: usize, x: f32) -> Option<DropTarget> {
        let view = self.editing(state);
        let line_index = line.min(view.lines.len().checked_sub(1)?);
        let line = &view.lines[line_index];
        let mut sides = line.sides.clone();
        sides.iter_mut().for_each(|s| s.retain(|g| *g != group));
        let placed = self.place(&sides, &view.collapsed);
        let edge = |g: Group| {
            let left = placed.iter().find(|p| p.0 == g).map_or(0.0, |p| p.1);
            (left, left + (self.width)(g, view.collapsed.contains(&g)))
        };
        let left_end = sides[0].last().map_or(MARGIN, |&g| edge(g).1);
        let right_start = sides[1].first().map_or(self.window - MARGIN, |&g| edge(g).0);
        let side = usize::from(x >= (left_end + right_start) / 2.0);
        let before = sides[side].iter().position(|&g| {
            let (a, b) = edge(g);
            (a + b) / 2.0 >= x
        });
        let saved = without(&state.rows, &[group]);
        let saved_side = &saved[line.saved][side];
        let position = |g: &Group| saved_side.iter().position(|s| s == g);
        let (index, marker_x) = match (before, sides[side].last()) {
            (Some(i), _) => (position(&sides[side][i]).unwrap_or(0), edge(sides[side][i]).0 - GAP / 2.0 - 1.0),
            (None, Some(last)) => (position(last).map_or(saved_side.len(), |i| i + 1), edge(*last).1 + GAP / 2.0 - 1.0),
            (None, None) => (if side == 0 { saved_side.len() } else { 0 }, if side == 0 { MARGIN - 5.0 } else { self.window - MARGIN + 3.0 }),
        };
        let rows = moved(&state.rows, group, line.saved, side, index);
        let ok = self.fits(&rows, None);
        Some(DropTarget { rows, ok, line: line_index, marker_x })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DropTarget {
    pub rows: Vec<Row>,
    pub ok: bool,
    pub line: usize,
    pub marker_x: f32,
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/ui/toolbar_layout.rs"]
mod tests;
