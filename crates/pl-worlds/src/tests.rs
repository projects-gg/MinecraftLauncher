//! World sharing against fakes of the friends service, the overlay bridge and the launcher. No
//! relay is reached: openings here end at the service's order (an empty one is "no way"), and the
//! join tests stop before a path.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pl_host::guest::GuestEvent;
use pl_host::manifest::matching::InstallInfo;
use pl_host::manifest::{Loader, Manifest, ModRef, PackRef};
use pl_host::policy::env::{CredsError, CredsReason, HostPaths};
use pl_host::report::{Event, Role};
use pl_host::transport::BoxFuture;
use pl_host::tunnel::SessionId;
use pl_overlay::protocol::{FromGame, ToGame};
use pl_overlay::{HostSignal, WorldsShown};
use pl_social::{
    E4mcHint, Hidden, TurnIce, WorldAck, WorldAnswer, WorldError, WorldEvent, WorldFrame,
    WorldInvite, WorldsStatus,
};
use serde_json::json;
use tokio::sync::oneshot;

use crate::link::{CredsTable, HostLink, Service, Transports};
use crate::manager::Person;
use crate::view::{
    HostNotice, HostSessionView, WorldGameMode, WorldGuestState, WorldInviteOutcome,
    WorldJoinFailure, WorldJoinStage, WorldOpenFailure, WorldOpenOutcome, WorldOpenRequest,
    WorldOpeningStep,
};
use crate::{
    Account, Bridge, HostInstall, Launcher, PlayTogether, Progress, UiEvent, Worlds, WorldsConfig,
};

// ── Fakes ────────────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct FakeService {
    frames: Mutex<Vec<WorldFrame>>,
    /// Answers by frame kind, in order; without one: an empty `ack`.
    answers: Mutex<HashMap<&'static str, VecDeque<Result<WorldAnswer, WorldError>>>>,
}

impl FakeService {
    fn answer(&self, kind: &'static str, answer: Result<WorldAnswer, WorldError>) {
        self.answers
            .lock()
            .unwrap()
            .entry(kind)
            .or_default()
            .push_back(answer);
    }

    fn frames(&self) -> Vec<WorldFrame> {
        self.frames.lock().unwrap().clone()
    }

    fn kinds(&self) -> Vec<&'static str> {
        self.frames().iter().map(WorldFrame::kind).collect()
    }
}

impl Service for FakeService {
    fn request(
        &self,
        frame: WorldFrame,
        _wait: Duration,
    ) -> BoxFuture<'_, Result<WorldAnswer, WorldError>> {
        let kind = frame.kind();
        self.frames.lock().unwrap().push(frame);
        let answer = self
            .answers
            .lock()
            .unwrap()
            .get_mut(kind)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| Ok(WorldAnswer::default()));
        Box::pin(async move { answer })
    }

    fn send(&self, frame: WorldFrame) {
        self.frames.lock().unwrap().push(frame);
    }
}

#[derive(Default)]
struct FakeBridge {
    sent: Mutex<Vec<ToGame>>,
    shown: Mutex<Vec<WorldsShown>>,
    connects: Mutex<Vec<(u16, String)>>,
}

impl FakeBridge {
    fn sent(&self) -> Vec<ToGame> {
        self.sent.lock().unwrap().clone()
    }
}

impl Bridge for FakeBridge {
    fn send(&self, frames: Vec<ToGame>) {
        self.sent.lock().unwrap().extend(frames);
    }

    fn show(&self, shown: WorldsShown) {
        self.shown.lock().unwrap().push(shown);
    }

    fn connect(&self, port: u16, label: String) -> BoxFuture<'_, bool> {
        self.connects.lock().unwrap().push((port, label));
        Box::pin(async { true })
    }
}

struct FakeLauncher {
    game_dir: PathBuf,
    hostable: bool,
    /// The backup waits for this, when set.
    backup_gate: Mutex<Option<oneshot::Receiver<()>>>,
}

impl FakeLauncher {
    fn new(game_dir: PathBuf) -> Self {
        Self {
            game_dir,
            hostable: true,
            backup_gate: Mutex::new(None),
        }
    }
}

fn manifest() -> Manifest {
    Manifest::new(
        "26.1.2",
        Loader {
            kind: "fabric".into(),
            version: "0.19.5".into(),
        },
        Some(PackRef {
            family: "projects".into(),
            id: "projects-26.1.2".into(),
            version: "2.3.0".into(),
        }),
        Vec::new(),
        vec![ModRef::new("P7dR8mSH", "9YVrKY0Z")],
        0,
    )
}

