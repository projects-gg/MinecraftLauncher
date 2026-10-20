//! Client for the Projects Launcher friends service (`services/social` in the Studio
//! repository): Discord sign-in, friends, blocks, privacy, friend categories, teams, settings
//! backup and the presence WebSocket.
//!
//! Two layers:
//! - [`SocialClient`] / [`Session`] / [`PresenceHandle`]: the HTTP API and the presence socket,
//!   stateless besides configuration.
//! - [`SocialService`]: what the launcher runs. It keeps the session token in a [`TokenStore`]
//!   (the OS keychain), restores the session at start, holds the presence connection, reduces
//!   everything into a [`SocialView`] for the UI ([`SocialState`]) and follows the running game
//!   ([`GameActivity`]). The feature flag decides whether it runs at all ([`service_url`]).
//!
//! ```no_run
//! # async fn demo(done_link: &str) -> pl_social::Result<()> {
//! use pl_social::{
//!     LocalStatus, PresenceOptions, SocialClient, SocialConfig, SocialError,
//!     auth_code_from_deep_link,
//! };
//!
//! let client = SocialClient::new(SocialConfig::production("ProjectsLauncher/2.0.0"))?;
//! let attempt = client.begin_login();
//! // Open attempt.browser_url() in the browser. The sign-in page then opens
//! // `projects://auth/done?c=<one-time code>`; the launcher passes that link on:
//! let code = auth_code_from_deep_link(done_link)
//!     .ok_or(SocialError::InvalidInput("not a sign-in link"))?;
//! let signed_in = client.complete_login(&attempt, &code, "Ev bilgisayarı").await?;
//! let session = client.session(signed_in.token);
//! let mut presence = session.connect_presence(PresenceOptions::default());
//! presence.set_status(LocalStatus::Playing { host: Some("projects.gg".into()) });
//! while let Some(event) = presence.next_event().await {
//!     println!("{event:?}");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Error handling: every failure is a [`SocialError`]. Transient ones
//! ([`SocialError::is_transient`]) carry `retry_after` when the service sent one; retry them later
//! with backoff, never in a loop. The launcher must keep working when the service is down.

mod auth;
mod backoff;
mod client;
mod codes;
mod error;
mod game;
mod gate;
mod model;
mod presence;
mod protocol;
mod qr;
mod service;
mod state;
mod store;
mod tls;
mod view;

#[cfg(test)]
mod testutil;

pub use auth::{AUTH_DONE_LINK, LoginAttempt, SessionToken, SignedIn, auth_code_from_deep_link};
pub use backoff::{Backoff, BackoffPolicy};
pub use client::{PRODUCTION_URL, Session, SocialClient, SocialConfig};
pub use codes::{
    CREW_CODE_PREFIX, FRIEND_DEEP_LINK_PREFIX, crew_code, friend_code_from_deep_link,
    friend_deep_link, friend_share_link, normalize_friend_code, normalize_team_code,
};
pub use error::{ErrorCode, Result, SocialError};
pub use game::{GameActivity, LogSignal, log_signal, may_signal, server_label};
pub use gate::service_url;
pub use model::{
    Avatar, BlockedUser, CodeLookup, DEFAULT_TEAM_COLOR, DEFAULT_TEAM_FRAME, FIRST_EMBLEMS, Friend,
    FriendGroup, FriendGroups, FriendRequest, FriendsOverview, LastSeen, Limits, MAX_FRIEND_GROUPS,
    MAX_MUTE_SECS, Me, PermissionLevel, PermissionsEdit, Privacy, ProfileUpdate, PublicProfile,
    Relation, RequestOutcome, SessionInfo, SettingsBackup, SettingsMeta, SettingsPrecondition,
    StatusMode, TEAM_FEATURE_CALLS, TEAM_FEATURE_LOOK, TEAM_FEATURE_MUTE, TEAM_FEATURE_ROLES, Team,
    TeamAnswer, TeamEdit, TeamExit, TeamInvite, TeamLook, TeamLookup, TeamMember,
    TeamNotifications, TeamNotificationsEdit, TeamPermissions, TeamRelation, TeamRole, TeamSummary,
    TeamsOverview, discord_avatar_url,
};
pub use presence::{
    InviteOutcome, PresenceControl, PresenceEvent, PresenceEvents, PresenceHandle, PresenceOptions,
    StopReason,
};
pub use protocol::{
    CallAnswer, CallEndReason, CallKind, CrewCall, Game, GameLocation, Invite, LocalStatus,
    OnlineStatus, Presence, PresenceState,
};
pub use qr::{QrPath, qr_path};
pub use service::{ServiceConfig, SocialService, SocialUpdate, UpdateSink};
pub use state::{Connection, INVITE_LIFETIME_SECS, SocialState};
pub use store::{KEYRING_SERVICE, KeyringTokenStore, MemoryTokenStore, TokenStore};
pub use view::{
    ActivityView, AuthPhase, CallAnswerView, CallOutcome, CrewCallView, CrewNotificationsView,
    FriendGroupsView, FriendView, GroupMember, InviteResult, InviteView, LookupView, MeView,
    ProfileView, RequestView, SendOutcome, SignInFailure, SocialNotice, SocialView, TeamInviteView,
    TeamLookupView, TeamMemberView, TeamView,
};
