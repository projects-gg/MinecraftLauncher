//! Forge and NeoForge.
//!
//! 1. When `versions/<id>/<id>.json` and the install marker ([`marker`]) exist and every file the
//!    installer step produced still verifies, only the core install runs (pre-launch checks,
//!    offline mode).
//! 2. Otherwise the installer jar is fetched (cached in `libraries/` as the `installer`
//!    classifier of the loader artifact, verified with the Maven `.sha1`) and read:
//!    - legacy profiles ([`legacy`]) are converted; the universal jar comes from the installer;
//!    - modern profiles: the libraries of `install_profile.json` are downloaded, jars shipped in
//!      the installer's `maven/` folder are extracted, and the client processors run
//!      ([`processors`]) with the Java runtime of the Minecraft version, skipping processors whose
//!      declared outputs already match.
//! 3. The marker and the profile are written, then the core installs everything else.

mod installer;
mod legacy;
mod marker;
mod processors;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use pl_net::{Checksum, DownloadItem, HashAlgo, NetError, VerifyMode, hash_file};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::install::{InstallOptions, InstallStage, McContext};
use crate::java::{EnsureJavaOptions, ensure_java};
use crate::loaders::LoaderKind;
use crate::loaders::common::{
    Progress, check_cancel, console_java, download, existing_profile, finish, manifest_for,
    prepare_profile, verify_mode, write_profile,
};
use crate::loaders::listing::{
    Endpoints, FORGE_MAVEN_URL, NEOFORGE_LEGACY_MC, NEOFORGE_MAVEN_URL, NeoForgeFilter,
    VersionEntry, loader_entries, read_cache,
};
use crate::paths::{DataDirs, safe_join};
use crate::version::{
    MavenCoord, ResolvedVersion, VersionJson, VersionManifest, load_manifest, resolve_version,
};

use installer::{
    Extraction, InstallProfile, ModernProfile, Processor, extract, jar_main_class, read_installer,
};
use marker::Marker;
use processors::{
    DataValue, Environment, Invocation, build_tokens, client_data, plan_invocation,
    run_invocation,
};

/// What is being installed.
#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub kind: LoaderKind,
    pub mc: String,
    /// Loader version as listed (Forge: loader part only).
    pub loader: String,
    pub id: String,
}

impl Target {
    fn is_legacy_neoforge(&self) -> bool {
        self.kind == LoaderKind::NeoForge && self.mc == NEOFORGE_LEGACY_MC
    }

    fn maven_base(&self) -> &'static str {
        if self.kind == LoaderKind::Forge {
            FORGE_MAVEN_URL
        } else {
            NEOFORGE_MAVEN_URL
        }
    }

    fn installer_coord(&self, maven_version: &str) -> Result<MavenCoord> {
        let (group, artifact) = match self.kind {
            LoaderKind::Forge => ("net.minecraftforge", "forge"),
            _ if self.is_legacy_neoforge() => ("net.neoforged", "forge"),
            _ => ("net.neoforged", "neoforge"),
        };
        MavenCoord::parse(&format!("{group}:{artifact}:{maven_version}:installer"))
    }

    fn describe(&self) -> String {
        format!(
            "{} {} for Minecraft {}",
            self.kind.display_name(),
            self.loader,
            self.mc
        )
    }

    /// Full Maven version of the loader artifact.
    async fn maven_version(&self, ctx: &McContext) -> Result<String> {
        match self.kind {
            LoaderKind::Forge => {
                if let Some(found) = read_cache(ctx, LoaderKind::Forge, &self.mc)
                    .await
                    .and_then(|cached| find_maven(cached, &self.mc, &self.loader))
                {
                    return Ok(found);
                }
                let entries =
                    loader_entries(ctx, &Endpoints::default(), LoaderKind::Forge, &self.mc).await?;
                find_maven(entries, &self.mc, &self.loader)
                    .ok_or_else(|| Error::NotFound(self.describe()))
            }
            LoaderKind::NeoForge if self.is_legacy_neoforge() => {
                Ok(format!("{}-{}", self.mc, self.loader))
            }
            LoaderKind::NeoForge => match NeoForgeFilter::for_minecraft(&self.mc) {
                Some(filter) if filter.matches(&self.loader) => Ok(self.loader.clone()),
                _ => Err(Error::Invalid(format!(
                    "NeoForge {} is not made for Minecraft {}",
                    self.loader, self.mc
                ))),
            },
            other => Err(Error::Unsupported(format!("{other:?} has no installer"))),
        }
    }
}

