# pl-host protocol, version 1 (HOST1)

"Open your world to friends" ("Dünyanı arkadaşlarına aç"): a player opens their singleplayer world to
friends they invite; a friend presses "Katıl" and their launcher prepares a matching install and
connects. This file is the contract between the parts that make that work:

- the **host launcher** (`H`): the player whose world it is;
- the **guest launcher** (`G`): a friend who joins;
- the **overlay mod** in the host's game (`M`, `mods/projects-overlay`, package
  `gg.projects.overlay.host`);
- the **friends service** (`S`, closed source): coordination on the presence socket and the TURN
  budget. Its side (messages, checks, limits, errors, budget) is in the service's README, sections
  "World sessions" and "TURN budget";
- two relays nobody here runs: **Cloudflare Realtime TURN** and the **e4mc relay** (QUIClime).

Status: written by HOST1-C on 23 October 2026 for phase 1; the lead agent approves it before the
implementation starts. Changes after approval are additive, or bump the version where this file
says so. Test vectors: [`testdata/`](testdata/README.md).

Conventions: MUST, SHOULD and MAY as usual. Integers on the wire are big-endian. "b64u" is base64url
without padding. JSON is UTF-8 without a BOM; receivers ignore fields they do not know unless a
section says otherwise; senders leave out absent optional fields. "VarInt" is Minecraft's
(LEB128 of the two's-complement `i32`, at most 5 bytes). Times are Unix seconds unless named `…Ms`.

## 1. Overview

```text
            presence socket (JSON)                          presence socket (JSON)
   H ◀────────────────────────────────▶ S ◀────────────────────────────────────────▶ G
   │ ▲  world, world_invite, turn_creds,     world_invite, world_update, turn_creds,  │
   │ │  world_peer_ok, world_report          world_peer, world_report                 │
   │ │                                                                                │
   │ │ PLJ1 (Noise, end to end) over a path: TURN (QUIC over two TURN allocations)    │
   │ └──────────────────────────────────────── or e4mc (TCP → relay → QUIC) ─────────┘
   │                                                                                   │
   │ PLG1 header + Minecraft bytes, 127.0.0.1 only           Minecraft bytes, 127.0.0.1 only
   ▼                                                                                   ▼
   M: integrated server on loopback                         guest's game (no mod needed)
```

- The service relays invites and path signals and hands out TURN credentials; it never carries game
  traffic and never sees the invite's keys in any form it keeps (it passes `join` along unread).
- Game traffic goes over one of two **paths**: Cloudflare TURN (both sides hold a TURN allocation and
  talk QUIC between the two relay addresses) or the e4mc relay (the host holds a QUIC connection to
  the relay, the guest a TCP connection). Either way neither player learns the other's IP address.
- Every stream on a path is a **PLJ1** stream: a Noise `NKpsk0` handshake proves the guest holds its
  invite's key and the host holds the session's static key; everything after it is encrypted, so the
  relays see nothing but sizes and times.
- The host launcher feeds each game stream to the integrated server on `127.0.0.1:<port>` and first
  writes a **PLG1** identity header the mod checks. The guest launcher listens on
  `127.0.0.1:<random port>` and its game connects there.
- A path may break or have to change (TURN budget, credential renewal, a relay failing). The game
  stream then **resumes** on another path or a fresh one from the last byte each side received; the
  games' own TCP connections never close.

## 2. Identifiers and secrets

| Name | Size, form | Made by | Travels | Secret? |
| --- | --- | --- | --- | --- |
| Session id `sid` | 16 random bytes, b64u (22) | H, per world session | Service messages, invite, Noise prologue, TURN label | No |
| Host session key | X25519 key pair; public b64u (43) | H, per session | Public half in every invite | Private half: yes, memory only |
| Slot | 16 random bytes, b64u (22) | H, per invitee per session | Invite, PLJ1 opening (in clear), `world_peer` | No |
| PSK | 32 random bytes, b64u (43) | H, per invitee per session | Invite only (S passes it on, keeps nothing) | **Yes** |
| Guest id | `g1` … `g999` | H, per slot, in order | PLJ1 `ok`, PLG1, bridge | No |
| Stream id | 8 random bytes, b64u (11) | H, per game or control stream | PLJ1 `ok`, `resume` | No |
| QUIC identity | P-256 key, self-signed certificate (SAN `plj.invalid`); pin = b64u(SHA-256(certificate DER)) (43) | H, per session, only with TURN | Pin in `paths` | Key: yes, memory only |
| Publish id | 1–32 characters `[A-Za-z0-9_-]` | H, per `host-publish` | Bridge, PLG1 | No |
| Gate key | 32 bytes | H and M, derived (§7.1) | Never | **Yes** |
| Binding key | 32 random bytes | H, once per computer, in `<data dir>/host/guest-binding.key` | Never | **Yes** (§9) |
| TURN credentials | `username`, `credential` from Cloudflare | S | `turn_creds` answer to one launcher | `credential`: **yes**; `username`: kept out of logs |

All random values come from the operating system's CSPRNG. Secrets live in memory only (the binding
key is the one file) and are wiped when the session ends. Section 13 says what may never be logged.

## 3. Lifecycle

### 3.1 Opening a world (host)

1. The player presses "Davet et" in the game (`host-request` on the bridge) or "Dünyayı aç" in the
   launcher (the launcher starts the game into that world, then waits for `host-world{open:true}`).
2. H checks it may host: signed `features.worlds` and service `features` both list `worlds`, signed
   in to friends, the install follows the official pack on 26.1.2, 26.2 or 26.3, the overlay is on,
   the mod's `hello.features` lists `host`, no vanilla LAN (`host-world.lan`), no e4mc or e4all jar
   in the install, not already hosting.
3. H makes `sid` and the host session key and sends `world{open:true, session, guests:0, max}`. An
   error ends here: bridge `host-state{open:false, error}` (`limit` for the daily opens, otherwise
   `unavailable`: "Dünyan şu an açılamıyor.").
4. H sends `host-publish{id, mode, commands, max, binding}`; M opens the world on loopback only and
   answers `host-published{id, ok:true, port, proto}` (or `ok:false` → step 3's error, and
   `world{open:false}`).
5. H asks `turn_creds{session, role:"host", reason:"primary"}`. The answer's `order` names the path
   to prepare; H prepares it (§5.3, §5.5) and, if it fails, the next one (TURN as a second choice
   needs `turn_creds{reason:"fallback"}` first). Nothing works: `world{open:false}`, bridge
   `host-stop`, "Dünyan şu an açılamıyor." (`testdata/policy/host-open.json`).
