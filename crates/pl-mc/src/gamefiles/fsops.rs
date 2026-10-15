//! Filesystem helpers shared by the game-file modules (mods, packs, worlds, modpacks, servers).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::modpack::safe_path::validate_file_name;

/// Icons/thumbnails larger than this are not loaded (the UI shows a placeholder instead).
pub(crate) const MAX_ICON_BYTES: u64 = 256 * 1024;
/// Metadata files (`fabric.mod.json`, `pack.mcmeta`, …) larger than this are ignored.
pub(crate) const MAX_METADATA_BYTES: u64 = 1024 * 1024;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Run blocking work (file I/O, zip, hashing) on tokio's blocking pool.
pub(crate) async fn blocking<T, F>(work: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(e) if e.is_panic() => std::panic::resume_unwind(e.into_panic()),
        Err(_) => Err(Error::Cancelled),
    }
}

/// `name` + suffix in the same directory (for temporary files that are renamed into place).
pub(crate) fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Write a file atomically: `<path>.tmp`, then rename over the target.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let tmp = sibling_with_suffix(path, ".tmp");
    let result = std::fs::write(&tmp, bytes)
        .map_err(|e| Error::io(&tmp, e))
        .and_then(|()| std::fs::rename(&tmp, path).map_err(|e| Error::io(path, e)));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub(crate) fn modified_ms(meta: &std::fs::Metadata) -> Option<i64> {
    let since_epoch = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(since_epoch.as_millis()).ok()
}

/// Total size of all files below `path` (0 when unreadable). Symlinks are not followed.
pub fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

pub(crate) fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(PNG_SIGNATURE)
}

/// Read at most `limit` bytes; `None` if the source is larger or is not a PNG.
pub(crate) fn read_png_limited(reader: impl Read, limit: u64) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= limit && is_png(&bytes)).then_some(bytes)
}

/// Read a small PNG from disk (icon.png, pack.png).
pub(crate) fn read_png_file(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    read_png_limited(file, MAX_ICON_BYTES)
}

/// Read a small text file from disk, lossily decoded.
pub(crate) fn read_text_file(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAX_METADATA_BYTES).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// Directory entries, or an empty list when the directory does not exist.
pub(crate) fn read_dir_or_empty(dir: &Path) -> Result<Vec<std::fs::DirEntry>> {
    match std::fs::read_dir(dir) {
        Ok(entries) => Ok(entries.filter_map(|e| e.ok()).collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(Error::io(dir, e)),
    }
}

/// How a path was removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RemovalMethod {
    /// Moved to the OS trash / Recycle Bin (restorable).
    Trash,
    /// Deleted for good (only when the trash was unavailable and the caller allowed it).
    Permanent,
}

/// Move a file or folder to the OS trash. If the trash is unavailable, delete permanently only
/// when `allow_permanent` is set; otherwise fail with [`Error::TrashUnavailable`] so the UI can
/// ask the user first.
pub async fn remove_path(path: &Path, allow_permanent: bool) -> Result<RemovalMethod> {
    remove_path_with(path, allow_permanent, |p| {
        trash::delete(p).map_err(|e| e.to_string())
    })
    .await
}

async fn remove_path_with(
    path: &Path,
    allow_permanent: bool,
    move_to_trash: impl FnOnce(&Path) -> std::result::Result<(), String> + Send + 'static,
) -> Result<RemovalMethod> {
    let path = path.to_path_buf();
    blocking(move || {
        let meta = std::fs::symlink_metadata(&path).map_err(|e| Error::io(&path, e))?;
        match move_to_trash(&path) {
            Ok(()) => Ok(RemovalMethod::Trash),
            Err(trash_error) if !allow_permanent => Err(Error::TrashUnavailable {
                path,
                message: trash_error,
            }),
            Err(trash_error) => {
                tracing::info!(path = %path.display(), error = %trash_error, "trash unavailable, deleting permanently");
                let removed = if meta.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                removed.map_err(|e| Error::io(&path, e))?;
                Ok(RemovalMethod::Permanent)
            }
        }
    })
    .await
}

/// What kind of file is being imported into a game directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportKind {
    Mod,
    ResourcePack,
    ShaderPack,
}

impl ImportKind {
    /// Folder inside the game directory.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::ResourcePack => "resourcepacks",
            Self::ShaderPack => "shaderpacks",
        }
    }

    fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Mod => &[".jar", ".zip", ".litemod"],
            Self::ResourcePack | Self::ShaderPack => &[".zip"],
        }
    }

    pub fn accepts(self, file_name: &str) -> bool {
        let lower = file_name.to_ascii_lowercase();
        self.extensions().iter().any(|ext| lower.ends_with(ext))
    }
}

/// Names that would collide with `name` in a folder (a mod and its `.disabled` twin count as one).
fn clash_names(name: &str) -> Vec<String> {
    let mut names = vec![name.to_owned(), format!("{name}.disabled")];
    if let Some(enabled) = name.strip_suffix(".disabled") {
        names.push(enabled.to_owned());
    }
    names
}

