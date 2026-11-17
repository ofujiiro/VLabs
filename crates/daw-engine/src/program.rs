// SPDX-License-Identifier: GPL-3.0-or-later

//! The playback program: the project compiled into a form the audio thread
//! can play without allocating or searching.

use daw_dsp::effects::{
    CompressorParams, DelayParams, EffectParams, EqParams, LimiterParams, ReverbParams,
};
use daw_model::{
    AutomationTarget, Clip, ClipContent, ClipId, DelayTime, Effect, EffectId, EffectKind, MeterMap,
    PatternId, Project, Region, TempoMap, Ticks, TrackId, seconds_per_tick,
};
use std::sync::Arc;

/// What the transport plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackScope {
    /// The whole song, as arranged on the timeline, looping the song's own
    /// loop region if it has one.
    Song,
    /// The song, looping one region instead of the song's own loop: for
    /// working on a pattern with the rest of the song around it.
    Looping(Region),
    /// The song, never looping (it plays on until stopped).
    Unlooped,
    /// One pattern on its own from its start, looping or not (the other
    /// tracks are silent but can still be played live).
    Pattern {
        /// The pattern's track.
        track: TrackId,
        /// The pattern.
        pattern: PatternId,
        /// Whether it plays again from its start when it ends.
        looping: bool,
    },
}

/// An effect in a compiled chain: where it is in the processor's effect
/// table, and its settings.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EffectRef {
    pub index: usize,
    /// The settings of a built-in effect (a plugin keeps its own).
    pub params: Option<EffectParams>,
    pub enabled: bool,
}

/// What an automation lane changes, as the audio thread reaches it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LaneTarget {
    Volume,
    Pan,
    /// A setting of a built-in effect: which of the track's automated
    /// effects, and which setting.
    BuiltinEffect {
        effect: usize,
        param: usize,
    },
    /// A parameter of a plugin effect, by its table entry.
    PluginEffect {
        index: usize,
        param: u32,
    },
    /// A parameter of the track's plugin instrument.
    Instrument {
        param: u32,
    },
}

/// An automation lane, ready to evaluate on the audio thread.
#[derive(Clone, Debug)]
pub(crate) struct CompiledLane {
    pub target: LaneTarget,
    /// (tick, value), in time order; never empty.
    pub points: Vec<(f64, f32)>,
    /// The value last applied (NaN before the first).
    pub last: f32,
}

impl CompiledLane {
    /// The value at a tick: straight lines between points.
    pub fn value_at(&self, at: f64) -> f32 {
        let points = &self.points;
        let next = points.partition_point(|p| p.0 <= at);
        if next == 0 {
            return points[0].1;
        }
        let Some(&(t1, v1)) = points.get(next) else {
            return points[points.len() - 1].1;
        };
        let (t0, v0) = points[next - 1];
        if t1 <= t0 {
            return v1;
        }
        #[allow(clippy::cast_possible_truncation, reason = "a fraction from 0 to 1")]
        let t = ((at - t0) / (t1 - t0)) as f32;
        v0 + (v1 - v0) * t
    }
}

/// A built-in effect whose settings are automated: where it is, and its
/// settings without automation.
#[derive(Clone, Debug)]
pub(crate) struct AutomatedEffect {
    pub index: usize,
    pub base: EffectKind,
    pub bpm: f64,
}

/// A note as the engine plays it: absolute positions on the timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NoteSpan {
    pub start: Ticks,
    pub end: Ticks,
    pub key: u8,
    pub velocity: f32,
}

/// One track's part of the program.
#[derive(Clone, Debug)]
pub(crate) struct TrackProgram {
    /// The instrument slot that plays this track.
    pub slot: usize,
    /// Every note of the track, sorted by start.
    pub notes: Vec<NoteSpan>,
    /// Output gains after volume, pan, mute and solo.
    pub gain_left: f32,
    pub gain_right: f32,
    /// Volume and pan as set (automation may override them while playing),
    /// and whether the track is heard at all (mute and solo).
    pub gain: f32,
    pub pan: f32,
    pub audible: bool,
    /// The gains applied at the end of the last block: changes glide from
    /// there over one block, so they never click.
    pub current_left: f32,
    pub current_right: f32,
    /// The track's effect chain, in order.
    pub effects: Vec<EffectRef>,
    /// Settings that change over the song.
    pub automation: Vec<CompiledLane>,
    /// Built-in effects with automated settings.
    pub automated_effects: Vec<AutomatedEffect>,
}

/// Output gains for a volume and a pan position: the centre is at full level
/// on both sides, and a side keeps its level as the sound moves to it.
pub(crate) fn pan_gains(gain: f32, pan: f32) -> (f32, f32) {
    let pan = pan.clamp(-1.0, 1.0);
    (gain * (1.0 - pan).min(1.0), gain * (1.0 + pan).min(1.0))
}

