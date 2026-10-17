//! Microsoft sign-in (browser with PKCE + loopback, or device code) and the
//! Microsoft → Xbox Live → XSTS → Minecraft Services chain.

use std::fmt::{self, Write as _};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jiff::{SignedDuration, Timestamp};
use pl_net::{CancellationToken, Http};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::config::AuthConfig;
use crate::error::{AuthError, AuthStep, Result, sanitize};
use crate::http::{FORM_CONTENT_TYPE, Reply, SendOptions, form_body, send};
use crate::loopback::LoopbackListener;
use crate::model::{
    AccountSecrets, Cape, DeviceCodeInfo, LoginStep, MinecraftProfile, Ownership, ProfileSkin,
    SkinVariant,
};
use crate::pkce::{Pkce, authorize_url, random_url_token};

/// Opens a URL in the user's browser. Injected so the crate stays free of UI dependencies.
pub type OpenUrlFn = Arc<dyn Fn(&str) + Send + Sync>;

/// Receives the sign-in steps as they start.
pub type LoginProgressFn = Arc<dyn Fn(LoginStep) + Send + Sync>;

const JSON: &str = "application/json";
const DEFAULT_MC_TOKEN_SECONDS: u64 = 86_400;

/// Result of a successful Microsoft sign-in.
#[derive(Debug, Clone)]
pub struct MicrosoftLogin {
    pub profile: MinecraftProfile,
    pub secrets: AccountSecrets,
    pub ownership: Ownership,
}

/// Result of a token refresh.
#[derive(Debug, Clone)]
pub struct RefreshedSession {
    pub secrets: AccountSecrets,
    /// The current profile when it could be fetched (best effort; names and skins change).
    pub profile: Option<MinecraftProfile>,
}

/// Stateless client for the Microsoft / Xbox / Minecraft Services APIs. Cheap to clone.
#[derive(Clone)]
pub struct MicrosoftAuth {
    http: Http,
    config: Arc<AuthConfig>,
}

impl fmt::Debug for MicrosoftAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MicrosoftAuth")
            .field("client_id", &self.config.client_id)
            .field("endpoints", &self.config.endpoints)
            .finish_non_exhaustive()
    }
}

struct MsToken {
    access_token: String,
    refresh_token: Option<String>,
}

struct XboxToken {
    token: String,
    user_hash: String,
    xuid: Option<String>,
}

struct McToken {
    access_token: String,
    expires_at: Timestamp,
}

impl MicrosoftAuth {
    pub fn new(http: Http) -> Self {
        Self::with_config(http, AuthConfig::default())
    }

    pub fn with_config(http: Http, config: AuthConfig) -> Self {
        Self {
            http,
            config: Arc::new(config),
        }
    }

    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    pub(crate) fn client(&self) -> &reqwest::Client {
        self.http.client()
    }

    pub(crate) fn send_options(&self) -> SendOptions {
        SendOptions {
            timeout: self.config.request_timeout,
            connect_retries: self.config.connect_retries,
        }
    }

    // --- browser sign-in -------------------------------------------------------------------

    /// Full browser sign-in: loopback listener, PKCE, `open_url(authorize URL)`, wait for the
    /// redirect (5 minutes by default), then the token chain. `progress` receives every step.
    pub async fn login_with_browser(
        &self,
        open_url: OpenUrlFn,
        progress: LoginProgressFn,
        cancel: &CancellationToken,
    ) -> Result<MicrosoftLogin> {
        progress(LoginStep::OpeningBrowser);
        let listener = LoopbackListener::bind()
            .await
            .map_err(|error| AuthError::Loopback {
                message: error.to_string(),
                at: Timestamp::now(),
            })?;
        let redirect_uri = listener.redirect_uri();
        let pkce = Pkce::generate();
        let state = random_url_token(16);
        let url = authorize_url(
            &self.config.endpoints.authorize,
            &self.config.client_id,
            &self.config.scope,
            &redirect_uri,
            &state,
            &pkce.challenge,
        )
        .map_err(|error| {
            AuthError::unexpected(
                AuthStep::Browser,
                format!("bad authorize endpoint: {error}"),
            )
        })?;
        tracing::info!(
            port = listener.port(),
            "Microsoft sign-in: opening the browser"
        );
        open_url(url.as_str());

        progress(LoginStep::WaitingForBrowser);
        let code = listener
            .wait_for_code(&state, self.config.browser_timeout, cancel)
            .await?;

        progress(LoginStep::Microsoft);
        let token = self
            .redeem(
                AuthStep::Microsoft,
                &[
                    ("client_id", &self.config.client_id),
                    ("grant_type", "authorization_code"),
                    ("code", &code),
                    ("redirect_uri", &redirect_uri),
                    ("code_verifier", &pkce.verifier),
                    ("scope", &self.config.scope),
                ],
                cancel,
            )
            .await?;
        self.complete_chain(token, &progress, cancel).await
    }

