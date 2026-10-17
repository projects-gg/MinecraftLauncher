//! The launcher's picture of the friends feature: the profile, friends with their live presence,
//! requests, blocks and waiting invites. A pure reducer over presence events and API results,
//! so every rule is testable without a service.

use std::collections::HashMap;

use serde::Serialize;

use crate::model::{
    BlockedUser, Friend, FriendRequest, FriendsOverview, Limits, Me, PublicProfile, RequestOutcome,
    SettingsMeta,
};
use crate::presence::{PresenceEvent, StopReason};
use crate::protocol::{Game, Invite, OnlineStatus, Presence, PresenceState};
use crate::view::{FriendView, InviteView, ProfileView, RequestView, SocialNotice};

/// How long a received invite stays worth answering (the service delivers them for an hour).
pub const INVITE_LIFETIME_SECS: i64 = 3_600;
const MAX_INVITES: usize = 30;
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
            // Nobody's presence is known during an outage: never show stale "in game".
            self.online.clear();
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
                    if before.is_none() {
                        notices.push(SocialNotice::FriendOnline {
                            id: presence.id.clone(),
                        });
                    }
                    if !was_on_projects && presence.game.as_ref().is_some_and(Game::joinable) {
                        notices.push(SocialNotice::FriendOnProjects {
                            id: presence.id.clone(),
                        });
                    }
                    self.online.insert(presence.id.clone(), presence.clone());
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

    /// Blocking removes friendship and requests both ways and hides presence both ways.
    pub fn blocked(&mut self, profile: PublicProfile) {
        self.forget(&profile.id);
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
            },
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
