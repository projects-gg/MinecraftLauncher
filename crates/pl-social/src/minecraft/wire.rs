//! The friends service's `/v1/mc` routes and the `mc` socket message (SKIN1; README "Minecraft
//! link and Projects skin", "Errors"): what they carry, read leniently (a field this version does
//! not know never hides the rest), and every failure with the details the launcher acts on.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use pl_auth::SkinVariant;
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use crate::client::{Reply, Session};
use crate::error::{SocialError, parse_retry_after};

/// `features`: linking a game account from the launcher (`mcLink`).
pub const FEATURE_MC_LINK: &str = "mcLink";
/// `features`: Projects skins (`skins`).
pub const FEATURE_SKINS: &str = "skins";
/// `features`: codeless linking in game (`mcCodeless`).
pub const FEATURE_MC_CODELESS: &str = "mcCodeless";

/// A value that does not read as `T` reads as missing.
pub(crate) fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// The items of a list that read as `T`; the others are left out.
fn readable<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    let values = Option::<Vec<serde_json::Value>>::deserialize(deserializer)?.unwrap_or_default();
    Ok(values
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}

/// A time that may be missing (`None`) or `null` (`Some(None)`).
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<i64>>, D::Error> {
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Some(None),
        serde_json::Value::Number(number) => number.as_i64().map(Some),
        _ => None,
    })
}

/// The arm model; anything but `slim` is classic.
fn variant_of(value: Option<&str>) -> Option<SkinVariant> {
    match value? {
        "slim" => Some(SkinVariant::Slim),
        "classic" => Some(SkinVariant::Classic),
        _ => None,
    }
}

fn lenient_variant<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<SkinVariant>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(variant_of(
        value.as_ref().and_then(serde_json::Value::as_str),
    ))
}

/// What the service knows of the Discord account's link to a game account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McLinkState {
    Linked,
    /// ProBot said there is none.
    None,
    /// Never learned, or learned more than 7 days ago: run a check.
    #[default]
    Unknown,
}

impl<'de> Deserialize<'de> for McLinkState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(
            match Option::<String>::deserialize(deserializer)?.as_deref() {
                Some("linked") => Self::Linked,
                Some("none") => Self::None,
                _ => Self::Unknown,
            },
        )
    }
}

/// `McState.link` (and every `mc` message's): times are Unix seconds.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McLink {
    #[serde(default)]
    pub state: McLinkState,
    /// The linked game account (the last known one while `unknown`).
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub at: Option<i64>,
    /// When the service last learned the link from ProBot; None: never.
    #[serde(default, deserialize_with = "lenient")]
    pub checked_at: Option<i64>,
}

impl McLink {
    /// Linked to this game account (names compare without regard to case).
    pub fn linked_to(&self, name: &str) -> bool {
        self.state == McLinkState::Linked
            && self
                .name
                .as_deref()
                .is_some_and(|linked| linked.eq_ignore_ascii_case(name))
    }
}

/// What a Projects skin is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McSkinKind {
    Png,
    Premium,
    Default,
}

impl McSkinKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "png" => Some(Self::Png),
            "premium" => Some(Self::Premium),
            "default" => Some(Self::Default),
            _ => None,
        }
    }
}

/// Who last changed the Projects skin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McSkinBy {
    Launcher,
    /// Staff reset it in game.
    Staff,
}

/// `McState.skin`: the Projects skin as last changed from a launcher or reset by staff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McSkin {
    pub kind: McSkinKind,
    /// The texture's id on Mojang's texture host (hex); None for `default`.
    pub texture: Option<String>,
    pub variant: SkinVariant,
    /// `premium`: the premium player's name.
    pub premium: Option<String>,
    pub at: i64,
    pub by: McSkinBy,
}

impl<'de> Deserialize<'de> for McSkin {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            kind: String,
            #[serde(default, deserialize_with = "lenient")]
            texture: Option<String>,
            #[serde(default, deserialize_with = "lenient_variant")]
            variant: Option<SkinVariant>,
            #[serde(default, deserialize_with = "lenient")]
            premium: Option<String>,
            #[serde(default, deserialize_with = "lenient")]
            at: Option<i64>,
            #[serde(default, deserialize_with = "lenient")]
            by: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let kind = McSkinKind::parse(&raw.kind)
            .ok_or_else(|| serde::de::Error::custom("unknown skin kind"))?;
        Ok(Self {
            kind,
            texture: raw
                .texture
                .filter(|texture| is_texture_id(texture) && kind != McSkinKind::Default)
                .map(|texture| texture.to_ascii_lowercase()),
            variant: raw.variant.unwrap_or_default(),
            premium: raw.premium.filter(|_| kind == McSkinKind::Premium),
            at: raw.at.unwrap_or_default(),
            by: if raw.by.as_deref() == Some("staff") {
                McSkinBy::Staff
            } else {
                McSkinBy::Launcher
            },
        })
    }
}

