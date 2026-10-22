//! An account's commands and auto-replies as the launcher's editor holds them, checked against the
//! contract's rules, and turned into the change the service takes: secret rows sealed here (their
//! words kept in the keychain), stored seals kept, or sealed again for a new server.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::copies::{RowCopies, RowCopy};
use crate::model::{
    CloudAntiAfk, CloudConfigView, CloudEvery, CloudLimits, CloudPayloadView, CloudServer,
    CommandInput, ConfigInput, PayloadInput, RuleInput, VaultKey,
};
use crate::seal::{SealError, SealTarget, fingerprint, seal};
use crate::text::{CloudTextProblem, check_text, looks_sensitive, preview};

/// Hard maxima of the schema, whatever Studio's numbers say.
const HARD_MAX_ROWS: usize = 50;
const MAX_DAILY_TIMES: usize = 6;
const MAX_DELAY_SECS: u32 = 60;
const MAX_SECS: u32 = 86_400;
const ANTI_AFK_MIN: u32 = 10;
const ANTI_AFK_MAX: u32 = 3_600;

/// A row's words in the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CloudRowText {
    /// Plain words, stored as they are.
    Plain { text: String },
    /// A secret whose words the launcher has: this computer's copy (`kept`), or typed now. Sealed
    /// before it is sent; the copy is kept after.
    Secret {
        text: String,
        #[serde(default)]
        kept: bool,
    },
    /// A secret only the account's machine can show ("Göster"): its preview. Kept as it is stored.
    Sealed { preview: String },
}

/// A command row: `on` is `join`, `start` or `reconnect` (with `delay`), `interval` (with
/// `every`) or `daily` (with `at`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudEditCommand {
    pub id: String,
    pub enabled: bool,
    pub on: String,
    #[serde(default)]
    pub delay: Option<u32>,
    #[serde(default)]
    pub every: Option<CloudEvery>,
    #[serde(default)]
    pub at: Option<Vec<String>>,
    pub words: CloudRowText,
}

/// An auto-reply rule: `from` is `any`, `player`, `private` or `server`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudEditRule {
    pub id: String,
    pub enabled: bool,
    pub pattern: String,
    pub regex: bool,
    pub from: String,
    pub owner_only: bool,
    pub cooldown: u32,
    pub reply: CloudRowText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudEditConfig {
    /// "" or the player's own in-game name ("Yalnız ana hesabımdan" rules listen to it).
    pub owner: String,
    pub anti_afk: CloudAntiAfk,
    pub commands: Vec<CloudEditCommand>,
    pub rules: Vec<CloudEditRule>,
}

/// Where a problem is and what it is. `place` is `general`, `commands` or `rules`; `index` the
/// row; `field` what of it (`owner`, `antiAfk`, `count`, `id`, `on`, `delay`, `every`, `at`,
/// `text`, `match`, `from`, `ownerOnly`, `cooldown`, `reply`); `problem` a key the UI words
/// (a text problem, a pattern problem, `range`, `order`, `format`, `duplicate`, `limit`, `reseal`,
/// `owner`, `server`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudIssue {
    pub place: String,
    pub index: Option<u32>,
    pub field: String,
    pub problem: String,
}

impl CloudIssue {
    fn general(field: &str, problem: &str) -> Self {
        Self {
            place: "general".into(),
            index: None,
            field: field.into(),
            problem: problem.into(),
        }
    }

    fn row(place: &str, index: usize, field: &str, problem: &str) -> Self {
        Self {
            place: place.into(),
            index: Some(index as u32),
            field: field.into(),
            problem: problem.into(),
        }
    }

    /// The issue of a refusal's `field` path (`commands[2].every.min`, `rules[0].reply`).
    pub fn from_path(path: &str, problem: &str) -> Self {
        let (head, rest) = path.split_once('.').unwrap_or((path, ""));
        if let Some((place, index)) = head.split_once('[')
            && let Some(index) = index.strip_suffix(']').and_then(|i| i.parse::<u32>().ok())
            && (place == "commands" || place == "rules")
        {
            let field = rest.split(['.', '[']).next().unwrap_or("");
            let field = match field {
                "" | "secret" | "sealed" | "preview" | "keep" if place == "commands" => "text",
                "" => "reply",
                "text" if place == "commands" => "text",
                other => other,
            };
            return Self {
                place: place.into(),
                index: Some(index),
                field: field.into(),
                problem: problem.into(),
            };
        }
        match head {
            "commands" | "rules" => Self {
                place: head.into(),
                index: None,
                field: "count".into(),
                problem: problem.into(),
            },
            _ => Self::general(head.split(['.', '[']).next().unwrap_or(head), problem),
        }
    }
}