fn find_maven(entries: Vec<VersionEntry>, mc: &str, loader: &str) -> Option<String> {
    entries
        .into_iter()
        .find(|e| e.version == loader)
        .map(|e| e.maven.unwrap_or_else(|| format!("{mc}-{}", e.version)))
}

/// Result of the installer step.
pub(crate) struct Outcome {
    /// The launcher profile to write.
    pub profile: Value,
    /// Files for the install marker.
    pub files: Vec<PathBuf>,
    /// Known SHA-1s of some of `files` (just verified or extracted).
    pub known: HashMap<PathBuf, String>,
}

#[derive(Clone, Copy)]
struct Job<'a> {
    ctx: &'a McContext,
    /// Data directories with an absolute root (processors run in another working directory).
    dirs: &'a DataDirs,
    target: &'a Target,
    installer: &'a Path,
    options: &'a InstallOptions,
}

fn absolute_dirs(dirs: &DataDirs) -> Result<DataDirs> {
    let root = std::path::absolute(dirs.root()).map_err(|e| Error::io(dirs.root(), e))?;
    Ok(DataDirs::new(root))
}

pub(super) async fn install(
    ctx: &McContext,
    kind: LoaderKind,
    mc: &str,
    loader: &str,
    id: &str,
    options: &InstallOptions,
) -> Result<String> {
    let target = Target {
        kind,
        mc: mc.to_owned(),
        loader: loader.to_owned(),
        id: id.to_owned(),
    };
    let dirs = absolute_dirs(&ctx.dirs)?;

    // Already installed: only the core checks run (fast pre-launch path, offline mode).
    if (options.offline || options.verify != VerifyMode::Full)
        && existing_profile(ctx, id, mc).await.is_some()
        && let Some(marker) = marker::read(&dirs, id).await
        && marker.is_for(kind, mc, loader)
        && marker::verify(&dirs, &marker, verify_mode(options)).await
    {
        tracing::debug!(%id, "loader files verified, skipping the installer");
        let manifest = manifest_for(ctx, options).await?;
        return finish(ctx, id, manifest.as_ref(), options).await;
    }
    if options.offline {
        tracing::warn!(%id, "loader not (completely) installed and offline");
        return Err(Error::Offline);
    }

    check_cancel(options)?;
    let manifest = load_manifest(ctx, false).await?;
    let maven_version = target.maven_version(ctx).await?;
    let coord = target.installer_coord(&maven_version)?;
    let (installer_path, fresh) = fetch_installer(ctx, &dirs, &target, &coord, false, options).await?;
    let contents = match read_installer(installer_path.clone()).await {
        Ok(contents) => contents,
        Err(error @ (Error::Archive { .. } | Error::Json { .. })) if !fresh => {
            tracing::warn!(%error, "cached installer is unreadable, downloading it again");
            let (path, _) = fetch_installer(ctx, &dirs, &target, &coord, true, options).await?;
            read_installer(path).await?
        }
        Err(error) => return Err(error),
    };

    let job = Job {
        ctx,
        dirs: &dirs,
        target: &target,
        installer: &installer_path,
        options,
    };
    let outcome = match contents.profile {
        InstallProfile::Legacy(profile) => {
            let conversion = legacy::convert(&profile, id, mc)?;
            legacy::install(ctx, &dirs, &installer_path, conversion, options).await?
        }
        InstallProfile::Modern(profile) => {
            install_modern(job, *profile, &contents.maven, &manifest).await?
        }
    };

    check_cancel(options)?;
    let files = marker::describe(&dirs, outcome.files, &outcome.known).await?;
    marker::write(&dirs, id, &Marker::new(kind, mc, loader, files)).await?;
    write_profile(ctx, id, &outcome.profile).await?;
    tracing::info!(%id, installer = %installer_path.display(), "loader profile written");
    finish(ctx, id, Some(&manifest), options).await
}

