//! Installing a resolved version: metadata (asset index, log config), Java, libraries, asset
//! objects, the client jar and natives.
//!
//! [`install_version`] is idempotent: files that already verify are skipped, so it doubles as
//! the "repair" and the pre-launch check. Offline mode never touches the network and fails with
//! [`Error::Offline`] when something is missing.

pub mod assets;
pub mod context;
pub mod libraries;
pub mod natives;

#[cfg(test)]
mod integration_tests;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use pl_net::{
    CancellationToken, DownloadItem, DownloadOptions, DownloadProgress, NetError, ProgressFn,
    VerifyMode, verify_file,
};
use serde::{Deserialize, Serialize};

pub use assets::{AssetIndex, AssetLayout, AssetObject, AssetsInfo, RESOURCES_URL};
pub use context::McContext;
pub use libraries::{LibraryPlan, NativeJar, plan_libraries};
pub use natives::{
    extract_natives, is_native_library, library_path_subdir, natives_key, natives_subdirs,
};

use crate::error::{Error, Result};
use crate::java::{EnsureJavaOptions, JavaInstallation, JavaSetting, ensure_java};
use crate::paths::safe_name;
use crate::platform::{Arch, Platform};
use crate::version::ResolvedVersion;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum InstallStage {
    Metadata,
    Java,
    Libraries,
    Assets,
    Client,
    Natives,
    Done,
}

mod ts {
    /// TypeScript shape of [`pl_net::DownloadProgress`] (pl-net does not depend on specta).
    #[derive(serde::Serialize, specta::Type)]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)]
    pub struct DownloadProgress {
        pub total_files: usize,
        pub done_files: usize,
        pub total_bytes: u64,
        pub done_bytes: u64,
        pub bytes_per_sec: u64,
    }
}

/// Progress event: the current stage and the download counters of that stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub stage: InstallStage,
    #[specta(type = ts::DownloadProgress)]
    pub download: DownloadProgress,
}

pub type InstallProgressFn = Arc<dyn Fn(InstallProgress) + Send + Sync>;

#[derive(Clone)]
pub struct InstallOptions {
    pub java: JavaSetting,
    /// No network at all: existing files are trusted, anything missing is [`Error::Offline`].
    pub offline: bool,
    /// How existing files are checked (ignored offline, where [`VerifyMode::Exists`] is used).
    /// `Full` re-hashes everything (repair); `Size` is the fast pre-launch check.
    pub verify: VerifyMode,
    /// Game directory, needed for pre-1.6 versions that expect assets in `<game dir>/resources`.
    pub game_dir: Option<PathBuf>,
    /// Parallel downloads (asset objects use twice as many).
    pub concurrency: usize,
    pub cancel: CancellationToken,
    pub progress: Option<InstallProgressFn>,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            java: JavaSetting::Auto,
            offline: false,
            verify: VerifyMode::Full,
            game_dir: None,
            concurrency: 16,
            cancel: CancellationToken::new(),
            progress: None,
        }
    }
}

impl std::fmt::Debug for InstallOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallOptions")
            .field("java", &self.java)
            .field("offline", &self.offline)
            .field("verify", &self.verify)
            .field("game_dir", &self.game_dir)
            .field("concurrency", &self.concurrency)
            .field("has_progress", &self.progress.is_some())
            .finish()
    }
}

/// Everything a launch needs from an installed version.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledVersion {
    pub java: JavaInstallation,
    /// Platform used for rules and natives: the OS with the architecture of the chosen Java.
    pub platform: Platform,
    /// Libraries in order, client jar last.
    pub classpath: Vec<PathBuf>,
    pub client_jar: PathBuf,
    /// `${natives_directory}`.
    pub natives_dir: PathBuf,
    pub assets: AssetsInfo,
    /// log4j configuration referenced by `logging.client.argument`, when available.
    pub logging_config: Option<PathBuf>,
}

