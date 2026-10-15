//! The instance model stored in `instances/<id>/instance.json`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::java::JavaSetting;
use crate::launch::{LaunchOptions, QuickPlay, WindowOptions};
use crate::loaders::LoaderKind;

/// Current `instance.json` schema version.
pub const INSTANCE_SCHEMA_VERSION: u32 = 1;

fn current_schema() -> u32 {
    INSTANCE_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InstanceIcon {
    /// Icon bundled with the launcher, by key (`grass`, `projects`, ...).
    Builtin { key: String },
    /// Image file inside the instance folder (path relative to it, e.g. `icon.png`).
    File { path: String },
}

impl Default for InstanceIcon {
    fn default() -> Self {
        Self::Builtin {
            key: "grass".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceLoader {
    pub kind: LoaderKind,
    /// Loader version (`0.16.5`, `47.2.0`...); `None` for vanilla.
    #[serde(default)]
    pub version: Option<String>,
}

impl Default for InstanceLoader {
    fn default() -> Self {
        Self {
            kind: LoaderKind::Vanilla,
            version: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MemorySettings {
    pub min_mb: u32,
    pub max_mb: u32,
}

impl Default for MemorySettings {
    fn default() -> Self {
        Self {
            min_mb: 1024,
            max_mb: 2048,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WindowSettings {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub fullscreen: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerAddress {
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
}

/// Commands run around the game (from the instance settings).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Hooks {
    #[serde(default)]
    pub pre_launch: Option<String>,
    #[serde(default)]
    pub post_exit: Option<String>,
    /// Wrapper command (`gamemoderun`, `prime-run`, ...).
    #[serde(default)]
    pub wrapper: Option<String>,
}

/// Link to a modpack managed by the launcher (official "Projects" pack, Modrinth pack...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManagedPack {
    pub pack_id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    #[serde(default = "current_schema")]
    pub schema_version: u32,
    /// Folder name under `instances/` (slug of the name at creation).
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: InstanceIcon,
    #[serde(default)]
    pub group: Option<String>,
    pub mc_version: String,
    #[serde(default)]
    pub loader: InstanceLoader,
    #[serde(default)]
    pub java: JavaSetting,
    #[serde(default)]
    pub memory: MemorySettings,
    /// Extra JVM arguments as typed by the user.
    #[serde(default)]
    pub jvm_args: String,
    #[serde(default)]
    pub window: WindowSettings,
    /// Server to join right after start.
    #[serde(default)]
    pub auto_join: Option<ServerAddress>,
    #[serde(default)]
    pub hooks: Hooks,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub managed_pack: Option<ManagedPack>,
    pub created_at: jiff::Timestamp,
    #[serde(default)]
    pub last_played_at: Option<jiff::Timestamp>,
    #[serde(default)]
    pub play_time_secs: u64,
}

impl Instance {
    /// A fresh instance with default settings.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        mc_version: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: INSTANCE_SCHEMA_VERSION,
            id: id.into(),
            name: name.into(),
            icon: InstanceIcon::default(),
            group: None,
            mc_version: mc_version.into(),
            loader: InstanceLoader::default(),
            java: JavaSetting::Auto,
            memory: MemorySettings::default(),
            jvm_args: String::new(),
            window: WindowSettings::default(),
            auto_join: None,
            hooks: Hooks::default(),
            env: BTreeMap::new(),
            notes: String::new(),
            managed_pack: None,
            created_at: jiff::Timestamp::now(),
            last_played_at: None,
            play_time_secs: 0,
        }
    }

    pub fn is_modded(&self) -> bool {
        self.loader.kind != LoaderKind::Vanilla
    }

    /// Launch options from the instance settings (memory, JVM arguments, window, auto-join,
    /// environment, wrapper).
    pub fn to_launch_options(
        &self,
        java_path: impl Into<PathBuf>,
        game_dir: impl Into<PathBuf>,
    ) -> LaunchOptions {
        let mut options = LaunchOptions::new(java_path, game_dir);
        options.min_memory_mb = Some(self.memory.min_mb);
        options.max_memory_mb = Some(self.memory.max_mb);
        options.jvm_args = self.jvm_args.clone();
        options.window = WindowOptions {
            width: self.window.width,
            height: self.window.height,
            fullscreen: self.window.fullscreen,
        };
        options.quick_play = self
            .auto_join
            .as_ref()
            .filter(|server| !server.host.trim().is_empty())
            .map(|server| QuickPlay::Multiplayer {
                host: server.host.trim().to_owned(),
                port: server.port,
            });
        options.env = self.env.clone();
        options.wrapper = self.hooks.wrapper.clone().filter(|w| !w.trim().is_empty());
        options
    }
}

/// Input for `InstanceStore::create`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NewInstance {
    pub name: String,
    pub mc_version: String,
    #[serde(default)]
    pub loader: InstanceLoader,
    #[serde(default)]
    pub icon: Option<InstanceIcon>,
    #[serde(default)]
    pub group: Option<String>,
    /// Defaults to the recommendation for this machine.
    #[serde(default)]
    pub memory: Option<MemorySettings>,
    #[serde(default)]
    pub managed_pack: Option<ManagedPack>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip_and_defaults() {
        let mut instance = Instance::new("my-world", "My World", "1.21.1");
        instance.loader = InstanceLoader {
            kind: LoaderKind::Fabric,
            version: Some("0.16.5".into()),
        };
        instance.auto_join = Some(ServerAddress {
            host: "play.projects.gg".into(),
            port: None,
        });
        let json = serde_json::to_string(&instance).unwrap();
        assert!(json.contains("\"mcVersion\":\"1.21.1\""));
        assert!(json.contains("\"playTimeSecs\":0"));
        assert!(json.contains("\"kind\":\"fabric\""));
        let back: Instance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, instance);

        // Older/minimal files still load.
        let minimal: Instance = serde_json::from_str(
            r#"{"id": "x", "name": "X", "mcVersion": "1.8.9", "createdAt": "2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(minimal.schema_version, INSTANCE_SCHEMA_VERSION);
        assert_eq!(minimal.java, JavaSetting::Auto);
        assert_eq!(minimal.loader.kind, LoaderKind::Vanilla);
    }

    #[test]
    fn launch_options_from_settings() {
        let mut instance = Instance::new("a", "A", "1.20.1");
        instance.memory = MemorySettings {
            min_mb: 1024,
            max_mb: 6144,
        };
        instance.jvm_args = "-XX:+UseG1GC".into();
        instance.window = WindowSettings {
            width: Some(1280),
            height: Some(720),
            fullscreen: false,
        };
        instance.auto_join = Some(ServerAddress {
            host: " play.projects.gg ".into(),
            port: Some(25566),
        });
        instance.hooks.wrapper = Some("  ".into());
        let options = instance.to_launch_options("/java", "/game");
        assert_eq!(options.max_memory_mb, Some(6144));
        assert_eq!(options.window.width, Some(1280));
        assert_eq!(
            options.quick_play,
            Some(QuickPlay::Multiplayer {
                host: "play.projects.gg".into(),
                port: Some(25566)
            })
        );
        assert_eq!(options.wrapper, None);
    }
}
