// SPDX-License-Identifier: GPL-3.0-or-later

//! The open song and everything the interface does with it, with no
//! drawing: editing commands, playing notes, the transport, recording and
//! files. The views call these methods; tests call them directly.
//!
//! The song-level operations (views, tracks, patterns, clips) are in
//! `arrange`, the mixer and the effects in `mixing`, plugins and
//! automation in `plugins`.

mod arrange;
mod audio;
mod audio_store;
mod conductor;
mod files;
mod kits;
mod library;
mod mixing;
mod plugins;
mod recorder;
mod synth;
mod theory;
mod voice;

pub use audio::{LiveTake, SoundState, TakeNote};
pub use files::SongFile;
pub use mixing::MAX_GAIN;
pub use plugins::{LaneChoice, PluginSearch};
pub use theory::Draw;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use daw_engine::{Engine, EngineEvent, PlaybackScope, PluginTarget, RecordState};
use daw_model::{
    ClipContent, ClipId, Document, MAX_KEY, MeterMap, Note, NoteId, Pattern, PatternId, Project,
    Region, TICKS_PER_WHOLE, Ticks, TrackId,
};
use egui::Key;

use crate::export::{Export, ExportFormat, ExportTarget};
use crate::keys::{HIGHEST_OFFSET, note_offset};
use crate::settings::{Settings, clamp_base};
use crate::text;

/// What the large surface in the middle of the window shows (decision
/// 0014).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// The whole song: tracks and their clips on a timeline.
    Song,
    /// One pattern: the step grid for drums, the piano roll for the rest.
    Pattern,
}

/// A rhythmic grid that notes snap to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grid {
    /// Whole bars.
    Bar,
    /// A note value: 1/`n` of a whole note.
    Note(i64),
    /// No grid: notes go exactly where they are put.
    Off,
}

impl Grid {
    /// The choices offered, in the menu's groups: straight values coarse to
    /// fine, then triplets, quintuplets and septuplets (decision 0018), then
    /// no grid.
    pub const CHOICES: [Self; 14] = [
        Self::Bar,
        Self::Note(2),
        Self::Note(4),
        Self::Note(8),
        Self::Note(16),
        Self::Note(32),
        Self::Note(6),
        Self::Note(12),
        Self::Note(24),
        Self::Note(10),
        Self::Note(20),
        Self::Note(14),
        Self::Note(28),
        Self::Off,
    ];

    /// The grid's step at a position. With no grid, placement is exact but
    /// nudging still moves by a 1/128 note.
    pub fn step(self, meter: &MeterMap, at: Ticks) -> Ticks {
        match self {
            Self::Bar => meter.signature_at(at).bar_length(),
            Self::Note(n) => Ticks(TICKS_PER_WHOLE / n),
            Self::Off => Ticks(TICKS_PER_WHOLE / 128),
        }
    }

    /// Where something placed at `at` lands. Grid lines count from each
    /// bar line (decision 0018).
    pub fn snap_down(self, meter: &MeterMap, at: Ticks) -> Ticks {
        let at = at.max(Ticks::ZERO);
        match self {
            Self::Off => at,
            _ => meter.grid_floor(at, self.step(meter, at)),
        }
    }

    /// The nearest grid point.
    pub fn snap_nearest(self, meter: &MeterMap, at: Ticks) -> Ticks {
        match self {
            Self::Off => at,
            _ => meter.grid_round(at.max(Ticks::ZERO), self.step(meter, at)),
        }
    }
}

/// Lengths offered for new notes, as fractions of a whole note: straight,
/// dotted, then triplets, quintuplets and septuplets.
pub const NOTE_LENGTHS: [(i64, i64); 15] = [
    (1, 1),
    (1, 2),
    (1, 4),
    (1, 8),
    (1, 16),
    (1, 32),
    (3, 4),
    (3, 8),
    (3, 16),
    (3, 32),
    (1, 6),
    (1, 12),
    (1, 24),
    (1, 20),
    (1, 28),
];

/// How long a status message stays.
const STATUS_TIME: Duration = Duration::from_secs(5);

/// How long the interface trusts its own idea of the transport before the
/// audio thread has confirmed it.
const INTENT_TIME: Duration = Duration::from_millis(250);

/// Where a pattern sits on its track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// Where the clip starts in the song.
    pub start: Ticks,
    /// How long the clip is.
    pub length: Ticks,
    /// Which point of the pattern plays at the clip's start.
    pub offset: Ticks,
}

impl Placement {
    /// A song position as a position in the pattern.
    pub fn to_pattern(self, song: Ticks) -> Ticks {
        song - self.start + self.offset
    }

    /// A pattern position as a song position.
    pub fn to_song(self, pattern: Ticks) -> Ticks {
        pattern - self.offset + self.start
    }

    /// A song position as a position in a pattern of `length` that the
    /// clip repeats: inside the clip it folds back onto the pattern; past
    /// the clip's end it carries on (a recording that runs past the end
    /// lengthens the pattern).
    pub fn to_pattern_folded(self, song: Ticks, length: Ticks) -> Ticks {
        let at = self.to_pattern(song);
        if song >= self.start && song < self.start + self.length && length > Ticks::ZERO {
            Ticks(at.0.rem_euclid(length.0))
        } else {
            at
        }
    }
}

/// One recording pass that is still running.
#[derive(Clone, Copy, Debug)]
struct Take {
    /// Joins all of the take's notes into one undo step.
    merge_key: u64,
    count: usize,
}

/// Keys used to merge repeated small edits into one undo step.
const MERGE_NUDGE: u64 = 1;
const MERGE_TRANSPOSE: u64 = 2;
/// Takes count from here, far from the fixed keys above.
const FIRST_TAKE: u64 = 1 << 32;

/// The note the computer keyboard's Z key plays on drum tracks: the kick
/// (General MIDI drum keys).
const DRUM_BASE: u8 = 36;

