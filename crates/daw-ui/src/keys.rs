// SPDX-License-Identifier: GPL-3.0-or-later

//! The keyboard: which computer keys play notes, and the table of command
//! shortcuts.
//!
//! The rule that keeps the two apart (decision 0011): **plain letter, digit
//! and punctuation keys play notes; commands use Ctrl or Alt, or keys that
//! never play notes** (Space, Delete, the arrows, Page Up/Down, Escape...).
//! Note keys follow the *physical position* of the keys, named after a US
//! QWERTY keyboard, so the two piano rows sit in the same place on AZERTY,
//! QWERTZ and every other layout. Shortcuts follow the *letter printed on
//! the key*, as people expect (Ctrl+Z is the key labelled Z).

use std::collections::{BTreeMap, HashMap};

use egui::{Key, Modifiers};

use crate::text;

// ----- Playing notes

/// The semitone a physical key plays, counted from the keyboard's lowest C.
///
/// Two rows, like two octaves of a piano: Z S X D C V G B H N J M , L . ; /
/// from the bottom row, and Q 2 W 3 E R 5 T 6 Y 7 U I 9 O 0 P [ = ] one
/// octave higher.
pub fn note_offset(physical: Key) -> Option<u8> {
    use Key::{
        B, C, CloseBracket, Comma, D, E, Equals, G, H, I, J, L, M, N, Num0, Num2, Num3, Num5, Num6,
        Num7, Num9, O, OpenBracket, P, Period, Q, R, S, Semicolon, Slash, T, U, V, W, X, Y, Z,
    };
    Some(match physical {
        Z => 0,
        S => 1,
        X => 2,
        D => 3,
        C => 4,
        V => 5,
        G => 6,
        B => 7,
        H => 8,
        N => 9,
        J => 10,
        M => 11,
        Comma | Q => 12,
        L | Num2 => 13,
        Period | W => 14,
        Semicolon | Num3 => 15,
        Slash | E => 16,
        R => 17,
        Num5 => 18,
        T => 19,
        Num6 => 20,
        Y => 21,
        Num7 => 22,
        U => 23,
        I => 24,
        Num9 => 25,
        O => 26,
        Num0 => 27,
        P => 28,
        OpenBracket => 29,
        Equals => 30,
        CloseBracket => 31,
        _ => return None,
    })
}

/// The highest offset [`note_offset`] returns.
pub const HIGHEST_OFFSET: u8 = 31;

/// The physical keys that play notes, with the offset each plays, in the
/// order they are shown on the on-screen keyboard (bottom row first).
pub const NOTE_KEYS: [(Key, u8); 37] = [
    (Key::Z, 0),
    (Key::S, 1),
    (Key::X, 2),
    (Key::D, 3),
    (Key::C, 4),
    (Key::V, 5),
    (Key::G, 6),
    (Key::B, 7),
    (Key::H, 8),
    (Key::N, 9),
    (Key::J, 10),
    (Key::M, 11),
    (Key::Comma, 12),
    (Key::L, 13),
    (Key::Period, 14),
    (Key::Semicolon, 15),
    (Key::Slash, 16),
    (Key::Q, 12),
    (Key::Num2, 13),
    (Key::W, 14),
    (Key::Num3, 15),
    (Key::E, 16),
    (Key::R, 17),
    (Key::Num5, 18),
    (Key::T, 19),
    (Key::Num6, 20),
    (Key::Y, 21),
    (Key::Num7, 22),
    (Key::U, 23),
    (Key::I, 24),
    (Key::Num9, 25),
    (Key::O, 26),
    (Key::Num0, 27),
    (Key::P, 28),
    (Key::OpenBracket, 29),
    (Key::Equals, 30),
    (Key::CloseBracket, 31),
];

/// Keyboard layouts whose key labels we know, for drawing the letters on
/// the on-screen keyboard. Playing works the same on every layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutFamily {
    /// US and UK keyboards, and most others.
    #[default]
    Qwerty,
    /// French and Belgian keyboards.
    Azerty,
    /// German, Austrian and Swiss keyboards.
    Qwertz,
}

