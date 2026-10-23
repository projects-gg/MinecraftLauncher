//! Local stand-ins for the relays, reached over loopback only (no network beyond it): a TURN server,
//! a QUIClime relay and its broker, test certificates and a log capture. Each test binary uses part
//! of it.

#![allow(dead_code)]

pub mod certs;
pub mod logs;
pub mod quiclime;
pub mod turn_server;

use std::time::Duration;

/// Waits for `future` at most `seconds`, failing the test otherwise.
pub async fn within<F: std::future::IntoFuture>(seconds: u64, future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(seconds), future)
        .await
        .expect("timed out")
}

/// The test vectors in `testdata/`.
pub fn vectors(path: &str) -> serde_json::Value {
    let text =
        std::fs::read_to_string(format!("{}/testdata/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    serde_json::from_str(&text).unwrap()
}

pub fn hex_field(value: &serde_json::Value) -> Vec<u8> {
    hex::decode(value.as_str().unwrap()).unwrap()
}