6. H announces the path: `world{open:true, session, guests:0, max, paths}`, then one `world_invite`
   per invitee, each with its own slot and PSK (§4). Invites go only once a path is ready.
7. H mirrors the state to the game (`host-state`) and the launcher UI.

The second path opens only when needed (a guest's order puts it first and asks with `world_peer`,
or a move needs it).

### 3.2 Joining (guest)

1. `world_invite` arrives; the launcher shows it. "Katıl":
2. G asks `turn_creds{session, host, role:"guest", reason:"primary"}` → `order`, maybe `ice`.
3. G tries the first path of `order` for at most 4 s (§5.7); TURN needs `world_peer` and its answer;
   a path missing from the invite's `paths` is asked for with `world_peer{path}`. On a path failure
   it tries the next one (for TURN after `turn_creds{reason:"fallback"}`); a PLJ1 refusal (`no`) is
   final.
4. G opens the **control stream** (`hello{kind:"control", name, premium}`), gets `ok` with a name
   verdict, the `manifest` and `state`. A name verdict other than `ok` → G offers to switch accounts
   (and opens a new control stream with the new name).
5. G picks or prepares an install (§10), opens its local endpoint (§8) and starts the game with
   quick play to `127.0.0.1:<port>`, or, if the game already runs, uses the bridge's `connect` with
   `label` ("Ayşe'nin dünyası") or asks to restart the game.
6. The game connects; G reads its handshake packet, opens a **game stream**
   (`hello{kind:"game", name, premium, mc, proto}`), and on `ok` relays the game's bytes.

### 3.3 During the session

- H sends `world{open:true, session, guests, max, paths}` when the guest count changes (at most once
  per 10 s) or a path changes (at once); S fans `world_update` out to invitees. Connected guests also
  get `state` on their control stream.
- Paths are moved without the players noticing (§6.7): TURN credential renewal, `turn_move` from S,
  a path failing, `move` from H.
- Launchers send `world_report` telemetry (§12).
- S keeps session state only with the presence socket. After the socket reconnects, H sends
  `world{open:true, session, guests, max, paths, invited}` again, and every launcher still holding
  TURN credentials sends `turn_held`; nothing else changes for the players.

### 3.4 Ending

