//! Which features players use, counted by the launcher command that does the thing ("Kur" in the
//! Modrinth browser is `content_install`). One central table instead of a call in every command:
//! nothing else in the launcher changes when a feature is counted, and what is counted is in one
//! place. Only actions count, never views or polls, and only how often: never what (no mod, no
//! server, no friend, no message).

/// A counted feature. The order is the wire order: the service keeps one number per feature in
/// this order (a new feature goes at the end, the service learns it first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    /// Mods, resource packs and shaders from Modrinth.
    Modrinth,
    /// A Modrinth modpack installed or imported.
    Modpack,
    /// A ready-made mod set ("Hazır setler").
    Modset,
    /// An install brought over from another launcher (or from v1).
    Import,
    /// A skin or cape changed (Microsoft) or a Projects skin saved.
    Skin,
    /// A local AFK account connected.
    Afk,
    /// A 7/24 Bulut account started.
    CloudAfk,
    /// Friends: a request sent or accepted, a sign-in.
    Friends,
    /// A message, sticker or reply sent.
    Chat,
    /// A crew made or joined.
    Crew,
    /// A world opened to friends, or a friend's world joined.
    Worlds,
    /// A voice call started or accepted, a crew room joined.
    Voice,
    /// Mod repair, reinstall or the troubleshooter.
    Repair,
    /// A support bundle made or a log shared.
    Support,
    /// Graphics presets and shaders.
    Graphics,
    /// An install switched to another Minecraft version.
    Versions,
    /// A world backed up or restored.
    Backup,
    /// Space freed from Settings → Veri → Temizlik.
    Cleanup,
}

impl Feature {
    /// Every feature, in the wire order.
    pub const ALL: [Self; 18] = [
        Self::Modrinth,
        Self::Modpack,
        Self::Modset,
        Self::Import,
        Self::Skin,
        Self::Afk,
        Self::CloudAfk,
        Self::Friends,
        Self::Chat,
        Self::Crew,
        Self::Worlds,
        Self::Voice,
        Self::Repair,
        Self::Support,
        Self::Graphics,
        Self::Versions,
        Self::Backup,
        Self::Cleanup,
    ];

    /// The wire name.
    pub fn key(self) -> &'static str {
        match self {
            Self::Modrinth => "modrinth",
            Self::Modpack => "modpack",
            Self::Modset => "modset",
            Self::Import => "import",
            Self::Skin => "skin",
            Self::Afk => "afk",
            Self::CloudAfk => "cloudAfk",
            Self::Friends => "friends",
            Self::Chat => "chat",
            Self::Crew => "crew",
            Self::Worlds => "worlds",
            Self::Voice => "voice",
            Self::Repair => "repair",
            Self::Support => "support",
            Self::Graphics => "graphics",
            Self::Versions => "versions",
            Self::Backup => "backup",
            Self::Cleanup => "cleanup",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|feature| feature.key() == key)
    }

    /// The feature a launcher command counts for; `None` for every other command.
    pub fn of_command(command: &str) -> Option<Self> {
        COMMANDS
            .iter()
            .find(|(name, _)| *name == command)
            .map(|(_, feature)| *feature)
    }
}

