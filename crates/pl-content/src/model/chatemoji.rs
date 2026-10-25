//! Chat emojis (CHAT-EMOJI): the codes a Minecraft server turns into its own symbols, expressions
//! and pictures in chat, for the in-game overlay's emoji picker. Studio publishes them as
//! `emojis/chat.<sha256>.json`; the picker uses them only while the game is on one of the
//! document's `hosts` (elsewhere it offers its own unicode set). While the index references none,
//! launchers use [`ChatEmojis::builtin`]: the Projects server's list as its scripts had it on
//! 26 Oct 2026. See `docs/content-format.md`, "Chat emojis".
//!
//! An entry is what the player types and the picker inserts (`code`), how the server shows it
//! (`look`, with Minecraft's `&` colour codes; a `pack` entry draws the server resource pack's
//! `glyph` instead while that pack is loaded) and what the player should know before using it: the
//! rank the server asks for and the servers of the network it works on. A `shortcut` is filled in by
//! the server with the player's own value or a link (`:ping:`); it says what it writes instead.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{check_schema, default_schema, is_private_use, is_valid_item_art_host};
use crate::error::{Error, Result};

/// Highest [`ChatEmojis::schema`] this build understands.
pub const CHAT_EMOJIS_SCHEMA: u32 = 1;
/// The hosts Studio starts a document with: the Projects server and its subdomains.
pub const DEFAULT_CHAT_EMOJI_HOSTS: [&str; 1] = ["projects.gg"];
/// Most entries of `hosts`.
pub const MAX_CHAT_EMOJI_HOSTS: usize = 16;
/// Most emojis a document may hold.
pub const MAX_CHAT_EMOJIS: usize = 256;
/// Longest code or alias, in characters.
pub const MAX_CHAT_EMOJI_CODE_CHARS: usize = 32;
/// Most aliases of one emoji.
pub const MAX_CHAT_EMOJI_ALIASES: usize = 4;
/// Longest look, in characters shown (the `&` codes not counted).
pub const MAX_CHAT_EMOJI_LOOK_CHARS: usize = 48;
/// Longest rank name ("VIP+"), in characters.
pub const MAX_CHAT_EMOJI_RANK_CHARS: usize = 12;
/// Most servers an emoji may name.
pub const MAX_CHAT_EMOJI_SERVERS: usize = 6;
/// Longest server name ("Survival"), in characters.
pub const MAX_CHAT_EMOJI_SERVER_CHARS: usize = 24;
/// Longest meaning of a shortcut, in characters, per language.
pub const MAX_CHAT_EMOJI_MEANING_CHARS: usize = 60;

/// The list launchers use while Studio has published none (`crates/pl-content/data/`; the overlay
/// mod's build copies the same file).
const BUILTIN: &str = include_str!("../../data/chat-emojis.json");

/// `emojis/chat.<sha256>.json`: a server's chat emojis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatEmojis {
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// The servers the emojis belong to: the game's server matches when its host equals an entry
    /// or is a subdomain of one, ignoring the port and case (the item art's rule,
    /// [`crate::item_art_host_matches`]). Empty: the emojis apply nowhere.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// In the picker's order (within each kind).
    #[serde(default)]
    pub emojis: Vec<ChatEmoji>,
}

impl Default for ChatEmojis {
    fn default() -> Self {
        Self {
            schema: CHAT_EMOJIS_SCHEMA,
            hosts: DEFAULT_CHAT_EMOJI_HOSTS.map(str::to_owned).to_vec(),
            emojis: Vec::new(),
        }
    }
}

