//! The player's server list from another launcher, merged into an instance's: the Projects
//! server first (once, never twice), then the instance's own servers, then the player's in their
//! order. A server already listed (same address, the default port and case aside) is not added
//! again, so running the import twice adds nothing.

use pl_mc::server::{ServerList, ServerListEntry};

/// An address as the game reaches it: no case, no trailing dot, no default port.
pub fn address_key(address: &str) -> String {
    let lower = address.trim().to_ascii_lowercase();
    let host = lower.strip_suffix(":25565").unwrap_or(&lower);
    host.trim_end_matches('.').to_owned()
}

/// What [`merge_server_lists`] made.
#[derive(Debug, Clone, PartialEq)]
pub struct ServersMerge {
    pub list: ServerList,
    /// The player's servers added.
    pub added: usize,
    /// The Projects server was put in.
    pub added_projects: bool,
}

/// `ours` (the instance's list, possibly empty) with `theirs` (the player's) merged in.
/// `projects` is the server every instance lists (name, address); `None` leaves it out.
pub fn merge_server_lists(
    ours: &ServerList,
    theirs: &ServerList,
    projects: Option<(&str, &str)>,
) -> ServersMerge {
    let mut list = ours.clone();
    let mut seen: Vec<String> = list.servers.iter().map(|s| address_key(&s.ip)).collect();
    let mut added_projects = false;
    if let Some((name, address)) = projects {
        let key = address_key(address);
        // Already in the instance's list: it stays where it is. Otherwise it goes first, as the
        // player's own entry (their name and icon) when they listed it.
        if !seen.contains(&key) {
            let theirs_entry = theirs
                .servers
                .iter()
                .find(|s| !s.hidden && address_key(&s.ip) == key)
                .cloned();
            added_projects = theirs_entry.is_none();
            list.servers.insert(
                0,
                theirs_entry.unwrap_or_else(|| ServerListEntry::new(name, address)),
            );
            seen.push(key);
        }
    }
    // Hidden entries stand for the last "Direct Connection"; they stay at the end.
    let hidden_at = list
        .servers
        .iter()
        .position(|s| s.hidden)
        .unwrap_or(list.servers.len());
    let mut added = 0;
    let mut insert_at = hidden_at;
    for server in theirs.servers.iter().filter(|s| !s.hidden) {
        let key = address_key(&server.ip);
        if key.is_empty() || seen.contains(&key) {
            continue;
        }
        list.servers.insert(insert_at, server.clone());
        insert_at += 1;
        seen.push(key);
        added += 1;
    }
    ServersMerge {
        list,
        added,
        added_projects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECTS: Option<(&str, &str)> = Some(("Projects", "play.projects.gg"));

    fn list(entries: &[(&str, &str, bool)]) -> ServerList {
        let mut list = ServerList::default();
        for (name, ip, hidden) in entries {
            let mut entry = ServerListEntry::new(*name, *ip);
            entry.hidden = *hidden;
            list.servers.push(entry);
        }
        list
    }

    fn names(list: &ServerList) -> Vec<String> {
        list.servers
            .iter()
            .map(|s| {
                format!(
                    "{}={}{}",
                    s.name,
                    s.ip,
                    if s.hidden { " (hidden)" } else { "" }
                )
            })
            .collect()
    }

    #[test]
    fn projects_first_then_the_players_servers_without_doubles() {
        let ours = list(&[]);
        let theirs = list(&[
            ("Hypixel", "mc.hypixel.net", false),
            ("Arkadaşlar", "192.168.1.20:25565", false),
            ("Aynı", "MC.HYPIXEL.NET.", false),
            ("", "direct.example", true),
        ]);
        let merged = merge_server_lists(&ours, &theirs, PROJECTS);
        assert_eq!(
            names(&merged.list),
            [
                "Projects=play.projects.gg",
                "Hypixel=mc.hypixel.net",
                "Arkadaşlar=192.168.1.20:25565",
            ]
        );
        assert_eq!((merged.added, merged.added_projects), (2, true));

        // Running it again adds nothing.
        let again = merge_server_lists(&merged.list, &theirs, PROJECTS);
        assert_eq!(names(&again.list), names(&merged.list));
        assert_eq!((again.added, again.added_projects), (0, false));
    }

    #[test]
    fn a_seeded_list_keeps_its_order_and_the_players_projects_entry_is_theirs() {
        let ours = list(&[
            ("Projects", "play.projects.gg", false),
            ("", "last.example", true),
        ]);
        let theirs = list(&[
            ("Benim sunucum", "PLAY.projects.gg:25565", false),
            ("Lobi", "lobi.example", false),
        ]);
        let merged = merge_server_lists(&ours, &theirs, PROJECTS);
        assert_eq!(
            names(&merged.list),
            [
                "Projects=play.projects.gg",
                "Lobi=lobi.example",
                "=last.example (hidden)",
            ]
        );
        // An empty instance list and a player who lists Projects themselves: their entry leads.
        let fresh = merge_server_lists(&list(&[]), &theirs, PROJECTS);
        assert_eq!(
            names(&fresh.list),
            ["Benim sunucum=PLAY.projects.gg:25565", "Lobi=lobi.example"]
        );
        assert!(!fresh.added_projects);
    }
}
