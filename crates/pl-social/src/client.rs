//! HTTP API client. Holds no state of its own besides configuration: the launcher keeps the
//! session token in the OS keychain and hands it to [`SocialClient::session`].
//!
//! Retries: a request that never reached the service (connection failure) is tried once more,
//! and so is a GET/DELETE that timed out or met a 502/504. Everything else is returned as a typed
//! error right away; rate limits and outages carry `retry_after`. Callers must not loop: follow
//! `retry_after`, or back off (see [`crate::BackoffPolicy`]).

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{
    ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, ETAG, HeaderName, IF_MATCH, IF_NONE_MATCH, RETRY_AFTER,
    USER_AGENT,
};
use reqwest::{Method, Response};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use url::Url;

use crate::auth::{LoginAttempt, SessionToken, SignedIn, is_auth_code};
use crate::backoff::jitter;
use crate::codes::{
    friend_share_link, normalize_crew_link, normalize_friend_code, normalize_team_code,
};
use crate::error::{ErrorCode, Result, SocialError, from_response};
use crate::model::{
    CodeLookup, CrewLink, CrewMute, CrewMuteEdit, Friend, FriendGroups, FriendsOverview,
    JoinOutcome, Me, ProfileUpdate, ReportReason, RequestOutcome, SessionInfo, SettingsBackup,
    SettingsMeta, SettingsPrecondition, TeamAnswer, TeamEdit, TeamLook, TeamLookup,
    TeamNotifications, TeamNotificationsEdit, TeamRequest, TeamsOverview,
};
use crate::presence::{ChatSetup, PresenceHandle, PresenceOptions, RelVersion};
use crate::protocol::presence_caps;

/// The production service.
pub const PRODUCTION_URL: &str = "https://social.projects.gg/";

const MAX_RESPONSE_BYTES: usize = 1 << 20;
const MAX_SETTINGS_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct SocialConfig {
    /// Service root, e.g. `https://social.projects.gg/` or `http://127.0.0.1:8797/` locally.
    pub base_url: Url,
    /// `ProjectsLauncher/<version>`.
    pub user_agent: String,
    /// Whole-request timeout.
    pub timeout: Duration,
    pub connect_timeout: Duration,
}

impl SocialConfig {
    pub fn new(base_url: Url, user_agent: impl Into<String>) -> Self {
        Self {
            base_url,
            user_agent: user_agent.into(),
            timeout: Duration::from_secs(15),
            connect_timeout: Duration::from_secs(10),
        }
    }

    pub fn production(user_agent: impl Into<String>) -> Self {
        Self::new(
            Url::parse(PRODUCTION_URL).expect("the production URL is valid"),
            user_agent,
        )
    }
}

/// Cheap to clone.
#[derive(Clone)]
pub struct SocialClient {
    inner: Arc<Inner>,
}

struct Inner {
    http: reqwest::Client,
    base: Url,
    user_agent: String,
    timeout: Duration,
}

impl std::fmt::Debug for SocialClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SocialClient")
            .field("base", &self.inner.base.as_str())
            .finish_non_exhaustive()
    }
}

pub(crate) struct Reply {
    pub status: u16,
    pub retry_after: Option<String>,
    pub etag: Option<String>,
    pub body: Vec<u8>,
}

impl Reply {
    fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    fn error(&self) -> SocialError {
        from_response(self.status, self.retry_after.as_deref(), &self.body)
    }

    fn json<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(&self.body).map_err(|error| {
            SocialError::Protocol(format!("invalid JSON (HTTP {}): {error}", self.status))
        })
    }
}

impl SocialClient {
    pub fn new(config: SocialConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(config.user_agent.clone())
            .connect_timeout(config.connect_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| SocialError::Network(error.to_string()))?;
        Ok(Self::with_http_client(http, config))
    }

    /// Reuses the launcher's shared reqwest client (connection pool, proxy settings).
    pub fn with_http_client(http: reqwest::Client, config: SocialConfig) -> Self {
        let mut base = config.base_url;
        if !base.path().ends_with('/') {
            let path = format!("{}/", base.path());
            base.set_path(&path);
        }
        Self {
            inner: Arc::new(Inner {
                http,
                base,
                user_agent: config.user_agent,
                timeout: config.timeout,
            }),
        }
    }

    pub fn base_url(&self) -> &Url {
        &self.inner.base
    }

    pub(crate) fn user_agent(&self) -> &str {
        &self.inner.user_agent
    }

    /// Starts a sign-in: open [`LoginAttempt::browser_url`] in the browser.
    pub fn begin_login(&self) -> LoginAttempt {
        LoginAttempt::new(&self.inner.base)
    }

    /// Claims the sign-in with `code`, the one-time code of the `projects://auth/done?c=…` link
    /// (see [`crate::auth_code_from_deep_link`]); there is no claim without it.
    /// [`ErrorCode::AuthPendingNotFound`] means the browser step is not done, expired or was
    /// already claimed, or the code belongs to another sign-in. Any failed claim uses the attempt
    /// up: start a new one instead of retrying.
    pub async fn complete_login(
        &self,
        attempt: &LoginAttempt,
        code: &str,
        device: &str,
    ) -> Result<SignedIn> {
        #[derive(Deserialize)]
        struct Claim {
            token: String,
            me: Me,
        }
        if !is_auth_code(code) {
            return Err(SocialError::InvalidInput("not a sign-in code"));
        }
        let body = serde_json::json!({
            "verifier": attempt.verifier(),
            "code": code,
            "device": device_label(device)?,
        });
        let reply = self
            .send(Method::POST, "v1/auth/claim", None, Some(body), &[])
            .await?;
        if !reply.is_success() {
            return Err(reply.error());
        }
        let claim: Claim = reply.json()?;
        let token = SessionToken::new(claim.token);
        if !token.is_well_formed() {
            return Err(SocialError::Protocol("malformed session token".into()));
        }
        Ok(SignedIn {
            token,
            me: claim.me,
        })
    }

    pub fn session(&self, token: SessionToken) -> Session {
        Session {
            client: self.clone(),
            token,
        }
    }

    /// The link a QR code points to: `<service>/f/PRJ-XXXX-XXXX`.
    pub fn friend_share_link(&self, code: &str) -> Option<Url> {
        friend_share_link(&self.inner.base, code)
    }

    /// A crew invite link's page on this service: `<service>/e/XXXX-XXXX-XXXX` (TEAM5).
    pub fn crew_link_url(&self, link: &str) -> Option<Url> {
        crate::codes::crew_link_url(&self.inner.base, link)
    }

    /// BETA-GATE (`GET /v1/beta`): whether this launcher may take the beta update channel; `token`
    /// is the friends session, when someone is signed in. A service from before BETA-GATE answers
    /// `404`: no gate, open. A session that ended (`401`) is nobody signed in. Other failures are
    /// errors (the caller treats them as "cannot tell").
    pub async fn beta(&self, token: Option<&SessionToken>) -> Result<crate::BetaAnswer> {
        let reply = self.send(Method::GET, "v1/beta", token, None, &[]).await?;
        match reply.status {
            404 => Ok(crate::BetaAnswer::Open),
            401 => Ok(crate::BetaAnswer::sign_in()),
            _ if !reply.is_success() => Err(reply.error()),
            _ => crate::beta::answer_of(&reply.body).map_err(|error| {
                SocialError::Protocol(format!(
                    "invalid beta answer (HTTP {}): {error}",
                    reply.status
                ))
            }),
        }
    }

    /// Cheap availability probe (no database access on the service).
    pub async fn health(&self) -> Result<()> {
        let reply = self.send(Method::GET, "v1/health", None, None, &[]).await?;
        if reply.is_success() {
            Ok(())
        } else {
            Err(reply.error())
        }
    }

