//! A server's dialog as the desktop console's card (AFK-UX: DIALOG-CARD on this computer).
//!
//! A client 4.2.4 started with the console kinds lists `caps: ["dialog"]` in its `connecting`
//! event and reports the open dialog as one event, `{"e":"dialog","rev","dialog":{…}}`
//! (`"clear":true` once it closed); a dialog the card cannot show as the game does stays plain
//! console lines. Here the dialog is checked before the card may show it ([`check`]): its size,
//! every button's value against the console's click rules, its input keys, and the account's
//! secrets (a dialog with a string the mask changes is not shown). A press ([`press_line`]) is
//! checked against the dialog the card showed: its `rev`, a button it has, its inputs' keys and
//! values, a command button only with the command the player confirmed. It goes to the client as
//! `/dialog act {json}`, which checks it all again, runs a command only when it is exactly the
//! button's command filled in from the inputs, and answers with a `dialogres` event (the client
//! fork's `docs/agent-bridge.md`, "Client 4.2.4: dialog cards"). The view checks the dialog again
//! too (`@pl/ui` `safeDialog`). Nothing of it reaches an AI: this computer runs no AFK+ agent.
//!
//! Client 4.2.5 (CONSOLE-ANY) reports them beside its classic console too (side events), shows the
//! game's "waiting for response" screen in a dialog's place ([`ShownDialog::waiting`], left with a
//! close of its revision), and says what the game's pause screen and quick actions key open
//! ([`check_menus`]; a press `{"open": "pause" | "quick"}` opens one, answered by `menures`).

use serde_json::{Map, Value};

use crate::looks::{click_fits, is_forbidden, units};

/// Most bytes of a dialog's JSON (the client's cap; 4.2.5's).
pub const MAX_DIALOG_BYTES: usize = 16 * 1024;
/// Most characters of a press's JSON (the client's).
pub const MAX_PRESS_CHARS: usize = 4096;
const MAX_BUTTONS: usize = 64;
const MAX_INPUTS: usize = 16;
/// Most bytes of the menus' JSON (two labels with their looks).
pub const MAX_MENUS_BYTES: usize = 2 * 1024;
/// The menus a client 4.2.5 reports: the pause screen's and the quick actions key's.
const MENUS: [&str; 2] = ["pause", "quick"];
/// A label's most UTF-16 code units (the client's).
const MAX_LABEL_UNITS: usize = 64;
/// Longest input value, in UTF-16 code units.
const MAX_INPUT_UNITS: usize = 256;
/// How deep the dialog's JSON goes (title, body parts, inputs' options and their looks).
const MAX_DEPTH: usize = 8;

const BUTTON_ACTIONS: [&str; 7] = [
    "run_command",
    "open_url",
    "suggest_command",
    "copy_to_clipboard",
    "custom",
    "dialog",
    "close",
];

/// The dialog the card shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownDialog {
    /// The client's revision of it (every new dialog counts up).
    pub rev: u32,
    /// Its JSON as checked here (empty for the waiting screen).
    pub json: String,
    /// The game's "waiting for response" screen in a dialog's place (client 4.2.5): no buttons,
    /// only its close.
    pub wait: bool,
    /// Each button's index and action, for the presses.
    buttons: Vec<(i64, String)>,
    /// The inputs' keys.
    inputs: Vec<String>,
}

impl ShownDialog {
    /// The game's waiting screen of revision `rev` (client 4.2.5).
    pub fn waiting(rev: i64) -> Option<Self> {
        Some(Self {
            rev: u32::try_from(rev).ok()?,
            json: String::new(),
            wait: true,
            buttons: Vec::new(),
            inputs: Vec::new(),
        })
    }
}

