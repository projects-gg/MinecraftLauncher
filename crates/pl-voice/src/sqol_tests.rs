//! SQOL's screen sharing scenarios on the machine (the lead's contract `docs/tasks/sqol.md` §1.2,
//! §2.1, §3): live changes of the player's own shares, a closed source's minute, what watchers see.

use std::time::Duration;

use pl_social::BackoffPolicy;

use crate::engine::PageReport;
use crate::machine::{Effect, Machine, ServiceVoice, StayStart};
use crate::settings::Store;
use crate::share::{
    HOLD_STOP_MS, ServiceShare, ShareAsked, ShareBy, ShareChange, ShareClosedWhy, ShareCmd,
    ShareEvent, ShareHoldWhy, ShareKind, ShareMotion, ShareNotice, SharePreset, ShareRefusal,
    ShareSource, ShareSt, ShareWire, ThumbJpeg,
};
use crate::wire::{FromRoom, MeWire, PersonWire, Role, RoomKind, RoomWire, Sdp, Ticket, ToRoom};

const ROOM: &str = "f0123456789abcdef";
const UNIX: i64 = 1_800_000_000;

/// Signed in, the service sharing (`edit`: live changes allowed), this computer sharing with sound
/// and a hardware encoder (`high`).
fn ready(edit: bool, high: bool) -> Machine {
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
    machine.set_share_publisher(true, true, high);
    machine.set_share_service(
        Some(ServiceShare {
            per_user: 2,
            max_preset: SharePreset::High,
            open: true,
            watch: true,
            edit,
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

/// In a friend room with `a` (sharing `a_shares`), the room sharing.
fn live(machine: &mut Machine, a_shares: Vec<ShareWire>) -> u64 {
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
        FromRoom::Hello {
            room: RoomWire {
                id: ROOM.into(),
                kind: RoomKind::Friend,
                crew: None,
                name: None,
                leader: Some("me".into()),
                mods: Vec::new(),
                max: 10,
                share_by: Some(ShareBy::All),
                ptt: false,
                chat: false,
                grew: false,
            },
            me: MeWire {
                role: Role::Leader,
                watch: None,
            },
            people: vec![person("me", Vec::new()), person("a", a_shares)],
            rings: Vec::new(),
            sdp: Some(Sdp::new("v=0 answer")),
            resume: Some(Ticket::new("v1.resume.mac")),
            now: Some(UNIX),
        },
        1_300,
        UNIX,
    );
    machine.page_report(PageReport::Connected, 1_400);
    generation
}

/// The room's `ack` of `rid` (`share_open`'s answer, or a live change's applied values).
fn ack(
    rid: u32,
    sdp: Option<Sdp>,
    preset: Option<SharePreset>,
    fps: Option<u8>,
    audio: Option<bool>,
) -> FromRoom {
    FromRoom::Ack {
        rid: Some(rid),
        ice: None,
        until: None,
        sdp,
        n: Some(1),
        preset,
        fps,
        audio,
        id: None,
        at: None,
        m: None,
    }
}

/// A live change's applied values.
fn applied(rid: u32, preset: SharePreset, fps: u8, audio: bool) -> FromRoom {
    ack(rid, None, Some(preset), Some(fps), Some(audio))
}

fn error(rid: u32, code: &str) -> FromRoom {
    FromRoom::Error {
        rid: Some(rid),
        code: code.into(),
        retry_after: None,
        until: None,
        item: None,
        reason: None,
        rule: None,
    }
}

/// The player shares `source` as share 1, the room took it, and `a` watches.
fn sharing(machine: &mut Machine, generation: u64, source: ShareSource, now: u64) {
    machine.share_start(source, "Notlar", now, UNIX).unwrap();
    let effects = machine.share_event(
        ShareEvent::Offer {
            n: 1,
            sdp: Sdp::new("v=0 publisher"),
        },
        now,
    );
    let rid = sent(&effects)
        .into_iter()
        .find_map(|frame| match frame {
            ToRoom::ShareOpen { rid, .. } => Some(*rid),
            _ => None,
        })
        .unwrap();
    machine.socket_message(
        generation,
        ack(rid, Some(Sdp::new("v=0 sfu")), None, None, None),
        now,
        UNIX,
    );
    machine.socket_message(
        generation,
        FromRoom::Watchers {
            n: 1,
            ids: vec!["a".into()],
        },
        now,
        UNIX,
    );
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

fn notices(effects: &[Effect]) -> Vec<&ShareNotice> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::ShareNotice(notice) => Some(notice),
            _ => None,
        })
        .collect()
}

/// The `rid` of the first `share_set` with one.
fn set_rid(effects: &[Effect]) -> Option<u32> {
    sent(effects).into_iter().find_map(|frame| match frame {
        ToRoom::ShareSet { rid, .. } => *rid,
        _ => None,
    })
}

fn window(id: &str) -> ShareSource {
    ShareSource::Window { id: id.into() }
}

#[test]
fn a_switch_keeps_the_watchers_and_tells_the_room_the_new_kind() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    let view = machine.view(2_000, UNIX).share;
    assert!(view.edit);
    assert_eq!(view.hw_max, SharePreset::High);
    assert_eq!(view.mine[0].source, ShareSource::Game);
    assert_eq!(view.mine[0].st, ShareSt::Live);
    // "Ekranı Değiştir": the publisher switches; nothing goes to the room yet.
    let effects = machine
        .share_switch(1, window("10:20"), "Notlar - Not Defteri")
        .unwrap();
    assert_eq!(
        to_publisher(&effects),
        [&ShareCmd::Switch {
            n: 1,
            source: window("10:20")
        }]
    );
    assert!(sent(&effects).is_empty());
    // It went: "Artık … paylaşılıyor.", the room hears the new kind (with `rid`), a game's
    // motion goes back to sharp text.
    let effects = machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1280,
            height: 720,
            minimized: false,
        },
        3_000,
    );
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Switched {
            n: 1,
            src: ShareKind::Window,
            label: "Notlar - Not Defteri".into()
        }]
    );
    assert!(matches!(
        sent(&effects)[..],
        [ToRoom::ShareSet {
            n: 1,
            rid: Some(_),
            src: Some(ShareKind::Window),
            preset: None,
            fps: None,
            audio: None,
            st: None,
            paused: None
        }]
    ));
    assert!(matches!(
        to_publisher(&effects)[..],
        [ShareCmd::Update { n: 1, opts }] if !opts.motion
    ));
    let view = machine.view(3_000, UNIX).share;
    assert_eq!(view.mine[0].kind, ShareKind::Window);
    assert_eq!(view.mine[0].label, "Notlar - Not Defteri");
    assert_eq!(view.mine[0].source, window("10:20"));
    // The room's answer changes nothing more.
    let rid = set_rid(&effects).unwrap();
    let effects = machine.socket_message(
        generation,
        applied(rid, SharePreset::Normal, 30, true),
        3_100,
        UNIX,
    );
    assert!(to_publisher(&effects).is_empty());
}

