// SPDX-License-Identifier: GPL-3.0-or-later

//! Automation: settings that change over the song (decision 0015). A lane
//! belongs to a track and changes one of its settings: volume, pan, a
//! setting of one of its effects, or a parameter of its plugin instrument.
//! Its points give values from 0 to 1 at moments of the song; between
//! points the value moves in a straight line.

use serde::{Deserialize, Serialize};

use crate::song::EffectId;
use crate::time::Ticks;

/// What a lane changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AutomationTarget {
    /// The track's volume.
    Volume,
    /// The track's position from left to right.
    Pan,
    /// A setting of one of the track's effects: for a built-in effect the
    /// setting's number (see [`crate::EffectKind::params`]), for a plugin
    /// effect its parameter's id.
    Effect {
        /// The effect.
        effect: EffectId,
        /// The setting.
        param: u32,
    },
    /// A parameter of the track's plugin instrument, by its id.
    Instrument {
        /// The parameter.
        param: u32,
    },
}

/// A point of a lane.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationPoint {
    /// When.
    pub at: Ticks,
    /// The value then, from 0 to 1.
    pub value: f32,
}

/// One setting's changes over the song.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutomationLane {
    /// What it changes.
    pub target: AutomationTarget,
    /// Its points, in time order.
    #[serde(default)]
    pub points: Vec<AutomationPoint>,
}

impl AutomationLane {
    /// An empty lane.
    pub fn new(target: AutomationTarget) -> Self {
        Self {
            target,
            points: Vec::new(),
        }
    }

    /// The value at a moment (in ticks, fractional): the first point's value
    /// before it, the last point's after it, a straight line in between.
    /// `None` if the lane has no points.
    pub fn value_at(&self, at: f64) -> Option<f32> {
        let points = &self.points;
        let first = points.first()?;
        #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
        let tick = |p: &AutomationPoint| p.at.0 as f64;
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

    /// Adds a point, keeping time order; a point already at that moment is
    /// replaced. Returns the point's index.
    pub fn insert(&mut self, point: AutomationPoint) -> usize {
        let point = AutomationPoint {
            at: point.at.max(Ticks::ZERO),
            value: point.value.clamp(0.0, 1.0),
        };
        match self.points.binary_search_by_key(&point.at, |p| p.at) {
            Ok(index) => {
                self.points[index] = point;
                index
            }
            Err(index) => {
                self.points.insert(index, point);
                index
            }
        }
    }

    /// Puts the points back in time order (after moving one).
    pub fn sort(&mut self) {
        self.points.sort_by_key(|p| p.at);
    }
}

/// The quietest volume a lane reaches, in dB (silence below).
const FLOOR_DB: f32 = -60.0;
/// The loudest volume a lane reaches, in dB.
const TOP_DB: f32 = 6.0;

/// A volume lane's value (0 to 1) as linear gain, on the mixer fader's
/// scale (-60 dB to +6 dB, silence at 0).
pub fn volume_gain(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else {
        10.0_f32.powf((FLOOR_DB + value.min(1.0) * (TOP_DB - FLOOR_DB)) / 20.0)
    }
}

/// Linear gain as a volume lane's value (the inverse of [`volume_gain`]).
pub fn gain_volume(gain: f32) -> f32 {
    if gain <= 0.001 {
        0.0
    } else {
        ((20.0 * gain.log10() - FLOOR_DB) / (TOP_DB - FLOOR_DB)).clamp(0.0, 1.0)
    }
}

/// A pan lane's value (0 to 1) as a pan position (-1 left to 1 right).
pub fn value_pan(value: f32) -> f32 {
    (value.clamp(0.0, 1.0) * 2.0 - 1.0).clamp(-1.0, 1.0)
}

/// A pan position as a pan lane's value.
pub fn pan_value(pan: f32) -> f32 {
    f32::midpoint(pan.clamp(-1.0, 1.0), 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane(points: &[(i64, f32)]) -> AutomationLane {
        AutomationLane {
            target: AutomationTarget::Volume,
            points: points
                .iter()
                .map(|&(at, value)| AutomationPoint {
                    at: Ticks(at),
                    value,
                })
                .collect(),
        }
    }

    #[test]
    fn values_follow_straight_lines_between_points() {
        assert_eq!(lane(&[]).value_at(10.0), None);
        let l = lane(&[(100, 0.2), (200, 0.6), (300, 0.6)]);
        assert!(
            (l.value_at(0.0).unwrap() - 0.2).abs() < 1e-6,
            "before: first"
        );
        assert!((l.value_at(150.0).unwrap() - 0.4).abs() < 1e-6, "half way");
        assert!((l.value_at(250.0).unwrap() - 0.6).abs() < 1e-6, "flat");
        assert!(
            (l.value_at(1000.0).unwrap() - 0.6).abs() < 1e-6,
            "after: last"
        );
    }

    #[test]
    fn points_stay_in_time_order() {
        let mut l = lane(&[(100, 0.2), (300, 0.6)]);
        assert_eq!(
            l.insert(AutomationPoint {
                at: Ticks(200),
                value: 2.0
            }),
            1
        );
        assert!((l.points[1].value - 1.0).abs() < 1e-6, "values are clamped");
        assert_eq!(
            l.insert(AutomationPoint {
                at: Ticks(100),
                value: 0.5
            }),
            0
        );
        assert_eq!(l.points.len(), 3, "a point at the same moment is replaced");
    }

    #[test]
    fn volume_and_pan_match_the_mixer() {
        assert_eq!(volume_gain(0.0), 0.0);
        let unity = gain_volume(1.0);
        assert!((volume_gain(unity) - 1.0).abs() < 1e-4, "0 dB round trips");
        assert!((volume_gain(1.0) - 2.0).abs() < 0.01, "+6 dB at the top");
        assert!((value_pan(pan_value(-0.3)) + 0.3).abs() < 1e-6);
    }
}