/// The song being worked on, with its undo history, connected to the engine.
#[allow(clippy::struct_excessive_bools, reason = "independent preferences")]
pub struct Session {
    doc: Document,
    engine: Engine,
    path: Option<PathBuf>,
    view: View,
    track: TrackId,
    pattern: PatternId,
    /// The pattern last opened on each track (what clicking the track
    /// places and opens).
    current: HashMap<TrackId, PatternId>,
    selected_clip: Option<ClipId>,
    /// Whether a pattern plays in a loop while it is open.
    loop_pattern: bool,
    /// The scope the engine was last given (to notice changes).
    scope: Option<PlaybackScope>,
    export: Option<Export>,
    selection: HashSet<NoteId>,
    selection_revision: u64,
    /// The grid notes snap to.
    pub grid: Grid,
    /// Whether new and moved notes stay in the song's key (decision 0018).
    pub keep_in_key: bool,
    /// What a click in the piano roll places: a note or a chord.
    pub draw: Draw,
    /// The inversion chord stamps place (0 is root position).
    pub chord_inversion: usize,
    /// The notes of a chord just placed, sounding while the mouse holds it.
    chord_held: Vec<u8>,
    /// The length of new notes.
    pub note_length: Ticks,
    clipboard: Vec<Note>,
    clipboard_span: Ticks,
    paste_at: Option<Ticks>,
    keyboard_base: u8,
    drum_base: u8,
    /// The voicebanks lyrics are checked against (decision 0027).
    voicebanks: voice::VoicebankCache,
    keyboard_velocity: f32,
    held_keys: HashMap<Key, u8>,
    pointer_note: Option<u8>,
    outside_notes: [bool; 128],
    metronome: bool,
    metronome_when_recording: bool,
    metronome_volume: f32,
    count_in_bars: u32,
    snap_recording: bool,
    take: Option<Take>,
    takes: u64,
    recording_held: HashMap<u8, (Ticks, f32)>,
    record_requested: Option<Instant>,
    playing_intent: Option<(bool, Instant)>,
    awaiting_stop: bool,
    take_finished: bool,
    status: Option<(String, Instant)>,
    /// Where the search for plugins stands.
    plugin_search: PluginSearch,
    /// The plugin parameter last moved in a plugin's window.
    last_moved: Option<(PluginTarget, u32)>,
    /// Plugin parameters' names, by plugin identity and parameter.
    param_names: HashMap<(String, u32), String>,
    /// The song's audio files: their folder, and their sound in memory.
    audio: audio_store::AudioStore,
    /// Writes takes of sound as they are recorded.
    recorder: Option<recorder::Recorder>,
    /// The audio track in focus in the song view, if one is.
    audio_focus: Option<TrackId>,
    /// The take of sound asked for, counting in or recording.
    audio_take: Option<audio::AudioTake>,
    /// Takes that ended, waiting for the recorder to finish their files.
    finishing: Vec<audio::AudioTake>,
    /// Until when the input device is wanted after a take.
    input_until: Option<Instant>,
    /// The input meter's level, falling back slowly.
    input_meter: f32,
    /// The synth tracks whose windows are open.
    synth_windows: Vec<TrackId>,
    /// Writes a recovery copy at once, before risky moments (decision
    /// 0021).
    safety: Option<Box<dyn SafetyCopy>>,
    /// The sound library's preview (decision 0023).
    preview: library::Preview,
    /// The status line says which instruments are being read.
    reading_notice: bool,
}

/// Writes a recovery copy of the song at once and waits until it is on
/// disk (the program's autosave gives the session one, decision 0021).
pub trait SafetyCopy {
    /// Writes `project` (from the file `path`, if it has one).
    fn write_now(&self, project: &Project, path: Option<&Path>);
}

impl Session {
    /// Opens a project in the engine, showing the song. `path` is where it
    /// was loaded from (or `None` for a new song); `saved` says whether the
    /// file on disk matches it.
    pub fn new(mut engine: Engine, project: Project, path: Option<PathBuf>, saved: bool) -> Self {
        let project = editable(project);
        let rate = engine.config().sample_rate;
        let recorder = engine
            .take_recorded_audio()
            .map(|source| recorder::Recorder::start(source, rate));
        let (track, pattern) = first_focus(&project);
        let doc = if saved {
            Document::new(project)
        } else {
            Document::new_unsaved(project)
        };
        let mut session = Self {
            doc,
            engine,
            path,
            view: View::Song,
            track,
            pattern,
            current: HashMap::new(),
            selected_clip: None,
            loop_pattern: true,
            scope: None,
            export: None,
            selection: HashSet::new(),
            selection_revision: 0,
            grid: Grid::Note(16),
            keep_in_key: false,
            draw: Draw::Note,
            chord_inversion: 0,
            chord_held: Vec::new(),
            note_length: Ticks::QUARTER,
            clipboard: Vec::new(),
            clipboard_span: Ticks::ZERO,
            paste_at: None,
            keyboard_base: 48,
            drum_base: DRUM_BASE,
            voicebanks: voice::VoicebankCache::default(),
            keyboard_velocity: 0.8,
            held_keys: HashMap::new(),
            pointer_note: None,
            outside_notes: [false; 128],
            metronome: false,
            metronome_when_recording: true,
            metronome_volume: 0.8,
            count_in_bars: 1,
            snap_recording: false,
            take: None,
            takes: FIRST_TAKE,
            recording_held: HashMap::new(),
            record_requested: None,
            playing_intent: None,
            awaiting_stop: false,
            take_finished: false,
            status: None,
            plugin_search: PluginSearch::default(),
            last_moved: None,
            param_names: HashMap::new(),
            audio: audio_store::AudioStore::new(rate),
            recorder,
            audio_focus: None,
            audio_take: None,
            finishing: Vec::new(),
            input_until: None,
            input_meter: 0.0,
            synth_windows: Vec::new(),
            safety: None,
            preview: library::Preview::default(),
            reading_notice: false,
        };
        session.selection_revision = session.doc.revision();
        session.load_song_audio();
        session.sync_engine();
        session
    }

    /// The song a new user starts with: a drum track and a piano track,
    /// shown as the whole song, like every song opened (decision 0014).
    pub fn starter(engine: Engine) -> Self {
        Self::new(engine, starter_project(), None, true)
    }

    /// Takes the preferences that affect playing and recording.
    pub fn apply_settings(&mut self, settings: &Settings) {
        self.metronome = settings.metronome;
        self.metronome_when_recording = settings.metronome_when_recording;
        self.metronome_volume = settings.metronome_volume;
        self.count_in_bars = settings.count_in_bars;
        self.snap_recording = settings.snap_recording;
        self.keyboard_base = clamp_base(settings.keyboard_base);
        self.keyboard_velocity = settings.keyboard_velocity;
        self.send_metronome();
    }

    /// Writes the preferences this session changed back into `settings`.
    pub fn store_settings(&self, settings: &mut Settings) {
        settings.metronome = self.metronome;
        settings.metronome_when_recording = self.metronome_when_recording;
        settings.metronome_volume = self.metronome_volume;
        settings.count_in_bars = self.count_in_bars;
        settings.snap_recording = self.snap_recording;
        settings.keyboard_base = self.keyboard_base;
        settings.keyboard_velocity = self.keyboard_velocity;
    }

    // ----- What is open

    /// Gives the session a way to write a recovery copy at once.
    pub fn set_safety_copy(&mut self, safety: Box<dyn SafetyCopy>) {
        self.safety = Some(safety);
    }

    /// Writes a recovery copy now, if the song has unsaved changes: right
    /// before a moment a plugin could crash the program (it loads, its
    /// window opens).
    pub(crate) fn safety_copy(&mut self) {
        if !self.is_dirty() || self.safety.is_none() {
            return;
        }
        self.capture_plugin_states();
        if let Some(safety) = &self.safety {
            safety.write_now(self.doc.project(), self.path.as_deref());
        }
    }

