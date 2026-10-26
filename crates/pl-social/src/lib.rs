//! Client for the Projects Launcher friends service (`services/social` in the Studio
//! repository): Discord sign-in, friends, blocks, privacy, friend categories, teams (TEAM6: crew
//! tags before names, hidden crews, join requests from a tag's card, "Hoş geldin"), settings
//! backup, the presence WebSocket, crew chat and direct messages with their history on this
//! device (CHAT2: replies, stickers, reactions, "yazıyor…", read marks, link cards), reports,
//! what moderators' measures mean for the player, and SKIN1: the game account's link to the
//! Discord account and the Projects skin of username accounts ([`ProjectsSkins`]).
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

mod afk;
mod auth;
mod backoff;
mod cards;
mod chat;
mod client;
mod closed;
mod codes;
mod error;
mod game;
mod gate;
mod history;
mod links;
mod minecraft;
mod model;
mod presence;
mod protocol;
mod qr;
mod service;
mod state;
mod stickers;
mod store;
mod tls;
mod together;
mod view;
mod world;

#[cfg(test)]
mod testutil;

pub use afk::{AfkItemName, AfkNotice};
pub use auth::{AUTH_DONE_LINK, LoginAttempt, SessionToken, SignedIn, auth_code_from_deep_link};
pub use backoff::{Backoff, BackoffPolicy};
pub use cards::{CARDS_MAX_BYTES, StoredCard};
pub use chat::{
    CARD_WINDOW_MS, CHAT_LINES_MAX, CHAT_TEXT_MAX, DELETE_WINDOW_MS, EDIT_WINDOW_MS,
    REACT_WINDOW_MS, REACTION_KINDS, REACTIONS_PER_PLAYER,
};
pub use client::{CrewKey, PRODUCTION_URL, ReportAnswer, Session, SocialClient, SocialConfig};
pub use codes::{
    CREW_CODE_PREFIX, FRIEND_DEEP_LINK_PREFIX, crew_code, crew_link_url,
    friend_code_from_deep_link, friend_deep_link, friend_share_link, normalize_crew_link,
    normalize_friend_code, normalize_team_code,
};
pub use error::{ErrorCode, Result, SocialError};
pub use game::{GameActivity, LogSignal, log_signal, may_signal, server_label};
pub use gate::service_url;
pub use history::{HISTORY_LIMIT, crew_conversation, direct_conversation, same_emoji};
pub use links::{find_links, first_link, host_of};
pub use minecraft::{
    AccountsFn, FEATURE_MC_CODELESS, FEATURE_MC_LINK, FEATURE_SKINS, KEPT_UNUSED, McBlocked,
    McCheck, McDeclined, McFailure, McHealth, McIntent, McJob, McJobKind, McJobResult, McJobState,
    McLimitedUntil, McLimits, McLink, McLinkState, McMessage, McPremium, McSkin, McSkinBy,
    McSkinKind, McSource, McState, ProjectsLink, ProjectsLinkState, ProjectsSkin,
    ProjectsSkinAccess, ProjectsSkinBy, ProjectsSkinChange, ProjectsSkinChoice,
    ProjectsSkinChoiceKind, ProjectsSkinJob, ProjectsSkinJobKind, ProjectsSkinJobState,
    ProjectsSkinKind, ProjectsSkinLimited, ProjectsSkinLimits, ProjectsSkinLook,
    ProjectsSkinLookup, ProjectsSkinOutcome, ProjectsSkinOverview, ProjectsSkinPick,
    ProjectsSkinPickRefusal, ProjectsSkinRefusal, ProjectsSkinRefusalCode, ProjectsSkinResult,
    ProjectsSkinResultCode, ProjectsSkinSave, ProjectsSkinService, ProjectsSkinView, ProjectsSkins,
    SkinAccount, SkinChangeSink, SkinsConfig,
};
pub use model::{
    Avatar, BlockedUser, ChatBody, ChatCaps, ChatDeletion, ChatExtras, ChatLinkRules, ChatMessage,
    ChatQuota, ChatReacted, ChatRejection, ChatReplyRef, ChatStatus, CodeLookup, CrewBadge,
    CrewEntry, CrewLink, CrewMute, CrewMuteEdit, DEFAULT_CREW_LIMIT, DEFAULT_TEAM_COLOR,
    DEFAULT_TEAM_FRAME, EditFeatures, FEATURE_CHAT, FIRST_EMBLEMS, Friend, FriendGroup,
    FriendGroups, FriendRequest, FriendsOverview, JOIN_REQUEST_LIFETIME_SECS, JoinMode,
    JoinOutcome, JoinRequest, LastSeen, Limited, LimitedItem, LimitedKey, Limits, LinkCard,
    LinkRule, MAX_FRIEND_GROUPS, MAX_MUTE_SECS, Me, ModNotice, NoticeKind, PLAN_NOTE_MAX,
    PermissionLevel, PermissionsEdit, Privacy, ProfileMark, ProfileUpdate, PublicProfile, Reaction,
    ReadMark, Relation, ReportReason, RequestOutcome, SessionInfo, SettingsBackup, SettingsMeta,
    SettingsPrecondition, StatusMode, TEAM_FEATURE_BADGE_REQUESTS, TEAM_FEATURE_BADGES,
    TEAM_FEATURE_CALL_SERVER, TEAM_FEATURE_CALLS, TEAM_FEATURE_CREW_FRIEND_REQUESTS,
    TEAM_FEATURE_LINK, TEAM_FEATURE_LOOK, TEAM_FEATURE_MULTI, TEAM_FEATURE_MUTE,
    TEAM_FEATURE_PLAN_NOTES, TEAM_FEATURE_PLANS, TEAM_FEATURE_REQUESTS, TEAM_FEATURE_ROLES, Team,
    TeamAnswer, TeamEdit, TeamExit, TeamInvite, TeamLook, TeamLookup, TeamMember,
    TeamNotifications, TeamNotificationsEdit, TeamPermissions, TeamRelation, TeamRequest, TeamRole,
    TeamSummary, TeamsOverview, WaitingRequest, discord_avatar_url,
};
pub use presence::{
    ChatSetup, InviteOutcome, PresenceControl, PresenceEvent, PresenceEvents, PresenceHandle,
    PresenceOptions, StopReason,
};
pub use protocol::{
    AFK_CAP, BADGES_CAP, CHAT_CAP, CHAT_EXTRA_CAPS, CallAnswer, CallEndReason, CallKind, CrewCall,
    CrewPlan, Game, GameLocation, Invite, LocalStatus, MARKS_CAP, MC_CAP, OnlineStatus, PLANS_CAP,
    PRESENCE_CAPS, PlanAnswer, Presence, PresenceState, REACTIONS_CAP, READS_CAP, REPLIES_CAP,
    STICKERS_CAP, TYPING_CAP, is_projects_host, presence_caps,
};
pub use qr::{QrPath, qr_path};
pub use service::{ServiceConfig, SocialService, SocialUpdate, UpdateSink};
pub use state::{Connection, GREET_WINDOW_SECS, INVITE_LIFETIME_SECS, SocialState};
pub use stickers::{Sticker, sticker, sticker_sets, sticker_text};
pub use store::{KEYRING_SERVICE, KeyringTokenStore, MemoryTokenStore, TokenStore};
pub use together::{MAX_INTERVAL_SECS, Periods, Together, TogetherLog};
pub use view::{
    ActivityView, AuthPhase, BadgeRefusal, BadgeRequestResult, CallAnswerView, CallOutcome,
    CallPlace, CardIcon, CardKind, CardState, ChatCardView, ChatChangeView, ChatDeletedView,
    ChatExtrasView, ChatLinksView, ChatMessageView, ChatPage, ChatReactionView, ChatReplyView,
    ChatTypingView, ChatView, ConversationView, CrewBadgeView, CrewCallView, CrewJoinView,
    CrewNotificationsView, FriendGroupsView, FriendView, GroupMember, InviteResult, InviteView,
    JoinRequestView, JoinResult, LastMessageView, LimitedView, LookupView, MeView, MessageState,
    ModNoticeView, PlanAnswerView, PlanView, ProfileView, ReportPlan, ReportReaction, ReportTarget,
    RequestView, RestrictionView, SendFailure, SendOutcome, SignInFailure, SkinBlockView,
    SocialNotice, SocialView, TagOffView, TeamInviteView, TeamLookupView, TeamMemberView,
    TeamRequestView, TeamView,
};
pub use world::{
    E4mcHint, Hidden, TurnIce, WORLDS_CAP, WorldAck, WorldAnswer, WorldError, WorldEvent,
    WorldFrame, WorldInvite, WorldRole, WorldSink, WorldsStatus,
};
