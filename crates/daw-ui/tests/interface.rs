// SPDX-License-Identifier: GPL-3.0-or-later

//! The interface itself, driven with simulated mouse and keyboard input and
//! inspected through its accessibility tree, with no window.

#![allow(
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::assertions_on_result_states,
    clippy::assert_is_empty,
    reason = "test code: helpers unwrap and convert freely"
)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use daw_dsp::drums::PadSound;
use daw_engine::{
    Engine, EngineConfig, EngineProcessor, PluginFileFormat, PluginInfo, PluginKind, PluginSystem,
    PluginTarget, ScanProblem, SoundLibrary,
};
use daw_model::{AutomationPoint, AutomationTarget, ChainPlace, EffectKind, Ticks};
use daw_ui::{
    BridgeState, Host, KeyLabels, LayoutFamily, PluginJobDone, PluginPlaces, PluginSearch, Request,
    Session, Settings, View, Workspace, text,
};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// Answers file dialogs with prepared paths.
#[derive(Clone, Default)]
struct FakeHost {
    save_to: Arc<Mutex<Option<PathBuf>>>,
    export_to: Arc<Mutex<Option<PathBuf>>>,
    audio: Arc<Mutex<Vec<PathBuf>>>,
    asked: Arc<Mutex<u32>>,
    plugin_files: Arc<Mutex<Vec<PathBuf>>>,
    folder: Arc<Mutex<Option<PathBuf>>>,
    shown_folders: Arc<Mutex<Vec<PathBuf>>>,
    soundfont: Arc<Mutex<Option<PathBuf>>>,
    kit_sounds: Arc<Mutex<Vec<PathBuf>>>,
}

impl Host for FakeHost {
    fn choose_file_to_open(&mut self) -> Option<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        None
    }

    fn choose_file_to_save(&mut self, _suggested_name: &str) -> Option<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        self.save_to.lock().unwrap().clone()
    }

    fn choose_export_file(
        &mut self,
        _suggested_name: &str,
        _target: daw_ui::ExportTarget,
    ) -> Option<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        self.export_to.lock().unwrap().clone()
    }

    fn choose_audio_files(&mut self) -> Vec<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        self.audio.lock().unwrap().clone()
    }

    fn choose_plugin_files(&mut self) -> Vec<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        self.plugin_files.lock().unwrap().clone()
    }

    fn choose_soundfont(&mut self) -> Option<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        self.soundfont.lock().unwrap().clone()
    }

    fn choose_kit_sounds(&mut self) -> Vec<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        self.kit_sounds.lock().unwrap().clone()
    }

    fn choose_folder(&mut self, _title: &str) -> Option<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        self.folder.lock().unwrap().clone()
    }

    fn open_folder(&mut self, folder: &std::path::Path) {
        self.shown_folders
            .lock()
            .unwrap()
            .push(folder.to_path_buf());
    }
}

struct Fixture {
    harness: Harness<'static, Option<Workspace>>,
    processor: EngineProcessor,
    host: FakeHost,
}

/// A new song, open on its piano pattern (the phase 1 tests work there).
fn fixture() -> Fixture {
    open_first_track(new_song(), |drums| !drums)
}

/// A new song, open on its empty beat (the step grid tests work there).
fn beat() -> Fixture {
    open_first_track(new_song(), |drums| drums)
}

/// Opens the first track whose instrument's drumness `wanted` accepts.
fn open_first_track(mut f: Fixture, wanted: impl Fn(bool) -> bool) -> Fixture {
    let session = f.harness.state_mut().as_mut().unwrap().session_mut();
    let track = session
        .project()
        .tracks
        .iter()
        .find(|t| wanted(t.instrument.is_drums()))
        .unwrap()
        .id;
    session.open_track(track);
    f.harness.run_steps(3);
    f
}

/// A new song as a new user sees it: the whole song.
fn new_song() -> Fixture {
    let mut harness = Harness::builder()
        .with_size(vec2(1280.0, 800.0))
        .build_ui_state(
            |ui, workspace: &mut Option<Workspace>| {
                if let Some(workspace) = workspace {
                    workspace.ui(ui);
                }
            },
            None,
        );
    // As the program does: SFZ instruments read in the background.
    let mut library = SoundLibrary::new();
    library.set_sfz_reader(|path| daw_formats::sfz::load(path).map_err(|e| e.to_string()));
    library.set_background_loading(true);
    let (engine, processor) = Engine::new(EngineConfig::default(), library);
    let host = FakeHost::default();
    let workspace = Workspace::new(
        &harness.ctx,
        Session::starter(engine),
        Settings::default(),
        Box::new(host.clone()),
        KeyLabels::new(LayoutFamily::Qwerty),
    );
    *harness.state_mut() = Some(workspace);
    harness.run_steps(3);
    Fixture {
        harness,
        processor,
        host,
    }
}

impl Fixture {
    fn workspace(&self) -> &Workspace {
        self.harness.state().as_ref().unwrap()
    }

    fn session(&self) -> &Session {
        self.workspace().session()
    }

    fn workspace_mut(&mut self) -> &mut Workspace {
        self.harness.state_mut().as_mut().unwrap()
    }

    /// Lets the audio thread run for a moment, then draws a few frames.
    fn tick(&mut self) {
        let mut buffer = vec![0.0_f32; 512];
        for _ in 0..4 {
            self.processor.process(&mut buffer, 2);
        }
        self.harness.run_steps(2);
    }

    fn roll(&self) -> Rect {
        self.harness.get_by_label(text::PIANO_ROLL).rect()
    }

    /// Presses and releases the left button at a point.
    fn click(&mut self, pos: Pos2, modifiers: Modifiers) {
        self.harness.hover_at(pos);
        self.harness.step();
        self.press(pos, PointerButton::Primary, true, modifiers);
        self.harness.step();
        self.press(pos, PointerButton::Primary, false, modifiers);
        self.harness.run_steps(2);
    }

    fn press(&mut self, pos: Pos2, button: PointerButton, pressed: bool, modifiers: Modifiers) {
        self.harness.event(Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        });
    }

    fn key(&mut self, key: Key, physical: Key, pressed: bool, modifiers: Modifiers) {
        self.harness.event(Event::Key {
            key,
            physical_key: Some(physical),
            pressed,
            repeat: false,
            modifiers,
        });
        self.harness.run_steps(2);
    }

    fn notes(&self) -> Vec<daw_model::Note> {
        self.session().pattern().notes.clone()
    }
}

#[test]
fn the_window_starts_with_an_empty_pattern_and_its_controls() {
    let f = fixture();
    for label in [
        text::PLAY,
        text::STOP,
        text::RECORD,
        text::LOOP,
        text::METRONOME,
        text::MENU,
        text::PIANO_ROLL,
        text::ON_SCREEN_KEYBOARD,
    ] {
        f.harness.get_by_label(label);
    }
    assert!(f.notes().is_empty());
    assert!(f.session().is_looping(), "a new song loops its pattern");
}

#[test]
fn clicking_the_grid_places_a_note_that_plays_and_can_be_undone() {
    let mut f = fixture();
    let roll = f.roll();
    let spot = roll.center();
    f.harness.hover_at(spot);
    f.harness.step();
    f.press(spot, PointerButton::Primary, true, Modifiers::NONE);
    f.harness.step();
    let sounding = f.session().pointer_note();
    assert!(
        sounding.is_some(),
        "the note plays while the button is held"
    );
    f.press(spot, PointerButton::Primary, false, Modifiers::NONE);
    f.harness.run_steps(2);
    assert_eq!(
        f.session().pointer_note(),
        None,
        "and stops when it is released"
    );

    let notes = f.notes();
    assert_eq!(notes.len(), 1, "one note was placed");
    assert_eq!(Some(notes[0].key), sounding);
    assert_eq!(
        notes[0].start.0 % Ticks::note_value(1, 16).0,
        0,
        "on the grid"
    );
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_ADD_NOTE)
    );

    f.key(Key::Z, Key::Z, true, Modifiers::COMMAND);
    f.key(Key::Z, Key::Z, false, Modifiers::COMMAND);
    assert!(f.notes().is_empty(), "Ctrl+Z took it away");
    assert_eq!(f.session().pointer_note(), None);
}

#[test]
fn dragging_from_an_empty_spot_stretches_the_new_note() {
    let mut f = fixture();
    let roll = f.roll();
    let from = roll.center() - vec2(200.0, 0.0);
    let to = from + vec2(160.0, 0.0);
    f.harness.hover_at(from);
    f.harness.step();
    f.press(from, PointerButton::Primary, true, Modifiers::NONE);
    f.harness.step();
    for i in 1..=8 {
        #[allow(clippy::cast_precision_loss, reason = "a small count")]
        f.harness.hover_at(from + vec2(20.0 * i as f32, 0.0));
        f.harness.step();
    }
    f.press(to, PointerButton::Primary, false, Modifiers::NONE);
    f.harness.run_steps(2);
    let notes = f.notes();
    assert_eq!(notes.len(), 1);
    assert!(
        notes[0].length > Ticks::QUARTER,
        "stretched: {:?}",
        notes[0].length
    );
    assert_eq!(
        f.session().document().undo_depth(),
        1,
        "placing and stretching is one undo step"
    );
}

#[test]
fn notes_move_by_dragging_and_go_away_with_a_right_click() {
    let mut f = fixture();
    let roll = f.roll();
    let spot = roll.center();
    f.click(spot, Modifiers::NONE);
    let before = f.notes()[0];

    // Drag the note's middle up and to the right.
    let grab = spot + vec2(10.0, 0.0);
    f.harness.hover_at(grab);
    f.harness.step();
    f.press(grab, PointerButton::Primary, true, Modifiers::NONE);
    f.harness.step();
    for i in 1..=6 {
        #[allow(clippy::cast_precision_loss, reason = "a small count")]
        f.harness
            .hover_at(grab + vec2(20.0 * i as f32, -8.0 * i as f32));
        f.harness.step();
    }
    let end = grab + vec2(120.0, -48.0);
    f.press(end, PointerButton::Primary, false, Modifiers::NONE);
    f.harness.run_steps(2);
    let after = f.notes()[0];
    assert!(
        after.start > before.start,
        "moved later: {:?} -> {:?}",
        before.start,
        after.start
    );
    assert!(
        after.key > before.key,
        "moved up: {} -> {}",
        before.key,
        after.key
    );
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_MOVE_NOTES)
    );

    // Right-click it away (it is now under the point where the drag ended).
    let note_spot = end;
    f.harness.hover_at(note_spot);
    f.harness.step();
    f.press(note_spot, PointerButton::Secondary, true, Modifiers::NONE);
    f.harness.step();
    f.press(note_spot, PointerButton::Secondary, false, Modifiers::NONE);
    f.harness.run_steps(2);
    assert!(f.notes().is_empty(), "right-click deletes");
}

#[test]
fn the_computer_keyboard_plays_and_space_plays_the_song() {
    let mut f = fixture();
    let base = f.session().keyboard_base();
    f.key(Key::Z, Key::Z, true, Modifiers::NONE);
    assert!(f.session().is_key_lit(base), "Z plays the lowest note");
    f.key(Key::Z, Key::Z, false, Modifiers::NONE);
    assert!(!f.session().is_key_lit(base));

    // On an AZERTY keyboard the key in Q's place types A: it still plays
    // by position.
    f.key(Key::A, Key::Q, true, Modifiers::NONE);
    assert!(f.session().is_key_lit(base + 12));
    f.key(Key::A, Key::Q, false, Modifiers::NONE);

    f.key(Key::Space, Key::Space, true, Modifiers::NONE);
    f.key(Key::Space, Key::Space, false, Modifiers::NONE);
    f.tick();
    assert!(f.session().is_playing(), "Space starts playback");
    assert!(f.session().engine().is_playing(), "the engine is playing");
    f.key(Key::Space, Key::Space, true, Modifiers::NONE);
    f.key(Key::Space, Key::Space, false, Modifiers::NONE);
    f.tick();
    assert!(!f.session().is_playing(), "and stops it");
}

#[test]
fn buttons_run_their_commands() {
    let mut f = fixture();
    f.harness.get_by_label(text::PLAY).click();
    f.harness.run_steps(2);
    f.tick();
    assert!(f.session().engine().is_playing());
    f.harness.get_by_label(text::STOP).click();
    f.harness.run_steps(2);
    f.tick();
    assert!(!f.session().engine().is_playing());

    f.harness.get_by_label(text::LOOP).click();
    f.harness.run_steps(2);
    assert!(
        !f.session().is_looping(),
        "the loop button switches looping off"
    );
    f.harness.get_by_label(text::METRONOME).click();
    f.harness.run_steps(2);
    assert!(f.session().metronome());

    let base = f.session().keyboard_base();
    f.harness.get_by_label(text::OCTAVE_UP).click();
    f.harness.run_steps(2);
    assert_eq!(f.session().keyboard_base(), base + 12);
}

#[test]
fn shortcuts_never_play_notes_and_notes_never_run_shortcuts() {
    let mut f = fixture();
    let roll = f.roll();
    f.click(roll.center(), Modifiers::NONE);
    f.click(roll.center() + vec2(120.0, 30.0), Modifiers::NONE);
    assert_eq!(f.notes().len(), 2);

    // Ctrl+A selects both; Ctrl+D duplicates them; no note sounds.
    f.key(Key::A, Key::A, true, Modifiers::COMMAND);
    f.key(Key::A, Key::A, false, Modifiers::COMMAND);
    assert_eq!(f.session().selection().len(), 2);
    f.key(Key::D, Key::D, true, Modifiers::COMMAND);
    f.key(Key::D, Key::D, false, Modifiers::COMMAND);
    assert_eq!(f.notes().len(), 4, "Ctrl+D duplicated the selection");
    let base = f.session().keyboard_base();
    assert!(
        !(0..32).any(|o| f.session().is_key_lit(base + o)),
        "no note played"
    );

    // Delete removes the selected copies.
    f.key(Key::Delete, Key::Delete, true, Modifiers::NONE);
    f.key(Key::Delete, Key::Delete, false, Modifiers::NONE);
    assert_eq!(f.notes().len(), 2);
}

#[test]
fn copy_and_paste_go_through_the_clipboard_text() {
    let mut f = fixture();
    let roll = f.roll();
    f.click(roll.center(), Modifiers::NONE);
    f.harness.event(Event::Copy);
    f.harness.step();
    let copied = f
        .harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .expect("copying puts text on the clipboard");
    assert!(copied.starts_with(daw_ident::PRODUCT_NAME));
    f.harness.event(Event::Paste(copied));
    f.harness.run_steps(2);
    assert_eq!(f.notes().len(), 2, "pasting brings a copy");
    f.harness.event(Event::Paste("some other text".to_owned()));
    f.harness.run_steps(2);
    assert_eq!(
        f.notes().len(),
        3,
        "foreign text pastes the notes copied here"
    );
}

#[test]
fn saving_a_new_song_asks_where_and_the_title_follows() {
    let mut f = fixture();
    let roll = f.roll();
    f.click(roll.center(), Modifiers::NONE);
    let path = std::env::temp_dir().join(format!("interface-test-{}", std::process::id()));
    *f.host.save_to.lock().unwrap() = Some(path.clone());
    f.key(Key::S, Key::S, true, Modifiers::COMMAND);
    f.key(Key::S, Key::S, false, Modifiers::COMMAND);
    assert_eq!(
        *f.host.asked.lock().unwrap(),
        1,
        "a new song asks where to save"
    );
    assert!(!f.session().is_dirty());
    let saved = f.session().path().expect("saved").to_path_buf();
    assert!(
        f.workspace().settings().recent_files.contains(&saved),
        "remembered as recent"
    );
    std::fs::remove_file(saved).ok();
}

