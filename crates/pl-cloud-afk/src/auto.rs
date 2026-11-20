//! Auto-fishing and auto-mining (the contract's "Auto-fishing and auto-mining", FISHMINE): a cloud
//! account fishes, or mines the blocks that appear in front of it (a cobblestone generator), where
//! it stands, with a few idle movements while mining. Only cloud accounts (plain and AFK+), never
//! desktop or phone ones. What the launcher holds: what an account may do
//! (`Account.features.fishing` / `.mining`), its own settings (`config.fishing` / `config.mining`,
//! read with the contract's defaults and always sent whole), the live frames (`autost`,
//! `autores`) and the shape of an operation, checked before it leaves. The Worker and the machine
//! check everything again and have the last word.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::model::{CloudEvery, CloudLocalized, CloudModPaused};

/// The blocks mining may break (`fixed.miningBlocks`: stone kinds and ores), in the contract's
/// order; a player's choice is a subset of it, in this order.
pub const MINING_BLOCKS: &[&str] = &[
    "minecraft:cobblestone",
    "minecraft:stone",
    "minecraft:cobbled_deepslate",
    "minecraft:deepslate",
    "minecraft:andesite",
    "minecraft:diorite",
    "minecraft:granite",
    "minecraft:tuff",
    "minecraft:basalt",
    "minecraft:smooth_basalt",
    "minecraft:blackstone",
    "minecraft:netherrack",
    "minecraft:end_stone",
    "minecraft:obsidian",
    "minecraft:coal_ore",
    "minecraft:deepslate_coal_ore",
    "minecraft:copper_ore",
    "minecraft:deepslate_copper_ore",
    "minecraft:iron_ore",
    "minecraft:deepslate_iron_ore",
    "minecraft:gold_ore",
    "minecraft:deepslate_gold_ore",
    "minecraft:redstone_ore",
    "minecraft:deepslate_redstone_ore",
    "minecraft:lapis_ore",
    "minecraft:deepslate_lapis_ore",
    "minecraft:diamond_ore",
    "minecraft:deepslate_diamond_ore",
    "minecraft:emerald_ore",
    "minecraft:deepslate_emerald_ore",
    "minecraft:nether_gold_ore",
    "minecraft:nether_quartz_ore",
    "minecraft:ancient_debris",
];

/// A rod or a pickaxe is put away with this much durability left, at least and at most.
pub const AUTO_KEEP_MIN: u32 = 1;
pub const AUTO_KEEP_MAX: u32 = 100;
const KEEP_DEFAULT: u32 = 5;
/// The idle movements' random wait, seconds (`idle.every`), at least and at most.
pub const AUTO_IDLE_MIN_SECS: u32 = 20;
pub const AUTO_IDLE_MAX_SECS: u32 = 3_600;
const IDLE_EVERY_DEFAULT: CloudEvery = CloudEvery { min: 60, max: 180 };
/// An aim's yaw (−180 to 180: 0 south, 90 west, ±180 north, −90 east) and pitch (−90 up to 90
/// down), the game's own.
const YAW_MAX: f64 = 180.0;
const PITCH_MAX: f64 = 90.0;
/// What a full inventory does (`full.do`).
const FULL_ACTIONS: &[&str] = &["wait", "keep", "command"];

/// `Account.features.fishing` and `.mining`: Studio's switch while the account's machine can do
/// the mode (else `off`: nothing of it shows), the account's own setting, why it cannot be turned
/// on now, and the public reason when the account's server is limited for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudAutoFeature {
    /// `off` or `on`.
    pub mode: String,
    /// `config.<mode>.enabled`.
    pub on: bool,
    /// Only while `mode` is on and `on` false: `player` (the player's own number of accounts with
    /// the mode is reached) or `host` (the machine's slots are taken); the switch waits.
    pub slot: Option<String>,
    /// The account's server is limited for this mode: the public reason. The switch shows disabled
    /// with it (also while `on` is true: the setting stays, the mode does not run there).
    pub limited: Option<CloudLocalized>,
}

impl Default for CloudAutoFeature {
    fn default() -> Self {
        Self {
            mode: "off".into(),
            on: false,
            slot: None,
            limited: None,
        }
    }
}

