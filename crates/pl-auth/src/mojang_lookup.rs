//! A premium player's skin by name, from Mojang (SKIN1, skin1.md §4.2): the profile, its signed
//! `textures` property (`value` + `signature`, which Lobi checks against Mojang's keys) and the
//! texture itself. Only Mojang's own hosts are asked: no third-party skin or avatar service, and
//! never MineSkin.
//!
//! Answers are kept for ten minutes per name, failures included: the same name is not asked
//! again meanwhile (Mojang rate-limits these endpoints per address).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::avatar::texture_id;
use crate::model::SkinVariant;

/// How long an answer about a name is kept.
pub const LOOKUP_TTL: Duration = Duration::from_secs(600);
/// Names kept at most (the oldest go first).
const CACHE_MAX: usize = 64;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_JSON_BYTES: usize = 64 * 1024;
/// Skin textures are a few KiB.
const MAX_TEXTURE_BYTES: usize = 2 * 1024 * 1024;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Where the profile, the signed textures and the textures come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MojangEndpoints {
    /// Minecraft Services (`/minecraft/profile/lookup/name/<name>`), without a trailing slash.
    pub services: String,
    /// The older profile API (`/users/profiles/minecraft/<name>`), asked only when the first
    /// does not answer.
    pub legacy: String,
    /// The session server (`/session/minecraft/profile/<id>?unsigned=false`).
    pub session: String,
    /// The texture host (`/texture/<id>`).
    pub textures: String,
}

impl Default for MojangEndpoints {
    fn default() -> Self {
        Self::production()
    }
}

impl MojangEndpoints {
    pub fn production() -> Self {
        Self {
            services: "https://api.minecraftservices.com".to_owned(),
            legacy: "https://api.mojang.com".to_owned(),
            session: "https://sessionserver.mojang.com".to_owned(),
            textures: "https://textures.minecraft.net".to_owned(),
        }
    }

    /// Everything under one base URL with the production paths (mock servers).
    pub fn with_base(base: &str) -> Self {
        let base = base.trim_end_matches('/').to_owned();
        Self {
            services: base.clone(),
            legacy: base.clone(),
            session: base.clone(),
            textures: base,
        }
    }
}

/// A premium player's skin as Mojang signed it.
#[derive(Clone, PartialEq, Eq)]
pub struct PremiumSkin {
    /// The name as Mojang writes it.
    pub name: String,
    /// The profile's id: 32 lower-case hex.
    pub id: String,
    /// The texture's id on Mojang's texture host (the URL's last part, lower case).
    pub texture: String,
    /// From the texture's metadata (`slim`), else classic.
    pub variant: SkinVariant,
    /// Mojang's signed `textures` property as it came (base64 JSON) and its signature.
    pub value: String,
    pub signature: String,
    /// The texture (PNG) as Mojang serves it.
    pub png: Vec<u8>,
}

impl fmt::Debug for PremiumSkin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PremiumSkin")
            .field("name", &self.name)
            .field("id", &self.id)
            .field("texture", &self.texture)
            .field("variant", &self.variant)
            .finish_non_exhaustive()
    }
}

/// What Mojang said about a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PremiumLookup {
    Found(Arc<PremiumSkin>),
    /// No Minecraft account has this name ("Bu adda bir Minecraft hesabı yok.").
    NoAccount,
    /// The player wears Minecraft's default skin: there is no texture to keep.
    NoSkin,
    /// Mojang answered 429 or 5xx, or not at all ("Şu an bakılamıyor.").
    Unreachable,
}

/// A name Mojang may know: 3–16 of `A–Z a–z 0–9 _`.
pub fn is_premium_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Asks Mojang about names and keeps the answers for [`LOOKUP_TTL`].
pub struct MojangLookup {
    http: reqwest::Client,
    endpoints: MojangEndpoints,
    cache: Mutex<HashMap<String, (Instant, PremiumLookup)>>,
}

