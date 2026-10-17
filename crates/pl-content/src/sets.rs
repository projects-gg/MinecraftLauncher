//! Per-version sets of a game directory.
//!
//! Every Minecraft version an instance has been on keeps its own [`PARKED_DIRS`] (`mods/` and
//! `config/`) and, for pack instances, its own pack state. The active set lives in place; the
//! other sets are parked inside the game directory:
//!
//! ```text
//! .projects/set.json                               the active set's metadata
//! .projects/pack-state.json                        the active set's pack state
//! .projects/versions/<minecraft>/set.json          a parked set's metadata
//! .projects/versions/<minecraft>/pack-state.json   its pack state (pack instances)
//! .projects/versions/<minecraft>/mods/             its mods folder
//! .projects/versions/<minecraft>/config/           its config folder
//! .projects/version-switch.json                    the journal of a switch in progress
//! ```
//!
//! [`VersionSets::switch`] parks the active set and brings the target's set back exactly, or
//! on the first visit starts it: an empty `mods/` (or the mods prepared for it with
//! [`VersionSets::prepare`]), a copy of `config/`, and the item choices carried over
//! ([`PackState::switch_version`]). Every step is recorded in the journal before it runs and is
//! safe to repeat, so a switch interrupted by a crash or a power cut is finished by
//! [`VersionSets::resume`] on the next start. Nothing is deleted on the way: a folder that is in
//! the way is moved aside into the parked set.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil::{read_optional, write_durable};
use crate::model::compare_minecraft_versions;
use crate::path::{LAUNCHER_DIR, check_relative_path};
use crate::sync::{COPIED_DIRS, PACK_STATE_FILE, PARKED_DIRS, PackState};

/// Folder inside `<game_dir>/.projects/` that holds the parked sets.
pub const SETS_DIR: &str = "versions";
/// Metadata file of a set (active: `.projects/set.json`; parked: inside its folder).
pub const SET_FILE: &str = "set.json";
/// The journal of a switch in progress, inside `<game_dir>/.projects/`.
pub const SWITCH_JOURNAL: &str = "version-switch.json";
/// Temporary name of a folder being copied in by a first visit.
const COPY_SUFFIX: &str = ".switch-copy";

/// Metadata of one set (`set.json`). The loader and pack fields are informational: they say
/// what the set was made for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SetInfo {
    pub minecraft: String,
    /// Mod loader (`fabric`, `neoForge`, …; empty for vanilla).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub loader: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub loader_version: String,
    /// The pack the set followed (pack instances).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pack_id: String,
    /// When the game last started with this set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played_at: Option<Timestamp>,
    /// When the set was parked (parked sets only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parked_at: Option<Timestamp>,
    /// Size of the parked folders in bytes, measured when the set was parked.
    pub size_bytes: u64,
    /// Prepared ahead of the first visit (mods moved into it from another version): the first
    /// switch still carries the choices over and copies `config/`.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub seeded: bool,
}

/// One step of a switch. Each runs completely before the journal moves on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SwitchStep {
    /// Moving the active set's folders into its parked folder.
    Park,
    /// Bringing the target's folders back, or starting them.
    Restore,
    /// Writing the pack states and the set metadata.
    States,
    /// The files are in place: the caller updates its own records (the instance's version)
    /// and calls [`VersionSets::finish`].
    Finalize,
}

/// `.projects/version-switch.json`: everything needed to finish a switch after a crash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchJournal {
    pub from: String,
    pub to: String,
    pub step: SwitchStep,
    /// The target had no parked set (or only a prepared one): `config/` is copied and the
    /// choices carried over.
    pub first_visit: bool,
    /// The target was prepared ([`VersionSets::prepare`]): its `mods/` comes in.
    #[serde(default)]
    pub prepared: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parked_state: Option<PackState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_state: Option<PackState>,
    pub parked_info: SetInfo,
    pub active_info: SetInfo,
    /// Folders that were in the way and were moved aside (relative to the game directory).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moved_aside: Vec<String>,
    /// Data the caller stored to finish the switch (e.g. the instance's new loader).
    #[serde(default)]
    pub context: serde_json::Value,
    pub started_at: Timestamp,
}

/// A parked set as the cleanup sees it ([`VersionSets::parked_sets`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ParkedSet {
    pub info: SetInfo,
    /// `.projects/versions/<minecraft>/`.
    pub dir: PathBuf,
    /// When the set was last used: last played, else parked, else when its metadata (or folder)
    /// last changed.
    pub last_used: Option<Timestamp>,
    /// Bytes on disk now.
    pub size_bytes: u64,
}

/// Input of [`VersionSets::switch`].
#[derive(Debug, Clone, Default)]
pub struct SwitchRequest {
    /// The set being left; `minecraft` is the active version. Without `last_played_at` the
    /// active `set.json` says when it was last played.
    pub from: SetInfo,
    /// The version the instance moves to, with its loader and pack.
    pub to: SetInfo,
    /// Stored in the journal for [`VersionSets::resume`].
    pub context: serde_json::Value,
}

/// The per-version sets of one game directory.
#[derive(Debug, Clone)]
pub struct VersionSets {
    game_dir: PathBuf,
}

impl VersionSets {
    pub fn new(game_dir: impl Into<PathBuf>) -> Self {
        Self {
            game_dir: game_dir.into(),
        }
    }

    pub fn game_dir(&self) -> &Path {
        &self.game_dir
    }

    fn launcher_dir(&self) -> PathBuf {
        self.game_dir.join(LAUNCHER_DIR)
    }

    /// `.projects/versions/`.
    pub fn sets_dir(&self) -> PathBuf {
        self.launcher_dir().join(SETS_DIR)
    }

    /// The folder of a parked set: `.projects/versions/<minecraft>/`. Characters outside
    /// `[A-Za-z0-9._-]` become `_`; names the path rules refuse are hex-encoded. The folder
    /// mirrors a game directory (`<set>/mods/`, `<set>/config/`).
    pub fn set_dir(&self, minecraft: &str) -> PathBuf {
        self.sets_dir().join(folder_name(minecraft))
    }

