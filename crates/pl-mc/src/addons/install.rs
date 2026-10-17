//! Installing Modrinth versions into a game directory.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use pl_net::{
    CancellationToken, Checksum, DownloadItem, DownloadOptions, DownloadProgress, Downloader,
    HashAlgo, ProgressFn, VerifyMode,
};
use serde::{Deserialize, Serialize};

use super::index::{AddonRecord, AddonSource, DISABLED_SUFFIX, index_key};
use super::{AddonEntry, Addons, SeenFile, now_ms};
use crate::error::{Error, Result};
use crate::gamefiles::ImportKind;
use crate::gamefiles::fsops::modified_ms;
use crate::modpack::safe_path::validate_file_name;
use crate::modrinth::{
    ModrinthClient, ModrinthDependencyConflict, ModrinthDependencyResolution, ModrinthInstalled,
    ModrinthResolveTarget, ModrinthUnresolvedDependency, ModrinthVersion,
};

/// Progress of [`Addons::install`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum AddonProgress {
    /// Looking at installed files and resolving dependencies.
    Resolving,
    #[serde(rename_all = "camelCase")]
    Downloading {
        done_files: u64,
        total_files: u64,
        done_bytes: u64,
        total_bytes: u64,
        bytes_per_sec: u64,
    },
    /// Replacing older files and updating the index.
    Finishing,
}

pub type AddonProgressFn = Arc<dyn Fn(AddonProgress) + Send + Sync>;

#[derive(Clone)]
pub struct AddonInstallOptions {
    /// Also install required dependencies (mods only).
    pub with_dependencies: bool,
    /// Parallel downloads.
    pub concurrency: usize,
    pub cancel: CancellationToken,
    pub progress: Option<AddonProgressFn>,
}

impl Default for AddonInstallOptions {
    fn default() -> Self {
        Self {
            with_dependencies: true,
            concurrency: 6,
            cancel: CancellationToken::new(),
            progress: None,
        }
    }
}

impl std::fmt::Debug for AddonInstallOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddonInstallOptions")
            .field("with_dependencies", &self.with_dependencies)
            .field("concurrency", &self.concurrency)
            .finish()
    }
}

/// A file [`Addons::install`] put in place (or found already there).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddonInstalled {
    pub kind: ImportKind,
    pub project_id: String,
    pub version_id: String,
    pub version_number: String,
    pub title: Option<String>,
    pub file_name: String,
    /// Installed because another project requires it.
    pub dependency: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddonInstallReport {
    /// New files, the requested versions first, then dependencies.
    pub installed: Vec<AddonInstalled>,
    /// Requested versions that were already installed.
    pub already_installed: Vec<AddonInstalled>,
    /// Older files of the same projects that were removed.
    pub replaced: Vec<String>,
    /// Required dependencies that could not be installed.
    pub unresolved: Vec<ModrinthUnresolvedDependency>,
    /// Installed projects that a new version declares incompatible.
    pub conflicts: Vec<ModrinthDependencyConflict>,
}

struct Planned {
    kind: ImportKind,
    version: ModrinthVersion,
    title: Option<String>,
    icon_url: Option<String>,
    dependency: bool,
}

impl Planned {
    fn report(&self, file_name: &str) -> AddonInstalled {
        AddonInstalled {
            kind: self.kind,
            project_id: self.version.project_id.clone(),
            version_id: self.version.id.clone(),
            version_number: self.version.version_number.clone(),
            title: self.title.clone(),
            file_name: file_name.to_owned(),
            dependency: self.dependency,
        }
    }
}

fn checksum_of(file: &crate::modrinth::ModrinthFile) -> Option<Checksum> {
    match (&file.hashes.sha512, &file.hashes.sha1) {
        (Some(sha512), _) if !sha512.trim().is_empty() => {
            Some(Checksum::new(HashAlgo::Sha512, sha512))
        }
        (_, Some(sha1)) if !sha1.trim().is_empty() => Some(Checksum::new(HashAlgo::Sha1, sha1)),
        _ => None,
    }
}

