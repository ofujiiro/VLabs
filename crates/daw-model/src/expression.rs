// SPDX-License-Identifier: GPL-3.0-or-later

//! How a voice sings each note (decision 0028): the note's pitch points and
//! vibrato, its colour and breath; how a voice track's notes fall into
//! phrases; and the pitch a voice sings through a phrase, worked out once
//! here so the engine renders exactly what the piano roll draws.
//!
//! Times here are seconds of the song (or milliseconds from a note's
//! start), not ticks: a glide or a vibrato is a gesture of the voice and
//! lasts as long at any tempo, as an audio clip's fades do.

use serde::{Deserialize, Serialize};

use crate::song::DEFAULT_VELOCITY;

/// A rest at least this long (seconds) starts a new phrase.
pub const PHRASE_GAP: f64 = 0.05;

/// How long a vibrato takes to die away before the next note takes over
/// (or its note ends), in seconds.
pub const VIBRATO_FADE_OUT: f64 = 0.08;

/// The most pitch points a note keeps.
pub const MAX_PITCH_POINTS: usize = 64;

/// How early a pitch point may be, in milliseconds before its note's start.
pub const MAX_LEAD_MS: f32 = 1000.0;

/// How late a pitch point may be, in milliseconds after its note's start.
pub const MAX_POINT_MS: f32 = 60_000.0;

/// How far a pitch point may bend, in cents either way (two octaves).
pub const MAX_BEND_CENTS: f32 = 2400.0;

/// The deepest vibrato, in cents either side.
pub const MAX_VIBRATO_DEPTH: f32 = 300.0;

/// The range of a vibrato's rate, in Hz.
pub const VIBRATO_RATES: std::ops::RangeInclusive<f32> = 0.5..=12.0;

/// How far colour moves the formants at its ends, in semitones.
pub const COLOUR_SEMITONES: f32 = 4.0;

/// The shape of a note's pitch curve from one point to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PitchShape {
    /// An S-curve: leaves gently and arrives gently.
    #[default]
    Smooth,
    /// A straight line.
    Straight,
    /// Leaves gently, arrives at speed.
    SlowStart,
    /// Leaves at speed, arrives gently.
    SlowEnd,
}

impl PitchShape {
    /// Every shape, in the order a menu offers them.
    pub const ALL: [Self; 4] = [Self::Smooth, Self::Straight, Self::SlowStart, Self::SlowEnd];

    /// How much of the way from one point to the next the pitch has gone
    /// (0 to 1) when `u` of the time between them (0 to 1) has passed.
    pub fn ease(self, u: f64) -> f64 {
        let u = u.clamp(0.0, 1.0);
        let half_pi = std::f64::consts::FRAC_PI_2;
        match self {
            Self::Smooth => 0.5 - 0.5 * (std::f64::consts::PI * u).cos(),
            Self::Straight => u,
            Self::SlowStart => 1.0 - (half_pi * u).cos(),
            Self::SlowEnd => (half_pi * u).sin(),
        }
    }

    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's skip_serializing_if passes a reference"
    )]
    fn is_smooth(&self) -> bool {
        *self == Self::Smooth
    }
}

/// A point of a note's pitch curve.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PitchPoint {
    /// When, in milliseconds from the note's start (negative: before it).
    pub ms: f32,
    /// How far from the note's key, in cents (100 to a semitone).
    pub cents: f32,
    /// The shape of the curve from this point to the next.
    #[serde(default, skip_serializing_if = "PitchShape::is_smooth")]
    pub shape: PitchShape,
}

impl PitchPoint {
    /// A point with the smooth shape after it.
    pub const fn new(ms: f32, cents: f32) -> Self {
        Self {
            ms,
            cents,
            shape: PitchShape::Smooth,
        }
    }
}

/// The pitch of a note with no points of its own: a glide of 80 ms from
/// wherever the voice is, centred on the note's start (decision 0027).
pub const PLAIN_GLIDE: [PitchPoint; 2] = [PitchPoint::new(-40.0, 0.0), PitchPoint::new(40.0, 0.0)];

/// A note's vibrato.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vibrato {
    /// How far the pitch swings either side, in cents.
    pub depth: f32,
    /// How many swings a second.
    pub rate: f32,
    /// When it starts, in seconds after the note's start.
    pub start: f32,
    /// How long it takes to grow to its full depth, in seconds.
    pub fade_in: f32,
}

