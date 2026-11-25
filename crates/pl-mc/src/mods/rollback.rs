//! "Önceki Sürüme Dön" (REPAIR-RB): going back to the mod version that worked.
//!
//! - [`ModHistory`] (`<instance>/.launcher/mod-history.json`, beside the add-on index, so a version
//!   switch leaves it where it is): for each Minecraft version, the enabled jars of the last game
//!   that ended without a crash ([`CleanGame`], made by [`clean_game`]), and the mods held at an
//!   older version after a crash ([`ModHold`]).
//! - [`rollback_candidate`]: a crash's suspect whose jar differs from the last clean game's (updated
//!   or added by a version switch, a pack sync or the player since). The same Minecraft version's
//!   record is the baseline; on a version's first visit, the newest record of another version is.
//!   A mod in the crash's stack trace counts only when nothing stronger is named: its change since
//!   the last clean game is the second clue.
//! - [`pick_rollback`] / [`resolve_rollback`]: the version to go back to: the last clean game's when
//!   Modrinth says it runs on the instance's Minecraft version and loader, else the newest version
//!   published before the crashing one that does (releases before betas and alphas), else none.
//! - [`HeldFiles`]: the jars a repair must not move to another version
//!   ([`super::repair::RepairContext::held`]); the launcher also keeps them out of update prompts
//!   and of the pack upgrades it makes by itself, until the player updates the mod (the way out).
//! - [`repeated_hold`]: a held mod the game crashed with again before any clean game: going back
//!   did not help, so it is no longer held and is not offered again.
//! - [`roll_back_own`] / [`undo_own`]: a mod of the player's goes back by swapping its jar (the
//!   crashing one to the launcher trash; "Geri Al" puts it back). Official pack mods go back as the
//!   player's version of the pack item (`pl_content::OverrideReason::Rollback`, done by the
//!   launcher).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use pl_net::{CancellationToken, Downloader};
use serde::{Deserialize, Serialize};

use super::scan::{ModSnapshot, ScannedJar, strip_disabled};
use crate::addons::{AddonInstallOptions, Addons};
use crate::error::{Error, Result};
use crate::gamefiles::{ImportKind, LauncherTrash, TrashKind};
use crate::launch::crash::{CrashDiagnosis, CrashKind, SuspectReason};
use crate::modrinth::{
    ModrinthClient, ModrinthHashAlgorithm, ModrinthResolveTarget, ModrinthVersion,
    ModrinthVersionType,
};

/// File name of the history, kept in `<instance>/.launcher/`.
pub const MOD_HISTORY_FILE: &str = "mod-history.json";
const SCHEMA: u32 = 1;
/// Minecraft versions whose last clean game is kept (the oldest goes first).
pub const MAX_CLEAN_SETS: usize = 4;

/// What the launcher remembers about the mods of one instance (see the module).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModHistory {
    pub schema: u32,
    /// The last game that ended without a crash, per Minecraft version.
    pub clean: BTreeMap<String, CleanGame>,
    /// Mods held at an older version after a crash, per Minecraft version.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub holds: BTreeMap<String, Vec<ModHold>>,
}

impl Default for ModHistory {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            clean: BTreeMap::new(),
            holds: BTreeMap::new(),
        }
    }
}

/// The enabled jars of a game that ended without a crash.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CleanGame {
    /// When the game ended (ms since the Unix epoch).
    pub at_ms: i64,
    /// Modrinth's name of the loader (`fabric`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub loader: String,
    pub mods: Vec<CleanMod>,
}

/// One jar of a clean game.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CleanMod {
    /// File name in `mods/`.
    pub file: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_ms: Option<i64>,
    /// Lowercase hex SHA-1; empty when the file could not be read.
    pub sha1: String,
    /// The mod ids its jar declares at the top level, and the ids they provide.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    /// The first declared mod's version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The Modrinth project and version, when the launcher knew them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    /// The official pack item that installed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack_item: Option<String>,
}

/// A mod held at an older version after a crash.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModHold {
    /// The jar's first declared mod id.
    pub mod_id: String,
    /// What players call it.
    pub name: String,
    /// The held jar in `mods/` (a pack mod's: its override's file).
    pub file: String,
    /// The official pack item, for a pack mod (its override holds the version).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack_item: Option<String>,
    /// The held version and the one that crashed, as players read them.
    pub version: String,
    pub from_version: String,
    pub at_ms: i64,
    /// The crashing jar in the launcher trash (a mod of the player's): "Geri Al" puts it back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trash_id: Option<String>,
    /// A game ended without a crash since: the held version works.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub proven: bool,
    /// The game crashed again with this mod before a clean game: going back did not help. No
    /// longer held; kept to say so, and so that going back is not offered again.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub failed: bool,
}

