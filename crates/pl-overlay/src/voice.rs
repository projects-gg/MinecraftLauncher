//! Voice chat in the game (VOICE-O; `docs/tasks/voice.md` "VOICE-O", `ovl2-protocol.md` rows V):
//! what the overlay shows of the launcher's voice chat, and how the player's presses there reach
//! it.
//!
//! The launcher's `pl_voice::Voice` stays the one source of truth. The app hands every state change,
//! every speaking change and every notice to the hub ([`crate::Overlay::set_voice`],
//! [`crate::Overlay::voice_speaking`], [`crate::Overlay::voice_notice`]), which keeps the newest
//! state for games that start later and passes them on to every game's session. A session tells
//! only a mod whose hello lists `voice`, and only what changed ([`VoiceMirror`]): the whole room
//! when something in it changes, the speakers on their own. Names come from the session's friend
//! lists (friends, then crewmates); someone else goes nameless and the mod says "Bir oyuncu".
//!
//! The mod's presses are checked against the newest state ([`check`]: a ring that still rings,
//! someone in the room, a friend on the list) and go to the app's voice chat
//! ([`crate::Overlay::on_voice`]), which calls the same `Voice` methods as the page's commands.
//! Nothing here runs while there is no game, and nothing in a game while voice chat is quiet.

use std::future::Future;
use std::pin::Pin;

use pl_voice::share::{
    ShareClosedWhy, ShareHoldWhy, ShareKind, ShareMinePhase, ShareNotice, ShareRefusal, ShareSt,
    VoiceShareView,
};
use pl_voice::view::{
    VoiceAnswer, VoiceLeftReason, VoiceMic, VoiceModAction, VoiceNotice, VoiceRefusal,
    VoiceSpeaking, VoiceState, shown_room_name,
};

use crate::protocol::{
    FromGame, ToGame, ToastKind, ToastView, VoiceCan, VoiceCrewRoomShown, VoiceCrewShown,
    VoiceLiveShown, VoiceMeShown, VoiceMyShareShown, VoiceNoticeView, VoicePersonRef,
    VoicePersonShown, VoiceRingShown, VoiceRoomShown, VoiceShareShown, VoiceView,
};

/// The top of a person's volume, in percent (the launcher's slider; an engine that plays through
/// `<audio>` holds it to 100 itself).
pub const MAX_VOLUME: u16 = 200;

/// What the app's voice chat answers a press with.
pub type VoiceFuture = Pin<Box<dyn Future<Output = VoiceAnswer> + Send>>;

/// SHARE-LIVE: what the app answers a still picture request with (the launcher's own
/// `share_thumb`: its 20 s keep, the player's own pictures from this computer).
pub type ThumbFuture = Pin<Box<dyn Future<Output = pl_social::VoiceThumb> + Send>>;

/// A press in the game's voice chat, for the launcher's (the page's commands of the same name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceAct {
    /// "Mikrofon": off (`true`) or on again (which ends deafening too).
    Mute(bool),
    /// "Sağırlaştır".
    Deaf(bool),
    /// "Ayrıl" ("İptal" while the player's call rings).
    Leave,
    /// "Katıl" on a ring.
    Accept { room: String },
    /// "Reddet" on a ring.
    Decline { room: String },
    /// "Ses seviyesi", in percent (0 to [`MAX_VOLUME`]).
    Volume { person: String, percent: u16 },
    /// "Benim için sustur".
    MuteForMe { person: String, on: bool },
    /// A leader's or moderator's row (only `mute`, `unmute`, `kick`, `unkick`, `mod` and `unmod`
    /// come from the game).
    Moderate(VoiceModAction),
    /// "Ara".
    Call { friend: String },
    /// "Odaya davet et".
    Invite { friend: String },
    /// "Katıl" on one of the player's crew rooms (`room`; none with `new`), or "Yeni oda" (`new`:
    /// the player opens one and joins it) in crew chat's "Sesli" (OVL-SOC).
    JoinCrew {
        crew: String,
        room: Option<String>,
        new: bool,
    },
    /// OVL-SHARE: "Oyunu paylaş": the running game with the player's saved share settings. A
    /// refusal reaches the game as a share notice ([`crate::Overlay::share_notice`]).
    ShareGame,
    /// OVL-SHARE: "Paylaşımı durdur" on the player's share `n`.
    ShareStop { n: u8 },
    /// SQOL-O: "Duraklat" (`on`: a blank picture, no sound) or "Sürdür" on the player's share `n`
    /// (only while `voice-share` says `edit`). The launcher's answer is [`hold_answer`]'s.
    ShareHold { n: u8, on: bool },
}

impl VoiceAct {
    /// The name its `voice-result` carries; none for the presses whose outcome the next `voice`
    /// shows by itself (the microphone, deafening, leaving, a decline, volumes).
    pub fn result_name(&self) -> Option<&'static str> {
        match self {
            Self::Accept { .. } | Self::JoinCrew { .. } => Some("join"),
            Self::Call { .. } => Some("call"),
            // OVL-SHARE: the mod's "Oyunu paylaş" waits for an answer (a refusal comes as a note).
            Self::ShareGame => Some("share"),
            // SQOL-O: "Duraklat" and "Sürdür" too (the button waits; the state shows how it went).
            Self::ShareHold { .. } => Some("share-hold"),
            Self::Invite { .. } => Some("invite"),
            Self::Moderate(action) => moderation_name(action),
            _ => None,
        }
    }

    /// Whom it is about (a friend, someone in the room), by id; empty for the player's own.
    pub fn who(&self) -> &str {
        match self {
            Self::Volume { person, .. } | Self::MuteForMe { person, .. } => person,
            Self::Call { friend } | Self::Invite { friend } => friend,
            Self::Moderate(
                VoiceModAction::Mute { person }
                | VoiceModAction::Unmute { person }
                | VoiceModAction::Kick { person }
                | VoiceModAction::Unkick { person }
                | VoiceModAction::Mod { person }
                | VoiceModAction::Unmod { person },
            ) => person,
            _ => "",
        }
    }
}

fn moderation_name(action: &VoiceModAction) -> Option<&'static str> {
    Some(match action {
        VoiceModAction::Mute { .. } => "mute",
        VoiceModAction::Unmute { .. } => "unmute",
        VoiceModAction::Kick { .. } => "kick",
        VoiceModAction::Unkick { .. } => "unkick",
        VoiceModAction::Mod { .. } => "mod",
        VoiceModAction::Unmod { .. } => "unmod",
        _ => return None,
    })
}

/// A press that names something the newest state does not have (a ring that is over, someone who
/// left the room, a friend no longer on the list): it does nothing, and the mod hears so when the
/// press has a result of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub act: Option<&'static str>,
    pub who: String,
    pub why: VoiceRefusal,
}

impl Refused {
    fn quiet() -> Self {
        Self {
            act: None,
            who: String::new(),
            why: VoiceRefusal::Nothing,
        }
    }

    fn of(act: &'static str, who: &str, why: VoiceRefusal) -> Self {
        Self {
            act: Some(act),
            who: who.to_owned(),
            why,
        }
    }
}

/// The launcher's act for a mod's voice frame, after its checks against `state` (the newest; none
/// before voice chat said anything), the friend list (`is_friend`) and the player's crews
/// (`is_crew`). `None`: the frame is not one of VOICE-O's.
pub fn check(
    frame: &FromGame,
    state: Option<&VoiceState>,
    is_friend: impl Fn(&str) -> bool,
    is_crew: impl Fn(&str) -> bool,
) -> Option<Result<VoiceAct, Refused>> {
    let room = state.and_then(|state| state.room.as_ref());
    let in_room =
        |person: &str| room.is_some_and(|room| room.people.iter().any(|p| p.id == person));
    Some(match frame {
        FromGame::VoiceSet { mute, deaf } => match (mute, deaf) {
            (Some(on), _) => Ok(VoiceAct::Mute(*on)),
            (None, Some(on)) => Ok(VoiceAct::Deaf(*on)),
            (None, None) => Err(Refused::quiet()),
        },
        FromGame::VoiceLeave {} => Ok(VoiceAct::Leave),
        FromGame::VoiceAnswer { room: ring, join } => {
            let rings = state.is_some_and(|state| state.rings.iter().any(|r| &r.room == ring));
            match (rings, join) {
                (true, true) => Ok(VoiceAct::Accept { room: ring.clone() }),
                (true, false) => Ok(VoiceAct::Decline { room: ring.clone() }),
                (false, true) => Err(Refused::of("join", "", VoiceRefusal::RoomGone)),
                (false, false) => Err(Refused::quiet()),
            }
        }
        FromGame::VoiceVol { person, percent } if in_room(person) => Ok(VoiceAct::Volume {
            person: person.clone(),
            percent: u16::try_from((*percent).clamp(0, i64::from(volume_top(state))))
                .unwrap_or(100),
        }),
        FromGame::VoiceLmute { person, on } if in_room(person) => Ok(VoiceAct::MuteForMe {
            person: person.clone(),
            on: *on,
        }),
        FromGame::VoiceVol { .. } | FromGame::VoiceLmute { .. } => Err(Refused::quiet()),
        FromGame::VoiceMod { person, act } => {
            let person = person.clone();
            let action = match act.as_str() {
                "mute" => VoiceModAction::Mute { person },
                "unmute" => VoiceModAction::Unmute { person },
                "kick" => VoiceModAction::Kick { person },
                "unkick" => VoiceModAction::Unkick { person },
                "mod" => VoiceModAction::Mod { person },
                "unmod" => VoiceModAction::Unmod { person },
                _ => return Some(Err(Refused::quiet())),
            };
            let act = VoiceAct::Moderate(action);
            // "Geri al" after "Odadan çıkar" names someone who is out of the room by now.
            let unkick = matches!(act, VoiceAct::Moderate(VoiceModAction::Unkick { .. }));
            if room.is_some() && (unkick || in_room(act.who())) {
                Ok(act)
            } else {
                Err(Refused::of(
                    act.result_name().unwrap_or("mod"),
                    act.who(),
                    VoiceRefusal::NotFound,
                ))
            }
        }
        FromGame::VoiceCall { friend } if is_friend(friend) => Ok(VoiceAct::Call {
            friend: friend.clone(),
        }),
        FromGame::VoiceCall { friend } => {
            Err(Refused::of("call", friend, VoiceRefusal::Unreachable))
        }
        FromGame::VoiceInvite { friend } if room.is_some() && is_friend(friend) => {
            Ok(VoiceAct::Invite {
                friend: friend.clone(),
            })
        }
        FromGame::VoiceInvite { friend } => {
            Err(Refused::of("invite", friend, VoiceRefusal::Nothing))
        }
        FromGame::VoiceJoin { crew, room, new } => join_crew(state, crew, room, *new, &is_crew),
        // OVL-SHARE: only while "Oyunu paylaş" shows (the room checks "Kim paylaşabilir" and its
        // limits again, and says no through a share notice).
        // A press that does nothing still hears a `voice-result` (the mod lays out again).
        FromGame::VoiceShareGame {} => match state {
            Some(state) if share_of(state).game == Some("ready") => Ok(VoiceAct::ShareGame),
            _ => Err(Refused::of("share", "", VoiceRefusal::Nothing)),
        },
        FromGame::VoiceShareStop { n } => match state {
            Some(state) if state.share.mine.iter().any(|share| share.n == *n) => {
                Ok(VoiceAct::ShareStop { n: *n })
            }
            _ => Err(Refused::quiet()),
        },
        // SQOL-O: "Duraklat" / "Sürdür" only on one of the player's shares while live changes may
        // go (`edit`); the launcher's voice chat checks the rest (a share still starting, a closed
        // source's "Sürdür").
        FromGame::VoiceShareHold { n, on } => match state {
            Some(state)
                if state.share.edit && state.share.mine.iter().any(|share| share.n == *n) =>
            {
                Ok(VoiceAct::ShareHold { n: *n, on: *on })
            }
            _ => Err(Refused::of("share-hold", "", VoiceRefusal::Nothing)),
        },
        _ => return None,
    })
}