    fn active_info_path(&self) -> PathBuf {
        self.launcher_dir().join(SET_FILE)
    }

    fn journal_path(&self) -> PathBuf {
        self.launcher_dir().join(SWITCH_JOURNAL)
    }

    /// Metadata of the active set; `None` before the first launch or switch recorded it.
    pub async fn active_info(&self) -> Result<Option<SetInfo>> {
        read_json(&self.active_info_path()).await
    }

    /// Records that the game started with the active set (`current` describes it).
    pub async fn record_played(&self, current: SetInfo, at: Timestamp) -> Result<()> {
        let info = SetInfo {
            last_played_at: Some(at),
            parked_at: None,
            size_bytes: 0,
            seeded: false,
            ..current
        };
        write_json(&self.active_info_path(), &info).await
    }

    /// Metadata of `minecraft`'s parked set; `None` when the instance has none. A parked
    /// folder without readable metadata still counts (with what its name says).
    pub async fn parked_info(&self, minecraft: &str) -> Result<Option<SetInfo>> {
        let dir = self.set_dir(minecraft);
        if let Some(info) = read_json::<SetInfo>(&dir.join(SET_FILE)).await? {
            return Ok(Some(info));
        }
        for parked in PARKED_DIRS {
            if exists(&dir.join(parked)).await {
                return Ok(Some(SetInfo {
                    minecraft: minecraft.to_owned(),
                    ..SetInfo::default()
                }));
            }
        }
        Ok(None)
    }

    /// The pack state of `minecraft`'s parked set, if it has one.
    pub async fn parked_state(&self, minecraft: &str) -> Result<Option<PackState>> {
        PackState::load_from(&self.set_dir(minecraft).join(PACK_STATE_FILE)).await
    }

