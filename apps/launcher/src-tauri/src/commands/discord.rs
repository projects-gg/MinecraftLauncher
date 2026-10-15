use pl_discord::{Activity, StatusDisplayType};
use serde::Deserialize;
use tauri::State;

use crate::state::AppState;

/// What the player is doing, sent by the UI whenever it changes. The backend turns it into a
/// Rich Presence activity (templates from the content config come later).
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PresenceContext {
    /// "home" | "library" | "news" | "afk" | "settings" | "downloading" | "playing"
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

fn build(ctx: &PresenceContext) -> Activity {
    let version_line = match (&ctx.loader, &ctx.mc_version) {
        (Some(loader), Some(version)) if loader != "vanilla" => format!("{loader} {version}"),
        (_, Some(version)) => format!("Minecraft {version}"),
        _ => "Minecraft".to_owned(),
    };
    let base = Activity::new()
        .large_image(LOGO)
        .large_text("Projects Launcher")
        .button("Launcher'ı indir", "https://projects.gg/indir")
        .button("Discord'a katıl", "https://projects.gg/discord");

    match ctx.activity.as_str() {
        "playing" => {
            let details = match (&ctx.server, ctx.show_server) {
                (Some(server), true) if server.contains("projects.gg") => {
                    "Projects'te oynuyor".to_owned()
                }
                (Some(server), true) => format!("{server} sunucusunda"),
                _ => "Minecraft oynuyor".to_owned(),
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
        "downloading" => base
            .details("Oyunu hazırlıyor")
            .state(format!("%{}", ctx.percent.unwrap_or(0))),
        "afk" => base
            .details("AFK hesaplarını yönetiyor")
            .state("Launcher'da"),
        "library" => base.details("Kütüphanede").state(
            ctx.instance_name
                .clone()
                .map(|name| format!("{name} kurulumunu inceliyor"))
                .unwrap_or_else(|| "Kurulumlarına bakıyor".to_owned()),
        ),
        "news" => base.details("Haberleri okuyor").state("Launcher'da"),
        _ => base.details("Launcher'da").state("Ana sayfada"),
    }
}

#[tauri::command]
#[specta::specta]
pub fn discord_update(state: State<'_, AppState>, context: PresenceContext) {
    state.presence.set(Some(build(&context)));
}

#[tauri::command]
#[specta::specta]
pub fn discord_set_enabled(state: State<'_, AppState>, enabled: bool) {
    state.presence.set_enabled(enabled);
}
