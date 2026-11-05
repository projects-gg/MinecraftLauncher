//! The state machine's scenarios (design §3, §5, §6; the S1 contract's room socket).

use std::time::Duration;

use pl_social::BackoffPolicy;

use crate::engine::{EngineCommand, EngineSound, PageReport};
use crate::input::KeyChange;
use crate::lifecycle;
use crate::machine::{
    CrewRoom, Effect, GIVE_UP_MS, IncomingRing, JOIN_RID, Machine, RATE_LIMITED_WAIT_MS, RingEnd,
    ServiceVoice, StayStart, SystemNotice,
};
use crate::settings::{Store, VoiceKey, VoiceSettings, VoiceTalkMode};
use crate::view::{
    VoiceAnswer, VoiceLeftReason, VoiceMic, VoiceModAction, VoiceNotice, VoicePhase, VoiceRefusal,
    VoiceRole, VoiceShortcut,
};
use crate::wire::{FromRoom, MeWire, PersonWire, Role, RoomKind, RoomWire, Sdp, Ticket, ToRoom};

const ROOM: &str = "f0123456789abcdef";
const CREW: &str = "0123456789abcdef";
const GENERAL: &str = "g0123456789abcdef";
const UNIX: i64 = 1_800_000_000;

fn on() -> ServiceVoice {
    ServiceVoice {
        on: true,
        ..ServiceVoice::default()
    }
}

/// Signed config on, the hub took `voice1` with Studio's switch on, the player signed in.
fn ready() -> Machine {
    let mut machine = Machine::new(Store::in_memory());
    machine.set_backoff(BackoffPolicy {
        initial: Duration::from_millis(1_000),
        max: Duration::from_millis(1_000),
        spread: Duration::ZERO,
    });
    machine.set_flag(true, 0);
    machine.set_me("me", 0);
    machine.hub_hello(true, Some(on()), Vec::new());
    machine
}

fn person(u: &str, j: i64) -> PersonWire {
    PersonWire {
        u: u.into(),
        j,
        mute: false,
        deaf: false,
        sm: false,
        q: 0,
        card: None,
        sh: Vec::new(),
    }
}

/// Someone with VOICE-S2's card (a friend's friend).
fn carded(u: &str, j: i64, name: &str) -> PersonWire {
    let card = serde_json::from_value(serde_json::json!({
        "id": u,
        "username": name.to_lowercase(),
        "displayName": name,
        "avatar": "preset:wolf",
        "avatarHash": null,
        "marks": ["verified"],
    }))
    .unwrap();
    PersonWire {
        card: Some(Box::new(card)),
        ..person(u, j)
    }
}

fn talk(open: bool, tail: bool) -> EngineCommand {
    EngineCommand::Talk { open, tail }
}

fn asks_join(effects: &[Effect]) -> bool {
    engine(effects)
        .iter()
        .any(|c| matches!(c, EngineCommand::Join))
}

fn hello(people: Vec<PersonWire>, role: Role, sdp: bool) -> FromRoom {
    FromRoom::Hello {
        room: RoomWire {
            id: ROOM.into(),
            kind: RoomKind::Friend,
            crew: None,
            name: None,
            leader: Some("me".into()),
            mods: Vec::new(),
            max: 10,
            share_by: None,
        },
        me: MeWire { role, watch: None },
        people,
        rings: Vec::new(),
        sdp: sdp.then(|| Sdp::new("v=0 answer")),
        resume: Some(Ticket::new("v1.resume.mac")),
        now: Some(UNIX),
    }
}

fn connect_generation(effects: &[Effect]) -> u64 {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Connect { generation, .. } => Some(*generation),
            _ => None,
        })
        .expect("a connect")
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

fn engine(effects: &[Effect]) -> Vec<&EngineCommand> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Engine(command) => Some(command),
            _ => None,
        })
        .collect()
}

fn notices(effects: &[Effect]) -> Vec<&VoiceNotice> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Notice(notice) => Some(notice),
            _ => None,
        })
        .collect()
}

fn answer(effects: &[Effect], rid: u32) -> Option<VoiceAnswer> {
    effects.iter().find_map(|effect| match effect {
        Effect::Answer { rid: r, answer } if *r == rid => Some(*answer),
        _ => None,
    })
}

/// A friend call that is live: joined, the page attached, its offer sent, `hello` in.
fn live_call(machine: &mut Machine, people: Vec<PersonWire>) -> u64 {
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
    let generation = connect_generation(&effects);
    machine.page_attached();
    machine.socket_open(generation, 1_100);
    machine.page_report(
        PageReport::Offer {
            sdp: Some("v=0 offer".into()),
            mid: Some("0".into()),
        },
        1_200,
    );
    machine.socket_message(generation, hello(people, Role::Leader, true), 1_300, UNIX);
    machine.page_report(PageReport::Connected, 1_400);
    generation
}

#[test]
fn nothing_without_the_switches() {
    let mut machine = Machine::new(Store::in_memory());
    assert_eq!(
        machine.may_call(),
        Err(VoiceAnswer::refused(VoiceRefusal::Off))
    );
    assert!(!machine.view(0, UNIX).available);
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("t"),
            calling: None,
            opened_by_me: false,
        },
        0,
        UNIX,
    );
    assert_eq!(
        answer(&effects, JOIN_RID),
        Some(VoiceAnswer::refused(VoiceRefusal::Off))
    );
    assert!(!machine.in_room());
    // And nothing wants a timer.
    assert_eq!(machine.next_deadline(0), None);
    let machine = ready();
    assert!(machine.may_call().is_ok());
    assert!(machine.view(0, UNIX).available);
    assert_eq!(machine.next_deadline(0), None);
}

#[test]
fn a_call_joins_and_goes_live() {
    let mut machine = ready();
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("v1.join.mac"),
            calling: Some("friend".into()),
            opened_by_me: false,
        },
        1_000,
        UNIX,
    );
    let generation = connect_generation(&effects);
    assert!(effects.contains(&Effect::Window(lifecycle::Action::Open)));
    // "Konuşunca" registers no input.
    assert!(!effects.iter().any(|e| matches!(e, Effect::Input(_))));
    let view = machine.view(1_000, UNIX);
    assert_eq!(view.room.as_ref().unwrap().phase, VoicePhase::Joining);
    assert_eq!(view.calling.as_ref().unwrap().friend, "friend");
    // The page attaches: configured and asked for an offer.
    let effects = machine.page_attached();
    let commands = engine(&effects);
    assert!(matches!(commands[0], EngineCommand::Configure { .. }));
    assert!(commands.iter().any(|c| matches!(c, EngineCommand::Join)));
    // The socket opens before the offer: the join's caller hears it worked.
    let effects = machine.socket_open(generation, 1_100);
    assert_eq!(answer(&effects, JOIN_RID), Some(VoiceAnswer::OK));
    assert!(sent(&effects).is_empty());
    // The offer goes out as `join`.
    let effects = machine.page_report(
        PageReport::Offer {
            sdp: Some("v=0 offer".into()),
            mid: Some("0".into()),
        },
        1_200,
    );
    assert!(matches!(
        sent(&effects)[..],
        [ToRoom::Join {
            sdp: Some(_),
            mid: Some(_),
            ..
        }]
    ));
    // `hello` brings the SFU's answer; the state goes out; the caller waits alone.
    let effects = machine.socket_message(
        generation,
        hello(vec![person("me", 1)], Role::Leader, true),
        1_300,
        UNIX,
    );
    let commands = engine(&effects);
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, EngineCommand::Answer { .. }))
    );
    assert!(commands.iter().any(|c| matches!(
        c,
        EngineCommand::Sound {
            sound: EngineSound::Waiting
        }
    )));
    assert!(matches!(
        sent(&effects)[..],
        [ToRoom::Set {
            mute: false,
            deaf: false
        }]
    ));
    let view = machine.view(1_300, UNIX);
    let room = view.room.unwrap();
    assert_eq!(room.phase, VoicePhase::Live);
    assert_eq!(room.me.role, VoiceRole::Leader);
    // The friend comes: the call is answered.
    let effects = machine.socket_message(
        generation,
        FromRoom::Joined {
            person: person("friend", 5),
        },
        2_000,
        UNIX,
    );
    let commands = engine(&effects);
    assert!(commands.iter().any(|c| matches!(
        c,
        EngineCommand::Sound {
            sound: EngineSound::Stop
        }
    )));
    assert!(
        commands
            .iter()
            .any(|c| matches!(c, EngineCommand::People { people } if people.len() == 1))
    );
    let view = machine.view(2_000, UNIX);
    assert!(view.calling.is_none());
    assert_eq!(view.room.unwrap().people[0].id, "friend");
}