/// Converts between ticks and seconds quickly, for a tempo map with changes.
#[derive(Clone, Debug)]
pub(crate) struct TempoTable {
    /// (start tick, start second, seconds per tick), sorted.
    segments: Vec<(f64, f64, f64)>,
}

impl TempoTable {
    #[allow(clippy::cast_precision_loss, reason = "tick counts are far below 2^52")]
    pub fn new(map: &TempoMap) -> Self {
        let mut segments = Vec::with_capacity(map.changes().len());
        let mut seconds = 0.0;
        let mut previous: Option<(f64, f64)> = None;
        for change in map.changes() {
            let tick = change.at.0 as f64;
            if let Some((start, per_tick)) = previous {
                seconds += (tick - start) * per_tick;
            }
            let per_tick = seconds_per_tick(change.bpm);
            segments.push((tick, seconds, per_tick));
            previous = Some((tick, per_tick));
        }
        Self { segments }
    }

    fn segment_for_tick(&self, tick: f64) -> (f64, f64, f64) {
        let index = self
            .segments
            .partition_point(|s| s.0 <= tick)
            .saturating_sub(1);
        self.segments[index]
    }

    pub fn seconds_at(&self, tick: f64) -> f64 {
        let (start, seconds, per_tick) = self.segment_for_tick(tick);
        seconds + (tick - start) * per_tick
    }

    pub fn ticks_at(&self, seconds: f64) -> f64 {
        let index = self
            .segments
            .partition_point(|s| s.1 <= seconds)
            .saturating_sub(1);
        let (start, at, per_tick) = self.segments[index];
        start + (seconds - at) / per_tick
    }
}

/// Everything the audio thread needs to play the song.
#[derive(Clone, Debug)]
pub(crate) struct Program {
    pub tempo: TempoTable,
    pub meter: MeterMap,
    pub loop_region: Option<Region>,
    pub tracks: Vec<TrackProgram>,
    /// The master bus's effect chain and gain.
    pub master_effects: Vec<EffectRef>,
    pub master_gain: f32,
}

/// The project as the scope plays it: for a pattern, the pattern alone,
/// placed at the start of its track.
pub(crate) fn scoped(project: &Project, scope: PlaybackScope) -> std::borrow::Cow<'_, Project> {
    let (track, pattern, looping) = match scope {
        PlaybackScope::Song => return std::borrow::Cow::Borrowed(project),
        PlaybackScope::Looping(region) => {
            let mut view = project.clone();
            view.loop_region = Some(region);
            return std::borrow::Cow::Owned(view);
        }
        PlaybackScope::Unlooped => {
            let mut view = project.clone();
            view.loop_region = None;
            return std::borrow::Cow::Owned(view);
        }
        PlaybackScope::Pattern {
            track,
            pattern,
            looping,
        } => (track, pattern, looping),
    };
    let Some(length) = project.pattern(pattern).map(|p| p.length) else {
        return std::borrow::Cow::Borrowed(project);
    };
    let mut view = project.clone();
    for t in &mut view.tracks {
        let t = Arc::make_mut(t);
        t.clips.clear();
        if t.id == track {
            t.clips.push(Clip {
                id: ClipId(0),
                start: Ticks::ZERO,
                length,
                content: ClipContent::Pattern {
                    pattern,
                    offset: Ticks::ZERO,
                },
            });
        }
    }
    view.loop_region = if looping {
        Region::new(Ticks::ZERO, length)
    } else {
        None
    };
    std::borrow::Cow::Owned(view)
}

