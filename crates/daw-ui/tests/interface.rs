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
    Host, KeyLabels, LayoutFamily, PluginSearch, Session, Settings, View, Workspace, text,
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

    fn choose_export_file(&mut self, _suggested_name: &str) -> Option<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        self.export_to.lock().unwrap().clone()
    }

    fn choose_audio_files(&mut self) -> Vec<PathBuf> {
        *self.asked.lock().unwrap() += 1;
        self.audio.lock().unwrap().clone()
    }
}

struct Fixture {
    harness: Harness<'static, Option<Workspace>>,
    processor: EngineProcessor,
    host: FakeHost,
}

/// A new song, open on its piano pattern (the phase 1 tests work there).
fn fixture() -> Fixture {
    let mut f = new_song();
    let session = f.harness.state_mut().as_mut().unwrap().session_mut();
    let piano = session
        .project()
        .tracks
        .iter()
        .find(|t| !t.instrument.is_drums())
        .unwrap()
        .id;
    session.open_track(piano);
    f.harness.run_steps(3);
    f
}

/// A new song as a new user sees it: open on the empty beat.
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
    let (engine, processor) = Engine::new(EngineConfig::default(), SoundLibrary::new());
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
fn a_new_song_opens_on_the_step_grid_and_squares_make_a_beat() {
    let mut f = new_song();
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
    let mut f = new_song();
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
    let mut f = new_song();
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
    let mut f = new_song();
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
    let mut f = new_song();
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
    let mut f = new_song();
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
    let mut f = new_song();
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
    let mut f = new_song();
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

#[test]
fn panels_and_menus_take_clicks_that_last_several_frames() {
    // A person's click lasts longer than a frame: the press must not close
    // the panel or menu it lands in before the release.
    let mut f = new_song();
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
    f.harness
        .get_by_label(&text::synth_title(text::SYNTH_TRACK));
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
    assert!(
        f.harness
            .query_by_label(&text::synth_title(text::SYNTH_TRACK))
            .is_none()
    );
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
