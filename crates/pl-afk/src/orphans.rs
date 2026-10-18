//! AFK clients a crashed launcher left running.
//!
//! The launcher drives a client through its standard input and output only. When the launcher
//! crashes those pipes are gone: the client can no longer be reached or asked to quit, yet it
//! stays on the server (Windows only ends child processes with their parent through a job
//! object), and the next launcher's client for the same account gets "logged in from another
//! location" or "already connected". Every spawned client is therefore recorded in its account
//! folder ([`RECORD_FILE_NAME`]: process id, the start time the OS reports for it, the client
//! executable and the account's config file), and the record goes when the client exits.
//!
//! When the manager starts, nothing of ours runs, so a record still there means the launcher
//! stopped without cleaning up. If that very process still runs (same id and start time, and our
//! client or our config file on its command line), it is terminated: with its pipes gone that is
//! the only way left to stop it, and re-adopting it is impossible for the same reason. Anything
//! else (the process is gone, or its id now belongs to another program) only drops the record: a
//! process this launcher did not start is never touched.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::util::{remove_file_if_exists, write_atomic};

/// The record of the client running in an account folder.
pub const RECORD_FILE_NAME: &str = "istemci.json";

/// How long a terminated client gets to disappear.
const TERMINATE_WAIT: Duration = Duration::from_secs(2);

/// A client this launcher started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientRecord {
    pub pid: u32,
    /// Start time in seconds since the Unix epoch, as the OS reports it (0: it could not be read).
    pub started: u64,
    pub exe: PathBuf,
    pub ini: PathBuf,
}

/// What the OS reports about a running process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessInfo {
    pub started: u64,
    pub exe: Option<PathBuf>,
    pub cmd: Vec<OsString>,
}

/// The OS process table (a fake in tests).
pub(crate) trait ProcessTable {
    /// The process with this id, if one runs.
    fn info(&mut self, pid: u32) -> Option<ProcessInfo>;
    /// Terminates the process and waits up to `wait` for it to go. True when it is gone.
    fn terminate(&mut self, pid: u32, wait: Duration) -> bool;
}

/// The real process table, through `sysinfo`.
pub(crate) struct OsProcesses {
    system: sysinfo::System,
}

impl OsProcesses {
    pub(crate) fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
        }
    }

    fn refresh(&mut self, pid: sysinfo::Pid) {
        self.system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&[pid]),
            true,
            sysinfo::ProcessRefreshKind::nothing()
                .with_exe(sysinfo::UpdateKind::Always)
                .with_cmd(sysinfo::UpdateKind::Always),
        );
    }

    /// Whether `pid` still runs. On Unix a killed process stays listed as a zombie until its
    /// parent reaps it; it runs no more.
    fn runs(&self, pid: sysinfo::Pid) -> bool {
        self.system
            .process(pid)
            .is_some_and(|process| process.status() != sysinfo::ProcessStatus::Zombie)
    }
}

impl ProcessTable for OsProcesses {
    fn info(&mut self, pid: u32) -> Option<ProcessInfo> {
        let pid = sysinfo::Pid::from_u32(pid);
        self.refresh(pid);
        let process = self.system.process(pid)?;
        Some(ProcessInfo {
            started: process.start_time(),
            exe: process.exe().map(Path::to_path_buf),
            cmd: process.cmd().to_vec(),
        })
    }

