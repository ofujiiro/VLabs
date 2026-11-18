// SPDX-License-Identifier: GPL-3.0-or-later

//! The playback program: the project compiled into a form the audio thread
//! can play without allocating or searching.

use daw_dsp::audio::AudioBuffer;
use daw_dsp::effects::{
    CompressorParams, DelayParams, EffectParams, EqParams, LimiterParams, ReverbParams,
};
use daw_model::{
    AutomationTarget, Clip, ClipContent, ClipId, DelayTime, Effect, EffectId, EffectKind, MeterMap,
    PatternId, Project, Region, TempoMap, Ticks, TrackId, piece_seconds, piece_ticks,
    seconds_per_tick,
};
use std::sync::Arc;

/// Every clip edge fades over this long, so a cut never clicks (decision
/// 0017), unless the clip carries on exactly where the one before stops.
const DECLICK_SECONDS: f64 = 0.0015;

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
    /// A setting of the track's built-in synth, by its number
    /// (`SynthSettings::params`).
    Synth {
        param: usize,
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

/// An audio clip as the audio thread plays it: positions are frames of the
/// song at the engine's rate, counted from its start.
#[derive(Clone, Debug)]
pub(crate) struct AudioSpan {
    pub start: i64,
    pub end: i64,
    pub sound: Arc<AudioBuffer>,
    /// The frame of the sound heard at `start`.
    pub offset: f64,
    /// Frames of the sound per frame of the song (1 at the engine's rate).
    pub step: f64,
    pub gain: f32,
    /// Frames of fade at either end.
    pub fade_in: i64,
    pub fade_out: i64,
}

impl AudioSpan {
    /// The clip's volume `at` frames after its start: its gain, through its
    /// fades (an equal-power curve).
    #[allow(clippy::cast_precision_loss, reason = "fade lengths are small")]
    pub fn gain_at(&self, at: i64) -> f32 {
        let mut gain = self.gain;
        if at < self.fade_in {
            gain *= (std::f32::consts::FRAC_PI_2 * (at as f32 + 0.5) / self.fade_in as f32).sin();
        }
        let left = self.end - self.start - at;
        if left <= self.fade_out {
            gain *=
                (std::f32::consts::FRAC_PI_2 * (left as f32 - 0.5) / self.fade_out as f32).sin();
        }
        gain
    }
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
    /// The track's synth settings without automation, when its synth has
    /// automated settings.
    pub synth_base: Option<daw_model::SynthSettings>,
    /// The track's audio clips, sorted by start.
    pub audio: Vec<AudioSpan>,
    /// For each audio clip, the furthest end of it and those before it (to
    /// find the clips a block reaches quickly).
    pub audio_reach: Vec<i64>,
}

impl TrackProgram {
    /// Whether any audio clip sounds between two frames of the song.
    pub fn audio_between(&self, from: i64, to: i64) -> bool {
        let first = self.audio_reach.partition_point(|&reach| reach <= from);
        self.audio[first..]
            .iter()
            .take_while(|span| span.start < to)
            .any(|span| span.end > from)
    }

    /// Adds the track's audio clips from frame `from` of the song into the
    /// buffers (both the same length). Real-time safe.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "frame positions are far below 2^52"
    )]
    pub fn mix_audio(&self, from: i64, left: &mut [f32], right: &mut [f32]) {
        let to = from + left.len() as i64;
        let first = self.audio_reach.partition_point(|&reach| reach <= from);
        for span in self.audio[first..].iter().take_while(|s| s.start < to) {
            let (start, end) = (span.start.max(from), span.end.min(to));
            let (sound_left, sound_right) = (span.sound.left(), span.sound.right());
            let frames = sound_left.len();
            for at in start..end {
                let index = (at - from) as usize;
                let gain = span.gain_at(at - span.start);
                let position = span.offset + (at - span.start) as f64 * span.step;
                let (l, r) = if (span.step - 1.0).abs() < f64::EPSILON {
                    let frame = position.round();
                    if frame < 0.0 || frame as usize >= frames {
                        continue;
                    }
                    (sound_left[frame as usize], sound_right[frame as usize])
                } else {
                    // At another rate: between two frames.
                    if position < 0.0 || position as usize + 1 >= frames {
                        continue;
                    }
                    let frame = position as usize;
                    let t = (position - frame as f64) as f32;
                    (
                        sound_left[frame] + (sound_left[frame + 1] - sound_left[frame]) * t,
                        sound_right[frame] + (sound_right[frame + 1] - sound_right[frame]) * t,
                    )
                };
                left[index] += l * gain;
                right[index] += r * gain;
            }
        }
    }
}