#[test]
fn a_ring_rings_then_is_missed() {
    let mut machine = ready();
    let ring = IncomingRing {
        room: ROOM.into(),
        from: "kaan".into(),
        at: UNIX,
        until: UNIX + 30,
        ticket: Ticket::new("v1.invite.mac"),
        with: vec!["ece".into()],
    };
    let effects = machine.ring(ring.clone(), false, 0, UNIX);
    assert!(effects.contains(&Effect::Ring(true)));
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::System(SystemNotice::Ring { .. })))
    );
    assert_eq!(machine.view(0, UNIX).rings.len(), 1);
    assert_eq!(machine.next_deadline(0), Some(30_000));
    // It runs out: a missed call that can still be joined.
    let effects = machine.tick(30_000, UNIX + 30, 0);
    assert!(effects.contains(&Effect::Ring(false)));
    assert!(
        notices(&effects)
            .iter()
            .any(|n| matches!(n, VoiceNotice::Missed { .. }))
    );
    let view = machine.view(30_000, UNIX + 30);
    assert!(view.rings.is_empty());
    assert!(view.missed[0].joinable);
    assert_eq!(machine.next_deadline(30_000), None);
    assert!(machine.ticket_for(ROOM, UNIX + 31).is_some());
    // The room ends: "Geri ara" instead of "Katıl".
    machine.ring_end(ROOM, RingEnd::Ended);
    assert!(!machine.view(31_000, UNIX + 31).missed[0].joinable);
    assert!(machine.ticket_for(ROOM, UNIX + 31).is_none());
}

#[test]
fn do_not_disturb_only_leaves_a_missed_call() {
    let mut machine = ready();
    let effects = machine.ring(
        IncomingRing {
            room: ROOM.into(),
            from: "kaan".into(),
            at: UNIX,
            until: UNIX + 30,
            ticket: Ticket::new("t"),
            with: Vec::new(),
        },
        true,
        0,
        UNIX,
    );
    assert!(!effects.contains(&Effect::Ring(true)));
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::System(SystemNotice::Ring { .. })))
    );
    assert!(machine.view(0, UNIX).rings.is_empty());
    assert_eq!(machine.view(0, UNIX).missed.len(), 1);
}

fn ring_from(from: &str, room: &str) -> IncomingRing {
    IncomingRing {
        room: room.into(),
        from: from.into(),
        at: UNIX,
        until: UNIX + 30,
        ticket: Ticket::new("t"),
        with: Vec::new(),
    }
}

/// Anything the player would see or hear of a ring: a card, a tone, a notification, a notice.
fn told(effects: &[Effect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            Effect::Ring(true)
                | Effect::Notice(_)
                | Effect::System(SystemNotice::Ring { .. } | SystemNotice::Missed { .. })
        )
    })
}

#[test]
fn an_ignored_persons_ring_waits_quietly_in_the_missed_list() {
    let mut machine = ready();
    let long = "k".repeat(65);
    assert!(
        machine
            .set_ignored(vec!["kaan".into(), "kaan".into(), String::new(), long])
            .is_empty()
    );
    let effects = machine.ring(ring_from("kaan", ROOM), false, 0, UNIX);
    assert!(!told(&effects));
    let view = machine.view(0, UNIX);
    assert!(view.rings.is_empty());
    assert_eq!(view.missed.len(), 1);
    assert!(view.missed[0].joinable);
    // Nothing is due for it later either.
    assert_eq!(machine.next_deadline(0), None);
    // Someone else still rings.
    let effects = machine.ring(ring_from("ece", "f00000000000000aa"), false, 0, UNIX);
    assert!(effects.contains(&Effect::Ring(true)));
    assert_eq!(machine.view(0, UNIX).rings.len(), 1);
}

#[test]
fn ignoring_someone_whose_ring_rings_quiets_it_at_once() {
    let mut machine = ready();
    machine.ring(ring_from("kaan", ROOM), false, 0, UNIX);
    let effects = machine.set_ignored(vec!["kaan".into()]);
    assert!(effects.contains(&Effect::Ring(false)));
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::System(SystemNotice::RingGone { room }) if room == ROOM))
    );
    assert!(!told(&effects));
    let view = machine.view(0, UNIX);
    assert!(view.rings.is_empty());
    assert_eq!(view.missed.len(), 1);
    // The same list again changes nothing; an empty one (SOC-NEXT's switch off) rings again.
    assert!(machine.set_ignored(vec!["kaan".into()]).is_empty());
    machine.set_ignored(Vec::new());
    let effects = machine.ring(ring_from("kaan", "f00000000000000aa"), false, 0, UNIX);
    assert!(effects.contains(&Effect::Ring(true)));
}

#[test]
fn declining_tells_the_hub_and_stops_the_ring() {
    let mut machine = ready();
    machine.ring(
        IncomingRing {
            room: ROOM.into(),
            from: "kaan".into(),
            at: UNIX,
            until: UNIX + 30,
            ticket: Ticket::new("t"),
            with: Vec::new(),
        },
        false,
        0,
        UNIX,
    );
    let effects = machine.decline(ROOM);
    assert!(effects.contains(&Effect::HubDecline { room: ROOM.into() }));
    assert!(effects.contains(&Effect::Ring(false)));
    assert!(machine.view(0, UNIX).missed.is_empty());
    // Joined elsewhere: no missed call either.
    machine.ring(
        IncomingRing {
            room: ROOM.into(),
            from: "kaan".into(),
            at: UNIX,
            until: UNIX + 30,
            ticket: Ticket::new("t"),
            with: Vec::new(),
        },
        false,
        0,
        UNIX,
    );
    let effects = machine.ring_end(ROOM, RingEnd::Joined);
    assert!(effects.contains(&Effect::Ring(false)));
    assert!(machine.view(0, UNIX).rings.is_empty() && machine.view(0, UNIX).missed.is_empty());
}

#[test]
fn a_dropped_socket_comes_back_with_sync() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("friend", 2)]);
    let effects = machine.socket_closed(generation, None, 10_000);
    assert!(effects.contains(&Effect::Disconnect { generation }));
    // The machine wants to come back within a second (jittered).
    let back = machine.next_deadline(10_000).unwrap();
    assert!((10_500..=11_000).contains(&back), "{back}");
    // Not "Yeniden bağlanıyor…" yet.
    assert_eq!(
        machine.view(10_400, UNIX).room.unwrap().phase,
        VoicePhase::Live
    );
    let effects = machine.tick(back, UNIX, back);
    let again = connect_generation(&effects);
    assert_ne!(again, generation);
    // With the resume ticket.
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::Connect { ticket, .. } if ticket.expose() == "v1.resume.mac"
    )));
    // The audio still flows: `sync`, no new offer.
    let effects = machine.socket_open(again, 11_500);
    assert!(matches!(sent(&effects)[..], [ToRoom::Sync { .. }]));
    // Old sockets' words change nothing.
    assert!(machine.socket_closed(generation, None, 11_600).is_empty());
    let effects = machine.tick(12_000, UNIX, 12_000);
    assert!(effects.contains(&Effect::Changed));
    assert_eq!(
        machine.view(12_000, UNIX).room.unwrap().phase,
        VoicePhase::Reconnecting
    );
    let effects = machine.socket_message(
        again,
        hello(
            vec![person("me", 1), person("friend", 2)],
            Role::Leader,
            false,
        ),
        12_100,
        UNIX,
    );
    assert!(matches!(sent(&effects)[..], [ToRoom::Set { .. }]));
    assert_eq!(
        machine.view(12_100, UNIX).room.unwrap().phase,
        VoicePhase::Live
    );
}

