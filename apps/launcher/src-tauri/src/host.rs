//! "Dünyanı arkadaşlarına aç" (HOST1) in the app: `pl_worlds::Worlds` wired to the friends
//! service (`world` frames on the presence socket), the in-game overlay (rows H1–H16) and the
//! launcher's instances, worlds, accounts and signed content. The logic and its tests live in
//! `pl-worlds`; this module only implements its traits with the app's parts.
//!
//! The feature shows only while the signed config's `features.worlds` (with `friends`) is on and
//! the service takes `worlds` with Studio's switch on (`HostView.enabled`). Debug builds may force
//! the signed side with `PROJECTS_SOCIAL_WORLDS=1` for a local `wrangler dev`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pl_content::PackState;
use pl_host::manifest::matching::InstallInfo;
use pl_host::manifest::{Loader, Manifest, ModRef, PackRef};
use pl_host::transport::BoxFuture;
use pl_mc::loaders::LoaderKind;
use pl_overlay::{ConnectOutcome, Overlay, WorldsShown};
use pl_social::{SocialService, WorldAnswer, WorldError, WorldFrame};
use pl_worlds::manifest::{InstallSide, Jar};
use pl_worlds::{
    Account, Bridge, HostInstall, Launcher, PlayTogether, Progress, Service, UiEvent, Worlds,
    WorldsConfig,
};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Every change of the view, whole (`HostView`).
pub const CHANGED_EVENT: &str = "host-changed";
/// Moments worth a toast or a notification (`HostNotice`).
pub const NOTICE_EVENT: &str = "host-notice";
/// The Minecraft versions the overlay mod hosts on (host1.md §0, phase 1).
const HOST_VERSIONS: [&str; 3] = ["26.1.2", "26.2", "26.3"];
/// Copies made for a friend's world: instance id → that friend's id (PROTOCOL.md §10.2
/// `copyOf`; kept in the launcher's data, never inside a game folder).
const COPIES_FILE: &str = "host/copies.json";
/// Jars that open a world to the internet by themselves: an install with one does not host.
const RELAY_MOD_NAMES: [&str; 2] = ["e4mc", "e4all"];

/// The friends service as world sessions use it.
struct SocialLink(SocialService);

impl Service for SocialLink {
    fn request(
        &self,
        frame: WorldFrame,
        wait: Duration,
    ) -> BoxFuture<'_, Result<WorldAnswer, WorldError>> {
        Box::pin(self.0.world_request(frame, wait))
    }

    fn send(&self, frame: WorldFrame) {
        self.0.world_send(frame);
    }
}

/// The running game's overlay.
struct OverlayLink(Overlay);

impl Bridge for OverlayLink {
    fn send(&self, frames: Vec<pl_overlay::protocol::ToGame>) {
        self.0.host_send(frames);
    }

    fn show(&self, shown: WorldsShown) {
        self.0.set_worlds(shown);
    }

    fn connect(&self, port: u16, label: String) -> BoxFuture<'_, bool> {
        Box::pin(
            async move { self.0.connect_world_in_game(port, label).await == ConnectOutcome::Done },
        )
    }

    /// In the overlay's language, which the player may have picked in the game (OVL-LANG).
    fn world_label(&self, friend: &str) -> String {
        pl_worlds::world_label_in(friend, self.0.language())
    }
}

/// "Birlikte oyna" for "Projects'te buluşun".
struct Together(SocialService);

impl PlayTogether for Together {
    fn invite(&self, friend: &str) -> BoxFuture<'_, ()> {
        let friend = friend.to_owned();
        Box::pin(async move {
            if let Err(error) = self.0.invite(&friend).await {
                tracing::info!(%error, "a play-together invite for a meeting on Projects did not go");
            }
        })
    }
}

