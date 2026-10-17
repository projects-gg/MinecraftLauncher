//! The launcher's own trash (`<root>/trash/`). Worlds, world backups and screenshots removed from
//! the instance page, mod folders set aside by "Yeniden yükle" and version sets cleaned up in
//! Settings → Veri are moved here instead of being deleted, so they can be put back from the
//! launcher (the OS trash is slow or missing for big folders and cannot be listed per instance).
//!
//! Layout: `trash/<id>/trash.json` describes the item, `trash/<id>/item/<original name>` is the
//! item itself. [`LauncherTrash::purge`] applies [`TrashLimits`]: items older than
//! [`TRASH_RETENTION`] go, and when the trash grows over its size cap the oldest items go first
//! (never the newest one, so what the player just deleted can always be put back).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::fsops::{blocking, dir_size, read_dir_or_empty, write_atomic};
use crate::error::{Error, Result};
use crate::paths::{copy_file_durable, safe_name};

/// How long items stay in the trash.
pub const TRASH_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// A slot without a complete description and item is an interrupted move; it is cleaned up once
/// it is this old (a move in progress is never touched).
pub const INCOMPLETE_SLOT_AGE: Duration = Duration::from_secs(24 * 60 * 60);

const META_FILE: &str = "trash.json";
const ITEM_DIR: &str = "item";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TrashKind {
    World,
    Backup,
    Screenshot,
    /// A version set's `mods/` or `config/` folder, set aside by "Yeniden yükle".
    ModSet,
    /// A whole parked version set (`.projects/versions/<minecraft>/`), cleaned up because the
    /// instance had not played that version for a long time.
    VersionSet,
    /// A whole instance folder (settings and game files), deleted from the library; `instance_id`
    /// is its id, `name` its display name.
    Instance,
}

/// What [`LauncherTrash::purge`] removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrashLimits {
    /// Items deleted longer ago than this go.
    pub max_age: Duration,
    /// When the items together take more than this, the oldest go first (the newest item always
    /// stays). `None`: no size cap.
    pub max_bytes: Option<u64>,
    /// Interrupted moves (slots without a complete item) older than this go.
    pub incomplete_age: Duration,
}

impl Default for TrashLimits {
    fn default() -> Self {
        Self {
            max_age: TRASH_RETENTION,
            max_bytes: None,
            incomplete_age: INCOMPLETE_SLOT_AGE,
        }
    }
}

/// What a purge (or emptying the trash) removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PurgeReport {
    pub removed: usize,
    pub freed_bytes: u64,
}

/// How much the trash holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrashUsage {
    pub items: usize,
    pub bytes: u64,
    /// When the oldest item was deleted (ms since the Unix epoch).
    pub oldest_deleted_ms: Option<i64>,
}

/// Something in the launcher trash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrashEntry {
    pub id: String,
    pub kind: TrashKind,
    /// Instance the item came from.
    pub instance_id: String,
    /// What the UI shows (a world's own name, a file name).
    pub name: String,
    /// File or folder name the item had.
    pub file_name: String,
    /// Where the item lived; restoring puts it back there (or next to it if the name is taken).
    pub original_path: PathBuf,
    /// The same place relative to the launcher data folder (the trash's parent), so items go
    /// back to the right place after the data folder was moved.
    #[serde(default)]
    pub relative_path: Option<PathBuf>,
    pub deleted_at_ms: i64,
    /// Bytes on disk.
    pub size: u64,
}

/// The launcher trash folder. Cheap to clone.
#[derive(Debug, Clone)]
pub struct LauncherTrash {
    dir: PathBuf,
}

impl LauncherTrash {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the item of `entry` sits inside the trash (a file can be copied back from there).
    pub fn item_path(&self, entry: &TrashEntry) -> PathBuf {
        self.dir
            .join(&entry.id)
            .join(ITEM_DIR)
            .join(&entry.file_name)
    }

