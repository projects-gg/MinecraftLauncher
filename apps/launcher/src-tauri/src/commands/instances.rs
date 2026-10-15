use std::path::{Path, PathBuf};

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
}

fn icon_key(icon: &InstanceIcon) -> String {
    match icon {
        InstanceIcon::Builtin { key } => key.clone(),
        InstanceIcon::File { .. } => "grass".to_owned(),
    }
}

pub fn view(state: &AppState, instance: &Instance) -> InstanceView {
    let dir = state.instances.instance_dir(&instance.id).ok();
    let ready = dir
        .as_deref()
        .and_then(read_marker)
        .is_some_and(|m| {
            m.mc_version == instance.mc_version
                && m.loader == instance.loader.kind
                && m.loader_version == instance.loader.version
        });
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
        state: if ready { "ready" } else { "needsInstall" }.to_owned(),
        auto_join: instance.auto_join.as_ref().map(|a| match a.port {
            Some(port) if port != 25565 => format!("{}:{port}", a.host),
            _ => a.host.clone(),
        }),
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
}

#[tauri::command]
#[specta::specta]
pub async fn instance_create(
    state: State<'_, AppState>,
    request: NewInstanceRequest,
) -> CmdResult<InstanceView> {
    let total = pl_mc::instance::system_memory_mb();
    let modded = request.loader != LoaderKind::Vanilla;
    let created = state
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
            memory: Some(pl_mc::instance::recommended_memory(total, modded)),
            managed_pack: None,
        })
        .await
        .map_err(CmdError::from)?;
    Ok(view(&state, &created))
}

#[tauri::command]
#[specta::specta]
pub async fn instance_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.instances.delete(&id).await.map_err(CmdError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn instance_duplicate(state: State<'_, AppState>, id: String) -> CmdResult<InstanceView> {
    let source = state.instances.get(&id).await.map_err(CmdError::from)?;
    let copy = state
        .instances
        .duplicate(&id, &format!("{} (kopya)", source.name))
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
    pub size_bytes: u64,
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

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSettingsView {
    /// "auto" | "custom"
    pub java_mode: String,
    pub java_path: Option<String>,
    pub min_memory_mb: u32,
    pub max_memory_mb: u32,
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
pub async fn instance_detail(state: State<'_, AppState>, id: String) -> CmdResult<InstanceDetailView> {
    let instance = state.instances.get(&id).await.map_err(CmdError::from)?;
    let game_dir = state.instances.game_dir(&id).map_err(CmdError::from)?;
    let pack_files = pack_owned_files(&game_dir);

    let mods = pl_mc::mods::list_mods(&game_dir.join("mods"), false)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| {
            let meta = m.metadata.as_ref();
            let base = m.file_name.trim_end_matches(".disabled").to_owned();
            ModView {
                name: meta
                    .and_then(|x| x.name.clone())
                    .unwrap_or_else(|| base.trim_end_matches(".jar").to_owned()),
                version: meta.and_then(|x| x.version.clone()).unwrap_or_default(),
                authors: meta.map(|x| x.authors.clone()).unwrap_or_default(),
                description: meta.and_then(|x| x.description.clone()).unwrap_or_default(),
                icon_url: meta.and_then(|x| x.icon_png.as_deref()).map(png_data_url),
                from_pack: pack_files.iter().any(|p| p.ends_with(&base)),
                enabled: m.enabled,
                size_bytes: m.size,
                file_name: m.file_name,
            }
        })
        .collect();

    let resource_packs = pl_mc::gamefiles::list_resource_packs(&game_dir)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| PackView {
            name: display_name(&p.file_name),
            description: p.description.clone().unwrap_or_default(),
            enabled: true,
            icon_url: p.icon_png.as_deref().map(png_data_url),
            size_bytes: p.size,
            file_name: p.file_name,
        })
        .collect();

    let shader_packs = pl_mc::gamefiles::list_shader_packs(&game_dir)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| PackView {
            name: display_name(&p.file_name),
            description: String::new(),
            enabled: true,
            icon_url: None,
            size_bytes: p.size,
            file_name: p.file_name,
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
        .map(|list| {
            list.servers
                .into_iter()
                .map(|s| ServerView { name: s.name, address: s.ip })
                .collect()
        })
        .unwrap_or_default();

    Ok(InstanceDetailView {
        summary: view(&state, &instance),
        settings: settings_view(&instance),
        mods,
        resource_packs,
        shader_packs,
        worlds,
        screenshots,
        servers,
        system_memory_mb: pl_mc::instance::system_memory_mb(),
    })
}

/// Files the official pack installed (so the UI can label them "Paketten").
fn pack_owned_files(game_dir: &Path) -> Vec<String> {
    let path = game_dir.join(".projects").join("pack-state.json");
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| v.get("files").and_then(|f| f.as_object()).map(|o| o.keys().cloned().collect()))
        .unwrap_or_default()
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
    state.instances.update(&instance).await.map_err(CmdError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn mod_set_enabled(
    state: State<'_, AppState>,
    instance_id: String,
    file_name: String,
    enabled: bool,
) -> CmdResult<()> {
    if file_name.contains(['/', '\\']) || file_name.contains("..") {
        return Err(CmdError::internal("invalid mod file name"));
    }
    let path = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?
        .join("mods")
        .join(&file_name);
    pl_mc::mods::set_enabled(&path, enabled)
        .await
        .map(|_| ())
        .map_err(CmdError::from)
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
}

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
    Ok(pl_mc::loaders::list_loader_versions(&state.mc, loader, &mc_version)
        .await
        .map_err(CmdError::from)?
        .into_iter()
        .map(|v| LoaderVersionView { version: v.version, stable: v.stable })
        .collect())
}
