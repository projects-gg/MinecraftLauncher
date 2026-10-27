//! Item pictures for the 7/24 Bulut inventory and AFK+'s item pickers (ITEMART). The atlas is drawn
//! by `pl_items` from the player's own Minecraft client jar and kept in `cache/items/`; nothing of
//! the game ships with the launcher. It is drawn only when a screen asks for it, once per jar.

use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use pl_mc::paths::DataDirs;
use serde::Serialize;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

/// An atlas on disk.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemAtlasFiles {
    /// The Minecraft version of the jar it was drawn from.
    pub minecraft: String,
    /// Local path of the atlas image; the UI loads it with `convertFileSrc` (`cache/` is allowed).
    pub image: String,
    /// Local path of the second image, 3D icons drawn at 48 px (`index.large`), when drawn.
    pub large: Option<String>,
    /// The atlas index (`pl_items::AtlasIndex`) as JSON text, read by `vanillaAtlas` in
    /// `@pl/ui/items`.
    pub index: String,
}

/// The large atlas's file name in a cache folder.
const LARGE_IMAGE: &str = "atlas-48.png";

/// One drawing at a time: a second screen asking meanwhile then finds the first one's result.
static DRAWING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The item atlas of `minecraft`'s client jar, else of the newest installed one; `None` while no
/// version is installed or no jar could be drawn (the sprites then show their stand-ins). Never
/// an error: a missing picture is not worth a toast.
#[tauri::command]
#[specta::specta]
pub async fn items_atlas(
    state: State<'_, AppState>,
    minecraft: Option<String>,
) -> CmdResult<Option<ItemAtlasFiles>> {
    let root = state.root.clone();
    let _drawing = DRAWING.lock().await;
    Ok(
        tokio::task::spawn_blocking(move || atlas_for(&root, minecraft.as_deref()))
            .await
            .ok()
            .flatten(),
    )
}

fn atlas_for(root: &Path, minecraft: Option<&str>) -> Option<ItemAtlasFiles> {
    let cache = root.join("cache").join("items");
    for jar in candidate_jars(root, minecraft) {
        match cached_or_drawn(&jar, &cache) {
            Ok(Some(files)) => return Some(files),
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(%error, jar = %jar.display(), "could not draw the item atlas");
            }
        }
    }
    None
}

/// The jars to try, best first: the asked version's own (its items as that player sees them),
/// unless it is older than item definitions (before 1.21.4: chests, heads, banners, shields and
/// beds would be left out), then the newest installed jar with definitions draws first; then the
/// rest, newest first.
fn candidate_jars(root: &Path, minecraft: Option<&str>) -> Vec<PathBuf> {
    let dirs = DataDirs::new(root);
    let installed = pl_auth::client_jars(&dirs.versions());
    let asked = minecraft
        .and_then(|version| dirs.version_jar(version).ok())
        .filter(|jar| jar.is_file());
    let mut jars: Vec<PathBuf> = Vec::new();
    match asked {
        Some(jar) if has_definitions(&jar) => jars.push(jar),
        asked => {
            jars.extend(installed.iter().find(|jar| has_definitions(jar)).cloned());
            jars.extend(asked);
        }
    }
    for jar in installed {
        if !jars.contains(&jar) {
            jars.push(jar);
        }
    }
    jars
}

/// Whether a client jar has item definitions (1.21.4 and later).
fn has_definitions(jar: &Path) -> bool {
    pl_items::JarSource::open(jar)
        .is_ok_and(|mut source| pl_items::has_item_definitions(&mut source))
}