    /// Move `path` (file or folder) into the trash.
    pub async fn put(
        &self,
        path: &Path,
        kind: TrashKind,
        instance_id: &str,
        name: &str,
    ) -> Result<TrashEntry> {
        let trash = self.clone();
        let path = path.to_path_buf();
        let instance_id = instance_id.to_owned();
        let name = name.to_owned();
        blocking(move || trash.put_blocking(&path, kind, &instance_id, &name)).await
    }

    /// [`Self::put`] for code that already runs on a blocking thread.
    pub fn put_blocking(
        &self,
        path: &Path,
        kind: TrashKind,
        instance_id: &str,
        name: &str,
    ) -> Result<TrashEntry> {
        let meta = std::fs::symlink_metadata(path).map_err(|e| Error::io(path, e))?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| Error::Invalid(format!("{} has no file name", path.display())))?;
        let size = if meta.is_dir() {
            dir_size(path)
        } else {
            meta.len()
        };
        let now = jiff::Zoned::now();
        let id = format!(
            "{}-{}",
            now.strftime("%Y%m%d-%H%M%S"),
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        );
        let slot = self.dir.join(&id);
        let entry = TrashEntry {
            id,
            kind,
            instance_id: instance_id.to_owned(),
            name: name.to_owned(),
            file_name: file_name.clone(),
            original_path: path.to_path_buf(),
            relative_path: self
                .dir
                .parent()
                .and_then(|root| path.strip_prefix(root).ok())
                .map(Path::to_path_buf),
            deleted_at_ms: now.timestamp().as_millisecond(),
            size,
        };
        let items = slot.join(ITEM_DIR);
        std::fs::create_dir_all(&items).map_err(|e| Error::io(&items, e))?;
        let bytes = serde_json::to_vec_pretty(&entry).map_err(|e| Error::json("trash entry", e))?;
        let moved = write_atomic(&slot.join(META_FILE), &bytes)
            .and_then(|()| move_path(path, &items.join(&file_name)));
        if let Err(error) = moved {
            let _ = std::fs::remove_dir_all(&slot);
            return Err(error);
        }
        Ok(entry)
    }

    /// Items in the trash, newest first; `instance_id` keeps one instance's items.
    pub async fn list(&self, instance_id: Option<&str>) -> Result<Vec<TrashEntry>> {
        let dir = self.dir.clone();
        let instance_id = instance_id.map(str::to_owned);
        blocking(move || {
            let mut entries: Vec<TrashEntry> = read_dir_or_empty(&dir)?
                .into_iter()
                .filter_map(|slot| read_entry(&slot.path()))
                .filter(|e| instance_id.as_ref().is_none_or(|id| &e.instance_id == id))
                .collect();
            entries.sort_by(|a, b| {
                b.deleted_at_ms
                    .cmp(&a.deleted_at_ms)
                    .then_with(|| b.id.cmp(&a.id))
            });
            Ok(entries)
        })
        .await
    }

    /// The newest item of `kind` named `name` that came from `instance_id` (the set of one
    /// Minecraft version that cleanup moved here, for "Eski seti geri getir").
    pub async fn latest(
        &self,
        instance_id: &str,
        kind: TrashKind,
        name: &str,
    ) -> Result<Option<TrashEntry>> {
        Ok(self
            .list(Some(instance_id))
            .await?
            .into_iter()
            .find(|entry| entry.kind == kind && entry.name == name))
    }

    /// One item by id.
    pub async fn get(&self, id: &str) -> Result<TrashEntry> {
        let slot = self.slot(id)?;
        blocking(move || {
            read_entry(&slot)
                .ok_or_else(|| Error::NotFound(format!("trash item {}", slot.display())))
        })
        .await
    }

    /// Put an item back where it was. If that place is taken, it goes next to it under a free
    /// name (`World (2)`, `shot (2).png`). Returns where it was put.
    pub async fn restore(&self, id: &str) -> Result<PathBuf> {
        let slot = self.slot(id)?;
        let root = self.dir.parent().map(Path::to_path_buf);
        blocking(move || {
            let entry = read_entry(&slot)
                .ok_or_else(|| Error::NotFound(format!("trash item {}", slot.display())))?;
            let item = slot.join(ITEM_DIR).join(&entry.file_name);
            let is_dir = item.is_dir();
            let inside = |rel: &Path| {
                rel.components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
            };
            let home = match (&root, &entry.relative_path) {
                (Some(root), Some(rel)) if inside(rel) => root.join(rel),
                _ => entry.original_path.clone(),
            };
            let target = free_path(&home, is_dir);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            move_path(&item, &target)?;
            let _ = std::fs::remove_dir_all(&slot);
            Ok(target)
        })
        .await
    }

    /// Put an item back at exactly `target` (an instance folder under the id the store picked).
    /// An empty folder there is replaced; anything else there is an error. Returns the entry.
    pub async fn restore_to(&self, id: &str, target: &Path) -> Result<TrashEntry> {
        let slot = self.slot(id)?;
        let target = target.to_path_buf();
        blocking(move || {
            let entry = read_entry(&slot)
                .ok_or_else(|| Error::NotFound(format!("trash item {}", slot.display())))?;
            let item = slot.join(ITEM_DIR).join(&entry.file_name);
            let empty_dir = std::fs::read_dir(&target).is_ok_and(|mut e| e.next().is_none());
            if empty_dir {
                std::fs::remove_dir(&target).map_err(|e| Error::io(&target, e))?;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            move_path(&item, &target)?;
            let _ = std::fs::remove_dir_all(&slot);
            Ok(entry)
        })
        .await
    }

    /// Delete an item for good.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let slot = self.slot(id)?;
        blocking(move || {
            if !slot.join(META_FILE).is_file() {
                return Err(Error::NotFound(format!("trash item {}", slot.display())));
            }
            remove_dir_all_robust(&slot).map_err(|e| Error::io(&slot, e))
        })
        .await
    }

    /// Delete items older than `max_age` (and old leftovers of interrupted moves). Returns how
    /// many items went. See [`Self::purge`] for the size cap.
    pub async fn purge_older_than(&self, max_age: Duration) -> Result<usize> {
        let limits = TrashLimits {
            max_age,
            ..TrashLimits::default()
        };
        Ok(self.purge(limits).await?.removed)
    }

    /// Apply `limits`: expired items go, then the oldest while the trash is over its size cap
    /// (never the newest item), and old leftovers of interrupted moves.
    pub async fn purge(&self, limits: TrashLimits) -> Result<PurgeReport> {
        let trash = self.clone();
        let now = jiff::Timestamp::now().as_millisecond();
        blocking(move || trash.purge_blocking(limits, now)).await
    }

    /// [`Self::purge`] for code that already runs on a blocking thread, at the time `now_ms`.
    pub fn purge_blocking(&self, limits: TrashLimits, now_ms: i64) -> Result<PurgeReport> {
        let millis = |d: Duration| i64::try_from(d.as_millis()).unwrap_or(i64::MAX);
        let expired_before = now_ms.saturating_sub(millis(limits.max_age));
        let incomplete_before = now_ms.saturating_sub(millis(limits.incomplete_age));
        let mut report = PurgeReport::default();
        let mut kept: Vec<(PathBuf, TrashEntry)> = Vec::new();
        for slot in read_dir_or_empty(&self.dir)? {
            let path = slot.path();
            if !path.is_dir() {
                continue;
            }
            match read_entry(&path) {
                Some(entry) if entry.deleted_at_ms < expired_before => {
                    report.remove(&path, entry.size);
                }
                Some(entry) => kept.push((path, entry)),
                // No description or no item: an interrupted move. Only removed once it is old,
                // so a move in progress is never touched. A description that exists but cannot
                // be read right now (a cloud-only file while offline, a file another program
                // holds, a newer format) keeps the slot: it may be a complete item.
                None => {
                    let old = slot
                        .metadata()
                        .ok()
                        .and_then(|m| super::fsops::modified_ms(&m))
                        .is_some_and(|ms| ms < incomplete_before);
                    if old && is_interrupted(&path) {
                        let size = dir_size(&path);
                        report.remove(&path, size);
                    }
                }
            }
        }
        if let Some(cap) = limits.max_bytes {
            // Oldest first; the newest item is never removed for size.
            kept.sort_by(|(_, a), (_, b)| {
                a.deleted_at_ms
                    .cmp(&b.deleted_at_ms)
                    .then_with(|| a.id.cmp(&b.id))
            });
            let mut total: u64 = kept.iter().map(|(_, entry)| entry.size).sum();
            let removable = kept.len().saturating_sub(1);
            for (path, entry) in kept.iter().take(removable) {
                if total <= cap {
                    break;
                }
                if report.remove(path, entry.size) {
                    total = total.saturating_sub(entry.size);
                }
            }
        }
        if report.removed > 0 {
            tracing::info!(
                removed = report.removed,
                freed = report.freed_bytes,
                "purged the launcher trash"
            );
        }
        Ok(report)
    }

    /// Items and bytes in the trash (complete items only).
    pub async fn usage(&self) -> Result<TrashUsage> {
        let dir = self.dir.clone();
        blocking(move || {
            let mut usage = TrashUsage::default();
            for slot in read_dir_or_empty(&dir)? {
                if let Some(entry) = read_entry(&slot.path()) {
                    usage.items += 1;
                    usage.bytes = usage.bytes.saturating_add(entry.size);
                    usage.oldest_deleted_ms = Some(
                        usage
                            .oldest_deleted_ms
                            .map_or(entry.deleted_at_ms, |ms| ms.min(entry.deleted_at_ms)),
                    );
                }
            }
            Ok(usage)
        })
        .await
    }

    /// Delete everything in the trash for good. Items that cannot be removed (in use) stay.
    pub async fn empty(&self) -> Result<PurgeReport> {
        let dir = self.dir.clone();
        blocking(move || {
            let mut report = PurgeReport::default();
            for slot in read_dir_or_empty(&dir)? {
                let path = slot.path();
                if !path.is_dir() {
                    continue;
                }
                let size = read_entry(&path).map_or_else(|| dir_size(&path), |entry| entry.size);
                report.remove(&path, size);
            }
            Ok(report)
        })
        .await
    }

    fn slot(&self, id: &str) -> Result<PathBuf> {
        Ok(self.dir.join(safe_name(id)?))
    }
}

