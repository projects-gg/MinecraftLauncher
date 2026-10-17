//! What the running game is doing, read from its log: the server it connects to, a
//! singleplayer world, or back in the menus. Pure and cheap, because every line of the game's
//! log passes through here.
//!
//! Lines the game writes (message part, without the `[time] [thread/LEVEL]:` prefix):
//! - `Connecting to play.projects.gg, 25565`: joining a server (every version since 1.7;
//!   1.17+ logs the address as typed, older versions the SRV target).
//! - `Starting integrated minecraft server version 1.21.1`: a singleplayer world opens.
//! - `Stopping server`: that world closes (back in the menus).
//! - `Couldn't connect to server`: a join failed (back in the menus).
//!
//! A normal disconnect from a server leaves no line, so the last server stays until the next
//! signal; the status never claims more than the log said.

use std::net::IpAddr;

use crate::protocol::LocalStatus;

const DEFAULT_PORT: u16 = 25565;

/// One meaningful log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogSignal {
    Connecting { host: String, port: u16 },
    Singleplayer,
    Menu,
}

/// A cheap pre-check: false for lines that can never be a [`LogSignal`].
pub fn may_signal(message: &str) -> bool {
    let message = message.trim_start();
    message.starts_with("Connecting to ")
        || message.starts_with("Starting integrated minecraft server")
        || message.starts_with("Stopping server")
        || message.starts_with("Couldn't connect to server")
}

/// Reads one log message.
pub fn log_signal(message: &str) -> Option<LogSignal> {
    let message = message.trim();
    if let Some(rest) = message.strip_prefix("Connecting to ") {
        let (host, port) = rest.rsplit_once(", ")?;
        let port = port.trim().parse::<u16>().ok()?;
        let host = host.trim();
        return (!host.is_empty()).then(|| LogSignal::Connecting {
            host: host.to_owned(),
            port,
        });
    }
    if message.starts_with("Starting integrated minecraft server") {
        return Some(LogSignal::Singleplayer);
    }
    if message == "Stopping server" || message.starts_with("Couldn't connect to server") {
        return Some(LogSignal::Menu);
    }
    None
}

/// `host` or `host:port` (default port left out) as friends may see it, or None for local
/// network addresses and anything that is not a plausible public server name.
pub fn server_label(host: &str, port: u16) -> Option<String> {
    let host = host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if let Ok(ip) = host.parse::<IpAddr>() {
        // A LAN world or the player's own machine: nothing friends could join. The service
        // takes host names and IPv4 only.
        if is_local(ip) || ip.is_ipv6() {
            return None;
        }
    } else if host == "localhost" || !valid_host_name(&host) {
        return None;
    }
    if port == 0 {
        return None;
    }
    Some(if port == DEFAULT_PORT {
        host
    } else {
        format!("{host}:{port}")
    })
}

fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
        }
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80
        }
    }
}

