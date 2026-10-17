//! Moving the data folder to another place (another disk, usually).
//!
//! The launcher finds a moved folder through a small pointer file that lives outside the data
//! folder (in the OS config folder, chosen by the app). A move is:
//!
//! 1. [`resolve_target`]: a `ProjectsLauncher` folder inside the folder the user picked (or the
//!    picked folder itself when it already has that name), new or empty. Never the current
//!    folder, never inside it, never a whole disk or a shared folder.
//! 2. [`copy_data_root`]: copy everything with progress and cancellation, flush every copied file
//!    to disk, then check that every file arrived with the same relative path and size (files
//!    that changed during the copy get one more pass). Links (symbolic links, and junctions on
//!    Windows, e.g. `saves/` kept on another disk) are made again in the copy pointing at the same
//!    place, never copied through. On any failure, a link that cannot be made included, the
//!    partial copy is removed and nothing else changes.
//! 3. [`switch_root`]: mark the old folder and point the launcher at the new one (old folder kept
//!    in `previous`, together with older folders whose deletion is still pending). The app
//!    restarts.
//! 4. [`finish_move`]: after the next start succeeded in the new place, delete the old folders,
//!    but only those whose marker leads to the new place, then forget them.

use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Deserializer, Serialize};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

use crate::error::{Error, Result, TargetProblem};
use crate::links;

/// Folder the data goes into, inside the folder the user picked.
pub const DEFAULT_FOLDER_NAME: &str = "ProjectsLauncher";
/// Written into the old folder by [`switch_root`]; [`finish_move`] only deletes folders with it.
pub const MOVED_MARKER: &str = ".projects-moved.json";
/// Free space kept on the target disk on top of the data itself.
pub const SPACE_MARGIN: u64 = 512 * 1024 * 1024;
const WRITE_PROBE: &str = ".projects-write-test";
const COPY_BUFFER: usize = 1024 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);
/// Copied files are flushed to disk by this many threads while the copy goes on (one flush per
/// file is slow on USB disks; several at once overlap).
const SYNC_WORKERS: usize = 8;
/// Copied files waiting for their flush, at most.
const SYNC_QUEUE: usize = 256;

/// The pointer file: where the data folder is, and the folders it was moved away from until
/// they are deleted (newest first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pointer {
    pub root: PathBuf,
    #[serde(
        default,
        deserialize_with = "one_or_many",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub previous: Vec<PathBuf>,
}

/// `previous` was a single path in the first pointer files.
fn one_or_many<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PathBuf>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(PathBuf),
        Many(Vec<PathBuf>),
    }
    Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::One(path)) => vec![path],
        Some(OneOrMany::Many(paths)) => paths,
    })
}

/// The pointer at `file`, if there is a readable one.
pub fn read_pointer(file: &Path) -> Option<Pointer> {
    let bytes = std::fs::read(file).ok()?;
    match serde_json::from_slice::<Pointer>(&bytes) {
        Ok(pointer) if pointer.root.is_absolute() => Some(pointer),
        Ok(_) => None,
        Err(error) => {
            tracing::warn!(path = %file.display(), %error, "ignoring an unreadable data folder pointer");
            None
        }
    }
}

/// Writes the pointer atomically (temporary file, then rename).
pub fn write_pointer(file: &Path, pointer: &Pointer) -> Result<()> {
    write_json_atomic(file, pointer)
}

/// The data folder the pointer at `file` selects: its `root`, when that folder exists (a
/// missing disk falls back to the default folder instead of an empty new one).
pub fn pointed_root(file: &Path) -> Option<PathBuf> {
    let pointer = read_pointer(file)?;
    if pointer.root.is_dir() {
        Some(pointer.root)
    } else {
        tracing::warn!(root = %pointer.root.display(), "the moved data folder is missing; using the default one");
        None
    }
}

/// Writes JSON atomically and durably: the temporary file is flushed to disk before it replaces
/// the old one (the pointer and the move markers decide which folder is deleted).
fn write_json_atomic(file: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let bytes = serde_json::to_vec_pretty(value)?;
    let written = std::fs::File::create(&tmp).and_then(|mut handle| {
        handle.write_all(&bytes)?;
        handle.sync_all()
    });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::io(&tmp, error));
    }
    if let Err(error) = std::fs::rename(&tmp, file) {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::io(file, error));
    }
    if let Some(parent) = file.parent() {
        pl_mc::paths::sync_dir(parent);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Paths

/// `path` made comparable: absolute, `.`/`..` resolved, the existing part canonicalized and the
/// case folded where file systems usually ignore it.
fn comparable(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    while !existing.exists() {
        match existing.file_name() {
            Some(name) => {
                rest.push(name.to_os_string());
                existing.pop();
            }
            None => break,
        }
    }
    let mut out = std::fs::canonicalize(&existing).unwrap_or(existing);
    for part in rest.iter().rev() {
        out.push(part);
    }
    let mut clean = PathBuf::new();
    for component in out.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other),
        }
    }
    if cfg!(any(windows, target_os = "macos")) {
        PathBuf::from(clean.to_string_lossy().to_lowercase())
    } else {
        clean
    }
}

/// Whether `a` and `b` name the same folder.
pub fn same_path(a: &Path, b: &Path) -> bool {
    comparable(a) == comparable(b)
}

/// Whether `inner` is strictly inside `outer`.
pub fn is_inside(inner: &Path, outer: &Path) -> bool {
    let (inner, outer) = (comparable(inner), comparable(outer));
    inner != outer && inner.starts_with(&outer)
}

fn is_empty_dir(path: &Path) -> std::io::Result<bool> {
    Ok(std::fs::read_dir(path)?.next().is_none())
}

/// Where the data folder goes when the user picks `picked`: a [`DEFAULT_FOLDER_NAME`] folder
/// inside it, or `picked` itself when it already has that name. The data never lands directly
/// in a disk's root or a folder shared with other things (moving it again later deletes the old
/// folder as a whole). The result must be new or empty.
pub fn resolve_target(current: &Path, picked: &Path) -> Result<PathBuf> {
    if !picked.is_absolute() {
        return Err(Error::target(picked, TargetProblem::NotAbsolute));
    }
    if same_path(picked, current) {
        return Err(Error::target(picked, TargetProblem::Same));
    }
    if picked.exists() && !picked.is_dir() {
        return Err(Error::target(picked, TargetProblem::NotAFolder));
    }
    let named_like_ours = picked.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .eq_ignore_ascii_case(DEFAULT_FOLDER_NAME)
    });
    let target = if named_like_ours {
        picked.to_path_buf()
    } else {
        picked.join(DEFAULT_FOLDER_NAME)
    };
    if same_path(&target, current) {
        return Err(Error::target(&target, TargetProblem::Same));
    }
    if is_inside(&target, current) {
        return Err(Error::target(&target, TargetProblem::InsideCurrent));
    }
    if is_inside(current, &target) {
        return Err(Error::target(&target, TargetProblem::ContainsCurrent));
    }
    if target.exists() {
        if !target.is_dir() {
            return Err(Error::target(&target, TargetProblem::NotAFolder));
        }
        if !is_empty_dir(&target).map_err(|e| Error::io(&target, e))? {
            return Err(Error::target(&target, TargetProblem::NotEmpty));
        }
    }
    Ok(target)
}

