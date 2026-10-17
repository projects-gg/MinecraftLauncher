use std::time::Duration;

use pl_mc::server::{PingOptions, ping_server};
use serde::Serialize;
use tauri::Manager;

use crate::state::AppState;

fn png_data_url(bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// Featured server shown in the title bar until the content config names another one.
pub const DEFAULT_SERVER: (&str, &str) = ("Projects", "play.projects.gg");

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatusView {
    pub name: String,
    pub address: String,
    pub online: bool,
    pub players: u32,
    pub max_players: u32,
    pub latency_ms: Option<u64>,
    pub motd: Option<String>,
    /// The server's own icon from the ping (a PNG data URL), when it sends one.
    pub icon_url: Option<String>,
}

/// The signed config's featured server (else its first one) as name and `host[:port]`, from the
/// content cache (no network).
async fn configured_server(app: &tauri::AppHandle) -> Option<(String, String)> {
    let state = app.try_state::<AppState>()?;
    let snapshot = state.content.load_cached().await.ok()?;
    let servers = &snapshot.config.servers;
    let server = servers
        .iter()
        .find(|s| s.featured)
        .or_else(|| servers.first())?;
    let host = server.address.trim();
    if host.is_empty() {
        return None;
    }
    let address = match server.port {
        Some(port) if !host.contains(':') => format!("{host}:{port}"),
        _ => host.to_owned(),
    };
    let name = Some(server.name.trim())
        .filter(|n| !n.is_empty())
        .unwrap_or(host)
        .to_owned();
    Some((name, address))
}

/// Server List Ping. Never fails: an unreachable server is reported as offline. Without a name
/// and address it pings the featured server Studio published (else `play.projects.gg`).
#[tauri::command]
#[specta::specta]
pub async fn server_status(
    app: tauri::AppHandle,
    name: Option<String>,
    address: Option<String>,
) -> ServerStatusView {
    let (name, address) = match (name, address) {
        (name, Some(address)) => (name.unwrap_or_else(|| DEFAULT_SERVER.0.to_owned()), address),
        (name, None) => match configured_server(&app).await {
            Some((featured, address)) => (name.unwrap_or(featured), address),
            None => (
                name.unwrap_or_else(|| DEFAULT_SERVER.0.to_owned()),
                DEFAULT_SERVER.1.to_owned(),
            ),
        },
    };
    let options = PingOptions {
        timeout: Duration::from_secs(4),
        ..PingOptions::default()
    };
    match ping_server(&address, &options).await {
        Ok(ping) => {
            let players = ping.players.as_ref();
            ServerStatusView {
                name,
                address,
                online: true,
                players: players.map(|p| p.online.max(0) as u32).unwrap_or(0),
                max_players: players.map(|p| p.max.max(0) as u32).unwrap_or(0),
                latency_ms: Some(ping.latency_ms),
                motd: Some(ping.motd.plain),
                icon_url: ping.favicon_png.as_deref().map(png_data_url),
            }
        }
        Err(error) => {
            tracing::debug!(%error, %address, "server ping failed");
            ServerStatusView {
                name,
                address,
                online: false,
                players: 0,
                max_players: 0,
                latency_ms: None,
                motd: None,
                icon_url: None,
            }
        }
    }
}