/// Output gains for a volume and a pan position: the centre is at full level
/// on both sides, and a side keeps its level as the sound moves to it.
pub(crate) fn pan_gains(gain: f32, pan: f32) -> (f32, f32) {
    let pan = pan.clamp(-1.0, 1.0);
    (gain * (1.0 - pan).min(1.0), gain * (1.0 + pan).min(1.0))
}

/// Converts between ticks and seconds quickly, for a tempo map with changes
/// and glides (decision 0018): the same formulas as the model's tempo map,
/// with the seconds at the start of each piece worked out in advance.
#[derive(Clone, Debug)]
pub(crate) struct TempoTable {
    segments: Vec<TempoSegment>,
}

/// One piece of the tempo map: from `start` (at `seconds`) at `bpm`, to
/// `end` at `next_bpm` (the same tempo unless it glides).
#[derive(Clone, Copy, Debug)]
struct TempoSegment {
    start: f64,
    seconds: f64,
    bpm: f64,
    end: f64,
    next_bpm: f64,
}

impl TempoTable {
    #[allow(clippy::cast_precision_loss, reason = "tick counts are far below 2^52")]
    pub fn new(map: &TempoMap) -> Self {
        let changes = map.changes();
        let mut segments = Vec::with_capacity(changes.len());
        let mut seconds = 0.0;
        for (index, change) in changes.iter().enumerate() {
            let start = change.at.0 as f64;
            let (end, next_bpm) = match changes.get(index + 1) {
                Some(next) if change.glide => (next.at.0 as f64, next.bpm),
                Some(next) => (next.at.0 as f64, change.bpm),
                None => (f64::INFINITY, change.bpm),
            };
            segments.push(TempoSegment {
                start,
                seconds,
                bpm: change.bpm,
                end,
                next_bpm,
            });
            if end.is_finite() {
                seconds += piece_seconds(start, change.bpm, end, next_bpm, start, end);
            }
        }
        Self { segments }
    }

    fn segment_for_tick(&self, tick: f64) -> TempoSegment {
        let index = self
            .segments
            .partition_point(|s| s.start <= tick)
            .saturating_sub(1);
        self.segments[index]
    }

    pub fn seconds_at(&self, tick: f64) -> f64 {
        let s = self.segment_for_tick(tick);
        if tick < s.start {
            // Before the song (a count-in): the first tempo carries on.
            return s.seconds + (tick - s.start) * 60.0 / (s.bpm * quarter());
        }
        s.seconds + piece_seconds(s.start, s.bpm, s.end, s.next_bpm, s.start, tick)
    }

    pub fn ticks_at(&self, seconds: f64) -> f64 {
        let index = self
            .segments
            .partition_point(|s| s.seconds <= seconds)
            .saturating_sub(1);
        let s = self.segments[index];
        if seconds < s.seconds {
            return s.start + (seconds - s.seconds) * s.bpm * quarter() / 60.0;
        }
        piece_ticks(s.start, s.bpm, s.end, s.next_bpm, seconds - s.seconds)
    }
}

