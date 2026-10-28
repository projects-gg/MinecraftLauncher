//! The checks before an AFK+ settings or order write (README "AfkPlusConfig v2" → Rules): a port
//! of the contract's reference `test-vectors/afkplus-config.mjs` that runs every case of
//! `afkplus-config-v2.json` (copied to `tests/vectors/`). It reads the document as JSON, the way
//! the Worker does, so unknown keys, wrong types and missing fields fail on the same path; the
//! first failing rule is the error, fields in document order (`customItems` before `orders`, which
//! name them), then the drops and counts.
//!
//! Two parts of the service's rules are not here: a persona text's (and a custom item's name)
//! links, contact data and listed words (the service's text rule and its judge; the launcher's
//! form says the same in TypeScript with the chat composer's rule), and the account's return rows
//! when the panel does not know them. What the Worker cannot see either (the item data: an item
//! in the account version's data, enchantments that fit) the order form checks with the data.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::plus::{
    CUSTOM_NAME_MAX, GENERIC_MAX_DEAL, MAX_BLOCKED, MAX_COUNT, MAX_FRIENDS, MAX_INTRO, MAX_NOTE,
    MAX_PAUSE_SECONDS, MAX_STATUS, MAX_WINDOWS, PlusIssue, is_player_name, is_time,
};

/// The largest stored document (raw UTF-8 bytes).
pub const MAX_CONFIG_BYTES: usize = 16 * 1024;
/// A finished order stays this long before a save drops it (the "Biten" list).
pub const FINISHED_KEEP_SECONDS: i64 = 7 * 86_400;
/// A custom item no order shows is dropped by a save once it is this old.
pub const CUSTOM_KEEP_SECONDS: i64 = 30 * 86_400;
const GENERIC: &str = "generic";
const INT32: i64 = 1 << 31;

const STATUS_PRESETS: [&str; 5] = [
    "back_soon",
    "back_evening",
    "back_tomorrow",
    "weekend",
    "busy",
];
const ROOT_KEYS: [&str; 19] = [
    "v",
    "enabled",
    "mode",
    "lang",
    "persona",
    "replies",
    "authority",
    "allow",
    "pay",
    "who",
    "orders",
    "customItems",
    "limits",
    "discovery",
    "trade",
    "summaryAt",
    "blocked",
    "schedule",
    "pauseUntil",
];
const ORDER_KEYS: [&str; 16] = [
    "id", "on", "side", "item", "qty", "req", "pay", "unit", "target", "limit", "style", "trade",
    "until", "note", "src", "at",
];
const CUSTOM_KEYS: [&str; 7] = ["k", "name", "base", "fp", "look", "from", "at"];
const LIMIT_KEYS: [&str; 7] = [
    "perDealMax",
    "dailySpendMax",
    "totalBudget",
    "balanceFloor",
    "approvalAbove",
    "newCounterpartApprovalAbove",
    "approvalTimeoutMin",
];

/// What a write is checked against (the reference's `ctx`, the vectors' `context`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusCheckContext {
    /// Unix seconds.
    pub now: i64,
    /// The profiles open for the account's server by id (the generic profile is always open).
    pub modes: BTreeMap<String, PlusModeFacts>,
    /// The stored mode stays allowed while its profile is closed (trading then waits).
    pub stored_mode: Option<String>,
    pub stored_profile: Option<PlusModeFacts>,
    /// The plain command rows of the account's config; None: not known here (the service checks).
    pub return_rows: Option<Vec<String>>,
    pub features: PlusCheckFeatures,
    pub limits: PlusCheckLimits,
    /// The stored document: orders and custom items a write leaves out stay; orders deleted during a
    /// trade (`end`) are the Worker's and always stay.
    pub stored: Option<Value>,
    /// The machine's last progress per order, for the drops.
    pub progress: BTreeMap<String, PlusProgress>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusModeFacts {
    pub max_deal: Option<u64>,
    pub ad_templates: Vec<String>,
    pub approval_above: Option<u64>,
}

/// Studio's switches: only an explicit `false` refuses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusCheckFeatures {
    pub item_pay: Option<bool>,
    pub custom_items: Option<bool>,
    /// "Para birimi": while off, a new money name is refused (the stored one may come back).
    pub currency: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusCheckLimits {
    pub order_max: u32,
    pub order_store: u32,
    pub custom_items_max: u32,
    pub order_days_max: u32,
    pub order_days_default: u32,
}

impl Default for PlusCheckLimits {
    fn default() -> Self {
        Self {
            order_max: 10,
            order_store: 20,
            custom_items_max: 20,
            order_days_max: 30,
            order_days_default: 14,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct PlusProgress {
    pub got: u64,
    pub at: Option<i64>,
}

/// Why a write would be refused, as the service refuses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlusRefusal {
    /// `400 invalid_plus_config {field, reason, problem?}`.
    Invalid {
        field: String,
        reason: String,
        problem: Option<String>,
    },
    /// `422 limit_rows {field, max}`.
    LimitRows { field: String, max: u32 },
    /// `422 limit_orders {scope: active | stored, max}`.
    LimitOrders { scope: String, max: u32 },
    /// `409 client_outdated` (a version 1 document).
    ClientOutdated,
    /// `413 payload_too_large`.
    TooLarge,
}

impl PlusRefusal {
    fn invalid(field: impl Into<String>, reason: &str) -> Self {
        Self::Invalid {
            field: field.into(),
            reason: reason.to_owned(),
            problem: None,
        }
    }

    /// The refusal as the panel shows it, on the contract's field path.
    pub fn issue(&self) -> PlusIssue {
        match self {
            Self::Invalid {
                field,
                reason,
                problem,
            } => PlusIssue {
                field: field.clone(),
                reason: reason.clone(),
                problem: problem.clone(),
            },
            Self::LimitRows { field, .. } => PlusIssue::new(field.clone(), "limit_rows"),
            Self::LimitOrders { scope, .. } => PlusIssue {
                field: "orders".into(),
                reason: "limit_orders".into(),
                problem: Some(scope.clone()),
            },
            Self::ClientOutdated => PlusIssue::new("v", "client_outdated"),
            Self::TooLarge => PlusIssue::new("config", "too_large"),
        }
    }
}

type Checked<T> = Result<T, PlusRefusal>;

fn kind_of(value: Option<&Value>) -> &'static str {
    if value.is_none() { "missing" } else { "type" }
}

fn object<'a>(value: Option<&'a Value>, field: &str) -> Checked<&'a Map<String, Value>> {
    match value {
        Some(Value::Object(map)) => Ok(map),
        other => Err(PlusRefusal::invalid(field, kind_of(other))),
    }
}

fn only(map: &Map<String, Value>, keys: &[&str], field: &str) -> Checked<()> {
    for key in map.keys() {
        if !keys.contains(&key.as_str()) {
            let short: String = key.chars().take(40).collect();
            let path = if field.is_empty() {
                short
            } else {
                format!("{field}.{short}")
            };
            return Err(PlusRefusal::invalid(path, "unknown"));
        }
    }
    Ok(())
}

fn boolean(value: Option<&Value>, field: &str) -> Checked<bool> {
    match value {
        Some(Value::Bool(b)) => Ok(*b),
        other => Err(PlusRefusal::invalid(field, kind_of(other))),
    }
}

fn string<'a>(value: Option<&'a Value>, field: &str) -> Checked<&'a str> {
    match value {
        Some(Value::String(text)) => Ok(text),
        other => Err(PlusRefusal::invalid(field, kind_of(other))),
    }
}