#[test]
fn a_switch_is_refused_when_it_cannot_be() {
    let mut machine = ready(false, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    // The service does not allow live changes: no live rows at all.
    assert!(!machine.view(2_000, UNIX).share.edit);
    assert_eq!(
        machine.share_switch(1, window("10:20"), ""),
        Err(ShareRefusal::Off)
    );
    assert_eq!(machine.share_hold(1, true), Err(ShareRefusal::Off));
    assert_eq!(
        machine.share_update(
            1,
            ShareChange {
                preset: Some(SharePreset::Low),
                ..ShareChange::default()
            },
            2_000
        ),
        Err(ShareRefusal::Off)
    );
    // "İmleci Göster" is this computer's own: it goes.
    assert!(
        machine
            .share_update(
                1,
                ShareChange {
                    cursor: Some(false),
                    ..ShareChange::default()
                },
                2_000
            )
            .is_ok()
    );
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    machine
        .share_start(window("30:40"), "Tarayıcı", 2_100, UNIX)
        .unwrap();
    // Another of the player's shares has it; the same source again does nothing; no such share.
    assert_eq!(
        machine.share_switch(1, window("30:40"), ""),
        Err(ShareRefusal::InUse)
    );
    assert_eq!(
        machine.share_switch(1, ShareSource::Game, ""),
        Ok(Vec::new())
    );
    assert_eq!(
        machine.share_switch(3, window("50:60"), ""),
        Err(ShareRefusal::Gone)
    );
    // A share still starting cannot switch yet.
    assert_eq!(
        machine.share_switch(2, window("50:60"), ""),
        Err(ShareRefusal::Failed)
    );
}

#[test]
fn a_failed_switch_keeps_the_old_source() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    machine.share_switch(1, window("10:20"), "Notlar").unwrap();
    let effects = machine.share_event(ShareEvent::SwitchFailed { n: 1 }, 2_500);
    assert_eq!(notices(&effects), [&ShareNotice::SwitchFailed { n: 1 }]);
    let view = machine.view(2_500, UNIX).share;
    assert_eq!(view.mine[0].source, ShareSource::Game);
    assert_eq!(view.mine[0].kind, ShareKind::Game);
}

