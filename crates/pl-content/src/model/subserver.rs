//! Sub-servers (AUTOJOIN): the games behind a Projects server entry's lobby (SkyBlock, SMP, …),
//! each with the command its lobby takes to switch there (`/aurora`). While `features.subServers`
//! is on, the launcher offers them beside "Doğrudan sunucuya bağlan" ("play.projects.gg →
//! SkyBlock"); the one the player picked goes to the in-game overlay, which sends its command once,
//! after the lobby says its login (and two-factor check) is done. Only Projects entries carry them.
//! See `docs/content-format.md`, "Sub-servers".

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{LauncherConfig, ServerEntry, is_private_use, item_art_host_matches};

/// Most sub-servers one server entry may list.
pub const MAX_SUB_SERVERS: usize = 16;
/// Longest `id`, in bytes (`[a-z0-9-]`).
pub const MAX_SUB_SERVER_ID_CHARS: usize = 32;
/// Longest name, in characters.
pub const MAX_SUB_SERVER_NAME_CHARS: usize = 24;
/// Longest command, in characters, its `/` included.
pub const MAX_SUB_SERVER_COMMAND_CHARS: usize = 64;
/// The only network whose entries carry sub-servers: `projects.gg` and its subdomains.
pub const SUB_SERVER_HOST: &str = "projects.gg";

/// One game behind a Projects lobby.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SubServer {
    /// Studio's short name for it (`skyblock`), kept when the name changes: the player's pick is
    /// saved by it.
    pub id: String,
    /// As players know it ("SkyBlock"); the launcher writes "play.projects.gg → SkyBlock".
    pub name: String,
    /// What the lobby takes to switch there, as a player would type it (`/aurora`).
    pub command: String,
    /// Offered only to players the launcher knows are in the beta program (BETA-GATE's Ar-Ge
    /// membership); hidden while that is unknown or the gate is off. Left out while false.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub testers_only: bool,
}

/// The rule one sub-server breaks (Studio words each one for its editor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubServerFault {
    /// Not 1–32 bytes of `[a-z0-9-]` (no `-` first or last).
    BadId,
    /// Empty, longer than [`MAX_SUB_SERVER_NAME_CHARS`], spaces around it, or a control, `§` or
    /// private use character.
    BadName,
    /// Not a command a player could type: see [`is_sub_server_command`].
    BadCommand,
    /// An earlier row has the same id.
    DuplicateId,
    /// An earlier row has the same name, compared without case.
    DuplicateName,
}

impl fmt::Display for SubServerFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadId => "id is not 1-32 characters of a-z, 0-9 and -",
            Self::BadName => "name is empty, too long or has a character that cannot be shown",
            Self::BadCommand => "command is not a single /command line a player could type",
            Self::DuplicateId => "id is used by an earlier row",
            Self::DuplicateName => "name is used by an earlier row",
        })
    }
}

/// What is wrong with a server entry's sub-servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubServerProblem {
    /// The entry's address is not Projects: launchers ignore its sub-servers.
    NotProjects,
    /// More than [`MAX_SUB_SERVERS`]: launchers use the first ones.
    TooMany,
    /// The row at `index` breaks `fault`: launchers leave it out.
    Row { index: usize, fault: SubServerFault },
}

impl fmt::Display for SubServerProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotProjects => write!(f, "only a {SUB_SERVER_HOST} server can list sub-servers"),
            Self::TooMany => write!(f, "more than {MAX_SUB_SERVERS} sub-servers"),
            Self::Row { index, fault } => write!(f, "sub-server {}: {fault}", index + 1),
        }
    }
}

/// A server address (`play.projects.gg`, `PLAY.projects.gg.:25565`) on the Projects network.
pub fn is_sub_server_host(address: &str) -> bool {
    item_art_host_matches(address, &[SUB_SERVER_HOST.to_owned()])
}