#[test]
fn new_song_with_unsaved_changes_asks_first() {
    let mut f = fixture();
    let roll = f.roll();
    f.click(roll.center(), Modifiers::NONE);
    f.key(Key::N, Key::N, true, Modifiers::COMMAND);
    f.key(Key::N, Key::N, false, Modifiers::COMMAND);
    f.harness.get_by_label(text::UNSAVED_QUESTION);
    assert_eq!(f.notes().len(), 1, "nothing is lost before answering");
    f.harness.get_by_label(text::DISCARD_CHANGES).click();
    f.harness.run_steps(3);
    assert!(f.notes().is_empty(), "a fresh song after discarding");
    assert!(f.harness.query_by_label(text::UNSAVED_QUESTION).is_none());
}

#[test]
fn the_menu_switches_theme() {
    let mut f = fixture();
    f.harness.get_by_label(text::MENU).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::THEME).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::THEME_LIGHT).click();
    f.harness.run_steps(3);
    assert_eq!(f.harness.ctx.theme(), egui::Theme::Light);
    assert_eq!(f.workspace().settings().theme, daw_ui::ThemeChoice::Light);
}

#[test]
fn shortcuts_can_be_changed_but_never_onto_note_keys() {
    let mut f = fixture();
    f.harness.get_by_label(text::MENU).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::SHORTCUTS).click();
    // Let the dialog settle into place: it re-centres for a few frames
    // while its size is worked out.
    f.harness.run_steps(6);
    // The rows follow the order of `Command::ALL`; Loop is the fourth.
    let change_buttons: Vec<_> = f.harness.get_all_by_label(text::CHANGE).collect();
    change_buttons[3].click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::PRESS_A_KEY);

    f.key(Key::P, Key::P, true, Modifiers::NONE);
    f.key(Key::P, Key::P, false, Modifiers::NONE);
    f.harness.get_by_label(&text::shortcut_plays_notes("P"));
    f.key(Key::Z, Key::Z, true, Modifiers::COMMAND);
    f.key(Key::Z, Key::Z, false, Modifiers::COMMAND);
    f.harness.get_by_label(&text::shortcut_in_use(
        "Ctrl+Z",
        text::command_name(daw_ui::Command::Undo),
    ));
    f.key(Key::F4, Key::F4, true, Modifiers::NONE);
    f.key(Key::F4, Key::F4, false, Modifiers::NONE);
    assert_eq!(f.workspace().keymap().text(daw_ui::Command::Loop), "F4");

    f.harness.get_by_label(text::CLOSE).click();
    f.harness.run_steps(2);
    assert!(f.session().is_looping());
    f.key(Key::F4, Key::F4, true, Modifiers::NONE);
    f.key(Key::F4, Key::F4, false, Modifiers::NONE);
    assert!(!f.session().is_looping(), "the new shortcut works");
    assert!(
        f.workspace().settings().shortcuts.contains_key("loop"),
        "and is remembered"
    );
}

#[test]
fn the_option_popups_and_about_box_open() {
    let mut f = fixture();
    f.harness.get_by_label(text::RECORDING_OPTIONS).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::QUANTIZE).click();
    f.harness.run_steps(2);
    assert!(f.session().snap_recording());
    f.harness.key_press(Key::Escape);
    f.harness.run_steps(2);
    f.harness.get_by_label(text::MENU).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::ABOUT).click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::CLOSE).click();
    f.harness.run_steps(2);
    assert!(f.harness.query_by_label(text::CLOSE).is_none());
}

/// Asks to close the window, as the window's close button does.
fn request_close(f: &mut Fixture) {
    f.harness
        .input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    f.harness.step();
}

fn sent_close_commands(f: &Fixture) -> (bool, bool) {
    let commands = f
        .harness
        .output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map_or_else(Vec::new, |v| v.commands.clone());
    (
        commands.contains(&egui::ViewportCommand::CancelClose),
        commands.contains(&egui::ViewportCommand::Close),
    )
}

#[test]
fn closing_with_unsaved_changes_asks_and_never_just_quits() {
    let mut f = fixture();
    // Nothing to save: closing is allowed straight away.
    request_close(&mut f);
    assert_eq!(
        sent_close_commands(&f),
        (false, false),
        "no reason to stop a clean close"
    );

    let roll = f.roll();
    f.click(roll.center(), Modifiers::NONE);
    request_close(&mut f);
    assert!(sent_close_commands(&f).0, "the close is held back");
    f.harness.run_steps(3);
    f.harness.get_by_label(text::UNSAVED_QUESTION);

    // Cancel keeps everything.
    f.harness.get_by_label(text::CANCEL).click();
    f.harness.run_steps(2);
    assert_eq!(f.notes().len(), 1);
    assert!(f.harness.query_by_label(text::UNSAVED_QUESTION).is_none());

    // Discard closes for real.
    request_close(&mut f);
    f.harness.run_steps(3);
    f.harness.get_by_label(text::DISCARD_CHANGES).click();
    f.harness.step();
    assert!(sent_close_commands(&f).1, "discarding closes the window");
}

// ----- Phase 2: the step grid, the song view, the mixer, export

/// The middle of a square of the step grid.
fn square(f: &Fixture, sound: PadSound, step: usize) -> Pos2 {
    f.harness
        .get_by_label(&text::step_cell(text::pad_name(sound), step))
        .rect()
        .center()
}

#[test]
fn a_new_song_opens_on_the_whole_song() {
    let f = new_song();
    assert_eq!(f.session().view(), View::Song);
    f.harness.get_by_label(text::SONG_VIEW);
    assert!(f.harness.query_by_label(text::STEP_GRID).is_none());
}

#[test]
fn the_beat_opens_on_the_step_grid_and_squares_make_a_beat() {
    let mut f = beat();
    f.harness.get_by_label(text::STEP_GRID);
    assert!(f.session().is_drum_track());
    assert!(f.notes().is_empty());

    let kick = square(&f, PadSound::Kick, 1);
    f.harness.hover_at(kick);
    f.harness.step();
    f.press(kick, PointerButton::Primary, true, Modifiers::NONE);
    f.harness.step();
    assert_eq!(
        f.session().pointer_note(),
        Some(36),
        "the kick plays while the button is held"
    );
    f.press(kick, PointerButton::Primary, false, Modifiers::NONE);
    f.harness.run_steps(2);
    assert_eq!(f.session().pointer_note(), None);
    let notes = f.notes();
    assert_eq!(notes.len(), 1);
    assert_eq!((notes[0].key, notes[0].start), (36, Ticks::ZERO));

    f.click(kick, Modifiers::NONE);
    assert!(f.notes().is_empty(), "a second click removes the hit");

    // Dragging along the hi-hat row adds a hit on every square passed.
    let first = square(&f, PadSound::ClosedHat, 1);
    let last = square(&f, PadSound::ClosedHat, 8);
    f.harness.hover_at(first);
    f.harness.step();
    f.press(first, PointerButton::Primary, true, Modifiers::NONE);
    f.harness.step();
    for i in 1..=10 {
        let t = i as f32 / 10.0;
        f.harness.hover_at(first + (last - first) * t);
        f.harness.step();
    }
    f.press(last, PointerButton::Primary, false, Modifiers::NONE);
    f.harness.run_steps(2);
    let notes = f.notes();
    assert_eq!(notes.len(), 8, "eight hits");
    assert!(notes.iter().all(|n| n.key == 42), "all closed hi-hats");
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_PAINT_HITS)
    );
    f.key(Key::Z, Key::Z, true, Modifiers::COMMAND);
    f.key(Key::Z, Key::Z, false, Modifiers::COMMAND);
    assert!(f.notes().is_empty(), "one undo takes the whole drag away");
}

#[test]
fn a_sound_name_plays_it_and_opens_its_settings() {
    let mut f = beat();
    f.harness
        .get_by_label(text::pad_name(PadSound::Snare))
        .click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::PAD_MUTED).click();
    f.harness.run_steps(2);
    let drums = f.session().track_id();
    assert!(f.session().pad(drums, 38).muted, "the snare is silenced");
    assert_eq!(f.session().document().undo_label(), Some(text::EDIT_PAD));
}

#[test]
fn the_location_line_and_the_song_view_lead_everywhere() {
    let mut f = new_song();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    assert_eq!(
        f.session().view(),
        View::Song,
        "the song's name shows the song"
    );
    let song = f.harness.get_by_label(text::SONG_VIEW).rect();

    f.harness
        .get_by_label(&format!("+ {}", text::ADD_PIANO))
        .click();
    f.harness.run_steps(2);
    assert_eq!(f.session().project().tracks.len(), 3);

    // The drum clip's menu (top row, from bar 1) opens its beat. (A
    // double-click does the same; the test clock is too slow for one.)
    // Under the ruler and the folded signature row.
    let clip = Pos2::new(song.left() + 236.0 + 60.0, song.top() + 28.0 + 28.0 + 32.0);
    f.harness.hover_at(clip);
    f.harness.step();
    f.press(clip, PointerButton::Secondary, true, Modifiers::NONE);
    f.harness.step();
    f.press(clip, PointerButton::Secondary, false, Modifiers::NONE);
    f.harness.run_steps(2);
    f.harness.get_by_label(text::OPEN_PATTERN).click();
    f.harness.run_steps(2);
    assert_eq!(f.session().view(), View::Pattern);
    assert!(f.session().is_drum_track());

    // F5 goes back to the song; clicking an empty spot places a pattern.
    f.key(Key::F5, Key::F5, true, Modifiers::NONE);
    f.key(Key::F5, Key::F5, false, Modifiers::NONE);
    assert_eq!(f.session().view(), View::Song);
    let drums = f.session().project().tracks[0].id;
    let empty = Pos2::new(clip.x + 800.0, clip.y);
    f.click(empty, Modifiers::NONE);
    let clips = f.session().project().track(drums).unwrap().clips.len();
    assert_eq!(clips, 2, "the beat was placed again");
    f.key(Key::Delete, Key::Delete, true, Modifiers::NONE);
    f.key(Key::Delete, Key::Delete, false, Modifiers::NONE);
    let clips = f.session().project().track(drums).unwrap().clips.len();
    assert_eq!(clips, 1, "Delete removes the selected clip");
}

#[test]
fn the_mixer_adds_an_effect_that_opens_in_a_window() {
    let mut f = new_song();
    f.harness.get_by_label(text::SHOW_MIXER).click();
    f.harness.run_steps(3);
    let add = format!("+ {}", text::ADD_EFFECT);
    f.harness.get_all_by_label(&add).next().unwrap().click();
    f.harness.run_steps(3);
    f.harness.get_by_label("Reverb").click();
    f.harness.run_steps(4);
    let drums = f.session().project().tracks[0].id;
    let chain = f
        .session()
        .project()
        .chain(ChainPlace::Track(drums))
        .to_vec();
    assert_eq!(chain.len(), 1, "a reverb on the drums");
    assert!(matches!(chain[0].kind, EffectKind::Reverb(_)));
    let title = text::effect_title("Reverb", text::DRUMS_TRACK);
    f.harness.get_by_label(&title);
    f.harness.get_by_label(text::EFFECT_ON).click();
    f.harness.run_steps(2);
    let effect = f.session().project().effect(chain[0].id).unwrap();
    assert!(!effect.enabled, "the On switch bypasses it");
    f.harness.get_by_label(text::REMOVE_EFFECT).click();
    f.harness.run_steps(3);
    assert!(
        f.session()
            .project()
            .chain(ChainPlace::Track(drums))
            .is_empty()
    );
    assert!(
        f.harness.query_by_label(&title).is_none(),
        "its window closed"
    );
}

#[test]
fn exporting_asks_where_then_writes_the_song() {
    let mut f = new_song();
    let path = std::env::temp_dir().join(format!("interface-export-{}.wav", std::process::id()));
    *f.host.export_to.lock().unwrap() = Some(path.clone());
    f.key(Key::E, Key::E, true, Modifiers::COMMAND);
    f.key(Key::E, Key::E, false, Modifiers::COMMAND);
    f.harness.get_by_label(text::EXPORT_TITLE);
    f.harness.get_by_label(text::EXPORT_CD).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::EXPORT_BUTTON).click();
    f.harness.run_steps(2);
    assert_eq!(*f.host.asked.lock().unwrap(), 1, "asked where");
    f.workspace_mut().session_mut().finish_export();
    f.harness.run_steps(3);
    assert!(f.harness.query_by_label(text::EXPORTING).is_none());
    let bytes = std::fs::read(&path).expect("the file was written");
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
        44_100,
        "CD quality"
    );
    std::fs::remove_file(path).ok();
}

#[test]
fn tracks_and_patterns_are_renamed_in_a_dialog() {
    let mut f = beat();
    f.harness.get_by_label_contains(text::FIRST_BEAT).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::RENAME_PATTERN).click();
    f.harness.run_steps(4);
    f.harness.get_by_label(text::RENAME);
    f.harness.event(Event::Key {
        key: Key::A,
        physical_key: Some(Key::A),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    });
    f.harness.event(Event::Text("Groove".to_owned()));
    f.harness.run_steps(2);
    f.harness.get_by_label(text::OK).click();
    f.harness.run_steps(2);
    assert_eq!(f.session().pattern().name, "Groove");
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_RENAME_PATTERN)
    );
}

// ----- Phase 3: plugins and automation

/// Plugins as the search would report them (made up: they cannot load).
fn made_up_plugins() -> Vec<PluginInfo> {
    let plugin = |name: &str, kind| PluginInfo {
        format: PluginFileFormat::Clap,
        path: PathBuf::from(format!("/nowhere/{name}.clap")),
        id: format!("org.example.{}", name.to_lowercase().replace(' ', "-")),
        name: name.to_owned(),
        vendor: "Example Audio".to_owned(),
        version: "1.0".to_owned(),
        kind,
    };
    vec![
        plugin("Cloud Synth", PluginKind::Instrument),
        plugin("Space Echo", PluginKind::Effect),
    ]
}

fn finished_search() -> PluginSearch {
    PluginSearch {
        done: true,
        ..PluginSearch::default()
    }
}

#[test]
fn instruments_are_chosen_from_a_list_that_includes_plugins() {
    let mut f = new_song();
    let other = if PluginSystem::current() == PluginSystem::Windows {
        PluginSystem::Linux
    } else {
        PluginSystem::Windows
    };
    let search = PluginSearch {
        problems: vec![(
            PathBuf::from("/plugins/nyasynth.vst3"),
            ScanProblem::OtherSystem(other),
        )],
        ..finished_search()
    };
    f.workspace_mut()
        .session_mut()
        .set_plugins(made_up_plugins(), search);
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_INSTRUMENT))
        .click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::INSTRUMENTS_TITLE);
    f.harness.get_by_label_contains(text::ADD_PIANO_HINT);
    // A plugin file made for another system is named, with why.
    f.harness.get_by_label_contains("nyasynth.vst3 is made for");
    assert!(
        f.harness.query_by_label_contains("Space Echo").is_none(),
        "effects are not offered as instruments"
    );
    // Typing narrows the list.
    f.harness.event(Event::Text("cloud".to_owned()));
    f.harness.run_steps(2);
    assert!(
        f.harness
            .query_by_label_contains(text::ADD_PIANO_HINT)
            .is_none(),
        "the piano does not match"
    );
    f.harness.get_by_label_contains("Cloud Synth").click();
    f.harness.run_steps(3);
    assert!(
        f.harness.query_by_label(text::INSTRUMENTS_TITLE).is_none(),
        "the chooser closed"
    );
    let synth = f.session().project().tracks.last().unwrap().clone();
    assert_eq!(f.session().project().tracks.len(), 3);
    assert_eq!(synth.name, "Cloud Synth", "the track is named after it");
    assert!(synth.instrument.plugin().is_some());
    // It is not really there: the person is told.
    assert!(
        f.session()
            .is_plugin_missing(PluginTarget::Instrument(synth.id))
    );
    f.harness
        .get_by_label_contains("\"Cloud Synth\" could not be started");
}

