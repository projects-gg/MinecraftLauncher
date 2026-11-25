//! REPAIR-RB: "Önceki Sürüme Dön" (`pl_mc::mods::rollback`), the launcher side, and the file the
//! crash dialog opens.
//!
//! - `features.modRollback` ([`apply_flag`], [`on`]): off, nothing is recorded or offered; versions
//!   held earlier stay held and can still be undone.
//! - [`game_ended_clean`]: a game that ended without a crash leaves its mods in the instance's mod
//!   history: in the background after the game, nothing on the launch path.
//! - [`crash_seen`]: a fresh crash with a held mod before any clean game marks its hold as one
//!   that did not help (a crash dialog built again later never counts twice).
//! - [`crash_offer`]: what the crash dialog offers (`modhealth::crash_view`): the version to go
//!   back to, looked up on Modrinth for a few seconds at most, or why there is none.
//! - [`mods_rollback`], [`mods_rollback_undo`]: going back, and "Geri Al". A player's mod swaps its
//!   jar (the crashing one to the launcher trash); an official pack mod becomes the player's
//!   version of the pack item (PACK-EDIT's override, reason `rollback`), with or without
//!   `features.packEdit`.
//! - [`holds_for`]: the holds of the active set, kept in line with the mods folder and the pack's
//!   overrides: repairs, update prompts and pack upgrades leave the held jars alone until the
//!   player updates the mod or goes back to the pack's version (the way out).
//! - [`crash_open_file`]: the crash report, the JVM's fatal error log or the game's own log of the
//!   crashed run (`pl_mc::launch::crash_file`), never the launcher's own log.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use parking_lot::Mutex;
use pl_content::{OverrideReason, PackSelection, PackState};
use pl_lang::pick;
use pl_mc::addons::{AddonIndex, index_key};
use pl_mc::gamefiles::LauncherTrash;
use pl_mc::instance::Instance;
use pl_mc::launch::crash::{CrashDiagnosis, attribute_crash};
use pl_mc::launch::crash_file::{CrashFileKind, crash_file};
use pl_mc::loaders::LoaderKind;
use pl_mc::modrinth::ModrinthResolveTarget;
use pl_mc::mods::check::{FileOrigin, FileOrigins};
use pl_mc::mods::rollback::{
    KnownFile, KnownFiles, ModHistory, ModHold, OwnRollbackJob, RollbackLookup, RollbackPlan,
    clean_game, failed_hold, file_key, history_path, repeated_hold, resolve_rollback,
    roll_back_own, rollback_candidate, sha1_of, undo_own,
};
use pl_mc::mods::scan::{ModSnapshot, strip_disabled};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::commands::modcenter::{PackSource, apply_selection, not_available, pack_source};
use crate::commands::modrinth::{
    addon_index_path, addons_for, content_error, instance_lock, modrinth_client, start_job,
};
use crate::commands::versions::{ensure_switch_finished, refuse_while_running};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

static FLAG: AtomicBool = AtomicBool::new(false);

/// The signed config's `features.modRollback`, from every applied content snapshot.
pub(crate) fn apply_flag(on: bool) {
    FLAG.store(on, Ordering::Relaxed);
}

/// Whether `features.modRollback` is on.
pub(crate) fn on() -> bool {
    FLAG.load(Ordering::Relaxed)
}

/// How long a crash dialog waits for Modrinth before it shows without the offer.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

/// One load-change-save of a mod history at a time (games ending, rollbacks, undos).
static HISTORY: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);

/// What "Önceki Sürüme Dön" installs, per instance: found for its last crash dialog.
static PLANS: LazyLock<Mutex<HashMap<String, Planned>>> = LazyLock::new(Default::default);

/// The file each instance's last crash dialog opens.
static CRASH_FILES: LazyLock<Mutex<HashMap<String, PathBuf>>> = LazyLock::new(Default::default);

#[derive(Debug, Clone)]
struct Planned {
    plan: RollbackPlan,
    /// The mod's catalog title.
    name: String,
    /// The official pack item, for a pack mod.
    pack_item: Option<String>,
    minecraft: String,
}