impl ModHold {
    /// Updates, repairs and pack upgrades leave the jar alone.
    pub fn holds(&self) -> bool {
        !self.failed
    }
}

/// The mod history's path for an instance folder.
pub fn history_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join(".launcher").join(MOD_HISTORY_FILE)
}

impl ModHistory {
    /// The history at `path`; a missing or unreadable file gives an empty one.
    pub async fn load(path: &Path) -> Self {
        match tokio::fs::read(path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                tracing::warn!(path = %path.display(), %error, "ignoring an unreadable mod history");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Writes it (compact) atomically.
    pub async fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec(self).map_err(|e| Error::json(MOD_HISTORY_FILE, e))?;
        crate::paths::write_atomic(path, &bytes).await
    }

    /// The last clean game to compare a crash of `minecraft` with: that version's own, else (a
    /// version's first visit) the newest of another version.
    pub fn baseline(&self, minecraft: &str) -> Option<&CleanGame> {
        self.clean
            .get(minecraft)
            .or_else(|| self.clean.values().max_by_key(|game| game.at_ms))
    }

    /// Records a clean game of `minecraft`: it proves the versions held in it, and ends the marks
    /// of holds that did not help. `false` when nothing changes on disk: the same jars as the
    /// version's record, which is already the newest one.
    pub fn record_clean(&mut self, minecraft: &str, game: CleanGame) -> bool {
        let newest = self.clean.values().map(|g| g.at_ms).max();
        let changed = match self.clean.get(minecraft) {
            Some(old) => old.mods != game.mods || newest != Some(old.at_ms),
            None => true,
        };
        let mut holds_changed = false;
        if let Some(holds) = self.holds.get_mut(minecraft) {
            let before = holds.len();
            holds.retain(|hold| !hold.failed);
            holds_changed = holds.len() != before;
            for hold in holds.iter_mut().filter(|hold| !hold.proven) {
                if game
                    .mods
                    .iter()
                    .any(|m| m.file.eq_ignore_ascii_case(&hold.file))
                {
                    hold.proven = true;
                    holds_changed = true;
                }
            }
            if holds.is_empty() {
                self.holds.remove(minecraft);
            }
        }
        if changed {
            self.clean.insert(minecraft.to_owned(), game);
            while self.clean.len() > MAX_CLEAN_SETS {
                let Some(oldest) = self
                    .clean
                    .iter()
                    .min_by_key(|(_, g)| g.at_ms)
                    .map(|(k, _)| k.clone())
                else {
                    break;
                };
                self.clean.remove(&oldest);
            }
        }
        changed || holds_changed
    }

    /// The holds of `minecraft`.
    pub fn holds_of(&self, minecraft: &str) -> &[ModHold] {
        self.holds
            .get(minecraft)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// Adds a hold, in place of one for the same jar, mod or pack item.
    pub fn hold(&mut self, minecraft: &str, hold: ModHold) {
        let list = self.holds.entry(minecraft.to_owned()).or_default();
        list.retain(|h| {
            !(h.file.eq_ignore_ascii_case(&hold.file)
                || h.mod_id == hold.mod_id
                || (h.pack_item.is_some() && h.pack_item == hold.pack_item))
        });
        list.push(hold);
    }

    /// Removes the hold of `mod_id` (the way out, or "Geri Al").
    pub fn release(&mut self, minecraft: &str, mod_id: &str) -> Option<ModHold> {
        let list = self.holds.get_mut(minecraft)?;
        let at = list.iter().position(|h| h.mod_id == mod_id)?;
        let hold = list.remove(at);
        if list.is_empty() {
            self.holds.remove(minecraft);
        }
        Some(hold)
    }

    /// Marks the hold of `mod_id` as one that did not help (the game crashed with it again).
    pub fn mark_failed(&mut self, minecraft: &str, mod_id: &str) -> bool {
        match self
            .holds
            .get_mut(minecraft)
            .and_then(|list| list.iter_mut().find(|h| h.mod_id == mod_id))
        {
            Some(hold) if !hold.failed => {
                hold.failed = true;
                true
            }
            _ => false,
        }
    }

    /// Drops the holds of `minecraft` that no longer describe the mods folder: a jar of the
    /// player's that is gone (updated or removed: their choice), a pack mod whose rollback override
    /// is gone ("Paketin Sürümüne Dön"). `rollback_items` are the pack items with such an override.
    /// Returns whether anything went.
    pub fn reconcile(
        &mut self,
        minecraft: &str,
        snapshot: &ModSnapshot,
        rollback_items: &HashSet<String>,
    ) -> bool {
        let Some(list) = self.holds.get_mut(minecraft) else {
            return false;
        };
        let before = list.len();
        list.retain(|hold| match &hold.pack_item {
            Some(item) => rollback_items.contains(item),
            None => snapshot.by_file(&hold.file).is_some(),
        });
        let changed = list.len() != before;
        if list.is_empty() {
            self.holds.remove(minecraft);
        }
        changed
    }
}

// --- recording a clean game --------------------------------------------------------------------

/// What the launcher knows about a file of the mods folder without reading it: from the add-on
/// index (only while the file has the recorded size and time) and the official pack's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownFile {
    pub sha1: Option<String>,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub pack_item: Option<String>,
}

/// Lowercase file name without `.disabled` → what is known about it.
pub type KnownFiles = HashMap<String, KnownFile>;

/// The key of a file in [`KnownFiles`] and [`HeldFiles`].
pub fn file_key(file: &str) -> String {
    strip_disabled(file).to_ascii_lowercase()
}

fn jar_ids(jar: &ScannedJar) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for m in &jar.contents.mods {
        for id in std::iter::once(&m.id).chain(&m.provides) {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    ids
}

/// The clean game of `snapshot`'s enabled jars. A jar's SHA-1 comes from `known`, else from the
/// `previous` record of the same file with the same size and time, else from `hash` (which reads
/// the file; only new or changed jars get there).
pub fn clean_game(
    snapshot: &ModSnapshot,
    loader: &str,
    at_ms: i64,
    known: &KnownFiles,
    previous: Option<&CleanGame>,
    mut hash: impl FnMut(&Path) -> Option<String>,
) -> CleanGame {
    let mods = snapshot
        .enabled()
        .map(|jar| {
            let know = known.get(&file_key(&jar.file_name));
            let carried = previous.and_then(|game| {
                game.mods.iter().find(|m| {
                    m.file == jar.file_name
                        && m.size == jar.size
                        && m.modified_ms == jar.modified_ms
                        && !m.sha1.is_empty()
                })
            });
            let sha1 = know
                .and_then(|k| k.sha1.clone())
                .or_else(|| carried.map(|m| m.sha1.clone()))
                .or_else(|| hash(&jar.path))
                .unwrap_or_default()
                .to_ascii_lowercase();
            CleanMod {
                file: jar.file_name.clone(),
                size: jar.size,
                modified_ms: jar.modified_ms,
                sha1,
                ids: jar_ids(jar),
                version: jar.contents.mods.first().and_then(|m| m.version.clone()),
                project_id: know
                    .and_then(|k| k.project_id.clone())
                    .or_else(|| carried.and_then(|m| m.project_id.clone())),
                version_id: know
                    .and_then(|k| k.version_id.clone())
                    .or_else(|| carried.and_then(|m| m.version_id.clone())),
                pack_item: know.and_then(|k| k.pack_item.clone()),
            }
        })
        .collect();
    CleanGame {
        at_ms,
        loader: loader.to_owned(),
        mods,
    }
}

/// SHA-1 of a file (lowercase hex), for [`clean_game`].
pub fn sha1_of(path: &Path) -> Option<String> {
    super::sha1_file(path)
        .inspect_err(|error| tracing::debug!(%error, "a mod was not hashed"))
        .ok()
}

// --- held jars ---------------------------------------------------------------------------------

/// Jars a repair must not move to another version (lowercase file names without `.disabled`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeldFiles(BTreeSet<String>);

static NOTHING_HELD: HeldFiles = HeldFiles(BTreeSet::new());

impl HeldFiles {
    /// No jar is held.
    pub fn none() -> &'static HeldFiles {
        &NOTHING_HELD
    }

    /// The jars `holds` keep (a failed hold keeps nothing).
    pub fn of(holds: &[ModHold]) -> Self {
        Self(
            holds
                .iter()
                .filter(|h| h.holds())
                .map(|h| file_key(&h.file))
                .collect(),
        )
    }

    /// `file` (with or without `.disabled`) is held.
    pub fn contains(&self, file: &str) -> bool {
        !self.0.is_empty() && self.0.contains(&file_key(file))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The hold that keeps `file` (a jar of `mods/`) at its version, if any.
pub fn hold_of<'a>(holds: &'a [ModHold], file: &str) -> Option<&'a ModHold> {
    let key = file_key(file);
    holds.iter().find(|h| h.holds() && file_key(&h.file) == key)
}

// --- a crash's candidate -----------------------------------------------------------------------

/// Suspect reasons that say the mod's own code failed.
fn strong(reason: SuspectReason) -> bool {
    matches!(
        reason,
        SuspectReason::MixinFailed | SuspectReason::LoadingFailed | SuspectReason::NamedInReport
    )
}

/// Crashes whose suspects can go back: a mod failed. Dependency, version and duplicate errors
/// have exact fixes of their own.
fn mod_failure(kind: CrashKind) -> bool {
    matches!(
        kind,
        CrashKind::ModError | CrashKind::ModCrash | CrashKind::Unknown
    )
}

/// A crash's suspect mod that changed since the last clean game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackCandidate {
    /// The jar's first declared mod id.
    pub mod_id: String,
    /// The crashing jar's file name in `mods/`.
    pub file: String,
    /// The jar's own name for the mod.
    pub name: String,
    /// The crashing jar's declared version.
    pub version: Option<String>,
    pub reason: SuspectReason,
    /// The same mod in the last clean game; `None` when it was added since.
    pub before: Option<CleanMod>,
    /// The crashing jar's SHA-1, when known.
    pub sha1: Option<String>,
}

