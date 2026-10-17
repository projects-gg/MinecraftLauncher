//! Constants and endpoint configuration.

use std::time::Duration;

/// Azure application (client) id of Projects Launcher, registered for "Personal Microsoft accounts
/// only" with the `http://localhost` redirect and approved by Mojang for Minecraft Services.
///
/// This is a public value: a native app cannot keep a client secret, which is why the browser
/// flow uses PKCE.
pub const CLIENT_ID: &str = "24f2a0f7-5640-4f4a-847b-ac8f31e4bb24";

/// The narrowest scope set Minecraft needs. Without `offline_access` Microsoft issues no refresh
/// token and the user would have to sign in on every start.
pub const SCOPE: &str = "XboxLive.signin offline_access";

/// Service name of the launcher's entries in the OS keychain.
pub const KEYRING_SERVICE: &str = "gg.projects.launcher";

/// How long the browser sign-in may take before it is abandoned.
pub const BROWSER_LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// A Minecraft access token that expires within this margin is refreshed before use: launching
/// with a token that dies a minute later gets the player kicked with "invalid session".
pub const REFRESH_MARGIN: Duration = Duration::from_secs(10 * 60);

/// URLs of every service the account flows talk to. Production values by default; tests point
/// them at a local mock server with [`Endpoints::with_base`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    pub authorize: String,
    pub token: String,
    pub device_code: String,
    pub xbox_user_auth: String,
    pub xsts_authorize: String,
    /// Minecraft Services base URL, without a trailing slash.
    pub minecraft_services: String,
    /// Mojang's texture host (public skin and cape pictures), without a trailing slash.
    pub textures: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self::production()
    }
}

impl Endpoints {
    pub fn production() -> Self {
        const MSA: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
        Self {
            authorize: format!("{MSA}/authorize"),
            token: format!("{MSA}/token"),
            device_code: format!("{MSA}/devicecode"),
            xbox_user_auth: "https://user.auth.xboxlive.com/user/authenticate".to_owned(),
            xsts_authorize: "https://xsts.auth.xboxlive.com/xsts/authorize".to_owned(),
            minecraft_services: "https://api.minecraftservices.com".to_owned(),
            textures: "https://textures.minecraft.net".to_owned(),
        }
    }

    /// Every endpoint under one base URL, keeping the production paths (mock servers).
    pub fn with_base(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self {
            authorize: format!("{base}/consumers/oauth2/v2.0/authorize"),
            token: format!("{base}/consumers/oauth2/v2.0/token"),
            device_code: format!("{base}/consumers/oauth2/v2.0/devicecode"),
            xbox_user_auth: format!("{base}/user/authenticate"),
            xsts_authorize: format!("{base}/xsts/authorize"),
            minecraft_services: base.to_owned(),
            textures: base.to_owned(),
        }
    }

    /// A Minecraft Services URL for `path` (which starts with `/`).
    pub(crate) fn minecraft(&self, path: &str) -> String {
        format!("{}{path}", self.minecraft_services)
    }

    /// Where the texture `id` (a [`crate::texture_id`]) is downloaded from.
    pub(crate) fn texture(&self, id: &str) -> String {
        format!("{}/texture/{id}", self.textures)
    }
}

/// Tunables of the Microsoft flows. `Default` is the production configuration.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub client_id: String,
    pub scope: String,
    pub endpoints: Endpoints,
    pub browser_timeout: Duration,
    pub refresh_margin: Duration,
    /// Upper bound for a single auth request (connect + response).
    pub request_timeout: Duration,
    /// Extra attempts when a request could not even connect. Other failures are never retried:
    /// authorization codes are single-use, so a blind retry could turn a slow answer into
    /// `invalid_grant`.
    pub connect_retries: u32,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            client_id: CLIENT_ID.to_owned(),
            scope: SCOPE.to_owned(),
            endpoints: Endpoints::production(),
            browser_timeout: BROWSER_LOGIN_TIMEOUT,
            refresh_margin: REFRESH_MARGIN,
            request_timeout: Duration::from_secs(30),
            connect_retries: 2,
        }
    }
}