impl Addons {
    /// Install `roots` (Modrinth versions of projects of `kind`) and, for mods, their required
    /// dependencies compatible with `target`. Files are verified against Modrinth's hashes.
    /// Older files of the same projects are removed afterwards; a project the player had
    /// disabled stays disabled.
    pub async fn install(
        &self,
        client: &ModrinthClient,
        downloader: &Downloader,
        kind: ImportKind,
        roots: Vec<ModrinthVersion>,
        target: &ModrinthResolveTarget,
        options: AddonInstallOptions,
    ) -> Result<AddonInstallReport> {
        let progress = |p: AddonProgress| {
            if let Some(report) = &options.progress {
                report(p);
            }
        };
        let cancel = &options.cancel;
        progress(AddonProgress::Resolving);

        let mut index = self.load_index().await;
        let files = self.scan_into(&mut index, kind).await?;
        // Best effort: an unknown file only weakens dependency skipping and replacement.
        self.lookup_sources(client, &mut index, kind.folder()).await;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let sources: HashMap<String, AddonSource> = files
            .iter()
            .filter_map(|f| {
                let source = index.files.get(&f.key)?.source.clone()?;
                Some((f.key.clone(), source))
            })
            .collect();
        let source_of = |f: &SeenFile| sources.get(&f.key);

        let resolution = if options.with_dependencies && kind == ImportKind::Mod {
            let installed = ModrinthInstalled {
                project_ids: files
                    .iter()
                    .filter_map(|f| source_of(f).map(|s| s.project_id.clone()))
                    .collect(),
                hashes: files.iter().map(|f| f.sha1.clone()).collect(),
            };
            tokio::select! {
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                resolved = client.resolve_dependencies(&roots, target, &installed) => resolved?,
            }
        } else {
            ModrinthDependencyResolution::default()
        };

        // Titles and icons of the requested projects (dependencies already carry theirs).
        let root_ids: Vec<String> = roots.iter().map(|v| v.project_id.clone()).collect();
        let root_info: HashMap<String, (String, Option<String>)> =
            match client.projects(&root_ids).await {
                Ok(projects) => projects
                    .into_iter()
                    .map(|p| (p.id, (p.title, p.icon_url)))
                    .collect(),
                Err(error) => {
                    tracing::debug!(%error, "could not fetch project titles");
                    HashMap::new()
                }
            };
        let mut plan: Vec<Planned> = roots
            .into_iter()
            .map(|version| {
                let (title, icon_url) = root_info
                    .get(&version.project_id)
                    .cloned()
                    .map_or((None, None), |(t, i)| (Some(t), i));
                Planned {
                    kind,
                    version,
                    title,
                    icon_url,
                    dependency: false,
                }
            })
            .collect();
        plan.extend(resolution.to_install.into_iter().map(|dep| Planned {
            kind: ImportKind::Mod,
            version: dep.version,
            title: dep.title,
            icon_url: dep.icon_url,
            dependency: true,
        }));

        let mut report = AddonInstallReport {
            unresolved: resolution.unresolved,
            conflicts: resolution.conflicts,
            ..AddonInstallReport::default()
        };
        let mut items = Vec::new();
        let mut todo: Vec<(Planned, String, Option<String>)> = Vec::new();
        for planned in plan {
            let file = planned.version.primary_file().ok_or_else(|| {
                Error::NotFound(format!("files of Modrinth version {}", planned.version.id))
            })?;
            validate_file_name(&file.filename)?;
            let folder = planned.kind.folder();
            let key = index_key(folder, &file.filename);
            let wanted_sha1 = file.hashes.sha1.as_deref().map(str::to_ascii_lowercase);
            // The exact version (or the exact file, enabled or not) is already there.
            let existing = files.iter().find(|f| {
                f.kind == planned.kind
                    && (source_of(f).is_some_and(|s| s.version_id == planned.version.id)
                        || (f.key == key && wanted_sha1.as_deref() == Some(f.sha1.as_str())))
            });
            if let Some(existing) = existing {
                if !planned.dependency {
                    report
                        .already_installed
                        .push(planned.report(&existing.file_name));
                }
                continue;
            }
            let dest = self.game_dir().join(folder).join(&file.filename);
            items.push(
                DownloadItem::new(file.url.clone(), dest)
                    .checksum(checksum_of(file))
                    .size(Some(file.size)),
            );
            let name = file.filename.clone();
            todo.push((planned, name, wanted_sha1));
        }

        if !items.is_empty() {
            let download_progress: Option<ProgressFn> = options.progress.clone().map(|report| {
                Arc::new(move |p: DownloadProgress| {
                    report(AddonProgress::Downloading {
                        done_files: p.done_files as u64,
                        total_files: p.total_files as u64,
                        done_bytes: p.done_bytes,
                        total_bytes: p.total_bytes,
                        bytes_per_sec: p.bytes_per_sec,
                    })
                }) as ProgressFn
            });
            downloader
                .download_all(
                    items,
                    &DownloadOptions {
                        concurrency: options.concurrency.max(1),
                        verify: VerifyMode::Full,
                        cancel: cancel.clone(),
                        progress: download_progress,
                        progress_interval: Duration::from_millis(150),
                    },
                )
                .await?;
        }
        progress(AddonProgress::Finishing);

        for (planned, file_name, sha1) in todo {
            let folder = planned.kind.folder();
            let dest = self.game_dir().join(folder).join(&file_name);
            let key = index_key(folder, &file_name);
            // Older files of the project, and a disabled twin of the new file name.
            let old: Vec<&SeenFile> = files
                .iter()
                .filter(|f| f.kind == planned.kind && f.path != dest)
                .filter(|f| {
                    f.key == key
                        || source_of(f).is_some_and(|s| s.project_id == planned.version.project_id)
                })
                .collect();
            let was_disabled = old.iter().any(|f| !f.enabled);
            for file in old {
                match self.delete_file(&file.path).await {
                    Ok(_) => {
                        report.replaced.push(file.file_name.clone());
                        if file.key != key {
                            index.files.remove(&file.key);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(path = %file.path.display(), %error, "could not remove the old file");
                    }
                }
            }
            let mut final_name = file_name.clone();
            if was_disabled {
                let disabled_name = format!("{file_name}{DISABLED_SUFFIX}");
                let disabled = dest.with_file_name(&disabled_name);
                if tokio::fs::symlink_metadata(&disabled).await.is_err()
                    && tokio::fs::rename(&dest, &disabled).await.is_ok()
                {
                    final_name = disabled_name;
                }
            }
            let path = dest.with_file_name(&final_name);
            let meta = tokio::fs::metadata(&path)
                .await
                .map_err(|e| Error::io(&path, e))?;
            let sha1 = match sha1 {
                Some(sha1) if sha1.len() == 40 => sha1,
                _ => pl_net::hash_file(&path, HashAlgo::Sha1).await?,
            };
            index.files.insert(
                key,
                AddonRecord {
                    size: meta.len(),
                    modified_ms: modified_ms(&meta),
                    sha1,
                    source: Some(AddonSource {
                        project_id: planned.version.project_id.clone(),
                        version_id: planned.version.id.clone(),
                        version_number: Some(planned.version.version_number.clone()),
                        title: planned.title.clone(),
                        icon_url: planned.icon_url.clone(),
                    }),
                    looked_up_ms: Some(now_ms()),
                },
            );
            report.installed.push(planned.report(&final_name));
        }
        self.save_index(&index).await?;

        let fresh: Vec<AddonEntry> = report
            .installed
            .iter()
            .filter_map(|installed| {
                let key = index_key(installed.kind.folder(), &installed.file_name);
                let record = index.files.get(&key)?;
                Some(AddonEntry {
                    kind: installed.kind,
                    file_name: installed.file_name.clone(),
                    enabled: !installed.file_name.ends_with(DISABLED_SUFFIX),
                    size: record.size,
                    sha1: record.sha1.clone(),
                    source: record.source.clone(),
                    update: None,
                })
            })
            .collect();
        self.cache_icons(client.http(), &fresh).await;
        Ok(report)
    }
}
