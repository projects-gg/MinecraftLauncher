//! `projects://` links: tray actions, links on projects.gg and desktop shortcuts.

use serde::Serialize;
use url::Url;

/// The URL scheme the launcher registers.
pub const SCHEME: &str = "projects";

/// Where a `projects://` link (or a tray action) sends the user. Emitted to the UI as the
/// `deep-link` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DeepLink {
    Home,
    /// Start the selected instance (`projects://play`, `projects://join`).
    Play,
    /// Start one instance (`projects://play/<instance>`, used by desktop shortcuts).
    PlayInstance {
        id: String,
    },
    News {
        id: String,
    },
    Instance {
        id: String,
    },
    Afk,
    /// Add a friend by code (`projects://friend/PRJ-XXXX-XXXX`, from the QR page): the friends
    /// panel shows who it is; nothing is sent until the player confirms.
    Friend {
        code: String,
    },
}

/// Ids that may travel in a link: 1–64 ASCII letters, digits, `-`, `_` or `.`.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && id != "."
        && id != ".."
}

/// The link a desktop shortcut opens to start `instance_id`. `None` when the id cannot travel
/// in a link.
pub fn play_link(instance_id: &str) -> Option<String> {
    valid_id(instance_id).then(|| format!("{SCHEME}://play/{instance_id}"))
}

/// Parses `projects://<route>[/<id>]`. Anything unknown is ignored rather than guessed.
/// `friend_code` normalizes friend codes (the friends service owns their format).
pub fn parse_link(raw: &str, friend_code: impl Fn(&str) -> Option<String>) -> Option<DeepLink> {
    let url = Url::parse(raw.trim()).ok()?;
    if url.scheme() != SCHEME {
        return None;
    }
    // `projects://news/x` puts "news" in the host; `projects:news/x` keeps it in the path.
    let mut parts: Vec<&str> = url
        .host_str()
        .into_iter()
        .chain(url.path().split('/'))
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() > 2 {
        return None;
    }
    let id = parts.get(1).copied();
    let route = if parts.is_empty() {
        "home"
    } else {
        parts.remove(0)
    };
    match (route.to_ascii_lowercase().as_str(), id) {
        ("home" | "open", None) => Some(DeepLink::Home),
        ("play" | "join" | "oyna", None) => Some(DeepLink::Play),
        ("play" | "oyna", Some(id)) if valid_id(id) => {
            Some(DeepLink::PlayInstance { id: id.to_owned() })
        }
        ("afk", None) => Some(DeepLink::Afk),
        ("news" | "haber", Some(id)) if valid_id(id) => Some(DeepLink::News { id: id.to_owned() }),
        ("instance" | "kurulum", Some(id)) if valid_id(id) => {
            Some(DeepLink::Instance { id: id.to_owned() })
        }
        ("friend" | "arkadas", Some(code)) => {
            friend_code(code).map(|code| DeepLink::Friend { code })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stand-in for the friends service's normalizer.
    fn code(input: &str) -> Option<String> {
        let compact: String = input
            .chars()
            .filter(|c| *c != '-')
            .map(|c| c.to_ascii_uppercase())
            .collect();
        compact
            .strip_prefix("PRJ")
            .filter(|rest| rest.len() == 8)
            .map(|rest| format!("PRJ-{}-{}", &rest[..4], &rest[4..]))
    }

    fn parse(raw: &str) -> Option<DeepLink> {
        parse_link(raw, code)
    }

    #[test]
    fn links() {
        assert_eq!(parse("projects://"), Some(DeepLink::Home));
        assert_eq!(parse("projects://play"), Some(DeepLink::Play));
        assert_eq!(parse("projects://join/"), Some(DeepLink::Play));
        assert_eq!(parse("projects://AFK"), Some(DeepLink::Afk));
        assert_eq!(
            parse("projects://news/yaz-etkinligi-2026"),
            Some(DeepLink::News {
                id: "yaz-etkinligi-2026".into()
            })
        );
        assert_eq!(
            parse("projects:instance/projects"),
            Some(DeepLink::Instance {
                id: "projects".into()
            })
        );
        assert_eq!(parse("projects://news"), None);
        assert_eq!(parse("projects://news/a/b"), None);
        assert_eq!(parse("projects://news/a b"), None);
        assert_eq!(parse("https://projects.gg/play"), None);
        assert_eq!(parse("projects://delete/all"), None);
        assert_eq!(
            parse("projects://friend/prj-abcd-efgh/"),
            Some(DeepLink::Friend {
                code: "PRJ-ABCD-EFGH".into()
            })
        );
        assert_eq!(parse("projects://friend/nope"), None);
        assert_eq!(parse("projects://friend"), None);
        // The sign-in link is not a navigation (the friends service takes it).
        assert_eq!(parse("projects://auth/done?c=abc"), None);
    }

    #[test]
    fn shortcut_links() {
        // What a desktop shortcut passes to the launcher, as the OS hands it over.
        let link = play_link("fabric-1-21").unwrap();
        assert_eq!(link, "projects://play/fabric-1-21");
        assert_eq!(
            parse(&link),
            Some(DeepLink::PlayInstance {
                id: "fabric-1-21".into()
            })
        );
        assert_eq!(
            parse(" projects://oyna/projects/ "),
            Some(DeepLink::PlayInstance {
                id: "projects".into()
            })
        );
        // `join` always means the selected instance.
        assert_eq!(parse("projects://join/projects"), None);
        assert_eq!(parse("projects://play/a/b"), None);
        assert_eq!(parse("projects://play/..%2F.."), None);
        assert_eq!(parse("projects://play/d%C3%BCnya"), None);
        assert_eq!(play_link("dünya"), None);
        assert_eq!(play_link(""), None);
        assert_eq!(play_link(&"x".repeat(65)), None);
        assert_eq!(play_link(".."), None);
    }
}
