//! Session console: a bounded, timestamped line buffer plus the legacy script/sensitive line rules.
//!
//! Classification follows the legacy console (`AfkConsoleForm.Classify`): a line that contains one
//! of the account's login commands (case-insensitive) is a *script* line, hidden unless "show
//! script commands" is on; if that command is a login/register/password command
//! (`AfkScriptCommands`) the line is *sensitive* and never shown. Unlike the legacy launcher, which
//! kept such lines verbatim in memory, every line is masked here before it is stored, emitted,
//! written to disk or logged.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::util::{eq_ignore_case, find_ignore_case};

/// Lines kept per session (legacy `MaxLogLines`).
pub const CONSOLE_CAPACITY: usize = 500;
/// Longest line kept, in characters; longer client output is cut with an ellipsis.
pub const MAX_LINE_CHARS: usize = 4096;
/// What replaces a secret.
pub const MASK: &str = "******";
/// Prefix of client stderr lines (legacy).
pub const STDERR_PREFIX: &str = "[hata] ";
/// Prefix of lines the user typed into the console.
pub const INPUT_PREFIX: &str = "> ";

/// Visibility class of a console line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ConsoleLineKind {
    /// Always visible.
    Normal,
    /// Output of a login command: visible only when "show script commands" is on.
    Script,
    /// Output of a login/register/password command: never visible (already masked anyway).
    Sensitive,
}

/// Where a console line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ConsoleOrigin {
    /// Client stdout.
    Client,
    /// Client stderr (text starts with `[hata] `).
    ClientError,
    /// Typed by the user (text starts with `> `).
    User,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleLine {
    /// When the line arrived (shown as local time; the date only if the account's chat log
    /// settings ask for it).
    pub at: jiff::Timestamp,
    /// Masked text.
    pub text: String,
    pub kind: ConsoleLineKind,
    pub origin: ConsoleOrigin,
}

impl ConsoleLine {
    /// The legacy console filter.
    pub fn is_visible(&self, show_script_commands: bool) -> bool {
        match self.kind {
            ConsoleLineKind::Normal => true,
            ConsoleLineKind::Script => show_script_commands,
            ConsoleLineKind::Sensitive => false,
        }
    }
}

/// Ring buffer of the last [`CONSOLE_CAPACITY`] lines.
#[derive(Debug, Clone, Default)]
pub(crate) struct ConsoleBuffer {
    lines: VecDeque<ConsoleLine>,
}

