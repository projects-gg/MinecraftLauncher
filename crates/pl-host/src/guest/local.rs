//! The guest's local endpoint (PROTOCOL.md §8): the game connects to `127.0.0.1:<random port>`.
//!
//! The listener binds only `127.0.0.1`, accepts for 60 s and one connection at a time, and goes
//! away once the game connected. The game's first packet (its handshake) gives the protocol
//! number; only a login connection is relayed, status pings are closed without an answer.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Instant, timeout, timeout_at};

use crate::tunnel::varint::{self, Decoded};

/// Longest first packet the endpoint reads.
pub const MAX_HANDSHAKE: usize = 512;
/// How long the game has to send its first packet.
pub const HANDSHAKE_WAIT: Duration = Duration::from_secs(5);
/// The handshake's `next state` for a login.
pub const LOGIN: i32 = 2;

/// Decides whether a connection may be relayed, from its remote address (the launcher checks that
/// it belongs to the game process it started).
pub type AcceptFilter = Arc<dyn Fn(SocketAddr) -> bool + Send + Sync>;

/// The game's first packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameHandshake {
    /// The game's protocol number.
    pub proto: i32,
    /// 1 status, 2 login, 3 transfer.
    pub next_state: i32,
    /// Everything read so far (the handshake and whatever followed it), sent first at offset 0.
    pub kept: Vec<u8>,
}

/// What a buffer holds so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parsed {
    /// More bytes are needed.
    Incomplete,
    /// Not a handshake packet.
    Bad,
    /// The handshake: protocol, next state.
    Done { proto: i32, next_state: i32 },
}

fn take_varint(buf: &[u8], at: &mut usize) -> Result<i32, Parsed> {
    match varint::decode(&buf[*at..]) {
        Decoded::Value(value, len) => {
            *at += len;
            Ok(value)
        }
        Decoded::Incomplete => Err(Parsed::Incomplete),
        Decoded::TooLong => Err(Parsed::Bad),
    }
}

/// Parses a handshake packet (`VarInt len`, id `0x00`, `VarInt protocol`, `String address`,
/// `u16 port`, `VarInt next state`) from the start of `buf`.
pub fn parse_handshake(buf: &[u8]) -> Parsed {
    let mut at = 0;
    let len = match take_varint(buf, &mut at) {
        Ok(len) => len,
        Err(parsed) => return parsed,
    };
    let Ok(len) = usize::try_from(len) else {
        return Parsed::Bad;
    };
    if len == 0 || at + len > MAX_HANDSHAKE {
        return Parsed::Bad;
    }
    if buf.len() < at + len {
        return Parsed::Incomplete;
    }
    let packet = &buf[at..at + len];
    let mut p = 0;
    let field = |p: &mut usize| take_varint(packet, p).ok();
    if field(&mut p) != Some(0) {
        return Parsed::Bad;
    }
    let Some(proto) = field(&mut p) else {
        return Parsed::Bad;
    };
    let Some(address_len) = field(&mut p).and_then(|l| usize::try_from(l).ok()) else {
        return Parsed::Bad;
    };
    p += address_len;
    p += 2;
    if p > packet.len() {
        return Parsed::Bad;
    }
    let Some(next_state) = field(&mut p) else {
        return Parsed::Bad;
    };
    if p != packet.len() {
        return Parsed::Bad;
    }
    Parsed::Done { proto, next_state }
}

/// Reads the game's first packet (at most 512 bytes) within `wait`.
pub async fn read_handshake<R: AsyncRead + Unpin>(
    reader: &mut R,
    wait: Duration,
) -> Option<GameHandshake> {
    let deadline = Instant::now() + wait;
    let mut kept = Vec::with_capacity(64);
    let mut buf = [0u8; MAX_HANDSHAKE];
    loop {
        match parse_handshake(&kept) {
            Parsed::Done { proto, next_state } => {
                return Some(GameHandshake {
                    proto,
                    next_state,
                    kept,
                });
            }
            Parsed::Bad => return None,
            Parsed::Incomplete => {}
        }
        let room = MAX_HANDSHAKE.saturating_sub(kept.len()).max(1);
        let n = timeout_at(deadline, reader.read(&mut buf[..room]))
            .await
            .ok()?
            .ok()?;
        if n == 0 {
            return None;
        }
        kept.extend_from_slice(&buf[..n]);
    }
}

