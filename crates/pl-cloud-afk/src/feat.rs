//! The cloud features of phase 1 (the contract's "Features": "Feature switches", "Inventory",
//! "Console kinds", "Rule placeholders", "Bulk management"): what Studio switched on and what an
//! account may use, the live inventory's frames and moves, the console's status strip and its
//! download, and the placeholders of auto-replies (the save checks, the value class and "Dene").
//!
//! Every feature is asleep until Studio switches it on and the account's machine can do it
//! (`Account.features`): the launcher shows nothing of a feature it may not use.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{CloudConsoleLine, CloudLocalized};
use crate::movement::CloudMovementFeature;
use crate::text::{MAX_TEXT_UNITS, without_leading_ignorable};

/// Studio's default allow list of commands a reply with placeholders may start with
/// (`feat/settings-v1.json`).
pub const DEFAULT_PLACEHOLDER_COMMANDS: &[&str] =
    &["msg", "m", "tell", "w", "whisper", "r", "reply", "pm"];

/// The longest value the client's strict class lets through, in UTF-16 code units.
pub const PLACEHOLDER_VALUE_MAX: usize = 24;

// --- Feature switches ---------------------------------------------------------------------------

/// `Offer.features` as the service sends it (absent from a service before this phase).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OfferFeatures {
    pub inventory: String,
    pub console_kinds: bool,
    pub placeholders: bool,
    pub placeholder_commands: Vec<String>,
}

impl Default for OfferFeatures {
    fn default() -> Self {
        Self {
            inventory: "off".into(),
            console_kinds: false,
            placeholders: false,
            placeholder_commands: DEFAULT_PLACEHOLDER_COMMANDS
                .iter()
                .map(|c| (*c).to_owned())
                .collect(),
        }
    }
}

/// `off`, `view` or `full`; anything else reads as `off`.
fn inventory_mode(mode: &str) -> &'static str {
    match mode {
        "view" => "view",
        "full" => "full",
        _ => "off",
    }
}

/// Studio's feature switches as the AFK page reads them (`CloudOverview.features`; None: a service
/// before this phase, which shows none of them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudFeatures {
    /// `off`, `view` (the inventory shows) or `full` (and takes moves).
    pub inventory: String,
    pub console_kinds: bool,
    /// The save checks of placeholders run (the account's own `features.placeholders` says whether
    /// its replies may use them).
    pub placeholders: bool,
    /// The commands a reply with placeholders may start with (without `/`, lower case).
    pub placeholder_commands: Vec<String>,
}

impl From<&OfferFeatures> for CloudFeatures {
    fn from(offer: &OfferFeatures) -> Self {
        Self {
            inventory: inventory_mode(&offer.inventory).to_owned(),
            console_kinds: offer.console_kinds,
            placeholders: offer.placeholders,
            placeholder_commands: offer
                .placeholder_commands
                .iter()
                .map(|c| c.to_ascii_lowercase())
                .collect(),
        }
    }
}

/// `Account.features.inventory`: Studio's switch while the account's machine can show an
/// inventory (else `off`), and the account's own setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudInventoryFeature {
    /// `off`, `view` or `full`.
    pub mode: String,
    /// The account's setting (`config.inventory.enabled`, else Studio's default).
    pub on: bool,
}

impl Default for CloudInventoryFeature {
    fn default() -> Self {
        Self {
            mode: "off".into(),
            on: false,
        }
    }
}

/// `Account.features`: what this account may use now. An account without a machine has
/// everything off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudAccountFeatures {
    pub inventory: CloudInventoryFeature,
    pub console_kinds: bool,
    /// Console lines in the game's colours, with their hovers and clicks (CONSOLE-COLOR): Studio's
    /// switch while the account's machine gives them. Off from a service before it.
    pub console_looks: bool,
    /// A server's dialog as a card the player may press (DIALOG-CARD): Studio's switch while the
    /// account's machine reports dialogs. Off from a service before it.
    pub dialog_cards: bool,
    pub placeholders: bool,
    /// Console kinds are on, but this account's player, private or server rules still match the
    /// old way until its settings are saved once.
    pub legacy_rules: bool,
    /// Movement (README "Movement"); None from a service before it: nothing of it shows and the
    /// account's `config.movement` is never sent.
    pub movement: Option<CloudMovementFeature>,
}

/// The account's own inventory choice (`config.inventory`; absent: never chosen).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudInventorySetting {
    pub enabled: bool,
}

// --- Inventory ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudInvDurability {
    pub left: u32,
    pub max: u32,
}

/// An active enchantment (or an enchanted book's), `label` as the game writes it ("Keskinlik V").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudInvEnchant {
    pub id: String,
    pub lvl: u32,
    pub label: CloudLocalized,
}

/// One of the first nine stacks inside a shulker box or bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudInvInsideStack {
    pub id: String,
    pub count: u32,
    pub label: CloudLocalized,
    pub cmd: Option<i64>,
    pub model: Option<String>,
}

/// The most stacks `inside.stacks` may say (afk-host's cap).
pub const MAX_INSIDE_STACKS: u32 = 256;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudInvInside {
    /// The items' total.
    pub count: u32,
    pub top: Vec<CloudInvInsideStack>,
    /// How many stacks it holds, as the game counts them for "and N more...", from machines that
    /// count them (CAFK-SVC); absent from older ones. Anything but a whole number up to 256 reads
    /// as absent and never costs the item; the view also wants at least the stacks listed.
    #[serde(
        default,
        deserialize_with = "stack_count",
        skip_serializing_if = "Option::is_none"
    )]
    #[specta(type = Option<u32>)]
    pub stacks: Option<u32>,
}