    /// Every parked set, newest Minecraft version first.
    pub async fn parked(&self) -> Result<Vec<SetInfo>> {
        let root = self.sets_dir();
        let mut sets = Vec::new();
        let mut entries = match tokio::fs::read_dir(&root).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(sets),
            Err(e) => return Err(Error::io(&root, e)),
        };
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| Error::io(&root, e))?
        {
            if !entry.file_type().await.is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let dir = entry.path();
            let info = read_json::<SetInfo>(&dir.join(SET_FILE))
                .await
                .ok()
                .flatten();
            let has_folders = {
                let mut any = false;
                for parked in PARKED_DIRS {
                    any |= exists(&dir.join(parked)).await;
                }
                any
            };
            match info {
                Some(info) if !info.minecraft.is_empty() => sets.push(info),
                _ if has_folders => sets.push(SetInfo {
                    minecraft: entry.file_name().to_string_lossy().into_owned(),
                    ..SetInfo::default()
                }),
                _ => {}
            }
        }
        sets.sort_by(|a, b| compare_minecraft_versions(&b.minecraft, &a.minecraft));
        Ok(sets)
    }

    /// Size of the active set's folders in bytes.
    pub async fn active_size(&self) -> u64 {
        let dirs: Vec<PathBuf> = PARKED_DIRS.iter().map(|d| self.game_dir.join(d)).collect();
        tokio::task::spawn_blocking(move || dirs.iter().map(|d| dir_size(d)).sum::<u64>())
            .await
            .unwrap_or(0)
    }

    /// The journal of an unfinished switch. A journal that cannot be read is set aside (and
    /// `None` returned): the folders stay where they are, nothing is lost.
    pub async fn pending(&self) -> Result<Option<SwitchJournal>> {
        let path = self.journal_path();
        let Some(bytes) = read_optional(&path).await? else {
            return Ok(None);
        };
        match serde_json::from_slice(&bytes) {
            Ok(journal) => Ok(Some(journal)),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "setting aside an unreadable version switch journal");
                let aside =
                    path.with_extension(format!("corrupt-{}", Timestamp::now().as_second()));
                tokio::fs::rename(&path, &aside)
                    .await
                    .map_err(|e| Error::io(&path, e))?;
                Ok(None)
            }
        }
    }

    /// Moves the instance from `request.from` to `request.to`: parks the active set and
    /// brings back the target's (or starts it on the first visit). Returns the journal at
    /// [`SwitchStep::Finalize`]: update the instance's records, then call [`Self::finish`].
    /// Refused while another switch is unfinished ([`Self::resume`] it first). Do not run it
    /// while the game uses the folders.
    pub async fn switch(&self, request: SwitchRequest) -> Result<SwitchJournal> {
        let journal = self.begin(request).await?;
        self.run(journal).await
    }

    /// Finishes an interrupted switch up to [`SwitchStep::Finalize`]; `None` when no switch
    /// was pending. Call [`Self::finish`] once the caller's records match the journal.
    pub async fn resume(&self) -> Result<Option<SwitchJournal>> {
        let Some(journal) = self.pending().await? else {
            return Ok(None);
        };
        tracing::info!(from = %journal.from, to = %journal.to, step = ?journal.step, "finishing an interrupted version switch");
        self.run(journal).await.map(Some)
    }

    /// Ends a switch: removes the journal.
    pub async fn finish(&self) -> Result<()> {
        remove_file_if_exists(&self.journal_path()).await
    }

    /// The set folder that receives files for a version that is not active: its parked set,
    /// or a new prepared one when the instance never visited that version. The folder mirrors
    /// a game directory, so its `mods/` takes the files.
    pub async fn prepare(&self, info: SetInfo) -> Result<PathBuf> {
        if info.minecraft.trim().is_empty() {
            return Err(Error::Invalid("a set needs its Minecraft version".into()));
        }
        let dir = self.set_dir(&info.minecraft);
        if self.parked_info(&info.minecraft).await?.is_none() {
            let prepared = SetInfo {
                last_played_at: None,
                parked_at: None,
                size_bytes: 0,
                seeded: true,
                ..info
            };
            write_json(&dir.join(SET_FILE), &prepared).await?;
        }
        let mods = dir.join("mods");
        tokio::fs::create_dir_all(&mods)
            .await
            .map_err(|e| Error::io(&mods, e))?;
        Ok(dir)
    }

    /// Deletes a parked set for good and returns the bytes freed. The active set and sets of an
    /// unfinished switch are refused. (The launcher's cleanup moves sets into its own trash
    /// instead, through [`Self::removable`].)
    pub async fn delete_parked(&self, minecraft: &str) -> Result<u64> {
        let Some(dir) = self.removable(minecraft).await? else {
            return Ok(0);
        };
        let measured = dir.clone();
        let size = tokio::task::spawn_blocking(move || dir_size(&measured))
            .await
            .unwrap_or(0);
        tokio::fs::remove_dir_all(&dir)
            .await
            .map_err(|e| Error::io(&dir, e))?;
        Ok(size)
    }

    /// The folder of `minecraft`'s parked set, checked for removal: refused for the active set
    /// and for a set that is part of an unfinished switch. `None` when there is no such set.
    pub async fn removable(&self, minecraft: &str) -> Result<Option<PathBuf>> {
        if let Some(journal) = self.pending().await?
            && (journal.from == minecraft || journal.to == minecraft)
        {
            return Err(Error::Invalid(format!(
                "the {minecraft} set is part of an unfinished version switch"
            )));
        }
        if self
            .active_info()
            .await?
            .is_some_and(|info| info.minecraft == minecraft)
        {
            return Err(Error::Invalid(format!("the {minecraft} set is active")));
        }
        let dir = self.set_dir(minecraft);
        Ok(exists(&dir).await.then_some(dir))
    }

    /// Every parked set with its folder, last use and current size, newest Minecraft first.
    pub async fn parked_sets(&self) -> Result<Vec<ParkedSet>> {
        let mut sets = Vec::new();
        for info in self.parked().await? {
            let dir = self.set_dir(&info.minecraft);
            let measured = dir.clone();
            let (size_bytes, changed) = tokio::task::spawn_blocking(move || {
                let changed = std::fs::metadata(measured.join(SET_FILE))
                    .or_else(|_| std::fs::metadata(&measured))
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|time| Timestamp::try_from(time).ok());
                (dir_size(&measured), changed)
            })
            .await
            .unwrap_or((0, None));
            sets.push(ParkedSet {
                last_used: info.last_played_at.or(info.parked_at).or(changed),
                info,
                dir,
                size_bytes,
            });
        }
        Ok(sets)
    }

    /// Parked sets last used before `cutoff`, longest unused first: candidates for the cleanup
    /// of versions not played for a long time. Never the active set; none at all while a switch
    /// is unfinished (its folders are moving).
    pub async fn stale(&self, cutoff: Timestamp) -> Result<Vec<ParkedSet>> {
        if self.pending().await?.is_some() {
            return Ok(Vec::new());
        }
        let active = self.active_info().await?.map(|info| info.minecraft);
        let mut stale: Vec<ParkedSet> = self
            .parked_sets()
            .await?
            .into_iter()
            .filter(|set| active.as_deref() != Some(set.info.minecraft.as_str()))
            .filter(|set| set.last_used.is_some_and(|used| used < cutoff))
            .collect();
        stale.sort_by(|a, b| {
            a.last_used
                .cmp(&b.last_used)
                .then_with(|| newest_first(&a.info, &b.info))
        });
        Ok(stale)
    }

    /// Checks the request, splits the pack state and writes the journal (step `Park`).
    async fn begin(&self, request: SwitchRequest) -> Result<SwitchJournal> {
        if let Some(pending) = self.pending().await? {
            return Err(Error::Invalid(format!(
                "the version switch from {} to {} is not finished",
                pending.from, pending.to
            )));
        }
        let from = request.from.minecraft.trim().to_owned();
        let to = request.to.minecraft.trim().to_owned();
        if from.is_empty() || to.is_empty() {
            return Err(Error::Invalid(
                "a version switch needs both versions".into(),
            ));
        }
        if from == to || folder_name(&from) == folder_name(&to) {
            return Err(Error::Invalid(format!(
                "{from} is already the active version"
            )));
        }

        let target = self.parked_info(&to).await?;
        let prepared = target.as_ref().is_some_and(|info| info.seeded);
        let returning = target.is_some() && !prepared;
        let returning_state = if returning {
            PackState::load_from(&self.set_dir(&to).join(PACK_STATE_FILE)).await?
        } else {
            None
        };
        let active_state = PackState::load(&self.game_dir).await?;
        let (parked_state, active_state) =
            split_states(active_state, &from, &to, returning, returning_state);

        // A leftover parked folder of the version being left would collide: set it aside.
        let parked_dir = self.set_dir(&from);
        let now = Timestamp::now();
        let mut moved_aside = Vec::new();
        for dir in PARKED_DIRS {
            let leftover = parked_dir.join(dir);
            if exists(&leftover).await {
                moved_aside.push(self.move_aside(&leftover, &parked_dir, dir, now).await?);
            }
        }

        let recorded = self
            .active_info()
            .await?
            .filter(|info| info.minecraft == from);
        let parked_info = SetInfo {
            minecraft: from.clone(),
            last_played_at: request
                .from
                .last_played_at
                .or_else(|| recorded.and_then(|info| info.last_played_at)),
            parked_at: Some(now),
            size_bytes: 0,
            seeded: false,
            ..request.from
        };
        let active_info = SetInfo {
            minecraft: to.clone(),
            last_played_at: target
                .filter(|_| returning)
                .and_then(|info| info.last_played_at),
            parked_at: None,
            size_bytes: 0,
            seeded: false,
            ..request.to
        };
        let journal = SwitchJournal {
            from,
            to,
            step: SwitchStep::Park,
            first_visit: !returning,
            prepared,
            parked_state,
            active_state,
            parked_info,
            active_info,
            moved_aside,
            context: request.context,
            started_at: now,
        };
        self.save_journal(&journal).await?;
        Ok(journal)
    }

    async fn run(&self, mut journal: SwitchJournal) -> Result<SwitchJournal> {
        loop {
            match journal.step {
                SwitchStep::Park => {
                    self.park(&mut journal).await?;
                    journal.step = SwitchStep::Restore;
                }
                SwitchStep::Restore => {
                    self.restore(&mut journal).await?;
                    journal.step = SwitchStep::States;
                }
                SwitchStep::States => {
                    self.write_states(&journal).await?;
                    journal.step = SwitchStep::Finalize;
                }
                SwitchStep::Finalize => return Ok(journal),
            }
            self.save_journal(&journal).await?;
        }
    }

    /// Moves the active folders into the leaving set's folder. Repeatable: a folder already
    /// moved is skipped.
    async fn park(&self, journal: &mut SwitchJournal) -> Result<()> {
        let parked = self.set_dir(&journal.from);
        create_dir(&parked).await?;
        for dir in PARKED_DIRS {
            let active = self.game_dir.join(dir);
            let destination = parked.join(dir);
            match (exists(&active).await, exists(&destination).await) {
                (true, false) => rename(&active, &destination).await?,
                (true, true) => tracing::warn!(
                    folder = dir,
                    "a parked folder and an active one both exist; leaving both"
                ),
                _ => {}
            }
        }
        Ok(())
    }

    /// Brings the target's folders in: its parked ones (returning, or a prepared `mods/`), a
    /// copy of the leaving set's [`COPIED_DIRS`] on the first visit, otherwise an empty
    /// `mods/`. Repeatable.
    async fn restore(&self, journal: &mut SwitchJournal) -> Result<()> {
        let target = self.set_dir(&journal.to);
        let parked = self.set_dir(&journal.from);
        let now = Timestamp::now();
        for dir in PARKED_DIRS {
            let active = self.game_dir.join(dir);
            let copied = journal.first_visit && COPIED_DIRS.contains(&dir);
            let incoming = target.join(dir);
            if !copied && exists(&incoming).await {
                if exists(&active).await {
                    let aside = self.move_aside(&active, &parked, dir, now).await?;
                    journal.moved_aside.push(aside);
                }
                rename(&incoming, &active).await?;
            } else if copied {
                if !exists(&active).await {
                    let source = parked.join(dir);
                    let staging = self.game_dir.join(format!("{dir}{COPY_SUFFIX}"));
                    if exists(&source).await {
                        let (from, into) = (source.clone(), staging.clone());
                        tokio::task::spawn_blocking(move || {
                            if into.exists() {
                                std::fs::remove_dir_all(&into).map_err(|e| Error::io(&into, e))?;
                            }
                            copy_dir(&from, &into)
                        })
                        .await
                        .map_err(|e| Error::Invalid(format!("copy task failed: {e}")))??;
                        rename(&staging, &active).await?;
                    }
                }
            } else if dir == "mods" && !exists(&active).await {
                create_dir(&active).await?;
            }
        }
        Ok(())
    }

    /// Writes the parked and the active pack state and metadata, then removes the target's
    /// parked leftovers (its folders are active now). Repeatable.
    async fn write_states(&self, journal: &SwitchJournal) -> Result<()> {
        let parked = self.set_dir(&journal.from);
        let parked_state_path = parked.join(PACK_STATE_FILE);
        match &journal.parked_state {
            Some(state) => state.save_to(&parked_state_path).await?,
            None => remove_file_if_exists(&parked_state_path).await?,
        }
        let measured = parked.clone();
        let size = tokio::task::spawn_blocking(move || {
            PARKED_DIRS
                .iter()
                .map(|d| dir_size(&measured.join(d)))
                .sum::<u64>()
        })
        .await
        .unwrap_or(0);
        let parked_info = SetInfo {
            size_bytes: size,
            ..journal.parked_info.clone()
        };
        write_json(&parked.join(SET_FILE), &parked_info).await?;

        match &journal.active_state {
            Some(state) => state.save(&self.game_dir).await?,
            None => remove_file_if_exists(&PackState::path(&self.game_dir)).await?,
        }
        write_json(&self.active_info_path(), &journal.active_info).await?;

        let target = self.set_dir(&journal.to);
        remove_file_if_exists(&target.join(PACK_STATE_FILE)).await?;
        remove_file_if_exists(&target.join(SET_FILE)).await?;
        // Only an empty folder goes; anything else left in it stays visible as a parked set.
        let _ = tokio::fs::remove_dir(&target).await;
        Ok(())
    }

    /// Moves `path` into `into` as `<name>.aside-<seconds>` and returns the new location
    /// relative to the game directory.
    async fn move_aside(
        &self,
        path: &Path,
        into: &Path,
        name: &str,
        now: Timestamp,
    ) -> Result<String> {
        let mut aside = into.join(format!("{name}.aside-{}", now.as_second()));
        let mut counter = 1;
        while exists(&aside).await {
            counter += 1;
            aside = into.join(format!("{name}.aside-{}-{counter}", now.as_second()));
        }
        tracing::warn!(from = %path.display(), to = %aside.display(), "moving a folder that is in the way aside");
        rename(path, &aside).await?;
        Ok(aside
            .strip_prefix(&self.game_dir)
            .unwrap_or(&aside)
            .to_string_lossy()
            .replace('\\', "/"))
    }

    async fn save_journal(&self, journal: &SwitchJournal) -> Result<()> {
        write_json(&self.journal_path(), journal).await
    }
}