#[test]
fn a_closed_window_holds_a_minute_then_ends() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, window("10:20"), 2_000);
    let effects = machine.share_event(ShareEvent::SourceGone { n: 1 }, 3_000);
    assert_eq!(
        notices(&effects),
        [&ShareNotice::SourceClosed {
            n: 1,
            src: ShareKind::Window
        }]
    );
    // Watchers see "Yayın duraklatıldı.": the room hears `st` (no answer asked).
    assert_eq!(sent(&effects), [&ToRoom::share_st(1, ShareSt::Hold)]);
    assert!(to_publisher(&effects).is_empty());
    let view = machine.view(3_000, UNIX).share;
    assert_eq!(view.mine[0].st, ShareSt::Hold);
    assert_eq!(view.mine[0].hold, Some(ShareHoldWhy::Closed));
    assert_eq!(
        view.mine[0].hold_until,
        Some(UNIX + (HOLD_STOP_MS / 1_000) as i64)
    );
    // "Sürdür" has nothing to go on with.
    assert_eq!(machine.share_hold(1, false), Err(ShareRefusal::Gone));
    // A minute without another source: "Paylaşım durdu.".
    assert_eq!(machine.next_deadline(3_000), Some(3_000 + HOLD_STOP_MS));
    let effects = machine.tick(3_000 + HOLD_STOP_MS, UNIX, 0);
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    assert!(sent(&effects).contains(&&ToRoom::ShareClose { n: 1 }));
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Closed {
            why: ShareClosedWhy::SourceGone,
            src: ShareKind::Window
        }]
    );
    assert!(machine.view(70_000, UNIX).share.mine.is_empty());
}

#[test]
fn another_source_ends_the_wait() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, window("10:20"), 2_000);
    machine.share_event(ShareEvent::SourceGone { n: 1 }, 3_000);
    // "Başka Ekran Seç".
    machine
        .share_switch(
            1,
            ShareSource::Screen {
                id: r"\\.\DISPLAY1".into(),
            },
            "",
        )
        .unwrap();
    let effects = machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1920,
            height: 1080,
            minimized: false,
        },
        4_000,
    );
    // The publisher's blank picture ends; watchers see the screen again; its sound is the whole
    // computer's.
    assert!(to_publisher(&effects).contains(&&ShareCmd::Hold { n: 1, on: false }));
    assert!(to_publisher(&effects).iter().any(|cmd| matches!(
        cmd,
        ShareCmd::Update { n: 1, opts } if opts.audio_system
    )));
    assert!(sent(&effects).contains(&&ToRoom::share_st(1, ShareSt::Live)));
    let view = machine.view(4_000, UNIX).share;
    assert_eq!(view.mine[0].st, ShareSt::Live);
    assert_eq!((view.mine[0].hold, view.mine[0].hold_until), (None, None));
    assert_eq!(view.mine[0].kind, ShareKind::Screen);
    // The minute no longer runs.
    let effects = machine.tick(3_000 + HOLD_STOP_MS, UNIX, 0);
    assert!(to_publisher(&effects).is_empty() && notices(&effects).is_empty());
    assert_eq!(machine.view(70_000, UNIX).share.mine.len(), 1);
}