impl Default for Vibrato {
    /// The vibrato "Add vibrato" gives: a moderate one that starts once
    /// the note is established, so short notes stay plain.
    fn default() -> Self {
        Self {
            depth: 35.0,
            rate: 5.5,
            start: 0.25,
            fade_in: 0.2,
        }
    }
}

impl Vibrato {
    /// The same vibrato with every number in its range.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let finite = |value: f32, fallback: f32| if value.is_finite() { value } else { fallback };
        let plain = Self::default();
        Self {
            depth: finite(self.depth, plain.depth).clamp(0.0, MAX_VIBRATO_DEPTH),
            rate: finite(self.rate, plain.rate).clamp(*VIBRATO_RATES.start(), *VIBRATO_RATES.end()),
            start: finite(self.start, plain.start).clamp(0.0, 60.0),
            fade_in: finite(self.fade_in, plain.fade_in).clamp(0.0, 10.0),
        }
    }
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

/// How a voice sings one note (decision 0028). A pattern keeps one only
/// for the notes that differ from the plain way.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NoteExpression {
    /// The note's pitch curve, sorted by time; empty for the plain glide.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pitch: Vec<PitchPoint>,
    /// Its vibrato, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vibrato: Option<Vibrato>,
    /// Its colour, from −1 (darker, deeper) to 1 (lighter, brighter).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub colour: f32,
    /// Its breath, from 0 (clear) to 1.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub breath: f32,
}

impl NoteExpression {
    /// Whether this is the plain way to sing a note (nothing to keep).
    pub fn is_plain(&self) -> bool {
        self.pitch.is_empty() && self.vibrato.is_none() && self.colour == 0.0 && self.breath == 0.0
    }

    /// The note's pitch points: its own, or the plain glide.
    pub fn points(&self) -> &[PitchPoint] {
        if self.pitch.is_empty() {
            &PLAIN_GLIDE
        } else {
            &self.pitch
        }
    }

    /// The same expression with every number in its range: points sorted,
    /// at most [`MAX_PITCH_POINTS`]; a curve of fewer than two points, or
    /// the plain glide itself, kept as no curve at all.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.pitch
            .retain(|p| p.ms.is_finite() && p.cents.is_finite());
        for point in &mut self.pitch {
            point.ms = point.ms.clamp(-MAX_LEAD_MS, MAX_POINT_MS);
            point.cents = point.cents.clamp(-MAX_BEND_CENTS, MAX_BEND_CENTS);
        }
        self.pitch.sort_by(|a, b| a.ms.total_cmp(&b.ms));
        self.pitch.truncate(MAX_PITCH_POINTS);
        if self.pitch.len() < 2 || self.pitch == PLAIN_GLIDE {
            self.pitch.clear();
        }
        self.vibrato = self
            .vibrato
            .map(Vibrato::sanitized)
            .filter(|v| v.depth > 0.0);
        self.colour = if self.colour.is_finite() {
            self.colour.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.breath = if self.breath.is_finite() {
            self.breath.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }

    /// How much faster (above 1) or slower the recording is read inside
    /// each grain for this note's colour: its formants move that much.
    pub fn formant_ratio(&self) -> f64 {
        f64::from(self.colour.clamp(-1.0, 1.0) * COLOUR_SEMITONES / 12.0).exp2()
    }
}

/// How loud a voice sings a note of `velocity` (decision 0028): the
/// sampler's square curve, with the default velocity at the level it
/// always sang.
pub fn sung_gain(velocity: f32) -> f32 {
    let ratio = velocity.clamp(0.0, 1.0) / DEFAULT_VELOCITY;
    ratio * ratio
}

/// A note as a voice sings it: in seconds of the song, with whatever its
/// singer needs to know about it.
#[derive(Clone, Debug, PartialEq)]
pub struct Timed<T> {
    /// When it starts.
    pub start: f64,
    /// When it ends.
    pub end: f64,
    /// Its key.
    pub key: u8,
    /// What else goes with it (its lyric, its expression…).
    pub data: T,
}

/// A voice sings one note at a time: notes in order (by start, then key),
/// each cut short by the next one starting before it ends, then cut into
/// phrases wherever a rest of at least [`PHRASE_GAP`] falls.
pub fn phrases<T>(mut notes: Vec<Timed<T>>) -> Vec<Vec<Timed<T>>> {
    notes.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.key.cmp(&b.key)));
    for i in 1..notes.len() {
        if notes[i].start < notes[i - 1].end {
            notes[i - 1].end = notes[i].start;
        }
    }
    notes.retain(|n| n.end > n.start);
    let mut phrases: Vec<Vec<Timed<T>>> = Vec::new();
    for note in notes {
        match phrases.last_mut() {
            Some(phrase)
                if phrase
                    .last()
                    .is_some_and(|last| note.start - last.end < PHRASE_GAP) =>
            {
                phrase.push(note);
            }
            _ => phrases.push(vec![note]),
        }
    }
    phrases
}

