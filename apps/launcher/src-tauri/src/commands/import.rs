//! Bringing a player over from another launcher (IMPORT1): the first-run step "Başka bir
//! launcher'dan geldiğini gördük" and Settings → Veri → "Başka launcher'dan aktar". Finding the
//! launchers, the sizes and the copies live in `pl_import`; this module finds the Projects
//! instance, lends the instance locks and runs the import as a content job (a data folder move
//! or a cleanup waits for it, `content_cancel` stops it).

use std::sync::Arc;

use pl_import::{
    ImportProgress, ImportReport, ImportRequest, ImportShared, ImportSource, Locations,
    OfficialTarget, RunContext, SourceMeasure,
};
use pl_lang::pick;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::commands::modrinth::{instance_lock, start_job};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// What the import leaves free on the disk: the games it sets up still download.
const RESERVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// The Projects instance's name (`commands::official`).
const PROJECTS_NAME: &str = "Projects";

/// Where the other launchers are looked for, the v1 Projects Launcher too (`commands::migrate`).
/// Debug builds take `PROJECTS_IMPORT_HOME` as a pretend home folder, so end-to-end runs never
/// read a real launcher's folders.
pub(crate) fn locations() -> Locations {
    #[cfg(debug_assertions)]
    if let Some(home) = std::env::var_os("PROJECTS_IMPORT_HOME").filter(|h| !h.is_empty()) {
        return Locations::under(std::path::Path::new(&home), pl_import::Os::current());
    }
    Locations::system()
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportScanView {
    /// Other launchers with something to bring, the `.minecraft`-style folders first.
    pub sources: Vec<ImportSource>,
    /// Free space on the data folder's disk.
    pub free_bytes: Option<u64>,
    /// What the import leaves free (the games still download).
    pub reserve_bytes: u64,
    /// The Projects instance already has game settings of its own: they are kept.
    pub projects_has_options: bool,
    /// The player deleted the Projects instance (LIB1): what goes into it is not ticked by
    /// default, and ticking it installs the instance again.
    pub projects_removed: bool,
}

/// The Projects instance without asking the network (the scan must stay quick).
async fn projects_instance(state: &AppState) -> Option<String> {
    let list = state.instances.list().await.ok()?;
    list.iter()
        .find(|i| i.managed_pack.is_some() && i.name.eq_ignore_ascii_case(PROJECTS_NAME))
        .or_else(|| {
            list.iter()
                .find(|i| i.name.eq_ignore_ascii_case(PROJECTS_NAME))
        })
        .map(|i| i.id.clone())
}

/// Other launchers' data on this computer (cheap: small files and folder listings).
#[tauri::command]
#[specta::specta]
pub async fn import_scan(state: State<'_, AppState>) -> CmdResult<ImportScanView> {
    let instances_root = state.instances.root().to_path_buf();
    let data_root = state.root.clone();
    let projects_game = projects_instance(&state)
        .await
        .and_then(|id| state.instances.game_dir(&id).ok());
    let projects_removed = projects_game.is_none()
        && pl_mc::instance::OfficialChoice::load(&state.root)
            .await
            .removed();
    tauri::async_runtime::spawn_blocking(move || {
        let imported = pl_import::imported_profiles(&instances_root);
        let sources = pl_import::scan(&locations(), &imported);
        tracing::info!(
            sources = ?sources.iter().map(|s| (s.id.as_str(), s.profiles.len())).collect::<Vec<_>>(),
            "other launchers found"
        );
        ImportScanView {
            sources,
            free_bytes: fs4::available_space(&data_root).ok(),
            reserve_bytes: RESERVE_BYTES,
            projects_has_options: projects_game.is_some_and(|dir| dir.join("options.txt").is_file()),
            projects_removed,
        }
    })
    .await
    .map_err(CmdError::internal)
}

/// Sizes and fitting mods of one source's rows (reads folders and mod jars).
#[tauri::command]
#[specta::specta]
pub async fn import_measure(
    state: State<'_, AppState>,
    source_id: String,
) -> CmdResult<SourceMeasure> {
    let instances_root = state.instances.root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let imported = pl_import::imported_profiles(&instances_root);
        pl_import::scan_source(&locations(), &source_id, &imported)
            .map(|source| pl_import::measure(&source))
            .ok_or_else(|| {
                CmdError::new(
                    "AKTAR-YOK",
                    pick(
                        "Bu launcher'ın verileri artık bulunamıyor.",
                        "This launcher's data can't be found any more.",
                    ),
                    format!("no import source {source_id}"),
                )
            })
    })
    .await
    .map_err(CmdError::internal)?
}