/// Install (or verify/repair) everything `resolved` needs.
pub async fn install_version(
    ctx: &McContext,
    resolved: &ResolvedVersion,
    options: InstallOptions,
) -> Result<InstalledVersion> {
    let run = Run {
        ctx,
        verify: if options.offline {
            VerifyMode::Exists
        } else {
            options.verify
        },
        options: &options,
    };
    let concurrency = options.concurrency.max(1);
    let started = std::time::Instant::now();

    // Metadata: asset index and logging configuration.
    let (index_path, index_item) = assets::index_download(&ctx.dirs, resolved)?;
    let logging = logging_download(ctx, resolved)?;
    let mut metadata: Vec<DownloadItem> = index_item.into_iter().collect();
    if let Some((_, Some(item))) = &logging {
        metadata.push(item.clone());
    }
    run.fetch(InstallStage::Metadata, metadata, concurrency)
        .await?;
    let asset_index = assets::read_index(&index_path).await?;
    let logging_config = logging.map(|(path, _)| path).filter(|p| p.is_file());

    // Java decides the architecture used for natives and rules.
    run.check_cancel()?;
    run.report(InstallStage::Java, DownloadProgress::default());
    let java = ensure_java(
        ctx,
        resolved,
        &options.java,
        &EnsureJavaOptions {
            offline: options.offline,
            verify: run.verify,
            cancel: options.cancel.clone(),
            progress: run.progress_for(InstallStage::Java),
        },
    )
    .await?;
    let platform = match java.arch() {
        Arch::Other => ctx.platform.clone(),
        arch => ctx.platform.with_arch(arch),
    };

    // Libraries.
    let plan = plan_libraries(&ctx.dirs, resolved, &platform)?;
    run.fetch(InstallStage::Libraries, plan.downloads.clone(), concurrency)
        .await?;

    // Asset objects and old layouts.
    let objects = assets::object_downloads(&ctx.dirs, &asset_index)?;
    run.fetch(InstallStage::Assets, objects, concurrency * 2)
        .await?;
    let assets_info = assets::assets_info(&ctx.dirs, resolved.assets_id(), &asset_index)?;
    assets::materialize_layout(
        &ctx.dirs,
        &assets_info,
        &asset_index,
        options.game_dir.as_deref(),
    )
    .await?;

    // Client jar.
    let client_jar = run.client_jar(resolved).await?;

    // Natives.
    run.check_cancel()?;
    run.report(
        InstallStage::Natives,
        DownloadProgress {
            total_files: plan.natives.len(),
            ..DownloadProgress::default()
        },
    );
    let natives_dir =
        ctx.dirs
            .natives_dir(&natives_key(&ctx.dirs, resolved, &platform, &plan.natives))?;
    let progress = options.progress.clone();
    extract_natives(
        plan.natives.clone(),
        natives_dir.clone(),
        library_path_subdir(resolved),
        natives_subdirs(resolved),
        options.cancel.clone(),
        move |done, total| {
            if let Some(progress) = &progress {
                progress(InstallProgress {
                    stage: InstallStage::Natives,
                    download: DownloadProgress {
                        total_files: total,
                        done_files: done,
                        ..DownloadProgress::default()
                    },
                });
            }
        },
    )
    .await?;

    run.report(InstallStage::Done, DownloadProgress::default());
    tracing::info!(
        version = %resolved.id,
        java = %java.path.display(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        offline = options.offline,
        "version ready"
    );

    let mut classpath = plan.classpath;
    if !classpath.contains(&client_jar) {
        classpath.push(client_jar.clone());
    }
    Ok(InstalledVersion {
        java,
        platform,
        classpath,
        client_jar,
        natives_dir,
        assets: assets_info,
        logging_config,
    })
}

/// Where the log4j config goes and how to fetch it.
fn logging_download(
    ctx: &McContext,
    resolved: &ResolvedVersion,
) -> Result<Option<(PathBuf, Option<DownloadItem>)>> {
    let Some(file) = resolved.logging.as_ref().and_then(|l| l.file.as_ref()) else {
        return Ok(None);
    };
    let path = ctx.dirs.log_configs().join(safe_name(&file.id)?);
    let item = file
        .url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .map(|url| {
            DownloadItem::new(url, &path)
                .checksum(file.sha1.as_deref().map(pl_net::Checksum::sha1))
                .size(file.size)
        });
    Ok(Some((path, item)))
}

fn net_error(error: NetError) -> Error {
    if error.is_cancelled() {
        Error::Cancelled
    } else {
        Error::Net(error)
    }
}

struct Run<'a> {
    ctx: &'a McContext,
    options: &'a InstallOptions,
    verify: VerifyMode,
}