impl CloudAutoFeature {
    /// Studio has the mode on for this account (its block and panel may show).
    pub fn offered(&self) -> bool {
        self.mode == "on"
    }
}

/// A rod's or a pickaxe's care (`rod`, `tool`): it is put away with `keep` durability left (1–100;
/// it never breaks), and with `switch` another one from the inventory (the hotbar first) is taken
/// instead of waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudToolCare {
    pub switch: bool,
    pub keep: u32,
}

impl Default for CloudToolCare {
    fn default() -> Self {
        Self {
            switch: true,
            keep: KEEP_DEFAULT,
        }
    }
}

/// What happens when the inventory has no room for what comes next (`full`): `wait` (until there
/// is room), `keep` (what does not fit falls to the ground) or `command` (the machine types
/// `command` at most once a minute, then waits).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudWhenFull {
    #[serde(rename = "do")]
    pub action: String,
    /// Only with `command`: plain words under the text rules, never a secret.
    pub command: Option<String>,
    /// The view's mark that a moderation case paused the command (the machine waits meanwhile, as
    /// with `wait`): never stored, dropped when the setting is written back; changing the command
    /// or `do` has it read again (FISHMINE-W `9b7aad4f`).
    pub paused: Option<CloudModPaused>,
}

impl Default for CloudWhenFull {
    fn default() -> Self {
        Self {
            action: "wait".into(),
            command: None,
            paused: None,
        }
    }
}

/// The idle movements while mining ("Arada hareket etsin"): a small turn of the head and back, a
/// moment of sneaking, a jump, one step back and again (only on a safe floor), once in a random
/// time within `every` (seconds, 20 ≤ min ≤ max ≤ 3600).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudIdleMoves {
    pub look: bool,
    pub sneak: bool,
    pub jump: bool,
    pub step: bool,
    pub every: CloudEvery,
}

impl Default for CloudIdleMoves {
    fn default() -> Self {
        Self {
            look: false,
            sneak: false,
            jump: false,
            step: false,
            every: IDLE_EVERY_DEFAULT,
        }
    }
}

/// `config.fishing`, read with the contract's defaults for what the service left out: the form
/// the view, the editor and the change all use (a change carries it whole). Its parts' defaults
/// are their own (`CloudToolCare`, `CloudWhenFull`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudFishingSetting {
    /// The player's own choice.
    pub enabled: bool,
    pub rod: CloudToolCare,
    pub full: CloudWhenFull,
    /// A small turn between casts.
    pub look: bool,
}

/// `config.mining`, read with the contract's defaults for what the service left out.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudMiningSetting {
    pub enabled: bool,
    /// None: every block of [`MINING_BLOCKS`]; else the player's subset of it.
    pub blocks: Option<Vec<String>>,
    pub tool: CloudToolCare,
    pub full: CloudWhenFull,
    pub idle: CloudIdleMoves,
}

/// `full` as the service takes it: `command` only with `command` (refused otherwise).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WhenFullInput {
    #[serde(rename = "do")]
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl From<&CloudWhenFull> for WhenFullInput {
    fn from(full: &CloudWhenFull) -> Self {
        let command = full.action == "command";
        Self {
            action: full.action.clone(),
            command: if command {
                full.command.as_deref().map(|c| c.trim().to_owned())
            } else {
                None
            },
        }
    }
}

/// `config.fishing` as a change carries it: whole, every part written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FishingInput {
    pub enabled: bool,
    pub rod: CloudToolCare,
    pub full: WhenFullInput,
    pub look: bool,
}

impl From<&CloudFishingSetting> for FishingInput {
    fn from(setting: &CloudFishingSetting) -> Self {
        Self {
            enabled: setting.enabled,
            rod: setting.rod,
            full: WhenFullInput::from(&setting.full),
            look: setting.look,
        }
    }
}

/// `config.mining` as a change carries it: whole, the blocks in the contract's order (all of them
/// left out).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MiningInput {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<String>>,
    pub tool: CloudToolCare,
    pub full: WhenFullInput,
    pub idle: CloudIdleMoves,
}

