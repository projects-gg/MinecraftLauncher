//! Loading version JSONs from disk / Mojang and resolving `inheritsFrom` chains into one
//! [`ResolvedVersion`].
//!
//! Merge rules (child = loader profile, parent = the version it inherits from):
//! - libraries: the child's first, then the parent's that the child does not override; identity is
//!   `group:artifact[:classifier]` so a loader can replace a vanilla library of another version.
//!   Entries inside one JSON are never deduplicated (vanilla lists OS-specific variants twice).
//! - `arguments.game` / `arguments.jvm`: parent first, child appended.
//! - `mainClass`, `minecraftArguments`: the child's wins (a child `minecraftArguments` replaces
//!   the parent's game arguments entirely); every other missing field is inherited.
//! - client jar: like the official launcher, `versions/<jar>/<jar>.jar` where `<jar>` is the
//!   nearest `jar` field or else the leaf id (Forge 1.17+ expects `${version_name}.jar` on the
//!   classpath). Its content is the nearest `downloads.client`, stored once under the version that
//!   declares it and linked into the leaf folder.

use std::collections::HashSet;

use pl_net::{DownloadItem, DownloadOptions};

use crate::error::{Error, Result};
use crate::install::McContext;
use crate::version::manifest::{ManifestVersion, VersionManifest};
use crate::version::model::{
    Argument, Artifact, AssetIndexInfo, JavaVersionInfo, Library, LoggingConfig, VersionJson,
    parse_timestamp,
};
use crate::version::rules::referenced_features;

/// Deepest `inheritsFrom` chain accepted (OptiFine on Forge on vanilla is 3).
const MAX_CHAIN: usize = 16;

/// A version with its whole `inheritsFrom` chain merged; what install and launch work with.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedVersion {
    /// Id of the leaf profile (passed to the game as `${version_name}`).
    pub id: String,
    /// Ids from leaf to root (`["fabric-loader-0.16.5-1.21.1", "1.21.1"]`).
    pub chain: Vec<String>,
    /// The client jar on the classpath is `versions/<jar_id>/<jar_id>.jar`.
    pub jar_id: String,
    /// Download of the client jar, when known.
    pub client: Option<Artifact>,
    /// Version that declares [`ResolvedVersion::client`]; the download is stored under its
    /// folder and linked/copied to the `jar_id` folder when they differ.
    pub client_origin: Option<String>,
    /// `release`, `snapshot`, `old_beta`, `old_alpha`, ... (`${version_type}`).
    pub kind: String,
    pub release_time: Option<jiff::Timestamp>,
    pub main_class: String,
    /// Pre-1.13 game arguments (space separated).
    pub minecraft_arguments: Option<String>,
    pub game_arguments: Vec<Argument>,
    pub jvm_arguments: Vec<Argument>,
    pub libraries: Vec<Library>,
    pub asset_index: Option<AssetIndexInfo>,
    /// Legacy asset id for JSONs without `assetIndex`.
    pub assets: Option<String>,
    pub java_version: Option<JavaVersionInfo>,
    pub logging: Option<LoggingConfig>,
}

impl ResolvedVersion {
    /// Merge a chain of version JSONs, leaf first (`chain[0]` inherits from `chain[1]`...).
    pub fn from_chain(chain: Vec<VersionJson>) -> Result<Self> {
        let Some(leaf) = chain.first() else {
            return Err(Error::Invalid("empty version chain".into()));
        };
        let leaf_id = leaf.id.clone();
        let ids: Vec<String> = chain.iter().map(|v| v.id.clone()).collect();
        let (jar_id, client, client_origin) = pick_client_jar(&chain);

        let mut levels = chain.into_iter().rev();
        let mut merged = levels
            .next()
            .ok_or_else(|| Error::Invalid("empty version chain".into()))?;
        for child in levels {
            merged = merge_pair(child, merged);
        }

        let main_class = merged
            .main_class
            .clone()
            .filter(|m| !m.trim().is_empty())
            .ok_or_else(|| Error::Invalid(format!("version {leaf_id} has no mainClass")))?;
        let arguments = merged.arguments.unwrap_or_default();
        Ok(Self {
            id: leaf_id,
            chain: ids,
            jar_id,
            client,
            client_origin,
            kind: merged.kind.unwrap_or_else(|| "release".to_owned()),
            release_time: merged.release_time.as_deref().and_then(parse_timestamp),
            main_class,
            minecraft_arguments: merged.minecraft_arguments,
            game_arguments: arguments.game,
            jvm_arguments: arguments.jvm,
            libraries: merged.libraries,
            asset_index: merged.asset_index,
            assets: merged.assets,
            java_version: merged.java_version,
            logging: merged.logging.and_then(|l| l.client),
        })
    }

