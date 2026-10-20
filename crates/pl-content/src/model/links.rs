//! Community and social links (`LauncherConfig.links`): the launcher's community row at the bottom
//! of Home's news column and Settings → Hakkında. They have no field of their own: Studio publishes
//! them as `custom` entries with a well-known `id` or `icon` ([`CommunityPlatform::key`]), so older
//! Studios carry them and older launchers list them among the other links. Discord is
//! `links.discord`.
//!
//! While `custom` names none of the platforms, launchers show the built-in addresses
//! ([`CommunityPlatform::default_url`]); once Studio publishes one, the config decides and a
//! platform it leaves out, or gives an empty or unsafe address, is not shown.

use serde::{Deserialize, Serialize};

use super::{CustomLink, Links, is_safe_link};

/// A community or social platform the launcher shows with its own mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CommunityPlatform {
    Wiki,
    Discord,
    Forum,
    Instagram,
    Tiktok,
    Youtube,
    X,
    Twitch,
    Kick,
}

impl CommunityPlatform {
    /// Every platform in the launcher's order: the community's own places, then the social
    /// accounts.
    pub const ALL: [Self; 9] = [
        Self::Wiki,
        Self::Discord,
        Self::Forum,
        Self::Instagram,
        Self::Tiktok,
        Self::Youtube,
        Self::X,
        Self::Twitch,
        Self::Kick,
    ];

    /// The `id` and `icon` Studio writes for the platform's `custom` entry. Discord's address is
    /// `links.discord`, so a `custom` entry named `discord` is an ordinary link.
    pub fn key(self) -> &'static str {
        match self {
            Self::Wiki => "wiki",
            Self::Discord => "discord",
            Self::Forum => "forum",
            Self::Instagram => "instagram",
            Self::Tiktok => "tiktok",
            Self::Youtube => "youtube",
            Self::X => "x",
            Self::Twitch => "twitch",
            Self::Kick => "kick",
        }
    }

    /// The platform's name, the same in every language (Studio's `label`, the launcher's
    /// tooltips).
    pub fn name(self) -> &'static str {
        match self {
            Self::Wiki => "Wiki",
            Self::Discord => "Discord",
            Self::Forum => "Forum",
            Self::Instagram => "Instagram",
            Self::Tiktok => "TikTok",
            Self::Youtube => "YouTube",
            Self::X => "X",
            Self::Twitch => "Twitch",
            Self::Kick => "Kick",
        }
    }

    /// A social account (the row's second group), as opposed to the community's own places.
    pub fn is_social(self) -> bool {
        !matches!(self, Self::Wiki | Self::Discord | Self::Forum)
    }

    /// The address launchers show while the config gives none: for Discord while
    /// `links.discord` is empty, for the others while `custom` names no platform at all.
    pub fn default_url(self) -> Option<&'static str> {
        match self {
            Self::Wiki => Some("https://wiki.projects.gg"),
            Self::Discord => Some("https://projects.gg/discord"),
            Self::Forum => Some("https://forum.projects.gg"),
            Self::Instagram => Some("https://www.instagram.com/projects.oyun/"),
            Self::Tiktok => Some("https://www.tiktok.com/@projects.oyun"),
            Self::Youtube => Some("https://www.youtube.com/@projectsoyun"),
            Self::X | Self::Twitch | Self::Kick => None,
        }
    }

    /// The platform a `custom` entry stands for: its `id` or `icon` is the platform's key (ASCII
    /// case and surrounding spaces do not matter; `twitter` is X). Never Discord.
    pub fn of_link(link: &CustomLink) -> Option<Self> {
        let known = |text: &str| {
            let text = text.trim().to_ascii_lowercase();
            if text == "twitter" {
                return Some(Self::X);
            }
            Self::ALL
                .into_iter()
                .find(|platform| *platform != Self::Discord && platform.key() == text)
        };
        known(&link.id).or_else(|| link.icon.as_deref().and_then(known))
    }
}

/// One link of the launcher's community row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommunityLink {
    pub platform: CommunityPlatform,
    pub url: String,
}