/// `inside.stacks` as read: a whole number from 0 to [`MAX_INSIDE_STACKS`], else absent.
fn stack_count<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(safe_int(&value)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n <= MAX_INSIDE_STACKS))
}

/// `InvItem`: one stack in the player inventory (`slot` 5–8 armour, 9–35 main, 36–44 hotbar, 45
/// off hand) or in an open window (`0 … size-1`); the cursor's item has no slot (0). `name`,
/// `lore` and every label are anyone's plain text: never markup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudInvItem {
    pub slot: u32,
    /// `minecraft:diamond_sword`.
    pub id: String,
    pub count: u32,
    pub label: CloudLocalized,
    /// A custom name (anvil) or a server's item name.
    pub name: Option<String>,
    /// The name's look in the game (machine 0.3.2): spans over `name`, each its length in UTF-16
    /// code units and its code (a colour `0`–`f` or `#rrggbb`, then the flags among `klmno`).
    #[serde(rename = "nameStyle")]
    pub name_style: Vec<(u32, String)>,
    /// At most 32 lines of at most 80 characters.
    pub lore: Vec<String>,
    /// A look a `lore` line, or none (machine 0.3.2).
    #[serde(rename = "loreStyle")]
    pub lore_style: Vec<Option<Vec<(u32, String)>>>,
    pub durability: Option<CloudInvDurability>,
    /// `custom_model_data` (its first float, rounded down).
    pub cmd: Option<i64>,
    /// The `item_model` component, when a server set one.
    pub model: Option<String>,
    /// A player head's skin: its texture's hash on Mojang's texture server (machine 0.3.2).
    pub skin: Option<String>,
    pub ench: Vec<CloudInvEnchant>,
    pub enchanted: bool,
    pub inside: Option<CloudInvInside>,
    /// A potion's type, `minecraft:strong_healing`.
    pub potion: Option<String>,
    /// A dye or a potion's own colour, `0xRRGGBB`.
    pub color: Option<u32>,
    /// Something could not be shown.
    pub partial: bool,
}

/// `win.open`: the game's menu name (`generic_9x3`, `hopper`, …, `unknown`), its plain title (at
/// most 80 characters; may keep a resource pack's private-use glyphs) and its own slots.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudWinOpen {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub size: u32,
}

/// Why a move was refused (`invres.error`): `invalid` (`reason`, `field`), `inventory_off`
/// (`reason`), `not_running`, `busy_trade`, `stale`, `rate_limited` (`retryAfter` seconds),
/// `not_found`, `host_offline`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudInvError {
    pub code: String,
    pub reason: Option<String>,
    pub field: Option<String>,
    pub retry_after: Option<u32>,
}

impl CloudInvError {
    pub fn new(code: &str) -> Self {
        Self {
            code: code.to_owned(),
            ..Self::default()
        }
    }

    fn invalid(reason: &str, field: Option<&str>) -> Self {
        Self {
            code: "invalid".into(),
            reason: Some(reason.to_owned()),
            field: field.map(str::to_owned),
            retry_after: None,
        }
    }
}

const INV_ACTIONS: &[&str] = &["click", "drop", "use", "hold", "close", "snapshot"];

fn inv_fields(action: &str) -> (&'static [&'static str], &'static [&'static str]) {
    // (allowed besides `action`, required)
    match action {
        "click" => (
            &["area", "slot", "button", "mode", "wseq"],
            &["area", "slot", "button", "mode", "wseq"],
        ),
        "drop" => (
            &["area", "slot", "stack", "wseq"],
            &["area", "slot", "stack", "wseq"],
        ),
        "use" => (&["slot"], &[]),
        "hold" => (&["slot"], &["slot"]),
        "close" => (&["wseq"], &["wseq"]),
        _ => (&[], &[]),
    }
}

/// A JSON number that is a whole number JavaScript holds exactly.
pub(crate) fn safe_int(value: &Value) -> Option<i64> {
    const MAX_SAFE: i64 = (1 << 53) - 1;
    let n = value.as_i64().or_else(|| {
        let f = value.as_f64()?;
        (f.fract() == 0.0 && f.abs() <= MAX_SAFE as f64).then_some(f as i64)
    })?;
    (-MAX_SAFE..=MAX_SAFE).contains(&n).then_some(n)
}

/// The shape of a move as the app wrote it (`{"t":"invop"}`'s `op`; the contract's `InvOp`,
/// vectors `inv-op-v1.json` → `shape`), checked before it leaves the launcher. The machine checks
/// it again and has the last word.
pub fn check_inv_op(op: &Value) -> Result<(), CloudInvError> {
    let Some(object) = op.as_object() else {
        return Err(CloudInvError::invalid("type", None));
    };
    let Some(action) = object
        .get("action")
        .and_then(Value::as_str)
        .filter(|a| INV_ACTIONS.contains(a))
    else {
        return Err(CloudInvError::invalid("action", None));
    };
    let (allowed, required) = inv_fields(action);
    if let Some(extra) = object
        .keys()
        .find(|key| *key != "action" && !allowed.contains(&key.as_str()))
    {
        return Err(CloudInvError::invalid("field", Some(extra)));
    }
    if let Some(missing) = required.iter().find(|key| !object.contains_key(**key)) {
        return Err(CloudInvError::invalid(missing, None));
    }
    let area = object.get("area");
    if let Some(area) = area
        && !matches!(area.as_str(), Some("inv" | "win"))
    {
        return Err(CloudInvError::invalid("area", None));
    }
    if let Some(slot) = object.get("slot") {
        let slot = safe_int(slot);
        let ok = match (action, area.and_then(Value::as_str)) {
            ("use" | "hold", _) => slot.is_some_and(|s| (0..=8).contains(&s)),
            (_, Some("inv")) => slot.is_some_and(|s| (5..=45).contains(&s)),
            _ => slot.is_some_and(|s| (0..54).contains(&s)),
        };
        if !ok {
            return Err(CloudInvError::invalid("slot", None));
        }
    }
    if let Some(button) = object.get("button")
        && !matches!(safe_int(button), Some(0 | 1))
    {
        return Err(CloudInvError::invalid("button", None));
    }
    if let Some(mode) = object.get("mode")
        && !matches!(mode.as_str(), Some("pickup" | "quick_move"))
    {
        return Err(CloudInvError::invalid("mode", None));
    }
    if let Some(stack) = object.get("stack")
        && !stack.is_boolean()
    {
        return Err(CloudInvError::invalid("stack", None));
    }
    let wseq = object.get("wseq").map(safe_int);
    if let Some(wseq) = wseq
        && !wseq.is_some_and(|w| w >= 0)
    {
        return Err(CloudInvError::invalid("wseq", None));
    }
    if (area.and_then(Value::as_str) == Some("win") || action == "close")
        && wseq.flatten() == Some(0)
    {
        return Err(CloudInvError::invalid("wseq", None));
    }
    Ok(())
}