/// SQOL-O: how "Duraklat" or "Sürdür" went, as `voice-result` says it: sharing that may not change
/// now is `off`, a share that is not there (or a closed source's "Sürdür") `not-found`, anything
/// else `failed`.
pub fn hold_answer(result: Result<(), ShareRefusal>) -> VoiceAnswer {
    match result {
        Ok(()) => VoiceAnswer::OK,
        Err(ShareRefusal::Off) => VoiceAnswer::refused(VoiceRefusal::Off),
        Err(ShareRefusal::Gone) => VoiceAnswer::refused(VoiceRefusal::NotFound),
        Err(_) => VoiceAnswer::refused(VoiceRefusal::Failed),
    }
}

/// "Katıl" on a crew room or "Yeni oda" (V22): only in one of the player's crews while voice chat
/// may be used, and only into a room `voice-crews` showed (one of the crew's live rooms, or its
/// "Genel" by its own id while nobody is in any). Joining a room the player is in already does
/// nothing.
fn join_crew(
    state: Option<&VoiceState>,
    crew: &str,
    room: &str,
    new: bool,
    is_crew: &impl Fn(&str) -> bool,
) -> Result<VoiceAct, Refused> {
    let Some(state) = state.filter(|state| state.available && state.enabled) else {
        return Err(Refused::of("join", "", VoiceRefusal::Off));
    };
    if crew.is_empty() || !is_crew(crew) {
        return Err(Refused::of("join", "", VoiceRefusal::NotMember));
    }
    if new {
        return Ok(VoiceAct::JoinCrew {
            crew: crew.to_owned(),
            room: None,
            new: true,
        });
    }
    if state.room.as_ref().is_some_and(|here| here.id == room) {
        return Err(Refused::quiet());
    }
    let live = state
        .crews
        .iter()
        .find(|entry| entry.crew == crew)
        .map(|entry| &entry.rooms[..])
        .unwrap_or_default();
    let shown = if live.is_empty() {
        room == general_room(crew)
    } else {
        live.iter().any(|r| r.id == room)
    };
    if !shown {
        return Err(Refused::of("join", "", VoiceRefusal::RoomGone));
    }
    Ok(VoiceAct::JoinCrew {
        crew: crew.to_owned(),
        room: Some(room.to_owned()),
        new: false,
    })
}

/// The crew's "Genel" while nobody is in any of its rooms: the friends service's own id for it
/// (`g` + the crew id), as the launcher's crew page names it.
fn general_room(crew: &str) -> String {
    format!("g{crew}")
}

/// The voice rooms of the player's crews for `voice-crews` (V21): every crew of `crews` (the
/// launcher's order), its live rooms in voice chat's order with who is in each (names from
/// `name_of`, the friend lists), else its "Genel" alone; "Yeni oda" while the crew may open one
/// more. `english`: the friends service's "Oda 2" is "Room 2" (the mod says "Genel" itself).
/// Nothing while voice chat may not be used.
pub fn crews_of<'a>(
    state: &VoiceState,
    crews: impl IntoIterator<Item = &'a str>,
    english: bool,
    name_of: impl Fn(&str) -> Option<String>,
) -> Vec<VoiceCrewShown> {
    if !(state.available && state.enabled) {
        return Vec::new();
    }
    let person = |id: &String, live: bool| VoicePersonRef {
        id: id.clone(),
        name: name_of(id).unwrap_or_default(),
        live,
    };
    crews
        .into_iter()
        .map(|crew| {
            let entry = state.crews.iter().find(|entry| entry.crew == crew);
            let rooms: Vec<VoiceCrewRoomShown> = match entry {
                Some(entry) if !entry.rooms.is_empty() => entry
                    .rooms
                    .iter()
                    .map(|room| {
                        // VOICE-ROOMS: a room's own limit below the crew's (the crew page's
                        // "3/5"); full at the lower of the two (its `crewRoomFull`).
                        let limit = room
                            .max
                            .map_or(entry.crew_max, |max| max.min(entry.crew_max));
                        VoiceCrewRoomShown {
                            id: room.id.clone(),
                            name: (!room.general).then(|| shown_room_name(&room.name, english)),
                            general: room.general,
                            // SHARE-LIVE: who shares there now.
                            people: room
                                .people
                                .iter()
                                .map(|id| person(id, room.live.contains(id)))
                                .collect(),
                            max: room.max.filter(|max| *max < entry.crew_max),
                            full: limit > 0 && room.people.len() >= usize::from(limit),
                        }
                    })
                    .collect(),
                _ => vec![VoiceCrewRoomShown {
                    id: general_room(crew),
                    name: None,
                    general: true,
                    people: Vec::new(),
                    max: None,
                    full: false,
                }],
            };
            VoiceCrewShown {
                crew: crew.to_owned(),
                conv: pl_social::crew_conversation(crew),
                rooms,
                add: entry.map_or(state.crew_rooms_max > 1, |entry| entry.can_add),
            }
        })
        .collect()
}

/// A refusal's name in `voice-result`.
pub fn refusal_name(why: VoiceRefusal) -> &'static str {
    match why {
        VoiceRefusal::Off => "off",
        VoiceRefusal::Unreachable => "unreachable",
        VoiceRefusal::RateLimited => "rate-limited",
        VoiceRefusal::AccountLimited => "account-limited",
        VoiceRefusal::StaffMuted => "staff-muted",
        VoiceRefusal::RoomFull => "room-full",
        VoiceRefusal::RoomsFull => "rooms-full",
        VoiceRefusal::RoomGone => "room-gone",
        VoiceRefusal::Kicked => "kicked",
        VoiceRefusal::NotMember => "not-member",
        VoiceRefusal::NotAllowed => "not-allowed",
        VoiceRefusal::NotFound => "not-found",
        VoiceRefusal::InvalidName => "invalid-name",
        VoiceRefusal::NameRejected => "name-rejected",
        VoiceRefusal::OldLauncher => "old-launcher",
        VoiceRefusal::Failed => "failed",
        VoiceRefusal::Nothing => "nothing",
    }
}

/// `voice-result` for an act that has one, as it went.
pub fn result_of(act: &VoiceAct, answer: &VoiceAnswer) -> Option<ToGame> {
    let name = act.result_name()?;
    Some(ToGame::VoiceResult {
        act: name,
        ok: answer.is_ok(),
        reason: answer.refused.map(refusal_name),
        who: act.who().to_owned(),
    })
}

/// `voice-result` for a press refused here (none for one without a result of its own).
pub fn refused_result(refused: &Refused) -> Option<ToGame> {
    Some(ToGame::VoiceResult {
        act: refused.act?,
        ok: false,
        reason: Some(refusal_name(refused.why)),
        who: refused.who.clone(),
    })
}

/// A person's volume at most now: the voice page's top (100 through `<audio>` elements), never past
/// [`MAX_VOLUME`]; [`MAX_VOLUME`] before voice chat said anything.
fn volume_top(state: Option<&VoiceState>) -> u16 {
    state.map_or(MAX_VOLUME, |state| state.volume_max.min(MAX_VOLUME))
}

