//! The legacy on-disk formats: `accounts.json` and `settings.json` as written by the C# launcher
//! (Newtonsoft.Json, PascalCase, `NullValueHandling.Ignore`).
//!
//! Reading mirrors Newtonsoft's tolerance: property names match case-insensitively, `null` keeps the
//! default, numbers and booleans are accepted in string form. Values that cannot be understood fall
//! back to their default instead of discarding the whole file. One Newtonsoft quirk is deliberately
//! *not* reproduced: it appended stored `KickMessages` to the four pre-filled defaults, so the
//! legacy list grew by four duplicates on every load/save cycle (see the store's normalization).
//!
//! Writing produces the same shape and property order (including the read-only `EffectiveHost`
//! Newtonsoft used to emit). Login commands are written only for accounts whose commands could not
//! be moved to the OS keychain yet; everything else gets an empty `Commands` list.
//!
//! This module is also the entry point for importing the old launcher's
//! `%APPDATA%\.projects\afk\accounts.json`.

use std::collections::HashMap;

use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde_json::{Map, Value};

use crate::error::{AfkError, Result};
use crate::model::{
    AfkAccount, AfkSettings, AlertsOptions, AntiAfkOptions, AutoEatOptions, AutoRelogOptions,
    AutoRespondMatch, AutoRespondOptions, ChatLogOptions, DEFAULT_TASK_NAME, LoginCommandsOptions,
    ScheduledTask, SchedulerOptions, default_kick_messages,
};

/// An account read from a legacy file, with the login commands it stored in plain text.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyAccount {
    pub account: AfkAccount,
    pub login_commands: Vec<String>,
}

/// Parses a legacy `accounts.json` (a JSON list; `null` means no accounts). Entries that are not
/// objects are skipped. Fails only when the text is not JSON or the root is not a list.
pub fn parse_accounts(json: &str) -> Result<Vec<LegacyAccount>> {
    let json = json.trim_start_matches('\u{feff}');
    let value: Value =
        serde_json::from_str(json).map_err(|e| AfkError::json("accounts.json", e))?;
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(items) => Ok(items
            .iter()
            .filter_map(Value::as_object)
            .map(|object| read_account(Obj(object)))
            .collect()),
        _ => Err(AfkError::InvalidData(
            "accounts.json does not contain a list".into(),
        )),
    }
}

/// Parses one legacy account object (as Newtonsoft serialized a single `AfkAccount`).
pub fn parse_account(json: &str) -> Result<LegacyAccount> {
    let json = json.trim_start_matches('\u{feff}');
    let value: Value = serde_json::from_str(json).map_err(|e| AfkError::json("account", e))?;
    value
        .as_object()
        .map(|object| read_account(Obj(object)))
        .ok_or_else(|| AfkError::InvalidData("account is not a JSON object".into()))
}

/// Parses `settings.json`; anything unreadable yields the defaults (legacy behavior).
pub fn parse_settings(json: &str) -> AfkSettings {
    let json = json.trim_start_matches('\u{feff}');
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Object(object)) => AfkSettings {
            show_script_commands: Obj(&object).bool("ShowScriptCommands", false),
            console_looks: Obj(&object).bool("ConsoleLooks", false),
            dialog_cards: Obj(&object).bool("DialogCards", false),
        },
        _ => AfkSettings::default(),
    }
}

/// Serializes accounts in the legacy shape. `plaintext` holds login commands that must stay in the
/// file because the keychain could not take them.
pub(crate) fn write_accounts(
    accounts: &[AfkAccount],
    plaintext: &HashMap<String, Vec<String>>,
) -> Result<String> {
    let records: Vec<AccountOut<'_>> = accounts
        .iter()
        .map(|account| AccountOut {
            account,
            commands: plaintext.get(&account.id).map_or(&[], Vec::as_slice),
        })
        .collect();
    serde_json::to_string_pretty(&records).map_err(|e| AfkError::json("accounts.json", e))
}

pub(crate) fn write_settings(settings: &AfkSettings) -> Result<String> {
    struct Out<'a>(&'a AfkSettings);
    impl Serialize for Out<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
            let mut st = s.serialize_struct("Data", 3)?;
            st.serialize_field("ShowScriptCommands", &self.0.show_script_commands)?;
            // The v1 launcher reads only its own field.
            st.serialize_field("ConsoleLooks", &self.0.console_looks)?;
            st.serialize_field("DialogCards", &self.0.dialog_cards)?;
            st.end()
        }
    }
    serde_json::to_string_pretty(&Out(settings)).map_err(|e| AfkError::json("settings.json", e))
}