impl PurgeReport {
    /// Removes one slot; counts it when it went.
    fn remove(&mut self, slot: &Path, size: u64) -> bool {
        match remove_dir_all_robust(slot) {
            Ok(()) => {
                self.removed += 1;
                self.freed_bytes = self.freed_bytes.saturating_add(size);
                true
            }
            Err(error) => {
                tracing::debug!(slot = %slot.display(), %error, "a trash item could not be removed yet");
                false
            }
        }
    }
}

/// `remove_dir_all` that also deletes read-only files (Windows refuses those otherwise).
fn remove_dir_all_robust(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(first) => {
            if cfg!(windows) {
                for entry in walkdir::WalkDir::new(dir)
                    .follow_links(false)
                    .into_iter()
                    .filter_map(std::result::Result::ok)
                {
                    if let Ok(meta) = entry.metadata() {
                        let mut permissions = meta.permissions();
                        if permissions.readonly() {
                            #[allow(clippy::permissions_set_readonly_false)]
                            permissions.set_readonly(false);
                            let _ = std::fs::set_permissions(entry.path(), permissions);
                        }
                    }
                }
            }
            std::fs::remove_dir_all(dir).map_err(|_| first)
        }
    }
}

/// A slot an interrupted move left: no description, or a readable description whose item is
/// missing. A description that cannot be read or understood does not count (written atomically,
/// it is never half-written by a move of ours).
fn is_interrupted(slot: &Path) -> bool {
    let meta = slot.join(META_FILE);
    let bytes = match std::fs::read(&meta) {
        Ok(bytes) => bytes,
        Err(e) => return e.kind() == std::io::ErrorKind::NotFound,
    };
    serde_json::from_slice::<TrashEntry>(&bytes).is_ok_and(|entry| {
        std::fs::symlink_metadata(slot.join(ITEM_DIR).join(&entry.file_name)).is_err()
    })
}