/// The view of `state` for the mod: names from `name_of` (the friend lists), else from the room's
/// cards (`profiles`: a friend's friend; none: "Bir oyuncu"), crew names from `crew_name`. `sound`:
/// the mod rings for a ring (the launcher's window is away). `speaking`: who speaks now, when that
/// changed after `state` (the state's flags are as of its own change; a speaking change comes
/// alone). `english`: the overlay's words are English, so the friends service's "Oda 2" is "Room 2"
/// (the mod says the crew's "Genel" itself).
pub fn view_of(
    state: &VoiceState,
    sound: bool,
    speaking: Option<&VoiceSpeaking>,
    english: bool,
    name_of: impl Fn(&str) -> Option<String>,
    crew_name: impl Fn(&str) -> Option<String>,
) -> VoiceView {
    let name = |id: &str| {
        name_of(id)
            .or_else(|| {
                state
                    .profiles
                    .iter()
                    .find(|card| card.id == id)
                    .map(|card| card.name.clone())
            })
            .unwrap_or_default()
    };
    let top = volume_top(Some(state));
    // SHARE-LIVE: who shares in the room now (only while the room shares).
    let live = |id: &str| state.share.on && state.share.live.iter().any(|share| share.person == id);
    let room = state.room.as_ref().map(|room| VoiceRoomShown {
        id: room.id.clone(),
        kind: room.kind,
        name: if room.general {
            None
        } else {
            room.shown_name(english)
        },
        general: room.general,
        crew: room.crew.as_deref().and_then(&crew_name),
        phase: room.phase,
        max: room.max,
        me: VoiceMeShown {
            role: room.me.role,
            mute: room.me.mute,
            deaf: room.me.deaf,
            muted_by_moderator: room.me.muted_by_moderator,
            talking: speaking.map_or(room.me.talking, |now| now.me),
            quality: room.me.quality,
            needs_key: room.me.needs_key,
        },
        people: room
            .people
            .iter()
            .map(|person| VoicePersonShown {
                id: person.id.clone(),
                name: name(&person.id),
                role: person.role,
                mute: person.mute,
                deaf: person.deaf,
                muted_by_moderator: person.muted_by_moderator,
                quality: person.quality,
                speaking: speaking.map_or(person.speaking, |now| now.ids.contains(&person.id)),
                volume: person.volume.min(top),
                muted_for_me: person.muted_for_me,
                can: VoiceCan {
                    mute: person.can.mute,
                    kick: person.can.kick,
                    moderator: person.can.moderator,
                },
                live: live(&person.id),
            })
            .collect(),
        ringing: room
            .ringing
            .iter()
            .map(|ring| VoicePersonRef {
                id: ring.to.clone(),
                name: name(&ring.to),
                live: false,
            })
            .collect(),
    });
    VoiceView {
        on: state.available && state.enabled,
        room,
        calling: state.calling.as_ref().map(|calling| VoicePersonRef {
            id: calling.friend.clone(),
            name: name(&calling.friend),
            live: false,
        }),
        rings: state
            .rings
            .iter()
            .map(|ring| VoiceRingShown {
                room: ring.room.clone(),
                from: ring.from.clone(),
                name: name(&ring.from),
                with: ring.with.iter().take(4).map(|id| name(id)).collect(),
                until: ring.until.saturating_mul(1000),
            })
            .collect(),
        sound: sound && !state.rings.is_empty(),
        volume_max: (top < MAX_VOLUME).then_some(top),
    }
}

/// A launcher notice as the game's note, or none for the ones the game leaves to the launcher (a
/// missed call: the ring's card said it, and the launcher keeps the list).
pub fn notice_of(
    notice: &VoiceNotice,
    name_of: impl Fn(&str) -> Option<String>,
) -> Option<VoiceNoticeView> {
    let view = |kind, reason| VoiceNoticeView {
        kind,
        reason,
        name: String::new(),
    };
    Some(match notice {
        VoiceNotice::Left { reason, friend, .. } => VoiceNoticeView {
            kind: "left",
            reason: Some(match reason {
                VoiceLeftReason::Closed => "closed",
                VoiceLeftReason::Alone => "alone",
                VoiceLeftReason::NoAnswer => "no-answer",
                VoiceLeftReason::Elsewhere => "elsewhere",
                VoiceLeftReason::Kicked => "kicked",
                VoiceLeftReason::Crew => "crew",
                VoiceLeftReason::Idle => "idle",
                VoiceLeftReason::Unavailable => "unavailable",
                VoiceLeftReason::Failed => "failed",
                // BAN-TYPES: a voice chat ban reads as every mod's "unavailable" in the game; the
                // launcher says the rest.
                VoiceLeftReason::Restricted => "unavailable",
            }),
            name: friend.as_deref().and_then(&name_of).unwrap_or_default(),
        },
        VoiceNotice::Moved { .. } => view("moved", None),
        VoiceNotice::MutedByModerator => view("muted", None),
        VoiceNotice::MicChanged => view("mic-changed", None),
        VoiceNotice::Mic { mic } => match mic {
            VoiceMic::Denied => view("mic", Some("denied")),
            VoiceMic::Missing => view("mic", Some("missing")),
            VoiceMic::Ok | VoiceMic::Unknown => return None,
        },
        VoiceNotice::Missed { .. } => return None,
    })
}

/// A share's kind on the wire.
fn kind_name(kind: ShareKind) -> &'static str {
    match kind {
        ShareKind::Game => "game",
        ShareKind::Screen => "screen",
        ShareKind::Window => "window",
    }
}

/// Screen sharing in the player's room as the game shows it (OVL-SHARE): "Oyunu paylaş" while it
/// may show (`ready`, or `off` with why it is dim), the player's own shares with how many watch,
/// and who else shares. Nothing while voice chat may not be used, out of a room, or while the
/// room does not share.
pub fn share_of(state: &VoiceState) -> VoiceShareShown {
    let share: &VoiceShareView = &state.share;
    if !(state.available && state.enabled) || state.room.is_none() || !share.on {
        return VoiceShareShown::default();
    }
    let mine: Vec<VoiceMyShareShown> = share
        .mine
        .iter()
        .map(|mine| VoiceMyShareShown {
            n: mine.n,
            kind: kind_name(mine.kind),
            live: mine.phase != ShareMinePhase::Starting,
            watchers: u16::try_from(mine.watchers.len()).unwrap_or(u16::MAX),
            th: mine.th,
            st: st_name(mine.st),
            hold: mine.hold.map(hold_name),
        })
        .collect();
    let game_shared = share.mine.iter().any(|mine| mine.kind == ShareKind::Game);
    let (game, why) = if !share.can_share || game_shared {
        (None, None)
    } else if share.mine_max == 0 {
        // The room lets nobody share now (a limit): not "Kim paylaşabilir".
        (Some("off"), Some("closed"))
    } else if !share.allowed {
        (Some("off"), Some("not-allowed"))
    } else if !share.open {
        (Some("off"), Some("closed"))
    } else if share.mine.len() >= usize::from(share.mine_max) {
        (Some("off"), Some("full"))
    } else {
        (Some("ready"), None)
    };
    VoiceShareShown {
        game,
        why,
        mine,
        live: share
            .live
            .iter()
            .map(|live| VoiceLiveShown {
                person: live.person.clone(),
                kind: kind_name(live.kind),
                n: live.n,
                watchers: live.watchers,
                th: live.th,
                st: st_name(live.st),
            })
            .collect(),
        edit: share.edit,
    }
}

/// SQOL-O: what a share's watchers see, on the wire (live: nothing).
fn st_name(st: ShareSt) -> Option<&'static str> {
    match st {
        ShareSt::Live => None,
        ShareSt::Hold => Some("hold"),
        ShareSt::Min => Some("min"),
    }
}

/// SQOL-O: why the player's share holds a blank picture, on the wire.
fn hold_name(why: ShareHoldWhy) -> &'static str {
    match why {
        ShareHoldWhy::Paused => "paused",
        ShareHoldWhy::Closed => "closed",
        ShareHoldWhy::GameClosed => "game-closed",
    }
}

/// SHARE-LIVE: the player in `voice-thumb` (as in `avatar-want`).
pub const ME: &str = "@me";

/// SHARE-LIVE: whether the game may ask for the still picture of share `n` of `u` in `room`: a
/// share in the player's own room (`voice-share` listed it; the player's own as [`ME`]), or
/// someone live in one of the crews' rooms `voice-crews` showed. Anything else is asked of nobody.
pub fn thumb_allowed(state: &VoiceState, room: &str, u: &str, n: u8) -> bool {
    if !(state.available && state.enabled) || n == 0 {
        return false;
    }
    let in_room = state.room.as_ref().is_some_and(|mine| mine.id == room)
        && state.share.on
        && if u == ME {
            state.share.mine.iter().any(|share| share.n == n)
        } else {
            state
                .share
                .live
                .iter()
                .any(|share| share.person == u && share.n == n)
        };
    let in_crew_room = state
        .crews
        .iter()
        .flat_map(|crew| crew.rooms.iter())
        .any(|crew_room| crew_room.id == room && crew_room.live.iter().any(|id| id == u));
    in_room || in_crew_room
}

/// SHARE-LIVE: the answer to `voice-thumb` (a picture becomes a PNG the mod can draw: Minecraft's
/// own image reader takes PNG only; one that does not read is none).
pub fn thumb_frame(room: String, u: String, n: u8, answer: &pl_social::VoiceThumb) -> ToGame {
    use base64::Engine as _;
    let (kind, png) = match answer {
        pl_social::VoiceThumb::Image(jpg) => match png_of(jpg) {
            Some(png) => (
                "image",
                Some(base64::engine::general_purpose::STANDARD.encode(png)),
            ),
            None => ("none", None),
        },
        pl_social::VoiceThumb::Locked => ("locked", None),
        pl_social::VoiceThumb::None => ("none", None),
    };
    ToGame::VoiceThumb {
        room,
        u,
        n,
        kind,
        png,
    }
}