/// What is printed on each physical key: used to label the on-screen
/// keyboard. Starts from a layout family and learns the real labels as keys
/// are pressed.
#[derive(Clone, Debug, Default)]
pub struct KeyLabels {
    family: LayoutFamily,
    learned: HashMap<Key, String>,
}

impl KeyLabels {
    /// Labels for a layout family.
    pub fn new(family: LayoutFamily) -> Self {
        Self {
            family,
            learned: HashMap::new(),
        }
    }

    /// Labels read from the system (on Windows the program can ask which
    /// character each key types).
    pub fn with_known(family: LayoutFamily, known: HashMap<Key, String>) -> Self {
        Self {
            family,
            learned: known,
        }
    }

    /// The layout family in use.
    pub fn family(&self) -> LayoutFamily {
        self.family
    }

    /// Changes the layout family; forgets what was learned.
    pub fn set_family(&mut self, family: LayoutFamily) {
        self.family = family;
        self.learned.clear();
    }

    /// Records what a key press revealed: `physical` is where the key is,
    /// `logical` what it typed.
    pub fn learn(&mut self, physical: Key, logical: Key) {
        if note_offset(physical).is_some() && is_printable(logical) {
            self.learned
                .insert(physical, logical.symbol_or_name().to_uppercase());
        }
    }

    /// The label of a physical key.
    pub fn label(&self, physical: Key) -> String {
        if let Some(label) = self.learned.get(&physical) {
            return label.clone();
        }
        let label = match (self.family, physical) {
            (LayoutFamily::Azerty, Key::Z) => "W",
            (LayoutFamily::Azerty, Key::M) => ",",
            (LayoutFamily::Azerty, Key::Comma) => ";",
            (LayoutFamily::Azerty, Key::Period) => ":",
            (LayoutFamily::Azerty, Key::Slash) => "!",
            (LayoutFamily::Azerty, Key::Semicolon) => "M",
            (LayoutFamily::Azerty, Key::Q) => "A",
            (LayoutFamily::Azerty, Key::W) | (LayoutFamily::Qwertz, Key::Y) => "Z",
            (LayoutFamily::Azerty, Key::OpenBracket) => "^",
            (LayoutFamily::Azerty, Key::CloseBracket) => "$",
            (LayoutFamily::Qwertz, Key::Z) => "Y",
            (LayoutFamily::Qwertz, Key::Semicolon) => "Ö",
            (LayoutFamily::Qwertz, Key::Slash) => "-",
            (LayoutFamily::Qwertz, Key::OpenBracket) => "Ü",
            (LayoutFamily::Qwertz, Key::CloseBracket) => "+",
            (LayoutFamily::Qwertz, Key::Equals) => "´",
            _ => physical.symbol_or_name(),
        };
        label.to_owned()
    }
}

/// Letters, digits and punctuation: keys that type a character.
fn is_printable(key: Key) -> bool {
    if key == Key::Minus {
        return true; // Its symbol is the typographic minus sign.
    }
    let mut chars = key.symbol_or_name().chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if c.is_ascii_graphic())
}

// ----- Commands

/// Everything that can be given a keyboard shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Command {
    /// Start or stop playback.
    PlayStop,
    /// Start or stop recording notes.
    Record,
    /// Go back to the start.
    ToStart,
    /// Loop on or off.
    Loop,
    /// Metronome on or off.
    Metronome,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// New song.
    New,
    /// Open a song.
    Open,
    /// Save.
    Save,
    /// Save under a new name.
    SaveAs,
    /// Select every note.
    SelectAll,
    /// Copy the selected notes.
    Copy,
    /// Cut the selected notes.
    Cut,
    /// Paste notes.
    Paste,
    /// Duplicate the selected notes right after themselves.
    Duplicate,
    /// Delete the selected notes.
    Delete,
    /// Clear the selection (or cancel a drag).
    Deselect,
    /// Move the selected notes earlier by one grid step.
    MoveEarlier,
    /// Move the selected notes later by one grid step.
    MoveLater,
    /// Move the selected notes up a semitone.
    TransposeUp,
    /// Move the selected notes down a semitone.
    TransposeDown,
    /// Move the selected notes up an octave.
    OctaveUp,
    /// Move the selected notes down an octave.
    OctaveDown,
    /// Play the computer keyboard an octave higher.
    KeyboardOctaveUp,
    /// Play the computer keyboard an octave lower.
    KeyboardOctaveDown,
    /// Zoom in on time.
    ZoomIn,
    /// Zoom out on time.
    ZoomOut,
    /// Show or hide the on-screen keyboard.
    ToggleKeyboard,
}

