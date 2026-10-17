//! "Devam et": the world or server an instance played last, joined directly with 1.20+ quick
//! play.
//!
//! Evidence, most reliable first:
//! 1. The game's own quick play log (`<game>/quickPlay/log.json`). The launcher passes
//!    `--quickPlayPath` to every 1.20+ game, which then writes the world or server it joined.
//! 2. The game logs the launcher captured (`Connecting to <host>, <port>` for a server, the
//!    integrated server starting for a world) together with `saves/*/level.dat` (`LastPlayed`).
//!
//! A world saved clearly later than that evidence wins (the instance was played without it,
//! e.g. by another launcher). Servers get their name and icon from `servers.dat`.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::gamefiles::fsops::{MAX_ICON_BYTES, blocking, read_png_file, read_png_limited};
use crate::install::McContext;
use crate::launch::QuickPlay;
use crate::server::ServerList;
use crate::version::{read_local_version, referenced_features};

/// Evidence older than the newest saved world by more than this is not trusted.
const STALE_MARGIN_MS: i64 = 5 * 60 * 1000;
/// Captured logs read when the quick play log is missing (newest first).
const LOGS_TO_READ: usize = 2;
/// The game's default port.
const DEFAULT_PORT: u16 = 25565;

/// Where the game writes its quick play log (`${quickPlayPath}`).
pub fn quick_play_log_path(game_dir: &Path) -> PathBuf {
    game_dir.join("quickPlay").join("log.json")
}

/// Whether the installed version understands quick play into worlds and servers (1.20+).
/// `false` while its JSON is not on disk (the instance never started).
pub async fn supports_quick_play(ctx: &McContext, mc_version: &str) -> bool {
    match read_local_version(ctx, mc_version).await {
        Ok(Some(json)) => json.arguments.as_ref().is_some_and(|args| {
            let features = referenced_features(&args.game);
            features.contains("is_quick_play_singleplayer")
                && features.contains("is_quick_play_multiplayer")
        }),
        _ => false,
    }
}

/// Where "Devam et" takes the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResumeTarget {
    #[serde(rename_all = "camelCase")]
    World {
        /// Folder in `saves/` (what quick play takes).
        folder: String,
        /// The world's own name.
        name: String,
        last_played_ms: Option<i64>,
        /// `icon.png` of the world (at most 256 KB).
        icon_png: Option<Vec<u8>>,
    },
    #[serde(rename_all = "camelCase")]
    Server {
        host: String,
        port: Option<u16>,
        /// The name in the server list, else the address.
        name: String,
        last_played_ms: Option<i64>,
        /// The server's icon from the server list.
        icon_png: Option<Vec<u8>>,
    },
}

impl ResumeTarget {
    pub fn quick_play(&self) -> QuickPlay {
        match self {
            Self::World { folder, .. } => QuickPlay::Singleplayer {
                world: folder.clone(),
            },
            Self::Server { host, port, .. } => QuickPlay::Multiplayer {
                host: host.clone(),
                port: *port,
            },
        }
    }
}

/// Kind of a quick play log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickPlayKind {
    Singleplayer,
    Multiplayer,
    Realms,
}

/// The newest entry of the game's quick play log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickPlayEntry {
    pub kind: QuickPlayKind,
    /// World folder, server address or realm id.
    pub id: String,
    /// World or server name as the game showed it.
    pub name: String,
    pub last_played_ms: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEntry {
    #[serde(rename = "type")]
    kind: String,
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    last_played_time: Option<String>,
}

