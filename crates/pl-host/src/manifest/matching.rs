//! Install matching (PROTOCOL.md §10.2): which of the guest's installs joins a host's world, or
//! what has to happen first.
//!
//! The guest computes the same manifest for each of its installs and calls [`decide`]. The guest's
//! own installs are never changed: when none fits, a copy is made for the host ("Ayşe'nin
//! dünyası"), marked with the host's friend id in the install's own metadata.

use std::cmp::Ordering;
use std::collections::HashSet;

use super::{Manifest, ModRef, PackRef, mods_hash};

/// One of the guest's installs, as matching sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallInfo {
    /// The launcher's id for the install.
    pub id: String,
    pub mc: String,
    /// The loader kind (`fabric`); `None` for an install without one.
    pub loader: Option<String>,
    /// The pack the install follows, if any.
    pub pack: Option<PackRef>,
    /// Its enabled both-side mods (the same rules as the host's manifest).
    pub mods: Vec<ModRef>,
    /// The friend id of the host this install was copied for (`copyOf`), if it is such a copy.
    pub copy_of: Option<String>,
    /// When it was last played (Unix seconds; 0 never).
    pub last_played: i64,
}

impl InstallInfo {
    /// The official install: one that follows the official pack and is not a copy.
    pub fn is_official(&self, family: &str) -> bool {
        self.copy_of.is_none() && self.pack.as_ref().is_some_and(|p| p.family == family)
    }
}

/// What the guest's launcher does before joining.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Join with this install. `warn_unknown` > 0: "2 mod bulunamadı; girerken sorun çıkabilir."
    Use { install: String, warn_unknown: u32 },
    /// Install the official pack at this Minecraft version (LIB1's "Kur"), then decide again.
    Install { mc: String },
    /// The normal pack sync of this install, then decide again.
    UpdatePack { install: String },
    /// "Ayşe'nin paketi eski. Ayşe oyunu yeniden açınca katılabilirsin."
    HostPackOld,
    /// The host's pack version is unknown here: refresh the signed content once and decide again
    /// (with `content_refreshed`).
    RefreshContent,
    /// Still unknown after a refresh: "Bağlanılamadı. Tekrar dene."
    Unavailable,
    /// Reinstall this earlier copy's mod set, then use it.
    UpdateCopy { install: String },
    /// Make a new install for the host: the official pack at the host's version, the host's `sets`
    /// and exactly the manifest's mods; `add` are the mods beyond what the official install has
    /// (the count and download size the offer shows).
    Copy { add: Vec<ModRef>, sets: Vec<String> },
}

