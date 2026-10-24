//! End to end against a real child process: the `pl-afk-fake-mcc` test binary plays the client
//! (see `tests/support/fake_mcc.rs`), so spawning, pipes, environment, working directory, exit
//! codes and killing are the real thing. Timings are shortened; no server is involved.

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use common::{TestEnv, is_error, until};
use pl_afk::{
    AfkManager, ClientProcess, ClientSpawner, ConsoleLineKind, ConsoleOrigin, FailureReason,
    LaunchSpec, ManagerConfig, ProcessSpawner, RestartSchedule, SessionState,
};

const WAIT: Duration = Duration::from_secs(30);

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
        stop_grace: Duration::from_secs(2),
        // Pacing has its own test below; the others connect at once.
        connect_spacing: Duration::ZERO,
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
    until("connected", WAIT, || connected(&manager, &id)).await;
    until("stats", WAIT, || {
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
    until("echo", WAIT, || {
        manager
            .console(&id)
            .iter()
            .any(|l| l.text == "[echo] merhaba dünya")
    })
    .await;

    manager.disconnect(&id);
    until("stopped", WAIT, || {
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
    until("console log", WAIT, || {
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
    until("reconnected", WAIT, || {
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
    until("gave up", WAIT, || {
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
    let env = fake_env();
    let id = env.add("Inatci").id;
    scenario(&env, &id, serde_json::json!([[{ "hang": true }]]));
    let config = ManagerConfig {
        stop_grace: Duration::from_millis(700),
        ..fast_config()
    };
    let manager = env.manager(None, config);
    manager.connect(&id).await.unwrap();
    until("running", WAIT, || manager.snapshot(&id).running).await;
    let started = std::time::Instant::now();
    manager.stop_all().await;
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(700),
        "waited for the grace period: {took:?}"
    );
    assert!(took < Duration::from_secs(10), "{took:?}");
    let snapshot = manager.snapshot(&id);
    assert_eq!(snapshot.state, SessionState::Stopped);
    assert!(!snapshot.running);
    assert!(env.states(&id).contains(&SessionState::Stopping));
    assert_eq!(runs(&env, &id), 1);
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connections_to_one_server_go_one_by_one_in_order() {
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
    for id in &ids {
        manager.connect(id).await.unwrap();
        // A clear order of asking (the actors run on two threads).
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    until("all back in game", WAIT, || {
        ids.iter()
            .all(|id| connected(&manager, id) && runs(&env, id) == 2)
    })
    .await;

    let spawns = spawner.spawns.lock().clone();
    assert_eq!(spawns.len(), 6, "{spawns:?}");
    // The first connections go in the order the accounts asked.
    let first: Vec<&str> = spawns.iter().take(3).map(|(id, _)| id.as_str()).collect();
    let asked: Vec<&str> = ids.iter().map(String::as_str).collect();
    assert_eq!(first, asked);
    // Never two connections to the server closer than the spacing (a little slack for the
    // settings each launch reads and writes after its slot).
    for pair in spawns.windows(2) {
        let gap = pair[1].1.duration_since(pair[0].1);
        assert!(
            gap >= SPACING - Duration::from_millis(100),
            "{} then {}: {gap:?}",
            pair[0].0,
            pair[1].0
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