impl Launcher for FakeLauncher {
    fn host_install(&self, _instance_id: &str) -> BoxFuture<'_, Option<HostInstall>> {
        let install = HostInstall {
            mc: "26.1.2".into(),
            family: Some("projects".into()),
            game_dir: self.game_dir.clone(),
            hostable: self.hostable,
        };
        Box::pin(async move { Some(install) })
    }

    fn manifest(&self, _instance_id: &str) -> BoxFuture<'_, Option<Manifest>> {
        Box::pin(async { Some(manifest()) })
    }

    fn world_name(&self, _instance_id: &str, folder: &str) -> BoxFuture<'_, String> {
        let name = format!("{folder} (adı)");
        Box::pin(async move { name })
    }

    fn backup_if_old(
        &self,
        _instance_id: &str,
        _folder: &str,
        progress: Progress,
    ) -> BoxFuture<'_, bool> {
        let gate = self.backup_gate.lock().unwrap().take();
        Box::pin(async move {
            progress(40);
            if let Some(gate) = gate {
                let _ = gate.await;
            }
            progress(100);
            true
        })
    }

    fn account(&self) -> Option<Account> {
        Some(Account {
            id: "a1".into(),
            name: "Ece".into(),
            premium: false,
        })
    }

    fn use_account(&self, _account_id: &str) -> BoxFuture<'_, Option<Account>> {
        Box::pin(async { None })
    }

    fn installs(&self, _mc: &str) -> BoxFuture<'_, Vec<InstallInfo>> {
        Box::pin(async { Vec::new() })
    }

    fn latest_pack(&self, _family: &str, _mc: &str) -> BoxFuture<'_, Option<String>> {
        Box::pin(async { Some("2.3.0".into()) })
    }

    fn refresh_content(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    fn install_official(&self, _mc: &str) -> BoxFuture<'_, Option<String>> {
        Box::pin(async { None })
    }

    fn mods_size(&self, _mods: &[ModRef]) -> BoxFuture<'_, u64> {
        Box::pin(async { 0 })
    }

    fn make_copy(
        &self,
        _name: &str,
        _host: &str,
        _manifest: &Manifest,
        _progress: Progress,
    ) -> BoxFuture<'_, Option<String>> {
        Box::pin(async { None })
    }

    fn update_copy(
        &self,
        _instance_id: &str,
        _manifest: &Manifest,
        _progress: Progress,
    ) -> BoxFuture<'_, bool> {
        Box::pin(async { false })
    }

    fn accept_filter(&self) -> Option<pl_host::guest::local::AcceptFilter> {
        None
    }
}

#[derive(Default)]
struct FakeTogether {
    invited: Mutex<Vec<String>>,
}

impl PlayTogether for FakeTogether {
    fn invite(&self, friend: &str) -> BoxFuture<'_, ()> {
        self.invited.lock().unwrap().push(friend.to_owned());
        Box::pin(async {})
    }
}

struct Harness {
    worlds: Worlds,
    service: Arc<FakeService>,
    bridge: Arc<FakeBridge>,
    together: Arc<FakeTogether>,
    ui: Arc<Mutex<Vec<UiEvent>>>,
    _dir: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        Self::with(|_| {})
    }

    fn with(adjust: impl FnOnce(&mut FakeLauncher)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let service = Arc::new(FakeService::default());
        let bridge = Arc::new(FakeBridge::default());
        let together = Arc::new(FakeTogether::default());
        let mut launcher = FakeLauncher::new(dir.path().join("game"));
        adjust(&mut launcher);
        let ui = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&ui);
        let worlds = Worlds::new(
            service.clone(),
            bridge.clone(),
            Arc::new(launcher),
            together.clone(),
            Arc::new(move |event| sink.lock().unwrap().push(event)),
            WorldsConfig::new("2.1.0-test", dir.path().to_path_buf()),
        );
        Self {
            worlds,
            service,
            bridge,
            together,
            ui,
            _dir: dir,
        }
    }

    /// Signed config, service and Studio all on.
    fn switched_on(self) -> Self {
        self.worlds.set_allowed(true);
        self.worlds.on_service(WorldEvent::Hello {
            taken: true,
            status: WorldsStatus {
                on: true,
                max_guests: 7,
            },
        });
        self
    }

    fn notices(&self) -> Vec<HostNotice> {
        self.ui
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                UiEvent::Notice(notice) => Some(notice.clone()),
                UiEvent::Changed(_) => None,
            })
            .collect()
    }

    /// A game of `projects` whose hosting mod is in world `W`.
    async fn in_world(&self) {
        self.worlds.game_started("projects", Some("t".repeat(43)));
        self.worlds.on_bridge(HostSignal::Connected { hosts: true });
        self.worlds
            .on_bridge(HostSignal::Frame(FromGame::HostWorld {
                open: true,
                folder: "W".into(),
                mode: "survival".into(),
                lan: false,
            }));
        // The install's check runs in the background.
        eventually("the game's install was looked at", || {
            self.bridge
                .shown
                .lock()
                .unwrap()
                .last()
                .is_some_and(|shown| shown.may_host)
        })
        .await;
    }
}

