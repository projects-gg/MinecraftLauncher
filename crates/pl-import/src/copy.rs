//! Copies that only read the other launcher's files and never leave half a file or folder in
//! ours: each file goes through a temporary file next to its target, each folder through a
//! temporary folder renamed at the end. Nothing that exists is replaced. Links are not
//! followed (a junction in a game folder could lead anywhere).

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

use crate::error::{Error, Result};

/// Part of every temporary name, so leftovers of an interrupted run are known.
pub(crate) const TEMP_MARK: &str = ".pl-import-";
const CHUNK: usize = 1024 * 1024;

pub(crate) struct Copier<'a> {
    pub cancel: &'a CancellationToken,
    /// Called with the size of every chunk written.
    pub on_bytes: &'a (dyn Fn(u64) + Send + Sync),
}

impl Copier<'_> {
    /// Copies the file `from` to `dest`, which must not exist. Keeps its modification time.
    pub(crate) fn file(&self, from: &Path, dest: &Path) -> Result<u64> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        if dest.exists() {
            return Err(exists(dest));
        }
        let tmp = temp_path(dest);
        let copied = self.copy_into(from, &tmp).and_then(|n| {
            if dest.exists() {
                return Err(exists(dest));
            }
            std::fs::rename(&tmp, dest).map_err(|e| Error::io(dest, e))?;
            Ok(n)
        });
        if copied.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        copied
    }

    /// Copies the folder `from` to `dest`, which must not exist. `skip` leaves out entries (by
    /// their path relative to `from`); `last` names top-level files copied after all others (a
    /// world's `level.dat`: until it is there, a folder is no world).
    pub(crate) fn folder(
        &self,
        from: &Path,
        dest: &Path,
        skip: &dyn Fn(&Path) -> bool,
        last: &[&str],
    ) -> Result<u64> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        if dest.exists() {
            return Err(exists(dest));
        }
        let tmp = temp_path(dest);
        let copied = self.fill(from, &tmp, skip, last).and_then(|n| {
            if dest.exists() {
                return Err(exists(dest));
            }
            std::fs::rename(&tmp, dest).map_err(|e| Error::io(dest, e))?;
            Ok(n)
        });
        if copied.is_err() {
            let _ = std::fs::remove_dir_all(&tmp);
        }
        copied
    }

    fn fill(
        &self,
        from: &Path,
        to: &Path,
        skip: &dyn Fn(&Path) -> bool,
        last: &[&str],
    ) -> Result<u64> {
        std::fs::create_dir_all(to).map_err(|e| Error::io(to, e))?;
        let mut total = 0;
        let mut deferred = Vec::new();
        let walker = WalkDir::new(from)
            .min_depth(1)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                e.path()
                    .strip_prefix(from)
                    .map(|rel| !skip(rel))
                    .unwrap_or(false)
            });
        for entry in walker {
            if self.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let entry = entry.map_err(|e| {
                let path = e
                    .path()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| from.to_path_buf());
                Error::io(
                    path,
                    e.into_io_error()
                        .unwrap_or_else(|| std::io::Error::other("walk failed")),
                )
            })?;
            let Ok(rel) = entry.path().strip_prefix(from) else {
                continue;
            };
            let target = to.join(rel);
            let kind = entry.file_type();
            if kind.is_dir() {
                std::fs::create_dir_all(&target).map_err(|e| Error::io(&target, e))?;
            } else if kind.is_file() {
                let top_level_last = entry.depth() == 1
                    && last.iter().any(|name| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .eq_ignore_ascii_case(name)
                    });
                if top_level_last {
                    deferred.push((entry.path().to_path_buf(), target));
                } else {
                    total += self.copy_into(entry.path(), &target)?;
                }
            }
            // Links (symbolic, junctions) are left out.
        }
        for (source, target) in deferred {
            total += self.copy_into(&source, &target)?;
        }
        Ok(total)
    }

    /// Copies one file's bytes into a new file at `to`.
    fn copy_into(&self, from: &Path, to: &Path) -> Result<u64> {
        let mut input = File::open(from).map_err(|e| Error::io(from, e))?;
        let meta = input.metadata().map_err(|e| Error::io(from, e))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(|e| Error::io(to, e))?;
        let mut buf = vec![0u8; CHUNK.min(meta.len().max(1) as usize).max(4096)];
        let mut total = 0u64;
        loop {
            if self.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let n = input.read(&mut buf).map_err(|e| Error::io(from, e))?;
            if n == 0 {
                break;
            }
            output.write_all(&buf[..n]).map_err(|e| Error::io(to, e))?;
            total += n as u64;
            (self.on_bytes)(n as u64);
        }
        if let Ok(modified) = meta.modified() {
            let _ = output.set_modified(modified);
        }
        Ok(total)
    }
}

fn exists(path: &Path) -> Error {
    Error::io(
        path,
        std::io::Error::new(std::io::ErrorKind::AlreadyExists, "already exists"),
    )
}

