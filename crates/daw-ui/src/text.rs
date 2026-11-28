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
pub const CTRL: &str = "Ctrl";
pub const ALT: &str = "Alt";
pub const SHIFT: &str = "Shift";

/// A friendlier name than egui's for some keys, in shortcuts.
pub fn key_name(key: egui::Key) -> Option<&'static str> {
    use egui::Key;
    Some(match key {
        Key::ArrowLeft => "Left arrow",
        Key::ArrowRight => "Right arrow",
        Key::ArrowUp => "Up arrow",
        Key::ArrowDown => "Down arrow",
        Key::PageUp => "Page Up",
        Key::PageDown => "Page Down",
        Key::Escape => "Esc",
        Key::Backspace => "Backspace",
        Key::Delete => "Delete",
        Key::Minus => "-",
        _ => return None,
    })
}

/// The name of a keyboard layout family.
pub fn layout_name(family: crate::keys::LayoutFamily) -> &'static str {
    use crate::keys::LayoutFamily;
    match family {
        LayoutFamily::Qwerty => "QWERTY",
        LayoutFamily::Azerty => "AZERTY",
        LayoutFamily::Qwertz => "QWERTZ",
    }
}

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
        C::TransposeUp => "Move notes up a semitone (a step, in the key)",
        C::TransposeDown => "Move notes down a semitone (a step, in the key)",
        C::InvertUp => "Invert chords up",
        C::InvertDown => "Invert chords down",
        C::OctaveUp => "Move notes up an octave",
        C::OctaveDown => "Move notes down an octave",
        C::KeyboardOctaveUp => "Computer keyboard an octave up",
        C::KeyboardOctaveDown => "Computer keyboard an octave down",
        C::ZoomIn => "Zoom in",
        C::ZoomOut => "Zoom out",
        C::ToggleKeyboard => "Show or hide the keyboard",
        C::ToggleMixer => "Show or hide the mixer",
        C::ToggleLibrary => "Show or hide the sound library",
        C::SwitchView => "Switch between the song and the pattern",
        C::Export => "Export",
        C::Split => "Split the clip at the play position",
        C::ImportAudio => "Import",
        C::UserGuide => "User guide",
        C::PitchMode => "Edit the notes or the sung pitch (voice tracks)",
    }
}
pub const ABOUT: &str = "About";
pub const SNAP: &str = "Snap";
pub const SNAP_HINT: &str = "Notes snap to this grid. Hold Alt while dragging to place freely.";
pub const SNAP_CLIPS: &str = "Snap to bars and beats";
pub const SNAP_CLIPS_HINT: &str = "Patterns land on bars and sounds on beats. Untick to put them anywhere, off the beat. Holding Alt while dragging does the opposite.";
pub const NOTE_LENGTH: &str = "New notes";
pub const KEYBOARD: &str = "Keyboard";
pub const ON_SCREEN_KEYBOARD: &str = "On-screen keyboard";
pub const KEYBOARD_HINT: &str = "Play with your computer keyboard: the two rows from Z and from Q are piano keys. Page Up and Page Down change the octave.";
pub const OCTAVE_DOWN: &str = "Octave down";
pub const OCTAVE_UP: &str = "Octave up";
pub const VELOCITY: &str = "Velocity";
/// The buttons beside the lane under the piano roll, for screen readers.
pub const LANE_VELOCITY: &str = "Velocity lane";
pub const LANE_TEMPO: &str = "Tempo lane";
pub const SHOW_VELOCITY: &str = "Show the notes' velocity under the piano roll";
pub const SHOW_TEMPO: &str = "Show the song's tempo under the piano roll, where this pattern plays";
pub const PIANO_ROLL: &str = "Piano roll";
pub const SHOW_KEYBOARD: &str = "Show the keyboard";
pub const HIDE_KEYBOARD: &str = "Hide the keyboard";

/// The hint shown in an empty piano roll.
pub const EMPTY_PATTERN_HINT: &str = "Click anywhere to place a note — press Space to hear it";
/// A second line under the hint.
pub const EMPTY_PATTERN_HINT_2: &str =
    "Or play the computer keyboard: Z S X D C V… and Q 2 W 3 E R…";

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
pub const EDIT_CONTROL_CURVE: &str = "change a controller's curve";
pub const EDIT_RECORD_CONTROLS: &str = "record the controllers";

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

/// The name of a note value, 1/`n` of a whole note: "1/8", "1/8 triplet",
/// "1/16 quintuplet" (five in the time of four), "1/16 septuplet".
pub fn note_value_name(n: i64) -> String {
    if n % 3 == 0 {
        format!("1/{} triplet", n * 2 / 3)
    } else if n % 5 == 0 {
        format!("1/{} quintuplet", n * 4 / 5)
    } else if n % 7 == 0 {
        format!("1/{} septuplet", n * 4 / 7)
    } else {
        format!("1/{n}")
    }
}

/// The name of a note length, `numerator`/`denominator` of a whole note:
/// "1/4", "1/4 dotted", "1/8 triplet".
pub fn length_name(numerator: i64, denominator: i64) -> String {
    if numerator == 3 && denominator % 2 == 0 {
        format!("1/{} dotted", denominator / 2)
    } else if numerator == 1 {
        note_value_name(denominator)
    } else {
        format!("{numerator}/{denominator}")
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

pub const RECORDED_CONTROLS: &str = "Recorded what the controller's wheels and pedals did";

pub fn recorded(count: usize) -> String {
    match count {
        0 => "Nothing was recorded".to_owned(),
        1 => "Recorded 1 note".to_owned(),
        _ => format!("Recorded {count} notes"),
    }
}

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
pub const RECOVERED_TAKE: &str =
    "It includes the sound that was being recorded when the program stopped.";
pub const RECOVERY_OLDER: &str =
    "The song's file was saved after this copy was made: recovering may bring back older work.";

/// Which song was found after a crash, and how old the copy is.
pub fn recovered_song(name: &str, age_seconds: u64) -> String {
    let minutes = age_seconds / 60;
    let when = match minutes {
        0 => "less than a minute ago".to_owned(),
        1 => "about a minute ago".to_owned(),
        2..=59 => format!("about {minutes} minutes ago"),
        60..=119 => "about an hour ago".to_owned(),
        _ if minutes < 48 * 60 => format!("about {} hours ago", minutes / 60),
        _ => format!("about {} days ago", minutes / (24 * 60)),
    };
    format!("{name}, saved automatically {when}")
}
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
        "{} {}\nAn easy, open-source music workstation.\nLicensed under the GNU GPL, version 3 or later.\nPiano: Osiris Piano by Versilian Studios and Karoryfer Samples (CC0).\nDrums: Big Rusty Drums by Karoryfer Samples (CC0).\nFonts: the Go fonts by Bigelow & Holmes (BSD licence).",
        daw_ident::PRODUCT_NAME,
        env!("CARGO_PKG_VERSION")
    )
}

/// The name of the project file type in open and save dialogs.
pub fn file_type_name() -> String {
    format!("{} song", daw_ident::PRODUCT_NAME)
}

// ----- Phase 2: drums, the song, the mixer, effects, export

/// The drum track of a new song.
pub const DRUMS_TRACK: &str = "Drums";
/// The first drum pattern of a new song.
pub const FIRST_BEAT: &str = "Beat 1";

/// "Beat 2": a drum pattern's default name.
pub fn beat_name(number: u32) -> String {
    format!("Beat {number}")
}

/// "Pattern 2": a pattern's default name.
pub fn pattern_name(number: u32) -> String {
    format!("Pattern {number}")
}

/// "Piano 2": a name made unique with a number.
pub fn numbered(base: &str, number: u32) -> String {
    format!("{base} {number}")
}

// Undo-step names.
pub const EDIT_ADD_TRACK: &str = "add track";
pub const EDIT_RENAME_TRACK: &str = "rename track";
pub const EDIT_REMOVE_TRACK: &str = "delete track";
pub const EDIT_MOVE_TRACK: &str = "move track";
pub const EDIT_KIT: &str = "change kit";
pub const EDIT_NEW_PATTERN: &str = "new pattern";
pub const EDIT_DUPLICATE_PATTERN: &str = "duplicate pattern";
pub const EDIT_RENAME_PATTERN: &str = "rename pattern";
pub const EDIT_REMOVE_PATTERN: &str = "delete pattern";
pub const EDIT_STEPS: &str = "change step size";
pub const EDIT_ADD_HIT: &str = "add drum hit";
pub const EDIT_REMOVE_HIT: &str = "remove drum hits";
pub const EDIT_PAINT_HITS: &str = "draw drum hits";
pub const EDIT_PLACE_PATTERN: &str = "place pattern";
pub const EDIT_MOVE_CLIP: &str = "move clip";
pub const EDIT_RESIZE_CLIP: &str = "change clip length";
pub const EDIT_REMOVE_CLIP: &str = "remove clip";
pub const EDIT_DUPLICATE_CLIP: &str = "duplicate clip";
pub const EDIT_VOLUME: &str = "change volume";
pub const EDIT_PAN: &str = "change pan";
pub const EDIT_MUTE: &str = "mute track";
pub const EDIT_SOLO: &str = "solo track";
pub const EDIT_MASTER_VOLUME: &str = "change master volume";
pub const EDIT_ADD_EFFECT: &str = "add effect";
pub const EDIT_REMOVE_EFFECT: &str = "remove effect";
pub const EDIT_MOVE_EFFECT: &str = "move effect";
pub const EDIT_EFFECT_ON: &str = "switch effect on";
pub const EDIT_EFFECT_OFF: &str = "bypass effect";
pub const EDIT_CHANGE_EFFECT: &str = "change effect";
pub const EDIT_PAD: &str = "change drum sound";

// The location line.
pub const SHOW_SONG: &str = "Show the whole song";
pub const OPEN_TRACK_HINT: &str = "Open another track";
pub const PATTERN_MENU_HINT: &str = "Choose, add, copy or rename patterns";
pub const NEW_PATTERN: &str = "New pattern";
pub const DUPLICATE_PATTERN: &str = "Duplicate this pattern";
pub const RENAME_PATTERN: &str = "Rename…";
pub const DELETE_PATTERN: &str = "Delete this pattern";
pub const NOT_PLACED: &str = "Not in the song yet";
pub const NOT_PLACED_HINT: &str =
    "This pattern plays on its own. In the song view, click on its track to place it.";
pub const RENAME: &str = "Rename";
pub const NAME: &str = "Name";
pub const OK: &str = "OK";
pub const KIT: &str = "Kit";
pub const KIT_HINT: &str = "Which drum kit this track plays";
pub const STEPS: &str = "Steps";
pub const STEPS_HINT: &str = "How long each square of the step grid is";
pub const LOOP_SONG: &str = "Loop the song (drag along the ruler to loop a part)";

/// A bundled kit's name.
pub fn kit_name(kit: &str) -> &str {
    match kit {
        daw_model::ACOUSTIC_KIT => "Acoustic",
        daw_model::ELECTRONIC_KIT => "Electronic",
        other => other,
    }
}

// The step sequencer.
pub const STEP_GRID: &str = "Step sequencer";
pub const LOUDNESS: &str = "Loudness";
pub const LOUDNESS_HINT: &str =
    "How hard each hit of the chosen sound plays. Drag a bar up or down.";
pub const PAD_HINT: &str = "Click to hear it and change its sound";
pub const EMPTY_BEAT_HINT: &str = "Click the squares to make a beat — press Space to hear it";

/// The second line of the empty step grid's hint, with the computer keys
/// that play the kick, the snare and the hi-hat.
pub fn beat_keys_hint(kick: &str, snare: &str, hat: &str) -> String {
    format!(
        "Or play the drums on the computer keyboard: {kick} is the kick, {snare} the snare, {hat} the hi-hat"
    )
}

/// A square of the step grid, for screen readers: "Kick, step 3".
pub fn step_cell(sound: &str, step: usize) -> String {
    format!("{sound}, step {step}")
}
pub const PAD_VOLUME: &str = "Volume";
pub const PAD_PAN: &str = "Pan";
pub const PAD_TUNE: &str = "Tune";
pub const PAD_LENGTH: &str = "Length";
pub const PAD_MUTED: &str = "Silence this sound";
pub const RESET_SOUND: &str = "Reset";

