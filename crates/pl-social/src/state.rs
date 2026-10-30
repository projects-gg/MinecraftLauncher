//! The launcher's picture of the friends feature: the profile, friends with their live presence,
//! requests, blocks and waiting invites. A pure reducer over presence events and API results,
//! so every rule is testable without a service.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::codes::crew_code;
use crate::model::{
    BlockedUser, CrewBadge, CrewEntry, CrewMute, DEFAULT_CREW_LIMIT, Friend, FriendGroups,
    FriendRequest, FriendsOverview, JOIN_REQUEST_LIFETIME_SECS, JoinRequest, Limits, Me, ModNotice,
    NoticeKind, ProfileMark, PublicProfile, RequestOutcome, SettingsMeta,
    TEAM_FEATURE_BADGE_REQUESTS, TEAM_FEATURE_BADGES, TEAM_FEATURE_MULTI, TEAM_FEATURE_MUTE,
    TEAM_FEATURE_PLAN_NOTES, Team, TeamAnswer, TeamExit, TeamInvite, TeamMember, TeamNotifications,
    TeamRequest, TeamRole, TeamsOverview, crews_of,
};
use crate::presence::{PresenceEvent, StopReason};
use crate::protocol::{
    CallAnswer, CallKind, CrewCall, CrewPlan, Game, GameLocation, Invite, LocalStatus,
    OnlineStatus, PlanAnswer, Presence, PresenceState, is_projects_host,
};
use crate::together::Together;
use crate::view::{
    CallAnswerView, CrewBadgeView, CrewCallView, CrewJoinView, FriendGroupsView, FriendView,
    GroupMember, InviteView, JoinRequestView, ModNoticeView, PlanAnswerView, PlanView, ProfileView,
    RequestView, SkinBlockView, SocialNotice, TagOffView, TeamInviteView, TeamMemberView,
    TeamRequestView, TeamView,
};

/// How long a received invite stays worth answering (the service delivers them for an hour).
pub const INVITE_LIFETIME_SECS: i64 = 3_600;
const MAX_INVITES: usize = 30;
/// Team invites shown at a time (the service lists at most this many too).
const MAX_TEAM_INVITES: usize = 20;
/// "Selam ver" stays this long after someone came in (TEAM6).
pub const GREET_WINDOW_SECS: i64 = 600;
/// "Ekibe katıldı" lines kept per crew (TEAM6): a crew has at most 30 members.
const MAX_CREW_JOINS: usize = 30;
/// A tag that came off live (its `badge` message) and its crew's message saying the crew was
/// hidden belong together within this long, in either order (TEAM6).
const TAG_DROP_SECS: i64 = 60;
/// A dropped connection that comes back within this long counts as "reconnecting", not as an
/// outage.
const RECONNECT_GRACE_SECS: u64 = 30;

/// The presence connection as the player should understand it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Connection {
    /// No connection (signed out, or the feature is off).
    #[default]
    Idle,
    Connecting,
    Online,
    /// Dropped a moment ago; coming back by itself.
    Reconnecting,
    /// The service cannot be reached or reached a daily limit; retried in the background.
    Unavailable,
    /// Another device of the account took over this connection.
    Replaced,
}

#[derive(Debug, Default, Clone)]
pub struct SocialState {
    me: Option<Me>,
    overview: Option<FriendsOverview>,
    /// Friends who are online right now (launcher or playing).
    online: HashMap<String, Presence>,
    invites: Vec<Invite>,
    /// Per friend, the time (Unix seconds) of the newest invite the player answered or closed.
    /// Every `hello` lists the invites of the last hour again; those stay closed.
    dismissed: HashMap<String, i64>,
    connection: Connection,
    /// Open crew calls the player sees (their own too), oldest first.
    calls: Vec<OpenCall>,
    /// Moderators' one-time notes still to be shown, and those shown in this run.
    notices: Vec<ModNotice>,
    seen_notices: HashSet<String>,
    /// The plans of the player's crews they see, as the service keeps them (TEAM5).
    plans: Vec<CrewPlan>,
    /// This computer's clock minus the service's (seconds), from the last `hello`: plans carry
    /// the service's times.
    plan_skew: i64,
    /// Who came into the player's crews while connected, in the order they came (TEAM6).
    joins: Vec<CrewJoin>,
    /// The crew whose tag the player wore when a `badge` message took it off, and when (TEAM6).
    tag_dropped: Option<(String, i64)>,
    /// The leader hid the crew whose tag the player wore: to be said once (TEAM6).
    tag_off: Option<TagOffView>,
}

/// Someone who came into one of the player's crews (TEAM6, "Hoş geldin").
#[derive(Debug, Clone)]
struct CrewJoin {
    crew: String,
    id: String,
    name: String,
    /// Unix seconds, this computer's clock.
    at: i64,
    /// "Selam ver" was pressed.
    greeted: bool,
}

/// A crew call with its times on this computer's clock.
#[derive(Debug, Clone)]
struct OpenCall {
    call: CrewCall,
    started: i64,
    ends: i64,
}

impl OpenCall {
    /// The service's clock may differ from this one: the call keeps the time it has left, not the
    /// time of day it ends at (`clock` is the service's time when the message left it).
    fn new(call: &CrewCall, clock: Option<i64>, now: i64) -> Self {
        let skew = clock.map_or(0, |clock| now - clock);
        Self {
            call: call.clone(),
            started: call.at + skew,
            ends: call.until + skew,
        }
    }
}

impl SocialState {
    pub fn me(&self) -> Option<&Me> {
        self.me.as_ref()
    }

    pub fn connection(&self) -> Connection {
        self.connection
    }

    pub fn set_connection(&mut self, connection: Connection) {
        self.connection = connection;
        if connection == Connection::Unavailable {
            // Nobody's presence is known during an outage: never show stale "in game", nor a
            // call that may have ended meanwhile (the next `hello` lists the open ones again).
            self.online.clear();
            self.calls.clear();
        }
    }

    /// The friend list and requests arrived.
    pub fn loaded(&self) -> bool {
        self.overview.is_some()
    }

    pub fn limits(&self) -> Option<Limits> {
        self.overview.as_ref().map(|o| o.limits)
    }

    pub fn set_me(&mut self, mut me: Me) {
        for notice in std::mem::take(&mut me.notices) {
            self.add_notice(notice);
        }
        // The player's own entries in the crews' member lists wear what `me` says (TEAM6): a tag
        // put on here shows on the crew pages at once; so do their mark (SOC-VERIFY) and a new
        // picture (SOC-LAG).
        let (id, badge, mark) = (
            me.profile.id.clone(),
            me.profile.badge.clone(),
            me.profile.mark,
        );
        let (avatar, hash) = (me.profile.avatar.clone(), me.profile.avatar_hash.clone());
        self.me = Some(me);
        self.set_badge_of(&id, badge.as_ref());
        self.set_mark_of(&id, mark);
        self.set_picture_of(&id, &avatar, hash.as_deref());
    }

    /// The friend list arrived. Its crews become one list in the player's order (`crews`), also
    /// from a service that knows one crew per player (`team`): nothing else reads `team`.
    pub fn set_overview(&mut self, mut overview: FriendsOverview) {
        let crews = crews_of(
            overview.crews.take(),
            overview.team.take(),
            std::mem::take(&mut overview.team_invited),
        );
        overview.crews = Some(crews);
        self.overview = Some(overview);
    }

    pub fn set_friend_code(&mut self, code: String) {
        if let Some(me) = &mut self.me {
            me.friend_code = code;
        }
    }

    pub fn set_settings_meta(&mut self, meta: Option<SettingsMeta>) {
        if let Some(me) = &mut self.me {
            me.settings = meta;
        }
    }

    /// Applies one presence event; returns the moments worth telling the player about.
    /// `now` is Unix seconds.
    pub fn apply(&mut self, event: &PresenceEvent, now: i64) -> Vec<SocialNotice> {
        let mut notices = Vec::new();
        match event {
            PresenceEvent::Connected {
                status,
                friends,
                invites,
                calls,
                clock,
                plans,
                ..
            } => {
                self.connection = Connection::Online;
                if let Some(clock) = clock {
                    self.plan_skew = now - clock;
                }
                // Every plan the player sees: whatever is not listed is gone.
                if let Some(plans) = plans {
                    self.plans = plans.clone();
                    self.prune_plans(now);
                }
                if let Some(me) = &mut self.me {
                    me.status = *status;
                }
                for presence in friends {
                    self.absorb_picture(presence);
                }
                // The list is complete: whoever is not in it is offline.
                self.online = friends
                    .iter()
                    .filter(|p| p.state != PresenceState::Offline)
                    .map(|p| (p.id.clone(), p.clone()))
                    .collect();
                for invite in invites {
                    self.add_invite(invite, true, now);
                }
                // Every open call, quietly: whatever is not listed has ended.
                self.calls = calls
                    .iter()
                    .map(|call| OpenCall::new(call, *clock, now))
                    .filter(|call| call.ends > now)
                    .collect();
                self.calls.sort_by_key(|call| call.started);
            }
            PresenceEvent::Presence(presence) => {
                self.absorb_picture(presence);
                if presence.state == PresenceState::Offline {
                    self.online.remove(&presence.id);
                    // Their bucket while they share it, nothing while they hide it (SOC-LAG: the
                    // friend list's stayed until the next list); a presence without it keeps it.
                    if let Some(seen) = presence.last_seen
                        && let Some(friend) = self.friend_mut(&presence.id)
                    {
                        friend.last_seen = seen;
                    }
                } else {
                    let before = self.online.get(&presence.id);
                    let was_on_projects = before
                        .and_then(|p| p.game.as_ref())
                        .is_some_and(Game::joinable);
                    let on_projects = presence.game.as_ref().is_some_and(Game::joinable);
                    // Coming online is news about friends; teammates show it in the team only.
                    let friend = self.is_listed_friend(&presence.id);
                    if friend && before.is_none() {
                        notices.push(SocialNotice::FriendOnline {
                            id: presence.id.clone(),
                        });
                    }
                    if friend && !was_on_projects && on_projects {
                        notices.push(SocialNotice::FriendOnProjects {
                            id: presence.id.clone(),
                        });
                    }
                    // Crewmates (friends or not) joining Projects: the launcher announces it only
                    // when the player asked for it ("Ekip arkadaşların Projects'e girince") and
                    // has not muted every crew they share ("Sessize al").
                    if !was_on_projects && on_projects && self.crewmate_heard(&presence.id, now) {
                        notices.push(SocialNotice::CrewmateOnProjects {
                            id: presence.id.clone(),
                        });
                    }
                    self.online.insert(presence.id.clone(), presence.clone());
                }
            }
            PresenceEvent::TeamChanged {
                team,
                crew,
                reason,
                by,
                joined,
            } => {
                // TEAM6: whether this hides a crew whose tag the player wore (its tag may have
                // come off just before, by a `badge` message), and which crew is gone.
                let hides_my_tag = team.as_ref().is_some_and(|team| {
                    team.hidden
                        && self.crew(&team.id).is_some_and(|entry| !entry.team.hidden)
                        && (self.my_badge_crew() == Some(team.id.as_str())
                            || self.tag_dropped.as_ref().is_some_and(|(dropped, at)| {
                                *dropped == team.id && now - at <= TAG_DROP_SECS
                            }))
                });
                let gone = match (team, crew) {
                    (Some(_), _) => None,
                    (None, Some(id)) => Some(id.clone()),
                    (None, None) => self.crews().first().map(|entry| entry.team.id.clone()),
                };
                if let Some(notice) =
                    self.crew_changed(team.as_ref(), crew.as_deref(), *reason, by.as_deref())
                {
                    notices.push(notice);
                }
                if let Some(team) = team {
                    self.badges_from(team);
                    self.marks_from(team);
                    self.crew_joined(team, joined, now);
                    let mine = self.me.as_ref().map(|me| me.profile.id.as_str());
                    if hides_my_tag && by.as_deref() != mine {
                        self.tag_off = Some(TagOffView {
                            crew: team.id.clone(),
                            name: team.name.clone(),
                        });
                        self.tag_dropped = None;
                    }
                }
                if let Some(gone) = gone {
                    self.crew_badges_gone(&gone, *reason == Some(TeamExit::Disbanded));
                }
                self.prune_crew_calls();
                self.prune_crew_plans();
                self.prune_crew_joins();
            }
            PresenceEvent::BadgeChanged { id, badge } => {
                // The player's own tag went off live: its crew's "hidden" may come next.
                let mine = self.me.as_ref().is_some_and(|me| me.profile.id == *id);
                if mine {
                    let before = self.my_badge_crew().map(str::to_owned);
                    let after = badge.as_ref().map(|badge| badge.crew.as_str());
                    self.tag_dropped = before
                        .filter(|crew| after != Some(crew.as_str()))
                        .map(|crew| (crew, now));
                }
                self.set_badge_of(id, badge.as_ref());
            }
            PresenceEvent::MarkChanged { id, mark } => self.set_mark_of(id, *mark),
            PresenceEvent::TeamRequests {
                crew,
                requests,
                added,
            } => {
                let heard = self.crew_heard(crew, now);
                let badges = self.offers(TEAM_FEATURE_BADGES);
                if let Some(entry) = self
                    .crews_mut()
                    .and_then(|crews| crews.iter_mut().find(|entry| &entry.team.id == crew))
                {
                    entry.requests = requests.clone();
                    let fresh = added
                        .as_deref()
                        .and_then(|id| requests.iter().find(|r| r.from.id == id))
                        .filter(|r| now - r.sent_at < JOIN_REQUEST_LIFETIME_SECS);
                    if let Some(request) = fresh.filter(|_| heard) {
                        notices.push(SocialNotice::JoinRequest {
                            crew: crew.clone(),
                            request: join_request_view(request, &entry.team.members, badges),
                        });
                    }
                }
            }
            PresenceEvent::Plan { plan, quiet } => {
                let known = self.plans.iter().any(|p| p.id == plan.id);
                let me = self.me.as_ref().map(|me| me.profile.id.as_str());
                let upcoming = plan.at + self.plan_skew > now;
                // Said once: a plan new to this launcher, live, someone else's, in a crew that
                // is not muted (the service quiets it too; a mute set a moment ago counts here).
                let announce = !known
                    && !*quiet
                    && upcoming
                    && me != Some(plan.by.as_str())
                    && self.crew_heard(&plan.crew, now);
                self.upsert_plan(plan.clone());
                if announce && let Some(view) = self.plan_view(plan) {
                    notices.push(SocialNotice::CrewPlan {
                        plan: Box::new(view),
                    });
                }
            }
            PresenceEvent::Plans { crew, plans } => {
                self.plans.retain(|plan| &plan.crew != crew);
                self.plans.extend(plans.iter().cloned());
                self.prune_plans(now);
            }
            PresenceEvent::PlanRemoved { plan, .. } => self.plan_gone(plan),
            PresenceEvent::CrewMuted {
                crew,
                notifications,
            } => self.set_crew_mute(crew, *notifications),
            PresenceEvent::CrewOrder(order) => self.set_crew_order(order),
            PresenceEvent::Call { call, quiet, clock } => {
                self.prune_calls(now);
                let next = OpenCall::new(call, *clock, now);
                let at = self.calls.iter().position(|open| open.call.id == call.id);
                if next.ends <= now {
                    if let Some(at) = at {
                        self.calls.remove(at);
                    }
                } else {
                    let me = self.me.as_ref().map(|me| me.profile.id.as_str());
                    // The service quiets calls for a player who muted the crews or switched calls
                    // off; a setting that changed a moment ago here counts too.
                    let announce =
                        !*quiet && me != Some(call.from.as_str()) && self.crew_calls_on(now, call);
                    match at {
                        None if announce => notices.push(SocialNotice::CrewCall {
                            call: Box::new(self.call_view(&next)),
                        }),
                        Some(at) if announce => {
                            let before = &self.calls[at];
                            let answer = me.and_then(|me| {
                                next.call
                                    .answers
                                    .get(me)
                                    .or_else(|| before.call.answers.get(me))
                            });
                            let coming = matches!(answer, Some(CallAnswer::Yes | CallAnswer::Soon));
                            if coming && moved(&before.call, &next.call) {
                                notices.push(SocialNotice::CrewCallJoin {
                                    call: Box::new(self.call_view(&next)),
                                });
                            }
                        }
                        _ => {}
                    }
                    match at {
                        Some(at) => self.calls[at] = next,
                        None => {
                            self.calls.push(next);
                            self.calls.sort_by_key(|call| call.started);
                        }
                    }
                }
            }
            PresenceEvent::CallAnswered { call, by, answer } => {
                if let Some(open) = self.calls.iter_mut().find(|open| &open.call.id == call) {
                    open.call.answers.insert(by.clone(), *answer);
                }
            }
            PresenceEvent::CallEnded { call, .. } => self.call_gone(call),
            PresenceEvent::TeamInvited(invite) => {
                if let Some(overview) = &mut self.overview {
                    overview
                        .team_invites
                        .retain(|i| i.team.id != invite.team.id);
                    overview.team_invites.insert(0, invite.clone());
                    overview.team_invites.truncate(MAX_TEAM_INVITES);
                    notices.push(SocialNotice::TeamInvite {
                        invite: Box::new(team_invite_view(invite)),
                    });
                }
            }
            PresenceEvent::CrewNotifications(settings) => self.set_team_notifications(*settings),
            PresenceEvent::GroupsChanged(groups) => {
                if let Some(overview) = &mut self.overview
                    && overview
                        .groups
                        .as_ref()
                        .is_none_or(|current| groups.version >= current.version)
                {
                    overview.groups = Some(groups.clone());
                }
            }
            PresenceEvent::FriendAdded { friend, presence } => {
                let id = friend.id.clone();
                self.add_friend(Friend {
                    profile: friend.clone(),
                    since: String::new(),
                    last_seen: None,
                });
                if presence.state == PresenceState::Offline {
                    self.online.remove(&id);
                } else {
                    self.online.insert(id.clone(), presence.clone());
                }
                if let Some(view) = self.friend_view(&id) {
                    notices.push(SocialNotice::FriendAdded { friend: view });
                }
            }
            PresenceEvent::FriendRemoved { id } => self.forget(id),
            PresenceEvent::RequestReceived { from } => {
                if let Some(overview) = &mut self.overview {
                    let known = overview.incoming.iter().any(|r| r.profile.id == from.id)
                        || overview.friends.iter().any(|f| f.profile.id == from.id);
                    if !known {
                        overview.incoming.insert(
                            0,
                            FriendRequest {
                                profile: from.clone(),
                                sent_at: now,
                            },
                        );
                    }
                }
                notices.push(SocialNotice::Request {
                    request: request_view(&FriendRequest {
                        profile: from.clone(),
                        sent_at: now,
                    }),
                });
            }
            PresenceEvent::RequestCancelled { id } => {
                if let Some(overview) = &mut self.overview {
                    overview.incoming.retain(|r| &r.profile.id != id);
                }
            }
            PresenceEvent::Invite(invite) => {
                if self.add_invite(invite, invite.quiet, now)
                    && !invite.quiet
                    && self.is_friend(&invite.from)
                {
                    notices.push(SocialNotice::Invite {
                        invite: self.invite_view(invite),
                    });
                }
            }
            PresenceEvent::QueuedInvites(invites) => {
                for invite in invites {
                    self.add_invite(invite, true, now);
                }
            }
            PresenceEvent::StatusChanged(status) => {
                if let Some(me) = &mut self.me {
                    me.status = *status;
                }
            }
            PresenceEvent::ProfileChanged {
                privacy,
                avatar,
                avatar_hash,
            } => {
                if let Some(me) = &mut self.me {
                    me.privacy = *privacy;
                    let id = me.profile.id.clone();
                    if let Some(avatar) = avatar {
                        self.set_picture_of(&id, avatar, avatar_hash.as_deref());
                    }
                }
            }
            PresenceEvent::Disconnected { retry_in } => {
                let brief = retry_in.as_secs() <= RECONNECT_GRACE_SECS;
                let next = if self.connection == Connection::Online && brief {
                    Connection::Reconnecting
                } else {
                    Connection::Unavailable
                };
                self.set_connection(next);
            }
            PresenceEvent::Stopped(reason) => {
                self.online.clear();
                self.calls.clear();
                self.connection = match reason {
                    StopReason::Replaced => Connection::Replaced,
                    StopReason::SignedOut
                    | StopReason::AccountDeleted
                    | StopReason::Shutdown
                    | StopReason::Restricted { .. } => Connection::Idle,
                };
            }
            PresenceEvent::Notice(notice) => self.add_notice(notice.clone()),
            // Chat lives in the service's history (history.rs), not here.
            // SKIN1's news go to `crate::minecraft`; Cloud AFK's notices straight to the launcher.
            PresenceEvent::Mc(_) | PresenceEvent::AfkNotice(_) => {}
            PresenceEvent::ChatState(_)
            | PresenceEvent::ChatMessage(_)
            | PresenceEvent::ChatEdited(_)
            | PresenceEvent::ChatDeleted { .. }
            | PresenceEvent::ChatSync { .. }
            | PresenceEvent::ChatSynced { .. }
            | PresenceEvent::ChatReacted { .. }
            | PresenceEvent::ChatTyping { .. }
            | PresenceEvent::ChatRead { .. } => {}
            // World sessions belong to the launcher's host (HOST1, `crate::world`).
            PresenceEvent::World(_) => {}
        }
        notices
    }