// --- views -------------------------------------------------------------------------------------

/// "Önceki Sürüme Dön" in the crash dialog.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RollbackOfferView {
    /// The mod, by its catalog title.
    pub name: String,
    /// The version that crashed and the one it goes back to, as players read them.
    pub from: String,
    pub to: String,
    /// The mod came after the last clean game (else it was updated since).
    pub added: bool,
}

/// Why the crash dialog offers no going back for a mod that changed since the last game that
/// worked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RollbackNoteKind {
    /// It went back already, and the game crashed with it again.
    Repeated,
    /// No older version of it runs on this Minecraft version and loader.
    NoOlder,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RollbackNoteView {
    pub kind: RollbackNoteKind,
    /// The mod, by its catalog title.
    pub name: String,
}

/// A mod went back.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RollbackDoneView {
    /// The mod, by its catalog title.
    pub name: String,
    /// The version in place now.
    pub version: String,
    /// What "Geri Al" names (`mods_rollback_undo`).
    pub mod_id: String,
}

/// What a crash dialog gets from here.
#[derive(Debug, Clone, Default)]
pub(crate) struct CrashOffer {
    pub offer: Option<RollbackOfferView>,
    pub note: Option<RollbackNoteView>,
    /// The jar "Bu modu kapat" acts on when the dialog has none (a mod only in the stack trace).
    pub turn_off: Option<(String, String)>,
}

// --- the history -------------------------------------------------------------------------------

/// What the add-on index and the pack state already know about the set's jars (no jar is read).
fn known_files(snapshot: &ModSnapshot, index: &AddonIndex, pack: Option<&PackState>) -> KnownFiles {
    snapshot
        .jars
        .iter()
        .map(|jar| {
            let record = index
                .files
                .get(&index_key("mods", &jar.file_name))
                .filter(|record| record.matches(jar.size, jar.modified_ms));
            let source = record.and_then(|record| record.source.as_ref());
            let pack_item = pack
                .and_then(|state| state.files.get(&format!("mods/{}", jar.base_name())))
                .and_then(|file| file.item.clone());
            (
                file_key(&jar.file_name),
                KnownFile {
                    sha1: record
                        .map(|record| record.sha1.to_ascii_lowercase())
                        .filter(|sha1| sha1.len() == 40),
                    project_id: source.map(|s| s.project_id.clone()),
                    version_id: source
                        .map(|s| s.version_id.clone())
                        .filter(|id| !id.is_empty()),
                    pack_item,
                },
            )
        })
        .collect()
}

/// The pack items the player went back on (their holds live in the pack state's overrides).
fn rollback_items(pack: Option<&PackState>) -> HashSet<String> {
    pack.map(|state| {
        state
            .overrides
            .iter()
            .filter(|(_, choice)| choice.reason == OverrideReason::Rollback)
            .map(|(item, _)| item.clone())
            .collect()
    })
    .unwrap_or_default()
}

/// A game of the instance ended without a crash: its mods become the instance's last clean game
/// for its Minecraft version (with `features.modRollback`; in the background).
pub(crate) fn game_ended_clean(app: &AppHandle, instance_id: &str) {
    if !on() {
        return;
    }
    let app = app.clone();
    let instance_id = instance_id.to_owned();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = record_clean_game(&state, &instance_id).await {
            tracing::debug!(%error, "the mods of a clean game were not recorded");
        }
    });
}