    // --- device code sign-in ---------------------------------------------------------------

    /// Starts the device-code sign-in (for when the browser redirect cannot work). Show
    /// [`DeviceCodeLogin::info`] to the player, then call [`DeviceCodeLogin::complete`].
    pub async fn start_device_code(&self, cancel: &CancellationToken) -> Result<DeviceCodeLogin> {
        #[derive(Deserialize)]
        struct DeviceCodeResponse {
            device_code: String,
            user_code: String,
            verification_uri: String,
            expires_in: u64,
            #[serde(default)]
            interval: Option<u64>,
            #[serde(default)]
            message: Option<String>,
        }

        let request = self
            .client()
            .post(&self.config.endpoints.device_code)
            .header(ACCEPT, JSON)
            .header(CONTENT_TYPE, FORM_CONTENT_TYPE)
            .body(form_body(&[
                ("client_id", &self.config.client_id),
                ("scope", &self.config.scope),
            ]));
        let reply = send(request, AuthStep::DeviceCode, self.send_options(), cancel).await?;
        if !reply.is_success() {
            return Err(token_error(AuthStep::DeviceCode, &reply));
        }
        let response: DeviceCodeResponse = reply.json(AuthStep::DeviceCode)?;
        let lifetime = Duration::from_secs(response.expires_in.clamp(30, 3600));
        let info = DeviceCodeInfo {
            user_code: response.user_code,
            verification_uri: response.verification_uri,
            expires_at: add_seconds(Timestamp::now(), lifetime.as_secs()),
            message: response.message,
        };
        Ok(DeviceCodeLogin {
            auth: self.clone(),
            info,
            device_code: response.device_code,
            interval: Duration::from_secs(response.interval.unwrap_or(5).clamp(1, 60)),
            deadline: tokio::time::Instant::now() + lifetime,
        })
    }

    // --- refresh -----------------------------------------------------------------------------