fn text_problem(problem: CloudTextProblem) -> &'static str {
    match problem {
        CloudTextProblem::Empty => "empty",
        CloudTextProblem::Length => "length",
        CloudTextProblem::Space => "space",
        CloudTextProblem::Control => "control",
        CloudTextProblem::Backslash => "backslash",
        CloudTextProblem::Variable => "variable",
    }
}

/// The editor's form of the account view: a secret row shows its words when this computer kept a
/// copy of exactly that seal (the same `fp`), else only its preview.
pub fn edit_config(view: &CloudConfigView, copies: &RowCopies) -> CloudEditConfig {
    let words = |id: &str, payload: &CloudPayloadView| -> CloudRowText {
        if !payload.secret {
            return CloudRowText::Plain {
                text: payload.text.clone().unwrap_or_default(),
            };
        }
        match (copies.get(id), payload.fp.as_deref()) {
            (Some(copy), Some(fp)) if copy.fp == fp => CloudRowText::Secret {
                text: copy.text.clone(),
                kept: true,
            },
            _ => CloudRowText::Sealed {
                preview: payload.preview.clone().unwrap_or_else(|| "******".into()),
            },
        }
    };
    CloudEditConfig {
        owner: view.owner.clone(),
        anti_afk: view.anti_afk.clone(),
        commands: view
            .commands
            .iter()
            .map(|row| CloudEditCommand {
                id: row.id.clone(),
                enabled: row.enabled,
                on: row.on.clone(),
                delay: row.delay,
                every: row.every,
                at: row.at.clone(),
                words: words(&row.id, &row.payload()),
            })
            .collect(),
        rules: view
            .rules
            .iter()
            .map(|rule| CloudEditRule {
                id: rule.id.clone(),
                enabled: rule.enabled,
                pattern: rule.pattern.clone(),
                regex: rule.regex,
                from: rule.from.clone(),
                owner_only: rule.owner_only,
                cooldown: rule.cooldown,
                reply: words(&rule.id, &rule.reply),
            })
            .collect(),
    }
}

fn is_row_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some('a'..='z'))
        && id.len() <= 16
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id != crate::seal::CONSOLE_ROW
}

fn is_clock(time: &str) -> bool {
    let bytes = time.as_bytes();
    bytes.len() == 5
        && bytes[2] == b':'
        && time[..2].parse::<u8>().is_ok_and(|h| h < 24)
        && time[3..].parse::<u8>().is_ok_and(|m| m < 60)
        && bytes[..2].iter().chain(&bytes[3..]).all(u8::is_ascii_digit)
}

fn check_words(
    words: &CloudRowText,
    place: &str,
    index: usize,
    field: &str,
    issues: &mut Vec<CloudIssue>,
) {
    let text = match words {
        CloudRowText::Plain { text } | CloudRowText::Secret { text, .. } => text,
        CloudRowText::Sealed { .. } => return,
    };
    if let Err(problem) = check_text(text) {
        issues.push(CloudIssue::row(place, index, field, text_problem(problem)));
    }
}

