// SPDX-License-Identifier: GPL-3.0-or-later

//! Lyrics as voicebanks name their recordings (decision 0027): Japanese
//! syllables typed in hiragana, katakana or romaji, the vowel a syllable
//! ends on, and the names (aliases) a classic UTAU voicebank may give the
//! recording of a syllable sung after another.

/// What a note sings when it has no lyric of its own.
pub const DEFAULT_LYRIC: &str = "a";

/// The lyric of a rest, as UTAU writes it: the note is silent.
pub const REST_LYRIC: &str = "R";

/// Romaji and the hiragana they stand for, longest spellings first where
/// one begins another. Hepburn and Kunrei spellings both work; `ti`, `tu`,
/// `di` and `du` are the sounds UTAU voicebanks give them (てぃ, とぅ, でぃ,
/// どぅ), as `chi`, `tsu`, `ji` and `zu` spell ち, つ, じ and ず.
#[rustfmt::skip]
const ROMAJI: &[(&str, &str)] = &[
    // Three letters.
    ("kya", "きゃ"), ("kyu", "きゅ"), ("kyo", "きょ"), ("kye", "きぇ"),
    ("gya", "ぎゃ"), ("gyu", "ぎゅ"), ("gyo", "ぎょ"), ("gye", "ぎぇ"),
    ("sha", "しゃ"), ("shi", "し"), ("shu", "しゅ"), ("she", "しぇ"), ("sho", "しょ"),
    ("sya", "しゃ"), ("syu", "しゅ"), ("syo", "しょ"),
    ("jya", "じゃ"), ("jyu", "じゅ"), ("jyo", "じょ"),
    ("zya", "じゃ"), ("zyu", "じゅ"), ("zyo", "じょ"),
    ("cha", "ちゃ"), ("chi", "ち"), ("chu", "ちゅ"), ("che", "ちぇ"), ("cho", "ちょ"),
    ("tya", "ちゃ"), ("tyu", "ちゅ"), ("tyo", "ちょ"),
    ("tsa", "つぁ"), ("tsi", "つぃ"), ("tsu", "つ"), ("tse", "つぇ"), ("tso", "つぉ"),
    ("dzu", "づ"), ("dji", "ぢ"),
    ("nya", "にゃ"), ("nyu", "にゅ"), ("nyo", "にょ"), ("nye", "にぇ"),
    ("hya", "ひゃ"), ("hyu", "ひゅ"), ("hyo", "ひょ"), ("hye", "ひぇ"),
    ("bya", "びゃ"), ("byu", "びゅ"), ("byo", "びょ"), ("bye", "びぇ"),
    ("pya", "ぴゃ"), ("pyu", "ぴゅ"), ("pyo", "ぴょ"), ("pye", "ぴぇ"),
    ("mya", "みゃ"), ("myu", "みゅ"), ("myo", "みょ"), ("mye", "みぇ"),
    ("rya", "りゃ"), ("ryu", "りゅ"), ("ryo", "りょ"), ("rye", "りぇ"),
    ("lya", "りゃ"), ("lyu", "りゅ"), ("lyo", "りょ"),
    // Two letters.
    ("ka", "か"), ("ki", "き"), ("ku", "く"), ("ke", "け"), ("ko", "こ"),
    ("ga", "が"), ("gi", "ぎ"), ("gu", "ぐ"), ("ge", "げ"), ("go", "ご"),
    ("sa", "さ"), ("si", "し"), ("su", "す"), ("se", "せ"), ("so", "そ"),
    ("za", "ざ"), ("zi", "じ"), ("ji", "じ"), ("zu", "ず"), ("ze", "ぜ"), ("zo", "ぞ"),
    ("ja", "じゃ"), ("ju", "じゅ"), ("je", "じぇ"), ("jo", "じょ"),
    ("ta", "た"), ("ti", "てぃ"), ("tu", "とぅ"), ("te", "て"), ("to", "と"),
    ("da", "だ"), ("di", "でぃ"), ("du", "どぅ"), ("de", "で"), ("do", "ど"),
    ("na", "な"), ("ni", "に"), ("nu", "ぬ"), ("ne", "ね"), ("no", "の"),
    ("ha", "は"), ("hi", "ひ"), ("hu", "ふ"), ("fu", "ふ"), ("he", "へ"), ("ho", "ほ"),
    ("fa", "ふぁ"), ("fi", "ふぃ"), ("fe", "ふぇ"), ("fo", "ふぉ"),
    ("ba", "ば"), ("bi", "び"), ("bu", "ぶ"), ("be", "べ"), ("bo", "ぼ"),
    ("pa", "ぱ"), ("pi", "ぴ"), ("pu", "ぷ"), ("pe", "ぺ"), ("po", "ぽ"),
    ("va", "ゔぁ"), ("vi", "ゔぃ"), ("vu", "ゔ"), ("ve", "ゔぇ"), ("vo", "ゔぉ"),
    ("ma", "ま"), ("mi", "み"), ("mu", "む"), ("me", "め"), ("mo", "も"),
    ("ya", "や"), ("yu", "ゆ"), ("ye", "いぇ"), ("yo", "よ"),
    ("ra", "ら"), ("ri", "り"), ("ru", "る"), ("re", "れ"), ("ro", "ろ"),
    ("la", "ら"), ("li", "り"), ("lu", "る"), ("le", "れ"), ("lo", "ろ"),
    ("wa", "わ"), ("wi", "うぃ"), ("we", "うぇ"), ("wo", "を"),
    // One letter.
    ("a", "あ"), ("i", "い"), ("u", "う"), ("e", "え"), ("o", "お"),
    ("n", "ん"),
];