impl From<&CloudMiningSetting> for MiningInput {
    fn from(setting: &CloudMiningSetting) -> Self {
        Self {
            enabled: setting.enabled,
            blocks: setting.blocks.as_ref().and_then(|chosen| {
                let ordered: Vec<String> = MINING_BLOCKS
                    .iter()
                    .filter(|block| chosen.iter().any(|c| c == *block))
                    .map(|block| (*block).to_owned())
                    .collect();
                // Every block chosen is the same as none: all of them.
                (ordered.len() < MINING_BLOCKS.len()).then_some(ordered)
            }),
            tool: setting.tool,
            full: WhenFullInput::from(&setting.full),
            idle: setting.idle,
        }
    }
}

/// Where a setting's problem is (the field of a `general` issue) and what it is.
pub(crate) type AutoProblem = (&'static str, &'static str);

fn care_problem(care: &CloudToolCare) -> Option<&'static str> {
    (!(AUTO_KEEP_MIN..=AUTO_KEEP_MAX).contains(&care.keep)).then_some("range")
}

fn full_problem(full: &CloudWhenFull) -> Option<&'static str> {
    if !FULL_ACTIONS.contains(&full.action.as_str()) {
        return Some("format");
    }
    if full.action != "command" {
        return None;
    }
    let command = full.command.as_deref().unwrap_or("").trim();
    if command.is_empty() {
        return Some("empty");
    }
    if let Err(problem) = crate::text::check_text(command) {
        return Some(problem.as_str());
    }
    // Typed by the machine as it is, stored plain: never a password or another secret.
    crate::text::looks_sensitive(command).then_some("secret")
}

/// The problems of the fishing setting (fields `fishing.rod`, `fishing.full`).
pub(crate) fn fishing_problems(setting: &CloudFishingSetting) -> Vec<AutoProblem> {
    let mut problems = Vec::new();
    if let Some(problem) = care_problem(&setting.rod) {
        problems.push(("fishing.rod", problem));
    }
    if let Some(problem) = full_problem(&setting.full) {
        problems.push(("fishing.full", problem));
    }
    problems
}

/// The problems of the mining setting (fields `mining.blocks`, `mining.tool`, `mining.full`,
/// `mining.idle`).
pub(crate) fn mining_problems(setting: &CloudMiningSetting) -> Vec<AutoProblem> {
    let mut problems = Vec::new();
    if let Some(blocks) = &setting.blocks {
        let mut seen = std::collections::HashSet::new();
        if blocks.is_empty() {
            problems.push(("mining.blocks", "empty"));
        } else if !blocks.iter().all(|b| MINING_BLOCKS.contains(&b.as_str())) {
            problems.push(("mining.blocks", "format"));
        } else if !blocks.iter().all(|b| seen.insert(b.as_str())) {
            problems.push(("mining.blocks", "duplicate"));
        }
    }
    if let Some(problem) = care_problem(&setting.tool) {
        problems.push(("mining.tool", problem));
    }
    if let Some(problem) = full_problem(&setting.full) {
        problems.push(("mining.full", problem));
    }
    let CloudEvery { min, max } = setting.idle.every;
    if min > max {
        problems.push(("mining.idle", "order"));
    } else if min < AUTO_IDLE_MIN_SECS || max > AUTO_IDLE_MAX_SECS {
        problems.push(("mining.idle", "range"));
    }
    problems
}

// --- The live socket ----------------------------------------------------------------------------

/// A count as the machine wrote it; anything but a whole, non-negative number reads as 0 (a
/// fraction is cut), so one odd field never costs the frame.
fn count<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.0)
        .map_or(0, |n| n.min(f64::from(u32::MAX)) as u32))
}

/// A time (Unix seconds) or a coordinate as the machine wrote it; none for anything but a number.
fn whole<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_i64().or_else(|| {
        value
            .as_f64()
            .filter(|n| n.is_finite() && n.abs() < 9.0e15)
            .map(|n| n.floor() as i64)
    }))
}