/// A whole JSON number (JavaScript's `Number.isInteger`: `2.0` is one, `1.5` is not).
fn whole(value: &Value) -> Option<i64> {
    let Value::Number(number) = value else {
        return None;
    };
    if let Some(n) = number.as_i64() {
        return Some(n);
    }
    if number.as_u64().is_some() {
        return Some(i64::MAX);
    }
    let f = number.as_f64()?;
    if !f.is_finite() || f.fract() != 0.0 {
        return None;
    }
    Some(if f >= i64::MAX as f64 {
        i64::MAX
    } else if f <= i64::MIN as f64 {
        i64::MIN
    } else {
        f as i64
    })
}

fn integer(value: Option<&Value>, field: &str) -> Checked<i64> {
    value
        .and_then(whole)
        .ok_or_else(|| PlusRefusal::invalid(field, kind_of(value)))
}

fn ranged(value: Option<&Value>, field: &str, min: i64, max: i64) -> Checked<i64> {
    let n = integer(value, field)?;
    if n < min || n > max {
        return Err(PlusRefusal::invalid(field, "range"));
    }
    Ok(n)
}

fn one_of<'a>(value: Option<&'a Value>, field: &str, values: &[&str]) -> Checked<&'a str> {
    let text = string(value, field)?;
    if !values.contains(&text) {
        return Err(PlusRefusal::invalid(field, "range"));
    }
    Ok(text)
}

fn array<'a>(value: Option<&'a Value>, field: &str) -> Checked<&'a Vec<Value>> {
    match value {
        Some(Value::Array(list)) => Ok(list),
        other => Err(PlusRefusal::invalid(field, kind_of(other))),
    }
}

fn amount(
    value: Option<&Value>,
    field: &str,
    min: i64,
    max_deal: u64,
    nullable: bool,
) -> Checked<Option<i64>> {
    if nullable && matches!(value, Some(Value::Null)) {
        return Ok(None);
    }
    let n = integer(value, field)?;
    if n < min {
        return Err(PlusRefusal::invalid(field, "range"));
    }
    if n > i64::try_from(max_deal).unwrap_or(i64::MAX) {
        return Err(PlusRefusal::invalid(field, "limits"));
    }
    Ok(Some(n))
}

fn cp(text: &str) -> usize {
    text.chars().count()
}

fn all_bytes(text: &str, ok: impl Fn(u8) -> bool) -> bool {
    text.bytes().all(ok)
}

/// `^minecraft:[a-z0-9_]{1,64}$`: the game's item (and enchantment) id, ITEMART's key.
pub fn is_vanilla_id(text: &str) -> bool {
    text.strip_prefix("minecraft:").is_some_and(|path| {
        (1..=64).contains(&path.len())
            && all_bytes(path, |b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
            })
    })
}

/// `^c[0-9a-z]{1,4}$`: a custom item's key.
pub fn is_custom_key(text: &str) -> bool {
    text.strip_prefix('c').is_some_and(|rest| {
        (1..=4).contains(&rest.len())
            && all_bytes(rest, |b| b.is_ascii_lowercase() || b.is_ascii_digit())
    })
}

/// `^[a-z0-9]{1,8}$`: an order's id.
pub fn is_order_id(text: &str) -> bool {
    (1..=8).contains(&text.len())
        && all_bytes(text, |b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn is_fingerprint(text: &str) -> bool {
    text.len() == 32 && all_bytes(text, |b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `^[a-z0-9_.-]{1,32}:[a-z0-9_./-]{1,64}$`: a look's item model.
fn is_model(text: &str) -> bool {
    let Some((space, path)) = text.split_once(':') else {
        return false;
    };
    (1..=32).contains(&space.len())
        && (1..=64).contains(&path.len())
        && all_bytes(space, |b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-')
        })
        && all_bytes(path, |b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'/' | b'-')
        })
}

fn is_template_id(text: &str) -> bool {
    (1..=32).contains(&text.len())
        && all_bytes(text, |b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
        })
}

/// Characters a chat line never keeps: controls (but the line break) and invisible format marks.
fn invisible(c: char) -> bool {
    (c.is_control() && c != '\n')
        || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}' | '\u{feff}' | '\u{00ad}' | '\u{115f}' | '\u{1160}'
            | '\u{3164}' | '\u{ffa0}' | '\u{2800}')
}

/// A text as the service keeps it (the chat filter's `cleanChatText`, without its Unicode
/// normalization): line breaks unified, invisible characters out, spaces collapsed, empty lines
/// dropped, at most five lines (more join the fifth), trimmed.
pub fn clean_text(value: &str) -> String {
    let unified = value
        .replace("\r\n", "\n")
        .replace(['\r', '\u{85}', '\u{2028}', '\u{2029}'], "\n");
    let kept: String = unified
        .chars()
        .map(|c| {
            if matches!(c, '\t' | '\u{b}' | '\u{c}') {
                ' '
            } else {
                c
            }
        })
        .filter(|c| !invisible(*c))
        .collect();
    let mut lines: Vec<String> = kept
        .split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() > 5 {
        let rest = lines.split_off(4).join(" ");
        lines.push(rest);
    }
    lines.join("\n")
}

/// A full-width form read as its ASCII letter, as NFKC reads it (the marker written wide).
fn narrow(c: char) -> char {
    match u32::from(c) {
        0xFF01..=0xFF5E => char::from_u32(u32::from(c) - 0xFEE0).unwrap_or(c),
        0x3000 => ' ',
        _ => c,
    }
}

/// The `[AFK+]` marker the bot's own lines carry, however it is spaced, cased or widened.
fn has_marker(text: &str) -> bool {
    let squeezed: String = text
        .chars()
        .map(narrow)
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    squeezed.contains("[afk+]")
}

/// A client variable (`%username%`): letters, digits or `_` between two `%`.
fn has_variable(text: &str) -> bool {
    let mut run: Option<usize> = None;
    for c in text.chars() {
        run = match (c, run) {
            ('%', Some(n)) if n > 0 => return true,
            ('%', _) => Some(0),
            (c, Some(n)) if c.is_alphanumeric() || c == '_' => Some(n + 1),
            _ => None,
        };
    }
    false
}

/// A combining mark (the ranges chat text knows): nothing that shows by itself.
fn is_mark(c: char) -> bool {
    matches!(
        u32::from(c),
        0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F
    )
}

/// The reference's form rule: one line, no command or format code first (`/`, `\`, `&`, `§`), no
/// colour code (`§`, `&a`), no client variable, and something that shows.
fn form_problem(cleaned: &str) -> bool {
    let chars: Vec<char> = cleaned.chars().collect();
    let colour = chars.windows(2).any(|pair| {
        pair[0] == '&'
            && matches!(pair[1].to_ascii_lowercase(), '0'..='9' | 'a'..='f' | 'k'..='o' | 'r')
    });
    let shows = chars
        .iter()
        .any(|c| !c.is_whitespace() && !is_mark(*c) && *c != '\u{200D}');
    cleaned.contains('\n')
        || cleaned.starts_with(['/', '\\', '&', '§'])
        || cleaned.contains('§')
        || colour
        || has_variable(cleaned)
        || !shows
}

/// The tag's most code points (README "AfkPlusConfig v2" → `persona.prefix`).
pub const PREFIX_MAX: usize = 16;
/// The tag's text until the player sets one (the tag itself is off).
pub const PREFIX_DEFAULT_TEXT: &str = "[AFK+]";