/// Kana ending on each vowel (small kana included: they decide the vowel
/// of a syllable such as きゃ).
const ENDS_ON: &[(char, &str)] = &[
    ('a', "あかがさざただなはばぱまやらわぁゃ"),
    ('i', "いきぎしじちぢにひびぴみりぃ"),
    ('u', "うくぐすずつづぬふぶぷむゆるぅゅゔ"),
    ('e', "えけげせぜてでねへべぺめれぇ"),
    ('o', "おこごそぞとどのほぼぽもよろをぉょ"),
    ('n', "ん"),
];

/// Whether a character is hiragana or katakana (with the long-vowel mark).
fn is_kana(c: char) -> bool {
    ('\u{3041}'..='\u{3096}').contains(&c) || ('\u{30A1}'..='\u{30FA}').contains(&c) || c == 'ー'
}

/// Katakana as hiragana; everything else unchanged.
fn hiragana_of(c: char) -> char {
    if ('\u{30A1}'..='\u{30F6}').contains(&c) {
        char::from_u32(c as u32 - 0x60).unwrap_or(c)
    } else {
        c
    }
}

/// A lyric in hiragana: kana stay as they are (katakana become hiragana),
/// romaji are spelled out, and anything else is kept, so a voicebank whose
/// recordings have other names can still be matched.
pub fn to_hiragana(lyric: &str) -> String {
    let lyric = lyric.trim();
    if lyric.chars().any(is_kana) {
        return lyric.chars().map(hiragana_of).collect();
    }
    let lower = lyric.to_lowercase();
    let mut out = String::new();
    let mut rest = lower.as_str();
    while let Some(first) = rest.chars().next() {
        // A doubled consonant is a small っ before the syllable.
        let mut letters = rest.chars();
        letters.next();
        if let Some(second) = letters.next()
            && first == second
            && first.is_ascii_alphabetic()
            && !"aeiouyn".contains(first)
        {
            out.push('っ');
            rest = &rest[first.len_utf8()..];
            continue;
        }
        // "n" before a consonant or at the end is ん ("nn" too, unless a
        // vowel follows: "kanna" is かんな); before a vowel or "y" it
        // starts the next syllable.
        if first == 'n' {
            let mut after = rest[1..].chars();
            let starts_syllable = |c: char| "aeiouy".contains(c);
            match after.next() {
                None => {
                    out.push('ん');
                    rest = &rest[1..];
                    continue;
                }
                Some('n') => {
                    out.push('ん');
                    let vowel_follows = after.next().is_some_and(starts_syllable);
                    rest = &rest[if vowel_follows { 1 } else { 2 }..];
                    continue;
                }
                Some(c) if !starts_syllable(c) => {
                    out.push('ん');
                    rest = &rest[1..];
                    continue;
                }
                Some(_) => {}
            }
        }
        if let Some((romaji, kana)) = ROMAJI.iter().find(|(romaji, _)| rest.starts_with(romaji)) {
            out.push_str(kana);
            rest = &rest[romaji.len()..];
        } else {
            out.push(first);
            rest = &rest[first.len_utf8()..];
        }
    }
    out
}

/// Whether a spelling is the one Hepburn romaji (and voicebanks with
/// romaji names) use, among the table's spellings of the same kana.
fn is_hepburn(romaji: &str) -> bool {
    !romaji.starts_with('l')
        && !matches!(
            romaji,
            "si" | "zi"
                | "hu"
                | "sya"
                | "syu"
                | "syo"
                | "zya"
                | "zyu"
                | "zyo"
                | "jya"
                | "jyu"
                | "jyo"
                | "tya"
                | "tyu"
                | "tyo"
        )
}

