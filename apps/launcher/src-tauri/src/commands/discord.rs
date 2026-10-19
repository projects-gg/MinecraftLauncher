use std::sync::Mutex;

use pl_discord::{Activity, StatusDisplayType};
use pl_lang::Lang;
use serde::Deserialize;
use tauri::{Manager, State};

use crate::state::AppState;

/// What the player is doing, sent by the UI whenever it changes. The backend turns it into a
/// Rich Presence activity (templates from the content config come later).
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PresenceContext {
    /// "home" | "library" | "news" | "afk" | "settings" | "downloading" | "starting" (the game's
    /// window is not on screen yet) | "playing"
    pub activity: String,
    pub instance_name: Option<String>,
    pub mc_version: Option<String>,
    pub loader: Option<String>,
    pub server: Option<String>,
    pub players: Option<u32>,
    pub max_players: Option<u32>,
    pub percent: Option<u32>,
    pub started_at_ms: Option<u64>,
    pub show_server: bool,
}

const LOGO: &str = "logo";

/// The last context the UI sent, so a language change can say the same thing in the new
/// language.
static LAST: Mutex<Option<PresenceContext>> = Mutex::new(None);

/// The activity in the interface's language (the player's Discord friends read it).
fn build(ctx: &PresenceContext, lang: Lang) -> Activity {
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
            let details = match (&ctx.server, ctx.show_server) {
                (Some(server), true) if server.contains("projects.gg") => lang
                    .pick("Projects'te oynuyor", "Playing on Projects")
                    .to_owned(),
                (Some(server), true) => lang.pick(
                    format!("{server} sunucusunda"),
                    format!("Playing on {server}"),
                ),
                _ => lang
                    .pick("Minecraft oynuyor", "Playing Minecraft")
                    .to_owned(),
            };
            let mut activity = base
                .details(details)
                .state(version_line)
                .status_display(StatusDisplayType::Details);
            if let Some(started) = ctx.started_at_ms {
                activity = activity.start_timestamp(started);
            }
            if let (Some(players), Some(max), true) =
                (ctx.players, ctx.max_players, ctx.show_server)
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

#[tauri::command]
#[specta::specta]
pub fn discord_update(state: State<'_, AppState>, context: PresenceContext) {
    state.presence.set(Some(build(&context, Lang::current())));
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(context);
}

/// The interface's language changed: the presence says the same thing in the new language.
pub fn refresh_language(app: &tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if let Some(context) = LAST.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        state.presence.set(Some(build(context, Lang::current())));
    }
}

#[tauri::command]
#[specta::specta]
pub fn discord_set_enabled(state: State<'_, AppState>, enabled: bool) {
    state.presence.set_enabled(enabled);
}
