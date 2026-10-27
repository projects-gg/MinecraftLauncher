//! "Buluta taşı": a desktop AFK account's settings in the cloud's form. The ideas are the same
//! (login commands, scheduled commands, auto-replies, anti-AFK), the cloud runs a safe subset:
//! walking, auto-eat, raw client actions, sounds, alert and chat log files and resource packs stay
//! on the desktop, and the plan says which of them the account had.
//!
//! Login commands live in the keychain on the desktop, so in the cloud they become secret rows
//! (sealed here, never sent in plain text). Scheduled commands and replies were plain on the
//! desktop and stay plain, unless they are password commands.

use serde::{Deserialize, Serialize};

use crate::config::{CloudEditCommand, CloudEditConfig, CloudEditRule, CloudRowText};
use crate::model::{CloudAntiAfk, CloudEvery, CloudLimits, CloudServer, CloudVersions};
use crate::server::{CloudAddressProblem, normalize};
use crate::text::{check_text, looks_sensitive};

/// What the cloud does not do, and how many of it the account had.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloudDroppedKind {
    /// Anti-AFK by walking (terrain handling).
    Walking,
    AutoEat,
    /// Client actions other than sending a line (`script`, `connect`, `move`, `dig`, …).
    RawActions,
    /// The sound when the account answers someone.
    Sounds,
    /// Alerts (their sound and file).
    Alerts,
    /// The chat log file.
    ChatLog,
    /// Downloading the server's resource pack.
    ResourcePacks,
    /// Own reconnect settings: the cloud always reconnects by the contract's rules.
    Relog,
    /// Rows beyond Studio's limits.
    TooMany,
    /// Rows whose words the cloud cannot send (an unknown `%variable%`, too long).
    Unusable,
    /// "Yalnız ana hesabımdan" rules while the launcher has no account name to listen to.
    OwnerOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudDropped {
    pub what: CloudDroppedKind,
    /// Rows or actions, where it counts them (1 for a setting).
    pub count: u32,
}

/// A value the cloud needs in another range, moved into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloudAdjusted {
    /// Intervals below Studio's minimum now wait that long.
    Interval,
    /// Auto-reply waits below Studio's minimum now wait that long.
    Cooldown,
    /// The game version is not one the machines have: the recommended one.
    Version,
}

/// The desktop account in the cloud's form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudMovePlan {
    pub name: String,
    /// The address as the service stores it; None when the cloud cannot connect there.
    pub server: Option<CloudServer>,
    pub server_problem: Option<CloudAddressProblem>,
    pub version: String,
    pub config: CloudEditConfig,
    pub dropped: Vec<CloudDropped>,
    pub adjusted: Vec<CloudAdjusted>,
}

/// What the plan needs besides the account.
#[derive(Debug, Clone, Copy)]
pub struct MoveContext<'a> {
    /// The launcher's active game account: whose messages "Yalnız ana hesabımdan" rules answer
    /// (the desktop's bot owner too).
    pub owner: &'a str,
    pub limits: &'a CloudLimits,
    pub versions: &'a CloudVersions,
}

struct Plan<'a> {
    ctx: MoveContext<'a>,
    commands: Vec<CloudEditCommand>,
    rules: Vec<CloudEditRule>,
    dropped: Vec<CloudDropped>,
    adjusted: Vec<CloudAdjusted>,
    raw_actions: u32,
    unusable: u32,
}

/// `send selam` → `selam`; any other client action is not a line to send.
fn sent_text(action: &str) -> Option<String> {
    let action = action.trim();
    let prefix_end = action
        .char_indices()
        .nth(5)
        .map_or(action.len(), |(i, _)| i);
    (action.chars().count() > 5 && action[..prefix_end].eq_ignore_ascii_case("send "))
        .then(|| action[prefix_end..].trim().to_owned())
        .filter(|text| !text.is_empty())
}

fn round_secs(seconds: f64) -> u32 {
    if seconds.is_finite() && seconds > 0.0 {
        seconds.round().min(f64::from(u32::MAX)) as u32
    } else {
        0
    }
}