| Event | H | G |
| --- | --- | --- |
| "Paylaşımı kapat", the host leaves the world, game exit | Bridge `host-stop` (or it came from M); `bye{closed}` on every control stream; game streams `close`; paths closed; `turn_release` for its credentials; `world{open:false}` | In game (the mod's kick): "Ayşe paylaşımı kapattı."; launcher: "Ayşe dünyayı kapattı." |
| H removes a guest: its button, or `world_revoked{to}` from S (block, unfriend, restriction) | Bridge `host-remove`; `bye{removed}`; slot deleted; `world_revoke{to}` only when H decided it | In game and launcher: "Bu dünyadan çıkarıldın." |
| Guest leaves | — | Game stream `close`; after 2 min without a game stream `bye{leave}` and the control stream closes |
| Path lost, no resume within 15 s | Local game connection closed | Local game connection closed; row shows "Yeniden katıl" (same invite) |

## 4. The invite (`join`)

H sends each invitee one `world_invite{session, to, join}`; S delivers `join` unread. Example (also
`testdata/invite/join.json`):

```json
{
  "v": 1,
  "session": "stuiX9ATk0i2-2_h8YikXQ",
  "key": "<host session public key, b64u, 43>",
  "slot": "<b64u, 22>",
  "psk": "<b64u, 43>",
  "mc": "26.1.2",
  "proto": 9999,
  "pack": "projects",
  "paths": {
    "turn": { "relay": "162.159.207.10:51234", "pin": "<b64u, 43>" },
    "e4mc": { "domain": "able-acid.eu.e4mc.link" }
  }
}
```

- `v`: 1. A guest that does not know `v` shows nothing and drops the invite.
- `mc`: the host's Minecraft version (shown small under the invite); `proto`: its protocol number,
  from `host-published.proto` (used in the e4mc handshake packet, §5.4, and checked by the host).
- `pack`: the official pack's family id the host's install follows (absent: phase 2 installs).
- `paths`: at least one ready path (§5.1).
- At most 1,500 bytes as compact JSON (typically ~370). No world name, no addresses but relay ones.
- The invite lives for the session. Re-inviting the same friend in a session sends the same `join`
  (same slot and PSK); after removal a new invite gets a new slot and PSK.

## 5. Paths

### 5.1 Path addresses (`paths`)

```json
{ "turn": { "relay": "<IPv4>:<port>", "pin": "<b64u>" }, "e4mc": { "domain": "<name>.e4mc.link" } }
```

- `turn.relay`: the XOR-RELAYED-ADDRESS of the host's TURN allocation, dotted IPv4 without leading
  zeros, port 1–65535. A guest refuses private, loopback, link-local, shared (100.64/10), multicast,
  broadcast, unspecified, documentation and other reserved ranges, and anything IPv6 (Cloudflare's
  relay addresses are IPv4). `pin`: §2.
- `e4mc.domain`: lowercase, matches `^[a-z0-9-]{1,63}\.[a-z0-9-]{1,63}\.e4mc\.link$`, at most 253
  characters. A guest connects to nothing else (a host cannot point guests at another server).
- Cases: `testdata/invite/join.json` (`relayAddresses`), `testdata/e4mc/wire.json` (`domains`).
- A launcher MUST NOT send its own public address anywhere: never the TURN XOR-MAPPED-ADDRESS, never
  an address learned from a socket.

### 5.2 Streams

Every PLJ1 stream is one reliable, ordered, bidirectional byte stream on a path: a QUIC
bidirectional stream (TURN) or one TCP connection through the e4mc relay. Only the guest opens
streams. Ending the write side ends the stream for the peer. One guest uses at most three streams at
a time: control, game and one being opened (a resume or a replacement control stream).

### 5.3 e4mc, host side

1. **Relay.** Studio's region (`e4mc.region` in the `turn_creds` answer): `auto` →
   `GET https://broker.e4mc.link/getBestRelay` with `User-Agent: ProjectsLauncher/<version>
   (+https://projects.gg)`, 1.5 s, answer `{"id":"eu","host":"eu.e4mc.link","port":25575}` (`host`
   must end in `.e4mc.link`, `port` 1–65535; `id` is the telemetry `region`). Broker failure:
   `eu.e4mc.link`, `de.e4mc.link`, `uk.e4mc.link` in turn, port 25575. `eu`/`de`/`uk`: that relay
   directly.
2. **QUIC** to the relay: ALPN `quiclime`, server name = the relay host, normal certificate
   verification (platform roots), 2.5 s to connect. Keep-alive 5 s, idle timeout 15 s. Allow the
   relay to open at least 32 concurrent bidirectional streams.
3. **Control stream**: open one bidirectional stream (the relay allows only one client-opened
   stream) and write `VarInt(len) || {"kind":"request_domain_assignment"}`. The relay answers with
   **one length byte** then JSON: `{"kind":"domain_assignment_complete","domain":"…"}` (2 s). Skip
   `has_capabilities` and `unknown_message`. After the domain, read and discard the control stream
   until it ends; never show or log a `request_message_broadcast`'s text (its `kind` may be logged).
   Bytes: `testdata/e4mc/wire.json`.
4. **Player streams**: the relay opens a bidirectional stream per incoming TCP connection. Its first
   bytes are the relay's re-encoding of the player's handshake packet (`VarInt len`, id `0x00`,
   `VarInt protocol`, `String address`, `u16 port`, `VarInt next state`). H reads that one packet
   (at most 512 bytes, 5 s), checks id 0 and next state 2, and that the address (FML suffixes
   `\0FML\0`, `\0FML2\0`, `\0FML3\0` cut) equals its domain ASCII case-insensitively; anything else
   closes the stream. The PLJ1 opening follows.
5. The domain lives as long as the QUIC connection. A new connection gets a new domain: announce
   the new `paths` (§3.3).

### 5.4 e4mc, guest side

1. Resolve the domain, TCP to port 25565 (`TCP_NODELAY`).
2. Write, in one write: the handshake packet (id 0, `proto` from the invite, address = the domain,
   port 25565, next state 2) and the PLJ1 opening (§6.2, framed like a Minecraft packet).
3. Read one VarInt `L`. `L == 48`: PLJ1 msg2 follows. Otherwise read `L` bytes (at most 4,096): a
   packet with id `0x00` and a string is a relay refusal: text containing `too fast` →
   `e4mc_rate_limited`, any other → `e4mc_unknown_domain`; anything else → `plj_auth_failed` (an
   address that now belongs to someone else).

Why the PLJ1 opening is framed like a Minecraft packet: a relay that refuses the connection reads one
packet of at most 128 bytes before it answers. Without the frame it would wait for bytes that never
come and the refusal would look like a timeout.

### 5.5 TURN, both sides

- **Credentials** come only in a `turn_creds` answer (`ice`: `urls`, `username`, `credential`,
  `ttl`). Long-term credential mechanism (RFC 8489 §9.2), realm and nonce from the server's 401;
  a stale nonce (438) is retried with the new one.
- **Transports**, in this order: `turn:…:3478?transport=udp`, `turn:…:443?transport=udp`,
  `turns:…:443?transport=tcp`, `turn:…:80?transport=tcp`, `turns:…:5349?transport=tcp`,
  `turn:…:3478?transport=tcp` (only URLs `ice.urls` lists). UDP gets a 1 s head start; then TLS 443
  races it. Success over TCP or TLS after UDP failed reports `turn_tcp_only` (a success).
- **Allocation**: REQUESTED-TRANSPORT UDP, LIFETIME 600 s, refreshed when 120 s are left; deleted
  (LIFETIME 0) when no longer needed.
- **Peers**: CreatePermission for the peer's relay IP (refreshed every 240 s) and ChannelBind for the
  peer's relay address (refreshed every 480 s); data as ChannelData (padded to 4 bytes on TCP/TLS).
  The host permits each guest's relay address when `world_peer` brings it; the guest permits the
  host's relay address from `paths`.
- **Renewal**: when 20 min of `ttl` are left and the path is still needed, ask
  `turn_creds{reason:"renew"}`, make a **new** allocation, move the streams to it (§6.7), then delete
  the old allocation and send `turn_release{username}` for the old credentials. A refused renewal
  moves the streams to e4mc. A host with no guest on TURN at renewal time releases instead.
- Unused credentials are released at once (`turn_release`), e.g. when the allocation failed and the
  other path won.

### 5.6 QUIC over TURN

- The host runs one QUIC server endpoint on its allocation; each guest runs a QUIC client on its own
  allocation and connects to the host's relay address. Datagrams to and from a peer go through the
  channel bound to that peer.
- ALPN `plj1`, server name `plj.invalid`, the host's per-session certificate; the client accepts
  exactly the certificate whose SHA-256 equals the pin (no other verification, no client
  certificate). Pin mismatch: `turn_quic_pin`.
- Idle timeout 10 s, keep-alive 2 s, at most 4 concurrent bidirectional streams per connection (all
  opened by the guest), no datagrams in version 1, initial MTU 1200.
- A new allocation on either side means a new QUIC connection (no QUIC migration).

### 5.7 Path policy

The numbers are data: `testdata/policy/client-policy.json` (the `Policy` default in code must equal
it; a test compares them). Behaviour as scenarios: `testdata/policy/guest-connect.json`,
`testdata/policy/host-open.json`. The service's side (which order it answers) is
`testdata/policy/service-order.json`.

- **Order.** The guest follows `order` from `turn_creds`. An empty order: "Bağlanılamadı. Tekrar
  dene." (for the host: "Dünyan şu an açılamıyor.").
- **Attempt.** One path attempt has 4 s, from its first step to `ok` on the control stream. TURN:
  allocation → `world_peer` → `world_peer_ok` → QUIC → PLJ1. e4mc: (`world_peer{path:"e4mc"}` if the
  invite has no domain) → TCP → PLJ1.
- **Failure vs refusal.** A failed step (timeout, relay refusal, `plj_auth_failed`, TLS, QUIC) moves
  to the next path in `order`. A PLJ1 `no` is final: show its text, try nothing else.
- **TURN second.** Before trying TURN after another path failed, ask
  `turn_creds{reason:"fallback"}`; `turn_denied` ends the attempt with that code.
- **Host, second path on demand.** `world_peer{path}` for a path H has not opened: H opens it (TURN:
  `turn_creds{role:"host", reason:"primary"}`, which S grants when a guest of the session was just
  given TURN credentials) and answers `world_peer_ok` with its `paths` either way. The guest checks
  whether the path it wanted is there.
- **Rates.** A guest opens at most 5 new streams a minute per session.
- **Telemetry.** One `attempt` event per path attempt (§12).

## 6. PLJ1

### 6.1 Noise

- `Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s` (Noise spec revision 34). Guest = initiator, host =
  responder. The responder's static key is the host session key (pre-message `<- s`).
- Pattern: `-> psk, e, es` then `<- e, ee`. PSK = the invite's PSK at position 0.
- Prologue: ASCII `PLJ1` (4 bytes) || slot (16 raw bytes) || `sid` (16 raw bytes) = 36 bytes.
- Both handshake payloads are empty in version 1: msg1 and msg2 are exactly 48 bytes each. A
  non-empty payload is a protocol error.
- Libraries: `snow` (Rust) and noise-java (phone, phase 2) speak it; the vectors were checked with
  snow 0.10.0 and Python noiseprotocol 0.3.1.

### 6.2 Opening

```text
guest → host:  0x44 || "PLJ1" || slot (16) || msg1 (48)       VarInt(68), then 68 bytes
host → guest:  0x30 || msg2 (48)                               VarInt(48), then 48 bytes
```

- The host reads exactly 69 bytes within 5 s of the stream starting (after the e4mc handshake packet
  on that path). A wrong length byte, wrong magic, unknown or removed slot, or a msg1 that does not
  decrypt: close the stream **without sending anything**. The host looks up the slot before any
  cryptography and never tries other keys.
- After msg2 both sides are in transport mode.
- **Failed openings** (anything that closed silently, and openings that time out) above 20 in a
  minute put the host in a 60 s busy window: it closes new streams at once without reading them.
  Live streams carry on.

### 6.3 Records

After the handshake every message is a **frame**: `u16 length || ciphertext`, where the ciphertext is
the Noise transport encryption of one **record** (nonces count from 0 in each direction). Length
17–16,400. A frame that does not decrypt closes the stream.

A record is `type (1 byte) || body`, at most 16,384 bytes:

| Type | Name | Body | Streams |
| --- | --- | --- | --- |
| `0x01` | `ctrl` | UTF-8 JSON object, the last piece of a control message | all |
| `0x02` | `ctrl-part` | UTF-8 JSON, a piece that more pieces follow | all |
| `0x03` | `data` | `u64 offset`, then 1–16,375 bytes of the game connection | game |
| `0x04` | `ack` | `u64 next`: bytes of this direction received in order | game |
| `0x05` | `ping` | 8 opaque bytes | all |
| `0x06` | `pong` | the ping's 8 bytes | all |

- A control message longer than one record is split into `ctrl-part` pieces of 16,383 bytes and a
  final `ctrl`; the receiver joins the bodies and parses once. At most 65,536 bytes per message.
- Unknown record types are skipped (later versions only add types that are safe to skip).
- A record a stream kind does not allow (`data` on a control stream) closes the stream.
- Encodings: `testdata/plj1/records.json`; encrypted: `testdata/noise/*.json`.

### 6.4 Control messages

Every stream starts with the guest's `hello`; the host answers `ok` or `no` (and then closes).

| Direction | Message | Fields | When |
| --- | --- | --- | --- |
| G → H | `hello` | `v:1`, `kind:"control"`, `name`, `premium`, `launcher`, `caps:[]` | Opening a control stream (a second one from the same slot replaces the first) |
| G → H | `hello` | `v:1`, `kind:"game"`, `name`, `premium`, `mc`, `proto`, `caps:[]` | Opening a game stream, after the game connected locally |
| G → H | `hello` | `v:1`, `kind:"resume"`, `stream`, `recv`, `caps:[]` | Continuing a game stream on another stream (§6.7) |
| H → G | `ok` | control: `guest`, `stream`, `name` (`ok`, `name`, `name-taken`), `caps`; game: `guest`, `stream`, `caps`; resume: `stream`, `recv` | Accepted |
| H → G | `no` | `reason` (§6.9) | Refused; the host closes the stream after it |
| H → G | `manifest` | §10.1 | After `ok` on every control stream |
| H → G | `state` | `guests`, `max`, `paths` | After `manifest`, and whenever they change |
| H → G | `move` | `to` (`turn`, `e4mc`), `why` (`budget`, `cap`, `renew`, `failure`, `off`) | Planned move of this guest's streams (§6.7) |
| H → G | `bye` | `reason`: `closed`, `removed` | The session ended, or this guest was removed |
| G → H | `bye` | `reason`: `leave` | The guest leaves (or has had no game stream for 2 min) |
| both, game stream | `close` | `reason` (`eof`, `error`), `at` (bytes this side sent) | The local game connection ended |

- `name`: the in-game name the guest will log in with (3–16 of `[A-Za-z0-9_]`); `premium`: the
  account is a Microsoft account and the game will authenticate with Mojang; `launcher`: the guest's
  launcher version; `mc`, `proto`: from the game's own handshake packet (§8); `proto` 0 when not
  known.
- `guests`: guests in the world now, the host not counted; `max`: guest limit (1–7). Interfaces show
  `guests + 1` of `max + 1` ("2/8").
- `caps`: empty in version 1; unknown entries are ignored.
- Examples with exact bytes: `testdata/noise/nkpsk0-control.json`, `…-game.json`, `…-resume.json`.

### 6.5 Game streams

- After `ok`, each side relays its local game connection as `data` records. `offset` is the position
  of the record's first byte in that direction (the first byte of the connection is offset 0).
- A receiver keeps `next` (bytes received in order). `offset < next`: drop the bytes it already has;
  `offset > next`: protocol error, close the stream.
- `ack{next}` after every 64 KiB received or 250 ms after the first unacknowledged byte, whichever
  is first; nothing when nothing arrived.
- Each side keeps what it sent until acknowledged, at most 8 MiB; when that is full it stops reading
  its local connection (the game slows down rather than anything being lost).
- `close{reason, at}` after the last `data`: the receiver delivers everything up to `at`, then closes
  its local connection. The side that sent `close` keeps the stream resumable until `at` is
  acknowledged or the resume window ends; a resume then sends the rest and `close` again.

### 6.6 Keepalive and failure

- A side sends `ping` on a stream after 1 s in which it sent nothing on it; the peer answers `pong`
  at once. One ping at a time; ping-pong round trips are the RTT the session report gives (§12).
- A stream that received nothing for 3.5 s, or whose path reports an error, is **broken**.
- Broken control stream: the guest opens a new one (§6.7). Broken game stream: §6.7.

### 6.7 Resume and moving

The games' TCP connections (guest's game to its local endpoint, host launcher to the integrated
server) stay open through all of this; Minecraft stalls for a moment and goes on (its own timeout is
15 s and more).

