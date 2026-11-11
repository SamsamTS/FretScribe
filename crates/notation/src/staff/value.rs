//! Note values in ticks (24 per quarter note, so every `SnapGrid` division is an integer) and the
//! split of a time span into the fewest engraved values.
//!
//! A value may cross a main-beat boundary only when it starts on a beat, is aligned to its own
//! undotted length and lands on a half beat (half note on beat 1 of 4/4, dotted quarter...). Rests
//! never cross a beat. Straight values avoid triplet beats, triplet values stay inside one.

use std::sync::LazyLock;

pub const TICKS_PER_BEAT: i64 = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
}

impl Base {
    const ALL: [Self; 6] = [Self::Whole, Self::Half, Self::Quarter, Self::Eighth, Self::Sixteenth, Self::ThirtySecond];

    pub fn ticks(self) -> i64 {
        96 >> self as u32
    }

    /// Flags (or beams) a note of this value carries.
    pub fn flags(self) -> u8 {
        (self as u8).saturating_sub(2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Duration {
    pub base: Base,
    pub dots: u8,
    pub triplet: bool,
}

impl Duration {
    pub const WHOLE: Self = Self { base: Base::Whole, dots: 0, triplet: false };

    pub fn ticks(self) -> i64 {
        let t = self.base.ticks();
        if self.triplet {
            t * 2 / 3
        } else {
            t * ((2 << self.dots) - 1) / (1 << self.dots)
        }
    }

    fn exact(self) -> bool {
        let t = self.base.ticks();
        self.triplet || t * ((2 << self.dots) - 1) % (1 << self.dots) == 0
    }

    pub fn beats(self) -> f64 {
        self.ticks() as f64 / TICKS_PER_BEAT as f64
    }
}

/// Every representable value, longest first.
static VALUES: LazyLock<Vec<Duration>> = LazyLock::new(|| {
    let mut all: Vec<Duration> = Base::ALL
        .iter()
        .flat_map(|&base| (0..=2).map(move |dots| Duration { base, dots, triplet: false }))
        .chain(Base::ALL.iter().map(|&base| Duration { base, dots: 0, triplet: true }))
        .filter(|d| d.exact())
        .collect();
    all.sort_by_key(|d| std::cmp::Reverse(d.ticks()));
    all
});

/// Main-beat length in ticks: the written beat, or a group of three for compound meters.
pub fn main_beat_ticks(numerator: u8, denominator: u8) -> i64 {
    let unit = TICKS_PER_BEAT * 4 / i64::from(denominator);
    if denominator >= 8 && numerator > 3 && numerator.is_multiple_of(3) {
        unit * 3
    } else {
        unit
    }
}

pub struct Context<'a> {
    pub beat: i64,
    /// Middle of the measure in ticks; a long value that does not start the measure stays on one side.
    pub half: i64,
    /// Per quarter-note window of the measure: whether it is divided in triplets.
    pub triplet: &'a [bool],
}

impl Context<'_> {
    fn triplet_at(&self, window: i64) -> bool {
        self.triplet.get(window as usize).copied().unwrap_or(false)
    }

    fn allowed(&self, p: i64, v: Duration, rest: bool, strict: bool) -> bool {
        let t = v.ticks();
        let (w0, w1) = (p / TICKS_PER_BEAT, (p + t - 1) / TICKS_PER_BEAT);
        let kinds = if v.triplet { w0 == w1 && self.triplet_at(w0) } else { (w0..=w1).all(|w| !self.triplet_at(w)) };
        if !strict || !kinds {
            return kinds;
        }
        let base = if v.triplet { t } else { v.base.ticks() };
        let within = p / self.beat == (p + t - 1) / self.beat;
        if t < self.beat {
            within && (p % self.beat) % base == 0
        } else {
            within && p % self.beat == 0 || !rest && p % self.beat == 0 && (2 * t) % self.beat == 0 && p % base == 0 && (p == 0 || !(p < self.half && p + t > self.half))
        }
    }
}

/// Fewest values covering `from..to` (ticks from the measure start), longest first among equals.
/// Falls back to ignoring the beat rules, and drops what no value can express.
pub fn split(ctx: &Context, from: i64, to: i64, rest: bool) -> Vec<(i64, Duration)> {
    let n = (to - from).max(0) as usize;
    for strict in [true, false] {
        let mut best: Vec<Option<(u32, Duration)>> = vec![None; n + 1];
        best[n] = Some((0, Duration::WHOLE));
        for i in (0..n).rev() {
            for &v in VALUES.iter() {
                let next = i + v.ticks() as usize;
                let Some((count, _)) = best.get(next).copied().flatten() else { continue };
                if ctx.allowed(from + i as i64, v, rest, strict) && best[i].is_none_or(|(c, _)| count + 1 < c) {
                    best[i] = Some((count + 1, v));
                }
            }
        }
        if best[0].is_some() || !strict {
            let (mut out, mut i) = (Vec::new(), 0);
            while let Some((_, v)) = best[i].filter(|_| i < n) {
                out.push((from + i as i64, v));
                i += v.ticks() as usize;
            }
            return out;
        }
    }
    Vec::new()
}
