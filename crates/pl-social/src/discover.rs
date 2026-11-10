//! Crew discovery ("Ekip bul", CREW-DISCOVER; the service's README "Crew discovery"): crews whose
//! leader asked, with a short text and a few tags, approved once a day by the service's wave.
//!
//! The launcher fetches the listing only when "Ekip bul" opens and keeps it (with its `ETag`)
//! until the next wave; a copy older than [`DISCOVER_REVALIDATE_SECS`] is asked again with
//! `If-None-Match` (a `304` costs the service nothing past the session). Search, filters and order
//! are the launcher's own. Joining from a card goes through the service's join request rules; the
//! leader's "Keşif" block reads and changes the crew's place in it.

use serde::{Deserialize, Serialize};

use crate::model::JoinMode;

/// The service offers crew discovery (Studio's switch is on): `teamFeatures` lists it.
pub const TEAM_FEATURE_DISCOVER: &str = "discover";

/// A kept listing is asked again (with its `ETag`) once it is this old, so Studio's removals reach
/// an open launcher within a quarter of an hour.
pub const DISCOVER_REVALIDATE_SECS: i64 = 15 * 60;

/// The discovery text's length (characters), as the service counts it.
pub const DISCOVER_TEXT_MIN: usize = 10;
pub const DISCOVER_TEXT_MAX: usize = 160;
/// Tags a crew picks at most.
pub const DISCOVER_MAX_TAGS: usize = 3;

/// The tags a crew may pick, in the service's order. A launcher shows only those it knows.
pub const DISCOVER_TAGS: &[&str] = &[
    "survival",
    "skyblock",
    "smp",
    "hg",
    "pvp",
    "build",
    "events",
    "chat",
    "voice",
    "newcomers",
];

/// One listed crew: its card, the leader's text and tags, how to join, its size, whether it was
/// active this week. Never its members, leader, code or link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverCrew {
    pub id: String,
    pub name: String,
    pub tag: String,
    pub emblem: String,
    pub color: String,
    pub frame: String,
    #[serde(default)]
    pub motto: String,
    pub text: String,
    /// Names from [`DISCOVER_TAGS`] (unknown ones dropped).
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub join: JoinMode,
    pub members: u32,
    /// CREW-CAP: the crew's cap when the listing was made (its own, else the service's default);
    /// none from a service before crew caps (its cap was 30).
    #[serde(default)]
    pub max_members: Option<u32>,
    /// At least 3 members (all of a smaller crew) were on in the last 7 days.
    #[serde(default)]
    pub active: bool,
}

/// The listing (`GET /v1/discover`, format 1), in the day's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverListing {
    pub ver: i64,
    /// When the wave built it (Unix seconds; 0 before the first).
    #[serde(default)]
    pub at: i64,
    /// The next wave (Unix seconds): the copy is good until then.
    pub next: i64,
    pub crews: Vec<DiscoverCrew>,
}

impl DiscoverListing {
    /// Tags this version knows only, each crew's in the service's order.
    pub fn known_tags(mut self) -> Self {
        for crew in &mut self.crews {
            crew.tags
                .retain(|tag| DISCOVER_TAGS.contains(&tag.as_str()));
        }
        self
    }
}

/// The listing with the `ETag` to ask again with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverPage {
    pub listing: DiscoverListing,
    pub etag: Option<String>,
}

/// Where a crew stands in discovery, as its leader sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DiscoverState {
    /// Not in discovery (also a refusal whose wait ran out).
    #[default]
    Off,
    /// Asked; the next wave decides.
    Waiting,
    Listed,
    /// The wave (or Projects) did not approve it: it may ask again at `retryAt`.
    Refused,
    /// Projects closed it for good: the switch shows off and cannot be used.
    Closed,
    /// Reports hid it until Projects decide.
    Held,
}

impl<'de> Deserialize<'de> for DiscoverState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "waiting" => Self::Waiting,
            "listed" => Self::Listed,
            "refused" => Self::Refused,
            "closed" => Self::Closed,
            "held" => Self::Held,
            _ => Self::Off,
        })
    }
}

/// A text and its tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverText {
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A requirement: what it takes and what the crew has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverCheck {
    pub need: u32,
    pub have: u32,
}

impl DiscoverCheck {
    pub fn holds(&self) -> bool {
        self.have >= self.need
    }
}

/// The requirements the launcher shows before the leader asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverChecks {
    pub members: DiscoverCheck,
    /// Days since the crew was founded.
    pub crew_days: DiscoverCheck,
    /// Days the leader's account has been around.
    pub leader_days: DiscoverCheck,
    /// Not a secret crew.
    pub visible: bool,
}

impl DiscoverChecks {
    pub fn hold(&self) -> bool {
        self.visible && self.members.holds() && self.crew_days.holds() && self.leader_days.holds()
    }
}

