//! Let ring is derived data: where a note keeps sounding, never stored (like rests).

use std::collections::HashMap;

use crate::model::{Project, Technique, TimePos, TICKS_PER_BEAT};
use crate::tempo::TempoMap;

/// Ring end of every note that starts in a let-ring measure, is not grace, dead or palm-muted, has a string
/// and ends its chain: the earlier of the barline (the nominal end for the last measure) and the next note on
/// the same string, never before the note's own end. Notated durations are not changed.
pub fn ring_ends(project: &Project) -> HashMap<u64, TimePos> {
    let map = TempoMap::new(&project.measures);
    let sec = |pos| map.pos_to_sec(pos);
    let mut ends = HashMap::new();
    for n in &project.notes {
        let Some((string, _)) = n.string_fret else { continue };
        let Some(index) = map.index_of(n.start.measure).filter(|&i| project.measures[i].let_ring) else { continue };
        if n.grace_of.is_some() || matches!(n.technique, Some(Technique::Dead | Technique::PalmMute)) || project.outgoing(n.id).is_some() {
            continue;
        }
        let barline = match project.measures.get(index + 1) {
            Some(next) => TimePos::new(next.id, 0),
            None => TimePos::new(project.measures[index].id, (map.beats_in_measure(index) * TICKS_PER_BEAT as f64).round() as i64),
        };
        let next_start = project.notes.iter().filter(|o| o.string_fret.is_some_and(|s| s.0 == string) && sec(o.start) > sec(n.start)).map(|o| o.start).min_by(|a, b| sec(*a).total_cmp(&sec(*b)));
        let ring = [Some(barline), next_start].into_iter().flatten().min_by(|a, b| sec(*a).total_cmp(&sec(*b))).unwrap_or(barline);
        ends.insert(n.id, if sec(ring) < sec(n.end) { n.end } else { ring });
    }
    ends
}

#[cfg(all(test, dev_tests))]
#[path = "../../../dev/tests/unit/core/ring.rs"]
mod tests;