async fn eventually(what: &str, check: impl Fn() -> bool) {
    let waited = tokio::time::timeout(Duration::from_secs(5), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "{what}");
}

fn publish_id(bridge: &FakeBridge) -> Option<String> {
    bridge.sent().iter().find_map(|frame| match frame {
        ToGame::HostPublish { id, .. } => Some(id.clone()),
        _ => None,
    })
}

fn join_vector() -> serde_json::Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../pl-host/testdata/invite/join.json"
    );
    let file: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    file["join"].clone()
}

fn invite(from: &str) -> WorldInvite {
    serde_json::from_value(json!({
        "id": "0123456789abcdef", "from": from, "at": 1792521000,
        "session": "stuiX9ATk0i2-2_h8YikXQ", "join": join_vector()
    }))
    .unwrap()
}

// ── The player's world ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_in_game_opening_without_a_way_fails_and_closes_what_it_opened() {
    let h = Harness::new().switched_on();
    h.in_world().await;
    // The service names no path: nothing can open.
    h.service.answer(
        "turn_creds",
        Ok(WorldAnswer {
            order: Vec::new(),
            ..WorldAnswer::default()
        }),
    );
    h.worlds.on_bridge(HostSignal::Frame(FromGame::HostRequest {
        mode: "creative".into(),
        commands: true,
        invite: vec!["f1".into()],
    }));
    eventually("the mod was asked to open the world", || {
        publish_id(&h.bridge).is_some()
    })
    .await;
    let opened = h.service.frames();
    let WorldFrame::World {
        open: true,
        session,
        guests: Some(0),
        max: Some(7),
        ..
    } = &opened[0]
    else {
        panic!("first world{{open:true}}, got {opened:?}");
    };
    let session = session.clone();
    assert!(SessionId::parse(&session).is_some());
    let publish = h
        .bridge
        .sent()
        .into_iter()
        .find(|f| matches!(f, ToGame::HostPublish { .. }))
        .unwrap();
    let ToGame::HostPublish {
        id,
        mode,
        commands,
        max,
        binding,
    } = publish
    else {
        unreachable!()
    };
    assert_eq!((mode.as_str(), commands, max), ("creative", true, 7));
    assert_eq!(binding.len(), 8, "the binding key's id");
    // The mod opened it.
    h.worlds
        .on_bridge(HostSignal::Frame(FromGame::HostPublished {
            id,
            ok: true,
            port: 51234,
            proto: 774,
            reason: String::new(),
        }));
    eventually("the opening failed", || {
        h.notices().contains(&HostNotice::OpenFailed {
            reason: WorldOpenFailure::Unavailable,
        })
    })
    .await;
    // The session asked for the order, then told the service and the mod it is over.
    assert!(h.service.kinds().contains(&"turn_creds"));
    assert!(h.service.frames().iter().any(|f| matches!(f,
        WorldFrame::World { open: false, session: s, .. } if *s == session)));
    let sent = h.bridge.sent();
    assert!(sent.iter().any(|f| matches!(f,
        ToGame::HostStop { reason: Some(r) } if r == "error")));
    assert!(sent.iter().any(|f| matches!(f,
        ToGame::HostState { open: false, error: Some(e), .. } if e == "unavailable")));
    let view = h.worlds.view();
    assert!(matches!(
        view.session,
        Some(HostSessionView::Failed {
            reason: WorldOpenFailure::Unavailable,
            ..
        })
    ));
    // "Kapat".
    h.worlds.dismiss_failure();
    assert!(h.worlds.view().session.is_none());
    assert!(!h.worlds.hosting());
}