/// The name of a drum sound.
pub fn pad_name(sound: daw_dsp::drums::PadSound) -> &'static str {
    use daw_dsp::drums::PadSound as P;
    match sound {
        P::Kick => "Kick",
        P::Snare => "Snare",
        P::Clap => "Clap",
        P::ClosedHat => "Closed hi-hat",
        P::PedalHat => "Pedal hi-hat",
        P::OpenHat => "Open hi-hat",
        P::Rimshot => "Rimshot",
        P::SideStick => "Side stick",
        P::LowTom => "Low tom",
        P::MidTom => "Mid tom",
        P::HighTom => "High tom",
        P::Crash => "Crash",
        P::Ride => "Ride",
        P::RideBell => "Ride bell",
        P::Cowbell => "Cowbell",
    }
}

// The song view.
pub const SONG_VIEW: &str = "Song timeline";
pub const TRACKS: &str = "Tracks";

/// What a drum track plays, under its name: "Acoustic kit".
pub fn kit_description(kit: &str) -> String {
    format!("{kit} kit")
}
pub const ADD_DRUMS: &str = "Drums";
pub const ADD_DRUMS_HINT: &str = "Add a drum track";
pub const ADD_PIANO: &str = "Piano";
pub const ADD_PIANO_HINT: &str = "Add a piano track";
/// What an audio track's header says it plays.
pub const AUDIO_TRACK_KIND: &str = "Recorded and imported sound";
pub const MUTE_SHORT: &str = "M";
pub const SOLO_SHORT: &str = "S";
pub const MUTE_HINT: &str = "Silence this track";
pub const SOLO_HINT: &str = "Hear this track alone";
pub const TRACK_MENU: &str = "Track options";
pub const MOVE_UP: &str = "Move up";
pub const MOVE_DOWN: &str = "Move down";
pub const DELETE_TRACK: &str = "Delete track";
pub const OPEN_PATTERN: &str = "Open";
pub const DUPLICATE_CLIP: &str = "Duplicate";
pub const REMOVE_CLIP: &str = "Remove from the song";
pub const SONG_HINT: &str = "Click on a track to place its pattern · right-click to choose which · drag to move · drag the end to repeat · double-click to open";
pub const PLACE_PATTERN_HERE: &str = "Place a pattern here";
pub const NEW_PATTERN_HERE: &str = "New pattern";
pub const PLAY_ANOTHER_PATTERN: &str = "Play another pattern";
pub const EDIT_SWAP_PATTERN: &str = "play another pattern";

// The mixer.
pub const SHOW_MIXER: &str = "Show the mixer";
pub const HIDE_MIXER: &str = "Hide the mixer";
pub const MASTER: &str = "Master";
pub const VOLUME: &str = "Volume";
pub const PAN: &str = "Pan";
pub const PAN_HINT: &str = "Where the track sits, from left to right. Double-click for the middle.";
pub const VOLUME_HINT: &str = "Double-click for 0 dB";
pub const ADD_EFFECT: &str = "Add effect";
pub const NO_EFFECTS: &str = "No effects";
pub const EFFECT_HINT: &str = "Click to adjust · right-click for more";
pub const EFFECT_ON: &str = "On";
pub const REMOVE_EFFECT: &str = "Remove";
pub const MOVE_EARLIER: &str = "Move up the chain";
pub const MOVE_LATER: &str = "Move down the chain";
pub const LEVEL: &str = "Level";

/// The name of an effect.
pub fn effect_name(kind: &daw_model::EffectKind) -> String {
    use daw_model::EffectKind as K;
    match kind {
        K::Eq(_) => "EQ".to_owned(),
        K::Compressor(_) => "Compressor".to_owned(),
        K::Reverb(_) => "Reverb".to_owned(),
        K::Delay(_) => "Delay".to_owned(),
        K::Limiter(_) => "Limiter".to_owned(),
        K::Plugin(setup) => setup.plugin.name.clone(),
    }
}

/// What an effect does, in a few words.
pub fn effect_description(kind: &daw_model::EffectKind) -> String {
    use daw_model::EffectKind as K;
    match kind {
        K::Eq(_) => "Shape the tone: more or less bass, middle and treble".to_owned(),
        K::Compressor(_) => "Even out the loudness: quiet parts up, loud parts down".to_owned(),
        K::Reverb(_) => "Put the sound in a room".to_owned(),
        K::Delay(_) => "Echoes in time with the song".to_owned(),
        K::Limiter(_) => "Keep peaks from distorting, and make it louder".to_owned(),
        K::Plugin(setup) => plugin_by(&setup.plugin.vendor),
    }
}

/// The name of a built-in effect's setting that can change over the song
/// (in the order of [`daw_model::EffectKind::params`]).
pub fn effect_param_name(kind: &daw_model::EffectKind, index: usize) -> &'static str {
    use daw_model::EffectKind as K;
    let names: &[&str] = match kind {
        K::Eq(_) => &[
            "Bass",
            "Bass frequency",
            "Middle",
            "Middle frequency",
            "Middle width",
            "Treble",
            "Treble frequency",
        ],
        K::Compressor(_) => &[THRESHOLD, RATIO, ATTACK, RELEASE, KNEE, MAKEUP],
        K::Reverb(_) => &[AMOUNT, ROOM_SIZE, DAMPING, STEREO_WIDTH, PRE_DELAY],
        K::Delay(_) => &[REPEATS, AMOUNT, TONE],
        K::Limiter(_) => &[DRIVE, CEILING, RELEASE],
        K::Plugin(_) => &[],
    };
    names.get(index).copied().unwrap_or("")
}

/// A built-in effect's setting as people read it ("+3.0 dB", "1.2 kHz").
pub fn effect_param_value(kind: &daw_model::EffectKind, index: usize, value: f32) -> String {
    use daw_model::EffectKind as K;
    match (kind, index) {
        (K::Eq(_), 0 | 2 | 5) | (K::Compressor(_), 0 | 4 | 5) | (K::Limiter(_), 0 | 1) => db(value),
        (K::Eq(_), 1 | 3 | 6) | (K::Delay(_), 2) => hz(value),
        (K::Eq(_), 4) => q(value),
        (K::Compressor(_), 1) => ratio(value),
        (K::Compressor(_), 2 | 3) | (K::Reverb(_), 4) | (K::Limiter(_), 2) => ms(value),
        _ => percent(value),
    }
}

/// Something the engine could not do, in words.
pub fn problem(problem: &daw_engine::Problem) -> String {
    use daw_engine::Problem as P;
    match problem {
        P::MissingSound(name) => {
            format!("The sound \"{name}\" is missing from the program; a plain sound plays instead")
        }
        P::MissingFile(path) => {
            let name = std::path::Path::new(path)
                .file_name()
                .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
            format!(
                "The file \"{name}\" is not on this computer (it was at {path}); a plain sound plays instead"
            )
        }
        P::UnreadableSound { sound, reason } => {
            format!("Could not read the sound {sound}; a plain sound plays instead ({reason})")
        }
        P::TooManyTracks(most) => format!("Too many tracks: at most {most} can play"),
        P::TooManyEffects(most) => format!("Too many effects: at most {most} can run"),
        P::PluginMissing(name) => {
            format!("The plugin \"{name}\" is not installed on this computer")
        }
        P::PluginFailed { plugin, reason } => {
            format!("The plugin \"{plugin}\" could not be started ({reason})")
        }
        P::PluginSettings { plugin, reason } => {
            format!("The plugin \"{plugin}\" did not take its saved settings ({reason})")
        }
        P::PartlyRead {
            sound,
            left_out,
            missing,
        } => partly_read(sound, left_out, missing),
    }
}

// The sound library and SoundFonts (decision 0023).
pub const LIBRARY: &str = "Library";
pub const SHOW_LIBRARY: &str = "Show the sound library";
pub const HIDE_LIBRARY: &str = "Hide the sound library";
pub const ADD_SOUND_FOLDER: &str = "Add a folder…";
pub const ADD_SOUND_FOLDER_HINT: &str = "Add a folder of sounds, MIDI files or instruments (SoundFonts, SFZ) to the library (nothing is copied)";
pub const LIBRARY_EMPTY: &str = "Add a folder of sounds to find, hear and use them here: WAV, AIFF, FLAC, MP3 and Ogg sounds, MIDI files, SoundFonts and SFZ instruments.";
pub const LIBRARY_HINT: &str = "Click a sound to hear it · drag it onto the song, or double-click to add it at the play position";
pub const LIBRARY_LISTING: &str = "Looking through the folder…";
pub const LIBRARY_NOTHING_HERE: &str = "No sounds, MIDI files or instruments here.";
pub const LIBRARY_UP: &str = "Up";
pub const LIBRARY_UP_HINT: &str = "Back to the folder above";
pub const LIBRARY_REFRESH: &str = "Look again";
pub const LIBRARY_REFRESH_HINT: &str = "List the folder again (files added since appear)";
pub const REMOVE_SOUND_FOLDER: &str = "Remove from the library";
pub const REMOVE_SOUND_FOLDER_HINT: &str =
    "The folder stays on the disk: the library just stops showing it";
pub const ADD_TO_SONG: &str = "Add to the song";
pub const PLAY_SOUND: &str = "Play";
pub const STOP_SOUND: &str = "Stop";
pub const PREVIEW_TOO_LARGE: &str = "it is too large to play here; drag it onto the song instead";
pub const SOUNDFONT: &str = "SoundFont";
pub const ADD_SOUNDFONT: &str = "SoundFont or SFZ…";
pub const ADD_SOUNDFONT_HINT: &str = "Play a .sf2 or .sfz instrument file";
pub const SOUNDFONT_FILE_TYPE: &str = "SoundFont or SFZ instrument";
pub const CHOOSE_SOUNDFONT_TITLE: &str = "Choose a SoundFont or an SFZ instrument";
pub const PRESET: &str = "Preset";
pub const PRESET_HINT: &str = "The sound of the SoundFont this track plays";
pub const OPEN_TRACK_OR_PRESET_HINT: &str =
    "Open another track, or choose the preset this SoundFont plays";
pub const EDIT_ADD_SOUNDFONT: &str = "add SoundFont track";
pub const EDIT_SOUNDFONT_PRESET: &str = "choose SoundFont preset";

/// A sound the library could not play.
pub fn cannot_preview(name: &str, why: &str) -> String {
    format!("Could not play {name}: {why}")
}

/// How many files a library folder holds.
pub fn library_count(count: usize, truncated: bool) -> String {
    match (count, truncated) {
        (_, true) => format!("{count}+ files"),
        (1, false) => "1 file".to_owned(),
        (n, false) => format!("{n} files"),
    }
}

/// How many files a search found, when not all are shown.
pub fn library_more_found(shown: usize, total: usize) -> String {
    format!("Showing {shown} of {total}: type more to narrow the search")
}

/// What a SoundFont track plays, under its name.
pub fn soundfont_track(file: &str) -> String {
    format!("SoundFont · {file}")
}

// SFZ instruments (decision 0024).
pub const SFZ: &str = "SFZ instrument";
pub const EDIT_ADD_SFZ: &str = "add SFZ instrument track";

/// What an SFZ track plays, under its name (`reading` while its sounds are
/// read).
pub fn sfz_track(file: &str, reading: bool) -> String {
    if reading {
        format!("SFZ · {file} · reading…")
    } else {
        format!("SFZ · {file}")
    }
}

// Singing voices (decision 0027).
pub const VOICEBANKS_TITLE: &str = "Voicebanks";
pub const VOICEBANKS_INTRO: &str = "Voicebanks sing the notes of voice tracks. Add classic UTAU \
     voicebanks: a voicebank's folder, or the .zip it came in.";
pub const VOICEBANKS_NO_FOLDER: &str = "Voicebanks cannot be added on this computer: the program \
     has no folder of its own to keep them in.";
pub const ADD_VOICEBANK_FOLDER: &str = "Add a folder…";
pub const ADD_VOICEBANK_FOLDER_HINT: &str =
    "A voicebank's folder (the one with oto.ini in it): it is copied here";
