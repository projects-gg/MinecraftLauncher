//! Skins and capes of Microsoft accounts (Minecraft Services profile API) and texture downloads
//! for the 3D preview.

use std::io::{ErrorKind, Read};
use std::path::Path;

use jiff::Timestamp;
use pl_net::CancellationToken;
use reqwest::header::ACCEPT;
use reqwest::multipart::{Form, Part};
use serde_json::json;
use url::Url;

use crate::error::{AuthError, AuthStep, Result, SkinFileProblem};
use crate::http::send;
use crate::microsoft::{MicrosoftAuth, ProfileResponse};
use crate::model::{MinecraftProfile, SkinVariant};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// Generous cap: a 64×64 RGBA skin is a few KiB; the server has its own, stricter checks.
pub const MAX_SKIN_BYTES: usize = 1024 * 1024;
const MAX_TEXTURE_BYTES: usize = 2 * 1024 * 1024;
/// Where Mojang serves skin and cape textures.
pub const TEXTURE_HOST: &str = "textures.minecraft.net";

/// True for a skin or cape texture on Mojang's texture host (`http` or `https`, default port,
/// no credentials). Lets the app fetch textures for the player without fetching arbitrary URLs.
pub fn is_mojang_texture_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && url.host_str() == Some(TEXTURE_HOST)
            && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

/// Reads a skin PNG the player picked. Files over [`MAX_SKIN_BYTES`] are rejected without being
/// read whole; the content is then checked like [`validate_skin_png`].
pub async fn read_skin_file(path: &Path) -> Result<Vec<u8>> {
    let path = path.to_owned();
    let bytes = tokio::task::spawn_blocking(move || read_capped(&path))
        .await
        .map_err(|error| AuthError::unexpected(AuthStep::Skin, error.to_string()))?
        .map_err(|problem| AuthError::InvalidSkinFile { problem })?;
    validate_skin_png(&bytes).map_err(|problem| AuthError::InvalidSkinFile { problem })?;
    Ok(bytes)
}

fn read_capped(path: &Path) -> Result<Vec<u8>, SkinFileProblem> {
    let unreadable = |error: std::io::Error| SkinFileProblem::Unreadable { kind: error.kind() };
    let file = std::fs::File::open(path).map_err(unreadable)?;
    let meta = file.metadata().map_err(unreadable)?;
    if !meta.is_file() {
        return Err(SkinFileProblem::Unreadable {
            kind: ErrorKind::InvalidInput,
        });
    }
    let limit = MAX_SKIN_BYTES as u64;
    if meta.len() > limit {
        return Err(SkinFileProblem::TooLarge);
    }
    let mut bytes = Vec::new();
    // The cap also holds when the file grows after the size check.
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    if bytes.len() > MAX_SKIN_BYTES {
        return Err(SkinFileProblem::TooLarge);
    }
    Ok(bytes)
}