fn refused(code: &str, tr: &str, en: &str, detail: &str) -> CmdError {
    CmdError::new(code, pick(tr, en), detail)
}

/// Copies what the player picked. Each item reports how it went; one that fails does not stop
/// the others. `content_cancel(job_id)` stops it after the file being copied.
#[tauri::command]
#[specta::specta]
pub async fn import_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportRequest,
    job_id: String,
    on_progress: Channel<ImportProgress>,
) -> CmdResult<ImportReport> {
    // One import at a time (the first-run step and Settings share the job).
    static RUNNING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let Ok(_one) = RUNNING.try_lock() else {
        return Err(refused(
            "AKTAR-SURUYOR",
            "Aktarım zaten sürüyor.",
            "An import is already running.",
            "import running",
        ));
    };
    // A running game writes its settings and server list when it closes, over the copies.
    if state.running.lock().is_some() || state.launch_cancel.lock().is_some() {
        return Err(refused(
            "AKTAR-OYUN",
            "Oyun açıkken aktarılamaz. Oyunu kapatıp tekrar dene.",
            "Close the game first, then try again.",
            "a game is running or being prepared",
        ));
    }
    if crate::commands::data::move_running(&app) {
        return Err(refused(
            "AKTAR-TASIMA",
            "Veri klasörü taşınıyor. Bitince tekrar dene.",
            "The data folder is moving. Try again when it's done.",
            "data folder move running",
        ));
    }
    let job = start_job(&app, &job_id);

    // Shared data goes into the Projects instance (made now when it is not there yet). Once the
    // player deleted it, only an item that goes into it makes it again (LIB1: such items are not
    // ticked by default then, so the player asked); worlds alone go to their versions' instances.
    let into_projects = request
        .sources
        .iter()
        .any(|s| s.shared.iter().any(|kind| *kind != ImportShared::Worlds));
    let official = if request.sources.iter().any(|s| !s.shared.is_empty()) {
        match crate::commands::official::ensure_official(&state, into_projects).await {
            Ok(Some(instance)) => Some(OfficialTarget {
                official_pack: instance.managed_pack.is_some(),
                id: instance.id,
                name: instance.name,
                minecraft: instance.mc_version,
            }),
            Ok(None) => {
                tracing::info!(
                    "the player deleted the Projects instance; worlds go to their versions' instances"
                );
                None
            }
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "Projects instance unavailable; shared data stays behind"
                );
                None
            }
        }
    } else {
        None
    };

    let instances_root = state.instances.root().to_path_buf();
    let sources = tauri::async_runtime::spawn_blocking(move || {
        let imported = pl_import::imported_profiles(&instances_root);
        pl_import::scan(&locations(), &imported)
    })
    .await
    .map_err(CmdError::internal)?;

    let locker: pl_import::InstanceLocker = {
        let app = app.clone();
        Arc::new(move |id: &str| instance_lock(&app, id))
    };
    let progress: pl_import::ProgressFn = Arc::new(move |p| {
        let _ = on_progress.send(p);
    });
    let report = pl_import::run_import(
        sources,
        request,
        RunContext {
            store: state.instances.clone(),
            official,
            locker,
            cancel: job.token.clone(),
            progress,
            reserve_bytes: RESERVE_BYTES,
        },
    )
    .await;
    tracing::info!(
        outcomes = report.outcomes.len(),
        created = report.created.len(),
        cancelled = report.cancelled,
        "import finished"
    );
    Ok(report)
}
