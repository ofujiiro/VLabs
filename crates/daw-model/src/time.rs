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
    /// Whether the tempo glides from here to the next change's tempo
    /// (decision 0018). The last change holds its tempo either way.
    #[serde(default, skip_serializing_if = "is_false")]
    pub glide: bool,
    /// How the glide bends (decision 0026), from −1 to 1: 0 is a straight
    /// line over the ticks; a positive bend holds the tempo back and changes
    /// it more and more towards the end (a ritardando that gathers); a
    /// negative one makes most of the change first, then settles. Kept when
    /// the glide is turned off, so turning it on again brings it back.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub curve: f64,
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_false(value: &bool) -> bool {
    !*value
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

impl TempoChange {
    /// A sudden change of tempo at `at`.
    pub fn new(at: Ticks, bpm: f64) -> Self {
        Self {
            at,
            bpm,
            glide: false,
            curve: 0.0,
        }
    }
}

/// How far a bend of 1 moves a glide's exponent from the straight line's 1
/// (decision 0026).
const BEND: f64 = 8.0;

/// Below this, an exponent or a rate counts as zero (where the closed forms
/// have their limits).
const TINY: f64 = 1e-12;

/// Keeps a bend in range, whatever a file or a drag said.
fn clamp_curve(curve: f64) -> f64 {
    if curve.is_finite() {
        curve.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// One piece of a tempo map (decisions 0018 and 0026): from `start` at
/// `bpm` to `end` (infinitely far for the last piece), arriving at
/// `next_bpm` there if it glides (the same tempo if it does not), bent by
/// `curve`.
///
/// Within a glide, with `x` the fraction passed and `w = next_bpm / bpm`,
/// the tempo is `bpm · [1 + (w^q − 1)·x]^(1/q)`, where `q = 1 ± 8·curve`
/// (plus when the tempo slows, minus when it speeds up, so a positive bend
/// always comes late). Seconds have closed forms both ways for every `q`,
/// so the model and the audio thread agree exactly, without steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoPiece {
    /// Where the piece starts, in ticks.
    pub start: f64,
    /// Where it ends, in ticks (infinite for the last piece).
    pub end: f64,
    /// The tempo at the start.
    pub bpm: f64,
    /// The tempo at the end (the same as `bpm` unless it glides).
    pub next_bpm: f64,
    /// The glide's bend, −1 to 1.
    pub curve: f64,
}

#[allow(clippy::cast_precision_loss, reason = "a constant")]
fn quarter() -> f64 {
    TICKS_PER_QUARTER as f64
}

impl TempoPiece {
    /// A piece that holds `bpm` from `start` to `end`.
    pub fn steady(start: f64, end: f64, bpm: f64) -> Self {
        Self {
            start,
            end,
            bpm,
            next_bpm: bpm,
            curve: 0.0,
        }
    }

    /// Whether the tempo moves within the piece.
    pub fn glides(&self) -> bool {
        self.end.is_finite()
            && self.end > self.start
            && (self.next_bpm - self.bpm).abs() > TINY * self.bpm
    }

    /// The exponent of the glide's shape: 1 is a straight line.
    fn exponent(&self) -> f64 {
        let direction = if self.next_bpm < self.bpm { 1.0 } else { -1.0 };
        1.0 + BEND * clamp_curve(self.curve) * direction
    }

    /// `ln(next_bpm / bpm)`.
    fn log_ratio(&self) -> f64 {
        (self.next_bpm / self.bpm).ln()
    }

    /// `ln(1 + (w^q − 1)·x)` for a glide's exponent `q` (not 0), accurate
    /// at both ends: near the start through `ln_1p`, near the end (where a
    /// strong bend between far-apart tempos makes it the log of a tiny
    /// number) as `ln((1 − x) + w^q·x)`, which does not cancel.
    fn log_base(q: f64, ln_w: f64, x: f64) -> f64 {
        let cx = (q * ln_w).exp_m1() * x;
        if cx.abs() < 0.5 {
            cx.ln_1p()
        } else {
            ((1.0 - x) + (q * ln_w).exp() * x).ln()
        }
    }

    /// The tempo at `tick` (held outside the piece).
    pub fn bpm_at(&self, tick: f64) -> f64 {
        if !self.glides() {
            return self.bpm;
        }
        let x = ((tick - self.start) / (self.end - self.start)).clamp(0.0, 1.0);
        let q = self.exponent();
        let ln_w = self.log_ratio();
        let bpm = if q.abs() < TINY {
            self.bpm * (x * ln_w).exp()
        } else {
            self.bpm * (Self::log_base(q, ln_w, x) / q).exp()
        };
        // (Never past either tempo, even by a rounding error.)
        bpm.clamp(self.bpm.min(self.next_bpm), self.bpm.max(self.next_bpm))
    }

    /// Seconds from the start of the piece to `tick` (within the piece).
    #[allow(
        clippy::many_single_char_names,
        reason = "the letters of the formula in decision 0026"
    )]
    pub fn seconds_to(&self, tick: f64) -> f64 {
        let seconds_per_tick = 60.0 / (self.bpm * quarter());
        let ticks = tick - self.start;
        if !self.glides() {
            return ticks * seconds_per_tick;
        }
        let length = self.end - self.start;
        let x = ticks / length;
        let q = self.exponent();
        let ln_w = self.log_ratio();
        // The integral of bpm / tempo over the fraction passed.
        let integral = if q.abs() < TINY {
            // The tempo grows by the same ratio all along: w^x.
            -(-x * ln_w).exp_m1() / ln_w
        } else {
            let c = (q * ln_w).exp_m1();
            let p = 1.0 - 1.0 / q;
            let y = Self::log_base(q, ln_w, x);
            if p.abs() < TINY {
                y / c
            } else {
                (p * y).exp_m1() / (c * p)
            }
        };
        integral * length * seconds_per_tick
    }

    /// The tick reached `seconds` after the start of the piece (the inverse
    /// of [`Self::seconds_to`]).
    #[allow(
        clippy::many_single_char_names,
        reason = "the letters of the formula in decision 0026"
    )]
    pub fn tick_after(&self, seconds: f64) -> f64 {
        let seconds_per_tick = 60.0 / (self.bpm * quarter());
        if !self.glides() {
            return self.start + seconds / seconds_per_tick;
        }
        let length = self.end - self.start;
        let integral = seconds / (seconds_per_tick * length);
        let q = self.exponent();
        let ln_w = self.log_ratio();
        // (Arguments of ln_1p stay above −1 even a hair past the end.)
        let floor = -1.0 + f64::EPSILON;
        let x = if q.abs() < TINY {
            -(-integral * ln_w).max(floor).ln_1p() / ln_w
        } else {
            let c = (q * ln_w).exp_m1();
            let p = 1.0 - 1.0 / q;
            if p.abs() < TINY {
                (c * integral).exp_m1() / c
            } else {
                ((c * p * integral).max(floor).ln_1p() / p).exp_m1() / c
            }
        };
        self.start + x * length
    }

    /// How long the whole piece lasts, in seconds (infinite for the last).
    pub fn seconds(&self) -> f64 {
        if self.end.is_finite() {
            self.seconds_to(self.end)
        } else {
            f64::INFINITY
        }
    }
}