    /// A friend or a teammate is online now (in the launcher or playing).
    pub fn is_online(&self, id: &str) -> bool {
        self.online.contains_key(id)
    }

    // --- moderators' notes (MOD1) -------------------------------------------------------------

    /// A note from moderators, once (the same id again changes nothing).
    fn add_notice(&mut self, notice: ModNotice) {
        if notice.kind == NoticeKind::Other
            || self.seen_notices.contains(&notice.id)
            || self.notices.iter().any(|n| n.id == notice.id)
        {
            return;
        }
        self.notices.push(notice);
    }

    /// The notes still to be shown, oldest first.
    pub fn notices(&self) -> Vec<ModNoticeView> {
        self.notices
            .iter()
            .map(|notice| ModNoticeView {
                id: notice.id.clone(),
                kind: notice.kind,
                at: notice.at,
                reason: notice.reason,
                // Only a skin reset blocks anything; a time of 0 or less is no block.
                blocked: match (notice.kind, notice.until) {
                    (NoticeKind::SkinReset, Some(None)) => Some(SkinBlockView { until: None }),
                    (NoticeKind::SkinReset, Some(Some(until))) if until > 0 => {
                        Some(SkinBlockView { until: Some(until) })
                    }
                    _ => None,
                },
            })
            .collect()
    }

    /// The launcher showed these notes: they never come back in this run.
    pub fn notices_seen(&mut self, ids: &[String]) {
        self.seen_notices.extend(ids.iter().cloned());
        self.notices.retain(|notice| !ids.contains(&notice.id));
    }

    /// A request was sent by code: pending, or friends right away when they had asked first.
    pub fn request_sent(&mut self, outcome: RequestOutcome) {
        match outcome {
            RequestOutcome::Requested { user, sent_at } => {
                if let Some(overview) = &mut self.overview
                    && !overview.outgoing.iter().any(|r| r.profile.id == user.id)
                {
                    overview.outgoing.insert(
                        0,
                        FriendRequest {
                            profile: user,
                            sent_at,
                        },
                    );
                }
            }
            RequestOutcome::Friends { friend } => self.add_friend(friend),
        }
    }

    pub fn request_accepted(&mut self, friend: Friend) {
        self.add_friend(friend);
    }

    pub fn request_declined(&mut self, from: &str) {
        if let Some(overview) = &mut self.overview {
            overview.incoming.retain(|r| r.profile.id != from);
        }
    }

    pub fn request_cancelled(&mut self, to: &str) {
        if let Some(overview) = &mut self.overview {
            overview.outgoing.retain(|r| r.profile.id != to);
        }
    }

    pub fn friend_removed(&mut self, id: &str) {
        self.forget(id);
    }

    /// The profile of someone the player can see (friend, request, blocked), for blocking.
    pub fn profile(&self, id: &str) -> Option<PublicProfile> {
        let overview = self.overview.as_ref()?;
        overview
            .friends
            .iter()
            .map(|f| &f.profile)
            .chain(overview.incoming.iter().map(|r| &r.profile))
            .chain(overview.outgoing.iter().map(|r| &r.profile))
            .chain(overview.blocked.iter().map(|b| &b.profile))
            .find(|p| p.id == id)
            .cloned()
    }

    /// Blocking removes friendship and requests both ways and hides presence both ways (and the
    /// calls of the one blocked, and their requests to join the player's crews).
    pub fn blocked(&mut self, profile: PublicProfile) {
        self.forget(&profile.id);
        self.calls.retain(|open| open.call.from != profile.id);
        if let Some(crews) = self.crews_mut() {
            for entry in crews {
                entry.requests.retain(|r| r.from.id != profile.id);
            }
        }
        if let Some(overview) = &mut self.overview
            && !overview.blocked.iter().any(|b| b.profile.id == profile.id)
        {
            overview.blocked.insert(
                0,
                BlockedUser {
                    profile,
                    since: String::new(),
                },
            );
        }
    }

    pub fn unblocked(&mut self, id: &str) {
        if let Some(overview) = &mut self.overview {
            overview.blocked.retain(|b| b.profile.id != id);
        }
    }

    // --- friend categories --------------------------------------------------------------------

    /// The player's categories as last saved or received (None: the service has none to offer).
    pub fn groups(&self) -> Option<&FriendGroups> {
        self.overview.as_ref()?.groups.as_ref()
    }

    pub fn set_groups(&mut self, groups: FriendGroups) {
        if let Some(overview) = &mut self.overview {
            overview.groups = Some(groups);
        }
    }

    /// The categories for the UI: entries of people who are no longer friends are left out (the
    /// next save drops them).
    pub fn groups_view(&self) -> Option<FriendGroupsView> {
        let overview = self.overview.as_ref()?;
        let groups = overview.groups.as_ref()?;
        Some(FriendGroupsView {
            version: groups.version,
            groups: groups.groups.clone(),
            members: groups
                .members
                .iter()
                .filter(|(friend, group)| {
                    overview.friends.iter().any(|f| &f.profile.id == *friend)
                        && groups.groups.iter().any(|g| &g.id == *group)
                })
                .map(|(friend, group)| GroupMember {
                    friend: friend.clone(),
                    group: group.clone(),
                })
                .collect(),
        })
    }

    // --- teams --------------------------------------------------------------------------------

    /// The service offers teams (an older one sends no team limit).
    pub fn teams_supported(&self) -> bool {
        self.overview
            .as_ref()
            .is_some_and(|o| o.limits.team_members.is_some())
    }

    /// What the service offers of crews beyond the first version (`teamFeatures`).
    pub fn team_features(&self) -> &[String] {
        self.overview
            .as_ref()
            .map_or(&[], |o| o.team_features.as_slice())
    }

    /// The service offers this part of crews (`look`, `calls`, …).
    /// The features the service named with the friend list (`GET /v1/friends`): `chat`, SKIN1's
    /// `mcLink`, `skins` and `mcCodeless`. None before the list came.
    pub fn features(&self) -> Option<&[String]> {
        self.overview.as_ref().map(|o| o.features.as_slice())
    }

    pub fn offers(&self, feature: &str) -> bool {
        self.team_features().iter().any(|f| f == feature)
    }

    /// The player's crews in their order; the first is the primary crew.
    pub fn crews(&self) -> &[CrewEntry] {
        self.overview
            .as_ref()
            .and_then(|o| o.crews.as_deref())
            .unwrap_or(&[])
    }

    /// One of the player's crews.
    pub fn crew(&self, id: &str) -> Option<&CrewEntry> {
        self.crews().iter().find(|entry| entry.team.id == id)
    }

    fn crews_mut(&mut self) -> Option<&mut Vec<CrewEntry>> {
        Some(self.overview.as_mut()?.crews.get_or_insert_with(Vec::new))
    }

    /// How many crews the player may be in: the service's limit where it has several crews,
    /// else one.
    pub fn crew_limit(&self) -> u32 {
        if !self.offers(TEAM_FEATURE_MULTI) {
            return 1;
        }
        self.overview
            .as_ref()
            .and_then(|o| o.crew_limit)
            .unwrap_or(DEFAULT_CREW_LIMIT)
            .max(1)
    }

    /// The crews' ids in the player's order.
    pub fn crew_order(&self) -> Vec<String> {
        self.crews()
            .iter()
            .map(|entry| entry.team.id.clone())
            .collect()
    }

    /// The crews in a new order (this device's, or another's): the ones named go first in that
    /// order, any others keep theirs after them.
    pub fn set_crew_order(&mut self, order: &[String]) {
        if let Some(crews) = self.crews_mut() {
            crews.sort_by_key(|entry| {
                order
                    .iter()
                    .position(|id| *id == entry.team.id)
                    .unwrap_or(usize::MAX)
            });
        }
    }

    /// The player's mute of one crew (the service answered, or another device changed it).
    pub fn set_crew_mute(&mut self, team_id: &str, mute: CrewMute) {
        if let Some(entry) = self
            .crews_mut()
            .and_then(|crews| crews.iter_mut().find(|entry| entry.team.id == team_id))
        {
            entry.notifications = mute;
        }
    }

    /// The crew part of the friend list read again (`GET /v1/teams`).
    pub fn set_teams(&mut self, teams: TeamsOverview) {
        if let Some(overview) = &mut self.overview {
            overview.crews = Some(crews_of(teams.crews, teams.team, teams.invited));
            overview.crew_limit = teams.crew_limit.or(overview.crew_limit);
            overview.team_requests = teams.team_requests;
            if let Some(settings) = teams.team_notifications {
                overview.team_notifications = Some(settings);
            }
        }
        self.prune_crew_plans();
    }

    /// The player's crew notification settings, from a service that keeps them (`mute`).
    pub fn team_notifications(&self) -> Option<TeamNotifications> {
        if !self.offers(TEAM_FEATURE_MUTE) {
            return None;
        }
        Some(
            self.overview
                .as_ref()?
                .team_notifications
                .unwrap_or_default(),
        )
    }

    /// The service answered, or another device of the player's changed them.
    pub fn set_team_notifications(&mut self, settings: TeamNotifications) {
        if let Some(overview) = &mut self.overview {
            overview.team_notifications = Some(settings);
        }
    }

    /// Every crew notification is silent at `now` ("Sessize al" for all crews; a timed mute ends
    /// by itself).
    fn crew_muted(&self, now: i64) -> bool {
        self.team_notifications().is_some_and(|n| n.muted_at(now))
    }

    /// A call may be announced at `now`: crew calls switched on, not muted for all crews, and at
    /// least one crew of the player's it names not muted on its own.
    fn crew_calls_on(&self, now: i64, call: &CrewCall) -> bool {
        if !self.team_notifications().is_none_or(|n| n.calls_at(now)) {
            return false;
        }
        let crews = self.call_crews(call);
        crews.is_empty() || crews.iter().any(|entry| !entry.notifications.muted_at(now))
    }

    /// A crewmate's news (joining Projects) may be said at `now`: not muted for all crews, and
    /// they share a crew with the player that is not muted on its own.
    fn crewmate_heard(&self, id: &str, now: i64) -> bool {
        self.is_teammate(id)
            && !self.crew_muted(now)
            && self.crews().iter().any(|entry| {
                !entry.notifications.muted_at(now)
                    && entry.team.members.iter().any(|m| m.profile.id == id)
            })
    }

    /// The player's crews a call names, in the player's order. A call from a service with one
    /// crew per player names none: it is that crew's.
    fn call_crews(&self, call: &CrewCall) -> Vec<&CrewEntry> {
        let crews = self.crews();
        if call.crews.is_empty() {
            crews.first().into_iter().collect()
        } else {
            crews
                .iter()
                .filter(|entry| call.crews.contains(&entry.team.id))
                .collect()
        }
    }

    /// Calls live in crews: one that names none of the player's crews any more goes, and so does
    /// one whose caller is in none of them (they left or were removed). The service says so too.
    fn prune_crew_calls(&mut self) {
        if self.overview.is_none() {
            return;
        }
        let me = self.me.as_ref().map(|me| me.profile.id.clone());
        let keep: Vec<bool> = self
            .calls
            .iter()
            .map(|open| {
                let crews = self.call_crews(&open.call);
                !crews.is_empty()
                    && (me.as_deref() == Some(open.call.from.as_str())
                        || crews.iter().any(|entry| {
                            entry
                                .team
                                .members
                                .iter()
                                .any(|m| m.profile.id == open.call.from)
                        }))
            })
            .collect();
        let mut keep = keep.into_iter();
        self.calls.retain(|_| keep.next().unwrap_or(true));
    }

    /// One of the player's crews changed (`team`), or they are no longer in it (`team` None, the
    /// crew named by `crew`, else the only one). A crew the player is new in goes last; with one
    /// crew per player it replaces the one there was. Being removed or seeing the crew disbanded
    /// by someone else is worth telling.
    fn crew_changed(
        &mut self,
        team: Option<&Team>,
        crew: Option<&str>,
        reason: Option<TeamExit>,
        by: Option<&str>,
    ) -> Option<SocialNotice> {
        let me = self.me.as_ref().map(|me| me.profile.id.clone());
        let multi = self.offers(TEAM_FEATURE_MULTI);
        let overview = self.overview.as_mut()?;
        let crews = overview.crews.get_or_insert_with(Vec::new);
        match team {
            Some(team) => {
                overview.team_invites.retain(|i| i.team.id != team.id);
                // A join request of the player's that a leader took: the crew comes with it.
                let asked = overview.team_requests.iter().any(|r| r.team.id == team.id);
                overview.team_requests.retain(|r| r.team.id != team.id);
                let mine = team
                    .members
                    .iter()
                    .find(|m| Some(&m.profile.id) == me.as_ref());
                // Invites go with the right to send them (the service withdraws them), and the
                // crew's join requests with the right to answer them.
                let may_invite =
                    mine.is_some_and(|m| team.permissions.invite.allows(m.role, m.moderator));
                let may_admit =
                    mine.is_some_and(|m| team.permissions.admit.allows(m.role, m.moderator));
                let new = !crews.iter().any(|entry| entry.team.id == team.id);
                let at = match crews.iter().position(|entry| entry.team.id == team.id) {
                    Some(at) => at,
                    None => {
                        if !multi {
                            crews.clear();
                        }
                        crews.push(CrewEntry {
                            team: team.clone(),
                            invited: Vec::new(),
                            notifications: CrewMute::default(),
                            requests: Vec::new(),
                        });
                        crews.len() - 1
                    }
                };
                let entry = &mut crews[at];
                entry.team = team.clone();
                entry
                    .invited
                    .retain(|id| !team.members.iter().any(|m| &m.profile.id == id));
                if !may_invite {
                    entry.invited.clear();
                }
                if !may_admit {
                    entry.requests.clear();
                }
                // Someone who joined meanwhile is no longer asking.
                entry
                    .requests
                    .retain(|r| !team.members.iter().any(|m| m.profile.id == r.from.id));
                (new && asked && by.is_some() && by.map(str::to_owned) != me).then(|| {
                    SocialNotice::RequestAccepted {
                        crew: team.id.clone(),
                        name: team.name.clone(),
                    }
                })
            }
            None => {
                let at = match crew {
                    Some(id) => crews.iter().position(|entry| entry.team.id == id),
                    None => (!crews.is_empty()).then_some(0),
                };
                let gone = at.map(|at| crews.remove(at));
                let told = matches!(reason, Some(TeamExit::Kicked | TeamExit::Disbanded))
                    && by.is_some()
                    && by.map(str::to_owned) != me
                    && (gone.is_some() || crew.is_none());
                told.then(|| SocialNotice::TeamGone {
                    reason: reason.unwrap_or(TeamExit::Other),
                    name: gone.map(|entry| entry.team.name),
                    staff: by == Some("staff"),
                })
            }
        }
    }

    /// The player's own open call, if any.
    pub fn my_call(&self) -> Option<&CrewCall> {
        let me = self.me.as_ref()?.profile.id.as_str();
        self.calls
            .iter()
            .map(|open| &open.call)
            .find(|call| call.from == me)
    }

    /// Answered here (the service also echoes it to every device of the player).
    pub fn call_answered(&mut self, call: &str, answer: CallAnswer) {
        let Some(me) = self.me.as_ref().map(|me| me.profile.id.clone()) else {
            return;
        };
        if let Some(open) = self.calls.iter_mut().find(|open| open.call.id == call) {
            open.call.answers.insert(me, answer);
        }
    }

    /// A call ended, or turned out to be gone.
    pub fn call_gone(&mut self, call: &str) {
        self.calls.retain(|open| open.call.id != call);
    }

    /// Open calls at `now` (local Unix seconds), oldest first. Calls that ran out are left out:
    /// no message says they ended.
    pub fn calls(&self, now: i64) -> Vec<CrewCallView> {
        self.calls
            .iter()
            .filter(|open| open.ends > now)
            .map(|open| self.call_view(open))
            .collect()
    }

    fn prune_calls(&mut self, now: i64) {
        self.calls.retain(|open| open.ends > now);
    }