#[test]
fn a_lane_added_from_the_track_header_is_drawn_on_with_the_mouse() {
    let mut f = new_song();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    let song = f.harness.get_by_label(text::SONG_VIEW).rect();
    let piano = f.session().project().tracks[1].id;
    // The piano's Automation button offers its settings; choose Volume.
    f.harness
        .get_all_by_label(text::AUTOMATION)
        .nth(1)
        .unwrap()
        .click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::VOLUME).click();
    f.harness.run_steps(3);
    let lane = f
        .session()
        .project()
        .lane(piano, AutomationTarget::Volume)
        .unwrap()
        .clone();
    assert_eq!(
        lane.points.len(),
        1,
        "it starts flat, at the present volume"
    );
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_ADD_LANE)
    );

    // A click low in the lane, a few bars in, adds a point there (Alt: not
    // moved to the nearest beat).
    let remove = f.harness.get_by_label(text::REMOVE_LANE).rect();
    let spot = Pos2::new(song.left() + 236.0 + 320.0, remove.center().y + 14.0);
    f.harness.event(Event::ModifiersChanged(Modifiers::ALT));
    f.click(spot, Modifiers::ALT);
    f.harness.event(Event::ModifiersChanged(Modifiers::NONE));
    f.harness.run_steps(2);
    let lane = f
        .session()
        .project()
        .lane(piano, AutomationTarget::Volume)
        .unwrap()
        .clone();
    assert_eq!(lane.points.len(), 2, "a second point");
    assert!(
        lane.points[1].at > Ticks::ZERO && lane.points[1].value < lane.points[0].value,
        "later and lower: a fade"
    );
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_ADD_POINT)
    );

    // A right-click on it takes it away; undo brings it back.
    f.harness.hover_at(spot);
    f.harness.step();
    f.press(spot, PointerButton::Secondary, true, Modifiers::NONE);
    f.harness.step();
    f.press(spot, PointerButton::Secondary, false, Modifiers::NONE);
    f.harness.run_steps(2);
    let points = |f: &Fixture| {
        f.session()
            .project()
            .lane(piano, AutomationTarget::Volume)
            .map_or(0, |l| l.points.len())
    };
    assert_eq!(points(&f), 1, "removed");
    f.key(Key::Z, Key::Z, true, Modifiers::COMMAND);
    f.key(Key::Z, Key::Z, false, Modifiers::COMMAND);
    assert_eq!(points(&f), 2, "undo brings it back");

    // The lane's own button removes it.
    f.harness.get_by_label(text::REMOVE_LANE).click();
    f.harness.run_steps(2);
    assert!(
        f.session()
            .project()
            .lane(piano, AutomationTarget::Volume)
            .is_none()
    );
}

/// A reference plugin (see `CLAUDE.md`), when `VLABS_TEST_PLUGINS` names
/// the folder that holds them.
fn reference_plugin(file: &str, kind: PluginKind) -> Option<PluginInfo> {
    let folder = PathBuf::from(std::env::var_os("VLABS_TEST_PLUGINS")?);
    daw_plugin_host::scan::describe(daw_plugin_host::Format::Clap, &folder.join(file))
        .ok()?
        .into_iter()
        .find(|p| p.kind == kind)
}

#[test]
fn a_plugin_track_keeps_its_settings_when_the_song_is_saved_and_opened() {
    let Some(surge) = reference_plugin("surge/Surge XT.clap", PluginKind::Instrument) else {
        return;
    };
    let effects = reference_plugin("surge/Surge XT Effects.clap", PluginKind::Effect);
    let mut f = new_song();
    let plugins: Vec<PluginInfo> = std::iter::once(surge.clone()).chain(effects).collect();
    f.workspace_mut()
        .session_mut()
        .set_plugins(plugins, finished_search());
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_INSTRUMENT))
        .click();
    f.harness.run_steps(3);
    f.harness.get_by_label_contains(&surge.name).click();
    f.harness.run_steps(3);
    let track = f.session().project().tracks.last().unwrap().id;
    let target = PluginTarget::Instrument(track);
    assert!(f.session().engine().has_plugin(target), "the plugin loaded");

    // Turn its volume down: through a lane, as the song plays.
    let volume = f
        .workspace_mut()
        .session_mut()
        .engine_mut()
        .plugin_params(target)
        .into_iter()
        .find(|p| p.name.to_lowercase().contains("global volume"))
        .unwrap()
        .id;
    let lane = AutomationTarget::Instrument { param: volume };
    let session = f.workspace_mut().session_mut();
    session.add_lane(track, lane);
    session.move_lane_point(
        track,
        lane,
        0,
        AutomationPoint {
            at: Ticks::ZERO,
            value: 0.2,
        },
    );
    session.play();
    for _ in 0..4 {
        f.tick();
    }
    f.workspace_mut().session_mut().stop();
    f.workspace_mut().session_mut().remove_lane(track, lane);
    f.tick();

    // Save, start a new song, open the saved one again.
    let path = std::env::temp_dir().join(format!(
        "interface-plugin-{}.{}",
        std::process::id(),
        daw_ident::PROJECT_FILE_EXTENSION
    ));
    f.workspace_mut().session_mut().save_as(&path).unwrap();
    f.workspace_mut().session_mut().new_song();
    f.tick();
    assert!(!f.session().engine().has_plugin(target));
    f.workspace_mut().session_mut().open(&path).unwrap();
    f.tick();
    let track = f
        .session()
        .project()
        .tracks
        .iter()
        .find(|t| t.instrument.plugin().is_some())
        .unwrap()
        .id;
    let target = PluginTarget::Instrument(track);
    let restored = f
        .workspace_mut()
        .session_mut()
        .engine_mut()
        .plugin_param_value(target, volume)
        .unwrap();
    assert!(
        (restored - 0.2).abs() < 0.01,
        "the volume came back turned down: {restored}"
    );
    assert!(!f.session().is_dirty(), "opening a song changes nothing");
    std::fs::remove_file(path).ok();
}

#[test]
fn a_song_with_a_plugin_track_exports_with_its_sound() {
    let Some(surge) = reference_plugin("surge/Surge XT.clap", PluginKind::Instrument) else {
        return;
    };
    let mut f = new_song();
    let path = std::env::temp_dir().join(format!("interface-plugin-{}.wav", std::process::id()));
    *f.host.export_to.lock().unwrap() = Some(path.clone());
    let session = f.workspace_mut().session_mut();
    session.set_plugins(vec![surge.clone()], finished_search());
    let track = session.add_plugin_track(&surge);
    session.open_track(track);
    // A long note on the plugin track only (the beat silenced).
    session.add_note(Ticks::ZERO, 60, 0.8);
    let drums = session.project().tracks[0].id;
    session.toggle_mute(drums);
    f.tick();
    f.key(Key::E, Key::E, true, Modifiers::COMMAND);
    f.key(Key::E, Key::E, false, Modifiers::COMMAND);
    f.harness.get_by_label(text::EXPORT_CD).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::EXPORT_BUTTON).click();
    f.harness.run_steps(2);
    f.workspace_mut().session_mut().finish_export();
    f.harness.run_steps(3);
    let bytes = std::fs::read(&path).expect("the file was written");
    let data = u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize;
    let loudest = bytes[44..44 + data]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c).unsigned_abs())
        .max()
        .unwrap();
    assert!(
        loudest > 1000,
        "the plugin's note is in the file: {loudest}"
    );
    std::fs::remove_file(path).ok();
}

// ----- Audio (phase 4)

/// A WAV file of `seconds` of a tone, at 48 kHz.
fn tone_file(name: &str, seconds: f64) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "interface-audio-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(name);
    let mut writer = daw_formats::wav::WavWriter::create_with_channels(
        &path,
        daw_formats::wav::WavFormat::Pcm16,
        48_000,
        1,
    )
    .unwrap();
    let samples: Vec<f32> = (0..(seconds * 48_000.0) as usize)
        .map(|i| (i as f32 * 0.06).sin() * 0.5)
        .collect();
    writer.write_interleaved(&samples).unwrap();
    writer.finish().unwrap();
    path
}

impl Fixture {
    /// Draws frames until the sound files are read.
    fn wait_for_audio(&mut self) {
        let start = std::time::Instant::now();
        while self.session().is_loading_audio() {
            assert!(start.elapsed() < std::time::Duration::from_secs(30));
            std::thread::sleep(std::time::Duration::from_millis(5));
            self.harness.run_steps(1);
        }
        self.harness.run_steps(2);
    }

    /// Presses at `from`, moves to `to` in steps, releases.
    fn drag(&mut self, from: Pos2, to: Pos2, modifiers: Modifiers) {
        self.harness.hover_at(from);
        self.harness.step();
        self.press(from, PointerButton::Primary, true, modifiers);
        self.harness.step();
        for i in 1..=8 {
            self.harness.hover_at(from + (to - from) * (i as f32 / 8.0));
            self.harness.step();
        }
        self.press(to, PointerButton::Primary, false, modifiers);
        self.harness.run_steps(2);
    }

    fn right_click(&mut self, pos: Pos2) {
        self.harness.hover_at(pos);
        self.harness.step();
        self.press(pos, PointerButton::Secondary, true, Modifiers::NONE);
        self.harness.step();
        self.press(pos, PointerButton::Secondary, false, Modifiers::NONE);
        self.harness.run_steps(3);
    }

    /// Where a tick is drawn in the song view, as it is first shown: the
    /// whole song (at least four bars) and two bars more fill the timeline.
    fn song_x(&self, tick: Ticks) -> f32 {
        let song = self.harness.get_by_label(text::SONG_VIEW).rect();
        let timeline_left = song.left() + 236.0;
        let width = song.width() - 236.0;
        let length = self.session().song_length() + self.session().project().meter.bar_start(2);
        let per_point = length.0 as f64 / f64::from(width - 16.0);
        timeline_left + (tick.0 as f64 / per_point) as f32
    }

    /// The top of a track's row in the song view (no lanes open): under
    /// the ruler and the folded signature row, 28 points each.
    fn row_top(&self, index: usize) -> f32 {
        let song = self.harness.get_by_label(text::SONG_VIEW).rect();
        song.top() + 28.0 + 28.0 + 64.0 * index as f32
    }
}

#[test]
fn sound_files_are_imported_from_the_menu_onto_a_track_of_their_own() {
    let mut f = new_song();
    *f.host.audio.lock().unwrap() = vec![tone_file("Guitar.wav", 2.0)];
    f.harness.get_by_label(text::MENU).click();
    f.harness.run_steps(2);
    f.harness.get_by_label_contains(text::IMPORT_AUDIO).click();
    f.harness.run_steps(2);
    f.wait_for_audio();
    assert_eq!(f.session().view(), View::Song, "the song comes on screen");
    let track = f.session().project().tracks.last().unwrap().clone();
    assert!(track.instrument.is_audio());
    assert_eq!(track.name, "Guitar");
    assert_eq!(track.clips.len(), 1);
    assert_eq!(track.clips[0].start, Ticks::ZERO, "at the play position");
    assert_eq!(f.session().document().undo_label(), Some(text::EDIT_IMPORT));
    // Ctrl+I asks for files too.
    let asked = *f.host.asked.lock().unwrap();
    f.key(Key::I, Key::I, true, Modifiers::COMMAND);
    f.key(Key::I, Key::I, false, Modifiers::COMMAND);
    assert_eq!(*f.host.asked.lock().unwrap(), asked + 1);
}

#[test]
fn the_audio_button_adds_a_track_that_records_sound() {
    let mut f = new_song();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_AUDIO))
        .click();
    f.harness.run_steps(3);
    let track = f.session().project().tracks.last().unwrap().id;
    assert!(f.session().is_audio_track(track));
    assert_eq!(f.session().focused_track(), track, "in focus");
    // The record button now records sound.
    f.harness.get_by_label(text::RECORD_SOUND);
}

#[test]
fn audio_clips_are_trimmed_faded_and_split_with_the_mouse() {
    let mut f = new_song();
    f.workspace_mut().session_mut().import_audio(
        vec![tone_file("Loop.wav", 2.0)],
        None,
        Ticks::ZERO,
    );
    f.wait_for_audio();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(3);
    let track = f.session().project().tracks.last().unwrap().id;
    let clip = f.session().project().track(track).unwrap().clips[0].id;
    let top = f.row_top(2);
    let middle = top + 34.0;

    // The left end, dragged one beat later: trimmed on the beat, the sound
    // staying where it was.
    let from = Pos2::new(f.song_x(Ticks::ZERO) + 3.0, middle);
    let to = Pos2::new(f.song_x(Ticks::QUARTER) + 1.0, middle);
    f.drag(from, to, Modifiers::NONE);
    let c = *f.session().project().clip(clip).unwrap().1;
    assert_eq!(c.start, Ticks::QUARTER, "trimmed to the beat");
    assert!(matches!(
        c.content,
        daw_model::ClipContent::Audio { offset: 24_000, .. }
    ));
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_TRIM_CLIP)
    );

    // The handle at the top right, dragged left: a fade out.
    let end = f.song_x(c.end());
    let handle = Pos2::new(end - 5.0, top + 4.0 + 5.0);
    f.drag(handle, handle - vec2(40.0, 0.0), Modifiers::NONE);
    let (fade_in, fade_out) = f.session().clip_fades(clip).unwrap();
    assert!(fade_in == 0.0 && fade_out > 0.1, "{fade_in} {fade_out}");
    assert_eq!(f.session().document().undo_label(), Some(text::EDIT_FADE));

    // Right-click in the middle: Split here.
    let inside = Pos2::new(f.song_x(Ticks::QUARTER * 3), middle);
    f.right_click(inside);
    f.harness.get_by_label(text::SPLIT_HERE).click();
    f.harness.run_steps(2);
    assert_eq!(f.session().project().track(track).unwrap().clips.len(), 2);

    // Ctrl+B splits the selected clip at the play position.
    let second = f.session().selected_clip().unwrap();
    f.workspace_mut()
        .session_mut()
        .locate_song(Ticks::QUARTER * 3 + Ticks::QUARTER / 2);
    f.tick();
    f.key(Key::B, Key::B, true, Modifiers::COMMAND);
    f.key(Key::B, Key::B, false, Modifiers::COMMAND);
    assert_eq!(f.session().project().track(track).unwrap().clips.len(), 3);
    assert_ne!(
        f.session().selected_clip(),
        Some(second),
        "the new part is selected"
    );
}

/// Runs the audio thread for one frame's worth of sound and returns it
/// (left channel).
fn listen(f: &mut Fixture, blocks: usize) -> Vec<f32> {
    let mut buffer = vec![0.0_f32; 512];
    let mut heard = Vec::new();
    for _ in 0..blocks {
        f.processor.process(&mut buffer, 2);
        heard.extend(buffer.iter().step_by(2));
    }
    heard
}

/// How many times a sound starts in `samples`: blocks of 256 frames whose
/// loudest sample jumps well above the block before.
fn starts(samples: &[f32]) -> usize {
    let peaks: Vec<f32> = std::iter::once(0.0)
        .chain(
            samples
                .chunks(256)
                .map(|c| c.iter().fold(0.0_f32, |m, s| m.max(s.abs()))),
        )
        .collect();
    peaks
        .windows(2)
        .filter(|w| w[1] > 0.05 && w[1] > w[0] * 2.0)
        .count()
}