    /// Whether game arguments come from `minecraftArguments` (pre-1.13 style). JVM arguments then
    /// start from the launcher defaults.
    pub fn uses_legacy_arguments(&self) -> bool {
        self.minecraft_arguments.is_some()
    }

    /// Asset index id (`${assets_index_name}`): `assetIndex.id`, else `assets`, else `legacy`.
    pub fn assets_id(&self) -> &str {
        self.asset_index
            .as_ref()
            .map(|a| a.id.as_str())
            .or(self.assets.as_deref())
            .unwrap_or("legacy")
    }

    /// Mojang runtime component; versions without `javaVersion` predate it and need Java 8.
    pub fn java_component(&self) -> &str {
        self.java_version
            .as_ref()
            .and_then(|j| j.component.as_deref())
            .filter(|c| !c.is_empty())
            .unwrap_or("jre-legacy")
    }

    /// Required Java major version (8 when unspecified).
    pub fn java_major(&self) -> u32 {
        self.java_version
            .as_ref()
            .and_then(|j| j.major_version)
            .unwrap_or_else(|| component_major(self.java_component()).unwrap_or(8))
    }

    /// Whether the version understands 1.20+ quick play arguments.
    pub fn supports_quick_play(&self) -> bool {
        referenced_features(&self.game_arguments).contains("is_quick_play_multiplayer")
    }

    /// Whether `inheritsFrom` was involved (a loader profile).
    pub fn is_inherited(&self) -> bool {
        self.chain.len() > 1
    }
}

/// Java major version of well-known Mojang runtime components.
pub fn component_major(component: &str) -> Option<u32> {
    match component {
        "jre-legacy" => Some(8),
        "java-runtime-alpha" => Some(16),
        "java-runtime-beta" | "java-runtime-gamma" | "java-runtime-gamma-snapshot" => Some(17),
        "java-runtime-delta" => Some(21),
        "java-runtime-epsilon" => Some(25),
        _ => None,
    }
}

/// `(jar id, client download, id of the version declaring that download)`.
fn pick_client_jar(chain: &[VersionJson]) -> (String, Option<Artifact>, Option<String>) {
    let client_of = |v: &VersionJson| v.downloads.get("client").map(|c| (c.clone(), v.id.clone()));
    let nearest = |from: usize| chain[from..].iter().find_map(client_of);
    let explicit = chain.iter().enumerate().find_map(|(index, level)| {
        level
            .jar
            .as_deref()
            .map(str::trim)
            .filter(|j| !j.is_empty())
            .map(|jar| (index, jar.to_owned()))
    });
    match explicit {
        Some((index, jar)) => {
            let (client, origin) = chain
                .iter()
                .find(|v| v.id == jar)
                .and_then(client_of)
                .or_else(|| nearest(index))
                .unzip();
            (jar, client, origin)
        }
        None => {
            let leaf = chain.first().map(|v| v.id.clone()).unwrap_or_default();
            let (client, origin) = nearest(0).unzip();
            (leaf, client, origin)
        }
    }
}