#[tokio::test]
async fn todays_opens_used_up_is_the_limit_and_nothing_reaches_the_game() {
    let h = Harness::new().switched_on();
    h.in_world().await;
    h.service.answer(
        "world",
        Err(WorldError::Refused {
            code: "rate_limited".into(),
            reason: None,
            item: Some("world_opens".into()),
            retry_after: Some(3600),
        }),
    );
    h.worlds.on_bridge(HostSignal::Frame(FromGame::HostRequest {
        mode: "survival".into(),
        commands: false,
        invite: vec!["f1".into()],
    }));
    eventually("the limit was told", || {
        h.notices().contains(&HostNotice::OpenFailed {
            reason: WorldOpenFailure::Limit,
        })
    })
    .await;
    assert!(publish_id(&h.bridge).is_none());
    assert!(h.bridge.sent().iter().any(|f| matches!(f,
        ToGame::HostState { open: false, error: Some(e), .. } if e == "limit")));
}

#[tokio::test]
async fn a_request_the_launcher_may_not_open_is_refused_in_the_game() {
    let h = Harness::with(|launcher| launcher.hostable = false).switched_on();
    h.worlds.game_started("projects", Some("t".repeat(43)));
    h.worlds.on_bridge(HostSignal::Connected { hosts: true });
    h.worlds.on_bridge(HostSignal::Frame(FromGame::HostWorld {
        open: true,
        folder: "W".into(),
        mode: "survival".into(),
        lan: false,
    }));
    tokio::time::sleep(Duration::from_millis(50)).await;
    h.worlds.on_bridge(HostSignal::Frame(FromGame::HostRequest {
        mode: "survival".into(),
        commands: false,
        invite: vec!["f1".into()],
    }));
    assert!(h.bridge.sent().iter().any(|f| matches!(f,
        ToGame::HostState { open: false, error: Some(e), .. } if e == "unavailable")));
    assert!(h.service.frames().is_empty());
}

#[tokio::test]
async fn opening_from_the_launcher_backs_up_starts_the_game_and_publishes_once_it_loaded() {
    let h = Harness::new().switched_on();
    let request = WorldOpenRequest {
        instance_id: "projects".into(),
        folder: "W".into(),
        mode: WorldGameMode::Survival,
        commands: false,
        invite: vec!["f1".into(), "f2".into()],
    };
    assert_eq!(h.worlds.open(request).await, WorldOpenOutcome::Start);
    let view = h.worlds.view();
    let Some(HostSessionView::Opening {
        step,
        guests,
        world_name,
        ..
    }) = view.session
    else {
        panic!("an opening")
    };
    assert_eq!(step, WorldOpeningStep::Starting);
    assert_eq!(world_name, "W (adı)");
    assert_eq!(guests.len(), 2);
    assert!(guests.iter().all(|g| g.state == WorldGuestState::Pending));
    // The backup's progress showed on the way.
    assert!(h.ui.lock().unwrap().iter().any(|event| matches!(event,
        UiEvent::Changed(view) if matches!(view.session,
            Some(HostSessionView::Opening { step: WorldOpeningStep::Backup, percent: Some(40), .. })))));
    assert!(h.worlds.hosting());
    // The game loads the world: it opens there.
    h.in_world().await;
    eventually("the mod was asked to open the world", || {
        publish_id(&h.bridge).is_some()
    })
    .await;
    // The mod could not.
    let id = publish_id(&h.bridge).unwrap();
    h.worlds
        .on_bridge(HostSignal::Frame(FromGame::HostPublished {
            id,
            ok: false,
            port: 0,
            proto: 0,
            reason: "lan".into(),
        }));
    eventually("the opening failed", || {
        h.notices().contains(&HostNotice::OpenFailed {
            reason: WorldOpenFailure::Unavailable,
        })
    })
    .await;
    // The service let the session go again.
    assert!(
        h.service
            .frames()
            .iter()
            .any(|f| matches!(f, WorldFrame::World { open: false, .. }))
    );
}

