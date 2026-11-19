// SPDX-License-Identifier: GPL-3.0-or-later

//! Singing voices (decision 0027): adding a voice track, the lyrics of its
//! notes (typed one at a time or as a phrase that goes to the notes in
//! turn, every change undoable), whether its voicebank has each lyric, and
//! whether its singing is still being rendered.
//!
//! Expression (decision 0028): the Pitch mode, the pitch the voice sings
//! through the open pattern (worked out as the engine sings it, for the
//! piano roll to draw), and the edits of each note's pitch points,
//! vibrato, colour and breath, every one undoable.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use daw_dsp::voice::VoiceBank;
use daw_model::expression::{
    CurveNote, NoteExpression, PLAIN_GLIDE, PhrasePitch, PitchPoint, PitchShape, Timed, Vibrato,
    phrases,
};
use daw_model::lyrics::{DEFAULT_LYRIC, alias_candidates, is_rest};
use daw_model::{Instrument, NoteId, TempoMap, Ticks, TrackId};

use super::Session;
use super::arrange::PIANO_BARS;
use crate::text;

/// The least depth a dragged vibrato keeps (a vibrato of no depth is none;
/// removing one is a right-click or its panel's button).
const LEAST_DRAGGED_DEPTH: f32 = 5.0;

/// One note of a sung phrase, as the piano roll draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineNote {
    /// The note.
    pub id: NoteId,
    /// Its key.
    pub key: u8,
    /// When it starts, in seconds of the song.
    pub start: f64,
    /// When it ends.
    pub end: f64,
}

/// A phrase of the open pattern, as the voice sings it.
#[derive(Clone, Debug)]
pub struct LinePhrase {
    /// Its notes, in order.
    pub notes: Vec<LineNote>,
    /// The pitch sung through it.
    pub pitch: PhrasePitch,
}

/// The pitch the voice sings through the open pattern (decision 0028),
/// where the pattern is first placed in the song: the same phrases and the
/// same arithmetic the engine sings with, so what is drawn is what is
/// sung.
#[derive(Clone, Debug)]
pub struct SungLine {
    /// The song tick where the pattern's start plays.
    zero: Ticks,
    tempo: TempoMap,
    /// The phrases, in order.
    pub phrases: Vec<LinePhrase>,
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "tick positions are far below 2^52"
)]
impl SungLine {
    /// The second of the song when a point of the pattern plays.
    pub fn seconds(&self, tick: f64) -> f64 {
        self.tempo
            .seconds_at(self.zero + Ticks(tick.round() as i64))
    }

    /// The point of the pattern that plays at a second of the song.
    pub fn tick(&self, seconds: f64) -> f64 {
        self.tempo.ticks_at(seconds) - self.zero.0 as f64
    }

    /// The phrase sounding at a second of the song (from its first note's
    /// start to its last note's end).
    pub fn phrase_at(&self, seconds: f64) -> Option<&LinePhrase> {
        self.phrases.iter().find(|phrase| {
            let (Some(first), Some(last)) = (phrase.notes.first(), phrase.notes.last()) else {
                return false;
            };
            seconds >= first.start && seconds <= last.end
        })
    }

    /// The pitch sung at a second of the song, if the voice sings then.
    pub fn pitch_at(&self, seconds: f64) -> Option<f64> {
        self.phrase_at(seconds)
            .map(|phrase| phrase.pitch.at(seconds))
    }

    /// The note in charge at a second of the song, with its place in its
    /// phrase: the one that has taken over by then.
    pub fn note_at(&self, seconds: f64) -> Option<(LineNote, usize, &LinePhrase)> {
        let phrase = self.phrase_at(seconds)?;
        let index = (0..phrase.notes.len())
            .rev()
            .find(|&i| phrase.pitch.takes_over(i) <= seconds)
            .unwrap_or(0);
        Some((phrase.notes[index], index, phrase))
    }
}

/// Where each of a note's points is, in its own terms (milliseconds from
/// its start, cents from its key), from a point on screen in the song's.
pub fn point_from(note: &LineNote, seconds: f64, pitch: f64) -> (f32, f32) {
    #[allow(clippy::cast_possible_truncation, reason = "small numbers")]
    (
        ((seconds - note.start) * 1000.0) as f32,
        ((pitch - f64::from(note.key)) * 100.0) as f32,
    )
}

