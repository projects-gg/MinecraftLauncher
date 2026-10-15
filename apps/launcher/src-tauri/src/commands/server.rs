use std::time::Duration;

use pl_mc::server::{PingOptions, ping_server};
use serde::Serialize;

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
}

/// Server List Ping. Never fails: an unreachable server is reported as offline.
#[tauri::command]
#[specta::specta]
pub async fn server_status(name: Option<String>, address: Option<String>) -> ServerStatusView {
    let name = name.unwrap_or_else(|| DEFAULT_SERVER.0.to_owned());
    let address = address.unwrap_or_else(|| DEFAULT_SERVER.1.to_owned());
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
            }
        }
    }
}