/// A login disconnect packet (id `0x00`, JSON text) for a game the host refused, in the guest's
/// language.
pub fn login_disconnect(text: &str) -> Vec<u8> {
    let json = serde_json::json!({ "text": text }).to_string();
    let mut body = Vec::with_capacity(json.len() + 8);
    varint::write(&mut body, 0x00);
    varint::write(&mut body, i32::try_from(json.len()).unwrap_or(i32::MAX));
    body.extend_from_slice(json.as_bytes());
    let mut packet = Vec::with_capacity(body.len() + 5);
    varint::write(&mut packet, i32::try_from(body.len()).unwrap_or(i32::MAX));
    packet.extend_from_slice(&body);
    packet
}

/// Sends a refusal to the game and closes its connection.
pub async fn refuse_game<W: tokio::io::AsyncWrite + Unpin>(mut game: W, text: &str) {
    let _ = timeout(Duration::from_secs(2), async {
        let _ = game.write_all(&login_disconnect(text)).await;
        let _ = game.shutdown().await;
    })
    .await;
}

/// The listener the game connects to.
#[derive(Debug)]
pub struct LocalEndpoint {
    listener: TcpListener,
    port: u16,
    until: Instant,
}

impl LocalEndpoint {
    /// Binds `127.0.0.1:0` (never another address) for `window`.
    pub async fn bind(window: Duration) -> io::Result<Self> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener,
            port,
            until: Instant::now() + window,
        })
    }

    /// The port for the game's quick play target (`127.0.0.1:<port>`).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Waits for the game's login connection until the window ends; status pings and connections
    /// the filter refuses are closed. The listener goes away with `self`.
    pub async fn accept(self, filter: Option<AcceptFilter>) -> Option<(TcpStream, GameHandshake)> {
        loop {
            let (mut stream, peer) = timeout_at(self.until, self.listener.accept())
                .await
                .ok()?
                .ok()?;
            if filter.as_ref().is_some_and(|allowed| !allowed(peer)) {
                continue;
            }
            let Some(handshake) = read_handshake(&mut stream, HANDSHAKE_WAIT).await else {
                continue;
            };
            if handshake.next_state != LOGIN {
                continue;
            }
            let _ = stream.set_nodelay(true);
            return Some((stream, handshake));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    fn game_bytes() -> Vec<u8> {
        let file = testdata::json("noise/nkpsk0-game.json");
        hex::decode(file["gameBytesGuestToHostHex"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn the_vector_handshake() {
        let bytes = game_bytes();
        assert_eq!(
            parse_handshake(&bytes),
            Parsed::Done {
                proto: 9999,
                next_state: 2
            }
        );
        for cut in 0..17 {
            assert_eq!(parse_handshake(&bytes[..cut]), Parsed::Incomplete, "{cut}");
        }
        // A login start is not a handshake.
        assert_eq!(parse_handshake(&bytes[17..]), Parsed::Bad);
    }

    #[tokio::test]
    async fn keeps_what_followed_the_handshake() {
        let bytes = game_bytes();
        let mut reader: &[u8] = &bytes;
        let handshake = read_handshake(&mut reader, HANDSHAKE_WAIT).await.unwrap();
        assert_eq!(handshake.proto, 9999);
        assert_eq!(handshake.kept, bytes);
    }

    #[tokio::test]
    async fn only_logins_are_relayed() {
        let endpoint = LocalEndpoint::bind(Duration::from_secs(60)).await.unwrap();
        let port = endpoint.port();
        let accepting = tokio::spawn(endpoint.accept(None));
        // A status ping first: closed without an answer.
        let mut ping = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut status = game_bytes()[..17].to_vec();
        status[16] = 1;
        ping.write_all(&status).await.unwrap();
        let mut answer = Vec::new();
        ping.read_to_end(&mut answer).await.unwrap();
        assert!(answer.is_empty());
        // Then the login.
        let mut game = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        game.write_all(&game_bytes()).await.unwrap();
        let (_stream, handshake) = accepting.await.unwrap().unwrap();
        assert_eq!(handshake.next_state, LOGIN);
        assert_eq!(handshake.kept, game_bytes());
        // The listener is gone.
        assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn the_window_closes() {
        let endpoint = LocalEndpoint::bind(Duration::from_secs(60)).await.unwrap();
        assert!(endpoint.accept(None).await.is_none());
    }

    #[test]
    fn disconnect_packet() {
        let packet = login_disconnect("Dünya dolu.");
        let Decoded::Value(len, at) = varint::decode(&packet) else {
            panic!()
        };
        assert_eq!(len as usize, packet.len() - at);
        assert_eq!(packet[at], 0x00);
        let text = String::from_utf8(packet[at + 2..].to_vec()).unwrap();
        assert_eq!(text, r#"{"text":"Dünya dolu."}"#);
    }
}
