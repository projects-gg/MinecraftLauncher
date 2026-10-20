//! The launcher's picture of the friends feature: the profile, friends with their live presence,
//! requests, blocks and waiting invites. A pure reducer over presence events and API results,
//! so every rule is testable without a service.

use std::collections::HashMap;

use serde::Serialize;

use crate::codes::crew_code;
use crate::model::{
    BlockedUser, Friend, FriendGroups, FriendRequest, FriendsOverview, Limits, Me, PublicProfile,
    RequestOutcome, SettingsMeta, TEAM_FEATURE_MUTE, Team, TeamAnswer, TeamExit, TeamInvite,
    TeamNotifications, TeamRole,
};
use crate::presence::{PresenceEvent, StopReason};
use crate::protocol::{
    CallAnswer, CallKind, CrewCall, Game, Invite, OnlineStatus, Presence, PresenceState,
};
use crate::view::{
    CallAnswerView, CrewCallView, FriendGroupsView, FriendView, GroupMember, InviteView,
    ProfileView, RequestView, SocialNotice, TeamInviteView, TeamMemberView, TeamView,
};

/// How long a received invite stays worth answering (the service delivers them for an hour).
pub const INVITE_LIFETIME_SECS: i64 = 3_600;
const MAX_INVITES: usize = 30;
/// Team invites shown at a time (the service lists at most this many too).
const MAX_TEAM_INVITES: usize = 20;
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

    pub fn set_me(&mut self, me: Me) {
        self.me = Some(me);
    }

    pub fn set_overview(&mut self, overview: FriendsOverview) {
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
            } => {
                self.connection = Connection::Online;
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
                    if let Some(seen) = presence.last_seen
                        && let Some(friend) = self.friend_mut(&presence.id)
                    {
                        friend.last_seen = Some(seen);
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
                    // has not muted the crew ("Sessize al").
                    if !was_on_projects
                        && on_projects
                        && self.is_teammate(&presence.id)
                        && !self.crew_muted(now)
                    {
                        notices.push(SocialNotice::CrewmateOnProjects {
                            id: presence.id.clone(),
                        });
                    }
                    self.online.insert(presence.id.clone(), presence.clone());
                }
            }
            PresenceEvent::TeamChanged { team, reason, by } => {
                let me = self.me.as_ref().map(|me| me.profile.id.clone());
                if let Some(overview) = &mut self.overview {
                    let before = overview.team.take();
                    match team {
                        Some(team) => {
                            overview
                                .team_invited
                                .retain(|id| !team.members.iter().any(|m| &m.profile.id == id));
                            // Invites go with the right to send them (the service withdraws them).
                            let may_invite = team
                                .members
                                .iter()
                                .find(|m| Some(&m.profile.id) == me.as_ref())
                                .is_some_and(|m| {
                                    team.permissions.invite.allows(m.role, m.moderator)
                                });
                            if !may_invite {
                                overview.team_invited.clear();
                            }
                            overview.team_invites.retain(|i| i.team.id != team.id);
                            overview.team = Some(team.clone());
                        }
                        None => {
                            overview.team_invited.clear();
                            let told =
                                matches!(reason, Some(TeamExit::Kicked | TeamExit::Disbanded))
                                    && by.is_some()
                                    && *by != me;
                            if told {
                                notices.push(SocialNotice::TeamGone {
                                    reason: reason.unwrap_or(TeamExit::Other),
                                    name: before.map(|team| team.name),
                                });
                            }
                        }
                    }
                }
                // Calls live in the crew: someone who left it takes their call along.
                match team {
                    Some(team) => self
                        .calls
                        .retain(|open| team.members.iter().any(|m| m.profile.id == open.call.from)),
                    None => self.calls.clear(),
                }
            }
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
                    // The service quiets calls for a player who muted the crew or switched calls
                    // off; a setting that changed a moment ago here counts too.
                    let announce =
                        !*quiet && me != Some(call.from.as_str()) && self.crew_calls_on(now);
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
                            if coming
                                && before.call.kind != CallKind::Join
                                && next.call.kind == CallKind::Join
                            {
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
                    StopReason::SignedOut | StopReason::AccountDeleted | StopReason::Shutdown => {
                        Connection::Idle
                    }
                };
            }
        }
        notices
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
    /// calls of the one blocked).
    pub fn blocked(&mut self, profile: PublicProfile) {
        self.forget(&profile.id);
        self.calls.retain(|open| open.call.from != profile.id);
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
    pub fn offers(&self, feature: &str) -> bool {
        self.team_features().iter().any(|f| f == feature)
    }

    pub fn team(&self) -> Option<&Team> {
        self.overview.as_ref()?.team.as_ref()
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

    /// Crew notifications are silent at `now` ("Sessize al"; a timed mute ends by itself).
    fn crew_muted(&self, now: i64) -> bool {
        self.team_notifications().is_some_and(|n| n.muted_at(now))
    }

    /// Crew calls may be announced at `now`: switched on and not muted.
    fn crew_calls_on(&self, now: i64) -> bool {
        self.team_notifications().is_none_or(|n| n.calls_at(now))
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
        let members = self.team().map_or(&[][..], |team| team.members.as_slice());
        let caller = members.iter().find(|m| m.profile.id == open.call.from);
        CrewCallView {
            id: open.call.id.clone(),
            from: open.call.from.clone(),
            mine: me == Some(open.call.from.as_str()),
            name: caller.map(|m| m.profile.name().to_owned()),
            avatar: caller.map(|m| m.profile.avatar.clone()),
            avatar_url: caller.and_then(|m| m.profile.discord_avatar_url(128)),
            kind: open.call.kind,
            started_at: open.started,
            ends_at: open.ends,
            // In the crew's order; answers of people no longer in it are not shown.
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

    /// In the player's crew (themself left out).
    fn is_teammate(&self, id: &str) -> bool {
        let me = self.me.as_ref().map(|me| me.profile.id.as_str());
        Some(id) != me
            && self
                .team()
                .is_some_and(|team| team.members.iter().any(|m| m.profile.id == id))
    }

    /// The answer of a call that changed the player's own team.
    pub fn team_answer(&mut self, answer: TeamAnswer) {
        if let Some(overview) = &mut self.overview {
            if let Some(team) = &answer.team {
                overview.team_invites.retain(|i| i.team.id != team.id);
            }
            overview.team = answer.team;
            overview.team_invited = answer.invited;
        }
    }

    /// The player left (or ended) their team.
    pub fn team_left(&mut self) {
        if let Some(overview) = &mut self.overview {
            overview.team = None;
            overview.team_invited.clear();
        }
    }

    pub fn team_code(&mut self, code: String) {
        if let Some(team) = self.overview.as_mut().and_then(|o| o.team.as_mut()) {
            team.code = code;
        }
    }

    /// The player invited a friend to their team.
    pub fn team_invite_sent(&mut self, friend: &str) {
        if let Some(overview) = &mut self.overview
            && !overview.team_invited.iter().any(|id| id == friend)
        {
            overview.team_invited.push(friend.to_owned());
        }
    }

    /// An invite to a team was answered (or turned out to be gone).
    pub fn team_invite_answered(&mut self, team_id: &str) {
        if let Some(overview) = &mut self.overview {
            overview.team_invites.retain(|i| i.team.id != team_id);
        }
    }

    /// The player's team with everyone's presence.
    pub fn team_view(&self) -> Option<TeamView> {
        let overview = self.overview.as_ref()?;
        let team = overview.team.as_ref()?;
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
                        }),
                    discord_url: member.profile.discord_profile_url(),
                    joined_at: member.joined_at,
                }
            })
            .collect();
        let mine = members.iter().find(|m| m.me);
        let role = mine.map_or(TeamRole::Member, |m| m.role);
        let moderator = mine.is_some_and(|m| m.moderator);
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
            invited: overview.team_invited.clone(),
            limit: overview.limits.team_members.unwrap_or(30),
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

    /// Friends with their presence, in the service's order.
    pub fn friends(&self) -> Vec<FriendView> {
        self.overview
            .as_ref()
            .map(|o| {
                o.friends
                    .iter()
                    .map(|friend| friend_view(friend, self.online.get(&friend.profile.id)))
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
        Some(friend_view(friend, self.online.get(id)))
    }

    /// A presence message carries the friend's current picture.
    fn absorb_picture(&mut self, presence: &Presence) {
        let Some(avatar) = presence.avatar.clone() else {
            return;
        };
        let hash = presence.avatar_hash.clone();
        if let Some(friend) = self.friend_mut(&presence.id) {
            friend.profile.avatar = avatar;
            friend.profile.avatar_hash = hash;
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
            at: invite.at,
            server: invite.server.clone(),
            quiet: invite.quiet,
        }
    }
}

fn friend_view(friend: &Friend, presence: Option<&Presence>) -> FriendView {
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

fn request_view(request: &FriendRequest) -> RequestView {
    let profile = ProfileView::from(&request.profile);
    RequestView {
        id: profile.id,
        name: profile.name,
        username: profile.username,
        avatar: profile.avatar,
        avatar_url: profile.avatar_url,
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
            },
            created_on: "2026-10-16".into(),
            settings: None,
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
            team_features: vec!["look".into(), "calls".into()],
            team_notifications: None,
        });
        state
    }

    fn presence(id: &str, state: PresenceState, game: Option<GameLocation>) -> Presence {
        Presence {
            id: id.into(),
            state,
            status: (state != PresenceState::Offline).then_some(OnlineStatus::Online),
            game: game.map(|location| Game {
                location,
                host: (location == GameLocation::Projects).then(|| "play.projects.gg".into()),
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
        gone.last_seen = Some(LastSeen::Today);
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
                host: None
            })
        );
        assert!(!friend.game.unwrap().joinable());
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
    fn connection_states() {
        let mut state = loaded();
        state.apply(
            &PresenceEvent::Connected {
                status: StatusMode::Online,
                friends: vec![presence("1", PresenceState::Launcher, None)],
                invites: vec![],
                calls: vec![],
                clock: None,
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
        let view = state.team_view().unwrap();
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
            },
            NOW,
        );
        let zeki = state.team_view().unwrap().members[2].clone();
        assert_eq!(zeki.state, PresenceState::Launcher);
    }

    fn crew_call(id: &str, from: &str, kind: CallKind, at: i64, until: i64) -> CrewCall {
        CrewCall {
            id: id.into(),
            from: from.into(),
            kind,
            at,
            until,
            answers: std::collections::BTreeMap::new(),
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
                reason: Some(TeamExit::Left),
                by: Some(ME.into()),
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
        let view = state.team_view().unwrap();
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
                reason: None,
                by: Some("1".into()),
            },
            NOW,
        );
        let view = state.team_view().unwrap();
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
                reason: None,
                by: None,
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
                reason: None,
                by: None,
            },
            NOW,
        );
        assert!(state.team_invites().is_empty());
        assert_eq!(state.team_view().unwrap().role, TeamRole::Member);

        // Removed by the founder: told once, with the team's name.
        let removed = state.apply(
            &PresenceEvent::TeamChanged {
                team: None,
                reason: Some(TeamExit::Kicked),
                by: Some("2".into()),
            },
            NOW,
        );
        assert!(
            matches!(&removed[..], [SocialNotice::TeamGone { reason: TeamExit::Kicked, name: Some(name) }] if name == "Gece Kartalları")
        );
        assert!(state.team_view().is_none());

        // Leaving on another device says nothing.
        state.team_answer(TeamAnswer {
            team: Some(team(&[("100000000000000000", "Ben", TeamRole::Founder)])),
            invited: vec![],
        });
        let left = state.apply(
            &PresenceEvent::TeamChanged {
                team: None,
                reason: Some(TeamExit::Disbanded),
                by: Some("100000000000000000".into()),
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
}
