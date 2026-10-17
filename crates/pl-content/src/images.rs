//! The hosts the launcher shows pictures from. Its content security policy (`img-src` in
//! `apps/launcher/src-tauri/tauri.conf.json`) lists exactly these besides its own bundled and
//! cached files; a test keeps the two the same. Studio warns before publishing a mod center icon
//! or a news picture from anywhere else: the launcher would not show it.

use url::Url;

/// Hosts the launcher loads `https://` pictures from: published media, Modrinth icons and
/// descriptions (with the GitHub-hosted pictures they use), Discord profile pictures of friends
/// and Minecraft skin textures. No third-party avatar service is ever asked (docs/security.md).
pub const LAUNCHER_IMAGE_HOSTS: &[&str] = &[
    "cdn.projects.gg",
    "raw.githubusercontent.com",
    "cdn.modrinth.com",
    "cdn.discordapp.com",
    "textures.minecraft.net",
];

/// Whether the launcher would show a picture from `url`: `https://` on one of
/// [`LAUNCHER_IMAGE_HOSTS`], without credentials or another port.
pub fn is_launcher_image_url(url: &str) -> bool {
    Url::parse(url.trim()).is_ok_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.host_str().is_some_and(|host| {
                LAUNCHER_IMAGE_HOSTS
                    .iter()
                    .any(|allowed| host.eq_ignore_ascii_case(allowed))
            })
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn only_listed_https_hosts_are_launcher_images() {
        for good in [
            "https://cdn.modrinth.com/data/AANobbMI/icon.png",
            "https://CDN.projects.gg/v1/media/a.webp",
            " https://textures.minecraft.net/texture/abc ",
        ] {
            assert!(is_launcher_image_url(good), "{good}");
        }
        for bad in [
            "http://cdn.modrinth.com/data/x/icon.png",
            "https://cdn.modrinth.com:8443/x.png",
            "https://user@cdn.modrinth.com/x.png",
            "https://cdn.modrinth.com.evil.example/x.png",
            "https://i.imgur.com/x.png",
            "data:image/png;base64,AAAA",
            "media/abc.png",
            "",
        ] {
            assert!(!is_launcher_image_url(bad), "{bad}");
        }
    }

    /// The launcher's CSP and this list are one decision: they must not drift apart.
    #[test]
    fn the_launcher_csp_allows_exactly_these_hosts() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/launcher/src-tauri/tauri.conf.json"
        );
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let img_src = config["app"]["security"]["csp"]["img-src"]
            .as_str()
            .expect("tauri.conf.json has a CSP img-src");
        let tokens: Vec<&str> = img_src.split_whitespace().collect();
        let listed: BTreeSet<&str> = tokens.iter().copied().collect();
        assert_eq!(listed.len(), tokens.len(), "duplicate sources: {img_src}");
        let mut expected: BTreeSet<String> = [
            "'self'",
            "asset:",
            "http://asset.localhost",
            "data:",
            "blob:",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        expected.extend(
            LAUNCHER_IMAGE_HOSTS
                .iter()
                .map(|host| format!("https://{host}")),
        );
        let listed: BTreeSet<String> = listed.into_iter().map(str::to_owned).collect();
        assert_eq!(listed, expected);
    }
}
