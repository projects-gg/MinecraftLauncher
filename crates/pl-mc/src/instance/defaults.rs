//! What the launcher puts into a game directory on its own:
//!
//! - the Projects server in a server list that is missing or empty ([`seed_default_server`]).
//!   The caller records on the instance that this happened ([`Instance::default_server_seeded`])
//!   and never does it again, so a server the player removed stays removed;
//! - first-launch options when there is no `options.txt` yet ([`seed_options`]): Turkish as the
//!   game language and GUI scale 2. An existing `options.txt` (the player's or a modpack's) is
//!   never touched.
//!
//! Files that cannot be read are left alone; the error is returned for logging.
//!
//! [`Instance::default_server_seeded`]: crate::instance::Instance::default_server_seeded

use std::io::Read;
use std::path::Path;

use crate::error::{Error, Result};
use crate::gamefiles::fsops::blocking;
use crate::server::{ServerList, ServerListEntry};

/// Name of the server every new instance lists.
pub const DEFAULT_SERVER_NAME: &str = "Projects";
/// Address of the server every new instance lists.
pub const DEFAULT_SERVER_ADDRESS: &str = "play.projects.gg";
/// GUI scale of a new game folder: 2 ("Normal"). The game's own default ("Auto") picks 4 on a
/// 1080p screen, which feels too big.
pub const DEFAULT_GUI_SCALE: u32 = 2;
const SERVERS_FILE: &str = "servers.dat";
const OPTIONS_FILE: &str = "options.txt";
/// `version.json` inside a client jar is a few hundred bytes; anything far bigger is not it.
const MAX_VERSION_JSON: u64 = 64 * 1024;

/// What [`seed_default_server`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerSeed {
    /// The list was missing or had no visible server: the default server was added.
    Added,
    /// The list already has servers (the player's or a modpack's): left as it is.
    Kept,
}

/// Adds the Projects server to `<game_dir>/servers.dat` when the list is missing or shows no
/// server (entries hidden by "Direct Connection" do not count). A list with servers is kept as
/// it is; one that cannot be read is never overwritten (the error says why).
pub async fn seed_default_server(game_dir: &Path) -> Result<ServerSeed> {
    let path = game_dir.join(SERVERS_FILE);
    let mut list = ServerList::read(&path).await?;
    if list.servers.iter().any(|server| !server.hidden) {
        return Ok(ServerSeed::Kept);
    }
    tokio::fs::create_dir_all(game_dir)
        .await
        .map_err(|e| Error::io(game_dir, e))?;
    list.servers.insert(
        0,
        ServerListEntry::new(DEFAULT_SERVER_NAME, DEFAULT_SERVER_ADDRESS),
    );
    list.write(&path).await?;
    Ok(ServerSeed::Added)
}

/// Adds servers to the top of `<game_dir>/servers.dat` in the given order, skipping addresses
/// already listed (case-insensitively). Returns how many were added. A list that cannot be read
/// is never overwritten.
pub async fn add_servers(game_dir: &Path, servers: &[(String, String)]) -> Result<usize> {
    let path = game_dir.join(SERVERS_FILE);
    let mut list = ServerList::read(&path).await?;
    let mut added = 0;
    for (name, address) in servers.iter().rev() {
        if !list
            .servers
            .iter()
            .any(|server| server.ip.eq_ignore_ascii_case(address))
        {
            list.servers.insert(0, ServerListEntry::new(name, address));
            added += 1;
        }
    }
    if added > 0 {
        tokio::fs::create_dir_all(game_dir)
            .await
            .map_err(|e| Error::io(game_dir, e))?;
        list.write(&path).await?;
    }
    Ok(added)
}

/// Where a Minecraft version sits on the release timeline, as far as the options defaults care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Era {
    /// Classic, indev, alpha and beta versions.
    Ancient,
    /// A `1.x.y` release, pre-release or release candidate.
    Release(u32, u32, u32),
    /// A `YYwWWx` snapshot.
    Snapshot(u32, u32),
    /// Year-numbered versions (`26.1`, …): newer than every `1.x`.
    YearNumbered,
}

