//! Mojang Java runtimes: `all.json` → platform → component → file manifest, installed into
//! `runtimes/<component>/<platform>/` (raw downloads, directories, executable bits, symlinks on
//! Unix). A marker file records which manifest is installed so repeat launches skip the work.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pl_net::{DownloadItem, DownloadOptions, VerifyMode};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::install::McContext;
use crate::java::detect::parse_major;
use crate::java::{EnsureJavaOptions, JavaInfo, JavaInstallation, JavaSource};
use crate::paths::{safe_join, safe_name, write_atomic};
use crate::platform::{Os, Platform, java_runtime_platform_arch};
use crate::version::component_major;

pub const JAVA_RUNTIME_MANIFEST_URL: &str = "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

/// Cached copy of `all.json` inside `meta/`.
const INDEX_CACHE_FILE: &str = "java_runtime_all.json";
/// Inside a runtime folder: which manifest is installed.
const MARKER_FILE: &str = ".projects-runtime.json";
/// Inside a runtime folder: the file manifest itself.
const MANIFEST_FILE: &str = ".projects-manifest.json";

/// `all.json`: platform key → component → available builds (first = current).
pub type RuntimeIndex = BTreeMap<String, BTreeMap<String, Vec<RuntimeEntry>>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEntry {
    pub manifest: RemoteFile,
    pub version: RuntimeVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFile {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeVersion {
    pub name: String,
    #[serde(default)]
    pub released: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeManifest {
    #[serde(default)]
    files: BTreeMap<String, RuntimeFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeFile {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    executable: bool,
    #[serde(default)]
    downloads: Option<RuntimeDownloads>,
    #[serde(default)]
    target: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeDownloads {
    raw: RemoteFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeMarker {
    component: String,
    platform: String,
    version: String,
    manifest_sha1: String,
}

fn parse_index(bytes: &[u8]) -> Result<RuntimeIndex> {
    let raw: BTreeMap<String, BTreeMap<String, Vec<serde_json::Value>>> =
        serde_json::from_slice(bytes).map_err(|e| Error::json("Java runtime index", e))?;
    Ok(raw
        .into_iter()
        .map(|(platform, components)| {
            let components = components
                .into_iter()
                .map(|(component, entries)| {
                    let entries = entries
                        .into_iter()
                        .filter_map(|e| serde_json::from_value::<RuntimeEntry>(e).ok())
                        .collect();
                    (component, entries)
                })
                .collect();
            (platform, components)
        })
        .collect())
}

/// Mojang's runtime index. Online it is refreshed (falling back to the cached copy when the
/// request fails); offline only the cache is used.
pub async fn runtime_index(ctx: &McContext, offline: bool) -> Result<RuntimeIndex> {
    let cache = ctx.dirs.meta().join(INDEX_CACHE_FILE);
    let mut fetch_error = None;
    if !offline {
        match ctx.http.get_bytes(JAVA_RUNTIME_MANIFEST_URL).await {
            Ok(bytes) => {
                let index = parse_index(&bytes)?;
                if let Err(error) = write_atomic(&cache, &bytes).await {
                    tracing::warn!(%error, "could not cache the Java runtime index");
                }
                return Ok(index);
            }
            Err(error) => {
                tracing::warn!(%error, "Java runtime index fetch failed, trying the cache");
                fetch_error = Some(error);
            }
        }
    }
    match tokio::fs::read(&cache).await {
        Ok(bytes) => parse_index(&bytes),
        Err(_) => Err(fetch_error.map(Error::from).unwrap_or(Error::Offline)),
    }
}

/// First platform key (native, then emulated fallbacks) that publishes `component`.
pub fn select_runtime<'a>(
    index: &'a RuntimeIndex,
    component: &str,
    platform: &Platform,
) -> Option<(&'static str, &'a RuntimeEntry)> {
    platform
        .java_runtime_platforms()
        .into_iter()
        .find_map(|key| {
            index
                .get(key)
                .and_then(|components| components.get(component))
                .and_then(|entries| entries.first())
                .map(|entry| (key, entry))
        })
}

/// Java executable inside a runtime folder: `bin/javaw.exe` on Windows (no console window),
/// `jre.bundle/Contents/Home/bin/java` on macOS, `bin/java` elsewhere.
pub fn runtime_java_executable(root: &Path, os: Os) -> PathBuf {
    let bin = root.join("bin");
    match os {
        Os::Windows => {
            let javaw = bin.join("javaw.exe");
            let java = bin.join("java.exe");
            if !javaw.is_file() && java.is_file() {
                java
            } else {
                javaw
            }
        }
        Os::MacOs => {
            let bundle = root
                .join("jre.bundle")
                .join("Contents")
                .join("Home")
                .join("bin")
                .join("java");
            let plain = bin.join("java");
            if !bundle.is_file() && plain.is_file() {
                plain
            } else {
                bundle
            }
        }
        Os::Linux | Os::Other => bin.join("java"),
    }
}

fn installation(
    root: &Path,
    os: Os,
    component: &str,
    platform_key: &str,
    version: &str,
) -> JavaInstallation {
    JavaInstallation {
        path: runtime_java_executable(root, os),
        info: JavaInfo {
            version: version.to_owned(),
            major: parse_major(version)
                .or_else(|| component_major(component))
                .unwrap_or(8),
            arch: java_runtime_platform_arch(platform_key),
            vendor: Some("Mojang".to_owned()),
        },
        source: JavaSource::Mojang {
            component: component.to_owned(),
            platform: platform_key.to_owned(),
        },
    }
}

async fn read_marker(root: &Path) -> Option<RuntimeMarker> {
    let bytes = tokio::fs::read(root.join(MARKER_FILE)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// An already installed runtime for `component` (native platform first, then emulated ones).
pub async fn installed_runtime(ctx: &McContext, component: &str) -> Option<JavaInstallation> {
    for key in ctx.platform.java_runtime_platforms() {
        let Ok(root) = ctx.dirs.runtime_dir(component, key) else {
            continue;
        };
        let Some(marker) = read_marker(&root).await else {
            continue;
        };
        let installed = installation(&root, ctx.platform.os, component, key, &marker.version);
        if installed.path.is_file() {
            return Some(installed);
        }
    }
    None
}

/// Download (or update/repair) the Mojang runtime `component` for the context platform.
pub async fn install_runtime(
    ctx: &McContext,
    component: &str,
    options: &EnsureJavaOptions,
) -> Result<JavaInstallation> {
    safe_name(component)?;
    let index = runtime_index(ctx, false).await?;
    let Some((platform_key, entry)) = select_runtime(&index, component, &ctx.platform) else {
        return Err(Error::Unsupported(format!(
            "Mojang publishes no {component} runtime for {}",
            ctx.platform
        )));
    };
    if Some(platform_key) != ctx.platform.java_runtime_platform() {
        tracing::info!(%component, %platform_key, "using an emulated Java runtime");
    }
    let root = ctx.dirs.runtime_dir(component, platform_key)?;
    let current = read_marker(&root)
        .await
        .is_some_and(|m| m.manifest_sha1 == entry.manifest.sha1);
    let java = runtime_java_executable(&root, ctx.platform.os);
    if current && options.verify != VerifyMode::Full && java.is_file() {
        return Ok(installation(
            &root,
            ctx.platform.os,
            component,
            platform_key,
            &entry.version.name,
        ));
    }

    let base = DownloadOptions {
        cancel: options.cancel.clone(),
        ..DownloadOptions::default()
    };
    let manifest_path = root.join(MANIFEST_FILE);
    ctx.downloader
        .download(
            DownloadItem::new(&entry.manifest.url, &manifest_path)
                .sha1(&entry.manifest.sha1)
                .size(Some(entry.manifest.size)),
            &base,
        )
        .await?;
    let bytes = tokio::fs::read(&manifest_path)
        .await
        .map_err(|e| Error::io(&manifest_path, e))?;
    let manifest: RuntimeManifest =
        serde_json::from_slice(&bytes).map_err(|e| Error::json("Java runtime manifest", e))?;

    let mut items = Vec::new();
    let mut executables = Vec::new();
    let mut links = Vec::new();
    for (name, file) in &manifest.files {
        let path = safe_join(&root, name)?;
        match file.kind.as_str() {
            "directory" => {
                tokio::fs::create_dir_all(&path)
                    .await
                    .map_err(|e| Error::io(&path, e))?;
            }
            "file" => {
                let Some(downloads) = &file.downloads else {
                    continue;
                };
                if file.executable {
                    executables.push(path.clone());
                }
                items.push(
                    DownloadItem::new(&downloads.raw.url, path)
                        .sha1(&downloads.raw.sha1)
                        .size(Some(downloads.raw.size))
                        .executable(file.executable),
                );
            }
            "link" => {
                if let Some(target) = &file.target {
                    links.push((path, target.clone()));
                }
            }
            other => tracing::debug!(%name, kind = other, "ignoring runtime entry"),
        }
    }

    let options_for_files = DownloadOptions {
        verify: if current {
            options.verify
        } else {
            VerifyMode::Full
        },
        progress: options.progress.clone(),
        ..base
    };
    ctx.downloader
        .download_all(items, &options_for_files)
        .await
        .map_err(|e| {
            if e.is_cancelled() {
                Error::Cancelled
            } else {
                e.into()
            }
        })?;

    tokio::task::spawn_blocking(move || finish_files(&executables, &links))
        .await
        .map_err(|e| Error::Java(format!("runtime finalization task failed: {e}")))??;

    let marker = RuntimeMarker {
        component: component.to_owned(),
        platform: platform_key.to_owned(),
        version: entry.version.name.clone(),
        manifest_sha1: entry.manifest.sha1.clone(),
    };
    let marker_bytes =
        serde_json::to_vec_pretty(&marker).map_err(|e| Error::json("runtime marker", e))?;
    write_atomic(&root.join(MARKER_FILE), &marker_bytes).await?;

    let installed = installation(
        &root,
        ctx.platform.os,
        component,
        platform_key,
        &entry.version.name,
    );
    if !installed.path.is_file() {
        return Err(Error::Java(format!(
            "runtime {component} has no Java executable at {}",
            installed.path.display()
        )));
    }
    Ok(installed)
}

/// Executable bits for files that already existed, and symlinks (Unix only).
fn finish_files(executables: &[PathBuf], links: &[(PathBuf, String)]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in executables {
            if let Ok(meta) = std::fs::metadata(path)
                && meta.permissions().mode() & 0o111 == 0
            {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| Error::io(path, e))?;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = executables;

    for (path, target) in links {
        make_link(path, target).map_err(|e| Error::io(path, e))?;
    }
    Ok(())
}

#[cfg(unix)]
fn make_link(path: &Path, target: &str) -> std::io::Result<()> {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() && std::fs::read_link(path)? == Path::new(target) {
            return Ok(());
        }
        if meta.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::os::unix::fs::symlink(target, path)
}

/// Windows runtimes contain no links; should one appear, copy its target instead.
#[cfg(not(unix))]
fn make_link(path: &Path, target: &str) -> std::io::Result<()> {
    let source = match path.parent() {
        Some(parent) => parent.join(target),
        None => PathBuf::from(target),
    };
    if source.is_file() && !path.exists() {
        std::fs::copy(&source, path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Arch;

    const INDEX: &str = r#"{
        "gamecore": {"jre-legacy": []},
        "mac-os": {"jre-legacy": [{"availability": {"group": 1, "progress": 100},
            "manifest": {"sha1": "aa", "size": 1, "url": "https://x/m1.json"},
            "version": {"name": "8u74", "released": "2016-01-01T00:00:00+00:00"}}],
            "java-runtime-delta": [{"manifest": {"sha1": "bb", "size": 2, "url": "https://x/m2.json"},
            "version": {"name": "21.0.7", "released": "2025-01-01T00:00:00+00:00"}}]},
        "mac-os-arm64": {"jre-legacy": [], "java-runtime-delta": [{"manifest": {"sha1": "cc", "size": 3, "url": "https://x/m3.json"},
            "version": {"name": "21.0.7", "released": "2025-01-01T00:00:00+00:00"}}]},
        "linux": {"java-runtime-delta": [{"broken": true}]}
    }"#;

    #[test]
    fn selects_native_then_rosetta() {
        let index = parse_index(INDEX.as_bytes()).unwrap();
        let mac_arm = Platform::new(Os::MacOs, Arch::Aarch64, "14.5");
        let (key, entry) = select_runtime(&index, "java-runtime-delta", &mac_arm).unwrap();
        assert_eq!((key, entry.manifest.sha1.as_str()), ("mac-os-arm64", "cc"));
        // No arm64 Java 8: fall back to the x64 build (Rosetta 2).
        let (key, entry) = select_runtime(&index, "jre-legacy", &mac_arm).unwrap();
        assert_eq!((key, entry.version.name.as_str()), ("mac-os", "8u74"));
        let linux = Platform::new(Os::Linux, Arch::X86_64, "6.1");
        assert!(select_runtime(&index, "java-runtime-delta", &linux).is_none());
        let linux_arm = Platform::new(Os::Linux, Arch::Aarch64, "6.1");
        assert!(select_runtime(&index, "jre-legacy", &linux_arm).is_none());
    }

    #[test]
    fn executable_paths() {
        let root = Path::new("/rt");
        assert_eq!(
            runtime_java_executable(root, Os::Windows),
            Path::new("/rt/bin/javaw.exe")
        );
        assert_eq!(
            runtime_java_executable(root, Os::MacOs),
            Path::new("/rt/jre.bundle/Contents/Home/bin/java")
        );
        assert_eq!(
            runtime_java_executable(root, Os::Linux),
            Path::new("/rt/bin/java")
        );
        let inst = installation(root, Os::Linux, "jre-legacy", "linux", "8u202");
        assert_eq!(inst.info.major, 8);
        assert_eq!(inst.info.arch, Arch::X86_64);
    }

    #[tokio::test]
    async fn installed_runtime_reads_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = McContext::new(
            pl_net::Http::new(pl_net::HttpConfig::new("pl-mc-test")).unwrap(),
            crate::paths::DataDirs::new(tmp.path()),
        )
        .with_platform(Platform::new(Os::Linux, Arch::X86_64, "6.1"));
        assert!(
            installed_runtime(&ctx, "java-runtime-delta")
                .await
                .is_none()
        );
        let root = ctx.dirs.runtime_dir("java-runtime-delta", "linux").unwrap();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin").join("java"), b"").unwrap();
        let marker = RuntimeMarker {
            component: "java-runtime-delta".into(),
            platform: "linux".into(),
            version: "21.0.7".into(),
            manifest_sha1: "bb".into(),
        };
        std::fs::write(root.join(MARKER_FILE), serde_json::to_vec(&marker).unwrap()).unwrap();
        let found = installed_runtime(&ctx, "java-runtime-delta").await.unwrap();
        assert_eq!(found.info.major, 21);
        assert_eq!(
            found.source,
            JavaSource::Mojang {
                component: "java-runtime-delta".into(),
                platform: "linux".into()
            }
        );
    }
}
