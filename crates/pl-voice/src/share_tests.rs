//! Screen sharing's scenarios on the machine (SHARE; the lead's contract share.md §3, §4, §6).

use std::time::Duration;

use pl_social::BackoffPolicy;

use crate::engine::{EngineCommand, EngineSound, PageReport};
use crate::machine::{CrewRoom, Effect, Machine, ServiceVoice, StayStart};
use crate::settings::{Store, VoiceSettings};
use crate::share::{
    ALONE_STOP_MS, HIDDEN_GRACE_MS, PAUSE_AFTER_MS, STILL_ANSWER_MS, STILL_ASK_MS, ServiceShare,
    ShareBy, ShareClosedWhy, ShareCmd, ShareEvent, ShareKind, ShareLayer, ShareMinePhase,
    ShareNotice, SharePreset, ShareRefusal, ShareSource, ShareSurface, ShareWatchPhase, ShareWire,
    THUMB_MAX_BYTES, ThumbJpeg, WatchCmd, WatchMidWire, WatchReport,
};
use crate::view::VoiceLeftReason;
use crate::wire::{
    FromRoom, LeftWhy, MeWire, PersonWire, Role, RoomKind, RoomWire, Sdp, Ticket, ToRoom,
};

const ROOM: &str = "f0123456789abcdef";
const UNIX: i64 = 1_800_000_000;
const MIN: u64 = 60_000;

fn ready() -> Machine {
    let mut machine = Machine::new(Store::in_memory());
    machine.set_backoff(BackoffPolicy {
        initial: Duration::from_millis(1_000),
        max: Duration::from_millis(1_000),
        spread: Duration::ZERO,
    });
    machine.set_flag(true, 0);
    machine.set_me("me", 0);
    machine.hub_hello(
        true,
        Some(ServiceVoice {
            on: true,
            ..ServiceVoice::default()
        }),
        Vec::new(),
    );
    machine.set_share_publisher(true, true, true);
    machine.set_share_service(
        Some(ServiceShare {
            per_user: 2,
            max_preset: SharePreset::High,
            open: true,
            watch: true,
            edit: false,
        }),
        0,
    );
    machine
}

fn person(u: &str, sh: Vec<ShareWire>) -> PersonWire {
    PersonWire {
        u: u.into(),
        j: 1,
        mute: false,
        deaf: false,
        sm: false,
        q: 0,
        card: None,
        sh,
    }
}

fn shared(n: u8) -> ShareWire {
    ShareWire {
        n,
        src: ShareKind::Game,
        audio: true,
        preset: SharePreset::Normal,
        fps: 30,
        w: 0,
        at: UNIX,
        paused: false,
        th: None,
        st: crate::share::ShareSt::Live,
    }
}

/// The room's `hello`; `share` its "Kim paylaşabilir" (none: an older room). With `Mods` the player
/// is a member (not allowed to share); otherwise they lead the room.
fn hello(people: Vec<PersonWire>, share: Option<ShareBy>, sdp: bool) -> FromRoom {
    FromRoom::Hello {
        room: RoomWire {
            id: ROOM.into(),
            kind: RoomKind::Friend,
            crew: None,
            name: None,
            leader: Some("me".into()),
            mods: Vec::new(),
            max: 10,
            share_by: share,
            ptt: false,
            chat: false,
            grew: false,
        },
        me: MeWire {
            role: if share == Some(ShareBy::Mods) {
                Role::Member
            } else {
                Role::Leader
            },
            watch: None,
        },
        people,
        rings: Vec::new(),
        sdp: sdp.then(|| Sdp::new("v=0 answer")),
        resume: Some(Ticket::new("v1.resume.mac")),
        now: Some(UNIX),
    }
}

fn share_on() -> Option<ShareBy> {
    Some(ShareBy::All)
}

/// A live call with a friend `a` (sharing `a_shares`), sharing on in the room unless `share` says.
fn live(machine: &mut Machine, a_shares: Vec<ShareWire>, share: Option<ShareBy>) -> u64 {
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("v1.join.mac"),
            calling: None,
            opened_by_me: false,
        },
        1_000,
        UNIX,
    );
    let generation = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Connect { generation, .. } => Some(*generation),
            _ => None,
        })
        .unwrap();
    machine.page_attached();
    machine.socket_open(generation, 1_100);
    machine.page_report(
        PageReport::Offer {
            sdp: Some("v=0 offer".into()),
            mid: Some("0".into()),
        },
        1_200,
    );
    machine.socket_message(
        generation,
        hello(
            vec![person("me", Vec::new()), person("a", a_shares)],
            share,
            true,
        ),
        1_300,
        UNIX,
    );
    machine.page_report(PageReport::Connected, 1_400);
    generation
}

fn sent(effects: &[Effect]) -> Vec<&ToRoom> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Send { frame, .. } => Some(frame),
            _ => None,
        })
        .collect()
}

fn to_publisher(effects: &[Effect]) -> Vec<&ShareCmd> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Share(cmd) => Some(cmd),
            _ => None,
        })
        .collect()
}

fn to_page(effects: &[Effect]) -> Vec<(ShareSurface, &WatchCmd)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Watch { surface, cmd } => Some((*surface, cmd)),
            _ => None,
        })
        .collect()
}

fn notices(effects: &[Effect]) -> Vec<&ShareNotice> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ShareNotice(notice) => Some(notice),
            _ => None,
        })
        .collect()
}

fn privacy(effects: &[Effect]) -> Option<(bool, bool)> {
    effects.iter().find_map(|effect| match effect {
        Effect::SharePrivacy { hide, quiet } => Some((*hide, *quiet)),
        _ => None,
    })
}

fn open_rid(effects: &[Effect]) -> u32 {
    sent(effects)
        .into_iter()
        .find_map(|frame| match frame {
            ToRoom::ShareOpen { rid, .. } => Some(*rid),
            _ => None,
        })
        .expect("a share_open")
}

fn watch_rid(effects: &[Effect]) -> u32 {
    sent(effects)
        .into_iter()
        .find_map(|frame| match frame {
            ToRoom::Watch { rid, .. } => Some(*rid),
            _ => None,
        })
        .expect("a watch")
}

/// The player shares the game and the room answered: live.
fn sharing(machine: &mut Machine, generation: u64, now: u64) {
    machine
        .share_start(ShareSource::Game, "", now, UNIX)
        .unwrap();
    let effects = machine.share_event(
        ShareEvent::Offer {
            n: 1,
            sdp: Sdp::new("v=0 publisher"),
        },
        now,
    );
    let rid = open_rid(&effects);
    machine.socket_message(
        generation,
        FromRoom::Ack {
            rid: Some(rid),
            ice: None,
            until: None,
            sdp: Some(Sdp::new("v=0 sfu")),
            n: Some(1),
            preset: None,
            fps: None,
            audio: None,
            id: None,
            at: None,
            m: None,
        },
        now,
        UNIX,
    );
}

#[test]
fn an_older_room_shows_nothing_of_sharing() {
    let mut machine = ready();
    live(&mut machine, Vec::new(), None);
    let view = machine.view(2_000, UNIX).share;
    assert!(!view.on);
    assert_eq!(
        machine.share_start(ShareSource::Game, "", 2_000, UNIX),
        Err(ShareRefusal::Off)
    );
    assert_eq!(machine.share_watch("a", 1, 2_000), Err(ShareRefusal::Off));
}

