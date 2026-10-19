//! What the import shows and does, as the UI sees it (camelCase JSON, exported to TypeScript).

use std::path::PathBuf;

use pl_mc::loaders::LoaderKind;
use serde::{Deserialize, Serialize};

/// The launcher a source belongs to. Names are brand names: the UI shows them as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    /// Mojang's Minecraft Launcher (`.minecraft`).
    Minecraft,
    /// TLauncher: `.minecraft` (or the folder its settings name) plus its own settings.
    TLauncher,
    /// SKlauncher: `.minecraft` plus its `sklauncher` folder.
    SkLauncher,
    CurseForge,
    Modrinth,
    Prism,
    PolyMc,
    MultiMc,
}

impl SourceKind {
    /// The launcher's own name ("TLauncher", "CurseForge").
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Minecraft => "Minecraft Launcher",
            Self::TLauncher => "TLauncher",
            Self::SkLauncher => "SKlauncher",
            Self::CurseForge => "CurseForge",
            Self::Modrinth => "Modrinth App",
            Self::Prism => "Prism Launcher",
            Self::PolyMc => "PolyMC",
            Self::MultiMc => "MultiMC",
        }
    }
}

/// What a `.minecraft`-style folder shares between all its versions, counted without opening
/// anything big.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharedData {
    /// `options.txt` is there (language, controls, video).
    pub options: bool,
    /// Servers listed in `servers.dat` (hidden "Direct Connection" entries left out).
    pub servers: u32,
    pub worlds: u32,
    pub resource_packs: u32,
    pub shader_packs: u32,
    pub screenshots: u32,
}

/// Why a profile cannot come over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Unsupported {
    /// Made with a mod loader this launcher does not install (`name`: LiteLoader, Rift,
    /// Legacy Fabric...; empty when the loader is not known at all).
    Loader { name: String },
    /// Its Minecraft version could not be read.
    UnknownVersion,
}

/// A version, profile or instance of another launcher that can become an instance here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportProfile {
    /// Stable within its source (`version:<id>`, `profile:<key>`, `instance:<folder>`).
    pub id: String,
    /// The name it becomes: the player's own (an instance, a launcher profile they named) or
    /// one read from the version ("Fabric 1.21.4", "Minecraft 1.8.9").
    pub name: String,
    pub minecraft: Option<String>,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    /// Set when it cannot come over; the row says why.
    pub unsupported: Option<Unsupported>,
    /// OptiFine was part of it: the instance is made without it (Sodium and Iris do its work).
    pub without_optifine: bool,
    /// It plays in a folder of its own (an instance, a launcher profile with its own game
    /// directory), copied as a whole. Otherwise it shares the source's folder and brings its
    /// mods, mod settings, game settings and server list.
    pub own_folder: bool,
    /// Mod files in its mods folder, before they are checked.
    pub mod_files: u32,
    pub last_used_ms: Option<i64>,
    /// An earlier import made it into this instance, which still exists.
    pub imported_as: Option<String>,
    /// The game folder it plays in.
    #[serde(skip)]
    pub(crate) game_dir: PathBuf,
    /// When it was installed (a `.minecraft` folder's versions): orders the profiles no
    /// launcher says were played.
    #[serde(skip)]
    pub(crate) installed_ms: Option<i64>,
}

impl ImportProfile {
    /// The loader and Minecraft version it runs with, when it can come over.
    pub fn supported(&self) -> bool {
        self.unsupported.is_none() && self.minecraft.is_some()
    }
}

/// One launcher's data on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSource {
    /// Stable between a scan and the import (`minecraft`, `curseforge`, ...).
    pub id: String,
    pub kind: SourceKind,
    /// Its folder (logs and support; the UI does not show paths).
    pub path: String,
    /// Settings, servers, worlds and packs shared by every version (`.minecraft`-style
    /// folders); `None` for launchers that keep everything per instance.
    pub shared: Option<SharedData>,
    /// Most recently used first.
    pub profiles: Vec<ImportProfile>,
    /// A player name the launcher used offline (TLauncher), offered for the account step.
    /// Accounts and their sign-ins never come over.
    pub offline_name: Option<String>,
    pub last_used_ms: Option<i64>,
    #[serde(skip)]
    pub(crate) root: PathBuf,
}

/// Sizes of the shared data, for the rows and the free-space check.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharedSizes {
    pub worlds: u64,
    pub resource_packs: u64,
    pub shader_packs: u64,
    pub screenshots: u64,
}

/// What one profile brings, read on demand.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMeasure {
    pub id: String,
    /// Bytes the copy takes.
    pub bytes: u64,
    /// Mods that come over: made for its loader and Minecraft version.
    pub mods: u32,
    /// Mods left behind: made for another loader or Minecraft version, or broken files.
    pub left_mods: u32,
    /// Worlds of a profile with its own folder.
    pub worlds: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceMeasure {
    pub source_id: String,
    pub shared: Option<SharedSizes>,
    pub profiles: Vec<ProfileMeasure>,
}

