//! Command lines, auto-replies and console lines (the contract's "Text" rule), which of them carry
//! a password (they are sealed, never sent in plain text), and what a secret row shows instead of
//! its words.

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
    /// Starts or ends with a space.
    Space,
    /// A control character, a line break or a tab.
    Control,
    /// Starts with `\` (the client's internal commands).
    Backslash,
    /// A `%name%` other than `%username%`, `%serverip%`, `%serverport%` and `%datetime%`.
    Variable,
}

const VARIABLES: &[&str] = &["username", "serverip", "serverport", "datetime"];

/// The contract's text rule, with one addition the machine applies too: Unicode spaces before a
/// `\` (the client trims them before it looks for its internal commands).
pub fn check_text(text: &str) -> Result<(), CloudTextProblem> {
    if text.is_empty() {
        return Err(CloudTextProblem::Empty);
    }
    if text.encode_utf16().count() > MAX_TEXT_UNITS {
        return Err(CloudTextProblem::Length);
    }
    if text.starts_with(' ') || text.ends_with(' ') {
        return Err(CloudTextProblem::Space);
    }
    if text
        .chars()
        .any(|c| c.is_control() || c == '\u{2028}' || c == '\u{2029}')
    {
        return Err(CloudTextProblem::Control);
    }
    if text.trim_start().starts_with('\\') {
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

fn first_word(text: &str) -> &str {
    let text = text.trim();
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

/// The contract's rule: the first word is a password command (each character folded to its simple
/// uppercase). Such a text must be sealed; the service refuses it in plain text.
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

    #[test]
    fn previews() {
        assert_eq!(preview("/login hunter2"), "/login ******");
        assert_eq!(preview("/spawn"), "/spawn ******");
        assert_eq!(preview("gizli söz"), "******");
        assert_eq!(preview(&format!("/{}", "x".repeat(40))), "******");
    }
}