#[test]
fn clicking_a_sound_name_plays_it_once() {
    let mut f = beat();
    let name = f
        .harness
        .get_by_label(text::pad_name(PadSound::Snare))
        .rect()
        .center();
    let mut heard = Vec::new();
    f.harness.hover_at(name);
    f.harness.step();
    f.press(name, PointerButton::Primary, true, Modifiers::NONE);
    for _ in 0..6 {
        f.harness.step();
        heard.extend(listen(&mut f, 8));
    }
    f.press(name, PointerButton::Primary, false, Modifiers::NONE);
    for _ in 0..12 {
        f.harness.step();
        heard.extend(listen(&mut f, 8));
    }
    assert_eq!(starts(&heard), 1, "the snare starts once, and only once");
    assert_eq!(f.session().pointer_note(), None, "released");
}

#[test]
fn a_quick_click_on_a_sound_name_plays_it_too() {
    let mut f = beat();
    let name = f
        .harness
        .get_by_label(text::pad_name(PadSound::Kick))
        .rect()
        .center();
    f.harness.hover_at(name);
    f.harness.step();
    // Pressed and released between two frames (a tap on a touchpad).
    f.press(name, PointerButton::Primary, true, Modifiers::NONE);
    f.press(name, PointerButton::Primary, false, Modifiers::NONE);
    let mut heard = Vec::new();
    for _ in 0..12 {
        f.harness.step();
        heard.extend(listen(&mut f, 8));
    }
    assert_eq!(starts(&heard), 1, "the kick plays once");
}

/// A file dropped on the window, as the desktop hands it over.
#[derive(Debug)]
struct DroppedSound(PathBuf);

impl egui::DroppedFile for DroppedSound {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

#[test]
fn a_sound_file_dropped_on_an_audio_track_lands_on_its_beat() {
    let mut f = new_song();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_AUDIO))
        .click();
    f.harness.run_steps(3);
    let track = f.session().project().tracks.last().unwrap().id;
    // Over the audio track, a little after bar 2.
    let spot = Pos2::new(f.song_x(Ticks::WHOLE) + 2.0, f.row_top(2) + 30.0);
    f.harness.hover_at(spot);
    f.harness.step();
    f.harness
        .input_mut()
        .dropped_files
        .push(Arc::new(DroppedSound(tone_file("Drop.wav", 1.0))));
    f.harness.step();
    f.wait_for_audio();
    let clips = f.session().project().track(track).unwrap().clips.clone();
    assert_eq!(clips.len(), 1, "on the track it was dropped on");
    assert_eq!(
        clips[0].start,
        Ticks::WHOLE,
        "on the beat under the pointer"
    );
}

#[test]
fn a_sound_file_dropped_on_a_pattern_brings_the_song_with_it() {
    let mut f = beat();
    assert_eq!(f.session().view(), View::Pattern);
    f.harness
        .input_mut()
        .dropped_files
        .push(Arc::new(DroppedSound(tone_file("Stem.wav", 1.0))));
    f.harness.step();
    f.wait_for_audio();
    assert_eq!(f.session().view(), View::Song);
    let last = f.session().project().tracks.last().unwrap();
    assert!(last.instrument.is_audio());
    assert_eq!(last.name, "Stem");
    assert_eq!(last.clips.len(), 1, "imported once");
}

// ----- Theory and rhythm (decision 0018)

fn c_major() -> daw_model::Scale {
    daw_model::Scale::new(0, daw_model::ScaleKind::Major)
}

#[test]
fn the_key_button_sets_the_songs_key() {
    let mut f = fixture();
    f.harness.get_by_label(text::NO_KEY).click();
    f.harness.run_steps(3);
    // B flat, then minor: the tonic keeps the scale, the scale the tonic.
    f.harness.get_by_label("Bb").click();
    f.harness.run_steps(2);
    let b_flat_major = daw_model::Scale::new(10, daw_model::ScaleKind::Major);
    assert_eq!(f.session().key(), Some(b_flat_major));
    f.harness.get_by_label("minor").click();
    f.harness.run_steps(2);
    let b_flat_minor = daw_model::Scale::new(10, daw_model::ScaleKind::Minor);
    assert_eq!(f.session().key(), Some(b_flat_minor));
    assert_eq!(f.session().pattern_key(), Some(b_flat_minor));
    f.key(Key::Escape, Key::Escape, true, Modifiers::NONE);
    f.key(Key::Escape, Key::Escape, false, Modifiers::NONE);
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&text::song_key_name(Some(b_flat_minor)));
    f.workspace_mut().session_mut().undo();
    assert_eq!(
        f.session().key(),
        Some(b_flat_major),
        "each choice is undoable"
    );
}

#[test]
fn a_chord_stamp_places_a_chord_of_the_key_with_one_click() {
    let mut f = fixture();
    f.workspace_mut().session_mut().set_key(Some(c_major()));
    f.harness.run_steps(2);
    f.harness.get_by_label(text::DRAW_NOTES_SHORT).click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::TRIADS_IN_KEY).click();
    f.harness.run_steps(3);
    assert_eq!(
        f.session().draw,
        daw_ui::session::Draw::Chord(daw_model::ChordShape::InKey(3))
    );
    let roll = f.roll();
    f.click(roll.center(), Modifiers::NONE);
    let notes = f.notes();
    assert_eq!(notes.len(), 3, "a triad");
    let mut keys: Vec<u8> = notes.iter().map(|n| n.key).collect();
    keys.sort_unstable();
    assert!(
        keys.iter().all(|&k| c_major().contains(k)),
        "{keys:?} in C major"
    );
    assert!(
        daw_model::ChordKind::recognise(keys[0], &keys).is_some(),
        "{keys:?} is a chord"
    );
    assert!(
        notes.iter().all(|n| n.start == notes[0].start),
        "all at once"
    );
    assert!(
        f.session()
            .status()
            .is_some_and(|s| s.starts_with("Placed"))
    );
    f.workspace_mut().session_mut().undo();
    assert!(f.notes().is_empty(), "one undo step");
}

#[test]
fn the_snap_menu_offers_tuplets_and_keeping_notes_in_the_key() {
    let mut f = fixture();
    f.harness.get_by_label("1/16").click();
    f.harness.run_steps(3);
    f.harness.get_by_label("1/16 quintuplet").click();
    f.harness.run_steps(2);
    assert_eq!(f.session().grid, daw_ui::session::Grid::Note(20));
    f.workspace_mut().session_mut().set_key(Some(c_major()));
    f.harness.run_steps(2);
    f.harness.get_by_label("1/16 quintuplet").click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::KEEP_IN_KEY).click();
    f.harness.run_steps(2);
    assert!(f.session().keeps_in_key());
    f.key(Key::Escape, Key::Escape, true, Modifiers::NONE);
    f.key(Key::Escape, Key::Escape, false, Modifiers::NONE);
    f.harness
        .get_by_label(&format!("1/16 quintuplet  ·  {}", text::IN_KEY));
}

#[test]
fn swing_is_chosen_for_the_beat() {
    let mut f = beat();
    f.harness.get_by_label(text::STRAIGHT).click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::TRIPLET_FEEL).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::SWING_EIGHTHS).click();
    f.harness.run_steps(2);
    let swing = f.session().swing();
    assert_eq!(swing.percent, 67);
    assert_eq!(swing.unit, Ticks::note_value(1, 8));
}

#[test]
fn the_end_of_a_beat_is_dragged_to_any_step() {
    let mut f = beat();
    let last = f.harness.get_by_label("Kick, step 16").rect();
    let step = last.width();
    let y = last.top() - 13.0;
    let from = Pos2::new(last.right(), y);
    let to = Pos2::new(last.right() - 4.0 * step, y);
    f.drag(from, to, Modifiers::NONE);
    assert_eq!(f.session().pattern().length, Ticks::note_value(12, 16));
    assert!(f.harness.query_by_label("Kick, step 13").is_none());
    f.harness.get_by_label(&text::steps(12));
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_PATTERN_LENGTH),
        "one undo step"
    );
}

#[test]
fn the_notes_menu_transposes_by_intervals_and_inverts() {
    let mut f = fixture();
    {
        let s = f.workspace_mut().session_mut();
        s.draw =
            daw_ui::session::Draw::Chord(daw_model::ChordShape::Kind(daw_model::ChordKind::Major));
        s.add_chord(Ticks::ZERO, 60, 0.8);
        s.pointer_off();
    }
    f.harness.run_steps(2);
    let keys = |f: &Fixture| {
        let mut keys: Vec<u8> = f.notes().iter().map(|n| n.key).collect();
        keys.sort_unstable();
        keys
    };
    // A drop-down button: its arrow follows its name.
    f.harness.get_by_label_contains(text::NOTES_MENU).click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::TRANSPOSE_UP).click();
    f.harness.run_steps(3);
    f.harness
        .get_by_label(&text::by_interval(true, text::interval_name(7)))
        .click();
    f.harness.run_steps(2);
    assert_eq!(keys(&f), [67, 71, 74], "up a perfect fifth");
    // A drop-down button: its arrow follows its name.
    f.harness.get_by_label_contains(text::NOTES_MENU).click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::INVERT_UP).click();
    f.harness.run_steps(2);
    assert_eq!(keys(&f), [71, 74, 79], "first inversion");
    // And from the keyboard: Alt+Down inverts back down.
    f.key(Key::ArrowDown, Key::ArrowDown, true, Modifiers::ALT);
    f.key(Key::ArrowDown, Key::ArrowDown, false, Modifiers::ALT);
    assert_eq!(keys(&f), [67, 71, 74]);
}

/// The song view, with the signature row opened.
fn song_with_signatures() -> Fixture {
    let mut f = new_song();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::SHOW_SIGNATURES).click();
    f.harness.run_steps(3);
    f
}

#[test]
fn the_signature_row_changes_the_time_signature_from_a_bar() {
    let mut f = song_with_signatures();
    let strip = f.harness.get_by_label(text::SIGNATURE_TAGS).rect();
    let bar3 = f.session().project().meter.bar_start(2);
    let spot = Pos2::new(f.song_x(bar3) + 12.0, strip.center().y);
    f.click(spot, Modifiers::NONE);
    f.harness.get_by_label(&text::from_bar(2));
    // The panel stays put while it is open (it once slid to the window's
    // edge, frame by frame, out from under the pointer).
    let before = f.harness.get_by_label("7/8").rect();
    f.harness.run_steps(3);
    assert_eq!(f.harness.get_by_label("7/8").rect(), before);
    f.harness.get_by_label("7/8").click();
    f.harness.run_steps(2);
    let seven_eight = daw_model::TimeSignature::new(7, 8).unwrap();
    assert_eq!(f.session().meter_change_at(2), Some(seven_eight));
    f.key(Key::Escape, Key::Escape, true, Modifiers::NONE);
    f.key(Key::Escape, Key::Escape, false, Modifiers::NONE);
    f.harness.run_steps(2);
    assert!(
        f.harness.query_by_label(&text::from_bar(2)).is_none(),
        "Escape closes the panel"
    );
    f.harness
        .get_by_label(&text::signature_tag(2, &text::time_signature(7, 8)));
    f.workspace_mut().session_mut().undo();
    assert_eq!(f.session().meter_change_at(2), None, "undone");
}

#[test]
fn the_tempo_line_takes_changes_that_are_dragged_and_removed() {
    let mut f = song_with_signatures();
    let strip = f.harness.get_by_label(text::TEMPO_LINE).rect();
    let bar5 = Ticks::WHOLE * 4;
    let spot = Pos2::new(f.song_x(bar5) + 1.0, strip.center().y + 14.0);
    f.click(spot, Modifiers::NONE);
    let changes = f.session().tempo_changes().to_vec();
    assert_eq!(changes.len(), 2, "a click adds a tempo change");
    assert_eq!(changes[1].at, bar5, "on the beat");
    f.harness.get_by_label(text::GLIDE);
    f.key(Key::Escape, Key::Escape, true, Modifiers::NONE);
    f.key(Key::Escape, Key::Escape, false, Modifiers::NONE);
    f.harness.run_steps(2);
    assert!(f.harness.query_by_label(text::GLIDE).is_none(), "closed");
    // A quick click (press and release in one frame) opens it too, and a
    // click elsewhere closes it.
    let point = f
        .harness
        .get_by_label(&text::tempo_point(changes[1].bpm, 4))
        .rect()
        .center();
    f.harness.hover_at(point);
    f.harness.step();
    f.press(point, PointerButton::Primary, true, Modifiers::NONE);
    f.press(point, PointerButton::Primary, false, Modifiers::NONE);
    f.harness.run_steps(3);
    f.harness.get_by_label(text::GLIDE);
    f.click(
        Pos2::new(point.x - 200.0, strip.top() - 60.0),
        Modifiers::NONE,
    );
    assert!(
        f.harness.query_by_label(text::GLIDE).is_none(),
        "closed again"
    );
    f.key(Key::Escape, Key::Escape, true, Modifiers::NONE);
    f.key(Key::Escape, Key::Escape, false, Modifiers::NONE);
    f.harness.run_steps(2);
    // Dragged up, it speeds up.
    let point = f
        .harness
        .get_by_label(&text::tempo_point(changes[1].bpm, 4))
        .rect()
        .center();
    f.drag(point, point - vec2(0.0, 12.0), Modifiers::NONE);
    let faster = f.session().tempo_changes()[1].bpm;
    assert!(faster > 120.0, "now {faster}");
    // Right-clicked, it goes.
    let point = f
        .harness
        .get_by_label(&text::tempo_point(faster, 4))
        .rect()
        .center();
    f.right_click(point);
    assert_eq!(f.session().tempo_changes().len(), 1);
}

fn escape(f: &mut Fixture) {
    f.key(Key::Escape, Key::Escape, true, Modifiers::NONE);
    f.key(Key::Escape, Key::Escape, false, Modifiers::NONE);
    f.harness.run_steps(2);
}

#[test]
fn a_ritardando_arrives_gradually_and_takes_a_shape() {
    let mut f = song_with_signatures();
    let strip = f.harness.get_by_label(text::TEMPO_LINE).rect();
    let bar5 = Ticks::WHOLE * 4;
    let spot = Pos2::new(f.song_x(bar5) + 1.0, strip.center().y + 14.0);
    f.click(spot, Modifiers::NONE);
    escape(&mut f);
    // Dragged down, the new tempo is slower.
    let point = f
        .harness
        .get_by_label(&text::tempo_point(120.0, 4))
        .rect()
        .center();
    f.drag(point, point + vec2(0.0, 16.0), Modifiers::NONE);
    let slower = f.session().tempo_changes()[1].bpm;
    assert!(slower < 120.0, "now {slower}");
    // Its panel makes the tempo arrive there gradually: a ritardando.
    let point = f
        .harness
        .get_by_label(&text::tempo_point(slower, 4))
        .rect()
        .center();
    f.click(point, Modifiers::NONE);
    f.harness.get_by_label(text::ARRIVE_GRADUALLY).click();
    f.harness.run_steps(2);
    assert!(
        f.session().tempo_changes()[0].glide,
        "the start glides to it"
    );
    escape(&mut f);
    // The glide's handle opens its shapes: Late holds the tempo back.
    let label = text::glide_handle(true, 0, 4);
    let handle = f.harness.get_by_label(&label).rect().center();
    f.click(handle, Modifiers::NONE);
    f.harness.get_by_label(&text::glide_tempos(120.0, slower));
    f.harness.get_by_label(text::GLIDE_LATE).click();
    f.harness.run_steps(2);
    assert_eq!(f.session().tempo_changes()[0].curve, 0.5);
    let middle = f.session().project().tempo.bpm_at(Ticks::WHOLE * 2);
    assert!(middle > f64::midpoint(120.0, slower), "held back: {middle}");
    escape(&mut f);
    assert!(
        f.harness.query_by_label(text::GLIDE_LATE).is_none(),
        "closed"
    );
    // Dragged down, the handle bends it the other way, in one undo step.
    let handle = f.harness.get_by_label(&label).rect().center();
    f.drag(handle, handle + vec2(0.0, 12.0), Modifiers::NONE);
    let bent = f.session().tempo_changes()[0].curve;
    assert!(bent < 0.5, "now {bent}");
    f.workspace_mut().session_mut().undo();
    assert_eq!(f.session().tempo_changes()[0].curve, 0.5, "undone at once");
    // Right-clicked, the handle makes the change sudden again.
    let handle = f.harness.get_by_label(&label).rect().center();
    f.right_click(handle);
    assert!(!f.session().tempo_changes()[0].glide);
}