/// World sharing for the app: its parts wired, the page told every change.
pub fn create(
    app: &AppHandle,
    root: &Path,
    version: &str,
    social: SocialService,
    overlay: Overlay,
) -> Worlds {
    let emitter = app.clone();
    let ui: pl_worlds::UiSink = Arc::new(move |event| {
        let _ = match event {
            UiEvent::Changed(view) => emitter.emit(CHANGED_EVENT, view),
            UiEvent::Notice(notice) => emitter.emit(NOTICE_EVENT, notice),
        };
    });
    let worlds = Worlds::new(
        Arc::new(SocialLink(social.clone())),
        Arc::new(OverlayLink(overlay.clone())),
        Arc::new(AppLauncher { app: app.clone() }),
        Arc::new(Together(social.clone())),
        ui,
        WorldsConfig::new(version, root.to_path_buf()),
    );
    let hears = worlds.clone();
    social.on_world(move |event| hears.on_service(event));
    let hears = worlds.clone();
    overlay.on_host(move |signal| hears.on_bridge(signal));
    if debug_worlds() {
        social.set_worlds_enabled(true);
        worlds.set_allowed(true);
    }
    worlds
}

/// The signed config's `features.worlds` (with `friends`): the presence connection asks for
/// `worlds` and the page may show the feature.
pub fn apply_flag(state: &AppState, friends: bool, worlds: bool) {
    let on = (friends && worlds) || debug_worlds();
    state.social.set_worlds_enabled(on);
    state.worlds.set_allowed(on);
}

/// `PROJECTS_SOCIAL_WORLDS=1` (debug builds only): world sharing on whatever the signed config
/// says, for a local `wrangler dev`.
fn debug_worlds() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("PROJECTS_SOCIAL_WORLDS").is_ok_and(|value| value == "1")
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

/// The friends feature's every update: world sharing reads the names and where the player is.
pub fn on_social(app: &AppHandle, update: &pl_social::SocialUpdate) {
    if let pl_social::SocialUpdate::View(view) = update
        && let Some(state) = app.try_state::<AppState>()
    {
        state.worlds.on_view(view);
    }
}

/// The launcher's own parts.
struct AppLauncher {
    app: AppHandle,
}

impl AppLauncher {
    fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }
}

fn copies_path(root: &Path) -> PathBuf {
    root.join(COPIES_FILE)
}

fn read_copies(root: &Path) -> BTreeMap<String, String> {
    std::fs::read(copies_path(root))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn remember_copy(root: &Path, instance_id: &str, host: &str) {
    let mut copies = read_copies(root);
    copies.insert(instance_id.to_owned(), host.to_owned());
    let path = copies_path(root);
    let saved = serde_json::to_vec_pretty(&copies)
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
            }
            pl_mc::paths::write_atomic_blocking(&path, &bytes).map_err(|error| error.to_string())
        });
    if let Err(error) = saved {
        tracing::warn!(%error, "a world copy's host was not remembered");
    }
}

/// The official pack family new installs follow, from the cached content.
async fn official_family(state: &AppState) -> Option<(pl_content::ContentSnapshot, String)> {
    let snapshot = state.content.load_cached().await.ok()?;
    let family = crate::commands::official::official_pack_ref(&snapshot)?
        .family_id()
        .to_owned();
    Some((snapshot, family))
}

/// Whether a relay mod (e4mc, e4all) is enabled in this `mods/` folder.
fn has_relay_mod(mods: &Path) -> bool {
    std::fs::read_dir(mods).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            name.ends_with(".jar") && RELAY_MOD_NAMES.iter().any(|relay| name.starts_with(relay))
        })
    })
}

/// The pack's own record of its jars: SHA-1 → Modrinth project and version, from the manifest of
/// the version the install is on (an install behind the index's version is looked up instead).
async fn pack_known(
    state: &AppState,
    snapshot: &pl_content::ContentSnapshot,
    pack: Option<&PackState>,
) -> HashMap<String, ModRef> {
    let Some(pack) = pack else {
        return HashMap::new();
    };
    let Some(entry) = snapshot.index.pack(&pack.pack_id) else {
        return HashMap::new();
    };
    if entry.latest != pack.version_id {
        return HashMap::new();
    }
    let Ok(manifest) = state.content.pack_manifest(entry).await else {
        return HashMap::new();
    };
    let sha_of: HashMap<&str, &str> = manifest
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.hashes.sha1.as_str()))
        .collect();
    manifest
        .projects
        .items
        .iter()
        .filter_map(|item| {
            let modrinth = item.modrinth.as_ref()?;
            let sha1 = sha_of.get(item.path.as_str())?;
            Some((
                sha1.to_ascii_lowercase(),
                ModRef::new(&modrinth.project_id, &modrinth.version_id),
            ))
        })
        .collect()
}