// --- Reading -------------------------------------------------------------------------------------

fn read_account(o: Obj<'_>) -> LegacyAccount {
    let login = o.object("LoginCommands");
    let account = AfkAccount {
        id: o.string("Id", ""),
        nickname: o.string("Nickname", ""),
        server_host: o.string("ServerHost", ""),
        server_port: o.i32("ServerPort", 0),
        auto_start: o.bool("AutoStart", false),
        // Missing means the legacy default, which today is "follow the recommended version".
        minecraft_version: o.string("MinecraftVersion", ""),
        allow_resource_packs: o.bool("AllowResourcePacks", false),
        anti_afk: o.object("AntiAfk").map(read_anti_afk).unwrap_or_default(),
        auto_relog: o
            .object("AutoRelog")
            .map(read_auto_relog)
            .unwrap_or_default(),
        auto_eat: o.object("AutoEat").map(read_auto_eat).unwrap_or_default(),
        auto_respond: o
            .object("AutoRespond")
            .map(read_auto_respond)
            .unwrap_or_default(),
        chat_log: o.object("ChatLog").map(read_chat_log).unwrap_or_default(),
        alerts: o.object("Alerts").map(read_alerts).unwrap_or_default(),
        scheduler: o
            .object("Scheduler")
            .map(read_scheduler)
            .unwrap_or_default(),
        login_commands: login.map(read_login_options).unwrap_or_default(),
    };
    let login_commands = login
        .map(|l| l.strings("Commands", Vec::new))
        .unwrap_or_default();
    LegacyAccount {
        account,
        login_commands,
    }
}

fn read_anti_afk(o: Obj<'_>) -> AntiAfkOptions {
    let d = AntiAfkOptions::default();
    AntiAfkOptions {
        enabled: o.bool("Enabled", d.enabled),
        delay_min: o.f64("DelayMin", d.delay_min),
        delay_max: o.f64("DelayMax", d.delay_max),
        command: o.string("Command", &d.command),
        use_sneak: o.bool("UseSneak", d.use_sneak),
        use_terrain_handling: o.bool("UseTerrainHandling", d.use_terrain_handling),
        walk_range: o.i32("WalkRange", d.walk_range),
        walk_retries: o.i32("WalkRetries", d.walk_retries),
    }
}

fn read_auto_relog(o: Obj<'_>) -> AutoRelogOptions {
    let d = AutoRelogOptions::default();
    AutoRelogOptions {
        enabled: o.bool("Enabled", d.enabled),
        delay_min: o.f64("DelayMin", d.delay_min),
        delay_max: o.f64("DelayMax", d.delay_max),
        retries: o.i32("Retries", d.retries),
        ignore_kick_message: o.bool("IgnoreKickMessage", d.ignore_kick_message),
        kick_messages: o.strings("KickMessages", default_kick_messages),
    }
}

fn read_auto_eat(o: Obj<'_>) -> AutoEatOptions {
    let d = AutoEatOptions::default();
    AutoEatOptions {
        enabled: o.bool("Enabled", d.enabled),
        threshold: o.i32("Threshold", d.threshold),
    }
}

fn read_auto_respond(o: Obj<'_>) -> AutoRespondOptions {
    AutoRespondOptions {
        enabled: o.bool("Enabled", false),
        match_colors: o.bool("MatchColors", false),
        sound_enabled: o.bool("SoundEnabled", false),
        matches: o
            .objects("Matches")
            .into_iter()
            .map(|m| AutoRespondMatch {
                pattern: m.string("Pattern", ""),
                is_regex: m.bool("IsRegex", false),
                action: m.string("Action", ""),
                private_action: m.string("PrivateAction", ""),
                other_action: m.string("OtherAction", ""),
                owners_only: m.bool("OwnersOnly", false),
                cooldown_seconds: m.i32("CooldownSeconds", 0),
            })
            .collect(),
    }
}

