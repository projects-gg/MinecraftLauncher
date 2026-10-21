//! The troubleshooter (Settings → Sorun yardımcısı, HELP1): every check runs once when the player
//! opens it or a launch fails, never in the background, and each row is sent as it settles. The
//! rules are `pl_data::troubleshoot`'s; this module gathers the facts:
//!
//! - the internet: the system's view of the network and short requests to well-known secure
//!   addresses (the content mirror, the website, Mojang), plus one plain HTTP request whose `Date`
//!   tells the clock even while secure connections fail;
//! - the Projects server (server list ping), the content mirrors, the friends service's health
//!   check and Discord (while friends are on), Microsoft's sign-in chain (with a Microsoft
//!   account);
//! - free space on the data folder's disk, and for the instance Play starts: its Java, its
//!   memory, its mod check and its last crash. A crash is remembered here when it happens
//!   (`meta/last-crashes.json`, with the diagnosis, so the crash dialog opens again after a
//!   restart) and counts as resolved once a later start reached the game without one.
//!
//! Nothing it sends carries personal data (`pl_data::troubleshoot`): the UI builds the support
//! summary from these findings and the system's facts alone.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use jiff::Timestamp;
use parking_lot::Mutex;
use pl_data::troubleshoot::{
    Assembler, CRASH_WINDOW, Check, ContentProbe, Finding, LastCrash, ModsSummary, Observation,
    ServerProbe,
};
use pl_mc::instance::Instance;
use pl_mc::launch::CrashDiagnosis;
use pl_mc::loaders::LoaderKind;
use pl_mc::mods::check::Severity;
use pl_mc::server::{PingFailure, PingOptions, ping_server};
use pl_net::{Probe, ProbeMethod, Reach};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::task::JoinSet;

use crate::commands::launch::CrashView;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// One probe's limit: an address that takes longer is out of reach for a player too.
const PROBE_TIMEOUT: Duration = Duration::from_secs(4);
/// The whole run's limit: whatever has not come by then could not be checked.
const RUN_LIMIT: Duration = Duration::from_secs(15);

/// Well-known secure addresses; any of them answering means the internet works. The content
/// mirror and the website are ours, Mojang's version list is what every launch reads.
const INTERNET: [&str; 3] = [
    "https://cdn.projects.gg/v1/index.json",
    "https://projects.gg/",
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
];
/// A plain address that only sends to its secure twin: its answer carries the server's clock.
const PLAIN: &str = "http://cdn.projects.gg/";
/// Discord's public gateway address (no account, no token).
const DISCORD: &str = "https://discord.com/api/v10/gateway";

/// Where the last crash of each instance is kept, under the data folder's `meta`.
const CRASH_FILE: &str = "last-crashes.json";

/// The kind of instance, for the summary (never its name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum InstanceKind {
    /// Kept up to date from the Projects pack.
    Official,
    Custom,
}

/// The instance the checks looked at, for the summary.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TroubleshootInstance {
    pub kind: InstanceKind,
    pub minecraft: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    /// The Projects pack's version.
    pub pack_version: Option<String>,
}

/// The computer, for the summary.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TroubleshootSystem {
    pub launcher: String,
    pub debug: bool,
    /// "Windows 11 Pro 24H2".
    pub os: String,
    pub arch: String,
    pub memory_mb: u64,
}

/// What a run sends while it goes.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TroubleshootEvent {
    /// The rows checked (the memory and clock rows come only with a finding).
    Plan { checks: Vec<Check> },
    /// A row settled (or changed).
    Finding { finding: Finding },
}

/// A run's result.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TroubleshootReport {
    pub findings: Vec<Finding>,
    pub system: TroubleshootSystem,
    pub instance: Option<TroubleshootInstance>,
}