#[allow(clippy::cast_precision_loss, reason = "a constant")]
fn quarter() -> f64 {
    daw_model::TICKS_PER_QUARTER as f64
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
            // A plugin instrument has parameters, the synth numbered
            // settings.
            AutomationTarget::Instrument { param } if track.instrument.plugin().is_some() => {
                LaneTarget::Instrument { param }
            }
            AutomationTarget::Instrument { param }
                if track.instrument.synth().is_some()
                    && (param as usize) < daw_model::SynthSettings::params().len() =>
            {
                LaneTarget::Synth {
                    param: param as usize,
                }
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

/// A track's audio clips, ready for the audio thread. Clips whose sound
/// has not been given to the engine yet are left out (they are silent until
/// it arrives and the song is compiled again).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "frame positions are far below 2^52"
)]
fn audio_spans(
    project: &Project,
    track: &daw_model::Track,
    sample_rate: u32,
    sound_of: &impl Fn(&str) -> Option<Arc<AudioBuffer>>,
) -> (Vec<AudioSpan>, Vec<i64>) {
    let rate = f64::from(sample_rate.max(1));
    let frame_at = |tick: Ticks| (project.tempo.seconds_at(tick) * rate).round() as i64;
    let mut spans: Vec<(AudioSpan, f32, f32)> = Vec::new();
    for clip in &track.clips {
        let ClipContent::Audio {
            audio,
            offset,
            gain,
            fade_in,
            fade_out,
        } = clip.content
        else {
            continue;
        };
        let Some(file) = project.audio_file(audio) else {
            continue;
        };
        let Some(sound) = sound_of(&file.file) else {
            continue;
        };
        let (start, end) = (frame_at(clip.start), frame_at(clip.end()));
        if end <= start {
            continue;
        }
        let sound_rate = f64::from(sound.sample_rate());
        let span = AudioSpan {
            start,
            end,
            offset: file.seconds_in(offset) * sound_rate,
            step: sound_rate / rate,
            sound,
            gain: if gain.is_finite() { gain.max(0.0) } else { 1.0 },
            fade_in: 0,
            fade_out: 0,
        };
        spans.push((span, fade_in, fade_out));
    }
    spans.sort_by_key(|(span, _, _)| span.start);
    let declick = (DECLICK_SECONDS * rate).round() as i64;
    let to_frames = |seconds: f32| {
        if seconds.is_finite() {
            (f64::from(seconds.max(0.0)) * rate).round() as i64
        } else {
            0
        }
    };
    let continues = |a: &AudioSpan, b: &AudioSpan| {
        b.start == a.end
            && Arc::ptr_eq(&a.sound, &b.sound)
            && (b.offset - (a.offset + (a.end - a.start) as f64 * a.step)).abs() < 1.0
    };
    let mut done: Vec<AudioSpan> = Vec::with_capacity(spans.len());
    for index in 0..spans.len() {
        let joined_before = index > 0 && continues(&spans[index - 1].0, &spans[index].0);
        let joined_after =
            index + 1 < spans.len() && continues(&spans[index].0, &spans[index + 1].0);
        let (span, fade_in, fade_out) = &spans[index];
        let mut span = span.clone();
        let edge_in = if joined_before { 0 } else { declick };
        let edge_out = if joined_after { 0 } else { declick };
        let (mut fade_in, mut fade_out) = (
            to_frames(*fade_in).max(edge_in),
            to_frames(*fade_out).max(edge_out),
        );
        let length = span.end - span.start;
        if fade_in + fade_out > length {
            // Fades longer than the clip share it.
            let total = fade_in + fade_out;
            fade_in = fade_in * length / total;
            fade_out = length - fade_in;
        }
        span.fade_in = fade_in;
        span.fade_out = fade_out;
        done.push(span);
    }
    let mut reach = Vec::with_capacity(done.len());
    let mut furthest = i64::MIN;
    for span in &done {
        furthest = furthest.max(span.end);
        reach.push(furthest);
    }
    (done, reach)
}