/// One code the server understands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatEmoji {
    /// What the player types and the picker inserts (`:kalp:`).
    pub code: String,
    /// Other codes the server takes for the same thing (`<3`); the picker finds the emoji by them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// How the server shows it in chat, with Minecraft's `&` codes for colour and style (`&c❤`).
    /// For a `pack` entry: how it shows to a player without the server's resource pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<String>,
    /// A `pack` entry's picture: one private use character of the server resource pack's default
    /// font, drawn while that pack is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    #[serde(default)]
    pub kind: ChatEmojiKind,
    /// The rank the server asks for, as players know it (`VIP`, `VIP+`); none: everyone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<String>,
    /// The servers of the network it works on (`Survival`); empty: all of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<String>,
    /// A shortcut's meaning: what the server writes for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaning: Option<ChatEmojiMeaning>,
}

/// The picker's groups.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum ChatEmojiKind {
    /// A single symbol (`❤`).
    #[default]
    Symbol,
    /// An expression or a short word (`¯\_(ツ)_/¯`, `GG`).
    Text,
    /// A picture of the server's resource pack (with a look for players without it).
    Pack,
    /// A code the server fills in with the player's own value or a link (`:ping:`).
    Shortcut,
    /// A kind this build does not know: shown with the symbols.
    #[serde(other)]
    Unknown,
}

/// A shortcut's meaning in the overlay's two languages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatEmojiMeaning {
    pub tr: String,
    pub en: String,
}

/// Why a document breaks a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEmojiProblem {
    TooManyHosts,
    BadHost(String),
    DuplicateHost(String),
    TooManyEmojis,
    BadEmoji {
        code: String,
        fault: ChatEmojiFault,
    },
    /// A code or alias that two entries (or one twice) claim, compared without case.
    DuplicateCode(String),
}

impl fmt::Display for ChatEmojiProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyHosts => write!(f, "more than {MAX_CHAT_EMOJI_HOSTS} hosts"),
            Self::BadHost(host) => write!(f, "host {host:?} is not a host name or IPv4 address"),
            Self::DuplicateHost(host) => write!(f, "host {host:?} is listed twice"),
            Self::TooManyEmojis => write!(f, "more than {MAX_CHAT_EMOJIS} emojis"),
            Self::BadEmoji { code, fault } => write!(f, "emoji {code:?}: {fault}"),
            Self::DuplicateCode(code) => write!(f, "code {code:?} is used twice"),
        }
    }
}

/// The rule one entry breaks (Studio words each one for its editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatEmojiFault {
    BadCode,
    TooManyAliases,
    BadAlias,
    NoLook,
    /// An `&` without a colour or style code after it.
    LookBadCode,
    /// A control, `§` or private use character in the look.
    LookBadCharacter,
    LookTooLong,
    NoGlyph,
    /// A pack emoji's glyph is not one private use character.
    BadGlyph,
    /// A glyph on an emoji that is not a pack emoji.
    GlyphNotPack,
    BadRank,
    TooManyServers,
    BadServer,
    ServerTwice,
    /// A shortcut without its meaning.
    NoMeaning,
    BadMeaning,
}

impl fmt::Display for ChatEmojiFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadCode => "bad code",
            Self::TooManyAliases => "too many aliases",
            Self::BadAlias => "bad alias",
            Self::NoLook => "no look",
            Self::LookBadCode => "an & without a colour or style code",
            Self::LookBadCharacter => "a control, § or private use character in the look",
            Self::LookTooLong => "the look is too long",
            Self::NoGlyph => "a pack emoji without a glyph",
            Self::BadGlyph => "the glyph is not one private use character",
            Self::GlyphNotPack => "a glyph on an emoji that is not a pack emoji",
            Self::BadRank => "bad rank",
            Self::TooManyServers => "too many servers",
            Self::BadServer => "bad server name",
            Self::ServerTwice => "a server named twice",
            Self::NoMeaning => "a shortcut without its meaning",
            Self::BadMeaning => "bad meaning",
        })
    }
}

impl ChatEmojis {
    /// The list launchers use while the index references none: the Projects server's emojis as
    /// its scripts had them on 26 Oct 2026.
    pub fn builtin() -> Self {
        serde_json::from_str(BUILTIN).unwrap_or_default()
    }