/// The voicebanks the piano roll checks lyrics against, by name, read the
/// first time (their `oto.ini` only: no sound is read).
#[derive(Default)]
pub(crate) struct VoicebankCache {
    root: Option<PathBuf>,
    banks: HashMap<String, Option<Arc<VoiceBank>>>,
}

impl std::fmt::Debug for VoicebankCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VoicebankCache")
            .field("root", &self.root)
            .field("banks", &self.banks.len())
            .finish()
    }
}

impl VoicebankCache {
    fn bank(&mut self, name: &str) -> Option<Arc<VoiceBank>> {
        let root = self.root.as_ref()?;
        self.banks
            .entry(name.to_owned())
            .or_insert_with(|| {
                daw_formats::utau::read_voicebank(&root.join(name))
                    .ok()
                    .map(Arc::new)
            })
            .clone()
    }
}

impl Session {
    /// Where the installed voicebanks are (the program's own folder).
    pub fn set_voicebanks_folder(&mut self, root: Option<&Path>) {
        self.voicebanks = VoicebankCache {
            root: root.map(Path::to_path_buf),
            banks: HashMap::new(),
        };
    }

    /// Forgets what is known of a voicebank (it was added again, or
    /// removed).
    pub fn forget_voicebank(&mut self, name: &str) {
        self.voicebanks.banks.remove(name);
    }

    /// Adds a track sung by the voicebank in the folder `voicebank` (shown as
    /// `name`), with an empty four-bar pattern at the start of the song, and
    /// puts it in focus.
    pub fn add_voice_track(&mut self, voicebank: &str, name: &str) -> TrackId {
        let track_name = self.unique_track_name(name);
        let voicebank = voicebank.to_owned();
        let track = self.doc.edit(text::EDIT_ADD_VOICE, |p| {
            let track = p.add_track(&track_name, Instrument::Voice { voicebank });
            let length = p.meter.bar_start(PIANO_BARS);
            let pattern = p.add_pattern_for(track, &text::pattern_name(1), length);
            p.add_clip(track, Ticks::ZERO, length, pattern);
            track
        });
        self.focus_track(track);
        track
    }

    /// The voicebank of the track in focus, if it is a voice.
    pub fn voicebank(&self) -> Option<&str> {
        self.project()
            .track(self.track)
            .and_then(|t| t.instrument.voicebank())
    }

    /// Whether the open pattern is sung.
    pub fn is_voice_track(&self) -> bool {
        self.voicebank().is_some()
    }

    /// What a note of the open pattern sings: its own lyric, or "a".
    pub fn lyric(&self, note: NoteId) -> &str {
        self.pattern().lyric(note).unwrap_or(DEFAULT_LYRIC)
    }

    /// Gives lyrics to the notes of the open pattern from `note` on: the
    /// words of `phrase` (separated by spaces) go to it and to the notes
    /// after it, in order (by start, then from the highest key, as the
    /// melody is usually on top). One undo step. Returns the note after the
    /// last one given a word, to go on typing there.
    pub fn sing_from(&mut self, note: NoteId, phrase: &str) -> Option<NoteId> {
        let mut order: Vec<(Ticks, u8, NoteId)> = self
            .pattern()
            .notes
            .iter()
            .map(|n| (n.start, n.key, n.id))
            .collect();
        order.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let first = order.iter().position(|(_, _, id)| *id == note)?;
        let words: Vec<&str> = phrase.split_whitespace().collect();
        let lyrics: Vec<(NoteId, String)> = if words.is_empty() {
            // Nothing typed: the note sings the default again.
            vec![(note, String::new())]
        } else {
            order[first..]
                .iter()
                .zip(&words)
                .map(|((_, _, id), word)| (*id, (*word).to_owned()))
                .collect()
        };
        let given = lyrics.len();
        let pattern = self.pattern;
        self.doc
            .edit(text::EDIT_LYRICS, |p| p.set_lyrics(pattern, &lyrics));
        order.get(first + given).map(|(_, _, id)| *id)
    }

