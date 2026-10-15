//! Rich Presence activity model and its validation.
//!
//! [`Activity`] is the UI-facing shape (camelCase, specta). [`Activity::to_wire`] produces the
//! `SET_ACTIVITY` payload after [`Activity::sanitized`] has applied Discord's limits, so a long
//! server name or an invalid URL never makes Discord reject the whole update.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Longest `details`, `state`, `large_text`, `small_text` and party id (UTF-16 units).
pub const TEXT_MAX: usize = 128;
/// Shortest text Discord accepts; one-character texts are padded.
pub const TEXT_MIN: usize = 2;
pub const BUTTON_LABEL_MAX: usize = 32;
pub const BUTTON_URL_MAX: usize = 512;
/// Longest `details_url`, `state_url`, image keys/URLs and image URLs.
pub const URL_MAX: usize = 256;
pub const MAX_BUTTONS: usize = 2;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum ActivityType {
    #[default]
    Playing,
    Listening,
    Watching,
    Competing,
}

impl ActivityType {
    /// Discord's numeric activity type.
    pub fn code(self) -> u8 {
        match self {
            Self::Playing => 0,
            Self::Listening => 2,
            Self::Watching => 3,
            Self::Competing => 5,
        }
    }
}

/// Which field the member list shows next to the app name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StatusDisplayType {
    Name,
    State,
    Details,
}

impl StatusDisplayType {
    pub fn code(self) -> u8 {
        match self {
            Self::Name => 0,
            Self::State => 1,
            Self::Details => 2,
        }
    }
}

/// Unix timestamps in milliseconds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Timestamps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
}