/// The leader's "Keşif" block (`GET`, `PUT`, `DELETE /v1/teams/:id/discover`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverStatus {
    pub state: DiscoverState,
    /// In discovery now (listed, or hidden by reports).
    #[serde(default)]
    pub listed: Option<DiscoverText>,
    /// What the editor starts from: the latest text the leader sent.
    #[serde(default)]
    pub draft: Option<DiscoverText>,
    /// A change of a listed crew waits for the wave.
    #[serde(default)]
    pub change_waits: bool,
    /// The next wave (Unix seconds).
    pub wave_at: i64,
    /// Refused: when the leader may ask again (Unix seconds).
    #[serde(default)]
    pub retry_at: Option<i64>,
    pub checks: DiscoverChecks,
    /// Changes left this week, and when the count starts again (Unix seconds).
    pub edits_left: u32,
    pub edits_reset: i64,
}

/// What joining from a card led to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DiscoverJoin {
    /// In the crew now.
    Joined,
    /// The crew approves newcomers: the request waits for its leaders.
    Requested,
    /// No longer in discovery (or not for this player): the card goes.
    Gone,
}

/// The discovery text as the service will keep it: one paragraph (line breaks become spaces,
/// spaces collapse), trimmed; None outside 10-160 characters or with fewer than 5 letters.
pub fn discover_text(text: &str) -> Option<String> {
    // Invisible characters go first, as the service drops them (control characters other than
    // spacing, zero-width spaces and joiners, the byte order mark).
    let visible: String = text
        .chars()
        .filter(|c| {
            let code = *c as u32;
            !(c.is_control() && !c.is_whitespace())
                && !(0x200B..=0x200F).contains(&code)
                && !(0x2060..=0x2064).contains(&code)
                && code != 0xFEFF
        })
        .collect();
    let clean = visible.split_whitespace().collect::<Vec<_>>().join(" ");
    let length = clean.chars().count();
    let letters = clean.chars().filter(|c| c.is_alphabetic()).count();
    ((DISCOVER_TEXT_MIN..=DISCOVER_TEXT_MAX).contains(&length) && letters >= 5).then_some(clean)
}

/// Tags as the service takes them: known, each once, at most three; None otherwise.
pub fn discover_tags(tags: &[String]) -> Option<Vec<String>> {
    if tags.len() > DISCOVER_MAX_TAGS {
        return None;
    }
    let mut out: Vec<String> = Vec::with_capacity(tags.len());
    for tag in tags {
        if !DISCOVER_TAGS.contains(&tag.as_str()) || out.contains(tag) {
            return None;
        }
        out.push(tag.clone());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_listing_and_drops_unknown_tags() {
        let wire = r#"{"v":1,"ver":7,"next":1762441200,"at":1762354800,"day":20397,"crews":[
            {"id":"0123456789abcdef","name":"Gece Kartalları","tag":"GKR","emblem":"sword",
             "color":"gold","frame":"cut","motto":"","text":"Akşamları SkyBlock oynuyoruz.",
             "tags":["skyblock","rocket","newcomers"],"join":"approval","members":12,"active":true,
             "later":1}]}"#;
        let listing: DiscoverListing = serde_json::from_str(wire).unwrap();
        let listing = listing.known_tags();
        assert_eq!(listing.ver, 7);
        assert_eq!(listing.crews[0].tags, ["skyblock", "newcomers"]);
        assert_eq!(listing.crews[0].join, JoinMode::Approval);
    }

    #[test]
    fn reads_the_status_and_an_unknown_state_as_off() {
        let wire = r#"{"state":"listed","listed":{"text":"Akşamları SkyBlock.","tags":["skyblock"]},
            "draft":{"text":"Akşamları SkyBlock.","tags":["skyblock"]},"changeWaits":false,
            "waveAt":1762441200,"retryAt":null,"checks":{"members":{"need":3,"have":12},
            "crewDays":{"need":3,"have":40},"leaderDays":{"need":14,"have":200},"visible":true},
            "editsLeft":4,"editsReset":1762646400}"#;
        let status: DiscoverStatus = serde_json::from_str(wire).unwrap();
        assert_eq!(status.state, DiscoverState::Listed);
        assert!(status.checks.hold());
        let later: DiscoverState = serde_json::from_str(r#""paused""#).unwrap();
        assert_eq!(later, DiscoverState::Off);
    }

    #[test]
    fn text_and_tags_as_the_service_takes_them() {
        assert_eq!(
            discover_text("  Akşamları\nSkyBlock   oynuyoruz  ").as_deref(),
            Some("Akşamları SkyBlock oynuyoruz")
        );
        assert_eq!(discover_text("kısa"), None);
        assert_eq!(discover_text(&"ş".repeat(161)), None);
        assert!(discover_text(&"ş".repeat(160)).is_some());
        // Invisible characters do not count (the service drops them too).
        let hidden = format!("{}{}", "ş".repeat(160), char::from_u32(0x200B).unwrap());
        assert!(discover_text(&hidden).is_some());
        let tags = |list: &[&str]| list.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            discover_tags(&tags(&["pvp", "hg"])),
            Some(tags(&["pvp", "hg"]))
        );
        assert_eq!(discover_tags(&tags(&["pvp", "pvp"])), None);
        assert_eq!(discover_tags(&tags(&["rocket"])), None);
        assert_eq!(discover_tags(&tags(&["pvp", "hg", "smp", "chat"])), None);
    }
}