    pub fn check_schema(&self) -> Result<()> {
        check_schema("chat emojis", self.schema, CHAT_EMOJIS_SCHEMA)
    }

    /// The schema and every rule; the first problem is the error.
    pub fn validate(&self) -> Result<()> {
        self.check_schema()?;
        match self.problems().into_iter().next() {
            Some(problem) => Err(Error::Invalid(format!("chat emojis: {problem}"))),
            None => Ok(()),
        }
    }

    /// Every rule the document breaks, in document order.
    pub fn problems(&self) -> Vec<ChatEmojiProblem> {
        let mut problems = host_problems(&self.hosts);
        if self.emojis.len() > MAX_CHAT_EMOJIS {
            problems.push(ChatEmojiProblem::TooManyEmojis);
        }
        let mut codes = HashSet::new();
        for emoji in &self.emojis {
            if let Some(fault) = emoji_fault(emoji) {
                problems.push(ChatEmojiProblem::BadEmoji {
                    code: emoji.code.clone(),
                    fault,
                });
                continue;
            }
            for code in emoji.codes() {
                if !codes.insert(code.to_lowercase()) {
                    problems.push(ChatEmojiProblem::DuplicateCode(code.to_owned()));
                }
            }
        }
        problems
    }

    /// The document as a client uses it: the entries that break a rule (or claim a code an earlier
    /// one has) left out, at most [`MAX_CHAT_EMOJIS`], and the bad hosts dropped; with what was
    /// left out. Studio refuses to publish such a document; a client still shows the rest.
    pub fn without_broken(mut self) -> (Self, Vec<ChatEmojiProblem>) {
        let mut problems = Vec::new();
        let mut hosts = HashSet::new();
        self.hosts.retain(|host| {
            if !is_valid_item_art_host(host) {
                problems.push(ChatEmojiProblem::BadHost(host.clone()));
                false
            } else {
                hosts.insert(host.to_ascii_lowercase())
            }
        });
        self.hosts.truncate(MAX_CHAT_EMOJI_HOSTS);
        let mut codes = HashSet::new();
        self.emojis.retain(|emoji| {
            if let Some(fault) = emoji_fault(emoji) {
                problems.push(ChatEmojiProblem::BadEmoji {
                    code: emoji.code.clone(),
                    fault,
                });
                return false;
            }
            let mine: Vec<String> = emoji.codes().map(str::to_lowercase).collect();
            if let Some(taken) = mine.iter().find(|code| codes.contains(*code)) {
                problems.push(ChatEmojiProblem::DuplicateCode(taken.clone()));
                return false;
            }
            codes.extend(mine);
            true
        });
        if self.emojis.len() > MAX_CHAT_EMOJIS {
            self.emojis.truncate(MAX_CHAT_EMOJIS);
            problems.push(ChatEmojiProblem::TooManyEmojis);
        }
        (self, problems)
    }

    /// Whether the emojis belong to the server at `host` (`play.projects.gg`, with or without a
    /// port).
    pub fn applies_to(&self, host: &str) -> bool {
        super::item_art_host_matches(host, &self.hosts)
    }
}

impl ChatEmoji {
    /// The code, then the aliases.
    pub fn codes(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.code.as_str()).chain(self.aliases.iter().map(String::as_str))
    }
}

fn host_problems(hosts: &[String]) -> Vec<ChatEmojiProblem> {
    let mut problems = Vec::new();
    if hosts.len() > MAX_CHAT_EMOJI_HOSTS {
        problems.push(ChatEmojiProblem::TooManyHosts);
    }
    let mut seen = HashSet::new();
    for host in hosts {
        if !is_valid_item_art_host(host) {
            problems.push(ChatEmojiProblem::BadHost(host.clone()));
        } else if !seen.insert(host.to_ascii_lowercase()) {
            problems.push(ChatEmojiProblem::DuplicateHost(host.clone()));
        }
    }
    problems
}