    fn terminate(&mut self, pid: u32, wait: Duration) -> bool {
        let pid = sysinfo::Pid::from_u32(pid);
        match self.system.process(pid) {
            Some(process) => {
                process.kill();
            }
            None => return true,
        }
        let deadline = Instant::now() + wait;
        loop {
            self.refresh(pid);
            if !self.runs(pid) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Start time of the process `pid` as the OS reports it (0 when it cannot be read).
fn start_time_of(pid: u32) -> u64 {
    OsProcesses::new().info(pid).map_or(0, |info| info.started)
}

/// Records the client just started in the account folder `dir` (failures are only logged: the
/// record only matters after a crash).
pub(crate) fn record_spawn(dir: &Path, pid: u32, exe: &Path, ini: &Path) {
    let record = ClientRecord {
        pid,
        started: start_time_of(pid),
        exe: exe.to_path_buf(),
        ini: ini.to_path_buf(),
    };
    let written = serde_json::to_vec_pretty(&record)
        .map_err(std::io::Error::other)
        .and_then(|bytes| write_atomic(&dir.join(RECORD_FILE_NAME), &bytes, false));
    if let Err(error) = written {
        tracing::warn!(%error, "could not record the AFK client process");
    }
}

/// Drops the record of the client `pid` once it has exited (a newer record is left alone).
pub(crate) fn clear_record(dir: &Path, pid: u32) {
    let path = dir.join(RECORD_FILE_NAME);
    if read_record(&path).is_some_and(|record| record.pid != pid) {
        return;
    }
    if let Err(error) = remove_file_if_exists(&path) {
        tracing::warn!(%error, "could not drop the AFK client record");
    }
}

fn read_record(path: &Path) -> Option<ClientRecord> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Whether two paths name the same file (letter case and separators as the OS sees them).
fn same_file(a: &Path, b: &Path) -> bool {
    let plain = |path: &Path| {
        let text = path.to_string_lossy().replace('/', "\\");
        let text = text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned();
        if cfg!(any(windows, target_os = "macos")) {
            text.to_lowercase()
        } else {
            text
        }
    };
    plain(a) == plain(b)
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

/// Whether `info` is the very process `record` describes: the same start time and our client
/// executable or the account's config file on its command line. Without a recorded start time,
/// both must match.
pub(crate) fn is_ours(record: &ClientRecord, info: &ProcessInfo) -> bool {
    let client = info
        .exe
        .as_deref()
        .is_some_and(|exe| same_file(exe, &record.exe));
    let config = info
        .cmd
        .iter()
        .any(|arg| same_file(Path::new(arg), &record.ini));
    if record.started == 0 {
        client && config
    } else {
        info.started.abs_diff(record.started) <= 1 && (client || config)
    }
}

/// Stops the clients a previous launcher left running under `accounts_dir` (see the module
/// docs) and drops every stale record. Returns the ids of the processes terminated.
pub(crate) fn stop_orphans(accounts_dir: &Path, table: &mut dyn ProcessTable) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir(accounts_dir) else {
        return Vec::new();
    };
    let mut stopped = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path().join(RECORD_FILE_NAME);
        if !path.is_file() {
            continue;
        }
        if let Some(record) = read_record(&path)
            && let Some(info) = table.info(record.pid)
            && is_ours(&record, &info)
        {
            if table.terminate(record.pid, TERMINATE_WAIT) {
                tracing::warn!(
                    pid = record.pid,
                    "stopped an AFK client a previous launcher left running"
                );
                stopped.push(record.pid);
            } else {
                // Keep the record: the next start tries again.
                tracing::warn!(pid = record.pid, "an AFK client left running did not stop");
                continue;
            }
        }
        if let Err(error) = remove_file_if_exists(&path) {
            tracing::warn!(%error, "could not drop a stale AFK client record");
        }
    }
    stopped
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct FakeTable {
        running: HashMap<u32, ProcessInfo>,
        stubborn: bool,
        terminated: Vec<u32>,
    }

    impl ProcessTable for FakeTable {
        fn info(&mut self, pid: u32) -> Option<ProcessInfo> {
            self.running.get(&pid).cloned()
        }

        fn terminate(&mut self, pid: u32, _wait: Duration) -> bool {
            if self.stubborn {
                return false;
            }
            self.terminated.push(pid);
            self.running.remove(&pid).is_some()
        }
    }

    struct Setup {
        dir: tempfile::TempDir,
        exe: PathBuf,
    }

    impl Setup {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let exe = dir.path().join("bin").join("ProjectsConsoleClient.exe");
            Self { dir, exe }
        }

        fn accounts(&self) -> PathBuf {
            self.dir.path().join("accounts")
        }

        fn ini(&self, account: &str) -> PathBuf {
            self.accounts().join(account).join("MinecraftClient.ini")
        }

        /// A record left in the account folder, as a crash leaves it.
        fn record(&self, account: &str, pid: u32, started: u64) -> PathBuf {
            let dir = self.accounts().join(account);
            std::fs::create_dir_all(&dir).unwrap();
            let record = ClientRecord {
                pid,
                started,
                exe: self.exe.clone(),
                ini: self.ini(account),
            };
            let path = dir.join(RECORD_FILE_NAME);
            std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
            path
        }

        /// What the OS shows for our client of `account`.
        fn client(&self, account: &str, started: u64) -> ProcessInfo {
            ProcessInfo {
                started,
                exe: Some(self.exe.clone()),
                cmd: vec![
                    self.exe.clone().into_os_string(),
                    self.ini(account).into_os_string(),
                    "BasicIO-NoColor".into(),
                ],
            }
        }
    }

    #[test]
    fn a_client_left_running_is_stopped() {
        let setup = Setup::new();
        let record = setup.record("hesap1", 4242, 1_760_000_000);
        let mut table = FakeTable::default();
        table
            .running
            .insert(4242, setup.client("hesap1", 1_760_000_000));
        assert_eq!(stop_orphans(&setup.accounts(), &mut table), vec![4242]);
        assert!(!record.exists());
        assert!(table.running.is_empty());
    }

    #[test]
    fn a_process_id_now_used_by_another_program_is_never_touched() {
        let setup = Setup::new();
        let reused = setup.record("hesap1", 4242, 1_760_000_000);
        let other = setup.record("hesap2", 5151, 1_760_000_000);
        let mut table = FakeTable::default();
        // The id came back for a later process of our very client (another account's), and for
        // a stranger started at the same second.
        table
            .running
            .insert(4242, setup.client("hesap2", 1_760_000_500));
        table.running.insert(
            5151,
            ProcessInfo {
                started: 1_760_000_000,
                exe: Some(PathBuf::from(r"C:\Windows\notepad.exe")),
                cmd: vec!["notepad.exe".into()],
            },
        );
        assert!(stop_orphans(&setup.accounts(), &mut table).is_empty());
        assert!(table.terminated.is_empty());
        assert!(!reused.exists() && !other.exists(), "stale records go");
    }

    #[test]
    fn a_client_that_already_exited_only_drops_its_record() {
        let setup = Setup::new();
        let gone = setup.record("hesap1", 4242, 1_760_000_000);
        let broken = setup.accounts().join("hesap2").join(RECORD_FILE_NAME);
        std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
        std::fs::write(&broken, b"{bozuk").unwrap();
        let mut table = FakeTable::default();
        assert!(stop_orphans(&setup.accounts(), &mut table).is_empty());
        assert!(table.terminated.is_empty());
        assert!(!gone.exists() && !broken.exists());
        // No accounts folder at all: nothing to do.
        assert!(stop_orphans(&setup.dir.path().join("yok"), &mut table).is_empty());
    }

    #[test]
    fn without_a_start_time_both_the_client_and_its_config_must_match() {
        let setup = Setup::new();
        let record = ClientRecord {
            pid: 7,
            started: 0,
            exe: setup.exe.clone(),
            ini: setup.ini("hesap1"),
        };
        assert!(is_ours(&record, &setup.client("hesap1", 123)));
        let mut only_exe = setup.client("hesap1", 123);
        only_exe
            .cmd
            .retain(|arg| arg != setup.ini("hesap1").as_os_str());
        assert!(!is_ours(&record, &only_exe));
        // With a start time, one of the two is enough (the OS may hide the executable path).
        let timed = ClientRecord {
            started: 123,
            ..record.clone()
        };
        let mut hidden_exe = setup.client("hesap1", 123);
        hidden_exe.exe = None;
        assert!(is_ours(&timed, &hidden_exe));
        assert!(!is_ours(&timed, &setup.client("hesap1", 200)));
    }

    #[test]
    fn a_client_that_does_not_stop_keeps_its_record_for_the_next_start() {
        let setup = Setup::new();
        let record = setup.record("hesap1", 4242, 99);
        let mut table = FakeTable {
            stubborn: true,
            ..FakeTable::default()
        };
        table.running.insert(4242, setup.client("hesap1", 99));
        assert!(stop_orphans(&setup.accounts(), &mut table).is_empty());
        assert!(record.exists());
    }

    #[test]
    fn records_belong_to_one_process() {
        let setup = Setup::new();
        let dir = setup.accounts().join("hesap1");
        std::fs::create_dir_all(&dir).unwrap();
        record_spawn(&dir, std::process::id(), &setup.exe, &setup.ini("hesap1"));
        let record = read_record(&dir.join(RECORD_FILE_NAME)).unwrap();
        assert_eq!(record.pid, std::process::id());
        assert!(record.started > 0, "the OS start time of this test process");
        // An older client's exit leaves the newer record alone.
        clear_record(&dir, record.pid + 1);
        assert!(dir.join(RECORD_FILE_NAME).exists());
        clear_record(&dir, record.pid);
        assert!(!dir.join(RECORD_FILE_NAME).exists());
    }
}
