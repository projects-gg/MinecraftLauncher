//! 7/24 Bulut (paid cloud AFK accounts on afk.projects.gg): thin commands over
//! `pl_cloud_afk::CloudService`. Nothing of it shows unless the service answers and is not off
//! (`cloud_afk_overview` → `available`); the account parts need the friends session, whose token is
//! the service's sign-in. Purchase, renewal and linking always open the site. Debug builds may point
//! it at a local `wrangler dev` with `PROJECTS_CLOUD_AFK_URL`.
//!
//! The live console streams as `cloud-afk-live` events while a console is watched.

use std::sync::{Arc, OnceLock};

use pl_cloud_afk::{
    Bearer, CloudAccount, CloudConfig, CloudEdit, CloudEditView, CloudError, CloudLogPage,
    CloudMovePlan, CloudOverview, CloudSaveOutcome, CloudServer, CloudService, Copies, LiveSink,
};
use pl_lang::{Lang, pick};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

static SERVICE: OnceLock<CloudService> = OnceLock::new();

/// The service, made on first use (the AFK page's first look).
pub(crate) fn service(app: &AppHandle, state: &AppState) -> &'static CloudService {
    SERVICE.get_or_init(|| {
        let identity = crate::identity::current();
        let store: Arc<dyn pl_afk::SecretStore> =
            if state.ephemeral_secrets || !pl_afk::KeyringSecretStore::is_available() {
                Arc::new(pl_afk::MemorySecretStore::new())
            } else {
                Arc::new(pl_afk::KeyringSecretStore::with_service(
                    identity.keychain_service.clone(),
                ))
            };
        let copies = Copies::new(store, state.root.join("cloud-afk").join("copies.json"));
        let version = app.package_info().version.to_string();
        let config = CloudConfig::new(
            pl_cloud_afk::api_url(debug_override().as_deref()),
            format!("ProjectsLauncher/{version}"),
        );
        let emitter = app.clone();
        let sink: LiveSink = Arc::new(move |event| {
            let _ = emitter.emit("cloud-afk-live", &event);
        });
        let tauri::async_runtime::RuntimeHandle::Tokio(handle) = tauri::async_runtime::handle();
        CloudService::new(state.http.client().clone(), config, copies, handle, sink)
    })
}

/// `PROJECTS_CLOUD_AFK_URL` (debug builds only): a local `wrangler dev` instead of the service.
fn debug_override() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        std::env::var("PROJECTS_CLOUD_AFK_URL")
            .ok()
            .filter(|url| !url.trim().is_empty())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// The friends session's token, or "sign in again".
pub(crate) fn token(state: &AppState) -> CmdResult<Bearer> {
    state
        .social
        .session_token()
        .map(|token| Bearer::new(token.expose()))
        .ok_or_else(|| CloudError::SignedOut.into())
}

/// A desktop AFK account and the cloud account with its name on its server (the same account in
/// two places keeps kicking itself).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudTwin {
    pub desktop_id: String,
    pub cloud_id: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudPageView {
    pub overview: CloudOverview,
    pub twins: Vec<CloudTwin>,
}

fn desktop_server(account: &pl_afk::AfkAccount) -> CloudServer {
    let port = u16::try_from(account.server_port).unwrap_or(0);
    pl_cloud_afk::normalize(account.effective_host(), port)
        .unwrap_or_else(|_| CloudServer::new(account.effective_host().to_ascii_lowercase(), port))
}

fn twins_of(state: &AppState, accounts: &[CloudAccount]) -> Vec<CloudTwin> {
    state
        .afk
        .accounts()
        .iter()
        .filter_map(|desktop| {
            let server = desktop_server(desktop);
            accounts
                .iter()
                .find(|cloud| {
                    cloud.name.eq_ignore_ascii_case(&desktop.nickname)
                        && pl_cloud_afk::same_server(&cloud.server, &server)
                })
                .map(|cloud| CloudTwin {
                    desktop_id: desktop.id.clone(),
                    cloud_id: cloud.id.clone(),
                })
        })
        .collect()
}