async fn record_clean_game(state: &AppState, instance_id: &str) -> pl_mc::Result<()> {
    let instance = state.instances.get(instance_id).await?;
    if instance.loader.kind == LoaderKind::Vanilla {
        return Ok(());
    }
    let game_dir = state.instances.game_dir(instance_id)?;
    let instance_dir = state.instances.instance_dir(instance_id)?;
    let snapshot = crate::commands::modhealth::scan(game_dir.join("mods")).await;
    let index = AddonIndex::load(&addon_index_path(&instance_dir)).await;
    let pack = PackState::load(&game_dir).await.ok().flatten();
    let known = known_files(&snapshot, &index, pack.as_ref());
    let loader = instance
        .loader
        .kind
        .modrinth_name()
        .unwrap_or_default()
        .to_owned();
    let path = history_path(&instance_dir);
    let _writing = HISTORY.lock().await;
    let mut history = ModHistory::load(&path).await;
    let previous = history.clean.get(&instance.mc_version).cloned();
    let at_ms = jiff::Timestamp::now().as_millisecond();
    let game = tokio::task::spawn_blocking(move || {
        clean_game(
            &snapshot,
            &loader,
            at_ms,
            &known,
            previous.as_ref(),
            sha1_of,
        )
    })
    .await
    .map_err(|error| pl_mc::Error::Invalid(format!("recording task failed: {error}")))?;
    let jars = game.mods.len();
    if history.record_clean(&instance.mc_version, game) {
        history.save(&path).await?;
        tracing::info!(instance = %instance_id, jars, "the mods of a clean game were recorded");
    }
    Ok(())
}

/// The holds of the active set (`minecraft`), without the ones its mods folder or the pack no
/// longer has (those are dropped from the history). Cheap without holds: one small file read.
pub(crate) async fn holds_for(
    instance_dir: &Path,
    minecraft: &str,
    snapshot: &ModSnapshot,
    pack: Option<&PackState>,
) -> Vec<ModHold> {
    let path = history_path(instance_dir);
    let history = ModHistory::load(&path).await;
    if history.holds_of(minecraft).is_empty() {
        return Vec::new();
    }
    let items = rollback_items(pack);
    if !history.clone().reconcile(minecraft, snapshot, &items) {
        return history.holds_of(minecraft).to_vec();
    }
    let _writing = HISTORY.lock().await;
    let mut current = ModHistory::load(&path).await;
    if current.reconcile(minecraft, snapshot, &items) {
        tracing::info!(%minecraft, "holds the player ended are forgotten");
        if let Err(error) = current.save(&path).await {
            tracing::warn!(%error, "the mod history was not saved");
        }
    }
    current.holds_of(minecraft).to_vec()
}

/// The active set's holds as the history has them (update prompts read them; no reconciling).
pub(crate) async fn recorded_holds(state: &AppState, instance: &Instance) -> Vec<ModHold> {
    let Ok(instance_dir) = state.instances.instance_dir(&instance.id) else {
        return Vec::new();
    };
    ModHistory::load(&history_path(&instance_dir))
        .await
        .holds_of(&instance.mc_version)
        .to_vec()
}

/// An update moved these jars of the player's: their holds end (also when a jar kept its file
/// name).
pub(crate) async fn release_updated(state: &AppState, instance: &Instance, files: &[String]) {
    let Ok(instance_dir) = state.instances.instance_dir(&instance.id) else {
        return;
    };
    let path = history_path(&instance_dir);
    let _writing = HISTORY.lock().await;
    let mut history = ModHistory::load(&path).await;
    if history.release_files(&instance.mc_version, files) {
        tracing::info!(instance = %instance.id, "holds ended by an update the player chose");
        if let Err(error) = history.save(&path).await {
            tracing::warn!(%error, "the mod history was not saved");
        }
    }
}

// --- crashes -----------------------------------------------------------------------------------