fn era(version: &str) -> Era {
    let version = version.trim();
    if let Some((year, rest)) = version.split_once('w')
        && let (Ok(year), Some(week)) = (year.parse::<u32>(), rest.get(..2))
        && let Ok(week) = week.parse::<u32>()
    {
        return Era::Snapshot(year, week);
    }
    let numbers: Vec<u32> = version
        .split(|c: char| !c.is_ascii_digit() && c != '.')
        .next()
        .unwrap_or_default()
        .split('.')
        .map_while(|part| part.parse().ok())
        .collect();
    match numbers.as_slice() {
        [1, minor, rest @ ..] => Era::Release(1, *minor, rest.first().copied().unwrap_or(0)),
        [major, ..] if *major >= 2 => Era::YearNumbered,
        _ => Era::Ancient,
    }
}

/// True for versions from `release` on, given the first snapshot (`year`, `week`) of that
/// release.
fn at_least(version: &str, release: (u32, u32, u32), snapshot: (u32, u32)) -> bool {
    match era(version) {
        Era::Ancient => false,
        Era::Release(a, b, c) => (a, b, c) >= release,
        Era::Snapshot(year, week) => (year, week) >= snapshot,
        Era::YearNumbered => true,
    }
}

/// The `lang` value Minecraft `version` uses for Turkish: `tr_tr` from 1.11 on (including its
/// snapshots from 16w32a and the year-numbered releases), `tr_TR` before.
pub fn turkish_language(version: &str) -> &'static str {
    if at_least(version, (1, 11, 0), (16, 32)) {
        "tr_tr"
    } else {
        "tr_TR"
    }
}

/// Versions from 1.19.3 (snapshot 22w42a) on show an accessibility onboarding screen to new
/// players. Their options data fixers turn it off when `options.txt` names no data version.
pub fn has_accessibility_onboarding(version: &str) -> bool {
    at_least(version, (1, 19, 3), (22, 42))
}

/// The data version (`world_version`) of a client jar's embedded `version.json`; `None` when
/// the jar has none or it cannot be read.
pub fn client_data_version(client_jar: &Path) -> Option<u32> {
    let file = std::fs::File::open(client_jar).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let entry = archive.by_name("version.json").ok()?;
    if entry.size() > MAX_VERSION_JSON {
        return None;
    }
    let mut text = String::new();
    entry
        .take(MAX_VERSION_JSON)
        .read_to_string(&mut text)
        .ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json.get("world_version")?.as_u64()?.try_into().ok()
}

/// What [`seed_options`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionsSeed {
    /// `options.txt` was written (`data_version` is the `version` line, when one was written).
    Written { data_version: Option<u32> },
    /// `options.txt` exists (the player's, a modpack's or the pack's): left as it is.
    Exists,
    /// Not written: the data version is unknown and this version has an accessibility
    /// onboarding the game would otherwise switch off.
    Skipped,
}