/// Checks that `bytes` is a PNG of a Java Edition skin (64×64, or legacy 64×32) and returns its
/// dimensions. Reads only the IHDR header.
pub fn validate_skin_png(bytes: &[u8]) -> Result<(u32, u32), SkinFileProblem> {
    if bytes.len() > MAX_SKIN_BYTES {
        return Err(SkinFileProblem::TooLarge);
    }
    if bytes.len() < 24 || !bytes.starts_with(PNG_SIGNATURE) || &bytes[12..16] != b"IHDR" {
        return Err(SkinFileProblem::NotPng);
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width == 64 && (height == 64 || height == 32) {
        Ok((width, height))
    } else {
        Err(SkinFileProblem::BadDimensions { width, height })
    }
}

impl MicrosoftAuth {
    /// Uploads a skin file (multipart: `variant` + `file`). Returns the updated profile.
    pub async fn upload_skin(
        &self,
        access_token: &str,
        png: Vec<u8>,
        variant: SkinVariant,
        cancel: &CancellationToken,
    ) -> Result<MinecraftProfile> {
        validate_skin_png(&png).map_err(|problem| AuthError::InvalidSkinFile { problem })?;
        let file = Part::bytes(png)
            .file_name("skin.png")
            .mime_str("image/png")
            .map_err(|error| AuthError::unexpected(AuthStep::Skin, error.to_string()))?;
        let form = Form::new()
            .text("variant", variant.api_name())
            .part("file", file);
        let request = self
            .client()
            .post(
                self.config()
                    .endpoints
                    .minecraft("/minecraft/profile/skins"),
            )
            .bearer_auth(access_token)
            .header(ACCEPT, "application/json")
            .multipart(form);
        self.profile_mutation(request, AuthStep::Skin, access_token, cancel)
            .await
    }

    /// Sets the skin to an image Mojang downloads from `skin_url`.
    pub async fn set_skin_url(
        &self,
        access_token: &str,
        skin_url: &str,
        variant: SkinVariant,
        cancel: &CancellationToken,
    ) -> Result<MinecraftProfile> {
        let valid = Url::parse(skin_url)
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.has_host());
        if !valid {
            return Err(AuthError::InvalidSkinFile {
                problem: SkinFileProblem::InvalidUrl,
            });
        }
        let request = self
            .client()
            .post(
                self.config()
                    .endpoints
                    .minecraft("/minecraft/profile/skins"),
            )
            .bearer_auth(access_token)
            .header(ACCEPT, "application/json")
            .json(&json!({ "variant": variant.api_name(), "url": skin_url }));
        self.profile_mutation(request, AuthStep::Skin, access_token, cancel)
            .await
    }

    /// Back to the default skin.
    pub async fn reset_skin(
        &self,
        access_token: &str,
        cancel: &CancellationToken,
    ) -> Result<MinecraftProfile> {
        let request = self
            .client()
            .delete(
                self.config()
                    .endpoints
                    .minecraft("/minecraft/profile/skins/active"),
            )
            .bearer_auth(access_token)
            .header(ACCEPT, "application/json");
        self.profile_mutation(request, AuthStep::Skin, access_token, cancel)
            .await
    }

    /// Shows the owned cape `cape_id`.
    pub async fn show_cape(
        &self,
        access_token: &str,
        cape_id: &str,
        cancel: &CancellationToken,
    ) -> Result<MinecraftProfile> {
        let request = self
            .client()
            .put(
                self.config()
                    .endpoints
                    .minecraft("/minecraft/profile/capes/active"),
            )
            .bearer_auth(access_token)
            .header(ACCEPT, "application/json")
            .json(&json!({ "capeId": cape_id }));
        self.profile_mutation(request, AuthStep::Cape, access_token, cancel)
            .await
    }

    /// Hides the cape.
    pub async fn hide_cape(
        &self,
        access_token: &str,
        cancel: &CancellationToken,
    ) -> Result<MinecraftProfile> {
        let request = self
            .client()
            .delete(
                self.config()
                    .endpoints
                    .minecraft("/minecraft/profile/capes/active"),
            )
            .bearer_auth(access_token)
            .header(ACCEPT, "application/json");
        self.profile_mutation(request, AuthStep::Cape, access_token, cancel)
            .await
    }

    /// Downloads a skin or cape texture (PNG bytes) for the 3D preview. Mojang's `http://`
    /// texture URLs are fetched over HTTPS.
    pub async fn download_texture(
        &self,
        texture_url: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>> {
        let mut url = Url::parse(texture_url)
            .map_err(|_| AuthError::unexpected(AuthStep::Texture, "invalid texture URL"))?;
        match url.scheme() {
            "https" => {}
            "http" => {
                if url.host_str() == Some("textures.minecraft.net") {
                    let _ = url.set_scheme("https");
                }
            }
            _ => {
                return Err(AuthError::unexpected(
                    AuthStep::Texture,
                    "texture URL must be http(s)",
                ));
            }
        }
        let request = self.client().get(url);
        let reply = send(request, AuthStep::Texture, self.send_options(), cancel).await?;
        if !reply.is_success() {
            return Err(AuthError::MinecraftApi {
                step: AuthStep::Texture,
                status: reply.status,
                detail: String::new(),
                at: Timestamp::now(),
            });
        }
        if reply.body.len() > MAX_TEXTURE_BYTES {
            return Err(AuthError::unexpected(
                AuthStep::Texture,
                "texture too large",
            ));
        }
        if !reply.body.starts_with(PNG_SIGNATURE) {
            return Err(AuthError::unexpected(
                AuthStep::Texture,
                "texture is not a PNG",
            ));
        }
        Ok(reply.body)
    }

    /// Sends a skin/cape change. The endpoints answer with the updated profile; when they do
    /// not, the profile is fetched.
    async fn profile_mutation(
        &self,
        request: reqwest::RequestBuilder,
        step: AuthStep,
        access_token: &str,
        cancel: &CancellationToken,
    ) -> Result<MinecraftProfile> {
        let reply = send(request, step, self.send_options(), cancel).await?;
        if !reply.is_success() {
            tracing::warn!(%step, status = reply.status, "profile change rejected");
            return Err(AuthError::MinecraftApi {
                step,
                status: reply.status,
                detail: reply.snippet(),
                at: Timestamp::now(),
            });
        }
        match serde_json::from_slice::<ProfileResponse>(&reply.body) {
            Ok(profile) => profile.into_profile(step),
            Err(_) => self.fetch_profile(access_token, cancel).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{self, MockReply, MockServer, Recorded};

    fn auth_for(server: &MockServer) -> MicrosoftAuth {
        MicrosoftAuth::with_config(testutil::http(), testutil::config(server))
    }

    #[test]
    fn skin_png_validation() {
        assert_eq!(validate_skin_png(&testutil::png(64, 64)), Ok((64, 64)));
        assert_eq!(validate_skin_png(&testutil::png(64, 32)), Ok((64, 32)));
        assert_eq!(
            validate_skin_png(&testutil::png(128, 128)),
            Err(SkinFileProblem::BadDimensions {
                width: 128,
                height: 128
            })
        );
        assert_eq!(
            validate_skin_png(b"GIF89a....................."),
            Err(SkinFileProblem::NotPng)
        );
        assert_eq!(validate_skin_png(&[]), Err(SkinFileProblem::NotPng));
        let mut huge = testutil::png(64, 64);
        huge.resize(MAX_SKIN_BYTES + 1, 0);
        assert_eq!(validate_skin_png(&huge), Err(SkinFileProblem::TooLarge));
    }

    fn problem(result: Result<Vec<u8>>) -> SkinFileProblem {
        match result {
            Err(AuthError::InvalidSkinFile { problem }) => problem,
            other => panic!("expected a skin file problem, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn skin_files_are_read_with_a_size_cap() {
        let dir = tempfile::tempdir().unwrap();
        let skin = dir.path().join("skin.png");
        std::fs::write(&skin, testutil::png(64, 64)).unwrap();
        assert_eq!(read_skin_file(&skin).await.unwrap(), testutil::png(64, 64));
        let legacy = dir.path().join("legacy.png");
        std::fs::write(&legacy, testutil::png(64, 32)).unwrap();
        assert!(read_skin_file(&legacy).await.is_ok());

        let big = dir.path().join("big.png");
        std::fs::write(&big, testutil::png(128, 128)).unwrap();
        assert_eq!(
            problem(read_skin_file(&big).await),
            SkinFileProblem::BadDimensions {
                width: 128,
                height: 128
            }
        );
        let huge = dir.path().join("huge.png");
        let mut bytes = testutil::png(64, 64);
        bytes.resize(MAX_SKIN_BYTES + 1, 0);
        std::fs::write(&huge, bytes).unwrap();
        assert_eq!(
            problem(read_skin_file(&huge).await),
            SkinFileProblem::TooLarge
        );
        let text = dir.path().join("notes.png");
        std::fs::write(&text, "not a picture").unwrap();
        assert_eq!(
            problem(read_skin_file(&text).await),
            SkinFileProblem::NotPng
        );

        let missing = problem(read_skin_file(&dir.path().join("gone.png")).await);
        assert_eq!(
            missing,
            SkinFileProblem::Unreadable {
                kind: ErrorKind::NotFound
            }
        );
        let folder = problem(read_skin_file(dir.path()).await);
        assert!(matches!(folder, SkinFileProblem::Unreadable { .. }));
        let error = AuthError::InvalidSkinFile { problem: folder };
        assert_eq!(error.code(), "SKIN-DOSYA");
        assert!(error.user_message_tr().contains("okunamadı"));
    }

    #[test]
    fn only_mojang_texture_urls_are_textures() {
        assert!(is_mojang_texture_url(
            "https://textures.minecraft.net/texture/abc"
        ));
        assert!(is_mojang_texture_url(
            "http://textures.minecraft.net/texture/abc"
        ));
        for url in [
            "https://textures.minecraft.net.evil.example/texture/abc",
            "https://evil.example/textures.minecraft.net/texture/abc",
            "https://user:pass@textures.minecraft.net/texture/abc",
            "https://textures.minecraft.net:8443/texture/abc",
            "file:///C:/skin.png",
            "data:image/png;base64,AAAA",
            "not a url",
        ] {
            assert!(!is_mojang_texture_url(url), "{url}");
        }
    }

    #[tokio::test]
    async fn upload_sends_multipart_with_variant() {
        let server = MockServer::start(testutil::happy_chain).await;
        let auth = auth_for(&server);
        let profile = auth
            .upload_skin(
                "mc-token",
                testutil::png(64, 64),
                SkinVariant::Slim,
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(profile.name, "Steve");
        let upload = server.only("/minecraft/profile/skins");
        assert_eq!(upload.method, "POST");
        assert!(
            upload
                .header("content-type")
                .unwrap()
                .starts_with("multipart/form-data; boundary=")
        );
        assert_eq!(upload.header("authorization"), Some("Bearer mc-token"));
        let body = String::from_utf8_lossy(&upload.body);
        assert!(body.contains("name=\"variant\""));
        assert!(body.contains("slim"));
        assert!(body.contains("name=\"file\"; filename=\"skin.png\""));
        assert!(body.contains("image/png"));

        let error = auth
            .upload_skin(
                "mc-token",
                testutil::png(10, 10),
                SkinVariant::Classic,
                &CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), "SKIN-DOSYA");
        assert_eq!(server.hits("/minecraft/profile/skins"), 1);
    }

    #[tokio::test]
    async fn skin_url_reset_and_capes_hit_the_right_endpoints() {
        let server = MockServer::start(testutil::happy_chain).await;
        let auth = auth_for(&server);
        let cancel = CancellationToken::new();
        auth.set_skin_url(
            "t",
            "https://example.com/skin.png",
            SkinVariant::Classic,
            &cancel,
        )
        .await
        .unwrap();
        let set = server.only("/minecraft/profile/skins");
        assert_eq!(set.json()["url"], "https://example.com/skin.png");
        assert_eq!(set.json()["variant"], "classic");
        assert!(
            auth.set_skin_url("t", "file:///etc/passwd", SkinVariant::Classic, &cancel)
                .await
                .is_err()
        );

        auth.reset_skin("t", &cancel).await.unwrap();
        assert_eq!(
            server.only("/minecraft/profile/skins/active").method,
            "DELETE"
        );

        auth.show_cape("t", "cape-1", &cancel).await.unwrap();
        auth.hide_cape("t", &cancel).await.unwrap();
        let capes: Vec<Recorded> = server
            .requests()
            .into_iter()
            .filter(|request| request.path == "/minecraft/profile/capes/active")
            .collect();
        assert_eq!(capes.len(), 2);
        assert_eq!(capes[0].method, "PUT");
        assert_eq!(capes[0].json()["capeId"], "cape-1");
        assert_eq!(capes[1].method, "DELETE");
    }

    #[tokio::test]
    async fn rejected_skin_maps_to_a_skin_error() {
        let server = MockServer::start(|request: &Recorded| {
            if request.path == "/minecraft/profile/skins" {
                MockReply::json(400, serde_json::json!({"errorMessage": "Invalid skin"}))
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        let error = auth_for(&server)
            .upload_skin(
                "t",
                testutil::png(64, 64),
                SkinVariant::Classic,
                &CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), "MC-SKIN-400");
        assert!(error.user_message_tr().contains("64×64"));
    }

    #[tokio::test]
    async fn downloads_png_textures_only() {
        let server = MockServer::start(testutil::happy_chain).await;
        let auth = auth_for(&server);
        let cancel = CancellationToken::new();
        let png = auth
            .download_texture(&format!("{}/textures/skin.png", server.base()), &cancel)
            .await
            .unwrap();
        assert!(png.starts_with(PNG_SIGNATURE));
        let not_png = auth
            .download_texture(&format!("{}/minecraft/profile", server.base()), &cancel)
            .await;
        assert!(not_png.is_err());
        let missing = auth
            .download_texture(&format!("{}/textures/missing.png", server.base()), &cancel)
            .await
            .unwrap_err();
        assert_eq!(missing.code(), "MC-DOKU-404");
        assert!(
            auth.download_texture("ftp://x/y.png", &cancel)
                .await
                .is_err()
        );
    }
}
