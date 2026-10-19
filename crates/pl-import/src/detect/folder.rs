//! `.minecraft`-style folders: Mojang's Minecraft Launcher, TLauncher and SKlauncher. One game
//! folder shared by every version (settings, servers, worlds, packs, one `mods` folder), the
//! installed versions under `versions/`, and Mojang's `launcher_profiles.json` (profiles may
//! name a game folder of their own).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use pl_mc::loaders::LoaderKind;
use pl_mc::server::ServerList;
use serde::Deserialize;

use super::{
    MINECRAFT_ID, TLAUNCHER_FOLDER_ID, count_entries, count_mods, is_pack_entry,
    is_screenshot_entry, mark_imported, modified_ms, read_small, sort_profiles, timestamp_ms,
};
use crate::journal::{ImportedFrom, same_path};
use crate::locate::Locations;
use crate::model::{ImportProfile, ImportSource, SharedData, SourceKind};
use crate::version_json::{MAX_VERSION_JSON, VersionInfo, display_name, read_version};

const MAX_SETTINGS: u64 = 4 * 1024 * 1024;
const MAX_SERVERS_DAT: u64 = 4 * 1024 * 1024;

pub(super) fn detect(locations: &Locations, imported: &[ImportedFrom]) -> Vec<ImportSource> {
    let mut out = Vec::new();
    let minecraft = locations.minecraft();
    let tlauncher = locations
        .tlauncher()
        .map(|dir| TLauncherSettings::read(&dir, minecraft.as_deref()));
    if let Some(root) = minecraft.as_deref()
        && looks_like_game_folder(root)
    {
        let tl = tlauncher
            .as_ref()
            .filter(|t| t.game_dir.as_deref().is_none_or(|d| same_path(d, root)));
        out.push(folder_source(MINECRAFT_ID, root, tl, None, imported));
    }
    // TLauncher can keep its game folder somewhere else.
    if let Some(settings) = tlauncher.as_ref()
        && let Some(dir) = settings.game_dir.as_deref()
        && minecraft.as_deref().is_none_or(|m| !same_path(m, dir))
        && looks_like_game_folder(dir)
    {
        out.push(folder_source(
            TLAUNCHER_FOLDER_ID,
            dir,
            Some(settings),
            Some(SourceKind::TLauncher),
            imported,
        ));
    }
    out
}

/// A game folder someone played in: versions installed, settings saved or worlds made.
fn looks_like_game_folder(root: &Path) -> bool {
    root.join("versions").is_dir()
        || root.join("options.txt").is_file()
        || root.join("saves").is_dir()
}

fn folder_source(
    id: &str,
    root: &Path,
    tlauncher: Option<&TLauncherSettings>,
    kind: Option<SourceKind>,
    imported: &[ImportedFrom],
) -> ImportSource {
    let tl_profiles = read_small(&root.join("TlauncherProfiles.json"), MAX_SETTINGS)
        .and_then(|text| serde_json::from_str::<TlProfiles>(&text).ok());
    let kind = kind.unwrap_or(
        if tlauncher.is_some_and(|t| t.found) || tl_profiles.is_some() {
            SourceKind::TLauncher
        } else if root.join("sklauncher").is_dir() {
            SourceKind::SkLauncher
        } else {
            SourceKind::Minecraft
        },
    );

    let shared = shared_data(root);
    let launcher_profiles = read_small(&root.join("launcher_profiles.json"), MAX_SETTINGS)
        .and_then(|text| serde_json::from_str::<LauncherProfiles>(&text).ok())
        .unwrap_or_default();
    let versions = installed_versions(root);
    let last_selected = tlauncher.and_then(|t| t.last_version.clone());
    let mut profiles = build_profiles(
        root,
        &versions,
        &launcher_profiles,
        last_selected.as_deref(),
    );
    mark_imported(root, &mut profiles, imported);
    sort_profiles(&mut profiles);

    let offline_name = (kind == SourceKind::TLauncher)
        .then(|| {
            tl_profiles
                .as_ref()
                .and_then(TlProfiles::offline_name)
                .or_else(|| tlauncher.and_then(TLauncherSettings::offline_name))
        })
        .flatten();
    let last_used_ms = profiles
        .iter()
        .filter_map(|p| p.last_used_ms)
        .max()
        .or_else(|| modified_ms(&root.join("options.txt")));
    ImportSource {
        id: id.to_owned(),
        kind,
        path: root.display().to_string(),
        shared: Some(shared),
        profiles,
        offline_name,
        last_used_ms,
        root: root.to_path_buf(),
    }
}

