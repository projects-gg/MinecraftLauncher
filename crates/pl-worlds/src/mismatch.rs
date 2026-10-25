//! A friend's world that turned the game away for mods it lacks, as the game's log says it
//! (PROTOCOL.md §10.2 step 7; host1.md §5.4). Fabric API's registry sync compares the host's
//! registries with the game's while it joins: entries the game does not know (a block, an item,
//! an entity of a mod only the host has) end the connection with "Received 3 registry entries
//! that are unknown to this client.", after the error lines below. The texts are Fabric API's own
//! (`fabric-registry-sync-v0`: `RegistrySyncManager`, `FabricRegistryClientInit` and its
//! `en_us.json`; the game shows them in English in every language, the module has no other).
//!
//! A protocol error alone ("Network Protocol Error") is not read as one: the game words it in the
//! player's language, and a broken connection says it too.

/// Fabric API's lines when the host's registries hold entries this game lacks.
const MARKS: [&str; 5] = [
    // `RegistrySyncManager.checkRemoteRemap`, then one line per entry.
    "Received unknown remote registry entries from server",
    "is missing from local registry",
    // `FabricRegistryClientInit`, when the sync failed.
    "Registry remapping failed",
    // The disconnect reason the game logs ("Client disconnected with reason: …").
    "that are unknown to this client",
    "that is unknown to this client",
];

/// Whether this line of the game's log says the world's mods differ from the game's.
pub fn is_mod_mismatch(line: &str) -> bool {
    MARKS.iter().any(|mark| line.contains(mark))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabric_registry_sync_lines_are_a_mismatch() {
        for line in [
            "Received unknown remote registry entries from server",
            "Registry entry (farmersdelight:tomato) is missing from local registry (minecraft:item)",
            "Registry remapping failed: net.fabricmc.fabric.impl.registry.sync.RemapException: \
             Received 3 registry entries that are unknown to this client.",
            "Registry remapping failed!",
            "Client disconnected with reason: Received 3 registry entries that are unknown to \
             this client.",
            "Client disconnected with reason: Received a registry entry that is unknown to this \
             client.",
        ] {
            assert!(is_mod_mismatch(line), "{line}");
        }
    }

    #[test]
    fn other_lines_are_not() {
        for line in [
            "",
            "Connecting to 127.0.0.1, 51234",
            "Client disconnected with reason: Network Protocol Error",
            "Client disconnected with reason: Bağlantı kesildi",
            "Client disconnected with reason: Dünya dolu.",
            "Registry entry (minecraft:stone) was changed",
            "An ID for {} was not sent by the server, assuming client only registry entry",
            "Syncing registry: minecraft:item",
            "[CHAT] Ayşe joined the game",
        ] {
            assert!(!is_mod_mismatch(line), "{line}");
        }
    }
}
