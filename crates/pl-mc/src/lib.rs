//! Minecraft core for Projects Launcher.
//!
//! Layout (each area owns its folder; cross-area use goes through the public items):
//! - [`paths`], [`platform`]: data directory layout and OS/architecture detection.
//! - [`version`]: Mojang version manifest, version JSON model, rules, `inheritsFrom` merging, arguments.
//! - [`install`]: libraries, natives, assets, client jar and logging config installation.
//! - [`java`]: Mojang Java runtimes and detection of Java installed on the system.
//! - [`launch`]: launch identity, command building, process supervision, log parsing, crash analysis.
//! - [`instance`]: instances (MultiMC-style isolated game directories) and their settings.
//! - [`loaders`]: Fabric, Quilt, Forge and NeoForge installation.
//! - [`mods`]: reading mod metadata from jars, enabling/disabling mods.
//! - [`modrinth`]: Modrinth API client.
//! - [`modpack`]: `.mrpack` read/write and Prism/MultiMC/CurseForge import.
//! - [`gamefiles`]: worlds, resource packs, shader packs and screenshots of a game directory.
//! - [`server`]: Server List Ping and `servers.dat`.
//! - [`addons`]: mods, resource packs and shader packs installed from Modrinth (index, updates).
//! - [`mask`]: the one place secrets are masked in text that is shown, copied or shared.
//! - [`shared_files`]: which versions, loader profiles, Java runtimes, libraries and assets no
//!   instance uses any more (cleanup in Settings → Veri).

pub mod addons;
pub mod error;
pub mod gamefiles;
pub mod install;
pub mod instance;
pub mod java;
pub mod launch;
pub mod loaders;
pub mod mask;
pub mod modpack;
pub mod modrinth;
pub mod mods;
pub mod paths;
pub mod platform;
pub mod server;
pub mod shared_files;
pub mod version;

pub use error::{Error, Result};
