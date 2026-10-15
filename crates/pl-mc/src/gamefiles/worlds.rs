//! Single-player worlds (`saves/`): `level.dat` summary, backups and deletion.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fastnbt::Value;
use futures::StreamExt;
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;

use super::fsops::{
    RemovalMethod, blocking, dir_size, modified_ms, read_dir_or_empty, read_png_file, remove_path,
    sibling_with_suffix,
};
use crate::error::{Error, Result};

/// `level.dat` files are small; anything bigger than this is not parsed.
const MAX_LEVEL_DAT: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum WorldGameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
    Unknown,
}

impl WorldGameMode {
    fn from_id(id: i64) -> Self {
        match id {
            0 => Self::Survival,
            1 => Self::Creative,
            2 => Self::Adventure,
            3 => Self::Spectator,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldInfo {
    pub folder_name: String,
    pub path: PathBuf,
    /// The world's own name (`LevelName`), else the folder name.
    pub name: String,
    pub last_played_ms: Option<i64>,
    pub game_mode: WorldGameMode,
    pub hardcore: bool,
    pub allow_commands: bool,
    /// Game version that last saved the world (missing before 1.9).
    pub version_name: Option<String>,
    /// `icon.png` (at most 256 KB).
    pub icon_png: Option<Vec<u8>>,
    /// Total size of the world folder in bytes.
    pub size: u64,
    /// False when `level.dat` could not be read; the other fields are then defaults.
    pub readable: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct LevelSummary {
    name: Option<String>,
    last_played: Option<i64>,
    game_type: Option<i64>,
    hardcore: bool,
    allow_commands: bool,
    version_name: Option<String>,
}

fn decompress(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .take(MAX_LEVEL_DAT)
            .read_to_end(&mut out)
            .ok()?;
        Some(out)
    } else {
        Some(bytes.to_vec())
    }
}

fn as_int(value: &Value) -> Option<i64> {
    match value {
        Value::Byte(v) => Some(i64::from(*v)),
        Value::Short(v) => Some(i64::from(*v)),
        Value::Int(v) => Some(i64::from(*v)),
        Value::Long(v) => Some(*v),
        _ => None,
    }
}

fn compound<'a>(map: &'a HashMap<String, Value>, key: &str) -> Option<&'a HashMap<String, Value>> {
    match map.get(key)? {
        Value::Compound(inner) => Some(inner),
        _ => None,
    }
}

fn string(map: &HashMap<String, Value>, key: &str) -> Option<String> {
    match map.get(key)? {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Parse a (gzip) `level.dat` into the fields the world list shows.
fn parse_level_dat(bytes: &[u8]) -> Option<LevelSummary> {
    let raw = decompress(bytes)?;
    let root: HashMap<String, Value> = fastnbt::from_bytes(&raw).ok()?;
    let data = compound(&root, "Data")?;
    let int = |key: &str| data.get(key).and_then(as_int);
    // Newer versions keep hardcore inside `difficulty_settings`.
    let hardcore = int("hardcore")
        .or_else(|| {
            compound(data, "difficulty_settings")?
                .get("hardcore")
                .and_then(as_int)
        })
        .unwrap_or(0)
        != 0;
    Some(LevelSummary {
        name: string(data, "LevelName").filter(|n| !n.trim().is_empty()),
        last_played: int("LastPlayed").filter(|t| *t > 0),
        game_type: int("GameType"),
        hardcore,
        allow_commands: int("allowCommands").unwrap_or(0) != 0,
        version_name: compound(data, "Version").and_then(|v| string(v, "Name")),
    })
}

fn read_level_file(world_dir: &Path) -> Option<LevelSummary> {
    ["level.dat", "level.dat_old"].iter().find_map(|name| {
        let path = world_dir.join(name);
        let len = std::fs::metadata(&path).ok()?.len();
        if len > MAX_LEVEL_DAT {
            return None;
        }
        parse_level_dat(&std::fs::read(&path).ok()?)
    })
}

fn read_world_blocking(world_dir: &Path) -> Result<WorldInfo> {
    let meta = std::fs::metadata(world_dir).map_err(|e| Error::io(world_dir, e))?;
    if !meta.is_dir() {
        return Err(Error::Invalid(format!(
            "{} is not a folder",
            world_dir.display()
        )));
    }
    let folder_name = world_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let summary = read_level_file(world_dir);
    let readable = summary.is_some();
    let summary = summary.unwrap_or_default();
    Ok(WorldInfo {
        name: summary.name.unwrap_or_else(|| folder_name.clone()),
        folder_name,
        path: world_dir.to_path_buf(),
        last_played_ms: summary
            .last_played
            .or_else(|| modified_ms(&std::fs::metadata(world_dir.join("level.dat")).ok()?)),
        game_mode: summary
            .game_type
            .map_or(WorldGameMode::Unknown, WorldGameMode::from_id),
        hardcore: summary.hardcore,
        allow_commands: summary.allow_commands,
        version_name: summary.version_name,
        icon_png: read_png_file(&world_dir.join("icon.png")),
        size: dir_size(world_dir),
        readable,
    })
}

/// Read one world folder.
pub async fn read_world(world_dir: &Path) -> Result<WorldInfo> {
    let dir = world_dir.to_path_buf();
    blocking(move || read_world_blocking(&dir)).await
}

/// Worlds in `<game_dir>/saves` (folders with a `level.dat`), most recently played first.
pub async fn list_worlds(game_dir: &Path) -> Result<Vec<WorldInfo>> {
    let saves = game_dir.join("saves");
    let dirs: Vec<PathBuf> = blocking(move || {
        Ok(read_dir_or_empty(&saves)?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| path.join("level.dat").is_file() || path.join("level.dat_old").is_file())
            .collect())
    })
    .await?;
    let mut worlds: Vec<WorldInfo> = futures::stream::iter(dirs)
        .map(|dir| async move { read_world(&dir).await.ok() })
        .buffer_unordered(4)
        .filter_map(|world| async move { world })
        .collect()
        .await;
    worlds.sort_by(|a, b| {
        b.last_played_ms
            .cmp(&a.last_played_ms)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(worlds)
}

/// Move a world to the trash (permanent deletion only if `allow_permanent`).
pub async fn delete_world(world_dir: &Path, allow_permanent: bool) -> Result<RemovalMethod> {
    remove_path(world_dir, allow_permanent).await
}

/// Progress of a world backup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupProgress {
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

pub type BackupProgressFn = Arc<dyn Fn(BackupProgress) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub path: PathBuf,
    pub files: u64,
    /// Uncompressed size of the backed-up files.
    pub bytes: u64,
}

/// Where the game itself puts backups: `<game_dir>/backups/<yyyy-MM-dd_HH-mm-ss>_<world>.zip`.
pub fn default_backup_path(game_dir: &Path, world_folder: &str) -> PathBuf {
    let stamp = jiff::Zoned::now().strftime("%Y-%m-%d_%H-%M-%S").to_string();
    game_dir
        .join("backups")
        .join(format!("{stamp}_{world_folder}.zip"))
}

fn zip_time(meta: &std::fs::Metadata) -> Option<zip::DateTime> {
    let modified = meta.modified().ok()?;
    let local = jiff::Timestamp::try_from(modified)
        .ok()?
        .to_zoned(jiff::tz::TimeZone::system());
    zip::DateTime::from_date_and_time(
        u16::try_from(local.year()).ok()?,
        u8::try_from(local.month()).ok()?,
        u8::try_from(local.day()).ok()?,
        u8::try_from(local.hour()).ok()?,
        u8::try_from(local.minute()).ok()?,
        u8::try_from(local.second()).ok()?,
    )
    .ok()
}

/// Zip a world folder to `dest_zip` (the archive holds a top-level folder named after the
/// world, like the game's own backups). Written to a temporary file and renamed at the end;
/// cancelling removes the partial file. `session.lock` is skipped (the game keeps it locked).
pub async fn backup_world(
    world_dir: &Path,
    dest_zip: &Path,
    cancel: CancellationToken,
    progress: Option<BackupProgressFn>,
) -> Result<BackupSummary> {
    let world_dir = world_dir.to_path_buf();
    let dest = dest_zip.to_path_buf();
    blocking(move || backup_blocking(&world_dir, &dest, &cancel, progress.as_ref())).await
}

fn backup_blocking(
    world_dir: &Path,
    dest: &Path,
    cancel: &CancellationToken,
    progress: Option<&BackupProgressFn>,
) -> Result<BackupSummary> {
    if !world_dir.is_dir() {
        return Err(Error::NotFound(format!(
            "world folder {}",
            world_dir.display()
        )));
    }
    if dest.exists() {
        return Err(Error::AlreadyExists(dest.to_path_buf()));
    }
    let root_name = world_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "world".to_owned());

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(world_dir).sort_by_file_name() {
        let entry = entry.map_err(|e| {
            let path = e.path().map(Path::to_path_buf).unwrap_or_default();
            Error::io(path, e.into())
        })?;
        let rel = entry.path().strip_prefix(world_dir).unwrap_or(entry.path());
        let rel_name = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if entry.file_type().is_dir() {
            dirs.push(rel_name);
        } else if entry.file_type().is_file() && rel_name != "session.lock" {
            let meta = entry
                .metadata()
                .map_err(|e| Error::io(entry.path(), e.into()))?;
            files.push((entry.path().to_path_buf(), rel_name, meta));
        }
    }

    let mut state = BackupProgress {
        files_total: files.len() as u64,
        bytes_total: files.iter().map(|(_, _, m)| m.len()).sum(),
        ..BackupProgress::default()
    };
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let tmp = sibling_with_suffix(dest, ".part");
    let job = BackupJob {
        root_name: &root_name,
        dirs: &dirs,
        files: &files,
        cancel,
        progress,
    };
    let result = job.write(&tmp, &mut state);
    if let Err(error) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    if dest.exists() {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::AlreadyExists(dest.to_path_buf()));
    }
    std::fs::rename(&tmp, dest).map_err(|e| Error::io(dest, e))?;
    if let Some(report) = progress {
        report(state);
    }
    Ok(BackupSummary {
        path: dest.to_path_buf(),
        files: state.files_done,
        bytes: state.bytes_done,
    })
}

struct BackupJob<'a> {
    root_name: &'a str,
    dirs: &'a [String],
    files: &'a [(PathBuf, String, std::fs::Metadata)],
    cancel: &'a CancellationToken,
    progress: Option<&'a BackupProgressFn>,
}