    fn call_view(&self, open: &OpenCall) -> CrewCallView {
        let me = self.me.as_ref().map(|me| me.profile.id.as_str());
        let crews = self.call_crews(&open.call);
        // Everyone the call reaches, crew by crew in the player's order, each once.
        let mut members: Vec<&TeamMember> = Vec::new();
        for entry in &crews {
            for member in &entry.team.members {
                if !members.iter().any(|m| m.profile.id == member.profile.id) {
                    members.push(member);
                }
            }
        }
        let caller = members
            .iter()
            .copied()
            .find(|m| m.profile.id == open.call.from)
            .or_else(|| {
                self.crews()
                    .iter()
                    .flat_map(|entry| entry.team.members.iter())
                    .find(|m| m.profile.id == open.call.from)
            });
        // A call with its server is always "join" (CALL1); one from before it is on Projects when
        // it is "join".
        let server = open.call.server.clone().or_else(|| {
            (open.call.kind == CallKind::Join).then_some(Game {
                location: GameLocation::Projects,
                host: None,
                ver: None,
            })
        });
        CrewCallView {
            id: open.call.id.clone(),
            from: open.call.from.clone(),
            mine: me == Some(open.call.from.as_str()),
            name: caller.map(|m| m.profile.name().to_owned()),
            avatar: caller.map(|m| m.profile.avatar.clone()),
            avatar_url: caller.and_then(|m| m.profile.discord_avatar_url(128)),
            mark: caller.and_then(|m| m.profile.mark),
            kind: if open.call.server.is_some() {
                CallKind::Join
            } else {
                open.call.kind
            },
            server,
            crews: crews.iter().map(|entry| entry.team.id.clone()).collect(),
            started_at: open.started,
            ends_at: open.ends,
            // In the crews' order; answers of people no longer in them are not shown.
            answers: members
                .iter()
                .filter_map(|m| {
                    open.call
                        .answers
                        .get(&m.profile.id)
                        .map(|answer| CallAnswerView {
                            id: m.profile.id.clone(),
                            answer: *answer,
                        })
                })
                .collect(),
            my_answer: me.and_then(|me| open.call.answers.get(me).copied()),
        }
    }

    /// Who is on Projects together with the player now ("Birlikte oynanan süre"): nobody unless
    /// the player plays on projects.gg; then the crewmates there too (not those blocked) and the
    /// crews they are in.
    pub fn together_now(&self, activity: &LocalStatus) -> Together {
        let mut together = Together::default();
        let on_projects = matches!(
            activity,
            LocalStatus::Playing { host: Some(host) } if is_projects_host(host)
        );
        let Some(overview) = self.overview.as_ref().filter(|_| on_projects) else {
            return together;
        };
        let me = self.me.as_ref().map(|me| me.profile.id.as_str());
        for entry in self.crews() {
            for member in &entry.team.members {
                let id = member.profile.id.as_str();
                let there = Some(id) != me
                    && !overview.blocked.iter().any(|b| b.profile.id == id)
                    && self.online.get(id).is_some_and(|p| {
                        p.state == PresenceState::Playing
                            && p.game.as_ref().is_some_and(Game::joinable)
                    });
                if there {
                    together.crews.insert(entry.team.id.clone());
                    together.people.insert(id.to_owned());
                }
            }
        }
        together
    }

    /// In one of the player's crews (themself left out).
    fn is_teammate(&self, id: &str) -> bool {
        let me = self.me.as_ref().map(|me| me.profile.id.as_str());
        Some(id) != me
            && self
                .crews()
                .iter()
                .any(|entry| entry.team.members.iter().any(|m| m.profile.id == id))
    }

    /// The answer of a call that changed one of the player's crews (founding, joining, editing,
    /// roles): that crew, a new one last; with one crew per player it replaces the one there was.
    pub fn team_answer(&mut self, answer: TeamAnswer) {
        let multi = self.offers(TEAM_FEATURE_MULTI);
        let Some(team) = answer.team else {
            return;
        };
        if self.overview.is_none() {
            return;
        }
        // The crew as it is now carries its members' tags as they are now (TEAM6).
        self.badges_from(&team);
        let Some(overview) = &mut self.overview else {
            return;
        };
        overview.team_invites.retain(|i| i.team.id != team.id);
        overview.team_requests.retain(|r| r.team.id != team.id);
        let crews = overview.crews.get_or_insert_with(Vec::new);
        match crews.iter_mut().find(|entry| entry.team.id == team.id) {
            Some(entry) => {
                // Someone the player just took in is no longer asking.
                entry
                    .requests
                    .retain(|r| !team.members.iter().any(|m| m.profile.id == r.from.id));
                entry.team = team;
                entry.invited = answer.invited;
            }
            None => {
                if !multi {
                    crews.clear();
                }
                crews.push(CrewEntry {
                    team,
                    invited: answer.invited,
                    notifications: CrewMute::default(),
                    requests: Vec::new(),
                });
            }
        }
    }

    /// The player left one of their crews, or ended it (`ended`: every tag of it goes too).
    pub fn team_left(&mut self, team_id: &str, ended: bool) {
        if let Some(crews) = self.crews_mut() {
            crews.retain(|entry| entry.team.id != team_id);
        }
        self.crew_badges_gone(team_id, ended);
        self.prune_crew_calls();
        self.prune_crew_plans();
        self.prune_crew_joins();
    }

    // --- crew tags (TEAM6) ----------------------------------------------------------------------

    /// The crew whose tag the player wears now.
    fn my_badge_crew(&self) -> Option<&str> {
        self.me
            .as_ref()
            .and_then(|me| me.profile.badge.as_ref())
            .map(|badge| badge.crew.as_str())
    }

    /// One player's tag changed: their card everywhere they show (the profile, the friend list,
    /// every crew's member list).
    fn set_badge_of(&mut self, id: &str, badge: Option<&CrewBadge>) {
        if let Some(me) = self.me.as_mut().filter(|me| me.profile.id == id) {
            me.profile.badge = badge.cloned();
        }
        let Some(overview) = &mut self.overview else {
            return;
        };
        for friend in overview.friends.iter_mut().filter(|f| f.profile.id == id) {
            friend.profile.badge = badge.cloned();
        }
        for entry in overview.crews.iter_mut().flatten() {
            for member in entry.team.members.iter_mut().filter(|m| m.profile.id == id) {
                member.profile.badge = badge.cloned();
            }
        }
    }

    /// Someone's mark changed (SOC-VERIFY): it shows wherever they do, the player's own card too
    /// (friends, crews' members, requests either way, crews' join requests and invites, blocks).
    fn set_mark_of(&mut self, id: &str, mark: Option<ProfileMark>) {
        if let Some(me) = self.me.as_mut().filter(|me| me.profile.id == id) {
            me.profile.mark = mark;
        }
        let Some(overview) = &mut self.overview else {
            return;
        };
        let profiles = overview
            .friends
            .iter_mut()
            .map(|f| &mut f.profile)
            .chain(overview.incoming.iter_mut().map(|r| &mut r.profile))
            .chain(overview.outgoing.iter_mut().map(|r| &mut r.profile))
            .chain(overview.blocked.iter_mut().map(|b| &mut b.profile))
            .chain(overview.team_invites.iter_mut().map(|i| &mut i.from));
        for profile in profiles.filter(|p| p.id == id) {
            profile.mark = mark;
        }
        for entry in overview.crews.iter_mut().flatten() {
            for member in entry.team.members.iter_mut().filter(|m| m.profile.id == id) {
                member.profile.mark = mark;
            }
            for request in entry.requests.iter_mut().filter(|r| r.from.id == id) {
                request.from.mark = mark;
            }
        }
    }

    /// A crew as it is now carries its members' marks as they are now (SOC-VERIFY): those go
    /// everywhere the members show.
    fn marks_from(&mut self, team: &Team) {
        let marks: Vec<(String, Option<ProfileMark>)> = team
            .members
            .iter()
            .map(|member| (member.profile.id.clone(), member.profile.mark))
            .collect();
        for (id, mark) in marks {
            self.set_mark_of(&id, mark);
        }
    }

    /// A crew as it is now (its message, or an answer) carries its members' tags as they are now:
    /// those cards go everywhere they show; and since only members wear a crew's tag, anyone else
    /// shown with this crew's card is shown without it (the service tells them apart too).
    fn badges_from(&mut self, team: &Team) {
        let worn: HashMap<&str, Option<&CrewBadge>> = team
            .members
            .iter()
            .map(|member| (member.profile.id.as_str(), member.profile.badge.as_ref()))
            .collect();
        let fix = |profile: &mut PublicProfile| match worn.get(profile.id.as_str()) {
            Some(badge) => profile.badge = badge.cloned(),
            None if profile
                .badge
                .as_ref()
                .is_some_and(|badge| badge.crew == team.id) =>
            {
                profile.badge = None;
            }
            None => {}
        };
        if let Some(me) = &mut self.me {
            fix(&mut me.profile);
        }
        let Some(overview) = &mut self.overview else {
            return;
        };
        for friend in &mut overview.friends {
            fix(&mut friend.profile);
        }
        for entry in overview.crews.iter_mut().flatten() {
            for member in &mut entry.team.members {
                fix(&mut member.profile);
            }
        }
    }

    /// The player is no longer in crew `crew`: their tag of it goes; when the crew ended, every
    /// card of it goes.
    fn crew_badges_gone(&mut self, crew: &str, ended: bool) {
        if self.my_badge_crew() == Some(crew)
            && let Some(id) = self.me.as_ref().map(|me| me.profile.id.clone())
        {
            self.set_badge_of(&id, None);
        }
        if !ended {
            return;
        }
        let take_off = |profile: &mut PublicProfile| {
            if profile
                .badge
                .as_ref()
                .is_some_and(|badge| badge.crew == crew)
            {
                profile.badge = None;
            }
        };
        if let Some(overview) = &mut self.overview {
            for friend in &mut overview.friends {
                take_off(&mut friend.profile);
            }
            for entry in overview.crews.iter_mut().flatten() {
                for member in &mut entry.team.members {
                    take_off(&mut member.profile);
                }
            }
        }
    }

    /// A profile's tag as the view shows it: only while the service offers crew tags, and the
    /// card takes requests only while it offers requests from tags.
    fn badge_view(&self, profile: &PublicProfile) -> Option<Box<CrewBadgeView>> {
        if !self.offers(TEAM_FEATURE_BADGES) {
            return None;
        }
        let mut view = CrewBadgeView::from(profile.badge.as_ref()?);
        view.requests &= self.offers(TEAM_FEATURE_BADGE_REQUESTS);
        Some(Box::new(view))
    }

    /// The player's own tag as the view shows it.
    pub fn my_badge(&self) -> Option<Box<CrewBadgeView>> {
        self.badge_view(&self.me.as_ref()?.profile)
    }

    /// The leader hid the crew whose tag the player wore (said once).
    pub fn take_tag_off(&mut self) -> Option<TagOffView> {
        self.tag_off.take()
    }

    // --- "Hoş geldin" (TEAM6) ---------------------------------------------------------------

    /// Who came into `team` now (by code, link, invite or an accepted request): a line each in its
    /// chat, at most [`MAX_CREW_JOINS`] a crew (the oldest go).
    fn crew_joined(&mut self, team: &Team, joined: &[String], now: i64) {
        let mut added = false;
        for id in joined {
            let Some(member) = team.members.iter().find(|m| &m.profile.id == id) else {
                continue;
            };
            self.joins.push(CrewJoin {
                crew: team.id.clone(),
                id: id.clone(),
                name: member.profile.name().to_owned(),
                at: now,
                greeted: false,
            });
            added = true;
        }
        if !added {
            return;
        }
        let mut extra = self
            .joins
            .iter()
            .filter(|join| join.crew == team.id)
            .count()
            .saturating_sub(MAX_CREW_JOINS);
        self.joins.retain(|join| {
            if extra > 0 && join.crew == team.id {
                extra -= 1;
                false
            } else {
                true
            }
        });
    }

    /// Lines live in crews: those of a crew the player left go.
    fn prune_crew_joins(&mut self) {
        if self.overview.is_none() {
            return;
        }
        let crews: HashSet<String> = self.crew_order().into_iter().collect();
        self.joins.retain(|join| crews.contains(&join.crew));
    }

    /// The crews' "Ekibe katıldı" lines, oldest first.
    pub fn crew_joins(&self) -> Vec<CrewJoinView> {
        let me = self.me.as_ref().map(|me| me.profile.id.as_str());
        self.joins
            .iter()
            .map(|join| {
                let mine = me == Some(join.id.as_str());
                CrewJoinView {
                    crew: join.crew.clone(),
                    id: join.id.clone(),
                    name: join.name.clone(),
                    mine,
                    at: join.at,
                    greet_until: (!mine && !join.greeted).then_some(join.at + GREET_WINDOW_SECS),
                }
            })
            .collect()
    }

    /// "Selam ver" may still be pressed on `id`'s line in crew `crew` at `now`.
    pub fn may_greet(&self, crew: &str, id: &str, now: i64) -> bool {
        let me = self.me.as_ref().map(|me| me.profile.id.as_str());
        me != Some(id)
            && self.joins.iter().any(|join| {
                join.crew == crew
                    && join.id == id
                    && !join.greeted
                    && now < join.at + GREET_WINDOW_SECS
            })
    }

    /// "Selam ver" was pressed: the button goes from every line of `id` in that crew.
    pub fn greeted(&mut self, crew: &str, id: &str) {
        for join in self
            .joins
            .iter_mut()
            .filter(|join| join.crew == crew && join.id == id)
        {
            join.greeted = true;
        }
    }

    pub fn team_code(&mut self, team_id: &str, code: String) {
        if let Some(entry) = self
            .crews_mut()
            .and_then(|crews| crews.iter_mut().find(|entry| entry.team.id == team_id))
        {
            entry.team.code = code;
        }
    }

    /// One of the player's crews has a new invite link, or none.
    pub fn team_link(&mut self, team_id: &str, link: Option<String>) {
        if let Some(entry) = self
            .crews_mut()
            .and_then(|crews| crews.iter_mut().find(|entry| entry.team.id == team_id))
        {
            entry.team.link = link;
        }
    }

    /// The player invited a friend to one of their crews.
    pub fn team_invite_sent(&mut self, team_id: &str, friend: &str) {
        if let Some(entry) = self
            .crews_mut()
            .and_then(|crews| crews.iter_mut().find(|entry| entry.team.id == team_id))
            && !entry.invited.iter().any(|id| id == friend)
        {
            entry.invited.push(friend.to_owned());
        }
    }

    /// An invite to a team was answered (or turned out to be gone).
    pub fn team_invite_answered(&mut self, team_id: &str) {
        if let Some(overview) = &mut self.overview {
            overview.team_invites.retain(|i| i.team.id != team_id);
        }
    }

    // --- joining a crew (TEAM5) -----------------------------------------------------------------

    /// A crew's news may be told at `now`: not every crew muted, nor this one on its own.
    fn crew_heard(&self, crew: &str, now: i64) -> bool {
        !self.crew_muted(now)
            && self
                .crew(crew)
                .is_none_or(|entry| !entry.notifications.muted_at(now))
    }

    /// The player's join request waits now (sent here, or the service said so again).
    pub fn join_requested(&mut self, request: TeamRequest) {
        if let Some(overview) = &mut self.overview {
            overview
                .team_requests
                .retain(|r| r.team.id != request.team.id);
            overview.team_requests.insert(0, request);
        }
    }

    /// The player's join request to a crew is gone: withdrawn, or no longer there.
    pub fn join_request_withdrawn(&mut self, team_id: &str) {
        if let Some(overview) = &mut self.overview {
            overview.team_requests.retain(|r| r.team.id != team_id);
        }
    }

    /// A join request to one of the player's crews was answered (or turned out to be gone).
    pub fn join_request_answered(&mut self, team_id: &str, user: &str) {
        if let Some(entry) = self
            .crews_mut()
            .and_then(|crews| crews.iter_mut().find(|entry| entry.team.id == team_id))
        {
            entry.requests.retain(|r| r.from.id != user);
        }
    }

    /// The player's own join requests that still wait at `now`, newest first.
    pub fn team_requests(&self, now: i64) -> Vec<TeamRequestView> {
        self.overview
            .as_ref()
            .map(|o| {
                o.team_requests
                    .iter()
                    .filter(|r| now - r.sent_at < JOIN_REQUEST_LIFETIME_SECS)
                    .map(team_request_view)
                    .collect()
            })
            .unwrap_or_default()
    }

    // --- crew plans (TEAM5) ---------------------------------------------------------------------

    /// A plan, new or changed: replaces the one with its id.
    pub fn upsert_plan(&mut self, plan: CrewPlan) {
        match self.plans.iter_mut().find(|p| p.id == plan.id) {
            Some(known) => *known = plan,
            None => self.plans.push(plan),
        }
    }

    /// The player answered a plan here (the service also sends it to every device of theirs).
    pub fn plan_answered(&mut self, plan: &str, answer: PlanAnswer) {
        let Some(me) = self.me.as_ref().map(|me| me.profile.id.clone()) else {
            return;
        };
        if let Some(known) = self.plans.iter_mut().find(|p| p.id == plan) {
            known.answers.insert(me, answer);
        }
    }

    /// A plan was cancelled, or turned out to be gone.
    pub fn plan_gone(&mut self, plan: &str) {
        self.plans.retain(|p| p.id != plan);
    }

    /// A plan the player sees, by its id.
    pub fn plan(&self, id: &str) -> Option<&CrewPlan> {
        self.plans.iter().find(|p| p.id == id)
    }

    /// Plans that are over at `now` (on this computer's clock) go: nothing says so.
    fn prune_plans(&mut self, now: i64) {
        let skew = self.plan_skew;
        self.plans.retain(|plan| plan.until + skew > now);
    }

    /// Plans live in crews: those of a crew the player left go (the service says so too).
    fn prune_crew_plans(&mut self) {
        if self.overview.is_none() {
            return;
        }
        let crews: HashSet<String> = self.crew_order().into_iter().collect();
        self.plans.retain(|plan| crews.contains(&plan.crew));
    }

    /// The plans the player sees at `now` (local Unix seconds), soonest first, those under way
    /// included.
    pub fn plans(&self, now: i64) -> Vec<PlanView> {
        let mut plans: Vec<PlanView> = self
            .plans
            .iter()
            .filter(|plan| plan.until + self.plan_skew > now)
            .filter_map(|plan| self.plan_view(plan))
            .collect();
        plans.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
        plans
    }

