use std::path::{Path, PathBuf};

use pl_lang::pick;
use pl_mc::instance::{
    Hooks, Instance, InstanceIcon, InstanceLoader, MemorySettings, NewInstance, WindowSettings,
};
use pl_mc::java::JavaSetting;
use pl_mc::loaders::LoaderKind;
use pl_mc::version::manifest::VersionType;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// Marker written after a successful install so the UI can show "ready" without hashing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMarker {
    pub version_id: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
}

pub fn marker_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join(".launcher").join("installed.json")
}

pub fn read_marker(instance_dir: &Path) -> Option<InstalledMarker> {
    let bytes = std::fs::read(marker_path(instance_dir)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write_marker(instance_dir: &Path, marker: &InstalledMarker) -> std::io::Result<()> {
    let path = marker_path(instance_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(marker).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceView {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub managed: bool,
    pub last_played_at: Option<String>,
    pub play_time_secs: u64,
    /// "ready" | "needsInstall" | "needsUpdate"
    pub state: String,
    pub auto_join: Option<String>,
    /// Library group (`None`: no group).
    pub group: Option<String>,
    pub created_at: String,
}

fn icon_key(icon: &InstanceIcon) -> String {
    match icon {
        InstanceIcon::Builtin { key } => key.clone(),
        InstanceIcon::File { .. } => "grass".to_owned(),
    }
}

pub fn view(state: &AppState, instance: &Instance) -> InstanceView {
    let dir = state.instances.instance_dir(&instance.id).ok();
    // Never installed → "needsInstall"; installed for another game or loader version (the
    // version picker, a pack update) → "needsUpdate".
    let install_state = match dir.as_deref().and_then(read_marker) {
        None => "needsInstall",
        Some(m)
            if m.mc_version == instance.mc_version
                && m.loader == instance.loader.kind
                && m.loader_version == instance.loader.version =>
        {
            "ready"
        }
        Some(_) => "needsUpdate",
    };
    InstanceView {
        id: instance.id.clone(),
        name: instance.name.clone(),
        icon: icon_key(&instance.icon),
        mc_version: instance.mc_version.clone(),
        loader: instance.loader.kind,
        loader_version: instance.loader.version.clone(),
        managed: instance.managed_pack.is_some(),
        last_played_at: instance.last_played_at.map(|t| t.to_string()),
        play_time_secs: instance.play_time_secs,
        state: install_state.to_owned(),
        auto_join: instance.auto_join.as_ref().map(|a| match a.port {
            Some(port) if port != 25565 => format!("{}:{port}", a.host),
            _ => a.host.clone(),
        }),
        group: instance.group.clone(),
        created_at: instance.created_at.to_string(),
    }
}

/// Offers the Projects server once per instance (at creation, after a modpack's own files are in
/// place, or at the first launch of an older instance): an empty server list gets it, and the
/// instance remembers that it was offered, so a server the player removed stays removed. An
/// unreadable list is left alone (and looked at again next time).
pub(crate) async fn seed_default_server_once(state: &AppState, instance: &mut Instance) {
    if instance.default_server_seeded {
        return;
    }
    let Ok(game_dir) = state.instances.game_dir(&instance.id) else {
        return;
    };
    match pl_mc::instance::seed_default_server(&game_dir).await {
        Ok(outcome) => {
            if outcome == pl_mc::instance::ServerSeed::Added {
                tracing::info!(instance = %instance.id, "added the default server");
            }
            instance.default_server_seeded = true;
            if let Err(error) = state.instances.update(instance).await {
                tracing::warn!(%error, "could not record the default server");
            }
        }
        Err(error) => {
            tracing::warn!(%error, instance = %instance.id, "server list unreadable; default server not added");
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn instances_list(state: State<'_, AppState>) -> CmdResult<Vec<InstanceView>> {
    let list = state.instances.list().await.map_err(CmdError::from)?;
    Ok(list.iter().map(|i| view(&state, i)).collect())
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NewInstanceRequest {
    pub name: String,
    pub icon: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    /// Settings → Oyun defaults: maximum memory (`None` = recommended for this machine).
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
    /// Settings → Oyun defaults: extra JVM arguments.
    #[serde(default)]
    pub jvm_args: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn instance_create(
    state: State<'_, AppState>,
    request: NewInstanceRequest,
) -> CmdResult<InstanceView> {
    let created = create_instance(&state, request).await?;
    Ok(view(&state, &created))
}

/// Creates an instance ("Otomatik" memory unless Settings → Oyun has a default) with the
/// Projects server in its list. New instances from a ready-made set use it too.
pub(crate) async fn create_instance(
    state: &AppState,
    request: NewInstanceRequest,
) -> CmdResult<Instance> {
    let total = pl_mc::instance::system_memory_mb();
    let modded = request.loader != LoaderKind::Vanilla;
    // Without a default from Settings → Oyun the instance is "Otomatik".
    let memory = request.max_memory_mb.map(|max_mb| {
        let mut memory = pl_mc::instance::recommended_memory(total, modded);
        memory.max_mb = max_mb;
        memory.min_mb = memory.min_mb.min(max_mb);
        pl_mc::instance::clamp_memory(&memory, total)
    });
    let mut created = state
        .instances
        .create(NewInstance {
            name: request.name.trim().to_owned(),
            mc_version: request.mc_version,
            loader: InstanceLoader {
                kind: request.loader,
                version: if modded { request.loader_version } else { None },
            },
            icon: Some(InstanceIcon::Builtin { key: request.icon }),
            group: None,
            memory,
            managed_pack: None,
        })
        .await
        .map_err(CmdError::from)?;
    if let Some(args) = request.jvm_args.filter(|a| !a.trim().is_empty()) {
        created.jvm_args = args.trim().to_owned();
        state
            .instances
            .update(&created)
            .await
            .map_err(CmdError::from)?;
    }
    seed_default_server_once(state, &mut created).await;
    Ok(created)
}

#[tauri::command]
#[specta::specta]
pub async fn instance_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<String> {
    let busy = |detail: &str| {
        CmdError::new(
            "KURULUM-MESGUL",
            pick(
                "Kurulum şu an kullanılıyor. Oyun ya da indirme bitince tekrar dene.",
                "This instance is in use right now. Try again when the game or download is done.",
            ),
            detail.to_owned(),
        )
    };
    if state
        .running
        .lock()
        .as_ref()
        .is_some_and(|running| running.instance_id == id)
    {
        return Err(busy("the game of this instance is running"));
    }
    // The launcher's own background download of it stops first (LIB1): the player never started
    // it, so it is no reason to refuse.
    crate::prepare::yield_instance(&app, &id).await;
    // A launch being prepared, a mod change or an install holds the instance: never move the
    // folder under it.
    let lock = crate::commands::modrinth::instance_lock(&app, &id);
    let Ok(_guard) = lock.try_lock() else {
        return Err(busy("the instance is busy"));
    };
    // Every instance can go, the official one too; deleting the last official instance is
    // remembered, so nothing makes it again by itself (LIB1). Until it is recorded, nothing
    // creates the official instance.
    let official = crate::commands::official::begin_deletion(&state).await;
    let trash = crate::commands::library::launcher_trash(&state);
    let entry = state
        .instances
        .move_to_trash(&id, &trash)
        .await
        .map_err(|error| match error {
            pl_mc::Error::NotFound(_) => CmdError::from(error),
            other => CmdError::new(
                "KURULUM-SIL",
                pick("Kurulum çöp kutusuna taşınamadı. Klasörünü açık tutan bir program olabilir; kapatıp tekrar dene.", "Couldn't move the instance to the trash. A program may have its folder open; close it and try again."),
                other.to_string(),
            ),
        })?;
    tracing::info!(instance = %id, trash = %entry.id, "instance moved to the launcher trash");
    official.record(&state).await;
    crate::commands::library::purge_trash_later(trash);
    Ok(entry.id)
}

/// "Geri al" of a deleted instance (the toast, or Settings → Veri → Çöp kutusu): it comes back
/// from the launcher trash, under its old id when that is free and with a numbered name when
/// another instance has taken its name meanwhile.
#[tauri::command]
#[specta::specta]
pub async fn instance_restore(
    state: State<'_, AppState>,
    trash_id: String,
) -> CmdResult<InstanceView> {
    let trash = crate::commands::library::launcher_trash(&state);
    let restored = state
        .instances
        .restore_from_trash(&trash, &trash_id)
        .await
        .map_err(|error| {
            CmdError::new(
                "KURULUM-GERI",
                pick(
                    "Kurulum geri alınamadı. Tekrar dene.",
                    "Couldn't bring the instance back. Try again.",
                ),
                error.to_string(),
            )
        })?;
    tracing::info!(instance = %restored.id, "instance restored from the launcher trash");
    // The official instance put back is as before: its deletion is forgotten.
    crate::commands::official::instance_restored(&state).await;
    Ok(view(&state, &restored))
}

#[tauri::command]
#[specta::specta]
pub async fn instance_duplicate(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<InstanceView> {
    // A copy with big worlds takes a while: as a job, it finishes before a closed window quits
    // (shell::close_requested), and a data folder move or a cleanup waits for it.
    let _job =
        crate::commands::modrinth::start_job(&app, &format!("duplicate:{}", uuid::Uuid::new_v4()));
    let source = state.instances.get(&id).await.map_err(CmdError::from)?;
    // The copy's name, in the interface's language ("Vanilla (kopya)" / "Vanilla (copy)").
    let name = pick(
        format!("{} (kopya)", source.name),
        format!("{} (copy)", source.name),
    );
    let copy = state
        .instances
        .duplicate(&id, &name)
        .await
        .map_err(CmdError::from)?;
    Ok(view(&state, &copy))
}

#[tauri::command]
#[specta::specta]
pub async fn instance_open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    sub: Option<String>,
) -> CmdResult<()> {
    let mut dir = state.instances.game_dir(&id).map_err(CmdError::from)?;
    if let Some(sub) = sub.filter(|s| !s.contains("..") && !s.contains(':')) {
        dir = dir.join(sub);
    }
    std::fs::create_dir_all(&dir).map_err(CmdError::internal)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(CmdError::internal)
}

// ---------------------------------------------------------------------------------------------
// Instance detail (mods, packs, worlds, screenshots, servers, settings)

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModView {
    pub file_name: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub description: String,
    pub enabled: bool,
    /// PNG as a data URL, when the jar has an icon.
    pub icon_url: Option<String>,
    pub from_pack: bool,
    /// The official pack's recommended mod (mod center item) this file is; toggling it changes
    /// the mod center selection instead of renaming the file.
    pub item_id: Option<String>,
    pub size_bytes: u64,
    /// Modrinth project of the file, when known (local add-on index, no network).
    pub project_id: Option<String>,
    /// Cached Modrinth project icon (local path) for files without an icon of their own.
    pub icon_path: Option<String>,
    /// The ready-made set that installed the file (its badge).
    pub mod_set: Option<crate::commands::modsets::SetBadgeView>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackView {
    pub file_name: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub icon_url: Option<String>,
    pub size_bytes: u64,
    /// Modrinth project of the file, when known (local add-on index, no network).
    pub project_id: Option<String>,
    /// Cached Modrinth project icon (local path) for packs without an icon of their own.
    pub icon_path: Option<String>,
    /// The ready-made set that installed the pack (its badge).
    pub mod_set: Option<crate::commands::modsets::SetBadgeView>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldView {
    pub folder: String,
    pub name: String,
    pub game_mode: String,
    pub last_played_at: String,
    pub size_bytes: u64,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotView {
    pub file_name: String,
    /// Local path; the UI converts it with `convertFileSrc`.
    pub path: String,
    pub taken_at: String,
}

/// A server of the instance's in-game server list (hidden "Direct Connection" entries left out).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    pub name: String,
    pub address: String,
    /// The icon the game saved for it (a PNG data URL).
    pub icon_url: Option<String>,
}

/// The listed servers of a `servers.dat` as the UI shows them.
fn server_views(list: &pl_mc::server::ServerList) -> Vec<ServerView> {
    list.listed()
        .map(|s| ServerView {
            name: s.name.clone(),
            address: s.ip.clone(),
            icon_url: s
                .icon_png()
                .filter(|png| png.starts_with(b"\x89PNG\r\n\x1a\n"))
                .map(|png| png_data_url(&png)),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSettingsView {
    /// "auto" | "custom"
    pub java_mode: String,
    pub java_path: Option<String>,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
    /// "Otomatik": the memory follows the machine and the enabled mods (the values above are
    /// kept for "Özel").
    #[serde(default)]
    pub memory_auto: bool,
    pub jvm_args: String,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub pre_launch: String,
    pub post_exit: String,
    pub wrapper: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDetailView {
    #[serde(flatten)]
    pub summary: InstanceView,
    pub settings: InstanceSettingsView,
    pub mods: Vec<ModView>,
    pub resource_packs: Vec<PackView>,
    pub shader_packs: Vec<PackView>,
    pub worlds: Vec<WorldView>,
    pub screenshots: Vec<ScreenshotView>,
    pub servers: Vec<ServerView>,
    pub system_memory_mb: u64,
    /// The maximum memory "Otomatik" gives this instance now.
    pub auto_memory_mb: u32,
}

fn display_name(file_name: &str) -> String {
    file_name
        .trim_end_matches(".disabled")
        .trim_end_matches(".zip")
        .to_owned()
}

fn ms_to_iso(ms: Option<i64>) -> String {
    ms.and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok())
        .map(|t| t.to_string())
        .unwrap_or_default()
}

fn enum_str<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn png_data_url(bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn settings_view(i: &Instance) -> InstanceSettingsView {
    InstanceSettingsView {
        java_mode: match i.java {
            JavaSetting::Auto => "auto",
            JavaSetting::Custom { .. } => "custom",
        }
        .to_owned(),
        java_path: match &i.java {
            JavaSetting::Custom { path } => Some(path.to_string_lossy().into_owned()),
            JavaSetting::Auto => None,
        },
        min_memory_mb: i.memory.min_mb,
        max_memory_mb: i.memory.max_mb,
        memory_auto: i.memory_auto,
        jvm_args: i.jvm_args.clone(),
        width: i.window.width.unwrap_or(1280),
        height: i.window.height.unwrap_or(720),
        fullscreen: i.window.fullscreen,
        pre_launch: i.hooks.pre_launch.clone().unwrap_or_default(),
        post_exit: i.hooks.post_exit.clone().unwrap_or_default(),
        wrapper: i.hooks.wrapper.clone().unwrap_or_default(),
        notes: i.notes.clone(),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn instance_detail(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<InstanceDetailView> {
    let mut instance = state.instances.get(&id).await.map_err(CmdError::from)?;
    // The page still opens when an interrupted switch cannot be finished yet (a launch, the
    // mod center or another switch try again and report it).
    if let Err(error) =
        crate::commands::versions::ensure_switch_finished(&state, &mut instance).await
    {
        tracing::warn!(?error, "an interrupted version switch is not finished yet");
    }
    let game_dir = state.instances.game_dir(&id).map_err(CmdError::from)?;
    // Files the official pack installed ("Paketten") and the mod center items among them.
    let pack_files: std::collections::HashMap<String, Option<String>> =
        pl_content::PackState::load(&game_dir)
            .await
            .ok()
            .flatten()
            .map(|state| {
                state
                    .files
                    .into_iter()
                    .map(|(path, record)| (path.to_ascii_lowercase(), record.item))
                    .collect()
            })
            .unwrap_or_default();
    // Titles and icons of files installed from Modrinth (mods & modpacks feature).
    let addon_index = match state.instances.instance_dir(&id) {
        Ok(dir) => pl_mc::addons::AddonIndex::load(&super::modrinth::addon_index_path(&dir)).await,
        Err(_) => pl_mc::addons::AddonIndex::default(),
    };
    let icon_dir = super::modrinth::icon_dir(&state);
    let modrinth_icon = |source: Option<&pl_mc::addons::AddonSource>| {
        source
            .and_then(|s| pl_mc::addons::cached_icon(&icon_dir, &s.project_id))
            .map(|p| p.to_string_lossy().into_owned())
    };
    // Files a ready-made set installed: their badge and the set's title for them.
    let mod_sets = pl_content::ModSetStates::load(&game_dir)
        .await
        .unwrap_or_default();
    let from_set = |folder: &str, file_name: &str| {
        super::modsets::set_of_file(&mod_sets, folder, file_name).map(|(id, set)| {
            (
                super::modsets::SetBadgeView::of(id, set),
                super::modsets::set_item_title(set, folder, file_name),
            )
        })
    };

    let mods = pl_mc::mods::list_mods(&game_dir.join("mods"), false)
        .await
        .unwrap_or_default()
        .into_iter()
        // The in-game overlay's mod belongs to its switch in Settings, not to this list.
        .filter(|m| {
            let base = m.file_name.trim_end_matches(".disabled");
            pack_files
                .get(&format!("mods/{base}").to_ascii_lowercase())
                .and_then(|item| item.as_deref())
                .is_none_or(|item| !pl_content::LauncherItems::owns(item))
        })
        .map(|m| {
            let meta = m.metadata.as_ref();
            let base = m.file_name.trim_end_matches(".disabled").to_owned();
            let pack_file = pack_files.get(&format!("mods/{base}").to_ascii_lowercase());
            let source = addon_index.source_for("mods", &m.file_name, m.size, m.modified_ms);
            let (mod_set, set_title) = from_set("mods", &m.file_name).unzip();
            ModView {
                name: source
                    .and_then(|s| s.title.clone())
                    .or(set_title.flatten())
                    .or_else(|| meta.and_then(|x| x.name.clone()))
                    .unwrap_or_else(|| base.trim_end_matches(".jar").to_owned()),
                version: meta
                    .and_then(|x| x.version.clone())
                    .filter(|v| !v.is_empty() && !v.contains("${"))
                    .or_else(|| source.and_then(|s| s.version_number.clone()))
                    .unwrap_or_default(),
                project_id: source.map(|s| s.project_id.clone()),
                icon_path: if meta.is_some_and(|x| x.icon_png.is_some()) {
                    None
                } else {
                    modrinth_icon(source)
                },
                authors: meta.map(|x| x.authors.clone()).unwrap_or_default(),
                description: meta.and_then(|x| x.description.clone()).unwrap_or_default(),
                icon_url: meta.and_then(|x| x.icon_png.as_deref()).map(png_data_url),
                from_pack: pack_file.is_some(),
                item_id: pack_file.cloned().flatten(),
                enabled: m.enabled,
                size_bytes: m.size,
                file_name: m.file_name,
                mod_set,
            }
        })
        .collect();

    let resource_packs = pl_mc::gamefiles::list_resource_packs(&game_dir)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let source =
                addon_index.source_for("resourcepacks", &p.file_name, p.size, p.modified_ms);
            let (mod_set, set_title) = from_set("resourcepacks", &p.file_name).unzip();
            PackView {
                name: source
                    .and_then(|s| s.title.clone())
                    .or(set_title.flatten())
                    .unwrap_or_else(|| display_name(&p.file_name)),
                description: p.description.clone().unwrap_or_default(),
                enabled: true,
                icon_url: p.icon_png.as_deref().map(png_data_url),
                size_bytes: p.size,
                project_id: source.map(|s| s.project_id.clone()),
                icon_path: if p.icon_png.is_some() {
                    None
                } else {
                    modrinth_icon(source)
                },
                file_name: p.file_name,
                mod_set,
            }
        })
        .collect();

    let shader_packs = pl_mc::gamefiles::list_shader_packs(&game_dir)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let source = addon_index.source_for("shaderpacks", &p.file_name, p.size, p.modified_ms);
            let (mod_set, set_title) = from_set("shaderpacks", &p.file_name).unzip();
            PackView {
                name: source
                    .and_then(|s| s.title.clone())
                    .or(set_title.flatten())
                    .unwrap_or_else(|| display_name(&p.file_name)),
                description: source
                    .and_then(|s| s.version_number.clone())
                    .unwrap_or_default(),
                enabled: true,
                icon_url: None,
                size_bytes: p.size,
                project_id: source.map(|s| s.project_id.clone()),
                icon_path: modrinth_icon(source),
                file_name: p.file_name,
                mod_set,
            }
        })
        .collect();

    let worlds = pl_mc::gamefiles::list_worlds(&game_dir)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|w| WorldView {
            name: w.name.clone(),
            game_mode: if w.hardcore {
                "hardcore".to_owned()
            } else {
                enum_str(&w.game_mode)
            },
            last_played_at: ms_to_iso(w.last_played_ms),
            size_bytes: w.size,
            icon_url: w.icon_png.as_deref().map(png_data_url),
            folder: w.folder_name,
        })
        .collect();

    let screenshots = pl_mc::gamefiles::list_screenshots(&game_dir)
        .await
        .unwrap_or_default()
        .into_iter()
        .take(120)
        .map(|s| ScreenshotView {
            file_name: s.file_name.clone(),
            path: s.path.to_string_lossy().into_owned(),
            taken_at: ms_to_iso(s.modified_ms),
        })
        .collect();

    let servers = pl_mc::server::ServerList::read(&game_dir.join("servers.dat"))
        .await
        .map(|list| server_views(&list))
        .unwrap_or_default();

    let system_memory_mb = pl_mc::instance::system_memory_mb();
    let auto_memory_mb = instance
        .automatic_memory(
            system_memory_mb,
            pl_mc::mods::count_enabled_mods(&game_dir.join("mods")),
        )
        .max_mb;
    Ok(InstanceDetailView {
        summary: view(&state, &instance),
        settings: settings_view(&instance),
        mods,
        resource_packs,
        shader_packs,
        worlds,
        screenshots,
        servers,
        system_memory_mb,
        auto_memory_mb,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn instance_update_settings(
    state: State<'_, AppState>,
    id: String,
    settings: InstanceSettingsView,
) -> CmdResult<()> {
    let mut instance = state.instances.get(&id).await.map_err(CmdError::from)?;
    let non_empty = |s: String| {
        let t = s.trim().to_owned();
        (!t.is_empty()).then_some(t)
    };
    instance.java = match (settings.java_mode.as_str(), settings.java_path) {
        ("custom", Some(path)) if !path.trim().is_empty() => JavaSetting::Custom {
            path: PathBuf::from(path.trim()),
        },
        _ => JavaSetting::Auto,
    };
    instance.memory = pl_mc::instance::clamp_memory(
        &MemorySettings {
            min_mb: settings.min_memory_mb,
            max_mb: settings.max_memory_mb,
        },
        pl_mc::instance::system_memory_mb(),
    );
    instance.memory_auto = settings.memory_auto;
    instance.jvm_args = settings.jvm_args.trim().to_owned();
    instance.window = WindowSettings {
        width: Some(settings.width.max(640)),
        height: Some(settings.height.max(480)),
        fullscreen: settings.fullscreen,
    };
    instance.hooks = Hooks {
        pre_launch: non_empty(settings.pre_launch),
        post_exit: non_empty(settings.post_exit),
        wrapper: non_empty(settings.wrapper),
    };
    instance.notes = settings.notes;
    state
        .instances
        .update(&instance)
        .await
        .map_err(CmdError::from)
}

// ---------------------------------------------------------------------------------------------
// Graphics presets (instance page → Ayarlar → Grafik)

/// The graphics preset of an instance's `options.txt`: Akıcı, Dengeli or Güzel, "Özel" when the
/// file has values of its own, or the preset the first launch will write.
#[tauri::command]
#[specta::specta]
pub async fn instance_graphics(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<pl_mc::instance::GraphicsState> {
    let instance = state.instances.get(&id).await.map_err(CmdError::from)?;
    let game_dir = state.instances.game_dir(&id).map_err(CmdError::from)?;
    Ok(pl_mc::instance::graphics_state(&game_dir, &instance.mc_version).await)
}

/// Waits for the instance (a launch preparing its game files, first-launch options included) and
/// refuses while its game runs: the game keeps its options in memory and writes them back over
/// any change.
async fn graphics_free(
    app: &AppHandle,
    state: &AppState,
    id: &str,
) -> CmdResult<tokio::sync::OwnedMutexGuard<()>> {
    let running = || {
        state
            .running
            .lock()
            .as_ref()
            .is_some_and(|r| r.instance_id == id)
    };
    let refuse_running = || {
        CmdError::new(
            "OYUN-ACIK",
            pick(
                "Oyun açıkken grafik ayarı değiştirilemez. Önce oyunu kapat.",
                "You can't change the graphics setting while the game is open. Close the game first.",
            ),
            format!("the game of instance {id} is running"),
        )
    };
    if running() {
        return Err(refuse_running());
    }
    let guard = crate::commands::modrinth::instance_lock(app, id)
        .lock_owned()
        .await;
    if running() {
        return Err(refuse_running());
    }
    Ok(guard)
}

/// Applies the graphics preset the player picked: into `options.txt` (only the preset's keys
/// change), or kept for the first launch while there is no such file. Refused while the
/// instance's game runs.
#[tauri::command]
#[specta::specta]
pub async fn instance_set_graphics(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    preset: pl_mc::instance::GraphicsPreset,
) -> CmdResult<pl_mc::instance::GraphicsState> {
    let _guard = graphics_free(&app, &state, &id).await?;
    let instance = state.instances.get(&id).await.map_err(CmdError::from)?;
    let game_dir = state.instances.game_dir(&id).map_err(CmdError::from)?;
    pl_mc::instance::set_graphics_preset(&game_dir, &instance.mc_version, preset)
        .await
        .map_err(CmdError::from)
}

/// Settings → Grafik ve FPS: the pick applies at once like `instance_set_graphics`, after a copy
/// of `options.txt` for "Geri al" (`instance_undo_graphics`).
#[tauri::command]
#[specta::specta]
pub async fn instance_switch_graphics(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    preset: pl_mc::instance::GraphicsPreset,
) -> CmdResult<pl_mc::instance::GraphicsSwitch> {
    let _guard = graphics_free(&app, &state, &id).await?;
    let instance = state.instances.get(&id).await.map_err(CmdError::from)?;
    let game_dir = state.instances.game_dir(&id).map_err(CmdError::from)?;
    pl_mc::instance::switch_graphics_preset(&game_dir, &instance.mc_version, preset)
        .await
        .map_err(CmdError::from)
}

/// "Geri al" after `instance_switch_graphics`: the preset's options go back from that copy.
/// `None` when the copy is gone (nothing changed).
#[tauri::command]
#[specta::specta]
pub async fn instance_undo_graphics(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    backup: String,
) -> CmdResult<Option<pl_mc::instance::GraphicsState>> {
    let _guard = graphics_free(&app, &state, &id).await?;
    let instance = state.instances.get(&id).await.map_err(CmdError::from)?;
    let game_dir = state.instances.game_dir(&id).map_err(CmdError::from)?;
    pl_mc::instance::undo_graphics_switch(&game_dir, &instance.mc_version, &backup)
        .await
        .map_err(CmdError::from)
}

/// The shader pack Iris draws in an instance's game (Settings → Grafik ve FPS, SMALL7).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShaderView {
    /// The pack's name as the instance's shader tab shows it: its Modrinth title, else its file
    /// name without `.zip`.
    pub name: String,
    /// Iris draws it; off, it waits for the player to turn it back on.
    pub on: bool,
}

async fn shader_view(
    state: &AppState,
    id: &str,
    game_dir: &Path,
    shader: pl_mc::instance::ShaderState,
) -> ShaderView {
    let addon_index = match state.instances.instance_dir(id) {
        Ok(dir) => pl_mc::addons::AddonIndex::load(&super::modrinth::addon_index_path(&dir)).await,
        Err(_) => pl_mc::addons::AddonIndex::default(),
    };
    // The index knows a file by its size and time, as the shader tab reads them.
    let title = pl_mc::gamefiles::list_shader_packs(game_dir)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|p| p.file_name == shader.pack)
        .and_then(|p| {
            addon_index
                .source_for("shaderpacks", &p.file_name, p.size, p.modified_ms)
                .and_then(|s| s.title.clone())
        });
    ShaderView {
        name: title.unwrap_or_else(|| display_name(&shader.pack)),
        on: shader.on,
    }
}

/// Settings → Grafik ve FPS: the shader pack Iris draws in the instance's game, on or off. `None`
/// when there is none to draw (no Iris among its mods, no pack chosen, the pack gone).
#[tauri::command]
#[specta::specta]
pub async fn instance_shader(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Option<ShaderView>> {
    let game_dir = state.instances.game_dir(&id).map_err(CmdError::from)?;
    let Some(shader) = pl_mc::instance::shader_state(&game_dir)
        .await
        .map_err(CmdError::from)?
    else {
        return Ok(None);
    };
    Ok(Some(shader_view(&state, &id, &game_dir, shader).await))
}

/// Turns the instance's shader pack on or off for its next start: only Iris's `enableShaders`
/// changes, so "Geri al" is the opposite call. Refused while the instance's game runs (Iris keeps
/// its settings in memory and writes them back).
#[tauri::command]
#[specta::specta]
pub async fn instance_set_shader(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    on: bool,
) -> CmdResult<Option<ShaderView>> {
    let _guard = graphics_free(&app, &state, &id).await?;
    let game_dir = state.instances.game_dir(&id).map_err(CmdError::from)?;
    let Some(shader) = pl_mc::instance::set_shaders(&game_dir, on)
        .await
        .map_err(CmdError::from)?
    else {
        return Ok(None);
    };
    Ok(Some(shader_view(&state, &id, &game_dir, shader).await))
}

/// Switches a mod on or off. A recommended mod of the official pack changes the mod center
/// selection (its file is installed or removed by the pack sync); other mods are renamed
/// (`x.jar` ↔ `x.jar.disabled`).
#[tauri::command]
#[specta::specta]
pub async fn mod_set_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
    enabled: bool,
) -> CmdResult<()> {
    if file_name.contains(['/', '\\']) || file_name.contains("..") {
        return Err(CmdError::internal("invalid mod file name"));
    }
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    if let Some(item) = crate::commands::modcenter::item_for_mod_file(&game_dir, &file_name).await {
        // The in-game overlay's mod (the crash help's "Bu modu kapat"): its switch in Settings
        // follows; the next launch's sync removes or brings back the file.
        if pl_content::LauncherItems::owns(&item) {
            crate::overlay::switched_elsewhere(&app, enabled);
            return Ok(());
        }
        let lock = crate::commands::modrinth::instance_lock(&app, &instance_id);
        let _guard = lock.lock().await;
        let selection = pl_content::PackSelection {
            items: std::collections::BTreeMap::from([(item, enabled)]),
            ..pl_content::PackSelection::default()
        };
        crate::commands::modcenter::apply_selection(
            &state,
            &instance_id,
            selection,
            None,
            &pl_net::CancellationToken::new(),
        )
        .await?;
        return Ok(());
    }
    let path = game_dir.join("mods").join(&file_name);
    pl_mc::mods::set_enabled(&path, enabled)
        .await
        .map(|_| ())
        .map_err(CmdError::from)
}

// ---------------------------------------------------------------------------------------------
// The in-game server list (instance page → Sunucular)

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerEditView {
    /// The listed servers after the edit.
    pub servers: Vec<ServerView>,
    /// The server a removal took off, handed back for "Geri al".
    pub removed: Option<pl_mc::server::ServerListEntry>,
}

/// Adds, changes, removes or puts back one server of the instance's `servers.dat` (positions
/// count listed servers only; see `pl_mc::server::ServerEdit`). Refused while the instance's
/// game runs: the game keeps the list in memory and writes it back over any change.
#[tauri::command]
#[specta::specta]
pub async fn instance_servers_edit(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    edit: pl_mc::server::ServerEdit,
) -> CmdResult<ServerEditView> {
    use pl_mc::server::{ServerEditError, ServerList};
    let running = || {
        state
            .running
            .lock()
            .as_ref()
            .is_some_and(|r| r.instance_id == instance_id)
    };
    let refuse_running = || {
        CmdError::new(
            "OYUN-ACIK",
            pick(
                "Oyun açıkken sunucu listesi değiştirilemez. Önce oyunu kapat.",
                "You can't change the server list while the game is open. Close the game first.",
            ),
            format!("the game of instance {instance_id} is running"),
        )
    };
    if running() {
        return Err(refuse_running());
    }
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let lock = crate::commands::modrinth::instance_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    if running() {
        return Err(refuse_running());
    }
    let path = game_dir.join("servers.dat");
    let mut list = ServerList::read(&path).await.map_err(|e| {
        CmdError::new(
            "SUNUCU-OKU",
            pick(
                "Sunucu listesi okunamadı. Dosya bozulmuş olabilir.",
                "Couldn't read the server list. The file may be damaged.",
            ),
            e.to_string(),
        )
    })?;
    let removed = list.apply(edit).map_err(|e| match &e {
        ServerEditError::BadAddress(_) => CmdError::new(
            "SUNUCU-ADRES",
            pick(
                "Bu bir sunucu adresi gibi görünmüyor.",
                "That doesn't look like a server address.",
            ),
            e.to_string(),
        ),
        ServerEditError::Duplicate(_) => CmdError::new(
            "SUNUCU-VAR",
            pick(
                "Bu sunucu listede zaten var.",
                "This server is already on the list.",
            ),
            e.to_string(),
        ),
        ServerEditError::Changed { .. } => CmdError::new(
            "SUNUCU-DEGISTI",
            pick(
                "Sunucu listesi oyunda değişmiş. Liste yenilendi, tekrar dene.",
                "The server list changed in the game. The list is refreshed; try again.",
            ),
            e.to_string(),
        ),
    })?;
    list.write(&path).await.map_err(|e| {
        CmdError::new(
            "SUNUCU-YAZ",
            pick(
                "Sunucu listesi kaydedilemedi. Tekrar dene.",
                "Couldn't save the server list. Try again.",
            ),
            e.to_string(),
        )
    })?;
    Ok(ServerEditView {
        servers: server_views(&list),
        removed,
    })
}

// ---------------------------------------------------------------------------------------------
// Versions

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct McVersionView {
    pub id: String,
    /// "release" | "snapshot" | "oldBeta" | "oldAlpha"
    pub kind: String,
    pub release_time: String,
    /// The launcher's recommended version ("Önerilen", preselected for new instances; listed
    /// with the releases even when it is a snapshot).
    pub recommended: bool,
}

/// Mojang's version list, newest first, with the launcher's recommended version marked (a
/// recommendation Mojang does not list marks nothing).
#[tauri::command]
#[specta::specta]
pub async fn minecraft_versions(state: State<'_, AppState>) -> CmdResult<Vec<McVersionView>> {
    let manifest = match pl_mc::version::manifest::load_manifest(&state.mc, false).await {
        Ok(m) => m,
        Err(e) if e.is_offline() => pl_mc::version::manifest::load_manifest(&state.mc, true)
            .await
            .map_err(CmdError::from)?,
        Err(e) => return Err(e.into()),
    };
    let recommended = crate::commands::content::recommended_minecraft(&state).await;
    Ok(manifest
        .versions
        .iter()
        .map(|v| McVersionView {
            id: v.id.clone(),
            kind: match v.kind {
                VersionType::Release => "release",
                VersionType::Snapshot => "snapshot",
                VersionType::OldBeta => "oldBeta",
                VersionType::OldAlpha => "oldAlpha",
                VersionType::Other => "snapshot",
            }
            .to_owned(),
            release_time: v.release_time.clone(),
            recommended: v.id == recommended,
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersionView {
    pub version: String,
    pub stable: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn loader_versions(
    state: State<'_, AppState>,
    loader: LoaderKind,
    mc_version: String,
) -> CmdResult<Vec<LoaderVersionView>> {
    if loader == LoaderKind::Vanilla {
        return Ok(Vec::new());
    }
    Ok(
        pl_mc::loaders::list_loader_versions(&state.mc, loader, &mc_version)
            .await
            .map_err(CmdError::from)?
            .into_iter()
            .map(|v| LoaderVersionView {
                version: v.version,
                stable: v.stable,
            })
            .collect(),
    )
}
