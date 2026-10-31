//! Console looks and dialog cards on this computer (AFK-UX): the console kinds only with Studio's
//! switch and a client of 4.2.4 (from 4.2.5 its side events beside the classic console,
//! CONSOLE-ANY), a line's look kept only as the account's rules allow, and a press of the dialog
//! card checked and answered.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{ScriptedSpawner, TestEnv, until};
use pl_afk::{ClientSpawner, ConsoleFeatures, ConsoleLineKind, ManagerConfig, SessionState};

const LONG: Duration = Duration::from_secs(600);

fn spawner(s: &Arc<ScriptedSpawner>) -> Option<Arc<dyn ClientSpawner>> {
    Some(s.clone())
}

const ON: ConsoleFeatures = ConsoleFeatures {
    looks: true,
    dialogs: true,
};

fn rules_dialog(rev: u32) -> String {
    format!(
        r#"{{"e":"dialog","rev":{rev},"dialog":{{"title":"Kurallar","body":[],"inputs":[{{"key":"name","kind":"text","label":"Ad","max":16,"multiline":false,"initial":""}}],"buttons":[{{"i":0,"label":"Kur","a":"run_command","v":"/klan kur $(name)"}},{{"i":1,"label":"Kapat","a":"close"}}],"cancel":true,"play":true,"cols":1}}}}"#
    )
}