// ---------------------------------------------------------------------------------------------
// Scan, copy, verify

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
}

/// One thing under the data folder, by path relative to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub rel: PathBuf,
    pub kind: EntryKind,
    pub size: u64,
}

/// Everything under a folder, parents before children.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    pub entries: Vec<Entry>,
    pub files: u64,
    pub bytes: u64,
}

fn skipped(rel: &Path) -> bool {
    rel == Path::new(MOVED_MARKER) || rel == Path::new(WRITE_PROBE)
}

/// Lists everything under `root` (links are not followed). The move markers are left out.
pub fn scan(root: &Path, cancel: &CancellationToken) -> Result<Manifest> {
    let mut manifest = Manifest::default();
    for entry in WalkDir::new(root).follow_links(false).min_depth(1) {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let entry = entry.map_err(|e| {
            let path = e.path().unwrap_or(root).to_path_buf();
            Error::io(&path, e.into())
        })?;
        let rel = entry
            .path()
            .strip_prefix(root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| entry.path().to_path_buf());
        if skipped(&rel) {
            continue;
        }
        let kind = entry.file_type();
        let (kind, size) = if kind.is_symlink() {
            (EntryKind::Symlink, 0)
        } else if kind.is_dir() {
            (EntryKind::Dir, 0)
        } else {
            let size = entry
                .metadata()
                .map_err(|e| Error::io(entry.path(), e.into()))?
                .len();
            (EntryKind::File, size)
        };
        if kind == EntryKind::File {
            manifest.files += 1;
            manifest.bytes += size;
        }
        manifest.entries.push(Entry { rel, kind, size });
    }
    Ok(manifest)
}

/// Copy progress.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CopyProgress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_files: u64,
    pub total_files: u64,
}

/// What a move is doing right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveStage {
    Scanning,
    Copying,
    Verifying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveProgress {
    pub stage: MoveStage,
    pub copy: CopyProgress,
}

pub type ProgressFn = Arc<dyn Fn(MoveProgress) + Send + Sync>;

/// Flushes copied files to disk on worker threads while the copy goes on, so a copy is durable
/// before the launcher switches to it (the old folder is deleted soon after).
struct Syncer {
    sender: Option<SyncSender<(PathBuf, std::fs::File)>>,
    workers: Vec<std::thread::JoinHandle<()>>,
    failure: Arc<Mutex<Option<Error>>>,
}

impl Syncer {
    fn start() -> Self {
        let (sender, receiver) = sync_channel::<(PathBuf, std::fs::File)>(SYNC_QUEUE);
        let receiver = Arc::new(Mutex::new(receiver));
        let failure: Arc<Mutex<Option<Error>>> = Arc::new(Mutex::new(None));
        let workers = (0..SYNC_WORKERS)
            .map(|_| {
                let receiver = Arc::clone(&receiver);
                let failure = Arc::clone(&failure);
                std::thread::spawn(move || {
                    loop {
                        // The lock is held only while waiting for the next file.
                        let next = receiver
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .recv();
                        let Ok((path, file)) = next else { break };
                        if let Err(error) = file.sync_all() {
                            let mut slot = failure
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            slot.get_or_insert(Error::io(&path, error));
                        }
                    }
                })
            })
            .collect();
        Self {
            sender: Some(sender),
            workers,
            failure,
        }
    }

    fn failed(&self) -> bool {
        self.failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// Queues a written file for its flush (waits while the queue is full).
    fn push(&self, path: PathBuf, file: std::fs::File) {
        if let Some(sender) = &self.sender {
            let _ = sender.send((path, file));
        }
    }

    /// Waits for every queued flush; the first failure, if any.
    fn finish(mut self) -> Result<()> {
        self.sender = None;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        match self
            .failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// Makes a link: `(link, target, folder)`. [`links::create`], except in tests.
type LinkMaker = fn(&Path, &Path, bool) -> std::io::Result<()>;

struct Copier<'a> {
    cancel: &'a CancellationToken,
    progress: &'a dyn Fn(CopyProgress),
    state: CopyProgress,
    last_report: Option<Instant>,
    buffer: Vec<u8>,
    syncer: &'a Syncer,
    make_link: LinkMaker,
}

impl Copier<'_> {
    fn report(&mut self, force: bool) {
        let due = self
            .last_report
            .is_none_or(|last| last.elapsed() >= PROGRESS_INTERVAL);
        if force || due {
            self.last_report = Some(Instant::now());
            (self.progress)(self.state);
        }
    }

    fn check(&self) -> Result<()> {
        if self.cancel.is_cancelled() {
            Err(Error::Cancelled)
        } else if self.syncer.failed() {
            // A flush failed (the disk is gone or full): stop early, `finish` reports it.
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Copies one file in chunks (so a cancel or the progress never waits for a big file),
    /// keeping its modification time and permissions, and hands it to the syncer for its flush.
    /// `counted`: its bytes are part of the progress total. `Ok(false)`: it vanished meanwhile.
    fn copy_file(&mut self, from: &Path, to: &Path, counted: bool) -> Result<bool> {
        let mut input = match std::fs::File::open(from) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(Error::io(from, e)),
        };
        let meta = input.metadata().map_err(|e| Error::io(from, e))?;
        let mut output = std::fs::File::create(to).map_err(|e| Error::io(to, e))?;
        loop {
            self.check()?;
            let read = input
                .read(&mut self.buffer)
                .map_err(|e| Error::io(from, e))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&self.buffer[..read])
                .map_err(|e| Error::io(to, e))?;
            if counted {
                self.state.done_bytes += read as u64;
                self.report(false);
            }
        }
        if let Ok(modified) = meta.modified() {
            let _ = output.set_modified(modified);
        }
        // Every file is flushed before the switch (the old folder is deleted after the next
        // start). A flush costs ~60 ms on a USB disk, so the syncer's threads do it while the
        // copy goes on.
        self.syncer.push(to.to_path_buf(), output);
        let _ = std::fs::set_permissions(to, meta.permissions());
        Ok(true)
    }
}

/// Makes the link `rel` of `src` again in `dst`, pointing at the same place (see
/// [`links::target_in_copy`]); its target is never copied (it may be a big folder on another
/// disk). The one exception: on Windows without the right to make symbolic links, a file link is
/// copied as the file it points to. A link that cannot be made stops the move ([`Error::Link`]).
fn copy_link(src: &Path, dst: &Path, rel: &Path, copier: &mut Copier<'_>) -> Result<()> {
    let from = src.join(rel);
    let to = dst.join(rel);
    let folder = links::is_folder_link(&from);
    let stored = std::fs::read_link(&from).map_err(|e| Error::link(&from, folder, e))?;
    let target = links::target_in_copy(src, dst, rel, &stored);
    // A second pass (the link changed during the copy) replaces what the first one made.
    links::remove(&to).map_err(|e| Error::link(&to, folder, e))?;
    match (copier.make_link)(&to, &target, folder) {
        Ok(()) => Ok(()),
        Err(error)
            if !folder
                && links::needs_privilege(&error)
                && std::fs::metadata(&from).is_ok_and(|meta| meta.is_file()) =>
        {
            tracing::info!(path = %from.display(), "copying a file link as its file (no right to make links)");
            copier.copy_file(&from, &to, false).map(|_| ())
        }
        Err(error) => {
            tracing::warn!(path = %from.display(), %error, "a link of the data folder cannot be made in the copy");
            Err(Error::link(&to, folder, error))
        }
    }
}

/// Copies the entries of `manifest` from `src` to `dst` (which exists) and flushes every copied
/// file to disk before returning. Returns the relative paths of files that vanished from `src`
/// during the copy.
pub fn copy_entries(
    src: &Path,
    dst: &Path,
    manifest: &Manifest,
    cancel: &CancellationToken,
    progress: &dyn Fn(CopyProgress),
) -> Result<Vec<PathBuf>> {
    copy_entries_with(src, dst, manifest, cancel, progress, links::create)
}

fn copy_entries_with(
    src: &Path,
    dst: &Path,
    manifest: &Manifest,
    cancel: &CancellationToken,
    progress: &dyn Fn(CopyProgress),
    make_link: LinkMaker,
) -> Result<Vec<PathBuf>> {
    let syncer = Syncer::start();
    let copied = copy_with(src, dst, manifest, cancel, progress, &syncer, make_link);
    let flushed = syncer.finish();
    match (copied, flushed) {
        // A failed flush stops the copy early; the flush error is the real cause.
        (Err(Error::Cancelled), Err(flush)) if !cancel.is_cancelled() => Err(flush),
        (Err(error), _) => Err(error),
        (Ok(_), Err(flush)) => Err(flush),
        (Ok(vanished), Ok(())) => {
            // New folder entries must be durable too where the OS needs that (Unix).
            if cfg!(unix) {
                for entry in &manifest.entries {
                    if entry.kind == EntryKind::Dir {
                        pl_mc::paths::sync_dir(&dst.join(&entry.rel));
                    }
                }
                pl_mc::paths::sync_dir(dst);
            }
            Ok(vanished)
        }
    }
}

fn copy_with(
    src: &Path,
    dst: &Path,
    manifest: &Manifest,
    cancel: &CancellationToken,
    progress: &dyn Fn(CopyProgress),
    syncer: &Syncer,
    make_link: LinkMaker,
) -> Result<Vec<PathBuf>> {
    let mut copier = Copier {
        cancel,
        progress,
        state: CopyProgress {
            total_bytes: manifest.bytes,
            total_files: manifest.files,
            ..CopyProgress::default()
        },
        last_report: None,
        buffer: vec![0; COPY_BUFFER],
        syncer,
        make_link,
    };
    copier.report(true);
    let mut vanished = Vec::new();
    for entry in &manifest.entries {
        copier.check()?;
        let from = src.join(&entry.rel);
        let to = dst.join(&entry.rel);
        match entry.kind {
            EntryKind::Dir => std::fs::create_dir_all(&to).map_err(|e| Error::io(&to, e))?,
            EntryKind::File => {
                if !copier.copy_file(&from, &to, true)? {
                    vanished.push(entry.rel.clone());
                }
                copier.state.done_files += 1;
                copier.report(false);
            }
            EntryKind::Symlink => copy_link(src, dst, &entry.rel, &mut copier)?,
        }
    }
    copier.report(true);
    Ok(vanished)
}

/// The running launcher's own log (`logs/launcher.<date>.log`): appended to all the time, so a
/// copy only needs to have it (see [`verify`]).
pub fn is_live_launcher_log(rel: &Path) -> bool {
    rel.parent() == Some(Path::new(crate::usage::LOGS_DIR))
        && rel
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("launcher."))
}