impl Run<'_> {
    fn report(&self, stage: InstallStage, download: DownloadProgress) {
        if let Some(progress) = &self.options.progress {
            progress(InstallProgress { stage, download });
        }
    }

    fn progress_for(&self, stage: InstallStage) -> Option<ProgressFn> {
        self.options.progress.clone().map(|progress| {
            Arc::new(move |download| progress(InstallProgress { stage, download })) as ProgressFn
        })
    }

    fn check_cancel(&self) -> Result<()> {
        if self.options.cancel.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Download (online) or check (offline) a batch of files for one stage.
    async fn fetch(
        &self,
        stage: InstallStage,
        items: Vec<DownloadItem>,
        concurrency: usize,
    ) -> Result<()> {
        self.check_cancel()?;
        self.report(stage, DownloadProgress::default());

        // Files produced by loader installers (Forge processors) have no URL.
        let (local, remote): (Vec<DownloadItem>, Vec<DownloadItem>) =
            items.into_iter().partition(|item| item.urls.is_empty());
        for item in &local {
            if !verify_file(&item.dest, item.checksum.as_ref(), item.size, self.verify).await {
                tracing::warn!(path = %item.dest.display(), "generated library is missing or damaged");
                return Err(Error::NotFound(format!(
                    "{} (created by the mod loader installer)",
                    item.dest.display()
                )));
            }
        }

        if self.options.offline {
            let total = remote.len();
            let missing: Vec<PathBuf> =
                futures::stream::iter(remote.into_iter().map(|item| async move {
                    let present = verify_file(&item.dest, None, None, VerifyMode::Exists).await;
                    (!present).then_some(item.dest)
                }))
                .buffer_unordered(64)
                .filter_map(|missing| async move { missing })
                .collect()
                .await;
            self.report(
                stage,
                DownloadProgress {
                    total_files: total,
                    done_files: total - missing.len(),
                    ..DownloadProgress::default()
                },
            );
            if let Some(first) = missing.first() {
                tracing::warn!(
                    ?stage,
                    missing = missing.len(),
                    first = %first.display(),
                    "files missing in offline mode"
                );
                return Err(Error::Offline);
            }
            return Ok(());
        }

        let options = DownloadOptions {
            concurrency,
            verify: self.verify,
            cancel: self.options.cancel.clone(),
            progress: self.progress_for(stage),
            progress_interval: Duration::from_millis(120),
        };
        self.ctx
            .downloader
            .download_all(remote, &options)
            .await
            .map_err(net_error)?;
        Ok(())
    }

    /// `versions/<jar_id>/<jar_id>.jar`, downloaded once under the version declaring it and
    /// hard-linked (or copied) into the leaf folder for loader profiles.
    async fn client_jar(&self, resolved: &ResolvedVersion) -> Result<PathBuf> {
        let dirs = &self.ctx.dirs;
        let target = dirs.version_jar(&resolved.jar_id)?;
        let download = resolved.client.as_ref().and_then(|artifact| {
            let url = artifact.url.as_deref().filter(|u| !u.trim().is_empty())?;
            Some((artifact, url))
        });
        let Some((artifact, url)) = download else {
            self.report(InstallStage::Client, DownloadProgress::default());
            if target.is_file() {
                return Ok(target);
            }
            return Err(if self.options.offline {
                Error::Offline
            } else {
                Error::NotFound(format!("client jar of {}", resolved.id))
            });
        };
        let origin = resolved
            .client_origin
            .as_deref()
            .unwrap_or(&resolved.jar_id);
        let canonical = dirs.version_jar(origin)?;
        let checksum = artifact.checksum();
        let item = DownloadItem::new(url, &canonical)
            .checksum(checksum.clone())
            .size(artifact.size);
        self.fetch(InstallStage::Client, vec![item], 1).await?;
        if canonical != target
            && !verify_file(&target, checksum.as_ref(), artifact.size, self.verify).await
        {
            link_or_copy(&canonical, &target).await?;
        }
        Ok(target)
    }
}