/// The settings the audio thread's effect takes for a project's effect, at
/// the song's starting tempo (a synced delay follows it). None for a plugin.
pub(crate) fn effect_params(kind: &EffectKind, bpm: f64) -> Option<EffectParams> {
    Some(match kind {
        EffectKind::Eq(e) => EffectParams::Eq(EqParams {
            low_cut_hz: e.low_cut_hz,
            low_gain_db: e.low_gain_db,
            low_hz: e.low_hz,
            mid_gain_db: e.mid_gain_db,
            mid_hz: e.mid_hz,
            mid_q: e.mid_q,
            high_gain_db: e.high_gain_db,
            high_hz: e.high_hz,
            high_cut_hz: e.high_cut_hz,
        }),
        EffectKind::Compressor(c) => EffectParams::Compressor(CompressorParams {
            threshold_db: c.threshold_db,
            ratio: c.ratio,
            attack_ms: c.attack_ms,
            release_ms: c.release_ms,
            knee_db: c.knee_db,
            makeup_db: c.makeup_db,
        }),
        EffectKind::Reverb(r) => EffectParams::Reverb(ReverbParams {
            size: r.size,
            damping: r.damping,
            width: r.width,
            mix: r.mix,
            pre_delay_ms: r.pre_delay_ms,
        }),
        EffectKind::Delay(d) => {
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                reason = "seconds"
            )]
            let time_seconds = match d.time {
                DelayTime::Synced(ticks) => (ticks.0 as f64 * seconds_per_tick(bpm)) as f32,
                DelayTime::Seconds(seconds) => seconds,
            };
            EffectParams::Delay(DelayParams {
                time_seconds,
                feedback: d.feedback,
                mix: d.mix,
                tone_hz: d.tone_hz,
                ping_pong: d.ping_pong,
            })
        }
        EffectKind::Limiter(l) => EffectParams::Limiter(LimiterParams {
            ceiling_db: l.ceiling_db,
            gain_db: l.gain_db,
            release_ms: l.release_ms,
        }),
        EffectKind::Plugin(_) => return None,
    })
}

fn chain(
    effects: &[Effect],
    bpm: f64,
    effect_index: &impl Fn(EffectId) -> Option<usize>,
) -> Vec<EffectRef> {
    effects
        .iter()
        .filter_map(|e| {
            Some(EffectRef {
                index: effect_index(e.id)?,
                params: effect_params(&e.kind, bpm),
                enabled: e.enabled,
            })
        })
        .collect()
}

/// A track's automation lanes, ready for the audio thread (lanes without
/// points, or whose effect is gone, are left out).
fn lanes(
    track: &daw_model::Track,
    bpm: f64,
    effect_index: &impl Fn(EffectId) -> Option<usize>,
) -> (Vec<CompiledLane>, Vec<AutomatedEffect>) {
    let mut lanes = Vec::new();
    let mut effects: Vec<AutomatedEffect> = Vec::new();
    for lane in &track.automation {
        if lane.points.is_empty() {
            continue;
        }
        let target = match lane.target {
            AutomationTarget::Volume => LaneTarget::Volume,
            AutomationTarget::Pan => LaneTarget::Pan,
            // Only a plugin instrument has parameters.
            AutomationTarget::Instrument { param } if track.instrument.plugin().is_some() => {
                LaneTarget::Instrument { param }
            }
            AutomationTarget::Instrument { .. } => continue,
            AutomationTarget::Effect { effect, param } => {
                let Some(found) = track.effects.iter().find(|e| e.id == effect) else {
                    continue;
                };
                let Some(index) = effect_index(effect) else {
                    continue;
                };
                if let EffectKind::Plugin(_) = found.kind {
                    LaneTarget::PluginEffect { index, param }
                } else {
                    let position = effects
                        .iter()
                        .position(|e| e.index == index)
                        .unwrap_or_else(|| {
                            effects.push(AutomatedEffect {
                                index,
                                base: found.kind.clone(),
                                bpm,
                            });
                            effects.len() - 1
                        });
                    LaneTarget::BuiltinEffect {
                        effect: position,
                        param: param as usize,
                    }
                }
            }
        };
        #[allow(clippy::cast_precision_loss, reason = "tick positions fit easily")]
        let points = lane
            .points
            .iter()
            .map(|p| (p.at.0 as f64, p.value.clamp(0.0, 1.0)))
            .collect();
        lanes.push(CompiledLane {
            target,
            points,
            last: f32::NAN,
        });
    }
    (lanes, effects)
}