/// A texture id on Mojang's texture host: hex, at most 128 characters.
pub(crate) fn is_texture_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McJobKind {
    Link,
    Unlink,
    Check,
    Skin,
    /// A kind this version does not know.
    Other,
}

impl McJobKind {
    /// The link family: one open job among `link`, `unlink` and `check`.
    pub fn is_link_family(self) -> bool {
        matches!(self, Self::Link | Self::Unlink | Self::Check)
    }
}

impl<'de> Deserialize<'de> for McJobKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "link" => Self::Link,
            "unlink" => Self::Unlink,
            "check" => Self::Check,
            "skin" => Self::Skin,
            _ => Self::Other,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McJobState {
    /// Waits for ProBot.
    Pending,
    /// ProBot took it.
    Working,
    /// A skin waits for the game server.
    Queued,
    Done,
    Failed,
    Cancelled,
}

impl McJobState {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Pending | Self::Working | Self::Queued)
    }
}

impl<'de> Deserialize<'de> for McJobState {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "pending" => Self::Pending,
            "working" => Self::Working,
            "queued" => Self::Queued,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            // An end this version does not know is still an end.
            _ => Self::Failed,
        })
    }
}

/// How a closed job ended. `code` is the service's (`applied`, `pin_invalid`, …).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct McJobResult {
    #[serde(default)]
    pub code: String,
    /// `other_account`: the game account the Discord account is linked to.
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    /// `skin_blocked`: until when (Unix seconds; `Some(None)`: for good).
    #[serde(default, deserialize_with = "present")]
    pub until: Option<Option<i64>>,
    /// `applied`: the skin's texture id and arm model.
    #[serde(default, deserialize_with = "lenient")]
    pub texture: Option<String>,
    #[serde(default, deserialize_with = "lenient_variant")]
    pub variant: Option<SkinVariant>,
}

/// One job of the player's (README "Job"). Times are milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McJob {
    pub id: String,
    pub kind: McJobKind,
    pub state: McJobState,
    /// The game account (`link` and `skin`).
    #[serde(default, deserialize_with = "lenient")]
    pub name: Option<String>,
    /// `skin`: `png`, `premium` or `default`.
    #[serde(default, deserialize_with = "lenient")]
    pub source: Option<String>,
    /// Once closed.
    #[serde(default, deserialize_with = "lenient")]
    pub result: Option<McJobResult>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

impl McJob {
    /// The service's result code, `cancelled` for a cancelled job without one.
    pub fn code(&self) -> &str {
        match (&self.result, self.state) {
            (Some(result), _) if !result.code.is_empty() => &result.code,
            (_, McJobState::Cancelled) => "cancelled",
            (_, McJobState::Done | McJobState::Failed) => "error",
            _ => "",
        }
    }

    pub fn applied(&self) -> bool {
        self.kind == McJobKind::Skin && self.state == McJobState::Done && self.code() == "applied"
    }

    /// For this game account (names compare without regard to case).
    pub fn is_for(&self, name: &str) -> bool {
        self.name
            .as_deref()
            .is_some_and(|own| own.eq_ignore_ascii_case(name))
    }
}

/// `McState.health`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct McHealth {
    /// ProBot asked in the last 10 minutes.
    #[serde(default = "yes")]
    pub bot: bool,
    /// Lobi took ProBot's delivery queue in the last 10 minutes.
    #[serde(default = "yes")]
    pub server: bool,
}

fn yes() -> bool {
    true
}

impl Default for McHealth {
    fn default() -> Self {
        Self {
            bot: true,
            server: true,
        }
    }
}

/// `McState.limits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McLimits {
    #[serde(default)]
    pub skins_per_day: u32,
    #[serde(default)]
    pub skins_left: u32,
    /// Unix seconds: when the Turkey day ends.
    #[serde(default)]
    pub reset_at: i64,
    #[serde(default)]
    pub link_tries_left: u32,
}

