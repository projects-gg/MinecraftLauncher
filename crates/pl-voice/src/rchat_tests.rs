//! The voice room's own text chat on the machine (SQOL `rchat1`, the lead's contract
//! `docs/tasks/sqol.md` §1.3, §2.2).

use std::time::Duration;

use pl_social::BackoffPolicy;
use serde_json::json;

use crate::engine::PageReport;
use crate::machine::{Effect, Machine, ServiceVoice, StayStart};
use crate::rchat::{
    LOG_MAX, REPORT_BEFORE, RoomChatBy, RoomChatError, RoomChatReason, RoomChatSend, VoiceChatEvent,
};
use crate::settings::Store;
use crate::wire::{
    FromRoom, LeftWhy, MeWire, PersonWire, Role, RoomKind, RoomWire, Sdp, Ticket, ToRoom,
};

const ROOM: &str = "f0123456789abcdef";
const UNIX: i64 = 1_800_000_000;

fn person(u: &str) -> PersonWire {
    PersonWire {
        u: u.into(),
        j: 1,
        mute: false,
        deaf: false,
        sm: false,
        q: 0,
        card: None,
        sh: Vec::new(),
    }
}

/// In a friend room with `others`; `chat`: the room has its own chat.
fn in_room(others: &[&str], chat: bool) -> (Machine, u64) {
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
    let mut people = vec![person("me")];
    people.extend(others.iter().map(|u| person(u)));
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
                share_by: None,
                ptt: false,
                chat,
                grew: false,
            },
            me: MeWire {
                role: Role::Leader,
                watch: None,
            },
            people,
            rings: Vec::new(),
            sdp: Some(Sdp::new("v=0 answer")),
            resume: Some(Ticket::new("v1.resume.mac")),
            now: Some(UNIX),
        },
        1_300,
        UNIX,
    );
    machine.page_report(PageReport::Connected, 1_400);
    (machine, generation)
}

fn message(id: &str, from: &str, text: &str) -> serde_json::Value {
    json!({"id": id, "u": from, "at": 1_731_500_000_000_i64, "text": text, "seal": format!("seal-{id}")})
}

fn chat_events(effects: &[Effect]) -> Vec<&VoiceChatEvent> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Chat(event) => Some(event),
            _ => None,
        })
        .collect()
}

