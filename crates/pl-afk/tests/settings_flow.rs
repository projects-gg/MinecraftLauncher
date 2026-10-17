//! The settings page against live sessions: saving while a client runs, the "reconnect to apply"
//! signal and the reconnect itself (paused tokio time, in-memory client).

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{ScriptedSpawner, TestEnv, until};
use pl_afk::{
    AfkError, AutoRespondRule, ClientSpawner, LoginCommandLine, ManagerConfig, PatternProblem,
    SessionState, ValidationIssue, VersionCatalog,
};

const LONG: Duration = Duration::from_secs(600);

fn spawner(s: &Arc<ScriptedSpawner>) -> Option<Arc<dyn ClientSpawner>> {
    Some(s.clone())
}

#[tokio::test(start_paused = true)]
async fn saving_while_connected_asks_for_a_reconnect_that_applies_the_settings() {
    let env = TestEnv::new(None);
    let id = env.add("Ayarci").id;
    env.store
        .set_login_commands(&id, &["/login hunter22".to_owned()])
        .unwrap();
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.until_quit().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.connect(&id).await.unwrap();
    until("running", LONG, || manager.snapshot(&id).running).await;
    assert!(!manager.settings_changed(&id));

    // Saving the page unchanged: nothing to apply.
    let page = manager.settings_page(&id).unwrap();
    let saved = manager.save_settings_page(&id, page.clone()).unwrap();
    assert!(!saved.reconnect_needed);
    assert!(!manager.settings_changed(&id));

    // A real change, keeping the stored password without ever seeing it.
    let mut changed = page;
    changed.server_host = "yeni.sunucu.net".into();
    changed.login_commands.enabled = true;
    assert!(matches!(
        changed.login_commands.commands.as_slice(),
        [LoginCommandLine::Stored { .. }]
    ));
    let saved = manager.save_settings_page(&id, changed).unwrap();
    assert!(saved.reconnect_needed);
    assert!(manager.settings_changed(&id));
    assert!(s.launch(0).ini.contains("play.projects.gg"));

    manager.reconnect(&id).await.unwrap();
    until("relaunched", LONG, || {
        s.launch_count() == 2 && manager.snapshot(&id).running
    })
    .await;
    assert!(s.received.lock().iter().any(|line| line == "/quit"));
    let relaunch = s.launch(1);
    assert!(
        relaunch
            .ini
            .contains("Server = { Host = \"yeni.sunucu.net\" }"),
        "{}",
        relaunch.ini
    );
    assert!(
        relaunch
            .login_script
            .is_some_and(|script| script.contains("send /login hunter22")),
        "the kept secret reaches the client"
    );
    assert!(!manager.settings_changed(&id));
    assert!(env.states(&id).contains(&SessionState::Stopping));

    // The secret never went through an event either.
    assert!(!env.all_event_json().contains("hunter22"));
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn reconnect_kills_a_stuck_client_and_connects_idle_accounts() {
    let env = TestEnv::new(None);
    let stuck = env.add("Inatci").id;
    let idle = env.add("Bosta").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.hang().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());

    manager.connect(&stuck).await.unwrap();
    until("running", LONG, || manager.snapshot(&stuck).running).await;
    let started = tokio::time::Instant::now();
    manager.reconnect(&stuck).await.unwrap();
    let took = tokio::time::Instant::now() - started;
    assert!(
        took >= Duration::from_secs(4),
        "waited for the kill, took {took:?}"
    );
    until("relaunched", LONG, || s.launch_count() == 2).await;

    // Not running: a reconnect is a plain connect.
    manager.reconnect(&idle).await.unwrap();
    until("idle account launched", LONG, || s.launch_count() == 3).await;
    assert!(manager.snapshot(&idle).running);

    assert!(matches!(
        manager.reconnect("missing").await,
        Err(AfkError::AccountNotFound(_))
    ));
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn invalid_pages_save_nothing() {
    let env = TestEnv::new(None);
    let id = env.add("Hatali").id;
    let manager = env.manager(None, ManagerConfig::default());
    let mut page = manager.settings_page(&id).unwrap();
    page.server_host = "play projects gg".into();
    page.auto_respond.enabled = true;
    page.auto_start = true;
    match manager.save_settings_page(&id, page) {
        Err(AfkError::InvalidSettings(issues)) => assert_eq!(
            issues,
            vec![
                ValidationIssue::InvalidServerHost,
                ValidationIssue::AutoRespondNoRules
            ]
        ),
        other => panic!("expected validation issues, got {other:?}"),
    }
    assert!(!env.store.account(&id).unwrap().auto_start);
    assert_eq!(env.store.revision(&id), 0);
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn bad_auto_respond_patterns_save_nothing() {
    let env = TestEnv::new(None);
    let id = env.add("Desenci").id;
    let manager = env.manager(None, ManagerConfig::default());
    let mut page = manager.settings_page(&id).unwrap();
    page.auto_respond.enabled = true;
    page.auto_respond.rules = vec![
        AutoRespondRule {
            pattern: "selam".into(),
            reply: "merhaba".into(),
            ..AutoRespondRule::default()
        },
        AutoRespondRule {
            pattern: "(selam".into(),
            reply: "merhaba".into(),
            is_regex: true,
            ..AutoRespondRule::default()
        },
    ];
    match manager.save_settings_page(&id, page) {
        Err(AfkError::InvalidSettings(issues)) => assert_eq!(
            issues,
            vec![ValidationIssue::AutoRespondPattern {
                rule: 1,
                problem: PatternProblem::Invalid
            }]
        ),
        other => panic!("expected a pattern issue, got {other:?}"),
    }
    assert!(
        env.store
            .account(&id)
            .unwrap()
            .auto_respond
            .matches
            .is_empty()
    );
    assert_eq!(env.store.revision(&id), 0);
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_published_version_list_drives_pages_and_configs() {
    let env = TestEnv::new(None);
    let id = env.add("Surumcu").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move { proc.until_quit().await });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    assert_eq!(
        manager.settings_page(&id).unwrap().minecraft_version,
        "26.3",
        "the built-in list until the index has one"
    );

    manager.set_versions(VersionCatalog::from_release(
        [("26.4", "26.4"), ("26.3", "26.3")],
        Some("26.4"),
    ));
    let page = manager.settings_page(&id).unwrap();
    assert_eq!(page.minecraft_version, "26.4", "new accounts follow");
    manager.connect(&id).await.unwrap();
    until("running", LONG, || manager.snapshot(&id).running).await;
    assert!(s.launch(0).ini.contains("MinecraftVersion = \"26.4\""));

    // Pinned to 26.3, then a list without it: the stored value stays, the client gets the
    // recommendation.
    let mut pinned = page;
    pinned.minecraft_version = "26.3".into();
    manager.save_settings_page(&id, pinned).unwrap();
    assert_eq!(env.store.account(&id).unwrap().minecraft_version, "26.3");
    manager.set_versions(VersionCatalog::from_release([("26.5", "26.5")], None));
    assert_eq!(
        manager.settings_page(&id).unwrap().minecraft_version,
        "26.5"
    );
    manager.reconnect(&id).await.unwrap();
    until("relaunched", LONG, || s.launch_count() == 2).await;
    assert!(s.launch(1).ini.contains("MinecraftVersion = \"26.5\""));
    assert_eq!(
        env.store.account(&id).unwrap().minecraft_version,
        "26.3",
        "never rewritten"
    );

    // Back on the built-in list (nothing usable published): 26.3 is valid there again.
    manager.set_versions(None);
    assert_eq!(
        manager.settings_page(&id).unwrap().minecraft_version,
        "26.3"
    );
    manager.shutdown().await;
}
