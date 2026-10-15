//! Forge/NeoForge installer jars: `install_profile.json` (modern `spec` 0/1 profiles with
//! processors, or legacy `install` + `versionInfo` profiles), the embedded `version.json`, the
//! `maven/` libraries shipped inside the jar and `data/` files used by processors.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer};
use serde_json::Value;
use zip::ZipArchive;
use zip::result::ZipError;

use crate::error::{Error, Result};
use crate::paths::write_atomic_blocking;
use crate::version::Library;

/// Largest metadata entry read into memory (profiles are a few kB; data files a few MB).
const MAX_ENTRY_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct InstallerContents {
    pub profile: InstallProfile,
    /// `maven/<path>` entries, without the `maven/` prefix.
    pub maven: BTreeSet<String>,
}

#[derive(Debug)]
pub(crate) enum InstallProfile {
    /// 1.12.2 (14.23.5.2851+) and newer: `version.json` plus processors.
    Modern(Box<ModernProfile>),
    /// 1.5.2 – 1.12.2 (up to 14.23.5.2847): `install` + `versionInfo`.
    Legacy(Box<LegacyProfile>),
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ModernProfile {
    #[serde(default)]
    pub spec: u32,
    #[serde(default)]
    pub minecraft: Option<String>,
    /// Entry name of the launcher profile (`/version.json`).
    #[serde(default)]
    pub json: Option<String>,
    #[serde(default)]
    pub data: BTreeMap<String, SidedValue>,
    #[serde(default)]
    pub processors: Vec<Processor>,
    /// Libraries the processors need (and, for 1.17+, runtime jars FML finds by coordinate).
    #[serde(default, deserialize_with = "lenient_libraries")]
    pub libraries: Vec<Library>,
    /// The launcher profile named by `json`.
    #[serde(skip)]
    pub version_json: Value,
}

/// A `data` entry. It has separate values for client and server installs; only the client one is
/// used.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct SidedValue {
    #[serde(default)]
    pub client: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Processor {
    /// Maven coordinate of the processor jar.
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Output file → expected SHA-1 (both may contain `{TOKENS}` / `[coordinates]`).
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    /// `client` and/or `server`; absent means both.
    #[serde(default)]
    pub sides: Option<Vec<String>>,
}

impl Processor {
    pub fn runs_on_client(&self) -> bool {
        self.sides
            .as_ref()
            .is_none_or(|sides| sides.iter().any(|s| s == "client"))
    }

    /// Short name for logs and errors: `installertools (DOWNLOAD_MOJMAPS)`, `binarypatcher`.
    pub fn display_name(&self) -> String {
        let artifact = self.jar.split(':').nth(1).unwrap_or(&self.jar);
        let task = self
            .args
            .iter()
            .position(|a| a == "--task")
            .and_then(|i| self.args.get(i + 1));
        match task {
            Some(task) => format!("{artifact} ({task})"),
            None => artifact.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyInstall {
    /// Maven coordinate the universal jar is installed as.
    pub path: String,
    /// Entry name of the universal jar inside the installer.
    pub file_path: String,
    #[serde(default)]
    pub minecraft: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LegacyProfile {
    pub install: LegacyInstall,
    /// The launcher profile (old format: `checksums`, `clientreq`/`serverreq`, Forge URLs).
    pub version_info: Value,
}

fn lenient_libraries<'de, D>(deserializer: D) -> std::result::Result<Vec<Library>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw: Option<Vec<Value>> = Option::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| match serde_json::from_value::<Library>(value) {
            Ok(library) => Some(library),
            Err(error) => {
                tracing::warn!(%error, "skipping malformed installer library");
                None
            }
        })
        .collect())
}

/// Parse `install_profile.json`; `read_entry` reads other installer entries (the modern
/// `version.json`).
pub(crate) fn parse_install_profile(
    bytes: &[u8],
    mut read_entry: impl FnMut(&str) -> Result<Option<Vec<u8>>>,
) -> Result<InstallProfile> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| Error::json("install_profile.json", e))?;
    if let (Some(install), Some(version_info)) = (value.get("install"), value.get("versionInfo")) {
        let install: LegacyInstall = serde_json::from_value(install.clone())
            .map_err(|e| Error::json("install_profile.json `install`", e))?;
        if !version_info.is_object() {
            return Err(Error::Invalid("install_profile.json `versionInfo`".into()));
        }
        return Ok(InstallProfile::Legacy(Box::new(LegacyProfile {
            install,
            version_info: version_info.clone(),
        })));
    }
    let mut profile: ModernProfile =
        serde_json::from_value(value).map_err(|e| Error::json("install_profile.json", e))?;
    let entry = profile
        .json
        .as_deref()
        .unwrap_or("/version.json")
        .trim_start_matches('/')
        .to_owned();
    let version_bytes =
        read_entry(&entry)?.ok_or_else(|| Error::Invalid(format!("installer has no `{entry}`")))?;
    profile.version_json =
        serde_json::from_slice(&version_bytes).map_err(|e| Error::json(&entry, e))?;
    Ok(InstallProfile::Modern(Box::new(profile)))
}