#[test]
fn the_piano_roll_shows_the_tempo_under_its_notes() {
    let mut f = fixture();
    assert!(
        f.harness.query_by_label(text::TEMPO_LINE).is_none(),
        "the lane shows velocity first"
    );
    f.harness.get_by_label(text::LANE_TEMPO).click();
    f.harness.run_steps(2);
    let strip = f.harness.get_by_label(text::TEMPO_LINE).rect();
    // A click on the line adds a tempo change where the pattern plays; its
    // panel opens above the lane, which is at the bottom of the window.
    let spot = Pos2::new(strip.left() + strip.width() * 0.5, strip.center().y + 10.0);
    f.click(spot, Modifiers::NONE);
    let changes = f.session().tempo_changes().to_vec();
    assert_eq!(changes.len(), 2, "a tempo change");
    let placement = f.session().placement();
    assert!(changes[1].at > placement.start && changes[1].at < placement.start + placement.length);
    assert!(f.notes().is_empty(), "no note was placed");
    let glide = f.harness.get_by_label(text::GLIDE).rect();
    assert!(glide.bottom() <= strip.top(), "above the lane");
    escape(&mut f);
    assert!(
        f.harness.query_by_label(text::GLIDE).is_none(),
        "Escape closes it"
    );
    // It is the song's tempo: the song view shows the same change.
    f.harness.get_by_label(text::LANE_VELOCITY).click();
    f.harness.run_steps(2);
    assert!(f.harness.query_by_label(text::TEMPO_LINE).is_none());
}

#[test]
fn panels_and_menus_take_clicks_that_last_several_frames() {
    // A person's click lasts longer than a frame: the press must not close
    // the panel or menu it lands in before the release.
    let mut f = beat();
    f.harness
        .get_by_label(text::pad_name(PadSound::Snare))
        .click();
    f.harness.run_steps(3);
    let mute = f.harness.get_by_label(text::PAD_MUTED).rect().center();
    f.click(mute, Modifiers::NONE);
    let drums = f.session().track_id();
    assert!(f.session().pad(drums, 38).muted, "the snare is silenced");

    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    let song = f.harness.get_by_label(text::SONG_VIEW).rect();
    // The drum clip: top row, under the ruler and the folded signature row.
    let clip = Pos2::new(song.left() + 236.0 + 60.0, song.top() + 28.0 + 28.0 + 32.0);
    f.right_click(clip);
    let remove = f.harness.get_by_label(text::REMOVE_CLIP).rect().center();
    f.click(remove, Modifiers::NONE);
    assert!(
        f.session().project().track(drums).unwrap().clips.is_empty(),
        "the clip menu's entry worked"
    );
}

#[test]
fn the_synth_is_chosen_from_the_list_and_shaped_in_its_own_window() {
    let mut f = new_song();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_INSTRUMENT))
        .click();
    f.harness.run_steps(3);
    f.harness
        .get_by_label_contains(text::ADD_SYNTH_HINT)
        .click();
    f.harness.run_steps(3);
    let synth = f.session().project().tracks.last().unwrap().clone();
    assert_eq!(synth.name, text::SYNTH_TRACK);
    assert_eq!(
        synth.instrument.synth(),
        Some(&daw_model::SynthPreset::SoftKeys.settings()),
        "a new synth starts with Soft keys"
    );
    f.harness.get_by_label_contains("Synth · Soft keys");

    // Its window opens from the track header.
    f.harness.get_by_label(text::OPEN_SYNTH).click();
    f.harness.run_steps(4);
    // (Its title is the track's name, "Synth", like the track's own
    // label: the window is found by what is in it.)
    f.harness.get_by_label(text::FILTER_CURVE);
    assert!(f.session().is_synth_window_open(synth.id));

    // A preset sets everything at once: one undo step.
    f.harness.get_by_label(text::SYNTH_PRESET).click();
    f.harness.run_steps(3);
    f.harness.get_by_label("Bass").click();
    f.harness.run_steps(3);
    assert_eq!(
        f.session().synth_settings(synth.id),
        Some(daw_model::SynthPreset::Bass.settings())
    );
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_SYNTH_PRESET)
    );

    // Dragging the cutoff slider changes it as one undo step.
    let before = f.session().synth_settings(synth.id).unwrap();
    let slider = f
        .harness
        .get_by_role_and_label(egui::accesskit::Role::Slider, text::CUTOFF)
        .rect();
    let depth = f.session().document().undo_depth();
    f.drag(
        slider.left_center() + vec2(10.0, 0.0),
        slider.left_center() + vec2(70.0, 0.0),
        Modifiers::NONE,
    );
    let after = f.session().synth_settings(synth.id).unwrap();
    assert!(after.cutoff_hz > before.cutoff_hz, "the filter opened");
    assert_eq!(f.session().document().undo_depth(), depth + 1, "one step");
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_CHANGE_SYNTH)
    );
    f.key(Key::Z, Key::Z, true, Modifiers::COMMAND);
    f.key(Key::Z, Key::Z, false, Modifiers::COMMAND);
    assert_eq!(f.session().synth_settings(synth.id), Some(before));

    // The computer keyboard plays the synth while its window is open.
    f.harness.run_steps(2);
    f.key(Key::Z, Key::Z, true, Modifiers::NONE);
    f.tick();
    let heard = listen(&mut f, 16);
    f.key(Key::Z, Key::Z, false, Modifiers::NONE);
    assert!(
        heard.iter().fold(0.0_f32, |m, s| m.max(s.abs())) > 0.01,
        "a note is heard"
    );

    // The same button closes the window (it opened clear of it), and so
    // does the window's own close button.
    f.harness.get_by_label(text::CLOSE_SYNTH).click();
    f.harness.run_steps(3);
    assert!(!f.session().is_synth_window_open(synth.id));
    assert!(f.harness.query_by_label(text::FILTER_CURVE).is_none());
    f.harness.get_by_label(text::OPEN_SYNTH).click();
    f.harness.run_steps(4);
    f.harness.get_by_label("Close window").click();
    f.harness.run_steps(3);
    assert!(!f.session().is_synth_window_open(synth.id));
}

#[test]
fn a_synth_setting_changes_over_the_song_with_a_lane() {
    let mut f = new_song();
    let synth = f.workspace_mut().session_mut().add_synth_track();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(3);
    // The synth's Automation button (third track) offers its settings.
    f.harness
        .get_all_by_label(text::AUTOMATION)
        .nth(2)
        .unwrap()
        .click();
    f.harness.run_steps(3);
    let cutoff = text::lane_name(text::SYNTH_TRACK, text::CUTOFF);
    f.harness.get_by_label(&cutoff).click();
    f.harness.run_steps(3);
    let target = AutomationTarget::Instrument { param: 0 };
    let lane = f.session().project().lane(synth, target).unwrap().clone();
    let settings = f.session().synth_settings(synth).unwrap();
    assert_eq!(lane.points.len(), 1);
    assert!(
        (lane.points[0].value - settings.param(0).unwrap()).abs() < 1e-6,
        "it starts at the present cutoff"
    );
}

/// Right-clicks a point of the window.
fn right_click(f: &mut Fixture, at: Pos2) {
    f.harness.hover_at(at);
    f.harness.step();
    f.press(at, PointerButton::Secondary, true, Modifiers::NONE);
    f.harness.step();
    f.press(at, PointerButton::Secondary, false, Modifiers::NONE);
    f.harness.run_steps(3);
}

#[test]
fn a_track_places_and_swaps_any_of_its_patterns_from_a_right_click() {
    // A copy of the beat, edited under its own name, as the owner did.
    let mut f = new_song();
    let drums = f.session().project().tracks[0].id;
    let first = f.session().pattern_id();
    let copy = f.workspace_mut().session_mut().duplicate_pattern().unwrap();
    let copy_name = f.session().project().pattern(copy).unwrap().name.clone();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    let song = f.harness.get_by_label(text::SONG_VIEW).rect();
    let row_y = song.top() + 28.0 + 28.0 + 32.0;

    // An empty spot of the drum track (after its four bars): its menu
    // names the track's patterns; choosing the copy places it there.
    let empty = Pos2::new(song.left() + 236.0 + 860.0, row_y);
    right_click(&mut f, empty);
    f.harness.get_by_label(text::PLACE_PATTERN_HERE);
    f.harness.get_by_label(&copy_name).click();
    f.harness.run_steps(3);
    let clips = f.session().project().track(drums).unwrap().clips.clone();
    assert_eq!(clips.len(), 2);
    let placed = clips.iter().find(|c| c.pattern() == Some(copy)).unwrap();
    assert!(
        placed.start > Ticks::WHOLE * 3,
        "where it was right-clicked"
    );
    assert!(f.harness.query_by_label(text::PLACE_PATTERN_HERE).is_none());

    // The first clip's menu swaps it for the copy; undo swaps it back.
    let clip = Pos2::new(song.left() + 236.0 + 60.0, row_y);
    right_click(&mut f, clip);
    f.harness.get_by_label(text::PLAY_ANOTHER_PATTERN);
    f.harness.get_by_label(&copy_name).click();
    f.harness.run_steps(3);
    let starts_with = |f: &Fixture| {
        f.session()
            .project()
            .track(drums)
            .unwrap()
            .clips
            .iter()
            .find(|c| c.start == Ticks::ZERO)
            .and_then(daw_model::Clip::pattern)
    };
    assert_eq!(starts_with(&f), Some(copy), "the first clip plays the copy");
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_SWAP_PATTERN)
    );
    f.key(Key::Z, Key::Z, true, Modifiers::COMMAND);
    f.key(Key::Z, Key::Z, false, Modifiers::COMMAND);
    assert_eq!(starts_with(&f), Some(first));

    // "New pattern" in an empty spot's menu makes one and places it.
    f.key(Key::Z, Key::Z, true, Modifiers::COMMAND);
    f.key(Key::Z, Key::Z, false, Modifiers::COMMAND);
    assert_eq!(f.session().project().track(drums).unwrap().clips.len(), 1);
    right_click(&mut f, empty);
    f.harness.get_by_label(text::NEW_PATTERN_HERE).click();
    f.harness.run_steps(3);
    assert_eq!(f.session().project().track_patterns(drums).count(), 3);
    assert_eq!(f.session().project().track(drums).unwrap().clips.len(), 2);
}

/// Opens the Export dialog with Ctrl+E.
fn open_export(f: &mut Fixture) {
    f.key(Key::E, Key::E, true, Modifiers::COMMAND);
    f.key(Key::E, Key::E, false, Modifiers::COMMAND);
    f.harness.run_steps(3);
    f.harness.get_by_label(text::EXPORT_TITLE);
}

#[test]
fn the_export_dialog_writes_flac_dawproject_and_midi() {
    let mut f = beat();
    // A note in the beat, so every file has something in it.
    let kick = square(&f, PadSound::Kick, 1);
    f.click(kick, Modifiers::NONE);
    let folder = std::env::temp_dir().join(format!("export-kinds-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();

    // FLAC, studio quality.
    let flac = folder.join("song.flac");
    *f.host.export_to.lock().unwrap() = Some(flac.clone());
    open_export(&mut f);
    f.harness.get_by_label(text::EXPORT_FLAC).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::EXPORT_BUTTON).click();
    f.harness.run_steps(2);
    f.workspace_mut().session_mut().finish_export();
    f.harness.run_steps(3);
    let sound = daw_formats::audio::decode(&std::fs::read(&flac).unwrap()).unwrap();
    assert_eq!(sound.format, daw_formats::audio::AudioFormat::Flac);
    assert_eq!(sound.sample_rate, 48_000);
    assert!(sound.frames() > 48_000, "the beat and its tail");
    let peak = sound.channels[0]
        .iter()
        .fold(0.0_f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05, "the kick is in it");

    // DAWproject: the song for another program.
    let project = folder.join("song");
    *f.host.export_to.lock().unwrap() = Some(project.clone());
    open_export(&mut f);
    f.harness.get_by_label(text::EXPORT_DAWPROJECT).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::EXPORT_BUTTON).click();
    f.harness.run_steps(3);
    let written = folder.join("song.dawproject");
    assert!(written.is_file(), "the ending is added");
    let names = daw_formats::dawproject::ImportNames {
        track: &|n| format!("Track {n}"),
        pattern: &|n| format!("Pattern {n}"),
    };
    let back = daw_formats::dawproject::import(&written, &folder.join("opened"), &names).unwrap();
    assert_eq!(back.project.tracks.len(), 2);

    // MIDI: the notes.
    let midi = folder.join("song.mid");
    *f.host.export_to.lock().unwrap() = Some(midi.clone());
    open_export(&mut f);
    f.harness.get_by_label(text::EXPORT_MIDI).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::EXPORT_BUTTON).click();
    f.harness.run_steps(3);
    let file = daw_formats::midi::read(&std::fs::read(&midi).unwrap()).unwrap();
    assert_eq!(
        file.tracks.len(),
        3,
        "the conductor, the drums and the piano"
    );
    std::fs::remove_dir_all(folder).ok();
}

#[test]
fn midi_and_dawproject_files_open_as_new_songs_and_import_into_one() {
    let folder = std::env::temp_dir().join(format!("open-other-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    // A MIDI file of a song: a melody.
    let mut source = daw_model::Project::starter("Tune", "Lead");
    let (_, pattern) = source.first_pattern().unwrap();
    for (i, key) in [60_u8, 62, 64].iter().enumerate() {
        source.add_note(
            pattern,
            daw_model::Note {
                id: daw_model::NoteId(0),
                start: Ticks::QUARTER * i64::try_from(i).unwrap(),
                length: Ticks::QUARTER,
                key: *key,
                velocity: 0.8,
                pitch_offset: 0.0,
            },
        );
    }
    let midi = folder.join("tune.mid");
    std::fs::write(
        &midi,
        daw_formats::midi::write(&daw_formats::midi::song_to_midi(&source)),
    )
    .unwrap();

    // Opened: a new, unsaved song named after the file.
    let mut f = new_song();
    f.workspace_mut().open(midi.clone());
    f.harness.run_steps(3);
    let s = f.session();
    assert_eq!(s.project().title, "tune");
    assert_eq!(s.project().tracks.len(), 1);
    assert_eq!(s.project().tracks[0].name, "Lead");
    assert!(s.is_dirty() && s.path().is_none(), "Save asks where");
    f.harness
        .get_by_label_contains("Opened tune.mid as a new song");

    // Imported into another song with Ctrl+I: a track of its own, one
    // undo step.
    let mut f = new_song();
    *f.host.audio.lock().unwrap() = vec![midi.clone()];
    f.key(Key::I, Key::I, true, Modifiers::COMMAND);
    f.key(Key::I, Key::I, false, Modifiers::COMMAND);
    f.harness.run_steps(3);
    assert_eq!(f.session().project().tracks.len(), 3);
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_IMPORT_MIDI)
    );
    let lead = f.session().project().tracks.last().unwrap().clone();
    assert_eq!(lead.name, "Lead");
    f.key(Key::Z, Key::Z, true, Modifiers::COMMAND);
    f.key(Key::Z, Key::Z, false, Modifiers::COMMAND);
    assert_eq!(f.session().project().tracks.len(), 2);

    // A DAWproject file opens too.
    let project = folder.join("tune.dawproject");
    let sounds = folder.join("sounds");
    std::fs::create_dir_all(&sounds).unwrap();
    daw_formats::dawproject::export(&source, &sounds, &project).unwrap();
    f.workspace_mut().open(project);
    f.harness.run_steps(3);
    assert_eq!(f.session().project().tracks.len(), 1);
    assert_eq!(f.session().project().tracks[0].name, "Lead");
    std::fs::remove_dir_all(folder).ok();
}