pub const ADD_VOICEBANK_ZIP: &str = "Add a .zip…";
pub const ADD_VOICEBANK_ZIP_HINT: &str = "The .zip a voicebank came in: it is unpacked here";
pub const CHOOSE_VOICEBANK_FOLDER: &str = "Choose a voicebank's folder";
pub const CHOOSE_VOICEBANK_ZIP_TITLE: &str = "Choose a voicebank's .zip";
pub const ZIP_FILE_TYPE: &str = "Zip archive";
pub const ADDING_VOICEBANK: &str = "Adding the voicebank…";
pub const ADDING_STOPPED: &str = "adding it stopped unexpectedly";
pub const NO_VOICEBANKS: &str = "No voicebank yet.";
pub const READ_THE_TERMS: &str =
    "Read the creator's terms before you use this voice: they say what you may do with it.";
pub const NO_TERMS: &str = "This voicebank came without a readme.";
pub const REMOVE_VOICEBANK: &str = "Remove this voicebank";
pub const VOICES: &str = "Voices";
pub const MANAGE_VOICEBANKS: &str = "Voicebanks…";
pub const MANAGE_VOICEBANKS_HINT: &str = "Add, read and remove voicebanks";
pub const RENDERING_VOICES: &str = "Rendering the voice…";
pub const LYRIC: &str = "Lyric";
pub const LYRIC_HINT: &str = "Type the lyric (words after a space go to the next notes); Enter \
     to finish, Escape to leave it as it was";
pub const LYRIC_NOT_FOUND: &str = "The voicebank has no recording for this lyric: it sings \"a\"";
pub const EDIT_LYRIC_HINT: &str = "Press Enter to type the selected note's lyric";

/// Why a song's voice is silent on this computer.
pub fn voicebank_not_installed(name: &str) -> String {
    format!("the voicebank “{name}” is not installed on this computer (add it in Voicebanks…)")
}

/// What the Voicebanks window says once one is added.
pub fn voicebank_added(name: &str) -> String {
    format!("Added {name}.")
}

/// What it says when one cannot be added.
pub fn could_not_add_voicebank(reason: &str) -> String {
    format!("Could not add it: {reason}.")
}

/// What it says once one is removed.
pub fn voicebank_removed(name: &str) -> String {
    format!("Removed {name}.")
}

/// What it says when one cannot be removed.
pub fn could_not_remove_voicebank(reason: &str) -> String {
    format!("Could not remove it: {reason}.")
}

/// A voicebank in a list: "by Someone · 412 syllables".
pub fn voicebank_detail(author: Option<&str>, syllables: usize) -> String {
    let count = if syllables == 1 {
        "1 syllable".to_owned()
    } else {
        format!("{syllables} syllables")
    };
    match author {
        Some(author) => format!("by {author} · {count}"),
        None => count,
    }
}

/// The title of a voicebank's terms.
pub fn voicebank_terms_title(name: &str) -> String {
    format!("{name}: its creator's terms")
}

/// A voice track's description: "Voice · Uta", while its voicebank is
/// read "Voice · Uta · reading…".
pub fn voice_track(voicebank: &str, reading: bool) -> String {
    if reading {
        format!("Voice · {voicebank} · reading…")
    } else {
        format!("Voice · {voicebank}")
    }
}

/// The status line while instruments are read.
pub fn reading_instruments(files: &[String]) -> String {
    match files {
        [one] => format!(
            "Reading the instrument {}… its track plays once it is ready",
            file_name(std::path::Path::new(one))
        ),
        many => format!(
            "Reading {} instruments… their tracks play once they are ready",
            many.len()
        ),
    }
}

/// What an SFZ instrument plays without, said once when it is ready.
pub fn partly_read(
    sound: &str,
    left_out: &[daw_dsp::sampler::LeftOut],
    missing: &[String],
) -> String {
    use daw_dsp::sampler::LeftOut as L;
    let mut parts: Vec<String> = left_out
        .iter()
        .filter_map(|part| {
            Some(
                match part {
                    L::Lfos => "LFOs (vibrato, tremolo)",
                    L::Envelopes => "pitch envelopes",
                    L::ControllerChanges => "changes made by controllers (the modulation wheel…)",
                    L::Effects => "its own effects",
                    L::Filters => "some of its filters",
                    L::Equalizers => "its equalisers",
                    L::Reversed => "sounds played backwards",
                    L::LoopCrossfades => "smoothed loop joins",
                    L::Keyswitches => "some keyswitches",
                    L::Waves => "built-in waves other than the sine",
                    L::EmbeddedSounds => "sounds stored inside the file",
                    L::Other(_) => return None,
                }
                .to_owned(),
            )
        })
        .collect();
    let unknown: Vec<&str> = left_out
        .iter()
        .filter_map(|part| match part {
            L::Other(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    if !unknown.is_empty() {
        parts.push(format!(
            "settings it does not know ({})",
            unknown.join(", ")
        ));
    }
    let name = file_name(std::path::Path::new(sound));
    let without = if parts.is_empty() {
        String::new()
    } else {
        format!(" plays without {}", parts.join(", "))
    };
    let gone = match missing {
        [] => String::new(),
        [one] => format!("; its sound file \"{one}\" is missing or damaged"),
        [first, ..] => format!(
            "; {} of its sound files are missing or damaged (\"{first}\"…)",
            missing.len()
        ),
    };
    format!("{name}{without}{gone}")
}

/// A preset of a SoundFont in the preset menu: its bank and number.
pub fn preset_number(bank: u16, program: u8) -> String {
    format!("{bank}:{program:03}")
}

// Custom kits (decision 0025).
pub const ADD_KIT: &str = "Custom kit";
pub const ADD_KIT_HINT: &str = "A drum kit of your own sounds";
pub const KIT_TRACK: &str = "My kit";
pub const SOUND_FILE_TYPE: &str = "Sound";
pub const CHOOSE_KIT_SOUNDS_TITLE: &str = "Choose sounds for the kit";
pub const ADD_SOUNDS: &str = "Add sounds…";
pub const ADD_SOUNDS_HINT: &str =
    "Add sounds to this kit (choose many at once): each goes on its own key";
pub const EMPTY_KIT_HINT: &str = "Add your sounds: each becomes a row here, on its own key";
pub const EMPTY_KIT_DETAIL: &str =
    "Click Add sounds… above, drop sound files here, or drag them from the sound library";
pub const DROP_ON_KIT: &str = "Let go to add these sounds to the kit";
pub const REMOVE_PAD: &str = "Remove from the kit";
pub const REMOVE_PAD_HINT: &str = "The sound leaves the kit, with the hits on its row";
pub const MAKE_KIT: &str = "Make a kit of these sounds";
pub const MAKE_KIT_HINT: &str = "A custom kit with every sound in this folder, each on its own key";
pub const EDIT_ADD_KIT: &str = "add custom kit";
pub const EDIT_ADD_PAD: &str = "add sound to kit";
pub const EDIT_REMOVE_PAD: &str = "remove sound from kit";

/// The second line of a custom kit's empty step grid: the computer key
/// that plays its first sound.
pub fn kit_keys_hint(key: &str, name: &str) -> String {
    format!(
        "Or play your sounds on the computer keyboard: {key} plays {name}, the keys after it the next sounds"
    )
}

/// The status line while sounds are being added to a custom kit.
pub fn adding_kit_sounds(count: usize, kit: &str) -> String {
    match count {
        1 => format!("Adding a sound to {kit}…"),
        n => format!("Adding {n} sounds to {kit}…"),
    }
}

/// A sound added to a custom kit.
pub fn added_to_kit(file: &str, kit: &str) -> String {
    format!("Added {file} to {kit}")
}

/// A custom kit that cannot take more sounds.
pub fn kit_full(kit: &str) -> String {
    format!("{kit} is full: a kit holds at most 128 sounds, one on each key")
}

/// What a custom kit's track plays, under its name.
pub fn custom_kit_description(count: usize) -> String {
    match count {
        0 => "Custom kit · no sounds yet".to_owned(),
        1 => "Custom kit · 1 sound".to_owned(),
        n => format!("Custom kit · {n} sounds"),
    }
}

// The synth (decision 0019).
pub const ADD_SYNTH: &str = "Synth";
pub const ADD_SYNTH_HINT: &str = "Add a synth track: shape your own sounds";
/// A new synth track's name.
pub const SYNTH_TRACK: &str = "Synth";
pub const OPEN_SYNTH: &str = "Open the synth";
pub const CLOSE_SYNTH: &str = "Close the synth";
pub const EDIT_ADD_SYNTH: &str = "add synth track";
pub const EDIT_CHANGE_SYNTH: &str = "change synth";
pub const EDIT_SYNTH_PRESET: &str = "choose synth preset";
pub const SYNTH_PRESET: &str = "Preset";
pub const SYNTH_PRESET_HINT: &str = "A starting point: every control is set for a kind of sound";
pub const SYNTH_CUSTOM: &str = "Custom";
pub const OSCILLATORS: &str = "Oscillators";
pub const OSCILLATOR_1: &str = "Oscillator 1";
pub const OSCILLATOR_2: &str = "Oscillator 2";
pub const WAVE: &str = "Wave";
pub const OCTAVE: &str = "Octave";
pub const SEMITONES: &str = "Semitones";
pub const FINE_TUNE: &str = "Fine";
pub const FINE_TUNE_HINT: &str = "Detune a little: two oscillators slightly apart sound wider";
pub const NOISE: &str = "Noise";
pub const PULSE_WIDTH: &str = "Pulse width";
pub const PULSE_WIDTH_HINT: &str = "Changes the colour of the square waves";
pub const FILTER: &str = "Filter";
pub const FILTER_KIND: &str = "Kind";
pub const CUTOFF: &str = "Cutoff";
pub const CUTOFF_HINT: &str = "Lower makes the sound darker";
pub const RESONANCE: &str = "Resonance";
pub const RESONANCE_HINT: &str = "Makes the filter ring at the cutoff";
pub const ENVELOPE_AMOUNT: &str = "Envelope";
pub const ENVELOPE_AMOUNT_HINT: &str =
    "How far the filter envelope opens the filter as a note plays (below zero, it closes it)";
pub const KEY_TRACKING: &str = "Follow keys";
pub const KEY_TRACKING_HINT: &str = "Higher notes open the filter further";
pub const FILTER_CURVE: &str = "The filter's response";
pub const ENVELOPES: &str = "Envelopes";
pub const VOLUME_ENVELOPE: &str = "Volume";
pub const FILTER_ENVELOPE: &str = "Filter";
pub const DECAY: &str = "Decay";
pub const SUSTAIN: &str = "Sustain";
pub const ENVELOPE_SHAPE: &str = "The envelope's shape";
pub const MOVEMENT: &str = "Movement";
pub const LFO_SHAPE: &str = "Shape";
pub const LFO_RATE: &str = "Speed";
pub const VIBRATO: &str = "Vibrato";
pub const VIBRATO_HINT: &str = "Wavers the pitch";
pub const WOBBLE: &str = "Wobble";
pub const WOBBLE_HINT: &str = "Moves the filter up and down";
pub const TREMOLO: &str = "Tremolo";
pub const TREMOLO_HINT: &str = "Wavers the volume";
pub const PLAYING: &str = "Playing";
pub const MONO: &str = "One note at a time";
pub const MONO_HINT: &str = "Like a solo instrument: a new note takes over, and can glide";
pub const SYNTH_GLIDE: &str = "Glide";
pub const SYNTH_GLIDE_HINT: &str = "Notes slide into each other (one note at a time only)";
pub const VELOCITY_AMOUNT: &str = "Velocity";
pub const VELOCITY_AMOUNT_HINT: &str = "How much harder playing is louder and brighter";

/// The synth window's title: "Synth · Bass line" (just "Synth" for a
/// track of that name).
pub fn synth_title(track: &str) -> String {
    if track == SYNTH_TRACK {
        SYNTH_TRACK.to_owned()
    } else {
        format!("{SYNTH_TRACK} · {track}")
    }
}

/// What a synth track plays, under its name: "Synth · Warm pad".
pub fn synth_description(settings: &daw_model::SynthSettings) -> String {
    match daw_model::SynthPreset::matching(settings) {
        Some(preset) => format!("{SYNTH_TRACK} · {}", synth_preset_name(preset)),
        None => format!("{SYNTH_TRACK} · {SYNTH_CUSTOM}"),
    }
}

/// A synth preset's name.
pub fn synth_preset_name(preset: daw_model::SynthPreset) -> &'static str {
    use daw_model::SynthPreset as P;
    match preset {
        P::SoftKeys => "Soft keys",
        P::WarmPad => "Warm pad",
        P::Bass => "Bass",
        P::SubBass => "Sub bass",
        P::Lead => "Lead",
        P::Pluck => "Pluck",
        P::Brass => "Brass",
        P::Plain => "Plain",
    }
}

/// An oscillator wave's name.
pub fn synth_wave_name(wave: daw_model::SynthWave) -> &'static str {
    use daw_model::SynthWave as W;
    match wave {
        W::Sine => "Sine",
        W::Triangle => "Triangle",
        W::Saw => "Saw",
        W::Square => "Square",
    }
}

/// A filter kind's name.
pub fn synth_filter_name(filter: daw_model::SynthFilter) -> &'static str {
    use daw_model::SynthFilter as F;
    match filter {
        F::LowPass24 => "Low-pass, steep",
        F::LowPass12 => "Low-pass, gentle",
        F::HighPass => "High-pass",
        F::BandPass => "Band-pass",
    }
}