/// Checks everything once, sending each row as it settles. `instance_id`: the instance Play
/// starts (its Java, memory, mods and last crash); without one the game rows are left out.
#[tauri::command]
#[specta::specta]
pub async fn troubleshoot_run(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: Option<String>,
    on_event: Channel<TroubleshootEvent>,
) -> CmdResult<TroubleshootReport> {
    let instance = match instance_id.as_deref() {
        Some(id) => state.instances.get(id).await.ok(),
        None => None,
    };
    let friends = state.social.view().enabled;
    let microsoft = state
        .auth
        .accounts()
        .iter()
        .any(|account| account.is_microsoft());

    let mut planned = vec![Check::Internet, Check::Server, Check::Content, Check::Disk];
    if friends {
        planned.extend([Check::Friends, Check::Discord]);
    }
    if microsoft {
        planned.push(Check::Microsoft);
    }
    if let Some(instance) = &instance {
        planned.extend([Check::Java, Check::Crash]);
        if instance.loader.kind != LoaderKind::Vanilla {
            planned.push(Check::Mods);
        }
    }
    let mut assembler = Assembler::new(planned);
    let _ = on_event.send(TroubleshootEvent::Plan {
        checks: assembler.planned().to_vec(),
    });

    let mut tasks: JoinSet<Observation> = JoinSet::new();
    tasks.spawn(internet(app.clone()));
    tasks.spawn(server(app.clone()));
    tasks.spawn(content(app.clone()));
    tasks.spawn(disk(app.clone()));
    if friends {
        tasks.spawn(friends_service(app.clone()));
        tasks.spawn(discord(app.clone()));
    }
    if microsoft {
        tasks.spawn(microsoft_services(app.clone()));
    }
    if let Some(instance) = &instance {
        tasks.spawn(java(app.clone(), instance.clone()));
        tasks.spawn(memory(instance.clone()));
        tasks.spawn(last_crash(app.clone(), instance.id.clone()));
        if instance.loader.kind != LoaderKind::Vanilla {
            tasks.spawn(mods(app.clone(), instance.id.clone()));
        }
    }

    let send = |findings: Vec<Finding>| {
        for finding in findings {
            let _ = on_event.send(TroubleshootEvent::Finding { finding });
        }
    };
    let deadline = tokio::time::Instant::now() + RUN_LIMIT;
    loop {
        match tokio::time::timeout_at(deadline, tasks.join_next()).await {
            Ok(Some(Ok(observation))) => send(assembler.observe(observation)),
            // A check that panicked: its row says it could not run (at the end).
            Ok(Some(Err(error))) => tracing::warn!(%error, "a troubleshooter check failed"),
            Ok(None) => break,
            Err(_) => {
                tracing::info!(left = tasks.len(), "troubleshooter checks ran out of time");
                tasks.abort_all();
                break;
            }
        }
    }
    send(assembler.finish());
    let findings = assembler.findings();
    tracing::info!(
        codes = ?findings.iter().map(|f| f.code).collect::<Vec<_>>(),
        "troubleshooter run done"
    );
    Ok(TroubleshootReport {
        findings,
        system: system(&app),
        instance: instance.as_ref().map(instance_facts),
    })
}

