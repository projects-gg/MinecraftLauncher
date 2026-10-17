//! MCC configuration for one account, byte-for-byte identical to the legacy `AfkConfigWriter`.
//!
//! Three files are produced per launch: `MinecraftClient.ini` (TOML), `matches.ini` (MCC's own INI
//! format for AutoRespond) and `giris-komutlari.txt` (the login-commands script run by the
//! ScriptScheduler). All are UTF-8 without BOM with CRLF line endings — the legacy writer used
//! `StringBuilder.AppendLine` on Windows, and MCC's readers (Tomlet, `File.ReadAllLines`) accept
//! CRLF on every platform. Numbers are culture invariant, exactly as .NET Framework 4.8 formats them.

use std::path::Path;

use crate::error::{AfkError, Result};
use crate::model::{
    AfkAccount, AlertsOptions, AntiAfkOptions, AutoEatOptions, AutoRelogOptions,
    AutoRespondOptions, ChatLogOptions, DEFAULT_TASK_NAME, LoginCommandsOptions, ScheduledTask,
};
use crate::pattern::{check_pattern, matches_line};
use crate::util::write_atomic;
use crate::versions::VersionCatalog;

pub const INI_FILE_NAME: &str = "MinecraftClient.ini";
pub const MATCHES_FILE_NAME: &str = "matches.ini";
/// MCC script with the login commands; may contain passwords, so it only exists while needed.
pub const LOGIN_SCRIPT_FILE_NAME: &str = "giris-komutlari.txt";

/// Name and action of the scheduler task that runs the login script.
const LOGIN_TASK_NAME: &str = "Giriş Komutları";

/// MCC's classic script interpreter counts `wait N` in client ticks (20 per second).
const SCRIPT_TICKS_PER_SECOND: f64 = 20.0;

const NEWLINE: &str = "\r\n";

/// The three generated files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFiles {
    pub ini: String,
    pub matches: String,
    pub login_script: String,
}

/// Builds all files for `account`. `login_commands` is the raw stored list (blank entries are
/// ignored); `launcher_nickname` is the player's main launcher nickname, written as the only bot
/// owner (blank means none); `versions` resolves the stored game version (see [`VersionCatalog`]).
pub fn build_config(
    account: &AfkAccount,
    login_commands: &[String],
    launcher_nickname: &str,
    versions: &VersionCatalog,
) -> ConfigFiles {
    ConfigFiles {
        ini: build_ini(account, login_commands, launcher_nickname, versions),
        matches: build_matches_ini(&account.auto_respond),
        login_script: build_login_script(&account.login_commands, login_commands),
    }
}

/// Writes the files into the account folder (created if needed). The login script is restricted
/// to the owner on Unix.
pub fn write_config(dir: &Path, files: &ConfigFiles) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| AfkError::io(dir, e))?;
    for (name, content, private) in [
        (INI_FILE_NAME, &files.ini, false),
        (MATCHES_FILE_NAME, &files.matches, false),
        (LOGIN_SCRIPT_FILE_NAME, &files.login_script, true),
    ] {
        let path = dir.join(name);
        write_atomic(&path, content.as_bytes(), private).map_err(|e| AfkError::io(&path, e))?;
    }
    Ok(())
}

/// Writes only the login script (used when a reconnect inside the running client needs it again).
pub(crate) fn write_login_script(dir: &Path, content: &str) -> Result<()> {
    let path = dir.join(LOGIN_SCRIPT_FILE_NAME);
    write_atomic(&path, content.as_bytes(), true).map_err(|e| AfkError::io(&path, e))
}

/// True when the login script has something to run (`HasLoginCommands`).
pub fn has_login_commands(options: &LoginCommandsOptions, commands: &[String]) -> bool {
    options.enabled && commands.iter().any(|c| !c.trim().is_empty())
}

struct Out(String);

impl Out {
    fn new() -> Self {
        Self(String::with_capacity(4096))
    }

    fn line(&mut self, text: &str) {
        self.0.push_str(text);
        self.0.push_str(NEWLINE);
    }

    fn blank(&mut self) {
        self.0.push_str(NEWLINE);
    }
}