fn parse_sha1(text: &str) -> Option<String> {
    let token = text.split_whitespace().next()?;
    (token.len() == 40 && token.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| token.to_ascii_lowercase())
}

/// The installer jar, cached in `libraries/`. Returns its path and whether it was fetched or
/// verified in this call. `force` discards the cached copy.
async fn fetch_installer(
    ctx: &McContext,
    dirs: &DataDirs,
    target: &Target,
    coord: &MavenCoord,
    force: bool,
    options: &InstallOptions,
) -> Result<(PathBuf, bool)> {
    let path = dirs.library_file(&coord.path())?;
    let cached = tokio::fs::metadata(&path)
        .await
        .is_ok_and(|m| m.is_file() && m.len() > 0);
    if cached && !force && options.verify != VerifyMode::Full {
        return Ok((path, false));
    }
    check_cancel(options)?;
    Progress::new(options).stage(InstallStage::Metadata);
    let url = coord.url(target.maven_base());
    let (checksum, missing) = match ctx.http.get_text(&format!("{url}.sha1")).await {
        Ok(text) => (parse_sha1(&text).map(Checksum::sha1), false),
        Err(NetError::Status { status: 404, .. }) => (None, true),
        Err(error) if error.is_network() => return Err(error.into()),
        Err(error) => {
            tracing::warn!(%error, %url, "installer checksum unavailable");
            (None, false)
        }
    };
    if force && cached {
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| Error::io(&path, e))?;
    }
    let item = DownloadItem::new(&url, &path).checksum(checksum);
    match download(ctx, vec![item], InstallStage::Metadata, options).await {
        Ok(()) => Ok((path, true)),
        Err(error) if missing && !error.is_cancelled() => {
            tracing::warn!(%error, %url, "installer not found");
            Err(Error::NotFound(target.describe()))
        }
        Err(error) => Err(error),
    }
}