/// Images are asset keys from the developer portal or `https://` URLs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Assets {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PartySize {
    pub current: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Party {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<PartySize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Button {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    #[serde(rename = "type", default)]
    pub kind: ActivityType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_display_type: Option<StatusDisplayType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<Timestamps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<Assets>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub party: Option<Party>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<Button>,
}

impl Activity {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn kind(mut self, kind: ActivityType) -> Self {
        self.kind = kind;
        self
    }

    pub fn status_display(mut self, display: StatusDisplayType) -> Self {
        self.status_display_type = Some(display);
        self
    }

    pub fn details(mut self, text: impl Into<String>) -> Self {
        self.details = Some(text.into());
        self
    }

    pub fn details_url(mut self, url: impl Into<String>) -> Self {
        self.details_url = Some(url.into());
        self
    }

    pub fn state(mut self, text: impl Into<String>) -> Self {
        self.state = Some(text.into());
        self
    }

    pub fn state_url(mut self, url: impl Into<String>) -> Self {
        self.state_url = Some(url.into());
        self
    }

    /// Elapsed time counts up from `unix_ms`.
    pub fn start_timestamp(mut self, unix_ms: u64) -> Self {
        self.timestamps
            .get_or_insert_with(Timestamps::default)
            .start = Some(unix_ms);
        self
    }

    /// Remaining time counts down to `unix_ms`.
    pub fn end_timestamp(mut self, unix_ms: u64) -> Self {
        self.timestamps.get_or_insert_with(Timestamps::default).end = Some(unix_ms);
        self
    }

    pub fn started_now(self) -> Self {
        self.start_timestamp(unix_millis())
    }

    pub fn large_image(mut self, key_or_url: impl Into<String>) -> Self {
        self.assets_mut().large_image = Some(key_or_url.into());
        self
    }

    pub fn large_text(mut self, text: impl Into<String>) -> Self {
        self.assets_mut().large_text = Some(text.into());
        self
    }

    pub fn large_url(mut self, url: impl Into<String>) -> Self {
        self.assets_mut().large_url = Some(url.into());
        self
    }

    pub fn small_image(mut self, key_or_url: impl Into<String>) -> Self {
        self.assets_mut().small_image = Some(key_or_url.into());
        self
    }

    pub fn small_text(mut self, text: impl Into<String>) -> Self {
        self.assets_mut().small_text = Some(text.into());
        self
    }

    pub fn small_url(mut self, url: impl Into<String>) -> Self {
        self.assets_mut().small_url = Some(url.into());
        self
    }

    pub fn party(mut self, id: impl Into<String>, current: u32, max: u32) -> Self {
        self.party = Some(Party {
            id: Some(id.into()),
            size: Some(PartySize { current, max }),
        });
        self
    }

    pub fn button(mut self, label: impl Into<String>, url: impl Into<String>) -> Self {
        self.buttons.push(Button {
            label: label.into(),
            url: url.into(),
        });
        self
    }

    fn assets_mut(&mut self) -> &mut Assets {
        self.assets.get_or_insert_with(Assets::default)
    }

    /// A copy that satisfies Discord's limits: texts trimmed, padded to 2 and truncated to
    /// 128 characters (with `…`), invalid URLs and images dropped, links without their text
    /// dropped, party size clamped, inverted timestamps fixed, at most two valid buttons.
    pub fn sanitized(&self) -> Activity {
        let details = self.details.as_deref().and_then(|t| fit_text(t, TEXT_MAX));
        let state = self.state.as_deref().and_then(|t| fit_text(t, TEXT_MAX));
        Activity {
            kind: self.kind,
            status_display_type: self.status_display_type,
            details_url: details.as_ref().and(
                self.details_url
                    .as_deref()
                    .and_then(|u| clean_url(u, URL_MAX)),
            ),
            state_url: state.as_ref().and(
                self.state_url
                    .as_deref()
                    .and_then(|u| clean_url(u, URL_MAX)),
            ),
            details,
            state,
            timestamps: self.timestamps.and_then(sanitize_timestamps),
            assets: self.assets.as_ref().and_then(sanitize_assets),
            party: self.party.as_ref().and_then(sanitize_party),
            buttons: self
                .buttons
                .iter()
                .filter_map(|button| {
                    Some(Button {
                        label: fit_text(&button.label, BUTTON_LABEL_MAX)?,
                        url: clean_url(&button.url, BUTTON_URL_MAX)?,
                    })
                })
                .take(MAX_BUTTONS)
                .collect(),
        }
    }

    /// The `activity` object of a `SET_ACTIVITY` command (sanitized, Discord field names).
    pub fn to_wire(&self) -> Value {
        let activity = self.sanitized();
        let mut wire = Map::new();
        wire.insert("type".into(), json!(activity.kind.code()));
        if let Some(display) = activity.status_display_type {
            wire.insert("status_display_type".into(), json!(display.code()));
        }
        insert(&mut wire, "details", activity.details);
        insert(&mut wire, "details_url", activity.details_url);
        insert(&mut wire, "state", activity.state);
        insert(&mut wire, "state_url", activity.state_url);
        if let Some(timestamps) = activity.timestamps {
            let mut map = Map::new();
            if let Some(start) = timestamps.start {
                map.insert("start".into(), json!(start));
            }
            if let Some(end) = timestamps.end {
                map.insert("end".into(), json!(end));
            }
            wire.insert("timestamps".into(), Value::Object(map));
        }
        if let Some(assets) = activity.assets {
            let mut map = Map::new();
            insert(&mut map, "large_image", assets.large_image);
            insert(&mut map, "large_text", assets.large_text);
            insert(&mut map, "large_url", assets.large_url);
            insert(&mut map, "small_image", assets.small_image);
            insert(&mut map, "small_text", assets.small_text);
            insert(&mut map, "small_url", assets.small_url);
            wire.insert("assets".into(), Value::Object(map));
        }
        if let Some(party) = activity.party {
            let mut map = Map::new();
            insert(&mut map, "id", party.id);
            if let Some(size) = party.size {
                map.insert("size".into(), json!([size.current, size.max]));
            }
            wire.insert("party".into(), Value::Object(map));
        }
        if !activity.buttons.is_empty() {
            let buttons = activity
                .buttons
                .into_iter()
                .map(|b| json!({ "label": b.label, "url": b.url }))
                .collect();
            wire.insert("buttons".into(), Value::Array(buttons));
        }
        wire.insert("instance".into(), json!(false));
        Value::Object(wire)
    }
}

fn insert(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_owned(), Value::String(value));
    }
}

fn sanitize_timestamps(timestamps: Timestamps) -> Option<Timestamps> {
    let start = timestamps.start.filter(|&t| t > 0);
    let end = timestamps
        .end
        .filter(|&t| t > 0 && start.is_none_or(|s| t >= s));
    (start.is_some() || end.is_some()).then_some(Timestamps { start, end })
}