/// Everything the service would refuse, found before anything is sent (Studio's numbers from
/// `/offer`).
pub fn validate(config: &CloudEditConfig, limits: &CloudLimits) -> Vec<CloudIssue> {
    let mut issues = Vec::new();
    if !config.owner.is_empty() && !pl_afk::is_valid_nickname(&config.owner) {
        issues.push(CloudIssue::general("owner", "format"));
    }
    let anti = &config.anti_afk;
    if !(ANTI_AFK_MIN..=ANTI_AFK_MAX).contains(&anti.delay_min)
        || !(ANTI_AFK_MIN..=ANTI_AFK_MAX).contains(&anti.delay_max)
        || anti.delay_min > anti.delay_max
    {
        issues.push(CloudIssue::general("antiAfk", "range"));
    }
    let max_commands = (limits.max_command_rows as usize).min(HARD_MAX_ROWS);
    if config.commands.len() > max_commands {
        issues.push(CloudIssue {
            place: "commands".into(),
            index: None,
            field: "count".into(),
            problem: "limit".into(),
        });
    }
    let max_rules = (limits.max_rules as usize).min(HARD_MAX_ROWS);
    if config.rules.len() > max_rules {
        issues.push(CloudIssue {
            place: "rules".into(),
            index: None,
            field: "count".into(),
            problem: "limit".into(),
        });
    }
    let mut ids = HashSet::new();
    let mut check_id = |id: &str, place: &str, index: usize, issues: &mut Vec<CloudIssue>| {
        if !is_row_id(id) {
            issues.push(CloudIssue::row(place, index, "id", "format"));
        } else if !ids.insert(id.to_owned()) {
            issues.push(CloudIssue::row(place, index, "id", "duplicate"));
        }
    };
    for (index, row) in config.commands.iter().enumerate() {
        check_id(&row.id, "commands", index, &mut issues);
        match row.on.as_str() {
            "join" | "start" | "reconnect" => {
                if row.delay.is_none_or(|delay| delay > MAX_DELAY_SECS) {
                    issues.push(CloudIssue::row("commands", index, "delay", "range"));
                }
            }
            "interval" => match row.every {
                Some(CloudEvery { min, max }) if min > max => {
                    issues.push(CloudIssue::row("commands", index, "every", "order"));
                }
                Some(CloudEvery { min, max })
                    if min >= limits.min_interval_sec && max <= MAX_SECS => {}
                _ => issues.push(CloudIssue::row("commands", index, "every", "range")),
            },
            "daily" => {
                let times = row.at.as_deref().unwrap_or_default();
                let distinct: HashSet<&String> = times.iter().collect();
                if times.is_empty() {
                    issues.push(CloudIssue::row("commands", index, "at", "empty"));
                } else if times.len() > MAX_DAILY_TIMES {
                    issues.push(CloudIssue::row("commands", index, "at", "many"));
                } else if !times.iter().all(|t| is_clock(t)) {
                    issues.push(CloudIssue::row("commands", index, "at", "format"));
                } else if distinct.len() != times.len() {
                    issues.push(CloudIssue::row("commands", index, "at", "duplicate"));
                }
            }
            _ => issues.push(CloudIssue::row("commands", index, "on", "format")),
        }
        check_words(&row.words, "commands", index, "text", &mut issues);
    }
    for (index, rule) in config.rules.iter().enumerate() {
        check_id(&rule.id, "rules", index, &mut issues);
        let pattern = rule.pattern.trim();
        if pattern.is_empty() {
            issues.push(CloudIssue::row("rules", index, "match", "empty"));
        } else if let Some(problem) = pl_afk::check_pattern(pattern, rule.regex) {
            let problem = match problem {
                pl_afk::PatternProblem::TooLong => "tooLong",
                pl_afk::PatternProblem::Invalid => "invalid",
                pl_afk::PatternProblem::TooComplex => "tooComplex",
            };
            issues.push(CloudIssue::row("rules", index, "match", problem));
        }
        if !matches!(rule.from.as_str(), "any" | "player" | "private" | "server") {
            issues.push(CloudIssue::row("rules", index, "from", "format"));
        }
        if rule.owner_only {
            if config.owner.is_empty() {
                issues.push(CloudIssue::row("rules", index, "ownerOnly", "owner"));
            } else if rule.from == "server" {
                issues.push(CloudIssue::row("rules", index, "ownerOnly", "server"));
            }
        }
        if rule.cooldown < limits.min_rule_cooldown_sec || rule.cooldown > MAX_SECS {
            issues.push(CloudIssue::row("rules", index, "cooldown", "range"));
        }
        check_words(&rule.reply, "rules", index, "reply", &mut issues);
    }
    issues
}

/// What sealing needs: the vault key, the owner, the account and its server as it will be stored.
#[derive(Debug, Clone, Copy)]
pub struct SealContext<'a> {
    pub key: &'a VaultKey,
    pub discord_id: &'a str,
    pub account_id: &'a str,
    pub server: &'a CloudServer,
    /// The change moves the account to `server`: every stored seal must be made again.
    pub server_changed: bool,
}

/// The change to send and the copies to keep once it is applied.
#[derive(Debug)]
pub struct Built {
    pub config: ConfigInput,
    /// Rows whose seal is new: their copies, recorded with the new fingerprints.
    pub sealed: RowCopies,
    /// Rows kept as stored (their copies, if any, stay as they are).
    pub kept: Vec<String>,
}