/// Addresses the community row opens: `https://` with a host, no credentials.
fn is_community_url(url: &str) -> bool {
    url.starts_with("https://") && is_safe_link(url)
}

impl Links {
    /// The community and social links launchers show, in [`CommunityPlatform::ALL`] order:
    /// Discord from `discord` (else, or when that is unusable, its built-in address); every other
    /// platform from its first
    /// `custom` entry, or from the built-in addresses while `custom` names none of them. A
    /// platform whose entry has an empty or unsafe address is not shown.
    pub fn community(&self) -> Vec<CommunityLink> {
        let named = self
            .custom
            .iter()
            .any(|link| CommunityPlatform::of_link(link).is_some());
        CommunityPlatform::ALL
            .into_iter()
            .filter_map(|platform| {
                let url = match platform {
                    // Never hidden: a missing or unusable address falls back to the built-in one.
                    CommunityPlatform::Discord => self
                        .discord
                        .as_deref()
                        .filter(|url| is_community_url(url))
                        .or(platform.default_url()),
                    _ if named => self
                        .custom
                        .iter()
                        .find(|link| CommunityPlatform::of_link(link) == Some(platform))
                        .map(|link| link.url.as_str()),
                    _ => platform.default_url(),
                }?;
                is_community_url(url).then(|| CommunityLink {
                    platform,
                    url: url.to_owned(),
                })
            })
            .collect()
    }

    /// The `custom` entries that are not community platforms (Settings → Hakkında lists them by
    /// their labels), in authored order.
    pub fn other_custom(&self) -> impl Iterator<Item = &CustomLink> {
        self.custom
            .iter()
            .filter(|link| CommunityPlatform::of_link(link).is_none())
    }