#[test]
fn a_game_the_launcher_starts_takes_the_closed_games_place() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    // Nothing to take while the game shows.
    assert!(machine.share_game_started().is_empty());
    machine.share_event(ShareEvent::SourceGone { n: 1 }, 3_000);
    assert_eq!(
        machine.view(3_000, UNIX).share.mine[0].hold,
        Some(ShareHoldWhy::GameClosed)
    );
    let effects = machine.share_game_started();
    assert_eq!(
        to_publisher(&effects),
        [&ShareCmd::Switch {
            n: 1,
            source: ShareSource::Game
        }]
    );
    // Asked once.
    assert!(machine.share_game_started().is_empty());
    // Its window did not come in time: no word, the wait goes on.
    let effects = machine.share_event(ShareEvent::SwitchFailed { n: 1 }, 20_000);
    assert!(notices(&effects).is_empty());
    machine.share_game_started();
    let effects = machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1280,
            height: 720,
            minimized: false,
        },
        30_000,
    );
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Switched {
            n: 1,
            src: ShareKind::Game,
            label: String::new()
        }]
    );
    assert_eq!(machine.view(30_000, UNIX).share.mine[0].st, ShareSt::Live);
}

#[test]
fn the_games_recreated_window_goes_on_without_a_word() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    let effects = machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1920,
            height: 1080,
            minimized: false,
        },
        3_000,
    );
    assert!(notices(&effects).is_empty());
    assert!(sent(&effects).is_empty());
    assert_eq!(
        machine.view(3_000, UNIX).share.mine[0].kind,
        ShareKind::Game
    );
}

#[test]
fn without_live_changes_a_closed_source_ends_at_once() {
    let mut machine = ready(false, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, window("10:20"), 2_000);
    let effects = machine.share_event(ShareEvent::SourceGone { n: 1 }, 3_000);
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Closed {
            why: ShareClosedWhy::SourceGone,
            src: ShareKind::Window
        }]
    );
}

#[test]
fn a_pause_holds_a_blank_picture_until_resumed() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    let effects = machine.share_hold(1, true).unwrap();
    assert_eq!(to_publisher(&effects), [&ShareCmd::Hold { n: 1, on: true }]);
    assert_eq!(sent(&effects), [&ToRoom::share_st(1, ShareSt::Hold)]);
    assert_eq!(
        machine.view(2_000, UNIX).share.mine[0].hold,
        Some(ShareHoldWhy::Paused)
    );
    // Twice is once.
    assert_eq!(machine.share_hold(1, true), Ok(Vec::new()));
    // A minimized window meanwhile: still "hold" for watchers.
    let effects = machine.share_event(ShareEvent::Minimized { n: 1, on: true }, 2_500);
    assert!(sent(&effects).is_empty());
    let effects = machine.share_hold(1, false).unwrap();
    assert_eq!(
        to_publisher(&effects),
        [&ShareCmd::Hold { n: 1, on: false }]
    );
    assert_eq!(sent(&effects), [&ToRoom::share_st(1, ShareSt::Min)]);
    let effects = machine.share_event(ShareEvent::Minimized { n: 1, on: false }, 3_000);
    assert_eq!(sent(&effects), [&ToRoom::share_st(1, ShareSt::Live)]);
    // The text form the room reads.
    assert_eq!(
        ToRoom::share_st(1, ShareSt::Min).text(),
        r#"{"t":"share_set","n":1,"st":"min"}"#
    );
}

#[test]
fn a_live_change_waits_for_the_rooms_applied_values() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    // "Yüksek" at 60: the room is asked; nothing changes before its answer.
    let effects = machine
        .share_update(
            1,
            ShareChange {
                preset: Some(SharePreset::High),
                fps: Some(60),
                ..ShareChange::default()
            },
            2_000,
        )
        .unwrap();
    assert!(to_publisher(&effects).is_empty());
    assert_eq!(
        sent(&effects)[0].text(),
        format!(
            r#"{{"t":"share_set","n":1,"rid":{},"preset":"high","fps":60}}"#,
            set_rid(&effects).unwrap()
        )
    );
    // The room allows "Normal" (the month's budget): the share goes on at that.
    let effects = machine.socket_message(
        generation,
        applied(set_rid(&effects).unwrap(), SharePreset::Normal, 30, true),
        2_100,
        UNIX,
    );
    assert!(to_publisher(&effects).is_empty(), "nothing changed");
    // "Düşük": applied as the room says.
    let effects = machine
        .share_update(
            1,
            ShareChange {
                preset: Some(SharePreset::Low),
                audio: Some(false),
                ..ShareChange::default()
            },
            3_000,
        )
        .unwrap();
    let effects = machine.socket_message(
        generation,
        applied(set_rid(&effects).unwrap(), SharePreset::Low, 30, false),
        3_100,
        UNIX,
    );
    assert!(matches!(
        to_publisher(&effects)[..],
        [ShareCmd::Update { n: 1, opts }]
            if opts.preset == SharePreset::Low && opts.fps == 30 && !opts.audio
    ));
    let view = machine.view(3_100, UNIX).share;
    assert_eq!(
        (view.mine[0].preset, view.mine[0].audio),
        (SharePreset::Low, false)
    );
    // "Görüntü" and the cursor are this computer's: at once, nothing to the room.
    let effects = machine
        .share_update(
            1,
            ShareChange {
                motion: Some(ShareMotion::Clarity),
                cursor: Some(false),
                ..ShareChange::default()
            },
            4_000,
        )
        .unwrap();
    assert!(sent(&effects).is_empty());
    assert!(matches!(
        to_publisher(&effects)[..],
        [ShareCmd::Update { n: 1, opts }] if !opts.motion && !opts.cursor
    ));
    let view = machine.view(4_000, UNIX).share;
    assert_eq!(view.mine[0].motion, ShareMotion::Clarity);
    assert!(!view.mine[0].cursor);
}