    /// Silent refresh: Microsoft refresh grant → Xbox Live → XSTS → Minecraft. Keeps
    /// `refresh_token` when Microsoft does not rotate it. The profile is fetched best effort.
    pub async fn refresh(
        &self,
        refresh_token: &str,
        cancel: &CancellationToken,
    ) -> Result<RefreshedSession> {
        let token = self
            .redeem(
                AuthStep::Refresh,
                &[
                    ("client_id", &self.config.client_id),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh_token),
                    ("scope", &self.config.scope),
                ],
                cancel,
            )
            .await?;
        let xbox = self.xbox_live(&token.access_token, cancel).await?;
        let xsts = self.xsts(&xbox.token, cancel).await?;
        let minecraft = self.minecraft_login(&xsts, cancel).await?;
        let profile = match self.profile_or_none(&minecraft.access_token, cancel).await {
            Ok(profile) => profile,
            Err(AuthError::Cancelled) => return Err(AuthError::Cancelled),
            Err(error) => {
                tracing::debug!(code = %error.code(), "profile update after refresh skipped");
                None
            }
        };
        let xuid = xsts
            .xuid
            .or(xbox.xuid)
            .or_else(|| xuid_from_jwt(&minecraft.access_token));
        Ok(RefreshedSession {
            secrets: AccountSecrets {
                refresh_token: token
                    .refresh_token
                    .or_else(|| Some(refresh_token.to_owned())),
                access_token: minecraft.access_token,
                expires_at: minecraft.expires_at,
                xuid,
            },
            profile,
        })
    }

    // --- chain -------------------------------------------------------------------------------

    async fn complete_chain(
        &self,
        token: MsToken,
        progress: &LoginProgressFn,
        cancel: &CancellationToken,
    ) -> Result<MicrosoftLogin> {
        progress(LoginStep::XboxLive);
        let xbox = self.xbox_live(&token.access_token, cancel).await?;
        progress(LoginStep::Xsts);
        let xsts = self.xsts(&xbox.token, cancel).await?;
        progress(LoginStep::Minecraft);
        let minecraft = self.minecraft_login(&xsts, cancel).await?;
        let ownership = self.ownership(&minecraft.access_token, cancel).await?;

        progress(LoginStep::Profile);
        let Some(profile) = self
            .profile_or_none(&minecraft.access_token, cancel)
            .await?
        else {
            // No profile: either no license at all, or a license (often Game Pass) whose
            // profile name was never chosen.
            let at = Timestamp::now();
            return Err(if ownership.java_edition {
                AuthError::NoProfile { at }
            } else {
                AuthError::NoLicense { at }
            });
        };
        let xuid = xsts
            .xuid
            .or(xbox.xuid)
            .or_else(|| xuid_from_jwt(&minecraft.access_token));
        let secrets = AccountSecrets {
            refresh_token: token.refresh_token,
            access_token: minecraft.access_token,
            expires_at: minecraft.expires_at,
            xuid,
        };
        if secrets.refresh_token.is_none() {
            tracing::warn!("Microsoft returned no refresh token; the session cannot be renewed");
        }
        progress(LoginStep::Done);
        tracing::info!(
            game_pass = ownership.game_pass,
            entitlements_verified = ownership.verified,
            "Microsoft sign-in complete"
        );
        Ok(MicrosoftLogin {
            profile,
            secrets,
            ownership,
        })
    }

    /// POST to the Microsoft token endpoint.
    async fn redeem(
        &self,
        step: AuthStep,
        form: &[(&str, &str)],
        cancel: &CancellationToken,
    ) -> Result<MsToken> {
        let request = self
            .client()
            .post(&self.config.endpoints.token)
            .header(ACCEPT, JSON)
            .header(CONTENT_TYPE, FORM_CONTENT_TYPE)
            .body(form_body(form));
        let reply = send(request, step, self.send_options(), cancel).await?;
        if !reply.is_success() {
            return Err(token_error(step, &reply));
        }
        parse_ms_token(step, &reply)
    }

    async fn xbox_live(
        &self,
        ms_access_token: &str,
        cancel: &CancellationToken,
    ) -> Result<XboxToken> {
        let body = json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                // "d=" marks a token from the Azure (consumers) endpoint.
                "RpsTicket": format!("d={ms_access_token}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        });
        let request = self
            .client()
            .post(&self.config.endpoints.xbox_user_auth)
            .header(ACCEPT, JSON)
            .json(&body);
        let reply = send(request, AuthStep::XboxLive, self.send_options(), cancel).await?;
        if !reply.is_success() {
            return Err(AuthError::XboxLive {
                status: reply.status,
                detail: reply.snippet(),
                at: Timestamp::now(),
            });
        }
        read_xbox_token(AuthStep::XboxLive, &reply)
    }

    async fn xsts(&self, xbox_token: &str, cancel: &CancellationToken) -> Result<XboxToken> {
        let body = json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbox_token],
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        });
        let request = self
            .client()
            .post(&self.config.endpoints.xsts_authorize)
            .header(ACCEPT, JSON)
            .json(&body);
        let reply = send(request, AuthStep::Xsts, self.send_options(), cancel).await?;
        if !reply.is_success() {
            let xerr = reply
                .json_value()
                .and_then(|value| match value.get("XErr")? {
                    serde_json::Value::Number(number) => number.as_u64(),
                    serde_json::Value::String(text) => text.trim().parse().ok(),
                    _ => None,
                });
            tracing::warn!(status = reply.status, xerr = ?xerr, "XSTS authorization rejected");
            return Err(AuthError::Xsts {
                status: reply.status,
                xerr,
                detail: reply.snippet(),
                at: Timestamp::now(),
            });
        }
        read_xbox_token(AuthStep::Xsts, &reply)
    }

    async fn minecraft_login(
        &self,
        xsts: &XboxToken,
        cancel: &CancellationToken,
    ) -> Result<McToken> {
        #[derive(Deserialize)]
        struct LoginResponse {
            access_token: String,
            #[serde(default)]
            expires_in: Option<u64>,
        }

        let body = json!({
            "identityToken": format!("XBL3.0 x={};{}", xsts.user_hash, xsts.token),
        });
        let request = self
            .client()
            .post(
                self.config
                    .endpoints
                    .minecraft("/authentication/login_with_xbox"),
            )
            .header(ACCEPT, JSON)
            .json(&body);
        let reply = send(request, AuthStep::Minecraft, self.send_options(), cancel).await?;
        if !reply.is_success() {
            // Mojang only lets approved launcher app ids into Minecraft Services. That rejection
            // is not about the player's account, so it gets its own code and message.
            let text = String::from_utf8_lossy(&reply.body).to_ascii_lowercase();
            let app_registration = text.contains("app registration") || text.contains("appreginfo");
            tracing::warn!(
                status = reply.status,
                app_registration,
                "Minecraft login rejected"
            );
            return Err(AuthError::MinecraftLogin {
                status: reply.status,
                app_registration,
                detail: reply.snippet(),
                at: Timestamp::now(),
            });
        }
        let response: LoginResponse = reply.json(AuthStep::Minecraft)?;
        if response.access_token.is_empty() {
            return Err(AuthError::unexpected(
                AuthStep::Minecraft,
                "empty Minecraft access token",
            ));
        }
        let seconds = response
            .expires_in
            .filter(|seconds| *seconds > 0)
            .unwrap_or(DEFAULT_MC_TOKEN_SECONDS);
        Ok(McToken {
            access_token: response.access_token,
            expires_at: add_seconds(Timestamp::now(), seconds),
        })
    }

    /// Entitlement check. Never fails the sign-in on its own (except cancellation): the profile
    /// lookup is the final word, the entitlements only explain a missing profile.
    async fn ownership(&self, access_token: &str, cancel: &CancellationToken) -> Result<Ownership> {
        #[derive(Deserialize)]
        struct Entitlements {
            #[serde(default)]
            items: Vec<EntitlementItem>,
        }

        let url = format!(
            "{}?requestId={}",
            self.config.endpoints.minecraft("/entitlements/license"),
            Uuid::new_v4()
        );
        let request = self
            .client()
            .get(url)
            .bearer_auth(access_token)
            .header(ACCEPT, JSON);
        let reply = match send(request, AuthStep::Entitlements, self.send_options(), cancel).await {
            Ok(reply) => reply,
            Err(AuthError::Cancelled) => return Err(AuthError::Cancelled),
            Err(error) => {
                tracing::warn!(code = %error.code(), "entitlement check failed, continuing");
                return Ok(Ownership::default());
            }
        };
        if !reply.is_success() {
            tracing::warn!(
                status = reply.status,
                "entitlement check failed, continuing"
            );
            return Ok(Ownership::default());
        }
        match reply.json::<Entitlements>(AuthStep::Entitlements) {
            Ok(entitlements) => Ok(ownership_from(&entitlements.items)),
            Err(error) => {
                tracing::warn!(code = %error.code(), "unreadable entitlements, continuing");
                Ok(Ownership::default())
            }
        }
    }

    /// The Java Edition profile; `None` when the account has none (HTTP 404).
    pub(crate) async fn profile_or_none(
        &self,
        access_token: &str,
        cancel: &CancellationToken,
    ) -> Result<Option<MinecraftProfile>> {
        let request = self
            .client()
            .get(self.config.endpoints.minecraft("/minecraft/profile"))
            .bearer_auth(access_token)
            .header(ACCEPT, JSON);
        let reply = send(request, AuthStep::Profile, self.send_options(), cancel).await?;
        if reply.status == 404 {
            return Ok(None);
        }
        if !reply.is_success() {
            return Err(AuthError::MinecraftApi {
                step: AuthStep::Profile,
                status: reply.status,
                detail: reply.snippet(),
                at: Timestamp::now(),
            });
        }
        let response: ProfileResponse = reply.json(AuthStep::Profile)?;
        response.into_profile(AuthStep::Profile).map(Some)
    }

    /// The Java Edition profile with skins and capes.
    pub async fn fetch_profile(
        &self,
        access_token: &str,
        cancel: &CancellationToken,
    ) -> Result<MinecraftProfile> {
        self.profile_or_none(access_token, cancel)
            .await?
            .ok_or(AuthError::NoProfile {
                at: Timestamp::now(),
            })
    }
}