#[tokio::test]
async fn vazgec_during_the_backup_cancels_the_opening() {
    let (release, gate) = oneshot::channel();
    let h = Harness::with(move |launcher| *launcher.backup_gate.lock().unwrap() = Some(gate))
        .switched_on();
    let request = WorldOpenRequest {
        instance_id: "projects".into(),
        folder: "W".into(),
        mode: WorldGameMode::Creative,
        commands: false,
        invite: vec!["f1".into()],
    };
    let opening = {
        let worlds = h.worlds.clone();
        tokio::spawn(async move { worlds.open(request).await })
    };
    eventually("the backup runs", || {
        matches!(
            h.worlds.view().session,
            Some(HostSessionView::Opening {
                step: WorldOpeningStep::Backup,
                ..
            })
        )
    })
    .await;
    h.worlds.cancel_open();
    assert_eq!(opening.await.unwrap(), WorldOpenOutcome::Cancelled);
    assert!(h.worlds.view().session.is_none());
    drop(release);
    assert!(h.service.frames().is_empty());
}

#[tokio::test]
async fn nothing_shows_and_nothing_opens_while_any_switch_is_off() {
    // The signed config is on, but the service did not take worlds.
    let h = Harness::new();
    h.worlds.set_allowed(true);
    h.worlds.on_service(WorldEvent::Hello {
        taken: false,
        status: WorldsStatus::default(),
    });
    assert!(!h.worlds.view().enabled);
    assert!(!h.worlds.place("projects").await.shown);
    let request = WorldOpenRequest {
        instance_id: "projects".into(),
        folder: "W".into(),
        mode: WorldGameMode::Survival,
        commands: false,
        invite: vec!["f1".into()],
    };
    assert_eq!(h.worlds.open(request).await, WorldOpenOutcome::Cancelled);
    assert!(h.service.frames().is_empty());
    // Studio's switch on: the row shows; with the world open in that game, it opens in place.
    h.worlds.on_service(WorldEvent::Hello {
        taken: true,
        status: WorldsStatus {
            on: true,
            max_guests: 4,
        },
    });
    let view = h.worlds.view();
    assert!(view.enabled && view.max_guests == 4);
    let place = h.worlds.place("projects").await;
    assert!(place.shown && place.playing.is_none());
    h.in_world().await;
    assert_eq!(
        h.worlds.place("projects").await.playing.as_deref(),
        Some("W")
    );
    assert_eq!(h.worlds.place("other").await.playing, None);
}

#[tokio::test]
async fn invites_without_an_open_world_are_refused_as_the_sessions() {
    let h = Harness::new().switched_on();
    assert_eq!(
        h.worlds.invite(vec!["f1".into()]).await,
        WorldInviteOutcome::Session
    );
}

// ── Friends' worlds ──────────────────────────────────────────────────────────────────────────

fn friends(online: bool) -> HashMap<String, Person> {
    [(
        "f-ayse".to_owned(),
        Person {
            name: "Ayşe".into(),
            avatar: Some("a3".into()),
            avatar_url: None,
            online,
        },
    )]
    .into()
}

#[tokio::test]
async fn a_world_invite_shows_tells_and_marks_the_friend_until_the_world_closes() {
    let h = Harness::new().switched_on();
    h.worlds.on_friends(friends(true), false);
    h.in_world().await;
    h.worlds.on_service(WorldEvent::Invite {
        invite: invite("f-ayse"),
        quiet: false,
    });
    let view = h.worlds.view();
    assert_eq!(view.invites.len(), 1);
    let shown = &view.invites[0];
    assert_eq!(
        (
            shown.name.as_str(),
            shown.mc.as_str(),
            shown.avatar.as_deref()
        ),
        ("Ayşe", "26.1.2", Some("a3"))
    );
    assert!(shown.join.is_none() && !shown.quiet);
    assert!(
        h.notices()
            .iter()
            .any(|n| matches!(n, HostNotice::Invite { invite } if invite.from == "f-ayse"))
    );
    // The game's overlay: a card, and Ayşe's row marked.
    assert!(h.bridge.sent().iter().any(|f| matches!(f,
        ToGame::Toast(toast) if toast.title == "Ayşe" && toast.key.starts_with("world:"))));
    let marks = h.bridge.shown.lock().unwrap().last().unwrap().marks.clone();
    assert_eq!(marks["f-ayse"].max, 7);
    // The count changes.
    h.worlds.on_service(WorldEvent::Update {
        from: "f-ayse".into(),
        session: "stuiX9ATk0i2-2_h8YikXQ".into(),
        guests: 2,
        max: 5,
        paths: None,
    });
    let view = h.worlds.view();
    assert_eq!((view.invites[0].guests, view.invites[0].max), (2, 5));
    // The world closes: the invite goes quietly (nobody was joining).
    h.worlds.on_service(WorldEvent::Closed {
        from: "f-ayse".into(),
        session: "stuiX9ATk0i2-2_h8YikXQ".into(),
    });
    assert!(h.worlds.view().invites.is_empty());
    assert!(
        !h.notices()
            .iter()
            .any(|n| matches!(n, HostNotice::JoinFailed { .. }))
    );
    assert!(
        h.bridge
            .shown
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .marks
            .is_empty()
    );
}