#[test]
fn live_changes_hold_to_what_is_allowed() {
    // No hardware encoder: "Yüksek" is "Normal" here; 60 frames need "Yüksek".
    let mut machine = ready(true, false);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    assert_eq!(machine.view(2_000, UNIX).share.hw_max, SharePreset::Normal);
    let effects = machine
        .share_update(
            1,
            ShareChange {
                preset: Some(SharePreset::High),
                fps: Some(60),
                ..ShareChange::default()
            },
            2_000,
        )
        .unwrap();
    assert!(
        sent(&effects).is_empty(),
        "nothing to ask: it is that already"
    );
    // A refusal for the month's budget is told; the room not taking changes yet is not.
    let effects = machine
        .share_update(
            1,
            ShareChange {
                fps: Some(15),
                ..ShareChange::default()
            },
            3_000,
        )
        .unwrap();
    let effects = machine.socket_message(
        generation,
        error(set_rid(&effects).unwrap(), "budget"),
        3_100,
        UNIX,
    );
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Refused {
            refusal: ShareRefusal::Budget,
            n: Some(1),
            change: Some(ShareAsked {
                fps: Some(15),
                ..ShareAsked::default()
            }),
        }]
    );
    let effects = machine
        .share_update(
            1,
            ShareChange {
                fps: Some(15),
                ..ShareChange::default()
            },
            4_000,
        )
        .unwrap();
    // SQOL-R3: any other refusal of the player's change is told too, with what it asked (the
    // page never rolls back silently).
    let effects = machine.socket_message(
        generation,
        error(set_rid(&effects).unwrap(), "share_off"),
        4_100,
        UNIX,
    );
    assert!(matches!(
        notices(&effects)[..],
        [ShareNotice::Refused {
            n: Some(1),
            change: Some(ShareAsked {
                fps: Some(15),
                preset: None,
                audio: None
            }),
            ..
        }]
    ));
    assert_eq!(machine.view(4_100, UNIX).share.mine[0].fps, 30);
    // An answer that never comes changes nothing after a while, and says so.
    let effects = machine
        .share_update(
            1,
            ShareChange {
                fps: Some(15),
                ..ShareChange::default()
            },
            5_000,
        )
        .unwrap();
    assert!(set_rid(&effects).is_some());
    assert!(machine.next_deadline(5_000).is_some_and(|at| at <= 15_000));
    let effects = machine.tick(15_000, UNIX, 15_000);
    assert_eq!(
        notices(&effects),
        [&ShareNotice::Refused {
            refusal: ShareRefusal::Failed,
            n: Some(1),
            change: Some(ShareAsked {
                fps: Some(15),
                ..ShareAsked::default()
            }),
        }]
    );
    assert_eq!(machine.view(15_000, UNIX).share.mine[0].fps, 30);
}

/// SQOL-R3: the launcher's own report of a switch's kind (nothing the player asked) stays quiet
/// when a room that does not take changes refuses it.
#[test]
fn a_refused_report_of_a_switchs_kind_stays_quiet() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    machine
        .share_switch(1, window("30:40"), "Tarayıcı")
        .unwrap();
    let effects = machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1280,
            height: 720,
            minimized: false,
        },
        2_500,
    );
    let rid = set_rid(&effects).expect("the new kind goes to the room");
    let effects = machine.socket_message(generation, error(rid, "bad_request"), 2_600, UNIX);
    assert!(notices(&effects).is_empty());
}