    /// The project as it is now.
    pub fn project(&self) -> &Project {
        self.doc.project()
    }

    /// The document with its undo history.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub(crate) fn document_mut(&mut self) -> &mut Document {
        &mut self.doc
    }

    /// Starts a gesture (a drag): the edits until [`Self::end_gesture`]
    /// become one undo step named `label`.
    pub fn begin_gesture(&mut self, label: &str) {
        self.doc.begin_gesture(label);
    }

    /// Ends the gesture in progress.
    pub fn end_gesture(&mut self) {
        self.doc.end_gesture();
    }

    /// Abandons the gesture in progress, putting everything back.
    pub fn cancel_gesture(&mut self) {
        self.doc.cancel_gesture();
    }

    /// The engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The engine, to change it.
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Where the song is saved, if it has been.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The song's name as people see it: the file name, or the title, or
    /// "Untitled song".
    pub fn display_name(&self) -> String {
        if let Some(stem) = self.path.as_deref().and_then(Path::file_stem) {
            return stem.to_string_lossy().into_owned();
        }
        let title = self.project().title.trim();
        if title.is_empty() {
            text::UNTITLED.to_owned()
        } else {
            title.to_owned()
        }
    }

    /// Whether there are changes not saved to a file.
    pub fn is_dirty(&self) -> bool {
        self.doc.is_dirty()
    }

    /// The track in focus.
    pub fn track_id(&self) -> TrackId {
        self.track
    }

    /// The pattern in focus.
    pub fn pattern_id(&self) -> PatternId {
        self.pattern
    }

    /// The pattern being edited.
    pub fn pattern(&self) -> &Pattern {
        static EMPTY: std::sync::OnceLock<Pattern> = std::sync::OnceLock::new();
        self.project().pattern(self.pattern).unwrap_or_else(|| {
            EMPTY.get_or_init(|| Pattern {
                id: PatternId(0),
                name: String::new(),
                length: Ticks::WHOLE,
                notes: Vec::new(),
                track: None,
                step: daw_model::default_step(),
                swing: daw_model::Swing::default(),
                lyrics: std::collections::BTreeMap::new(),
                expression: std::collections::BTreeMap::new(),
            })
        })
    }

    /// The pattern being edited, shared (cheap to keep while editing).
    pub fn pattern_arc(&self) -> std::sync::Arc<Pattern> {
        self.project()
            .patterns
            .iter()
            .find(|p| p.id == self.pattern)
            .cloned()
            .unwrap_or_else(|| std::sync::Arc::new(self.pattern().clone()))
    }

    /// The name of the track in focus.
    pub fn track_name(&self) -> &str {
        self.project()
            .track(self.track)
            .map_or("", |t| t.name.as_str())
    }

    /// Where the pattern in focus sits on its track.
    pub fn placement(&self) -> Placement {
        placement(self.project(), self.track, self.pattern).unwrap_or(Placement {
            start: Ticks::ZERO,
            length: self.pattern().length,
            offset: Ticks::ZERO,
        })
    }

    /// The song's time signatures as the open pattern sees them: its bars
    /// count from where it starts in the song (its first placement), so the
    /// grid and bar lines of the pattern editors match the song's there.
    pub fn pattern_meter(&self) -> MeterMap {
        let placement = self.placement();
        let meter = &self.project().meter;
        let zero = placement.start - placement.offset;
        let at = meter.position(zero.max(Ticks::ZERO));
        if zero >= Ticks::ZERO && at.beat == 0 && at.ticks == Ticks::ZERO {
            meter.from_bar(at.bar)
        } else {
            MeterMap::constant(meter.signature_at(zero.max(Ticks::ZERO)))
        }
    }

    /// The pattern's length in whole bars (rounded up).
    pub fn pattern_bars(&self) -> u32 {
        let placement = self.placement();
        let meter = &self.project().meter;
        let first = meter.position(placement.start).bar;
        let end = meter.position(placement.start + self.pattern().length);
        let partial = u32::from(end.beat > 0 || end.ticks > Ticks::ZERO);
        (end.bar + partial).saturating_sub(first).max(1)
    }

    // ----- Frame by frame

    /// Brings everything up to date: sends changes to the engine, collects
    /// what it reports (recorded notes, the transport stopping), and stops
    /// at the end of the pattern when not looping. Call once per frame.
    pub fn update(&mut self) {
        self.sync_engine();
        for event in self.engine.poll() {
            self.handle_engine_event(event);
        }
        self.plugin_idle();
        self.poll_audio();
        self.poll_preview();
        for problem in self.engine.take_problems() {
            self.set_status(text::problem(&problem));
        }
        self.note_reading();
        if self.doc.revision() != self.selection_revision {
            self.selection_revision = self.doc.revision();
            self.keep_focus_valid();
            let pattern = self.pattern();
            let present: HashSet<NoteId> = pattern.notes.iter().map(|n| n.id).collect();
            self.selection.retain(|id| present.contains(id));
        }
        self.poll_export();
        if let Some((_, since)) = self.playing_intent
            && (self.engine.is_playing() == self.is_playing() || since.elapsed() > INTENT_TIME)
        {
            self.playing_intent = None;
        }
        if self.record_requested.is_some_and(|since| {
            self.engine.record_state() != RecordState::Idle || since.elapsed() > INTENT_TIME
        }) {
            self.record_requested = None;
        }
        self.stop_at_the_end();
        if self
            .status
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > STATUS_TIME)
        {
            self.status = None;
        }
    }

    fn sync_engine(&mut self) {
        let scope = self.playback_scope();
        if self.scope.is_some_and(|old| old != scope) {
            self.relocate_for(scope);
        }
        self.scope = Some(scope);
        self.engine
            .sync(self.doc.project(), self.doc.revision(), scope);
        self.engine.set_live_target(Some(self.track));
    }

    /// What Space plays (decision 0014): in the song view, the song; in a
    /// pattern, the part of the song where the pattern is placed (looping
    /// unless the loop is off), or the pattern alone if it is not placed.
    pub fn playback_scope(&self) -> PlaybackScope {
        if self.view == View::Song {
            // A take of sound is one clip: the song plays on through its loop.
            if self.audio_take.is_some() {
                return PlaybackScope::Unlooped;
            }
            return PlaybackScope::Song;
        }
        match placement(self.project(), self.track, self.pattern) {
            Some(place) if self.loop_pattern => {
                Region::new(place.start, place.start + place.length)
                    .map_or(PlaybackScope::Unlooped, PlaybackScope::Looping)
            }
            Some(_) => PlaybackScope::Unlooped,
            None => PlaybackScope::Pattern {
                track: self.track,
                pattern: self.pattern,
                looping: self.loop_pattern,
            },
        }
    }

    /// The part of the timeline that plays in a loop now, if any (in the
    /// pattern's own time while a pattern that is not placed plays alone).
    pub fn loop_in_play(&self) -> Option<Region> {
        match self.playback_scope() {
            PlaybackScope::Song => self.project().loop_region,
            PlaybackScope::Looping(region) => Some(region),
            PlaybackScope::Unlooped => None,
            PlaybackScope::Pattern { looping, .. } => looping
                .then(|| Region::new(Ticks::ZERO, self.pattern().length))
                .flatten(),
        }
    }

    /// Moves the play position into what a new scope plays, when the view
    /// changes (a pattern alone counts from its own start).
    fn relocate_for(&mut self, scope: PlaybackScope) {
        let at = self.engine.position();
        let target = match scope {
            PlaybackScope::Looping(region) if at < region.start || at >= region.end => {
                Some(region.start)
            }
            PlaybackScope::Pattern { .. } => Some(Ticks::ZERO),
            PlaybackScope::Song | PlaybackScope::Unlooped
                if matches!(self.scope, Some(PlaybackScope::Pattern { .. })) =>
            {
                Some(self.placement().start)
            }
            _ => None,
        };
        if let Some(target) = target {
            self.engine.locate(target);
        }
    }

    fn stop_at_the_end(&mut self) {
        if self.is_looping()
            || self.awaiting_stop
            || !self.engine.is_playing()
            || self.engine.record_state() != RecordState::Idle
            || self.playing_intent.is_some()
        {
            return;
        }
        let end = match self.view {
            View::Song => self.project().song_end(),
            View::Pattern => {
                let placement = self.placement();
                placement.start + placement.length
            }
        };
        if self.engine.position() >= end {
            self.stop();
        }
    }

    fn handle_engine_event(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::RecordingStarted { .. } => {
                self.takes += 1;
                self.take = Some(Take {
                    merge_key: self.takes,
                    count: 0,
                });
                self.recording_held.clear();
            }
            EngineEvent::RecordingNoteStarted {
                key,
                velocity,
                start,
            } => {
                self.recording_held.insert(key, (start, velocity));
            }
            EngineEvent::RecordedNote {
                key,
                velocity,
                start,
                end,
            } => {
                self.recording_held.remove(&key);
                // A note held across the loop's end carries on from its start.
                if let Some(region) = self.loop_in_play()
                    && end == region.end
                    && start < end
                    && self.key_is_down(key)
                {
                    self.recording_held.insert(key, (region.start, velocity));
                }
                self.add_recorded_note(key, velocity, start, end);
            }
            EngineEvent::RecordingStopped => {
                if let Some(take) = self.take.take() {
                    self.set_status(text::recorded(take.count));
                    self.take_finished = true;
                }
                self.recording_held.clear();
                self.audio_recording_ended();
            }
            EngineEvent::Stopped => {
                self.awaiting_stop = false;
            }
            EngineEvent::AudioRecordingStarted {
                take, at, latency, ..
            } => self.audio_recording_started(take, at, latency),
            EngineEvent::AudioRecordingStopped {
                take,
                frames,
                dropped,
            } => self.audio_recording_stopped(take, frames, dropped),
        }
    }

    fn key_is_down(&self, key: u8) -> bool {
        self.held_keys.values().any(|&k| k == key)
            || self.pointer_note == Some(key)
            || self.chord_held.contains(&key)
            || self.outside_notes[usize::from(key & 0x7F)]
    }

    fn add_recorded_note(&mut self, key: u8, velocity: f32, start: Ticks, end: Ticks) {
        let Some(mut take) = self.take else {
            return;
        };
        let placement = self.placement();
        let pattern_length = self.pattern().length;
        // What was played in a swung pattern is written where it plays back
        // as heard (decision 0018).
        let swing = self.pattern().swing;
        let played = placement.to_pattern_folded(start, pattern_length);
        let written = swing.remove(played);
        let written_end = swing.remove(played + (end - start).max(Ticks(1)));
        let mut note = Note {
            id: NoteId(0),
            start: written,
            length: (written_end - written).max(Ticks(1)),
            key,
            velocity,
            pitch_offset: 0.0,
        };
        if self.snap_recording {
            let mut notes = [note];
            let step = self.grid.step(&self.pattern_meter(), note.start);
            daw_model::quantize_starts(&mut notes, step);
            note = notes[0];
            if self.loop_in_play().is_some() && note.start >= pattern_length {
                note.start -= pattern_length;
            }
        }
        note.start = note.start.max(Ticks::ZERO);
        take.count += 1;
        self.take = Some(take);
        let (track, pattern) = (self.track, self.pattern);
        self.doc
            .edit_merged(&text::edit_record(take.count), take.merge_key, |p| {
                p.add_note(pattern, note);
                grow_to_fit(p, track, pattern, note.end());
            });
    }

    /// Whether a recording has just ended (once: reading it clears it), so
    /// the editor can bring the new notes into view.
    pub fn take_finished(&mut self) -> bool {
        std::mem::take(&mut self.take_finished)
    }

    /// Notes that are being recorded right now (key still down): key,
    /// velocity and start, in pattern time.
    pub fn notes_being_recorded(&self) -> Vec<(u8, f32, Ticks)> {
        let placement = self.placement();
        let length = self.pattern().length;
        let mut notes: Vec<_> = self
            .recording_held
            .iter()
            .map(|(&key, &(start, velocity))| {
                (key, velocity, placement.to_pattern_folded(start, length))
            })
            .collect();
        notes.sort_by_key(|n| n.0);
        notes
    }

    // ----- Status line

    /// Shows a message for a few seconds.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now()));
        self.reading_notice = false;
    }

    /// The message to show, if any.
    pub fn status(&self) -> Option<&str> {
        self.status.as_ref().map(|(text, _)| text.as_str())
    }

    // ----- Transport

    /// Whether the song is playing (as soon as play is pressed, before the
    /// audio thread has confirmed it).
    pub fn is_playing(&self) -> bool {
        match self.playing_intent {
            Some((playing, _)) => playing,
            None => self.engine.is_playing(),
        }
    }

    /// What the recorder is doing.
    pub fn record_state(&self) -> RecordState {
        match self.engine.record_state() {
            RecordState::Idle if self.record_requested.is_some() => {
                RecordState::CountIn(Ticks::ZERO)
            }
            state => state,
        }
    }

    /// Whether recording or counting in.
    pub fn is_recording(&self) -> bool {
        self.record_state() != RecordState::Idle
    }

    /// The play position in the pattern being edited (folded back onto the
    /// pattern where its clip repeats it).
    pub fn position(&self) -> Ticks {
        self.placement()
            .to_pattern_folded(self.engine.position(), self.pattern().length)
    }

    /// The play position in the song (in the pattern's own time while a
    /// pattern that is not placed plays alone).
    pub fn song_position(&self) -> Ticks {
        self.engine.position()
    }

    /// Starts playback.
    pub fn play(&mut self) {
        self.sync_engine();
        if let Some(PlaybackScope::Looping(region)) = self.scope {
            let at = self.engine.position();
            if at < region.start || at >= region.end {
                self.engine.locate(region.start);
            }
        }
        self.engine.play();
        self.playing_intent = Some((true, Instant::now()));
    }

    /// Stops playback and recording. Stopping when already stopped goes
    /// back to the start.
    pub fn stop(&mut self) {
        self.engine.stop();
        self.awaiting_stop = true;
        self.record_requested = None;
        self.playing_intent = Some((false, Instant::now()));
    }

    /// Play if stopped, stop if playing or recording.
    pub fn toggle_play(&mut self) {
        if self.is_playing() || self.is_recording() {
            self.stop();
        } else {
            self.play();
        }
    }

    /// Starts recording into the pattern in focus (after the count-in), or
    /// stops if already recording. In the song view it opens the pattern
    /// first: recording always goes into the pattern on screen.
    pub fn toggle_record(&mut self) {
        if self.is_recording() {
            self.stop();
            return;
        }
        if let Some(track) = self.audio_focus.filter(|_| self.view == View::Song) {
            self.record_audio(track);
            return;
        }
        if self.view == View::Song {
            let (track, pattern) = (self.track, self.pattern);
            self.open_pattern(track, pattern);
        }
        self.sync_engine();
        self.engine.record(self.track, self.count_in_bars);
        self.record_requested = Some(Instant::now());
    }

    /// Moves the play position to a point in the pattern.
    pub fn locate(&mut self, at: Ticks) {
        let at = at.max(Ticks::ZERO);
        self.engine.locate(self.placement().to_song(at));
        self.paste_at = Some(at);
    }

    /// Moves the play position to a point in the song.
    pub fn locate_song(&mut self, at: Ticks) {
        self.engine.locate(at.max(Ticks::ZERO));
    }

    /// Back to the start of the pattern, or of the song.
    pub fn to_start(&mut self) {
        match self.view {
            View::Song => self.locate_song(Ticks::ZERO),
            View::Pattern => self.locate(Ticks::ZERO),
        }
    }

    /// Whether what is on screen plays in a loop: the open pattern, or
    /// the song's loop region in the song view.
    pub fn is_looping(&self) -> bool {
        match self.view {
            View::Song => self.project().loop_region.is_some(),
            View::Pattern => self.loop_pattern,
        }
    }

    /// Loop on or off. In the song view, the loop covers the whole song
    /// until a part is chosen on the ruler.
    pub fn toggle_loop(&mut self) {
        match self.view {
            View::Pattern => self.loop_pattern = !self.loop_pattern,
            View::Song => {
                let region = if self.is_looping() {
                    None
                } else {
                    Region::new(Ticks::ZERO, self.song_length())
                };
                self.doc.edit(text::EDIT_LOOP, |p| p.loop_region = region);
            }
        }
    }

    /// Loops a part of the song (or nothing).
    pub fn set_loop_region(&mut self, region: Option<Region>) {
        if self.project().loop_region != region {
            self.doc.edit(text::EDIT_LOOP, |p| p.loop_region = region);
        }
    }

    /// Whether the metronome clicks while playing.
    pub fn metronome(&self) -> bool {
        self.metronome
    }

    /// Metronome on or off while playing.
    pub fn toggle_metronome(&mut self) {
        self.metronome = !self.metronome;
        self.send_metronome();
    }

    /// Whether the metronome clicks while recording.
    pub fn metronome_when_recording(&self) -> bool {
        self.metronome_when_recording
    }

    /// Metronome on or off while recording.
    pub fn set_metronome_when_recording(&mut self, on: bool) {
        self.metronome_when_recording = on;
        self.send_metronome();
    }

    /// The metronome's volume, 0 to 1.
    pub fn metronome_volume(&self) -> f32 {
        self.metronome_volume
    }

    /// Changes the metronome's volume.
    pub fn set_metronome_volume(&mut self, volume: f32) {
        self.metronome_volume = volume.clamp(0.0, 1.0);
        self.send_metronome();
    }

    fn send_metronome(&mut self) {
        self.engine.set_metronome(
            self.metronome,
            self.metronome_when_recording,
            self.metronome_volume,
        );
    }

    /// Bars of count-in before recording.
    pub fn count_in_bars(&self) -> u32 {
        self.count_in_bars
    }

    /// Changes the count-in.
    pub fn set_count_in_bars(&mut self, bars: u32) {
        self.count_in_bars = bars.min(4);
    }

    /// Whether recorded notes snap to the grid.
    pub fn snap_recording(&self) -> bool {
        self.snap_recording
    }

    /// Recorded notes snap to the grid, or not.
    pub fn set_snap_recording(&mut self, on: bool) {
        self.snap_recording = on;
    }

    /// Makes the pattern a number of bars long (at least long enough for
    /// its notes).
    pub fn set_pattern_bars(&mut self, bars: u32) {
        let bars = bars.clamp(1, 999);
        let (track, pattern) = (self.track, self.pattern);
        self.doc.edit(text::EDIT_PATTERN_LENGTH, |p| {
            set_length_in_bars(p, track, pattern, bars);
        });
    }

    // ----- Undo

    /// Undoes the last step.
    pub fn undo(&mut self) {
        // The step undone may bring a plugin back: with its latest settings.
        self.capture_plugin_states();
        match self.doc.undo() {
            Some(label) => self.set_status(text::undone(&label)),
            None => self.set_status(text::NOTHING_TO_UNDO),
        }
    }

    /// Redoes the last undone step.
    pub fn redo(&mut self) {
        self.capture_plugin_states();
        match self.doc.redo() {
            Some(label) => self.set_status(text::redone(&label)),
            None => self.set_status(text::NOTHING_TO_REDO),
        }
    }

    // ----- Selection

    /// The selected notes.
    pub fn selection(&self) -> &HashSet<NoteId> {
        &self.selection
    }

    /// Whether a note is selected.
    pub fn is_selected(&self, id: NoteId) -> bool {
        self.selection.contains(&id)
    }

    /// Selects exactly these notes.
    pub fn select_only(&mut self, ids: impl IntoIterator<Item = NoteId>) {
        self.selection.clear();
        self.selection.extend(ids);
    }

    /// Adds or removes one note from the selection.
    pub fn toggle_selected(&mut self, id: NoteId) {
        if !self.selection.remove(&id) {
            self.selection.insert(id);
        }
    }

    /// Selects every note of the pattern.
    pub fn select_all(&mut self) {
        let ids: Vec<NoteId> = self.pattern().notes.iter().map(|n| n.id).collect();
        self.select_only(ids);
    }

    /// Clears the selection.
    pub fn deselect(&mut self) {
        self.selection.clear();
    }

    fn selected_ids(&self) -> Vec<NoteId> {
        self.pattern()
            .notes
            .iter()
            .filter(|n| self.selection.contains(&n.id))
            .map(|n| n.id)
            .collect()
    }

    fn selected_notes(&self) -> Vec<Note> {
        self.pattern()
            .notes
            .iter()
            .filter(|n| self.selection.contains(&n.id))
            .copied()
            .collect()
    }

    // ----- Editing notes

    /// Adds a note with the current length; selects it and returns its id.
    pub fn add_note(&mut self, start: Ticks, key: u8, velocity: f32) -> Option<NoteId> {
        let note = Note {
            id: NoteId(0),
            start: start.max(Ticks::ZERO),
            length: self.note_length.max(Ticks(1)),
            key: key.min(MAX_KEY),
            velocity,
            pitch_offset: 0.0,
        };
        let (track, pattern) = (self.track, self.pattern);
        let id = self.doc.edit(text::EDIT_ADD_NOTE, |p| {
            let id = p.add_note(pattern, note);
            grow_to_fit(p, track, pattern, note.end());
            id
        })?;
        self.select_only([id]);
        Some(id)
    }

    /// Deletes the selected notes.
    pub fn delete_selected(&mut self) {
        let ids = self.selected_ids();
        if ids.is_empty() {
            return;
        }
        let pattern = self.pattern;
        self.doc
            .edit(text::EDIT_DELETE_NOTES, |p| p.remove_notes(pattern, &ids));
        self.selection.clear();
    }

    /// Copies the selected notes.
    pub fn copy(&mut self) -> bool {
        let notes = self.selected_notes();
        let Some(first) = notes.iter().map(|n| n.start).min() else {
            return false;
        };
        let end = notes.iter().map(Note::end).max().unwrap_or(first);
        self.clipboard = notes
            .iter()
            .map(|n| Note {
                start: n.start - first,
                ..*n
            })
            .collect();
        self.clipboard_span = self.span_of(first, end);
        self.paste_at = Some(first + self.clipboard_span);
        true
    }

    /// Copies, then deletes, the selected notes.
    pub fn cut(&mut self) {
        if self.copy() {
            let first = self.selected_notes().iter().map(|n| n.start).min();
            self.delete_selected();
            self.paste_at = first;
        }
    }

    /// The copied notes, relative to the first one's start.
    pub fn clipboard(&self) -> &[Note] {
        &self.clipboard
    }

    /// Replaces the copied notes (from another program's clipboard).
    pub fn set_clipboard(&mut self, notes: Vec<Note>) {
        let end = notes.iter().map(Note::end).max().unwrap_or(Ticks::ZERO);
        self.clipboard_span = self.span_of(Ticks::ZERO, end);
        self.clipboard = notes;
    }

    /// Pastes the copied notes: after the copied notes the first time, then
    /// one after another, or at the play position after clicking the ruler.
    pub fn paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let at = self.paste_at.unwrap_or(Ticks::ZERO);
        let notes: Vec<Note> = self
            .clipboard
            .iter()
            .map(|n| Note {
                start: n.start + at,
                ..*n
            })
            .collect();
        self.insert_notes(text::EDIT_PASTE, &notes);
        self.paste_at = Some(at + self.clipboard_span);
    }

    /// Puts a copy of the selected notes right after them.
    pub fn duplicate(&mut self) {
        let notes = self.selected_notes();
        let Some(first) = notes.iter().map(|n| n.start).min() else {
            return;
        };
        let end = notes.iter().map(Note::end).max().unwrap_or(first);
        let span = self.span_of(first, end);
        let copies: Vec<Note> = notes
            .iter()
            .map(|n| Note {
                start: n.start + span,
                ..*n
            })
            .collect();
        self.insert_notes(text::EDIT_DUPLICATE, &copies);
    }

    fn insert_notes(&mut self, label: &str, notes: &[Note]) {
        let end = notes.iter().map(Note::end).max().unwrap_or(Ticks::ZERO);
        let (track, pattern) = (self.track, self.pattern);
        let ids = self.doc.edit(label, |p| {
            let ids = p.add_notes(pattern, notes);
            grow_to_fit(p, track, pattern, end);
            ids
        });
        self.select_only(ids);
    }

    /// How far apart repeated copies of notes from `first` to `end` go:
    /// their length rounded up to whole beats, or to whole bars once they
    /// are longer than half a bar.
    fn span_of(&self, first: Ticks, end: Ticks) -> Ticks {
        let signature = self.pattern_meter().signature_at(first);
        let length = (end - first).max(Ticks(1));
        let unit = if length * 2 > signature.bar_length() {
            signature.bar_length()
        } else {
            signature.beat_length()
        };
        length.ceil_to(unit)
    }

    /// Moves the selected notes by whole grid steps (negative is earlier).
    pub fn nudge(&mut self, steps: i64) {
        let ids = self.selected_ids();
        if ids.is_empty() {
            return;
        }
        let first = self
            .selected_notes()
            .iter()
            .map(|n| n.start)
            .min()
            .unwrap_or(Ticks::ZERO);
        let step = self.grid.step(&self.pattern_meter(), first);
        let delta = (step * steps).max(-first);
        if delta == Ticks::ZERO {
            return;
        }
        let (track, pattern) = (self.track, self.pattern);
        let end = self
            .selected_notes()
            .iter()
            .map(Note::end)
            .max()
            .unwrap_or(Ticks::ZERO)
            + delta;
        self.doc
            .edit_merged(text::EDIT_MOVE_NOTES, MERGE_NUDGE, |p| {
                p.update_notes(pattern, &ids, |n| n.start += delta);
                grow_to_fit(p, track, pattern, end);
            });
    }

    /// Moves the selected notes up or down by semitones, unless that would
    /// push one outside the keyboard.
    pub fn transpose(&mut self, semitones: i32) {
        let notes = self.selected_notes();
        let fits = notes.iter().all(|n| {
            let key = i32::from(n.key) + semitones;
            (0..=i32::from(MAX_KEY)).contains(&key)
        });
        if notes.is_empty() || !fits {
            return;
        }
        let ids: Vec<NoteId> = notes.iter().map(|n| n.id).collect();
        let pattern = self.pattern;
        self.doc
            .edit_merged(text::EDIT_TRANSPOSE, MERGE_TRANSPOSE, |p| {
                p.update_notes(pattern, &ids, |n| {
                    n.key = u8::try_from(i32::from(n.key) + semitones).unwrap_or(n.key);
                });
            });
    }

    // ----- Playing notes live

    /// The note the computer keyboard's Z key plays: the kick on drum
    /// tracks.
    pub fn keyboard_base(&self) -> u8 {
        if self.is_drum_track() {
            self.drum_base
        } else {
            self.keyboard_base
        }
    }

    /// How hard computer-keyboard notes play.
    pub fn keyboard_velocity(&self) -> f32 {
        self.keyboard_velocity
    }

    /// Changes how hard computer-keyboard notes play.
    pub fn set_keyboard_velocity(&mut self, velocity: f32) {
        self.keyboard_velocity = velocity.clamp(0.05, 1.0);
    }

    /// Moves the computer keyboard up or down by octaves.
    pub fn shift_keyboard_octave(&mut self, octaves: i32) {
        let current = self.keyboard_base();
        let base = i32::from(current) + 12 * octaves;
        let highest = i32::from(clamp_base(127));
        let base = u8::try_from(base.clamp(0, highest)).unwrap_or(current);
        if self.is_drum_track() {
            self.drum_base = base;
        } else {
            self.keyboard_base = base;
        }
    }

    /// A computer key went down. Returns whether it plays a note.
    pub fn key_down(&mut self, physical: Key) -> bool {
        let Some(offset) = note_offset(physical) else {
            return false;
        };
        if self.held_keys.contains_key(&physical) {
            return true; // Auto-repeat.
        }
        let key = self.keyboard_base().saturating_add(offset).min(MAX_KEY);
        self.held_keys.insert(physical, key);
        self.engine.note_on(self.track, key, self.keyboard_velocity);
        true
    }

    /// A computer key came up.
    pub fn key_up(&mut self, physical: Key) {
        if let Some(key) = self.held_keys.remove(&physical)
            && !self.held_keys.values().any(|&k| k == key)
        {
            self.engine.note_off(self.track, key);
        }
    }

    /// Releases every held computer key (when the window loses focus, the
    /// key-up events would never arrive).
    pub fn release_keys(&mut self) {
        let keys: Vec<Key> = self.held_keys.keys().copied().collect();
        for key in keys {
            self.key_up(key);
        }
        self.pointer_off();
    }

    /// Plays a note while the mouse holds it (in the editor or on the
    /// on-screen keyboard); a different note replaces it.
    pub fn pointer_on(&mut self, key: u8, velocity: f32) {
        if self.pointer_note == Some(key) {
            return;
        }
        self.pointer_off();
        self.pointer_note = Some(key);
        self.engine.note_on(self.track, key, velocity);
    }

    /// Releases the note (or chord) the mouse was playing.
    pub fn pointer_off(&mut self) {
        if let Some(key) = self.pointer_note.take() {
            self.engine.note_off(self.track, key);
        }
        for key in std::mem::take(&mut self.chord_held) {
            self.engine.note_off(self.track, key);
        }
    }

    /// The note the mouse is playing.
    pub fn pointer_note(&self) -> Option<u8> {
        self.pointer_note
    }

    /// Shows a note played on a MIDI keyboard (it is heard through the
    /// engine directly; this only lights it up).
    pub fn show_outside_note(&mut self, key: u8, down: bool) {
        self.outside_notes[usize::from(key & 0x7F)] = down;
    }

    /// Whether a note is sounding because of a key held by the person
    /// (computer keyboard, mouse or MIDI keyboard).
    pub fn is_key_lit(&self, key: u8) -> bool {
        self.key_is_down(key)
    }

    /// The computer key that plays a note, if any, at the current octave.
    pub fn computer_key_for(&self, key: u8) -> Option<u8> {
        let offset = key.checked_sub(self.keyboard_base())?;
        (offset <= HIGHEST_OFFSET).then_some(offset)
    }

    // ----- Files

    /// Saves to the file the song came from. Returns `false` if it has none
    /// yet (the caller asks where to save).
    pub fn save(&mut self) -> Result<bool, String> {
        let Some(path) = self.path.clone() else {
            return Ok(false);
        };
        self.save_as(&path).map(|()| true)
    }

    /// Saves to a new file, which becomes the song's file.
    pub fn save_as(&mut self, path: &Path) -> Result<(), String> {
        let path = with_extension(path);
        self.capture_plugin_states();
        match daw_formats::save_project(self.project(), &path, Some(self.audio.folder())) {
            Ok(()) => {
                self.doc.mark_saved();
                self.set_status(text::saved(&path.display().to_string()));
                self.path = Some(path);
                Ok(())
            }
            Err(error) => {
                let message = text::could_not_save(&error.to_string());
                self.set_status(message.clone());
                Err(message)
            }
        }
    }

    /// Replaces the song with another project (opened, new or recovered),
    /// showing the song.
    pub fn replace(&mut self, project: Project, path: Option<PathBuf>, saved: bool) {
        self.release_keys();
        self.engine.stop();
        self.engine.panic();
        // The new song's plugins start with their own settings.
        self.engine.unload_plugins();
        self.last_moved = None;
        let project = editable(project);
        let (track, pattern) = first_focus(&project);
        self.doc = if saved {
            Document::new(project)
        } else {
            Document::new_unsaved(project)
        };
        self.engine.invalidate();
        self.path = path;
        self.view = View::Song;
        self.track = track;
        self.pattern = pattern;
        self.current.clear();
        self.selected_clip = None;
        self.scope = None;
        self.selection.clear();
        self.paste_at = None;
        self.take = None;
        self.recording_held.clear();
        self.audio_focus = None;
        self.audio_take = None;
        self.finishing.clear();
        self.synth_windows.clear();
        // The old song's sound goes; the new song's is read.
        let wanted: HashSet<String> = self
            .project()
            .audio
            .iter()
            .map(|f| f.file.clone())
            .collect();
        self.engine.retain_audio(|name| wanted.contains(name));
        self.audio.retain(|name| wanted.contains(name));
        // And its instruments' (decisions 0024 and 0027).
        let instruments: HashSet<String> = self
            .project()
            .tracks
            .iter()
            .filter_map(|t| {
                t.instrument
                    .sfz_file()
                    .or_else(|| t.instrument.voicebank())
                    .map(str::to_owned)
            })
            .collect();
        self.engine.retain_sounds(|file| instruments.contains(file));
        self.load_song_audio();
        self.awaiting_stop = true;
        self.sync_engine();
        self.engine.locate(Ticks::ZERO);
    }

    /// Opens a song from a file.
    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        match daw_formats::load_project(path, Some(self.audio.folder())) {
            Ok(project) => {
                self.replace(project, Some(path.to_path_buf()), true);
                self.set_status(text::opened(&path.display().to_string()));
                Ok(())
            }
            Err(error) => {
                let message = text::could_not_open(&error.to_string());
                self.set_status(message.clone());
                Err(message)
            }
        }
    }

    /// Starts a new song, shown as the whole song.
    pub fn new_song(&mut self) {
        self.replace(starter_project(), None, true);
    }

    // ----- Export

    /// Starts writing the song to a sound file (WAV, or FLAC with
    /// `target`) in the background.
    pub fn start_export(&mut self, path: &Path, format: ExportFormat) {
        self.start_sound_export(path, format, ExportTarget::Wav);
    }

    /// Starts writing the song to a WAV or FLAC file in the background.
    pub fn start_sound_export(&mut self, path: &Path, format: ExportFormat, target: ExportTarget) {
        if self.export.is_some() || !target.is_sound() {
            return;
        }
        let path = target.with_extension(path);
        self.capture_plugin_states();
        self.export = Some(Export::start(
            self.project().clone(),
            self.engine.library().clone(),
            self.engine.config(),
            path,
            format,
            target,
        ));
    }

    /// How far the export has come, 0 to 1, while one runs.
    pub fn export_progress(&self) -> Option<f32> {
        self.export.as_ref().map(Export::progress)
    }

    /// Stops the export (nothing is written).
    pub fn cancel_export(&mut self) {
        if let Some(export) = &self.export {
            export.cancel();
        }
    }

    /// Waits for the export to end (for tests and for quitting).
    pub fn finish_export(&mut self) {
        if let Some(export) = &mut self.export {
            export.wait();
        }
        self.poll_export();
    }

    fn poll_export(&mut self) {
        let Some(result) = self.export.as_mut().and_then(Export::result) else {
            return;
        };
        self.export = None;
        match result {
            Ok(Some(path)) => self.set_status(text::exported(&path.display().to_string())),
            Ok(None) => self.set_status(text::EXPORT_CANCELLED),
            Err(error) => self.set_status(text::could_not_export(&error)),
        }
    }
}