/// The menus of a client 4.2.5 `menus` event as the card may offer them: `pause` and `quick`,
/// each with a label of at most 64 code units outside the rule list that the account's mask leaves
/// as it is (its looks go along; the view checks them). Their JSON, or none when none is left.
pub fn check_menus(menus: &Map<String, Value>, mask: &dyn Fn(&str) -> String) -> Option<String> {
    let mut out = Map::new();
    for name in MENUS {
        let Some(menu) = menus.get(name).and_then(Value::as_object) else {
            continue;
        };
        let Some(label) = menu.get("label").and_then(Value::as_str) else {
            continue;
        };
        if label.is_empty()
            || units(label) > MAX_LABEL_UNITS
            || label.chars().any(is_forbidden)
            || mask(label) != label
        {
            continue;
        }
        let mut shown = Map::new();
        shown.insert("label".into(), Value::from(label));
        if let Some(looks) = menu.get("labelSt") {
            shown.insert("labelSt".into(), looks.clone());
        }
        out.insert(name.to_owned(), Value::Object(shown));
    }
    if out.is_empty() {
        return None;
    }
    let json = Value::Object(out).to_string();
    (json.len() <= MAX_MENUS_BYTES).then_some(json)
}

/// What goes to the client for a press, and the command it may run (from then on masked in the
/// console if it carries a password).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PressLine {
    pub line: String,
    pub command: Option<String>,
}