#[test]
fn the_still_picture_goes_only_while_chosen() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    let jpg = || ThumbJpeg(vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3]);
    let effects = machine.share_event(ShareEvent::Thumb { n: 1, jpg: jpg() }, 5_000);
    assert!(matches!(
        sent(&effects)[..],
        [ToRoom::ShareThumb { n: 1, .. }]
    ));
    machine
        .share_update(
            1,
            ShareChange {
                thumbs: Some(false),
                ..ShareChange::default()
            },
            6_000,
        )
        .unwrap();
    assert!(!machine.view(6_000, UNIX).share.mine[0].thumbs);
    let effects = machine.share_event(ShareEvent::Thumb { n: 1, jpg: jpg() }, 65_000);
    assert!(sent(&effects).is_empty());
}

#[test]
fn the_preview_goes_to_the_publisher() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    assert_eq!(
        to_publisher(&machine.share_preview(1, true)),
        [&ShareCmd::Preview { n: 1, on: true }]
    );
    assert!(machine.share_preview(2, true).is_empty());
}

#[test]
fn others_shares_say_what_watchers_see_and_their_sound_goes_to_two_hundred() {
    let mut machine = ready(true, true);
    let wire: ShareWire = serde_json::from_value(serde_json::json!({
        "n": 1, "src": "window", "audio": true, "preset": "normal", "fps": 30, "st": "min"
    }))
    .unwrap();
    live(&mut machine, vec![wire]);
    let view = machine.view(2_000, UNIX).share;
    assert_eq!(view.live[0].st, ShareSt::Min);
    machine.set_stream_volume("a", 180, false);
    let view = machine.view(2_000, UNIX);
    assert_eq!(view.volume_max, 200);
    assert_eq!(view.share.volumes["a"].volume, 180);
    machine.set_stream_volume("a", 900, false);
    assert_eq!(machine.view(2_000, UNIX).share.volumes["a"].volume, 200);
}

/// SQOL-R3 (P2-5): a pause the player chose before the source closed stays after another source
/// comes; without one the share goes on.
#[test]
fn a_pause_chosen_before_the_source_closed_stays_after_a_switch() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, window("10:20"), 2_000);
    machine.share_hold(1, true).unwrap();
    machine.share_event(ShareEvent::SourceGone { n: 1 }, 3_000);
    assert_eq!(
        machine.view(3_000, UNIX).share.mine[0].hold,
        Some(ShareHoldWhy::Closed)
    );
    machine
        .share_switch(1, window("30:40"), "Tarayıcı")
        .unwrap();
    let effects = machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1280,
            height: 720,
            minimized: false,
        },
        3_500,
    );
    assert!(
        !to_publisher(&effects)
            .iter()
            .any(|cmd| matches!(cmd, ShareCmd::Hold { on: false, .. }))
    );
    let view = machine.view(3_500, UNIX).share;
    assert_eq!(view.mine[0].hold, Some(ShareHoldWhy::Paused));
    assert_eq!(view.mine[0].st, ShareSt::Hold);
    // "Sürdür" goes on with the new source.
    let effects = machine.share_hold(1, false).unwrap();
    assert_eq!(
        to_publisher(&effects),
        [&ShareCmd::Hold { n: 1, on: false }]
    );
    // Not paused before: the new source shows.
    machine.share_event(ShareEvent::SourceGone { n: 1 }, 4_000);
    machine.share_switch(1, window("50:60"), "Not").unwrap();
    let effects = machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1280,
            height: 720,
            minimized: false,
        },
        4_500,
    );
    assert!(to_publisher(&effects).contains(&&ShareCmd::Hold { n: 1, on: false }));
    assert_eq!(machine.view(4_500, UNIX).share.mine[0].hold, None);
}

