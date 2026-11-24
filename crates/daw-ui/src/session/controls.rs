// SPDX-License-Identifier: GPL-3.0-or-later

//! A pattern's controller curves (decision 0033): recorded with a take's
//! notes, drawn and edited in the piano roll's lane.

use daw_model::controls::{thin, thinning_tolerance};
use daw_model::{ControlKind, ControlPoint, Ticks};

use super::Session;
use crate::text;

/// One controller's move during a take, in the pattern's time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RecordedMove {
    pub control: ControlKind,
    pub at: Ticks,
    pub value: f32,
}

impl Session {
    /// Whether the instrument in focus answers a controller's wheels and
    /// pedals, so its pattern's lane offers their curves.
    pub fn takes_controls(&self) -> bool {
        self.project()
            .track(self.track)
            .is_some_and(|t| t.instrument.takes_controls())
    }

    /// The curves the pattern in focus holds, by controller.
    pub fn control_lanes(&self) -> Vec<ControlKind> {
        self.pattern().controls.iter().map(|l| l.control).collect()
    }

    /// A controller's curve in the pattern in focus (no points if it has
    /// none).
    pub fn control_points(&self, control: ControlKind) -> Vec<ControlPoint> {
        self.pattern()
            .control(control)
            .map_or_else(Vec::new, |lane| lane.points.clone())
    }

    /// Replaces a controller's curve in the pattern in focus (no points
    /// removes it): one undo step, or part of the gesture under way.
    pub fn set_control_points(&mut self, control: ControlKind, points: Vec<ControlPoint>) {
        let pattern = self.pattern;
        if self.control_points(control) == points {
            return;
        }
        self.doc.edit(text::EDIT_CONTROL_CURVE, |p| {
            if let Some(pattern) = p.pattern_mut(pattern) {
                pattern.set_control(control, points);
            }
        });
    }

    /// Adds a point to a curve (replacing one at the same tick); returns
    /// where it landed among the curve's points.
    pub fn add_control_point(&mut self, control: ControlKind, at: Ticks, value: f32) -> usize {
        let mut points = self.control_points(control);
        let at = at.max(Ticks::ZERO);
        points.retain(|p| p.at != at);
        let value = control.clamp(value);
        let index = points.partition_point(|p| p.at < at);
        points.insert(index, ControlPoint { at, value });
        self.set_control_points(control, points);
        index
    }

    /// Moves a curve's point, keeping it between its neighbours; returns
    /// where it is now.
    pub fn move_control_point(
        &mut self,
        control: ControlKind,
        index: usize,
        at: Ticks,
        value: f32,
    ) -> usize {
        let mut points = self.control_points(control);
        if index >= points.len() {
            return index;
        }
        let low = index
            .checked_sub(1)
            .map_or(Ticks::ZERO, |i| points[i].at + Ticks(1));
        let high = points.get(index + 1).map(|p| p.at - Ticks(1));
        let mut at = at.max(low);
        if let Some(high) = high {
            at = at.min(high.max(low));
        }
        points[index] = ControlPoint {
            at,
            value: control.clamp(value),
        };
        self.set_control_points(control, points);
        index
    }

    /// Removes a curve's point (the last removes the curve).
    pub fn remove_control_point(&mut self, control: ControlKind, index: usize) {
        let mut points = self.control_points(control);
        if index < points.len() {
            points.remove(index);
            self.set_control_points(control, points);
        }
    }

    /// Takes a controller's move during a take (from the engine, in song
    /// time): kept in the pattern's time, where it plays back as heard in a
    /// swung pattern (decision 0018).
    pub(crate) fn add_recorded_move(&mut self, control: ControlKind, value: f32, at: Ticks) {
        // Drums and voices neither play nor show curves: nothing to keep.
        if !self.takes_controls() {
            return;
        }
        let placement = self.placement();
        let length = self.pattern().length;
        let swing = self.pattern().swing;
        let played = placement.to_pattern_folded(at, length);
        let at = swing.remove(played).max(Ticks::ZERO);
        self.recorded_moves.push(RecordedMove {
            control,
            at,
            value: control.clamp(value),
        });
    }

    /// Writes the take's controller moves into the pattern, thinned, each
    /// pass over a looping pattern replacing what it covered; in the take's
    /// undo step (named `label`). Returns whether anything was written.
    pub(crate) fn write_recorded_moves(&mut self, merge_key: u64, label: &str) -> bool {
        let moves = std::mem::take(&mut self.recorded_moves);
        if moves.is_empty() {
            return false;
        }
        let pattern = self.pattern;
        let mut controls: Vec<ControlKind> = moves.iter().map(|m| m.control).collect();
        controls.sort();
        controls.dedup();
        let mut lanes = Vec::new();
        for control in controls {
            let mut lane = self
                .pattern()
                .control(control)
                .cloned()
                .unwrap_or_else(|| daw_model::ControlLane::new(control, Vec::new()));
            // A pass ends where the pattern starts again (the take's time
            // goes back).
            let mut pass: Vec<ControlPoint> = Vec::new();
            let mut passes = Vec::new();
            for m in moves.iter().filter(|m| m.control == control) {
                if pass.last().is_some_and(|p| m.at < p.at) {
                    passes.push(std::mem::take(&mut pass));
                }
                // One point per tick: the last move there.
                match pass.last_mut() {
                    Some(last) if last.at == m.at => last.value = m.value,
                    _ => pass.push(ControlPoint {
                        at: m.at,
                        value: m.value,
                    }),
                }
            }
            passes.push(pass);
            for pass in passes {
                let (Some(first), Some(last)) = (pass.first(), pass.last()) else {
                    continue;
                };
                let (from, to) = (first.at, last.at);
                let thinned = thin(&pass, thinning_tolerance(control));
                lane.replace_span(from, to, &thinned);
            }
            lanes.push(lane);
        }
        self.doc.edit_merged(label, merge_key, |p| {
            if let Some(pattern) = p.pattern_mut(pattern) {
                for lane in lanes {
                    pattern.set_control(lane.control, lane.points);
                }
            }
        });
        true
    }
}