/// The suspect of an attributed crash (`crate::launch::crash::attribute_crash`) that changed
/// since `baseline`: strong suspects only (a mod in the stack trace only when the crash names
/// nothing stronger), enabled jars only, never one in `skip` (lowercase file names: jars another
/// owner updates, such as a ready-made set's). `known` gives the jars' SHA-1s (else the jar is
/// compared by file name, size and time). `None` when no suspect changed.
pub fn rollback_candidate(
    diagnosis: &CrashDiagnosis,
    snapshot: &ModSnapshot,
    baseline: &CleanGame,
    known: &KnownFiles,
    skip: &HashSet<String>,
) -> Option<RollbackCandidate> {
    if !mod_failure(diagnosis.kind) {
        return None;
    }
    let any_strong = diagnosis.suspects.iter().any(|s| strong(s.reason));
    diagnosis
        .suspects
        .iter()
        .filter(|s| {
            if any_strong {
                strong(s.reason)
            } else {
                s.reason == SuspectReason::InStackTrace
            }
        })
        .find_map(|s| {
            let jar = snapshot
                .by_file(s.file.as_deref()?)
                .filter(|jar| jar.enabled)?;
            if skip.contains(&file_key(&jar.file_name)) {
                return None;
            }
            let first = jar.contents.mods.first()?;
            let ids = jar_ids(jar);
            let sha1 = known
                .get(&file_key(&jar.file_name))
                .and_then(|k| k.sha1.clone())
                .map(|h| h.to_ascii_lowercase());
            let before = baseline
                .mods
                .iter()
                .find(|m| m.ids.iter().any(|id| ids.contains(id)))
                .or_else(|| {
                    baseline
                        .mods
                        .iter()
                        .find(|m| m.file.eq_ignore_ascii_case(&jar.file_name))
                });
            let unchanged = before.is_some_and(|m| match &sha1 {
                Some(sha1) if !m.sha1.is_empty() => m.sha1.eq_ignore_ascii_case(sha1),
                // Without a hash: the same file, size and time.
                _ => {
                    m.file == jar.file_name
                        && m.size == jar.size
                        && m.modified_ms == jar.modified_ms
                }
            });
            (!unchanged).then(|| RollbackCandidate {
                mod_id: first.id.clone(),
                file: jar.file_name.clone(),
                name: first.display_name().to_owned(),
                version: first.version.clone(),
                reason: s.reason,
                before: before.cloned(),
                sha1,
            })
        })
}