/// An LFO shape's name.
pub fn lfo_shape_name(shape: daw_model::LfoShape) -> &'static str {
    use daw_model::LfoShape as S;
    match shape {
        S::Sine => "Smooth",
        S::Triangle => "Triangle",
        S::Square => "Square",
        S::Saw => "Saw",
        S::Random => "Random",
    }
}

/// The name of a synth setting that can change over the song (in the
/// order of [`daw_model::SynthSettings::params`]).
pub fn synth_param_name(index: usize) -> &'static str {
    const NAMES: [&str; 25] = [
        CUTOFF,
        RESONANCE,
        "Filter envelope",
        KEY_TRACKING,
        "Oscillator 1 level",
        "Oscillator 1 fine",
        "Oscillator 2 level",
        "Oscillator 2 fine",
        NOISE,
        PULSE_WIDTH,
        ATTACK,
        DECAY,
        SUSTAIN,
        RELEASE,
        "Filter attack",
        "Filter decay",
        "Filter sustain",
        "Filter release",
        "LFO speed",
        VIBRATO,
        WOBBLE,
        TREMOLO,
        SYNTH_GLIDE,
        VELOCITY_AMOUNT,
        VOLUME,
    ];
    NAMES.get(index).copied().unwrap_or("")
}

/// A synth setting as people read it ("1.2 kHz", "350 ms", "+2.0 oct").
pub fn synth_param_value(index: usize, value: f32) -> String {
    match index {
        0 => hz(value),
        2 | 20 => octaves(value),
        5 | 7 | 19 => cents(value),
        10 | 11 | 13 | 14 | 15 | 17 | 22 => time(value),
        18 => rate(value),
        24 => db(value),
        _ => percent(value),
    }
}

/// A time: "4.0 ms", "350 ms", "1.25 s".
pub fn time(seconds_value: f32) -> String {
    if seconds_value < 1.0 {
        ms(seconds_value * 1000.0)
    } else {
        seconds(seconds_value)
    }
}

/// Octaves: "+2.0 oct".
pub fn octaves(value: f32) -> String {
    if value > 0.04 {
        format!("+{value:.1} oct")
    } else {
        format!("{value:.1} oct")
    }
}

/// Cents: "+7 ct".
pub fn cents(value: f32) -> String {
    #[allow(clippy::cast_possible_truncation, reason = "a display value")]
    let rounded = value.round() as i32;
    if rounded > 0 {
        format!("+{rounded} ct")
    } else {
        format!("{rounded} ct")
    }
}

/// A speed: "5.0 Hz", "0.25 Hz".
pub fn rate(value: f32) -> String {
    if value < 1.0 {
        format!("{value:.2} Hz")
    } else {
        format!("{value:.1} Hz")
    }
}

/// A whole number of steps with its sign: "+1", "0", "-2".
pub fn signed(value: i8) -> String {
    if value > 0 {
        format!("+{value}")
    } else {
        value.to_string()
    }
}

// Plugins.
pub const ADD_INSTRUMENT: &str = "Instrument…";
pub const ADD_INSTRUMENT_HINT: &str =
    "Add a track with another instrument: the included ones, or a plugin";
pub const INSTRUMENTS_TITLE: &str = "Choose an instrument";
pub const SEARCH: &str = "Search";
pub const INCLUDED: &str = "Included";
pub const PLUGINS: &str = "Plugins";
pub const NO_PLUGINS_FOUND: &str =
    "No plugins found yet. CLAP and VST3 plugins installed in the usual folders appear here.";
pub const NOTHING_MATCHES: &str = "Nothing matches";
pub const LOOK_AGAIN: &str = "Look again";
pub const LOOK_AGAIN_HINT: &str = "Look for plugins installed since the program started";
pub const OPEN_PLUGIN: &str = "Open the plugin's window";
pub const CLOSE_PLUGIN: &str = "Close the plugin's window";
pub const NO_PLUGIN_WINDOW: &str = "This plugin has no window of its own.";
pub const PARAMETER: &str = "Parameter";
pub const PLUGIN_MISSING: &str = "not available";
pub const PLUGIN_EFFECT_HINT: &str = "Click to open its window · right-click for more";

/// While plugins are being looked for.
pub fn looking_for_plugins(checked: usize, total: usize) -> String {
    if total == 0 {
        "Looking for plugins…".to_owned()
    } else {
        format!("Looking for plugins… ({checked} of {total})")
    }
}

/// Plugin files that could not be read.
pub fn unreadable_plugins(count: usize) -> String {
    if count == 1 {
        "1 plugin file could not be opened".to_owned()
    } else {
        format!("{count} plugin files could not be opened")
    }
}

/// Why a plugin file cannot be used, in words.
pub fn scan_problem(file: &str, problem: &daw_engine::ScanProblem) -> String {
    use daw_engine::{PluginSystem as S, ScanProblem as P};
    let name = |system: S| match system {
        S::Windows => "Windows",
        S::Linux => "Linux",
        S::Mac => "Mac",
    };
    let here = S::current();
    match problem {
        P::OtherSystem(S::Windows) if here == S::Linux => format!(
            "{file} is made for Windows: install its Linux version (or set it up with yabridge)"
        ),
        P::OtherSystem(system) => format!(
            "{file} is made for {}: install its {} version",
            name(*system),
            name(here)
        ),
        P::TooSlow => format!("{file} took too long to answer when it was checked"),
        P::Crashed => format!("{file} crashed when it was checked"),
        P::NoAnswer => format!("{file} has no plugins inside"),
        P::Failed(reason) => format!("{file} could not be opened ({reason})"),
        P::MissingLibrary(library) => format!(
            "{file} needs {library}, a part of the system that is not installed: install it with your system's software manager (the plugin's install notes name the package)"
        ),
    }
}

/// "and 4 more", after a short list.
pub fn and_more(count: usize) -> String {
    format!("and {count} more")
}

/// Who made a plugin, and its format: "Surge Synth Team · CLAP".
pub fn plugin_detail(vendor: &str, format: &str) -> String {
    if vendor.is_empty() {
        format.to_owned()
    } else {
        format!("{vendor} · {format}")
    }
}

/// A plugin, by its maker.
pub fn plugin_by(vendor: &str) -> String {
    if vendor.is_empty() {
        "A plugin".to_owned()
    } else {
        format!("A plugin by {vendor}")
    }
}

// The Plugins window (decision 0023).
pub const MANAGE_PLUGINS: &str = "Plugins…";
pub const MANAGE_PLUGINS_HINT: &str =
    "Add plugins from files, folders or downloads, and see every plugin found";
pub const PLUGINS_TITLE: &str = "Plugins";
pub const ADD_PLUGIN_FILES: &str = "Add plugin files…";
pub const ADD_PLUGIN_FILES_HINT: &str =
    "Add .vst3 or .clap plugins, downloaded .zip files, or a plugin's installer";
pub const ADD_PLUGIN_FOLDER: &str = "Add a plugin folder…";
pub const ADD_PLUGIN_FOLDER_HINT: &str = "Add a .vst3 plugin (which is a folder) or a folder with plugins inside: they are copied into the program's own plugin folder";
pub const DROP_PLUGINS_HINT: &str =
    "You can also drop plugin files, folders or downloaded .zip files onto the window.";
pub const PLUGINS_BUSY: &str = "Adding plugins…";
pub const INSTRUMENTS_HEADING: &str = "Instruments";
pub const EFFECTS_HEADING: &str = "Effects";
pub const REMOVE: &str = "Remove";
pub const WINDOWS_PLUGINS_WAITING: &str = "Windows plugins waiting for yabridge";
pub const WINDOWS_PLUGINS_HEADING: &str = "Windows plugins: not set up";
pub const WINDOWS_PLUGINS_READY_HEADING: &str = "Windows plugins: ready";
pub const WINDOWS_PLUGINS_READY: &str = "Wine and yabridge are installed: Windows plugins you add here are set up for this program automatically.";
pub const WINDOWS_PLUGINS_STEPS: &[&str] = &[
    "1. Install Wine from your distribution (for instance: sudo apt install wine).",
    "2. Download yabridge from github.com/robbert-vdh/yabridge/releases and unpack it into ~/.local/share/yabridge.",
    "3. Click Check again, then add your Windows plugins here.",
];
pub const CHECK_AGAIN: &str = "Check again";
pub const CHECK_AGAIN_HINT: &str =
    "Look for Wine and yabridge again, and set up the Windows plugins added here";
pub const FOLDERS_SEARCHED: &str = "Folders searched";
pub const OWN_PLUGIN_FOLDER: &str = "Plugins added here are kept in the program's own folder.";
pub const SHOW_FOLDER: &str = "Show";
pub const STOP_SEARCHING_FOLDER: &str = "Stop searching this folder";
pub const SEARCH_ANOTHER_FOLDER: &str = "Search another folder…";
pub const SEARCH_ANOTHER_FOLDER_HINT: &str =
    "Look for plugins in a folder of your own, where they are (nothing is copied)";
pub const PLUGIN_FILE_TYPE: &str = "Plugins, downloads and installers";
pub const CHOOSE_PLUGINS_TITLE: &str = "Add plugins";

/// A heading with how many things are under it: "Effects (4)".
pub fn counted(what: &str, count: usize) -> String {
    format!("{what} ({count})")
}

/// The hint of a plugin's Remove button.
pub fn remove_plugin(name: &str) -> String {
    format!("Remove {name} from the program's plugin folder")
}

/// What is missing for Windows plugins on Linux.
pub fn windows_plugins_missing(wine: bool, yabridge: bool) -> String {
    match (wine, yabridge) {
        (false, false) => "To use Windows plugins on Linux, install Wine and yabridge:".to_owned(),
        (true, false) => "Wine is installed, yabridge is missing:".to_owned(),
        (false, true) => "yabridge is installed, Wine is missing:".to_owned(),
        (true, true) => WINDOWS_PLUGINS_READY.to_owned(),
    }
}

/// A short list of names: "A, B and C", or "A, B, C and 4 more".
fn name_list(names: &[String]) -> String {
    const SHOWN: usize = 5;
    match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        n if n <= SHOWN => format!("{} and {}", names[..n - 1].join(", "), names[n - 1]),
        n => format!("{} and {} more", names[..SHOWN].join(", "), n - SHOWN),
    }
}

