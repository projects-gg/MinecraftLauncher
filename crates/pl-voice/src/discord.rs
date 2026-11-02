//! Discord's push-to-talk, mute and deafen keys on this computer (VOICE-KEYS), offered once as the
//! player first opens voice chat (the first room or Settings → Sesli sohbet): "Discord kısayolların
//! kopyalansın mı?". Read only when the page asks, never at the launcher's start; nothing leaves
//! the computer.
//!
//! Discord's desktop app keeps them in its Chromium localStorage (LevelDB, `%APPDATA%\discord\
//! Local Storage\leveldb`; PTB and Canary beside it), under its web origin:
//! - `keybinds`: `{"_state": {"<id>": {"action": "TOGGLE_MUTE", "shortcut": [[0, 162, 1], [0, 77,
//!   1]], "enabled": true, …}}, "_version": n}`; a bind counts when it is enabled and its shortcut
//!   not empty. Actions: `PUSH_TO_TALK`, `TOGGLE_MUTE`, `TOGGLE_DEAFEN`.
//! - `MediaEngineStore`: `{"default": {"mode": "PUSH_TO_TALK" | "VOICE_ACTIVITY", "modeOptions":
//!   {"shortcut": […]}}}`; the managed push-to-talk bind mirrors that shortcut while the mode is
//!   push-to-talk.
//!
//! A shortcut's parts are `[type, code, platform or device]`: type 0 a key (2 a modifier, the same
//! to us), 1 a mouse button (Windows: 0 left, 1 right, 2 middle, 3 back, 4 forward), 3 a gamepad;
//! a key's code is a Windows virtual-key code, left and right modifiers apart (160–165). A virtual
//! key becomes our key by this keyboard's own mapping (its scan code, so the same physical key, and
//! the character on it), which the launcher passes in ([`KeyMap`]).

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::keys::{ScanCode, code_of, usable};
use crate::leveldb;
use crate::settings::{VoiceKey, VoiceModifier, VoiceMouseButton};
use crate::view::VoiceShortcut;

/// Discord's desktop apps: their folder under the roaming app data, and their web origin.
const CHANNELS: &[(&str, &str)] = &[
    ("discord", "https://discord.com"),
    ("discordptb", "https://ptb.discord.com"),
    ("discordcanary", "https://canary.discord.com"),
];

/// Discord's shortcuts as ours (`voice_discord_offer`): the card lists those set; copying fills
/// ours with them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceDiscordOffer {
    pub push_to_talk: Option<VoiceKey>,
    pub mute: Option<VoiceKey>,
    pub deafen: Option<VoiceKey>,
    /// Discord's microphone works by push-to-talk: copying turns "Bas-konuş" on too.
    pub push_to_talk_mode: bool,
    /// Shortcuts Discord has that cannot be used here (a gamepad, the Windows key, two keys at
    /// once, push-to-talk with Ctrl…): the card's one line names them.
    pub skipped: Vec<VoiceShortcut>,
}

impl VoiceDiscordOffer {
    /// Nothing that could be copied.
    pub fn is_empty(&self) -> bool {
        self.push_to_talk.is_none() && self.mute.is_none() && self.deafen.is_none()
    }
}

/// This keyboard's mapping of a Windows virtual-key code: the key's scan code and the character
/// written on it (empty for keys that write none). None: a key the keyboard does not have.
pub type KeyMap<'a> = &'a dyn Fn(u16) -> Option<(ScanCode, String)>;

/// Discord's keys under `roaming` (`%APPDATA%`): the stable app first, then PTB, then Canary, the
/// first with anything to copy. None: no Discord here, or nothing that could be copied.
pub fn read(roaming: &Path, map: KeyMap) -> Option<VoiceDiscordOffer> {
    for (folder, origin) in CHANNELS {
        let dir = roaming.join(folder).join("Local Storage").join("leveldb");
        if !dir.is_dir() {
            continue;
        }
        let binds_key = storage_key(origin, "keybinds");
        let media_key = storage_key(origin, "MediaEngineStore");
        let found = leveldb::newest(&dir, &[&binds_key, &media_key]);
        let json = |key: &Vec<u8>| -> Option<Value> {
            serde_json::from_str(&storage_value(found.get(key)?)?).ok()
        };
        if let Some(offer) = offer(json(&binds_key).as_ref(), json(&media_key).as_ref(), map) {
            return Some(offer);
        }
    }
    None
}

