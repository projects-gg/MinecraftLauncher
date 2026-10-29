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
//!
//! Crew tags (TEAM6-O): the scene keeps the cards of the tags friends and crewmates wear and who
//! wears which; a mod that said `tags` hears the cards once (`badges`) and then only their
//! changes, and the crews' "Ece ekibe katıldı" lines (`note`). Friends, messages and toasts name a
//! crew by its id.

use std::collections::{BTreeMap, HashMap, HashSet};

use pl_lang::Lang;
use pl_social::{
    ActivityView, AuthPhase, CallAnswer as SocialAnswer, CardState, ChatChangeView, ChatExtrasView,
    ChatMessageView, ChatPage, Connection, CrewBadgeView, GameLocation, MeView, MessageState,
    PermissionLevel, PresenceState, SendFailure as SocialFailure, SocialNotice, SocialView,
    StatusMode, TEAM_FEATURE_BADGES, TEAM_FEATURE_CALLS, TEAM_FEATURE_CREW_FRIEND_REQUESTS,
    TEAM_FEATURE_ROLES, TeamMemberView, TeamRole, TeamView,
};

use crate::links::{self, LinkLists};
use crate::policy::{self, CrewMute, CrewSettings, Mute, Notice, NotifyPrefs, Situation};
use crate::protocol::{
    AnswerCounts, BadgeView, CallAnswer, CallView, CardView, ConvKind, ConvView, CrewItem,
    CrewPlace, CrewRoles, FEATURE_INVITE, FEATURE_PEOPLE, FEATURE_PEOPLE2, FEATURE_PEOPLE3,
    FEATURE_TAGS, FriendView, GroupItem, LinkView, MAX_HISTORY, MAX_LAUNCHER_FRAME,
    MAX_QUOTE_CHARS, MateRights, Me, MsgView, NoteKind, PROTO, Place, ReactionView, ReplyView,
    SendFailure, ToGame, ToastView, WorldMark,
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
    /// What chat offers beyond CHAT1 now (CHAT2): the service took it and Studio has it on.
    pub extras: Option<ChatExtrasView>,
    /// Studio's link lists while links are on (CHAT2): which links of a message may be pressed.
    pub links: Option<LinkLists>,
    /// Crew tags are on (TEAM6): the service offers them and Studio has them on.
    pub tags: bool,
    /// The cards of the tags friends and crewmates wear, by crew id (only while `tags`; a hidden
    /// crew's never comes from the service).
    pub badges: BTreeMap<String, BadgeView>,
    /// The crews' "Ece ekibe katıldı" lines, oldest first (TEAM6 §7).
    pub joins: Vec<JoinLine>,
    /// The crew whose tag the player wears (OVL-UX; its card is in `badges`; only while `tags`).
    pub me_badge: Option<String>,
    /// What the person menu's rows choose from (OVL-UX2, `lists`).
    pub lists: PeopleLists,
    /// The player's crewmates (not the player): the person menu of crew chat's senders may act
    /// on them too (OVL-UX2).
    crewmates: HashSet<String>,
    /// User id → the crew whose tag they wear (friends and crewmates; only while `tags`).
    wears: HashMap<String, String>,
    /// Friend id → their Discord web profile, for "Discord'dan mesaj at" from the game (OVL-UX).
    discord: HashMap<String, String>,
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

/// "Ece ekibe katıldı" in a crew's chat, as the mod hears it (`note`, TEAM6-O T7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinLine {
    pub conv: String,
    /// Stays the same for the same arrival (who and when).
    pub id: String,
    pub name: String,
    pub mine: bool,
    /// Unix milliseconds.
    pub at: i64,
}

impl JoinLine {
    fn frame(&self) -> ToGame {
        ToGame::Note {
            conv: self.conv.clone(),
            id: self.id.clone(),
            kind: NoteKind::Joined,
            name: self.name.clone(),
            mine: self.mine,
            at: self.at,
        }
    }
}

/// A crew tag's card for the mod: the month it was founded (`YYYY-MM`, the mod words it) and
/// whether the player is in that crew.
fn badge_view(badge: &CrewBadgeView, view: &SocialView) -> BadgeView {
    let month = badge.created_on.get(..7).filter(|month| {
        let bytes = month.as_bytes();
        bytes[4] == b'-'
            && bytes[..4].iter().all(u8::is_ascii_digit)
            && bytes[5..].iter().all(u8::is_ascii_digit)
    });
    BadgeView {
        tag: badge.tag.clone(),
        color: badge.color.clone(),
        frame: badge.frame.clone(),
        emblem: badge.emblem.clone(),
        name: badge.name.clone(),
        motto: badge.motto.trim().to_owned(),
        since: month.unwrap_or_default().to_owned(),
        members: badge.members,
        mine: view.crews.iter().any(|crew| crew.id == badge.crew),
    }
}

/// "Birlikte oyna" can go now (OVL-UX; the launcher's `canInvite`): the player's game is on
/// Projects, or on another server they show their friends.
fn may_invite(view: &SocialView, me: &MeView) -> bool {
    match &view.activity {
        ActivityView::Playing { host: Some(host) } => {
            pl_social::is_projects_host(host) || me.privacy.share_server
        }
        _ => false,
    }
}

/// A Discord profile the launcher would open: on the web, over `https` (OVL-UX).
fn discord_profile(url: &str) -> bool {
    url.starts_with("https://")
}

/// The crews' "Ece ekibe katıldı" lines in their chats, oldest first; none of someone the player
/// blocked (their own always shows), none where the crew has no conversation here.
fn join_lines(view: &SocialView, blocked: &HashSet<String>) -> Vec<JoinLine> {
    let Some(chat) = view.chat.as_ref().filter(|_| view.teams) else {
        return Vec::new();
    };
    view.crew_joins
        .iter()
        .filter_map(|join| {
            let conv = chat
                .conversations
                .iter()
                .find(|conv| conv.crew.as_deref() == Some(join.crew.as_str()))?;
            let shut = blocked.contains(&join.id)
                || view.crews.iter().any(|crew| {
                    crew.members
                        .iter()
                        .any(|member| member.id == join.id && member.blocked)
                });
            (join.mine || !shut).then(|| JoinLine {
                conv: conv.id.clone(),
                id: format!("j:{}:{}", join.id, join.at),
                name: join.name.clone(),
                mine: join.mine,
                at: join.at.saturating_mul(1000),
            })
        })
        .collect()
}

/// What the person menu's rows choose from (OVL-UX2, `lists`): the player's friend categories, the
/// crews they may invite friends to, and crewmates' friend requests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeopleLists {
    pub groups: Vec<GroupItem>,
    pub crews: Vec<CrewItem>,
    /// Crewmates may ask each other to be friends (the service offers it).
    pub crewmates: bool,
    /// Crewmates who asked the player to be friends.
    pub asked: Vec<String>,
    /// Crewmates the player asked.
    pub asking: Vec<String>,
    /// Crewmates who are not friends, playing on Projects now ("Katıl"; Q15).
    pub projects: Vec<String>,
    /// Crewmates who are not friends, whose Discord profile is known (Q15).
    pub discord: Vec<String>,
    /// OVL-UX3 (R1): the player's rights over crewmates, by crew ([`crew_roles`]).
    pub roles: BTreeMap<String, CrewRoles>,
}

impl PeopleLists {
    /// The frame, with the crews' rights only for a mod that has the crew rows (`people3`).
    fn frame(&self, people3: bool) -> ToGame {
        ToGame::Lists {
            groups: self.groups.clone(),
            crews: self.crews.clone(),
            crewmates: self.crewmates,
            asked: self.asked.clone(),
            asking: self.asking.clone(),
            projects: self.projects.clone(),
            discord: self.discord.clone(),
            roles: if people3 {
                self.roles.clone()
            } else {
                BTreeMap::new()
            },
        }
    }
}

/// The most moderators a crew may have besides its leader (the service's limit; the launcher's
/// `MAX_MODERATORS`).
pub const MAX_MODERATORS: usize = 4;

/// Whether the player may remove `member` from `crew` (the launcher's `canRemove`): never the
/// leader nor themself; the leader anyone else; with roles on, a moderator whose crew lets
/// moderators remove members a plain member.
pub fn may_remove(crew: &TeamView, member: &TeamMemberView, roles: bool) -> bool {
    if member.me || member.role == TeamRole::Founder {
        return false;
    }
    crew.role == TeamRole::Founder
        || (roles && !member.moderator && allows(crew.permissions.kick, crew))
}

/// The player's rights over crewmates, by crew (OVL-UX3, `lists.roles`), the launcher's rules
/// (`person-menu.tsx`): the leader hands the lead on and removes anyone, and names or takes back
/// moderators while the service offers roles ("Moderatör yap" dimmed at [`MAX_MODERATORS`]); a
/// moderator whose crew lets moderators remove members removes plain ones ([`may_remove`]). Only
/// crews with one of these rights, only the crewmates a row may be about, only while crews are on.
fn crew_roles(view: &SocialView) -> BTreeMap<String, CrewRoles> {
    if !view.teams {
        return BTreeMap::new();
    }
    let roles = view.team_features.iter().any(|f| f == TEAM_FEATURE_ROLES);
    let mut out = BTreeMap::new();
    for crew in &view.crews {
        let lead = crew.role == TeamRole::Founder;
        let mates: BTreeMap<String, MateRights> = crew
            .members
            .iter()
            .filter(|member| !member.me && member.role != TeamRole::Founder)
            .map(|member| {
                let rights = MateRights {
                    moderator: member.moderator,
                    kick: may_remove(crew, member, roles),
                };
                (member.id.clone(), rights)
            })
            .filter(|(_, rights)| lead || rights.kick)
            .collect();
        if !lead && mates.is_empty() {
            continue;
        }
        let moderators = crew
            .members
            .iter()
            .filter(|member| member.moderator)
            .count();
        out.insert(
            crew.id.clone(),
            CrewRoles {
                lead,
                roles: lead && roles,
                mods_full: lead && roles && moderators >= MAX_MODERATORS,
                mates,
            },
        );
    }
    out
}