/// One note of a phrase, as the pitch needs it.
#[derive(Clone, Copy, Debug)]
pub struct CurveNote<'a> {
    /// When it starts, in seconds of the song.
    pub start: f64,
    /// When it ends.
    pub end: f64,
    /// Its key.
    pub key: u8,
    /// How it is sung (`None`: the plain way).
    pub expression: Option<&'a NoteExpression>,
}

/// A note of a phrase with its points placed in the song.
#[derive(Clone, Debug)]
struct Placed {
    start: f64,
    end: f64,
    key: f64,
    /// When it takes over from the note before (−∞ for the first).
    takes_over: f64,
    /// When the next note takes over (+∞ for the last).
    hands_over: f64,
    /// Its points: seconds of the song, semitones from its key, shape.
    points: Vec<(f64, f64, PitchShape)>,
    vibrato: Option<Vibrato>,
}

impl Placed {
    /// How far from its key the note's curve is at `t` (semitones),
    /// vibrato aside.
    fn bend(&self, t: f64) -> f64 {
        let Some(first) = self.points.first() else {
            return 0.0;
        };
        if t <= first.0 {
            return first.1;
        }
        for pair in self.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t < b.0 {
                let span = b.0 - a.0;
                let u = if span > 0.0 { (t - a.0) / span } else { 1.0 };
                return a.1 + (b.1 - a.1) * a.2.ease(u);
            }
        }
        self.points.last().map_or(0.0, |p| p.1)
    }

    /// Where its vibrato ends: when the next note takes over, or its end.
    fn vibrato_end(&self) -> f64 {
        self.end.min(self.hands_over)
    }

    /// Its vibrato at `t`, in semitones.
    fn vibrato(&self, t: f64) -> f64 {
        let Some(vibrato) = self.vibrato else {
            return 0.0;
        };
        let since = t - (self.start + f64::from(vibrato.start));
        if since <= 0.0 {
            return 0.0;
        }
        let grow = if vibrato.fade_in > 0.0 {
            (since / f64::from(vibrato.fade_in)).min(1.0)
        } else {
            1.0
        };
        let fade = ((self.vibrato_end() - t) / VIBRATO_FADE_OUT).clamp(0.0, 1.0);
        let swing = (std::f64::consts::TAU * f64::from(vibrato.rate) * since).sin();
        f64::from(vibrato.depth) / 100.0 * grow * fade * swing
    }

    /// The pitch it sings at `t`, in semitones (a fractional key).
    fn pitch(&self, t: f64) -> f64 {
        self.key + self.bend(t) + self.vibrato(t)
    }
}

/// The pitch a voice sings through a phrase (decision 0028): each note
/// takes over at its first pitch point, gliding from wherever the voice is
/// then (the first note of a phrase uses its first point as it is), follows
/// its points and their shapes, and swings with its vibrato.
#[derive(Clone, Debug)]
pub struct PhrasePitch {
    notes: Vec<Placed>,
}

