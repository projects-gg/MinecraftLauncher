//! Installed Projects Console Client builds.
//!
//! Every version lives in its own folder, `<afk>/bin/<version>/`, so installing never touches a
//! build a running session uses: the previous version stays until the new one is complete, and a
//! version folder only appears (by rename) once it is fully extracted, verified and prepared.
//! `bin/current.json` remembers the selected version (it wins over a newer folder, so a rollback in
//! the content index is respected); without it the newest installed version is used, which is also
//! the offline fallback.

use std::cmp::Ordering;
use std::io::Read;
use std::path::{Path, PathBuf};

use pl_net::{Checksum, DownloadItem, DownloadOptions, Downloader};
use serde::{Deserialize, Serialize};

use crate::error::{AfkError, Result};
use crate::util::{remove_file_if_exists, write_atomic};

/// File name of the client executable (renamed from `MinecraftClient` by the publish script).
pub const CLIENT_EXE_NAME: &str = if cfg!(windows) {
    "Projects-Konsol-Hesap.exe"
} else {
    "Projects-Konsol-Hesap"
};

/// Written into a version folder once it is complete.
const INSTALL_MARKER: &str = ".pl-afk-install.json";
const CURRENT_FILE: &str = "current.json";
const STAGING_DIR: &str = ".staging";
/// Versions kept by [`AfkRuntime::prune`] besides the ones in use.
const KEEP_VERSIONS: usize = 2;

/// A client build as published in the signed content index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkClientRelease {
    pub version: String,
    pub build: AfkClientBuild,
}

/// The platform zip of a release (`v1/afk/<version>/<os-arch>.zip`); the executable sits at the
/// zip root, next to its native libraries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkClientBuild {
    /// Primary URL first, then mirrors.
    pub urls: Vec<String>,
    pub sha256: String,
    pub size: u64,
}

