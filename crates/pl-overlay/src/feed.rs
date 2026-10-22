//! What the mod hears, made from the friends feature's views: a greeting (`welcome` and `snapshot`)
//! after every handshake, then only what changed. Pure: the session calls it with the newest view
//! and the clock.
//!
//! - [`Scene`] distils a [`SocialView`] into the protocol's views (names instead of ids, crews by
//!   name, calls with their answer counts) and the facts the toast rules need.
//! - [`Mirror`] remembers what the live connection was told, so a new view becomes `friend`,
//!   `friend-gone`, `conv`, `call`, `call-end` and `dnd` frames (or a new greeting when the account,
//!   the features or the reduced-motion choice changed).
//! - [`chat_frames`] turns one conversation's change into `msg`, `msg-gone`, `history` (cleared),
//!   `sent` and `send-failed`.
//! - [`toast_of`] asks [`crate::policy`] about a notice.

use std::collections::{BTreeMap, HashMap, HashSet};

use pl_social::{
    AuthPhase, CallAnswer as SocialAnswer, ChatChangeView, ChatMessageView, ChatPage, Connection,
    GameLocation, MessageState, PresenceState, SendFailure as SocialFailure, SocialNotice,
    SocialView, StatusMode, TEAM_FEATURE_CALLS,
};

use crate::policy::{self, CrewMute, CrewSettings, Mute, Notice, NotifyPrefs, Situation};
use crate::protocol::{
    AnswerCounts, CallAnswer, CallView, ConvKind, ConvView, FriendView, MAX_HISTORY,
    MAX_LAUNCHER_FRAME, Me, MsgView, PROTO, Place, SendFailure, ToGame, ToastView,
};

/// Conversations a snapshot lists at most (the newest first).
pub const MAX_CONVS: usize = 50;
/// Friends a snapshot lists at most (in game and online first).
pub const MAX_FRIENDS: usize = 400;
/// The snapshot stays this far below the frame limit.
const SNAPSHOT_BUDGET: usize = MAX_LAUNCHER_FRAME - 16 * 1024;

/// The friends feature as the overlay sees it at one moment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    /// Signed in with the friends feature on.
    pub signed_in: bool,
    /// The account (a new one means a new greeting).
    pub me_id: String,
    pub me: String,
    /// "Rahatsız etmeyin".
    pub dnd: bool,
    /// The presence connection is up: a message can go now.
    pub online: bool,
    pub features: Vec<String>,
    pub friends: Vec<FriendView>,
    pub convs: Vec<ConvView>,
    pub calls: Vec<CallView>,
    /// What the toast rules look at.
    pub service: Option<CrewSettings>,
    pub crews: Vec<CrewMute>,
    /// User id → name (the player, friends, crewmates).
    names: HashMap<String, String>,
    crew_names: HashMap<String, String>,
    blocked: HashSet<String>,
    /// Friend id → the ids of their invites still worth answering.
    invites: HashMap<String, Vec<String>>,
    /// Server addresses friends and crews share right now (a friend's game, an invite, a call),
    /// as [`server_address`] reads them: the only places besides Projects the launcher would
    /// send the game to.
    shared: HashSet<(String, u16)>,
}

/// `host[:port]` as a server address: the host in lower case, the port (25565 when none). None
/// for anything that is not a plain host name or IPv4 address.
pub fn server_address(address: &str) -> Option<(String, u16)> {
    let address = address.trim();
    let (host, port) = match address.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, port.parse::<u16>().ok()?)
        }
        _ => (address, 25565),
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let plain = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    (plain && port != 0).then_some((host, port))
}

fn place_of(state: PresenceState, game: Option<&pl_social::Game>) -> (Place, Option<String>) {
    match state {
        PresenceState::Offline => (Place::Offline, None),
        PresenceState::Launcher => (Place::Launcher, None),
        PresenceState::Playing => match game {
            Some(game) => match game.location {
                GameLocation::Projects => (Place::Projects, None),
                GameLocation::Singleplayer => (Place::Single, None),
                GameLocation::Other => (Place::Server, game.host.clone()),
            },
            // In game, where is not known (the menus): the mod says "Oyunda".
            None => (Place::Game, None),
        },
    }
}

fn answer_of(answer: SocialAnswer) -> CallAnswer {
    match answer {
        SocialAnswer::Yes => CallAnswer::Coming,
        SocialAnswer::Soon => CallAnswer::Soon,
        SocialAnswer::No => CallAnswer::Cant,
    }
}

