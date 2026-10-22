//! Command lines, auto-replies and console lines (the contract's "Text" rule), which of them carry
//! a password (they are sealed, never sent in plain text), and what a secret row shows instead of
//! its words.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// The longest text in UTF-16 units (Minecraft's own chat limit).
pub const MAX_TEXT_UNITS: usize = 256;

/// Why a text cannot be sent (`invalid_command`'s `reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloudTextProblem {
    /// Nothing to send.
    Empty,
    /// More than 256 UTF-16 units.
    Length,
    /// Not well-formed Unicode: a lone UTF-16 surrogate (only UTF-16 input can have one).
    Unicode,
    /// Starts or ends with a space.
    Space,
    /// A control character, a line break or a tab.
    Control,
    /// Starts with `\` (the client's internal commands), also after leading spaces and invisible
    /// format characters.
    Backslash,
    /// A `%name%` other than `%username%`, `%serverip%`, `%serverport%` and `%datetime%`.
    Variable,
}

impl CloudTextProblem {
    /// The contract's `reason`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Length => "length",
            Self::Unicode => "unicode",
            Self::Space => "space",
            Self::Control => "control",
            Self::Backslash => "backslash",
            Self::Variable => "variable",
        }
    }
}

const VARIABLES: &[&str] = &["username", "serverip", "serverport", "datetime"];

/// Default_Ignorable_Code_Point (Unicode 16.0, DerivedCoreProperties.txt) as sorted ranges: the
/// same set as JavaScript's `\p{Default_Ignorable_Code_Point}`, which the service's rule uses.
const DEFAULT_IGNORABLE: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x034F, 0x034F),
    (0x061C, 0x061C),
    (0x115F, 0x1160),
    (0x17B4, 0x17B5),
    (0x180B, 0x180F),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x206F),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0F),
    (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0),
    (0xFFF0, 0xFFF8),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0000, 0xE0FFF),
];