/// The song a new user starts with: drums on top (a one-bar beat repeated
/// over four bars), a piano with an empty four-bar pattern, a limiter on
/// the master. It plays through once; patterns loop on their own.
fn starter_project() -> Project {
    let mut project = Project::starter(text::FIRST_PATTERN, text::FIRST_TRACK);
    project.add_drums_with_pattern(
        text::DRUMS_TRACK,
        text::FIRST_BEAT,
        daw_model::ACOUSTIC_KIT,
        1,
        4,
    );
    project.loop_region = None;
    project
}

/// The track and pattern in focus when a project opens: the first track's
/// first pattern.
fn first_focus(project: &Project) -> (TrackId, PatternId) {
    project
        .first_pattern()
        .or_else(|| {
            project.tracks.iter().find_map(|t| {
                project
                    .track_patterns(t.id)
                    .next()
                    .map(|pattern| (t.id, pattern.id))
            })
        })
        .unwrap_or((TrackId(0), PatternId(0)))
}

/// Adds the project's file ending if the path has none.
pub fn with_extension(path: &Path) -> PathBuf {
    let wanted = daw_ident::PROJECT_FILE_EXTENSION;
    let has_it = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(wanted));
    if has_it {
        path.to_path_buf()
    } else {
        let mut name = path.as_os_str().to_owned();
        name.push(".");
        name.push(wanted);
        PathBuf::from(name)
    }
}