/// SQOL-R3 (P2-6): a change made back before the room answered the first is asked for too; the
/// share ends as the player left it.
#[test]
fn a_quick_change_back_is_asked_for_and_wins() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    let before = machine.view(2_000, UNIX).share.mine[0].clone();
    let on = !before.audio;
    let change = |audio| ShareChange {
        audio: Some(audio),
        ..ShareChange::default()
    };
    let first = set_rid(&machine.share_update(1, change(on), 2_100).unwrap()).unwrap();
    // Back at once, the room's answer to the first still on its way: asked as well.
    let effects = machine.share_update(1, change(!on), 2_200).unwrap();
    let second = set_rid(&effects).expect("the change back is asked for");
    assert!(matches!(
        sent(&effects)[..],
        [ToRoom::ShareSet { audio: Some(audio), .. }] if *audio == !on
    ));
    // The answers come in order: the share ends as the player left it.
    machine.socket_message(
        generation,
        applied(first, before.preset, before.fps, on),
        2_300,
        UNIX,
    );
    machine.socket_message(
        generation,
        applied(second, before.preset, before.fps, !on),
        2_400,
        UNIX,
    );
    assert_eq!(machine.view(2_400, UNIX).share.mine[0].audio, !on);
    // The same for the quality: another one, then back.
    let other = if before.preset == SharePreset::Low {
        SharePreset::Normal
    } else {
        SharePreset::Low
    };
    let low = ShareChange {
        preset: Some(other),
        ..ShareChange::default()
    };
    let back = ShareChange {
        preset: Some(before.preset),
        ..ShareChange::default()
    };
    assert!(set_rid(&machine.share_update(1, low, 2_500).unwrap()).is_some());
    assert!(set_rid(&machine.share_update(1, back, 2_600).unwrap()).is_some());
    // Nothing waits any more: the same value again asks nothing.
    machine.tick(2_600 + 10_000, UNIX, 0);
    assert!(set_rid(&machine.share_update(1, back, 13_000).unwrap()).is_none());
}

/// SQOL-R3 (P2-7): the picker's double click asks for one switch.
#[test]
fn a_switch_to_the_source_already_on_its_way_is_one_switch() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, window("10:20"), 2_000);
    let effects = machine
        .share_switch(1, window("30:40"), "Tarayıcı")
        .unwrap();
    assert!(!to_publisher(&effects).is_empty());
    assert_eq!(
        machine.share_switch(1, window("30:40"), "Tarayıcı"),
        Ok(Vec::new())
    );
    // Another source is a new switch.
    let effects = machine.share_switch(1, window("50:60"), "Not").unwrap();
    assert_eq!(
        to_publisher(&effects),
        [&ShareCmd::Switch {
            n: 1,
            source: window("50:60")
        }]
    );
}

/// SQOL-R3 (P2-11): a window minimized when the switch lands reads as minimized (the publisher
/// tells it after the switch's news).
#[test]
fn a_minimized_window_switched_to_reads_as_minimized() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, window("10:20"), 2_000);
    machine
        .share_switch(1, window("30:40"), "Tarayıcı")
        .unwrap();
    // SQOL-R4: the switch's news says it itself (a restore told before it is in it already).
    let effects = machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1280,
            height: 720,
            minimized: true,
        },
        2_500,
    );
    assert_eq!(sent(&effects), [&ToRoom::share_st(1, ShareSt::Min)]);
    assert_eq!(machine.view(2_500, UNIX).share.mine[0].st, ShareSt::Min);
    // Restored afterwards: its own news.
    let effects = machine.share_event(ShareEvent::Minimized { n: 1, on: false }, 3_000);
    assert_eq!(sent(&effects), [&ToRoom::share_st(1, ShareSt::Live)]);
}

/// SQOL-R4: the launcher's own report of a switch's kind, refused for the budget, says so without
/// a share or a change (the player changed nothing to roll back).
#[test]
fn a_refused_report_of_a_switchs_kind_names_no_change() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    machine
        .share_switch(1, window("30:40"), "Tarayıcı")
        .unwrap();
    let effects = machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1280,
            height: 720,
            minimized: false,
        },
        2_500,
    );
    let rid = set_rid(&effects).expect("the new kind goes to the room");
    let effects = machine.socket_message(generation, error(rid, "budget"), 2_600, UNIX);
    assert_eq!(
        notices(&effects),
        [&ShareNotice::refused(ShareRefusal::Budget)]
    );
}

