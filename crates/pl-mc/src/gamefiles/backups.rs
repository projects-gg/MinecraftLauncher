//! World backups in `<game_dir>/backups/` (where the game's own "Make Backup" and
//! [`super::backup_world`] put them): listing with the archived world's name, and restoring one
//! into `saves/`.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures::StreamExt;
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use super::fsops::{blocking, modified_ms, read_dir_or_empty};
use super::trash::{LauncherTrash, TrashEntry, TrashKind, free_path};
use super::worlds::{
    BackupProgress, BackupProgressFn, BackupReason, level_name, level_name_from_bytes,
};
use crate::error::{Error, Result};
use crate::modpack::safe_path::{validate_file_name, validate_relative_path};

/// Archived `level.dat` files larger than this are not read for the world name.
const MAX_LEVEL_DAT: u64 = 64 * 1024 * 1024;

/// One backup archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldBackupInfo {
    pub file_name: String,
    pub path: PathBuf,
    pub size: u64,
    /// When the backup was made (the archive's modification time).
    pub created_ms: Option<i64>,
    /// Folder name the world had (the archive's top-level folder, else taken from the file name).
    pub world_folder: Option<String>,
    /// The world's own name from the archived `level.dat`.
    pub world_name: Option<String>,
    /// False when the archive does not hold a world (no `level.dat`) or cannot be read.
    pub valid: bool,
    /// Why the launcher made it by itself (the archive's comment); none for the player's own
    /// backups and the game's.
    pub reason: Option<BackupReason>,
}

/// Where the world sits inside an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Layout {
    /// Entry name prefix of the world's files (`"World/"`, or `""` for a flat archive).
    prefix: String,
    /// The world's folder name, when the archive has one top-level folder.
    root: Option<String>,
}

/// Find the world in a list of entry names: `level.dat` at the top, or one folder holding it.
fn find_layout(names: &[String]) -> Option<Layout> {
    if names.iter().any(|n| n == "level.dat") {
        return Some(Layout {
            prefix: String::new(),
            root: None,
        });
    }
    let roots: BTreeSet<&str> = names
        .iter()
        .filter_map(|n| n.split('/').next())
        .filter(|r| !r.is_empty())
        .collect();
    roots
        .into_iter()
        .find(|root| names.iter().any(|n| *n == format!("{root}/level.dat")))
        .map(|root| Layout {
            prefix: format!("{root}/"),
            root: Some(root.to_owned()),
        })
}

fn entry_names<R: Read + std::io::Seek>(archive: &ZipArchive<R>) -> Vec<String> {
    archive
        .file_names()
        .filter_map(|name| name.ok())
        .map(|name| name.replace('\\', "/"))
        .collect()
}

/// `2026-10-15_22-44-12_My World.zip` → `My World` (the game's and our backup names).
fn folder_from_file_name(file_name: &str) -> Option<String> {
    let stem = file_name
        .strip_suffix(".zip")
        .or_else(|| file_name.strip_suffix(".ZIP"))
        .unwrap_or(file_name);
    let bytes = stem.as_bytes();
    let stamped = bytes.len() >= 20
        && bytes[19] == b'_'
        && bytes[..19].iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            10 => *b == b'_',
            13 | 16 => *b == b'-',
            _ => b.is_ascii_digit(),
        });
    let name = if stamped { &stem[20..] } else { stem };
    validate_file_name(name).ok().map(|()| name.to_owned())
}

fn open_archive(path: &Path) -> Result<ZipArchive<BufReader<File>>> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    ZipArchive::new(BufReader::new(file)).map_err(|e| Error::archive(path, e))
}

fn read_backup_info(path: &Path, file_name: String, meta: &std::fs::Metadata) -> WorldBackupInfo {
    let mut info = WorldBackupInfo {
        world_folder: folder_from_file_name(&file_name),
        file_name,
        path: path.to_path_buf(),
        size: meta.len(),
        created_ms: modified_ms(meta),
        world_name: None,
        valid: false,
        reason: None,
    };
    let Ok(mut archive) = open_archive(path) else {
        return info;
    };
    info.reason = BackupReason::from_comment(archive.comment());
    let names = entry_names(&archive);
    let Some(layout) = find_layout(&names) else {
        return info;
    };
    info.valid = true;
    if let Some(root) = layout.root.filter(|r| validate_file_name(r).is_ok()) {
        info.world_folder = Some(root);
    }
    let level = format!("{}level.dat", layout.prefix);
    if let Ok(entry) = archive.by_name(&level)
        && entry.size() <= MAX_LEVEL_DAT
    {
        let mut bytes = Vec::new();
        if entry.take(MAX_LEVEL_DAT).read_to_end(&mut bytes).is_ok() {
            info.world_name = level_name_from_bytes(&bytes);
        }
    }
    info
}

