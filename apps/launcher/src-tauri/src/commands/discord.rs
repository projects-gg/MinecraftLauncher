use std::sync::Mutex;
use std::time::{Duration, Instant};

use pl_discord::{Activity, GamePlace, PlacePrivacy, ShownPlace, StatusDisplayType, shown_place};
use pl_lang::Lang;
use pl_social::LocalStatus;
use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

use crate::state::{AppState, DISCORD_CLIENT_ID};

/// What the player is doing, sent by the UI whenever it changes. The backend turns it into a
/// Rich Presence activity; where the game is comes from the game's log (OVL-RPC), so a server
/// change shows while the launcher's page rests in the tray.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PresenceContext {
    /// "home" | "library" | "news" | "afk" | "settings" | "downloading" | "starting" (the game's
    /// window is not on screen yet) | "playing"
    pub activity: String,
    pub instance_name: Option<String>,
    pub mc_version: Option<String>,
    pub loader: Option<String>,
    /// Projects' player count, shown only while the game is on Projects.
    pub players: Option<u32>,
    pub max_players: Option<u32>,
    pub percent: Option<u32>,
    pub started_at_ms: Option<u64>,
    /// "Bağlandığım Sunucuyu Göster": where the game is (Projects, singleplayer, a server).
    pub show_server: bool,
    /// "Sunucu Adresini Göster": another server's address too (Projects' is always public).
    pub show_address: bool,
}

const LOGO: &str = "logo";
/// OVL-RPC: a generic Minecraft block, the picture on singleplayer and on servers that are not
/// Projects' (never the server's own icon). An asset of the Discord application, uploaded in its
/// developer portal.
const BLOCK: &str = "block";

/// The last context the UI sent, so a language change or a move in the game says the same thing
/// anew.
static LAST: Mutex<Option<PresenceContext>> = Mutex::new(None);

/// The interface's language (the player's Discord friends read it), `shown` the place as the
/// player lets it show, `block` whether the block picture may stand for it.
fn build(ctx: &PresenceContext, shown: ShownPlace<'_>, lang: Lang, block: bool) -> Activity {
    let version_line = match (&ctx.loader, &ctx.mc_version) {
        (Some(loader), Some(version)) if loader != "vanilla" => format!("{loader} {version}"),
        (_, Some(version)) => format!("Minecraft {version}"),
        _ => "Minecraft".to_owned(),
    };
    let in_launcher = lang.pick("Launcher'da", "In the launcher");
    let base = Activity::new()
        .large_image(LOGO)
        .large_text("Projects Launcher")
        .button(
            lang.pick("Launcher'ı indir", "Get the launcher"),
            "https://projects.gg/indir",
        )
        .button(
            lang.pick("Discord'a katıl", "Join the Discord"),
            "https://projects.gg/discord",
        );

    match ctx.activity.as_str() {
        "playing" => {
            let details = match shown {
                ShownPlace::Minecraft => lang.pick("Minecraft oynuyor", "Playing Minecraft"),
                ShownPlace::Singleplayer => {
                    lang.pick("Tek oyunculu oynuyor", "Playing singleplayer")
                }
                ShownPlace::Projects { .. } => {
                    lang.pick("Projects'te oynuyor", "Playing on Projects")
                }
                ShownPlace::Server { .. } => {
                    lang.pick("Bir sunucuda oynuyor", "Playing on a server")
                }
            };
            // The address beside the picture; the version then on the picture's hover.
            let address = match shown {
                ShownPlace::Projects { address }
                | ShownPlace::Server {
                    address: Some(address),
                } => Some(address),
                _ => None,
            };
            let mut activity = base
                .details(details)
                .state(address.unwrap_or(version_line.as_str()))
                .status_display(StatusDisplayType::Details);
            if block && shown.generic_picture() {
                activity = activity
                    .large_image(BLOCK)
                    .large_text(version_line.clone())
                    .small_image(LOGO)
                    .small_text("Projects Launcher");
            } else if address.is_some() {
                activity = activity.large_text(version_line.clone());
            }
            if let Some(started) = ctx.started_at_ms {
                activity = activity.start_timestamp(started);
            }
            if let (Some(players), Some(max), ShownPlace::Projects { .. }) =
                (ctx.players, ctx.max_players, shown)
            {
                activity = activity.party("projects-server", players, max);
            }
            activity
        }
        "downloading" => {
            let percent = ctx.percent.unwrap_or(0);
            base.details(lang.pick("Oyunu hazırlıyor", "Getting the game ready"))
                .state(lang.pick(format!("%{percent}"), format!("{percent}%")))
        }
        // "Playing" only once the game's window is on screen, as in the launcher.
        "starting" => base
            .details(lang.pick("Oyunu açıyor", "Starting the game"))
            .state(version_line),
        "afk" => base
            .details(lang.pick("AFK hesaplarını yönetiyor", "Managing AFK accounts"))
            .state(in_launcher),
        "library" => base
            .details(lang.pick("Kütüphanede", "In the library"))
            .state(match &ctx.instance_name {
                Some(name) => lang.pick(
                    format!("{name} kurulumunu inceliyor"),
                    format!("Looking at {name}"),
                ),
                None => lang
                    .pick("Kurulumlarına bakıyor", "Looking at their instances")
                    .to_owned(),
            }),
        "news" => base
            .details(lang.pick("Haberleri okuyor", "Reading the news"))
            .state(in_launcher),
        _ => base
            .details(in_launcher)
            .state(lang.pick("Ana sayfada", "On the home page")),
    }
}