/// A move's id: 1–32 of `[A-Za-z0-9_-]`, unique among the socket's pending moves.
pub fn is_inv_op_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

// --- The status strip ---------------------------------------------------------------------------

/// The action bar's text, shown until `untilMs` (Unix milliseconds) unless a newer strip came.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudBarText {
    pub text: String,
    pub until_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudTitleBar {
    pub text: String,
    pub subtitle: Option<String>,
    pub until_ms: i64,
}

/// A boss bar: `id` the machine's own (`b1`), `progress` 0–1, `color` the game's name of it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudBossBar {
    pub id: String,
    pub text: String,
    pub progress: f64,
    pub color: String,
}

/// `bars`: what the server keeps showing (the action bar, a title, at most four boss bars), kept
/// out of the log and shown above the console.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudBars {
    pub actionbar: Option<CloudBarText>,
    pub title: Option<CloudTitleBar>,
    pub boss: Vec<CloudBossBar>,
}

// --- Console download ---------------------------------------------------------------------------

/// "İndir": which lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloudLogRange {
    /// "Son 1 saat".
    Hour,
    /// "Bugün": since midnight in Turkey.
    Today,
    /// "Son 3 gün" (the log keeps three days).
    Days3,
}

/// Turkey's clock: UTC+3 all year (no summer time since 2016).
const TURKEY_OFFSET: i64 = 3 * 3600;
const DAY: i64 = 86_400;

impl CloudLogRange {
    /// The range's first second (Unix seconds) for `now`.
    pub fn from(self, now: i64) -> i64 {
        match self {
            Self::Hour => now - 3600,
            Self::Today => {
                let local = now + TURKEY_OFFSET;
                local - local.rem_euclid(DAY) - TURKEY_OFFSET
            }
            Self::Days3 => now - 3 * DAY,
        }
    }
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// A Unix time on Turkey's clock: the day and the time of day.
fn turkey_clock(at: i64) -> ((i64, u32, u32), (i64, i64, i64)) {
    let local = at + TURKEY_OFFSET;
    let seconds = local.rem_euclid(DAY);
    (
        civil_from_days(local.div_euclid(DAY)),
        (seconds / 3600, seconds % 3600 / 60, seconds % 60),
    )
}

/// The kind a console line shows under: a command row's line (`k: "s"`) whatever its `c`, else
/// its `c` (an unknown one as the client's own), else `console` (a line from before kinds).
pub fn line_kind(line: &CloudConsoleLine) -> &'static str {
    if line.k == "s" {
        return "command";
    }
    match line.c.as_deref() {
        Some("chat") => "chat",
        Some("pm") => "pm",
        Some("sys") => "sys",
        Some("title") => "title",
        Some("err") => "err",
        Some(_) => "cli",
        None => "console",
    }
}

/// The kind's name in a downloaded file.
fn kind_name(kind: &str, english: bool) -> &'static str {
    match (kind, english) {
        ("command", false) => "Komut",
        ("command", true) => "Command",
        ("chat", false) => "Sohbet",
        ("chat", true) => "Chat",
        ("pm", false) => "Özel",
        ("pm", true) => "Private",
        ("sys", false) => "Sunucu",
        ("sys", true) => "Server",
        ("title", false) => "Başlık",
        ("title", true) => "Title",
        ("cli", false) => "Bağlantı",
        ("cli", true) => "Connection",
        ("err", false) => "Hata",
        ("err", true) => "Error",
        (_, false) => "Konsol",
        (_, true) => "Console",
    }
}

/// The text of a console download (the contract's "Logs and downloads"): UTF-8 with a byte order
/// mark and CRLF line ends, `[HH:MM:SS] [<kind>] <text>` in Turkey time, `--- DD.MM.YYYY ---`
/// before each new day; `lang` `en` for English kind names. The lines are the masked log's.
pub fn console_text(lines: &[CloudConsoleLine], lang: &str) -> String {
    let english = lang == "en";
    let mut out = String::from("\u{feff}");
    let mut day = None;
    for line in lines {
        let ((year, month, date), (hour, minute, second)) = turkey_clock(line.at);
        if day != Some((year, month, date)) {
            day = Some((year, month, date));
            out.push_str(&format!("--- {date:02}.{month:02}.{year:04} ---\r\n"));
        }
        let text: String = line
            .text
            .chars()
            .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
            .collect();
        out.push_str(&format!(
            "[{hour:02}:{minute:02}:{second:02}] [{}] {text}\r\n",
            kind_name(line_kind(line), english)
        ));
    }
    out
}