/// Backups in `<game_dir>/backups`, newest first.
pub async fn list_world_backups(game_dir: &Path) -> Result<Vec<WorldBackupInfo>> {
    let dir = game_dir.join("backups");
    let files: Vec<(PathBuf, String, std::fs::Metadata)> = blocking(move || {
        Ok(read_dir_or_empty(&dir)?
            .into_iter()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.to_ascii_lowercase().ends_with(".zip") || name.starts_with('.') {
                    return None;
                }
                let meta = entry.metadata().ok().filter(|m| m.is_file())?;
                Some((entry.path(), name, meta))
            })
            .collect())
    })
    .await?;
    let mut backups: Vec<WorldBackupInfo> = futures::stream::iter(files)
        .map(|(path, name, meta)| async move {
            blocking(move || Ok(read_backup_info(&path, name, &meta)))
                .await
                .ok()
        })
        .buffer_unordered(4)
        .filter_map(|info| async move { info })
        .collect()
        .await;
    backups.sort_by(|a, b| {
        b.created_ms
            .cmp(&a.created_ms)
            .then_with(|| b.file_name.cmp(&a.file_name))
    });
    Ok(backups)
}

/// How a restored world is put into `saves/`.
#[derive(Debug, Clone)]
pub enum RestoreMode {
    /// Next to the existing worlds; a taken folder name gets a number (`World (2)`).
    AsNew,
    /// In place of the world with the same folder name, which is moved to `trash` first.
    Replace {
        trash: LauncherTrash,
        instance_id: String,
    },
}

/// Result of [`restore_world_backup`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoredWorld {
    /// Folder in `saves/` the world now lives in.
    pub folder: String,
    pub path: PathBuf,
    pub files: u64,
    pub bytes: u64,
    /// The world it replaced, now in the launcher trash.
    pub replaced: Option<TrashEntry>,
}

/// Unpack a backup into `saves_dir`. The files go to a hidden temporary folder first and are
/// moved into place at the end, so a failed or cancelled restore never leaves a half world
/// behind (and in [`RestoreMode::Replace`] the old world is only moved away once the new one
/// is complete). Entries with unsafe paths, links and `session.lock` are skipped.
pub async fn restore_world_backup(
    backup: &Path,
    saves_dir: &Path,
    mode: RestoreMode,
    cancel: CancellationToken,
    progress: Option<BackupProgressFn>,
) -> Result<RestoredWorld> {
    let backup = backup.to_path_buf();
    let saves = saves_dir.to_path_buf();
    blocking(move || restore_blocking(&backup, &saves, mode, &cancel, progress.as_ref())).await
}

