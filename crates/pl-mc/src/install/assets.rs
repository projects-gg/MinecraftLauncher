//! Assets: the index JSON, the hashed objects and the pre-1.7 layouts (`virtual: true` copies into
//! `assets/virtual/<index>/`, `map_to_resources: true` copies into `<game dir>/resources/`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pl_net::{Checksum, DownloadItem};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::{DataDirs, safe_join};
use crate::version::ResolvedVersion;

pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndex {
    #[serde(default)]
    pub objects: BTreeMap<String, AssetObject>,
    /// Pre-1.7.3 ("legacy"): the game reads files by name from `assets/virtual/<index>/`.
    #[serde(rename = "virtual", default)]
    pub is_virtual: bool,
    /// Pre-1.6 ("pre-1.6"): files are expected in `<game dir>/resources/`.
    #[serde(default)]
    pub map_to_resources: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// How the game expects its assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AssetLayout {
    /// `assets/indexes` + `assets/objects` (1.7.3+).
    Standard,
    /// Named copies in `assets/virtual/<index>/`.
    Virtual,
    /// Named copies in `<game dir>/resources/`.
    Resources,
}

/// Asset locations for the launch command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetsInfo {
    /// `${assets_index_name}`.
    pub index_id: String,
    pub layout: AssetLayout,
    /// `${assets_root}`.
    pub root: PathBuf,
    /// Folder holding named copies for [`AssetLayout::Virtual`]; for
    /// [`AssetLayout::Resources`] the launcher uses `<game dir>/resources`.
    pub virtual_dir: Option<PathBuf>,
}

impl AssetsInfo {
    /// `${game_assets}` for a given game directory.
    pub fn game_assets(&self, game_dir: &Path) -> PathBuf {
        match self.layout {
            AssetLayout::Standard => self.root.clone(),
            AssetLayout::Virtual => self
                .virtual_dir
                .clone()
                .unwrap_or_else(|| self.root.clone()),
            AssetLayout::Resources => game_dir.join("resources"),
        }
    }
}

/// Download item for the asset index JSON (and where it lives), if the version has one.
pub fn index_download(
    dirs: &DataDirs,
    resolved: &ResolvedVersion,
) -> Result<(PathBuf, Option<DownloadItem>)> {
    let id = resolved.assets_id();
    let path = dirs.asset_index_file(id)?;
    let item = resolved.asset_index.as_ref().and_then(|info| {
        let url = info.url.as_deref().filter(|u| !u.trim().is_empty())?;
        Some(
            DownloadItem::new(url, &path)
                .checksum(info.sha1.as_deref().map(Checksum::sha1))
                .size(info.size),
        )
    });
    Ok((path, item))
}

/// Parse an asset index file; a missing file yields an empty index (versions without assets).
pub async fn read_index(path: &Path) -> Result<AssetIndex> {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| Error::json(path.display().to_string(), e))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AssetIndex::default()),
        Err(e) => Err(Error::io(path, e)),
    }
}

/// One download per distinct object, from `resources.download.minecraft.net/<hh>/<hash>`.
pub fn object_downloads(dirs: &DataDirs, index: &AssetIndex) -> Result<Vec<DownloadItem>> {
    let mut items = Vec::with_capacity(index.objects.len());
    let mut seen = std::collections::HashSet::new();
    for object in index.objects.values() {
        let hash = object.hash.to_ascii_lowercase();
        if !seen.insert(hash.clone()) {
            continue;
        }
        let path = dirs.asset_object(&hash)?;
        let url = format!("{RESOURCES_URL}/{}/{hash}", &hash[..2]);
        items.push(
            DownloadItem::new(url, path)
                .sha1(&hash)
                .size(Some(object.size)),
        );
    }
    Ok(items)
}

/// Layout and paths for the launch command.
pub fn assets_info(dirs: &DataDirs, index_id: &str, index: &AssetIndex) -> Result<AssetsInfo> {
    let layout = if index.map_to_resources {
        AssetLayout::Resources
    } else if index.is_virtual {
        AssetLayout::Virtual
    } else {
        AssetLayout::Standard
    };
    let virtual_dir = match layout {
        AssetLayout::Virtual => Some(dirs.asset_virtual_dir(index_id)?),
        _ => None,
    };
    Ok(AssetsInfo {
        index_id: index_id.to_owned(),
        layout,
        root: dirs.assets(),
        virtual_dir,
    })
}