impl fmt::Debug for MojangLookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MojangLookup")
            .field("endpoints", &self.endpoints)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct NameAnswer {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct ProfileAnswer {
    id: String,
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
    #[serde(default)]
    signature: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TexturesValue {
    #[serde(default)]
    profile_id: Option<String>,
    #[serde(default)]
    textures: Textures,
}

#[derive(Default, Deserialize)]
struct Textures {
    #[serde(default, rename = "SKIN")]
    skin: Option<SkinTexture>,
}

#[derive(Deserialize)]
struct SkinTexture {
    url: String,
    #[serde(default)]
    metadata: Option<SkinMetadata>,
}

#[derive(Deserialize)]
struct SkinMetadata {
    #[serde(default)]
    model: Option<String>,
}

/// One request's outcome.
enum Answer<T> {
    Ok(T),
    /// 204, 400 or 404: nothing by that name or id.
    Missing,
    /// 429, 5xx, a network failure, or an answer that does not read.
    Failed,
}

impl MojangLookup {
    pub fn new(http: reqwest::Client) -> Self {
        Self::with_endpoints(http, MojangEndpoints::production())
    }

    pub fn with_endpoints(http: reqwest::Client, endpoints: MojangEndpoints) -> Self {
        Self {
            http,
            endpoints,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn cached(&self, key: &str) -> Option<PremiumLookup> {
        let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .get(key)
            .filter(|(at, _)| at.elapsed() < LOOKUP_TTL)
            .map(|(_, answer)| answer.clone())
    }

    fn keep(&self, key: String, answer: &PremiumLookup) {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|_, (at, _)| at.elapsed() < LOOKUP_TTL);
        if cache.len() >= CACHE_MAX
            && let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(name, _)| name.clone())
        {
            cache.remove(&oldest);
        }
        cache.insert(key, (Instant::now(), answer.clone()));
    }

    /// The premium player's skin with its signed property and texture. A name that cannot be a
    /// Minecraft name is [`PremiumLookup::NoAccount`] without asking.
    pub async fn lookup(&self, name: &str) -> PremiumLookup {
        if !is_premium_name(name) {
            return PremiumLookup::NoAccount;
        }
        let key = name.to_ascii_lowercase();
        if let Some(answer) = self.cached(&key) {
            return answer;
        }
        let answer = self.ask(name).await;
        self.keep(key, &answer);
        answer
    }

    async fn ask(&self, name: &str) -> PremiumLookup {
        let services = format!(
            "{}/minecraft/profile/lookup/name/{name}",
            self.endpoints.services
        );
        let profile = match self.json::<NameAnswer>(&services).await {
            Answer::Ok(profile) => profile,
            Answer::Missing => return PremiumLookup::NoAccount,
            Answer::Failed => {
                let legacy = format!("{}/users/profiles/minecraft/{name}", self.endpoints.legacy);
                match self.json::<NameAnswer>(&legacy).await {
                    Answer::Ok(profile) => profile,
                    Answer::Missing => return PremiumLookup::NoAccount,
                    Answer::Failed => return PremiumLookup::Unreachable,
                }
            }
        };
        let id = profile.id.to_ascii_lowercase();
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return PremiumLookup::Unreachable;
        }
        let session = format!(
            "{}/session/minecraft/profile/{id}?unsigned=false",
            self.endpoints.session
        );
        let answer = match self.json::<ProfileAnswer>(&session).await {
            Answer::Ok(answer) => answer,
            Answer::Missing => return PremiumLookup::NoAccount,
            Answer::Failed => return PremiumLookup::Unreachable,
        };
        if !answer.id.eq_ignore_ascii_case(&id) {
            return PremiumLookup::Unreachable;
        }
        let Some(property) = answer.properties.into_iter().find(|p| p.name == "textures") else {
            return PremiumLookup::NoSkin;
        };
        let Some(signature) = property.signature.filter(|s| !s.is_empty()) else {
            return PremiumLookup::Unreachable;
        };
        let Some(decoded) = STANDARD
            .decode(property.value.as_bytes())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<TexturesValue>(&bytes).ok())
        else {
            return PremiumLookup::Unreachable;
        };
        if decoded
            .profile_id
            .as_deref()
            .is_some_and(|owner| !owner.eq_ignore_ascii_case(&id))
        {
            return PremiumLookup::Unreachable;
        }
        let Some(skin) = decoded.textures.skin else {
            return PremiumLookup::NoSkin;
        };
        // Only a texture on Mojang's texture host, by its content address.
        let Some(texture) = texture_id(&skin.url).filter(|id| (32..=64).contains(&id.len())) else {
            return PremiumLookup::Unreachable;
        };
        let variant = match skin.metadata.and_then(|m| m.model).as_deref() {
            Some("slim") => SkinVariant::Slim,
            _ => SkinVariant::Classic,
        };
        let Some(png) = self.texture(&texture).await else {
            return PremiumLookup::Unreachable;
        };
        PremiumLookup::Found(Arc::new(PremiumSkin {
            name: profile.name,
            id,
            texture,
            variant,
            value: property.value,
            signature,
            png,
        }))
    }

    async fn get(&self, url: &str) -> Option<reqwest::Response> {
        match self
            .http
            .get(url)
            .timeout(REQUEST_TIMEOUT)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
        {
            Ok(response) => Some(response),
            Err(error) => {
                tracing::debug!(error = %error.without_url(), "Mojang did not answer");
                None
            }
        }
    }

    async fn json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Answer<T> {
        let Some(response) = self.get(url).await else {
            return Answer::Failed;
        };
        match response.status() {
            StatusCode::OK => {}
            StatusCode::NO_CONTENT | StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST => {
                return Answer::Missing;
            }
            status => {
                tracing::debug!(%status, "Mojang refused a lookup");
                return Answer::Failed;
            }
        }
        match read_capped(response, MAX_JSON_BYTES).await {
            Some(body) => serde_json::from_slice(&body).map_or(Answer::Failed, Answer::Ok),
            None => Answer::Failed,
        }
    }

    /// A texture (PNG) from Mojang's texture host by its id (hex, as in a texture URL). Not kept:
    /// callers keep what they need.
    pub async fn texture(&self, id: &str) -> Option<Vec<u8>> {
        let valid = !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_hexdigit());
        if !valid {
            return None;
        }
        let url = format!("{}/texture/{id}", self.endpoints.textures);
        let response = self.get(&url).await?;
        if response.status() != StatusCode::OK {
            return None;
        }
        read_capped(response, MAX_TEXTURE_BYTES)
            .await
            .filter(|body| body.starts_with(PNG_SIGNATURE))
    }
}