/// A device-code sign-in in progress. `Debug` hides the device code.
pub struct DeviceCodeLogin {
    auth: MicrosoftAuth,
    info: DeviceCodeInfo,
    device_code: String,
    interval: Duration,
    deadline: tokio::time::Instant,
}

impl fmt::Debug for DeviceCodeLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceCodeLogin")
            .field("info", &self.info)
            .field("interval", &self.interval)
            .finish_non_exhaustive()
    }
}

impl DeviceCodeLogin {
    /// What to show the player: the code and the page to type it into.
    pub fn info(&self) -> &DeviceCodeInfo {
        &self.info
    }

    /// Polls until the player approves (or declines, or the code expires), then runs the token
    /// chain. Steps: `WaitingForBrowser`, `Microsoft`, … `Done`.
    pub async fn complete(
        self,
        progress: LoginProgressFn,
        cancel: &CancellationToken,
    ) -> Result<MicrosoftLogin> {
        progress(LoginStep::WaitingForBrowser);
        let config = self.auth.config();
        let mut interval = self.interval;
        let token = loop {
            let wake = tokio::time::Instant::now() + interval;
            if wake >= self.deadline {
                return Err(AuthError::DeviceCodeExpired {
                    at: Timestamp::now(),
                });
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => return Err(AuthError::Cancelled),
                () = tokio::time::sleep_until(wake) => {}
            }
            let request = self
                .auth
                .client()
                .post(&config.endpoints.token)
                .header(ACCEPT, JSON)
                .header(CONTENT_TYPE, FORM_CONTENT_TYPE)
                .body(form_body(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", &config.client_id),
                    ("device_code", &self.device_code),
                ]));
            let reply = match send(
                request,
                AuthStep::DeviceCode,
                self.auth.send_options(),
                cancel,
            )
            .await
            {
                Ok(reply) => reply,
                // A blip while the player types the code must not end a 15-minute wait.
                Err(AuthError::Network { .. }) => continue,
                Err(error) => return Err(error),
            };
            if reply.is_success() {
                break parse_ms_token(AuthStep::DeviceCode, &reply)?;
            }
            let body: OAuthErrorBody = serde_json::from_slice(&reply.body).unwrap_or_default();
            match body.error.as_deref() {
                Some("authorization_pending") => {}
                Some("slow_down") => interval += Duration::from_secs(5),
                Some("authorization_declined") => {
                    return Err(AuthError::DeviceCodeDeclined {
                        at: Timestamp::now(),
                    });
                }
                Some("expired_token" | "code_expired") => {
                    return Err(AuthError::DeviceCodeExpired {
                        at: Timestamp::now(),
                    });
                }
                _ => return Err(token_error(AuthStep::DeviceCode, &reply)),
            }
        };
        progress(LoginStep::Microsoft);
        self.auth.complete_chain(token, &progress, cancel).await
    }
}