/// Labels of `[a-z0-9-]`, 1–63 characters, no leading or trailing hyphen (what the service
/// accepts).
fn valid_host_name(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// Follows one game session, from start to exit.
#[derive(Debug, Clone)]
pub struct GameActivity {
    status: LocalStatus,
    /// The server the launch joins by itself: it names the first connection, because older
    /// versions log the SRV target rather than the address the player knows.
    quick_play: Option<String>,
}

impl GameActivity {
    /// The game just started: playing, server not known yet. `quick_play` is the server the
    /// launch joins by itself (`host` or `host:port`), when it does.
    pub fn started(quick_play: Option<&str>) -> Self {
        Self {
            status: LocalStatus::Playing { host: None },
            quick_play: quick_play.map(str::to_owned),
        }
    }

    pub fn status(&self) -> &LocalStatus {
        &self.status
    }

    /// Reads one log message; returns the new status when it changed.
    pub fn observe(&mut self, message: &str) -> Option<LocalStatus> {
        let next = match log_signal(message)? {
            LogSignal::Connecting { host, port } => LocalStatus::Playing {
                host: match self.quick_play.take() {
                    Some(quick_play) => Some(quick_play),
                    None => server_label(&host, port),
                },
            },
            LogSignal::Singleplayer => {
                self.quick_play = None;
                LocalStatus::Singleplayer
            }
            LogSignal::Menu => LocalStatus::Playing { host: None },
        };
        if next == self.status {
            return None;
        }
        self.status = next.clone();
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing(host: &str) -> LocalStatus {
        LocalStatus::Playing {
            host: Some(host.to_owned()),
        }
    }

    #[test]
    fn reads_the_lines_that_matter() {
        assert_eq!(
            log_signal("Connecting to play.projects.gg, 25565"),
            Some(LogSignal::Connecting {
                host: "play.projects.gg".into(),
                port: 25565
            })
        );
        assert_eq!(
            log_signal("  Connecting to mc.hypixel.net, 25565 "),
            Some(LogSignal::Connecting {
                host: "mc.hypixel.net".into(),
                port: 25565
            })
        );
        assert_eq!(
            log_signal("Starting integrated minecraft server version 1.21.1"),
            Some(LogSignal::Singleplayer)
        );
        assert_eq!(log_signal("Stopping server"), Some(LogSignal::Menu));
        assert_eq!(
            log_signal("Couldn't connect to server"),
            Some(LogSignal::Menu)
        );
        for other in [
            "Connecting to",
            "Connecting to play.projects.gg",
            "Connecting to play.projects.gg, port",
            "Connecting to , 25565",
            "Stopping server now",
            "Loaded 7 advancements",
            "[CHAT] Connecting to play.projects.gg, 25565 is fun",
        ] {
            assert_eq!(log_signal(other), None, "{other}");
        }
        assert!(may_signal("Connecting to a, 1"));
        assert!(!may_signal("Setting user: Ayse"));
    }

    #[test]
    fn labels_only_public_servers() {
        assert_eq!(
            server_label("Play.Projects.GG.", 25565).as_deref(),
            Some("play.projects.gg")
        );
        assert_eq!(
            server_label("mc.example.net", 25570).as_deref(),
            Some("mc.example.net:25570")
        );
        assert_eq!(
            server_label("203.0.113.7", 25565).as_deref(),
            Some("203.0.113.7")
        );
        for local in [
            "localhost",
            "127.0.0.1",
            "192.168.1.20",
            "10.0.0.5",
            "172.20.1.1",
            "169.254.3.3",
            "0.0.0.0",
            "[::1]",
            "fe80::1",
            "2001:db8::1",
            "bad_host!",
            "-bad.example",
            "",
        ] {
            assert_eq!(server_label(local, 25565), None, "{local}");
        }
        assert_eq!(server_label("mc.example.net", 0), None);
    }

    #[test]
    fn follows_a_session() {
        let mut game = GameActivity::started(None);
        assert_eq!(game.status(), &LocalStatus::Playing { host: None });
        assert_eq!(game.observe("Loaded 7 advancements"), None);
        assert_eq!(
            game.observe("Starting integrated minecraft server version 1.21.1"),
            Some(LocalStatus::Singleplayer)
        );
        // The same world again is not a change.
        assert_eq!(
            game.observe("Starting integrated minecraft server version 1.21.1"),
            None
        );
        assert_eq!(
            game.observe("Stopping server"),
            Some(LocalStatus::Playing { host: None })
        );
        assert_eq!(
            game.observe("Connecting to mc.example.net, 25565"),
            Some(playing("mc.example.net"))
        );
        assert_eq!(game.observe("Connecting to mc.example.net, 25565"), None);
        assert_eq!(
            game.observe("Connecting to 192.168.1.4, 51234"),
            Some(LocalStatus::Playing { host: None })
        );
        assert_eq!(
            game.observe("Couldn't connect to server"),
            None,
            "already in the menus"
        );
    }

    #[test]
    fn the_quick_play_server_names_the_first_connection() {
        // Older versions log the SRV target; the launch knows the address the player knows.
        let mut game = GameActivity::started(Some("play.projects.gg"));
        assert_eq!(
            game.observe("Connecting to node7.hosting.example, 25577"),
            Some(playing("play.projects.gg"))
        );
        // Later connections are what the log says.
        assert_eq!(
            game.observe("Connecting to mc.example.net, 25565"),
            Some(playing("mc.example.net"))
        );
        // A singleplayer world first: the quick play target is forgotten.
        let mut game = GameActivity::started(Some("play.projects.gg"));
        game.observe("Starting integrated minecraft server version 1.21.1");
        assert_eq!(
            game.observe("Connecting to mc.example.net, 25565"),
            Some(playing("mc.example.net"))
        );
    }
}