fn read_chat_log(o: Obj<'_>) -> ChatLogOptions {
    let d = ChatLogOptions::default();
    ChatLogOptions {
        enabled: o.bool("Enabled", d.enabled),
        add_date_time: o.bool("AddDateTime", d.add_date_time),
        show_date: o.bool("ShowDate", d.show_date),
        filter: o.string("Filter", &d.filter),
    }
}

fn read_alerts(o: Obj<'_>) -> AlertsOptions {
    let d = AlertsOptions::default();
    AlertsOptions {
        enabled: o.bool("Enabled", d.enabled),
        beep_enabled: o.bool("BeepEnabled", d.beep_enabled),
        trigger_by_words: o.bool("TriggerByWords", d.trigger_by_words),
        log_to_file: o.bool("LogToFile", d.log_to_file),
        matches: o.strings("Matches", Vec::new),
        excludes: o.strings("Excludes", Vec::new),
    }
}

fn read_scheduler(o: Obj<'_>) -> SchedulerOptions {
    let d = ScheduledTask::default();
    SchedulerOptions {
        enabled: o.bool("Enabled", false),
        tasks: o
            .objects("Tasks")
            .into_iter()
            .map(|t| ScheduledTask {
                name: t.string("Name", DEFAULT_TASK_NAME),
                action: t.string("Action", ""),
                on_login: t.bool("OnLogin", d.on_login),
                on_first_login: t.bool("OnFirstLogin", d.on_first_login),
                interval_enabled: t.bool("IntervalEnabled", d.interval_enabled),
                min_seconds: t.f64("MinSeconds", d.min_seconds),
                max_seconds: t.f64("MaxSeconds", d.max_seconds),
            })
            .collect(),
    }
}

fn read_login_options(o: Obj<'_>) -> LoginCommandsOptions {
    let d = LoginCommandsOptions::default();
    LoginCommandsOptions {
        enabled: o.bool("Enabled", d.enabled),
        only_first_login: o.bool("OnlyFirstLogin", d.only_first_login),
        delay_seconds: o.f64("DelaySeconds", d.delay_seconds),
    }
}

/// A JSON object read the way Newtonsoft binds properties.
#[derive(Clone, Copy)]
struct Obj<'a>(&'a Map<String, Value>);

impl<'a> Obj<'a> {
    /// Exact name first, then case-insensitive; `null` counts as absent.
    fn field(&self, name: &str) -> Option<&'a Value> {
        let value = self.0.get(name).or_else(|| {
            self.0
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value)
        })?;
        (!value.is_null()).then_some(value)
    }

    fn string(&self, name: &str, default: &str) -> String {
        self.field(name)
            .and_then(as_string)
            .unwrap_or_else(|| default.to_owned())
    }

    fn bool(&self, name: &str, default: bool) -> bool {
        self.field(name).and_then(as_bool).unwrap_or(default)
    }

    fn i32(&self, name: &str, default: i32) -> i32 {
        self.field(name).and_then(as_i32).unwrap_or(default)
    }

    fn f64(&self, name: &str, default: f64) -> f64 {
        self.field(name).and_then(as_f64).unwrap_or(default)
    }

    /// A list of strings; `null` items become empty strings (they are ignored downstream).
    fn strings(&self, name: &str, default: impl FnOnce() -> Vec<String>) -> Vec<String> {
        match self.field(name) {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(|item| match item {
                    Value::Null => Some(String::new()),
                    other => as_string(other),
                })
                .collect(),
            _ => default(),
        }
    }

    fn object(&self, name: &str) -> Option<Obj<'a>> {
        self.field(name).and_then(Value::as_object).map(Obj)
    }

    fn objects(&self, name: &str) -> Vec<Obj<'a>> {
        match self.field(name) {
            Some(Value::Array(items)) => {
                items.iter().filter_map(Value::as_object).map(Obj).collect()
            }
            _ => Vec::new(),
        }
    }
}

fn as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "True" } else { "False" }.to_owned()),
        _ => None,
    }
}

fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_f64().map(|f| f != 0.0),
        Value::String(s) => {
            let s = s.trim();
            if s.eq_ignore_ascii_case("true") {
                Some(true)
            } else if s.eq_ignore_ascii_case("false") {
                Some(false)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Integers as Newtonsoft converts them: integral JSON numbers, floats rounded half-to-even
/// (`Convert.ToInt32`), numeric strings; out-of-range values are rejected.
fn as_i32(value: &Value) -> Option<i32> {
    let from_f64 = |f: f64| {
        let rounded = f.round_ties_even();
        (rounded.is_finite() && rounded >= f64::from(i32::MIN) && rounded <= f64::from(i32::MAX))
            .then_some(rounded as i32)
    };
    match value {
        Value::Number(n) => match n.as_i64() {
            Some(i) => i32::try_from(i).ok(),
            None => n.as_f64().and_then(from_f64),
        },
        Value::String(s) => {
            let s = s.trim();
            match s.parse::<i64>() {
                Ok(i) => i32::try_from(i).ok(),
                Err(_) => s.parse::<f64>().ok().and_then(from_f64),
            }
        }
        _ => None,
    }
}

// --- Writing -------------------------------------------------------------------------------------

struct AccountOut<'a> {
    account: &'a AfkAccount,
    commands: &'a [String],
}

impl Serialize for AccountOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let a = self.account;
        let mut st = s.serialize_struct("AfkAccount", 16)?;
        st.serialize_field("Id", &a.id)?;
        st.serialize_field("Nickname", &a.nickname)?;
        st.serialize_field("ServerHost", &a.server_host)?;
        st.serialize_field("ServerPort", &a.server_port)?;
        st.serialize_field("AutoStart", &a.auto_start)?;
        st.serialize_field("MinecraftVersion", &a.minecraft_version)?;
        st.serialize_field("AllowResourcePacks", &a.allow_resource_packs)?;
        st.serialize_field("AntiAfk", &AntiAfkOut(&a.anti_afk))?;
        st.serialize_field("AutoRelog", &AutoRelogOut(&a.auto_relog))?;
        st.serialize_field("AutoEat", &AutoEatOut(&a.auto_eat))?;
        st.serialize_field("AutoRespond", &AutoRespondOut(&a.auto_respond))?;
        st.serialize_field("ChatLog", &ChatLogOut(&a.chat_log))?;
        st.serialize_field("Alerts", &AlertsOut(&a.alerts))?;
        st.serialize_field("Scheduler", &SchedulerOut(&a.scheduler))?;
        st.serialize_field(
            "LoginCommands",
            &LoginOut {
                options: &a.login_commands,
                commands: self.commands,
            },
        )?;
        st.serialize_field("EffectiveHost", a.effective_host())?;
        st.end()
    }
}

struct AntiAfkOut<'a>(&'a AntiAfkOptions);

impl Serialize for AntiAfkOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let o = self.0;
        let mut st = s.serialize_struct("AntiAfkOptions", 8)?;
        st.serialize_field("Enabled", &o.enabled)?;
        st.serialize_field("DelayMin", &o.delay_min)?;
        st.serialize_field("DelayMax", &o.delay_max)?;
        st.serialize_field("Command", &o.command)?;
        st.serialize_field("UseSneak", &o.use_sneak)?;
        st.serialize_field("UseTerrainHandling", &o.use_terrain_handling)?;
        st.serialize_field("WalkRange", &o.walk_range)?;
        st.serialize_field("WalkRetries", &o.walk_retries)?;
        st.end()
    }
}

struct AutoRelogOut<'a>(&'a AutoRelogOptions);

impl Serialize for AutoRelogOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let o = self.0;
        let mut st = s.serialize_struct("AutoRelogOptions", 6)?;
        st.serialize_field("Enabled", &o.enabled)?;
        st.serialize_field("DelayMin", &o.delay_min)?;
        st.serialize_field("DelayMax", &o.delay_max)?;
        st.serialize_field("Retries", &o.retries)?;
        st.serialize_field("IgnoreKickMessage", &o.ignore_kick_message)?;
        st.serialize_field("KickMessages", &o.kick_messages)?;
        st.end()
    }
}

struct AutoEatOut<'a>(&'a AutoEatOptions);

impl Serialize for AutoEatOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("AutoEatOptions", 2)?;
        st.serialize_field("Enabled", &self.0.enabled)?;
        st.serialize_field("Threshold", &self.0.threshold)?;
        st.end()
    }
}

struct AutoRespondOut<'a>(&'a AutoRespondOptions);