/// The hold the game crashed with again before a clean game proved it: going back did not help.
pub fn repeated_hold<'a>(holds: &'a [ModHold], diagnosis: &CrashDiagnosis) -> Option<&'a ModHold> {
    if !mod_failure(diagnosis.kind) {
        return None;
    }
    holds
        .iter()
        .filter(|h| !h.proven && !h.failed)
        .find(|hold| {
            diagnosis.suspects.iter().any(|s| {
                (strong(s.reason) || s.reason == SuspectReason::InStackTrace)
                    && (s
                        .file
                        .as_deref()
                        .is_some_and(|f| file_key(f) == file_key(&hold.file))
                        || s.mod_id.as_deref() == Some(hold.mod_id.as_str()))
            })
        })
}

/// A failed hold of a mod the crash names: going back was tried and did not help.
pub fn failed_hold<'a>(holds: &'a [ModHold], diagnosis: &CrashDiagnosis) -> Option<&'a ModHold> {
    holds.iter().filter(|h| h.failed).find(|hold| {
        diagnosis.suspects.iter().any(|s| {
            s.file
                .as_deref()
                .is_some_and(|f| file_key(f) == file_key(&hold.file))
                || s.mod_id.as_deref() == Some(hold.mod_id.as_str())
        })
    })
}