/// Why a change could not be built.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("the change has problems")]
    Invalid(Vec<CloudIssue>),
    #[error("could not seal a row: {0}")]
    Seal(SealError),
}

/// The change for `config`: plain rows as they are (a password command is sealed anyway), secret
/// rows with words sealed for `ctx` (or kept, when they are this computer's copy of the stored
/// seal and the server stays), rows only the cloud can show kept; with a new server those must be
/// typed again (`reseal`).
pub fn build(
    config: &CloudEditConfig,
    limits: &CloudLimits,
    ctx: &SealContext<'_>,
    copies: &RowCopies,
    fps: &std::collections::HashMap<String, String>,
) -> Result<Built, BuildError> {
    let mut issues = validate(config, limits);
    if ctx.server_changed {
        for (index, row) in config.commands.iter().enumerate() {
            if matches!(row.words, CloudRowText::Sealed { .. }) {
                issues.push(CloudIssue::row("commands", index, "text", "reseal"));
            }
        }
        for (index, rule) in config.rules.iter().enumerate() {
            if matches!(rule.reply, CloudRowText::Sealed { .. }) {
                issues.push(CloudIssue::row("rules", index, "reply", "reseal"));
            }
        }
    }
    if !issues.is_empty() {
        return Err(BuildError::Invalid(issues));
    }
    let mut sealed = RowCopies::new();
    let mut kept = Vec::new();
    let mut payload = |id: &str, words: &CloudRowText| -> Result<PayloadInput, BuildError> {
        let secret_words = match words {
            CloudRowText::Plain { text } if !looks_sensitive(text) => {
                return Ok(PayloadInput::plain(text.clone()));
            }
            CloudRowText::Plain { text } | CloudRowText::Secret { text, .. } => text,
            CloudRowText::Sealed { .. } => {
                kept.push(id.to_owned());
                return Ok(PayloadInput::keep());
            }
        };
        // This computer's copy of the stored seal, unchanged, on the same server: keep it.
        let stored_fp = fps.get(id);
        if !ctx.server_changed
            && let (Some(copy), Some(fp)) = (copies.get(id), stored_fp)
            && copy.fp == *fp
            && copy.text == *secret_words
        {
            kept.push(id.to_owned());
            return Ok(PayloadInput::keep());
        }
        let target = SealTarget {
            discord_id: ctx.discord_id,
            account_id: ctx.account_id,
            row_id: id,
            server: ctx.server,
        };
        let value = seal(ctx.key, secret_words, &target).map_err(BuildError::Seal)?;
        sealed.insert(
            id.to_owned(),
            RowCopy {
                text: secret_words.clone(),
                fp: fingerprint(&value),
            },
        );
        Ok(PayloadInput::sealed(value, preview(secret_words)))
    };
    let mut commands = Vec::with_capacity(config.commands.len());
    for row in &config.commands {
        let (delay, every, at) = match row.on.as_str() {
            "interval" => (None, row.every, None),
            "daily" => {
                let mut times = row.at.clone().unwrap_or_default();
                times.sort();
                (None, None, Some(times))
            }
            _ => (Some(row.delay.unwrap_or(0)), None, None),
        };
        commands.push(CommandInput {
            id: row.id.clone(),
            enabled: row.enabled,
            on: row.on.clone(),
            delay,
            every,
            at,
            payload: payload(&row.id, &row.words)?,
        });
    }
    let mut rules = Vec::with_capacity(config.rules.len());
    for rule in &config.rules {
        rules.push(RuleInput {
            id: rule.id.clone(),
            enabled: rule.enabled,
            pattern: rule.pattern.trim().to_owned(),
            regex: rule.regex,
            from: rule.from.clone(),
            owner_only: rule.owner_only,
            cooldown: rule.cooldown,
            reply: payload(&rule.id, &rule.reply)?,
        });
    }
    Ok(Built {
        config: ConfigInput {
            v: 1,
            owner: config.owner.clone(),
            anti_afk: config.anti_afk.clone(),
            commands,
            rules,
        },
        sealed,
        kept,
    })
}

