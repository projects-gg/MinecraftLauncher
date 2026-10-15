//! Mod loaders. The installers (Fabric, Quilt, Forge, NeoForge) live in submodules of this folder;
//! [`LoaderKind`] is shared by the whole crate (instances, mods, Modrinth, modpacks).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl LoaderKind {
    /// Loader name as used by the Modrinth API (`loaders` facet).
    pub fn modrinth_name(self) -> Option<&'static str> {
        match self {
            Self::Vanilla => None,
            Self::Fabric => Some("fabric"),
            Self::Quilt => Some("quilt"),
            Self::Forge => Some("forge"),
            Self::NeoForge => Some("neoforge"),
        }
    }

    /// Key used in the `.mrpack` `dependencies` object.
    pub fn mrpack_dependency_key(self) -> Option<&'static str> {
        match self {
            Self::Vanilla => None,
            Self::Fabric => Some("fabric-loader"),
            Self::Quilt => Some("quilt-loader"),
            Self::Forge => Some("forge"),
            Self::NeoForge => Some("neoforge"),
        }
    }

    pub fn from_mrpack_dependency_key(key: &str) -> Option<Self> {
        match key {
            "fabric-loader" => Some(Self::Fabric),
            "quilt-loader" => Some(Self::Quilt),
            "forge" => Some(Self::Forge),
            "neoforge" => Some(Self::NeoForge),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mrpack_keys_round_trip() {
        for kind in [
            LoaderKind::Fabric,
            LoaderKind::Quilt,
            LoaderKind::Forge,
            LoaderKind::NeoForge,
        ] {
            let key = kind.mrpack_dependency_key().unwrap();
            assert_eq!(LoaderKind::from_mrpack_dependency_key(key), Some(kind));
        }
        assert_eq!(LoaderKind::Vanilla.mrpack_dependency_key(), None);
    }
}