impl Plan<'_> {
    fn note(&mut self, adjusted: CloudAdjusted) {
        if !self.adjusted.contains(&adjusted) {
            self.adjusted.push(adjusted);
        }
    }

    fn drop(&mut self, what: CloudDroppedKind, count: u32) {
        if count == 0 {
            return;
        }
        match self.dropped.iter_mut().find(|d| d.what == what) {
            Some(dropped) => dropped.count += count,
            None => self.dropped.push(CloudDropped { what, count }),
        }
    }

    /// The words of a row, or None (counted) when the cloud cannot send them.
    fn words(&mut self, text: &str, secret: bool) -> Option<CloudRowText> {
        let text = text.trim();
        if check_text(text).is_err() {
            self.unusable += 1;
            return None;
        }
        Some(if secret || looks_sensitive(text) {
            CloudRowText::Secret {
                text: text.to_owned(),
                kept: false,
            }
        } else {
            CloudRowText::Plain {
                text: text.to_owned(),
            }
        })
    }

    fn every(&mut self, min: f64, max: f64) -> CloudEvery {
        let floor = self.ctx.limits.min_interval_sec.max(1);
        let (mut min, mut max) = (round_secs(min.min(max)), round_secs(min.max(max)));
        if min < floor {
            self.note(CloudAdjusted::Interval);
            min = floor;
        }
        max = max.clamp(min, 86_400);
        min = min.min(max);
        CloudEvery { min, max }
    }

    fn command(
        &mut self,
        on: &str,
        enabled: bool,
        delay: Option<u32>,
        every: Option<CloudEvery>,
        words: CloudRowText,
    ) {
        let id = format!("c{}", self.commands.len() + 1);
        self.commands.push(CloudEditCommand {
            id,
            enabled,
            on: on.to_owned(),
            delay,
            every,
            at: None,
            words,
        });
    }
}