/// The first rule an entry breaks.
pub fn emoji_fault(emoji: &ChatEmoji) -> Option<ChatEmojiFault> {
    if !is_chat_emoji_code(&emoji.code) {
        return Some(ChatEmojiFault::BadCode);
    }
    if emoji.aliases.len() > MAX_CHAT_EMOJI_ALIASES {
        return Some(ChatEmojiFault::TooManyAliases);
    }
    if emoji.aliases.iter().any(|alias| !is_chat_emoji_code(alias)) {
        return Some(ChatEmojiFault::BadAlias);
    }
    let needs_look = matches!(
        emoji.kind,
        ChatEmojiKind::Symbol | ChatEmojiKind::Text | ChatEmojiKind::Pack | ChatEmojiKind::Unknown
    );
    match emoji.look.as_deref() {
        None if needs_look => return Some(ChatEmojiFault::NoLook),
        Some(look) => {
            if let Some(fault) = look_problem(look) {
                return Some(fault);
            }
        }
        None => {}
    }
    match (emoji.kind, emoji.glyph.as_deref()) {
        (ChatEmojiKind::Pack, None) => return Some(ChatEmojiFault::NoGlyph),
        (ChatEmojiKind::Pack, Some(glyph)) if !is_one_private_use(glyph) => {
            return Some(ChatEmojiFault::BadGlyph);
        }
        (ChatEmojiKind::Pack, Some(_)) => {}
        (_, Some(_)) => return Some(ChatEmojiFault::GlyphNotPack),
        (_, None) => {}
    }
    if emoji
        .rank
        .as_deref()
        .is_some_and(|rank| !is_plain(rank, MAX_CHAT_EMOJI_RANK_CHARS))
    {
        return Some(ChatEmojiFault::BadRank);
    }
    if emoji.servers.len() > MAX_CHAT_EMOJI_SERVERS {
        return Some(ChatEmojiFault::TooManyServers);
    }
    let mut servers = HashSet::new();
    for server in &emoji.servers {
        if !is_plain(server, MAX_CHAT_EMOJI_SERVER_CHARS) {
            return Some(ChatEmojiFault::BadServer);
        }
        if !servers.insert(server.to_lowercase()) {
            return Some(ChatEmojiFault::ServerTwice);
        }
    }
    match (&emoji.meaning, emoji.kind) {
        (None, ChatEmojiKind::Shortcut) => return Some(ChatEmojiFault::NoMeaning),
        (Some(meaning), _)
            if !is_plain(&meaning.tr, MAX_CHAT_EMOJI_MEANING_CHARS)
                || !is_plain(&meaning.en, MAX_CHAT_EMOJI_MEANING_CHARS) =>
        {
            return Some(ChatEmojiFault::BadMeaning);
        }
        _ => {}
    }
    None
}

/// A code or alias: 1–[`MAX_CHAT_EMOJI_CODE_CHARS`] characters, no spaces, control characters,
/// the game's `§` or private use characters.
pub fn is_chat_emoji_code(code: &str) -> bool {
    let count = code.chars().count();
    (1..=MAX_CHAT_EMOJI_CODE_CHARS).contains(&count)
        && !code
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '§' || is_private_use(c))
}

/// Why a look breaks the rules: every `&` starts a colour (`0`–`9`, `a`–`f`) or a style (`l` bold,
/// `o` italic, `n` underlined, `m` struck through, `r` reset; either case), and the text shown is
/// 1–[`MAX_CHAT_EMOJI_LOOK_CHARS`] characters without control characters, `§` or private use
/// characters, not only spaces.
pub fn look_problem(look: &str) -> Option<ChatEmojiFault> {
    let mut shown = 0usize;
    let mut visible = false;
    let mut chars = look.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some(code) if is_look_code(code) => continue,
                _ => return Some(ChatEmojiFault::LookBadCode),
            }
        }
        if c.is_control() || c == '§' || is_private_use(c) {
            return Some(ChatEmojiFault::LookBadCharacter);
        }
        shown += 1;
        visible |= !c.is_whitespace();
    }
    if !visible {
        return Some(ChatEmojiFault::NoLook);
    }
    if shown > MAX_CHAT_EMOJI_LOOK_CHARS {
        return Some(ChatEmojiFault::LookTooLong);
    }
    None
}

