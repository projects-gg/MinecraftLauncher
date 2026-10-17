//! AFK clients a crashed launcher left running, with real processes: the `pl-afk-fake-mcc` test
//! binary plays a client that hangs. The "crash" is a launcher that started the client and wrote
//! its record, then let go of it without stopping it.

mod common;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use common::TestEnv;
use pl_afk::{CONSOLE_MODE_ARG, INI_FILE_NAME, ManagerConfig, RECORD_FILE_NAME};
use sysinfo::{Pid, ProcessesToUpdate, System};

fn process(pid: u32) -> Option<(u64, System)> {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
    let started = system.process(Pid::from_u32(pid))?.start_time();
    Some((started, system))
}

fn alive(pid: u32) -> bool {
    process(pid).is_some()
}

fn kill(pid: u32) {
    if let Some((_, system)) = process(pid)
        && let Some(found) = system.process(Pid::from_u32(pid))
    {
        found.kill();
    }
}

/// Starts a hanging client for `account` as the manager would, records it the way the launcher
/// does, and lets go of it (a crash). `started_offset` shifts the recorded start time.
fn leave_running(env: &TestEnv, account: &str, started_offset: u64) -> u32 {
    let dir = env.account_dir(account);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fake-mcc.json"), r#"{"runs":[[{"hang":true}]]}"#).unwrap();
    let exe = env.runtime.resolve().unwrap().path;
    let ini = dir.join(INI_FILE_NAME);
    let child = Command::new(&exe)
        .arg(&ini)
        .arg(CONSOLE_MODE_ARG)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    // The launcher's handle is gone; the client is not.
    drop(child);
    let (started, _) = process(pid).expect("the client runs");
    let record = serde_json::json!({
        "pid": pid,
        "started": started + started_offset,
        "exe": exe,
        "ini": ini,
    });
    std::fs::write(
        dir.join(RECORD_FILE_NAME),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    pid
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_next_start_stops_only_the_clients_this_launcher_left_running() {
    let env = TestEnv::new(Some(Path::new(env!("CARGO_BIN_EXE_pl-afk-fake-mcc"))));
    let ours = env.add("Kalan").id;
    let unproven = env.add("Yabanci").id;
    let ours_pid = leave_running(&env, &ours, 0);
    // Its id is recorded, but the start time is not that process's: as if the id now belonged
    // to another program.
    let unproven_pid = leave_running(&env, &unproven, 3600);
    assert!(alive(ours_pid) && alive(unproven_pid));

    // The launcher starts again.
    let manager = env.manager(None, ManagerConfig::default());
    let gone = {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if !alive(ours_pid) {
                break true;
            }
            if std::time::Instant::now() > deadline {
                break false;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let unproven_alive = alive(unproven_pid);
    // Never leave a test client behind, whatever the outcome.
    kill(unproven_pid);
    kill(ours_pid);
    assert!(gone, "the client a crash left running is stopped");
    assert!(
        unproven_alive,
        "a process the record does not prove ours is never touched"
    );
    assert!(!env.account_dir(&ours).join(RECORD_FILE_NAME).exists());
    assert!(
        !env.account_dir(&unproven).join(RECORD_FILE_NAME).exists(),
        "the stale record goes"
    );
    manager.shutdown().await;
}