/// The description of a trash slot, if it is complete (description and item both present).
fn read_entry(slot: &Path) -> Option<TrashEntry> {
    let bytes = std::fs::read(slot.join(META_FILE)).ok()?;
    let entry: TrashEntry = serde_json::from_slice(&bytes).ok()?;
    let item = slot.join(ITEM_DIR).join(&entry.file_name);
    std::fs::symlink_metadata(&item).ok()?;
    (slot.file_name()? == entry.id.as_str()).then_some(entry)
}

/// `path` if free, else `name (2)`, `name (3)`, … next to it (files keep their extension).
pub(crate) fn free_path(path: &Path, is_dir: bool) -> PathBuf {
    if std::fs::symlink_metadata(path).is_err() {
        return path.to_path_buf();
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, ext) = match file_name.rsplit_once('.') {
        Some((stem, ext)) if !is_dir && !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
        _ => (file_name.clone(), String::new()),
    };
    (2..10_000)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|candidate| std::fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| path.with_file_name(format!("{stem} ({}){ext}", uuid::Uuid::new_v4())))
}

/// Move a file or folder. A rename when possible; across drives (a relocated data folder or an
/// instance linked from elsewhere) the item is copied, every copied file is flushed to disk, and
/// only then the original is removed.
pub(crate) fn move_path(from: &Path, to: &Path) -> Result<()> {
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(Error::AlreadyExists(to.to_path_buf()));
    }
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if is_cross_device(&e) => move_by_copy(from, to),
        Err(e) => Err(Error::io(from, e)),
    }
}