/// Undo Java string escapes (`\uXXXX`, `\\`, `\"`, `\n`...): the game stores ids with
/// `StringEscapeUtils.escapeJava`. Unknown escapes are kept as written.
pub fn unescape_java(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(decoded) if hex.len() == 4 => out.push(decoded),
                    _ => {
                        out.push_str("\\u");
                        out.push_str(&hex);
                    }
                }
            }
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some(other @ ('\\' | '"' | '\'' | '/')) => out.push(other),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The newest entry of a quick play log (a JSON list; the game writes the session's world).
pub fn parse_quick_play_log(text: &str) -> Option<QuickPlayEntry> {
    let entries: Vec<RawEntry> = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    entries
        .into_iter()
        .filter_map(|raw| {
            let kind = match raw.kind.as_str() {
                "singleplayer" => QuickPlayKind::Singleplayer,
                "multiplayer" => QuickPlayKind::Multiplayer,
                "realms" => QuickPlayKind::Realms,
                _ => return None,
            };
            let id = unescape_java(&raw.id);
            if id.trim().is_empty() {
                return None;
            }
            let last_played_ms = raw
                .last_played_time
                .and_then(|t| t.parse::<jiff::Timestamp>().ok())
                .map(|t| t.as_millisecond());
            Some(QuickPlayEntry {
                kind,
                id,
                name: raw.name,
                last_played_ms,
            })
        })
        .max_by_key(|entry| entry.last_played_ms)
}

/// `host`, `host:port`, `[v6]:port` → host and port. `None` for an empty or malformed address.
pub fn parse_server_address(raw: &str) -> Option<(String, Option<u16>)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(rest) = raw.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(port) => Some(port.parse().ok()?),
            None if after.is_empty() => None,
            None => return None,
        };
        return (!host.is_empty()).then(|| (host.to_owned(), port));
    }
    match raw.rsplit_once(':') {
        // More than one colon without brackets: a bare IPv6 address.
        Some((host, _)) if host.contains(':') => Some((raw.to_owned(), None)),
        Some((host, port)) => {
            let port = port.parse().ok()?;
            (!host.is_empty()).then(|| (host.to_owned(), Some(port)))
        }
        None => Some((raw.to_owned(), None)),
    }
}

/// Same server, ignoring case, a trailing dot and the default port.
pub fn same_server(a: (&str, Option<u16>), b: (&str, Option<u16>)) -> bool {
    let host = |h: &str| h.trim().trim_end_matches('.').to_ascii_lowercase();
    host(a.0) == host(b.0) && a.1.unwrap_or(DEFAULT_PORT) == b.1.unwrap_or(DEFAULT_PORT)
}

/// What a captured game log says the session did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// A singleplayer world was opened.
    World,
    /// A server was joined.
    Server { host: String, port: Option<u16> },
}

const CONNECTING: &str = "Connecting to ";
const INTEGRATED_SERVER: &str = "Starting integrated minecraft server";

/// The activity in one log line, if any.
fn line_activity(line: &str) -> Option<Activity> {
    if line.contains(INTEGRATED_SERVER) {
        return Some(Activity::World);
    }
    let (_, rest) = line.split_once(CONNECTING)?;
    // `Connecting to play.projects.gg, 25565` (also inside log4j XML, followed by `]]>`).
    let (host, rest) = rest.split_once(", ")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let port: u16 = digits.parse().ok()?;
    let host = host.trim();
    if host.is_empty() || host.contains(char::is_whitespace) {
        return None;
    }
    Some(Activity::Server {
        host: host.to_owned(),
        port: (port != DEFAULT_PORT).then_some(port),
    })
}

/// The last activity of a log (the session's final world or server).
pub fn last_activity(log: impl BufRead) -> Option<Activity> {
    let mut last = None;
    for line in log.split(b'\n').map_while(|line| line.ok()) {
        let line = String::from_utf8_lossy(&line);
        if let Some(activity) = line_activity(&line) {
            last = Some(activity);
        }
    }
    last
}

/// A world in `saves/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldCandidate {
    pub folder: String,
    pub name: String,
    pub last_played_ms: Option<i64>,
}

/// Everything the choice is made from (read from disk by [`resume_target`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evidence {
    /// The quick play log's newest entry, with the world it names when that still exists.
    pub quick_play: Option<(QuickPlayEntry, Option<WorldCandidate>)>,
    /// The most recently played world.
    pub newest_world: Option<WorldCandidate>,
    /// The newest captured log that shows any activity, and when it was last written.
    pub log: Option<(Activity, Option<i64>)>,
}

/// The choice before names and icons are added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    World(WorldCandidate),
    Server {
        host: String,
        port: Option<u16>,
        /// The name the game showed (quick play log).
        name: Option<String>,
        last_played_ms: Option<i64>,
    },
}

/// A world saved clearly after `when`.
fn newer_world(newest: Option<&WorldCandidate>, when: Option<i64>) -> Option<WorldCandidate> {
    let world = newest?;
    let saved = world.last_played_ms?;
    match when {
        Some(when) if saved <= when + STALE_MARGIN_MS => None,
        _ => Some(world.clone()),
    }
}

