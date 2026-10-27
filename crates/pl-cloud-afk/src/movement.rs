//! Movement (the contract's "Movement", CAFK-FEAT §3.7): the player walks a cloud account by hand
//! (a few steps, a jump, sneaking, a walk to coordinates, next to or after one of their own
//! accounts), within a few movement slots per machine. What the launcher holds: what the account
//! may do (`Account.features.movement`), its own switch (`config.movement`), the live frames
//! (`movst`, `pos`, `movres`) and the shape of an operation, checked before it leaves. The Worker
//! and the machine check everything again and have the last word.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::feat::safe_int;
use crate::plus::is_player_name;

/// A step goes 1 to this many blocks.
pub const MOVE_STEP_MAX: u32 = 8;
/// A walk to coordinates goes at most this far (the straight distance to the block's centre).
pub const MOVE_GOTO_MAX_BLOCKS: u32 = 256;
const COORD_MAX: i64 = 30_000_000;
const Y_MIN: i64 = -2048;
const Y_MAX: i64 = 2048;

/// Whom the account may walk to or follow (`Account.features.movement.follow`): the player's
/// verified main name (only on a server whose login proves names), and their other cloud accounts
/// on the same machine and server, in the list's order. Never a name typed by anyone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudMovementFollow {
    pub main: Option<String>,
    pub own: Vec<String>,
}

/// `Account.features.movement`: Studio's switch while the account's machine can move it (else
/// `off`: nothing of movement shows), the account's own setting, why it cannot be turned on now,
/// and whom it may walk to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudMovementFeature {
    /// `off` or `on`.
    pub mode: String,
    /// `config.movement.enabled`: the "Hareket" panel shows.
    pub on: bool,
    /// Only while `mode` is on and `on` false: `player` (another of the player's accounts has
    /// movement on) or `host` (the machine's movement slots are taken); the switch waits.
    pub slot: Option<String>,
    pub follow: CloudMovementFollow,
}

impl Default for CloudMovementFeature {
    fn default() -> Self {
        Self {
            mode: "off".into(),
            on: false,
            slot: None,
            follow: CloudMovementFollow::default(),
        }
    }
}

/// The account's own movement choice (`config.movement`; absent: off).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudMovementSetting {
    pub enabled: bool,
}

/// A block's coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudBlock {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

/// `movst`: whether the account can move now (`state` `ok`) and what it does, or why it cannot
/// (`off` with `reason`: `not_running`, `setting`, `restart`, `slots`, `version`, `service`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudMovementState {
    pub state: String,
    pub reason: Option<String>,
    /// With `ok`: `idle`, `step`, `goto`, `approach` or `follow`.
    pub doing: Option<String>,
    /// `goto`: the target block.
    pub to: Option<CloudBlock>,
    /// `approach` and `follow`: the account it walks to.
    pub name: Option<String>,
    /// `goto`, `approach`, `follow`: when the machine stops it by itself (Unix seconds).
    pub until: Option<i64>,
    /// `idle`: how the last activity ended by itself (`arrived`, `blocked`, `no_path`, `timeout`,
    /// `limit`, `out_of_sight`, `reset`, `service`, `trade`); none after a stop.
    pub ended: Option<String>,
    pub sneak: Option<bool>,
    /// `trade` while an AFK+ deal is open: everything but `stop` waits.
    pub lock: Option<String>,
}

/// `pos`: the block the account stands in, its dimension (`minecraft:overworld`, …, or a server's
/// own) and where it looks (`north`, `south`, `east`, `west`: what the direction buttons are
/// relative to).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudPos {
    pub x: i64,
    pub y: i64,
    pub z: i64,
    pub dim: String,
    pub facing: String,
}

/// Why an operation was refused (`movres.error`): `invalid` (`reason`, `field`), `movement_off`
/// (`reason`), `not_running`, `busy_trade`, `too_far`, `not_allowed`, `not_visible`, `blocked`,
/// `loop` and `rate_limited` (`retryAfter` seconds), `stale`, `not_found`, `host_offline`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudMovementError {
    pub code: String,
    pub reason: Option<String>,
    pub field: Option<String>,
    pub retry_after: Option<u32>,
}

impl CloudMovementError {
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
            retry_after: None,
        }
    }
}

const MOVE_ACTIONS: &[&str] = &[
    "step", "jump", "sneak", "goto", "approach", "follow", "stop",
];
const MOVE_DIRS: &[&str] = &["forward", "back", "left", "right"];

/// The fields an action takes besides `action`, all of them required.
fn move_fields(action: &str) -> &'static [&'static str] {
    match action {
        "step" => &["dir", "n"],
        "sneak" => &["on"],
        "goto" => &["x", "y", "z"],
        "approach" | "follow" => &["name"],
        _ => &[],
    }
}