**Host.** A broken game stream is **suspended** for 15 s: the local connection stays open, its bytes
are kept (up to 8 MiB). A `hello{kind:"resume", stream, recv}` from the same slot for that stream
within 15 s: H answers `ok{stream, recv:<what H received>}`, resends from the guest's `recv`, and the
stream lives on the new one. A resume while the old stream still looks alive replaces it at once.
After 15 s H closes the local connection (the mod reports the guest gone). Unknown or expired stream:
`no{resume}`.

**Guest.** On a broken game stream, `move`, `turn_move` or a TURN renewal, G opens a new stream and
resumes:

1. Target: `move.to`, else the other path in `paths`, else the same path (new addresses from `state`
   or `world_update` when the host renewed). Missing address: `world_peer{path}` (§5.7).
2. Open the stream, opening, `hello{kind:"resume", stream, recv:<what G received>}`, `ok{recv}`,
   resend from the host's `recv`. Then a new control stream on the same path (the old one is closed).
3. Give up 12 s after the stream broke (for planned moves: when the old path is gone and no target
   worked): close the local game connection, report `resume_failed`, show "Yeniden katıl".

**Planned moves** (`move`, `turn_move`, renewal): the guest opens the new stream while the old one
still works and switches on `ok`; both sides close the old stream right away (anything unacknowledged
is resent from `recv`). The host keeps an old path open for 30 s after its last guest moved, then
closes it (TURN: deletes the allocation, `turn_release`).