fn chat_answer(effects: &[Effect], rid: u32) -> Option<&RoomChatSend> {
    effects.iter().find_map(|effect| match effect {
        Effect::ChatAnswer {
            rid: answered,
            answer,
        } if *answered == rid => Some(answer),
        _ => None,
    })
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

#[test]
fn no_chat_without_the_rooms_word() {
    let (mut machine, _) = in_room(&["a"], false);
    let room = machine.view(2_000, UNIX).room.unwrap();
    assert!(!room.chat && !room.grew);
    assert_eq!(
        machine.chat_send(ROOM, Some("selam"), None),
        Err(RoomChatSend::refused(RoomChatError::Off))
    );
    // Another room: gone.
    assert_eq!(
        machine.chat_send("f1111111111111111", Some("selam"), None),
        Err(RoomChatSend::refused(RoomChatError::Gone))
    );
}

#[test]
fn messages_the_log_and_deletions_reach_the_page() {
    let (mut machine, generation) = in_room(&["a"], true);
    assert!(machine.view(2_000, UNIX).room.unwrap().chat);
    // The room's log on joining replaces whatever was held.
    let effects = machine.socket_message(
        generation,
        FromRoom::RchatLog {
            ms: vec![message("m1", "a", "selam"), json!({"odd": true})],
        },
        2_000,
        UNIX,
    );
    assert!(matches!(
        chat_events(&effects)[..],
        [VoiceChatEvent::Log { room, msgs }] if room == ROOM && msgs.len() == 1 && msgs[0].id == "m1"
    ));
    let effects = machine.socket_message(
        generation,
        FromRoom::Rchat {
            m: message("m2", "me", "naber"),
        },
        2_100,
        UNIX,
    );
    assert!(matches!(
        chat_events(&effects)[..],
        [VoiceChatEvent::Msg { room, msg }]
            if room == ROOM && msg.from == "me" && msg.text.as_deref() == Some("naber")
    ));
    assert_eq!(
        machine
            .chat_log(ROOM)
            .iter()
            .map(|msg| msg.id.as_str())
            .collect::<Vec<_>>(),
        ["m1", "m2"]
    );
    assert!(machine.chat_log("f1111111111111111").is_empty());
    let effects = machine.socket_message(
        generation,
        FromRoom::RchatDeleted {
            id: "m1".into(),
            by: "mod".into(),
        },
        2_200,
        UNIX,
    );
    assert_eq!(
        chat_events(&effects),
        [&VoiceChatEvent::Del {
            room: ROOM.into(),
            id: "m1".into(),
            by: RoomChatBy::Mod
        }]
    );
    assert_eq!(machine.chat_log(ROOM).len(), 1);
}

#[test]
fn a_message_waits_for_the_rooms_answer() {
    let (mut machine, generation) = in_room(&["a"], true);
    let (rid, effects) = machine.chat_send(ROOM, Some(" selam "), None).unwrap();
    assert_eq!(
        sent(&effects),
        [&ToRoom::Rchat {
            rid,
            text: Some("selam".into()),
            st: None
        }]
    );
    let effects = machine.socket_message(
        generation,
        FromRoom::Ack {
            rid: Some(rid),
            ice: None,
            until: None,
            sdp: None,
            n: None,
            preset: None,
            fps: None,
            audio: None,
            id: Some("1731500000123".into()),
            at: Some(1_731_500_000_123),
            m: Some(json!({"id": "1731500000123", "u": "me", "at": 1_731_500_000_123_i64, "text": "selam", "seal": "s"})),
        },
        2_000,
        UNIX,
    );
    assert_eq!(
        chat_answer(&effects, rid),
        Some(&RoomChatSend::Sent {
            id: "1731500000123".into(),
            at: 1_731_500_000_123
        })
    );
    // The sender gets no `rchat` of their own: the answer's message shows (as the room cleaned it).
    assert!(matches!(
        chat_events(&effects)[..],
        [VoiceChatEvent::Msg { msg, .. }] if msg.from == "me" && msg.text.as_deref() == Some("selam")
    ));
    assert_eq!(machine.chat_log(ROOM).len(), 1);
    // A sticker goes as its id; the room's refusal comes back with its reason and rule.
    let (rid, effects) = machine.chat_send(ROOM, None, Some("pl1/gg")).unwrap();
    assert_eq!(
        sent(&effects),
        [&ToRoom::Rchat {
            rid,
            text: None,
            st: Some("pl1/gg".into())
        }]
    );
    let effects = machine.socket_message(
        generation,
        FromRoom::Error {
            rid: Some(rid),
            code: "chat_rejected".into(),
            retry_after: None,
            until: None,
            item: None,
            reason: Some("link".into()),
            rule: Some("r1".into()),
        },
        2_100,
        UNIX,
    );
    assert_eq!(
        chat_answer(&effects, rid),
        Some(&RoomChatSend::Refused {
            error: RoomChatError::Rejected,
            reason: Some(RoomChatReason::Link),
            rule: Some("r1".into()),
            retry_after: None,
            until: None,
        })
    );
    // Too long never goes.
    assert!(matches!(
        machine.chat_send(ROOM, Some(&"a".repeat(301)), None),
        Err(RoomChatSend::Refused {
            reason: Some(RoomChatReason::Length),
            ..
        })
    ));
}

#[test]
fn only_a_held_message_is_deleted_or_reported() {
    let (mut machine, generation) = in_room(&["a"], true);
    assert_eq!(
        machine.chat_delete(ROOM, "m9"),
        Err(RoomChatSend::refused(RoomChatError::Gone))
    );
    for i in 0..(LOG_MAX + 5) {
        machine.socket_message(
            generation,
            FromRoom::Rchat {
                m: message(&format!("m{i}"), "a", "selam"),
            },
            2_000,
            UNIX,
        );
    }
    // The oldest go past the log's size.
    let log = machine.chat_log(ROOM);
    assert_eq!(log.len(), LOG_MAX);
    assert_eq!(log[0].id, "m5");
    let (rid, effects) = machine.chat_delete(ROOM, "m30").unwrap();
    assert_eq!(
        sent(&effects),
        [&ToRoom::RchatDel {
            rid,
            id: "m30".into()
        }]
    );
    // The report's evidence: the sealed message and the twenty before it, as held.
    let (sealed, before) = machine.chat_evidence(ROOM, "m40").unwrap();
    assert_eq!(sealed["seal"], "seal-m40");
    assert_eq!(before.len(), REPORT_BEFORE);
    assert_eq!(before[0]["id"], "m20");
    assert_eq!(before[REPORT_BEFORE - 1]["id"], "m39");
    let (_, before) = machine.chat_evidence(ROOM, "m5").unwrap();
    assert!(before.is_empty());
    assert_eq!(machine.chat_evidence(ROOM, "m1"), None);
    assert_eq!(machine.chat_evidence("f1111111111111111", "m40"), None);
}

#[test]
fn the_room_grew_once_a_third_came() {
    let (mut machine, generation) = in_room(&["a"], true);
    assert!(!machine.view(2_000, UNIX).room.unwrap().grew);
    machine.socket_message(
        generation,
        FromRoom::Joined {
            person: person("b"),
        },
        2_000,
        UNIX,
    );
    assert!(machine.view(2_000, UNIX).room.unwrap().grew);
    machine.socket_message(
        generation,
        FromRoom::Left {
            u: "b".into(),
            why: LeftWhy::Left,
        },
        2_100,
        UNIX,
    );
    // It stays the room's chat for good.
    assert!(machine.view(2_100, UNIX).room.unwrap().grew);
    let (machine, _) = in_room(&["a", "b"], true);
    assert!(machine.view(2_000, UNIX).room.unwrap().grew);
}

#[test]
fn leaving_answers_waiting_messages_and_forgets_the_log() {
    let (mut machine, generation) = in_room(&["a"], true);
    machine.socket_message(
        generation,
        FromRoom::Rchat {
            m: message("m1", "a", "selam"),
        },
        2_000,
        UNIX,
    );
    let (rid, _) = machine.chat_send(ROOM, Some("görüşürüz"), None).unwrap();
    let effects = machine.leave(3_000);
    assert_eq!(
        chat_answer(&effects, rid),
        Some(&RoomChatSend::refused(RoomChatError::Gone))
    );
    assert!(machine.chat_log(ROOM).is_empty());
}

/// SQOL-R3: a slow answer still settles the sender's message (nothing waits on a clock in the
/// machine); a socket that goes answers at once, since its answer can no longer come.
#[test]
fn a_slow_answer_still_settles_and_a_lost_socket_answers_at_once() {
    let (mut machine, generation) = in_room(&["a"], true);
    let (rid, _) = machine.chat_send(ROOM, Some("selam"), None).unwrap();
    machine.tick(30_000, UNIX, 0);
    let effects = machine.socket_message(
        generation,
        FromRoom::Ack {
            rid: Some(rid),
            ice: None,
            until: None,
            sdp: None,
            n: None,
            preset: None,
            fps: None,
            audio: None,
            id: Some("1731500000123".into()),
            at: Some(1_731_500_000_123),
            m: Some(json!({"id": "1731500000123", "u": "me", "at": 1_731_500_000_123_i64, "text": "selam", "seal": "s"})),
        },
        31_000,
        UNIX,
    );
    assert!(matches!(
        chat_answer(&effects, rid),
        Some(RoomChatSend::Sent { .. })
    ));
    assert_eq!(machine.chat_log(ROOM).len(), 1);
    let (rid, _) = machine.chat_send(ROOM, Some("orada mısın"), None).unwrap();
    let effects = machine.socket_closed(generation, None, 32_000);
    assert_eq!(
        chat_answer(&effects, rid),
        Some(&RoomChatSend::refused(RoomChatError::Failed))
    );
}

#[test]
fn a_chat_switched_off_takes_its_messages() {
    let (mut machine, generation) = in_room(&["a"], true);
    machine.socket_message(
        generation,
        FromRoom::Rchat {
            m: message("m1", "a", "selam"),
        },
        2_000,
        UNIX,
    );
    // Studio's switch: the room says its `room` again, without `chat`.
    let effects = machine.socket_message(
        generation,
        FromRoom::Room {
            name: None,
            leader: Some("me".into()),
            mods: Vec::new(),
            max: None,
            share_by: None,
            ptt: false,
            chat: false,
        },
        3_000,
        UNIX,
    );
    assert!(matches!(
        chat_events(&effects)[..],
        [VoiceChatEvent::Log { msgs, .. }] if msgs.is_empty()
    ));
    assert!(!machine.view(3_000, UNIX).room.unwrap().chat);
    assert!(machine.chat_log(ROOM).is_empty());
    // A deletion of a message no longer held says nothing.
    let effects = machine.socket_message(
        generation,
        FromRoom::RchatDeleted {
            id: "m1".into(),
            by: "staff".into(),
        },
        3_100,
        UNIX,
    );
    assert!(chat_events(&effects).is_empty());
}

#[test]
fn a_blocked_senders_messages_leave_the_page() {
    let (mut machine, generation) = in_room(&["a", "b"], true);
    for (id, from) in [("m1", "a"), ("m2", "b"), ("m3", "a")] {
        machine.socket_message(
            generation,
            FromRoom::Rchat {
                m: message(id, from, "selam"),
            },
            2_000,
            UNIX,
        );
    }
    let effects = machine.blocked("a", 3_000);
    assert!(matches!(
        chat_events(&effects)[..],
        [VoiceChatEvent::Log { msgs, .. }] if msgs.len() == 1 && msgs[0].id == "m2"
    ));
}