/// The plan for `account` (its login commands come from the keychain, `login_commands`).
pub fn plan(
    account: &pl_afk::AfkAccount,
    login_commands: &[String],
    ctx: MoveContext<'_>,
) -> CloudMovePlan {
    let mut plan = Plan {
        ctx,
        commands: Vec::new(),
        rules: Vec::new(),
        dropped: Vec::new(),
        adjusted: Vec::new(),
        raw_actions: 0,
        unusable: 0,
    };

    // Login commands: in order after every join, the desktop's wait before each. Every reconnect
    // is a new client process on the desktop too, so "only the first login" ran on each join.
    let login = &account.login_commands;
    let delay = round_secs(login.delay_seconds).min(60);
    for line in login_commands.iter().filter(|line| !line.trim().is_empty()) {
        if let Some(words) = plan.words(line, true) {
            plan.command("join", login.enabled, Some(delay), None, words);
        }
    }

    // The anti-AFK command: sent every round, now a row of its own.
    let anti = &account.anti_afk;
    if anti.enabled && !anti.command.trim().is_empty() {
        let every = plan.every(anti.delay_min, anti.delay_max);
        let command = anti.command.clone();
        if let Some(words) = plan.words(&command, false) {
            plan.command("interval", true, None, Some(every), words);
        }
    }
    if anti.enabled && anti.use_terrain_handling {
        plan.drop(CloudDroppedKind::Walking, 1);
    }

    // Scheduled commands: a login trigger is a join row, an interval an interval row.
    let scheduler = &account.scheduler;
    for task in scheduler
        .tasks
        .iter()
        .filter(|t| !t.action.trim().is_empty())
    {
        let Some(text) = sent_text(&task.action) else {
            plan.raw_actions += 1;
            continue;
        };
        if (task.on_login || task.on_first_login)
            && let Some(words) = plan.words(&text, false)
        {
            plan.command("join", scheduler.enabled, Some(0), None, words);
        }
        if task.interval_enabled {
            let every = plan.every(task.min_seconds, task.max_seconds);
            if let Some(words) = plan.words(&text, false) {
                plan.command("interval", scheduler.enabled, None, Some(every), words);
            }
        }
    }

    // Auto-replies: one rule per source that has a reply (one for all when the three are the
    // same).
    let respond = &account.auto_respond;
    let owner = if pl_afk::is_valid_nickname(ctx.owner.trim()) {
        ctx.owner.trim().to_owned()
    } else {
        String::new()
    };
    let mut owner_only_dropped = 0;
    for rule in respond
        .matches
        .iter()
        .filter(|r| !r.pattern.trim().is_empty())
    {
        let pattern = rule.pattern.trim().to_owned();
        if pl_afk::check_pattern(&pattern, rule.is_regex).is_some() {
            plan.unusable += 1;
            continue;
        }
        let replies: Vec<(&str, &str)> = [
            ("player", rule.action.as_str()),
            ("private", rule.private_action.as_str()),
            ("server", rule.other_action.as_str()),
        ]
        .into_iter()
        .filter(|(_, action)| !action.trim().is_empty())
        .collect();
        let mut sources: Vec<(&str, String)> = Vec::new();
        for (from, action) in replies {
            match sent_text(action) {
                Some(text) => sources.push((from, text)),
                None => plan.raw_actions += 1,
            }
        }
        if sources.len() == 3 && sources.iter().all(|(_, text)| *text == sources[0].1) {
            let text = sources[0].1.clone();
            sources = vec![("any", text)];
        }
        for (from, text) in sources {
            if rule.owners_only && (owner.is_empty() || from == "server") {
                // Without a name to listen to it would answer everyone; a server message has no
                // sender, so such a rule never answered one on the desktop either.
                if from != "server" {
                    owner_only_dropped += 1;
                }
                continue;
            }
            let Some(reply) = plan.words(&text, false) else {
                continue;
            };
            let floor = ctx.limits.min_rule_cooldown_sec;
            let mut cooldown = u32::try_from(rule.cooldown_seconds.max(0)).unwrap_or(0);
            if cooldown < floor {
                plan.note(CloudAdjusted::Cooldown);
                cooldown = floor;
            }
            let id = format!("r{}", plan.rules.len() + 1);
            plan.rules.push(CloudEditRule {
                id,
                enabled: respond.enabled,
                pattern: pattern.clone(),
                regex: rule.is_regex,
                from: from.to_owned(),
                owner_only: rule.owners_only,
                cooldown: cooldown.min(86_400),
                reply,
            });
        }
    }
    if respond.enabled && respond.sound_enabled {
        plan.drop(CloudDroppedKind::Sounds, 1);
    }

    let raw = plan.raw_actions;
    plan.drop(CloudDroppedKind::RawActions, raw);
    let unusable = plan.unusable;
    plan.drop(CloudDroppedKind::Unusable, unusable);
    plan.drop(CloudDroppedKind::OwnerOnly, owner_only_dropped);
    if account.auto_eat.enabled {
        plan.drop(CloudDroppedKind::AutoEat, 1);
    }
    if account.alerts.enabled {
        plan.drop(CloudDroppedKind::Alerts, 1);
    }
    if account.chat_log.enabled {
        plan.drop(CloudDroppedKind::ChatLog, 1);
    }
    if account.allow_resource_packs {
        plan.drop(CloudDroppedKind::ResourcePacks, 1);
    }
    let relog = &account.auto_relog;
    if !relog.enabled || relog.retries != -1 || !relog.ignore_kick_message {
        plan.drop(CloudDroppedKind::Relog, 1);
    }

    // Studio's limits: the first rows fit, the rest are counted.
    let max_commands = ctx.limits.max_command_rows as usize;
    let max_rules = ctx.limits.max_rules as usize;
    let over = plan.commands.len().saturating_sub(max_commands)
        + plan.rules.len().saturating_sub(max_rules);
    plan.commands.truncate(max_commands);
    plan.rules.truncate(max_rules);
    plan.drop(CloudDroppedKind::TooMany, over as u32);

    let version = match account.minecraft_version.trim() {
        "" => recommended(ctx.versions),
        pl_afk::AUTO_MINECRAFT_VERSION => pl_afk::AUTO_MINECRAFT_VERSION.to_owned(),
        wanted if ctx.versions.has(wanted) => wanted.to_owned(),
        _ => {
            plan.note(CloudAdjusted::Version);
            recommended(ctx.versions)
        }
    };

    let port = u16::try_from(account.server_port).unwrap_or(0);
    let (server, server_problem) = match normalize(account.effective_host(), port) {
        Ok(server) => (Some(server), None),
        Err(problem) => (None, Some(problem)),
    };

    CloudMovePlan {
        name: account.nickname.clone(),
        server,
        server_problem,
        version,
        config: CloudEditConfig {
            owner,
            anti_afk: CloudAntiAfk {
                enabled: anti.enabled,
                delay_min: round_secs(anti.delay_min.min(anti.delay_max)).clamp(10, 3_600),
                delay_max: round_secs(anti.delay_max.max(anti.delay_min)).clamp(10, 3_600),
            },
            commands: plan.commands,
            rules: plan.rules,
            // The cloud account's own inventory and movement choices stay as they are.
            inventory: None,
            movement: None,
        },
        dropped: plan.dropped,
        adjusted: plan.adjusted,
    }
}

