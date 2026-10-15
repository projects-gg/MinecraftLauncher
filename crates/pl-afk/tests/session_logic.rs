//! Session state machine and manager scheduling under paused tokio time, against an in-memory
//! client (see `common::ScriptedSpawner`). Durations are virtual, so the real 15-60 s timings are
//! checked exactly.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{ScriptedSpawner, TestEnv, is_error, until};
use pl_afk::{
    AfkError, AfkEventKind, ClientSpawner, ConsoleLineKind, ConsoleOrigin, FailureReason,
    ManagerConfig, SessionState, SessionStats,
};

const LONG: Duration = Duration::from_secs(600);

fn spawner(s: &Arc<ScriptedSpawner>) -> Option<Arc<dyn ClientSpawner>> {
    Some(s.clone())
}

fn secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

/// Gap between launch `i` and the exit of launch `i - 1` (each run lives `run` seconds).
fn gaps(spawner: &ScriptedSpawner, run: Duration) -> Vec<u64> {
    let launches = spawner.launches.lock();
    launches
        .windows(2)
        .map(|w| (w[1].at - (w[0].at + run)).as_secs_f64().round() as u64)
        .collect()
}

#[tokio::test(start_paused = true)]
async fn restart_backoff_is_15_30_45_60_then_60() {
    let env = TestEnv::new(None);
    let id = env.add("Backoff").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"connecting","host":"h","port":25565}"#)
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        Some(3)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();

    until("six launches", LONG, || s.launch_count() >= 6).await;
    assert_eq!(gaps(&s, Duration::from_secs(1)), vec![15, 30, 45, 60, 60]);

    let reconnecting: Vec<u32> = env
        .states(&id)
        .into_iter()
        .filter_map(|state| match state {
            SessionState::Reconnecting {
                attempt,
                next_attempt_at: Some(_),
                reason,
            } => {
                assert_eq!(
                    reason,
                    Some(FailureReason::ConnectionLost { message: None })
                );
                Some(attempt)
            }
            _ => None,
        })
        .collect();
    assert_eq!(reconnecting, vec![1, 2, 3, 4, 5]);
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn retry_limit_stops_the_restart_loop() {
    let env = TestEnv::new(None);
    let id = env.add("Limited").id;
    env.update(&id, |a| a.auto_relog.retries = 3);
    let s = ScriptedSpawner::new(|_, _proc| async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Some(3)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();

    until("retry limit", LONG, || is_error(&env.last_state(&id))).await;
    assert_eq!(s.launch_count(), 4, "initial launch + 3 restarts");
    assert_eq!(
        env.last_state(&id),
        Some(SessionState::Error {
            reason: FailureReason::ConnectionLost { message: None },
            retry_limit_reached: true,
        })
    );
    // Nothing else happens later.
    tokio::time::sleep(Duration::from_secs(300)).await;
    assert_eq!(s.launch_count(), 4);
    assert!(!manager.any_active());
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn joining_resets_the_backoff() {
    let env = TestEnv::new(None);
    let id = env.add("Rejoin").id;
    let s = ScriptedSpawner::new(|run, mut proc| async move {
        if run == 1 {
            proc.event(r#"{"e":"joined","user":"Rejoin","host":"h","port":25565,"protocol":777}"#)
                .await;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        Some(3)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("three launches", LONG, || s.launch_count() >= 3).await;
    // Without the join the second gap would be 30 s.
    assert_eq!(gaps(&s, Duration::from_secs(1)), vec![15, 15]);
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn kick_message_filter_decides_whether_to_reconnect() {
    let env = TestEnv::new(None);
    let id = env.add("Kicked").id;
    env.update(&id, |a| {
        a.auto_relog.ignore_kick_message = false;
        a.auto_relog.kick_messages = vec!["yeniden başlatılıyor".into()];
    });
    let s = ScriptedSpawner::new(|run, mut proc| async move {
        let message = if run == 0 {
            "§cSunucu Yeniden Başlatılıyor"
        } else {
            "Spam yüzünden atıldınız"
        };
        proc.event(&format!(
            r#"{{"e":"disconnected","reason":"InGameKick","msg":"{message}"}}"#
        ))
        .await;
        proc.event(r#"{"e":"error","reason":"InGameKick","versionError":false}"#)
            .await;
        Some(2)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();

    until("kick without match", LONG, || {
        is_error(&env.last_state(&id))
    })
    .await;
    assert_eq!(s.launch_count(), 2, "first kick matched, second did not");
    assert_eq!(
        env.last_state(&id),
        Some(SessionState::Error {
            reason: FailureReason::Kicked {
                message: Some("Spam yüzünden atıldınız".into())
            },
            retry_limit_reached: false,
        })
    );
    // The first kick showed its message (kept over the message-less `error` event).
    assert!(env.states(&id).iter().any(|s| matches!(
        s,
        SessionState::Reconnecting { reason: Some(FailureReason::Kicked { message: Some(m) }), .. }
            if m == "Sunucu Yeniden Başlatılıyor"
    )));
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn restarts_use_the_latest_settings() {
    let env = TestEnv::new(None);
    let id = env.add("Fresh").id;
    let s = ScriptedSpawner::new(|_, _proc| async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Some(3)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("first restart scheduled", LONG, || {
        matches!(
            env.last_state(&id),
            Some(SessionState::Reconnecting {
                next_attempt_at: Some(_),
                ..
            })
        )
    })
    .await;

    // Edited while waiting: the legacy launcher restarted with the stale account object.
    env.update(&id, |a| {
        a.server_host = "yeni.sunucu.net".into();
        a.anti_afk.command = "/yeni".into();
        a.auto_relog.retries = 1;
    });
    until("second launch", LONG, || s.launch_count() == 2).await;
    let ini = s.launch(1).ini;
    assert!(
        ini.contains("Server = { Host = \"yeni.sunucu.net\" }"),
        "{ini}"
    );
    assert!(ini.contains("Command = \"/yeni\""));
    assert!(s.launch(0).ini.contains("play.projects.gg"));

    // The decision after the second exit also uses the new retry limit (1 restart allowed).
    until("gave up", LONG, || is_error(&env.last_state(&id))).await;
    assert!(matches!(
        env.last_state(&id),
        Some(SessionState::Error {
            retry_limit_reached: true,
            ..
        })
    ));
    assert_eq!(s.launch_count(), 2);
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn auto_relog_off_ends_in_error_without_restart() {
    let env = TestEnv::new(None);
    let id = env.add("NoRelog").id;
    env.update(&id, |a| a.auto_relog.enabled = false);
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"error","msg":"Bot doğrulaması başarısız","reason":"LoginRejected","versionError":false}"#)
            .await;
        Some(4)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("error", LONG, || {
        is_error(&env.last_state(&id)) && !manager.any_active()
    })
    .await;
    assert_eq!(
        env.last_state(&id),
        Some(SessionState::Error {
            reason: FailureReason::LoginRejected {
                message: Some("Bot doğrulaması başarısız".into())
            },
            retry_limit_reached: false,
        })
    );
    tokio::time::sleep(Duration::from_secs(120)).await;
    assert_eq!(s.launch_count(), 1);
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn stop_sends_quit_and_kills_after_the_grace_period() {
    let env = TestEnv::new(None);
    let id = env.add("Stubborn").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.hang().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("running", LONG, || manager.snapshot(&id).running).await;

    let stopped_at = tokio::time::Instant::now();
    manager.disconnect(&id);
    until("stopped", LONG, || {
        manager.snapshot(&id).state == SessionState::Stopped
    })
    .await;
    let took = tokio::time::Instant::now() - stopped_at;
    assert!(
        (4.0..4.2).contains(&secs(took)),
        "killed after the 4 s grace period, took {took:?}"
    );
    assert_eq!(s.received.lock().as_slice(), ["/quit"]);
    assert!(env.states(&id).contains(&SessionState::Stopping));
    assert!(!manager.any_active());
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn graceful_quit_and_user_typed_quit_stop_without_restart() {
    let env = TestEnv::new(None);
    let id = env.add("Polite").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.until_quit().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());

    manager.connect(&id).await.unwrap();
    until("running", LONG, || manager.snapshot(&id).running).await;
    let start = tokio::time::Instant::now();
    manager.disconnect(&id);
    until("stopped", LONG, || {
        manager.snapshot(&id).state == SessionState::Stopped
    })
    .await;
    assert!(secs(tokio::time::Instant::now() - start) < 1.0);

    // Typing /quit in the console is a stop too, not a crash to recover from.
    manager.connect(&id).await.unwrap();
    until("running again", LONG, || manager.snapshot(&id).running).await;
    manager.send(&id, "/quit").unwrap();
    until("stopped again", LONG, || {
        !manager.snapshot(&id).running && manager.snapshot(&id).state == SessionState::Stopped
    })
    .await;
    tokio::time::sleep(Duration::from_secs(120)).await;
    assert_eq!(s.launch_count(), 2);
    assert!(matches!(manager.send(&id, "hi"), Err(AfkError::NotRunning)));
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn console_masks_login_commands_and_tracks_the_script_file() {
    let env = TestEnv::new(None);
    let id = env.add("Masked").id;
    let mut form = env.store.settings_form(&id).unwrap();
    form.login_commands.enabled = true;
    form.login_commands.commands = vec!["/login hunter22".into(), "/kit".into()];
    form.auto_respond.enabled = true;
    form.auto_respond.sound_enabled = true;
    form.auto_respond.rules = vec![pl_afk::AutoRespondRule {
        pattern: "selam".into(),
        reply: "Merhaba".into(),
        ..Default::default()
    }];
    env.store.save_settings(&id, &form).unwrap();

    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.out("[MCC] Sunucuya bağlanılıyor...").await;
        proc.event(r#"{"e":"connecting","host":"h","port":25565}"#)
            .await;
        proc.event(r#"{"e":"joined","user":"Masked","host":"h","port":25565,"protocol":777}"#)
            .await;
        proc.out("[Script] send /login hunter22").await;
        proc.out("[Script] send /kit").await;
        proc.err("Unhandled warning").await;
        proc.event(r#"{"e":"health","hp":19.5,"food":17}"#).await;
        proc.event(r#"{"e":"players","n":3}"#).await;
        proc.event(r#"{"e":"autorespond","u":"Hazar"}"#).await;
        proc.out("@@MCC@@{broken").await;
        proc.until_quit().await
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("connected", LONG, || {
        matches!(manager.snapshot(&id).state, SessionState::Connected { .. })
    })
    .await;
    until("stats", LONG, || {
        manager.snapshot(&id).stats.players == Some(3)
    })
    .await;

    // The script existed for the client and was deleted once the game was joined.
    let launch = s.launch(0);
    assert!(
        launch
            .login_script
            .unwrap()
            .contains("send /login hunter22\r\n")
    );
    assert!(!env.login_script_exists(&id));
    assert_eq!(launch.spec.cwd, env.account_dir(&id));
    assert_eq!(
        launch.spec.ini,
        env.account_dir(&id).join("MinecraftClient.ini")
    );
    assert!(launch.ini.contains("BotOwners = [ \"Hazar\" ]"));

    let console = manager.console(&id);
    let texts: Vec<(&str, ConsoleLineKind, ConsoleOrigin)> = console
        .iter()
        .map(|l| (l.text.as_str(), l.kind, l.origin))
        .collect();
    assert_eq!(
        texts,
        vec![
            (
                "[MCC] Sunucuya bağlanılıyor...",
                ConsoleLineKind::Normal,
                ConsoleOrigin::Client
            ),
            (
                "[Script] send /login ******",
                ConsoleLineKind::Sensitive,
                ConsoleOrigin::Client
            ),
            (
                "[Script] send /kit",
                ConsoleLineKind::Script,
                ConsoleOrigin::Client
            ),
            (
                "[hata] Unhandled warning",
                ConsoleLineKind::Normal,
                ConsoleOrigin::ClientError
            ),
        ]
    );
    assert_eq!(
        manager.snapshot(&id).stats,
        SessionStats {
            hp: Some(19.5),
            food: Some(17),
            players: Some(3)
        }
    );
    assert!(env.events.lock().iter().any(|e| e.kind
        == AfkEventKind::AutoRespond {
            user: Some("Hazar".into()),
            sound: true
        }));

    // User input: echoed (masked when sensitive) and written to stdin.
    manager.send(&id, "/l topsecret99").unwrap();
    manager.send(&id, "selam herkese").unwrap();
    until("input delivered", LONG, || s.received.lock().len() == 2).await;
    assert_eq!(
        s.received.lock().as_slice(),
        ["/l topsecret99", "selam herkese"]
    );
    let echoes: Vec<String> = manager
        .console(&id)
        .into_iter()
        .filter(|l| l.origin == ConsoleOrigin::User)
        .map(|l| l.text)
        .collect();
    assert_eq!(echoes, vec!["> /l ******", "> selam herkese"]);

    // An in-client reconnect needs the script again.
    manager.send(&id, "/reco").unwrap();
    until("script restored", LONG, || env.login_script_exists(&id)).await;

    manager.disconnect(&id);
    until("stopped", LONG, || {
        manager.snapshot(&id).state == SessionState::Stopped
    })
    .await;
    assert!(
        !env.login_script_exists(&id),
        "no password file after the session"
    );
    assert_eq!(manager.snapshot(&id).stats, SessionStats::default());

    let everything = env.all_event_json();
    assert!(!everything.contains("hunter22") && !everything.contains("topsecret99"));
    let console_log = env.account_dir(&id).join("konsol.txt");
    until("console log written", LONG, || {
        std::fs::read_to_string(&console_log).is_ok_and(|t| t.contains("send /kit"))
    })
    .await;
    let log = std::fs::read_to_string(&console_log).unwrap();
    assert!(
        !log.contains("hunter22") && !log.contains("topsecret99"),
        "{log}"
    );
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn connect_all_is_staggered_with_a_live_countdown() {
    let env = TestEnv::new(None);
    let ids: Vec<String> = ["Birinci", "Ikinci", "Ucuncu"]
        .iter()
        .map(|n| env.add(n).id)
        .collect();
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"joined","user":"x","host":"h","port":25565,"protocol":777}"#)
            .await;
        proc.until_quit().await
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    let start = tokio::time::Instant::now();
    let bulk = tokio::spawn({
        let manager = manager.clone();
        async move { manager.connect_all().await }
    });
    until("bulk started", LONG, || manager.bulk_running()).await;
    assert!(
        manager.any_active(),
        "a running bulk connect keeps the launcher alive"
    );
    assert!(matches!(manager.connect_all().await, Err(AfkError::Busy)));

    let outcome = bulk.await.unwrap().unwrap();
    assert_eq!(outcome.started, ids);
    assert!(!outcome.cancelled);
    assert!(
        manager.any_active(),
        "started accounts count as active immediately"
    );
    until("third launch", LONG, || s.launch_count() == 3).await;
    let offsets: Vec<u64> = s
        .launches
        .lock()
        .iter()
        .map(|l| (l.at - start).as_secs_f64().round() as u64)
        .collect();
    assert_eq!(offsets, vec![0, 15, 30]);
    let expected: Vec<u32> = (0..=15).rev().collect();
    assert!(
        env.countdowns(&ids[0]).is_empty(),
        "the first account connects at once"
    );
    assert_eq!(env.countdowns(&ids[1]), expected);
    assert_eq!(env.countdowns(&ids[2]), expected);

    // Already connected accounts are skipped.
    let again = manager.connect_all().await.unwrap();
    assert_eq!(again.skipped, ids);
    assert_eq!(s.launch_count(), 3);

    manager.stop_all().await;
    assert!(!manager.any_active());
    for id in &ids {
        assert_eq!(manager.snapshot(id).state, SessionState::Stopped);
    }
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn cancelling_a_bulk_connect_stops_the_remaining_accounts() {
    let env = TestEnv::new(None);
    let ids: Vec<String> = ["Birinci", "Ikinci", "Ucuncu"]
        .iter()
        .map(|n| env.add(n).id)
        .collect();
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.until_quit().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    let bulk = tokio::spawn({
        let manager = manager.clone();
        async move { manager.connect_all().await }
    });
    until("second account", LONG, || s.launch_count() == 2).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    manager.cancel_bulk();
    let outcome = bulk.await.unwrap().unwrap();
    assert!(outcome.cancelled);
    assert_eq!(outcome.started, ids[..2].to_vec());
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(s.launch_count(), 2);
    assert_eq!(
        env.countdowns(&ids[2]).last(),
        Some(&0),
        "countdown cleared"
    );
    assert!(!manager.bulk_running());
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn auto_start_is_staggered_too() {
    let env = TestEnv::new(None);
    let first = env.add("Oto1").id;
    let _manual = env.add("Elle").id;
    let second = env.add("Oto2").id;
    for id in [&first, &second] {
        env.update(id, |a| a.auto_start = true);
    }
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.until_quit().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    let start = tokio::time::Instant::now();
    let outcome = manager.auto_start().await.unwrap();
    assert_eq!(outcome.started, vec![first.clone(), second.clone()]);
    until("second launch", LONG, || s.launch_count() == 2).await;
    let launches = s.launches.lock().clone();
    assert_eq!(launches.len(), 2);
    assert_eq!((launches[1].at - start).as_secs_f64().round() as u64, 15);
    assert_eq!(launches[1].spec.cwd, env.account_dir(&second));
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn removing_an_account_stops_its_client_and_deletes_everything() {
    let env = TestEnv::new(None);
    let id = env.add("Gone").id;
    env.store
        .set_login_commands(&id, &["/login pw1234".to_owned()])
        .unwrap();
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.until_quit().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("running", LONG, || manager.snapshot(&id).running).await;
    manager.remove_account(&id).await.unwrap();
    assert!(env.store.account(&id).is_none());
    assert!(!env.account_dir(&id).exists());
    assert!(env.secrets.entries().is_empty());
    assert!(!manager.any_active());
    assert!(matches!(
        manager.connect(&id).await,
        Err(AfkError::AccountNotFound(_))
    ));
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn missing_client_fails_the_connect() {
    let env = TestEnv::new(None);
    let id = env.add("NoClient").id;
    let runtime = Arc::new(pl_afk::AfkRuntime::new(env.paths.bin_dir()));
    let manager = pl_afk::AfkManager::new(
        env.store.clone(),
        runtime,
        env.sink(),
        pl_afk::ManagerOptions::default(),
    );
    assert!(matches!(
        manager.connect(&id).await,
        Err(AfkError::ClientNotInstalled)
    ));
    let outcome = manager.connect_all().await.unwrap();
    assert_eq!(outcome.failed, vec![id.clone()]);
    assert_eq!(manager.snapshot(&id).state, SessionState::Stopped);
}