/// The crash dialog for the instance's last crash (the row's "Ayrıntılar"), built again from
/// the diagnosis kept when it happened, against the mods folder as it is now. `None` when the
/// crash is resolved, old or gone.
#[tauri::command]
#[specta::specta]
pub async fn troubleshoot_crash(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Option<CrashView>> {
    let Some(stored) = stored_crash(&app, &instance_id) else {
        return Ok(None);
    };
    if stored.resolved || Timestamp::now().duration_since(stored.at) > CRASH_WINDOW {
        return Ok(None);
    }
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let view = crate::commands::modhealth::crash_view(
        &app,
        &state,
        &instance,
        stored.java_major,
        stored.diagnosis,
    )
    .await;
    Ok(Some(view))
}

/// The system's date and time settings (the clock row's fix): Windows' Settings page, macOS's
/// System Settings pane. Linux desktops have no common one; the row offers no fix there.
#[tauri::command]
#[specta::specta]
pub fn troubleshoot_open_clock(app: AppHandle) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let target = if cfg!(target_os = "windows") {
        Some("ms-settings:dateandtime")
    } else if cfg!(target_os = "macos") {
        Some("x-apple.systempreferences:com.apple.Date-Time-Settings.extension")
    } else {
        None
    };
    let failed = |details: String| {
        CmdError::new(
            "CLOCK-SETTINGS",
            pl_lang::pick(
                "Saat ayarları açılamadı. Saati bilgisayarının ayarlarından düzeltebilirsin.",
                "Couldn't open the date and time settings. You can fix the clock in your system settings.",
            ),
            details,
        )
    };
    let target = target.ok_or_else(|| failed("no date and time settings on this system".into()))?;
    app.opener()
        .open_url(target, None::<&str>)
        .map_err(|error| failed(error.to_string()))
}

// --- the checks ------------------------------------------------------------------------------

fn user_agent(app: &AppHandle) -> String {
    format!(
        "ProjectsLauncher/{} (+https://projects.gg)",
        app.package_info().version
    )
}

async fn head_all(app: &AppHandle, urls: &[String]) -> Vec<Probe> {
    let http = app.state::<AppState>().http.clone();
    let mut set = JoinSet::new();
    for (index, url) in urls.iter().enumerate() {
        let (http, url) = (http.clone(), url.clone());
        set.spawn(async move {
            (
                index,
                http.probe(ProbeMethod::Head, &url, PROBE_TIMEOUT).await,
            )
        });
    }
    let mut probes: Vec<(usize, Probe)> = Vec::with_capacity(urls.len());
    while let Some(done) = set.join_next().await {
        if let Ok(done) = done {
            probes.push(done);
        }
    }
    probes.sort_by_key(|(index, _)| *index);
    probes.into_iter().map(|(_, probe)| probe).collect()
}

async fn internet(app: AppHandle) -> Observation {
    let os = pl_net::background::network_hint().map(|hint| hint.connectivity);
    let urls: Vec<String> = INTERNET.iter().map(|url| (*url).to_owned()).collect();
    let agent = user_agent(&app);
    let (secure, plain) = tokio::join!(
        head_all(&app, &urls),
        pl_net::probe::probe_unfollowed(&agent, PLAIN, PROBE_TIMEOUT)
    );
    Observation::Internet {
        os,
        secure,
        plain: vec![plain],
    }
}

async fn server(app: AppHandle) -> Observation {
    let address = crate::commands::server::configured_server(&app)
        .await
        .map(|(_, address)| address)
        .unwrap_or_else(|| crate::commands::server::DEFAULT_SERVER.1.to_owned());
    let options = PingOptions {
        timeout: PROBE_TIMEOUT,
        ..PingOptions::default()
    };
    let probe = match ping_server(&address, &options).await {
        Ok(ping) => ServerProbe::Online {
            latency_ms: u32::try_from(ping.latency_ms).unwrap_or(u32::MAX),
            players: ping
                .players
                .as_ref()
                .map(|players| u32::try_from(players.online.max(0)).unwrap_or(0)),
        },
        Err(pl_mc::Error::Ping { kind, .. }) => ServerProbe::Offline(kind),
        Err(error) => {
            tracing::debug!(%error, "troubleshooter ping failed");
            ServerProbe::Offline(PingFailure::Io)
        }
    };
    Observation::Server(probe)
}

async fn content(app: AppHandle) -> Observation {
    let state = app.state::<AppState>();
    let urls: Vec<String> = state
        .content
        .mirrors()
        .iter()
        .filter_map(|mirror| mirror.join("index.json").ok())
        .map(String::from)
        .collect();
    let mirrors = head_all(&app, &urls)
        .await
        .into_iter()
        .map(|probe| probe.reach)
        .collect();
    let cached = state.content.load_cached().await.is_ok();
    Observation::Content(ContentProbe { mirrors, cached })
}