    /// Drops platform entries without an address while another platform entry has one. Studio
    /// keeps an emptied platform in its workspace; only when every platform is empty are they
    /// published that way, so launchers hide them all instead of falling back to the built-in
    /// addresses.
    pub fn drop_empty_platforms(&mut self) {
        let blank = |link: &CustomLink| {
            CommunityPlatform::of_link(link).is_some() && link.url.trim().is_empty()
        };
        let any_address = self
            .custom
            .iter()
            .any(|link| CommunityPlatform::of_link(link).is_some() && !blank(link));
        if any_address {
            self.custom.retain(|link| !blank(link));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(id: &str, url: &str, icon: Option<&str>) -> CustomLink {
        CustomLink {
            id: id.into(),
            label: id.into(),
            url: url.into(),
            icon: icon.map(Into::into),
        }
    }

    fn row(links: &Links) -> Vec<(&'static str, String)> {
        links
            .community()
            .into_iter()
            .map(|link| (link.platform.key(), link.url))
            .collect()
    }

    #[test]
    fn nothing_published_shows_the_built_in_addresses() {
        let links = Links::default();
        assert_eq!(
            row(&links),
            [
                ("wiki", "https://wiki.projects.gg".to_owned()),
                ("discord", "https://projects.gg/discord".to_owned()),
                ("forum", "https://forum.projects.gg".to_owned()),
                (
                    "instagram",
                    "https://www.instagram.com/projects.oyun/".to_owned()
                ),
                ("tiktok", "https://www.tiktok.com/@projects.oyun".to_owned()),
                (
                    "youtube",
                    "https://www.youtube.com/@projectsoyun".to_owned()
                ),
            ]
        );
        // Other links do not count as platforms: the built-in addresses stay.
        let links = Links {
            discord: Some("https://discord.gg/projects".into()),
            custom: vec![link("magaza", "https://store.projects.gg", None)],
            ..Links::default()
        };
        let shown = row(&links);
        assert_eq!(shown.len(), 6);
        assert_eq!(
            shown[1],
            ("discord", "https://discord.gg/projects".to_owned())
        );
        assert_eq!(
            links
                .other_custom()
                .map(|l| l.id.as_str())
                .collect::<Vec<_>>(),
            ["magaza"]
        );
    }

    #[test]
    fn a_published_platform_makes_the_config_decide() {
        let links = Links {
            custom: vec![
                link("youtube", "https://www.youtube.com/@yeni", Some("youtube")),
                link("x", "https://x.com/projects", None),
            ],
            ..Links::default()
        };
        // Wiki, forum, Instagram and TikTok are left out now; Discord keeps its fallback.
        assert_eq!(
            row(&links),
            [
                ("discord", "https://projects.gg/discord".to_owned()),
                ("youtube", "https://www.youtube.com/@yeni".to_owned()),
                ("x", "https://x.com/projects".to_owned()),
            ]
        );
        assert_eq!(links.other_custom().count(), 0);
    }

    #[test]
    fn platforms_are_known_by_id_or_icon() {
        let links = Links {
            custom: vec![
                link("Wiki ", "https://wiki.projects.gg/tr", None),
                link(
                    "bizi-izle",
                    "https://www.twitch.tv/projects",
                    Some("TWITCH"),
                ),
                link(
                    "eski-hesap",
                    "https://twitter.com/projects",
                    Some("twitter"),
                ),
                link("discord", "https://discord.gg/other", None),
            ],
            ..Links::default()
        };
        assert_eq!(
            row(&links),
            [
                ("wiki", "https://wiki.projects.gg/tr".to_owned()),
                ("discord", "https://projects.gg/discord".to_owned()),
                ("x", "https://twitter.com/projects".to_owned()),
                ("twitch", "https://www.twitch.tv/projects".to_owned()),
            ]
        );
        // A custom entry named discord is an ordinary link: Discord is `links.discord`.
        assert_eq!(
            links
                .other_custom()
                .map(|l| l.id.as_str())
                .collect::<Vec<_>>(),
            ["discord"]
        );
    }

    #[test]
    fn empty_unsafe_and_repeated_entries_hide_or_lose() {
        let links = Links {
            discord: Some("  ".into()),
            custom: vec![
                link("instagram", "", Some("instagram")),
                link("tiktok", "http://www.tiktok.com/@projects.oyun", None),
                link("youtube", "javascript:alert(1)", None),
                link("forum", "projects://news/forum", None),
                link("kick", "https://kick.com/first", None),
                link("kick-2", "https://kick.com/second", Some("kick")),
                link("wiki", "https://user:pass@wiki.projects.gg", None),
            ],
            ..Links::default()
        };
        // Only https with a host and no credentials; the first entry of a platform wins, even
        // when a later one would be shown.
        assert_eq!(
            row(&links),
            [
                ("discord", "https://projects.gg/discord".to_owned()),
                ("kick", "https://kick.com/first".to_owned()),
            ]
        );
    }

    #[test]
    fn every_platform_emptied_hides_them_all() {
        let mut links = Links {
            custom: vec![
                link("wiki", "", Some("wiki")),
                link("instagram", " ", Some("instagram")),
                link("site", "https://projects.gg/rehber", None),
            ],
            ..Links::default()
        };
        let before = links.clone();
        links.drop_empty_platforms();
        assert_eq!(links, before, "kept: they stand for 'hide every platform'");
        assert_eq!(
            row(&links),
            [("discord", "https://projects.gg/discord".to_owned())]
        );

        // With an address somewhere, the empty ones are simply left out.
        links.custom.push(link(
            "youtube",
            "https://www.youtube.com/@projectsoyun",
            None,
        ));
        links.drop_empty_platforms();
        assert_eq!(
            links
                .custom
                .iter()
                .map(|l| l.id.as_str())
                .collect::<Vec<_>>(),
            ["site", "youtube"]
        );
        assert_eq!(
            row(&links),
            [
                ("discord", "https://projects.gg/discord".to_owned()),
                (
                    "youtube",
                    "https://www.youtube.com/@projectsoyun".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn the_wire_names_are_the_keys() {
        for platform in CommunityPlatform::ALL {
            assert_eq!(
                serde_json::to_value(platform).unwrap(),
                serde_json::Value::String(platform.key().to_owned())
            );
            assert_eq!(
                platform.is_social(),
                CommunityPlatform::ALL[3..].contains(&platform)
            );
            if let Some(url) = platform.default_url() {
                assert!(is_community_url(url), "{url}");
            }
        }
    }
}
