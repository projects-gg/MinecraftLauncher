//! Room on the disk for an install. Before anything big is downloaded, a launch checks that the
//! disks it writes to can hold what it still has to download (`DISK-DOLU` in the launcher, which
//! offers "Yer aç"); a disk that fills up anyway is recognised by [`crate::Error::is_disk_full`].
//!
//! Only missing files count and the margin stays small: a cautious guess that stops a launch which
//! would have fit is worse than the rare disk that fills up during the download. A roomy disk costs
//! one look at the version's file list ([`GameFiles::upper_bound`] reads no file); the files are
//! only looked at when room is tight ([`GameFiles::missing`]).

use std::collections::HashSet;

use futures::StreamExt;
use pl_net::{CancellationToken, DownloadItem, DownloadOptions, VerifyMode, verify_file};

use super::libraries::plan_libraries;
use super::{McContext, assets, logging_download, net_error};
use crate::error::{Error, Result};
use crate::java::JavaSetting;
use crate::java::runtime::{download_manifest, installed_runtime, runtime_index, select_runtime};
use crate::loaders::{LoaderKind, loader_profile_id};
use crate::paths::{DataDirs, safe_join};
use crate::version::{ResolvedVersion, VersionManifest, resolve_version};

const MB: u64 = 1024 * 1024;

/// Working room on top of a download (a mod loader installer's output, natives, the game's first
/// start): as much as the download itself, at least this much...
pub const WORK_ROOM_MIN: u64 = 32 * MB;
/// ...and at most this much.
pub const WORK_ROOM_MAX: u64 = 200 * MB;

/// A Mojang Java runtime that is not installed yet, while its file list is out of reach (the
/// runtimes take 120–190 MB).
pub const JAVA_RUNTIME_GUESS: u64 = 200 * MB;

/// Below this much free space a failed install is put down to the full disk.
pub const NEARLY_FULL: u64 = 64 * MB;

/// The environment variable that stands in for the free space of every disk in debug builds
/// (bytes), so an end-to-end run reaches "Yer aç" without filling a disk.
pub const FREE_BYTES_ENV: &str = "PROJECTS_E2E_FREE_BYTES";

/// The files a first install of a mod loader adds besides the game's (their sizes are only known
/// once its installer ran): Forge's and NeoForge's installer, libraries and patched game files; a
/// few libraries for Fabric and Quilt.
pub fn loader_files_guess(kind: LoaderKind) -> u64 {
    match kind {
        LoaderKind::Vanilla => 0,
        LoaderKind::Fabric | LoaderKind::Quilt => 10 * MB,
        LoaderKind::Forge | LoaderKind::NeoForge => 150 * MB,
    }
}

/// Free space needed to download `missing` bytes: the files, a tenth more for the file system and
/// the guesses, and working room. Nothing to download needs nothing.
pub fn room_needed(missing: u64) -> u64 {
    if missing == 0 {
        return 0;
    }
    missing
        .saturating_add(missing / 10)
        .saturating_add(missing.clamp(WORK_ROOM_MIN, WORK_ROOM_MAX))
}

/// One place a launch writes to: the free space measured there (`None` when unknown) and the bytes
/// still to download into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub free: Option<u64>,
    pub missing: u64,
}

/// A disk too small for its part of a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortfall {
    /// [`room_needed`] for the download.
    pub needed: u64,
    pub free: u64,
    /// The bytes to download.
    pub missing: u64,
}

/// The first disk that cannot hold its part: the shared game files (versions, libraries, assets,
/// Java) and the instance's own folder (the pack's files). Both live in the data folder unless a
/// link sends the instance elsewhere, so the same free space read at both is taken for one disk
/// and their downloads add up. An unknown free space never stops anything.
pub fn shortfall(shared: Target, instance: Target) -> Option<Shortfall> {
    first_short(shared, instance, 0, true)
}

/// Free space that background preparation leaves alone: a download nobody asked for yet never
/// takes a disk below this much (the player's own OYNA still may, after [`shortfall`]).
pub const BACKGROUND_RESERVE: u64 = 2 * 1024 * MB;

/// [`shortfall`] for background preparation: [`BACKGROUND_RESERVE`] has to stay free on top of
/// the usual room, and a disk whose free space is unknown counts as too small.
pub fn background_shortfall(shared: Target, instance: Target) -> Option<Shortfall> {
    first_short(shared, instance, BACKGROUND_RESERVE, false)
}