impl Scene {
    pub fn from_view(view: &SocialView, prefs: &NotifyPrefs, now_ms: i64) -> Self {
        let Some(me) = view
            .me
            .as_ref()
            .filter(|_| view.enabled && view.auth == AuthPhase::SignedIn)
        else {
            return Self::default();
        };
        let mut names: HashMap<String, String> = HashMap::new();
        names.insert(me.id.clone(), me.name.clone());
        for crew in &view.crews {
            for member in &crew.members {
                names.insert(member.id.clone(), member.name.clone());
            }
        }
        // The friend list's names win: they are the ones the player sees everywhere.
        for friend in &view.friends {
            names.insert(friend.id.clone(), friend.name.clone());
        }
        let crew_names: HashMap<String, String> = view
            .crews
            .iter()
            .map(|crew| (crew.id.clone(), crew.name.clone()))
            .collect();
        let service = view.crew_notifications.map(|all| CrewSettings {
            calls: all.calls,
            mute: Mute {
                muted: all.muted,
                until: all.mute_until,
            },
        });
        let crews: Vec<CrewMute> = view
            .crews
            .iter()
            .map(|crew| CrewMute {
                id: crew.id.clone(),
                mute: Mute {
                    muted: crew.notifications.muted,
                    until: crew.notifications.mute_until,
                },
            })
            .collect();
        let all_muted = service.is_some_and(|all| all.mute.on_at(now_ms));

        let mut features = vec!["friends".to_owned()];
        let chat_on = view.chat.as_ref().is_some_and(|chat| chat.on);
        if chat_on {
            features.push("chat".to_owned());
        }
        if view.teams && view.team_features.iter().any(|f| f == TEAM_FEATURE_CALLS) {
            features.push("calls".to_owned());
        }

        let mut friends: Vec<FriendView> = view
            .friends
            .iter()
            .map(|friend| {
                let (place, host) = place_of(friend.state, friend.game.as_ref());
                FriendView {
                    id: friend.id.clone(),
                    name: friend.name.clone(),
                    place,
                    joinable: place == Place::Projects
                        || (place == Place::Server && host.is_some()),
                    host,
                }
            })
            .collect();
        // In game first, then online, then the rest, each in the list's order (a stable sort).
        friends.sort_by_key(|friend| match friend.place {
            Place::Projects | Place::Server | Place::Single | Place::Game => 0,
            Place::Launcher => 1,
            Place::Offline => 2,
        });
        friends.truncate(MAX_FRIENDS);

        let convs: Vec<ConvView> = view
            .chat
            .iter()
            .flat_map(|chat| chat.conversations.iter())
            .take(MAX_CONVS)
            .map(|conv| {
                let crew = conv.crew.as_deref();
                let title = match (crew, conv.peer.as_deref()) {
                    (Some(crew), _) => crew_names.get(crew).cloned(),
                    (None, Some(peer)) => names.get(peer).cloned(),
                    (None, None) => None,
                }
                .or_else(|| conv.title.clone())
                .unwrap_or_default();
                let crew_muted = crew.is_some_and(|crew| {
                    all_muted
                        || crews
                            .iter()
                            .any(|one| one.id == crew && one.mute.on_at(now_ms))
                });
                ConvView {
                    id: conv.id.clone(),
                    kind: if crew.is_some() {
                        ConvKind::Crew
                    } else {
                        ConvKind::Dm
                    },
                    title,
                    unread: conv.unread,
                    muted: prefs.conversation_muted(&conv.id, now_ms) || crew_muted,
                    writable: chat_on && conv.writable,
                }
            })
            .collect();

        let calls: Vec<CallView> = view
            .calls
            .iter()
            .filter(|call| call.ends_at * 1000 > now_ms)
            .map(|call| {
                let crew = call
                    .crews
                    .first()
                    .and_then(|id| crew_names.get(id))
                    .or_else(|| view.crews.first().map(|crew| &crew.name))
                    .cloned()
                    .unwrap_or_default();
                let mut answers = AnswerCounts::default();
                for answer in &call.answers {
                    match answer.answer {
                        SocialAnswer::Yes => answers.coming += 1,
                        SocialAnswer::Soon => answers.soon += 1,
                        SocialAnswer::No => answers.cant += 1,
                    }
                }
                let (place, host) = match &call.server {
                    Some(game) => match game.location {
                        GameLocation::Projects => (Some(Place::Projects), None),
                        GameLocation::Other => (Some(Place::Server), game.host.clone()),
                        GameLocation::Singleplayer => (None, None),
                    },
                    None => (None, None),
                };
                CallView {
                    id: call.id.clone(),
                    crew,
                    from: call
                        .name
                        .clone()
                        .or_else(|| names.get(&call.from).cloned())
                        .unwrap_or_default(),
                    since: call.started_at * 1000,
                    until: call.ends_at * 1000,
                    answers,
                    mine: call.my_answer.map(answer_of),
                    place,
                    host,
                    own: call.mine,
                }
            })
            .collect();

        let mut invites: HashMap<String, Vec<String>> = HashMap::new();
        for invite in &view.invites {
            invites
                .entry(invite.from.clone())
                .or_default()
                .push(invite.id.clone());
        }
        let other_host = |game: Option<&pl_social::Game>| {
            game.filter(|game| game.location == GameLocation::Other)
                .and_then(|game| game.host.as_deref())
                .and_then(server_address)
        };
        let shared: HashSet<(String, u16)> = view
            .friends
            .iter()
            .filter(|friend| friend.state == PresenceState::Playing)
            .filter_map(|friend| other_host(friend.game.as_ref()))
            .chain(
                view.invites
                    .iter()
                    .filter_map(|invite| other_host(invite.server.as_ref())),
            )
            .chain(
                view.calls
                    .iter()
                    .filter(|call| call.ends_at * 1000 > now_ms)
                    .filter_map(|call| other_host(call.server.as_ref())),
            )
            .collect();

        Self {
            signed_in: true,
            me_id: me.id.clone(),
            me: me.name.clone(),
            dnd: me.status == StatusMode::Dnd,
            online: view.connection == Connection::Online,
            features,
            friends,
            convs,
            calls,
            service,
            crews,
            names,
            crew_names,
            blocked: view.blocked.iter().map(|b| b.id.clone()).collect(),
            invites,
            shared,
        }
    }

    /// Where "Bağlan" may send the running game: `target` (`host[:port]`) when it is Projects or
    /// an address a friend or crew shares right now, else None.
    pub fn connect_target(&self, target: &str) -> Option<(String, u16)> {
        let (host, port) = server_address(target)?;
        (pl_social::is_projects_host(&host) || self.shared.contains(&(host.clone(), port)))
            .then_some((host, port))
    }

    /// A user's name as the lists show it.
    pub fn name_of(&self, id: &str) -> Option<&str> {
        self.names.get(id).map(String::as_str)
    }

    pub fn crew_name(&self, id: &str) -> Option<&str> {
        self.crew_names.get(id).map(String::as_str)
    }

    pub fn is_blocked(&self, id: &str) -> bool {
        self.blocked.contains(id)
    }

    /// The friend's invites still worth answering.
    pub fn invites_from(&self, friend: &str) -> &[String] {
        self.invites.get(friend).map_or(&[], Vec::as_slice)
    }

    pub fn friend(&self, id: &str) -> Option<&FriendView> {
        self.friends.iter().find(|friend| friend.id == id)
    }

    /// The name a message shows for its sender: as the lists know them, else the conversation's
    /// title for a direct conversation (the sender is the other one), else "someone".
    fn sender(&self, conv: &str, from: &str) -> String {
        if let Some(name) = self.name_of(from) {
            return name.to_owned();
        }
        if conv.starts_with("d:")
            && let Some(conv) = self.convs.iter().find(|one| one.id == conv)
            && !conv.title.is_empty()
        {
            return conv.title.clone();
        }
        pl_lang::pick("Biri", "Someone").to_owned()
    }
}

