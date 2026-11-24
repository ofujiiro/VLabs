// SPDX-License-Identifier: GPL-3.0-or-later

//! What a MIDI controller's wheels, pedals and knobs did in a pattern
//! (decision 0033): one curve per controller, beside the notes. A take
//! records them; the piano roll's lane draws and edits them; they repeat
//! and swing with the pattern's notes.

use serde::{Deserialize, Serialize};

use crate::Ticks;

/// What a curve moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    /// The pitch wheel: values from -1 (all the way down) to 1, 0 at rest.
    PitchBend,
    /// A numbered controller (1 is the modulation wheel, 11 the expression
    /// pedal): values from 0 to 1.
    Controller(u8),
    /// How hard the keys are pressed once down (channel aftertouch): values
    /// from 0 to 1.
    Pressure,
}

/// The modulation wheel.
pub const MODULATION: ControlKind = ControlKind::Controller(1);
/// The expression pedal.
pub const EXPRESSION: ControlKind = ControlKind::Controller(11);

/// The most points one curve keeps (a damaged file must not exhaust
/// memory; a recorded gesture keeps a handful).
pub const MAX_CONTROL_POINTS: usize = 100_000;

impl ControlKind {
    /// The lowest and highest value.
    pub fn range(self) -> (f32, f32) {
        match self {
            Self::PitchBend => (-1.0, 1.0),
            Self::Controller(_) | Self::Pressure => (0.0, 1.0),
        }
    }

    /// A value brought into the range (anything not a number is the
    /// resting value, or the bottom).
    pub fn clamp(self, value: f32) -> f32 {
        let (low, high) = self.range();
        if value.is_finite() {
            value.clamp(low, high)
        } else {
            self.rest().unwrap_or(low)
        }
    }

    /// Where it rests when nobody touches it, if it has a resting place:
    /// the wheel centred, no modulation, no pressure, full expression. Other
    /// controllers keep their last value, as MIDI does.
    pub fn rest(self) -> Option<f32> {
        match self {
            Self::PitchBend | Self::Pressure | MODULATION => Some(0.0),
            EXPRESSION => Some(1.0),
            Self::Controller(_) => None,
        }
    }

    /// Whether a take records it: everything but the sustain pedal (which
    /// lengthens the notes instead), bank select and the channel mode
    /// messages.
    pub fn is_recordable(self) -> bool {
        !matches!(self, Self::Controller(0 | 32 | 64 | 120..=127))
    }
}

/// A point of a curve.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlPoint {
    /// Where, in the pattern.
    pub at: Ticks,
    /// The value there (see [`ControlKind::range`]).
    pub value: f32,
}

/// One controller's curve in a pattern: points in time order, a straight
/// line between them; before the first point the curve holds its value,
/// after the last too.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlLane {
    /// What it moves.
    pub control: ControlKind,
    /// Its points, in time order.
    #[serde(default)]
    pub points: Vec<ControlPoint>,
}

impl ControlLane {
    /// A lane of `points`, put in order and range: sorted by time, values
    /// clamped, at most one point at each tick (the last given wins), no
    /// more than [`MAX_CONTROL_POINTS`].
    pub fn new(control: ControlKind, points: Vec<ControlPoint>) -> Self {
        let mut lane = Self { control, points };
        lane.tidy();
        lane
    }

    /// Puts the points in order and range (see [`Self::new`]).
    pub fn tidy(&mut self) {
        let control = self.control;
        for point in &mut self.points {
            point.value = control.clamp(point.value);
            point.at = point.at.max(Ticks::ZERO);
        }
        // Stable: of points at the same tick, the last given stays.
        self.points.sort_by_key(|p| p.at);
        let mut kept: Vec<ControlPoint> = Vec::with_capacity(self.points.len());
        for point in self.points.drain(..) {
            match kept.last_mut() {
                Some(last) if last.at == point.at => *last = point,
                _ => kept.push(point),
            }
        }
        kept.truncate(MAX_CONTROL_POINTS);
        self.points = kept;
    }

    /// The value at a moment of the pattern (in ticks, fractional): the
    /// first point's before it, the last's after it, a straight line in
    /// between; `None` with no points.
    pub fn value_at(&self, at: f64) -> Option<f32> {
        value_at(&self.points, at)
    }

    /// Replaces the points from `from` to `to` (both included) with
    /// `points` (a take's, which lie in that span): what the take played
    /// stands, the rest of the curve stays.
    pub fn replace_span(&mut self, from: Ticks, to: Ticks, points: &[ControlPoint]) {
        self.points.retain(|p| p.at < from || p.at > to);
        self.points.extend_from_slice(points);
        self.tidy();
    }
}