    pub(crate) fn presence_url(&self) -> Url {
        self.presence_url_with(false)
    }

    /// The presence socket's address, asking for chat too when `chat` (CHAT1).
    pub(crate) fn presence_url_with(&self, chat: bool) -> Url {
        let mut url = self
            .inner
            .base
            .join("v1/presence")
            .expect("a fixed relative path always joins");
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        // http(s) → ws(s) is always a valid scheme change.
        let _ = url.set_scheme(scheme);
        // What this launcher understands (older services ignore the query); commas stay as they
        // are, as the service reads them.
        url.set_query(Some(&format!("caps={}", presence_caps(chat))));
        url
    }

    pub(crate) async fn send(
        &self,
        method: Method,
        path: &str,
        token: Option<&SessionToken>,
        json: Option<serde_json::Value>,
        headers: &[(HeaderName, String)],
    ) -> Result<Reply> {
        let url = self
            .inner
            .base
            .join(path)
            .map_err(|_| SocialError::InvalidInput("invalid path"))?;
        let body = json
            .map(|value| serde_json::to_vec(&value))
            .transpose()
            .map_err(|error| SocialError::Protocol(error.to_string()))?;
        let retry_timeouts = matches!(method, Method::GET | Method::DELETE);
        let mut attempt = 0u32;
        loop {
            let mut request = self
                .inner
                .http
                .request(method.clone(), url.clone())
                .timeout(self.inner.timeout)
                .header(USER_AGENT, &self.inner.user_agent)
                .header(ACCEPT, "application/json");
            if let Some(token) = token {
                request = request.bearer_auth(token.expose());
            }
            for (name, value) in headers {
                request = request.header(name, value);
            }
            if let Some(body) = &body {
                request = request
                    .header(CONTENT_TYPE, "application/json")
                    .body(body.clone());
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if retry_timeouts && attempt == 0 && matches!(status, 502 | 504) {
                        attempt += 1;
                        tokio::time::sleep(jitter(
                            Duration::from_millis(300),
                            Duration::from_millis(1_000),
                        ))
                        .await;
                        continue;
                    }
                    return read_reply(response).await;
                }
                Err(error) => {
                    let retry = error.is_connect() || (retry_timeouts && error.is_timeout());
                    if retry && attempt == 0 {
                        attempt += 1;
                        tokio::time::sleep(jitter(
                            Duration::from_millis(300),
                            Duration::from_millis(1_000),
                        ))
                        .await;
                        continue;
                    }
                    tracing::debug!(error = %describe(&error), "social request failed");
                    return Err(SocialError::Network(describe(&error)));
                }
            }
        }
    }
}

async fn read_reply(mut response: Response) -> Result<Reply> {
    let status = response.status().as_u16();
    let header = |name: HeaderName| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let retry_after = header(RETRY_AFTER);
    let etag = header(ETAG);
    if header(CONTENT_LENGTH)
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > MAX_RESPONSE_BYTES)
    {
        return Err(SocialError::Protocol("response too large".into()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| SocialError::Network(describe(&error)))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(SocialError::Protocol("response too large".into()));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Reply {
        status,
        retry_after,
        etag,
        body,
    })
}

/// A reqwest error with its source chain. URLs here never carry secrets.
fn describe(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(inner) = source {
        let text = inner.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = inner.source();
    }
    message
}

fn device_label(device: &str) -> Result<String> {
    let cleaned: String = device
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned: String = cleaned.chars().take(64).collect();
    if cleaned.is_empty() {
        return Err(SocialError::InvalidInput("device label is empty"));
    }
    Ok(cleaned)
}

fn user_id(id: &str) -> Result<&str> {
    if (17..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()) {
        Ok(id)
    } else {
        Err(SocialError::InvalidInput("not a user id"))
    }
}

fn team_id(id: &str) -> Result<&str> {
    if id.len() == 16
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(id)
    } else {
        Err(SocialError::InvalidInput("not a team id"))
    }
}

/// `"multi": true` in a body, for a service with several crews (an older one refuses the key).
fn with_multi(mut body: serde_json::Value, multi: bool) -> serde_json::Value {
    if multi {
        body["multi"] = true.into();
    }
    body
}

/// What a crew is reached by before joining it: its code (`EKP-…`, `TKM-…` or the symbols
/// alone), or its invite link (the 12 symbols; a service with `link`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrewKey {
    Code(String),
    Link(String),
}

impl CrewKey {
    /// `{"code": "XXXX-XXXX"}` or `{"link": "XXXX-XXXX-XXXX"}`; None when it cannot be one.
    fn body(&self) -> Result<serde_json::Value> {
        match self {
            Self::Code(code) => normalize_team_code(code)
                .map(|code| serde_json::json!({ "code": code }))
                .ok_or(SocialError::InvalidInput("not a team code")),
            Self::Link(link) => normalize_crew_link(link)
                .map(|link| serde_json::json!({ "link": link }))
                .ok_or(SocialError::InvalidInput("not a crew link")),
        }
    }
}

/// An authenticated client for one session. Cheap to clone.
#[derive(Clone)]
pub struct Session {
    client: SocialClient,
    token: SessionToken,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").finish_non_exhaustive()
    }
}

impl Session {
    pub fn token(&self) -> &SessionToken {
        &self.token
    }

    pub fn client(&self) -> &SocialClient {
        &self.client
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        json: Option<serde_json::Value>,
    ) -> Result<Reply> {
        let reply = self
            .client
            .send(method, path, Some(&self.token), json, &[])
            .await?;
        if reply.is_success() {
            Ok(reply)
        } else {
            Err(reply.error())
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.call(Method::GET, path, None).await?.json()
    }

    /// Profile, friend code, status, privacy and the settings backup metadata.
    pub async fn me(&self) -> Result<Me> {
        self.get("v1/me").await
    }

    /// Profile picture, status mode (online / dnd / invisible) and privacy switches.
    pub async fn update_profile(&self, update: &ProfileUpdate) -> Result<Me> {
        let body = serde_json::to_value(update)
            .map_err(|error| SocialError::Protocol(error.to_string()))?;
        self.call(Method::PATCH, "v1/me", Some(body)).await?.json()
    }

    /// A new friend code; the old one stops working immediately.
    pub async fn rotate_friend_code(&self) -> Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Rotated {
            friend_code: String,
        }
        let rotated: Rotated = self
            .call(Method::POST, "v1/me/friend-code", None)
            .await?
            .json()?;
        Ok(rotated.friend_code)
    }

    /// "Verilerimi sil": every row of the account; all sessions end.
    pub async fn delete_account(&self) -> Result<()> {
        self.call(Method::DELETE, "v1/me", None).await.map(drop)
    }

    /// Ends this session. Forget the token afterwards even if this fails.
    pub async fn logout(&self) -> Result<()> {
        self.call(Method::POST, "v1/auth/logout", None)
            .await
            .map(drop)
    }

    pub async fn sessions(&self) -> Result<Vec<SessionInfo>> {
        #[derive(Deserialize)]
        struct Sessions {
            sessions: Vec<SessionInfo>,
        }
        Ok(self.get::<Sessions>("v1/sessions").await?.sessions)
    }