/// `.<name>.pl-import-<random>` next to `dest`.
fn temp_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dest.with_file_name(format!(
        ".{name}{TEMP_MARK}{}",
        uuid::Uuid::new_v4().simple()
    ))
}

/// Removes what an interrupted run left in `dir` (its own temporary names only).
pub(crate) fn remove_leftovers(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') || !name.contains(TEMP_MARK) {
            continue;
        }
        let path = entry.path();
        let removed = if entry.file_type().is_ok_and(|t| t.is_dir()) {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if let Err(error) = removed {
            tracing::warn!(%error, path = %path.display(), "import leftover not removed");
        }
    }
}

/// Bytes of the files under `path` (a file's own size), links not followed; `skip` as in
/// [`Copier::folder`].
pub(crate) fn tree_size(path: &Path, skip: &dyn Fn(&Path) -> bool) -> u64 {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => return meta.len(),
        Ok(meta) if meta.is_dir() => {}
        _ => return 0,
    }
    WalkDir::new(path)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            e.path()
                .strip_prefix(path)
                .map(|rel| !skip(rel))
                .unwrap_or(false)
        })
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// A name in `dir` for `name` that nothing has yet: `name`, `name (2)`, `name (3)`… (files keep
/// their extension: `pack (2).zip`).
pub(crate) fn free_name(dir: &Path, name: &str, is_file: bool) -> String {
    if !dir.join(name).exists() {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if is_file && !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (2..10_000u32)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !dir.join(candidate).exists())
        .unwrap_or_else(|| format!("{stem} ({}){ext}", uuid::Uuid::new_v4().simple()))
}

pub(crate) fn no_skip(_: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    #[test]
    fn folders_arrive_whole_with_level_dat_last_and_links_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("src/World");
        std::fs::create_dir_all(from.join("region")).unwrap();
        std::fs::write(from.join("level.dat"), b"level").unwrap();
        std::fs::write(from.join("region/r.0.0.mca"), vec![1u8; 3000]).unwrap();
        std::fs::write(from.join("session.lock"), b"x").unwrap();
        let cancel = CancellationToken::new();
        let counted = AtomicU64::new(0);
        let count = |n: u64| {
            counted.fetch_add(n, Ordering::Relaxed);
        };
        let copy = Copier {
            cancel: &cancel,
            on_bytes: &count,
        };
        let dest = dir.path().join("dst/saves/World");
        let skip = |rel: &Path| rel == Path::new("session.lock");
        let bytes = copy.folder(&from, &dest, &skip, &["level.dat"]).unwrap();
        assert_eq!(bytes, 3005);
        assert_eq!(counted.load(Ordering::Relaxed), 3005);
        assert_eq!(std::fs::read(dest.join("level.dat")).unwrap(), b"level");
        assert!(!dest.join("session.lock").exists());
        // The source is as it was; nothing temporary is left.
        assert!(from.join("session.lock").exists());
        let leftovers: Vec<_> = std::fs::read_dir(dest.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(leftovers, [std::ffi::OsString::from("World")]);
        // An existing target is never replaced.
        assert!(copy.folder(&from, &dest, &no_skip, &[]).is_err());
        assert!(
            copy.file(&from.join("level.dat"), &dest.join("level.dat"))
                .is_err()
        );
    }

    #[test]
    fn a_cancelled_copy_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("big.zip");
        std::fs::write(&from, vec![0u8; 3 * CHUNK]).unwrap();
        let cancel = CancellationToken::new();
        let counted = AtomicU64::new(0);
        let cancel_after_first = {
            let cancel = cancel.clone();
            move |n: u64| {
                counted.fetch_add(n, Ordering::Relaxed);
                cancel.cancel();
            }
        };
        let copy = Copier {
            cancel: &cancel,
            on_bytes: &cancel_after_first,
        };
        let dest = dir.path().join("out/big.zip");
        assert!(matches!(copy.file(&from, &dest), Err(Error::Cancelled)));
        assert!(!dest.exists());
        assert_eq!(
            std::fs::read_dir(dir.path().join("out")).unwrap().count(),
            0
        );
    }

    #[test]
    fn free_names_and_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pack.zip"), b"x").unwrap();
        std::fs::create_dir(dir.path().join("New World")).unwrap();
        assert_eq!(free_name(dir.path(), "pack.zip", true), "pack (2).zip");
        assert_eq!(free_name(dir.path(), "New World", false), "New World (2)");
        assert_eq!(free_name(dir.path(), "Other", false), "Other");
        std::fs::create_dir(dir.path().join(format!(".New World{TEMP_MARK}abc"))).unwrap();
        std::fs::write(dir.path().join(format!(".a.zip{TEMP_MARK}def")), b"x").unwrap();
        remove_leftovers(dir.path());
        let mut names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["New World", "pack.zip"]);
        assert_eq!(tree_size(dir.path(), &no_skip), 1);
    }
}