/// Whether the copy of the link `rel` arrived: a link leading where [`links::target_in_copy`]
/// says, or (Windows without the right to make file links) a copy of the file it points to.
fn link_arrived(src: &Path, dst: &Path, rel: &Path) -> bool {
    let (from, to) = (src.join(rel), dst.join(rel));
    let Ok(meta) = std::fs::symlink_metadata(&to) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        let (Ok(stored), Ok(made)) = (std::fs::read_link(&from), std::fs::read_link(&to)) else {
            return false;
        };
        let wanted = links::target_in_copy(src, dst, rel, &stored);
        links::same_target(&to, &wanted, &made)
    } else {
        cfg!(windows)
            && meta.is_file()
            && std::fs::metadata(&from).is_ok_and(|file| file.is_file() && file.len() == meta.len())
    }
}

/// Relative paths of files under `src` that are missing from `dst` or have another size there,
/// and of links that were not made again there. `live` files (written to all the time, like the
/// running launcher's own log) only need to exist.
pub fn verify(
    src: &Path,
    dst: &Path,
    live: &dyn Fn(&Path) -> bool,
    cancel: &CancellationToken,
) -> Result<Vec<PathBuf>> {
    let mut differ = Vec::new();
    for entry in scan(src, cancel)?.entries {
        let copy = dst.join(&entry.rel);
        let ok = match entry.kind {
            EntryKind::Dir => copy.is_dir(),
            EntryKind::Symlink => link_arrived(src, dst, &entry.rel),
            EntryKind::File => match std::fs::metadata(&copy) {
                Ok(meta) => meta.is_file() && (meta.len() == entry.size || live(&entry.rel)),
                Err(_) => false,
            },
        };
        if !ok {
            differ.push(entry.rel);
        }
    }
    Ok(differ)
}

/// Removes a partial copy: the whole `target` when the move created it, otherwise only what is
/// inside (it was empty before).
pub fn discard_copy(target: &Path, created: bool) {
    let result = if created {
        remove_dir_all_robust(target)
    } else {
        std::fs::read_dir(target).and_then(|entries| {
            for entry in entries.filter_map(std::result::Result::ok) {
                let path = entry.path();
                let kind = entry.file_type();
                if kind.as_ref().is_ok_and(std::fs::FileType::is_symlink) {
                    // A link the copy made: only the link goes, never what it points at.
                    links::remove(&path)?;
                } else if kind.is_ok_and(|t| t.is_dir()) {
                    remove_dir_all_robust(&path)?;
                } else {
                    remove_file_robust(&path)?;
                }
            }
            Ok(())
        })
    };
    if let Err(error) = result {
        tracing::warn!(target = %target.display(), %error, "could not remove the partial copy");
    }
}

/// Windows refuses to delete read-only files; elsewhere the folder's permissions decide.
#[cfg(windows)]
fn clear_readonly(path: &Path) {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        let mut permissions = meta.permissions();
        if permissions.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
}

#[cfg(not(windows))]
fn clear_readonly(_path: &Path) {}

fn remove_file_robust(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            clear_readonly(path);
            std::fs::remove_file(path)
        }
        other => other,
    }
}

/// `remove_dir_all` that also deletes read-only files (Windows refuses those otherwise).
pub fn remove_dir_all_robust(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(first) => {
            for entry in WalkDir::new(dir)
                .follow_links(false)
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                clear_readonly(entry.path());
            }
            std::fs::remove_dir_all(dir).map_err(|_| first)
        }
    }
}

/// A finished copy, waiting for [`switch_root`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyReport {
    pub target: PathBuf,
    pub files: u64,
    pub bytes: u64,
    /// The move created `target` (so [`discard_copy`] removes it entirely).
    pub created_target: bool,
}

