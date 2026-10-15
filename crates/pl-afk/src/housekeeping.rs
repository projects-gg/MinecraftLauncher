//! Disk hygiene for account folders: size-capped logs and no stale password files.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::config::LOGIN_SCRIPT_FILE_NAME;
use crate::console::ConsoleLine;
use crate::paths::{ALERTS_FILE_NAME, CHAT_LOG_FILE_NAME, CONSOLE_LOG_FILE_NAME};
use crate::util::remove_file_if_exists;

/// Default size cap of `sohbet.txt`, `uyarilar.txt` and `konsol.txt`.
pub const DEFAULT_LOG_CAP_BYTES: u64 = 4 * 1024 * 1024;

/// `sohbet.txt` → `sohbet.1.txt`.
fn rotated_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(ext) => format!("{stem}.1.{}", ext.to_string_lossy()),
        None => format!("{stem}.1"),
    };
    path.with_file_name(name)
}

/// Moves `path` to its `.1` sibling (replacing an older one) when it is larger than `cap`.
pub(crate) fn rotate_if_larger(path: &Path, cap: u64) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() > cap => {
            let rotated = rotated_path(path);
            let _ = remove_file_if_exists(&rotated);
            match std::fs::rename(path, &rotated) {
                Ok(()) => true,
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "could not rotate AFK log");
                    false
                }
            }
        }
        _ => false,
    }
}

/// Rotates the chat, alert and console logs of an account folder (called before each launch).
pub(crate) fn rotate_account_logs(dir: &Path, cap: u64) {
    for name in [CHAT_LOG_FILE_NAME, ALERTS_FILE_NAME, CONSOLE_LOG_FILE_NAME] {
        rotate_if_larger(&dir.join(name), cap);
    }
}

/// Deletes login scripts left behind by a crash (nothing runs when the manager starts). Returns
/// how many were removed.
pub(crate) fn purge_login_scripts(accounts_dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(accounts_dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.path().join(LOGIN_SCRIPT_FILE_NAME).is_file())
        .filter(|entry| remove_file_if_exists(&entry.path().join(LOGIN_SCRIPT_FILE_NAME)).is_ok())
        .count()
}

/// Appends (already masked) console lines to `konsol.txt` on a background thread so slow disks
/// never stall a session. Rotates the file if it grows past twice the cap while running.
pub(crate) struct ConsoleLog {
    tx: std::sync::mpsc::Sender<ConsoleLine>,
}

impl ConsoleLog {
    pub(crate) fn open(path: PathBuf, cap: u64) -> Self {
        let (tx, rx) = std::sync::mpsc::channel::<ConsoleLine>();
        let spawned = std::thread::Builder::new()
            .name("pl-afk-console-log".into())
            .spawn(move || write_loop(&path, cap, &rx));
        if let Err(error) = spawned {
            tracing::warn!(%error, "AFK console log disabled");
        }
        Self { tx }
    }

    pub(crate) fn write(&self, line: &ConsoleLine) {
        let _ = self.tx.send(line.clone());
    }
}

fn open_append(path: &Path) -> Option<(std::io::BufWriter<std::fs::File>, u64)> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    Some((std::io::BufWriter::new(file), size))
}

fn write_loop(path: &Path, cap: u64, rx: &std::sync::mpsc::Receiver<ConsoleLine>) {
    let Some((mut writer, mut size)) = open_append(path) else {
        return;
    };
    let zone = jiff::tz::TimeZone::system();
    while let Ok(first) = rx.recv() {
        let mut next = Some(first);
        while let Some(line) = next {
            let stamp = line.at.to_zoned(zone.clone()).strftime("%Y-%m-%d %H:%M:%S");
            let text = format!("[{stamp}] {}\n", line.text);
            if writer.write_all(text.as_bytes()).is_err() {
                return;
            }
            size += text.len() as u64;
            next = rx.try_recv().ok();
        }
        if writer.flush().is_err() {
            return;
        }
        if size > cap.saturating_mul(2) {
            drop(writer);
            rotate_if_larger(path, cap);
            match open_append(path) {
                Some((w, s)) => (writer, size) = (w, s),
                None => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::{ConsoleLineKind, ConsoleOrigin};

    #[test]
    fn rotation_respects_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let chat = dir.path().join(CHAT_LOG_FILE_NAME);
        std::fs::write(&chat, vec![b'x'; 10]).unwrap();
        std::fs::write(dir.path().join("sohbet.1.txt"), b"old").unwrap();
        std::fs::write(dir.path().join(ALERTS_FILE_NAME), b"small").unwrap();
        rotate_account_logs(dir.path(), 8);
        assert!(!chat.exists());
        assert_eq!(
            std::fs::read(dir.path().join("sohbet.1.txt"))
                .unwrap()
                .len(),
            10
        );
        assert!(
            dir.path().join(ALERTS_FILE_NAME).exists(),
            "below the cap stays"
        );
        assert_eq!(
            rotated_path(Path::new("a/konsol.txt")),
            PathBuf::from("a/konsol.1.txt")
        );
        assert_eq!(rotated_path(Path::new("a/log")), PathBuf::from("a/log.1"));
    }

    #[test]
    fn purges_leftover_login_scripts() {
        let dir = tempfile::tempdir().unwrap();
        for id in ["a", "b"] {
            std::fs::create_dir_all(dir.path().join(id)).unwrap();
        }
        std::fs::write(
            dir.path().join("a").join(LOGIN_SCRIPT_FILE_NAME),
            b"send /login pw",
        )
        .unwrap();
        assert_eq!(purge_login_scripts(dir.path()), 1);
        assert!(!dir.path().join("a").join(LOGIN_SCRIPT_FILE_NAME).exists());
        assert_eq!(purge_login_scripts(&dir.path().join("missing")), 0);
    }

    #[test]
    fn console_log_appends_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONSOLE_LOG_FILE_NAME);
        let log = ConsoleLog::open(path.clone(), 1024);
        log.write(&ConsoleLine {
            at: jiff::Timestamp::UNIX_EPOCH,
            text: "merhaba".into(),
            kind: ConsoleLineKind::Normal,
            origin: ConsoleOrigin::Client,
        });
        drop(log);
        for _ in 0..200 {
            if std::fs::read_to_string(&path).is_ok_and(|t| t.contains("merhaba")) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("console log was not written");
    }
}