async fn read_capped(mut response: reqwest::Response, max: usize) -> Option<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > max as u64)
    {
        return None;
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if body.len() + chunk.len() > max {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    Some(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{self, MockReply, MockServer, Recorded};

    const ID: &str = "069a79f444e94726a5befca90e38aaf5";
    const TEXTURE: &str = "1a4af718455d4aab528e7a61f86fa25e6a369d1768dcb13f7df319a713eb810b";

    fn value(model: Option<&str>, skin: bool) -> String {
        let mut textures = serde_json::json!({});
        if skin {
            let mut entry = serde_json::json!({
                "url": format!("http://textures.minecraft.net/texture/{TEXTURE}")
            });
            if let Some(model) = model {
                entry["metadata"] = serde_json::json!({ "model": model });
            }
            textures["SKIN"] = entry;
        }
        STANDARD.encode(
            serde_json::json!({
                "timestamp": 1792600000000u64,
                "profileId": ID,
                "profileName": "Notch",
                "signatureRequired": true,
                "textures": textures,
            })
            .to_string(),
        )
    }

    fn mojang(model: Option<&'static str>, skin: bool) -> impl Fn(&Recorded) -> MockReply {
        move |request: &Recorded| match request.path.as_str() {
            "/minecraft/profile/lookup/name/notch" | "/minecraft/profile/lookup/name/Notch" => {
                MockReply::json(200, serde_json::json!({ "id": ID, "name": "Notch" }))
            }
            path if path == format!("/session/minecraft/profile/{ID}") => {
                assert_eq!(request.query, "unsigned=false");
                MockReply::json(
                    200,
                    serde_json::json!({
                        "id": ID,
                        "name": "Notch",
                        "properties": [{
                            "name": "textures",
                            "value": value(model, skin),
                            "signature": "c2lnbmF0dXJl",
                        }],
                    }),
                )
            }
            path if path == format!("/texture/{TEXTURE}") => {
                MockReply::bytes(200, "image/png", testutil::png(64, 64))
            }
            _ => MockReply::json(404, serde_json::json!({ "errorMessage": "nope" })),
        }
    }

    fn lookup_for(server: &MockServer) -> MojangLookup {
        MojangLookup::with_endpoints(
            testutil::http().client().clone(),
            MojangEndpoints::with_base(server.base()),
        )
    }

    #[tokio::test]
    async fn finds_the_signed_texture_and_keeps_the_answer() {
        let server = MockServer::start(mojang(Some("slim"), true)).await;
        let lookup = lookup_for(&server);
        let PremiumLookup::Found(skin) = lookup.lookup("notch").await else {
            panic!("expected a skin");
        };
        assert_eq!(skin.name, "Notch");
        assert_eq!(skin.id, ID);
        assert_eq!(skin.texture, TEXTURE);
        assert_eq!(skin.variant, SkinVariant::Slim);
        assert_eq!(skin.value, value(Some("slim"), true));
        assert_eq!(skin.signature, "c2lnbmF0dXJl");
        assert!(skin.png.starts_with(PNG_SIGNATURE));
        // The same name, any case, is not asked again for ten minutes.
        assert!(matches!(
            lookup.lookup("NOTCH").await,
            PremiumLookup::Found(_)
        ));
        assert_eq!(server.requests().len(), 3);
    }

    #[tokio::test]
    async fn classic_without_metadata_and_no_skin_without_one() {
        let server = MockServer::start(mojang(None, true)).await;
        let PremiumLookup::Found(skin) = lookup_for(&server).lookup("Notch").await else {
            panic!("expected a skin");
        };
        assert_eq!(skin.variant, SkinVariant::Classic);

        let server = MockServer::start(mojang(None, false)).await;
        assert_eq!(
            lookup_for(&server).lookup("Notch").await,
            PremiumLookup::NoSkin
        );
    }

    #[tokio::test]
    async fn unknown_and_impossible_names_have_no_account() {
        let server = MockServer::start(mojang(None, true)).await;
        let lookup = lookup_for(&server);
        assert_eq!(lookup.lookup("Nobody_Here").await, PremiumLookup::NoAccount);
        // Never asked: not a Minecraft name.
        assert_eq!(lookup.lookup("ab").await, PremiumLookup::NoAccount);
        assert_eq!(lookup.lookup("a b c").await, PremiumLookup::NoAccount);
        assert_eq!(server.requests().len(), 1);
    }

    #[tokio::test]
    async fn the_older_api_answers_when_minecraft_services_does_not() {
        let server = MockServer::start(|request: &Recorded| match request.path.as_str() {
            "/minecraft/profile/lookup/name/Notch" => {
                MockReply::json(429, serde_json::json!({ "error": "TooManyRequests" }))
            }
            "/users/profiles/minecraft/Notch" => {
                MockReply::json(200, serde_json::json!({ "id": ID, "name": "Notch" }))
            }
            _ => mojang(Some("slim"), true)(request),
        })
        .await;
        assert!(matches!(
            lookup_for(&server).lookup("Notch").await,
            PremiumLookup::Found(_)
        ));
        assert_eq!(server.hits("/users/profiles/minecraft/Notch"), 1);
    }

    #[tokio::test]
    async fn outages_are_unreachable_and_kept_too() {
        let server = MockServer::start(|_: &Recorded| {
            MockReply::json(503, serde_json::json!({ "error": "down" }))
        })
        .await;
        let lookup = lookup_for(&server);
        assert_eq!(lookup.lookup("Notch").await, PremiumLookup::Unreachable);
        assert_eq!(lookup.lookup("Notch").await, PremiumLookup::Unreachable);
        // The first answer stands: two requests (the API and its fallback), not four.
        assert_eq!(server.requests().len(), 2);
    }

    #[tokio::test]
    async fn a_texture_elsewhere_or_unsigned_is_not_kept() {
        let server = MockServer::start(|request: &Recorded| {
            if request.path.starts_with("/session/") {
                let foreign = STANDARD.encode(
                    serde_json::json!({
                        "profileId": ID,
                        "textures": {"SKIN": {"url": "https://example.com/skin.png"}},
                    })
                    .to_string(),
                );
                MockReply::json(
                    200,
                    serde_json::json!({
                        "id": ID,
                        "properties": [{"name": "textures", "value": foreign, "signature": "c2ln"}],
                    }),
                )
            } else {
                mojang(None, true)(request)
            }
        })
        .await;
        assert_eq!(
            lookup_for(&server).lookup("Notch").await,
            PremiumLookup::Unreachable
        );

        let server = MockServer::start(|request: &Recorded| {
            if request.path.starts_with("/session/") {
                MockReply::json(
                    200,
                    serde_json::json!({
                        "id": ID,
                        "properties": [{"name": "textures", "value": value(None, true)}],
                    }),
                )
            } else {
                mojang(None, true)(request)
            }
        })
        .await;
        assert_eq!(
            lookup_for(&server).lookup("Notch").await,
            PremiumLookup::Unreachable
        );
    }

    #[test]
    fn premium_names() {
        assert!(is_premium_name("Dream"));
        assert!(is_premium_name("a_b"));
        assert!(!is_premium_name("ab"));
        assert!(!is_premium_name("seventeen_chars_x"));
        assert!(!is_premium_name("Ahmet!"));
        assert!(!is_premium_name(".Ahmet"));
    }
}
