//! What a source's rows weigh, read on demand (the first-run screen shows its rows at once and
//! the sizes as they come): folder sizes, and which mods fit the version a profile runs with.
//! Mods are judged by the mod health check (`pl_mc::mods::check`): a jar made for another
//! loader or Minecraft version, or one that is not a readable jar, stays behind.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use pl_mc::loaders::LoaderKind;
use pl_mc::mods::check::{CheckEnvironment, FileOrigins, IssueKind, check_mods_dir};
use pl_mc::mods::scan::ModScanner;

use crate::copy::{no_skip, tree_size};
use crate::model::{ImportProfile, ImportSource, ProfileMeasure, SharedSizes, SourceMeasure};

/// Sizes and mod counts of every row of `source` (blocking: walks folders, reads mod jars).
pub fn measure(source: &ImportSource) -> SourceMeasure {
    let shared = source.shared.as_ref().map(|_| shared_sizes(&source.root));
    let profiles = source
        .profiles
        .iter()
        .map(|profile| measure_profile(source, profile))
        .collect();
    SourceMeasure {
        source_id: source.id.clone(),
        shared,
        profiles,
    }
}

pub(crate) fn shared_sizes(root: &Path) -> SharedSizes {
    SharedSizes {
        worlds: crate::worlds::list_worlds(&root.join("saves"))
            .iter()
            .map(|w| tree_size(&w.path, &no_skip))
            .sum(),
        resource_packs: entries_size(&root.join("resourcepacks"), crate::detect::is_pack_entry),
        shader_packs: tree_size(&root.join("shaderpacks"), &no_skip),
        screenshots: entries_size(
            &root.join("screenshots"),
            crate::detect::is_screenshot_entry,
        ),
    }
}

fn entries_size(dir: &Path, keep: fn(&std::fs::DirEntry) -> bool) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(keep)
                .map(|e| tree_size(&e.path(), &no_skip))
                .sum()
        })
        .unwrap_or(0)
}

fn measure_profile(source: &ImportSource, profile: &ImportProfile) -> ProfileMeasure {
    let mut out = ProfileMeasure {
        id: profile.id.clone(),
        ..ProfileMeasure::default()
    };
    if !profile.supported() {
        return out;
    }
    let mods = plan_mods(profile);
    out.mods = mods.take.len() as u32;
    out.left_mods = mods.left;
    out.bytes = mods.bytes;
    if profile.own_folder {
        out.bytes += tree_size(&profile.game_dir, &instance_skip);
        out.worlds = crate::worlds::count_worlds(&profile.game_dir.join("saves"));
    } else if profile.loader != LoaderKind::Vanilla {
        out.bytes += tree_size(&source.root.join("config"), &no_skip);
    }
    out
}

/// The mod files a profile brings and how many stay behind.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ModPlan {
    pub take: Vec<PathBuf>,
    pub bytes: u64,
    pub left: u32,
}

pub(crate) fn plan_mods(profile: &ImportProfile) -> ModPlan {
    let dir = profile.game_dir.join("mods");
    let Some(minecraft) = profile.minecraft.as_deref() else {
        return ModPlan::default();
    };
    if profile.loader == LoaderKind::Vanilla {
        // Vanilla loads no mods: whatever sits in a shared folder belongs to other versions.
        let left = if profile.own_folder {
            crate::detect::count_mods(&dir)
        } else {
            0
        };
        return ModPlan {
            left,
            ..ModPlan::default()
        };
    }
    if !dir.is_dir() {
        return ModPlan::default();
    }
    let env = CheckEnvironment {
        minecraft: minecraft.to_owned(),
        loader: profile.loader,
        loader_version: profile.loader_version.clone(),
        java_major: None,
    };
    let (snapshot, report) = check_mods_dir(ModScanner::shared(), &dir, &env, &FileOrigins::new());
    let unfit: HashSet<String> = report
        .issues
        .iter()
        .filter(|issue| {
            matches!(
                issue.kind,
                IssueKind::WrongLoader | IssueKind::IncompatibleMinecraft | IssueKind::Corrupt
            )
        })
        .flat_map(|issue| issue.mods.iter().map(|m| m.file.to_lowercase()))
        .collect();
    let mut plan = ModPlan::default();
    for jar in &snapshot.jars {
        if unfit.contains(&jar.file_name.to_lowercase()) {
            plan.left += 1;
        } else {
            plan.bytes += jar.size;
            plan.take.push(jar.path.clone());
        }
    }
    plan
}

/// Entries of an instance's folder that are not the player's: logs, crash reports, caches,
/// launcher files and game files the launcher installs itself. The rest is copied as it is,
/// apart from what the import treats on its own (`mods`, `servers.dat`) and what it copies one
/// by one (`saves`).
pub(crate) fn instance_skip(rel: &Path) -> bool {
    let mut parts = rel.components();
    let Some(first) = parts.next() else {
        return false;
    };
    let top = first.as_os_str().to_string_lossy().to_lowercase();
    if parts.next().is_some() {
        // Deeper entries: only the world lock (a running game's).
        return rel.file_name().is_some_and(|n| n == "session.lock");
    }
    const NOT_THE_PLAYERS: [&str; 18] = [
        "logs",
        "crash-reports",
        "debug",
        "natives",
        "libraries",
        "versions",
        "assets",
        "bin",
        "downloads",
        "minecraftinstance.json",
        "profile.json",
        "instance.cfg",
        "mmc-pack.json",
        "launcher_profiles.json",
        "launcher_accounts.json",
        "tlauncherprofiles.json",
        "usercache.json",
        "realms_persistence.json",
    ];
    top.starts_with('.')
        || top.ends_with(".log")
        || top.starts_with("hs_err_pid")
        || NOT_THE_PLAYERS.contains(&top.as_str())
        || top == "mods"
        || top == "servers.dat"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_copies_leave_logs_caches_and_launcher_files() {
        for skipped in [
            "logs",
            ".fabric",
            "crash-reports",
            "minecraftinstance.json",
            "latest.log",
            "mods",
            "servers.dat",
            "saves/World/session.lock",
        ] {
            assert!(instance_skip(Path::new(skipped)), "{skipped}");
        }
        for kept in [
            "config",
            "options.txt",
            "saves",
            "saves/World/level.dat",
            "journeymap",
            "kubejs",
        ] {
            assert!(!instance_skip(Path::new(kept)), "{kept}");
        }
    }
}