/// SQOL-R4: an older change the room refused no longer counts as waiting: asking for it again
/// goes to the room (it was swallowed).
#[test]
fn a_refused_older_change_can_be_asked_again() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    let fps = ShareChange {
        fps: Some(15),
        ..ShareChange::default()
    };
    let older = set_rid(&machine.share_update(1, fps, 2_100).unwrap()).unwrap();
    let before = machine.view(2_100, UNIX).share.mine[0].audio;
    set_rid(
        &machine
            .share_update(
                1,
                ShareChange {
                    audio: Some(!before),
                    ..ShareChange::default()
                },
                2_200,
            )
            .unwrap(),
    )
    .unwrap();
    machine.socket_message(generation, error(older, "budget"), 2_300, UNIX);
    // The newer one still waits; frame rate again: asked.
    assert!(set_rid(&machine.share_update(1, fps, 2_400).unwrap()).is_some());
}

/// SQOL-R3 (P3): sound the publisher found none of stays off when a later answer of the room
/// says it is on; the player's own "Sesi de Paylaş" tries again.
#[test]
fn sound_found_missing_stays_off_whatever_the_room_answers() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, ShareSource::Game, 2_000);
    let audio = |on| ShareChange {
        audio: Some(on),
        ..ShareChange::default()
    };
    if !machine.view(2_000, UNIX).share.mine[0].audio {
        let rid = set_rid(&machine.share_update(1, audio(true), 2_050).unwrap()).unwrap();
        machine.socket_message(
            generation,
            applied(rid, SharePreset::Normal, 30, true),
            2_060,
            UNIX,
        );
    }
    let effects = machine.share_event(ShareEvent::AudioUnavailable { n: 1 }, 2_100);
    assert_eq!(notices(&effects), [&ShareNotice::AudioOff]);
    // Another change's answer carries the room's own record of the sound.
    let rid = set_rid(
        &machine
            .share_update(
                1,
                ShareChange {
                    preset: Some(SharePreset::Low),
                    ..ShareChange::default()
                },
                2_200,
            )
            .unwrap(),
    )
    .unwrap();
    let effects = machine.socket_message(
        generation,
        applied(rid, SharePreset::Low, 30, true),
        2_300,
        UNIX,
    );
    assert!(!machine.view(2_300, UNIX).share.mine[0].audio);
    assert!(
        !to_publisher(&effects)
            .iter()
            .any(|cmd| matches!(cmd, ShareCmd::Update { opts, .. } if opts.audio))
    );
    // The player turns it on: asked, and the answer counts.
    let rid = set_rid(&machine.share_update(1, audio(true), 2_400).unwrap()).unwrap();
    machine.socket_message(
        generation,
        applied(rid, SharePreset::Low, 30, true),
        2_500,
        UNIX,
    );
    assert!(machine.view(2_500, UNIX).share.mine[0].audio);
}

/// SQOL-R4 (E2E): a window picked after the shown one closed is a source like the first: its own
/// close holds again, and the minute after it ends the share.
#[test]
fn a_window_picked_after_a_close_closes_like_the_first() {
    let mut machine = ready(true, true);
    let generation = live(&mut machine, Vec::new());
    sharing(&mut machine, generation, window("10:20"), 2_000);
    machine.share_event(ShareEvent::SourceGone { n: 1 }, 3_000);
    machine
        .share_switch(1, window("30:40"), "SQOL Test A")
        .unwrap();
    machine.share_event(
        ShareEvent::Switched {
            n: 1,
            width: 1280,
            height: 720,
            minimized: false,
        },
        3_500,
    );
    let view = machine.view(3_500, UNIX).share;
    assert_eq!((view.mine[0].hold, view.mine[0].st), (None, ShareSt::Live));
    let effects = machine.share_event(ShareEvent::SourceGone { n: 1 }, 10_000);
    assert_eq!(
        notices(&effects),
        [&ShareNotice::SourceClosed {
            n: 1,
            src: ShareKind::Window
        }]
    );
    let view = machine.view(10_000, UNIX).share;
    assert_eq!(view.mine[0].hold, Some(ShareHoldWhy::Closed));
    assert_eq!(view.mine[0].st, ShareSt::Hold);
    let effects = machine.tick(10_000 + HOLD_STOP_MS, UNIX, 0);
    assert_eq!(to_publisher(&effects), [&ShareCmd::Stop { n: 1 }]);
}