fn restore_blocking(
    backup: &Path,
    saves: &Path,
    mode: RestoreMode,
    cancel: &CancellationToken,
    progress: Option<&BackupProgressFn>,
) -> Result<RestoredWorld> {
    let mut archive = open_archive(backup)?;
    let names = entry_names(&archive);
    let layout = find_layout(&names)
        .ok_or_else(|| Error::Invalid(format!("{} holds no world", backup.display())))?;
    let file_name = backup
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = layout
        .root
        .clone()
        .filter(|r| validate_file_name(r).is_ok())
        .or_else(|| folder_from_file_name(&file_name))
        .unwrap_or_else(|| "world".to_owned());

    std::fs::create_dir_all(saves).map_err(|e| Error::io(saves, e))?;
    let temp = saves.join(format!(
        ".restore-{}.part",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    ));
    let extracted = extract_world(
        &mut archive,
        backup,
        &layout.prefix,
        &temp,
        cancel,
        progress,
    );
    let state = match extracted {
        Ok(state) => state,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&temp);
            return Err(error);
        }
    };
    if cancel.is_cancelled() {
        let _ = std::fs::remove_dir_all(&temp);
        return Err(Error::Cancelled);
    }

    let wanted = saves.join(&folder);
    let mut replaced = None;
    let target = match &mode {
        RestoreMode::Replace { trash, instance_id } if wanted.exists() => {
            let name = level_name(&wanted).unwrap_or_else(|| folder.clone());
            match trash.put_blocking(&wanted, TrashKind::World, instance_id, &name) {
                Ok(entry) => replaced = Some(entry),
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&temp);
                    return Err(error);
                }
            }
            wanted
        }
        _ => free_path(&wanted, true),
    };
    if let Err(e) = std::fs::rename(&temp, &target) {
        let _ = std::fs::remove_dir_all(&temp);
        // Put the replaced world back rather than leave the player with neither.
        if let (Some(entry), RestoreMode::Replace { trash, .. }) = (&replaced, &mode) {
            let slot = trash.dir().join(&entry.id);
            let item = slot.join("item").join(&entry.file_name);
            if std::fs::rename(&item, &target).is_ok() {
                let _ = std::fs::remove_dir_all(&slot);
            }
        }
        return Err(Error::io(&target, e));
    }
    if let Some(report) = progress {
        report(state);
    }
    Ok(RestoredWorld {
        folder: target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or(folder),
        path: target,
        files: state.files_done,
        bytes: state.bytes_done,
        replaced,
    })
}

