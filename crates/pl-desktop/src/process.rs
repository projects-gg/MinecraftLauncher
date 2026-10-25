//! A process the launcher started and later left alone (SMALL7: the game a launcher quit beside,
//! `pl_mc::launch::detached`): telling it apart from a process that got the same number later,
//! waiting for its end without looking again and again, and closing it.

/// When the process `pid` started (seconds since the Unix epoch, as the process table says), or
/// `None` when no such process runs.
pub fn start_time(pid: u32) -> Option<u64> {
    let mut system = sysinfo::System::new();
    let pid = sysinfo::Pid::from_u32(pid);
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[pid]),
        true,
        sysinfo::ProcessRefreshKind::nothing(),
    );
    system
        .process(pid)
        // A Unix process that ended stays listed until its parent reaps it; it runs no more.
        .filter(|process| process.status() != sysinfo::ProcessStatus::Zombie)
        .map(sysinfo::Process::start_time)
}

/// The process `pid` runs and is the one that started at `started` ([`start_time`]).
pub fn is_same(pid: u32, started: u64) -> bool {
    start_time(pid) == Some(started)
}

/// Whether the process `pid` is `root` or was started by it, directly or through others: the
/// Java a wrapper command (`hooks.wrapper`) starts belongs to the game started as `root`. One
/// look at the process table; stale parent numbers are not followed
/// ([`crate::game_window::descends_from`]). False when the table cannot tell.
pub fn started_by(pid: u32, root: u32) -> bool {
    if pid == root {
        return true;
    }
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing(),
    );
    crate::game_window::descends_from(pid, root, |id| {
        let process = system.process(sysinfo::Pid::from_u32(id))?;
        Some((process.parent()?.as_u32(), process.start_time()))
    })
}

/// Blocks until the process `pid` that started at `started` has ended; its exit code when the OS
/// still tells it. Returns at once when that process does not run (any more). Windows waits on the
/// process itself, so nothing wakes meanwhile; elsewhere it looks every [`LOOK_EVERY`].
pub fn wait_for_exit(pid: u32, started: u64) -> Option<i32> {
    imp::wait_for_exit(pid, started)
}

/// How often the wait looks where the OS cannot wait on another program's process.
pub const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// Ends the process `pid` that started at `started` ("game_kill" in end-to-end tests). False when
/// it does not run or could not be ended.
pub fn kill(pid: u32, started: u64) -> bool {
    let mut system = sysinfo::System::new();
    let id = sysinfo::Pid::from_u32(pid);
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[id]),
        true,
        sysinfo::ProcessRefreshKind::nothing(),
    );
    system
        .process(id)
        .filter(|process| process.start_time() == started)
        .is_some_and(sysinfo::Process::kill)
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, INFINITE, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    pub(super) fn wait_for_exit(pid: u32, started: u64) -> Option<i32> {
        // SAFETY: plain Win32 calls on a handle this function opens and closes.
        let handle = unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            )
        }
        .ok()?;
        // While the handle is open the number cannot pass to another process: the one it names
        // must be ours, started when ours did.
        let ours = super::is_same(pid, started);
        let code = ours.then(|| {
            // SAFETY: as above.
            let waited = unsafe { WaitForSingleObject(handle, INFINITE) };
            let mut code = 0u32;
            // SAFETY: as above; `code` outlives the call.
            let read = unsafe { GetExitCodeProcess(handle, &mut code) }.is_ok();
            (waited == WAIT_OBJECT_0 && read).then_some(code.cast_signed())
        });
        // SAFETY: the handle opened above, closed once.
        let _ = unsafe { CloseHandle(handle) };
        code.flatten()
    }
}

#[cfg(not(windows))]
mod imp {
    pub(super) fn wait_for_exit(pid: u32, started: u64) -> Option<i32> {
        while super::is_same(pid, started) {
            std::thread::sleep(super::LOOK_EVERY);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A child that lives about a second, then exits with 3.
    fn short_lived() -> std::process::Child {
        let mut command = if cfg!(windows) {
            let mut command = std::process::Command::new("cmd");
            command.args(["/C", "ping -n 2 127.0.0.1 >NUL & exit 3"]);
            command
        } else {
            let mut command = std::process::Command::new("sh");
            command.args(["-c", "sleep 1; exit 3"]);
            command
        };
        command.stdout(std::process::Stdio::null()).spawn().unwrap()
    }

    #[test]
    fn a_process_is_told_by_its_number_and_start() {
        let mut child = short_lived();
        let pid = child.id();
        let started = start_time(pid).expect("the child runs");
        assert!(is_same(pid, started));
        assert!(!is_same(pid, started + 1));
        assert!(!kill(pid, started + 1), "another start is another process");
        child.wait().unwrap();
        drop(child);
        assert!(!is_same(pid, started));
    }

    /// A process this one started counts as started by it; this one was not started by its child.
    #[test]
    fn a_child_is_started_by_its_parent() {
        let mut child = short_lived();
        let pid = child.id();
        let me = std::process::id();
        assert!(started_by(pid, me));
        assert!(started_by(me, me));
        assert!(!started_by(me, pid));
        child.wait().unwrap();
    }

    #[test]
    fn waiting_ends_with_the_process() {
        let mut child = short_lived();
        let pid = child.id();
        let started = start_time(pid).expect("the child runs");
        let since = Instant::now();
        let waiter = std::thread::spawn(move || wait_for_exit(pid, started));
        let code = waiter.join().unwrap();
        // Windows waits on the process itself; elsewhere the look comes within `LOOK_EVERY`.
        assert!(since.elapsed() < LOOK_EVERY + Duration::from_secs(10));
        if cfg!(windows) {
            assert_eq!(code, Some(3));
        }
        child.wait().unwrap();
        drop(child);
        // Not ours (any more): no wait at all.
        let since = Instant::now();
        assert_eq!(wait_for_exit(pid, started), None);
        assert!(since.elapsed() < Duration::from_secs(2));
    }
}