#[test]
fn a_connection_that_does_not_come_back_gives_up_and_may_retry() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("friend", 2)]);
    machine.socket_closed(generation, None, 10_000);
    let mut now = 10_000;
    let mut current = generation;
    // Every try fails.
    loop {
        let Some(at) = machine.next_deadline(now) else {
            panic!("no deadline while away");
        };
        assert!(at >= now, "a past deadline would spin");
        now = at;
        let effects = machine.tick(now, UNIX, now);
        if let Some(Effect::Connect { generation, .. }) =
            effects.iter().find(|e| matches!(e, Effect::Connect { .. }))
        {
            current = *generation;
            machine.socket_refused(current, None, None, now + 100);
        }
        if !machine.in_room() {
            assert!(notices(&effects).contains(&&VoiceNotice::Left {
                reason: VoiceLeftReason::Failed,
                friend: None,
                until: None
            }));
            break;
        }
        assert!(now <= 10_000 + GIVE_UP_MS);
    }
    assert!(machine.retry_ticket().is_some());
    let _ = current;
}

#[test]
fn close_codes() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1)]);
    let effects = machine.socket_closed(generation, Some(4001), 5_000);
    assert!(!machine.in_room());
    assert!(notices(&effects).contains(&&VoiceNotice::Left {
        reason: VoiceLeftReason::Unavailable,
        friend: None,
        until: None
    }));

    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1)]);
    machine.socket_closed(generation, Some(4008), 5_000);
    assert!(machine.in_room());
    // "Yeniden bağlanıyor…" after 2 s, but no new socket for a minute.
    let shown = machine.next_deadline(5_000).unwrap();
    assert_eq!(shown, 7_000);
    let effects = machine.tick(shown, UNIX, shown);
    assert!(!effects.iter().any(|e| matches!(e, Effect::Connect { .. })));
    assert!(machine.next_deadline(shown).unwrap() >= 5_000 + RATE_LIMITED_WAIT_MS);

    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1)]);
    machine.socket_closed(generation, Some(4003), 5_000);
    assert!(!machine.in_room());
}

#[test]
fn the_rooms_endings_tell_the_player() {
    let mut machine = ready();
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("t"),
            calling: Some("friend".into()),
            opened_by_me: false,
        },
        0,
        UNIX,
    );
    let generation = connect_generation(&effects);
    machine.socket_open(generation, 10);
    machine.socket_message(
        generation,
        hello(vec![person("me", 1)], Role::Leader, false),
        20,
        UNIX,
    );
    let effects = machine.socket_message(
        generation,
        FromRoom::Closed {
            why: crate::wire::ClosedWhy::NoAnswer,
            until: None,
        },
        30_000,
        UNIX,
    );
    assert!(notices(&effects).contains(&&VoiceNotice::Left {
        reason: VoiceLeftReason::NoAnswer,
        friend: Some("friend".into()),
        until: None
    }));
    assert!(effects.contains(&Effect::Window(lifecycle::Action::None)) || !machine.in_room());

    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    let effects = machine.socket_message(generation, FromRoom::Kicked, 5_000, UNIX);
    assert!(notices(&effects).contains(&&VoiceNotice::Left {
        reason: VoiceLeftReason::Kicked,
        friend: None,
        until: None
    }));
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::System(SystemNotice::Left {
            reason: VoiceLeftReason::Kicked
        })
    )));
    // Leaving lets the window linger a minute, then it closes.
    assert_eq!(
        machine.next_deadline(5_000),
        Some(5_000 + lifecycle::LINGER_MS)
    );
    let effects = machine.tick(5_000 + lifecycle::LINGER_MS, UNIX, 0);
    assert!(effects.contains(&Effect::Window(lifecycle::Action::Close)));
    assert_eq!(machine.next_deadline(5_000 + lifecycle::LINGER_MS), None);
}

#[test]
fn a_voice_chat_ban_takes_the_player_out_until_its_end() {
    // BAN-TYPES: `closed` `muted` ends the stay at once and says until when.
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    let effects = machine.socket_message(
        generation,
        FromRoom::Closed {
            why: crate::wire::ClosedWhy::Muted,
            until: Some(UNIX + 3_600),
        },
        5_000,
        UNIX,
    );
    assert!(!machine.in_room());
    assert!(notices(&effects).contains(&&VoiceNotice::Left {
        reason: VoiceLeftReason::Restricted,
        friend: None,
        until: Some(UNIX + 3_600)
    }));
    assert!(effects.contains(&Effect::System(SystemNotice::Left {
        reason: VoiceLeftReason::Restricted
    })));

    // Coming back meets the ban (`voice_muted`, no end in the refusal): no "Tekrar dene".
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    machine.socket_closed(generation, None, 5_000);
    let mut now = 5_000;
    let again = loop {
        now = machine.next_deadline(now).expect("a deadline while away");
        assert!(now <= 5_000 + GIVE_UP_MS, "no reconnect");
        let effects = machine.tick(now, UNIX, now);
        if let Some(generation) = effects.iter().find_map(|effect| match effect {
            Effect::Connect { generation, .. } => Some(*generation),
            _ => None,
        }) {
            break generation;
        }
    };
    let effects = machine.socket_refused(again, Some(403), Some("voice_muted"), now + 100);
    assert!(!machine.in_room());
    assert!(notices(&effects).contains(&&VoiceNotice::Left {
        reason: VoiceLeftReason::Restricted,
        friend: None,
        until: None
    }));
    assert!(machine.retry_ticket().is_none());
}

#[test]
fn a_move_joins_the_other_room() {
    let mut machine = ready();
    machine.hub_hello(
        true,
        Some(on()),
        vec![(
            CREW.into(),
            vec![
                CrewRoom {
                    id: GENERAL.into(),
                    name: "Genel".into(),
                    ids: vec!["me".into()],
                    ..CrewRoom::default()
                },
                CrewRoom {
                    id: "c00000000000000aa".into(),
                    name: "Oda 2".into(),
                    ids: Vec::new(),
                    ..CrewRoom::default()
                },
            ],
        )],
    );
    let generation = live_call(&mut machine, vec![person("me", 1)]);
    let effects = machine.socket_message(
        generation,
        FromRoom::Moved {
            room: "c00000000000000aa".into(),
            ticket: Ticket::new("v1.move.mac"),
        },
        5_000,
        UNIX,
    );
    let moved = connect_generation(&effects);
    assert_ne!(moved, generation);
    assert!(effects.contains(&Effect::Disconnect { generation }));
    assert!(notices(&effects).contains(&&VoiceNotice::Moved {
        room: "c00000000000000aa".into(),
        name: "Oda 2".into()
    }));
    // Nobody waits for that join's answer.
    assert_eq!(answer(&effects, JOIN_RID), None);
    assert_eq!(machine.room_id(), Some("c00000000000000aa"));
}

#[test]
fn mute_and_deafen() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("friend", 2)]);
    let effects = machine.set_mute(true);
    assert!(effects.contains(&Effect::Send {
        generation,
        frame: ToRoom::Set {
            mute: true,
            deaf: false
        }
    }));
    // A mute cuts at once (no push-to-talk tail).
    assert!(engine(&effects).contains(&&talk(false, false)));
    let effects = machine.set_deaf(true);
    assert!(engine(&effects).contains(&&EngineCommand::Deaf { on: true }));
    // Opening the microphone ends deafness too.
    let effects = machine.set_mute(false);
    assert!(engine(&effects).contains(&&EngineCommand::Deaf { on: false }));
    assert!(engine(&effects).contains(&&talk(true, false)));
    let view = machine.view(0, UNIX).room.unwrap();
    assert!(!view.me.mute && !view.me.deaf);
    // Muted for everyone by a moderator: closed whatever the player does.
    let effects = machine.socket_message(
        generation,
        FromRoom::State {
            u: "me".into(),
            mute: None,
            deaf: None,
            sm: Some(true),
            q: None,
            sh: None,
        },
        10,
        UNIX,
    );
    assert!(notices(&effects).contains(&&VoiceNotice::MutedByModerator));
    assert!(engine(&effects).contains(&&talk(false, false)));
}