    pub async fn revoke_session(&self, id: &str) -> Result<()> {
        if id.len() != 16 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(SocialError::InvalidInput("not a session id"));
        }
        self.call(Method::DELETE, &format!("v1/sessions/{id}"), None)
            .await
            .map(drop)
    }

    /// Signs out every other device. Returns how many sessions ended.
    pub async fn revoke_other_sessions(&self) -> Result<u32> {
        #[derive(Deserialize)]
        struct Revoked {
            revoked: u32,
        }
        let revoked: Revoked = self
            .call(Method::POST, "v1/sessions/revoke-others", None)
            .await?
            .json()?;
        Ok(revoked.revoked)
    }

    /// Friends, incoming and outgoing requests and blocked users. SCALE-SOC: with `rel=1` a
    /// service that knows it also brings `me` and the relations version (an older one ignores
    /// the query and brings neither).
    pub async fn friends(&self) -> Result<FriendsOverview> {
        self.get("v1/friends?rel=1").await
    }

    /// Who owns a code (counts against a daily lookup quota). Malformed codes fail locally.
    pub async fn lookup_code(&self, code: &str) -> Result<CodeLookup> {
        let code =
            normalize_friend_code(code).ok_or(SocialError::InvalidInput("not a friend code"))?;
        self.call(
            Method::POST,
            "v1/friends/lookup",
            Some(serde_json::json!({ "code": code })),
        )
        .await?
        .json()
    }

    /// Sends a request by code; if they had already asked, you become friends right away.
    pub async fn send_request(&self, code: &str) -> Result<RequestOutcome> {
        let code =
            normalize_friend_code(code).ok_or(SocialError::InvalidInput("not a friend code"))?;
        self.call(
            Method::POST,
            "v1/friends/requests",
            Some(serde_json::json!({ "code": code })),
        )
        .await?
        .json()
    }

    /// Sends a request to a crewmate, without their code (FIX2; a service with
    /// `crewFriendRequests`): the code's answers, caps and daily lookup quota. Someone who is not
    /// a crewmate (or blocked the player, or is suspended) answers `user_not_found`.
    pub async fn send_request_to(&self, user: &str) -> Result<RequestOutcome> {
        let user = user_id(user)?;
        self.call(
            Method::POST,
            "v1/friends/requests",
            Some(serde_json::json!({ "user": user })),
        )
        .await?
        .json()
    }

    pub async fn accept_request(&self, from: &str) -> Result<Friend> {
        #[derive(Deserialize)]
        struct Accepted {
            friend: Friend,
        }
        let path = format!("v1/friends/requests/{}/accept", user_id(from)?);
        let accepted: Accepted = self.call(Method::POST, &path, None).await?.json()?;
        Ok(accepted.friend)
    }

    pub async fn decline_request(&self, from: &str) -> Result<()> {
        let path = format!("v1/friends/requests/{}/decline", user_id(from)?);
        self.call(Method::POST, &path, None).await.map(drop)
    }

    pub async fn cancel_request(&self, to: &str) -> Result<()> {
        let path = format!("v1/friends/requests/{}", user_id(to)?);
        self.call(Method::DELETE, &path, None).await.map(drop)
    }

    pub async fn remove_friend(&self, id: &str) -> Result<()> {
        let path = format!("v1/friends/{}", user_id(id)?);
        self.call(Method::DELETE, &path, None).await.map(drop)
    }

    /// Removes the friendship and pending requests both ways and hides presence both ways.
    pub async fn block(&self, id: &str) -> Result<()> {
        let path = format!("v1/blocks/{}", user_id(id)?);
        self.call(Method::PUT, &path, None).await.map(drop)
    }

    pub async fn unblock(&self, id: &str) -> Result<()> {
        let path = format!("v1/blocks/{}", user_id(id)?);
        self.call(Method::DELETE, &path, None).await.map(drop)
    }

    // --- friend categories --------------------------------------------------------------------

    /// Replaces the player's friend categories (the whole document); returns the new version.
    /// Only the player ever sees them; their other devices get them over the presence socket.
    pub async fn put_friend_groups(&self, groups: &FriendGroups) -> Result<u64> {
        #[derive(Deserialize)]
        struct Saved {
            version: u64,
        }
        let body = serde_json::json!({ "groups": groups.groups, "members": groups.members });
        let saved: Saved = self
            .call(Method::PUT, "v1/friends/groups", Some(body))
            .await?
            .json()?;
        Ok(saved.version)
    }

    // --- teams --------------------------------------------------------------------------------

    /// The player's team, invites to them and their own waiting invites (also part of
    /// [`Session::friends`]).
    pub async fn teams(&self) -> Result<TeamsOverview> {
        self.get("v1/teams").await
    }

    /// Founds a team. `look` only for a service that offers looks (an older one refuses the
    /// keys); `multi` only for one that offers several crews: with it a player below the crew
    /// limit founds one more, without it only a player in no crew founds one.
    pub async fn create_team(
        &self,
        name: &str,
        tag: &str,
        emblem: &str,
        look: Option<&TeamLook>,
        multi: bool,
    ) -> Result<TeamAnswer> {
        let mut body = serde_json::json!({ "name": name, "tag": tag, "emblem": emblem });
        if let Some(look) = look {
            body["color"] = look.color.clone().into();
            body["frame"] = look.frame.clone().into();
            body["motto"] = look.motto.clone().into();
        }
        if multi {
            body["multi"] = true.into();
        }
        self.call(Method::POST, "v1/teams", Some(body))
            .await?
            .json()
    }

    /// The player's crews in a new order (every crew exactly once; the first becomes the primary
    /// crew). `crews_changed` when they are not exactly the player's crews any more.
    pub async fn order_crews(&self, order: &[String]) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct Ordered {
            order: Vec<String>,
        }
        for id in order {
            team_id(id)?;
        }
        let ordered: Ordered = self
            .call(
                Method::PUT,
                "v1/teams/order",
                Some(serde_json::json!({ "order": order })),
            )
            .await?
            .json()?;
        Ok(ordered.order)
    }

    /// Mutes one of the player's crews for the player, or unmutes it (a service with several
    /// crews): answers the crew's mute as the service keeps it now.
    pub async fn set_crew_mute(&self, team: &str, edit: &CrewMuteEdit) -> Result<CrewMute> {
        let body =
            serde_json::to_value(edit).map_err(|error| SocialError::Protocol(error.to_string()))?;
        let path = format!("v1/teams/{}/notifications", team_id(team)?);
        self.call(Method::PATCH, &path, Some(body)).await?.json()
    }

    /// Renames the team, changes its tag, emblem or look (founder).
    pub async fn update_team(&self, team: &str, edit: &TeamEdit) -> Result<TeamAnswer> {
        let body =
            serde_json::to_value(edit).map_err(|error| SocialError::Protocol(error.to_string()))?;
        let path = format!("v1/teams/{}", team_id(team)?);
        self.call(Method::PATCH, &path, Some(body)).await?.json()
    }

    /// Ends the team for everyone (founder).
    pub async fn disband_team(&self, team: &str) -> Result<()> {
        let path = format!("v1/teams/{}", team_id(team)?);
        self.call(Method::DELETE, &path, None).await.map(drop)
    }

    /// Leaves the team. A founder with members hands the role on first.
    pub async fn leave_team(&self, team: &str) -> Result<()> {
        let path = format!("v1/teams/{}/leave", team_id(team)?);
        self.call(Method::POST, &path, None).await.map(drop)
    }

    /// A new team code (founder); the old one stops working at once.
    pub async fn rotate_team_code(&self, team: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Rotated {
            code: String,
        }
        let path = format!("v1/teams/{}/code", team_id(team)?);
        let rotated: Rotated = self.call(Method::POST, &path, None).await?.json()?;
        Ok(rotated.code)
    }

    /// Invites one of the player's friends to their team.
    pub async fn invite_to_team(&self, team: &str, friend: &str) -> Result<()> {
        let path = format!("v1/teams/{}/invites", team_id(team)?);
        let body = serde_json::json!({ "to": user_id(friend)? });
        self.call(Method::POST, &path, Some(body)).await.map(drop)
    }

    /// Removes a member (founder); the team code is renewed with it.
    pub async fn remove_team_member(&self, team: &str, member: &str) -> Result<()> {
        let path = format!("v1/teams/{}/members/{}", team_id(team)?, user_id(member)?);
        self.call(Method::DELETE, &path, None).await.map(drop)
    }

    /// Hands the founder role to another member.
    pub async fn transfer_team_founder(&self, team: &str, member: &str) -> Result<TeamAnswer> {
        let path = format!("v1/teams/{}/founder", team_id(team)?);
        let body = serde_json::json!({ "to": user_id(member)? });
        self.call(Method::POST, &path, Some(body)).await?.json()
    }

    /// Makes a member a moderator, or a moderator a member again (leader; a service with
    /// roles). Asking for what already is answers the crew as it is.
    /// The player's crew notification settings (`PATCH /v1/teams/notifications`, a service with
    /// `mute`): answers what the service keeps now.
    pub async fn set_team_notifications(
        &self,
        edit: &TeamNotificationsEdit,
    ) -> Result<TeamNotifications> {
        let body =
            serde_json::to_value(edit).map_err(|error| SocialError::Protocol(error.to_string()))?;
        self.call(Method::PATCH, "v1/teams/notifications", Some(body))
            .await?
            .json()
    }

    pub async fn set_team_moderator(
        &self,
        team: &str,
        member: &str,
        moderator: bool,
    ) -> Result<TeamAnswer> {
        let path = format!(
            "v1/teams/{}/moderators/{}",
            team_id(team)?,
            user_id(member)?
        );
        let method = if moderator {
            Method::PUT
        } else {
            Method::DELETE
        };
        self.call(method, &path, None).await?.json()
    }

    /// What a team code leads to (counts against the daily code lookup quota). With `multi` (a
    /// service with several crews) a player in other crews reads `none` below the crew limit and
    /// `limit` at it, never `elsewhere`.
    pub async fn lookup_team(&self, code: &str, multi: bool) -> Result<TeamLookup> {
        let code = normalize_team_code(code).ok_or(SocialError::InvalidInput("not a team code"))?;
        self.call(
            Method::POST,
            "v1/teams/lookup",
            Some(with_multi(serde_json::json!({ "code": code }), multi)),
        )
        .await?
        .json()
    }

    /// Joins the team of a code (one more crew with `multi`); answers the crew joined.
    pub async fn join_team(&self, code: &str, multi: bool) -> Result<TeamAnswer> {
        let code = normalize_team_code(code).ok_or(SocialError::InvalidInput("not a team code"))?;
        self.call(
            Method::POST,
            "v1/teams/join",
            Some(with_multi(serde_json::json!({ "code": code }), multi)),
        )
        .await?
        .json()
    }

    /// Accepts an invite (one more crew with `multi`); answers the crew joined.
    pub async fn accept_team_invite(&self, team: &str, multi: bool) -> Result<TeamAnswer> {
        let path = format!("v1/teams/invites/{}/accept", team_id(team)?);
        let body = multi.then(|| serde_json::json!({ "multi": true }));
        self.call(Method::POST, &path, body).await?.json()
    }

    pub async fn decline_team_invite(&self, team: &str) -> Result<()> {
        let path = format!("v1/teams/invites/{}/decline", team_id(team)?);
        self.call(Method::POST, &path, None).await.map(drop)
    }

    // --- joining a crew (TEAM5) -------------------------------------------------------------

    /// What a crew code or invite link leads to (counts against the daily code lookup quota):
    /// the crew, how it relates to the player, whether it approves newcomers and the player's
    /// join request to it while that waits. `multi` as for [`Session::lookup_team`].
    pub async fn lookup_crew(&self, key: &CrewKey, multi: bool) -> Result<TeamLookup> {
        self.call(
            Method::POST,
            "v1/teams/lookup",
            Some(with_multi(key.body()?, multi)),
        )
        .await?
        .json()
    }

    /// Joins the crew of a code or an invite link. With `request` (a service with `requests`) a
    /// crew that approves newcomers takes a join request instead (`202`); without it such a crew
    /// answers `team_approval_required`. Asking again while a request waits answers the same.
    pub async fn join_crew(
        &self,
        key: &CrewKey,
        multi: bool,
        request: bool,
    ) -> Result<JoinOutcome> {
        #[derive(Deserialize)]
        struct Requested {
            request: TeamRequest,
        }
        let mut body = with_multi(key.body()?, multi);
        if request {
            body["request"] = true.into();
        }
        let reply = self.call(Method::POST, "v1/teams/join", Some(body)).await?;
        if reply.status == 202 {
            let requested: Requested = reply.json()?;
            Ok(JoinOutcome::Requested(requested.request))
        } else {
            Ok(JoinOutcome::Joined(reply.json()?))
        }
    }

    /// Asks to join a crew from the card of the tag `via` wears (TEAM6, a service with
    /// `badgeRequests`): the request always waits for the crew's leaders, even in an open crew.
    /// Answers the request as it waits (the first `sentAt` when one waited already). A block from
    /// someone in the crew answers the same and stores nothing.
    pub async fn request_from_badge(&self, team: &str, via: &str) -> Result<TeamRequest> {
        #[derive(Deserialize)]
        struct Requested {
            request: TeamRequest,
        }
        let path = format!("v1/teams/{}/requests", team_id(team)?);
        let body = serde_json::json!({ "via": user_id(via)? });
        let requested: Requested = self.call(Method::POST, &path, Some(body)).await?.json()?;
        Ok(requested.request)
    }

    /// Withdraws the player's waiting join request to a crew (`team_request_not_found` when
    /// none waits).
    pub async fn withdraw_team_request(&self, team: &str) -> Result<()> {
        let path = format!("v1/teams/requests/{}", team_id(team)?);
        self.call(Method::DELETE, &path, None).await.map(drop)
    }

    /// Takes a player who asked to join into one of the player's crews (`permissions.admit`);
    /// answers the crew. `limit_team_members` and `limit_crews` leave the request waiting.
    pub async fn accept_team_request(&self, team: &str, user: &str) -> Result<TeamAnswer> {
        let path = format!(
            "v1/teams/{}/requests/{}/accept",
            team_id(team)?,
            user_id(user)?
        );
        self.call(Method::POST, &path, None).await?.json()
    }

    /// Declines a join request (silent for the player who asked).
    pub async fn decline_team_request(&self, team: &str, user: &str) -> Result<()> {
        let path = format!(
            "v1/teams/{}/requests/{}/decline",
            team_id(team)?,
            user_id(user)?
        );
        self.call(Method::POST, &path, None).await.map(drop)
    }

    /// A new invite link for one of the player's crews, or one in place of the old one (which
    /// stops working at once; `permissions.admit`; shares the 5 code renewals a day).
    pub async fn create_team_link(&self, team: &str) -> Result<CrewLink> {
        let path = format!("v1/teams/{}/link", team_id(team)?);
        self.call(Method::POST, &path, None).await?.json()
    }

    /// The crew has no invite link any more (`permissions.admit`).
    pub async fn delete_team_link(&self, team: &str) -> Result<()> {
        let path = format!("v1/teams/{}/link", team_id(team)?);
        self.call(Method::DELETE, &path, None).await.map(drop)
    }

    /// The settings backup, or None if there is none.
    pub async fn settings(&self) -> Result<Option<SettingsBackup>> {
        match self.get("v1/settings").await {
            Ok(backup) => Ok(Some(backup)),
            Err(SocialError::Refused {
                code: ErrorCode::SettingsNotFound,
                ..
            }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Uploads launcher preferences only (never passwords, tokens or AFK login commands; the
    /// service rejects secret-looking keys). `data` must be a JSON object of at most 64 KB.
    pub async fn put_settings(
        &self,
        device: &str,
        data: &serde_json::Value,
        precondition: SettingsPrecondition,
    ) -> Result<SettingsMeta> {
        if !data.is_object() {
            return Err(SocialError::InvalidInput("settings must be a JSON object"));
        }
        let size = serde_json::to_vec(data)
            .map_err(|error| SocialError::Protocol(error.to_string()))?
            .len();
        if size > MAX_SETTINGS_BYTES {
            return Err(SocialError::InvalidInput("settings are larger than 64 KB"));
        }
        let header = match precondition {
            SettingsPrecondition::Create => (IF_NONE_MATCH, "*".to_owned()),
            SettingsPrecondition::Replace(version) => (IF_MATCH, format!("\"{version}\"")),
        };
        let body = serde_json::json!({ "device": device_label(device)?, "data": data });
        let reply = self
            .client
            .send(
                Method::PUT,
                "v1/settings",
                Some(&self.token),
                Some(body),
                &[header],
            )
            .await?;
        if !reply.is_success() {
            return Err(reply.error());
        }
        let meta: SettingsMeta = reply.json()?;
        if let Some(etag) = &reply.etag
            && etag.trim_matches('"') != meta.version.to_string()
        {
            return Err(SocialError::Protocol("ETag and version disagree".into()));
        }
        Ok(meta)
    }

    /// Turns the backup off and deletes the stored copy.
    pub async fn delete_settings(&self) -> Result<()> {
        self.call(Method::DELETE, "v1/settings", None)
            .await
            .map(drop)
    }

    /// Starts the presence connection in the background (reconnects by itself).
    pub fn connect_presence(&self, options: PresenceOptions) -> PresenceHandle {
        PresenceHandle::spawn(
            self.client.presence_url(),
            self.token.clone(),
            self.client.user_agent().to_owned(),
            options,
            None,
            None,
        )
    }

    /// The presence connection with chat (CHAT1): `chat` in the capabilities, and a sync from
    /// `chat.since` after every connect.
    pub fn connect_presence_with_chat(
        &self,
        options: PresenceOptions,
        chat: ChatSetup,
    ) -> PresenceHandle {
        PresenceHandle::spawn(
            self.client.presence_url_with(true),
            self.token.clone(),
            self.client.user_agent().to_owned(),
            options,
            Some(chat),
            None,
        )
    }

    /// The presence connection with chat when `chat` is given (CHAT1) and world sessions when
    /// `worlds` (HOST1: `worlds2` in the capabilities; its frames flow once the service took it).
    /// `rel` (SCALE-SOC): every connect names the relations version it holds.
    pub fn connect_presence_with(
        &self,
        options: PresenceOptions,
        chat: Option<ChatSetup>,
        worlds: bool,
        rel: Option<RelVersion>,
    ) -> PresenceHandle {
        let mut url = self.client.presence_url_with(chat.is_some());
        if worlds {
            crate::world::ask_for_worlds(&mut url);
        }
        PresenceHandle::spawn(
            url,
            self.token.clone(),
            self.client.user_agent().to_owned(),
            options,
            chat,
            rel,
        )
    }

    /// "Rapor et" (MOD1): `target` is the JSON the service takes (`{"kind": "player", "id"}`,
    /// `{"kind": "crew", "id", "code"?}`, `{"kind": "message", "message", "before"}`). A note of at
    /// most 200 characters; `block` also blocks the player (players and messages only). Answers
    /// whether a report about it from the player was still open (nothing new was filed then).
    pub async fn report(
        &self,
        target: serde_json::Value,
        reason: ReportReason,
        note: Option<&str>,
        block: bool,
    ) -> Result<ReportAnswer> {
        let mut body = serde_json::json!({ "target": target, "reason": reason.as_wire() });
        if let Some(note) = note.map(str::trim).filter(|note| !note.is_empty()) {
            body["note"] = note.into();
        }
        if block {
            body["block"] = true.into();
        }
        self.call(Method::POST, "v1/reports", Some(body))
            .await?
            .json()
    }

    /// The launcher showed these moderators' notes (unknown ids are ignored).
    pub async fn notices_seen(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.call(
            Method::POST,
            "v1/me/notices/seen",
            Some(serde_json::json!({ "ids": ids })),
        )
        .await
        .map(drop)
    }
}

/// What a report answered.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReportAnswer {
    #[serde(default)]
    pub id: String,
    /// The player's earlier report about it is still open: nothing new was filed.
    #[serde(default)]
    pub duplicate: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{MockReply, MockServer};

    fn client(server: &MockServer) -> SocialClient {
        SocialClient::new(SocialConfig::new(
            Url::parse(&server.base()).unwrap(),
            "ProjectsLauncher/test",
        ))
        .unwrap()
    }

    fn token() -> SessionToken {
        SessionToken::new(format!("pls_{}", "A".repeat(43)))
    }

    const ME: &str = r#"{"id":"123456789012345678","username":"ayse","displayName":null,"avatar":"discord",
        "avatarHash":null,"discordAvatarHash":null,"friendCode":"PRJ-ABCD-EFGH","status":"online",
        "privacy":{"shareServer":false,"shareLastSeen":true},"createdOn":"2026-10-16","settings":null}"#;

    /// A well-formed one-time code, as in `projects://auth/done?c=…`.
    fn sign_in_code() -> String {
        "C".repeat(43)
    }

    #[tokio::test]
    async fn claims_a_sign_in_with_the_verifier_and_the_code() {
        let server = MockServer::start(|req| {
            assert_eq!(req.path, "/v1/auth/claim");
            let body = req.json();
            assert_eq!(body["device"], "Ev PC");
            assert_eq!(body["verifier"].as_str().unwrap().len(), 43);
            assert_eq!(body["code"], "C".repeat(43));
            MockReply::json(
                200,
                &format!(r#"{{"token":"pls_{}","me":{ME}}}"#, "B".repeat(43)),
            )
        })
        .await;
        let client = client(&server);
        let attempt = client.begin_login();
        assert!(
            attempt
                .browser_url()
                .as_str()
                .starts_with(&format!("{}auth/discord/start?challenge=", server.base()))
        );
        let link = format!("projects://auth/done?c={}", sign_in_code());
        let code = crate::auth_code_from_deep_link(&link).unwrap();
        let signed_in = client
            .complete_login(&attempt, &code, " Ev \u{7} PC ")
            .await
            .unwrap();
        assert!(signed_in.token.is_well_formed());
        assert_eq!(signed_in.me.friend_code, "PRJ-ABCD-EFGH");
        let recorded = server.requests();
        assert_eq!(
            recorded[0].header("user-agent"),
            Some("ProjectsLauncher/test")
        );
        // Verifier and code only travel in the claim body, never in a URL.
        assert!(!recorded[0].target.contains(attempt.verifier()));
        assert!(!recorded[0].target.contains(&code));
    }

    #[tokio::test]
    async fn a_claim_without_a_valid_code_never_reaches_the_service() {
        let server = MockServer::start(|_| MockReply::json(500, "{}")).await;
        let client = client(&server);
        let attempt = client.begin_login();
        let malformed = [
            String::new(),
            "short".to_owned(),
            format!("{}!", "C".repeat(42)),
            "C".repeat(44),
        ];
        for code in &malformed {
            assert!(matches!(
                client.complete_login(&attempt, code, "PC").await,
                Err(SocialError::InvalidInput(_))
            ));
        }
        assert!(server.requests().is_empty());
    }

    #[tokio::test]
    async fn sends_the_bearer_token_and_maps_errors() {
        let server = MockServer::start(|req| match req.path.as_str() {
            "/v1/me" => MockReply::json(200, ME),
            "/v1/friends" => MockReply::json(401, r#"{"error":{"code":"unauthorized","message":"x"}}"#),
            "/v1/friends/lookup" => MockReply::json(
                429,
                r#"{"error":{"code":"rate_limited","message":"Daily limit reached","retryAfter":3600}}"#,
            )
            .with_header("Retry-After", "3600"),
            "/v1/friends/requests" => MockReply::json(
                409,
                r#"{"error":{"code":"already_friends","message":"Already friends"}}"#,
            ),
            _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
        })
        .await;
        let session = client(&server).session(token());
        assert_eq!(session.me().await.unwrap().profile.username, "ayse");
        let requests = server.requests();
        assert_eq!(
            requests[0].header("authorization"),
            Some(format!("Bearer pls_{}", "A".repeat(43)).as_str())
        );
        assert!(matches!(
            session.friends().await,
            Err(SocialError::Unauthorized)
        ));
        let limited = session.lookup_code("PRJ-ABCD-EFGH").await.unwrap_err();
        assert_eq!(limited.retry_after(), Some(Duration::from_secs(3600)));
        let refused = session.send_request("abcd efgh").await.unwrap_err();
        assert_eq!(refused.code(), Some(&ErrorCode::AlreadyFriends));
        // Malformed codes and ids never reach the service.
        let before = server.requests().len();
        assert!(matches!(
            session.lookup_code("nope").await,
            Err(SocialError::InvalidInput(_))
        ));
        assert!(matches!(
            session.block("../me").await,
            Err(SocialError::InvalidInput(_))
        ));
        assert_eq!(server.requests().len(), before);
    }

    #[tokio::test]
    async fn asks_about_the_beta_channel_with_or_without_a_session() {
        let server = MockServer::start(|req| {
            match req.header("authorization") {
                // The session of a player on the team.
                Some(bearer) if bearer.ends_with(&"A".repeat(43)) => MockReply::json(
                    200,
                    r#"{"gate":true,"access":"member","apply":"https://projects.gg/basvuru"}"#,
                ),
                Some(_) => {
                    MockReply::json(401, r#"{"error":{"code":"unauthorized","message":"x"}}"#)
                }
                None => MockReply::json(
                    200,
                    r#"{"gate":true,"access":"signin","apply":"https://projects.gg/arge"}"#,
                ),
            }
        })
        .await;
        let social = client(&server);
        assert_eq!(
            social.beta(Some(&token())).await.unwrap(),
            crate::BetaAnswer::Member
        );
        assert_eq!(
            social.beta(None).await.unwrap(),
            crate::BetaAnswer::SignIn {
                apply: Url::parse("https://projects.gg/arge").unwrap()
            }
        );
        // A session that ended is nobody signed in.
        let ended = SessionToken::new(format!("pls_{}", "B".repeat(43)));
        assert_eq!(
            social.beta(Some(&ended)).await.unwrap(),
            crate::BetaAnswer::sign_in()
        );
        let requests = server.requests();
        assert!(
            requests
                .iter()
                .all(|r| r.method == "GET" && r.path == "/v1/beta")
        );
        assert_eq!(requests[1].header("authorization"), None);

        // A service from before BETA-GATE has no gate; one that is down is an error.
        let old = MockServer::start(|_| {
            MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#)
        })
        .await;
        assert_eq!(
            client(&old).beta(None).await.unwrap(),
            crate::BetaAnswer::Open
        );
        let down = MockServer::start(|_| {
            MockReply::json(503, r#"{"error":{"code":"unavailable","message":"x"}}"#)
        })
        .await;
        assert!(client(&down).beta(None).await.is_err());
    }

    const TEAM: &str = r#"{"team":{"id":"0123456789abcdef","name":"Gece Kartalları","tag":"GKR",
        "emblem":"sword","code":"TKM-ABCD-EFGH","createdOn":"2026-10-19","members":[{"id":"123456789012345678",
        "username":"ayse","displayName":null,"avatar":"discord","avatarHash":null,"role":"founder",
        "joinedAt":5}]},"invited":[]}"#;

    #[tokio::test]
    async fn team_and_category_calls() {
        let server = MockServer::start(|req| match (req.method.as_str(), req.path.as_str()) {
            ("POST", "/v1/teams") => {
                let body = req.json();
                assert_eq!(body["name"], "Gece Kartalları");
                if body["emblem"] == "sword" {
                    // No look: only the keys every service takes.
                    assert!(body.get("color").is_none() && body.get("motto").is_none());
                } else {
                    assert_eq!(body["emblem"], "controller");
                    assert_eq!(body["color"], "teal");
                    assert_eq!(body["frame"], "banner");
                    assert_eq!(body["motto"], "");
                }
                MockReply::json(201, TEAM)
            }
            ("POST", "/v1/teams/join") | ("POST", "/v1/teams/lookup") => {
                // Without a prefix: every service version takes the symbols alone.
                assert_eq!(req.json()["code"], "ABCD-EFGH");
                if req.path.ends_with("lookup") {
                    MockReply::json(
                        200,
                        r#"{"team":{"id":"0123456789abcdef","name":"Gece Kartalları","tag":"GKR",
                                "emblem":"sword","members":3},"relation":"elsewhere"}"#,
                    )
                } else {
                    MockReply::json(200, TEAM)
                }
            }
            ("PATCH", "/v1/teams/0123456789abcdef") => {
                assert_eq!(req.body_text(), r#"{"tag":"PVP"}"#);
                MockReply::json(200, TEAM)
            }
            ("POST", "/v1/teams/0123456789abcdef/invites") => {
                assert_eq!(req.json()["to"], "223456789012345678");
                MockReply::empty(204)
            }
            ("POST", "/v1/teams/0123456789abcdef/code") => {
                MockReply::json(200, r#"{"code":"TKM-WXYZ-1234"}"#)
            }
            ("PATCH", "/v1/teams/notifications") => {
                assert_eq!(req.body_text(), r#"{"muteUntil":1792600000,"muted":true}"#);
                MockReply::json(200, r#"{"calls":true,"muted":true,"muteUntil":1792600000}"#)
            }
            ("PUT" | "DELETE", "/v1/teams/0123456789abcdef/moderators/223456789012345678") => {
                assert!(req.body_text().is_empty());
                MockReply::json(200, TEAM)
            }
            ("DELETE", "/v1/teams/0123456789abcdef/members/223456789012345678")
            | ("POST", "/v1/teams/0123456789abcdef/leave")
            | ("POST", "/v1/teams/invites/fedcba9876543210/decline") => MockReply::empty(204),
            ("POST", "/v1/teams/invites/fedcba9876543210/accept") => MockReply::json(
                409,
                r#"{"error":{"code":"already_in_team","message":"Leave your team first"}}"#,
            ),
            ("PUT", "/v1/friends/groups") => {
                let body = req.json();
                assert_eq!(body["groups"][0]["id"], "okul");
                assert_eq!(body["members"]["223456789012345678"], "okul");
                assert!(body.get("version").is_none());
                MockReply::json(200, r#"{"version":4}"#)
            }
            _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
        })
        .await;
        let session = client(&server).session(token());
        let created = session
            .create_team("Gece Kartalları", "gkr", "sword", None, false)
            .await
            .unwrap();
        assert_eq!(created.team.unwrap().code, "TKM-ABCD-EFGH");
        let look = TeamLook {
            color: "teal".into(),
            frame: "banner".into(),
            motto: String::new(),
        };
        session
            .create_team("Gece Kartalları", "GKR", "controller", Some(&look), false)
            .await
            .unwrap();
        assert!(session.lookup_team("ekp abcd efgh", false).await.is_ok());
        let lookup = session.lookup_team("tkm abcd efgh", false).await.unwrap();
        assert_eq!(lookup.relation, crate::model::TeamRelation::Elsewhere);
        assert_eq!(lookup.team.members, 3);
        assert!(
            session
                .join_team("abcdefgh", false)
                .await
                .unwrap()
                .team
                .is_some()
        );
        let edit = TeamEdit {
            tag: Some("PVP".into()),
            ..TeamEdit::default()
        };
        session
            .update_team("0123456789abcdef", &edit)
            .await
            .unwrap();
        session
            .invite_to_team("0123456789abcdef", "223456789012345678")
            .await
            .unwrap();
        assert_eq!(
            session.rotate_team_code("0123456789abcdef").await.unwrap(),
            "TKM-WXYZ-1234"
        );
        session
            .remove_team_member("0123456789abcdef", "223456789012345678")
            .await
            .unwrap();
        for moderator in [true, false] {
            session
                .set_team_moderator("0123456789abcdef", "223456789012345678", moderator)
                .await
                .unwrap();
        }
        let methods: Vec<String> = server
            .requests()
            .iter()
            .filter(|r| r.path.contains("/moderators/"))
            .map(|r| r.method.clone())
            .collect();
        assert_eq!(methods, vec!["PUT".to_owned(), "DELETE".to_owned()]);
        let muted = session
            .set_team_notifications(&TeamNotificationsEdit {
                muted: Some(true),
                mute_until: Some(1_792_600_000),
                ..TeamNotificationsEdit::default()
            })
            .await
            .unwrap();
        assert!(muted.calls && muted.muted && muted.mute_until == Some(1_792_600_000));
        session.leave_team("0123456789abcdef").await.unwrap();
        session
            .decline_team_invite("fedcba9876543210")
            .await
            .unwrap();
        let refused = session
            .accept_team_invite("fedcba9876543210", false)
            .await
            .unwrap_err();
        assert_eq!(refused.code(), Some(&ErrorCode::AlreadyInTeam));

        let mut groups = FriendGroups::default();
        groups.groups.push(crate::model::FriendGroup {
            id: "okul".into(),
            name: "Okul".into(),
        });
        groups
            .members
            .insert("223456789012345678".into(), "okul".into());
        assert_eq!(session.put_friend_groups(&groups).await.unwrap(), 4);

        // Malformed codes and ids never reach the service.
        let before = server.requests().len();
        assert!(session.join_team("PRJ-ABCD-EFGH", false).await.is_err());
        assert!(session.leave_team("../me").await.is_err());
        assert!(
            session
                .invite_to_team("0123456789abcdef", "x")
                .await
                .is_err()
        );
        assert_eq!(server.requests().len(), before);
    }

    #[tokio::test]
    async fn several_crews_ask_for_one_more_and_keep_their_order_and_mutes() {
        let server = MockServer::start(|req| match (req.method.as_str(), req.path.as_str()) {
            ("POST", "/v1/teams") => {
                assert_eq!(req.json()["multi"], true);
                MockReply::json(201, TEAM)
            }
            ("POST", "/v1/teams/lookup") => {
                assert_eq!(req.json()["multi"], true);
                MockReply::json(
                    200,
                    r#"{"team":{"id":"fedcba9876543210","name":"PvP","tag":"PVP","emblem":"bolt",
                        "members":4},"relation":"limit"}"#,
                )
            }
            ("POST", "/v1/teams/join") => {
                assert_eq!(req.json()["multi"], true);
                MockReply::json(
                    422,
                    r#"{"error":{"code":"limit_crews","message":"You are in as many crews as you may be"}}"#,
                )
            }
            ("POST", "/v1/teams/invites/fedcba9876543210/accept") => {
                assert_eq!(req.body_text(), r#"{"multi":true}"#);
                MockReply::json(200, TEAM)
            }
            ("PUT", "/v1/teams/order") => {
                let body = req.json();
                if body["order"][0] == "0123456789abcdef" {
                    MockReply::json(200, &req.body_text().to_owned())
                } else {
                    MockReply::json(
                        409,
                        r#"{"error":{"code":"crews_changed","message":"Your crews changed meanwhile"}}"#,
                    )
                }
            }
            ("PATCH", "/v1/teams/0123456789abcdef/notifications") => {
                assert_eq!(req.body_text(), r#"{"muteUntil":1792600000,"muted":true}"#);
                MockReply::json(200, r#"{"muted":true,"muteUntil":1792600000}"#)
            }
            _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
        })
        .await;
        let session = client(&server).session(token());
        session
            .create_team("Gece Kartalları", "GKR", "sword", None, true)
            .await
            .unwrap();
        let lookup = session.lookup_team("EKP-ABCD-EFGH", true).await.unwrap();
        assert_eq!(lookup.relation, crate::model::TeamRelation::Limit);
        assert_eq!(
            session
                .join_team("EKP-ABCD-EFGH", true)
                .await
                .unwrap_err()
                .code(),
            Some(&ErrorCode::LimitCrews)
        );
        assert!(
            session
                .accept_team_invite("fedcba9876543210", true)
                .await
                .unwrap()
                .team
                .is_some()
        );
        let order = vec!["0123456789abcdef".to_owned(), "fedcba9876543210".to_owned()];
        assert_eq!(session.order_crews(&order).await.unwrap(), order);
        let stale = vec!["fedcba9876543210".to_owned()];
        assert_eq!(
            session.order_crews(&stale).await.unwrap_err().code(),
            Some(&ErrorCode::CrewsChanged)
        );
        let mute = session
            .set_crew_mute(
                "0123456789abcdef",
                &CrewMuteEdit {
                    muted: true,
                    mute_until: Some(1_792_600_000),
                },
            )
            .await
            .unwrap();
        assert!(mute.muted_at(1_792_599_999) && !mute.muted_at(1_792_600_000));
        // Ids that are not crews never reach the service.
        let before = server.requests().len();
        assert!(session.order_crews(&["../me".to_owned()]).await.is_err());
        assert!(
            session
                .set_crew_mute(
                    "x",
                    &CrewMuteEdit {
                        muted: false,
                        mute_until: None
                    }
                )
                .await
                .is_err()
        );
        assert_eq!(server.requests().len(), before);
    }

    #[tokio::test]
    async fn joining_by_request_or_link_and_answering_requests() {
        let summary = r#"{"id":"fedcba9876543210","name":"PvP","tag":"PVP","emblem":"bolt",
            "members":12,"join":"approval"}"#;
        let requested = format!(r#"{{"request":{{"team":{summary},"sentAt":1792519000}}}}"#);
        let lookup =
            format!(r#"{{"team":{summary},"relation":"none","request":{{"sentAt":1792519000}}}}"#);
        let server = MockServer::start(move |req| match (req.method.as_str(), req.path.as_str()) {
            ("POST", "/v1/teams/lookup") => {
                let body = req.json();
                // The link goes as its 12 symbols, never with the code.
                assert_eq!(body["link"], "ABCD-EFGH-JKMN");
                assert!(body.get("code").is_none());
                MockReply::json(200, &lookup)
            }
            ("POST", "/v1/teams/join") => {
                let body = req.json();
                assert_eq!(body["request"], true);
                assert_eq!(body["multi"], true);
                if body.get("link").is_some() {
                    MockReply::json(202, &requested)
                } else {
                    assert_eq!(body["code"], "ABCD-EFGH");
                    MockReply::json(200, TEAM)
                }
            }
            ("DELETE", "/v1/teams/requests/fedcba9876543210") => MockReply::empty(204),
            ("DELETE", "/v1/teams/requests/0123456789abcdef") => MockReply::json(
                404,
                r#"{"error":{"code":"team_request_not_found","message":"x"}}"#,
            ),
            ("POST", "/v1/teams/0123456789abcdef/requests/223456789012345678/accept") => {
                MockReply::json(200, TEAM)
            }
            ("POST", "/v1/teams/0123456789abcdef/requests/323456789012345678/accept") => {
                MockReply::json(
                    422,
                    r#"{"error":{"code":"limit_team_members","message":"x"}}"#,
                )
            }
            ("POST", "/v1/teams/0123456789abcdef/requests/223456789012345678/decline")
            | ("DELETE", "/v1/teams/0123456789abcdef/link") => MockReply::empty(204),
            ("POST", "/v1/teams/0123456789abcdef/link") => MockReply::json(
                200,
                r#"{"link":"ABCD-EFGH-JKMN","url":"https://social.projects.gg/e/ABCD-EFGH-JKMN"}"#,
            ),
            _ => MockReply::json(404, r#"{"error":{"code":"not_found","message":"x"}}"#),
        })
        .await;
        let session = client(&server).session(token());
        let link = CrewKey::Link("abcd efgh jkmn".into());
        let found = session.lookup_crew(&link, true).await.unwrap();
        assert_eq!(found.team.join, crate::model::JoinMode::Approval);
        assert_eq!(found.request.map(|r| r.sent_at), Some(1_792_519_000));
        match session.join_crew(&link, true, true).await.unwrap() {
            JoinOutcome::Requested(request) => {
                assert_eq!(request.team.name, "PvP");
                assert_eq!(request.sent_at, 1_792_519_000);
            }
            other => panic!("unexpected {other:?}"),
        }
        let code = CrewKey::Code("EKP-ABCD-EFGH".into());
        assert!(matches!(
            session.join_crew(&code, true, true).await.unwrap(),
            JoinOutcome::Joined(TeamAnswer { team: Some(_), .. })
        ));
        session
            .withdraw_team_request("fedcba9876543210")
            .await
            .unwrap();
        assert_eq!(
            session
                .withdraw_team_request("0123456789abcdef")
                .await
                .unwrap_err()
                .code(),
            Some(&ErrorCode::TeamRequestNotFound)
        );
        assert!(
            session
                .accept_team_request("0123456789abcdef", "223456789012345678")
                .await
                .unwrap()
                .team
                .is_some()
        );
        assert_eq!(
            session
                .accept_team_request("0123456789abcdef", "323456789012345678")
                .await
                .unwrap_err()
                .code(),
            Some(&ErrorCode::LimitTeamMembers)
        );
        session
            .decline_team_request("0123456789abcdef", "223456789012345678")
            .await
            .unwrap();
        let made = session.create_team_link("0123456789abcdef").await.unwrap();
        assert_eq!(made.link, "ABCD-EFGH-JKMN");
        assert!(made.url.ends_with("/e/ABCD-EFGH-JKMN"));
        session.delete_team_link("0123456789abcdef").await.unwrap();

        // Malformed links and ids never reach the service.
        let before = server.requests().len();
        for broken in ["ABCD-EFGH", "PRJ-ABCD-EFGH", "nope"] {
            let key = CrewKey::Link(broken.into());
            assert!(matches!(
                session.lookup_crew(&key, false).await,
                Err(SocialError::InvalidInput(_))
            ));
        }
        assert!(session.withdraw_team_request("../x").await.is_err());
        assert!(
            session
                .accept_team_request("0123456789abcdef", "x")
                .await
                .is_err()
        );
        assert!(session.create_team_link("x").await.is_err());
        assert_eq!(server.requests().len(), before);
    }

    #[tokio::test]
    async fn settings_round_trip_and_conflicts() {
        let server = MockServer::start(|req| match (req.method.as_str(), req.header("if-match")) {
            ("GET", _) => MockReply::json(
                200,
                r#"{"version":4,"updatedAt":1760000000,"device":"Laptop","data":{"theme":"dark"}}"#,
            ),
            ("PUT", Some("\"4\"")) => MockReply::json(
                200,
                r#"{"version":5,"updatedAt":1760000001,"device":"PC","size":17}"#,
            )
            .with_header("ETag", "\"5\""),
            ("PUT", _) => MockReply::json(
                412,
                r#"{"error":{"code":"precondition_failed","message":"x","current":{"version":4,"updatedAt":1,"device":"Laptop","size":17}}}"#,
            ),
            _ => MockReply::json(404, "{}"),
        })
        .await;
        let session = client(&server).session(token());
        let backup = session.settings().await.unwrap().unwrap();
        assert_eq!(backup.data["theme"], "dark");
        let meta = session
            .put_settings(
                "PC",
                &serde_json::json!({"theme": "light"}),
                SettingsPrecondition::Replace(4),
            )
            .await
            .unwrap();
        assert_eq!(meta.version, 5);
        match session
            .put_settings(
                "PC",
                &serde_json::json!({"theme": "light"}),
                SettingsPrecondition::Create,
            )
            .await
        {
            Err(SocialError::SettingsConflict {
                current: Some(meta),
            }) => {
                assert_eq!(meta.device, "Laptop");
            }
            other => panic!("unexpected {other:?}"),
        }
        let put = server
            .requests()
            .into_iter()
            .find(|r| r.method == "PUT")
            .unwrap();
        assert_eq!(put.json()["device"], "PC");
        let too_big = serde_json::json!({ "x": "y".repeat(70_000) });
        assert!(matches!(
            session
                .put_settings("PC", &too_big, SettingsPrecondition::Create)
                .await,
            Err(SocialError::InvalidInput(_))
        ));
    }

    #[tokio::test]
    async fn missing_settings_is_none() {
        let server = MockServer::start(|_| {
            MockReply::json(
                404,
                r#"{"error":{"code":"settings_not_found","message":"No settings backup"}}"#,
            )
        })
        .await;
        assert!(
            client(&server)
                .session(token())
                .settings()
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn retries_a_get_once_on_502_but_not_a_post() {
        let server = MockServer::start(|req| {
            if req.path == "/v1/health" {
                MockReply::json(502, "bad gateway")
            } else {
                MockReply::json(503, "")
            }
        })
        .await;
        let client = client(&server);
        assert!(matches!(
            client.health().await,
            Err(SocialError::Unavailable { .. })
        ));
        assert_eq!(server.requests().len(), 2);
        let attempt = client.begin_login();
        assert!(matches!(
            client.complete_login(&attempt, &sign_in_code(), "PC").await,
            Err(SocialError::Unavailable { .. })
        ));
        assert_eq!(server.requests().len(), 3);
    }

    #[tokio::test]
    async fn unreachable_service_is_a_network_error() {
        let client = SocialClient::new(SocialConfig::new(
            Url::parse("http://127.0.0.1:9/").unwrap(),
            "ProjectsLauncher/test",
        ))
        .unwrap();
        let error = client.health().await.unwrap_err();
        assert!(matches!(error, SocialError::Network(_)));
        assert!(error.is_transient());
    }

    #[test]
    fn urls() {
        let client = SocialClient::new(SocialConfig::production("ProjectsLauncher/test")).unwrap();
        assert_eq!(
            client.presence_url().as_str(),
            "wss://social.projects.gg/v1/presence?caps=callServer,crews,plans,badges,mc,afk,marks,profile"
        );
        assert_eq!(
            client.friend_share_link("abcdefgh").unwrap().as_str(),
            "https://social.projects.gg/f/PRJ-ABCD-EFGH"
        );
        assert_eq!(
            client.crew_link_url("abcd-efgh-jkmn").unwrap().as_str(),
            "https://social.projects.gg/e/ABCD-EFGH-JKMN"
        );
        let local = SocialClient::new(SocialConfig::new(
            Url::parse("http://127.0.0.1:8797").unwrap(),
            "ProjectsLauncher/test",
        ))
        .unwrap();
        assert_eq!(
            local.presence_url().as_str(),
            "ws://127.0.0.1:8797/v1/presence?caps=callServer,crews,plans,badges,mc,afk,marks,profile"
        );
    }
}
