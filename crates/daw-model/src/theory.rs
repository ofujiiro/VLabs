// SPDX-License-Identifier: GPL-3.0-or-later

//! Music theory on MIDI keys (decision 0018): scales and the song's key,
//! chords, inversions and intervals. Pitch classes count semitones above C
//! (0 is C, 1 is C sharp or D flat, … 11 is B). Names for people live in the
//! interface's text; this module deals in numbers only.

use serde::{Deserialize, Serialize};

use crate::song::MAX_KEY;

/// The kinds of scale a key can have.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleKind {
    /// Major (Ionian).
    Major,
    /// Natural minor (Aeolian).
    Minor,
    /// Harmonic minor: natural minor with a raised seventh.
    HarmonicMinor,
    /// Melodic minor, as it ascends: major with a lowered third.
    MelodicMinor,
    /// Dorian: minor with a raised sixth.
    Dorian,
    /// Phrygian: minor with a lowered second.
    Phrygian,
    /// Lydian: major with a raised fourth.
    Lydian,
    /// Mixolydian: major with a lowered seventh.
    Mixolydian,
    /// Locrian: diminished fifth and lowered second.
    Locrian,
    /// Major pentatonic: five notes.
    MajorPentatonic,
    /// Minor pentatonic: five notes.
    MinorPentatonic,
    /// Blues: minor pentatonic with the diminished fifth.
    Blues,
}

impl ScaleKind {
    /// Every kind, in the order the interface offers them.
    pub const ALL: [Self; 12] = [
        Self::Major,
        Self::Minor,
        Self::HarmonicMinor,
        Self::MelodicMinor,
        Self::Dorian,
        Self::Phrygian,
        Self::Lydian,
        Self::Mixolydian,
        Self::Locrian,
        Self::MajorPentatonic,
        Self::MinorPentatonic,
        Self::Blues,
    ];

    /// Semitones above the tonic of each note of the scale, rising.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Self::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Self::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Self::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Self::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Self::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Self::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Self::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Self::MajorPentatonic => &[0, 2, 4, 7, 9],
            Self::MinorPentatonic => &[0, 3, 5, 7, 10],
            Self::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }

    /// Whether the scale has a minor third above its tonic (for spelling
    /// its tonic's name the way musicians do: C sharp minor, D flat major).
    pub fn is_minor(self) -> bool {
        self.intervals().contains(&3) && !self.intervals().contains(&4)
    }
}

/// A key: a tonic and a scale, such as A minor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Scale {
    /// The tonic's pitch class (0 is C).
    pub tonic: u8,
    /// The scale.
    pub kind: ScaleKind,
}

impl Scale {
    /// A key; the tonic is taken as a pitch class.
    pub fn new(tonic: u8, kind: ScaleKind) -> Self {
        Self {
            tonic: tonic % 12,
            kind,
        }
    }

    /// The scale's notes as twelve bits, bit 0 for C.
    pub fn mask(self) -> u16 {
        self.kind
            .intervals()
            .iter()
            .fold(0, |mask, i| mask | 1 << ((self.tonic + i) % 12))
    }

    /// Whether a key (a MIDI note) is in the scale.
    pub fn contains(self, key: u8) -> bool {
        self.mask() & (1 << (key % 12)) != 0
    }

    /// Whether a key is the tonic, in any octave.
    pub fn is_tonic(self, key: u8) -> bool {
        key % 12 == self.tonic
    }

    /// The nearest key in the scale (halfway between two, the higher one),
    /// within the keyboard.
    pub fn nearest(self, key: u8) -> u8 {
        let key = i32::from(key.min(MAX_KEY));
        for distance in 0..12 {
            for candidate in [key + distance, key - distance] {
                if (0..=i32::from(MAX_KEY)).contains(&candidate)
                    && self.contains(u8::try_from(candidate).unwrap_or(0))
                {
                    return u8::try_from(candidate).unwrap_or(0);
                }
            }
        }
        u8::try_from(key).unwrap_or(0)
    }