fn sanitize_assets(assets: &Assets) -> Option<Assets> {
    let large_image = assets.large_image.as_deref().and_then(clean_image);
    let small_image = assets.small_image.as_deref().and_then(clean_image);
    let text = |image: &Option<String>, text: &Option<String>| {
        image
            .as_ref()
            .and(text.as_deref().and_then(|t| fit_text(t, TEXT_MAX)))
    };
    let link = |image: &Option<String>, url: &Option<String>| {
        image
            .as_ref()
            .and(url.as_deref().and_then(|u| clean_url(u, URL_MAX)))
    };
    let cleaned = Assets {
        large_text: text(&large_image, &assets.large_text),
        large_url: link(&large_image, &assets.large_url),
        small_text: text(&small_image, &assets.small_text),
        small_url: link(&small_image, &assets.small_url),
        large_image,
        small_image,
    };
    (cleaned.large_image.is_some() || cleaned.small_image.is_some()).then_some(cleaned)
}

fn sanitize_party(party: &Party) -> Option<Party> {
    let id = party.id.as_deref().and_then(|id| fit_text(id, TEXT_MAX));
    let size = party.size.filter(|s| s.max > 0).map(|s| PartySize {
        current: s.current.clamp(1, s.max),
        max: s.max,
    });
    (id.is_some() || size.is_some()).then_some(Party { id, size })
}

/// Trims `text`; empty becomes `None`, one character is padded with a zero-width space (Discord
/// rejects texts shorter than two characters) and anything longer than `max` UTF-16 units is
/// cut at a character boundary and ends with `…`.
pub fn fit_text(text: &str, max: usize) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut out = if text.encode_utf16().count() <= max {
        text.to_owned()
    } else {
        let mut cut = String::new();
        let mut used = 0;
        for c in text.chars() {
            if used + c.len_utf16() > max.saturating_sub(1) {
                break;
            }
            used += c.len_utf16();
            cut.push(c);
        }
        let mut cut = cut.trim_end().to_owned();
        cut.push('…');
        cut
    };
    if out.encode_utf16().count() < TEXT_MIN {
        out.push('\u{200B}');
    }
    Some(out)
}

/// An absolute `http(s)://` URL without whitespace and at most `max` bytes long.
fn clean_url(url: &str, max: usize) -> Option<String> {
    let url = url.trim();
    let scheme_len = if starts_with_ignore_case(url, "https://") {
        8
    } else if starts_with_ignore_case(url, "http://") {
        7
    } else {
        return None;
    };
    (url.len() > scheme_len && url.len() <= max && !url.chars().any(char::is_whitespace))
        .then(|| url.to_owned())
}

/// An asset key or an image URL.
fn clean_image(image: &str) -> Option<String> {
    let image = image.trim();
    if starts_with_ignore_case(image, "http://") || starts_with_ignore_case(image, "https://") {
        return clean_url(image, URL_MAX);
    }
    (!image.is_empty() && image.len() <= URL_MAX && !image.chars().any(char::is_whitespace))
        .then(|| image.to_owned())
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}