/// Create the named copies the old layouts need: hard links into `assets/virtual/<index>/`
/// (falling back to copies) and real copies into `<game dir>/resources/`.
pub async fn materialize_layout(
    dirs: &DataDirs,
    info: &AssetsInfo,
    index: &AssetIndex,
    game_dir: Option<&Path>,
) -> Result<()> {
    let (target, hard_link) = match info.layout {
        AssetLayout::Standard => return Ok(()),
        AssetLayout::Virtual => match &info.virtual_dir {
            Some(dir) => (dir.clone(), true),
            None => return Ok(()),
        },
        AssetLayout::Resources => match game_dir {
            Some(game) => (game.join("resources"), false),
            None => {
                tracing::warn!(index = %info.index_id, "no game directory given; skipping resources copy");
                return Ok(());
            }
        },
    };
    let mut pairs = Vec::with_capacity(index.objects.len());
    for (name, object) in &index.objects {
        let source = dirs.asset_object(&object.hash.to_ascii_lowercase())?;
        let dest = safe_join(&target, name)?;
        pairs.push((source, dest, object.size));
    }
    let target_for_error = target.clone();
    tokio::task::spawn_blocking(move || {
        for (source, dest, size) in pairs {
            place_file(&source, &dest, size, hard_link)?;
        }
        Ok(())
    })
    .await
    .map_err(|e| Error::io(target_for_error, std::io::Error::other(e.to_string())))?
}

/// Put `source` at `dest` unless a file of the right size is already there.
fn place_file(source: &Path, dest: &Path, size: u64, hard_link: bool) -> Result<()> {
    if std::fs::metadata(dest).is_ok_and(|m| m.is_file() && m.len() == size) {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    if dest.exists() {
        std::fs::remove_file(dest).map_err(|e| Error::io(dest, e))?;
    }
    if hard_link && std::fs::hard_link(source, dest).is_ok() {
        return Ok(());
    }
    std::fs::copy(source, dest).map_err(|e| Error::io(dest, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_legacy_flags() {
        let legacy: AssetIndex = serde_json::from_str(
            r#"{"virtual": true, "objects": {"sounds/a.ogg": {"hash": "AB12", "size": 3}}}"#,
        )
        .unwrap();
        assert!(legacy.is_virtual);
        let pre16: AssetIndex =
            serde_json::from_str(r#"{"map_to_resources": true, "objects": {}}"#).unwrap();
        assert!(pre16.map_to_resources);
        let dirs = DataDirs::new("/data");
        let items = object_downloads(&dirs, &legacy).unwrap();
        assert_eq!(
            items[0].urls[0],
            "https://resources.download.minecraft.net/ab/ab12"
        );
        let info = assets_info(&dirs, "legacy", &legacy).unwrap();
        assert_eq!(info.layout, AssetLayout::Virtual);
        assert_eq!(
            info.game_assets(Path::new("/game")),
            Path::new("/data/assets/virtual/legacy")
        );
        let info = assets_info(&dirs, "pre-1.6", &pre16).unwrap();
        assert_eq!(
            info.game_assets(Path::new("/game")),
            Path::new("/game/resources")
        );
    }

    #[tokio::test]
    async fn materializes_virtual_and_resources() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = DataDirs::new(tmp.path());
        let hash = "a9993e364706816aba3e25717850c26c9cd0d89d";
        let object = dirs.asset_object(hash).unwrap();
        std::fs::create_dir_all(object.parent().unwrap()).unwrap();
        std::fs::write(&object, b"abc").unwrap();
        let index: AssetIndex = serde_json::from_str(&format!(
            r#"{{"virtual": true, "objects": {{"sound/step/grass1.ogg": {{"hash": "{hash}", "size": 3}}}}}}"#
        ))
        .unwrap();
        let info = assets_info(&dirs, "legacy", &index).unwrap();
        materialize_layout(&dirs, &info, &index, None)
            .await
            .unwrap();
        let named = dirs
            .asset_virtual_dir("legacy")
            .unwrap()
            .join("sound/step/grass1.ogg");
        assert_eq!(std::fs::read(&named).unwrap(), b"abc");

        let mut resources = index.clone();
        resources.is_virtual = false;
        resources.map_to_resources = true;
        let info = assets_info(&dirs, "pre-1.6", &resources).unwrap();
        let game = tmp.path().join("game");
        materialize_layout(&dirs, &info, &resources, Some(&game))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(game.join("resources/sound/step/grass1.ogg")).unwrap(),
            b"abc"
        );
        // Idempotent.
        materialize_layout(&dirs, &info, &resources, Some(&game))
            .await
            .unwrap();
    }
}
