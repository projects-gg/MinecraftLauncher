//! The end-to-end rig's overrides (HOST1-E), in debug builds only: `lib.rs` declares this module
//! under `#[cfg(debug_assertions)]`, so a release build has none of it (and the launcher cannot call
//! it outside its own debug-only code); the test at the end holds that in place.
//!
//! With them, test launchers share worlds over a local e4mc relay (QUIClime) and a local TURN server
//! and never reach the real ones:
//! - `PROJECTS_E4MC_BROKER`: the broker's address (`http://` allowed);
//! - `PROJECTS_E4MC_RELAYS`: the relays tried without it (comma-separated);
//! - `PROJECTS_E4MC_RELAY_PORT`: their QUIC port; `PROJECTS_E4MC_PLAYER_PORT`: the relay's port for
//!   guests;
//! - `PROJECTS_HOST_RIG_IP`: where every relay name resolves (the e4mc relays and the TURN servers);
//! - `PROJECTS_HOST_TEST_CA`: a PEM file with the only roots the relays' TLS may chain to;
//! - `PROJECTS_HOST_GUEST_ANY=1`: any program on this computer may come in through a join's local
//!   endpoint (the rig's light guest, an AFK client, stands in for the game).
//!
//! The TURN servers themselves come from the (local) friends service as always.

use std::net::IpAddr;

use pl_host::transport::net::{Resolver, Trust};

use crate::Transports;

pub const BROKER: &str = "PROJECTS_E4MC_BROKER";
pub const RELAYS: &str = "PROJECTS_E4MC_RELAYS";
pub const RELAY_PORT: &str = "PROJECTS_E4MC_RELAY_PORT";
pub const PLAYER_PORT: &str = "PROJECTS_E4MC_PLAYER_PORT";
pub const RIG_IP: &str = "PROJECTS_HOST_RIG_IP";
pub const TEST_CA: &str = "PROJECTS_HOST_TEST_CA";
pub const GUEST_ANY: &str = "PROJECTS_HOST_GUEST_ANY";

/// Every variable the rig reads.
pub const VARIABLES: [&str; 7] = [
    BROKER,
    RELAYS,
    RELAY_PORT,
    PLAYER_PORT,
    RIG_IP,
    TEST_CA,
    GUEST_ANY,
];

/// The rig's relays in `transports`, from the variables `var` reads (an empty value counts as
/// unset). True when the paths now lead to the rig.
pub fn apply(transports: &mut Transports, var: impl Fn(&str) -> Option<String>) -> bool {
    let var = |name: &str| var(name).filter(|value| !value.trim().is_empty());
    let mut rig = false;
    if let Some(url) = var(BROKER) {
        transports.e4mc.broker_url = url.trim().to_owned();
        rig = true;
    }
    if let Some(relays) = var(RELAYS) {
        transports.e4mc.fallback_relays = relays
            .split(',')
            .map(str::trim)
            .filter(|relay| !relay.is_empty())
            .map(str::to_owned)
            .collect();
        rig = true;
    }
    if let Some(port) = var(RELAY_PORT).and_then(|port| port.trim().parse().ok()) {
        transports.e4mc.relay_port = port;
    }
    if let Some(port) = var(PLAYER_PORT).and_then(|port| port.trim().parse().ok()) {
        transports.e4mc.player_port = port;
    }
    if let Some(ip) = var(RIG_IP).and_then(|ip| ip.trim().parse::<IpAddr>().ok()) {
        transports.e4mc.resolver = Resolver::fixed(ip);
        transports.turn.resolver = Resolver::fixed(ip);
        rig = true;
    }
    if let Some(file) = var(TEST_CA) {
        match std::fs::read(file.trim())
            .ok()
            .and_then(|pem| Trust::from_pem(&pem))
        {
            Some(trust) => {
                transports.e4mc.trust = trust.clone();
                transports.turn.trust = trust;
                rig = true;
            }
            None => tracing::warn!("the rig's test CA file holds no certificate"),
        }
    }
    rig
}

/// [`apply`] with the process's environment.
pub fn apply_env(transports: &mut Transports) -> bool {
    let rig = apply(transports, env_var);
    if rig {
        tracing::info!("world sharing uses the local test rig's relays (debug build)");
    }
    rig
}

/// `PROJECTS_HOST_GUEST_ANY=1`: a join's local endpoint lets any program in.
pub fn any_guest(var: impl Fn(&str) -> Option<String>) -> bool {
    var(GUEST_ANY).is_some_and(|value| value.trim() == "1")
}

