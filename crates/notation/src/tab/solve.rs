//! Viterbi over "items" (simultaneous notes, plus linked groups solved as one unit).
//!
//! Notes joined by slide, bend or legato links form a group that sits on one string, so a group's
//! candidates are just its strings: every fret follows from the pitches (a bent note takes the fret
//! of the lowest note of its bend class). Items cluster groups starting within `CLUSTER_SEC` and
//! grace notes with their main note. Combos of an item are enumerated by backtracking, then
//! transitions between items are scored. Constraints that cannot always hold (distinct strings,
//! skipping a note) are tried hard first and relaxed only when no combo exists.
//!
//! The hand is modelled as a window of `max_hand_span` frets: playing inside it is free, moving it costs
//! `movement_weight` per fret (leaps beyond a couple of frets cost more), and crossing strings costs
//! `string_jump_weight` per string. So a phrase is rather played across strings in one position than slid
//! along one string. The window is part of the Viterbi state, so the path that moves it pays for it.

use std::collections::HashMap;

use fretscribe_core::{Note, NoteTimes, Technique, Tuning};

use super::TabOptions;

type Slot = Option<(u8, u8)>;

const CLUSTER_SEC: f64 = 0.040;
const OVERLAP_EPS: f64 = 1e-6;
const BEND_MAX_SEMITONES: u8 = 4;
/// Cost per fret of an open string being further from the hand than the first position.


const OPEN_FREE: u8 = 2;
const OPEN_REACH: f32 = 0.75;
/// Cost per fret of playing above the low edge of the hand window, which keeps the hand low in its position.
const DRIFT: f32 = 0.3;
/// Hand moves beyond `NEAR_SHIFT` frets cost `1 + FAR_FACTOR` times as much per fret.
const NEAR_SHIFT: u8 = 2;
const FAR_FACTOR: f32 = 2.0;
const SKIP_PENALTY: f32 = 1e6;
const OVERLAP_PENALTY: f32 = 1e5;
const SPAN_PENALTY: f32 = 50.0;
const SEARCH_CAP: usize = 20_000;
const KEEP_COMBOS: usize = 120;

struct Ctx<'a> {
    tuning: &'a Tuning,
    capo: u8,
    opts: &'a TabOptions,
    spans: Vec<(f64, f64)>,
    family: Vec<u64>,
}

impl Ctx<'_> {
    fn overlap(&self, a: usize, b: usize) -> bool {
        self.family[a] != self.family[b]
            && self.spans[a].0 < self.spans[b].1 - OVERLAP_EPS
            && self.spans[b].0 < self.spans[a].1 - OVERLAP_EPS
    }

    fn fretted(&self, fret: u8) -> bool {
        fret > self.capo
    }

    fn position_cost(&self, (string, fret): (u8, u8)) -> f32 {
        let o = self.opts;
        let base = o.position_weight * f32::from(fret.saturating_sub(self.capo)) + 1e-4 * f32::from(string);
        if self.fretted(fret) { base } else { base - o.open_string_bonus }
    }
}

struct Group {
    members: Vec<usize>,
    options: Vec<Vec<Slot>>,
    locked: bool,
}

struct Item {
    groups: Vec<Group>,
    offsets: Vec<usize>,
    notes: Vec<usize>,
    conflicts: Vec<(usize, usize)>,
    /// Pairs of picked (not linked) grace notes and the notes they are best played apart from: their main
    /// note and the other graces of it.
    apart: Vec<(usize, usize)>,
}

struct Combo {
    slots: Vec<Slot>,
    /// Lowest and highest fretted fret of the combo.
    reach: Option<(u8, u8)>,
    cost: f32,
    /// Mean string of the first and last notes of each group.
    entry: Option<f32>,
    exit: Option<f32>,
}

fn is_locked(n: &Note) -> bool {
    n.locked_fret && n.string_fret.is_some()
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    parent[ra.max(rb)] = ra.min(rb);
}

struct Links {
    bends: Vec<(usize, usize)>,
    arrival: Vec<bool>,
    parent: Vec<usize>,
}

