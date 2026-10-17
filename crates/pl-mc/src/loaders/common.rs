//! Helpers shared by the loader installers: progress, downloads, profile files and the final
//! `install_version` pass.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pl_net::{DownloadItem, DownloadOptions, DownloadProgress, NetError, ProgressFn, VerifyMode};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::install::{InstallOptions, InstallProgress, InstallStage, McContext, install_version};
use crate::paths::write_atomic;
use crate::version::{
    ResolvedVersion, VersionJson, VersionManifest, load_manifest, read_local_version,
    resolve_version,
};

/// Progress reporting for loader work. The loader's own work (profile, installer, installer
/// libraries, processors) is [`InstallStage::Loader`], counted in steps so its bar only moves
/// forward ([`Self::loader`], [`Self::loader_download_fn`]): one per installer file and one per
/// processor. The Java runtime and the client jar the processors need report `Java` and `Client`
/// like the core install does.
pub(super) struct Progress<'a> {
    options: &'a InstallOptions,
}

impl<'a> Progress<'a> {
    pub fn new(options: &'a InstallOptions) -> Self {
        Self { options }
    }

    /// `done` of `total` loader steps (`total` 0: not known yet).
    pub fn loader(&self, done: usize, total: usize) {
        self.files(InstallStage::Loader, done.min(total), total);
    }

    /// Loader steps while files download: `done` steps before this download plus its finished
    /// files, of `total`, with the download speed (no byte counts, which would restart the bar).
    pub fn loader_download_fn(&self, done: usize, total: usize) -> Option<ProgressFn> {
        self.options.progress.clone().map(|progress| {
            Arc::new(move |download: DownloadProgress| {
                progress(InstallProgress {
                    stage: InstallStage::Loader,
                    download: DownloadProgress {
                        total_files: total,
                        done_files: (done + download.done_files).min(total),
                        bytes_per_sec: download.bytes_per_sec,
                        ..DownloadProgress::default()
                    },
                })
            }) as ProgressFn
        })
    }

    pub fn report(&self, stage: InstallStage, download: DownloadProgress) {
        if let Some(progress) = &self.options.progress {
            progress(InstallProgress { stage, download });
        }
    }

    pub fn stage(&self, stage: InstallStage) {
        self.report(stage, DownloadProgress::default());
    }

    pub fn files(&self, stage: InstallStage, done: usize, total: usize) {
        self.report(
            stage,
            DownloadProgress {
                total_files: total,
                done_files: done,
                ..DownloadProgress::default()
            },
        );
    }

    pub fn download_fn(&self, stage: InstallStage) -> Option<ProgressFn> {
        self.options.progress.clone().map(|progress| {
            Arc::new(move |download| progress(InstallProgress { stage, download })) as ProgressFn
        })
    }
}

pub(super) fn check_cancel(options: &InstallOptions) -> Result<()> {
    if options.cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}

pub(super) fn net_error(error: NetError) -> Error {
    if error.is_cancelled() {
        Error::Cancelled
    } else {
        Error::Net(error)
    }
}

/// How existing files are checked: like `install_version`, offline only checks existence.
pub(super) fn verify_mode(options: &InstallOptions) -> VerifyMode {
    if options.offline {
        VerifyMode::Exists
    } else {
        options.verify
    }
}

/// Download (or verify) `items` for one stage.
pub(super) async fn download(
    ctx: &McContext,
    items: Vec<DownloadItem>,
    stage: InstallStage,
    options: &InstallOptions,
) -> Result<()> {
    let progress = Progress::new(options).download_fn(stage);
    download_reporting(ctx, items, progress, options).await
}

/// [`download`] reporting through `progress` (loader steps, see [`Progress`]).
pub(super) async fn download_reporting(
    ctx: &McContext,
    items: Vec<DownloadItem>,
    progress: Option<ProgressFn>,
    options: &InstallOptions,
) -> Result<()> {
    check_cancel(options)?;
    if items.is_empty() {
        return Ok(());
    }
    let download_options = DownloadOptions {
        concurrency: options.concurrency.max(1),
        verify: verify_mode(options),
        cancel: options.cancel.clone(),
        progress,
        progress_interval: Duration::from_millis(120),
    };
    ctx.downloader
        .download_all(items, &download_options)
        .await
        .map_err(net_error)?;
    Ok(())
}

/// The version manifest when online (cached copy when Mojang is unreachable), `None` offline.
pub(super) async fn manifest_for(
    ctx: &McContext,
    options: &InstallOptions,
) -> Result<Option<VersionManifest>> {
    if options.offline {
        Ok(None)
    } else {
        load_manifest(ctx, false).await.map(Some)
    }
}

/// Resolve the loader profile and install everything it needs (vanilla parent, libraries,
/// assets, Java, natives, client jar).
pub(super) async fn finish(
    ctx: &McContext,
    id: &str,
    manifest: Option<&VersionManifest>,
    options: &InstallOptions,
) -> Result<String> {
    check_cancel(options)?;
    let resolved: ResolvedVersion = resolve_version(ctx, id, manifest).await?;
    install_version(ctx, &resolved, options.clone()).await?;
    Ok(id.to_owned())
}