fn build_ini(
    account: &AfkAccount,
    login_commands: &[String],
    launcher_nickname: &str,
    versions: &VersionCatalog,
) -> String {
    // AutoEat reads the inventory and AntiAFK's walking needs terrain handling: forced on here
    // instead of asking the user.
    let inventory_handling = account.auto_eat.enabled;
    let terrain_and_movements = account.anti_afk.enabled && account.anti_afk.use_terrain_handling;

    let owner = launcher_nickname.trim();
    let owners: Vec<&str> = if owner.is_empty() {
        Vec::new()
    } else {
        vec![owner]
    };

    let mut out = Out::new();
    out.line("# Projects Launcher tarafından üretildi.");
    out.line("# Bu dosya her bağlanışta yeniden yazılır; elle yapılan değişiklikler kaybolur.");
    out.line("# Ayarları başlatıcıdaki AFK Hesapları ekranından düzenleyin.");
    out.blank();

    out.line("[Main]");
    out.blank();
    out.line("[Main.General]");
    // Password "-" is MCC's offline (non-premium) session: no authentication request is made.
    out.line(&format!(
        "Account = {{ Login = {}, Password = \"-\" }}",
        toml_str(&account.nickname)
    ));
    out.line(&format!("Server = {}", server_table(account)));
    out.blank();

    out.line("[Main.Advanced]");
    out.line("EnableSentry = false");
    out.line("Language = \"tr_tr\"");
    out.line("ConsoleTitle = \"\"");
    out.line("PlayerHeadAsIcon = false");
    out.line("ShowGithubStarReminder = false");
    out.line("ExitOnFailure = true");
    out.line(&format!(
        "MinecraftVersion = {}",
        toml_str(&versions.resolve(&account.minecraft_version).value)
    ));
    out.line(&format!(
        "LoadResourcePackTranslations = {}",
        toml_bool(account.allow_resource_packs)
    ));
    out.line("AutoRespawn = true");
    out.line(&format!(
        "TerrainAndMovements = {}",
        toml_bool(terrain_and_movements)
    ));
    out.line(&format!(
        "InventoryHandling = {}",
        toml_bool(inventory_handling)
    ));
    out.line("EntityHandling = false");
    out.line("SessionCache = \"none\"");
    out.line("ProfileKeyCache = \"none\"");
    out.line(&format!("BotOwners = {}", toml_arr(&owners)));
    out.blank();

    out.line("[Console]");
    out.blank();
    out.line("[Console.General]");
    out.line("ConsoleMode = \"classic\"");
    out.line("Display_Icon_Banner = false");
    out.blank();

    out.line("[ChatBot]");
    out.blank();

    append_alerts(&mut out, &account.alerts);
    append_anti_afk(&mut out, &account.anti_afk);
    append_auto_eat(&mut out, &account.auto_eat);
    append_auto_relog(&mut out, &account.auto_relog);
    append_auto_respond(&mut out, &account.auto_respond);
    append_chat_log(&mut out, &account.chat_log);
    // Arrays of tables must follow their parent's plain keys, so the scheduler goes last.
    append_scheduler(&mut out, account, login_commands);

    out.0
}

fn server_table(account: &AfkAccount) -> String {
    let host = toml_str(account.effective_host());
    // Without a port MCC resolves the SRV record first, then falls back to 25565.
    if account.server_port <= 0 || account.server_port > 65535 {
        format!("{{ Host = {host} }}")
    } else {
        format!("{{ Host = {host}, Port = {} }}", account.server_port)
    }
}

fn append_alerts(out: &mut Out, o: &AlertsOptions) {
    out.line("[ChatBot.Alerts]");
    out.line(&format!("Enabled = {}", toml_bool(o.enabled)));
    out.line(&format!("Beep_Enabled = {}", toml_bool(o.beep_enabled)));
    out.line(&format!(
        "Trigger_By_Words = {}",
        toml_bool(o.trigger_by_words)
    ));
    out.line("Trigger_By_Rain = false");
    out.line("Trigger_By_Thunderstorm = false");
    out.line(&format!("Log_To_File = {}", toml_bool(o.log_to_file)));
    out.line("Log_File = \"uyarilar.txt\"");
    out.line(&format!("Matches = {}", toml_arr(&o.matches)));
    out.line(&format!("Excludes = {}", toml_arr(&o.excludes)));
    out.blank();
}