/// The file name of a path, for messages.
fn file_name(path: &std::path::Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// What adding, removing or setting up plugins did, line by line.
pub fn plugin_job_lines(done: &crate::plugin_manager::PluginJobDone) -> Vec<String> {
    use crate::plugin_manager::{InstallerRun, file_title};
    use daw_plugin_host::install::Skip;
    let titles = |paths: &[std::path::PathBuf]| -> Vec<String> {
        paths.iter().map(|p| file_title(p)).collect()
    };
    let mut lines = Vec::new();
    if !done.added.is_empty() {
        lines.push(format!("Added: {}.", name_list(&titles(&done.added))));
    }
    if !done.windows.is_empty() {
        let names = name_list(&titles(&done.windows));
        lines.push(match &done.bridge {
            Some(Ok(())) => format!("Set up with yabridge: {names}."),
            Some(Err(why)) => {
                format!("{names}: Windows plugins that yabridge could not set up ({why}).")
            }
            None => format!(
                "{names}: Windows plugins. Install Wine and yabridge to use them (see Windows plugins below)."
            ),
        });
    } else if let Some(bridge) = &done.bridge {
        lines.push(match bridge {
            Ok(()) => "yabridge has set up the Windows plugins added here.".to_owned(),
            Err(why) => format!("yabridge could not set up the Windows plugins ({why})."),
        });
    }
    for (path, run) in &done.installers {
        let file = file_name(path);
        lines.push(match run {
            InstallerRun::Opened => {
                format!("{file}: the installer has opened. When it has finished, click Look again.")
            }
            InstallerRun::Finished => format!("{file}: the installer has finished."),
            InstallerRun::Failed(why) => format!("{file}: the installer could not run ({why})."),
            InstallerRun::NeedsWine => format!(
                "{file} is a Windows installer: install Wine to run it (see Windows plugins below)."
            ),
        });
    }
    for (path, skip) in &done.skipped {
        let file = file_name(path);
        lines.push(match skip {
            Skip::NotAPlugin => format!("{file}: no plugin found inside."),
            Skip::Vst2 => format!(
                "{file} is a VST2 plugin: those are not supported yet. Look for its VST3 or CLAP version."
            ),
            Skip::Installer => format!("{file} is an installer: run it, then click Look again."),
            Skip::OtherSystem(system) => {
                let name = |s: daw_engine::PluginSystem| match s {
                    daw_engine::PluginSystem::Windows => "Windows",
                    daw_engine::PluginSystem::Linux => "Linux",
                    daw_engine::PluginSystem::Mac => "Mac",
                };
                format!(
                    "{file} is made for {}: get its {} version.",
                    name(*system),
                    name(daw_engine::PluginSystem::current())
                )
            }
            Skip::Failed(why) => format!("{file} could not be added ({why})."),
        });
    }
    if !done.removed.is_empty() {
        lines.push(format!("Removed: {}.", name_list(&titles(&done.removed))));
    }
    if let Some((path, why)) = &done.remove_failed {
        lines.push(format!(
            "{} could not be removed ({why}).",
            file_title(path)
        ));
    }
    if done.bridge_missing {
        lines.push(
            "Not ready yet: install what is missing (see Windows plugins below), then click Check again."
                .to_owned(),
        );
    }
    lines
}

/// What a plugin track plays, under its name.
pub fn plugin_track(name: &str, missing: bool) -> String {
    if missing {
        format!("{name} ({PLUGIN_MISSING})")
    } else {
        name.to_owned()
    }
}

/// The title of a plugin's window: the plugin, then where it is.
pub fn plugin_window_title(plugin: &str, place: &str) -> String {
    format!("{plugin} — {place}")
}

// Automation.
pub const AUTOMATION: &str = "Automation";
pub const AUTOMATION_HINT: &str =
    "Make a setting change over the song: it is drawn on a lane under the track";
pub const LANE_HINT: &str = "Click to add a point · drag to move · right-click to remove";
pub const REMOVE_LANE: &str = "Remove this lane";
pub const LAST_MOVED: &str = "Last moved in the plugin";
pub const SEARCH_SETTINGS: &str = "Search settings";
pub const AUTOMATED_HINT: &str = "Follows its automation lane while the song plays";
pub const EDIT_ADD_LANE: &str = "add automation lane";
pub const EDIT_REMOVE_LANE: &str = "remove automation lane";
pub const EDIT_ADD_POINT: &str = "add automation point";
pub const EDIT_MOVE_POINT: &str = "move automation point";
pub const EDIT_REMOVE_POINT: &str = "remove automation point";

/// A lane's name: the setting, after what it belongs to ("Reverb · Amount").
pub fn lane_name(owner: &str, setting: &str) -> String {
    if owner.is_empty() {
        setting.to_owned()
    } else {
        format!("{owner} · {setting}")
    }
}

/// An effect window's title.
pub fn effect_title(effect: &str, place: &str) -> String {
    format!("{effect} — {place}")
}

// Effect controls.
pub const LOW_CUT: &str = "Low cut";
pub const HIGH_CUT: &str = "High cut";
pub const LOW: &str = "Bass";
pub const MID: &str = "Middle";
pub const HIGH: &str = "Treble";
pub const FREQUENCY: &str = "Frequency";
pub const GAIN: &str = "Gain";
pub const WIDTH_Q: &str = "Width";
pub const THRESHOLD: &str = "Threshold";
pub const RATIO: &str = "Ratio";
pub const ATTACK: &str = "Attack";
pub const RELEASE: &str = "Release";
pub const KNEE: &str = "Softness";
pub const MAKEUP: &str = "Make-up gain";
pub const ROOM_SIZE: &str = "Room size";
pub const DAMPING: &str = "Damping";
pub const STEREO_WIDTH: &str = "Width";
pub const AMOUNT: &str = "Amount";
pub const PRE_DELAY: &str = "Pre-delay";
pub const TIME: &str = "Time";
pub const IN_TIME: &str = "In time with the song";
pub const REPEATS: &str = "Repeats";
pub const TONE: &str = "Tone";
pub const PING_PONG: &str = "Bounce left and right";
pub const CEILING: &str = "Ceiling";
pub const DRIVE: &str = "Loudness";
pub const EQ_CURVE: &str = "EQ curve";
pub const THRESHOLD_HINT: &str = "Sound louder than this is turned down";
pub const RATIO_HINT: &str =
    "How strongly: 4:1 lets 1 dB through for every 4 dB above the threshold";
pub const DAMPING_HINT: &str = "Higher makes the room sound darker";
pub const CEILING_HINT: &str = "The output never goes above this";
pub const DRIVE_HINT: &str = "Turns the sound up into the limiter";
pub const REPEATS_HINT: &str = "How much of each echo comes back again";
pub const TONE_HINT: &str = "Lower makes the echoes darker";

/// Decibels: "-3.0 dB", "+6.0 dB".
pub fn db(value: f32) -> String {
    if value <= -60.0 {
        "−∞ dB".to_owned()
    } else if value > 0.04 {
        format!("+{value:.1} dB")
    } else {
        format!("{value:.1} dB")
    }
}

/// A frequency: "80 Hz", "1.2 kHz".
pub fn hz(value: f32) -> String {
    if value >= 1000.0 {
        format!("{:.1} kHz", value / 1000.0)
    } else {
        format!("{value:.0} Hz")
    }
}

/// Milliseconds: "12 ms".
pub fn ms(value: f32) -> String {
    if value < 10.0 {
        format!("{value:.1} ms")
    } else {
        format!("{value:.0} ms")
    }
}

/// A share as a percentage: "35%".
pub fn percent(value: f32) -> String {
    format!("{:.0}%", value * 100.0)
}

/// A compression ratio: "4:1".
pub fn ratio(value: f32) -> String {
    if value >= 19.95 {
        "∞:1".to_owned()
    } else {
        format!("{value:.1}:1")
    }
}

/// A pan position: "L 30", "Centre", "R 30".
pub fn pan(value: f32) -> String {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a percentage from -100 to 100"
    )]
    let amount = (value * 100.0).round() as i32;
    match amount {
        0 => "Centre".to_owned(),
        a if a < 0 => format!("L {}", -a),
        a => format!("R {a}"),
    }
}

/// A tuning in semitones: "+2 st".
pub fn semitones(value: f32) -> String {
    if value.abs() < 0.05 {
        "0 st".to_owned()
    } else if value > 0.0 {
        format!("+{value:.1} st")
    } else {
        format!("{value:.1} st")
    }
}

/// A Q value as a width.
pub fn q(value: f32) -> String {
    format!("Q {value:.2}")
}

/// Seconds: "1.25 s".
pub fn seconds(value: f32) -> String {
    format!("{value:.2} s")
}

// Export (decision 0020).
pub const EXPORT: &str = "Export…";
pub const EXPORT_TITLE: &str = "Export the song";
pub const EXPORT_HINT: &str = "Choose what to make of the song.";
pub const EXPORT_SOUND: &str = "A sound file";
pub const EXPORT_SOUND_HINT: &str = "The whole song as you hear it, with its echoes and reverb fading out at the end: to play anywhere or share.";
pub const EXPORT_WAV: &str = "WAV";
pub const EXPORT_WAV_HINT: &str = "Plays everywhere";
pub const EXPORT_FLAC: &str = "FLAC";
pub const EXPORT_FLAC_HINT: &str = "About half the size, with nothing lost";
pub const EXPORT_DAWPROJECT: &str = "A project for another music program";
pub const EXPORT_DAWPROJECT_HINT: &str = "DAWproject: tracks, notes, sound, mixer, plugins and automation, for Bitwig Studio, Studio One, Cubase and others.";
pub const EXPORT_MIDI: &str = "The notes, as a MIDI file";
pub const EXPORT_MIDI_HINT: &str = "Notes, tempo and time signatures, for any music program.";
pub const EXPORT_CD: &str = "CD quality";
pub const EXPORT_CD_HINT: &str = "16-bit, 44.1 kHz — plays everywhere";
pub const EXPORT_STUDIO: &str = "Studio quality";
pub const EXPORT_STUDIO_HINT: &str = "24-bit, 48 kHz — for further work";
pub const EXPORT_BUTTON: &str = "Choose where and export…";
pub const EXPORTING: &str = "Exporting the song…";
pub const EXPORT_CANCELLED: &str = "Export cancelled";
pub const EXPORT_FILE_TYPE: &str = "WAV sound file";
pub const FLAC_FILE_TYPE: &str = "FLAC sound file";
pub const DAWPROJECT_FILE_TYPE: &str = "DAWproject file";
pub const MIDI_FILE_TYPE: &str = "MIDI file";
/// Every kind of file Open takes.
pub const ANY_SONG_FILE_TYPE: &str = "Songs";

/// The file type an export writes, for the save dialog.
pub fn export_file_type(target: crate::export::ExportTarget) -> &'static str {
    use crate::export::ExportTarget as T;
    match target {
        T::Wav => EXPORT_FILE_TYPE,
        T::Flac => FLAC_FILE_TYPE,
        T::DawProject => DAWPROJECT_FILE_TYPE,
        T::Midi => MIDI_FILE_TYPE,
    }
}

pub fn exported(path: &str) -> String {
    format!("Exported {path}")
}

pub fn could_not_export(error: &str) -> String {
    format!("Could not export the song: {error}")
}

// ----- Audio (phase 4, decision 0017)

/// The name of a new audio track.
pub const AUDIO_TRACK: &str = "Audio";
pub const ADD_AUDIO: &str = "Audio";
pub const ADD_AUDIO_HINT: &str =
    "Add a track for recording from a microphone or importing sound files";
pub const RECORD_SOUND: &str = "Record sound";
pub const IMPORT_AUDIO: &str = "Import…";
pub const USER_GUIDE: &str = "User guide";
pub const IMPORT_FILE_TYPE: &str = "Sound and MIDI files";
pub const IMPORT_TITLE: &str = "Import sound or MIDI files";
pub const EDIT_IMPORT_MIDI: &str = "import MIDI file";
/// A track named by its number ("Track 3").
pub const TRACK: &str = "Track";
pub const NO_INPUT_DEVICE: &str = "No microphone or sound input was found. Plug one in, or allow this program to use it, then record again.";
pub const EDIT_ADD_AUDIO_TRACK: &str = "add audio track";
pub const EDIT_IMPORT: &str = "import sound";
pub const EDIT_RECORD_SOUND: &str = "record sound";
pub const EDIT_SPLIT_CLIP: &str = "split clip";
pub const EDIT_TRIM_CLIP: &str = "trim clip";
pub const EDIT_FADE: &str = "change fade";
pub const EDIT_CLIP_VOLUME: &str = "change clip volume";
pub const SPLIT_HERE: &str = "Split here";
pub const SPLIT_AT_PLAY: &str = "Split at the play position";
pub const CLIP_VOLUME: &str = "Volume";
pub const CLIP_VOLUME_HINT: &str = "How loud this clip plays, before the track's own volume";
pub const FADE_IN_HINT: &str = "Drag to fade in";
pub const FADE_OUT_HINT: &str = "Drag to fade out";
pub const NO_INPUT: &str = "No sound came in from the microphone. Check that one is connected, switched on and allowed for this program.";
pub const LOADING_SOUND: &str = "Loading…";
pub const DROP_HERE: &str = "Drop sound files to import them";
pub const AUDIO_HINT: &str = "Press Record to record from the microphone, or drop sound files here";
pub const LAST_INSTRUMENT: &str = "A song keeps at least one instrument track";
pub const INPUT_LEVEL_HINT: &str = "How loud the microphone is: keep it out of the red";
pub const RECORDING_SKIPPED: &str =
    "The computer fell behind while recording: parts of the take are silent.";