/// A rate as the machine wrote it; none for anything but a finite, non-negative number.
fn rate<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_f64().filter(|n| n.is_finite() && *n >= 0.0))
}

/// A rod or a pickaxe in hand: its namespaced id, durability left and maximum.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudAutoTool {
    pub item: String,
    #[serde(deserialize_with = "count")]
    #[specta(type = u32)]
    pub dur: u32,
    #[serde(deserialize_with = "count")]
    #[specta(type = u32)]
    pub max: u32,
}

/// One kind of item and how many today.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudAutoCount {
    pub id: String,
    #[serde(deserialize_with = "count")]
    #[specta(type = u32)]
    pub n: u32,
}

/// One of today's last catches: the item, its category (`fish`, `treasure` or `junk`), whether it
/// is enchanted, when (Unix seconds).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudCatch {
    pub id: String,
    pub cat: String,
    pub ench: Option<bool>,
    #[serde(deserialize_with = "whole")]
    #[specta(type = Option<i64>)]
    pub at: Option<i64>,
}

/// Today's fishing (since 00:00 Europe/Istanbul, kept by the machine across restarts).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudFishToday {
    #[serde(deserialize_with = "count")]
    #[specta(type = u32)]
    pub catches: u32,
    #[serde(deserialize_with = "count")]
    #[specta(type = u32)]
    pub fish: u32,
    #[serde(deserialize_with = "count")]
    #[specta(type = u32)]
    pub treasure: u32,
    #[serde(deserialize_with = "count")]
    #[specta(type = u32)]
    pub junk: u32,
    /// Catches per hour of fishing time today.
    #[serde(deserialize_with = "rate")]
    #[specta(type = Option<f64>)]
    pub per_hour: Option<f64>,
    /// By kind, most first (at most 12).
    pub items: Vec<CloudAutoCount>,
    /// Newest first (at most 8).
    pub last: Vec<CloudCatch>,
}

/// What fishing does (`autost`'s `fishing`): `off` with why (`service`, `setting`, `restart`,
/// `slots`, `version`, `limited`, `not_running`), `on` with what (`cast`, `wait`, `reel`),
/// `paused` (`player` and `moved` wait for the player's "Devam et"; `window` and `trade` end by
/// themselves) or `waiting` (`full`, `no_rod`, `rod_low`, `no_water`: it goes on by itself).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudFishState {
    pub state: String,
    pub reason: Option<String>,
    /// When this state began (Unix seconds).
    #[serde(deserialize_with = "whole")]
    #[specta(type = Option<i64>)]
    pub since: Option<i64>,
    pub doing: Option<String>,
    pub rod: Option<CloudAutoTool>,
    pub today: CloudFishToday,
}

/// The block mining looks at (its coordinates go only to the player's own watchers).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudMineTarget {
    pub id: String,
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

/// Today's mining.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudMineToday {
    #[serde(deserialize_with = "count")]
    #[specta(type = u32)]
    pub blocks: u32,
    #[serde(deserialize_with = "rate")]
    #[specta(type = Option<f64>)]
    pub per_minute: Option<f64>,
    /// Blocks broken by kind, most first (at most 12).
    pub items: Vec<CloudAutoCount>,
}

/// What mining does (`autost`'s `mining`): as fishing, `waiting` with `full`, `no_tool`,
/// `tool_low` or `no_target`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudMineState {
    pub state: String,
    pub reason: Option<String>,
    #[serde(deserialize_with = "whole")]
    #[specta(type = Option<i64>)]
    pub since: Option<i64>,
    pub target: Option<CloudMineTarget>,
    pub tool: Option<CloudAutoTool>,
    pub today: CloudMineToday,
}

const MAX_ITEMS: usize = 12;
const MAX_LAST: usize = 8;

/// A mode's state from its part of an `autost` frame; none when that part is missing or out of
/// shape (the other mode still reads).
pub(crate) fn fish_state(value: Option<Value>) -> Option<CloudFishState> {
    let mut state: CloudFishState = serde_json::from_value(value?).ok()?;
    state.today.items.truncate(MAX_ITEMS);
    state.today.last.truncate(MAX_LAST);
    Some(state)
}