/// Counts of what every version shares; nothing big is opened.
pub(crate) fn shared_data(root: &Path) -> SharedData {
    let servers = read_servers(&root.join("servers.dat"))
        .map(|list| list.servers.iter().filter(|s| !s.hidden).count() as u32)
        .unwrap_or(0);
    SharedData {
        options: root.join("options.txt").is_file(),
        servers,
        worlds: crate::worlds::count_worlds(&root.join("saves")),
        resource_packs: count_entries(&root.join("resourcepacks"), is_pack_entry),
        shader_packs: count_entries(&root.join("shaderpacks"), is_pack_entry),
        screenshots: count_entries(&root.join("screenshots"), is_screenshot_entry),
    }
}

pub(crate) fn read_servers(path: &Path) -> Option<ServerList> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_SERVERS_DAT {
        return None;
    }
    ServerList::from_nbt(&std::fs::read(path).ok()?).ok()
}

fn installed_versions(root: &Path) -> HashMap<String, (VersionInfo, Option<i64>)> {
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(root.join("versions")) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().into_owned();
        let json = entry.path().join(format!("{folder}.json"));
        let Some(info) =
            read_small(&json, MAX_VERSION_JSON).and_then(|t| read_version(&t, &folder))
        else {
            continue;
        };
        out.insert(folder, (info, modified_ms(&json)));
    }
    out
}

fn build_profiles(
    root: &Path,
    versions: &HashMap<String, (VersionInfo, Option<i64>)>,
    launcher: &LauncherProfiles,
    last_selected: Option<&str>,
) -> Vec<ImportProfile> {
    let mut profiles = Vec::new();
    let mut covered: HashSet<String> = HashSet::new();
    let shared_mods = count_mods(&root.join("mods"));

    // Profiles the player made in Mojang's launcher (the "latest" ones stand for the newest
    // release, which the Projects instance already is).
    let mut keys: Vec<&String> = launcher.profiles.keys().collect();
    keys.sort();
    for key in keys {
        let profile = &launcher.profiles[key];
        let game_dir = profile
            .game_dir
            .as_deref()
            .map(PathBuf::from)
            .filter(|dir| dir.is_dir() && !same_path(dir, root));
        if profile.kind != "custom" && game_dir.is_none() {
            continue;
        }
        let version_id = profile.last_version_id.trim();
        if version_id.is_empty() || version_id.starts_with("latest-") {
            continue;
        }
        let info = versions
            .get(version_id)
            .map(|(info, _)| info.clone())
            .or_else(|| info_from_id(version_id));
        let Some(info) = info else { continue };
        covered.insert(version_id.to_owned());
        let name = if is_own_name(&profile.name, version_id) {
            profile.name.trim().to_owned()
        } else {
            version_name(&info, version_id)
        };
        let own_folder = game_dir.is_some();
        let dir = game_dir.unwrap_or_else(|| root.to_path_buf());
        let mod_files = if info.loader == LoaderKind::Vanilla {
            0
        } else if own_folder {
            count_mods(&dir.join("mods"))
        } else {
            shared_mods
        };
        profiles.push(profile_of(
            format!("profile:{key}"),
            name,
            &info,
            own_folder,
            dir,
            mod_files,
            (
                profile.last_used.as_deref().and_then(timestamp_ms),
                versions
                    .get(version_id)
                    .and_then(|(_, installed)| *installed),
            ),
        ));
    }

    // Installed versions nobody's profile points at. A vanilla version that only another
    // version stands on (a loader's base) was downloaded for it, not played on its own.
    let parents: HashSet<&str> = versions
        .values()
        .filter_map(|(info, _)| info.inherits_from.as_deref())
        .collect();
    let mut folders: Vec<&String> = versions.keys().collect();
    folders.sort();
    for folder in folders {
        if covered.contains(folder) {
            continue;
        }
        let (info, modified) = &versions[folder];
        let picked_last = last_selected.is_some_and(|v| v == folder);
        if info.loader == LoaderKind::Vanilla
            && !info.optifine
            && parents.contains(folder.as_str())
            && !picked_last
        {
            continue;
        }
        let name = version_name(info, folder);
        let mod_files = if info.loader == LoaderKind::Vanilla {
            0
        } else {
            shared_mods
        };
        // Only the version TLauncher started last was certainly played (when the game last saved
        // its settings); the others are ordered by when they were installed.
        let last_used = picked_last
            .then(|| modified_ms(&root.join("options.txt")).or(*modified))
            .flatten();
        profiles.push(profile_of(
            format!("version:{folder}"),
            name,
            info,
            false,
            root.to_path_buf(),
            mod_files,
            (last_used, *modified),
        ));
    }
    dedupe_names(&mut profiles);
    profiles
}