impl ConsoleBuffer {
    pub(crate) fn push(&mut self, line: ConsoleLine) {
        if self.lines.len() == CONSOLE_CAPACITY {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub(crate) fn snapshot(&self) -> Vec<ConsoleLine> {
        self.lines.iter().cloned().collect()
    }
}

/// Command names that carry passwords (`AfkScriptCommands.SensitiveNames`). Matching is exact on
/// the first word, case-insensitive: `/l` hides `/l pw` but not `/list`.
const SENSITIVE_COMMANDS: &[&str] = &[
    // login
    "/login",
    "/l",
    "/log",
    "/signin",
    "/g",
    "/gir",
    "/giris",
    "/giriş",
    "/girisyap",
    "/girişyap",
    // register
    "/register",
    "/reg",
    "/signup",
    "/k",
    "/kayit",
    "/kayıt",
    "/kaydol",
    // password / identity
    "/password",
    "/pass",
    "/passwd",
    "/sifre",
    "/şifre",
    "/changepassword",
    "/changepass",
    "/cp",
    "/auth",
    "/authme",
    "/2fa",
    "/2fauth",
];

/// The command names [`is_sensitive_command`] treats as carrying passwords (lowercase), for UIs
/// that mask such commands while they are typed.
pub fn sensitive_command_names() -> &'static [&'static str] {
    SENSITIVE_COMMANDS
}

/// First word of a command, split at a space or tab.
fn first_token(command: &str) -> &str {
    let trimmed = command.trim();
    trimmed.split([' ', '\t']).next().unwrap_or(trimmed)
}

/// `AfkScriptCommands.IsSensitive`: the first word is a login/register/password command.
pub fn is_sensitive_command(command: &str) -> bool {
    let name = first_token(command);
    !name.is_empty()
        && SENSITIVE_COMMANDS
            .iter()
            .any(|sensitive| eq_ignore_case(sensitive, name))
}

/// A secret to scrub from client output.
#[derive(Debug, Clone)]
struct Secret {
    /// The whole command as typed (`/login hunter2`), matched case-insensitively.
    command: String,
    /// `/login ******`.
    replacement: String,
    /// The arguments as separate words, masked wherever they appear verbatim (covers echoes that
    /// do not repeat the command exactly). Very short words are skipped to keep the console usable.
    words: Vec<String>,
}

/// Minimum length (in characters) of an argument word masked on its own.
const MIN_MASKED_WORD_CHARS: usize = 4;

impl Secret {
    fn new(command: &str) -> Option<Self> {
        let command = command.trim();
        let head = first_token(command);
        let args = command[head.len()..].trim();
        if args.is_empty() {
            return None;
        }
        Some(Self {
            command: command.to_owned(),
            replacement: format!("{head} {MASK}"),
            words: args
                .split_whitespace()
                .filter(|w| w.chars().count() >= MIN_MASKED_WORD_CHARS)
                .map(str::to_owned)
                .collect(),
        })
    }
}

/// Per-launch line rules: script classification and secret masking.
#[derive(Debug, Clone, Default)]
pub struct LineFilter {
    /// Distinct trimmed login commands, with their sensitivity.
    script_commands: Vec<(String, bool)>,
    secrets: Vec<Secret>,
}

impl LineFilter {
    /// Rules for an account's login commands (enabled or not: hiding is always on, like legacy).
    pub fn new<S: AsRef<str>>(login_commands: &[S]) -> Self {
        let mut filter = Self::default();
        for command in login_commands {
            let command = command.as_ref().trim();
            if command.is_empty() || filter.script_commands.iter().any(|(c, _)| c == command) {
                continue;
            }
            let sensitive = is_sensitive_command(command);
            filter.script_commands.push((command.to_owned(), sensitive));
            if sensitive {
                filter.add_secret(command);
            }
        }
        filter
    }

    /// Remembers a sensitive line the user typed so the client's echo of it is masked too.
    pub fn add_user_command(&mut self, line: &str) {
        if is_sensitive_command(line) {
            self.add_secret(line);
        }
    }

    fn add_secret(&mut self, command: &str) {
        if let Some(secret) = Secret::new(command)
            && !self.secrets.iter().any(|s| s.command == secret.command)
        {
            self.secrets.push(secret);
            // Longest first, so a command that contains another is replaced whole.
            self.secrets
                .sort_by_key(|s| std::cmp::Reverse(s.command.len()));
        }
    }

    /// Legacy classification of a raw (unmasked) line.
    pub fn classify(&self, raw: &str) -> ConsoleLineKind {
        let mut kind = ConsoleLineKind::Normal;
        for (command, sensitive) in &self.script_commands {
            if find_ignore_case(raw, command, 0).is_some() {
                if *sensitive {
                    return ConsoleLineKind::Sensitive;
                }
                kind = ConsoleLineKind::Script;
            }
        }
        kind
    }

    /// Replaces every known secret in `text`.
    pub fn mask(&self, text: &str) -> String {
        let mut out = text.to_owned();
        for secret in &self.secrets {
            let mut from = 0;
            while let Some((start, end)) = find_ignore_case(&out, &secret.command, from) {
                out.replace_range(start..end, &secret.replacement);
                from = start + secret.replacement.len();
            }
        }
        for secret in &self.secrets {
            for word in &secret.words {
                if out.contains(word.as_str()) {
                    out = out.replace(word.as_str(), MASK);
                }
            }
        }
        out
    }