/// `<name>-konsol-<YYYY-MM-DD>.txt` (`console` in English), the day in Turkey. Only the name's
/// letters, digits and `_` are kept (an in-game name has nothing else).
pub fn console_file_name(name: &str, lang: &str, now: i64) -> String {
    let ((year, month, date), _) = turkey_clock(now);
    let safe: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(16)
        .collect();
    let safe = if safe.is_empty() {
        "hesap".into()
    } else {
        safe
    };
    let word = if lang == "en" { "console" } else { "konsol" };
    format!("{safe}-{word}-{year:04}-{month:02}-{date:02}.txt")
}

/// Whether `name` is a file [`console_file_name`] makes (with a ` (2)` … the launcher adds when the
/// name is taken): only such a file is shown in its folder.
pub fn is_console_file_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".txt") else {
        return false;
    };
    let stem = match stem.rsplit_once(" (") {
        Some((head, tail))
            if tail.len() > 1
                && tail.ends_with(')')
                && tail[..tail.len() - 1].bytes().all(|b| b.is_ascii_digit()) =>
        {
            head
        }
        _ => stem,
    };
    let mut parts = stem.rsplitn(5, '-');
    let (Some(day), Some(month), Some(year), Some(word), Some(who)) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return false;
    };
    let digits =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    digits(year, 4)
        && digits(month, 2)
        && digits(day, 2)
        && matches!(word, "konsol" | "console")
        && !who.is_empty()
        && who.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

// --- Rule placeholders --------------------------------------------------------------------------

/// The placeholder names of a text, left to right, repeats included (`u`, `1`…`9`): exactly
/// `{u}` and `{1}`…`{9}`; anything else with braces is plain text.
pub fn placeholder_names(text: &str) -> Vec<char> {
    let bytes = text.as_bytes();
    let mut names = Vec::new();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'{' && bytes[i + 2] == b'}' && matches!(bytes[i + 1], b'u' | b'1'..=b'9') {
            names.push(char::from(bytes[i + 1]));
            i += 3;
        } else {
            i += 1;
        }
    }
    names
}

pub fn has_placeholder(text: &str) -> bool {
    !placeholder_names(text).is_empty()
}

fn starts_with_placeholder(text: &str) -> bool {
    placeholder_names(text.get(..3).unwrap_or("")).len() == 1
}

/// The index just after the bracketed class that starts at `chars[start]` (`[`).
fn skip_class(chars: &[char], start: usize) -> usize {
    let mut i = start + 1;
    if chars.get(i) == Some(&'^') {
        i += 1;
    }
    if chars.get(i) == Some(&']') {
        i += 1;
    }
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '[' => i = skip_class(chars, i),
            ']' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

/// The syntax reading of the capture count (the contract's reference), for a pattern Rust's
/// parser refuses: the save fails on the pattern anyway, the count only orders the checks alike.
fn scan_capture_count(pattern: &str) -> usize {
    let chars: Vec<char> = pattern.chars().collect();
    let mut count = 0;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                i += 2;
                continue;
            }
            '[' => {
                i = skip_class(&chars, i);
                continue;
            }
            '(' => {
                let named = chars.get(i + 2) == Some(&'<')
                    || (chars.get(i + 2) == Some(&'P') && chars.get(i + 3) == Some(&'<'));
                if chars.get(i + 1) != Some(&'?') || named {
                    count += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    count
}

fn groups_of(pattern: &str) -> usize {
    let pattern = pattern.trim();
    pl_afk::capture_count(pattern).unwrap_or_else(|| scan_capture_count(pattern))
}

/// A plain row the placeholder checks read: a command row, a rule's reply with its rule, or a
/// console line. Secret rows are never read (only the machine sees them, it sends them as written).
#[derive(Debug, Clone, Copy)]
pub enum PlaceholderRow<'a> {
    Command(&'a str),
    Console(&'a str),
    Reply {
        text: &'a str,
        pattern: &'a str,
        regex: bool,
        from: &'a str,
    },
}

/// Why a row breaks the placeholder rules: the contract's `invalid_command` reason
/// (`placeholder_row`, `placeholder_part` with the first `part` beyond the pattern's groups,
/// `placeholder_sender`, `placeholder_start`, `placeholder_command`, `placeholder_length`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceholderProblem {
    pub reason: &'static str,
    pub part: Option<u32>,
}

impl PlaceholderProblem {
    fn of(reason: &'static str) -> Self {
        Self { reason, part: None }
    }
}

/// The placeholder checks of the contract's "Rule placeholders", in their order. They run after
/// the text rule, the password commands and the pattern check (a row those refuse never gets
/// here), and only while Studio has placeholders on. `commands`: Studio's allow list.
pub fn placeholder_problem(
    row: PlaceholderRow<'_>,
    commands: &[String],
) -> Option<PlaceholderProblem> {
    let text = match row {
        PlaceholderRow::Command(text) | PlaceholderRow::Console(text) => text,
        PlaceholderRow::Reply { text, .. } => text,
    };
    let names = placeholder_names(text);
    if names.is_empty() {
        return None;
    }
    let (pattern, regex, from) = match row {
        // The console sends what is typed: braces are plain text there.
        PlaceholderRow::Console(_) => return None,
        PlaceholderRow::Command(_) => return Some(PlaceholderProblem::of("placeholder_row")),
        PlaceholderRow::Reply {
            pattern,
            regex,
            from,
            ..
        } => (pattern, regex, from),
    };
    let groups = if regex { groups_of(pattern) } else { 0 };
    if let Some(part) = names
        .iter()
        .filter_map(|name| name.to_digit(10))
        .find(|n| *n as usize > groups)
    {
        return Some(PlaceholderProblem {
            reason: "placeholder_part",
            part: Some(part),
        });
    }
    if from == "server" && names.contains(&'u') {
        return Some(PlaceholderProblem::of("placeholder_sender"));
    }
    let lead = without_leading_ignorable(text);
    if starts_with_placeholder(lead) {
        return Some(PlaceholderProblem::of("placeholder_start"));
    }
    if let Some(rest) = lead.strip_prefix('/') {
        let word = rest.split(' ').next().unwrap_or("");
        let command = word.to_ascii_lowercase();
        if has_placeholder(word) || !commands.contains(&command) {
            return Some(PlaceholderProblem::of("placeholder_command"));
        }
    }
    let units = text.encode_utf16().count();
    if units + (PLACEHOLDER_VALUE_MAX - 3) * names.len() > MAX_TEXT_UNITS {
        return Some(PlaceholderProblem::of("placeholder_length"));
    }
    None
}

/// Whether the save checks run, and with which allow list (Studio's `features`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaceholderRules {
    pub on: bool,
    pub commands: Vec<String>,
}

impl PlaceholderRules {
    /// Off: braces are plain text, as before the feature.
    pub fn off() -> Self {
        Self::default()
    }

    pub fn of(features: Option<&OfferFeatures>) -> Self {
        match features {
            Some(features) if features.placeholders => Self {
                on: true,
                commands: features
                    .placeholder_commands
                    .iter()
                    .map(|c| c.to_ascii_lowercase())
                    .collect(),
            },
            _ => Self::off(),
        }
    }
}

static VALUE_CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:_|[[\p{L}\p{N}]&&[\x{0}-\x{FFFF}]&&[^\p{Default_Ignorable_Code_Point}]])+$")
        .expect("the value class compiles")
});