#[test]
fn the_shortcuts_deafen_and_undeafen_as_discord_does() {
    let mut machine = ready();
    live_call(&mut machine, vec![person("me", 1), person("friend", 2)]);
    let press = |machine: &mut Machine, shortcut| {
        machine.key(KeyChange {
            shortcut,
            down: true,
        })
    };
    let me = |machine: &Machine| machine.view(0, UNIX).room.unwrap().me;
    // Deafened, the gate closes with its sound; the microphone choice itself is kept.
    let effects = press(&mut machine, VoiceShortcut::Deafen);
    assert!(engine(&effects).contains(&&talk(false, false)));
    assert!(engine(&effects).contains(&&EngineCommand::Sound {
        sound: EngineSound::DeafOn
    }));
    assert!(me(&machine).deaf && !me(&machine).mute);
    // Undeafened, the microphone is back as it was before (open).
    let effects = press(&mut machine, VoiceShortcut::Deafen);
    assert!(engine(&effects).contains(&&talk(true, false)));
    // Muted first, deafened, undeafened: still muted.
    press(&mut machine, VoiceShortcut::Mute);
    press(&mut machine, VoiceShortcut::Deafen);
    let effects = press(&mut machine, VoiceShortcut::Deafen);
    assert!(engine(&effects).contains(&&talk(false, false)));
    assert!(me(&machine).mute && !me(&machine).deaf);
    // While deafened, the mute key opens the microphone and ends deafness at once (it shows as
    // off, so one press must turn it on).
    press(&mut machine, VoiceShortcut::Mute);
    press(&mut machine, VoiceShortcut::Deafen);
    assert!(me(&machine).deaf && !me(&machine).mute);
    let effects = press(&mut machine, VoiceShortcut::Mute);
    assert!(engine(&effects).contains(&&EngineCommand::Deaf { on: false }));
    assert!(engine(&effects).contains(&&talk(true, false)));
    assert!(!me(&machine).deaf && !me(&machine).mute);
}

#[test]
fn input_is_registered_only_in_a_room_with_a_shortcut() {
    let mut machine = ready();
    let ptt = VoiceKey::keyboard("KeyB", "B");
    let (_, effects) = machine.save_settings(
        VoiceSettings {
            mode: VoiceTalkMode::PushToTalk,
            push_to_talk_key: Some(ptt.clone()),
            ..VoiceSettings::default()
        },
        0,
    );
    // No room: nothing registered.
    assert!(!effects.iter().any(|e| matches!(e, Effect::Input(_))));
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("t"),
            calling: None,
            opened_by_me: false,
        },
        0,
        UNIX,
    );
    assert!(effects.contains(&Effect::Input(Some(vec![(VoiceShortcut::PushToTalk, ptt)]))));
    let generation = connect_generation(&effects);
    machine.page_attached();
    machine.socket_open(generation, 1);
    machine.socket_message(
        generation,
        hello(
            vec![person("me", 1), person("friend", 2)],
            Role::Member,
            false,
        ),
        2,
        UNIX,
    );
    // Push-to-talk opens and closes the gate; letting go keeps the end of the word.
    let effects = machine.key(KeyChange {
        shortcut: VoiceShortcut::PushToTalk,
        down: true,
    });
    assert!(engine(&effects).contains(&&talk(true, false)));
    let effects = machine.key(KeyChange {
        shortcut: VoiceShortcut::PushToTalk,
        down: false,
    });
    assert!(engine(&effects).contains(&&talk(false, true)));
    // A mute while the key is held cuts at once (VOICE-INT: L2 asked for `tail`).
    machine.key(KeyChange {
        shortcut: VoiceShortcut::PushToTalk,
        down: true,
    });
    let effects = machine.set_mute(true);
    assert!(engine(&effects).contains(&&talk(false, false)));
    machine.set_mute(false);
    // Back to "Konuşunca" mid-call: the watch stops.
    let (_, effects) = machine.save_settings(VoiceSettings::default(), 3);
    assert!(effects.contains(&Effect::Input(None)));
    // A mute shortcut mid-call: watched again.
    let mute = VoiceKey::mouse(crate::settings::VoiceMouseButton::Back);
    let (_, effects) = machine.save_settings(
        VoiceSettings {
            mute_key: Some(mute.clone()),
            ..VoiceSettings::default()
        },
        4,
    );
    assert!(effects.contains(&Effect::Input(Some(vec![(VoiceShortcut::Mute, mute)]))));
    let effects = machine.key(KeyChange {
        shortcut: VoiceShortcut::Mute,
        down: true,
    });
    assert!(engine(&effects).iter().any(|c| matches!(
        c,
        EngineCommand::Sound {
            sound: EngineSound::MicOff
        }
    )));
    // Leaving stops it.
    let effects = machine.leave(5);
    assert!(effects.contains(&Effect::Input(None)));
    assert!(effects.contains(&Effect::Send {
        generation,
        frame: ToRoom::Leave
    }));
}

#[test]
fn the_inactivity_rule_ends_a_quiet_stay() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    let thirty = 30 * 60_000;
    // Touched at 1 400: the deadline is half an hour after that.
    assert_eq!(machine.next_deadline(1_400), Some(1_400 + thirty));
    // Speech moves it.
    machine.page_report(
        PageReport::Speaking {
            ids: Vec::new(),
            me: true,
        },
        600_000,
    );
    assert_eq!(machine.next_deadline(1_400), Some(600_000 + thirty));
    // A touch since: not yet.
    let effects = machine.tick(600_000 + thirty, UNIX, 700_000);
    assert!(machine.in_room());
    assert!(notices(&effects).is_empty());
    let effects = machine.tick(700_000 + thirty, UNIX, 700_000);
    assert!(!machine.in_room());
    assert!(notices(&effects).contains(&&VoiceNotice::Left {
        reason: VoiceLeftReason::Idle,
        friend: None,
        until: None
    }));
    assert!(effects.contains(&Effect::Send {
        generation,
        frame: ToRoom::Leave
    }));
}

#[test]
fn powers_follow_the_roles() {
    let mut machine = ready();
    let generation = live_call(
        &mut machine,
        vec![person("me", 1), person("mod", 2), person("member", 3)],
    );
    machine.socket_message(
        generation,
        FromRoom::Room {
            name: None,
            leader: Some("me".into()),
            mods: vec!["mod".into()],
            max: None,
            share_by: None,
        },
        5,
        UNIX,
    );
    let room = machine.view(5, UNIX).room.unwrap();
    assert!(room.can_rename && room.can_close);
    let by_id = |id: &str| room.people.iter().find(|p| p.id == id).unwrap().clone();
    assert!(by_id("mod").can.kick && by_id("mod").can.moderator);
    assert_eq!(by_id("mod").role, VoiceRole::Moderator);
    // A friend room has nowhere to move people.
    assert!(!by_id("member").can.move_to);
    // As a moderator: members only, no rename in a friend room, no close.
    machine.socket_message(
        generation,
        FromRoom::Me {
            role: Role::Moderator,
        },
        6,
        UNIX,
    );
    machine.socket_message(
        generation,
        FromRoom::Room {
            name: None,
            leader: Some("mod".into()),
            mods: vec!["me".into()],
            max: None,
            share_by: None,
        },
        6,
        UNIX,
    );
    let room = machine.view(6, UNIX).room.unwrap();
    assert!(!room.can_rename && !room.can_close);
    let by_id = |id: &str| room.people.iter().find(|p| p.id == id).unwrap().clone();
    assert!(!by_id("mod").can.kick);
    assert!(by_id("member").can.kick && by_id("member").can.mute);
    assert!(!by_id("member").can.moderator);
}

