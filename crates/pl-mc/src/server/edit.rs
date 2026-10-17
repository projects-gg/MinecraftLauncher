//! Edits the launcher makes to an instance's server list for the player (the instance page's
//! servers tab): add, rename or re-address, remove, and put a removed server back ("Geri al").
//!
//! Positions count listed servers only. Hidden entries (the game keeps its "Direct Connection"
//! history there) are never shown, never moved and never matched. Every edit that points at a
//! server also names the address the player saw there, so an edit made against a list the game
//! has changed meanwhile finds its server again or fails instead of hitting another one.

use serde::{Deserialize, Serialize};

use super::ping::ServerAddress;
use super::servers_dat::{ServerList, ServerListEntry};

/// Longest name the launcher writes (the game's own field stops there too).
pub const MAX_SERVER_NAME: usize = 64;
/// Longest address the launcher writes.
pub const MAX_SERVER_ADDRESS: usize = 255;

/// One change to the listed servers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ServerEdit {
    /// A new server at the end of the list, as the game adds them. An empty name takes the
    /// address.
    #[serde(rename_all = "camelCase")]
    Add { name: String, address: String },
    /// A listed server's name and address (a new address drops the old server's icon).
    #[serde(rename_all = "camelCase")]
    Update {
        index: u32,
        /// The address the player saw at `index`.
        was: String,
        name: String,
        address: String,
    },
    /// Takes a listed server off; [`ServerList::apply`] returns it for "Geri al".
    #[serde(rename_all = "camelCase")]
    Remove { index: u32, was: String },
    /// Puts a removed server back at its listed position (clamped to the end).
    #[serde(rename_all = "camelCase")]
    Restore { index: u32, server: ServerListEntry },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServerEditError {
    #[error("not a server address: {0:?}")]
    BadAddress(String),
    #[error("a listed server already has the address {0:?}")]
    Duplicate(String),
    #[error("no listed server at {index} (or anywhere) has the address {was:?}")]
    Changed { index: u32, was: String },
}

/// The address as the game stores it: trimmed, checked, at most [`MAX_SERVER_ADDRESS`] long.
fn clean_address(address: &str) -> Result<String, ServerEditError> {
    let address = address.trim();
    if address.chars().count() > MAX_SERVER_ADDRESS || ServerAddress::parse(address).is_err() {
        return Err(ServerEditError::BadAddress(address.to_owned()));
    }
    Ok(address.to_owned())
}

/// The name as written: trimmed, the address when empty, at most [`MAX_SERVER_NAME`] long.
fn clean_name(name: &str, address: &str) -> String {
    let name = name.trim();
    let name = if name.is_empty() { address } else { name };
    name.chars().take(MAX_SERVER_NAME).collect()
}