impl PhrasePitch {
    /// The pitch of a phrase of notes (in order, one at a time, as
    /// [`phrases`] gives them).
    pub fn new(notes: &[CurveNote<'_>]) -> Self {
        let mut placed: Vec<Placed> = Vec::with_capacity(notes.len());
        for note in notes {
            let plain = NoteExpression::default();
            let expression = note.expression.unwrap_or(&plain);
            let key = f64::from(note.key);
            let mut points: Vec<(f64, f64, PitchShape)> = expression
                .points()
                .iter()
                .map(|p| {
                    (
                        note.start + f64::from(p.ms) / 1000.0,
                        f64::from(p.cents) / 100.0,
                        p.shape,
                    )
                })
                .collect();
            let takes_over = if let Some(before) = placed.last() {
                // Never before the note before starts, nor after this
                // note ends; its first point moves with it.
                let low = before.start.max(before.takes_over);
                let high = note.end.max(low);
                let at = points.first().map_or(note.start, |p| p.0).clamp(low, high);
                for point in &mut points {
                    point.0 = point.0.max(at);
                }
                at
            } else {
                f64::NEG_INFINITY
            };
            if let Some(before) = placed.last_mut() {
                before.hands_over = takes_over;
            }
            if let (Some(before), Some(first)) = (placed.last(), points.first_mut()) {
                // The glide leaves from wherever the voice is.
                first.1 = before.pitch(takes_over) - key;
            }
            placed.push(Placed {
                start: note.start,
                end: note.end,
                key,
                takes_over,
                hands_over: f64::INFINITY,
                points,
                vibrato: expression.vibrato.filter(|v| v.depth > 0.0),
            });
        }
        Self { notes: placed }
    }

    /// The note in charge at `t`.
    fn note_at(&self, t: f64) -> Option<&Placed> {
        let index = self
            .notes
            .partition_point(|n| n.takes_over <= t)
            .saturating_sub(1);
        self.notes.get(index)
    }

    /// The pitch sung at `t` (seconds of the song), in semitones: a
    /// fractional key, 69 being A 440.
    pub fn at(&self, t: f64) -> f64 {
        self.note_at(t).map_or(0.0, |note| note.pitch(t))
    }

    /// Where note `index`'s pitch points are, as sung: seconds of the song
    /// and the pitch there in semitones (its first point where the voice
    /// takes it from, after a note).
    pub fn points(&self, index: usize) -> Vec<(f64, f64)> {
        self.notes.get(index).map_or_else(Vec::new, |note| {
            note.points
                .iter()
                .map(|&(t, bend, _)| (t, note.key + bend))
                .collect()
        })
    }

    /// When note `index` takes over (−∞ for the first note of the phrase).
    pub fn takes_over(&self, index: usize) -> f64 {
        self.notes
            .get(index)
            .map_or(f64::NEG_INFINITY, |note| note.takes_over)
    }

    /// Where note `index`'s vibrato handle is: where the vibrato starts,
    /// as high above the note as the vibrato is deep (seconds, semitones).
    /// `None` for a note without vibrato, or one too short to reach it.
    pub fn vibrato_handle(&self, index: usize) -> Option<(f64, f64)> {
        let note = self.notes.get(index)?;
        let vibrato = note.vibrato?;
        let at = note.start + f64::from(vibrato.start);
        (at < note.vibrato_end()).then(|| (at, note.key + f64::from(vibrato.depth) / 100.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(start: f64, end: f64, key: u8, expression: Option<&NoteExpression>) -> CurveNote<'_> {
        CurveNote {
            start,
            end,
            key,
            expression,
        }
    }

    /// The glide of decision 0027 (milestone 7a), as it was written there.
    fn glide_7a(keys: &[(f64, f64)], at: f64) -> f64 {
        let index = keys
            .partition_point(|(start, _)| start - 0.04 <= at)
            .saturating_sub(1);
        let (start, key) = keys[index];
        let Some(&(_, before)) = index.checked_sub(1).map(|i| &keys[i]) else {
            return key;
        };
        let into = (at - (start - 0.04)) / 0.08;
        if into >= 1.0 {
            return key;
        }
        let eased = 0.5 - 0.5 * (std::f64::consts::PI * into.clamp(0.0, 1.0)).cos();
        before + (key - before) * eased
    }

    #[test]
    fn notes_without_points_glide_exactly_as_before() {
        let notes = [
            note(1.0, 1.5, 60, None),
            note(1.5, 2.0, 64, None),
            note(2.0, 2.5, 67, None),
        ];
        let pitch = PhrasePitch::new(&notes);
        let keys = [(1.0, 60.0), (1.5, 64.0), (2.0, 67.0)];
        for step in 0..=2000 {
            let t = 0.9 + f64::from(step) * 0.0008;
            let expected = glide_7a(&keys, t);
            assert!(
                (pitch.at(t) - expected).abs() < 1e-9,
                "at {t}: {} against {expected}",
                pitch.at(t)
            );
        }
        // Half-way through the glide into E, the pitch is half-way there.
        assert!((pitch.at(1.5) - 62.0).abs() < 1e-9);
        assert!((pitch.takes_over(1) - 1.46).abs() < 1e-12);
    }

    #[test]
    fn a_curve_follows_its_points_and_shapes() {
        // An overshoot: the voice glides from C up past E, then settles.
        let overshoot = NoteExpression {
            pitch: vec![
                PitchPoint::new(-50.0, 0.0),
                PitchPoint {
                    ms: 50.0,
                    cents: 40.0,
                    shape: PitchShape::Straight,
                },
                PitchPoint::new(150.0, 0.0),
            ],
            ..NoteExpression::default()
        };
        let notes = [
            note(0.0, 1.0, 60, None),
            note(1.0, 2.0, 64, Some(&overshoot)),
        ];
        let pitch = PhrasePitch::new(&notes);
        assert!((pitch.at(0.9) - 60.0).abs() < 1e-9, "C until the glide");
        // The first point leaves from C (four semitones under E).
        assert!((pitch.at(0.95) - 60.0).abs() < 1e-9);
        // Smooth from C to 40 cents over E: half-way at 1.0 s.
        assert!((pitch.at(1.0) - 62.2).abs() < 1e-9);
        assert!((pitch.at(1.05) - 64.4).abs() < 1e-9, "the overshoot");
        // Straight back down: half-way at 1.1 s.
        assert!((pitch.at(1.1) - 64.2).abs() < 1e-9);
        assert!((pitch.at(1.5) - 64.0).abs() < 1e-9, "settled on E");
        let points = pitch.points(1);
        assert_eq!(points.len(), 3);
        assert!(
            (points[0].1 - 60.0).abs() < 1e-9,
            "drawn where it leaves from"
        );
    }

    #[test]
    fn the_first_note_of_a_phrase_scoops_from_where_its_point_says() {
        let scoop = NoteExpression {
            pitch: vec![PitchPoint::new(0.0, -200.0), PitchPoint::new(100.0, 0.0)],
            ..NoteExpression::default()
        };
        let pitch = PhrasePitch::new(&[note(1.0, 2.0, 67, Some(&scoop))]);
        assert!((pitch.at(0.5) - 65.0).abs() < 1e-9, "held before it");
        assert!((pitch.at(1.05) - 66.0).abs() < 1e-9);
        assert!((pitch.at(1.5) - 67.0).abs() < 1e-9);
        assert_eq!(pitch.takes_over(0), f64::NEG_INFINITY);
    }

    #[test]
    fn vibrato_starts_late_grows_and_hands_over_smoothly() {
        let vibrato = NoteExpression {
            vibrato: Some(Vibrato {
                depth: 50.0,
                rate: 5.0,
                start: 0.2,
                fade_in: 0.1,
            }),
            ..NoteExpression::default()
        };
        let notes = [note(0.0, 1.0, 60, Some(&vibrato)), note(1.0, 1.5, 62, None)];
        let pitch = PhrasePitch::new(&notes);
        assert!((pitch.at(0.15) - 60.0).abs() < 1e-12, "not yet");
        // Grown to its full depth after 0.1 s; at 5 Hz the crests come a
        // quarter of a swing in, then every 0.2 s.
        let crest = 0.2 + 0.25;
        assert!((pitch.at(crest) - 60.5).abs() < 1e-9, "{}", pitch.at(crest));
        // Gone by the time the next note takes over (40 ms before it).
        assert!((pitch.at(0.959) - 60.0).abs() < 0.01);
        // So the glide leaves from C itself.
        assert!((pitch.points(1)[0].1 - 60.0).abs() < 1e-9);
        let (at, height) = pitch.vibrato_handle(0).unwrap();
        assert!((at - 0.2).abs() < 1e-6 && (height - 60.5).abs() < 1e-6);
        // A note shorter than the vibrato's start has none.
        let short = PhrasePitch::new(&[note(0.0, 0.15, 60, Some(&vibrato))]);
        assert!(short.vibrato_handle(0).is_none());
        assert!((short.at(0.1) - 60.0).abs() < 1e-12);
    }

    #[test]
    fn a_take_over_never_jumps_even_from_a_swinging_note() {
        // Vibrato right to the end, and a next note that takes over early.
        let vibrato = NoteExpression {
            vibrato: Some(Vibrato {
                depth: 80.0,
                rate: 7.0,
                start: 0.0,
                fade_in: 0.0,
            }),
            ..NoteExpression::default()
        };
        let early = NoteExpression {
            pitch: vec![PitchPoint::new(-300.0, 0.0), PitchPoint::new(0.0, 0.0)],
            ..NoteExpression::default()
        };
        let notes = [
            note(0.0, 1.0, 60, Some(&vibrato)),
            note(1.0, 2.0, 65, Some(&early)),
        ];
        let pitch = PhrasePitch::new(&notes);
        let mut last = pitch.at(0.0);
        for step in 1..=20_000 {
            let t = f64::from(step) * 0.0001;
            let now = pitch.at(t);
            assert!((now - last).abs() < 0.05, "a jump at {t}: {last} to {now}");
            last = now;
        }
    }

    #[test]
    fn notes_fall_into_phrases_one_at_a_time() {
        let timed = |start: f64, end: f64, key: u8| Timed {
            start,
            end,
            key,
            data: key,
        };
        let sung = phrases(vec![
            timed(2.0, 2.5, 64),
            timed(0.0, 1.2, 60),
            timed(1.0, 1.5, 62),
            // A rest of 0.5 s, then a note of its own.
            timed(3.0, 3.5, 67),
        ]);
        assert_eq!(sung.len(), 3);
        assert_eq!(sung[0].len(), 2);
        assert_eq!(sung[0][0].end, 1.0, "cut short by the next note");
        assert_eq!(sung[1][0].data, 64);
        assert_eq!(sung[2][0].data, 67);
    }

    #[test]
    fn expressions_keep_their_numbers_in_range() {
        let wild = NoteExpression {
            pitch: vec![
                PitchPoint::new(500.0, 9000.0),
                PitchPoint::new(f32::NAN, 0.0),
                PitchPoint::new(-5000.0, 0.0),
            ],
            vibrato: Some(Vibrato {
                depth: 1000.0,
                rate: 0.0,
                start: -1.0,
                fade_in: f32::INFINITY,
            }),
            colour: 3.0,
            breath: -1.0,
        }
        .sanitized();
        assert_eq!(wild.pitch.len(), 2);
        assert_eq!(wild.pitch[0].ms, -MAX_LEAD_MS, "sorted and limited");
        assert_eq!(wild.pitch[1].cents, MAX_BEND_CENTS);
        let vibrato = wild.vibrato.unwrap();
        assert_eq!(vibrato.depth, MAX_VIBRATO_DEPTH);
        assert_eq!(vibrato.rate, *VIBRATO_RATES.start());
        assert_eq!((vibrato.start, vibrato.fade_in), (0.0, 0.2));
        assert_eq!((wild.colour, wild.breath), (1.0, 0.0));
        // The plain glide, or a single point, is no curve at all.
        let plain = NoteExpression {
            pitch: PLAIN_GLIDE.to_vec(),
            ..NoteExpression::default()
        };
        assert!(plain.sanitized().is_plain());
        let lone = NoteExpression {
            pitch: vec![PitchPoint::new(0.0, 50.0)],
            ..NoteExpression::default()
        };
        assert!(lone.sanitized().is_plain());
        let flat = NoteExpression {
            vibrato: Some(Vibrato {
                depth: 0.0,
                ..Vibrato::default()
            }),
            ..NoteExpression::default()
        };
        assert!(flat.sanitized().is_plain(), "a vibrato of no depth is none");
    }

    #[test]
    fn velocity_and_colour_have_their_curves() {
        assert!((sung_gain(DEFAULT_VELOCITY) - 1.0).abs() < 1e-6);
        assert!((sung_gain(1.0) - 1.613).abs() < 1e-3);
        assert!(sung_gain(0.0).abs() < 1e-9);
        let lighter = NoteExpression {
            colour: 1.0,
            ..NoteExpression::default()
        };
        assert!((lighter.formant_ratio() - 2.0_f64.powf(1.0 / 3.0)).abs() < 1e-6);
        assert!((NoteExpression::default().formant_ratio() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn shapes_leave_and_arrive_as_named() {
        for shape in PitchShape::ALL {
            assert!(shape.ease(0.0).abs() < 1e-12 && (shape.ease(1.0) - 1.0).abs() < 1e-12);
        }
        assert!(PitchShape::SlowStart.ease(0.5) < 0.5);
        assert!(PitchShape::SlowEnd.ease(0.5) > 0.5);
        assert!((PitchShape::Smooth.ease(0.5) - 0.5).abs() < 1e-12);
    }
}