    /// Whether the voicebank of the track in focus has a recording for a
    /// note's lyric, sung after the note before it (true when there is no
    /// voicebank to ask, or the lyric is a rest).
    pub fn lyric_found(&mut self, note: NoteId) -> bool {
        let Some(name) = self.voicebank().map(str::to_owned) else {
            return true;
        };
        let pattern = self.pattern();
        let Some(this) = pattern.note(note).copied() else {
            return true;
        };
        let lyric = self.lyric(note).to_owned();
        if is_rest(&lyric) {
            return true;
        }
        // The note sounding just before (touching it), for VCV.
        let previous = pattern
            .notes
            .iter()
            .filter(|n| {
                n.id != note
                    && n.end() <= this.start
                    && this.start - n.end() < Ticks::note_value(1, 16)
            })
            .max_by_key(|n| n.end())
            .map(|n| self.lyric(n.id).to_owned());
        let Some(bank) = self.voicebanks.bank(&name) else {
            return true;
        };
        bank.find(&alias_candidates(&lyric, previous.as_deref()), this.key)
            .is_some()
    }

    /// Whether voice tracks' singing is still being rendered (silent where
    /// it is not ready).
    pub fn is_rendering_voices(&self) -> bool {
        self.engine.is_rendering_voices()
    }

    // ----- Expression (decision 0028)

    /// Whether the piano roll edits the sung pitch (only on voice tracks).
    pub fn pitch_mode(&self) -> bool {
        self.pitch_mode && self.is_voice_track()
    }

    /// Edits the sung pitch, or the notes.
    pub fn set_pitch_mode(&mut self, on: bool) {
        self.pitch_mode = on;
    }

    /// Switches between editing the notes and the sung pitch, on a voice
    /// track.
    pub fn toggle_pitch_mode(&mut self) {
        if self.is_voice_track() {
            self.pitch_mode = !self.pitch_mode;
        }
    }

    /// How a note of the open pattern is sung (the plain way if it has
    /// nothing of its own).
    pub fn note_expression(&self, note: NoteId) -> NoteExpression {
        self.pattern().expression(note).cloned().unwrap_or_default()
    }

    /// The pitch the voice sings through the open pattern, on a voice
    /// track: its notes as the engine sings them (one at a time, rests
    /// left out, in phrases), where the pattern is first placed.
    pub fn sung_line(&self) -> Option<SungLine> {
        if !self.is_voice_track() {
            return None;
        }
        let placement = self.placement();
        let zero = placement.start - placement.offset;
        let tempo = self.project().tempo.clone();
        let pattern = self.pattern();
        let timed: Vec<Timed<NoteId>> = pattern
            .notes
            .iter()
            .filter(|n| n.start < pattern.length)
            .filter(|n| !is_rest(pattern.lyric(n.id).unwrap_or(DEFAULT_LYRIC)))
            .map(|n| Timed {
                start: tempo.seconds_at(zero + n.start),
                end: tempo.seconds_at(zero + n.end()),
                key: n.key,
                data: n.id,
            })
            .collect();
        let phrases = phrases(timed)
            .into_iter()
            .map(|phrase| {
                let curve: Vec<CurveNote<'_>> = phrase
                    .iter()
                    .map(|n| CurveNote {
                        start: n.start,
                        end: n.end,
                        key: n.key,
                        expression: pattern.expression(n.data),
                    })
                    .collect();
                LinePhrase {
                    pitch: PhrasePitch::new(&curve),
                    notes: phrase
                        .iter()
                        .map(|n| LineNote {
                            id: n.data,
                            key: n.key,
                            start: n.start,
                            end: n.end,
                        })
                        .collect(),
                }
            })
            .collect();
        Some(SungLine {
            zero,
            tempo,
            phrases,
        })
    }

    /// Changes how notes of the open pattern are sung, as one edit (or a
    /// step of the gesture under way).
    pub fn change_expression(
        &mut self,
        label: &str,
        notes: &[NoteId],
        change: impl FnMut(&daw_model::Note, &mut NoteExpression),
    ) {
        if notes.is_empty() {
            return;
        }
        let pattern = self.pattern;
        self.doc
            .edit(label, |p| p.update_expressions(pattern, notes, change));
    }