/// Decide where "Devam et" goes.
pub fn pick(evidence: Evidence) -> Option<Pick> {
    let Evidence {
        quick_play,
        newest_world,
        log,
    } = evidence;
    if let Some((entry, world)) = quick_play {
        let candidate = match entry.kind {
            QuickPlayKind::Singleplayer => world.map(Pick::World),
            QuickPlayKind::Multiplayer => {
                parse_server_address(&entry.id).map(|(host, port)| Pick::Server {
                    host,
                    port,
                    name: Some(entry.name.clone()).filter(|n| !n.trim().is_empty()),
                    last_played_ms: entry.last_played_ms,
                })
            }
            QuickPlayKind::Realms => None,
        };
        if let Some(candidate) = candidate {
            return Some(
                match newer_world(newest_world.as_ref(), entry.last_played_ms) {
                    Some(world) => Pick::World(world),
                    None => candidate,
                },
            );
        }
    }
    match log {
        Some((Activity::Server { host, port }, written)) => {
            Some(match newer_world(newest_world.as_ref(), written) {
                Some(world) => Pick::World(world),
                None => Pick::Server {
                    host,
                    port,
                    name: None,
                    last_played_ms: written,
                },
            })
        }
        Some((Activity::World, _)) | None => newest_world.map(Pick::World),
    }
}

fn world_candidate(saves: &Path, folder: &str) -> Option<WorldCandidate> {
    // Quick play log ids are folder names: never a path.
    if folder.is_empty() || folder.contains(['/', '\\']) || folder == "." || folder == ".." {
        return None;
    }
    let dir = saves.join(folder);
    let (name, last_played_ms) = crate::gamefiles::world_summary(&dir)?;
    Some(WorldCandidate {
        folder: folder.to_owned(),
        name,
        last_played_ms,
    })
}

fn newest_world(saves: &Path) -> Option<WorldCandidate> {
    std::fs::read_dir(saves)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|entry| world_candidate(saves, &entry.file_name().to_string_lossy()))
        .max_by_key(|world| world.last_played_ms)
}

fn log_evidence(logs: &[PathBuf]) -> Option<(Activity, Option<i64>)> {
    logs.iter().take(LOGS_TO_READ).find_map(|path| {
        let file = std::fs::File::open(path).ok()?;
        let written = file
            .metadata()
            .ok()
            .and_then(|m| crate::gamefiles::fsops::modified_ms(&m));
        last_activity(BufReader::new(file)).map(|activity| (activity, written))
    })
}

fn gather(game_dir: &Path, logs: &[PathBuf]) -> Evidence {
    let saves = game_dir.join("saves");
    let quick_play = std::fs::read_to_string(quick_play_log_path(game_dir))
        .ok()
        .and_then(|text| parse_quick_play_log(&text))
        .map(|entry| {
            let world = (entry.kind == QuickPlayKind::Singleplayer)
                .then(|| world_candidate(&saves, &entry.id))
                .flatten();
            (entry, world)
        });
    // The logs only matter when the quick play log says nothing usable.
    let usable_quick_play = quick_play
        .as_ref()
        .is_some_and(|(entry, world)| match entry.kind {
            QuickPlayKind::Singleplayer => world.is_some(),
            QuickPlayKind::Multiplayer => true,
            QuickPlayKind::Realms => false,
        });
    Evidence {
        log: if usable_quick_play {
            None
        } else {
            log_evidence(logs)
        },
        newest_world: newest_world(&saves),
        quick_play,
    }
}

fn server_from_list(
    game_dir: &Path,
    host: &str,
    port: Option<u16>,
) -> Option<(String, Option<Vec<u8>>)> {
    let bytes = std::fs::read(game_dir.join("servers.dat")).ok()?;
    let list = ServerList::from_nbt(&bytes).ok()?;
    let entry = list.servers.into_iter().find(|entry| {
        parse_server_address(&entry.ip).is_some_and(|(h, p)| same_server((&h, p), (host, port)))
    })?;
    let icon = entry
        .icon_png()
        .and_then(|png| read_png_limited(png.as_slice(), MAX_ICON_BYTES));
    let name = Some(entry.name.trim().to_owned()).filter(|n| !n.is_empty())?;
    Some((name, icon))
}