/// Modrinth's versions of these jars (by SHA-1); none when it can't be asked.
async fn look_up(state: &AppState, hashes: Vec<String>) -> HashMap<String, ModRef> {
    if hashes.is_empty() {
        return HashMap::new();
    }
    let client = crate::commands::modrinth::modrinth_client(state);
    match client
        .version_files(&hashes, pl_mc::modrinth::ModrinthHashAlgorithm::Sha1)
        .await
    {
        Ok(found) => found
            .into_iter()
            .map(|(hash, version)| {
                (
                    hash.to_ascii_lowercase(),
                    ModRef::new(version.project_id, version.id),
                )
            })
            .collect(),
        Err(error) => {
            tracing::info!(%error, "Modrinth did not name a world's mods");
            HashMap::new()
        }
    }
}

/// The world manifest of an install (`pl_worlds::manifest`), from its files on disk.
async fn install_manifest(state: &AppState, instance_id: &str) -> Option<(Manifest, i64)> {
    let instance = state.instances.get(instance_id).await.ok()?;
    if instance.loader.kind != LoaderKind::Fabric {
        return None;
    }
    let game_dir = state.instances.game_dir(instance_id).ok()?;
    let mods = game_dir.join("mods");
    let jars: Vec<Jar> = tokio::task::spawn_blocking(move || pl_worlds::manifest::scan(&mods))
        .await
        .ok()?;
    let pack_state = PackState::load(&game_dir).await.ok().flatten();
    let snapshot = state.content.load_cached().await.ok();
    let known = match &snapshot {
        Some(snapshot) => pack_known(state, snapshot, pack_state.as_ref()).await,
        None => HashMap::new(),
    };
    let looked_up = look_up(state, pl_worlds::manifest::unknown_hashes(&jars, &known)).await;
    let pack = pack_state
        .as_ref()
        .filter(|_| instance.managed_pack.is_some())
        .filter(|pack| !pack.version_id.is_empty())
        .map(|pack| PackRef {
            family: if pack.family.is_empty() {
                pack.pack_id.clone()
            } else {
                pack.family.clone()
            },
            id: pack.pack_id.clone(),
            version: pack.version_id.clone(),
        });
    // The ready-made sets installed here: a friend's copy adds the same ones (their both-side
    // mods are in `mods` too).
    let sets = pl_content::ModSetStates::load(&game_dir)
        .await
        .map(|states| states.installed_ids())
        .unwrap_or_default();
    let side = InstallSide {
        mc: instance.mc_version.clone(),
        loader: Loader {
            kind: "fabric".into(),
            version: instance.loader.version.clone().unwrap_or_default(),
        },
        pack,
        sets,
    };
    let last_played = instance.last_played_at.map_or(0, |at| at.as_second());
    Some((
        pl_worlds::manifest::build(side, &jars, &known, &looked_up),
        last_played,
    ))
}

/// A new install following the official family's member at `mc`; its id.
async fn create_member(state: &AppState, name: &str, mc: &str) -> Option<String> {
    let (snapshot, family) = official_family(state).await?;
    let member = snapshot.index.family_member(&family, mc)?;
    let manifest = state.content.pack_manifest(member).await.ok()?;
    let loader = crate::commands::official::pack_loader(&manifest);
    let total = pl_mc::instance::system_memory_mb();
    let mut memory = pl_mc::instance::recommended_memory(total, loader.kind != LoaderKind::Vanilla);
    if let Some(mb) = manifest.projects.recommended_memory_mb {
        memory.max_mb = memory.max_mb.max(mb);
        memory = pl_mc::instance::clamp_memory(&memory, total);
    }
    let mut created = state
        .instances
        .create(pl_mc::instance::NewInstance {
            name: name.to_owned(),
            mc_version: manifest.dependencies.minecraft.clone(),
            loader,
            icon: None,
            group: None,
            memory: Some(memory),
            managed_pack: Some(pl_mc::instance::ManagedPack {
                pack_id: member.id.clone(),
                // Empty until the first sync records the installed version.
                version: String::new(),
            }),
        })
        .await
        .map_err(|error| tracing::warn!(%error, "an install for a friend's world was not made"))
        .ok()?;
    created.memory_auto = true;
    if let Err(error) = state.instances.update(&created).await {
        tracing::warn!(%error, "an install for a friend's world keeps its first settings");
    }
    Some(created.id)
}

