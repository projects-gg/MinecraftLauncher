//! End to end against a real child process: the `pl-afk-fake-mcc` test binary plays the client
//! (see `tests/support/fake_mcc.rs`), so spawning, pipes, environment, working directory, exit
//! codes and killing are the real thing. Timings are shortened; no server is involved.
//!
//! These run while other builds load the machine, where one restart of the client took more than
//! a minute (a busy-loop on 12 of 16 threads besides the builds). So every wait is for an event,
//! with a deadline that only runs out when something is broken, the client's last lines are
//! always read before its exit is decided, and no assertion depends on how soon a process starts
//! or a thread gets to run.

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use common::{TestEnv, eventually, is_error};
use pl_afk::{
    AfkManager, ClientProcess, ClientSpawner, ConsoleLineKind, ConsoleOrigin, FailureReason,
    LaunchSpec, ManagerConfig, ProcessSpawner, RestartSchedule, SessionState,
};

/// Deadline of every wait (see the module comment).
const WAIT: Duration = Duration::from_secs(300);

fn fast_config() -> ManagerConfig {
    ManagerConfig {
        stagger: Duration::from_millis(200),
        // Every account delay (5-10 s by default) shrinks to 0.6 s.
        restart: RestartSchedule {
            floor: Duration::from_millis(300),
            ceiling: Duration::from_millis(600),
            ramp: 4,
            ..RestartSchedule::default()
        },
        // These clients leave on `/quit`: a slow machine must not turn that into a kill. (The one
        // that ignores `/quit` has a grace period of its own.)
        stop_grace: WAIT,
        // Pacing has its own test below; the others connect at once.
        connect_spacing: Duration::ZERO,
        // Why a client left comes in its last lines; a slow machine must not lose them.
        reader_drain: WAIT,
        ..ManagerConfig::default()
    }
}

fn fake_env() -> TestEnv {
    TestEnv::new(Some(Path::new(env!("CARGO_BIN_EXE_pl-afk-fake-mcc"))))
}

fn scenario(env: &TestEnv, id: &str, runs: serde_json::Value) {
    let dir = env.account_dir(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("fake-mcc.json"),
        serde_json::to_string(&serde_json::json!({ "runs": runs })).unwrap(),
    )
    .unwrap();
}

fn read(env: &TestEnv, id: &str, file: &str) -> String {
    std::fs::read_to_string(env.account_dir(id).join(file)).unwrap_or_default()
}

fn runs(env: &TestEnv, id: &str) -> usize {
    read(env, id, "fake-mcc-runs.txt")
        .trim()
        .parse()
        .unwrap_or(0)
}

fn connected(manager: &AfkManager, id: &str) -> bool {
    matches!(manager.snapshot(id).state, SessionState::Connected { .. })
}