/// Hard-link `source` to `dest` (copy when links are not possible), replacing `dest`.
async fn link_or_copy(source: &Path, dest: &Path) -> Result<()> {
    let source = source.to_path_buf();
    let dest = dest.to_path_buf();
    let dest_for_error = dest.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        if dest.exists() {
            std::fs::remove_file(&dest).map_err(|e| Error::io(&dest, e))?;
        }
        if std::fs::hard_link(&source, &dest).is_ok() {
            return Ok(());
        }
        let bytes = std::fs::read(&source).map_err(|e| Error::io(&source, e))?;
        crate::paths::write_atomic_blocking(&dest, &bytes)
    })
    .await
    .map_err(|e| Error::io(dest_for_error, std::io::Error::other(e.to_string())))?
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use parking_lot::Mutex;
    use pl_net::{Http, HttpConfig};

    use super::*;
    use crate::paths::DataDirs;
    use crate::platform::Os;
    use crate::version::{VersionJson, resolve_with};

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// Offline installs never touch the network: everything missing is `Error::Offline`, and a
    /// complete set of files (including a loader-style profile) installs fine.
    #[tokio::test]
    async fn offline_install_uses_only_local_files() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = McContext::new(
            Http::new(HttpConfig::new("pl-mc-test")).unwrap(),
            DataDirs::new(tmp.path()),
        )
        .with_platform(Platform::new(Os::Linux, Arch::X86_64, "6.1"));
        let dirs = ctx.dirs.clone();

        // An installed Mojang Java 8 runtime.
        let runtime = dirs.runtime_dir("jre-legacy", "linux").unwrap();
        write(&runtime.join("bin").join("java"), b"");
        write(
            &runtime.join(".projects-runtime.json"),
            br#"{"component": "jre-legacy", "platform": "linux", "version": "8u202", "manifestSha1": "x"}"#,
        );

        let vanilla: VersionJson = serde_json::from_str(
            r#"{"id": "1.8.9", "type": "release", "mainClass": "net.minecraft.client.main.Main",
                "minecraftArguments": "--username ${auth_player_name} --assetsDir ${assets_root}",
                "assetIndex": {"id": "1.8", "sha1": "00", "url": "https://example.invalid/1.8.json"},
                "downloads": {"client": {"sha1": "00", "url": "https://example.invalid/client.jar"}},
                "libraries": [
                    {"name": "com.example:lib:1.0"},
                    {"name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4", "natives": {"linux": "natives-linux"},
                     "extract": {"exclude": ["META-INF/"]}}
                ]}"#,
        )
        .unwrap();
        let profile: VersionJson = serde_json::from_str(
            r#"{"id": "loader-1.8.9", "inheritsFrom": "1.8.9", "mainClass": "loader.Main",
                "libraries": [{"name": "com.example:loader:2.0", "url": "https://example.invalid/maven/"}]}"#,
        )
        .unwrap();
        let resolved = resolve_with("loader-1.8.9", |id| {
            Ok(if id == "1.8.9" {
                vanilla.clone()
            } else {
                profile.clone()
            })
        })
        .unwrap();

        let stages = Arc::new(Mutex::new(Vec::<InstallStage>::new()));
        let seen = Arc::clone(&stages);
        let options = move || InstallOptions {
            offline: true,
            progress: Some(Arc::new({
                let seen = Arc::clone(&seen);
                move |p: InstallProgress| {
                    let mut seen = seen.lock();
                    if seen.last() != Some(&p.stage) {
                        seen.push(p.stage);
                    }
                }
            })),
            ..InstallOptions::default()
        };
        assert!(matches!(
            install_version(&ctx, &resolved, options()).await,
            Err(Error::Offline)
        ));

        // Provide every file.
        let hash = "a9993e364706816aba3e25717850c26c9cd0d89d";
        write(
            &dirs.asset_index_file("1.8").unwrap(),
            format!(r#"{{"objects": {{"icons/icon.png": {{"hash": "{hash}", "size": 3}}}}}}"#)
                .as_bytes(),
        );
        write(&dirs.asset_object(hash).unwrap(), b"abc");
        write(
            &dirs
                .library_file("com/example/lib/1.0/lib-1.0.jar")
                .unwrap(),
            b"jar",
        );
        write(
            &dirs
                .library_file("com/example/loader/2.0/loader-2.0.jar")
                .unwrap(),
            b"jar",
        );
        let natives_jar = dirs
            .library_file(
                "org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-linux.jar",
            )
            .unwrap();
        std::fs::create_dir_all(natives_jar.parent().unwrap()).unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&natives_jar).unwrap());
        let file_options = zip::write::SimpleFileOptions::default();
        zip.start_file("META-INF/MANIFEST.MF", file_options)
            .unwrap();
        zip.write_all(b"x").unwrap();
        zip.start_file("liblwjgl64.so", file_options).unwrap();
        zip.write_all(b"elf").unwrap();
        zip.finish().unwrap();
        write(&dirs.version_jar("1.8.9").unwrap(), b"client");

        stages.lock().clear();
        let installed = install_version(&ctx, &resolved, options()).await.unwrap();
        assert_eq!(
            *stages.lock(),
            [
                InstallStage::Metadata,
                InstallStage::Java,
                InstallStage::Libraries,
                InstallStage::Assets,
                InstallStage::Client,
                InstallStage::Natives,
                InstallStage::Done
            ]
        );
        assert_eq!(installed.java.info.major, 8);
        assert_eq!(installed.platform.arch, Arch::X86_64);
        // Loader library first, then vanilla, then the client jar linked into the leaf folder.
        assert_eq!(installed.classpath.len(), 3);
        assert!(installed.classpath[0].ends_with("loader-2.0.jar"));
        assert_eq!(
            installed.client_jar,
            dirs.version_jar("loader-1.8.9").unwrap()
        );
        assert_eq!(std::fs::read(&installed.client_jar).unwrap(), b"client");
        assert!(installed.natives_dir.join("liblwjgl64.so").is_file());
        assert!(!installed.natives_dir.join("META-INF").exists());
        assert_eq!(installed.assets.layout, AssetLayout::Standard);
        assert!(installed.logging_config.is_none());
    }
}