/// Decides which install joins `host`'s world.
///
/// `latest_pack`: the pack version the guest's signed content names for the host's Minecraft
/// version. `host_id`: the host's friend id (copies remember it).
pub fn decide(
    host: &Manifest,
    installs: &[InstallInfo],
    latest_pack: Option<&str>,
    host_id: &str,
    content_refreshed: bool,
) -> Decision {
    // The host's pack first: whether anyone can join it does not depend on the guest's installs, so
    // nothing is installed for a world that cannot be joined anyway.
    if let Some(pack) = &host.pack {
        match latest_pack.map(|latest| compare_versions(&pack.version, latest)) {
            Some(Ordering::Equal) => {}
            Some(Ordering::Less) => return Decision::HostPackOld,
            _ if content_refreshed => return Decision::Unavailable,
            _ => return Decision::RefreshContent,
        }
    }

    // Same Minecraft version and loader.
    let fitting: Vec<&InstallInfo> = installs
        .iter()
        .filter(|i| i.mc == host.mc && i.loader.as_deref() == Some(host.loader.kind.as_str()))
        .collect();
    if fitting.is_empty() {
        return Decision::Install {
            mc: host.mc.clone(),
        };
    }

    let official = host.pack.as_ref().and_then(|pack| {
        fitting
            .iter()
            .copied()
            .filter(|i| i.is_official(&pack.family))
            .max_by_key(|i| i.last_played)
    });

    // The host's pack is the latest: an official install behind it syncs first.
    if let (Some(pack), Some(install)) = (&host.pack, official) {
        let behind = install
            .pack
            .as_ref()
            .is_some_and(|p| compare_versions(&p.version, &pack.version) == Ordering::Less);
        if behind {
            return Decision::UpdatePack {
                install: install.id.clone(),
            };
        }
    }

    let wanted: HashSet<(&str, &str)> = host
        .mods
        .iter()
        .map(|m| (m.project.as_str(), m.version.as_str()))
        .collect();
    let contains_all = |install: &InstallInfo| {
        let have: HashSet<(&str, &str)> = install
            .mods
            .iter()
            .map(|m| (m.project.as_str(), m.version.as_str()))
            .collect();
        wanted.is_subset(&have)
    };
    let use_it = |install: &InstallInfo| Decision::Use {
        install: install.id.clone(),
        warn_unknown: host.unknown,
    };

    // 1. A copy made for this host whose mods equal the manifest's.
    let copies: Vec<&InstallInfo> = by_recent(
        fitting
            .iter()
            .copied()
            .filter(|i| i.copy_of.as_deref() == Some(host_id)),
    );
    if let Some(copy) = copies.iter().find(|i| mods_hash(&i.mods) == host.hash) {
        return use_it(copy);
    }
    // 2. The official install, when it has every mod of the manifest.
    if let Some(install) = official.filter(|i| contains_all(i)) {
        return use_it(install);
    }
    // 3. Any other install that has them all, most recently played first.
    let others = by_recent(
        fitting
            .iter()
            .copied()
            .filter(|i| Some(i.id.as_str()) != official.map(|o| o.id.as_str())),
    );
    if let Some(install) = others.into_iter().find(|i| contains_all(i)) {
        return use_it(install);
    }

    // None fits: refresh the earlier copy, or offer a new one.
    if let Some(copy) = copies.first() {
        return Decision::UpdateCopy {
            install: copy.id.clone(),
        };
    }
    let base: HashSet<(&str, &str)> = official
        .map(|i| {
            i.mods
                .iter()
                .map(|m| (m.project.as_str(), m.version.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let add = host
        .mods
        .iter()
        .filter(|m| !base.contains(&(m.project.as_str(), m.version.as_str())))
        .cloned()
        .collect();
    Decision::Copy {
        add,
        sets: host.sets.clone(),
    }
}

fn by_recent<'a>(installs: impl Iterator<Item = &'a InstallInfo>) -> Vec<&'a InstallInfo> {
    let mut list: Vec<&InstallInfo> = installs.collect();
    list.sort_by_key(|install| std::cmp::Reverse(install.last_played));
    list
}

/// Compares two pack versions: dot-separated parts, numerically where both parts are numbers,
/// otherwise as text.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let mut left = a.split('.');
    let mut right = b.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (Some(x), None) => {
                return if x.chars().all(|c| c == '0') {
                    Ordering::Equal
                } else {
                    Ordering::Greater
                };
            }
            (None, Some(y)) => {
                return if y.chars().all(|c| c == '0') {
                    Ordering::Equal
                } else {
                    Ordering::Less
                };
            }
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    _ => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::manifest::Loader;
    use crate::testdata;

    fn install(value: &Value) -> InstallInfo {
        let loader: Loader = serde_json::from_value(value["loader"].clone()).unwrap();
        InstallInfo {
            id: value["name"].as_str().unwrap().to_owned(),
            mc: value["mc"].as_str().unwrap().to_owned(),
            loader: Some(loader.kind),
            pack: serde_json::from_value(value["pack"].clone()).unwrap(),
            mods: serde_json::from_value(value["mods"].clone()).unwrap(),
            copy_of: value["copyOf"].as_str().map(str::to_owned),
            last_played: value["lastPlayed"].as_i64().unwrap_or(0),
        }
    }

    #[test]
    fn scenarios() {
        let file = testdata::json("manifest/match.json");
        let scenarios = file["scenarios"].as_array().unwrap();
        assert_eq!(scenarios.len(), 11);
        for scenario in scenarios {
            let case = scenario["case"].as_str().unwrap();
            let host: Manifest = serde_json::from_value(scenario["host"].clone()).unwrap();
            assert!(host.hash_ok(), "{case}");
            let installs: Vec<InstallInfo> = scenario["installs"]
                .as_array()
                .unwrap()
                .iter()
                .map(install)
                .collect();
            let decision = decide(
                &host,
                &installs,
                scenario["latestPack"].as_str(),
                scenario["hostId"].as_str().unwrap(),
                false,
            );
            let expect = &scenario["expect"];
            let wanted = match expect["action"].as_str().unwrap() {
                "use" => Decision::Use {
                    install: expect["install"].as_str().unwrap().to_owned(),
                    warn_unknown: expect["warnUnknown"].as_u64().unwrap_or(0) as u32,
                },
                "update-pack" => Decision::UpdatePack {
                    install: expect["install"].as_str().unwrap().to_owned(),
                },
                "update-copy" => Decision::UpdateCopy {
                    install: expect["install"].as_str().unwrap().to_owned(),
                },
                "host-pack-old" => Decision::HostPackOld,
                "refresh-content" => Decision::RefreshContent,
                "install" => Decision::Install {
                    mc: expect["mc"].as_str().unwrap().to_owned(),
                },
                "copy" => Decision::Copy {
                    add: expect["add"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|m| {
                            ModRef::new(
                                m["project"].as_str().unwrap(),
                                m["version"].as_str().unwrap(),
                            )
                        })
                        .collect(),
                    sets: serde_json::from_value(expect["sets"].clone()).unwrap(),
                },
                other => panic!("unknown action {other}"),
            };
            assert_eq!(decision, wanted, "{case}");
        }
    }

    #[test]
    fn refreshed_content_that_still_does_not_know_the_pack() {
        let file = testdata::json("manifest/match.json");
        let scenario = file["scenarios"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["expect"]["action"] == "refresh-content")
            .unwrap();
        let host: Manifest = serde_json::from_value(scenario["host"].clone()).unwrap();
        let installs: Vec<InstallInfo> = scenario["installs"]
            .as_array()
            .unwrap()
            .iter()
            .map(install)
            .collect();
        assert_eq!(
            decide(&host, &installs, Some("2.3.0"), "1", true),
            Decision::Unavailable
        );
        assert_eq!(
            decide(&host, &installs, None, "1", false),
            Decision::RefreshContent
        );
    }

    #[test]
    fn versions_compare_by_parts() {
        assert_eq!(compare_versions("2.3.0", "2.3.0"), Ordering::Equal);
        assert_eq!(compare_versions("2.10.0", "2.9.0"), Ordering::Greater);
        assert_eq!(compare_versions("2.3", "2.3.0"), Ordering::Equal);
        assert_eq!(compare_versions("2.2.0", "2.3.0"), Ordering::Less);
        assert_eq!(compare_versions("2.3.1", "2.3"), Ordering::Greater);
    }
}
