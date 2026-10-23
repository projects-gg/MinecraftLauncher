//! "Dünyanı arkadaşlarına aç" (HOST1): a player opens their singleplayer world to the friends they
//! invite, and the friends' launchers join it through a relay. The contract is
//! [`PROTOCOL.md`](https://github.com/projects-gg/MinecraftLauncher/blob/v2/crates/pl-host/PROTOCOL.md);
//! its test vectors are in `testdata/`.
//!
//! [`transport`] is the path a stream rides on (Cloudflare TURN or the e4mc relay).

pub mod transport;