**Host-side moves.** `turn_move` for the host's own credentials, a lost relay connection or a
renewal: H opens the target path if needed (TURN renewal: permits every TURN guest's relay address on
the new allocation first), announces `paths` (`world` → `world_update`; `state` on control streams),
then sends `move{to, why}` to each guest on the old path.

Every move is one `move` telemetry event (§12).

### 6.8 Admission

Per slot the host keeps: guest id, friend id, the friend's binding digest (§9), PSK, state
(`invited`, `joining`, `in`, `removed`) and streams.

Control `hello`: the slot must not be removed and the session open (else `no{removed}` /
`no{closed}`), and step 6's stream limits hold (else `no{busy}`); the name gets a verdict by steps 3
and 4 without refusing the stream.

Game `hello`, in this order:

1. Slot removed → `no{removed}`; session closing → `no{closed}`.
2. `proto` > 0 and not the host game's protocol (`host-published.proto`) → `no{version}`.
3. Name not 3–16 of `[A-Za-z0-9_]`, or equal to the host's own in-game name (ASCII
   case-insensitive), or in the world already from another stream → `no{name}`.
4. Name bound to another friend in `projects-guests.json` (§9) → `no{name-taken}`.
5. Guests in the world (other than this slot) ≥ `max` → `no{full}`.
6. Busy window, more than 5 new streams from this slot in the last minute, or already 3 open streams
   from it → `no{busy}`.
7. Connect to `127.0.0.1:<host-published.port>`, write the PLG1 header (§7) → `ok`. The connect
   fails → `no{closed}`.

The mod checks names, binding, count and authentication again at login (§7.3); the host launcher's
checks only give the guest a clear answer before the game starts.

### 6.9 Refusal reasons

| `no.reason` | Telemetry | Guest shows (TR / EN) |
| --- | --- | --- |
| `full` | `plj_full` | Dünya dolu. / The world is full. |
| `closed` | `plj_closed` | Ayşe dünyayı kapattı. / Ayşe closed their world. |
| `removed` | `plj_removed` | Bu dünyadan çıkarıldın. / You were removed from this world. |
| `name` | `plj_name` | Bu adla giremezsin. / You can't join with this name. (+ switch account) |
| `name-taken` | `plj_name` | Bu ad bu dünyada başka birine ait. / Someone else uses this name in this world. (+ switch account) |
| `version` | `plj_version` | Ayşe'nin sürümü (26.3) sende yok. / … (+ "Kur") |
| `busy` | `plj_busy` | Bağlanılamadı. Tekrar dene. / Couldn't connect. Try again. |
| `resume` | `resume_failed` | Row: "Yeniden katıl" |
| `unsupported` (hello `v` or `kind` unknown) | `plj_error` | Bağlanılamadı. Tekrar dene. |

Unknown reasons read as `busy`.

## 7. Host launcher → game (PLG1)

### 7.1 Gate key

`gate key = HMAC-SHA256(key = the bridge token's 43 ASCII bytes, message = ASCII "projects host gate v1")`.
Both sides already hold the token (`-Dprojects.overlay.token`, OVL1 §4); no new secret travels.

### 7.2 Header

On every local connection to the world's port, before any game byte, the host launcher writes:

```text
"PLG1" (4) || u16 length || JSON (length bytes, at most 1,024) || HMAC-SHA256(gate key, all bytes before) (32)
```

```json
{"v":1,"id":"p1","conn":1,"guest":"g1","name":"Notch","premium":false,"friend":"1UbirWHKbvkTXKMZNmDDug"}
```

- `id`: the publish id of `host-publish`; `conn`: a positive integer, unique within that publish id;
  `guest`: guest id; `name`, `premium`: from the game `hello` (§6.4); `friend`: the friend's binding
  digest (§9).
- Vectors: `testdata/gate/plg1.json`.

### 7.3 What the mod does

- Its handler is first in the Netty pipeline of every loopback connection while our session is open
  (HOST1-M's mixin on `ServerConnectionListener.startTcpServerListener`): within 5 s it reads the magic,
  length (≤ 1,024), JSON and MAC, compares the MAC in constant time, parses, checks `v == 1`, `id` ==
  the current publish id, `conn` unused, then binds the identity to the channel and removes itself.
  Anything else, or no header: close without a word.
- At login (`handleHello` mixin) for a connection with an identity:
  - the requested name must equal the header's name (ASCII case-insensitive) → else "Bu adla
    giremezsin.";
  - never the host's own name (the vanilla hole: an unauthenticated remote login with the host's name
    gets the host's profile) → "Bu adla giremezsin.";
  - binding: bound to another digest → "Bu ad bu dünyada başka birine ait."; unbound → bound to this
    digest after the login completes (§9);
  - the same name may not be online twice;
  - `premium: true` → Mojang authentication for this connection (encryption, `hasJoined`), the real
    UUID and skin; `false` → offline profile (`UUIDUtil.createOfflineProfile`).
- `PlayerList.canPlayerLogin` mixin: guests ≥ `max` → "Dünya dolu."; removed guests (by guest id,
  this session) refused.
- Kick and close texts go in the guest's language (`clientInformation().language()`; Turkish or
  English, anything else English) as plain text.
