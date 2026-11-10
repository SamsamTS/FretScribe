//! Selection shared by the spectrogram, staff and tab views.

use fretscribe_core::{Note, NoteTimes, TempoMap};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub ids: Vec<u64>,
    /// Last note clicked without Shift, the start of a Shift range.
    pub anchor: Option<u64>,
}

/// Note ids in time order, lowest pitch first among simultaneous notes.
pub fn time_order(notes: &[Note], tempo: &TempoMap) -> Vec<u64> {
    let times = NoteTimes::build(notes, tempo);
    let mut order: Vec<&Note> = notes.iter().collect();
    order.sort_by(|a, b| {
        let start = |n: &Note| times.start_sec(n.id).unwrap_or(0.0);
        start(a).total_cmp(&start(b)).then(a.midi.cmp(&b.midi)).then(a.id.cmp(&b.id))
    });
    order.into_iter().map(|n| n.id).collect()
}

/// Notes whose time span and pitch row touch the rectangle.
pub fn marquee_hits(notes: &[Note], times: &NoteTimes, secs: (f64, f64), midis: (f32, f32)) -> Vec<u64> {
    let (t0, t1, m0, m1) = (secs.0.min(secs.1), secs.0.max(secs.1), midis.0.min(midis.1), midis.0.max(midis.1));
    notes
        .iter()
        .filter(|n| {
            let (Some(s), Some(e)) = (times.start_sec(n.id), times.end_sec(n.id)) else { return false };
            let pitch = f32::from(n.midi);
            s <= t1 && e >= t0 && pitch + 0.5 >= m0 && pitch - 0.5 <= m1
        })
        .map(|n| n.id)
        .collect()
}

impl Selection {
    pub fn contains(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn set(&mut self, ids: Vec<u64>) {
        self.anchor = ids.last().copied();
        self.ids = ids;
    }

    fn add(&mut self, id: u64) {
        if !self.ids.contains(&id) {
            self.ids.push(id);
        }
    }

    /// Plain click selects only the note, Ctrl toggles it, Shift adds every note between the anchor
    /// and the clicked one in time order (`order`).
    pub fn click(&mut self, id: u64, ctrl: bool, shift: bool, order: &[u64]) {
        let position = |n: u64| order.iter().position(|&o| o == n);
        if shift {
            if let (Some(a), Some(b)) = (self.anchor.and_then(position), position(id)) {
                for &n in &order[a.min(b)..=a.max(b)] {
                    self.add(n);
                }
                return;
            }
        }
        if ctrl {
            if let Some(at) = self.ids.iter().position(|&n| n == id) {
                self.ids.remove(at);
            } else {
                self.ids.push(id);
            }
        } else {
            self.ids = vec![id];
        }
        self.anchor = Some(id);
    }

    pub fn marquee(&mut self, hits: &[u64], add: bool) {
        if !add {
            self.ids.clear();
        }
        for &id in hits {
            self.add(id);
        }
        self.anchor = self.ids.last().copied();
    }

    pub fn select_all(&mut self, order: &[u64]) {
        self.ids = order.to_vec();
        self.anchor = order.first().copied();
    }

    /// Moves to the next or previous note in time order from the anchor; `extend` keeps the rest.
    pub fn step(&mut self, forward: bool, extend: bool, order: &[u64]) {
        let from = self.anchor.or_else(|| self.ids.last().copied()).and_then(|a| order.iter().position(|&o| o == a));
        let target = match (from, forward) {
            (Some(i), true) => order.get(i + 1),
            (Some(i), false) => i.checked_sub(1).and_then(|j| order.get(j)),
            (None, true) => order.first(),
            (None, false) => order.last(),
        };
        let Some(&id) = target else { return };
        if !extend {
            self.ids.clear();
        }
        self.add(id);
        self.anchor = Some(id);
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;

    fn setup() -> (fretscribe_core::Project, [u64; 4]) {
        let mut p = project();
        let ids = [add(&mut p, 60, 0.0, 1.0), add(&mut p, 72, 1.0, 2.0), add(&mut p, 48, 2.0, 3.0), add(&mut p, 55, 1.5, 2.5)];
        (p, ids)
    }

    #[test]
    fn time_order_ignores_pitch_across_time_and_sorts_chords_low_to_high() {
        let (p, [a, b, c, d]) = setup();
        let tempo = TempoMap::new(&p.measures);
        assert_eq!(time_order(&p.notes, &tempo), [a, b, d, c]);
    }

    #[test]
    fn click_ctrl_and_shift_range() {
        let (p, [a, b, c, d]) = setup();
        let order = time_order(&p.notes, &TempoMap::new(&p.measures));
        let mut s = Selection::default();
        s.click(a, false, false, &order);
        assert_eq!(s.ids, [a]);
        s.click(c, true, false, &order);
        assert_eq!(s.ids, [a, c]);
        s.click(a, true, false, &order);
        assert_eq!(s.ids, [c]);
        s.click(b, false, false, &order);
        s.click(c, false, true, &order);
        assert_eq!(s.ids, [b, d, c]);
        s.click(d, false, false, &order);
        assert_eq!(s.ids, [d]);
    }

    #[test]
    fn shift_range_runs_backwards_too_and_adds_to_the_selection() {
        let (p, [a, b, c, d]) = setup();
        let order = time_order(&p.notes, &TempoMap::new(&p.measures));
        let mut s = Selection::default();
        s.click(c, false, false, &order);
        s.click(a, false, true, &order);
        assert_eq!(s.ids.len(), 4);
        assert!([a, b, c, d].iter().all(|&n| s.contains(n)));
    }

    #[test]
    fn marquee_touches_and_ctrl_adds() {
        let (p, [a, b, _, d]) = setup();
        let tempo = TempoMap::new(&p.measures);
        let times = NoteTimes::build(&p.notes, &tempo);
        let hits = marquee_hits(&p.notes, &times, (0.3, 0.45), (59.6, 60.4));
        assert_eq!(hits, [a]);
        let mut s = Selection::default();
        s.click(b, false, false, &[]);
        s.marquee(&hits, true);
        assert_eq!(s.ids, [b, a]);
        s.marquee(&marquee_hits(&p.notes, &times, (0.0, 4.0), (54.0, 56.0)), false);
        assert_eq!(s.ids, [d]);
    }

    #[test]
    fn step_and_select_all() {
        let (p, [a, b, c, d]) = setup();
        let order = time_order(&p.notes, &TempoMap::new(&p.measures));
        let mut s = Selection::default();
        s.step(true, false, &order);
        assert_eq!(s.ids, [a]);
        s.step(true, true, &order);
        assert_eq!(s.ids, [a, b]);
        s.step(false, false, &order);
        assert_eq!(s.ids, [a]);
        s.step(false, false, &order);
        assert_eq!(s.ids, [a]);
        s.select_all(&order);
        assert_eq!(s.ids, [a, b, d, c]);
    }
}
