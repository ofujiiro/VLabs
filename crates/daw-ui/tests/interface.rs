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

use daw_engine::{Engine, EngineConfig, EngineProcessor, SoundLibrary};
use daw_model::Ticks;
use daw_ui::{Host, KeyLabels, LayoutFamily, Session, Settings, Workspace, text};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// Answers file dialogs with prepared paths.
#[derive(Clone, Default)]
struct FakeHost {
    save_to: Arc<Mutex<Option<PathBuf>>>,
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
}

struct Fixture {
    harness: Harness<'static, Option<Workspace>>,
    processor: EngineProcessor,
    host: FakeHost,
}

fn fixture() -> Fixture {
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