/// The launcher commands that count (their names as the page invokes them).
pub const COMMANDS: &[(&str, Feature)] = &[
    ("content_install", Feature::Modrinth),
    ("content_update", Feature::Modrinth),
    ("modpack_install", Feature::Modpack),
    ("modpack_import", Feature::Modpack),
    ("mod_set_apply", Feature::Modset),
    ("instance_create_from_set", Feature::Modset),
    ("import_start", Feature::Import),
    ("legacy_import", Feature::Import),
    ("skin_upload", Feature::Skin),
    ("skin_set_cape", Feature::Skin),
    ("skin_history_wear", Feature::Skin),
    ("projects_skin_save", Feature::Skin),
    ("afk_connect", Feature::Afk),
    ("afk_connect_all", Feature::Afk),
    ("cloud_afk_start", Feature::CloudAfk),
    ("cloud_afk_restart", Feature::CloudAfk),
    ("social_sign_in", Feature::Friends),
    ("social_send_request", Feature::Friends),
    ("social_accept", Feature::Friends),
    ("social_chat_send", Feature::Chat),
    ("social_chat_sticker", Feature::Chat),
    ("social_chat_reply", Feature::Chat),
    ("social_team_create", Feature::Crew),
    ("social_team_join", Feature::Crew),
    ("social_team_join_link", Feature::Crew),
    ("social_team_accept", Feature::Crew),
    ("host_open", Feature::Worlds),
    ("host_join", Feature::Worlds),
    ("voice_call", Feature::Voice),
    ("voice_accept", Feature::Voice),
    ("voice_join_crew", Feature::Voice),
    ("mods_repair", Feature::Repair),
    ("mods_reinstall", Feature::Repair),
    ("troubleshoot_run", Feature::Repair),
    ("data_support_bundle", Feature::Support),
    ("game_log_share", Feature::Support),
    ("instance_set_graphics", Feature::Graphics),
    ("instance_switch_graphics", Feature::Graphics),
    ("instance_set_shader", Feature::Graphics),
    ("instance_switch_version", Feature::Versions),
    ("world_backup", Feature::Backup),
    ("world_restore", Feature::Backup),
    ("cleanup_remove_sets", Feature::Cleanup),
    ("cleanup_delete_shared", Feature::Cleanup),
    ("cleanup_clean_leftovers", Feature::Cleanup),
    ("cleanup_empty_trash", Feature::Cleanup),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_are_unique() {
        for feature in Feature::ALL {
            assert_eq!(Feature::from_key(feature.key()), Some(feature));
        }
        let mut keys: Vec<_> = Feature::ALL.iter().map(|f| f.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Feature::ALL.len());
        // The service keeps at most 20 numbers per point.
        assert!(Feature::ALL.len() <= 20);
    }

    /// The service keeps one number per feature in this order (`services/studio-team`'s
    /// `FEATURES`, its README "Telemetry"): a new feature goes at the end, there first.
    #[test]
    fn the_wire_order_is_the_services() {
        let keys: Vec<_> = Feature::ALL.iter().map(|f| f.key()).collect();
        assert_eq!(
            keys,
            [
                "modrinth", "modpack", "modset", "import", "skin", "afk", "cloudAfk", "friends",
                "chat", "crew", "worlds", "voice", "repair", "support", "graphics", "versions",
                "backup", "cleanup",
            ]
        );
    }

    #[test]
    fn every_feature_has_a_command_and_commands_are_unique() {
        for feature in Feature::ALL {
            assert!(
                COMMANDS.iter().any(|(_, f)| *f == feature),
                "{feature:?} has no command"
            );
        }
        let mut names: Vec<_> = COMMANDS.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), COMMANDS.len());
        assert_eq!(Feature::of_command("voice_call"), Some(Feature::Voice));
        assert_eq!(Feature::of_command("voice_state"), None);
    }

    /// A renamed command would stop counting without a word: every name must still be one the
    /// launcher's page can invoke (its generated bindings).
    #[test]
    fn every_command_exists_in_the_launcher() {
        let bindings = include_str!("../../../apps/launcher/src/bindings.ts");
        let invokes: Vec<&str> = bindings
            .lines()
            .filter(|line| line.contains("__TAURI_INVOKE"))
            .collect();
        for (name, _) in COMMANDS {
            // `__TAURI_INVOKE("name", …)` or, without a result, `__TAURI_INVOKE<void>("name")`.
            let call = format!("(\"{name}\"");
            assert!(
                invokes.iter().any(|line| line.contains(&call)),
                "the launcher has no command {name}"
            );
        }
    }
}