/// Whether the player's place in `crew` has a right at `level` (the launcher's `allows`): the
/// leader has every right; the others as the level says.
fn allows(level: PermissionLevel, crew: &TeamView) -> bool {
    crew.role == TeamRole::Founder
        || level == PermissionLevel::Members
        || (level == PermissionLevel::Moderators && crew.moderator)
}

/// The crews the player may invite friends to, in the launcher's order (its `inviteTargets`):
/// only while crews are on.
fn invite_crews(view: &SocialView) -> Vec<&TeamView> {
    if !view.teams {
        return Vec::new();
    }
    view.crews
        .iter()
        .filter(|crew| allows(crew.permissions.invite, crew))
        .collect()
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
        // Crew tags (TEAM6): the service offers them and Studio has them on. Switched off, a new
        // greeting tells the mod and it hides every chip at once.
        let tags = view.teams && view.team_features.iter().any(|f| f == TEAM_FEATURE_BADGES);
        if tags {
            features.push(FEATURE_TAGS.to_owned());
        }
        let mut badges: BTreeMap<String, BadgeView> = BTreeMap::new();
        let mut wears: HashMap<String, String> = HashMap::new();
        if tags {
            let mut worn: HashMap<&str, Option<&CrewBadgeView>> = HashMap::new();
            for crew in &view.crews {
                for member in crew.members.iter().filter(|member| !member.me) {
                    worn.insert(&member.id, member.badge.as_deref());
                }
            }
            // The friend list's word wins, as it does for names.
            for friend in &view.friends {
                worn.insert(&friend.id, friend.badge.as_deref());
            }
            for (id, badge) in worn {
                if let Some(badge) = badge.filter(|badge| !badge.crew.is_empty()) {
                    badges
                        .entry(badge.crew.clone())
                        .or_insert_with(|| badge_view(badge, view));
                    wears.insert(id.to_owned(), badge.crew.clone());
                }
            }
        }
        // The tag the player wears (OVL-UX): before their own name in the panel's header. A hidden
        // crew's never comes from the service.
        let me_badge = me
            .badge
            .as_deref()
            .filter(|badge| tags && !badge.crew.is_empty())
            .map(|badge| {
                badges
                    .entry(badge.crew.clone())
                    .or_insert_with(|| badge_view(badge, view));
                badge.crew.clone()
            });
        // CHAT2's extras, each on its own (Studio may switch one off at once).
        let extras = view
            .chat
            .as_ref()
            .filter(|chat| chat.on)
            .and_then(|chat| chat.extras);
        if let Some(extras) = extras {
            for (on, name) in [
                (extras.replies, "replies"),
                (extras.reactions, "reactions"),
                (extras.stickers, "stickers"),
                (extras.typing, "typing"),
                (extras.reads, "reads"),
                (extras.links, "links"),
            ] {
                if on {
                    features.push(name.to_owned());
                }
            }
        }
        // The friend menu's frames (OVL-UX): this launcher takes them while signed in; "Birlikte
        // oyna" only while it can go.
        features.push(FEATURE_PEOPLE.to_owned());
        // The launcher's whole person menu (OVL-UX2): favourites, online notices, categories,
        // crew invites, crewmates' friend requests, a crewmate's "Katıl" and Discord.
        features.push(FEATURE_PEOPLE2.to_owned());
        // OVL-UX3: a crewmate's crew rows and "Yeni kategori…".
        features.push(FEATURE_PEOPLE3.to_owned());
        if may_invite(view, me) {
            features.push(FEATURE_INVITE.to_owned());
        }
        let links = view
            .chat
            .as_ref()
            .and_then(|chat| chat.links.as_ref())
            .filter(|_| extras.is_some_and(|extras| extras.links))
            .map(|lists| LinkLists {
                trusted: lists.trusted.clone(),
                blocked: lists.blocked.clone(),
                invites: lists.invites,
            });
        let reads = extras.is_some_and(|extras| extras.reads);

        // OVL-UX2: what the person menu chooses from.
        let crewmates: HashSet<String> = view
            .crews
            .iter()
            .flat_map(|crew| crew.members.iter())
            .filter(|member| !member.me)
            .map(|member| member.id.clone())
            .collect();
        let inviting = invite_crews(view);
        let groups: Vec<GroupItem> = view
            .groups
            .as_ref()
            .map(|doc| {
                doc.groups
                    .iter()
                    .map(|group| GroupItem {
                        id: group.id.clone(),
                        name: group.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let group_of: HashMap<&str, &str> = view
            .groups
            .as_ref()
            .map(|doc| {
                doc.members
                    .iter()
                    .filter(|member| groups.iter().any(|group| group.id == member.group))
                    .map(|member| (member.friend.as_str(), member.group.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let crew_requests = view.teams
            && view
                .team_features
                .iter()
                .any(|f| f == TEAM_FEATURE_CREW_FRIEND_REQUESTS);
        let crewmate_requests = |requests: &[pl_social::RequestView]| -> Vec<String> {
            if !crew_requests {
                return Vec::new();
            }
            requests
                .iter()
                .filter(|request| crewmates.contains(&request.id))
                .map(|request| request.id.clone())
                .collect()
        };
        // Q15: crewmates who are not friends, as the launcher's person menu reads them (its
        // `presence = friend ?? member`): "Katıl" while they play on Projects (never for a blocked
        // one), "Discord'dan mesaj at" while their profile is known. Each once, in the crews' order.
        let friend_ids: HashSet<&str> = view.friends.iter().map(|f| f.id.as_str()).collect();
        let mut mates: Vec<&pl_social::TeamMemberView> = Vec::new();
        for member in view.crews.iter().flat_map(|crew| crew.members.iter()) {
            if !member.me
                && !friend_ids.contains(member.id.as_str())
                && !mates.iter().any(|mate| mate.id == member.id)
            {
                mates.push(member);
            }
        }
        let blocked_ids: HashSet<&str> = view.blocked.iter().map(|b| b.id.as_str()).collect();
        let lists = PeopleLists {
            groups: groups.clone(),
            crews: inviting
                .iter()
                .map(|crew| CrewItem {
                    id: crew.id.clone(),
                    name: crew.name.clone(),
                    full: crew.members.len() >= usize::try_from(crew.limit).unwrap_or(usize::MAX),
                })
                .collect(),
            crewmates: crew_requests,
            asked: crewmate_requests(&view.incoming),
            asking: crewmate_requests(&view.outgoing),
            projects: mates
                .iter()
                .filter(|mate| !mate.blocked && !blocked_ids.contains(mate.id.as_str()))
                .filter(|mate| place_of(mate.state, mate.game.as_ref()).0 == Place::Projects)
                .map(|mate| mate.id.clone())
                .collect(),
            discord: mates
                .iter()
                .filter(|mate| discord_profile(&mate.discord_url))
                .map(|mate| mate.id.clone())
                .collect(),
            roles: crew_roles(view),
        };

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
                    world: None,
                    badge: wears.get(&friend.id).cloned(),
                    // Friends write to each other (a new conversation too) while chat is on.
                    dm: chat_on.then(|| pl_social::direct_conversation(&me.id, &friend.id)),
                    discord: discord_profile(&friend.discord_url),
                    // The launcher's list says it only under an offline friend (`presenceLine`).
                    seen: friend.last_seen.filter(|_| place == Place::Offline),
                    fav: prefs.favorite(&friend.id),
                    alert: prefs.alerts(&friend.id),
                    group: group_of
                        .get(friend.id.as_str())
                        .map(|group| (*group).to_owned()),
                    crews: inviting
                        .iter()
                        .filter_map(|crew| {
                            let place = if crew.members.iter().any(|member| member.id == friend.id)
                            {
                                CrewPlace::Member
                            } else if crew.invited.contains(&friend.id) {
                                CrewPlace::Invited
                            } else {
                                return None;
                            };
                            Some((crew.id.clone(), place))
                        })
                        .collect(),
                }
            })
            .collect();
        // Friends' profiles, and crewmates' who are not friends (their menu in crew chat, Q15).
        let discord: HashMap<String, String> = view
            .friends
            .iter()
            .map(|friend| (&friend.id, &friend.discord_url))
            .chain(mates.iter().map(|mate| (&mate.id, &mate.discord_url)))
            .filter(|(_, url)| discord_profile(url))
            .map(|(id, url)| (id.clone(), url.clone()))
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
                    seen: conv.peer_read.filter(|_| reads && crew.is_none()),
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

        let blocked: HashSet<String> = view.blocked.iter().map(|b| b.id.clone()).collect();
        let joins = join_lines(view, &blocked);

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
            extras,
            links,
            tags,
            badges,
            joins,
            me_badge,
            lists,
            crewmates,
            wears,
            discord,
            names,
            crew_names,
            blocked,
            invites,
            shared,
        }
    }

    /// A friend's Discord web profile ("Discord'dan mesaj at" from the game; OVL-UX), or None.
    pub fn discord_url(&self, id: &str) -> Option<&str> {
        self.discord.get(id).map(String::as_str)
    }

    /// "Birlikte oyna" can go now (OVL-UX).
    pub fn may_invite(&self) -> bool {
        self.features
            .iter()
            .any(|feature| feature == FEATURE_INVITE)
    }

    /// The crew whose tag `id` wears, as friends and crewmates see it (None while tags are off).
    pub fn badge_of(&self, id: &str) -> Option<&str> {
        self.wears.get(id).map(String::as_str)
    }

    /// Someone wears another tag than before, or none any more: messages the panel shows name the
    /// old one (they carry it), so its conversation comes again.
    pub fn wears_differ(&self, other: &Self) -> bool {
        self.wears != other.wears
    }

    /// A crew's conversation (`c:` ids; a listed conversation says so itself).
    pub(crate) fn is_crew_conv(&self, conv: &str) -> bool {
        self.convs
            .iter()
            .find(|one| one.id == conv)
            .map_or_else(|| conv.starts_with("c:"), |one| one.kind == ConvKind::Crew)
    }

    /// CHAT2 offers this extra now.
    pub fn offers(&self, extra: impl Fn(&ChatExtrasView) -> bool) -> bool {
        self.extras.as_ref().is_some_and(extra)
    }

    /// HOST1 (H13): the friends who invited the player to their open world, with its count: their
    /// rows say so and "Katıl" works (a world join). Nobody else learns that a world is open.
    pub fn mark_worlds(&mut self, marks: &HashMap<String, WorldMark>) {
        for friend in &mut self.friends {
            friend.world = marks.get(&friend.id).copied();
            if friend.world.is_some() {
                friend.joinable = true;
            }
        }
    }

    /// Where "Bağlan" may send the running game: `target` (`host[:port]`) when it is Projects or
    /// an address a friend or crew shares right now, else None.
    pub fn connect_target(&self, target: &str) -> Option<(String, u16)> {
        let (host, port) = server_address(target)?;
        (pl_social::is_projects_host(&host) || self.shared.contains(&(host.clone(), port)))
            .then_some((host, port))
    }

    /// `id` is in one of the player's crews (not the player): crew chat's senders (OVL-UX2).
    pub fn is_crewmate(&self, id: &str) -> bool {
        self.crewmates.contains(id)
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
    /// title for a direct conversation (the sender is the other one), else "someone" in the
    /// overlay's language.
    pub(crate) fn sender(&self, conv: &str, from: &str, english: bool) -> String {
        if let Some(name) = self.name_of(from) {
            return name.to_owned();
        }
        if conv.starts_with("d:")
            && let Some(conv) = self.convs.iter().find(|one| one.id == conv)
            && !conv.title.is_empty()
        {
            return conv.title.clone();
        }
        if english { "Someone" } else { "Biri" }.to_owned()
    }
}

/// What the greeting said: a change sends a new one (and a fresh snapshot).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Identity {
    me_id: String,
    me: String,
    /// The tag the player wears (OVL-UX): another one (or none) greets again.
    me_badge: Option<String>,
    features: Vec<String>,
    reduced_motion: bool,
    /// The overlay's language (OVL-LANG): another one greets again.
    lang: Option<Lang>,
}

/// What the live connection was told, for the next differences.
#[derive(Debug, Clone, Default)]
pub struct Mirror {
    identity: Option<Identity>,
    /// The language the mod's words follow (OVL-LANG: `lang` in its hello), none for an older mod.
    lang: Option<Lang>,
    friends: BTreeMap<String, FriendView>,
    convs: BTreeMap<String, ConvView>,
    calls: BTreeMap<String, CallView>,
    dnd: bool,
    /// The mod draws crew tags (`tags` in its hello): it hears `badges` and `note` (TEAM6-O).
    tags: bool,
    badges: BTreeMap<String, BadgeView>,
    /// The "ekibe katıldı" lines it heard.
    joins: HashSet<String>,
    /// The mod has the launcher's whole person menu (`people2` in its hello): it hears `lists`
    /// (OVL-UX2).
    people2: bool,
    /// The mod has the crew rows (`people3` in its hello): its `lists` carry `roles` (OVL-UX3).
    people3: bool,
    lists: Option<PeopleLists>,
}

impl Mirror {
    /// For a mod that knows TEAM6-O's frames (`tags` in its hello) and OVL-UX2's lists
    /// (`people2`), or not.
    pub fn for_mod(tags: bool, people2: bool) -> Self {
        Self {
            tags,
            people2,
            ..Self::default()
        }
    }

    /// For a mod that also has OVL-UX3's crew rows (`people3` in its hello), or not.
    pub fn with_people3(mut self, people3: bool) -> Self {
        self.people3 = people3;
        self
    }

    /// The language the mod's words follow from now on (OVL-LANG; a mod whose hello lists
    /// `lang`): another one than the greeting named greets again in the next update.
    pub fn set_lang(&mut self, lang: Option<Lang>) {
        self.lang = lang;
    }

    /// `welcome` and `snapshot`: everything after a handshake or a change of account. A mod that
    /// draws tags hears every card first (`badges`, the whole set) and the crews' join lines after.
    pub fn greet(&mut self, scene: &Scene, launcher: &str, reduced_motion: bool) -> Vec<ToGame> {
        let identity = Identity {
            me_id: scene.me_id.clone(),
            me: scene.me.clone(),
            me_badge: scene.me_badge.clone(),
            features: if scene.signed_in {
                scene.features.clone()
            } else {
                Vec::new()
            },
            reduced_motion,
            lang: self.lang,
        };
        let welcome = ToGame::Welcome {
            proto: PROTO,
            launcher: launcher.to_owned(),
            me: Me {
                name: identity.me.clone(),
                badge: identity.me_badge.clone(),
            },
            dnd: scene.dnd,
            reduced_motion,
            features: identity.features.clone(),
            lang: identity.lang,
        };
        let (friends, convs, calls) = fitting_snapshot(scene);
        let tags = self.tags;
        let people2 = self.people2;
        let people3 = self.people3;
        let lang = self.lang;
        *self = Self {
            identity: Some(identity),
            lang,
            friends: friends.iter().map(|f| (f.id.clone(), f.clone())).collect(),
            convs: convs.iter().map(|c| (c.id.clone(), c.clone())).collect(),
            calls: calls.iter().map(|c| (c.id.clone(), c.clone())).collect(),
            dnd: scene.dnd,
            tags,
            badges: BTreeMap::new(),
            joins: HashSet::new(),
            people2,
            people3,
            lists: None,
        };
        let snapshot = ToGame::Snapshot {
            friends,
            convs,
            calls,
            dnd: scene.dnd,
        };
        let mut frames = vec![welcome];
        if tags {
            self.badges = scene.badges.clone();
            let cards = scene
                .badges
                .iter()
                .map(|(crew, card)| (crew.clone(), Some(card.clone())))
                .collect();
            frames.push(ToGame::Badges { all: true, cards });
        }
        // The lists before the friends that name their categories and crews (OVL-UX2).
        if people2 && scene.signed_in {
            self.lists = Some(scene.lists.clone());
            frames.push(scene.lists.frame(people3));
        }
        frames.push(snapshot);
        if tags {
            for join in &scene.joins {
                self.joins.insert(join.id.clone());
                frames.push(join.frame());
            }
        }
        frames
    }

    /// The differences since the mod last heard, or a new greeting.
    pub fn update(&mut self, scene: &Scene, launcher: &str, reduced_motion: bool) -> Vec<ToGame> {
        let identity_changed = self.identity.as_ref().is_none_or(|known| {
            known.me_id != scene.me_id
                || known.me != scene.me
                || known.me_badge != scene.me_badge
                || known.reduced_motion != reduced_motion
                || known.lang != self.lang
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
        if self.tags && self.badges != scene.badges {
            // The cards first: the friends that follow may name a new one.
            let mut cards: BTreeMap<String, Option<BadgeView>> = scene
                .badges
                .iter()
                .filter(|(crew, card)| self.badges.get(*crew) != Some(*card))
                .map(|(crew, card)| (crew.clone(), Some(card.clone())))
                .collect();
            for crew in self.badges.keys() {
                if !scene.badges.contains_key(crew) {
                    cards.insert(crew.clone(), None);
                }
            }
            self.badges = scene.badges.clone();
            frames.push(ToGame::Badges { all: false, cards });
        }
        if self.people2 && scene.signed_in && self.lists.as_ref() != Some(&scene.lists) {
            self.lists = Some(scene.lists.clone());
            frames.push(scene.lists.frame(self.people3));
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
        if self.tags {
            for join in &scene.joins {
                if self.joins.insert(join.id.clone()) {
                    frames.push(join.frame());
                }
            }
        }
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

fn failure_of(failure: Option<SocialFailure>, listener: Listener) -> SendFailure {
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
        // CHAT2's own daily limits and switches: a mod that knows them hears them, an older one
        // the nearest reason it knows.
        Some(SocialFailure::Stickers) if listener.chat2 => SendFailure::Stickers,
        Some(SocialFailure::Links) if listener.chat2 => SendFailure::Links,
        Some(SocialFailure::LimitedLinks) if listener.chat2 => SendFailure::LimitedLinks,
        Some(SocialFailure::FeatureOff) if listener.chat2 => SendFailure::FeatureOff,
        Some(SocialFailure::Stickers | SocialFailure::Links) => SendFailure::DailyLimit,
        Some(SocialFailure::LimitedLinks) => SendFailure::Link,
        Some(SocialFailure::FeatureOff) => SendFailure::Off,
        Some(SocialFailure::Connection) | None => SendFailure::Offline,
    }
}

/// Who hears the frames: the overlay's language (the words the launcher writes for it: a sticker
/// in words, a sender nobody named) and whether the mod knows CHAT2 (`chat2` in its hello: it hears
/// the precise reasons).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Listener {
    pub english: bool,
    pub chat2: bool,
}

impl Listener {
    /// From the mod's hello: its features and its language. A mod whose words follow the launcher
    /// (OVL-LANG: `lang` among its features) shows `overlay`, the overlay's language; an older one
    /// says its own (the game's: `tr_tr`, `en_us`; `overlay` when it says none).
    pub fn from_hello(language: &str, features: &[String], overlay: Lang) -> Self {
        let follows = features
            .iter()
            .any(|feature| feature == crate::protocol::FEATURE_LANG);
        let english = if follows || language.trim().is_empty() {
            overlay == Lang::En
        } else {
            !language.trim().to_ascii_lowercase().starts_with("tr")
        };
        Self {
            english,
            chat2: features
                .iter()
                .any(|feature| feature == crate::protocol::FEATURE_CHAT2),
        }
    }
}

/// A sticker as a toast says it: "Çıkartma · GG" ("Çıkartma" for one of a set this launcher does
/// not have).
fn sticker_words(reference: &str, english: bool) -> String {
    let word = if english { "Sticker" } else { "Çıkartma" };
    match pl_social::sticker(reference) {
        Some(found) => {
            let name = if english {
                &found.name_en
            } else {
                &found.name_tr
            };
            format!("{word} · {name}")
        }
        None => word.to_owned(),
    }
}

/// At most `max` characters of `text`, with an ellipsis when it was longer.
fn clip(text: &str, max: usize) -> String {
    let mut chars = text.chars();
    let kept: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{}…", kept.trim_end())
    } else {
        kept
    }
}

/// Names of reactors a reaction shows besides the count.
const REACTION_NAMES: usize = 3;

/// A message as the mod shows it.
pub fn msg_view(
    conv: &str,
    message: &ChatMessageView,
    scene: &Scene,
    listener: Listener,
) -> MsgView {
    let deleted = message.deleted.is_some();
    let sticker = message.sticker.clone().filter(|_| !deleted);
    let text = if deleted {
        // A tombstone (deleted for everyone) carries no text; the mod says "Bu mesaj silindi".
        String::new()
    } else if let Some(sticker) = &sticker {
        pl_social::sticker_text(sticker, listener.english)
    } else {
        message.text.clone()
    };
    let links: Vec<LinkView> = match &scene.links {
        Some(lists) if !deleted && sticker.is_none() => links::pressable(&message.text, lists)
            .into_iter()
            .map(|link| LinkView {
                url: link.url,
                trusted: link.trusted,
            })
            .collect(),
        _ => Vec::new(),
    };
    // The first link's card, while that link may still be pressed (a card for a link the rules
    // turned back into text is not shown).
    let card = message
        .card
        .as_ref()
        .filter(|card| card.state == CardState::Ready)
        .filter(|card| links.first().is_some_and(|link| link.url == card.url))
        .and_then(|card| {
            let title = card.title.as_deref().map(str::trim).unwrap_or_default();
            (!title.is_empty()).then(|| CardView {
                site: clip(&card.site, 48),
                title: clip(title, MAX_QUOTE_CHARS),
            })
        });
    let reply = message.reply.as_ref().filter(|_| !deleted).map(|reply| {
        let gone = reply.missing || scene.is_blocked(&reply.from);
        let mine = !gone && reply.from == scene.me_id;
        ReplyView {
            id: reply.key.clone().filter(|_| !gone),
            sid: reply.id,
            from: if gone {
                String::new()
            } else if mine {
                scene.me.clone()
            } else {
                scene.sender(conv, &reply.from, listener.english)
            },
            mine,
            text: reply
                .text
                .as_deref()
                .filter(|_| !gone && !reply.deleted && reply.sticker.is_none())
                .map(|text| clip(text, MAX_QUOTE_CHARS)),
            sticker: reply.sticker.clone().filter(|_| !gone && !reply.deleted),
            deleted: !gone && reply.deleted,
            missing: gone,
        }
    });
    let reactions = if deleted {
        Vec::new()
    } else {
        message
            .reactions
            .iter()
            .filter(|reaction| !reaction.by.is_empty())
            .map(|reaction| ReactionView {
                e: reaction.e.clone(),
                n: u32::try_from(reaction.by.len()).unwrap_or(u32::MAX),
                mine: reaction.mine,
                who: reaction
                    .by
                    .iter()
                    .filter(|id| **id != scene.me_id)
                    .filter_map(|id| scene.name_of(id))
                    .take(REACTION_NAMES)
                    .map(str::to_owned)
                    .collect(),
            })
            .collect()
    };
    MsgView {
        conv: Some(conv.to_owned()),
        id: message.key.clone(),
        cid: None,
        from: if message.mine {
            scene.me.clone()
        } else {
            scene.sender(conv, &message.from, listener.english)
        },
        text,
        at: message.at,
        mine: message.mine,
        edited: message.edited && !deleted,
        deleted,
        sid: message.id,
        sticker,
        reply,
        reactions,
        links,
        card,
        // The chip before a name over someone's group: only crew chat names its senders.
        badge: if message.mine || !scene.is_crew_conv(conv) {
            None
        } else {
            scene.badge_of(&message.from).map(str::to_owned)
        },
        // Their name opens their person menu (OVL-UX2): only crew chat names its senders.
        by: (!message.mine && scene.is_crew_conv(conv) && !message.from.is_empty())
            .then(|| message.from.clone()),
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

/// The name of a link rule as the service says it (`shortener`).
fn rule_name(rule: pl_social::LinkRule) -> Option<String> {
    serde_json::to_value(rule)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
}

/// Whether the panel should ask for a message's card (CHAT2 §9.3): links are on, it went, it is
/// not a sticker nor deleted, it has none yet, its first link may be pressed, and it is younger
/// than a week (the service's mailbox; older ones are never asked for).
pub fn wants_card(message: &ChatMessageView, scene: &Scene, now_ms: i64) -> bool {
    let Some(lists) = &scene.links else {
        return false;
    };
    message.state == MessageState::Sent
        && message.deleted.is_none()
        && message.sticker.is_none()
        && message.card.is_none()
        && message.at > now_ms - pl_social::CARD_WINDOW_MS
        && pl_social::first_link(&message.text)
            .is_some_and(|url| links::problem(url, lists).is_none())
}

pub fn chat_frames(
    change: &ChatChangeView,
    scene: &Scene,
    sends: &mut Sends,
    listener: Listener,
) -> ChatOutcome {
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
                        sid: message.id,
                    });
                    // What the panel cannot work out itself follows (the quote as this computer
                    // holds it, the links it may press): the same message again, by its id.
                    if listener.chat2 {
                        out.frames.push(ToGame::Msg(Box::new(msg_view(
                            conv, message, scene, listener,
                        ))));
                    }
                }
                MessageState::Failed => {
                    sends.by_key.remove(&message.key);
                    sends.dropped.insert(message.key.clone());
                    let reason = failure_of(message.failure, listener);
                    out.frames.push(ToGame::SendFailed {
                        cid,
                        reason,
                        rule: message
                            .failure_rule
                            .filter(|_| listener.chat2 && reason == SendFailure::Link)
                            .and_then(rule_name),
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
            out.frames.push(ToGame::Msg(Box::new(msg_view(
                conv, message, scene, listener,
            ))));
        }
    }
    out
}

/// The answer to `open-conv`: the newest messages that went, oldest first.
pub fn history_frame(page: &ChatPage, scene: &Scene, listener: Listener) -> ToGame {
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
                ..msg_view(&page.conv, message, scene, listener)
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
    listener: Listener,
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
                .unwrap_or_else(|| scene.sender(conv, &message.from, listener.english));
            // A sticker is told in words ("Çıkartma · GG"); a mod that knows it draws it too.
            let text = match &message.sticker {
                Some(sticker) => sticker_words(sticker, listener.english),
                None => message.text.clone(),
            };
            policy::toast_for(
                &Notice::Message {
                    conv,
                    crew: crew_name,
                    from: &from,
                    text: &text,
                    mine: message.mine,
                    blocked: scene.is_blocked(&message.from),
                },
                &situation,
            )
            .map(|toast| ToastView {
                sticker: message.sticker.clone().filter(|_| listener.chat2),
                // "[GK] Ayşe" in the card's title: a friend's or a crewmate's tag.
                badge: scene.badge_of(&message.from).map(str::to_owned),
                ..toast
            })
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
        // Cloud AFK's notices (AFKPLUS): Cloud AFK already chose them by the player's own switches;
        // do-not-disturb is a social status and does not apply.
        SocialNotice::AfkNotice { notice } => crate::afk::afk_toast(notice, listener.english),
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
                share_read: None,
                share_typing: None,
            },
            backup: None,
            limited: None,
            badge: None,
            mark: None,
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
            badge: None,
            mark: None,
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
                    badge: None,
                    mark: None,
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
            hidden: false,
            badge_requests: true,
            wearers: 0,
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
            peer_read: None,
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
                mark: None,
            }],
            invites: vec![InviteView {
                id: "inv-1".into(),
                from: "f-ayse".into(),
                name: Some("Ayşe".into()),
                avatar: None,
                avatar_url: None,
                mark: None,
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
            crew_joins: Vec::new(),
            together: false,
            calls: vec![CrewCallView {
                id: "k1".into(),
                from: "u-cem".into(),
                mine: false,
                name: None,
                avatar: None,
                avatar_url: None,
                mark: None,
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
                extras: None,
                sticker_sets: Vec::new(),
                links: None,
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
            failure_rule: None,
            edit_until: None,
            delete_until: None,
            deleted: None,
            sticker: None,
            reply: None,
            reactions: Vec::new(),
            card: None,
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
        assert_eq!(
            scene.features,
            ["friends", "chat", "calls", "people", "people2", "people3"]
        );
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
            matches!(again[0], ToGame::Welcome { ref features, .. } if features == &["friends", "calls", "people", "people2", "people3"])
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
        let out = chat_frames(&change, &scene, &mut sends, Listener::default());
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
        let out = chat_frames(&sent, &scene, &mut sends, Listener::default());
        assert_eq!(
            out.frames,
            [ToGame::Sent {
                cid: "cid-1".into(),
                id: "n:abc".into(),
                sid: None,
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
            Listener::default(),
        );
        assert_eq!(
            out.frames,
            [ToGame::SendFailed {
                cid: "cid-2".into(),
                reason: SendFailure::Link,
                rule: None,
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
            Listener::default(),
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
            Listener::default(),
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
            Listener::default(),
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
        assert!(
            toast_of(
                &notice,
                &scene,
                &NotifyPrefs::default(),
                NOW,
                Listener::default()
            )
            .is_none()
        );
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
            Listener::default(),
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
            Listener::default(),
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
        let toast = toast_of(&dm, &scene, &prefs, NOW, Listener::default()).unwrap();
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
            toast_of(&crew_chat, &scene, &prefs, NOW, Listener::default()).is_none(),
            "crew chat is quiet by default"
        );
        let loud = NotifyPrefs {
            crew_chat: true,
            ..NotifyPrefs::default()
        };
        let toast = toast_of(&crew_chat, &scene, &loud, NOW, Listener::default()).unwrap();
        assert_eq!(
            (toast.title.as_str(), toast.crew.as_deref()),
            ("Cem", Some("Gece Kuşları"))
        );

        let call = SocialNotice::CrewCall {
            call: Box::new(v.calls[0].clone()),
        };
        let toast = toast_of(&call, &scene, &prefs, NOW, Listener::default()).unwrap();
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
                NOW,
                Listener::default()
            )
            .is_none()
        );

        let invite = SocialNotice::Invite {
            invite: v.invites[0].clone(),
        };
        assert_eq!(
            toast_of(&invite, &scene, &prefs, NOW, Listener::default())
                .unwrap()
                .title,
            "Ayşe"
        );

        // Rahatsız etmeyin keeps them all quiet; other notices are never toasts.
        let mut dnd = v.clone();
        dnd.me.as_mut().unwrap().status = StatusMode::Dnd;
        let quiet = Scene::from_view(&dnd, &prefs, NOW);
        assert!(toast_of(&dm, &quiet, &prefs, NOW, Listener::default()).is_none());
        assert!(toast_of(&call, &quiet, &prefs, NOW, Listener::default()).is_none());
        assert!(
            toast_of(
                &SocialNotice::FriendOnline { id: "f-can".into() },
                &scene,
                &prefs,
                NOW,
                Listener::default()
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
        assert!(toast_of(&blocked, &scene, &prefs, NOW, Listener::default()).is_none());
    }

    #[test]
    fn unknown_senders_get_a_readable_name() {
        let scene = Scene::from_view(&view(), &NotifyPrefs::default(), NOW);
        let in_dm = msg_view(
            "d:f-ayse:me",
            &message("i:1", "gone-id", false, MessageState::Sent),
            &scene,
            Listener::default(),
        );
        assert_eq!(in_dm.from, "Ayşe", "the other one of a direct conversation");
        let in_crew = msg_view(
            "c:t1",
            &message("i:2", "gone-id", false, MessageState::Sent),
            &scene,
            Listener::default(),
        );
        assert!(!in_crew.from.is_empty());
        let mine = msg_view(
            "c:t1",
            &message("i:3", "me", true, MessageState::Sent),
            &scene,
            Listener::default(),
        );
        assert_eq!(mine.from, "Ece");
    }

    /// `view()` with every CHAT2 extra on and Studio's default link lists.
    pub fn chat2_view() -> SocialView {
        let mut v = view();
        if let Some(chat) = v.chat.as_mut() {
            chat.extras = Some(ChatExtrasView {
                replies: true,
                stickers: true,
                reactions: true,
                typing: true,
                reads: true,
                links: true,
            });
            let lists = links::default_lists();
            chat.links = Some(pl_social::ChatLinksView {
                rules: links::version(),
                trusted: lists.trusted,
                blocked: lists.blocked,
                invites: true,
            });
            chat.conversations[0].peer_read = Some(5);
            chat.conversations[1].peer_read = Some(5);
        }
        v
    }

    const CHAT2: Listener = Listener {
        english: false,
        chat2: true,
    };

    #[test]
    fn chat2_extras_are_features_each_on_its_own() {
        let scene = Scene::from_view(&chat2_view(), &NotifyPrefs::default(), NOW);
        assert_eq!(
            scene.features,
            [
                "friends",
                "chat",
                "calls",
                "replies",
                "reactions",
                "stickers",
                "typing",
                "reads",
                "links",
                "people",
                "people2"
            ]
        );
        assert!(scene.offers(|e| e.typing));
        // "Görüldü" belongs to direct conversations only.
        assert_eq!(scene.convs[0].seen, Some(5));
        assert_eq!(scene.convs[1].seen, None);
        // Studio switches one off: the rest stay, the mod is greeted again.
        let mut fewer = chat2_view();
        if let Some(extras) = fewer.chat.as_mut().and_then(|chat| chat.extras.as_mut()) {
            extras.reads = false;
            extras.links = false;
        }
        let scene = Scene::from_view(&fewer, &NotifyPrefs::default(), NOW);
        assert!(!scene.features.iter().any(|f| f == "reads" || f == "links"));
        assert_eq!(scene.convs[0].seen, None);
        assert!(scene.links.is_none());
        // Chat off: none of them.
        let mut off = chat2_view();
        if let Some(chat) = off.chat.as_mut() {
            chat.on = false;
        }
        let scene = Scene::from_view(&off, &NotifyPrefs::default(), NOW);
        assert_eq!(
            scene.features,
            ["friends", "calls", "people", "people2", "people3"]
        );
    }

    #[test]
    fn a_message_carries_its_reply_reactions_links_and_card() {
        let scene = Scene::from_view(&chat2_view(), &NotifyPrefs::default(), NOW);
        let mut m = message("i:8", "u-cem", false, MessageState::Sent);
        m.text = "şuna bak https://www.youtube.com/watch?v=dQw4w9WgXcQ ve https://bit.ly/x".into();
        m.reply = Some(pl_social::ChatReplyView {
            id: 7,
            from: "me".into(),
            key: Some("n:abc".into()),
            text: Some("Ş".repeat(200)),
            sticker: None,
            deleted: false,
            missing: false,
        });
        m.reactions = vec![
            pl_social::ChatReactionView {
                e: "👍".into(),
                by: vec![
                    "me".into(),
                    "f-ayse".into(),
                    "u-cem".into(),
                    "x-unknown".into(),
                ],
                mine: true,
            },
            pl_social::ChatReactionView {
                e: "😂".into(),
                by: Vec::new(),
                mine: false,
            },
        ];
        m.card = Some(pl_social::ChatCardView {
            state: CardState::Ready,
            url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
            site: "YouTube".into(),
            icon: pl_social::CardIcon::Youtube,
            kind: pl_social::CardKind::Video,
            title: Some("  Rekor kırıldı  ".into()),
            author: None,
            desc: None,
            image: Some("data:image/jpeg;base64,AAAA".into()),
            w: Some(320),
            h: Some(180),
            rule: None,
        });
        let view = msg_view("c:t1", &m, &scene, CHAT2);
        assert_eq!(view.sid, Some(8));
        let reply = view.reply.clone().unwrap();
        assert_eq!(
            (
                reply.id.as_deref(),
                reply.sid,
                reply.from.as_str(),
                reply.mine
            ),
            (Some("n:abc"), 7, "Ece", true)
        );
        let quoted = reply.text.unwrap();
        assert_eq!(quoted.chars().count(), MAX_QUOTE_CHARS + 1);
        assert!(quoted.ends_with('…'));
        // Counts, the player's own marked, a few names of the others; an empty kind is left out.
        assert_eq!(view.reactions.len(), 1);
        assert_eq!(view.reactions[0].n, 4);
        assert!(view.reactions[0].mine);
        assert_eq!(view.reactions[0].who, ["Ayşe", "Cem"]);
        // The shortener stays text; the trusted video may be pressed, and its card shows.
        assert_eq!(
            view.links,
            [LinkView {
                url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
                trusted: true
            }]
        );
        assert_eq!(
            view.card,
            Some(CardView {
                site: "YouTube".into(),
                title: "Rekor kırıldı".into()
            })
        );
        // No picture ever reaches the game.
        assert!(
            !serde_json::to_string(&ToGame::Msg(Box::new(view)))
                .unwrap()
                .contains("data:image")
        );

        // Links switched off: no link may be pressed, no card shows.
        let mut off = chat2_view();
        if let Some(extras) = off.chat.as_mut().and_then(|chat| chat.extras.as_mut()) {
            extras.links = false;
        }
        let quiet = Scene::from_view(&off, &NotifyPrefs::default(), NOW);
        let view = msg_view("c:t1", &m, &quiet, CHAT2);
        assert!(view.links.is_empty() && view.card.is_none());
    }

    #[test]
    fn a_quote_says_what_this_computer_holds() {
        let scene = Scene::from_view(&chat2_view(), &NotifyPrefs::default(), NOW);
        let quote = |reply: pl_social::ChatReplyView| {
            let mut m = message("i:9", "f-ayse", false, MessageState::Sent);
            m.reply = Some(reply);
            msg_view("d:f-ayse:me", &m, &scene, CHAT2).reply.unwrap()
        };
        let base = pl_social::ChatReplyView {
            id: 3,
            from: "f-ayse".into(),
            key: Some("i:3".into()),
            text: Some("akşam?".into()),
            sticker: None,
            deleted: false,
            missing: false,
        };
        let theirs = quote(base.clone());
        assert_eq!((theirs.from.as_str(), theirs.mine), ("Ayşe", false));
        let sticker = quote(pl_social::ChatReplyView {
            text: None,
            sticker: Some("pl1/gg".into()),
            ..base.clone()
        });
        assert_eq!(sticker.sticker.as_deref(), Some("pl1/gg"));
        let deleted = quote(pl_social::ChatReplyView {
            deleted: true,
            ..base.clone()
        });
        assert!(deleted.deleted && deleted.text.is_none());
        // Not here any more, or from someone blocked: "Önceki bir mesaj", no name, nothing to scroll to.
        let missing = quote(pl_social::ChatReplyView {
            missing: true,
            key: None,
            ..base.clone()
        });
        assert!(missing.missing && missing.from.is_empty() && missing.id.is_none());
        let blocked = quote(pl_social::ChatReplyView {
            from: "x-bad".into(),
            ..base
        });
        assert!(blocked.missing && blocked.text.is_none() && blocked.from.is_empty());
    }

    #[test]
    fn a_sticker_is_drawn_by_a_new_mod_and_told_in_words_to_an_old_one() {
        let scene = Scene::from_view(&chat2_view(), &NotifyPrefs::default(), NOW);
        let mut m = message("i:10", "f-ayse", false, MessageState::Sent);
        m.text = String::new();
        m.sticker = Some("pl1/gg".into());
        let view = msg_view("d:f-ayse:me", &m, &scene, Listener::default());
        assert_eq!(view.sticker.as_deref(), Some("pl1/gg"));
        assert_eq!(view.text, "[Çıkartma: GG]");
        let english = msg_view(
            "d:f-ayse:me",
            &m,
            &scene,
            Listener {
                english: true,
                chat2: false,
            },
        );
        assert_eq!(english.text, "[Sticker: GG]");
        // Its toast says it in words; only a mod that knows stickers hears which one.
        let notice = SocialNotice::ChatMessage {
            conv: "d:f-ayse:me".into(),
            message: Box::new(m.clone()),
            name: Some("Ayşe".into()),
            crew: None,
        };
        let old = toast_of(
            &notice,
            &scene,
            &NotifyPrefs::default(),
            NOW,
            Listener::default(),
        )
        .unwrap();
        assert_eq!(
            (old.body.as_str(), old.sticker.as_deref()),
            ("Çıkartma · GG", None)
        );
        let new = toast_of(&notice, &scene, &NotifyPrefs::default(), NOW, CHAT2).unwrap();
        assert_eq!(new.sticker.as_deref(), Some("pl1/gg"));
        // Deleted: nothing of it is left.
        let mut gone = m;
        gone.deleted = Some(pl_social::ChatDeletedView {
            by: "f-ayse".into(),
            at: None,
        });
        let view = msg_view("d:f-ayse:me", &gone, &scene, CHAT2);
        assert!(view.sticker.is_none() && view.text.is_empty() && view.deleted);
    }

    #[test]
    fn the_panels_own_message_comes_back_whole_for_a_new_mod() {
        let scene = Scene::from_view(&chat2_view(), &NotifyPrefs::default(), NOW);
        let mut sends = Sends::default();
        sends.insert("n:abc".into(), "cid-1".into());
        let mut sent = message("n:abc", "me", true, MessageState::Sent);
        sent.id = Some(12);
        let change = ChatChangeView {
            conv: "d:f-ayse:me".into(),
            upsert: vec![sent],
            remove: Vec::new(),
            reset: false,
        };
        let out = chat_frames(&change, &scene, &mut sends, CHAT2);
        assert_eq!(
            out.frames[0],
            ToGame::Sent {
                cid: "cid-1".into(),
                id: "n:abc".into(),
                sid: Some(12)
            }
        );
        assert!(matches!(&out.frames[1], ToGame::Msg(m) if m.id == "n:abc" && m.sid == Some(12)));

        // A refusal names CHAT2's own reasons and the link rule to a mod that knows them.
        sends.insert("n:def".into(), "cid-2".into());
        let mut failed = message("n:def", "me", true, MessageState::Failed);
        failed.failure = Some(SocialFailure::Link);
        failed.failure_rule = Some(pl_social::LinkRule::Shortener);
        let mut stickers = message("n:ghi", "me", true, MessageState::Failed);
        stickers.failure = Some(SocialFailure::Stickers);
        sends.insert("n:ghi".into(), "cid-3".into());
        let change = ChatChangeView {
            conv: "d:f-ayse:me".into(),
            upsert: vec![failed.clone(), stickers.clone()],
            remove: Vec::new(),
            reset: false,
        };
        let out = chat_frames(&change, &scene, &mut sends, CHAT2);
        assert_eq!(
            out.frames,
            [
                ToGame::SendFailed {
                    cid: "cid-2".into(),
                    reason: SendFailure::Link,
                    rule: Some("shortener".into()),
                },
                ToGame::SendFailed {
                    cid: "cid-3".into(),
                    reason: SendFailure::Stickers,
                    rule: None,
                },
            ]
        );
        // An older mod hears the nearest reason it knows.
        sends.insert("n:def".into(), "cid-2".into());
        sends.insert("n:ghi".into(), "cid-3".into());
        let change = ChatChangeView {
            conv: "d:f-ayse:me".into(),
            upsert: vec![failed, stickers],
            remove: Vec::new(),
            reset: false,
        };
        let out = chat_frames(&change, &scene, &mut sends, Listener::default());
        assert_eq!(
            out.frames,
            [
                ToGame::SendFailed {
                    cid: "cid-2".into(),
                    reason: SendFailure::Link,
                    rule: None,
                },
                ToGame::SendFailed {
                    cid: "cid-3".into(),
                    reason: SendFailure::DailyLimit,
                    rule: None,
                },
            ]
        );
    }

    #[test]
    fn cards_are_asked_only_for_recent_links_that_may_be_pressed() {
        let scene = Scene::from_view(&chat2_view(), &NotifyPrefs::default(), NOW);
        let mut m = message("i:11", "f-ayse", false, MessageState::Sent);
        m.text = "https://youtu.be/dQw4w9WgXcQ".into();
        assert!(wants_card(&m, &scene, NOW));
        let old = ChatMessageView {
            at: NOW - pl_social::CARD_WINDOW_MS - 1,
            ..m.clone()
        };
        assert!(!wants_card(&old, &scene, NOW));
        let refused = ChatMessageView {
            text: "https://bit.ly/x".into(),
            ..m.clone()
        };
        assert!(!wants_card(&refused, &scene, NOW));
        let quiet = Scene::from_view(&view(), &NotifyPrefs::default(), NOW);
        assert!(!wants_card(&m, &quiet, NOW), "links off");
        assert_eq!(
            Listener::from_hello("en_us", &["connect".into(), "chat2".into()], Lang::Tr),
            Listener {
                english: true,
                chat2: true
            }
        );
        assert_eq!(
            Listener::from_hello("tr_tr", &[], Lang::En),
            Listener::default()
        );
    }

    /// OVL-LANG: a mod whose words follow the launcher hears the overlay's language whatever its
    /// game speaks; an older one keeps the game's (the overlay's only when its hello names none).
    #[test]
    fn the_launchers_words_follow_the_overlays_language() {
        let follows: Vec<String> = vec!["chat2".into(), "lang".into()];
        assert!(Listener::from_hello("tr_tr", &follows, Lang::En).english);
        assert!(!Listener::from_hello("en_us", &follows, Lang::Tr).english);
        assert!(Listener::from_hello("", &[], Lang::En).english);
        assert!(Listener::from_hello("de_de", &[], Lang::Tr).english);
        // A sender nobody named, in the overlay's words.
        let scene = Scene::from_view(&view(), &NotifyPrefs::default(), NOW);
        assert_eq!(scene.sender("t:t1", "u-nobody", false), "Biri");
        assert_eq!(scene.sender("t:t1", "u-nobody", true), "Someone");
        // The greeting names the language only for a mod that follows it, and another language
        // greets again.
        let mut old = Mirror::for_mod(false, false);
        let greeting = old.greet(&scene, "2.0.0", false);
        assert!(matches!(greeting[0], ToGame::Welcome { lang: None, .. }));
        let mut mirror = Mirror::for_mod(false, false);
        mirror.set_lang(Some(Lang::Tr));
        let greeting = mirror.greet(&scene, "2.0.0", false);
        assert!(matches!(
            greeting[0],
            ToGame::Welcome {
                lang: Some(Lang::Tr),
                ..
            }
        ));
        assert!(mirror.update(&scene, "2.0.0", false).is_empty());
        mirror.set_lang(Some(Lang::En));
        let again = mirror.update(&scene, "2.0.0", false);
        assert!(matches!(
            again[0],
            ToGame::Welcome {
                lang: Some(Lang::En),
                ..
            }
        ));
        assert!(matches!(again.last(), Some(ToGame::Snapshot { .. })));
        assert!(mirror.update(&scene, "2.0.0", false).is_empty());
    }

    pub fn card(crew: &str, tag: &str, name: &str) -> Box<CrewBadgeView> {
        Box::new(CrewBadgeView {
            crew: crew.into(),
            tag: tag.into(),
            color: "navy".into(),
            frame: "shield".into(),
            emblem: "moon".into(),
            name: name.into(),
            motto: " Gece bizimdir ".into(),
            created_on: "2026-09-14".into(),
            members: 12,
            requests: true,
        })
    }

    /// The test view with crew tags on: Ayşe (a friend) wears Gece Kartalları's tag, Cem (a
    /// crewmate, not a friend) and Mert (a friend) the player's own crew's; two came into the crew
    /// meanwhile (one of them blocked) and the player too.
    pub fn team6_view() -> SocialView {
        let mut v = view();
        v.team_features.push("badges".into());
        v.friends[0].badge = Some(card("t9", "GK", "Gece Kartalları"));
        v.friends[3].badge = Some(card("t1", "GKŞ", "Gece Kuşları"));
        v.crews[0].members[1].badge = Some(card("t1", "GKŞ", "Gece Kuşları"));
        let join = |id: &str, name: &str, mine: bool, at: i64| pl_social::CrewJoinView {
            crew: "t1".into(),
            id: id.into(),
            name: name.into(),
            mine,
            at,
            greet_until: None,
        };
        v.crew_joins = vec![
            join("me", "Ece", true, NOW / 1000 - 600),
            join("x-bad", "Kötü", false, NOW / 1000 - 300),
            join("u-deniz", "Deniz", false, NOW / 1000 - 60),
        ];
        v
    }

    fn kinds(frames: &[ToGame]) -> Vec<String> {
        frames
            .iter()
            .map(|f| {
                serde_json::to_value(f).unwrap()["t"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn crew_tags_ride_on_friends_crew_messages_and_toasts() {
        let prefs = NotifyPrefs::default();
        let scene = Scene::from_view(&team6_view(), &prefs, NOW);
        assert_eq!(
            scene.features,
            [
                "friends", "chat", "calls", "tags", "people", "people2", "people3"
            ]
        );
        assert_eq!(
            scene.badges.keys().map(String::as_str).collect::<Vec<_>>(),
            ["t1", "t9"]
        );
        let theirs = &scene.badges["t9"];
        assert_eq!(
            (theirs.since.as_str(), theirs.motto.as_str(), theirs.mine),
            ("2026-09", "Gece bizimdir", false)
        );
        assert!(scene.badges["t1"].mine, "the player's own crew");
        assert_eq!(scene.friend("f-ayse").unwrap().badge.as_deref(), Some("t9"));
        assert_eq!(scene.friend("f-can").unwrap().badge, None);
        // Crew chat names its senders: a crewmate's tag rides on their message, never on the
        // player's own, and a direct conversation names nobody.
        let cem = msg_view(
            "c:t1",
            &message("i:1", "u-cem", false, MessageState::Sent),
            &scene,
            CHAT2,
        );
        assert_eq!(cem.badge.as_deref(), Some("t1"));
        let mine = msg_view(
            "c:t1",
            &message("i:2", "me", true, MessageState::Sent),
            &scene,
            CHAT2,
        );
        assert_eq!(mine.badge, None);
        let direct = msg_view(
            "d:f-ayse:me",
            &message("i:3", "f-ayse", false, MessageState::Sent),
            &scene,
            CHAT2,
        );
        assert_eq!(direct.badge, None);
        // The toast's title carries it: "[GK] Ayşe".
        let toast = toast_of(
            &SocialNotice::ChatMessage {
                conv: "d:f-ayse:me".into(),
                message: Box::new(message("i:4", "f-ayse", false, MessageState::Sent)),
                name: None,
                crew: None,
            },
            &scene,
            &prefs,
            NOW,
            CHAT2,
        )
        .unwrap();
        assert_eq!(toast.badge.as_deref(), Some("t9"));
        // Join lines: the player's own and Deniz's; nothing of someone the player blocked.
        let joins: Vec<(&str, bool)> = scene
            .joins
            .iter()
            .map(|j| (j.name.as_str(), j.mine))
            .collect();
        assert_eq!(joins, [("Ece", true), ("Deniz", false)]);
        assert_eq!(scene.joins[1].conv, "c:t1");
        assert_eq!(scene.joins[1].at, (NOW / 1000 - 60) * 1000);
    }

    #[test]
    fn a_mod_that_draws_tags_hears_the_cards_once_then_only_what_changed() {
        let prefs = NotifyPrefs::default();
        let scene = Scene::from_view(&team6_view(), &prefs, NOW);
        // An older mod: no cards and no lines (the friends' `badge` fields it skips).
        let old = Mirror::default().greet(&scene, "2.0.0", false);
        assert_eq!(kinds(&old), ["welcome", "snapshot"]);

        let mut mirror = Mirror::for_mod(true, false);
        let greeting = mirror.greet(&scene, "2.0.0", false);
        assert_eq!(
            kinds(&greeting),
            ["welcome", "badges", "snapshot", "note", "note"]
        );
        let ToGame::Badges { all, cards } = &greeting[1] else {
            panic!("no cards");
        };
        assert!(*all && cards.len() == 2 && cards.values().all(Option::is_some));
        assert!(mirror.update(&scene, "2.0.0", false).is_empty());

        // Its leader writes a new motto: only that card again.
        let mut v = team6_view();
        v.friends[0].badge.as_mut().unwrap().motto = "Şafakta buluşuruz".into();
        let frames = mirror.update(&Scene::from_view(&v, &prefs, NOW), "2.0.0", false);
        assert_eq!(kinds(&frames), ["badges"], "her row names the same crew");
        let ToGame::Badges { all, cards } = &frames[0] else {
            panic!()
        };
        assert!(!*all);
        assert_eq!(
            cards["t9"].as_ref().map(|card| card.motto.as_str()),
            Some("Şafakta buluşuruz")
        );
        assert!(!cards.contains_key("t1"));

        // Ayşe takes it off (or her leader hides the crew): the card goes, her row loses it.
        v.friends[0].badge = None;
        // Somebody new comes in meanwhile.
        v.crew_joins.push(pl_social::CrewJoinView {
            crew: "t1".into(),
            id: "u-elif".into(),
            name: "Elif".into(),
            mine: false,
            at: NOW / 1000,
            greet_until: Some(NOW / 1000 + 600),
        });
        let after = Scene::from_view(&v, &prefs, NOW);
        assert!(after.wears_differ(&scene));
        let frames = mirror.update(&after, "2.0.0", false);
        assert_eq!(kinds(&frames), ["badges", "friend", "note"]);
        let ToGame::Badges { cards, .. } = &frames[0] else {
            panic!()
        };
        assert_eq!(cards.get("t9"), Some(&None));
        assert!(matches!(&frames[1], ToGame::Friend(f) if f.id == "f-ayse" && f.badge.is_none()));
        assert!(matches!(&frames[2], ToGame::Note { name, .. } if name == "Elif"));
    }

    #[test]
    fn tags_switched_off_greet_again_without_any() {
        let prefs = NotifyPrefs::default();
        let mut mirror = Mirror::for_mod(true, false);
        mirror.greet(
            &Scene::from_view(&team6_view(), &prefs, NOW),
            "2.0.0",
            false,
        );
        // Studio's switch (or a service without them): the profiles carry no cards any more and
        // the service stops offering them.
        let mut off = team6_view();
        off.team_features.retain(|f| f != "badges");
        let scene = Scene::from_view(&off, &prefs, NOW);
        assert!(!scene.tags && scene.badges.is_empty());
        assert!(scene.friends.iter().all(|f| f.badge.is_none()));
        assert_eq!(
            msg_view(
                "c:t1",
                &message("i:1", "u-cem", false, MessageState::Sent),
                &scene,
                CHAT2
            )
            .badge,
            None
        );
        let frames = mirror.update(&scene, "2.0.0", false);
        assert_eq!(kinds(&frames)[..3], ["welcome", "badges", "snapshot"]);
        assert!(
            matches!(&frames[0], ToGame::Welcome { features, .. } if !features.iter().any(|f| f == "tags"))
        );
        assert!(matches!(&frames[1], ToGame::Badges { all: true, cards } if cards.is_empty()));
    }

    #[test]
    fn friends_carry_their_conversation_and_discord_and_invites_follow_the_game() {
        let prefs = NotifyPrefs::default();
        let mut v = view();
        v.friends[0].discord_url = "https://discord.com/users/f-ayse".into();
        // Not a profile the launcher would open.
        v.friends[3].discord_url = "discord://-/users/f-mert".into();
        let scene = Scene::from_view(&v, &prefs, NOW);
        let ayse = scene.friend("f-ayse").unwrap();
        assert_eq!(ayse.dm.as_deref(), Some("d:me:f-ayse"));
        assert!(ayse.discord);
        assert!(!scene.friend("f-mert").unwrap().discord);
        assert_eq!(
            scene.discord_url("f-ayse"),
            Some("https://discord.com/users/f-ayse")
        );
        assert_eq!(scene.discord_url("f-mert"), None);
        // In the launcher: "Birlikte oyna" cannot go.
        assert!(!scene.may_invite());
        // Chat off: no conversation to open.
        let mut quiet = v.clone();
        quiet.chat.as_mut().unwrap().on = false;
        let scene = Scene::from_view(&quiet, &prefs, NOW);
        assert!(scene.friends.iter().all(|f| f.dm.is_none()));

        // On Projects it can; on another server only while the player shows it.
        v.activity = pl_social::ActivityView::Playing {
            host: Some("play.projects.gg".into()),
        };
        let scene = Scene::from_view(&v, &prefs, NOW);
        assert!(scene.may_invite());
        assert_eq!(
            scene.features,
            [
                "friends", "chat", "calls", "people", "people2", "people3", "invite"
            ]
        );
        v.activity = pl_social::ActivityView::Playing {
            host: Some("mc.example.net".into()),
        };
        assert!(!Scene::from_view(&v, &prefs, NOW).may_invite());
        v.me.as_mut().unwrap().privacy.share_server = true;
        assert!(Scene::from_view(&v, &prefs, NOW).may_invite());
        v.activity = pl_social::ActivityView::Singleplayer;
        assert!(!Scene::from_view(&v, &prefs, NOW).may_invite());
    }

    #[test]
    fn the_players_own_tag_rides_on_the_welcome_with_its_card() {
        let prefs = NotifyPrefs::default();
        let mut v = team6_view();
        v.me.as_mut().unwrap().badge = Some(card("t1", "GKŞ", "Gece Kuşları"));
        let scene = Scene::from_view(&v, &prefs, NOW);
        assert_eq!(scene.me_badge.as_deref(), Some("t1"));
        assert_eq!(scene.badges["t1"].tag, "GKŞ");
        assert!(scene.badges["t1"].mine);
        let mut mirror = Mirror::for_mod(true, false);
        let frames = mirror.greet(&scene, "2.0.0", false);
        assert!(
            matches!(&frames[0], ToGame::Welcome { me, .. } if me.badge.as_deref() == Some("t1") && me.name == "Ece")
        );
        // Unchanged: nothing goes.
        assert!(mirror.update(&scene, "2.0.0", false).is_empty());
        // Taken off (or its crew hidden by the leader): greeted again without it.
        v.me.as_mut().unwrap().badge = None;
        let off = Scene::from_view(&v, &prefs, NOW);
        let again = mirror.update(&off, "2.0.0", false);
        assert!(matches!(&again[0], ToGame::Welcome { me, .. } if me.badge.is_none()));
        // Tags switched off: never one, even if the profile still had it.
        let mut no_tags = team6_view();
        no_tags.team_features.retain(|f| f != "badges");
        no_tags.me.as_mut().unwrap().badge = Some(card("t1", "GKŞ", "Gece Kuşları"));
        let scene = Scene::from_view(&no_tags, &prefs, NOW);
        assert!(scene.me_badge.is_none() && scene.badges.is_empty());
    }

    #[test]
    fn an_offline_friend_says_when_they_were_seen_as_roughly_as_the_launcher() {
        let prefs = NotifyPrefs::default();
        let mut v = view();
        // Can is offline; Mert is in the launcher (an online friend never says it).
        v.friends[1].last_seen = Some(pl_social::LastSeen::Yesterday);
        v.friends[3].last_seen = Some(pl_social::LastSeen::Today);
        let scene = Scene::from_view(&v, &prefs, NOW);
        assert_eq!(
            scene.friend("f-can").unwrap().seen,
            Some(pl_social::LastSeen::Yesterday)
        );
        assert_eq!(scene.friend("f-mert").unwrap().seen, None);
        let can = serde_json::to_value(scene.friend("f-can").unwrap()).unwrap();
        assert_eq!(can["seen"], "yesterday");
        // A friend who keeps it to themselves: the service gives nothing, nothing goes.
        v.friends[1].last_seen = None;
        let scene = Scene::from_view(&v, &prefs, NOW);
        let can = serde_json::to_value(scene.friend("f-can").unwrap()).unwrap();
        assert!(can.get("seen").is_none());
        // It changes on its own: the mirror tells the mod.
        let mut mirror = Mirror::for_mod(false, false);
        mirror.greet(&scene, "2.0.0", false);
        v.friends[1].last_seen = Some(pl_social::LastSeen::Week);
        let later = Scene::from_view(&v, &prefs, NOW);
        let frames = mirror.update(&later, "2.0.0", false);
        assert!(
            matches!(&frames[..], [ToGame::Friend(f)] if f.id == "f-can" && f.seen == Some(pl_social::LastSeen::Week))
        );
    }

    #[test]
    fn the_person_menus_lists_follow_the_players_rights_in_each_crew() {
        let prefs = NotifyPrefs::default();
        let mut v = view();
        // Members invite (the default): the player's crew is listed, with room.
        let scene = Scene::from_view(&v, &prefs, NOW);
        assert_eq!(
            scene.lists.crews,
            [CrewItem {
                id: "t1".into(),
                name: "Gece Kuşları".into(),
                full: false
            }]
        );
        assert!(scene.lists.groups.is_empty() && !scene.lists.crewmates);
        assert!(scene.is_crewmate("u-cem") && !scene.is_crewmate("me"));
        assert!(scene.features.iter().any(|f| f == FEATURE_PEOPLE2));
        // Only moderators invite, and the player is a plain member: no crew to invite to.
        v.crews[0].permissions.invite = PermissionLevel::Moderators;
        assert!(Scene::from_view(&v, &prefs, NOW).lists.crews.is_empty());
        v.crews[0].moderator = true;
        assert_eq!(Scene::from_view(&v, &prefs, NOW).lists.crews.len(), 1);
        // The leader always may; a full crew says so.
        v.crews[0].moderator = false;
        v.crews[0].permissions.invite = PermissionLevel::Leader;
        v.crews[0].role = TeamRole::Founder;
        v.crews[0].limit = 2;
        let scene = Scene::from_view(&v, &prefs, NOW);
        assert!(scene.lists.crews[0].full);
        // Crews off: none.
        v.teams = false;
        assert!(Scene::from_view(&v, &prefs, NOW).lists.crews.is_empty());
    }

    #[test]
    fn a_crewmate_who_is_not_a_friend_is_joined_and_written_to_as_the_launcher_offers() {
        let prefs = NotifyPrefs::default();
        let mut v = view();
        // Cem in the launcher, no Discord profile: nothing to offer.
        let scene = Scene::from_view(&v, &prefs, NOW);
        assert!(scene.lists.projects.is_empty() && scene.lists.discord.is_empty());
        // On Projects with a profile; a friend in the crew is never listed (their own frame
        // carries it), nor the player.
        v.crews[0].members[1].state = PresenceState::Playing;
        v.crews[0].members[1].game = projects();
        v.crews[0].members[1].discord_url = "https://discord.com/users/42".into();
        let mut friend_too = v.crews[0].members[1].clone();
        friend_too.id = v.friends[0].id.clone();
        v.crews[0].members.push(friend_too);
        v.crews[0].members[0].discord_url = "https://discord.com/users/me".into();
        let scene = Scene::from_view(&v, &prefs, NOW);
        assert_eq!(scene.lists.projects, ["u-cem"]);
        assert_eq!(scene.lists.discord, ["u-cem"]);
        assert_eq!(
            scene.discord_url("u-cem"),
            Some("https://discord.com/users/42")
        );
        // Blocked: no "Katıl" (the profile stays, as in the launcher's menu).
        v.crews[0].members[1].blocked = true;
        let blocked = Scene::from_view(&v, &prefs, NOW);
        assert!(blocked.lists.projects.is_empty());
        assert_eq!(blocked.lists.discord, ["u-cem"]);
        // Anywhere but Projects: never (another server is never named for a crewmate).
        v.crews[0].members[1].blocked = false;
        v.crews[0].members[1].game = Some(Game {
            location: GameLocation::Singleplayer,
            host: None,
        });
        assert!(Scene::from_view(&v, &prefs, NOW).lists.projects.is_empty());
        // A web profile that is not `https`: no row.
        v.crews[0].members[1].discord_url = "http://discord.com/users/42".into();
        assert!(Scene::from_view(&v, &prefs, NOW).lists.discord.is_empty());
        // The mod hears it when Cem comes to Projects.
        let mut mirror = Mirror::for_mod(false, true);
        mirror.greet(&Scene::from_view(&v, &prefs, NOW), "2.0.0", false);
        v.crews[0].members[1].game = projects();
        let frames = mirror.update(&Scene::from_view(&v, &prefs, NOW), "2.0.0", false);
        assert!(matches!(&frames[..], [ToGame::Lists { projects, .. }] if projects == &["u-cem"]));
    }

    #[test]
    fn a_mod_with_the_whole_person_menu_hears_the_lists_before_the_friends_and_their_changes() {
        let prefs = NotifyPrefs::default();
        let mut v = view();
        v.groups = Some(pl_social::FriendGroupsView {
            version: 1,
            groups: vec![pl_social::FriendGroup {
                id: "g1".into(),
                name: "Okul".into(),
            }],
            // A member of a category that is gone counts as "Arkadaşlar".
            members: vec![
                pl_social::GroupMember {
                    friend: "f-can".into(),
                    group: "g1".into(),
                },
                pl_social::GroupMember {
                    friend: "f-mert".into(),
                    group: "gone".into(),
                },
            ],
        });
        let scene = Scene::from_view(&v, &prefs, NOW);
        assert_eq!(scene.friend("f-can").unwrap().group.as_deref(), Some("g1"));
        assert_eq!(scene.friend("f-mert").unwrap().group, None);
        let mut mirror = Mirror::for_mod(false, true);
        let frames = mirror.greet(&scene, "2.0.0", false);
        assert!(matches!(
            &frames[..],
            [ToGame::Welcome { .. }, ToGame::Lists { groups, .. }, ToGame::Snapshot { .. }]
                if groups.len() == 1
        ));
        assert!(mirror.update(&scene, "2.0.0", false).is_empty());
        // A new category: the lists again, nothing else.
        v.groups
            .as_mut()
            .unwrap()
            .groups
            .push(pl_social::FriendGroup {
                id: "g2".into(),
                name: "PvP".into(),
            });
        let later = Scene::from_view(&v, &prefs, NOW);
        let frames = mirror.update(&later, "2.0.0", false);
        assert!(matches!(&frames[..], [ToGame::Lists { groups, .. }] if groups.len() == 2));
        // A mod without it never hears them.
        let frames = Mirror::for_mod(false, false).greet(&later, "2.0.0", false);
        assert!(!frames.iter().any(|f| matches!(f, ToGame::Lists { .. })));
    }
}
