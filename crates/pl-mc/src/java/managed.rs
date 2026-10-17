//! Java runtimes the launcher downloaded (Mojang's, `runtimes/<component>/<platform>/`): the
//! list Settings → Java shows, updates and removal.
//!
//! An update never touches a runtime a game may be using. The new build is downloaded next to it
//! into `<platform>.next/` (files that did not change are hard-linked from the build in use, so
//! only what changed is downloaded; the marker is written last, so a `.next` folder with a
//! marker is complete). It takes the old build's place only while no game runs:
//! `<platform>` → `<platform>.old`, `<platform>.next` → `<platform>`, then `.old` goes. Every step
//! is a rename, so a crash leaves a state [`recover`] finishes or undoes before anything reads the
//! folder. A removed runtime is renamed to `<platform>.gone` first and deleted after, so a removal
//! that stops half-way never leaves a broken runtime that looks installed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pl_net::{CancellationToken, ProgressFn, VerifyMode};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{Error, Result};
use crate::install::McContext;
use crate::java::detect::parse_major;
use crate::java::runtime::{
    MANIFEST_FILE, MARKER_FILE, RuntimeEntry, RuntimeIndex, RuntimeManifest, RuntimeMarker,
    download_manifest, install_files, read_manifest_blocking, read_marker, read_marker_blocking,
    runtime_java_executable, write_marker,
};
use crate::paths::{DataDirs, safe_join, safe_name, write_atomic_blocking};
use crate::platform::{Arch, Os, java_runtime_platform_arch};
use crate::version::component_major;

/// Next to `<platform>/`: a complete update once its marker exists.
pub const STAGED_SUFFIX: &str = ".next";
/// Next to `<platform>/`: the previous build during a swap.
pub const OLD_SUFFIX: &str = ".old";
/// Next to `<platform>/`: a removed runtime being deleted.
pub const REMOVED_SUFFIX: &str = ".gone";

/// When the launcher asks Mojang for runtime updates again after a successful check.
pub const CHECK_EVERY_MS: i64 = 24 * 60 * 60 * 1000;
/// When it tries again after a failed check.
pub const RETRY_AFTER_MS: i64 = 60 * 60 * 1000;

/// `meta/java-updates.json`.
const SCHEDULE_FILE: &str = "java-updates.json";

fn sibling(root: &Path, suffix: &str) -> PathBuf {
    let mut name = root.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    root.with_file_name(name)
}

/// `<platform>.next/` of a runtime folder.
pub fn staged_dir(root: &Path) -> PathBuf {
    sibling(root, STAGED_SUFFIX)
}

/// `<platform>.old/` of a runtime folder.
pub fn old_dir(root: &Path) -> PathBuf {
    sibling(root, OLD_SUFFIX)
}

fn removed_dir(root: &Path) -> PathBuf {
    sibling(root, REMOVED_SUFFIX)
}

/// An update waiting in `<platform>.next/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedRuntime {
    pub version: String,
    pub manifest_sha1: String,
}

/// An installed launcher runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedRuntime {
    pub component: String,
    /// Mojang platform key (`windows-x64`).
    pub platform: String,
    pub root: PathBuf,
    /// Installed build (`21.0.7`, `8u51-cacert462b08`).
    pub version: String,
    pub manifest_sha1: String,
    pub major: u32,
    /// The executable games start with (`bin/javaw.exe` on Windows).
    pub executable: PathBuf,
    pub arch: Arch,
    /// A complete update waiting for the runtime to be free.
    pub staged: Option<StagedRuntime>,
}

fn sorted_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

