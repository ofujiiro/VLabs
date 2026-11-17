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
        C::TransposeUp => "Move notes up a semitone",
        C::TransposeDown => "Move notes down a semitone",
        C::OctaveUp => "Move notes up an octave",
        C::OctaveDown => "Move notes down an octave",
        C::KeyboardOctaveUp => "Computer keyboard an octave up",
        C::KeyboardOctaveDown => "Computer keyboard an octave down",
        C::ZoomIn => "Zoom in",
        C::ZoomOut => "Zoom out",
        C::ToggleKeyboard => "Show or hide the keyboard",
        C::ToggleMixer => "Show or hide the mixer",
        C::SwitchView => "Switch between the song and the pattern",
        C::Export => "Export as WAV",
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
pub const SONG_HINT: &str = "Click on a track to place its pattern · drag to move · drag the end to repeat · double-click to open";

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

// Export.
pub const EXPORT: &str = "Export as WAV…";
pub const EXPORT_TITLE: &str = "Export the song";
pub const EXPORT_HINT: &str =
    "The whole song is written to a WAV file, with its echoes and reverb fading out at the end.";
pub const EXPORT_CD: &str = "CD quality";
pub const EXPORT_CD_HINT: &str = "16-bit, 44.1 kHz — plays everywhere";
pub const EXPORT_STUDIO: &str = "Studio quality";
pub const EXPORT_STUDIO_HINT: &str = "24-bit, 48 kHz — for further work";
pub const EXPORT_BUTTON: &str = "Choose where and export…";
pub const EXPORTING: &str = "Exporting the song…";
pub const EXPORT_CANCELLED: &str = "Export cancelled";
pub const EXPORT_FILE_TYPE: &str = "WAV sound file";

pub fn exported(path: &str) -> String {
    format!("Exported {path}")
}

pub fn could_not_export(error: &str) -> String {
    format!("Could not export the song: {error}")
}