fn append_anti_afk(out: &mut Out, o: &AntiAfkOptions) {
    out.line("[ChatBot.AntiAFK]");
    out.line(&format!("Enabled = {}", toml_bool(o.enabled)));
    out.line(&format!("Delay = {}", toml_range(o.delay_min, o.delay_max)));
    out.line(&format!("Command = {}", toml_str(&o.command)));
    out.line(&format!("Use_Sneak = {}", toml_bool(o.use_sneak)));
    out.line(&format!(
        "Use_Terrain_Handling = {}",
        toml_bool(o.use_terrain_handling)
    ));
    out.line(&format!("Walk_Range = {}", o.walk_range));
    out.line(&format!("Walk_Retries = {}", o.walk_retries));
    out.blank();
}

fn append_auto_eat(out: &mut Out, o: &AutoEatOptions) {
    out.line("[ChatBot.AutoEat]");
    out.line(&format!("Enabled = {}", toml_bool(o.enabled)));
    out.line(&format!("Threshold = {}", o.threshold));
    out.blank();
}

fn append_auto_relog(out: &mut Out, o: &AutoRelogOptions) {
    out.line("[ChatBot.AutoRelog]");
    out.line(&format!("Enabled = {}", toml_bool(o.enabled)));
    out.line(&format!("Delay = {}", toml_range(o.delay_min, o.delay_max)));
    out.line(&format!("Retries = {}", o.retries));
    out.line(&format!(
        "Ignore_Kick_Message = {}",
        toml_bool(o.ignore_kick_message)
    ));
    out.line(&format!("Kick_Messages = {}", toml_arr(&o.kick_messages)));
    out.blank();
}

fn append_auto_respond(out: &mut Out, o: &AutoRespondOptions) {
    out.line("[ChatBot.AutoRespond]");
    // With no rules the bot would keep re-reading an empty file; the count includes rules that
    // matches.ini later skips, exactly like the legacy writer.
    out.line(&format!(
        "Enabled = {}",
        toml_bool(o.enabled && !o.matches.is_empty())
    ));
    out.line("Matches_File = \"matches.ini\"");
    out.line(&format!("Match_Colors = {}", toml_bool(o.match_colors)));
    out.blank();
}

fn append_chat_log(out: &mut Out, o: &ChatLogOptions) {
    out.line("[ChatBot.ChatLog]");
    out.line(&format!("Enabled = {}", toml_bool(o.enabled)));
    out.line(&format!("Add_DateTime = {}", toml_bool(o.add_date_time)));
    out.line("Log_File = \"sohbet.txt\"");
    out.line(&format!("Filter = {}", toml_str(&o.filter)));
    out.blank();
}