impl Serialize for AutoRespondOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let o = self.0;
        let matches: Vec<MatchOut<'_>> = o.matches.iter().map(MatchOut).collect();
        let mut st = s.serialize_struct("AutoRespondOptions", 4)?;
        st.serialize_field("Enabled", &o.enabled)?;
        st.serialize_field("MatchColors", &o.match_colors)?;
        st.serialize_field("SoundEnabled", &o.sound_enabled)?;
        st.serialize_field("Matches", &matches)?;
        st.end()
    }
}

struct MatchOut<'a>(&'a AutoRespondMatch);

impl Serialize for MatchOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let m = self.0;
        let mut st = s.serialize_struct("AutoRespondMatch", 7)?;
        st.serialize_field("Pattern", &m.pattern)?;
        st.serialize_field("IsRegex", &m.is_regex)?;
        st.serialize_field("Action", &m.action)?;
        st.serialize_field("PrivateAction", &m.private_action)?;
        st.serialize_field("OtherAction", &m.other_action)?;
        st.serialize_field("OwnersOnly", &m.owners_only)?;
        st.serialize_field("CooldownSeconds", &m.cooldown_seconds)?;
        st.end()
    }
}

struct ChatLogOut<'a>(&'a ChatLogOptions);

impl Serialize for ChatLogOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let o = self.0;
        let mut st = s.serialize_struct("ChatLogOptions", 4)?;
        st.serialize_field("Enabled", &o.enabled)?;
        st.serialize_field("AddDateTime", &o.add_date_time)?;
        st.serialize_field("ShowDate", &o.show_date)?;
        st.serialize_field("Filter", &o.filter)?;
        st.end()
    }
}

struct AlertsOut<'a>(&'a AlertsOptions);

impl Serialize for AlertsOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let o = self.0;
        let mut st = s.serialize_struct("AlertsOptions", 6)?;
        st.serialize_field("Enabled", &o.enabled)?;
        st.serialize_field("BeepEnabled", &o.beep_enabled)?;
        st.serialize_field("TriggerByWords", &o.trigger_by_words)?;
        st.serialize_field("LogToFile", &o.log_to_file)?;
        st.serialize_field("Matches", &o.matches)?;
        st.serialize_field("Excludes", &o.excludes)?;
        st.end()
    }
}

struct SchedulerOut<'a>(&'a SchedulerOptions);

impl Serialize for SchedulerOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let tasks: Vec<TaskOut<'_>> = self.0.tasks.iter().map(TaskOut).collect();
        let mut st = s.serialize_struct("SchedulerOptions", 2)?;
        st.serialize_field("Enabled", &self.0.enabled)?;
        st.serialize_field("Tasks", &tasks)?;
        st.end()
    }
}

struct TaskOut<'a>(&'a ScheduledTask);

impl Serialize for TaskOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let t = self.0;
        let mut st = s.serialize_struct("ScheduledTask", 7)?;
        st.serialize_field("Name", &t.name)?;
        st.serialize_field("Action", &t.action)?;
        st.serialize_field("OnLogin", &t.on_login)?;
        st.serialize_field("OnFirstLogin", &t.on_first_login)?;
        st.serialize_field("IntervalEnabled", &t.interval_enabled)?;
        st.serialize_field("MinSeconds", &t.min_seconds)?;
        st.serialize_field("MaxSeconds", &t.max_seconds)?;
        st.end()
    }
}

struct LoginOut<'a> {
    options: &'a LoginCommandsOptions,
    commands: &'a [String],
}