    /// How many notes of the scale lie below `key` (counting from the
    /// tonic of octave zero; negative below it).
    fn index_below(self, key: i32) -> i32 {
        let steps = self.kind.intervals();
        let size = i32::try_from(steps.len()).unwrap_or(7);
        let from_tonic = key - i32::from(self.tonic);
        let octave = from_tonic.div_euclid(12);
        let rest = from_tonic.rem_euclid(12);
        let below = steps.iter().filter(|&&i| i32::from(i) < rest).count();
        octave * size + i32::try_from(below).unwrap_or(0)
    }

    /// The key of the scale note at `index` (see [`Self::index_below`]).
    fn key_at(self, index: i32) -> i32 {
        let steps = self.kind.intervals();
        let size = i32::try_from(steps.len()).unwrap_or(7);
        let octave = index.div_euclid(size);
        let step = steps[usize::try_from(index.rem_euclid(size)).unwrap_or(0)];
        i32::from(self.tonic) + octave * 12 + i32::from(step)
    }

    /// Where `key` sits among the notes of the scale: the place of its
    /// nearest scale note, counted from the tonic of octave zero. Moving
    /// every note of a chord by the same number of places keeps it a chord
    /// of the key.
    pub fn index(self, key: u8) -> i32 {
        self.index_below(i32::from(self.nearest(key)))
    }

    /// The key at a place among the notes of the scale (see
    /// [`Self::index`]), if it is on the keyboard.
    pub fn key_at_index(self, index: i32) -> Option<u8> {
        u8::try_from(self.key_at(index))
            .ok()
            .filter(|&k| k <= MAX_KEY)
    }

    /// The key `steps` notes of the scale above `key` (below if negative).
    /// A key outside the scale counts its nearest scale note in that
    /// direction as the first step. `None` if the result is off the
    /// keyboard.
    pub fn step(self, key: u8, steps: i32) -> Option<u8> {
        let key = i32::from(key);
        let index = self.index_below(key);
        let in_scale = self.contains(u8::try_from(key).unwrap_or(0));
        let target = match (in_scale, steps.signum()) {
            (false, 1) => index + steps - 1,
            (false, 0) => return Some(self.nearest(u8::try_from(key).unwrap_or(0))),
            _ => index + steps,
        };
        u8::try_from(self.key_at(target))
            .ok()
            .filter(|&k| k <= MAX_KEY)
    }

    /// The chord the scale builds on `root` (moved to the nearest scale
    /// note first) by stacking every other note of the scale: three notes
    /// make a triad, four a seventh chord. Keys off the keyboard are left
    /// out.
    pub fn chord_on(self, root: u8, size: usize) -> Vec<u8> {
        let root = self.nearest(root);
        let index = self.index_below(i32::from(root));
        (0..size)
            .filter_map(|n| {
                let n = i32::try_from(n).unwrap_or(0);
                u8::try_from(self.key_at(index + 2 * n)).ok()
            })
            .filter(|&k| k <= MAX_KEY)
            .collect()
    }
}

/// The song's key over time: a key (or none) from bar zero, and changes at
/// bars (decision 0018). Always holds a change at bar zero; changes are
/// sorted and at distinct bars.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyMap {
    changes: Vec<KeyChange>,
}

/// A change of key at the start of a bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyChange {
    /// The bar the key starts on, counted from zero.
    pub bar: u32,
    /// The key from that bar on; `None` is no key (all twelve notes alike).
    pub scale: Option<Scale>,
}

impl KeyMap {
    /// One key (or none) for the whole song.
    pub fn constant(scale: Option<Scale>) -> Self {
        Self {
            changes: vec![KeyChange { bar: 0, scale }],
        }
    }

    /// The changes, in order; the first is at bar zero.
    pub fn changes(&self) -> &[KeyChange] {
        &self.changes
    }

    /// The key in effect in a bar.
    pub fn at_bar(&self, bar: u32) -> Option<Scale> {
        let index = self
            .changes
            .partition_point(|c| c.bar <= bar)
            .saturating_sub(1);
        self.changes[index].scale
    }