/// A disk too full for background preparation to go on downloading.
pub fn background_tight(free: u64) -> bool {
    free < BACKGROUND_RESERVE
}

fn first_short(
    shared: Target,
    instance: Target,
    reserve: u64,
    unknown_fits: bool,
) -> Option<Shortfall> {
    let check = |free: Option<u64>, missing: u64| {
        if missing == 0 {
            return None;
        }
        let free = match free {
            Some(free) => free,
            None if unknown_fits => return None,
            None => 0,
        };
        let needed = room_needed(missing).saturating_add(reserve);
        (needed > free).then_some(Shortfall {
            needed,
            free,
            missing,
        })
    };
    if shared.free.is_some() && shared.free == instance.free {
        return check(shared.free, shared.missing.saturating_add(instance.missing));
    }
    check(shared.free, shared.missing).or_else(|| check(instance.free, instance.missing))
}

/// Whether a disk with `free` bytes left is (nearly) full.
pub fn nearly_full(free: u64) -> bool {
    free < NEARLY_FULL
}

/// "300 MB", "1,2 GB" / "1.2 GB": the launcher's byte format (`formatBytes`): the language's
/// decimal mark, at most one decimal, and never four digits before the unit (999,96 MB reads
/// "1 GB").
pub fn format_size(bytes: u64, lang: pl_lang::Lang) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let shown = |value: f64, unit: usize| {
        if unit == 0 {
            value.round()
        } else {
            (value * 10.0).round() / 10.0
        }
    };
    let mut value = bytes as f64;
    let mut unit = 0;
    while shown(value, unit) >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let figure = shown(value, unit);
    let text = if figure.fract() == 0.0 {
        format!("{figure:.0}")
    } else {
        lang.pick(
            format!("{figure:.1}").replace('.', ","),
            format!("{figure:.1}"),
        )
    };
    format!("{text} {}", UNITS[unit])
}