fn existing_clash(dest_dir: &Path, name: &str) -> Option<PathBuf> {
    clash_names(name)
        .into_iter()
        .map(|n| dest_dir.join(n))
        .find(|p| std::fs::symlink_metadata(p).is_ok())
}

/// Copy `src` into `dest_dir` under its own file name. Refuses unsafe names and never
/// overwrites: an existing file (or its `.disabled` twin) gives [`Error::AlreadyExists`].
pub async fn import_file(src: &Path, dest_dir: &Path) -> Result<PathBuf> {
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::Invalid(format!("{} has no usable file name", src.display())))?
        .to_owned();
    validate_file_name(&name)?;
    let src = src.to_path_buf();
    let dest_dir = dest_dir.to_path_buf();
    blocking(move || {
        let meta = std::fs::metadata(&src).map_err(|e| Error::io(&src, e))?;
        if !meta.is_file() {
            return Err(Error::Invalid(format!("{} is not a file", src.display())));
        }
        std::fs::create_dir_all(&dest_dir).map_err(|e| Error::io(&dest_dir, e))?;
        if let Some(existing) = existing_clash(&dest_dir, &name) {
            return Err(Error::AlreadyExists(existing));
        }
        let dest = dest_dir.join(&name);
        let tmp = dest_dir.join(format!(".{name}.part"));
        let copied = std::fs::copy(&src, &tmp).map_err(|e| Error::io(&tmp, e));
        let result = copied.and_then(|_| {
            // Re-check right before the rename: a rename would silently replace a file that
            // appeared while copying.
            if let Some(existing) = existing_clash(&dest_dir, &name) {
                return Err(Error::AlreadyExists(existing));
            }
            std::fs::rename(&tmp, &dest).map_err(|e| Error::io(&dest, e))
        });
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result.map(|()| dest)
    })
    .await
}

/// Import a mod, resource pack or shader pack file into the matching folder of `game_dir`
/// (the old launcher's "Doku Paketi Aktar", generalized).
pub async fn import_into(game_dir: &Path, kind: ImportKind, src: &Path) -> Result<PathBuf> {
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !kind.accepts(&name) {
        return Err(Error::Invalid(format!(
            "{name:?} is not a {} file",
            match kind {
                ImportKind::Mod => "mod",
                ImportKind::ResourcePack => "resource pack",
                ImportKind::ShaderPack => "shader pack",
            }
        )));
    }
    import_file(src, &game_dir.join(kind.folder())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn import_copies_and_refuses_clashes() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("Faithful 32x.zip");
        std::fs::write(&src, b"zipdata").unwrap();
        let game = dir.path().join("game");

        let dest = import_into(&game, ImportKind::ResourcePack, &src)
            .await
            .unwrap();
        assert_eq!(dest, game.join("resourcepacks").join("Faithful 32x.zip"));
        assert_eq!(std::fs::read(&dest).unwrap(), b"zipdata");

        let again = import_into(&game, ImportKind::ResourcePack, &src).await;
        assert!(matches!(again, Err(Error::AlreadyExists(_))));

        // A disabled twin counts as a clash for mods.
        let jar = dir.path().join("sodium.jar");
        std::fs::write(&jar, b"jar").unwrap();
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods").join("sodium.jar.disabled"), b"old").unwrap();
        let clash = import_into(&game, ImportKind::Mod, &jar).await;
        assert!(matches!(clash, Err(Error::AlreadyExists(_))));

        // Wrong extension and directories are refused; no temp files are left behind.
        assert!(
            import_into(&game, ImportKind::ShaderPack, &jar)
                .await
                .is_err()
        );
        assert!(import_file(dir.path(), &game).await.is_err());
        let leftovers: Vec<_> = std::fs::read_dir(game.join("resourcepacks"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[tokio::test]
    async fn removal_falls_back_to_permanent_only_when_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.txt");
        let folder = dir.path().join("world");
        std::fs::write(&file, b"x").unwrap();
        std::fs::create_dir_all(folder.join("region")).unwrap();
        std::fs::write(folder.join("region").join("r.0.0.mca"), b"r").unwrap();
        let no_trash = |_: &Path| Err("no trash here".to_owned());

        // The real OS trash is not touched in tests: a fake one is injected.
        let refused = remove_path_with(&file, false, no_trash).await;
        assert!(matches!(refused, Err(Error::TrashUnavailable { .. })));
        assert!(file.exists());

        let method = remove_path_with(&file, true, no_trash).await.unwrap();
        assert_eq!(method, RemovalMethod::Permanent);
        assert!(!file.exists());
        let method = remove_path_with(&folder, true, no_trash).await.unwrap();
        assert_eq!(method, RemovalMethod::Permanent);
        assert!(!folder.exists());

        let trashed = remove_path_with(dir.path(), false, |_: &Path| Ok(())).await;
        assert_eq!(trashed.unwrap(), RemovalMethod::Trash);
        assert!(
            remove_path_with(&file, true, no_trash).await.is_err(),
            "missing path is an error"
        );
    }

    #[test]
    fn atomic_write_replaces_target() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("f.dat");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert!(!sibling_with_suffix(&path, ".tmp").exists());
    }
}