/// The friends service's health check while the feature is on (a debug build's local service,
/// `PROJECTS_SOCIAL_URL`, too): it answers without touching the service's database.
fn friends_health() -> Option<String> {
    #[cfg(debug_assertions)]
    let custom = std::env::var("PROJECTS_SOCIAL_URL").ok();
    #[cfg(not(debug_assertions))]
    let custom: Option<String> = None;
    pl_social::service_url(true, custom.as_deref())
        .and_then(|url| url.join("v1/health").ok())
        .map(String::from)
}

async fn friends_service(app: AppHandle) -> Observation {
    let Some(health) = friends_health() else {
        return Observation::Failed(Check::Friends);
    };
    let http = app.state::<AppState>().http.clone();
    let probe = http.probe(ProbeMethod::Get, &health, PROBE_TIMEOUT).await;
    Observation::Friends(vec![probe.reach])
}

async fn microsoft_services(app: AppHandle) -> Observation {
    let endpoints = pl_auth::Endpoints::production();
    let urls = [
        endpoints.authorize,
        endpoints.xbox_user_auth,
        endpoints.minecraft_services,
    ];
    let reaches: Vec<Reach> = head_all(&app, &urls)
        .await
        .into_iter()
        .map(|probe| probe.reach)
        .collect();
    Observation::Microsoft(reaches)
}

async fn discord(app: AppHandle) -> Observation {
    let reaches = head_all(&app, &[DISCORD.to_owned()])
        .await
        .into_iter()
        .map(|probe| probe.reach)
        .collect();
    Observation::Discord(reaches)
}

async fn disk(app: AppHandle) -> Observation {
    let root = app.state::<AppState>().mc.dirs.root().to_path_buf();
    // The launch's own measure (a debug build's end-to-end stand-in included).
    let free = tokio::task::spawn_blocking(move || crate::commands::launch::free_space(&root))
        .await
        .ok()
        .flatten();
    Observation::Disk(free)
}

async fn java(app: AppHandle, instance: Instance) -> Observation {
    let state = app.state::<AppState>();
    let (state, fixable) = crate::commands::java::troubleshoot_java(&app, &state, &instance).await;
    Observation::Java { state, fixable }
}

async fn memory(instance: Instance) -> Observation {
    let total = tokio::task::spawn_blocking(pl_mc::instance::system_memory_mb)
        .await
        .unwrap_or(0);
    if total == 0 {
        return Observation::Memory {
            automatic: true,
            validation: Default::default(),
        };
    }
    Observation::Memory {
        automatic: instance.memory_auto,
        validation: pl_mc::instance::validate_memory(&instance.memory, total),
    }
}

async fn mods(app: AppHandle, instance_id: String) -> Observation {
    let state = app.state::<AppState>();
    match crate::commands::modhealth::mods_check(app.clone(), state, instance_id).await {
        Ok(check) => {
            let warnings = check
                .issues
                .iter()
                .filter(|issue| issue.severity == Severity::Warning)
                .count();
            Observation::Mods(Some(ModsSummary {
                blocking: check.blocking,
                warnings: u32::try_from(warnings).unwrap_or(u32::MAX),
                repairable: check.repairable,
                mods: check.checked_files,
            }))
        }
        Err(error) => {
            tracing::debug!(code = %error.code, "troubleshooter mod check failed");
            Observation::Mods(None)
        }
    }
}

async fn last_crash(app: AppHandle, instance_id: String) -> Observation {
    let last = stored_crash(&app, &instance_id).map(|stored| LastCrash {
        at: stored.at,
        kind: stored.diagnosis.kind,
        suspect: stored.suspect,
        resolved: stored.resolved,
    });
    Observation::Crash {
        last,
        now: Timestamp::now(),
    }
}

