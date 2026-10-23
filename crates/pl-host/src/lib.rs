//! "Dünyanı arkadaşlarına aç" (HOST1): a player opens their singleplayer world to the friends they
//! invite, and the friends' launchers join it through a relay. The contract is `PROTOCOL.md` next to
//! this crate's manifest; its test vectors are in `testdata/`.
//!
//! - [`tunnel`]: PLJ1, the Noise tunnel every stream rides in, with resumable game streams.
//! - [`host`]: the host's side: admission, the PLG1 identity header, name bindings, the session.
//! - [`guest`]: the guest's side: joining through the service's `order`, the local endpoint the
//!   game connects to, moving streams between paths.
//! - [`manifest`]: the world manifest and install matching.
//! - [`policy`]: the client's timers and limits, and the path state machines.
//! - [`report`]: anonymous health telemetry.
//! - [`transport`]: the paths a stream rides on (Cloudflare TURN or the e4mc relay), behind one
//!   interface the tunnel uses without knowing which relay it is.
//!
//! Nothing here logs an address, a port, a domain, a slot, a key or a credential.

pub mod guest;
pub mod host;
pub mod manifest;
pub mod policy;
pub mod report;
pub mod transport;
pub mod tunnel;

#[cfg(test)]
pub(crate) mod testdata {
    //! Reading `testdata/`.

    use std::path::PathBuf;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join(name)
    }

    pub fn bytes(name: &str) -> Vec<u8> {
        std::fs::read(path(name)).unwrap_or_else(|e| panic!("testdata/{name}: {e}"))
    }

    pub fn json(name: &str) -> serde_json::Value {
        serde_json::from_slice(&bytes(name)).unwrap_or_else(|e| panic!("testdata/{name}: {e}"))
    }
}