#[test]
fn requests_are_answered_by_rid() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1)]);
    let (rid, effects) = machine.invite("friend").unwrap();
    assert_eq!(
        sent(&effects),
        vec![&ToRoom::Invite {
            to: "friend".into(),
            rid
        }]
    );
    let effects = machine.socket_message(
        generation,
        FromRoom::Ack {
            rid: Some(rid),
            ice: None,
            until: Some(UNIX + 30),
            sdp: None,
            n: None,
            preset: None,
        },
        10,
        UNIX,
    );
    assert_eq!(answer(&effects, rid), Some(VoiceAnswer::OK));
    let (rid, _) = machine
        .moderate(&VoiceModAction::Kick { person: "x".into() })
        .unwrap();
    let effects = machine.socket_message(
        generation,
        FromRoom::Error {
            rid: Some(rid),
            code: "not_allowed".into(),
            retry_after: None,
            until: None,
            item: None,
        },
        11,
        UNIX,
    );
    assert_eq!(
        answer(&effects, rid).and_then(|a| a.refused),
        Some(VoiceRefusal::NotAllowed)
    );
    // A name out of range never goes out.
    assert_eq!(
        machine
            .moderate(&VoiceModAction::Rename { name: "ab".into() })
            .unwrap_err()
            .refused,
        Some(VoiceRefusal::InvalidName)
    );
    // Not in a room: nothing to do it on.
    machine.leave(12);
    assert_eq!(
        machine.invite("friend").unwrap_err().refused,
        Some(VoiceRefusal::Nothing)
    );
}

#[test]
fn turn_credentials_reach_the_page() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1)]);
    let effects = machine.page_report(PageReport::NeedTurn, 10);
    let rid = match sent(&effects)[..] {
        [ToRoom::Turn { rid }] => *rid,
        _ => panic!("no turn request"),
    };
    let ice: crate::wire::IceWire =
        serde_json::from_str(r#"{"urls":["turn:x"],"username":"u","credential":"c","ttl":7200}"#)
            .unwrap();
    let effects = machine.socket_message(
        generation,
        FromRoom::Ack {
            rid: Some(rid),
            ice: Some(ice),
            until: None,
            sdp: None,
            n: None,
            preset: None,
        },
        11,
        UNIX,
    );
    assert!(
        engine(&effects)
            .iter()
            .any(|c| matches!(c, EngineCommand::Ice { .. }))
    );
}

#[test]
fn a_crashed_page_comes_back_a_few_times() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    for at in [10_000, 11_000, 12_000] {
        let effects = machine.page_crashed(at);
        assert!(effects.contains(&Effect::Window(lifecycle::Action::Reopen)));
        // The new page joins anew on the same socket.
        let effects = machine.page_attached();
        assert!(
            engine(&effects)
                .iter()
                .any(|c| matches!(c, EngineCommand::Join))
        );
        let effects = machine.page_report(
            PageReport::Offer {
                sdp: Some("v=0 new".into()),
                mid: Some("0".into()),
            },
            at + 100,
        );
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::Send {
                generation: g,
                frame: ToRoom::Join { .. }
            } if *g == generation
        )));
    }
    let effects = machine.page_crashed(13_000);
    assert!(!machine.in_room());
    assert!(notices(&effects).contains(&&VoiceNotice::Left {
        reason: VoiceLeftReason::Failed,
        friend: None,
        until: None
    }));
}

#[test]
fn speaking_goes_out_only_on_change() {
    let mut machine = ready();
    live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    let effects = machine.page_report(
        PageReport::Speaking {
            ids: vec!["x".into(), "stranger".into()],
            me: false,
        },
        10,
    );
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::Speaking(s) if s.ids == vec!["x".to_owned()] && !s.me
    )));
    let again = machine.page_report(
        PageReport::Speaking {
            ids: vec!["x".into()],
            me: false,
        },
        20,
    );
    assert!(!again.iter().any(|e| matches!(e, Effect::Speaking(_))));
    assert!(machine.view(20, UNIX).room.unwrap().people[0].speaking);
}

#[test]
fn switching_voice_chat_off_ends_everything() {
    let mut machine = ready();
    machine.ring(
        IncomingRing {
            room: "f00000000000000ff".into(),
            from: "kaan".into(),
            at: UNIX,
            until: UNIX + 30,
            ticket: Ticket::new("t"),
            with: Vec::new(),
        },
        false,
        0,
        UNIX,
    );
    let generation = live_call(&mut machine, vec![person("me", 1)]);
    let (_, effects) = machine.save_settings(
        VoiceSettings {
            enabled: false,
            ..VoiceSettings::default()
        },
        100,
    );
    assert!(effects.contains(&Effect::HubSwitch { on: false }));
    assert!(effects.contains(&Effect::Send {
        generation,
        frame: ToRoom::Leave
    }));
    assert!(effects.contains(&Effect::Ring(false)));
    assert!(!machine.in_room());
    assert!(machine.view(100, UNIX).rings.is_empty());
    // Rings are ignored while it is off.
    let effects = machine.ring(
        IncomingRing {
            room: ROOM.into(),
            from: "kaan".into(),
            at: UNIX,
            until: UNIX + 30,
            ticket: Ticket::new("t"),
            with: Vec::new(),
        },
        false,
        200,
        UNIX,
    );
    assert!(effects.is_empty());
    // The hub hears the switch again after a reconnect.
    let effects = machine.hub_hello(true, Some(on()), Vec::new());
    assert!(effects.contains(&Effect::HubSwitch { on: false }));
}

#[test]
fn blocking_the_only_other_person_ends_the_call() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    let effects = machine.blocked("x", 100);
    assert!(!machine.in_room());
    assert!(effects.contains(&Effect::Send {
        generation,
        frame: ToRoom::Leave
    }));
    // In a group the room hides them from each other instead.
    let mut machine = ready();
    live_call(
        &mut machine,
        vec![person("me", 1), person("x", 2), person("y", 3)],
    );
    machine.blocked("x", 100);
    assert!(machine.in_room());
}

#[test]
fn the_settings_page_runs_the_engine_only_while_it_may() {
    let mut machine = Machine::new(Store::in_memory());
    assert!(machine.set_setup(true, 0).is_empty());
    let mut machine = ready();
    let effects = machine.set_setup(true, 0);
    assert!(effects.contains(&Effect::Window(lifecycle::Action::Open)));
    let effects = machine.page_attached();
    assert!(engine(&effects).contains(&&EngineCommand::Setup { on: true }));
    let effects = machine.page_report(
        PageReport::Mic {
            mic: VoiceMic::Denied,
            changed: false,
        },
        1,
    );
    assert!(effects.contains(&Effect::Changed));
    assert_eq!(machine.view(1, UNIX).mic, VoiceMic::Denied);
    let effects = machine.page_report(
        PageReport::Level {
            level: 2.0,
            open: true,
        },
        2,
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Level(l) if l.level == 1.0))
    );
    machine.set_test(true, 3);
    assert!(machine.view(3, UNIX).testing);
    let effects = machine.set_setup(false, 4);
    assert!(engine(&effects).contains(&&EngineCommand::SelfTest { on: false }));
    assert!(!machine.view(4, UNIX).testing);
    // The window lingers a minute, then closes.
    assert_eq!(machine.next_deadline(4), Some(4 + lifecycle::LINGER_MS));
}

#[test]
fn per_person_choices_reach_the_page() {
    let mut machine = ready();
    live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    let effects = machine.set_volume("x", 150);
    assert!(engine(&effects).iter().any(|c| matches!(
        c,
        EngineCommand::People { people } if people[0].volume == 150 && !people[0].silent
    )));
    let effects = machine.set_muted_for_me("x", true);
    assert!(engine(&effects).iter().any(|c| matches!(
        c,
        EngineCommand::People { people } if people[0].silent
    )));
    let view = machine.view(0, UNIX).room.unwrap();
    assert_eq!(view.people[0].volume, 150);
    assert!(view.people[0].muted_for_me);
    // The same again changes nothing.
    assert!(machine.set_volume("x", 150).is_empty());
}

#[test]
fn crew_rooms_show_with_their_limit() {
    let mut machine = ready();
    machine.crew_rooms(
        CREW,
        vec![CrewRoom {
            id: GENERAL.into(),
            name: "Genel".into(),
            ids: vec!["a".into()],
            ..CrewRoom::default()
        }],
    );
    let view = machine.view(0, UNIX);
    assert_eq!(view.crews.len(), 1);
    assert!(view.crews[0].rooms[0].general);
    assert!(view.crews[0].can_add);
    machine.crew_rooms(CREW, Vec::new());
    assert!(machine.view(0, UNIX).crews.is_empty());
}

