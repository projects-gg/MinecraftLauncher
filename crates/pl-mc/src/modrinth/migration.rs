//! Moving the player's own mods to another Minecraft version. One `version_files/update`
//! request answers for every file at once which mods have a version for the target: it carries
//! the own mods' hashes and the target set's hashes (their projects show which mods the target
//! already has), never one request per mod.

use std::collections::{HashMap, HashSet};

use super::{ModrinthClient, ModrinthHashAlgorithm, ModrinthResolveTarget, ModrinthVersion};
use crate::error::Result;

/// What one of the player's own mods can do on the target version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationFit {
    /// Modrinth has a version of the mod for the target: installing it moves the mod.
    Compatible(Box<ModrinthVersion>),
    /// The target already has the mod (the same file or the same Modrinth project).
    AlreadyThere,
    /// No version for the target, or a file Modrinth does not know.
    Unavailable,
}

impl MigrationFit {
    /// The version to install, for a compatible mod.
    pub fn version(&self) -> Option<&ModrinthVersion> {
        match self {
            Self::Compatible(version) => Some(version),
            Self::AlreadyThere | Self::Unavailable => None,
        }
    }
}

/// The hashes one request asks about: the own mods first, then the target's files; each hash
/// once, empty ones (files that could not be read) left out.
pub fn migration_hashes(own: &[String], target_files: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    own.iter()
        .chain(target_files)
        .filter(|hash| !hash.is_empty())
        .filter(|hash| seen.insert(hash.as_str()))
        .cloned()
        .collect()
}

/// Sorts each own mod (`own[i]` → result `[i]`, by SHA-1) using one `version_files/update`
/// answer (`hash → newest version for the target`) that covered the own mods and the target
/// set's files (`target_files`).
pub fn migration_fits(
    own: &[String],
    target_files: &[String],
    newest: &HashMap<String, ModrinthVersion>,
) -> Vec<MigrationFit> {
    let files: HashSet<&str> = target_files
        .iter()
        .map(String::as_str)
        .filter(|hash| !hash.is_empty())
        .collect();
    let projects: HashSet<&str> = files
        .iter()
        .filter_map(|hash| newest.get(*hash))
        .map(|version| version.project_id.as_str())
        .collect();
    own.iter()
        .map(|hash| {
            if hash.is_empty() {
                return MigrationFit::Unavailable;
            }
            if files.contains(hash.as_str()) {
                return MigrationFit::AlreadyThere;
            }
            match newest.get(hash) {
                Some(version) if projects.contains(version.project_id.as_str()) => {
                    MigrationFit::AlreadyThere
                }
                Some(version) => MigrationFit::Compatible(Box::new(version.clone())),
                None => MigrationFit::Unavailable,
            }
        })
        .collect()
}

/// The mods to move, in order: `(index into own, version to install)` for each compatible
/// mod; a second file of the same project (a duplicate jar) moves once.
pub fn migration_moves(fits: &[MigrationFit]) -> Vec<(usize, &ModrinthVersion)> {
    let mut projects = HashSet::new();
    fits.iter()
        .enumerate()
        .filter_map(|(index, fit)| fit.version().map(|version| (index, version)))
        .filter(|(_, version)| projects.insert(version.project_id.as_str()))
        .collect()
}