/// Every complete runtime under `runtimes/` (marker and executable present), interrupted swaps
/// finished first. Blocking (directory listing, small files).
pub fn list_managed(dirs: &DataDirs, os: Os) -> Vec<ManagedRuntime> {
    let mut out = Vec::new();
    for component_dir in sorted_dirs(&dirs.runtimes()) {
        let Some(component) = file_name(&component_dir).filter(|n| !n.starts_with('.')) else {
            continue;
        };
        for root in sorted_dirs(&component_dir) {
            let Some(platform) = file_name(&root).filter(|n| !n.contains('.')) else {
                continue;
            };
            if let Err(error) = recover(&root) {
                tracing::warn!(%error, root = %root.display(), "could not finish a runtime swap");
            }
            let Some(marker) = read_marker_blocking(&root) else {
                continue;
            };
            let executable = runtime_java_executable(&root, os);
            if !executable.is_file() {
                continue;
            }
            let staged = read_marker_blocking(&staged_dir(&root))
                .filter(|next| next.manifest_sha1 != marker.manifest_sha1)
                .map(|next| StagedRuntime {
                    version: next.version,
                    manifest_sha1: next.manifest_sha1,
                });
            out.push(ManagedRuntime {
                major: parse_major(&marker.version)
                    .or_else(|| component_major(&component))
                    .unwrap_or(8),
                arch: java_runtime_platform_arch(&platform),
                component: component.clone(),
                platform,
                root,
                version: marker.version,
                manifest_sha1: marker.manifest_sha1,
                executable,
                staged,
            });
        }
    }
    out
}

/// The launcher runtime (component, platform) a Java executable lives in, if any.
pub fn managed_runtime_of(dirs: &DataDirs, executable: &Path) -> Option<(String, String)> {
    let runtimes = dirs.runtimes();
    let relative = executable
        .strip_prefix(&runtimes)
        .map(Path::to_path_buf)
        .ok()
        .or_else(|| {
            // Typed paths may differ in case or spelling (`C:/` vs `C:\`).
            let exe = std::fs::canonicalize(executable).ok()?;
            let base = std::fs::canonicalize(&runtimes).ok()?;
            exe.strip_prefix(base).map(Path::to_path_buf).ok()
        })?;
    let mut parts = relative.components();
    let component = parts.next()?.as_os_str().to_str()?.to_owned();
    let platform = parts.next()?.as_os_str().to_str()?.to_owned();
    (parts.next().is_some() && !platform.contains('.') && !component.starts_with('.'))
        .then_some((component, platform))
}

/// The build Mojang offers now for this runtime, when it differs from the installed one.
pub fn update_for<'a>(
    index: &'a RuntimeIndex,
    runtime: &ManagedRuntime,
) -> Option<&'a RuntimeEntry> {
    index
        .get(&runtime.platform)?
        .get(&runtime.component)?
        .first()
        .filter(|entry| entry.manifest.sha1 != runtime.manifest_sha1)
}

/// The runtimes to stage now: those `wanted` (in use, or asked for) with an update that is not
/// staged yet.
pub fn runtimes_to_stage<'a>(
    installed: &'a [ManagedRuntime],
    index: &RuntimeIndex,
    wanted: impl Fn(&ManagedRuntime) -> bool,
) -> Vec<(&'a ManagedRuntime, RuntimeEntry)> {
    installed
        .iter()
        .filter(|runtime| wanted(runtime))
        .filter_map(|runtime| {
            let entry = update_for(index, runtime)?;
            let staged = runtime
                .staged
                .as_ref()
                .is_some_and(|staged| staged.manifest_sha1 == entry.manifest.sha1);
            (!staged).then(|| (runtime, entry.clone()))
        })
        .collect()
}

/// Files of `manifest` that did not change since the build in `root`: linked (else copied) into
/// `next`, so only what changed is downloaded. Blocking. Returns how many were taken over.
fn seed_from_current(root: &Path, next: &Path, manifest: &RuntimeManifest) -> usize {
    let Some(current) = read_manifest_blocking(&root.join(MANIFEST_FILE)) else {
        return 0;
    };
    let mut seeded = 0;
    for (name, file) in &manifest.files {
        if file.kind != "file" {
            continue;
        }
        let (Some(new), Some(old)) = (
            file.downloads.as_ref(),
            current
                .files
                .get(name)
                .and_then(|old| old.downloads.as_ref()),
        ) else {
            continue;
        };
        if new.raw.sha1 != old.raw.sha1 {
            continue;
        }
        let (Ok(source), Ok(dest)) = (safe_join(root, name), safe_join(next, name)) else {
            continue;
        };
        if dest.exists()
            || !std::fs::metadata(&source).is_ok_and(|m| m.is_file() && m.len() == new.raw.size)
        {
            continue;
        }
        if let Some(parent) = dest.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            continue;
        }
        if std::fs::hard_link(&source, &dest).is_ok() || std::fs::copy(&source, &dest).is_ok() {
            seeded += 1;
        }
    }
    seeded
}