impl BackupJob<'_> {
    fn write(&self, tmp: &Path, state: &mut BackupProgress) -> Result<()> {
        let root_name = self.root_name;
        let file = File::create(tmp).map_err(|e| Error::io(tmp, e))?;
        let mut zip = zip::ZipWriter::new(BufWriter::new(file));
        let base =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for dir in self.dirs {
            let name = if dir.is_empty() {
                format!("{root_name}/")
            } else {
                format!("{root_name}/{dir}/")
            };
            zip.add_directory(name, base)
                .map_err(|e| Error::archive(tmp, e))?;
        }
        let mut last_report = Instant::now();
        for (path, rel_name, meta) in self.files {
            if self.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let mut options = base.large_file(meta.len() >= 0xffff_ffff);
            if let Some(time) = zip_time(meta) {
                options = options.last_modified_time(time);
            }
            let mut source = File::open(path).map_err(|e| Error::io(path, e))?;
            zip.start_file(format!("{root_name}/{rel_name}"), options)
                .map_err(|e| Error::archive(tmp, e))?;
            let copied = std::io::copy(&mut source, &mut zip).map_err(|e| Error::io(path, e))?;
            state.files_done += 1;
            state.bytes_done += copied;
            if let Some(report) = self.progress
                && last_report.elapsed() >= Duration::from_millis(100)
            {
                report(*state);
                last_report = Instant::now();
            }
        }
        let mut writer = zip.finish().map_err(|e| Error::archive(tmp, e))?;
        writer.flush().map_err(|e| Error::io(tmp, e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::mods::test_support::png;

    fn level_dat(name: &str, game_type: i32, hardcore_in_settings: bool) -> Vec<u8> {
        let mut data = HashMap::new();
        data.insert("LevelName".to_owned(), Value::String(name.into()));
        data.insert("LastPlayed".to_owned(), Value::Long(1_700_000_000_000));
        data.insert("GameType".to_owned(), Value::Int(game_type));
        data.insert("allowCommands".to_owned(), Value::Byte(1));
        let mut version = HashMap::new();
        version.insert("Name".to_owned(), Value::String("1.21.1".into()));
        version.insert("Id".to_owned(), Value::Int(3955));
        data.insert("Version".to_owned(), Value::Compound(version));
        if hardcore_in_settings {
            let mut settings = HashMap::new();
            settings.insert("hardcore".to_owned(), Value::Byte(1));
            data.insert("difficulty_settings".to_owned(), Value::Compound(settings));
        } else {
            data.insert("hardcore".to_owned(), Value::Byte(0));
        }
        let mut root = HashMap::new();
        root.insert("Data".to_owned(), Value::Compound(data));
        let nbt = fastnbt::to_bytes(&Value::Compound(root)).unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&nbt).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn level_dat_summary() {
        let summary = parse_level_dat(&level_dat("My World", 1, false)).unwrap();
        assert_eq!(summary.name.as_deref(), Some("My World"));
        assert_eq!(summary.last_played, Some(1_700_000_000_000));
        assert_eq!(summary.game_type, Some(1));
        assert!(!summary.hardcore);
        assert!(summary.allow_commands);
        assert_eq!(summary.version_name.as_deref(), Some("1.21.1"));
        assert!(parse_level_dat(&level_dat("H", 0, true)).unwrap().hardcore);
        assert!(parse_level_dat(b"garbage").is_none());
        assert!(parse_level_dat(&[0x1f, 0x8b, 1, 2, 3]).is_none());
    }

    #[tokio::test]
    async fn list_backup_and_restore_layout() {
        let dir = tempfile::tempdir().unwrap();
        let saves = dir.path().join("saves");
        let world = saves.join("World 1");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::create_dir_all(world.join("data")).unwrap(); // empty dir is kept
        std::fs::write(world.join("level.dat"), level_dat("Survival Fun", 0, false)).unwrap();
        std::fs::write(world.join("region").join("r.0.0.mca"), vec![7u8; 5000]).unwrap();
        std::fs::write(world.join("session.lock"), b"lock").unwrap();
        let icon = png(16);
        std::fs::write(world.join("icon.png"), &icon).unwrap();
        // A broken world still shows up (unreadable) and a folder without level.dat does not.
        let broken = saves.join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("level.dat"), b"corrupt").unwrap();
        std::fs::create_dir_all(saves.join("not a world")).unwrap();

        let worlds = list_worlds(dir.path()).await.unwrap();
        assert_eq!(worlds.len(), 2);
        let fun = worlds.iter().find(|w| w.folder_name == "World 1").unwrap();
        assert_eq!(fun.name, "Survival Fun");
        assert_eq!(fun.game_mode, WorldGameMode::Survival);
        assert_eq!(fun.icon_png.as_deref(), Some(icon.as_slice()));
        assert!(fun.size >= 5000);
        assert!(fun.readable);
        let bad = worlds.iter().find(|w| w.folder_name == "broken").unwrap();
        assert!(!bad.readable);
        assert_eq!(bad.name, "broken");

        let dest = default_backup_path(dir.path(), "World 1");
        assert!(dest.starts_with(dir.path().join("backups")));
        let reports = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let sink = Arc::clone(&reports);
        let progress: BackupProgressFn = Arc::new(move |p| sink.lock().push(p));
        let summary = backup_world(&world, &dest, CancellationToken::new(), Some(progress))
            .await
            .unwrap();
        assert_eq!(summary.files, 3); // level.dat, region file, icon.png
        let last = *reports.lock().last().unwrap();
        assert_eq!(last.files_done, last.files_total);

        let mut archive = zip::ZipArchive::new(Cursor::new(std::fs::read(&dest).unwrap())).unwrap();
        let names: Vec<String> = archive
            .file_names()
            .map(|n| n.unwrap().into_owned())
            .collect();
        assert!(names.contains(&"World 1/region/r.0.0.mca".to_owned()));
        assert!(names.contains(&"World 1/data/".to_owned()));
        assert!(!names.iter().any(|n| n.ends_with("session.lock")));
        let mut region = Vec::new();
        archive
            .by_name("World 1/region/r.0.0.mca")
            .unwrap()
            .read_to_end(&mut region)
            .unwrap();
        assert_eq!(region.len(), 5000);

        // Never overwrites, and a cancelled backup leaves nothing behind.
        let again = backup_world(&world, &dest, CancellationToken::new(), None).await;
        assert!(matches!(again, Err(Error::AlreadyExists(_))));
        let cancelled_dest = dir.path().join("backups").join("cancelled.zip");
        let token = CancellationToken::new();
        token.cancel();
        let cancelled = backup_world(&world, &cancelled_dest, token, None).await;
        assert!(matches!(cancelled, Err(Error::Cancelled)));
        assert!(!cancelled_dest.exists());
        assert!(!sibling_with_suffix(&cancelled_dest, ".part").exists());
    }
}