/// The pack states after a switch: `(parked, active)`. Returning to a set whose pack state is
/// missing still carries the choices over (but none of the leaving set's folder records).
fn split_states(
    active: Option<PackState>,
    from: &str,
    to: &str,
    returning: bool,
    returning_state: Option<PackState>,
) -> (Option<PackState>, Option<PackState>) {
    match active {
        Some(active) => {
            let returning_state = match returning_state {
                Some(state) => Some(state),
                None if returning => Some(PackState {
                    family: active.family.clone(),
                    groups: active.groups.clone(),
                    items: active.items.clone(),
                    seen_items: active.seen_items.clone(),
                    ..PackState::default()
                }),
                None => None,
            };
            let switch = active.switch_version(from, to, returning_state);
            (Some(switch.parked), Some(switch.active))
        }
        None => (
            None,
            returning_state.map(|state| PackState {
                minecraft: to.to_owned(),
                ..state
            }),
        ),
    }
}

/// `text` as one safe folder name: characters outside `[A-Za-z0-9._-]` become `_`; names the
/// path rules refuse (empty, `.`, `..`, trailing dots, device names) are hex-encoded with a `_`
/// prefix. Callers that keep their own per-set files use it to name them the same way.
pub fn folder_name(text: &str) -> String {
    let name: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if check_relative_path(&name).is_ok() {
        name
    } else {
        let hex: String = text.bytes().map(|b| format!("{b:02x}")).collect();
        format!("_{hex}")
    }
}