fn decorate(game_dir: &Path, pick: Pick) -> ResumeTarget {
    match pick {
        Pick::World(world) => ResumeTarget::World {
            icon_png: read_png_file(&game_dir.join("saves").join(&world.folder).join("icon.png")),
            folder: world.folder,
            name: world.name,
            last_played_ms: world.last_played_ms,
        },
        Pick::Server {
            host,
            port,
            name,
            last_played_ms,
        } => {
            let listed = server_from_list(game_dir, &host, port);
            let address = match port {
                Some(port) => format!("{host}:{port}"),
                None => host.clone(),
            };
            let (name, icon_png) = match listed {
                Some((listed_name, icon)) => (listed_name, icon),
                None => (name.unwrap_or(address), None),
            };
            ResumeTarget::Server {
                host,
                port,
                name,
                last_played_ms,
                icon_png,
            }
        }
    }
}

/// Where "Devam et" goes for `game_dir`. `logs` are the instance's captured game logs, newest
/// first. `None` when it never joined a world or a server.
pub async fn resume_target(game_dir: &Path, logs: Vec<PathBuf>) -> Result<Option<ResumeTarget>> {
    let game_dir = game_dir.to_path_buf();
    blocking(move || {
        let evidence = gather(&game_dir, &logs);
        Ok(pick(evidence).map(|pick| decorate(&game_dir, pick)))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    fn world(folder: &str, last_played_ms: i64) -> WorldCandidate {
        WorldCandidate {
            folder: folder.into(),
            name: folder.into(),
            last_played_ms: Some(last_played_ms),
        }
    }

    fn entry(kind: QuickPlayKind, id: &str, at: i64) -> QuickPlayEntry {
        QuickPlayEntry {
            kind,
            id: id.into(),
            name: "Projects".into(),
            last_played_ms: Some(at),
        }
    }

    #[test]
    fn quick_play_log_entries() {
        let log = r#"[{"type":"singleplayer","id":"Yeni D\\u00fcnya","name":"Yeni Dünya","gamemode":"survival","lastPlayedTime":"2026-10-16T18:20:31.112Z"}]"#;
        let parsed = parse_quick_play_log(log).unwrap();
        assert_eq!(parsed.kind, QuickPlayKind::Singleplayer);
        assert_eq!(parsed.id, "Yeni Dünya");
        assert_eq!(parsed.name, "Yeni Dünya");
        assert_eq!(
            parsed.last_played_ms,
            Some(
                "2026-10-16T18:20:31.112Z"
                    .parse::<jiff::Timestamp>()
                    .unwrap()
                    .as_millisecond()
            )
        );
        // Several entries: the newest wins; unknown kinds and empty ids are skipped.
        let log = r#"[
            {"type":"multiplayer","id":"old.example.com","name":"Old","gamemode":"survival","lastPlayedTime":"2026-10-01T10:00:00Z"},
            {"type":"multiplayer","id":"play.projects.gg","name":"Projects","gamemode":"survival","lastPlayedTime":"2026-10-15T10:00:00Z"},
            {"type":"party","id":"x","name":"x","gamemode":"survival","lastPlayedTime":"2026-10-16T10:00:00Z"},
            {"type":"singleplayer","id":"","name":"x","gamemode":"survival","lastPlayedTime":"2026-10-16T11:00:00Z"}
        ]"#;
        let parsed = parse_quick_play_log(log).unwrap();
        assert_eq!(parsed.kind, QuickPlayKind::Multiplayer);
        assert_eq!(parsed.id, "play.projects.gg");
        assert!(parse_quick_play_log("not json").is_none());
        assert!(parse_quick_play_log("[]").is_none());
    }

    #[test]
    fn java_escapes() {
        assert_eq!(unescape_java(r"Dünya şato"), "Dünya şato");
        assert_eq!(unescape_java(r#"a\\b \"c\""#), r#"a\b "c""#);
        assert_eq!(unescape_java(r"bad \uZZ end \q"), r"bad \uZZ end \q");
        assert_eq!(unescape_java("trailing\\"), "trailing\\");
        assert_eq!(unescape_java("plain"), "plain");
    }

    #[test]
    fn server_addresses() {
        assert_eq!(
            parse_server_address("play.projects.gg"),
            Some(("play.projects.gg".into(), None))
        );
        assert_eq!(
            parse_server_address(" mc.example.com:25570 "),
            Some(("mc.example.com".into(), Some(25570)))
        );
        assert_eq!(
            parse_server_address("[2001:db8::1]:25566"),
            Some(("2001:db8::1".into(), Some(25566)))
        );
        assert_eq!(
            parse_server_address("2001:db8::1"),
            Some(("2001:db8::1".into(), None))
        );
        assert_eq!(parse_server_address("host:notaport"), None);
        assert_eq!(parse_server_address(""), None);
        assert!(same_server(
            ("Play.Projects.GG.", None),
            ("play.projects.gg", Some(25565))
        ));
        assert!(!same_server(
            ("play.projects.gg", Some(25566)),
            ("play.projects.gg", None)
        ));
    }

    #[test]
    fn log_activity() {
        let log = "\
[18:01:02] [Render thread/INFO]: Setting user: Oyuncu
[18:02:10] [Server thread/INFO]: Starting integrated minecraft server version 1.21.1
[18:20:00] [Render thread/INFO]: Connecting to play.projects.gg, 25565
[18:40:00] [Render thread/INFO]: Stopping!
";
        assert_eq!(
            last_activity(log.as_bytes()),
            Some(Activity::Server {
                host: "play.projects.gg".into(),
                port: None
            })
        );
        let xml = r#"<log4j:Event logger="ewz" level="INFO" thread="Render thread"><log4j:Message><![CDATA[Connecting to mc.example.com, 25570]]></log4j:Message></log4j:Event>
<log4j:Event logger="x" level="INFO" thread="Server thread"><log4j:Message><![CDATA[Starting integrated minecraft server version 1.20.4]]></log4j:Message></log4j:Event>"#;
        assert_eq!(last_activity(xml.as_bytes()), Some(Activity::World));
        let only_server = "[1] Connecting to mc.example.com, 25570]]>";
        assert_eq!(
            last_activity(only_server.as_bytes()),
            Some(Activity::Server {
                host: "mc.example.com".into(),
                port: Some(25570)
            })
        );
        assert_eq!(last_activity("[12:00] Stopping!".as_bytes()), None);
    }

    #[test]
    fn picks_quick_play_first() {
        let now = 100 * HOUR;
        let server = Evidence {
            quick_play: Some((
                entry(QuickPlayKind::Multiplayer, "play.projects.gg", now),
                None,
            )),
            newest_world: Some(world("Yeni Dünya", now - HOUR)),
            log: None,
        };
        assert_eq!(
            pick(server),
            Some(Pick::Server {
                host: "play.projects.gg".into(),
                port: None,
                name: Some("Projects".into()),
                last_played_ms: Some(now),
            })
        );
        // A world saved a minute after joining it is the same session.
        let same = Evidence {
            quick_play: Some((
                entry(QuickPlayKind::Singleplayer, "Ada", now),
                Some(world("Ada", now + 60_000)),
            )),
            newest_world: Some(world("Ada", now + 60_000)),
            log: None,
        };
        assert_eq!(pick(same), Some(Pick::World(world("Ada", now + 60_000))));
        // The quick play log is stale: a world was played hours later.
        let stale = Evidence {
            quick_play: Some((
                entry(QuickPlayKind::Multiplayer, "play.projects.gg", now),
                None,
            )),
            newest_world: Some(world("Yeni Dünya", now + 3 * HOUR)),
            log: None,
        };
        assert_eq!(
            pick(stale),
            Some(Pick::World(world("Yeni Dünya", now + 3 * HOUR)))
        );
        // A world the log names but that was deleted: fall back to the logs and saves.
        let deleted = Evidence {
            quick_play: Some((entry(QuickPlayKind::Singleplayer, "Gone", now), None)),
            newest_world: Some(world("Other", now - HOUR)),
            log: None,
        };
        assert_eq!(pick(deleted), Some(Pick::World(world("Other", now - HOUR))));
    }

    #[test]
    fn picks_from_logs_and_saves() {
        let now = 100 * HOUR;
        let server = Activity::Server {
            host: "mc.example.com".into(),
            port: Some(25570),
        };
        let from_log = Evidence {
            quick_play: None,
            newest_world: Some(world("Eski", now - 5 * HOUR)),
            log: Some((server.clone(), Some(now))),
        };
        assert_eq!(
            pick(from_log),
            Some(Pick::Server {
                host: "mc.example.com".into(),
                port: Some(25570),
                name: None,
                last_played_ms: Some(now),
            })
        );
        let world_later = Evidence {
            quick_play: None,
            newest_world: Some(world("Yeni", now + HOUR)),
            log: Some((server, Some(now))),
        };
        assert_eq!(
            pick(world_later),
            Some(Pick::World(world("Yeni", now + HOUR)))
        );
        let world_session = Evidence {
            quick_play: None,
            newest_world: Some(world("Ada", now)),
            log: Some((Activity::World, Some(now))),
        };
        assert_eq!(pick(world_session), Some(Pick::World(world("Ada", now))));
        assert_eq!(pick(Evidence::default()), None);
        let realms_only = Evidence {
            quick_play: Some((entry(QuickPlayKind::Realms, "123", now), None)),
            ..Evidence::default()
        };
        assert_eq!(pick(realms_only), None);
    }

    fn level_dat(name: &str, last_played: i64) -> Vec<u8> {
        use std::collections::HashMap;
        let mut data = HashMap::new();
        data.insert("LevelName".to_owned(), fastnbt::Value::String(name.into()));
        data.insert("LastPlayed".to_owned(), fastnbt::Value::Long(last_played));
        let mut root = HashMap::new();
        root.insert("Data".to_owned(), fastnbt::Value::Compound(data));
        fastnbt::to_bytes(&fastnbt::Value::Compound(root)).unwrap()
    }

    #[tokio::test]
    async fn target_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let saves = game.join("saves");
        let now = jiff::Timestamp::now().as_millisecond();
        for (folder, name, at) in [
            ("Yeni Dunya", "Yeni Dünya", now - HOUR),
            ("Eski", "Eski", now - 50 * HOUR),
        ] {
            std::fs::create_dir_all(saves.join(folder)).unwrap();
            std::fs::write(saves.join(folder).join("level.dat"), level_dat(name, at)).unwrap();
        }
        // Nothing but saves: the newest world.
        let target = resume_target(&game, Vec::new()).await.unwrap().unwrap();
        assert!(
            matches!(&target, ResumeTarget::World { folder, name, .. } if folder == "Yeni Dunya" && name == "Yeni Dünya")
        );
        assert_eq!(
            target.quick_play(),
            QuickPlay::Singleplayer {
                world: "Yeni Dunya".into()
            }
        );

        // The quick play log names the server; the server list gives its name and icon.
        let mut list = ServerList::default();
        let mut projects = crate::server::ServerListEntry::new("Projects", "play.projects.gg");
        let icon = crate::mods::test_support::png(64);
        projects.set_icon_png(Some(&icon));
        list.servers.push(projects);
        list.write(&game.join("servers.dat")).await.unwrap();
        let when = jiff::Timestamp::from_millisecond(now).unwrap();
        std::fs::create_dir_all(game.join("quickPlay")).unwrap();
        std::fs::write(
            quick_play_log_path(&game),
            format!(r#"[{{"type":"multiplayer","id":"play.projects.gg:25565","name":"Sunucu","gamemode":"survival","lastPlayedTime":"{when}"}}]"#),
        )
        .unwrap();
        let target = resume_target(&game, Vec::new()).await.unwrap().unwrap();
        assert_eq!(
            target,
            ResumeTarget::Server {
                host: "play.projects.gg".into(),
                port: Some(25565),
                name: "Projects".into(),
                last_played_ms: Some(now),
                icon_png: Some(icon),
            }
        );

        // Without the quick play log, the captured log decides.
        std::fs::remove_file(quick_play_log_path(&game)).unwrap();
        let log = dir.path().join("game-x-20261016-180000.log");
        std::fs::write(
            &log,
            "[18:20:00] [Render thread/INFO]: Connecting to mc.example.com, 25565\n",
        )
        .unwrap();
        let target = resume_target(&game, vec![log]).await.unwrap().unwrap();
        assert!(
            matches!(&target, ResumeTarget::Server { name, port: None, .. } if name == "mc.example.com")
        );

        let empty = dir.path().join("empty");
        assert_eq!(resume_target(&empty, Vec::new()).await.unwrap(), None);
    }
}