/// The jar's atlas from the cache, drawn first when missing; `None` for a jar without items.
fn cached_or_drawn(jar: &Path, cache: &Path) -> io::Result<Option<ItemAtlasFiles>> {
    let version = jar
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let modified = std::fs::metadata(jar)?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|age| age.as_secs())
        .unwrap_or(0);
    let key = format!("{version}-r{}-{modified}", pl_items::ATLAS_REVISION);
    let dir = cache.join(&key);
    let files = |index: String| ItemAtlasFiles {
        minecraft: version.clone(),
        image: dir.join("atlas.png").to_string_lossy().into_owned(),
        large: dir
            .join(LARGE_IMAGE)
            .is_file()
            .then(|| dir.join(LARGE_IMAGE).to_string_lossy().into_owned()),
        index,
    };
    if dir.join("atlas.png").is_file()
        && let Ok(index) = std::fs::read_to_string(dir.join("index.json"))
    {
        return Ok(Some(files(index)));
    }
    let mut source = pl_items::JarSource::open(jar).map_err(io::Error::other)?;
    let options = pl_items::BuildOptions {
        minecraft: version.clone(),
        large_image_name: LARGE_IMAGE.into(),
        ..pl_items::BuildOptions::default()
    };
    let atlas = match pl_items::build_atlas(&mut source, &options) {
        Ok(atlas) => atlas,
        Err(pl_items::Error::NoItems) => return Ok(None),
        Err(error) => return Err(io::Error::other(error)),
    };
    let index = serde_json::to_string(&atlas.index)?;
    // Written beside it, then moved into place: a half-written atlas is never read.
    let staging = cache.join(format!(".{key}.tmp"));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;
    std::fs::write(staging.join("atlas.png"), &atlas.png)?;
    if let Some(png) = &atlas.large_png {
        std::fs::write(staging.join(LARGE_IMAGE), png)?;
    }
    std::fs::write(staging.join("index.json"), &index)?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&staging, &dir)?;
    forget_older(cache, &version, &key);
    tracing::info!(
        %version,
        icons = atlas.index.icons.len(),
        missing = atlas.missing.len(),
        "drew the item atlas"
    );
    Ok(Some(files(index)))
}

/// Removes the version's atlases of older jars or drawings (`<version>-r<revision>-<modified>`;
/// another version whose name starts the same, like `1.21-rc1` beside `1.21`, is kept).
fn forget_older(cache: &Path, version: &str, keep: &str) {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return;
    };
    let prefix = format!("{version}-r");
    let numbers = |rest: &str| {
        rest.split_once('-').is_some_and(|(revision, modified)| {
            [revision, modified]
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        })
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name != keep && name.strip_prefix(&prefix).is_some_and(numbers) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// One picture of the item art document, on this computer.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemArtLocalFile {
    /// The document's own path (`itemart/atlas-16.<sha256>.png`).
    pub path: String,
    /// Where it is on disk; the UI loads it with `convertFileSrc` (`cache/` is allowed).
    pub local: String,
}

/// IA-ART's signed `itemArt` document (the Projects server's own item looks and menu pictures,
/// `docs/content-format.md` "Item art") with its pictures downloaded and verified.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemArtOverlayFiles {
    /// The document as JSON text, as the content index publishes it.
    pub doc: String,
    pub files: Vec<ItemArtLocalFile>,
}

/// The content index's item art document, for the 7/24 Bulut inventory on the servers it names:
/// read from the signed tree (cached), its pictures fetched once and verified by SHA-256. `None`
/// without a document or while the tree cannot be read (items keep the game's pictures). Never an
/// error.
#[tauri::command]
#[specta::specta]
pub async fn items_overlay(state: State<'_, AppState>) -> CmdResult<Option<ItemArtOverlayFiles>> {
    let Ok(snapshot) = state.content.load_cached().await else {
        return Ok(None);
    };
    let art = match state.content.item_art(&snapshot.index).await {
        Ok(Some(art)) => art,
        Ok(None) => return Ok(None),
        Err(error) => {
            tracing::debug!(%error, "the item art document is not readable now");
            return Ok(None);
        }
    };
    let mut files = Vec::new();
    for file in art.files() {
        match state.content.item_art_file(file).await {
            Ok(local) => files.push(ItemArtLocalFile {
                path: file.path.clone(),
                local: local.to_string_lossy().into_owned(),
            }),
            Err(error) => {
                tracing::debug!(%error, path = %file.path, "an item art picture is missing")
            }
        }
    }
    let Ok(doc) = serde_json::to_string(&art) else {
        return Ok(None);
    };
    Ok(Some(ItemArtOverlayFiles { doc, files }))
}
