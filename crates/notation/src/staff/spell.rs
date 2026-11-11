//! Pitch spelling and staff positions. A diatonic number counts letters from C (C0 = 0), a staff
//! step counts them from the bottom line of the treble staff (E4 = 0, B4 = 4 is the middle line).

const NATURAL_PC: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const SHARP_ORDER: [usize; 7] = [3, 0, 4, 1, 5, 2, 6];
const FLAT_ORDER: [usize; 7] = [6, 2, 5, 1, 4, 0, 3];
const E4_DIATONIC: i32 = 4 * 7 + 2;

/// Number of sharps (positive) or flats (negative), -7..=7.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeySignature(pub i8);

impl KeySignature {
    pub fn accidental(self, letter: usize) -> i8 {
        let n = usize::from(self.0.unsigned_abs()).min(7);
        let order = if self.0 > 0 { &SHARP_ORDER } else { &FLAT_ORDER };
        if order[..n].contains(&letter) {
            self.0.signum()
        } else {
            0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accidental {
    Sharp,
    Flat,
    Natural,
}

impl Accidental {
    pub fn from_alter(alter: i8) -> Self {
        match alter {
            1.. => Self::Sharp,
            0 => Self::Natural,
            _ => Self::Flat,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spelled {
    pub letter: usize,
    pub alter: i8,
    pub octave: i32,
}

impl Spelled {
    pub fn step(self) -> i32 {
        self.octave * 7 + self.letter as i32 - E4_DIATONIC
    }
}

/// Diatonic spelling of the key first, then the natural letter, then sharps (flats in flat keys).
pub fn spell(midi: i32, key: KeySignature) -> Spelled {
    let pc = midi.rem_euclid(12);
    let make = |letter: usize, alter: i8| Spelled { letter, alter, octave: (midi - i32::from(alter)).div_euclid(12) - 1 };
    let natural = |pc: i32| NATURAL_PC.iter().position(|&n| n == pc.rem_euclid(12));
    if let Some(letter) = (0..7).find(|&l| (NATURAL_PC[l] + i32::from(key.accidental(l))).rem_euclid(12) == pc) {
        return make(letter, key.accidental(letter));
    }
    if let Some(letter) = natural(pc) {
        return make(letter, 0);
    }
    match (key.0 >= 0, natural(pc - 1), natural(pc + 1)) {
        (true, Some(letter), _) => make(letter, 1),
        (_, _, Some(letter)) => make(letter, -1),
        _ => make(0, 0),
    }
}