/// The value of a curve of `points` (in time order) at `at` (fractional
/// ticks): held before the first and after the last, a straight line in
/// between.
pub fn value_at(points: &[ControlPoint], at: f64) -> Option<f32> {
    let first = points.first()?;
    #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
    let tick = |p: &ControlPoint| p.at.0 as f64;
    if at <= tick(first) {
        return Some(first.value);
    }
    let next = points.partition_point(|p| tick(p) <= at);
    let Some(after) = points.get(next) else {
        return points.last().map(|p| p.value);
    };
    let before = &points[next - 1];
    let span = tick(after) - tick(before);
    if span <= 0.0 {
        return Some(after.value);
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a fraction between 0 and 1"
    )]
    let t = ((at - tick(before)) / span) as f32;
    Some(before.value + (after.value - before.value) * t)
}

/// Keeps the points a straight line between their neighbours does not
/// predict within `tolerance` (in the value's units): a recorded gesture
/// of hundreds of points keeps the handful that shape it. The first and
/// last points always stay.
pub fn thin(points: &[ControlPoint], tolerance: f32) -> Vec<ControlPoint> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    // Ramer–Douglas–Peucker, measuring how far each point is from the
    // straight line in value (not in distance on screen), without
    // recursion.
    let mut spans = vec![(0, points.len() - 1)];
    while let Some((first, last)) = spans.pop() {
        if last <= first + 1 {
            continue;
        }
        let (a, b) = (points[first], points[last]);
        #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
        let line = |p: &ControlPoint| {
            let span = (b.at.0 - a.at.0) as f32;
            if span <= 0.0 {
                a.value
            } else {
                a.value + (b.value - a.value) * (p.at.0 - a.at.0) as f32 / span
            }
        };
        let (farthest, distance) = (first + 1..last)
            .map(|i| (i, (points[i].value - line(&points[i])).abs()))
            .fold(
                (first, 0.0_f32),
                |best, next| {
                    if next.1 > best.1 { next } else { best }
                },
            );
        if distance > tolerance {
            keep[farthest] = true;
            spans.push((first, farthest));
            spans.push((farthest, last));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, kept)| kept.then_some(*p))
        .collect()
}

/// How closely a recorded curve is kept: a little under one step of a
/// seven-bit controller, a fraction of a cent on the pitch wheel's usual
/// range (two semitones).
pub fn thinning_tolerance(control: ControlKind) -> f32 {
    let (low, high) = control.range();
    (high - low) * 0.006
}

/// A knob, fader, button or pad of a MIDI controller, as it identifies
/// itself (decision 0033): a channel and what it sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MidiControl {
    /// The MIDI channel, 0 to 15.
    pub channel: u8,
    /// What it sends.
    pub source: MidiSource,
}

/// What a control of a MIDI controller sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidiSource {
    /// A numbered controller: knobs, faders and most buttons.
    Controller(u8),
    /// A key: pads, and some buttons.
    Note(u8),
}

impl MidiControl {
    /// The same control with its numbers in MIDI's ranges.
    #[must_use]
    pub fn sane(self) -> Self {
        Self {
            channel: self.channel & 0x0F,
            source: match self.source {
                MidiSource::Controller(n) => MidiSource::Controller(n & 0x7F),
                MidiSource::Note(n) => MidiSource::Note(n & 0x7F),
            },
        }
    }
}

/// A setting of the song that a knob or fader can move (decision 0033).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "on")]
pub enum LinkTarget {
    /// A track's volume, pan, a built-in effect's setting or the synth's
    /// setting, as automation names them.
    Track {
        /// The track.
        track: crate::TrackId,
        /// The setting.
        setting: crate::AutomationTarget,
    },
    /// The master's volume.
    MasterVolume,
    /// A setting of a built-in effect on the master.
    MasterEffect {
        /// The effect.
        effect: crate::EffectId,
        /// The setting's number (see [`crate::EffectKind::params`]).
        param: u32,
    },
}

/// A knob or fader of a MIDI controller given a setting of the song to
/// move (decision 0033).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiLink {
    /// The control.
    pub control: MidiControl,
    /// What it moves.
    pub target: LinkTarget,
}

#[cfg(test)]
#[allow(clippy::cast_precision_loss, reason = "test values")]
mod tests {
    use super::*;

    fn point(at: i64, value: f32) -> ControlPoint {
        ControlPoint {
            at: Ticks(at),
            value,
        }
    }