fn read_links(notes: &[Note]) -> Links {
    let index: HashMap<u64, usize> = notes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let mut links = Links { bends: Vec::new(), arrival: vec![false; notes.len()], parent: (0..notes.len()).collect() };
    for (a, n) in notes.iter().enumerate() {
        if let Some(&b) = n.link_to.and_then(|to| index.get(&to)) {
            union(&mut links.parent, a, b);
            if n.technique == Some(Technique::Bend) {
                links.bends.push((a, b));
            } else {
                links.arrival[b] = true;
            }
        }
        if let Some(&source) = n.legato.and_then(|l| index.get(&l.from)) {
            union(&mut links.parent, source, a);
            links.arrival[a] = true;
        }
    }
    links
}

/// Frets of `members` when the group sits on `string`, or `None` when it cannot be played there.
fn derive(notes: &[Note], links: &Links, ctx: &Ctx, members: &[usize], string: usize) -> Option<Vec<u8>> {
    let open = i16::from(ctx.tuning.open_strings[string]);
    let at = |i: usize| members.iter().position(|&m| m == i);
    let mut class: Vec<usize> = (0..members.len()).collect();
    for &(a, b) in &links.bends {
        if let (Some(pa), Some(pb)) = (at(a), at(b)) {
            if notes[a].midi.abs_diff(notes[b].midi) > BEND_MAX_SEMITONES {
                return None;
            }
            union(&mut class, pa, pb);
        }
    }
    let mut frets = Vec::with_capacity(members.len());
    for p in 0..members.len() {
        let root = find(&mut class, p);
        let mates: Vec<usize> = (0..members.len()).filter(|&q| find(&mut class, q) == root).collect();
        let own = i16::from(notes[members[p]].midi) - open;
        let fret = if mates.len() == 1 {
            own
        } else {
            let lowest = mates.iter().map(|&q| notes[members[q]].midi).min().map_or(own, |m| i16::from(m) - open);
            if links.arrival[members[p]] && own != lowest {
                return None;
            }
            lowest
        };
        frets.push(u8::try_from(fret).ok().filter(|f| (ctx.capo..=ctx.opts.max_fret).contains(f))?);
    }
    Some(frets)
}

fn build_groups(notes: &[Note], ctx: &Ctx) -> Vec<Group> {
    let mut links = read_links(notes);
    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..notes.len() {
        let root = find(&mut links.parent, i);
        by_root.entry(root).or_default().push(i);
    }
    let mut groups: Vec<Group> = by_root
        .into_values()
        .map(|mut members| {
            members.sort_by(|&a, &b| ctx.spans[a].0.total_cmp(&ctx.spans[b].0).then(notes[a].id.cmp(&notes[b].id)));
            let locked = members.iter().any(|&m| is_locked(&notes[m]));
            let mut options: Vec<Vec<Slot>> = (0..ctx.tuning.open_strings.len())
                .filter_map(|s| {
                    let frets = derive(notes, &links, ctx, &members, s)?;
                    Some(frets.into_iter().map(|f| Some((s as u8, f))).collect())
                })
                .collect();
            if locked {
                options.retain(|o| members.iter().zip(o).all(|(&m, slot)| !is_locked(&notes[m]) || *slot == notes[m].string_fret));
                if options.is_empty() {
                    let held = members.iter().map(|&m| is_locked(&notes[m]).then_some(notes[m].string_fret).flatten()).collect();
                    options.push(held);
                }
            }
            Group { members, options, locked }
        })
        .collect();
    groups.sort_by(|a, b| ctx.spans[a.members[0]].0.total_cmp(&ctx.spans[b.members[0]].0).then(notes[a.members[0]].id.cmp(&notes[b.members[0]].id)));
    groups
}

fn build_items(notes: &[Note], ctx: &Ctx) -> Vec<Item> {
    let mut items: Vec<(f64, Vec<Group>)> = Vec::new();
    for group in build_groups(notes, ctx) {
        let start = ctx.spans[group.members[0]].0;
        let joins = items.last().is_some_and(|(first, current)| {
            start - first <= CLUSTER_SEC
                || current.iter().flat_map(|g| &g.members).any(|&a| group.members.iter().any(|&b| ctx.family[a] == ctx.family[b]))
        });
        match items.last_mut() {
            Some((_, current)) if joins => current.push(group),
            _ => items.push((start, vec![group])),
        }
    }
    items.into_iter().map(|(_, groups)| finish_item(notes, ctx, groups)).collect()
}