/// What the greeting said: a change sends a new one (and a fresh snapshot).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity {
    me_id: String,
    me: String,
    features: Vec<String>,
    reduced_motion: bool,
}

/// What the live connection was told, for the next differences.
#[derive(Debug, Clone, Default)]
pub struct Mirror {
    identity: Option<Identity>,
    friends: BTreeMap<String, FriendView>,
    convs: BTreeMap<String, ConvView>,
    calls: BTreeMap<String, CallView>,
    dnd: bool,
}

impl Mirror {
    /// `welcome` and `snapshot`: everything after a handshake or a change of account.
    pub fn greet(&mut self, scene: &Scene, launcher: &str, reduced_motion: bool) -> Vec<ToGame> {
        let identity = Identity {
            me_id: scene.me_id.clone(),
            me: scene.me.clone(),
            features: if scene.signed_in {
                scene.features.clone()
            } else {
                Vec::new()
            },
            reduced_motion,
        };
        let welcome = ToGame::Welcome {
            proto: PROTO,
            launcher: launcher.to_owned(),
            me: Me {
                name: identity.me.clone(),
            },
            dnd: scene.dnd,
            reduced_motion,
            features: identity.features.clone(),
        };
        let (friends, convs, calls) = fitting_snapshot(scene);
        *self = Self {
            identity: Some(identity),
            friends: friends.iter().map(|f| (f.id.clone(), f.clone())).collect(),
            convs: convs.iter().map(|c| (c.id.clone(), c.clone())).collect(),
            calls: calls.iter().map(|c| (c.id.clone(), c.clone())).collect(),
            dnd: scene.dnd,
        };
        let snapshot = ToGame::Snapshot {
            friends,
            convs,
            calls,
            dnd: scene.dnd,
        };
        vec![welcome, snapshot]
    }

    /// The differences since the mod last heard, or a new greeting.
    pub fn update(&mut self, scene: &Scene, launcher: &str, reduced_motion: bool) -> Vec<ToGame> {
        let identity_changed = self.identity.as_ref().is_none_or(|known| {
            known.me_id != scene.me_id
                || known.me != scene.me
                || known.reduced_motion != reduced_motion
                || (if scene.signed_in {
                    known.features != scene.features
                } else {
                    !known.features.is_empty()
                })
        });
        if identity_changed {
            return self.greet(scene, launcher, reduced_motion);
        }
        let mut frames = Vec::new();
        if scene.dnd != self.dnd {
            self.dnd = scene.dnd;
            frames.push(ToGame::Dnd { on: scene.dnd });
        }
        diff(
            &mut self.friends,
            &scene.friends,
            |f| &f.id,
            &mut frames,
            |f| ToGame::Friend(f.clone()),
            |id| ToGame::FriendGone { id },
        );
        // Conversations never leave the list (a newer one may push one out of the snapshot's
        // top; it stays in the mod as it was).
        for conv in &scene.convs {
            if self.convs.get(&conv.id) != Some(conv) {
                self.convs.insert(conv.id.clone(), conv.clone());
                frames.push(ToGame::Conv(conv.clone()));
            }
        }
        diff(
            &mut self.calls,
            &scene.calls,
            |c| &c.id,
            &mut frames,
            |c| ToGame::Call(c.clone()),
            |id| ToGame::CallEnd { id },
        );
        frames
    }
}

/// Upserts for new and changed entries, removals for the ones gone; `known` follows.
fn diff<T: Clone + PartialEq>(
    known: &mut BTreeMap<String, T>,
    now: &[T],
    id: impl Fn(&T) -> &String,
    frames: &mut Vec<ToGame>,
    upsert: impl Fn(&T) -> ToGame,
    gone: impl Fn(String) -> ToGame,
) {
    let ids: HashSet<&String> = now.iter().map(&id).collect();
    let removed: Vec<String> = known
        .keys()
        .filter(|key| !ids.contains(key))
        .cloned()
        .collect();
    for key in removed {
        known.remove(&key);
        frames.push(gone(key));
    }
    for entry in now {
        if known.get(id(entry)) != Some(entry) {
            known.insert(id(entry).clone(), entry.clone());
            frames.push(upsert(entry));
        }
    }
}

/// The scene's lists, cut down until the snapshot fits well inside a frame: offline friends go
/// first, then the oldest conversations, then friends from the end.
fn fitting_snapshot(scene: &Scene) -> (Vec<FriendView>, Vec<ConvView>, Vec<CallView>) {
    let mut friends = scene.friends.clone();
    let mut convs = scene.convs.clone();
    let calls = scene.calls.clone();
    loop {
        let size = serde_json::to_vec(&ToGame::Snapshot {
            friends: friends.clone(),
            convs: convs.clone(),
            calls: calls.clone(),
            dnd: scene.dnd,
        })
        .map_or(usize::MAX, |bytes| bytes.len());
        if size <= SNAPSHOT_BUDGET {
            return (friends, convs, calls);
        }
        if let Some(at) = friends.iter().rposition(|f| f.place == Place::Offline) {
            friends.remove(at);
        } else if convs.len() > 10 {
            convs.pop();
        } else if !friends.is_empty() {
            friends.pop();
        } else {
            convs.clear();
            return (friends, convs, calls);
        }
    }
}

fn failure_of(failure: Option<SocialFailure>) -> SendFailure {
    match failure {
        Some(SocialFailure::Link) => SendFailure::Link,
        Some(SocialFailure::Phone) => SendFailure::Phone,
        Some(SocialFailure::Words) => SendFailure::Words,
        Some(SocialFailure::Invalid) => SendFailure::TooLong,
        Some(SocialFailure::TooFast) => SendFailure::Rate,
        Some(SocialFailure::DailyLimit) => SendFailure::DailyLimit,
        Some(SocialFailure::Muted) => SendFailure::Muted,
        Some(SocialFailure::Off) => SendFailure::Off,
        Some(SocialFailure::NotAllowed) => SendFailure::NotAllowed,
        Some(SocialFailure::Connection) | None => SendFailure::Offline,
    }
}