/// Staff, system and server words, folded, anywhere in a tag's letters (the reference's list).
const PREFIX_STAFF_WORDS: [&str; 30] = [
    "yetkili",
    "admin",
    "moderator",
    "sistem",
    "system",
    "server",
    "sunucu",
    "konsol",
    "console",
    "staff",
    "kurucu",
    "founder",
    "owner",
    "destek",
    "support",
    "rehber",
    "helper",
    "duyuru",
    "bilgi",
    "info",
    "projects",
    "operator",
    "yonetic",
    "yonetim",
    "guvenlik",
    "security",
    "resmi",
    "official",
    "gelistirici",
    "developer",
];
/// Short staff words: whole words only (camel case split first).
const PREFIX_STAFF_SHORT: [&str; 6] = ["mod", "adm", "op", "gm", "sys", "dev"];

/// A tag's letters folded the way the reference reads them: lower case, Turkish letters plain,
/// look-alike digits and marks as letters (`4dm1n`), camel case split (`ModBot` → `mod bot`).
fn staff_form(text: &str) -> String {
    let mut split = String::with_capacity(text.len() + 4);
    let mut previous_lower = false;
    for c in text.chars() {
        if previous_lower && c.is_uppercase() {
            split.push(' ');
        }
        previous_lower = c.is_lowercase();
        split.push(c);
    }
    split
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !is_mark(*c))
        .map(|c| match c {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' => 'i',
            'ö' => 'o',
            'ş' => 's',
            'ü' => 'u',
            'â' => 'a',
            'î' => 'i',
            'û' => 'u',
            '0' => 'o',
            '1' | '!' | '|' => 'i',
            '3' => 'e',
            '4' => 'a',
            '5' => 's',
            '7' => 't',
            other => other,
        })
        .collect()
}

/// The tag before the assistant's lines, as the launcher checks it (the reference's
/// `checkPrefix`): the cleaned text, or why not, in its order: `length` (empty), `form` (one line,
/// no code, only Latin letters, digits, spaces and `[]()+-_.!?*#|~^=:'`), `length` (over 16),
/// `staff` (a staff, system or server word) and `name` (one of `names`: the document's friends and
/// blocked players). Links, contact data, apps and listed words are the service's.
pub fn check_prefix(raw: &str, names: &[String]) -> Result<String, &'static str> {
    static CHARS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^[\p{Script=Latin}0-9 \[\]()+\-_.!?*#|~^=:']+$")
            .expect("a valid pattern")
    });
    let cleaned = clean_text(raw);
    if cleaned.is_empty() {
        return Err("length");
    }
    if form_problem(&cleaned) || !CHARS.is_match(&cleaned) {
        return Err("form");
    }
    if cp(&cleaned) > PREFIX_MAX {
        return Err("length");
    }
    let form = staff_form(&cleaned);
    let short = form
        .split(|c: char| !c.is_ascii_lowercase())
        .any(|word| PREFIX_STAFF_SHORT.contains(&word));
    let letters: String = form.chars().filter(char::is_ascii_lowercase).collect();
    if short || PREFIX_STAFF_WORDS.iter().any(|word| letters.contains(word)) {
        return Err("staff");
    }
    let lower = cleaned.to_lowercase();
    let named = lower
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .any(|word| !word.is_empty() && names.iter().any(|name| name.to_lowercase() == word));
    if named {
        return Err("name");
    }
    Ok(cleaned)
}

/// The longest money name (`trade.currency`, code points).
pub const CURRENCY_MAX: usize = 16;

/// The money name the player gives ("Para birimi", the reference's `checkCurrency`): the cleaned
/// text, or why not, in its order: `length` (empty), `form` (one line, only Latin letters, spaces,
/// `. _ -` and `$ € ₺ £`, with a letter or a sign; no digit, since the reply rule reads every digit
/// as a number) and `length` (over 16). Listed words, links, contact data and apps are the
/// service's.
pub fn check_currency(raw: &str) -> Result<String, &'static str> {
    static CHARS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^[\p{Script=Latin} ._\-$€₺£]+$").expect("a valid pattern")
    });
    static SHOWN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"[\p{Script=Latin}$€₺£]").expect("a valid pattern")
    });
    let cleaned = clean_text(raw);
    if cleaned.is_empty() {
        return Err("length");
    }
    if form_problem(&cleaned) || !CHARS.is_match(&cleaned) || !SHOWN.is_match(&cleaned) {
        return Err("form");
    }
    if cp(&cleaned) > CURRENCY_MAX {
        return Err("length");
    }
    Ok(cleaned)
}

/// The persona text rule's part the launcher knows (README "Persona texts"; a custom item's name
/// too): the cleaned text, or why not (`form`, `length`, `marker`, in the reference's order).
/// Links, contact data and listed words are the service's.
pub fn check_persona(raw: &str, max: usize) -> Result<String, &'static str> {
    let cleaned = clean_text(raw);
    if cleaned.is_empty() {
        return Ok(cleaned);
    }
    if form_problem(&cleaned) {
        return Err("form");
    }
    if cp(&cleaned) > max {
        return Err("length");
    }
    if has_marker(&cleaned) {
        return Err("marker");
    }
    Ok(cleaned)
}

/// The money cap of a mode: its profile's `money.maxDeal`, the generic profile's 100,000,000.
fn max_deal_of(ctx: &PlusCheckContext, mode: &str) -> u64 {
    if mode == GENERIC {
        return GENERIC_MAX_DEAL;
    }
    ctx.modes
        .get(mode)
        .and_then(|facts| facts.max_deal)
        .or_else(|| ctx.stored_profile.as_ref().and_then(|facts| facts.max_deal))
        .unwrap_or(GENERIC_MAX_DEAL)
}

/// An item reference: a vanilla id, or a custom item of the document.
fn ref_of(value: Option<&Value>, custom_keys: &HashSet<String>) -> Option<String> {
    let text = value?.as_str()?;
    if is_vanilla_id(text) {
        return Some(text.to_owned());
    }
    let key = text.strip_prefix("custom:")?;
    (is_custom_key(key) && custom_keys.contains(key)).then(|| text.to_owned())
}

fn custom_key_of(reference: &str) -> Option<&str> {
    reference
        .strip_prefix("custom:")
        .filter(|key| is_custom_key(key))
}

fn int_field(order: &Value, key: &str) -> i64 {
    order.get(key).and_then(whole).unwrap_or(0)
}

/// When an order finished (expired at its `until`, done at its progress's time), if it did.
pub fn finished_at(
    order: &Value,
    now: i64,
    progress: &BTreeMap<String, PlusProgress>,
) -> Option<i64> {
    let id = order.get("id").and_then(Value::as_str).unwrap_or("");
    let qty = int_field(order, "qty");
    if let Some(entry) = progress.get(id)
        && i64::try_from(entry.got).unwrap_or(i64::MAX) >= qty
    {
        // Done without a time (an older snapshot): as if just now, so it stays its week.
        return Some(entry.at.unwrap_or(now));
    }
    let until = int_field(order, "until");
    (until <= now).then_some(until)
}

/// Whether an order is active: on, not deleted during a trade, not finished.
pub fn is_active(order: &Value, now: i64, progress: &BTreeMap<String, PlusProgress>) -> bool {
    order.get("on").and_then(Value::as_bool) == Some(true)
        && order.get("end").and_then(Value::as_bool) != Some(true)
        && finished_at(order, now, progress).is_none()
}