fn finish_item(notes: &[Note], ctx: &Ctx, groups: Vec<Group>) -> Item {
    let mut offsets = Vec::new();
    let mut flat: Vec<usize> = Vec::new();
    let mut owner = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        offsets.push(flat.len());
        flat.extend(&group.members);
        owner.extend(std::iter::repeat_n(g, group.members.len()));
    }
    let mut conflicts = Vec::new();
    let mut apart: Vec<(usize, usize)> = Vec::new();
    let main_of = |b: usize| notes[flat[b]].grace_of.and_then(|g| flat.iter().position(|&i| notes[i].id == g.main));
    for b in 0..flat.len() {
        for a in 0..b {
            if owner[a] != owner[b] && ctx.overlap(flat[a], flat[b]) {
                conflicts.push((a, b));
            }
        }
        if let Some(main) = main_of(b) {
            apart.extend([(b, main)].into_iter().chain((0..b).filter(|&a| main_of(a) == Some(main)).map(|a| (a, b))).filter(|&(x, y)| owner[x] != owner[y]));
        }
    }
    Item { groups, offsets, notes: flat, conflicts, apart }
}

/// Mean string of `placed`.
fn summarize(placed: impl Iterator<Item = (u8, u8)>) -> Option<f32> {
    let strings: Vec<f32> = placed.map(|(string, _)| f32::from(string)).collect();
    (!strings.is_empty()).then(|| strings.iter().sum::<f32>() / strings.len() as f32)
}

fn ends(item: &Item, pick: impl Fn(&Group) -> usize) -> Vec<usize> {
    item.groups.iter().enumerate().map(|(g, group)| item.offsets[g] + pick(group)).collect()
}

struct Search<'a> {
    item: &'a Item,
    ctx: &'a Ctx<'a>,
    allow_skip: bool,
    hard: bool,
    slots: Vec<Slot>,
    found: Vec<(f32, Vec<Slot>)>,
    visited: usize,
}

impl Search<'_> {
    fn run(&mut self, g: usize, cost: f32) {
        if self.visited >= SEARCH_CAP {
            return;
        }
        let Some(group) = self.item.groups.get(g) else {
            self.visited += 1;
            let cost = cost - self.grace_bonus();
            self.found.push((cost, self.slots.clone()));
            return;
        };
        let base = self.item.offsets[g];
        let free_skip = group.options.is_empty();
        let skip = vec![None; group.members.len()];
        let skippable = free_skip || (self.allow_skip && !group.locked);
        let options = group.options.iter().chain(skippable.then_some(&skip));
        for option in options {
            let skipped = option.iter().all(Option::is_none) && !group.members.is_empty();
            self.slots[base..base + option.len()].copy_from_slice(option);
            let mut added = if skipped && !free_skip { SKIP_PENALTY } else { 0.0 };
            added += option.iter().flatten().map(|&s| self.ctx.position_cost(s)).sum::<f32>();
            if let Some(extra) = self.conflict_cost(base, base + option.len()) {
                self.run(g + 1, cost + added + extra);
            }
            self.slots[base..base + option.len()].fill(None);
        }
    }

    fn conflict_cost(&self, from: usize, to: usize) -> Option<f32> {
        let span = f32::from(self.ctx.opts.max_hand_span);
        let mut extra = 0.0;
        for &(a, b) in self.item.conflicts.iter().filter(|c| (from..to).contains(&c.1)) {
            let (Some((sa, fa)), Some((sb, fb))) = (self.slots[a], self.slots[b]) else { continue };
            if sa == sb {
                if self.hard {
                    return None;
                }
                extra += OVERLAP_PENALTY;
            }
            if self.ctx.fretted(fa) && self.ctx.fretted(fb) {
                extra += SPAN_PENALTY * (f32::from(fa.abs_diff(fb)) - span).max(0.0);
            }
        }
        Some(extra)
    }

    fn grace_bonus(&self) -> f32 {
        let spread = self.item.apart.iter().filter(|&&(a, b)| matches!((self.slots[a], self.slots[b]), (Some(x), Some(y)) if x.0 != y.0));
        self.ctx.opts.grace_bonus * spread.count() as f32
    }
}