/// VOICE-ROOMS: "Genel" first even when an older service left it out, the crew's order after it,
/// each room's own limit and opener, the switch, and the room count with "Genel" in it.
#[test]
fn crew_rooms_list_genel_first_in_the_crews_order() {
    let mut machine = ready();
    machine.hub_settings(ServiceVoice {
        manage: true,
        ..on()
    });
    machine.crew_rooms(
        CREW,
        vec![
            CrewRoom {
                id: "c00000000000000bb".into(),
                name: "Taş Ocağı".into(),
                ids: vec!["a".into()],
                max: Some(4),
                own: true,
                share_mods: false,
            },
            CrewRoom {
                id: "c00000000000000aa".into(),
                name: "Oda 2".into(),
                ..CrewRoom::default()
            },
        ],
    );
    let crews = machine.view(0, UNIX).crews;
    let rooms = &crews[0].rooms;
    assert_eq!(
        rooms.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        [GENERAL, "c00000000000000bb", "c00000000000000aa"]
    );
    assert!(rooms[0].general && rooms[0].people.is_empty());
    assert_eq!((rooms[1].max, rooms[1].own), (Some(4), true));
    assert_eq!(rooms[2].shown_name(true), "Room 2");
    assert!(crews[0].manage);
    // Three rooms with "Genel": the crew has all it may (3 by default).
    assert!(!crews[0].can_add);
}

#[test]
fn a_refused_first_connect_answers_the_join() {
    let mut machine = ready();
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("t"),
            calling: None,
            opened_by_me: false,
        },
        0,
        UNIX,
    );
    let generation = connect_generation(&effects);
    let effects = machine.socket_refused(generation, Some(409), Some("room_full"), 10);
    assert_eq!(
        answer(&effects, JOIN_RID).and_then(|a| a.refused),
        Some(VoiceRefusal::RoomFull)
    );
    assert!(!machine.in_room());
    // No toast beyond the answer.
    assert!(notices(&effects).is_empty());
}

// --- VOICE-INT --------------------------------------------------------------------------------

/// The page asked for TURN; its request's `rid`.
fn ask_turn(machine: &mut Machine, now: u64) -> u32 {
    let effects = machine.page_report(PageReport::NeedTurn, now);
    match sent(&effects)[..] {
        [ToRoom::Turn { rid }] => *rid,
        _ => panic!("no turn request"),
    }
}

#[test]
fn a_refused_turn_request_is_a_lost_connection() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    let rid = ask_turn(&mut machine, 10_000);
    // Waiting for TURN is no audio: "Yeniden bağlanıyor…" after 2 s.
    assert_eq!(
        machine.view(12_000, UNIX).room.unwrap().phase,
        VoicePhase::Reconnecting
    );
    let effects = machine.socket_message(
        generation,
        FromRoom::Error {
            rid: Some(rid),
            code: "turn_unavailable".into(),
            retry_after: None,
            until: None,
            item: None,
        },
        10_100,
        UNIX,
    );
    // L2's P1: never a stay without a connection; the page joins anew at once.
    assert!(asks_join(&effects));
    // The room's next hello answers the new offer, but only `connected` brings the call back.
    machine.page_report(
        PageReport::Offer {
            sdp: Some("v=0 again".into()),
            mid: Some("0".into()),
        },
        10_200,
    );
    machine.socket_message(
        generation,
        hello(vec![person("me", 1), person("x", 2)], Role::Leader, true),
        10_300,
        UNIX,
    );
    assert_eq!(
        machine.view(12_500, UNIX).room.unwrap().phase,
        VoicePhase::Reconnecting
    );
    machine.page_report(PageReport::Connected, 13_000);
    assert_eq!(
        machine.view(13_000, UNIX).room.unwrap().phase,
        VoicePhase::Live
    );
    // An `ack` without credentials, and a TURN request with no socket to go on, are lost too.
    let rid = ask_turn(&mut machine, 20_000);
    let effects = machine.socket_message(
        generation,
        FromRoom::Ack {
            rid: Some(rid),
            ice: None,
            until: None,
            sdp: None,
            n: None,
            preset: None,
        },
        20_100,
        UNIX,
    );
    assert!(asks_join(&effects));
    machine.page_report(
        PageReport::Offer {
            sdp: Some("v=0 third".into()),
            mid: Some("0".into()),
        },
        20_200,
    );
    machine.page_report(PageReport::Connected, 20_300);
    machine.socket_closed(generation, None, 30_000);
    let effects = machine.page_report(PageReport::NeedTurn, 30_100);
    assert!(sent(&effects).is_empty());
    assert!(asks_join(&effects));
}

#[test]
fn a_lost_connection_asks_again_even_without_an_offer() {
    let mut machine = ready();
    live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    // The page could not even make its connection (`failed("offer")` + `lost`): the offer the
    // launcher asked for never came, and still a new join goes out.
    let effects = machine.page_report(PageReport::Lost, 10_000);
    assert!(asks_join(&effects));
    machine.page_report(
        PageReport::Failed {
            code: "offer".into(),
        },
        10_050,
    );
    let effects = machine.page_report(PageReport::Lost, 10_100);
    assert!(!asks_join(&effects), "the second rejoin waits");
    // 1 s later, by the one timer.
    assert_eq!(machine.next_deadline(10_100), Some(11_100));
    let effects = machine.tick(11_100, UNIX, 11_100);
    assert!(asks_join(&effects));
    // A working connection starts the waits over.
    machine.page_report(
        PageReport::Offer {
            sdp: Some("v=0 ok".into()),
            mid: Some("0".into()),
        },
        11_200,
    );
    machine.page_report(PageReport::Connected, 11_500);
    let effects = machine.page_report(PageReport::Lost, 20_000);
    assert!(asks_join(&effects));
}

#[test]
fn rejoins_that_never_connect_give_up_without_spinning() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    let mut now = 10_000;
    let mut joins = 0;
    let mut ticks = 0;
    // The page reports the connection lost: now, then 5 s after every join's answer (the room
    // answers each new offer, the connection never comes, as through a network without UDP).
    let mut lost_due = Some(now);
    while machine.in_room() {
        let next = match (machine.next_deadline(now), lost_due) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => panic!("nothing to wait for while the audio is away"),
        };
        assert!(next >= now, "a moment in the past");
        now = next;
        let effects = if lost_due == Some(now) {
            lost_due = None;
            machine.page_report(PageReport::Lost, now)
        } else {
            ticks += 1;
            assert!(ticks < 100, "the timer spins");
            machine.tick(now, UNIX, now)
        };
        if asks_join(&effects) {
            joins += 1;
            machine.page_report(
                PageReport::Offer {
                    sdp: Some("v=0".into()),
                    mid: Some("0".into()),
                },
                now,
            );
            machine.socket_message(
                generation,
                hello(vec![person("me", 1), person("x", 2)], Role::Leader, true),
                now,
                UNIX,
            );
            lost_due = Some(now + 5_000);
        }
    }
    assert!(now <= 10_000 + GIVE_UP_MS, "gave up at {now}");
    assert!((4..=10).contains(&joins), "{joins} joins in a minute");
    assert!(machine.retry_ticket().is_some());
}

#[test]
fn the_playout_path_holds_the_volume_slider() {
    let mut machine = ready();
    live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    machine.set_volume("x", 150);
    assert_eq!(machine.view(0, UNIX).volume_max, 200);
    let effects = machine.page_report(
        PageReport::Playout {
            playout: crate::gain::Playout::Element,
        },
        10,
    );
    assert!(engine(&effects).iter().any(|c| matches!(
        c,
        EngineCommand::People { people } if people[0].volume == 100
    )));
    let view = machine.view(10, UNIX);
    assert_eq!(view.volume_max, 100);
    assert_eq!(view.room.unwrap().people[0].volume, 100);
    // The same path again says nothing; the kept 150 comes back with WebAudio.
    assert!(
        machine
            .page_report(
                PageReport::Playout {
                    playout: crate::gain::Playout::Element,
                },
                20,
            )
            .is_empty()
    );
    machine.page_report(
        PageReport::Playout {
            playout: crate::gain::Playout::WebAudio,
        },
        30,
    );
    let view = machine.view(30, UNIX);
    assert_eq!(view.volume_max, 200);
    assert_eq!(view.room.unwrap().people[0].volume, 150);
}