impl Command {
    /// Every command, in the order the shortcuts window lists them.
    pub const ALL: [Self; 29] = [
        Self::PlayStop,
        Self::Record,
        Self::ToStart,
        Self::Loop,
        Self::Metronome,
        Self::Undo,
        Self::Redo,
        Self::New,
        Self::Open,
        Self::Save,
        Self::SaveAs,
        Self::SelectAll,
        Self::Copy,
        Self::Cut,
        Self::Paste,
        Self::Duplicate,
        Self::Delete,
        Self::Deselect,
        Self::MoveEarlier,
        Self::MoveLater,
        Self::TransposeUp,
        Self::TransposeDown,
        Self::OctaveUp,
        Self::OctaveDown,
        Self::KeyboardOctaveUp,
        Self::KeyboardOctaveDown,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ToggleKeyboard,
    ];

    /// The stable name used in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            Self::PlayStop => "play_stop",
            Self::Record => "record",
            Self::ToStart => "to_start",
            Self::Loop => "loop",
            Self::Metronome => "metronome",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::New => "new",
            Self::Open => "open",
            Self::Save => "save",
            Self::SaveAs => "save_as",
            Self::SelectAll => "select_all",
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::Paste => "paste",
            Self::Duplicate => "duplicate",
            Self::Delete => "delete",
            Self::Deselect => "deselect",
            Self::MoveEarlier => "move_earlier",
            Self::MoveLater => "move_later",
            Self::TransposeUp => "transpose_up",
            Self::TransposeDown => "transpose_down",
            Self::OctaveUp => "octave_up",
            Self::OctaveDown => "octave_down",
            Self::KeyboardOctaveUp => "keyboard_octave_up",
            Self::KeyboardOctaveDown => "keyboard_octave_down",
            Self::ZoomIn => "zoom_in",
            Self::ZoomOut => "zoom_out",
            Self::ToggleKeyboard => "toggle_keyboard",
        }
    }

    /// A command by its settings-file name.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    /// Whether holding the key repeats the command.
    pub fn repeats(self) -> bool {
        matches!(
            self,
            Self::MoveEarlier
                | Self::MoveLater
                | Self::TransposeUp
                | Self::TransposeDown
                | Self::OctaveUp
                | Self::OctaveDown
                | Self::ZoomIn
                | Self::ZoomOut
                | Self::Undo
                | Self::Redo
        )
    }

    /// Copy, cut and paste always use the system's own keys (the window
    /// system turns them into clipboard events), so they cannot be changed.
    pub fn rebindable(self) -> bool {
        !matches!(self, Self::Copy | Self::Cut | Self::Paste)
    }
}

/// A key with modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    /// The key, as printed on the keyboard.
    pub key: Key,
    /// Ctrl (Command on a Mac).
    pub ctrl: bool,
    /// Shift.
    pub shift: bool,
    /// Alt.
    pub alt: bool,
}

