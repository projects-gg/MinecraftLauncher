//! Legacy Forge installers (1.5.2 – 1.12.2 up to 14.23.5.2847): `install_profile.json` holds an
//! `install` section and a complete `versionInfo` launcher profile.
//!
//! The profile is converted to what the core installer understands: our id, `inheritsFrom` and
//! `jar` set to the Minecraft version, server-only (`clientreq: false`) libraries dropped,
//! `files.minecraftforge.net/maven` URLs moved to `maven.minecraftforge.net`, and the universal
//! jar (shipped inside the installer) described as a local file. Libraries that the old installer
//! fetched as `.pack.xz` carry a `checksums` list; their plain jars (on the Forge Maven) must match
//! one of those SHA-1s, which is then written into the profile so later checks verify them.

use std::collections::HashMap;
use std::path::Path;

use pl_net::{DownloadItem, HashAlgo, hash_file};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::install::{InstallOptions, InstallStage, McContext};
use crate::loaders::common::{Progress, download, prepare_profile};
use crate::loaders::forge::Outcome;
use crate::loaders::forge::installer::{Extraction, LegacyProfile, extract};
use crate::paths::DataDirs;
use crate::version::{MavenCoord, maven_urls};

/// The converted profile and the work the installer still has to do.
#[derive(Debug, Clone)]
pub(crate) struct Conversion {
    pub profile: Value,
    pub universal: Universal,
    pub checked: Vec<CheckedLibrary>,
}

/// The universal jar: an installer entry installed as a library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Universal {
    pub entry: String,
    /// Maven path under `libraries/`.
    pub path: String,
    /// Index in the profile's `libraries`.
    pub index: usize,
}

/// A library whose jar must match one of the profile's `checksums`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CheckedLibrary {
    pub index: usize,
    pub path: String,
    pub urls: Vec<String>,
    pub checksums: Vec<String>,
}

/// `http://files.minecraftforge.net/maven/` → `https://maven.minecraftforge.net/`; other plain
/// HTTP repositories are upgraded to HTTPS.
pub(crate) fn normalize_forge_url(url: &str) -> String {
    let url = url.trim();
    for old in [
        "http://files.minecraftforge.net/maven",
        "https://files.minecraftforge.net/maven",
    ] {
        if let Some(rest) = url.strip_prefix(old) {
            return format!("https://maven.minecraftforge.net{rest}");
        }
    }
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_owned(),
    }
}

