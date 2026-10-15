//! `versions/<id>/projects-loader.json`: every file the Forge/NeoForge installer step placed
//! outside the profile's own library list (installer libraries, jars shipped inside the installer,
//! processor outputs) with its SHA-1 and size. A marker whose files all still verify means the
//! installer step is complete, so re-installs and pre-launch checks skip the installer entirely.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use futures::StreamExt;
use pl_net::{Checksum, HashAlgo, VerifyMode, hash_file, verify_file};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::loaders::LoaderKind;
use crate::paths::{DataDirs, safe_join, write_atomic};

pub(crate) const MARKER_FILE: &str = "projects-loader.json";
const FORMAT: u32 = 1;
/// Parallel hashing/verification jobs.
const PARALLEL: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Marker {
    pub format: u32,
    pub kind: LoaderKind,
    pub minecraft: String,
    pub loader: String,
    pub files: Vec<MarkerFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MarkerFile {
    /// Relative to the data root, `/`-separated.
    pub path: String,
    pub sha1: String,
    pub size: u64,
}

impl Marker {
    pub fn new(kind: LoaderKind, minecraft: &str, loader: &str, files: Vec<MarkerFile>) -> Self {
        Self {
            format: FORMAT,
            kind,
            minecraft: minecraft.to_owned(),
            loader: loader.to_owned(),
            files,
        }
    }

    pub fn is_for(&self, kind: LoaderKind, minecraft: &str, loader: &str) -> bool {
        self.format == FORMAT
            && self.kind == kind
            && self.minecraft == minecraft
            && self.loader == loader
    }
}

fn marker_path(dirs: &DataDirs, id: &str) -> Result<PathBuf> {
    Ok(dirs.version_dir(id)?.join(MARKER_FILE))
}

pub(crate) async fn read(dirs: &DataDirs, id: &str) -> Option<Marker> {
    let path = marker_path(dirs, id).ok()?;
    let bytes = tokio::fs::read(&path).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) async fn write(dirs: &DataDirs, id: &str, marker: &Marker) -> Result<()> {
    let path = marker_path(dirs, id)?;
    let bytes = serde_json::to_vec_pretty(marker).map_err(|e| Error::json(MARKER_FILE, e))?;
    write_atomic(&path, &bytes).await
}

/// Whether every file of the marker is still there (and matches, per `mode`).
pub(crate) async fn verify(dirs: &DataDirs, marker: &Marker, mode: VerifyMode) -> bool {
    let root = dirs.root();
    // Futures are built eagerly so the returned future stays `Send` for spawned callers.
    let checks: Vec<_> = marker
        .files
        .iter()
        .map(|file| {
            let path = safe_join(root, &file.path);
            let checksum = Checksum::sha1(&file.sha1);
            let size = file.size;
            async move {
                match path {
                    Ok(path) => verify_file(&path, Some(&checksum), Some(size), mode).await,
                    Err(_) => false,
                }
            }
        })
        .collect();
    let results: Vec<bool> = futures::stream::iter(checks)
        .buffer_unordered(PARALLEL)
        .collect()
        .await;
    results.into_iter().all(|ok| ok)
}

/// `path` relative to `root` with `/` separators (`None` when outside the root).
pub(crate) fn relative(root: &Path, path: &Path) -> Option<String> {
    let rest = path.strip_prefix(root).ok()?;
    let parts = rest
        .components()
        .map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// Describe existing `paths` (missing ones and paths outside the root are left out). `known`
/// SHA-1s avoid hashing files that were just verified or extracted.
pub(crate) async fn describe(
    dirs: &DataDirs,
    paths: Vec<PathBuf>,
    known: &HashMap<PathBuf, String>,
) -> Result<Vec<MarkerFile>> {
    let root = dirs.root().to_path_buf();
    let mut seen = HashSet::new();
    let jobs: Vec<_> = paths
        .into_iter()
        .filter(|p| seen.insert(p.clone()))
        .map(|path| {
            let known = known.get(&path).cloned();
            let relative = relative(&root, &path);
            async move {
                let Some(relative) = relative else {
                    tracing::warn!(path = %path.display(), "installer output outside the data folder");
                    return Ok(None);
                };
                let meta = match tokio::fs::metadata(&path).await {
                    Ok(meta) if meta.is_file() => meta,
                    _ => return Ok(None),
                };
                let sha1 = match known {
                    Some(sha1) => sha1,
                    None => hash_file(&path, HashAlgo::Sha1).await?,
                };
                Ok::<_, Error>(Some(MarkerFile {
                    path: relative,
                    sha1,
                    size: meta.len(),
                }))
            }
        })
        .collect();
    let results: Vec<Result<Option<MarkerFile>>> = futures::stream::iter(jobs)
        .buffer_unordered(PARALLEL)
        .collect()
        .await;
    let mut files = Vec::new();
    for result in results {
        if let Some(file) = result? {
            files.push(file);
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn marker_round_trip_and_verification() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(tmp.path());
        let a = dirs.library_file("net/x/a/1/a-1.jar").unwrap();
        let b = dirs.library_file("net/x/b/1/b-1.jar").unwrap();
        for (path, bytes) in [(&a, b"abc".as_slice()), (&b, b"hello".as_slice())] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let known = HashMap::from([(
            a.clone(),
            "a9993e364706816aba3e25717850c26c9cd0d89d".to_owned(),
        )]);
        let files = describe(
            &dirs,
            vec![
                b.clone(),
                a.clone(),
                a.clone(),
                dirs.library_file("missing.jar").unwrap(),
                PathBuf::from("/elsewhere/x.jar"),
            ],
            &known,
        )
        .await
        .unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "libraries/net/x/a/1/a-1.jar");
        assert_eq!(files[0].size, 3);
        assert_eq!(files[1].sha1, "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");

        let marker = Marker::new(LoaderKind::Forge, "1.20.1", "47.4.10", files);
        write(&dirs, "1.20.1-forge-47.4.10", &marker).await.unwrap();
        let read_back = read(&dirs, "1.20.1-forge-47.4.10").await.unwrap();
        assert_eq!(read_back, marker);
        assert!(read_back.is_for(LoaderKind::Forge, "1.20.1", "47.4.10"));
        assert!(!read_back.is_for(LoaderKind::NeoForge, "1.20.1", "47.4.10"));
        assert!(!read_back.is_for(LoaderKind::Forge, "1.20.1", "47.4.9"));
        assert!(verify(&dirs, &marker, VerifyMode::Full).await);

        // Same size, other content: only a full check notices.
        std::fs::write(&b, b"HELLO").unwrap();
        assert!(verify(&dirs, &marker, VerifyMode::Size).await);
        assert!(!verify(&dirs, &marker, VerifyMode::Full).await);
        std::fs::remove_file(&a).unwrap();
        assert!(!verify(&dirs, &marker, VerifyMode::Exists).await);
        assert!(read(&dirs, "other").await.is_none());
    }

    #[test]
    fn relative_paths() {
        let root = Path::new("/data");
        assert_eq!(
            relative(root, Path::new("/data/libraries/a/b.jar")).as_deref(),
            Some("libraries/a/b.jar")
        );
        assert_eq!(relative(root, Path::new("/other/a.jar")), None);
        assert_eq!(relative(root, Path::new("/data")), None);
    }
}