/// `^[A-Za-z0-9_]{1,32}$` (the client's input keys).
fn is_input_key(key: &str) -> bool {
    (1..=32).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Every string of the value (keys aside) as the mask leaves it.
fn unmasked(value: &Value, mask: &dyn Fn(&str) -> String, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    match value {
        Value::String(text) => mask(text) == *text,
        Value::Array(items) => items.iter().all(|item| unmasked(item, mask, depth + 1)),
        Value::Object(fields) => fields
            .values()
            .all(|field| unmasked(field, mask, depth + 1)),
        _ => true,
    }
}

/// The client's dialog when the card may show it: an object of at most [`MAX_DIALOG_BYTES`], every
/// string of it left as it is by the account's mask, at most 64 buttons of the known actions with
/// distinct indexes (a value only as the console's click rules allow it), at most 16 inputs with
/// the client's keys, each once.
pub fn check(rev: i64, dialog: &Value, mask: &dyn Fn(&str) -> String) -> Option<ShownDialog> {
    let rev = u32::try_from(rev).ok()?;
    let fields = dialog.as_object()?;
    let json = serde_json::to_string(dialog).ok()?;
    if json.len() > MAX_DIALOG_BYTES || !unmasked(dialog, mask, 0) {
        return None;
    }
    let raw_buttons = fields.get("buttons")?.as_array()?;
    if raw_buttons.len() > MAX_BUTTONS {
        return None;
    }
    let mut buttons: Vec<(i64, String)> = Vec::with_capacity(raw_buttons.len());
    for button in raw_buttons {
        let button = button.as_object()?;
        let index = button.get("i")?.as_i64()?;
        let action = button.get("a")?.as_str()?;
        if index < 0
            || !BUTTON_ACTIONS.contains(&action)
            || buttons.iter().any(|(other, _)| *other == index)
        {
            return None;
        }
        match button.get("v") {
            // A command button's template follows the click rules as written; the filled command
            // is checked again at the press.
            Some(value) if BUTTON_ACTIONS[..4].contains(&action) => {
                if !click_fits(action, value.as_str()?) {
                    return None;
                }
            }
            None if !BUTTON_ACTIONS[..4].contains(&action) => {}
            _ => return None,
        }
        buttons.push((index, action.to_owned()));
    }
    let raw_inputs = fields.get("inputs")?.as_array()?;
    if raw_inputs.len() > MAX_INPUTS {
        return None;
    }
    let mut inputs: Vec<String> = Vec::with_capacity(raw_inputs.len());
    for input in raw_inputs {
        let key = input.as_object()?.get("key")?.as_str()?;
        if !is_input_key(key) || inputs.iter().any(|other| other == key) {
            return None;
        }
        inputs.push(key.to_owned());
    }
    Some(ShownDialog {
        rev,
        json,
        wait: false,
        buttons,
        inputs,
    })
}

/// A press of the card (`{rev, i, inputs?, cmd?}` or `{rev, cancel: true}`, as `@pl/ui`
/// `dialogPress` builds it) checked against the dialog it was made on (`shown`), as the client's
/// `/dialog act` line; client 4.2.5's `{open: "pause" | "quick"}` only for a menu it offers
/// (`menus`: [`check_menus`]'s JSON). `Err` with the client's own word for the refusal: `line`
/// (not a press), `stale` (another dialog now, or none), `button`, `input`, `cmd`, `menu`.
pub fn press_line(
    press: &str,
    shown: Option<&ShownDialog>,
    menus: Option<&str>,
) -> Result<PressLine, &'static str> {
    if press.len() > MAX_PRESS_CHARS {
        return Err("line");
    }
    let value: Value = serde_json::from_str(press).map_err(|_| "line")?;
    let fields = value.as_object().ok_or("line")?;
    if let Some(open) = fields.get("open") {
        let menu = open
            .as_str()
            .filter(|menu| MENUS.contains(menu))
            .ok_or("line")?;
        let offered = menus
            .and_then(|json| serde_json::from_str::<Value>(json).ok())
            .is_some_and(|menus| menus.get(menu).is_some());
        if !offered {
            return Err("menu");
        }
        let mut out = Map::new();
        out.insert("open".into(), Value::from(menu));
        return Ok(PressLine {
            line: act_line(out),
            command: None,
        });
    }
    let shown = shown.ok_or("stale")?;
    let rev = fields.get("rev").and_then(Value::as_u64).ok_or("line")?;
    if rev != u64::from(shown.rev) {
        return Err("stale");
    }
    let mut out = Map::new();
    out.insert("rev".into(), Value::from(rev));
    if fields.get("cancel") == Some(&Value::Bool(true)) {
        out.insert("cancel".into(), Value::Bool(true));
        return Ok(PressLine {
            line: act_line(out),
            command: None,
        });
    }
    // The waiting screen has no buttons, only its close.
    if shown.wait {
        return Err("stale");
    }
    let index = fields.get("i").and_then(Value::as_i64).ok_or("line")?;
    let (_, action) = shown
        .buttons
        .iter()
        .find(|(other, _)| *other == index)
        .ok_or("button")?;
    out.insert("i".into(), Value::from(index));
    if let Some(inputs) = fields.get("inputs") {
        let inputs = inputs.as_object().ok_or("line")?;
        if inputs.len() > MAX_INPUTS {
            return Err("input");
        }
        for (key, value) in inputs {
            // A multiline text may hold `\n`; the client checks each input's own rule again.
            let value = value.as_str().ok_or("input")?;
            if !shown.inputs.contains(key)
                || units(value) > MAX_INPUT_UNITS
                || value.chars().any(|c| c != '\n' && is_forbidden(c))
            {
                return Err("input");
            }
        }
        out.insert("inputs".into(), Value::Object(inputs.clone()));
    }
    if action != "run_command" {
        return Ok(PressLine {
            line: act_line(out),
            command: None,
        });
    }
    let command = fields
        .get("cmd")
        .and_then(Value::as_str)
        .filter(|command| click_fits("run_command", command))
        .ok_or("cmd")?;
    out.insert("cmd".into(), Value::from(command));
    Ok(PressLine {
        line: act_line(out),
        command: Some(command.to_owned()),
    })
}