/// Staff keep the player from changing skins (`until` None: for good).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct McBlocked {
    #[serde(default)]
    pub until: Option<i64>,
}

/// A new Discord account may not change skins until `until` (Unix seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct McLimitedUntil {
    pub until: i64,
}

/// A game account name that answered "Hayır" in game: no permit opens for it before `until`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct McDeclined {
    pub name: String,
    pub until: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
struct McCodeless {
    #[serde(default, deserialize_with = "readable")]
    declined: Vec<McDeclined>,
}

/// `GET /v1/mc`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct McState {
    #[serde(default, deserialize_with = "readable")]
    pub features: Vec<String>,
    #[serde(default)]
    pub health: McHealth,
    #[serde(default)]
    pub link: McLink,
    #[serde(default, deserialize_with = "lenient")]
    pub skin: Option<McSkin>,
    /// Each group's open job and its latest closed one of the last day, newest first.
    #[serde(default, deserialize_with = "readable")]
    pub jobs: Vec<McJob>,
    #[serde(default)]
    pub limits: McLimits,
    #[serde(default, deserialize_with = "lenient")]
    pub blocked: Option<McBlocked>,
    #[serde(default, deserialize_with = "lenient")]
    pub limited: Option<McLimitedUntil>,
    #[serde(default, deserialize_with = "codeless_declined", rename = "codeless")]
    pub declined: Vec<McDeclined>,
}

fn codeless_declined<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<McDeclined>, D::Error> {
    Ok(lenient::<D, McCodeless>(deserializer)?
        .map(|codeless| codeless.declined)
        .unwrap_or_default())
}

impl McState {
    pub fn offers(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }

    /// Puts a job the service answered with in its place: the one with its id, else the newest
    /// of its group (one open and one closed job a group).
    pub fn put_job(&mut self, job: McJob) {
        let family = job.kind.is_link_family();
        let open = job.state.is_open();
        self.jobs.retain(|known| {
            known.id != job.id
                && !(known.kind.is_link_family() == family && known.state.is_open() == open)
        });
        self.jobs.insert(0, job);
    }

    /// What an `mc` message says now: the link, the skin and the block replace these, a closed
    /// job its place.
    pub fn apply(&mut self, message: &McMessage) {
        self.link = message.link.clone();
        self.skin = message.skin.clone();
        self.blocked = message.blocked;
        if let Some(job) = &message.job {
            self.put_job(job.clone());
        }
    }
}

/// `{"t":"mc"}` on the presence socket (with `mc` in `caps`): a job of the player's closed
/// (`job`), or ProBot reported a link change or a staff reset of the skin (no `job`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct McMessage {
    #[serde(default, deserialize_with = "lenient")]
    pub job: Option<McJob>,
    #[serde(default)]
    pub link: McLink,
    #[serde(default, deserialize_with = "lenient")]
    pub skin: Option<McSkin>,
    #[serde(default, deserialize_with = "lenient")]
    pub blocked: Option<McBlocked>,
}

/// `POST /v1/mc/link/check`: a `check` job, or the link the service learned lately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McCheck {
    Job(McJob),
    Link(McLink),
}

/// A codeless linking permit (`POST /v1/mc/intents`): handed only to the overlay mod.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McIntent {
    pub nonce: String,
    /// Unix seconds.
    pub expires_at: i64,
}

impl std::fmt::Debug for McIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McIntent")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// A premium player's skin as the launcher read it from Mojang (`source.premium`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McPremium {
    pub name: String,
    pub id: String,
    pub texture: String,
    pub variant: SkinVariant,
    pub value: String,
    pub signature: String,
}

/// What `PUT /v1/mc/skin` saves.
#[derive(Clone, PartialEq, Eq)]
pub enum McSource {
    /// The launcher's clean re-encoding of the player's file and its SHA-256 (hex).
    Png {
        variant: SkinVariant,
        png: Vec<u8>,
        sha256: String,
    },
    Premium(McPremium),
    /// No Projects skin.
    Default,
}

impl std::fmt::Debug for McSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png { variant, png, .. } => f
                .debug_struct("Png")
                .field("variant", variant)
                .field("bytes", &png.len())
                .finish_non_exhaustive(),
            Self::Premium(premium) => f.debug_tuple("Premium").field(&premium.name).finish(),
            Self::Default => f.write_str("Default"),
        }
    }
}