// --- the version to go back to -----------------------------------------------------------------

fn published(version: &ModrinthVersion) -> Option<jiff::Timestamp> {
    version.date_published.parse().ok()
}

fn has_sha1(version: &ModrinthVersion, sha1: &str) -> bool {
    version.files.iter().any(|f| {
        f.hashes
            .sha1
            .as_deref()
            .is_some_and(|h| h.eq_ignore_ascii_case(sha1))
    })
}

/// The version to go back to, from a project's versions: the last clean game's (`before`, by
/// Modrinth version id, else by SHA-1) when `target` accepts it; else the newest version `target`
/// accepts that was published before the crashing one (`current`), releases before betas and
/// alphas. Never the crashing version itself (`current`, `current_sha1`).
pub fn pick_rollback<'a>(
    versions: &'a [ModrinthVersion],
    target: &ModrinthResolveTarget,
    before: Option<&CleanMod>,
    current: Option<&ModrinthVersion>,
    current_sha1: Option<&str>,
) -> Option<&'a ModrinthVersion> {
    let not_current = |v: &&ModrinthVersion| {
        current.is_none_or(|c| c.id != v.id) && current_sha1.is_none_or(|sha1| !has_sha1(v, sha1))
    };
    let usable = || {
        versions
            .iter()
            .filter(|v| target.accepts(v) && v.primary_file().is_some())
            .filter(not_current)
    };
    if let Some(before) = before {
        let same = |v: &&ModrinthVersion| {
            before.version_id.as_deref() == Some(v.id.as_str())
                || (!before.sha1.is_empty() && has_sha1(v, &before.sha1))
        };
        if let Some(found) = usable().find(same) {
            return Some(found);
        }
    }
    let crashed_at = published(current?)?;
    let older: Vec<&ModrinthVersion> = usable()
        .filter(|v| published(v).is_some_and(|at| at < crashed_at))
        .collect();
    [
        ModrinthVersionType::Release,
        ModrinthVersionType::Beta,
        ModrinthVersionType::Alpha,
    ]
    .iter()
    .find_map(|kind| {
        older
            .iter()
            .filter(|v| v.version_type == *kind)
            .max_by_key(|v| published(v))
            .copied()
    })
}

/// A resolved rollback: what to install instead of the crashing jar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackPlan {
    pub candidate: RollbackCandidate,
    pub project_id: String,
    /// The crashing version as Modrinth knows it, when it does.
    pub current: Option<ModrinthVersion>,
    pub target: ModrinthVersion,
}

impl RollbackPlan {
    /// The crashing version as players read it.
    pub fn from_version(&self) -> String {
        self.current
            .as_ref()
            .map(|v| v.version_number.clone())
            .or_else(|| self.candidate.version.clone())
            .unwrap_or_default()
    }
}

/// What [`resolve_rollback`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackLookup {
    Found(Box<RollbackPlan>),
    /// The mod is on Modrinth, but no older version runs on this Minecraft version and loader.
    NoOlder,
    /// Modrinth does not know the mod.
    Unknown,
}