/// The AFK page's look at 7/24 Bulut: read when the page opens and after an action, never on a
/// timer. `force`: "Tekrar dene" (the gate is read again now).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_overview(
    app: AppHandle,
    state: State<'_, AppState>,
    force: bool,
) -> CmdResult<CloudPageView> {
    let cloud = service(&app, &state);
    if state.social.is_signed_out() {
        // The contract: the keychain copies go with a sign-out.
        cloud.forget().await;
    }
    let token = state
        .social
        .session_token()
        .map(|token| Bearer::new(token.expose()));
    let overview = cloud.overview(token.as_ref(), force).await;
    let twins = twins_of(&state, &overview.accounts);
    Ok(CloudPageView { overview, twins })
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_start(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<CloudAccount> {
    let token = token(&state)?;
    Ok(service(&app, &state).start(&token, &id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_stop(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<CloudAccount> {
    let token = token(&state)?;
    Ok(service(&app, &state).stop(&token, &id).await?)
}

/// "Yeniden bağlan": the machine starts the client again (never repeated by itself).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_restart(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<CloudAccount> {
    let token = token(&state)?;
    Ok(service(&app, &state).restart(&token, &id).await?)
}

/// A console line; a password command is sealed before it leaves the launcher. Nothing is echoed:
/// the server's answer shows in the console.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_send(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    line: String,
) -> CmdResult<()> {
    let token = token(&state)?;
    Ok(service(&app, &state).send(&token, &id, &line).await?)
}

/// The machine's masked log before `before` (Unix seconds; none: the last hour).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_logs(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    before: Option<i64>,
) -> CmdResult<CloudLogPage> {
    let token = token(&state)?;
    Ok(service(&app, &state).logs(&token, &id, before).await?)
}

/// The console is on screen: its lines and state stream as `cloud-afk-live`.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_watch(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    let token = token(&state)?;
    Ok(service(&app, &state).watch(&token, &id).await?)
}

/// The console left the screen (closed, the window hidden): the socket closes with the last one.
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_unwatch(app: AppHandle, state: State<'_, AppState>, id: String) {
    service(&app, &state).unwatch(&id);
}

/// The account for the settings sheet, read now; secret rows with this computer's copies.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_edit(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<CloudEditView> {
    let token = token(&state)?;
    Ok(service(&app, &state).edit_view(&token, &id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_save(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    edit: CloudEdit,
) -> CmdResult<CloudSaveOutcome> {
    let token = token(&state)?;
    Ok(service(&app, &state).save(&token, &id, edit).await?)
}

/// How "Göster" ended.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CloudRevealOutcome {
    /// The words, for a few seconds on screen: never stored.
    Shown { text: String },
    /// The machine cannot show this row (it does not open, it is not there, an older machine):
    /// write it again.
    Retype,
    /// The account's machine is not connected now.
    Offline,
    /// Too many reveals; `retry_after` seconds when the service said.
    Limited { retry_after: Option<u64> },
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_reveal(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    row_id: String,
) -> CmdResult<CloudRevealOutcome> {
    let token = token(&state)?;
    match service(&app, &state).reveal(&token, &id, &row_id).await {
        Ok(text) => Ok(CloudRevealOutcome::Shown {
            text: text.as_str().to_owned(),
        }),
        Err(error) => match error.code() {
            "secret_unreadable" | "seal" => Ok(CloudRevealOutcome::Retype),
            // The service answers 503 while the account's machine is away.
            "host_offline" | "unavailable" => Ok(CloudRevealOutcome::Offline),
            "rate_limited" => Ok(CloudRevealOutcome::Limited {
                retry_after: error.retry_after(),
            }),
            _ => Err(error.into()),
        },
    }
}

/// The launcher's active game account: whose messages "Yalnız ana hesabımdan" rules answer.
fn owner_name(state: &AppState) -> String {
    state
        .auth
        .active_account()
        .map(|account| account.username)
        .unwrap_or_default()
}

async fn desktop_parts(
    state: &AppState,
    desktop_id: &str,
) -> CmdResult<(pl_afk::AfkAccount, Vec<String>)> {
    let store = state.afk.store().clone();
    let id = desktop_id.to_owned();
    let found = tauri::async_runtime::spawn_blocking(move || {
        let account = store.account(&id)?;
        Some((account, store.login_commands(&id)))
    })
    .await
    .map_err(CmdError::internal)?;
    let Some((account, commands)) = found else {
        return Err(pl_afk::AfkError::AccountNotFound(desktop_id.to_owned()).into());
    };
    Ok((account, commands?))
}

/// "Buluta taşı": the desktop account in the cloud's form, and what stays on the desktop.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_move_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    desktop_id: String,
) -> CmdResult<CloudMovePlan> {
    let (account, commands) = desktop_parts(&state, &desktop_id).await?;
    let owner = owner_name(&state);
    Ok(service(&app, &state)
        .move_plan(&account, &commands, &owner)
        .await)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudMoveResult {
    pub outcome: CloudSaveOutcome,
    /// The desktop session was stopped (it would kick the cloud one).
    pub stopped_here: bool,
    /// The cloud account was started.
    pub started: bool,
}

/// Writes the desktop account over the cloud account `cloud_id` (`rev` as the player saw it), then
/// stops it here, turns its start with the launcher off, and starts it in the cloud (`run`).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_move(
    app: AppHandle,
    state: State<'_, AppState>,
    desktop_id: String,
    cloud_id: String,
    rev: u64,
    run: bool,
) -> CmdResult<CloudMoveResult> {
    let token = token(&state)?;
    let cloud = service(&app, &state);
    let (account, commands) = desktop_parts(&state, &desktop_id).await?;
    let owner = owner_name(&state);
    let plan = cloud.move_plan(&account, &commands, &owner).await;
    if plan.server.is_none() {
        return Err(CmdError::new(
            "BULUT-ADRES",
            pick(
                "Bulut hesapları bu sunucu adresine bağlanamaz.",
                "Cloud accounts can't connect to this server address.",
            ),
            format!("{:?}", plan.server_problem),
        ));
    }
    let outcome = cloud.move_to(&token, &cloud_id, rev, &plan).await?;
    let mut result = CloudMoveResult {
        outcome,
        stopped_here: false,
        started: false,
    };
    if !matches!(result.outcome, CloudSaveOutcome::Saved { .. }) {
        return Ok(result);
    }
    if state.afk.snapshot(&desktop_id).state.is_active() {
        state.afk.disconnect(&desktop_id);
        result.stopped_here = true;
    }
    if account.auto_start {
        let store = state.afk.store().clone();
        let mut changed = account.clone();
        changed.auto_start = false;
        let saved = tauri::async_runtime::spawn_blocking(move || store.update_account(changed))
            .await
            .map_err(CmdError::internal)?;
        if let Err(error) = saved {
            tracing::warn!(%error, "the moved AFK account still starts with the launcher");
        }
    }
    if run {
        match cloud.start(&token, &cloud_id).await {
            Ok(started) => {
                result.started = true;
                result.outcome = CloudSaveOutcome::Saved { account: started };
            }
            Err(error) => tracing::info!(%error, "the moved cloud account did not start"),
        }
    }
    Ok(result)
}

/// "Tümünü bağla" while some accounts run in the cloud: every other desktop account connects
/// (staggered, as usual). Returns how many were left to the cloud.
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_connect_here(app: AppHandle, state: State<'_, AppState>) -> u32 {
    let cloud = service(&app, &state);
    let mut skipped = 0;
    let ids: Vec<String> = state
        .afk
        .accounts()
        .into_iter()
        .filter(|desktop| {
            let away = cloud
                .twin(&desktop.nickname, &desktop_server(desktop))
                .is_some_and(|twin| twin.wants_to_run());
            if away {
                skipped += 1;
            }
            !away
        })
        .map(|desktop| desktop.id)
        .collect();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;
        let state = app.state::<AppState>();
        if let Err(error) = state.afk.connect_staggered(&ids).await {
            tracing::warn!(%error, "AFK connect-all failed");
        }
    });
    skipped
}

/// Which page of afk.projects.gg to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloudPage {
    /// The site's front page ("afk.projects.gg'de başla").
    Start,
    /// Buying a new account.
    Buy,
    /// Renewing or reactivating `account`.
    Renew,
    /// The account's own page.
    Account,
    /// The player's accounts.
    Accounts,
    /// AFK+'s page ("AFK+ ne yapar?").
    Plus,
    /// Upgrading `account` to AFK+, or buying a new AFK+ account without one.
    PlusBuy,
    /// The AFK+ tab of `account` on the site (what the launcher leaves to the site: plan changes).
    PlusAccount,
}