impl McSource {
    fn body(&self) -> serde_json::Value {
        match self {
            Self::Png {
                variant,
                png,
                sha256,
            } => serde_json::json!({
                "kind": "png",
                "variant": variant.api_name(),
                "png": STANDARD.encode(png),
                "sha256": sha256,
            }),
            Self::Premium(premium) => serde_json::json!({
                "kind": "premium",
                "premium": {
                    "name": premium.name,
                    "id": premium.id,
                    "texture": premium.texture,
                    "variant": premium.variant.api_name(),
                    "value": premium.value,
                    "signature": premium.signature,
                },
            }),
            Self::Default => serde_json::json!({ "kind": "default" }),
        }
    }
}

/// A `/v1/mc` request that did not go through, with what the service said about it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McFailure {
    /// HTTP status; 0 when the service was not reached (or answered nothing readable).
    pub status: u16,
    /// The service's code (`job_pending`, …); `network`, `protocol` or `session` otherwise.
    pub code: String,
    /// Seconds to wait (`Retry-After` or the body's `retryAfter`).
    pub retry_after: Option<u64>,
    /// `account_limited`, `skin_blocked`, `account_restricted`, `declined`: Unix seconds, or
    /// `Some(None)` for good.
    pub until: Option<Option<i64>>,
    /// `account_limited` (`skin_change`) and `rate_limited` (`skin_changes`, `link_tries`,
    /// `mc_intents`).
    pub item: Option<String>,
    /// `already_linked`, `other_account`: the linked game account.
    pub name: Option<String>,
    /// `invalid_skin` and `503 unavailable` (`off`, `bot`, `paused`).
    pub reason: Option<String>,
}

impl McFailure {
    fn of(status: u16, code: &str) -> Self {
        Self {
            status,
            code: code.to_owned(),
            ..Self::default()
        }
    }

    /// The service could not be reached.
    pub fn network() -> Self {
        Self::of(0, "network")
    }

    /// Not signed in to friends.
    pub fn signed_out() -> Self {
        Self::of(401, "session")
    }

    fn protocol(status: u16) -> Self {
        Self::of(status, "protocol")
    }

    fn from_social(error: &SocialError) -> Self {
        match error {
            SocialError::Unauthorized => Self::of(401, "unauthorized"),
            SocialError::Network(_) => Self::network(),
            _ => Self::protocol(0),
        }
    }

