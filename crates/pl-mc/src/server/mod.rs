//! Multiplayer servers: Server List Ping (status, MOTD, players, favicon, latency), the
//! `servers.dat` server list, and chat-component/`§`-code formatting used by both.

pub mod chat;
mod ping;
mod servers_dat;

pub use chat::{ChatSpan, ChatText};
pub use ping::{
    DEFAULT_PORT, PingFailure, PingOptions, PingPlayer, PingPlayers, PingVersion, ServerAddress,
    ServerPing, ServerPinger, decode_favicon, ping_server,
};
pub use servers_dat::{ServerList, ServerListEntry};