impl Shortcut {
    /// A key on its own.
    pub const fn plain(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
            alt: false,
        }
    }

    /// Ctrl and a key.
    pub const fn ctrl(key: Key) -> Self {
        Self {
            ctrl: true,
            ..Self::plain(key)
        }
    }

    /// Ctrl, Shift and a key.
    pub const fn ctrl_shift(key: Key) -> Self {
        Self {
            ctrl: true,
            shift: true,
            ..Self::plain(key)
        }
    }

    /// Shift and a key.
    pub const fn shift(key: Key) -> Self {
        Self {
            shift: true,
            ..Self::plain(key)
        }
    }

    /// The shortcut for a key press.
    pub fn from_press(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key,
            ctrl: modifiers.command || modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
        }
    }

    /// Whether the shortcut would take a key that plays notes: letters,
    /// digits and punctuation without Ctrl or Alt are kept for playing.
    pub fn takes_a_note_key(self) -> bool {
        !self.ctrl && !self.alt && (is_printable(self.key) || note_offset(self.key).is_some())
    }

    /// As people read it: "Ctrl+Shift+Z", "Space", "Page Up".
    pub fn display(self) -> String {
        let mut shown = String::new();
        if self.ctrl {
            shown.push_str(text::CTRL);
            shown.push('+');
        }
        if self.alt {
            shown.push_str(text::ALT);
            shown.push('+');
        }
        if self.shift {
            shown.push_str(text::SHIFT);
            shown.push('+');
        }
        shown.push_str(text::key_name(self.key).unwrap_or_else(|| self.key.symbol_or_name()));
        shown
    }

    /// As stored in the settings file: "ctrl+shift+Z".
    pub fn to_setting(self) -> String {
        let mut text = String::new();
        if self.ctrl {
            text.push_str("ctrl+");
        }
        if self.alt {
            text.push_str("alt+");
        }
        if self.shift {
            text.push_str("shift+");
        }
        text.push_str(self.key.name());
        text
    }

    /// Reads a shortcut written by [`Shortcut::to_setting`].
    pub fn from_setting(text: &str) -> Option<Self> {
        let mut shortcut = Self::plain(Key::Space);
        let mut rest = text;
        loop {
            if let Some(r) = rest.strip_prefix("ctrl+") {
                shortcut.ctrl = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("alt+") {
                shortcut.alt = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("shift+") {
                shortcut.shift = true;
                rest = r;
            } else {
                break;
            }
        }
        shortcut.key = Key::from_name(rest)?;
        Some(shortcut)
    }
}

/// Why a shortcut could not be assigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindError {
    /// The key plays notes; add Ctrl or Alt.
    PlaysNotes,
    /// Another command already uses it.
    InUse(Command),
    /// This command's shortcut is fixed.
    Fixed,
}

/// Which shortcuts run which commands. A command can have several.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<(Command, Shortcut)>,
}

impl Default for Keymap {
    fn default() -> Self {
        use Shortcut as S;
        let bindings = vec![
            (Command::PlayStop, S::plain(Key::Space)),
            (Command::Record, S::ctrl(Key::R)),
            (Command::ToStart, S::plain(Key::Home)),
            (Command::Loop, S::ctrl(Key::L)),
            (Command::Metronome, S::ctrl(Key::M)),
            (Command::Undo, S::ctrl(Key::Z)),
            (Command::Redo, S::ctrl_shift(Key::Z)),
            (Command::Redo, S::ctrl(Key::Y)),
            (Command::New, S::ctrl(Key::N)),
            (Command::Open, S::ctrl(Key::O)),
            (Command::Save, S::ctrl(Key::S)),
            (Command::SaveAs, S::ctrl_shift(Key::S)),
            (Command::SelectAll, S::ctrl(Key::A)),
            (Command::Copy, S::ctrl(Key::C)),
            (Command::Cut, S::ctrl(Key::X)),
            (Command::Paste, S::ctrl(Key::V)),
            (Command::Duplicate, S::ctrl(Key::D)),
            (Command::Delete, S::plain(Key::Delete)),
            (Command::Delete, S::plain(Key::Backspace)),
            (Command::Deselect, S::plain(Key::Escape)),
            (Command::MoveEarlier, S::plain(Key::ArrowLeft)),
            (Command::MoveLater, S::plain(Key::ArrowRight)),
            (Command::TransposeUp, S::plain(Key::ArrowUp)),
            (Command::TransposeDown, S::plain(Key::ArrowDown)),
            (Command::OctaveUp, S::shift(Key::ArrowUp)),
            (Command::OctaveDown, S::shift(Key::ArrowDown)),
            (Command::KeyboardOctaveUp, S::plain(Key::PageUp)),
            (Command::KeyboardOctaveDown, S::plain(Key::PageDown)),
            (Command::ZoomIn, S::ctrl(Key::Equals)),
            (Command::ZoomIn, S::ctrl(Key::Plus)),
            (Command::ZoomOut, S::ctrl(Key::Minus)),
            (Command::ToggleKeyboard, S::ctrl(Key::K)),
        ];
        Self { bindings }
    }
}

