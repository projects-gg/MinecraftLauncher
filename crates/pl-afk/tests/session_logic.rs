//! Session state machine and manager scheduling under paused tokio time, against an in-memory
//! client (see `common::ScriptedSpawner`). Durations are virtual, so the real 15-60 s timings are
//! checked exactly.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{ScriptedSpawner, TestEnv, is_error, until};
use pl_afk::{
    AfkError, AfkEventKind, ClientSpawner, ConsoleLineKind, ConsoleOrigin, FailureReason,
    KickCause, ManagerConfig, SessionState, SessionStats,
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

/// [`gaps`] in fractional seconds.
fn exact_gaps(spawner: &ScriptedSpawner, run: Duration) -> Vec<f64> {
    let launches = spawner.launches.lock();
    launches
        .windows(2)
        .map(|w| (w[1].at - (w[0].at + run)).as_secs_f64())
        .collect()
}

/// Each restart waits its base delay ±20 % (`RestartSchedule::spread_delay`), so clients that
/// dropped together do not all come back in the same second. Gaps are whole seconds.
fn assert_gaps_near(measured: &[u64], base: &[u64]) {
    assert_eq!(measured.len(), base.len(), "{measured:?}");
    for (gap, base) in measured.iter().zip(base) {
        let (low, high) = (base * 8 / 10, base * 12 / 10 + 1);
        assert!(
            (low..=high).contains(gap),
            "restart gap {gap}s outside {low}..={high}s (base {base}s): {measured:?}"
        );
    }
}

/// The legacy back-off: the account's reconnect delay set to 15-60 s.
fn legacy_delays(env: &TestEnv, id: &str) {
    env.update(id, |a| {
        a.auto_relog.delay_min = 15.0;
        a.auto_relog.delay_max = 60.0;
    });
}

#[tokio::test(start_paused = true)]
async fn restart_backoff_ramps_through_the_account_delay_with_spread() {
    let env = TestEnv::new(None);
    let id = env.add("Backoff").id;
    legacy_delays(&env, &id);
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"connecting","host":"h","port":25565}"#)
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        Some(3)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();

    until("six launches", LONG, || s.launch_count() >= 6).await;
    // 15-60 s: 15 s, then doubling (and the ramp) up to 60 s, then the steps towards 4 minutes.
    assert_gaps_near(&gaps(&s, Duration::from_secs(1)), &[15, 30, 60, 90, 120]);

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
async fn new_accounts_reconnect_after_their_short_default_delay() {
    let env = TestEnv::new(None);
    let id = env.add("Hizli").id;
    let s = ScriptedSpawner::new(|_, _proc| async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Some(3)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("five launches", LONG, || s.launch_count() >= 5).await;
    // 5-10 s: 5 s doubling while the client keeps failing, each ±20 %.
    let measured = exact_gaps(&s, Duration::from_secs(1));
    assert_eq!(measured.len(), 4, "{measured:?}");
    for (gap, base) in measured.iter().zip([5.0, 10.0, 20.0, 40.0]) {
        assert!(
            (base * 0.8 - 0.01..=base * 1.2 + 0.01).contains(gap),
            "gap {gap:.2}s outside ±20 % of {base}s: {measured:?}"
        );
    }

    // Changed while the fifth client runs: its restart already waits the new delay (longer than
    // the back-off's four minutes, so the account's own delay is what counts).
    env.update(&id, |a| {
        a.auto_relog.delay_min = 300.0;
        a.auto_relog.delay_max = 300.0;
    });
    until("sixth launch", LONG, || s.launch_count() >= 6).await;
    let gap = exact_gaps(&s, Duration::from_secs(1))[4];
    assert!((240.0..=360.01).contains(&gap), "{gap}");
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
async fn a_lasting_connection_resets_the_backoff() {
    let env = TestEnv::new(None);
    let id = env.add("Rejoin").id;
    legacy_delays(&env, &id);
    let s = ScriptedSpawner::new(|run, mut proc| async move {
        if run == 1 {
            proc.event(r#"{"e":"joined","user":"Rejoin","host":"h","port":25565,"protocol":777}"#)
                .await;
            // In game for two and a half minutes: the connection worked.
            tokio::time::sleep(Duration::from_secs(150)).await;
        } else {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Some(3)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("three launches", LONG, || s.launch_count() >= 3).await;
    let launches = s.launches.lock().clone();
    let first = (launches[1].at - (launches[0].at + Duration::from_secs(1))).as_secs_f64();
    let second = (launches[2].at - (launches[1].at + Duration::from_secs(150))).as_secs_f64();
    // Without the lasting connection the second gap would be 30 s (24..=36 s with the spread).
    for gap in [first, second] {
        assert!((12.0..=18.01).contains(&gap), "{first} {second}");
    }
    manager.shutdown().await;
}

/// Joined, then kicked a second later, again and again (a login plugin, an anti-bot check): the
/// short join is not a working connection, so the restarts back off instead of hammering the
/// server every few seconds.
#[tokio::test(start_paused = true)]
async fn a_quick_kick_after_joining_keeps_backing_off() {
    let env = TestEnv::new(None);
    let id = env.add("Atilan").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"joined","user":"Atilan","host":"h","port":25565,"protocol":777}"#)
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        proc.event(r#"{"e":"disconnected","reason":"InGameKick","msg":"Spam yüzünden atıldınız"}"#)
            .await;
        Some(2)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("six launches", LONG, || s.launch_count() >= 6).await;
    // 5 s doubling up to about a minute, each ±20 %.
    assert_gaps_near(&gaps(&s, Duration::from_secs(1)), &[5, 10, 20, 40, 60]);
    manager.shutdown().await;
}

/// A ban does not go away by reconnecting: the session stops with the reason.
#[tokio::test(start_paused = true)]
async fn a_ban_stops_reconnecting() {
    let env = TestEnv::new(None);
    let id = env.add("Yasakli").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"disconnected","reason":"LoginRejected","msg":"§cYou are banned from this server!"}"#)
            .await;
        Some(4)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("stopped by the ban", LONG, || {
        is_error(&env.last_state(&id)) && !manager.any_active()
    })
    .await;
    assert_eq!(
        env.last_state(&id),
        Some(SessionState::Error {
            reason: FailureReason::Refused {
                cause: KickCause::Banned,
                message: Some("You are banned from this server!".into()),
            },
            retry_limit_reached: false,
        })
    );
    tokio::time::sleep(Duration::from_secs(600)).await;
    assert_eq!(s.launch_count(), 1, "no reconnect after a ban");
    manager.shutdown().await;
}

/// "Tümünü bağla" leaves an account the server stopped for good (here a refused password: trying
/// it again can lock the account on the server) and connects the others (AFK-CLEAR).
#[tokio::test(start_paused = true)]
async fn connect_all_leaves_an_account_stopped_for_good() {
    let env = TestEnv::new(None);
    let refused = env.add("Sifresiz").id;
    let other = env.add("Diger").id;
    let s = ScriptedSpawner::new(|run, mut proc| async move {
        if run == 0 {
            proc.event(r#"{"e":"disconnected","reason":"LoginRejected","msg":"§cYanlış şifre!"}"#)
                .await;
            Some(4)
        } else {
            proc.event(r#"{"e":"joined","user":"Diger","host":"h","port":25565,"protocol":777}"#)
                .await;
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Some(0)
        }
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&refused).await.unwrap();
    until("stopped on the password", LONG, || {
        is_error(&env.last_state(&refused)) && !manager.any_active()
    })
    .await;
    assert!(matches!(
        env.last_state(&refused),
        Some(SessionState::Error {
            reason: FailureReason::Refused {
                cause: KickCause::WrongPassword,
                ..
            },
            ..
        })
    ));
    let outcome = manager.connect_all().await.unwrap();
    assert_eq!(outcome.skipped, vec![refused.clone()]);
    assert_eq!(outcome.started, vec![other.clone()]);
    // The client starts a moment after the connect returns.
    until("the other one runs", LONG, || s.launch_count() >= 2).await;
    let refused_dir = env.account_dir(&refused);
    let tries = s
        .launches
        .lock()
        .iter()
        .filter(|launch| launch.spec.cwd == refused_dir)
        .count();
    assert_eq!(tries, 1, "the refused password was not tried again");
    manager.shutdown().await;
}

/// "Already connected" can be the server still holding a dropped session for a few seconds, so
/// it is retried (backing off); five in a row mean the name really is online elsewhere.
#[tokio::test(start_paused = true)]
async fn already_connected_stops_after_five_in_a_row() {
    let env = TestEnv::new(None);
    let id = env.add("Ikiz").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"disconnected","reason":"LoginRejected","msg":"You are already connected to this proxy!"}"#)
            .await;
        Some(4)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("gave up on the twin", LONG, || {
        is_error(&env.last_state(&id)) && !manager.any_active()
    })
    .await;
    assert_eq!(s.launch_count(), 5);
    assert_gaps_near(&gaps(&s, Duration::ZERO), &[5, 10, 20, 40]);
    assert_eq!(
        env.last_state(&id),
        Some(SessionState::Error {
            reason: FailureReason::Refused {
                cause: KickCause::AlreadyConnected,
                message: Some("You are already connected to this proxy!".into()),
            },
            retry_limit_reached: false,
        })
    );
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

    // User input: written to stdin, never echoed by the launcher (the server's answer is the
    // feedback), and the password never reaches the console.
    manager.send(&id, "/l topsecret99").unwrap();
    manager.send(&id, "selam herkese").unwrap();
    until("input delivered", LONG, || s.received.lock().len() == 2).await;
    assert_eq!(
        s.received.lock().as_slice(),
        ["/l topsecret99", "selam herkese"]
    );
    let console = manager.console(&id);
    assert!(console.iter().all(|l| l.origin != ConsoleOrigin::User));
    assert!(console.iter().all(|l| !l.text.contains("topsecret99")));

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
    // A real thread writes the file: waited for on the real clock (the paused one rushes ahead
    // while nothing else is due, a deadline of 600 s on it passes in moments).
    let deadline = std::time::Instant::now() + Duration::from_secs(300);
    while !std::fs::read_to_string(&console_log).is_ok_and(|t| t.contains("send /kit")) {
        assert!(
            std::time::Instant::now() < deadline,
            "console log not written"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
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
async fn an_account_being_removed_is_never_started_again() {
    let env = TestEnv::new(None);
    let id = env.add("Gidiyor").id;
    // The client ignores `/quit`: the removal waits for the kill after the grace period.
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.hang().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("running", LONG, || manager.snapshot(&id).running).await;

    let removal = tokio::spawn({
        let manager = manager.clone();
        let id = id.clone();
        async move { manager.remove_account(&id).await }
    });
    until("stopping", LONG, || {
        env.last_state(&id) == Some(SessionState::Stopping)
    })
    .await;
    // A connect meanwhile (a bulk connect reaching the account, a click) must not start it.
    assert!(matches!(
        manager.connect(&id).await,
        Err(AfkError::AccountNotFound(_))
    ));
    removal.await.unwrap().unwrap();
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(s.launch_count(), 1);
    assert!(!manager.any_active());
    assert!(env.store.account(&id).is_none());
    assert!(!env.account_dir(&id).exists());
    assert!(matches!(
        manager.remove_account(&id).await,
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
    // No build installed, none known, nothing to refresh the index with (client_prepare.rs covers
    // the refresh): the connect fails with what the card tells the player.
    assert!(matches!(
        manager.connect(&id).await,
        Err(AfkError::ClientUnavailable {
            problem: pl_afk::ClientProblem::NotPublished,
            ..
        })
    ));
    let outcome = manager.connect_all().await.unwrap();
    assert_eq!(outcome.failed, vec![id.clone()]);
    assert_eq!(manager.snapshot(&id).state, SessionState::Stopped);
    assert_eq!(
        manager.client_step(&id),
        Some(pl_afk::ClientStep::Failed {
            problem: pl_afk::ClientProblem::NotPublished
        })
    );
}

#[tokio::test(start_paused = true)]
async fn an_update_restart_brings_back_running_and_queued_accounts() {
    let env = TestEnv::new(None);
    let ids: Vec<String> = ["Birinci", "Ikinci", "Ucuncu"]
        .iter()
        .map(|n| env.add(n).id)
        .collect();
    let never_started = env.add("Bekleyen").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.until_quit().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    assert!(manager.resume_candidates().is_empty(), "nothing runs yet");

    // The three connect one by one (a removed account's id is skipped).
    let mut asked = ids.clone();
    asked.push("silinmis".into());
    let bulk = tokio::spawn({
        let manager = manager.clone();
        async move { manager.connect_staggered(&asked).await }
    });
    until("first launch", LONG, || s.launch_count() == 1).await;
    assert_eq!(
        manager.resume_candidates(),
        ids,
        "the one connecting and the two still waiting in the queue"
    );
    assert!(!manager.resume_candidates().contains(&never_started));

    // Stopped by the player meanwhile: not brought back.
    manager.disconnect(&ids[0]);
    until("first stopped", LONG, || {
        manager.snapshot(&ids[0]).state == SessionState::Stopped
    })
    .await;
    assert_eq!(manager.resume_candidates(), ids[1..].to_vec());

    // The update stops everything, the queue included.
    manager.stop_all().await;
    assert!(bulk.await.unwrap().unwrap().cancelled);
    assert!(manager.resume_candidates().is_empty());
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn auto_start_also_connects_the_accounts_an_update_stopped() {
    let env = TestEnv::new(None);
    let auto = env.add("Oto").id;
    let stopped_by_update = env.add("Gece").id;
    let _left_alone = env.add("Elle").id;
    env.update(&auto, |a| a.auto_start = true);
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.until_quit().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    let start = tokio::time::Instant::now();
    let outcome = manager
        .auto_start_with(&[stopped_by_update.clone(), "silinmis".into()])
        .await
        .unwrap();
    assert_eq!(
        outcome.started,
        vec![auto.clone(), stopped_by_update.clone()]
    );
    until("second launch", LONG, || s.launch_count() == 2).await;
    let launches = s.launches.lock().clone();
    assert_eq!(launches.len(), 2, "the account nobody asked for stays off");
    assert_eq!(
        (launches[1].at - start).as_secs_f64().round() as u64,
        15,
        "the same staggered queue as \"connect all\""
    );
    assert_eq!(launches[1].spec.cwd, env.account_dir(&stopped_by_update));
    manager.shutdown().await;
}

/// Connections to one server go one by one, in the order they asked (`pacing`): three accounts
/// asking together start a spacing apart, and the first one's reconnect after the server dropped
/// it waits behind the two that asked before it. Real clients: `session_e2e.rs`.
#[tokio::test(start_paused = true)]
async fn connections_to_one_server_go_one_by_one_in_order() {
    let env = TestEnv::new(None);
    let ids: Vec<String> = ["Birinci", "Ikinci", "Ucuncu"]
        .iter()
        .map(|n| env.add(n).id)
        .collect();
    // The first client is dropped at once (its server restarted); every other one stays in game.
    let s = ScriptedSpawner::new(|run, mut proc| async move {
        if run == 0 {
            proc.event(r#"{"e":"disconnected","reason":"ConnectionLost","msg":"Sunucu kapandı"}"#)
                .await;
            return Some(3);
        }
        proc.event(r#"{"e":"joined","user":"x","host":"h","port":25565,"protocol":777}"#)
            .await;
        proc.until_quit().await
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    let start = tokio::time::Instant::now();
    for id in &ids {
        manager.connect(id).await.unwrap();
        // The actor takes its slot as it picks the start up, before this thread runs anything else.
        until("start picked up", LONG, || {
            manager.snapshot(id).state != SessionState::Stopped
        })
        .await;
    }
    until("four launches", LONG, || s.launch_count() == 4).await;

    let launches = s.launches.lock().clone();
    let order: Vec<_> = launches.iter().map(|l| l.spec.cwd.clone()).collect();
    let asked: Vec<_> = [&ids[0], &ids[1], &ids[2], &ids[0]]
        .iter()
        .map(|id| env.account_dir(id))
        .collect();
    assert_eq!(order, asked);
    // `MIN_CONNECT_SPACING` (4.5 s) apart from the first ask: the reconnect, due after its 4-6 s
    // delay, goes after the slot the third account took.
    let offsets: Vec<f64> = launches.iter().map(|l| secs(l.at - start)).collect();
    for (offset, expected) in offsets.iter().zip([0.0, 4.5, 9.0, 13.5]) {
        assert!((offset - expected).abs() < 0.1, "{offsets:?}");
    }
    // Waiting for its turn, the reconnect showed when it would go, on top of its delay's own time.
    let shown = env
        .states(&ids[0])
        .into_iter()
        .filter(|state| {
            matches!(
                state,
                SessionState::Reconnecting {
                    next_attempt_at: Some(_),
                    ..
                }
            )
        })
        .count();
    assert_eq!(shown, 2, "{:?}", env.states(&ids[0]));
    // A first start that waits for its slot stays "starting".
    for id in &ids[1..] {
        assert!(
            !env.states(id)
                .iter()
                .any(|state| matches!(state, SessionState::Reconnecting { .. })),
            "{id}"
        );
    }
    manager.shutdown().await;
}

/// A client of 4.1.0 or later gets a bridge nonce, and only lines with it are events: a server's
/// chat or kick text that starts with the bare bridge prefix (or guesses a nonce) stays console
/// text and fakes neither a join nor a disconnect (the fork's "Stdout bridge hardening").
#[tokio::test(start_paused = true)]
async fn forged_bridge_lines_stay_text_for_a_client_with_a_nonce() {
    let env = TestEnv::with_client_version("4.1.0");
    let id = env.add("NonceBot").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        // Before the real join: a server line that would fake one.
        proc.forged(r#"{"e":"joined","user":"NonceBot","host":"h","port":25565,"protocol":777}"#)
            .await;
        proc.event(r#"{"e":"connecting","host":"h","port":25565}"#)
            .await;
        proc.event(r#"{"e":"joined","user":"NonceBot","host":"h","port":25565,"protocol":777}"#)
            .await;
        // In the game: a kick text that would fake a disconnect, and one with a guessed nonce.
        proc.forged(r#"{"e":"disconnected","reason":"InGameKick","msg":"You are banned"}"#)
            .await;
        let guessed = "0".repeat(32);
        proc.out(&format!(
            r#"{guessed}@@MCC@@{{"e":"disconnected","reason":"LoginRejected","msg":"x"}}"#
        ))
        .await;
        proc.event(r#"{"e":"players","n":4}"#).await;
        proc.until_quit().await
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    // The real event after the forged ones has arrived, so they have been read too.
    until("the players count", LONG, || {
        manager.snapshot(&id).stats.players == Some(4)
    })
    .await;

    let nonce = s.launch(0).spec.nonce.expect("a 4.1.0 client gets a nonce");
    assert!(pl_afk::bridge::is_valid_nonce(&nonce), "{nonce}");
    assert!(matches!(
        manager.snapshot(&id).state,
        SessionState::Connected { .. }
    ));
    let states = env.states(&id);
    assert!(
        !states.iter().any(|state| matches!(
            state,
            SessionState::Reconnecting { .. } | SessionState::Error { .. }
        )),
        "{states:?}"
    );
    // Every forged line is plain console text; the nonce is in no line and in no event.
    let console = manager.console(&id);
    let texts: Vec<&str> = console.iter().map(|l| l.text.as_str()).collect();
    for start in [
        r#"@@MCC@@{"e":"joined""#,
        r#"@@MCC@@{"e":"disconnected""#,
        "00000000000000000000000000000000@@MCC@@",
    ] {
        assert!(
            texts.iter().any(|t| t.starts_with(start)),
            "{start}: {texts:?}"
        );
    }
    assert!(texts.iter().all(|t| !t.contains(&nonce)), "{texts:?}");
    assert!(!env.all_event_json().contains(&nonce));
    manager.shutdown().await;
}

/// Each client process has its own nonce: a restarted client never reuses the last one.
#[tokio::test(start_paused = true)]
async fn every_client_process_gets_its_own_nonce() {
    let env = TestEnv::with_client_version("4.2.0");
    let id = env.add("YeniSurec").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"connecting","host":"h","port":25565}"#)
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        Some(3)
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("two launches", LONG, || s.launch_count() >= 2).await;
    let first = s.launch(0).spec.nonce.expect("first nonce");
    let second = s.launch(1).spec.nonce.expect("second nonce");
    assert_ne!(first, second);
    manager.shutdown().await;
}

/// The version check never fails open: a `v` or a missing minor and patch still read as a version,
/// and a release whose version does not read as one counts as a new client.
#[tokio::test(start_paused = true)]
async fn odd_release_versions_still_get_a_nonce() {
    for version in ["v4.1.0", "5", "garbage"] {
        let env = TestEnv::with_client_version(version);
        let id = env.add("TuhafSurum").id;
        let s = ScriptedSpawner::new(|_, mut proc| async move {
            proc.event(
                r#"{"e":"joined","user":"TuhafSurum","host":"h","port":25565,"protocol":777}"#,
            )
            .await;
            proc.until_quit().await
        });
        let manager = env.manager(spawner(&s), ManagerConfig::default());
        manager.connect(&id).await.unwrap();
        until("connected by a nonce line", LONG, || {
            matches!(manager.snapshot(&id).state, SessionState::Connected { .. })
        })
        .await;
        assert!(s.launch(0).spec.nonce.is_some(), "{version}");
        manager.shutdown().await;
    }
}

/// A client before 4.1.0 (the v1 launcher's one adopted without a version file too) ignores the
/// variable and a development build's version is unknown: none gets a nonce, and their bare event
/// lines still count, as before.
#[tokio::test(start_paused = true)]
async fn clients_before_4_1_and_dev_builds_get_no_nonce_and_keep_their_events() {
    for env in [
        TestEnv::with_client_version("4.0.1"),
        TestEnv::with_client_version(pl_afk::bridge::LEGACY_CLIENT_VERSION),
        TestEnv::new(None),
    ] {
        let id = env.add("EskiSurum").id;
        let s = ScriptedSpawner::new(|_, mut proc| async move {
            proc.event(
                r#"{"e":"joined","user":"EskiSurum","host":"h","port":25565,"protocol":777}"#,
            )
            .await;
            proc.until_quit().await
        });
        let manager = env.manager(spawner(&s), ManagerConfig::default());
        manager.connect(&id).await.unwrap();
        until("connected by a bare event line", LONG, || {
            matches!(manager.snapshot(&id).state, SessionState::Connected { .. })
        })
        .await;
        assert_eq!(s.launch(0).spec.nonce, None);
        manager.shutdown().await;
    }
}