#[tokio::test(start_paused = true)]
async fn the_kinds_or_the_side_events_wait_for_the_switch_and_the_client() {
    for (version, features, kinds, side) in [
        ("4.2.4", ConsoleFeatures::default(), false, None),
        ("4.2.3", ON, false, None),
        ("4.2.4", ON, true, None),
        ("4.2.5", ConsoleFeatures::default(), false, None),
        ("4.2.5", ON, false, Some("looks,dialogs")),
        (
            "4.3.0",
            ConsoleFeatures {
                looks: false,
                dialogs: true,
            },
            false,
            Some("dialogs"),
        ),
    ] {
        let env = TestEnv::with_client_version(version);
        let id = env.add("LooksBot").id;
        let s = ScriptedSpawner::new(|_, mut proc| async move {
            proc.event(r#"{"e":"connecting","host":"h","port":25565}"#)
                .await;
            proc.until_quit().await
        });
        let manager = env.manager(spawner(&s), ManagerConfig::default());
        manager.set_console_features(features);
        manager.connect(&id).await.unwrap();
        until("the launch", LONG, || s.launch_count() == 1).await;
        assert_eq!(
            s.launch(0).spec.console_kinds,
            kinds,
            "{version} {features:?}"
        );
        assert_eq!(
            s.launch(0).spec.console_side.as_deref(),
            side,
            "{version} {features:?}"
        );
        manager.shutdown().await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_line_keeps_its_look_only_as_the_account_s_rules_allow() {
    let env = TestEnv::with_client_version("4.2.4");
    let id = env.add("LooksBot").id;
    let mut form = env.store.settings_form(&id).unwrap();
    form.login_commands.enabled = true;
    form.login_commands.commands = vec!["/login gizli123".into()];
    env.store.save_settings(&id, &form).unwrap();
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"connecting","host":"h","port":25565}"#)
            .await;
        proc.event(r#"{"e":"joined","user":"LooksBot","host":"h","port":25565,"protocol":777}"#)
            .await;
        // A server line in its colours with a command to click.
        proc.event(
            r#"{"e":"line","src":"system","text":"[Duyuru] Etkinlik başladı","st":[[8,"6l",0],[17,"f"]],"ev":[{"click":{"a":"run_command","v":"/etkinlik"}}]}"#,
        )
        .await;
        // A player's line: its sender is kept for the card.
        proc.event(
            r#"{"e":"line","src":"player","text":"<Kaan_07> selam","who":"Kaan_07","st":[[9,"e"],[6,""]]}"#,
        )
        .await;
        // What the server keeps on screen is no console line.
        proc.event(r#"{"e":"line","src":"overlay","text":"Coins: 120","st":[[10,"6"]]}"#)
            .await;
        proc.event(r#"{"e":"line","src":"title","text":"Hoş geldin"}"#)
            .await;
        // A line the mask changes keeps no look; an event with a secret goes.
        proc.event(
            r#"{"e":"line","src":"client","text":"[Script] send /login gizli123","st":[[29,"7"]]}"#,
        )
        .await;
        proc.event(
            r#"{"e":"line","src":"system","text":"Kodu kopyala","st":[[12,"a",0]],"ev":[{"click":{"a":"copy_to_clipboard","v":"gizli123"}}]}"#,
        )
        .await;
        proc.event(r#"{"e":"players","n":2}"#).await;
        proc.until_quit().await
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.set_console_features(ON);
    manager.connect(&id).await.unwrap();
    until("the last line", LONG, || {
        manager.snapshot(&id).stats.players == Some(2)
    })
    .await;

    let console = manager.console(&id);
    let texts: Vec<&str> = console.iter().map(|l| l.text.as_str()).collect();
    assert!(
        !texts
            .iter()
            .any(|t| t.contains("Coins") || t.contains("Hoş geldin")),
        "{texts:?}"
    );

    let news = console
        .iter()
        .find(|l| l.text == "[Duyuru] Etkinlik başladı")
        .unwrap();
    let look = news.looks.as_ref().expect("a server line keeps its look");
    assert_eq!(look.ev.len(), 1);
    assert_eq!(look.ev[0].click.as_ref().unwrap().v, "/etkinlik");
    assert_eq!(news.who, None);

    let chat = console
        .iter()
        .find(|l| l.text == "<Kaan_07> selam")
        .unwrap();
    assert!(chat.looks.is_some());
    assert_eq!(chat.who.as_deref(), Some("Kaan_07"));

    let script = console
        .iter()
        .find(|l| l.text.starts_with("[Script] send /login"))
        .unwrap();
    assert_eq!(script.kind, ConsoleLineKind::Sensitive);
    assert!(script.looks.is_none());
    assert!(!script.text.contains("gizli123"));

    let copy = console.iter().find(|l| l.text == "Kodu kopyala").unwrap();
    let look = copy.looks.as_ref().expect("its colour stays");
    assert!(look.ev.is_empty(), "the secret's copy is dropped");

    // Events and the log carry the text alone.
    assert!(
        env.lines(&id)
            .iter()
            .all(|l| l.looks.is_none() && l.who.is_none())
    );
    assert!(!env.all_event_json().contains("gizli123"));
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_dialog_press_is_checked_and_answered() {
    let env = TestEnv::with_client_version("4.2.4");
    let id = env.add("DialogBot").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"connecting","host":"h","port":25565,"caps":["dialog"]}"#)
            .await;
        proc.event(r#"{"e":"joined","user":"DialogBot","host":"h","port":25565,"protocol":777}"#)
            .await;
        proc.event(&rules_dialog(3)).await;
        // The press the card sends, answered as the client answers it.
        loop {
            let line = proc.read().await?;
            if line == "/quit" {
                return Some(0);
            }
            if line.starts_with("/dialog act ") {
                proc.event(r#"{"e":"dialogres","rev":3,"ok":true}"#).await;
                proc.event(r#"{"e":"dialog","rev":3,"clear":true}"#).await;
            }
        }
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.set_console_features(ON);
    manager.connect(&id).await.unwrap();
    until("the dialog", LONG, || manager.dialog(&id).is_some()).await;
    let dialog = manager.dialog(&id).unwrap();
    assert_eq!(dialog.rev, 3);
    assert!(dialog.json.contains("Kurallar"));

    // Refused here, never sent: another dialog, a button it has not, a command not confirmed.
    for (press, why) in [
        (r#"{"rev":2,"i":0}"#, "stale"),
        (r#"{"rev":3,"i":5}"#, "button"),
        (r#"{"rev":3,"i":0,"inputs":{"name":"Ejder"}}"#, "cmd"),
    ] {
        let outcome = manager.press_dialog(&id, press).await.unwrap();
        assert!(!outcome.ok, "{press}");
        assert_eq!(outcome.why.as_deref(), Some(why), "{press}");
    }
    assert!(s.received.lock().iter().all(|l| !l.starts_with("/dialog")));

    let outcome = manager
        .press_dialog(
            &id,
            r#"{"rev":3,"i":0,"inputs":{"name":"Ejder"},"cmd":"/klan kur Ejder"}"#,
        )
        .await
        .unwrap();
    assert!(outcome.ok, "{outcome:?}");
    let sent: Vec<String> = s
        .received
        .lock()
        .iter()
        .filter(|l| l.starts_with("/dialog act "))
        .cloned()
        .collect();
    assert_eq!(sent.len(), 1);
    let press: serde_json::Value = serde_json::from_str(&sent[0]["/dialog act ".len()..]).unwrap();
    assert_eq!(press["cmd"], "/klan kur Ejder");
    until("the dialog closes", LONG, || manager.dialog(&id).is_none()).await;
    assert!(matches!(
        manager.snapshot(&id).state,
        SessionState::Connected { .. }
    ));
    manager.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn no_card_without_the_switch_or_the_client_s_word() {
    for (features, caps) in [
        (
            ConsoleFeatures {
                looks: true,
                dialogs: false,
            },
            r#","caps":["dialog"]"#,
        ),
        (ON, ""),
    ] {
        let env = TestEnv::with_client_version("4.2.4");
        let id = env.add("DialogBot").id;
        let connecting = format!(r#"{{"e":"connecting","host":"h","port":25565{caps}}}"#);
        let s = ScriptedSpawner::new(move |_, mut proc| {
            let connecting = connecting.clone();
            async move {
                proc.event(&connecting).await;
                proc.event(&rules_dialog(1)).await;
                proc.event(r#"{"e":"players","n":1}"#).await;
                proc.until_quit().await
            }
        });
        let manager = env.manager(spawner(&s), ManagerConfig::default());
        manager.set_console_features(features);
        manager.connect(&id).await.unwrap();
        until("the players count", LONG, || {
            manager.snapshot(&id).stats.players == Some(1)
        })
        .await;
        assert!(manager.dialog(&id).is_none(), "{features:?} {caps}");
        let outcome = manager
            .press_dialog(&id, r#"{"rev":1,"i":1}"#)
            .await
            .unwrap();
        assert_eq!(outcome.why.as_deref(), Some("stale"));
        manager.shutdown().await;
    }
}

#[tokio::test]
async fn the_switches_are_remembered() {
    let env = TestEnv::with_client_version("4.2.4");
    let manager = env.manager(None, ManagerConfig::default());
    assert_eq!(manager.console_features(), ConsoleFeatures::default());
    manager.set_console_features(ON);
    manager.shutdown().await;
    // A launcher started again uses them before the offer is read.
    let (store, _) = pl_afk::AccountStore::open(env.paths.clone(), env.secrets.clone()).unwrap();
    let settings = store.afk_settings();
    assert!(settings.console_looks && settings.dialog_cards);
    let again = env.manager(None, ManagerConfig::default());
    assert_eq!(again.console_features(), ON);
    again.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_client_of_4_2_5_keeps_its_classic_console_beside_its_side_events() {
    let env = TestEnv::with_client_version("4.2.5");
    let id = env.add("SideBot").id;
    let s = ScriptedSpawner::new(|_, mut proc| async move {
        proc.event(r#"{"e":"connecting","host":"h","port":25565,"caps":["dialog"]}"#)
            .await;
        proc.event(r#"{"e":"joined","user":"SideBot","host":"h","port":25565,"protocol":777}"#)
            .await;
        // A chat line's looks right before it, exactly its text; then looks whose line differs.
        proc.event(r#"{"e":"looks","text":"[Duyuru] Etkinlik","st":[[8,"6l"],[9,"f"]]}"#)
            .await;
        proc.out("[Duyuru] Etkinlik").await;
        proc.event(r#"{"e":"looks","text":"[Duyuru] Yarın","st":[[8,"6l"],[6,"f"]]}"#)
            .await;
        proc.out("[12:00] [Duyuru] Yarın").await;
        proc.out("Düz satır").await;
        proc.event(r#"{"e":"menus","pause":{"label":"Kurallar"}}"#)
            .await;
        proc.event(r#"{"e":"dialog","rev":4,"wait":true}"#).await;
        loop {
            let line = proc.read().await?;
            if line == "/quit" {
                return Some(0);
            }
            if line.starts_with("/dialog act ") && line.contains("\"open\"") {
                proc.event(&rules_dialog(5)).await;
                proc.event(r#"{"e":"menures","menu":"pause","ok":true}"#)
                    .await;
            } else if line.starts_with("/dialog act ") {
                proc.event(r#"{"e":"dialogres","rev":4,"ok":true}"#).await;
                proc.event(r#"{"e":"dialog","rev":4,"clear":true}"#).await;
            }
        }
    });
    let manager = env.manager(spawner(&s), ManagerConfig::default());
    manager.set_console_features(ON);
    manager.connect(&id).await.unwrap();
    until("the waiting screen", LONG, || {
        manager.dialog(&id).is_some_and(|dialog| dialog.wait)
    })
    .await;
    let spec = s.launch(0).spec;
    assert!(!spec.console_kinds);
    assert_eq!(spec.console_side.as_deref(), Some("looks,dialogs"));

    // The console is the classic one; looks only on the very line that is exactly their text.
    let console = manager.console(&id);
    let line = |text: &str| console.iter().find(|l| l.text == text).unwrap().clone();
    assert!(line("[Duyuru] Etkinlik").looks.is_some());
    assert!(line("[12:00] [Duyuru] Yarın").looks.is_none());
    assert!(line("Düz satır").looks.is_none());
    assert!(!console.iter().any(|l| l.text.contains("\"e\"")));
    assert!(manager.dialog_menus(&id).unwrap().contains("Kurallar"));

    // The waiting screen's back; a menu the client offered opens, another does not.
    let back = manager
        .press_dialog(&id, r#"{"rev":4,"cancel":true}"#)
        .await
        .unwrap();
    assert!(back.ok, "{back:?}");
    until("the waiting screen goes", LONG, || {
        manager.dialog(&id).is_none()
    })
    .await;
    let quick = manager
        .press_dialog(&id, r#"{"open":"quick"}"#)
        .await
        .unwrap();
    assert_eq!(quick.why.as_deref(), Some("menu"));
    let open = manager
        .press_dialog(&id, r#"{"open":"pause"}"#)
        .await
        .unwrap();
    assert!(open.ok, "{open:?}");
    until("the menu's dialog", LONG, || {
        manager
            .dialog(&id)
            .is_some_and(|dialog| dialog.rev == 5 && !dialog.wait)
    })
    .await;
    manager.shutdown().await;
}
