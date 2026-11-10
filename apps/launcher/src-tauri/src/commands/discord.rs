use std::sync::Mutex;
use std::time::{Duration, Instant};

use pl_discord::{
    Activity, CrewNow, CrewPrivacy, GamePlace, PlacePrivacy, ShownCrew, ShownPlace,
    StatusDisplayType, shown_crew, shown_place,
};
use pl_lang::Lang;
use pl_social::{CrewTogether, LocalStatus, SocialUpdate};
use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

use crate::state::{AppState, DISCORD_CLIENT_ID};

/// What the player is doing and what they let Discord show, sent by the UI whenever it changes.
/// The backend turns it into a Rich Presence activity; where the game is comes from the game's
/// log (OVL-RPC) and the crew from the friends service (RPC-PLUS), so both follow the game while
/// the launcher's page rests in the tray.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PresenceContext {
    /// "home" | "library" | "news" | "afk" | "settings" | "downloading" | "starting" (the game's
    /// window is not on screen yet) | "playing"
    pub activity: String,
    /// The pack's name, only for an instance the launcher keeps from a pack (the official
    /// Projects pack); a name the player gave their own instance never goes to Discord.
    pub pack_name: Option<String>,
    pub mc_version: Option<String>,
    pub loader: Option<String>,
    /// Projects' player count, shown only while the game is on Projects.
    pub players: Option<u32>,
    pub max_players: Option<u32>,
    pub percent: Option<u32>,
    pub started_at_ms: Option<u64>,
    /// "Oynadığım Yeri Göster": where the game is (Projects, singleplayer, a server).
    pub show_server: bool,
    /// "Sunucu Adresini Göster": another server's address too (Projects' is always public).
    pub show_address: bool,
    /// "Ekibimle Oynadığımı Göster" (RPC-PLUS).
    pub show_crew: bool,
    /// "Kaç Kişi Olduğumuzu Göster": the crewmates together, of the crew's size.
    pub show_crew_count: bool,
    /// "Ekibimin Adını Göster": the name of the crew whose tag the player wears.
    pub show_crew_name: bool,
    /// "Paketi ve Sürümü Göster".
    pub show_pack: bool,
    /// "Oyun Süresini Göster".
    pub show_elapsed: bool,
}

const LOGO: &str = "logo";
/// OVL-RPC: a generic Minecraft block, the picture on singleplayer and on servers that are not
/// Projects' (never the server's own icon). An asset of the Discord application, uploaded in its
/// developer portal.
const BLOCK: &str = "discord-block";

/// The last context the UI sent, so a language change or a move in the game says the same thing
/// anew.
static LAST: Mutex<Option<PresenceContext>> = Mutex::new(None);

/// RPC-PLUS: the crew the player plays with now, from the friends service's last view.
static CREW: Mutex<Option<CrewTogether>> = Mutex::new(None);