/// The name of a recorded take.
pub fn recording_name(n: usize) -> String {
    format!("Recording {n}")
}

/// Said when a take of sound is added to the song.
pub fn recorded_sound(seconds: f64) -> String {
    format!("Recorded {seconds:.1} seconds of sound")
}

pub fn importing(name: &str) -> String {
    format!("Importing {name}…")
}

pub fn imported(name: &str) -> String {
    format!("Imported {name}")
}

pub fn could_not_import(name: &str, error: &str) -> String {
    format!("Could not import {name}: {error}")
}

/// Said after a MIDI file's tracks were added to the song.
/// Says a MIDI file's notes went into a track the song has (decision 0034).
pub fn imported_midi_into(name: &str, track: &str) -> String {
    format!("Imported {name} into {track}")
}

pub fn imported_midi(name: &str, tracks: usize) -> String {
    match tracks {
        1 => format!("Imported {name}: one track of notes"),
        n => format!("Imported {name}: {n} tracks of notes"),
    }
}

pub fn no_notes_in(name: &str) -> String {
    format!("{name} has no notes to import")
}

/// Said after another program's song was opened, with what it could not
/// bring.
pub fn opened_other(name: &str, left_out: &[daw_formats::dawproject::LeftOut]) -> String {
    use daw_formats::dawproject::LeftOut as L;
    let mut parts: Vec<String> = Vec::new();
    let devices: Vec<&str> = left_out
        .iter()
        .filter_map(|l| match l {
            L::Device(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();
    if !devices.is_empty() {
        parts.push(format!(
            "devices {} cannot play ({})",
            daw_ident::PRODUCT_NAME,
            devices.join(", ")
        ));
    }
    for item in left_out {
        match item {
            L::Device(_) => {}
            L::Routing => parts.push("group tracks and sends".to_owned()),
            L::StretchedAudio => {
                parts.push("time-stretching (the sound plays at its own speed)".to_owned());
            }
            L::MissingAudio(file) => parts.push(format!("the missing sound file \"{file}\"")),
            L::Scenes => parts.push("clip launcher scenes".to_owned()),
        }
    }
    if parts.is_empty() {
        format!("Opened {name} as a new song: save it to keep it")
    } else {
        format!(
            "Opened {name} as a new song. Left out: {}",
            parts.join("; ")
        )
    }
}

pub fn could_not_record(error: &str) -> String {
    format!("Could not keep the recording: {error}")
}

pub fn unreadable_sound(name: &str, error: &str) -> String {
    format!("The sound of \"{name}\" could not be read: {error}")
}

// ----- Theory and rhythm (decision 0018)

pub const EDIT_ADD_CHORD: &str = "place chord";
pub const EDIT_INVERT: &str = "invert chord";
pub const EDIT_SWING: &str = "change swing";
pub const EDIT_KEY: &str = "change key";
pub const EDIT_ADD_TEMPO_CHANGE: &str = "add tempo change";
pub const EDIT_REMOVE_TEMPO_CHANGE: &str = "remove tempo change";
pub const EDIT_MOVE_TEMPO_CHANGE: &str = "move tempo change";
pub const EDIT_GLIDE: &str = "change tempo glide";
pub const EDIT_GLIDE_SHAPE: &str = "bend tempo glide";
pub const EDIT_ADD_VOICE: &str = "add voice track";
pub const EDIT_LYRICS: &str = "change lyrics";
pub const EDIT_MOVE_SIGNATURE: &str = "move signature change";
pub const EDIT_REMOVE_SIGNATURE: &str = "remove signature change";

pub const NOTHING_TO_INVERT: &str = "Select a chord first: two or more notes that start together";
pub const WOULD_LEAVE_KEYBOARD: &str = "That would move notes off the keyboard";

/// Said after a chord stamp places a chord.
pub fn placed(name: &str) -> String {
    format!("Placed {name}")
}

/// Tonic names, spelled the way musicians spell major keys (D flat major)
/// and minor keys (C sharp minor). ASCII signs, like note names.
const MAJOR_TONICS: [&str; 12] = [
    "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];
const MINOR_TONICS: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "G#", "A", "Bb", "B",
];

/// A pitch class's name as the tonic of a key of this kind.
pub fn tonic_name(tonic: u8, kind: daw_model::ScaleKind) -> &'static str {
    let names = if kind.is_minor() {
        &MINOR_TONICS
    } else {
        &MAJOR_TONICS
    };
    names[usize::from(tonic % 12)]
}

/// A scale's name: "major", "harmonic minor", "Dorian".
pub fn scale_kind_name(kind: daw_model::ScaleKind) -> &'static str {
    use daw_model::ScaleKind as K;
    match kind {
        K::Major => "major",
        K::Minor => "minor",
        K::HarmonicMinor => "harmonic minor",
        K::MelodicMinor => "melodic minor",
        K::Dorian => "Dorian",
        K::Phrygian => "Phrygian",
        K::Lydian => "Lydian",
        K::Mixolydian => "Mixolydian",
        K::Locrian => "Locrian",
        K::MajorPentatonic => "major pentatonic",
        K::MinorPentatonic => "minor pentatonic",
        K::Blues => "blues",
    }
}

/// A key's name: "A minor", or "No key".
pub fn song_key_name(scale: Option<daw_model::Scale>) -> String {
    match scale {
        Some(scale) => format!(
            "{} {}",
            tonic_name(scale.tonic, scale.kind),
            scale_kind_name(scale.kind)
        ),
        None => NO_KEY.to_owned(),
    }
}

pub const KEY: &str = "Key";
pub const NO_KEY: &str = "No key";
pub const KEY_HINT: &str = "The song's key at the play position. With a key, the piano roll shades \
     the notes outside it and chord stamps build chords in it.";
pub const TONIC: &str = "Tonic";
pub const SCALE: &str = "Scale";

/// A chord's name: "D minor", "G7", "Fmaj7".
pub fn chord_name(root: u8, kind: daw_model::ChordKind) -> String {
    use daw_model::ChordKind as K;
    let r = NOTE_NAMES[usize::from(root % 12)];
    match kind {
        K::Major => format!("{r} major"),
        K::Minor => format!("{r} minor"),
        K::Diminished => format!("{r} diminished"),
        K::Augmented => format!("{r} augmented"),
        K::Sus2 => format!("{r}sus2"),
        K::Sus4 => format!("{r}sus4"),
        K::Power => format!("{r}5"),
        K::Sixth => format!("{r}6"),
        K::MinorSixth => format!("{r}m6"),
        K::Seventh => format!("{r}7"),
        K::MajorSeventh => format!("{r}maj7"),
        K::MinorSeventh => format!("{r}m7"),
        K::HalfDiminished => format!("{r}m7b5"),
        K::DiminishedSeventh => format!("{r}dim7"),
        K::AddNine => format!("{r}add9"),
        K::Ninth => format!("{r}9"),
        K::MajorNinth => format!("{r}maj9"),
        K::MinorNinth => format!("{r}m9"),
    }
}

/// A chord kind's name in the Draw menu.
pub fn chord_kind_name(kind: daw_model::ChordKind) -> &'static str {
    use daw_model::ChordKind as K;
    match kind {
        K::Major => "Major",
        K::Minor => "Minor",
        K::Diminished => "Diminished",
        K::Augmented => "Augmented",
        K::Sus2 => "Suspended second (sus2)",
        K::Sus4 => "Suspended fourth (sus4)",
        K::Power => "Power chord (5)",
        K::Sixth => "Sixth (6)",
        K::MinorSixth => "Minor sixth (m6)",
        K::Seventh => "Seventh (7)",
        K::MajorSeventh => "Major seventh (maj7)",
        K::MinorSeventh => "Minor seventh (m7)",
        K::HalfDiminished => "Half-diminished (m7b5)",
        K::DiminishedSeventh => "Diminished seventh (dim7)",
        K::AddNine => "Added ninth (add9)",
        K::Ninth => "Ninth (9)",
        K::MajorNinth => "Major ninth (maj9)",
        K::MinorNinth => "Minor ninth (m9)",
    }
}

/// What the Draw button says, short so the location line fits: "Notes",
/// "Triads in key", "maj7", with the inversion if there is one.
pub fn draw_button(draw: crate::session::Draw, inversion: usize) -> String {
    use crate::session::Draw;
    use daw_model::{ChordKind as K, ChordShape};
    let name = match draw {
        Draw::Note => return DRAW_NOTES_SHORT.to_owned(),
        Draw::Chord(ChordShape::InKey(3)) => "Triads in key",
        Draw::Chord(ChordShape::InKey(_)) => "7ths in key",
        Draw::Chord(ChordShape::Kind(kind)) => match kind {
            K::Major => "Major",
            K::Minor => "Minor",
            K::Diminished => "dim",
            K::Augmented => "aug",
            K::Sus2 => "sus2",
            K::Sus4 => "sus4",
            K::Power => "5",
            K::Sixth => "6",
            K::MinorSixth => "m6",
            K::Seventh => "7",
            K::MajorSeventh => "maj7",
            K::MinorSeventh => "m7",
            K::HalfDiminished => "m7b5",
            K::DiminishedSeventh => "dim7",
            K::AddNine => "add9",
            K::Ninth => "9",
            K::MajorNinth => "maj9",
            K::MinorNinth => "m9",
        },
    };
    match inversion {
        0 => name.to_owned(),
        n => format!("{name}, inv. {n}"),
    }
}

pub const DRAW: &str = "Draw";
pub const DRAW_NOTES_SHORT: &str = "Notes";
pub const DRAW_HINT: &str = "What a click places: one note, or a whole chord on the note you click";
pub const DRAW_NOTES: &str = "Single notes";
pub const TRIADS_IN_KEY: &str = "Triads in the key";
pub const SEVENTHS_IN_KEY: &str = "Seventh chords in the key";
pub const IN_KEY_NEEDS_KEY: &str = "Choose the song's key first (Key, in the bar at the top)";
pub const CHORD_KINDS: &str = "Chords of one kind";
pub const INVERSION: &str = "Inversion";

/// An inversion's name.
pub fn inversion_name(inversion: usize) -> &'static str {
    match inversion {
        0 => "Root position",
        1 => "1st inversion",
        2 => "2nd inversion",
        _ => "3rd inversion",
    }
}

pub const KEEP_IN_KEY: &str = "Keep notes in the key";
pub const KEEP_IN_KEY_HINT: &str = "New and moved notes land on notes of the song's key, and the \
     up and down arrows move by steps of the key";
pub const KEEP_IN_KEY_NEEDS_KEY: &str = "Choose the song's key first";
pub const IN_KEY: &str = "In key";
pub const STRAIGHT_GRIDS: &str = "Straight";
pub const TRIPLETS: &str = "Triplets";
pub const QUINTUPLETS: &str = "Quintuplets";
pub const SEPTUPLETS: &str = "Septuplets";
pub const DOTTED: &str = "Dotted";

/// An interval's name, in semitones from 1 to 12.
pub fn interval_name(semitones: u8) -> &'static str {
    match semitones {
        1 => "a minor second",
        2 => "a major second",
        3 => "a minor third",
        4 => "a major third",
        5 => "a perfect fourth",
        6 => "a tritone",
        7 => "a perfect fifth",
        8 => "a minor sixth",
        9 => "a major sixth",
        10 => "a minor seventh",
        11 => "a major seventh",
        _ => "an octave",
    }
}

