//! The per-account settings page, ported from the legacy `AfkAccountSettingsForm`.
//!
//! [`AccountSettings::from_account`] is `LoadFromAccount` (values clamped into the ranges the form's
//! numeric fields allow, replies shown without the `send ` prefix) and [`AccountSettings::apply`] is
//! `OnSaveClicked` + `ApplyToAccount` (trimming, swapping inverted min/max pairs, the `send ` prefix
//! rule for replies and the three "enabled but empty" checks).

use serde::{Deserialize, Serialize};

use crate::model::{
    AfkAccount, AlertsOptions, AntiAfkOptions, AutoEatOptions, AutoRelogOptions, AutoRespondMatch,
    DEFAULT_TASK_NAME, ScheduledTask, SchedulerOptions, normalize_minecraft_version,
};
use crate::util::eq_ignore_case;

/// Everything the settings page edits, in the shape the UI works with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AccountSettings {
    pub server_host: String,
    pub server_port: i32,
    pub minecraft_version: String,
    pub auto_start: bool,
    pub allow_resource_packs: bool,
    pub login_commands: LoginCommandsSettings,
    pub scheduler: SchedulerOptions,
    pub anti_afk: AntiAfkOptions,
    /// `retries` is -1 for "unlimited" (the form's toggle); otherwise 1-100.
    pub auto_relog: AutoRelogOptions,
    pub auto_eat: AutoEatOptions,
    pub auto_respond: AutoRespondSettings,
    pub chat_log: ChatLogSettings,
    pub alerts: AlertsOptions,
}

impl Default for AccountSettings {
    fn default() -> Self {
        Self::from_account(&AfkAccount::default(), &[])
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct LoginCommandsSettings {
    pub enabled: bool,
    pub only_first_login: bool,
    pub delay_seconds: f64,
    /// One command or chat line per entry, typed as-is in game.
    pub commands: Vec<String>,
}

impl Default for LoginCommandsSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            only_first_login: false,
            delay_seconds: 2.0,
            commands: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoRespondSettings {
    pub enabled: bool,
    pub sound_enabled: bool,
    pub match_colors: bool,
    pub rules: Vec<AutoRespondRule>,
}

/// One row of the auto-respond grid. Replies are what the user types (`selam`, `/home`); the
/// `send ` prefix MCC needs is added on save (see [`display_to_action`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoRespondRule {
    pub pattern: String,
    pub reply: String,
    pub private_reply: String,
    pub other_reply: String,
    pub owners_only: bool,
    pub cooldown_seconds: i32,
    pub is_regex: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatLogSettings {
    pub enabled: bool,
    pub add_date_time: bool,
    pub show_date: bool,
    pub filter: ChatLogFilter,
}

impl Default for ChatLogSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            add_date_time: true,
            show_date: false,
            filter: ChatLogFilter::Messages,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChatLogFilter {
    All,
    #[default]
    Messages,
    Chat,
}

impl ChatLogFilter {
    /// `FilterToIndex`: anything unknown is "messages".
    pub fn parse(value: &str) -> Self {
        if eq_ignore_case(value, "all") {
            Self::All
        } else if eq_ignore_case(value, "chat") {
            Self::Chat
        } else {
            Self::Messages
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Messages => "messages",
            Self::Chat => "chat",
        }
    }
}

/// Why the settings could not be saved (the legacy form's "Eksik Bilgi" warnings, in its order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ValidationIssue {
    /// Login commands are on but the list is empty.
    LoginCommandsEmpty,
    /// Auto-respond is on but no rule has both a pattern and a reply.
    AutoRespondNoRules,
    /// The scheduler is on but no task has an action.
    SchedulerNoTasks,
}

/// MCC action verbs other than `send` that a reply may start with; such replies are kept verbatim.
const RAW_ACTION_VERBS: &[&str] = &[
    "log",
    "script",
    "connect",
    "disconnect",
    "respawn",
    "move",
    "useitem",
    "dig",
    "sneak",
    "wait",
];

/// `ActionToDisplay`: `send selam` → `selam`; other actions are shown verbatim.
pub fn action_to_display(raw: &str) -> String {
    // Compare the first five characters (not bytes), like `StartsWith("send ", OrdinalIgnoreCase)`.
    let prefix_end = raw.char_indices().nth(5).map_or(raw.len(), |(i, _)| i);
    if raw.chars().count() >= 5 && eq_ignore_case(&raw[..prefix_end], "send ") {
        raw[prefix_end..].to_owned()
    } else {
        raw.to_owned()
    }
}

/// `DisplayToAction`: `selam` → `send selam`, `/home` → `send /home`; replies starting with another
/// MCC verb (`log`, `script`, ...) stay as typed. Empty stays empty.
pub fn display_to_action(display: &str) -> String {
    let text = display.trim();
    if text.is_empty() {
        return String::new();
    }
    let first_word = text.split(' ').next().unwrap_or(text);
    if RAW_ACTION_VERBS
        .iter()
        .any(|verb| eq_ignore_case(verb, first_word))
    {
        text.to_owned()
    } else {
        format!("send {text}")
    }
}

/// `SplitLines`: trims every line and drops the empty ones.
pub fn split_lines<S: AsRef<str>>(lines: &[S]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.as_ref().trim())
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    value.clamp(min, max)
}