// ----- Plugins and sound libraries (decision 0023)

/// Opens the Plugins window from the menu.
fn open_plugins_window(f: &mut Fixture) {
    f.harness.get_by_label(text::MENU).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::MANAGE_PLUGINS).click();
    f.harness.run_steps(4);
}

#[test]
fn the_plugins_window_lists_every_plugin_and_removes_the_ones_added_here() {
    let mut f = new_song();
    f.workspace_mut()
        .session_mut()
        .set_plugins(made_up_plugins(), finished_search());
    let mine = PathBuf::from("/nowhere/Space Echo.clap");
    f.workspace_mut().set_plugin_places(PluginPlaces {
        usual: vec![PathBuf::from("/usr/lib/clap")],
        own: Some(PathBuf::from("/data/plugins")),
        added: vec![mine.clone()],
        bridge: None,
        busy: false,
    });
    open_plugins_window(&mut f);
    f.harness.get_by_label(text::PLUGINS_TITLE);
    f.harness
        .get_by_label(&text::counted(text::INSTRUMENTS_HEADING, 1));
    f.harness
        .get_by_label(&text::counted(text::EFFECTS_HEADING, 1));
    f.harness.get_by_label("Cloud Synth");
    f.harness.get_by_label("Space Echo");
    // Only the plugin added here can be removed here.
    assert_eq!(f.harness.query_all_by_label(text::REMOVE).count(), 1);
    f.harness.get_by_label(text::REMOVE).click();
    f.harness.run_steps(2);
    assert!(
        f.workspace_mut()
            .take_requests()
            .contains(&Request::RemovePlugin(mine.clone()))
    );
    // The program says what it did; the window shows it.
    f.workspace_mut().plugin_job_done(&PluginJobDone {
        removed: vec![mine],
        ..PluginJobDone::default()
    });
    f.harness.run_steps(2);
    // In the window, and on the status line under it.
    assert!(f.harness.query_all_by_label("Removed: Space Echo.").count() >= 1);
    // Searching narrows the list.
    f.harness
        .get_by_role(egui::accesskit::Role::TextInput)
        .click();
    f.harness.run_steps(2);
    f.harness.event(Event::Text("cloud".to_owned()));
    f.harness.run_steps(2);
    assert!(f.harness.query_by_label("Space Echo").is_none());
    f.harness.get_by_label("Cloud Synth");
}

#[test]
fn a_plugin_dropped_on_the_window_is_added_and_the_outcome_explained() {
    let mut f = new_song();
    let download = PathBuf::from("/downloads/Reverb.vst3");
    let old = PathBuf::from("/downloads/Old.dll");
    f.harness.input_mut().dropped_files.extend([
        Arc::new(DroppedSound(download.clone())) as Arc<dyn egui::DroppedFile + Send + Sync>,
        Arc::new(DroppedSound(old.clone())),
    ]);
    f.harness.step();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::PLUGINS_TITLE);
    let requests = f.workspace_mut().take_requests();
    assert!(
        requests.contains(&Request::AddPlugins(vec![download, old.clone()])),
        "{requests:?}"
    );
    assert!(
        f.session()
            .project()
            .tracks
            .iter()
            .all(|t| !t.instrument.is_audio()),
        "plugins are not imported as sounds"
    );
    f.workspace_mut().plugin_job_done(&PluginJobDone {
        added: vec![PathBuf::from("/data/plugins/vst3/Reverb.vst3")],
        skipped: vec![(old, daw_plugin_host::install::Skip::Vst2)],
        ..PluginJobDone::default()
    });
    f.harness.run_steps(2);
    assert!(f.harness.query_all_by_label("Added: Reverb.").count() >= 1);
    f.harness.get_by_label_contains("Old.dll is a VST2 plugin");
}

#[test]
fn plugins_come_from_files_and_from_folders_searched_where_they_are() {
    let mut f = new_song();
    let files = vec![PathBuf::from("/downloads/Synth.zip")];
    *f.host.plugin_files.lock().unwrap() = files.clone();
    open_plugins_window(&mut f);
    f.harness.get_by_label(text::ADD_PLUGIN_FILES).click();
    f.harness.run_steps(2);
    assert!(
        f.workspace_mut()
            .take_requests()
            .contains(&Request::AddPlugins(files))
    );
    // A folder of the person's own is searched where it is.
    let folder = PathBuf::from("/music/plugins");
    *f.host.folder.lock().unwrap() = Some(folder.clone());
    f.harness.get_by_label(text::FOLDERS_SEARCHED).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::SEARCH_ANOTHER_FOLDER).click();
    f.harness.run_steps(2);
    assert_eq!(
        f.workspace().settings().plugin_folders,
        vec![folder.clone()]
    );
    assert!(
        f.workspace_mut()
            .take_requests()
            .contains(&Request::FindPlugins)
    );
    f.harness.get_by_label(&folder.display().to_string());
    // Away from the button just used (its hint would cover the list).
    f.harness.hover_at(Pos2::new(4.0, 4.0));
    f.harness.run_steps(2);
    f.harness.get_by_label(text::REMOVE).click();
    f.harness.run_steps(2);
    assert!(f.workspace().settings().plugin_folders.is_empty());
    assert!(
        f.workspace_mut()
            .take_requests()
            .contains(&Request::FindPlugins)
    );
}

#[test]
fn windows_plugins_on_linux_say_what_is_missing() {
    let mut f = new_song();
    f.workspace_mut().set_plugin_places(PluginPlaces {
        bridge: Some(BridgeState {
            wine: true,
            yabridge: false,
        }),
        ..PluginPlaces::default()
    });
    open_plugins_window(&mut f);
    f.harness
        .get_by_label(&text::windows_plugins_missing(true, false));
    f.harness.get_by_label(text::WINDOWS_PLUGINS_STEPS[1]);
    f.harness.get_by_label(text::CHECK_AGAIN).click();
    f.harness.run_steps(2);
    assert!(
        f.workspace_mut()
            .take_requests()
            .contains(&Request::SetUpWindowsPlugins)
    );
}

#[test]
fn the_instrument_chooser_leads_to_the_plugins_window_and_back() {
    let mut f = new_song();
    f.workspace_mut()
        .session_mut()
        .set_plugins(made_up_plugins(), finished_search());
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_INSTRUMENT))
        .click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::MANAGE_PLUGINS).click();
    f.harness.run_steps(4);
    f.harness.get_by_label(text::PLUGINS_TITLE);
    assert!(f.harness.query_by_label(text::INSTRUMENTS_TITLE).is_none());
    f.harness.get_by_label(text::CLOSE).click();
    f.harness.run_steps(4);
    f.harness.get_by_label(text::INSTRUMENTS_TITLE);
}

/// A folder of sounds for the library: two drum sounds, a loop, and a
/// SoundFont with two presets.
fn sound_folder() -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "interface-library-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    for (relative, seconds) in [
        ("Drums/Kick.wav", 0.5),
        ("Drums/Snare.wav", 0.5),
        ("Loops/Groove.wav", 1.0),
    ] {
        let path = folder.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(tone_file("tone.wav", seconds), &path).unwrap();
    }
    let mut font = daw_dsp::sf2_writer::sine_test_font(48_000);
    font.presets.push(daw_dsp::sf2_writer::PresetDef {
        name: "Second sine".to_owned(),
        bank: 0,
        program: 5,
        instrument: 0,
    });
    std::fs::write(folder.join("Tones.sf2"), font.to_bytes()).unwrap();
    folder
}

/// Draws frames until `label` is on screen (a folder being listed).
fn wait_for_label(f: &mut Fixture, label: &str) {
    let start = std::time::Instant::now();
    while f.harness.query_by_label(label).is_none() {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(20),
            "{label} never appeared"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        f.harness.run_steps(1);
    }
}

/// Shows the library and adds `folder` to it, as a person does.
fn open_library(f: &mut Fixture, folder: &std::path::Path) {
    *f.host.folder.lock().unwrap() = Some(folder.to_path_buf());
    f.harness.get_by_label(text::SHOW_LIBRARY).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::ADD_SOUND_FOLDER).click();
    f.harness.run_steps(2);
    wait_for_label(f, "Loops");
}

/// The loudest sample the engine plays over a few blocks.
fn loudest(f: &mut Fixture) -> f32 {
    let mut buffer = vec![0.0_f32; 1024];
    let mut loudest = 0.0_f32;
    for _ in 0..4 {
        f.processor.process(&mut buffer, 2);
        loudest = buffer.iter().fold(loudest, |m, s| m.max(s.abs()));
    }
    loudest
}

#[test]
fn the_library_browses_folders_and_plays_a_sound_when_clicked() {
    let mut f = beat();
    let folder = sound_folder();
    open_library(&mut f, &folder);
    assert_eq!(f.workspace().settings().sound_folders, vec![folder.clone()]);
    assert!(f.workspace().settings().show_library);
    for label in ["Drums", "Loops", "Tones.sf2"] {
        f.harness.get_by_label(label);
    }
    assert!(
        f.harness.query_by_label(text::ON_SCREEN_KEYBOARD).is_none(),
        "in its place"
    );
    f.harness.get_by_label("Drums").click();
    f.harness.run_steps(2);
    f.harness.get_by_label("Snare.wav");
    // A click plays it: read in the background, then heard.
    f.harness.get_by_label("Kick.wav").click();
    f.harness.run_steps(2);
    let kick = folder.join("Drums").join("Kick.wav");
    assert_eq!(f.session().previewing(), Some(kick.as_path()));
    let start = std::time::Instant::now();
    while loudest(&mut f) < 0.1 {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(20),
            "never heard"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        f.harness.run_steps(1);
    }
    // Clicked again, it stops.
    f.harness.get_by_label("Kick.wav").click();
    f.harness.run_steps(2);
    assert!(f.session().previewing().is_none());
    assert!(loudest(&mut f) < 1e-6);
    // Back up to the top.
    f.harness.get_by_label(text::LIBRARY_UP).click();
    f.harness.run_steps(2);
    f.harness.get_by_label("Loops");
}

/// A library folder's button in the folders column, by part of its name.
fn library_folder<'a>(f: &'a Fixture, name: &'a str) -> egui_kittest::Node<'a> {
    f.harness.get_by(move |node| {
        node.role() == egui::accesskit::Role::Button
            && node.label().is_some_and(|label| label.contains(name))
    })
}

#[test]
fn a_second_library_folder_opens_beside_the_first() {
    let mut f = new_song();
    let first = sound_folder();
    open_library(&mut f, &first);
    // A second folder, added the same way, opens at once (this crashed
    // the program in 1.0: it showed the new folder before listing it).
    let second = first.with_file_name(
        first
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace("library", "second"),
    );
    std::fs::create_dir_all(&second).unwrap();
    std::fs::copy(tone_file("tone.wav", 0.5), second.join("Bass.wav")).unwrap();
    *f.host.folder.lock().unwrap() = Some(second.clone());
    f.harness.get_by_label(text::ADD_SOUND_FOLDER).click();
    f.harness.run_steps(2);
    wait_for_label(&mut f, "Bass.wav");
    assert_eq!(
        f.workspace().settings().sound_folders,
        vec![first.clone(), second.clone()]
    );
    assert!(f.harness.query_by_label("Loops").is_none());
    // And a third, then back to the first.
    let third = first.with_file_name(
        first
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace("library", "third"),
    );
    std::fs::create_dir_all(&third).unwrap();
    std::fs::copy(tone_file("tone.wav", 0.5), third.join("Pad.wav")).unwrap();
    *f.host.folder.lock().unwrap() = Some(third.clone());
    f.harness.get_by_label(text::ADD_SOUND_FOLDER).click();
    f.harness.run_steps(2);
    wait_for_label(&mut f, "Pad.wav");
    assert_eq!(f.workspace().settings().sound_folders.len(), 3);
    let name = first.file_name().unwrap().to_string_lossy().into_owned();
    library_folder(&f, &name).click();
    f.harness.run_steps(2);
    wait_for_label(&mut f, "Loops");
    // Removing the second folder keeps the first one shown.
    let second_name = second.file_name().unwrap().to_string_lossy().into_owned();
    library_folder(&f, &second_name).click_secondary();
    f.harness.run_steps(4);
    f.harness.get_by_label(text::REMOVE_SOUND_FOLDER).click();
    f.harness.run_steps(4);
    assert_eq!(
        f.workspace().settings().sound_folders,
        vec![first.clone(), third.clone()]
    );
    f.harness.get_by_label("Loops");
    // Removing the folder shown shows the one before it.
    library_folder(&f, &third.file_name().unwrap().to_string_lossy()).click();
    f.harness.run_steps(2);
    wait_for_label(&mut f, "Pad.wav");
    library_folder(&f, &third.file_name().unwrap().to_string_lossy()).click_secondary();
    f.harness.run_steps(4);
    f.harness.get_by_label(text::REMOVE_SOUND_FOLDER).click();
    f.harness.run_steps(4);
    assert_eq!(f.workspace().settings().sound_folders, vec![first.clone()]);
    wait_for_label(&mut f, "Loops");
}

#[test]
fn searching_the_library_looks_in_every_folder() {
    let mut f = new_song();
    let folder = sound_folder();
    open_library(&mut f, &folder);
    f.harness
        .get_by_role(egui::accesskit::Role::TextInput)
        .click();
    f.harness.run_steps(2);
    f.harness.event(Event::Text("snare".to_owned()));
    f.harness.run_steps(2);
    f.harness.get_by_label("Snare.wav");
    assert!(f.harness.query_by_label("Kick.wav").is_none());
    assert!(f.harness.query_by_label("Loops").is_none());
}

#[test]
fn a_sound_dragged_from_the_library_onto_the_song_lands_where_it_is_dropped() {
    let mut f = new_song();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    let folder = sound_folder();
    open_library(&mut f, &folder);
    f.harness.get_by_label("Loops").click();
    f.harness.run_steps(2);
    let from = f.harness.get_by_label("Groove.wav").rect().center();
    let tracks = f.session().project().tracks.len();
    // Below the tracks, at bar 2.
    let to = Pos2::new(f.song_x(Ticks::WHOLE) + 2.0, f.row_top(tracks) + 20.0);
    f.drag(from, to, Modifiers::NONE);
    f.wait_for_audio();
    let last = f.session().project().tracks.last().unwrap();
    assert_eq!(f.session().project().tracks.len(), tracks + 1);
    assert!(last.instrument.is_audio());
    assert_eq!(last.name, "Groove");
    assert_eq!(last.clips.len(), 1);
    assert_eq!(
        last.clips[0].start,
        Ticks::WHOLE,
        "on the beat it was dropped on"
    );
}