fn open(path: &Path) -> Result<ZipArchive<BufReader<std::fs::File>>> {
    let file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    ZipArchive::new(BufReader::new(file)).map_err(|e| Error::archive(path, e))
}

fn read_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    archive_path: &Path,
    name: &str,
) -> Result<Option<Vec<u8>>> {
    match archive.by_name(name) {
        Ok(entry) => {
            let mut bytes = Vec::with_capacity(entry.size().min(16 * 1024 * 1024) as usize);
            entry
                .take(MAX_ENTRY_BYTES)
                .read_to_end(&mut bytes)
                .map_err(|e| Error::archive(archive_path, format!("{name}: {e}")))?;
            Ok(Some(bytes))
        }
        Err(ZipError::FileNotFound) => Ok(None),
        Err(error) => Err(Error::archive(archive_path, format!("{name}: {error}"))),
    }
}

fn read_installer_blocking(path: &Path) -> Result<InstallerContents> {
    let mut archive = open(path)?;
    let profile_bytes =
        read_entry(&mut archive, path, "install_profile.json")?.ok_or_else(|| {
            Error::Unsupported(format!(
                "{} has no install_profile.json (installer too old)",
                path.display()
            ))
        })?;
    let profile =
        parse_install_profile(&profile_bytes, |name| read_entry(&mut archive, path, name))?;
    let maven = archive
        .file_names()
        .filter_map(|name| name.ok())
        .filter_map(|name| {
            name.strip_prefix("maven/")
                .filter(|rest| !rest.is_empty() && !rest.ends_with('/'))
                .map(str::to_owned)
        })
        .collect();
    Ok(InstallerContents { profile, maven })
}

/// Read and parse an installer jar (on the blocking pool).
pub(crate) async fn read_installer(path: PathBuf) -> Result<InstallerContents> {
    let display = path.clone();
    tokio::task::spawn_blocking(move || read_installer_blocking(&path))
        .await
        .map_err(|e| Error::archive(display, format!("installer task failed: {e}")))?
}

/// One installer entry to write to disk.
#[derive(Debug, Clone)]
pub(crate) struct Extraction {
    /// Entry name inside the installer (`maven/net/...jar`, `data/client.lzma`).
    pub entry: String,
    pub dest: PathBuf,
    /// SHA-1 the entry must have (lowercase hex).
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Extracted {
    pub dest: PathBuf,
    pub sha1: String,
    pub size: u64,
}

fn extract_blocking(installer: &Path, items: &[Extraction]) -> Result<Vec<Extracted>> {
    let mut archive = open(installer)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let bytes = read_entry(&mut archive, installer, &item.entry)?.ok_or_else(|| {
            Error::Invalid(format!(
                "the installer {} has no `{}`",
                installer.display(),
                item.entry
            ))
        })?;
        let sha1 = pl_net::hash_bytes(&bytes, pl_net::HashAlgo::Sha1);
        if let Some(expected) = &item.sha1
            && !expected.eq_ignore_ascii_case(&sha1)
        {
            return Err(Error::Invalid(format!(
                "`{}` in the installer has checksum {sha1}, expected {expected}",
                item.entry
            )));
        }
        let unchanged = std::fs::metadata(&item.dest)
            .is_ok_and(|m| m.is_file() && m.len() == bytes.len() as u64)
            && std::fs::read(&item.dest).is_ok_and(|existing| existing == bytes);
        if !unchanged {
            write_atomic_blocking(&item.dest, &bytes)?;
        }
        out.push(Extracted {
            dest: item.dest.clone(),
            sha1,
            size: bytes.len() as u64,
        });
    }
    Ok(out)
}