/// A runnable client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientExe {
    pub path: PathBuf,
    /// `"dev"` for the development override.
    pub version: String,
    pub dev: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Marker {
    version: String,
    sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Current {
    version: String,
}

#[derive(Debug, Clone)]
pub struct AfkRuntime {
    bin_dir: PathBuf,
    dev_dir: Option<PathBuf>,
}

impl AfkRuntime {
    /// `bin_dir` is `AfkPaths::bin_dir()`.
    pub fn new(bin_dir: impl Into<PathBuf>) -> Self {
        Self {
            bin_dir: bin_dir.into(),
            dev_dir: None,
        }
    }

    /// Development override: when `dir` contains the client executable it is always used and
    /// nothing is downloaded (legacy: an `afk` folder next to the launcher executable).
    pub fn with_dev_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.dev_dir = dir;
        self
    }

    /// The legacy override location: `<launcher executable folder>/afk`.
    pub fn default_dev_dir() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        Some(exe.parent()?.join("afk"))
    }

    pub fn bin_dir(&self) -> &Path {
        &self.bin_dir
    }

    pub fn dev_client(&self) -> Option<ClientExe> {
        let path = self.dev_dir.as_ref()?.join(CLIENT_EXE_NAME);
        path.is_file().then(|| ClientExe {
            path,
            version: "dev".to_owned(),
            dev: true,
        })
    }

    /// Complete installed versions, newest first.
    pub fn installed_versions(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.bin_dir) else {
            return Vec::new();
        };
        let mut versions: Vec<String> = entries
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| is_valid_version(name) && self.is_installed(name))
            .collect();
        versions.sort_by(|a, b| compare_versions(b, a));
        versions
    }

    pub fn is_installed(&self, version: &str) -> bool {
        let dir = self.bin_dir.join(version);
        dir.join(INSTALL_MARKER).is_file() && dir.join(CLIENT_EXE_NAME).is_file()
    }

    fn installed(&self, version: &str) -> Option<ClientExe> {
        (is_valid_version(version) && self.is_installed(version)).then(|| ClientExe {
            path: self.bin_dir.join(version).join(CLIENT_EXE_NAME),
            version: version.to_owned(),
            dev: false,
        })
    }

    /// The selected version from `current.json`, if it is still installed.
    pub fn current_version(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.bin_dir.join(CURRENT_FILE)).ok()?;
        let current: Current = serde_json::from_str(&text).ok()?;
        self.is_installed(&current.version)
            .then_some(current.version)
    }

    /// The client to run now: dev override, then the selected version, then the newest installed.
    pub fn resolve(&self) -> Option<ClientExe> {
        if let Some(dev) = self.dev_client() {
            return Some(dev);
        }
        self.current_version()
            .and_then(|v| self.installed(&v))
            .or_else(|| {
                self.installed_versions()
                    .first()
                    .and_then(|v| self.installed(v))
            })
    }

    fn select(&self, version: &str) -> Result<()> {
        let path = self.bin_dir.join(CURRENT_FILE);
        let text = serde_json::to_string(&Current {
            version: version.to_owned(),
        })
        .map_err(|e| AfkError::json("current.json", e))?;
        write_atomic(&path, text.as_bytes(), false).map_err(|e| AfkError::io(&path, e))
    }

    /// Copies an already extracted client folder (the v1 launcher's `.projects/afk/bin`) in as
    /// `version`. It is selected only when no version is selected yet, so a published release
    /// still wins. Blocking: call it from a blocking task.
    pub fn adopt_folder(&self, version: &str, source: &Path) -> Result<ClientExe> {
        if !is_valid_version(version) {
            return Err(AfkError::io(
                source,
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid version name"),
            ));
        }
        if let Some(existing) = self.installed(version) {
            return Ok(existing);
        }
        if !source.join(CLIENT_EXE_NAME).is_file() {
            return Err(AfkError::ClientNotInstalled);
        }
        let staging = self
            .bin_dir
            .join(STAGING_DIR)
            .join(format!("{version}-adopt"));
        let _ = std::fs::remove_dir_all(&staging);
        copy_dir(source, &staging).map_err(|e| AfkError::io(&staging, e))?;
        let marker = serde_json::to_vec(&Marker {
            version: version.to_owned(),
            sha256: "adopted".to_owned(),
        })
        .map_err(|e| AfkError::json(INSTALL_MARKER, e))?;
        std::fs::write(staging.join(INSTALL_MARKER), marker)
            .map_err(|e| AfkError::io(&staging, e))?;
        let target = self.bin_dir.join(version);
        std::fs::rename(&staging, &target).map_err(|e| AfkError::io(&target, e))?;
        if self.current_version().is_none() {
            self.select(version)?;
        }
        self.installed(version).ok_or(AfkError::ClientNotInstalled)
    }

    /// Makes `release` available (installing it when needed) and selects it. When that fails for
    /// any reason but cancellation and some version is installed, that version is used instead
    /// (offline start). Without a release the installed client is used.
    pub async fn ensure(
        &self,
        downloader: Option<&Downloader>,
        release: Option<&AfkClientRelease>,
        options: &DownloadOptions,
    ) -> Result<ClientExe> {
        if let Some(dev) = self.dev_client() {
            return Ok(dev);
        }
        if let (Some(downloader), Some(release)) = (downloader, release) {
            match self.install(downloader, release, options).await {
                Ok(client) => return Ok(client),
                Err(error) if error.is_cancelled() => return Err(error),
                Err(error) => match self.resolve() {
                    Some(client) => {
                        tracing::warn!(%error, version = %client.version, "AFK client update failed, using the installed build");
                        return Ok(client);
                    }
                    None => return Err(error),
                },
            }
        }
        self.resolve().ok_or(AfkError::ClientNotInstalled)
    }

    /// Downloads (sha256 + size verified by pl-net), extracts and prepares `release` in a staging
    /// folder, then moves it into place and selects it. Already installed versions are only
    /// selected.
    pub async fn install(
        &self,
        downloader: &Downloader,
        release: &AfkClientRelease,
        options: &DownloadOptions,
    ) -> Result<ClientExe> {
        let version = release.version.trim();
        if !is_valid_version(version) {
            return Err(AfkError::InvalidRelease(format!(
                "unsafe version name {version:?}"
            )));
        }
        if release.build.urls.is_empty() || release.build.sha256.trim().is_empty() {
            return Err(AfkError::InvalidRelease("missing URL or checksum".into()));
        }
        if let Some(client) = self.installed(version) {
            self.select(version)?;
            return Ok(client);
        }

        let staging_root = self.bin_dir.join(STAGING_DIR);
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let zip_path = staging_root.join(format!("{version}-{nonce}.zip"));
        let unpack_dir = staging_root.join(format!("{version}-{nonce}"));

        let item = DownloadItem::with_urls(release.build.urls.clone(), &zip_path)
            .checksum(Some(Checksum::sha256(&release.build.sha256)))
            .size(Some(release.build.size));
        // `Downloader::download` futures are not provably `Send` (a rustc higher-ranked lifetime
        // limitation inside `download_all`), which would make every manager future non-`Send`.
        // Driving it on the blocking pool keeps the verified, cancellable download while the
        // future returned here stays `Send`.
        let downloaded = {
            let downloader = downloader.clone();
            let options = options.clone();
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                handle.block_on(downloader.download(item, &options))
            })
            .await
            .map_err(|e| AfkError::InvalidData(format!("download task failed: {e}")))?
        };
        let result = match downloaded {
            Ok(_) => {
                let bin_dir = self.bin_dir.clone();
                let version = version.to_owned();
                let sha256 = Checksum::sha256(&release.build.sha256).hex;
                let zip = zip_path.clone();
                let unpack = unpack_dir.clone();
                tokio::task::spawn_blocking(move || {
                    finish_install(&bin_dir, &version, &sha256, &zip, &unpack)
                })
                .await
                .map_err(|e| AfkError::InvalidData(format!("install task failed: {e}")))?
            }
            Err(error) => Err(AfkError::from(error)),
        };

        let _ = remove_file_if_exists(&zip_path);
        if unpack_dir.exists() {
            let _ = std::fs::remove_dir_all(&unpack_dir);
        }
        let client = result?;
        self.select(&client.version)?;
        Ok(client)
    }

    /// Deletes installed versions except the selected one, the `in_use` ones and the newest
    /// [`KEEP_VERSIONS`], plus leftovers of interrupted installs. Never call it for a version a
    /// session still runs (the manager passes them in `in_use`). Returns the removed versions.
    pub fn prune(&self, in_use: &[String]) -> Vec<String> {
        let current = self.current_version();
        let mut removed = Vec::new();
        for (index, version) in self.installed_versions().into_iter().enumerate() {
            let keep = index < KEEP_VERSIONS
                || current.as_deref() == Some(version.as_str())
                || in_use.contains(&version);
            if keep {
                continue;
            }
            match std::fs::remove_dir_all(self.bin_dir.join(&version)) {
                Ok(()) => removed.push(version),
                Err(error) => {
                    tracing::warn!(%version, %error, "could not remove an old AFK client")
                }
            }
        }
        let _ = std::fs::remove_dir_all(self.bin_dir.join(STAGING_DIR));
        removed
    }
}

