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
        C::SwitchView => "Switch between the song and the pattern",
        C::Export => "Export",
        C::Split => "Split the clip at the play position",
        C::ImportAudio => "Import",
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
    }
}

// The synth (decision 0019).
pub const ADD_SYNTH: &str = "Synth";
pub const ADD_SYNTH_HINT: &str =
    "Add a track with the included synth: shape your own sounds, from basses to pads";
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

/// The synth window's title: "Synth · Bass line".
pub fn synth_title(track: &str) -> String {
    format!("{SYNTH_TRACK} · {track}")
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
     click it for its exact tempo, right-click it to remove it";
pub const SHOW_SIGNATURES: &str = "Show tempo and signature changes";
pub const HIDE_SIGNATURES: &str = "Hide tempo and signature changes";
pub const SAME_AS_BEFORE: &str = "Same as before";
pub const REMOVE_CHANGES: &str = "Remove these changes";
pub const GLIDE: &str = "Glide to the next tempo";
pub const GLIDE_HINT: &str = "Speed up or slow down gradually until the next tempo change";
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

/// The label of a tempo point, for screen readers: "Tempo 96 at bar 5".
pub fn tempo_point(bpm: f64, bar: u32) -> String {
    format!("Tempo {} at bar {}", self::bpm(bpm), bar + 1)
}