/// A still picture (JPEG, at most 24 KB and 320 px on its long side) as PNG; none when it does
/// not read or is larger than a picture the room keeps.
fn png_of(jpg: &[u8]) -> Option<Vec<u8>> {
    use image::ImageEncoder as _;
    if !pl_social::VoiceThumb::is_picture(jpg) {
        return None;
    }
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(jpg), image::ImageFormat::Jpeg);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(640);
    limits.max_image_height = Some(640);
    limits.max_alloc = Some(4 << 20);
    reader.limits(limits);
    let picture = reader.decode().ok()?.into_rgb8();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            picture.as_raw(),
            picture.width(),
            picture.height(),
            image::ExtendedColorType::Rgb8,
        )
        .ok()?;
    Some(png)
}

/// Cards keep their words to themselves now (OVL-SHARE): the player shares the game or a whole
/// screen (the game is in the picture) and hides notifications while sharing (`quiet`,
/// "Paylaşırken bildirimleri gizle", on unless they turned it off). A window's share shows no
/// game.
pub fn private(state: Option<&VoiceState>, quiet: bool) -> bool {
    quiet
        && state.is_some_and(|state| {
            state
                .share
                .mine
                .iter()
                .any(|mine| matches!(mine.kind, ShareKind::Game | ShareKind::Screen))
        })
}

/// A card while the game is shared ([`private`]; OVL-SHARE), or none: what waits for an answer (a
/// message, an invite, a world invite, a crew's call, a plan) comes without whose it is and what it
/// says (the mod's "Biri", "Yeni mesaj"), so the player still sees something waits and opens it in
/// the panel; the stream's watchers learn nothing. Every other card names someone or says
/// something of the player's own (a request, a friend online, a crew's news, a missed call, an AFK
/// notice): it stays away while the share runs (the launcher's lists keep them).
pub fn keep_private(mut toast: ToastView) -> Option<ToastView> {
    match toast.kind {
        ToastKind::Message
        | ToastKind::Invite
        | ToastKind::World
        | ToastKind::Call
        | ToastKind::Plan => {
            toast.title.clear();
            toast.body.clear();
            toast.crew = None;
            toast.sticker = None;
            toast.badge = None;
            toast.marks.clear();
            toast.by = None;
            Some(toast)
        }
        _ => None,
    }
}

/// A screen sharing moment as the game's note (OVL-SHARE), or none for the ones the game leaves
/// to the launcher (watching happens there; leaving the room says itself).
pub fn share_notice_of(notice: &ShareNotice) -> Option<VoiceNoticeView> {
    let view = |kind, reason| VoiceNoticeView {
        kind,
        reason,
        name: String::new(),
    };
    Some(match notice {
        ShareNotice::Refused { refusal, .. } => view(
            "share-refused",
            Some(match refusal {
                ShareRefusal::Off => "off",
                ShareRefusal::Unsupported => "unsupported",
                ShareRefusal::NotAllowed => "not-allowed",
                ShareRefusal::Limited => "limited",
                ShareRefusal::Full => "full",
                ShareRefusal::RoomFull => "room-full",
                ShareRefusal::Budget => "budget",
                ShareRefusal::Failed => "failed",
                // SQOL: `inUse` is the picker's own (it never offers such a source).
                ShareRefusal::WatchFull
                | ShareRefusal::WatchMax
                | ShareRefusal::Gone
                | ShareRefusal::InUse => {
                    return None;
                }
            }),
        ),
        ShareNotice::Closed { why, src } => view(
            "share-closed",
            Some(match why {
                ShareClosedWhy::Mod => "mod",
                ShareClosedWhy::Budget => "budget",
                ShareClosedWhy::Idle => "idle",
                ShareClosedWhy::Limit => "limit",
                ShareClosedWhy::SourceGone if *src == ShareKind::Game => "game-gone",
                ShareClosedWhy::SourceGone => "source-gone",
                ShareClosedWhy::Network => "network",
                ShareClosedWhy::Error => "error",
                ShareClosedWhy::Left => return None,
            }),
        ),
        ShareNotice::AudioOff => view("share-audio-off", None),
        // SQOL: a switch and a closed source's wait are the launcher's (its card offers the
        // choices); the game hears the end as `share-closed` when it comes.
        ShareNotice::WatchStopped
        | ShareNotice::WatchFailed
        | ShareNotice::Switched { .. }
        | ShareNotice::SwitchFailed { .. }
        | ShareNotice::SourceClosed { .. } => return None,
    })
}

/// What the live mod was told of voice chat, for the next differences.
#[derive(Debug, Default)]
pub struct VoiceMirror {
    view: Option<VoiceView>,
    /// The crews' rooms the mod was told (V21), none before the first.
    crews: Option<Vec<VoiceCrewShown>>,
    /// Screen sharing the mod was told (OVL-SHARE), none before the first.
    share: Option<VoiceShareShown>,
}

impl VoiceMirror {
    /// `voice-share` when screen sharing differs from what the mod has (OVL-SHARE). A mod that
    /// heard none hears nothing while there is nothing to show; once it heard some, an empty one
    /// says it is gone.
    pub fn update_share(&mut self, share: VoiceShareShown) -> Option<ToGame> {
        if self.share.as_ref() == Some(&share) {
            return None;
        }
        if self.share.is_none() && share.is_empty() {
            self.share = Some(share);
            return None;
        }
        self.share = Some(share.clone());
        Some(ToGame::VoiceShare(share))
    }

    /// `voice-crews` when the crews' rooms differ from what the mod has (V21). A mod that heard
    /// none hears nothing while there is nothing to show (voice chat off, or the player in no
    /// crew); once it heard some, an empty list says they are gone.
    pub fn update_crews(&mut self, crews: Vec<VoiceCrewShown>) -> Option<ToGame> {
        if self.crews.as_ref() == Some(&crews) {
            return None;
        }
        if self.crews.is_none() && crews.is_empty() {
            self.crews = Some(crews);
            return None;
        }
        self.crews = Some(crews.clone());
        Some(ToGame::VoiceCrews { crews })
    }

    /// `voice` when `view` differs from what the mod has. While voice chat has nothing to show
    /// (off, no room, no ring) a mod that heard nothing of it hears nothing: its settings show no
    /// voice chat row for a feature that is not on.
    pub fn update(&mut self, view: VoiceView) -> Option<ToGame> {
        if self.view.as_ref() == Some(&view) {
            return None;
        }
        if self.view.is_none() && view.is_empty() {
            self.view = Some(view);
            return None;
        }
        self.view = Some(view.clone());
        Some(ToGame::Voice(Box::new(view)))
    }