- Every mixin is `require = 1`.

### 7.4 Bridge frames

The overlay bridge (OVL1 §3, one JSON object per line) gains, only towards a mod whose `hello`
lists `host`: `host-world` (the player entered or left a singleplayer world), `host-request` ("Davet
et" in the game), `host-publish` / `host-published` (open the world on loopback; the port and the
protocol number), `host-guest` (a guest in, out or refused at the gate), `host-state` (the session
mirrored for the mod's screens), `host-invite`, `host-remove`, `host-stop`, `connect.label`,
`friend.world` and toast kind `world`. Fields and rules: the OVL2 protocol log, section "HOST1"
(rows H1–H16), which `crates/pl-overlay/src/protocol.rs` and the mod's `bridge/Protocol.java`
follow. The launcher gives the game commands (`connect`, `host-publish`, `host-stop`,
`host-remove`) only right after the player's own press, except for security events (block,
unfriend, restriction, the launcher quitting); path moves never reach the game.

## 8. Guest local endpoint

- Bind `127.0.0.1:0` (never another address) when "Katıl" or "Yeniden katıl" is pressed; accept for
  60 s, one connection at a time; close the listener when the game connected or the 60 s passed.
- The launcher SHOULD accept only a connection whose remote end belongs to the game process it
  started (Windows: owning process of the TCP row).
- Read the game's first packet (the handshake, at most 512 bytes, 5 s). A next state other than 2
  (login) closes the connection (no status answers). Take `proto` from it and the version from the
  install; keep the bytes.
- Open a game stream with `hello{kind:"game"}`; on `ok` send the kept bytes as the first `data`
  (offset 0) and relay from then on. On `no`, the launcher MAY answer the game with a login
  disconnect packet (id `0x00`, JSON text) in the guest's language before closing; HOST1-E checks
  the packet on 26.1.2, 26.2 and 26.3. The launcher shows §6.9's text either way.

## 9. Name binding file

`<world folder>/projects-guests.json`, written by the mod (temporary file, then replace), read by
both the mod and the host launcher (for the control stream's name verdict):

```json
{
  "v": 1,
  "key": "sW9hzU3c",
  "names": [
    { "name": "Notch", "friend": "1UbirWHKbvkTXKMZNmDDug", "last": 1792521000 }
  ]
}
```

- `key`: id of the binding key the digests were made with, b64u(SHA-256(binding key)[0..6]), 8
  characters. The launcher sends its key id in `host-publish.binding`. A file with another key id
  (world copied from another computer, launcher data reset) counts as empty and is rewritten on the
  next binding.
- `friend`: b64u(HMAC-SHA256(binding key, ASCII `projects guest binding v1` || `0x00` || UTF-8
  friend user id)[0..16]), 22 characters. No Discord id is ever written into the world.
- `last`: last login with that name, updated at most once per session.
- Names compare ASCII case-insensitively; one entry per name; at most 512 entries (the oldest `last`
  goes). The host's own name is never stored.
- Malformed file (not JSON, unknown `v`, an entry that breaks these rules): the mod renames it to
  `projects-guests.json.bad` (replacing an older one), logs one line and starts empty; the launcher
  treats it as empty too. Only the host's own hand breaks the file (the mod replaces it atomically),
  and every guest is the host's friend, so availability wins over keeping old bindings.
- The binding key is 32 random bytes in `<launcher data dir>/host/guest-binding.key`, made on first
  hosting, never sent anywhere. Vectors: `testdata/binding/`.

## 10. World manifest and install matching

### 10.1 Manifest

Sent by the host on every control stream after `ok` (never through the service):

```json
{
  "t": "manifest", "v": 1,
  "mc": "26.1.2",
  "loader": { "kind": "fabric", "version": "0.19.5" },
  "pack": { "family": "projects", "id": "projects-26.1.2", "version": "2.3.0" },
  "sets": ["pvp"],
  "mods": [ { "project": "P7dR8mSH", "version": "9YVrKY0Z", "env": "both" } ],
  "unknown": 0,
  "hash": "<64 hex>"
}
```

- `pack`: from the install's pack state (`packId`, `family`, `versionId`); `null` for an install that
  follows no pack (phase 2). `sets`: the ready-made sets added to the install.
- `mods`: every **enabled** jar in the install's `mods/` whose `fabric.mod.json` `environment` is `*`
  or absent (both sides), pack jars included, as Modrinth project and version ids (pack items carry
  them; other jars by Modrinth's version-file lookup with the jar's SHA-512). Jar-in-jar mods are
  not listed. `env` is `both` in version 1.
- `unknown`: how many such jars Modrinth does not know. No file name, path or hash of them is sent;
  nothing is ever uploaded from a disk.
- `hash`: lowercase hex SHA-256 of UTF-8 `plj-mods-v1\n` followed by `<project>:<version>\n` for every
  entry of `mods`, the lines sorted bytewise. Equal hashes and `unknown == 0` on both sides mean the
  same both-side mods.
- At most 1,000 entries (a larger list is cut and the rest counted in `unknown`). Examples:
  `testdata/manifest/examples.json`.

### 10.2 Matching

The guest computes the same manifest for each of its installs (same code) and decides
(`testdata/manifest/match.json`); "latest" is the pack version the guest's signed content names for
that Minecraft version. An install that follows a pack it has not installed yet (a new launcher's
official install before its first start) counts as that pack at version `""`: no mods, so step 2
syncs it (`update-pack`) instead of step 4 offering a copy of every mod.

1. `mc` and `loader.kind` must match; no install of that version → `install` (LIB1's "Kur" for the
   official pack at that version), then decide again.
2. Pack versions: host's == latest and the guest's install is older → `update-pack` (the normal pack
   sync), then decide again. Host's older than latest → `host-pack-old` ("Ayşe'nin paketi eski. Ayşe
   oyunu yeniden açınca katılabilirsin."). Host's unknown to the guest → refresh the signed content
   once and decide again; still unknown → "Bağlanılamadı. Tekrar dene."
3. Candidates, best first: a copy made for this host whose mods equal the manifest's; the official
   install whose mods contain all of the manifest's (extra both-side mods on the guest are allowed);
   any other install that contains them all, most recently played first. First candidate → `use`.
4. None: a copy for this host exists but differs → `update-copy` (reinstall its mod set); else
   `copy`: "Ayşe'nin kurulumu farklı. Katılmak için Ayşe'nin modlarıyla bir kurulum oluşturulur." with
   the count and download size, "Kopyala ve katıl" / "Vazgeç". The copy is a new install
   "Ayşe'nin dünyası": the official pack at the host's version, the host's `sets`, and exactly the
   manifest's mods. The launcher keeps the host's friend id in the install's own metadata (`copyOf`,
   never inside the game folder); the copy enters the usual cleanup when unused.