    /// Whether the song has a key anywhere.
    pub fn has_any(&self) -> bool {
        self.changes.iter().any(|c| c.scale.is_some())
    }

    /// Sets the key from `bar` on (replacing a change at that bar).
    pub fn set(&mut self, bar: u32, scale: Option<Scale>) {
        match self.changes.binary_search_by(|c| c.bar.cmp(&bar)) {
            Ok(index) => self.changes[index].scale = scale,
            Err(index) => self.changes.insert(index, KeyChange { bar, scale }),
        }
    }

    /// Removes the change at a bar (the one at bar zero stays).
    pub fn remove(&mut self, bar: u32) {
        if bar > 0 {
            self.changes.retain(|c| c.bar != bar);
        }
    }

    /// Moves the change at `from` to `to` (replacing one there). The change
    /// at bar zero does not move, and nothing moves onto bar zero.
    pub fn move_change(&mut self, from: u32, to: u32) {
        if from == 0 || to == 0 || from == to {
            return;
        }
        if let Some(change) = self.changes.iter().find(|c| c.bar == from).copied() {
            self.remove(from);
            self.set(to, change.scale);
        }
    }

    /// Makes the map valid whatever a file said: sorted, one change per
    /// bar, a change at bar zero.
    pub fn repair(&mut self) {
        self.changes.sort_by_key(|c| c.bar);
        self.changes.dedup_by_key(|c| c.bar);
        if self.changes.first().is_none_or(|c| c.bar != 0) {
            self.changes.insert(
                0,
                KeyChange {
                    bar: 0,
                    scale: None,
                },
            );
        }
    }
}

impl Default for KeyMap {
    fn default() -> Self {
        Self::constant(None)
    }
}

/// Kinds of chord a chord stamp can place, besides chords built in the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChordKind {
    /// Major triad.
    Major,
    /// Minor triad.
    Minor,
    /// Diminished triad.
    Diminished,
    /// Augmented triad.
    Augmented,
    /// Suspended second.
    Sus2,
    /// Suspended fourth.
    Sus4,
    /// Power chord: root, fifth and octave.
    Power,
    /// Major sixth.
    Sixth,
    /// Minor sixth.
    MinorSixth,
    /// Dominant seventh.
    Seventh,
    /// Major seventh.
    MajorSeventh,
    /// Minor seventh.
    MinorSeventh,
    /// Half-diminished seventh (minor seventh, flat five).
    HalfDiminished,
    /// Diminished seventh.
    DiminishedSeventh,
    /// Major triad with an added ninth.
    AddNine,
    /// Dominant ninth.
    Ninth,
    /// Major ninth.
    MajorNinth,
    /// Minor ninth.
    MinorNinth,
}

impl ChordKind {
    /// Every kind, in the order the interface offers them.
    pub const ALL: [Self; 18] = [
        Self::Major,
        Self::Minor,
        Self::Diminished,
        Self::Augmented,
        Self::Sus2,
        Self::Sus4,
        Self::Power,
        Self::Sixth,
        Self::MinorSixth,
        Self::Seventh,
        Self::MajorSeventh,
        Self::MinorSeventh,
        Self::HalfDiminished,
        Self::DiminishedSeventh,
        Self::AddNine,
        Self::Ninth,
        Self::MajorNinth,
        Self::MinorNinth,
    ];