/// A sub-server id: 1–[`MAX_SUB_SERVER_ID_CHARS`] bytes of `[a-z0-9-]`, not starting or ending
/// with `-`.
pub fn is_sub_server_id(id: &str) -> bool {
    (1..=MAX_SUB_SERVER_ID_CHARS).contains(&id.len())
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A command a player could type in chat: `/`, then a command name (no `/` or space first), at
/// most [`MAX_SUB_SERVER_COMMAND_CHARS`] characters in all, words separated by single spaces,
/// nothing around it, no control, `§` or private use character.
pub fn is_sub_server_command(command: &str) -> bool {
    let Some(rest) = command.strip_prefix('/') else {
        return false;
    };
    !rest.is_empty()
        && command.chars().count() <= MAX_SUB_SERVER_COMMAND_CHARS
        && !rest.starts_with('/')
        && !rest.starts_with(' ')
        && !rest.ends_with(' ')
        && !rest.contains("  ")
        && !rest.chars().any(|c| {
            c.is_control() || (c.is_whitespace() && c != ' ') || c == '§' || is_private_use(c)
        })
}

fn is_sub_server_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.chars().count() <= MAX_SUB_SERVER_NAME_CHARS
        && !name
            .chars()
            .any(|c| c.is_control() || c == '§' || is_private_use(c))
}

/// The rule `sub` breaks on its own (duplicates need the whole list: [`ServerEntry::sub_server_problems`]).
pub fn sub_server_fault(sub: &SubServer) -> Option<SubServerFault> {
    if !is_sub_server_id(&sub.id) {
        Some(SubServerFault::BadId)
    } else if !is_sub_server_name(&sub.name) {
        Some(SubServerFault::BadName)
    } else if !is_sub_server_command(&sub.command) {
        Some(SubServerFault::BadCommand)
    } else {
        None
    }
}

impl ServerEntry {
    /// Everything Studio refuses to publish about this entry's sub-servers.
    pub fn sub_server_problems(&self) -> Vec<SubServerProblem> {
        let mut problems = Vec::new();
        if self.sub_servers.is_empty() {
            return problems;
        }
        if !is_sub_server_host(&self.address) {
            problems.push(SubServerProblem::NotProjects);
        }
        if self.sub_servers.len() > MAX_SUB_SERVERS {
            problems.push(SubServerProblem::TooMany);
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for (index, sub) in self.sub_servers.iter().enumerate() {
            let fault = sub_server_fault(sub).or_else(|| {
                if !ids.insert(sub.id.as_str()) {
                    Some(SubServerFault::DuplicateId)
                } else if !names.insert(sub.name.to_lowercase()) {
                    Some(SubServerFault::DuplicateName)
                } else {
                    None
                }
            });
            if let Some(fault) = fault {
                problems.push(SubServerProblem::Row { index, fault });
            }
        }
        problems
    }

    /// The sub-servers launchers offer, in Studio's order: none unless the entry is on Projects;
    /// rows that break a rule, and rows whose id or name an earlier row has, are left out; at most
    /// [`MAX_SUB_SERVERS`].
    pub fn usable_sub_servers(&self) -> Vec<&SubServer> {
        if !is_sub_server_host(&self.address) {
            return Vec::new();
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        self.sub_servers
            .iter()
            .filter(|sub| sub_server_fault(sub).is_none())
            .filter(|sub| ids.insert(sub.id.as_str()) && names.insert(sub.name.to_lowercase()))
            .take(MAX_SUB_SERVERS)
            .collect()
    }
}

impl LauncherConfig {
    /// The sub-servers offered for an instance that joins `address`: none while
    /// `features.subServers` is off or `address` is not on Projects; else those of the entry with
    /// that host, or of the first Projects entry that has any (Projects is one network under
    /// several names). Rows marked [`SubServer::testers_only`] are included: the caller shows them
    /// only to members of the beta program.
    pub fn sub_servers_for(&self, address: &str) -> Vec<&SubServer> {
        if !self.features.sub_servers || !is_sub_server_host(address) {
            return Vec::new();
        }
        let host = host_of(address);
        let mut fallback = None;
        for server in &self.servers {
            let subs = server.usable_sub_servers();
            if subs.is_empty() {
                continue;
            }
            if host_of(&server.address) == host {
                return subs;
            }
            if fallback.is_none() {
                fallback = Some(subs);
            }
        }
        fallback.unwrap_or_default()
    }

    /// The sub-server `id` offered for `address` ([`Self::sub_servers_for`]), if it is one.
    pub fn sub_server(&self, address: &str, id: &str) -> Option<&SubServer> {
        self.sub_servers_for(address)
            .into_iter()
            .find(|sub| sub.id == id)
    }
}

/// The host of a server address, lowercase, without the port or a trailing dot.
fn host_of(address: &str) -> String {
    let address = address.trim();
    let name = match address.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) && !name.contains(':') => {
            name
        }
        _ => address,
    };
    name.trim_end_matches('.').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(id: &str, name: &str, command: &str) -> SubServer {
        SubServer {
            id: id.into(),
            name: name.into(),
            command: command.into(),
            testers_only: false,
        }
    }

