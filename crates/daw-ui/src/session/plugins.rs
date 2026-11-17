// SPDX-License-Identifier: GPL-3.0-or-later

//! Plugins and automation in the song (decision 0016): the plugins found on
//! this computer, tracks and effects that use them, their own windows,
//! keeping their settings with the song, and lanes that change a setting
//! over the song. Every change to the song is one undo step.

use daw_engine::{PluginInfo, PluginKind, PluginTarget, plugin_ref};
use daw_model::{
    AutomationLane, AutomationPoint, AutomationTarget, ChainPlace, EffectId, EffectKind,
    Instrument, PluginSetup, Ticks, TrackId, gain_volume, pan_value, value_pan, volume_gain,
};

use super::Session;
use super::arrange::PIANO_BARS;
use crate::text;
use crate::widgets::gain_to_db;

/// Where the search for plugins stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginSearch {
    /// Whether it has finished.
    pub done: bool,
    /// Plugin files looked at so far.
    pub checked: usize,
    /// Plugin files to look at.
    pub total: usize,
    /// Files that cannot be used, each with the reason.
    pub problems: Vec<(std::path::PathBuf, daw_engine::ScanProblem)>,
}

/// A setting a lane can change, as the lane chooser offers it.
#[derive(Clone, Debug, PartialEq)]
pub struct LaneChoice {
    /// The setting.
    pub target: AutomationTarget,
    /// Its name, after what it belongs to ("Reverb · Amount").
    pub name: String,
    /// It is the one last moved in a plugin's window.
    pub last_moved: bool,
}

impl Session {
    // ----- Finding plugins

    /// The plugins found on this computer so far, and where the search
    /// stands.
    pub fn set_plugins(&mut self, plugins: Vec<PluginInfo>, search: PluginSearch) {
        if plugins != self.engine.plugins() {
            self.engine.set_plugins(plugins);
        }
        self.plugin_search = search;
    }

    /// Where the search for plugins stands.
    pub fn plugin_search(&self) -> &PluginSearch {
        &self.plugin_search
    }

