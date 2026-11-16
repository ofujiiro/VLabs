// SPDX-License-Identifier: GPL-3.0-or-later

//! Every piece of text the interface shows, in one place, so the program
//! can be translated later. English only for now.
//!
//! Text that names the product is built from `daw_ident` at run time.

#![allow(
    missing_docs,
    reason = "each constant is the text itself; a comment would repeat it"
)]

/// Window title: the song's name, a mark if it has unsaved changes, and the
/// product name.
pub fn window_title(song: &str, unsaved: bool) -> String {
    let mark = if unsaved { "• " } else { "" };
    format!("{mark}{song} — {}", daw_ident::PRODUCT_NAME)
}

/// The name of a song that has never been saved.
pub const UNTITLED: &str = "Untitled song";
/// The first pattern of a new song.
pub const FIRST_PATTERN: &str = "Pattern 1";
/// The first track of a new song.
pub const FIRST_TRACK: &str = "Piano";

pub const PLAY: &str = "Play";
pub const STOP: &str = "Stop";
pub const RECORD: &str = "Record notes";
pub const STOP_RECORDING: &str = "Stop recording";
pub const LOOP: &str = "Loop the pattern";
pub const METRONOME: &str = "Metronome";
pub const COUNT_IN: &str = "Count-in";
pub const QUANTIZE: &str = "Snap recorded notes";
pub const OFF: &str = "Off";
pub const TEMPO: &str = "Tempo (beats per minute). Drag up or down, or double-click to type.";
pub const TIME_SIGNATURE: &str = "Time signature";
pub const POSITION: &str = "Play position: bar . beat . sixteenth";
pub const UNDO: &str = "Undo";
pub const REDO: &str = "Redo";
pub const NOTHING_TO_UNDO: &str = "Nothing to undo";
pub const NOTHING_TO_REDO: &str = "Nothing to redo";
pub const MENU: &str = "Menu";
pub const NEW: &str = "New song";
pub const OPEN: &str = "Open…";
pub const SAVE: &str = "Save";
pub const SAVE_AS: &str = "Save as…";
pub const THEME: &str = "Theme";
pub const THEME_SYSTEM: &str = "Follow the system";
pub const THEME_LIGHT: &str = "Light";
pub const THEME_DARK: &str = "Dark";
pub const SHORTCUTS: &str = "Keyboard shortcuts";
pub const SHORTCUTS_HINT: &str = "Letters, digits and punctuation play notes, so shortcuts use Ctrl or Alt with them, or keys that never play notes.";
pub const PRESS_A_KEY: &str = "Press a key… (Esc to cancel)";
pub const NO_SHORTCUT: &str = "None";
pub const CHANGE: &str = "Change";
pub const RESET: &str = "Reset";
pub const RESET_ALL: &str = "Reset all";
pub const FIXED_SHORTCUT: &str = "Set by the system";
pub const CLOSE: &str = "Close";
pub const OPEN_RECENT: &str = "Open recent";
pub const KEY_LABELS: &str = "Keyboard labels";
pub const KEY_LABELS_HINT: &str =
    "Which letters the on-screen keyboard shows. Playing works the same on every keyboard.";
pub const KEY_LABELS_AUTOMATIC: &str = "Automatic";

pub fn shortcut_plays_notes(shortcut: &str) -> String {
    format!("{shortcut} plays a note. Add Ctrl or Alt, or choose a key that does not play notes.")
}

pub fn shortcut_in_use(shortcut: &str, other: &str) -> String {
    format!("{shortcut} is already used for \u{201c}{other}\u{201d}.")
}

/// The name of a command in the shortcuts window.
pub fn command_name(command: crate::keys::Command) -> &'static str {
    use crate::keys::Command as C;
    match command {
        C::PlayStop => "Play or stop",
        C::Record => "Record notes",
        C::ToStart => "Back to the start",
        C::Loop => "Loop on or off",
        C::Metronome => "Metronome on or off",
        C::Undo => "Undo",
        C::Redo => "Redo",
        C::New => "New song",
        C::Open => "Open a song",
        C::Save => "Save",
        C::SaveAs => "Save as",
        C::SelectAll => "Select all notes",
        C::Copy => "Copy",
        C::Cut => "Cut",
        C::Paste => "Paste",
        C::Duplicate => "Duplicate notes",
        C::Delete => "Delete notes",
        C::Deselect => "Clear the selection",
        C::MoveEarlier => "Move notes earlier",
        C::MoveLater => "Move notes later",
        C::TransposeUp => "Move notes up a semitone",
        C::TransposeDown => "Move notes down a semitone",
        C::OctaveUp => "Move notes up an octave",
        C::OctaveDown => "Move notes down an octave",
        C::KeyboardOctaveUp => "Computer keyboard an octave up",
        C::KeyboardOctaveDown => "Computer keyboard an octave down",
        C::ZoomIn => "Zoom in",
        C::ZoomOut => "Zoom out",
        C::ToggleKeyboard => "Show or hide the keyboard",
    }
}
pub const ABOUT: &str = "About";
pub const SNAP: &str = "Snap";
pub const SNAP_HINT: &str = "Notes snap to this grid. Hold Alt while dragging to place freely.";
pub const NOTE_LENGTH: &str = "New notes";
pub const KEYBOARD: &str = "Keyboard";
pub const ON_SCREEN_KEYBOARD: &str = "On-screen keyboard";
pub const KEYBOARD_HINT: &str = "Play with your computer keyboard: the two rows from Z and from Q are piano keys. Page Up and Page Down change the octave.";
pub const OCTAVE_DOWN: &str = "Octave down";
pub const OCTAVE_UP: &str = "Octave up";
pub const VELOCITY: &str = "Velocity";
pub const PIANO_ROLL: &str = "Piano roll";
pub const SHOW_KEYBOARD: &str = "Show the keyboard";
pub const HIDE_KEYBOARD: &str = "Hide the keyboard";

