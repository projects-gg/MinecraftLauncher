# pl-host test vectors

Data for the tests of `pl-host` (HOST1-R1, R2), the overlay mod's gate (HOST1-M) and the friends
service's world and TURN code (HOST1-S, through the `launcher/` submodule). The rules they check are
in [`../PROTOCOL.md`](../PROTOCOL.md); a test that disagrees with a file here is a bug in the test or
the code, never a reason to edit the file by hand.

## How they were made

HOST1-C generated every file on 23 October 2026 with a throwaway Python tool outside the repository:

- Python 3.14.2, `noiseprotocol` 0.3.1 (Noise), `cryptography` 50.0.2 (X25519 public keys, the
  P-256 certificate), the standard library for SHA-256, HMAC, base64 and JSON.
- Every input comes from a label: `seed(label, n) = SHA-256("pl-host test vector: " + label)[0..n]`
  (for example the host session key is `seed("host session static key")`, the PSK
  `seed("invite psk")`, the slot `seed("invite slot", 16)`, the session id `seed("session id", 16)`,
  the ephemeral keys `seed("<file>: guest ephemeral")` and `seed("<file>: host ephemeral")`). The
  files hold the derived bytes too, so nobody needs the tool to use them.
- The Noise files were checked independently with Rust `snow` 0.10.0
  (`Builder::fixed_ephemeral_key_for_testing_only`): every handshake message, handshake hash and
  transport ciphertext matched, and every negative case was refused. The gate key and headers, the
  binding digests, the aliases, the manifest hashes, the VarInts, the e4mc handshake packet and the
  certificate pin were checked again with Node.js 24's `crypto`.
- `turn/pin.json` holds a random certificate; regenerating gives another one (only the rule matters).
- JSON files are written with two-space indentation and LF line ends. Biome leaves this folder alone
  (`biome.json`), as it does other generated vectors.

Protocol numbers in these files (`9999`) are illustrative; they are no real Minecraft version's.

## Files

| File | Contents | Used by |
| --- | --- | --- |
| `noise/nkpsk0-control.json` | Fixed keys, prologue, msg1, msg2, the opening and reply bytes, handshake hash, and 8 transport frames of a control stream (`hello`, `ok`, `manifest`, `state`, ping/pong, `move`, `bye`) with nonces, plaintext records and ciphertexts | R1, phone (phase 2) |
| `noise/nkpsk0-game.json` | A game stream: `hello{game}`, `ok`, the game's first bytes as `data`, acks, `close` | R1 |
| `noise/nkpsk0-resume.json` | A resume: both `recv` values and the resent `data` | R1 |
| `noise/nkpsk0-negative.json` | Wrong PSK, tampered msg1, wrong session in the prologue, unknown slot, bad length, bad magic, tampered frame | R1 |
| `plj1/records.json` | VarInt encodings, plaintext records of every type, a manifest split into `ctrl-part` + `ctrl` | R1 |
| `e4mc/wire.json` | QUIClime control stream bytes, the guest's first write (handshake packet + opening), the relay's two refusal packets, domain checks | R2 |
| `gate/plg1.json` | Bridge token → gate key, identity headers (good and bad) | R1, M |
| `binding/digest.json` | Binding key id and friend digests | R1, M |
| `binding/projects-guests.example.json` | A name binding file | M |
| `binding/rules.json` | Name admission cases against that file | R1, M |
| `turn/alias.json` | Monthly aliases and TURN `customIdentifier` labels | S |
| `turn/pin.json` | A QUIC certificate and its pin | R2 |
| `manifest/examples.json` | Manifests and their `hash`, with one canonical string spelled out | R1 |
| `manifest/match.json` | Install matching: host manifest + guest installs → action | R1, L1 |
| `invite/join.json` | An invite's `join`, its `world_invite` frame, relay address checks | R1, L1, S |
| `report/world-report.json` | Telemetry frames the service keeps and drops | R1, S |
| `policy/client-policy.json` | Every client timer and limit; `pl-host`'s `Policy::default()` must equal it | R1, R2 |
| `policy/guest-connect.json` | A guest working through `order` (messages it sends, events, result) | R1 |
| `policy/host-open.json` | A host preparing its first path | R1 |
| `policy/service-order.json` | The service's answer to `turn_creds` for given budget, health and limits | S |
| `policy/budget.json` | Day share arithmetic and the safe-side rule | S |

## Conventions inside the files

- `…Hex`: lowercase hex bytes; `…B64u`: base64url without padding.
- In `noise/*.json`, `transport[]` is in send order; `nonce` counts per direction from 0; `frameHex` is
  `u16 length || ciphertext`; `record` describes the plaintext.
- In `policy/guest-connect.json` and `host-open.json`, `script` lists `step:outcome` in the order the
  steps happen (`plj:no:full` is a PLJ1 refusal); `messages` lists presence frames as `t/reason` or
  `t/path`; `events` lists attempt events as `[path, step, code]`.