/// Download `entry` (Mojang's current build of `component` for `platform`) into
/// `<platform>.next/` next to the installed runtime, without touching it. A complete copy of the
/// same build is kept as it is; an interrupted one continues (files already there are checked by
/// their hash).
pub async fn stage_update(
    ctx: &McContext,
    component: &str,
    platform: &str,
    entry: &RuntimeEntry,
    cancel: &CancellationToken,
    progress: Option<ProgressFn>,
) -> Result<StagedRuntime> {
    safe_name(component)?;
    let root = ctx.dirs.runtime_dir(component, platform)?;
    let next = staged_dir(&root);
    let staged = StagedRuntime {
        version: entry.version.name.clone(),
        manifest_sha1: entry.manifest.sha1.clone(),
    };
    if read_marker(&next)
        .await
        .is_some_and(|marker| marker.manifest_sha1 == entry.manifest.sha1)
    {
        return Ok(staged);
    }
    // Incomplete until the marker is written again, at the very end.
    let marker_path = next.join(MARKER_FILE);
    match tokio::fs::remove_file(&marker_path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(Error::io(&marker_path, error)),
    }
    let manifest = download_manifest(ctx, entry, &next, cancel).await?;
    let seeded = {
        let (root, next, manifest) = (root.clone(), next.clone(), manifest.clone());
        tokio::task::spawn_blocking(move || seed_from_current(&root, &next, &manifest))
            .await
            .unwrap_or(0)
    };
    install_files(ctx, &next, &manifest, VerifyMode::Full, cancel, progress).await?;
    write_marker(
        &next,
        &RuntimeMarker {
            component: component.to_owned(),
            platform: platform.to_owned(),
            version: entry.version.name.clone(),
            manifest_sha1: entry.manifest.sha1.clone(),
        },
    )
    .await?;
    tracing::info!(
        %component,
        %platform,
        version = %entry.version.name,
        seeded,
        files = manifest.files.len(),
        "Java runtime update staged"
    );
    Ok(staged)
}

/// What [`swap_in_staged`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwapOutcome {
    /// The staged build is the runtime now.
    Swapped { version: String },
    /// Nothing complete is staged.
    NothingStaged,
    /// The runtime could not be moved (a program still has its files open); nothing changed.
    InUse,
}

/// Puts a complete staged update in place of the runtime in `root`. The caller makes sure no
/// game of this launcher runs; a program that still holds the files makes the first rename fail
/// and nothing changes (`InUse`). Blocking.
pub fn swap_in_staged(root: &Path) -> Result<SwapOutcome> {
    recover(root)?;
    let next = staged_dir(root);
    let Some(staged) = read_marker_blocking(&next) else {
        return Ok(SwapOutcome::NothingStaged);
    };
    let old = old_dir(root);
    if old.exists()
        && let Err(error) = std::fs::remove_dir_all(&old)
    {
        tracing::info!(%error, "an earlier runtime is still in use; the update waits");
        return Ok(SwapOutcome::InUse);
    }
    let had_current = root.exists();
    if had_current && let Err(error) = std::fs::rename(root, &old) {
        tracing::info!(%error, root = %root.display(), "runtime in use; the update waits");
        return Ok(SwapOutcome::InUse);
    }
    if let Err(error) = std::fs::rename(&next, root) {
        tracing::warn!(%error, "could not move the staged runtime; keeping the old one");
        if had_current {
            std::fs::rename(&old, root).map_err(|e| Error::io(root, e))?;
        }
        return Ok(SwapOutcome::InUse);
    }
    if had_current && let Err(error) = std::fs::remove_dir_all(&old) {
        tracing::debug!(%error, "the previous runtime is removed later");
    }
    tracing::info!(root = %root.display(), version = %staged.version, "Java runtime updated");
    Ok(SwapOutcome::Swapped {
        version: staged.version,
    })
}