fn same_address(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl ServerList {
    /// The servers the game lists (hidden entries left out), in order.
    pub fn listed(&self) -> impl Iterator<Item = &ServerListEntry> {
        self.servers.iter().filter(|s| !s.hidden)
    }

    /// Where the listed server number `index` sits in [`ServerList::servers`].
    fn position_of_listed(&self, index: usize) -> Option<usize> {
        self.servers
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.hidden)
            .nth(index)
            .map(|(position, _)| position)
    }

    /// The listed server the player meant: the one at `index` when it still has the address
    /// they saw, else the first listed server with that address.
    fn find_listed(&self, index: u32, was: &str) -> Result<usize, ServerEditError> {
        let at_index = self
            .position_of_listed(index as usize)
            .filter(|&p| same_address(&self.servers[p].ip, was));
        at_index
            .or_else(|| {
                self.servers
                    .iter()
                    .position(|s| !s.hidden && same_address(&s.ip, was))
            })
            .ok_or_else(|| ServerEditError::Changed {
                index,
                was: was.to_owned(),
            })
    }

    fn ensure_unique(&self, address: &str, except: Option<usize>) -> Result<(), ServerEditError> {
        let taken = self
            .servers
            .iter()
            .enumerate()
            .any(|(p, s)| Some(p) != except && !s.hidden && same_address(&s.ip, address));
        if taken {
            Err(ServerEditError::Duplicate(address.to_owned()))
        } else {
            Ok(())
        }
    }

    /// Applies one edit. A removal returns the server it took off (unknown tags included while
    /// it stays in memory).
    pub fn apply(&mut self, edit: ServerEdit) -> Result<Option<ServerListEntry>, ServerEditError> {
        match edit {
            ServerEdit::Add { name, address } => {
                let address = clean_address(&address)?;
                self.ensure_unique(&address, None)?;
                let name = clean_name(&name, &address);
                self.servers.push(ServerListEntry::new(name, address));
                Ok(None)
            }
            ServerEdit::Update {
                index,
                was,
                name,
                address,
            } => {
                let position = self.find_listed(index, &was)?;
                let address = clean_address(&address)?;
                self.ensure_unique(&address, Some(position))?;
                let entry = &mut self.servers[position];
                if !same_address(&entry.ip, &address) {
                    // The icon (and the resource pack answer) belonged to the old server.
                    entry.icon = None;
                    entry.accept_textures = None;
                }
                entry.name = clean_name(&name, &address);
                entry.ip = address;
                Ok(None)
            }
            ServerEdit::Remove { index, was } => {
                let position = self.find_listed(index, &was)?;
                Ok(Some(self.servers.remove(position)))
            }
            ServerEdit::Restore { index, mut server } => {
                server.ip = clean_address(&server.ip)?;
                self.ensure_unique(&server.ip, None)?;
                server.name = clean_name(&server.name, &server.ip);
                server.hidden = false;
                let position = self
                    .position_of_listed(index as usize)
                    .unwrap_or(self.servers.len());
                self.servers.insert(position, server);
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> ServerList {
        let mut hidden = ServerListEntry::new("Doğrudan bağlantı", "10.0.0.5");
        hidden.hidden = true;
        let mut projects = ServerListEntry::new("Projects", "play.projects.gg");
        projects.icon = Some("aWNvbg==".into());
        projects.accept_textures = Some(true);
        let mut list = ServerList::default();
        list.servers.extend([
            projects,
            hidden,
            ServerListEntry::new("Hypixel", "mc.hypixel.net"),
        ]);
        list
    }

    fn names(list: &ServerList) -> Vec<&str> {
        list.listed().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn adds_at_the_end_and_names_by_address() {
        let mut list = list();
        list.apply(ServerEdit::Add {
            name: "  ".into(),
            address: " oyna.ornek.com:25566 ".into(),
        })
        .unwrap();
        assert_eq!(
            names(&list),
            ["Projects", "Hypixel", "oyna.ornek.com:25566"]
        );
        assert_eq!(list.servers.last().unwrap().ip, "oyna.ornek.com:25566");
    }

    #[test]
    fn refuses_bad_and_duplicate_addresses() {
        let mut list = list();
        let bad = list.apply(ServerEdit::Add {
            name: "x".into(),
            address: "play projects".into(),
        });
        assert!(matches!(bad, Err(ServerEditError::BadAddress(_))));
        let twice = list.apply(ServerEdit::Add {
            name: "x".into(),
            address: "PLAY.projects.gg".into(),
        });
        assert!(matches!(twice, Err(ServerEditError::Duplicate(_))));
        // A hidden entry's address does not count.
        list.apply(ServerEdit::Add {
            name: "Ev".into(),
            address: "10.0.0.5".into(),
        })
        .unwrap();
        assert_eq!(names(&list), ["Projects", "Hypixel", "Ev"]);
    }

    #[test]
    fn positions_skip_hidden_entries() {
        let mut list = list();
        let removed = list
            .apply(ServerEdit::Remove {
                index: 1,
                was: "mc.hypixel.net".into(),
            })
            .unwrap()
            .unwrap();
        assert_eq!(removed.name, "Hypixel");
        assert_eq!(list.servers.len(), 2);
        assert!(list.servers[1].hidden);
        list.apply(ServerEdit::Restore {
            index: 1,
            server: removed,
        })
        .unwrap();
        assert_eq!(names(&list), ["Projects", "Hypixel"]);
    }

    #[test]
    fn restores_at_the_old_place() {
        let mut list = list();
        let removed = list
            .apply(ServerEdit::Remove {
                index: 0,
                was: "play.projects.gg".into(),
            })
            .unwrap()
            .unwrap();
        list.apply(ServerEdit::Restore {
            index: 0,
            server: removed,
        })
        .unwrap();
        assert_eq!(names(&list), ["Projects", "Hypixel"]);
        // Everything it had comes back (the hidden entry may now sit before it: never shown).
        let projects = list.listed().next().unwrap();
        assert_eq!(projects.icon.as_deref(), Some("aWNvbg=="));
        assert_eq!(projects.accept_textures, Some(true));
    }

    #[test]
    fn finds_the_server_when_the_list_moved() {
        let mut list = list();
        // The player saw Hypixel at 1; the game put it first meanwhile.
        list.servers.swap(0, 2);
        list.apply(ServerEdit::Remove {
            index: 1,
            was: "mc.hypixel.net".into(),
        })
        .unwrap();
        assert_eq!(names(&list), ["Projects"]);
        let gone = list.apply(ServerEdit::Remove {
            index: 0,
            was: "mc.hypixel.net".into(),
        });
        assert!(matches!(gone, Err(ServerEditError::Changed { .. })));
    }

    #[test]
    fn a_new_address_drops_the_old_icon() {
        let mut list = list();
        list.apply(ServerEdit::Update {
            index: 0,
            was: "play.projects.gg".into(),
            name: "Projects".into(),
            address: "play.projects.gg".into(),
        })
        .unwrap();
        assert!(list.servers[0].icon.is_some());
        list.apply(ServerEdit::Update {
            index: 0,
            was: "play.projects.gg".into(),
            name: "Yeni".into(),
            address: "yeni.projects.gg".into(),
        })
        .unwrap();
        assert_eq!(list.servers[0].name, "Yeni");
        assert_eq!(list.servers[0].icon, None);
        assert_eq!(list.servers[0].accept_textures, None);
        let clash = list.apply(ServerEdit::Update {
            index: 0,
            was: "yeni.projects.gg".into(),
            name: "x".into(),
            address: "mc.hypixel.net".into(),
        });
        assert!(matches!(clash, Err(ServerEditError::Duplicate(_))));
    }

    #[test]
    fn long_names_are_cut() {
        let mut list = ServerList::default();
        list.apply(ServerEdit::Add {
            name: "ş".repeat(100),
            address: "a.b".into(),
        })
        .unwrap();
        assert_eq!(list.servers[0].name.chars().count(), MAX_SERVER_NAME);
    }
}