#[test]
fn the_room_socket_asks_for_share1() {
    let mut machine = ready();
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("v1.join.mac"),
            calling: None,
            opened_by_me: false,
        },
        1_000,
        UNIX,
    );
    let generation = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Connect { generation, .. } => Some(*generation),
            _ => None,
        })
        .unwrap();
    machine.page_attached();
    machine.socket_open(generation, 1_100);
    let effects = machine.page_report(
        PageReport::Offer {
            sdp: Some("v=0".into()),
            mid: None,
        },
        1_200,
    );
    // SHARE-LIVE: `live1` with it (still pictures, their times in the share state).
    assert!(matches!(
        sent(&effects)[..],
        [ToRoom::Join { caps, .. }] if caps == &vec!["share1".to_owned(), "live1".to_owned(), "share2".to_owned(), "rchat1".to_owned()]
    ));
    assert_eq!(
        sent(&effects)[0].text(),
        r#"{"t":"join","sdp":"v=0","caps":["share1","live1","share2","rchat1"]}"#
    );
}

#[test]
fn a_share_goes_live_rests_alone_and_ends_after_fifteen_minutes() {
    let mut machine = ready();
    let generation = live(&mut machine, Vec::new(), share_on());
    let effects = machine
        .share_start(ShareSource::Game, "ignored", 2_000, UNIX)
        .unwrap();
    assert!(matches!(
        to_publisher(&effects)[..],
        [ShareCmd::Prepare { n: 1, source: ShareSource::Game, opts }]
            if opts.preset == SharePreset::Normal && opts.fps == 30 && opts.motion && opts.audio
    ));
    // The launcher leaves the picture and its notifications go quiet (both on by default).
    assert_eq!(privacy(&effects), Some((true, true)));
    let view = machine.view(2_000, UNIX).share;
    assert!(view.on && view.can_share && view.allowed);
    assert_eq!(view.mine[0].phase, ShareMinePhase::Starting);
    assert_eq!(view.mine[0].label, "");
    // The publisher's offer goes to the room; titles never do.
    let effects = machine.share_event(
        ShareEvent::Offer {
            n: 1,
            sdp: Sdp::new("v=0 publisher"),
        },
        2_100,
    );
    let rid = open_rid(&effects);
    assert!(sent(&effects)[0].text().starts_with(
        r#"{"t":"share_open","n":1,"src":"game","audio":true,"preset":"normal","fps":30"#
    ));
    let effects = machine.socket_message(
        generation,
        FromRoom::Ack {
            rid: Some(rid),
            ice: None,
            until: None,
            sdp: Some(Sdp::new("v=0 sfu")),
            n: Some(1),
            preset: None,
            fps: None,
            audio: None,
            id: None,
            at: None,
            m: None,
        },
        2_200,
        UNIX,
    );
    assert!(matches!(
        to_publisher(&effects)[..],
        [ShareCmd::Answer { n: 1, .. }]
    ));
    assert_eq!(
        machine.view(2_200, UNIX).share.mine[0].phase,
        ShareMinePhase::Live
    );
    // A watcher comes: a key frame.
    let effects = machine.socket_message(
        generation,
        FromRoom::Watchers {
            n: 1,
            ids: vec!["a".into()],
        },
        3_000,
        UNIX,
    );
    assert_eq!(to_publisher(&effects), [&ShareCmd::KeyFrame { n: 1 }]);
    assert_eq!(machine.view(3_000, UNIX).share.mine[0].watchers, ["a"]);
    // They leave: a minute later encoding rests.
    machine.socket_message(
        generation,
        FromRoom::Watchers {
            n: 1,
            ids: Vec::new(),
        },
        10_000,
        UNIX,
    );
    assert_eq!(machine.next_deadline(10_000), Some(10_000 + PAUSE_AFTER_MS));
    let effects = machine.tick(10_000 + PAUSE_AFTER_MS, UNIX, 10_000 + PAUSE_AFTER_MS);
    assert_eq!(
        to_publisher(&effects),
        [&ShareCmd::Pause { n: 1, paused: true }]
    );
    assert_eq!(sent(&effects), [&ToRoom::share_paused(1, true)]);
    assert_eq!(
        machine.view(80_000, UNIX).share.mine[0].phase,
        ShareMinePhase::Resting
    );
    // Someone comes back: on again with a key frame.
    let effects = machine.socket_message(
        generation,
        FromRoom::Watchers {
            n: 1,
            ids: vec!["a".into(), "me".into()],
        },
        100_000,
        UNIX,
    );
    assert_eq!(
        to_publisher(&effects),
        [
            &ShareCmd::Pause {
                n: 1,
                paused: false
            },
            &ShareCmd::KeyFrame { n: 1 }
        ]
    );
    assert_eq!(machine.view(100_000, UNIX).share.mine[0].watchers, ["a"]);
    // Alone for 15 minutes: the share ends, said quietly.
    machine.socket_message(
        generation,
        FromRoom::Watchers {
            n: 1,
            ids: Vec::new(),
        },
        200_000,
        UNIX,
    );
    machine.tick(200_000 + PAUSE_AFTER_MS, UNIX, 200_000 + PAUSE_AFTER_MS);
    let end = 200_000 + ALONE_STOP_MS;
    let effects = machine.tick(end, UNIX, end);
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    assert_eq!(sent(&effects), [&ToRoom::ShareClose { n: 1 }]);
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Closed {
            why: ShareClosedWhy::Idle,
            src: ShareKind::Game,
        }]
    );
    assert_eq!(privacy(&effects), Some((false, false)));
    // The voice stay goes on.
    assert!(machine.in_room());
}

#[test]
fn shares_are_held_to_the_rooms_limits() {
    let mut machine = ready();
    machine.set_share_service(
        Some(ServiceShare {
            per_user: 1,
            max_preset: SharePreset::Low,
            ..ServiceShare::default()
        }),
        0,
    );
    let generation = live(&mut machine, Vec::new(), share_on());
    let effects = machine
        .share_start(
            ShareSource::Window { id: "0x1f".into() },
            "Notlar",
            2_000,
            UNIX,
        )
        .unwrap();
    assert!(matches!(
        to_publisher(&effects)[..],
        [ShareCmd::Prepare { opts, .. }] if opts.preset == SharePreset::Low && !opts.motion
    ));
    assert_eq!(machine.view(2_000, UNIX).share.mine[0].label, "Notlar");
    assert_eq!(
        machine.share_start(ShareSource::Game, "", 2_000, UNIX),
        Err(ShareRefusal::Full)
    );
    // The same source again is no second share.
    assert_eq!(
        machine.share_start(
            ShareSource::Window { id: "0x1f".into() },
            "Notlar",
            2_000,
            UNIX
        ),
        Ok(Vec::new())
    );
    // The room refuses it: the publisher stops, the page hears why.
    let effects = machine.share_event(
        ShareEvent::Offer {
            n: 1,
            sdp: Sdp::new("v=0"),
        },
        2_100,
    );
    let rid = open_rid(&effects);
    let effects = machine.socket_message(
        generation,
        FromRoom::Error {
            rid: Some(rid),
            code: "share_full".into(),
            retry_after: None,
            until: None,
            item: None,
            reason: None,
            rule: None,
        },
        2_200,
        UNIX,
    );
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    assert!(sent(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Refused {
            refusal: ShareRefusal::RoomFull
        }]
    );
    assert!(machine.view(2_200, UNIX).share.mine.is_empty());
    // Who may share is the room's: a member it does not allow.
    let mut machine = ready();
    live(&mut machine, Vec::new(), Some(ShareBy::Mods));
    assert!(!machine.view(2_000, UNIX).share.allowed);
    assert_eq!(
        machine.share_start(ShareSource::Game, "", 2_000, UNIX),
        Err(ShareRefusal::NotAllowed)
    );
    // A computer without a publisher only watches.
    let mut machine = ready();
    machine.set_share_publisher(false, false, false);
    live(&mut machine, Vec::new(), share_on());
    assert_eq!(
        machine.share_start(ShareSource::Game, "", 2_000, UNIX),
        Err(ShareRefusal::Unsupported)
    );
}