/// The manifest's mods an install lacks, from Modrinth, into its `mods/` (the pack's own items
/// come with the game's start, the ready-made sets' are installed already).
async fn add_mods(
    app: &AppHandle,
    state: &AppState,
    instance_id: &str,
    manifest: &Manifest,
    progress: &Progress,
) -> bool {
    let Ok(instance) = state.instances.get(instance_id).await else {
        return false;
    };
    let Ok(game_dir) = state.instances.game_dir(instance_id) else {
        return false;
    };
    // The pack's items by project: its sync brings them.
    let mut provided: std::collections::HashSet<String> = match official_family(state).await {
        Some((snapshot, family)) => match snapshot.index.family_member(&family, &manifest.mc) {
            Some(member) => state
                .content
                .pack_manifest(member)
                .await
                .map(|pack| {
                    pack.projects
                        .items
                        .iter()
                        .filter_map(|item| item.modrinth.as_ref().map(|m| m.project_id.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            None => Default::default(),
        },
        None => Default::default(),
    };
    // The ready-made sets' files (a second copy of one of their mods would stop the game).
    if let Ok(states) = pl_content::ModSetStates::load(&game_dir).await {
        provided.extend(states.owned_projects());
    }
    let mods = game_dir.join("mods");
    let have: std::collections::HashSet<String> = {
        let jars = tokio::task::spawn_blocking(move || pl_worlds::manifest::scan(&mods))
            .await
            .unwrap_or_default();
        let found = look_up(state, jars.iter().map(|jar| jar.sha1.clone()).collect()).await;
        found.into_values().map(|m| m.version).collect()
    };
    let wanted = pl_worlds::manifest::mods_to_add(manifest, &provided, &have);
    if wanted.is_empty() {
        progress(100);
        return true;
    }
    let client = crate::commands::modrinth::modrinth_client(state);
    let Ok(versions) = client.versions(&wanted).await else {
        return false;
    };
    let Ok(addons) = crate::commands::modrinth::addons_for(state, instance_id) else {
        return false;
    };
    let lock = crate::commands::modrinth::instance_lock(app, instance_id);
    let _guard = lock.lock().await;
    let target = pl_mc::modrinth::ModrinthResolveTarget::for_loader(
        instance.loader.kind,
        instance.mc_version.clone(),
    );
    let report = Arc::clone(progress);
    let installed = addons
        .install(
            &client,
            &state.mc.downloader,
            pl_mc::gamefiles::ImportKind::Mod,
            versions,
            &target,
            pl_mc::addons::AddonInstallOptions {
                with_dependencies: false,
                concurrency: 6,
                cancel: pl_net::CancellationToken::new(),
                progress: Some(Arc::new(move |p| {
                    if let pl_mc::addons::AddonProgress::Downloading {
                        done_bytes,
                        total_bytes,
                        ..
                    } = p
                        && let Some(percent) =
                            done_bytes.saturating_mul(100).checked_div(total_bytes)
                    {
                        report(u8::try_from(percent.min(99)).unwrap_or(99));
                    }
                })),
            },
        )
        .await;
    match installed {
        Ok(_) => {
            progress(100);
            true
        }
        Err(error) => {
            tracing::warn!(%error, "a friend's world's mods did not install");
            false
        }
    }
}

impl Launcher for AppLauncher {
    fn host_install(&self, instance_id: &str) -> BoxFuture<'_, Option<HostInstall>> {
        let instance_id = instance_id.to_owned();
        Box::pin(async move {
            let state = self.state();
            let instance = state.instances.get(&instance_id).await.ok()?;
            let game_dir = state.instances.game_dir(&instance_id).ok()?;
            let official = official_family(&state).await;
            let family = official.as_ref().and_then(|(snapshot, _)| {
                crate::commands::official::instance_family(
                    &snapshot.index,
                    &instance,
                    Some(game_dir.as_path()),
                )
            });
            let follows = matches!((&official, &family), (Some((_, official)), Some(family)) if official == family);
            // The overlay's own switch is not part of it: with it off the button still shows and
            // its window offers "Aç". The mod's hello (`host`) decides once the world opens.
            let hostable = follows
                && instance.loader.kind == LoaderKind::Fabric
                && HOST_VERSIONS.contains(&instance.mc_version.as_str())
                && !has_relay_mod(&game_dir.join("mods"));
            Some(HostInstall {
                mc: instance.mc_version.clone(),
                family,
                game_dir,
                hostable,
            })
        })
    }

    fn manifest(&self, instance_id: &str) -> BoxFuture<'_, Option<Manifest>> {
        let instance_id = instance_id.to_owned();
        Box::pin(async move {
            let state = self.state();
            install_manifest(&state, &instance_id)
                .await
                .map(|(manifest, _)| manifest)
        })
    }

    fn world_name(&self, instance_id: &str, folder: &str) -> BoxFuture<'_, String> {
        let (instance_id, folder) = (instance_id.to_owned(), folder.to_owned());
        Box::pin(async move {
            let state = self.state();
            let Ok(game_dir) = state.instances.game_dir(&instance_id) else {
                return folder;
            };
            pl_mc::gamefiles::list_worlds(&game_dir)
                .await
                .ok()
                .and_then(|worlds| worlds.into_iter().find(|w| w.folder_name == folder))
                .map(|world| world.name)
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(folder)
        })
    }

    fn backup_if_old(
        &self,
        instance_id: &str,
        folder: &str,
        progress: Progress,
    ) -> BoxFuture<'_, bool> {
        let (instance_id, folder) = (instance_id.to_owned(), folder.to_owned());
        Box::pin(async move {
            const DAY_MS: i64 = 24 * 60 * 60 * 1000;
            let state = self.state();
            let Ok(game_dir) = state.instances.game_dir(&instance_id) else {
                return false;
            };
            let newest = pl_mc::gamefiles::list_world_backups(&game_dir)
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|backup| backup.world_folder.as_deref() == Some(folder.as_str()))
                .filter_map(|backup| backup.created_ms)
                .max();
            let now_ms = jiff::Timestamp::now().as_millisecond();
            if newest.is_some_and(|at| now_ms.saturating_sub(at) < DAY_MS) {
                return true;
            }
            let lock = crate::commands::library::world_lock(&self.app, &instance_id);
            let _guard = lock.lock().await;
            let world = game_dir.join("saves").join(&folder);
            let dest = pl_mc::gamefiles::default_backup_path(&game_dir, &folder);
            let report: pl_mc::gamefiles::BackupProgressFn = Arc::new(move |p| {
                if let Some(percent) = p.bytes_done.saturating_mul(100).checked_div(p.bytes_total) {
                    progress(u8::try_from(percent.min(100)).unwrap_or(100));
                }
            });
            // Marked, so the Worlds tab lists it as "Arkadaşlarına açılmadan önce".
            match pl_mc::gamefiles::backup_world_marked(
                &world,
                &dest,
                Some(pl_mc::gamefiles::BackupReason::BeforeSharing),
                pl_net::CancellationToken::new(),
                Some(report),
            )
            .await
            {
                Ok(_) => true,
                Err(error) => {
                    tracing::warn!(%error, "the backup before sharing a world failed");
                    false
                }
            }
        })
    }

    fn account(&self) -> Option<Account> {
        let account = self.state().auth.active_account()?;
        Some(Account {
            id: account.id.to_string(),
            name: account.username.clone(),
            premium: account.kind == pl_auth::AccountKind::Microsoft,
        })
    }

    fn use_account(&self, account_id: &str) -> BoxFuture<'_, Option<Account>> {
        let id = uuid::Uuid::parse_str(account_id).ok();
        Box::pin(async move {
            self.state().auth.set_active(id?).ok()?;
            self.account()
        })
    }

    fn installs(&self, mc: &str) -> BoxFuture<'_, Vec<InstallInfo>> {
        let mc = mc.to_owned();
        Box::pin(async move {
            let state = self.state();
            let copies = read_copies(&state.root);
            let Ok(instances) = state.instances.list().await else {
                return Vec::new();
            };
            let mut installs = Vec::new();
            for instance in instances
                .into_iter()
                .filter(|i| i.mc_version == mc && i.loader.kind == LoaderKind::Fabric)
            {
                let Some((manifest, last_played)) = install_manifest(&state, &instance.id).await
                else {
                    continue;
                };
                installs.push(InstallInfo {
                    id: instance.id.clone(),
                    mc: manifest.mc.clone(),
                    loader: Some(manifest.loader.kind.clone()),
                    pack: manifest.pack.clone(),
                    mods: manifest.mods.clone(),
                    copy_of: copies.get(&instance.id).cloned(),
                    last_played,
                });
            }
            installs
        })
    }

    fn latest_pack(&self, family: &str, mc: &str) -> BoxFuture<'_, Option<String>> {
        let (family, mc) = (family.to_owned(), mc.to_owned());
        Box::pin(async move {
            let state = self.state();
            let snapshot = state.content.load_cached().await.ok()?;
            snapshot
                .index
                .family_member(&family, &mc)
                .map(|pack| pack.latest.clone())
        })
    }

    fn refresh_content(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let state = self.state();
            match state.content.refresh().await {
                Ok(snapshot) => crate::commands::content::apply_snapshot(&state, &snapshot),
                Err(error) => tracing::info!(%error, "content refresh for a world's pack failed"),
            }
        })
    }

    fn install_official(&self, mc: &str) -> BoxFuture<'_, Option<String>> {
        let mc = mc.to_owned();
        Box::pin(async move {
            let state = self.state();
            create_member(&state, &format!("Projects {mc}"), &mc).await
        })
    }

    fn mods_size(&self, mods: &[ModRef]) -> BoxFuture<'_, u64> {
        let ids: Vec<String> = mods.iter().map(|m| m.version.clone()).collect();
        Box::pin(async move {
            let state = self.state();
            let client = crate::commands::modrinth::modrinth_client(&state);
            client.versions(&ids).await.map_or(0, |versions| {
                versions
                    .iter()
                    .filter_map(|version| version.primary_file())
                    .map(|file| file.size)
                    .sum()
            })
        })
    }

    fn make_copy(
        &self,
        name: &str,
        host: &str,
        manifest: &Manifest,
        progress: Progress,
    ) -> BoxFuture<'_, Option<String>> {
        let (name, host, manifest) = (name.to_owned(), host.to_owned(), manifest.clone());
        Box::pin(async move {
            let state = self.state();
            let instance_id = create_member(&state, &name, &manifest.mc).await?;
            remember_copy(&state.root, &instance_id, &host);
            // The host's ready-made sets first, then the world's mods nothing else brings.
            let sets = pl_worlds::manifest::copy_sets(&manifest);
            crate::commands::modsets::install_for_copy(&self.app, &state, &instance_id, &sets)
                .await;
            add_mods(&self.app, &state, &instance_id, &manifest, &progress)
                .await
                .then_some(instance_id)
        })
    }

    fn update_copy(
        &self,
        instance_id: &str,
        manifest: &Manifest,
        progress: Progress,
    ) -> BoxFuture<'_, bool> {
        let (instance_id, manifest) = (instance_id.to_owned(), manifest.clone());
        Box::pin(async move {
            let state = self.state();
            // Sets the host added since the copy was made come first, like in a new copy.
            let sets = pl_worlds::manifest::copy_sets(&manifest);
            crate::commands::modsets::install_for_copy(&self.app, &state, &instance_id, &sets)
                .await;
            add_mods(&self.app, &state, &instance_id, &manifest, &progress).await
        })
    }

    /// Only a game this launcher started may come in (PROTOCOL.md §8): the connection's owner
    /// must be one of its processes, or one they started (a wrapper command's Java:
    /// `hooks.wrapper` starts the game through another program), when Windows tells who owns it.
    /// With several games (PLAY-AGAIN) the one that joins may be any of them.
    fn accept_filter(&self) -> Option<pl_host::guest::local::AcceptFilter> {
        let app = self.app.clone();
        Some(Arc::new(move |remote: std::net::SocketAddr| {
            let Some(state) = app.try_state::<AppState>() else {
                return false;
            };
            let games: Vec<u32> = state
                .games
                .lock()
                .iter()
                .filter(|game| !game.adopted)
                .filter_map(|game| game.process.pid())
                .collect();
            if games.is_empty() {
                tracing::info!("a connection to a friend's world came while no game of ours ran");
                return false;
            }
            match pl_desktop::tcp::loopback_owner(remote.port()) {
                Some(owner)
                    if !games
                        .iter()
                        .any(|game| pl_desktop::process::started_by(owner, *game)) =>
                {
                    tracing::warn!("a connection to a friend's world came from another program");
                    false
                }
                // A game's (or its wrapper's Java), or the system can't tell.
                _ => true,
            }
        }))
    }
}