/// Right before the first launch, when `<game_dir>/options.txt` does not exist: writes
/// `version:<data version>` (read from `client_jar`), `lang:<Turkish>` and `guiScale:2`, and
/// nothing else; the game fills in the rest. An existing file is never touched.
///
/// The `version` line matters: without it the game runs its options data fixers from version 0
/// and one of them (1.19.3 on) turns the accessibility onboarding off for a new player. When the
/// data version cannot be read the file is written without it for older versions (they have no
/// onboarding) and not at all from 1.19.3 on.
pub async fn seed_options(
    game_dir: &Path,
    minecraft: &str,
    client_jar: &Path,
) -> Result<OptionsSeed> {
    let path = game_dir.join(OPTIONS_FILE);
    if tokio::fs::try_exists(&path)
        .await
        .map_err(|e| Error::io(&path, e))?
    {
        return Ok(OptionsSeed::Exists);
    }
    let jar = client_jar.to_path_buf();
    let data_version = blocking(move || Ok(client_data_version(&jar))).await?;
    if data_version.is_none() && has_accessibility_onboarding(minecraft) {
        return Ok(OptionsSeed::Skipped);
    }
    let mut content = String::new();
    if let Some(version) = data_version {
        content.push_str(&format!("version:{version}\n"));
    }
    content.push_str(&format!("lang:{}\n", turkish_language(minecraft)));
    content.push_str(&format!("guiScale:{DEFAULT_GUI_SCALE}\n"));
    blocking(move || {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        // `create_new` fails when the file exists: one appearing meanwhile is never replaced.
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(content.as_bytes())
                    .map_err(|e| Error::io(&path, e))?;
                Ok(OptionsSeed::Written { data_version })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(OptionsSeed::Exists),
            Err(e) => Err(Error::io(&path, e)),
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    async fn list_with(servers: Vec<ServerListEntry>, game: &Path) {
        let mut list = ServerList::default();
        list.servers.extend(servers);
        list.write(&game.join(SERVERS_FILE)).await.unwrap();
    }

    async fn names(game: &Path) -> Vec<String> {
        ServerList::read(&game.join(SERVERS_FILE))
            .await
            .unwrap()
            .servers
            .into_iter()
            .map(|s| format!("{}={}", s.name, s.ip))
            .collect()
    }

    #[tokio::test]
    async fn the_default_server_goes_into_empty_lists_only() {
        // No servers.dat at all.
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("minecraft");
        assert_eq!(seed_default_server(&game).await.unwrap(), ServerSeed::Added);
        assert_eq!(names(&game).await, ["Projects=play.projects.gg"]);

        // An empty list, or one with only a hidden "Direct Connection" entry.
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let mut hidden = ServerListEntry::new("", "direct.example");
        hidden.hidden = true;
        list_with(vec![hidden], game).await;
        assert_eq!(seed_default_server(game).await.unwrap(), ServerSeed::Added);
        assert_eq!(
            names(game).await,
            ["Projects=play.projects.gg", "=direct.example"]
        );

        // A list with servers (the player's or a modpack's) is kept.
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        list_with(
            vec![ServerListEntry::new("Modpack", "mc.example.org")],
            game,
        )
        .await;
        assert_eq!(seed_default_server(game).await.unwrap(), ServerSeed::Kept);
        assert_eq!(names(game).await, ["Modpack=mc.example.org"]);
    }

    #[tokio::test]
    async fn unreadable_server_lists_are_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        std::fs::write(game.join(SERVERS_FILE), b"not nbt at all").unwrap();
        assert!(seed_default_server(game).await.is_err());
        let servers = [("Projects".to_owned(), "play.projects.gg".to_owned())];
        assert!(add_servers(game, &servers).await.is_err());
        assert_eq!(
            std::fs::read(game.join(SERVERS_FILE)).unwrap(),
            b"not nbt at all"
        );
    }

    #[tokio::test]
    async fn pack_servers_are_added_on_top_once() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        list_with(vec![ServerListEntry::new("Mine", "PLAY.projects.gg")], game).await;
        let servers = [
            ("Projects".to_owned(), "play.projects.gg".to_owned()),
            ("Lobi".to_owned(), "lobi.projects.gg".to_owned()),
        ];
        assert_eq!(add_servers(game, &servers).await.unwrap(), 1);
        assert_eq!(
            names(game).await,
            ["Lobi=lobi.projects.gg", "Mine=PLAY.projects.gg"]
        );
        assert_eq!(add_servers(game, &servers).await.unwrap(), 0);
    }

    #[test]
    fn versions_gate_the_language_code_and_the_onboarding() {
        for (version, lang, onboarding) in [
            ("1.21.4", "tr_tr", true),
            ("1.19.3", "tr_tr", true),
            ("1.19.3-pre1", "tr_tr", true),
            ("22w42a", "tr_tr", true),
            ("1.19.2", "tr_tr", false),
            ("22w24a", "tr_tr", false),
            ("1.11", "tr_tr", false),
            ("1.11-pre1", "tr_tr", false),
            ("16w32a", "tr_tr", false),
            ("26.1", "tr_tr", true),
            ("26.1-snapshot-1", "tr_tr", true),
            ("1.10.2", "tr_TR", false),
            ("1.8.9", "tr_TR", false),
            ("16w21a", "tr_TR", false),
            ("b1.7.3", "tr_TR", false),
            ("a1.2.6", "tr_TR", false),
            ("", "tr_TR", false),
        ] {
            assert_eq!(turkish_language(version), lang, "{version}");
            assert_eq!(
                has_accessibility_onboarding(version),
                onboarding,
                "{version}"
            );
        }
    }

    /// A client jar holding `version.json` with `json` (or no `version.json` at all).
    fn client_jar(dir: &Path, json: Option<&str>) -> std::path::PathBuf {
        let path = dir.join("client.jar");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        writer
            .start_file("net/minecraft/client/Main.class", options)
            .unwrap();
        writer.write_all(b"class").unwrap();
        if let Some(json) = json {
            writer.start_file("version.json", options).unwrap();
            writer.write_all(json.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    fn options(game: &Path) -> String {
        std::fs::read_to_string(game.join(OPTIONS_FILE)).unwrap()
    }

    #[tokio::test]
    async fn first_launch_options_carry_the_data_version() {
        let dir = tempfile::tempdir().unwrap();
        let jar = client_jar(
            dir.path(),
            Some(r#"{"id":"1.21.1","world_version":3955,"java_version":21}"#),
        );
        assert_eq!(client_data_version(&jar), Some(3955));
        let game = dir.path().join("minecraft");
        assert_eq!(
            seed_options(&game, "1.21.1", &jar).await.unwrap(),
            OptionsSeed::Written {
                data_version: Some(3955)
            }
        );
        assert_eq!(options(&game), "version:3955\nlang:tr_tr\nguiScale:2\n");
        // Only once: from now on the file is the game's.
        assert_eq!(
            seed_options(&game, "1.21.1", &jar).await.unwrap(),
            OptionsSeed::Exists
        );
    }

    #[tokio::test]
    async fn without_a_data_version_only_old_versions_get_options() {
        let dir = tempfile::tempdir().unwrap();
        let no_json = client_jar(dir.path(), None);
        assert_eq!(client_data_version(&no_json), None);
        assert_eq!(client_data_version(&dir.path().join("missing.jar")), None);

        // 1.19.3 and newer: no file, so the game's own first start keeps the onboarding.
        let new = dir.path().join("new");
        assert_eq!(
            seed_options(&new, "1.21.4", &no_json).await.unwrap(),
            OptionsSeed::Skipped
        );
        assert!(!new.join(OPTIONS_FILE).exists());

        // Older versions have no onboarding: written without the version line.
        let old = dir.path().join("old");
        assert_eq!(
            seed_options(&old, "1.8.9", &no_json).await.unwrap(),
            OptionsSeed::Written { data_version: None }
        );
        assert_eq!(options(&old), "lang:tr_TR\nguiScale:2\n");

        // A jar whose version.json has no world_version counts as unknown.
        let other = tempfile::tempdir().unwrap();
        let broken = client_jar(other.path(), Some(r#"{"id":"1.20.1"}"#));
        assert_eq!(client_data_version(&broken), None);
    }

    #[tokio::test]
    async fn existing_options_are_never_touched() {
        let dir = tempfile::tempdir().unwrap();
        let jar = client_jar(dir.path(), Some(r#"{"world_version":3955}"#));
        // The player's options.
        let game = dir.path().join("player");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join(OPTIONS_FILE), "lang:en_us\nfov:0.5\n").unwrap();
        assert_eq!(
            seed_options(&game, "1.21.1", &jar).await.unwrap(),
            OptionsSeed::Exists
        );
        assert_eq!(options(&game), "lang:en_us\nfov:0.5\n");
        // A modpack's options.txt, even an empty one.
        let pack = dir.path().join("modpack");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join(OPTIONS_FILE), "").unwrap();
        assert_eq!(
            seed_options(&pack, "1.21.1", &jar).await.unwrap(),
            OptionsSeed::Exists
        );
        assert_eq!(options(&pack), "");
    }
}