#[test]
fn the_publishers_and_the_rooms_ends() {
    let mut machine = ready();
    let generation = live(&mut machine, Vec::new(), share_on());
    sharing(&mut machine, generation, 2_000);
    // The window closed.
    let effects = machine.share_event(ShareEvent::SourceGone { n: 1 }, 3_000);
    assert_eq!(sent(&effects), [&ToRoom::ShareClose { n: 1 }]);
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Closed {
            why: ShareClosedWhy::SourceGone,
            src: ShareKind::Game,
        }]
    );
    // A moderator stopped it: nothing more to tell the room.
    sharing(&mut machine, generation, 4_000);
    let effects = machine.socket_message(
        generation,
        FromRoom::ShareClosed {
            n: 1,
            why: ShareClosedWhy::Mod,
        },
        5_000,
        UNIX,
    );
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    assert!(sent(&effects).is_empty());
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Closed {
            why: ShareClosedWhy::Mod,
            src: ShareKind::Game,
        }]
    );
    // The app's sound cannot be had: the share goes on without it.
    sharing(&mut machine, generation, 6_000);
    let effects = machine.share_event(ShareEvent::AudioUnavailable { n: 1 }, 6_100);
    assert_eq!(notices(&effects), [&ShareNotice::AudioOff]);
    assert!(!machine.view(6_100, UNIX).share.mine[0].audio);
    // Leaving the room stops it without a toast.
    let effects = machine.leave(7_000);
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    assert!(notices(&effects).is_empty());
    assert_eq!(privacy(&effects), Some((false, false)));
}

#[test]
fn sharing_off_or_a_lost_share_on_return_ends_it() {
    let mut machine = ready();
    let generation = live(&mut machine, Vec::new(), share_on());
    sharing(&mut machine, generation, 2_000);
    // The socket comes back and the room no longer lists the share.
    machine.socket_closed(generation, None, 3_000);
    let effects = machine.tick(4_000, UNIX, 4_000);
    let generation = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Connect { generation, .. } => Some(*generation),
            _ => None,
        })
        .unwrap();
    let effects = machine.socket_open(generation, 4_100);
    assert!(matches!(sent(&effects)[..], [ToRoom::Sync { caps }] if caps.len() == 4));
    let effects = machine.socket_message(
        generation,
        hello(
            vec![person("me", Vec::new()), person("a", Vec::new())],
            share_on(),
            false,
        ),
        4_200,
        UNIX,
    );
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    // Listed: it goes on. Sharing off: every share ends.
    sharing(&mut machine, generation, 5_000);
    let effects = machine.socket_message(
        generation,
        hello(
            vec![person("me", vec![shared(1)]), person("a", Vec::new())],
            share_on(),
            false,
        ),
        5_100,
        UNIX,
    );
    assert!(to_publisher(&effects).is_empty());
    let effects = machine.socket_message(
        generation,
        hello(vec![person("me", vec![shared(1)])], None, false),
        5_200,
        UNIX,
    );
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Closed {
            why: ShareClosedWhy::Limit,
            src: ShareKind::Game,
        }]
    );
    assert!(!machine.view(5_200, UNIX).share.on);
}

#[test]
fn watching_asks_the_room_and_hands_the_offer_to_the_page() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1)], share_on());
    let view = machine.view(2_000, UNIX).share;
    assert_eq!(view.live.len(), 1);
    assert_eq!(view.live[0].person, "a");
    assert_eq!(view.live[0].kind, ShareKind::Game);
    // The page watches from the main window.
    assert!(machine.share_attach(ShareSurface::Main, 2_000).is_empty());
    let effects = machine.share_watch("a", 1, 2_000).unwrap();
    let rid = watch_rid(&effects);
    assert_eq!(
        sent(&effects)[0].text(),
        format!(r#"{{"t":"watch","u":"a","n":1,"layer":"f","rid":{rid}}}"#)
    );
    assert_eq!(
        machine.view(2_000, UNIX).share.watching[0].phase,
        ShareWatchPhase::Connecting
    );
    let effects = machine.socket_message(
        generation,
        FromRoom::Woffer {
            sdp: Sdp::new("v=0 sfu offer"),
            fresh: true,
            mids: [
                (
                    "0".to_owned(),
                    WatchMidWire {
                        u: "a".into(),
                        n: 1,
                        k: "v".into(),
                    },
                ),
                (
                    "1".to_owned(),
                    WatchMidWire {
                        u: "a".into(),
                        n: 1,
                        k: "a".into(),
                    },
                ),
            ]
            .into(),
        },
        2_100,
        UNIX,
    );
    let pages = to_page(&effects);
    assert!(matches!(
        pages[..],
        [(ShareSurface::Main, WatchCmd::Offer { mids, .. })] if mids.len() == 2 && mids[0].video && !mids[1].video
    ));
    let effects = machine.share_watch_report(
        ShareSurface::Main,
        WatchReport::Answer {
            sdp: "v=0 answer".into(),
        },
        2_200,
    );
    assert_eq!(
        sent(&effects),
        [&ToRoom::Wanswer {
            sdp: Sdp::new("v=0 answer")
        }]
    );
    // Another window's report is not this session's.
    assert!(
        machine
            .share_watch_report(ShareSurface::Popout, WatchReport::Connected, 2_250)
            .is_empty()
    );
    machine.share_watch_report(ShareSurface::Main, WatchReport::Connected, 2_300);
    assert_eq!(
        machine.view(2_300, UNIX).share.watching[0].phase,
        ShareWatchPhase::Live
    );
    // A small tile asks for the small layer, once.
    let effects = machine.share_layer("a", 1, ShareLayer::Q);
    assert_eq!(
        sent(&effects),
        [&ToRoom::Layer {
            u: "a".into(),
            n: 1,
            layer: ShareLayer::Q
        }]
    );
    assert!(machine.share_layer("a", 1, ShareLayer::Q).is_empty());
    // The share ends: the page closes its connection.
    let effects = machine.socket_message(
        generation,
        FromRoom::State {
            u: "a".into(),
            mute: None,
            deaf: None,
            sm: None,
            q: None,
            sh: Some(Vec::new()),
        },
        3_000,
        UNIX,
    );
    assert!(sent(&effects).is_empty());
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: false })]
    ));
    assert!(machine.view(3_000, UNIX).share.watching.is_empty());
    assert_eq!(machine.share_watch("a", 1, 3_000), Err(ShareRefusal::Gone));
}