/// The shape of an operation as the app wrote it (`{"t":"move"}`'s `op`; the contract's `MoveOp`,
/// vectors `move-op-v1.json` → `shape`), checked before it leaves the launcher.
pub fn check_move_op(op: &Value) -> Result<(), CloudMovementError> {
    let Some(object) = op.as_object() else {
        return Err(CloudMovementError::invalid("type", None));
    };
    let Some(action) = object
        .get("action")
        .and_then(Value::as_str)
        .filter(|a| MOVE_ACTIONS.contains(a))
    else {
        return Err(CloudMovementError::invalid("action", None));
    };
    let fields = move_fields(action);
    if let Some(extra) = object
        .keys()
        .find(|key| *key != "action" && !fields.contains(&key.as_str()))
    {
        return Err(CloudMovementError::invalid("field", Some(extra)));
    }
    if let Some(missing) = fields.iter().find(|key| !object.contains_key(**key)) {
        return Err(CloudMovementError::invalid(missing, None));
    }
    let int = |key: &str| object.get(key).and_then(safe_int);
    match action {
        "step" => {
            if !object
                .get("dir")
                .and_then(Value::as_str)
                .is_some_and(|dir| MOVE_DIRS.contains(&dir))
            {
                return Err(CloudMovementError::invalid("dir", None));
            }
            if !int("n").is_some_and(|n| (1..=i64::from(MOVE_STEP_MAX)).contains(&n)) {
                return Err(CloudMovementError::invalid("n", None));
            }
        }
        "sneak" if !object.get("on").is_some_and(Value::is_boolean) => {
            return Err(CloudMovementError::invalid("on", None));
        }
        "goto" => {
            if !int("x").is_some_and(|x| x.abs() <= COORD_MAX) {
                return Err(CloudMovementError::invalid("x", None));
            }
            if !int("y").is_some_and(|y| (Y_MIN..=Y_MAX).contains(&y)) {
                return Err(CloudMovementError::invalid("y", None));
            }
            if !int("z").is_some_and(|z| z.abs() <= COORD_MAX) {
                return Err(CloudMovementError::invalid("z", None));
            }
        }
        "approach" | "follow"
            if !object
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(is_player_name) =>
        {
            return Err(CloudMovementError::invalid("name", None));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! The contract's shared cases, copied from projects-studio `services/cloud-afk/test-vectors/`
    //! (`main` `5158bdb`, MOVE-K): `move-op-v1.json`. Copy it again when the contract changes it.

    use super::*;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../tests/vectors/move-op-v1.json")).unwrap()
    }

    #[test]
    fn every_shared_operation_shape() {
        let all = vectors();
        let cases = all["shape"].as_array().unwrap();
        assert_eq!(cases.len(), 45);
        for case in cases {
            let got = check_move_op(&case["op"]).err().map(|e| {
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
        assert_eq!(cases.len(), 9);
        for case in cases {
            let expected = &case["expected"];
            let read: CloudMovementFeature = serde_json::from_value(expected.clone()).unwrap();
            assert_eq!(
                &serde_json::to_value(&read).unwrap(),
                expected,
                "{}",
                case["name"]
            );
        }
        // A service before movement sends nothing: nothing of it shows.
        let none: CloudMovementFeature = serde_json::from_str("{}").unwrap();
        assert_eq!(none.mode, "off");
        assert!(!none.on && none.slot.is_none() && none.follow.own.is_empty());
    }

    #[test]
    fn the_fixed_rules_are_the_contract_s() {
        let fixed = &vectors()["settings"]["fixed"];
        assert_eq!(fixed["stepMax"], u64::from(MOVE_STEP_MAX));
        assert_eq!(fixed["gotoMaxBlocks"], u64::from(MOVE_GOTO_MAX_BLOCKS));
        assert_eq!(fixed["coordMax"], COORD_MAX);
        assert_eq!(fixed["yMin"], Y_MIN);
        assert_eq!(fixed["yMax"], Y_MAX);
    }

    #[test]
    fn frames_read_with_unknown_fields_and_values() {
        let state: CloudMovementState = serde_json::from_str(
            r#"{"state":"ok","doing":"follow","name":"Kaan_07","until":1793000000,"sneak":false,"new":1}"#,
        )
        .unwrap();
        assert_eq!(state.doing.as_deref(), Some("follow"));
        assert_eq!(state.until, Some(1_793_000_000));
        let pos: CloudPos = serde_json::from_str(
            r#"{"x":-12,"y":64,"z":30,"dim":"minecraft:the_nether","facing":"west"}"#,
        )
        .unwrap();
        assert_eq!((pos.x, pos.y, pos.z), (-12, 64, 30));
        let error: CloudMovementError =
            serde_json::from_str(r#"{"code":"loop","retryAfter":240}"#).unwrap();
        assert_eq!(error.retry_after, Some(240));
    }
}
