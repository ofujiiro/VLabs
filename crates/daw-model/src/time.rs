// SPDX-License-Identifier: GPL-3.0-or-later

//! Musical time, counted exactly in integer ticks (decision 0007).

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Rem, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// Ticks in one quarter note: 2⁶ × 3² × 5 × 7 × 11 × 13. Every common
/// subdivision, including triplets, quintuplets, septuplets and 11- and
/// 13-note groups, is a whole number of ticks.
pub const TICKS_PER_QUARTER: i64 = 2_882_880;

/// Ticks in one whole note.
pub const TICKS_PER_WHOLE: i64 = 4 * TICKS_PER_QUARTER;

/// A position or a length in musical time, in ticks.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Ticks(pub i64);

impl Ticks {
    /// The start of the song, or no length at all.
    pub const ZERO: Self = Self(0);
    /// One quarter note.
    pub const QUARTER: Self = Self(TICKS_PER_QUARTER);
    /// One whole note.
    pub const WHOLE: Self = Self(TICKS_PER_WHOLE);

    /// `n` quarter notes.
    pub const fn quarters(n: i64) -> Self {
        Self(n * TICKS_PER_QUARTER)
    }

    /// A note value as a fraction of a whole note: `note_value(1, 4)` is a
    /// quarter note, `note_value(1, 12)` an eighth-note triplet,
    /// `note_value(3, 8)` a dotted quarter.
    ///
    /// Exact whenever the result is a whole number of ticks, which is the
    /// case for every common value (decision 0007); otherwise it is rounded
    /// to the nearest tick.
    pub fn note_value(numerator: i64, denominator: i64) -> Self {
        assert!(denominator > 0, "a note value needs a positive denominator");
        let whole = i128::from(TICKS_PER_WHOLE) * i128::from(numerator);
        let denominator = i128::from(denominator);
        let rounded = (2 * whole + denominator) / (2 * denominator);
        Self(i64::try_from(rounded).unwrap_or(i64::MAX))
    }

    /// This position or length in quarter notes (for display and
    /// interchange only; never compute positions with it).
    #[allow(clippy::cast_precision_loss, reason = "display and interchange only")]
    pub fn as_quarters(self) -> f64 {
        self.0 as f64 / TICKS_PER_QUARTER as f64
    }

    /// The nearest tick to a number of quarter notes (for importing
    /// interchange formats that store beats as decimals).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "rounded to the nearest tick on purpose"
    )]
    pub fn from_quarters_f64(quarters: f64) -> Self {
        Self((quarters * TICKS_PER_QUARTER as f64).round() as i64)
    }

    /// Rounds to the nearest multiple of `grid` (half-way rounds up).
    #[must_use]
    pub fn round_to(self, grid: Self) -> Self {
        if grid.0 <= 0 {
            return self;
        }
        Self((self.0 + grid.0 / 2).div_euclid(grid.0) * grid.0)
    }

    /// Rounds down to a multiple of `grid`.
    #[must_use]
    pub fn floor_to(self, grid: Self) -> Self {
        if grid.0 <= 0 {
            return self;
        }
        Self(self.0.div_euclid(grid.0) * grid.0)
    }

    /// Rounds up to a multiple of `grid`.
    #[must_use]
    pub fn ceil_to(self, grid: Self) -> Self {
        if grid.0 <= 0 {
            return self;
        }
        let floor = self.floor_to(grid);
        if floor == self { self } else { floor + grid }
    }

    /// The larger of two positions.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        if self >= other { self } else { other }
    }

    /// The smaller of two positions.
    #[must_use]
    pub fn min(self, other: Self) -> Self {
        if self <= other { self } else { other }
    }
}

impl Add for Ticks {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl AddAssign for Ticks {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl Sub for Ticks {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl SubAssign for Ticks {
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

impl Neg for Ticks {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<i64> for Ticks {
    type Output = Self;
    fn mul(self, factor: i64) -> Self {
        Self(self.0 * factor)
    }
}

impl Div<i64> for Ticks {
    type Output = Self;
    fn div(self, divisor: i64) -> Self {
        Self(self.0 / divisor)
    }
}

impl Rem for Ticks {
    type Output = Self;
    fn rem(self, other: Self) -> Self {
        Self(self.0.rem_euclid(other.0))
    }
}

impl Sum for Ticks {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(iter.map(|t| t.0).sum())
    }
}

/// A time signature, such as 4/4 or 7/8.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeSignature {
    /// Beats per bar.
    pub numerator: u8,
    /// The note value of one beat: 1, 2, 4, 8, 16 or 32.
    pub denominator: u8,
}

impl TimeSignature {
    /// Four quarter notes per bar.
    pub const FOUR_FOUR: Self = Self {
        numerator: 4,
        denominator: 4,
    };

