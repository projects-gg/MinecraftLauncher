//! What an installed version of a `.minecraft`-style folder is: its Minecraft version and mod
//! loader, read from `versions/<id>/<id>.json` the way the launchers wrote it (Mojang's launcher,
//! the loaders' installers, TLauncher's merged files with names of its own).

use std::sync::LazyLock;

use pl_mc::loaders::LoaderKind;
use regex::Regex;
use serde::Deserialize;

use crate::model::Unsupported;

/// Version JSONs are tens of kilobytes; anything far bigger is not one.
pub(crate) const MAX_VERSION_JSON: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VersionInfo {
    pub id: String,
    pub minecraft: Option<String>,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub unsupported: Option<Unsupported>,
    pub optifine: bool,
    pub inherits_from: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawVersion {
    id: String,
    inherits_from: Option<String>,
    jar: Option<String>,
    main_class: Option<String>,
    libraries: Vec<RawLibrary>,
    arguments: Option<RawArguments>,
    minecraft_arguments: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawLibrary {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawArguments {
    game: Vec<serde_json::Value>,
}

/// Minecraft version ids: releases, pre-releases and release candidates (`1.21.4`, `1.21-pre1`),
/// year-numbered releases (`26.1.2`), snapshots (`25w14a`) and the old alpha and beta ids.
static VERSION_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:\d+\.\d+(?:\.\d+)?(?:[-_ ](?:pre|rc|pre-release |release candidate )?\d+)?|\d{2}w\d{2}[a-z~]|[abc]\d+\.\d+(?:\.\d+)?(?:_\d+)?[a-z]?|rd-\d+|inf-\d+)$",
    )
    .expect("valid version regex")
});

/// A Minecraft version inside a longer name (`Fabric 1.21.4`, `ForgeOptiFine 1.20.1`).
static VERSION_IN_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^\d.])(\d+\.\d+(?:\.\d+)?|\d{2}w\d{2}[a-z])(?:$|[^\d.])")
        .expect("valid version regex")
});

pub(crate) fn is_version_id(text: &str) -> bool {
    VERSION_ID.is_match(text.trim())
}

/// The last Minecraft-looking version in a name. The last one, because a loader's own version
/// comes first in names such as `fabric-loader-0.16.9-1.21.4`.
pub(crate) fn version_in_name(name: &str) -> Option<String> {
    let mut found = None;
    let mut rest = name;
    while let Some(caps) = VERSION_IN_NAME.captures(rest) {
        let Some(m) = caps.get(1) else { break };
        found = Some(m.as_str().to_owned());
        rest = &rest[m.end()..];
    }
    found
}