#[test]
fn at_most_two_watched_and_the_rooms_refusal() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1), shared(2)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    let effects = machine.share_watch("a", 2, 2_000).unwrap();
    let rid = watch_rid(&effects);
    let effects = machine.socket_message(
        generation,
        FromRoom::Error {
            rid: Some(rid),
            code: "watch_full".into(),
            retry_after: None,
            until: None,
            item: None,
            reason: None,
            rule: None,
        },
        2_100,
        UNIX,
    );
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Refused {
            refusal: ShareRefusal::WatchFull
        }]
    );
    assert_eq!(machine.view(2_100, UNIX).share.watching.len(), 1);
    machine.share_watch("a", 2, 2_200).unwrap();
    // A third one is too many (the player's own preview never counts).
    let mut machine = ready();
    live(
        &mut machine,
        vec![shared(1), shared(2), shared(3)],
        share_on(),
    );
    machine.share_watch("a", 1, 2_000).unwrap();
    machine.share_watch("a", 2, 2_000).unwrap();
    assert_eq!(
        machine.share_watch("a", 3, 2_000),
        Err(ShareRefusal::WatchMax)
    );
}

#[test]
fn watching_lets_go_out_of_sight_and_comes_back() {
    let mut machine = ready();
    live(&mut machine, vec![shared(1)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    // A quick minimize costs nothing.
    machine.share_visible(ShareSurface::Main, false, 3_000);
    assert_eq!(machine.next_deadline(3_000), Some(3_000 + HIDDEN_GRACE_MS));
    assert!(
        machine
            .share_visible(ShareSurface::Main, true, 4_000)
            .is_empty()
    );
    // Out of sight for longer: the room lets go, the page closes its connection.
    machine.share_visible(ShareSurface::Main, false, 5_000);
    let effects = machine.tick(5_000 + HIDDEN_GRACE_MS, UNIX, 5_000);
    assert_eq!(
        sent(&effects),
        [&ToRoom::Unwatch {
            u: "a".into(),
            n: 1
        }]
    );
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: false })]
    ));
    assert_eq!(
        machine.view(9_000, UNIX).share.watching[0].phase,
        ShareWatchPhase::Held
    );
    // Held, nothing asks "Hâlâ izliyor musun?" and the inactivity rule runs again.
    let effects = machine.share_visible(ShareSurface::Main, true, 60_000);
    assert!(matches!(sent(&effects)[..], [ToRoom::Watch { .. }]));
}

#[test]
fn the_popout_window_takes_watching_and_gives_it_back() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    let effects = machine.share_popout(true, 3_000);
    assert!(effects.contains(&Effect::SharePopout(true)));
    assert_eq!(
        sent(&effects),
        [&ToRoom::Unwatch {
            u: "a".into(),
            n: 1
        }]
    );
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: false })]
    ));
    assert_eq!(
        machine.view(3_000, UNIX).share.surface,
        ShareSurface::Popout
    );
    // The window's page attaches: it asks the room.
    let effects = machine.share_attach(ShareSurface::Popout, 2_000);
    assert!(matches!(sent(&effects)[..], [ToRoom::Watch { .. }]));
    let effects = machine.socket_message(
        generation,
        FromRoom::Woffer {
            sdp: Sdp::new("v=0"),
            fresh: true,
            mids: Default::default(),
        },
        3_100,
        UNIX,
    );
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Popout, WatchCmd::Offer { .. })]
    ));
    // The window closes: back in the launcher's window.
    let effects = machine.share_detach(ShareSurface::Popout, 4_000);
    assert!(matches!(
        sent(&effects)[..],
        [ToRoom::Unwatch { .. }, ToRoom::Watch { .. }]
    ));
    assert_eq!(machine.view(4_000, UNIX).share.surface, ShareSurface::Main);
}

#[test]
fn still_watching_asks_after_half_an_hour() {
    let mut machine = ready();
    live(&mut machine, vec![shared(1)], share_on());
    // Not attached: no `watch` goes out, so no offer is waited for here.
    machine.share_watch("a", 1, 2_000).unwrap();
    let ask = 2_000 + STILL_ASK_MS;
    assert!(machine.next_deadline(0).is_some_and(|at| at <= ask));
    machine.tick(ask, UNIX, 0);
    assert!(machine.view(ask, UNIX).share.still_asking);
    // "Evet".
    machine.share_still(ask + 1_000);
    assert!(!machine.view(ask + 1_000, UNIX).share.still_asking);
    // No answer: watching stops.
    let again = ask + 1_000 + STILL_ASK_MS;
    machine.tick(again, UNIX, 0);
    let effects = machine.tick(again + STILL_ANSWER_MS, UNIX, 0);
    // Nothing went to the room for it (no page attached), so nothing to let go there.
    assert!(sent(&effects).is_empty());
    assert_eq!(notices(&effects), [&ShareNotice::WatchStopped]);
    assert!(
        machine
            .view(again + STILL_ANSWER_MS, UNIX)
            .share
            .watching
            .is_empty()
    );
    // A touch of the computer answers too.
    machine.share_watch("a", 1, 10 * 60 * MIN).unwrap();
    let ask = 10 * 60 * MIN + STILL_ASK_MS;
    machine.tick(ask, UNIX, 0);
    assert!(machine.view(ask, UNIX).share.still_asking);
    machine.tick(ask + 5_000, UNIX, ask + 4_000);
    assert!(!machine.view(ask + 5_000, UNIX).share.still_asking);
}

#[test]
fn a_streamer_or_viewer_is_no_idle_player() {
    let mut machine = ready();
    let generation = live(&mut machine, Vec::new(), share_on());
    sharing(&mut machine, generation, 2_000);
    machine.socket_message(
        generation,
        FromRoom::Watchers {
            n: 1,
            ids: vec!["a".into()],
        },
        2_100,
        UNIX,
    );
    // Without a touch for an hour (the rule is 30 minutes): still in, still sharing.
    let mut now = 2_100;
    while now < 60 * MIN {
        now += 5 * MIN;
        let effects = machine.tick(now, UNIX, 0);
        assert!(
            !effects.iter().any(|effect| matches!(
                effect,
                Effect::Notice(crate::view::VoiceNotice::Left {
                    reason: VoiceLeftReason::Idle,
                    ..
                })
            )),
            "{now}"
        );
    }
    assert!(machine.in_room());
    // The share ends: the rule counts from then.
    machine.share_stop(1, now);
    assert_eq!(machine.next_deadline(0), Some(now + 30 * MIN));
}