fn system(app: &AppHandle) -> TroubleshootSystem {
    let machine = pl_data::bundle::MachineInfo::collect();
    TroubleshootSystem {
        launcher: app.package_info().version.to_string(),
        debug: cfg!(debug_assertions),
        os: machine.os,
        arch: machine.arch,
        memory_mb: machine.memory_total / (1024 * 1024),
    }
}

fn instance_facts(instance: &Instance) -> TroubleshootInstance {
    TroubleshootInstance {
        kind: if instance.managed_pack.is_some() {
            InstanceKind::Official
        } else {
            InstanceKind::Custom
        },
        minecraft: instance.mc_version.clone(),
        loader: instance.loader.kind,
        loader_version: instance.loader.version.clone(),
        pack_version: instance
            .managed_pack
            .as_ref()
            .map(|pack| pack.version.clone()),
    }
}

// --- the last crash ----------------------------------------------------------------------------

/// The last crash of one instance, as kept on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCrash {
    at: Timestamp,
    diagnosis: CrashDiagnosis,
    /// The Java that ran the game.
    java_major: Option<u32>,
    /// The mod the crash points at most strongly (display name).
    suspect: Option<String>,
    /// A later start reached the game and ended without a crash.
    #[serde(default)]
    resolved: bool,
}

/// The crashes, read from disk on first use.
#[derive(Default)]
struct Crashes(Mutex<Option<HashMap<String, StoredCrash>>>);

fn crash_file(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().mc.dirs.meta().join(CRASH_FILE)
}

/// Runs `change` on the kept crashes (read from disk first if needed); `true` from it writes the
/// file again, without the crashes older than a week.
fn with_crashes<R>(
    app: &AppHandle,
    change: impl FnOnce(&mut HashMap<String, StoredCrash>) -> (R, bool),
) -> R {
    if app.try_state::<Crashes>().is_none() {
        app.manage(Crashes::default());
    }
    let crashes = app.state::<Crashes>();
    let file = crash_file(app);
    let mut guard = crashes.0.lock();
    let map = guard.get_or_insert_with(|| {
        std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    });
    let (result, write) = change(map);
    if write {
        let now = Timestamp::now();
        map.retain(|_, crash| now.duration_since(crash.at) <= CRASH_WINDOW);
        match serde_json::to_vec_pretty(&*map) {
            Ok(bytes) => {
                if let Some(dir) = file.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Err(error) = pl_mc::paths::write_atomic_blocking(&file, &bytes) {
                    tracing::warn!(%error, "could not keep the last crash");
                }
            }
            Err(error) => tracing::warn!(%error, "could not write the last crash"),
        }
    }
    result
}

fn stored_crash(app: &AppHandle, instance_id: &str) -> Option<StoredCrash> {
    with_crashes(app, |map| (map.get(instance_id).cloned(), false))
}

/// A game crashed: keep its diagnosis for the troubleshooter's row and its "Ayrıntılar".
pub(crate) fn remember_crash(
    app: &AppHandle,
    instance_id: &str,
    diagnosis: CrashDiagnosis,
    java_major: Option<u32>,
    view: &CrashView,
) {
    let suspect = view
        .suspects
        .iter()
        .find(|suspect| suspect.strong)
        .map(|suspect| suspect.name.clone());
    let stored = StoredCrash {
        at: Timestamp::now(),
        diagnosis,
        java_major,
        suspect,
        resolved: false,
    };
    with_crashes(app, |map| {
        map.insert(instance_id.to_owned(), stored);
        ((), true)
    });
}

/// A start reached the game and ended without a crash: the instance's last crash is resolved.
pub(crate) fn played_without_crash(app: &AppHandle, instance_id: &str) {
    with_crashes(app, |map| match map.get_mut(instance_id) {
        Some(crash) if !crash.resolved => {
            crash.resolved = true;
            ((), true)
        }
        _ => ((), false),
    });
}