/// Where the game is, from the friends service's reading of the game's log.
fn place_of(status: &LocalStatus) -> GamePlace<'_> {
    match status {
        LocalStatus::Playing {
            host: Some(address),
        } if pl_social::is_projects_host(address) => GamePlace::Projects { address },
        LocalStatus::Playing {
            host: Some(address),
        } => GamePlace::Server { address },
        LocalStatus::Singleplayer => GamePlace::Singleplayer,
        LocalStatus::Launcher | LocalStatus::Playing { host: None } => GamePlace::Menus,
    }
}

/// Shows `ctx` as things are now.
fn show(app: &AppHandle, state: &AppState, ctx: &PresenceContext) {
    let status = state.social.game_place();
    let shown = shown_place(
        place_of(&status),
        PlacePrivacy {
            show_place: ctx.show_server,
            show_address: ctx.show_address,
        },
    );
    let block = ctx.activity == "playing" && shown.generic_picture() && block_ready(app);
    state
        .presence
        .set(Some(build(ctx, shown, Lang::current(), block)));
}

#[tauri::command]
#[specta::specta]
pub fn discord_update(app: AppHandle, state: State<'_, AppState>, context: PresenceContext) {
    show(&app, &state, &context);
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(context);
}

/// The same context anew: the interface's language changed, the game moved (another server,
/// singleplayer, the menus), or the block picture turned out to be there.
pub fn refresh(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let last = LAST.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(context) = last {
        show(app, &state, &context);
    }
}

#[tauri::command]
#[specta::specta]
pub fn discord_set_enabled(state: State<'_, AppState>, enabled: bool) {
    state.presence.set_enabled(enabled);
}

// --- the block picture -------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum BlockAsset {
    Unknown,
    Checking,
    Present,
    Absent(Instant),
}

static BLOCK_ASSET: Mutex<BlockAsset> = Mutex::new(BlockAsset::Unknown);

/// A miss is looked at again after this long (the asset may have been uploaded meanwhile).
const BLOCK_RECHECK: Duration = Duration::from_secs(60 * 60);
const BLOCK_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether the application has the [`BLOCK`] asset. Discord shows an empty square for a key it
/// does not know, so until the asset is there the logo stays. Discord's public list of the
/// application's assets says so; it is read only when an activity would show the block, once a
/// run (again an hour after a miss), off the caller's thread: this call says false meanwhile and
/// the activity is shown again once the asset is found.
fn block_ready(app: &AppHandle) -> bool {
    {
        let mut asset = BLOCK_ASSET.lock().unwrap_or_else(|e| e.into_inner());
        match *asset {
            BlockAsset::Present => return true,
            BlockAsset::Checking => return false,
            BlockAsset::Absent(at) if at.elapsed() < BLOCK_RECHECK => return false,
            BlockAsset::Unknown | BlockAsset::Absent(_) => {}
        }
        *asset = BlockAsset::Checking;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let present = has_block_asset(&app).await;
        *BLOCK_ASSET.lock().unwrap_or_else(|e| e.into_inner()) = if present {
            BlockAsset::Present
        } else {
            BlockAsset::Absent(Instant::now())
        };
        if present {
            refresh(&app);
        }
    });
    false
}

#[derive(Deserialize)]
struct AssetEntry {
    name: String,
}

async fn has_block_asset(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<AppState>() else {
        return false;
    };
    let url = format!("https://discord.com/api/v10/oauth2/applications/{DISCORD_CLIENT_ID}/assets");
    let response = state
        .http
        .client()
        .get(url)
        .timeout(BLOCK_CHECK_TIMEOUT)
        .send()
        .await;
    let body = match response {
        Ok(response) if response.status().is_success() => response.bytes().await,
        Ok(response) => {
            tracing::info!(status = %response.status(), "Discord's asset list did not answer");
            return false;
        }
        Err(error) => {
            tracing::info!(%error, "Discord's asset list could not be read");
            return false;
        }
    };
    let present = body
        .ok()
        .and_then(|body| serde_json::from_slice::<Vec<AssetEntry>>(&body).ok())
        .is_some_and(|assets| assets.iter().any(|asset| asset.name == BLOCK));
    if !present {
        tracing::info!("the Discord application has no block picture yet; the logo stays");
    }
    present
}