#[tokio::test]
async fn a_quiet_invite_is_listed_only_and_a_host_going_offline_takes_it() {
    let h = Harness::new().switched_on();
    h.worlds.on_friends(friends(true), false);
    h.in_world().await;
    h.worlds.on_service(WorldEvent::Invite {
        invite: invite("f-ayse"),
        quiet: true,
    });
    assert!(h.worlds.view().invites[0].quiet);
    assert!(h.notices().is_empty());
    assert!(
        !h.bridge
            .sent()
            .iter()
            .any(|f| matches!(f, ToGame::Toast(_)))
    );
    h.worlds.on_friends(friends(false), false);
    assert!(h.worlds.view().invites.is_empty());
}

const SESSION: &str = "stuiX9ATk0i2-2_h8YikXQ";

fn mods_failures(h: &Harness) -> usize {
    h.notices()
        .iter()
        .filter(|n| {
            matches!(n, HostNotice::JoinFailed { session, name, failure: WorldJoinFailure::Mods }
                if session == SESSION && name == "Ayşe")
        })
        .count()
}

/// Fabric's registry sync in the game's log while the game is in the friend's world: the join
/// ends with `mods` once (its several lines tell once), and other lines change nothing.
#[tokio::test]
async fn a_registry_mismatch_in_the_games_log_ends_the_join_with_mods() {
    let h = Harness::new().switched_on();
    h.worlds.on_friends(friends(true), false);
    h.worlds.on_service(WorldEvent::Invite {
        invite: invite("f-ayse"),
        quiet: false,
    });
    // No join under way: the game's other servers are none of this.
    h.worlds
        .game_log("Received unknown remote registry entries from server");
    assert!(h.worlds.view().invites[0].join.is_none());
    h.worlds.force_join_stage(SESSION, Some(WorldJoinStage::In));
    h.worlds
        .game_log("Client disconnected with reason: Network Protocol Error");
    assert_eq!(h.worlds.view().invites[0].join, Some(WorldJoinStage::In));
    for line in [
        "Received unknown remote registry entries from server",
        "Registry entry (farmersdelight:tomato) is missing from local registry (minecraft:item)",
        "Registry remapping failed!",
        "Client disconnected with reason: Received 1 registry entries that are unknown to this \
         client.",
    ] {
        h.worlds.game_log(line);
    }
    assert_eq!(
        h.worlds.view().invites[0].join,
        Some(WorldJoinStage::Failed {
            failure: WorldJoinFailure::Mods
        })
    );
    assert_eq!(mods_failures(&h), 1);
    // The connection's end after it changes nothing.
    h.worlds.guest_event(SESSION, GuestEvent::GameEnded);
    assert_eq!(
        h.worlds.view().invites[0].join,
        Some(WorldJoinStage::Failed {
            failure: WorldJoinFailure::Mods
        })
    );
}

/// The connection may end before the log says why: a line within half a minute still counts, a
/// later one (another server, another game) does not.
#[tokio::test(start_paused = true)]
async fn a_mismatch_line_right_after_the_connection_ended_still_counts() {
    let h = Harness::new().switched_on();
    h.worlds.on_friends(friends(true), false);
    h.worlds.on_service(WorldEvent::Invite {
        invite: invite("f-ayse"),
        quiet: false,
    });
    h.worlds
        .force_join_stage(SESSION, Some(WorldJoinStage::Connecting));
    h.worlds.guest_event(SESSION, GuestEvent::GameEnded);
    assert!(h.worlds.view().invites[0].join.is_none());
    tokio::time::advance(Duration::from_secs(5)).await;
    h.worlds.game_log(
        "Client disconnected with reason: Received a registry entry that is unknown to this client.",
    );
    assert_eq!(
        h.worlds.view().invites[0].join,
        Some(WorldJoinStage::Failed {
            failure: WorldJoinFailure::Mods
        })
    );
    assert_eq!(mods_failures(&h), 1);

    // Half a minute later the same words are another story.
    h.worlds.force_join_stage(SESSION, Some(WorldJoinStage::In));
    h.worlds.guest_event(SESSION, GuestEvent::GameEnded);
    tokio::time::advance(Duration::from_secs(31)).await;
    h.worlds
        .game_log("Received unknown remote registry entries from server");
    assert!(h.worlds.view().invites[0].join.is_none());
    assert_eq!(mods_failures(&h), 1);
}