/// The stored seals' fingerprints of a view, by row id (secret rows only).
pub fn fingerprints(view: &CloudConfigView) -> std::collections::HashMap<String, String> {
    let commands = view
        .commands
        .iter()
        .filter(|row| row.secret)
        .filter_map(|row| Some((row.id.clone(), row.fp.clone()?)));
    let rules = view
        .rules
        .iter()
        .filter(|rule| rule.reply.secret)
        .filter_map(|rule| Some((rule.id.clone(), rule.reply.fp.clone()?)));
    commands.chain(rules).collect()
}

/// The copies to keep after a change was applied: the new seals' words, and the kept rows' old
/// copies when the account still has that seal (`view`, the answer of the change).
pub fn copies_after(built: Built, before: &RowCopies, view: &CloudConfigView) -> RowCopies {
    let fps = fingerprints(view);
    let mut after = RowCopies::new();
    for id in &built.kept {
        if let (Some(copy), Some(fp)) = (before.get(id), fps.get(id))
            && copy.fp == *fp
        {
            after.insert(id.clone(), copy.clone());
        }
    }
    for (id, copy) in built.sealed {
        // Recorded only when the service stored exactly this seal.
        if fps.get(&id).is_some_and(|fp| *fp == copy.fp) {
            after.insert(id, copy);
        }
    }
    after
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::model::{CloudCommandView, CloudRuleView};

    fn limits() -> CloudLimits {
        CloudLimits::default()
    }

    fn command(id: &str, on: &str, words: CloudRowText) -> CloudEditCommand {
        CloudEditCommand {
            id: id.into(),
            enabled: true,
            on: on.into(),
            delay: Some(2),
            every: Some(CloudEvery { min: 300, max: 600 }),
            at: Some(vec!["20:30".into(), "08:00".into()]),
            words,
        }
    }

    fn config(commands: Vec<CloudEditCommand>, rules: Vec<CloudEditRule>) -> CloudEditConfig {
        CloudEditConfig {
            owner: "Ahmet".into(),
            anti_afk: CloudAntiAfk {
                enabled: true,
                delay_min: 60,
                delay_max: 90,
            },
            commands,
            rules,
        }
    }

    fn rule(id: &str, reply: CloudRowText) -> CloudEditRule {
        CloudEditRule {
            id: id.into(),
            enabled: true,
            pattern: " selam ".into(),
            regex: false,
            from: "player".into(),
            owner_only: false,
            cooldown: 30,
            reply,
        }
    }

    fn test_key() -> VaultKey {
        VaultKey {
            kid: "k1".into(),
            alg: crate::seal::SEAL_ALG.into(),
            public_key:
                "BGZR9887CRjlQ5y2FJVdcTk4SXvaJgki0UHBIcGQzgyFeGlXz23CCIqwY7y-ILfjQNauyJJzhJnh8Og_O-rig7Y"
                    .into(),
        }
    }

    fn plain(text: &str) -> CloudRowText {
        CloudRowText::Plain { text: text.into() }
    }

    fn secret(text: &str) -> CloudRowText {
        CloudRowText::Secret {
            text: text.into(),
            kept: false,
        }
    }

    #[test]
    fn finds_what_the_service_refuses() {
        let mut bad = config(
            vec![
                CloudEditCommand {
                    delay: Some(61),
                    ..command("c1", "join", plain("/is home"))
                },
                CloudEditCommand {
                    every: Some(CloudEvery { min: 30, max: 600 }),
                    ..command("c2", "interval", plain("/spawn"))
                },
                CloudEditCommand {
                    every: Some(CloudEvery { min: 600, max: 300 }),
                    ..command("c3", "interval", plain("/spawn"))
                },
                CloudEditCommand {
                    at: Some(vec!["8:00".into()]),
                    ..command("c4", "daily", plain("/günlük"))
                },
                command("c4", "join", plain("%players%")),
                command("console", "teleport", plain(" x")),
            ],
            vec![CloudEditRule {
                pattern: "(a+)+$".into(),
                regex: true,
                owner_only: true,
                from: "server".into(),
                cooldown: 1,
                ..rule("r1", plain("\\quit"))
            }],
        );
        bad.anti_afk.delay_min = 5;
        let issues = validate(&bad, &limits());
        let has = |place: &str, index: Option<u32>, field: &str, problem: &str| {
            issues.iter().any(|i| {
                i.place == place && i.index == index && i.field == field && i.problem == problem
            })
        };
        assert!(has("general", None, "antiAfk", "range"));
        assert!(has("commands", Some(0), "delay", "range"));
        assert!(has("commands", Some(1), "every", "range"));
        assert!(has("commands", Some(2), "every", "order"));
        assert!(has("commands", Some(3), "at", "format"));
        assert!(has("commands", Some(4), "id", "duplicate"));
        assert!(has("commands", Some(4), "text", "variable"));
        assert!(has("commands", Some(5), "id", "format"));
        assert!(has("commands", Some(5), "on", "format"));
        assert!(has("commands", Some(5), "text", "space"));
        assert!(has("rules", Some(0), "match", "tooComplex"));
        assert!(has("rules", Some(0), "ownerOnly", "server"));
        assert!(has("rules", Some(0), "cooldown", "range"));
        assert!(has("rules", Some(0), "reply", "backslash"));
        assert!(
            validate(
                &config(vec![command("c1", "join", plain("/is home"))], vec![]),
                &limits()
            )
            .is_empty()
        );
    }

    #[test]
    fn counts_follow_studio() {
        let rows: Vec<CloudEditCommand> = (0..21)
            .map(|i| command(&format!("c{i}"), "join", plain("/is home")))
            .collect();
        let issues = validate(&config(rows, vec![]), &limits());
        assert_eq!(
            issues,
            vec![CloudIssue {
                place: "commands".into(),
                index: None,
                field: "count".into(),
                problem: "limit".into()
            }]
        );
    }

    #[test]
    fn seals_secrets_and_password_commands_typed_plain() {
        let server = CloudServer::new("play.projects.gg", 0);
        let key = test_key();
        let ctx = SealContext {
            key: &key,
            discord_id: "123456789012345678",
            account_id: "a_7KQ2MZ3P9TW4XB1C",
            server: &server,
            server_changed: false,
        };
        let edit = config(
            vec![
                command("c1", "join", secret("/login hunter2")),
                command("c2", "join", plain("/LOGIN hunter3")),
                command("c3", "daily", plain("/günlük")),
            ],
            vec![rule("r1", plain("Şu an AFK'yım"))],
        );
        let built = build(&edit, &limits(), &ctx, &RowCopies::new(), &HashMap::new()).unwrap();
        let c1 = &built.config.commands[0].payload;
        assert!(c1.secret && c1.sealed.as_deref().unwrap().starts_with("pl1.k1."));
        assert_eq!(c1.preview.as_deref(), Some("/login ******"));
        assert_eq!(c1.text, None);
        let c2 = &built.config.commands[1].payload;
        assert!(
            c2.secret && c2.sealed.is_some(),
            "a password typed plain is sealed anyway"
        );
        assert_eq!(
            built.config.commands[2].at,
            Some(vec!["08:00".into(), "20:30".into()])
        );
        assert_eq!(built.config.commands[2].delay, None);
        assert_eq!(built.config.commands[0].every, None);
        assert_eq!(built.config.rules[0].pattern, "selam");
        assert_eq!(
            built.config.rules[0].reply,
            PayloadInput::plain("Şu an AFK'yım")
        );
        assert_eq!(built.sealed.len(), 2);
        assert_eq!(built.sealed["c1"].text, "/login hunter2");
        assert_eq!(
            built.sealed["c1"].fp,
            fingerprint(c1.sealed.as_deref().unwrap())
        );
    }

    #[test]
    fn keeps_unchanged_copies_and_stored_seals() {
        let server = CloudServer::new("play.projects.gg", 0);
        let key = test_key();
        let ctx = SealContext {
            key: &key,
            discord_id: "1",
            account_id: "a_1",
            server: &server,
            server_changed: false,
        };
        let mut copies = RowCopies::new();
        copies.insert(
            "c1".into(),
            RowCopy {
                text: "/login hunter2".into(),
                fp: "FP1".into(),
            },
        );
        let fps = HashMap::from([
            ("c1".to_owned(), "FP1".to_owned()),
            ("c2".to_owned(), "FP2".to_owned()),
        ]);
        let edit = config(
            vec![
                command("c1", "join", secret("/login hunter2")),
                command(
                    "c2",
                    "join",
                    CloudRowText::Sealed {
                        preview: "/login ******".into(),
                    },
                ),
            ],
            vec![],
        );
        let built = build(&edit, &limits(), &ctx, &copies, &fps).unwrap();
        assert_eq!(built.config.commands[0].payload, PayloadInput::keep());
        assert_eq!(built.config.commands[1].payload, PayloadInput::keep());
        assert!(built.sealed.is_empty());
        assert_eq!(built.kept, vec!["c1".to_owned(), "c2".to_owned()]);
        // Changed words are sealed again.
        let edit = config(vec![command("c1", "join", secret("/login yeni"))], vec![]);
        let built = build(&edit, &limits(), &ctx, &copies, &fps).unwrap();
        assert!(built.config.commands[0].payload.sealed.is_some());
    }

    #[test]
    fn a_server_change_seals_again_or_asks_to_retype() {
        let server = CloudServer::new("mc.example.net", 0);
        let key = test_key();
        let ctx = SealContext {
            key: &key,
            discord_id: "1",
            account_id: "a_1",
            server: &server,
            server_changed: true,
        };
        let mut copies = RowCopies::new();
        copies.insert(
            "c1".into(),
            RowCopy {
                text: "/login hunter2".into(),
                fp: "FP1".into(),
            },
        );
        let fps = HashMap::from([("c1".to_owned(), "FP1".to_owned())]);
        let edit = config(
            vec![command("c1", "join", secret("/login hunter2"))],
            vec![],
        );
        let built = build(&edit, &limits(), &ctx, &copies, &fps).unwrap();
        assert!(
            built.config.commands[0].payload.sealed.is_some(),
            "sealed for the new server"
        );
        let edit = config(
            vec![],
            vec![rule(
                "r1",
                CloudRowText::Sealed {
                    preview: "/login ******".into(),
                },
            )],
        );
        match build(&edit, &limits(), &ctx, &copies, &fps) {
            Err(BuildError::Invalid(issues)) => assert_eq!(
                issues,
                vec![CloudIssue {
                    place: "rules".into(),
                    index: Some(0),
                    field: "reply".into(),
                    problem: "reseal".into()
                }]
            ),
            other => panic!("expected reseal, got {other:?}"),
        }
    }

    #[test]
    fn shows_copies_only_for_the_same_seal() {
        let view = CloudConfigView {
            v: 1,
            owner: String::new(),
            anti_afk: CloudAntiAfk::default(),
            commands: vec![
                CloudCommandView {
                    id: "c1".into(),
                    enabled: true,
                    on: "join".into(),
                    delay: Some(2),
                    secret: true,
                    preview: Some("/login ******".into()),
                    fp: Some("FP1".into()),
                    ..CloudCommandView::default()
                },
                CloudCommandView {
                    id: "c2".into(),
                    enabled: true,
                    on: "join".into(),
                    delay: Some(2),
                    secret: true,
                    preview: Some("/login ******".into()),
                    fp: Some("OTHER".into()),
                    ..CloudCommandView::default()
                },
            ],
            rules: vec![CloudRuleView {
                id: "r1".into(),
                reply: CloudPayloadView {
                    secret: false,
                    text: Some("selam".into()),
                    ..CloudPayloadView::default()
                },
                ..CloudRuleView::default()
            }],
        };
        let mut copies = RowCopies::new();
        for id in ["c1", "c2"] {
            copies.insert(
                id.into(),
                RowCopy {
                    text: "/login hunter2".into(),
                    fp: "FP1".into(),
                },
            );
        }
        let edit = edit_config(&view, &copies);
        assert_eq!(
            edit.commands[0].words,
            CloudRowText::Secret {
                text: "/login hunter2".into(),
                kept: true
            }
        );
        assert_eq!(
            edit.commands[1].words,
            CloudRowText::Sealed {
                preview: "/login ******".into()
            },
            "sealed again elsewhere: Göster instead"
        );
        assert_eq!(edit.rules[0].reply, plain("selam"));
        assert_eq!(fingerprints(&view).len(), 2);
    }

    #[test]
    fn refusal_paths_point_at_rows() {
        assert_eq!(
            CloudIssue::from_path("commands[2].every.min", "range"),
            CloudIssue {
                place: "commands".into(),
                index: Some(2),
                field: "every".into(),
                problem: "range".into()
            }
        );
        assert_eq!(
            CloudIssue::from_path("rules[0].reply", "variable").field,
            "reply"
        );
        assert_eq!(CloudIssue::from_path("commands[1]", "reseal").field, "text");
        assert_eq!(CloudIssue::from_path("commands", "limit").field, "count");
        assert_eq!(
            CloudIssue::from_path("antiAfk.delayMin", "range").field,
            "antiAfk"
        );
    }
}