async fn install_modern(
    job: Job<'_>,
    profile: ModernProfile,
    maven: &BTreeSet<String>,
    manifest: &VersionManifest,
) -> Result<Outcome> {
    let Job {
        ctx,
        dirs,
        target,
        installer,
        options,
    } = job;
    if let Some(minecraft) = profile.minecraft.as_deref()
        && minecraft != target.mc
    {
        return Err(Error::Invalid(format!(
            "this {} installer is for Minecraft {minecraft}, not {}",
            target.kind.display_name(),
            target.mc
        )));
    }
    let what = "installer version.json";
    let profile_value =
        prepare_profile(profile.version_json.clone(), &target.id, &target.mc, what)?;
    let version: VersionJson =
        serde_json::from_value(profile_value.clone()).map_err(|e| Error::json(what, e))?;
    let progress = Progress::new(options);
    progress.stage(InstallStage::Libraries);

    // Libraries of both files: install_profile ones are downloaded here (processor tools and,
    // since 1.17, jars FML loads by coordinate); empty URLs are shipped in `maven/` or made by
    // processors; version.json ones are left to the core installer.
    let mut files = Vec::new();
    let mut known = HashMap::new();
    let mut extractions: Vec<Extraction> = Vec::new();
    let mut downloads = Vec::new();
    let from_profile = profile.libraries.iter().map(|l| (l, true));
    let from_version = version.libraries.iter().map(|l| (l, false));
    for (library, in_install_profile) in from_profile.chain(from_version) {
        if !library.applies_to(&ctx.platform) {
            continue;
        }
        let Some(artifact) = library.artifact() else {
            continue;
        };
        let dest = dirs.library_file(&artifact.path)?;
        let sha1 = artifact
            .checksum
            .as_ref()
            .filter(|c| c.algo == HashAlgo::Sha1)
            .map(|c| c.hex.clone());
        if artifact.urls.is_empty() {
            if maven.contains(&artifact.path) && !extractions.iter().any(|e| e.dest == dest) {
                extractions.push(Extraction {
                    entry: format!("maven/{}", artifact.path),
                    dest,
                    sha1,
                });
            }
        } else if in_install_profile {
            if let Some(sha1) = sha1 {
                known.insert(dest.clone(), sha1);
            }
            files.push(dest.clone());
            downloads.push(
                DownloadItem::with_urls(artifact.urls.clone(), &dest)
                    .checksum(artifact.checksum.clone())
                    .size(artifact.size),
            );
        }
    }
    for extracted in extract(installer, extractions).await? {
        known.insert(extracted.dest.clone(), extracted.sha1);
        files.push(extracted.dest);
    }
    download(ctx, downloads, InstallStage::Libraries, options).await?;

    let processors: Vec<&Processor> = profile
        .processors
        .iter()
        .filter(|p| p.runs_on_client())
        .collect();
    if !processors.is_empty() {
        let data = client_data(&profile.data, &dirs.libraries())?;
        let previous = marker::read(dirs, &target.id)
            .await
            .filter(|m| m.is_for(target.kind, &target.mc, &target.loader));
        let verified = match &previous {
            Some(previous) => marker::verify(dirs, previous, verify_mode(options)).await,
            None => false,
        };
        match previous {
            Some(previous) if verified => {
                tracing::info!(id = %target.id, "installer outputs verified, skipping processors");
                for file in previous.files {
                    let path = safe_join(dirs.root(), &file.path)?;
                    known.insert(path.clone(), file.sha1);
                    files.push(path);
                }
            }
            _ => {
                let outputs = run_processors(job, &profile, &processors, &data, manifest).await?;
                files.extend(outputs);
            }
        }
        files.extend(data.into_iter().filter_map(|(_, value)| match value {
            DataValue::Artifact(path) => Some(path),
            _ => None,
        }));
    }
    Ok(Outcome {
        profile: profile_value,
        files,
        known,
    })
}

/// Scratch folder for installer data files, removed on drop.
struct WorkDir(PathBuf);

impl WorkDir {
    async fn create(dirs: &DataDirs) -> Result<Self> {
        let path = dirs
            .cache()
            .join("loader-install")
            .join(uuid::Uuid::new_v4().simple().to_string());
        tokio::fs::create_dir_all(&path)
            .await
            .map_err(|e| Error::io(&path, e))?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            tracing::debug!(%error, path = %self.0.display(), "could not remove installer work folder");
        }
    }
}

/// `versions/<mc>/<mc>.jar` (the processors' `{MINECRAFT_JAR}`).
async fn vanilla_client_jar(
    ctx: &McContext,
    dirs: &DataDirs,
    vanilla: &ResolvedVersion,
    options: &InstallOptions,
) -> Result<PathBuf> {
    let missing = || Error::NotFound(format!("client jar of Minecraft {}", vanilla.id));
    let client = vanilla.client.as_ref().ok_or_else(missing)?;
    let url = client
        .url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(missing)?;
    let origin = vanilla.client_origin.as_deref().unwrap_or(&vanilla.jar_id);
    let path = dirs.version_jar(origin)?;
    Progress::new(options).stage(InstallStage::Client);
    let item = DownloadItem::new(url, &path)
        .checksum(client.checksum())
        .size(client.size);
    download(ctx, vec![item], InstallStage::Client, options).await?;
    Ok(path)
}

