//! Turns notes into ticks. How long a note was played decides its value (nearest of every
//! straight, dotted, double-dotted and triplet value up to the 64th), so the grid in use never
//! changes the result. Starts snap to the coarsest straight or triplet lattice that fits them, and
//! a quarter-note window becomes a triplet when its notes say so. Notes joined by a tie are one note.

use std::collections::{HashMap, HashSet};

use fretscribe_core::{Note, Technique, TempoMap};

use super::value::{nearest, TICKS_PER_BEAT};

const EPS: f64 = 1e-9;
/// A finer lattice must be better than a coarser one by more than this many beats per edge.
/// Largest gap (beats) between a note's end and another start for the two to count as touching.
const TOUCH: f64 = 0.1;
const TOLERANCE: f64 = 0.01;
/// Silences shorter than half of this many beats are absorbed into the previous note.
const GAP_UNIT: f64 = 0.25;
const STRAIGHT: [u8; 5] = [1, 2, 4, 8, 16];
const TRIPLET: [u8; 4] = [3, 6, 12, 24];
/// Shortest note: a 64th.
const MIN_TICKS: i64 = TICKS_PER_BEAT / 16;

pub struct QNote {
    pub id: u64,
    pub midi: u8,
    pub s: i64,
    pub e: i64,
    pub actual_sec: f64,
}

pub struct Quantised {
    /// Start tick of every measure, then the end of the last one.
    pub bounds: Vec<i64>,
    /// Per measure and quarter-note window: divided in triplets.
    pub triplet: Vec<Vec<bool>>,
    pub notes: Vec<QNote>,
    /// `(tied follower, first note of its tie chain)`: the follower is engraved as part of the first.
    pub aliases: Vec<(u64, u64)>,
}

struct Raw<'a> {
    note: &'a Note,
    s: f64,
    e: f64,
}

/// Silences shorter than half a gap unit are absorbed into the previous note.
fn absorb_gaps(raws: &mut [Raw]) {
    let spans: Vec<(f64, f64)> = raws.iter().map(|r| (r.s, r.e)).collect();
    for r in raws.iter_mut() {
        let sounding = spans.iter().any(|&(s, e)| s <= r.e + EPS && e > r.e + EPS);
        let next = spans.iter().map(|&(s, _)| s).filter(|&s| s > r.e + EPS).fold(f64::INFINITY, f64::min);
        if !sounding && next - r.e < GAP_UNIT / 2.0 {
            r.e = next;
        }
    }
}

fn lattice_error(beat: f64, d: u8) -> f64 {
    (beat - (beat * f64::from(d)).round() / f64::from(d)).abs()
}

/// Coarsest division of `list` (coarsest first) within the tolerance of the best one, with its error.
fn coarsest_fit(beat: f64, list: &[u8]) -> (u8, f64) {
    let best = list.iter().map(|&d| lattice_error(beat, d)).fold(f64::INFINITY, f64::min);
    let d = list.iter().copied().find(|&d| lattice_error(beat, d) <= best + TOLERANCE).unwrap_or(list[0]);
    (d, lattice_error(beat, d))
}

/// Merges tie chains: the first note carries the whole length, followers are returned as aliases.
fn merge_ties<'a>(notes: &'a [Note], tempo: &TempoMap) -> (Vec<Raw<'a>>, Vec<(u64, u64)>) {
    let by_id: HashMap<u64, &Note> = notes.iter().map(|n| (n.id, n)).collect();
    let next_of = |n: &Note| n.link_to.filter(|_| n.technique == Some(Technique::Tie)).and_then(|to| by_id.get(&to).copied()).filter(|t| t.grace_of.is_none());
    let followers: HashSet<u64> = notes.iter().filter_map(|n| next_of(n).map(|t| t.id)).collect();
    let (mut raws, mut aliases) = (Vec::new(), Vec::new());
    for n in notes.iter().filter(|n| n.grace_of.is_none() && !followers.contains(&n.id)) {
        let mut last = n;
        for _ in 0..notes.len() {
            let Some(next) = next_of(last) else { break };
            aliases.push((next.id, n.id));
            last = next;
        }
        if let (Some(s), Some(e)) = (tempo.pos_to_beats(n.start), tempo.pos_to_beats(last.end)) {
            if s > -EPS {
                raws.push(Raw { note: n, s, e: e.max(s) });
            }
        }
    }
    (raws, aliases)
}