    /// Semitones above the root of each note, rising.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Self::Major => &[0, 4, 7],
            Self::Minor => &[0, 3, 7],
            Self::Diminished => &[0, 3, 6],
            Self::Augmented => &[0, 4, 8],
            Self::Sus2 => &[0, 2, 7],
            Self::Sus4 => &[0, 5, 7],
            Self::Power => &[0, 7, 12],
            Self::Sixth => &[0, 4, 7, 9],
            Self::MinorSixth => &[0, 3, 7, 9],
            Self::Seventh => &[0, 4, 7, 10],
            Self::MajorSeventh => &[0, 4, 7, 11],
            Self::MinorSeventh => &[0, 3, 7, 10],
            Self::HalfDiminished => &[0, 3, 6, 10],
            Self::DiminishedSeventh => &[0, 3, 6, 9],
            Self::AddNine => &[0, 4, 7, 14],
            Self::Ninth => &[0, 4, 7, 10, 14],
            Self::MajorNinth => &[0, 4, 7, 11, 14],
            Self::MinorNinth => &[0, 3, 7, 10, 14],
        }
    }

    /// The chord on `root`, in root position. Keys off the keyboard are
    /// left out.
    pub fn on(self, root: u8) -> Vec<u8> {
        self.intervals()
            .iter()
            .map(|i| u16::from(root) + u16::from(*i))
            .filter(|&k| k <= u16::from(MAX_KEY))
            .filter_map(|k| u8::try_from(k).ok())
            .collect()
    }

    /// The kind of chord these keys make with `root` as the root, if it is
    /// one of the kinds above (octaves and order do not matter).
    pub fn recognise(root: u8, keys: &[u8]) -> Option<Self> {
        let mut classes: Vec<u8> = keys.iter().map(|&k| (k + 12 - root % 12) % 12).collect();
        classes.sort_unstable();
        classes.dedup();
        Self::ALL.into_iter().find(|kind| {
            let mut wanted: Vec<u8> = kind.intervals().iter().map(|i| i % 12).collect();
            wanted.sort_unstable();
            wanted.dedup();
            wanted == classes
        })
    }
}

/// What a chord stamp places on each click.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChordShape {
    /// A chord built in the song's key on the clicked note, with this many
    /// notes (3 for triads, 4 for seventh chords).
    InKey(usize),
    /// A chord of a fixed kind on the clicked note.
    Kind(ChordKind),
}

impl ChordShape {
    /// The chord's keys on `root`, rising, in the given inversion (0 is
    /// root position). In-key chords need the key; without one they fall
    /// back to a major triad or seventh.
    pub fn keys(self, root: u8, key: Option<Scale>, inversion: usize) -> Vec<u8> {
        let keys = match (self, key) {
            (Self::InKey(size), Some(scale)) => scale.chord_on(root, size),
            (Self::InKey(size), None) if size >= 4 => ChordKind::Seventh.on(root),
            (Self::InKey(_), None) => ChordKind::Major.on(root),
            (Self::Kind(kind), _) => kind.on(root),
        };
        invert(&keys, inversion)
    }
}

/// A chord's keys in an inversion: each inversion moves the lowest note up
/// an octave (rising order is kept). A note that would go off the keyboard
/// stays where it is.
pub fn invert(keys: &[u8], inversion: usize) -> Vec<u8> {
    let mut keys: Vec<u8> = keys.to_vec();
    keys.sort_unstable();
    for _ in 0..inversion.min(keys.len().saturating_sub(1)) {
        let lowest = keys[0];
        let highest = keys[keys.len() - 1];
        let mut raised = lowest;
        while raised <= highest {
            match raised.checked_add(12).filter(|&k| k <= MAX_KEY) {
                Some(k) => raised = k,
                None => break,
            }
        }
        if raised <= highest {
            break;
        }
        keys.remove(0);
        keys.push(raised);
    }
    keys
}

/// Moves a chord's lowest note above its highest (`up`), or its highest
/// below its lowest; the other notes stay. `None` if that would leave the
/// keyboard or the chord has fewer than two notes.
pub fn invert_once(keys: &[u8], up: bool) -> Option<Vec<u8>> {
    let lowest = *keys.iter().min()?;
    let highest = *keys.iter().max()?;
    if keys.len() < 2 || lowest == highest {
        return None;
    }
    let mut out = keys.to_vec();
    if up {
        let mut raised = lowest;
        while raised <= highest {
            raised = raised.checked_add(12).filter(|&k| k <= MAX_KEY)?;
        }
        let at = out.iter().position(|&k| k == lowest)?;
        out[at] = raised;
    } else {
        let mut lowered = highest;
        while lowered >= lowest {
            lowered = lowered.checked_sub(12)?;
        }
        let at = out.iter().position(|&k| k == highest)?;
        out[at] = lowered;
    }
    Some(out)
}