    /// Classifies, masks and length-limits one raw line.
    pub fn process(&self, raw: &str) -> (String, ConsoleLineKind) {
        let kind = self.classify(raw);
        (cap_line(&self.mask(raw)), kind)
    }
}

fn cap_line(text: &str) -> String {
    match text.char_indices().nth(MAX_LINE_CHARS) {
        Some((cut, _)) => {
            let mut capped = text[..cut].to_owned();
            capped.push('…');
            capped
        }
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: ConsoleLineKind) -> ConsoleLine {
        ConsoleLine {
            at: jiff::Timestamp::UNIX_EPOCH,
            text: String::new(),
            kind,
            origin: ConsoleOrigin::Client,
        }
    }

    #[test]
    fn sensitive_names_match_the_first_word_exactly() {
        assert!(is_sensitive_command("/login pw"));
        assert!(is_sensitive_command("  /LOGIN\tpw"));
        assert!(is_sensitive_command("/giriş Şifre!"));
        assert!(is_sensitive_command("/l pw"));
        assert!(!is_sensitive_command("/list"));
        assert!(!is_sensitive_command("/kit login"));
        assert!(!is_sensitive_command(""));
    }

    #[test]
    fn classifies_like_the_legacy_console() {
        let filter = LineFilter::new(&["/login hunter22", " /kit ", "", "/kit"]);
        assert_eq!(
            filter.classify("[Script] send /kit"),
            ConsoleLineKind::Script
        );
        assert_eq!(
            filter.classify("[Script] send /LOGIN hunter22"),
            ConsoleLineKind::Sensitive
        );
        assert_eq!(filter.classify("Hoş geldin!"), ConsoleLineKind::Normal);
        assert!(line(ConsoleLineKind::Normal).is_visible(false));
        assert!(!line(ConsoleLineKind::Script).is_visible(false));
        assert!(line(ConsoleLineKind::Script).is_visible(true));
        assert!(!line(ConsoleLineKind::Sensitive).is_visible(true));
    }

    #[test]
    fn masks_secrets_at_the_source() {
        let filter = LineFilter::new(&["/login hunter22", "/register Parola1 Parola1", "/kit"]);
        let (text, kind) = filter.process("[Script] send /LOGIN hunter22");
        assert_eq!(text, "[Script] send /login ******");
        assert_eq!(kind, ConsoleLineKind::Sensitive);
        let (text, _) = filter.process("[Betik] send /register Parola1 Parola1");
        assert_eq!(text, "[Betik] send /register ******");
        // A partial echo still loses the password words.
        assert_eq!(filter.mask("parola: hunter22!"), "parola: ******!");
        assert_eq!(filter.mask("[Script] send /kit"), "[Script] send /kit");
    }

    #[test]
    fn user_typed_sensitive_lines_are_masked_too() {
        let mut filter = LineFilter::new::<&str>(&[]);
        filter.add_user_command("/kit");
        filter.add_user_command("/l secret123");
        assert_eq!(filter.mask("> /l secret123"), "> /l ******");
        assert_eq!(filter.classify("> /l secret123"), ConsoleLineKind::Normal);
    }

    #[test]
    fn buffer_keeps_the_last_500_lines() {
        let mut buffer = ConsoleBuffer::default();
        for i in 0..(CONSOLE_CAPACITY + 20) {
            let mut l = line(ConsoleLineKind::Normal);
            l.text = i.to_string();
            buffer.push(l);
        }
        let lines = buffer.snapshot();
        assert_eq!(lines.len(), CONSOLE_CAPACITY);
        assert_eq!(lines[0].text, "20");
    }

    #[test]
    fn long_lines_are_capped() {
        let filter = LineFilter::default();
        let (text, _) = filter.process(&"ş".repeat(MAX_LINE_CHARS + 5));
        assert_eq!(text.chars().count(), MAX_LINE_CHARS + 1);
        assert!(text.ends_with('…'));
    }
}