/// `/dialog act {json}`: one line (JSON escapes line breaks), `%` written as `%` since the
/// client expands `%name%` in the lines it reads.
fn act_line(press: Map<String, Value>) -> String {
    let json = Value::Object(press).to_string().replace('%', "\\u0025");
    format!("/dialog act {json}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn same(text: &str) -> String {
        text.to_owned()
    }

    fn rules() -> Value {
        json!({
            "title": "Sunucu Kuralları",
            "body": [{ "text": "1. Saygılı ol", "st": [[13, "e"]] }],
            "inputs": [
                { "key": "name", "kind": "text", "label": "Klan adı", "max": 16, "multiline": false, "initial": "" },
                { "key": "ok", "kind": "bool", "label": "Kabul", "initial": false, "onTrue": "evet", "onFalse": "hayir" }
            ],
            "buttons": [
                { "i": 0, "label": "Kur", "a": "run_command", "v": "/klan kur $(name)" },
                { "i": 1, "label": "Site", "a": "open_url", "v": "https://projects.gg" },
                { "i": 2, "label": "Kapat", "a": "close" }
            ],
            "cancel": true,
            "play": true,
            "cols": 2
        })
    }

    #[test]
    fn a_dialog_shows_only_as_the_rules_allow() {
        let shown = check(3, &rules(), &same).unwrap();
        assert_eq!(shown.rev, 3);
        assert_eq!(shown.buttons.len(), 3);
        assert_eq!(shown.inputs, vec!["name".to_owned(), "ok".to_owned()]);

        let mut bad = rules();
        bad["buttons"][0]["v"] = json!("klan kur");
        assert_eq!(check(3, &bad, &same), None, "a command without /");
        let mut bad = rules();
        bad["buttons"][1]["v"] = json!("https://127.0.0.1/");
        assert_eq!(check(3, &bad, &same), None, "an address outside the rule");
        let mut bad = rules();
        bad["buttons"][2]["v"] = json!("x");
        assert_eq!(check(3, &bad, &same), None, "a value on a close button");
        let mut bad = rules();
        bad["buttons"][2]["a"] = json!("open_file");
        assert_eq!(check(3, &bad, &same), None, "an action it does not know");
        let mut bad = rules();
        bad["buttons"][2]["i"] = json!(0);
        assert_eq!(check(3, &bad, &same), None, "an index twice");
        let mut bad = rules();
        bad["inputs"][1]["key"] = json!("name");
        assert_eq!(check(3, &bad, &same), None, "a key twice");
        let mut bad = rules();
        bad["inputs"][1]["key"] = json!("bad key");
        assert_eq!(check(3, &bad, &same), None, "a key outside the client's");
        let mut big = rules();
        big["title"] = json!("a".repeat(MAX_DIALOG_BYTES));
        assert_eq!(check(3, &big, &same), None, "too large");
        assert_eq!(check(-1, &rules(), &same), None);
        assert_eq!(check(3, &json!("Kurallar"), &same), None);

        // A string the account's mask changes: the dialog is not shown.
        let mask = |text: &str| text.replace("Saygılı", "******");
        assert_eq!(check(3, &rules(), &mask), None);
    }

    #[test]
    fn a_press_is_checked_against_the_dialog_it_was_made_on() {
        let shown = check(3, &rules(), &same).unwrap();
        let line = press_line(
            r#"{"rev":3,"i":0,"inputs":{"name":"Ejder","ok":"true"},"cmd":"/klan kur Ejder"}"#,
            Some(&shown),
            None,
        )
        .unwrap();
        assert_eq!(line.command.as_deref(), Some("/klan kur Ejder"));
        assert!(line.line.starts_with("/dialog act {"));
        let sent: Value = serde_json::from_str(&line.line["/dialog act ".len()..]).unwrap();
        assert_eq!(
            sent,
            json!({ "rev": 3, "i": 0, "inputs": { "name": "Ejder", "ok": "true" }, "cmd": "/klan kur Ejder" })
        );

        let link = press_line(r#"{"rev":3,"i":1,"inputs":{}}"#, Some(&shown), None).unwrap();
        assert_eq!(link.command, None);
        let cancel = press_line(r#"{"rev":3,"cancel":true}"#, Some(&shown), None).unwrap();
        assert_eq!(cancel.command, None);
        assert_eq!(
            serde_json::from_str::<Value>(&cancel.line["/dialog act ".len()..]).unwrap(),
            json!({ "rev": 3, "cancel": true })
        );

        for (press, why) in [
            (r#"{"rev":2,"i":0}"#, "stale"),
            (r#"{"rev":3,"i":9}"#, "button"),
            (r#"{"rev":3,"i":0}"#, "cmd"),
            (r#"{"rev":3,"i":0,"cmd":"klan kur"}"#, "cmd"),
            // JSON escapes: a direction override and a zero-width space.
            ("{\"rev\":3,\"i\":0,\"cmd\":\"/klan\\u202ekur\"}", "cmd"),
            (r#"{"rev":3,"i":1,"inputs":{"other":"x"}}"#, "input"),
            (
                "{\"rev\":3,\"i\":1,\"inputs\":{\"name\":\"a\\u200bb\"}}",
                "input",
            ),
            (r#"{"rev":3,"i":1,"inputs":{"name":1}}"#, "input"),
            (r#"{"rev":3,"i":1,"inputs":[]}"#, "line"),
            (r#"{"i":1}"#, "line"),
            ("not json", "line"),
        ] {
            assert_eq!(press_line(press, Some(&shown), None), Err(why), "{press}");
        }
        assert_eq!(
            press_line(
                &format!("{{\"rev\":3,\"i\":1,\"x\":\"{}\"}}", "a".repeat(5000)),
                Some(&shown),
                None
            ),
            Err("line")
        );
    }

    #[test]
    fn a_percent_never_reaches_the_client_s_line_as_it_is() {
        let shown = check(7, &rules(), &same).unwrap();
        let line = press_line(
            r#"{"rev":7,"i":0,"inputs":{"name":"%100"},"cmd":"/klan kur %100"}"#,
            Some(&shown),
            None,
        )
        .unwrap();
        assert!(!line.line.contains('%'));
        assert!(line.line.contains("\\u0025100"));
        // The client reads it back as the confirmed command.
        let sent: Value = serde_json::from_str(&line.line["/dialog act ".len()..]).unwrap();
        assert_eq!(sent["cmd"], json!("/klan kur %100"));
        // A multiline text keeps its line break, escaped on the one line.
        let shown = check(8, &rules(), &same).unwrap();
        let line = press_line(
            r#"{"rev":8,"i":2,"inputs":{"name":"a\nb"}}"#,
            Some(&shown),
            None,
        )
        .unwrap();
        assert!(!line.line.contains('\n'));
    }
    #[test]
    fn client_4_2_5_s_waiting_screen_and_menus() {
        // The waiting screen takes only its close.
        let waiting = ShownDialog::waiting(4).unwrap();
        let close = press_line(r#"{"rev":4,"cancel":true}"#, Some(&waiting), None).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&close.line["/dialog act ".len()..]).unwrap(),
            json!({ "rev": 4, "cancel": true })
        );
        assert_eq!(
            press_line(r#"{"rev":4,"i":0}"#, Some(&waiting), None),
            Err("stale")
        );
        assert_eq!(
            press_line(r#"{"rev":4,"cancel":true}"#, None, None),
            Err("stale")
        );

        // Menus: checked labels, the account's mask, only the two the game has.
        let menus = json!({
            "pause": { "label": "Kurallar", "labelSt": [[8, "6"]] },
            "quick": { "label": "şifre hunter22" },
            "other": { "label": "x" }
        });
        let mask = |text: &str| text.replace("hunter22", "******");
        let checked = check_menus(menus.as_object().unwrap(), &mask).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&checked).unwrap(),
            json!({ "pause": { "label": "Kurallar", "labelSt": [[8, "6"]] } })
        );
        assert_eq!(check_menus(&Map::new(), &same), None);

        // Opening one: only a menu the client offered.
        let open = press_line(r#"{"open":"pause"}"#, None, Some(&checked)).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&open.line["/dialog act ".len()..]).unwrap(),
            json!({ "open": "pause" })
        );
        assert_eq!(open.command, None);
        assert_eq!(
            press_line(r#"{"open":"quick"}"#, None, Some(&checked)),
            Err("menu")
        );
        assert_eq!(press_line(r#"{"open":"pause"}"#, None, None), Err("menu"));
        assert_eq!(
            press_line(r#"{"open":"inventory"}"#, None, Some(&checked)),
            Err("line")
        );
    }
}