/// The hint shown in an empty piano roll.
pub const EMPTY_PATTERN_HINT: &str = "Click anywhere to place a note — press Space to hear it";
/// A second line under the hint.
pub const EMPTY_PATTERN_HINT_2: &str =
    "Or play the computer keyboard: Z S X D C V… and Q 2 W 3 E R…";

/// The start of the location line: "song  ›  track  ›" (the pattern's
/// name follows, larger).
pub fn location_prefix(song: &str, track: &str) -> String {
    format!("{song}  ›  {track}  ›")
}

pub fn undo_named(label: &str) -> String {
    format!("Undo {label}")
}

pub fn redo_named(label: &str) -> String {
    format!("Redo {label}")
}

pub fn with_shortcut(text: &str, shortcut: &str) -> String {
    if shortcut.is_empty() {
        text.to_owned()
    } else {
        format!("{text}  ({shortcut})")
    }
}

// Undo-step names (shown as "Undo …").
pub const EDIT_ADD_NOTE: &str = "add note";
pub const EDIT_MOVE_NOTES: &str = "move notes";
pub const EDIT_RESIZE_NOTES: &str = "change note length";
pub const EDIT_DELETE_NOTES: &str = "delete notes";
pub const EDIT_VELOCITY: &str = "change velocity";
pub const EDIT_PASTE: &str = "paste notes";
pub const EDIT_DUPLICATE: &str = "duplicate notes";
pub const EDIT_TRANSPOSE: &str = "transpose notes";
pub const EDIT_TEMPO: &str = "change tempo";
pub const EDIT_TIME_SIGNATURE: &str = "change time signature";
pub const EDIT_LOOP: &str = "change loop";
pub const EDIT_PATTERN_LENGTH: &str = "change pattern length";

pub fn edit_record(count: usize) -> String {
    if count == 1 {
        "record 1 note".to_owned()
    } else {
        format!("record {count} notes")
    }
}

/// The name of a grid choice.
pub fn grid_name(grid: crate::session::Grid) -> String {
    use crate::session::Grid;
    match grid {
        Grid::Bar => "Bar".to_owned(),
        Grid::Note(n) => note_value_name(n),
        Grid::Off => OFF.to_owned(),
    }
}

/// The name of a note value, 1/`n` of a whole note: "1/8", "1/8 triplet".
pub fn note_value_name(n: i64) -> String {
    if n % 3 == 0 {
        format!("1/{} triplet", n * 2 / 3)
    } else {
        format!("1/{n}")
    }
}

/// "1 bar", "4 bars".
pub fn bars(count: u32) -> String {
    if count == 1 {
        "1 bar".to_owned()
    } else {
        format!("{count} bars")
    }
}

pub const PATTERN_LENGTH: &str = "Length";
pub const LONGER: &str = "One bar longer";
pub const SHORTER: &str = "One bar shorter";
pub const CUSTOM_LENGTH: &str = "As drawn";
pub const NOTE_LENGTH_HINT: &str =
    "How long a new note is when you click to place it. Stretching a note changes it.";
pub const RECORDING_OPTIONS: &str = "Recording options";
pub const METRONOME_OPTIONS: &str = "Metronome options";
pub const PATTERN_LENGTH_HINT: &str =
    "How many bars the pattern lasts. It also grows by itself when notes go past its end.";