/// Convert a legacy profile for Minecraft `mc` into the launcher profile `id`.
pub(crate) fn convert(profile: &LegacyProfile, id: &str, mc: &str) -> Result<Conversion> {
    if let Some(minecraft) = profile.install.minecraft.as_deref()
        && minecraft != mc
    {
        return Err(Error::Invalid(format!(
            "this Forge installer is for Minecraft {minecraft}, not {mc}"
        )));
    }
    let universal_coord = MavenCoord::parse(&profile.install.path)?;
    let universal_path = universal_coord.path();
    let entry = profile.install.file_path.trim_start_matches('/').to_owned();
    if entry.is_empty() {
        return Err(Error::Invalid(
            "legacy install profile has no filePath".into(),
        ));
    }

    let mut info = profile.version_info.clone();
    let object = info
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("legacy versionInfo is not an object".into()))?;
    let raw_libraries = match object.remove("libraries") {
        Some(Value::Array(list)) => list,
        _ => Vec::new(),
    };

    let universal_artifact = |path: &str| json!({"artifact": {"path": path, "url": ""}});
    let mut libraries: Vec<Value> = Vec::with_capacity(raw_libraries.len());
    let mut universal_index = None;
    let mut checked = Vec::new();
    for raw in raw_libraries {
        let Value::Object(mut library) = raw else {
            continue;
        };
        let Some(name) = library
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        if library.get("clientreq").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let checksums: Vec<String> = match library.remove("checksums") {
            Some(Value::Array(list)) => list
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            _ => Vec::new(),
        };
        for key in ["clientreq", "serverreq", "comment"] {
            library.remove(key);
        }
        let index = libraries.len();
        let coord = MavenCoord::parse(&name).ok();
        let is_universal = name == profile.install.path
            || coord.as_ref().is_some_and(|c| c.path() == universal_path);
        if is_universal && universal_index.is_none() {
            library.remove("url");
            library.insert("downloads".into(), universal_artifact(&universal_path));
            universal_index = Some(index);
        } else {
            let url = library
                .get("url")
                .and_then(Value::as_str)
                .map(normalize_forge_url)
                .filter(|u| !u.is_empty());
            match &url {
                Some(url) => library.insert("url".into(), Value::from(url.as_str())),
                None => library.remove("url"),
            };
            if !checksums.is_empty()
                && let Some(coord) = &coord
            {
                let path = coord.path();
                checked.push(CheckedLibrary {
                    index,
                    urls: maven_urls(url.as_deref(), &path),
                    path,
                    checksums,
                });
            }
        }
        libraries.push(Value::Object(library));
    }

    // Profiles always list the universal jar; add it in front if one does not.
    let index = match universal_index {
        Some(index) => index,
        None => {
            let mut library = Map::new();
            library.insert("name".into(), Value::from(profile.install.path.as_str()));
            library.insert("downloads".into(), universal_artifact(&universal_path));
            libraries.insert(0, Value::Object(library));
            for library in &mut checked {
                library.index += 1;
            }
            0
        }
    };
    object.insert("libraries".into(), Value::Array(libraries));
    if object
        .get("jar")
        .and_then(Value::as_str)
        .is_none_or(|j| j.trim().is_empty())
    {
        object.insert("jar".into(), Value::from(mc));
    }
    let profile_value = prepare_profile(info, id, mc, "Forge versionInfo")?;
    Ok(Conversion {
        profile: profile_value,
        universal: Universal {
            entry,
            path: universal_path,
            index,
        },
        checked,
    })
}

/// Record the verified checksum and size of library `index`: in `downloads.artifact` when the
/// entry has one (the universal jar), else as `sha1`/`size` next to the coordinate.
pub(crate) fn set_library_file(profile: &mut Value, index: usize, sha1: &str, size: u64) {
    let Some(library) = profile
        .get_mut("libraries")
        .and_then(|l| l.get_mut(index))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let target = match library
        .get_mut("downloads")
        .and_then(|d| d.get_mut("artifact"))
        .and_then(Value::as_object_mut)
    {
        Some(artifact) => artifact,
        None => library,
    };
    target.insert("sha1".into(), Value::from(sha1));
    target.insert("size".into(), Value::from(size));
}

async fn matching_sha1(path: &Path, checksums: &[String]) -> Option<(String, u64)> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    if !meta.is_file() {
        return None;
    }
    let actual = hash_file(path, HashAlgo::Sha1).await.ok()?;
    checksums.contains(&actual).then_some((actual, meta.len()))
}