impl ModrinthClient {
    /// Which of `own` (the SHA-1s of the player's own mods) have a version for `target`, with
    /// one `version_files/update` request that also covers `target_files` (the SHA-1s of the
    /// target set's mods). Without a loader (vanilla) nothing is asked: no mod can move.
    pub async fn migration_check(
        &self,
        own: &[String],
        target_files: &[String],
        target: &ModrinthResolveTarget,
    ) -> Result<Vec<MigrationFit>> {
        let asks = own.iter().any(|hash| !hash.is_empty());
        if target.loaders.is_empty() || !asks {
            return Ok(migration_fits(own, target_files, &HashMap::new()));
        }
        let hashes = migration_hashes(own, target_files);
        let newest = self
            .update_check(
                &hashes,
                ModrinthHashAlgorithm::Sha1,
                &target.loaders,
                &target.game_versions,
            )
            .await?;
        Ok(migration_fits(own, target_files, &newest))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{Value, json};

    use super::super::decode;
    use super::super::tests::{Handler, client_for, mock_api, version_json};
    use super::*;
    use crate::loaders::LoaderKind;

    fn version(id: &str, project: &str) -> ModrinthVersion {
        decode(
            version_json(id, project, "release", json!([]), &format!("{id}-sha")),
            "test version",
        )
        .unwrap()
    }

    fn hashes(list: &[&str]) -> Vec<String> {
        list.iter().map(|hash| (*hash).to_owned()).collect()
    }

    #[test]
    fn hashes_are_asked_once_own_first() {
        let own = hashes(&["a", "b", "", "a"]);
        let target = hashes(&["c", "b", ""]);
        assert_eq!(migration_hashes(&own, &target), hashes(&["a", "b", "c"]));
        assert!(migration_hashes(&[], &[]).is_empty());
    }

    #[test]
    fn fits_sort_compatible_already_there_and_unavailable() {
        // own: sodium (compatible), iris (the target has iris from the pack), a file Modrinth
        // does not know, an unreadable file and a jar that sits in both sets unchanged.
        let own = hashes(&["sodium-old", "iris-old", "mystery", "", "shared"]);
        let target = hashes(&["iris-new", "pack-only", "shared"]);
        let newest: HashMap<String, ModrinthVersion> = [
            ("sodium-old", version("sodium-1214", "sodium")),
            ("iris-old", version("iris-1214", "iris")),
            ("iris-new", version("iris-1214", "iris")),
            ("pack-only", version("lithium-1214", "lithium")),
        ]
        .into_iter()
        .map(|(hash, version)| (hash.to_owned(), version))
        .collect();

        let fits = migration_fits(&own, &target, &newest);
        assert_eq!(fits.len(), own.len());
        assert_eq!(
            fits[0].version().map(|v| v.id.as_str()),
            Some("sodium-1214")
        );
        assert_eq!(fits[1], MigrationFit::AlreadyThere);
        assert_eq!(fits[2], MigrationFit::Unavailable);
        assert_eq!(fits[3], MigrationFit::Unavailable);
        assert_eq!(fits[4], MigrationFit::AlreadyThere);
        assert_eq!(moves(&fits), vec![(0, "sodium-1214")]);
    }

    fn moves(fits: &[MigrationFit]) -> Vec<(usize, &str)> {
        migration_moves(fits)
            .into_iter()
            .map(|(index, version)| (index, version.id.as_str()))
            .collect()
    }

    #[test]
    fn duplicate_jars_of_one_project_install_once() {
        let own = hashes(&["zoom-a", "zoom-b", "menu"]);
        let newest: HashMap<String, ModrinthVersion> = [
            ("zoom-a", version("zoom-2", "zoomify")),
            ("zoom-b", version("zoom-2", "zoomify")),
            ("menu", version("menu-5", "modmenu")),
        ]
        .into_iter()
        .map(|(hash, version)| (hash.to_owned(), version))
        .collect();
        let fits = migration_fits(&own, &[], &newest);
        assert!(fits.iter().all(|fit| fit.version().is_some()));
        assert_eq!(moves(&fits), vec![(0, "zoom-2"), (2, "menu-5")]);
    }

    #[tokio::test]
    async fn check_makes_one_update_request_for_every_file() {
        let paths = Arc::new(parking_lot::Mutex::new(Vec::<(String, String)>::new()));
        let log = Arc::clone(&paths);
        let handler: Handler = Arc::new(move |method, url| {
            log.lock().push((method.to_owned(), url.path().to_owned()));
            let body: Value = json!({
                "own-sodium": version_json("sodium-1214", "sodium", "release", json!([]), "s"),
                "own-iris": version_json("iris-1214", "iris", "release", json!([]), "i"),
                "target-iris": version_json("iris-1214", "iris", "release", json!([]), "i"),
            });
            (200, Vec::new(), body.to_string())
        });
        let client = client_for(&mock_api(handler).await);
        let target = ModrinthResolveTarget::for_loader(LoaderKind::Fabric, "1.21.4");

        let own = hashes(&["own-sodium", "own-iris", "own-unknown"]);
        let fits = client
            .migration_check(&own, &hashes(&["target-iris"]), &target)
            .await
            .unwrap();
        assert_eq!(
            fits[0].version().map(|v| v.id.as_str()),
            Some("sodium-1214")
        );
        assert_eq!(fits[1], MigrationFit::AlreadyThere);
        assert_eq!(fits[2], MigrationFit::Unavailable);
        assert_eq!(
            *paths.lock(),
            vec![("POST".to_owned(), "/v2/version_files/update".to_owned())]
        );

        // No loader (vanilla) or nothing to ask about: no request at all.
        let vanilla = ModrinthResolveTarget::for_loader(LoaderKind::Vanilla, "1.21.4");
        let fits = client.migration_check(&own, &[], &vanilla).await.unwrap();
        assert!(fits.iter().all(|fit| *fit == MigrationFit::Unavailable));
        let fits = client
            .migration_check(&hashes(&[""]), &[], &target)
            .await
            .unwrap();
        assert_eq!(fits, vec![MigrationFit::Unavailable]);
        assert_eq!(paths.lock().len(), 1);
    }
}
