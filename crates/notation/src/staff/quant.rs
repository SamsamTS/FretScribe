//! Snaps note edges to ticks. Every quarter-note window of a measure picks, among the divisions of
//! the grid, the one that fits its edges best (straight wins ties), so three evenly spaced notes
//! become a triplet and four become sixteenths.

use std::collections::HashMap;

use fretscribe_core::{Note, SnapGrid, TempoMap};

use super::value::TICKS_PER_BEAT;

const EPS: f64 = 1e-9;

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
}

struct Raw<'a> {
    note: &'a Note,
    s: f64,
    e: f64,
}

fn divisions(grid: &SnapGrid) -> Vec<u8> {
    let mut d: Vec<u8> = grid.divisions.iter().copied().filter(|&d| d > 0 && TICKS_PER_BEAT % i64::from(d) == 0).collect();
    d.sort_by_key(|&d| (!d.is_power_of_two(), d));
    d.dedup();
    if d.is_empty() {
        d.push(4);
    }
    d
}

/// Silences shorter than half the finest grid step are absorbed into the previous note.
fn absorb_gaps(raws: &mut [Raw], unit: f64) {
    let spans: Vec<(f64, f64)> = raws.iter().map(|r| (r.s, r.e)).collect();
    for r in raws.iter_mut() {
        let sounding = spans.iter().any(|&(s, e)| s <= r.e + EPS && e > r.e + EPS);
        let next = spans.iter().map(|&(s, _)| s).filter(|&s| s > r.e + EPS).fold(f64::INFINITY, f64::min);
        if !sounding && next - r.e < unit / 2.0 {
            r.e = next;
        }
    }
}

pub fn quantise(notes: &[Note], tempo: &TempoMap, grid: &SnapGrid) -> Quantised {
    let divs = divisions(grid);
    let mut bounds = vec![0];
    for i in 0..tempo.len() {
        bounds.push(bounds[i] + (tempo.beats_in_measure(i) * TICKS_PER_BEAT as f64).round() as i64);
    }
    let total = *bounds.last().unwrap_or(&0);

    let mut raws: Vec<Raw> = notes
        .iter()
        .filter(|n| n.grace_of.is_none())
        .filter_map(|n| {
            let (s, e) = (tempo.pos_to_beats(n.start)?, tempo.pos_to_beats(n.end)?);
            (s > -EPS).then_some(Raw { note: n, s, e: e.max(s) })
        })
        .collect();
    let finest = f64::from(divs.iter().copied().max().unwrap_or(4));
    absorb_gaps(&mut raws, 1.0 / finest);

    let at = |abs: f64| {
        let pos = tempo.beats_to_pos(abs);
        (tempo.index_of(pos.measure).unwrap_or(0), pos.beat)
    };
    let mut edges: HashMap<(usize, usize), Vec<f64>> = HashMap::new();
    for r in &raws {
        for abs in [r.s, r.e] {
            let (i, beat) = at(abs);
            edges.entry((i, beat.max(0.0).floor() as usize)).or_default().push(beat);
        }
    }
    let error = |beats: &[f64], d: u8| beats.iter().map(|b| (b - (b * f64::from(d)).round() / f64::from(d)).abs()).sum::<f64>();
    let chosen: HashMap<(usize, usize), u8> = edges
        .iter()
        .map(|(&key, beats)| {
            let best = divs.iter().fold((f64::INFINITY, divs[0]), |acc, &d| {
                let err = error(beats, d);
                if err < acc.0 - EPS { (err, d) } else { acc }
            });
            (key, best.1)
        })
        .collect();

    let division = |i: usize, beat: f64| chosen.get(&(i, beat.max(0.0).floor() as usize)).copied().unwrap_or(divs[0]);
    let tick = |i: usize, beat: f64| {
        let d = division(i, beat);
        (bounds[i] + (beat * f64::from(d)).round() as i64 * (TICKS_PER_BEAT / i64::from(d))).min(total)
    };

    let mut triplet: Vec<Vec<bool>> = (0..tempo.len()).map(|i| vec![false; tempo.beats_in_measure(i).ceil() as usize]).collect();
    for (&(i, k), &d) in &chosen {
        if let Some(w) = triplet[i].get_mut(k) {
            *w = d % 3 == 0;
        }
    }

    let times = fretscribe_core::NoteTimes::build(notes, tempo);
    let mut out: Vec<QNote> = raws
        .iter()
        .filter_map(|r| {
            let (si, sb) = at(r.s);
            let (ei, eb) = at(r.e);
            let s = tick(si, sb);
            let e = tick(ei, eb);
            let e = if e <= s { s + TICKS_PER_BEAT / i64::from(division(si, sb)) } else { e };
            (s < total).then(|| QNote { id: r.note.id, midi: r.note.midi, s, e: e.min(total), actual_sec: times.start_sec(r.note.id).unwrap_or(0.0) })
        })
        .collect();
    out.sort_by_key(|n| (n.s, n.midi, n.id));
    Quantised { bounds, triplet, notes: out }
}