/// Metadata and pack state of a set folder kept outside the game directory: the launcher's trash
/// keeps a cleaned-up set's folder as it was parked, and the version dialog shows what putting
/// it back would bring.
pub async fn read_set_dir(dir: &Path) -> Result<(Option<SetInfo>, Option<PackState>)> {
    let info = read_json::<SetInfo>(&dir.join(SET_FILE)).await?;
    let state = PackState::load_from(&dir.join(PACK_STATE_FILE)).await?;
    Ok((info, state))
}

async fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let Some(bytes) = read_optional(path).await? else {
        return Ok(None);
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => Ok(Some(value)),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "ignoring unreadable set metadata");
            Ok(None)
        }
    }
}

async fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| Error::json(SET_FILE, e))?;
    write_durable(path, &bytes).await
}

async fn exists(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path).await.is_ok()
}

async fn create_dir(path: &Path) -> Result<()> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|e| Error::io(path, e))
}

async fn rename(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        create_dir(parent).await?;
    }
    tokio::fs::rename(from, to)
        .await
        .map_err(|e| Error::io(from, e))
}

async fn remove_file_if_exists(path: &Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io(path, e)),
    }
}

/// Bytes under `path` (0 when it is missing). Symbolic links are not followed.
fn dir_size(path: &Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.is_file() {
        return metadata.len();
    }
    if !metadata.is_dir() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| dir_size(&entry.path()))
                .sum()
        })
        .unwrap_or(0)
}

/// Copies a folder tree (files and folders; symbolic links are skipped).
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to).map_err(|e| Error::io(to, e))?;
    for entry in std::fs::read_dir(from).map_err(|e| Error::io(from, e))? {
        let entry = entry.map_err(|e| Error::io(from, e))?;
        let kind = entry.file_type().map_err(|e| Error::io(entry.path(), e))?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &target).map_err(|e| Error::io(entry.path(), e))?;
        } else {
            tracing::debug!(path = %entry.path().display(), "not copying a link");
        }
    }
    Ok(())
}