pub const COUNT_IN_HINT: &str = "Bars of clicks before recording starts, so you can find the beat.";
pub const METRONOME_HINT: &str = "Click on every beat while the song plays.";
pub const METRONOME_WHEN_RECORDING: &str = "Click while recording";
pub const METRONOME_VOLUME: &str = "Click volume";
pub const QUANTIZE_HINT: &str = "Move recorded notes onto the grid, so they land exactly in time.";
pub const BACK_TO_START: &str = "Back to the start";
pub const VELOCITY_LANE_HINT: &str = "How hard each note is played. Drag a bar up or down.";

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A key's name with its octave: 60 is "C4" (middle C).
pub fn note_name(key: u8) -> String {
    let octave = i32::from(key / 12) - 1;
    format!("{}{octave}", NOTE_NAMES[usize::from(key % 12)])
}

/// A note's name with how hard it is played, for the editor's corner.
pub fn note_details(key: u8, velocity: f32) -> String {
    format!("{}  ·  {}", note_name(key), velocity_number(velocity))
}

/// Velocity as the familiar 1 to 127 number.
pub fn velocity_number(velocity: f32) -> u8 {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=127"
    )]
    let number = (velocity.clamp(0.0, 1.0) * 127.0).round() as u8;
    number.max(1)
}

/// The range the computer keyboard plays, for the keyboard panel.
pub fn keyboard_range(low: u8, high: u8) -> String {
    format!("{} – {}", note_name(low), note_name(high))
}

/// The time signature button's tooltip.
pub fn time_signature(numerator: u8, denominator: u8) -> String {
    format!("{numerator}/{denominator}")
}

/// Bar.beat.sixteenth, as shown in the transport.
pub fn position(bar: u32, beat: u32, sixteenth: u32) -> String {
    format!("{bar}.{beat}.{sixteenth}")
}

/// Minutes and seconds, as shown under the position.
pub fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive time"
    )]
    let whole = seconds.floor() as u64;
    let tenths = ((seconds - seconds.floor()) * 10.0).floor();
    format!("{}:{:02}.{tenths}", whole / 60, whole % 60)
}

/// The tempo readout.
pub fn bpm(bpm: f64) -> String {
    if (bpm - bpm.round()).abs() < 0.005 {
        format!("{bpm:.0}")
    } else {
        format!("{bpm:.1}")
    }
}

pub const BPM: &str = "BPM";

// Status messages.
pub fn saved(path: &str) -> String {
    format!("Saved {path}")
}

pub fn opened(path: &str) -> String {
    format!("Opened {path}")
}

pub fn could_not_open(error: &str) -> String {
    format!("Could not open the song: {error}")
}

pub fn could_not_save(error: &str) -> String {
    format!("Could not save the song: {error}")
}

pub fn undone(label: &str) -> String {
    format!("Undone: {label}")
}

pub fn redone(label: &str) -> String {
    format!("Redone: {label}")
}

pub fn recorded(count: usize) -> String {
    match count {
        0 => "Nothing was recorded".to_owned(),
        1 => "Recorded 1 note".to_owned(),
        _ => format!("Recorded {count} notes"),
    }
}

pub fn counting_in(beats_left: u32) -> String {
    format!("Count-in… {beats_left}")
}

pub const RECORDING: &str = "Recording — play notes; press Space to stop";
pub const NO_SOUND_DEVICE: &str = "No sound device found: everything works, but nothing can be heard. Plug one in and it will be used.";
pub const UNSAVED_QUESTION: &str = "This song has changes that are not saved.";
pub const SAVE_CHANGES: &str = "Save changes";
pub const DISCARD_CHANGES: &str = "Discard changes";
pub const CANCEL: &str = "Cancel";
pub const RECOVER_TITLE: &str = "Recover your work?";

pub fn recover_question() -> String {
    format!(
        "{} did not close normally last time. Your work was saved automatically a short time before that. Do you want to continue with it?",
        daw_ident::PRODUCT_NAME
    )
}

pub const RECOVER: &str = "Recover it";
pub const OPEN_TITLE: &str = "Open a song";
pub const SAVE_TITLE: &str = "Save the song";

pub fn midi_connected(name: &str) -> String {
    format!("MIDI keyboard connected: {name}")
}

pub fn could_not_start(error: &str) -> String {
    format!(
        "{} could not start.\n\n{error}\n\nIf this keeps happening, please report it, with this message.",
        daw_ident::PRODUCT_NAME
    )
}
pub const DISCARD_RECOVERY: &str = "Start fresh";

pub fn about() -> String {
    format!(
        "{} {}\nAn easy, open-source music workstation.\nLicensed under the GNU GPL, version 3 or later.\nPiano: Osiris Piano by Versilian Studios and Karoryfer Samples (CC0).\nFonts: the Go fonts by Bigelow & Holmes (BSD licence).",
        daw_ident::PRODUCT_NAME,
        env!("CARGO_PKG_VERSION")
    )
}

/// The name of the project file type in open and save dialogs.
pub fn file_type_name() -> String {
    format!("{} song", daw_ident::PRODUCT_NAME)
}
