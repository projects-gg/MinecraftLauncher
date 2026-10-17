//! Multiplayer servers: Server List Ping (status, MOTD, players, favicon, latency), the
//! `servers.dat` server list, and chat-component/`§`-code formatting used by both.

pub mod chat;
mod edit;
mod ping;
mod servers_dat;

pub use chat::{ChatSpan, ChatText};
pub use edit::{MAX_SERVER_ADDRESS, MAX_SERVER_NAME, ServerEdit, ServerEditError};
pub use ping::{
    DEFAULT_PORT, PingFailure, PingOptions, PingPlayer, PingPlayers, PingVersion, ServerAddress,
    ServerPing, ServerPinger, decode_favicon, ping_server,
};
pub use servers_dat::{ServerList, ServerListEntry};