/// A localStorage key in Chromium's database: `_`, the origin, a zero, then the key in Latin-1.
fn storage_key(origin: &str, key: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(origin.len() + key.len() + 3);
    out.push(b'_');
    out.extend_from_slice(origin.as_bytes());
    out.extend_from_slice(&[0, 1]);
    out.extend_from_slice(key.as_bytes());
    out
}

/// A localStorage value: 1 then Latin-1, or 0 then UTF-16 (little-endian).
fn storage_value(bytes: &[u8]) -> Option<String> {
    match bytes.split_first()? {
        (1, rest) => Some(rest.iter().map(|&b| char::from(b)).collect()),
        (0, rest) if rest.len() % 2 == 0 => {
            let (pairs, _) = rest.as_chunks::<2>();
            let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
            String::from_utf16(&units).ok()
        }
        _ => None,
    }
}

/// What Discord's two stores hold, as an offer; None when nothing could be copied.
pub fn offer(
    binds: Option<&Value>,
    media: Option<&Value>,
    map: KeyMap,
) -> Option<VoiceDiscordOffer> {
    let state = binds.map(|binds| binds.get("_state").unwrap_or(binds));
    let voice = media.and_then(|media| media.get("default"));
    let push_to_talk_mode = voice
        .and_then(|voice| voice.get("mode"))
        .and_then(Value::as_str)
        == Some("PUSH_TO_TALK");
    let mut result = VoiceDiscordOffer {
        push_to_talk_mode,
        ..VoiceDiscordOffer::default()
    };
    for (shortcut, action) in [
        (VoiceShortcut::PushToTalk, "PUSH_TO_TALK"),
        (VoiceShortcut::Mute, "TOGGLE_MUTE"),
        (VoiceShortcut::Deafen, "TOGGLE_DEAFEN"),
    ] {
        let mut parts = bound(state, action);
        // Push-to-talk set in Discord's voice settings, before its bind was made.
        if parts.is_none() && shortcut == VoiceShortcut::PushToTalk && push_to_talk_mode {
            parts = voice
                .and_then(|voice| voice.pointer("/modeOptions/shortcut"))
                .and_then(Value::as_array)
                .filter(|parts| !parts.is_empty());
        }
        let Some(parts) = parts else {
            continue;
        };
        match key_of(parts, shortcut, map) {
            Some(key) => match shortcut {
                VoiceShortcut::PushToTalk => result.push_to_talk = Some(key),
                VoiceShortcut::Mute => result.mute = Some(key),
                VoiceShortcut::Deafen => result.deafen = Some(key),
            },
            None => result.skipped.push(shortcut),
        }
    }
    // As the settings keep them: a key two would share stays with the first.
    if let (Some(ptt), Some(mute)) = (&result.push_to_talk, &result.mute)
        && ptt.overlaps(mute)
    {
        result.mute = None;
        result.skipped.push(VoiceShortcut::Mute);
    }
    if let Some(deafen) = &result.deafen
        && [&result.push_to_talk, &result.mute]
            .into_iter()
            .flatten()
            .any(|earlier| earlier.overlaps(deafen))
    {
        result.deafen = None;
        result.skipped.push(VoiceShortcut::Deafen);
    }
    (!result.is_empty()).then_some(result)
}

/// The shortcut of `action`'s first bind in use (by id), if any.
fn bound<'a>(state: Option<&'a Value>, action: &str) -> Option<&'a Vec<Value>> {
    let mut binds: Vec<(u64, &Vec<Value>)> = state?
        .as_object()?
        .iter()
        .filter_map(|(id, bind)| {
            let shortcut = bind.get("shortcut")?.as_array()?;
            let used = bind.get("action")?.as_str()? == action
                && bind.get("enabled").and_then(Value::as_bool) != Some(false)
                && !shortcut.is_empty();
            used.then(|| (id.parse().unwrap_or(u64::MAX), shortcut))
        })
        .collect();
    binds.sort_by_key(|(id, _)| *id);
    binds.first().map(|(_, shortcut)| *shortcut)
}