/// Orders sets for display: newest Minecraft version first.
pub fn newest_first(a: &SetInfo, b: &SetInfo) -> Ordering {
    compare_minecraft_versions(&b.minecraft, &a.minecraft)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::model::FilePolicy;
    use crate::sync::InstalledFile;
    use crate::testutil::sha1_hex;

    fn write(dir: &Path, rel: &str, content: &[u8]) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn read(dir: &Path, rel: &str) -> Vec<u8> {
        std::fs::read(dir.join(rel)).unwrap()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn info(minecraft: &str) -> SetInfo {
        SetInfo {
            minecraft: minecraft.into(),
            loader: "fabric".into(),
            ..SetInfo::default()
        }
    }

    fn request(from: &str, to: &str) -> SwitchRequest {
        SwitchRequest {
            from: info(from),
            to: info(to),
            context: serde_json::json!({ "loader": "fabric", "to": to }),
        }
    }

    fn record(content: &[u8], item: Option<&str>) -> InstalledFile {
        InstalledFile {
            sha1: sha1_hex(content),
            policy: FilePolicy::Overwrite,
            item: item.map(str::to_owned),
            disabled: false,
            modified: false,
        }
    }

    /// A pack instance on 1.21.1: a pack mod, an own mod, a config and shared files.
    fn pack_game(game: &Path) -> PackState {
        write(game, "mods/sodium.jar", b"sodium 1.21.1");
        write(game, "mods/own.jar", b"own mod");
        write(game, "config/sodium.json", b"{\"v\":1}");
        write(game, "config/nested/deep.toml", b"deep");
        write(game, "options.txt", b"shared options");
        write(game, "resourcepacks/pack.zip", b"resource pack");
        write(game, "saves/World/level.dat", b"world");
        PackState {
            pack_id: "projects".into(),
            version_id: "2.3.0".into(),
            family: "projects".into(),
            minecraft: "1.21.1".into(),
            files: BTreeMap::from([
                (
                    "mods/sodium.jar".into(),
                    record(b"sodium 1.21.1", Some("sodium")),
                ),
                ("config/sodium.json".into(), record(b"{\"v\":1}", None)),
                ("options.txt".into(), record(b"shared options", None)),
            ]),
            items: BTreeMap::from([("sodium".into(), true), ("iris".into(), false)]),
            seen_items: BTreeSet::from(["sodium".into(), "iris".into()]),
            ..PackState::default()
        }
    }

    #[tokio::test]
    async fn switching_parks_starts_and_restores_sets() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let state = pack_game(game);
        state.save(game).await.unwrap();
        let sets = VersionSets::new(game);
        let played: Timestamp = "2026-10-01T12:00:00Z".parse().unwrap();
        sets.record_played(info("1.21.1"), played).await.unwrap();

        // First visit of 1.21.4.
        let journal = sets.switch(request("1.21.1", "1.21.4")).await.unwrap();
        assert_eq!(journal.step, SwitchStep::Finalize);
        assert!(journal.first_visit && !journal.prepared);
        assert_eq!(journal.context["to"], "1.21.4");
        assert!(
            sets.pending().await.unwrap().is_some(),
            "until the caller finishes"
        );
        sets.finish().await.unwrap();
        assert!(sets.pending().await.unwrap().is_none());

        // mods/ starts empty, config/ is a copy, shared files did not move.
        assert!(names(&game.join("mods")).is_empty());
        assert_eq!(read(game, "config/sodium.json"), b"{\"v\":1}");
        assert_eq!(read(game, "config/nested/deep.toml"), b"deep");
        assert_eq!(read(game, "options.txt"), b"shared options");
        assert_eq!(read(game, "resourcepacks/pack.zip"), b"resource pack");
        assert_eq!(read(game, "saves/World/level.dat"), b"world");
        assert!(!game.join(format!("config{COPY_SUFFIX}")).exists());
        let parked = sets.set_dir("1.21.1");
        assert_eq!(read(&parked, "mods/sodium.jar"), b"sodium 1.21.1");
        assert_eq!(read(&parked, "mods/own.jar"), b"own mod");
        assert_eq!(read(&parked, "config/sodium.json"), b"{\"v\":1}");
        let parked_info = sets.parked_info("1.21.1").await.unwrap().unwrap();
        assert_eq!(parked_info.last_played_at, Some(played));
        assert!(parked_info.parked_at.is_some() && !parked_info.seeded);
        assert_eq!(
            parked_info.size_bytes,
            (b"sodium 1.21.1".len() + b"own mod".len() + b"{\"v\":1}".len() + b"deep".len()) as u64
        );

        // The new active state carries the choices and the copied config's record.
        let active = PackState::load(game).await.unwrap().unwrap();
        assert_eq!(active.items, state.items);
        assert_eq!(active.seen_items, state.seen_items);
        assert_eq!(active.minecraft, "1.21.4");
        assert_eq!(
            active.files.keys().collect::<Vec<_>>(),
            ["config/sodium.json", "options.txt"]
        );
        let parked_state = PackState::load_from(&parked.join(PACK_STATE_FILE))
            .await
            .unwrap()
            .unwrap();
        assert!(parked_state.files.contains_key("mods/sodium.jar"));
        assert_eq!(
            sets.active_info().await.unwrap().unwrap().minecraft,
            "1.21.4"
        );

        // 1.21.4 gets its own mods, config and choices.
        write(game, "mods/sodium-new.jar", b"sodium 1.21.4");
        write(game, "config/sodium.json", b"{\"v\":2}");
        let mut v1214 = PackState::load(game).await.unwrap().unwrap();
        v1214.items.insert("sodium".into(), false);
        v1214.save(game).await.unwrap();

        // Back to 1.21.1: exactly as it was left.
        let journal = sets.switch(request("1.21.4", "1.21.1")).await.unwrap();
        assert!(!journal.first_visit);
        sets.finish().await.unwrap();
        assert_eq!(names(&game.join("mods")), ["own.jar", "sodium.jar"]);
        assert_eq!(read(game, "mods/sodium.jar"), b"sodium 1.21.1");
        assert_eq!(read(game, "config/sodium.json"), b"{\"v\":1}");
        assert_eq!(read(game, "options.txt"), b"shared options");
        let back = PackState::load(game).await.unwrap().unwrap();
        assert_eq!(back.items, state.items);
        assert_eq!(back.files, state.files);
        assert_eq!(back.minecraft, "1.21.1");
        assert!(!parked.exists(), "the returned set's folder is gone");
        let active_info = sets.active_info().await.unwrap().unwrap();
        assert_eq!(
            (active_info.minecraft.as_str(), active_info.last_played_at),
            ("1.21.1", Some(played))
        );

        // 1.21.4 is parked with its own files.
        let v1214_dir = sets.set_dir("1.21.4");
        assert_eq!(read(&v1214_dir, "mods/sodium-new.jar"), b"sodium 1.21.4");
        assert_eq!(read(&v1214_dir, "config/sodium.json"), b"{\"v\":2}");
        let listed: Vec<String> = sets
            .parked()
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.minecraft)
            .collect();
        assert_eq!(listed, ["1.21.4"]);

        // And to 1.21.4 again: its choices come back.
        sets.switch(request("1.21.1", "1.21.4")).await.unwrap();
        sets.finish().await.unwrap();
        let again = PackState::load(game).await.unwrap().unwrap();
        assert!(!again.items["sodium"]);
        assert_eq!(read(game, "mods/sodium-new.jar"), b"sodium 1.21.4");
    }

    /// Cleanup moves a parked set's folder out of the game directory (the launcher's trash):
    /// it reads there like a parked set, the version then has no set, and once the folder is
    /// put back the next switch brings it back exactly (not a first visit).
    #[tokio::test]
    async fn a_set_kept_outside_comes_back_when_put_back() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let outside = dir.path().join("trash-item");
        std::fs::create_dir_all(&outside).unwrap();
        let state = pack_game(&game);
        state.save(&game).await.unwrap();
        let sets = VersionSets::new(&game);
        let played: Timestamp = "2026-08-01T12:00:00Z".parse().unwrap();
        sets.record_played(info("1.21.1"), played).await.unwrap();
        sets.switch(request("1.21.1", "1.21.4")).await.unwrap();
        sets.finish().await.unwrap();

        let kept = outside.join(folder_name("1.21.1"));
        std::fs::rename(sets.set_dir("1.21.1"), &kept).unwrap();
        assert!(sets.parked_info("1.21.1").await.unwrap().is_none());
        let (kept_info, kept_state) = read_set_dir(&kept).await.unwrap();
        let kept_info = kept_info.unwrap();
        assert_eq!(kept_info.minecraft, "1.21.1");
        assert_eq!(kept_info.last_played_at, Some(played));
        let kept_state = kept_state.unwrap();
        assert!(kept_state.files.contains_key("mods/sodium.jar"));
        assert_eq!(kept_state.items, state.items);
        assert_eq!(
            read_set_dir(&outside.join("missing")).await.unwrap(),
            (None, None)
        );

        std::fs::rename(&kept, sets.set_dir("1.21.1")).unwrap();
        let journal = sets.switch(request("1.21.4", "1.21.1")).await.unwrap();
        assert!(!journal.first_visit);
        sets.finish().await.unwrap();
        assert_eq!(names(&game.join("mods")), ["own.jar", "sodium.jar"]);
        assert_eq!(read(&game, "config/sodium.json"), b"{\"v\":1}");
        assert_eq!(
            PackState::load(&game).await.unwrap().unwrap().items,
            state.items
        );
    }

    #[tokio::test]
    async fn instances_without_a_pack_park_their_folders_too() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        write(game, "mods/a.jar", b"a");
        write(game, "config/a.json", b"{}");
        let sets = VersionSets::new(game);

        sets.switch(request("1.20.1", "1.21.1")).await.unwrap();
        sets.finish().await.unwrap();
        assert!(names(&game.join("mods")).is_empty());
        assert_eq!(read(game, "config/a.json"), b"{}");
        write(game, "mods/b.jar", b"b");
        sets.switch(request("1.21.1", "1.21.4")).await.unwrap();
        sets.finish().await.unwrap();
        sets.switch(request("1.21.4", "1.20.1")).await.unwrap();
        sets.finish().await.unwrap();
        assert_eq!(names(&game.join("mods")), ["a.jar"]);
        assert!(!PackState::path(game).exists(), "no pack state appears");

        let listed: Vec<String> = sets
            .parked()
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.minecraft)
            .collect();
        assert_eq!(listed, ["1.21.4", "1.21.1"]);

        // Cleanup frees a parked set; the active one and unfinished switches are refused.
        let freed = sets.delete_parked("1.21.1").await.unwrap();
        assert!(
            freed > 1 + 2,
            "b.jar, the copied config and the metadata: {freed}"
        );
        assert!(!sets.set_dir("1.21.1").exists());
        assert!(sets.delete_parked("1.20.1").await.is_err());
        assert_eq!(sets.delete_parked("1.19.4").await.unwrap(), 0);
        assert!(sets.active_size().await >= 3);
    }

    #[tokio::test]
    async fn a_prepared_version_keeps_its_mods_on_the_first_visit() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let state = pack_game(game);
        state.save(game).await.unwrap();
        let sets = VersionSets::new(game);

        let prepared = sets.prepare(info("1.21.4")).await.unwrap();
        write(&prepared, "mods/migrated.jar", b"migrated for 1.21.4");
        let seeded = sets.parked_info("1.21.4").await.unwrap().unwrap();
        assert!(seeded.seeded);
        // Preparing again keeps what is there.
        assert_eq!(sets.prepare(info("1.21.4")).await.unwrap(), prepared);
        assert!(prepared.join("mods/migrated.jar").exists());

        let journal = sets.switch(request("1.21.1", "1.21.4")).await.unwrap();
        sets.finish().await.unwrap();
        assert!(journal.first_visit && journal.prepared);
        assert_eq!(names(&game.join("mods")), ["migrated.jar"]);
        assert_eq!(read(game, "config/sodium.json"), b"{\"v\":1}");
        let active = PackState::load(game).await.unwrap().unwrap();
        assert_eq!(active.items, state.items, "choices still carry over");
        assert!(!active.files.contains_key("mods/sodium.jar"));
        assert!(!prepared.exists());
    }

    #[tokio::test]
    async fn an_interrupted_switch_is_finished_on_resume() {
        for stop in [SwitchStep::Park, SwitchStep::Restore, SwitchStep::States] {
            let dir = tempfile::tempdir().unwrap();
            let game = dir.path();
            let state = pack_game(game);
            state.save(game).await.unwrap();
            let sets = VersionSets::new(game);

            // The launcher wrote the journal and got through part of the switch.
            let mut journal = sets.begin(request("1.21.1", "1.21.4")).await.unwrap();
            match stop {
                SwitchStep::Park => {
                    // Only mods/ moved before the crash.
                    let parked = sets.set_dir("1.21.1");
                    std::fs::create_dir_all(&parked).unwrap();
                    std::fs::rename(game.join("mods"), parked.join("mods")).unwrap();
                }
                SwitchStep::Restore => {
                    sets.park(&mut journal).await.unwrap();
                    journal.step = SwitchStep::Restore;
                    sets.save_journal(&journal).await.unwrap();
                    // A half-finished copy of config/.
                    write(game, &format!("config{COPY_SUFFIX}/partial.json"), b"half");
                }
                SwitchStep::States => {
                    sets.park(&mut journal).await.unwrap();
                    sets.restore(&mut journal).await.unwrap();
                    journal.step = SwitchStep::States;
                    sets.save_journal(&journal).await.unwrap();
                    // The parked state was written, the active one not yet.
                    journal
                        .parked_state
                        .as_ref()
                        .unwrap()
                        .save_to(&sets.set_dir("1.21.1").join(PACK_STATE_FILE))
                        .await
                        .unwrap();
                }
                SwitchStep::Finalize => unreachable!(),
            }
            // A new switch waits for the old one.
            assert!(sets.switch(request("1.21.1", "1.20.1")).await.is_err());

            let resumed = sets.resume().await.unwrap().unwrap();
            assert_eq!(resumed.step, SwitchStep::Finalize, "{stop:?}");
            assert_eq!(resumed.context["to"], "1.21.4");
            sets.finish().await.unwrap();

            assert!(names(&game.join("mods")).is_empty(), "{stop:?}");
            assert_eq!(
                names(&game.join("config")),
                ["nested", "sodium.json"],
                "{stop:?}"
            );
            assert!(
                !game.join(format!("config{COPY_SUFFIX}")).exists(),
                "{stop:?}"
            );
            let parked = sets.set_dir("1.21.1");
            assert_eq!(
                names(&parked.join("mods")),
                ["own.jar", "sodium.jar"],
                "{stop:?}"
            );
            let active = PackState::load(game).await.unwrap().unwrap();
            assert_eq!(active.minecraft, "1.21.4", "{stop:?}");
            assert!(parked.join(PACK_STATE_FILE).is_file() && parked.join(SET_FILE).is_file());
            assert!(sets.resume().await.unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn folders_in_the_way_are_moved_aside_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        write(game, "mods/a.jar", b"a");
        let sets = VersionSets::new(game);
        // A stray parked folder for the version being left.
        write(&sets.set_dir("1.20.1"), "mods/stray.jar", b"stray");
        let journal = sets.switch(request("1.20.1", "1.21.1")).await.unwrap();
        sets.finish().await.unwrap();
        assert_eq!(journal.moved_aside.len(), 1);
        assert!(
            game.join(&journal.moved_aside[0])
                .join("stray.jar")
                .is_file()
        );
        assert_eq!(read(&sets.set_dir("1.20.1"), "mods/a.jar"), b"a");
    }

    #[tokio::test]
    async fn unreadable_journals_are_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let sets = VersionSets::new(dir.path());
        write(
            dir.path(),
            &format!("{LAUNCHER_DIR}/{SWITCH_JOURNAL}"),
            b"{broken",
        );
        assert!(sets.pending().await.unwrap().is_none());
        assert!(!dir.path().join(LAUNCHER_DIR).join(SWITCH_JOURNAL).exists());
        assert_eq!(names(&dir.path().join(LAUNCHER_DIR)).len(), 1);
    }

    #[test]
    fn set_folder_names_are_safe() {
        let sets = VersionSets::new("/game");
        let base = Path::new("/game").join(LAUNCHER_DIR).join(SETS_DIR);
        assert_eq!(sets.set_dir("1.21.4"), base.join("1.21.4"));
        assert_eq!(
            sets.set_dir("1.14 Pre-Release 1"),
            base.join("1.14_Pre-Release_1")
        );
        assert_eq!(sets.set_dir(".."), base.join("_2e2e"));
        assert_eq!(sets.set_dir("con"), base.join("_636f6e"));
        let mut list = [info("1.20.1"), info("26.1"), info("1.21.10")];
        list.sort_by(newest_first);
        let order: Vec<&str> = list.iter().map(|i| i.minecraft.as_str()).collect();
        assert_eq!(order, ["26.1", "1.21.10", "1.20.1"]);
    }

    /// A parked set's metadata, played `days_ago` days ago.
    fn parked_info(minecraft: &str, days_ago: Option<i64>) -> String {
        let played = days_ago.map(|days| {
            Timestamp::now()
                .checked_sub(jiff::SignedDuration::from_hours(24 * days))
                .unwrap()
        });
        serde_json::to_string(&SetInfo {
            last_played_at: played,
            size_bytes: 1,
            ..info(minecraft)
        })
        .unwrap()
    }

    #[tokio::test]
    async fn stale_sets_are_the_long_unplayed_parked_ones() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let sets = VersionSets::new(game);
        write(
            game,
            &format!("{LAUNCHER_DIR}/{SET_FILE}"),
            parked_info("1.21.1", Some(0)).as_bytes(),
        );
        let parked = |minecraft: &str, days: Option<i64>| {
            let base = format!("{LAUNCHER_DIR}/{SETS_DIR}/{minecraft}");
            write(
                game,
                &format!("{base}/{SET_FILE}"),
                parked_info(minecraft, days).as_bytes(),
            );
            write(game, &format!("{base}/mods/m.jar"), &[0u8; 100]);
        };
        parked("1.20.1", Some(90));
        parked("1.19.2", Some(120));
        parked("1.20.4", Some(10));
        // A set without metadata: dated by its folder (fresh here).
        write(
            game,
            &format!("{LAUNCHER_DIR}/{SETS_DIR}/1.18.2/mods/x.jar"),
            b"x",
        );

        let cutoff = Timestamp::now()
            .checked_sub(jiff::SignedDuration::from_hours(24 * 60))
            .unwrap();
        let stale = sets.stale(cutoff).await.unwrap();
        let names: Vec<&str> = stale.iter().map(|s| s.info.minecraft.as_str()).collect();
        assert_eq!(names, ["1.19.2", "1.20.1"], "longest unused first");
        assert!(stale.iter().all(|s| s.size_bytes > 100));
        assert_eq!(stale[1].dir, sets.set_dir("1.20.1"));
        let all = sets.parked_sets().await.unwrap();
        assert_eq!(all.len(), 4);
        assert!(all.iter().all(|s| s.last_used.is_some()));

        // Removal checks: the active set and unknown versions.
        assert!(sets.removable("1.21.1").await.is_err());
        assert_eq!(sets.removable("1.16.5").await.unwrap(), None);
        assert_eq!(
            sets.removable("1.20.1").await.unwrap(),
            Some(sets.set_dir("1.20.1"))
        );

        // While a switch is unfinished nothing is offered, and its sets are refused.
        let journal = SwitchJournal {
            from: "1.21.1".into(),
            to: "1.20.1".into(),
            step: SwitchStep::Park,
            first_visit: false,
            prepared: false,
            parked_state: None,
            active_state: None,
            parked_info: info("1.21.1"),
            active_info: info("1.20.1"),
            moved_aside: Vec::new(),
            context: serde_json::Value::Null,
            started_at: Timestamp::now(),
        };
        write(
            game,
            &format!("{LAUNCHER_DIR}/{SWITCH_JOURNAL}"),
            &serde_json::to_vec(&journal).unwrap(),
        );
        assert!(sets.stale(cutoff).await.unwrap().is_empty());
        assert!(sets.removable("1.20.1").await.is_err());
        assert!(sets.removable("1.19.2").await.unwrap().is_some());
        assert!(sets.delete_parked("1.20.1").await.is_err());
        assert!(sets.delete_parked("1.19.2").await.unwrap() > 100);
        assert!(!sets.set_dir("1.19.2").exists());
    }
}