/// Blocking part of an install: extract, check, prepare, mark complete, move into place.
fn finish_install(
    bin_dir: &Path,
    version: &str,
    sha256: &str,
    zip_path: &Path,
    unpack_dir: &Path,
) -> Result<ClientExe> {
    extract_zip(zip_path, unpack_dir)?;
    let staged_exe = unpack_dir.join(CLIENT_EXE_NAME);
    if !staged_exe.is_file() {
        return Err(AfkError::InvalidRelease(format!(
            "the package has no {CLIENT_EXE_NAME} at its root"
        )));
    }
    make_executable(&staged_exe)?;
    prepare_macos(unpack_dir);

    let marker = serde_json::to_string(&Marker {
        version: version.to_owned(),
        sha256: sha256.to_owned(),
    })
    .map_err(|e| AfkError::json("install marker", e))?;
    let marker_path = unpack_dir.join(INSTALL_MARKER);
    write_atomic(&marker_path, marker.as_bytes(), false)
        .map_err(|e| AfkError::io(&marker_path, e))?;

    let final_dir = bin_dir.join(version);
    if final_dir.exists() {
        if final_dir.join(INSTALL_MARKER).is_file() && final_dir.join(CLIENT_EXE_NAME).is_file() {
            // Another install finished first; keep it.
            return Ok(ClientExe {
                path: final_dir.join(CLIENT_EXE_NAME),
                version: version.to_owned(),
                dev: false,
            });
        }
        // An incomplete folder cannot be in use: replace it.
        std::fs::remove_dir_all(&final_dir).map_err(|e| AfkError::io(&final_dir, e))?;
    }
    std::fs::rename(unpack_dir, &final_dir).map_err(|e| AfkError::io(&final_dir, e))?;
    Ok(ClientExe {
        path: final_dir.join(CLIENT_EXE_NAME),
        version: version.to_owned(),
        dev: false,
    })
}