#[test]
fn cards_name_a_friends_friend() {
    let mut machine = ready();
    let generation = live_call(
        &mut machine,
        vec![
            person("me", 1),
            person("x", 2),
            carded("909", 3, "KumFirtinasi"),
        ],
    );
    let view = machine.view(0, UNIX);
    assert_eq!(view.profiles.len(), 1);
    assert_eq!(view.profiles[0].id, "909");
    assert_eq!(view.profiles[0].name, "KumFirtinasi");
    assert_eq!(view.profiles[0].avatar, "preset:wolf");
    assert_eq!(view.profiles[0].marks, vec!["verified".to_owned()]);
    machine.socket_message(
        generation,
        FromRoom::Joined {
            person: carded("910", 4, "GeceKusu_7"),
        },
        10,
        UNIX,
    );
    assert_eq!(machine.view(10, UNIX).profiles.len(), 2);
    machine.socket_message(
        generation,
        FromRoom::Left {
            u: "909".into(),
            why: crate::wire::LeftWhy::Left,
        },
        20,
        UNIX,
    );
    let names: Vec<String> = machine
        .view(20, UNIX)
        .profiles
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["GeceKusu_7"]);
    machine.leave(30);
    assert!(machine.view(30, UNIX).profiles.is_empty());
}

#[test]
fn crewmates_in_a_crew_room_are_in_voice() {
    let mut machine = ready();
    machine.crew_rooms(
        CREW,
        vec![CrewRoom {
            id: GENERAL.into(),
            name: "Genel".into(),
            ids: vec!["a".into(), "me".into()],
            ..CrewRoom::default()
        }],
    );
    let presence = machine.view(0, UNIX).presence;
    assert_eq!(presence.len(), 1);
    assert_eq!(presence.get("a"), Some(&pl_social::PresenceVoice::In));
    machine.crew_rooms(CREW, Vec::new());
    assert!(machine.view(0, UNIX).presence.is_empty());
}

#[test]
fn calling_back_ends_the_missed_call() {
    let mut machine = ready();
    machine.ring(
        IncomingRing {
            room: "f00000000000000ff".into(),
            from: "kaan".into(),
            at: UNIX,
            until: UNIX,
            ticket: Ticket::new("t"),
            with: Vec::new(),
        },
        false,
        0,
        UNIX,
    );
    assert_eq!(machine.view(0, UNIX).missed.len(), 1);
    // "Geri ara": a new room that rings kaan.
    machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("v1.call.mac"),
            calling: Some("kaan".into()),
            opened_by_me: false,
        },
        10,
        UNIX + 60,
    );
    assert!(machine.view(10, UNIX + 60).missed.is_empty());
}

#[test]
fn the_backup_revision_follows_what_it_carries() {
    let mut machine = ready();
    live_call(&mut machine, vec![person("me", 1), person("x", 2)]);
    let start = machine.view(0, UNIX).backup_rev;
    machine.set_volume("x", 120);
    assert_eq!(machine.view(0, UNIX).backup_rev, start + 1);
    machine.set_muted_for_me("x", true);
    machine.save_settings(
        VoiceSettings {
            sounds: false,
            ..VoiceSettings::default()
        },
        0,
    );
    assert_eq!(machine.view(0, UNIX).backup_rev, start + 3);
    // The same again changes nothing.
    machine.set_volume("x", 120);
    assert_eq!(machine.view(0, UNIX).backup_rev, start + 3);
}

// --- VOICE-CALL: the call's feel ------------------------------------------------------------------

/// The page's sounds among the effects, in order.
fn sounds(effects: &[Effect]) -> Vec<EngineSound> {
    engine(effects)
        .into_iter()
        .filter_map(|command| match command {
            EngineCommand::Sound { sound } => Some(*sound),
            _ => None,
        })
        .collect()
}

/// A call the player starts: joined, the page attached, the socket open, the offer sent.
fn calling(machine: &mut Machine, friend: &str) -> u64 {
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("v1.call.mac"),
            calling: Some(friend.into()),
            opened_by_me: false,
        },
        1_000,
        UNIX,
    );
    let generation = connect_generation(&effects);
    machine.page_attached();
    machine.socket_open(generation, 1_100);
    machine.page_report(
        PageReport::Offer {
            sdp: Some("v=0 offer".into()),
            mid: Some("0".into()),
        },
        1_200,
    );
    generation
}

#[test]
fn alone_nothing_goes_out_until_someone_comes() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1)]);
    // Alone: the gate stays shut, whatever the microphone.
    assert!(!machine.view(0, UNIX).room.unwrap().me.mute);
    let effects = machine.socket_message(
        generation,
        FromRoom::Joined {
            person: person("friend", 5),
        },
        2_000,
        UNIX,
    );
    assert!(engine(&effects).contains(&&talk(true, false)));
    let effects = machine.socket_message(
        generation,
        FromRoom::Left {
            u: "friend".into(),
            why: crate::wire::LeftWhy::Gone,
        },
        3_000,
        UNIX,
    );
    assert!(engine(&effects).contains(&&talk(false, false)));
}

#[test]
fn the_call_timer_starts_with_the_first_one_to_share_the_room() {
    let mut machine = ready();
    let generation = calling(&mut machine, "friend");
    machine.socket_message(
        generation,
        hello(vec![person("me", 1)], Role::Leader, true),
        1_300,
        UNIX,
    );
    assert_eq!(machine.view(1_300, UNIX).together_since, None);
    machine.socket_message(
        generation,
        FromRoom::Joined {
            person: person("friend", 9),
        },
        5_000,
        UNIX + 4,
    );
    assert_eq!(machine.view(5_000, UNIX + 4).together_since, Some(UNIX + 4));
    // People coming and going, and a reconnect's `hello`, keep it.
    machine.socket_message(
        generation,
        FromRoom::Joined {
            person: person("other", 12),
        },
        8_000,
        UNIX + 7,
    );
    machine.socket_message(
        generation,
        hello(
            vec![person("me", 1), person("friend", 9)],
            Role::Leader,
            false,
        ),
        9_000,
        UNIX + 8,
    );
    assert_eq!(machine.view(9_000, UNIX + 8).together_since, Some(UNIX + 4));
    // The one who answers: from their first `hello` with the caller in it.
    let mut callee = ready();
    live_call(&mut callee, vec![person("me", 3), person("caller", 1)]);
    assert_eq!(callee.view(0, UNIX).together_since, Some(UNIX));
    callee.leave(10_000);
    assert_eq!(callee.view(10_000, UNIX).together_since, None);
}

#[test]
fn a_call_sounds_as_it_goes() {
    let mut machine = ready();
    let generation = calling(&mut machine, "friend");
    // Waiting alone: the caller's tone, no "connected" yet.
    let effects = machine.socket_message(
        generation,
        hello(vec![person("me", 1)], Role::Leader, true),
        1_300,
        UNIX,
    );
    assert_eq!(sounds(&effects), [EngineSound::Waiting]);
    // The friend comes: the tone stops and the call connects (not "someone came").
    let effects = machine.socket_message(
        generation,
        FromRoom::Joined {
            person: person("friend", 5),
        },
        2_000,
        UNIX,
    );
    assert_eq!(
        sounds(&effects),
        [EngineSound::Stop, EngineSound::Connected]
    );
    // Someone else comes, then leaves.
    let effects = machine.socket_message(
        generation,
        FromRoom::Joined {
            person: person("other", 6),
        },
        3_000,
        UNIX,
    );
    assert_eq!(sounds(&effects), [EngineSound::Join]);
    let effects = machine.socket_message(
        generation,
        FromRoom::Left {
            u: "other".into(),
            why: crate::wire::LeftWhy::Gone,
        },
        4_000,
        UNIX,
    );
    assert_eq!(sounds(&effects), [EngineSound::Leave]);
    // The player's own toggles, wherever they come from.
    assert_eq!(sounds(&machine.set_mute(true)), [EngineSound::MicOff]);
    assert_eq!(sounds(&machine.set_mute(false)), [EngineSound::MicOn]);
    assert_eq!(sounds(&machine.set_deaf(true)), [EngineSound::DeafOn]);
    // Opening the microphone while deafened brings the sound back: that is what it says.
    assert_eq!(sounds(&machine.set_mute(false)), [EngineSound::DeafOff]);
    assert_eq!(
        sounds(&machine.key(KeyChange {
            shortcut: VoiceShortcut::Deafen,
            down: true,
        })),
        [EngineSound::DeafOn]
    );
    machine.set_deaf(false);
    // A moderator's mute turns the microphone off, its end on again.
    let state = |sm| FromRoom::State {
        u: "me".into(),
        mute: None,
        deaf: None,
        sm: Some(sm),
        q: None,
        sh: None,
    };
    let effects = machine.socket_message(generation, state(true), 5_000, UNIX);
    assert_eq!(sounds(&effects), [EngineSound::MicOff]);
    let effects = machine.socket_message(generation, state(false), 5_100, UNIX);
    assert_eq!(sounds(&effects), [EngineSound::MicOn]);
    // Leaving: the end is heard before the page closes the call.
    let effects = machine.leave(6_000);
    let commands = engine(&effects);
    let ended = commands
        .iter()
        .position(|c| {
            **c == EngineCommand::Sound {
                sound: EngineSound::Ended,
            }
        })
        .expect("the end's sound");
    let leave = commands
        .iter()
        .position(|c| matches!(c, EngineCommand::Leave))
        .expect("the page's leave");
    assert!(ended < leave);
}