impl Keymap {
    /// The command a key press runs, if any.
    pub fn command_for(&self, key: Key, modifiers: Modifiers) -> Option<Command> {
        let pressed = Shortcut::from_press(key, modifiers);
        self.bindings
            .iter()
            .find(|(_, shortcut)| *shortcut == pressed)
            .map(|(command, _)| *command)
    }

    /// Every shortcut of a command.
    pub fn shortcuts(&self, command: Command) -> impl Iterator<Item = Shortcut> + '_ {
        self.bindings
            .iter()
            .filter(move |(c, _)| *c == command)
            .map(|(_, s)| *s)
    }

    /// The main shortcut of a command as people read it, or "" if it has
    /// none: shown in tooltips and menus.
    pub fn text(&self, command: Command) -> String {
        self.shortcuts(command)
            .next()
            .map_or_else(String::new, Shortcut::display)
    }

    /// Gives a command a single new shortcut, replacing its others.
    pub fn bind(&mut self, command: Command, shortcut: Shortcut) -> Result<(), BindError> {
        if !command.rebindable() {
            return Err(BindError::Fixed);
        }
        if shortcut.takes_a_note_key() {
            return Err(BindError::PlaysNotes);
        }
        if let Some((other, _)) = self
            .bindings
            .iter()
            .find(|(c, s)| *s == shortcut && *c != command)
        {
            return Err(BindError::InUse(*other));
        }
        self.bindings.retain(|(c, _)| *c != command);
        self.bindings.push((command, shortcut));
        Ok(())
    }

    /// Removes every shortcut of a command.
    pub fn clear(&mut self, command: Command) {
        if command.rebindable() {
            self.bindings.retain(|(c, _)| *c != command);
        }
    }

    /// Puts one command's shortcuts back to the defaults (unless another
    /// command has taken one of them since).
    pub fn reset(&mut self, command: Command) {
        let defaults: Vec<Shortcut> = Self::default().shortcuts(command).collect();
        self.bindings.retain(|(c, _)| *c != command);
        for shortcut in defaults {
            if !self.bindings.iter().any(|(_, s)| *s == shortcut) {
                self.bindings.push((command, shortcut));
            }
        }
    }

    /// The changes from the defaults, for the settings file: for each
    /// changed command, its shortcuts (an empty list means none).
    pub fn to_settings(&self) -> BTreeMap<String, Vec<String>> {
        let defaults = Self::default();
        let mut changed = BTreeMap::new();
        for command in Command::ALL {
            let mine: Vec<Shortcut> = self.shortcuts(command).collect();
            let theirs: Vec<Shortcut> = defaults.shortcuts(command).collect();
            if mine != theirs {
                changed.insert(
                    command.id().to_owned(),
                    mine.into_iter().map(Shortcut::to_setting).collect(),
                );
            }
        }
        changed
    }

    /// The defaults with the changes from the settings file applied.
    /// Anything unreadable or not allowed is skipped.
    pub fn from_settings(changes: &BTreeMap<String, Vec<String>>) -> Self {
        let mut keymap = Self::default();
        for (id, shortcuts) in changes {
            let Some(command) = Command::from_id(id) else {
                continue;
            };
            if !command.rebindable() {
                continue;
            }
            keymap.bindings.retain(|(c, _)| *c != command);
            for shortcut in shortcuts.iter().filter_map(|s| Shortcut::from_setting(s)) {
                let free = !keymap.bindings.iter().any(|(_, s)| *s == shortcut);
                if free && !shortcut.takes_a_note_key() {
                    keymap.bindings.push((command, shortcut));
                }
            }
        }
        keymap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_rows_make_a_chromatic_keyboard() {
        let mut offsets: Vec<u8> = NOTE_KEYS.iter().map(|(_, o)| *o).collect();
        for (key, offset) in NOTE_KEYS {
            assert_eq!(note_offset(key), Some(offset), "{key:?}");
        }
        offsets.sort_unstable();
        offsets.dedup();
        assert_eq!(
            offsets,
            (0..=HIGHEST_OFFSET).collect::<Vec<_>>(),
            "every semitone from the lowest C to the highest key is playable"
        );
        assert_eq!(note_offset(Key::Space), None);
        assert_eq!(note_offset(Key::ArrowUp), None);
    }

    #[test]
    fn default_shortcuts_never_take_a_note_key_and_never_clash() {
        let keymap = Keymap::default();
        let mut seen = std::collections::HashSet::new();
        for (command, shortcut) in &keymap.bindings {
            assert!(
                !shortcut.takes_a_note_key(),
                "{command:?} uses {}, which plays a note",
                shortcut.display()
            );
            assert!(
                seen.insert(*shortcut),
                "{} is used twice",
                shortcut.display()
            );
        }
        for command in Command::ALL {
            assert!(
                keymap.shortcuts(command).next().is_some(),
                "{command:?} has no shortcut"
            );
        }
    }

    #[test]
    fn shortcuts_match_presses_exactly() {
        let keymap = Keymap::default();
        assert_eq!(
            keymap.command_for(Key::Z, Modifiers::COMMAND),
            Some(Command::Undo)
        );
        assert_eq!(
            keymap.command_for(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT),
            Some(Command::Redo)
        );
        assert_eq!(
            keymap.command_for(Key::Z, Modifiers::NONE),
            None,
            "a plain Z plays a note"
        );
        assert_eq!(
            keymap.command_for(Key::Space, Modifiers::NONE),
            Some(Command::PlayStop)
        );
        assert_eq!(keymap.text(Command::Redo), "Ctrl+Shift+Z");
        assert_eq!(keymap.text(Command::KeyboardOctaveUp), "Page Up");
    }

    #[test]
    fn rebinding_refuses_note_keys_and_clashes() {
        let mut keymap = Keymap::default();
        assert_eq!(
            keymap.bind(Command::Loop, Shortcut::plain(Key::P)),
            Err(BindError::PlaysNotes)
        );
        assert_eq!(
            keymap.bind(Command::Loop, Shortcut::ctrl(Key::Z)),
            Err(BindError::InUse(Command::Undo))
        );
        assert_eq!(
            keymap.bind(Command::Copy, Shortcut::ctrl(Key::F8)),
            Err(BindError::Fixed)
        );
        assert_eq!(keymap.bind(Command::Loop, Shortcut::plain(Key::F4)), Ok(()));
        assert_eq!(
            keymap.command_for(Key::F4, Modifiers::NONE),
            Some(Command::Loop)
        );
        assert_eq!(keymap.command_for(Key::L, Modifiers::COMMAND), None);
    }

    #[test]
    fn changes_survive_the_settings_file() {
        let mut keymap = Keymap::default();
        keymap
            .bind(Command::Loop, Shortcut::plain(Key::F4))
            .unwrap();
        keymap.clear(Command::ToggleKeyboard);
        let saved = keymap.to_settings();
        assert_eq!(saved.len(), 2, "only changes are written: {saved:?}");
        assert_eq!(Keymap::from_settings(&saved), keymap);

        let mut hostile = BTreeMap::new();
        hostile.insert(
            "loop".to_owned(),
            vec!["Q".to_owned(), "nonsense".to_owned()],
        );
        hostile.insert("no_such_command".to_owned(), vec!["F2".to_owned()]);
        let read = Keymap::from_settings(&hostile);
        assert_eq!(
            read.shortcuts(Command::Loop).count(),
            0,
            "a note key or garbage is never bound"
        );
    }

    #[test]
    fn shortcuts_round_trip_through_text() {
        for (_, shortcut) in Keymap::default().bindings {
            assert_eq!(
                Shortcut::from_setting(&shortcut.to_setting()),
                Some(shortcut)
            );
        }
    }

    #[test]
    fn labels_follow_the_layout_and_learn() {
        let mut labels = KeyLabels::new(LayoutFamily::Azerty);
        assert_eq!(labels.label(Key::Q), "A");
        assert_eq!(labels.label(Key::Z), "W");
        assert_eq!(labels.label(Key::X), "X");
        labels.learn(Key::Q, Key::B);
        assert_eq!(labels.label(Key::Q), "B", "what the key really typed wins");
        assert_eq!(KeyLabels::new(LayoutFamily::Qwertz).label(Key::Y), "Z");
        assert_eq!(KeyLabels::new(LayoutFamily::Qwerty).label(Key::Comma), ",");
    }
}