/// A number of steps of the key, from 1 to 7: "a step", "a third".
pub fn steps_name(steps: u8) -> &'static str {
    match steps {
        1 => "a step",
        2 => "a third",
        3 => "a fourth",
        4 => "a fifth",
        5 => "a sixth",
        6 => "a seventh",
        _ => "an octave",
    }
}

/// A transposition menu entry: "Up a perfect fifth", "Down a third".
pub fn by_interval(up: bool, name: &str) -> String {
    if up {
        format!("Up {name}")
    } else {
        format!("Down {name}")
    }
}

pub const NOTES_MENU: &str = "Transpose";
pub const NOTES_MENU_HINT: &str = "Transpose and invert the selected notes";
pub const TRANSPOSE_UP: &str = "Transpose up";
pub const TRANSPOSE_DOWN: &str = "Transpose down";
pub const TRANSPOSE_IN_KEY: &str = "Transpose in the key";
pub const INVERT_UP: &str = "Invert up";
pub const INVERT_DOWN: &str = "Invert down";
pub const SELECT_NOTES_FIRST: &str = "Select notes first";

/// What the swing button says: "Straight" or "Swing 54%".
pub fn swing_name(percent: u8) -> String {
    if percent == 0 {
        STRAIGHT.to_owned()
    } else {
        format!("Swing {percent}%")
    }
}

pub const SWING: &str = "Swing";
pub const STRAIGHT: &str = "Straight";
pub const SWING_HINT: &str = "How late every second eighth or sixteenth plays: 0% is straight, \
     67% the triplet feel, 100% a dotted rhythm. The notes stay where they are written.";
pub const SWING_AMOUNT: &str = "Swing amount";
pub const SWING_EIGHTHS: &str = "On eighths";
pub const SWING_SIXTEENTHS: &str = "On sixteenths";
pub const TRIPLET_FEEL: &str = "Triplet feel";

/// A pattern's length that is not whole bars: "3 beats", "12 steps".
pub fn beats(count: i64) -> String {
    if count == 1 {
        "1 beat".to_owned()
    } else {
        format!("{count} beats")
    }
}

/// A pattern's length in steps of the step grid: "12 steps".
pub fn steps(count: i64) -> String {
    if count == 1 {
        "1 step".to_owned()
    } else {
        format!("{count} steps")
    }
}

pub const PATTERN_END: &str = "End of the pattern";
pub const PATTERN_END_HINT: &str = "Drag to make the pattern end on any step: a pattern of \
     another length turns against the others as it repeats";
pub const OTHER_SIGNATURE: &str = "Other…";
pub const BEATS: &str = "Beats";
pub const NOTE_VALUE: &str = "Note value";

pub const SIGNATURE_ROW: &str = "Tempo and signatures";
pub const SIGNATURE_ROW_HINT: &str = "Changes of tempo, time signature and key along the song";
pub const SIGNATURES: &str = "Signatures";
pub const SIGNATURES_HINT: &str =
    "Click a bar to change the time signature or the key from there; drag a change to move it";
pub const TEMPO_LANE: &str = "Tempo";
pub const SIGNATURE_TAGS: &str = "Time-signature and key changes";
pub const TEMPO_LINE: &str = "Tempo changes";
pub const TEMPO_LANE_HINT: &str = "Click to add a tempo change; drag a point to change it, \
     click it for more, right-click it to remove it. Drag the handle in the middle of a glide \
     to bend it";
pub const SHOW_SIGNATURES: &str = "Show tempo and signature changes";
pub const HIDE_SIGNATURES: &str = "Hide tempo and signature changes";
pub const SAME_AS_BEFORE: &str = "Same as before";
pub const REMOVE_CHANGES: &str = "Remove these changes";
pub const GLIDE: &str = "Glide to the next tempo";
pub const GLIDE_HINT: &str = "Speed up or slow down gradually until the next tempo change";
pub const ARRIVE_GRADUALLY: &str = "Arrive gradually";
pub const ARRIVE_GRADUALLY_HINT: &str =
    "Speed up or slow down gradually from the previous tempo change to this one";
/// Over a glide that slows down, as a score writes it.
pub const RIT: &str = "rit.";
/// Over a glide that speeds up.
pub const ACCEL: &str = "accel.";
pub const GLIDE_SHAPE: &str = "Shape";
pub const GLIDE_EARLY: &str = "Early";
pub const GLIDE_EARLY_HINT: &str =
    "Most of the change comes first, then the tempo settles into the new one";
pub const GLIDE_EVEN: &str = "Even";
pub const GLIDE_EVEN_HINT: &str = "The tempo moves by the same amount on every beat";
pub const GLIDE_LATE: &str = "Late";
pub const GLIDE_LATE_HINT: &str = "The tempo holds back, then changes more and more towards the \
     end: a ritardando that gathers, the way a runner slows to a stop";
pub const GLIDE_SHAPE_HINT: &str = "Or drag the handle in the middle of the glide up or down";
pub const MAKE_SUDDEN: &str = "Change suddenly instead";
pub const REMOVE_TEMPO_CHANGE: &str = "Remove this tempo change";
pub const TEMPO_CHANGE: &str = "Tempo change";

/// The summary of the signature row: "120 BPM · 4/4 · C major".
pub fn signature_summary(bpm: f64, numerator: u8, denominator: u8, key: &str) -> String {
    format!(
        "{} {BPM}  ·  {}  ·  {key}",
        self::bpm(bpm),
        time_signature(numerator, denominator)
    )
}

/// The title of the panel of a bar's changes: "From bar 9".
pub fn from_bar(bar: u32) -> String {
    format!("From bar {}", bar + 1)
}

/// The label of a signature tag, for screen readers: "Bar 9: 7/8, A minor".
pub fn signature_tag(bar: u32, what: &str) -> String {
    format!("Bar {}: {what}", bar + 1)
}

/// The handle of a glide, and the title of its panel: "rit., bars 5 to 9".
pub fn glide_handle(slows: bool, from_bar: u32, to_bar: u32) -> String {
    let word = if slows { RIT } else { ACCEL };
    if from_bar == to_bar {
        format!("{word}, bar {}", from_bar + 1)
    } else {
        format!("{word}, bars {} to {}", from_bar + 1, to_bar + 1)
    }
}

/// The tempos a glide goes between: "100 → 60 BPM".
pub fn glide_tempos(bpm: f64, next_bpm: f64) -> String {
    format!("{} → {} {BPM}", self::bpm(bpm), self::bpm(next_bpm))
}

/// The label of a tempo point, for screen readers: "Tempo 96 at bar 5".
pub fn tempo_point(bpm: f64, bar: u32) -> String {
    format!("Tempo {} at bar {}", self::bpm(bpm), bar + 1)
}

// ----- Singing expression (decision 0028)

/// The switch on a voice track's location line: what the piano roll edits.
pub const NOTES_MODE: &str = "Notes";
pub const PITCH_MODE: &str = "Pitch";
/// Their labels for screen readers.
pub const EDIT_THE_NOTES: &str = "Edit the notes";
pub const EDIT_THE_PITCH: &str = "Edit the sung pitch";
pub const NOTES_MODE_HINT: &str = "Place, move and stretch notes";
pub const PITCH_MODE_HINT: &str = "Shape the sung pitch: click over a note to bend it there, drag \
    a point, click a point for its details, right-click to remove it; drag a vibrato's handle. \
    The notes stay where they are.";
pub const PITCH_LINE_HINT: &str =
    "Click over a note to bend its pitch there · drag a point · right-click to remove";

/// The lane under a voice track's notes.
pub const COLOUR: &str = "Colour";
pub const BREATH: &str = "Breath";
pub const LANE_COLOUR: &str = "Colour lane";
pub const LANE_BREATH: &str = "Breath lane";
pub const SHOW_COLOUR: &str = "Show how light or deep each note is sung, without changing its \
    pitch: up is lighter and brighter, down darker and deeper";
pub const SHOW_BREATH: &str = "Show how breathy each note is sung";
pub const SHOW_VELOCITY_SUNG: &str = "Show how loud each note is sung";

/// The singing commands of the notes menu.
pub const SINGING: &str = "Singing";
pub const ADD_VIBRATO: &str = "Add vibrato";
pub const ADD_VIBRATO_HINT: &str =
    "Long notes swing a little once they are established; short notes stay plain";
pub const REMOVE_VIBRATO: &str = "Remove vibrato";
pub const PLAIN_PITCH: &str = "Plain pitch";
pub const PLAIN_PITCH_HINT: &str = "Back to a plain glide into each note: the points go";

/// The panel of a pitch point.
pub const PITCH_POINT: &str = "Pitch point";
pub const POINT_TIME: &str = "Time";
pub const POINT_TIME_HINT: &str = "From the note's start (negative: before it)";
pub const MS: &str = "ms";
pub const POINT_PITCH: &str = "Pitch";
pub const POINT_PITCH_HINT: &str = "From the note's own pitch (100 cents to a semitone)";
pub const CENTS: &str = "cents";
pub const POINT_LEAVES: &str = "The voice leaves from wherever it is: move this point in time only";
pub const CURVE_AFTER: &str = "Curve after it";
pub const SHAPE_SMOOTH: &str = "Smooth";
pub const SHAPE_STRAIGHT: &str = "Straight";
pub const SHAPE_SLOW_START: &str = "Slow start";
pub const SHAPE_SLOW_END: &str = "Slow end";
pub const SHAPE_SMOOTH_HINT: &str = "Leaves gently and arrives gently";
pub const SHAPE_STRAIGHT_HINT: &str = "Changes evenly";
pub const SHAPE_SLOW_START_HINT: &str = "Leaves gently, then arrives quickly";
pub const SHAPE_SLOW_END_HINT: &str = "Leaves quickly, then arrives gently";
pub const REMOVE_POINT: &str = "Remove the point";

/// The panel of a note's vibrato (its title is the synth's `VIBRATO`).
pub const VIBRATO_DEPTH: &str = "Depth";
pub const VIBRATO_DEPTH_HINT: &str = "How far the pitch swings either side";
pub const VIBRATO_RATE: &str = "Rate";
pub const VIBRATO_RATE_HINT: &str = "How many swings a second";
pub const HZ: &str = "Hz";
pub const VIBRATO_START: &str = "Starts after";
pub const VIBRATO_START_HINT: &str = "How long into the note the vibrato starts";
pub const VIBRATO_GROWS: &str = "Grows over";
pub const VIBRATO_GROWS_HINT: &str = "How long it takes to reach its full depth";
pub const REMOVE_THE_VIBRATO: &str = "Remove the vibrato";

// Undo-step names.
pub const EDIT_ADD_PITCH_POINT: &str = "add pitch point";
pub const EDIT_MOVE_PITCH_POINT: &str = "move pitch point";
pub const EDIT_REMOVE_PITCH_POINT: &str = "remove pitch point";
pub const EDIT_PITCH_SHAPE: &str = "change pitch curve";
pub const EDIT_PLAIN_PITCH: &str = "plain pitch";
pub const EDIT_ADD_VIBRATO: &str = "add vibrato";
pub const EDIT_REMOVE_VIBRATO: &str = "remove vibrato";
pub const EDIT_VIBRATO: &str = "change vibrato";
pub const EDIT_COLOUR: &str = "change colour";
pub const EDIT_BREATH: &str = "change breath";

/// A pitch point, for screen readers: "Pitch point 2 of E4".
pub fn pitch_point(key: u8, index: usize) -> String {
    format!("Pitch point {} of {}", index + 1, note_name(key))
}

/// A shape in a pitch point's panel, for screen readers: "Curve after it:
/// Straight" (the swing button can say "Straight" at the same time).
pub fn curve_shape(name: &str) -> String {
    format!("{CURVE_AFTER}: {name}")
}

/// A vibrato's handle, for screen readers: "Vibrato of E4".
pub fn vibrato_handle(key: u8) -> String {
    format!("Vibrato of {}", note_name(key))
}

/// A number with its unit, as the panels show it: "−40 ms", "+25 cents".
pub fn with_sign(value: f64, unit: &str) -> String {
    if value.abs() < 0.5 {
        format!("0 {unit}")
    } else if value > 0.0 {
        format!("+{value:.0} {unit}")
    } else {
        format!("−{:.0} {unit}", -value)
    }
}