    fn entry(address: &str, subs: Vec<SubServer>) -> ServerEntry {
        ServerEntry {
            id: "main".into(),
            name: "Projects".into(),
            address: address.into(),
            port: None,
            featured: true,
            description: None,
            icon: None,
            sub_servers: subs,
        }
    }

    fn today() -> Vec<SubServer> {
        vec![
            sub("skyblock", "SkyBlock", "/aurora"),
            sub("smp", "SMP", "/smp"),
            sub("survival", "Survival", "/zephyr"),
            sub("hg", "HG", "/hg"),
            SubServer {
                testers_only: true,
                ..sub("testserver", "TestServer", "/testserver")
            },
            sub("pvp", "PvP", "/aergia"),
        ]
    }

    #[test]
    fn commands_are_what_a_player_could_type() {
        for good in [
            "/aurora",
            "/smp",
            "/testserver sky",
            "/goqueue aurora-1",
            "/açlıkoyunları",
        ] {
            assert!(is_sub_server_command(good), "{good}");
        }
        let long = format!("/{}", "a".repeat(MAX_SUB_SERVER_COMMAND_CHARS));
        for bad in [
            "",
            "/",
            "aurora",
            "//aurora",
            "/ aurora",
            "/aurora ",
            " /aurora",
            "/a  b",
            "/a\tb",
            "/a\nb",
            "/§aaurora",
            "/a\u{e000}",
            long.as_str(),
        ] {
            assert!(!is_sub_server_command(bad), "{bad:?}");
        }
    }

    #[test]
    fn ids_and_names_follow_their_rules() {
        assert!(is_sub_server_id("skyblock") && is_sub_server_id("aurora-1"));
        let long = "a".repeat(33);
        for bad in ["", "-a", "a-", "SkyBlock", "sky block", long.as_str()] {
            assert!(!is_sub_server_id(bad), "{bad:?}");
        }
        assert_eq!(
            sub_server_fault(&sub("a", " SMP", "/smp")),
            Some(SubServerFault::BadName)
        );
        assert_eq!(
            sub_server_fault(&sub("a", "", "/smp")),
            Some(SubServerFault::BadName)
        );
        assert_eq!(
            sub_server_fault(&sub("a", &"x".repeat(25), "/smp")),
            Some(SubServerFault::BadName)
        );
        assert_eq!(
            sub_server_fault(&sub("a", "SMP", "smp")),
            Some(SubServerFault::BadCommand)
        );
        assert_eq!(sub_server_fault(&sub("a", "Açlık Oyunları", "/hg")), None);
    }