/// The interface's language (the player's Discord friends read it), `shown` the place as the
/// player lets it show, `crew` the crew line, `block` whether the block picture may stand for it.
fn build(
    ctx: &PresenceContext,
    shown: ShownPlace<'_>,
    crew: Option<ShownCrew<'_>>,
    lang: Lang,
    block: bool,
) -> Activity {
    let version_line = match (&ctx.loader, &ctx.mc_version) {
        (Some(loader), Some(version)) if loader != "vanilla" => format!("{loader} {version}"),
        (_, Some(version)) => format!("Minecraft {version}"),
        _ => "Minecraft".to_owned(),
    };
    // "Projects · Fabric 26.1.2" for the official pack, else the loader and version.
    let pack_line = match &ctx.pack_name {
        Some(name) => format!("{name} · {version_line}"),
        None => version_line,
    };
    let pack = ctx.show_pack.then_some(pack_line.as_str());
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
            let address = match shown {
                ShownPlace::Projects { address }
                | ShownPlace::Server {
                    address: Some(address),
                } => Some(address),
                _ => None,
            };
            // The second line: the crew, else the address beside the picture, else the pack.
            let mut party = None;
            let second = if let Some(crew) = crew {
                party = crew
                    .party
                    .map(|(together, size)| ("projects-crew", together, size));
                Some(match crew.name {
                    Some(name) => {
                        lang.pick(format!("{name} ekibiyle"), format!("With the {name} crew"))
                    }
                    None => lang.pick("Ekibiyle", "With their crew").to_owned(),
                })
            } else if let Some(address) = address {
                if let (Some(players), Some(max), ShownPlace::Projects { .. }) =
                    (ctx.players, ctx.max_players, shown)
                {
                    party = Some(("projects-server", players, max));
                }
                Some(address.to_owned())
            } else {
                pack.map(str::to_owned)
            };
            let second_is_pack = second.as_deref() == pack;
            let mut activity = base
                .details(details)
                .status_display(StatusDisplayType::Details);
            if let Some(second) = second {
                activity = activity.state(second);
            }
            // The pack on the picture's hover when the second line says something else.
            let hover = pack.filter(|_| !second_is_pack);
            if block && shown.generic_picture() {
                activity = activity
                    .large_image(BLOCK)
                    .large_text(hover.unwrap_or("Minecraft"))
                    .small_image(LOGO)
                    .small_text("Projects Launcher");
            } else if let Some(hover) = hover {
                activity = activity.large_text(hover);
            }
            if let (Some(started), true) = (ctx.started_at_ms, ctx.show_elapsed) {
                activity = activity.start_timestamp(started);
            }
            if let Some((id, current, max)) = party {
                activity = activity.party(id, current, max);
            }
            activity
        }
        "downloading" => {
            let percent = ctx.percent.unwrap_or(0);
            base.details(lang.pick("Oyunu hazırlıyor", "Getting the game ready"))
                .state(lang.pick(format!("%{percent}"), format!("{percent}%")))
        }
        // "Playing" only once the game's window is on screen, as in the launcher.
        "starting" => {
            let activity = base.details(lang.pick("Oyunu açıyor", "Starting the game"));
            match pack {
                Some(pack) => activity.state(pack),
                None => activity,
            }
        }
        "afk" => base
            .details(lang.pick("AFK hesaplarını yönetiyor", "Managing AFK accounts"))
            .state(in_launcher),
        "library" => base
            .details(lang.pick("Kütüphanede", "In the library"))
            .state(match &ctx.pack_name {
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
    let crew_now = CREW.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let crew = shown_crew(
        crew_now.as_ref().map(|crew| CrewNow {
            name: crew.name.as_deref(),
            together: crew.together,
            size: crew.size,
        }),
        CrewPrivacy {
            show_crew: ctx.show_crew,
            show_count: ctx.show_crew_count,
            show_name: ctx.show_crew_name,
        },
    );
    let block = ctx.activity == "playing" && shown.generic_picture() && block_ready(app);
    state
        .presence
        .set(Some(build(ctx, shown, crew, Lang::current(), block)));
}

#[tauri::command]
#[specta::specta]
pub fn discord_update(app: AppHandle, state: State<'_, AppState>, context: PresenceContext) {
    show(&app, &state, &context);
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(context);
}

/// The same context anew: the interface's language changed, the game moved (another server,
/// singleplayer, the menus), the crew line changed, or the block picture turned out to be there.
pub fn refresh(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let last = LAST.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(context) = last {
        show(app, &state, &context);
    }
}

/// RPC-PLUS: every view of the friends service; the activity is shown anew only when the crew
/// line's facts changed (crewmates coming and going on Projects, the worn tag).
pub fn on_social(app: &AppHandle, update: &SocialUpdate) {
    let SocialUpdate::View(view) = update else {
        return;
    };
    let now = view.crew_together();
    {
        let mut crew = CREW.lock().unwrap_or_else(|e| e.into_inner());
        if *crew == now {
            return;
        }
        *crew = now;
    }
    refresh(app);
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