/// A colour or style code after `&`.
fn is_look_code(code: char) -> bool {
    code.is_ascii_hexdigit() || matches!(code.to_ascii_lowercase(), 'l' | 'o' | 'n' | 'm' | 'r')
}

fn is_one_private_use(text: &str) -> bool {
    let mut chars = text.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if is_private_use(c))
}

/// 1–`max` characters, trimmed, without control characters, `§` or private use characters.
fn is_plain(text: &str, max: usize) -> bool {
    let count = text.chars().count();
    (1..=max).contains(&count)
        && text.trim() == text
        && !text
            .chars()
            .any(|c| c.is_control() || c == '§' || is_private_use(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(code: &str, look: &str) -> ChatEmoji {
        ChatEmoji {
            code: code.into(),
            aliases: Vec::new(),
            look: Some(look.into()),
            glyph: None,
            kind: ChatEmojiKind::Symbol,
            rank: None,
            servers: Vec::new(),
            meaning: None,
        }
    }

    fn doc(emojis: Vec<ChatEmoji>) -> ChatEmojis {
        ChatEmojis {
            emojis,
            ..ChatEmojis::default()
        }
    }

    #[test]
    fn the_builtin_list_is_the_servers_and_valid() {
        let builtin = ChatEmojis::builtin();
        builtin.validate().unwrap();
        assert_eq!(builtin.hosts, ["projects.gg"]);
        assert!(builtin.applies_to("play.projects.gg:25565"));
        assert!(!builtin.applies_to("mc.hypixel.net"));
        let kalp = builtin.emojis.iter().find(|e| e.code == ":kalp:").unwrap();
        assert_eq!(kalp.look.as_deref(), Some("&c❤"));
        assert_eq!(kalp.rank.as_deref(), Some("VIP"));
        assert!(kalp.codes().any(|code| code == "<3"));
        // The ten pictures of the server's resource pack, U+E106 to U+E10F, Survival only.
        let pack: Vec<&ChatEmoji> = builtin
            .emojis
            .iter()
            .filter(|e| e.kind == ChatEmojiKind::Pack)
            .collect();
        assert_eq!(pack.len(), 10);
        assert!(
            pack.iter()
                .all(|e| e.servers == ["Survival"] && e.look.is_some())
        );
        let mut glyphs: Vec<u32> = pack
            .iter()
            .map(|e| u32::from(e.glyph.as_deref().unwrap().chars().next().unwrap()))
            .collect();
        glyphs.sort_unstable();
        assert_eq!(glyphs, (0xE106..=0xE10F).collect::<Vec<u32>>());
        let shortcuts = builtin
            .emojis
            .iter()
            .filter(|e| e.kind == ChatEmojiKind::Shortcut)
            .count();
        assert_eq!(shortcuts, 12);
        assert!(
            builtin
                .emojis
                .iter()
                .all(|e| matches!(e.rank.as_deref(), Some("VIP" | "VIP+")))
        );
    }

    #[test]
    fn a_minimal_document_reads_with_its_defaults() {
        let read: ChatEmojis =
            serde_json::from_str(r#"{"emojis":[{"code":":x:","look":"x"}]}"#).unwrap();
        assert_eq!(read.schema, 1);
        assert!(read.hosts.is_empty());
        assert_eq!(read.emojis[0].kind, ChatEmojiKind::Symbol);
        read.validate().unwrap();
        // Hosts empty: it applies nowhere.
        assert!(!read.applies_to("play.projects.gg"));
    }

    #[test]
    fn unknown_kinds_and_fields_are_read_and_need_a_look() {
        let read: ChatEmojis = serde_json::from_str(
            r#"{"later":1,"emojis":[{"code":":x:","look":"x","kind":"sticker","size":2}]}"#,
        )
        .unwrap();
        assert_eq!(read.emojis[0].kind, ChatEmojiKind::Unknown);
        read.validate().unwrap();
        let mut bare = read.clone();
        bare.emojis[0].look = None;
        assert!(bare.validate().is_err());
    }

    #[test]
    fn codes_and_looks_follow_the_rules() {
        assert!(is_chat_emoji_code(":kalp:"));
        assert!(is_chat_emoji_code("<3"));
        assert!(is_chat_emoji_code(":başarım:"));
        for bad in ["", " :x:", ":a b:", "§c", ":\u{e106}:", &"x".repeat(33)] {
            assert!(!is_chat_emoji_code(bad), "{bad:?}");
        }
        assert_eq!(look_problem("&c❤"), None);
        assert_eq!(look_problem("&6&lWOW"), None);
        assert_eq!(look_problem("&B(╥﹏╥)&R"), None);
        assert_eq!(look_problem("¯\\_(ツ)_/¯"), None);
        assert_eq!(look_problem("&"), Some(ChatEmojiFault::LookBadCode));
        assert_eq!(look_problem("&x❤"), Some(ChatEmojiFault::LookBadCode));
        assert_eq!(
            look_problem("&k❤"),
            Some(ChatEmojiFault::LookBadCode),
            "no obfuscated text"
        );
        assert_eq!(
            look_problem("&c"),
            Some(ChatEmojiFault::NoLook),
            "colour only shows nothing"
        );
        assert_eq!(look_problem("&c "), Some(ChatEmojiFault::NoLook));
        assert_eq!(look_problem("§c❤"), Some(ChatEmojiFault::LookBadCharacter));
        assert_eq!(look_problem("a\nb"), Some(ChatEmojiFault::LookBadCharacter));
        assert_eq!(
            look_problem("\u{e106}"),
            Some(ChatEmojiFault::LookBadCharacter)
        );
        assert_eq!(
            look_problem(&"x".repeat(49)),
            Some(ChatEmojiFault::LookTooLong)
        );
        assert_eq!(look_problem(&format!("&c{}", "x".repeat(48))), None);
    }

    #[test]
    fn every_kind_has_what_it_needs() {
        let mut pack = symbol(":hmm:", "&e(・・?)");
        pack.kind = ChatEmojiKind::Pack;
        assert_eq!(emoji_fault(&pack), Some(ChatEmojiFault::NoGlyph));
        assert!(doc(vec![pack.clone()]).validate().is_err(), "no glyph");
        pack.glyph = Some("\u{e106}".into());
        doc(vec![pack.clone()]).validate().unwrap();
        pack.glyph = Some("\u{e106}\u{e107}".into());
        assert!(doc(vec![pack.clone()]).validate().is_err());
        pack.glyph = Some("a".into());
        assert!(doc(vec![pack]).validate().is_err());

        let mut glyph_on_symbol = symbol(":x:", "x");
        glyph_on_symbol.glyph = Some("\u{e106}".into());
        assert_eq!(
            emoji_fault(&glyph_on_symbol),
            Some(ChatEmojiFault::GlyphNotPack)
        );

        let mut shortcut = ChatEmoji {
            look: None,
            kind: ChatEmojiKind::Shortcut,
            ..symbol(":ping:", "")
        };
        assert_eq!(emoji_fault(&shortcut), Some(ChatEmojiFault::NoMeaning));
        shortcut.meaning = Some(ChatEmojiMeaning {
            tr: "Pingini yazar".into(),
            en: "Writes your ping".into(),
        });
        doc(vec![shortcut.clone()]).validate().unwrap();
        shortcut.meaning.as_mut().unwrap().en = " padded".into();
        assert!(doc(vec![shortcut]).validate().is_err());
    }

    #[test]
    fn ranks_servers_and_aliases_are_short_plain_words() {
        let mut emoji = symbol(":x:", "x");
        emoji.rank = Some("VIP+".into());
        emoji.servers = vec!["Survival".into(), "SMP".into()];
        emoji.aliases = vec![":y:".into()];
        doc(vec![emoji.clone()]).validate().unwrap();
        let mut long_rank = emoji.clone();
        long_rank.rank = Some("x".repeat(13));
        assert!(doc(vec![long_rank]).validate().is_err());
        let mut twice = emoji.clone();
        twice.servers = vec!["Survival".into(), "survival".into()];
        assert!(doc(vec![twice]).validate().is_err());
        let mut many = emoji.clone();
        many.aliases = (0..5).map(|i| format!(":a{i}:")).collect();
        assert!(doc(vec![many]).validate().is_err());
    }

    #[test]
    fn a_code_is_claimed_once_without_case() {
        let mut second = symbol(":KALP:", "&c❤");
        let first = symbol(":kalp:", "&c❤");
        let problems = doc(vec![first.clone(), second.clone()]).problems();
        assert_eq!(problems, [ChatEmojiProblem::DuplicateCode(":KALP:".into())]);
        second.code = ":heart:".into();
        second.aliases = vec![":Kalp:".into()];
        assert!(doc(vec![first.clone(), second]).validate().is_err());
        let mut own = first;
        own.aliases = vec![":kalp:".into()];
        assert!(doc(vec![own]).validate().is_err());
    }

    #[test]
    fn hosts_follow_the_item_arts_rules() {
        let mut list = doc(vec![symbol(":x:", "x")]);
        list.hosts = vec!["projects.gg".into(), "PROJECTS.gg".into()];
        assert_eq!(
            list.problems(),
            [ChatEmojiProblem::DuplicateHost("PROJECTS.gg".into())]
        );
        list.hosts = vec!["https://projects.gg".into()];
        assert!(list.validate().is_err());
        list.hosts = vec!["10.0.0.5".into()];
        list.validate().unwrap();
        assert!(list.applies_to("10.0.0.5:25566"));
        assert!(!list.applies_to("10.0.0.50"));
    }

    #[test]
    fn a_client_keeps_what_is_good() {
        let mut bad = symbol(":bad:", "&");
        bad.rank = Some("VIP".into());
        let list = ChatEmojis {
            hosts: vec!["projects.gg".into(), "not a host".into()],
            emojis: vec![
                symbol(":a:", "a"),
                bad,
                symbol(":A:", "again"),
                symbol(":b:", "b"),
            ],
            ..ChatEmojis::default()
        };
        let (kept, problems) = list.without_broken();
        assert_eq!(kept.hosts, ["projects.gg"]);
        let codes: Vec<&str> = kept.emojis.iter().map(|e| e.code.as_str()).collect();
        assert_eq!(codes, [":a:", ":b:"]);
        assert_eq!(problems.len(), 3);
        kept.validate().unwrap();
    }

    #[test]
    fn too_many_emojis_are_refused_and_cut_for_clients() {
        let list = doc((0..=MAX_CHAT_EMOJIS)
            .map(|i| symbol(&format!(":e{i}:"), "x"))
            .collect());
        assert!(list.problems().contains(&ChatEmojiProblem::TooManyEmojis));
        let (kept, _) = list.without_broken();
        assert_eq!(kept.emojis.len(), MAX_CHAT_EMOJIS);
    }

    #[test]
    fn writers_leave_out_what_is_empty() {
        let json = serde_json::to_string(&symbol(":x:", "x")).unwrap();
        assert_eq!(json, r#"{"code":":x:","look":"x","kind":"symbol"}"#);
    }
}