/// Finishes or undoes a swap a crash interrupted, and removes leftovers of earlier swaps and
/// removals. Never restores a removed runtime. Blocking.
pub fn recover(root: &Path) -> Result<()> {
    let next = staged_dir(root);
    let old = old_dir(root);
    if !root.exists() {
        if read_marker_blocking(&next).is_some() {
            std::fs::rename(&next, root).map_err(|e| Error::io(root, e))?;
            tracing::info!(root = %root.display(), "finished an interrupted runtime update");
        } else if old.exists() {
            std::fs::rename(&old, root).map_err(|e| Error::io(root, e))?;
            tracing::info!(root = %root.display(), "undid an interrupted runtime update");
        }
    }
    if root.exists() && old.exists() {
        let _ = std::fs::remove_dir_all(&old);
    }
    let removed = removed_dir(root);
    if removed.exists() {
        let _ = std::fs::remove_dir_all(&removed);
    }
    Ok(())
}

/// Total size of the files under `dir`. Blocking.
pub fn tree_bytes(dir: &Path) -> u64 {
    WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

/// Removes a launcher runtime (and a staged update of it). The caller makes sure nothing uses
/// it. Fails without changing anything while a program holds its files. Returns the bytes freed.
/// Blocking.
pub fn remove_runtime(root: &Path) -> Result<u64> {
    let removed = removed_dir(root);
    if removed.exists() {
        std::fs::remove_dir_all(&removed).map_err(|e| Error::io(&removed, e))?;
    }
    let mut freed = 0;
    if root.exists() {
        freed += tree_bytes(root);
        std::fs::rename(root, &removed).map_err(|e| Error::io(root, e))?;
    }
    for extra in [staged_dir(root), old_dir(root)] {
        if extra.exists() {
            freed += tree_bytes(&extra);
            if let Err(error) = std::fs::remove_dir_all(&extra) {
                tracing::debug!(%error, dir = %extra.display(), "removed later");
            }
        }
    }
    if let Err(error) = std::fs::remove_dir_all(&removed) {
        tracing::debug!(%error, "the removed runtime is deleted later");
    }
    // The component folder goes with its last runtime.
    if let Some(component_dir) = root.parent() {
        let _ = std::fs::remove_dir(component_dir);
    }
    Ok(freed)
}

/// When the launcher last asked Mojang for runtime updates (`meta/java-updates.json`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSchedule {
    /// The last check, successful or not (ms since the epoch).
    #[serde(default)]
    pub last_attempt: Option<i64>,
    /// The last check that reached Mojang (ms since the epoch).
    #[serde(default)]
    pub last_success: Option<i64>,
}

impl UpdateSchedule {
    /// Blocking; a missing or unreadable file is an empty schedule.
    pub fn load(dirs: &DataDirs) -> Self {
        std::fs::read(dirs.meta().join(SCHEDULE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Blocking.
    pub fn save(&self, dirs: &DataDirs) -> Result<()> {
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|e| Error::json("update schedule", e))?;
        write_atomic_blocking(&dirs.meta().join(SCHEDULE_FILE), &bytes)
    }

    /// Whether a check is due: a day after the last success, an hour after a failure. A clock
    /// that jumped far back makes it due too.
    pub fn due(&self, now_ms: i64) -> bool {
        let recent =
            |at: Option<i64>, window: i64| at.is_some_and(|at| (now_ms - at).abs() < window);
        !recent(self.last_success, CHECK_EVERY_MS) && !recent(self.last_attempt, RETRY_AFTER_MS)
    }

    pub fn record(&mut self, now_ms: i64, success: bool) {
        self.last_attempt = Some(now_ms);
        if success {
            self.last_success = Some(now_ms);
        }
    }
}

/// Runtimes by (component, platform), for lookups.
pub fn by_key(runtimes: &[ManagedRuntime]) -> BTreeMap<(String, String), &ManagedRuntime> {
    runtimes
        .iter()
        .map(|runtime| {
            (
                (runtime.component.clone(), runtime.platform.clone()),
                runtime,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