/// The intervals offered for transposing, in semitones: from a minor
/// second up to an octave.
pub const INTERVALS: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

#[cfg(test)]
mod tests {
    use super::*;

    const C: u8 = 60;

    fn c_major() -> Scale {
        Scale::new(0, ScaleKind::Major)
    }

    #[test]
    fn scales_know_their_notes() {
        let scale = c_major();
        let white: Vec<u8> = (60..72).filter(|&k| scale.contains(k)).collect();
        assert_eq!(white, [60, 62, 64, 65, 67, 69, 71]);
        let a_minor = Scale::new(9, ScaleKind::Minor);
        assert_eq!(a_minor.mask(), scale.mask(), "relative keys share notes");
        assert!(
            Scale::new(9, ScaleKind::HarmonicMinor).contains(68),
            "G sharp"
        );
        assert!(a_minor.is_tonic(57) && a_minor.is_tonic(69) && !a_minor.is_tonic(60));
        assert!(ScaleKind::Minor.is_minor() && ScaleKind::Dorian.is_minor());
        assert!(!ScaleKind::Major.is_minor() && !ScaleKind::Lydian.is_minor());
        for kind in ScaleKind::ALL {
            let steps = kind.intervals();
            assert_eq!(steps[0], 0, "{kind:?} starts on its tonic");
            assert!(steps.windows(2).all(|w| w[0] < w[1]), "{kind:?} rises");
        }
    }

    #[test]
    fn notes_snap_to_the_nearest_note_of_the_key() {
        let scale = c_major();
        assert_eq!(scale.nearest(61), 62, "halfway between C and D: up");
        assert_eq!(scale.nearest(66), 67, "F sharp goes up to G");
        assert_eq!(scale.nearest(64), 64);
        let pentatonic = Scale::new(0, ScaleKind::MajorPentatonic);
        assert_eq!(pentatonic.nearest(65), 64, "F is nearer to E than to G");
        assert_eq!(scale.nearest(127), 127, "G9 is in C major");
        assert_eq!(Scale::new(1, ScaleKind::Major).nearest(127), 126);
    }

    #[test]
    fn steps_of_the_scale() {
        let scale = c_major();
        assert_eq!(scale.step(C, 1), Some(62));
        assert_eq!(scale.step(C, 2), Some(64), "a third up");
        assert_eq!(scale.step(C, 7), Some(72), "an octave is seven steps");
        assert_eq!(scale.step(C, -1), Some(59), "B below");
        assert_eq!(scale.step(64, 1), Some(65), "E to F is a step");
        assert_eq!(
            scale.step(61, 1),
            Some(62),
            "C sharp: D is the first step up"
        );
        assert_eq!(scale.step(61, -1), Some(60), "and C the first down");
        assert_eq!(scale.step(61, 2), Some(64));
        assert_eq!(scale.step(127, 1), None, "off the keyboard");
        let blues = Scale::new(9, ScaleKind::Blues);
        assert_eq!(blues.step(57, 3), Some(63), "A, C, D, E flat");
    }

    #[test]
    fn places_among_the_notes_of_the_scale() {
        let scale = c_major();
        assert_eq!(scale.index(60), 35, "five octaves of seven notes below C4");
        assert_eq!(scale.index(61), scale.index(62), "C sharp counts as D");
        assert_eq!(scale.key_at_index(scale.index(64) + 2), Some(67));
        assert_eq!(scale.key_at_index(-1), None);
        assert_eq!(scale.key_at_index(scale.index(127) + 1), None);
    }