/// Makes sure a project has an instrument track with a pattern to edit.
fn editable(mut project: Project) -> Project {
    if project.first_pattern().is_some() {
        return project;
    }
    let length = project.meter.bar_start(4);
    let track = match project.tracks.iter().find(|t| !t.instrument.is_audio()) {
        Some(track) => track.id,
        None => project.add_track(text::FIRST_TRACK, daw_model::Instrument::builtin_piano()),
    };
    let pattern = project.add_pattern_for(track, text::FIRST_PATTERN, length);
    project.add_clip(track, Ticks::ZERO, length, pattern);
    if project.loop_region.is_none() {
        project.loop_region = Region::new(Ticks::ZERO, length);
    }
    project
}

/// Where a pattern sits on a track: its first clip.
fn placement(project: &Project, track: TrackId, pattern: PatternId) -> Option<Placement> {
    project
        .track(track)?
        .clips
        .iter()
        .find_map(|clip| match clip.content {
            ClipContent::Pattern { pattern: p, offset } if p == pattern => Some(Placement {
                start: clip.start,
                length: clip.length,
                offset,
            }),
            ClipContent::Pattern { .. } | ClipContent::Audio { .. } => None,
        })
}

/// Sets a pattern's length (and its clip's, and the loop's if it covered
/// the clip) to `bars` whole bars, or more if its notes need the room.
fn set_length_in_bars(project: &mut Project, track: TrackId, pattern: PatternId, bars: u32) {
    let Some(place) = placement(project, track, pattern) else {
        return;
    };
    let meter = &project.meter;
    let first_bar = meter.position(place.start).bar;
    let wanted = meter.bar_start(first_bar + bars) - place.start;
    let notes_end = project
        .pattern(pattern)
        .and_then(|p| p.notes.iter().map(Note::end).max())
        .unwrap_or(Ticks::ZERO)
        - place.offset;
    let length = wanted.max(round_up_to_bar(meter, place.start, notes_end));
    set_length(project, track, pattern, place, length);
}