#[test]
fn a_moderator_stops_someones_share() {
    let mut machine = ready();
    live(&mut machine, vec![shared(1)], share_on());
    let (rid, effects) = machine.share_unshare("a", 1).unwrap();
    assert_eq!(
        sent(&effects)[0].text(),
        format!(r#"{{"t":"mod","act":"unshare","to":"a","n":1,"rid":{rid}}}"#)
    );
}

#[test]
fn stream_volumes_are_kept_per_person() {
    let mut machine = ready();
    live(&mut machine, vec![shared(1)], share_on());
    assert!(machine.view(2_000, UNIX).share.volumes.is_empty());
    assert_eq!(
        machine.set_stream_volume("a", 40, false),
        vec![Effect::Changed]
    );
    let volumes = machine.view(2_000, UNIX).share.volumes;
    assert_eq!(volumes["a"].volume, 40);
    assert!(machine.set_stream_volume("a", 40, false).is_empty());
    // SQOL: held to the slider's top (200 % while the voice page plays through WebAudio).
    machine.set_stream_volume("a", 250, true);
    let volumes = machine.view(2_000, UNIX).share.volumes;
    assert_eq!((volumes["a"].volume, volumes["a"].muted), (200, true));
}

/// `hello` again on a socket that came back, the room's watch session carrying `watch`.
fn hello_watching(people: Vec<PersonWire>, watch: &[(&str, u8)]) -> FromRoom {
    let FromRoom::Hello {
        room,
        people,
        rings,
        resume,
        now,
        ..
    } = hello(people, share_on(), false)
    else {
        unreachable!()
    };
    FromRoom::Hello {
        room,
        me: MeWire {
            role: Role::Leader,
            watch: Some(
                watch
                    .iter()
                    .map(|(u, n)| crate::share::WatchWire {
                        u: (*u).to_owned(),
                        n: *n,
                    })
                    .collect(),
            ),
        },
        people,
        rings,
        sdp: None,
        resume,
        now,
    }
}

/// Back on a new socket after the connection dropped (`sync`); its generation.
fn come_back(machine: &mut Machine, generation: u64, at: u64) -> u64 {
    machine.socket_closed(generation, None, at);
    let effects = machine.tick(at + 1_000, UNIX, at + 1_000);
    let generation = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Connect { generation, .. } => Some(*generation),
            _ => None,
        })
        .unwrap();
    machine.socket_open(generation, at + 1_100);
    generation
}

/// The room's `woffer` for a watch session carrying `shares` (their video m-lines).
fn woffer(shares: &[(&str, u8)], fresh: bool) -> FromRoom {
    FromRoom::Woffer {
        sdp: Sdp::new("v=0 sfu offer"),
        fresh,
        mids: shares
            .iter()
            .enumerate()
            .map(|(at, (u, n))| {
                (
                    at.to_string(),
                    WatchMidWire {
                        u: (*u).into(),
                        n: *n,
                        k: "v".into(),
                    },
                )
            })
            .collect(),
    }
}

/// The `turn` request among `effects`.
fn turn_rid(effects: &[Effect]) -> u32 {
    sent(effects)
        .into_iter()
        .find_map(|frame| match frame {
            ToRoom::Turn { rid } => Some(*rid),
            _ => None,
        })
        .expect("a turn")
}

/// The room's TURN for the watch connection, good for two hours (the room's `ttl`).
fn turn_ack(rid: u32) -> FromRoom {
    FromRoom::parse(&format!(
        r#"{{"t":"ack","rid":{rid},"ice":{{"urls":["turn:turn.example:3478"],"username":"u","credential":"c","ttl":7200}}}}"#
    ))
}

#[test]
fn a_socket_that_comes_back_keeps_what_the_room_still_carries() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1), shared(2)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    machine.socket_message(generation, woffer(&[("a", 1)], true), 2_100, UNIX);
    machine.share_watch("a", 2, 2_200).unwrap();
    let generation = come_back(&mut machine, generation, 3_000);
    // The room still carries share 1 (its offer came): only share 2 is asked for again, the
    // page keeps its connection.
    let effects = machine.socket_message(
        generation,
        hello_watching(
            vec![
                person("me", Vec::new()),
                person("a", vec![shared(1), shared(2)]),
            ],
            &[("a", 1)],
        ),
        4_200,
        UNIX,
    );
    assert!(matches!(
        sent(&effects)[..],
        [ToRoom::Set { .. }, ToRoom::Watch { u, n: 2, .. }] if u == "a"
    ));
    assert!(to_page(&effects).is_empty());
    // Nothing carried any more: a new session, the page's connection closes first.
    let generation = come_back(&mut machine, generation, 10_000);
    let effects = machine.socket_message(
        generation,
        hello_watching(
            vec![
                person("me", Vec::new()),
                person("a", vec![shared(1), shared(2)]),
            ],
            &[],
        ),
        11_200,
        UNIX,
    );
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: true })]
    ));
    assert_eq!(
        sent(&effects)
            .iter()
            .filter(|frame| matches!(frame, ToRoom::Watch { .. }))
            .count(),
        2
    );
}

/// MOB-WATCH: a `watch` whose offer went to the old socket is carried by the room but will not
/// be offered again: the room lets it go and it is asked for anew (it was waited for forever).
#[test]
fn a_watch_whose_offer_went_to_the_old_socket_is_asked_for_anew() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1), shared(2)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    let generation = come_back(&mut machine, generation, 3_000);
    let people = || {
        vec![
            person("me", Vec::new()),
            person("a", vec![shared(1), shared(2)]),
        ]
    };
    // The only watch: the session is new (the page's connection closes, TURN it has stays).
    let effects = machine.socket_message(
        generation,
        hello_watching(people(), &[("a", 1)]),
        4_200,
        UNIX,
    );
    assert!(matches!(
        sent(&effects)[..],
        [
            ToRoom::Set { .. },
            ToRoom::Unwatch { n: 1, .. },
            ToRoom::Watch { n: 1, .. }
        ]
    ));
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: true })]
    ));
    // It is waited for again: no offer in time is a lost connection, asked anew.
    let effects = machine.tick(
        4_200 + crate::share::WATCH_OFFER_WAIT_MS,
        UNIX,
        4_200 + crate::share::WATCH_OFFER_WAIT_MS,
    );
    assert!(
        sent(&effects)
            .iter()
            .any(|frame| matches!(frame, ToRoom::Watch { n: 1, .. }))
    );
    // Share 1 came; share 2's offer goes to the old socket: share 1 goes on, share 2 is let go
    // there and asked for anew.
    let now = 4_200 + crate::share::WATCH_OFFER_WAIT_MS;
    machine.socket_message(generation, woffer(&[("a", 1)], true), now + 100, UNIX);
    machine.share_watch("a", 2, now + 200).unwrap();
    let generation = come_back(&mut machine, generation, now + 1_000);
    let effects = machine.socket_message(
        generation,
        hello_watching(people(), &[("a", 1), ("a", 2)]),
        now + 2_200,
        UNIX,
    );
    assert!(matches!(
        sent(&effects)[..],
        [
            ToRoom::Set { .. },
            ToRoom::Unwatch { n: 2, .. },
            ToRoom::Watch { n: 2, .. }
        ]
    ));
    assert!(to_page(&effects).is_empty());
}