pub(crate) fn mine_state(value: Option<Value>) -> Option<CloudMineState> {
    let mut state: CloudMineState = serde_json::from_value(value?).ok()?;
    state.today.items.truncate(MAX_ITEMS);
    Some(state)
}

/// Why an operation was refused (`autores.error`): `invalid` (`reason`, `field`), `rate_limited`,
/// `host_offline`, `not_running`, `auto_off` (`reason`: `service`, `setting`, `restart`, `slots`,
/// `version`, `limited`), `busy_trade`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudAutoError {
    pub code: String,
    pub reason: Option<String>,
    pub field: Option<String>,
}

impl CloudAutoError {
    pub fn new(code: &str) -> Self {
        Self {
            code: code.to_owned(),
            ..Self::default()
        }
    }

    pub(crate) fn invalid(reason: &str, field: Option<&str>) -> Self {
        Self {
            code: "invalid".into(),
            reason: Some(reason.to_owned()),
            field: field.map(str::to_owned),
        }
    }
}

const AUTO_MODES: &[&str] = &["fishing", "mining"];

/// The fields an action takes besides `action`, all of them required.
fn auto_fields(action: &str) -> Option<&'static [&'static str]> {
    match action {
        "pause" | "resume" => Some(&["mode"]),
        "aim" => Some(&["yaw", "pitch"]),
        _ => None,
    }
}