fn extract_world<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    archive_path: &Path,
    prefix: &str,
    dest: &Path,
    cancel: &CancellationToken,
    progress: Option<&BackupProgressFn>,
) -> Result<BackupProgress> {
    // What will be extracted (sizes come from the central directory, nothing is inflated).
    let mut plan: Vec<(usize, PathBuf, bool)> = Vec::new();
    let mut state = BackupProgress::default();
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index_raw(index) else {
            continue;
        };
        let Ok(name) = entry.name().map(|n| n.replace('\\', "/")) else {
            continue;
        };
        let Some(rel) = name.strip_prefix(prefix) else {
            continue;
        };
        let is_dir = entry.is_dir() || rel.ends_with('/');
        let rel = rel.trim_end_matches('/');
        if rel.is_empty() || rel == "session.lock" || entry.is_symlink() {
            continue;
        }
        let Ok(rel_path) = validate_relative_path(rel) else {
            continue;
        };
        if !is_dir {
            state.files_total += 1;
            state.bytes_total += entry.size();
        }
        plan.push((index, rel_path, is_dir));
    }

    std::fs::create_dir_all(dest).map_err(|e| Error::io(dest, e))?;
    let mut last_report = Instant::now();
    for (index, rel_path, is_dir) in plan {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let target = dest.join(rel_path);
        if is_dir {
            std::fs::create_dir_all(&target).map_err(|e| Error::io(&target, e))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut entry = archive
            .by_index(index)
            .map_err(|e| Error::archive(archive_path, e))?;
        let file = File::create(&target).map_err(|e| Error::io(&target, e))?;
        let mut out = BufWriter::new(file);
        let copied = std::io::copy(&mut entry, &mut out).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                Error::archive(archive_path, e)
            } else {
                Error::io(&target, e)
            }
        })?;
        out.flush().map_err(|e| Error::io(&target, e))?;
        state.files_done += 1;
        state.bytes_done += copied;
        if let Some(report) = progress
            && last_report.elapsed() >= Duration::from_millis(100)
        {
            report(state);
            last_report = Instant::now();
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use fastnbt::Value;

    use super::*;
    use crate::gamefiles::{backup_world, default_backup_path};
    use crate::mods::test_support::zip_bytes;

    fn level_dat(name: &str) -> Vec<u8> {
        let mut data = HashMap::new();
        data.insert("LevelName".to_owned(), Value::String(name.into()));
        data.insert("GameType".to_owned(), Value::Int(0));
        let mut root = HashMap::new();
        root.insert("Data".to_owned(), Value::Compound(data));
        let nbt = fastnbt::to_bytes(&Value::Compound(root)).unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&nbt).unwrap();
        gz.finish().unwrap()
    }

    fn make_world(saves: &Path, folder: &str, name: &str) -> PathBuf {
        let world = saves.join(folder);
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::write(world.join("level.dat"), level_dat(name)).unwrap();
        std::fs::write(world.join("region").join("r.0.0.mca"), vec![3u8; 2048]).unwrap();
        world
    }

    #[test]
    fn layouts_and_names() {
        let names = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            find_layout(&names(&["World/", "World/level.dat", "World/region/r.mca"])),
            Some(Layout {
                prefix: "World/".into(),
                root: Some("World".into())
            })
        );
        assert_eq!(
            find_layout(&names(&["level.dat", "region/r.mca"]))
                .unwrap()
                .prefix,
            ""
        );
        assert_eq!(
            find_layout(&names(&["__MACOSX/x", "Mine/level.dat"]))
                .unwrap()
                .root
                .as_deref(),
            Some("Mine")
        );
        assert_eq!(find_layout(&names(&["mods/a.jar", "readme.txt"])), None);
        assert_eq!(
            folder_from_file_name("2026-10-15_22-44-12_My World.zip").as_deref(),
            Some("My World")
        );
        assert_eq!(folder_from_file_name("Kale.zip").as_deref(), Some("Kale"));
        assert_eq!(folder_from_file_name("2026-10-15_22-44-12_.zip"), None);
    }

    #[tokio::test]
    async fn list_and_restore_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("minecraft");
        let saves = game.join("saves");
        let world = make_world(&saves, "World 1", "Survival Fun");
        std::fs::write(world.join("session.lock"), b"lock").unwrap();

        let dest = default_backup_path(&game, "World 1");
        backup_world(&world, &dest, CancellationToken::new(), None)
            .await
            .unwrap();
        // A flat archive made by hand and a zip that is not a world.
        let backups = game.join("backups");
        std::fs::write(
            backups.join("Kale.zip"),
            zip_bytes(&[("level.dat", &level_dat("Kale")), ("region/r.mca", b"r")]),
        )
        .unwrap();
        std::fs::write(backups.join("notes.zip"), zip_bytes(&[("a.txt", b"a")])).unwrap();
        std::fs::write(backups.join("broken.zip"), b"not a zip").unwrap();

        let list = list_world_backups(&game).await.unwrap();
        assert_eq!(list.len(), 4);
        let ours = list
            .iter()
            .find(|b| b.world_folder.as_deref() == Some("World 1"))
            .unwrap();
        assert!(ours.valid);
        assert_eq!(ours.world_name.as_deref(), Some("Survival Fun"));
        let kale = list.iter().find(|b| b.file_name == "Kale.zip").unwrap();
        assert!(kale.valid);
        assert_eq!(kale.world_folder.as_deref(), Some("Kale"));
        assert!(
            !list
                .iter()
                .find(|b| b.file_name == "notes.zip")
                .unwrap()
                .valid
        );
        assert!(
            !list
                .iter()
                .find(|b| b.file_name == "broken.zip")
                .unwrap()
                .valid
        );

        // As a new world next to the existing one.
        let reports = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let sink = Arc::clone(&reports);
        let progress: BackupProgressFn = Arc::new(move |p| sink.lock().push(p));
        let restored = restore_world_backup(
            &ours.path,
            &saves,
            RestoreMode::AsNew,
            CancellationToken::new(),
            Some(progress),
        )
        .await
        .unwrap();
        assert_eq!(restored.folder, "World 1 (2)");
        assert_eq!(restored.files, 2);
        assert!(restored.replaced.is_none());
        assert_eq!(
            std::fs::read(restored.path.join("region").join("r.0.0.mca"))
                .unwrap()
                .len(),
            2048
        );
        assert!(!restored.path.join("session.lock").exists());
        let last = *reports.lock().last().unwrap();
        assert_eq!(last.files_done, last.files_total);

        // In place of the original, which goes to the trash.
        std::fs::write(world.join("level.dat"), level_dat("Changed")).unwrap();
        let trash = LauncherTrash::new(dir.path().join("trash"));
        let replaced = restore_world_backup(
            &ours.path,
            &saves,
            RestoreMode::Replace {
                trash: trash.clone(),
                instance_id: "inst".into(),
            },
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(replaced.folder, "World 1");
        let old = replaced.replaced.unwrap();
        assert_eq!(old.name, "Changed");
        assert_eq!(trash.list(Some("inst")).await.unwrap().len(), 1);
        assert_eq!(level_name(&world).as_deref(), Some("Survival Fun"));

        // A flat archive takes its folder name from the file name.
        let flat = restore_world_backup(
            &backups.join("Kale.zip"),
            &saves,
            RestoreMode::AsNew,
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(flat.folder, "Kale");

        // Not a world, or cancelled: nothing is left behind.
        assert!(
            restore_world_backup(
                &backups.join("notes.zip"),
                &saves,
                RestoreMode::AsNew,
                CancellationToken::new(),
                None
            )
            .await
            .is_err()
        );
        let token = CancellationToken::new();
        token.cancel();
        let cancelled =
            restore_world_backup(&ours.path, &saves, RestoreMode::AsNew, token, None).await;
        assert!(matches!(cancelled, Err(Error::Cancelled)));
        let leftovers: Vec<String> = std::fs::read_dir(&saves)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    /// The backup taken before a world first opens to friends keeps that in its zip comment; the
    /// player's own backups and other tools' comments carry no reason.
    #[tokio::test]
    async fn a_backup_before_sharing_says_so() {
        use crate::gamefiles::{BackupReason, backup_world_marked};

        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("minecraft");
        let world = make_world(&game.join("saves"), "Kale", "Kale");
        let backups = game.join("backups");
        let marked = backups.join("2026-10-24_21-00-00_Kale.zip");
        backup_world_marked(
            &world,
            &marked,
            Some(BackupReason::BeforeSharing),
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap();
        backup_world(
            &world,
            &backups.join("2026-10-24_21-00-01_Kale.zip"),
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap();
        let mut other = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        other.set_comment("made by another tool").unwrap();
        other
            .start_file("Kale/level.dat", zip::write::SimpleFileOptions::default())
            .unwrap();
        other.write_all(&level_dat("Kale")).unwrap();
        std::fs::write(
            backups.join("other.zip"),
            other.finish().unwrap().into_inner(),
        )
        .unwrap();

        let list = list_world_backups(&game).await.unwrap();
        let reason_of = |name: &str| list.iter().find(|b| b.file_name == name).unwrap().reason;
        assert_eq!(
            reason_of("2026-10-24_21-00-00_Kale.zip"),
            Some(BackupReason::BeforeSharing)
        );
        assert_eq!(reason_of("2026-10-24_21-00-01_Kale.zip"), None);
        assert_eq!(reason_of("other.zip"), None);
        assert!(list.iter().all(|b| b.valid));

        // It restores like any other backup.
        let restored = restore_world_backup(
            &marked,
            &game.join("saves"),
            RestoreMode::AsNew,
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(restored.folder, "Kale (2)");
    }

    #[test]
    fn only_the_launchers_own_comment_names_a_reason() {
        use crate::gamefiles::BackupReason;

        assert_eq!(
            BackupReason::from_comment(b"projects-launcher-backup:before-sharing"),
            Some(BackupReason::BeforeSharing)
        );
        assert_eq!(
            BackupReason::from_comment(BackupReason::BeforeSharing.comment().as_bytes()),
            Some(BackupReason::BeforeSharing)
        );
        for comment in [
            &b""[..],
            &b"projects-launcher-backup:"[..],
            &b"projects-launcher-backup:something-new"[..],
            &b"before-sharing"[..],
            &b"\xff\xfe"[..],
        ] {
            assert_eq!(BackupReason::from_comment(comment), None, "{comment:?}");
        }
        // The page reads it in camelCase like the other enums.
        assert_eq!(
            serde_json::to_value(BackupReason::BeforeSharing).unwrap(),
            serde_json::json!("beforeSharing")
        );
    }

    #[tokio::test]
    async fn unsafe_entries_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("evil.zip");
        std::fs::write(
            &zip,
            zip_bytes(&[
                ("W/level.dat", &level_dat("W")),
                ("W/../../escape.txt", b"x"),
                ("W/ok.txt", b"ok"),
            ]),
        )
        .unwrap();
        let saves = dir.path().join("saves");
        let restored = restore_world_backup(
            &zip,
            &saves,
            RestoreMode::AsNew,
            CancellationToken::new(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(restored.files, 2);
        assert!(saves.join("W").join("ok.txt").is_file());
        assert!(!dir.path().join("escape.txt").exists());
    }
}
