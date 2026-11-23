//! Turns notes into ticks. Positions are whole ticks already, so a note keeps its exact start and
//! end; only the splitter decides how a span is written (as tied values when no single value fits).
//! Edges closer than the shortest value (a 64th triplet) are merged, a quarter-note window becomes a
//! triplet when more of its edges sit on the triplet lattice only than on the straight one only,
//! and notes joined by a tie are one note.

use std::collections::{BTreeSet, HashMap, HashSet};

use fretscribe_core::{Note, Technique, TempoMap};

use super::value::TICKS_PER_BEAT;

/// Shortest value: a 64th triplet. Edges closer than this are one edge.
const MIN_TICKS: i64 = 2;

pub(crate) struct QNote {
    pub id: u64,
    pub midi: u8,
    pub s: i64,
    pub e: i64,
    pub actual_sec: f64,
}

pub(crate) struct Quantised {
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
    s: i64,
    e: i64,
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
        if let (Some(s), Some(e)) = (tempo.pos_to_ticks(n.start), tempo.pos_to_ticks(last.end)) {
            if s >= 0 {
                raws.push(Raw { note: n, s, e: e.max(s) });
            }
        }
    }
    (raws, aliases)
}

/// Maps every edge to the edge it is merged into: an edge within `MIN_TICKS` of the previous kept one joins it,
/// except that a measure bound always stays.
fn merged_edges(edges: &BTreeSet<i64>, bounds: &[i64]) -> HashMap<i64, i64> {
    let is_bound = |x: i64| bounds.binary_search(&x).is_ok();
    let (mut map, mut last): (HashMap<i64, i64>, Option<i64>) = (HashMap::new(), None);
    for &x in edges {
        match last {
            Some(l) if x - l < MIN_TICKS && !is_bound(l) => {
                let keep = if is_bound(x) { x } else { l };
                map.values_mut().filter(|v| **v == l).for_each(|v| *v = keep);
                map.insert(l, keep);
                map.insert(x, keep);
                last = Some(keep);
            }
            Some(l) if x - l < MIN_TICKS => {
                map.insert(x, l);
            }
            _ => {
                map.insert(x, x);
                last = Some(x);
            }
        }
    }
    map
}

pub(crate) fn quantise(notes: &[Note], tempo: &TempoMap) -> Quantised {
    let mut bounds = vec![0];
    for i in 0..tempo.len() {
        bounds.push(bounds[i] + tempo.measure_ticks(i));
    }
    let total = *bounds.last().unwrap_or(&0);

    let (raws, aliases) = merge_ties(notes, tempo);
    let edges: BTreeSet<i64> = raws.iter().flat_map(|r| [r.s, r.e]).chain(bounds.iter().copied()).collect();
    let merged = merged_edges(&edges, &bounds);

    // A window is a triplet when more edges inside it sit on the triplet lattice only (2 ticks, not 3) than on the straight one only.
    let measure_of = |tick: i64| bounds[..tempo.len()].partition_point(|&b| b <= tick).saturating_sub(1);
    let mut votes: HashMap<(usize, usize), (u32, u32)> = HashMap::new();
    for &x in merged.values().collect::<BTreeSet<_>>() {
        let (m, within) = (measure_of(x), (x - bounds[measure_of(x)]) % TICKS_PER_BEAT);
        let (straight, triplet) = (within % 3 == 0 && within % 2 != 0, within % 2 == 0 && within % 3 != 0);
        if x < total && (straight || triplet) {
            let v = votes.entry((m, ((x - bounds[m]) / TICKS_PER_BEAT) as usize)).or_default();
            if triplet { v.1 += 1 } else { v.0 += 1 }
        }
    }
    let mut triplet: Vec<Vec<bool>> = (0..tempo.len()).map(|i| vec![false; tempo.beats_in_measure(i).ceil() as usize]).collect();
    for (&(i, k), &(straight, trip)) in &votes {
        if let Some(w) = triplet[i].get_mut(k) {
            *w = trip > straight;
        }
    }

    let times = fretscribe_core::NoteTimes::build(notes, tempo);
    let mut out: Vec<QNote> = raws
        .iter()
        .filter_map(|r| {
            let (s, e) = (merged[&r.s], merged[&r.e]);
            (s < total).then(|| QNote { id: r.note.id, midi: r.note.midi, s, e: e.max(s + MIN_TICKS).min(total), actual_sec: times.start_sec(r.note.id).unwrap_or(0.0) })
        })
        .collect();
    out.sort_by_key(|n| (n.s, n.midi, n.id));
    Quantised { bounds, triplet, notes: out, aliases }
}