/// MOB-WATCH: TURN asked for after ICE failed serves the connection asked for anew after it (the
/// page's TURN was dropped right after it came), until it runs out; it goes when watching ends.
#[test]
fn turn_for_the_watch_connection_stays_for_the_session_asked_anew() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    machine.socket_message(generation, woffer(&[("a", 1)], true), 2_100, UNIX);
    // No connection in 5 s: the page asks for TURN; it comes and goes to the page.
    let effects = machine.share_watch_report(ShareSurface::Main, WatchReport::NeedTurn, 7_100);
    let rid = turn_rid(&effects);
    let effects = machine.socket_message(generation, turn_ack(rid), 7_200, UNIX);
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Ice { .. })]
    ));
    // The page ends that connection: the session is asked anew, TURN stays for it.
    let effects = machine.share_watch_report(ShareSurface::Main, WatchReport::Lost, 7_300);
    assert!(matches!(
        sent(&effects)[..],
        [ToRoom::Unwatch { n: 1, .. }, ToRoom::Watch { n: 1, .. }]
    ));
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: true })]
    ));
    // Through TURN it works; dropped later, still within its two hours: TURN stays.
    machine.socket_message(generation, woffer(&[("a", 1)], true), 7_400, UNIX);
    machine.share_watch_report(ShareSurface::Main, WatchReport::Connected, 8_000);
    let effects = machine.share_watch_report(ShareSurface::Main, WatchReport::Lost, 60 * MIN);
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: true })]
    ));
    // Past its two hours (less a minute to set up on it): it goes, the next connection asks anew.
    machine.socket_message(generation, woffer(&[("a", 1)], true), 60 * MIN + 100, UNIX);
    machine.share_watch_report(ShareSurface::Main, WatchReport::Connected, 60 * MIN + 200);
    let late = 7_200 + 120 * MIN - 60_000;
    let effects = machine.share_watch_report(ShareSurface::Main, WatchReport::Lost, late);
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: false })]
    ));
    // TURN anew, then watching ends: it goes.
    machine.socket_message(generation, woffer(&[("a", 1)], true), late + 100, UNIX);
    let effects = machine.share_watch_report(ShareSurface::Main, WatchReport::NeedTurn, late + 200);
    let rid = turn_rid(&effects);
    machine.socket_message(generation, turn_ack(rid), late + 300, UNIX);
    let effects = machine.share_unwatch("a", 1, late + 400);
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: false })]
    ));
}

/// MOB-WATCH: a TURN answer comes only on the socket that asked; asked with none, or lost with
/// the old one, the connection is asked anew instead of waiting for it.
#[test]
fn a_turn_answer_lost_with_the_socket_is_not_waited_for() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    machine.socket_message(generation, woffer(&[("a", 1)], true), 2_100, UNIX);
    let effects = machine.share_watch_report(ShareSurface::Main, WatchReport::NeedTurn, 7_100);
    turn_rid(&effects);
    // The socket drops before the answer; the room still carries the watch.
    let generation = come_back(&mut machine, generation, 8_000);
    let effects = machine.socket_message(
        generation,
        hello_watching(
            vec![person("me", Vec::new()), person("a", vec![shared(1)])],
            &[("a", 1)],
        ),
        9_200,
        UNIX,
    );
    assert!(matches!(
        sent(&effects)[..],
        [
            ToRoom::Set { .. },
            ToRoom::Unwatch { n: 1, .. },
            ToRoom::Watch { n: 1, .. }
        ]
    ));
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: true })]
    ));
    // No socket to ask on: as a lost connection (the socket's return asks anew).
    machine.socket_message(generation, woffer(&[("a", 1)], true), 9_300, UNIX);
    machine.socket_closed(generation, None, 10_000);
    let effects = machine.share_watch_report(ShareSurface::Main, WatchReport::NeedTurn, 14_300);
    assert!(sent(&effects).is_empty());
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Reset { keep_turn: true })]
    ));
}

#[test]
fn the_rooms_allowed_quality_and_the_months_budget() {
    let mut machine = ready();
    let high = crate::settings::VoiceSettings {
        share: crate::share::VoiceShareSettings {
            quality: SharePreset::High,
            ..crate::share::VoiceShareSettings::default()
        },
        ..crate::settings::VoiceSettings::default()
    };
    machine.save_settings(high, 0);
    let generation = live(&mut machine, Vec::new(), share_on());
    machine
        .share_start(ShareSource::Game, "", 2_000, UNIX)
        .unwrap();
    let effects = machine.share_event(
        ShareEvent::Offer {
            n: 1,
            sdp: Sdp::new("v=0"),
        },
        2_100,
    );
    let rid = open_rid(&effects);
    // The room allowed "Normal" only: the publisher goes on at that.
    let effects = machine.socket_message(
        generation,
        FromRoom::Ack {
            rid: Some(rid),
            ice: None,
            until: None,
            sdp: Some(Sdp::new("v=0 sfu")),
            n: Some(1),
            preset: Some(SharePreset::Normal),
            fps: None,
            audio: None,
            id: None,
            at: None,
            m: None,
        },
        2_200,
        UNIX,
    );
    assert!(matches!(
        to_publisher(&effects)[..],
        [ShareCmd::Answer { n: 1, .. }, ShareCmd::Update { n: 1, opts }]
            if opts.preset == SharePreset::Normal && opts.fps == 30
    ));
    assert_eq!(
        machine.view(2_200, UNIX).share.mine[0].preset,
        SharePreset::Normal
    );
    // The month's budget: no new share, no new watch; the view says so.
    machine.set_share_service(
        Some(ServiceShare {
            open: false,
            watch: false,
            edit: false,
            ..ServiceShare::default()
        }),
        3_000,
    );
    assert!(!machine.view(3_000, UNIX).share.open);
    assert_eq!(
        machine.share_start(ShareSource::Screen { id: "d1".into() }, "", 3_000, UNIX),
        Err(ShareRefusal::Budget)
    );
    // Studio switched sharing off: everything ends here too.
    let effects = machine.set_share_service(None, 4_000);
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    assert!(!machine.view(4_000, UNIX).share.on);
}

#[test]
fn who_may_share_is_the_rooms_and_its_leaders_to_change() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1)], share_on());
    let view = machine.view(2_000, UNIX).share;
    assert_eq!(view.share_by, ShareBy::All);
    assert!(view.can_set_share_by && view.allowed);
    let (rid, effects) = machine.share_set_sharers(ShareBy::Mods).unwrap();
    assert_eq!(
        sent(&effects)[0].text(),
        format!(r#"{{"t":"mod","act":"sharers","who":"mods","rid":{rid}}}"#)
    );
    machine.socket_message(
        generation,
        FromRoom::Room {
            name: None,
            leader: Some("me".into()),
            mods: Vec::new(),
            max: None,
            share_by: Some(ShareBy::Mods),
            ptt: false,
            chat: false,
        },
        2_100,
        UNIX,
    );
    // The leader still may.
    let view = machine.view(2_100, UNIX).share;
    assert_eq!(view.share_by, ShareBy::Mods);
    assert!(view.allowed);
}

#[test]
fn a_fresh_watch_session_is_a_new_connection_on_the_page() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    let effects = machine.socket_message(
        generation,
        FromRoom::parse(
            r#"{"t":"woffer","sdp":"v=0","mids":{"0":{"u":"a","n":1,"k":"v"}},"fresh":true}"#,
        ),
        2_100,
        UNIX,
    );
    assert!(matches!(
        to_page(&effects)[..],
        [(ShareSurface::Main, WatchCmd::Offer { fresh: true, .. })]
    ));
    // The room's "off" reads as the limit's ending.
    assert_eq!(
        serde_json::from_str::<ShareClosedWhy>(r#""off""#).unwrap(),
        ShareClosedWhy::Limit
    );
}