/// An installed profile `id` that inherits from `mc` and has a main class.
pub(super) async fn existing_profile(ctx: &McContext, id: &str, mc: &str) -> Option<VersionJson> {
    match read_local_version(ctx, id).await {
        Ok(Some(json))
            if json.id == id
                && json.inherits_from.as_deref() == Some(mc)
                && json
                    .main_class
                    .as_deref()
                    .is_some_and(|m| !m.trim().is_empty()) =>
        {
            Some(json)
        }
        Ok(Some(_)) => {
            tracing::warn!(%id, "existing loader profile does not match, reinstalling it");
            None
        }
        Ok(None) => None,
        Err(error) => {
            tracing::warn!(%id, %error, "existing loader profile is unreadable, reinstalling it");
            None
        }
    }
}

/// Force `id`/`inheritsFrom` on a profile from a loader and check that it is a usable version
/// JSON. A profile made for another Minecraft version is rejected.
pub(super) fn prepare_profile(mut value: Value, id: &str, mc: &str, what: &str) -> Result<Value> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| Error::Invalid(format!("{what} is not a JSON object")))?;
    if let Some(parent) = object.get("inheritsFrom").and_then(Value::as_str)
        && !parent.trim().is_empty()
        && parent != mc
    {
        return Err(Error::Invalid(format!(
            "{what} is made for Minecraft {parent}, not {mc}"
        )));
    }
    object.insert("id".into(), Value::from(id));
    object.insert("inheritsFrom".into(), Value::from(mc));
    let parsed: VersionJson =
        serde_json::from_value(value.clone()).map_err(|e| Error::json(what, e))?;
    if parsed
        .main_class
        .as_deref()
        .is_none_or(|m| m.trim().is_empty())
    {
        return Err(Error::Invalid(format!("{what} has no mainClass")));
    }
    Ok(value)
}

/// Write `versions/<id>/<id>.json` atomically.
pub(super) async fn write_profile(ctx: &McContext, id: &str, value: &Value) -> Result<()> {
    let path = ctx.dirs.version_json(id)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| Error::json(id, e))?;
    write_atomic(&path, &bytes).await
}

/// `java.exe` next to `javaw.exe`: processors are console programs whose output we read.
pub(super) fn console_java(path: &Path) -> PathBuf {
    if path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe"))
    {
        let java = path.with_file_name("java.exe");
        if java.is_file() {
            return java;
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_progress_counts_steps_and_never_goes_back() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::<InstallProgress>::new()));
        let sink = Arc::clone(&seen);
        let options = InstallOptions {
            progress: Some(Arc::new(move |p| sink.lock().unwrap().push(p))),
            ..InstallOptions::default()
        };
        let progress = Progress::new(&options);
        progress.loader(1, 10);
        let download = progress.loader_download_fn(3, 10).unwrap();
        download(DownloadProgress {
            total_files: 4,
            done_files: 2,
            total_bytes: 500,
            done_bytes: 300,
            bytes_per_sec: 77,
        });
        download(DownloadProgress {
            total_files: 4,
            done_files: 9,
            ..DownloadProgress::default()
        });
        progress.loader(12, 10);
        let seen = seen.lock().unwrap();
        assert!(seen.iter().all(|p| p.stage == InstallStage::Loader));
        let steps: Vec<(usize, usize, u64, u64)> = seen
            .iter()
            .map(|p| {
                let d = p.download;
                (d.done_files, d.total_files, d.total_bytes, d.bytes_per_sec)
            })
            .collect();
        assert_eq!(
            steps,
            vec![
                (1, 10, 0, 0),
                (5, 10, 0, 77),
                (10, 10, 0, 0),
                (10, 10, 0, 0)
            ],
            "steps only, the download's own bytes never restart the bar"
        );
    }

    #[test]
    fn profiles_get_our_id_and_parent() {
        let value: Value = serde_json::from_str(
            r#"{"id": "fabric-loader-0.16.10-1.21.1", "inheritsFrom": "1.21.1",
                "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient", "custom": [1, 2],
                "libraries": [{"name": "net.fabricmc:fabric-loader:0.16.10", "url": "https://maven.fabricmc.net/"}]}"#,
        )
        .unwrap();
        let prepared = prepare_profile(value.clone(), "my-id", "1.21.1", "profile").unwrap();
        assert_eq!(prepared["id"], "my-id");
        assert_eq!(prepared["inheritsFrom"], "1.21.1");
        assert_eq!(prepared["custom"][1], 2, "unknown fields are kept");

        assert!(matches!(
            prepare_profile(value, "x", "1.20.1", "profile"),
            Err(Error::Invalid(_))
        ));
        let no_main: Value = serde_json::from_str(r#"{"id": "a", "libraries": []}"#).unwrap();
        assert!(matches!(
            prepare_profile(no_main, "a", "1.21.1", "profile"),
            Err(Error::Invalid(_))
        ));
        let legacy: Value =
            serde_json::from_str(r#"{"id": "1.6.4-Forge9.11.1.1345", "mainClass": "M"}"#).unwrap();
        let prepared = prepare_profile(legacy, "1.6.4-forge-9.11.1.1345", "1.6.4", "p").unwrap();
        assert_eq!(prepared["inheritsFrom"], "1.6.4");
        assert!(prepare_profile(Value::from(3), "a", "b", "p").is_err());
    }
}