/// Copies the data folder `current` into `target` (from [`resolve_target`]) and verifies the
/// copy. `live` names files that may grow during the copy. On error or cancel the partial copy
/// is removed and `current` is untouched.
pub fn copy_data_root(
    current: &Path,
    target: &Path,
    cancel: &CancellationToken,
    progress: ProgressFn,
    live: &dyn Fn(&Path) -> bool,
) -> Result<CopyReport> {
    copy_data_root_with(current, target, cancel, progress, live, links::create)
}

fn copy_data_root_with(
    current: &Path,
    target: &Path,
    cancel: &CancellationToken,
    progress: ProgressFn,
    live: &dyn Fn(&Path) -> bool,
    make_link: LinkMaker,
) -> Result<CopyReport> {
    let created_target = !target.exists();
    std::fs::create_dir_all(target).map_err(|e| writable_error(target, e))?;
    let result = copy_and_verify(current, target, cancel, &progress, live, make_link);
    match result {
        Ok((files, bytes)) => Ok(CopyReport {
            target: target.to_path_buf(),
            files,
            bytes,
            created_target,
        }),
        Err(error) => {
            discard_copy(target, created_target);
            Err(error)
        }
    }
}

fn writable_error(path: &Path, error: std::io::Error) -> Error {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem => {
            Error::target(path, TargetProblem::NotWritable)
        }
        _ => Error::io(path, error),
    }
}

fn copy_and_verify(
    current: &Path,
    target: &Path,
    cancel: &CancellationToken,
    progress: &ProgressFn,
    live: &dyn Fn(&Path) -> bool,
    make_link: LinkMaker,
) -> Result<(u64, u64)> {
    let probe = target.join(WRITE_PROBE);
    std::fs::write(&probe, b"ok").map_err(|e| writable_error(target, e))?;
    let _ = std::fs::remove_file(&probe);

    progress(MoveProgress {
        stage: MoveStage::Scanning,
        copy: CopyProgress::default(),
    });
    let manifest = scan(current, cancel)?;
    ensure_space(manifest.bytes, crate::usage::free_space(target))?;

    let on_copy = |copy: CopyProgress| {
        progress(MoveProgress {
            stage: MoveStage::Copying,
            copy,
        })
    };
    copy_entries_with(current, target, &manifest, cancel, &on_copy, make_link)?;
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }

    let verifying = |copy: CopyProgress| {
        progress(MoveProgress {
            stage: MoveStage::Verifying,
            copy,
        })
    };
    let done = CopyProgress {
        done_bytes: manifest.bytes,
        total_bytes: manifest.bytes,
        done_files: manifest.files,
        total_files: manifest.files,
    };
    verifying(done);
    let mut differ = verify(current, target, live, cancel)?;
    if !differ.is_empty() {
        // Files written during the copy (a cache refresh, a log): one more pass for those.
        tracing::info!(
            count = differ.len(),
            "copying files that changed during the move again"
        );
        let again = Manifest {
            entries: differ
                .iter()
                .flat_map(|rel| {
                    let parent = rel.parent().map(Path::to_path_buf).unwrap_or_default();
                    let file = std::fs::symlink_metadata(current.join(rel)).ok();
                    let dir_entry = Entry {
                        rel: parent,
                        kind: EntryKind::Dir,
                        size: 0,
                    };
                    let entry = file.map(|meta| Entry {
                        rel: rel.clone(),
                        kind: if meta.file_type().is_symlink() {
                            EntryKind::Symlink
                        } else if meta.is_dir() {
                            EntryKind::Dir
                        } else {
                            EntryKind::File
                        },
                        size: meta.len(),
                    });
                    std::iter::once(dir_entry).chain(entry)
                })
                .collect(),
            files: 0,
            bytes: 0,
        };
        copy_entries_with(current, target, &again, cancel, &|_| {}, make_link)?;
        differ = verify(current, target, live, cancel)?;
        verifying(done);
    }
    if let Some(first) = differ.first() {
        return Err(Error::Mismatch {
            count: differ.len(),
            first: first.display().to_string(),
        });
    }
    if cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    Ok((manifest.files, manifest.bytes))
}

/// Space a copy of `bytes` needs on the target disk: the data plus [`SPACE_MARGIN`]. An unknown
/// amount of free space does not block the move (the copy itself fails cleanly when full).
pub fn ensure_space(bytes: u64, available: Option<u64>) -> Result<()> {
    let needed = bytes.saturating_add(SPACE_MARGIN);
    match available {
        Some(available) if available < needed => Err(Error::NotEnoughSpace { needed, available }),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------------------------
// Switch and finish

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovedMarker {
    moved_to: PathBuf,
}

/// Records a finished copy: marks `current` as moved to `target` and points the launcher at
/// `target`, keeping `current` in `previous` until [`finish_move`] deletes it. Folders an earlier
/// move left waiting for deletion (a second move before the first one finished) stay in
/// `previous` too: their markers lead to `current`, whose marker leads to `target`. When this
/// fails nothing changed (the caller removes the copy).
pub fn switch_root(pointer_file: &Path, current: &Path, target: &Path) -> Result<()> {
    let waiting: Vec<PathBuf> = read_pointer(pointer_file)
        .filter(|pointer| same_path(&pointer.root, current))
        .map(|pointer| pointer.previous)
        .unwrap_or_default()
        .into_iter()
        .filter(|old| old.exists() && !overlaps(old, current) && !overlaps(old, target))
        .collect();
    let marker = current.join(MOVED_MARKER);
    write_json_atomic(
        &marker,
        &MovedMarker {
            moved_to: target.to_path_buf(),
        },
    )?;
    let mut previous = vec![current.to_path_buf()];
    previous.extend(waiting);
    let pointer = Pointer {
        root: target.to_path_buf(),
        previous,
    };
    if let Err(error) = write_pointer(pointer_file, &pointer) {
        let _ = std::fs::remove_file(&marker);
        return Err(error);
    }
    Ok(())
}

/// Whether two folders are the same or one is inside the other.
fn overlaps(a: &Path, b: &Path) -> bool {
    same_path(a, b) || is_inside(a, b) || is_inside(b, a)
}

/// An old folder [`finish_move`] did not delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftBehind {
    pub folder: PathBuf,
    pub reason: &'static str,
}

/// What [`finish_move`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishOutcome {
    /// No move waiting to be finished.
    Nothing,
    /// The launcher did not start from the pointer's folder (a development override, a missing
    /// disk): nothing was touched; try again at a later start.
    NotStartedThere,
    /// Old folders were handled: `deleted` are gone, `left` stay (forgotten unless `retry`).
    Finished {
        deleted: Vec<PathBuf>,
        left: Vec<LeftBehind>,
        /// Some folders could not be deleted yet (in use) and stay in the pointer.
        retry: bool,
    },
}

