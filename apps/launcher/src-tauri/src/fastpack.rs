//! "Hızlı kaynak paketi yükleme" (FASTPACK): Settings → Genel's switch over the official pack's
//! fast-pack items (quick-pack, and ResourcePackCached once Studio lists it), shown while the
//! signed config's `features.fastPacks` is on. The page owns the choice and pushes it at its start
//! and on every change; the data folder keeps a copy of it and of the last applied flag, so the
//! syncs before the page asks or the content is read (background preparation) follow them too
//! and never remove what the next sync would bring back. Which items install and when lives in
//! `pl_content` (`LauncherItems`, `fast_pack_taken`; tested there): this module wires the switch,
//! the flag and the instance's own mods to it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use pl_content::{FAST_PACK_ITEM, FastPackTaken, LauncherItems, PackState, fast_pack_taken};
use pl_mc::mods::scan::ModScanner;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::modcenter::{PackSource, pack_source};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// The choice, in the data folder.
const CONFIG_FILE: &str = "fast-packs.json";

/// The switch (on by default), for the code that plans pack syncs far from the page.
static ENABLED: AtomicBool = AtomicBool::new(true);
/// The applied content's `features.fastPacks` (off until Studio turns it on).
static FLAG: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Kept {
    /// The switch.
    enabled: bool,
    /// The flag of the content applied last.
    flag: bool,
}

impl Default for Kept {
    fn default() -> Self {
        Self {
            enabled: true,
            flag: false,
        }
    }
}

/// Reads the kept choice and flag once at the start (before any sync can run).
pub fn load(root: &Path) {
    let kept: Kept = std::fs::read(root.join(CONFIG_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    ENABLED.store(kept.enabled, Ordering::Relaxed);
    FLAG.store(kept.flag, Ordering::Relaxed);
}

/// The applied content snapshot's flag (`commands::content::apply_snapshot`), kept when it changed.
pub fn apply_flag(root: &Path, on: bool) {
    if FLAG.swap(on, Ordering::Relaxed) != on {
        save(root);
    }
}

/// The launcher settings that own pack items for a sync of `game_dir`: the overlay's, this switch
/// (while the flag is on too) and the fast-pack items the folder's own mods already cover.
pub async fn pack_items(game_dir: &Path, previous: Option<&PackState>) -> LauncherItems {
    let mut items = crate::overlay::pack_items();
    items.fast_packs = ENABLED.load(Ordering::Relaxed) && FLAG.load(Ordering::Relaxed);
    if items.fast_packs {
        items.fast_packs_taken = taken(game_dir.join("mods"), previous).await;
    }
    items
}

/// The enabled jars of `mods_dir` with the mod ids each declares or provides, read through the
/// shared scanner (a warm scan costs a listing and a `stat` per jar).
async fn taken(mods_dir: PathBuf, previous: Option<&PackState>) -> FastPackTaken {
    let jars = tokio::task::spawn_blocking(move || {
        let snapshot = ModScanner::shared().scan_dir(&mods_dir);
        snapshot
            .enabled()
            .map(|jar| {
                let ids = jar
                    .contents
                    .all_mods()
                    .flat_map(|declared| {
                        std::iter::once(declared.id.clone()).chain(declared.provides.clone())
                    })
                    .collect::<Vec<_>>();
                (format!("mods/{}", jar.file_name), ids)
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "mods folder not read for the fast packs");
        Vec::new()
    });
    fast_pack_taken(&jars, previous)
}

fn save(root: &Path) {
    let kept = Kept {
        enabled: ENABLED.load(Ordering::Relaxed),
        flag: FLAG.load(Ordering::Relaxed),
    };
    let saved = serde_json::to_vec(&kept)
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            pl_mc::paths::write_atomic_blocking(&root.join(CONFIG_FILE), &bytes)
                .map_err(|error| error.to_string())
        });
    if let Err(error) = saved {
        tracing::warn!(%error, "fast pack setting not saved");
    }
}

/// The page's choice: called at its start and on every change. The next sync of an official pack
/// install (its next start, the mod center, background preparation) installs or removes the mods.
#[tauri::command]
#[specta::specta]
pub fn fast_packs_configure(state: State<'_, AppState>, enabled: bool) {
    if ENABLED.swap(enabled, Ordering::Relaxed) != enabled {
        save(&state.root);
    }
}

/// What Settings → Genel "Hızlı kaynak paketi yükleme" shows for the install Play opens.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FastPacksView {
    /// The signed config's `features.fastPacks`: the switch shows.
    pub shown: bool,
    /// The install follows the official pack (the switch works there only).
    pub official: bool,
    /// Its pack lists quick-pack for the install's Minecraft version; true while that cannot be
    /// read (offline without a cache), so no line claims what is not known.
    pub available: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn fast_packs_state(
    state: State<'_, AppState>,
    instance_id: Option<String>,
) -> CmdResult<FastPacksView> {
    let mut view = FastPacksView {
        shown: FLAG.load(Ordering::Relaxed),
        official: true,
        available: true,
    };
    let Some(instance_id) = instance_id.filter(|_| view.shown) else {
        return Ok(view);
    };
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    match pack_source(&state, &instance, &game_dir).await {
        PackSource::NotManaged => view.official = false,
        PackSource::Ready { manifest, .. } => {
            view.available = manifest.projects.item(FAST_PACK_ITEM).is_some();
        }
        PackSource::Unavailable | PackSource::Dropped(_) => {}
    }
    Ok(view)
}