    /// The plugins of a kind, by name (the CLAP one first when a plugin
    /// comes in both formats).
    pub fn plugins_of(&self, kind: PluginKind) -> Vec<PluginInfo> {
        let mut found: Vec<PluginInfo> = self
            .engine
            .plugins()
            .iter()
            .filter(|p| p.kind == kind)
            .cloned()
            .collect();
        found.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.format.cmp(&b.format))
        });
        found
    }

    // ----- Plugin tracks and effects

    /// Adds a track playing a plugin instrument, named after it, with an
    /// empty four-bar pattern at the start of the song, and puts it in
    /// focus.
    pub fn add_plugin_track(&mut self, info: &PluginInfo) -> TrackId {
        let name = self.unique_track_name(&info.name);
        let instrument = Instrument::Plugin(PluginSetup::new(plugin_ref(info)));
        let track = self.doc.edit(text::EDIT_ADD_TRACK, |p| {
            let track = p.add_track(&name, instrument);
            let length = p.meter.bar_start(PIANO_BARS);
            let pattern = p.add_pattern_for(track, &text::pattern_name(1), length);
            p.add_clip(track, Ticks::ZERO, length, pattern);
            track
        });
        self.focus_track(track);
        track
    }

    /// Adds a plugin effect at the end of a chain, loading it at once (so
    /// its window can open straight away).
    pub fn add_plugin_effect(&mut self, place: ChainPlace, info: &PluginInfo) -> Option<EffectId> {
        let kind = EffectKind::Plugin(PluginSetup::new(plugin_ref(info)));
        let id = self.add_effect(place, kind);
        self.sync_engine();
        id
    }

    // ----- Plugin windows

    /// The plugin a track plays, if it plays one.
    pub fn track_plugin(&self, track: TrackId) -> Option<PluginTarget> {
        self.project()
            .track(track)?
            .instrument
            .plugin()
            .map(|_| PluginTarget::Instrument(track))
    }

    /// The plugin an effect is, if it is one.
    pub fn effect_plugin(&self, id: EffectId) -> Option<PluginTarget> {
        match self.project().effect(id)?.kind {
            EffectKind::Plugin(_) => Some(PluginTarget::Effect(id)),
            _ => None,
        }
    }

    /// The setup of a plugin of the song.
    fn plugin_setup(&self, target: PluginTarget) -> Option<&PluginSetup> {
        match target {
            PluginTarget::Instrument(track) => self.project().track(track)?.instrument.plugin(),
            PluginTarget::Effect(id) => match &self.project().effect(id)?.kind {
                EffectKind::Plugin(setup) => Some(setup),
                _ => None,
            },
        }
    }

    /// A plugin's name.
    pub fn plugin_name(&self, target: PluginTarget) -> String {
        self.plugin_setup(target)
            .map_or_else(String::new, |s| s.plugin.name.clone())
    }

    /// Whether a plugin of the song is not installed on this computer (or
    /// failed to start).
    pub fn is_plugin_missing(&self, target: PluginTarget) -> bool {
        self.engine.is_plugin_missing(target)
    }

    /// Whether a plugin has a window of its own.
    pub fn has_plugin_window(&mut self, target: PluginTarget) -> bool {
        self.engine.plugin_has_editor(target)
    }

    /// Whether a plugin's window is open.
    pub fn is_plugin_window_open(&self, target: PluginTarget) -> bool {
        self.engine.is_plugin_editor_open(target)
    }

    /// Whether any plugin's window is open (they need the program's
    /// attention many times a second).
    pub fn any_plugin_window_open(&self) -> bool {
        self.engine.any_plugin_editor_open()
    }

    /// Opens a plugin's window, or closes it if it is open.
    pub fn toggle_plugin_window(&mut self, target: PluginTarget) {
        if self.is_plugin_window_open(target) {
            self.engine.close_plugin_editor(target);
        } else {
            self.show_plugin_window(target);
        }
    }

    /// Opens a plugin's window, or brings it to the front.
    pub fn show_plugin_window(&mut self, target: PluginTarget) {
        let place = match target {
            PluginTarget::Instrument(track) => self.project().track(track).map(|t| t.name.clone()),
            PluginTarget::Effect(id) => match self.project().effect_place(id) {
                Some((ChainPlace::Track(track), _)) => {
                    self.project().track(track).map(|t| t.name.clone())
                }
                _ => Some(text::MASTER.to_owned()),
            },
        };
        let title =
            text::plugin_window_title(&self.plugin_name(target), &place.unwrap_or_default());
        if let Err(problem) = self.engine.open_plugin_editor(target, &title) {
            self.set_status(problem);
        }
    }

    // ----- Plugin settings

    /// Copies every plugin's settings into the song: before it is saved or
    /// exported, and before a change that may need them again (undo, or
    /// removing a plugin). Not an undo step of its own.
    pub fn capture_plugin_states(&mut self) {
        let engine = &mut self.engine;
        let mut problems = Vec::new();
        self.doc
            .refresh(|p| problems = engine.capture_plugin_states(p));
        for problem in problems {
            self.set_status(text::problem(&problem));
        }
    }

    /// The plugins' housekeeping (their windows, their callbacks) and what
    /// they report: a change made in a plugin's window means unsaved work.
    /// (Many plugins also report a change just after taking their saved
    /// settings back: on a song with nothing unsaved, a change counts only
    /// if the settings now differ from those saved with it.)
    pub(super) fn plugin_idle(&mut self) {
        let news = self.engine.plugin_idle();
        if news.changed
            && (self.doc.is_dirty() || self.engine.plugins_differ_from(self.doc.project()))
        {
            self.doc.mark_changed_outside();
        }
        if let Some(&moved) = news.touched.last() {
            self.last_moved = Some(moved);
        }
    }

    // ----- Automation

    /// A track's lanes, in the order they were added.
    pub fn lanes(&self, track: TrackId) -> &[AutomationLane] {
        self.project()
            .track(track)
            .map_or(&[], |t| t.automation.as_slice())
    }

    /// The settings of a track that can get a lane (those without one yet):
    /// the one last moved in a plugin's window first, then volume and pan,
    /// the effects' settings and the plugin instrument's parameters.
    pub fn lane_choices(&mut self, track: TrackId) -> Vec<LaneChoice> {
        let Some(t) = self.project().track(track).cloned() else {
            return Vec::new();
        };
        let mut targets = vec![AutomationTarget::Volume, AutomationTarget::Pan];
        for effect in &t.effects {
            match &effect.kind {
                EffectKind::Plugin(_) => {
                    for param in self.engine.plugin_params(PluginTarget::Effect(effect.id)) {
                        if param.automatable {
                            targets.push(AutomationTarget::Effect {
                                effect: effect.id,
                                param: param.id,
                            });
                        }
                    }
                }
                kind => {
                    for index in 0..kind.params().len() {
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "a handful of settings"
                        )]
                        targets.push(AutomationTarget::Effect {
                            effect: effect.id,
                            param: index as u32,
                        });
                    }
                }
            }
        }
        if t.instrument.plugin().is_some() {
            for param in self.engine.plugin_params(PluginTarget::Instrument(track)) {
                if param.automatable {
                    targets.push(AutomationTarget::Instrument { param: param.id });
                }
            }
        }
        let last = self.last_moved.and_then(|(target, param)| match target {
            PluginTarget::Instrument(id) if id == track => {
                Some(AutomationTarget::Instrument { param })
            }
            PluginTarget::Effect(effect) if t.effects.iter().any(|e| e.id == effect) => {
                Some(AutomationTarget::Effect { effect, param })
            }
            _ => None,
        });
        if let Some(last) = last
            && let Some(at) = targets.iter().position(|&t| t == last)
        {
            targets.remove(at);
            targets.insert(0, last);
        }
        targets
            .into_iter()
            .filter(|&target| t.automation.iter().all(|l| l.target != target))
            .map(|target| LaneChoice {
                name: self.lane_name(track, target),
                last_moved: Some(target) == last,
                target,
            })
            .collect()
    }

    /// Gives a track a lane for a setting, starting with one point at the
    /// setting's present value (a flat line to draw from).
    pub fn add_lane(&mut self, track: TrackId, target: AutomationTarget) {
        if self.project().track(track).is_none() || self.project().lane(track, target).is_some() {
            return;
        }
        let value = self.current_value(track, target);
        self.doc.edit(text::EDIT_ADD_LANE, |p| {
            p.add_lane(track, target);
            p.update_lane(track, target, |lane| {
                lane.insert(AutomationPoint {
                    at: Ticks::ZERO,
                    value,
                });
            });
        });
    }

    /// Takes a lane, with its points, away from a track.
    pub fn remove_lane(&mut self, track: TrackId, target: AutomationTarget) {
        if self.project().lane(track, target).is_none() {
            return;
        }
        self.doc
            .edit(text::EDIT_REMOVE_LANE, |p| p.remove_lane(track, target));
    }

    /// Adds a point to a lane. Returns its index among the lane's points.
    pub fn add_lane_point(
        &mut self,
        track: TrackId,
        target: AutomationTarget,
        point: AutomationPoint,
    ) -> Option<usize> {
        self.project().lane(track, target)?;
        self.doc.edit(text::EDIT_ADD_POINT, |p| {
            let mut index = None;
            p.update_lane(track, target, |lane| index = Some(lane.insert(point)));
            index
        })
    }

    /// Moves a point of a lane; it stays between its neighbours in time.
    pub fn move_lane_point(
        &mut self,
        track: TrackId,
        target: AutomationTarget,
        index: usize,
        point: AutomationPoint,
    ) {
        let Some(lane) = self.project().lane(track, target) else {
            return;
        };
        let Some(&current) = lane.points.get(index) else {
            return;
        };
        let earliest = index
            .checked_sub(1)
            .and_then(|i| lane.points.get(i))
            .map_or(Ticks::ZERO, |p| p.at);
        let latest = lane.points.get(index + 1).map(|p| p.at);
        let mut at = point.at.max(earliest);
        if let Some(latest) = latest {
            at = at.min(latest);
        }
        let moved = AutomationPoint {
            at,
            value: point.value.clamp(0.0, 1.0),
        };
        if moved == current {
            return;
        }
        self.doc.edit(text::EDIT_MOVE_POINT, |p| {
            p.update_lane(track, target, |lane| lane.points[index] = moved);
        });
    }

    /// Removes a point of a lane.
    pub fn remove_lane_point(&mut self, track: TrackId, target: AutomationTarget, index: usize) {
        if self
            .project()
            .lane(track, target)
            .is_none_or(|l| index >= l.points.len())
        {
            return;
        }
        self.doc.edit(text::EDIT_REMOVE_POINT, |p| {
            p.update_lane(track, target, |lane| {
                lane.points.remove(index);
            });
        });
    }

    /// Whether a lane changes a track's setting as the song plays.
    pub fn is_automated(&self, track: TrackId, target: AutomationTarget) -> bool {
        self.project()
            .lane(track, target)
            .is_some_and(|l| !l.points.is_empty())
    }

    /// A lane's name: "Volume", "Reverb · Amount", "Surge XT · Filter 1
    /// Cutoff".
    pub fn lane_name(&mut self, track: TrackId, target: AutomationTarget) -> String {
        match target {
            AutomationTarget::Volume => text::VOLUME.to_owned(),
            AutomationTarget::Pan => text::PAN.to_owned(),
            AutomationTarget::Effect { effect, param } => {
                let Some(kind) = self.project().effect(effect).map(|e| e.kind.clone()) else {
                    return String::new();
                };
                let setting = match &kind {
                    EffectKind::Plugin(_) => self.param_name(PluginTarget::Effect(effect), param),
                    kind => text::effect_param_name(kind, param as usize).to_owned(),
                };
                text::lane_name(&text::effect_name(&kind), &setting)
            }
            AutomationTarget::Instrument { param } => {
                let target = PluginTarget::Instrument(track);
                let setting = self.param_name(target, param);
                text::lane_name(&self.plugin_name(target), &setting)
            }
        }
    }

    /// A value of a lane (0 to 1) as people read it: "-6.0 dB", "L 30",
    /// "1.2 kHz", or as the plugin writes it.
    pub fn lane_value_text(
        &mut self,
        track: TrackId,
        target: AutomationTarget,
        value: f32,
    ) -> String {
        match target {
            AutomationTarget::Volume => text::db(gain_to_db(volume_gain(value))),
            AutomationTarget::Pan => text::pan(value_pan(value)),
            AutomationTarget::Effect { effect, param } => {
                let Some(kind) = self.project().effect(effect).map(|e| e.kind.clone()) else {
                    return String::new();
                };
                match &kind {
                    EffectKind::Plugin(_) => self
                        .engine
                        .plugin_param_text(PluginTarget::Effect(effect), param, f64::from(value))
                        .unwrap_or_else(|| text::percent(value)),
                    kind => {
                        let index = param as usize;
                        kind.params().get(index).map_or_else(String::new, |spec| {
                            text::effect_param_value(kind, index, spec.value(value))
                        })
                    }
                }
            }
            AutomationTarget::Instrument { param } => self
                .engine
                .plugin_param_text(PluginTarget::Instrument(track), param, f64::from(value))
                .unwrap_or_else(|| text::percent(value)),
        }
    }

    /// A setting's present value, as a lane value.
    fn current_value(&mut self, track: TrackId, target: AutomationTarget) -> f32 {
        let Some(t) = self.project().track(track) else {
            return 0.5;
        };
        #[allow(clippy::cast_possible_truncation, reason = "a value from 0 to 1")]
        match target {
            AutomationTarget::Volume => gain_volume(t.strip.gain),
            AutomationTarget::Pan => pan_value(t.strip.pan),
            AutomationTarget::Effect { effect, param } => {
                match self.project().effect(effect).map(|e| e.kind.clone()) {
                    Some(EffectKind::Plugin(_)) => self
                        .engine
                        .plugin_param_value(PluginTarget::Effect(effect), param)
                        .map_or(0.5, |v| v as f32),
                    Some(kind) => kind.param(param as usize).unwrap_or(0.5),
                    None => 0.5,
                }
            }
            AutomationTarget::Instrument { param } => self
                .engine
                .plugin_param_value(PluginTarget::Instrument(track), param)
                .map_or(0.5, |v| v as f32),
        }
    }

    /// A plugin parameter's name (remembered: a plugin can have hundreds).
    fn param_name(&mut self, target: PluginTarget, param: u32) -> String {
        let Some(plugin) = self.plugin_setup(target).map(|s| s.plugin.id.clone()) else {
            return String::new();
        };
        if let Some(name) = self.param_names.get(&(plugin.clone(), param)) {
            return name.clone();
        }
        let params = self.engine.plugin_params(target);
        if params.is_empty() {
            return text::numbered(text::PARAMETER, param);
        }
        for p in params {
            self.param_names.insert((plugin.clone(), p.id), p.name);
        }
        self.param_names
            .get(&(plugin, param))
            .cloned()
            .unwrap_or_else(|| text::numbered(text::PARAMETER, param))
    }
}