/// After a start in `current_root`: deletes the folders the data was moved away from and clears
/// them from the pointer. A folder is deleted only when the launcher really runs from the
/// pointer's folder and the folder's [`MOVED_MARKER`] leads to `current_root` (directly, or
/// through the newer old folders of a second move). Disk roots and personal folders are never
/// deleted. Folders that cannot be deleted yet stay in the pointer for the next start. When the
/// pointer then names `default_root` with nothing left, the pointer file itself goes.
pub fn finish_move(
    pointer_file: &Path,
    current_root: &Path,
    default_root: Option<&Path>,
) -> Result<FinishOutcome> {
    let Some(pointer) = read_pointer(pointer_file) else {
        return Ok(FinishOutcome::Nothing);
    };
    if pointer.previous.is_empty() {
        return Ok(FinishOutcome::Nothing);
    }
    if !same_path(&pointer.root, current_root) {
        return Ok(FinishOutcome::NotStartedThere);
    }
    let marker_of = |folder: &Path| {
        std::fs::read(folder.join(MOVED_MARKER))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<MovedMarker>(&bytes).ok())
    };
    // Judge every folder before deleting any (a chain is checked through its newer links).
    let leads_home = |start: &Path| -> bool {
        let mut at = start.to_path_buf();
        for _ in 0..=pointer.previous.len() {
            let Some(marker) = marker_of(&at) else {
                return false;
            };
            if same_path(&marker.moved_to, current_root) {
                return true;
            }
            if !pointer
                .previous
                .iter()
                .any(|other| same_path(other, &marker.moved_to))
            {
                return false;
            }
            at = marker.moved_to;
        }
        false
    };
    let verdicts: Vec<(PathBuf, std::result::Result<(), &'static str>)> = pointer
        .previous
        .iter()
        .map(|folder| {
            let verdict = if overlaps(folder, current_root) {
                Err("the old folder overlaps the current one")
            } else if is_protected(folder) {
                Err("the old folder is a disk or a personal folder")
            } else if !folder.exists() {
                Err("the old folder is already gone")
            } else if !leads_home(folder) {
                Err("the old folder has no matching move marker")
            } else {
                Ok(())
            };
            (folder.clone(), verdict)
        })
        .collect();

    let mut deleted = Vec::new();
    let mut left = Vec::new();
    let mut waiting = Vec::new();
    for (folder, verdict) in verdicts {
        match verdict {
            Ok(()) => match remove_dir_all_robust(&folder) {
                Ok(()) => deleted.push(folder),
                Err(error) => {
                    tracing::warn!(previous = %folder.display(), %error, "could not delete an old data folder yet");
                    left.push(LeftBehind {
                        folder: folder.clone(),
                        reason: "the old folder could not be deleted (in use?)",
                    });
                    waiting.push(folder);
                }
            },
            Err(reason) => left.push(LeftBehind { folder, reason }),
        }
    }
    let retry = !waiting.is_empty();
    settle_pointer(pointer_file, &pointer.root, waiting, default_root)?;
    Ok(FinishOutcome::Finished {
        deleted,
        left,
        retry,
    })
}

/// Folders never deleted as a whole, whatever a pointer says: disk roots and the user's own
/// folders.
fn is_protected(path: &Path) -> bool {
    if comparable(path).parent().is_none() {
        return true;
    }
    [
        dirs::home_dir(),
        dirs::desktop_dir(),
        dirs::document_dir(),
        dirs::download_dir(),
        dirs::data_dir(),
        dirs::data_local_dir(),
        dirs::config_dir(),
    ]
    .into_iter()
    .flatten()
    .any(|personal| same_path(&personal, path))
}