fn console_has(manager: &AfkManager, id: &str, text: &str) -> bool {
    manager.console(id).iter().any(|l| l.text == text)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_session_with_a_real_process() {
    let env = fake_env();
    let id = env.add("Gercek").id;
    env.store
        .set_login_commands(&id, &["/login hunter22".into(), "/kit".into()])
        .unwrap();
    env.update(&id, |a| {
        a.login_commands.enabled = true;
        a.login_commands.delay_seconds = 0.0;
    });
    scenario(
        &env,
        &id,
        serde_json::json!([[
            { "out": "[MCC] Projects Konsol Hesap başlıyor" },
            { "out": r#"@@MCC@@{"e":"connecting","host":"localhost","port":25565}"# },
            { "join": true },
            { "err": "uyarı: deneme" },
            { "out": r#"@@MCC@@{"e":"health","hp":20,"food":19}"# },
            { "out": r#"@@MCC@@{"e":"players","n":7}"# },
            { "echoUntilQuit": true }
        ]]),
    );

    let manager = env.manager(None, fast_config());
    manager.connect(&id).await.unwrap();
    eventually("connected", WAIT, || connected(&manager, &id)).await;
    eventually("stats", WAIT, || {
        manager.snapshot(&id).stats.players == Some(7)
    })
    .await;

    let report = read(&env, &id, "fake-mcc-report.txt");
    assert!(
        report.contains(
            "run=1 ini_name=MinecraftClient.ini ini_exists=true mode=BasicIO-NoColor bridge=1 login_script_at_start=true"
        ),
        "{report}"
    );
    assert!(report.contains("login_script_at_join=true"), "{report}");
    assert!(
        !env.login_script_exists(&id),
        "deleted once the game was joined"
    );
    // The running client is recorded for the next start, in case this launcher crashes.
    let record = read(&env, &id, pl_afk::RECORD_FILE_NAME);
    assert!(
        record.contains("\"pid\"") && record.contains("MinecraftClient.ini"),
        "{record}"
    );

    // Stdout and stderr are read apart: the error line may come after the stats printed after it.
    eventually("the client's error line", WAIT, || {
        console_has(&manager, &id, "[hata] uyarı: deneme")
    })
    .await;
    let console = manager.console(&id);
    let find = |text: &str| console.iter().find(|l| l.text == text).cloned();
    assert_eq!(
        find("[MCC] Projects Konsol Hesap başlıyor").map(|l| l.kind),
        Some(ConsoleLineKind::Normal)
    );
    assert_eq!(
        find("[Script] send /login ******").map(|l| l.kind),
        Some(ConsoleLineKind::Sensitive)
    );
    assert_eq!(
        find("[Script] send /kit").map(|l| l.kind),
        Some(ConsoleLineKind::Script)
    );
    assert_eq!(
        find("[hata] uyarı: deneme").map(|l| l.origin),
        Some(ConsoleOrigin::ClientError)
    );
    assert!(console.iter().all(|l| !l.text.starts_with("@@MCC@@")));

    manager.send(&id, "merhaba dünya").unwrap();
    eventually("echo", WAIT, || {
        console_has(&manager, &id, "[echo] merhaba dünya")
    })
    .await;

    manager.disconnect(&id);
    eventually("stopped", WAIT, || {
        let snapshot = manager.snapshot(&id);
        snapshot.state == SessionState::Stopped && !snapshot.running
    })
    .await;
    assert_eq!(
        read(&env, &id, "fake-mcc-stdin.txt"),
        "merhaba dünya\n/quit\n"
    );
    assert_eq!(runs(&env, &id), 1, "a requested stop is never restarted");
    assert!(
        !env.account_dir(&id).join(pl_afk::RECORD_FILE_NAME).exists(),
        "the record goes with the client"
    );
    assert!(!env.all_event_json().contains("hunter22"));
    eventually("console log", WAIT, || {
        read(&env, &id, "konsol.txt").contains("[echo] merhaba dünya")
    })
    .await;
    assert!(!read(&env, &id, "konsol.txt").contains("hunter22"));
    manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exit_codes_drive_the_restart_loop() {
    let env = fake_env();
    let id = env.add("Donen").id;
    scenario(
        &env,
        &id,
        serde_json::json!([
            [
                { "out": r#"@@MCC@@{"e":"connecting","host":"localhost","port":25565}"# },
                { "out": r#"@@MCC@@{"e":"disconnected","reason":"ConnectionLost","msg":"Bağlantı koptu"}"# },
                { "out": r#"@@MCC@@{"e":"error","reason":"ConnectionLost","versionError":false}"# },
                { "exit": 3 }
            ],
            [
                { "out": r#"@@MCC@@{"e":"connecting","host":"localhost","port":25565}"# },
                { "join": true },
                { "echoUntilQuit": true }
            ]
        ]),
    );
    let manager = env.manager(None, fast_config());
    manager.connect(&id).await.unwrap();
    eventually("reconnected", WAIT, || {
        connected(&manager, &id) && runs(&env, &id) == 2
    })
    .await;
    assert!(env.states(&id).iter().any(|s| matches!(
        s,
        SessionState::Reconnecting {
            attempt: 1,
            next_attempt_at: Some(_),
            reason: Some(FailureReason::ConnectionLost { message: Some(m) }),
        } if m == "Bağlantı koptu"
    )));
    assert!(manager.any_active());
    manager.stop_all().await;
    assert!(!manager.any_active());
    assert_eq!(manager.snapshot(&id).state, SessionState::Stopped);
    manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retry_limit_with_real_exit_codes() {
    let env = fake_env();
    let id = env.add("Reddedilen").id;
    env.update(&id, |a| a.auto_relog.retries = 2);
    scenario(
        &env,
        &id,
        serde_json::json!([[
            { "out": r#"@@MCC@@{"e":"error","msg":"Bot doğrulaması başarısız","reason":"LoginRejected","versionError":false}"# },
            { "exit": 4 }
        ]]),
    );
    let manager = env.manager(None, fast_config());
    manager.connect(&id).await.unwrap();
    eventually("gave up", WAIT, || {
        is_error(&env.last_state(&id)) && !manager.any_active()
    })
    .await;
    assert_eq!(
        manager.snapshot(&id).state,
        SessionState::Error {
            reason: FailureReason::LoginRejected {
                message: Some("Bot doğrulaması başarısız".into())
            },
            retry_limit_reached: true,
        }
    );
    assert_eq!(runs(&env, &id), 3, "initial launch + 2 restarts");
    assert!(!env.login_script_exists(&id));
    manager.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_ignoring_quit_is_killed() {
    const GRACE: Duration = Duration::from_millis(700);
    const UP: &str = "[MCC] Projects Konsol Hesap başlıyor";
    let env = fake_env();
    let id = env.add("Inatci").id;
    scenario(
        &env,
        &id,
        serde_json::json!([[{ "out": UP }, { "hang": true }]]),
    );
    let config = ManagerConfig {
        stop_grace: GRACE,
        ..fast_config()
    };
    let manager = env.manager(None, config);
    manager.connect(&id).await.unwrap();
    // Up and ignoring `/quit` from here on. That its process exists is not enough: on a loaded
    // machine the fresh copy of the client took longer than the grace period to start running.
    eventually("client up", WAIT, || console_has(&manager, &id, UP)).await;

    let started = std::time::Instant::now();
    manager.stop_all().await;
    let took = started.elapsed();
    assert!(took >= GRACE, "waited for the grace period: {took:?}");
    let gone = || {
        let snapshot = manager.snapshot(&id);
        snapshot.state == SessionState::Stopped && !snapshot.running
    };
    // `stop_all` waits for the kill to end the client, though not for ever: the grace period and
    // 3 s more. Only a starved machine runs that out.
    assert!(
        gone() || took >= GRACE + Duration::from_secs(3),
        "stop_all returned after {took:?} with the client still there"
    );
    eventually("killed", WAIT, gone).await;
    assert!(env.states(&id).contains(&SessionState::Stopping));
    assert_eq!(runs(&env, &id), 1, "a requested stop is never restarted");
    assert!(
        !env.account_dir(&id).join(pl_afk::RECORD_FILE_NAME).exists(),
        "the record goes with the killed client"
    );
    manager.shutdown().await;
}

/// The real process spawner, noting when each account's client starts.
struct RecordingSpawner {
    spawns: parking_lot::Mutex<Vec<(String, std::time::Instant)>>,
}

impl ClientSpawner for RecordingSpawner {
    fn spawn(&self, spec: &LaunchSpec) -> std::io::Result<ClientProcess> {
        let account = spec
            .cwd
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.spawns
            .lock()
            .push((account, std::time::Instant::now()));
        ProcessSpawner.spawn(spec)
    }
}

/// Real clients connecting to one server never start closer than the spacing: the first
/// connections, nor the reconnects after the server dropped them all. That they go in the order
/// they asked is checked on the virtual clock (`session_logic.rs`,
/// `connections_to_one_server_go_one_by_one_in_order`): here a launch's order depends on how soon
/// the machine runs it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_clients_to_one_server_start_a_spacing_apart() {
    const SPACING: Duration = Duration::from_millis(600);
    let env = fake_env();
    let ids: Vec<String> = ["Birinci", "Ikinci", "Ucuncu"]
        .iter()
        .map(|name| env.add(name).id)
        .collect();
    for id in &ids {
        // Dropped once (the server restarted), then in game.
        scenario(
            &env,
            id,
            serde_json::json!([
                [
                    { "out": r#"@@MCC@@{"e":"connecting","host":"localhost","port":25565}"# },
                    { "out": r#"@@MCC@@{"e":"disconnected","reason":"ConnectionLost","msg":"Sunucu kapandı"}"# },
                    { "out": r#"@@MCC@@{"e":"error","reason":"ConnectionLost","versionError":false}"# },
                    { "exit": 3 }
                ],
                [
                    { "out": r#"@@MCC@@{"e":"connecting","host":"localhost","port":25565}"# },
                    { "join": true },
                    { "echoUntilQuit": true }
                ]
            ]),
        );
    }
    let spawner = Arc::new(RecordingSpawner {
        spawns: parking_lot::Mutex::new(Vec::new()),
    });
    let config = ManagerConfig {
        connect_spacing: SPACING,
        ..fast_config()
    };
    let manager = env.manager(Some(spawner.clone()), config);
    // The three ask right after one another.
    let asked_at = std::time::Instant::now();
    for id in &ids {
        manager.connect(id).await.unwrap();
    }
    eventually("all back in game", WAIT, || {
        ids.iter()
            .all(|id| connected(&manager, id) && runs(&env, id) == 2)
    })
    .await;

    let spawns = spawner.spawns.lock().clone();
    assert_eq!(spawns.len(), 6, "{spawns:?}");
    for id in &ids {
        assert_eq!(
            spawns.iter().filter(|(account, _)| account == id).count(),
            2,
            "{id}: {spawns:?}"
        );
    }
    // Each launch waits for its own slot and the slots to one server are a spacing apart, so the
    // n-th connection starts at least n spacings after the first ask, however late a launch then
    // got through (unpaced, the first three would start within milliseconds). Neither the gap
    // between two launches nor their order is compared: a launch prepares after its slot (the
    // account's settings are read and written on a blocking thread), and on a busy machine one
    // launch got through 340 ms slower than the next one did.
    for (n, (id, at)) in spawns.iter().enumerate() {
        let after = at.duration_since(asked_at);
        let floor = SPACING * u32::try_from(n).unwrap();
        assert!(
            after >= floor,
            "connection {n} ({id}) came {after:?} after the first ask"
        );
    }
    // A reconnect waiting for its turn showed when it would go.
    for id in &ids {
        assert!(
            env.states(id).iter().any(|s| matches!(
                s,
                SessionState::Reconnecting {
                    next_attempt_at: Some(_),
                    ..
                }
            )),
            "{id}"
        );
    }
    manager.stop_all().await;
    manager.shutdown().await;
}