// --- response parsing ------------------------------------------------------------------------

#[derive(Default, Deserialize)]
struct OAuthErrorBody {
    error: Option<String>,
    error_description: Option<String>,
    error_codes: Option<Vec<i64>>,
    trace_id: Option<String>,
    correlation_id: Option<String>,
}

fn token_error(step: AuthStep, reply: &Reply) -> AuthError {
    let body: OAuthErrorBody = serde_json::from_slice(&reply.body).unwrap_or_default();
    let mut detail = String::new();
    if let Some(codes) = &body.error_codes {
        let _ = write!(detail, "codes={codes:?} ");
    }
    if let Some(trace_id) = &body.trace_id {
        let _ = write!(detail, "trace_id={trace_id} ");
    }
    if let Some(correlation_id) = &body.correlation_id {
        let _ = write!(detail, "correlation_id={correlation_id} ");
    }
    if let Some(description) = &body.error_description {
        detail.push_str(description.lines().next().unwrap_or_default());
    }
    if detail.trim().is_empty() {
        detail = reply.snippet();
    }
    tracing::warn!(%step, status = reply.status, error = ?body.error, "Microsoft token request rejected");
    AuthError::MicrosoftToken {
        step,
        status: reply.status,
        error: body.error,
        detail: sanitize(&detail),
        at: Timestamp::now(),
    }
}

fn parse_ms_token(step: AuthStep, reply: &Reply) -> Result<MsToken> {
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
    }

    let response: TokenResponse = reply.json(step)?;
    if response.access_token.is_empty() {
        return Err(AuthError::unexpected(step, "empty Microsoft access token"));
    }
    Ok(MsToken {
        access_token: response.access_token,
        refresh_token: response.refresh_token.filter(|token| !token.is_empty()),
    })
}

fn read_xbox_token(step: AuthStep, reply: &Reply) -> Result<XboxToken> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct XboxResponse {
        token: String,
        display_claims: DisplayClaims,
    }
    #[derive(Deserialize)]
    struct DisplayClaims {
        #[serde(default)]
        xui: Vec<UserClaims>,
    }
    #[derive(Deserialize)]
    struct UserClaims {
        uhs: Option<String>,
        xid: Option<String>,
    }

    let response: XboxResponse = reply.json(step)?;
    let claims = response.display_claims.xui.into_iter().next();
    let (user_hash, xuid) = match claims {
        Some(claims) => (claims.uhs.unwrap_or_default(), claims.xid),
        None => (String::new(), None),
    };
    if response.token.is_empty() || user_hash.is_empty() {
        return Err(AuthError::unexpected(step, "Token or uhs missing"));
    }
    Ok(XboxToken {
        token: response.token,
        user_hash,
        xuid: xuid.filter(|xuid| !xuid.is_empty()),
    })
}

#[derive(Deserialize)]
struct EntitlementItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    source: Option<String>,
}

fn ownership_from(items: &[EntitlementItem]) -> Ownership {
    let game_pass = items.iter().any(|item| {
        item.name.contains("game_pass")
            || item
                .source
                .as_deref()
                .is_some_and(|source| source.to_ascii_uppercase().contains("GAMEPASS"))
    });
    let java_edition = game_pass
        || items
            .iter()
            .any(|item| matches!(item.name.as_str(), "product_minecraft" | "game_minecraft"));
    Ownership {
        java_edition,
        game_pass,
        verified: true,
    }
}