#[tokio::test]
async fn an_unreadable_invite_is_dropped() {
    let h = Harness::new().switched_on();
    let mut bad = invite("f-ayse");
    bad.join = Hidden(json!({"v": 2, "anything": true}));
    h.worlds.on_service(WorldEvent::Invite {
        invite: bad,
        quiet: false,
    });
    assert!(h.worlds.view().invites.is_empty());
}

#[tokio::test]
async fn projects_te_bulusun_invites_once_the_player_is_on_projects() {
    let h = Harness::new().switched_on();
    h.worlds
        .meet_on_projects(vec!["f-ayse".into(), "f-mert".into()])
        .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(h.together.invited.lock().unwrap().is_empty());
    h.worlds.on_friends(friends(true), true);
    eventually("the invites went", || {
        h.together.invited.lock().unwrap().len() == 2
    })
    .await;
    // Only once.
    h.worlds.on_friends(friends(true), true);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(h.together.invited.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn projects_te_bulusun_from_the_game_invites_the_same_way() {
    let h = Harness::new().switched_on();
    h.worlds.on_bridge(HostSignal::Frame(FromGame::HostMeet {
        invite: vec!["f-ayse".into(), "f-mert".into()],
    }));
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(h.together.invited.lock().unwrap().is_empty());
    h.worlds.on_friends(friends(true), true);
    eventually("the invites went", || {
        h.together.invited.lock().unwrap().len() == 2
    })
    .await;
}

// ── The service and the credentials ──────────────────────────────────────────────────────────

fn ice(username: &str) -> TurnIce {
    serde_json::from_value(json!({
        "urls": ["turn:turn.example:3478?transport=udp"],
        "username": username, "credential": "c", "ttl": 7200
    }))
    .unwrap()
}

#[tokio::test]
async fn turn_credentials_stay_here_and_go_back_named() {
    let service = Arc::new(FakeService::default());
    let table = Arc::new(CredsTable::default());
    let link = HostLink::new(
        service.clone(),
        table.clone(),
        "s".into(),
        Transports::new("2.1.0-test"),
        |_| {},
    );
    service.answer(
        "turn_creds",
        Ok(WorldAnswer {
            order: vec!["turn".into(), "e4mc".into(), "later".into()],
            ice: Some(ice("u1")),
            e4mc: Some(E4mcHint {
                region: "de".into(),
            }),
            ..WorldAnswer::default()
        }),
    );
    let answer = link.turn_creds(CredsReason::Primary).await.unwrap();
    assert_eq!(answer.order.len(), 2, "unknown paths are skipped");
    let creds = answer.creds.unwrap();
    assert_eq!(creds.ttl, Duration::from_secs(7200));
    assert_eq!(table.len(), 1);
    // The service's refusals as pl-host reads them.
    let refusals = [
        (
            Err(WorldError::Refused {
                code: "turn_denied".into(),
                reason: Some("budget".into()),
                item: None,
                retry_after: None,
            }),
            CredsError::Denied("budget".into()),
        ),
        (
            Err(WorldError::Refused {
                code: "not_hosting".into(),
                reason: None,
                item: None,
                retry_after: None,
            }),
            CredsError::Gone,
        ),
        (
            Err(WorldError::Refused {
                code: "turn_failed".into(),
                reason: None,
                item: None,
                retry_after: None,
            }),
            CredsError::Failed,
        ),
        (Err(WorldError::Unavailable), CredsError::Failed),
        (Err(WorldError::Off), CredsError::Gone),
    ];
    for (answer, expected) in refusals {
        service.answer("turn_creds", answer);
        assert_eq!(
            link.turn_creds(CredsReason::Fallback).await.unwrap_err(),
            expected
        );
    }
    // Let go: the service hears the username, and only once.
    link.turn_release(creds);
    link.turn_release(creds);
    let released: Vec<String> = service
        .frames()
        .iter()
        .filter_map(|f| match f {
            WorldFrame::TurnRelease { username } => Some(username.expose().clone()),
            _ => None,
        })
        .collect();
    assert_eq!(released, ["u1"]);
    assert!(table.is_empty());
}

#[tokio::test]
async fn after_a_reconnect_held_credentials_are_claimed_four_a_frame() {
    let h = Harness::new().switched_on();
    // Nothing held, nothing claimed.
    assert!(h.worlds.creds().held_frames().is_empty());
    let service = Arc::new(FakeService::default());
    let shared = Arc::new(CredsTable::default());
    let link = HostLink::new(
        service.clone(),
        shared.clone(),
        "s".into(),
        Transports::new("t"),
        |_| {},
    );
    // Five credentials of one session (renewals under way).
    for n in 0..5 {
        service.answer(
            "turn_creds",
            Ok(WorldAnswer {
                order: vec!["turn".into()],
                ice: Some(ice(&format!("u{n}"))),
                ..WorldAnswer::default()
            }),
        );
        link.turn_creds(CredsReason::Renew).await.unwrap();
    }
    let frames = shared.held_frames();
    assert_eq!(frames.len(), 2);
    let counts: Vec<usize> = frames
        .iter()
        .map(|f| match f {
            WorldFrame::TurnHeld {
                usernames, role, ..
            } => {
                assert_eq!(*role, pl_social::WorldRole::Host);
                usernames.len()
            }
            _ => 0,
        })
        .collect();
    assert_eq!(counts, [4, 1]);
}

#[tokio::test]
async fn telemetry_goes_as_world_reports_and_waits_while_the_socket_is_down() {
    let h = Harness::new().switched_on();
    let usage = Event::Usage {
        session: SessionId::parse("stuiX9ATk0i2-2_h8YikXQ").unwrap(),
        role: Role::Host,
        turn_rx: 1000,
    };
    h.worlds.report("26.1.2", usage.clone());
    eventually("the usage went at once", || {
        h.service.kinds().contains(&"world_report")
    })
    .await;
    let WorldFrame::Report(frame) = h
        .service
        .frames()
        .into_iter()
        .find(|f| f.kind() == "world_report")
        .unwrap()
    else {
        unreachable!()
    };
    let frame: serde_json::Value = serde_json::from_str(&frame).unwrap();
    assert_eq!(
        (frame["mc"].clone(), frame["lv"].clone()),
        ("26.1.2".into(), "2.1.0-test".into())
    );
    // Down: kept (its bytes carried), sent once the socket is back.
    h.worlds.on_service(WorldEvent::Down);
    h.worlds.report("26.1.2", usage);
    let before = h.service.kinds().len();
    h.worlds.on_service(WorldEvent::Hello {
        taken: true,
        status: WorldsStatus {
            on: true,
            max_guests: 7,
        },
    });
    eventually("the carried bytes went", || {
        h.service.kinds()[before..].contains(&"world_report")
    })
    .await;
}

#[tokio::test]
async fn a_first_answer_lowers_the_guest_limit_to_studios() {
    let h = Harness::new().switched_on();
    h.in_world().await;
    h.service.answer(
        "world",
        Ok(WorldAnswer {
            world: Some(WorldAck {
                session: String::new(),
                max: Some(3),
            }),
            ..WorldAnswer::default()
        }),
    );
    h.worlds.on_bridge(HostSignal::Frame(FromGame::HostRequest {
        mode: "adventure".into(),
        commands: false,
        invite: vec!["f1".into(), "f2".into()],
    }));
    eventually("the mod was asked to open the world", || {
        publish_id(&h.bridge).is_some()
    })
    .await;
    assert!(
        h.bridge
            .sent()
            .iter()
            .any(|f| matches!(f, ToGame::HostPublish { max: 3, mode, .. } if mode == "adventure"))
    );
    // A late answer to another publish closes what it opened.
    h.worlds
        .on_bridge(HostSignal::Frame(FromGame::HostPublished {
            id: "someone-else".into(),
            ok: true,
            port: 1,
            proto: 1,
            reason: String::new(),
        }));
    assert!(h.bridge.sent().iter().any(|f| matches!(f,
        ToGame::HostStop { reason: Some(r) } if r == "error")));
}