fn is_default_ignorable(c: char) -> bool {
    let c = u32::from(c);
    DEFAULT_IGNORABLE
        .binary_search_by(|&(start, end)| {
            if end < c {
                Ordering::Less
            } else if start > c {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

/// White_Space or Default_Ignorable_Code_Point: what a client (MCC) trims from the start of a line
/// before it reads it.
pub fn is_leading_ignorable(c: char) -> bool {
    c.is_whitespace() || is_default_ignorable(c)
}

/// The text without its leading [`is_leading_ignorable`] characters (the contract's rule 4).
pub fn without_leading_ignorable(text: &str) -> &str {
    text.trim_start_matches(is_leading_ignorable)
}

/// The contract's text rule ("Text"; every implementation's shared cases are
/// `tests/vectors/text-rule-v1.json`). A `&str` is always well-formed: the `unicode` reason is for
/// UTF-16 input, [`check_text_utf16`].
pub fn check_text(text: &str) -> Result<(), CloudTextProblem> {
    if text.is_empty() {
        return Err(CloudTextProblem::Empty);
    }
    if text.encode_utf16().count() > MAX_TEXT_UNITS {
        return Err(CloudTextProblem::Length);
    }
    check_well_formed(text)
}

/// The rule for text as UTF-16 code units, the form a web page holds it in: a lone surrogate is
/// [`CloudTextProblem::Unicode`].
pub fn check_text_utf16(units: &[u16]) -> Result<(), CloudTextProblem> {
    if units.is_empty() {
        return Err(CloudTextProblem::Empty);
    }
    if units.len() > MAX_TEXT_UNITS {
        return Err(CloudTextProblem::Length);
    }
    let text = String::from_utf16(units).map_err(|_| CloudTextProblem::Unicode)?;
    check_well_formed(&text)
}

/// The rule after its length (rules 2 to 5).
fn check_well_formed(text: &str) -> Result<(), CloudTextProblem> {
    if text.starts_with(' ') || text.ends_with(' ') {
        return Err(CloudTextProblem::Space);
    }
    if text
        .chars()
        .any(|c| c.is_control() || c == '\u{2028}' || c == '\u{2029}')
    {
        return Err(CloudTextProblem::Control);
    }
    // The client trims these first: a `\` after them still starts its internal command.
    if without_leading_ignorable(text).starts_with('\\') {
        return Err(CloudTextProblem::Backslash);
    }
    if !variables_allowed(text) {
        return Err(CloudTextProblem::Variable);
    }
    Ok(())
}

/// The client's own variable scan: at a `%`, the longest run of name characters (ASCII letters
/// and digits, `_`, anything non-ASCII) followed by another `%` names a variable.
fn variables_allowed(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut end = start;
        while end < chars.len() && is_name_char(chars[end]) {
            end += 1;
        }
        if end > start && end < chars.len() && chars[end] == '%' {
            let name: String = chars[start..end].iter().collect();
            if !VARIABLES.iter().any(|v| v.eq_ignore_ascii_case(&name)) {
                return false;
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    true
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii()
}

/// The command names that carry passwords: pl-afk's list (the desktop AFK client's and the
/// contract's).
pub fn sensitive_command_names() -> &'static [&'static str] {
    pl_afk::sensitive_command_names()
}

/// JavaScript's `trim()`, as the service's check does it: White_Space and U+FEFF (NEL aside, a
/// control character the rule refuses anyway).
fn js_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// The first word (up to a space or tab), after the leading characters a client trims.
fn first_word(text: &str) -> &str {
    let text = js_trim(without_leading_ignorable(text));
    text.split([' ', '\t']).next().unwrap_or(text)
}

/// Each character's simple uppercase form (the default mapping, not the Turkish one); a character
/// whose uppercase is longer stays as it is.
fn fold_upper(word: &str) -> String {
    word.chars()
        .map(|c| {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(u), None) => u,
                _ => c,
            }
        })
        .collect()
}

/// Lowercase as a Turkish reader writes it (`I` → `ı`, `İ` → `i`).
fn turkish_lower(word: &str) -> String {
    word.chars()
        .map(|c| match c {
            'I' => 'ı',
            'İ' => 'i',
            c => {
                let mut lower = c.to_lowercase();
                match (lower.next(), lower.next()) {
                    (Some(l), None) => l,
                    _ => c,
                }
            }
        })
        .collect()
}

/// The contract's rule: the first word (after the leading characters a client trims) is a
/// password command, each character folded to its simple uppercase. Such a text must be sealed; the
/// service refuses it in plain text.
pub fn is_sensitive(text: &str) -> bool {
    let name = first_word(text);
    !name.is_empty()
        && sensitive_command_names()
            .iter()
            .any(|sensitive| fold_upper(sensitive) == fold_upper(name))
}

/// What the launcher seals by itself: the contract's rule, and also what a Turkish keyboard types in
/// capitals (`/GİRİŞ` is `/giriş` to a Turkish reader). Sealing more is always allowed.
pub fn looks_sensitive(text: &str) -> bool {
    if is_sensitive(text) {
        return true;
    }
    let name = first_word(text);
    !name.is_empty()
        && sensitive_command_names()
            .iter()
            .any(|sensitive| turkish_lower(sensitive) == turkish_lower(name))
}

/// What a secret row shows instead of its words: the command and `******` (`/login ******`). A
/// secret that is not a command (a chat line) shows only the stars: its first word may be the
/// secret itself.
pub fn preview(text: &str) -> String {
    let name = first_word(text);
    // The service checks `^(\S{1,32} )?\*{6}$` (UTF-16 units; JavaScript's `\s` has U+FEFF too).
    let plain_word = !name.contains(|c: char| c.is_whitespace() || c == '\u{feff}');
    if name.starts_with('/') && name.encode_utf16().count() <= 32 && plain_word {
        format!("{name} ******")
    } else {
        "******".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_rule() {
        assert_eq!(check_text("/is home"), Ok(()));
        assert_eq!(check_text("50% indirim"), Ok(()));
        assert_eq!(check_text("%%"), Ok(()));
        assert_eq!(check_text("Selam %username%!"), Ok(()));
        assert_eq!(check_text("%DateTime% saatinde"), Ok(()));
        assert_eq!(check_text(""), Err(CloudTextProblem::Empty));
        assert_eq!(check_text(&"a".repeat(256)), Ok(()));
        assert_eq!(check_text(&"a".repeat(257)), Err(CloudTextProblem::Length));
        // An astral character is two UTF-16 units.
        assert_eq!(check_text(&"🙂".repeat(128)), Ok(()));
        assert_eq!(check_text(&"🙂".repeat(129)), Err(CloudTextProblem::Length));
        assert_eq!(check_text(" /spawn"), Err(CloudTextProblem::Space));
        assert_eq!(check_text("/spawn "), Err(CloudTextProblem::Space));
        assert_eq!(check_text("/a\tb"), Err(CloudTextProblem::Control));
        assert_eq!(check_text("/a\nb"), Err(CloudTextProblem::Control));
        assert_eq!(check_text("a\u{2028}b"), Err(CloudTextProblem::Control));
        assert_eq!(check_text("a\u{85}b"), Err(CloudTextProblem::Control));
        assert_eq!(check_text("\\quit"), Err(CloudTextProblem::Backslash));
        assert_eq!(check_text("\u{a0}\\quit"), Err(CloudTextProblem::Backslash));
        assert_eq!(
            check_text("\u{200b}\\quit"),
            Err(CloudTextProblem::Backslash)
        );
        assert_eq!(
            check_text("\u{e0020}\\quit"),
            Err(CloudTextProblem::Backslash)
        );
        assert_eq!(check_text("a\\quit"), Ok(()));
        assert_eq!(
            check_text_utf16(&[0x61, 0xD800, 0x62]),
            Err(CloudTextProblem::Unicode)
        );
        assert_eq!(check_text_utf16(&[0x61, 0xD83D, 0xDE42]), Ok(()));
        assert_eq!(check_text("%login%"), Err(CloudTextProblem::Variable));
        assert_eq!(
            check_text("selam %players% kişi"),
            Err(CloudTextProblem::Variable)
        );
        assert_eq!(check_text("%şifre%"), Err(CloudTextProblem::Variable));
        // A `%` without a closing one, or with nothing between, is plain text.
        assert_eq!(check_text("%50 %"), Ok(()));
        assert_eq!(check_text("100%"), Ok(()));
    }

    #[test]
    fn password_commands() {
        for line in [
            "/login hunter2",
            "/LOGIN hunter2",
            "/l pw",
            "/giriş 1234",
            "/GIRIŞ 1234",
            "/register a a",
            "/şifre x",
            "/2fa 123456",
            "  /login  pw",
        ] {
            assert!(is_sensitive(line), "{line}");
            assert!(looks_sensitive(line), "{line}");
        }
        for line in ["/list", "/is home", "/logins", "selam", "", "/"] {
            assert!(!is_sensitive(line), "{line}");
            assert!(!looks_sensitive(line), "{line}");
        }
        // A Turkish keyboard's capitals: the contract's rule misses them, the launcher seals them.
        assert!(!is_sensitive("/GİRİŞ 1234"));
        assert!(looks_sensitive("/GİRİŞ 1234"));
        assert!(looks_sensitive("/KAYIT a a"));
    }

    /// The copies of the contract's vectors start with `//` lines naming their source.
    fn vectors(raw: &str) -> serde_json::Value {
        let json: String = raw
            .lines()
            .filter(|line| !line.starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        serde_json::from_str(&json).expect("vectors are JSON")
    }

    fn code_point(label: &str) -> u32 {
        u32::from_str_radix(label.strip_prefix("U+").expect("U+XXXX"), 16).unwrap()
    }

    #[test]
    fn every_shared_text_case() {
        let vectors = vectors(include_str!("../tests/vectors/text-rule-v1.json"));
        assert_eq!(
            vectors["maxTextUnits"].as_u64(),
            Some(MAX_TEXT_UNITS as u64)
        );
        let reasons: Vec<&str> = vectors["problems"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        let cases = vectors["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 60);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let units: Vec<u16> = match case["text"].as_str() {
                Some(text) => text.encode_utf16().collect(),
                None => case["utf16"]
                    .as_array()
                    .expect("a text that is not well-formed comes as code units")
                    .iter()
                    .map(|unit| u16::from_str_radix(unit.as_str().unwrap(), 16).unwrap())
                    .collect(),
            };
            assert_eq!(Some(units.len() as u64), case["units"].as_u64(), "{name}");
            let expected = case["problem"].as_str();
            let got = check_text_utf16(&units).err().map(CloudTextProblem::as_str);
            assert_eq!(got, expected, "{name}");
            if let Some(reason) = expected {
                assert!(reasons.contains(&reason), "{name}: {reason}");
            }
            let Some(text) = case["text"].as_str() else {
                continue;
            };
            assert_eq!(
                check_text(text).err().map(CloudTextProblem::as_str),
                expected,
                "{name}"
            );
            // The long texts come without their code points.
            if let Some(listed) = case["codePoints"].as_array() {
                let listed: Vec<u32> = listed
                    .iter()
                    .map(|p| code_point(p.as_str().unwrap()))
                    .collect();
                let points: Vec<u32> = text.chars().map(u32::from).collect();
                assert_eq!(points, listed, "{name}");
            }
            if let Some(sensitive) = case["sensitive"].as_bool() {
                assert_eq!(is_sensitive(text), sensitive, "{name}");
                assert_eq!(looks_sensitive(text), sensitive, "{name}");
            }
        }
        for example in vectors["ignorableExamples"].as_array().unwrap() {
            let c = char::from_u32(code_point(example.as_str().unwrap())).unwrap();
            assert!(is_leading_ignorable(c), "{example}");
        }
    }

    #[test]
    fn the_ignorable_ranges_are_sorted_and_apart() {
        for pair in DEFAULT_IGNORABLE.windows(2) {
            assert!(
                pair[0].0 <= pair[0].1 && pair[0].1 + 1 < pair[1].0,
                "{pair:?}"
            );
        }
        assert!(is_default_ignorable('\u{ad}'));
        assert!(is_default_ignorable('\u{e0fff}'));
        assert!(!is_default_ignorable('\u{e1000}'));
        assert!(!is_default_ignorable('a'));
        assert!(!is_leading_ignorable('/'));
        assert_eq!(without_leading_ignorable("\u{feff}\u{a0} /a b"), "/a b");
    }

    #[test]
    fn previews() {
        assert_eq!(preview("/login hunter2"), "/login ******");
        assert_eq!(preview("/spawn"), "/spawn ******");
        assert_eq!(preview("gizli söz"), "******");
        assert_eq!(preview(&format!("/{}", "x".repeat(40))), "******");
    }
}