/// Minecraft Services profile JSON.
#[derive(Deserialize)]
pub(crate) struct ProfileResponse {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<SkinEntry>,
    #[serde(default)]
    capes: Vec<CapeEntry>,
}

#[derive(Deserialize)]
struct SkinEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    state: String,
    url: String,
    #[serde(default)]
    variant: Option<String>,
    #[serde(default)]
    alias: Option<String>,
}

#[derive(Deserialize)]
struct CapeEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    state: String,
    url: String,
    #[serde(default)]
    alias: Option<String>,
}

impl ProfileResponse {
    pub(crate) fn into_profile(self, step: AuthStep) -> Result<MinecraftProfile> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|_| AuthError::unexpected(step, "invalid profile id"))?;
        if self.name.is_empty() {
            return Err(AuthError::unexpected(step, "empty profile name"));
        }
        Ok(MinecraftProfile {
            id,
            name: self.name,
            skins: self
                .skins
                .into_iter()
                .map(|skin| ProfileSkin {
                    id: skin.id,
                    url: secure_texture_url(skin.url),
                    variant: SkinVariant::from_api(skin.variant.as_deref().unwrap_or_default()),
                    active: skin.state.eq_ignore_ascii_case("ACTIVE"),
                    alias: skin.alias,
                })
                .collect(),
            capes: self
                .capes
                .into_iter()
                .map(|cape| Cape {
                    name: cape.alias.unwrap_or_else(|| cape.id.clone()),
                    id: cape.id,
                    url: secure_texture_url(cape.url),
                    active: cape.state.eq_ignore_ascii_case("ACTIVE"),
                })
                .collect(),
        })
    }
}

/// Mojang still hands out `http://textures.minecraft.net/…`; the host serves HTTPS too.
pub(crate) fn secure_texture_url(url: String) -> String {
    match url.strip_prefix("http://textures.minecraft.net/") {
        Some(rest) => format!("https://textures.minecraft.net/{rest}"),
        None => url,
    }
}

/// The `xuid` claim of the Minecraft access token (a JWT). XSTS for Minecraft Services does not
/// return the `xid` claim, the token does.
fn xuid_from_jwt(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    match claims.get("xuid")? {
        serde_json::Value::String(xuid) if !xuid.is_empty() => Some(xuid.clone()),
        serde_json::Value::Number(xuid) => Some(xuid.to_string()),
        _ => None,
    }
}