pub(crate) fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_activity_wire_snapshot() {
        let activity = Activity::new()
            .kind(ActivityType::Competing)
            .status_display(StatusDisplayType::State)
            .details("Projects · 1.21.1")
            .details_url("https://projects.gg")
            .state("play.projects.gg")
            .state_url("https://projects.gg/oyna")
            .start_timestamp(1_760_000_000_000)
            .end_timestamp(1_760_000_600_000)
            .large_image("logo")
            .large_text("Projects Launcher")
            .large_url("https://projects.gg")
            .small_image("https://mc-heads.net/avatar/Steve/64")
            .small_text("Steve")
            .party("lobby-1", 3, 10)
            .button("Sunucuya katıl", "https://projects.gg/oyna")
            .button("Discord", "https://discord.gg/projects");
        assert_eq!(
            activity.to_wire(),
            json!({
                "type": 5,
                "status_display_type": 1,
                "details": "Projects · 1.21.1",
                "details_url": "https://projects.gg",
                "state": "play.projects.gg",
                "state_url": "https://projects.gg/oyna",
                "timestamps": { "start": 1_760_000_000_000u64, "end": 1_760_000_600_000u64 },
                "assets": {
                    "large_image": "logo",
                    "large_text": "Projects Launcher",
                    "large_url": "https://projects.gg",
                    "small_image": "https://mc-heads.net/avatar/Steve/64",
                    "small_text": "Steve"
                },
                "party": { "id": "lobby-1", "size": [3, 10] },
                "buttons": [
                    { "label": "Sunucuya katıl", "url": "https://projects.gg/oyna" },
                    { "label": "Discord", "url": "https://discord.gg/projects" }
                ],
                "instance": false
            })
        );
    }

    #[test]
    fn minimal_activity_wire_snapshot() {
        assert_eq!(
            Activity::new().to_wire(),
            json!({ "type": 0, "instance": false })
        );
        assert_eq!(
            Activity::new()
                .kind(ActivityType::Watching)
                .state("Haberler")
                .to_wire(),
            json!({ "type": 3, "state": "Haberler", "instance": false })
        );
    }

    #[test]
    fn ui_json_shape() {
        let activity = Activity::new().details("x").party("p", 1, 2);
        let json = serde_json::to_value(&activity).unwrap();
        assert_eq!(json["type"], "playing");
        assert_eq!(json["party"]["size"]["current"], 1);
        let parsed: Activity = serde_json::from_value(json).unwrap();
        assert_eq!(parsed, activity);
    }

    #[test]
    fn text_limits() {
        assert_eq!(fit_text("  ", TEXT_MAX), None);
        assert_eq!(fit_text(" a ", TEXT_MAX).unwrap(), "a\u{200B}");
        assert_eq!(fit_text("ab", TEXT_MAX).unwrap(), "ab");
        let long = "ğ".repeat(200);
        let cut = fit_text(&long, TEXT_MAX).unwrap();
        assert_eq!(cut.encode_utf16().count(), TEXT_MAX);
        assert!(cut.ends_with('…'));
        // Astral characters count as two UTF-16 units and are never split.
        let emoji = "🐉".repeat(100);
        let cut = fit_text(&emoji, TEXT_MAX).unwrap();
        assert!(cut.encode_utf16().count() <= TEXT_MAX);
        assert!(cut.trim_end_matches('…').chars().all(|c| c == '🐉'));
        let label = fit_text(
            "Çok uzun bir buton etiketi burada duruyor",
            BUTTON_LABEL_MAX,
        )
        .unwrap();
        assert_eq!(label.encode_utf16().count(), BUTTON_LABEL_MAX);
    }

    #[test]
    fn sanitization_drops_invalid_parts() {
        let activity = Activity::new()
            .details("d")
            .details_url("javascript:alert(1)")
            .state_url("https://projects.gg")
            .start_timestamp(2_000)
            .end_timestamp(1_000)
            .large_image("has space")
            .large_text("orphan text")
            .small_image("https://ok.example/a.png")
            .small_url("not a url")
            .party("", 12, 10)
            .button("", "https://projects.gg")
            .button("Bir", "ftp://projects.gg")
            .button("İki", "https://projects.gg/2")
            .button("Üç", "https://projects.gg/3")
            .button("Dört", "https://projects.gg/4");
        let clean = activity.sanitized();
        assert_eq!(clean.details.as_deref(), Some("d\u{200B}"));
        assert_eq!(clean.details_url, None, "only http(s) links");
        assert_eq!(clean.state_url, None, "a link needs its text");
        assert_eq!(
            clean.timestamps,
            Some(Timestamps {
                start: Some(2_000),
                end: None
            })
        );
        let assets = clean.assets.unwrap();
        assert_eq!(assets.large_image, None);
        assert_eq!(assets.large_text, None);
        assert_eq!(
            assets.small_image.as_deref(),
            Some("https://ok.example/a.png")
        );
        assert_eq!(assets.small_url, None);
        assert_eq!(
            clean.party,
            Some(Party {
                id: None,
                size: Some(PartySize {
                    current: 10,
                    max: 10
                })
            })
        );
        let labels: Vec<_> = clean.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["İki", "Üç"]);

        let empty = Activity::new()
            .large_image(" ")
            .party("  ", 0, 0)
            .start_timestamp(0)
            .sanitized();
        assert_eq!(empty.assets, None);
        assert_eq!(empty.party, None);
        assert_eq!(empty.timestamps, None);
        let long_url = format!("https://projects.gg/{}", "a".repeat(URL_MAX));
        assert_eq!(
            Activity::new().large_image(long_url).sanitized().assets,
            None
        );
    }
}