/// Write installer entries to disk (skipping files that are already identical).
pub(crate) async fn extract(installer: &Path, items: Vec<Extraction>) -> Result<Vec<Extracted>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let installer = installer.to_path_buf();
    let display = installer.clone();
    tokio::task::spawn_blocking(move || extract_blocking(&installer, &items))
        .await
        .map_err(|e| Error::archive(display, format!("extraction task failed: {e}")))?
}

/// `Main-Class` of a jar manifest (continuation lines start with one space).
pub(crate) fn manifest_main_class(manifest: &str) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in manifest.lines() {
        match (line.strip_prefix(' '), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(line.to_owned()),
        }
    }
    lines
        .iter()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("Main-Class")
                .then(|| value.trim().to_owned())
        })
        .filter(|class| !class.is_empty())
}

/// `Main-Class` of a processor jar.
pub(crate) async fn jar_main_class(jar: PathBuf) -> Result<String> {
    let display = jar.clone();
    tokio::task::spawn_blocking(move || -> Result<String> {
        let mut archive = open(&jar)?;
        let manifest = read_entry(&mut archive, &jar, "META-INF/MANIFEST.MF")?
            .ok_or_else(|| Error::Invalid(format!("{} has no manifest", jar.display())))?;
        manifest_main_class(&String::from_utf8_lossy(&manifest))
            .ok_or_else(|| Error::Invalid(format!("{} has no Main-Class", jar.display())))
    })
    .await
    .map_err(|e| Error::archive(display, format!("manifest task failed: {e}")))?
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    use super::*;

    /// Write a zip with the given entries.
    pub(crate) fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn manifest_main_class_with_continuations() {
        let manifest = "Manifest-Version: 1.0\r\nMain-Class: net.minecraftforge.installertools.Con\r\n soleTool\r\nName: net/minecraftforge/\r\nImplementation-Title: x\r\n";
        assert_eq!(
            manifest_main_class(manifest).as_deref(),
            Some("net.minecraftforge.installertools.ConsoleTool")
        );
        assert_eq!(manifest_main_class("Manifest-Version: 1.0\n"), None);
        assert_eq!(
            manifest_main_class("main-class:  a.B \n").as_deref(),
            Some("a.B")
        );
    }

    #[test]
    fn processor_sides_and_names() {
        let p: Processor = serde_json::from_str(
            r#"{"sides": ["server"], "jar": "net.minecraftforge:installertools:1.4.1", "args": ["--task", "EXTRACT_FILES"]}"#,
        )
        .unwrap();
        assert!(!p.runs_on_client());
        assert_eq!(p.display_name(), "installertools (EXTRACT_FILES)");
        let both: Processor =
            serde_json::from_str(r#"{"jar": "net.minecraftforge:binarypatcher:1.1.1"}"#).unwrap();
        assert!(both.runs_on_client());
        assert_eq!(both.display_name(), "binarypatcher");
        let client: Processor =
            serde_json::from_str(r#"{"jar": "a:b:1", "sides": ["client"]}"#).unwrap();
        assert!(client.runs_on_client());
    }

    #[tokio::test]
    async fn reads_modern_and_legacy_installers() {
        let tmp = tempfile::tempdir().unwrap();
        let modern = tmp.path().join("modern-installer.jar");
        write_zip(
            &modern,
            &[
                (
                    "install_profile.json",
                    br#"{"spec": 1, "profile": "forge", "minecraft": "1.20.1", "json": "/version.json",
                        "data": {"BINPATCH": {"client": "/data/client.lzma", "server": "/data/server.lzma"}},
                        "processors": [{"jar": "net.minecraftforge:binarypatcher:1.1.1", "args": ["--apply", "{BINPATCH}"]}],
                        "libraries": [{"name": "net.minecraftforge:binarypatcher:1.1.1", "downloads": {"artifact": {"path": "net/minecraftforge/binarypatcher/1.1.1/binarypatcher-1.1.1.jar", "url": "https://maven.minecraftforge.net/net/minecraftforge/binarypatcher/1.1.1/binarypatcher-1.1.1.jar", "sha1": "aa", "size": 1}}}, 7]}"#,
                ),
                (
                    "version.json",
                    br#"{"id": "1.20.1-forge-47.4.10", "inheritsFrom": "1.20.1", "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher"}"#,
                ),
                ("maven/net/minecraftforge/forge/1/forge-1.jar", b"jar"),
                ("data/client.lzma", b"patches"),
            ],
        );
        let contents = read_installer(modern.clone()).await.unwrap();
        let InstallProfile::Modern(profile) = contents.profile else {
            panic!("modern profile expected");
        };
        assert_eq!(profile.spec, 1);
        assert_eq!(profile.libraries.len(), 1, "malformed entries are skipped");
        assert_eq!(profile.processors.len(), 1);
        assert_eq!(
            profile.data["BINPATCH"].client.as_deref(),
            Some("/data/client.lzma")
        );
        assert_eq!(profile.version_json["id"], "1.20.1-forge-47.4.10");
        assert_eq!(
            contents.maven.into_iter().collect::<Vec<_>>(),
            ["net/minecraftforge/forge/1/forge-1.jar"]
        );

        // Extraction verifies checksums and skips identical files.
        let dest = tmp.path().join("out").join("client.lzma");
        let extracted = extract(
            &modern,
            vec![Extraction {
                entry: "data/client.lzma".into(),
                dest: dest.clone(),
                sha1: None,
            }],
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"patches");
        assert_eq!(extracted[0].size, 7);
        let again = extract(
            &modern,
            vec![Extraction {
                entry: "data/client.lzma".into(),
                dest: dest.clone(),
                sha1: Some(extracted[0].sha1.to_ascii_uppercase()),
            }],
        )
        .await
        .unwrap();
        assert_eq!(again, extracted);
        let wrong = extract(
            &modern,
            vec![Extraction {
                entry: "data/client.lzma".into(),
                dest: dest.clone(),
                sha1: Some("00".into()),
            }],
        )
        .await;
        assert!(matches!(wrong, Err(Error::Invalid(_))));
        let missing = extract(
            &modern,
            vec![Extraction {
                entry: "data/nope".into(),
                dest,
                sha1: None,
            }],
        )
        .await;
        assert!(matches!(missing, Err(Error::Invalid(_))));

        let legacy = tmp.path().join("legacy-installer.jar");
        write_zip(
            &legacy,
            &[
                (
                    "install_profile.json",
                    br#"{"install": {"profileName": "Forge", "target": "1.7.10-Forge10.13.4.1614-1.7.10",
                        "path": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10",
                        "filePath": "forge-1.7.10-10.13.4.1614-1.7.10-universal.jar", "minecraft": "1.7.10"},
                        "versionInfo": {"id": "1.7.10-Forge10.13.4.1614-1.7.10", "mainClass": "net.minecraft.launchwrapper.Launch", "libraries": []}}"#,
                ),
                ("forge-1.7.10-10.13.4.1614-1.7.10-universal.jar", b"universal"),
            ],
        );
        let contents = read_installer(legacy).await.unwrap();
        let InstallProfile::Legacy(profile) = contents.profile else {
            panic!("legacy profile expected");
        };
        assert_eq!(
            profile.install.file_path,
            "forge-1.7.10-10.13.4.1614-1.7.10-universal.jar"
        );
        assert_eq!(profile.install.minecraft.as_deref(), Some("1.7.10"));
        assert!(contents.maven.is_empty());

        let empty = tmp.path().join("old.jar");
        write_zip(&empty, &[("cpw/Installer.class", b"x")]);
        assert!(matches!(
            read_installer(empty).await,
            Err(Error::Unsupported(_))
        ));
        let broken = tmp.path().join("broken.jar");
        std::fs::write(&broken, b"not a zip").unwrap();
        assert!(matches!(
            read_installer(broken).await,
            Err(Error::Archive { .. })
        ));
    }

    #[tokio::test]
    async fn main_class_from_jar() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("tool.jar");
        write_zip(
            &jar,
            &[(
                "META-INF/MANIFEST.MF",
                b"Manifest-Version: 1.0\nMain-Class: net.minecraftforge.binarypatcher.ConsoleTool\n",
            )],
        );
        assert_eq!(
            jar_main_class(jar).await.unwrap(),
            "net.minecraftforge.binarypatcher.ConsoleTool"
        );
        let no_manifest = tmp.path().join("plain.jar");
        write_zip(&no_manifest, &[("a.class", b"x")]);
        assert!(jar_main_class(no_manifest).await.is_err());
    }
}