/// A message as the mod shows it.
pub fn msg_view(conv: &str, message: &ChatMessageView, scene: &Scene) -> MsgView {
    MsgView {
        conv: Some(conv.to_owned()),
        id: message.key.clone(),
        cid: None,
        from: if message.mine {
            scene.me.clone()
        } else {
            scene.sender(conv, &message.from)
        },
        // A tombstone (deleted for everyone) carries no text; the mod says "Bu mesaj silindi".
        text: if message.deleted.is_some() {
            String::new()
        } else {
            message.text.clone()
        },
        at: message.at,
        mine: message.mine,
        edited: message.edited && message.deleted.is_none(),
        deleted: message.deleted.is_some(),
    }
}

/// What one conversation's change means for the mod, and the messages the overlay sent that did
/// not go (`discard`: the launcher drops them from the history; the panel shows them as not sent).
#[derive(Debug, Default, PartialEq)]
pub struct ChatOutcome {
    pub frames: Vec<ToGame>,
    pub discard: Vec<String>,
    /// A message of someone else arrived (the open conversation then counts as read).
    pub arrived: bool,
}

/// The messages the panel sent: their keys in the history, mapped to the panel's own ids (`cid`),
/// and the keys of those that did not go and are being dropped from the history.
#[derive(Debug, Default)]
pub struct Sends {
    by_key: HashMap<String, String>,
    dropped: HashSet<String>,
}

impl Sends {
    pub fn insert(&mut self, key: String, cid: String) {
        self.by_key.insert(key, cid);
    }

    /// Messages still on their way.
    #[cfg(test)]
    pub fn pending(&self) -> usize {
        self.by_key.len()
    }
}

pub fn chat_frames(change: &ChatChangeView, scene: &Scene, sends: &mut Sends) -> ChatOutcome {
    let mut out = ChatOutcome::default();
    let conv = change.conv.as_str();
    if change.reset {
        out.frames.push(ToGame::History {
            conv: conv.to_owned(),
            msgs: Vec::new(),
        });
    }
    for key in &change.remove {
        // A message of the panel's that did not go already heard why; one still on its way was
        // dropped elsewhere (the launcher's "Sil"), which the panel's timeout tells.
        if sends.dropped.remove(key) || sends.by_key.remove(key).is_some() {
            continue;
        }
        out.frames.push(ToGame::MsgGone {
            conv: conv.to_owned(),
            id: key.clone(),
        });
    }
    for message in &change.upsert {
        if let Some(cid) = sends.by_key.get(&message.key).cloned() {
            match message.state {
                MessageState::Sending => {}
                MessageState::Sent => {
                    sends.by_key.remove(&message.key);
                    out.frames.push(ToGame::Sent {
                        cid,
                        id: message.key.clone(),
                    });
                }
                MessageState::Failed => {
                    sends.by_key.remove(&message.key);
                    sends.dropped.insert(message.key.clone());
                    out.frames.push(ToGame::SendFailed {
                        cid,
                        reason: failure_of(message.failure),
                    });
                    out.discard.push(message.key.clone());
                }
            }
            continue;
        }
        // The player's messages from the launcher or another device show once they went; a
        // message deleted for everyone comes back as its tombstone (same id).
        if message.state == MessageState::Sent {
            out.arrived |= !message.mine && message.deleted.is_none();
            out.frames.push(ToGame::Msg(msg_view(conv, message, scene)));
        }
    }
    out
}

/// The answer to `open-conv`: the newest messages that went, oldest first.
pub fn history_frame(page: &ChatPage, scene: &Scene) -> ToGame {
    let sent: Vec<&ChatMessageView> = page
        .messages
        .iter()
        .filter(|message| message.state == MessageState::Sent)
        .collect();
    let skip = sent.len().saturating_sub(MAX_HISTORY);
    ToGame::History {
        conv: page.conv.clone(),
        msgs: sent
            .into_iter()
            .skip(skip)
            .map(|message| MsgView {
                conv: None,
                ..msg_view(&page.conv, message, scene)
            })
            .collect(),
    }
}