/// The name a version reads as: what it runs ("Fabric 1.21.4"), a loader this launcher does not
/// install by its own name ("LiteLoader 1.12.2"), else the version's folder name.
fn version_name(info: &VersionInfo, folder: &str) -> String {
    match (&info.unsupported, info.minecraft.as_deref()) {
        (Some(crate::model::Unsupported::Loader { name }), Some(mc)) if !name.is_empty() => {
            format!("{name} {mc}")
        }
        (Some(_), _) => folder.to_owned(),
        (None, mc) => display_name(mc, info.loader, folder),
    }
}

/// A launcher profile's name the player chose, not one an installer or launcher wrote
/// ("forge", "fabric-loader-1.21.4", "Latest release", TLauncher's own).
fn is_own_name(name: &str, version_id: &str) -> bool {
    const GENERIC: [&str; 9] = [
        "",
        "default",
        "(default)",
        "latest release",
        "latest snapshot",
        "tlauncher",
        "sklauncher",
        "forge",
        "fabric",
    ];
    let lower = name.trim().to_lowercase();
    !GENERIC.contains(&lower.as_str())
        && lower != version_id.to_lowercase()
        && !lower.starts_with("fabric-loader-")
        && !lower.starts_with("quilt-loader-")
}

/// What a version id says when its files are gone (a profile still names it): the patterns the
/// installers use. Anything else is left out.
fn info_from_id(id: &str) -> Option<VersionInfo> {
    let lower = id.to_ascii_lowercase();
    let (loader, loader_version, minecraft) = if let Some(rest) = lower
        .strip_prefix("fabric-loader-")
        .map(|r| (LoaderKind::Fabric, r))
        .or_else(|| {
            lower
                .strip_prefix("quilt-loader-")
                .map(|r| (LoaderKind::Quilt, r))
        }) {
        let (kind, rest) = rest;
        let (lv, mc) = rest.split_once('-')?;
        (kind, Some(lv.to_owned()), mc.to_owned())
    } else if let Some((mc, forge)) = lower.split_once("-forge") {
        let version = forge.trim_start_matches('-');
        let version = version.strip_suffix(&format!("-{mc}")).unwrap_or(version);
        (LoaderKind::Forge, Some(version.to_owned()), mc.to_owned())
    } else if let Some((mc, _)) = lower.split_once("-optifine") {
        (LoaderKind::Vanilla, None, mc.to_owned())
    } else {
        (LoaderKind::Vanilla, None, lower.clone())
    };
    if !crate::version_json::is_version_id(&minecraft) {
        return None;
    }
    Some(VersionInfo {
        id: id.to_owned(),
        minecraft: Some(minecraft),
        loader,
        loader_version: loader_version.filter(|v| !v.is_empty()),
        unsupported: None,
        optifine: lower.contains("optifine"),
        inherits_from: None,
    })
}

fn profile_of(
    id: String,
    name: String,
    info: &VersionInfo,
    own_folder: bool,
    game_dir: PathBuf,
    mod_files: u32,
    (last_used_ms, installed_ms): (Option<i64>, Option<i64>),
) -> ImportProfile {
    ImportProfile {
        id,
        name,
        minecraft: info.minecraft.clone(),
        loader: info.loader,
        loader_version: info.loader_version.clone(),
        unsupported: info.unsupported.clone(),
        without_optifine: info.optifine,
        own_folder,
        mod_files,
        last_used_ms,
        imported_as: None,
        game_dir,
        installed_ms,
    }
}