/// Merge `child` over its already-resolved `parent`.
pub fn merge_pair(child: VersionJson, parent: VersionJson) -> VersionJson {
    let child_keys: HashSet<String> = child.libraries.iter().map(Library::dedup_key).collect();
    let mut libraries = child.libraries;
    libraries.extend(
        parent
            .libraries
            .into_iter()
            .filter(|lib| !child_keys.contains(&lib.dedup_key())),
    );

    let replaces_game_args = child.minecraft_arguments.is_some();
    let arguments = match (parent.arguments, child.arguments) {
        (Some(mut p), Some(c)) => {
            if replaces_game_args {
                p.game.clear();
            }
            p.game.extend(c.game);
            p.jvm.extend(c.jvm);
            Some(p)
        }
        (Some(mut p), None) => {
            if replaces_game_args {
                p.game.clear();
            }
            Some(p)
        }
        (None, c) => c,
    };

    let mut downloads = parent.downloads;
    downloads.extend(child.downloads);

    VersionJson {
        id: child.id,
        inherits_from: None,
        kind: child.kind.or(parent.kind),
        main_class: child.main_class.or(parent.main_class),
        minecraft_arguments: child.minecraft_arguments.or(parent.minecraft_arguments),
        arguments,
        libraries,
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        downloads,
        java_version: child.java_version.or(parent.java_version),
        logging: child.logging.or(parent.logging),
        release_time: child.release_time.or(parent.release_time),
        time: child.time.or(parent.time),
        jar: child.jar.or(parent.jar),
        compliance_level: child.compliance_level.or(parent.compliance_level),
        minimum_launcher_version: child
            .minimum_launcher_version
            .or(parent.minimum_launcher_version),
    }
}

/// Resolve a chain using `lookup` for every id (pure; used by tests and callers with their own
/// storage).
pub fn resolve_with<F>(id: &str, mut lookup: F) -> Result<ResolvedVersion>
where
    F: FnMut(&str) -> Result<VersionJson>,
{
    let mut chain: Vec<VersionJson> = Vec::new();
    let mut next = Some(id.to_owned());
    while let Some(current) = next {
        if chain.iter().any(|v| v.id == current) || chain.len() >= MAX_CHAIN {
            return Err(Error::Invalid(format!("inheritsFrom loop at {current}")));
        }
        let json = lookup(&current)?;
        next = json.inherits_from.clone().filter(|p| !p.trim().is_empty());
        chain.push(json);
    }
    ResolvedVersion::from_chain(chain)
}

/// Read `versions/<id>/<id>.json` if it exists.
pub async fn read_local_version(ctx: &McContext, id: &str) -> Result<Option<VersionJson>> {
    let path = ctx.dirs.version_json(id)?;
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mut json = VersionJson::from_slice(&bytes, &path.display().to_string())?;
            if json.id.is_empty() {
                json.id = id.to_owned();
            }
            Ok(Some(json))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(&path, e)),
    }
}

/// Make sure the Mojang JSON for `entry` is on disk and matches the manifest SHA-1 (downloading
/// it when missing or outdated), then parse it.
pub async fn fetch_version_json(ctx: &McContext, entry: &ManifestVersion) -> Result<VersionJson> {
    let path = ctx.dirs.version_json(&entry.id)?;
    let item = DownloadItem::new(&entry.url, &path).sha1(&entry.sha1);
    ctx.downloader
        .download(item, &DownloadOptions::default())
        .await?;
    read_local_version(ctx, &entry.id)
        .await?
        .ok_or_else(|| Error::VersionNotFound(entry.id.clone()))
}

/// Load one version JSON: Mojang versions are verified/downloaded against `manifest`; loader
/// profiles and offline mode (`manifest == None`) use the file on disk.
pub async fn load_version_json(
    ctx: &McContext,
    id: &str,
    manifest: Option<&VersionManifest>,
) -> Result<VersionJson> {
    if let Some(entry) = manifest.and_then(|m| m.find(id)) {
        return fetch_version_json(ctx, entry).await;
    }
    match read_local_version(ctx, id).await? {
        Some(json) => Ok(json),
        None if manifest.is_none() => {
            tracing::warn!(%id, "version JSON missing and no manifest available (offline)");
            Err(Error::Offline)
        }
        None => Err(Error::VersionNotFound(id.to_owned())),
    }
}