/// The cross-device half of [`move_path`]: copy (flushed), then remove the original. Once the copy
/// is complete it is the item: when removing the original stops half-way (a file open elsewhere),
/// what is left of the original stays where it was and the move still counts as done, so no
/// caller discards the only complete copy.
fn move_by_copy(from: &Path, to: &Path) -> Result<()> {
    if let Err(error) = copy_tree(from, to) {
        let _ = remove_any(to);
        return Err(error);
    }
    if let Err(error) = remove_any(from) {
        tracing::warn!(from = %from.display(), %error, "moved by copying; part of the original could not be removed");
    }
    Ok(())
}

fn is_cross_device(error: &std::io::Error) -> bool {
    // ERROR_NOT_SAME_DEVICE on Windows, EXDEV on Unix.
    let code = if cfg!(windows) { 17 } else { 18 };
    error.raw_os_error() == Some(code)
}

fn remove_any(path: &Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(path)?.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Copies a file and flushes the copy to disk (the original is removed right after).
fn copy_durable(from: &Path, to: &Path) -> Result<()> {
    copy_file_durable(from, to).map(|_| ())
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    if !from.is_dir() {
        return copy_durable(from, to);
    }
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.map_err(|e| {
            let path = e.path().map(Path::to_path_buf).unwrap_or_default();
            Error::io(path, e.into())
        })?;
        let rel = entry.path().strip_prefix(from).unwrap_or(entry.path());
        let target = to.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| Error::io(&target, e))?;
        } else if entry.file_type().is_file() {
            copy_durable(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn put_list_restore_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let trash = LauncherTrash::new(dir.path().join("trash"));
        let saves = dir.path().join("saves");
        let world = saves.join("World 1");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::write(world.join("level.dat"), b"level").unwrap();
        std::fs::write(world.join("region").join("r.0.0.mca"), vec![1u8; 300]).unwrap();
        let shot = dir.path().join("shots").join("2026-10-15_22.44.12.png");
        std::fs::create_dir_all(shot.parent().unwrap()).unwrap();
        std::fs::write(&shot, b"png").unwrap();

        let trashed = trash
            .put(&world, TrashKind::World, "inst", "Survival Fun")
            .await
            .unwrap();
        assert!(!world.exists());
        assert_eq!(trashed.size, 305);
        assert_eq!(trashed.file_name, "World 1");
        let shot_entry = trash
            .put(
                &shot,
                TrashKind::Screenshot,
                "other",
                "2026-10-15_22.44.12.png",
            )
            .await
            .unwrap();

        let all = trash.list(None).await.unwrap();
        assert_eq!(all.len(), 2);
        let mine = trash.list(Some("inst")).await.unwrap();
        assert_eq!(mine, vec![trashed.clone()]);
        assert_eq!(trash.get(&trashed.id).await.unwrap(), trashed);

        // Restoring next to a world that took the old name keeps both.
        std::fs::create_dir_all(&world).unwrap();
        let back = trash.restore(&trashed.id).await.unwrap();
        assert_eq!(back, saves.join("World 1 (2)"));
        assert_eq!(std::fs::read(back.join("level.dat")).unwrap(), b"level");
        assert!(trash.list(Some("inst")).await.unwrap().is_empty());

        // A file restores under its own name, or with a numbered stem.
        std::fs::write(&shot, b"new").unwrap();
        let shot_back = trash.restore(&shot_entry.id).await.unwrap();
        assert_eq!(
            shot_back.file_name().unwrap(),
            "2026-10-15_22.44.12 (2).png"
        );

        let again = trash
            .put(&shot_back, TrashKind::Screenshot, "other", "x")
            .await
            .unwrap();
        trash.delete(&again.id).await.unwrap();
        assert!(trash.list(None).await.unwrap().is_empty());
        assert!(trash.delete(&again.id).await.is_err());
        assert!(trash.restore("../escape").await.is_err());
        assert!(
            trash
                .put(&dir.path().join("missing"), TrashKind::World, "i", "m")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn purge_removes_only_old_items() {
        let dir = tempfile::tempdir().unwrap();
        let trash = LauncherTrash::new(dir.path().join("trash"));
        let file = dir.path().join("a.png");
        std::fs::write(&file, b"a").unwrap();
        let entry = trash
            .put(&file, TrashKind::Screenshot, "i", "a.png")
            .await
            .unwrap();
        assert_eq!(trash.purge_older_than(TRASH_RETENTION).await.unwrap(), 0);
        assert_eq!(trash.list(None).await.unwrap().len(), 1);

        // Age the item by rewriting its description.
        let mut old = entry.clone();
        old.deleted_at_ms -= 31 * 24 * 60 * 60 * 1000;
        let meta = trash.dir().join(&entry.id).join(META_FILE);
        std::fs::write(&meta, serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(trash.purge_older_than(TRASH_RETENTION).await.unwrap(), 1);
        assert!(trash.list(None).await.unwrap().is_empty());
        assert!(!trash.dir().join(&entry.id).exists());
    }

    /// Puts a file of `size` bytes into the trash, deleted `days_ago` days before now.
    async fn trashed(trash: &LauncherTrash, dir: &Path, name: &str, size: usize, days_ago: i64) {
        let file = dir.join(name);
        std::fs::write(&file, vec![7u8; size]).unwrap();
        let mut entry = trash
            .put(&file, TrashKind::VersionSet, "i", name)
            .await
            .unwrap();
        entry.deleted_at_ms -= days_ago * 24 * 60 * 60 * 1000;
        let meta = trash.dir().join(&entry.id).join(META_FILE);
        std::fs::write(&meta, serde_json::to_vec(&entry).unwrap()).unwrap();
    }

    #[tokio::test]
    async fn latest_finds_the_newest_item_of_a_version() {
        let dir = tempfile::tempdir().unwrap();
        let trash = LauncherTrash::new(dir.path().join("trash"));
        let put = |name: &str, kind: TrashKind, instance: &str, folder: &str| {
            let trash = trash.clone();
            let path = dir.path().join(folder);
            let (name, instance) = (name.to_owned(), instance.to_owned());
            async move {
                std::fs::create_dir_all(path.join("mods")).unwrap();
                trash.put(&path, kind, &instance, &name).await.unwrap()
            }
        };
        put("1.20.1", TrashKind::VersionSet, "i", "old-a").await;
        let newer = {
            let mut entry = put("1.20.1", TrashKind::VersionSet, "i", "old-b").await;
            entry.deleted_at_ms += 1000;
            let meta = trash.dir().join(&entry.id).join(META_FILE);
            std::fs::write(&meta, serde_json::to_vec(&entry).unwrap()).unwrap();
            entry
        };
        put("1.20.1", TrashKind::ModSet, "i", "mods-aside").await;
        put("1.20.1", TrashKind::VersionSet, "other", "theirs").await;
        put("1.21.1", TrashKind::VersionSet, "i", "newer-version").await;

        let found = trash
            .latest("i", TrashKind::VersionSet, "1.20.1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, newer.id);
        assert!(trash.item_path(&found).join("mods").is_dir());
        assert!(
            trash
                .latest("i", TrashKind::VersionSet, "1.19.4")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            trash
                .latest("nobody", TrashKind::VersionSet, "1.20.1")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn size_cap_removes_the_oldest_but_never_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let trash = LauncherTrash::new(dir.path().join("trash"));
        trashed(&trash, dir.path(), "a", 400, 20).await;
        trashed(&trash, dir.path(), "b", 300, 10).await;
        trashed(&trash, dir.path(), "c", 200, 2).await;
        let usage = trash.usage().await.unwrap();
        assert_eq!((usage.items, usage.bytes), (3, 900));

        // Under the cap: nothing goes.
        let roomy = TrashLimits {
            max_bytes: Some(900),
            ..TrashLimits::default()
        };
        assert_eq!(trash.purge(roomy).await.unwrap(), PurgeReport::default());

        // Over the cap: the oldest goes first, until it fits.
        let tight = TrashLimits {
            max_bytes: Some(600),
            ..TrashLimits::default()
        };
        let report = trash.purge(tight).await.unwrap();
        assert_eq!(
            report,
            PurgeReport {
                removed: 1,
                freed_bytes: 400
            }
        );
        let names: Vec<String> = trash
            .list(None)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["c", "b"]);

        // A cap smaller than the newest item alone still keeps the newest item.
        let tiny = TrashLimits {
            max_bytes: Some(10),
            ..TrashLimits::default()
        };
        assert_eq!(trash.purge(tiny).await.unwrap().removed, 1);
        let left = trash.list(None).await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name, "c");
    }

    #[tokio::test]
    async fn expiry_cap_and_interrupted_moves_together() {
        let dir = tempfile::tempdir().unwrap();
        let trash = LauncherTrash::new(dir.path().join("trash"));
        trashed(&trash, dir.path(), "old", 100, 31).await;
        trashed(&trash, dir.path(), "recent", 100, 1).await;
        // An interrupted move: a slot with a description but no item.
        let broken = trash.dir().join("20260101-000000-deadbeef");
        std::fs::create_dir_all(broken.join(ITEM_DIR)).unwrap();
        let described = TrashEntry {
            id: "20260101-000000-deadbeef".into(),
            kind: TrashKind::World,
            instance_id: "i".into(),
            name: "World".into(),
            file_name: "World".into(),
            original_path: dir.path().join("World"),
            relative_path: None,
            deleted_at_ms: 0,
            size: 0,
        };
        std::fs::write(
            broken.join(META_FILE),
            serde_json::to_vec(&described).unwrap(),
        )
        .unwrap();

        // Fresh interrupted moves are left alone (a move may be running right now).
        let now = jiff::Timestamp::now().as_millisecond();
        let report = trash.purge_blocking(TrashLimits::default(), now).unwrap();
        assert_eq!(report.removed, 1, "only the expired item");
        assert!(broken.exists());

        // A day later the leftover goes too.
        let later = now + 25 * 60 * 60 * 1000;
        trash.purge_blocking(TrashLimits::default(), later).unwrap();
        assert!(!broken.exists());
        let left: Vec<String> = trash
            .list(None)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(left, ["recent"]);
    }

    /// A description that cannot be read or understood right now (a cloud-only file while
    /// offline, a file another program holds, a newer format) is not an interrupted move: the
    /// item stays until the player deletes it (it is not listed, so no 30-day count applies).
    #[tokio::test]
    async fn an_unreadable_description_keeps_the_item() {
        let dir = tempfile::tempdir().unwrap();
        let trash = LauncherTrash::new(dir.path().join("trash"));
        let slot = trash.dir().join("20260101-000000-cafebabe");
        let world = slot.join(ITEM_DIR).join("World");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("level.dat"), b"level").unwrap();
        std::fs::write(
            slot.join(META_FILE),
            br#"{"id":"20260101-000000-cafebabe","kind":"somethingNew","fileName":"World"}"#,
        )
        .unwrap();

        let later = jiff::Timestamp::now().as_millisecond() + 25 * 60 * 60 * 1000;
        let report = trash.purge_blocking(TrashLimits::default(), later).unwrap();
        assert_eq!(report.removed, 0);
        assert!(world.join("level.dat").is_file());
    }

    #[tokio::test]
    async fn empty_removes_everything() {
        let dir = tempfile::tempdir().unwrap();
        let trash = LauncherTrash::new(dir.path().join("trash"));
        trashed(&trash, dir.path(), "a", 10, 0).await;
        trashed(&trash, dir.path(), "b", 20, 0).await;
        let report = trash.empty().await.unwrap();
        assert_eq!(
            report,
            PurgeReport {
                removed: 2,
                freed_bytes: 30
            }
        );
        assert_eq!(trash.usage().await.unwrap(), TrashUsage::default());
        // An empty or missing trash is fine.
        assert_eq!(trash.empty().await.unwrap(), PurgeReport::default());
        let missing = LauncherTrash::new(dir.path().join("nowhere"));
        assert_eq!(missing.usage().await.unwrap(), TrashUsage::default());
    }

    #[tokio::test]
    async fn restores_into_a_moved_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("old-root");
        let world = root.join("instances").join("i").join("saves").join("W");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("level.dat"), b"x").unwrap();
        let entry = LauncherTrash::new(root.join("trash"))
            .put(&world, TrashKind::World, "i", "W")
            .await
            .unwrap();
        assert_eq!(
            entry.relative_path.as_deref(),
            Some(
                Path::new("instances")
                    .join("i")
                    .join("saves")
                    .join("W")
                    .as_path()
            )
        );

        // The player moved the whole data folder elsewhere.
        let moved = dir.path().join("new-root");
        std::fs::rename(&root, &moved).unwrap();
        let back = LauncherTrash::new(moved.join("trash"))
            .restore(&entry.id)
            .await
            .unwrap();
        assert_eq!(
            back,
            moved.join("instances").join("i").join("saves").join("W")
        );
        assert!(back.join("level.dat").is_file());
        assert!(!root.exists());
    }

    /// Across drives an item is copied, then the original is removed. When the removal stops
    /// half-way (another program holds a file), the complete copy must stay: before, the error
    /// made `put` delete the trash slot while part of the original was already gone.
    #[cfg(windows)]
    #[test]
    fn a_copied_item_survives_an_original_that_cannot_be_removed() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let world = dir.path().join("World");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::write(world.join("level.dat"), b"level").unwrap();
        std::fs::write(world.join("region").join("r.0.0.mca"), vec![3u8; 64]).unwrap();
        // Held by another program that lets others read it but not delete it.
        const FILE_SHARE_READ: u32 = 1;
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(world.join("region").join("r.0.0.mca"))
            .unwrap();

        let copy = dir.path().join("item");
        move_by_copy(&world, &copy).unwrap();
        assert_eq!(std::fs::read(copy.join("level.dat")).unwrap(), b"level");
        assert_eq!(
            std::fs::read(copy.join("region").join("r.0.0.mca")).unwrap(),
            vec![3u8; 64]
        );
        drop(held);
    }

    #[test]
    fn free_names() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("shot.png");
        assert_eq!(free_path(&file, false), file);
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(free_path(&file, false), dir.path().join("shot (2).png"));
        std::fs::write(dir.path().join("shot (2).png"), b"x").unwrap();
        assert_eq!(free_path(&file, false), dir.path().join("shot (3).png"));
        let folder = dir.path().join("My.World");
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(free_path(&folder, true), dir.path().join("My.World (2)"));
        assert!(matches!(
            move_path(&file, &dir.path().join("shot (2).png")),
            Err(Error::AlreadyExists(_))
        ));
    }
}