/// Reads a version JSON. `folder` is the version's folder name (TLauncher renames ids).
pub(crate) fn read_version(text: &str, folder: &str) -> Option<VersionInfo> {
    let raw: RawVersion = serde_json::from_str(text).ok()?;
    let id = if raw.id.trim().is_empty() {
        folder.to_owned()
    } else {
        raw.id.trim().to_owned()
    };

    let mut loader = LoaderKind::Vanilla;
    let mut loader_version: Option<String> = None;
    let mut minecraft_hint: Option<String> = None;
    let mut unsupported: Option<Unsupported> = None;
    let mut optifine = false;
    let unsupported_loader = |name: &str| {
        Some(Unsupported::Loader {
            name: name.to_owned(),
        })
    };

    for library in &raw.libraries {
        let mut parts = library.name.split(':');
        let (Some(group), Some(artifact), Some(version)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        match (group, artifact) {
            ("net.fabricmc", "fabric-loader") => {
                set_loader(
                    &mut loader,
                    &mut loader_version,
                    LoaderKind::Fabric,
                    version,
                );
            }
            ("net.fabricmc", "intermediary") | ("org.quiltmc", "hashed") => {
                minecraft_hint.get_or_insert_with(|| version.to_owned());
            }
            ("org.quiltmc", "quilt-loader") => {
                set_loader(&mut loader, &mut loader_version, LoaderKind::Quilt, version);
            }
            ("net.neoforged", "neoforge") => {
                set_loader(
                    &mut loader,
                    &mut loader_version,
                    LoaderKind::NeoForge,
                    version,
                );
            }
            // NeoForge for 1.20.1 kept Forge's coordinates (`1.20.1-47.1.106`).
            ("net.neoforged", "forge") => {
                let (mc, own) = split_minecraft_prefix(version);
                set_loader(&mut loader, &mut loader_version, LoaderKind::NeoForge, own);
                if let Some(mc) = mc {
                    minecraft_hint = Some(mc.to_owned());
                }
            }
            ("net.minecraftforge", "forge" | "fmlloader" | "minecraftforge")
                if loader != LoaderKind::NeoForge =>
            {
                let (mc, own) = split_minecraft_prefix(version);
                set_loader(&mut loader, &mut loader_version, LoaderKind::Forge, own);
                if let Some(mc) = mc {
                    minecraft_hint = Some(mc.to_owned());
                }
            }
            ("optifine", _) => optifine = true,
            ("com.mumfrey", "liteloader") => unsupported = unsupported_loader("LiteLoader"),
            ("org.dimdev", "rift") => unsupported = unsupported_loader("Rift"),
            (group, _) if group.starts_with("net.legacyfabric") => {
                unsupported = unsupported_loader("Legacy Fabric");
            }
            (group, _) if group.starts_with("net.ornithemc") => {
                unsupported = unsupported_loader("Ornithe");
            }
            (group, _) if group.contains("babric") => {
                unsupported = unsupported_loader("Babric");
            }
            _ => {}
        }
    }

    // Arguments name Forge's and NeoForge's versions too (and OptiFine's tweaker).
    let mut args: Vec<String> = raw
        .arguments
        .iter()
        .flat_map(|a| a.game.iter())
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    if let Some(legacy) = raw.minecraft_arguments.as_deref() {
        args.extend(legacy.split_whitespace().map(str::to_owned));
    }
    let mut pairs = args.windows(2);
    while let Some([key, value]) = pairs.next() {
        match key.as_str() {
            "--fml.neoForgeVersion" => {
                set_loader(
                    &mut loader,
                    &mut loader_version,
                    LoaderKind::NeoForge,
                    value,
                );
            }
            "--fml.forgeVersion" if loader != LoaderKind::NeoForge => {
                set_loader(&mut loader, &mut loader_version, LoaderKind::Forge, value);
            }
            "--fml.mcVersion" => minecraft_hint = Some(value.clone()),
            "--tweakClass" if value.to_ascii_lowercase().contains("optifine") => optifine = true,
            _ => {}
        }
    }

    // A main class nobody here knows means a loader nobody here installs.
    if unsupported.is_none() && loader == LoaderKind::Vanilla {
        let main = raw.main_class.as_deref().unwrap_or_default();
        let known = main.is_empty()
            || main == "net.minecraft.client.main.Main"
            || main == "net.minecraft.launchwrapper.Launch"
            || main.starts_with("net.minecraft.client.Minecraft")
            || main.starts_with("com.mojang.");
        if !known {
            unsupported = unsupported_loader("");
        }
    }

    let minecraft = minecraft_hint
        .or_else(|| raw.inherits_from.clone().filter(|v| !v.trim().is_empty()))
        .or_else(|| raw.jar.clone().filter(|v| is_version_id(v)))
        .or_else(|| is_version_id(&id).then(|| id.clone()))
        .or_else(|| version_in_name(&id))
        .or_else(|| version_in_name(folder))
        .map(|v| v.trim().to_owned());
    if minecraft.is_none() && unsupported.is_none() {
        unsupported = Some(Unsupported::UnknownVersion);
    }
    Some(VersionInfo {
        id,
        minecraft,
        loader,
        loader_version: loader_version.map(|v| clean_loader_version(&v)),
        unsupported,
        optifine,
        inherits_from: raw.inherits_from.filter(|v| !v.trim().is_empty()),
    })
}

fn set_loader(
    loader: &mut LoaderKind,
    loader_version: &mut Option<String>,
    kind: LoaderKind,
    version: &str,
) {
    // NeoForge wins over the Forge coordinates its 1.20.1 files also carry.
    if *loader == LoaderKind::NeoForge && kind == LoaderKind::Forge {
        return;
    }
    *loader = kind;
    let version = version.trim();
    if !version.is_empty() {
        *loader_version = Some(version.to_owned());
    }
}

/// `1.20.1-47.3.0` → (`1.20.1`, `47.3.0`); `1.7.10-10.13.4.1614-1.7.10` → (`1.7.10`,
/// `10.13.4.1614`); a version without a Minecraft prefix stays as it is.
fn split_minecraft_prefix(version: &str) -> (Option<&str>, &str) {
    match version.split_once('-') {
        Some((mc, rest)) if is_version_id(mc) => {
            let rest = rest
                .strip_suffix(&format!("-{mc}"))
                .unwrap_or(rest)
                .trim_end_matches("-universal");
            (Some(mc), rest)
        }
        _ => (None, version),
    }
}

/// Loader versions as the installers take them (`0.16.9`, `47.3.0`), without classifiers.
fn clean_loader_version(version: &str) -> String {
    version
        .trim()
        .trim_end_matches(":client")
        .trim_end_matches(":universal")
        .to_owned()
}

/// `forge-47.2.0`, `neoforge-1.20.1-47.1.84`, `fabric-0.15.0`, `quilt-0.20.0` (CurseForge's
/// loader names) → loader and version, without a Minecraft prefix.
pub(crate) fn parse_loader_id(id: &str, minecraft: &str) -> Option<(LoaderKind, String)> {
    let (kind, version) = id.trim().split_once('-')?;
    let kind = match kind.to_ascii_lowercase().as_str() {
        "forge" => LoaderKind::Forge,
        "neoforge" => LoaderKind::NeoForge,
        "fabric" => LoaderKind::Fabric,
        "quilt" => LoaderKind::Quilt,
        _ => return None,
    };
    let version = version
        .strip_prefix(&format!("{minecraft}-"))
        .unwrap_or(version)
        .trim()
        .to_owned();
    (!version.is_empty()).then_some((kind, version))
}

/// The name an instance gets from what it runs ("Fabric 1.21.4", "Minecraft 1.8.9").
pub(crate) fn display_name(minecraft: Option<&str>, loader: LoaderKind, fallback: &str) -> String {
    match (minecraft, loader) {
        (Some(mc), LoaderKind::Vanilla) => format!("Minecraft {mc}"),
        (Some(mc), loader) => format!("{} {mc}", loader.display_name()),
        (None, _) => fallback.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(json: &str) -> VersionInfo {
        read_version(json, "folder").expect("readable")
    }

    #[test]
    fn mojangs_own_versions_are_vanilla() {
        let info = read(
            r#"{"id":"1.21.4","type":"release","mainClass":"net.minecraft.client.main.Main",
                "libraries":[{"name":"com.mojang:brigadier:1.3.10"}]}"#,
        );
        assert_eq!(info.minecraft.as_deref(), Some("1.21.4"));
        assert_eq!(info.loader, LoaderKind::Vanilla);
        assert_eq!(info.unsupported, None);
        let old = read(r#"{"id":"b1.7.3","mainClass":"net.minecraft.launchwrapper.Launch"}"#);
        assert_eq!(old.minecraft.as_deref(), Some("b1.7.3"));
        let snapshot = read(r#"{"id":"25w14a","mainClass":"net.minecraft.client.main.Main"}"#);
        assert_eq!(snapshot.minecraft.as_deref(), Some("25w14a"));
    }

    #[test]
    fn loader_installers_are_read_from_their_libraries() {
        let fabric = read(
            r#"{"id":"fabric-loader-0.16.9-1.21.4","inheritsFrom":"1.21.4",
                "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
                "libraries":[{"name":"net.fabricmc:intermediary:1.21.4"},
                             {"name":"net.fabricmc:fabric-loader:0.16.9"}]}"#,
        );
        assert_eq!(fabric.minecraft.as_deref(), Some("1.21.4"));
        assert_eq!(fabric.loader, LoaderKind::Fabric);
        assert_eq!(fabric.loader_version.as_deref(), Some("0.16.9"));

        let forge = read(
            r#"{"id":"1.20.1-forge-47.3.0","inheritsFrom":"1.20.1",
                "mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
                "arguments":{"game":["--launchTarget","forgeclient","--fml.forgeVersion","47.3.0",
                                     "--fml.mcVersion","1.20.1"]},
                "libraries":[{"name":"net.minecraftforge:fmlloader:1.20.1-47.3.0"}]}"#,
        );
        assert_eq!(forge.minecraft.as_deref(), Some("1.20.1"));
        assert_eq!(forge.loader, LoaderKind::Forge);
        assert_eq!(forge.loader_version.as_deref(), Some("47.3.0"));

        let legacy_forge = read(
            r#"{"id":"1.7.10-Forge10.13.4.1614-1.7.10","inheritsFrom":"1.7.10",
                "mainClass":"net.minecraft.launchwrapper.Launch",
                "minecraftArguments":"--tweakClass cpw.mods.fml.common.launcher.FMLTweaker",
                "libraries":[{"name":"net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10"}]}"#,
        );
        assert_eq!(legacy_forge.minecraft.as_deref(), Some("1.7.10"));
        assert_eq!(legacy_forge.loader, LoaderKind::Forge);
        assert_eq!(legacy_forge.loader_version.as_deref(), Some("10.13.4.1614"));

        let neoforge = read(
            r#"{"id":"neoforge-21.1.77","inheritsFrom":"1.21.1",
                "mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
                "arguments":{"game":["--fml.neoForgeVersion","21.1.77","--fml.mcVersion","1.21.1"]},
                "libraries":[{"name":"net.neoforged.fancymodloader:loader:4.0.31"},
                             {"name":"net.neoforged:neoforge:21.1.77:client"}]}"#,
        );
        assert_eq!(neoforge.minecraft.as_deref(), Some("1.21.1"));
        assert_eq!(neoforge.loader, LoaderKind::NeoForge);
        assert_eq!(neoforge.loader_version.as_deref(), Some("21.1.77"));

        let quilt = read(
            r#"{"id":"quilt-loader-0.27.1-1.21.4","inheritsFrom":"1.21.4",
                "libraries":[{"name":"org.quiltmc:hashed:1.21.4"},
                             {"name":"org.quiltmc:quilt-loader:0.27.1"}],
                "mainClass":"org.quiltmc.loader.impl.launch.knot.KnotClient"}"#,
        );
        assert_eq!(quilt.loader, LoaderKind::Quilt);
        assert_eq!(quilt.minecraft.as_deref(), Some("1.21.4"));
    }

    #[test]
    fn tlaunchers_merged_versions_keep_their_own_names() {
        // TLauncher writes whole version files under names of its own, without inheritsFrom.
        let fabric = read(
            r#"{"id":"Fabric 1.21.4","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
                "libraries":[{"name":"net.fabricmc:fabric-loader:0.16.10"},
                             {"name":"net.fabricmc:intermediary:1.21.4"}]}"#,
        );
        assert_eq!(fabric.minecraft.as_deref(), Some("1.21.4"));
        assert_eq!(fabric.loader, LoaderKind::Fabric);

        let forge_optifine = read(
            r#"{"id":"ForgeOptiFine 1.20.1","mainClass":"cpw.mods.bootstraplauncher.BootstrapLauncher",
                "libraries":[{"name":"net.minecraftforge:fmlloader:1.20.1-47.2.0"},
                             {"name":"optifine:OptiFine:1.20.1_HD_U_I6"}]}"#,
        );
        assert_eq!(forge_optifine.loader, LoaderKind::Forge);
        assert!(forge_optifine.optifine);
        assert_eq!(forge_optifine.minecraft.as_deref(), Some("1.20.1"));

        let optifine = read(
            r#"{"id":"1.8.9-OptiFine_HD_U_M5","inheritsFrom":"1.8.9",
                "mainClass":"net.minecraft.launchwrapper.Launch",
                "minecraftArguments":"--tweakClass optifine.OptiFineTweaker",
                "libraries":[{"name":"optifine:OptiFine:1.8.9_HD_U_M5"}]}"#,
        );
        assert_eq!(optifine.loader, LoaderKind::Vanilla);
        assert!(optifine.optifine);
        assert_eq!(optifine.unsupported, None);
        assert_eq!(optifine.minecraft.as_deref(), Some("1.8.9"));
    }

    #[test]
    fn loaders_this_launcher_does_not_install_are_named() {
        let lite = read(
            r#"{"id":"1.12.2-LiteLoader1.12.2","inheritsFrom":"1.12.2",
                "mainClass":"net.minecraft.launchwrapper.Launch",
                "libraries":[{"name":"com.mumfrey:liteloader:1.12.2-SNAPSHOT"}]}"#,
        );
        assert_eq!(
            lite.unsupported,
            Some(Unsupported::Loader {
                name: "LiteLoader".into()
            })
        );
        let legacy_fabric = read(
            r#"{"id":"fabric-loader-0.14.24-1.8.9","inheritsFrom":"1.8.9",
                "libraries":[{"name":"net.fabricmc:fabric-loader:0.14.24"},
                             {"name":"net.legacyfabric:intermediary:1.8.9"}]}"#,
        );
        assert_eq!(
            legacy_fabric.unsupported,
            Some(Unsupported::Loader {
                name: "Legacy Fabric".into()
            })
        );
        let custom = read(r#"{"id":"Lunar 1.8.9","mainClass":"com.moonsworth.lunar.Main"}"#);
        assert_eq!(
            custom.unsupported,
            Some(Unsupported::Loader { name: "".into() })
        );
        let nameless = read(r#"{"id":"my-pack","mainClass":"net.minecraft.client.main.Main"}"#);
        assert_eq!(nameless.unsupported, Some(Unsupported::UnknownVersion));
        assert_eq!(read_version("not json", "x"), None);
    }

    #[test]
    fn versions_inside_names() {
        assert_eq!(
            version_in_name("fabric-loader-0.16.9-1.21.4").as_deref(),
            Some("1.21.4")
        );
        assert_eq!(version_in_name("Fabric 1.21.4").as_deref(), Some("1.21.4"));
        assert_eq!(version_in_name("ATM9").as_deref(), None);
        assert!(is_version_id("1.21"));
        assert!(is_version_id("26.1.2"));
        assert!(is_version_id("1.21-pre1"));
        assert!(is_version_id("24w14a"));
        assert!(!is_version_id("Fabric 1.21.4"));
    }

    #[test]
    fn curseforge_loader_names() {
        assert_eq!(
            parse_loader_id("forge-47.2.0", "1.20.1"),
            Some((LoaderKind::Forge, "47.2.0".into()))
        );
        assert_eq!(
            parse_loader_id("neoforge-1.20.1-47.1.84", "1.20.1"),
            Some((LoaderKind::NeoForge, "47.1.84".into()))
        );
        assert_eq!(parse_loader_id("liteloader-1.12", "1.12.2"), None);
    }

    #[test]
    fn names_come_from_what_it_runs() {
        assert_eq!(
            display_name(Some("1.21.4"), LoaderKind::Fabric, "x"),
            "Fabric 1.21.4"
        );
        assert_eq!(
            display_name(Some("1.8.9"), LoaderKind::Vanilla, "x"),
            "Minecraft 1.8.9"
        );
        assert_eq!(display_name(None, LoaderKind::Forge, "my-pack"), "my-pack");
    }
}