5. `unknown > 0`: join anyway with "2 mod bulunamadı; girerken sorun çıkabilir."
6. The guest's own installs are never changed (no sets or mods added to them).
7. A registry or mod mismatch found in the game's log after joining → "Ayşe'nin modlarından bazıları
   sende yok." + "Kurulumu kopyala".

## 11. `Transport` interface

The code below is the API surface between the tunnel (R1) and the paths (R2). **Bootstrap**: R1's
first commit, before R2 starts, adds `crates/pl-host/Cargo.toml` (R1's dependencies, and a
`# transport` block R2 extends), `src/lib.rs`, `src/transport/mod.rs` = the code below plus
`pub mod e4mc;` and `pub mod turn;`, empty `src/transport/e4mc.rs` and `src/transport/turn.rs`, and
removes the root manifest's `exclude = ["crates/pl-host"]`. R2 branches from that commit and owns
`src/transport/**` from then on (changes to this surface are additive and noted here); R1 never edits
those files again, so nothing conflicts. R1's tests use fakes in its own modules.

```rust
//! Paths a PLJ1 stream rides on (PROTOCOL.md §5, §11). Implementations: `e4mc`, `turn`.

use std::borrow::Cow;
use std::future::Future;
use std::net::SocketAddrV4;
use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncWrite};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Which relay a path goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PathKind {
    Turn,
    E4mc,
}

impl PathKind {
    /// The wire name (`paths`, `order`, telemetry).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::E4mc => "e4mc",
        }
    }
}

/// What a host announces for one of its paths (an entry of `paths`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathAddress {
    /// The host allocation's relay address and the SHA-256 of its QUIC certificate.
    Turn { relay: SocketAddrV4, pin: [u8; 32] },
    /// The e4mc domain, lowercase.
    E4mc { domain: String },
}

/// A step of a path attempt (PROTOCOL.md §12.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    Creds,
    Alloc,
    Peer,
    Broker,
    Quic,
    Domain,
    Tcp,
    Plj,
    Resume,
}

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Creds => "creds",
            Self::Alloc => "alloc",
            Self::Peer => "peer",
            Self::Broker => "broker",
            Self::Quic => "quic",
            Self::Domain => "domain",
            Self::Tcp => "tcp",
            Self::Plj => "plj",
            Self::Resume => "resume",
        }
    }
}

/// A path failure: the step and one code of PROTOCOL.md §12.2. Never carries an address, a domain
/// or a credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError {
    pub step: Step,
    pub code: Cow<'static, str>,
}

/// One reliable, ordered, bidirectional byte stream: a QUIC stream (TURN) or a TCP connection
/// through the e4mc relay. Shutting down the write half ends the stream for the peer.
pub trait PathStream: AsyncRead + AsyncWrite + Send + Unpin + 'static {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> PathStream for T {}

pub type BoxStream = Box<dyn PathStream>;

/// The host's side of one path.
pub trait HostPath: Send + Sync {
    fn kind(&self) -> PathKind;
    /// What invites, `world` and `world_peer_ok` announce for this path.
    fn address(&self) -> PathAddress;
    /// TURN: permit and bind a guest's relay address until `forget_peer`. e4mc: nothing.
    fn admit_peer(&self, relay: SocketAddrV4) -> BoxFuture<'_, Result<(), PathError>>;
    fn forget_peer(&self, relay: SocketAddrV4);
    /// The next stream a guest opened, positioned at the PLJ1 opening (e4mc: the relay's handshake
    /// packet already read and checked).
    fn accept(&self) -> BoxFuture<'_, Result<BoxStream, PathError>>;
    /// Resolves once the path is gone for good (relay closed it, allocation or credentials lost).
    fn closed(&self) -> BoxFuture<'_, PathError>;
    /// Bytes received from the relay so far, transport overhead included (TURN: what is billed).
    fn rx_bytes(&self) -> u64;
    fn close(&self) -> BoxFuture<'_, ()>;
}

/// The guest's side of one path to one host.
pub trait GuestPath: Send + Sync {
    fn kind(&self) -> PathKind;
    /// A new stream to the host (e4mc: a TCP connection with the handshake packet written; TURN: a
    /// QUIC stream, connecting first if needed).
    fn open(&self) -> BoxFuture<'_, Result<BoxStream, PathError>>;
    /// The opening got a reply that is not msg2 (`packet`: the bytes after its `VarInt` length):
    /// the path's own name for it (e4mc: `e4mc_unknown_domain`, `e4mc_rate_limited`), if any.
    fn explain_reply(&self, packet: &[u8]) -> Option<Cow<'static, str>>;
    fn closed(&self) -> BoxFuture<'_, PathError>;
    fn rx_bytes(&self) -> u64;
    fn close(&self) -> BoxFuture<'_, ()>;
}
```

Constructors (R2; shapes, not fixed by this contract):

- `e4mc::open_host(region, user_agent) -> Result<impl HostPath, PathError>` (steps `broker`, `quic`,
  `domain`; reports the relay `id` for telemetry);
- `e4mc::guest(domain, proto) -> Result<impl GuestPath, PathError>` (validates the domain, §5.1);
- `turn::open_host(ice, quic_identity) -> Result<impl HostPath, PathError>` (step `alloc`);
- `turn::allocate(ice) -> Result<TurnGuest, PathError>`, `TurnGuest::relay() -> SocketAddrV4`,
  `TurnGuest::connect(host_relay, pin) -> Result<impl GuestPath, PathError>` (steps `alloc`, `peer`,
  `quic`).