/// Numeric fields can only hold finite numbers inside their range; anything else becomes `fallback`.
fn clamp_f64(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

/// `NormalizeRange`: swaps an inverted pair.
fn ordered(min: f64, max: f64) -> (f64, f64) {
    if min > max { (max, min) } else { (min, max) }
}

/// The relog retry count as the form stores it: -1 (unlimited) or 1-100.
fn form_retries(retries: i32) -> i32 {
    if retries < 0 {
        -1
    } else {
        clamp_i32(retries, 1, 100)
    }
}

impl AccountSettings {
    /// `LoadFromAccount`.
    pub fn from_account(account: &AfkAccount, login_commands: &[String]) -> Self {
        let anti_afk = &account.anti_afk;
        let relog = &account.auto_relog;
        let respond = &account.auto_respond;
        let chat_log = &account.chat_log;
        Self {
            server_host: account.server_host.clone(),
            server_port: clamp_i32(account.server_port, 0, 65535),
            minecraft_version: normalize_minecraft_version(&account.minecraft_version).to_owned(),
            auto_start: account.auto_start,
            allow_resource_packs: account.allow_resource_packs,
            login_commands: LoginCommandsSettings {
                enabled: account.login_commands.enabled,
                only_first_login: account.login_commands.only_first_login,
                delay_seconds: clamp_f64(account.login_commands.delay_seconds, 0.0, 60.0, 2.0),
                commands: login_commands.to_vec(),
            },
            scheduler: account.scheduler.clone(),
            anti_afk: AntiAfkOptions {
                delay_min: clamp_f64(anti_afk.delay_min, 1.0, 3600.0, 60.0),
                delay_max: clamp_f64(anti_afk.delay_max, 1.0, 3600.0, 90.0),
                walk_range: clamp_i32(anti_afk.walk_range, 1, 32),
                walk_retries: clamp_i32(anti_afk.walk_retries, 1, 100),
                ..anti_afk.clone()
            },
            auto_relog: AutoRelogOptions {
                delay_min: clamp_f64(relog.delay_min, 1.0, 3600.0, 5.0),
                delay_max: clamp_f64(relog.delay_max, 1.0, 3600.0, 10.0),
                retries: form_retries(relog.retries),
                ..relog.clone()
            },
            auto_eat: AutoEatOptions {
                enabled: account.auto_eat.enabled,
                threshold: clamp_i32(account.auto_eat.threshold, 0, 20),
            },
            auto_respond: AutoRespondSettings {
                enabled: respond.enabled,
                sound_enabled: respond.sound_enabled,
                match_colors: respond.match_colors,
                rules: respond
                    .matches
                    .iter()
                    .map(|m| AutoRespondRule {
                        pattern: m.pattern.clone(),
                        reply: action_to_display(&m.action),
                        private_reply: action_to_display(&m.private_action),
                        other_reply: action_to_display(&m.other_action),
                        owners_only: m.owners_only,
                        cooldown_seconds: m.cooldown_seconds,
                        is_regex: m.is_regex,
                    })
                    .collect(),
            },
            chat_log: ChatLogSettings {
                enabled: chat_log.enabled,
                add_date_time: chat_log.add_date_time,
                show_date: chat_log.show_date,
                filter: ChatLogFilter::parse(&chat_log.filter),
            },
            alerts: account.alerts.clone(),
        }
    }

    /// `OnSaveClicked` + `ApplyToAccount`: validates, then writes the normalized values into
    /// `account` (id and nickname untouched) and returns the normalized login commands. On failure
    /// `account` is left unchanged and every issue is reported in the legacy check order.
    pub fn apply(&self, account: &mut AfkAccount) -> Result<Vec<String>, Vec<ValidationIssue>> {
        let mut issues = Vec::new();

        let login_commands = split_lines(&self.login_commands.commands);
        if self.login_commands.enabled && login_commands.is_empty() {
            issues.push(ValidationIssue::LoginCommandsEmpty);
        }

        let matches = self.collect_matches();
        if self.auto_respond.enabled && matches.is_empty() {
            issues.push(ValidationIssue::AutoRespondNoRules);
        }

        let tasks = self.collect_tasks();
        if self.scheduler.enabled && tasks.is_empty() {
            issues.push(ValidationIssue::SchedulerNoTasks);
        }

        if !issues.is_empty() {
            return Err(issues);
        }

        account.server_host = self.server_host.trim().to_owned();
        account.server_port = clamp_i32(self.server_port, 0, 65535);
        account.minecraft_version = normalize_minecraft_version(&self.minecraft_version).to_owned();
        account.auto_start = self.auto_start;
        account.allow_resource_packs = self.allow_resource_packs;

        let login = &mut account.login_commands;
        login.enabled = self.login_commands.enabled;
        login.only_first_login = self.login_commands.only_first_login;
        login.delay_seconds = clamp_f64(self.login_commands.delay_seconds, 0.0, 60.0, 2.0);

        let source = &self.anti_afk;
        let (delay_min, delay_max) = ordered(
            clamp_f64(source.delay_min, 1.0, 3600.0, 60.0),
            clamp_f64(source.delay_max, 1.0, 3600.0, 90.0),
        );
        account.anti_afk = AntiAfkOptions {
            enabled: source.enabled,
            delay_min,
            delay_max,
            command: source.command.trim().to_owned(),
            use_sneak: source.use_sneak,
            use_terrain_handling: source.use_terrain_handling,
            walk_range: clamp_i32(source.walk_range, 1, 32),
            walk_retries: clamp_i32(source.walk_retries, 1, 100),
        };

        let source = &self.auto_relog;
        let (delay_min, delay_max) = ordered(
            clamp_f64(source.delay_min, 1.0, 3600.0, 5.0),
            clamp_f64(source.delay_max, 1.0, 3600.0, 10.0),
        );
        account.auto_relog = AutoRelogOptions {
            enabled: source.enabled,
            delay_min,
            delay_max,
            retries: form_retries(source.retries),
            ignore_kick_message: source.ignore_kick_message,
            kick_messages: split_lines(&source.kick_messages),
        };

        account.auto_eat = AutoEatOptions {
            enabled: self.auto_eat.enabled,
            threshold: clamp_i32(self.auto_eat.threshold, 0, 20),
        };

        let respond = &mut account.auto_respond;
        respond.enabled = self.auto_respond.enabled;
        respond.sound_enabled = self.auto_respond.sound_enabled;
        respond.match_colors = self.auto_respond.match_colors;
        respond.matches = matches;

        let chat_log = &mut account.chat_log;
        chat_log.enabled = self.chat_log.enabled;
        chat_log.add_date_time = self.chat_log.add_date_time;
        chat_log.show_date = self.chat_log.show_date;
        chat_log.filter = self.chat_log.filter.as_str().to_owned();

        let alerts = &mut account.alerts;
        alerts.enabled = self.alerts.enabled;
        alerts.beep_enabled = self.alerts.beep_enabled;
        alerts.trigger_by_words = self.alerts.trigger_by_words;
        alerts.log_to_file = self.alerts.log_to_file;
        alerts.matches = split_lines(&self.alerts.matches);
        alerts.excludes = split_lines(&self.alerts.excludes);

        account.scheduler.enabled = self.scheduler.enabled;
        account.scheduler.tasks = tasks;

        Ok(login_commands)
    }

    /// `CollectRespondMatches`: rows need a pattern and at least one reply.
    fn collect_matches(&self) -> Vec<AutoRespondMatch> {
        self.auto_respond
            .rules
            .iter()
            .filter_map(|rule| {
                let pattern = rule.pattern.trim();
                let action = display_to_action(&rule.reply);
                let private_action = display_to_action(&rule.private_reply);
                let other_action = display_to_action(&rule.other_reply);
                if pattern.is_empty()
                    || (action.is_empty() && private_action.is_empty() && other_action.is_empty())
                {
                    return None;
                }
                Some(AutoRespondMatch {
                    pattern: pattern.to_owned(),
                    is_regex: rule.is_regex,
                    action,
                    private_action,
                    other_action,
                    owners_only: rule.owners_only,
                    cooldown_seconds: rule.cooldown_seconds,
                })
            })
            .collect()
    }

    /// `CollectScheduledTasks`: rows need an action; unnamed tasks get the default name and an
    /// inverted interval is swapped. Non-finite seconds fall back to the grid defaults (300/600).
    fn collect_tasks(&self) -> Vec<ScheduledTask> {
        self.scheduler
            .tasks
            .iter()
            .filter_map(|task| {
                let action = task.action.trim();
                if action.is_empty() {
                    return None;
                }
                let name = task.name.trim();
                let min = if task.min_seconds.is_finite() {
                    task.min_seconds
                } else {
                    300.0
                };
                let max = if task.max_seconds.is_finite() {
                    task.max_seconds
                } else {
                    600.0
                };
                let (min_seconds, max_seconds) = ordered(min, max);
                Some(ScheduledTask {
                    name: if name.is_empty() {
                        DEFAULT_TASK_NAME
                    } else {
                        name
                    }
                    .to_owned(),
                    action: action.to_owned(),
                    on_login: task.on_login,
                    on_first_login: task.on_first_login,
                    interval_enabled: task.interval_enabled,
                    min_seconds,
                    max_seconds,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_prefix_round_trip() {
        assert_eq!(action_to_display("send selam"), "selam");
        assert_eq!(action_to_display("SEND /home"), "/home");
        assert_eq!(action_to_display("log merhaba"), "log merhaba");
        assert_eq!(action_to_display(""), "");
        assert_eq!(display_to_action("  selam  "), "send selam");
        assert_eq!(display_to_action("/home"), "send /home");
        assert_eq!(display_to_action("LOG not sent"), "LOG not sent");
        assert_eq!(display_to_action("script cevap.txt"), "script cevap.txt");
        assert_eq!(display_to_action("   "), "");
    }

    fn valid_settings() -> AccountSettings {
        AccountSettings::from_account(&AfkAccount::new("Test"), &[])
    }

    #[test]
    fn swaps_inverted_ranges_and_clamps_fields() {
        let mut settings = valid_settings();
        settings.anti_afk.delay_min = 500.0;
        settings.anti_afk.delay_max = 20.0;
        settings.auto_relog.delay_min = 0.0;
        settings.auto_relog.delay_max = 99999.0;
        settings.auto_relog.retries = 0;
        settings.anti_afk.walk_range = 99;
        settings.auto_eat.threshold = -3;
        settings.server_port = 70000;
        settings.login_commands.delay_seconds = f64::NAN;
        settings.server_host = "  mc.example.org ".into();
        settings.minecraft_version = "AUTO".into();
        settings.scheduler.tasks = vec![ScheduledTask {
            name: "   ".into(),
            action: " send /kit ".into(),
            min_seconds: 900.0,
            max_seconds: 30.0,
            ..ScheduledTask::default()
        }];

        let mut account = AfkAccount::new("Test");
        let id = account.id.clone();
        let commands = settings.apply(&mut account).unwrap();
        assert!(commands.is_empty());
        assert_eq!(account.id, id);
        assert_eq!(
            (account.anti_afk.delay_min, account.anti_afk.delay_max),
            (20.0, 500.0)
        );
        assert_eq!(
            (account.auto_relog.delay_min, account.auto_relog.delay_max),
            (1.0, 3600.0)
        );
        assert_eq!(account.auto_relog.retries, 1);
        assert_eq!(account.anti_afk.walk_range, 32);
        assert_eq!(account.auto_eat.threshold, 0);
        assert_eq!(account.server_port, 65535);
        assert_eq!(account.login_commands.delay_seconds, 2.0);
        assert_eq!(account.server_host, "mc.example.org");
        assert_eq!(account.minecraft_version, "auto");
        let task = &account.scheduler.tasks[0];
        assert_eq!(task.name, DEFAULT_TASK_NAME);
        assert_eq!(task.action, "send /kit");
        assert_eq!((task.min_seconds, task.max_seconds), (30.0, 900.0));
    }

    #[test]
    fn reports_every_enabled_but_empty_section_in_legacy_order() {
        let mut settings = valid_settings();
        settings.login_commands.enabled = true;
        settings.login_commands.commands = vec!["   ".into(), String::new()];
        settings.auto_respond.enabled = true;
        settings.auto_respond.rules = vec![
            AutoRespondRule {
                pattern: "selam".into(),
                ..AutoRespondRule::default()
            },
            AutoRespondRule {
                pattern: "  ".into(),
                reply: "hi".into(),
                ..AutoRespondRule::default()
            },
        ];
        settings.scheduler.enabled = true;
        settings.scheduler.tasks = vec![ScheduledTask {
            action: "  ".into(),
            ..ScheduledTask::default()
        }];

        let mut account = AfkAccount::new("Test");
        let before = account.clone();
        let issues = settings.apply(&mut account).unwrap_err();
        assert_eq!(
            issues,
            vec![
                ValidationIssue::LoginCommandsEmpty,
                ValidationIssue::AutoRespondNoRules,
                ValidationIssue::SchedulerNoTasks
            ]
        );
        assert_eq!(account, before, "a failed save must not touch the account");
    }

    #[test]
    fn replies_get_the_send_prefix_and_lists_are_split() {
        let mut settings = valid_settings();
        settings.auto_respond.enabled = true;
        settings.auto_respond.rules = vec![AutoRespondRule {
            pattern: "  selam ".into(),
            reply: "Merhaba $u".into(),
            private_reply: "log özel".into(),
            other_reply: String::new(),
            owners_only: true,
            cooldown_seconds: 5,
            is_regex: false,
        }];
        settings.login_commands.enabled = true;
        settings.login_commands.commands = vec!["  /login pw ".into(), "".into(), "/kit".into()];
        settings.alerts.matches = vec![" a ".into(), " ".into()];
        settings.auto_relog.kick_messages = vec!["  x ".into(), "".into()];
        settings.chat_log.filter = ChatLogFilter::Chat;

        let mut account = AfkAccount::new("Test");
        let commands = settings.apply(&mut account).unwrap();
        assert_eq!(commands, vec!["/login pw", "/kit"]);
        let rule = &account.auto_respond.matches[0];
        assert_eq!(rule.pattern, "selam");
        assert_eq!(rule.action, "send Merhaba $u");
        assert_eq!(rule.private_action, "log özel");
        assert_eq!(rule.other_action, "");
        assert_eq!(account.alerts.matches, vec!["a"]);
        assert_eq!(account.auto_relog.kick_messages, vec!["x"]);
        assert_eq!(account.chat_log.filter, "chat");

        // Loading the saved account shows the replies without the prefix again.
        let loaded = AccountSettings::from_account(&account, &commands);
        assert_eq!(loaded.auto_respond.rules[0].reply, "Merhaba $u");
        assert_eq!(loaded.login_commands.commands, commands);
    }

    #[test]
    fn load_clamps_like_the_numeric_fields() {
        let mut account = AfkAccount::new("Test");
        account.auto_relog.retries = 500;
        account.anti_afk.delay_min = 0.0;
        account.chat_log.filter = "ALL".into();
        account.minecraft_version = "weird".into();
        let settings = AccountSettings::from_account(&account, &[]);
        assert_eq!(settings.auto_relog.retries, 100);
        assert_eq!(settings.anti_afk.delay_min, 1.0);
        assert_eq!(settings.chat_log.filter, ChatLogFilter::All);
        assert_eq!(settings.minecraft_version, "26.3");
        account.auto_relog.retries = -1;
        assert_eq!(
            AccountSettings::from_account(&account, &[])
                .auto_relog
                .retries,
            -1
        );
    }
}