/// The machines' recommended version, or `auto` when they list none.
fn recommended(versions: &CloudVersions) -> String {
    if !versions.recommended.is_empty() && versions.has(&versions.recommended) {
        versions.recommended.clone()
    } else {
        pl_afk::AUTO_MINECRAFT_VERSION.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use pl_afk::{AfkAccount, AutoRespondMatch, ScheduledTask};

    use super::*;
    use crate::config::validate;
    use crate::model::CloudVersionOption;

    fn versions() -> CloudVersions {
        CloudVersions {
            versions: vec![
                CloudVersionOption {
                    value: "26.3".into(),
                    label: "26.3".into(),
                },
                CloudVersionOption {
                    value: "1.21.4".into(),
                    label: "1.21.4".into(),
                },
            ],
            recommended: "26.3".into(),
        }
    }

    fn ctx<'a>(limits: &'a CloudLimits, versions: &'a CloudVersions) -> MoveContext<'a> {
        MoveContext {
            owner: "Ahmet",
            limits,
            versions,
        }
    }

    #[test]
    fn a_default_account_moves_as_it_is() {
        let limits = CloudLimits::default();
        let versions = versions();
        let account = AfkAccount::new("Afk_Bekci");
        let plan = plan(&account, &[], ctx(&limits, &versions));
        assert_eq!(plan.name, "Afk_Bekci");
        assert_eq!(plan.server, Some(CloudServer::new("play.projects.gg", 0)));
        assert_eq!(plan.version, "26.3");
        assert!(plan.config.commands.is_empty() && plan.config.rules.is_empty());
        assert!(plan.dropped.is_empty(), "{:?}", plan.dropped);
        assert!(plan.config.anti_afk.enabled);
        assert_eq!(
            (
                plan.config.anti_afk.delay_min,
                plan.config.anti_afk.delay_max
            ),
            (60, 90)
        );
        assert!(validate(&plan.config, &limits).is_empty());
    }

    #[test]
    fn login_commands_become_secret_join_rows() {
        let limits = CloudLimits::default();
        let versions = versions();
        let mut account = AfkAccount::new("Afk_Bekci");
        account.login_commands.enabled = true;
        account.login_commands.delay_seconds = 3.4;
        let commands = vec![
            "/login hunter2".to_owned(),
            " ".to_owned(),
            "/is home".to_owned(),
        ];
        let plan = plan(&account, &commands, ctx(&limits, &versions));
        assert_eq!(plan.config.commands.len(), 2);
        for (row, text) in plan
            .config
            .commands
            .iter()
            .zip(["/login hunter2", "/is home"])
        {
            assert_eq!(row.on, "join");
            assert_eq!(row.delay, Some(3));
            assert!(row.enabled);
            assert_eq!(
                row.words,
                CloudRowText::Secret {
                    text: text.into(),
                    kept: false
                }
            );
        }
        assert_eq!(plan.config.commands[0].id, "c1");
        assert!(validate(&plan.config, &limits).is_empty());
    }

    #[test]
    fn says_what_stays_on_the_desktop() {
        let limits = CloudLimits::default();
        let versions = versions();
        let mut account = AfkAccount::new("Afk_Bekci");
        account.anti_afk.use_terrain_handling = true;
        account.anti_afk.command = "/afk".into();
        account.anti_afk.delay_min = 20.0;
        account.auto_eat.enabled = true;
        account.alerts.enabled = true;
        account.chat_log.enabled = true;
        account.allow_resource_packs = true;
        account.auto_relog.retries = 5;
        account.scheduler.enabled = true;
        account.scheduler.tasks = vec![
            ScheduledTask {
                action: "send /kit daily".into(),
                on_login: true,
                interval_enabled: true,
                min_seconds: 5.0,
                max_seconds: 900_000.0,
                ..ScheduledTask::default()
            },
            ScheduledTask {
                action: "script yaz.cs".into(),
                ..ScheduledTask::default()
            },
        ];
        account.auto_respond.enabled = true;
        account.auto_respond.sound_enabled = true;
        account.auto_respond.matches = vec![
            AutoRespondMatch {
                pattern: "selam".into(),
                action: "send Selam!".into(),
                private_action: "send Selam!".into(),
                other_action: "send Selam!".into(),
                cooldown_seconds: 2,
                ..AutoRespondMatch::default()
            },
            AutoRespondMatch {
                pattern: "dur".into(),
                action: "send /afk off".into(),
                other_action: "log x".into(),
                owners_only: true,
                cooldown_seconds: 60,
                ..AutoRespondMatch::default()
            },
            AutoRespondMatch {
                pattern: "fiyat".into(),
                action: "send %players% kişi".into(),
                ..AutoRespondMatch::default()
            },
        ];
        account.minecraft_version = "1.8.9".into();
        let plan = plan(&account, &[], ctx(&limits, &versions));
        let count = |what| {
            plan.dropped
                .iter()
                .find(|d| d.what == what)
                .map(|d| d.count)
        };
        assert_eq!(count(CloudDroppedKind::Walking), Some(1));
        assert_eq!(count(CloudDroppedKind::AutoEat), Some(1));
        assert_eq!(count(CloudDroppedKind::Alerts), Some(1));
        assert_eq!(count(CloudDroppedKind::ChatLog), Some(1));
        assert_eq!(count(CloudDroppedKind::ResourcePacks), Some(1));
        assert_eq!(count(CloudDroppedKind::Relog), Some(1));
        assert_eq!(count(CloudDroppedKind::Sounds), Some(1));
        assert_eq!(
            count(CloudDroppedKind::RawActions),
            Some(2),
            "script and log"
        );
        assert_eq!(count(CloudDroppedKind::Unusable), Some(1), "%players%");
        assert_eq!(plan.version, "26.3");
        assert!(plan.adjusted.contains(&CloudAdjusted::Version));
        assert!(plan.adjusted.contains(&CloudAdjusted::Interval));
        assert!(plan.adjusted.contains(&CloudAdjusted::Cooldown));
        // The anti-AFK command, the task's login row and its interval row.
        let rows: Vec<(&str, &str)> = plan
            .config
            .commands
            .iter()
            .map(|row| {
                (
                    row.on.as_str(),
                    match &row.words {
                        CloudRowText::Plain { text } => text.as_str(),
                        _ => "",
                    },
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("interval", "/afk"),
                ("join", "/kit daily"),
                ("interval", "/kit daily")
            ]
        );
        assert_eq!(
            plan.config.commands[0].every,
            Some(CloudEvery { min: 60, max: 90 })
        );
        assert_eq!(
            plan.config.commands[2].every,
            Some(CloudEvery {
                min: 60,
                max: 86_400
            })
        );
        // One rule for all three sources, one for the owner's player chat.
        assert_eq!(plan.config.rules.len(), 2);
        assert_eq!(plan.config.rules[0].from, "any");
        assert_eq!(plan.config.rules[0].cooldown, 10);
        assert_eq!(plan.config.rules[1].from, "player");
        assert!(plan.config.rules[1].owner_only);
        assert_eq!(plan.config.owner, "Ahmet");
        assert!(
            validate(&plan.config, &limits).is_empty(),
            "{:?}",
            validate(&plan.config, &limits)
        );
    }

    #[test]
    fn owner_only_rules_need_a_name() {
        let limits = CloudLimits::default();
        let versions = versions();
        let mut account = AfkAccount::new("Afk_Bekci");
        account.auto_respond.enabled = true;
        account.auto_respond.matches = vec![AutoRespondMatch {
            pattern: "dur".into(),
            action: "send /afk off".into(),
            owners_only: true,
            cooldown_seconds: 30,
            ..AutoRespondMatch::default()
        }];
        let plan = plan(
            &account,
            &[],
            MoveContext {
                owner: "",
                limits: &limits,
                versions: &versions,
            },
        );
        assert!(plan.config.rules.is_empty());
        assert_eq!(
            plan.dropped,
            vec![CloudDropped {
                what: CloudDroppedKind::OwnerOnly,
                count: 1
            }]
        );
    }

    #[test]
    fn studio_limits_cut_the_list() {
        let limits = CloudLimits {
            max_command_rows: 2,
            ..CloudLimits::default()
        };
        let versions = versions();
        let mut account = AfkAccount::new("Afk_Bekci");
        account.login_commands.enabled = true;
        let commands: Vec<String> = (0..4).map(|i| format!("/warp {i}")).collect();
        let plan = plan(&account, &commands, ctx(&limits, &versions));
        assert_eq!(plan.config.commands.len(), 2);
        assert_eq!(
            plan.dropped,
            vec![CloudDropped {
                what: CloudDroppedKind::TooMany,
                count: 2
            }]
        );
    }

    #[test]
    fn a_private_server_cannot_move() {
        let limits = CloudLimits::default();
        let versions = versions();
        let mut account = AfkAccount::new("Afk_Bekci");
        account.server_host = "192.168.1.5".into();
        let plan = plan(&account, &[], ctx(&limits, &versions));
        assert_eq!(plan.server, None);
        assert_eq!(plan.server_problem, Some(CloudAddressProblem::Private));
    }

    #[test]
    fn only_sent_lines_are_rows() {
        assert_eq!(sent_text("send /home").as_deref(), Some("/home"));
        assert_eq!(sent_text("SEND selam").as_deref(), Some("selam"));
        assert_eq!(sent_text("send "), None);
        assert_eq!(sent_text("log selam"), None);
        assert_eq!(sent_text("script a.txt"), None);
    }
}