    /// `voice-speaking` when the speakers changed, only while the player is in a room (someone
    /// who is not in it any more is left out). The speakers stand in the mirror too, so the next
    /// whole `voice` that only says the same is not sent.
    pub fn speaking(&mut self, speaking: &VoiceSpeaking) -> Option<ToGame> {
        let room = self.view.as_mut()?.room.as_mut()?;
        let mut changed = room.me.talking != speaking.me;
        room.me.talking = speaking.me;
        for person in &mut room.people {
            let now = speaking.ids.contains(&person.id);
            changed |= person.speaking != now;
            person.speaking = now;
        }
        changed.then(|| ToGame::VoiceSpeaking {
            ids: room
                .people
                .iter()
                .filter(|person| person.speaking)
                .map(|person| person.id.clone())
                .collect(),
            me: room.me.talking,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use pl_voice::view::{
        VoiceCalling, VoiceMe, VoiceOutRing, VoicePerson, VoicePhase, VoicePowers, VoiceQuality,
        VoiceRing, VoiceRole, VoiceRoomKind, VoiceRoomView,
    };
    use serde_json::json;

    use super::*;

    fn person(id: &str) -> VoicePerson {
        VoicePerson {
            id: id.into(),
            role: VoiceRole::Member,
            mute: false,
            deaf: false,
            muted_by_moderator: false,
            quality: VoiceQuality::Good,
            speaking: false,
            volume: 100,
            muted_for_me: false,
            joined_at: 1_800_000_000,
            can: VoicePowers::default(),
        }
    }

    pub(crate) fn in_room() -> VoiceState {
        let mut kaan = person("u-kaan");
        kaan.role = VoiceRole::Leader;
        kaan.speaking = true;
        kaan.volume = 250;
        let mut stranger = person("u-x");
        stranger.mute = true;
        stranger.quality = VoiceQuality::Weak;
        stranger.muted_for_me = true;
        stranger.can = VoicePowers {
            mute: true,
            kick: true,
            move_to: true,
            moderator: false,
        };
        VoiceState {
            available: true,
            enabled: true,
            room: Some(VoiceRoomView {
                id: "f0123456789abcdef".into(),
                kind: VoiceRoomKind::Friend,
                crew: None,
                name: None,
                general: false,
                max: 10,
                phase: VoicePhase::Live,
                me: VoiceMe {
                    id: "me".into(),
                    role: VoiceRole::Moderator,
                    mute: false,
                    deaf: true,
                    muted_by_moderator: false,
                    talking: false,
                    quality: VoiceQuality::Good,
                    needs_key: false,
                },
                people: vec![kaan, stranger],
                ringing: vec![VoiceOutRing {
                    to: "f-can".into(),
                    until: 1_800_000_030,
                }],
                can_rename: false,
                can_close: false,
                ptt: false,
                chat: false,
                grew: false,
            }),
            calling: Some(VoiceCalling {
                friend: "f-can".into(),
                until: 1_800_000_030,
            }),
            rings: vec![VoiceRing {
                room: "c0123456789abcdef".into(),
                from: "f-ayse".into(),
                with: vec!["u-kaan".into(), "u-x".into()],
                until: 1_800_000_020,
            }],
            ..VoiceState::off()
        }
    }

    fn names(id: &str) -> Option<String> {
        match id {
            "u-kaan" => Some("Kaan_07".into()),
            "f-ayse" => Some("Ayşe".into()),
            "f-can" => Some("Can".into()),
            _ => None,
        }
    }

    fn crews(id: &str) -> Option<String> {
        (id == "t-gk").then(|| "Gece Kuşları".to_owned())
    }

    #[test]
    fn the_room_goes_with_names_and_only_what_is_set() {
        let view = view_of(&in_room(), true, None, false, names, crews);
        assert_eq!(
            serde_json::to_value(ToGame::Voice(Box::new(view))).unwrap(),
            json!({
                "t": "voice", "on": true, "sound": true,
                "room": {
                    "id": "f0123456789abcdef", "kind": "friend", "phase": "live", "max": 10,
                    "me": {"role": "moderator", "deaf": true},
                    "people": [
                        {"id": "u-kaan", "name": "Kaan_07", "role": "leader", "speaking": true,
                         "volume": 200},
                        {"id": "u-x", "role": "member", "mute": true, "quality": "weak",
                         "volume": 100, "mutedForMe": true, "can": {"mute": true, "kick": true}},
                    ],
                    "ringing": [{"id": "f-can", "name": "Can"}],
                },
                "calling": {"id": "f-can", "name": "Can"},
                "rings": [{"room": "c0123456789abcdef", "from": "f-ayse", "name": "Ayşe",
                           "with": ["Kaan_07", ""], "until": 1_800_000_020_000_i64}],
            })
        );
    }

    #[test]
    fn a_friends_friend_is_named_from_the_rooms_cards_and_volumes_keep_to_the_pages_top() {
        let card = |id: &str, name: &str| pl_voice::view::VoiceProfile {
            id: id.into(),
            name: name.into(),
            avatar: "preset:fox".into(),
            avatar_url: None,
            marks: Vec::new(),
        };
        let mut state = in_room();
        state.profiles = vec![card("u-x", "Zeyno_x"), card("u-kaan", "NotKaan")];
        state.volume_max = 100;
        let view = serde_json::to_value(ToGame::Voice(Box::new(view_of(
            &state, false, None, false, names, crews,
        ))))
        .unwrap();
        let people = &view["room"]["people"];
        assert_eq!(
            people[0]["name"], "Kaan_07",
            "the friend list's own name first"
        );
        assert_eq!(people[1]["name"], "Zeyno_x");
        assert_eq!(view["rings"][0]["with"], json!(["Kaan_07", "Zeyno_x"]));
        assert_eq!(people[0]["volume"], 100, "held to the page's top");
        assert_eq!(view["volumeMax"], 100);
        // A press past it is held there too.
        let press = FromGame::VoiceVol {
            person: "u-x".into(),
            percent: 150,
        };
        assert_eq!(
            check(&press, Some(&state), |_| false, |_| false)
                .unwrap()
                .unwrap(),
            VoiceAct::Volume {
                person: "u-x".into(),
                percent: 100
            }
        );
        // Through WebAudio the top is the slider's own: nothing is said of it.
        state.volume_max = 200;
        assert_eq!(
            view_of(&state, false, None, false, names, crews).volume_max,
            None
        );
    }

    #[test]
    fn a_crew_rooms_general_is_worded_by_the_mod_and_names_its_crew() {
        let mut state = in_room();
        let room = state.room.as_mut().unwrap();
        room.kind = VoiceRoomKind::Crew;
        room.crew = Some("t-gk".into());
        room.name = Some("Genel".into());
        room.general = true;
        state.rings.clear();
        let view = view_of(&state, true, None, false, names, crews);
        let room = view.room.as_ref().unwrap();
        assert_eq!(
            (room.name.as_deref(), room.general, room.crew.as_deref()),
            (None, true, Some("Gece Kuşları"))
        );
        assert!(!view.sound, "nothing rings: nothing to ring for");
        state.room.as_mut().unwrap().general = false;
        state.room.as_mut().unwrap().name = Some("Oda 2".into());
        let view = view_of(&state, false, None, false, names, crews);
        assert_eq!(view.room.unwrap().name.as_deref(), Some("Oda 2"));
        // The service names new crew rooms in Turkish: an English overlay says "Room 2", a name
        // its leader typed stays as typed.
        let view = view_of(&state, false, None, true, names, crews);
        assert_eq!(view.room.unwrap().name.as_deref(), Some("Room 2"));
        state.room.as_mut().unwrap().name = Some("Taş Ocağı".into());
        let view = view_of(&state, false, None, true, names, crews);
        assert_eq!(view.room.unwrap().name.as_deref(), Some("Taş Ocağı"));
        // Voice chat switched off by the player: no "Ara" anywhere.
        let mut off = VoiceState::off();
        off.available = true;
        off.enabled = false;
        assert_eq!(
            serde_json::to_value(ToGame::Voice(Box::new(view_of(
                &off, true, None, false, names, crews
            ))))
            .unwrap(),
            json!({"t": "voice"})
        );
    }

    #[test]
    fn the_mirror_sends_a_room_once_and_speakers_as_they_change() {
        let mut mirror = VoiceMirror::default();
        let speaking = VoiceSpeaking {
            ids: vec!["u-x".into()],
            me: true,
        };
        assert!(
            mirror.speaking(&speaking).is_none(),
            "no room told yet: nothing"
        );
        let view = view_of(&in_room(), false, None, false, names, crews);
        assert!(mirror.update(view.clone()).is_some());
        assert!(mirror.update(view.clone()).is_none(), "the same again");
        assert_eq!(
            serde_json::to_value(mirror.speaking(&speaking).unwrap()).unwrap(),
            json!({"t": "voice-speaking", "ids": ["u-x"], "me": true})
        );
        assert!(mirror.speaking(&speaking).is_none(), "no change");
        // Someone who left (or never came) is not told.
        let ghost = VoiceSpeaking {
            ids: vec!["u-x".into(), "u-gone".into()],
            me: true,
        };
        assert!(mirror.speaking(&ghost).is_none());
        // A whole state that says what the speakers said already goes unsent.
        let mut state = in_room();
        let room = state.room.as_mut().unwrap();
        room.people[0].speaking = false;
        room.people[1].speaking = true;
        room.me.talking = true;
        assert!(
            mirror
                .update(view_of(&state, false, None, false, names, crews))
                .is_none()
        );
        // Nobody speaks: one last frame, then silence.
        let quiet = VoiceSpeaking::default();
        assert_eq!(
            serde_json::to_value(mirror.speaking(&quiet).unwrap()).unwrap(),
            json!({"t": "voice-speaking", "ids": [], "me": false})
        );
        assert!(mirror.speaking(&quiet).is_none());
        // Out of the room: speakers are nobody's business.
        assert!(
            mirror
                .update(view_of(
                    &VoiceState::off(),
                    false,
                    None,
                    false,
                    names,
                    crews
                ))
                .is_some()
        );
        assert!(mirror.speaking(&speaking).is_none());
    }

    #[test]
    fn the_newest_speakers_win_and_nothing_goes_while_there_is_nothing() {
        // Voice chat off from the start: a mod that heard nothing of it hears nothing.
        let mut mirror = VoiceMirror::default();
        assert!(
            mirror
                .update(view_of(
                    &VoiceState::off(),
                    false,
                    None,
                    false,
                    names,
                    crews
                ))
                .is_none()
        );
        // The state says Kaan_07 speaks; a later change says nobody does: a whole view made after
        // it (a name changed) says the later one.
        let quiet = VoiceSpeaking::default();
        let view = view_of(&in_room(), false, Some(&quiet), false, names, crews);
        let room = view.room.as_ref().unwrap();
        assert!(room.people.iter().all(|p| !p.speaking) && !room.me.talking);
        let talking = VoiceSpeaking {
            ids: vec!["u-x".into()],
            me: true,
        };
        let view = view_of(&in_room(), false, Some(&talking), false, names, crews);
        let room = view.room.as_ref().unwrap();
        assert_eq!(
            room.people
                .iter()
                .map(|p| (p.id.as_str(), p.speaking))
                .collect::<Vec<_>>(),
            [("u-kaan", false), ("u-x", true)]
        );
        assert!(room.me.talking);
        assert!(mirror.update(view).is_some(), "a room is something to show");
    }

    #[test]
    fn presses_go_only_for_what_the_state_has() {
        let state = in_room();
        let friends = |id: &str| matches!(id, "f-ayse" | "f-can");
        let frame = |value: serde_json::Value| serde_json::from_value::<FromGame>(value).unwrap();
        let go = |value| check(&frame(value), Some(&state), friends, |_| false).unwrap();
        assert_eq!(
            go(json!({"t":"voice-set","mute":true})),
            Ok(VoiceAct::Mute(true))
        );
        assert_eq!(
            go(json!({"t":"voice-set","deaf":false})),
            Ok(VoiceAct::Deaf(false))
        );
        assert!(go(json!({"t":"voice-set"})).is_err());
        assert_eq!(go(json!({"t":"voice-leave"})), Ok(VoiceAct::Leave));
        assert_eq!(
            go(json!({"t":"voice-answer","room":"c0123456789abcdef","join":true})),
            Ok(VoiceAct::Accept {
                room: "c0123456789abcdef".into()
            })
        );
        assert_eq!(
            go(json!({"t":"voice-answer","room":"c0123456789abcdef","join":false})),
            Ok(VoiceAct::Decline {
                room: "c0123456789abcdef".into()
            })
        );
        // A ring that is over: "Katıl" hears so, "Reddet" does nothing.
        assert_eq!(
            go(json!({"t":"voice-answer","room":"cffffffffffffffff","join":true})),
            Err(Refused::of("join", "", VoiceRefusal::RoomGone))
        );
        assert_eq!(
            go(json!({"t":"voice-answer","room":"cffffffffffffffff"})),
            Err(Refused::quiet())
        );
        // Volumes are held to the slider's ends; only for someone in the room.
        assert_eq!(
            go(json!({"t":"voice-vol","person":"u-x","percent":350})),
            Ok(VoiceAct::Volume {
                person: "u-x".into(),
                percent: 200
            })
        );
        assert_eq!(
            go(json!({"t":"voice-vol","person":"u-x","percent":-4})),
            Ok(VoiceAct::Volume {
                person: "u-x".into(),
                percent: 0
            })
        );
        assert!(go(json!({"t":"voice-vol","person":"f-ayse","percent":50})).is_err());
        assert_eq!(
            go(json!({"t":"voice-lmute","person":"u-kaan","on":true})),
            Ok(VoiceAct::MuteForMe {
                person: "u-kaan".into(),
                on: true
            })
        );
        assert!(go(json!({"t":"voice-lmute","person":"nobody","on":true})).is_err());
        // Moderation: the room decides the role; here only who and what.
        assert_eq!(
            go(json!({"t":"voice-mod","person":"u-x","act":"kick"})),
            Ok(VoiceAct::Moderate(VoiceModAction::Kick {
                person: "u-x".into()
            }))
        );
        assert_eq!(
            go(json!({"t":"voice-mod","person":"u-gone","act":"unkick"})),
            Ok(VoiceAct::Moderate(VoiceModAction::Unkick {
                person: "u-gone".into()
            })),
            "the undo names someone who is out"
        );
        assert_eq!(
            go(json!({"t":"voice-mod","person":"u-gone","act":"mute"})),
            Err(Refused::of("mute", "u-gone", VoiceRefusal::NotFound))
        );
        assert_eq!(
            go(json!({"t":"voice-mod","person":"u-x","act":"close"})),
            Err(Refused::quiet()),
            "closing and moving stay in the launcher"
        );
        // Calls and invites: friends only; an invite needs a room.
        assert_eq!(
            go(json!({"t":"voice-call","friend":"f-ayse"})),
            Ok(VoiceAct::Call {
                friend: "f-ayse".into()
            })
        );
        assert_eq!(
            go(json!({"t":"voice-call","friend":"u-kaan"})),
            Err(Refused::of("call", "u-kaan", VoiceRefusal::Unreachable))
        );
        assert_eq!(
            go(json!({"t":"voice-invite","friend":"f-can"})),
            Ok(VoiceAct::Invite {
                friend: "f-can".into()
            })
        );
        let invite = frame(json!({"t":"voice-invite","friend":"f-can"}));
        assert_eq!(
            check(&invite, Some(&VoiceState::off()), friends, |_| false),
            Some(Err(Refused::of("invite", "f-can", VoiceRefusal::Nothing)))
        );
        assert_eq!(
            check(&frame(json!({"t":"voice-leave"})), None, friends, |_| false),
            Some(Ok(VoiceAct::Leave))
        );
        assert_eq!(
            check(
                &frame(json!({"t":"panel","open":true})),
                None,
                friends,
                |_| false
            ),
            None
        );
    }

    #[test]
    fn results_and_notices_use_the_wire_names() {
        let call = VoiceAct::Call {
            friend: "f-ayse".into(),
        };
        let refused = result_of(&call, &VoiceAnswer::refused(VoiceRefusal::Unreachable)).unwrap();
        assert_eq!(
            serde_json::to_value(refused).unwrap(),
            json!({"t":"voice-result","act":"call","ok":false,"reason":"unreachable","who":"f-ayse"})
        );
        let kick = VoiceAct::Moderate(VoiceModAction::Kick {
            person: "u-x".into(),
        });
        assert_eq!(
            serde_json::to_value(result_of(&kick, &VoiceAnswer::OK).unwrap()).unwrap(),
            json!({"t":"voice-result","act":"kick","ok":true,"who":"u-x"})
        );
        assert!(result_of(&VoiceAct::Mute(true), &VoiceAnswer::OK).is_none());
        assert_eq!(
            serde_json::to_value(
                refused_result(&Refused::of("join", "", VoiceRefusal::RoomGone)).unwrap()
            )
            .unwrap(),
            json!({"t":"voice-result","act":"join","ok":false,"reason":"room-gone"})
        );
        assert!(refused_result(&Refused::quiet()).is_none());
        let notice = |notice: VoiceNotice| {
            notice_of(&notice, names)
                .map(|view| serde_json::to_value(ToGame::VoiceNotice(view)).unwrap())
        };
        assert_eq!(
            notice(VoiceNotice::Left {
                reason: VoiceLeftReason::NoAnswer,
                friend: Some("f-can".into()),
                until: None
            }),
            Some(json!({"t":"voice-notice","kind":"left","reason":"no-answer","name":"Can"}))
        );
        assert_eq!(
            notice(VoiceNotice::Left {
                reason: VoiceLeftReason::Kicked,
                friend: None,
                until: None
            }),
            Some(json!({"t":"voice-notice","kind":"left","reason":"kicked"}))
        );
        assert_eq!(
            notice(VoiceNotice::MutedByModerator),
            Some(json!({"t":"voice-notice","kind":"muted"}))
        );
        assert_eq!(
            notice(VoiceNotice::Mic {
                mic: VoiceMic::Denied
            }),
            Some(json!({"t":"voice-notice","kind":"mic","reason":"denied"}))
        );
        assert_eq!(
            notice(VoiceNotice::MicChanged),
            Some(json!({"t":"voice-notice","kind":"mic-changed"}))
        );
        assert_eq!(
            notice(VoiceNotice::Moved {
                room: "c0123456789abcdef".into(),
                name: "Oda 2".into()
            }),
            Some(json!({"t":"voice-notice","kind":"moved"}))
        );
        assert_eq!(
            notice(VoiceNotice::Missed {
                from: "f-ayse".into(),
                room: "c0123456789abcdef".into()
            }),
            None,
            "the ring's card said it; the launcher keeps the list"
        );
    }

    /// The player's crew "t-gk" with two live rooms (OVL-SOC).
    fn crew_state() -> VoiceState {
        use pl_voice::view::{VoiceCrewRoom, VoiceCrewRooms};
        let mut state = in_room();
        state.rings.clear();
        state.calling = None;
        state.crews = vec![VoiceCrewRooms {
            crew: "t-gk".into(),
            rooms: vec![
                VoiceCrewRoom {
                    id: "c0123456789abcdef".into(),
                    name: "Genel".into(),
                    general: true,
                    people: vec!["u-kaan".into(), "u-x".into()],
                    max: None,
                    own: false,
                    live: Vec::new(),
                    ptt: false,
                },
                VoiceCrewRoom {
                    id: "c1111111111111111".into(),
                    name: "Oda 2".into(),
                    general: false,
                    people: vec!["f-ayse".into()],
                    max: None,
                    own: false,
                    live: Vec::new(),
                    ptt: false,
                },
            ],
            can_add: true,
            manage: false,
            crew_max: 15,
            ptt: false,
        }];
        state
    }

    #[test]
    fn crews_rooms_go_in_the_launchers_order_and_an_empty_crew_shows_its_general() {
        let state = crew_state();
        let crews = crews_of(&state, ["t-gk", "t-empty"], false, names);
        assert_eq!(
            serde_json::to_value(ToGame::VoiceCrews {
                crews: crews.clone()
            })
            .unwrap(),
            json!({"t": "voice-crews", "crews": [
                {"crew": "t-gk", "conv": "c:t-gk", "add": true, "rooms": [
                    {"id": "c0123456789abcdef", "general": true,
                     "people": [{"id": "u-kaan", "name": "Kaan_07"}, {"id": "u-x"}]},
                    {"id": "c1111111111111111", "name": "Oda 2",
                     "people": [{"id": "f-ayse", "name": "Ayşe"}]},
                ]},
                {"crew": "t-empty", "conv": "c:t-empty", "add": true, "rooms": [
                    {"id": "gt-empty", "general": true},
                ]},
            ]})
        );
        // In English the friends service's "Oda 2" is "Room 2"; the mod says "Genel" itself.
        let english = crews_of(&state, ["t-gk"], true, names);
        assert_eq!(english[0].rooms[1].name.as_deref(), Some("Room 2"));
        assert_eq!(english[0].rooms[0].name, None);
        // A crew that may have one room only: no "Yeni oda".
        let mut one = state.clone();
        one.crew_rooms_max = 1;
        assert!(!crews_of(&one, ["t-empty"], false, names)[0].add);
        // Voice chat off: nothing at all.
        let mut off = state.clone();
        off.enabled = false;
        assert!(crews_of(&off, ["t-gk"], false, names).is_empty());
    }

    #[test]
    fn a_crew_room_with_its_own_limit_says_how_full_it_is() {
        let mut state = crew_state();
        // OVL-SOC 2 (VOICE-ROOMS): "Oda 2" takes one person, Genel the crew's fifteen.
        state.crews[0].rooms[1].max = Some(1);
        let crews = crews_of(&state, ["t-gk"], false, names);
        let rooms = serde_json::to_value(&crews[0].rooms).unwrap();
        assert_eq!(rooms[1]["max"], 1);
        assert_eq!(rooms[1]["full"], true);
        assert!(rooms[0].get("max").is_none() && rooms[0].get("full").is_none());
        // A limit at the crew's maximum is no limit of its own (no "15/15" pill).
        state.crews[0].rooms[1].max = Some(15);
        let crews = crews_of(&state, ["t-gk"], false, names);
        assert_eq!(
            (crews[0].rooms[1].max, crews[0].rooms[1].full),
            (None, false)
        );
        // The crew's maximum fills a room without its own limit.
        state.crews[0].crew_max = 2;
        state.crews[0].rooms[1].max = None;
        let crews = crews_of(&state, ["t-gk"], false, names);
        assert!(crews[0].rooms[0].full && !crews[0].rooms[1].full);
    }

    #[test]
    fn crew_joins_go_only_into_rooms_the_crews_showed() {
        let state = crew_state();
        let frame = |value: serde_json::Value| serde_json::from_value::<FromGame>(value).unwrap();
        let crews = |id: &str| matches!(id, "t-gk" | "t-empty");
        let go = |state: Option<&VoiceState>, value| {
            check(&frame(value), state, |_| false, crews).unwrap()
        };
        let join = |crew: &str, room: Option<&str>, new: bool| {
            Ok(VoiceAct::JoinCrew {
                crew: crew.into(),
                room: room.map(str::to_owned),
                new,
            })
        };
        assert_eq!(
            go(
                Some(&state),
                json!({"t":"voice-join","crew":"t-gk","room":"c1111111111111111"})
            ),
            join("t-gk", Some("c1111111111111111"), false)
        );
        // Nobody in any room of the crew: its "Genel" by its own id, nothing else.
        assert_eq!(
            go(
                Some(&state),
                json!({"t":"voice-join","crew":"t-empty","room":"gt-empty"})
            ),
            join("t-empty", Some("gt-empty"), false)
        );
        assert_eq!(
            go(
                Some(&state),
                json!({"t":"voice-join","crew":"t-gk","room":"c2222222222222222"})
            ),
            Err(Refused::of("join", "", VoiceRefusal::RoomGone))
        );
        // "Yeni oda".
        assert_eq!(
            go(
                Some(&state),
                json!({"t":"voice-join","crew":"t-gk","new":true})
            ),
            join("t-gk", None, true)
        );
        // Not one of the player's crews.
        assert_eq!(
            go(
                Some(&state),
                json!({"t":"voice-join","crew":"t-x","room":"gt-x"})
            ),
            Err(Refused::of("join", "", VoiceRefusal::NotMember))
        );
        // The room the player is in already: nothing to do, nothing to say.
        let mut here = state.clone();
        here.room.as_mut().unwrap().id = "c1111111111111111".into();
        assert_eq!(
            go(
                Some(&here),
                json!({"t":"voice-join","crew":"t-gk","room":"c1111111111111111"})
            ),
            Err(Refused::quiet())
        );
        // Voice chat off, or never heard of.
        let mut off = state.clone();
        off.available = false;
        let press = json!({"t":"voice-join","crew":"t-gk","room":"c1111111111111111"});
        assert_eq!(
            go(Some(&off), press.clone()),
            Err(Refused::of("join", "", VoiceRefusal::Off))
        );
        assert_eq!(
            go(None, press),
            Err(Refused::of("join", "", VoiceRefusal::Off))
        );
        // Its result is the ring's "Katıl"'s.
        let act = VoiceAct::JoinCrew {
            crew: "t-gk".into(),
            room: None,
            new: true,
        };
        assert_eq!(
            serde_json::to_value(
                result_of(&act, &VoiceAnswer::refused(VoiceRefusal::RoomsFull)).unwrap()
            )
            .unwrap(),
            json!({"t":"voice-result","act":"join","ok":false,"reason":"rooms-full"})
        );
    }

    #[test]
    fn the_mirror_tells_the_crews_rooms_when_they_change_only() {
        let mut mirror = VoiceMirror::default();
        assert!(
            mirror.update_crews(Vec::new()).is_none(),
            "nothing to show a mod that heard nothing"
        );
        let crews = crews_of(&crew_state(), ["t-gk"], false, names);
        assert!(mirror.update_crews(crews.clone()).is_some());
        assert!(mirror.update_crews(crews).is_none(), "the same again");
        assert_eq!(
            serde_json::to_value(mirror.update_crews(Vec::new()).unwrap()).unwrap(),
            json!({"t": "voice-crews", "crews": []}),
            "gone: said once"
        );
        assert!(mirror.update_crews(Vec::new()).is_none());
    }

    /// The player in a room that shares (OVL-SHARE): Kaan_07 shares a window.
    fn sharing_state() -> VoiceState {
        use pl_voice::share::{SharePreset, VoiceLiveShare};
        let mut state = in_room();
        state.share.on = true;
        state.share.can_share = true;
        state.share.allowed = true;
        state.share.open = true;
        state.share.mine_max = 2;
        state.share.live = vec![VoiceLiveShare {
            person: "u-kaan".into(),
            n: 1,
            kind: ShareKind::Window,
            audio: true,
            preset: SharePreset::Normal,
            fps: 30,
            watchers: 1,
            since: 1_800_000_000,
            th: Some(1_800_000_060),
            st: pl_voice::share::ShareSt::Live,
        }];
        state
    }

    fn my_share(
        n: u8,
        kind: ShareKind,
        phase: ShareMinePhase,
        watchers: usize,
    ) -> pl_voice::share::VoiceMyShare {
        pl_voice::share::VoiceMyShare {
            n,
            kind,
            label: String::new(),
            phase,
            audio: true,
            preset: pl_voice::share::SharePreset::Normal,
            fps: 30,
            watchers: (0..watchers).map(|i| format!("u-{i}")).collect(),
            since: 1_800_000_000,
            th: None,
            source: pl_voice::share::ShareSource::Game,
            st: pl_voice::share::ShareSt::Live,
            hold: None,
            hold_until: None,
            motion: pl_voice::share::ShareMotion::Auto,
            cursor: true,
            thumbs: true,
        }
    }

    /// SHARE-LIVE: who shares shows on the room's and the crew rooms' rows; the hover card's
    /// picture is asked for only for a share the state shows, and comes as a PNG.
    #[test]
    fn live_rows_and_the_hover_cards_picture() {
        use base64::Engine as _;
        let mut state = sharing_state();
        let view = view_of(&state, false, None, false, names, |_| None);
        let room = view.room.unwrap();
        assert!(room.people.iter().all(|p| p.live == (p.id == "u-kaan")));
        let row = |id: &str| {
            serde_json::to_value(room.people.iter().find(|p| p.id == id).unwrap()).unwrap()
        };
        assert_eq!(row("u-kaan")["live"], json!(true));
        assert!(row("u-x").get("live").is_none());
        // Crew rooms: their live lists' people.
        state.crews = crew_state().crews;
        state.crews[0].rooms[1].live = vec!["f-ayse".into()];
        let crews = crews_of(&state, ["t-gk"], false, names);
        assert!(crews[0].rooms[1].people[0].live);
        assert!(crews[0].rooms[0].people.iter().all(|p| !p.live));
        // The picture: a share in the player's room (theirs as `@me`), or someone live in a crew
        // room the game was shown.
        let room_id = state.room.as_ref().unwrap().id.clone();
        assert!(thumb_allowed(&state, &room_id, "u-kaan", 1));
        assert!(!thumb_allowed(&state, &room_id, "u-kaan", 2));
        assert!(!thumb_allowed(&state, &room_id, "u-x", 1));
        assert!(!thumb_allowed(&state, &room_id, ME, 1));
        state.share.mine = vec![my_share(1, ShareKind::Game, ShareMinePhase::Live, 0)];
        assert!(thumb_allowed(&state, &room_id, ME, 1));
        assert!(thumb_allowed(&state, "c1111111111111111", "f-ayse", 1));
        assert!(!thumb_allowed(&state, "c0123456789abcdef", "f-ayse", 1));
        state.share.on = false;
        assert!(!thumb_allowed(&state, &room_id, "u-kaan", 1));
        // The answer: a PNG the game can draw, the lock, or nothing.
        let mut jpg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 70)
            .encode(&[90u8; 32 * 18 * 3], 32, 18, image::ExtendedColorType::Rgb8)
            .unwrap();
        let asked: FromGame =
            serde_json::from_value(json!({"t": "voice-thumb", "room": "r", "u": "u-kaan", "n": 1}))
                .unwrap();
        assert_eq!(
            asked,
            FromGame::VoiceThumb {
                room: "r".into(),
                u: "u-kaan".into(),
                n: 1
            }
        );
        let frame = serde_json::to_value(thumb_frame(
            "r".into(),
            "u-kaan".into(),
            1,
            &pl_social::VoiceThumb::Image(jpg),
        ))
        .unwrap();
        assert_eq!(
            (frame["t"].as_str(), frame["kind"].as_str()),
            (Some("voice-thumb"), Some("image"))
        );
        let png = base64::engine::general_purpose::STANDARD
            .decode(frame["png"].as_str().unwrap())
            .unwrap();
        let picture = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap();
        assert_eq!((picture.width(), picture.height()), (32, 18));
        assert_eq!(
            serde_json::to_value(thumb_frame(
                "r".into(),
                "u".into(),
                1,
                &pl_social::VoiceThumb::Locked
            ))
            .unwrap(),
            json!({"t": "voice-thumb", "room": "r", "u": "u", "n": 1, "kind": "locked"})
        );
        // Something that is no picture is none.
        let broken = thumb_frame(
            "r".into(),
            "u".into(),
            1,
            &pl_social::VoiceThumb::Image(vec![0xFF, 0xD8, 0xFF, 0]),
        );
        assert_eq!(serde_json::to_value(broken).unwrap()["kind"], "none");
    }

    #[test]
    fn sharing_shows_who_is_live_the_game_button_and_the_players_own_shares() {
        let mut state = sharing_state();
        // SHARE-LIVE: with the share's number, its watchers and its picture's time.
        assert_eq!(
            serde_json::to_value(ToGame::VoiceShare(share_of(&state))).unwrap(),
            json!({"t": "voice-share", "game": "ready",
                   "live": [{"person": "u-kaan", "kind": "window", "n": 1, "watchers": 1,
                             "th": 1_800_000_060_i64}]})
        );
        // The game shared: no "Oyunu paylaş", the share with how many watch.
        state.share.mine = vec![my_share(1, ShareKind::Game, ShareMinePhase::Live, 2)];
        state.share.mine[0].th = Some(1_800_000_030);
        assert_eq!(
            serde_json::to_value(share_of(&state)).unwrap(),
            json!({"mine": [{"n": 1, "kind": "game", "live": true, "watchers": 2,
                             "th": 1_800_000_030_i64}],
                   "live": [{"person": "u-kaan", "kind": "window", "n": 1, "watchers": 1,
                             "th": 1_800_000_060_i64}]})
        );
        // A window from the launcher's picker: the game may still go, while one more may.
        state.share.mine = vec![my_share(2, ShareKind::Window, ShareMinePhase::Starting, 0)];
        let share = share_of(&state);
        assert_eq!((share.game, share.why), (Some("ready"), None));
        assert!(!share.mine[0].live);
        state.share.mine_max = 1;
        assert_eq!(share_of(&state).why, Some("full"));
        state.share.mine_max = 0;
        assert_eq!(share_of(&state).why, Some("closed"), "nobody may share now");
        state.share.mine_max = 2;
        // Dim with why: the room's "Kim paylaşabilir", the month's budget.
        state.share.mine.clear();
        state.share.allowed = false;
        assert_eq!(
            (share_of(&state).game, share_of(&state).why),
            (Some("off"), Some("not-allowed"))
        );
        state.share.allowed = true;
        state.share.open = false;
        assert_eq!(share_of(&state).why, Some("closed"));
        // This computer cannot share: watching only, no button.
        state.share.can_share = false;
        assert_eq!(share_of(&state).game, None);
        // The room does not share, or no room: nothing at all.
        state.share.on = false;
        assert!(share_of(&state).is_empty());
        let mut out = sharing_state();
        out.room = None;
        assert!(share_of(&out).is_empty());
    }

    #[test]
    fn sharing_presses_go_only_while_they_show() {
        let frame = |value: serde_json::Value| serde_json::from_value::<FromGame>(value).unwrap();
        let go = |state: &VoiceState, value| {
            check(&frame(value), Some(state), |_| false, |_| false).unwrap()
        };
        let mut state = sharing_state();
        assert_eq!(
            go(&state, json!({"t": "voice-share-game"})),
            Ok(VoiceAct::ShareGame)
        );
        assert!(frame(json!({"t": "voice-share-game"})).is_voice());
        state.share.allowed = false;
        assert_eq!(
            go(&state, json!({"t": "voice-share-game"})),
            Err(Refused::of("share", "", VoiceRefusal::Nothing))
        );
        assert!(refused_result(&Refused::of("share", "", VoiceRefusal::Nothing)).is_some());
        state.share.mine = vec![my_share(1, ShareKind::Game, ShareMinePhase::Live, 0)];
        assert_eq!(
            go(&state, json!({"t": "voice-share-stop", "n": 1})),
            Ok(VoiceAct::ShareStop { n: 1 })
        );
        assert_eq!(
            go(&state, json!({"t": "voice-share-stop", "n": 2})),
            Err(Refused::quiet()),
            "not one of the player's"
        );
        assert_eq!(
            check(
                &frame(json!({"t": "voice-share-game"})),
                None,
                |_| false,
                |_| false
            ),
            Some(Err(Refused::of("share", "", VoiceRefusal::Nothing)))
        );
        assert_eq!(VoiceAct::ShareGame.result_name(), Some("share"));
    }

    #[test]
    fn the_mirror_tells_sharing_when_it_changes_only() {
        let mut mirror = VoiceMirror::default();
        assert!(mirror.update_share(VoiceShareShown::default()).is_none());
        let share = share_of(&sharing_state());
        assert!(mirror.update_share(share.clone()).is_some());
        assert!(mirror.update_share(share).is_none());
        assert_eq!(
            serde_json::to_value(mirror.update_share(VoiceShareShown::default()).unwrap()).unwrap(),
            json!({"t": "voice-share"})
        );
    }

    #[test]
    fn cards_keep_their_words_while_the_game_is_in_the_share() {
        let mut state = sharing_state();
        assert!(!private(Some(&state), true), "not sharing");
        state.share.mine = vec![my_share(1, ShareKind::Window, ShareMinePhase::Live, 0)];
        assert!(!private(Some(&state), true), "a window shows no game");
        state
            .share
            .mine
            .push(my_share(2, ShareKind::Screen, ShareMinePhase::Live, 0));
        assert!(private(Some(&state), true));
        assert!(
            !private(Some(&state), false),
            "the player chose to show them"
        );
        assert!(!private(None, true));
        let card = |kind| ToastView {
            kind,
            key: "k".into(),
            title: "Kaan_07".into(),
            body: "the secret".into(),
            conv: Some("d:u-kaan".into()),
            crew: Some("Gece Kuşları".into()),
            sticker: Some("gg".into()),
            badge: Some("GK".into()),
            marks: vec!["early".into()],
            by: Some("u-kaan".into()),
            at: None,
        };
        let message = keep_private(card(ToastKind::Message)).unwrap();
        assert_eq!(
            serde_json::to_value(ToGame::Toast(message)).unwrap(),
            json!({"t": "toast", "kind": "message", "key": "k", "title": "", "body": "",
                   "conv": "d:u-kaan"})
        );
        for kind in [
            ToastKind::Invite,
            ToastKind::World,
            ToastKind::Call,
            ToastKind::Plan,
        ] {
            let card = keep_private(card(kind)).unwrap();
            assert!(
                card.title.is_empty()
                    && card.body.is_empty()
                    && card.by.is_none()
                    && card.crew.is_none()
            );
        }
        // Who came online, who asks, a missed call, an AFK notice: not while the share runs.
        for kind in [
            ToastKind::Request,
            ToastKind::Friend,
            ToastKind::Online,
            ToastKind::Projects,
            ToastKind::CrewInvite,
            ToastKind::CrewAsk,
            ToastKind::CrewIn,
            ToastKind::Missed,
            ToastKind::Info,
        ] {
            assert!(keep_private(card(kind)).is_none(), "{kind:?}");
        }
    }

    #[test]
    fn sharing_notices_use_the_wire_names() {
        let notice = |notice: ShareNotice| {
            share_notice_of(&notice)
                .map(|view| serde_json::to_value(ToGame::VoiceNotice(view)).unwrap())
        };
        assert_eq!(
            notice(ShareNotice::refused(ShareRefusal::NotAllowed)),
            Some(json!({"t": "voice-notice", "kind": "share-refused", "reason": "not-allowed"}))
        );
        assert_eq!(
            notice(ShareNotice::Closed {
                why: ShareClosedWhy::Idle,
                src: ShareKind::Game
            }),
            Some(json!({"t": "voice-notice", "kind": "share-closed", "reason": "idle"}))
        );
        assert_eq!(
            notice(ShareNotice::AudioOff),
            Some(json!({"t": "voice-notice", "kind": "share-audio-off"}))
        );
        assert_eq!(
            notice(ShareNotice::Closed {
                why: ShareClosedWhy::Left,
                src: ShareKind::Game
            }),
            None
        );
        assert_eq!(notice(ShareNotice::WatchFailed), None);
        assert_eq!(notice(ShareNotice::refused(ShareRefusal::WatchFull)), None);
    }

    /// SQOL-O: the player's share says how its watchers see it, others' shares when they are paused
    /// or minimized; "Duraklat" goes only for the player's own share while live changes may.
    #[test]
    fn a_paused_or_minimized_share_says_so_and_pausing_goes_only_while_it_may() {
        use pl_voice::share::{ShareHoldWhy, ShareSt};
        let mut state = sharing_state();
        let mut mine = my_share(1, ShareKind::Game, ShareMinePhase::Live, 2);
        mine.st = ShareSt::Hold;
        mine.hold = Some(ShareHoldWhy::Paused);
        state.share.mine = vec![mine];
        state.share.live[0].st = ShareSt::Min;
        let shown = serde_json::to_value(share_of(&state)).unwrap();
        assert_eq!(shown["mine"][0]["st"], "hold");
        assert_eq!(shown["mine"][0]["hold"], "paused");
        assert_eq!(shown["live"][0]["st"], "min");
        assert!(shown.get("edit").is_none());
        // Without the service's live changes the press is refused here, said as `share-hold`.
        let press = FromGame::VoiceShareHold { n: 1, on: false };
        let refused = check(&press, Some(&state), |_| true, |_| true)
            .unwrap()
            .unwrap_err();
        assert_eq!(
            refused_result(&refused),
            Some(ToGame::VoiceResult {
                act: "share-hold",
                ok: false,
                reason: Some("nothing"),
                who: String::new(),
            })
        );
        state.share.edit = true;
        assert_eq!(
            serde_json::to_value(share_of(&state)).unwrap()["edit"],
            true
        );
        assert_eq!(
            check(&press, Some(&state), |_| true, |_| true),
            Some(Ok(VoiceAct::ShareHold { n: 1, on: false }))
        );
        // Only the player's own shares.
        let other = FromGame::VoiceShareHold { n: 3, on: true };
        assert!(
            check(&other, Some(&state), |_| true, |_| true)
                .unwrap()
                .is_err()
        );
        // The launcher's answer as the mod hears it.
        let act = VoiceAct::ShareHold { n: 1, on: true };
        assert_eq!(
            result_of(&act, &hold_answer(Ok(()))),
            Some(ToGame::VoiceResult {
                act: "share-hold",
                ok: true,
                reason: None,
                who: String::new(),
            })
        );
        assert_eq!(
            hold_answer(Err(ShareRefusal::Gone)).refused,
            Some(VoiceRefusal::NotFound)
        );
        assert_eq!(
            hold_answer(Err(ShareRefusal::Off)).refused,
            Some(VoiceRefusal::Off)
        );
        assert_eq!(
            hold_answer(Err(ShareRefusal::Budget)).refused,
            Some(VoiceRefusal::Failed)
        );
    }
}