/// Resolve `id` and its whole `inheritsFrom` chain. Pass the manifest when online (vanilla JSONs
/// are then verified and downloaded as needed) and `None` offline (only files on disk are used).
pub async fn resolve_version(
    ctx: &McContext,
    id: &str,
    manifest: Option<&VersionManifest>,
) -> Result<ResolvedVersion> {
    let mut chain: Vec<VersionJson> = Vec::new();
    let mut next = Some(id.to_owned());
    while let Some(current) = next {
        if chain.iter().any(|v| v.id == current) || chain.len() >= MAX_CHAIN {
            return Err(Error::Invalid(format!("inheritsFrom loop at {current}")));
        }
        let json = load_version_json(ctx, &current, manifest).await?;
        next = json.inherits_from.clone().filter(|p| !p.trim().is_empty());
        chain.push(json);
    }
    ResolvedVersion::from_chain(chain)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::platform::{Arch, Os, Platform};
    use crate::version::rules::{Features, evaluate_arguments};

    fn vanilla() -> VersionJson {
        serde_json::from_str(
            r#"{
            "id": "1.21.1", "type": "release", "mainClass": "net.minecraft.client.main.Main",
            "releaseTime": "2024-08-08T12:24:45+00:00",
            "assetIndex": {"id": "17", "sha1": "aa", "size": 1, "totalSize": 2, "url": "https://x/17.json"},
            "assets": "17",
            "javaVersion": {"component": "java-runtime-delta", "majorVersion": 21},
            "downloads": {"client": {"sha1": "cc", "size": 3, "url": "https://x/client.jar"}},
            "logging": {"client": {"argument": "-Dlog4j.configurationFile=${path}",
                "file": {"id": "client-1.12.xml", "sha1": "dd", "size": 4, "url": "https://x/l.xml"}, "type": "log4j2-xml"}},
            "arguments": {
                "game": ["--username", "${auth_player_name}",
                    {"rules": [{"action": "allow", "features": {"is_quick_play_multiplayer": true}}],
                     "value": ["--quickPlayMultiplayer", "${quickPlayMultiplayer}"]}],
                "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"]
            },
            "libraries": [
                {"name": "org.ow2.asm:asm:9.3", "downloads": {"artifact": {"path": "org/ow2/asm/asm/9.3/asm-9.3.jar", "url": "https://libraries.minecraft.net/org/ow2/asm/asm/9.3/asm-9.3.jar", "sha1": "1", "size": 1}}},
                {"name": "org.lwjgl:lwjgl:3.3.3", "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar", "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar", "sha1": "2", "size": 2}}},
                {"name": "org.lwjgl:lwjgl:3.3.3:natives-windows", "rules": [{"action": "allow", "os": {"name": "windows"}}], "downloads": {"artifact": {"path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar", "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar", "sha1": "3", "size": 3}}}
            ]
        }"#,
        )
        .unwrap()
    }

    fn fabric() -> VersionJson {
        serde_json::from_str(
            r#"{
            "id": "fabric-loader-0.16.5-1.21.1", "inheritsFrom": "1.21.1", "type": "release",
            "releaseTime": "2024-09-10T00:00:00+0000",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": {"game": [], "jvm": ["-DFabricMcEmu= net.minecraft.client.main.Main "]},
            "libraries": [
                {"name": "org.ow2.asm:asm:9.7.1", "url": "https://maven.fabricmc.net/", "sha1": "f1", "size": 5},
                {"name": "net.fabricmc:fabric-loader:0.16.5", "url": "https://maven.fabricmc.net/"}
            ]
        }"#,
        )
        .unwrap()
    }

    fn lookup(map: HashMap<String, VersionJson>) -> impl FnMut(&str) -> Result<VersionJson> {
        move |id: &str| {
            map.get(id)
                .cloned()
                .ok_or_else(|| Error::VersionNotFound(id.to_owned()))
        }
    }

    #[test]
    fn fabric_inherits_vanilla() {
        let map = HashMap::from([
            ("1.21.1".to_owned(), vanilla()),
            ("fabric-loader-0.16.5-1.21.1".to_owned(), fabric()),
        ]);
        let r = resolve_with("fabric-loader-0.16.5-1.21.1", lookup(map)).unwrap();
        assert_eq!(r.id, "fabric-loader-0.16.5-1.21.1");
        assert_eq!(r.chain, ["fabric-loader-0.16.5-1.21.1", "1.21.1"]);
        assert_eq!(r.jar_id, "fabric-loader-0.16.5-1.21.1");
        assert_eq!(r.client_origin.as_deref(), Some("1.21.1"));
        assert_eq!(r.client.as_ref().unwrap().sha1.as_deref(), Some("cc"));
        assert_eq!(
            r.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(r.assets_id(), "17");
        assert_eq!(r.java_component(), "java-runtime-delta");
        assert_eq!(r.java_major(), 21);
        assert!(r.logging.is_some());
        assert!(r.supports_quick_play());
        assert!(r.is_inherited());
        assert!(!r.uses_legacy_arguments());

        // Child libraries first; the vanilla ASM is replaced by Fabric's newer one.
        let names: Vec<&str> = r.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "org.ow2.asm:asm:9.7.1",
                "net.fabricmc:fabric-loader:0.16.5",
                "org.lwjgl:lwjgl:3.3.3",
                "org.lwjgl:lwjgl:3.3.3:natives-windows"
            ]
        );

        // Parent arguments first, child appended.
        let p = Platform::new(Os::Windows, Arch::X86_64, "10.0");
        let jvm = evaluate_arguments(&r.jvm_arguments, &p, &Features::default());
        assert_eq!(
            jvm,
            [
                "-Djava.library.path=${natives_directory}",
                "-cp",
                "${classpath}",
                "-DFabricMcEmu= net.minecraft.client.main.Main "
            ]
        );
    }

    #[test]
    fn legacy_forge_style_profile() {
        let parent: VersionJson = serde_json::from_str(
            r#"{"id": "1.12.2", "type": "release", "mainClass": "net.minecraft.client.main.Main",
                "minecraftArguments": "--username ${auth_player_name} --version ${version_name}",
                "assetIndex": {"id": "1.12"}, "assets": "1.12",
                "downloads": {"client": {"sha1": "c1", "size": 1, "url": "https://x/1.12.2.jar"}},
                "libraries": [
                    {"name": "com.mojang:text2speech:1.10.3", "downloads": {"artifact": {"path": "a.jar", "url": "https://x/a.jar"}}},
                    {"name": "com.mojang:text2speech:1.10.3", "natives": {"windows": "natives-windows"}, "downloads": {"classifiers": {"natives-windows": {"path": "b.jar", "url": "https://x/b.jar"}}}},
                    {"name": "org.lwjgl.lwjgl:lwjgl:2.9.4-nightly-20150209", "rules": [{"action": "allow"}, {"action": "disallow", "os": {"name": "osx"}}]},
                    {"name": "org.lwjgl.lwjgl:lwjgl:2.9.2-nightly-20140822", "rules": [{"action": "allow", "os": {"name": "osx"}}]}
                ]}"#,
        )
        .unwrap();
        let forge: VersionJson = serde_json::from_str(
            r#"{"id": "1.12.2-forge-14.23.5.2860", "inheritsFrom": "1.12.2", "jar": "1.12.2",
                "mainClass": "net.minecraft.launchwrapper.Launch",
                "minecraftArguments": "--username ${auth_player_name} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
                "libraries": [{"name": "net.minecraftforge:forge:1.12.2-14.23.5.2860", "url": "https://maven.minecraftforge.net/"},
                              {"name": "net.minecraft:launchwrapper:1.12"}]}"#,
        )
        .unwrap();
        let map = HashMap::from([
            ("1.12.2".to_owned(), parent),
            ("1.12.2-forge-14.23.5.2860".to_owned(), forge),
        ]);
        let r = resolve_with("1.12.2-forge-14.23.5.2860", lookup(map)).unwrap();
        assert_eq!(r.jar_id, "1.12.2");
        assert_eq!(r.client_origin.as_deref(), Some("1.12.2"));
        assert_eq!(r.client.as_ref().unwrap().sha1.as_deref(), Some("c1"));
        assert_eq!(r.main_class, "net.minecraft.launchwrapper.Launch");
        assert!(
            r.minecraft_arguments
                .as_ref()
                .unwrap()
                .contains("FMLTweaker")
        );
        // Same-name entries inside one JSON are kept (artifact + natives, OS variants).
        assert_eq!(r.libraries.len(), 6);
        assert_eq!(r.java_component(), "jre-legacy");
        assert_eq!(r.java_major(), 8);
        assert!(!r.supports_quick_play());
    }

    #[test]
    fn child_minecraft_arguments_replace_parent_game_arguments() {
        let parent = vanilla();
        let child: VersionJson = serde_json::from_str(
            r#"{"id": "custom", "inheritsFrom": "1.21.1", "minecraftArguments": "--username ${auth_player_name}"}"#,
        )
        .unwrap();
        let map = HashMap::from([("1.21.1".to_owned(), parent), ("custom".to_owned(), child)]);
        let r = resolve_with("custom", lookup(map)).unwrap();
        assert!(r.game_arguments.is_empty());
        assert_eq!(r.jvm_arguments.len(), 3);
        assert!(r.uses_legacy_arguments());
    }

    #[test]
    fn three_level_chain_and_loops() {
        let optifine: VersionJson = serde_json::from_str(
            r#"{"id": "of", "inheritsFrom": "fabric-loader-0.16.5-1.21.1", "arguments": {"game": ["--tweakClass", "optifine.OptiFineTweaker"]},
                "libraries": [{"name": "optifine:OptiFine:1.21.1_HD_U_J1"}]}"#,
        )
        .unwrap();
        let map = HashMap::from([
            ("1.21.1".to_owned(), vanilla()),
            ("fabric-loader-0.16.5-1.21.1".to_owned(), fabric()),
            ("of".to_owned(), optifine),
        ]);
        let r = resolve_with("of", lookup(map)).unwrap();
        assert_eq!(r.chain.len(), 3);
        assert_eq!(r.libraries[0].name, "optifine:OptiFine:1.21.1_HD_U_J1");
        let p = Platform::new(Os::Linux, Arch::X86_64, "6.1");
        let game = evaluate_arguments(&r.game_arguments, &p, &Features::default());
        assert_eq!(
            game,
            [
                "--username",
                "${auth_player_name}",
                "--tweakClass",
                "optifine.OptiFineTweaker"
            ]
        );

        let looping: VersionJson =
            serde_json::from_str(r#"{"id": "a", "inheritsFrom": "a", "mainClass": "x"}"#).unwrap();
        let map = HashMap::from([("a".to_owned(), looping)]);
        assert!(matches!(
            resolve_with("a", lookup(map)),
            Err(Error::Invalid(_))
        ));

        let missing: VersionJson =
            serde_json::from_str(r#"{"id": "b", "inheritsFrom": "nope", "mainClass": "x"}"#)
                .unwrap();
        let map = HashMap::from([("b".to_owned(), missing)]);
        assert!(matches!(
            resolve_with("b", lookup(map)),
            Err(Error::VersionNotFound(_))
        ));
    }

    #[tokio::test]
    async fn offline_resolution_reads_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = McContext::new(
            pl_net::Http::new(pl_net::HttpConfig::new("pl-mc-test")).unwrap(),
            crate::paths::DataDirs::new(tmp.path()),
        );
        for json in [vanilla(), fabric()] {
            let path = ctx.dirs.version_json(&json.id).unwrap();
            crate::paths::write_atomic(&path, &serde_json::to_vec(&json).unwrap())
                .await
                .unwrap();
        }
        let r = resolve_version(&ctx, "fabric-loader-0.16.5-1.21.1", None)
            .await
            .unwrap();
        assert_eq!(r.client_origin.as_deref(), Some("1.21.1"));
        assert!(matches!(
            resolve_version(&ctx, "1.8.9", None).await,
            Err(Error::Offline)
        ));
    }
}