    #[test]
    fn a_curve_holds_before_and_after_and_is_straight_between() {
        let lane = ControlLane::new(
            ControlKind::PitchBend,
            vec![point(100, 0.0), point(300, 1.0), point(200, 0.25)],
        );
        assert_eq!(lane.value_at(0.0), Some(0.0));
        assert_eq!(lane.value_at(150.0), Some(0.125));
        assert_eq!(lane.value_at(250.0), Some(0.625));
        assert_eq!(lane.value_at(1000.0), Some(1.0));
        assert_eq!(ControlLane::new(MODULATION, Vec::new()).value_at(0.0), None);
    }

    #[test]
    fn points_are_tidied_into_order_range_and_one_per_tick() {
        let lane = ControlLane::new(
            MODULATION,
            vec![
                point(50, 2.0),
                point(-10, 0.5),
                point(50, 0.25),
                point(20, f32::NAN),
            ],
        );
        assert_eq!(
            lane.points,
            [point(0, 0.5), point(20, 0.0), point(50, 0.25)],
            "sorted, clamped, the last at a tick kept, nothing below zero"
        );
        let bend = ControlLane::new(ControlKind::PitchBend, vec![point(0, -3.0)]);
        assert_eq!(bend.points[0].value, -1.0);
    }

    #[test]
    fn a_take_replaces_its_span_only() {
        let mut lane = ControlLane::new(
            MODULATION,
            vec![
                point(0, 0.1),
                point(100, 0.2),
                point(200, 0.3),
                point(300, 0.4),
            ],
        );
        lane.replace_span(Ticks(100), Ticks(200), &[point(120, 0.9), point(180, 0.8)]);
        assert_eq!(
            lane.points,
            [
                point(0, 0.1),
                point(120, 0.9),
                point(180, 0.8),
                point(300, 0.4)
            ]
        );
    }

    #[test]
    fn thinning_keeps_the_shape_of_a_gesture() {
        // A wheel pushed up in a straight line, held, then let go: hundreds
        // of points, four that matter.
        let mut points = Vec::new();
        for i in 0..=100 {
            points.push(point(i * 10, i as f32 / 100.0));
        }
        for i in 101..=200 {
            points.push(point(i * 10, 1.0));
        }
        for i in 201..=210 {
            points.push(point(i * 10, 1.0 - (i - 200) as f32 / 10.0));
        }
        let thinned = thin(&points, thinning_tolerance(ControlKind::PitchBend));
        assert_eq!(
            thinned,
            [
                point(0, 0.0),
                point(1000, 1.0),
                point(2000, 1.0),
                point(2100, 0.0)
            ]
        );
        // Every original point lies on the thinned curve.
        for p in &points {
            #[allow(clippy::cast_precision_loss, reason = "test values")]
            let at = p.at.0 as f64;
            let error = (value_at(&thinned, at).unwrap() - p.value).abs();
            assert!(error < 0.013, "{p:?} is {error} away");
        }
        assert_eq!(thin(&points[..2], 0.1).len(), 2);
    }

    #[test]
    fn resting_places_and_what_a_take_records() {
        assert_eq!(ControlKind::PitchBend.rest(), Some(0.0));
        assert_eq!(MODULATION.rest(), Some(0.0));
        assert_eq!(EXPRESSION.rest(), Some(1.0));
        assert_eq!(ControlKind::Controller(74).rest(), None);
        assert!(ControlKind::Controller(74).is_recordable());
        assert!(
            !ControlKind::Controller(64).is_recordable(),
            "the pedal lengthens notes"
        );
        assert!(!ControlKind::Controller(0).is_recordable());
    }

    #[test]
    fn links_are_kept_readably_in_the_song_file() {
        let link = MidiLink {
            control: MidiControl {
                channel: 0,
                source: MidiSource::Controller(74),
            },
            target: LinkTarget::Track {
                track: crate::TrackId(3),
                setting: crate::AutomationTarget::Volume,
            },
        };
        let json = serde_json::to_string(&link).unwrap();
        assert_eq!(
            json,
            r#"{"control":{"channel":0,"source":{"controller":74}},"target":{"on":"track","track":3,"setting":{"type":"volume"}}}"#
        );
        assert_eq!(serde_json::from_str::<MidiLink>(&json).unwrap(), link);
        let master: LinkTarget = serde_json::from_str(r#"{"on":"master_volume"}"#).unwrap();
        assert_eq!(master, LinkTarget::MasterVolume);
    }

    #[test]
    fn curves_are_kept_readably_in_the_song_file() {
        let lane = ControlLane::new(ControlKind::Controller(74), vec![point(10, 0.5)]);
        let json = serde_json::to_string(&lane).unwrap();
        assert_eq!(
            json,
            r#"{"control":{"controller":74},"points":[{"at":10,"value":0.5}]}"#
        );
        let bend: ControlKind = serde_json::from_str(r#""pitch_bend""#).unwrap();
        assert_eq!(bend, ControlKind::PitchBend);
    }
}
