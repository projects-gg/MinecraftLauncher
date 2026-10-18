//! Screenshots as the game takes them: while a game runs, `<game_dir>/screenshots` is watched
//! with the system's change notifications (ReadDirectoryChangesW, inotify, FSEvents; nothing is
//! polled) and each new picture is reported once the game has finished writing it. Pictures that
//! were already there when the watch began are never reported, even when they change.
//!
//! The game writes a screenshot straight into its final name, in pieces, so a picture counts as
//! finished when no change came for a moment ([`Timing::settle`]) and the file ends with the PNG
//! end chunk. A file that stays unfinished long after its last change ([`Timing::give_up`]) is
//! reported as such.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::fsops::is_png;
use crate::error::{Error, Result};

/// The last 12 bytes of every complete PNG: the empty `IEND` chunk and its CRC.
const PNG_END: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];

/// How patient the watch is with a picture being written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Quiet time after the last change before a picture is looked at.
    pub settle: Duration,
    /// Wait before looking again at a picture that was not finished yet.
    pub retry: Duration,
    /// A picture still unfinished this long after its last change is given up on.
    pub give_up: Duration,
}

impl Default for Timing {
    /// The game opens the file before it encodes the picture, so a big screenshot on a slow
    /// computer can sit empty for seconds: the watch waits up to 20 s after the last change
    /// (looking at the file's last bytes twice a second meanwhile, nothing else).
    fn default() -> Self {
        Self {
            settle: Duration::from_millis(350),
            retry: Duration::from_millis(500),
            give_up: Duration::from_secs(20),
        }
    }
}

/// What the watch saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenshotEvent {
    /// A new picture the game finished writing.
    Ready(PathBuf),
    /// A new picture that never became a complete PNG.
    Unfinished(PathBuf),
}

/// Watches a game's screenshots folder until it is dropped.
pub struct ScreenshotWatcher {
    _watcher: notify::RecommendedWatcher,
    task: tokio::task::JoinHandle<()>,
}

impl ScreenshotWatcher {
    /// Starts watching `<game_dir>/screenshots` (made when missing, so the game's first picture
    /// is seen too) and returns the stream of the pictures that appear from now on, in the order
    /// they were finished. Needs a Tokio runtime.
    pub fn start(game_dir: &Path) -> Result<(Self, mpsc::UnboundedReceiver<ScreenshotEvent>)> {
        Self::start_with(game_dir, Timing::default())
    }

    /// [`Self::start`] with other waits (tests).
    pub fn start_with(
        game_dir: &Path,
        timing: Timing,
    ) -> Result<(Self, mpsc::UnboundedReceiver<ScreenshotEvent>)> {
        let dir = game_dir.join("screenshots");
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let (changes_tx, changes) = mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(
            move |result: notify::Result<notify::Event>| match result {
                Ok(event) if counts(&event.kind) => {
                    for path in event.paths {
                        let _ = changes_tx.send(path);
                    }
                }
                Ok(_) => {}
                Err(error) => tracing::debug!(%error, "screenshot watch reported an error"),
            },
        )
        .map_err(|e| watch_error(&dir, e))?;
        watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .map_err(|e| watch_error(&dir, e))?;
        // Listed once the watch is on: what is there now is old, whatever happens to it later,
        // and nothing written meanwhile slips between the listing and the watch.
        let existing: HashSet<OsString> = std::fs::read_dir(&dir)
            .map_err(|e| Error::io(&dir, e))?
            .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
            .collect();
        let (events_tx, events) = mpsc::unbounded_channel();
        let task = tokio::spawn(follow(dir, existing, timing, changes, events_tx));
        Ok((
            Self {
                _watcher: watcher,
                task,
            },
            events,
        ))
    }
}

impl Drop for ScreenshotWatcher {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn watch_error(dir: &Path, error: notify::Error) -> Error {
    Error::io(dir, std::io::Error::other(error.to_string()))
}

/// Changes that can mean a new or growing picture (not removals or plain reads).
fn counts(kind: &EventKind) -> bool {
    use notify::event::{AccessKind, AccessMode};
    match kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any | EventKind::Other => true,
        // inotify: the game closed the file it wrote.
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) | EventKind::Remove(_) => false,
    }
}

/// A picture the game may name: `*.png`, not hidden.
pub fn is_screenshot_name(name: &str) -> bool {
    !name.starts_with('.') && name.to_ascii_lowercase().ends_with(".png")
}

/// Whether `tail` (the last bytes of a file) closes a complete PNG.
pub fn ends_like_png(tail: &[u8]) -> bool {
    tail.ends_with(&PNG_END)
}

/// Whether the file at `path` is a complete PNG now (an unreadable file is not, yet).
fn finished(path: &Path) -> bool {
    let read = || -> std::io::Result<bool> {
        let mut file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        if len < (8 + PNG_END.len()) as u64 {
            return Ok(false);
        }
        let mut head = [0_u8; 8];
        file.read_exact(&mut head)?;
        if !is_png(&head) {
            return Ok(false);
        }
        file.seek(SeekFrom::End(-(PNG_END.len() as i64)))?;
        let mut tail = [0_u8; PNG_END.len()];
        file.read_exact(&mut tail)?;
        Ok(ends_like_png(&tail))
    };
    read().unwrap_or(false)
}

struct Pending {
    /// When to look at it next.
    due: Instant,
    /// Its last change.
    changed: Instant,
}