/// The pointer with only the old folders still `waiting`; no pointer at all when it names the
/// default folder and nothing waits.
fn settle_pointer(
    file: &Path,
    root: &Path,
    waiting: Vec<PathBuf>,
    default_root: Option<&Path>,
) -> Result<()> {
    if waiting.is_empty() && default_root.is_some_and(|default| same_path(default, root)) {
        return match std::fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::io(file, e)),
        };
    }
    write_pointer(
        file,
        &Pointer {
            root: root.to_path_buf(),
            previous: waiting,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;

    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// A small data folder with nested folders, an empty folder and a bigger file.
    fn sample_root(root: &Path) {
        write(&root.join("accounts.json"), b"{\"accounts\":[]}");
        write(&root.join("instances/projects/instance.json"), b"{}");
        write(
            &root.join("instances/projects/minecraft/mods/a.jar"),
            &[7u8; 3000],
        );
        write(
            &root.join("assets/objects/ab/abcdef"),
            &vec![1u8; 2 * COPY_BUFFER + 17],
        );
        write(&root.join("logs/launcher.2026-10-15.log"), b"line\n");
        fs::create_dir_all(root.join("instances/projects/minecraft/screenshots")).unwrap();
    }

    fn no_progress() -> ProgressFn {
        Arc::new(|_| {})
    }

    fn never() -> CancellationToken {
        CancellationToken::new()
    }

    fn never_live(_: &Path) -> bool {
        false
    }

    #[test]
    fn resolves_targets() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("data");
        sample_root(&current);
        let empty = dir.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        let busy = dir.path().join("busy");
        write(&busy.join("other.txt"), b"x");
        let file = dir.path().join("file.txt");
        write(&file, b"x");

        // Always a folder of our own inside the picked one...
        assert_eq!(
            resolve_target(&current, &empty).unwrap(),
            empty.join(DEFAULT_FOLDER_NAME)
        );
        assert_eq!(
            resolve_target(&current, &busy).unwrap(),
            busy.join(DEFAULT_FOLDER_NAME)
        );
        let new = dir.path().join("new/folder");
        assert_eq!(
            resolve_target(&current, &new).unwrap(),
            new.join(DEFAULT_FOLDER_NAME)
        );
        // ...unless the picked folder already is one (new or empty).
        let named = dir.path().join("games/projectslauncher");
        assert_eq!(resolve_target(&current, &named).unwrap(), named);
        // The parent of the data folder works too.
        assert_eq!(
            resolve_target(&current, dir.path()).unwrap(),
            dir.path().join(DEFAULT_FOLDER_NAME)
        );

        let problem = |picked: &Path| match resolve_target(&current, picked) {
            Err(Error::Target { problem, .. }) => problem,
            other => panic!("expected a target problem, got {other:?}"),
        };
        assert_eq!(problem(&current), TargetProblem::Same);
        assert_eq!(
            problem(&current.join("instances")),
            TargetProblem::InsideCurrent
        );
        assert_eq!(
            problem(&current.join("new-sub")),
            TargetProblem::InsideCurrent
        );
        assert_eq!(problem(&file), TargetProblem::NotAFolder);
        assert_eq!(
            problem(Path::new("relative/dir")),
            TargetProblem::NotAbsolute
        );
        write(&busy.join(DEFAULT_FOLDER_NAME).join("x"), b"x");
        assert_eq!(problem(&busy), TargetProblem::NotEmpty);
        // `..` and case differences do not hide the current folder.
        let sneaky = current.join("instances").join("..");
        assert_eq!(problem(&sneaky), TargetProblem::Same);
        if cfg!(windows) {
            let upper = PathBuf::from(current.to_string_lossy().to_uppercase());
            assert_eq!(problem(&upper), TargetProblem::Same);
        }
        // A data folder named like ours is matched through its parent as well.
        let ours = dir.path().join("x").join(DEFAULT_FOLDER_NAME);
        sample_root(&ours);
        assert!(matches!(
            resolve_target(&ours, &dir.path().join("x")),
            Err(Error::Target {
                problem: TargetProblem::Same,
                ..
            })
        ));
    }

    #[test]
    fn only_the_launcher_log_is_live() {
        assert!(is_live_launcher_log(Path::new(
            "logs/launcher.2026-10-15.log"
        )));
        assert!(!is_live_launcher_log(Path::new(
            "logs/game-x-20261015-120000.log"
        )));
        assert!(!is_live_launcher_log(Path::new(
            "instances/x/minecraft/logs/launcher.log"
        )));
    }

    #[test]
    fn space_check_keeps_a_margin() {
        assert!(ensure_space(1000, None).is_ok());
        assert!(ensure_space(1000, Some(1000 + SPACE_MARGIN)).is_ok());
        assert!(matches!(
            ensure_space(1000, Some(SPACE_MARGIN)),
            Err(Error::NotEnoughSpace { needed, available })
                if needed == 1000 + SPACE_MARGIN && available == SPACE_MARGIN
        ));
        assert!(ensure_space(u64::MAX, Some(u64::MAX - 1)).is_err());
    }

    #[test]
    fn copies_and_verifies_with_progress() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("data");
        sample_root(&current);
        write(&current.join(MOVED_MARKER), b"{\"movedTo\":\"x\"}");
        let target = resolve_target(&current, &dir.path().join("moved")).unwrap();

        let seen: Arc<Mutex<Vec<MoveProgress>>> = Arc::default();
        let sink = Arc::clone(&seen);
        let progress: ProgressFn = Arc::new(move |p| sink.lock().unwrap().push(p));
        let report = copy_data_root(
            &current,
            &target,
            &CancellationToken::new(),
            progress,
            &never_live,
        )
        .unwrap();

        assert!(report.created_target);
        assert_eq!(report.files, 5);
        assert_eq!(
            report.bytes,
            15 + 2 + 3000 + (2 * COPY_BUFFER as u64 + 17) + 5
        );
        assert!(
            verify(&current, &target, &never_live, &never())
                .unwrap()
                .is_empty()
        );
        assert!(
            target
                .join("instances/projects/minecraft/screenshots")
                .is_dir()
        );
        assert!(
            !target.join(MOVED_MARKER).exists(),
            "markers are not copied"
        );
        assert!(!target.join(WRITE_PROBE).exists());
        assert_eq!(
            fs::read(target.join("assets/objects/ab/abcdef")).unwrap(),
            fs::read(current.join("assets/objects/ab/abcdef")).unwrap()
        );
        let original = fs::metadata(current.join("instances/projects/minecraft/mods/a.jar"))
            .unwrap()
            .modified()
            .unwrap();
        let copied = fs::metadata(target.join("instances/projects/minecraft/mods/a.jar"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(original, copied, "modification times are kept");

        let seen = seen.lock().unwrap();
        assert_eq!(seen.first().unwrap().stage, MoveStage::Scanning);
        let last_copy = seen
            .iter()
            .rev()
            .find(|p| p.stage == MoveStage::Copying)
            .unwrap();
        assert_eq!(last_copy.copy.done_bytes, report.bytes);
        assert_eq!(last_copy.copy.done_files, report.files);
        assert_eq!(seen.last().unwrap().stage, MoveStage::Verifying);
        // Progress only grows.
        let copying: Vec<_> = seen
            .iter()
            .filter(|p| p.stage == MoveStage::Copying)
            .collect();
        assert!(
            copying
                .windows(2)
                .all(|w| w[0].copy.done_bytes <= w[1].copy.done_bytes)
        );
    }

    #[test]
    fn verify_finds_missing_and_resized_files() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("data");
        sample_root(&current);
        let target = dir.path().join("copy");
        fs::create_dir_all(&target).unwrap();
        let manifest = scan(&current, &never()).unwrap();
        copy_entries(&current, &target, &manifest, &never(), &|_| {}).unwrap();
        assert!(
            verify(&current, &target, &never_live, &never())
                .unwrap()
                .is_empty()
        );

        fs::remove_file(target.join("accounts.json")).unwrap();
        write(
            &target.join("instances/projects/minecraft/mods/a.jar"),
            b"short",
        );
        write(
            &current.join("logs/launcher.2026-10-15.log"),
            b"line\nmore lines\n",
        );
        let mut differ = verify(&current, &target, &never_live, &never()).unwrap();
        differ.sort();
        assert_eq!(
            differ,
            [
                PathBuf::from("accounts.json"),
                PathBuf::from("instances/projects/minecraft/mods/a.jar"),
                PathBuf::from("logs/launcher.2026-10-15.log"),
            ]
        );
        let live = |rel: &Path| rel.starts_with("logs");
        assert_eq!(verify(&current, &target, &live, &never()).unwrap().len(), 2);
    }

    #[test]
    fn cancel_and_errors_remove_the_partial_copy() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("data");
        sample_root(&current);

        // Cancelled during the copy: the new folder is gone, the data untouched.
        let target = dir.path().join("moved");
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let progress: ProgressFn = Arc::new(move |p| {
            if p.stage == MoveStage::Copying && p.copy.done_bytes > 0 {
                trigger.cancel();
            }
        });
        let result = copy_data_root(&current, &target, &cancel, progress, &never_live);
        assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
        assert!(!target.exists());
        assert_eq!(scan(&current, &never()).unwrap().files, 5);
        assert!(current.join("accounts.json").is_file());

        // An empty folder the user picked stays (empty) after a failure.
        let picked = dir.path().join("picked");
        fs::create_dir_all(&picked).unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = copy_data_root(&current, &picked, &cancel, no_progress(), &never_live);
        assert!(result.unwrap_err().is_cancelled());
        assert!(picked.is_dir());
        assert_eq!(fs::read_dir(&picked).unwrap().count(), 0);
    }

    #[test]
    fn files_changed_during_the_copy_get_another_pass() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("data");
        sample_root(&current);
        let target = dir.path().join("moved");
        // The first progress report of the copy stage writes to a file that is not copied yet.
        let source = current.clone();
        let progress: ProgressFn = Arc::new(move |p| {
            if p.stage == MoveStage::Copying && p.copy.done_files == 0 {
                let _ = fs::write(source.join("cache.json"), b"written during the move");
            }
        });
        let report = copy_data_root(
            &current,
            &target,
            &CancellationToken::new(),
            progress,
            &never_live,
        )
        .unwrap();
        assert_eq!(
            fs::read(report.target.join("cache.json")).unwrap(),
            b"written during the move"
        );
    }

    #[test]
    fn pointer_round_trip_and_switch() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config/gg.projects.launcher/location.json");
        let current = dir.path().join("data");
        sample_root(&current);
        let target = dir.path().join("moved");
        fs::create_dir_all(&target).unwrap();

        assert_eq!(read_pointer(&config), None);
        assert_eq!(pointed_root(&config), None);
        switch_root(&config, &current, &target).unwrap();
        let pointer = read_pointer(&config).unwrap();
        assert_eq!(pointer.root, target);
        assert_eq!(pointer.previous, vec![current.clone()]);
        assert_eq!(pointed_root(&config), Some(target.clone()));
        assert!(current.join(MOVED_MARKER).is_file());
        assert!(!config.with_extension("json.tmp").exists());

        // A pointer to a folder that is gone (unplugged disk) is ignored, not followed.
        write_pointer(
            &config,
            &Pointer {
                root: dir.path().join("unplugged"),
                previous: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(pointed_root(&config), None);
        fs::write(&config, b"not json").unwrap();
        assert_eq!(read_pointer(&config), None);

        // The first pointer files kept one old folder as a plain path.
        let old_style = serde_json::json!({ "root": target, "previous": current });
        fs::write(&config, serde_json::to_vec(&old_style).unwrap()).unwrap();
        assert_eq!(read_pointer(&config).unwrap().previous, vec![current]);
    }

    #[test]
    fn two_moves_in_a_row_leave_no_old_folder_behind() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("location.json");
        let first = dir.path().join("first");
        sample_root(&first);
        let move_to = |from: &Path, picked: &str| {
            let report = copy_data_root(
                from,
                &dir.path().join(picked),
                &CancellationToken::new(),
                no_progress(),
                &never_live,
            )
            .unwrap();
            switch_root(&config, from, &report.target).unwrap();
            report.target
        };
        let second = move_to(&first, "second");
        // Moved again before the first move was finished (or while its old folder was in use).
        let third = move_to(&second, "third");
        assert_eq!(
            read_pointer(&config).unwrap().previous,
            vec![second.clone(), first.clone()]
        );

        let outcome = finish_move(&config, &third, None).unwrap();
        assert_eq!(
            outcome,
            FinishOutcome::Finished {
                deleted: vec![second.clone(), first.clone()],
                left: Vec::new(),
                retry: false,
            }
        );
        assert!(!first.exists() && !second.exists());
        assert!(third.join("accounts.json").is_file());
        assert!(read_pointer(&config).unwrap().previous.is_empty());
    }

    #[test]
    fn a_chain_with_a_broken_link_keeps_the_unproven_folder() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("location.json");
        let current = dir.path().join("current");
        sample_root(&current);
        // `newer` really moved here; `older` claims to have moved somewhere unrelated.
        let newer = dir.path().join("newer");
        write(
            &newer.join(MOVED_MARKER),
            &serde_json::to_vec(&serde_json::json!({ "movedTo": current })).unwrap(),
        );
        let older = dir.path().join("older");
        write(&older.join("keep.txt"), b"keep");
        write(
            &older.join(MOVED_MARKER),
            &serde_json::to_vec(&serde_json::json!({ "movedTo": dir.path().join("x") })).unwrap(),
        );
        write_pointer(
            &config,
            &Pointer {
                root: current.clone(),
                previous: vec![newer.clone(), older.clone()],
            },
        )
        .unwrap();
        let FinishOutcome::Finished { deleted, left, .. } =
            finish_move(&config, &current, None).unwrap()
        else {
            panic!("expected a finished move");
        };
        assert_eq!(deleted, vec![newer.clone()]);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].folder, older);
        assert!(older.join("keep.txt").exists());
        assert!(!newer.exists());
    }

    #[test]
    fn finish_deletes_only_the_marked_old_folder() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("location.json");
        let old = dir.path().join("old");
        sample_root(&old);
        let new = dir.path().join("new");
        let report = copy_data_root(
            &old,
            &new,
            &CancellationToken::new(),
            no_progress(),
            &never_live,
        )
        .unwrap();
        switch_root(&config, &old, &report.target).unwrap();

        // Not started from the new folder yet (e.g. a development override): nothing happens.
        let elsewhere = finish_move(&config, &old, None).unwrap();
        assert_eq!(elsewhere, FinishOutcome::NotStartedThere);
        assert!(old.join("accounts.json").exists());

        // Started from the new folder: the old one goes, the pointer forgets it.
        let done = finish_move(&config, &new, None).unwrap();
        assert_eq!(
            done,
            FinishOutcome::Finished {
                deleted: vec![old.clone()],
                left: Vec::new(),
                retry: false
            }
        );
        assert!(!old.exists());
        assert_eq!(
            read_pointer(&config).unwrap(),
            Pointer {
                root: new.clone(),
                previous: Vec::new()
            }
        );
        assert_eq!(
            finish_move(&config, &new, None).unwrap(),
            FinishOutcome::Nothing
        );
        assert!(new.join("accounts.json").exists());
    }

    #[test]
    fn finish_never_deletes_unmarked_or_overlapping_folders() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("location.json");
        let current = dir.path().join("current");
        sample_root(&current);

        // A pointer naming some unrelated folder as "previous" (hand-edited or corrupted).
        let unrelated = dir.path().join("documents");
        write(&unrelated.join("important.txt"), b"keep me");
        write_pointer(
            &config,
            &Pointer {
                root: current.clone(),
                previous: vec![unrelated.clone()],
            },
        )
        .unwrap();
        let outcome = finish_move(&config, &current, None).unwrap();
        assert!(
            matches!(&outcome, FinishOutcome::Finished { deleted, left, retry: false }
                if deleted.is_empty() && left.len() == 1),
            "{outcome:?}"
        );
        assert!(unrelated.join("important.txt").exists());
        assert!(read_pointer(&config).unwrap().previous.is_empty());

        // A marker that names another folder does not count either.
        write(
            &unrelated.join(MOVED_MARKER),
            br#"{"movedTo":"/somewhere/else"}"#,
        );
        write_pointer(
            &config,
            &Pointer {
                root: current.clone(),
                previous: vec![unrelated.clone()],
            },
        )
        .unwrap();
        finish_move(&config, &current, None).unwrap();
        assert!(unrelated.join("important.txt").exists());

        // "previous" containing the current folder (or being it) is never deleted.
        for previous in [dir.path().to_path_buf(), current.clone()] {
            write_pointer(
                &config,
                &Pointer {
                    root: current.clone(),
                    previous: vec![previous],
                },
            )
            .unwrap();
            finish_move(&config, &current, None).unwrap();
            assert!(current.join("accounts.json").exists());
        }
    }

    #[test]
    fn moving_back_to_the_default_folder_removes_the_pointer() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("location.json");
        let default = dir.path().join("ProjectsLauncher");
        let elsewhere = dir.path().join("elsewhere");
        sample_root(&elsewhere);
        let report = copy_data_root(
            &elsewhere,
            &default,
            &CancellationToken::new(),
            no_progress(),
            &never_live,
        )
        .unwrap();
        switch_root(&config, &elsewhere, &report.target).unwrap();
        let outcome = finish_move(&config, &default, Some(&default)).unwrap();
        assert_eq!(
            outcome,
            FinishOutcome::Finished {
                deleted: vec![elsewhere.clone()],
                left: Vec::new(),
                retry: false
            }
        );
        assert!(!config.exists());
        assert!(!elsewhere.exists());
    }

    #[cfg(windows)]
    #[test]
    fn removes_read_only_files() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        let file = tree.join("sub/locked.txt");
        write(&file, b"x");
        let mut permissions = fs::metadata(&file).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions).unwrap();
        remove_dir_all_robust(&tree).unwrap();
        assert!(!tree.exists());
    }

    // ---- links (junctions on Windows, symbolic links elsewhere) ----

    /// A folder link as a player makes one: a junction on Windows (no extra rights needed).
    fn folder_link(link: &Path, target: &Path) {
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        links::create(link, target, true).unwrap();
    }

    fn is_link(path: &Path) -> bool {
        fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
    }

    /// `sample_root` plus worlds kept outside the data folder (another disk, say) behind a
    /// folder link, and a second instance whose screenshots link to the first one's.
    fn root_with_links(dir: &Path) -> (PathBuf, PathBuf) {
        let current = dir.join("data");
        sample_root(&current);
        let worlds = dir.join("elsewhere").join("worlds");
        write(&worlds.join("Dünya/level.dat"), b"level");
        folder_link(&current.join("instances/projects/minecraft/saves"), &worlds);
        folder_link(
            &current.join("instances/second/minecraft/screenshots"),
            &current.join("instances/projects/minecraft/screenshots"),
        );
        (current, worlds)
    }

    #[test]
    fn folder_links_are_made_again_not_copied_through() {
        let dir = tempfile::tempdir().unwrap();
        let (current, worlds) = root_with_links(dir.path());
        let target = resolve_target(&current, &dir.path().join("moved")).unwrap();
        let report =
            copy_data_root(&current, &target, &never(), no_progress(), &never_live).unwrap();

        // What is behind the links was neither copied nor counted.
        assert_eq!(report.files, 5);
        let saves = target.join("instances/projects/minecraft/saves");
        assert!(is_link(&saves), "the worlds link is a link again");
        assert!(links::same_target(
            &saves,
            &fs::read_link(&saves).unwrap(),
            &worlds
        ));
        assert_eq!(fs::read(saves.join("Dünya/level.dat")).unwrap(), b"level");
        // A link into the data folder leads to the same place in the copy, not back into the
        // old folder (which goes away after the move).
        let screenshots = target.join("instances/second/minecraft/screenshots");
        assert!(is_link(&screenshots));
        assert!(links::same_target(
            &screenshots,
            &fs::read_link(&screenshots).unwrap(),
            &target.join("instances/projects/minecraft/screenshots")
        ));
        assert!(
            verify(&current, &target, &never_live, &never())
                .unwrap()
                .is_empty()
        );
        // The original links are untouched.
        assert!(is_link(&current.join("instances/projects/minecraft/saves")));
    }

    #[test]
    fn deleting_a_folder_never_follows_its_links() {
        let dir = tempfile::tempdir().unwrap();
        let (current, worlds) = root_with_links(dir.path());
        let target = resolve_target(&current, &dir.path().join("moved")).unwrap();
        copy_data_root(&current, &target, &never(), no_progress(), &never_live).unwrap();

        // A discarded copy and the old folder after a move both go without their links'
        // targets: the worlds stay where they are.
        discard_copy(&target, true);
        assert!(!target.exists());
        remove_dir_all_robust(&current).unwrap();
        assert!(!current.exists());
        assert_eq!(fs::read(worlds.join("Dünya/level.dat")).unwrap(), b"level");

        // A top-level link in a folder the user picked (kept, emptied): only the link goes.
        let picked = dir.path().join("picked");
        fs::create_dir_all(&picked).unwrap();
        folder_link(&picked.join("saves"), &worlds);
        discard_copy(&picked, false);
        assert!(picked.is_dir());
        assert_eq!(fs::read_dir(&picked).unwrap().count(), 0);
        assert_eq!(fs::read(worlds.join("Dünya/level.dat")).unwrap(), b"level");
    }

    #[test]
    fn a_link_that_cannot_be_made_stops_the_move_before_anything_changes() {
        let dir = tempfile::tempdir().unwrap();
        let (current, worlds) = root_with_links(dir.path());
        let target = dir.path().join("moved");
        let refuse: LinkMaker = |_, _, _| Err(std::io::Error::from_raw_os_error(5));
        let result = copy_data_root_with(
            &current,
            &target,
            &never(),
            no_progress(),
            &never_live,
            refuse,
        );
        assert!(
            matches!(result, Err(Error::Link { folder: true, .. })),
            "{result:?}"
        );
        assert!(!target.exists(), "the partial copy is gone");
        assert_eq!(scan(&current, &never()).unwrap().files, 5);
        assert!(is_link(&current.join("instances/projects/minecraft/saves")));
        assert_eq!(fs::read(worlds.join("Dünya/level.dat")).unwrap(), b"level");
    }

    #[test]
    fn a_link_changed_during_the_copy_gets_another_pass() {
        let dir = tempfile::tempdir().unwrap();
        let (current, _) = root_with_links(dir.path());
        let other = dir.path().join("elsewhere").join("other-worlds");
        fs::create_dir_all(&other).unwrap();
        let target = dir.path().join("moved");
        // The player points the link elsewhere while the copy runs.
        let link = current.join("instances/projects/minecraft/saves");
        let (moved_link, new_target) = (link.clone(), other.clone());
        let progress: ProgressFn = Arc::new(move |p| {
            if p.stage == MoveStage::Verifying && fs::read_link(&moved_link).is_ok() {
                let points_there = fs::read_link(&moved_link)
                    .is_ok_and(|now| links::same_target(&moved_link, &now, &new_target));
                if !points_there {
                    links::remove(&moved_link).unwrap();
                    links::create(&moved_link, &new_target, true).unwrap();
                }
            }
        });
        copy_data_root(&current, &target, &never(), progress, &never_live).unwrap();
        let copied = target.join("instances/projects/minecraft/saves");
        assert!(is_link(&copied));
        assert!(links::same_target(
            &copied,
            &fs::read_link(&copied).unwrap(),
            &other
        ));
    }

    #[test]
    fn file_links_are_made_again_or_copied_without_the_right_to_make_them() {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join("data");
        sample_root(&current);
        let options = dir.path().join("shared/options.txt");
        write(&options, b"guiScale:2");
        let link = current.join("instances/projects/minecraft/options.txt");
        if let Err(error) = links::create(&link, &options, false) {
            // Windows without Developer Mode cannot make file links at all, so a player cannot
            // have one either.
            assert!(links::needs_privilege(&error), "{error}");
            return;
        }
        let target = dir.path().join("moved");
        copy_data_root(&current, &target, &never(), no_progress(), &never_live).unwrap();
        let copied = target.join("instances/projects/minecraft/options.txt");
        assert!(is_link(&copied));
        assert_eq!(fs::read(&copied).unwrap(), b"guiScale:2");
        // Without the right to make links, the file it points to is copied instead.
        let fallback = dir.path().join("moved-2");
        let no_right: LinkMaker = |_, _, folder| {
            if folder {
                Ok(())
            } else {
                Err(std::io::Error::from_raw_os_error(1314))
            }
        };
        let result = copy_data_root_with(
            &current,
            &fallback,
            &never(),
            no_progress(),
            &never_live,
            no_right,
        );
        if cfg!(windows) {
            result.unwrap();
            let copied = fallback.join("instances/projects/minecraft/options.txt");
            assert!(!is_link(&copied));
            assert_eq!(fs::read(&copied).unwrap(), b"guiScale:2");
        } else {
            // Elsewhere that error is not about rights: the move stops.
            assert!(matches!(result, Err(Error::Link { folder: false, .. })));
        }
    }
}