/// Opens afk.projects.gg in the browser: purchase, renewal and linking always happen there.
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_open(
    app: AppHandle,
    state: State<'_, AppState>,
    page: CloudPage,
    account: Option<String>,
) -> CmdResult<()> {
    let client = service(&app, &state).client();
    let id = account.filter(|id| {
        !id.is_empty()
            && id.len() <= 32
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    });
    let path = match (page, id) {
        (CloudPage::Start, _) => String::new(),
        (CloudPage::Buy, _) => "satin-al".to_owned(),
        (CloudPage::Renew, Some(id)) => format!("satin-al?hesap={id}"),
        (CloudPage::Account, Some(id)) => format!("hesap/{id}"),
        (CloudPage::Accounts, _)
        | (CloudPage::Renew | CloudPage::Account | CloudPage::PlusAccount, None) => {
            "hesaplar".to_owned()
        }
        (CloudPage::Plus, _) => "afk-plus".to_owned(),
        (CloudPage::PlusBuy, Some(id)) => format!("satin-al?hesap={id}&tier=plus"),
        (CloudPage::PlusBuy, None) => "satin-al?tier=plus".to_owned(),
        (CloudPage::PlusAccount, Some(id)) => format!("hesap/{id}/afk-plus"),
    };
    let url = client.site_url(&path);
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|error| {
            CmdError::new(
                "BULUT-AC",
                pick(
                    "afk.projects.gg açılamadı. Tarayıcında kendin açabilirsin.",
                    "Couldn't open afk.projects.gg. You can open it in your browser yourself.",
                ),
                error.to_string(),
            )
        })
}