/// A game crashed (a fresh crash, not a dialog built again): when it crashed with a mod held
/// after an earlier crash before any game ended cleanly, going back did not help: the hold is
/// marked so (no longer held, not offered again) and a pack mod's override ends with it.
pub(crate) async fn crash_seen(
    app: &AppHandle,
    state: &AppState,
    instance: &Instance,
    diagnosis: &CrashDiagnosis,
    java: Option<u32>,
) {
    let Ok(instance_dir) = state.instances.instance_dir(&instance.id) else {
        return;
    };
    let path = history_path(&instance_dir);
    let minecraft = &instance.mc_version;
    if ModHistory::load(&path)
        .await
        .holds_of(minecraft)
        .iter()
        .all(|hold| hold.proven || hold.failed)
    {
        return;
    }
    let Ok(game_dir) = state.instances.game_dir(&instance.id) else {
        return;
    };
    let snapshot = crate::commands::modhealth::scan(game_dir.join("mods")).await;
    let mut attributed = diagnosis.clone();
    attribute_crash(
        &mut attributed,
        &snapshot,
        &crate::commands::modhealth::environment(instance, java),
    );
    let pack_mod = {
        let _writing = HISTORY.lock().await;
        let mut history = ModHistory::load(&path).await;
        let Some(hold) = repeated_hold(history.holds_of(minecraft), &attributed).cloned() else {
            return;
        };
        if !history.mark_failed(minecraft, &hold.mod_id) {
            return;
        }
        tracing::info!(instance = %instance.id, mod_id = %hold.mod_id, "the game crashed again with a mod that went back");
        if let Err(error) = history.save(&path).await {
            tracing::warn!(%error, "the mod history was not saved");
            return;
        }
        hold.pack_item.is_some()
    };
    // A pack mod's override goes with its hold: the pack's file comes back with the next sync
    // (which ends the override itself when the instance is busy now).
    if pack_mod {
        let lock = instance_lock(app, &instance.id);
        if let Ok(_guard) = lock.try_lock() {
            end_unheld_rollbacks(state, instance, &game_dir).await;
        }
    }
}

/// A pack mod's rollback override lives only while its hold does: one whose hold did not help,
/// or that the mod history no longer has, ends and the sync puts the pack's file back. Before
/// every pack sync; one small file read without such an override.
pub(crate) async fn end_unheld_rollbacks(state: &AppState, instance: &Instance, game_dir: &Path) {
    let Ok(Some(mut pack_state)) = PackState::load(game_dir).await else {
        return;
    };
    if !pack_state
        .overrides
        .values()
        .any(|choice| choice.reason == OverrideReason::Rollback)
    {
        return;
    }
    let Ok(instance_dir) = state.instances.instance_dir(&instance.id) else {
        return;
    };
    let held = ModHistory::load(&history_path(&instance_dir))
        .await
        .held_pack_items(&instance.mc_version);
    let ended = pl_content::end_rollbacks(&mut pack_state, &held);
    if ended.is_empty() {
        return;
    }
    tracing::info!(instance = %instance.id, ?ended, "rollback overrides without a hold ended");
    if let Err(error) = pack_state.save(game_dir).await {
        tracing::warn!(%error, "pack state not saved after ending rollback overrides");
    }
}

/// What the crash dialog needs from the mods folder the crashed game used.
pub(crate) struct OfferInputs<'a> {
    pub game_dir: &'a Path,
    pub instance_dir: &'a Path,
    pub snapshot: &'a ModSnapshot,
    pub origins: &'a FileOrigins,
    pub pack: Option<&'a PackState>,
    pub holds: &'a [ModHold],
    /// A file's catalog title.
    pub title: &'a (dyn Fn(&str) -> Option<String> + Sync),
}

/// SHA-1s of the jars the crash names (a few files), for comparing them with the last clean game.
async fn suspect_hashes(diagnosis: &CrashDiagnosis, snapshot: &ModSnapshot) -> KnownFiles {
    let paths: Vec<(String, PathBuf)> = diagnosis
        .suspects
        .iter()
        .filter_map(|s| snapshot.by_file(s.file.as_deref()?))
        .map(|jar| (file_key(&jar.file_name), jar.path.clone()))
        .collect();
    tokio::task::spawn_blocking(move || {
        let mut known = KnownFiles::new();
        for (key, path) in paths {
            if known.contains_key(&key) {
                continue;
            }
            known.insert(
                key,
                KnownFile {
                    sha1: sha1_of(&path),
                    ..KnownFile::default()
                },
            );
        }
        known
    })
    .await
    .unwrap_or_default()
}

/// Jars whose versions are another owner's: a ready-made set's (it has its own "Güncelle") and
/// the in-game overlay's (Settings).
async fn others_jars(game_dir: &Path, diagnosis: &CrashDiagnosis) -> HashSet<String> {
    let sets = pl_content::ModSetStates::load(game_dir)
        .await
        .unwrap_or_default();
    let overlay = crate::overlay::owned_files(game_dir);
    diagnosis
        .suspects
        .iter()
        .filter_map(|s| s.file.as_deref())
        .filter(|file| {
            crate::commands::modsets::set_of_file(&sets, "mods", file).is_some()
                || pl_overlay::install::is_owned(&overlay, file)
        })
        .map(file_key)
        .collect()
}