/// Run the legacy install: universal jar from the installer, checksum-listed libraries. Returns
/// the profile to write and the files for the install marker.
pub(crate) async fn install(
    ctx: &McContext,
    dirs: &DataDirs,
    installer: &Path,
    conversion: Conversion,
    options: &InstallOptions,
) -> Result<Outcome> {
    let Conversion {
        mut profile,
        universal,
        checked,
    } = conversion;
    let progress = Progress::new(options);
    progress.stage(InstallStage::Libraries);

    let universal_dest = dirs.library_file(&universal.path)?;
    let extracted = extract(
        installer,
        vec![Extraction {
            entry: universal.entry.clone(),
            dest: universal_dest.clone(),
            sha1: None,
        }],
    )
    .await?;
    let mut known = HashMap::new();
    if let Some(file) = extracted.first() {
        set_library_file(&mut profile, universal.index, &file.sha1, file.size);
        known.insert(file.dest.clone(), file.sha1.clone());
    }

    // Libraries with a checksum list: keep a matching jar, download the others and check them.
    let mut pending = Vec::new();
    for library in &checked {
        let dest = dirs.library_file(&library.path)?;
        match matching_sha1(&dest, &library.checksums).await {
            Some((sha1, size)) => set_library_file(&mut profile, library.index, &sha1, size),
            None => {
                if dest.exists() {
                    tokio::fs::remove_file(&dest)
                        .await
                        .map_err(|e| Error::io(&dest, e))?;
                }
                pending.push((library, dest));
            }
        }
    }
    let items = pending
        .iter()
        .map(|(library, dest)| DownloadItem::with_urls(library.urls.clone(), dest))
        .collect();
    download(ctx, items, InstallStage::Libraries, options).await?;
    for (library, dest) in &pending {
        match matching_sha1(dest, &library.checksums).await {
            Some((sha1, size)) => set_library_file(&mut profile, library.index, &sha1, size),
            None => {
                let _ = tokio::fs::remove_file(dest).await;
                return Err(Error::Invalid(format!(
                    "{} does not match any checksum of the Forge profile",
                    library.path
                )));
            }
        }
    }
    Ok(Outcome {
        profile,
        files: vec![universal_dest],
        known,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::forge::installer::LegacyInstall;
    use crate::version::VersionJson;

    fn legacy_1710() -> LegacyProfile {
        LegacyProfile {
            install: LegacyInstall {
                path: "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10".into(),
                file_path: "forge-1.7.10-10.13.4.1614-1.7.10-universal.jar".into(),
                minecraft: Some("1.7.10".into()),
            },
            version_info: serde_json::from_str(
                r#"{"id": "1.7.10-Forge10.13.4.1614-1.7.10", "time": "2015-12-10T00:05:37-0500",
                "releaseTime": "1960-01-01T00:00:00-0700", "type": "release",
                "minecraftArguments": "--username ${auth_player_name} --tweakClass cpw.mods.fml.common.launcher.FMLTweaker",
                "mainClass": "net.minecraft.launchwrapper.Launch", "minimumLauncherVersion": 14,
                "assets": "1.7.10", "inheritsFrom": "1.7.10", "jar": "1.7.10",
                "libraries": [
                    {"name": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10", "url": "http://files.minecraftforge.net/maven/"},
                    {"name": "net.minecraft:launchwrapper:1.12", "serverreq": true},
                    {"name": "com.typesafe.akka:akka-actor_2.11:2.3.3", "url": "http://files.minecraftforge.net/maven/",
                     "checksums": ["ED62E9FC709CA0F2FF1A3220DAA8B70A2870078E", "25a86ccfdb6f6dfe08971f4825d0a01be83a6f2e"],
                     "serverreq": true, "clientreq": true},
                    {"name": "org.jline:jline:3.5.1", "url": "https://maven.minecraftforge.net/", "checksums": ["aa"], "serverreq": true, "clientreq": false},
                    {"name": "lzma:lzma:0.0.1", "serverreq": true},
                    "garbage"
                ]}"#,
            )
            .unwrap(),
        }
    }

    #[test]
    fn converts_legacy_profiles() {
        let conversion = convert(&legacy_1710(), "1.7.10-forge-10.13.4.1614", "1.7.10").unwrap();
        let profile = &conversion.profile;
        assert_eq!(profile["id"], "1.7.10-forge-10.13.4.1614");
        assert_eq!(profile["inheritsFrom"], "1.7.10");
        assert_eq!(profile["jar"], "1.7.10");
        let libraries = profile["libraries"].as_array().unwrap();
        let names: Vec<&str> = libraries
            .iter()
            .map(|l| l["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10",
                "net.minecraft:launchwrapper:1.12",
                "com.typesafe.akka:akka-actor_2.11:2.3.3",
                "lzma:lzma:0.0.1"
            ],
            "server-only and malformed entries are dropped"
        );
        assert_eq!(
            libraries[0]["downloads"]["artifact"]["path"],
            "net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10.jar"
        );
        assert_eq!(libraries[0]["downloads"]["artifact"]["url"], "");
        assert!(libraries[0].get("url").is_none());
        assert_eq!(libraries[2]["url"], "https://maven.minecraftforge.net/");
        assert!(libraries[2].get("checksums").is_none());
        assert!(libraries[1].get("serverreq").is_none());

        assert_eq!(
            conversion.universal,
            Universal {
                entry: "forge-1.7.10-10.13.4.1614-1.7.10-universal.jar".into(),
                path: "net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10.jar".into(),
                index: 0,
            }
        );
        assert_eq!(conversion.checked.len(), 1);
        let akka = &conversion.checked[0];
        assert_eq!(akka.index, 2);
        assert_eq!(
            akka.urls[0],
            "https://maven.minecraftforge.net/com/typesafe/akka/akka-actor_2.11/2.3.3/akka-actor_2.11-2.3.3.jar"
        );
        assert_eq!(
            akka.checksums[0],
            "ed62e9fc709ca0f2ff1a3220daa8b70a2870078e"
        );

        // Verified checksums end up where the core installer reads them.
        let mut profile = conversion.profile.clone();
        set_library_file(&mut profile, 0, "u1", 9);
        set_library_file(&mut profile, 2, "a1", 7);
        let parsed: VersionJson = serde_json::from_value(profile).unwrap();
        let universal = parsed.libraries[0].artifact().unwrap();
        assert!(universal.urls.is_empty(), "local file");
        assert_eq!(universal.checksum, Some(pl_net::Checksum::sha1("u1")));
        assert_eq!(universal.size, Some(9));
        let akka = parsed.libraries[2].artifact().unwrap();
        assert_eq!(akka.checksum, Some(pl_net::Checksum::sha1("a1")));
        assert_eq!(akka.size, Some(7));
        assert_eq!(
            parsed.libraries[1].artifact().unwrap().urls[0],
            "https://libraries.minecraft.net/net/minecraft/launchwrapper/1.12/launchwrapper-1.12.jar"
        );
    }

    #[test]
    fn old_profiles_without_parent_get_one() {
        let mut legacy = legacy_1710();
        legacy.install.minecraft = Some("1.6.4".into());
        legacy.install.path = "net.minecraftforge:minecraftforge:9.11.1.1345".into();
        let info = legacy.version_info.as_object_mut().unwrap();
        info.remove("inheritsFrom");
        info.remove("jar");
        info.insert(
            "libraries".into(),
            json!([{"name": "net.sf.jopt-simple:jopt-simple:4.5", "checksums": ["bb"]}]),
        );
        let conversion = convert(&legacy, "1.6.4-forge-9.11.1.1345", "1.6.4").unwrap();
        assert_eq!(conversion.profile["inheritsFrom"], "1.6.4");
        assert_eq!(conversion.profile["jar"], "1.6.4");
        assert_eq!(
            conversion.universal.index, 0,
            "missing universal entry is added"
        );
        assert_eq!(conversion.checked[0].index, 1);
        assert_eq!(
            conversion.checked[0].urls[0],
            "https://libraries.minecraft.net/net/sf/jopt-simple/jopt-simple/4.5/jopt-simple-4.5.jar"
        );
        assert!(matches!(
            convert(&legacy, "x", "1.7.10"),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn forge_urls() {
        assert_eq!(
            normalize_forge_url("http://files.minecraftforge.net/maven/"),
            "https://maven.minecraftforge.net/"
        );
        assert_eq!(
            normalize_forge_url(" https://files.minecraftforge.net/maven "),
            "https://maven.minecraftforge.net"
        );
        assert_eq!(
            normalize_forge_url("http://repo.maven.apache.org/maven2/"),
            "https://repo.maven.apache.org/maven2/"
        );
        assert_eq!(
            normalize_forge_url("https://maven.minecraftforge.net/"),
            "https://maven.minecraftforge.net/"
        );
    }
}
