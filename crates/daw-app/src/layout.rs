// SPDX-License-Identifier: GPL-3.0-or-later

//! Guessing the keyboard layout, so the on-screen keyboard can show the
//! letters printed on the person's keys. Only the labels depend on it:
//! playing works by key position on every layout. The labels also correct
//! themselves as keys are pressed.

use daw_ui::LayoutFamily;

/// The layout family of the keyboard in use, as well as the system says.
#[cfg(not(windows))]
pub fn detect() -> LayoutFamily {
    system_layout().unwrap_or_default()
}

/// The layout family of the keyboard in use, as well as the system says.
#[cfg(windows)]
pub fn detect() -> LayoutFamily {
    windows_layout()
}

/// The family of an XKB layout name or a language code ("fr", "de_CH"...).
#[cfg(any(not(windows), test))]
fn family_of(name: &str) -> Option<LayoutFamily> {
    let code = name
        .split([',', '_', '-', '.', '(', ':'])
        .next()?
        .trim()
        .to_ascii_lowercase();
    match code.as_str() {
        "fr" | "be" => Some(LayoutFamily::Azerty),
        "de" | "at" | "ch" | "cz" | "hu" | "sk" | "si" | "hr" => Some(LayoutFamily::Qwertz),
        "" => None,
        _ => Some(LayoutFamily::Qwerty),
    }
}

#[cfg(not(windows))]
fn system_layout() -> Option<LayoutFamily> {
    // The system keyboard settings, as Debian, Ubuntu, Fedora and Arch
    // write them, then the language.
    let from_files = [
        ("/etc/default/keyboard", "XKBLAYOUT="),
        ("/etc/X11/xorg.conf.d/00-keyboard.conf", "\"XkbLayout\""),
        ("/etc/vconsole.conf", "KEYMAP="),
    ]
    .into_iter()
    .find_map(|(file, key)| {
        let contents = std::fs::read_to_string(file).ok()?;
        let line = contents
            .lines()
            .find(|l| l.trim_start().starts_with(key) || l.contains(key))?;
        let value = line.split(key).nth(1)?.trim().trim_matches('"').trim();
        family_of(value.trim_start_matches('"'))
    });
    from_files.or_else(|| {
        ["LC_ALL", "LC_CTYPE", "LANG"]
            .into_iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
            .and_then(|locale| {
                // fr_CH and de_CH are Swiss keyboards (QWERTZ); fr_CA is QWERTY.
                let country = locale.split(['_', '.']).nth(1).unwrap_or("");
                match (locale.get(..2).unwrap_or(""), country) {
                    (_, "CH") => Some(LayoutFamily::Qwertz),
                    ("fr", "CA") => Some(LayoutFamily::Qwerty),
                    (language, _) => family_of(language),
                }
            })
    })
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "asks Windows for the active keyboard layout")]
fn windows_layout() -> LayoutFamily {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
    // SAFETY: GetKeyboardLayout takes a thread id (0: this thread) and
    // returns a handle by value; it reads no memory of ours.
    let layout = unsafe { GetKeyboardLayout(0) } as usize;
    // The low word is the language: primary language in the low 10 bits,
    // the region above them.
    let language = layout & 0xFFFF;
    let primary = language & 0x3FF;
    let region = language >> 10;
    match (primary, region) {
        // French (Switzerland); German, Czech, Hungarian, Slovak, Slovenian
        // and Croatian.
        (0x0C, 4) | (0x07 | 0x05 | 0x0E | 0x1B | 0x24 | 0x1A, _) => LayoutFamily::Qwertz,
        // French (but not Canada's, which is QWERTY), Dutch (Belgium).
        (0x0C, region) | (0x13, region @ 2) if region != 3 => LayoutFamily::Azerty,
        _ => LayoutFamily::Qwerty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_names_map_to_families() {
        assert_eq!(family_of("fr"), Some(LayoutFamily::Azerty));
        assert_eq!(family_of("fr(oss)"), Some(LayoutFamily::Azerty));
        assert_eq!(family_of("be,us"), Some(LayoutFamily::Azerty));
        assert_eq!(family_of("de"), Some(LayoutFamily::Qwertz));
        assert_eq!(family_of("us"), Some(LayoutFamily::Qwerty));
        assert_eq!(family_of("gb"), Some(LayoutFamily::Qwerty));
        assert_eq!(family_of(""), None);
    }
}