    fn plan_view(&self, plan: &CrewPlan) -> Option<PlanView> {
        let entry = self.crew(&plan.crew)?;
        let me = self.me.as_ref().map(|me| me.profile.id.as_str());
        let members = &entry.team.members;
        let maker = members.iter().find(|m| m.profile.id == plan.by);
        let leads = members
            .iter()
            .find(|m| Some(m.profile.id.as_str()) == me)
            .is_some_and(|m| m.role == TeamRole::Founder || m.moderator);
        let mine = me == Some(plan.by.as_str());
        let skew = self.plan_skew;
        let notes = self.offers(TEAM_FEATURE_PLAN_NOTES);
        Some(PlanView {
            id: plan.id.clone(),
            crew: plan.crew.clone(),
            by: plan.by.clone(),
            by_name: maker.map(|m| m.profile.name().to_owned()),
            mine,
            can_cancel: mine || leads,
            at: plan.at,
            starts_at: plan.at + skew,
            ends_at: plan.until + skew,
            created_at: plan.created_at + skew,
            // In the crew's order; answers of people no longer in it are not shown.
            answers: members
                .iter()
                .filter_map(|m| {
                    plan.answers
                        .get(&m.profile.id)
                        .map(|answer| PlanAnswerView {
                            id: m.profile.id.clone(),
                            answer: *answer,
                        })
                })
                .collect(),
            my_answer: me.and_then(|me| plan.answers.get(me).copied()),
            note: plan.note.clone().filter(|_| notes),
            can_write_note: notes && mine,
            can_clear_note: notes && (mine || leads),
        })
    }

    /// The player's crews with everyone's presence, in the player's order.
    pub fn crew_views(&self, now: i64) -> Vec<TeamView> {
        self.crews()
            .iter()
            .filter_map(|entry| self.team_view(entry, now))
            .collect()
    }

    /// One crew with everyone's presence.
    fn team_view(&self, entry: &CrewEntry, now: i64) -> Option<TeamView> {
        let overview = self.overview.as_ref()?;
        let team = &entry.team;
        let me = self.me.as_ref().map(|me| me.profile.id.as_str());
        let members: Vec<TeamMemberView> = team
            .members
            .iter()
            .map(|member| {
                let id = member.profile.id.as_str();
                let mine = Some(id) == me;
                let presence = if mine {
                    None
                } else {
                    self.online
                        .get(id)
                        .filter(|p| p.state != PresenceState::Offline)
                };
                TeamMemberView {
                    id: id.to_owned(),
                    name: member.profile.name().to_owned(),
                    username: member.profile.username.clone(),
                    avatar: member.profile.avatar.clone(),
                    avatar_url: member.profile.discord_avatar_url(128),
                    role: member.role,
                    moderator: member.moderator && member.role != TeamRole::Founder,
                    me: mine,
                    friend: overview.friends.iter().any(|f| f.profile.id == id),
                    blocked: overview.blocked.iter().any(|b| b.profile.id == id),
                    state: presence.map_or(PresenceState::Offline, |p| p.state),
                    dnd: presence.is_some_and(|p| p.status == Some(OnlineStatus::Dnd)),
                    game: presence
                        .filter(|p| p.state == PresenceState::Playing)
                        .and_then(|p| p.game.clone())
                        .map(|game| Game {
                            location: game.location,
                            host: None,
                            ver: game.ver,
                        }),
                    discord_url: member.profile.discord_profile_url(),
                    joined_at: member.joined_at,
                    together_month: 0,
                    badge: self.badge_view(&member.profile),
                    mark: member.profile.mark,
                }
            })
            .collect();
        let mine = members.iter().find(|m| m.me);
        let role = mine.map_or(TeamRole::Member, |m| m.role);
        let moderator = mine.is_some_and(|m| m.moderator);
        let badges = self.offers(TEAM_FEATURE_BADGES);
        let requests = entry
            .requests
            .iter()
            .filter(|r| now - r.sent_at < JOIN_REQUEST_LIFETIME_SECS)
            .map(|request| join_request_view(request, &team.members, badges))
            .collect();
        let wearers = members
            .iter()
            .filter(|m| m.badge.as_ref().is_some_and(|badge| badge.crew == team.id))
            .count();
        Some(TeamView {
            id: team.id.clone(),
            name: team.name.clone(),
            tag: team.tag.clone(),
            emblem: team.emblem.clone(),
            color: team.color.clone(),
            frame: team.frame.clone(),
            motto: team.motto.clone(),
            code: crew_code(&team.code),
            created_on: team.created_on.clone(),
            role,
            moderator,
            permissions: team.permissions,
            members,
            invited: entry.invited.clone(),
            limit: overview.limits.team_members.unwrap_or(30),
            notifications: entry.notifications.at(now),
            text_locked_until: team.text_locked_until.filter(|until| *until > now),
            join: team.join,
            link: team.link.clone(),
            link_url: None,
            requests,
            together_week: 0,
            hidden: badges && team.hidden,
            badge_requests: team.badge_requests,
            wearers: u32::try_from(wearers).unwrap_or(u32::MAX),
        })
    }

    pub fn team_invites(&self) -> Vec<TeamInviteView> {
        self.overview
            .as_ref()
            .map(|o| o.team_invites.iter().map(team_invite_view).collect())
            .unwrap_or_default()
    }

    pub fn dismiss_invite(&mut self, id: &str) {
        if let Some(invite) = self.invites.iter().find(|i| i.id == id) {
            let at = self
                .dismissed
                .entry(invite.from.clone())
                .or_insert(invite.at);
            *at = (*at).max(invite.at);
        }
        self.invites.retain(|i| i.id != id);
    }

    /// Per friend, the time of the newest invite the player answered or closed (kept on disk by
    /// the service, so a restart within the hour does not bring them back).
    pub fn closed_invites(&self) -> &HashMap<String, i64> {
        &self.dismissed
    }

    /// Invites answered or closed in an earlier run: the service lists the last hour's again on
    /// every connection, and these stay closed until they expire. One already listed goes away.
    pub fn remember_closed_invites(&mut self, closed: HashMap<String, i64>, now: i64) {
        for (from, at) in closed {
            if now - at >= INVITE_LIFETIME_SECS {
                continue;
            }
            let newest = self.dismissed.entry(from).or_insert(at);
            *newest = (*newest).max(at);
        }
        let dismissed = &self.dismissed;
        self.invites
            .retain(|i| !dismissed.get(&i.from).is_some_and(|at| i.at <= *at));
    }