    /// Adds a pitch point to a note (its plain glide becomes points of its
    /// own first); returns the new point's place among its points.
    pub fn add_pitch_point(&mut self, note: NoteId, ms: f32, cents: f32) -> Option<usize> {
        let points = self.note_expression(note).points().to_vec();
        let index = points.partition_point(|p| p.ms <= ms);
        let mut placed = None;
        self.change_expression(text::EDIT_ADD_PITCH_POINT, &[note], |_, e| {
            let mut points = e.points().to_vec();
            points.insert(index, PitchPoint::new(ms, cents));
            e.pitch = points;
            placed = Some(index);
        });
        placed
    }

    /// Moves a note's pitch point, kept between the points either side of
    /// it so the points stay in order.
    pub fn move_pitch_point(&mut self, note: NoteId, index: usize, ms: f32, cents: f32) {
        self.change_expression(text::EDIT_MOVE_PITCH_POINT, &[note], |_, e| {
            let mut points = e.points().to_vec();
            let low = index
                .checked_sub(1)
                .map_or(f32::NEG_INFINITY, |i| points[i].ms);
            let high = points.get(index + 1).map_or(f32::INFINITY, |p| p.ms);
            if let Some(point) = points.get_mut(index) {
                point.ms = ms.clamp(low, high.max(low));
                point.cents = cents;
            }
            e.pitch = points;
        });
    }

    /// Removes a note's pitch point (with fewer than two left, the note
    /// glides the plain way again).
    pub fn remove_pitch_point(&mut self, note: NoteId, index: usize) {
        self.change_expression(text::EDIT_REMOVE_PITCH_POINT, &[note], |_, e| {
            let mut points = e.points().to_vec();
            if index < points.len() {
                points.remove(index);
            }
            e.pitch = points;
        });
    }

    /// The shape of the curve after a note's pitch point.
    pub fn set_pitch_shape(&mut self, note: NoteId, index: usize, shape: PitchShape) {
        self.change_expression(text::EDIT_PITCH_SHAPE, &[note], |_, e| {
            let mut points = e.points().to_vec();
            if let Some(point) = points.get_mut(index) {
                point.shape = shape;
            }
            e.pitch = points;
        });
    }

    /// The selected notes glide the plain way again.
    pub fn plain_pitch_for_selection(&mut self) {
        let ids = self.selected_ids();
        self.change_expression(text::EDIT_PLAIN_PITCH, &ids, |_, e| e.pitch.clear());
    }

    /// The selected notes get the usual vibrato (those that have one keep
    /// theirs).
    pub fn add_vibrato_to_selection(&mut self) {
        let ids = self.selected_ids();
        self.change_expression(text::EDIT_ADD_VIBRATO, &ids, |_, e| {
            e.vibrato.get_or_insert_with(Vibrato::default);
        });
    }

    /// The selected notes lose their vibrato.
    pub fn remove_vibrato_from_selection(&mut self) {
        let ids = self.selected_ids();
        self.change_expression(text::EDIT_REMOVE_VIBRATO, &ids, |_, e| e.vibrato = None);
    }

    /// Sets a note's vibrato (`None` removes it).
    pub fn set_vibrato(&mut self, note: NoteId, vibrato: Option<Vibrato>) {
        self.change_expression(text::EDIT_VIBRATO, &[note], |_, e| e.vibrato = vibrato);
    }

    /// Moves a note's vibrato as its handle is dragged: where it starts (in
    /// seconds after the note's start) and how deep it is (cents).
    pub fn drag_vibrato(&mut self, note: NoteId, start: f32, depth: f32) {
        self.change_expression(text::EDIT_VIBRATO, &[note], |_, e| {
            let mut vibrato = e.vibrato.unwrap_or_default();
            vibrato.start = start.max(0.0);
            vibrato.depth = depth.max(LEAST_DRAGGED_DEPTH);
            e.vibrato = Some(vibrato);
        });
    }

    /// Whether a note's points are its own (rather than the plain glide).
    pub fn has_own_pitch(&self, note: NoteId) -> bool {
        self.pattern()
            .expression(note)
            .is_some_and(|e| !e.pitch.is_empty() && e.pitch != PLAIN_GLIDE)
    }
}