    /// A time signature, if it is a valid one.
    pub fn new(numerator: u8, denominator: u8) -> Option<Self> {
        let valid = (1..=64).contains(&numerator) && matches!(denominator, 1 | 2 | 4 | 8 | 16 | 32);
        valid.then_some(Self {
            numerator,
            denominator,
        })
    }

    /// The length of one bar.
    pub fn bar_length(self) -> Ticks {
        Ticks::note_value(i64::from(self.numerator), i64::from(self.denominator))
    }

    /// The length of one beat (the note value of the denominator).
    pub fn beat_length(self) -> Ticks {
        Ticks::note_value(1, i64::from(self.denominator))
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self::FOUR_FOUR
    }
}

impl fmt::Display for TimeSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

/// The slowest and fastest tempos accepted, in beats per minute.
pub const TEMPO_RANGE: std::ops::RangeInclusive<f64> = 20.0..=999.0;

/// A tempo change: from `at` onwards, `bpm` quarter notes per minute.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TempoChange {
    /// Where the tempo starts.
    pub at: Ticks,
    /// Quarter notes per minute.
    pub bpm: f64,
}

/// The tempo over the whole song: a tempo at the start, and changes.
///
/// Always holds at least one change, at tick zero; changes are kept sorted
/// and at distinct positions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TempoMap {
    changes: Vec<TempoChange>,
}

impl TempoMap {
    /// A constant tempo.
    pub fn constant(bpm: f64) -> Self {
        Self {
            changes: vec![TempoChange {
                at: Ticks::ZERO,
                bpm: clamp_tempo(bpm),
            }],
        }
    }

    /// The changes, in order; the first is at tick zero.
    pub fn changes(&self) -> &[TempoChange] {
        &self.changes
    }

    /// The tempo at the start of the song.
    pub fn initial_bpm(&self) -> f64 {
        self.changes[0].bpm
    }

    /// The tempo in effect at `at`.
    pub fn bpm_at(&self, at: Ticks) -> f64 {
        let index = self
            .changes
            .partition_point(|c| c.at <= at)
            .saturating_sub(1);
        self.changes[index].bpm
    }

    /// Sets the tempo from `at` onwards (replacing a change at the same
    /// position). Setting it at zero changes the initial tempo.
    pub fn set(&mut self, at: Ticks, bpm: f64) {
        let at = at.max(Ticks::ZERO);
        let bpm = clamp_tempo(bpm);
        match self.changes.binary_search_by(|c| c.at.cmp(&at)) {
            Ok(index) => self.changes[index].bpm = bpm,
            Err(index) => self.changes.insert(index, TempoChange { at, bpm }),
        }
    }

    /// Removes a tempo change (the one at zero cannot be removed).
    pub fn remove(&mut self, at: Ticks) {
        if at > Ticks::ZERO {
            self.changes.retain(|c| c.at != at);
        }
    }

    /// Seconds from the start of the song to `at`.
    #[allow(clippy::cast_precision_loss, reason = "tick counts are far below 2^52")]
    pub fn seconds_at(&self, at: Ticks) -> f64 {
        let mut seconds = 0.0;
        for (index, change) in self.changes.iter().enumerate() {
            if change.at >= at {
                break;
            }
            let end = self
                .changes
                .get(index + 1)
                .map_or(at, |next| next.at.min(at));
            seconds += (end - change.at).0 as f64 * seconds_per_tick(change.bpm);
        }
        seconds
    }

    /// The position reached after `seconds`, as fractional ticks.
    #[allow(clippy::cast_precision_loss, reason = "tick counts are far below 2^52")]
    pub fn ticks_at(&self, seconds: f64) -> f64 {
        let mut elapsed = 0.0;
        for (index, change) in self.changes.iter().enumerate() {
            let per_tick = seconds_per_tick(change.bpm);
            if let Some(next) = self.changes.get(index + 1) {
                let span = (next.at - change.at).0 as f64 * per_tick;
                if elapsed + span > seconds {
                    return change.at.0 as f64 + (seconds - elapsed) / per_tick;
                }
                elapsed += span;
            } else {
                return change.at.0 as f64 + (seconds - elapsed) / per_tick;
            }
        }
        0.0
    }
}

impl Default for TempoMap {
    fn default() -> Self {
        Self::constant(120.0)
    }
}

fn clamp_tempo(bpm: f64) -> f64 {
    if bpm.is_finite() {
        bpm.clamp(*TEMPO_RANGE.start(), *TEMPO_RANGE.end())
    } else {
        120.0
    }
}

/// Seconds per tick at a tempo.
#[allow(clippy::cast_precision_loss, reason = "a constant")]
pub fn seconds_per_tick(bpm: f64) -> f64 {
    60.0 / (bpm * TICKS_PER_QUARTER as f64)
}

/// A time-signature change, at the start of a bar (counted from zero).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeterChange {
    /// The bar the signature starts on, counted from zero.
    pub bar: u32,
    /// The signature from that bar on.
    pub signature: TimeSignature,
}