    /// Friends with their presence and the crew tags they wear, in the service's order.
    pub fn friends(&self) -> Vec<FriendView> {
        self.overview
            .as_ref()
            .map(|o| {
                o.friends
                    .iter()
                    .map(|friend| {
                        friend_view(
                            friend,
                            self.online.get(&friend.profile.id),
                            self.badge_view(&friend.profile),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn incoming(&self) -> Vec<RequestView> {
        self.overview
            .as_ref()
            .map(|o| o.incoming.iter().map(request_view).collect())
            .unwrap_or_default()
    }

    pub fn outgoing(&self) -> Vec<RequestView> {
        self.overview
            .as_ref()
            .map(|o| o.outgoing.iter().map(request_view).collect())
            .unwrap_or_default()
    }

    pub fn blocked_users(&self) -> Vec<ProfileView> {
        self.overview
            .as_ref()
            .map(|o| {
                o.blocked
                    .iter()
                    .map(|b| ProfileView::from(&b.profile))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Invites still worth answering at `now`, newest first.
    pub fn invites(&self, now: i64) -> Vec<InviteView> {
        let mut invites: Vec<&Invite> = self
            .invites
            .iter()
            .filter(|i| now - i.at < INVITE_LIFETIME_SECS)
            .filter(|i| self.is_friend(&i.from))
            .collect();
        invites.sort_by_key(|invite| std::cmp::Reverse(invite.at));
        invites.into_iter().map(|i| self.invite_view(i)).collect()
    }

    fn is_friend(&self, id: &str) -> bool {
        self.overview
            .as_ref()
            // Before the list arrives, an invite can only come from a friend.
            .is_none_or(|o| o.friends.iter().any(|f| f.profile.id == id))
    }

    /// On the friend list that has arrived (teammates who are not friends are not).
    fn is_listed_friend(&self, id: &str) -> bool {
        self.overview
            .as_ref()
            .is_some_and(|o| o.friends.iter().any(|f| f.profile.id == id))
    }

    fn friend_mut(&mut self, id: &str) -> Option<&mut Friend> {
        self.overview
            .as_mut()?
            .friends
            .iter_mut()
            .find(|f| f.profile.id == id)
    }

    fn friend_view(&self, id: &str) -> Option<FriendView> {
        let overview = self.overview.as_ref()?;
        let friend = overview.friends.iter().find(|f| f.profile.id == id)?;
        Some(friend_view(
            friend,
            self.online.get(id),
            self.badge_view(&friend.profile),
        ))
    }

    /// A presence message carries the friend's (or crewmate's) current picture.
    fn absorb_picture(&mut self, presence: &Presence) {
        if let Some(avatar) = presence.avatar.as_deref() {
            self.set_picture_of(&presence.id, avatar, presence.avatar_hash.as_deref());
        }
    }

    /// Someone's picture, wherever they show: the player's own card, the friend list and every
    /// crew's member list (SOC-LAG: the crew pages kept the old one until the next list).
    fn set_picture_of(&mut self, id: &str, avatar: &str, hash: Option<&str>) {
        let set = |profile: &mut PublicProfile| {
            avatar.clone_into(&mut profile.avatar);
            profile.avatar_hash = hash.map(str::to_owned);
        };
        if let Some(me) = self.me.as_mut().filter(|me| me.profile.id == id) {
            set(&mut me.profile);
        }
        let Some(overview) = &mut self.overview else {
            return;
        };
        for friend in overview.friends.iter_mut().filter(|f| f.profile.id == id) {
            set(&mut friend.profile);
        }
        for entry in overview.crews.iter_mut().flatten() {
            for member in entry.team.members.iter_mut().filter(|m| m.profile.id == id) {
                set(&mut member.profile);
            }
        }
    }

    fn add_friend(&mut self, friend: Friend) {
        let Some(overview) = &mut self.overview else {
            return;
        };
        let id = friend.profile.id.clone();
        overview.incoming.retain(|r| r.profile.id != id);
        overview.outgoing.retain(|r| r.profile.id != id);
        match overview.friends.iter_mut().find(|f| f.profile.id == id) {
            Some(existing) => {
                existing.profile = friend.profile;
                if !friend.since.is_empty() {
                    existing.since = friend.since;
                }
            }
            None => overview.friends.push(friend),
        }
    }

    /// Unfriended, blocked or deleted: gone from every list.
    fn forget(&mut self, id: &str) {
        self.online.remove(id);
        self.invites.retain(|i| i.from != id);
        if let Some(overview) = &mut self.overview {
            overview.friends.retain(|f| f.profile.id != id);
            overview.incoming.retain(|r| r.profile.id != id);
            overview.outgoing.retain(|r| r.profile.id != id);
        }
    }

    /// Returns true when the invite is new.
    fn add_invite(&mut self, invite: &Invite, quiet: bool, now: i64) -> bool {
        self.invites.retain(|i| now - i.at < INVITE_LIFETIME_SECS);
        self.dismissed
            .retain(|_, at| now - *at < INVITE_LIFETIME_SECS);
        if now - invite.at >= INVITE_LIFETIME_SECS
            || self.invites.iter().any(|i| i.id == invite.id)
            // Already answered or closed, or a newer one from the same friend was.
            || self.dismissed.get(&invite.from).is_some_and(|at| invite.at <= *at)
            // A `hello` lists older invites too: they never replace the newest.
            || self
                .invites
                .iter()
                .any(|i| i.from == invite.from && i.at >= invite.at)
        {
            return false;
        }
        // One invite per friend is enough: the newest replaces an older one.
        self.invites.retain(|i| i.from != invite.from);
        self.invites.push(Invite {
            quiet,
            ..invite.clone()
        });
        if self.invites.len() > MAX_INVITES {
            self.invites
                .sort_by_key(|invite| std::cmp::Reverse(invite.at));
            self.invites.truncate(MAX_INVITES);
        }
        true
    }

    fn invite_view(&self, invite: &Invite) -> InviteView {
        let friend = self
            .overview
            .as_ref()
            .and_then(|o| o.friends.iter().find(|f| f.profile.id == invite.from));
        InviteView {
            id: invite.id.clone(),
            from: invite.from.clone(),
            name: friend.map(|f| f.profile.name().to_owned()),
            avatar: friend.map(|f| f.profile.avatar.clone()),
            avatar_url: friend.and_then(|f| f.profile.discord_avatar_url(128)),
            mark: friend.and_then(|f| f.profile.mark),
            at: invite.at,
            server: invite.server.clone(),
            quiet: invite.quiet,
        }
    }
}

/// A call the player is coming to went somewhere new: a call from before CALL1 became "join" (its
/// caller entered Projects), or the call followed its caller to another server.
fn moved(before: &CrewCall, next: &CrewCall) -> bool {
    (before.kind != CallKind::Join && next.kind == CallKind::Join)
        || (before.server.is_some() && next.server.is_some() && before.server != next.server)
}

fn friend_view(
    friend: &Friend,
    presence: Option<&Presence>,
    badge: Option<Box<CrewBadgeView>>,
) -> FriendView {
    let profile = &friend.profile;
    let online = presence.filter(|p| p.state != PresenceState::Offline);
    FriendView {
        id: profile.id.clone(),
        name: profile.name().to_owned(),
        username: profile.username.clone(),
        avatar: profile.avatar.clone(),
        avatar_url: profile.discord_avatar_url(128),
        since: friend.since.clone(),
        last_seen: friend.last_seen,
        state: online.map_or(PresenceState::Offline, |p| p.state),
        dnd: online.is_some_and(|p| p.status == Some(OnlineStatus::Dnd)),
        game: online
            .filter(|p| p.state == PresenceState::Playing)
            .and_then(|p| p.game.clone()),
        discord_url: profile.discord_profile_url(),
        badge,
        mark: profile.mark,
    }
}

fn team_invite_view(invite: &TeamInvite) -> TeamInviteView {
    TeamInviteView {
        team_id: invite.team.id.clone(),
        name: invite.team.name.clone(),
        tag: invite.team.tag.clone(),
        emblem: invite.team.emblem.clone(),
        color: invite.team.color.clone(),
        frame: invite.team.frame.clone(),
        motto: invite.team.motto.clone(),
        members: invite.team.members,
        from: ProfileView::from(&invite.from),
        sent_at: invite.sent_at,
    }
}

/// A join request to a crew of `members`; `badges`: the service offers crew tags (a request from
/// a tag says through whose, TEAM6).
fn join_request_view(
    request: &JoinRequest,
    members: &[TeamMember],
    badges: bool,
) -> JoinRequestView {
    let profile = ProfileView::from(&request.from);
    // "Ayşe aracılığıyla": the member is in the crew's list while the service names them.
    let via = request
        .via
        .as_deref()
        .filter(|_| badges)
        .and_then(|via| members.iter().find(|m| m.profile.id == via));
    JoinRequestView {
        id: profile.id,
        name: profile.name,
        username: profile.username,
        avatar: profile.avatar,
        avatar_url: profile.avatar_url,
        mark: profile.mark,
        sent_at: request.sent_at,
        via: via.map(|m| m.profile.id.clone()),
        via_name: via.map(|m| m.profile.name().to_owned()),
    }
}

fn team_request_view(request: &TeamRequest) -> TeamRequestView {
    let team = &request.team;
    TeamRequestView {
        team_id: team.id.clone(),
        name: team.name.clone(),
        tag: team.tag.clone(),
        emblem: team.emblem.clone(),
        color: team.color.clone(),
        frame: team.frame.clone(),
        motto: team.motto.clone(),
        members: team.members,
        sent_at: request.sent_at,
    }
}

fn request_view(request: &FriendRequest) -> RequestView {
    let profile = ProfileView::from(&request.profile);
    RequestView {
        id: profile.id,
        name: profile.name,
        username: profile.username,
        avatar: profile.avatar,
        avatar_url: profile.avatar_url,
        mark: profile.mark,
        sent_at: request.sent_at,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::model::{LastSeen, Privacy, StatusMode};
    use crate::protocol::GameLocation;

    const NOW: i64 = 1_760_000_000;

    fn profile(id: &str, name: &str) -> PublicProfile {
        PublicProfile {
            id: id.into(),
            username: name.to_lowercase(),
            display_name: Some(name.into()),
            avatar: "discord".into(),
            avatar_hash: None,
            badge: None,
            mark: None,
        }
    }

    fn friend(id: &str, name: &str) -> Friend {
        Friend {
            profile: profile(id, name),
            since: "2026-10-01".into(),
            last_seen: Some(LastSeen::Week),
        }
    }

    fn me() -> Me {
        Me {
            profile: profile("100000000000000000", "Ben"),
            discord_avatar_hash: None,
            friend_code: "PRJ-AAAA-BBBB".into(),
            status: StatusMode::Online,
            privacy: Privacy {
                share_server: false,
                share_last_seen: true,
                share_read: None,
                share_typing: None,
            },
            created_on: "2026-10-16".into(),
            settings: None,
            limited: None,
            notices: Vec::new(),
        }
    }

    fn loaded() -> SocialState {
        let mut state = SocialState::default();
        state.set_me(me());
        state.set_overview(FriendsOverview {
            friends: vec![friend("1", "Ayşe"), friend("2", "Bora"), friend("3", "Can")],
            incoming: vec![FriendRequest {
                profile: profile("4", "Deniz"),
                sent_at: NOW - 60,
            }],
            outgoing: vec![FriendRequest {
                profile: profile("5", "Ece"),
                sent_at: NOW - 120,
            }],
            blocked: vec![],
            limits: Limits {
                friends: 200,
                outgoing_requests: 50,
                team_members: Some(30),
                groups: Some(20),
            },
            groups: Some(FriendGroups::default()),
            team: None,
            team_invited: vec![],
            team_invites: vec![],
            team_requests: vec![],
            team_features: vec!["look".into(), "calls".into()],
            team_notifications: None,
            crews: None,
            crew_limit: None,
            features: Vec::new(),
        });
        state
    }

    /// The first (primary) crew as the UI gets it.
    fn crew_view(state: &SocialState) -> Option<TeamView> {
        state.crew_views(NOW).into_iter().next()
    }

    fn presence(id: &str, state: PresenceState, game: Option<GameLocation>) -> Presence {
        Presence {
            id: id.into(),
            state,
            status: (state != PresenceState::Offline).then_some(OnlineStatus::Online),
            game: game.map(|location| Game {
                location,
                host: (location == GameLocation::Projects).then(|| "play.projects.gg".into()),
                ver: None,
            }),
            avatar: None,
            avatar_hash: None,
            last_seen: None,
        }
    }

    fn invite(id: &str, from: &str, at: i64, quiet: bool) -> Invite {
        Invite {
            id: id.into(),
            from: from.into(),
            at,
            server: Some(Game {
                location: GameLocation::Projects,
                host: Some("play.projects.gg".into()),
                ver: None,
            }),
            quiet,
        }
    }

    fn by_id<'a>(friends: &'a [FriendView], id: &str) -> &'a FriendView {
        friends.iter().find(|f| f.id == id).unwrap()
    }

    #[test]
    fn connected_replaces_the_online_list_without_notices() {
        let mut state = loaded();
        state.apply(
            &PresenceEvent::Presence(presence("3", PresenceState::Launcher, None)),
            NOW,
        );
        let notices = state.apply(
            &PresenceEvent::Connected {
                status: StatusMode::Dnd,
                friends: vec![
                    presence("1", PresenceState::Playing, Some(GameLocation::Projects)),
                    presence("2", PresenceState::Launcher, None),
                ],
                invites: vec![invite("i1", "1", NOW - 10, false)],
                calls: vec![],
                clock: None,
                chat: None,
                plans: None,
            },
            NOW,
        );
        // Coming online together after a (re)connect is not news.
        assert!(notices.is_empty());
        assert_eq!(state.connection(), Connection::Online);
        assert_eq!(state.me().unwrap().status, StatusMode::Dnd);
        let friends = state.friends();
        assert_eq!(by_id(&friends, "1").state, PresenceState::Playing);
        assert!(by_id(&friends, "1").game.as_ref().unwrap().joinable());
        assert_eq!(by_id(&friends, "2").state, PresenceState::Launcher);
        assert_eq!(by_id(&friends, "2").game, None);
        // Not in the list: offline now.
        assert_eq!(by_id(&friends, "3").state, PresenceState::Offline);
        // Invites that waited are quiet.
        assert!(state.invites(NOW)[0].quiet);
    }

    #[test]
    fn live_presence_changes_raise_notices_once() {
        let mut state = loaded();
        state.apply(
            &PresenceEvent::Connected {
                status: StatusMode::Online,
                friends: vec![],
                invites: vec![],
                calls: vec![],
                clock: None,
                chat: None,
                plans: None,
            },
            NOW,
        );
        let came_online = state.apply(
            &PresenceEvent::Presence(presence("2", PresenceState::Launcher, None)),
            NOW,
        );
        assert_eq!(
            came_online,
            vec![SocialNotice::FriendOnline { id: "2".into() }]
        );
        let joined = state.apply(
            &PresenceEvent::Presence(presence(
                "2",
                PresenceState::Playing,
                Some(GameLocation::Projects),
            )),
            NOW,
        );
        assert_eq!(
            joined,
            vec![SocialNotice::FriendOnProjects { id: "2".into() }]
        );
        // Still on projects.gg: nothing new.
        let again = state.apply(
            &PresenceEvent::Presence(presence(
                "2",
                PresenceState::Playing,
                Some(GameLocation::Projects),
            )),
            NOW,
        );
        assert!(again.is_empty());
        // Straight from offline into projects.gg: both.
        let both = state.apply(
            &PresenceEvent::Presence(presence(
                "3",
                PresenceState::Playing,
                Some(GameLocation::Projects),
            )),
            NOW,
        );
        assert_eq!(both.len(), 2);
        // Going offline keeps the coarse "last seen" the service sent.
        let mut gone = presence("2", PresenceState::Offline, None);
        gone.last_seen = Some(Some(LastSeen::Today));
        assert!(state.apply(&PresenceEvent::Presence(gone), NOW).is_empty());
        let friends = state.friends();
        assert_eq!(by_id(&friends, "2").state, PresenceState::Offline);
        assert_eq!(by_id(&friends, "2").last_seen, Some(LastSeen::Today));
    }

    #[test]
    fn do_not_disturb_and_hidden_servers() {
        let mut state = loaded();
        let mut busy = presence("1", PresenceState::Playing, Some(GameLocation::Other));
        busy.status = Some(OnlineStatus::Dnd);
        state.apply(&PresenceEvent::Presence(busy), NOW);
        let friend = by_id(&state.friends(), "1").clone();
        assert!(friend.dnd);
        assert_eq!(
            friend.game,
            Some(Game {
                location: GameLocation::Other,
                host: None,
                ver: None,
            })
        );
        assert!(!friend.game.unwrap().joinable());
    }

    /// SOC-LAG: "Son görülmemi göster" turned off reaches friends when the player goes offline
    /// (`lastSeen: null` forgets the friend list's); a presence without the field keeps it.
    #[test]
    fn a_hidden_last_seen_leaves_the_list_when_the_friend_goes_offline() {
        let mut state = loaded();
        assert_eq!(by_id(&state.friends(), "1").last_seen, Some(LastSeen::Week));
        state.apply(
            &PresenceEvent::Presence(presence("1", PresenceState::Offline, None)),
            NOW,
        );
        assert_eq!(by_id(&state.friends(), "1").last_seen, Some(LastSeen::Week));
        let mut hidden = presence("1", PresenceState::Offline, None);
        hidden.last_seen = Some(None);
        state.apply(&PresenceEvent::Presence(hidden), NOW);
        assert_eq!(by_id(&state.friends(), "1").last_seen, None);
        let mut shown = presence("1", PresenceState::Offline, None);
        shown.last_seen = Some(Some(LastSeen::Today));
        state.apply(&PresenceEvent::Presence(shown), NOW);
        assert_eq!(
            by_id(&state.friends(), "1").last_seen,
            Some(LastSeen::Today)
        );
    }

    /// SOC-LAG: a friend's or crewmate's new picture shows on the crew page too, and a change the
    /// player made on another of their devices shows here (switches and picture).
    #[test]
    fn pictures_and_the_players_own_switches_follow_live() {
        let mut state = loaded();
        state.team_answer(TeamAnswer {
            team: Some(team(&[
                ("100000000000000000", "Ben", TeamRole::Founder),
                ("1", "Ayşe", TeamRole::Member),
                ("9", "Zeki", TeamRole::Member),
            ])),
            invited: vec![],
        });
        let avatar_of = |state: &SocialState, id: &str| {
            crew_view(state)
                .unwrap()
                .members
                .into_iter()
                .find(|member| member.id == id)
                .unwrap()
                .avatar
        };
        // A friend in the crew, and a crewmate who is not a friend.
        for id in ["1", "9"] {
            let mut pictured = presence(id, PresenceState::Launcher, None);
            pictured.avatar = Some("preset:fox".into());
            state.apply(&PresenceEvent::Presence(pictured), NOW);
            assert_eq!(avatar_of(&state, id), "preset:fox");
        }
        assert_eq!(by_id(&state.friends(), "1").avatar, "preset:fox");
        // The player's other device.
        state.apply(
            &PresenceEvent::ProfileChanged {
                privacy: Privacy {
                    share_server: true,
                    share_last_seen: false,
                    share_read: Some(false),
                    share_typing: Some(true),
                },
                avatar: Some("preset:owl".into()),
                avatar_hash: None,
            },
            NOW,
        );
        let me = state.me().unwrap().clone();
        assert!(me.privacy.share_server && !me.privacy.share_last_seen);
        assert_eq!(me.privacy.share_read, Some(false));
        assert_eq!(me.profile.avatar, "preset:owl");
        assert_eq!(avatar_of(&state, "100000000000000000"), "preset:owl");
        // The service's answer (`set_me`) puts the player's picture on their crew pages too.
        let mut answered = me;
        answered.profile.avatar = "discord".into();
        state.set_me(answered);
        assert_eq!(avatar_of(&state, "100000000000000000"), "discord");
    }

    #[test]
    fn requests_friends_and_blocks() {
        let mut state = loaded();
        // A request arrives live.
        let notices = state.apply(
            &PresenceEvent::RequestReceived {
                from: profile("6", "Fatma"),
            },
            NOW,
        );
        assert!(
            matches!(&notices[..], [SocialNotice::Request { request }] if request.name == "Fatma")
        );
        assert_eq!(state.incoming()[0].id, "6");
        // ... and is withdrawn.
        state.apply(&PresenceEvent::RequestCancelled { id: "6".into() }, NOW);
        assert!(state.incoming().iter().all(|r| r.id != "6"));

        // Accepting moves the request into the friends.
        state.request_accepted(friend("4", "Deniz"));
        assert!(state.incoming().is_empty());
        assert!(state.friends().iter().any(|f| f.id == "4"));

        // Ece accepts our request on her side.
        let notices = state.apply(
            &PresenceEvent::FriendAdded {
                friend: profile("5", "Ece"),
                presence: presence("5", PresenceState::Launcher, None),
            },
            NOW,
        );
        assert!(
            matches!(&notices[..], [SocialNotice::FriendAdded { friend }] if friend.id == "5" && friend.state == PresenceState::Launcher)
        );
        assert!(state.outgoing().is_empty());

        // Sending by code: pending, or friends at once.
        state.request_sent(RequestOutcome::Requested {
            user: profile("7", "Gül"),
            sent_at: NOW,
        });
        assert_eq!(state.outgoing()[0].id, "7");
        state.request_sent(RequestOutcome::Friends {
            friend: friend("7", "Gül"),
        });
        assert!(state.outgoing().is_empty());
        assert!(state.friends().iter().any(|f| f.id == "7"));

        // Blocking takes them out of everything.
        state.apply(
            &PresenceEvent::Presence(presence("1", PresenceState::Launcher, None)),
            NOW,
        );
        let ayse = state.profile("1").unwrap();
        state.blocked(ayse);
        assert!(state.friends().iter().all(|f| f.id != "1"));
        assert_eq!(state.blocked_users()[0].id, "1");
        state.unblocked("1");
        assert!(state.blocked_users().is_empty());

        // Removed on the other side (or their account went away).
        state.apply(&PresenceEvent::FriendRemoved { id: "2".into() }, NOW);
        assert!(state.friends().iter().all(|f| f.id != "2"));
        state.request_declined("nobody");
        state.request_cancelled("nobody");
        state.friend_removed("3");
        assert!(state.friends().iter().all(|f| f.id != "3"));
    }

    #[test]
    fn invites_are_deduplicated_expire_and_respect_quiet() {
        let mut state = loaded();
        let live = state.apply(&PresenceEvent::Invite(invite("i1", "1", NOW, false)), NOW);
        assert!(
            matches!(&live[..], [SocialNotice::Invite { invite }] if invite.name.as_deref() == Some("Ayşe") && !invite.quiet)
        );
        // The same invite twice is one invite.
        assert!(
            state
                .apply(&PresenceEvent::Invite(invite("i1", "1", NOW, false)), NOW)
                .is_empty()
        );
        // A newer one from the same friend replaces the older.
        state.apply(
            &PresenceEvent::Invite(invite("i2", "1", NOW + 5, false)),
            NOW + 5,
        );
        assert_eq!(state.invites(NOW + 5).len(), 1);
        assert_eq!(state.invites(NOW + 5)[0].id, "i2");
        // Quiet ones (do-not-disturb) never become toasts.
        let quiet = state.apply(
            &PresenceEvent::QueuedInvites(vec![invite("i3", "2", NOW, false)]),
            NOW,
        );
        assert!(quiet.is_empty());
        assert!(state.invites(NOW).iter().any(|i| i.id == "i3" && i.quiet));
        // An hour later they are gone; an invite from a stranger never shows.
        assert!(state.invites(NOW + INVITE_LIFETIME_SECS + 10).is_empty());
        assert!(
            state
                .apply(&PresenceEvent::Invite(invite("i4", "999", NOW, false)), NOW)
                .is_empty()
        );
        assert!(state.invites(NOW).iter().all(|i| i.id != "i4"));
        state.dismiss_invite("i2");
        assert!(state.invites(NOW + 5).iter().all(|i| i.id != "i2"));
        // Unfriending drops their invites.
        state.apply(&PresenceEvent::FriendRemoved { id: "2".into() }, NOW);
        assert!(state.invites(NOW).is_empty());
    }

    /// Every `hello` (and leaving do-not-disturb) lists all invites of the last hour again,
    /// newest first.
    fn reconnect_with(invites: Vec<Invite>) -> PresenceEvent {
        PresenceEvent::Connected {
            status: StatusMode::Online,
            friends: vec![],
            invites,
            calls: vec![],
            clock: None,
            chat: None,
            plans: None,
        }
    }

    #[test]
    fn a_reconnect_keeps_the_newest_invite_of_a_friend() {
        let mut state = loaded();
        state.apply(&PresenceEvent::Invite(invite("i1", "1", NOW, false)), NOW);
        state.apply(
            &PresenceEvent::Invite(invite("i2", "1", NOW + 400, false)),
            NOW + 400,
        );
        state.apply(
            &reconnect_with(vec![
                invite("i2", "1", NOW + 400, true),
                invite("i1", "1", NOW, true),
            ]),
            NOW + 500,
        );
        let invites = state.invites(NOW + 500);
        assert_eq!(invites.len(), 1);
        assert_eq!(invites[0].id, "i2");
        // An older one arriving alone does not replace it either.
        state.apply(
            &PresenceEvent::QueuedInvites(vec![invite("i1", "1", NOW, true)]),
            NOW + 600,
        );
        assert_eq!(state.invites(NOW + 600)[0].id, "i2");
    }

    #[test]
    fn a_dismissed_invite_does_not_come_back() {
        let mut state = loaded();
        state.apply(&PresenceEvent::Invite(invite("i1", "1", NOW, false)), NOW);
        state.apply(
            &PresenceEvent::Invite(invite("i2", "2", NOW + 10, false)),
            NOW + 10,
        );
        // Answered ("Katıl") or closed in the panel.
        state.dismiss_invite("i1");
        state.apply(
            &reconnect_with(vec![
                invite("i2", "2", NOW + 10, true),
                invite("i1", "1", NOW, true),
            ]),
            NOW + 60,
        );
        state.apply(
            &PresenceEvent::QueuedInvites(vec![invite("i1", "1", NOW, true)]),
            NOW + 90,
        );
        let invites = state.invites(NOW + 90);
        assert_eq!(invites.len(), 1);
        assert_eq!(invites[0].id, "i2");
        // A newer invite from the same friend shows as usual, with its toast.
        let live = state.apply(
            &PresenceEvent::Invite(invite("i3", "1", NOW + 400, false)),
            NOW + 400,
        );
        assert_eq!(live.len(), 1);
        assert!(state.invites(NOW + 400).iter().any(|i| i.id == "i3"));
    }

    #[test]
    fn invites_closed_before_a_restart_stay_closed() {
        let mut before = loaded();
        before.apply(&PresenceEvent::Invite(invite("i1", "1", NOW, false)), NOW);
        before.dismiss_invite("i1");
        let closed = before.closed_invites().clone();
        assert_eq!(closed.get("1"), Some(&NOW));
        // The launcher restarts within the hour: a new state, the closed invites from disk,
        // then the `hello` lists the last hour's invites again.
        let mut after = loaded();
        after.remember_closed_invites(closed.clone(), NOW + 300);
        after.apply(
            &reconnect_with(vec![
                invite("i2", "2", NOW + 10, true),
                invite("i1", "1", NOW, true),
            ]),
            NOW + 300,
        );
        let invites = after.invites(NOW + 300);
        assert_eq!(invites.len(), 1);
        assert_eq!(invites[0].id, "i2");
        // A newer invite from the same friend shows as usual, with its toast.
        let live = after.apply(
            &PresenceEvent::Invite(invite("i3", "1", NOW + 400, false)),
            NOW + 400,
        );
        assert_eq!(live.len(), 1);
        // The profile came after the `hello` (the service was down at start): the listed
        // invite goes away once the closed ones are known.
        let mut late = loaded();
        late.apply(
            &reconnect_with(vec![invite("i1", "1", NOW, true)]),
            NOW + 300,
        );
        assert_eq!(late.invites(NOW + 300).len(), 1);
        late.remember_closed_invites(closed.clone(), NOW + 300);
        assert!(late.invites(NOW + 300).is_empty());
        // An hour after the invite there is nothing left to keep closed.
        let mut later = loaded();
        later.remember_closed_invites(closed, NOW + INVITE_LIFETIME_SECS);
        assert!(later.closed_invites().is_empty());
    }

    #[test]
    fn connection_states() {
        let mut state = loaded();
        state.apply(
            &PresenceEvent::Connected {
                status: StatusMode::Online,
                friends: vec![presence("1", PresenceState::Launcher, None)],
                invites: vec![],
                calls: vec![],
                clock: None,
                chat: None,
                plans: None,
            },
            NOW,
        );
        state.apply(
            &PresenceEvent::Disconnected {
                retry_in: Duration::from_secs(3),
            },
            NOW,
        );
        assert_eq!(state.connection(), Connection::Reconnecting);
        // Presence stays while reconnecting (no flicker) ...
        assert_eq!(by_id(&state.friends(), "1").state, PresenceState::Launcher);
        // ... but a failed attempt is an outage: nobody's presence is known then.
        state.apply(
            &PresenceEvent::Disconnected {
                retry_in: Duration::from_secs(8),
            },
            NOW,
        );
        assert_eq!(state.connection(), Connection::Unavailable);
        assert_eq!(by_id(&state.friends(), "1").state, PresenceState::Offline);
        state.apply(&PresenceEvent::Stopped(StopReason::Replaced), NOW);
        assert_eq!(state.connection(), Connection::Replaced);
        state.apply(&PresenceEvent::StatusChanged(StatusMode::Invisible), NOW);
        assert_eq!(state.me().unwrap().status, StatusMode::Invisible);
    }

    fn team(members: &[(&str, &str, TeamRole)]) -> Team {
        Team {
            id: "0123456789abcdef".into(),
            name: "Gece Kartalları".into(),
            tag: "GKR".into(),
            emblem: "sword".into(),
            color: "teal".into(),
            frame: "banner".into(),
            motto: "Birlikte güçlüyüz!".into(),
            code: "TKM-ABCD-EFGH".into(),
            created_on: "2026-10-19".into(),
            members: members
                .iter()
                .enumerate()
                .map(|(i, (id, name, role))| crate::model::TeamMember {
                    profile: profile(id, name),
                    role: *role,
                    moderator: false,
                    joined_at: i64::try_from(i).unwrap(),
                })
                .collect(),
            permissions: crate::model::TeamPermissions::default(),
            text_locked_until: None,
            join: crate::model::JoinMode::Open,
            link: None,
            hidden: false,
            badge_requests: true,
        }
    }

    #[test]
    fn teammates_have_presence_but_no_friend_notices() {
        let mut state = loaded();
        state.team_answer(TeamAnswer {
            team: Some(team(&[
                ("100000000000000000", "Ben", TeamRole::Founder),
                ("1", "Ayşe", TeamRole::Member),
                ("9", "Zeki", TeamRole::Member),
            ])),
            invited: vec!["2".into()],
        });
        // Zeki is a teammate, not a friend: presence yes, "came online" toasts no; joining
        // Projects is crew news (the launcher says it only when the player asked for it).
        let notices = state.apply(
            &PresenceEvent::Presence(presence(
                "9",
                PresenceState::Playing,
                Some(GameLocation::Projects),
            )),
            NOW,
        );
        assert_eq!(
            notices,
            vec![SocialNotice::CrewmateOnProjects { id: "9".into() }]
        );
        let view = crew_view(&state).unwrap();
        assert_eq!(view.role, TeamRole::Founder);
        assert_eq!(view.invited, vec!["2".to_owned()]);
        // The crew shows its code with the crew prefix, and its look.
        assert_eq!(view.code, "EKP-ABCD-EFGH");
        assert_eq!(
            (view.color.as_str(), view.frame.as_str()),
            ("teal", "banner")
        );
        assert_eq!(view.motto, "Birlikte güçlüyüz!");
        let zeki = view.members.iter().find(|m| m.id == "9").unwrap();
        assert!(!zeki.friend && !zeki.me);
        assert_eq!(zeki.state, PresenceState::Playing);
        assert!(zeki.game.as_ref().unwrap().joinable());
        assert_eq!(zeki.game.as_ref().unwrap().host, None);
        let me = view.members.iter().find(|m| m.me).unwrap();
        assert_eq!(me.state, PresenceState::Offline);
        assert!(view.members.iter().find(|m| m.id == "1").unwrap().friend);
        // A reconnect lists teammates with the friends.
        state.apply(
            &PresenceEvent::Connected {
                status: StatusMode::Online,
                friends: vec![presence("9", PresenceState::Launcher, None)],
                invites: vec![],
                calls: vec![],
                clock: None,
                chat: None,
                plans: None,
            },
            NOW,
        );
        let zeki = crew_view(&state).unwrap().members[2].clone();
        assert_eq!(zeki.state, PresenceState::Launcher);
    }

    #[test]
    fn a_mark_shows_wherever_its_player_does_and_follows_live_changes() {
        let mut state = loaded();
        state.team_answer(TeamAnswer {
            team: Some(team(&[
                ("100000000000000000", "Ben", TeamRole::Founder),
                ("1", "Ayşe", TeamRole::Member),
                ("9", "Zeki", TeamRole::Member),
            ])),
            invited: vec![],
        });
        let mark_of = |state: &SocialState, id: &str| {
            state
                .friends()
                .iter()
                .find(|f| f.id == id)
                .and_then(|f| f.mark)
        };
        assert_eq!(mark_of(&state, "1"), None);
        // Ayşe (a friend and crewmate) is verified: her row, her crew entry.
        state.apply(
            &PresenceEvent::MarkChanged {
                id: "1".into(),
                mark: Some(ProfileMark::Verified),
            },
            NOW,
        );
        assert_eq!(mark_of(&state, "1"), Some(ProfileMark::Verified));
        let ayse = crew_view(&state).unwrap().members[1].clone();
        assert_eq!(ayse.mark, Some(ProfileMark::Verified));
        // Requests either way carry it too.
        state.apply(
            &PresenceEvent::MarkChanged {
                id: "4".into(),
                mark: Some(ProfileMark::Admin),
            },
            NOW,
        );
        assert_eq!(state.incoming()[0].mark, Some(ProfileMark::Admin));
        assert_eq!(state.outgoing()[0].mark, None);
        // The player's own mark: their card and their own crew entry.
        state.apply(
            &PresenceEvent::MarkChanged {
                id: "100000000000000000".into(),
                mark: Some(ProfileMark::Admin),
            },
            NOW,
        );
        assert_eq!(state.me().unwrap().profile.mark, Some(ProfileMark::Admin));
        assert!(
            crew_view(&state)
                .unwrap()
                .members
                .iter()
                .any(|m| m.me && m.mark == Some(ProfileMark::Admin))
        );
        // A crew's message carries its members' marks as they are now: they go everywhere.
        let mut now = team(&[
            ("100000000000000000", "Ben", TeamRole::Founder),
            ("1", "Ayşe", TeamRole::Member),
            ("9", "Zeki", TeamRole::Member),
        ]);
        now.members[2].profile.mark = Some(ProfileMark::Verified);
        state.apply(
            &PresenceEvent::TeamChanged {
                team: Some(now),
                crew: Some("0123456789abcdef".into()),
                reason: None,
                by: None,
                joined: vec![],
            },
            NOW,
        );
        // Taken off: the mark goes from every place, Ayşe's too (the crew carries none for her).
        assert_eq!(mark_of(&state, "1"), None);
        let view = crew_view(&state).unwrap();
        assert_eq!(view.members[2].mark, Some(ProfileMark::Verified));
        assert_eq!(view.members[0].mark, None);
    }

    fn crew_call(id: &str, from: &str, kind: CallKind, at: i64, until: i64) -> CrewCall {
        CrewCall {
            id: id.into(),
            from: from.into(),
            kind,
            at,
            until,
            answers: std::collections::BTreeMap::new(),
            crews: vec![],
            server: None,
        }
    }

    const ME: &str = "100000000000000000";

    fn crewed() -> SocialState {
        let mut state = loaded();
        state.team_answer(TeamAnswer {
            team: Some(team(&[
                (ME, "Ben", TeamRole::Founder),
                ("1", "Ayşe", TeamRole::Member),
                ("9", "Zeki", TeamRole::Member),
            ])),
            invited: vec![],
        });
        state
    }

    #[test]
    fn a_muted_crew_stays_quiet_until_the_mute_ends() {
        let mut state = crewed();
        let call = |id: &str| PresenceEvent::Call {
            call: crew_call(id, "1", CallKind::Ready, NOW, NOW + 1200),
            quiet: false,
            clock: Some(NOW),
        };
        let projects = |id: &str, state: PresenceState| {
            PresenceEvent::Presence(presence(
                id,
                state,
                (state == PresenceState::Playing).then_some(GameLocation::Projects),
            ))
        };
        // A service without `mute` keeps no settings: nothing is muted, the launcher's own
        // switches decide.
        state.apply(
            &PresenceEvent::CrewNotifications(TeamNotifications {
                calls: false,
                muted: true,
                mute_until: None,
            }),
            NOW,
        );
        assert_eq!(state.team_notifications(), None);
        assert_eq!(state.apply(&call("c0"), NOW).len(), 1);

        // With `mute`: settings from another device arrive live and silence the crew.
        if let Some(overview) = &mut state.overview {
            overview.team_features.push(TEAM_FEATURE_MUTE.into());
            overview.team_notifications = None;
        }
        assert_eq!(
            state.team_notifications(),
            Some(TeamNotifications::default()),
            "a service with `mute` that sent nothing yet: the defaults"
        );
        let until = NOW + 3_600;
        state.apply(
            &PresenceEvent::CrewNotifications(TeamNotifications {
                calls: true,
                muted: true,
                mute_until: Some(until),
            }),
            NOW,
        );
        assert!(state.apply(&call("c1"), NOW).is_empty());
        assert!(
            state
                .apply(&projects("9", PresenceState::Playing), NOW)
                .is_empty()
        );
        // Quiet, but there to answer.
        assert_eq!(state.calls(NOW).len(), 2);

        // The mute ends by itself: no message, the clock says so.
        state.apply(&projects("9", PresenceState::Launcher), until);
        assert_eq!(
            state.apply(&projects("9", PresenceState::Playing), until),
            vec![SocialNotice::CrewmateOnProjects { id: "9".into() }]
        );
        assert!(matches!(
            &state.apply(&call("c2"), until)[..],
            [SocialNotice::CrewCall { call }] if call.id == "c2"
        ));

        // Crew calls switched off: calls are quiet, a crewmate joining Projects is not.
        state.set_team_notifications(TeamNotifications {
            calls: false,
            muted: false,
            mute_until: None,
        });
        assert!(state.apply(&call("c3"), until).is_empty());
        state.apply(&projects("9", PresenceState::Launcher), until);
        assert_eq!(
            state
                .apply(&projects("9", PresenceState::Playing), until)
                .len(),
            1
        );
    }

    #[test]
    fn calls_keep_their_time_left_on_this_clock_and_say_only_what_is_new() {
        let mut state = crewed();
        // The service's clock is 5 minutes behind this computer's: a call with 15 minutes left
        // there has 15 minutes left here too.
        let service_now = NOW - 300;
        let notices = state.apply(
            &PresenceEvent::Connected {
                status: StatusMode::Online,
                friends: vec![],
                invites: vec![],
                calls: vec![crew_call(
                    "c1",
                    "9",
                    CallKind::Ready,
                    service_now - 300,
                    service_now + 900,
                )],
                clock: Some(service_now),
                chat: None,
                plans: None,
            },
            NOW,
        );
        assert!(notices.is_empty(), "calls listed on connect are quiet");
        let calls = state.calls(NOW);
        assert_eq!(calls.len(), 1);
        assert_eq!(
            (calls[0].started_at, calls[0].ends_at),
            (NOW - 300, NOW + 900)
        );
        assert_eq!(calls[0].name.as_deref(), Some("Zeki"));
        assert!(!calls[0].mine);

        // A new call, live: announced once with the caller's name.
        let ayse = crew_call("c2", "1", CallKind::Join, NOW, NOW + 1200);
        let live = state.apply(
            &PresenceEvent::Call {
                call: ayse.clone(),
                quiet: false,
                clock: Some(NOW),
            },
            NOW,
        );
        assert!(
            matches!(&live[..], [SocialNotice::CrewCall { call }] if call.id == "c2" && call.kind == CallKind::Join && call.name.as_deref() == Some("Ayşe"))
        );
        // The same call again (a resend) and quiet ones say nothing.
        for quiet in [false, true] {
            assert!(
                state
                    .apply(
                        &PresenceEvent::Call {
                            call: ayse.clone(),
                            quiet,
                            clock: Some(NOW)
                        },
                        NOW
                    )
                    .is_empty()
            );
        }
        // The player's own call (on every device) is never news.
        let mine = state.apply(
            &PresenceEvent::Call {
                call: crew_call("c3", ME, CallKind::Ready, NOW, NOW + 1200),
                quiet: false,
                clock: Some(NOW),
            },
            NOW,
        );
        assert!(mine.is_empty());
        assert_eq!(state.my_call().map(|c| c.id.as_str()), Some("c3"));

        // "Geliyorum" to Zeki's call, then Zeki goes to projects.gg: "Zeki Projects'te".
        state.call_answered("c1", CallAnswer::Yes);
        state.apply(
            &PresenceEvent::CallAnswered {
                call: "c1".into(),
                by: "1".into(),
                answer: CallAnswer::No,
            },
            NOW,
        );
        let view = state.calls(NOW).into_iter().find(|c| c.id == "c1").unwrap();
        assert_eq!(view.my_answer, Some(CallAnswer::Yes));
        // In the crew's order: the player (founder) first, then Ayşe.
        assert_eq!(
            view.answers
                .iter()
                .map(|a| (a.id.as_str(), a.answer))
                .collect::<Vec<_>>(),
            vec![(ME, CallAnswer::Yes), ("1", CallAnswer::No)]
        );
        let mut joined = crew_call(
            "c1",
            "9",
            CallKind::Join,
            service_now - 300,
            service_now + 900,
        );
        joined.answers.insert(ME.into(), CallAnswer::Yes);
        let arrived = state.apply(
            &PresenceEvent::Call {
                call: joined,
                quiet: false,
                clock: Some(service_now + 60),
            },
            NOW + 60,
        );
        assert!(matches!(&arrived[..], [SocialNotice::CrewCallJoin { call }] if call.id == "c1"));
        // The service's copy replaced the old one (answers that went away are gone).
        assert_eq!(
            state
                .calls(NOW + 60)
                .into_iter()
                .find(|c| c.id == "c1")
                .unwrap()
                .answers
                .len(),
            1
        );

        // Running out needs no message; ending does.
        assert!(state.calls(NOW + 901).iter().all(|c| c.id != "c1"));
        state.apply(
            &PresenceEvent::CallEnded {
                call: "c2".into(),
                reason: crate::protocol::CallEndReason::Ended,
            },
            NOW,
        );
        assert_eq!(
            state
                .calls(NOW)
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c1", "c3"]
        );

        // An outage, or leaving the crew, takes every call away.
        state.set_connection(Connection::Unavailable);
        assert!(state.calls(NOW).is_empty());
        state.apply(
            &PresenceEvent::Call {
                call: crew_call("c4", "1", CallKind::Ready, NOW, NOW + 1200),
                quiet: true,
                clock: Some(NOW),
            },
            NOW,
        );
        state.apply(
            &PresenceEvent::TeamChanged {
                team: None,
                crew: None,
                reason: Some(TeamExit::Left),
                by: Some(ME.into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(state.calls(NOW).is_empty());
    }

    #[test]
    fn moderators_and_permissions_reach_the_view_and_take_invites_along() {
        use crate::model::PermissionLevel;
        let mut state = loaded();
        // The player is a moderator in Ayşe's crew, where only moderators invite.
        let mut led = team(&[
            ("1", "Ayşe", TeamRole::Founder),
            (ME, "Ben", TeamRole::Member),
            ("9", "Zeki", TeamRole::Member),
        ]);
        led.members[1].moderator = true;
        led.permissions.invite = PermissionLevel::Moderators;
        state.team_answer(TeamAnswer {
            team: Some(led.clone()),
            invited: vec!["2".into()],
        });
        let view = crew_view(&state).unwrap();
        assert!(view.moderator && view.role == TeamRole::Member);
        assert_eq!(view.permissions.invite, PermissionLevel::Moderators);
        assert!(view.members.iter().find(|m| m.me).unwrap().moderator);
        assert!(!view.members.iter().find(|m| m.id == "1").unwrap().moderator);
        assert_eq!(view.invited, vec!["2".to_owned()]);

        // The leader takes the flag back: the player's waiting invites go with the right.
        led.members[1].moderator = false;
        state.apply(
            &PresenceEvent::TeamChanged {
                team: Some(led),
                crew: None,
                reason: None,
                by: Some("1".into()),
                joined: Vec::new(),
            },
            NOW,
        );
        let view = crew_view(&state).unwrap();
        assert!(!view.moderator);
        assert!(view.invited.is_empty());
    }

    #[test]
    fn a_call_of_someone_who_left_or_was_blocked_goes() {
        let mut state = crewed();
        state.apply(
            &PresenceEvent::Call {
                call: crew_call("c1", "9", CallKind::Ready, NOW, NOW + 1200),
                quiet: true,
                clock: Some(NOW),
            },
            NOW,
        );
        state.apply(
            &PresenceEvent::Call {
                call: crew_call("c2", "1", CallKind::Ready, NOW, NOW + 1200),
                quiet: true,
                clock: Some(NOW),
            },
            NOW,
        );
        // Zeki left the crew.
        state.apply(
            &PresenceEvent::TeamChanged {
                team: Some(team(&[
                    (ME, "Ben", TeamRole::Founder),
                    ("1", "Ayşe", TeamRole::Member),
                ])),
                crew: None,
                reason: None,
                by: None,
                joined: Vec::new(),
            },
            NOW,
        );
        assert_eq!(state.calls(NOW).len(), 1);
        // Blocking Ayşe hides hers.
        let ayse = state.profile("1").unwrap();
        state.blocked(ayse);
        assert!(state.calls(NOW).is_empty());
    }

    #[test]
    fn team_changes_invites_and_leaving() {
        let mut state = loaded();
        let invite = TeamInvite {
            team: crate::model::TeamSummary {
                id: "fedcba9876543210".into(),
                name: "PvP".into(),
                tag: "PVP".into(),
                emblem: "bolt".into(),
                color: "crimson".into(),
                frame: "hex".into(),
                motto: String::new(),
                members: 4,
                join: crate::model::JoinMode::Open,
            },
            from: profile("2", "Bora"),
            sent_at: NOW,
        };
        let notices = state.apply(&PresenceEvent::TeamInvited(invite.clone()), NOW);
        assert!(
            matches!(&notices[..], [SocialNotice::TeamInvite { invite }] if invite.name == "PvP" && invite.from.name == "Bora")
        );
        // The same invite again replaces it.
        state.apply(&PresenceEvent::TeamInvited(invite), NOW);
        assert_eq!(state.team_invites().len(), 1);

        // Joining that team (on another device): the invite goes, the team comes.
        let mut joined = team(&[
            ("2", "Bora", TeamRole::Founder),
            ("100000000000000000", "Ben", TeamRole::Member),
        ]);
        joined.id = "fedcba9876543210".into();
        state.apply(
            &PresenceEvent::TeamChanged {
                team: Some(joined),
                crew: None,
                reason: None,
                by: None,
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(state.team_invites().is_empty());
        assert_eq!(crew_view(&state).unwrap().role, TeamRole::Member);

        // Removed by the founder: told once, with the team's name.
        let removed = state.apply(
            &PresenceEvent::TeamChanged {
                team: None,
                crew: None,
                reason: Some(TeamExit::Kicked),
                by: Some("2".into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(
            matches!(&removed[..], [SocialNotice::TeamGone { reason: TeamExit::Kicked, name: Some(name), .. }] if name == "Gece Kartalları")
        );
        assert!(crew_view(&state).is_none());

        // Leaving on another device says nothing.
        state.team_answer(TeamAnswer {
            team: Some(team(&[("100000000000000000", "Ben", TeamRole::Founder)])),
            invited: vec![],
        });
        let left = state.apply(
            &PresenceEvent::TeamChanged {
                team: None,
                crew: None,
                reason: Some(TeamExit::Disbanded),
                by: Some("100000000000000000".into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(left.is_empty());
        assert!(state.teams_supported());
    }

    #[test]
    fn categories_follow_other_devices_and_drop_old_friends() {
        let mut state = loaded();
        let mut groups = FriendGroups {
            version: 2,
            ..FriendGroups::default()
        };
        groups.groups.push(crate::model::FriendGroup {
            id: "okul".into(),
            name: "Okul".into(),
        });
        groups.members.insert("1".into(), "okul".into());
        groups.members.insert("777".into(), "okul".into());
        groups.members.insert("2".into(), "gone".into());
        state.apply(&PresenceEvent::GroupsChanged(groups.clone()), NOW);
        let view = state.groups_view().unwrap();
        assert_eq!(view.version, 2);
        // Only friends, and only into categories that exist.
        assert_eq!(
            view.members,
            vec![GroupMember {
                friend: "1".into(),
                group: "okul".into()
            }]
        );
        // An older document (a save that crossed this one) does not win.
        let older = FriendGroups {
            version: 1,
            ..FriendGroups::default()
        };
        state.apply(&PresenceEvent::GroupsChanged(older), NOW);
        assert_eq!(state.groups_view().unwrap().version, 2);
    }

    const CREW_A: &str = "0123456789abcdef";
    const CREW_B: &str = "fedcba9876543210";

    fn crew(id: &str, name: &str, members: &[(&str, &str, TeamRole)]) -> CrewEntry {
        let mut team = team(members);
        team.id = id.into();
        team.name = name.into();
        CrewEntry {
            team,
            invited: vec![],
            notifications: CrewMute::default(),
            requests: vec![],
        }
    }

    /// A service with several crews: the player leads "Gece Kartalları" (A: Ayşe, Zeki) and is a
    /// member of "PvP" (B: Bora, who leads it, and Zeki).
    fn multi() -> SocialState {
        let mut state = loaded();
        let mut overview = state.overview.take().unwrap();
        overview.team_features = vec![
            "look".into(),
            "calls".into(),
            "roles".into(),
            "mute".into(),
            TEAM_FEATURE_MULTI.into(),
            "callServer".into(),
        ];
        overview.crews = Some(vec![
            crew(
                CREW_A,
                "Gece Kartalları",
                &[
                    (ME, "Ben", TeamRole::Founder),
                    ("1", "Ayşe", TeamRole::Member),
                    ("9", "Zeki", TeamRole::Member),
                ],
            ),
            crew(
                CREW_B,
                "PvP",
                &[
                    ("2", "Bora", TeamRole::Founder),
                    (ME, "Ben", TeamRole::Member),
                    ("9", "Zeki", TeamRole::Member),
                ],
            ),
        ]);
        overview.crew_limit = Some(5);
        // What an older launcher would read stays aside.
        overview.team = Some(overview.crews.as_ref().unwrap()[0].team.clone());
        state.set_overview(overview);
        state
    }

    fn crew_ids(state: &SocialState) -> Vec<String> {
        state.crew_views(NOW).into_iter().map(|c| c.id).collect()
    }

    #[test]
    fn several_crews_keep_their_order_mutes_and_limit() {
        let mut state = multi();
        assert_eq!(crew_ids(&state), vec![CREW_A, CREW_B]);
        assert_eq!(state.crew_limit(), 5);
        // Reordered on another device: the first is the primary crew.
        state.apply(
            &PresenceEvent::CrewOrder(vec![CREW_B.into(), CREW_A.into()]),
            NOW,
        );
        assert_eq!(crew_ids(&state), vec![CREW_B, CREW_A]);
        // An order that names one crew only puts it first; the rest keep theirs.
        state.set_crew_order(&[CREW_A.into()]);
        assert_eq!(crew_ids(&state), vec![CREW_A, CREW_B]);
        // One crew muted for an hour on another device; it ends by itself.
        state.apply(
            &PresenceEvent::CrewMuted {
                crew: CREW_B.into(),
                notifications: CrewMute {
                    muted: true,
                    mute_until: Some(NOW + 3_600),
                },
            },
            NOW,
        );
        let views = state.crew_views(NOW);
        assert!(views[1].notifications.muted && !views[0].notifications.muted);
        assert!(!state.crew_views(NOW + 3_600)[1].notifications.muted);
        // Without `multi` a service has one crew: the limit is one, whatever it says.
        let single = crewed();
        assert_eq!(single.crew_limit(), 1);
        assert_eq!(crew_ids(&single), vec!["0123456789abcdef"]);
    }

    #[test]
    fn calls_name_their_crews_and_stay_quiet_where_all_are_muted() {
        let mut state = multi();
        state.set_crew_mute(
            CREW_B,
            CrewMute {
                muted: true,
                mute_until: None,
            },
        );
        let mut bora = crew_call("c1", "2", CallKind::Join, NOW, NOW + 1200);
        bora.crews = vec![CREW_B.into()];
        bora.server = Some(Game {
            location: GameLocation::Other,
            host: Some("mc.example.com".into()),
            ver: None,
        });
        // Only PvP, which is muted: shown, not announced.
        let quiet = state.apply(
            &PresenceEvent::Call {
                call: bora,
                quiet: false,
                clock: Some(NOW),
            },
            NOW,
        );
        assert!(quiet.is_empty());
        let view = state.calls(NOW).pop().unwrap();
        assert_eq!(view.crews, vec![CREW_B.to_owned()]);
        assert_eq!(view.name.as_deref(), Some("Bora"));
        assert_eq!(
            view.server,
            Some(Game {
                location: GameLocation::Other,
                host: Some("mc.example.com".into()),
                ver: None,
            })
        );
        // Zeki calls both crews: PvP is muted, Gece Kartalları is not.
        let mut zeki = crew_call("c2", "9", CallKind::Join, NOW, NOW + 1200);
        zeki.crews = vec![CREW_B.into(), CREW_A.into()];
        zeki.server = Some(Game {
            location: GameLocation::Projects,
            host: Some("play.projects.gg".into()),
            ver: None,
        });
        zeki.answers.insert("1".into(), CallAnswer::Yes);
        zeki.answers.insert("2".into(), CallAnswer::Soon);
        let live = state.apply(
            &PresenceEvent::Call {
                call: zeki,
                quiet: false,
                clock: Some(NOW),
            },
            NOW,
        );
        let [SocialNotice::CrewCall { call }] = &live[..] else {
            panic!("unexpected {live:?}");
        };
        // In the player's order of crews, and every answer of someone in them, each once.
        assert_eq!(call.crews, vec![CREW_A.to_owned(), CREW_B.to_owned()]);
        assert_eq!(
            call.answers
                .iter()
                .map(|a| (a.id.as_str(), a.answer))
                .collect::<Vec<_>>(),
            vec![("1", CallAnswer::Yes), ("2", CallAnswer::Soon)]
        );
        // Removed from PvP: Bora's call (only PvP) goes, Zeki's stays through the other crew.
        let gone = state.apply(
            &PresenceEvent::TeamChanged {
                team: None,
                crew: Some(CREW_B.into()),
                reason: Some(TeamExit::Kicked),
                by: Some("2".into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(
            matches!(&gone[..], [SocialNotice::TeamGone { name: Some(name), .. }] if name == "PvP")
        );
        assert_eq!(crew_ids(&state), vec![CREW_A]);
        assert_eq!(
            state
                .calls(NOW)
                .iter()
                .map(|c| (c.id.as_str(), c.crews.clone()))
                .collect::<Vec<_>>(),
            vec![("c2", vec![CREW_A.to_owned()])]
        );
        // A crew this player never had says nothing when it goes.
        let unknown = state.apply(
            &PresenceEvent::TeamChanged {
                team: None,
                crew: Some("aaaaaaaaaaaaaaaa".into()),
                reason: Some(TeamExit::Disbanded),
                by: Some("2".into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(unknown.is_empty());
    }

    #[test]
    fn a_call_follows_its_caller_and_the_coming_hear_where() {
        let mut state = multi();
        let mut call = crew_call("c1", "1", CallKind::Join, NOW, NOW + 1200);
        call.crews = vec![CREW_A.into()];
        call.server = Some(Game {
            location: GameLocation::Projects,
            host: Some("play.projects.gg".into()),
            ver: None,
        });
        state.apply(
            &PresenceEvent::Call {
                call: call.clone(),
                quiet: false,
                clock: Some(NOW),
            },
            NOW,
        );
        state.call_answered("c1", CallAnswer::Soon);
        // Ayşe moves to another server she shares: the service tells those coming, not quietly.
        call.server = Some(Game {
            location: GameLocation::Other,
            host: Some("mc.example.com".into()),
            ver: None,
        });
        call.answers.insert(ME.into(), CallAnswer::Soon);
        let moved = state.apply(
            &PresenceEvent::Call {
                call: call.clone(),
                quiet: false,
                clock: Some(NOW + 30),
            },
            NOW + 30,
        );
        assert!(matches!(
            &moved[..],
            [SocialNotice::CrewCallJoin { call }]
                if call.server.as_ref().and_then(|s| s.host.as_deref()) == Some("mc.example.com")
        ));
        // The same server again is not news; neither is a quiet copy.
        assert!(
            state
                .apply(
                    &PresenceEvent::Call {
                        call,
                        quiet: false,
                        clock: Some(NOW + 40)
                    },
                    NOW + 40
                )
                .is_empty()
        );
        // A call from a service before CALL1 shows as it was: "Hazır mısın?", nothing to join.
        let mut ready = crew_call("c2", "9", CallKind::Ready, NOW, NOW + 1200);
        ready.crews = vec![CREW_A.into()];
        state.apply(
            &PresenceEvent::Call {
                call: ready,
                quiet: true,
                clock: Some(NOW),
            },
            NOW,
        );
        let view = state.calls(NOW).into_iter().find(|c| c.id == "c2").unwrap();
        assert_eq!((view.kind, view.server), (CallKind::Ready, None));
        // ... and a "join" one from then is on Projects.
        let mut join = crew_call("c3", "9", CallKind::Join, NOW, NOW + 1200);
        join.crews = vec![CREW_A.into()];
        state.apply(
            &PresenceEvent::Call {
                call: join,
                quiet: true,
                clock: Some(NOW),
            },
            NOW,
        );
        let view = state.calls(NOW).into_iter().find(|c| c.id == "c3").unwrap();
        assert_eq!(
            view.server.map(|s| s.location),
            Some(GameLocation::Projects)
        );
    }

    #[test]
    fn crews_come_and_go_and_crewmates_are_heard_through_an_unmuted_one() {
        let mut state = multi();
        // Joined on another device: the new crew goes last.
        let mut third = team(&[
            ("3", "Can", TeamRole::Founder),
            (ME, "Ben", TeamRole::Member),
        ]);
        third.id = "1111222233334444".into();
        state.apply(
            &PresenceEvent::TeamChanged {
                team: Some(third),
                crew: Some("1111222233334444".into()),
                reason: None,
                by: None,
                joined: Vec::new(),
            },
            NOW,
        );
        assert_eq!(crew_ids(&state), vec![CREW_A, CREW_B, "1111222233334444"]);
        // Founding one here: last too; changing one: in place, with its invites.
        let mut founded = team(&[(ME, "Ben", TeamRole::Founder)]);
        founded.id = "5555666677778888".into();
        state.team_answer(TeamAnswer {
            team: Some(founded),
            invited: vec![],
        });
        assert_eq!(crew_ids(&state).last().unwrap(), "5555666677778888");
        let mut renamed = state.crew(CREW_B).unwrap().team.clone();
        renamed.name = "PvP Ustaları".into();
        state.team_answer(TeamAnswer {
            team: Some(renamed),
            invited: vec!["4".into()],
        });
        let views = state.crew_views(NOW);
        assert_eq!(views[1].name, "PvP Ustaları");
        assert_eq!(views[1].invited, vec!["4".to_owned()]);
        state.team_invite_sent(CREW_A, "5");
        state.team_code(CREW_A, "TKM-WXYZ-1234".into());
        let views = state.crew_views(NOW);
        assert_eq!(views[0].invited, vec!["5".to_owned()]);
        assert_eq!(views[0].code, "EKP-WXYZ-1234");
        state.team_left("5555666677778888", false);
        assert_eq!(crew_ids(&state).len(), 3);

        // Zeki is in both crews; Ayşe only in Gece Kartalları, which the player mutes.
        state.set_crew_mute(
            CREW_A,
            CrewMute {
                muted: true,
                mute_until: None,
            },
        );
        let projects = |id: &str| {
            PresenceEvent::Presence(presence(
                id,
                PresenceState::Playing,
                Some(GameLocation::Projects),
            ))
        };
        let ayse = state.apply(&projects("1"), NOW);
        assert!(
            !ayse
                .iter()
                .any(|n| matches!(n, SocialNotice::CrewmateOnProjects { .. })),
            "Ayşe shares only a muted crew"
        );
        assert_eq!(
            state.apply(&projects("9"), NOW),
            vec![SocialNotice::CrewmateOnProjects { id: "9".into() }]
        );
    }

    #[test]
    fn a_new_picture_reaches_the_list() {
        let mut state = loaded();
        let mut changed = presence("1", PresenceState::Launcher, None);
        changed.avatar = Some("preset:fox".into());
        state.apply(&PresenceEvent::Presence(changed), NOW);
        let ayse = by_id(&state.friends(), "1").clone();
        assert_eq!(ayse.avatar, "preset:fox");
        assert_eq!(ayse.avatar_url, None);
        assert_eq!(ayse.discord_url, "https://discord.com/users/1");
    }

    fn join_request(id: &str, name: &str, sent_at: i64) -> JoinRequest {
        JoinRequest {
            from: profile(id, name),
            sent_at,
            via: None,
        }
    }

    #[test]
    fn join_requests_reach_those_who_answer_and_go_with_the_right() {
        let mut state = multi();
        // The leader of Gece Kartalları (A) hears a new request; an old one is not shown.
        let notices = state.apply(
            &PresenceEvent::TeamRequests {
                crew: CREW_A.into(),
                requests: vec![
                    join_request("7", "Eski", NOW - JOIN_REQUEST_LIFETIME_SECS),
                    join_request("8", "Ece", NOW - 5),
                ],
                added: Some("8".into()),
            },
            NOW,
        );
        assert!(matches!(
            notices.as_slice(),
            [SocialNotice::JoinRequest { crew, request }] if crew == CREW_A && request.name == "Ece"
        ));
        let crews = state.crew_views(NOW);
        assert_eq!(
            crews[0]
                .requests
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Ece"]
        );
        // A muted crew says nothing, the list still changes.
        state.set_crew_mute(
            CREW_A,
            CrewMute {
                muted: true,
                mute_until: None,
            },
        );
        let quiet = state.apply(
            &PresenceEvent::TeamRequests {
                crew: CREW_A.into(),
                requests: vec![
                    join_request("8", "Ece", NOW - 5),
                    join_request("6", "Fe", NOW),
                ],
                added: Some("6".into()),
            },
            NOW,
        );
        assert!(quiet.is_empty());
        assert_eq!(state.crew_views(NOW)[0].requests.len(), 2);
        // Answered here: the row goes; blocking someone takes their request too.
        state.join_request_answered(CREW_A, "6");
        state.blocked(profile("8", "Ece"));
        assert!(state.crew_views(NOW)[0].requests.is_empty());

        // Losing the right (`admit` for the leader alone, the player only a member) drops them.
        state.apply(
            &PresenceEvent::TeamRequests {
                crew: CREW_B.into(),
                requests: vec![join_request("5", "Gül", NOW)],
                added: None,
            },
            NOW,
        );
        assert_eq!(state.crew_views(NOW)[1].requests.len(), 1);
        let mut pvp = state.crew(CREW_B).unwrap().team.clone();
        pvp.permissions.admit = crate::model::PermissionLevel::Leader;
        state.apply(
            &PresenceEvent::TeamChanged {
                team: Some(pvp),
                crew: Some(CREW_B.into()),
                reason: None,
                by: Some("2".into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(state.crew_views(NOW)[1].requests.is_empty());
    }

    #[test]
    fn a_request_of_the_players_waits_and_its_crew_comes_when_taken() {
        let mut state = multi();
        let summary = crate::model::TeamSummary {
            id: "5555666677778888".into(),
            name: "Skyblock".into(),
            tag: "SKY".into(),
            emblem: "castle".into(),
            color: "purple".into(),
            frame: "hex".into(),
            motto: String::new(),
            members: 4,
            join: crate::model::JoinMode::Approval,
        };
        state.join_requested(TeamRequest {
            team: summary.clone(),
            sent_at: NOW - 60,
        });
        assert_eq!(state.team_requests(NOW)[0].name, "Skyblock");
        // A week later it is gone without a word.
        assert!(
            state
                .team_requests(NOW + JOIN_REQUEST_LIFETIME_SECS)
                .is_empty()
        );
        // Withdrawn here.
        state.join_request_withdrawn("5555666677778888");
        assert!(state.team_requests(NOW).is_empty());
        // Sent again, then a leader takes it: the crew arrives with a word for the player.
        state.join_requested(TeamRequest {
            team: summary,
            sent_at: NOW,
        });
        let mut sky = team(&[
            ("801", "Deniz", TeamRole::Founder),
            (ME, "Ben", TeamRole::Member),
        ]);
        sky.id = "5555666677778888".into();
        sky.name = "Skyblock".into();
        let notices = state.apply(
            &PresenceEvent::TeamChanged {
                team: Some(sky.clone()),
                crew: Some(sky.id.clone()),
                reason: None,
                by: Some("801".into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(matches!(
            notices.as_slice(),
            [SocialNotice::RequestAccepted { name, .. }] if name == "Skyblock"
        ));
        assert!(state.team_requests(NOW).is_empty());
        assert_eq!(state.crew_views(NOW).len(), 3);
        // The same crew changing later is no news.
        let again = state.apply(
            &PresenceEvent::TeamChanged {
                team: Some(sky),
                crew: Some("5555666677778888".into()),
                reason: None,
                by: Some("801".into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(again.is_empty());
    }

    fn plan(id: &str, crew: &str, by: &str, at: i64) -> CrewPlan {
        CrewPlan {
            id: id.into(),
            crew: crew.into(),
            by: by.into(),
            at,
            until: at + 7_200,
            created_at: at - 3_600,
            answers: [(by.to_owned(), PlanAnswer::Yes)].into_iter().collect(),
            note: None,
        }
    }

    fn hello_with_plans(plans: Vec<CrewPlan>, clock: i64) -> PresenceEvent {
        PresenceEvent::Connected {
            status: StatusMode::Online,
            friends: vec![],
            invites: vec![],
            calls: vec![],
            clock: Some(clock),
            chat: None,
            plans: Some(plans),
        }
    }

    #[test]
    fn plans_keep_the_services_time_and_end_on_this_clock() {
        let mut state = multi();
        // This computer is 30 s ahead of the service.
        state.apply(
            &hello_with_plans(
                vec![
                    plan("p2", CREW_A, "1", NOW + 7_200),
                    plan("p1", CREW_B, "2", NOW + 900),
                    plan("over", CREW_A, "1", NOW - 7_300),
                    plan("elsewhere", "9999aaaabbbbcccc", "3", NOW + 900),
                ],
                NOW - 30,
            ),
            NOW,
        );
        let plans = state.plans(NOW);
        // Soonest first; one that is over or of a crew the player is not in is not there.
        assert_eq!(
            plans.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            vec!["p1", "p2"]
        );
        let p1 = &plans[0];
        assert_eq!(p1.at, NOW + 900);
        assert_eq!(p1.starts_at, NOW + 930);
        assert_eq!(p1.ends_at, NOW + 900 + 7_200 + 30);
        assert_eq!(p1.by_name.as_deref(), Some("Bora"));
        assert!(!p1.mine && !p1.can_cancel);
        assert_eq!(p1.answers.len(), 1);
        // The player leads A: they may cancel Ayşe's plan there.
        assert!(plans[1].can_cancel);
        // Answered here; the hello's list replaces everything.
        state.plan_answered("p1", PlanAnswer::Maybe);
        assert_eq!(state.plans(NOW)[0].my_answer, Some(PlanAnswer::Maybe));
        assert_eq!(state.plans(NOW)[0].answers.len(), 2);
        // Over two hours after its time, by this computer's clock with the difference.
        assert_eq!(state.plans(NOW + 900 + 7_200 + 29).len(), 2);
        assert_eq!(state.plans(NOW + 900 + 7_200 + 30).len(), 1);
    }

    #[test]
    fn new_plans_are_said_once_and_follow_the_crews() {
        let mut state = multi();
        state.apply(&hello_with_plans(vec![], NOW), NOW);
        let new = state.apply(
            &PresenceEvent::Plan {
                plan: plan("p1", CREW_B, "2", NOW + 3_600),
                quiet: false,
            },
            NOW,
        );
        assert!(matches!(
            new.as_slice(),
            [SocialNotice::CrewPlan { plan }] if plan.id == "p1" && plan.by_name.as_deref() == Some("Bora")
        ));
        // The same plan again (an answer) is no news; a quiet one or the player's own neither.
        let mut answered = plan("p1", CREW_B, "2", NOW + 3_600);
        answered.answers.insert("9".into(), PlanAnswer::No);
        assert!(
            state
                .apply(
                    &PresenceEvent::Plan {
                        plan: answered,
                        quiet: false
                    },
                    NOW
                )
                .is_empty()
        );
        assert_eq!(state.plans(NOW)[0].answers.len(), 2);
        assert!(
            state
                .apply(
                    &PresenceEvent::Plan {
                        plan: plan("p2", CREW_B, "2", NOW + 7_200),
                        quiet: true
                    },
                    NOW
                )
                .is_empty()
        );
        assert!(
            state
                .apply(
                    &PresenceEvent::Plan {
                        plan: plan("p3", CREW_A, ME, NOW + 7_200),
                        quiet: false
                    },
                    NOW
                )
                .is_empty()
        );
        // A muted crew's plan is shown, not said.
        state.set_crew_mute(
            CREW_A,
            CrewMute {
                muted: true,
                mute_until: None,
            },
        );
        assert!(
            state
                .apply(
                    &PresenceEvent::Plan {
                        plan: plan("p4", CREW_A, "1", NOW + 10_800),
                        quiet: false
                    },
                    NOW
                )
                .is_empty()
        );
        assert_eq!(state.plans(NOW).len(), 4);
        // A crew's whole list (also empty), a cancel, and leaving a crew.
        state.apply(
            &PresenceEvent::Plans {
                crew: CREW_B.into(),
                plans: vec![],
            },
            NOW,
        );
        assert_eq!(state.plans(NOW).len(), 2);
        state.apply(
            &PresenceEvent::PlanRemoved {
                plan: "p3".into(),
                crew: CREW_A.into(),
                by: ME.into(),
            },
            NOW,
        );
        assert_eq!(state.plans(NOW).len(), 1);
        state.team_left(CREW_A, false);
        assert!(state.plans(NOW).is_empty());
    }

    #[test]
    fn together_means_on_projects_at_the_same_time() {
        let mut state = multi();
        let projects = |id: &str| {
            PresenceEvent::Presence(presence(
                id,
                PresenceState::Playing,
                Some(GameLocation::Projects),
            ))
        };
        state.apply(&projects("9"), NOW);
        state.apply(&projects("1"), NOW);
        state.apply(
            &PresenceEvent::Presence(presence("2", PresenceState::Launcher, None)),
            NOW,
        );
        let on_projects = LocalStatus::Playing {
            host: Some("play.projects.gg".into()),
        };
        let together = state.together_now(&on_projects);
        // Zeki (both crews) and Ayşe (A); Bora is only in the launcher.
        assert_eq!(
            together.people.iter().cloned().collect::<Vec<_>>(),
            vec!["1".to_owned(), "9".to_owned()]
        );
        assert_eq!(together.crews.len(), 2);
        // Elsewhere the player is with nobody.
        assert!(state.together_now(&LocalStatus::Launcher).is_empty());
        let other = LocalStatus::Playing {
            host: Some("mc.example.com".into()),
        };
        assert!(state.together_now(&other).is_empty());
        // Someone the player blocked does not count.
        state.blocked(profile("9", "Zeki"));
        let together = state.together_now(&on_projects);
        assert_eq!(together.people.len(), 1);
        assert_eq!(together.crews.len(), 1);
    }

    // --- crew tags and "Hoş geldin" (TEAM6) ------------------------------------------------------

    /// The card of a crew's tag, as the service makes it.
    fn card(crew: &str, name: &str, tag: &str) -> CrewBadge {
        CrewBadge {
            crew: crew.into(),
            tag: tag.into(),
            color: "navy".into(),
            frame: "shield".into(),
            emblem: "bolt".into(),
            name: name.into(),
            motto: "Gece bizimdir".into(),
            created_on: "2026-09-20".into(),
            members: 3,
            requests: true,
        }
    }

    /// `multi()` on a service with crew tags: the player and Ayşe (a friend, in A) wear A's tag
    /// ("GK"), Bora (a friend, B's leader) wears B's ("PVP"), Can (a friend in no crew) none.
    fn tagged() -> SocialState {
        let mut state = multi();
        if let Some(overview) = &mut state.overview {
            overview.team_features.extend([
                TEAM_FEATURE_BADGES.into(),
                TEAM_FEATURE_BADGE_REQUESTS.into(),
            ]);
        }
        let a = card(CREW_A, "Gece Kartalları", "GK");
        state.set_badge_of(ME, Some(&a));
        state.set_badge_of("1", Some(&a));
        state.set_badge_of("2", Some(&card(CREW_B, "PvP", "PVP")));
        state
    }

    /// Crew A as the player holds it now, to change and send back as its message.
    fn crew_a(state: &SocialState) -> Team {
        state.crew(CREW_A).unwrap().team.clone()
    }

    fn crew_message(team: Team, by: &str, joined: &[&str]) -> PresenceEvent {
        PresenceEvent::TeamChanged {
            crew: Some(team.id.clone()),
            team: Some(team),
            reason: None,
            by: Some(by.into()),
            joined: joined.iter().map(|id| (*id).to_owned()).collect(),
        }
    }

    fn badge_of<'a>(friends: &'a [FriendView], id: &str) -> Option<&'a str> {
        by_id(friends, id).badge.as_ref().map(|b| b.tag.as_str())
    }

    fn member_badge(state: &SocialState, crew: usize, id: &str) -> Option<String> {
        state.crew_views(NOW)[crew]
            .members
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.badge.as_ref())
            .map(|b| b.name.clone())
    }

    #[test]
    fn crew_tags_follow_their_wearers_into_every_list() {
        let mut state = tagged();
        let friends = state.friends();
        assert_eq!(badge_of(&friends, "1"), Some("GK"));
        assert_eq!(badge_of(&friends, "2"), Some("PVP"));
        assert_eq!(badge_of(&friends, "3"), None);
        assert_eq!(
            member_badge(&state, 0, "1").as_deref(),
            Some("Gece Kartalları")
        );
        // In B's list the player and Bora wear their tags (the player A's, of another crew).
        assert_eq!(
            member_badge(&state, 1, ME).as_deref(),
            Some("Gece Kartalları")
        );
        let views = state.crew_views(NOW);
        assert_eq!((views[0].wearers, views[1].wearers), (2, 1));
        assert!(!views[0].hidden && views[0].badge_requests);
        let mine = state.my_badge().unwrap();
        assert_eq!((mine.tag.as_str(), mine.requests), ("GK", true));

        // Live: Ayşe takes hers off, then puts on another; Zeki (in both crews) puts B's on.
        state.apply(
            &PresenceEvent::BadgeChanged {
                id: "1".into(),
                badge: None,
            },
            NOW,
        );
        assert_eq!(badge_of(&state.friends(), "1"), None);
        assert_eq!(member_badge(&state, 0, "1"), None);
        state.apply(
            &PresenceEvent::BadgeChanged {
                id: "9".into(),
                badge: Some(card(CREW_B, "PvP", "PVP")),
            },
            NOW,
        );
        assert_eq!(member_badge(&state, 0, "9").as_deref(), Some("PvP"));
        assert_eq!(member_badge(&state, 1, "9").as_deref(), Some("PvP"));
        // Someone this launcher does not show changes nothing.
        state.apply(
            &PresenceEvent::BadgeChanged {
                id: "77".into(),
                badge: Some(card(CREW_B, "PvP", "PVP")),
            },
            NOW,
        );
        assert_eq!(state.friends().len(), 3);

        // Studio's request switch off: the cards stay, none takes requests.
        if let Some(overview) = &mut state.overview {
            overview
                .team_features
                .retain(|f| f != TEAM_FEATURE_BADGE_REQUESTS);
        }
        assert_eq!(state.my_badge().map(|b| b.requests), Some(false));
        // Crew tags off (or an older service): no card anywhere, no hidden crew.
        if let Some(overview) = &mut state.overview {
            overview.team_features.retain(|f| f != TEAM_FEATURE_BADGES);
            if let Some(crews) = &mut overview.crews {
                crews[0].team.hidden = true;
            }
        }
        assert!(state.friends().iter().all(|f| f.badge.is_none()));
        assert!(state.my_badge().is_none());
        let views = state.crew_views(NOW);
        assert!(views[0].members.iter().all(|m| m.badge.is_none()));
        assert!(!views[0].hidden && views[0].wearers == 0);
    }

    #[test]
    fn a_crews_message_carries_its_members_tags_as_they_are_now() {
        let mut state = tagged();
        // A is renamed: the card of everyone wearing it follows, in every list.
        let renamed = card(CREW_A, "Gece Şahinleri", "GK");
        let mut a = crew_a(&state);
        a.name = "Gece Şahinleri".into();
        for member in &mut a.members {
            if member.profile.id == ME || member.profile.id == "1" {
                member.profile.badge = Some(renamed.clone());
            }
        }
        state.apply(&crew_message(a.clone(), "1", &[]), NOW);
        assert_eq!(
            by_id(&state.friends(), "1")
                .badge
                .as_ref()
                .map(|b| b.name.as_str()),
            Some("Gece Şahinleri")
        );
        assert_eq!(
            state.my_badge().map(|b| b.name),
            Some("Gece Şahinleri".into())
        );
        assert_eq!(
            member_badge(&state, 1, ME).as_deref(),
            Some("Gece Şahinleri")
        );
        // Bora is not in A: B's card stays his.
        assert_eq!(badge_of(&state.friends(), "2"), Some("PVP"));
        // Only members wear a crew's tag: a friend still shown with A's card who is not in it
        // loses it with A's next message.
        state.set_badge_of("3", Some(&renamed));
        state.apply(&crew_message(a.clone(), "1", &[]), NOW);
        assert_eq!(badge_of(&state.friends(), "3"), None);
        // An answer of the player's own change carries them too (Ayşe took hers off meanwhile).
        for member in &mut a.members {
            if member.profile.id == "1" {
                member.profile.badge = None;
            }
        }
        state.team_answer(TeamAnswer {
            team: Some(a),
            invited: vec![],
        });
        assert_eq!(badge_of(&state.friends(), "1"), None);
    }

    #[test]
    fn hiding_the_crew_whose_tag_the_player_wore_says_so_once_in_either_order() {
        let hidden = |state: &SocialState| {
            let mut a = crew_a(state);
            a.hidden = true;
            for member in &mut a.members {
                member.profile.badge = None;
            }
            a
        };
        // The crew's message first: the tag comes off with it.
        let mut state = tagged();
        let a = hidden(&state);
        state.apply(&crew_message(a.clone(), "1", &[]), NOW);
        assert!(state.my_badge().is_none());
        assert_eq!(badge_of(&state.friends(), "1"), None);
        assert!(state.crew_views(NOW)[0].hidden);
        assert_eq!(
            state.take_tag_off(),
            Some(TagOffView {
                crew: CREW_A.into(),
                name: "Gece Kartalları".into()
            })
        );
        assert_eq!(state.take_tag_off(), None);
        // The message again (or the player's own `badge` message after it) says nothing more.
        state.apply(&crew_message(a, "1", &[]), NOW);
        state.apply(
            &PresenceEvent::BadgeChanged {
                id: ME.into(),
                badge: None,
            },
            NOW,
        );
        assert_eq!(state.take_tag_off(), None);

        // The player's own `badge` message first: the crew's message right after still says it.
        let mut state = tagged();
        state.apply(
            &PresenceEvent::BadgeChanged {
                id: ME.into(),
                badge: None,
            },
            NOW,
        );
        let a = hidden(&state);
        state.apply(&crew_message(a, "1", &[]), NOW + 2);
        assert!(state.take_tag_off().is_some());

        // Long after the tag came off, hiding the crew is not about it.
        let mut state = tagged();
        state.apply(
            &PresenceEvent::BadgeChanged {
                id: ME.into(),
                badge: None,
            },
            NOW,
        );
        let a = hidden(&state);
        state.apply(&crew_message(a, "1", &[]), NOW + TAG_DROP_SECS + 1);
        assert!(state.take_tag_off().is_none());

        // The player hid it themself (here or on another device): nothing to say.
        let mut state = tagged();
        let a = hidden(&state);
        state.apply(&crew_message(a, ME, &[]), NOW);
        assert!(state.take_tag_off().is_none());
        assert!(state.my_badge().is_none());

        // Another crew's tag worn: hiding A takes nothing of the player's.
        let mut state = tagged();
        state.set_badge_of(ME, Some(&card(CREW_B, "PvP", "PVP")));
        let mut a = hidden(&state);
        for member in &mut a.members {
            if member.profile.id == ME {
                member.profile.badge = Some(card(CREW_B, "PvP", "PVP"));
            }
        }
        state.apply(&crew_message(a, "1", &[]), NOW);
        assert!(state.take_tag_off().is_none());
        assert_eq!(state.my_badge().map(|b| b.tag), Some("PVP".into()));
    }

    #[test]
    fn leaving_or_ending_a_crew_takes_its_tags() {
        // The player leaves A: their own A tag goes, Ayşe keeps hers (A goes on without them).
        let mut state = tagged();
        state.apply(
            &PresenceEvent::TeamChanged {
                team: None,
                crew: Some(CREW_A.into()),
                reason: Some(TeamExit::Left),
                by: Some(ME.into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert!(state.my_badge().is_none());
        assert_eq!(member_badge(&state, 0, ME), None);
        assert_eq!(badge_of(&state.friends(), "1"), Some("GK"));
        // B ends: every card of it goes.
        state.apply(
            &PresenceEvent::TeamChanged {
                team: None,
                crew: Some(CREW_B.into()),
                reason: Some(TeamExit::Disbanded),
                by: Some("2".into()),
                joined: Vec::new(),
            },
            NOW,
        );
        assert_eq!(badge_of(&state.friends(), "2"), None);
        // The same through the player's own calls.
        let mut state = tagged();
        state.team_left(CREW_B, true);
        assert_eq!(badge_of(&state.friends(), "2"), None);
        state.team_left(CREW_A, false);
        assert!(state.my_badge().is_none());
        assert_eq!(badge_of(&state.friends(), "1"), Some("GK"));
    }

    #[test]
    fn the_players_own_tag_shows_on_their_entries_once_the_service_answers() {
        let mut state = tagged();
        let mut wearing = me();
        wearing.profile.badge = Some(card(CREW_B, "PvP", "PVP"));
        state.set_me(wearing);
        assert_eq!(state.my_badge().map(|b| b.tag), Some("PVP".into()));
        assert_eq!(member_badge(&state, 0, ME).as_deref(), Some("PvP"));
        assert_eq!(member_badge(&state, 1, ME).as_deref(), Some("PvP"));
        assert_eq!(
            (
                state.crew_views(NOW)[0].wearers,
                state.crew_views(NOW)[1].wearers
            ),
            (1, 2)
        );
        state.set_me(me());
        assert_eq!(member_badge(&state, 0, ME), None);
    }

    #[test]
    fn who_came_in_shows_in_the_crews_chat_with_selam_ver_for_ten_minutes() {
        let mut state = tagged();
        let mut a = crew_a(&state);
        a.members.push(TeamMember {
            profile: profile("5", "Ece"),
            role: TeamRole::Member,
            moderator: false,
            joined_at: NOW,
        });
        // Someone named who is not in the list (a race) gets no line.
        state.apply(&crew_message(a.clone(), "1", &["5", "88"]), NOW);
        let joins = state.crew_joins();
        assert_eq!(
            joins,
            [CrewJoinView {
                crew: CREW_A.into(),
                id: "5".into(),
                name: "Ece".into(),
                mine: false,
                at: NOW,
                greet_until: Some(NOW + GREET_WINDOW_SECS),
            }]
        );
        assert!(state.may_greet(CREW_A, "5", NOW + GREET_WINDOW_SECS - 1));
        assert!(!state.may_greet(CREW_A, "5", NOW + GREET_WINDOW_SECS));
        assert!(!state.may_greet(CREW_B, "5", NOW));
        state.greeted(CREW_A, "5");
        assert_eq!(state.crew_joins()[0].greet_until, None);
        assert!(!state.may_greet(CREW_A, "5", NOW));

        // The player's own coming in: "Ekibe katıldın", nothing to greet.
        let b = state.crew(CREW_B).unwrap().team.clone();
        state.apply(&crew_message(b, "2", &[ME]), NOW + 5);
        let own = state.crew_joins().pop().unwrap();
        assert!(own.mine && own.greet_until.is_none());
        assert!(!state.may_greet(CREW_B, ME, NOW + 5));

        // At most 30 lines a crew: the oldest go.
        for n in 0..40 {
            state.apply(&crew_message(a.clone(), "1", &["5"]), NOW + 10 + n);
        }
        let in_a: Vec<_> = state
            .crew_joins()
            .into_iter()
            .filter(|join| join.crew == CREW_A)
            .collect();
        assert_eq!(in_a.len(), MAX_CREW_JOINS);
        assert_eq!(in_a[0].at, NOW + 20);
        // Leaving the crew takes its lines.
        state.team_left(CREW_A, false);
        assert!(state.crew_joins().iter().all(|join| join.crew != CREW_A));
        assert_eq!(state.crew_joins().len(), 1);
    }

    #[test]
    fn a_request_from_a_tag_names_the_member_it_came_through() {
        let mut state = tagged();
        let mut from_tag = join_request("5", "Ece", NOW - 60);
        from_tag.via = Some("1".into());
        let mut gone = join_request("6", "Fatih", NOW - 30);
        gone.via = Some("77".into());
        state.apply(
            &PresenceEvent::TeamRequests {
                crew: CREW_A.into(),
                requests: vec![from_tag, gone, join_request("7", "Gül", NOW - 10)],
                added: Some("5".into()),
            },
            NOW,
        );
        let requests = &state.crew_views(NOW)[0].requests;
        assert_eq!(
            requests
                .iter()
                .map(|r| (r.via.as_deref(), r.via_name.as_deref()))
                .collect::<Vec<_>>(),
            [(Some("1"), Some("Ayşe")), (None, None), (None, None)]
        );
        // Without crew tags the line is not shown.
        if let Some(overview) = &mut state.overview {
            overview.team_features.retain(|f| f != TEAM_FEATURE_BADGES);
        }
        assert!(state.crew_views(NOW)[0].requests[0].via.is_none());
    }
}