/// Compiles a project. `slot_of` gives the instrument slot of each track
/// (tracks without one are left out), `effect_index` the effect table
/// entry of each effect (effects without one are left out).
pub(crate) fn compile(
    project: &Project,
    slot_of: impl Fn(TrackId) -> Option<usize>,
    effect_index: impl Fn(EffectId) -> Option<usize>,
) -> Program {
    let bpm = project.tempo.initial_bpm();
    let any_solo = project.tracks.iter().any(|t| t.strip.solo);
    let tracks = project
        .tracks
        .iter()
        .filter_map(|track| {
            let slot = slot_of(track.id)?;
            let audible = !track.strip.mute && (!any_solo || track.strip.solo);
            let gain = if audible {
                track.strip.gain.max(0.0)
            } else {
                0.0
            };
            let pan = track.strip.pan.clamp(-1.0, 1.0);
            let mut notes = Vec::new();
            for clip in &track.clips {
                let ClipContent::Pattern { pattern, offset } = clip.content;
                let Some(pattern) = project.pattern(pattern) else {
                    continue;
                };
                if pattern.length <= Ticks::ZERO {
                    continue;
                }
                // The pattern repeats from `offset` for the length of the clip.
                let mut cycle_start = clip.start - offset;
                while cycle_start < clip.end() {
                    for note in &pattern.notes {
                        if note.start >= pattern.length {
                            continue;
                        }
                        let start = cycle_start + note.start;
                        let end = (cycle_start + note.end()).min(clip.end());
                        if start >= clip.start && start < clip.end() && end > start {
                            notes.push(NoteSpan {
                                start,
                                end,
                                key: note.key,
                                velocity: note.velocity,
                            });
                        }
                    }
                    cycle_start += pattern.length;
                }
            }
            notes.sort_by_key(|n| (n.start, n.key));
            let (gain_left, gain_right) = pan_gains(gain, pan);
            let (automation, automated_effects) = lanes(track, bpm, &effect_index);
            Some(TrackProgram {
                slot,
                notes,
                gain_left,
                gain_right,
                gain: track.strip.gain.max(0.0),
                pan,
                audible,
                current_left: gain_left,
                current_right: gain_right,
                effects: chain(&track.effects, bpm, &effect_index),
                automation,
                automated_effects,
            })
        })
        .collect();
    Program {
        tempo: TempoTable::new(&project.tempo),
        meter: project.meter.clone(),
        loop_region: project.loop_region,
        tracks,
        master_effects: chain(&project.master.effects, bpm, &effect_index),
        master_gain: project.master.gain.max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_model::{DEFAULT_VELOCITY, Note, NoteId};

    fn note(start: Ticks, length: Ticks, key: u8) -> Note {
        Note {
            id: NoteId(0),
            start,
            length,
            key,
            velocity: DEFAULT_VELOCITY,
            pitch_offset: 0.0,
        }
    }

    #[test]
    fn patterns_repeat_across_long_clips_and_are_cut_at_the_clip_end() {
        let mut project = Project::starter("P", "T");
        let (track, pattern) = project.first_pattern().unwrap();
        project.pattern_mut(pattern).unwrap().length = Ticks::WHOLE;
        project.add_note(pattern, note(Ticks::ZERO, Ticks::QUARTER, 60));
        project.add_note(pattern, note(Ticks::QUARTER * 3, Ticks::QUARTER * 2, 64));
        // The starter clip is four bars long: the one-bar pattern plays four times.
        let program = compile(&project, |id| (id == track).then_some(0), |_| None);
        let notes = &program.tracks[0].notes;
        assert_eq!(notes.len(), 8);
        assert_eq!(notes[2].start, Ticks::WHOLE);
        // The last note would run past the clip; it is cut at the clip end.
        assert_eq!(notes[7].end, Ticks::WHOLE * 4);
    }

    #[test]
    fn mute_and_solo_set_gains() {
        let mut project = Project::starter("P", "T");
        let (track, _) = project.first_pattern().unwrap();
        let other = project.add_track("Other", daw_model::Instrument::builtin_piano());
        project.track_mut(track).unwrap().strip.solo = true;
        let program = compile(&project, |id| Some(usize::from(id != track)), |_| None);
        assert_eq!(program.tracks[0].gain_left, 1.0);
        assert_eq!(program.tracks[1].gain_left, 0.0, "another track is soloed");
        project.track_mut(track).unwrap().strip.solo = false;
        project.track_mut(other).unwrap().strip.mute = true;
        let program = compile(&project, |id| Some(usize::from(id != track)), |_| None);
        assert_eq!(program.tracks[1].gain_right, 0.0, "muted");
    }

    #[test]
    fn panning_keeps_the_centre_at_full_level() {
        let mut project = Project::starter("P", "T");
        let (track, _) = project.first_pattern().unwrap();
        project.track_mut(track).unwrap().strip.pan = 0.5;
        let program = compile(&project, |_| Some(0), |_| None);
        assert_eq!(
            (program.tracks[0].gain_left, program.tracks[0].gain_right),
            (0.5, 1.0)
        );
    }

    #[test]
    fn the_tempo_table_matches_the_tempo_map() {
        let mut map = TempoMap::constant(120.0);
        map.set(Ticks::quarters(4), 90.0);
        map.set(Ticks::quarters(10), 150.0);
        let table = TempoTable::new(&map);
        for quarters in [0, 1, 4, 7, 10, 13] {
            let tick = Ticks::quarters(quarters);
            #[allow(clippy::cast_precision_loss, reason = "test values")]
            let from_table = table.seconds_at(tick.0 as f64);
            assert!((from_table - map.seconds_at(tick)).abs() < 1e-9);
            #[allow(clippy::cast_precision_loss, reason = "test values")]
            let back = table.ticks_at(from_table) - tick.0 as f64;
            assert!(back.abs() < 1e-3);
        }
    }
}