    fn from_reply(reply: &Reply) -> Self {
        #[derive(Deserialize)]
        struct Envelope {
            error: Body,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Body {
            #[serde(default)]
            code: String,
            #[serde(default)]
            retry_after: Option<u64>,
            #[serde(default, deserialize_with = "present")]
            until: Option<Option<i64>>,
            #[serde(default, deserialize_with = "lenient")]
            item: Option<String>,
            #[serde(default, deserialize_with = "lenient")]
            name: Option<String>,
            #[serde(default, deserialize_with = "lenient")]
            reason: Option<String>,
        }
        let body = serde_json::from_slice::<Envelope>(&reply.body)
            .ok()
            .map(|envelope| envelope.error);
        let header = parse_retry_after(reply.retry_after.as_deref()).map(|wait| wait.as_secs());
        match body {
            Some(body) => Self {
                status: reply.status,
                code: body.code,
                retry_after: header.or(body.retry_after),
                until: body.until,
                item: body.item,
                name: body.name,
                reason: body.reason,
            },
            // A page that is not the service's (a proxy's, a limit's): by status alone.
            None => Self {
                retry_after: header,
                ..Self::of(reply.status, "")
            },
        }
    }

    /// The session is no longer valid.
    pub fn unauthorized(&self) -> bool {
        self.status == 401
    }
}

/// A job id the service made: `j` and 22 base64url characters (others are not asked about).
fn job_path(id: &str) -> Result<String, Box<McFailure>> {
    let valid = id.len() == 23
        && id.starts_with('j')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if valid {
        Ok(format!("v1/mc/jobs/{id}"))
    } else {
        Err(Box::new(McFailure::of(404, "not_found")))
    }
}

fn parse<T: DeserializeOwned>(reply: &Reply) -> Result<T, Box<McFailure>> {
    serde_json::from_slice(&reply.body).map_err(|error| {
        tracing::debug!(%error, status = reply.status, "unreadable /v1/mc answer");
        Box::new(McFailure::protocol(reply.status))
    })
}

#[derive(Deserialize)]
struct JobAnswer {
    job: McJob,
}

impl Session {
    async fn mc_call(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Reply, Box<McFailure>> {
        let reply = self
            .client()
            .send(method, path, Some(self.token()), body, &[])
            .await
            .map_err(|error| Box::new(McFailure::from_social(&error)))?;
        if (200..300).contains(&reply.status) {
            Ok(reply)
        } else {
            Err(Box::new(McFailure::from_reply(&reply)))
        }
    }

    /// `GET /v1/mc`: the link, the Projects skin, the open and latest jobs, today's limits.
    pub async fn mc_state(&self) -> Result<McState, Box<McFailure>> {
        parse(&self.mc_call(Method::GET, "v1/mc", None).await?)
    }

    /// `POST /v1/mc/link`: links `name` with the 6-digit code `/discord eşle` showed in game.
    pub async fn mc_link(&self, name: &str, pin: &str) -> Result<McJob, Box<McFailure>> {
        let body = serde_json::json!({ "name": name, "pin": pin });
        let reply = self.mc_call(Method::POST, "v1/mc/link", Some(body)).await?;
        Ok(parse::<JobAnswer>(&reply)?.job)
    }

    /// `DELETE /v1/mc/link`: ProBot removes the link (`/eşleme kaldır`).
    pub async fn mc_unlink(&self) -> Result<McJob, Box<McFailure>> {
        let reply = self.mc_call(Method::DELETE, "v1/mc/link", None).await?;
        Ok(parse::<JobAnswer>(&reply)?.job)
    }

    /// `POST /v1/mc/link/check`: a `check` job, or the link the service learned lately.
    pub async fn mc_check(&self) -> Result<McCheck, Box<McFailure>> {
        #[derive(Deserialize)]
        struct Answer {
            #[serde(default)]
            job: Option<McJob>,
            #[serde(default)]
            link: Option<McLink>,
        }
        let reply = self
            .mc_call(
                Method::POST,
                "v1/mc/link/check",
                Some(serde_json::json!({})),
            )
            .await?;
        let answer: Answer = parse(&reply)?;
        match (answer.job, answer.link) {
            (Some(job), _) => Ok(McCheck::Job(job)),
            (None, Some(link)) => Ok(McCheck::Link(link)),
            (None, None) => Err(Box::new(McFailure::protocol(reply.status))),
        }
    }

    /// `PUT /v1/mc/skin`: the Projects skin of the game account `name`.
    pub async fn mc_put_skin(
        &self,
        name: &str,
        source: &McSource,
    ) -> Result<McJob, Box<McFailure>> {
        let body = serde_json::json!({ "name": name, "source": source.body() });
        let reply = self.mc_call(Method::PUT, "v1/mc/skin", Some(body)).await?;
        Ok(parse::<JobAnswer>(&reply)?.job)
    }

    /// `GET /v1/mc/jobs/:id`: one job, for a launcher without the socket.
    pub async fn mc_job(&self, id: &str) -> Result<McJob, Box<McFailure>> {
        let reply = self.mc_call(Method::GET, &job_path(id)?, None).await?;
        Ok(parse::<JobAnswer>(&reply)?.job)
    }

    /// `DELETE /v1/mc/jobs/:id`: cancels a job ProBot has not taken yet (`409 job_taken` after).
    pub async fn mc_cancel_job(&self, id: &str) -> Result<(), Box<McFailure>> {
        self.mc_call(Method::DELETE, &job_path(id)?, None).await?;
        Ok(())
    }

    /// `POST /v1/mc/intents`: a codeless linking permit for `name` (30 minutes, once).
    pub async fn mc_open_intent(&self, name: &str) -> Result<McIntent, Box<McFailure>> {
        let body = serde_json::json!({ "name": name });
        parse(
            &self
                .mc_call(Method::POST, "v1/mc/intents", Some(body))
                .await?,
        )
    }

    /// `DELETE /v1/mc/intents`: the permit stops working (the game closed, another account).
    pub async fn mc_close_intent(&self) -> Result<(), Box<McFailure>> {
        self.mc_call(Method::DELETE, "v1/mc/intents", None).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::SessionToken;
    use crate::client::{SocialClient, SocialConfig};
    use crate::testutil::{MockReply, MockServer, Recorded};

    pub(crate) const JOB_ID: &str = "jq3TzK0vY8dMw2aLp5bXcQw";

    fn session(server: &MockServer) -> Session {
        let client = SocialClient::new(SocialConfig::new(
            url::Url::parse(&server.base()).unwrap(),
            "ProjectsLauncher/test",
        ))
        .unwrap();
        client.session(SessionToken::new(format!("pls_{}", "a".repeat(43))))
    }

    pub(crate) fn state_json() -> serde_json::Value {
        serde_json::json!({
            "features": ["mcLink", "skins"],
            "health": {"bot": true, "server": false},
            "link": {"state": "linked", "name": "Ahmet", "at": 1792500000, "checkedAt": 1792600000},
            "skin": {"kind": "png", "texture": "ABC123", "variant": "slim", "premium": null,
                     "at": 1792600100, "by": "launcher"},
            "jobs": [
                {"id": JOB_ID, "kind": "skin", "state": "queued", "name": "Ahmet", "source": "png",
                 "createdAt": 1792600000123u64, "updatedAt": 1792600004567u64},
                {"id": "jold000000000000000000a", "kind": "link", "state": "done", "name": "Ahmet",
                 "result": {"code": "linked"}, "createdAt": 1792500000000u64,
                 "updatedAt": 1792500001000u64},
                {"id": "jodd", "kind": "skin", "state": "done", "result": "garbled"},
                "not a job"
            ],
            "limits": {"skinsPerDay": 3, "skinsLeft": 2, "resetAt": 1792627200, "linkTriesLeft": 5},
            "blocked": null,
            "limited": {"until": 1792700000},
            "codeless": {"declined": [{"name": "Ahmet", "until": 1795192000}, {"name": 3}]},
            "future": {"something": true}
        })
    }

    #[test]
    fn reads_the_state_leniently() {
        let state: McState = serde_json::from_value(state_json()).unwrap();
        assert!(state.offers(FEATURE_SKINS) && state.offers(FEATURE_MC_LINK));
        assert!(!state.offers(FEATURE_MC_CODELESS));
        assert!(state.health.bot && !state.health.server);
        assert!(state.link.linked_to("ahmet"));
        assert!(!state.link.linked_to("Mehmet"));
        let skin = state.skin.unwrap();
        assert_eq!(skin.kind, McSkinKind::Png);
        assert_eq!(skin.texture.as_deref(), Some("abc123"));
        assert_eq!(skin.variant, SkinVariant::Slim);
        assert_eq!(skin.by, McSkinBy::Launcher);
        // A job whose result is not an object keeps the job; a list item that is not a job goes.
        assert_eq!(state.jobs.len(), 3);
        assert_eq!(state.jobs[0].state, McJobState::Queued);
        assert!(state.jobs[0].state.is_open());
        assert_eq!(state.jobs[1].code(), "linked");
        assert_eq!(state.jobs[2].result, None);
        assert_eq!(state.jobs[2].code(), "error");
        assert_eq!(state.limits.skins_left, 2);
        assert_eq!(state.limited, Some(McLimitedUntil { until: 1792700000 }));
        assert_eq!(state.declined.len(), 1);
        // An older answer without most of it.
        let old: McState = serde_json::from_str(r#"{"link": {"state": "weird"}}"#).unwrap();
        assert_eq!(old.link.state, McLinkState::Unknown);
        assert!(old.health.bot, "no health: not called away");
        assert!(old.jobs.is_empty() && old.skin.is_none());
    }

    #[test]
    fn a_default_skin_has_no_texture_and_odd_skins_drop() {
        let state: McState = serde_json::from_value(serde_json::json!({
            "skin": {"kind": "default", "texture": null, "variant": null, "premium": null,
                     "at": 5, "by": "staff"}
        }))
        .unwrap();
        let skin = state.skin.unwrap();
        assert_eq!(skin.kind, McSkinKind::Default);
        assert_eq!(skin.texture, None);
        assert_eq!(skin.variant, SkinVariant::Classic);
        assert_eq!(skin.by, McSkinBy::Staff);
        let odd: McState =
            serde_json::from_value(serde_json::json!({"skin": {"kind": "hologram"}})).unwrap();
        assert_eq!(odd.skin, None);
    }

    #[test]
    fn results_keep_their_details() {
        let job: McJob = serde_json::from_value(serde_json::json!({
            "id": JOB_ID, "kind": "skin", "state": "failed", "name": "Ahmet",
            "result": {"code": "skin_blocked", "until": null}, "createdAt": 1, "updatedAt": 2
        }))
        .unwrap();
        assert_eq!(job.code(), "skin_blocked");
        assert_eq!(job.result.as_ref().unwrap().until, Some(None));
        let job: McJob = serde_json::from_value(serde_json::json!({
            "id": JOB_ID, "kind": "skin", "state": "done", "name": "Ahmet",
            "result": {"code": "applied", "texture": "ff", "variant": "slim"}
        }))
        .unwrap();
        assert!(job.applied());
        assert!(job.is_for("AHMET"));
        let cancelled: McJob = serde_json::from_value(serde_json::json!({
            "id": JOB_ID, "kind": "link", "state": "cancelled"
        }))
        .unwrap();
        assert_eq!(cancelled.code(), "cancelled");
        let other: McJob = serde_json::from_value(serde_json::json!({
            "id": JOB_ID, "kind": "teleport", "state": "exploded"
        }))
        .unwrap();
        assert_eq!(other.kind, McJobKind::Other);
        assert_eq!(other.state, McJobState::Failed);
    }

    #[test]
    fn messages_replace_what_was_known() {
        let mut state: McState = serde_json::from_value(state_json()).unwrap();
        let message: McMessage = serde_json::from_value(serde_json::json!({
            "job": {"id": JOB_ID, "kind": "skin", "state": "done", "name": "Ahmet",
                    "result": {"code": "applied", "texture": "ff", "variant": "classic"}},
            "link": {"state": "linked", "name": "Ahmet", "at": 1, "checkedAt": 2},
            "skin": {"kind": "png", "texture": "ff", "variant": "classic", "at": 3, "by": "launcher"},
            "blocked": {"until": 99}
        }))
        .unwrap();
        state.apply(&message);
        assert_eq!(state.blocked, Some(McBlocked { until: Some(99) }));
        assert_eq!(state.skin.as_ref().unwrap().texture.as_deref(), Some("ff"));
        // The closed job took the open one's place (same id).
        let skins: Vec<&McJob> = state
            .jobs
            .iter()
            .filter(|job| job.kind == McJobKind::Skin)
            .collect();
        assert_eq!(skins[0].id, JOB_ID);
        assert!(skins[0].applied());
        assert!(state.jobs.iter().all(|job| !job.state.is_open()));
    }

    /// The single request made to `path`.
    fn only(server: &MockServer, path: &str) -> Recorded {
        let matching: Vec<Recorded> = server
            .requests()
            .into_iter()
            .filter(|request| request.path == path)
            .collect();
        assert_eq!(matching.len(), 1, "requests to {path}");
        matching.into_iter().next().unwrap()
    }

    #[tokio::test]
    async fn calls_carry_the_token_and_the_contracts_bodies() {
        let server = MockServer::start(|request: &Recorded| {
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/v1/mc") => MockReply::json(200, &state_json().to_string()),
                ("POST", "/v1/mc/link") => MockReply::json(
                    202,
                    &serde_json::json!({"job": {"id": JOB_ID, "kind": "link", "state": "pending",
                                            "name": "Ahmet", "createdAt": 1, "updatedAt": 1}})
                    .to_string(),
                ),
                ("PUT", "/v1/mc/skin") | ("DELETE", "/v1/mc/link") => MockReply::json(
                    202,
                    &serde_json::json!({"job": {"id": JOB_ID, "kind": "skin", "state": "pending",
                                            "createdAt": 1, "updatedAt": 1}})
                    .to_string(),
                ),
                ("POST", "/v1/mc/link/check") => MockReply::json(
                    200,
                    r#"{"link": {"state": "none", "name": null, "at": null, "checkedAt": 7}}"#,
                ),
                ("DELETE", path) if path.starts_with("/v1/mc/jobs/") => MockReply::empty(204),
                ("POST", "/v1/mc/intents") => MockReply::json(
                    201,
                    &format!(r#"{{"nonce": "i{}", "expiresAt": 1800}}"#, "A".repeat(32)),
                ),
                ("DELETE", "/v1/mc/intents") => MockReply::empty(204),
                _ => MockReply::json(404, r#"{"error": {"code": "not_found", "message": "x"}}"#),
            }
        })
        .await;
        let session = session(&server);
        let state = session.mc_state().await.unwrap();
        assert_eq!(state.limits.skins_per_day, 3);
        let job = session.mc_link("Ahmet", "482913").await.unwrap();
        assert_eq!(job.kind, McJobKind::Link);
        let link = only(&server, "/v1/mc/link");
        assert_eq!(
            link.json(),
            serde_json::json!({"name": "Ahmet", "pin": "482913"})
        );
        assert!(
            link.header("authorization")
                .unwrap()
                .starts_with("Bearer pls_")
        );

        let png = vec![1, 2, 3];
        session
            .mc_put_skin(
                "Ahmet",
                &McSource::Png {
                    variant: SkinVariant::Slim,
                    png: png.clone(),
                    sha256: "ab".repeat(32),
                },
            )
            .await
            .unwrap();
        session
            .mc_put_skin(
                "Ahmet",
                &McSource::Premium(McPremium {
                    name: "Dream".into(),
                    id: "0".repeat(32),
                    texture: "f".repeat(64),
                    variant: SkinVariant::Classic,
                    value: "dmFsdWU=".into(),
                    signature: "c2ln".into(),
                }),
            )
            .await
            .unwrap();
        session
            .mc_put_skin("Ahmet", &McSource::Default)
            .await
            .unwrap();
        let puts: Vec<serde_json::Value> = server
            .requests()
            .into_iter()
            .filter(|request| request.method == "PUT")
            .map(|request| request.json())
            .collect();
        assert_eq!(
            puts[0],
            serde_json::json!({"name": "Ahmet", "source": {"kind": "png", "variant": "slim",
                "png": STANDARD.encode(&png), "sha256": "ab".repeat(32)}})
        );
        assert_eq!(
            puts[1]["source"],
            serde_json::json!({"kind": "premium", "premium": {"name": "Dream", "id": "0".repeat(32),
                "texture": "f".repeat(64), "variant": "classic", "value": "dmFsdWU=",
                "signature": "c2ln"}})
        );
        assert_eq!(puts[2]["source"], serde_json::json!({"kind": "default"}));

        assert!(matches!(
            session.mc_check().await.unwrap(),
            McCheck::Link(McLink {
                state: McLinkState::None,
                checked_at: Some(7),
                ..
            })
        ));
        session.mc_unlink().await.unwrap();
        session.mc_cancel_job(JOB_ID).await.unwrap();
        // A malformed id is never put in a path.
        assert_eq!(
            session.mc_cancel_job("../me").await.unwrap_err().code,
            "not_found"
        );
        let intent = session.mc_open_intent("Ahmet").await.unwrap();
        assert_eq!(intent.expires_at, 1800);
        assert!(
            !format!("{intent:?}").contains("AAAA"),
            "the nonce never shows"
        );
        session.mc_close_intent().await.unwrap();
    }

    #[tokio::test]
    async fn failures_keep_every_detail() {
        let server = MockServer::start(|request: &Recorded| match request.path.as_str() {
            "/v1/mc/skin" => MockReply::json(
                429,
                r#"{"error": {"code": "rate_limited", "message": "Daily limit reached",
                    "retryAfter": 3600, "item": "skin_changes"}}"#,
            )
            .with_header("Retry-After", "3500"),
            "/v1/mc/link" => MockReply::json(
                403,
                r#"{"error": {"code": "skin_blocked", "message": "x", "until": null}}"#,
            ),
            "/v1/mc/link/check" => MockReply::json(
                409,
                r#"{"error": {"code": "other_account", "message": "x", "name": "Mehmet"}}"#,
            ),
            "/v1/mc" => MockReply::json(
                503,
                r#"{"error": {"code": "unavailable", "message": "x", "retryAfter": 300,
                    "reason": "bot"}}"#,
            ),
            _ => MockReply::empty(502),
        })
        .await;
        let session = session(&server);
        let quota = session
            .mc_put_skin("Ahmet", &McSource::Default)
            .await
            .unwrap_err();
        assert_eq!(
            (quota.status, quota.code.as_str(), quota.retry_after),
            (429, "rate_limited", Some(3500))
        );
        assert_eq!(quota.item.as_deref(), Some("skin_changes"));
        let blocked = session.mc_link("Ahmet", "000000").await.unwrap_err();
        assert_eq!(blocked.until, Some(None), "null: for good");
        let other = session.mc_check().await.unwrap_err();
        assert_eq!(other.name.as_deref(), Some("Mehmet"));
        let away = session.mc_state().await.unwrap_err();
        assert_eq!(away.reason.as_deref(), Some("bot"));
        let gateway = session.mc_job(JOB_ID).await.unwrap_err();
        assert_eq!((gateway.status, gateway.code.as_str()), (502, ""));
    }
}