/// [`any_guest`] with the process's environment.
pub fn any_guest_env() -> bool {
    any_guest(env_var)
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn without_the_variables_the_paths_are_the_real_ones() {
        let mut transports = Transports::new("2.0.0");
        assert!(!apply(&mut transports, vars(&[])));
        assert!(!apply(
            &mut transports,
            vars(&[(BROKER, " "), (RIG_IP, "")])
        ));
        let real = Transports::new("2.0.0");
        assert_eq!(transports.e4mc.broker_url, real.e4mc.broker_url);
        assert_eq!(
            transports.e4mc.broker_url,
            "https://broker.e4mc.link/getBestRelay"
        );
        assert_eq!(transports.e4mc.fallback_relays, real.e4mc.fallback_relays);
        assert_eq!(transports.e4mc.relay_port, 25575);
        assert_eq!(transports.e4mc.player_port, 25565);
        assert!(matches!(transports.e4mc.trust, Trust::Platform));
        assert!(matches!(transports.turn.trust, Trust::Platform));
        assert!(!any_guest(vars(&[])));
        assert!(!any_guest(vars(&[(GUEST_ANY, "0")])));
        assert!(!any_guest(vars(&[(GUEST_ANY, "true")])));
    }

    #[test]
    fn the_variables_point_every_path_at_the_rig() {
        let dir = tempfile::tempdir().unwrap();
        let ca = dir.path().join("ca.pem");
        // The PEM armour around three bytes is enough here (pl-host's own test parses real ones).
        std::fs::write(
            &ca,
            "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        let ca = ca.to_string_lossy().into_owned();
        let mut transports = Transports::new("2.0.0");
        assert!(apply(
            &mut transports,
            vars(&[
                (BROKER, "http://127.0.0.1:20246/getBestRelay"),
                (RELAYS, "loc.e4mc.link, ,other.e4mc.link"),
                (RELAY_PORT, "25404"),
                (PLAYER_PORT, " 27151 "),
                (RIG_IP, "127.0.0.1"),
                (TEST_CA, &ca),
                (GUEST_ANY, "1"),
            ])
        ));
        assert_eq!(
            transports.e4mc.broker_url,
            "http://127.0.0.1:20246/getBestRelay"
        );
        assert_eq!(
            transports.e4mc.fallback_relays,
            ["loc.e4mc.link", "other.e4mc.link"]
        );
        assert_eq!(transports.e4mc.relay_port, 25404);
        assert_eq!(transports.e4mc.player_port, 27151);
        for trust in [&transports.e4mc.trust, &transports.turn.trust] {
            let Trust::Roots(roots) = trust else {
                panic!("only the rig's roots");
            };
            assert_eq!(roots.len(), 1);
            assert_eq!(&roots[0][..], [1u8, 2, 3]);
        }
        assert!(any_guest(vars(&[(GUEST_ANY, " 1 ")])));

        // A file without a certificate leaves the platform's trust in place.
        std::fs::write(dir.path().join("empty.pem"), "nothing").unwrap();
        let empty = dir.path().join("empty.pem").to_string_lossy().into_owned();
        let mut transports = Transports::new("2.0.0");
        assert!(!apply(&mut transports, vars(&[(TEST_CA, &empty)])));
        assert!(matches!(transports.e4mc.trust, Trust::Platform));
    }

    /// The repository's root (this crate sits in `crates/pl-worlds`).
    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if !path.ends_with("target") && !path.ends_with("node_modules") {
                    rust_files(&path, out);
                }
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

    /// The attribute line right above `item` in `source` (doc comments skipped).
    fn gate_of(source: &str, item: &str) -> String {
        let lines: Vec<&str> = source.lines().collect();
        let at = lines
            .iter()
            .position(|line| line.trim_start().starts_with(item))
            .unwrap_or_else(|| panic!("{item} is gone"));
        lines[..at]
            .iter()
            .rev()
            .map(|line| line.trim())
            .find(|line| !line.starts_with("///"))
            .unwrap_or_default()
            .to_owned()
    }

    /// The rig cannot reach a release build: this module and pl-host's two helpers are compiled
    /// into debug builds only, and no other code of the launcher names the rig's variables.
    #[test]
    fn the_rig_exists_in_debug_builds_only() {
        let lib = include_str!("lib.rs");
        assert_eq!(gate_of(lib, "pub mod rig;"), "#[cfg(debug_assertions)]");
        let net = include_str!("../../pl-host/src/transport/net.rs");
        for item in ["pub fn fixed(", "pub fn from_pem("] {
            assert_eq!(gate_of(net, item), "#[cfg(debug_assertions)]", "{item}");
        }
        let mut files = Vec::new();
        rust_files(&root().join("crates"), &mut files);
        rust_files(&root().join("apps/launcher/src-tauri/src"), &mut files);
        assert!(
            files.len() > 50,
            "the launcher's sources are where they were"
        );
        let this = Path::new("pl-worlds").join("src").join("rig.rs");
        for file in files.iter().filter(|file| !file.ends_with(&this)) {
            let text = std::fs::read_to_string(file).unwrap();
            for name in VARIABLES {
                assert!(!text.contains(name), "{} names {name}", file.display());
            }
        }
        // The launcher's own hooks call this module, which exists in debug builds only.
        let host =
            std::fs::read_to_string(root().join("apps/launcher/src-tauri/src/host.rs")).unwrap();
        assert!(host.contains("pl_worlds::rig::apply_env("));
        assert!(host.contains("pl_worlds::rig::any_guest_env()"));
    }
}