/// The time signatures over the whole song. Changes happen at bar lines.
///
/// Always holds at least one change, at bar zero.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeterMap {
    changes: Vec<MeterChange>,
}

/// A position as a musician reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarPosition {
    /// The bar, counted from zero (shown to people as `bar + 1`).
    pub bar: u32,
    /// The beat in the bar, counted from zero.
    pub beat: u32,
    /// Ticks after the start of that beat.
    pub ticks: Ticks,
    /// The signature in effect.
    pub signature: TimeSignature,
}

impl MeterMap {
    /// One signature for the whole song.
    pub fn constant(signature: TimeSignature) -> Self {
        Self {
            changes: vec![MeterChange { bar: 0, signature }],
        }
    }

    /// The changes, in order; the first is at bar zero.
    pub fn changes(&self) -> &[MeterChange] {
        &self.changes
    }

    /// The signature at the start of the song.
    pub fn initial(&self) -> TimeSignature {
        self.changes[0].signature
    }

    /// Sets the signature from `bar` onwards.
    pub fn set(&mut self, bar: u32, signature: TimeSignature) {
        match self.changes.binary_search_by(|c| c.bar.cmp(&bar)) {
            Ok(index) => self.changes[index].signature = signature,
            Err(index) => self.changes.insert(index, MeterChange { bar, signature }),
        }
    }

    /// Removes a change (the one at bar zero cannot be removed).
    pub fn remove(&mut self, bar: u32) {
        if bar > 0 {
            self.changes.retain(|c| c.bar != bar);
        }
    }

    /// Where bar `bar` (counted from zero) starts.
    pub fn bar_start(&self, bar: u32) -> Ticks {
        let mut start = Ticks::ZERO;
        for (index, change) in self.changes.iter().enumerate() {
            let next_bar = self
                .changes
                .get(index + 1)
                .map_or(u32::MAX, |next| next.bar);
            if bar <= next_bar {
                return start + change.signature.bar_length() * i64::from(bar - change.bar);
            }
            start += change.signature.bar_length() * i64::from(next_bar - change.bar);
        }
        start
    }

    /// The bar, beat and remaining ticks of a position.
    pub fn position(&self, at: Ticks) -> BarPosition {
        let at = at.max(Ticks::ZERO);
        let mut start = Ticks::ZERO;
        for (index, change) in self.changes.iter().enumerate() {
            let bar_length = change.signature.bar_length();
            let section_end = self
                .changes
                .get(index + 1)
                .map(|next| start + bar_length * i64::from(next.bar - change.bar));
            if section_end.is_none_or(|end| at < end) {
                let into = at - start;
                let bars = into.0.div_euclid(bar_length.0);
                let in_bar = Ticks(into.0.rem_euclid(bar_length.0));
                let beat_length = change.signature.beat_length();
                return BarPosition {
                    bar: change.bar + u32::try_from(bars).unwrap_or(u32::MAX),
                    beat: u32::try_from(in_bar.0 / beat_length.0).unwrap_or(0),
                    ticks: Ticks(in_bar.0 % beat_length.0),
                    signature: change.signature,
                };
            }
            start = section_end.unwrap_or(start);
        }
        BarPosition {
            bar: 0,
            beat: 0,
            ticks: Ticks::ZERO,
            signature: self.initial(),
        }
    }

    /// The signature in effect at a position.
    pub fn signature_at(&self, at: Ticks) -> TimeSignature {
        self.position(at).signature
    }
}

impl Default for MeterMap {
    fn default() -> Self {
        Self::constant(TimeSignature::FOUR_FOUR)
    }
}

/// A span of time from `start` (included) to `end` (excluded).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// Where the region starts.
    pub start: Ticks,
    /// Where the region ends (not included).
    pub end: Ticks,
}

impl Region {
    /// A region, if `end` is after `start`.
    pub fn new(start: Ticks, end: Ticks) -> Option<Self> {
        (end > start).then_some(Self { start, end })
    }

    /// The length of the region.
    pub fn length(self) -> Ticks {
        self.end - self.start
    }

    /// Whether `at` is inside the region.
    pub fn contains(self, at: Ticks) -> bool {
        at >= self.start && at < self.end
    }
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, reason = "test values")]
mod tests {
    use super::*;

    #[test]
    fn common_subdivisions_are_exact() {
        for denominator in [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 20, 24, 28, 32, 48, 64, 128, 256,
        ] {
            let value = Ticks::note_value(1, denominator);
            assert_eq!(
                value.0 * denominator,
                TICKS_PER_WHOLE,
                "1/{denominator} of a whole note is not a whole number of ticks"
            );
        }
        // A triplet inside a quintuplet.
        assert_eq!(Ticks::note_value(1, 4 * 3 * 5).0 * 60, TICKS_PER_WHOLE);
    }

