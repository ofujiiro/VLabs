// SPDX-License-Identifier: GPL-3.0-or-later

//! MIDI controllers' knobs and faders given settings of the song (decision
//! 0033): waiting for a knob, assigning it, moving the setting with it.

use std::time::{Duration, Instant};

use daw_model::{AutomationTarget, LinkTarget, MidiControl};

use super::Session;
use crate::text;

/// Knob moves this far apart are separate undo steps.
const TURN_PAUSE: Duration = Duration::from_secs(1);

/// Undo merge keys of knob turns count from here, far from the takes'.
pub(crate) const FIRST_TURN: u64 = 1 << 48;

/// A knob being turned: its undo merge key, and when it last moved.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Turn {
    control: MidiControl,
    key: u64,
    at: Instant,
}

impl Session {
    /// Starts waiting for a knob or fader of a MIDI controller to give
    /// `target` to (the next one moved gets it).
    pub fn learn_midi(&mut self, target: LinkTarget) {
        self.midi_learning = Some(target);
        self.set_status(text::WAITING_FOR_KNOB);
    }

    /// The setting waiting for a knob, if one is.
    pub fn learning_midi(&self) -> Option<LinkTarget> {
        self.midi_learning
    }

    /// Stops waiting for a knob. Returns whether it was waiting.
    pub fn stop_learning_midi(&mut self) -> bool {
        let was = self.midi_learning.take().is_some();
        if was {
            self.set_status(text::STOPPED_WAITING_FOR_KNOB);
        }
        was
    }

    /// The knob or fader that moves a setting, if one does.
    pub fn midi_control_of(&self, target: LinkTarget) -> Option<MidiControl> {
        self.project().midi_control_of(target)
    }

    /// Lets go of the knob that moves a setting (an undoable edit).
    pub fn forget_midi(&mut self, target: LinkTarget) {
        if self.midi_control_of(target).is_none() {
            return;
        }
        self.doc
            .edit(text::EDIT_FORGET_MIDI, |p| p.forget_midi(target));
    }

    /// The song's assignments whose settings are still there: the control
    /// and what it moves.
    pub fn midi_links(&self) -> Vec<daw_model::MidiLink> {
        let project = self.project();
        project
            .midi_links
            .iter()
            .filter(|l| project.has_link_target(l.target))
            .copied()
            .collect()
    }

    /// A setting's name as the person knows it: "Piano · Volume",
    /// "Lead · Synth · Cutoff", "Master · Reverb · Amount".
    pub fn link_name(&mut self, target: LinkTarget) -> String {
        match target {
            LinkTarget::Track { track, setting } => {
                let owner = self
                    .project()
                    .track(track)
                    .map_or_else(String::new, |t| t.name.clone());
                let setting = self.lane_name(track, setting);
                text::lane_name(&owner, &setting)
            }
            LinkTarget::MasterVolume => text::lane_name(text::MASTER, text::VOLUME),
            LinkTarget::MasterEffect { effect, param } => {
                let Some(kind) = self.project().effect(effect).map(|e| e.kind.clone()) else {
                    return String::new();
                };
                let setting = text::lane_name(
                    &text::effect_name(&kind),
                    text::effect_param_name(&kind, param as usize),
                );
                text::lane_name(text::MASTER, &setting)
            }
        }
    }

    /// A knob or fader moved while the session waits for one: it gets the
    /// waiting setting (one undoable edit). Returns whether it was taken.
    pub fn midi_learned(&mut self, control: MidiControl) -> bool {
        let Some(target) = self.midi_learning.take() else {
            return false;
        };
        if !self.project().has_link_target(target) {
            return false;
        }
        self.doc
            .edit(text::EDIT_ASSIGN_MIDI, |p| p.link_midi(control, target));
        let name = self.link_name(target);
        self.set_status(text::midi_learned(control, &name));
        true
    }

    /// A knob or fader with a setting moved to `value` (0 to 127): the
    /// setting follows. Each burst of moves is one undo step. Returns
    /// whether the control moves a setting of the song.
    pub fn midi_moved(&mut self, control: MidiControl, value: u8) -> bool {
        let targets: Vec<LinkTarget> = self
            .project()
            .midi_links
            .iter()
            .filter(|l| l.control == control)
            .map(|l| l.target)
            .collect();
        if targets.is_empty() {
            return false;
        }
        let now = Instant::now();
        let key = match self.midi_turn {
            Some(turn) if turn.control == control && now - turn.at < TURN_PAUSE => turn.key,
            _ => {
                self.next_turn += 1;
                self.next_turn
            }
        };
        self.midi_turn = Some(Turn {
            control,
            key,
            at: now,
        });
        let fraction = f32::from(value.min(127)) / 127.0;
        let label = match targets.first() {
            Some(
                LinkTarget::Track {
                    setting: AutomationTarget::Volume,
                    ..
                }
                | LinkTarget::MasterVolume,
            ) => text::EDIT_MIDI_VOLUME,
            _ => text::EDIT_MIDI_SETTING,
        };
        self.doc.edit_merged(label, key, |p| {
            for target in &targets {
                p.set_linked(*target, fraction);
            }
        });
        true
    }
}