#[test]
fn a_share_waiting_for_its_answer_ends_when_the_socket_comes_back() {
    let mut machine = ready();
    let generation = live(&mut machine, Vec::new(), share_on());
    machine
        .share_start(ShareSource::Game, "", 2_000, UNIX)
        .unwrap();
    machine.share_event(
        ShareEvent::Offer {
            n: 1,
            sdp: Sdp::new("v=0"),
        },
        2_100,
    );
    // The `ack` was lost with the socket; the room lists the share: it is closed there too.
    let generation = come_back(&mut machine, generation, 3_000);
    let effects = machine.socket_message(
        generation,
        hello(
            vec![person("me", vec![shared(1)]), person("a", Vec::new())],
            share_on(),
            false,
        ),
        4_200,
        UNIX,
    );
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    assert!(sent(&effects).contains(&&ToRoom::ShareClose { n: 1 }));
    assert!(machine.view(4_200, UNIX).share.mine.is_empty());
    // A share the room lists that the player no longer has is closed there; one whose watchers
    // came meanwhile is awake.
    sharing(&mut machine, generation, 5_000);
    machine.tick(5_000 + PAUSE_AFTER_MS, UNIX, 5_000 + PAUSE_AFTER_MS);
    let generation = come_back(&mut machine, generation, 70_000);
    let mut watched = shared(1);
    watched.w = 2;
    let effects = machine.socket_message(
        generation,
        hello(
            vec![
                person("me", vec![watched, shared(3)]),
                person("a", Vec::new()),
            ],
            share_on(),
            false,
        ),
        71_200,
        UNIX,
    );
    assert!(sent(&effects).contains(&&ToRoom::ShareClose { n: 3 }));
    assert!(sent(&effects).contains(&&ToRoom::share_paused(1, false)));
    assert!(to_publisher(&effects).contains(&&ShareCmd::KeyFrame { n: 1 }));
    assert_eq!(
        machine.view(71_200, UNIX).share.mine[0].phase,
        ShareMinePhase::Live
    );
}

#[test]
fn a_watch_the_room_leaves_unanswered_is_asked_again_then_given_up() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    assert_eq!(
        machine.next_deadline(2_000),
        Some(2_000 + crate::share::WATCH_OFFER_WAIT_MS)
    );
    let mut now = 2_000;
    let mut asked = 0;
    for _ in 0..crate::share::WATCH_LOSSES_MAX {
        now += crate::share::WATCH_OFFER_WAIT_MS;
        let effects = machine.tick(now, UNIX, now);
        asked += sent(&effects)
            .iter()
            .filter(|frame| matches!(frame, ToRoom::Watch { .. }))
            .count();
        if notices(&effects).contains(&&ShareNotice::WatchFailed) {
            break;
        }
    }
    assert_eq!(asked, 2);
    assert!(machine.view(now, UNIX).share.watching.is_empty());
    let _ = generation;
}

#[test]
fn watching_kept_by_the_room_but_not_by_the_player_is_let_go() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1)], share_on());
    let generation = come_back(&mut machine, generation, 3_000);
    // An `unwatch` lost with the socket: the room still carries share 1.
    let effects = machine.socket_message(
        generation,
        hello_watching(
            vec![person("me", Vec::new()), person("a", vec![shared(1)])],
            &[("a", 1)],
        ),
        4_200,
        UNIX,
    );
    assert!(sent(&effects).contains(&&ToRoom::Unwatch {
        u: "a".into(),
        n: 1
    }));
}

#[test]
fn closing_the_popout_while_the_launcher_is_away_holds_watching() {
    let mut machine = ready();
    live(&mut machine, vec![shared(1)], share_on());
    machine.share_attach(ShareSurface::Main, 2_000);
    machine.share_watch("a", 1, 2_000).unwrap();
    machine.share_popout(true, 2_100);
    machine.share_attach(ShareSurface::Popout, 2_200);
    // The launcher's window goes to the tray meanwhile.
    machine.share_visible(ShareSurface::Main, false, 3_000);
    let effects = machine.share_detach(ShareSurface::Popout, 4_000);
    assert!(
        !sent(&effects)
            .iter()
            .any(|frame| matches!(frame, ToRoom::Watch { .. }))
    );
    assert_eq!(
        machine.view(4_000, UNIX).share.watching[0].phase,
        ShareWatchPhase::Held
    );
    // Back in front: asked again.
    let effects = machine.share_visible(ShareSurface::Main, true, 9_000);
    assert!(matches!(sent(&effects)[..], [ToRoom::Watch { .. }]));
}

/// SHARE-S: the crew room form reads the service's switch and each room's "Kim paylaşabilir",
/// outside a room as well.
#[test]
fn the_crew_room_form_knows_who_may_share() {
    let mut machine = ready();
    machine.crew_rooms(
        "0123456789abcdef",
        vec![
            crate::machine::CrewRoom {
                id: "g0123456789abcdef".into(),
                name: "Genel".into(),
                ..Default::default()
            },
            crate::machine::CrewRoom {
                id: "c0123456789abcdef".into(),
                name: "Oda 2".into(),
                share_mods: true,
                ..Default::default()
            },
        ],
    );
    let share = machine.view(0, UNIX).share;
    assert!(share.service);
    assert_eq!(share.crew_mods, ["c0123456789abcdef"]);
    machine.set_share_service(None, 0);
    assert!(!machine.view(0, UNIX).share.service);
}

// --- SHARE-LIVE (the lead's contract share-live.md §1, §2) ---------------------------------------

/// The voice page's sounds among the effects, in order.
fn sounds(effects: &[Effect]) -> Vec<EngineSound> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Engine(EngineCommand::Sound { sound }) => Some(*sound),
            _ => None,
        })
        .collect()
}

/// `u`'s whole list of live shares, as the room tells it.
fn state(u: &str, sh: Vec<ShareWire>) -> FromRoom {
    FromRoom::State {
        u: u.into(),
        mute: None,
        deaf: None,
        sm: None,
        q: None,
        sh: Some(sh),
    }
}

fn ack(rid: u32, n: u8) -> FromRoom {
    FromRoom::Ack {
        rid: Some(rid),
        ice: None,
        until: None,
        sdp: Some(Sdp::new("v=0 sfu")),
        n: Some(n),
        preset: None,
        fps: None,
        audio: None,
        id: None,
        at: None,
        m: None,
    }
}

/// The player's share of `source` (number `n`), taken by the room at `now`; its effects.
fn share_live(
    machine: &mut Machine,
    generation: u64,
    source: ShareSource,
    n: u8,
    now: u64,
) -> Vec<Effect> {
    machine.share_start(source, "Notlar", now, UNIX).unwrap();
    let effects = machine.share_event(
        ShareEvent::Offer {
            n,
            sdp: Sdp::new("v=0 publisher"),
        },
        now,
    );
    assert!(sounds(&effects).is_empty());
    machine.socket_message(generation, ack(open_rid(&effects), n), now + 100, UNIX)
}

fn with_th(n: u8, th: i64) -> ShareWire {
    ShareWire {
        th: Some(th),
        st: crate::share::ShareSt::Live,
        ..shared(n)
    }
}