fn add_seconds(timestamp: Timestamp, seconds: u64) -> Timestamp {
    let seconds = i64::try_from(seconds).unwrap_or(i64::MAX);
    timestamp
        .checked_add(SignedDuration::from_secs(seconds))
        .unwrap_or(Timestamp::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use parking_lot::Mutex;

    use super::*;
    use crate::config::Endpoints;
    use crate::pkce::code_challenge;
    use crate::testutil::{self, MockReply, MockServer, Recorded};

    fn auth_for(server: &MockServer) -> MicrosoftAuth {
        MicrosoftAuth::with_config(testutil::http(), testutil::config(server))
    }

    fn steps_recorder() -> (LoginProgressFn, Arc<Mutex<Vec<LoginStep>>>) {
        let steps = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&steps);
        (Arc::new(move |step| sink.lock().push(step)), steps)
    }

    /// Plays the browser: follows the authorize URL's redirect_uri with a code and the state.
    fn fake_browser(code: &'static str, seen_url: Arc<Mutex<Option<String>>>) -> OpenUrlFn {
        Arc::new(move |url: &str| {
            *seen_url.lock() = Some(url.to_owned());
            let parsed = url::Url::parse(url).unwrap();
            let query: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
            let redirect = url::Url::parse(&query["redirect_uri"]).unwrap();
            let port = redirect.port().unwrap();
            let state = query["state"].clone();
            tokio::spawn(async move {
                let page =
                    testutil::raw_get(("127.0.0.1", port), &format!("/?code={code}&state={state}"))
                        .await;
                assert!(page.starts_with("HTTP/1.1 200 OK"), "{page}");
            });
        })
    }

    #[tokio::test]
    async fn browser_login_runs_the_whole_chain() {
        let server = MockServer::start(testutil::happy_chain).await;
        let auth = auth_for(&server);
        let (progress, steps) = steps_recorder();
        let seen_url = Arc::new(Mutex::new(None));
        let login = auth
            .login_with_browser(
                fake_browser("auth-code-1", Arc::clone(&seen_url)),
                progress,
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(login.profile.name, "Steve");
        assert_eq!(login.profile.id, testutil::profile_uuid());
        assert_eq!(login.profile.capes.len(), 1);
        assert!(login.profile.capes[0].active);
        assert_eq!(
            login.profile.active_skin().unwrap().url,
            "https://textures.minecraft.net/texture/abc"
        );
        assert_eq!(
            login.secrets.refresh_token.as_deref(),
            Some("ms-refresh-new")
        );
        assert_eq!(login.secrets.access_token, testutil::mc_token());
        assert_eq!(login.secrets.xuid.as_deref(), Some("2535400000000001"));
        assert!(login.secrets.expires_at > Timestamp::now());
        assert!(login.ownership.java_edition && login.ownership.verified);
        assert!(!login.ownership.game_pass);
        assert_eq!(
            *steps.lock(),
            vec![
                LoginStep::OpeningBrowser,
                LoginStep::WaitingForBrowser,
                LoginStep::Microsoft,
                LoginStep::XboxLive,
                LoginStep::Xsts,
                LoginStep::Minecraft,
                LoginStep::Profile,
                LoginStep::Done,
            ]
        );

        // The token request proves possession of the PKCE verifier behind the challenge.
        let authorize = url::Url::parse(seen_url.lock().as_deref().unwrap()).unwrap();
        let authorize: HashMap<String, String> = authorize.query_pairs().into_owned().collect();
        let token_request = server.only("/consumers/oauth2/v2.0/token");
        let form = token_request.form();
        assert_eq!(form["grant_type"], "authorization_code");
        assert_eq!(form["code"], "auth-code-1");
        assert_eq!(form["redirect_uri"], authorize["redirect_uri"]);
        assert_eq!(
            code_challenge(&form["code_verifier"]),
            authorize["code_challenge"]
        );
        assert_eq!(form["client_id"], crate::config::CLIENT_ID);

        let xbox = server.only("/user/authenticate").json();
        assert_eq!(xbox["Properties"]["RpsTicket"], "d=ms-access");
        let xsts = server.only("/xsts/authorize").json();
        assert_eq!(xsts["RelyingParty"], "rp://api.minecraftservices.com/");
        assert_eq!(xsts["Properties"]["UserTokens"][0], "xbl-token");
        let login_request = server.only("/authentication/login_with_xbox").json();
        assert_eq!(
            login_request["identityToken"],
            "XBL3.0 x=user-hash;xsts-token"
        );
        let entitlements = server.only("/entitlements/license");
        assert!(entitlements.query.starts_with("requestId="));
        assert_eq!(
            entitlements.header("authorization"),
            Some(format!("Bearer {}", testutil::mc_token()).as_str())
        );
    }

    #[tokio::test]
    async fn xsts_errors_are_mapped() {
        let server = MockServer::start(|request: &Recorded| {
            if request.path == "/xsts/authorize" {
                MockReply::json(
                    401,
                    json!({"Identity": "0", "XErr": 2_148_916_238_u64, "Message": "",
                           "Redirect": "https://start.ui.xboxlive.com/AddChildToFamily"}),
                )
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        let error = auth_for(&server)
            .refresh("old-refresh", &CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(error.code(), "XSTS-2148916238");
        assert!(error.user_message_tr().contains("çocuk hesabı"));
        assert!(error.details().contains("HTTP: 401"));
    }

    #[tokio::test]
    async fn app_registration_rejection_is_recognised() {
        let server = MockServer::start(|request: &Recorded| {
            if request.path == "/authentication/login_with_xbox" {
                MockReply::json(
                    403,
                    json!({"path": "/authentication/login_with_xbox", "errorMessage":
                        "Invalid app registration, see https://aka.ms/AppRegInfo for more information"}),
                )
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        let error = auth_for(&server)
            .refresh("old-refresh", &CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(error.code(), "MC-APPREG");
        assert!(error.user_message_tr().contains("hesabında değil"));
    }

    async fn login_with_profile_404(entitlements: serde_json::Value) -> AuthError {
        let server = MockServer::start(move |request: &Recorded| match request.path.as_str() {
            "/minecraft/profile" => MockReply::json(404, json!({"error": "NOT_FOUND"})),
            "/entitlements/license" => MockReply::json(200, entitlements.clone()),
            _ => testutil::happy_chain(request),
        })
        .await;
        let (progress, _) = steps_recorder();
        auth_for(&server)
            .login_with_browser(
                fake_browser("code", Arc::new(Mutex::new(None))),
                progress,
                &CancellationToken::new(),
            )
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn missing_profile_is_explained_by_entitlements() {
        let none = login_with_profile_404(json!({"items": []})).await;
        assert_eq!(none.code(), "MC-LISANSYOK");
        let owned = login_with_profile_404(json!({"items": [
            {"name": "product_minecraft", "source": "PURCHASE"},
            {"name": "game_minecraft", "source": "PURCHASE"}
        ]}))
        .await;
        assert_eq!(owned.code(), "MC-PROFILYOK");
        let game_pass = login_with_profile_404(json!({"items": [
            {"name": "product_game_pass_pc", "source": "GAMEPASS"}
        ]}))
        .await;
        assert_eq!(game_pass.code(), "MC-PROFILYOK");
        assert!(game_pass.user_message_tr().contains("oyuncu profili"));
    }

    #[test]
    fn game_pass_entitlements() {
        let items = vec![EntitlementItem {
            name: "product_minecraft".into(),
            source: Some("GAMEPASS".into()),
        }];
        let ownership = ownership_from(&items);
        assert!(ownership.java_edition && ownership.game_pass && ownership.verified);
        assert!(!ownership_from(&[]).java_edition);
    }

    #[tokio::test]
    async fn device_code_flow_polls_until_approved() {
        let polls = Arc::new(Mutex::new(0u32));
        let counter = Arc::clone(&polls);
        let server = MockServer::start(move |request: &Recorded| match request.path.as_str() {
            "/consumers/oauth2/v2.0/devicecode" => MockReply::json(
                200,
                json!({"device_code": "dev-code", "user_code": "ABCD-1234",
                           "verification_uri": "https://www.microsoft.com/link",
                           "expires_in": 900, "interval": 1,
                           "message": "Enter the code"}),
            ),
            "/consumers/oauth2/v2.0/token"
                if request.form()["grant_type"]
                    == "urn:ietf:params:oauth:grant-type:device_code" =>
            {
                assert_eq!(request.form()["device_code"], "dev-code");
                let mut polls = counter.lock();
                *polls += 1;
                if *polls == 1 {
                    MockReply::json(400, json!({"error": "authorization_pending"}))
                } else {
                    testutil::happy_chain(request)
                }
            }
            _ => testutil::happy_chain(request),
        })
        .await;
        let auth = auth_for(&server);
        let cancel = CancellationToken::new();
        let device = auth.start_device_code(&cancel).await.unwrap();
        assert_eq!(device.info().user_code, "ABCD-1234");
        assert_eq!(
            device.info().verification_uri,
            "https://www.microsoft.com/link"
        );
        assert!(!format!("{device:?}").contains("dev-code"));
        let (progress, steps) = steps_recorder();
        let login = device.complete(progress, &cancel).await.unwrap();
        assert_eq!(login.profile.name, "Steve");
        assert_eq!(*polls.lock(), 2);
        assert_eq!(steps.lock().first(), Some(&LoginStep::WaitingForBrowser));
        assert_eq!(steps.lock().last(), Some(&LoginStep::Done));
    }

    #[tokio::test]
    async fn device_code_decline_and_cancel() {
        let server = MockServer::start(|request: &Recorded| match request.path.as_str() {
            "/consumers/oauth2/v2.0/devicecode" => MockReply::json(
                200,
                json!({"device_code": "d", "user_code": "U", "verification_uri": "https://x",
                       "expires_in": 900, "interval": 1}),
            ),
            _ => MockReply::json(400, json!({"error": "authorization_declined"})),
        })
        .await;
        let auth = auth_for(&server);
        let cancel = CancellationToken::new();
        let (progress, _) = steps_recorder();
        let device = auth.start_device_code(&cancel).await.unwrap();
        let error = device
            .complete(Arc::clone(&progress), &cancel)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "MSA-authorization_declined");

        let device = auth.start_device_code(&cancel).await.unwrap();
        cancel.cancel();
        let error = device.complete(progress, &cancel).await.unwrap_err();
        assert!(error.is_cancelled());
    }

    #[test]
    fn xuid_comes_from_the_minecraft_jwt() {
        assert_eq!(
            xuid_from_jwt(&testutil::mc_token()).as_deref(),
            Some("2535400000000001")
        );
        assert_eq!(xuid_from_jwt("not-a-jwt"), None);
    }

    #[test]
    fn texture_urls_are_upgraded() {
        assert_eq!(
            secure_texture_url("http://textures.minecraft.net/texture/x".into()),
            "https://textures.minecraft.net/texture/x"
        );
        assert_eq!(
            secure_texture_url("http://other/x".into()),
            "http://other/x"
        );
    }

    #[test]
    fn production_endpoints() {
        let endpoints = Endpoints::production();
        assert_eq!(
            endpoints.minecraft("/minecraft/profile"),
            "https://api.minecraftservices.com/minecraft/profile"
        );
        assert!(endpoints.token.ends_with("/consumers/oauth2/v2.0/token"));
    }
}