#[test]
fn joining_sounds_once_and_a_move_is_no_ending() {
    let mut machine = ready();
    // Into a room with people: in.
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("t"),
            calling: None,
            opened_by_me: false,
        },
        1_000,
        UNIX,
    );
    let generation = connect_generation(&effects);
    machine.page_attached();
    machine.socket_open(generation, 1_100);
    let effects = machine.socket_message(
        generation,
        hello(
            vec![person("me", 2), person("friend", 1)],
            Role::Member,
            true,
        ),
        1_300,
        UNIX,
    );
    assert_eq!(sounds(&effects), [EngineSound::Connected]);
    // Another room at once (a crew room's "Katıl", a move): no ending in between.
    let effects = machine.join(
        StayStart {
            room: "c00000000000000aa".into(),
            ticket: Ticket::new("t2"),
            calling: None,
            opened_by_me: false,
        },
        2_000,
        UNIX,
    );
    assert!(sounds(&effects).is_empty());
    assert!(engine(&effects).contains(&&EngineCommand::Leave));
}

#[test]
fn the_called_friend_there_already_answers_the_call() {
    let mut machine = ready();
    let generation = calling(&mut machine, "friend");
    let effects = machine.socket_message(
        generation,
        hello(
            vec![person("me", 1), person("friend", 2)],
            Role::Leader,
            true,
        ),
        1_300,
        UNIX,
    );
    assert!(machine.view(1_300, UNIX).calling.is_none());
    assert_eq!(
        sounds(&effects),
        [EngineSound::Stop, EngineSound::Connected]
    );
    assert!(engine(&effects).contains(&&talk(true, false)));
}

#[test]
fn a_lost_connection_is_heard_and_so_is_its_return() {
    let mut machine = ready();
    let generation = live_call(&mut machine, vec![person("me", 1), person("friend", 2)]);
    machine.socket_closed(generation, None, 10_000);
    let back = machine.next_deadline(10_000).unwrap();
    let effects = machine.tick(back, UNIX, back);
    let again = connect_generation(&effects);
    assert!(sounds(&effects).is_empty(), "a blip under 2 s says nothing");
    machine.socket_open(again, 11_500);
    let effects = machine.tick(12_000, UNIX, 12_000);
    assert_eq!(sounds(&effects), [EngineSound::Lost]);
    let effects = machine.socket_message(
        again,
        hello(
            vec![person("me", 1), person("friend", 2)],
            Role::Leader,
            false,
        ),
        12_100,
        UNIX,
    );
    assert_eq!(sounds(&effects), [EngineSound::Connected]);
}

#[test]
fn voice_chat_sounds_off_keeps_only_the_callers_tone() {
    let mut machine = ready();
    machine.save_settings(
        VoiceSettings {
            sounds: false,
            ..VoiceSettings::default()
        },
        0,
    );
    let generation = calling(&mut machine, "friend");
    let effects = machine.socket_message(
        generation,
        hello(vec![person("me", 1)], Role::Leader, true),
        1_300,
        UNIX,
    );
    assert_eq!(sounds(&effects), [EngineSound::Waiting]);
    let effects = machine.socket_message(
        generation,
        FromRoom::Joined {
            person: person("friend", 5),
        },
        2_000,
        UNIX,
    );
    assert_eq!(sounds(&effects), [EngineSound::Stop]);
    assert!(sounds(&machine.set_mute(true)).is_empty());
    assert!(sounds(&machine.leave(3_000)).is_empty());
}

#[test]
fn the_meters_run_only_while_the_page_shows_them() {
    let mut machine = ready();
    let meters = |effects: &[Effect]| -> Vec<(bool, bool)> {
        engine(effects)
            .into_iter()
            .filter_map(|command| match command {
                EngineCommand::Meters { me, people } => Some((*me, *people)),
                _ => None,
            })
            .collect()
    };
    // Asked without a room: nothing to measure.
    assert!(meters(&machine.set_meters(true)).is_empty());
    assert!(
        machine
            .levels_of(0.5, true, Vec::new(), Vec::new())
            .is_none()
    );
    // In a room the page measures both.
    let effects = machine.join(
        StayStart {
            room: ROOM.into(),
            ticket: Ticket::new("t"),
            calling: None,
            opened_by_me: false,
        },
        1_000,
        UNIX,
    );
    let generation = connect_generation(&effects);
    assert_eq!(meters(&machine.page_attached()), [(true, true)]);
    machine.socket_open(generation, 1_100);
    machine.socket_message(
        generation,
        hello(
            vec![person("me", 2), person("friend", 1)],
            Role::Member,
            true,
        ),
        1_300,
        UNIX,
    );
    // Only people in the room, levels held to 0–1.
    let levels = machine
        .levels_of(
            1.5,
            true,
            vec!["friend".into(), "stranger".into()],
            vec![0.4, 0.9],
        )
        .unwrap();
    assert_eq!(levels.me, 1.0);
    assert!(levels.open);
    assert_eq!(levels.people.len(), 1);
    assert_eq!(levels.people[0].id, "friend");
    // Muted: the microphone is not measured any more (and says nothing).
    assert_eq!(meters(&machine.set_mute(true)), [(false, true)]);
    let levels = machine
        .levels_of(0.5, true, Vec::new(), Vec::new())
        .unwrap();
    assert_eq!(levels.me, 0.0);
    assert!(!levels.open);
    assert_eq!(meters(&machine.set_mute(false)), [(true, true)]);
    // The window went away: both off.
    assert_eq!(meters(&machine.set_meters(false)), [(false, false)]);
    assert!(
        machine
            .levels_of(0.5, true, Vec::new(), Vec::new())
            .is_none()
    );
    // A leave turns the page's meters off by itself: nothing more to say.
    machine.set_meters(true);
    let effects = machine.leave(2_000);
    assert!(meters(&effects).is_empty());
    assert!(meters(&machine.set_meters(false)).is_empty());
}

#[test]
fn asking_before_a_call_and_the_master_volume_are_settings() {
    let mut machine = ready();
    assert!(machine.view(0, UNIX).ask_before_call);
    let (_, effects) = machine.save_settings(
        VoiceSettings {
            ask_before_call: false,
            output_volume: 160,
            ..VoiceSettings::default()
        },
        0,
    );
    assert!(!machine.view(0, UNIX).ask_before_call);
    // No page yet: nothing configured.
    assert!(engine(&effects).is_empty());
    let configured = |effects: &[Effect]| {
        engine(effects)
            .into_iter()
            .find_map(|command| match command {
                EngineCommand::Configure { settings } => Some(settings.output_volume),
                _ => None,
            })
    };
    live_call(&mut machine, vec![person("me", 1), person("friend", 2)]);
    let effects = machine.page_attached();
    assert_eq!(configured(&effects), Some(160));
    // A page that plays through `<audio>` stops at 100 %, for everyone as for each.
    let effects = machine.page_report(
        PageReport::Playout {
            playout: crate::gain::Playout::Element,
        },
        2_000,
    );
    assert_eq!(configured(&effects), Some(100));
}