async fn follow(
    dir: PathBuf,
    existing: HashSet<OsString>,
    timing: Timing,
    mut changes: mpsc::UnboundedReceiver<PathBuf>,
    events: mpsc::UnboundedSender<ScreenshotEvent>,
) {
    let mut done: HashSet<OsString> = HashSet::new();
    let mut pending: HashMap<PathBuf, Pending> = HashMap::new();
    loop {
        let next = pending.values().map(|p| p.due).min();
        tokio::select! {
            change = changes.recv() => {
                let Some(path) = change else { break };
                let Some(name) = path.file_name().map(OsString::from) else { continue };
                let wanted = path.parent() == Some(dir.as_path())
                    && name.to_str().is_some_and(is_screenshot_name)
                    && !existing.contains(&name)
                    && !done.contains(&name);
                if wanted {
                    let now = Instant::now();
                    pending.insert(path, Pending { due: now + timing.settle, changed: now });
                }
            }
            () = sleep_until(next), if next.is_some() => {
                let now = Instant::now();
                let mut ripe: Vec<(PathBuf, Instant)> = pending
                    .iter()
                    .filter(|(_, p)| p.due <= now)
                    .map(|(path, p)| (path.clone(), p.changed))
                    .collect();
                // The order the game wrote them in.
                ripe.sort_by_key(|(_, changed)| *changed);
                for (path, _) in ripe {
                    let check = path.clone();
                    let complete = tokio::task::spawn_blocking(move || finished(&check))
                        .await
                        .unwrap_or(false);
                    let Some(state) = pending.get_mut(&path) else { continue };
                    let event = if complete {
                        ScreenshotEvent::Ready(path.clone())
                    } else if state.changed.elapsed() >= timing.give_up {
                        ScreenshotEvent::Unfinished(path.clone())
                    } else {
                        state.due = Instant::now() + timing.retry;
                        continue;
                    };
                    pending.remove(&path);
                    if let Some(name) = path.file_name() {
                        done.insert(name.to_owned());
                    }
                    if events.send(event).is_err() {
                        return;
                    }
                }
            }
        }
    }
}

async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// Short waits, so the tests do not idle; the give-up stays long enough for a loaded machine
    /// (the test's own pause between two writes must never look like a broken file).
    const QUICK: Timing = Timing {
        settle: Duration::from_millis(120),
        retry: Duration::from_millis(120),
        give_up: Duration::from_secs(5),
    };

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 120, 40, 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    async fn next(events: &mut mpsc::UnboundedReceiver<ScreenshotEvent>) -> ScreenshotEvent {
        tokio::time::timeout(Duration::from_secs(30), events.recv())
            .await
            .expect("an event in time")
            .expect("the watch is running")
    }

    #[test]
    fn names_and_png_ends() {
        assert!(is_screenshot_name("2026-10-18_20.15.03.png"));
        assert!(is_screenshot_name("2026-10-18_20.15.03_1.PNG"));
        assert!(!is_screenshot_name(".2026-10-18.png"));
        assert!(!is_screenshot_name("notes.txt"));
        let bytes = png();
        assert!(ends_like_png(&bytes));
        assert!(!ends_like_png(&bytes[..bytes.len() - 1]));
    }

    #[test]
    fn removals_and_reads_do_not_count() {
        use notify::event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind};
        assert!(counts(&EventKind::Create(CreateKind::File)));
        assert!(counts(&EventKind::Modify(ModifyKind::Any)));
        assert!(counts(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
        assert!(!counts(&EventKind::Access(AccessKind::Read)));
        assert!(!counts(&EventKind::Remove(RemoveKind::File)));
    }

    #[tokio::test]
    async fn reports_new_pictures_once_they_are_written_and_never_old_ones() {
        let dir = tempfile::tempdir().unwrap();
        let shots = dir.path().join("screenshots");
        std::fs::create_dir_all(&shots).unwrap();
        let old = shots.join("2026-10-18_19.00.00.png");
        std::fs::write(&old, png()).unwrap();

        let (watcher, mut events) = ScreenshotWatcher::start_with(dir.path(), QUICK).unwrap();
        // An old picture that changes stays old; other files are not pictures.
        std::fs::write(&old, png()).unwrap();
        std::fs::write(shots.join("notes.txt"), b"x").unwrap();
        // A new one, written in two steps like the game does: half, a pause, the rest.
        let bytes = png();
        let new = shots.join("2026-10-18_20.15.03.png");
        let mut file = std::fs::File::create(&new).unwrap();
        file.write_all(&bytes[..bytes.len() / 2]).unwrap();
        file.flush().unwrap();
        tokio::time::sleep(QUICK.settle * 3).await;
        file.write_all(&bytes[bytes.len() / 2..]).unwrap();
        drop(file);

        assert_eq!(next(&mut events).await, ScreenshotEvent::Ready(new.clone()));
        // Writing it again reports nothing more; the next picture comes on its own.
        std::fs::write(&new, png()).unwrap();
        let second = shots.join("2026-10-18_20.15.09.png");
        std::fs::write(&second, png()).unwrap();
        assert_eq!(next(&mut events).await, ScreenshotEvent::Ready(second));
        drop(watcher);
        // Nothing after the watch ends.
        std::fs::write(shots.join("2026-10-18_20.16.00.png"), png()).unwrap();
        let late = tokio::time::timeout(Duration::from_secs(1), events.recv()).await;
        assert!(matches!(late, Ok(None) | Err(_)), "{late:?}");
    }

    #[tokio::test]
    async fn makes_the_folder_and_gives_up_on_a_broken_picture() {
        let dir = tempfile::tempdir().unwrap();
        let (_watcher, mut events) = ScreenshotWatcher::start_with(dir.path(), QUICK).unwrap();
        let shots = dir.path().join("screenshots");
        assert!(shots.is_dir());
        let broken = shots.join("2026-10-18_21.00.00.png");
        let bytes = png();
        std::fs::write(&broken, &bytes[..bytes.len() - 4]).unwrap();
        assert_eq!(next(&mut events).await, ScreenshotEvent::Unfinished(broken));
    }
}