/// A lyric in romaji (Hepburn, as voicebanks with romaji names spell them).
pub fn to_romaji(lyric: &str) -> String {
    let kana = to_hiragana(lyric);
    let mut out = String::new();
    let mut rest = kana.as_str();
    let mut double = false;
    while let Some(first) = rest.chars().next() {
        if first == 'っ' {
            double = true;
            rest = &rest[first.len_utf8()..];
            continue;
        }
        // The longest kana spelling that starts here (two kana, then one),
        // its first romaji in the table.
        let found = ROMAJI
            .iter()
            .filter(|(romaji, kana)| rest.starts_with(kana) && is_hepburn(romaji))
            .max_by_key(|(_, kana)| kana.len());
        if let Some((romaji, kana)) = found {
            if double && let Some(c) = romaji.chars().next() {
                out.push(c);
            }
            out.push_str(romaji);
            rest = &rest[kana.len()..];
        } else {
            out.push(first);
            rest = &rest[first.len_utf8()..];
        }
        double = false;
    }
    out
}

/// The vowel a lyric ends on (`n` for ん), as the next syllable of a VCV
/// voicebank needs it; none for a lyric with no vowel it knows.
pub fn ending_vowel(lyric: &str) -> Option<char> {
    let kana = to_hiragana(lyric);
    for c in kana.chars().rev() {
        if let Some((vowel, _)) = ENDS_ON.iter().find(|(_, kana)| kana.contains(c)) {
            return Some(*vowel);
        }
    }
    None
}

/// Whether a lyric is a rest: the note is silent.
pub fn is_rest(lyric: &str) -> bool {
    matches!(lyric.trim(), "R" | "r" | "-")
}

/// The names a classic UTAU voicebank may give the recording of `lyric`,
/// sung after `previous` (none at the start of a phrase), in the order to
/// try them (decision 0027): VCV first ("a か", or "- か" at the start of
/// a phrase), then CV ("か"), then the same in romaji, then the lyric as it
/// was typed.
pub fn alias_candidates(lyric: &str, previous: Option<&str>) -> Vec<String> {
    let typed = lyric.trim();
    let typed = if typed.is_empty() {
        DEFAULT_LYRIC
    } else {
        typed
    };
    let kana = to_hiragana(typed);
    let romaji = to_romaji(typed);
    let lead = previous.and_then(ending_vowel);
    let mut out: Vec<String> = Vec::new();
    for name in [&kana, &romaji] {
        if let Some(vowel) = lead {
            out.push(format!("{vowel} {name}"));
        } else {
            out.push(format!("- {name}"));
            out.push(format!("* {name}"));
        }
        out.push(name.clone());
    }
    out.push(typed.to_owned());
    let mut seen = std::collections::HashSet::new();
    out.retain(|name| seen.insert(name.clone()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn romaji_and_katakana_become_hiragana() {
        assert_eq!(to_hiragana("ka"), "か");
        assert_eq!(to_hiragana("kasane sora"), "かさね そら");
        assert_eq!(to_hiragana("Shi"), "し");
        assert_eq!(to_hiragana("kyo"), "きょ");
        assert_eq!(to_hiragana("tsu"), "つ");
        assert_eq!(to_hiragana("n"), "ん");
        assert_eq!(to_hiragana("kon"), "こん");
        assert_eq!(to_hiragana("kanna"), "かんな");
        assert_eq!(to_hiragana("kitte"), "きって");
        assert_eq!(to_hiragana("ソラ"), "そら");
        assert_eq!(to_hiragana("さ"), "さ");
        assert_eq!(to_hiragana("ti"), "てぃ", "as UTAU voicebanks name it");
        // Words that are not Japanese are kept as typed.
        assert_eq!(to_hiragana("x"), "x");
    }

    #[test]
    fn kana_become_romaji() {
        assert_eq!(to_romaji("か"), "ka");
        assert_eq!(to_romaji("し"), "shi");
        assert_eq!(to_romaji("きょ"), "kyo");
        assert_eq!(to_romaji("そら"), "sora");
        assert_eq!(to_romaji("きって"), "kitte");
        assert_eq!(to_romaji("ん"), "n");
        assert_eq!(to_romaji("ふ"), "fu");
    }

    #[test]
    fn a_syllable_ends_on_its_vowel() {
        assert_eq!(ending_vowel("か"), Some('a'));
        assert_eq!(ending_vowel("きょ"), Some('o'));
        assert_eq!(ending_vowel("sora"), Some('a'));
        assert_eq!(ending_vowel("ん"), Some('n'));
        assert_eq!(ending_vowel("かー"), Some('a'), "a long vowel");
        assert_eq!(ending_vowel("x"), None);
    }

    #[test]
    fn aliases_are_tried_vcv_then_cv() {
        assert_eq!(
            alias_candidates("ka", Some("te")),
            ["e か", "か", "e ka", "ka"]
        );
        assert_eq!(
            alias_candidates("か", None),
            ["- か", "* か", "か", "- ka", "* ka", "ka"]
        );
        // A note with no lyric sings "a".
        assert_eq!(alias_candidates("", Some("ん"))[0], "n あ");
        assert!(is_rest("R") && !is_rest("ra"));
    }
}