pub fn quantise(notes: &[Note], tempo: &TempoMap) -> Quantised {
    let mut bounds = vec![0];
    for i in 0..tempo.len() {
        bounds.push(bounds[i] + (tempo.beats_in_measure(i) * TICKS_PER_BEAT as f64).round() as i64);
    }
    let total = *bounds.last().unwrap_or(&0);

    let (mut raws, aliases) = merge_ties(notes, tempo);
    absorb_gaps(&mut raws);

    let at = |abs: f64| {
        let pos = tempo.beats_to_pos(abs);
        (tempo.index_of(pos.measure).unwrap_or(0), pos.beat)
    };
    // A length within 12% of one value (3% of a triplet one, which sit close to the others) is that value; anything else (2.5 beats) is a span the
    // splitter writes as tied values, its end snapped like a start.
    let value_of = |r: &Raw| {
        let ticks = (r.e - r.s) * TICKS_PER_BEAT as f64;
        nearest(ticks).filter(|v| (v.ticks() as f64 - ticks).abs() <= (if v.triplet { 0.03 } else { 0.12 } * v.ticks() as f64).max(0.02 * TICKS_PER_BEAT as f64))
    };

    // A window is a triplet when more of the notes starting in it fit a triplet lattice (or are
    // triplet values) than a straight one; notes on the beat itself fit both and do not vote.
    let mut votes: HashMap<(usize, usize), (u32, u32)> = HashMap::new();
    for r in &raws {
        let (i, beat) = at(r.s);
        let key = (i, beat.max(0.0).floor() as usize);
        let frac = beat - beat.floor();
        let triplet_value = value_of(r).is_some_and(|v| v.triplet);
        let vote = if triplet_value {
            Some(true)
        } else if frac > EPS && 1.0 - frac > EPS {
            let (straight, triplet) = (coarsest_fit(frac, &STRAIGHT).1, coarsest_fit(frac, &TRIPLET).1);
            Some(triplet + TOLERANCE < straight)
        } else {
            None
        };
        if let Some(triplet) = vote {
            let v = votes.entry(key).or_default();
            if triplet { v.1 += 1 } else { v.0 += 1 }
        }
    }
    let mut triplet: Vec<Vec<bool>> = (0..tempo.len()).map(|i| vec![false; tempo.beats_in_measure(i).ceil() as usize]).collect();
    for (&(i, k), &(straight, trip)) in &votes {
        if let Some(w) = triplet[i].get_mut(k) {
            *w = trip > straight;
        }
    }

    let tick = |abs: f64| {
        let (i, beat) = at(abs);
        let window = beat.max(0.0).floor() as usize;
        let list: &[u8] = if triplet[i].get(window).copied().unwrap_or(false) { &TRIPLET } else { &STRAIGHT };
        let (d, _) = coarsest_fit(beat, list);
        (bounds[i] + (beat * f64::from(d)).round() as i64 * (TICKS_PER_BEAT / i64::from(d))).min(total)
    };

    let times = fretscribe_core::NoteTimes::build(notes, tempo);
    let mut out: Vec<QNote> = raws
        .iter()
        .filter_map(|r| {
            let s = tick(r.s);
            // An end touching another note's start is that start's tick (no rest or overlap); otherwise the length picks the value.
            let next = raws.iter().filter(|o| o.s > r.s + EPS && (o.s - r.e).abs() <= TOUCH).min_by(|a, b| (a.s - r.e).abs().total_cmp(&(b.s - r.e).abs()));
            let e = next.map_or_else(|| value_of(r).map_or_else(|| tick(r.e), |v| s + v.ticks()), |o| tick(o.s)).max(s + MIN_TICKS);
            (s < total).then(|| QNote { id: r.note.id, midi: r.note.midi, s, e: e.min(total), actual_sec: times.start_sec(r.note.id).unwrap_or(0.0) })
        })
        .collect();
    out.sort_by_key(|n| (n.s, n.midi, n.id));
    Quantised { bounds, triplet, notes: out, aliases }
}