fn combos(item: &Item, ctx: &Ctx) -> Vec<Combo> {
    let mut found = Vec::new();
    for (allow_skip, hard) in [(false, true), (true, true), (true, false)] {
        let mut search = Search { item, ctx, allow_skip, hard, slots: vec![None; item.notes.len()], found: Vec::new(), visited: 0 };
        search.run(0, 0.0);
        if !search.found.is_empty() {
            found = search.found;
            break;
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.truncate(KEEP_COMBOS);
    let (firsts, lasts) = (ends(item, |_| 0), ends(item, |g| g.members.len() - 1));
    found
        .into_iter()
        .map(|(cost, slots)| {
            let entry = summarize(firsts.iter().filter_map(|&i| slots[i]));
            let exit = summarize(lasts.iter().filter_map(|&i| slots[i]));
            let frets = slots.iter().flatten().map(|s| s.1).filter(|&f| ctx.fretted(f));
            let reach = frets.clone().min().zip(frets.max());
            Combo { slots, reach, cost, entry, exit }
        })
        .collect()
}

fn string_jump(ctx: &Ctx, from: Option<f32>, to: Option<f32>) -> f32 {
    let (Some(a), Some(b)) = (from, to) else { return 0.0 };
    ctx.opts.string_jump_weight * (a - b).abs()
}

/// The hand window (its lowest fret) after playing `combo`, and what moving it cost.
fn reposition(ctx: &Ctx, anchor: Option<u8>, combo: &Combo) -> (Option<u8>, f32) {
    let Some((lo, hi)) = combo.reach else {
        // Without a position yet, the hand rests at the nut.
        let a = anchor.unwrap_or(ctx.capo);
        return (Some(a), OPEN_REACH * f32::from(a.saturating_sub(ctx.capo + OPEN_FREE)));
    };
    let Some(a) = anchor else { return (Some(lo), 0.0) };
    let span = ctx.opts.max_hand_span;
    if lo >= a && hi <= a.saturating_add(span) {
        return (Some(a), DRIFT * f32::from(lo - a));
    }
    let next = if lo < a { lo } else { hi.saturating_sub(span).min(lo) };
    (Some(next), shift_cost(ctx, next.abs_diff(a)))
}

/// Cost of moving the hand window by `frets`: a step or two is cheap, a leap along the neck is not.
fn shift_cost(ctx: &Ctx, frets: u8) -> f32 {
    ctx.opts.movement_weight * (f32::from(frets) + FAR_FACTOR * f32::from(frets.saturating_sub(NEAR_SHIFT)))
}

fn same_string_overlaps(ctx: &Ctx, prev: (&Item, &Combo), next: (&Item, &Combo)) -> f32 {
    let mut count = 0.0;
    for (a, &na) in prev.0.notes.iter().enumerate() {
        for (b, &nb) in next.0.notes.iter().enumerate() {
            if let (Some(x), Some(y)) = (prev.1.slots[a], next.1.slots[b]) {
                if x.0 == y.0 && ctx.overlap(na, nb) {
                    count += 1.0;
                }
            }
        }
    }
    count * OVERLAP_PENALTY
}

/// Going from an open string to a high position takes the hand as far as an open string was far from it.
fn back_from_open(ctx: &Ctx, prev: &Combo, cur: &Combo) -> f32 {
    match (prev.reach, cur.reach) {
        (None, Some((lo, _))) => OPEN_REACH * f32::from(lo.saturating_sub(ctx.capo + OPEN_FREE)),
        _ => 0.0,
    }
}

/// One way to reach a combo: the hand window it leaves, the cost of the best path, and the state of the
/// previous item it came from.
struct State {
    anchor: Option<u8>,
    combo: usize,
    cost: f32,
    from: usize,
}

/// Best combo index per item: Viterbi over (combo, hand window) states, so the cost of having moved the hand
/// is paid by the path that moved it.
fn viterbi(ctx: &Ctx, items: &[&Item], options: &[Vec<Combo>], start: Option<f32>, start_anchor: Option<u8>) -> Vec<usize> {
    let first = options[0].iter().enumerate().map(|(combo, c)| {
        let (anchor, moved) = reposition(ctx, start_anchor, c);
        State { anchor, combo, cost: c.cost + string_jump(ctx, start, c.entry) + moved, from: 0 }
    });
    let mut layers: Vec<Vec<State>> = vec![first.collect()];
    for k in 1..items.len() {
        let mut next: Vec<State> = Vec::new();
        for (combo, cur) in options[k].iter().enumerate() {
            let first_of_combo = next.len();
            for (from, prev) in layers[k - 1].iter().enumerate() {
                let before = &options[k - 1][prev.combo];
                let (anchor, moved) = reposition(ctx, prev.anchor, cur);
                let step = string_jump(ctx, before.exit, cur.entry) + moved + back_from_open(ctx, before, cur) + same_string_overlaps(ctx, (items[k - 1], before), (items[k], cur));
                let cost = prev.cost + step + cur.cost;
                match next[first_of_combo..].iter_mut().find(|s| s.anchor == anchor) {
                    Some(s) if s.cost <= cost => {}
                    Some(s) => *s = State { anchor, combo, cost, from },
                    None => next.push(State { anchor, combo, cost, from }),
                }
            }
        }
        layers.push(next);
    }
    let mut at = layers.last().and_then(|l| l.iter().enumerate().min_by(|a, b| a.1.cost.total_cmp(&b.1.cost))).map_or(0, |(i, _)| i);
    let mut picks = Vec::with_capacity(items.len());
    for layer in layers.iter().rev() {
        let state = &layer[at];
        picks.push(state.combo);
        at = state.from;
    }
    picks.reverse();
    picks
}

enum Scope<'a> {
    All,
    From { edited: &'a [usize], backwards: bool },
}

struct Outcome {
    changed: Vec<u64>,
    unplaceable: Vec<u64>,
}

fn run(notes: &mut [Note], times: &NoteTimes, tuning: &Tuning, capo: u8, opts: &TabOptions, scope: Scope<'_>) -> Outcome {
    let mut outcome = Outcome { changed: Vec::new(), unplaceable: Vec::new() };
    if notes.is_empty() || tuning.open_strings.is_empty() {
        return outcome;
    }
    let ctx = Ctx {
        tuning,
        capo,
        opts,
        spans: notes.iter().map(|n| (times.start_sec(n.id).unwrap_or(0.0), times.end_sec(n.id).unwrap_or(0.0))).collect(),
        family: notes.iter().map(|n| n.grace_of.map_or(n.id, |g| g.main)).collect(),
    };
    let items = build_items(notes, &ctx);
    let locked_item = |i: usize| items[i].groups.iter().any(|g| g.locked);
    let windows = match scope {
        Scope::All => vec![(0, items.len() - 1)],
        Scope::From { edited, backwards } => {
            let mut item_of = vec![usize::MAX; notes.len()];
            for (k, it) in items.iter().enumerate() {
                for &i in &it.notes {
                    item_of[i] = k;
                }
            }
            let mut windows: Vec<(usize, usize)> = edited
                .iter()
                .filter_map(|&n| item_of.get(n).copied().filter(|&e| e != usize::MAX))
                .map(|e| {
                    let hi = (e + 1..items.len()).find(|&j| locked_item(j)).unwrap_or(items.len() - 1);
                    let lo = if backwards { (0..e).rev().find(|&k| locked_item(k)).map_or(0, |k| k + 1) } else { e };
                    (lo, hi.max(e))
                })
                .collect();
            windows.sort_unstable();
            let mut merged: Vec<(usize, usize)> = Vec::new();
            for w in windows {
                match merged.last_mut() {
                    Some(last) if w.0 <= last.1 => last.1 = last.1.max(w.1),
                    _ => merged.push(w),
                }
            }
            merged
        }
    };
    for (lo, hi) in windows {
        // The notes the previous item ended with: their mean string and the hand window they leave.
        let before: Vec<(u8, u8)> = (lo > 0)
            .then(|| {
                let last = &items[lo - 1];
                ends(last, |g| g.members.len() - 1).iter().filter_map(|&i| notes[last.notes[i]].string_fret).collect()
            })
            .unwrap_or_default();
        let start = summarize(before.iter().copied());
        let start_anchor = before.iter().map(|s| s.1).filter(|&f| ctx.fretted(f)).min();
        let range: Vec<&Item> = items[lo..=hi].iter().collect();
        let options: Vec<Vec<Combo>> = range.iter().map(|it| combos(it, &ctx)).collect();
        let picks = viterbi(&ctx, &range, &options, start, start_anchor);

        for ((item, combos), &pick) in range.iter().zip(&options).zip(&picks) {
            let Some(combo) = combos.get(pick) else { continue };
            for (&i, &slot) in item.notes.iter().zip(&combo.slots) {
                let note = &mut notes[i];
                if is_locked(note) {
                    continue;
                }
                if note.string_fret != slot {
                    outcome.changed.push(note.id);
                }
                note.string_fret = slot;
                if slot.is_none() {
                    outcome.unplaceable.push(note.id);
                }
            }
        }
    }
    outcome
}

/// Assigns every unlocked note; returns the notes that have no playable placement (their `string_fret` is `None`).
pub fn assign_tabs(notes: &mut [Note], times: &NoteTimes, tuning: &Tuning, capo: u8, opts: &TabOptions) -> Vec<u64> {
    run(notes, times, tuning, capo, opts, Scope::All).unplaceable
}

/// Re-solves after the note at `edited_index` was locked, changed or reset: the following automatic notes up to the
/// next locked note, plus the preceding ones back to the previous locked note when `propagate_backwards`.
/// Returns the ids whose placement changed.
pub fn assign_tabs_range(notes: &mut [Note], times: &NoteTimes, tuning: &Tuning, capo: u8, opts: &TabOptions, edited_index: usize, propagate_backwards: bool) -> Vec<u64> {
    assign_tabs_ranges(notes, times, tuning, capo, opts, &[edited_index], propagate_backwards)
}

/// [`assign_tabs_range`] for several edited notes at once: the windows are merged and each is solved once.
pub fn assign_tabs_ranges(notes: &mut [Note], times: &NoteTimes, tuning: &Tuning, capo: u8, opts: &TabOptions, edited: &[usize], propagate_backwards: bool) -> Vec<u64> {
    run(notes, times, tuning, capo, opts, Scope::From { edited, backwards: propagate_backwards }).changed
}

/// One way to play the slide / bend / legato group of a note: every member on `string`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupOption {
    pub string: u8,
    pub frets: Vec<(u64, u8)>,
}

impl GroupOption {
    pub fn fret_of(&self, id: u64) -> Option<u8> {
        self.frets.iter().find(|f| f.0 == id).map(|f| f.1)
    }
}

/// The strings (lowest first) that play the whole group of `id` (the note alone when it has no link),
/// with the frets that follow; locks are ignored.
pub fn group_options(notes: &[Note], tuning: &Tuning, capo: u8, opts: &TabOptions, id: u64) -> Vec<GroupOption> {
    let Some(at) = notes.iter().position(|n| n.id == id) else { return Vec::new() };
    let ctx = Ctx { tuning, capo, opts, spans: Vec::new(), family: Vec::new() };
    let mut links = read_links(notes);
    let root = find(&mut links.parent, at);
    let members: Vec<usize> = (0..notes.len()).filter(|&i| find(&mut links.parent, i) == root).collect();
    (0..tuning.open_strings.len())
        .filter_map(|s| {
            let frets = derive(notes, &links, &ctx, &members, s)?;
            Some(GroupOption { string: s as u8, frets: members.iter().map(|&m| notes[m].id).zip(frets).collect() })
        })
        .collect()
}