/// The bend that makes a glide from `bpm` to `next_bpm` pass through
/// `middle_bpm` half-way (decision 0026: what dragging the glide's handle
/// does). A middle outside the two tempos gives the strongest bend that
/// way; a glide that does not move has no bend.
pub fn curve_through(bpm: f64, next_bpm: f64, middle_bpm: f64) -> f64 {
    let piece = |curve: f64| TempoPiece {
        start: 0.0,
        end: 2.0,
        bpm,
        next_bpm,
        curve,
    };
    if !piece(0.0).glides() || !middle_bpm.is_finite() {
        return 0.0;
    }
    // The middle tempo moves one way as the bend grows: find which.
    let at = |curve: f64| piece(curve).bpm_at(1.0);
    let rising = at(1.0) > at(-1.0);
    let (mut low, mut high) = (-1.0_f64, 1.0_f64);
    for _ in 0..60 {
        let mid = f64::midpoint(low, high);
        if (at(mid) < middle_bpm) == rising {
            low = mid;
        } else {
            high = mid;
        }
    }
    f64::midpoint(low, high)
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
            changes: vec![TempoChange::new(Ticks::ZERO, clamp_tempo(bpm))],
        }
    }

    /// A tempo map from a list of changes, in any order: tempos are kept in
    /// range, a later change at the same tick wins, and the tempo at zero is
    /// the first change's if none is at zero.
    pub fn from_changes(changes: impl IntoIterator<Item = TempoChange>) -> Self {
        let mut sorted: Vec<TempoChange> = changes
            .into_iter()
            .map(|c| TempoChange {
                at: c.at.max(Ticks::ZERO),
                bpm: clamp_tempo(c.bpm),
                glide: c.glide,
                curve: clamp_curve(c.curve),
            })
            .collect();
        sorted.sort_by_key(|c| c.at);
        let mut map = Self {
            changes: Vec::with_capacity(sorted.len()),
        };
        for change in sorted {
            match map.changes.last_mut() {
                Some(last) if last.at == change.at => *last = change,
                _ => map.changes.push(change),
            }
        }
        match map.changes.first().copied() {
            None => map.changes.push(TempoChange::new(Ticks::ZERO, 120.0)),
            Some(first) if first.at > Ticks::ZERO => map.changes.insert(
                0,
                TempoChange {
                    at: Ticks::ZERO,
                    ..first
                },
            ),
            Some(_) => {}
        }
        map
    }

    /// Makes the map valid whatever a file said (see [`Self::from_changes`]).
    pub fn repair(&mut self) {
        *self = Self::from_changes(std::mem::take(&mut self.changes));
    }

    /// The changes, in order; the first is at tick zero.
    pub fn changes(&self) -> &[TempoChange] {
        &self.changes
    }

    /// The tempo at the start of the song.
    pub fn initial_bpm(&self) -> f64 {
        self.changes[0].bpm
    }

    /// The index of the change in effect at `at`.
    fn index_at(&self, at: Ticks) -> usize {
        self.changes
            .partition_point(|c| c.at <= at)
            .saturating_sub(1)
    }

    /// The change in effect at `at` (the piece of the song it is in).
    pub fn change_at(&self, at: Ticks) -> TempoChange {
        self.changes[self.index_at(at)]
    }

    /// One piece of the map: from a change to the next (infinitely far
    /// for the last).
    #[allow(clippy::cast_precision_loss, reason = "tick counts are far below 2^52")]
    pub fn piece(&self, index: usize) -> TempoPiece {
        let change = self.changes[index];
        let start = change.at.0 as f64;
        match self.changes.get(index + 1) {
            Some(next) if change.glide => TempoPiece {
                start,
                end: next.at.0 as f64,
                bpm: change.bpm,
                next_bpm: next.bpm,
                curve: change.curve,
            },
            Some(next) => TempoPiece::steady(start, next.at.0 as f64, change.bpm),
            None => TempoPiece::steady(start, f64::INFINITY, change.bpm),
        }
    }

    /// The pieces of the map, in order.
    pub fn pieces(&self) -> impl Iterator<Item = TempoPiece> + '_ {
        (0..self.changes.len()).map(|index| self.piece(index))
    }

    /// The tempo at `at`: the change in effect, or on a glide the tempo
    /// reached so far.
    #[allow(clippy::cast_precision_loss, reason = "tick counts are far below 2^52")]
    pub fn bpm_at(&self, at: Ticks) -> f64 {
        self.piece(self.index_at(at)).bpm_at(at.0 as f64)
    }

    /// Sets the tempo from `at` onwards (replacing the tempo of a change at
    /// the same position and keeping its glide). Setting it at zero changes
    /// the initial tempo.
    pub fn set(&mut self, at: Ticks, bpm: f64) {
        let at = at.max(Ticks::ZERO);
        let bpm = clamp_tempo(bpm);
        match self.changes.binary_search_by(|c| c.at.cmp(&at)) {
            Ok(index) => self.changes[index].bpm = bpm,
            Err(index) => self.changes.insert(index, TempoChange::new(at, bpm)),
        }
    }

    /// Makes the change at `at` glide to the next one, or not.
    pub fn set_glide(&mut self, at: Ticks, glide: bool) {
        if let Some(change) = self.changes.iter_mut().find(|c| c.at == at) {
            change.glide = glide;
        }
    }

    /// Bends the glide that starts at `at` (decision 0026; −1 to 1, 0 is
    /// straight).
    pub fn set_curve(&mut self, at: Ticks, curve: f64) {
        if let Some(change) = self.changes.iter_mut().find(|c| c.at == at) {
            change.curve = clamp_curve(curve);
        }
    }

    /// Moves the change at `from` to `to` (replacing one there). The change
    /// at zero does not move, and nothing moves onto zero.
    pub fn move_change(&mut self, from: Ticks, to: Ticks) {
        if from <= Ticks::ZERO || to <= Ticks::ZERO || from == to {
            return;
        }
        if let Some(change) = self.changes.iter().find(|c| c.at == from).copied() {
            self.remove(from);
            let mut changes = std::mem::take(&mut self.changes);
            changes.push(TempoChange { at: to, ..change });
            *self = Self::from_changes(changes);
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
        let at = at.0 as f64;
        let mut seconds = 0.0;
        for piece in self.pieces() {
            if piece.start >= at {
                break;
            }
            if piece.end < at {
                seconds += piece.seconds();
            } else {
                seconds += piece.seconds_to(at);
                break;
            }
        }
        seconds
    }

    /// The position reached after `seconds`, as fractional ticks.
    pub fn ticks_at(&self, seconds: f64) -> f64 {
        let mut elapsed = 0.0;
        for piece in self.pieces() {
            let span = piece.seconds();
            if elapsed + span > seconds {
                return piece.tick_after(seconds - elapsed);
            }
            elapsed += span;
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

    /// The signature of a bar.
    pub fn signature_at_bar(&self, bar: u32) -> TimeSignature {
        let index = self
            .changes
            .partition_point(|c| c.bar <= bar)
            .saturating_sub(1);
        self.changes[index].signature
    }

    /// The map as seen from `bar`: its bar zero is `bar` of this map (for a
    /// pattern placed there, whose bars count from its start).
    #[must_use]
    pub fn from_bar(&self, bar: u32) -> Self {
        let mut changes = vec![MeterChange {
            bar: 0,
            signature: self.signature_at_bar(bar),
        }];
        changes.extend(
            self.changes
                .iter()
                .filter(|c| c.bar > bar)
                .map(|c| MeterChange {
                    bar: c.bar - bar,
                    signature: c.signature,
                }),
        );
        Self { changes }
    }

    /// Moves the change at `from` to bar `to` (replacing one there). The
    /// change at bar zero does not move, and nothing moves onto bar zero.
    pub fn move_change(&mut self, from: u32, to: u32) {
        if from == 0 || to == 0 || from == to {
            return;
        }
        if let Some(change) = self.changes.iter().find(|c| c.bar == from).copied() {
            self.remove(from);
            self.set(to, change.signature);
        }
    }

    /// Makes the map valid whatever a file said: sorted, one change per
    /// bar, a change at bar zero.
    pub fn repair(&mut self) {
        self.changes.sort_by_key(|c| c.bar);
        self.changes.dedup_by_key(|c| c.bar);
        if self.changes.first().is_none_or(|c| c.bar != 0) {
            let signature = self
                .changes
                .first()
                .map_or(TimeSignature::FOUR_FOUR, |c| c.signature);
            self.changes.insert(0, MeterChange { bar: 0, signature });
        }
    }

    /// The start of the bar `at` is in.
    pub fn bar_floor(&self, at: Ticks) -> Ticks {
        self.bar_start(self.position(at).bar)
    }

    /// The last grid line at or before `at`: grid lines are every `step`
    /// from the start of each bar (decision 0018), so after an odd bar the
    /// grid still meets the bar lines.
    pub fn grid_floor(&self, at: Ticks, step: Ticks) -> Ticks {
        let bar = self.bar_floor(at);
        if step <= Ticks::ZERO {
            return at;
        }
        bar + (at - bar).floor_to(step)
    }

    /// The grid line nearest `at` (halfway rounds up); the next bar line
    /// counts as a grid line even when the steps do not fill the bar evenly.
    pub fn grid_round(&self, at: Ticks, step: Ticks) -> Ticks {
        if step <= Ticks::ZERO {
            return at;
        }
        let before = self.grid_floor(at, step);
        let next_bar = self.bar_start(self.position(at).bar + 1);
        let after = (before + step).min(next_bar);
        if (at - before) * 2 >= after - before {
            after
        } else {
            before
        }
    }

    /// Where a position under this map lands under `other` if it keeps its
    /// bar and its distance from the bar line (decision 0018: changing a
    /// time signature keeps the song in its bars).
    pub fn keep_bars(&self, other: &Self, at: Ticks) -> Ticks {
        if at <= Ticks::ZERO {
            return at;
        }
        let bar = self.position(at).bar;
        other.bar_start(bar) + (at - self.bar_start(bar))
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
    fn tempo_glides_are_exact_both_ways() {
        let mut map = TempoMap::constant(60.0);
        map.set(Ticks::quarters(4), 120.0);
        map.set_glide(Ticks::ZERO, true);
        // A glide from 60 to 120 over four quarters lasts 4 * ln 2 seconds
        // (the integral of 60 / bpm over the quarters).
        let expected = 4.0 * std::f64::consts::LN_2;
        assert!((map.seconds_at(Ticks::quarters(4)) - expected).abs() < 1e-9);
        assert!((map.bpm_at(Ticks::quarters(2)) - 90.0).abs() < 1e-9);
        assert_eq!(map.bpm_at(Ticks::quarters(5)), 120.0);
        for quarters in [0.5, 1.0, 2.5, 3.99, 4.0, 7.0] {
            let tick = Ticks::from_quarters_f64(quarters);
            let back = map.ticks_at(map.seconds_at(tick));
            assert!((back - tick.0 as f64).abs() < 1e-3, "{quarters} comes back");
        }
        // Past the glide the tempo holds: four more quarters at 120 BPM.
        let after = map.seconds_at(Ticks::quarters(8)) - map.seconds_at(Ticks::quarters(4));
        assert!((after - 2.0).abs() < 1e-9);
        // A glide that hardly moves gives the same answer as no glide.
        let mut flat = TempoMap::constant(100.0);
        flat.set(Ticks::quarters(4), 100.000_001);
        flat.set_glide(Ticks::ZERO, true);
        assert!((flat.seconds_at(Ticks::quarters(4)) - 2.4).abs() < 1e-6);
    }

    /// A glide of `quarters` quarter notes from `bpm` to `next_bpm`.
    fn glide(bpm: f64, next_bpm: f64, quarters: f64, curve: f64) -> TempoPiece {
        TempoPiece {
            start: 1000.0,
            end: 1000.0 + quarters * quarter(),
            bpm,
            next_bpm,
            curve,
        }
    }

    /// How fast the seconds of a piece grow at `tick`, by a central
    /// difference of its closed form: it must match 60 / (tempo × ticks per
    /// quarter), the time one tick takes at the tempo there.
    fn seconds_rate(piece: &TempoPiece, tick: f64) -> f64 {
        let h = 1e-6 * (piece.end - piece.start);
        (piece.seconds_to(tick + h) - piece.seconds_to(tick - h)) / (2.0 * h)
    }

    #[test]
    fn shaped_glides_are_exact_both_ways() {
        // Bends where the exponent crosses 0 (±0.125) and 1 (0) included.
        let curves = [-1.0, -0.5, -0.125, -0.05, 0.0, 0.05, 0.125, 0.3, 0.5, 1.0];
        let tempos = [
            (100.0, 60.0),
            (60.0, 100.0),
            (20.0, 999.0),
            (999.0, 20.0),
            (120.0, 120.000_000_1),
        ];
        for curve in curves {
            for (bpm, next_bpm) in tempos {
                let piece = glide(bpm, next_bpm, 7.0, curve);
                let what = format!("{bpm} to {next_bpm}, bend {curve}");
                assert_eq!(piece.seconds_to(piece.start), 0.0, "{what}: starts at 0 s");
                for fraction in [0.0, 0.1, 0.37, 0.5, 0.9, 1.0] {
                    let tick = piece.start + fraction * (piece.end - piece.start);
                    let seconds = piece.seconds_to(tick);
                    let back = piece.tick_after(seconds);
                    assert!(
                        (back - tick).abs() < 1e-4,
                        "{what}: {fraction} comes back ({back} for {tick})"
                    );
                    if fraction > 0.0 && fraction < 1.0 {
                        // The closed form is the integral of the tempo's
                        // time per tick: its slope is that time.
                        let expected = 60.0 / (piece.bpm_at(tick) * quarter());
                        let rate = seconds_rate(&piece, tick);
                        assert!(
                            (rate - expected).abs() < 1e-5 * expected,
                            "{what}: at {fraction}, {rate} s per tick, not {expected}"
                        );
                    }
                }
                // The tempo moves one way only, from one tempo to the other.
                let (low, high) = (bpm.min(next_bpm), bpm.max(next_bpm));
                let mut last = piece.bpm_at(piece.start);
                assert!(
                    (last - bpm).abs() < 1e-9 * bpm,
                    "{what}: starts at its tempo"
                );
                for i in 1..=50 {
                    let tick = piece.start + f64::from(i) / 50.0 * (piece.end - piece.start);
                    let now = piece.bpm_at(tick);
                    assert!(now >= low && now <= high, "{what}: in range");
                    assert!(
                        (now - last) * (next_bpm - bpm).signum() >= -1e-9 * high,
                        "{what}: never turns back"
                    );
                    last = now;
                }
                assert!((last - next_bpm).abs() < 1e-6 * next_bpm, "{what}: arrives");
            }
        }
    }

    #[test]
    fn half_way_a_glide_is_at_the_power_mean_of_its_tempos() {
        let middle = |piece: &TempoPiece| piece.bpm_at(f64::midpoint(piece.start, piece.end));
        // Straight: the arithmetic mean, as before (decision 0018).
        assert!((middle(&glide(100.0, 60.0, 4.0, 0.0)) - 80.0).abs() < 1e-9);
        // A slowing exponent of 0 (bend −0.125): the geometric mean.
        let geometric = (100.0_f64 * 60.0).sqrt();
        assert!((middle(&glide(100.0, 60.0, 4.0, -0.125)) - geometric).abs() < 1e-9);
        // Exponent −1 (bend −0.25 when slowing): the harmonic mean.
        let harmonic = 2.0 * 100.0 * 60.0 / 160.0;
        assert!((middle(&glide(100.0, 60.0, 4.0, -0.25)) - harmonic).abs() < 1e-9);
        // Late and Early on a ritardando from 100 to 60 (decision 0026).
        let late = middle(&glide(100.0, 60.0, 4.0, 0.5));
        let early = middle(&glide(100.0, 60.0, 4.0, -0.5));
        assert!((late - 88.37).abs() < 0.01, "late: {late}");
        assert!((early - 70.82).abs() < 0.01, "early: {early}");
        // A positive bend comes late whichever way the tempo goes.
        let late_up = middle(&glide(60.0, 100.0, 4.0, 0.5));
        let early_up = middle(&glide(60.0, 100.0, 4.0, -0.5));
        assert!(late_up < 80.0 && early_up > 80.0, "{late_up} {early_up}");
    }

    #[test]
    fn the_bend_through_a_middle_tempo_is_found() {
        for (bpm, next_bpm) in [(100.0, 60.0), (60.0, 100.0), (30.0, 400.0)] {
            for curve in [-1.0, -0.6, -0.125, 0.0, 0.2, 0.5, 0.97] {
                let piece = glide(bpm, next_bpm, 4.0, curve);
                let middle = piece.bpm_at(f64::midpoint(piece.start, piece.end));
                let found = curve_through(bpm, next_bpm, middle);
                assert!(
                    (found - curve).abs() < 1e-6,
                    "{bpm} {next_bpm} {curve}: {found}"
                );
            }
        }
        // Beyond the strongest bend, the strongest bend that way.
        assert!((curve_through(100.0, 60.0, 99.9) - 1.0).abs() < 1e-6);
        assert!((curve_through(100.0, 60.0, 60.1) + 1.0).abs() < 1e-6);
        assert!((curve_through(60.0, 100.0, 60.1) - 1.0).abs() < 1e-6);
        // A glide that does not move has nothing to bend.
        assert_eq!(curve_through(90.0, 90.0, 120.0), 0.0);
    }

    #[test]
    fn a_bent_glide_shapes_the_map_and_survives_repairs() {
        let mut map = TempoMap::constant(100.0);
        map.set(Ticks::quarters(8), 60.0);
        map.set_glide(Ticks::ZERO, true);
        let straight = map.seconds_at(Ticks::quarters(8));
        map.set_curve(Ticks::ZERO, 0.5);
        let late = map.seconds_at(Ticks::quarters(8));
        // Holding the tempo back for longer makes the ritardando shorter.
        assert!(late < straight, "{late} < {straight}");
        assert!(map.bpm_at(Ticks::quarters(4)) > 80.0);
        for quarters in [0.5, 3.0, 7.9, 8.0, 11.0] {
            let tick = Ticks::from_quarters_f64(quarters);
            let back = map.ticks_at(map.seconds_at(tick));
            assert!((back - tick.0 as f64).abs() < 1e-3, "{quarters} comes back");
        }
        // Past the glide the tempo holds at 60: four quarters, four seconds.
        let after = map.seconds_at(Ticks::quarters(12)) - map.seconds_at(Ticks::quarters(8));
        assert!((after - 4.0).abs() < 1e-9);
        // Bends stay in range and travel with their change.
        map.set_curve(Ticks::ZERO, 7.0);
        assert_eq!(map.changes()[0].curve, 1.0);
        map.set(Ticks::quarters(16), 90.0);
        map.set_glide(Ticks::quarters(8), true);
        map.set_curve(Ticks::quarters(8), -0.4);
        map.move_change(Ticks::quarters(8), Ticks::quarters(10));
        assert_eq!(
            map.changes()[1].curve,
            -0.4,
            "a moved change keeps its bend"
        );
        let repaired = TempoMap::from_changes([TempoChange {
            curve: f64::NAN,
            glide: true,
            ..TempoChange::new(Ticks::ZERO, 100.0)
        }]);
        assert_eq!(repaired.changes()[0].curve, 0.0);
        // A straight change writes no bend; a bent one does.
        let plain = serde_json::to_string(&TempoChange::new(Ticks::ZERO, 100.0)).unwrap();
        assert!(!plain.contains("curve"), "{plain}");
        let bent: TempoChange =
            serde_json::from_str(&serde_json::to_string(&map.changes()[1]).unwrap()).unwrap();
        assert_eq!(bent, map.changes()[1]);
    }

    #[test]
    fn tempo_changes_move_and_maps_are_repaired() {
        let mut map = TempoMap::constant(120.0);
        map.set(Ticks::quarters(4), 90.0);
        map.set_glide(Ticks::quarters(4), true);
        map.move_change(Ticks::quarters(4), Ticks::quarters(8));
        assert_eq!(map.changes()[1].at, Ticks::quarters(8));
        assert!(map.changes()[1].glide, "a moved change keeps its glide");
        map.move_change(Ticks::ZERO, Ticks::quarters(2));
        assert_eq!(map.changes()[0].at, Ticks::ZERO, "the start stays");
        let repaired = TempoMap::from_changes([
            TempoChange::new(Ticks::quarters(4), 5.0),
            TempoChange::new(Ticks::quarters(2), 150.0),
            TempoChange::new(Ticks::quarters(4), 80.0),
        ]);
        assert_eq!(repaired.changes().len(), 3);
        assert_eq!(
            repaired.initial_bpm(),
            150.0,
            "the first tempo starts the song"
        );
        assert_eq!(
            repaired.bpm_at(Ticks::quarters(5)),
            80.0,
            "the later one wins"
        );
    }

    #[test]
    fn grids_count_from_each_bar_line() {
        let mut meter = MeterMap::default();
        meter.set(1, TimeSignature::new(7, 8).unwrap());
        meter.set(2, TimeSignature::FOUR_FOUR);
        let quarter = Ticks::QUARTER;
        // Bar 2 starts three and a half quarters after bar 1.
        let bar2 = meter.bar_start(2);
        assert_eq!(bar2, Ticks::WHOLE + Ticks::note_value(7, 8));
        assert_eq!(meter.grid_floor(bar2 + Ticks(5), quarter), bar2);
        assert_eq!(
            meter.grid_floor(bar2 + quarter + Ticks(5), quarter),
            bar2 + quarter
        );
        // In the 7/8 bar, a quarter grid ends early at the bar line.
        let last = meter.bar_start(1) + quarter * 3;
        assert_eq!(meter.grid_round(last + Ticks(1), quarter), last);
        assert_eq!(meter.grid_round(bar2 - Ticks(1), quarter), bar2);
        assert_eq!(meter.bar_floor(bar2 - Ticks(1)), meter.bar_start(1));
        assert_eq!(meter.signature_at_bar(1).numerator, 7);
    }

    #[test]
    fn positions_keep_their_bars_when_the_meter_changes() {
        let old = MeterMap::default();
        let mut new = MeterMap::default();
        new.set(2, TimeSignature::new(3, 4).unwrap());
        new.set(3, TimeSignature::FOUR_FOUR);
        // Bar 5, beat 2 in 4/4 everywhere moves a quarter earlier.
        let at = old.bar_start(4) + Ticks::QUARTER;
        assert_eq!(old.keep_bars(&new, at), at - Ticks::QUARTER);
        assert_eq!(
            old.keep_bars(&new, Ticks::QUARTER),
            Ticks::QUARTER,
            "before: unchanged"
        );
        let seen = new.from_bar(2);
        assert_eq!(seen.initial().numerator, 3);
        assert_eq!(seen.bar_start(1), Ticks::quarters(3));
        let mut moved = new.clone();
        moved.move_change(2, 5);
        assert_eq!(moved.signature_at_bar(2), TimeSignature::FOUR_FOUR);
        assert_eq!(moved.signature_at_bar(5).numerator, 3);
        let mut damaged: MeterMap = serde_json::from_str(
            r#"{"changes": [{"bar": 3, "signature": {"numerator": 3, "denominator": 4}}]}"#,
        )
        .unwrap();
        damaged.repair();
        assert_eq!(damaged.changes()[0].bar, 0);
    }

    #[test]
    fn regions() {
        assert!(Region::new(Ticks(5), Ticks(5)).is_none());
        let region = Region::new(Ticks(5), Ticks(10)).unwrap();
        assert!(region.contains(Ticks(5)) && !region.contains(Ticks(10)));
        assert_eq!(region.length(), Ticks(5));
    }
}