/// The in-game toast for a notice, when the rules want one.
pub fn toast_of(
    notice: &SocialNotice,
    scene: &Scene,
    prefs: &NotifyPrefs,
    now_ms: i64,
) -> Option<ToastView> {
    let situation = Situation {
        dnd: scene.dnd,
        service: scene.service,
        crews: &scene.crews,
        prefs,
        now_ms,
    };
    match notice {
        // A tombstone is never news.
        SocialNotice::ChatMessage { message, .. } if message.deleted.is_some() => None,
        SocialNotice::ChatMessage {
            conv,
            message,
            name,
            crew,
        } => {
            let crew_name = crew
                .as_deref()
                .map(|id| (id, scene.crew_name(id).unwrap_or_default()));
            let from = name
                .clone()
                .unwrap_or_else(|| scene.sender(conv, &message.from));
            policy::toast_for(
                &Notice::Message {
                    conv,
                    crew: crew_name,
                    from: &from,
                    text: &message.text,
                    mine: message.mine,
                    blocked: scene.is_blocked(&message.from),
                },
                &situation,
            )
        }
        SocialNotice::CrewCall { call } if !call.mine => {
            let crew = call
                .crews
                .first()
                .and_then(|id| scene.crew_name(id))
                .or_else(|| {
                    scene
                        .crews
                        .first()
                        .and_then(|first| scene.crew_name(&first.id))
                })
                .unwrap_or_default();
            let from = call
                .name
                .clone()
                .or_else(|| scene.name_of(&call.from).map(str::to_owned))
                .unwrap_or_default();
            policy::toast_for(
                &Notice::Call {
                    id: &call.id,
                    crews: &call.crews,
                    crew,
                    from: &from,
                },
                &situation,
            )
        }
        SocialNotice::Invite { invite } => {
            let from = invite
                .name
                .clone()
                .or_else(|| scene.name_of(&invite.from).map(str::to_owned))
                .unwrap_or_default();
            policy::toast_for(
                &Notice::Invite {
                    id: &invite.id,
                    from: &from,
                    quiet: invite.quiet,
                },
                &situation,
            )
        }
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pl_social::{
        CallAnswerView, CallKind, CallPlace, ChatView, ConversationView, CrewCallView,
        CrewNotificationsView, Game, InviteView, MeView, Privacy, ProfileView, TeamView,
    };

    pub const NOW: i64 = 1_800_000_000_000;

    pub fn me() -> MeView {
        MeView {
            id: "me".into(),
            name: "Ece".into(),
            username: "ece".into(),
            avatar: "discord".into(),
            avatar_url: None,
            discord_avatar_url: String::new(),
            friend_code: "PRJ-AAAA-BBBB".into(),
            share_link: None,
            qr: None,
            status: StatusMode::Online,
            privacy: Privacy {
                share_server: false,
                share_last_seen: true,
            },
            backup: None,
            limited: None,
        }
    }

    pub fn friend(
        id: &str,
        name: &str,
        state: PresenceState,
        game: Option<Game>,
    ) -> pl_social::FriendView {
        pl_social::FriendView {
            id: id.into(),
            name: name.into(),
            username: name.to_lowercase(),
            avatar: "discord".into(),
            avatar_url: None,
            since: String::new(),
            last_seen: None,
            state,
            dnd: false,
            game,
            discord_url: String::new(),
        }
    }

    pub fn projects() -> Option<Game> {
        Some(Game {
            location: GameLocation::Projects,
            host: None,
        })
    }

    pub fn crew(id: &str, name: &str, members: &[(&str, &str)]) -> TeamView {
        TeamView {
            id: id.into(),
            name: name.into(),
            tag: "GK".into(),
            emblem: "sword".into(),
            color: "gold".into(),
            frame: "cut".into(),
            motto: String::new(),
            code: "EKP-AAAA-BBBB".into(),
            created_on: "2026-10-20".into(),
            role: pl_social::TeamRole::Member,
            moderator: false,
            permissions: pl_social::TeamPermissions::default(),
            members: members
                .iter()
                .map(|(id, name)| pl_social::TeamMemberView {
                    id: (*id).into(),
                    name: (*name).into(),
                    username: name.to_lowercase(),
                    avatar: "discord".into(),
                    avatar_url: None,
                    role: pl_social::TeamRole::Member,
                    moderator: false,
                    me: *id == "me",
                    friend: false,
                    blocked: false,
                    state: PresenceState::Launcher,
                    dnd: false,
                    game: None,
                    discord_url: String::new(),
                    joined_at: 0,
                    together_month: 0,
                })
                .collect(),
            invited: Vec::new(),
            limit: 30,
            notifications: pl_social::CrewMute::default(),
            text_locked_until: None,
            join: pl_social::JoinMode::Open,
            link: None,
            link_url: None,
            requests: Vec::new(),
            together_week: 0,
        }
    }

    pub fn conversation(
        id: &str,
        peer: Option<&str>,
        crew: Option<&str>,
        unread: u32,
    ) -> ConversationView {
        ConversationView {
            id: id.into(),
            peer: peer.map(Into::into),
            crew: crew.map(Into::into),
            title: Some("Eski ad".into()),
            last: None,
            unread,
            writable: true,
            failed: false,
        }
    }

    pub fn view() -> SocialView {
        SocialView {
            seq: 1,
            enabled: true,
            auth: AuthPhase::SignedIn,
            connection: Connection::Online,
            me: Some(me()),
            loaded: true,
            friends: vec![
                friend("f-ayse", "Ayşe", PresenceState::Playing, projects()),
                friend("f-can", "Can", PresenceState::Offline, None),
                friend(
                    "f-zey",
                    "Zeynep",
                    PresenceState::Playing,
                    Some(Game {
                        location: GameLocation::Other,
                        host: Some("mc.example.net".into()),
                    }),
                ),
                friend("f-mert", "Mert", PresenceState::Launcher, None),
            ],
            incoming: Vec::new(),
            outgoing: Vec::new(),
            blocked: vec![ProfileView {
                id: "x-bad".into(),
                name: "Kötü".into(),
                username: "kotu".into(),
                avatar: "discord".into(),
                avatar_url: None,
            }],
            invites: vec![InviteView {
                id: "inv-1".into(),
                from: "f-ayse".into(),
                name: Some("Ayşe".into()),
                avatar: None,
                avatar_url: None,
                at: 1,
                server: projects(),
                quiet: false,
            }],
            limits: None,
            activity: pl_social::ActivityView::Launcher,
            groups: None,
            teams: true,
            team_features: vec!["calls".into(), "mute".into()],
            crews: vec![crew(
                "t1",
                "Gece Kuşları",
                &[("me", "Ece"), ("u-cem", "Cem")],
            )],
            crew_limit: 5,
            team_invites: Vec::new(),
            team_requests: Vec::new(),
            plans: Vec::new(),
            together: false,
            calls: vec![CrewCallView {
                id: "k1".into(),
                from: "u-cem".into(),
                mine: false,
                name: None,
                avatar: None,
                avatar_url: None,
                kind: CallKind::Join,
                server: projects(),
                crews: vec!["t1".into()],
                started_at: NOW / 1000 - 60,
                ends_at: NOW / 1000 + 1_140,
                answers: vec![
                    CallAnswerView {
                        id: "f-ayse".into(),
                        answer: SocialAnswer::Yes,
                    },
                    CallAnswerView {
                        id: "f-mert".into(),
                        answer: SocialAnswer::No,
                    },
                ],
                my_answer: None,
            }],
            call_place: CallPlace::NeedsServer,
            crew_notifications: Some(CrewNotificationsView {
                calls: true,
                muted: false,
                mute_until: None,
            }),
            chat_allowed: true,
            chat: Some(ChatView {
                on: true,
                mailbox: true,
                per_day: 500,
                muted_until: None,
                conversations: vec![
                    conversation("d:f-ayse:me", Some("f-ayse"), None, 2),
                    conversation("c:t1", None, Some("t1"), 0),
                ],
            }),
            notices: Vec::new(),
            restriction: None,
            backup_waits_until: None,
        }
    }

    pub fn message(key: &str, from: &str, mine: bool, state: MessageState) -> ChatMessageView {
        ChatMessageView {
            key: key.into(),
            id: key.strip_prefix("i:").and_then(|id| id.parse().ok()),
            from: from.into(),
            mine,
            text: format!("metin {key}"),
            at: NOW,
            edited: false,
            state,
            failure: None,
            edit_until: None,
            delete_until: None,
            deleted: None,
        }
    }

    /// `message` deleted for everyone, as the friends feature shows it (FIX2-D).
    pub fn tombstone(key: &str, from: &str) -> ChatMessageView {
        ChatMessageView {
            text: String::new(),
            deleted: Some(pl_social::ChatDeletedView {
                by: from.into(),
                at: Some(NOW),
            }),
            ..message(key, from, false, MessageState::Sent)
        }
    }

    #[test]
    fn a_view_becomes_names_places_and_counts() {
        let scene = Scene::from_view(&view(), &NotifyPrefs::default(), NOW);
        assert!(scene.signed_in && scene.online && !scene.dnd);
        assert_eq!(scene.me, "Ece");
        assert_eq!(scene.features, ["friends", "chat", "calls"]);
        // In game first, then the launcher, then offline.
        let order: Vec<&str> = scene.friends.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(order, ["Ayşe", "Zeynep", "Mert", "Can"]);
        let zeynep = scene.friend("f-zey").unwrap();
        assert_eq!(
            (zeynep.place, zeynep.joinable, zeynep.host.as_deref()),
            (Place::Server, true, Some("mc.example.net"))
        );
        assert!(scene.friend("f-ayse").unwrap().joinable);
        assert!(!scene.friend("f-mert").unwrap().joinable);
        // Conversations by the lists' names.
        assert_eq!(scene.convs[0].title, "Ayşe");
        assert_eq!(scene.convs[0].kind, ConvKind::Dm);
        assert_eq!(scene.convs[0].unread, 2);
        assert_eq!(scene.convs[1].title, "Gece Kuşları");
        assert!(scene.convs.iter().all(|c| c.writable && !c.muted));
        // The call: its crew by name, the caller from the crew list, answers counted.
        let call = &scene.calls[0];
        assert_eq!(
            (call.crew.as_str(), call.from.as_str()),
            ("Gece Kuşları", "Cem")
        );
        assert_eq!(
            call.answers,
            AnswerCounts {
                coming: 1,
                soon: 0,
                cant: 1
            }
        );
        assert_eq!(call.since, NOW - 60_000);
        assert_eq!(call.place, Some(Place::Projects));
        assert_eq!(scene.invites_from("f-ayse"), ["inv-1"]);
        assert!(scene.is_blocked("x-bad"));
    }

    #[test]
    fn the_game_is_sent_only_where_the_launcher_would_join() {
        let scene = Scene::from_view(&view(), &NotifyPrefs::default(), NOW);
        assert_eq!(
            scene.connect_target("play.projects.gg"),
            Some(("play.projects.gg".to_owned(), 25565))
        );
        assert_eq!(
            scene.connect_target("Play.Projects.gg:25566"),
            Some(("play.projects.gg".to_owned(), 25566))
        );
        // Zeynep shares her server's address.
        assert_eq!(
            scene.connect_target("mc.example.net:25565"),
            Some(("mc.example.net".to_owned(), 25565))
        );
        assert_eq!(scene.connect_target("mc.example.net:25570"), None);
        assert_eq!(scene.connect_target("other.example.org"), None);
        assert_eq!(scene.connect_target("http://play.projects.gg"), None);
        assert_eq!(scene.connect_target(""), None);
        assert_eq!(scene.connect_target("projects.gg.evil.com"), None);
        // A shared address that is not shared any more is not a target either.
        let mut stopped = view();
        stopped.friends[2].state = PresenceState::Launcher;
        let scene = Scene::from_view(&stopped, &NotifyPrefs::default(), NOW);
        assert_eq!(scene.connect_target("mc.example.net"), None);
        assert_eq!(
            server_address("  MC.Example.NET.:25570 "),
            Some(("mc.example.net".to_owned(), 25570))
        );
        assert_eq!(server_address("host:0"), None);
        assert_eq!(server_address("a b"), None);
    }

    #[test]
    fn signed_out_is_an_empty_scene() {
        let mut signed_out = view();
        signed_out.auth = AuthPhase::SignedOut;
        signed_out.me = None;
        let scene = Scene::from_view(&signed_out, &NotifyPrefs::default(), NOW);
        assert_eq!(scene, Scene::default());
        let frames = Mirror::default().greet(&scene, "2.0.0", false);
        let welcome = serde_json::to_value(&frames[0]).unwrap();
        assert_eq!(welcome["features"], serde_json::json!([]));
        assert_eq!(welcome["me"]["name"], "");
    }

    #[test]
    fn mutes_and_ended_calls_show() {
        let mut v = view();
        v.crews[0].notifications = pl_social::CrewMute {
            muted: true,
            mute_until: None,
        };
        v.calls[0].ends_at = NOW / 1000 - 1;
        let prefs = NotifyPrefs {
            chat_mutes: BTreeMap::from([("d:f-ayse:me".to_owned(), None)]),
            ..NotifyPrefs::default()
        };
        let scene = Scene::from_view(&v, &prefs, NOW);
        assert!(scene.convs.iter().all(|c| c.muted));
        assert!(scene.calls.is_empty(), "an ended call is not shown");
    }

    #[test]
    fn the_mirror_sends_only_what_changed() {
        let prefs = NotifyPrefs::default();
        let mut mirror = Mirror::default();
        let scene = Scene::from_view(&view(), &prefs, NOW);
        let greeting = mirror.greet(&scene, "2.0.0", false);
        assert!(matches!(greeting[0], ToGame::Welcome { .. }));
        let ToGame::Snapshot {
            friends,
            convs,
            calls,
            ..
        } = &greeting[1]
        else {
            panic!("no snapshot");
        };
        assert_eq!((friends.len(), convs.len(), calls.len()), (4, 2, 1));
        assert!(mirror.update(&scene, "2.0.0", false).is_empty());

        let mut v = view();
        v.friends[1].state = PresenceState::Launcher; // Can comes online
        v.friends.retain(|f| f.id != "f-mert"); // Mert is no longer a friend
        if let Some(chat) = v.chat.as_mut() {
            chat.conversations[0].unread = 3;
        }
        v.calls.clear();
        v.me.as_mut().unwrap().status = StatusMode::Dnd;
        let frames = mirror.update(&Scene::from_view(&v, &prefs, NOW), "2.0.0", false);
        let kinds: Vec<String> = frames
            .iter()
            .map(|f| {
                serde_json::to_value(f).unwrap()["t"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(kinds, ["dnd", "friend-gone", "friend", "conv", "call-end"]);
        assert!(
            matches!(&frames[2], ToGame::Friend(f) if f.name == "Can" && f.place == Place::Launcher)
        );

        // Another account, or the features changing, greets again.
        let mut other = view();
        other.me.as_mut().unwrap().id = "me2".into();
        let again = mirror.update(&Scene::from_view(&other, &prefs, NOW), "2.0.0", false);
        assert!(matches!(again[0], ToGame::Welcome { .. }));
        let mut no_chat = other.clone();
        no_chat.chat = None;
        let again = mirror.update(&Scene::from_view(&no_chat, &prefs, NOW), "2.0.0", false);
        assert!(
            matches!(again[0], ToGame::Welcome { ref features, .. } if features == &["friends", "calls"])
        );
        // So does the reduced-motion choice.
        let again = mirror.update(&Scene::from_view(&no_chat, &prefs, NOW), "2.0.0", true);
        assert!(matches!(
            again[0],
            ToGame::Welcome {
                reduced_motion: true,
                ..
            }
        ));
    }

    #[test]
    fn a_huge_snapshot_still_fits_one_frame() {
        let mut v = view();
        v.friends = (0..3_000)
            .map(|n| {
                friend(
                    &format!("f{n}"),
                    &"Ş".repeat(32),
                    if n % 10 == 0 {
                        PresenceState::Launcher
                    } else {
                        PresenceState::Offline
                    },
                    None,
                )
            })
            .collect();
        let scene = Scene::from_view(&v, &NotifyPrefs::default(), NOW);
        let frames = Mirror::default().greet(&scene, "2.0.0", false);
        let bytes = crate::server::encode(&frames[1]).unwrap();
        assert!(bytes.len() <= MAX_LAUNCHER_FRAME);
        let ToGame::Snapshot { friends, .. } = &frames[1] else {
            panic!()
        };
        // The online ones stayed.
        assert!(
            friends
                .iter()
                .filter(|f| f.place == Place::Launcher)
                .count()
                >= 300
        );
    }

    #[test]
    fn chat_changes_become_messages_confirmations_and_removals() {
        let scene = Scene::from_view(&view(), &NotifyPrefs::default(), NOW);
        let mut sends = Sends::default();
        sends.insert("n:abc".into(), "cid-1".into());
        let change = ChatChangeView {
            conv: "d:f-ayse:me".into(),
            upsert: vec![
                message("i:1", "f-ayse", false, MessageState::Sent),
                message("n:abc", "me", true, MessageState::Sending),
                // Typed in the launcher, still on its way: nothing yet.
                message("n:zzz", "me", true, MessageState::Sending),
            ],
            remove: vec!["i:0".into()],
            reset: false,
        };
        let out = chat_frames(&change, &scene, &mut sends);
        assert!(out.arrived);
        let json: Vec<serde_json::Value> = out
            .frames
            .iter()
            .map(|f| serde_json::to_value(f).unwrap())
            .collect();
        assert_eq!(
            json[0],
            serde_json::json!({"t":"msg-gone","conv":"d:f-ayse:me","id":"i:0"})
        );
        assert_eq!(json[1]["t"], "msg");
        assert_eq!(json[1]["from"], "Ayşe");
        assert_eq!(json.len(), 2);
        assert_eq!(sends.pending(), 1, "still on its way");

        let sent = ChatChangeView {
            conv: "d:f-ayse:me".into(),
            upsert: vec![message("n:abc", "me", true, MessageState::Sent)],
            remove: Vec::new(),
            reset: false,
        };
        let out = chat_frames(&sent, &scene, &mut sends);
        assert_eq!(
            out.frames,
            [ToGame::Sent {
                cid: "cid-1".into(),
                id: "n:abc".into()
            }]
        );
        assert!(!out.arrived && sends.pending() == 0);

        // A refusal: the panel hears why, the launcher drops it from the history.
        sends.insert("n:def".into(), "cid-2".into());
        let mut failed = message("n:def", "me", true, MessageState::Failed);
        failed.failure = Some(SocialFailure::Link);
        let out = chat_frames(
            &ChatChangeView {
                conv: "d:f-ayse:me".into(),
                upsert: vec![failed],
                remove: Vec::new(),
                reset: false,
            },
            &scene,
            &mut sends,
        );
        assert_eq!(
            out.frames,
            [ToGame::SendFailed {
                cid: "cid-2".into(),
                reason: SendFailure::Link
            }]
        );
        assert_eq!(out.discard, ["n:def"]);
        // Its removal that follows says nothing more.
        let out = chat_frames(
            &ChatChangeView {
                conv: "d:f-ayse:me".into(),
                upsert: Vec::new(),
                remove: vec!["n:def".into()],
                reset: false,
            },
            &scene,
            &mut sends,
        );
        assert!(out.frames.is_empty());

        // "Geçmişi temizle" empties the conversation in the panel too.
        let out = chat_frames(
            &ChatChangeView {
                conv: "c:t1".into(),
                upsert: Vec::new(),
                remove: Vec::new(),
                reset: true,
            },
            &scene,
            &mut Sends::default(),
        );
        assert_eq!(
            out.frames,
            [ToGame::History {
                conv: "c:t1".into(),
                msgs: Vec::new()
            }]
        );
    }

    #[test]
    fn a_message_deleted_for_everyone_becomes_its_tombstone() {
        let scene = Scene::from_view(&view(), &NotifyPrefs::default(), NOW);
        let out = chat_frames(
            &ChatChangeView {
                conv: "d:f-ayse:me".into(),
                upsert: vec![tombstone("i:4", "f-ayse")],
                remove: Vec::new(),
                reset: false,
            },
            &scene,
            &mut Sends::default(),
        );
        assert!(!out.arrived, "nothing new to read");
        let json = serde_json::to_value(&out.frames[0]).unwrap();
        assert_eq!(json["t"], "msg");
        assert_eq!(json["id"], "i:4");
        assert_eq!(json["deleted"], true);
        assert_eq!(json["text"], "");
        // It never pops, even where a message would.
        let notice = SocialNotice::ChatMessage {
            conv: "d:f-ayse:me".into(),
            message: Box::new(tombstone("i:4", "f-ayse")),
            name: Some("Ayşe".into()),
            crew: None,
        };
        assert!(toast_of(&notice, &scene, &NotifyPrefs::default(), NOW).is_none());
        // A history carries it in its place.
        let ToGame::History { msgs, .. } = history_frame(
            &ChatPage {
                conv: "d:f-ayse:me".into(),
                messages: vec![
                    message("i:3", "f-ayse", false, MessageState::Sent),
                    tombstone("i:4", "f-ayse"),
                ],
                more: false,
            },
            &scene,
        ) else {
            panic!()
        };
        assert!(!msgs[0].deleted && msgs[1].deleted && msgs[1].text.is_empty());
    }

    #[test]
    fn history_keeps_the_newest_that_went() {
        let scene = Scene::from_view(&view(), &NotifyPrefs::default(), NOW);
        let mut messages: Vec<ChatMessageView> = (0..130)
            .map(|n| message(&format!("i:{n}"), "u-cem", false, MessageState::Sent))
            .collect();
        messages.push(message("n:wait", "me", true, MessageState::Sending));
        let frame = history_frame(
            &ChatPage {
                conv: "c:t1".into(),
                messages,
                more: true,
            },
            &scene,
        );
        let ToGame::History { conv, msgs } = frame else {
            panic!()
        };
        assert_eq!(conv, "c:t1");
        assert_eq!(msgs.len(), MAX_HISTORY);
        assert_eq!(msgs[0].id, "i:30");
        assert_eq!(msgs.last().unwrap().id, "i:129");
        assert_eq!(msgs[0].from, "Cem");
        assert!(msgs[0].conv.is_none());
    }

    #[test]
    fn notices_become_toasts_by_the_rules() {
        let v = view();
        let scene = Scene::from_view(&v, &NotifyPrefs::default(), NOW);
        let prefs = NotifyPrefs::default();
        let dm = SocialNotice::ChatMessage {
            conv: "d:f-ayse:me".into(),
            message: Box::new(message("i:7", "f-ayse", false, MessageState::Sent)),
            name: Some("Ayşe".into()),
            crew: None,
        };
        let toast = toast_of(&dm, &scene, &prefs, NOW).unwrap();
        assert_eq!(
            (toast.title.as_str(), toast.body.as_str()),
            ("Ayşe", "metin i:7")
        );

        let crew_chat = SocialNotice::ChatMessage {
            conv: "c:t1".into(),
            message: Box::new(message("i:8", "u-cem", false, MessageState::Sent)),
            name: None,
            crew: Some("t1".into()),
        };
        assert!(
            toast_of(&crew_chat, &scene, &prefs, NOW).is_none(),
            "crew chat is quiet by default"
        );
        let loud = NotifyPrefs {
            crew_chat: true,
            ..NotifyPrefs::default()
        };
        let toast = toast_of(&crew_chat, &scene, &loud, NOW).unwrap();
        assert_eq!(
            (toast.title.as_str(), toast.crew.as_deref()),
            ("Cem", Some("Gece Kuşları"))
        );

        let call = SocialNotice::CrewCall {
            call: Box::new(v.calls[0].clone()),
        };
        let toast = toast_of(&call, &scene, &prefs, NOW).unwrap();
        assert_eq!(
            (
                toast.key.as_str(),
                toast.title.as_str(),
                toast.body.as_str()
            ),
            ("call:k1", "Gece Kuşları", "Cem")
        );
        let mut own = v.calls[0].clone();
        own.mine = true;
        assert!(
            toast_of(
                &SocialNotice::CrewCall {
                    call: Box::new(own)
                },
                &scene,
                &prefs,
                NOW
            )
            .is_none()
        );

        let invite = SocialNotice::Invite {
            invite: v.invites[0].clone(),
        };
        assert_eq!(
            toast_of(&invite, &scene, &prefs, NOW).unwrap().title,
            "Ayşe"
        );

        // Rahatsız etmeyin keeps them all quiet; other notices are never toasts.
        let mut dnd = v.clone();
        dnd.me.as_mut().unwrap().status = StatusMode::Dnd;
        let quiet = Scene::from_view(&dnd, &prefs, NOW);
        assert!(toast_of(&dm, &quiet, &prefs, NOW).is_none());
        assert!(toast_of(&call, &quiet, &prefs, NOW).is_none());
        assert!(
            toast_of(
                &SocialNotice::FriendOnline { id: "f-can".into() },
                &scene,
                &prefs,
                NOW
            )
            .is_none()
        );

        // A blocked sender never pops.
        let blocked = SocialNotice::ChatMessage {
            conv: "d:x-bad:me".into(),
            message: Box::new(message("i:9", "x-bad", false, MessageState::Sent)),
            name: None,
            crew: None,
        };
        assert!(toast_of(&blocked, &scene, &prefs, NOW).is_none());
    }

    #[test]
    fn unknown_senders_get_a_readable_name() {
        let scene = Scene::from_view(&view(), &NotifyPrefs::default(), NOW);
        let in_dm = msg_view(
            "d:f-ayse:me",
            &message("i:1", "gone-id", false, MessageState::Sent),
            &scene,
        );
        assert_eq!(in_dm.from, "Ayşe", "the other one of a direct conversation");
        let in_crew = msg_view(
            "c:t1",
            &message("i:2", "gone-id", false, MessageState::Sent),
            &scene,
        );
        assert!(!in_crew.from.is_empty());
        let mine = msg_view(
            "c:t1",
            &message("i:3", "me", true, MessageState::Sent),
            &scene,
        );
        assert_eq!(mine.from, "Ece");
    }
}
