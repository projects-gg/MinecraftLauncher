//! Fabric and Quilt: the launcher profile comes ready-made from their meta servers
//! (`/versions/loader/<mc>/<loader>/profile/json`); its libraries are plain Maven coordinates with
//! a repository `url` (plus `sha1`/`size` on Fabric) that the core installer downloads.

use pl_net::{NetError, VerifyMode};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::install::{InstallOptions, McContext};
use crate::loaders::LoaderKind;
use crate::loaders::common::{
    Progress, check_cancel, existing_profile, finish, manifest_for, prepare_profile, write_profile,
};
use crate::loaders::listing::{FABRIC_META_URL, QUILT_META_URL, meta_url};

/// Profile JSON URL of `loader` for `mc`.
pub(super) fn profile_url(kind: LoaderKind, mc: &str, loader: &str) -> Result<String> {
    let base = match kind {
        LoaderKind::Fabric => FABRIC_META_URL,
        LoaderKind::Quilt => QUILT_META_URL,
        other => return Err(Error::Unsupported(format!("{other:?} has no meta profile"))),
    };
    meta_url(base, &["versions", "loader", mc, loader, "profile", "json"])
}

pub(super) async fn install(
    ctx: &McContext,
    kind: LoaderKind,
    mc: &str,
    loader: &str,
    id: &str,
    options: &InstallOptions,
) -> Result<String> {
    let progress = Progress::new(options);
    let installed = existing_profile(ctx, id, mc).await.is_some();
    // Profiles never change for a loader/Minecraft pair; only a full repair fetches it again.
    let reuse = installed && (options.offline || options.verify != VerifyMode::Full);
    if !reuse {
        if options.offline {
            tracing::warn!(%id, "loader profile missing in offline mode");
            return Err(Error::Offline);
        }
        check_cancel(options)?;
        // The profile is the loader's only own step; its libraries come with the core install.
        progress.loader(0, 1);
        let url = profile_url(kind, mc, loader)?;
        match ctx.http.get_bytes(&url).await {
            Ok(bytes) => {
                let value: Value = serde_json::from_slice(&bytes)
                    .map_err(|e| Error::json(format!("{} profile", kind.display_name()), e))?;
                let value =
                    prepare_profile(value, id, mc, &format!("{} profile", kind.display_name()))?;
                write_profile(ctx, id, &value).await?;
                progress.loader(1, 1);
                tracing::info!(%id, "loader profile written");
            }
            Err(NetError::Status {
                status: 400 | 404, ..
            }) => {
                return Err(Error::NotFound(format!(
                    "{} {loader} for Minecraft {mc}",
                    kind.display_name()
                )));
            }
            Err(error) if installed && error.is_network() => {
                tracing::warn!(%error, %id, "profile refresh failed, keeping the installed one");
            }
            Err(error) => return Err(error.into()),
        }
    }
    let manifest = manifest_for(ctx, options).await?;
    finish(ctx, id, manifest.as_ref(), options).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_urls() {
        assert_eq!(
            profile_url(LoaderKind::Fabric, "1.21.1", "0.16.10").unwrap(),
            "https://meta.fabricmc.net/v2/versions/loader/1.21.1/0.16.10/profile/json"
        );
        assert_eq!(
            profile_url(LoaderKind::Quilt, "26.3", "0.30.1").unwrap(),
            "https://meta.quiltmc.org/v3/versions/loader/26.3/0.30.1/profile/json"
        );
        assert!(profile_url(LoaderKind::Forge, "1.20.1", "47.4.10").is_err());
    }
}