/// A voicebank's picture, for screen readers (decision 0029): "Picture of
/// Uta".
pub fn voicebank_picture(voicebank: &str) -> String {
    format!("Picture of {voicebank}")
}

// ----- Shaping sounds (decision 0031)

pub const NOTE_SHAPE: &str = "Note shape";
pub const NOTE_SHAPE_HINT: &str = "How this track's notes start and end: a slow rise, a long ring, or notes cut off as their key comes up";
pub const EDIT_NOTE_SHAPE: &str = "change note shape";
pub const SHAPE_ATTACK: &str = "Attack";
pub const SHAPE_ATTACK_HINT: &str =
    "How long a note takes to rise to full level (unticked: as the instrument plays it)";
pub const SHAPE_RELEASE: &str = "Release";
pub const SHAPE_RELEASE_HINT: &str = "How long a note takes to fall silent once its key comes up (unticked: as the instrument plays it)";
pub const SHAPE_NATURAL_VALUE: &str = "as recorded";
pub const SHAPE_CUT: &str = "Cut at note end";
pub const SHAPE_CUT_HINT: &str =
    "Every note stops the moment its key comes up: no ringing tail (a \"release cut\")";
pub const SHAPE_SOFT: &str = "Soft start";
pub const SHAPE_SOFT_HINT: &str = "Notes swell in gently";
pub const SHAPE_LONG: &str = "Long tail";
pub const SHAPE_LONG_HINT: &str = "Notes ring on for a few seconds after their key comes up";
pub const SHAPE_RESET: &str = "Natural";
pub const SHAPE_RESET_HINT: &str = "Notes start and end as the instrument plays them";
pub const SHAPE_KIT_NOTE: &str =
    "With a release, each hit stops at the end of its note, however long its sound.";
pub const SHAPE_PICTURE: &str = "The shape of a note: its rise, then its end once the key comes up";
pub const SHAPE_KEY_UP: &str = "key up";

/// A time in milliseconds or seconds, as a slider shows it.
pub fn shape_time(seconds: f32) -> String {
    if seconds < 1.0 {
        format!("{:.0} ms", seconds * 1000.0)
    } else {
        format!("{seconds:.2} s")
    }
}

// The sound editor (decision 0031).
pub const EDIT_SOUND: &str = "Edit sound…";
pub const EDIT_SOUND_HINT: &str =
    "Cut, fade, reverse, re-pitch and level this clip, in a window of its own";
pub const EDIT_CLIP_REVERSE: &str = "play clip backwards";
pub const EDIT_CLIP_PITCH: &str = "change clip pitch";
pub const EDIT_NORMALIZE_CLIP: &str = "make clip as loud as it can be";
pub const SOUND_WAVEFORM: &str =
    "The clip's sound: drag its ends to cut it, the small squares to fade it";
pub const SOUND_START_HANDLE: &str = "Where the clip starts in its sound";
pub const SOUND_END_HANDLE: &str = "Where the clip ends in its sound";
pub const CLIP_REVERSE: &str = "Backwards";
pub const CLIP_REVERSE_HINT: &str = "Play the clip's sound backwards";
pub const CLIP_PITCH: &str = "Pitch";
pub const CLIP_PITCH_HINT: &str =
    "Higher or lower, in semitones: like a tape, the sound also plays faster or slower";
pub const CLIP_NORMALIZE: &str = "As loud as it can be";
pub const CLIP_NORMALIZE_HINT: &str =
    "Sets the volume so the loudest moment of the clip is just under full scale";
pub const CLIP_FADE_IN: &str = "Fade in";
pub const CLIP_FADE_OUT: &str = "Fade out";
pub const SOUND_NOT_READY: &str = "The sound is still being read…";

/// The sound editor's title.
pub fn sound_editor_title(name: &str) -> String {
    format!("Sound — {name}")
}

/// How much of its sound a clip plays.
pub fn clip_span(start: f64, end: f64, total: f64) -> String {
    format!(
        "{:.2} s to {:.2} s of {:.2} s ({:.2} s long)",
        start,
        end,
        total,
        end - start
    )
}

// ----- Selecting and moving several tracks or clips (decision 0032)

pub const EDIT_MOVE_TRACKS: &str = "move tracks";
pub const EDIT_MOVE_CLIPS: &str = "move clips";
pub const EDIT_REMOVE_CLIPS: &str = "remove clips";
pub const EDIT_DUPLICATE_CLIPS: &str = "duplicate clips";
pub const TRACK_HEADER_HINT: &str = "Click to select · Ctrl+click to add or remove · Shift+click for a range · drag up or down to move · double-click to rename";
pub const SELECTION_BOX: &str = "Selection box";

// ----- MIDI controllers (decision 0033)

/// A controller's name, as the piano roll's lane and the MIDI window say it.
pub fn control_name(control: daw_model::ControlKind) -> String {
    use daw_model::ControlKind;
    match control {
        ControlKind::PitchBend => "Bend".to_owned(),
        ControlKind::Pressure => "Pressure".to_owned(),
        ControlKind::Controller(1) => "Modulation".to_owned(),
        ControlKind::Controller(2) => "Breath".to_owned(),
        ControlKind::Controller(7) => "Volume".to_owned(),
        ControlKind::Controller(10) => "Pan".to_owned(),
        ControlKind::Controller(11) => "Expression".to_owned(),
        ControlKind::Controller(number) => format!("Controller {number}"),
    }
}

/// What a curve in the piano roll's lane is, for its tooltip.
pub fn control_hint(control: daw_model::ControlKind) -> String {
    use daw_model::ControlKind;
    let what = match control {
        ControlKind::PitchBend => "The pitch wheel: how far notes are bent, up or down.".to_owned(),
        ControlKind::Pressure => {
            "How hard the keys were pressed once down (aftertouch).".to_owned()
        }
        ControlKind::Controller(1) => "The modulation wheel: how much vibrato.".to_owned(),
        ControlKind::Controller(11) => "The expression pedal: how loud.".to_owned(),
        ControlKind::Controller(number) => {
            format!("MIDI controller {number}, as the instrument uses it.")
        }
    };
    format!(
        "{what} Recorded from a MIDI controller, or drawn: click to add a point, drag it, right-click to remove it."
    )
}

/// The accessible name of a point of a curve in the piano roll's lane.
pub fn control_point(control: daw_model::ControlKind, index: usize) -> String {
    format!("{} point {}", control_name(control), index + 1)
}

/// The accessible name of the curve area in the piano roll's lane.
pub fn control_curve(control: daw_model::ControlKind) -> String {
    format!("{} curve", control_name(control))
}

/// The accessible name of a curve's button in the piano roll's lane switch.
pub fn curve_lane(control: daw_model::ControlKind) -> String {
    format!("{} lane", control_name(control))
}

/// The tooltip of a curve's button in the piano roll's lane switch.
pub fn show_curve(control: daw_model::ControlKind) -> String {
    use daw_model::ControlKind;
    match control {
        ControlKind::PitchBend => {
            "Show how the pitch wheel bends the notes (recorded from a MIDI controller, or drawn)"
                .to_owned()
        }
        ControlKind::Controller(1) => {
            "Show how much vibrato the modulation wheel adds (recorded from a MIDI controller, or drawn)"
                .to_owned()
        }
        other => format!(
            "Show what the {} does here (right-click for the pattern's other controllers)",
            control_name(other).to_lowercase()
        ),
    }
}

/// A control of a MIDI controller, as the window names it: "controller 74,
/// channel 1", "key C1, channel 10".
pub fn midi_control(control: daw_model::MidiControl) -> String {
    let channel = control.channel + 1;
    match control.source {
        daw_model::MidiSource::Controller(number) => {
            format!("controller {number}, channel {channel}")
        }
        daw_model::MidiSource::Note(key) => format!("key {}, channel {channel}", note_name(key)),
    }
}

/// A control of a MIDI controller, at the start of a line.
pub fn midi_control_title(control: daw_model::MidiControl) -> String {
    let name = midi_control(control);
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

pub const ASSIGN_MIDI_CONTROL: &str = "Assign a MIDI control…";
pub const ASSIGN_MIDI_CONTROL_HINT: &str =
    "Then move a knob or fader on your MIDI controller: it will move this setting";
pub const ASSIGN_OTHER_MIDI_CONTROL: &str = "Assign another MIDI control…";
pub const FORGET_MIDI_CONTROL: &str = "Forget the MIDI control";
pub const WAITING_FOR_KNOB: &str =
    "Move a knob or fader on your MIDI controller to give it this setting (Escape stops waiting)";
pub const STOPPED_WAITING_FOR_KNOB: &str = "No longer waiting for a MIDI control";
pub const EDIT_ASSIGN_MIDI: &str = "assign a MIDI control";
pub const EDIT_FORGET_MIDI: &str = "forget a MIDI control";
pub const EDIT_MIDI_VOLUME: &str = "change volume from the MIDI controller";
pub const EDIT_MIDI_SETTING: &str = "change a setting from the MIDI controller";

/// Says which control now moves a setting.
pub fn midi_learned(control: daw_model::MidiControl, setting: &str) -> String {
    format!("{} now moves {setting}", midi_control_title(control))
}

/// A slider's menu line saying which control moves it.
pub fn moved_by(control: daw_model::MidiControl) -> String {
    format!("Moved by {}", midi_control(control))
}

pub const MIDI_CONTROLLERS_MENU: &str = "MIDI controllers…";
pub const MIDI_CONTROLLERS_HINT: &str =
    "The MIDI controllers connected, and what their knobs, faders and buttons do";
pub const MIDI_CONTROLLERS: &str = "MIDI controllers";
pub const MIDI_WINDOW_INTRO: &str = "Keys, pads, the pitch and modulation wheels, pressure and \
pedals play the instrument in focus, and recording keeps what they did. Transport buttons play, \
stop and record. To give a knob or fader a job, right-click a fader, a pan, or an effect's or the \
synth's setting and choose Assign a MIDI control. Plugins take the other knobs: use their own \
MIDI learn.";
pub const MIDI_DEVICES: &str = "Connected";
pub const NO_MIDI_DEVICES: &str =
    "No MIDI controller is connected. Plug one in: it is found on its own, even now.";
pub const MIDI_DEVICE_HINT: &str =
    "Switched off, the controller is left alone (another program can use it)";
pub const MIDI_KNOBS: &str = "Knobs and faders in this song";
pub const NO_MIDI_KNOBS: &str = "None yet: right-click a slider and choose Assign a MIDI control.";
pub const MIDI_BUTTONS: &str = "Buttons and pads";
pub const NO_MIDI_BUTTONS: &str = "None yet: give them commands in Keyboard shortcuts.";
pub const GIVE_BUTTONS_COMMANDS: &str = "Give buttons commands…";
pub const FORGET: &str = "Forget";
pub const MIDI_BUTTONS_HINT: &str =
    "A MIDI controller's buttons and pads can run commands too: click Learn, then press one.";
pub const PRESS_A_BUTTON: &str = "Press a button or pad…";
pub const NO_MIDI_BUTTON: &str = "No MIDI button";
pub const LEARN_BUTTON: &str = "Learn";
pub const LEARN_BUTTON_HINT: &str =
    "Then press a button or pad on your MIDI controller: it will run this command";
pub const CLEAR_BUTTON: &str = "Clear";

/// The accessible name of a command's Learn button.
pub fn learn_button_for(command: &str) -> String {
    format!("Learn a MIDI button for {command}")
}

/// The accessible name of a knob's Forget button in the MIDI window.
pub fn forget_knob(setting: &str) -> String {
    format!("Forget the MIDI control of {setting}")
}

/// Says which button now runs a command.
pub fn button_learned(control: daw_model::MidiControl, command: &str) -> String {
    format!("{} now runs {command}", midi_control_title(control))
}

/// A controller's name on the piano roll lane's narrow buttons (the full
/// name is in their tooltips and accessible names).
pub fn control_button(control: daw_model::ControlKind) -> String {
    use daw_model::ControlKind;
    match control {
        ControlKind::Controller(1) => "Mod".to_owned(),
        ControlKind::Controller(11) => "Expr.".to_owned(),
        ControlKind::Controller(number) if !matches!(number, 2 | 7 | 10) => {
            format!("CC {number}")
        }
        other => control_name(other),
    }
}