/// The going back a crash dialog offers (with `features.modRollback`): for the crash's suspect
/// that changed since the last clean game, the version to go back to, found on Modrinth within
/// [`LOOKUP_TIMEOUT`]; or why there is none: it went back already and crashed again, or nothing
/// older runs here (then "Bu modu kapat" gets the jar). Offline or slow: nothing.
pub(crate) async fn crash_offer(
    state: &AppState,
    instance: &Instance,
    diagnosis: &CrashDiagnosis,
    inputs: &OfferInputs<'_>,
) -> CrashOffer {
    PLANS.lock().remove(&instance.id);
    if !on() {
        return CrashOffer::default();
    }
    let name_of =
        |file: &str, fallback: &str| (inputs.title)(file).unwrap_or_else(|| fallback.to_owned());
    if let Some(hold) = failed_hold(inputs.holds, diagnosis) {
        let jar = inputs
            .snapshot
            .by_file(&hold.file)
            .filter(|jar| jar.enabled)
            .map(|jar| jar.file_name.clone());
        return CrashOffer {
            note: Some(RollbackNoteView {
                kind: RollbackNoteKind::Repeated,
                name: name_of(&hold.file, &hold.name),
            }),
            turn_off: jar.map(|file| (file, name_of(&hold.file, &hold.name))),
            ..CrashOffer::default()
        };
    }
    let history = ModHistory::load(&history_path(inputs.instance_dir)).await;
    let Some(baseline) = history.baseline(&instance.mc_version) else {
        return CrashOffer::default();
    };
    let known = suspect_hashes(diagnosis, inputs.snapshot).await;
    let skip = others_jars(inputs.game_dir, diagnosis).await;
    let Some(candidate) = rollback_candidate(diagnosis, inputs.snapshot, baseline, &known, &skip)
    else {
        return CrashOffer::default();
    };
    let name = name_of(&candidate.file, &candidate.name);
    let base = strip_disabled(&candidate.file);
    let pack_item = inputs
        .pack
        .and_then(|state| state.files.get(&format!("mods/{base}")))
        .and_then(|file| file.item.clone());
    let mut hint = match inputs.origins.get(base) {
        Some(FileOrigin::Modrinth { project_id, .. }) => Some(project_id.clone()),
        _ => None,
    };
    if hint.is_none()
        && let Some(item) = &pack_item
        && let PackSource::Ready { manifest, .. } =
            pack_source(state, instance, inputs.game_dir).await
    {
        hint = crate::commands::packedit::project_of(&manifest, item);
    }
    let target =
        ModrinthResolveTarget::for_loader(instance.loader.kind, instance.mc_version.clone());
    let client = modrinth_client(state);
    let lookup = tokio::time::timeout(
        LOOKUP_TIMEOUT,
        resolve_rollback(&client, &candidate, hint.as_deref(), &target),
    )
    .await;
    let turn_off = Some((candidate.file.clone(), name.clone()));
    match lookup {
        Ok(Ok(RollbackLookup::Found(plan))) => {
            let offer = RollbackOfferView {
                name: name.clone(),
                from: plan.from_version(),
                to: plan.target.version_number.clone(),
                added: plan.candidate.before.is_none(),
            };
            tracing::info!(instance = %instance.id, mod_id = %candidate.mod_id, to = %offer.to, "a crashed mod can go back");
            PLANS.lock().insert(
                instance.id.clone(),
                Planned {
                    plan: *plan,
                    name,
                    pack_item,
                    minecraft: instance.mc_version.clone(),
                },
            );
            CrashOffer {
                offer: Some(offer),
                ..CrashOffer::default()
            }
        }
        Ok(Ok(RollbackLookup::NoOlder)) => CrashOffer {
            note: Some(RollbackNoteView {
                kind: RollbackNoteKind::NoOlder,
                name,
            }),
            turn_off,
            ..CrashOffer::default()
        },
        Ok(Ok(RollbackLookup::Unknown)) => CrashOffer::default(),
        Ok(Err(error)) => {
            tracing::info!(%error, "no going back looked up for the crash");
            CrashOffer::default()
        }
        Err(_) => {
            tracing::info!("Modrinth did not answer in time for the crash dialog");
            CrashOffer::default()
        }
    }
}