/// Signed out of the friends service: the words of the rows this launcher sealed leave the
/// keychain, the console sockets close.
pub async fn signed_out(app: &AppHandle, state: &AppState) {
    service(app, state).forget().await;
}

fn text_problem_message(reason: &str) -> &'static str {
    match reason {
        "length" => pick(
            "Satır en çok 256 karakter olabilir.",
            "A line has up to 256 characters.",
        ),
        "control" => pick(
            "Satır sonu ve görünmeyen karakter kullanma.",
            "Don't use line breaks or invisible characters.",
        ),
        "backslash" => pick(
            "Satır ters bölü (\\) ile başlayamaz.",
            "A line can't start with a backslash (\\).",
        ),
        "variable" => pick(
            "Yalnız %username%, %serverip%, %serverport% ve %datetime% kullanılabilir.",
            "Only %username%, %serverip%, %serverport% and %datetime% can be used.",
        ),
        "space" => pick(
            "Satırın başında ya da sonunda boşluk olamaz.",
            "A line can't start or end with a space.",
        ),
        _ => pick("Bu satır gönderilemez.", "This line can't be sent."),
    }
}

impl From<CloudError> for CmdError {
    /// 7/24 Bulut failures: a short sentence; the code stays in the details.
    fn from(error: CloudError) -> Self {
        let details = match &error {
            CloudError::Refused { status, code, .. } => format!("HTTP {status} {code}"),
            other => other.to_string(),
        };
        let lang = Lang::current();
        let (code, message): (&str, String) = match &error {
            CloudError::SignedOut | CloudError::Unauthorized => (
                "BULUT-OTURUM",
                pick(
                    "Bulut hesapların için Arkadaşlar'da Discord ile yeniden giriş yap.",
                    "Sign in to Friends with Discord again to see your cloud accounts.",
                )
                .into(),
            ),
            CloudError::Network(_) => (
                "BULUT-AG",
                pick(
                    "7/24 Bulut'a ulaşılamadı. İnternet bağlantını kontrol et.",
                    "Couldn't reach 24/7 Cloud. Check your internet connection.",
                )
                .into(),
            ),
            CloudError::Unavailable { .. } | CloudError::Protocol(_) => (
                "BULUT-YOK",
                pick(
                    "7/24 Bulut'a şu an ulaşılamıyor. Biraz sonra tekrar dene.",
                    "24/7 Cloud isn't reachable right now. Try again in a bit.",
                )
                .into(),
            ),
            CloudError::RateLimited { .. } => (
                "BULUT-SINIR",
                pick(
                    "Bunu çok sık denedin. Biraz sonra tekrar dene.",
                    "You tried that too often. Try again in a bit.",
                )
                .into(),
            ),
            CloudError::Seal(_) => (
                "BULUT-MUHUR",
                pick(
                    "Gizli satır şifrelenemedi. Biraz sonra tekrar dene.",
                    "Couldn't encrypt the secret line. Try again in a bit.",
                )
                .into(),
            ),
            CloudError::Keychain(_) => (
                "BULUT-ANAHTAR",
                pick(
                    "Bilgisayarın güvenli anahtar deposuna ulaşılamadı. Tekrar dene.",
                    "Couldn't reach your computer's secure storage. Try again.",
                )
                .into(),
            ),
            CloudError::Invalid("console line") => (
                "BULUT-SATIR",
                pick("Bu satır gönderilemez.", "This line can't be sent.").into(),
            ),
            CloudError::Invalid("no machine yet") => (
                "BULUT-YER",
                pick(
                    "Bu hesap henüz bir AFK sunucusuna yerleşmedi.",
                    "This account doesn't have an AFK server yet.",
                )
                .into(),
            ),
            CloudError::Invalid(_) => (
                "BULUT",
                pick(
                    "Bir sorun oluştu. Biraz sonra tekrar dene.",
                    "Something went wrong. Try again in a bit.",
                )
                .into(),
            ),
            CloudError::Refused { code, .. } => match code.as_str() {
                "host_offline" => (
                    "BULUT-SUNUCU",
                    pick(
                        "AFK sunucusu şu an bağlı değil. Hesap çalışmaya devam ediyor olabilir.",
                        "The AFK server isn't connected right now. The account may still be running.",
                    )
                    .into(),
                ),
                "not_running" => (
                    "BULUT-OYUNDA-DEGIL",
                    pick(
                        "Hesap şu an oyunda değil.",
                        "The account isn't in the game right now.",
                    )
                    .into(),
                ),
                "service_paused" => (
                    "BULUT-BAKIM",
                    pick(
                        "7/24 Bulut bakımda. Bu, bakım bitince yapılabilir.",
                        "24/7 Cloud is under maintenance. This can wait until it's over.",
                    )
                    .into(),
                ),
                "account_suspended" => (
                    "BULUT-ASKIDA",
                    pick(
                        "Bu hesabı yetkililer durdurdu.",
                        "Staff stopped this account.",
                    )
                    .into(),
                ),
                "player_restricted" => (
                    "BULUT-UZAK",
                    pick(
                        "7/24 Bulut'tan uzaklaştırıldın.",
                        "You're banned from 24/7 Cloud.",
                    )
                    .into(),
                ),
                "account_expired" => (
                    "BULUT-SURE",
                    pick(
                        "Bu hesabın süresi doldu. afk.projects.gg'de yeniden etkinleştirebilirsin.",
                        "This account's time is up. You can reactivate it on afk.projects.gg.",
                    )
                    .into(),
                ),
                "server_blocked" => {
                    let reason = error
                        .extra("reason")
                        .and_then(|reason| {
                            let text = reason.get(lang.pick("tr", "en"))?.as_str()?.trim();
                            (!text.is_empty()).then(|| text.to_owned())
                        });
                    (
                        "BULUT-ENGEL",
                        match reason {
                            Some(reason) => lang.pick(
                                format!("Bu sunucuya giriş engelli: {reason}"),
                                format!("This server is blocked: {reason}"),
                            ),
                            None => pick(
                                "Bu sunucuya giriş engelli.",
                                "This server is blocked.",
                            )
                            .into(),
                        },
                    )
                }
                "server_full" => (
                    "BULUT-DOLU",
                    pick(
                        "Bu sunucuda bulut hesapları için yer kalmadı.",
                        "This server has no room for cloud accounts right now.",
                    )
                    .into(),
                ),
                "duplicate_account" => (
                    "BULUT-IKIZ",
                    pick(
                        "Aynı ad ve sunucuyla başka bir bulut hesabın var. İkisi birbirini atar.",
                        "Another of your cloud accounts has this name on this server. They'd keep kicking each other.",
                    )
                    .into(),
                ),
                "server_change_limit" => (
                    "BULUT-SUNUCU-DEGISTI",
                    pick(
                        "Bu saat içinde sunucu değiştirme hakkın doldu. Biraz sonra tekrar dene.",
                        "You've changed the server too often this hour. Try again later.",
                    )
                    .into(),
                ),
                "account_not_found" | "not_found" => (
                    "BULUT-HESAP-YOK",
                    pick(
                        "Bu bulut hesabı artık yok.",
                        "This cloud account is gone.",
                    )
                    .into(),
                ),
                "rev_conflict" => (
                    "BULUT-DEGISTI",
                    pick(
                        "Hesap başka bir yerde değişti. Son halini yükleyip tekrar dene.",
                        "The account changed somewhere else. Load the latest and try again.",
                    )
                    .into(),
                ),
                "invalid_command" => (
                    "BULUT-SATIR",
                    text_problem_message(error.extra_str("reason").unwrap_or("")).into(),
                ),
                "invalid_seal" | "secret_unreadable" => (
                    "BULUT-MUHUR",
                    pick(
                        "Gizli satır okunamadı. Yeniden yaz.",
                        "The secret line can't be read. Type it again.",
                    )
                    .into(),
                ),
                "plus_off" | "feature_off" => (
                    "AFKPLUS-KAPALI",
                    pick(
                        "AFK+ şu an kullanılamıyor.",
                        "AFK+ isn't available right now.",
                    )
                    .into(),
                ),
                "plus_not_active" => (
                    "AFKPLUS-YOK",
                    pick(
                        "Bu hesapta AFK+ yok.",
                        "This account doesn't have AFK+.",
                    )
                    .into(),
                ),
                "plus_projects_only" => (
                    "AFKPLUS-PROJECTS",
                    pick(
                        "AFK+ yalnız Projects'te çalışır.",
                        "AFK+ only works on Projects.",
                    )
                    .into(),
                ),
                "plus_restricted" => (
                    "AFKPLUS-UZAK",
                    pick(
                        "AFK+'dan uzaklaştırıldın.",
                        "You're banned from AFK+.",
                    )
                    .into(),
                ),
                "approval_closed" | "approval_not_found" => (
                    "AFKPLUS-ONAY",
                    pick(
                        "Bu onay artık beklemiyor.",
                        "This approval isn't waiting any more.",
                    )
                    .into(),
                ),
                "history_not_found" => (
                    "AFKPLUS-GECMIS",
                    pick(
                        "Bu eski hal artık kayıtlı değil.",
                        "This earlier version isn't saved any more.",
                    )
                    .into(),
                ),
                _ => (
                    "BULUT",
                    pick(
                        "Bir sorun oluştu. Biraz sonra tekrar dene.",
                        "Something went wrong. Try again in a bit.",
                    )
                    .into(),
                ),
            },
        };
        Self::new(code, message, details)
    }
}
