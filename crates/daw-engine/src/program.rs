// SPDX-License-Identifier: GPL-3.0-or-later

//! The playback program: the project compiled into a form the audio thread
//! can play without allocating or searching.

use daw_dsp::effects::{
    CompressorParams, DelayParams, EffectParams, EqParams, LimiterParams, ReverbParams,
};
use daw_model::{
    Clip, ClipContent, ClipId, DelayTime, Effect, EffectId, EffectKind, MeterMap, PatternId,
    Project, Region, TempoMap, Ticks, TrackId, seconds_per_tick,
};
use std::sync::Arc;

/// What the transport plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackScope {
    /// The whole song, as arranged on the timeline.
    Song,
    /// One pattern on its own, looping from its start (the other tracks are
    /// silent but can still be played live).
    Pattern {
        /// The pattern's track.
        track: TrackId,
        /// The pattern.
        pattern: PatternId,
    },
}

/// An effect in a compiled chain: where it is in the processor's effect
/// table, and its settings.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EffectRef {
    pub index: usize,
    pub params: EffectParams,
    pub enabled: bool,
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
    /// The track's effect chain, in order.
    pub effects: Vec<EffectRef>,
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
/// placed at the start of its track and looping.
pub(crate) fn scoped(project: &Project, scope: PlaybackScope) -> std::borrow::Cow<'_, Project> {
    let PlaybackScope::Pattern { track, pattern } = scope else {
        return std::borrow::Cow::Borrowed(project);
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
    view.loop_region = Region::new(Ticks::ZERO, length);
    std::borrow::Cow::Owned(view)
}

/// The settings the audio thread's effect takes for a project's effect, at
/// the song's starting tempo (a synced delay follows it).
pub(crate) fn effect_params(kind: &EffectKind, bpm: f64) -> EffectParams {
    match kind {
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
    }
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
            Some(TrackProgram {
                slot,
                notes,
                gain_left: gain * (1.0 - pan).min(1.0),
                gain_right: gain * (1.0 + pan).min(1.0),
                effects: chain(&track.effects, bpm, &effect_index),
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