impl Serialize for LoginOut<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let o = self.options;
        let mut st = s.serialize_struct("LoginCommandsOptions", 4)?;
        st.serialize_field("Enabled", &o.enabled)?;
        st.serialize_field("OnlyFirstLogin", &o.only_first_login)?;
        st.serialize_field("DelaySeconds", &o.delay_seconds)?;
        st.serialize_field("Commands", self.commands)?;
        st.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_null_fields_take_legacy_defaults() {
        let parsed = parse_accounts(
            r#"[{"Id":"abc","Nickname":"Test","MinecraftVersion":null,"AntiAfk":null,
                 "AutoRelog":{"Enabled":false},"ChatLog":{"Filter":null},
                 "Scheduler":{"Enabled":true,"Tasks":[null,{"Action":"send /kit","Name":null}]}}]"#,
        )
        .unwrap();
        let account = &parsed[0].account;
        assert_eq!(
            account.minecraft_version, "",
            "follows the recommended version"
        );
        assert_eq!(account.anti_afk, AntiAfkOptions::default());
        assert!(!account.auto_relog.enabled);
        assert_eq!(account.auto_relog.kick_messages, default_kick_messages());
        assert_eq!(account.chat_log.filter, "messages");
        assert_eq!(account.scheduler.tasks.len(), 1);
        assert_eq!(account.scheduler.tasks[0].name, DEFAULT_TASK_NAME);
        assert!(parsed[0].login_commands.is_empty());
    }

    #[test]
    fn tolerates_newtonsoft_style_coercions() {
        let parsed = parse_account(
            r#"{"nickname":"Lower","ServerPort":"25570","AutoStart":"True","AutoEat":{"Threshold":7.5},
                "AutoRelog":{"Retries":"x","KickMessages":["a",null,3]},
                "LoginCommands":{"Enabled":1,"Commands":["/login pw"]}}"#,
        )
        .unwrap();
        let a = &parsed.account;
        assert_eq!(a.nickname, "Lower");
        assert_eq!(a.server_port, 25570);
        assert!(a.auto_start);
        assert_eq!(a.auto_eat.threshold, 8, "7.5 rounds half to even");
        assert_eq!(
            a.auto_relog.retries, -1,
            "unreadable values keep the default"
        );
        assert_eq!(a.auto_relog.kick_messages, vec!["a", "", "3"]);
        assert!(a.login_commands.enabled);
        assert_eq!(parsed.login_commands, vec!["/login pw"]);
    }

    #[test]
    fn rejects_non_list_roots_but_accepts_null() {
        assert!(parse_accounts("null").unwrap().is_empty());
        assert!(parse_accounts("{}").is_err());
        assert!(parse_accounts("[1, \"x\"]").unwrap().is_empty());
        assert!(parse_accounts("not json").is_err());
    }

    #[test]
    fn writes_the_legacy_shape_in_declaration_order() {
        let mut account = AfkAccount::new("Test");
        account.id = "abc".into();
        let mut plaintext = HashMap::new();
        let none = write_accounts(std::slice::from_ref(&account), &plaintext).unwrap();
        let keys: Vec<String> = serde_json::from_str::<Value>(&none).unwrap()[0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        // serde_json maps sort keys; check order on the raw text instead.
        let positions: Vec<usize> = [
            "\"Id\"",
            "\"Nickname\"",
            "\"ServerHost\"",
            "\"ServerPort\"",
            "\"AutoStart\"",
            "\"MinecraftVersion\"",
            "\"AllowResourcePacks\"",
            "\"AntiAfk\"",
            "\"AutoRelog\"",
            "\"AutoEat\"",
            "\"AutoRespond\"",
            "\"ChatLog\"",
            "\"Alerts\"",
            "\"Scheduler\"",
            "\"LoginCommands\"",
            "\"EffectiveHost\"",
        ]
        .iter()
        .map(|k| none.find(k).unwrap())
        .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{none}");
        assert_eq!(keys.len(), 16);
        assert!(none.contains("\"Commands\": []"));
        assert!(none.contains("\"DelayMin\": 60.0"));

        plaintext.insert("abc".to_owned(), vec!["/login pw".to_owned()]);
        let with = write_accounts(&[account.clone()], &plaintext).unwrap();
        let back = parse_accounts(&with).unwrap();
        assert_eq!(back[0].account, account);
        assert_eq!(back[0].login_commands, vec!["/login pw"]);
    }

    #[test]
    fn settings_round_trip_and_fallback() {
        let text = write_settings(&AfkSettings {
            show_script_commands: true,
            ..AfkSettings::default()
        })
        .unwrap();
        assert!(text.contains("\"ShowScriptCommands\": true"));
        assert!(parse_settings(&text).show_script_commands);
        assert!(!parse_settings("{broken").show_script_commands);
        assert!(!parse_settings("[]").show_script_commands);
        // Studio's console switches as last known.
        let switches = AfkSettings {
            console_looks: true,
            dialog_cards: true,
            ..AfkSettings::default()
        };
        assert_eq!(
            parse_settings(&write_settings(&switches).unwrap()),
            switches
        );
        assert!(!parse_settings(&text).console_looks);
    }
}