#[test]
fn sharing_sounds_follow_the_room() {
    let mut machine = ready();
    // `a` shares already when the player comes in: the room's `hello` says nothing.
    let generation = live(&mut machine, vec![shared(1)], share_on());
    // The player's own share sounds when the room takes it.
    let effects = share_live(&mut machine, generation, ShareSource::Game, 1, 2_000);
    assert_eq!(sounds(&effects), [EngineSound::ShareStart]);
    assert!(machine.view(2_200, UNIX).share.live_now());
    // Someone else starts another: once; a picture's time on a known share is no start.
    let effects = machine.socket_message(
        generation,
        state("a", vec![shared(1), shared(2)]),
        3_000,
        UNIX,
    );
    assert_eq!(sounds(&effects), [EngineSound::StreamStart]);
    let effects = machine.socket_message(
        generation,
        state("a", vec![with_th(1, UNIX + 60), shared(2)]),
        3_500,
        UNIX,
    );
    assert!(sounds(&effects).is_empty());
    // A watched share that ends sounds; one the player lets go of does not.
    machine.share_watch("a", 1, 4_000).unwrap();
    machine.share_watch("a", 2, 4_000).unwrap();
    let effects = machine.socket_message(generation, state("a", vec![shared(2)]), 5_000, UNIX);
    assert_eq!(sounds(&effects), [EngineSound::StreamEnd]);
    assert!(sounds(&machine.share_unwatch("a", 2, 6_000)).is_empty());
    // A share nobody here watched ends quietly.
    let effects = machine.socket_message(generation, state("a", Vec::new()), 7_000, UNIX);
    assert!(sounds(&effects).is_empty());
    // A watched sharer who leaves: the leaving and the stream's end.
    let effects = machine.socket_message(generation, state("a", vec![shared(3)]), 8_000, UNIX);
    assert_eq!(sounds(&effects), [EngineSound::StreamStart]);
    machine.share_watch("a", 3, 8_100).unwrap();
    let effects = machine.socket_message(
        generation,
        FromRoom::Left {
            u: "a".into(),
            why: LeftWhy::Left,
        },
        9_000,
        UNIX,
    );
    assert!(sounds(&effects).contains(&EngineSound::StreamEnd));
    // The player's live share ends: its sound; one that never went live says nothing.
    assert_eq!(
        sounds(&machine.share_stop(1, 10_000)),
        [EngineSound::ShareStop]
    );
    assert!(!machine.view(10_000, UNIX).share.live_now());
    machine
        .share_start(ShareSource::Game, "", 11_000, UNIX)
        .unwrap();
    assert!(sounds(&machine.share_stop(1, 11_500)).is_empty());
}

#[test]
fn sharing_sounds_once_together_and_never_on_a_catch_up() {
    let mut machine = ready();
    let generation = live(&mut machine, Vec::new(), share_on());
    share_live(&mut machine, generation, ShareSource::Game, 1, 2_000);
    // Two shares that start within a second sound once.
    let window = || ShareSource::Window { id: "0x1f".into() };
    let effects = share_live(&mut machine, generation, window(), 2, 2_500);
    assert!(sounds(&effects).is_empty());
    // Both end together (Studio's switch): one sound.
    let effects = machine.set_share_service(None, 5_000);
    assert_eq!(sounds(&effects), [EngineSound::ShareStop]);
    machine.set_share_service(
        Some(ServiceShare {
            per_user: 2,
            max_preset: SharePreset::High,
            open: true,
            watch: true,
            edit: false,
        }),
        5_100,
    );
    // Someone's share that the room's `hello` lists after a reconnect is no start.
    let generation = come_back(&mut machine, generation, 6_000);
    let effects = machine.socket_message(
        generation,
        hello_watching(
            vec![person("me", Vec::new()), person("a", vec![shared(1)])],
            &[],
        ),
        7_200,
        UNIX,
    );
    assert!(sounds(&effects).is_empty());
    // Studio's switch off ends what the player watches: its sound.
    machine.share_watch("a", 1, 8_000).unwrap();
    let effects = machine.set_share_service(None, 9_000);
    assert_eq!(sounds(&effects), [EngineSound::StreamEnd]);
}

#[test]
fn sharing_sounds_keep_voice_chat_sounds_off() {
    let mut machine = ready();
    machine.save_settings(
        VoiceSettings {
            sounds: false,
            ..VoiceSettings::default()
        },
        0,
    );
    let generation = live(&mut machine, Vec::new(), share_on());
    let effects = share_live(&mut machine, generation, ShareSource::Game, 1, 2_000);
    assert!(sounds(&effects).is_empty());
    let effects = machine.socket_message(generation, state("a", vec![shared(1)]), 3_000, UNIX);
    assert!(sounds(&effects).is_empty());
    assert!(sounds(&machine.share_stop(1, 4_000)).is_empty());
}

#[test]
fn a_live_shares_still_picture_goes_to_the_room() {
    let mut machine = ready();
    let generation = live(&mut machine, vec![shared(1)], share_on());
    let jpeg = |len: usize| {
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0];
        bytes.resize(len, 7);
        ThumbJpeg(bytes)
    };
    machine
        .share_start(ShareSource::Game, "", 2_000, UNIX)
        .unwrap();
    // Before the room took the share: nothing goes.
    let effects = machine.share_event(
        ShareEvent::Thumb {
            n: 1,
            jpg: jpeg(100),
        },
        2_000,
    );
    assert!(sent(&effects).is_empty());
    let effects = machine.share_event(
        ShareEvent::Offer {
            n: 1,
            sdp: Sdp::new("v=0 publisher"),
        },
        2_000,
    );
    machine.socket_message(generation, ack(open_rid(&effects), 1), 2_100, UNIX);
    let effects = machine.share_event(ShareEvent::Thumb { n: 1, jpg: jpeg(6) }, 5_000);
    assert_eq!(
        sent(&effects),
        [&ToRoom::ShareThumb {
            n: 1,
            jpg: "/9j/4AcH".into()
        }]
    );
    // Not a JPEG, too large, or no such share: nothing.
    for (n, jpg) in [
        (1, ThumbJpeg(b"\x89PNG\r\n".to_vec())),
        (1, jpeg(THUMB_MAX_BYTES + 1)),
        (2, jpeg(100)),
    ] {
        assert!(sent(&machine.share_event(ShareEvent::Thumb { n, jpg }, 5_100)).is_empty());
    }
    // The room's times of the pictures: the player's own and someone else's.
    machine.socket_message(
        generation,
        state("me", vec![with_th(1, UNIX + 5)]),
        6_000,
        UNIX,
    );
    machine.socket_message(
        generation,
        state("a", vec![with_th(1, UNIX + 30)]),
        6_100,
        UNIX,
    );
    let view = machine.view(6_200, UNIX).share;
    assert_eq!(view.mine[0].th, Some(UNIX + 5));
    assert_eq!(view.live[0].th, Some(UNIX + 30));
    // The bytes never show in a log line.
    assert_eq!(format!("{:?}", jpeg(6)), "<jpeg 6 bytes>");
}

#[test]
fn crew_rooms_say_who_is_live() {
    let mut machine = ready();
    machine.crew_rooms(
        "0123456789abcdef",
        vec![CrewRoom {
            id: "g0123456789abcdef".into(),
            name: "Genel".into(),
            ids: vec!["a".into(), "b".into()],
            // Someone the list does not show is not told.
            live: vec!["b".into(), "c".into()],
            ..CrewRoom::default()
        }],
    );
    let crews = machine.view(0, UNIX).crews;
    assert_eq!(crews[0].rooms[0].live, ["b"]);
}
