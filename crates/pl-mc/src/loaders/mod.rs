//! Mod loaders. The installers (Fabric, Quilt, Forge, NeoForge) live in submodules of this folder;
//! [`LoaderKind`] is shared by the whole crate (instances, mods, Modrinth, modpacks).
//!
//! - [`list_loader_versions`]: installable loader versions for a Minecraft version, newest first,
//!   cached in `meta/loaders/` so the list also works offline.
//! - [`install_loader`]: writes the loader profile `versions/<id>/<id>.json` (inheriting from the
//!   vanilla version), runs the loader's own install steps (Forge/NeoForge installers) and the
//!   core `install_version`, and returns the version id to launch.
//! - [`loader_profile_id`]: that id, without installing anything.

mod common;
mod fabric;
mod forge;
mod listing;
mod order;

#[cfg(test)]
mod integration_tests;

use std::time::Instant;

use serde::{Deserialize, Serialize};

pub use listing::{LoaderVersion, list_loader_versions};

use crate::error::{Error, Result};
use crate::install::{InstallOptions, McContext};
use crate::paths::safe_name;
use listing::NEOFORGE_LEGACY_MC;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl LoaderKind {
    /// Loader name as used by the Modrinth API (`loaders` facet).
    pub fn modrinth_name(self) -> Option<&'static str> {
        match self {
            Self::Vanilla => None,
            Self::Fabric => Some("fabric"),
            Self::Quilt => Some("quilt"),
            Self::Forge => Some("forge"),
            Self::NeoForge => Some("neoforge"),
        }
    }

    /// Key used in the `.mrpack` `dependencies` object.
    pub fn mrpack_dependency_key(self) -> Option<&'static str> {
        match self {
            Self::Vanilla => None,
            Self::Fabric => Some("fabric-loader"),
            Self::Quilt => Some("quilt-loader"),
            Self::Forge => Some("forge"),
            Self::NeoForge => Some("neoforge"),
        }
    }

    pub fn from_mrpack_dependency_key(key: &str) -> Option<Self> {
        match key {
            "fabric-loader" => Some(Self::Fabric),
            "quilt-loader" => Some(Self::Quilt),
            "forge" => Some(Self::Forge),
            "neoforge" => Some(Self::NeoForge),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

/// Loader version as used in ids and by the installers: trimmed; Forge Maven versions
/// (`1.20.1-47.4.10`, `1.7.10-10.13.4.1614-1.7.10`) are reduced to the loader part and NeoForge
/// 1.20.1 Maven versions (`1.20.1-47.1.106`) lose the Minecraft prefix.
fn normalize_loader_version(kind: LoaderKind, mc: &str, loader: &str) -> Result<String> {
    if kind == LoaderKind::Vanilla {
        return Ok(String::new());
    }
    let loader = loader.trim();
    let without_mc = loader
        .strip_prefix(mc)
        .and_then(|rest| rest.strip_prefix('-'));
    let normalized = match kind {
        LoaderKind::Forge => {
            let rest = without_mc.unwrap_or(loader);
            rest.split('-').next().unwrap_or(rest)
        }
        LoaderKind::NeoForge if mc == NEOFORGE_LEGACY_MC => without_mc.unwrap_or(loader),
        _ => loader,
    };
    if normalized.is_empty()
        || normalized
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(Error::Invalid(format!(
            "{} version `{loader}`",
            kind.display_name()
        )));
    }
    Ok(normalized.to_owned())
}

/// Version id [`install_loader`] installs and returns for this loader:
/// `fabric-loader-<loader>-<mc>`, `quilt-loader-<loader>-<mc>`, `<mc>-forge-<loader>`,
/// `neoforge-<loader>` (`1.20.1-neoforge-<loader>` for NeoForge on 1.20.1), or `mc` for vanilla.
pub fn loader_profile_id(
    kind: LoaderKind,
    mc_version: &str,
    loader_version: &str,
) -> Result<String> {
    let mc = mc_version.trim();
    safe_name(mc)?;
    let loader = normalize_loader_version(kind, mc, loader_version)?;
    let id = match kind {
        LoaderKind::Vanilla => mc.to_owned(),
        LoaderKind::Fabric => format!("fabric-loader-{loader}-{mc}"),
        LoaderKind::Quilt => format!("quilt-loader-{loader}-{mc}"),
        LoaderKind::Forge => format!("{mc}-forge-{loader}"),
        LoaderKind::NeoForge if mc == NEOFORGE_LEGACY_MC => format!("{mc}-neoforge-{loader}"),
        LoaderKind::NeoForge => format!("neoforge-{loader}"),
    };
    safe_name(&id)?;
    Ok(id)
}

/// Install `kind` `loader_version` for Minecraft `mc_version` and everything a launch needs;
/// returns the version id to launch (a profile that `inheritsFrom` the vanilla version).
///
/// Idempotent: files that verify (per `options.verify`) are kept, so this is also the repair and
/// the pre-launch check. With `options.offline` nothing is downloaded; it succeeds when the loader
/// was installed before and fails with [`Error::Offline`] otherwise. `LoaderKind::Vanilla`
/// installs the vanilla version (`loader_version` is ignored).
///
/// Errors: [`Error::NotFound`] for unknown loader versions, [`Error::Process`] when a Forge or
/// NeoForge installer step fails (with the end of its output), [`Error::Invalid`] for profiles or
/// installers that do not fit, [`Error::Cancelled`] when `options.cancel` fires.
pub async fn install_loader(
    ctx: &McContext,
    mc_version: &str,
    kind: LoaderKind,
    loader_version: &str,
    options: InstallOptions,
) -> Result<String> {
    let mc = mc_version.trim();
    let id = loader_profile_id(kind, mc, loader_version)?;
    let loader = normalize_loader_version(kind, mc, loader_version)?;
    let started = Instant::now();
    let result = match kind {
        LoaderKind::Vanilla => match common::manifest_for(ctx, &options).await {
            Ok(manifest) => common::finish(ctx, &id, manifest.as_ref(), &options).await,
            Err(error) => Err(error),
        },
        LoaderKind::Fabric | LoaderKind::Quilt => {
            fabric::install(ctx, kind, mc, &loader, &id, &options).await
        }
        LoaderKind::Forge | LoaderKind::NeoForge => {
            forge::install(ctx, kind, mc, &loader, &id, &options).await
        }
    };
    match &result {
        Ok(_) => tracing::info!(
            %id,
            offline = options.offline,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "loader ready"
        ),
        Err(error) => tracing::warn!(%id, %error, "loader install failed"),
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pl_net::{Http, HttpConfig};

    use super::*;
    use crate::paths::DataDirs;
    use crate::platform::{Arch, Os, Platform};

    #[test]
    fn mrpack_keys_round_trip() {
        for kind in [
            LoaderKind::Fabric,
            LoaderKind::Quilt,
            LoaderKind::Forge,
            LoaderKind::NeoForge,
        ] {
            let key = kind.mrpack_dependency_key().unwrap();
            assert_eq!(LoaderKind::from_mrpack_dependency_key(key), Some(kind));
        }
        assert_eq!(LoaderKind::Vanilla.mrpack_dependency_key(), None);
    }

    #[test]
    fn profile_ids() {
        let id = |kind, mc, loader| loader_profile_id(kind, mc, loader).unwrap();
        assert_eq!(
            id(LoaderKind::Fabric, "1.21.1", "0.16.10"),
            "fabric-loader-0.16.10-1.21.1"
        );
        assert_eq!(
            id(LoaderKind::Quilt, "1.21.1", " 0.29.2 "),
            "quilt-loader-0.29.2-1.21.1"
        );
        assert_eq!(
            id(LoaderKind::Forge, "1.20.1", "47.4.10"),
            "1.20.1-forge-47.4.10"
        );
        assert_eq!(
            id(LoaderKind::Forge, "1.20.1", "1.20.1-47.4.10"),
            "1.20.1-forge-47.4.10",
            "Maven versions are accepted"
        );
        assert_eq!(
            id(LoaderKind::Forge, "1.7.10", "1.7.10-10.13.4.1614-1.7.10"),
            "1.7.10-forge-10.13.4.1614"
        );
        assert_eq!(
            id(LoaderKind::Forge, "1.7.10", "10.13.4.1614-1.7.10"),
            "1.7.10-forge-10.13.4.1614"
        );
        assert_eq!(
            id(LoaderKind::NeoForge, "1.21.1", "21.1.262"),
            "neoforge-21.1.262"
        );
        assert_eq!(
            id(LoaderKind::NeoForge, "26.3", "26.3.0.83"),
            "neoforge-26.3.0.83"
        );
        assert_eq!(
            id(LoaderKind::NeoForge, "1.20.1", "1.20.1-47.1.106"),
            "1.20.1-neoforge-47.1.106"
        );
        assert_eq!(id(LoaderKind::Vanilla, "1.21.1", "ignored"), "1.21.1");
        for (kind, mc, loader) in [
            (LoaderKind::Fabric, "1.21.1", ""),
            (LoaderKind::Fabric, "1.21.1", "a b"),
            (LoaderKind::Fabric, "1.21.1", "../x"),
            (LoaderKind::Forge, "1.20.1", "1.20.1-"),
            (LoaderKind::Fabric, "../1.21.1", "0.16.10"),
            (LoaderKind::Fabric, "", "0.16.10"),
        ] {
            assert!(
                loader_profile_id(kind, mc, loader).is_err(),
                "{kind:?} {mc} {loader}"
            );
        }
    }

    /// Public futures must be `Send` when awaited inside a spawned task (Tauri commands).
    #[test]
    fn public_futures_are_send() {
        fn require_send<F: std::future::Future + Send + 'static>(_future: F) {}
        let ctx = McContext::new(
            Http::new(HttpConfig::new("pl-mc-test")).unwrap(),
            DataDirs::new("unused"),
        );
        let list_ctx = ctx.clone();
        require_send(async move {
            let _ = list_loader_versions(&list_ctx, LoaderKind::Forge, "1.20.1").await;
        });
        require_send(async move {
            let _ = install_loader(
                &ctx,
                "1.20.1",
                LoaderKind::Forge,
                "47.4.10",
                InstallOptions::default(),
            )
            .await;
        });
    }

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// Offline installs use only what is on disk: a missing profile (or, for Forge, an install
    /// marker whose files no longer verify) is `Error::Offline`; a complete install succeeds.
    #[tokio::test]
    async fn offline_installs_use_installed_files_only() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = McContext::new(
            Http::new(HttpConfig::new("pl-mc-test")).unwrap(),
            DataDirs::new(tmp.path()),
        )
        .with_platform(Platform::new(Os::Linux, Arch::X86_64, "6.1"));
        let dirs = ctx.dirs.clone();
        let offline = || InstallOptions {
            offline: true,
            ..InstallOptions::default()
        };

        // An installed vanilla 1.12.2 with its Java 8 runtime.
        let runtime = dirs.runtime_dir("jre-legacy", "linux").unwrap();
        write(&runtime.join("bin").join("java"), b"");
        write(
            &runtime.join(".projects-runtime.json"),
            br#"{"component": "jre-legacy", "platform": "linux", "version": "8u202", "manifestSha1": "x"}"#,
        );
        write(
            &dirs.version_json("1.12.2").unwrap(),
            br#"{"id": "1.12.2", "type": "release", "mainClass": "net.minecraft.client.main.Main",
                "minecraftArguments": "--username ${auth_player_name}",
                "assetIndex": {"id": "1.12", "sha1": "00", "url": "https://example.invalid/1.12.json"},
                "downloads": {"client": {"sha1": "00", "url": "https://example.invalid/client.jar"}},
                "libraries": [{"name": "com.example:lib:1.0"}]}"#,
        );
        write(
            &dirs.asset_index_file("1.12").unwrap(),
            br#"{"objects": {}}"#,
        );
        write(
            &dirs
                .library_file("com/example/lib/1.0/lib-1.0.jar")
                .unwrap(),
            b"jar",
        );
        write(&dirs.version_jar("1.12.2").unwrap(), b"client");

        // Vanilla through the same entry point.
        assert_eq!(
            install_loader(&ctx, "1.12.2", LoaderKind::Vanilla, "", offline())
                .await
                .unwrap(),
            "1.12.2"
        );

        // Fabric: only the profile and its libraries are needed.
        assert!(matches!(
            install_loader(&ctx, "1.12.2", LoaderKind::Fabric, "0.16.10", offline()).await,
            Err(Error::Offline)
        ));
        let fabric_id = loader_profile_id(LoaderKind::Fabric, "1.12.2", "0.16.10").unwrap();
        write(
            &dirs.version_json(&fabric_id).unwrap(),
            format!(
                r#"{{"id": "{fabric_id}", "inheritsFrom": "1.12.2",
                    "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
                    "libraries": [{{"name": "net.fabricmc:fabric-loader:0.16.10", "url": "https://maven.fabricmc.net/"}}]}}"#
            )
            .as_bytes(),
        );
        write(
            &dirs
                .library_file("net/fabricmc/fabric-loader/0.16.10/fabric-loader-0.16.10.jar")
                .unwrap(),
            b"loader",
        );
        assert_eq!(
            install_loader(&ctx, "1.12.2", LoaderKind::Fabric, "0.16.10", offline())
                .await
                .unwrap(),
            fabric_id
        );

        // Forge: the profile alone is not enough, the install marker must verify too.
        let forge_id = loader_profile_id(LoaderKind::Forge, "1.12.2", "14.23.5.2859").unwrap();
        let forge_path =
            "net/minecraftforge/forge/1.12.2-14.23.5.2859/forge-1.12.2-14.23.5.2859.jar";
        let forge_jar = dirs.library_file(forge_path).unwrap();
        write(&forge_jar, b"forge");
        let sha1 = pl_net::hash_bytes(b"forge", pl_net::HashAlgo::Sha1);
        write(
            &dirs.version_json(&forge_id).unwrap(),
            format!(
                r#"{{"id": "{forge_id}", "inheritsFrom": "1.12.2", "mainClass": "net.minecraft.launchwrapper.Launch",
                    "minecraftArguments": "--username ${{auth_player_name}} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
                    "libraries": [{{"name": "net.minecraftforge:forge:1.12.2-14.23.5.2859",
                        "downloads": {{"artifact": {{"path": "{forge_path}", "url": "", "sha1": "{sha1}", "size": 5}}}}}}]}}"#
            )
            .as_bytes(),
        );
        assert!(matches!(
            install_loader(&ctx, "1.12.2", LoaderKind::Forge, "14.23.5.2859", offline()).await,
            Err(Error::Offline)
        ));
        write(
            &dirs
                .version_dir(&forge_id)
                .unwrap()
                .join("projects-loader.json"),
            format!(
                r#"{{"format": 1, "kind": "forge", "minecraft": "1.12.2", "loader": "14.23.5.2859",
                    "files": [{{"path": "libraries/{forge_path}", "sha1": "{sha1}", "size": 5}}]}}"#
            )
            .as_bytes(),
        );
        assert_eq!(
            install_loader(
                &ctx,
                "1.12.2",
                LoaderKind::Forge,
                "1.12.2-14.23.5.2859",
                offline()
            )
            .await
            .unwrap(),
            forge_id
        );
        assert!(
            dirs.version_jar(&forge_id).unwrap().is_file(),
            "client jar linked"
        );

        std::fs::remove_file(&forge_jar).unwrap();
        assert!(matches!(
            install_loader(&ctx, "1.12.2", LoaderKind::Forge, "14.23.5.2859", offline()).await,
            Err(Error::Offline)
        ));
    }
}