/// Free space to use instead of the real one: debug builds with [`FREE_BYTES_ENV`] set. Always
/// `None` in release builds.
pub fn free_bytes_override() -> Option<u64> {
    #[cfg(debug_assertions)]
    {
        parse_free_bytes(std::env::var(FREE_BYTES_ENV).ok().as_deref())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

#[cfg(any(debug_assertions, test))]
fn parse_free_bytes(value: Option<&str>) -> Option<u64> {
    value?.trim().parse().ok()
}

/// The game files one launch installs: the version the install works through and a guess for
/// what a mod loader installed for the first time adds.
#[derive(Debug, Clone)]
pub struct GameFiles {
    /// The mod loader's profile when it is installed (its libraries count exactly), else the
    /// game's own version.
    pub resolved: ResolvedVersion,
    /// [`loader_files_guess`] for a mod loader whose profile is not installed yet.
    pub loader_extra: u64,
    /// The instance's Java: an automatic one may still have to be downloaded.
    pub java: JavaSetting,
}

impl GameFiles {
    /// What launching Minecraft `mc_version` with `loader` (`loader_version`; `None` for vanilla)
    /// installs. May download the game's version JSON (small; the install reads it next).
    pub async fn for_launch(
        ctx: &McContext,
        mc_version: &str,
        loader: LoaderKind,
        loader_version: Option<&str>,
        java: &JavaSetting,
        manifest: Option<&VersionManifest>,
    ) -> Result<Self> {
        let profile = match (loader, loader_version) {
            (LoaderKind::Vanilla, _) | (_, None) => None,
            (kind, Some(version)) => Some(loader_profile_id(kind, mc_version, version)?),
        };
        let installed = match &profile {
            Some(id) => tokio::fs::try_exists(ctx.dirs.version_json(id)?)
                .await
                .unwrap_or(false),
            None => false,
        };
        let (id, loader_extra) = match profile {
            Some(id) if installed => (id, 0),
            _ => (mc_version.trim().to_owned(), loader_files_guess(loader)),
        };
        Ok(Self {
            resolved: resolve_version(ctx, &id, manifest).await?,
            loader_extra,
            java: java.clone(),
        })
    }

    /// Every byte the install could download, as if no file were on disk (only an installed Java
    /// runtime's marker is read). A disk with room for this needs no closer look. `None` when the
    /// version does not tell (no total size for its asset objects).
    pub async fn upper_bound(&self, ctx: &McContext) -> Option<u64> {
        let resolved = &self.resolved;
        let index = resolved.asset_index.as_ref()?;
        let assets = index.size.unwrap_or(0).saturating_add(index.total_size?);
        let client = resolved.client.as_ref().and_then(|c| c.size).unwrap_or(0);
        let logging = resolved
            .logging
            .as_ref()
            .and_then(|l| l.file.as_ref())
            .and_then(|f| f.size)
            .unwrap_or(0);
        let mut seen = HashSet::new();
        let libraries: u64 = plan_libraries(&ctx.dirs, resolved, &ctx.platform)
            .ok()?
            .downloads
            .into_iter()
            .filter(|item| !item.urls.is_empty() && seen.insert(item.dest.clone()))
            .filter_map(|item| item.size)
            .sum();
        let java = self.java_guess(ctx).await;
        Some(
            [assets, client, logging, libraries, java, self.loader_extra]
                .into_iter()
                .fold(0u64, u64::saturating_add),
        )
    }

    /// What the install still has to download: files that are missing or have another size (a
    /// repeat launch trusts sizes too). Small metadata is downloaded for this when it is not here
    /// yet, as the install needs it next anyway: the asset index and, for a Java runtime that is
    /// not installed, its file list.
    pub async fn missing(&self, ctx: &McContext, cancel: &CancellationToken) -> Result<u64> {
        let resolved = &self.resolved;
        let (index_path, index_item) = assets::index_download(&ctx.dirs, resolved)?;
        if let Some(item) = index_item {
            let options = DownloadOptions {
                verify: VerifyMode::Size,
                cancel: cancel.clone(),
                ..DownloadOptions::default()
            };
            ctx.downloader
                .download(item, &options)
                .await
                .map_err(net_error)?;
        }
        let index = assets::read_index(&index_path).await?;
        let mut items = assets::object_downloads(&ctx.dirs, &index)?;
        items.extend(
            plan_libraries(&ctx.dirs, resolved, &ctx.platform)?
                .downloads
                .into_iter()
                // Files a mod loader's installer makes have no download.
                .filter(|item| !item.urls.is_empty()),
        );
        items.extend(client_item(&ctx.dirs, resolved)?);
        if let Some((_, Some(item))) = logging_download(ctx, resolved)? {
            items.push(item);
        }
        let files = missing_bytes(items, cancel).await?;
        let java = self.java_missing(ctx, cancel).await?;
        Ok(files.saturating_add(java).saturating_add(self.loader_extra))
    }

    /// A Mojang runtime that is not installed yet, as a guess.
    async fn java_guess(&self, ctx: &McContext) -> u64 {
        match self.java {
            JavaSetting::Custom { .. } => 0,
            JavaSetting::Auto => {
                match installed_runtime(ctx, self.resolved.java_component()).await {
                    Some(_) => 0,
                    None => JAVA_RUNTIME_GUESS,
                }
            }
        }
    }

    /// The files of a Mojang runtime that is not installed yet, from its file list (kept for the
    /// install); the guess when Mojang does not answer.
    async fn java_missing(&self, ctx: &McContext, cancel: &CancellationToken) -> Result<u64> {
        if !matches!(self.java, JavaSetting::Auto) {
            return Ok(0);
        }
        let component = self.resolved.java_component();
        if installed_runtime(ctx, component).await.is_some() {
            return Ok(0);
        }
        let index = match pl_net::until_cancelled(cancel, runtime_index(ctx, false)).await {
            Ok(index) => index,
            Err(error) if error.is_cancelled() => return Err(error),
            Err(error) => {
                tracing::debug!(%error, "Java runtime index unknown; guessing its size");
                return Ok(JAVA_RUNTIME_GUESS);
            }
        };
        // Mojang publishes nothing for this platform: a Java of the system is used.
        let Some((platform_key, entry)) = select_runtime(&index, component, &ctx.platform) else {
            return Ok(0);
        };
        let root = ctx.dirs.runtime_dir(component, platform_key)?;
        let manifest = match download_manifest(ctx, entry, &root, cancel).await {
            Ok(manifest) => manifest,
            Err(error) if error.is_cancelled() => return Err(error),
            Err(error) => {
                tracing::debug!(%error, "Java runtime file list unknown; guessing its size");
                return Ok(JAVA_RUNTIME_GUESS);
            }
        };
        let items = manifest
            .files
            .iter()
            .filter(|(_, file)| file.kind == "file")
            .filter_map(|(name, file)| {
                let raw = &file.downloads.as_ref()?.raw;
                let dest = safe_join(&root, name).ok()?;
                Some(DownloadItem::new(&raw.url, dest).size(Some(raw.size)))
            })
            .collect();
        missing_bytes(items, cancel).await
    }
}

/// The client jar's download, where the install keeps it (under the version declaring it).
fn client_item(dirs: &DataDirs, resolved: &ResolvedVersion) -> Result<Option<DownloadItem>> {
    let Some(artifact) = resolved.client.as_ref() else {
        return Ok(None);
    };
    let Some(url) = artifact.url.as_deref().filter(|u| !u.trim().is_empty()) else {
        return Ok(None);
    };
    let origin = resolved
        .client_origin
        .as_deref()
        .unwrap_or(&resolved.jar_id);
    Ok(Some(
        DownloadItem::new(url, dirs.version_jar(origin)?).size(artifact.size),
    ))
}

/// Bytes of the files in `items` that are missing or have another size (each destination once;
/// a missing file of unknown size counts as nothing).
async fn missing_bytes(items: Vec<DownloadItem>, cancel: &CancellationToken) -> Result<u64> {
    let mut seen = HashSet::new();
    let items: Vec<DownloadItem> = items
        .into_iter()
        .filter(|item| seen.insert(item.dest.clone()))
        .collect();
    let checks = items.into_iter().map(|item| async move {
        if verify_file(&item.dest, None, item.size, VerifyMode::Size).await {
            0
        } else {
            item.size.unwrap_or(0)
        }
    });
    let total = futures::stream::iter(checks)
        .buffer_unordered(64)
        .fold(0u64, |sum, bytes| async move { sum.saturating_add(bytes) });
    pl_net::until_cancelled(cancel, async { Ok::<u64, Error>(total.await) }).await
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pl_net::{Http, HttpConfig, NetError};

    use super::*;
    use crate::platform::{Arch, Os, Platform};
    use crate::version::{VersionJson, resolve_with};

    const KB: u64 = 1024;
    const GB: u64 = 1024 * MB;

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    fn context(root: &Path) -> McContext {
        McContext::new(
            Http::new(HttpConfig::new("pl-mc-test")).unwrap(),
            DataDirs::new(root),
        )
        .with_platform(Platform::new(Os::Linux, Arch::X86_64, "6.1"))
    }

    /// A version with an asset index (`asset_index` JSON), two libraries (40 and 60 bytes), a
    /// 2000-byte client jar and a 7-byte log configuration. Nothing is fetched from its URLs.
    fn version(asset_index: &str) -> ResolvedVersion {
        let json: VersionJson = serde_json::from_str(&format!(
            r#"{{"id": "1.21", "type": "release", "mainClass": "net.minecraft.client.main.Main",
                "assetIndex": {asset_index},
                "downloads": {{"client": {{"size": 2000, "url": "https://example.invalid/client.jar"}}}},
                "logging": {{"client": {{"argument": "-Dlog4j.configurationFile=${{path}}",
                    "file": {{"id": "client-1.12.xml", "size": 7, "url": "https://example.invalid/l.xml"}},
                    "type": "log4j2-xml"}}}},
                "libraries": [
                    {{"name": "com.example:a:1.0", "downloads": {{"artifact": {{"path": "com/example/a/1.0/a-1.0.jar",
                        "url": "https://example.invalid/a.jar", "size": 40}}}}}},
                    {{"name": "com.example:b:1.0", "downloads": {{"artifact": {{"path": "com/example/b/1.0/b-1.0.jar",
                        "url": "https://example.invalid/b.jar", "size": 60}}}}}}
                ]}}"#
        ))
        .unwrap();
        resolve_with("1.21", |_| Ok(json.clone())).unwrap()
    }

    fn files(resolved: ResolvedVersion, java: JavaSetting, loader_extra: u64) -> GameFiles {
        GameFiles {
            resolved,
            loader_extra,
            java,
        }
    }

    fn custom_java() -> JavaSetting {
        JavaSetting::Custom {
            path: "/usr/bin/java".into(),
        }
    }

    #[test]
    fn nothing_to_download_needs_no_room() {
        assert_eq!(room_needed(0), 0);
        let free = Some(0);
        assert_eq!(
            shortfall(Target { free, missing: 0 }, Target { free, missing: 0 }),
            None
        );
    }

    #[test]
    fn background_preparation_leaves_a_reserve() {
        let missing = 600 * MB;
        let needed = room_needed(missing) + BACKGROUND_RESERVE;
        let at = |free: u64| {
            background_shortfall(
                Target {
                    free: Some(free),
                    missing,
                },
                Target {
                    free: Some(free),
                    missing: 0,
                },
            )
        };
        assert_eq!(at(needed), None);
        assert_eq!(
            at(needed - 1),
            Some(Shortfall {
                needed,
                free: needed - 1,
                missing,
            })
        );
        // What would still let the player's own launch go ahead.
        assert_eq!(
            shortfall(
                Target {
                    free: Some(needed - 1),
                    missing
                },
                Target {
                    free: Some(needed - 1),
                    missing: 0
                },
            ),
            None
        );
        assert!(background_tight(BACKGROUND_RESERVE - 1));
        assert!(!background_tight(BACKGROUND_RESERVE));
    }

    #[test]
    fn background_preparation_does_not_guess_an_unknown_disk() {
        let unknown = Target {
            free: None,
            missing: MB,
        };
        let nothing = Target {
            free: None,
            missing: 0,
        };
        assert!(background_shortfall(unknown, nothing).is_some());
        assert!(background_shortfall(nothing, unknown).is_some());
        // Nothing to download needs no room, known or not.
        assert_eq!(background_shortfall(nothing, nothing), None);
        // The player's own launch never stops for an unknown disk.
        assert_eq!(shortfall(unknown, unknown), None);
    }

    #[test]
    fn background_preparation_adds_up_one_disk() {
        let free = Some(room_needed(3 * GB) + BACKGROUND_RESERVE);
        let two = background_shortfall(
            Target {
                free,
                missing: 2 * GB,
            },
            Target { free, missing: GB },
        );
        assert_eq!(two, None);
        let more = background_shortfall(
            Target {
                free,
                missing: 2 * GB,
            },
            Target {
                free,
                missing: GB + MB,
            },
        );
        assert!(more.is_some_and(|short| short.missing == 3 * GB + MB));
    }

    #[test]
    fn the_margin_is_a_tenth_and_working_room() {
        // A small download asks for a little working room, not the full 200 MB.
        assert_eq!(room_needed(KB), KB + KB / 10 + WORK_ROOM_MIN);
        assert_eq!(room_needed(5 * MB), 5 * MB + MB / 2 + WORK_ROOM_MIN);
        assert_eq!(room_needed(100 * MB), 100 * MB + 10 * MB + 100 * MB);
        assert_eq!(room_needed(GB), GB + GB / 10 + WORK_ROOM_MAX);
        assert_eq!(room_needed(u64::MAX), u64::MAX);
    }

    #[test]
    fn one_disk_adds_both_downloads_up() {
        let shared = Target {
            free: Some(700 * MB),
            missing: 400 * MB,
        };
        let pack = Target {
            free: Some(700 * MB),
            missing: 100 * MB,
        };
        // 500 MB on one disk needs 500 + 50 + 200 MB.
        assert_eq!(
            shortfall(shared, pack),
            Some(Shortfall {
                needed: 750 * MB,
                free: 700 * MB,
                missing: 500 * MB
            })
        );
        assert_eq!(
            shortfall(
                Target {
                    free: Some(750 * MB),
                    ..shared
                },
                Target {
                    free: Some(750 * MB),
                    ..pack
                }
            ),
            None,
            "exactly enough room"
        );
    }

    #[test]
    fn separate_disks_hold_their_own_part() {
        let shared = Target {
            free: Some(10 * GB),
            missing: 400 * MB,
        };
        let pack = Target {
            free: Some(100 * MB),
            missing: 100 * MB,
        };
        let short = shortfall(shared, pack).unwrap();
        assert_eq!((short.free, short.missing), (100 * MB, 100 * MB));
        assert_eq!(
            shortfall(
                shared,
                Target {
                    free: Some(GB),
                    ..pack
                }
            ),
            None
        );
    }

    #[test]
    fn an_unknown_free_space_never_stops_a_launch() {
        let unknown = Target {
            free: None,
            missing: 10 * GB,
        };
        assert_eq!(shortfall(unknown, unknown), None);
        let short = shortfall(
            unknown,
            Target {
                free: Some(MB),
                missing: 10 * MB,
            },
        );
        assert_eq!(short.map(|s| s.free), Some(MB));
    }

    #[test]
    fn sizes_read_like_the_launcher() {
        let format_size = |bytes| format_size(bytes, pl_lang::Lang::Tr);
        // The cases of `formatBytes` (lib/format.test.ts).
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1536), "1,5 KB");
        assert_eq!(format_size(817_869), "798,7 KB");
        assert_eq!(format_size(1_048_471_142), "999,9 MB");
        assert_eq!(format_size(3 * GB / 2), "1,5 GB");
        assert_eq!(format_size(1000), "1 KB");
        assert_eq!(format_size(1023), "1 KB");
        assert_eq!(format_size(1000 * KB), "1 MB");
        assert_eq!(format_size(1_059_795_763), "1 GB");
        assert_eq!(format_size(1_073_636_966), "1 GB");
        assert_eq!(format_size(1_048_534_057), "1 GB");
        // The disk message's figures.
        assert_eq!(format_size(300 * MB), "300 MB");
        assert_eq!(format_size(1_288_490_189), "1,2 GB");
    }

    #[test]
    fn english_sizes_use_a_decimal_point() {
        let english = |bytes| format_size(bytes, pl_lang::Lang::En);
        assert_eq!(english(1_288_490_189), "1.2 GB");
        assert_eq!(english(817_869), "798.7 KB");
        assert_eq!(english(300 * MB), "300 MB");
    }

    #[test]
    fn the_free_space_stand_in_is_plain_bytes() {
        assert_eq!(parse_free_bytes(Some("314572800")), Some(300 * MB));
        assert_eq!(parse_free_bytes(Some(" 0 ")), Some(0));
        assert_eq!(parse_free_bytes(Some("")), None);
        assert_eq!(parse_free_bytes(Some("300MB")), None);
        assert_eq!(parse_free_bytes(None), None);
        // The e2e protocol and run scripts set this exact name.
        assert_eq!(FREE_BYTES_ENV, "PROJECTS_E2E_FREE_BYTES");
    }

    #[test]
    fn a_first_forge_install_is_guessed_bigger_than_fabric() {
        assert_eq!(loader_files_guess(LoaderKind::Vanilla), 0);
        assert!(loader_files_guess(LoaderKind::Fabric) < loader_files_guess(LoaderKind::Forge));
        assert_eq!(
            loader_files_guess(LoaderKind::NeoForge),
            loader_files_guess(LoaderKind::Forge)
        );
        assert!(nearly_full(NEARLY_FULL - 1));
        assert!(!nearly_full(NEARLY_FULL));
    }

    #[test]
    fn full_disks_are_recognised_however_they_failed() {
        let full = std::io::Error::from(std::io::ErrorKind::StorageFull);
        let text =
            std::io::Error::from_raw_os_error(if cfg!(windows) { 112 } else { 28 }).to_string();
        assert!(Error::io("/data/versions/1.21/1.21.jar", full).is_disk_full());
        assert!(
            Error::Process(format!("processor failed: java.io.IOException: {text}")).is_disk_full()
        );
        assert!(Error::archive("/data/natives.jar", &text).is_disk_full());
        let batch = NetError::Batch {
            failed: 2,
            total: 9,
            first: format!("I/O error at /data/assets/x.part: {text}"),
            offline: false,
        };
        assert!(Error::Net(batch).is_disk_full());
        assert!(!Error::NotFound("client jar".into()).is_disk_full());
        assert!(
            !Error::io(
                "/x",
                std::io::Error::from(std::io::ErrorKind::PermissionDenied)
            )
            .is_disk_full()
        );
    }

    #[tokio::test]
    async fn the_upper_bound_counts_every_file_named() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = context(tmp.path());
        let resolved = version(
            r#"{"id": "17", "size": 300, "totalSize": 5000, "url": "https://example.invalid/17.json"}"#,
        );
        let all = 300 + 5000 + 2000 + 7 + 40 + 60;
        let custom = files(resolved.clone(), custom_java(), 0);
        assert_eq!(custom.upper_bound(&ctx).await, Some(all));
        // An automatic Java that is not installed, and a mod loader installed for the first time.
        let auto = files(
            resolved,
            JavaSetting::Auto,
            loader_files_guess(LoaderKind::Fabric),
        );
        assert_eq!(
            auto.upper_bound(&ctx).await,
            Some(all + JAVA_RUNTIME_GUESS + loader_files_guess(LoaderKind::Fabric))
        );
        // Without the objects' total the bound is unknown: the files are looked at instead.
        let untold = files(
            version(r#"{"id": "17", "url": "https://example.invalid/17.json"}"#),
            custom_java(),
            0,
        );
        assert_eq!(untold.upper_bound(&ctx).await, None);
    }

    #[tokio::test]
    async fn only_missing_files_count() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = context(tmp.path());
        let dirs = &ctx.dirs;
        let hashes = [
            "1111111111111111111111111111111111111111",
            "2222222222222222222222222222222222222222",
            "3333333333333333333333333333333333333333",
        ];
        let index = format!(
            r#"{{"objects": {{"a": {{"hash": "{}", "size": 100}}, "b": {{"hash": "{}", "size": 200}},
                "c": {{"hash": "{}", "size": 300}}, "c-again": {{"hash": "{}", "size": 300}}}}}}"#,
            hashes[0], hashes[1], hashes[2], hashes[2]
        );
        let index_path = dirs.asset_index_file("17").unwrap();
        std::fs::create_dir_all(index_path.parent().unwrap()).unwrap();
        std::fs::write(&index_path, &index).unwrap();
        // The index is here with its declared size: it is not downloaded again.
        let resolved = version(&format!(
            r#"{{"id": "17", "size": {}, "totalSize": 600, "url": "https://example.invalid/17.json"}}"#,
            index.len()
        ));
        let game = files(resolved, custom_java(), 0);

        write(&dirs.asset_object(hashes[0]).unwrap(), 100);
        // Another size than the index says: downloaded again.
        write(&dirs.asset_object(hashes[2]).unwrap(), 10);
        write(
            &dirs.library_file("com/example/a/1.0/a-1.0.jar").unwrap(),
            40,
        );
        write(&dirs.log_configs().join("client-1.12.xml"), 7);
        let cancel = CancellationToken::new();
        assert_eq!(
            game.missing(&ctx, &cancel).await.unwrap(),
            200 + 300 + 60 + 2000
        );

        // A first loader install adds its guess on top.
        let modded = GameFiles {
            loader_extra: 10 * MB,
            ..game.clone()
        };
        assert_eq!(
            modded.missing(&ctx, &cancel).await.unwrap(),
            200 + 300 + 60 + 2000 + 10 * MB
        );

        // Everything in place: nothing to download, no room needed.
        write(&dirs.asset_object(hashes[1]).unwrap(), 200);
        write(&dirs.asset_object(hashes[2]).unwrap(), 300);
        write(
            &dirs.library_file("com/example/b/1.0/b-1.0.jar").unwrap(),
            60,
        );
        write(&dirs.version_jar("1.21").unwrap(), 2000);
        assert_eq!(game.missing(&ctx, &cancel).await.unwrap(), 0);
        assert_eq!(room_needed(0), 0);
    }

    #[tokio::test]
    async fn an_installed_loader_profile_names_its_own_files() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = context(tmp.path());
        let dirs = &ctx.dirs;
        let vanilla =
            r#"{"id": "1.20.1", "type": "release", "mainClass": "net.minecraft.client.main.Main"}"#;
        let fabric = r#"{"id": "fabric-loader-0.16.5-1.20.1", "inheritsFrom": "1.20.1",
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient"}"#;
        for (id, json) in [("1.20.1", vanilla), ("fabric-loader-0.16.5-1.20.1", fabric)] {
            let path = dirs.version_json(id).unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, json).unwrap();
        }
        let java = JavaSetting::Auto;
        let installed = GameFiles::for_launch(
            &ctx,
            "1.20.1",
            LoaderKind::Fabric,
            Some("0.16.5"),
            &java,
            None,
        )
        .await
        .unwrap();
        assert_eq!(installed.resolved.id, "fabric-loader-0.16.5-1.20.1");
        assert_eq!(installed.loader_extra, 0);

        // Not installed yet: the game's version and a guess for the loader's files.
        let first = GameFiles::for_launch(
            &ctx,
            "1.20.1",
            LoaderKind::Forge,
            Some("47.3.0"),
            &java,
            None,
        )
        .await
        .unwrap();
        assert_eq!(first.resolved.id, "1.20.1");
        assert_eq!(first.loader_extra, loader_files_guess(LoaderKind::Forge));

        let plain = GameFiles::for_launch(&ctx, "1.20.1", LoaderKind::Vanilla, None, &java, None)
            .await
            .unwrap();
        assert_eq!(plain.resolved.id, "1.20.1");
        assert_eq!(plain.loader_extra, 0);
    }
}