/// The shape of an operation as the app wrote it (`{"t":"auto"}`'s `op`; the contract's
/// `AutoOp`), checked before it leaves the launcher.
pub fn check_auto_op(op: &Value) -> Result<(), CloudAutoError> {
    let Some(object) = op.as_object() else {
        return Err(CloudAutoError::invalid("type", None));
    };
    let Some((action, fields)) = object
        .get("action")
        .and_then(Value::as_str)
        .and_then(|action| Some((action, auto_fields(action)?)))
    else {
        return Err(CloudAutoError::invalid("action", None));
    };
    if let Some(extra) = object
        .keys()
        .find(|key| *key != "action" && !fields.contains(&key.as_str()))
    {
        return Err(CloudAutoError::invalid("field", Some(extra)));
    }
    if let Some(missing) = fields.iter().find(|key| !object.contains_key(**key)) {
        return Err(CloudAutoError::invalid(missing, None));
    }
    let within = |key: &str, max: f64| {
        object
            .get(key)
            .and_then(Value::as_f64)
            .is_some_and(|n| n.is_finite() && (-max..=max).contains(&n))
    };
    if action == "aim" {
        if !within("yaw", YAW_MAX) {
            return Err(CloudAutoError::invalid("yaw", None));
        }
        if !within("pitch", PITCH_MAX) {
            return Err(CloudAutoError::invalid("pitch", None));
        }
    } else if !object
        .get("mode")
        .and_then(Value::as_str)
        .is_some_and(|mode| AUTO_MODES.contains(&mode))
    {
        return Err(CloudAutoError::invalid("mode", None));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! The contract's shared cases, copied from projects-studio `fishmine-w` `9b7aad4f`
    //! (FISHMINE-W): `services/cloud-afk/test-vectors/auto-op-v1.json`. Copy it again when the contract
    //! changes it.

    use super::*;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../tests/vectors/auto-op-v1.json")).unwrap()
    }

    #[test]
    fn every_shared_operation_shape() {
        let all = vectors();
        let cases = all["shape"].as_array().unwrap();
        assert_eq!(cases.len(), 31);
        for case in cases {
            let got = check_auto_op(&case["op"]).err().map(|e| {
                (
                    e.code,
                    e.reason.unwrap_or_default(),
                    e.field.unwrap_or_default(),
                )
            });
            let want = case["problem"].as_object().map(|p| {
                (
                    "invalid".to_owned(),
                    p["reason"].as_str().unwrap().to_owned(),
                    p.get("field")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                )
            });
            assert_eq!(got, want, "{}", case["name"]);
        }
    }

    #[test]
    fn every_shared_account_view_reads_back_as_sent() {
        let all = vectors();
        let cases = all["view"].as_array().unwrap();
        assert_eq!(cases.len(), 22);
        for case in cases {
            let expected = &case["expected"];
            let read: CloudAutoFeature = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(
                &serde_json::to_value(&read).unwrap(),
                expected,
                "{}",
                case["name"]
            );
        }
        // A service before the modes sends nothing: nothing of them shows.
        let none: CloudAutoFeature = serde_json::from_str("{}").unwrap();
        assert!(!none.offered() && !none.on && none.slot.is_none() && none.limited.is_none());
    }

    #[test]
    fn the_fixed_rules_and_defaults_are_the_contract_s() {
        let settings = &vectors()["settings"];
        let fixed: Vec<&str> = settings["fixed"]["miningBlocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b.as_str().unwrap())
            .collect();
        assert_eq!(fixed, MINING_BLOCKS);
        let defaults = &settings["configDefaults"];
        let care: CloudToolCare = serde_json::from_value(defaults["tool"].clone()).unwrap();
        assert_eq!(care, CloudToolCare::default());
        let idle: CloudIdleMoves = serde_json::from_value(defaults["idle"].clone()).unwrap();
        assert_eq!(idle, CloudIdleMoves::default());
        assert_eq!(defaults["full"], serde_json::json!({"do": "wait"}));
        assert_eq!(defaults["blocks"], Value::Null);
        assert_eq!(defaults["look"], false);
        // Every machine refusal code and every auto_off reason this launcher words.
        let autores = &settings["autores"];
        let codes: Vec<&str> = autores["codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        assert_eq!(
            codes,
            [
                "invalid",
                "rate_limited",
                "host_offline",
                "not_running",
                "auto_off",
                "busy_trade"
            ]
        );
    }

    /// A stored form as the launcher sends it back: blocks every one of the list is the same as all
    /// (null), and a full inventory's command goes trimmed.
    fn sent_as_stored(mode: &str, value: &Value) -> Value {
        if mode == "fishing" {
            let read: CloudFishingSetting = serde_json::from_value(value.clone()).unwrap();
            return serde_json::to_value(FishingInput::from(&read)).unwrap();
        }
        let read: CloudMiningSetting = serde_json::from_value(value.clone()).unwrap();
        let mut sent = serde_json::to_value(MiningInput::from(&read)).unwrap();
        if sent.get("blocks").is_none() {
            sent["blocks"] = Value::Null;
        }
        sent
    }

    #[test]
    fn every_shared_stored_setting_reads_and_goes_back_whole() {
        let all = vectors();
        let cases = all["config"].as_array().unwrap();
        assert_eq!(cases.len(), 60);
        for case in cases
            .iter()
            .filter(|c| c["expected"].get("stored").is_some())
        {
            let mode = case["mode"].as_str().unwrap();
            let mut stored = case["expected"]["stored"].clone();
            // Every block of the list named is all of them.
            if stored["blocks"]
                .as_array()
                .is_some_and(|b| b.len() == MINING_BLOCKS.len())
            {
                stored["blocks"] = Value::Null;
            }
            assert_eq!(
                sent_as_stored(mode, &case["value"]),
                stored,
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn every_shared_problem_the_editor_can_make_is_found_before_saving() {
        // The editor holds typed settings: a wrong type, a missing or an unknown part never leaves
        // it. What it can hold is checked here as the Worker would refuse it (the part named), or
        // put right before it goes (blocks in the list's order, a command trimmed).
        let all = vectors();
        let mut checked = 0;
        for case in all["config"].as_array().unwrap() {
            let Some(error) = case["expected"].get("error") else {
                continue;
            };
            let mode = case["mode"].as_str().unwrap();
            let field = error["field"].as_str().unwrap();
            let reason = error["reason"].as_str().unwrap();
            let typed = error["code"] == "invalid_command" || reason == "range";
            if !typed {
                continue;
            }
            let problems = if mode == "fishing" {
                let Ok(read) = serde_json::from_value::<CloudFishingSetting>(case["value"].clone())
                else {
                    continue;
                };
                fishing_problems(&read)
            } else {
                let Ok(read) = serde_json::from_value::<CloudMiningSetting>(case["value"].clone())
                else {
                    continue;
                };
                // Out of the list's order goes out in its order: never refused.
                if case["name"] == "blocks out of the list's order" {
                    let sent = serde_json::to_value(MiningInput::from(&read)).unwrap();
                    assert_eq!(
                        sent["blocks"],
                        serde_json::json!(["minecraft:cobblestone", "minecraft:stone"])
                    );
                    continue;
                }
                mining_problems(&read)
            };
            // A command's leading space is trimmed before it goes.
            if reason == "space" {
                assert!(problems.is_empty(), "{}", case["name"]);
                continue;
            }
            let part: Vec<&str> = field.split('.').take(2).collect();
            let part = part.join(".");
            assert!(
                problems.iter().any(|(at, _)| *at == part),
                "{}: {problems:?}",
                case["name"]
            );
            checked += 1;
        }
        assert!(checked >= 15, "{checked}");
    }

    #[test]
    fn a_change_sends_the_command_only_with_command_and_the_blocks_in_order() {
        let mut fishing = CloudFishingSetting {
            full: CloudWhenFull {
                action: "wait".into(),
                command: Some("/sell all".into()),
                paused: Some(CloudModPaused {
                    case: "mc_1".into(),
                    category: "advertising".into(),
                }),
            },
            ..CloudFishingSetting::default()
        };
        let sent = serde_json::to_value(FishingInput::from(&fishing)).unwrap();
        assert_eq!(sent["full"], serde_json::json!({"do": "wait"}));
        fishing.full.action = "command".into();
        fishing.full.command = Some(" /sell all ".into());
        let sent = serde_json::to_value(FishingInput::from(&fishing)).unwrap();
        assert_eq!(
            sent["full"],
            serde_json::json!({"do": "command", "command": "/sell all"})
        );
        // Every block chosen is all of them: left out.
        let mining = CloudMiningSetting {
            blocks: Some(MINING_BLOCKS.iter().map(|b| (*b).to_owned()).collect()),
            ..CloudMiningSetting::default()
        };
        let sent = serde_json::to_value(MiningInput::from(&mining)).unwrap();
        assert!(sent.get("blocks").is_none());
    }

    #[test]
    fn frames_read_leniently() {
        let state = fish_state(Some(serde_json::json!({
            "state": "on", "doing": "wait", "since": 1_793_000_000,
            "rod": {"item": "minecraft:fishing_rod", "dur": 52, "max": 64},
            "today": {"catches": 31, "fish": 24, "treasure": 2, "junk": 5, "perHour": 11.4,
                      "items": [{"id": "minecraft:cod", "n": 17.0}],
                      "last": [{"id": "minecraft:name_tag", "cat": "treasure", "at": 1_793_000_100}],
                      "new": true},
            "later": 1
        })))
        .unwrap();
        assert_eq!(state.today.per_hour, Some(11.4));
        assert_eq!(state.today.items[0].n, 17);
        assert_eq!(state.rod.unwrap().dur, 52);
        // An odd number reads as 0 or none, never costing the frame.
        let state = mine_state(Some(serde_json::json!({
            "state": "waiting", "reason": "no_target", "since": "soon",
            "today": {"blocks": -3, "perMinute": "fast", "items": []}
        })))
        .unwrap();
        assert_eq!(
            (state.today.blocks, state.today.per_minute, state.since),
            (0, None, None)
        );
        // The Worker's own switch-off has no today and no since.
        let state = fish_state(Some(
            serde_json::json!({"state": "off", "reason": "service"}),
        ))
        .unwrap();
        assert_eq!((state.today.catches, state.since), (0, None));
        // Lists keep their contract's length.
        let many: Vec<Value> = (0..20)
            .map(|n| serde_json::json!({"id": format!("minecraft:item_{n}"), "n": 1}))
            .collect();
        let state = mine_state(Some(
            serde_json::json!({"state": "on", "today": {"items": many}}),
        ))
        .unwrap();
        assert_eq!(state.today.items.len(), 12);
        assert_eq!(fish_state(None), None);
        assert_eq!(fish_state(Some(serde_json::json!("on"))), None);
    }
}