R1's tunnel takes `&dyn HostPath` / `&dyn GuestPath` and never knows which relay it uses; the mod
never knows either.

## 12. Telemetry

Kept anonymous by construction: the service drops the sender at once and adds events to hourly
totals (README "World sessions" → Telemetry). Frames go on the presence socket:

```json
{"t":"world_report","mc":"26.1.2","lv":"2.1.0","r":[ <1–20 events> ]}
```

### 12.1 Events

| `k` | Fields | When |
| --- | --- | --- |
| `a` attempt | `path`, `role` (`host`, `guest`), `step`, `code`, `ms`, `region?` (e4mc relay id) | One per path attempt (guest: §5.7; host: preparing a path); the host's broker failure is its own event before the attempt's |
| `m` move | `from`, `to`, `why` (`budget`, `cap`, `renew`, `failure`, `off`, `host`), `code` (`resume_ok`, `resume_failed`), `ms` | One per stream move (§6.7) |
| `s` session | `role`, `min` (how long, in minutes), `guests` (most at once), `turnRx`, `e4mcRx` (bytes received from the relays), `moves`, `rttTurn`, `rttE4mc` (median, 0 when unused) | When the launcher's part of a session ends |
| `u` usage | `session`, `role`, `turnRx` (bytes since the previous `u`) | Every 5 min while a TURN allocation is in use, and when it is deleted |

`step` per path: TURN `creds`, `alloc`, `peer`, `quic`, `plj`; e4mc `broker`, `quic`, `domain`
(host), `tcp`, `plj` (guest). An event with a field outside its list, a step that does not belong to
its path, a code outside §12.2 or an unbucketed number is dropped whole.

### 12.2 Codes

Success: `ok`, and `turn_tcp_only` (TURN worked only over TCP or TLS).

| Path / part | Codes |
| --- | --- |
| e4mc | `e4mc_broker_timeout`, `e4mc_broker_http_<3-digit status>`, `e4mc_broker_bad`, `e4mc_quic_timeout`, `e4mc_quic_refused`, `e4mc_domain_timeout`, `e4mc_closed`, `e4mc_rate_limited`, `e4mc_unknown_domain`, `e4mc_dns_failed`, `e4mc_tcp_refused`, `e4mc_tcp_timeout`, `e4mc_protocol_changed` |
| TURN | `turn_denied_budget`, `turn_denied_day`, `turn_denied_session`, `turn_denied_player`, `turn_denied_full`, `turn_denied_off`, `turn_creds_error`, `turn_alloc_unauthorized`, `turn_alloc_timeout`, `turn_alloc_error`, `turn_permission_failed`, `turn_peer_timeout`, `turn_quic_timeout`, `turn_quic_pin` |
| PLJ1 | `plj_auth_failed`, `plj_timeout`, `plj_full`, `plj_closed`, `plj_removed`, `plj_name`, `plj_version`, `plj_busy`, `plj_error` |
| Moves | `resume_ok`, `resume_failed` |

`e4mc_protocol_changed`: an ALPN refusal, an unknown control message before the domain, or a reply
neither msg2 nor a known refusal from a relay that answered before. `e4mc_closed`: the relay closed
a working connection. `turn_denied_<reason>`: the service's `turn_denied` with that `reason`;
`turn_creds_error`: its `turn_failed`, another error, or no answer within 4 s. `turn_peer_timeout`:
no `world_peer_ok` within the attempt.

### 12.3 Buckets

- `ms`: 0, 50, 100, 200, 400, 800, 1600, 3200, 6400 (the largest not above the value).
- `min`: 0, 5, 15, 30, 60, 120, 240, 480.
- `rtt…`: 0, 10, 20, 40, 60, 80, 100, 150, 200, 300, 500, 1000.
- `turnRx`, `e4mcRx`, `guests`, `moves`: exact (they are not personal).

### 12.4 Never in telemetry

Names, ids (other than `session` in `u`), IP addresses, ports, domains, world names, slots, keys,
PSKs, TURN usernames or passwords, free text of any kind.

### 12.5 Sending

Events wait at most 60 s and go 20 to a frame (a session's end and every `u` event flush at once).
At most 20 frames a minute. While the presence socket is down events are dropped, except that
`turnRx` of `u` events that could not go is added to the next one. Examples:
`testdata/report/world-report.json`.

## 13. Logging and secrets

- Never in any log, crash report or telemetry: PSKs, the host session private key, the QUIC private
  key, the gate key, the binding key, TURN credentials and usernames, the bridge token, IP addresses
  (also relay addresses), ports of relay addresses, slots.
- e4mc domains appear only as `*.e4mc.link`.
- Allowed in the launcher's local log: session opened or closed, a guest's in-game name and guest id
  in or out, the path kind, step, code and durations.
- `Debug` of every type holding a secret prints `<redacted>` (as `pl_overlay::protocol::Secret`).

## 14. Versioning

- Version 1 is `PLJ1`/`PLG1`, `hello.v = 1`, `join.v = 1`, `manifest.v = 1`. Additions (fields,
  messages, record types safe to skip, `caps`) keep the version. A breaking change takes `PLJ2` (a
  new magic and prologue) and a new `join.v`; a launcher keeps speaking the older version for at
  least two releases.
- The bridge frames for hosting are listed in the OVL2 protocol log (`docs/tasks/ovl2-protocol.md`,
  section "HOST1"; the mod's `bridge/Protocol.java` and `crates/pl-overlay/src/protocol.rs` follow
  it).

## 15. Test vectors

`testdata/README.md` lists the files and how they were made. In short:

| File | What |
| --- | --- |
| `noise/nkpsk0-{control,game,resume}.json` | Full handshakes with fixed keys, openings, every transport frame |
| `noise/nkpsk0-negative.json` | Openings and frames a host must refuse |
| `plj1/records.json` | VarInts, plaintext records, a control message split in two |
| `e4mc/wire.json` | Relay control stream, the guest's first write, relay refusals, domain checks |
| `gate/plg1.json` | Gate key and identity headers, good and bad |
| `binding/*.json` | Binding key id, digests, an example file, admission by name |
| `turn/alias.json`, `turn/pin.json` | TURN labels (service side) and a certificate pin |
| `manifest/*.json` | Manifests with hashes, install matching scenarios |
| `invite/join.json` | An invite and relay address checks |
| `report/world-report.json` | Telemetry frames, good and bad |
| `policy/*.json` | Client numbers, guest and host path scenarios, service order rules, budget arithmetic |
