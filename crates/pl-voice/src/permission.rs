//! Who gets the microphone (design §6.1, §8.7).
//!
//! WebView2 asks the app about every permission a page wants (`PermissionRequested`). The voice
//! window answers each request itself and never lets the answer be saved in the profile, which
//! all the launcher's windows share: a "yes" for the voice page must not become a "yes" for the
//! main page, and no answer may outlive the rule. A window that is never shown could not show
//! WebView2's own question either.

use url::Url;

/// What a page asks for, as far as the voice window tells kinds apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Microphone,
    /// Camera, screen, location, notifications, clipboard, sensors and the rest: never.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Allow,
    Deny,
}

/// Where the launcher's own pages come from: WebView2's custom protocol on Windows (`http`, or
/// `https` with `useHttpsScheme`) and the `tauri://` scheme on macOS and Linux.
const APP_ORIGINS: &[&str] = &[
    "http://tauri.localhost",
    "https://tauri.localhost",
    "tauri://localhost",
];

/// Vite's dev server (`tauri dev`), in debug builds only.
#[cfg(debug_assertions)]
const DEV_ORIGIN: &str = "http://localhost:1420";

/// The origin of an absolute URL (`scheme://host[:port]`, lower case, without the scheme's
/// default port); `None` when it has none (relative, `data:`, `about:blank`) or names a user (an
/// address like `http://tauri.localhost@elsewhere/` is not ours).
pub fn origin(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    })
}

/// Whether `url` is one of the launcher's own pages.
pub fn is_app_origin(url: &str) -> bool {
    origin(url).is_some_and(|origin| APP_ORIGINS.contains(&origin.as_str()) || is_dev(&origin))
}

#[cfg(debug_assertions)]
fn is_dev(origin: &str) -> bool {
    origin == DEV_ORIGIN
}

#[cfg(not(debug_assertions))]
fn is_dev(_origin: &str) -> bool {
    false
}

/// The answer to `ask` from `asked_from` (WebView2 gives the asking origin) in `window`: the
/// microphone, for a launcher origin in the voice window (which may navigate to nothing else);
/// everything else is denied. The asking origin counts, not the window's address: right after the
/// window is built that is still `about:blank` (VOICE-0).
pub fn decide(window: &str, asked_from: &str, ask: Ask) -> Answer {
    if window == crate::VOICE_WINDOW && ask == Ask::Microphone && is_app_origin(asked_from) {
        Answer::Allow
    } else {
        Answer::Deny
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VOICE_WINDOW;

    const PAGE: &str = "http://tauri.localhost/voice.html";

    #[test]
    fn the_voice_page_gets_the_microphone() {
        assert_eq!(decide(VOICE_WINDOW, PAGE, Ask::Microphone), Answer::Allow);
        assert_eq!(
            decide(
                VOICE_WINDOW,
                "tauri://localhost/voice.html",
                Ask::Microphone
            ),
            Answer::Allow
        );
    }

    #[test]
    fn nothing_else_is_ever_allowed() {
        assert_eq!(decide(VOICE_WINDOW, PAGE, Ask::Other), Answer::Deny);
        // The main window shares the profile, never the answer.
        assert_eq!(decide("main", PAGE, Ask::Microphone), Answer::Deny);
        // A frame or a page from elsewhere in the voice window.
        assert_eq!(
            decide(VOICE_WINDOW, "https://example.com/x", Ask::Microphone),
            Answer::Deny
        );
        assert_eq!(
            decide(
                VOICE_WINDOW,
                "http://tauri.localhost@example.com/",
                Ask::Microphone
            ),
            Answer::Deny
        );
        assert_eq!(
            decide(VOICE_WINDOW, "about:blank", Ask::Microphone),
            Answer::Deny
        );
    }

    #[test]
    fn origins_are_compared_as_origins() {
        assert_eq!(
            origin("HTTP://Tauri.LocalHost:80/voice.html?x#y").as_deref(),
            Some("http://tauri.localhost")
        );
        assert_eq!(
            origin("https://tauri.localhost:8443/").as_deref(),
            Some("https://tauri.localhost:8443")
        );
        assert_eq!(
            origin("TAURI://LOCALHOST/voice.html").as_deref(),
            Some("tauri://localhost")
        );
        assert_eq!(origin("voice.html"), None);
        assert_eq!(origin("data:text/html,hi"), None);
        // A user name in front of the host: the request is not from the app's page.
        assert_eq!(origin("http://tauri.localhost@example.com/"), None);
        assert!(!is_app_origin("http://tauri.localhost.example.com/"));
        assert!(!is_app_origin("http://tauri.localhost:8080/"));
        assert!(is_app_origin("https://tauri.localhost/index.html"));
    }

    #[test]
    fn the_dev_server_counts_in_debug_builds_only() {
        assert_eq!(
            is_app_origin("http://localhost:1420/voice.html"),
            cfg!(debug_assertions)
        );
        assert!(!is_app_origin("http://localhost:1421/"));
    }
}