/// A write's settings as checked and stored (the reference's `checkConfig`): the document the
/// service would keep, or why it would refuse it.
pub fn check_config_value(input: &Value, ctx: &PlusCheckContext) -> Checked<Value> {
    let config = object(Some(input), "config")?;
    if config.get("v").and_then(whole) == Some(1) {
        return Err(PlusRefusal::ClientOutdated);
    }
    only(config, &ROOT_KEYS, "")?;
    if config.get("v").and_then(whole) != Some(2) {
        return Err(PlusRefusal::invalid("v", kind_or_range(config.get("v"))));
    }
    let now = ctx.now;
    let stored = ctx.stored.as_ref();
    let enabled = boolean(config.get("enabled"), "enabled")?;
    let mode = string(config.get("mode"), "mode")?;
    if mode != GENERIC && !ctx.modes.contains_key(mode) && ctx.stored_mode.as_deref() != Some(mode)
    {
        return Err(PlusRefusal::invalid("mode", "mode"));
    }
    let max_deal = max_deal_of(ctx, mode);
    let lang = one_of(config.get("lang"), "lang", &["auto", "tr", "en"])?;

    let persona_in = object(config.get("persona"), "persona")?;
    only(
        persona_in,
        &[
            "preset",
            "intro",
            "status",
            "statusPreset",
            "smallTalk",
            "playerTalk",
            "prefix",
        ],
        "persona",
    )?;
    let preset = one_of(
        persona_in.get("preset"),
        "persona.preset",
        &["friendly", "brief", "polite", "playful"],
    )?;
    let status_preset = match persona_in.get("statusPreset") {
        Some(Value::Null) => None,
        other => Some(one_of(other, "persona.statusPreset", &STATUS_PRESETS)?),
    };
    let mut intro = String::new();
    let mut status = String::new();
    for (field, max) in [("intro", MAX_INTRO), ("status", MAX_STATUS)] {
        let path = format!("persona.{field}");
        let raw = string(persona_in.get(field), &path)?;
        if field == "status" && status_preset.is_some() {
            continue;
        }
        let checked = check_persona(raw, max).map_err(|problem| PlusRefusal::Invalid {
            field: path.clone(),
            reason: "text".into(),
            problem: Some(problem.into()),
        })?;
        if field == "intro" {
            intro = checked;
        } else {
            status = checked;
        }
    }
    let small_talk = boolean(persona_in.get("smallTalk"), "persona.smallTalk")?;
    // "Oyuncu gibi konuşsun" and the tag (27 Oct 2026): a write without them keeps the stored ones,
    // so a client that does not know them yet changes nothing.
    let stored_persona = stored.and_then(|doc| doc.get("persona"));
    let player_talk = match persona_in.get("playerTalk") {
        None => stored_persona
            .and_then(|persona| persona.get("playerTalk"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        other => boolean(other, "persona.playerTalk")?,
    };
    let tag_refusal = |problem: &'static str| PlusRefusal::Invalid {
        field: "persona.prefix.text".into(),
        reason: "text".into(),
        problem: Some(problem.into()),
    };
    let (prefix_on, prefix_text) = match persona_in.get("prefix") {
        None => stored_persona
            .and_then(|persona| persona.get("prefix"))
            .map(|prefix| {
                (
                    prefix.get("on").and_then(Value::as_bool).unwrap_or(false),
                    prefix
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or(PREFIX_DEFAULT_TEXT)
                        .to_owned(),
                )
            })
            .unwrap_or((false, PREFIX_DEFAULT_TEXT.to_owned())),
        Some(value) => {
            let prefix_in = object(Some(value), "persona.prefix")?;
            only(prefix_in, &["on", "text"], "persona.prefix")?;
            let on = boolean(prefix_in.get("on"), "persona.prefix.on")?;
            let raw = string(prefix_in.get("text"), "persona.prefix.text")?;
            (on, check_prefix(raw, &[]).map_err(tag_refusal)?)
        }
    };

    let replies_in = object(config.get("replies"), "replies")?;
    only(replies_in, &["scope", "away", "saveAt"], "replies")?;
    let scope = one_of(
        replies_in.get("scope"),
        "replies.scope",
        &["all", "trade", "off"],
    )?;
    let away = boolean(replies_in.get("away"), "replies.away")?;
    let save_at = match replies_in.get("saveAt") {
        Some(Value::Null) => Value::Null,
        Some(value) if matches!(whole(value), Some(50 | 80)) => value.clone(),
        Some(value @ Value::Number(_)) => {
            let _ = value;
            return Err(PlusRefusal::invalid("replies.saveAt", "range"));
        }
        other => return Err(PlusRefusal::invalid("replies.saveAt", kind_of(other))),
    };

    let authority = one_of(
        config.get("authority"),
        "authority",
        &["reply", "ask", "deal"],
    )?;
    let allow_in = object(config.get("allow"), "allow")?;
    only(allow_in, &["buy", "sell"], "allow")?;
    let allow_buy = boolean(allow_in.get("buy"), "allow.buy")?;
    let allow_sell = boolean(allow_in.get("sell"), "allow.sell")?;
    let pay = one_of(config.get("pay"), "pay", &["money", "items", "both"])?;
    if pay != "money" && ctx.features.item_pay == Some(false) {
        return Err(PlusRefusal::invalid("pay", "feature"));
    }

    let who_in = object(config.get("who"), "who")?;
    only(who_in, &["mode", "friends"], "who")?;
    let who_mode = one_of(who_in.get("mode"), "who.mode", &["all", "friends"])?;
    let friends_in = array(who_in.get("friends"), "who.friends")?;
    if friends_in.len() > MAX_FRIENDS {
        return Err(PlusRefusal::LimitRows {
            field: "who.friends".into(),
            max: MAX_FRIENDS as u32,
        });
    }
    let mut friend_names = HashSet::new();
    let mut friends = Vec::new();
    for (i, value) in friends_in.iter().enumerate() {
        let path = format!("who.friends[{i}]");
        let name = string(Some(value), &path)?;
        if !is_player_name(name) || !friend_names.insert(name.to_ascii_lowercase()) {
            return Err(PlusRefusal::invalid(path, "name"));
        }
        friends.push(name.to_owned());
    }

    // Custom items first: orders name them.
    let custom_items: Vec<Value> = match config.get("customItems") {
        None => stored
            .and_then(|doc| doc.get("customItems"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        Some(value) => parse_custom_items(value, now)?,
    };
    let custom_keys: HashSet<String> = custom_items
        .iter()
        .filter_map(|item| item.get("k").and_then(Value::as_str).map(str::to_owned))
        .collect();

    let stored_orders: Vec<Value> = stored
        .and_then(|doc| doc.get("orders"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let is_end = |order: &Value| order.get("end").and_then(Value::as_bool) == Some(true);
    let kept: Vec<Value> = stored_orders
        .iter()
        .filter(|o| is_end(o))
        .cloned()
        .collect();
    let orders: Vec<Value> = match config.get("orders") {
        None => stored_orders
            .iter()
            .filter(|o| !is_end(o))
            .cloned()
            .collect(),
        Some(value) => {
            let orders_in = array(Some(value), "orders")?;
            let mut seen: HashSet<String> = kept
                .iter()
                .filter_map(|o| o.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect();
            let facts = OrderFacts {
                ctx,
                now,
                pay,
                max_deal,
                custom_keys: &custom_keys,
                stored_orders: &stored_orders,
            };
            let mut out = Vec::with_capacity(orders_in.len());
            for (i, value) in orders_in.iter().enumerate() {
                out.push(parse_order(
                    value,
                    &format!("orders[{i}]"),
                    &facts,
                    &mut seen,
                )?);
            }
            out
        }
    };

    let limits_in = object(config.get("limits"), "limits")?;
    only(limits_in, &LIMIT_KEYS, "limits")?;
    let per_deal_max = amount(
        limits_in.get("perDealMax"),
        "limits.perDealMax",
        1,
        max_deal,
        true,
    )?;
    let daily_spend_max = amount(
        limits_in.get("dailySpendMax"),
        "limits.dailySpendMax",
        1,
        max_deal,
        true,
    )?;
    let total_budget = amount(
        limits_in.get("totalBudget"),
        "limits.totalBudget",
        1,
        max_deal,
        true,
    )?;
    let balance_floor = amount(
        limits_in.get("balanceFloor"),
        "limits.balanceFloor",
        0,
        max_deal,
        false,
    )?;
    let approval_above = amount(
        limits_in.get("approvalAbove"),
        "limits.approvalAbove",
        1,
        max_deal,
        true,
    )?;
    if let (Some(above), Some(cap)) = (approval_above, per_deal_max)
        && above > cap
    {
        return Err(PlusRefusal::invalid("limits.approvalAbove", "limits"));
    }
    let new_counterpart = amount(
        limits_in.get("newCounterpartApprovalAbove"),
        "limits.newCounterpartApprovalAbove",
        1,
        max_deal,
        true,
    )?;
    let timeout = integer(
        limits_in.get("approvalTimeoutMin"),
        "limits.approvalTimeoutMin",
    )?;
    if !(1..=60).contains(&timeout) {
        return Err(PlusRefusal::invalid("limits.approvalTimeoutMin", "limits"));
    }

    let discovery_in = object(config.get("discovery"), "discovery")?;
    only(
        discovery_in,
        &["scanChat", "proactivePm", "ads", "adTemplate"],
        "discovery",
    )?;
    let scan_chat = boolean(discovery_in.get("scanChat"), "discovery.scanChat")?;
    let proactive_pm = boolean(discovery_in.get("proactivePm"), "discovery.proactivePm")?;
    let ads = boolean(discovery_in.get("ads"), "discovery.ads")?;
    let ad_template = string(discovery_in.get("adTemplate"), "discovery.adTemplate")?;
    let templates: &[String] = if mode == GENERIC {
        &[]
    } else {
        ctx.modes
            .get(mode)
            .map_or(&[][..], |facts| facts.ad_templates.as_slice())
    };
    if !is_template_id(ad_template) || (ads && !templates.iter().any(|t| t == ad_template)) {
        return Err(PlusRefusal::invalid("discovery.adTemplate", "range"));
    }

    let trade_in = object(config.get("trade"), "trade")?;
    only(trade_in, &["returnRow", "currency"], "trade")?;
    let return_row = match trade_in.get("returnRow") {
        Some(Value::Null) => Value::Null,
        other => {
            let id = string(other, "trade.returnRow")?;
            if let Some(rows) = &ctx.return_rows
                && !rows.iter().any(|row| row == id)
            {
                return Err(PlusRefusal::invalid("trade.returnRow", "row"));
            }
            Value::String(id.to_owned())
        }
    };
    // The money's name (28 Oct 2026): a write without it keeps the stored one (a client that does
    // not know it changes nothing), null clears it. While Studio's switch is off a new name is
    // refused; the stored one may come back unchanged, so a client that sends it back is not stuck.
    let stored_currency = stored
        .and_then(|doc| doc.get("trade"))
        .and_then(|trade| trade.get("currency"))
        .and_then(Value::as_str);
    let currency = match trade_in.get("currency") {
        None => stored_currency.map(str::to_owned),
        Some(Value::Null) => None,
        other => {
            let raw = string(other, "trade.currency")?;
            let name = check_currency(raw).map_err(|problem| PlusRefusal::Invalid {
                field: "trade.currency".into(),
                reason: "text".into(),
                problem: Some(problem.into()),
            })?;
            if ctx.features.currency == Some(false) && stored_currency != Some(name.as_str()) {
                return Err(PlusRefusal::invalid("trade.currency", "feature"));
            }
            Some(name)
        }
    };
    let mut trade = serde_json::json!({"returnRow": return_row});
    if let Some(name) = currency {
        trade["currency"] = Value::String(name);
    }

    let summary_at = string(config.get("summaryAt"), "summaryAt")?;
    if !is_time(summary_at) {
        return Err(PlusRefusal::invalid("summaryAt", "range"));
    }

    let blocked_in = array(config.get("blocked"), "blocked")?;
    if blocked_in.len() > MAX_BLOCKED {
        return Err(PlusRefusal::LimitRows {
            field: "blocked".into(),
            max: MAX_BLOCKED as u32,
        });
    }
    let mut names = HashSet::new();
    let mut blocked = Vec::new();
    for (i, value) in blocked_in.iter().enumerate() {
        let path = format!("blocked[{i}]");
        let name = string(Some(value), &path)?;
        if !is_player_name(name) || !names.insert(name.to_ascii_lowercase()) {
            return Err(PlusRefusal::invalid(path, "name"));
        }
        blocked.push(name.to_owned());
    }
    for (i, name) in friends.iter().enumerate() {
        if names.contains(&name.to_ascii_lowercase()) {
            return Err(PlusRefusal::invalid(format!("who.friends[{i}]"), "blocked"));
        }
    }
    // The tag may not name a player the document knows (a friend or a blocked player).
    let known: Vec<String> = friends.iter().chain(blocked.iter()).cloned().collect();
    check_prefix(&prefix_text, &known).map_err(tag_refusal)?;

    let schedule = parse_schedule(config.get("schedule"))?;
    let pause_until = match config.get("pauseUntil") {
        Some(Value::Null) => None,
        other => Some(integer(other, "pauseUntil")?),
    };
    if pause_until.is_some_and(|until| until > now + MAX_PAUSE_SECONDS) {
        return Err(PlusRefusal::invalid("pauseUntil", "range"));
    }
    let pause_until = pause_until.filter(|until| *until > now);

    // The drops, then the counts.
    let progress = &ctx.progress;
    let limits = &ctx.limits;
    let mut all: Vec<Value> = orders;
    all.extend(
        kept.into_iter()
            .filter(|order| int_field(order, "until") > now),
    );
    all.retain(|order| {
        finished_at(order, now, progress).is_none_or(|at| at > now - FINISHED_KEEP_SECONDS)
    });
    let store = limits.order_store as usize;
    if all.len() > store {
        let mut finished: Vec<(i64, String)> = all
            .iter()
            .filter_map(|order| {
                finished_at(order, now, progress).map(|at| {
                    let id = order.get("id").and_then(Value::as_str).unwrap_or("");
                    (at, id.to_owned())
                })
            })
            .collect();
        finished.sort();
        let drop: BTreeSet<String> = finished
            .into_iter()
            .take(all.len() - store)
            .map(|(_, id)| id)
            .collect();
        all.retain(|order| !drop.contains(order.get("id").and_then(Value::as_str).unwrap_or("")));
    }
    if all.len() > store {
        return Err(PlusRefusal::LimitOrders {
            scope: "stored".into(),
            max: limits.order_store,
        });
    }
    let active = all
        .iter()
        .filter(|order| is_active(order, now, progress))
        .count();
    if active > limits.order_max as usize {
        return Err(PlusRefusal::LimitOrders {
            scope: "active".into(),
            max: limits.order_max,
        });
    }
    let shown: HashSet<&str> = all
        .iter()
        .flat_map(|order| {
            ["item", "pay"]
                .into_iter()
                .filter_map(|key| order.get(key).and_then(Value::as_str))
        })
        .filter_map(custom_key_of)
        .collect();
    let items: Vec<Value> = custom_items
        .into_iter()
        .filter(|item| {
            let k = item.get("k").and_then(Value::as_str).unwrap_or("");
            shown.contains(k) || int_field(item, "at") > now - CUSTOM_KEEP_SECONDS
        })
        .collect();
    if items.len() > limits.custom_items_max as usize {
        return Err(PlusRefusal::LimitRows {
            field: "customItems".into(),
            max: limits.custom_items_max,
        });
    }

    let out = serde_json::json!({
        "v": 2,
        "enabled": enabled,
        "mode": mode,
        "lang": lang,
        "persona": {"preset": preset, "intro": intro, "status": status,
            "statusPreset": status_preset, "smallTalk": small_talk, "playerTalk": player_talk,
            "prefix": {"on": prefix_on, "text": prefix_text}},
        "replies": {"scope": scope, "away": away, "saveAt": save_at},
        "authority": authority,
        "allow": {"buy": allow_buy, "sell": allow_sell},
        "pay": pay,
        "who": {"mode": who_mode, "friends": friends},
        "orders": all,
        "customItems": items,
        "limits": {"perDealMax": per_deal_max, "dailySpendMax": daily_spend_max,
            "totalBudget": total_budget, "balanceFloor": balance_floor,
            "approvalAbove": approval_above, "newCounterpartApprovalAbove": new_counterpart,
            "approvalTimeoutMin": timeout},
        "discovery": {"scanChat": scan_chat, "proactivePm": proactive_pm, "ads": ads,
            "adTemplate": ad_template},
        "trade": trade,
        "summaryAt": summary_at,
        "blocked": blocked,
        "schedule": schedule,
        "pauseUntil": pause_until,
    });
    if out.to_string().len() > MAX_CONFIG_BYTES {
        return Err(PlusRefusal::TooLarge);
    }
    Ok(out)
}

fn kind_or_range(value: Option<&Value>) -> &'static str {
    if value.is_none() { "missing" } else { "range" }
}

fn parse_custom_items(value: &Value, now: i64) -> Checked<Vec<Value>> {
    let list = array(Some(value), "customItems")?;
    let mut keys = HashSet::new();
    let mut fps = HashSet::new();
    let mut out = Vec::with_capacity(list.len());
    for (i, entry) in list.iter().enumerate() {
        let field = format!("customItems[{i}]");
        let item = object(Some(entry), &field)?;
        only(item, &CUSTOM_KEYS, &field)?;
        let k = string(item.get("k"), &format!("{field}.k"))?;
        if !is_custom_key(k) || !keys.insert(k.to_owned()) {
            return Err(PlusRefusal::invalid(format!("{field}.k"), "id"));
        }
        let name_field = format!("{field}.name");
        let raw = string(item.get("name"), &name_field)?;
        let text_refusal = |problem: &str| PlusRefusal::Invalid {
            field: name_field.clone(),
            reason: "text".into(),
            problem: Some(problem.into()),
        };
        let name = check_persona(raw, MAX_INTRO).map_err(&text_refusal)?;
        if name.is_empty() || cp(&name) > CUSTOM_NAME_MAX {
            return Err(text_refusal("length"));
        }
        let base = match item.get("base") {
            Some(Value::Null) => Value::Null,
            other => {
                let base = string(other, &format!("{field}.base"))?;
                if !is_vanilla_id(base) {
                    return Err(PlusRefusal::invalid(format!("{field}.base"), "item"));
                }
                Value::String(base.to_owned())
            }
        };
        let fp = match item.get("fp") {
            Some(Value::Null) => Value::Null,
            other => {
                let fp = string(other, &format!("{field}.fp"))?;
                if !is_fingerprint(fp) {
                    return Err(PlusRefusal::invalid(format!("{field}.fp"), "range"));
                }
                if !fps.insert(fp.to_owned()) {
                    return Err(PlusRefusal::invalid(format!("{field}.fp"), "fp"));
                }
                Value::String(fp.to_owned())
            }
        };
        let look = match item.get("look") {
            Some(Value::Null) => Value::Null,
            other => {
                let look_field = format!("{field}.look");
                let look_in = object(other, &look_field)?;
                only(look_in, &["cmd", "model"], &look_field)?;
                let mut look = Map::new();
                if let Some(cmd) = look_in.get("cmd") {
                    let cmd = ranged(Some(cmd), &format!("{look_field}.cmd"), -INT32, INT32 - 1)?;
                    look.insert("cmd".into(), cmd.into());
                }
                if let Some(model) = look_in.get("model") {
                    let model = string(Some(model), &format!("{look_field}.model"))?;
                    if !is_model(model) {
                        return Err(PlusRefusal::invalid(format!("{look_field}.model"), "range"));
                    }
                    look.insert("model".into(), Value::String(model.to_owned()));
                }
                if look.is_empty() {
                    return Err(PlusRefusal::invalid(look_field, "range"));
                }
                Value::Object(look)
            }
        };
        let from = one_of(
            item.get("from"),
            &format!("{field}.from"),
            &["inv", "text", "seen"],
        )?;
        let at = integer(item.get("at"), &format!("{field}.at"))?;
        if at > now {
            return Err(PlusRefusal::invalid(format!("{field}.at"), "range"));
        }
        out.push(serde_json::json!({
            "k": k, "name": name, "base": base, "fp": fp, "look": look, "from": from, "at": at,
        }));
    }
    Ok(out)
}

struct OrderFacts<'a> {
    ctx: &'a PlusCheckContext,
    now: i64,
    pay: &'a str,
    max_deal: u64,
    custom_keys: &'a HashSet<String>,
    stored_orders: &'a [Value],
}

fn parse_order(
    value: &Value,
    field: &str,
    facts: &OrderFacts<'_>,
    seen: &mut HashSet<String>,
) -> Checked<Value> {
    let ctx = facts.ctx;
    let order = object(Some(value), field)?;
    only(order, &ORDER_KEYS, field)?;
    let id = string(order.get("id"), &format!("{field}.id"))?;
    if !is_order_id(id) || !seen.insert(id.to_owned()) {
        return Err(PlusRefusal::invalid(format!("{field}.id"), "id"));
    }
    let on = boolean(order.get("on"), &format!("{field}.on"))?;
    let side = one_of(
        order.get("side"),
        &format!("{field}.side"),
        &["buy", "sell"],
    )?;
    let Some(item) = ref_of(order.get("item"), facts.custom_keys) else {
        let reason = if order.get("item").is_none() {
            "missing"
        } else {
            "item"
        };
        return Err(PlusRefusal::invalid(format!("{field}.item"), reason));
    };
    if item.starts_with("custom:") && ctx.features.custom_items == Some(false) {
        return Err(PlusRefusal::invalid(format!("{field}.item"), "feature"));
    }
    let qty = ranged(
        order.get("qty"),
        &format!("{field}.qty"),
        1,
        MAX_COUNT.into(),
    )?;

    // What pays decides whether there are conditions: on what the bot receives.
    let pay_ref = if order.get("pay").and_then(Value::as_str) == Some("money") {
        "money".to_owned()
    } else {
        match ref_of(order.get("pay"), facts.custom_keys) {
            Some(reference) => reference,
            None => {
                let reason = if order.get("pay").is_none() {
                    "missing"
                } else {
                    "item"
                };
                return Err(PlusRefusal::invalid(format!("{field}.pay"), reason));
            }
        }
    };
    let receives = side == "buy" || pay_ref != "money";
    let req_value = order
        .get("req")
        .ok_or_else(|| PlusRefusal::invalid(format!("{field}.req"), "missing"))?;
    if receives != !req_value.is_null() {
        return Err(PlusRefusal::invalid(format!("{field}.req"), "req"));
    }
    let req = if req_value.is_null() {
        Value::Null
    } else {
        let req_field = format!("{field}.req");
        let req_in = object(Some(req_value), &req_field)?;
        only(req_in, &["ench", "dur", "named", "cursed"], &req_field)?;
        let ench_in = object(req_in.get("ench"), &format!("{req_field}.ench"))?;
        if ench_in.len() > 8 {
            return Err(PlusRefusal::invalid(format!("{req_field}.ench"), "range"));
        }
        let mut ench = Map::new();
        for (key, level) in ench_in {
            let short: String = key.chars().take(74).collect();
            let at = format!("{req_field}.ench.{short}");
            if !is_vanilla_id(key) {
                return Err(PlusRefusal::invalid(at, "item"));
            }
            ench.insert(key.clone(), ranged(Some(level), &at, 1, 10)?.into());
        }
        serde_json::json!({
            "ench": ench,
            "dur": ranged(req_in.get("dur"), &format!("{req_field}.dur"), 0, 100)?,
            "named": boolean(req_in.get("named"), &format!("{req_field}.named"))?,
            "cursed": boolean(req_in.get("cursed"), &format!("{req_field}.cursed"))?,
        })
    };

    if pay_ref != "money" {
        if pay_ref == item {
            return Err(PlusRefusal::invalid(format!("{field}.pay"), "same"));
        }
        if ctx.features.item_pay == Some(false) {
            return Err(PlusRefusal::invalid(format!("{field}.pay"), "feature"));
        }
        if pay_ref.starts_with("custom:") && ctx.features.custom_items == Some(false) {
            return Err(PlusRefusal::invalid(format!("{field}.pay"), "feature"));
        }
        if facts.pay == "money" {
            return Err(PlusRefusal::invalid(format!("{field}.pay"), "pay"));
        }
    } else if facts.pay == "items" {
        return Err(PlusRefusal::invalid(format!("{field}.pay"), "pay"));
    }
    let unit = one_of(
        order.get("unit"),
        &format!("{field}.unit"),
        &["each", "total"],
    )?;
    let top = if pay_ref == "money" {
        i64::try_from(facts.max_deal).unwrap_or(i64::MAX)
    } else {
        MAX_COUNT.into()
    };
    let target = ranged(order.get("target"), &format!("{field}.target"), 1, top)?;
    let limit = ranged(order.get("limit"), &format!("{field}.limit"), 1, top)?;
    if (side == "buy" && target > limit) || (side == "sell" && limit > target) {
        return Err(PlusRefusal::invalid(format!("{field}.limit"), "order"));
    }
    let style = one_of(
        order.get("style"),
        &format!("{field}.style"),
        &["patient", "balanced", "quick"],
    )?;
    let trade = one_of(
        order.get("trade"),
        &format!("{field}.trade"),
        &["auto", "owner"],
    )?;
    let until = integer(order.get("until"), &format!("{field}.until"))?;
    let before = facts
        .stored_orders
        .iter()
        .find(|stored| stored.get("id").and_then(Value::as_str) == Some(id));
    if before.is_none_or(|stored| int_field(stored, "until") != until) {
        let reach = facts.now + i64::from(ctx.limits.order_days_max) * 86_400;
        if until <= facts.now || until > reach {
            return Err(PlusRefusal::invalid(format!("{field}.until"), "until"));
        }
    }
    let note_raw = string(order.get("note"), &format!("{field}.note"))?;
    let note = clean_text(note_raw).replace('\n', " ");
    if cp(&note) > MAX_NOTE {
        return Err(PlusRefusal::invalid(format!("{field}.note"), "range"));
    }
    let src = one_of(
        order.get("src"),
        &format!("{field}.src"),
        &["text", "form", "inv"],
    )?;
    let at = integer(order.get("at"), &format!("{field}.at"))?;
    if at > facts.now {
        return Err(PlusRefusal::invalid(format!("{field}.at"), "range"));
    }
    Ok(serde_json::json!({
        "id": id, "on": on, "side": side, "item": item, "qty": qty, "req": req, "pay": pay_ref,
        "unit": unit, "target": target, "limit": limit, "style": style, "trade": trade,
        "until": until, "note": note, "src": src, "at": at,
    }))
}

fn parse_schedule(value: Option<&Value>) -> Checked<Value> {
    let schedule_in = object(value, "schedule")?;
    only(schedule_in, &["on", "windows"], "schedule")?;
    let on = boolean(schedule_in.get("on"), "schedule.on")?;
    let windows_in = array(schedule_in.get("windows"), "schedule.windows")?;
    if windows_in.len() > MAX_WINDOWS || (on && windows_in.is_empty()) {
        return Err(PlusRefusal::invalid("schedule.windows", "range"));
    }
    let mut windows = Vec::with_capacity(windows_in.len());
    for (i, entry) in windows_in.iter().enumerate() {
        let field = format!("schedule.windows[{i}]");
        let window_in = object(Some(entry), &field)?;
        only(window_in, &["days", "from", "to"], &field)?;
        let days_in = array(window_in.get("days"), &format!("{field}.days"))?;
        if days_in.is_empty() || days_in.len() > 7 {
            return Err(PlusRefusal::invalid(format!("{field}.days"), "range"));
        }
        let mut days = Vec::with_capacity(days_in.len());
        for (j, day) in days_in.iter().enumerate() {
            days.push(ranged(Some(day), &format!("{field}.days[{j}]"), 1, 7)?);
        }
        let distinct: BTreeSet<i64> = days.iter().copied().collect();
        if distinct.len() != days.len() {
            return Err(PlusRefusal::invalid(format!("{field}.days"), "range"));
        }
        let mut times = [String::new(), String::new()];
        for (slot, key) in ["from", "to"].into_iter().enumerate() {
            let time = string(window_in.get(key), &format!("{field}.{key}"))?;
            if !is_time(time) {
                return Err(PlusRefusal::invalid(format!("{field}.{key}"), "range"));
            }
            time.clone_into(&mut times[slot]);
        }
        let [from, to] = times;
        windows.push(serde_json::json!({
            "days": distinct.into_iter().collect::<Vec<_>>(), "from": from, "to": to,
        }));
    }
    Ok(serde_json::json!({ "on": on, "windows": windows }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTORS: &str = include_str!("../tests/vectors/afkplus-config-v2.json");

    /// The shared context with a case's changes over it (top-level keys replace).
    fn context_of(shared: &Value, changes: &Value) -> PlusCheckContext {
        let mut merged = shared.as_object().cloned().unwrap_or_default();
        if let Some(changes) = changes.as_object() {
            for (key, value) in changes {
                merged.insert(key.clone(), value.clone());
            }
        }
        serde_json::from_value(Value::Object(merged)).expect("a vector's context")
    }

    /// The service's refusal in the vectors' words.
    fn as_result(refusal: &PlusRefusal) -> Value {
        match refusal {
            PlusRefusal::Invalid {
                field,
                reason,
                problem,
            } => {
                let mut out = serde_json::json!({"ok": false, "status": 400,
                    "code": "invalid_plus_config", "field": field, "reason": reason});
                if let Some(problem) = problem {
                    out["problem"] = Value::String(problem.clone());
                }
                out
            }
            PlusRefusal::LimitRows { field, max } => serde_json::json!({"ok": false,
                "status": 422, "code": "limit_rows", "field": field, "max": max}),
            PlusRefusal::LimitOrders { scope, max } => serde_json::json!({"ok": false,
                "status": 422, "code": "limit_orders", "scope": scope, "max": max}),
            PlusRefusal::ClientOutdated => {
                serde_json::json!({"ok": false, "status": 409, "code": "client_outdated"})
            }
            PlusRefusal::TooLarge => {
                serde_json::json!({"ok": false, "status": 413, "code": "payload_too_large"})
            }
        }
    }

    /// Cases that hang on the parts of the rule that are the service's: the text rule's links,
    /// contact data and listed words (the order form says them in TypeScript).
    const SERVICE_ONLY: [&str; 3] = [
        "a custom name with a link",
        "a tag naming another app",
        "a tag with a link",
    ];

    #[test]
    fn every_settings_vector_holds() {
        let vectors: Value = serde_json::from_str(VECTORS).unwrap();
        assert_eq!(vectors["maxBytes"], Value::from(MAX_CONFIG_BYTES));
        assert_eq!(
            vectors["finishedKeepSeconds"],
            Value::from(FINISHED_KEEP_SECONDS)
        );
        assert_eq!(
            vectors["customKeepSeconds"],
            Value::from(CUSTOM_KEEP_SECONDS)
        );
        let shared = &vectors["context"];
        let cases = vectors["cases"].as_array().unwrap();
        assert_eq!(
            Some(cases.len() as u64),
            vectors["counts"]["cases"].as_u64()
        );
        let mut ran = 0;
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let ctx = context_of(shared, &case["ctx"]);
            let got = check_config_value(&case["input"], &ctx);
            let want = &case["result"];
            if SERVICE_ONLY.contains(&name) {
                // The launcher lets it pass: the service says no with the field.
                assert!(got.is_ok(), "{name}: {got:?}");
                continue;
            }
            match (&got, want["ok"].as_bool()) {
                (Ok(config), Some(true)) => {
                    let expected = if want["same"] == Value::Bool(true) {
                        &case["input"]
                    } else {
                        &want["config"]
                    };
                    assert_eq!(config, expected, "{name}");
                }
                (Err(refusal), Some(false)) => {
                    assert_eq!(&as_result(refusal), want, "{name}");
                }
                _ => panic!("{name}: got {got:?}, want {want}"),
            }
            ran += 1;
        }
        assert_eq!(ran + SERVICE_ONLY.len(), cases.len());
    }

    #[test]
    fn texts_are_cleaned_as_the_service_keeps_them() {
        assert_eq!(
            clean_text("  Elytra   alırım,\n en fazla 40 bin  "),
            "Elytra alırım,\nen fazla 40 bin"
        );
        assert_eq!(clean_text("a\u{200b}b\r\n\r\nc\td"), "ab\nc d");
        assert_eq!(clean_text("1\n2\n3\n4\n5\n6\n7"), "1\n2\n3\n4\n5 6 7");
        assert_eq!(check_persona("   ", 60), Ok(String::new()));
        assert_eq!(check_persona("[ AFK + ] selam", 60), Err("marker"));
        assert_eq!(check_persona("［ＡＦＫ＋］ selam", 60), Err("marker"));
        for bad in ["/spawn", "&aYeşil", "Merhaba §lkalın", "Selam %username%"] {
            assert_eq!(check_persona(bad, 60), Err("form"), "{bad}");
        }
        assert_eq!(
            check_persona("%50 indirim, 20% kâr", 60),
            Ok("%50 indirim, 20% kâr".into())
        );
        assert_eq!(check_persona("Tom & Jerry", 60), Ok("Tom & Jerry".into()));
        assert_eq!(check_persona(&"y".repeat(61), 60), Err("length"));
        assert_eq!(check_persona(" Yakut ", 32), Ok("Yakut".into()));
    }

    #[test]
    fn money_names_are_checked_as_the_service_checks_them() {
        for good in [
            "TL",
            "coin",
            "ripple",
            "Altın Para",
            "₺",
            "$",
            "Gold.Coin",
            "Elmas-Puan",
            "Şeker_Ç",
        ] {
            assert_eq!(check_currency(good), Ok(good.to_owned()), "{good}");
        }
        assert_eq!(check_currency("  Altın   Para "), Ok("Altın Para".into()));
        assert_eq!(check_currency("   "), Err("length"));
        // A digit is a number to the reply rule; a name needs a letter or a sign.
        for bad in ["TL2", "100 coin", "-", "...", "TL!", "Аltın", "&aCoin", "Coin\nTL"] {
            assert_eq!(check_currency(bad), Err("form"), "{bad}");
        }
        assert_eq!(check_currency(&"x".repeat(16)), Ok("x".repeat(16)));
        assert_eq!(check_currency(&"x".repeat(17)), Err("length"));
    }

    #[test]
    fn the_money_name_is_kept_cleared_or_checked() {
        let base = serde_json::to_value(crate::plus::PlusConfig {
            mode: GENERIC.into(),
            ..crate::plus::PlusConfig::default()
        })
        .unwrap();
        let with = |currency: Value| {
            let mut doc = base.clone();
            doc["trade"]["currency"] = currency;
            doc
        };
        // The stored document names the money "TL"; `on` is Studio's switch.
        let check = |currency: Option<Value>, on: bool| {
            let ctx = PlusCheckContext {
                now: 1_792_900_000,
                stored: Some(with(Value::from("TL"))),
                features: PlusCheckFeatures {
                    currency: Some(on),
                    ..PlusCheckFeatures::default()
                },
                ..PlusCheckContext::default()
            };
            check_config_value(&currency.map_or_else(|| base.clone(), with), &ctx)
        };
        let kept = |out: Checked<Value>| out.unwrap()["trade"].get("currency").cloned();
        let refused = |reason: &str, problem: Option<&str>| -> Checked<Value> {
            Err(PlusRefusal::Invalid {
                field: "trade.currency".into(),
                reason: reason.into(),
                problem: problem.map(str::to_owned),
            })
        };
        let tl = Some(Value::from("TL"));
        // A write without it keeps the stored one; null clears it (the document leaves it out).
        assert_eq!(kept(check(None, true)), tl);
        assert_eq!(kept(check(Some(Value::Null), true)), None);
        let spaced = Some(Value::from(" Altın  Para "));
        assert_eq!(kept(check(spaced, true)), Some(Value::from("Altın Para")));
        assert_eq!(check(Some(Value::from("TL2")), true), refused("text", Some("form")));
        assert_eq!(check(Some(Value::from(5)), true), refused("type", None));
        // Studio's switch off: a new name is refused; the stored one comes back, and clearing it
        // works.
        assert_eq!(check(Some(Value::from("coin")), false), refused("feature", None));
        assert_eq!(kept(check(tl.clone(), false)), tl);
        assert_eq!(kept(check(Some(Value::Null), false)), None);
    }

    #[test]
    fn ids_and_references() {
        assert!(is_vanilla_id("minecraft:diamond_block"));
        assert!(!is_vanilla_id("minecraft:Diamond"));
        assert!(!is_vanilla_id("diamond"));
        assert!(!is_vanilla_id("projects:ruby"));
        assert!(is_custom_key("c1") && is_custom_key("cz9a") && !is_custom_key("c12345"));
        assert!(!is_custom_key("x1") && !is_custom_key("c"));
        assert!(is_order_id("a1b2c3d4") && !is_order_id("a1b2c3d4e") && !is_order_id("A1"));
        assert!(is_model("projects:item/ruby") && !is_model("ruby") && !is_model("a:B"));
        assert!(is_fingerprint("0123456789abcdef0123456789abcdef"));
        assert!(!is_fingerprint("0123456789ABCDEF0123456789abcdef"));
    }
}