/// Compiles a project. `slot_of` gives the instrument slot of each track
/// (tracks without one are left out), `effect_index` the effect table
/// entry of each effect (effects without one are left out), `sound_of` the
/// sound of each audio file (by stored name).
pub(crate) fn compile(
    project: &Project,
    sample_rate: u32,
    slot_of: impl Fn(TrackId) -> Option<usize>,
    effect_index: impl Fn(EffectId) -> Option<usize>,
    sound_of: impl Fn(&str) -> Option<Arc<AudioBuffer>>,
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
            // Each pattern clip repeats its pattern for its length, swung as
            // it plays (decision 0018).
            let mut notes: Vec<NoteSpan> = track
                .clips
                .iter()
                .filter_map(|clip| Some((clip, project.pattern(clip.pattern()?)?)))
                .flat_map(|(clip, pattern)| pattern.played_in(clip))
                .map(|n| NoteSpan {
                    start: n.start,
                    end: n.end,
                    key: n.key,
                    velocity: n.velocity,
                })
                .collect();
            notes.sort_by_key(|n| (n.start, n.key));
            let (gain_left, gain_right) = pan_gains(gain, pan);
            let (automation, automated_effects) = lanes(track, bpm, &effect_index);
            let synth_base = track.instrument.synth().copied().filter(|_| {
                automation
                    .iter()
                    .any(|l| matches!(l.target, LaneTarget::Synth { .. }))
            });
            let (audio, audio_reach) = audio_spans(project, track, sample_rate, &sound_of);
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
                synth_base,
                audio,
                audio_reach,
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
        let program = compile(
            &project,
            48_000,
            |id| (id == track).then_some(0),
            |_| None,
            |_| None,
        );
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
        let program = compile(
            &project,
            48_000,
            |id| Some(usize::from(id != track)),
            |_| None,
            |_| None,
        );
        assert_eq!(program.tracks[0].gain_left, 1.0);
        assert_eq!(program.tracks[1].gain_left, 0.0, "another track is soloed");
        project.track_mut(track).unwrap().strip.solo = false;
        project.track_mut(other).unwrap().strip.mute = true;
        let program = compile(
            &project,
            48_000,
            |id| Some(usize::from(id != track)),
            |_| None,
            |_| None,
        );
        assert_eq!(program.tracks[1].gain_right, 0.0, "muted");
    }

    #[test]
    fn panning_keeps_the_centre_at_full_level() {
        let mut project = Project::starter("P", "T");
        let (track, _) = project.first_pattern().unwrap();
        project.track_mut(track).unwrap().strip.pan = 0.5;
        let program = compile(&project, 48_000, |_| Some(0), |_| None, |_| None);
        assert_eq!(
            (program.tracks[0].gain_left, program.tracks[0].gain_right),
            (0.5, 1.0)
        );
    }

    #[test]
    fn swung_patterns_play_their_off_beats_late() {
        let mut project = Project::starter("P", "T");
        let (track, pattern) = project.first_pattern().unwrap();
        let sixteenth = Ticks::note_value(1, 16);
        {
            let p = project.pattern_mut(pattern).unwrap();
            p.length = Ticks::WHOLE;
            p.swing = daw_model::Swing {
                percent: 100,
                unit: sixteenth,
            };
        }
        project.add_note(pattern, note(Ticks::ZERO, sixteenth, 60));
        project.add_note(pattern, note(sixteenth, sixteenth, 62));
        let program = compile(
            &project,
            48_000,
            |id| (id == track).then_some(0),
            |_| None,
            |_| None,
        );
        let notes = &program.tracks[0].notes;
        assert_eq!(notes[0].start, Ticks::ZERO, "the beat stays");
        assert_eq!(notes[0].end, Ticks::note_value(3, 32), "and lasts longer");
        assert_eq!(
            notes[1].start,
            Ticks::note_value(3, 32),
            "the off-beat is late"
        );
        assert_eq!(notes[1].end, Ticks::note_value(1, 8), "and ends on time");
        assert_eq!(notes[2].start, Ticks::WHOLE, "every repeat swings the same");
    }

    #[test]
    fn the_tempo_table_matches_the_tempo_map() {
        let mut map = TempoMap::constant(120.0);
        map.set(Ticks::quarters(4), 90.0);
        map.set(Ticks::quarters(10), 150.0);
        map.set(Ticks::quarters(16), 60.0);
        map.set_glide(Ticks::quarters(10), true);
        let table = TempoTable::new(&map);
        for quarters in [0, 1, 4, 7, 10, 12, 13, 15, 16, 20] {
            let tick = Ticks::quarters(quarters);
            #[allow(clippy::cast_precision_loss, reason = "test values")]
            let from_table = table.seconds_at(tick.0 as f64);
            assert!(
                (from_table - map.seconds_at(tick)).abs() < 1e-9,
                "{quarters} quarters"
            );
            #[allow(clippy::cast_precision_loss, reason = "test values")]
            let back = table.ticks_at(from_table) - tick.0 as f64;
            assert!(back.abs() < 1e-3);
        }
    }
}