/// The client's strict value class (`MCC_MATCH_VARS=strict`): 1–24 characters, each `_` or a
/// letter or number of the Basic Multilingual Plane that shows something. A reply whose value
/// fails it is not sent at all: no space, `.`, `/`, `%`, `\`, link or command gets through.
pub fn strict_value(value: &str) -> bool {
    let units = value.encode_utf16().count();
    (1..=PLACEHOLDER_VALUE_MAX).contains(&units) && VALUE_CLASS.is_match(value)
}

/// One chat line as the client reads it: `public` (a player's chat), `private`, or `other`
/// (anything else, with no sender).
#[derive(Debug, Clone, Copy)]
pub struct RuleLine<'a> {
    pub kind: &'a str,
    pub sender: &'a str,
    pub message: &'a str,
}

/// One rule for [`run_rule`].
#[derive(Debug, Clone, Copy)]
pub struct RuleRun<'a> {
    pub pattern: &'a str,
    pub regex: bool,
    pub from: &'a str,
    pub reply: &'a str,
}

/// Why a rule sent nothing: `from` (not a line of its kind), `self` (the bot's own line),
/// `nomatch`, `value` (a part outside the strict class), `pattern` (it does not compile here).
pub type RuleSilence = &'static str;

/// What the client sends for one chat line by one rule, with strict values: the reply with its
/// values filled in, or why nothing. The engine here is Rust's: "Dene" on the device.
pub fn run_rule(
    rule: RuleRun<'_>,
    line: RuleLine<'_>,
    self_name: &str,
) -> Result<String, RuleSilence> {
    let answers: &[&str] = match rule.from {
        "any" => &["public", "private", "other"],
        "player" => &["public"],
        "private" => &["private"],
        "server" => &["other"],
        _ => &[],
    };
    if !answers.contains(&line.kind) {
        return Err("from");
    }
    if line.kind != "other" && line.sender == self_name {
        return Err("self");
    }
    let pattern = rule.pattern.trim();
    let mut groups: Vec<String> = Vec::new();
    if rule.regex {
        let Ok(compiled) = Regex::new(pattern) else {
            return Err("pattern");
        };
        let Some(caps) = compiled.captures(line.message) else {
            return Err("nomatch");
        };
        groups = (0..caps.len())
            .map(|n| caps.get(n).map_or(String::new(), |m| m.as_str().to_owned()))
            .collect();
    } else if !line
        .message
        .to_lowercase()
        .contains(&pattern.to_lowercase())
    {
        return Err("nomatch");
    }
    let value = |name: char| -> String {
        match name {
            'u' if line.kind == "other" => String::new(),
            'u' => line.sender.to_owned(),
            n => n
                .to_digit(10)
                .and_then(|n| groups.get(n as usize))
                .cloned()
                .unwrap_or_default(),
        }
    };
    let names = placeholder_names(rule.reply);
    if names.iter().any(|name| !strict_value(&value(*name))) {
        return Err("value");
    }
    let mut out = String::with_capacity(rule.reply.len());
    let mut rest = rule.reply;
    while let Some(at) = rest.find('{') {
        let token = rest.get(at..at + 3).unwrap_or("");
        match placeholder_names(token).first() {
            Some(name) if token.len() == 3 => {
                out.push_str(&rest[..at]);
                out.push_str(&value(*name));
                rest = &rest[at + 3..];
            }
            _ => {
                out.push_str(&rest[..=at]);
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// What "Dene" shows for a sample message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CloudReplyTry {
    /// The rule answers with this reply.
    Matches { reply: String },
    /// The message does not match.
    NoMatch,
    /// It matches, but a part is not one word of letters, digits or `_`: no reply goes out.
    Value,
    /// The pattern can't be used (the editor says why).
    Pattern,
}

/// "Dene": the rule against a sample message someone (`sender`: the account's owner, else
/// "Oyuncu") writes, of the kind the rule answers (public chat for `any` and `player`, a private
/// message for `private`, a server line for `server`).
pub fn try_reply(
    pattern: &str,
    regex: bool,
    from: &str,
    reply: &str,
    sample: &str,
    sender: &str,
) -> CloudReplyTry {
    if pl_afk::check_pattern(pattern, regex).is_some() || pattern.trim().is_empty() {
        return CloudReplyTry::Pattern;
    }
    let kind = match from {
        "private" => "private",
        "server" => "other",
        _ => "public",
    };
    let line = RuleLine {
        kind,
        sender,
        message: sample,
    };
    let rule = RuleRun {
        pattern,
        regex,
        from,
        reply,
    };
    match run_rule(rule, line, "") {
        Ok(reply) => CloudReplyTry::Matches { reply },
        Err("value") => CloudReplyTry::Value,
        Err("pattern") => CloudReplyTry::Pattern,
        Err(_) => CloudReplyTry::NoMatch,
    }
}

#[cfg(test)]
mod tests {
    //! The contract's shared cases, copied from projects-studio `services/cloud-afk/test-vectors/`
    //! (`main` `0bac34b`): `rule-placeholder-v1.json` and `console-kinds-v1.json` at `73f5a46`,
    //! `inv-op-v1.json` at `f0fa3a2` (FEAT-W). Copy them again when the contract changes them.

    use super::*;
    use crate::text::{check_text, is_sensitive};

    fn vectors(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn placeholder_vectors() -> Value {
        vectors(include_str!("../tests/vectors/rule-placeholder-v1.json"))
    }

    fn strings(value: &Value) -> Vec<String> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn every_shared_template_case() {
        let all = placeholder_vectors();
        let commands = strings(&all["placeholderCommands"]);
        let mut checked = 0;
        for case in all["templates"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let Some(text) = case["text"].as_str() else {
                continue; // a lone surrogate: a Rust string cannot hold it (text.rs covers it)
            };
            let commands = case
                .get("placeholderCommands")
                .map_or_else(|| commands.clone(), strings);
            let secret = case["secret"].as_bool().unwrap_or(false);
            let expected = &case["problem"];
            // The save's order: the text rule, the password commands, then the placeholders.
            let found = if let Err(problem) = check_text(text) {
                Some((problem.as_str().to_owned(), None))
            } else if !secret && is_sensitive(text) {
                Some(("secret".to_owned(), None))
            } else if secret {
                None
            } else {
                let row = match case["kind"].as_str().unwrap() {
                    "command" => PlaceholderRow::Command(text),
                    "console" => PlaceholderRow::Console(text),
                    _ => PlaceholderRow::Reply {
                        text,
                        pattern: case["rule"]["match"].as_str().unwrap(),
                        regex: case["rule"]["regex"].as_bool().unwrap(),
                        from: case["rule"]["from"].as_str().unwrap(),
                    },
                };
                placeholder_problem(row, &commands).map(|p| (p.reason.to_owned(), p.part))
            };
            let want = expected.as_object().map(|problem| {
                (
                    problem["reason"].as_str().unwrap().to_owned(),
                    problem
                        .get("part")
                        .and_then(Value::as_u64)
                        .map(|p| p as u32),
                )
            });
            assert_eq!(found, want, "{name}: {text}");
            if let Some(captures) = case.get("captures").and_then(Value::as_u64) {
                let pattern = case["rule"]["match"].as_str().unwrap();
                assert_eq!(groups_of(pattern), captures as usize, "{name}");
            }
            checked += 1;
        }
        assert!(checked >= 120, "only {checked} cases");
    }

    #[test]
    fn every_shared_capture_count() {
        let all = placeholder_vectors();
        for case in all["captures"].as_array().unwrap() {
            let pattern = case["pattern"].as_str().unwrap();
            let count = case["count"].as_u64().unwrap() as usize;
            assert_eq!(pl_afk::capture_count(pattern), Some(count), "{pattern}");
            assert_eq!(scan_capture_count(pattern), count, "scan: {pattern}");
        }
    }

    #[test]
    fn every_shared_value_case() {
        let all = placeholder_vectors();
        for case in all["values"].as_array().unwrap() {
            let Some(value) = case["value"].as_str() else {
                continue; // a lone surrogate: never a Rust string
            };
            let ok = case["ok"].as_bool().unwrap();
            assert_eq!(strict_value(value), ok, "{}: {value:?}", case["name"]);
        }
    }

    #[test]
    fn every_shared_run() {
        let all = placeholder_vectors();
        let me = all["self"].as_str().unwrap();
        for case in all["runs"].as_array().unwrap() {
            let rule = RuleRun {
                pattern: case["rule"]["match"].as_str().unwrap(),
                regex: case["rule"]["regex"].as_bool().unwrap(),
                from: case["rule"]["from"].as_str().unwrap(),
                reply: case["rule"]["reply"].as_str().unwrap(),
            };
            let line = RuleLine {
                kind: case["line"]["type"].as_str().unwrap(),
                sender: case["line"]["sender"].as_str().unwrap(),
                message: case["line"]["message"].as_str().unwrap(),
            };
            let got = run_rule(rule, line, me);
            let want = match case["reply"].as_str() {
                Some(reply) => Ok(reply.to_owned()),
                None => Err(case["why"].as_str().unwrap()),
            };
            assert_eq!(got, want, "{}", case["name"]);
        }
    }

    #[test]
    fn try_it_reads_the_rule_as_the_bot_does() {
        let regex = |m: &str, from: &str, reply: &str, sample: &str| {
            try_reply(m, true, from, reply, sample, "Mert")
        };
        assert_eq!(
            regex(
                "^!davet (\\w+)$",
                "any",
                "/msg {u} {1} eklendi.",
                "!davet Ali"
            ),
            CloudReplyTry::Matches {
                reply: "/msg Mert Ali eklendi.".into()
            }
        );
        assert_eq!(
            regex("^!davet (.+)$", "any", "/msg {u} {1}", "!davet a.b"),
            CloudReplyTry::Value
        );
        assert_eq!(
            regex("^fiyat (\\d+)$", "private", "{1} tamam", "selam"),
            CloudReplyTry::NoMatch
        );
        assert_eq!(regex("(a+)+$", "any", "x", "aaa"), CloudReplyTry::Pattern);
        assert_eq!(
            try_reply(
                "SELAM",
                false,
                "player",
                "Selam {u}!",
                "selam millet",
                "Mert"
            ),
            CloudReplyTry::Matches {
                reply: "Selam Mert!".into()
            }
        );
        // A server line has no sender: `{u}` never fills.
        assert_eq!(
            try_reply("ödül", false, "server", "{u} tamam", "Ödül hazır", "Mert"),
            CloudReplyTry::Value
        );
        // Plain braces stay as written.
        assert_eq!(
            try_reply("a", false, "any", "{x} {10} {u}", "a", "Mert"),
            CloudReplyTry::Matches {
                reply: "{x} {10} Mert".into()
            }
        );
    }

    #[test]
    fn every_shared_move_shape() {
        let all = vectors(include_str!("../tests/vectors/inv-op-v1.json"));
        for case in all["shape"].as_array().unwrap() {
            let got = check_inv_op(&case["op"]).err().map(|e| {
                (
                    e.reason.unwrap_or_default(),
                    e.field.unwrap_or_default(),
                    e.code,
                )
            });
            let want = case["problem"].as_object().map(|p| {
                (
                    p["reason"].as_str().unwrap().to_owned(),
                    p.get("field")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                    "invalid".to_owned(),
                )
            });
            assert_eq!(got, want, "{}", case["name"]);
        }
        assert!(is_inv_op_id("m-12_aB"));
        assert!(!is_inv_op_id(""));
        assert!(!is_inv_op_id(&"a".repeat(33)));
        assert!(!is_inv_op_id("a b"));
    }

    fn line(at: i64, text: &str, k: &str, c: Option<&str>) -> CloudConsoleLine {
        CloudConsoleLine {
            at,
            text: text.into(),
            k: k.into(),
            c: c.map(str::to_owned),
            ..CloudConsoleLine::default()
        }
    }

    #[test]
    fn every_kind_of_the_contract_has_a_name() {
        let all = vectors(include_str!("../tests/vectors/console-kinds-v1.json"));
        // `bar` (the action bar) goes to the strip, never to the log.
        for kind in strings(&all["kinds"]).iter().filter(|k| *k != "bar") {
            let kind_of = line_kind(&line(0, "x", "n", Some(kind)));
            assert_eq!(kind_of, kind.as_str(), "{kind}");
            assert_ne!(kind_name(kind_of, false), "Konsol");
            assert_ne!(kind_name(kind_of, true), "Console");
        }
        for case in all["cases"].as_array().unwrap() {
            let expected = &case["expected"];
            if expected["log"].as_bool() == Some(true)
                && let Some(c) = expected["c"].as_str()
            {
                assert_eq!(
                    line_kind(&line(0, "x", "n", Some(c))),
                    c,
                    "{}",
                    case["name"]
                );
            }
        }
        assert_eq!(
            line_kind(&line(0, "/is home", "s", Some("chat"))),
            "command"
        );
        assert_eq!(line_kind(&line(0, "x", "n", Some("later"))), "cli");
        assert_eq!(line_kind(&line(0, "x", "e", None)), "console");
    }

    #[test]
    fn a_download_reads_like_the_contract_says() {
        // 25 Oct 2026 23:59:58 and 26 Oct 00:00:03 in Turkey (UTC+3).
        let first = 1_792_961_998;
        let lines = [
            line(first, "Mert: selam", "n", Some("pm")),
            line(first + 5, "/spawn", "s", Some("sys")),
            line(first + 6, "Bağlanıyor", "n", None),
        ];
        let text = console_text(&lines, "tr");
        assert_eq!(
            text,
            "\u{feff}--- 25.10.2026 ---\r\n[23:59:58] [Özel] Mert: selam\r\n\
             --- 26.10.2026 ---\r\n[00:00:03] [Komut] /spawn\r\n[00:00:04] [Konsol] Bağlanıyor\r\n"
        );
        assert!(console_text(&lines, "en").contains("[Private] Mert: selam"));
        assert_eq!(
            console_file_name("Ahmet_AFK", "tr", first + 5),
            "Ahmet_AFK-konsol-2026-10-26.txt"
        );
        assert_eq!(
            console_file_name("../x", "en", first),
            "x-console-2026-10-25.txt"
        );
        assert!(is_console_file_name("Ahmet_AFK-konsol-2026-10-26.txt"));
        assert!(is_console_file_name("Ahmet_AFK-console-2026-10-26 (2).txt"));
        assert!(!is_console_file_name("Ahmet_AFK-konsol-2026-10-26.exe"));
        assert!(!is_console_file_name("../x-konsol-2026-10-26.txt"));
        assert!(!is_console_file_name("x-tablo-2026-10-26.txt"));
        assert!(!is_console_file_name("x-konsol-26-10-2026.txt"));
        assert!(!is_console_file_name("-konsol-2026-10-26.txt"));
        // "Bugün" starts at midnight in Turkey.
        assert_eq!(CloudLogRange::Today.from(first + 5), first + 2);
        assert_eq!(CloudLogRange::Hour.from(first), first - 3600);
        assert_eq!(CloudLogRange::Days3.from(first), first - 3 * DAY);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn features_read_whatever_the_service_adds() {
        let offer: OfferFeatures =
            serde_json::from_str(r#"{"inventory":"full","placeholders":true,"later":1}"#).unwrap();
        let view = CloudFeatures::from(&offer);
        assert_eq!(view.inventory, "full");
        assert!(view.placeholders && !view.console_kinds);
        assert_eq!(view.placeholder_commands.len(), 8);
        let odd: OfferFeatures = serde_json::from_str(r#"{"inventory":"half"}"#).unwrap();
        assert_eq!(CloudFeatures::from(&odd).inventory, "off");
        let account: CloudAccountFeatures = serde_json::from_str(
            r#"{"inventory":{"mode":"view","on":true},"consoleKinds":true,"more":2}"#,
        )
        .unwrap();
        assert_eq!(account.inventory.mode, "view");
        assert!(account.inventory.on && account.console_kinds && !account.legacy_rules);
        assert_eq!(PlaceholderRules::of(Some(&odd)), PlaceholderRules::off());
        assert!(PlaceholderRules::of(Some(&offer)).on);
    }

    #[test]
    fn frames_read_the_contract_items() {
        let item: CloudInvItem = serde_json::from_str(
            r#"{"slot":36,"id":"minecraft:diamond_sword","count":1,
                "label":{"tr":"Elmas Kılıç","en":"Diamond Sword"},
                "durability":{"left":1450,"max":1561},
                "ench":[{"id":"minecraft:sharpness","lvl":5,"label":{"tr":"Keskinlik V","en":"Sharpness V"}}],
                "enchanted":true,"cmd":10001,"future":"x"}"#,
        )
        .unwrap();
        assert_eq!(item.slot, 36);
        assert_eq!(
            item.durability,
            Some(CloudInvDurability {
                left: 1450,
                max: 1561
            })
        );
        assert_eq!(item.ench[0].label.tr, "Keskinlik V");
        assert!(item.enchanted && item.lore.is_empty());
        assert!(item.name_style.is_empty() && item.lore_style.is_empty() && item.skin.is_none());
        // Machine 0.3.2: a head's skin and the looks of names and lore.
        let head: CloudInvItem = serde_json::from_str(
            r#"{"slot":3,"id":"minecraft:player_head","count":1,"skin":"8a6d5e1f0c3b4a29",
                "name":"Kılıç","nameStyle":[[5,"6lo"]],
                "lore":["Hasar +12",""],"loreStyle":[[[6,"7"],[3,"c"]],null]}"#,
        )
        .unwrap();
        assert_eq!(head.skin.as_deref(), Some("8a6d5e1f0c3b4a29"));
        assert_eq!(head.name_style, [(5, "6lo".to_owned())]);
        assert_eq!(
            head.lore_style,
            [Some(vec![(6, "7".to_owned()), (3, "c".to_owned())]), None]
        );
        assert_eq!(
            serde_json::to_value(&head).unwrap()["nameStyle"],
            serde_json::json!([[5, "6lo"]])
        );
        let open: CloudWinOpen =
            serde_json::from_str(r#"{"type":"generic_9x3","title":"Kitler","size":27}"#).unwrap();
        assert_eq!(open.kind, "generic_9x3");
        assert_eq!(
            serde_json::to_value(&open).unwrap()["type"],
            Value::from("generic_9x3")
        );
    }

    #[test]
    fn a_container_reads_its_stack_count_leniently() {
        let read = |stacks: &str| -> CloudInvItem {
            serde_json::from_str(&format!(
                r#"{{"slot":17,"id":"minecraft:shulker_box","count":1,
                    "label":{{"tr":"Shulker Kutusu","en":"Shulker Box"}},
                    "inside":{{"count":1000,"top":[{{"id":"minecraft:dirt","count":64}}]{stacks}}}}}"#
            ))
            .unwrap()
        };
        let stacks = |s: &str| read(s).inside.unwrap().stacks;
        assert_eq!(stacks(r#","stacks":16"#), Some(16));
        assert_eq!(stacks(r#","stacks":256"#), Some(256));
        // An older machine sends none; it writes none back either.
        assert_eq!(stacks(""), None);
        let old = serde_json::to_value(read("")).unwrap();
        assert!(old["inside"].get("stacks").is_none());
        assert_eq!(
            serde_json::to_value(read(r#","stacks":16"#)).unwrap()["inside"]["stacks"],
            Value::from(16)
        );
        // An odd count is absent; the item and the rest of its inside still read.
        for odd in ["257", "-1", "12.5", r#""16""#, "null", "[16]", "true"] {
            let item = read(&format!(r#","stacks":{odd}"#));
            let inside = item.inside.expect("the inside reads");
            assert_eq!(inside.stacks, None, "{odd}");
            assert_eq!((inside.count, inside.top.len()), (1000, 1), "{odd}");
        }
    }

    #[test]
    fn placeholder_names_are_exactly_the_tokens() {
        assert_eq!(
            placeholder_names("{u} {1}{9} {0} {10} {U} { u }"),
            vec!['u', '1', '9']
        );
        assert!(starts_with_placeholder("{1}x"));
        assert!(!starts_with_placeholder("{x}"));
        assert!(!has_placeholder("ç{"));
    }
}