    #[test]
    fn tuplets_add_up_exactly() {
        // Seven septuplet sixteenths fill exactly one half note.
        let septuplet = Ticks::note_value(1, 28);
        assert_eq!(septuplet * 7, Ticks::note_value(1, 4));
        // Three eighth triplets make a quarter.
        assert_eq!(Ticks::note_value(1, 12) * 3, Ticks::QUARTER);
    }

    #[test]
    fn rounding_to_a_grid() {
        let sixteenth = Ticks::note_value(1, 16);
        assert_eq!(Ticks(sixteenth.0 / 2 - 1).round_to(sixteenth), Ticks::ZERO);
        assert_eq!(Ticks(sixteenth.0 / 2).round_to(sixteenth), sixteenth);
        assert_eq!(Ticks(sixteenth.0 + 1).floor_to(sixteenth), sixteenth);
        assert_eq!(Ticks(sixteenth.0 + 1).ceil_to(sixteenth), sixteenth * 2);
        assert_eq!(Ticks(-1).floor_to(sixteenth), -sixteenth);
    }

    #[test]
    fn time_signatures() {
        assert_eq!(TimeSignature::FOUR_FOUR.bar_length(), Ticks::WHOLE);
        let seven_eight = TimeSignature::new(7, 8).unwrap();
        assert_eq!(seven_eight.bar_length(), Ticks::note_value(7, 8));
        assert_eq!(seven_eight.beat_length(), Ticks::note_value(1, 8));
        assert!(TimeSignature::new(4, 3).is_none());
        assert!(TimeSignature::new(0, 4).is_none());
        assert_eq!(seven_eight.to_string(), "7/8");
    }

    #[test]
    fn constant_tempo_converts_both_ways() {
        let map = TempoMap::constant(120.0);
        // At 120 BPM a quarter note lasts half a second.
        assert!((map.seconds_at(Ticks::QUARTER) - 0.5).abs() < 1e-12);
        assert!((map.ticks_at(0.5) - TICKS_PER_QUARTER as f64).abs() < 1e-6);
    }

    #[test]
    fn tempo_changes_are_followed() {
        let mut map = TempoMap::constant(120.0);
        map.set(Ticks::quarters(4), 60.0);
        // Four quarters at 120 (2 s), then four at 60 (4 s).
        assert!((map.seconds_at(Ticks::quarters(8)) - 6.0).abs() < 1e-9);
        assert!((map.ticks_at(6.0) - Ticks::quarters(8).0 as f64).abs() < 1e-3);
        assert!((map.ticks_at(1.0) - Ticks::quarters(2).0 as f64).abs() < 1e-3);
        assert_eq!(map.bpm_at(Ticks::quarters(3)), 120.0);
        assert_eq!(map.bpm_at(Ticks::quarters(4)), 60.0);
        map.remove(Ticks::quarters(4));
        assert_eq!(map.changes().len(), 1);
        map.remove(Ticks::ZERO);
        assert_eq!(map.changes().len(), 1, "the initial tempo stays");
    }

    #[test]
    fn tempo_is_kept_in_range() {
        assert_eq!(TempoMap::constant(5.0).initial_bpm(), 20.0);
        assert_eq!(TempoMap::constant(f64::NAN).initial_bpm(), 120.0);
    }

    #[test]
    fn bars_and_beats_follow_meter_changes() {
        let mut meter = MeterMap::default();
        meter.set(2, TimeSignature::new(7, 8).unwrap());
        assert_eq!(meter.bar_start(0), Ticks::ZERO);
        assert_eq!(meter.bar_start(2), Ticks::WHOLE * 2);
        assert_eq!(
            meter.bar_start(3),
            Ticks::WHOLE * 2 + Ticks::note_value(7, 8)
        );

        let p = meter.position(Ticks::WHOLE + Ticks::QUARTER * 2 + Ticks(5));
        assert_eq!((p.bar, p.beat, p.ticks), (1, 2, Ticks(5)));

        let p = meter.position(meter.bar_start(3) + Ticks::note_value(3, 8));
        assert_eq!((p.bar, p.beat, p.ticks), (3, 3, Ticks::ZERO));
        assert_eq!(p.signature.numerator, 7);
    }

    #[test]
    fn regions() {
        assert!(Region::new(Ticks(5), Ticks(5)).is_none());
        let region = Region::new(Ticks(5), Ticks(10)).unwrap();
        assert!(region.contains(Ticks(5)) && !region.contains(Ticks(10)));
        assert_eq!(region.length(), Ticks(5));
    }
}