/// Shift, Ctrl and Alt as virtual-key codes (either side; 16–18 are side-less, a browser's).
fn modifier(vk: u16) -> Option<VoiceModifier> {
    match vk {
        16 | 160 | 161 => Some(VoiceModifier::Shift),
        17 | 162 | 163 => Some(VoiceModifier::Ctrl),
        18 | 164 | 165 => Some(VoiceModifier::Alt),
        _ => None,
    }
}

/// A keyboard key by its virtual-key code, as this keyboard has it.
fn keyboard(vk: u16, map: KeyMap) -> Option<VoiceKey> {
    let (scan, label) = map(vk)?;
    Some(VoiceKey::keyboard(code_of(scan)?, label))
}

/// One of Discord's shortcuts as ours, if `shortcut` can use it: one key or mouse button with the
/// modifiers held with it, or a modifier alone.
fn key_of(parts: &[Value], shortcut: VoiceShortcut, map: KeyMap) -> Option<VoiceKey> {
    let mut mods = Vec::new();
    let mut lone: Option<u16> = None;
    let mut main: Option<VoiceKey> = None;
    for part in parts {
        let part = part.as_array()?;
        let kind = part.first()?.as_u64()?;
        let code = u16::try_from(part.get(1)?.as_u64()?).ok()?;
        // Recorded on macOS (2) or Linux (3): their codes are not Windows keys.
        if matches!(part.get(2).and_then(Value::as_u64), Some(2 | 3)) {
            return None;
        }
        let next = match kind {
            0 | 2 => {
                if let Some(held) = modifier(code) {
                    mods.push(held);
                    lone = Some(code);
                    continue;
                }
                keyboard(code, map)?
            }
            1 => VoiceKey::mouse(match code {
                2 => VoiceMouseButton::Middle,
                3 => VoiceMouseButton::Back,
                4 => VoiceMouseButton::Forward,
                _ => return None,
            }),
            _ => return None,
        };
        // Two keys at once: not something a shortcut here can be.
        if main.replace(next).is_some() {
            return None;
        }
    }
    let key = match main {
        Some(key) => key.with_mods(&mods),
        None if mods.len() == 1 => keyboard(lone?, map)?,
        None => return None,
    };
    usable(&key, shortcut).then_some(key)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::keys::scan_code;
    use crate::leveldb::fixture;

    /// A US keyboard's mapping for the keys the tests use.
    fn us(vk: u16) -> Option<(ScanCode, String)> {
        let (code, label) = match vk {
            0x41..=0x5A => {
                let letter = char::from(vk as u8);
                return Some((scan_code(&format!("Key{letter}"))?, letter.to_string()));
            }
            0x30..=0x39 => {
                let digit = char::from(vk as u8);
                return Some((scan_code(&format!("Digit{digit}"))?, digit.to_string()));
            }
            0x6A => ("NumpadMultiply", "*"),
            0x77 => ("F8", ""),
            160 => ("ShiftLeft", ""),
            161 => ("ShiftRight", ""),
            162 => ("ControlLeft", ""),
            163 => ("ControlRight", ""),
            164 => ("AltLeft", ""),
            0xC0 => ("Backquote", "`"),
            // The Windows key: a scan code the launcher has no name for.
            91 => {
                return Some((
                    ScanCode {
                        make: 0x5B,
                        extended: true,
                    },
                    String::new(),
                ));
            }
            _ => return None,
        };
        Some((scan_code(code)?, label.to_owned()))
    }

    fn binds(state: Value) -> Value {
        json!({"_state": state, "_version": 5})
    }

    #[test]
    fn discords_binds_become_ours() {
        let binds = binds(json!({
            "1": {"id": "1", "action": "SAVE_CLIP", "shortcut": [[0, 164, 1], [0, 67, 1]], "enabled": true},
            "7": {"id": "7", "action": "TOGGLE_MUTE", "shortcut": [[0, 162, 1], [0, 160, 1], [0, 77, 1]], "enabled": true},
            "14": {"id": "14", "action": "TOGGLE_DEAFEN", "shortcut": [[0, 106, "0:0"]], "enabled": true, "managed": false},
            "20": {"id": "20", "action": "PUSH_TO_TALK", "shortcut": [[1, 3, 1]], "enabled": true}
        }));
        let offer = offer(Some(&binds), None, &us).unwrap();
        assert_eq!(
            offer.mute,
            Some(
                VoiceKey::keyboard("KeyM", "M")
                    .with_mods(&[VoiceModifier::Ctrl, VoiceModifier::Shift])
            )
        );
        assert_eq!(
            offer.deafen,
            Some(VoiceKey::keyboard("NumpadMultiply", "*"))
        );
        assert_eq!(
            offer.push_to_talk,
            Some(VoiceKey::mouse(VoiceMouseButton::Back))
        );
        assert!(!offer.push_to_talk_mode);
        assert!(offer.skipped.is_empty());
    }

    #[test]
    fn push_to_talk_comes_from_the_voice_settings_too() {
        let media = json!({
            "default": {"mode": "PUSH_TO_TALK", "modeOptions": {"shortcut": [[0, 192, 1]], "delay": 20}},
            "stream": {"mode": "VOICE_ACTIVITY", "modeOptions": {"shortcut": []}}
        });
        let offer = offer(None, Some(&media), &us).unwrap();
        assert_eq!(
            offer.push_to_talk,
            Some(VoiceKey::keyboard("Backquote", "`"))
        );
        assert!(offer.push_to_talk_mode);
        // Voice activity: a push-to-talk key left from before is not in use.
        let idle = json!({"default": {"mode": "VOICE_ACTIVITY", "modeOptions": {"shortcut": [[0, 192, 1]]}}});
        assert_eq!(super::offer(None, Some(&idle), &us), None);
    }

    #[test]
    fn what_cannot_be_used_here_is_named_and_left_out() {
        let binds = binds(json!({
            // Disabled, empty, or another action: not counted.
            "2": {"action": "TOGGLE_MUTE", "shortcut": [[0, 77, 1]], "enabled": false},
            "3": {"action": "TOGGLE_DEAFEN", "shortcut": [], "enabled": true},
            // A gamepad, the Windows key with M, and push-to-talk with Ctrl.
            "4": {"action": "TOGGLE_MUTE", "shortcut": [[3, 1, "0:0"]], "enabled": true},
            "5": {"action": "TOGGLE_DEAFEN", "shortcut": [[0, 91, 1], [0, 77, 1]], "enabled": true},
            "6": {"action": "PUSH_TO_TALK", "shortcut": [[0, 162, 1], [0, 86, 1]], "enabled": true}
        }));
        // Nothing usable: nothing offered.
        assert_eq!(offer(Some(&binds), None, &us), None);
        let mut more = binds.clone();
        more["_state"]["9"] =
            json!({"action": "TOGGLE_DEAFEN", "shortcut": [[0, 119, 1]], "enabled": true});
        // The first bind by id wins: 5 (the Windows key) before 9.
        let offer = offer(Some(&more), None, &us);
        assert_eq!(offer, None);
        let mut usable_deafen = binds;
        usable_deafen["_state"]["5"] =
            json!({"action": "TOGGLE_DEAFEN", "shortcut": [[0, 119, 1]], "enabled": true});
        let offer = super::offer(Some(&usable_deafen), None, &us).unwrap();
        assert_eq!(offer.deafen, Some(VoiceKey::keyboard("F8", "")));
        assert_eq!(
            offer.skipped,
            vec![VoiceShortcut::PushToTalk, VoiceShortcut::Mute]
        );
    }

    #[test]
    fn a_modifier_alone_and_mouse_buttons() {
        let binds = binds(json!({
            "1": {"action": "PUSH_TO_TALK", "shortcut": [[2, 163, 1]], "enabled": true},
            "2": {"action": "TOGGLE_MUTE", "shortcut": [[0, 162, 1], [1, 4, 1]], "enabled": true},
            // The left button never.
            "3": {"action": "TOGGLE_DEAFEN", "shortcut": [[1, 0, 1]], "enabled": true}
        }));
        let offer = offer(Some(&binds), None, &us).unwrap();
        assert_eq!(
            offer.push_to_talk,
            Some(VoiceKey::keyboard("ControlRight", ""))
        );
        assert_eq!(
            offer.mute,
            Some(VoiceKey::mouse(VoiceMouseButton::Forward).with_mods(&[VoiceModifier::Ctrl]))
        );
        assert_eq!(offer.skipped, vec![VoiceShortcut::Deafen]);
    }

    #[test]
    fn two_shortcuts_on_one_key_keep_the_first() {
        let binds = binds(json!({
            "1": {"action": "PUSH_TO_TALK", "shortcut": [[0, 77, 1]], "enabled": true},
            "2": {"action": "TOGGLE_MUTE", "shortcut": [[0, 162, 1], [0, 77, 1]], "enabled": true},
            "3": {"action": "TOGGLE_DEAFEN", "shortcut": [[0, 78, 1]], "enabled": true}
        }));
        let offer = offer(Some(&binds), None, &us).unwrap();
        assert!(offer.push_to_talk.is_some() && offer.mute.is_none());
        assert_eq!(offer.deafen, Some(VoiceKey::keyboard("KeyN", "N")));
        assert_eq!(offer.skipped, vec![VoiceShortcut::Mute]);
    }

    /// A localStorage value as Chromium writes it: 1 and Latin-1.
    fn latin1(json: &Value) -> Vec<u8> {
        let mut out = vec![1];
        out.extend(json.to_string().chars().map(|c| c as u8));
        out
    }

    #[test]
    fn read_finds_them_in_a_made_up_discord_folder() {
        let roaming = tempfile::tempdir().unwrap();
        // No Discord at all.
        assert_eq!(read(roaming.path(), &us), None);
        // PTB only, its value in UTF-16, an old version in a table and the newest in the log.
        let dir = roaming
            .path()
            .join("discordptb")
            .join("Local Storage")
            .join("leveldb");
        std::fs::create_dir_all(&dir).unwrap();
        let key = storage_key("https://ptb.discord.com", "keybinds");
        let old = latin1(&binds(
            json!({"1": {"action": "TOGGLE_MUTE", "shortcut": [[0, 77, 1]], "enabled": true}}),
        ));
        let newer = binds(
            json!({"1": {"action": "TOGGLE_MUTE", "shortcut": [[0, 119, 1]], "enabled": true}}),
        );
        let mut utf16 = vec![0];
        for unit in newer.to_string().encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        std::fs::write(
            dir.join("000005.ldb"),
            fixture::table(&[(&key, 10, Some(&old))]),
        )
        .unwrap();
        std::fs::write(
            dir.join("000006.log"),
            fixture::log(11, &[(&key, Some(&utf16))]),
        )
        .unwrap();
        // Other keys (a session's) are never asked for.
        std::fs::write(
            dir.join("000007.log"),
            fixture::log(
                12,
                &[(
                    b"_https://ptb.discord.com\x00\x01token",
                    Some(b"\x01secret"),
                )],
            ),
        )
        .unwrap();
        let offer = read(roaming.path(), &us).unwrap();
        assert_eq!(offer.mute, Some(VoiceKey::keyboard("F8", "")));
        // The stable app first when both have something.
        let stable = roaming
            .path()
            .join("discord")
            .join("Local Storage")
            .join("leveldb");
        std::fs::create_dir_all(&stable).unwrap();
        let stable_key = storage_key("https://discord.com", "keybinds");
        let theirs = latin1(&binds(
            json!({"1": {"action": "TOGGLE_DEAFEN", "shortcut": [[0, 78, 1]], "enabled": true}}),
        ));
        std::fs::write(
            stable.join("000003.log"),
            fixture::log(1, &[(&stable_key, Some(&theirs))]),
        )
        .unwrap();
        let offer = read(roaming.path(), &us).unwrap();
        assert_eq!(offer.deafen, Some(VoiceKey::keyboard("KeyN", "N")));
        assert_eq!(offer.mute, None);
    }

    #[test]
    fn storage_values_decode_both_ways() {
        assert_eq!(storage_value(b"\x01ab\xe7").as_deref(), Some("abç"));
        assert_eq!(storage_value(&[0, b'h', 0, b'i', 0]).as_deref(), Some("hi"));
        assert_eq!(storage_value(&[0, b'h']), None);
        assert_eq!(storage_value(&[2, b'x']), None);
        assert_eq!(storage_value(&[]), None);
    }
}