/// Extracts every entry with a safe relative path (zip-slip protection via `enclosed_name`).
fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path).map_err(|e| AfkError::io(zip_path, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| AfkError::archive(zip_path, e))?;
    std::fs::create_dir_all(dest).map_err(|e| AfkError::io(dest, e))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| AfkError::archive(zip_path, e))?;
        let Some(relative) = entry.enclosed_name() else {
            tracing::warn!(index, "skipping an unsafe path in the AFK client package");
            continue;
        };
        let target = dest.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| AfkError::io(&target, e))?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AfkError::io(parent, e))?;
        }
        let mut out = std::fs::File::create(&target).map_err(|e| AfkError::io(&target, e))?;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = entry
                .read(&mut buffer)
                .map_err(|e| AfkError::archive(zip_path, e))?;
            if read == 0 {
                break;
            }
            std::io::Write::write_all(&mut out, &buffer[..read])
                .map_err(|e| AfkError::io(&target, e))?;
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let mode = mode & 0o777;
            if mode != 0 {
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode))
                    .map_err(|e| AfkError::io(&target, e))?;
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| AfkError::io(path, e))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// macOS: clear the quarantine flag and ad-hoc sign the executable and native libraries (Apple
/// Silicon refuses unsigned code). Missing tools or failures are logged, never fatal.
#[cfg(target_os = "macos")]
fn prepare_macos(dir: &Path) {
    use std::ffi::OsStr;
    run_tool(
        "xattr",
        &[
            OsStr::new("-dr"),
            OsStr::new("com.apple.quarantine"),
            dir.as_os_str(),
        ],
    );
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == CLIENT_EXE_NAME || name.ends_with(".dylib") {
            run_tool(
                "codesign",
                &[
                    OsStr::new("--force"),
                    OsStr::new("-s"),
                    OsStr::new("-"),
                    path.as_os_str(),
                ],
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn run_tool(tool: &str, args: &[&std::ffi::OsStr]) {
    match std::process::Command::new(tool)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::debug!(%tool, %status, "macOS preparation step failed"),
        Err(error) => tracing::debug!(%tool, %error, "macOS preparation tool unavailable"),
    }
}

#[cfg(not(target_os = "macos"))]
fn prepare_macos(_dir: &Path) {}

/// Version names become folder names: short, no separators, no dot-only names.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn is_valid_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 64
        && !version.starts_with('.')
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}

/// Natural order: digit runs compare numerically (`v10` > `v9`, `1.10.0` > `1.9.2`). A `v` right
/// before the first digit is ignored, so the v1 launcher's `v3` sorts below `4.0.0`, and a
/// pre-release suffix sorts below the plain release (`4.1.0-beta` < `4.1.0`).
fn compare_versions(a: &str, b: &str) -> Ordering {
    let mut a = chunks(without_v_prefix(a)).into_iter();
    let mut b = chunks(without_v_prefix(b)).into_iter();
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(rest)) if rest.starts_with('-') => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (Some(rest), None) if rest.starts_with('-') => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let order = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    _ => x.cmp(y),
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

fn without_v_prefix(version: &str) -> &str {
    match version
        .strip_prefix('v')
        .or_else(|| version.strip_prefix('V'))
    {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => rest,
        _ => version,
    }
}

fn chunks(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    for i in 1..=bytes.len() {
        if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[i - 1].is_ascii_digit() {
            out.push(&text[start..i]);
            start = i;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_version_order() {
        assert_eq!(compare_versions("v10", "v9"), Ordering::Greater);
        assert_eq!(compare_versions("1.10.0", "1.9.2"), Ordering::Greater);
        assert_eq!(compare_versions("v1", "v1"), Ordering::Equal);
        assert_eq!(compare_versions("v1", "v1.1"), Ordering::Less);
    }

    #[test]
    fn legacy_v1_client_sorts_below_numbered_releases() {
        assert_eq!(compare_versions("v3", "4.0.0"), Ordering::Less);
        assert_eq!(compare_versions("4.0.0", "v3"), Ordering::Greater);
        assert_eq!(compare_versions("v3", "3"), Ordering::Equal);
        assert_eq!(without_v_prefix("vanilla"), "vanilla");
    }

    #[test]
    fn pre_releases_sort_below_the_release() {
        assert_eq!(compare_versions("4.1.0-beta", "4.1.0"), Ordering::Less);
        assert_eq!(compare_versions("4.1.0", "4.1.0-beta"), Ordering::Greater);
        assert_eq!(compare_versions("4.1.0-beta", "4.0.0"), Ordering::Greater);
        assert_eq!(
            compare_versions("4.1.0-beta.2", "4.1.0-beta.10"),
            Ordering::Less
        );
    }

    #[test]
    fn version_names_must_be_safe_folder_names() {
        assert!(is_valid_version("v12"));
        assert!(is_valid_version("2.0.1-beta+3"));
        assert!(!is_valid_version("../x"));
        assert!(!is_valid_version(".."));
        assert!(!is_valid_version("a/b"));
        assert!(!is_valid_version(""));
    }
}