/// The file that explains the crash (kept for [`crash_open_file`]), and what it is.
pub(crate) fn remember_crash_file(
    instance_id: &str,
    diagnosis: &CrashDiagnosis,
    game_dir: &Path,
) -> Option<CrashFileKind> {
    let started = SystemTime::now().checked_sub(Duration::from_secs(diagnosis.runtime_secs));
    let found = crash_file(diagnosis, game_dir, started);
    let mut files = CRASH_FILES.lock();
    match found {
        Some(file) => {
            files.insert(instance_id.to_owned(), file.path);
            Some(file.kind)
        }
        None => {
            files.remove(instance_id);
            None
        }
    }
}

fn file_gone() -> CmdError {
    CmdError::new(
        "COKME-KAYDI-YOK",
        pick(
            "Bu çökmenin kaydı artık yok.",
            "This crash's report is no longer there.",
        ),
        "the crash file is gone",
    )
}

/// Opens the file that explains the instance's last crash (its crash dialog's): the crash report,
/// the JVM's fatal error log or the game's log of the run, in the system's text viewer.
#[tauri::command]
#[specta::specta]
pub async fn crash_open_file(app: AppHandle, instance_id: String) -> CmdResult<()> {
    let path = CRASH_FILES
        .lock()
        .get(&instance_id)
        .cloned()
        .filter(|path| path.is_file())
        .ok_or_else(file_gone)?;
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| {
            CmdError::new(
                "COKME-KAYDI-AC",
                pick("Çökme kaydı açılamadı.", "Couldn't open the crash report."),
                e.to_string(),
            )
        })
}

// --- going back --------------------------------------------------------------------------------

fn not_offered() -> CmdError {
    CmdError::new(
        "ONCEKI-SURUM-YOK",
        pick(
            "Bu mod için önceki sürüm artık hazır değil. Pencereyi kapatıp yeniden dene.",
            "The earlier version isn't ready any more. Close this and try again.",
        ),
        "no rollback planned for the instance's last crash",
    )
}

fn nothing_to_undo() -> CmdError {
    CmdError::new(
        "GERI-AL-YOK",
        pick(
            "Geri alınacak bir şey kalmadı.",
            "There's nothing left to undo.",
        ),
        "no hold for the mod",
    )
}

/// Removes the rollback override this command wrote for `item` (a sync that failed).
async fn drop_rollback_override(game_dir: &Path, item: &str, version_id: &str) {
    let Ok(Some(mut pack_state)) = PackState::load(game_dir).await else {
        return;
    };
    let ours = pack_state.overrides.get(item).is_some_and(|choice| {
        choice.reason == OverrideReason::Rollback && choice.version_id == version_id
    });
    if ours {
        pack_state.overrides.remove(item);
        if let Err(error) = pack_state.save(game_dir).await {
            tracing::warn!(%error, "a rollback override was not removed after a failed sync");
        }
    }
}

/// Records `hold` for `minecraft` in the instance's mod history.
async fn save_hold(instance_dir: &Path, minecraft: &str, hold: ModHold) -> CmdResult<()> {
    let path = history_path(instance_dir);
    let _writing = HISTORY.lock().await;
    let mut history = ModHistory::load(&path).await;
    history.hold(minecraft, hold);
    history.save(&path).await.map_err(content_error)
}