    #[test]
    fn launchers_keep_the_good_rows_in_order() {
        let mut subs = today();
        subs.insert(1, sub("bad id", "Bad", "/bad"));
        subs.push(sub("skyblock", "Again", "/again"));
        subs.push(sub("other", "smp", "/other"));
        let server = entry("play.projects.gg", subs);
        let ids: Vec<&str> = server
            .usable_sub_servers()
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(
            ids,
            ["skyblock", "smp", "survival", "hg", "testserver", "pvp"]
        );
        assert_eq!(
            server.sub_server_problems(),
            [
                SubServerProblem::Row {
                    index: 1,
                    fault: SubServerFault::BadId
                },
                SubServerProblem::Row {
                    index: 7,
                    fault: SubServerFault::DuplicateId
                },
                SubServerProblem::Row {
                    index: 8,
                    fault: SubServerFault::DuplicateName
                },
            ]
        );
        let many = entry(
            "play.projects.gg",
            (0..20)
                .map(|i| sub(&format!("s{i}"), &format!("S{i}"), "/s"))
                .collect(),
        );
        assert_eq!(many.usable_sub_servers().len(), MAX_SUB_SERVERS);
        assert_eq!(many.sub_server_problems(), [SubServerProblem::TooMany]);
    }

    #[test]
    fn only_projects_carries_sub_servers() {
        assert!(is_sub_server_host("play.projects.gg"));
        assert!(is_sub_server_host("MC.Projects.GG.:25566"));
        assert!(is_sub_server_host("projects.gg"));
        for other in [
            "notprojects.gg",
            "projects.gg.evil.net",
            "mc.example.net",
            "",
        ] {
            assert!(!is_sub_server_host(other), "{other}");
        }
        let elsewhere = entry("mc.example.net", today());
        assert!(elsewhere.usable_sub_servers().is_empty());
        assert_eq!(
            elsewhere.sub_server_problems(),
            [SubServerProblem::NotProjects]
        );
        assert!(
            entry("mc.example.net", Vec::new())
                .sub_server_problems()
                .is_empty()
        );
    }

    #[test]
    fn the_switch_and_the_address_decide_what_is_offered() {
        let mut config = LauncherConfig {
            servers: vec![entry("play.projects.gg", today())],
            ..LauncherConfig::default()
        };
        // Off by default: nothing is offered.
        assert!(config.sub_servers_for("play.projects.gg").is_empty());
        config.features.sub_servers = true;
        assert_eq!(config.sub_servers_for("play.projects.gg").len(), 6);
        // Another name of the network and a port still find Projects' list.
        assert_eq!(config.sub_servers_for("mc.projects.gg:25565").len(), 6);
        assert!(config.sub_servers_for("mc.example.net").is_empty());
        assert_eq!(
            config
                .sub_server("play.projects.gg", "skyblock")
                .map(|s| s.command.as_str()),
            Some("/aurora")
        );
        assert!(config.sub_server("play.projects.gg", "lobby").is_none());
        // The entry with the same host comes first.
        let mut test = entry("test.projects.gg", vec![sub("build", "Build", "/build")]);
        test.id = "test".into();
        config.servers.push(test);
        assert_eq!(config.sub_servers_for("test.projects.gg")[0].id, "build");
        assert_eq!(config.sub_servers_for("play.projects.gg")[0].id, "skyblock");
    }

    #[test]
    fn the_wire_shape_is_compact_and_older_configs_read() {
        let server: ServerEntry =
            serde_json::from_str(r#"{"id":"main","name":"Projects","address":"play.projects.gg"}"#)
                .unwrap();
        assert!(server.sub_servers.is_empty());
        let written = serde_json::to_value(&server).unwrap();
        assert!(written.get("subServers").is_none(), "left out while empty");
        let with = entry("play.projects.gg", today());
        let written = serde_json::to_value(&with).unwrap();
        assert_eq!(
            written["subServers"][0],
            serde_json::json!({"id":"skyblock","name":"SkyBlock","command":"/aurora"})
        );
        assert_eq!(written["subServers"][4]["testersOnly"], true);
        let back: ServerEntry = serde_json::from_value(written).unwrap();
        assert_eq!(back, with);
        let features = serde_json::to_value(super::super::Features::default()).unwrap();
        assert!(
            features.get("subServers").is_none(),
            "the switch is written only when on"
        );
    }
}