fn append_scheduler(out: &mut Out, account: &AfkAccount, login_commands: &[String]) {
    let scheduler = &account.scheduler;
    let mut tasks: Vec<ScheduledTask> = if scheduler.enabled {
        scheduler
            .tasks
            .iter()
            .filter(|t| !t.action.trim().is_empty())
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    // The login commands run as one script task triggered on login; it enables the scheduler even
    // when the user's own task list is off.
    if has_login_commands(&account.login_commands, login_commands) {
        let only_first = account.login_commands.only_first_login;
        tasks.push(ScheduledTask {
            name: LOGIN_TASK_NAME.to_owned(),
            action: format!("script {LOGIN_SCRIPT_FILE_NAME}"),
            on_first_login: only_first,
            on_login: !only_first,
            interval_enabled: false,
            ..ScheduledTask::default()
        });
    }

    out.line("[ChatBot.ScriptScheduler]");
    out.line(&format!("Enabled = {}", toml_bool(!tasks.is_empty())));

    if tasks.is_empty() {
        // Left out, MCC would load its sample tasks ("send /hello"): empty it explicitly.
        out.line("TaskList = [ ]");
        out.blank();
        return;
    }

    out.blank();
    for task in &tasks {
        let name = if task.name.trim().is_empty() {
            DEFAULT_TASK_NAME
        } else {
            task.name.as_str()
        };
        out.line("[[ChatBot.ScriptScheduler.TaskList]]");
        out.line(&format!("Task_Name = {}", toml_str(name)));
        out.line(&format!(
            "Trigger_On_First_Login = {}",
            toml_bool(task.on_first_login)
        ));
        out.line(&format!("Trigger_On_Login = {}", toml_bool(task.on_login)));
        out.line("Trigger_On_Times = { Enable = false, Times = [ ] }");
        out.line(&format!(
            "Trigger_On_Interval = {{ Enable = {}, MinTime = {}, MaxTime = {} }}",
            toml_bool(task.interval_enabled),
            dotnet_number(task.min_seconds),
            dotnet_number(task.max_seconds)
        ));
        out.line(&format!("Action = {}", toml_str(&task.action)));
        out.blank();
    }
}

/// The login script: every line is an MCC internal command; `send` types into the chat and
/// `wait N` waits N ticks. Servers swallow chat sent right after joining, so the delay also
/// applies before the first command.
fn build_login_script(options: &LoginCommandsOptions, commands: &[String]) -> String {
    let mut out = Out::new();
    out.line("# Projects Launcher tarafından üretildi. Elle yapılan değişiklikler kaybolur.");

    if !has_login_commands(options, commands) {
        return out.0;
    }

    let commands: Vec<&str> = commands
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .collect();
    let wait_ticks = script_wait_ticks(options.delay_seconds);

    if wait_ticks > 0 {
        out.line(&format!("wait {wait_ticks}"));
    }
    for (i, command) in commands.iter().enumerate() {
        out.line(&format!("send {command}"));
        if wait_ticks > 0 && i + 1 < commands.len() {
            out.line(&format!("wait {wait_ticks}"));
        }
    }
    out.0
}

/// `(int)Math.Round(Math.Max(0, seconds) * 20)`: banker's rounding; a result outside `int` turned
/// into `int.MinValue` on .NET Framework, which means "no waits".
fn script_wait_ticks(seconds: f64) -> i32 {
    let ticks = (seconds.max(0.0) * SCRIPT_TICKS_PER_SECOND).round_ties_even();
    if ticks.is_finite() && ticks <= f64::from(i32::MAX) {
        ticks as i32
    } else {
        i32::MIN
    }
}

/// `matches.ini` for AutoRespond (MCC's INI format, not TOML): one `[Match]` block per rule with a
/// pattern and at least one action. A pattern the client could not use is left out (see
/// [`crate::pattern`]), so a rule saved before the check existed never breaks the client.
fn build_matches_ini(options: &AutoRespondOptions) -> String {
    let mut out = Out::new();
    out.line("# Projects Launcher tarafından üretildi. Elle yapılan değişiklikler kaybolur.");
    out.blank();

    for (index, rule) in options.matches.iter().enumerate() {
        if rule.pattern.trim().is_empty() {
            continue;
        }
        let has_action = [&rule.action, &rule.private_action, &rule.other_action]
            .iter()
            .any(|a| !a.trim().is_empty());
        if !has_action {
            continue;
        }
        if let Some(problem) = check_pattern(&rule.pattern, rule.is_regex) {
            tracing::warn!(
                rule = index,
                ?problem,
                "auto-respond rule left out: unusable pattern"
            );
            continue;
        }

        out.line("[Match]");
        out.line(&matches_line(&rule.pattern, rule.is_regex));
        for (key, value) in [
            ("action=", &rule.action),
            ("actionprivate=", &rule.private_action),
            ("actionother=", &rule.other_action),
        ] {
            if !value.trim().is_empty() {
                out.line(&format!("{key}{}", value.trim()));
            }
        }
        out.line(&format!(
            "ownersonly={}",
            if rule.owners_only { "true" } else { "false" }
        ));
        out.line(&format!("cooldown={}", rule.cooldown_seconds.max(0)));
        out.blank();
    }
    out.0
}

// --- TOML primitives ------------------------------------------------------------------------------

fn toml_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn toml_range(min: f64, max: f64) -> String {
    format!(
        "{{ min = {}, max = {} }}",
        dotnet_number(min),
        dotnet_number(max)
    )
}

/// TOML basic string with the legacy escaping: `\\ \" \n \r \t`, other control characters below
/// U+0020 as `\uXXXX` (uppercase hex); everything else verbatim.
fn toml_str(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// TOML array of the non-blank items, trimmed: `[ ]` when empty, `[ "a", "b" ]` otherwise.
fn toml_arr<S: AsRef<str>>(items: &[S]) -> String {
    let values: Vec<String> = items
        .iter()
        .map(|item| item.as_ref().trim())
        .filter(|item| !item.is_empty())
        .map(toml_str)
        .collect();
    if values.is_empty() {
        "[ ]".to_owned()
    } else {
        format!("[ {} ]", values.join(", "))
    }
}

/// `double.ToString("0.0##", CultureInfo.InvariantCulture)` as .NET Framework 4.8 produces it.
///
/// .NET sees a double through 15 significant digits for custom formats, then rounds that decimal
/// string half away from zero to three decimals, keeping at least one. MCC requires a decimal
/// point for these TOML floats. Values that round to zero print without a sign (`-0.0001` → `0.0`;
/// .NET Core 3.0+ would print `-0.0`, but the legacy launcher ran on .NET Framework).
pub(crate) fn dotnet_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }

    // 15 significant digits: "d.dddddddddddddde<exp>".
    let scientific = format!("{:.14e}", value.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .unwrap_or((scientific.as_str(), "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();

    // Place the digits around the decimal point: `point` digits form the integer part.
    let point = exponent + 1;
    let mut integer: Vec<u8> = Vec::new();
    let mut fraction: Vec<u8> = Vec::new();
    if point > 0 {
        let point = point as usize;
        for i in 0..point.max(digits.len()) {
            let digit = digits.get(i).copied().unwrap_or(0);
            if i < point {
                integer.push(digit);
            } else {
                fraction.push(digit);
            }
        }
    } else {
        integer.push(0);
        fraction.extend(std::iter::repeat_n(0, (-point) as usize));
        fraction.extend_from_slice(&digits);
    }

    // Round half away from zero at the third decimal.
    let round_up = fraction.get(3).is_some_and(|&d| d >= 5);
    fraction.resize(3, 0);
    if round_up {
        let mut carry = true;
        for digit in fraction.iter_mut().rev().chain(integer.iter_mut().rev()) {
            if !carry {
                break;
            }
            if *digit == 9 {
                *digit = 0;
            } else {
                *digit += 1;
                carry = false;
            }
        }
        if carry {
            integer.insert(0, 1);
        }
    }

    while fraction.len() > 1 && fraction.last() == Some(&0) {
        fraction.pop();
    }
    let first_nonzero = integer
        .iter()
        .position(|&d| d != 0)
        .unwrap_or(integer.len() - 1);
    let integer = &integer[first_nonzero..];

    let is_zero = integer.iter().chain(fraction.iter()).all(|&d| d == 0);
    let mut out = String::new();
    if value < 0.0 && !is_zero {
        out.push('-');
    }
    out.extend(integer.iter().map(|d| char::from(b'0' + d)));
    out.push('.');
    out.extend(fraction.iter().map(|d| char::from(b'0' + d)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Outputs captured from `double.ToString("0.0##", InvariantCulture)` on .NET Framework 4.8
    /// (the legacy launcher's runtime) and .NET 10; they agree except for negative zero.
    #[test]
    fn number_format_matches_dotnet_framework() {
        let cases: &[(f64, &str)] = &[
            (60.0, "60.0"),
            (90.0, "90.0"),
            (45.5, "45.5"),
            (120.25, "120.25"),
            (3.0, "3.0"),
            (3600.0, "3600.0"),
            (0.1, "0.1"),
            (2.5, "2.5"),
            (7.0005, "7.001"),
            (1234567.891, "1234567.891"),
            (99.9995, "100.0"),
            (123.4564999, "123.456"),
            (-2.5, "-2.5"),
            (0.0005, "0.001"),
            (1e-5, "0.0"),
            (2.0005, "2.001"),
            (-0.0001, "0.0"),
            (-0.0, "0.0"),
            (0.0, "0.0"),
            (1e20, "100000000000000000000.0"),
            (0.3333333, "0.333"),
            (1.0 / 3.0, "0.333"),
            (0.0015, "0.002"),
            (0.0025, "0.003"),
            (1.0005, "1.001"),
            (2.675, "2.675"),
            (1e15 + 0.3, "1000000000000000.0"),
            (123456789012.3456, "123456789012.346"),
            (0.00049999, "0.0"),
            (4.4445, "4.445"),
            (-7.0005, "-7.001"),
            (61.5, "61.5"),
            (9.9999, "10.0"),
        ];
        for &(value, expected) in cases {
            assert_eq!(dotnet_number(value), expected, "formatting {value:?}");
        }
        assert_eq!(dotnet_number(f64::NAN), "NaN");
        assert_eq!(dotnet_number(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn wait_ticks_use_bankers_rounding_and_overflow_like_dotnet_framework() {
        assert_eq!(script_wait_ticks(0.125), 2);
        assert_eq!(script_wait_ticks(0.025), 0);
        assert_eq!(script_wait_ticks(1.5), 30);
        assert_eq!(script_wait_ticks(0.075), 2);
        assert_eq!(script_wait_ticks(0.375), 8);
        assert_eq!(script_wait_ticks(-4.0), 0);
        assert_eq!(script_wait_ticks(1e10), i32::MIN);
        assert_eq!(script_wait_ticks(f64::NAN), 0);
    }

    #[test]
    fn toml_string_escaping() {
        assert_eq!(toml_str("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(
            toml_str("\u{1}\u{8}\u{c}\t\n\r"),
            r#""\u0001\u0008\u000C\t\n\r""#
        );
        assert_eq!(toml_str("del\u{7f} 🎮 ğ"), "\"del\u{7f} 🎮 ğ\"");
        assert_eq!(toml_arr(&["  a ", " ", ""]), r#"[ "a" ]"#);
        assert_eq!(toml_arr::<&str>(&[]), "[ ]");
    }

    #[test]
    fn unusable_patterns_never_reach_the_client() {
        let rule = |pattern: &str, is_regex: bool| crate::model::AutoRespondMatch {
            pattern: pattern.into(),
            is_regex,
            action: "send merhaba".into(),
            ..crate::model::AutoRespondMatch::default()
        };
        let options = AutoRespondOptions {
            enabled: true,
            matches: vec![
                rule("[bozuk", true),
                rule("(a+)+$", true),
                rule("#genel selam", false),
                rule(r"^\[#(\w+)\]", true),
                rule("selam", false),
            ],
            ..AutoRespondOptions::default()
        };
        let ini = build_matches_ini(&options);
        let patterns: Vec<&str> = ini
            .lines()
            .filter(|l| l.starts_with("regex=") || l.starts_with("match="))
            .collect();
        assert_eq!(
            patterns,
            vec![
                r"regex=(?i)\x23genel selam",
                r"regex=^\[\x23(\w+)\]",
                "match=selam"
            ]
        );
        assert_eq!(ini.matches("[Match]").count(), 3);
    }

    #[test]
    fn stored_versions_resolve_against_the_list() {
        let versions = VersionCatalog::built_in();
        let mut account = AfkAccount::new("Test");
        let ini = |account: &AfkAccount| build_ini(account, &[], "", &versions);
        assert!(
            ini(&account).contains("MinecraftVersion = \"26.3\"\r\n"),
            "follows"
        );
        account.minecraft_version = " AUTO ".into();
        assert!(ini(&account).contains("MinecraftVersion = \"auto\"\r\n"));
        account.minecraft_version = "1.9.2".into();
        assert!(ini(&account).contains("MinecraftVersion = \"1.9.2\"\r\n"));
        account.minecraft_version = "1.2.3".into();
        assert!(ini(&account).contains("MinecraftVersion = \"26.3\"\r\n"));
        let index = VersionCatalog::from_release([("26.4", "26.4")], None).unwrap();
        account.minecraft_version = String::new();
        assert!(
            build_ini(&account, &[], "", &index).contains("MinecraftVersion = \"26.4\"\r\n"),
            "follows the published recommendation"
        );
    }

    #[test]
    fn login_script_without_enabled_commands_is_header_only() {
        let options = LoginCommandsOptions {
            enabled: false,
            ..LoginCommandsOptions::default()
        };
        let script = build_login_script(&options, &["/login x".to_owned()]);
        assert_eq!(
            script,
            "# Projects Launcher tarafından üretildi. Elle yapılan değişiklikler kaybolur.\r\n"
        );
        assert!(!has_login_commands(
            &LoginCommandsOptions {
                enabled: true,
                ..options
            },
            &["  ".to_owned()]
        ));
    }
}