/// "Önceki Sürüme Dön": puts back the version the instance's last crash dialog offered. A
/// player's mod swaps its jar (the crashing one to the launcher trash); a pack mod becomes the
/// player's version of the pack item, which the sync keeps. Refused while the instance's game
/// runs.
#[tauri::command]
#[specta::specta]
pub async fn mods_rollback(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    job_id: String,
) -> CmdResult<RollbackDoneView> {
    refuse_while_running(&state, &instance_id)?;
    let job = start_job(&app, &job_id);
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };
    // A launch that held the lock may have started the game meanwhile.
    refuse_while_running(&state, &instance_id)?;
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let planned = PLANS
        .lock()
        .get(&instance_id)
        .cloned()
        .filter(|planned| planned.minecraft == instance.mc_version)
        .ok_or_else(not_offered)?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let instance_dir = state
        .instances
        .instance_dir(&instance_id)
        .map_err(CmdError::from)?;
    let plan = &planned.plan;
    if !game_dir.join("mods").join(&plan.candidate.file).is_file() {
        return Err(not_offered());
    }
    let at_ms = jiff::Timestamp::now().as_millisecond();
    let hold = match &planned.pack_item {
        Some(item) => {
            let PackSource::Ready { manifest, .. } =
                pack_source(&state, &instance, &game_dir).await
            else {
                return Err(not_available());
            };
            let choice = crate::commands::packedit::rollback_override(
                &manifest,
                item,
                &plan.target,
                plan.candidate.sha1.as_deref(),
            )
            .ok_or_else(not_offered)?;
            let file = choice
                .path
                .strip_prefix("mods/")
                .unwrap_or(&choice.path)
                .to_owned();
            let mut pack_state = PackState::load(&game_dir)
                .await
                .map_err(crate::commands::official::pack_error)?
                .unwrap_or_default();
            let version_id = choice.version_id.clone();
            pack_state.overrides.insert(item.clone(), choice);
            pack_state
                .save(&game_dir)
                .await
                .map_err(crate::commands::official::pack_error)?;
            if let Err(error) = apply_selection(
                &state,
                &instance_id,
                PackSelection::default(),
                None,
                &job.token,
            )
            .await
            {
                // No half state: the override goes with the failed sync.
                drop_rollback_override(&game_dir, item, &version_id).await;
                return Err(error);
            }
            ModHold {
                mod_id: plan.candidate.mod_id.clone(),
                name: planned.name.clone(),
                file,
                pack_item: Some(item.clone()),
                version: plan.target.version_number.clone(),
                from_version: plan.from_version(),
                from_version_id: plan.current.as_ref().map(|v| v.id.clone()),
                from_published: plan.current.as_ref().map(|v| v.date_published.clone()),
                at_ms,
                ..ModHold::default()
            }
        }
        None => {
            let addons = addons_for(&state, &instance_id)?;
            let trash = LauncherTrash::new(state.root.join("trash"));
            let client = modrinth_client(&state);
            let own = OwnRollbackJob {
                addons: &addons,
                client: &client,
                downloader: &state.mc.downloader,
                trash: &trash,
                instance_id: &instance_id,
                target: ModrinthResolveTarget::for_loader(
                    instance.loader.kind,
                    instance.mc_version.clone(),
                ),
                cancel: job.token.clone(),
            };
            roll_back_own(&own, plan, &planned.name, at_ms)
                .await
                .map_err(content_error)?
        }
    };
    let done = RollbackDoneView {
        name: planned.name.clone(),
        version: hold.version.clone(),
        mod_id: hold.mod_id.clone(),
    };
    if let Err(error) = save_hold(&instance_dir, &instance.mc_version, hold.clone()).await {
        // No half state: without its hold the swap is undone.
        match &hold.pack_item {
            Some(item) => {
                let mut selection = PackSelection::default();
                selection.revert.insert(item.clone());
                if let Err(back) =
                    apply_selection(&state, &instance_id, selection, None, &job.token).await
                {
                    tracing::warn!(
                        ?back,
                        "a pack mod's rollback was not undone after a failed save"
                    );
                }
            }
            None => {
                let trash = LauncherTrash::new(state.root.join("trash"));
                if let Err(back) =
                    undo_own(&game_dir.join("mods"), &trash, &instance_id, &hold).await
                {
                    tracing::warn!(%back, "a rollback was not undone after a failed save");
                }
            }
        }
        return Err(error);
    }
    PLANS.lock().remove(&instance_id);
    tracing::info!(instance = %instance_id, mod_id = %done.mod_id, version = %done.version, pack = planned.pack_item.is_some(), "a crashed mod went back");
    Ok(done)
}