/// Whether every declared output exists with its checksum; damaged outputs are deleted (like
/// the official installer) so the processor writes them again.
async fn outputs_match(outputs: &[(PathBuf, String)]) -> bool {
    let mut all = true;
    for (path, sha1) in outputs {
        if !path.is_file() {
            all = false;
            continue;
        }
        let actual = hash_file(path, HashAlgo::Sha1).await.ok();
        if actual.as_deref() != Some(sha1.as_str()) {
            let _ = tokio::fs::remove_file(path).await;
            all = false;
        }
    }
    all
}

async fn check_outputs(invocation: &Invocation) -> Result<()> {
    for (path, sha1) in &invocation.outputs {
        let actual = if path.is_file() {
            hash_file(path, HashAlgo::Sha1).await?
        } else {
            return Err(Error::Process(format!(
                "installer step {} did not create {}",
                invocation.name,
                path.display()
            )));
        };
        if &actual != sha1 {
            let _ = tokio::fs::remove_file(path).await;
            return Err(Error::Process(format!(
                "installer step {} created {} with checksum {actual}, expected {sha1}",
                invocation.name,
                path.display()
            )));
        }
    }
    Ok(())
}

/// Run the client processors; returns their declared outputs.
async fn run_processors(
    job: Job<'_>,
    profile: &ModernProfile,
    processors: &[&Processor],
    data: &[(String, DataValue)],
    manifest: &VersionManifest,
) -> Result<Vec<PathBuf>> {
    let Job {
        ctx,
        dirs,
        target,
        installer,
        options,
    } = job;
    let progress = Progress::new(options);

    // The Minecraft version's Java and vanilla client jar.
    check_cancel(options)?;
    let vanilla = resolve_version(ctx, &target.mc, Some(manifest)).await?;
    progress.stage(InstallStage::Java);
    let java = ensure_java(
        ctx,
        &vanilla,
        &options.java,
        &EnsureJavaOptions {
            offline: false,
            // The core install re-verifies the runtime afterwards.
            verify: VerifyMode::Size,
            cancel: options.cancel.clone(),
            progress: progress.download_fn(InstallStage::Java),
        },
    )
    .await?;
    let java_exe = console_java(&java.path);
    let minecraft_jar = vanilla_client_jar(ctx, dirs, &vanilla, options).await?;

    // Installer files named by the data map (`/data/client.lzma`).
    let work = WorkDir::create(dirs).await?;
    let mut extractions: Vec<Extraction> = Vec::new();
    for (_, value) in data {
        if let DataValue::InstallerFile(entry) = value
            && !extractions.iter().any(|e| &e.entry == entry)
        {
            extractions.push(Extraction {
                entry: entry.clone(),
                dest: safe_join(work.path(), entry)?,
                sha1: None,
            });
        }
    }
    let extracted: HashMap<String, PathBuf> = extractions
        .iter()
        .map(|e| (e.entry.clone(), e.dest.clone()))
        .collect();
    extract(installer, extractions).await?;
    let libraries = dirs.libraries();
    let minecraft_version = profile.minecraft.as_deref().unwrap_or(&target.mc);
    let tokens = build_tokens(
        data,
        &extracted,
        &Environment {
            root: dirs.root(),
            libraries: &libraries,
            installer,
            minecraft_jar: &minecraft_jar,
            minecraft_version,
        },
    )?;

    let separator = ctx.platform.classpath_separator();
    let total = processors.len();
    let started = Instant::now();
    let mut outputs = Vec::new();
    for (index, processor) in processors.iter().enumerate() {
        check_cancel(options)?;
        progress.files(InstallStage::Libraries, index, total);
        let invocation = plan_invocation(processor, &tokens, &libraries)?;
        outputs.extend(invocation.outputs.iter().map(|(path, _)| path.clone()));
        if !invocation.outputs.is_empty() && outputs_match(&invocation.outputs).await {
            tracing::info!(processor = %invocation.name, "outputs already valid, skipping");
            continue;
        }
        if let Some(missing) = invocation.classpath.iter().find(|p| !p.is_file()) {
            return Err(Error::NotFound(format!(
                "{} (needed by installer step {})",
                missing.display(),
                invocation.name
            )));
        }
        let main_class = jar_main_class(invocation.jar.clone()).await?;
        let step = Instant::now();
        run_invocation(
            &java_exe,
            &main_class,
            &invocation,
            separator,
            work.path(),
            &options.cancel,
        )
        .await?;
        check_outputs(&invocation).await?;
        tracing::info!(
            processor = %invocation.name,
            elapsed_ms = step.elapsed().as_millis() as u64,
            "installer step finished"
        );
    }
    progress.files(InstallStage::Libraries, total, total);
    tracing::info!(
        id = %target.id,
        processors = total,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "installer processors finished"
    );
    drop(work);
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_coordinates() {
        let target = |kind, mc: &str, loader: &str| Target {
            kind,
            mc: mc.into(),
            loader: loader.into(),
            id: "x".into(),
        };
        let forge = target(LoaderKind::Forge, "1.7.10", "10.13.4.1614");
        let coord = forge
            .installer_coord("1.7.10-10.13.4.1614-1.7.10")
            .unwrap();
        assert_eq!(
            coord.url(forge.maven_base()),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.7.10-10.13.4.1614-1.7.10/forge-1.7.10-10.13.4.1614-1.7.10-installer.jar"
        );
        let neo = target(LoaderKind::NeoForge, "1.21.1", "21.1.262");
        assert_eq!(
            neo.installer_coord("21.1.262")
                .unwrap()
                .url(neo.maven_base()),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.262/neoforge-21.1.262-installer.jar"
        );
        let legacy = target(LoaderKind::NeoForge, "1.20.1", "47.1.106");
        assert!(legacy.is_legacy_neoforge());
        assert_eq!(
            legacy
                .installer_coord("1.20.1-47.1.106")
                .unwrap()
                .url(legacy.maven_base()),
            "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar"
        );
        assert_eq!(
            parse_sha1("66BFEA9963BFA60D88BAB6B2750E74A958392715  forge.jar\n").as_deref(),
            Some("66bfea9963bfa60d88bab6b2750e74a958392715")
        );
        assert_eq!(parse_sha1("<html>"), None);
        assert_eq!(
            find_maven(
                vec![VersionEntry {
                    version: "10.13.4.1614".into(),
                    stable: true,
                    recommended: true,
                    maven: Some("1.7.10-10.13.4.1614-1.7.10".into()),
                }],
                "1.7.10",
                "10.13.4.1614"
            )
            .as_deref(),
            Some("1.7.10-10.13.4.1614-1.7.10")
        );
    }

    #[tokio::test]
    async fn neoforge_versions_must_match_minecraft() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = McContext::new(
            pl_net::Http::new(pl_net::HttpConfig::new("pl-mc-test")).unwrap(),
            DataDirs::new(tmp.path()),
        );
        let target = Target {
            kind: LoaderKind::NeoForge,
            mc: "1.21.1".into(),
            loader: "21.1.262".into(),
            id: "neoforge-21.1.262".into(),
        };
        assert_eq!(target.maven_version(&ctx).await.unwrap(), "21.1.262");
        let wrong = Target {
            mc: "1.21.4".into(),
            ..target.clone()
        };
        assert!(matches!(
            wrong.maven_version(&ctx).await,
            Err(Error::Invalid(_))
        ));
        let year = Target {
            mc: "26.3".into(),
            loader: "26.3.0.83".into(),
            ..target
        };
        assert_eq!(year.maven_version(&ctx).await.unwrap(), "26.3.0.83");
    }
}