/// A kind of shared data.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum ImportShared {
    Options,
    Servers,
    Worlds,
    ResourcePacks,
    ShaderPacks,
    Screenshots,
}

/// What to bring from one source.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceRequest {
    pub source_id: String,
    /// Shared data that goes into the Projects instance.
    #[serde(default)]
    pub shared: Vec<ImportShared>,
    /// Profile ids, each becoming an instance.
    #[serde(default)]
    pub profiles: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub sources: Vec<SourceRequest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ImportStage {
    /// Reading what the picked items take.
    Preparing,
    Copying,
    /// Writing the last records.
    Finishing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub stage: ImportStage,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_items: u32,
    pub total_items: u32,
    /// What is being brought over now (a world's or an instance's name), when it has one.
    pub current: Option<String>,
}

/// One item of a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ItemRef {
    Shared { kind: ImportShared },
    Profile { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeStatus {
    /// Everything came over.
    Done,
    /// Some of it came over; `left` says how much did not, `reason` why.
    Partial,
    /// Nothing was copied, on purpose (already there, the player's own kept, cancelled).
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OutcomeReason {
    /// The disk had no room for it.
    NoSpace,
    /// The other launcher's file could not be read.
    Unreadable,
    /// It came over before.
    AlreadyThere,
    /// The Projects instance has its own game settings; they stay.
    KeptExisting,
    Cancelled,
    /// The copy failed (details in `detail`).
    Failed,
    /// The Projects instance could not be prepared (offline at the first start).
    NoTarget,
    /// Mods made for another loader or Minecraft version stayed behind.
    IncompatibleMods,
    /// The other launcher's item is gone.
    NotFound,
    /// Its loader or version cannot be installed here.
    Unsupported,
    /// A world is open in a running game.
    InUse,
}

/// Where an item's copies went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub instance_id: String,
    pub instance_name: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemOutcome {
    pub source_id: String,
    pub item: ItemRef,
    /// The profile's or instance's name; empty for shared data.
    pub label: String,
    pub status: OutcomeStatus,
    /// Things that came over (worlds, packs, screenshots, servers added, mods).
    pub copied: u32,
    /// Things that stayed behind.
    pub left: u32,
    pub reason: Option<OutcomeReason>,
    /// The instances the copies went into.
    pub destinations: Vec<Destination>,
    /// Technical detail for support ("Ayrıntıları kopyala"); never shown as it is.
    pub detail: Option<String>,
}

impl ItemOutcome {
    pub(crate) fn new(source_id: &str, item: ItemRef, label: impl Into<String>) -> Self {
        Self {
            source_id: source_id.to_owned(),
            item,
            label: label.into(),
            status: OutcomeStatus::Done,
            copied: 0,
            left: 0,
            reason: None,
            destinations: Vec::new(),
            detail: None,
        }
    }

    pub(crate) fn skipped(mut self, reason: OutcomeReason) -> Self {
        self.status = OutcomeStatus::Skipped;
        self.reason = Some(reason);
        self
    }

    pub(crate) fn failed(mut self, reason: OutcomeReason, detail: impl Into<String>) -> Self {
        self.status = OutcomeStatus::Failed;
        self.reason = Some(reason);
        self.detail = Some(detail.into());
        self
    }

    /// Counts one copy into `instance`.
    pub(crate) fn add_destination(&mut self, instance_id: &str, instance_name: &str, count: u32) {
        if count == 0 {
            return;
        }
        match self
            .destinations
            .iter_mut()
            .find(|d| d.instance_id == instance_id)
        {
            Some(found) => found.count += count,
            None => self.destinations.push(Destination {
                instance_id: instance_id.to_owned(),
                instance_name: instance_name.to_owned(),
                count,
            }),
        }
    }

    /// Done, partial or failed from what was copied and left (a reason set earlier stays).
    pub(crate) fn settle(mut self, reason_for_left: OutcomeReason) -> Self {
        if self.status != OutcomeStatus::Done {
            return self;
        }
        if self.left > 0 {
            self.reason.get_or_insert(reason_for_left);
            self.status = if self.copied > 0 {
                OutcomeStatus::Partial
            } else {
                OutcomeStatus::Failed
            };
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreatedInstance {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// In the order they ran.
    pub outcomes: Vec<ItemOutcome>,
    pub cancelled: bool,
    /// Instances made from profiles.
    pub created: Vec<CreatedInstance>,
    /// The Projects instance, when shared data went into it.
    pub official_id: Option<String>,
}