/// "Geri Al" after "Önceki Sürüme Dön": the version that crashed comes back (a player's mod from
/// the launcher trash, a pack mod from the pack) and the hold ends. Refused while the instance's
/// game runs.
#[tauri::command]
#[specta::specta]
pub async fn mods_rollback_undo(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    mod_id: String,
    job_id: String,
) -> CmdResult<()> {
    refuse_while_running(&state, &instance_id)?;
    let job = start_job(&app, &job_id);
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };
    refuse_while_running(&state, &instance_id)?;
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let instance_dir = state
        .instances
        .instance_dir(&instance_id)
        .map_err(CmdError::from)?;
    let path = history_path(&instance_dir);
    // Read without holding the history: the sync below runs checks that may tidy the holds.
    let hold = ModHistory::load(&path)
        .await
        .holds_of(&instance.mc_version)
        .iter()
        .find(|hold| hold.mod_id == mod_id)
        .cloned()
        .ok_or_else(nothing_to_undo)?;
    match &hold.pack_item {
        Some(item) => {
            let mut selection = PackSelection::default();
            selection.revert.insert(item.clone());
            apply_selection(&state, &instance_id, selection, None, &job.token).await?;
        }
        None => {
            let trash = LauncherTrash::new(state.root.join("trash"));
            undo_own(&game_dir.join("mods"), &trash, &instance_id, &hold)
                .await
                .map_err(|error| match error {
                    pl_mc::Error::NotFound(_) => nothing_to_undo(),
                    other => content_error(other),
                })?;
        }
    }
    {
        let _writing = HISTORY.lock().await;
        let mut history = ModHistory::load(&path).await;
        if history.release(&instance.mc_version, &mod_id).is_some() {
            history.save(&path).await.map_err(content_error)?;
        }
    }
    tracing::info!(instance = %instance_id, %mod_id, "going back was undone");
    Ok(())
}

/// Settings → Veri (or the instance's trash): a jar "Önceki Sürüme Dön" put in the launcher trash
/// comes back to the mods folder. While its hold lasts that is "Geri Al": the held jar goes to the
/// trash and the hold ends. Refused while the instance's game runs. Returns the jar's file name.
pub(crate) async fn restore_trashed_mod(
    app: &AppHandle,
    state: &AppState,
    instance_id: &str,
    trash_id: &str,
) -> CmdResult<String> {
    refuse_while_running(state, instance_id)?;
    let lock = instance_lock(app, instance_id);
    let _guard = lock.lock().await;
    refuse_while_running(state, instance_id)?;
    let mut instance = state
        .instances
        .get(instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(instance_id)
        .map_err(CmdError::from)?;
    let instance_dir = state
        .instances
        .instance_dir(instance_id)
        .map_err(CmdError::from)?;
    let history_file = history_path(&instance_dir);
    let holds = ModHistory::load(&history_file)
        .await
        .holds_of(&instance.mc_version)
        .to_vec();
    let trash = LauncherTrash::new(state.root.join("trash"));
    let back = pl_mc::mods::rollback::restore_trashed_mod(
        &game_dir.join("mods"),
        &trash,
        instance_id,
        &holds,
        trash_id,
    )
    .await
    .map_err(|error| {
        CmdError::new(
            "COP-GERI",
            pick(
                "Geri alınamadı. Tekrar dene.",
                "Couldn't bring it back. Try again.",
            ),
            error.to_string(),
        )
    })?;
    let restored = match back {
        pl_mc::mods::rollback::TrashedModBack::HoldEnded { mod_id, path } => {
            let _writing = HISTORY.lock().await;
            let mut history = ModHistory::load(&history_file).await;
            if history.release(&instance.mc_version, &mod_id).is_some() {
                history.save(&history_file).await.map_err(content_error)?;
            }
            tracing::info!(instance = %instance_id, %mod_id, "a held mod's jar came back from the trash");
            path
        }
        pl_mc::mods::rollback::TrashedModBack::Restored(path) => path,
    };
    Ok(restored
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default())
}
