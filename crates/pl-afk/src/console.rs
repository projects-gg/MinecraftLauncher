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
    /// Typed by the user. No longer produced: typed lines are not echoed, the server's answer is
    /// the feedback. Kept so the bindings stay as they are.
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
            words: maskable_words(args),
        })
    }

    /// A line the user marked secret, whatever its command. The whole line is masked: a command
    /// keeps its name (`/msg ******`, as the line's preview shows it), anything else becomes
    /// [`MASK`]. Its words (a command's arguments) are masked wherever they appear too.
    fn marked(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let head = first_token(line);
        let args = line[head.len()..].trim();
        let (replacement, words) = if head.starts_with('/') && !args.is_empty() {
            (format!("{head} {MASK}"), args)
        } else {
            (MASK.to_owned(), line)
        };
        Some(Self {
            command: line.to_owned(),
            replacement,
            words: maskable_words(words),
        })
    }
}

/// The words of `text` masked on their own (very short words are skipped to keep the console
/// usable).
fn maskable_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| w.chars().count() >= MIN_MASKED_WORD_CHARS)
        .map(str::to_owned)
        .collect()
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

    /// [`Self::new`] for an account whose lines the user marks secret one by one (cloud AFK
    /// accounts, where any line can carry a password): every line of `secrets` is also masked
    /// whatever its command (the whole line, and its words of at least four characters wherever
    /// they appear) and classified [`ConsoleLineKind::Sensitive`].
    pub fn with_secrets<S: AsRef<str>, T: AsRef<str>>(login_commands: &[S], secrets: &[T]) -> Self {
        let mut filter = Self::new(login_commands);
        for line in secrets {
            let line = line.as_ref().trim();
            if line.is_empty() {
                continue;
            }
            match filter.script_commands.iter_mut().find(|(c, _)| c == line) {
                Some((_, sensitive)) => *sensitive = true,
                None => filter.script_commands.push((line.to_owned(), true)),
            }
            filter.add_secret_line(line);
        }
        filter
    }

    /// Remembers a sensitive line the user typed so the client's echo of it is masked too.
    pub fn add_user_command(&mut self, line: &str) {
        if is_sensitive_command(line) {
            self.add_secret(line);
        }
    }

    /// Remembers a line the user marked secret (a sealed console line) so the client's echo of it
    /// is masked too, whatever its command.
    pub fn add_secret_line(&mut self, line: &str) {
        if let Some(secret) = Secret::marked(line) {
            self.push_secret(secret);
        }
    }

    fn add_secret(&mut self, command: &str) {
        if let Some(secret) = Secret::new(command) {
            self.push_secret(secret);
        }
    }

    fn push_secret(&mut self, secret: Secret) {
        if !self.secrets.iter().any(|s| s.command == secret.command) {
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
    fn lines_marked_secret_are_masked_whatever_their_command() {
        let filter = LineFilter::with_secrets(
            &["/is home", "/login hunter22"],
            &[
                " /msg Ahmet gizli-kod-42 ",
                "parolam123",
                "/login hunter22",
                "",
            ],
        );
        let (text, kind) = filter.process("[Script] send /msg Ahmet gizli-kod-42");
        assert_eq!(text, "[Script] send /msg ******");
        assert_eq!(kind, ConsoleLineKind::Sensitive);
        // Arguments and plain secret lines lose their words anywhere, the command name stays.
        assert_eq!(
            filter.mask("<Ahmet> gizli-kod-42 tamam mı? /msg"),
            "<******> ****** tamam mı? /msg"
        );
        assert_eq!(
            filter.mask("[Script] send parolam123"),
            "[Script] send ******"
        );
        assert_eq!(filter.mask("PAROLAM123"), "******", "whole line, any case");
        assert_eq!(filter.mask("/LOGIN hunter22"), "/login ******");
        // Plain login commands stay script lines, other text is untouched.
        assert_eq!(
            filter.process("[Script] send /is home"),
            ("[Script] send /is home".to_owned(), ConsoleLineKind::Script)
        );
        assert_eq!(filter.classify("parolam123"), ConsoleLineKind::Sensitive);
        assert_eq!(filter.mask("Hoş geldin!"), "Hoş geldin!");

        // Without secrets it is the desktop filter.
        let plain = LineFilter::with_secrets::<&str, &str>(&["/login hunter22"], &[]);
        assert_eq!(plain.mask("send /login hunter22"), "send /login ******");
        assert_eq!(plain.mask("Ahmet"), "Ahmet");
    }

    #[test]
    fn typed_lines_marked_secret_are_masked_too() {
        let mut filter = LineFilter::default();
        filter.add_secret_line("  ");
        filter.add_secret_line("/pay Ahmet 1000");
        filter.add_secret_line("/sifremiunuttum");
        assert_eq!(filter.mask("> /pay Ahmet 1000"), "> /pay ******");
        assert_eq!(filter.mask("1000 TL gönderildi"), "****** TL gönderildi");
        assert_eq!(filter.mask("ok /sifremiunuttum"), "ok ******");
        assert_eq!(
            filter.classify("> /pay Ahmet 1000"),
            ConsoleLineKind::Normal,
            "typed lines are not script lines"
        );
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