/// Finds the version to go back to on Modrinth: the project from `project_hint` (the file's
/// origin, a pack item's project) or the last clean game's record, else by the files' hashes
/// (one request, which also tells the crashing version); then the project's versions for
/// `target` (one request) and [`pick_rollback`].
pub async fn resolve_rollback(
    client: &ModrinthClient,
    candidate: &RollbackCandidate,
    project_hint: Option<&str>,
    target: &ModrinthResolveTarget,
) -> Result<RollbackLookup> {
    let before = candidate.before.as_ref();
    let mut current: Option<ModrinthVersion> = None;
    let mut project = project_hint
        .map(str::to_owned)
        .or_else(|| before.and_then(|b| b.project_id.clone()));
    let hashes: Vec<String> = candidate
        .sha1
        .iter()
        .chain(before.map(|b| &b.sha1).filter(|h| !h.is_empty()))
        .cloned()
        .collect();
    if project.is_none() && !hashes.is_empty() {
        let found = client
            .version_files(&hashes, ModrinthHashAlgorithm::Sha1)
            .await?;
        if let Some(sha1) = &candidate.sha1 {
            current = found.get(sha1).cloned();
        }
        project = hashes
            .iter()
            .find_map(|h| found.get(h).map(|v| v.project_id.clone()));
    }
    let Some(project) = project else {
        return Ok(RollbackLookup::Unknown);
    };
    let versions = client
        .project_versions(&project, &target.loaders, &target.game_versions)
        .await?;
    if current.is_none()
        && let Some(sha1) = &candidate.sha1
    {
        current = versions.iter().find(|v| has_sha1(v, sha1)).cloned();
    }
    let picked = pick_rollback(
        &versions,
        target,
        before,
        current.as_ref(),
        candidate.sha1.as_deref(),
    )
    .cloned();
    Ok(match picked {
        Some(version) => RollbackLookup::Found(Box::new(RollbackPlan {
            candidate: candidate.clone(),
            project_id: project,
            current,
            target: version,
        })),
        None => RollbackLookup::NoOlder,
    })
}

// --- a mod of the player's ---------------------------------------------------------------------

/// Everything a rollback of a player's mod runs with.
pub struct OwnRollbackJob<'a> {
    /// Installs into the set's `mods/` and keeps its add-on index.
    pub addons: &'a Addons,
    pub client: &'a ModrinthClient,
    pub downloader: &'a Downloader,
    pub trash: &'a LauncherTrash,
    pub instance_id: &'a str,
    pub target: ModrinthResolveTarget,
    pub cancel: CancellationToken,
}

/// Swaps the crashing jar of `plan` for its target version: the jar goes to the launcher trash
/// and the target is installed alone (no dependencies: only this jar changes). When the install
/// fails the jar comes back. Returns the hold to record.
pub async fn roll_back_own(
    job: &OwnRollbackJob<'_>,
    plan: &RollbackPlan,
    name: &str,
    at_ms: i64,
) -> Result<ModHold> {
    let file = plan
        .target
        .primary_file()
        .map(|f| f.filename.clone())
        .ok_or_else(|| Error::NotFound(format!("files of Modrinth version {}", plan.target.id)))?;
    let crashing = job
        .addons
        .game_dir()
        .join("mods")
        .join(&plan.candidate.file);
    let from_version = plan.from_version();
    let label = if from_version.is_empty() {
        name.to_owned()
    } else {
        format!("{name} {from_version}")
    };
    let trashed = job
        .trash
        .put(&crashing, TrashKind::Mod, job.instance_id, &label)
        .await?;
    let installed = job
        .addons
        .install(
            job.client,
            job.downloader,
            ImportKind::Mod,
            vec![plan.target.clone()],
            &job.target,
            AddonInstallOptions {
                with_dependencies: false,
                concurrency: 1,
                cancel: job.cancel.clone(),
                progress: None,
            },
        )
        .await;
    if let Err(error) = installed {
        if let Err(back) = job.trash.restore(&trashed.id).await {
            tracing::warn!(%back, "the crashing jar could not be put back after a failed rollback");
        }
        return Err(error);
    }
    Ok(ModHold {
        mod_id: plan.candidate.mod_id.clone(),
        name: name.to_owned(),
        file,
        pack_item: None,
        version: plan.target.version_number.clone(),
        from_version,
        at_ms,
        trash_id: Some(trashed.id),
        proven: false,
        failed: false,
    })
}

/// "Geri Al" after a player's mod went back: the held jar goes to the launcher trash and the jar
/// that crashed comes back from it (checked first: nothing moves when it is gone).
pub async fn undo_own(
    mods_dir: &Path,
    trash: &LauncherTrash,
    instance_id: &str,
    hold: &ModHold,
) -> Result<()> {
    let trash_id = hold
        .trash_id
        .as_deref()
        .ok_or_else(|| Error::NotFound("the jar that crashed".into()))?;
    trash.get(trash_id).await?;
    for name in [hold.file.clone(), format!("{}.disabled", hold.file)] {
        let path = mods_dir.join(&name);
        if tokio::fs::symlink_metadata(&path).await.is_ok() {
            let label = format!("{} {}", hold.name, hold.version);
            trash
                .put(&path, TrashKind::Mod, instance_id, &label)
                .await?;
        }
    }
    trash.restore(trash_id).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