#[test]
fn a_soundfont_from_the_library_becomes_an_instrument_with_its_presets() {
    let mut f = new_song();
    let folder = sound_folder();
    open_library(&mut f, &folder);
    let row = f.harness.get_by_label("Tones.sf2").rect().center();
    f.right_click(row);
    f.harness.get_by_label(text::ADD_TO_SONG).click();
    f.harness.run_steps(3);
    let track = f.session().project().tracks.last().unwrap().clone();
    assert_eq!(track.name, "Tones");
    let file = folder.join("Tones.sf2").to_string_lossy().into_owned();
    assert_eq!(
        track.instrument,
        daw_model::Instrument::SoundFont {
            source: daw_model::SoundSource::File(file),
            bank: 0,
            program: 0,
        }
    );
    f.harness.get_by_label(&text::soundfont_track("Tones.sf2"));
    // Its presets, in the track menu of its pattern.
    f.workspace_mut().session_mut().open_track(track.id);
    f.harness.run_steps(3);
    f.harness
        .get_by(|node| {
            node.role() == egui::accesskit::Role::Button
                && node.label().is_some_and(|label| label.trim() == "Tones")
        })
        .click();
    f.harness.run_steps(2);
    f.harness.get_by_label("0:005  Second sine").click();
    f.harness.run_steps(2);
    assert_eq!(f.session().soundfont_preset(track.id), Some((0, 5)));
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_SOUNDFONT_PRESET)
    );
    assert!(
        f.harness
            .query_by(|node| node
                .label()
                .is_some_and(|l| l.contains("Tones") && l.contains("plain sound")))
            .is_none(),
        "the SoundFont plays (no plain sound in its place)"
    );
}

#[test]
fn a_folder_dropped_on_the_window_joins_the_library() {
    let mut f = new_song();
    let folder = sound_folder();
    f.harness
        .input_mut()
        .dropped_files
        .push(Arc::new(DroppedSound(folder.clone())));
    f.harness.step();
    wait_for_label(&mut f, "Loops");
    assert_eq!(f.workspace().settings().sound_folders, vec![folder]);
    assert!(f.workspace().settings().show_library);
}

#[test]
fn the_instrument_chooser_adds_a_soundfont_track() {
    let mut f = new_song();
    let folder = sound_folder();
    *f.host.soundfont.lock().unwrap() = Some(folder.join("Tones.sf2"));
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_INSTRUMENT))
        .click();
    f.harness.run_steps(3);
    f.harness
        .get_by_label_contains(text::ADD_SOUNDFONT_HINT)
        .click();
    f.harness.run_steps(3);
    let last = f.session().project().tracks.last().unwrap();
    assert_eq!(last.name, "Tones");
    assert!(matches!(
        last.instrument,
        daw_model::Instrument::SoundFont {
            source: daw_model::SoundSource::File(_),
            ..
        }
    ));
}

/// A folder holding an SFZ instrument: its file, and its sound in a folder
/// it names with other letter cases (as instruments made on Windows do).
fn instrument_folder() -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "interface-instruments-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let sound = folder.join("Strings").join("Samples").join("A4.wav");
    std::fs::create_dir_all(sound.parent().unwrap()).unwrap();
    std::fs::copy(tone_file("a4.wav", 1.0), &sound).unwrap();
    std::fs::write(
        folder.join("Strings").join("Strings.sfz"),
        "// A test instrument.\n<region> sample=samples\\a4.wav loop_mode=loop_continuous amplfo_depth=2\n",
    )
    .unwrap();
    folder
}

/// Draws frames until the status line says something containing `part`.
fn wait_for_status(f: &mut Fixture, part: &str) {
    let start = std::time::Instant::now();
    while !f.session().status().is_some_and(|s| s.contains(part)) {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(20),
            "the status line never said {part}: it says {:?}",
            f.session().status()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        f.harness.run_steps(1);
    }
}

#[test]
fn an_sfz_instrument_from_the_library_plays_once_its_sounds_are_read() {
    let mut f = new_song();
    let folder = instrument_folder();
    *f.host.folder.lock().unwrap() = Some(folder.clone());
    f.harness.get_by_label(text::SHOW_LIBRARY).click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::ADD_SOUND_FOLDER).click();
    f.harness.run_steps(2);
    wait_for_label(&mut f, "Strings");
    f.harness.get_by_label("Strings").click();
    f.harness.run_steps(2);
    let row = f.harness.get_by_label("Strings.sfz").rect().center();
    f.right_click(row);
    f.harness.get_by_label(text::ADD_TO_SONG).click();
    f.harness.run_steps(3);
    let track = f.session().project().tracks.last().unwrap().clone();
    assert_eq!(track.name, "Strings");
    let file = folder
        .join("Strings")
        .join("Strings.sfz")
        .to_string_lossy()
        .into_owned();
    assert_eq!(track.instrument, daw_model::Instrument::Sfz { file });
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_ADD_SFZ)
    );

    // Read in the background; once ready, the status line says what it
    // plays without, and its track says what it plays.
    wait_for_status(&mut f, "plays without LFOs");
    assert!(!f.session().is_reading_instruments());
    f.harness
        .get_by_label(&text::sfz_track("Strings.sfz", false));
    assert!(
        f.harness
            .query_by(|node| node.label().is_some_and(|l| l.contains("plain sound")))
            .is_none(),
        "its sound was found (no plain sound in its place)"
    );
    // Played from the computer keyboard: its own sound, looped.
    f.key(Key::Q, Key::Q, true, Modifiers::NONE);
    let start = std::time::Instant::now();
    while loudest(&mut f) < 0.1 {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(20),
            "never heard"
        );
        f.harness.run_steps(1);
    }
    f.key(Key::Q, Key::Q, false, Modifiers::NONE);
}

#[test]
fn the_instrument_chooser_adds_an_sfz_track() {
    let mut f = new_song();
    let folder = instrument_folder();
    *f.host.soundfont.lock().unwrap() = Some(folder.join("Strings").join("Strings.sfz"));
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_INSTRUMENT))
        .click();
    f.harness.run_steps(3);
    f.harness
        .get_by_label_contains(text::ADD_SOUNDFONT_HINT)
        .click();
    f.harness.run_steps(3);
    let last = f.session().project().tracks.last().unwrap();
    assert_eq!(last.name, "Strings");
    assert!(
        last.instrument
            .sfz_file()
            .is_some_and(|f| f.ends_with("Strings.sfz"))
    );
}

/// Two short sounds for a custom kit, and a file that is not a sound.
fn kit_sounds() -> Vec<PathBuf> {
    let folder = std::env::temp_dir().join(format!(
        "interface-kit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&folder).unwrap();
    for name in ["Boom.wav", "Clap.wav"] {
        std::fs::copy(tone_file("kit.wav", 0.3), folder.join(name)).unwrap();
    }
    std::fs::write(folder.join("notes.txt"), b"not a sound").unwrap();
    ["Boom.wav", "Clap.wav", "notes.txt"]
        .iter()
        .map(|n| folder.join(n))
        .collect()
}

/// Draws frames until a track's custom kit has `count` pads (its sounds are
/// read in the background).
fn wait_for_pads(f: &mut Fixture, track: daw_model::TrackId, count: usize) {
    let start = std::time::Instant::now();
    while f.session().kit_pads(track).len() != count {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(20),
            "the kit has {} pads, not {count}",
            f.session().kit_pads(track).len()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        f.harness.run_steps(1);
    }
}

#[test]
fn a_custom_kit_is_made_of_chosen_sounds_and_played_from_the_step_grid() {
    let mut f = new_song();
    let sounds = kit_sounds();
    *f.host.kit_sounds.lock().unwrap() = sounds.clone();
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_INSTRUMENT))
        .click();
    f.harness.run_steps(3);
    f.harness.get_by_label_contains(text::ADD_KIT_HINT).click();
    f.harness.run_steps(3);
    let kit = f.session().focused_track();
    assert!(f.session().is_custom_kit(kit), "the kit's beat is open");
    wait_for_pads(&mut f, kit, 2);
    let pads: Vec<(String, u8)> = f
        .session()
        .kit_pads(kit)
        .iter()
        .map(|p| (p.name.clone(), p.setting.key))
        .collect();
    assert_eq!(
        pads,
        [("Boom".to_owned(), 36), ("Clap".to_owned(), 37)],
        "each sound on its own key; the text file is left out"
    );
    assert_eq!(
        f.session().project().audio.len(),
        2,
        "the sounds are in the song"
    );

    // A row for each sound: a click on its first square puts a hit there.
    f.harness.get_by_label("Boom, step 1").click();
    f.harness.run_steps(2);
    let keys: Vec<u8> = f.session().pattern().notes.iter().map(|n| n.key).collect();
    assert_eq!(keys, [36]);
    // The first key of the computer keyboard plays the first sound.
    f.key(Key::Z, Key::Z, true, Modifiers::NONE);
    let start = std::time::Instant::now();
    while loudest(&mut f) < 0.05 {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(20),
            "never heard"
        );
        f.harness.run_steps(1);
    }
    f.key(Key::Z, Key::Z, false, Modifiers::NONE);

    // A sound leaves the kit from its settings, and comes back with undo.
    f.harness
        .get_by(|node| {
            node.role() == egui::accesskit::Role::Button && node.label().as_deref() == Some("Clap")
        })
        .click();
    f.harness.run_steps(2);
    f.harness.get_by_label(text::REMOVE_PAD).click();
    f.harness.run_steps(2);
    assert_eq!(f.session().kit_pads(kit).len(), 1);
    assert_eq!(
        f.session().document().undo_label(),
        Some(text::EDIT_REMOVE_PAD)
    );
    f.workspace_mut().session_mut().undo();
    f.harness.run_steps(2);
    assert_eq!(f.session().kit_pads(kit).len(), 2);

    // Sounds dropped on the kit's beat join it.
    f.harness
        .input_mut()
        .dropped_files
        .push(Arc::new(DroppedSound(sounds[0].clone())));
    f.harness.step();
    wait_for_pads(&mut f, kit, 3);
    assert_eq!(f.session().kit_pads(kit)[2].setting.key, 38);
}

#[test]
fn a_library_folder_becomes_a_custom_kit() {
    let mut f = beat();
    let folder = sound_folder();
    open_library(&mut f, &folder);
    let row = f.harness.get_by_label("Drums").rect().center();
    f.right_click(row);
    f.harness.get_by_label(text::MAKE_KIT).click();
    f.harness.run_steps(3);
    let kit = f.session().focused_track();
    assert!(f.session().is_custom_kit(kit));
    wait_for_pads(&mut f, kit, 2);
    let track = f.session().project().track(kit).unwrap();
    assert_eq!(track.name, "Drums 2", "named after the folder");
    let names: Vec<&str> = track
        .instrument
        .kit_pads()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["Kick", "Snare"]);
}

// ----- Singing voices (decision 0027)

/// A fresh folder of this test run's own.
fn fresh_folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "interface-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

/// A voicebank made for the tests in `folder`: one recording of a vowel,
/// named "- a", "a", "ka" and "a ka" (plain letters, which the interface's
/// own fonts draw), a `character.txt` and a readme.
fn test_voicebank(folder: &std::path::Path) {
    std::fs::create_dir_all(folder).unwrap();
    let rate = 22_050_u32;
    let samples: Vec<f32> = (0..rate)
        .map(|i| {
            (0.3 * (std::f64::consts::TAU * 220.0 * f64::from(i) / f64::from(rate)).sin()) as f32
        })
        .collect();
    let mut writer = daw_formats::wav::WavWriter::create_with_channels(
        &folder.join("vowel.wav"),
        daw_formats::wav::WavFormat::Pcm16,
        rate,
        1,
    )
    .unwrap();
    writer.write_interleaved(&samples).unwrap();
    writer.finish().unwrap();
    std::fs::write(
        folder.join("oto.ini"),
        "vowel.wav=- a,20,30,-900,40,10\nvowel.wav=a,20,30,-900,40,10\n\
         vowel.wav=ka,20,30,-900,40,10\nvowel.wav=a ka,20,30,-900,40,10\n",
    )
    .unwrap();
    std::fs::write(
        folder.join("character.txt"),
        "name=Test Voice\nauthor=Tester\n",
    )
    .unwrap();
    std::fs::write(folder.join("readme.txt"), "Free to use in tests.\n").unwrap();
}

#[test]
fn a_voice_track_sings_lyrics_typed_on_its_notes() {
    let folder = fresh_folder("voice-track");
    let root = folder.join("voicebanks");
    test_voicebank(&root.join("Test Voice"));
    let mut f = new_song();
    f.workspace_mut().set_voicebanks_folder(Some(root));
    // The instrument chooser offers it among the voices.
    f.harness.get_by_label(text::UNTITLED).click();
    f.harness.run_steps(2);
    f.harness
        .get_by_label(&format!("+ {}", text::ADD_INSTRUMENT))
        .click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::VOICES);
    // (The chooser, taller with its voices, settles before the click.)
    f.harness.run_steps(8);
    f.harness.get_by_label_contains("Test Voice").click();
    f.harness.run_steps(3);
    assert_eq!(f.session().voicebank(), Some("Test Voice"));
    let track = f.session().track_id();
    f.workspace_mut().session_mut().open_track(track);
    f.harness.run_steps(3);
    let ids: Vec<daw_model::NoteId> = (0..3_u8)
        .map(|i| {
            f.workspace_mut()
                .session_mut()
                .add_note(Ticks::QUARTER * i64::from(i), 60 + 2 * i, 0.8)
                .unwrap()
        })
        .collect();
    f.workspace_mut().session_mut().select_only([ids[0]]);
    f.harness.run_steps(2);
    // Enter types the selected note's lyric; words after a space go to the
    // next notes.
    f.key(Key::Enter, Key::Enter, true, Modifiers::NONE);
    f.key(Key::Enter, Key::Enter, false, Modifiers::NONE);
    f.harness.run_steps(2);
    f.harness.event(Event::Text("ka sa".to_owned()));
    f.harness.run_steps(1);
    f.key(Key::Enter, Key::Enter, true, Modifiers::NONE);
    f.key(Key::Enter, Key::Enter, false, Modifiers::NONE);
    f.harness.run_steps(2);
    let pattern = f.session().pattern();
    assert_eq!(pattern.lyric(ids[0]), Some("ka"));
    assert_eq!(pattern.lyric(ids[1]), Some("sa"));
    assert_eq!(
        pattern.lyric(ids[2]),
        None,
        "the third note still sings \"a\""
    );
    // The voicebank has "ka" but not "sa" (which sings "a"): marked.
    assert!(f.workspace_mut().session_mut().lyric_found(ids[0]));
    assert!(!f.workspace_mut().session_mut().lyric_found(ids[1]));
    assert!(f.workspace_mut().session_mut().lyric_found(ids[2]));
    // The phrase was one step: one undo takes it back.
    f.workspace_mut().session_mut().undo();
    assert!(f.session().pattern().lyrics.is_empty());
    // Escape leaves a lyric as it was.
    f.key(Key::Enter, Key::Enter, true, Modifiers::NONE);
    f.key(Key::Enter, Key::Enter, false, Modifiers::NONE);
    f.harness.run_steps(2);
    f.harness.event(Event::Text("ne".to_owned()));
    f.harness.run_steps(1);
    f.key(Key::Escape, Key::Escape, true, Modifiers::NONE);
    f.key(Key::Escape, Key::Escape, false, Modifiers::NONE);
    f.harness.run_steps(2);
    assert!(
        f.session().pattern().lyrics.is_empty(),
        "nothing typed was kept"
    );
    std::fs::remove_dir_all(folder).ok();
}