/// Lengthens a pattern (to whole bars) if a note now ends after it.
fn grow_to_fit(project: &mut Project, track: TrackId, pattern: PatternId, end: Ticks) {
    let Some(place) = placement(project, track, pattern) else {
        return;
    };
    let current = project.pattern(pattern).map_or(Ticks::ZERO, |p| p.length);
    if end <= current {
        return;
    }
    let length = round_up_to_bar(&project.meter, place.start, end - place.offset);
    set_length(project, track, pattern, place, length);
}

fn round_up_to_bar(meter: &MeterMap, clip_start: Ticks, length: Ticks) -> Ticks {
    let end = meter.position(clip_start + length);
    let bar = if end.beat == 0 && end.ticks == Ticks::ZERO {
        end.bar
    } else {
        end.bar + 1
    };
    meter.bar_start(bar) - clip_start
}

/// Changes a pattern's length (`length` counted from the clip's offset).
/// Clips that played it exactly once follow; clips that repeat it keep
/// their length.
fn set_length(
    project: &mut Project,
    track: TrackId,
    pattern: PatternId,
    place: Placement,
    length: Ticks,
) {
    let old_end = place.start + place.length;
    let old_length = project.pattern(pattern).map_or(Ticks::ZERO, |p| p.length);
    if let Some(p) = project.pattern_mut(pattern) {
        p.length = length + place.offset;
    }
    if let Some(t) = project.track_mut(track) {
        for clip in &mut t.clips {
            if let ClipContent::Pattern { pattern: p, offset } = clip.content
                && p == pattern
                && clip.length == old_length - offset
            {
                clip.length = length + place.offset - offset;
            }
        }
    }
    let followed = place.length == old_length - place.offset;
    if followed
        && let Some(region) = project.loop_region
        && region.start == place.start
        && region.end == old_end
    {
        project.loop_region = Region::new(place.start, place.start + length);
    }
}