/// Two versions that read the same ("Fabric 1.21.4" from two loader versions) get the loader
/// version added, so the rows tell them apart.
fn dedupe_names(profiles: &mut [ImportProfile]) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for profile in profiles.iter() {
        *counts.entry(profile.name.to_lowercase()).or_default() += 1;
    }
    for profile in profiles.iter_mut() {
        if counts[&profile.name.to_lowercase()] > 1
            && let Some(version) = profile.loader_version.as_deref()
        {
            profile.name = format!("{} ({version})", profile.name);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LauncherProfiles {
    profiles: HashMap<String, LauncherProfile>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LauncherProfile {
    name: String,
    #[serde(rename = "type")]
    kind: String,
    last_version_id: String,
    game_dir: Option<String>,
    last_used: Option<String>,
}

/// TLauncher's account list. Only names are read; sign-ins stay where they are.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct TlProfiles {
    accounts: HashMap<String, TlAccount>,
    #[serde(rename = "selectedAccountUUID")]
    selected: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct TlAccount {
    username: Option<String>,
    display_name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

impl TlAccount {
    fn offline(&self) -> bool {
        self.kind
            .as_deref()
            .is_some_and(|k| k.eq_ignore_ascii_case("free") || k.eq_ignore_ascii_case("offline"))
    }

    fn name(&self) -> Option<&str> {
        self.display_name
            .as_deref()
            .or(self.username.as_deref())
            .map(str::trim)
            .filter(|n| valid_offline_name(n))
    }
}

impl TlProfiles {
    /// The selected offline account's name, else the first offline one's.
    fn offline_name(&self) -> Option<String> {
        let selected = self
            .selected
            .as_ref()
            .and_then(|id| self.accounts.get(id))
            .filter(|a| a.offline())
            .and_then(TlAccount::name);
        let mut ids: Vec<&String> = self.accounts.keys().collect();
        ids.sort();
        selected
            .or_else(|| {
                ids.into_iter()
                    .map(|id| &self.accounts[id])
                    .filter(|a| a.offline())
                    .find_map(TlAccount::name)
            })
            .map(str::to_owned)
    }
}

/// What TLauncher's settings file says: where its game folder is, the version it started last
/// and the offline name it signed in with.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct TLauncherSettings {
    /// Its settings file exists.
    pub found: bool,
    pub game_dir: Option<PathBuf>,
    pub last_version: Option<String>,
    account: Option<String>,
    account_type: Option<String>,
}

impl TLauncherSettings {
    /// `tlauncher-2.0.properties` in TLauncher's folder (or, from older versions, in the game
    /// folder).
    fn read(dir: &Path, minecraft: Option<&Path>) -> Self {
        let file = [Some(dir), minecraft]
            .into_iter()
            .flatten()
            .map(|d| d.join("tlauncher-2.0.properties"))
            .find(|p| p.is_file());
        let Some(text) = file.and_then(|f| read_small(&f, MAX_SETTINGS)) else {
            return Self::default();
        };
        let values = parse_properties(&text);
        let get = |key: &str| {
            values
                .get(key)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        Self {
            found: true,
            game_dir: get("minecraft.gamedir").map(PathBuf::from),
            last_version: get("login.version.game").or_else(|| get("login.version")),
            account: get("login.account"),
            account_type: get("login.account.type"),
        }
    }

    fn offline_name(&self) -> Option<String> {
        let offline = self
            .account_type
            .as_deref()
            .is_none_or(|k| k.eq_ignore_ascii_case("free") || k.eq_ignore_ascii_case("offline"));
        self.account
            .as_deref()
            .filter(|name| offline && valid_offline_name(name))
            .map(str::to_owned)
    }
}

/// Java `.properties` text: `key=value` or `key: value`, `#`/`!` comments, backslash escapes.
pub(crate) fn parse_properties(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let mut key = String::new();
        let mut chars = line.chars();
        let mut escaped = false;
        for c in chars.by_ref() {
            if escaped {
                key.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '=' || c == ':' {
                break;
            } else {
                key.push(c);
            }
        }
        out.insert(key.trim().to_owned(), unescape(chars.as_str().trim_start()));
    }
    out
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// What the game accepts as an offline name: 3 to 16 letters, digits or underscores.
pub(crate) fn valid_offline_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_properties() {
        let values = parse_properties(
            "#TLauncher\nminecraft.gamedir=C\\:\\\\Oyunlar\\\\mc\nlogin.account = Ejder_42\nlogin.account.type=FREE\nname\\=x: \\u00e7ok\n",
        );
        assert_eq!(values["minecraft.gamedir"], "C:\\Oyunlar\\mc");
        assert_eq!(values["login.account"], "Ejder_42");
        assert_eq!(values["name=x"], "çok");
    }

    #[test]
    fn offline_names_only() {
        assert!(valid_offline_name("Ejder_42"));
        assert!(!valid_offline_name("Çağrı"));
        assert!(!valid_offline_name("ab"));
        let tl: TlProfiles = serde_json::from_str(
            r#"{"accounts":{"a":{"username":"premium@mail","type":"MICROSOFT","accessToken":"x"},
                            "b":{"username":"Ejder_42","type":"FREE"}},
                "selectedAccountUUID":"a"}"#,
        )
        .unwrap();
        assert_eq!(tl.offline_name().as_deref(), Some("Ejder_42"));
        let settings = TLauncherSettings {
            found: true,
            account: Some("Steve".into()),
            account_type: Some("MOJANG".into()),
            ..Default::default()
        };
        assert_eq!(settings.offline_name(), None);
    }
}