#[test]
fn the_voicebanks_window_adds_one_shows_its_terms_and_removes_it() {
    let folder = fresh_folder("voicebanks");
    let source = folder.join("downloaded").join("Test Voice");
    test_voicebank(&source);
    let root = folder.join("voicebanks");
    let mut f = new_song();
    f.workspace_mut().set_voicebanks_folder(Some(root.clone()));
    *f.host.folder.lock().unwrap() = Some(source);
    f.workspace_mut().open_voicebanks_window();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::VOICEBANKS_TITLE);
    f.harness.get_by_label(text::NO_VOICEBANKS);
    f.harness.get_by_label(text::ADD_VOICEBANK_FOLDER).click();
    // It is copied on a thread of its own.
    for _ in 0..200 {
        f.harness.run_steps(1);
        if f.harness
            .query_by_label(&text::voicebank_added("Test Voice"))
            .is_some()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    f.harness.get_by_label(&text::voicebank_added("Test Voice"));
    // Its terms are shown, with a reminder to read them.
    f.harness.get_by_label(text::READ_THE_TERMS);
    f.harness.get_by_label_contains("Free to use in tests.");
    assert!(root.join("Test Voice").join("oto.ini").is_file());
    // Removed, it is gone.
    // (The window grew with the terms: it settles before the click.)
    f.harness.run_steps(8);
    f.harness.get_by_label(text::REMOVE_VOICEBANK).click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::NO_VOICEBANKS);
    assert!(!root.join("Test Voice").exists());
    f.harness.get_by_label(text::CLOSE).click();
    f.harness.run_steps(3);
    assert!(f.harness.query_by_label(text::VOICEBANKS_TITLE).is_none());
    std::fs::remove_dir_all(folder).ok();
}

// ----- Singing expression (decision 0028)

/// A new song with a voice track open on its pattern, singing C4 then E4,
/// a half note each (a second each at 120 BPM), in the Pitch mode.
fn singing() -> (Fixture, PathBuf, [daw_model::NoteId; 2]) {
    let folder = fresh_folder("singing");
    let root = folder.join("voicebanks");
    test_voicebank(&root.join("Test Voice"));
    let mut f = new_song();
    f.workspace_mut().set_voicebanks_folder(Some(root));
    let track = f
        .workspace_mut()
        .session_mut()
        .add_voice_track("Test Voice", "Test Voice");
    f.workspace_mut().session_mut().open_track(track);
    f.workspace_mut().session_mut().note_length = Ticks::QUARTER * 2;
    let ids = [(Ticks::ZERO, 60), (Ticks::QUARTER * 2, 64)].map(|(at, key)| {
        f.workspace_mut()
            .session_mut()
            .add_note(at, key, 0.8)
            .unwrap()
    });
    f.harness.run_steps(3);
    // The location line offers the switch where Draw is on other tracks.
    assert!(f.harness.query_by_label(text::DRAW).is_none());
    f.harness.get_by_label(text::EDIT_THE_PITCH).click();
    f.harness.run_steps(3);
    assert!(f.session().pitch_mode());
    (f, folder, ids)
}

impl Fixture {
    /// The middle of a handle on the sung pitch, found by its label.
    fn handle(&self, label: &str) -> Pos2 {
        self.harness.get_by_label(label).rect().center()
    }

    fn expression(&self, note: daw_model::NoteId) -> daw_model::expression::NoteExpression {
        self.session().note_expression(note)
    }
}

#[test]
fn the_pitch_mode_bends_the_sung_pitch_where_it_is_clicked() {
    let (mut f, folder, [c4, e4]) = singing();
    let notes = f.notes();
    // Each note shows its plain glide's two points.
    let leaves = f.handle(&text::pitch_point(64, 0));
    let arrives = f.handle(&text::pitch_point(64, 1));
    assert!(leaves.x < arrives.x);
    assert!(
        leaves.y > arrives.y,
        "E4's glide leaves from C4, lower on screen"
    );
    // A click over E4, two rows above it, bends the voice up there.
    let spot = arrives + vec2(60.0, -32.0);
    f.click(spot, Modifiers::NONE);
    let points = f.expression(e4).pitch;
    assert_eq!(points.len(), 3, "{points:?}");
    assert!((points[2].cents - 200.0).abs() < 20.0, "{points:?}");
    assert!(points[2].ms > 100.0, "{points:?}");
    assert_eq!(f.notes(), notes, "the notes stay where they are");
    assert!(f.session().is_selected(e4));
    // One undo step takes it away.
    f.workspace_mut().session_mut().undo();
    assert!(f.expression(e4).pitch.is_empty());
    f.click(spot, Modifiers::NONE);
    // A point is dragged in time and pitch: the arriving one, a row up.
    let arrives = f.handle(&text::pitch_point(64, 1));
    f.drag(arrives, arrives + vec2(0.0, -16.0), Modifiers::NONE);
    let points = f.expression(e4).pitch;
    assert!((points[1].cents - 100.0).abs() < 15.0, "{points:?}");
    // The point leaving from C4 moves in time only.
    let leaves = f.handle(&text::pitch_point(64, 0));
    f.drag(leaves, leaves + vec2(-10.0, -40.0), Modifiers::NONE);
    let points = f.expression(e4).pitch;
    assert!(points[0].ms < -50.0, "earlier: {points:?}");
    assert!(points[0].cents.abs() < 1e-3, "{points:?}");
    // A right-click removes a point.
    let third = f.handle(&text::pitch_point(64, 2));
    f.right_click(third);
    assert_eq!(f.expression(e4).pitch.len(), 2);
    // Clicking a point opens its panel: the shape of the curve after it,
    // and Remove. The glide's shape is its leaving point's.
    let leaves = f.handle(&text::pitch_point(64, 0));
    f.click(leaves, Modifiers::NONE);
    f.harness.run_steps(8);
    f.harness.get_by_label(text::POINT_LEAVES);
    // (Clicked slowly, as a person does: the panel stays open for it.)
    let straight = f.handle(&text::curve_shape(text::SHAPE_STRAIGHT));
    f.click(straight, Modifiers::NONE);
    assert_eq!(
        f.expression(e4).pitch[0].shape,
        daw_model::expression::PitchShape::Straight
    );
    f.harness.get_by_label(text::REMOVE_POINT).click();
    f.harness.run_steps(3);
    assert!(
        f.expression(e4).pitch.is_empty(),
        "with one point left, E4 glides the plain way"
    );
    assert!(f.expression(c4).is_plain());
    // Ctrl+P goes back to the notes.
    f.key(Key::P, Key::P, true, Modifiers::CTRL);
    f.key(Key::P, Key::P, false, Modifiers::CTRL);
    assert!(!f.session().pitch_mode());
    assert!(
        f.harness
            .query_by_label(&text::pitch_point(64, 0))
            .is_none()
    );
    std::fs::remove_dir_all(folder).ok();
}

#[test]
fn vibrato_colour_and_breath_shape_the_selected_notes() {
    let (mut f, folder, [c4, e4]) = singing();
    // The notes menu gives every selected note a vibrato.
    f.workspace_mut().session_mut().select_all();
    f.harness.get_by_label_contains(text::NOTES_MENU).click();
    f.harness.run_steps(3);
    f.harness.get_by_label(text::ADD_VIBRATO).click();
    f.harness.run_steps(3);
    for note in [c4, e4] {
        assert_eq!(
            f.expression(note).vibrato,
            Some(daw_model::expression::Vibrato::default())
        );
    }
    // Its handle sits above the note by its depth: dragged up a row, it
    // swings a semitone more.
    let handle = f.handle(&text::vibrato_handle(60));
    f.drag(handle, handle + vec2(0.0, -16.0), Modifiers::NONE);
    let depth = f.expression(c4).vibrato.unwrap().depth;
    assert!((depth - 135.0).abs() < 15.0, "{depth}");
    // Clicked, it opens its panel; Remove takes it away.
    let handle = f.handle(&text::vibrato_handle(60));
    f.click(handle, Modifiers::NONE);
    f.harness.run_steps(8);
    f.harness.get_by_label(text::VIBRATO_RATE);
    let remove = f.handle(text::REMOVE_THE_VIBRATO);
    f.click(remove, Modifiers::NONE);
    assert_eq!(f.expression(c4).vibrato, None);
    assert!(f.expression(e4).vibrato.is_some(), "E4 keeps its own");
    // The lane under a voice's notes shows each note's colour and breath,
    // beside velocity and tempo: all four fit beside the lane.
    for lane in [
        text::LANE_VELOCITY,
        text::LANE_COLOUR,
        text::LANE_BREATH,
        text::LANE_TEMPO,
    ] {
        let button = f.harness.get_by_label(lane).rect();
        assert!(
            button.bottom() <= f.roll().bottom() && button.height() >= 14.0,
            "{lane}: {button:?} in {:?}",
            f.roll()
        );
    }
    f.harness.get_by_label(text::LANE_COLOUR).click();
    f.harness.run_steps(2);
    let lane_bottom = f.roll().bottom() - 4.0;
    let usable = 104.0 - 10.0;
    // C4's bar stands at its start: half-way between its glide's points.
    let start_x = f32::midpoint(
        f.handle(&text::pitch_point(60, 0)).x,
        f.handle(&text::pitch_point(60, 1)).x,
    );
    let middle = Pos2::new(start_x + 1.0, lane_bottom - usable * 0.5);
    f.workspace_mut().session_mut().select_only([c4]);
    f.drag(middle, middle - vec2(0.0, usable * 0.3), Modifiers::NONE);
    let colour = f.expression(c4).colour;
    assert!((colour - 0.6).abs() < 0.1, "lighter: {colour}");
    assert_eq!(f.expression(e4).colour, 0.0, "only the selected note");
    f.harness.get_by_label(text::LANE_BREATH).click();
    f.harness.run_steps(2);
    let bottom = Pos2::new(start_x + 1.0, lane_bottom - 1.0);
    f.drag(bottom, bottom - vec2(0.0, usable * 0.5), Modifiers::NONE);
    let breath = f.expression(c4).breath;
    assert!((breath - 0.5).abs() < 0.1, "breathier: {breath}");
    // Each drag was one step.
    f.workspace_mut().session_mut().undo();
    assert_eq!(f.expression(c4).breath, 0.0);
    assert!((f.expression(c4).colour - colour).abs() < 1e-6);
    std::fs::remove_dir_all(folder).ok();
}

#[test]
fn copied_notes_keep_their_lyrics_and_expression() {
    let (mut f, folder, [c4, e4]) = singing();
    let bent = daw_model::expression::NoteExpression {
        pitch: vec![
            daw_model::expression::PitchPoint::new(-40.0, 0.0),
            daw_model::expression::PitchPoint::new(80.0, 30.0),
        ],
        breath: 0.4,
        ..daw_model::expression::NoteExpression::default()
    };
    {
        let session = f.workspace_mut().session_mut();
        session.sing_from(c4, "ka sa");
        session.select_only([e4]);
        session.change_expression("test", &[e4], |_, e| *e = bent.clone());
        session.select_all();
    }
    // Copied through the clipboard's text and pasted back.
    f.harness.event(Event::Copy);
    f.harness.step();
    let copied = f
        .harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .expect("copying puts text on the clipboard");
    f.harness.event(Event::Paste(copied));
    f.harness.run_steps(2);
    let pattern = f.session().pattern().clone();
    assert_eq!(pattern.notes.len(), 4);
    let sung: Vec<(&str, bool)> = pattern
        .notes
        .iter()
        .map(|n| {
            (
                pattern.lyric(n.id).unwrap_or(""),
                pattern.expression(n.id) == Some(&bent),
            )
        })
        .collect();
    assert_eq!(
        sung,
        [("ka", false), ("sa", true), ("ka", false), ("sa", true)]
    );
    // One undo takes the pasted notes, lyrics and all.
    f.workspace_mut().session_mut().undo();
    assert_eq!(f.session().pattern().lyrics.len(), 2);
    assert_eq!(f.session().pattern().expression.len(), 1);
    // Duplicated notes keep them too.
    f.workspace_mut().session_mut().select_only([e4]);
    f.key(Key::D, Key::D, true, Modifiers::CTRL);
    f.key(Key::D, Key::D, false, Modifiers::CTRL);
    let pattern = f.session().pattern().clone();
    let copy = pattern
        .notes
        .iter()
        .find(|n| n.id != c4 && n.id != e4)
        .unwrap();
    assert_eq!(pattern.lyric(copy.id), Some("sa"));
    assert_eq!(pattern.expression(copy.id), Some(&bent));
    // Notes copied by an older copy of the program paste, singing "a".
    let old = format!(
        "{} notes 1\n[{{\"id\":1,\"start\":0,\"length\":{},\"key\":67,\"velocity\":0.8}}]",
        daw_ident::PRODUCT_NAME,
        Ticks::QUARTER.0
    );
    f.harness.event(Event::Paste(old));
    f.harness.run_steps(2);
    let pattern = f.session().pattern().clone();
    let pasted = pattern.notes.iter().find(|n| n.key == 67).unwrap();
    assert_eq!(pattern.lyric(pasted.id), None);
    std::fs::remove_dir_all(folder).ok();
}

// ----- A voicebank's picture (decision 0029)

#[test]
fn a_voice_s_picture_shows_faintly_on_its_track_and_behind_its_notes() {
    let folder = fresh_folder("voice-picture");
    let root = folder.join("voicebanks");
    let voice = root.join("Test Voice");
    test_voicebank(&voice);
    image::RgbaImage::from_pixel(100, 100, image::Rgba([200, 120, 160, 255]))
        .save(voice.join("icon.png"))
        .unwrap();
    std::fs::write(
        voice.join("character.txt"),
        "name=Test Voice\nauthor=Tester\nimage=icon.png\n",
    )
    .unwrap();
    // A second voicebank, without a picture.
    test_voicebank(&root.join("Plain Voice"));
    let mut f = new_song();
    f.workspace_mut().set_voicebanks_folder(Some(root));
    let track = f
        .workspace_mut()
        .session_mut()
        .add_voice_track("Test Voice", "Test Voice");
    f.workspace_mut()
        .session_mut()
        .add_voice_track("Plain Voice", "Plain Voice");
    // Pictures are read in the background: wait (two seconds at most) until
    // `count` of them show.
    let label = text::voicebank_picture("Test Voice");
    let wait_for = |f: &mut Fixture, count: usize| {
        for _ in 0..200 {
            f.harness.run_steps(1);
            if f.harness.query_all_by_label(&label).count() >= count {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    wait_for(&mut f, 2);
    // In the song view: on the track's header and at the start of its clip.
    assert_eq!(f.harness.query_all_by_label(&label).count(), 2);
    assert!(
        f.harness
            .query_by_label(&text::voicebank_picture("Plain Voice"))
            .is_none(),
        "a voicebank without a picture shows none"
    );
    // Behind the piano roll's notes, at the bottom right of the grid, a
    // 100-point icon at most two and a half times its size.
    // The piano roll asks for the large picture, read anew the first time.
    f.workspace_mut().session_mut().open_track(track);
    wait_for(&mut f, 1);
    let picture = f.harness.get_by_label(&label).rect();
    let roll = f.roll();
    assert!(roll.contains_rect(picture), "{picture:?} in {roll:?}");
    assert!(picture.right() > roll.right() - 40.0, "at the right");
    assert!(
        picture.width() <= 250.5 && picture.width() > 100.0,
        "{picture:?}"
    );
    std::fs::remove_dir_all(folder).ok();
}