    #[test]
    fn chords_in_the_key_follow_the_scale_degree() {
        let scale = c_major();
        assert_eq!(scale.chord_on(C, 3), [60, 64, 67], "C major");
        assert_eq!(scale.chord_on(62, 3), [62, 65, 69], "D minor");
        assert_eq!(scale.chord_on(71, 3), [71, 74, 77], "B diminished");
        assert_eq!(scale.chord_on(67, 4), [67, 71, 74, 77], "G7");
        assert_eq!(scale.chord_on(61, 3), [62, 65, 69], "C sharp goes to D");
        assert_eq!(
            ChordKind::recognise(62, &scale.chord_on(62, 3)),
            Some(ChordKind::Minor)
        );
        assert_eq!(
            ChordKind::recognise(71, &scale.chord_on(71, 4)),
            Some(ChordKind::HalfDiminished)
        );
        let harmonic = Scale::new(9, ScaleKind::HarmonicMinor);
        assert_eq!(
            ChordKind::recognise(64, &harmonic.chord_on(64, 4)),
            Some(ChordKind::Seventh),
            "E7 in A harmonic minor"
        );
    }

    #[test]
    fn chord_kinds_and_inversions() {
        assert_eq!(ChordKind::MajorSeventh.on(C), [60, 64, 67, 71]);
        assert_eq!(ChordKind::Ninth.on(C), [60, 64, 67, 70, 74]);
        assert_eq!(ChordKind::Major.on(125), [125], "only what fits");
        let shape = ChordShape::Kind(ChordKind::Major);
        assert_eq!(shape.keys(C, None, 0), [60, 64, 67]);
        assert_eq!(shape.keys(C, None, 1), [64, 67, 72], "first inversion");
        assert_eq!(shape.keys(C, None, 2), [67, 72, 76], "second inversion");
        assert_eq!(
            shape.keys(C, None, 5),
            [67, 72, 76],
            "no more than there are"
        );
        let in_key = ChordShape::InKey(3);
        assert_eq!(in_key.keys(69, Some(c_major()), 0), [69, 72, 76], "A minor");
        assert_eq!(in_key.keys(69, None, 0), [69, 73, 76], "no key: major");
        for kind in ChordKind::ALL {
            assert_eq!(
                ChordKind::recognise(C, &kind.on(C)),
                Some(kind),
                "{kind:?} is recognised"
            );
        }
    }

    #[test]
    fn inverting_a_chord_step_by_step() {
        assert_eq!(invert_once(&[60, 64, 67], true), Some(vec![72, 64, 67]));
        assert_eq!(invert_once(&[60, 64, 67], false), Some(vec![60, 64, 55]));
        assert_eq!(invert_once(&[60, 72], true), Some(vec![84, 72]));
        assert_eq!(invert_once(&[60], true), None);
        assert_eq!(invert_once(&[120, 127], true), None, "off the keyboard");
        assert_eq!(invert_once(&[0, 5], false), None);
    }

    #[test]
    fn the_key_map_changes_at_bars() {
        let mut map = KeyMap::default();
        assert_eq!(map.at_bar(5), None);
        assert!(!map.has_any());
        map.set(0, Some(c_major()));
        map.set(8, Some(Scale::new(4, ScaleKind::Major)));
        assert_eq!(map.at_bar(7), Some(c_major()));
        assert_eq!(map.at_bar(8).map(|s| s.tonic), Some(4));
        map.move_change(8, 12);
        assert_eq!(map.at_bar(8), Some(c_major()));
        assert_eq!(map.at_bar(12).map(|s| s.tonic), Some(4));
        map.move_change(0, 3);
        assert_eq!(map.changes()[0].bar, 0, "the start stays");
        map.remove(12);
        map.remove(0);
        assert_eq!(map.changes().len(), 1);
        assert!(map.has_any());
    }

    #[test]
    fn a_damaged_key_map_is_repaired() {
        let mut map: KeyMap = serde_json::from_str(
            r#"{"changes": [{"bar": 4, "scale": null}, {"bar": 4, "scale": null}]}"#,
        )
        .unwrap();
        map.repair();
        assert_eq!(map.changes().len(), 2);
        assert_eq!(map.changes()[0].bar, 0);
    }
}
