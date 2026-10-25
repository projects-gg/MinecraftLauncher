//! Cloud AFK's notices on the presence socket (README "AFK notices", AFKPLUS): with `afk` in the
//! capabilities a socket gets `{"t":"afk_notice","notice":{…}}` about the player's cloud accounts
//! (an approval waiting, a deal done, an account that stopped…) instead of the phone, and answers
//! `{"t":"afk_seen","id"}` once the player really saw one (the window in front, or the in-game
//! overlay showed it). Notices are data: the launcher writes their words in its own language.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

/// One notice. `event`: `stopped`, `renewal`, `expired`, `found`, `approval`, `approval_closed`,
/// `deal_done`, `deal_failed`, `quota`, `summary`, `paused`, `incident`, `message`, `weekly`, or the
/// service's own `night` (kept as text, so a newer event still arrives). `kind`: `afk_alert`,
/// `afk_deal`, `afk_info`; `priority`: `high` or `normal`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AfkNotice {
    /// A ULID, the same on every retry (`night:<until>` for the night's digest).
    pub id: String,
    pub event: String,
    pub kind: String,
    pub priority: String,
    /// Unix seconds.
    pub at: i64,
    /// A newer notice with the same tag replaces this one (`approval:<id>`).
    pub tag: Option<String>,
    /// The player's quiet hours: shown in the launcher only, without a system notification.
    pub quiet: bool,
    /// Account ids, item ids, amounts, times, reason codes and the bot account's own name
    /// (`name`); never a counterpart's name or a message's text. Numbers that come as numbers read
    /// as their text.
    #[serde(deserialize_with = "text_map")]
    #[specta(type = BTreeMap<String, String>)]
    pub data: BTreeMap<String, String>,
    /// The item's name in both languages, when this launcher knows the mode's catalog (filled in
    /// by the launcher; the service never sends it).
    pub item_name: Option<AfkItemName>,
}

impl AfkNotice {
    /// A value of `data`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(String::as_str)
    }
}

/// An item's name in Turkish and English (from the AFK+ catalog).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct AfkItemName {
    pub tr: String,
    pub en: String,
}

/// `data`'s values as text: strings as they are, numbers and booleans in their JSON form, anything
/// else left out (one odd value never hides the notice).
fn text_map<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    let raw = Option::<BTreeMap<String, serde_json::Value>>::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, value)| match value {
            serde_json::Value::String(text) => Some((key, text)),
            serde_json::Value::Number(number) => Some((key, number.to_string())),
            serde_json::Value::Bool(flag) => Some((key, flag.to_string())),
            _ => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_contracts_notice() {
        let notice: AfkNotice = serde_json::from_str(
            r#"{"id":"01JBXK4V7Q2M9R3T5W8Y0ZA1BC","event":"approval","kind":"afk_deal",
                "priority":"high","at":1792521000,"tag":"approval:01JBXK3D2E",
                "data":{"acct":"a_7f3k2m","name":"Ahmet_AFK","approval":"01JBXK3D2E","side":"buy",
                  "item":"minecraft:elytra","qty":"1","price":"37500","currency":"smp",
                  "expires":"1792521600"}}"#,
        )
        .unwrap();
        assert_eq!(notice.event, "approval");
        assert_eq!(notice.tag.as_deref(), Some("approval:01JBXK3D2E"));
        assert!(!notice.quiet);
        assert_eq!(notice.get("price"), Some("37500"));
        assert_eq!(notice.item_name, None);
    }

    #[test]
    fn odd_values_never_hide_a_notice() {
        let notice: AfkNotice = serde_json::from_str(
            r#"{"id":"night:1792540800","event":"night","kind":"afk_info","priority":"normal",
                "at":1792540800,"quiet":true,"later":1,
                "data":{"counts":"deal_done=2,found=1","n":3,"ok":true,"odd":{"x":1}}}"#,
        )
        .unwrap();
        assert!(notice.quiet);
        assert_eq!(notice.get("counts"), Some("deal_done=2,found=1"));
        assert_eq!(notice.get("n"), Some("3"));
        assert_eq!(notice.get("ok"), Some("true"));
        assert_eq!(notice.get("odd"), None);
        let bare: AfkNotice = serde_json::from_str(r#"{"id":"x","event":"later"}"#).unwrap();
        assert!(bare.data.is_empty());
    }
}
